//! The window's git state: the repository, the thread that runs the commands, and what it last said.
//!
//! Nothing here draws and nothing here runs a git command. The commands run on
//! [`unluminous_git::Worker`]'s thread and the drawing is in `components::git_panel` and
//! `components::git_dialogs`; this is what sits between them, holding the last answer so that the
//! window has something to draw between one command and the next.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use unluminous_git::worker::{Reply, Request, Snapshot};
use unluminous_git::{Commit, Repository, Worker};

use crate::app::files::OpenFiles;
use crate::components::git_dialogs::GitDialogs;
use crate::components::git_panel::CommitPanel;
use crate::components::gutter::{BlameRow, Change};

use crate::components::branch_widget;
use crate::components::git_dialogs::{self, Dialog};
use crate::components::git_panel;
use crate::components::prompt_dialog::{Prompt, Purpose};

use crate::app::actions::{Action, GitAction};
use crate::app::git;
use crate::app::{Answer, Confirmation, UnluminousApp};

/// How many commits the history window reads. A repository can hold a hundred thousand; a window
/// shows a few dozen and scrolling past two hundred is not how anyone looks for a commit.
pub const HISTORY_LIMIT: usize = 200;

/// Everything the window knows about the repository it is in.
pub struct GitState {
    pub repository: Repository,
    worker: Worker,
    /// What the last refresh read.
    pub snapshot: Snapshot,
    pub panel: CommitPanel,
    pub dialogs: GitDialogs,
    pub history: Vec<Commit>,
    /// The messages of the last few commits, for the panel's clock button.
    pub recent_messages: Vec<String>,
    /// What git last said, for the status bar.
    pub message: Option<String>,
    /// Set once the first read has come back.
    read: bool,
}

impl GitState {
    /// Start working on the repository `folder` is in, if it is in one.
    pub fn open(folder: &Path, waker: Arc<dyn Fn() + Send + Sync>) -> Option<Self> {
        let repository = Repository::discover(folder)?;
        Self::from_repository(repository, waker)
    }

    /// Start working on an already discovered repository.
    ///
    /// `None` when the thread could not be started, which is `task-1922` B6. A machine too short of
    /// threads to start one more should lose git for this session; it should not lose the editor,
    /// which is what an `expect` here did.
    pub fn from_repository(
        repository: Repository,
        waker: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<Self> {
        let mut worker = Worker::start(repository.clone(), waker).ok()?;
        worker.send(Request::Refresh);
        Some(Self {
            repository,
            worker,
            snapshot: Snapshot::default(),
            panel: CommitPanel::default(),
            dialogs: GitDialogs::default(),
            history: Vec::new(),
            recent_messages: Vec::new(),
            message: None,
            read: false,
        })
    }

    /// Ask the thread for something.
    pub fn send(&mut self, request: Request) {
        self.message = Some(format!("{}\u{2026}", request.label()));
        self.worker.send(request);
    }

    /// What is running, for the status bar.
    pub fn running(&self) -> Option<&str> {
        self.worker.running()
    }

    /// Whether the worker still owes the window its first or latest answer.
    pub fn is_busy(&self) -> bool {
        !self.read || self.worker.running().is_some()
    }

    /// A path as git spells it, relative to the root.
    pub fn relative(&self, path: &Path) -> Option<String> {
        self.repository.relative(path)
    }

    /// Take everything the thread has answered and put it where the window will draw it.
    ///
    /// Returns true when something needs laying out again, which is only ever the blame column and
    /// the change bars, because those are the only replies that change what the editing area shows.
    pub fn take_replies(&mut self, files: &mut OpenFiles) -> bool {
        let mut redraw = false;
        for reply in self.worker.poll() {
            match reply {
                Reply::Snapshot(snapshot) => {
                    self.snapshot = *snapshot;
                    self.read = true;
                    // A commit that has just been made is not in the panel's message any more.
                    if let Some(label) = self.snapshot.in_progress {
                        self.message = Some(format!(
                            "{label} \u{2014} finish it or abandon it from the Git menu"
                        ));
                    }
                }
                Reply::Blame(path, blame) => {
                    let rows: Vec<BlameRow> = blame
                        .lines
                        .into_iter()
                        .map(|line| BlameRow {
                            date: line.date,
                            author: shorten(&line.author),
                            commit: line.commit,
                            age: line.age,
                            summary: line.summary,
                        })
                        .collect();
                    if let Some(index) = files.index_of(&path) {
                        if let Some(file) = files.get_mut(index) {
                            file.blame = Some(rows);
                        }
                        redraw = true;
                    }
                    self.message = None;
                }
                Reply::ChangedLines(path, changes) => {
                    let changes: Vec<(usize, Change)> = changes
                        .into_iter()
                        .map(|(line, kind)| {
                            (
                                line,
                                match kind {
                                    unluminous_git::LineChange::Added => Change::Added,
                                    unluminous_git::LineChange::Modified => Change::Modified,
                                },
                            )
                        })
                        .collect();
                    if let Some(index) = files.index_of(&path) {
                        if let Some(file) = files.get_mut(index) {
                            file.line_changes = changes;
                        }
                        redraw = true;
                    }
                    self.message = None;
                }
                Reply::Log(commits) => {
                    self.recent_messages = commits
                        .iter()
                        .take(20)
                        .map(|commit| {
                            if commit.body.trim().is_empty() {
                                commit.subject.clone()
                            } else {
                                format!("{}\n\n{}", commit.subject, commit.body.trim())
                            }
                        })
                        .collect();
                    self.history = commits;
                    self.message = None;
                }
                Reply::Text { title, body } => {
                    self.dialogs.open =
                        Some(crate::components::git_dialogs::Dialog::Text { title, body });
                    self.message = None;
                }
                Reply::Done { label, outcome } => {
                    // Git's own message, always. A rejected push and a merge conflict both explain
                    // themselves better than anything Unluminous could say about them.
                    let said = outcome.summary();
                    self.message = Some(if outcome.ok {
                        if said.is_empty() {
                            format!("{label}: done")
                        } else {
                            said
                        }
                    } else {
                        format!("{label} failed: {said}")
                    });
                    self.worker.send(Request::Refresh);
                    self.worker.send(Request::Log { path: None, limit: HISTORY_LIMIT });
                    // What git says about a file has changed, so anything annotated is annotated
                    // against a version that has gone.
                    files.forget_git();
                    redraw = true;
                }
                Reply::Cloned { folder, outcome } => {
                    self.message = Some(if outcome.ok {
                        format!("Cloned into {}", folder.display())
                    } else {
                        format!("Clone failed: {}", outcome.summary())
                    });
                    if outcome.ok {
                        crate::services::launcher::open_window(&folder);
                    }
                }
            }
        }
        redraw
    }

    /// What the status bar says about the repository, once there is anything to say.
    ///
    /// `None` until the first read comes back. Reading happens on a thread, so for the first few
    /// frames there is nothing to report, and an empty status has no branch in it — which the label
    /// spells `detached HEAD`. Announcing that in the gap is alarming, wrong, and the first thing
    /// anybody sees when they open a project.
    pub fn status_label(&self) -> Option<String> {
        if !self.read {
            return None;
        }
        let mut label = self.snapshot.status.branch_label();
        if let Some(what) = self.snapshot.in_progress {
            label = format!("{label} \u{00B7} {what}");
        }
        let changed = self.snapshot.status.entries.len();
        if changed > 0 {
            label = format!("{label} \u{00B7} {changed} changed");
        }
        Some(label)
    }

    /// What git thinks of a file in the explorer, so the row can be tinted by it.
    pub fn state_of(&self, path: &Path) -> Option<unluminous_git::State> {
        let relative = self.relative(path)?;
        let entry = self.snapshot.status.entry(&relative)?;
        Some(if entry.index == unluminous_git::State::Unchanged {
            entry.worktree
        } else {
            entry.index
        })
    }
}

/// How the discovered repository root relates to the project Unluminous opened.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootRelation {
    Project,
    Ancestor,
}

impl RootRelation {
    /// The stable spelling used by command-line JSON.
    pub fn name(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Ancestor => "ancestor",
        }
    }
}

/// Compare two folders after resolving aliases where the file system permits it.
fn same_folder(left: &Path, right: &Path) -> bool {
    left.canonicalize().unwrap_or_else(|_| left.to_path_buf())
        == right.canonicalize().unwrap_or_else(|_| right.to_path_buf())
}

/// Say whether a repository is the project itself or one of its ancestors.
pub fn root_relation(repository: &Path, project: &Path) -> RootRelation {
    if same_folder(repository, project) {
        RootRelation::Project
    } else {
        RootRelation::Ancestor
    }
}

/// Whether the project itself has acquired a normal or worktree git marker.
pub fn has_direct_marker(project: &Path) -> bool {
    project.join(".git").exists()
}

/// An author's first name, which is what fits in a blame column.
///
/// `Jason McAffee` becomes `Jason`, and an address becomes the part in front of the at sign, because
/// a commit made by a robot often has no name at all. A single word is left alone.
fn shorten(author: &str) -> String {
    let author = author.trim();
    if let Some((before, _)) = author.split_once('@') {
        return before.to_owned();
    }
    author.split_whitespace().next().unwrap_or(author).to_owned()
}

/// Where to look for a repository when a window opens: the folder the explorer is showing.
pub fn repository_for(folder: &Path) -> Option<PathBuf> {
    Repository::discover(folder).map(|repository| repository.root().to_path_buf())
}

impl UnluminousApp {
    /// Start working on the repository the project is in, if it is in one.
    ///
    /// Called when a window opens and again when it is pointed at another folder, because the second
    /// folder may be a different repository, or none.
    pub fn open_repository(&mut self) {
        let waker = self.thread_waker();
        self.git = GitState::open(self.tree.root(), waker);
        self.git_looked = true;
        self.files.forget_git();
    }

    /// Re-discover the repository from the open project and replace stale repository state.
    ///
    /// The project can become its own repository while this window stays open. Discovery at the
    /// moment of use prevents a status read or mutation from continuing to target an ancestor.
    pub fn refresh_repository(&mut self) -> bool {
        let discovered = unluminous_git::Repository::discover(self.tree.root());
        let unchanged = match (&self.git, &discovered) {
            (Some(current), Some(next)) => current.repository == *next,
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            return false;
        }
        let waker = self.thread_waker();
        self.git = discovered.and_then(|repository| GitState::from_repository(repository, waker));
        self.git_looked = true;
        self.files.forget_git();
        true
    }

    /// Whether repository controls should be available before the next authoritative discovery.
    pub(crate) fn repository_controls_apply(&self) -> bool {
        self.git.is_some() || git::has_direct_marker(self.tree.root())
    }

    /// Ask git what it thinks of the file that is showing, once per file.
    ///
    /// Only the change bars are asked for. Blame is not, because annotating is something a person
    /// turns on: reading the whole history of every file that is opened would be work nobody asked
    /// for, on a large file it is slow, and the column takes room the text would rather have.
    pub(crate) fn ask_git_about_the_open_file(&mut self) {
        if self.files.active().git_asked || self.git.is_none() {
            return;
        }
        let Some(path) = self.files.active().path().map(Path::to_path_buf) else {
            return;
        };
        self.files.active_mut().git_asked = true;
        if let Some(git) = self.git.as_mut() {
            if git.relative(&path).is_some() {
                git.send(unluminous_git::worker::Request::ChangedLines(path));
            }
        }
    }

    /// The path a git entry is about: the one the menu named, or the file that is open.
    fn git_target(&self, named: Option<PathBuf>) -> Option<PathBuf> {
        named.or_else(|| self.document().path().map(Path::to_path_buf))
    }

    /// Do what the Git menu asked for.
    ///
    /// Nothing here runs a git command: each arm either opens a dialog or sends a request to the
    /// worker thread, so the window never waits for git.
    pub(crate) fn run_git(&mut self, what: GitAction) {
        use unluminous_git::worker::Request;
        let target = match &what {
            GitAction::Add(path)
            | GitAction::ShowDiff(path)
            | GitAction::CompareWithRevision(path)
            | GitAction::ShowHistory(path)
            | GitAction::Rollback(path) => self.git_target(path.clone()),
            _ => self.document().path().map(Path::to_path_buf),
        };
        // Clone is the one entry that works with no repository, because it is how you get one.
        if what == GitAction::Clone {
            self.prompt = Some(Prompt::new(
                "Clone",
                &format!(
                    "Clone into a folder under {}, and open it in a window of its own.",
                    self.tree.root().display()
                ),
                "",
                "Clone",
                Purpose::Clone,
            ));
            return;
        }
        self.refresh_repository();
        let Some(git) = self.git.as_mut() else {
            self.message = Some("This folder is not in a git repository.".to_owned());
            return;
        };
        let relative = target.as_deref().and_then(|path| git.relative(path));
        match what {
            GitAction::Commit => {
                // The same entry shuts it again, because the rail's git button is this action and a
                // button that only ever opens something is a button you can press once.
                if git.panel.open {
                    git.panel.open = false;
                } else {
                    git.panel.open();
                    git.send(Request::Log { path: None, limit: git::HISTORY_LIMIT });
                }
            }
            GitAction::Add(_) => {
                if let Some(path) = relative {
                    git.send(Request::Add(vec![path]));
                }
            }
            GitAction::ShowDiff(_) => {
                if let Some(path) = target {
                    git.send(Request::Diff { path, staged: false, revision: None });
                }
            }
            GitAction::CompareWithRevision(_) => {
                if let Some(path) = target {
                    self.prompt = Some(Prompt::new(
                        "Compare with Revision",
                        "A commit, a branch or a tag to compare this file against.",
                        "HEAD~1",
                        "Compare",
                        Purpose::CompareWithRevision(path),
                    ));
                }
            }
            GitAction::ShowHistory(path) => {
                let of = path.or(target);
                git.send(Request::Log { path: of, limit: git::HISTORY_LIMIT });
                git.dialogs.open = Some(Dialog::History);
            }
            GitAction::ShowCurrentRevision => git.send(Request::ShowCommit("HEAD".to_owned())),
            GitAction::Annotate => {
                let annotated = self.files.active().blame.is_some();
                if annotated {
                    self.files.active_mut().blame = None;
                } else if let Some(path) = target {
                    git.send(Request::Blame(path));
                }
            }
            GitAction::Rollback(_) => {
                if let Some(path) = relative {
                    self.confirmation = Some(Confirmation {
                        title: "Rollback".to_owned(),
                        note: format!(
                            "Throw away the changes to {path}. They are not in a commit and not in a stash, so this cannot be undone."
                        ),
                        button: "ROLL BACK".to_owned(),
                        answer: Answer::Git(Request::Rollback(vec![path])),
                    });
                }
            }
            GitAction::Push => {
                let (status, remotes) = (git.snapshot.status.clone(), git.snapshot.remotes.clone());
                git.dialogs.open(Dialog::Push, &status, &remotes);
            }
            GitAction::Pull => {
                let (status, remotes) = (git.snapshot.status.clone(), git.snapshot.remotes.clone());
                git.dialogs.open(Dialog::Pull, &status, &remotes);
            }
            GitAction::Fetch => git.send(Request::Fetch),
            GitAction::Merge => {
                let (status, remotes) = (git.snapshot.status.clone(), git.snapshot.remotes.clone());
                git.dialogs.open(Dialog::Merge { rebase: false }, &status, &remotes);
            }
            GitAction::Rebase => {
                let (status, remotes) = (git.snapshot.status.clone(), git.snapshot.remotes.clone());
                git.dialogs.open(Dialog::Merge { rebase: true }, &status, &remotes);
            }
            GitAction::Continue => {
                let request = match git.snapshot.in_progress {
                    Some("Rebasing") => Request::ResumeRebase(unluminous_git::Resume::Continue),
                    _ => Request::ResumeMerge(unluminous_git::Resume::Continue),
                };
                git.send(request);
            }
            GitAction::Abort => {
                let request = match git.snapshot.in_progress {
                    Some("Rebasing") => Request::ResumeRebase(unluminous_git::Resume::Abort),
                    _ => Request::ResumeMerge(unluminous_git::Resume::Abort),
                };
                git.send(request);
            }
            GitAction::Branches => {
                let (status, remotes) = (git.snapshot.status.clone(), git.snapshot.remotes.clone());
                git.dialogs.open(Dialog::Branches, &status, &remotes);
            }
            // The branch flyout's row and `unluminous-cli git switch`, both reaching the request the
            // `Branches...` dialog already sends. Refused when it is the branch already checked out,
            // rather than running a git command that would do nothing and report that it had worked.
            GitAction::Switch(name) => {
                if git.snapshot.status.branch.as_deref() == Some(name.as_str()) {
                    self.message = Some(format!("Already on {name}."));
                    return;
                }
                git.send(Request::Switch(name));
            }
            GitAction::NewBranch => {
                self.prompt = Some(Prompt::new(
                    "New Branch",
                    "Start a branch here and move to it.",
                    "",
                    "Create",
                    Purpose::NewBranch,
                ));
            }
            GitAction::NewTag => {
                self.prompt = Some(Prompt::new(
                    "New Tag",
                    "Tag the commit that is checked out.",
                    "",
                    "Tag",
                    Purpose::NewTag,
                ));
            }
            GitAction::ResetHead => {
                let (status, remotes) = (git.snapshot.status.clone(), git.snapshot.remotes.clone());
                git.dialogs.open(Dialog::Reset, &status, &remotes);
                git.send(Request::Log { path: None, limit: git::HISTORY_LIMIT });
            }
            GitAction::Stash => {
                self.prompt = Some(Prompt::new(
                    "Stash Changes",
                    "Put the changes away under a message, leaving the working tree clean. Untracked files go with them.",
                    "",
                    "Stash",
                    Purpose::Stash,
                ));
            }
            GitAction::Unstash => {
                git.panel.open();
                git.panel.tab = git_panel::Tab::Stashes;
            }
            GitAction::Remotes => {
                let (status, remotes) = (git.snapshot.status.clone(), git.snapshot.remotes.clone());
                git.dialogs.open(Dialog::Remotes, &status, &remotes);
            }
            GitAction::Clone => {}
            GitAction::Exclude => {
                let exclude = git.repository.root().join(".git/info/exclude");
                let path = exclude.clone();
                if !path.is_file() {
                    let _ = std::fs::create_dir_all(path.parent().unwrap_or(&path));
                    let _ = std::fs::write(
                        &path,
                        "# Paths listed here are ignored, and this file is not committed.\n",
                    );
                }
                let _ = self.open_path_permanently(&path);
            }
            GitAction::Refresh => git.send(Request::Refresh),
        }
    }

    /// Draw the commit panel, whichever git dialog is open, and the confirmation.
    ///
    /// Returns an action when one of them asked for something that goes through `run_action`, which
    /// is how a click in the commit panel and an entry on the Git menu end up in the same place.
    pub(crate) fn show_git_windows(&mut self, ctx: &egui::Context) -> Option<Action> {
        use unluminous_git::worker::Request;
        let git = self.git.as_mut()?;
        let mut action = None;

        // The commit panel.
        let status = git.snapshot.status.clone();
        let stashes = git.snapshot.stashes.clone();
        let repository = git.repository.name();
        let recent = git.recent_messages.clone();
        let outcome = git_panel::show(ctx, &mut git.panel, &status, &stashes, &repository, &recent);
        if !outcome.stage.is_empty() {
            git.send(Request::Add(outcome.stage));
        }
        if !outcome.unstage.is_empty() {
            git.send(Request::Unstage(outcome.unstage));
        }
        if let Some(path) = outcome.show {
            git.send(Request::Diff {
                path: git.repository.root().join(&path),
                staged: false,
                revision: None,
            });
        }
        if let Some(push) = outcome.commit {
            let message = git.panel.message.clone();
            let amend = git.panel.amend;
            git.panel.message.clear();
            git.panel.amend = false;
            git.panel.open = false;
            if push {
                let target = unluminous_git::PushTarget {
                    remote: git
                        .snapshot
                        .remotes
                        .first()
                        .map(|remote| remote.name.clone())
                        .unwrap_or_else(|| "origin".to_owned()),
                    branch: status.branch.clone().unwrap_or_default(),
                    set_upstream: status.upstream.is_none(),
                    force: false,
                    tags: false,
                };
                git.send(Request::CommitAndPush { message, amend, target });
            } else {
                git.send(Request::Commit { message, amend });
            }
        }
        if let Some((name, drop)) = outcome.unstash {
            git.send(Request::Unstash { name, drop });
        }
        if let Some(name) = outcome.drop_stash {
            self.confirmation = Some(Confirmation {
                title: "Drop Stash".to_owned(),
                note: format!(
                    "Throw {name} away. What is in it is nowhere else, so this cannot be undone."
                ),
                button: "DROP".to_owned(),
                answer: Answer::Git(Request::DropStash(name)),
            });
            return action;
        }
        if outcome.refresh {
            git.send(Request::Refresh);
        }

        // The dialogs.
        let branches = git.snapshot.branches.clone();
        let remotes = git.snapshot.remotes.clone();
        let history = git.history.clone();
        let outcome =
            git_dialogs::show(ctx, &mut git.dialogs, &status, &branches, &remotes, &history);
        if let Some(target) = outcome.push {
            git.send(Request::Push(target));
            git.dialogs.close();
        }
        if let Some((remote, branch, strategy)) = outcome.pull {
            git.send(Request::Pull { remote, branch, strategy });
            git.dialogs.close();
        }
        if let Some((branch, options)) = outcome.merge {
            git.send(Request::Merge { branch, options });
            git.dialogs.close();
        }
        if let Some(branch) = outcome.rebase {
            git.send(Request::Rebase(branch));
            git.dialogs.close();
        }
        if let Some((revision, mode)) = outcome.reset {
            git.dialogs.close();
            // Only a hard reset throws work away, so only a hard reset asks first.
            if mode == unluminous_git::ResetMode::Hard {
                self.confirmation = Some(Confirmation {
                    title: "Reset HEAD".to_owned(),
                    note: format!(
                        "Move the branch to {revision} and throw away everything after it, including changes that were never committed. This cannot be undone."
                    ),
                    button: "RESET".to_owned(),
                    answer: Answer::Git(Request::Reset { revision, mode }),
                });
                return action;
            }
            git.send(Request::Reset { revision, mode });
        }
        if let Some(name) = outcome.switch {
            git.send(Request::Switch(name));
            git.dialogs.close();
        }
        if let Some(name) = outcome.delete_branch {
            self.confirmation = Some(Confirmation {
                title: "Delete Branch".to_owned(),
                note: format!(
                    "Delete {name}. Git refuses if it holds commits that are nowhere else."
                ),
                button: "DELETE".to_owned(),
                answer: Answer::Git(Request::DeleteBranch { name, force: false }),
            });
            self.git.as_mut()?.dialogs.close();
            return action;
        }
        if let Some(hash) = outcome.show_commit {
            git.send(Request::ShowCommit(hash));
        }
        if let Some((name, url)) = outcome.add_remote {
            git.send(Request::AddRemote { name, url });
            git.dialogs.remote_name.clear();
            git.dialogs.remote_url.clear();
        }
        if let Some(name) = outcome.remove_remote {
            git.send(Request::RemoveRemote(name));
        }
        if action.is_none() && self.confirmation.is_none() {
            action = None;
        }
        action
    }

    /// Send a request to the git thread, saying so when there is no repository to send it to.
    pub(crate) fn send_git(&mut self, request: unluminous_git::worker::Request) {
        self.refresh_repository();
        match self.git.as_mut() {
            Some(git) => git.send(request),
            None => self.message = Some("This folder is not in a git repository.".to_owned()),
        }
    }

    /// Where the inline code is, for a test.
    /// The links in the preview of the tab that is showing: where each one's words are and where it
    /// goes. Read by `unluminous-cli editor preview --json` and by the tests, which need it to work out
    /// where on the screen a link is rather than clicking at a guessed position.
    /// What the branch widget in the title bar needs: the branch that is checked out and the local
    /// branches to offer. Empty outside a repository, which is what makes the widget absent.
    ///
    /// The branches come from the git snapshot the worker already keeps, so this costs no git command
    /// and asks nothing on the frame it is read — a widget that ran `git branch` once a frame would run
    /// it sixty times a second.
    pub fn branch_state(&self) -> branch_widget::BranchState {
        let Some(git) = self.git.as_ref() else { return branch_widget::BranchState::default() };
        branch_widget::BranchState {
            current: git.snapshot.status.branch.clone(),
            locals: git
                .snapshot
                .branches
                .iter()
                .filter(|branch| !branch.remote)
                .map(|branch| branch.name.clone())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The relation distinguishes the project root from a repository above it.
    #[test]
    fn a_repository_root_says_whether_it_is_the_project_or_an_ancestor() {
        let project = Path::new("project");
        assert_eq!(root_relation(project, project), RootRelation::Project);
        assert_eq!(root_relation(Path::new("."), project), RootRelation::Ancestor);
    }

    #[test]
    fn a_blame_column_shows_a_first_name() {
        assert_eq!(shorten("Jason McAffee"), "Jason");
        assert_eq!(shorten("Jason"), "Jason");
        assert_eq!(shorten("  Kim Lee  "), "Kim");
        // A commit with an address where a name should be, which a robot often makes.
        assert_eq!(shorten("bot@example.com"), "bot");
        assert_eq!(shorten(""), "");
    }
}
