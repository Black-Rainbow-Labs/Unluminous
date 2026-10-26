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
/// this machine rather than about the code.
///
/// **In the main checkout, whichever checkout the binary was built in (`task-2240`).** A ticket runs
/// the suite in its own worktree and the release is cut from the main checkout, so while each
/// checkout kept its own folder the main checkout had no receipts after a merge, and the whole suite
/// was built and run a second time there on code it had already passed. A receipt names a commit, and
/// `tools/window-suite.mjs` only accepts one that HEAD is built on with nothing a window is made of
/// changed since, so a receipt from another ticket's branch is reported stale rather than trusted.
pub fn folder() -> std::path::PathBuf {
    main_checkout().join("_agent_output").join("window-suite")
}

/// The main checkout: the parent of git's common directory, asked from this checkout. A worktree's
/// common directory is the main checkout's `.git`. Outside git, or when git cannot answer, this
/// checkout.
fn main_checkout() -> std::path::PathBuf {
    let root = repository_root();
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let common = std::path::PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            common.parent().map(std::path::Path::to_path_buf).unwrap_or(root)
        }
        _ => root,
    }
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
pub fn binary_name() -> String {
    let path = std::env::current_exe().expect("the test binary's own path");
    let stem = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    match stem.rsplit_once('-') {
        Some((name, hash)) if hash.len() >= 8 && hash.chars().all(|c| c.is_ascii_hexdigit()) => {
            name.to_owned()
        }
        _ => stem,
    }
}

/// The file this binary's receipt at `commit` is written to: `<binary>@<commit>.txt`.
///
/// **One file per commit, not one per binary (`task-2285`).** The folder is shared by every worktree,
/// and with one file a binary, two tickets running the suite at once replaced each other's receipts:
/// a ticket whose suite had passed at its own commit found the other ticket's base commit in every
/// file and could not release. A receipt per commit lets each run keep its own, and a panic removes
/// only the receipt of the run that panicked. `tools/window-suite.mjs` still reads `<binary>.txt`,
/// the name used before this.
fn path(commit: &str) -> std::path::PathBuf {
    folder().join(format!("{}@{}.txt", binary_name(), &commit[..commit.len().min(12)]))
}

/// How long a receipt is kept. A receipt older than a release cycle answers nothing a release asks.
const KEPT_FOR: std::time::Duration = std::time::Duration::from_secs(14 * 24 * 60 * 60);

/// Remove this binary's receipts that are older than [`KEPT_FOR`], so one file a commit does not grow
/// for ever.
fn forget_old_receipts(folder: &std::path::Path) {
    let prefix = format!("{}@", binary_name());
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > KEPT_FOR);
        if name.starts_with(&prefix) && name.ends_with(".txt") && old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Whether libtest was asked to run every test in the binary: no name to filter by, no `--skip`, and
/// not only the ignored tests or a listing.
///
/// `task-2364`. A receipt says the whole binary passed, and it was written by any run of the binary,
/// so `cargo test --test word -- one_test` left a receipt for all of `word.rs`. A filtered run writes
/// none now. `tools/window-suite.mjs --run` runs a failed test again on its own and sets
/// `UNLUMINOUS_RECEIPT_RETRY`, because the rest of that binary has just passed in the same run.
fn runs_the_whole_binary(mut args: impl Iterator<Item = String>) -> bool {
    // The options libtest reads a value after.
    const TAKES_A_VALUE: &[&str] =
        &["--test-threads", "--format", "--color", "--logfile", "-Z", "--shuffle-seed"];
    while let Some(arg) = args.next() {
        if TAKES_A_VALUE.contains(&arg.as_str()) {
            args.next();
            continue;
        }
        if arg == "--skip" || arg.starts_with("--skip=") {
            return false;
        }
        if matches!(arg.as_str(), "--ignored" | "--list" | "--exact") {
            return false;
        }
        if !arg.starts_with('-') {
            return false;
        }
    }
    true
}

thread_local! {
    /// Set while this thread runs work that may panic and is caught on purpose, so the hook leaves the
    /// receipt alone. See [`while_a_panic_is_expected`].
    static A_PANIC_IS_EXPECTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Run `work`, during which a panic on this thread is caught by the caller and does not mean a test
/// failed, so the receipt is not taken away for it.
///
/// `task-2364`. `egui_kittest` panics rather than returning an error when the graphics card does not
/// finish a render in ten seconds (`Failed to poll device: Timeout`), which a loaded machine does.
/// `common::PatientRenderer` catches that and renders again; without this, the panic it caught took
/// the binary's receipt away although every test in it passed.
pub fn while_a_panic_is_expected<T>(work: impl FnOnce() -> T) -> T {
    A_PANIC_IS_EXPECTED.with(|expected| expected.set(true));
    let answer = work();
    A_PANIC_IS_EXPECTED.with(|expected| expected.set(false));
    answer
}

/// Write this binary's receipt and arrange for it to be taken away again if anything panics.
///
/// Called from `common::builder`, which every harness in the suite is built through, so a test file
/// added later is covered without its author knowing this exists. Once per process: the hook is
/// installed once and the file is written once.
pub fn note_this_binary_started() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        if !runs_the_whole_binary(std::env::args().skip(1))
            && std::env::var_os("UNLUMINOUS_RECEIPT_RETRY").is_none()
        {
            return;
        }
        let Some(commit) = head() else { return };
        let folder = folder();
        if std::fs::create_dir_all(&folder).is_err() {
            return;
        }
        forget_old_receipts(&folder);
        let receipt = path(&commit);
        // The default hook first, so a failing test still prints its message and its backtrace, and
        // then the receipt goes. `set_hook` replaces rather than adds, so the previous one is kept
        // and called: another hook installed by a test is not thrown away by this.
        let previous = std::panic::take_hook();
        let taken_away = receipt.clone();
        std::panic::set_hook(Box::new(move |info| {
            previous(info);
            if !A_PANIC_IS_EXPECTED.with(std::cell::Cell::get) {
                let _ = std::fs::remove_file(&taken_away);
            }
        }));
        let when = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or_default();
        if let Ok(mut file) = std::fs::File::create(&receipt) {
            let _ = writeln!(file, "{commit} {when} {}", binary_name());
        }
    });
}
