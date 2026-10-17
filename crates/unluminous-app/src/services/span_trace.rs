//! What egui, eframe and egui-wgpu spent a frame on, in the frame trace (`task-2288`).
//!
//! The frame trace names every phase of `UnluminousApp::ui`, and everything after it is one number,
//! `egui-cpu`: egui's own layout, the tessellation, the texture uploads, the render pass and whatever
//! egui-wgpu does when a screenshot comes back. Opening Settings measured that number at 96 ms on one frame,
//! with nothing in the trace able to say which of those it was.
//!
//! Those crates already mark their work with `profiling::scope!`. A build made with `diagnostic-spans` turns
//! the marks into `tracing` spans, and this is the smallest subscriber that times them: every span that took
//! longer than [`SLOWEST_KEPT_MS`] is written onto the next frame line as `~<name> <ms>`, through
//! [`frame_trace::value`](crate::services::frame_trace::value), so the reader adds it up with the rest. A
//! normal build compiles the marks to nothing and has none of this.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use tracing_core::field::{Field, Visit};
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{Event, Metadata, Subscriber};

/// A span shorter than this is not written down: there are hundreds a frame and nearly all are nothing.
pub const SLOWEST_KEPT_MS: f64 = 0.5;

/// Start timing the marks, if the frame trace is on. Called once, before the window exists.
pub fn start() {
    if std::env::var_os("UNLUMINOUS_FRAME_TRACE").is_none() {
        return;
    }
    let _ =
        tracing_core::dispatcher::set_global_default(tracing_core::Dispatch::new(Spans::default()));
}

/// Every span's name, by id, and the names already turned into `&'static str` for the frame trace.
#[derive(Default)]
struct Spans {
    next: AtomicU64,
    names: Mutex<HashMap<u64, &'static str>>,
    interned: Mutex<HashMap<String, &'static str>>,
}

thread_local! {
    /// The spans this thread is inside, with when each was entered.
    static OPEN: RefCell<Vec<(u64, Instant)>> = const { RefCell::new(Vec::new()) };
}

impl Spans {
    /// The name a span is written under: `~` and the mark's name, or for a function mark the function's.
    fn name_of(&self, attributes: &Attributes<'_>) -> &'static str {
        let mut message = Message(None);
        attributes.record(&mut message);
        let plain = attributes.metadata().name();
        let name = match message.0 {
            Some(function) => format!("~{}", function.rsplit("::").next().unwrap_or(&function)),
            None => format!("~{plain}"),
        };
        let mut interned = self.interned.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(kept) = interned.get(&name) {
            return kept;
        }
        // A few hundred distinct names over the life of a diagnostic build, so they are leaked once each.
        let kept: &'static str = Box::leak(name.clone().into_boxed_str());
        interned.insert(name, kept);
        kept
    }
}

/// The `message` of a span, which is where `profiling::function_scope!` puts the function's name.
struct Message(Option<String>);

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = Some(format!("{value:?}"));
        }
    }
}

impl Subscriber for Spans {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.is_span()
    }

    fn new_span(&self, attributes: &Attributes<'_>) -> Id {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let name = self.name_of(attributes);
        self.names.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(id, name);
        Id::from_u64(id)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, _event: &Event<'_>) {}

    fn enter(&self, span: &Id) {
        OPEN.with(|open| open.borrow_mut().push((span.into_u64(), Instant::now())));
    }

    fn exit(&self, span: &Id) {
        let id = span.into_u64();
        let Some(entered) = OPEN.with(|open| {
            let mut open = open.borrow_mut();
            let at = open.iter().rposition(|(open_id, _)| *open_id == id)?;
            Some(open.remove(at).1)
        }) else {
            return;
        };
        let took = entered.elapsed().as_secs_f64() * 1000.0;
        if took < SLOWEST_KEPT_MS {
            return;
        }
        let name =
            self.names.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&id).copied();
        if let Some(name) = name {
            crate::services::frame_trace::value(name, took);
        }
    }

    fn try_close(&self, span: Id) -> bool {
        self.names.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&span.into_u64());
        true
    }
}
