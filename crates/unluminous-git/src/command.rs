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
    // **Its own process group**, so that stopping it reaches the children it starts: `git fetch
    // --all` runs a `git fetch` per remote, and killing only the parent leaves that one holding the
    // pipe this thread is reading. On Windows the job object in `reap` does the same job.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
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
pub struct Running(Mutex<Option<Held>>);

/// One `git`, and whatever is needed to end everything it started along with it.
struct Held {
    child: Child,
    /// The job object the child was put in, on the platform that has them.
    #[cfg(windows)]
    job: Option<reap::Job>,
}

impl Running {
    /// Kill whatever is running on the thread this belongs to, and everything it started.
    ///
    /// **The whole tree, not the one process.** `task-1922` B5 first killed only the child this crate
    /// spawned, and CI measured what that is worth: `git fetch --all` runs a `git fetch` of its own
    /// per remote, so the parent died and the grandchild went on holding the pipe the reader is
    /// blocked on. The drop then waited out the entire fetch on the window's own thread, which is
    /// the fault it was meant to fix. `unluminous-terminal::reap` made the same measurement about a
    /// shell and answered it the same way: a job object on Windows, a process group on Unix.
    pub fn stop(&self) {
        if let Ok(mut slot) = self.0.lock() {
            if let Some(mut held) = slot.take() {
                reap::everything_below(&held.child);
                #[cfg(windows)]
                if let Some(job) = held.job.take() {
                    job.terminate();
                }
                let _ = held.child.kill();
                let _ = held.child.wait();
            }
        }
    }

    fn put(&self, child: Child) {
        #[cfg(windows)]
        let job = reap::Job::holding(&child);
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(Held {
                child,
                #[cfg(windows)]
                job,
            });
        }
    }

    fn take(&self) -> Option<Child> {
        self.0.lock().ok().and_then(|mut slot| slot.take()).map(|held| held.child)
    }
}

/// Ending a `git` and everything it started.
///
/// Two platforms and two mechanisms, which is `unluminous-terminal::reap`'s shape. There is no shared
/// crate: this is forty lines, that one is about a pseudoconsole, and a crate holding both would be a
/// trait with two unrelated implementations behind it.
mod reap {
    use std::process::Child;

    /// Kill every process the child started, on the platform where that is a process group.
    ///
    /// Nothing to do on Windows, where the job object below has already done it.
    #[cfg(unix)]
    pub fn everything_below(child: &Child) {
        // The child leads its own process group, because `run` asks for one. A negative process id
        // names that whole group, which is what makes this reach `git fetch --all`'s own children.
        // SAFETY: `kill` takes two integers and returns one. Nothing here holds a pointer.
        unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
    }

    /// The job object holds the tree on Windows, so there is nothing to do here.
    #[cfg(not(unix))]
    pub fn everything_below(_: &Child) {}

    #[cfg(windows)]
    pub use windows::Job;

    #[cfg(windows)]
    mod windows {
        use std::ffi::c_void;
        use std::os::windows::io::AsRawHandle;
        use std::process::Child;

        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        /// One job object holding one `git` and its descendants.
        pub struct Job(HANDLE);

        // The handle is owned by this value and only ever used through it: created here, closed in
        // `Drop`, and never handed to another thread while a second copy exists.
        unsafe impl Send for Job {}
        unsafe impl Sync for Job {}

        impl Job {
            /// A job holding `child`, or `None` when any step of it fails.
            ///
            /// Every failure is the same answer, because there is one thing to do about any of them:
            /// a git command that runs without the guarantee is better than one that does not run.
            /// The likeliest by far is a process that has already exited, which needs no reaping.
            ///
            /// A child that starts one of its own between being spawned and being adopted here is
            /// outside the job. `CREATE_SUSPENDED` would close that window and
            /// `std::process::Command` cannot ask for it; `unluminous-terminal::reap` accepts the
            /// same window for the same reason.
            pub fn holding(child: &Child) -> Option<Self> {
                let handle = child.as_raw_handle();
                if handle.is_null() {
                    return None;
                }
                // SAFETY: an unnamed job with default security, then the one limit that makes
                // closing the handle end what is inside it, then the child. Each call is checked
                // before the next is made, and the handle is closed by `Drop` on every path out.
                unsafe {
                    let made = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                    if made.is_null() {
                        return None;
                    }
                    let job = Self(made);
                    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                    let set = SetInformationJobObject(
                        job.0,
                        JobObjectExtendedLimitInformation,
                        std::ptr::addr_of!(limits) as *const c_void,
                        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                    );
                    if set == 0 {
                        return None;
                    }
                    if AssignProcessToJobObject(job.0, handle as HANDLE) == 0 {
                        return None;
                    }
                    Some(job)
                }
            }

            /// End everything in the job now. The handle is closed by `Drop` straight after.
            pub fn terminate(self) {
                // SAFETY: a job handle this value owns and has not closed.
                unsafe { TerminateJobObject(self.0, 1) };
            }
        }

        impl Drop for Job {
            /// Closing the handle is what ends what is inside it, because of the limit above.
            fn drop(&mut self) {
                // SAFETY: closed exactly once, here, for a handle this value owns.
                unsafe { CloseHandle(self.0) };
            }
        }
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
