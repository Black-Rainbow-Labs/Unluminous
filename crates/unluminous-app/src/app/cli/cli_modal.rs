//! `modal` -- the dialogs that take the whole window's keyboard while they are open: `Find Action`,
//! `Go to File`, `Find in Files`, `Settings`, the text prompts and the confirmations. `MODALS` is
//! the one list of which ones exist at all.

use super::*;

impl UnluminousApp {
    pub(crate) fn cli_modal(
        &mut self,
        request: &Request,
        verb: &str,
        ctx: &egui::Context,
    ) -> Outcome {
        match verb {
            "list" => {
                let open = self.open_modal();
                lines(
                    request,
                    match &open {
                        Some(name) => format!("{name} is open"),
                        None => "No modal is open".to_owned(),
                    },
                    MODALS.iter().map(|(name, what)| format!("{name:<16}{what}")).collect(),
                    json!({
                        "open": open,
                        "modals": MODALS.iter().map(|(name, what)| json!({ "name": name, "summary": what })).collect::<Vec<Value>>(),
                    }),
                )
            }
            "open" => self.cli_modal_open(request),
            "state" => ok(request, self.modal_sentence(), self.modal_value(ctx)),
            "type" => self.cli_modal_type(request),
            "results" => self.cli_modal_results(request),
            "choose" => self.cli_modal_choose(request),
            "accept" => self.cli_modal_accept(request, ctx),
            "cancel" => self.cli_modal_cancel(request),
            "move" | "size" | "reset" => self.cli_modal_geometry(request, verb, ctx),
            _ => unknown(request),
        }
    }

    /// Which modal is open, by the name `modal open` takes.
    fn open_modal(&self) -> Option<String> {
        if self.palette.is_some() {
            return Some("command-palette".to_owned());
        }
        if self.go_to_file.is_some() {
            return Some("go-to-file".to_owned());
        }
        if self.find_in_files.is_some() {
            return Some("find-in-files".to_owned());
        }
        if self.settings_window.open {
            return Some("settings".to_owned());
        }
        if self.about.is_some() {
            return Some("about".to_owned());
        }
        if let Some(prompt) = &self.prompt {
            return Some(prompt_name(prompt).to_owned());
        }
        if self.new_project.is_some() {
            return Some("create-project".to_owned());
        }
        if self.background_grid.is_some() {
            return Some("background".to_owned());
        }
        if self.confirmation.is_some() {
            return Some("confirmation".to_owned());
        }
        if self.git.as_ref().is_some_and(|git| git.panel.open) {
            return Some("commit".to_owned());
        }
        if self.git.as_ref().is_some_and(|git| git.dialogs.open.is_some()) {
            return Some("git-dialog".to_owned());
        }
        None
    }

    fn modal_sentence(&self) -> String {
        match self.open_modal() {
            Some(name) => format!("{name} is open"),
            None => "No modal is open".to_owned(),
        }
    }

    pub(crate) fn modal_value(&self, ctx: &egui::Context) -> Value {
        let Some(name) = self.open_modal() else {
            return json!({ "open": Value::Null });
        };
        let id = modal_id(&name);
        let rect = id.and_then(|id| modal::drawn(ctx, id));
        let mut value = json!({
            "open": name,
            "rect": rect.map(|rect| json!({
                "x": rect.min.x, "y": rect.min.y, "width": rect.width(), "height": rect.height(),
            })),
        });
        let map = value.as_object_mut().expect("an object");
        if let Some(palette) = &self.palette {
            map.insert("query".to_owned(), json!(palette.query));
            map.insert("results".to_owned(), json!(palette.results().len()));
            map.insert("chosen".to_owned(), json!(palette.chosen));
        }
        if let Some(go) = &self.go_to_file {
            map.insert("query".to_owned(), json!(go.query));
            map.insert("results".to_owned(), json!(go.results().len()));
            map.insert("chosen".to_owned(), json!(go.chosen));
        }
        if let Some(find) = &self.find_in_files {
            map.insert("query".to_owned(), json!(find.query));
            map.insert("matchCase".to_owned(), json!(find.match_case));
            map.insert("results".to_owned(), json!(find.hits().len()));
            map.insert("chosen".to_owned(), json!(find.chosen));
            map.insert("searching".to_owned(), json!(find.is_searching()));
        }
        if let Some(about) = &self.about {
            map.insert("developer".to_owned(), json!(about.developer));
            map.insert("version".to_owned(), json!(about.version));
            map.insert("buildDate".to_owned(), json!(about.built));
        }
        if self.settings_window.open {
            map.insert("page".to_owned(), json!(self.settings_window.page.title()));
            map.insert("search".to_owned(), json!(self.settings_window.search));
        }
        if let Some(prompt) = &self.prompt {
            map.insert("title".to_owned(), json!(prompt.title));
            map.insert("note".to_owned(), json!(prompt.note));
            map.insert("query".to_owned(), json!(prompt.value));
            map.insert("confirm".to_owned(), json!(prompt.confirm));
        }
        if let Some(question) = &self.confirmation {
            map.insert("title".to_owned(), json!(question.title));
            map.insert("note".to_owned(), json!(question.note));
            map.insert("confirm".to_owned(), json!(question.button));
        }
        value
    }

    fn cli_modal_open(&mut self, request: &Request) -> Outcome {
        let Some(name) = request.text("name") else {
            return no(request, code::USAGE, "Say which modal to open.");
        };
        let query = request.text("query").unwrap_or_default();
        match name.as_str() {
            // The palette is opened over the entries the menus hold right now, which is the one
            // function `Find Action` itself calls — `task-1922` WP4.
            "command-palette" => {
                self.close_every_modal();
                self.open_the_command_palette();
                let palette = self.palette.as_mut().expect("it is open");
                palette.query = query;
                palette.refresh();
                let found = palette.results().len();
                ok(
                    request,
                    format!("Find Action is open with {found} results"),
                    json!({ "open": "command-palette", "results": found }),
                )
            }
            "go-to-file" => {
                self.close_every_modal();
                self.tree.reload();
                let mut go = GoToFile::default();
                go.query = query;
                let root = self.tree.root().to_path_buf();
                let files = self.tree.all_files().to_vec();
                go.refresh(&root, &files);
                let found = go.results().len();
                self.go_to_file = Some(go);
                ok(
                    request,
                    format!("Go to File is open with {found} results"),
                    json!({ "open": "go-to-file", "results": found }),
                )
            }
            "find-in-files" => {
                self.close_every_modal();
                self.tree.reload();
                let mut find = FindInFiles::open(self.thread_waker());
                find.query = query;
                find.match_case = request.switch("match-case");
                let files = self.tree.all_files().to_vec();
                find.pump(&files);
                self.find_in_files = Some(find);
                ok(
                    request,
                    "Find in Files is open. `modal results --wait 5000` waits for the search.",
                    json!({ "open": "find-in-files" }),
                )
            }
            "settings" => {
                self.close_every_modal();
                self.settings_window.open();
                if let Some(page) = request.text("page") {
                    let Some(chosen) = settings_page(&page) else {
                        return no(
                            request,
                            code::USAGE,
                            format!("{page} is not a Settings page. Say appearance, editor, plugins, terminal or mcp."),
                        );
                    };
                    self.settings_window.page = chosen;
                }
                ok(
                    request,
                    format!("Settings is open at {}", self.settings_window.page.title()),
                    json!({ "open": "settings", "page": self.settings_window.page.title() }),
                )
            }
            "about" => {
                self.close_every_modal();
                let about = crate::components::about_dialog::About::current();
                let answer = json!({
                    "open": "about",
                    "developer": about.developer,
                    "version": about.version,
                    "buildDate": about.built,
                });
                self.about = Some(about);
                ok(request, "About Unluminous is open", answer)
            }
            "new-file" | "rename" => self.cli_modal_open_prompt(request, &name, &query),
            // **The dialog, not the making.** `explorer new-project` is how a project is made with no
            // dialog at all; this is for driving the dialog itself, which is what every other modal
            // here is for. `--query` seeds the name, because that is what `query` means on the others.
            "background" => {
                self.close_every_modal();
                self.background_grid = Some(None);
                let pictures = self.background_names();
                ok(
                    request,
                    format!("Background is open with {} pictures", pictures.len()),
                    json!({
                        "open": "background",
                        "pictures": pictures,
                        "showing": self.settings.background_image,
                    }),
                )
            }
            "create-project" => {
                self.close_every_modal();
                let mut project =
                    crate::components::new_project_dialog::NewProject::beside(self.tree.root());
                if !query.trim().is_empty() {
                    project.name = query.clone();
                }
                if let Some(location) = request.text("path") {
                    project.location = location;
                }
                let folder = project.folder().to_string_lossy().into_owned();
                let refused = project.why_not();
                self.new_project = Some(project);
                ok(
                    request,
                    format!("Create Project is open, and would make {folder}"),
                    json!({ "open": "create-project", "folder": folder, "refused": refused }),
                )
            }
            other => no(
                request,
                code::USAGE,
                format!("There is no modal called {other}. `modal list` names them."),
            ),
        }
    }

    fn cli_modal_open_prompt(&mut self, request: &Request, name: &str, query: &str) -> Outcome {
        let path = match self.cli_path_argument(request, "path") {
            Some(path) => path,
            None if name == "rename" => match self.document().path() {
                Some(path) => path.to_path_buf(),
                None => return no(request, code::USAGE, "Say --path: which file to rename."),
            },
            None => self.tree.root().to_path_buf(),
        };
        if !path.exists() {
            return no(request, code::NOT_FOUND, format!("There is nothing at {}", path.display()));
        }
        self.close_every_modal();
        let mut prompt = match name {
            "new-file" => {
                if !path.is_dir() {
                    return no(
                        request,
                        code::USAGE,
                        format!("{} is not a folder; a new file goes in one.", path.display()),
                    );
                }
                Prompt::new(
                    "New File",
                    &format!(
                        "A new, empty file in {}. Any extension: example.txt, test.json, main.rs.",
                        crate::services::paths::the_useful_end_of(&path.display().to_string())
                    ),
                    "example.txt",
                    "Create",
                    Purpose::NewFile(path),
                )
            }
            _ => {
                let existing = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                Prompt::new(
                    "Rename",
                    &format!("Rename {}.", path.display()),
                    &existing,
                    "Rename",
                    Purpose::Rename(path),
                )
            }
        };
        if !query.is_empty() {
            prompt.value = query.to_owned();
        }
        let title = prompt.title.clone();
        let value = prompt.value.clone();
        self.prompt = Some(prompt);
        ok(
            request,
            format!("{title} is open with {value} in its box"),
            json!({ "open": name, "query": value }),
        )
    }

    fn cli_modal_type(&mut self, request: &Request) -> Outcome {
        let text = request.text("text").unwrap_or_default();
        if let Some(palette) = &mut self.palette {
            palette.query = text.clone();
            palette.refresh();
            let found = palette.results().len();
            return ok(
                request,
                format!("{found} commands match {text}"),
                json!({ "query": text, "results": found }),
            );
        }
        if let Some(go) = &mut self.go_to_file {
            go.query = text.clone();
            let root = self.tree.root().to_path_buf();
            let files = self.tree.all_files().to_vec();
            go.refresh(&root, &files);
            let found = go.results().len();
            return ok(
                request,
                format!("{found} files match {text}"),
                json!({ "query": text, "results": found }),
            );
        }
        if let Some(project) = &mut self.new_project {
            // The name, because that is the field a person types in first and `query` meant the name
            // when the dialog was opened. `--path` is the other field, for the same reason.
            project.name = text.clone();
            if let Some(location) = request.text("path") {
                project.location = location;
            }
            project.problem = None;
            let folder = project.folder().to_string_lossy().into_owned();
            let refused = project.why_not();
            return ok(
                request,
                format!("Create Project would make {folder}"),
                json!({ "name": text, "folder": folder, "refused": refused }),
            );
        }
        if let Some(find) = &mut self.find_in_files {
            find.query = text.clone();
            if request.switch("match-case") {
                find.match_case = true;
            }
            let files = self.tree.all_files().to_vec();
            find.pump(&files);
            return ok(
                request,
                format!("Searching for {text}"),
                json!({ "query": text, "searching": true }),
            );
        }
        if self.settings_window.open {
            self.settings_window.search = text.clone();
            return ok(
                request,
                format!("Searching the settings for {text}"),
                json!({ "query": text }),
            );
        }
        if let Some(prompt) = &mut self.prompt {
            prompt.value = text.clone();
            return ok(request, format!("Put {text} in the box"), json!({ "query": text }));
        }
        no(request, code::NOT_APPLICABLE, "No modal with a box in it is open.")
    }

    fn cli_modal_results(&mut self, request: &Request) -> Outcome {
        let limit = request.whole("limit").unwrap_or(50);
        if let Some(find) = &mut self.find_in_files {
            let files = self.tree.all_files().to_vec();
            find.pump(&files);
            if find.is_searching() && request.has("wait") {
                return Outcome::Hold(Waiting::ModalResults {
                    limit,
                    until: waits_for(request, "wait", DEFAULT_WAIT),
                });
            }
        }
        Outcome::Reply(self.modal_results_reply(request, limit))
    }

    /// What `modal results` answers, once there is something to answer with.
    pub(crate) fn modal_results_reply(&self, request: &Request, limit: usize) -> Reply {
        if let Some(palette) = &self.palette {
            let rows: Vec<Value> = palette
                .results()
                .iter()
                .take(limit)
                .enumerate()
                .map(|(at, row)| {
                    json!({
                        "index": at,
                        "name": row.command.name,
                        "label": row.command.label,
                        "menu": row.command.menu,
                        "shortcut": row.command.shortcut,
                        "enabled": row.command.enabled,
                    })
                })
                .collect();
            let printed: Vec<String> = palette
                .results()
                .iter()
                .take(limit)
                .enumerate()
                .map(|(at, row)| format!("{at:<4}{:<32}{}", row.command.label, row.command.menu))
                .collect();
            return Reply::done(
                &request.command,
                format!("{} commands match {}", palette.results().len(), palette.query),
                json!({
                    "results": rows,
                    "total": palette.results().len(),
                    "chosen": palette.chosen,
                    "lines": printed,
                }),
            );
        }
        if let Some(go) = &self.go_to_file {
            let rows: Vec<Value> = go
                .results()
                .iter()
                .take(limit)
                .enumerate()
                .map(|(at, found)| {
                    json!({
                        "index": at,
                        "name": found.name,
                        "folder": found.folder,
                        "path": found.path.to_string_lossy(),
                        "score": found.score,
                    })
                })
                .collect();
            let printed: Vec<String> = go
                .results()
                .iter()
                .take(limit)
                .enumerate()
                .map(|(at, found)| format!("{at:<4}{:<32}{}", found.name, found.folder))
                .collect();
            return Reply::done(
                &request.command,
                format!("{} files match {}", go.results().len(), go.query),
                json!({ "results": rows, "total": go.results().len(), "chosen": go.chosen, "lines": printed }),
            );
        }
        if let Some(find) = &self.find_in_files {
            let rows: Vec<Value> = find
                .hits()
                .iter()
                .take(limit)
                .enumerate()
                .map(|(at, hit)| {
                    json!({
                        "index": at,
                        "path": hit.path.to_string_lossy(),
                        "line": hit.line,
                        "text": hit.text,
                    })
                })
                .collect();
            let printed: Vec<String> = find
                .hits()
                .iter()
                .take(limit)
                .enumerate()
                .map(|(at, hit)| {
                    format!("{at:<4}{}:{} {}", hit.path.display(), hit.line, hit.text.trim())
                })
                .collect();
            return Reply::done(
                &request.command,
                format!(
                    "{} matches for {}{}",
                    find.hits().len(),
                    find.query,
                    if find.is_searching() { ", still searching" } else { "" }
                ),
                json!({
                    "results": rows,
                    "total": find.hits().len(),
                    "chosen": find.chosen,
                    "searching": find.is_searching(),
                    "lines": printed,
                }),
            );
        }
        Reply::failed(
            &request.command,
            code::NOT_APPLICABLE,
            "No modal with results in it is open.",
        )
    }

    fn cli_modal_choose(&mut self, request: &Request) -> Outcome {
        let Some(index) = request.whole("index") else {
            return no(request, code::USAGE, "Say which row, counting from 0.");
        };
        if let Some(palette) = &mut self.palette {
            if index >= palette.results().len() {
                return no(
                    request,
                    code::NOT_FOUND,
                    format!("There is no row {index}; there are {}.", palette.results().len()),
                );
            }
            palette.chosen = index;
            let name = palette.chosen_command().map(|command| command.name.clone());
            return ok(
                request,
                format!("Chose row {index}"),
                json!({ "chosen": index, "name": name }),
            );
        }
        if let Some(go) = &mut self.go_to_file {
            if index >= go.results().len() {
                return no(
                    request,
                    code::NOT_FOUND,
                    format!("There is no row {index}; there are {}.", go.results().len()),
                );
            }
            go.chosen = index;
            let path = go.chosen_path().map(|path| path.to_string_lossy().to_string());
            return ok(
                request,
                format!("Chose row {index}"),
                json!({ "chosen": index, "path": path }),
            );
        }
        if let Some(find) = &mut self.find_in_files {
            if index >= find.hits().len() {
                return no(
                    request,
                    code::NOT_FOUND,
                    format!("There is no row {index}; there are {}.", find.hits().len()),
                );
            }
            find.chosen = index;
            return ok(request, format!("Chose row {index}"), json!({ "chosen": index }));
        }
        no(request, code::NOT_APPLICABLE, "No modal with a list in it is open.")
    }

    fn cli_modal_accept(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        if request.has("index") {
            if let Outcome::Reply(reply) = self.cli_modal_choose(request) {
                if !reply.ok {
                    return Outcome::Reply(reply);
                }
            }
        }
        if let Some(palette) = self.palette.take() {
            let Some(command) = palette.chosen_command().cloned() else {
                self.palette = Some(palette);
                return no(
                    request,
                    code::NOT_FOUND,
                    "Nothing is chosen, so there is nothing to run.",
                );
            };
            if !command.enabled {
                // Refused with the reason rather than run, which is what the palette itself does
                // with a dimmed row — and it is left open, because being told why is the answer.
                self.palette = Some(palette);
                return no(
                    request,
                    code::NOT_APPLICABLE,
                    format!(
                        "{} cannot be used just now. It is on the {} menu.",
                        command.label, command.menu
                    ),
                );
            }
            let name = command.name.clone();
            self.run_action(command.action, ctx);
            return ok(
                request,
                self.message.clone().unwrap_or_else(|| format!("Ran {name}")),
                json!({ "ran": name, "message": self.message }),
            );
        }
        if let Some(go) = self.go_to_file.take() {
            let Some(path) = go.chosen_path() else {
                self.go_to_file = Some(go);
                return no(
                    request,
                    code::NOT_FOUND,
                    "Nothing is chosen, so there is nothing to open.",
                );
            };
            if let Err(reason) = self.open_path_permanently(&path) {
                return no(request, code::FAILED, reason);
            }
            return ok(
                request,
                format!("Opened {}", path.display()),
                json!({ "path": path.to_string_lossy(), "tab": self.files.active_index() }),
            );
        }
        if let Some(find) = self.find_in_files.take() {
            let Some(hit) = find.chosen_hit().cloned() else {
                self.find_in_files = Some(find);
                return no(
                    request,
                    code::NOT_FOUND,
                    "Nothing is chosen, so there is nothing to open.",
                );
            };
            self.open_the_match(&hit.path, hit.offset.clone());
            let at = self.caret_position();
            return ok(
                request,
                format!("Opened {} at line {}", hit.path.display(), hit.line),
                json!({
                    "path": hit.path.to_string_lossy(),
                    "line": hit.line,
                    "caret": { "line": at.line, "column": at.column },
                }),
            );
        }
        if self.settings_window.open {
            self.settings_window.open = false;
            return done(request, "Closed Settings.");
        }
        if self.about.take().is_some() {
            // There is nothing to accept in the About box, so its one button and `modal accept` do
            // the same thing the Close button does.
            return done(request, "Closed About Unluminous.");
        }
        if let Some(project) = self.new_project.take() {
            // Through `make_the_project`, which is the one place a project is made — so the dialog's
            // own button, this, and `explorer new-project` are one thing.
            if let Some(problem) = project.why_not() {
                let folder = project.folder().to_string_lossy().into_owned();
                self.new_project = Some(project);
                return no(request, code::NOT_APPLICABLE, format!("{problem} ({folder})"));
            }
            let folder = project.folder();
            return match self.make_the_project(&folder, project.git) {
                Ok(()) => ok(
                    request,
                    format!("Made {}", folder.display()),
                    json!({ "folder": folder.to_string_lossy(), "git": project.git }),
                ),
                Err(problem) => no(request, code::REFUSED, problem),
            };
        }
        if let Some(prompt) = self.prompt.take() {
            let value = prompt.value.clone();
            self.run_prompt(prompt);
            return ok(
                request,
                self.message.clone().unwrap_or_else(|| format!("Confirmed with {value}")),
                json!({ "value": value, "message": self.message }),
            );
        }
        if let Some(question) = self.confirmation.take() {
            // Through the same function the dialog's button goes through, so a question answered
            // from a script and one answered by hand are the same answer.
            let label = self.answer_the_question(question.answer);
            return ok(request, format!("Confirmed: {label}"), json!({ "confirmed": label }));
        }
        if self.git.as_ref().is_some_and(|git| git.panel.open) {
            return no(
                request,
                code::NOT_APPLICABLE,
                "The commit panel needs a message and files chosen; drive it with `git action` instead.",
            );
        }
        let _ = ctx;
        no(request, code::NOT_APPLICABLE, "No modal is open.")
    }

    fn cli_modal_cancel(&mut self, request: &Request) -> Outcome {
        let Some(name) = self.open_modal() else {
            return no(request, code::NOT_APPLICABLE, "No modal is open.");
        };
        self.close_every_modal();
        ok(request, format!("Shut {name}"), json!({ "closed": name }))
    }

    /// Shut whatever is open, which is what opening another one does and what Escape does.
    ///
    /// `pub(crate)` because `run_action` is in `app::mod`, two modules up from here rather than one,
    /// and `About Unluminous` shuts the others exactly as `modal open` does.
    pub(crate) fn close_every_modal(&mut self) {
        self.palette = None;
        self.go_to_file = None;
        self.find_in_files = None;
        self.about = None;
        self.settings_window.open = false;
        self.prompt = None;
        self.new_project = None;
        self.background_grid = None;
        self.confirmation = None;
        self.breakpoint_dialog = None;
        self.evaluate = None;
        if let Some(git) = self.git.as_mut() {
            git.panel.open = false;
            git.dialogs.open = None;
        }
    }

    fn cli_modal_geometry(
        &mut self,
        request: &Request,
        verb: &str,
        ctx: &egui::Context,
    ) -> Outcome {
        let Some(name) = self.open_modal() else {
            return no(request, code::NOT_APPLICABLE, "No modal is open.");
        };
        let Some(id) = modal_id(&name) else {
            return no(request, code::NOT_APPLICABLE, format!("{name} cannot be moved."));
        };
        let Some(rect) = modal::drawn(ctx, id) else {
            return no(
                request,
                code::NOT_APPLICABLE,
                format!("{name} has not been drawn yet, so there is nothing to move."),
            );
        };
        if verb == "reset" {
            modal::reset_placement(ctx, id);
            return done(request, format!("Put {name} back in the middle."));
        }
        let mut placement = modal::placement(ctx, id);
        let message = if verb == "move" {
            let x = request.number("x").map(|value| value as f32).unwrap_or(rect.min.x);
            let y = request.number("y").map(|value| value as f32).unwrap_or(rect.min.y);
            placement.offset += egui::Pos2::new(x, y) - rect.min;
            format!("Moved {name} to {x}, {y}")
        } else {
            let width = request.number("width").map(|value| value as f32).unwrap_or(rect.width());
            let height =
                request.number("height").map(|value| value as f32).unwrap_or(rect.height());
            placement.grown += egui::Vec2::new(width, height) - rect.size();
            format!("Made {name} {width} by {height}")
        };
        modal::set_placement(ctx, id, placement);
        ctx.request_repaint();
        done(request, message)
    }
}

/// The modals `modal open` knows, and what each one is.
const MODALS: &[(&str, &str)] = &[
    ("command-palette", "Find Action: find a menu entry by part of its wording and run it."),
    ("go-to-file", "Find a file in the project by part of its name and open it."),
    ("find-in-files", "Search every file's text, with the chosen file shown underneath."),
    (
        "settings",
        "Edit -> Settings: the font, the background, the gutter, the plugins, the terminal.",
    ),
    ("about", "Who wrote Unluminous, what version this is and when it was built."),
    ("new-file", "Make an empty file in a folder. Takes --path."),
    (
        "background",
        "Settings -> Appearance -> Behind: the grid of backgrounds. `background list`, `background use` and `background remove` do the same things with no dialog.",
    ),
    (
        "create-project",
        "File -> Create Project: a name, a location and whether to start a git repository. Takes --query for the name and --path for the location. `explorer new-project` does the whole thing with no dialog.",
    ),
    ("rename", "Rename a file or a folder. Takes --path."),
];

/// The egui id a modal is drawn under, which is what its placement is remembered against.
fn modal_id(name: &str) -> Option<&'static str> {
    Some(match name {
        "command-palette" => "unluminous-command-palette",
        "go-to-file" => "unluminous-go-to-file",
        "find-in-files" => "unluminous-find-in-files",
        "settings" => "unluminous-settings",
        "about" => "unluminous-about",
        "new-file" | "rename" | "prompt" => "unluminous-prompt",
        "create-project" => "unluminous-new-project",
        "background" => "unluminous-background",
        "confirmation" => "unluminous-confirmation",
        "commit" => "unluminous-commit",
        "git-dialog" => "unluminous-git-dialog",
        _ => return None,
    })
}

/// Which of the two prompts is open, by the name `modal open` takes.
fn prompt_name(prompt: &Prompt) -> &'static str {
    match prompt.purpose {
        Purpose::NewFile(_) => "new-file",
        Purpose::Rename(_) => "rename",
        _ => "prompt",
    }
}

fn settings_page(name: &str) -> Option<crate::settings::Page> {
    use crate::settings::Page;
    Some(match name.trim().to_lowercase().as_str() {
        "appearance" => Page::Appearance,
        "editor" => Page::Editor,
        "plugins" => Page::Plugins,
        "terminal" => Page::Terminal,
        "mcp" => Page::Mcp,
        _ => return None,
    })
}
