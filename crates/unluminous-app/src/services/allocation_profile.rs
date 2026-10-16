//! Which code holds the Rust heap: every live allocation by the call site that made it, in a build made
//! with `diagnostic-allocations` and run with `UNLUMINOUS_HEAP_PROFILE=<file>` (`task-2279`).
//!
//! `allocation_trace` counts the heap; this says whose it is. Each allocation of [`SMALLEST`] bytes or more
//! is recorded with the return addresses above it and forgotten when it is freed, so what is left when the
//! window closes is what was held and who made it. The report groups the allocations by the first three
//! frames that are not the allocator or the standard library, largest first, and is written to the file
//! the variable names. Capturing a backtrace for every allocation makes the window several times slower,
//! which is why it is behind both the feature and the variable.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// The smallest allocation recorded. Smaller ones are counted in the total but not attributed.
const SMALLEST: usize = 64;
/// How many return addresses are kept for each allocation.
const DEPTH: usize = 24;

/// Whether recording is on, which is decided once from the variable.
static ON: AtomicBool = AtomicBool::new(false);
/// The return addresses above an allocation, and its size.
type Recorded = ([usize; DEPTH], usize);

/// The live allocations: address to frames and size.
static LIVE: Mutex<Option<HashMap<usize, Recorded>>> = Mutex::new(None);

thread_local! {
    /// Set while this thread is inside the recorder, so its own allocations are not recorded.
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

/// Turns recording on when `UNLUMINOUS_HEAP_PROFILE` is set. Called once, at start.
pub fn start() {
    if std::env::var_os("UNLUMINOUS_HEAP_PROFILE").is_some() {
        ON.store(true, Ordering::SeqCst);
    }
}

/// Records an allocation.
///
/// @param pointer - where it is
/// @param size - how many bytes
pub(crate) fn allocated(pointer: *mut u8, size: usize) {
    if pointer.is_null() || size < SMALLEST || !ON.load(Ordering::Relaxed) {
        return;
    }
    let _ = INSIDE.try_with(|inside| {
        if inside.replace(true) {
            return;
        }
        let mut frames = [0usize; DEPTH];
        let mut at = 0;
        // SAFETY: the unsynchronised walk is safe as long as no other thread is unwinding through this
        // one, and the recorder never panics while it walks.
        unsafe {
            backtrace::trace_unsynchronized(|frame| {
                frames[at] = frame.ip() as usize;
                at += 1;
                at < DEPTH
            });
        }
        if let Ok(mut live) = LIVE.lock() {
            live.get_or_insert_with(HashMap::new).insert(pointer as usize, (frames, size));
        }
        inside.set(false);
    });
}

/// Forgets an allocation that was freed.
///
/// @param pointer - where it was
pub(crate) fn freed(pointer: *mut u8) {
    if !ON.load(Ordering::Relaxed) {
        return;
    }
    let _ = INSIDE.try_with(|inside| {
        if inside.replace(true) {
            return;
        }
        if let Ok(mut live) = LIVE.lock() {
            if let Some(live) = live.as_mut() {
                live.remove(&(pointer as usize));
            }
        }
        inside.set(false);
    });
}

/// Whether a frame's name is the allocator, the standard library or the recorder itself, which say nothing
/// about whose memory it is.
///
/// @param name - the frame's symbol
fn is_plumbing(name: &str) -> bool {
    name.is_empty()
        || name.contains("allocation_profile")
        || name.contains("allocation_trace")
        || [
            "alloc::",
            "core::",
            "std::",
            "backtrace",
            "hashbrown",
            "<alloc",
            "<core",
            "<std",
            "<hashbrown",
        ]
        .iter()
        .any(|start| name.starts_with(start))
        || name.contains("__rust")
}

/// Writes the report to the file `UNLUMINOUS_HEAP_PROFILE` names, if recording is on. Called as the window
/// closes.
pub fn write_the_report() {
    let Some(file) = std::env::var_os("UNLUMINOUS_HEAP_PROFILE") else { return };
    if !ON.swap(false, Ordering::SeqCst) {
        return;
    }
    let held: Vec<Recorded> = LIVE
        .lock()
        .ok()
        .and_then(|live| live.as_ref().map(|l| l.values().cloned().collect()))
        .unwrap_or_default();
    let mut names: HashMap<usize, String> = HashMap::new();
    let mut sites: HashMap<String, (usize, usize)> = HashMap::new();
    for (frames, size) in held {
        let mut chain = Vec::new();
        for ip in frames.iter().copied().filter(|ip| *ip != 0) {
            let name = names.entry(ip).or_insert_with(|| {
                let mut found = String::new();
                backtrace::resolve(ip as *mut std::ffi::c_void, |symbol| {
                    if found.is_empty() {
                        found = symbol.name().map(|n| format!("{n:#}")).unwrap_or_default();
                    }
                });
                found
            });
            if !is_plumbing(name) {
                chain.push(name.clone());
                if chain.len() == 3 {
                    break;
                }
            }
        }
        let site = sites.entry(chain.join("  <-  ")).or_insert((0, 0));
        site.0 += size;
        site.1 += 1;
    }
    let mut sites: Vec<(String, (usize, usize))> = sites.into_iter().collect();
    sites.sort_by_key(|site| std::cmp::Reverse(site.1 .0));
    let total: usize = sites.iter().map(|s| s.1 .0).sum();
    let mut report = format!(
        "live heap in allocations of {SMALLEST} bytes or more: {:.1} MB\n",
        total as f64 / 1_048_576.0
    );
    for (site, (bytes, count)) in sites.into_iter().take(60) {
        report.push_str(&format!("{:8.2} MB {:7}  {site}\n", bytes as f64 / 1_048_576.0, count));
    }
    let _ = std::fs::write(file, report);
}
