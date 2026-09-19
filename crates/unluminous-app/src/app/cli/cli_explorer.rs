//! `explorer` -- the file tree: selecting, creating, deleting, moving and reading the project's
//! files.

use super::*;

impl UnluminousApp {
    pub(crate) fn cli_explorer(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            // **Through `show_a_panel`, which is where the two rules live** (`task-1984` A9). Writing
            // `explorer_visible` here left a pane maximised beside a showing explorer, which is a
            // state no pointer can produce, and `explorer hide` with the editing area already hidden
            // left a window with nothing in it. `terminal` and `space` have always gone this way.
            "show" | "hide" | "toggle" => {
                let wanted = match verb {
                    "show" => true,
                    "hide" => false,
                    _ => !self.explorer_visible,
                };
                self.show_a_panel(crate::app::dock::Panel::Explorer, wanted);
                ok(
                    request,
                    if self.explorer_visible {
                        "The explorer is showing."
                    } else {
                        "The explorer is hidden."
                    },
                    json!({ "visible": self.explorer_visible, "width": self.panes.explorer_width }),
                )
            }
            "width" => {
                if let Some(points) = request.number("points") {
                    self.panes.explorer_width =
                        (points as f32).clamp(settings::EXPLORER_MIN, settings::EXPLORER_MAX);
                    self.unsaved_settings = true;
                }
                ok(
                    request,
                    format!("The explorer is {} points wide", self.panes.explorer_width),
                    json!({ "width": self.panes.explorer_width }),
                )
            }
            "filter" => {
                self.filter = request.text("text").unwrap_or_default();
                let matched = self.tree.matching(&self.filter).len();
                ok(
                    request,
                    if self.filter.is_empty() {
                        "The filter box is empty.".to_owned()
                    } else {
                        format!("{matched} files match {}", self.filter)
                    },
                    json!({ "filter": self.filter, "matches": matched }),
                )
            }
            "select-open-file" => {
                self.select_the_open_file();
                match self.files.active().path() {
                    Some(path) => ok(
                        request,
                        format!("The explorer is showing {}", path.display()),
                        json!({ "path": path.to_string_lossy(), "visible": self.explorer_visible }),
                    ),
                    None => no(
                        request,
                        code::NOT_APPLICABLE,
                        "The tab that is showing has never been saved, so there is no row to select.",
                    ),
                }
            }
            "new-file" => self.cli_explorer_new(request, false),
            "new-folder" => self.cli_explorer_new(request, true),
            "reload" => {
                self.tree.reload();
                ok(
                    request,
                    format!("Read {} again", self.tree.root().display()),
                    json!({ "files": self.tree.file_count() }),
                )
            }
            "select" => self.cli_explorer_select(request),
            "delete" => self.cli_explorer_delete(request),
            "move" => self.cli_explorer_move(request),
            "expand" => self.cli_explorer_expand(request),
            "collapse" => self.cli_explorer_collapse(request),
            "tree" => self.cli_explorer_tree(request),
            "files" => self.cli_explorer_files(request),
            "reveal" => match self.cli_path_argument(request, "path") {
                Some(path) if path.exists() => {
                    crate::services::launcher::reveal(&path);
                    done(request, format!("Showing {} in the file manager", path.display()))
                }
                Some(path) => {
                    no(request, code::NOT_FOUND, format!("There is nothing at {}", path.display()))
                }
                None => no(request, code::USAGE, "Say which path to show."),
            },
            _ => unknown(request),
        }
    }

    /// `explorer select` — set the row the explorer's cursor is on, or read it.
    fn cli_explorer_select(&mut self, request: &Request) -> Outcome {
        if let Some(path) = self.cli_path_argument(request, "path") {
            if !path.exists() {
                return no(
                    request,
                    code::NOT_FOUND,
                    format!("There is nothing at {}", path.display()),
                );
            }
            self.explorer_visible = true;
            self.tree.expand(&path);
            self.selected = Some(path);
            self.focus = crate::app::Focus::Explorer;
        }
        match &self.selected {
            Some(path) => ok(
                request,
                format!("Selected {}", path.display()),
                json!({ "selected": path.to_string_lossy(), "focused": self.focus == crate::app::Focus::Explorer }),
            ),
            None => ok(
                request,
                "Nothing is selected in the explorer.",
                json!({ "selected": null, "focused": self.focus == crate::app::Focus::Explorer }),
            ),
        }
    }

    /// `explorer new-file` and `explorer new-folder` — make one, with no dialog in front of it.
    ///
    /// The menu asks for a name in a prompt because a menu entry has nowhere else to get one; the
    /// command line has already been told. `task-1693` asks for the folder half, and the file half
    /// is here beside it because there was no way to make either from a script.
    ///
    /// The folders above are made too, which is what anybody typing a path with a slash in it means.
    /// A path that is already there is refused rather than emptied.
    fn cli_explorer_new(&mut self, request: &Request, directory: bool) -> Outcome {
        let what = if directory { "folder" } else { "file" };
        let Some(path) = self.cli_path_argument(request, "path") else {
            return no(request, code::USAGE, format!("Say where the {what} goes."));
        };
        if path.exists() {
            return no(
                request,
                code::NOT_APPLICABLE,
                format!("There is already something at {}", path.display()),
            );
        }
        let above = match directory {
            true => Some(path.as_path()),
            false => path.parent(),
        };
        if let Some(above) = above {
            if let Err(problem) = std::fs::create_dir_all(above) {
                return no(
                    request,
                    code::REFUSED,
                    format!("Unluminous could not make {}: {problem}", above.display()),
                );
            }
        }
        if !directory {
            if let Err(problem) = std::fs::write(&path, "") {
                return no(
                    request,
                    code::REFUSED,
                    format!("Unluminous could not make {}: {problem}", path.display()),
                );
            }
        }
        self.tree.reload();
        self.tree.expand(&path);
        // The file was made whatever happens next, so this stays an `ok` -- but whether it also
        // *opened* is a second fact and the reply says which, rather than letting a caller infer a
        // tab that is not there. `task-1804` §7.2.
        let opened = if directory { None } else { self.open_path_permanently(&path).err() };
        self.selected = Some(path.clone());
        let said = match &opened {
            None => format!("Made {}", path.display()),
            Some(reason) => format!("Made {}, and it did not open: {reason}", path.display()),
        };
        ok(
            request,
            said,
            json!({
                "path": path.to_string_lossy(),
                "directory": directory,
                "opened": !directory && opened.is_none(),
            }),
        )
    }

    /// `explorer delete` — throw a file away, with no question in front of it.
    ///
    /// The question the menu asks exists so that a click cannot destroy something by accident.
    /// Typing this command is the deliberate act, so asking again would be asking twice.
    fn cli_explorer_delete(&mut self, request: &Request) -> Outcome {
        let Some(path) = self.cli_path_argument(request, "path") else {
            return no(request, code::USAGE, "Say which path to delete.");
        };
        if !path.exists() {
            return no(request, code::NOT_FOUND, format!("There is nothing at {}", path.display()));
        }
        if path == *self.tree.root() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "The project folder itself cannot be deleted from here.",
            );
        }
        self.delete_path(&path);
        let message = self.message.clone().unwrap_or_default();
        ok(
            request,
            message,
            json!({
                "deleted": path.to_string_lossy(),
                "destination": crate::services::recycle::destination().name(),
                "gone": !path.exists(),
            }),
        )
    }

    /// `explorer move` — move a path and rewrite what names it.
    fn cli_explorer_move(&mut self, request: &Request) -> Outcome {
        let Some(path) = self.cli_path_argument(request, "path") else {
            return no(request, code::USAGE, "Say which path to move.");
        };
        let Some(folder) = self.cli_path_argument(request, "folder") else {
            return no(request, code::USAGE, "Say which folder it goes into.");
        };
        if !path.exists() {
            return no(request, code::NOT_FOUND, format!("There is nothing at {}", path.display()));
        }
        if !folder.is_dir() {
            return no(request, code::NOT_FOUND, format!("{} is not a folder.", folder.display()));
        }
        if folder == path || folder.starts_with(&path) {
            return no(request, code::NOT_APPLICABLE, "A folder cannot be moved into itself.");
        }
        let name = path.file_name().map(|name| name.to_owned()).unwrap_or_default();
        let target = folder.join(&name);
        let refactor = !request.switch("no-refactor");
        if request.switch("dry-run") {
            let started = std::time::Instant::now();
            let plan = match refactor {
                true => self.plan_a_move(&path, &target),
                false => crate::services::file_move::Plan::default(),
            };
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let files: Vec<serde_json::Value> = plan
                .files
                .iter()
                .map(|file| {
                    json!({
                        "path": file.path.to_string_lossy(),
                        "edits": file.edits.len(),
                        "changes": file
                            .edits
                            .iter()
                            .map(|(range, text)| json!({ "from": range.start, "to": range.end, "text": text }))
                            .collect::<Vec<_>>(),
                    })
                })
                .collect();
            return ok(
                request,
                format!(
                    "Would move {} to {} \u{00B7} {} \u{00B7} worked out in {elapsed:.1} ms",
                    path.display(),
                    folder.display(),
                    plan.sentence()
                ),
                json!({
                    "from": path.to_string_lossy(),
                    "to": target.to_string_lossy(),
                    "moved": plan.moved.len(),
                    "references": plan.references(),
                    "files": files,
                    "notes": plan.notes,
                    "milliseconds": elapsed,
                    "applied": false,
                }),
            );
        }
        if !self.move_path(&path, &target, refactor) {
            return no(
                request,
                code::NOT_APPLICABLE,
                self.message.clone().unwrap_or_else(|| "The move did not happen.".to_owned()),
            );
        }
        let message = self.message.clone().unwrap_or_default();
        ok(
            request,
            message,
            json!({
                "from": path.to_string_lossy(),
                "to": target.to_string_lossy(),
                "applied": true,
            }),
        )
    }

    fn cli_explorer_expand(&mut self, request: &Request) -> Outcome {
        let Some(path) = self.cli_path_argument(request, "path") else {
            return no(request, code::USAGE, "Say which folder to open.");
        };
        if !path.is_dir() {
            return no(request, code::NOT_FOUND, format!("{} is not a folder.", path.display()));
        }
        self.tree.expand(&path);
        self.explorer_visible = true;
        ok(
            request,
            format!("Opened {} in the tree", path.display()),
            json!({ "rows": self.tree.rows().len() }),
        )
    }

    fn cli_explorer_collapse(&mut self, request: &Request) -> Outcome {
        match self.cli_path_argument(request, "path") {
            Some(path) => {
                if self.tree.find(&path).is_none() {
                    return no(
                        request,
                        code::NOT_FOUND,
                        format!("{} is not in the tree.", path.display()),
                    );
                }
                if self.tree.find(&path).is_some_and(|entry| entry.expanded) {
                    self.tree.toggle(&path);
                }
                ok(
                    request,
                    format!("Shut {}", path.display()),
                    json!({ "rows": self.tree.rows().len() }),
                )
            }
            None => {
                // Deepest first, so shutting one does not take the next out of the tree before it
                // has been shut.
                let mut open = self.tree.expanded_folders();
                open.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
                let shut = open.len();
                for path in open {
                    self.tree.toggle(&path);
                }
                ok(
                    request,
                    format!("Shut {shut} folders"),
                    json!({ "closed": shut, "rows": self.tree.rows().len() }),
                )
            }
        }
    }

    fn cli_explorer_tree(&mut self, request: &Request) -> Outcome {
        let limit = request.whole("limit").unwrap_or(200);
        self.tree.reload();
        let rows = self.tree.rows();
        let shown: Vec<Value> = rows
            .iter()
            .take(limit)
            .map(|row| {
                json!({
                    "name": row.entry.name,
                    "path": row.entry.path.to_string_lossy(),
                    "depth": row.depth,
                    "folder": row.entry.is_directory,
                    "expanded": row.entry.expanded,
                    "openable": row.entry.openable,
                })
            })
            .collect();
        let printed: Vec<String> = rows
            .iter()
            .take(limit)
            .map(|row| {
                format!(
                    "{:indent$}{}{}",
                    "",
                    row.entry.name,
                    if row.entry.is_directory { "/" } else { "" },
                    indent = row.depth * 2
                )
            })
            .collect();
        lines(
            request,
            format!("{} rows, {} shown", rows.len(), shown.len()),
            printed,
            json!({ "rows": shown, "total": rows.len() }),
        )
    }

    /// Every file in the project, read from the folder rather than from what the tree last held.
    ///
    /// The folder is walked again first, because the caller is very often the thing that just wrote
    /// the file it is asking about. Unluminous reloads the tree after each of its own file operations, and
    /// nothing told it about anybody else's — so a file written a moment ago by an agent, a build or a
    /// `git checkout` was missing from the answer and from `Go to File` with it. It is the rule the
    /// index already follows for a closed file: **the disk-owned side is re-checked at the moment of
    /// use.** Measured at 20 ms over Unluminous's own repository, which is what a read from a command line
    /// can afford where a walk on every frame could not.
    fn cli_explorer_files(&mut self, request: &Request) -> Outcome {
        let limit = request.whole("limit").unwrap_or(500);
        self.tree.reload();
        let all = self.tree.all_files();
        let shown: Vec<String> =
            all.iter().take(limit).map(|path| path.to_string_lossy().to_string()).collect();
        lines(
            request,
            format!("{} files, {} shown", all.len(), shown.len()),
            shown.clone(),
            json!({ "files": shown, "total": all.len() }),
        )
    }
}
