//! A thread that runs one git command at a time, so the window never waits for one.
//!
//! `git fetch` over a slow link takes seconds and `git status` on a large repository is not free
//! either. A window that ran either of them where it draws would stop drawing until git finished,
//! which on a fetch means a window that looks as though it has crashed.
//!
//! So every operation is a [`Request`] sent to a thread, and every result is a [`Reply`] the window
//! drains once a frame. The thread holds a waker — a function that asks the window to draw again —
//! which is the same arrangement the terminal already uses, and for the same reason: a reply
//! arriving while the window is idle has to draw itself rather than waiting for the next mouse move.
//!
//! One at a time on purpose. Two git commands at once in one repository fight over `index.lock`, and
//! the failure that produces is confusing enough that people have written blog posts about it.
//! Queueing them costs nothing here: a person cannot press two menu entries at once.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use crate::branch::{self, MergeOptions, Resume};
use crate::command::Outcome;
use crate::ops::{self, PullStrategy, PushTarget, ResetMode, Stash};
use crate::{Blame, Branch, Commit, Repository, Status};

/// What the window asks the thread to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Read the working tree, the branches, the stashes and what is half-finished.
    Refresh,
    /// Who last changed each line of this file.
    Blame(PathBuf),
    /// Which lines of this file differ from the version git has.
    ChangedLines(PathBuf),
    /// The history of one file, or of the repository when there is no path.
    Log {
        path: Option<PathBuf>,
        limit: usize,
    },
    /// The unified diff of one file.
    Diff {
        path: PathBuf,
        staged: bool,
        revision: Option<String>,
    },
    /// The unified diff of one commit.
    ShowCommit(String),
    Add(Vec<String>),
    Unstage(Vec<String>),
    Rollback(Vec<String>),
    Commit {
        message: String,
        amend: bool,
    },
    /// Commit and then push, which is what `COMMIT AND PUSH...` does.
    CommitAndPush {
        message: String,
        amend: bool,
        target: PushTarget,
    },
    Push(PushTarget),
    Pull {
        remote: String,
        branch: String,
        strategy: PullStrategy,
    },
    Fetch,
    Merge {
        branch: String,
        options: MergeOptions,
    },
    Rebase(String),
    ResumeMerge(Resume),
    ResumeRebase(Resume),
    Reset {
        revision: String,
        mode: ResetMode,
    },
    Switch(String),
    CreateBranch(String),
    DeleteBranch {
        name: String,
        force: bool,
    },
    Tag(String),
    Stash {
        message: String,
        include_untracked: bool,
    },
    Unstash {
        name: String,
        drop: bool,
    },
    DropStash(String),
    AddRemote {
        name: String,
        url: String,
    },
    SetRemoteUrl {
        name: String,
        url: String,
    },
    RemoveRemote(String),
    Clone {
        parent: PathBuf,
        url: String,
    },
}

impl Request {
    /// What the status bar says while this is running.
    pub fn label(&self) -> String {
        match self {
            Request::Refresh => "Reading the repository".to_owned(),
            Request::Blame(_) => "Annotating".to_owned(),
            Request::ChangedLines(_) => "Reading the changes".to_owned(),
            Request::Log { .. } => "Reading the history".to_owned(),
            Request::Diff { .. } | Request::ShowCommit(_) => "Reading the diff".to_owned(),
            Request::Add(_) => "Staging".to_owned(),
            Request::Unstage(_) => "Unstaging".to_owned(),
            Request::Rollback(_) => "Rolling back".to_owned(),
            Request::Commit { .. } => "Committing".to_owned(),
            Request::CommitAndPush { .. } => "Committing and pushing".to_owned(),
            Request::Push(_) => "Pushing".to_owned(),
            Request::Pull { .. } => "Pulling".to_owned(),
            Request::Fetch => "Fetching".to_owned(),
            Request::Merge { branch, .. } => format!("Merging {branch}"),
            Request::Rebase(branch) => format!("Rebasing onto {branch}"),
            Request::ResumeMerge(_) => "Finishing the merge".to_owned(),
            Request::ResumeRebase(_) => "Finishing the rebase".to_owned(),
            Request::Reset { .. } => "Resetting".to_owned(),
            Request::Switch(name) => format!("Switching to {name}"),
            Request::CreateBranch(name) => format!("Starting {name}"),
            Request::DeleteBranch { name, .. } => format!("Deleting {name}"),
            Request::Tag(name) => format!("Tagging {name}"),
            Request::Stash { .. } => "Stashing".to_owned(),
            Request::Unstash { .. } => "Unstashing".to_owned(),
            Request::DropStash(_) => "Dropping the stash".to_owned(),
            Request::AddRemote { .. } | Request::SetRemoteUrl { .. } | Request::RemoveRemote(_) => {
                "Changing the remotes".to_owned()
            }
            Request::Clone { .. } => "Cloning".to_owned(),
        }
    }

    /// True when this could have changed the working tree, so the status is worth reading again.
    pub fn changes_things(&self) -> bool {
        !matches!(
            self,
            Request::Refresh
                | Request::Blame(_)
                | Request::ChangedLines(_)
                | Request::Log { .. }
                | Request::Diff { .. }
                | Request::ShowCommit(_)
        )
    }
}

/// Everything about the repository that one refresh reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub status: Status,
    pub branches: Vec<Branch>,
    pub stashes: Vec<Stash>,
    pub remotes: Vec<ops::Remote>,
    pub in_progress: Option<&'static str>,
}

/// What the thread sends back.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// A refresh, with everything it read.
    Snapshot(Box<Snapshot>),
    Blame(PathBuf, Blame),
    ChangedLines(PathBuf, Vec<(usize, crate::LineChange)>),
    Log(Vec<Commit>),
    /// Text to show in a panel, with a title saying what it is of.
    Text {
        title: String,
        body: String,
    },
    /// Something that changed the repository has finished. The label is what it was.
    Done {
        label: String,
        outcome: Outcome,
    },
    /// A clone finished, and this is where it landed.
    Cloned {
        folder: PathBuf,
        outcome: Outcome,
    },
}

/// A function the thread calls to have the window drawn again.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// The thread, and the two channels to it.
pub struct Worker {
    requests: Option<Sender<Request>>,
    replies: Receiver<Reply>,
    /// What is running now, for the status bar. `None` when the thread is idle.
    running: Option<String>,
    /// How many requests have been sent and not yet answered.
    outstanding: usize,
    /// The thread, kept so that [`Worker::drop`] can wait for it.
    thread: Option<std::thread::JoinHandle<()>>,
    /// The `git` the thread is running, so that [`Worker::drop`] can kill it.
    ///
    /// The thread installs this as its own in `command::put_this_thread_s_children_in`, so a child
    /// started over there is reachable from here -- which is the window's thread, where `Drop` runs.
    child: std::sync::Arc<crate::command::Running>,
}

impl Worker {
    /// Start a thread working on `repository`.
    pub fn start(repository: Repository, waker: Waker) -> std::io::Result<Self> {
        let (request_sender, request_receiver) = std::sync::mpsc::channel::<Request>();
        let (reply_sender, reply_receiver) = std::sync::mpsc::channel::<Reply>();
        let child = std::sync::Arc::new(crate::command::Running::default());
        let theirs = std::sync::Arc::clone(&child);
        let thread = std::thread::Builder::new()
            .name("unluminous-git".to_owned())
            .spawn(move || {
                // Every `git` this thread starts goes into the slot the `Worker` also holds, so a
                // window closing mid fetch can reach the process rather than leaving it running.
                crate::command::put_this_thread_s_children_in(theirs);
                // The loop ends when the sender is dropped, which happens when the window closes.
                for request in request_receiver {
                    let label = request.label();
                    let reply = run(&repository, request, &label);
                    if reply_sender.send(reply).is_err() {
                        break;
                    }
                    waker();
                }
            })?;
        Ok(Self {
            requests: Some(request_sender),
            replies: reply_receiver,
            running: None,
            outstanding: 0,
            thread: Some(thread),
            child,
        })
    }

    /// Ask for something. Returns false when the thread has gone, which only happens if it could not
    /// be started.
    pub fn send(&mut self, request: Request) -> bool {
        let label = request.label();
        let Some(requests) = self.requests.as_ref() else {
            return false;
        };
        match requests.send(request) {
            Ok(()) => {
                self.outstanding += 1;
                self.running = Some(label);
                true
            }
            Err(_) => false,
        }
    }

    /// Everything the thread has answered since the last time this was called.
    pub fn poll(&mut self) -> Vec<Reply> {
        let mut replies = Vec::new();
        while let Ok(reply) = self.replies.try_recv() {
            self.outstanding = self.outstanding.saturating_sub(1);
            replies.push(reply);
        }
        if self.outstanding == 0 {
            self.running = None;
        }
        replies
    }

    /// What is running, for the status bar.
    pub fn running(&self) -> Option<&str> {
        self.running.as_deref()
    }

    pub fn is_busy(&self) -> bool {
        self.outstanding > 0
    }
}

/// **Kill the git that is running, close the channel, and wait for the thread.** `task-1922` B5.
///
/// This is `unluminous_db::Worker::drop`'s shape, which is the one the review named as right. Before
/// it there was no `Drop` at all: the `JoinHandle` was thrown away at `start` and `command::run` used
/// `Command::output()`, which keeps the child to itself -- so a worker dropped mid fetch left `git`
/// running with nobody reading it. That is the fault `task-1769` fixed for terminals.
///
/// **Killed before it is waited for**, which is why this is three steps rather than one. The thread
/// reads the next request between jobs, so a worker in the middle of a fetch would not notice the
/// closed channel until the fetch finished -- and the join would then block the window for as long
/// as the network took. Killing the child first is what makes the wait short.
impl Drop for Worker {
    fn drop(&mut self) {
        self.child.stop();
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Do one request. Runs on the thread, so nothing here may touch the window.
fn run(repository: &Repository, request: Request, label: &str) -> Reply {
    let root = repository.root().to_path_buf();
    let done = |outcome: Outcome| Reply::Done { label: label.to_owned(), outcome };
    let borrowed: Vec<&str>;
    match request {
        Request::Refresh => Reply::Snapshot(Box::new(Snapshot {
            status: repository.status().unwrap_or_default(),
            branches: repository.branches(),
            stashes: ops::stashes(&root),
            remotes: ops::remotes(&root),
            in_progress: repository.in_progress().label(),
        })),
        Request::Blame(path) => {
            let blame = repository.blame(&path).unwrap_or_default();
            Reply::Blame(path, blame)
        }
        Request::ChangedLines(path) => {
            let changes = crate::diff::changed_lines(&root, &path);
            Reply::ChangedLines(path, changes)
        }
        Request::Log { path, limit } => {
            Reply::Log(repository.log(path.as_deref(), limit).unwrap_or_default())
        }
        Request::Diff { path, staged, revision } => {
            let outcome = crate::diff::of_path(&root, &path, staged, revision.as_deref());
            Reply::Text {
                title: format!("Diff \u{2014} {}", path.display()),
                body: if outcome.stdout.trim().is_empty() {
                    outcome.message()
                } else {
                    outcome.stdout
                },
            }
        }
        Request::ShowCommit(hash) => {
            let outcome = crate::diff::of_commit(&root, &hash);
            Reply::Text {
                title: format!("Commit {}", &hash[..hash.len().min(8)]),
                body: if outcome.stdout.trim().is_empty() {
                    outcome.message()
                } else {
                    outcome.stdout
                },
            }
        }
        Request::Add(paths) => {
            borrowed = paths.iter().map(String::as_str).collect();
            done(ops::add(&root, &borrowed))
        }
        Request::Unstage(paths) => {
            borrowed = paths.iter().map(String::as_str).collect();
            done(ops::unstage(&root, &borrowed))
        }
        Request::Rollback(paths) => {
            borrowed = paths.iter().map(String::as_str).collect();
            done(ops::rollback(&root, &borrowed))
        }
        Request::Commit { message, amend } => done(ops::commit(&root, &message, amend)),
        Request::CommitAndPush { message, amend, target } => {
            let committed = ops::commit(&root, &message, amend);
            // A push after a commit that did not happen would push whatever was there before, which
            // is not what was asked for.
            if !committed.ok {
                return done(committed);
            }
            done(ops::push(&root, &target))
        }
        Request::Push(target) => done(ops::push(&root, &target)),
        Request::Pull { remote, branch, strategy } => {
            done(ops::pull(&root, &remote, &branch, strategy))
        }
        Request::Fetch => done(ops::fetch(&root)),
        Request::Merge { branch, options } => done(branch::merge(&root, &branch, options)),
        Request::Rebase(name) => done(branch::rebase(&root, &name)),
        Request::ResumeMerge(what) => done(branch::resume_merge(&root, what)),
        Request::ResumeRebase(what) => done(branch::resume_rebase(&root, what)),
        Request::Reset { revision, mode } => done(ops::reset(&root, &revision, mode)),
        Request::Switch(name) => done(branch::switch(&root, &name)),
        Request::CreateBranch(name) => done(branch::create(&root, &name)),
        Request::DeleteBranch { name, force } => done(branch::delete(&root, &name, force)),
        Request::Tag(name) => done(ops::tag(&root, &name)),
        Request::Stash { message, include_untracked } => {
            done(ops::stash(&root, &message, include_untracked))
        }
        Request::Unstash { name, drop } => done(ops::unstash(&root, &name, drop)),
        Request::DropStash(name) => done(ops::drop_stash(&root, &name)),
        Request::AddRemote { name, url } => done(ops::add_remote(&root, &name, &url)),
        Request::SetRemoteUrl { name, url } => done(ops::set_remote_url(&root, &name, &url)),
        Request::RemoveRemote(name) => done(ops::remove_remote(&root, &name)),
        Request::Clone { parent, url } => {
            let (outcome, folder) = ops::clone(&parent, &url);
            Reply::Cloned { folder, outcome }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_operations_that_change_something_ask_for_a_refresh() {
        assert!(!Request::Refresh.changes_things());
        assert!(!Request::Log { path: None, limit: 20 }.changes_things());
        // A fetch does not touch the working tree, but it does move the remote branches, so how far
        // ahead or behind the branch is has changed and the status is worth reading again.
        assert!(Request::Fetch.changes_things());
        assert!(!Request::Blame(PathBuf::from("a.rs")).changes_things());
        assert!(Request::Commit { message: "m".to_owned(), amend: false }.changes_things());
        assert!(Request::Switch("main".to_owned()).changes_things());
    }

    #[test]
    fn every_request_says_what_it_is_doing() {
        let requests = [
            Request::Refresh,
            Request::Fetch,
            Request::Push(PushTarget {
                remote: "origin".to_owned(),
                branch: "main".to_owned(),
                set_upstream: false,
                force: false,
                tags: false,
            }),
            Request::Merge { branch: "feature".to_owned(), options: MergeOptions::default() },
        ];
        for request in requests {
            assert!(!request.label().is_empty(), "{request:?} should say what it is doing");
        }
        assert_eq!(
            Request::Merge { branch: "feature".to_owned(), options: MergeOptions::default() }
                .label(),
            "Merging feature",
            "the label names what it is working on, not just what kind of thing it is"
        );
    }

    /// **Dropping a worker kills the git it is running and waits for its thread.** `task-1922` B5.
    ///
    /// A fetch from a listener that accepts the connection and then says nothing stands in for a
    /// fetch over a slow network, because when a real network call comes back is not something a
    /// test can know and this is. `git://` rather than `https://` on purpose: the git protocol is
    /// spoken by git's own process, so the thing that hangs is the child this crate started and the
    /// kill reaches it directly.
    ///
    /// Two things are asserted and neither alone would be enough:
    ///
    /// - the drop returns in far less time than the fetch would take, which is what the kill buys.
    ///   With the join and no kill it would wait out the whole fetch, on the window's own thread.
    /// - the thread has really ended, read off the strong count of the slot it holds. Without a
    ///   `Drop` at all -- which is how this was -- the drop is instant and the thread is simply
    ///   abandoned, so the timing on its own would pass on the code this fixes.
    ///
    /// What this does **not** claim is that every descendant dies. A `pre-commit` hook inherits
    /// git's own standard output, so killing git leaves the hook holding the pipe this thread is
    /// reading; that is a process tree rather than a child, and reaping one is `unluminous-terminal`'s
    /// job object rather than anything here.
    #[test]
    fn dropping_a_worker_kills_the_git_it_is_running_and_waits_for_its_thread() {
        let root = std::env::temp_dir().join("unluminous-git-tests").join("worker-drop");
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).expect("make the folder");
        assert!(crate::command::run(&root, &["init", "--initial-branch=main"]).ok);

        // Accepts, and then says nothing at all. Held open for the length of the test by the thread
        // below, which is what makes git wait rather than fail.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("the address").port();
        // Held for far longer than the assertion below allows, and deliberately never joined: if
        // the connection were released early, git would finish by itself and the test would pass
        // whether or not anything killed it. The thread is asleep and the process ends with the
        // suite.
        std::thread::spawn(move || {
            let held = listener.incoming().next();
            std::thread::sleep(std::time::Duration::from_secs(60));
            drop(held);
        });
        assert!(
            crate::command::run(&root, &["remote", "add", "slow", &format!("git://127.0.0.1:{port}/x")])
                .ok
        );

        let repository = Repository::discover(&root).expect("a repository");
        let mut worker = Worker::start(repository, Arc::new(|| {})).expect("a worker");
        let slot = std::sync::Arc::clone(&worker.child);
        assert!(worker.send(Request::Fetch));

        // Wait until git is really connected, rather than for a number of milliseconds somebody
        // guessed. The slot holding a child is what says so.
        let waited = std::time::Instant::now();
        while std::sync::Arc::strong_count(&slot) == 2
            && waited.elapsed() < std::time::Duration::from_secs(20)
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::thread::sleep(std::time::Duration::from_millis(300));

        let dropping = std::time::Instant::now();
        drop(worker);
        let took = dropping.elapsed();
        assert!(took < std::time::Duration::from_secs(10), "the drop waited out the fetch: {took:?}");
        assert_eq!(
            std::sync::Arc::strong_count(&slot),
            1,
            "the thread has ended, so nothing but this test still holds the slot"
        );
    }
}
