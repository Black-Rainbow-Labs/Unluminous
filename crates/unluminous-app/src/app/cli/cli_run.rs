//! `run` -- named run configurations and the terminal-shaped tile a running one prints into.
//! `task-1683` is the design and `task-1684` the implementation.

use super::*;

/// What a configuration's origin is called in a reply: `permanent`, `temporary` or `suggested`.
fn origin_name(origin: Origin) -> &'static str {
    match origin {
        Origin::Permanent => "permanent",
        Origin::Temporary => "temporary",
        Origin::Suggested => "suggested",
    }
}

impl UnluminousApp {
    /// `unluminous-cli run ...` — the whole of `task-1683` from the command line, which also makes every
    /// one of these an MCP tool the day it lands, because the tools are generated from the
    /// catalogue.
    ///
    /// `run output` is the one to notice: it reads the run's `Screen` — the same screen the painter
    /// reads — so an agent can start a dev server, read its port out of the log, exercise it and
    /// stop it, with nobody watching.
    pub(crate) fn cli_run(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "list" => self.cli_run_list(request),
            "add" => self.cli_run_add(request),
            "remove" => self.cli_run_remove(request),
            "start" => self.cli_run_do(request, RunAction::Start),
            "stop" => self.cli_run_do(request, RunAction::Stop),
            "rerun" => self.cli_run_do(request, RunAction::Rerun),
            "select" => self.cli_run_select(request),
            "output" => self.cli_run_output(request),
            "status" => self.cli_run_status(request),
            _ => unknown(request),
        }
    }

    fn cli_run_list(&mut self, request: &Request) -> Outcome {
        let rows: Vec<String> = self
            .run_rows()
            .iter()
            .map(|row| {
                let state = self
                    .run
                    .index_of(&row.name)
                    .and_then(|at| self.run.at(at))
                    .map(|run| run.state().label())
                    .unwrap_or_else(|| "not started".to_owned());
                format!(
                    "{}{:<28} {:<11} {state}",
                    if self.run_selected.as_deref() == Some(row.name.as_str()) { "*" } else { " " },
                    row.name,
                    origin_name(row.origin)
                )
            })
            .collect();
        let count = rows.len();
        lines(request, format!("{count} run configurations"), rows, self.run_value())
    }

    fn cli_run_add(&mut self, request: &Request) -> Outcome {
        let Some(name) = request.text("name") else {
            return no(request, code::USAGE, "Say what to call it.");
        };
        let Some(command) = request.text("command") else {
            return no(request, code::USAGE, "Say what to run.");
        };
        if self.run_configurations.find(&name).is_some() {
            return no(
                request,
                code::USAGE,
                format!("There is already a run configuration called {name}."),
            );
        }
        let configuration = Configuration {
            name: name.clone(),
            command: command.clone(),
            directory: request.text("directory").unwrap_or_default(),
            env: request.text("env").unwrap_or_default(),
        };
        let Some((program, _)) = configuration.program_and_arguments() else {
            return no(request, code::USAGE, "The command has no program in it.");
        };
        // Said rather than refused. A configuration may name a program that will exist by the time
        // it is run, so this is a note; what it saves is `task-1691`'s first failure, where `node
        // primes.js` was accepted without comment and only failed at `run start`, on a window
        // launched from Finder with no nvm directory on its `PATH`.
        let directory = configuration.working_directory(self.tree.root());
        let said = match run_configurations::found_on_path(&program, &directory) {
            true => format!("Added {name}"),
            false => format!(
                "Added {name}, but {program} could not be found on this window's PATH. It will not \
                 start until it can be — an Unluminous opened from the desktop does not have a version \
                 manager's directories on its PATH, so naming the program in full may be what is \
                 wanted."
            ),
        };
        self.run_configurations.add_permanent(configuration);
        self.run_selected = Some(name.clone());
        self.unsaved_run_configurations = true;
        ok(request, said, self.run_value())
    }

    fn cli_run_remove(&mut self, request: &Request) -> Outcome {
        let Some(name) = request.text("name") else {
            return no(request, code::USAGE, "Say which configuration.");
        };
        if self.run_configurations.find(&name).is_none() {
            return no(
                request,
                code::NOT_FOUND,
                format!("There is no run configuration called {name}."),
            );
        }
        // Typing the command is the deliberate act the dialog's question exists to ask for, which is
        // the rule `explorer delete` already keeps — so the run is stopped and the configuration
        // goes, with no question.
        let stopped = self.run.index_of(&name).is_some();
        self.answer_the_question(crate::app::Answer::RemoveRun(name.clone()));
        ok(
            request,
            match stopped {
                true => format!("Stopped and removed {name}"),
                false => format!("Removed {name}"),
            },
            self.run_value(),
        )
    }

    /// The three that do the same thing to a configuration: start it, stop it, run it again.
    ///
    /// One arm rather than three, because all three go through `run_a_configuration`, which is the
    /// one place a run action turns into a change — so a thing done from a script and the same
    /// thing done from the widget are the same thing, including what it says about it.
    fn cli_run_do(
        &mut self,
        request: &Request,
        what: impl FnOnce(Option<String>) -> RunAction,
    ) -> Outcome {
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
        // A program that could not be spawned is a failure, not a success with an apology in the
        // message. `task-1691` measured this arm reporting `isError` false for `node primes.js`
        // with no node on the window's `PATH`, which left an agent unable to tell a program that
        // failed to start from one that ran and exited at once.
        if let Err(problem) = self.run_a_configuration(what(named)) {
            return no(request, code::FAILED, problem);
        }
        let said = self.message.clone().unwrap_or_default();
        ok(request, said, self.run_value())
    }

    fn cli_run_select(&mut self, request: &Request) -> Outcome {
        let Some(name) = request.text("name") else {
            return no(request, code::USAGE, "Say which configuration.");
        };
        if self.configuration_named(Some(&name)).is_none() {
            return no(
                request,
                code::NOT_FOUND,
                format!("There is no run configuration called {name}."),
            );
        }
        if let Err(problem) = self.run_a_configuration(RunAction::Select(name.clone())) {
            return no(request, code::FAILED, problem);
        }
        ok(request, format!("{name} is chosen"), self.run_value())
    }

    fn cli_run_output(&mut self, request: &Request) -> Outcome {
        // Take in whatever the programs have written since the last frame, so a read straight after
        // a start is not looking at the screen as it was before anything ran.
        self.run.settle();
        let name = request.text("name");
        let tail = request.whole("tail");
        let Some(text) = self.run_output(name.as_deref(), tail) else {
            return no(
                request,
                code::NOT_APPLICABLE,
                match name {
                    Some(name) => format!("{name} has not been run."),
                    None => "Nothing has been run. `run start` starts something.".to_owned(),
                },
            );
        };
        match request.text("wait-for") {
            Some(needle) if !text.contains(&needle) => Outcome::Hold(Waiting::RunOutput {
                name,
                needle,
                tail,
                until: waits_for(request, "timeout", DEFAULT_WAIT),
            }),
            Some(needle) => ok(
                request,
                String::new(),
                json!({ "text": text, "waitedFor": needle, "found": true }),
            ),
            None => ok(request, String::new(), json!({ "text": text })),
        }
    }

    fn cli_run_status(&mut self, request: &Request) -> Outcome {
        self.run.settle();
        let name = request.text("name").or_else(|| self.run_selected.clone());
        let Some(name) = name else {
            return no(
                request,
                code::NOT_APPLICABLE,
                "No run configuration is chosen. `run select <name>` chooses one.",
            );
        };
        let Some(run) = self.run.index_of(&name).and_then(|at| self.run.at(at)) else {
            return ok(
                request,
                format!("{name} has not been run."),
                json!({ "name": name, "started": false, "running": false, "state": "not started" }),
            );
        };
        let state = run.state();
        ok(
            request,
            format!("{name} is {}", state.label()),
            json!({
                "name": name,
                "started": true,
                "running": state.is_running(),
                "state": state.label(),
                "exitCode": run.exit_code(),
            }),
        )
    }

    /// What a run has written, with the blank lines under it trimmed away and at most `tail` lines
    /// kept. `None` when there is no such run.
    ///
    /// The scrollback is read, not just the screen. A program that printed more lines than the run
    /// tile is tall has had the start of its output scrolled off the top, and reading only what was
    /// showing meant a `tail` larger than the tile's height silently answered with the tile's height
    /// — so a dev server whose port had scrolled past could not be asked for its port. What the
    /// terminal still holds is [`unluminous_terminal::SCROLLBACK`] lines.
    pub(crate) fn run_output(&self, name: Option<&str>, tail: Option<usize>) -> Option<String> {
        let index = match name {
            Some(name) => self.run.index_of(name)?,
            None => self.run.active_index(),
        };
        let run = self.run.at(index)?;
        Some(run.session.written_text(tail))
    }

    /// Everything about running, which every one of these commands answers with.
    fn run_value(&self) -> Value {
        let configurations: Vec<Value> = self
            .run_rows()
            .iter()
            .map(|row| {
                let configuration = self.configuration_named(Some(&row.name)).unwrap_or_default();
                let run = self.run.index_of(&row.name).and_then(|at| self.run.at(at));
                json!({
                    "name": row.name,
                    "command": configuration.command,
                    "directory": configuration.directory,
                    "env": configuration.env,
                    "origin": origin_name(row.origin),
                    "started": run.is_some(),
                    "running": row.running,
                    "state": run.map(|run| run.state().label()),
                    "exitCode": run.and_then(|run| run.exit_code()),
                })
            })
            .collect();
        json!({
            "selected": self.run_selected,
            "visible": self.run.visible,
            "height": self.panes.run_height,
            "configurations": configurations,
            "runs": self.run.names(),
            "activeRun": self.run.active().map(|run| run.name().to_owned()),
        })
    }
}
