//! Running a program, and debugging one.
//!
//! A run configuration is a **named command line**, a folder and some environment variables, and
//! pressing play spawns a `unluminous_terminal::Session` with the program in place of the shell.
//! Debug is the same configuration under a debugger: same command, same folder, same environment.
//!
//! Which debugger a language uses is data in a plugin and the debugger itself is code in Unluminous —
//! `services::debuggers` is that half — and nothing here speaks the protocol, which is `unluminous_dap`.

use std::path::{Path, PathBuf};

use egui::Vec2;
use unluminous_core::Command;

use crate::app::debug::{Built, DebugState, PendingBuild};
use crate::components::debug_panel::{self};
use crate::components::run_panel::{self};
use crate::components::run_widget;
use crate::services::debuggers;
use crate::services::locators;
use crate::services::run_configurations::{self, Configuration, Origin};

use crate::app::actions::{DebugAction, RunAction};
use crate::app::dock;
use crate::app::{UnluminousApp, ADAPTER_SEARCH_TTL};

impl UnluminousApp {
    /// Do what the `Run` menu, the run widget, the keyboard or the command line asked for.
    ///
    /// Split out of [`Self::run_action`] rather than written into it, for the reason `run_git` is
    /// split out: seven arms about one subject read better together, and the arm in `run_action`
    /// stays one line. It is still the one place a run action turns into a change.
    /// The reply is the reason it could not be done, or nothing when it was.
    ///
    /// It used to say nothing at all and leave the reason in the status bar, which the command line
    /// then read back and reported as a **success**: `task-1691` measured `run start` on a
    /// configuration whose program was not on the `PATH` coming back with `isError` false, `started`
    /// false and no reason anywhere. The one place a run action turns into a change is also the one
    /// place that knows whether it did, so it is what says so.
    pub fn run_a_configuration(&mut self, what: RunAction) -> Result<(), String> {
        match what {
            RunAction::Start(named) => match self.configuration_named(named.as_deref()) {
                Some(configuration) => self.start_a_run(configuration),
                None => Err(self.no_such_configuration(named.as_deref())),
            },
            // Rerun and start are the same thing, because starting one that is already running
            // stops it and starts it again — §5.2. Two entries because two words are what a person
            // reaches for, one path because they mean the same.
            RunAction::Rerun(named) => match self.configuration_named(named.as_deref()) {
                Some(configuration) => self.start_a_run(configuration),
                None => Err(self.no_such_configuration(named.as_deref())),
            },
            RunAction::Stop(named) => {
                let name = named.or_else(|| self.run_selected.clone());
                match name.as_deref().and_then(|name| self.run.index_of(name)) {
                    Some(at) => {
                        let name =
                            self.run.at(at).map(|run| run.name().to_owned()).unwrap_or_default();
                        self.run.stop(at);
                        self.message = Some(format!("Stopping {name}"));
                        Ok(())
                    }
                    None => {
                        // Nothing to stop is a refusal rather than a quiet nothing, for the same
                        // reason a run that would not start is: a caller that is told it worked
                        // cannot tell it from a program that stopped by itself a moment ago.
                        let problem = match name {
                            Some(name) => format!("{name} is not running."),
                            None => "Nothing is running.".to_owned(),
                        };
                        self.message = Some(problem.clone());
                        Err(problem)
                    }
                }
            }
            RunAction::Select(name) => match self.configuration_named(Some(&name)) {
                Some(_) => {
                    self.run_selected = Some(name);
                    Ok(())
                }
                None => Err(self.no_such_configuration(Some(&name))),
            },
            RunAction::CurrentFile => self.run_the_current_file(),
            RunAction::Edit => {
                self.close_every_modal();
                let chosen = self.run_selected.clone();
                self.run_dialog.open(chosen);
                Ok(())
            }
        }
    }

    /// Say that a configuration of this name is not there, in the words that fit which question was
    /// asked: naming one that is not there and having chosen nothing at all are two different
    /// things to be told.
    fn no_such_configuration(&mut self, named: Option<&str>) -> String {
        let problem = match named {
            Some(name) => format!("There is no run configuration called {name}."),
            None => "No run configuration is chosen. Press the play button to make one.".to_owned(),
        };
        self.message = Some(problem.clone());
        problem
    }

    /// The configuration of this name, or the one the widget has chosen when no name is given.
    ///
    /// A **suggestion** counts: running one is how it becomes a temporary, which is what makes the
    /// detectors worth having. Everything that runs something comes through here, so the widget,
    /// the menu and the command line cannot come to different answers about what a name means.
    pub fn configuration_named(&self, named: Option<&str>) -> Option<Configuration> {
        let name = match named {
            Some(name) => name.to_owned(),
            None => self.run_selected.clone()?,
        };
        if let Some((_, configuration)) = self.run_configurations.find(&name) {
            return Some(configuration.clone());
        }
        self.suggestions().into_iter().find(|configuration| configuration.name == name)
    }

    /// What the built-in detectors offer for this project, given the plugins that are switched on.
    ///
    /// Worked out at the moment of use rather than held, which is the rule `Plugins::renders`
    /// keeps: switching the JavaScript plugin off withdraws the npm suggestions in the same frame.
    pub fn suggestions(&self) -> Vec<Configuration> {
        let runners = self.plugins.project_runners();
        let offered = run_configurations::detect(self.tree.root(), &runners);
        // A suggestion whose name is already a configuration is not offered twice: once somebody
        // has kept `cargo run`, the detector has nothing left to say about it.
        offered
            .into_iter()
            .filter(|configuration| self.run_configurations.find(&configuration.name).is_none())
            .collect()
    }

    /// Every configuration the widget's flyout and the `Run` menu list, in that order.
    pub fn run_rows(&self) -> Vec<run_widget::Row> {
        let mut rows: Vec<run_widget::Row> = self
            .run_configurations
            .listed()
            .into_iter()
            .map(|(origin, configuration)| run_widget::Row {
                name: configuration.name.clone(),
                origin,
                running: self
                    .run
                    .index_of(&configuration.name)
                    .and_then(|at| self.run.at(at))
                    .is_some_and(run_panel::Run::is_running),
            })
            .collect();
        rows.extend(self.suggestions().into_iter().map(|configuration| run_widget::Row {
            name: configuration.name,
            origin: Origin::Suggested,
            running: false,
        }));
        rows
    }

    /// Start a configuration, showing the tile and the run it made.
    ///
    /// Running a suggestion or a file makes a **temporary**, which is what puts it in the list so
    /// it can be run again and kept. Running something that is already permanent adds nothing.
    fn start_a_run(&mut self, configuration: Configuration) -> Result<(), String> {
        let root = self.tree.root().to_path_buf();
        let size = self.run_grid_size();
        let waker = self.waker();
        let name = configuration.name.clone();
        // Remembered before it is started, so a program that will not start still leaves the thing
        // that was tried in the list rather than vanishing with the error message.
        if self.run_configurations.find(&name).is_none() {
            self.run_configurations.add_temporary(configuration.clone());
        }
        self.run_selected = Some(name.clone());
        match self.run.start(configuration, &root, size, waker) {
            Ok(_) => {
                self.show_the_run_tile(true);
                self.a_tile_took_the_keyboard(dock::Panel::Run);
                self.message = Some(format!("Running {name}"));
                Ok(())
            }
            Err(problem) => {
                self.message = Some(problem.clone());
                Err(problem)
            }
        }
    }

    /// `Run Current File`: the open file's language's own command, with `{file}` replaced.
    fn run_the_current_file(&mut self) -> Result<(), String> {
        let Some(template) = self.run_file_template() else {
            let problem = "This file's language has not said how one file of it is run.".to_owned();
            self.message = Some(problem.clone());
            return Err(problem);
        };
        let Some(path) = self.document().path().map(Path::to_path_buf) else {
            // A document that has never been saved has no path, so there is nothing to run.
            let problem = "Save the file first, so there is something to run.".to_owned();
            self.message = Some(problem.clone());
            return Err(problem);
        };
        let root = self.tree.root().to_path_buf();
        let configuration = run_configurations::for_file(&template, &root, &path);
        self.start_a_run(configuration)
    }

    /// The open file's `run.file`, when a plugin that is switched on claims it and the file has
    /// been saved somewhere.
    ///
    /// What decides whether `Run Current File` is on the menu and in the flyout at all — absent,
    /// not dimmed, which is the rule the three code-navigation entries already follow.
    pub fn run_file_template(&self) -> Option<String> {
        let path = self.document().path()?;
        self.plugins.run_file(path).map(str::to_owned)
    }

    /// Everything on the Run menu's debug half, and the tile's own buttons.
    ///
    /// **The one place a debug action turns into a change**, which is `run_action`'s rule applied
    /// within the family: the menu, the keyboard, the tile and the command line all come through
    /// here, so a thing done from a script and the same thing done by hand are the same thing,
    /// including what it says about it.
    pub fn debug_a_configuration(&mut self, what: DebugAction) {
        match what {
            DebugAction::Start(named) => match self.configuration_named(named.as_deref()) {
                Some(configuration) => self.start_debugging(configuration, None),
                // The sentence goes into the status bar, which is where every other refusal in
                // this family goes; `cli_debug_start` reads back whether a session exists rather
                // than what was said, so it needs no value here.
                None => {
                    self.no_such_configuration(named.as_deref());
                }
            },
            DebugAction::CurrentFile => self.debug_the_current_file(),
            DebugAction::Stop => match self.debug.as_mut() {
                Some(debug) => {
                    debug.stop();
                    self.message = debug.message.clone();
                }
                None => self.message = Some("Nothing is being debugged.".to_owned()),
            },
            DebugAction::Resume => self.step(unluminous_dap::Step::Resume),
            DebugAction::StepOver => self.step(unluminous_dap::Step::Over),
            DebugAction::StepInto => self.step(unluminous_dap::Step::Into),
            DebugAction::StepOut => self.step(unluminous_dap::Step::Out),
            DebugAction::Pause => self.step(unluminous_dap::Step::Pause),
            DebugAction::RunToCursor => self.run_to_cursor(),
            DebugAction::ToggleBreakpoint => self.toggle_breakpoint_here(),
            DebugAction::EditBreakpoint => self.open_the_breakpoint_dialog(),
            DebugAction::ToggleBreakpointEnabled => self.toggle_the_breakpoint_enabled(),
            DebugAction::ShowValue => self.show_the_value_at_the_caret(),
            DebugAction::EvaluateExpression => self.open_the_expression_box(),
            DebugAction::ToggleTile => {
                let showing = self.debug_panel.visible;
                self.show_the_debug_tile(!showing);
            }
            DebugAction::InstallAdapter(adapter) => self.install_an_adapter(&adapter),
        }
    }

    /// Install a debug adapter, by running its own install command in the run tile.
    ///
    /// **The editor still fetches nothing.** What runs is a package manager or an editor's extension
    /// installer, named by the registry entry, in a visible terminal with a program in it that can be
    /// watched, read with `run output` and stopped — which is every other run's rules, applied to
    /// this one. `task-1692` §7.1, and `tools/release.ps1` installing `gh` with winget is the same
    /// move made a year earlier.
    ///
    /// The configuration it makes is a temporary, so it is offered again if it has to be run again
    /// and is never written into the project's own file. What was selected before is put back
    /// afterwards: installing something is not choosing what the play button does.
    fn install_an_adapter(&mut self, adapter: &str) {
        let adapter = match adapter.trim().is_empty() {
            // An empty name means "the one that could not start", which is the adapter the file that
            // is showing would use — the same question the refusal itself asked.
            true => match self.document().path().and_then(|path| self.plugins.debugger_for(path)) {
                Some(named) => named.to_owned(),
                None => {
                    self.message = Some("Say which debugger to install.".to_owned());
                    return;
                }
            },
            false => adapter.trim().to_owned(),
        };
        let Some(entry) = debuggers::find(&adapter) else {
            self.message = Some(format!(
                "This version of Unluminous does not know a debugger called {adapter}."
            ));
            return;
        };
        let command = entry.install_command();
        if command.is_empty() {
            self.message = Some(format!(
                "Unluminous has no way to install {adapter} here. {}. Set {} to it once you have one.",
                entry.comes_from,
                format_args!("debug.{adapter}")
            ));
            return;
        }
        let was_selected = self.run_selected.clone();
        self.forget_the_adapter_search();
        let configuration = Configuration::new(format!("Install {adapter}"), &command);
        match self.start_a_run(configuration) {
            Ok(()) => self.message = Some(format!("Installing {adapter}: {command}")),
            Err(problem) => self.message = Some(problem),
        }
        self.run_selected = was_selected;
    }

    /// Start a configuration under its debugger.
    ///
    /// **Debug is Run, under a debugger** — the same `Configuration` the play button starts, same
    /// command, same folder, same environment, which is the reference editor's own model. A second session
    /// replaces the first, because there is one at a time.
    ///
    /// `for_file` names the file the session was started for, which is what decides which language's
    /// debugger to use when the configuration is a temporary made from `run.file`.
    fn start_debugging(&mut self, configuration: Configuration, for_file: Option<PathBuf>) {
        let Some(adapter) = self.adapter_for(&configuration, for_file.as_deref()) else {
            self.message = Some(
                "Nothing has said which debugger to use for this configuration, so there is nothing to start."
                    .to_owned(),
            );
            return;
        };
        // A configuration that names a build tool is built first and debugged second — `cargo run`
        // is the commonest configuration there is, and refusing it was `task-1692`'s second sentence.
        if let Some(build) = locators::locate(&configuration.command) {
            self.begin_a_build(build, configuration, adapter, for_file);
            return;
        }
        self.launch_a_session(configuration, adapter, None);
    }

    /// What the debug tile says when there is no session: a build in flight, a debugger this machine
    /// has not got, or the invitation to press Debug.
    ///
    /// Worked out each frame from a **cached** search, because looking for an adapter reads
    /// directories — the extension folders, LLVM's install locations — and a frame may not.
    /// [`Self::forget_the_adapter_search`] is what makes an install take effect.
    pub(crate) fn debug_idle(&mut self) -> debug_panel::Idle {
        if let Some(pending) = self.debug_build.as_ref() {
            return debug_panel::Idle::Building {
                what: pending.what.clone(),
                seconds: pending.started.elapsed().as_secs(),
            };
        }
        let ready = debug_panel::Idle::Ready(
            "Nothing is being debugged. Press the bug button in the title bar, or set a breakpoint and press Shift+F9."
                .to_owned(),
        );
        // Which adapter this project would use, asked of the configuration the buttons would start.
        let Some(configuration) =
            self.configuration_named(None).or_else(|| self.suggestions().into_iter().next())
        else {
            return ready;
        };
        let Some(adapter) = self.adapter_for(&configuration, None) else {
            return ready;
        };
        let report = self.adapter_report(&adapter);
        if report.is_found() {
            return ready;
        }
        debug_panel::Idle::Missing(debug_panel::Missing {
            sentence: format!(
                "Debugging {} needs {}. {}.",
                report.name,
                report.programs.join(" or "),
                report.comes_from
            ),
            adapter: report.name.to_owned(),
            install: report.install,
        })
    }

    /// What was found the last time this adapter was looked for.
    ///
    /// The search is cached because it reads directories and the tile asks every frame; it is
    /// forgotten whenever something could have changed it, which is an install finishing or the
    /// settings being written.
    pub(crate) fn adapter_report(&mut self, adapter: &str) -> debuggers::Report {
        if let Some((looked, known)) = self.debug_adapters.get(adapter) {
            if looked.elapsed() < ADAPTER_SEARCH_TTL {
                return known.clone();
            }
        }
        let Some(entry) = debuggers::find(adapter) else {
            return debuggers::Report {
                name: "",
                found: None,
                configured: false,
                programs: Vec::new(),
                languages: Vec::new(),
                comes_from: "this version of Unluminous does not know it",
                install: String::new(),
                settings_key: format!("debug.{adapter}"),
                caveat: "",
            };
        };
        let override_path = self.settings.debug_adapter(adapter).map(str::to_owned);
        let mut report = debuggers::report(entry, override_path.as_deref());
        report.languages = self.plugins.languages_debugged_by(adapter);
        self.debug_adapters.insert(adapter.to_owned(), (std::time::Instant::now(), report.clone()));
        report
    }

    /// Look for the adapters again next time somebody asks, because something that could have
    /// changed the answer has happened — an install has finished, or the settings have moved.
    pub(crate) fn forget_the_adapter_search(&mut self) {
        self.debug_adapters.clear();
    }

    /// Which debugger a configuration is given to.
    ///
    /// **The configuration first, the open file only as a fallback.** Asking the open file first is
    /// what made debugging a Node server while reading `README.md` answer that the file's language
    /// had named no debugger — a refusal about the wrong thing. `debuggers::adapter_for` reads the
    /// command line, the plugins answer for a program whose extension one of them claims, and the
    /// file that is showing is what is left.
    pub(crate) fn adapter_for(
        &self,
        configuration: &Configuration,
        for_file: Option<&Path>,
    ) -> Option<String> {
        if let Some((program, _)) = configuration.program_and_arguments() {
            if let Some(named) = debuggers::adapter_for(&program) {
                return Some(named.to_owned());
            }
            if let Some(named) = self.plugins.debugger_for(Path::new(&program)) {
                return Some(named.to_owned());
            }
        }
        let path = for_file
            .map(Path::to_path_buf)
            .or_else(|| self.document().path().map(Path::to_path_buf));
        path.as_deref().and_then(|path| self.plugins.debugger_for(path)).map(str::to_owned)
    }

    /// Start the build a locator asked for, on a thread.
    ///
    /// The window is woken when it finishes, exactly as the git worker wakes it, and nothing else
    /// waits: the editor draws, the tile counts the seconds, and pressing Debug again replaces the
    /// build the way starting a second session replaces the first.
    fn begin_a_build(
        &mut self,
        build: locators::Build,
        configuration: Configuration,
        adapter: String,
        for_file: Option<PathBuf>,
    ) {
        let root = self.tree.root().to_path_buf();
        let folder = configuration.working_directory(&root);
        let (sender, replies) = std::sync::mpsc::channel();
        let waker = self.waker();
        let command = build.command();
        let wanted = build.wanted.clone();
        let program = build.program.clone();
        let args = build.args.clone();
        std::thread::spawn(move || {
            let answer = match std::process::Command::new(&program)
                .args(&args)
                .current_dir(&folder)
                .output()
            {
                Ok(output) if output.status.success() => {
                    let printed = String::from_utf8_lossy(&output.stdout);
                    match locators::executable(&printed, wanted.as_deref()) {
                        Some(program) => Built::Program(program),
                        None => Built::Nothing,
                    }
                }
                // The compiler's own words, which is what `--message-format=json-render-diagnostics`
                // puts on standard error and is more use than anything Unluminous could write instead.
                Ok(output) => Built::Failed(String::from_utf8_lossy(&output.stderr).to_string()),
                Err(problem) => Built::Failed(format!("{program} would not start: {problem}")),
            };
            let _ = sender.send(answer);
            waker();
        });
        // A build replaces whatever was being debugged, because Debug always means "this, now".
        self.stop_debugging();
        self.debug_output.clear();
        self.message = Some(format!("{}\u{2026}", build.what));
        self.debug_build = Some(PendingBuild {
            configuration,
            adapter,
            for_file,
            wanted: build.wanted,
            program_args: build.program_args,
            what: build.what,
            command,
            started: std::time::Instant::now(),
            replies,
        });
        self.show_the_debug_tile(true);
    }

    /// Take the build's answer, once there is one, and start the session it was for.
    ///
    /// Called once a frame from [`Self::take_the_debug_replies`], which is where every other thread's
    /// replies are already taken.
    fn take_the_build(&mut self) {
        let Some(pending) = self.debug_build.as_ref() else {
            return;
        };
        let Ok(answer) = pending.replies.try_recv() else {
            return;
        };
        let pending = self.debug_build.take().expect("just looked at it");
        match answer {
            Built::Program(program) => {
                let built = (program.to_string_lossy().to_string(), pending.program_args);
                self.launch_a_session(pending.configuration, pending.adapter, Some(built));
            }
            Built::Failed(said) => {
                // The compiler's words go where an adapter's words go, so `debug output` carries
                // them and nothing has to be read off a terminal.
                self.debug_output.extend(said.lines().map(str::to_owned));
                self.message = Some("The build failed \u{2014} see the debug output.".to_owned());
            }
            Built::Nothing => {
                self.message =
                    Some(format!("Nothing to debug: `{}` built no program.", pending.command));
            }
        }
    }

    /// Start the adapter and open the session, once there is a program to give it.
    ///
    /// `built` is what a locator produced, and it stands in for the configuration's command line in
    /// the launch request **only** — the configuration keeps its own command, so the play button
    /// still runs `cargo run` and the tile still says so.
    fn launch_a_session(
        &mut self,
        configuration: Configuration,
        adapter: String,
        built: Option<(String, Vec<String>)>,
    ) {
        let root = self.tree.root().to_path_buf();
        let override_path = self.settings.debug_adapter(&adapter).map(str::to_owned);
        // The refusal is one sentence naming what was looked for, where it comes from and the
        // command that installs it, built by the registry entry that knew — never an error dialog
        // and never a dead button.
        let prepared = match debuggers::prepare(
            &adapter,
            &configuration,
            &root,
            override_path.as_deref(),
            built,
        ) {
            Ok(prepared) => prepared,
            Err(refusal) => {
                self.message = Some(refusal.message());
                // And the tile comes up saying it, with the Install button under it. A sentence in
                // the status bar is what a person misses; `task-1692`'s whole complaint is that
                // pressing Debug looked like nothing happening.
                //
                // A session that has already ended is thrown away first, because the tile draws the
                // offer only where there is no session at all — and the empty panes of a program
                // that finished a minute ago are worth less than the reason this one never started.
                if self.debug.as_ref().is_some_and(|debug| !debug.is_alive()) {
                    self.debug = None;
                }
                self.forget_the_adapter_search();
                self.show_the_debug_tile(true);
                return;
            }
        };
        // The one that was there is stopped and thrown away first, so its adapter does not outlive
        // the session that replaced it. Nothing ever orphans a child on purpose.
        self.stop_debugging();
        let name = configuration.name.clone();
        if self.run_configurations.find(&name).is_none() {
            self.run_configurations.add_temporary(configuration.clone());
        }
        self.run_selected = Some(name.clone());
        let waker = self.waker();
        match DebugState::start(
            &adapter,
            &prepared.adapter,
            prepared.body,
            prepared.caveat,
            configuration,
            waker,
        ) {
            Ok(state) => {
                self.debug = Some(state);
                self.send_every_breakpoint();
                self.show_the_debug_tile(true);
                self.message = Some(match prepared.caveat.is_empty() {
                    true => format!("Debugging {name}"),
                    // The adapter's own limits reach the person rather than being discovered as
                    // wrong-looking values later. §5.3.
                    false => format!("Debugging {name}. {}", prepared.caveat),
                });
            }
            Err(problem) => self.message = Some(problem),
        }
    }

    /// `Debug Current File`: the open file's language's own command, under its own debugger.
    ///
    /// It exists exactly where `Run Current File` exists **and** the language names an adapter, so a
    /// `.rs` file offers neither — Rust deliberately has no `run.file`, because running one file of
    /// a Cargo project is not a thing cargo does — and a `.css` file offers nothing.
    fn debug_the_current_file(&mut self) {
        let Some(template) = self.run_file_template() else {
            self.message =
                Some("This file's language has not said how one file of it is run.".to_owned());
            return;
        };
        let Some(path) = self.document().path().map(Path::to_path_buf) else {
            self.message = Some("Save the file first, so there is something to debug.".to_owned());
            return;
        };
        let root = self.tree.root().to_path_buf();
        let configuration = run_configurations::for_file(&template, &root, &path);
        self.start_debugging(configuration, Some(path));
    }

    /// One of the five stepping requests, with the adapter's own refusal if it will not.
    pub(crate) fn step(&mut self, step: unluminous_dap::Step) {
        let Some(debug) = self.debug.as_mut() else {
            self.message = Some("Nothing is being debugged.".to_owned());
            return;
        };
        // What every row showed, remembered before the program goes on, so the next stop can mark
        // what moved: "changed" means "different from the last time you looked".
        debug.remember_the_values();
        match debug.step(step) {
            Ok(()) => self.message = debug.message.clone(),
            Err(problem) => self.message = Some(problem),
        }
        self.debug_panel.stop_editing();
    }

    /// `Run to Cursor`: a breakpoint on the caret's line for the length of one resume.
    ///
    /// DAP has no request for it and every client builds it the same way — a temporary breakpoint,
    /// a `continue`, and the breakpoint taken away at the next stop. It is done through the ordinary
    /// breakpoint path rather than a second one, so the dot is real while it lasts and the adapter is
    /// told about it exactly as it is told about any other.
    fn run_to_cursor(&mut self) {
        if self.debug.as_ref().is_none_or(|debug| !debug.is_paused()) {
            self.message = Some("The program is not stopped.".to_owned());
            return;
        }
        let Some(path) = self.document().path().map(Path::to_path_buf) else {
            self.message = Some("Save the file first, so there is a line to run to.".to_owned());
            return;
        };
        let caret = self.document().selection().head;
        let line = self.document_mut().line_start_of(caret);
        // A line that already has one needs no temporary: resuming will stop there anyway.
        let temporary = self.document().breakpoints().at(line).is_none();
        if temporary {
            self.document_mut().toggle_breakpoint(line);
            self.run_to = Some((path.clone(), line));
            self.send_the_breakpoints_of(&path);
        }
        self.step(unluminous_dap::Step::Resume);
    }

    /// Take away the temporary breakpoint `Run to Cursor` made, once the program has stopped again.
    fn clear_the_run_to_breakpoint(&mut self) {
        let Some((path, offset)) = self.run_to.take() else {
            return;
        };
        self.change_breakpoints(&path, |breakpoints| {
            breakpoints.remove_at(offset);
        });
        self.send_the_breakpoints_of(&path);
    }

    /// End the session, killing the adapter. What closing the window, closing the project and
    /// starting a second session all do.
    pub fn stop_debugging(&mut self) {
        if let Some(mut debug) = self.debug.take() {
            debug.kill();
        }
        self.debug_panel.stop_editing();
        self.run_to = None;
    }

    /// Everything the debug tile reported this frame.
    ///
    /// One function rather than twelve arms in the middle of `ui`, for the reason the run tile's
    /// outcome is settled in one place: the tile decides nothing and this decides everything, so
    /// pressing `Step Over` in the tile and pressing `F8` are the same call.
    pub(crate) fn act_on_the_debug_tile(
        &mut self,
        outcome: debug_panel::DebugOutcome,
        ctx: &egui::Context,
    ) {
        if outcome.hide {
            self.show_the_debug_tile(false);
        }
        if let Some(adapter) = outcome.install {
            self.debug_a_configuration(DebugAction::InstallAdapter(adapter));
        }
        if let Some(command) = outcome.copy {
            ctx.copy_text(command.clone());
            self.message = Some(format!("Copied: {command}"));
        }
        if outcome.console {
            // One press, both directions: the debuggee's terminal is the run tile, and two grids
            // cannot show at once.
            self.show_the_run_tile(true);
        }
        if let Some(step) = outcome.step {
            self.step(step);
        }
        if outcome.stop {
            self.debug_a_configuration(DebugAction::Stop);
        }
        if let Some(frame) = outcome.show_frame {
            if let Some(debug) = self.debug.as_mut() {
                debug.show_frame(frame);
            }
            // Clicking a frame moves the execution point to it without resuming, which is
            // The reference editor's own behaviour.
            self.follow_the_execution_point();
        }
        if let Some(key) = outcome.toggle_row {
            if let Some(debug) = self.debug.as_mut() {
                debug.toggle_row(&key);
            }
        }
        if let Some((key, value)) = outcome.set_value {
            if let Some(debug) = self.debug.as_mut() {
                if let Err(problem) = debug.set_value(&key, &value) {
                    self.message = Some(problem);
                }
            }
        }
        if let Some(expression) = outcome.add_watch {
            if let Some(debug) = self.debug.as_mut() {
                debug.add_watch(&expression);
            }
        }
        if let Some(expression) = outcome.remove_watch {
            if let Some(debug) = self.debug.as_mut() {
                debug.remove_watch(&expression);
            }
        }
        if let Some(filters) = outcome.filters {
            if let Some(debug) = self.debug.as_mut() {
                debug.set_filters(filters);
            }
        }
    }

    /// Whether the place the program is stopped is somewhere this window has not jumped to yet.
    ///
    /// `None` for the location means the frames have not come back; there is nothing to jump to yet
    /// and the next frame asks again.
    fn the_execution_point_has_not_been_followed(&self) -> bool {
        let Some(debug) = self.debug.as_ref() else {
            return false;
        };
        let Some((path, line)) = debug.location() else {
            return false;
        };
        self.followed_stop != Some((debug.stops(), path, line))
    }

    /// Take everything the adapter has said and act on it.
    ///
    /// Called once a frame, beside the git worker's own poll and the run tile's `settle`, which is
    /// where every other thread's replies are already taken.
    pub(crate) fn take_the_debug_replies(&mut self, ctx: &egui::Context) {
        // A build that has finished is what starts the session, so it is asked first — and it is
        // asked whether or not a session exists, which the early return below would have skipped.
        self.take_the_build();
        let Some(debug) = self.debug.as_mut() else {
            return;
        };
        let asked = debug.take_replies();
        let paused = debug.is_paused();
        let ended = !debug.is_alive();
        if let Some(said) = debug.message.take() {
            self.message = Some(said);
        }
        for event in asked {
            match event {
                unluminous_dap::Event::RunInTerminal { seq, title, cwd, args, env } => {
                    self.run_the_debuggee(seq, &title, &cwd, args, env);
                }
                // js-debug's child session. Opening it is the session's own business; re-sending the
                // breakpoints is the window's, because the child has never been told about any of
                // them and the store is what knows them all. `task-1692`.
                unluminous_dap::Event::StartDebugging { seq, request, configuration } => {
                    let opened = self
                        .debug
                        .as_mut()
                        .map(|debug| debug.adopt_child(seq, &request, configuration));
                    match opened {
                        Some(Ok(())) => self.send_every_breakpoint(),
                        Some(Err(problem)) => self.message = Some(problem),
                        None => {}
                    }
                }
                _ => {}
            }
        }
        // **Once a stop, not once a frame.** Being paused is a state that lasts, and a window that
        // jumped every frame it was true put the caret back on the stopped line sixty times a second
        // — so the caret could not be moved at all while a program was stopped, which is a fault
        // nothing noticed until `task-1696` asked for the word at the caret and always found the
        // first one on the line. The key is the stop's own number *and* where it turned out to be,
        // because the frames arrive a round trip after the `stopped` event and a loop stopping twice
        // on one line is two stops.
        if paused && self.the_execution_point_has_not_been_followed() {
            // The temporary breakpoint `Run to Cursor` made has done its job.
            self.clear_the_run_to_breakpoint();
            self.follow_the_execution_point();
        }
        if ended {
            self.debug_panel.stop_editing();
        }
        // A polite stop has to actually run out, and an idle window draws nothing — so it is woken
        // once, when the grace ends, rather than kept drawing for the whole two seconds.
        if let Some(left) = self.debug.as_ref().and_then(DebugState::stopping_in) {
            ctx.request_repaint_after(left);
        }
    }

    /// Answer the adapter's `runInTerminal` by starting the command in the run tile.
    ///
    /// **This is what puts a real ConPTY behind the debuggee** — its colours, its interactivity, and
    /// the run tile's own rules about opening at final size and never resizing while starting. It
    /// *is* a run, so it goes through `RunPanel::start` rather than through a second path.
    fn run_the_debuggee(
        &mut self,
        seq: i64,
        title: &str,
        cwd: &str,
        args: Vec<String>,
        env: Vec<(String, String)>,
    ) {
        let Some((program, rest)) = args.split_first() else {
            if let Some(debug) = self.debug.as_mut() {
                debug.answer_run_in_terminal(seq, false, None);
            }
            return;
        };
        let name = self
            .debug
            .as_ref()
            .map(|debug| debug.configuration.name.clone())
            .unwrap_or_else(|| title.to_owned());
        // A configuration built from what the adapter asked for rather than from what was chosen:
        // the adapter often wraps the program in one of its own, which is exactly how lldb-dap's
        // comm-file scheme works, and running the chosen command instead would run the wrong thing.
        let configuration = Configuration {
            name,
            command: run_configurations::join_command(program, rest),
            directory: String::new(),
            env: env
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<String>>()
                .join("; "),
        };
        let folder = match cwd.trim().is_empty() {
            true => self.tree.root().to_path_buf(),
            false => PathBuf::from(cwd),
        };
        let size = self.run_grid_size();
        let waker = self.waker();
        let started = self.run.start(configuration, &folder, size, waker);
        if let Some(problem) = started.as_ref().err() {
            self.message = Some(problem.clone());
        }
        if let Some(debug) = self.debug.as_mut() {
            // No process id: a pseudoconsole hands back a console rather than a child, and the
            // specification makes the id optional for exactly this reason. `started` is what the
            // adapter needs to know.
            debug.answer_run_in_terminal(seq, started.is_ok(), None);
        }
    }

    /// Open the file the program stopped in, scroll the least amount that shows the line, and put
    /// the caret on it.
    ///
    /// `open_the_match`'s path, which is what `Find in Files` and `Go to Definition` already use, so
    /// a jump from a stop and a jump from a search are the same jump.
    fn follow_the_execution_point(&mut self) {
        let Some(debug) = self.debug.as_ref() else {
            return;
        };
        let Some((path, line)) = debug.location() else {
            return;
        };
        // Written down before the early return below, so a frame in a library with no source is not
        // asked about again on every frame either.
        self.followed_stop = Some((debug.stops(), path.clone(), line));
        if !path.exists() {
            // A frame in a library Unluminous has no source for. The tile still lists it; there is simply
            // nowhere to jump to, and saying nothing is better than saying something wrong.
            return;
        }
        // Guarded, because everything after this places a caret **in the file that was opened**.
        // Unguarded it would have moved the caret in whatever tab happened to be showing, which is
        // the shape of fault `task-1804` §7.2 found in `tab open`.
        if self.open_path_permanently(&path).is_err() {
            return;
        }
        let offset = self.document().offset_of_line_number(line);
        // The caret is **placed** rather than the line selected, which is what `open_the_match` does
        // for a search hit. A stop already marks its line with a band across the whole width of the
        // pane, and a selection over the same line would be two decorations saying one thing — and
        // the wrong one of the two, since nothing about a stop is selected. The reference editor places the
        // caret here too.
        self.document_mut().apply(Command::PlaceCaret { offset, extend: false });
        self.reveal_caret = true;
        // The layout may not have been worked out at this width yet, so the scroll is asked for on
        // the next frame rather than computed here — `reveal_caret`'s own arrangement.
        self.files.at_mut(self.files.active_index()).forget_what_was_worked_out();
    }

    /// How large the run tile's grid is going to be, so a program is opened at that size and is
    /// **never resized**.
    ///
    /// This is not a nicety. A pseudoconsole resized while its child is writing its first line
    /// loses that line — measured six times out of six on `cmd /c echo something`, which writes and
    /// exits inside a millisecond and was therefore always still starting when the tile drew its
    /// first frame and told it the real size. An empty tab for a program that plainly printed
    /// something is the one thing a run tile must not do, because the whole point of it is that the
    /// evidence outlives the process.
    ///
    /// So the size is worked out from the rectangle the tile really has — `RunPanel::tile`, which
    /// the window records every frame whether the tile is showing or not — through the same
    /// function the tile itself uses, and the two agree exactly. The guess underneath is only ever
    /// reached before the window has drawn a frame at all.
    fn run_grid_size(&self) -> unluminous_terminal::session::Size {
        let cell = self.renderer.cell_metrics(self.settings.terminal_font_size);
        let tile = match self.run.tile.width() > 1.0 && self.run.tile.height() > 1.0 {
            true => self.run.tile.size(),
            false => Vec2::new(self.editor_area.width().max(600.0), self.panes.run_height),
        };
        run_panel::grid_size(tile, cell)
    }

    /// Write the project's run configurations down, if this window is the one that may.
    pub(crate) fn remember_the_run_configurations(&mut self) {
        if !self.unsaved_run_configurations {
            return;
        }
        self.unsaved_run_configurations = false;
        if !self.remembers_this_project() {
            return;
        }
        run_configurations::save(self.tree.root(), &self.run_configurations);
    }

    /// True when the file at `path` has a language that names a debugger, which is what decides
    /// whether the gutter takes a click at all.
    ///
    /// **The one question the menus, the title bar, the gutter and the command line all ask**, so
    /// none of them can disagree about whether a file can be debugged — `Plugins::debugger_for`, one
    /// reading, exactly as `file_kind::definitions_apply` is one reading.
    pub(crate) fn debug_applies_to(&self, path: Option<&Path>) -> bool {
        path.is_some_and(|path| self.plugins.debugger_for(path).is_some())
    }

    /// The same question about the file that is showing.
    pub(crate) fn debug_applies_here(&self) -> bool {
        self.debug_applies_to(self.document().path())
    }
}
