// The window suite's receipt, so that a release cannot be made from a commit the suite never saw.
//
// `task-1928` took the continuous integration out and made `tools/release.ps1` the only gate, and
// `task-1984` T9 found what that left: the gate deliberately does not run the 639 window tests --
// they need a graphics card and a person to open any image that changed, which is the one thing a
// script must not satisfy on its own -- so nothing anywhere records when they last passed. A
// release could be, and was, cut from a commit whose window suite had never been run.
//
// So each window test binary leaves a receipt naming the commit it ran at, and both release scripts
// refuse to publish while a receipt is missing or names a commit that is not an ancestor of HEAD.
// The suite stays manual; what stops being possible is releasing without having run it.
//
// **A receipt is written when the binary starts and deleted the moment anything in it panics.** A
// test binary cannot be asked at the end whether it passed -- libtest runs no code of ours after the
// last test -- but it can be asked, through a panic hook, the instant one fails. So the presence of
// a receipt means "this binary ran at this commit and nothing in it panicked", which is the property
// the release script needs, arrived at from the other direction. A failure that is not a panic (an
// abort, a stack overflow) is outside what this can see, which is why the receipt is a guard against
// forgetting to run the suite rather than a second opinion about whether it passed.

use std::io::Write;
use std::sync::OnceLock;

/// Where the receipts live. Under `_agent_output`, which is gitignored, because a receipt is about
/// this machine and this checkout rather than about the code.
pub fn folder() -> std::path::PathBuf {
    repository_root().join("_agent_output").join("window-suite")
}

/// The checkout this test is running out of.
///
/// `CARGO_MANIFEST_DIR` is `crates/unluminous-app`, so the root is two above it. Asked of cargo
/// rather than of the current directory, which a test may have changed.
fn repository_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the workspace root above crates/unluminous-app")
        .to_path_buf()
}

/// The commit HEAD is at, or `None` outside a git checkout.
fn head() -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repository_root())
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    match output.status.success() {
        true => Some(String::from_utf8_lossy(&output.stdout).trim().to_owned()),
        false => None,
    }
}

/// This test binary's name, which is what the receipt is filed under.
///
/// `std::env::current_exe` is `…/deps/window_and_chrome-<hash>.exe`, so the hash comes off. The
/// binary name is what `cargo test --test <name>` takes and what the release script's list holds.
fn binary_name() -> String {
    let path = std::env::current_exe().expect("the test binary's own path");
    let stem = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    match stem.rsplit_once('-') {
        Some((name, hash)) if hash.len() >= 8 && hash.chars().all(|c| c.is_ascii_hexdigit()) => {
            name.to_owned()
        }
        _ => stem,
    }
}

/// The file this binary's receipt is written to.
fn path() -> std::path::PathBuf {
    folder().join(format!("{}.txt", binary_name()))
}

/// Write this binary's receipt and arrange for it to be taken away again if anything panics.
///
/// Called from `common::builder`, which every harness in the suite is built through, so a test file
/// added later is covered without its author knowing this exists. Once per process: the hook is
/// installed once and the file is written once.
pub fn note_this_binary_started() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let Some(commit) = head() else { return };
        let folder = folder();
        if std::fs::create_dir_all(&folder).is_err() {
            return;
        }
        // The default hook first, so a failing test still prints its message and its backtrace, and
        // then the receipt goes. `set_hook` replaces rather than adds, so the previous one is kept
        // and called: another hook installed by a test is not thrown away by this.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            previous(info);
            let _ = std::fs::remove_file(path());
        }));
        let when = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or_default();
        if let Ok(mut file) = std::fs::File::create(path()) {
            let _ = writeln!(file, "{commit} {when} {}", binary_name());
        }
    });
}
