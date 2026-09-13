//! Running `git` and reading what it said.
//!
//! Every call in this crate goes through here, and every one of them returns an [`Outcome`] whether
//! it worked or not, holding git's own standard output and standard error. Nothing invents a message
//! of its own for a failure. A rejected push, a merge conflict, a detached HEAD and a missing
//! upstream all have good messages already, written by people who know exactly what happened, and
//! replacing them with "could not push" would be a step backwards.
//!
//! Output is read as bytes and turned into text with `from_utf8_lossy`. A file name on Windows can
//! hold anything the file system allows, and a path Unluminous cannot spell is still a path it should be
//! able to list.

use std::cell::RefCell;
use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

/// What a git command did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// True when git exited with a status of zero.
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Outcome {
    /// Git's own message, which is what the window shows when something goes wrong.
    ///
    /// Standard error first, because that is where git explains itself, then standard output, which
    /// carries the summary of what a successful command did.
    pub fn message(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let stderr = self.stderr.trim();
        let stdout = self.stdout.trim();
        if !stderr.is_empty() {
            parts.push(stderr);
        }
        if !stdout.is_empty() {
            parts.push(stdout);
        }
        parts.join("\n")
    }

    /// The first line of the message, which is what fits in the status bar.
    pub fn summary(&self) -> String {
        self.message().lines().next().unwrap_or_default().to_owned()
    }

    /// A failure that never reached git at all: it is not installed, or the folder has gone.
    pub fn failed_to_run(problem: &std::io::Error) -> Self {
        Self {
            ok: false,
            stdout: String::new(),
            stderr: format!("git could not be run: {problem}"),
        }
    }
}

/// Run `git` in `folder` with `arguments`.
///
/// The working directory is set rather than `-C` being passed, so that a caller reading the command
/// back sees the same thing git sees.
/// What git is told before a value a person typed, so the value cannot be read as an option.
///
/// `task-1922` B4. Paths were already put after `--`; revisions, branch names, tags, remote names and
/// URLs were not, so a value beginning with a dash was read by git as an option to the subcommand it
/// was handed to. The shape that mattered was not the one that errors, it was the one that *works*:
/// `Reset` takes its revision from a text field and `git reset --soft --hard` is a hard reset,
/// because git takes the last mode named. Somebody who typed `--hard` into a box asking for a
/// revision lost everything they had not committed, and git reported success.
///
/// `--` does not answer it. For `reset`, `show` and `diff` the thing after `--` is a *path*, so
/// putting a revision there means something else entirely. `--end-of-options` is git's own answer,
/// exists in every builtin that parses options, and says exactly this: nothing after here is an
/// option. Measured against git 2.53 on each subcommand this crate uses it for -- `reset`, `switch`,
/// `branch`, `tag`, `merge`, `rebase`, `show`, `diff`, `push`, `pull`, `stash` and `remote` -- an
/// ordinary value is unaffected and a value beginning with a dash is refused in git's own words.
pub const END_OF_OPTIONS: &str = "--end-of-options";

pub fn run<S: AsRef<OsStr>>(folder: &Path, arguments: &[S]) -> Outcome {
    let mut command = Command::new("git");
    command.current_dir(folder);
    for argument in arguments {
        command.arg(argument);
    }
    // Git will not open an editor or ask for a password on the terminal from inside Unluminous: there is
    // no terminal for it to ask on, and a git that sits waiting for an answer nobody can give would
    // hang the worker thread for ever. A credential helper still works, because that is a program of
    // its own with a window of its own.
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GIT_EDITOR", "true");
    #[cfg(target_os = "windows")]
    {
        // Do not flash a console window for each command. 0x08000000 is CREATE_NO_WINDOW.
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // **Spawned rather than `Command::output()`**, which is `task-1922` B5: `output()` keeps the
    // child to itself, so there was no handle anywhere for a worker being dropped to kill, and
    // dropping one mid fetch left `git` running with nobody reading it. This is `task-1769`'s
    // measurement about terminals, made about git.
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(problem) => return Outcome::failed_to_run(&problem),
    };
    // The pipes come off the child before it is put where somebody else can kill it, so a kill
    // during the read still leaves this thread holding the ends it is reading.
    let output = child.stdout.take();
    let errors = child.stderr.take();
    let slot = hold(child);

    // **Standard error is drained on a thread of its own.** Both pipes have to be read at the same
    // time or a child that fills one while this thread reads the other never finishes. `output()`
    // did this itself; doing it by hand means doing this part by hand too.
    let draining = errors.map(|mut errors| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = errors.read_to_end(&mut bytes);
            bytes
        })
    });
    let mut said = Vec::new();
    if let Some(mut output) = output {
        let _ = output.read_to_end(&mut said);
    }
    let complained = draining.and_then(|thread| thread.join().ok()).unwrap_or_default();

    // Nothing to take means a worker's `Drop` killed and reaped it while this was reading, which is
    // exactly what closing a window during a fetch looks like from here.
    let Some(mut child) = slot.take() else {
        return Outcome { ok: false, stdout: String::new(), stderr: String::new() };
    };
    match child.wait() {
        Ok(status) => Outcome {
            ok: status.success(),
            stdout: String::from_utf8_lossy(&said).into_owned(),
            stderr: String::from_utf8_lossy(&complained).into_owned(),
        },
        Err(problem) => Outcome::failed_to_run(&problem),
    }
}

/// The `git` this thread is running, so that something else can kill it.
///
/// `task-1922` B5. `unluminous_git::Worker` had no `Drop`, discarded its `JoinHandle`, and ran git
/// through `Command::output()`, which holds the child itself -- so there was no handle anywhere for
/// a worker being dropped to reach, and a worker dropped mid fetch orphaned the git process. That is
/// the fault `task-1769` fixed for terminals, still open here.
#[derive(Default)]
pub struct Running(Mutex<Option<Child>>);

impl Running {
    /// Kill whatever is running on the thread this belongs to, if anything is.
    pub fn stop(&self) {
        if let Ok(mut held) = self.0.lock() {
            if let Some(mut child) = held.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn put(&self, child: Child) {
        if let Ok(mut held) = self.0.lock() {
            *held = Some(child);
        }
    }

    fn take(&self) -> Option<Child> {
        self.0.lock().ok().and_then(|mut held| held.take())
    }
}

thread_local! {
    /// Where [`run`] puts its child on this thread, when somebody has asked for one.
    ///
    /// A thread local because [`run`] is called from everywhere in this crate and from its tests, and
    /// threading a slot through every one of those signatures would be a parameter that eleven
    /// functions pass on and do not use. What is in it is an `Arc` the worker also holds, so the
    /// worker's `Drop` -- which runs on the window's thread -- reaches the child on this one.
    static RUNNING_HERE: RefCell<Option<Arc<Running>>> = const { RefCell::new(None) };
}

/// Put whatever `run` starts on this thread into `slot`, until the thread ends.
///
/// Called by [`crate::worker::Worker`] as the first thing its thread does.
pub fn put_this_thread_s_children_in(slot: Arc<Running>) {
    RUNNING_HERE.with(|here| *here.borrow_mut() = Some(slot));
}

/// Where this call's child goes: the thread's slot when there is one, a fresh slot when there is not.
///
/// A call with no worker behind it -- the window asking git its version, or a test -- still needs
/// somewhere to keep the child while it reads the pipes. It gets one of its own, which nothing else
/// can see and which goes away with the call.
fn hold(child: Child) -> Arc<Running> {
    let slot = RUNNING_HERE
        .with(|here| here.borrow().clone())
        .unwrap_or_else(|| Arc::new(Running::default()));
    slot.put(child);
    slot
}

/// Whether there is a `git` on this machine at all, and which version it is.
///
/// Asked once when a window starts. With no git, every git entry is dimmed and the status bar says
/// why, rather than each operation failing separately with a message about a program that is not
/// there.
pub fn version() -> Option<String> {
    let outcome = run(Path::new("."), &["--version"]);
    outcome.ok.then(|| outcome.stdout.trim().to_owned())
}

/// Split output that was asked for with `-z`, which separates its records with a zero byte.
///
/// Used wherever a path could be in the output. Git's ordinary output quotes and escapes a path with
/// a space or a newline in it, and unpicking that is a parser nobody should have to write; the zero
/// byte cannot appear in a path on either platform, so there is nothing to escape and nothing to
/// unpick.
pub fn split_nul(text: &str) -> Vec<&str> {
    text.split('\0').filter(|part| !part.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_puts_what_git_explained_first() {
        let outcome = Outcome {
            ok: false,
            stdout: "  To github.com:me/thing\n".to_owned(),
            stderr: " ! [rejected] main -> main (fetch first)\n".to_owned(),
        };
        assert_eq!(outcome.summary(), "! [rejected] main -> main (fetch first)");
        assert!(outcome.message().contains("To github.com:me/thing"));
    }

    #[test]
    fn a_message_with_nothing_in_it_is_empty_rather_than_blank_lines() {
        let outcome = Outcome { ok: true, stdout: "\n".to_owned(), stderr: String::new() };
        assert_eq!(outcome.message(), "");
        assert_eq!(outcome.summary(), "");
    }

    #[test]
    fn zero_separated_records_are_split_and_the_trailing_empty_one_dropped() {
        assert_eq!(split_nul("one\0two\0three\0"), vec!["one", "two", "three"]);
        assert_eq!(split_nul(""), Vec::<&str>::new());
        // A space in a path is left alone, which is the whole reason for asking for `-z`.
        assert_eq!(split_nul("my notes.md\0"), vec!["my notes.md"]);
    }
}
