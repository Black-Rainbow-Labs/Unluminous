//! Drawing the window's own furniture with `rux`: the title bar, the rail and the controls in the bar.
//!
//! `task-2241`. The Claude Design canvas "Unluminous", page Editing code, redraws the bar and the rail in the
//! language the Agent-Chat and Agent-Tasks panes already use: chips raised out of the bar, wells carved into
//! it, round buttons and pills. Those surfaces are `rux` decoration, so the window keeps one `RuxState` for
//! them.
//!
//! **It is held per thread and belongs to one egui context.** A window is one thread, and the screenshot
//! tests run many windows at once on many threads, each of which must draw its own furniture with its own
//! canvases. A test thread can also build a second window after the first, and a canvas is a texture of the
//! context that made it, so a state made for one context is thrown away when another context draws.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use egui::Rect;

/// The number given to the next egui context that draws furniture.
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static STATE: RefCell<Option<(u64, Rc<rux::RuxState>)>> = const { RefCell::new(None) };
}

/// The number this context was given the first time it drew furniture.
fn owner(ctx: &egui::Context) -> u64 {
    ctx.data_mut(|data| {
        *data.get_temp_mut_or_insert_with(egui::Id::new("chrome-kit-owner"), || {
            NEXT_OWNER.fetch_add(1, Ordering::Relaxed)
        })
    })
}

/// The furniture's state for the context drawing now, made fresh when another context drew last.
fn state_for(ctx: &egui::Context) -> Rc<rux::RuxState> {
    let owner = owner(ctx);
    STATE.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_ref() {
            Some((held, state)) if *held == owner => state.clone(),
            _ => {
                let state = Rc::new(rux::RuxState::deterministic(crate::theme::rux_theme()));
                *slot = Some((owner, state.clone()));
                state
            }
        }
    })
}

/// The window's `rux` state for the context drawing now, for a part of the window that opens its own layers:
/// the Settings window's cards (`components::settings_kit`), which share the furniture's canvases and its
/// `end_frame`.
pub fn state_for_the_window(ctx: &egui::Context) -> Rc<rux::RuxState> {
    state_for(ctx)
}

/// Draw with `rux` into `rect`, in a layer that reaches `reach` points past it for the shadows.
pub fn layer<R>(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    rect: Rect,
    reach: f32,
    add: impl FnOnce(&mut rux::Rux<'_>) -> R,
) -> R {
    let id = ui.id().with(("window-chrome", salt));
    let state = state_for(ui.ctx());
    crate::theme::in_step(&state);
    state.set_still(true);
    rux::layer(ui, &state, id, rect.expand(reach), add)
}

/// Give back the canvases of furniture not drawn this frame, once the frame has been drawn.
pub fn end_frame(ctx: &egui::Context) {
    state_for(ctx).end_frame();
}

/// The design's values for the window's own ground.
pub fn chat() -> rux::theme::Chat {
    crate::theme::rux_theme().chat
}

/// The groove the design draws between the bar and the body and down the rail's edge: a dark line and a lit
/// one beside it, `border-top: 1px solid #0e1115; border-bottom: 1px solid #2a2f37`.
pub fn groove(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2) {
    let chat = chat();
    // Below a line that runs across, and to the right of one that runs down.
    let across = match (to.x - from.x).abs() > (to.y - from.y).abs() {
        true => egui::Vec2::Y,
        false => egui::Vec2::X,
    };
    painter.line_segment([from, to], egui::Stroke::new(1.0, chat.dark));
    painter.line_segment([from + across, to + across], egui::Stroke::new(1.0, chat.light));
}
