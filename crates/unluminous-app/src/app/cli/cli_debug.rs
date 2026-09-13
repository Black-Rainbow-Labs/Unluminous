//! `debug` -- breakpoints, stepping, the call stack, and the value under the pointer, all spoken
//! over the Debug Adapter Protocol by `unluminous-dap`. `task-1687` is the design and `task-1689`
//! the implementation.

use super::*;

impl UnluminousApp {
    /// What a breakpoint command did, in the past tense, so its sentence reads like one.
    ///
    /// Here rather than four `format!`s at the call site: the four verbs differ only in this word,
    /// and four sentences that almost agreed would be four chances for one of them to read oddly.
    fn breakpoint_verbed(action: &str) -> &'static str {
        match action {
            "add" => "Breakpoint on",
            "remove" => "Breakpoint removed from",
            "enable" => "Breakpoint enabled on",
            _ => "Breakpoint disabled on",
        }
    }

    /// How long a stepping verb waits for the program to stop again.
    ///
    /// Longer than [`DEFAULT_WAIT`], because a `continue` runs to the next breakpoint and how long
    /// that takes is a fact about the program rather than about Unluminous.
    ///
    /// **The number is the catalogue's**, because the client has to know it: with no explicit
    /// `--timeout` a client waited fifteen seconds while the window was still correctly waiting
    /// thirty, and reported a timeout for something that was about to work. `task-1922` B11.
    const DEBUG_WAIT: Duration = Duration::from_millis(unluminous_cli::catalogue::DEBUG_WAIT_MS);

    /// How long `debug start --wait-for-pause` waits when a locator has to build the program first.
    ///
    /// Ten minutes, because a cold `cargo build` of a real workspace is minutes and a caller that
    /// gave up after thirty seconds would report a failure of a build that was working perfectly.
    /// The same reasoning as [`Self::DEBUG_WAIT`], one step further out, and from the same place.
    const BUILD_WAIT: Duration = Duration::from_millis(unluminous_cli::catalogue::BUILD_WAIT_MS);

    pub(crate) fn cli_debug(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "start" => self.cli_debug_start(request),
            "stop" => self.cli_debug_stop(request),
            "continue" => self.cli_debug_step(request, unluminous_dap::Step::Resume),
            "step-over" => self.cli_debug_step(request, unluminous_dap::Step::Over),
            "step-into" => self.cli_debug_step(request, unluminous_dap::Step::Into),
            "step-out" => self.cli_debug_step(request, unluminous_dap::Step::Out),
            "run-to" => self.cli_debug_run_to(request),
            "breakpoint" => self.cli_debug_breakpoint(request),
            "frames" => self.cli_debug_frames(request),
            "variables" => self.cli_debug_variables(request),
            "set-value" => self.cli_debug_set_value(request),
            "set-expression" => self.cli_debug_set_expression(request),
            "hover" => self.cli_debug_hover(request),
            "evaluate" => self.cli_debug_evaluate(request),
            "watch" => self.cli_debug_watch(request),
            "output" => self.cli_debug_output(request),
            "status" => self.cli_debug_status(request),
            "adapters" => self.cli_debug_adapters(request),
            "install" => self.cli_debug_install(request),
            _ => unknown(request),
        }
    }

    fn cli_debug_start(&mut self, request: &Request) -> Outcome {
        let named = request.text("name");
        if let Some(name) = &named {
            if self.configuration_named(Some(name)).is_none() {
                return no(
                    request,
                    code::NOT_FOUND,
                    format!("There is no run configuration called {name}."),
                );
            }
        } else if self.run_selected.is_none() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "No run configuration is chosen. `run select <name>` chooses one.",
            );
        }
        self.message = None;
        self.debug_a_configuration(DebugAction::Start(named));
        // A session that could not be started said why, and `debug` holds nothing — which is a
        // refusal rather than something to wait for.
        let Some(said) = self.message.clone() else {
            return ok(request, "Debugging", self.debug_state_value());
        };
        // Unless a locator is building the program first, which is neither: `cargo run` has no
        // session yet and has not failed. `task-1692`.
        if self.debug.is_none() && self.debug_build.is_none() {
            return no(request, code::NOT_APPLICABLE, said);
        }
        self.wait_for_a_pause(request, said)
    }

    fn cli_debug_stop(&mut self, request: &Request) -> Outcome {
        if self.debug.is_none() {
            return no(request, code::NOT_APPLICABLE, "Nothing is being debugged.");
        }
        self.debug_a_configuration(DebugAction::Stop);
        ok(request, "Stopping the debugger", self.debug_state_value())
    }

    /// The four that ask the program to go on. One arm rather than four, because all four go through
    /// `UnluminousApp::step`, which is the one place a stepping request turns into a change — so a thing
    /// done from a script and the same thing done from the tile are the same thing.
    fn cli_debug_step(&mut self, request: &Request, step: unluminous_dap::Step) -> Outcome {
        let Some(debug) = self.debug.as_ref() else {
            return no(request, code::NOT_APPLICABLE, "Nothing is being debugged.");
        };
        if !debug.is_paused() {
            return no(request, code::NOT_APPLICABLE, "The program is not stopped.");
        }
        self.message = None;
        self.step(step);
        let said = self.message.clone().unwrap_or_else(|| step.label().to_owned());
        self.wait_for_a_pause(request, said)
    }

    fn cli_debug_run_to(&mut self, request: &Request) -> Outcome {
        let Some(path) = request.text("path") else {
            return no(request, code::USAGE, "Say which file.");
        };
        let Some(line) = request.whole("line").filter(|line| *line > 0) else {
            return no(request, code::USAGE, "Say which line, counting from 1.");
        };
        if self.debug.as_ref().is_none_or(|debug| !debug.is_paused()) {
            return no(request, code::NOT_APPLICABLE, "The program is not stopped.");
        }
        let path = self.cli_path(&path);
        // The caret is moved to the line first, and then the ordinary `Run to Cursor` runs — so the
        // command line and the menu entry are the same path rather than two that have to agree.
        if let Err(reason) = self.open_path_permanently(&path) {
            // Everything below places a caret in the file that was opened and then runs to it.
            // Unguarded it would run to a line of whatever tab happened to be showing.
            return no(request, code::FAILED, reason);
        }
        let offset = self.document().offset_of_line_number(line);
        self.document_mut().apply(unluminous_core::Command::PlaceCaret { offset, extend: false });
        self.message = None;
        self.debug_a_configuration(DebugAction::RunToCursor);
        let said =
            self.message.clone().unwrap_or_else(|| format!("Running to {}:{line}", path.display()));
        self.wait_for_a_pause(request, said)
    }

    /// Answer now, or hold the request until the program stops — which is what `--wait-for-pause` is
    /// and what makes the whole feature scriptable.
    fn wait_for_a_pause(&mut self, request: &Request, said: String) -> Outcome {
        if !request.switch("wait-for-pause") {
            return ok(request, said, self.debug_state_value());
        }
        if self.debug.as_ref().is_some_and(DebugState::is_ready) {
            return ok(request, said, self.debug_pause_value());
        }
        // A locator's build comes first, and a cold `cargo build` of a real project is minutes
        // rather than seconds — so the wait is the build's, not a step's. A caller that says
        // `--timeout` still gets exactly what it asked for.
        let usual = match self.debug_build.is_some() {
            true => Self::BUILD_WAIT,
            false => Self::DEBUG_WAIT,
        };
        Outcome::Hold(Waiting::DebugPause {
            command: request.command.clone(),
            until: waits_for(request, "timeout", usual),
        })
    }

    fn cli_debug_breakpoint(&mut self, request: &Request) -> Outcome {
        let Some(action) = request.text("action") else {
            return no(request, code::USAGE, "Say add, remove, enable, disable, list or clear.");
        };
        match action.as_str() {
            "list" => self.cli_breakpoint_list(request),
            "clear" => {
                let cleared = self.clear_every_breakpoint();
                ok(request, format!("{cleared} breakpoints cleared"), self.breakpoints_value())
            }
            "add" | "remove" | "enable" | "disable" => {
                let Some(path) = request.text("path") else {
                    return no(request, code::USAGE, "Say which file.");
                };
                let Some(line) = request.whole("line").filter(|line| *line > 0) else {
                    return no(request, code::USAGE, "Say which line, counting from 1.");
                };
                let path = self.cli_path(&path);
                if !self.debug_applies_to(Some(&path)) {
                    return no(
                        request,
                        code::NOT_APPLICABLE,
                        format!(
                            "{}'s language has not said which debugger to use.",
                            path.display()
                        ),
                    );
                }
                let condition = request.text("condition");
                let log = request.text("log");
                let offset = self.offset_of_line(&path, line);
                let changed = match action.as_str() {
                    "add" => self.change_breakpoints(&path, |breakpoints| {
                        breakpoints.set(unluminous_core::Breakpoint {
                            offset,
                            enabled: true,
                            condition,
                            log_message: log,
                        });
                    }),
                    "remove" => self.change_breakpoints(&path, |breakpoints| {
                        breakpoints.remove_at(offset);
                    }),
                    on => {
                        let enabled = on == "enable";
                        self.change_breakpoints(&path, |breakpoints| {
                            breakpoints.set_enabled(offset, enabled);
                        })
                    }
                };
                if !changed && action == "remove" {
                    return no(
                        request,
                        code::NOT_FOUND,
                        format!("There is no breakpoint on line {line} of {}.", path.display()),
                    );
                }
                self.send_the_breakpoints_of(&path);
                ok(
                    request,
                    format!(
                        "{} line {line} of {}",
                        Self::breakpoint_verbed(&action),
                        path.display()
                    ),
                    self.breakpoints_value(),
                )
            }
            other => no(
                request,
                code::USAGE,
                format!("`{other}` is not one of add, remove, enable, disable, list or clear."),
            ),
        }
    }

    fn cli_breakpoint_list(&mut self, request: &Request) -> Outcome {
        let root = self.tree.root().to_path_buf();
        let mut rows: Vec<String> = Vec::new();
        for (path, breakpoints) in self.every_breakpoint() {
            let shown = crate::services::project_state::relative(&root, &path);
            for breakpoint in breakpoints.iter() {
                let line = self.offset_line_number(&path, breakpoint.offset);
                // What the debugger said, while one is running. `-` rather than a guess when there
                // is nobody to have said anything, which is the honesty rule the gutter keeps too.
                let verified = match self
                    .debug
                    .as_ref()
                    .and_then(|debug| debug.verified(&path, breakpoint.offset))
                {
                    Some(answered) => match answered.verified {
                        true => "verified",
                        false => "unverified",
                    },
                    None => "-",
                };
                rows.push(format!(
                    "{}{}:{line:<6} {:<11} {}",
                    if breakpoint.enabled { " " } else { "-" },
                    shown.display(),
                    verified,
                    breakpoint.condition.clone().unwrap_or_default(),
                ));
            }
        }
        let count = rows.len();
        lines(request, format!("{count} breakpoints"), rows, self.breakpoints_value())
    }

    fn cli_debug_frames(&mut self, request: &Request) -> Outcome {
        let Some(debug) = self.debug.as_ref() else {
            return no(request, code::NOT_APPLICABLE, "Nothing is being debugged.");
        };
        let rows: Vec<String> = debug
            .frames
            .iter()
            .filter(|frame| request.switch("include-subtle") || !frame.subtle)
            .map(|frame| match &frame.path {
                Some(path) => format!("{:<28} {path}:{}", frame.name, frame.line),
                None => frame.name.clone(),
            })
            .collect();
        let count = rows.len();
        lines(
            request,
            format!("{count} frames"),
            rows,
            self.debug_frames_value(request.switch("include-subtle")),
        )
    }

    fn cli_debug_variables(&mut self, request: &Request) -> Outcome {
        if self.debug.is_none() {
            return no(request, code::NOT_APPLICABLE, "Nothing is being debugged.");
        }
        // A frame was named, so the variables are read from that one. The whole of the read is
        // through `show_frame`, which is what the tile's own click goes through.
        if let Some(index) = request.whole("frame") {
            let frame =
                self.debug.as_ref().and_then(|debug| debug.frames.get(index).map(|frame| frame.id));
            let Some(frame) = frame else {
                return no(request, code::NOT_FOUND, format!("There is no frame {index}."));
            };
            if let Some(debug) = self.debug.as_mut() {
                debug.show_frame(frame);
            }
        }
        // Opening a row asks the debugger for its children, which is the tile's own laziness: the
        // answer arrives on a later frame, so the row is printed as it is now and again next time.
        if let Some(key) = request.text("expand") {
            if let Some(debug) = self.debug.as_mut() {
                debug.toggle_row(&key);
            }
        }
        let debug = self.debug.as_ref().expect("still there");
        if !debug.is_paused() {
            return no(request, code::NOT_APPLICABLE, "The program is not stopped.");
        }
        let rows: Vec<String> = debug
            .rows
            .iter()
            .map(|row| {
                let indent = "  ".repeat(row.depth);
                match (&row.kind, row.is_scope) {
                    (_, true) => format!("{indent}{}", row.name),
                    (Some(kind), _) => format!("{indent}{}: {kind} = {}", row.name, row.value),
                    (None, _) => format!("{indent}{} = {}", row.name, row.value),
                }
            })
            .collect();
        let count = rows.len();
        lines(request, format!("{count} rows"), rows, self.debug_variables_value())
    }

    fn cli_debug_set_value(&mut self, request: &Request) -> Outcome {
        let Some(key) = request.text("path") else {
            return no(request, code::USAGE, "Say which row, as `debug variables` names it.");
        };
        let Some(value) = request.text("value") else {
            return no(request, code::USAGE, "Say what to set it to.");
        };
        let Some(debug) = self.debug.as_mut() else {
            return no(request, code::NOT_APPLICABLE, "Nothing is being debugged.");
        };
        match debug.set_value(&key, &value) {
            Ok(()) => {
                ok(request, format!("Setting {key} to {value}"), self.debug_variables_value())
            }
            Err(problem) => no(request, code::NOT_APPLICABLE, problem),
        }
    }

    /// `debug hover`: what a person sees when they rest the pointer on a name.
    ///
    /// **The half that is new is the position.** `debug evaluate` already asks the debugger about a
    /// string somebody typed; what this adds is `unluminous_core::expressions`' reading — an agent that
    /// has just been told the program stopped at `src/main.rs:42` can ask what the third word on
    /// that line holds without first working out what that word is. It goes through the same
    /// function the popup and the menu entry do, so the three cannot come to different conclusions.
    ///
    /// It is also the only way to walk into a value that was reached by an expression: `debug
    /// evaluate` answers `expandable: true` and there is nothing to expand it with. `--expand` is
    /// that, naming a row the way `debug variables --expand` already does.
    fn cli_debug_hover(&mut self, request: &Request) -> Outcome {
        if self.debug.as_ref().is_none_or(|debug| !debug.is_paused()) {
            return no(request, code::NOT_APPLICABLE, "The program is not stopped.");
        }
        // Named outright, which is what makes a value from `debug evaluate` expandable at all.
        let wanted = match request.text("expression") {
            Some(expression) => expression,
            None => {
                let offset = match self.cli_offset(request) {
                    Ok(offset) => offset,
                    Err(problem) => return no(request, code::USAGE, problem),
                };
                let index = self.files.active_index();
                let Some(range) = self.expression_at(index, offset) else {
                    return no(
                        request,
                        code::NOT_FOUND,
                        "There is no name there to ask about. A keyword, a number, an operator and anything inside a comment or a string are all nothing to a debugger.",
                    );
                };
                self.files.at(index).document.text().byte_slice(range).to_string()
            }
        };
        // Already open on this expression, so it is not asked again: `--expand` on the row it is
        // already showing is the commonest second call there is.
        let asking = self
            .debug
            .as_ref()
            .and_then(|debug| debug.hover.as_ref())
            .is_none_or(|hover| hover.expression != wanted);
        if let Some(debug) = self.debug.as_mut() {
            if asking {
                debug.ask_the_hover(&wanted);
            }
        }
        // A question that was already open can be expanded now; one that has just been asked has
        // no tree yet, so the row is opened once the answer arrives — see the wait below.
        let mut expand = request.text("expand");
        if !asking {
            if let Some(key) = expand.take() {
                if let Some(debug) = self.debug.as_mut() {
                    debug.toggle_hover_row(&key);
                }
            }
        }
        let id = self
            .debug
            .as_ref()
            .and_then(|debug| debug.hover.as_ref())
            .map(|hover| hover.id)
            .unwrap_or_default();
        if expand.is_none() && self.debug.as_ref().is_some_and(DebugState::hover_is_ready) {
            return Outcome::Reply(self.hover_reply(request));
        }
        Outcome::Hold(Waiting::DebugHover {
            id,
            expand,
            until: waits_for(request, "timeout", DEFAULT_WAIT),
        })
    }

    /// The value tooltip as it stands, in the shape `debug variables` prints a tree in.
    pub(crate) fn hover_reply(&self, request: &Request) -> Reply {
        let Some(hover) = self.debug.as_ref().and_then(|debug| debug.hover.as_ref()) else {
            return Reply::failed(
                &request.command,
                code::NOT_APPLICABLE,
                "Nothing is being debugged.",
            );
        };
        if let Some(said) = hover.refusal() {
            return Reply::failed(&request.command, code::NOT_APPLICABLE, said);
        }
        let rows: Vec<String> = hover
            .rows
            .iter()
            .map(|row| {
                let indent = "  ".repeat(row.depth);
                match &row.kind {
                    Some(kind) => format!("{indent}{}: {kind} = {}", row.name, row.value),
                    None => format!("{indent}{} = {}", row.name, row.value),
                }
            })
            .collect();
        let root = hover.rows.first();
        let said = match root {
            Some(row) => format!("{} = {}", hover.expression, row.value),
            None => format!("{} has no value here", hover.expression),
        };
        let mut result = Map::new();
        result.insert("expression".to_owned(), json!(hover.expression));
        result.insert("value".to_owned(), json!(root.map(|row| row.value.clone())));
        result.insert("type".to_owned(), json!(root.and_then(|row| row.kind.clone())));
        result.insert(
            "rows".to_owned(),
            json!(hover
                .rows
                .iter()
                .map(|row| json!({
                    "path": row.key,
                    "depth": row.depth,
                    "name": row.name,
                    "value": row.value,
                    "type": row.kind,
                    "expandable": row.has_children(),
                    "expanded": row.expanded,
                }))
                .collect::<Vec<_>>()),
        );
        result.insert("lines".to_owned(), json!(rows));
        Reply::done(&request.command, said, Value::Object(result))
    }

    /// `debug set-expression`: assign to whatever an expression names.
    ///
    /// The other half of `debug set-value`, and each is used where it is the only one that can do
    /// the job — see `DebugState::set_hover_value`.
    fn cli_debug_set_expression(&mut self, request: &Request) -> Outcome {
        let Some(expression) = request.text("expression") else {
            return no(request, code::USAGE, "Say which expression to assign to.");
        };
        let Some(value) = request.text("value") else {
            return no(request, code::USAGE, "Say what to set it to.");
        };
        let Some(debug) = self.debug.as_mut() else {
            return no(request, code::NOT_APPLICABLE, "Nothing is being debugged.");
        };
        match debug.set_expression(&expression, &value) {
            Ok(()) => ok(
                request,
                format!("Setting {expression} to {value}"),
                self.debug_variables_value(),
            ),
            Err(problem) => no(request, code::NOT_APPLICABLE, problem),
        }
    }

    fn cli_debug_evaluate(&mut self, request: &Request) -> Outcome {
        let Some(expression) = request.text("expression") else {
            return no(request, code::USAGE, "Say what to evaluate.");
        };
        let Some(debug) = self.debug.as_mut() else {
            return no(request, code::NOT_APPLICABLE, "Nothing is being debugged.");
        };
        if !debug.is_paused() {
            return no(request, code::NOT_APPLICABLE, "The program is not stopped.");
        }
        debug.evaluate(&expression);
        let id = debug.evaluated.as_ref().map(|(id, _, _)| *id).unwrap_or_default();
        Outcome::Hold(Waiting::DebugEvaluate {
            expression,
            id,
            until: waits_for(request, "timeout", DEFAULT_WAIT),
        })
    }

    fn cli_debug_watch(&mut self, request: &Request) -> Outcome {
        let Some(action) = request.text("action") else {
            return no(request, code::USAGE, "Say add, remove or list.");
        };
        let Some(debug) = self.debug.as_mut() else {
            return no(request, code::NOT_APPLICABLE, "Nothing is being debugged.");
        };
        match action.as_str() {
            "list" => {
                let rows: Vec<String> = debug
                    .watches
                    .iter()
                    .map(|watch| {
                        let said = match &watch.result {
                            Some(Ok(value)) => value.value.clone(),
                            Some(Err(problem)) => problem.clone(),
                            None => "\u{2014}".to_owned(),
                        };
                        format!("{:<28} {said}", watch.expression)
                    })
                    .collect();
                let count = rows.len();
                lines(request, format!("{count} watches"), rows, self.debug_watches_value())
            }
            "add" | "remove" => {
                let Some(expression) = request.text("expression") else {
                    return no(request, code::USAGE, "Say which expression.");
                };
                match action.as_str() {
                    "add" => {
                        debug.add_watch(&expression);
                        ok(request, format!("Watching {expression}"), self.debug_watches_value())
                    }
                    _ => match debug.remove_watch(&expression) {
                        true => ok(
                            request,
                            format!("{expression} is no longer watched"),
                            self.debug_watches_value(),
                        ),
                        false => no(
                            request,
                            code::NOT_FOUND,
                            format!("{expression} is not being watched."),
                        ),
                    },
                }
            }
            other => {
                no(request, code::USAGE, format!("`{other}` is not one of add, remove or list."))
            }
        }
    }

    /// What the adapter itself has said, which is the one place a debugger's own explanation lands.
    ///
    /// **Not the program's output** — that goes to the run tile through `runInTerminal` and is read
    /// with `run output`. This is the adapter talking about itself: what it loaded, what it could not
    /// find, and the traceback it printed instead of answering. Nothing here is invented; it is
    /// carried whole, which is `unluminous-git`'s rule about git's standard error.
    fn cli_debug_output(&mut self, request: &Request) -> Outcome {
        let Some(debug) = self.debug.as_ref() else {
            // What was said while there was no session to hold it, which is a failed build's
            // compiler errors — the reason a session never started belongs where somebody looking
            // for the reason will look. `task-1692`.
            if !self.debug_output.is_empty() {
                let rows = self.debug_output.clone();
                let count = rows.len();
                return lines(request, format!("{count} lines"), rows, Value::Null);
            }
            return no(request, code::NOT_APPLICABLE, "Nothing is being debugged.");
        };
        let mut rows: Vec<String> = debug
            .output
            .iter()
            .flat_map(|line| line.lines().map(str::to_owned).collect::<Vec<String>>())
            .collect();
        if let Some(tail) = request.whole("tail") {
            let from = rows.len().saturating_sub(tail);
            rows = rows[from..].to_vec();
        }
        let count = rows.len();
        lines(request, format!("{count} lines"), rows, Value::Null)
    }

    /// `debug adapters` — the doctor, and the one command to run before any of the others.
    ///
    /// It exists because the alternative was reading `services/debuggers.rs`, which is what
    /// `task-1692`'s first sentence describes somebody doing. Every debugger this version drives,
    /// where each one really is on this machine, what is missing, and the command that would install
    /// it — in fields under `--json`, because an agent deciding between "start a session" and "tell
    /// the person to install something" should not have to read prose to do it.
    fn cli_debug_adapters(&mut self, request: &Request) -> Outcome {
        self.forget_the_adapter_search();
        let names: Vec<&str> = debuggers::ALL.iter().map(|entry| entry.name).collect();
        let mut rows: Vec<String> = Vec::new();
        let mut adapters: Vec<Value> = Vec::new();
        let mut found = 0;
        for name in names {
            let report = self.adapter_report(name);
            let where_it_is = match &report.found {
                Some(path) => path.display().to_string(),
                None => String::new(),
            };
            match report.found.is_some() {
                true => {
                    found += 1;
                    rows.push(format!("{:<8} found    {where_it_is}", report.name));
                }
                false => rows.push(format!("{:<8} missing  {}", report.name, report.comes_from)),
            }
            if !report.languages.is_empty() {
                rows.push(format!("{:<8} used by  {}", "", report.languages.join(", ")));
            }
            rows.push(format!("{:<8} looked   {}", "", report.programs.join(", ")));
            if report.found.is_none() && !report.install.is_empty() {
                rows.push(format!("{:<8} install  {}", "", report.install));
            }
            if !report.caveat.is_empty() {
                rows.push(format!("{:<8} note     {}", "", report.caveat));
            }
            adapters.push(json!({
                "name": report.name,
                "found": report.found.is_some(),
                "path": where_it_is,
                "configured": report.configured,
                "programs": report.programs,
                "languages": report.languages,
                "comes_from": report.comes_from,
                "install": report.install,
                "settings_key": report.settings_key,
                "caveat": report.caveat,
            }));
        }
        let total = adapters.len();
        lines(
            request,
            format!("{found} of {total} debug adapters are installed"),
            rows,
            json!({ "adapters": adapters }),
        )
    }

    /// `debug install <adapter>` — the Install button's own path, which is what makes the button
    /// allowed to exist: everything reachable by hand is reachable from the command line.
    ///
    /// It runs a package manager or an editor's extension installer **in the run tile**, so the
    /// install is a visible program that `run output` can read and `run stop` can stop. Unluminous itself
    /// still fetches nothing.
    fn cli_debug_install(&mut self, request: &Request) -> Outcome {
        let Some(name) = request.text("adapter") else {
            return no(request, code::USAGE, "Say which debugger to install.");
        };
        let Some(entry) = debuggers::find(&name) else {
            let known: Vec<&str> = debuggers::ALL.iter().map(|entry| entry.name).collect();
            return no(
                request,
                code::NOT_FOUND,
                format!("This version of Unluminous drives {}.", known.join(" and ")),
            );
        };
        let command = entry.install_command();
        if command.is_empty() {
            return no(
                request,
                code::NOT_APPLICABLE,
                format!(
                    "Unluminous has no way to install {name} here. {}. Set debug.{name} to it once you have one.",
                    entry.comes_from
                ),
            );
        }
        self.message = None;
        self.debug_a_configuration(DebugAction::InstallAdapter(name.clone()));
        let said = self.message.clone().unwrap_or_else(|| format!("Installing {name}"));
        match self.run.index_of(&format!("Install {name}")) {
            Some(_) => ok(request, said, json!({ "adapter": name, "command": command })),
            // The run would not start, and the reason is what the status bar was given.
            None => no(request, code::NOT_APPLICABLE, said),
        }
    }

    fn cli_debug_status(&mut self, request: &Request) -> Outcome {
        if self.debug.is_none() {
            // A build in flight is not "nothing": a caller polling status through a `cargo build`
            // has to be able to tell it from a session that never started.
            if let Some(pending) = self.debug_build.as_ref() {
                let said = format!("{} for {}", pending.what, pending.configuration.name);
                let value = json!({
                    "running": false,
                    "state": "building",
                    "building": pending.command,
                    "seconds": pending.started.elapsed().as_secs(),
                    "configuration": pending.configuration.name,
                });
                if request.switch("wait-for-pause") {
                    return Outcome::Hold(Waiting::DebugPause {
                        command: request.command.clone(),
                        until: waits_for(request, "timeout", Self::BUILD_WAIT),
                    });
                }
                return ok(request, said, value);
            }
            return ok(
                request,
                "Nothing is being debugged.",
                json!({ "running": false, "state": "none" }),
            );
        }
        if request.switch("wait-for-pause")
            && !self.debug.as_ref().is_some_and(DebugState::is_ready)
        {
            return Outcome::Hold(Waiting::DebugPause {
                command: request.command.clone(),
                until: waits_for(request, "timeout", Self::DEBUG_WAIT),
            });
        }
        Outcome::Reply(self.debug_status_reply(request))
    }

    /// One sentence and the whole state, which is what a `--wait-for-pause` answers with when the
    /// program stops and what `debug status` answers with straight away.
    pub(crate) fn debug_status_reply(&self, request: &Request) -> Reply {
        let said = match self.debug.as_ref() {
            Some(debug) => format!("{} is {}", debug.configuration.name, debug.where_it_is()),
            None => "Nothing is being debugged.".to_owned(),
        };
        Reply::done(&request.command, said, self.debug_status_value())
    }

    /// Chooses the compact debugger state or the paused location and locals for a status reply.
    fn debug_status_value(&self) -> Value {
        match self.debug.as_ref().is_some_and(DebugState::is_paused) {
            true => self.debug_pause_value(),
            false => self.debug_state_value(),
        }
    }

    /// Serializes only the session facts shared by debugger commands of every kind.
    pub(crate) fn debug_state_value(&self) -> Value {
        let Some(debug) = self.debug.as_ref() else {
            return json!({ "running": false, "state": "none", "visible": self.debug_panel.visible });
        };
        let location = debug.location();
        json!({
            "running": true,
            "configuration": debug.configuration.name,
            "adapter": debug.adapter,
            "state": debug.state().label(),
            "where": debug.where_it_is(),
            "paused": debug.is_paused(),
            "exitCode": debug.exit_code(),
            "visible": self.debug_panel.visible,
            "path": location.as_ref().map(|(path, _)| path.to_string_lossy()),
            "line": location.as_ref().map(|(_, line)| *line),
        })
    }

    /// Leads a stopped reply with the selected frame and the runtime locals already fetched for it.
    fn debug_pause_value(&self) -> Value {
        let Some(debug) = self.debug.as_ref() else {
            return self.debug_state_value();
        };
        let mut value = self.debug_state_value();
        let frame = debug
            .frame
            .and_then(|id| debug.frames.iter().find(|frame| frame.id == id))
            .or_else(|| debug.frames.first())
            .map(Self::debug_frame_value);
        let locals = debug
            .rows
            .iter()
            .filter(|row| !row.is_scope)
            .map(Self::debug_variable_value)
            .collect::<Vec<Value>>();
        let lines = debug
            .rows
            .iter()
            .filter(|row| !row.is_scope)
            .map(|row| match &row.kind {
                Some(kind) => format!("{}: {kind} = {}", row.name, row.value),
                None => format!("{} = {}", row.name, row.value),
            })
            .collect::<Vec<String>>();
        if let Value::Object(object) = &mut value {
            object.insert("pausedFrame".to_owned(), json!(frame));
            object.insert("locals".to_owned(), Value::Array(locals));
            object.insert("lines".to_owned(), json!(lines));
        }
        value
    }

    /// Adds only the variable tree to the shared debugger state for variable-oriented commands.
    fn debug_variables_value(&self) -> Value {
        let mut value = self.debug_state_value();
        let variables = self
            .debug
            .as_ref()
            .map(|debug| debug.rows.iter().map(Self::debug_variable_value).collect::<Vec<Value>>())
            .unwrap_or_default();
        if let Value::Object(object) = &mut value {
            object.insert("variables".to_owned(), Value::Array(variables));
        }
        value
    }

    /// Adds only watched expressions to the shared debugger state for watch commands.
    fn debug_watches_value(&self) -> Value {
        let mut value = self.debug_state_value();
        let watches = self.debug.as_ref().map(|debug| debug.watches.iter().map(|watch| json!({
            "expression": watch.expression,
            "value": watch.result.as_ref().and_then(|result| result.as_ref().ok()).map(|value| value.value.clone()),
            "error": watch.result.as_ref().and_then(|result| result.as_ref().err()).cloned(),
        })).collect::<Vec<Value>>()).unwrap_or_default();
        if let Value::Object(object) = &mut value {
            object.insert("watches".to_owned(), Value::Array(watches));
        }
        value
    }

    /// Adds the requested call stack to an otherwise compact debugger reply.
    fn debug_frames_value(&self, include_subtle: bool) -> Value {
        let Some(debug) = self.debug.as_ref() else {
            return self.debug_state_value();
        };
        let mut value = self.debug_state_value();
        let frames = debug
            .frames
            .iter()
            .filter(|frame| include_subtle || !frame.subtle)
            .map(Self::debug_frame_value)
            .collect::<Vec<Value>>();
        if let Value::Object(object) = &mut value {
            object.insert(
                "hiddenFrames".to_owned(),
                json!(debug.frames.len().saturating_sub(frames.len())),
            );
            object.insert("frames".to_owned(), Value::Array(frames));
        }
        value
    }

    /// Serializes one fetched variable row for either a locals snapshot or the variables command.
    fn debug_variable_value(row: &crate::app::debug::Row) -> Value {
        json!({
            "path": row.key,
            "name": row.name,
            "value": row.value,
            "type": row.kind,
            "expandable": row.has_children(),
            "expanded": row.expanded,
            "changed": row.changed,
        })
    }

    /// Serializes one stored DAP frame without changing which frames Unluminous retains for the UI.
    fn debug_frame_value(frame: &unluminous_dap::Frame) -> Value {
        json!({
            "name": frame.name,
            "path": frame.path,
            "line": frame.line,
            "subtle": frame.subtle,
        })
    }

    /// Every breakpoint in the project, as a command answers with it.
    fn breakpoints_value(&self) -> Value {
        let root = self.tree.root().to_path_buf();
        let rows: Vec<Value> = self
            .every_breakpoint()
            .into_iter()
            .flat_map(|(path, breakpoints)| {
                let shown = crate::services::project_state::relative(&root, &path);
                breakpoints
                    .iter()
                    .map(|breakpoint| {
                        let answered = self
                            .debug
                            .as_ref()
                            .and_then(|debug| debug.verified(&path, breakpoint.offset));
                        json!({
                            "path": shown.to_string_lossy(),
                            "line": self.offset_line_number(&path, breakpoint.offset),
                            "enabled": breakpoint.enabled,
                            "condition": breakpoint.condition,
                            "log": breakpoint.log_message,
                            "verified": answered.map(|answer| answer.verified),
                        })
                    })
                    .collect::<Vec<Value>>()
            })
            .collect();
        json!({ "breakpoints": rows })
    }

    /// Take every breakpoint out of the project, open files and closed ones alike.
    fn clear_every_breakpoint(&mut self) -> usize {
        let files = self.every_breakpoint();
        let cleared = files.iter().map(|(_, set)| set.len()).sum();
        for (path, _) in files {
            self.change_breakpoints(&path, |breakpoints| {
                breakpoints.clear();
            });
            self.send_the_breakpoints_of(&path);
        }
        cleared
    }

    /// The byte offset of the start of a **one-based** line in a file, open or not.
    ///
    /// The ownership rule, once more: an open file's own text answers, and a closed file's bytes are
    /// read at the moment of use.
    fn offset_of_line(&self, path: &Path, line: usize) -> usize {
        if let Some(index) = self.files.index_of(path) {
            return self.files.at(index).document.offset_of_line_number(line);
        }
        // The file as a `Document` would hold it, so an offset means the same byte whether or not
        // the file happens to be open — `task-1794`, where it did not.
        let Ok(text) = unluminous_core::document::read_to_normalised_string(path) else {
            return 0;
        };
        text.split_inclusive('\n')
            .take(line.saturating_sub(1))
            .map(str::len)
            .sum::<usize>()
            .min(text.len())
    }

    /// And the other direction, on the same terms.
    fn offset_line_number(&self, path: &Path, offset: usize) -> usize {
        if let Some(index) = self.files.index_of(path) {
            return self.files.at(index).document.line_number_of(offset);
        }
        let Ok(text) = unluminous_core::document::read_to_normalised_string(path) else {
            return 1;
        };
        text.as_bytes()[..offset.min(text.len())].iter().filter(|byte| **byte == b'\n').count() + 1
    }
}
