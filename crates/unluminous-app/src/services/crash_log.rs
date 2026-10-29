//! Writing down what Unluminous was doing when it stopped.
//!
//! A panic in a graphical application on macOS leaves nothing behind. The message goes to standard
//! error, and an application launched from the Finder has no standard error — launchd sends it
//! nowhere — so the window disappears and there is not a line anywhere saying why. It does not even
//! leave a crash report: the panic unwinds out of the event loop and the process *exits*, and macOS
//! only files a report for a process that aborts or is killed by a signal.
//!
//! That is how a real report arrived — a crash while typing in a JavaScript file — with no message,
//! no location and nothing in `~/Library/Logs/DiagnosticReports`, and it could not be reproduced from
//! the description. A fault nobody can see is a fault nobody can fix, so this makes the next one
//! visible: every panic, on any thread, is appended to `crash.log` in the folder Unluminous keeps its
//! settings in, with the version, the time, where in the code it happened and a backtrace.
//!
//! It changes nothing else. The message still goes to standard error for anybody running Unluminous from a
//! terminal, and the panic still does what it did before.
//!
//! The one thing it does stop is a graphics error ending the window: [`survive_graphics_errors`] writes it here and
//! the window carries on, which is `task-2401`.

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The file panics are appended to, inside the folder given to [`install`].
pub const FILE: &str = "crash.log";
/// The file the log is moved to when it grows past [`LIMIT`], so one crash a day for a year cannot
/// fill a disk and the most recent ones are always the ones kept.
pub const OLD_FILE: &str = "crash.log.old";
/// How large the log may get before it is rotated.
pub const LIMIT: u64 = 256 * 1024;

/// Start writing panics to `folder`.
///
/// Chains to whatever hook was there before, so the message still reaches standard error exactly as
/// it did. Called once, from `main`, before the window is built — a panic while the window is being
/// built is one of the ones worth catching.
pub fn install(folder: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        // Written first, because formatting a backtrace is the slow part and a second panic inside
        // this hook would leave nothing at all.
        let report = report(panic, &std::backtrace::Backtrace::force_capture().to_string());
        if let Err(problem) = append(&folder, &report) {
            eprintln!("Unluminous could not write {}: {problem}", folder.join(FILE).display());
        } else {
            eprintln!("Unluminous wrote what happened to {}", folder.join(FILE).display());
        }
        previous(panic);
    }));
}

/// How many graphics errors one window writes down before it stops writing them. An error that comes back on
/// every frame would otherwise fill the log in seconds and push out the reports worth reading.
pub const GRAPHICS_ERRORS_WRITTEN: usize = 20;

/// Write a graphics error down and carry on drawing, rather than end the process.
///
/// wgpu's own answer to an error nothing asked about is to panic, and `task-2401` measured what that costs: a
/// window with the Skills pane open closed with `wgpu error: Out of Memory`, raised when a frame's command buffer
/// could not be finished, at a moment when both graphics cards were nearly full with the llama server and ComfyUI.
/// A frame that cannot be finished is a frame that is not drawn; the next one usually can be, so the window loses
/// one picture instead of every unsaved change in it.
///
/// @param device - the window's graphics device
/// @param folder - the folder `crash.log` is in, the one given to [`install`]
pub fn survive_graphics_errors(device: &eframe::wgpu::Device, folder: PathBuf) {
    let written = std::sync::atomic::AtomicUsize::new(0);
    device.on_uncaptured_error(std::sync::Arc::new(move |error: eframe::wgpu::Error| {
        let count = written.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count >= GRAPHICS_ERRORS_WRITTEN {
            return;
        }
        let report = graphics_report(&error.to_string(), count + 1 == GRAPHICS_ERRORS_WRITTEN);
        eprintln!("{report}");
        let _ = append(&folder, &report);
    }));
}

/// What one graphics error is written as: the panic's shape, so the log reads the same, with a line saying the
/// window went on.
fn graphics_report(error: &str, last: bool) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("Unluminous {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("at   {}\n", now()));
    out.push_str("graphics error, written down and drawing carried on\n");
    out.push_str(&format!("said {}\n", error.trim_end()));
    if last {
        out.push_str(&format!(
            "No more graphics errors are written down by this window after {GRAPHICS_ERRORS_WRITTEN}.\n"
        ));
    }
    out
}

/// What one panic is written as.
///
/// Split from the hook so that a test can read it without panicking: the thread's name is in it
/// because a panic on the symbol indexer's thread and a panic in the window are different faults,
/// and the version is in it because a report about a version nobody is running any more is the
/// commonest kind of report there is.
fn report(panic: &std::panic::PanicHookInfo<'_>, backtrace: &str) -> String {
    let thread = std::thread::current();
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("Unluminous {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("at   {}\n", now()));
    out.push_str(&format!("on   the {} thread\n", thread.name().unwrap_or("unnamed")));
    match panic.location() {
        Some(location) => out.push_str(&format!(
            "in   {}:{}:{}\n",
            location.file(),
            location.line(),
            location.column()
        )),
        None => out.push_str("in   somewhere with no location\n"),
    }
    out.push_str(&format!("said {}\n", message(panic)));
    out.push('\n');
    out.push_str(backtrace);
    if !backtrace.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// What the panic said, whichever of the two shapes a payload takes.
fn message(panic: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = panic.payload();
    if let Some(text) = payload.downcast_ref::<&str>() {
        return (*text).to_owned();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "a panic with nothing to say".to_owned()
}

/// The time, as far as the standard library alone can tell it: seconds since the epoch.
///
/// A civil date would need a calendar, and `unluminous-core` has one for git dates but this crate would
/// have to reach for it for one line in a file a person reads next to `date -r <seconds>`. The
/// seconds are unambiguous, which is what a log wants.
fn now() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(since) => format!("{} seconds past the epoch", since.as_secs()),
        Err(_) => "a time before the epoch".to_owned(),
    }
}

/// Add one report to the log, rotating it first if it has grown too large.
fn append(folder: &Path, report: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let path = folder.join(FILE);
    if std::fs::metadata(&path).map(|about| about.len() > LIMIT).unwrap_or(false) {
        // A rename rather than a truncation, so the crash that filled it is still readable.
        let _ = std::fs::rename(&path, folder.join(OLD_FILE));
    }
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(report.as_bytes())?;
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(name);
        std::fs::remove_dir_all(&folder).ok();
        folder
    }

    /// The hook is a process-wide thing, so this is one test rather than several: two tests setting a
    /// hook at the same time would each see the other's panics.
    #[test]
    fn a_panic_is_written_down_with_where_and_what_and_a_backtrace() {
        let folder = temporary("unluminous-crash-log");
        install(folder.clone());

        let outcome = std::panic::catch_unwind(|| {
            panic!("the fault this exists for");
        });
        assert!(outcome.is_err(), "the panic still happens; the hook only writes it down");

        let written = std::fs::read_to_string(folder.join(FILE)).expect("the log was written");
        assert!(written.contains("the fault this exists for"), "what it said: {written}");
        assert!(written.contains(env!("CARGO_PKG_VERSION")), "which version it was");
        assert!(written.contains("crash_log.rs:"), "where in the code, so it can be found");
        assert!(written.contains("seconds past the epoch"), "when");
        assert!(
            written.contains("unluminous_app") || written.contains("backtrace"),
            "and a backtrace: {written}"
        );

        // A second panic is appended rather than replacing the first, because the interesting one is
        // often not the last one.
        let _ = std::panic::catch_unwind(|| panic!("the second one"));
        let written = std::fs::read_to_string(folder.join(FILE)).expect("still there");
        assert!(written.contains("the fault this exists for"));
        assert!(written.contains("the second one"));
        // Counted by message rather than by how many blocks the file holds. The hook is process
        // wide, so a *different* test failing writes a block here too, and counting blocks made this
        // test fail whenever anything else in the crate did — which is a test that reports somebody
        // else's fault as its own.
        assert_eq!(written.matches("said the fault this exists for\n").count(), 1);
        assert_eq!(written.matches("said the second one\n").count(), 1);

        // A panic on another thread is caught too, which is where the indexer, the git worker and the
        // terminal's reader all live.
        let elsewhere = std::thread::Builder::new()
            .name("a-worker".to_owned())
            .spawn(|| panic!("from a thread of its own"))
            .expect("start the thread");
        assert!(elsewhere.join().is_err());
        let written = std::fs::read_to_string(folder.join(FILE)).expect("still there");
        assert!(written.contains("from a thread of its own"));
        assert!(written.contains("on   the a-worker thread"), "which thread it was: {written}");

        let _ = std::panic::take_hook();
        std::fs::remove_dir_all(&folder).ok();
    }

    /// `task-2401`: a graphics error is written in the panic's shape, says the window carried on, and the last one
    /// a window writes says no more will follow.
    #[test]
    fn a_graphics_error_is_written_down_as_one_the_window_survived() {
        let report = graphics_report("Out of Memory\n", false);
        assert!(report.starts_with("---\nUnluminous "), "{report}");
        assert!(report.contains("drawing carried on"), "{report}");
        assert!(report.contains("said Out of Memory\n"), "{report}");
        assert!(!report.contains("No more graphics errors"), "{report}");
        assert!(graphics_report("Out of Memory", true).contains("No more graphics errors"));
    }

    #[test]
    fn a_log_that_has_grown_too_large_is_rotated_rather_than_growing_for_ever() {
        let folder = temporary("unluminous-crash-log-rotate");
        std::fs::create_dir_all(&folder).expect("make the folder");
        std::fs::write(folder.join(FILE), vec![b'x'; LIMIT as usize + 1]).expect("a full log");

        append(&folder, "a new report\n").expect("append");

        let now = std::fs::read_to_string(folder.join(FILE)).expect("the new log");
        assert_eq!(now, "a new report\n", "the new one starts again");
        let old = std::fs::metadata(folder.join(OLD_FILE)).expect("the old one was kept");
        assert!(old.len() > LIMIT, "and it still holds what filled it");
        std::fs::remove_dir_all(&folder).ok();
    }
}
