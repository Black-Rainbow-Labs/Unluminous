//! `tab` and `pane` -- what is open in the editing area, and how the area is split.
//!
//! A pane is a column of tabs and `OpenFiles` is the model both verbs act on, so a split made from
//! a script and a split made by dragging a tab apart are the same split -- `task-1664`.

use super::*;

impl UnluminousApp {
    pub(crate) fn cli_tab(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "open" => self.cli_tab_open(request),
            "list" => {
                let rows: Vec<String> = self
                    .files
                    .iter()
                    .enumerate()
                    .map(|(at, file)| {
                        format!(
                            "{}{at:<3} {}{}",
                            if at == self.files.active_index() { "*" } else { " " },
                            file.name(),
                            if file.document.is_modified() { " (unsaved)" } else { "" }
                        )
                    })
                    .collect();
                lines(
                    request,
                    format!("{} open", self.files.len()),
                    rows,
                    json!({ "tabs": self.tabs_value(), "activeTab": self.files.active_index() }),
                )
            }
            "show" => match self.cli_find_tab(request, "tab") {
                Ok(index) => {
                    self.show_tab(index);
                    // Bringing a tab forward is a moment of use, so its file is checked too: a tab
                    // left open while something else rewrote its file is showing the old text.
                    self.reread_if_the_file_changed();
                    done(request, format!("Showing {}", self.files.active().name()))
                }
                Err(outcome) => *outcome,
            },
            "close" => self.cli_tab_close(request),
            "next" => {
                self.files.next();
                self.forget_layout();
                done(request, format!("Showing {}", self.files.active().name()))
            }
            "previous" => {
                self.files.previous();
                self.forget_layout();
                done(request, format!("Showing {}", self.files.active().name()))
            }
            "move" => self.cli_tab_move(request),
            "save" => self.cli_tab_save(request),
            "save-as" => self.cli_tab_save_as(request),
            "reload" => self.cli_tab_reload(request),
            _ => unknown(request),
        }
    }

    /// `unluminous-cli tab move <position> [--tab] [--pane]` — what dragging a tab does.
    ///
    /// It goes through `OpenFiles::drag_tab`, which is the same call the drag makes, so a
    /// rearrangement made from a script and one made with the pointer are the same rearrangement —
    /// including what `position` counts, which is the target pane's tabs as they are on the screen.
    fn cli_tab_move(&mut self, request: &Request) -> Outcome {
        let Some(position) = request.whole("position") else {
            return no(request, code::USAGE, "Say where it goes, counting from 0.");
        };
        let index = if request.has("tab") {
            match self.cli_find_tab(request, "tab") {
                Ok(index) => index,
                Err(outcome) => return *outcome,
            }
        } else {
            self.files.active_index()
        };
        let pane = request.whole("pane").unwrap_or_else(|| self.files.pane_of(index));
        if pane >= self.files.pane_count() {
            return no(
                request,
                code::NOT_FOUND,
                format!("There is no pane {pane}; there are {}.", self.files.pane_count()),
            );
        }
        let name = self.files.at(index).name();
        if !self.files.drag_tab(index, pane, position) {
            return no(request, code::NOT_APPLICABLE, format!("{name} could not be moved there."));
        }
        self.forget_layout();
        let landed = self.files.active_index();
        ok(
            request,
            format!(
                "{name} is tab {} of pane {}",
                self.files
                    .tabs_in(self.files.pane_of(landed))
                    .iter()
                    .position(|at| *at == landed)
                    .unwrap_or(0),
                self.files.pane_of(landed)
            ),
            self.panes_value(),
        )
    }

    /// `unluminous-cli pane ...` — the editing area split into panes.
    ///
    /// Every verb goes through the action or the `OpenFiles` method the menus go through, so a split
    /// made from a script and a split made by right clicking a tab are the same split.
    pub(crate) fn cli_pane(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "list" => {
                let rows: Vec<String> = (0..self.files.pane_count())
                    .map(|pane| {
                        let showing = self
                            .files
                            .showing_in(pane)
                            .map(|index| self.files.at(index).name())
                            .unwrap_or_default();
                        format!(
                            "{}{pane:<3} {:>2} tab{}  showing {showing}",
                            if pane == self.files.focused_pane() { "*" } else { " " },
                            self.files.tabs_in(pane).len(),
                            if self.files.tabs_in(pane).len() == 1 { " " } else { "s" },
                        )
                    })
                    .collect();
                let panes = self.files.pane_count();
                lines(
                    request,
                    format!("{panes} pane{}", if panes == 1 { "" } else { "s" }),
                    rows,
                    self.panes_value(),
                )
            }
            "split" => {
                self.files.split_right();
                ok(
                    request,
                    format!(
                        "Split into {} panes, showing {}",
                        self.files.pane_count(),
                        self.files.active().name()
                    ),
                    self.panes_value(),
                )
            }
            "move" => {
                let Some(direction) = request.text("direction") else {
                    return no(request, code::USAGE, "Say which way: left or right.");
                };
                let right = match direction.trim().to_ascii_lowercase().as_str() {
                    "right" => true,
                    "left" => false,
                    other => {
                        return no(
                            request,
                            code::USAGE,
                            format!("{other} is not a direction. Say left or right."),
                        )
                    }
                };
                if !self.files.move_tab(right) {
                    return no(
                        request,
                        code::NOT_APPLICABLE,
                        format!("There is no pane to the {direction} of this one."),
                    );
                }
                ok(
                    request,
                    format!(
                        "{} is in pane {}",
                        self.files.active().name(),
                        self.files.focused_pane()
                    ),
                    self.panes_value(),
                )
            }
            "focus" => {
                let Some(pane) = request.number("pane") else {
                    return no(request, code::USAGE, "Say which pane, counting from 0.");
                };
                let pane = pane.max(0.0) as usize;
                if !self.files.focus_pane(pane) {
                    return no(
                        request,
                        code::NOT_FOUND,
                        format!("There is no pane {pane}. There are {}.", self.files.pane_count()),
                    );
                }
                self.focus = crate::app::Focus::Editor;
                ok(
                    request,
                    format!("Pane {pane} has the keyboard, showing {}", self.files.active().name()),
                    self.panes_value(),
                )
            }
            "width" => {
                let (Some(pane), Some(fraction)) =
                    (request.number("pane"), request.number("fraction"))
                else {
                    return no(request, code::USAGE, "Say which pane and what share of the width.");
                };
                let pane = pane.max(0.0) as usize;
                if !self.files.set_pane_width(pane, fraction as f32) {
                    return no(
                        request,
                        code::NOT_APPLICABLE,
                        format!(
                            "There is no pane {pane} to widen. There are {}.",
                            self.files.pane_count()
                        ),
                    );
                }
                ok(
                    request,
                    format!("Pane {pane} is {fraction} of the editing area"),
                    self.panes_value(),
                )
            }
            "unsplit" | "unsplit-all" => {
                let all = verb == "unsplit-all";
                let done_it = if all { self.files.unsplit_all() } else { self.files.unsplit() };
                if !done_it {
                    return no(request, code::NOT_APPLICABLE, "The editing area is not split.");
                }
                ok(
                    request,
                    format!(
                        "{} pane{} left",
                        self.files.pane_count(),
                        if self.files.pane_count() == 1 { "" } else { "s" }
                    ),
                    self.panes_value(),
                )
            }
            _ => unknown(request),
        }
    }

    /// The panes, for `pane list` and for `status`.
    pub(crate) fn panes_value(&self) -> Value {
        json!({
            "count": self.files.pane_count(),
            "focused": self.files.focused_pane(),
            "panes": (0..self.files.pane_count())
                .map(|pane| json!({
                    "pane": pane,
                    "width": self.files.pane_widths().get(pane).copied().unwrap_or(0.0),
                    "tabs": self.files.tabs_in(pane),
                    "showing": self.files.showing_in(pane),
                    "name": self
                        .files
                        .showing_in(pane)
                        .map(|index| self.files.at(index).name()),
                }))
                .collect::<Vec<Value>>(),
        })
    }

    fn cli_tab_open(&mut self, request: &Request) -> Outcome {
        let Some(path) = self.cli_path_argument(request, "path") else {
            return no(request, code::USAGE, "Say which file to open.");
        };
        if !path.exists() {
            return no(request, code::NOT_FOUND, format!("There is no file at {}", path.display()));
        }
        if path.is_dir() {
            return no(
                request,
                code::NOT_APPLICABLE,
                format!("{} is a folder. `project open` shows a folder.", path.display()),
            );
        }
        if let Err(refusal) = file_kind::openable(&path) {
            return no(
                request,
                code::NOT_APPLICABLE,
                format!("Unluminous cannot open {}: {}", path.display(), refusal.reason()),
            );
        }
        // **The answer is what happened, not what was attempted.** `task-1804` §7.2 measured
        // the alternative: on a file that exists and cannot be decoded this replied `ok: true`,
        // `tab: 0`, exit 0, while `tab list` showed no such tab and the reason sat in the status bar
        // where no caller was going to read it. For a person that is a bug they can see; for an
        // agent it is the worst kind there is, because it is told the file is open, given a tab
        // number, and every step after it works on whatever was there before.
        let opened = if request.switch("permanent") {
            self.open_path_permanently(&path)
        } else {
            self.open_path(&path)
        };
        if let Err(reason) = opened {
            return no(request, code::FAILED, reason);
        }
        ok(
            request,
            format!("Opened {} in tab {}", path.display(), self.files.active_index()),
            json!({
                "tab": self.files.active_index(),
                "path": path.to_string_lossy(),
                "picture": self.files.active().is_picture(),
            }),
        )
    }

    fn cli_tab_close(&mut self, request: &Request) -> Outcome {
        let index = if request.has("tab") {
            match self.cli_find_tab(request, "tab") {
                Ok(index) => index,
                Err(outcome) => return *outcome,
            }
        } else {
            self.files.active_index()
        };
        let name = self.files.get(index).map(|file| file.name()).unwrap_or_default();
        // A tab is written on the way out, which is what closing one by hand does. A script that
        // means to throw the changes away has no menu to say so through, so it says so here.
        if request.switch("discard") {
            self.close_tab_without_saving(index);
        } else {
            self.close_tab(index);
        }
        ok(request, format!("Closed {name}"), json!({ "closed": name, "tabs": self.files.len() }))
    }

    fn cli_tab_save(&mut self, request: &Request) -> Outcome {
        if self.files.active().is_browser() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "A browser tab has no editable source. Use browser reload to reload it.",
            );
        }
        if self.files.active().is_picture() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "A picture cannot be edited, so there is nothing to save.",
            );
        }
        if self.files.active().path().is_none() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "This tab has never been saved. Use `tab save-as <path>`.",
            );
        }
        self.save();
        match self.files.active().document.is_modified() {
            false => {
                let path =
                    self.files.active().path().map(|p| p.display().to_string()).unwrap_or_default();
                ok(request, format!("Saved {path}"), json!({ "path": path }))
            }
            true => no(
                request,
                code::FAILED,
                "Unluminous could not write the file. The status bar says why.",
            ),
        }
    }

    fn cli_tab_save_as(&mut self, request: &Request) -> Outcome {
        let Some(path) = self.cli_path_argument(request, "path") else {
            return no(request, code::USAGE, "Say where to write it.");
        };
        if self.files.active().is_picture() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "A picture cannot be edited, so there is nothing to save.",
            );
        }
        if self.files.active().is_browser() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "A browser tab has no editable source to save.",
            );
        }
        match self.files.active_mut().document.save_as(&path) {
            Ok(()) => {
                self.tree.reload();
                ok(
                    request,
                    format!("Saved {}", path.display()),
                    json!({ "path": path.to_string_lossy() }),
                )
            }
            Err(problem) => {
                no(request, code::FAILED, format!("Could not write {}: {problem}", path.display()))
            }
        }
    }

    fn cli_tab_reload(&mut self, request: &Request) -> Outcome {
        if self.files.active().is_browser() {
            return no(request, code::NOT_APPLICABLE, "Use `browser reload` for a rendered page.");
        }
        let Some(path) = self.files.active().path().map(Path::to_path_buf) else {
            return no(request, code::NOT_APPLICABLE, "This tab has never been saved.");
        };
        let discard = request.switch("discard");
        if !discard && self.document().is_modified() {
            return no(
                request,
                code::NOT_APPLICABLE,
                format!(
                    "{} has unsaved changes, so it was not reloaded. Save it first, or say \
                     --discard to throw them away.",
                    path.display()
                ),
            );
        }
        if self.reload_from_disk(&path, discard) {
            ok(
                request,
                format!("Read {} again", path.display()),
                json!({ "path": path.to_string_lossy(), "discarded": discard }),
            )
        } else {
            no(
                request,
                code::FAILED,
                self.message
                    .clone()
                    .unwrap_or_else(|| format!("Could not reload {}", path.display())),
            )
        }
    }

    /// The tab an argument names: its number, its name, or its path.
    ///
    /// The error is boxed because `Outcome` carries the whole of `Waiting`, which is large enough
    /// that clippy flags an unboxed `Err` here as bloating every `Ok` return alongside it.
    fn cli_find_tab(&self, request: &Request, name: &str) -> Result<usize, Box<Outcome>> {
        let Some(text) = request.text(name) else {
            return Err(Box::new(no(request, code::USAGE, "Say which tab.")));
        };
        if let Ok(index) = text.trim().parse::<usize>() {
            return if index < self.files.len() {
                Ok(index)
            } else {
                Err(Box::new(no(
                    request,
                    code::NOT_FOUND,
                    format!("There is no tab {index}; there are {}.", self.files.len()),
                )))
            };
        }
        let wanted = self.cli_path(&text);
        let found = self
            .files
            .iter()
            .position(|file| file.path() == Some(wanted.as_path()) || file.name() == text);
        found.ok_or_else(|| {
            Box::new(no(request, code::NOT_FOUND, format!("No tab is showing {text}.")))
        })
    }

    pub(crate) fn tabs_value(&self) -> Value {
        json!(self
            .files
            .iter()
            .enumerate()
            .map(|(at, file)| json!({
                "index": at,
                "name": file.name(),
                "path": file.path().map(|path| path.to_string_lossy()),
                "modified": file.document.is_modified(),
                "picture": file.is_picture(),
                "browser": file.is_browser(),
                "transient": file.transient,
                "viewMode": view_mode_name(file.view_mode),
                "pane": file.home.pane(),
                // Which canvas node this tab lives in, when it lives on one rather than in a
                // pane - `task-1904`. Absent for an ordinary tab, which is what a reader of this
                // answer already means by "in the editing area".
                "node": file.home.node(),
            }))
            .collect::<Vec<Value>>())
    }
}
