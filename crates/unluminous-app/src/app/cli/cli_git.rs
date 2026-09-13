//! `git` -- status, branches, and the actions the Git menu offers, all run through
//! `unluminous_git::Worker` on its own thread so a slow fetch cannot stop the window drawing.

use super::*;

impl UnluminousApp {
    pub(crate) fn cli_git(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "status" => {
                let changed = self.refresh_repository();
                if changed && self.git.as_ref().is_some_and(|git| git.is_busy()) {
                    Outcome::Hold(Waiting::Git {
                        until: waits_for(request, "wait", DEFAULT_WAIT),
                        answer: GitAnswer::GitStatus,
                    })
                } else {
                    Outcome::Reply(self.git_status_reply(request))
                }
            }
            "actions" => {
                let rows: Vec<String> =
                    GitAction::ALL.iter().map(|what| what.name().to_owned()).collect();
                lines(
                    request,
                    format!("{} entries on the Git menu", rows.len()),
                    rows.clone(),
                    json!({ "actions": rows }),
                )
            }
            "action" => self.cli_git_action(request),
            "branches" => self.cli_git_branches(request),
            "switch" => self.cli_git_switch(request),
            _ => unknown(request),
        }
    }

    /// The branches, which is what the title bar's branch button lists.
    ///
    /// Read from the snapshot the git worker already keeps rather than by running a git command, which is
    /// what the widget itself reads — so the two cannot disagree about what is checked out.
    fn cli_git_branches(&mut self, request: &Request) -> Outcome {
        self.refresh_repository();
        let Some(git) = &self.git else {
            return no(request, code::NOT_APPLICABLE, "This folder is not in a git repository.");
        };
        let current = git.snapshot.status.branch.clone();
        let rows: Vec<String> = git
            .snapshot
            .branches
            .iter()
            .map(|branch| match branch.current {
                true => format!("* {}", branch.name),
                false => format!("  {}", branch.name),
            })
            .collect();
        let branches: Vec<Value> = git
            .snapshot
            .branches
            .iter()
            .map(|branch| {
                json!({
                    "name": branch.name,
                    "current": branch.current,
                    "remote": branch.remote,
                    "upstream": branch.upstream,
                })
            })
            .collect();
        lines(
            request,
            match &current {
                Some(name) => format!("on {name}, {} branches", branches.len()),
                None => format!("{} branches, with no branch checked out", branches.len()),
            },
            rows,
            json!({ "current": current, "branches": branches }),
        )
    }

    /// Move to a branch, which is what a row of the title bar's branch flyout does.
    ///
    /// A branch this repository has no such branch for is refused **by name** rather than handed to git,
    /// because git's own message for one is about a pathspec — it reads `did not match any file(s) known
    /// to git`, which is a sentence about the wrong thing entirely for somebody who misspelled a branch.
    fn cli_git_switch(&mut self, request: &Request) -> Outcome {
        let Some(name) = request.text("branch") else {
            return no(request, code::USAGE, "Say which branch to move to.");
        };
        self.refresh_repository();
        let Some(git) = &self.git else {
            return no(request, code::NOT_APPLICABLE, "This folder is not in a git repository.");
        };
        if git.snapshot.status.branch.as_deref() == Some(name.as_str()) {
            return ok(request, format!("Already on {name}."), json!({ "branch": name }));
        }
        let known = git.snapshot.branches.iter().any(|branch| branch.name == name);
        if !known && !git.snapshot.branches.is_empty() {
            return no(
                request,
                code::NOT_FOUND,
                format!("There is no branch called {name}. `git branches` lists them."),
            );
        }
        self.run_git(GitAction::Switch(name.clone()));
        if request.has("wait") {
            return Outcome::Hold(Waiting::Git {
                until: waits_for(request, "wait", DEFAULT_WAIT),
                answer: GitAnswer::GitStatus,
            });
        }
        ok(
            request,
            format!("Asked git to move to {name}. `git status` says what came back."),
            json!({ "asked": name }),
        )
    }

    fn cli_git_action(&mut self, request: &Request) -> Outcome {
        let Some(name) = request.text("name") else {
            return no(request, code::USAGE, "Say which entry on the Git menu.");
        };
        let path = self.cli_path_argument(request, "path");
        let Some(what) = GitAction::from_name(&name, path) else {
            return no(
                request,
                code::NOT_FOUND,
                format!("There is no Git entry called {name}. `git actions` lists them."),
            );
        };
        self.refresh_repository();
        if self.git.is_none() && what != GitAction::Clone {
            return no(request, code::NOT_APPLICABLE, "This folder is not in a git repository.");
        }
        self.run_git(what);
        if request.has("wait") {
            return Outcome::Hold(Waiting::Git {
                until: waits_for(request, "wait", DEFAULT_WAIT),
                answer: GitAnswer::GitStatus,
            });
        }
        ok(
            request,
            format!("Asked git for {name}. `git status` says what came back."),
            json!({ "asked": name }),
        )
    }

    /// What git has to say, which is also what a `git action --wait` is waiting for.
    pub(crate) fn git_status_reply(&self, request: &Request) -> Reply {
        let Some(git) = &self.git else {
            return Reply::failed(
                &request.command,
                code::NOT_APPLICABLE,
                "This folder is not in a git repository.",
            );
        };
        let status = &git.snapshot.status;
        let project_root = self.tree.root();
        let relation = crate::app::git::root_relation(git.repository.root(), project_root);
        let changed: Vec<Value> = status
            .entries
            .iter()
            .map(|entry| {
                json!({
                    "path": entry.path,
                    "index": entry.index.letter().trim(),
                    "worktree": entry.worktree.letter().trim(),
                    "staged": entry.staged(),
                    "untracked": entry.untracked(),
                })
            })
            .collect();
        let printed: Vec<String> = status
            .entries
            .iter()
            .map(|entry| {
                format!("{}{} {}", entry.index.letter(), entry.worktree.letter(), entry.path)
            })
            .collect();
        let mut result = json!({
            "root": git.repository.root().to_string_lossy(),
            "projectRoot": project_root.to_string_lossy(),
            "rootRelation": relation.name(),
            "branch": status.branch,
            "upstream": status.upstream,
            "ahead": status.ahead,
            "behind": status.behind,
            "changed": changed,
            "unfinished": git.snapshot.in_progress,
            "branches": git.snapshot.branches.iter().map(|branch| branch.name.clone()).collect::<Vec<String>>(),
            "running": git.running(),
            "message": git.message,
            "lines": printed,
        });
        if let Some(map) = result.as_object_mut() {
            map.insert("annotated".to_owned(), json!(self.files.active().blame.is_some()));
        }
        Reply::done(
            &request.command,
            format!(
                "{} \u{00B7} {} changed \u{00B7} repository {}{}{}{}",
                status.branch.clone().unwrap_or_else(|| "detached".to_owned()),
                status.entries.len(),
                git.repository.root().display(),
                if relation == crate::app::git::RootRelation::Project {
                    " (project root)"
                } else {
                    " (ancestor of project "
                },
                if relation == crate::app::git::RootRelation::Project {
                    String::new()
                } else {
                    format!("{})", project_root.display())
                },
                git.message.as_ref().map(|text| format!(" \u{00B7} {text}")).unwrap_or_default()
            ),
            result,
        )
    }

    pub(crate) fn git_value(&self) -> Value {
        match &self.git {
            Some(git) => {
                let project_root = self.tree.root();
                let relation = crate::app::git::root_relation(git.repository.root(), project_root);
                json!({
                    "repository": true,
                    "root": git.repository.root().to_string_lossy(),
                    "projectRoot": project_root.to_string_lossy(),
                    "rootRelation": relation.name(),
                    "branch": git.snapshot.status.branch,
                    "changed": git.snapshot.status.entries.len(),
                    "unfinished": git.snapshot.in_progress,
                    "running": git.running(),
                    "message": git.message,
                })
            }
            None => json!({ "repository": false }),
        }
    }
}
