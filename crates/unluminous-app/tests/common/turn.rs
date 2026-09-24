// One window test binary on this machine at a time, whichever checkout it was built in.
//
// Every fixture in this module is a folder under the temporary folder with a fixed name, and every
// one of them is cleared before it is written: `unluminous-screenshot-folder`, the folders
// `fixture(name)` makes, `git_folder(name)`, `copy_out_of_the_repository`. The name cannot carry
// anything unique, because it is drawn: the title bar and the explorer's heading of hundreds of
// accepted pictures say `unluminous-screenshot-folder`.
//
// Their comments all said the same thing about that: `cargo test` runs one test binary at a time, so
// there is only ever one writer. That is true of one checkout and false of a machine. Each ticket
// works in a git worktree of its own, and each worktree's `cargo test` runs its own binaries, so two
// or three suites run at once on one `%TEMP%`. **`task-2100` is what it cost.**
// `the_canvas_zoom_controls` failed by about 1,100 pixels in both of its pictures, in three runs out
// of five on a loaded machine: another worktree's `git` binary started at the same second
// `unluminous-screenshot-folder` was deleted and written again, the task-2062 watch noticed that
// `readme.md` had changed on disk and re-read it, the status bar said `Reloaded ...readme.md`, and
// the explorer's tree was rebuilt with its one visible row at a different scroll position. Load only
// decided whether a test lived long enough for `WATCH_INTERVAL` to come round after the rebuild.
//
// So a binary takes the machine's turn before it builds a fixture or a window, and holds it until the
// process ends, when the operating system lets go of the lock whether the binary passed, failed or was
// killed. A second binary waits, and says which process it is waiting for.

use std::io::Write;
use std::sync::OnceLock;

/// How long a binary waits for its turn before it gives up and says why.
///
/// One window binary takes about twenty seconds alone and a few minutes on a loaded machine, and a
/// binary can wait behind several of them from other checkouts, so this is generous. It exists so a
/// holder that has hung is reported as a hang rather than as a test run that never ends.
const LONGEST_WAIT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// How often a waiting binary asks again.
const ASK_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

/// Take this machine's turn at the window test fixtures, once per binary, and keep it until exit.
///
/// Called by everything in `common` that builds a window or writes a fixture under the temporary
/// folder, so a test cannot reach either without it. Every call after the first returns at once.
pub fn take_the_machines_turn() {
    static TURN: OnceLock<std::fs::File> = OnceLock::new();
    TURN.get_or_init(wait_for_the_turn);
}

/// Wait for the lock file, then record which process holds it.
///
/// `File::try_lock` is stable from Rust 1.89 and the workspace declares `rust-version = "1.85"`, so
/// clippy's `incompatible_msrv` refuses it. The allowance is for this test harness only: the window
/// tests are built with the toolchain `rust-toolchain.toml` pins, which is 1.95, and nothing shipped
/// calls this. The shipped crates still keep to 1.85.
#[allow(clippy::incompatible_msrv)]
fn wait_for_the_turn() -> std::fs::File {
    let lock = std::env::temp_dir().join("unluminous-window-tests.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock)
        .unwrap_or_else(|error| panic!("open {}: {error}", lock.display()));
    let started = std::time::Instant::now();
    let mut said_so = false;
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) => {
                // Written to the stream itself rather than through `eprintln!`, which libtest captures
                // and shows only when a test fails, so a binary that waited minutes said nothing.
                if !said_so {
                    let _ = writeln!(
                        std::io::stderr(),
                        "waiting for another window test binary to finish: {}",
                        holder()
                    );
                    said_so = true;
                }
                assert!(
                    started.elapsed() < LONGEST_WAIT,
                    "waited {} minutes for another window test binary, which may have hung: {}",
                    LONGEST_WAIT.as_secs() / 60,
                    holder()
                );
                std::thread::sleep(ASK_EVERY);
            }
            Err(std::fs::TryLockError::Error(error)) => panic!("lock {}: {error}", lock.display()),
        }
    }
    note_the_holder();
    file
}

/// The file naming the process that holds the turn.
///
/// A file of its own, because on Windows a locked file cannot be read by anybody else.
fn holder_file() -> std::path::PathBuf {
    std::env::temp_dir().join("unluminous-window-tests.holder")
}

/// Write down which process holds the turn, so a waiting binary can say what it is waiting for.
fn note_the_holder() {
    let exe = std::env::current_exe().map(|path| path.display().to_string()).unwrap_or_default();
    if let Ok(mut file) = std::fs::File::create(holder_file()) {
        let _ = writeln!(file, "process {} running {exe}", std::process::id());
    }
}

/// Which process holds the turn, as far as the holder file says.
fn holder() -> String {
    std::fs::read_to_string(holder_file())
        .map(|text| text.trim().to_owned())
        .unwrap_or_else(|_| "a process that has not said which it is".to_owned())
}
