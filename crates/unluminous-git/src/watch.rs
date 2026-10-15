//! Whether something outside Unluminous has changed what git would say about the repository.
//!
//! `task-2274`: the window read the status once when it opened and again after each of its own Git menu
//! actions, and at no other time. A branch switched in a terminal, a commit made by an agent and a file
//! made by a build never reached the status bar, the commit panel or the explorer's tint until the
//! window was started again.
//!
//! **This asks the disk rather than git**, because it runs on the window's thread twice a second and a
//! `git` process there would be a process per tick. Everything git keeps about where the repository is
//! lives in a handful of files and folders, and every command that moves HEAD, a branch, the index or
//! a stash writes one of them: `HEAD` and `index` are rewritten, a ref is renamed into its folder from
//! a lock file, which moves that folder's modification time, and `logs/HEAD` gains a line whenever HEAD
//! moves at all. So comparing a modification time and a length for each of them is the whole check,
//! about a dozen `metadata` calls.
//!
//! **A worktree keeps its own `HEAD`, `index` and `logs/HEAD`** under the main repository's
//! `.git/worktrees/<name>`, and shares the branches with it. That is two folders, not one, and
//! `rev-parse` is the only thing that knows both, so it is asked once, when the watch is made.
//!
//! What this cannot see is a tracked file whose contents changed while nothing else did, because git
//! itself only learns that by reading the file. The window answers that half by asking git again on
//! the occasions where it matters; see `GitState::notice_outside_changes` in the window.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::command::run;

/// What one watched path looked like: its modification time and length, or nothing when it is absent.
type Seen = Option<(SystemTime, u64)>;

/// The files and folders git rewrites when the repository moves, and how each looked last time.
#[derive(Debug, Clone)]
pub struct Watch {
    paths: Vec<PathBuf>,
    seen: Vec<Seen>,
}

impl Watch {
    /// Watch the repository whose working tree is `root`.
    ///
    /// Runs one `git rev-parse`, for the two folders. A folder git will not answer for is watched as
    /// though it had nothing in it, which never reports a change; the window still reads git at the
    /// moments it asks to.
    pub fn of(root: &Path) -> Self {
        let outcome = run(root, &["rev-parse", "--absolute-git-dir", "--git-common-dir"]);
        let mut lines = outcome.stdout.lines().map(str::trim).filter(|line| !line.is_empty());
        let (own, common) = match (outcome.ok, lines.next(), lines.next()) {
            (true, Some(own), Some(common)) => {
                let own = PathBuf::from(own);
                // `--git-common-dir` answers relative to the folder it was asked in when it is the
                // ordinary `.git`, and absolute in a worktree.
                let common = PathBuf::from(common);
                let common = if common.is_absolute() { common } else { root.join(common) };
                (own, common)
            }
            _ => return Self::over(Vec::new()),
        };
        Self::over(Self::paths_for(&own, &common))
    }

    /// The paths that say the repository moved, given the folder this working tree keeps its own state
    /// in and the folder its branches are kept in. The two are one folder outside a worktree.
    pub fn paths_for(own: &Path, common: &Path) -> Vec<PathBuf> {
        let mut paths = vec![
            // A merge, a rebase or a cherry pick starting or ending adds or removes a file here.
            own.to_path_buf(),
            own.join("HEAD"),
            own.join("index"),
            own.join("logs").join("HEAD"),
        ];
        if common != own {
            paths.push(common.to_path_buf());
        }
        paths.extend([
            common.join("packed-refs"),
            common.join("refs").join("heads"),
            common.join("refs").join("remotes"),
            common.join("refs").join("stash"),
            common.join("logs").join("refs").join("stash"),
            common.join("FETCH_HEAD"),
        ]);
        paths
    }

    /// A watch over exactly these paths, taken as they are now.
    pub fn over(paths: Vec<PathBuf>) -> Self {
        let seen = paths.iter().map(|path| look(path)).collect();
        Self { paths, seen }
    }

    /// Whether any watched path has changed since the last time this was asked, or since the watch was
    /// made. Each answer takes the paths as they are now, so a change is reported once.
    pub fn changed(&mut self) -> bool {
        let mut changed = false;
        for (path, seen) in self.paths.iter().zip(self.seen.iter_mut()) {
            let now = look(path);
            if now != *seen {
                *seen = now;
                changed = true;
            }
        }
        changed
    }

    /// The paths being watched, for a test and for anybody asking why a change was or was not seen.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

fn look(path: &Path) -> Seen {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(folder: &Path, arguments: &[&str]) {
        let outcome = run(folder, arguments);
        assert!(outcome.ok, "git {arguments:?} failed: {}", outcome.message());
    }

    /// A repository with one commit, and a worktree beside it, in a folder of their own.
    fn repository(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir()
            .join(format!("unluminous-git-watch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let main = base.join("main");
        std::fs::create_dir_all(&main).unwrap();
        git(&main, &["init", "-q", "-b", "main"]);
        git(&main, &["config", "user.email", "t@example.com"]);
        git(&main, &["config", "user.name", "tester"]);
        std::fs::write(main.join("a.txt"), "one\n").unwrap();
        git(&main, &["add", "a.txt"]);
        git(&main, &["commit", "-q", "-m", "first"]);
        let tree = base.join("tree");
        git(&main, &["worktree", "add", "-q", "--detach", tree.to_str().unwrap(), "main"]);
        (main, tree)
    }

    /// Modification times on some file systems are coarse, so a change made in the same instant as the
    /// watch was taken could look like no change. A short pause keeps the tests about git rather than
    /// about the clock.
    fn pause() {
        std::thread::sleep(std::time::Duration::from_millis(30));
    }

    #[test]
    fn a_worktree_is_watched_in_its_own_folder_and_in_the_shared_one() {
        let (main, tree) = repository("paths");
        let watch = Watch::of(&tree);
        let paths: Vec<String> =
            watch.paths().iter().map(|path| path.to_string_lossy().replace('\\', "/")).collect();
        assert!(
            paths.iter().any(|path| path.contains("/.git/worktrees/") && path.ends_with("/HEAD")),
            "the worktree's own HEAD is under .git/worktrees: {paths:?}"
        );
        assert!(
            paths.iter().any(|path| path.ends_with("/main/.git/refs/heads")),
            "the branches are the main repository's: {paths:?}"
        );
        let _ = std::fs::remove_dir_all(main.parent().unwrap());
    }

    #[test]
    fn nothing_happening_is_not_a_change() {
        let (main, tree) = repository("quiet");
        let mut watch = Watch::of(&tree);
        assert!(!watch.changed());
        assert!(!watch.changed());
        let _ = std::fs::remove_dir_all(main.parent().unwrap());
    }

    #[test]
    fn a_branch_switched_outside_is_seen_in_a_worktree_and_in_a_checkout() {
        let (main, tree) = repository("switch");
        for folder in [&tree, &main] {
            let mut watch = Watch::of(folder);
            pause();
            let name = format!("feature/{}", folder.file_name().unwrap().to_string_lossy());
            git(folder, &["switch", "-q", "-c", &name]);
            assert!(watch.changed(), "a switch in {} was not seen", folder.display());
            assert!(!watch.changed(), "a change is reported once");
        }
        let _ = std::fs::remove_dir_all(main.parent().unwrap());
    }

    #[test]
    fn a_commit_and_a_stash_made_outside_are_seen() {
        let (main, tree) = repository("commit");
        let mut watch = Watch::of(&tree);
        pause();
        std::fs::write(tree.join("b.txt"), "two\n").unwrap();
        git(&tree, &["add", "b.txt"]);
        assert!(watch.changed(), "staging rewrites the index");
        pause();
        git(&tree, &["commit", "-q", "-m", "second"]);
        assert!(watch.changed(), "a commit moves HEAD");
        pause();
        std::fs::write(tree.join("a.txt"), "changed\n").unwrap();
        git(&tree, &["stash", "-q"]);
        assert!(watch.changed(), "a stash writes refs/stash");
        let _ = std::fs::remove_dir_all(main.parent().unwrap());
    }

    #[test]
    fn a_folder_that_is_not_a_repository_watches_nothing() {
        let folder =
            std::env::temp_dir().join(format!("unluminous-git-watch-none-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let mut watch = Watch::of(&folder);
        // A temporary folder can be inside a repository on some machines; what matters is that asking
        // does not fail and does not report a change nobody made.
        assert!(!watch.changed());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
