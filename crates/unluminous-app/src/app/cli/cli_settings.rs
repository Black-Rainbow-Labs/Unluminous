//! `settings`, `mcp`, `theme` and `update` -- the window's own configuration, grouped into one
//! file because each is small on its own and all four are what Unluminous itself is set to rather
//! than anything about a project.
//!
//! `PaneMeasure` and the pane and plugin setting helpers (`pane_setting`, `pane_settings`,
//! `plugin_folder`, `plugin_ids`, `plugin_setting`, `plugin_settings`) live here rather than beside
//! `panel`, where they sat by proximity in the file this was split out of: `grep` across the whole
//! original file before moving them found no use of any of the seven inside the `panel` command
//! handler itself, only inside `settings get`/`set`/`reset` and `settings list`.

use super::*;

impl UnluminousApp {
    /// A contributed pane's three settings keys, resolved to the slot and the measurement they name.
    ///
    /// They cannot be rows in `SETTINGS`, because a pane's key is `<plugin id>/<pane id>` and which
    /// plugins are installed is not known until the manifests are read. `task-1794`: Unluminous wrote
    /// `panes.agent-chat/chat.width` into its own settings file and then refused it back with "there
    /// is no setting called that", which is the one shape of dishonesty the settings file should
    /// never have — a value it writes is a value it reads.
    fn pane_setting(&self, name: &str) -> Option<(usize, PaneMeasure)> {
        let rest = name.strip_prefix("panes.")?;
        let (key, last) = rest.rsplit_once('.')?;
        let measure = match last {
            "width" => PaneMeasure::Width,
            "height" => PaneMeasure::Height,
            "zoom" => PaneMeasure::Zoom,
            _ => return None,
        };
        let slot = self.plugin_ui.slot_of(key)?;
        Some((slot, measure))
    }

    /// Where the installed plugins keep their own folders, when this window has a settings store.
    ///
    /// `None` in a test with no store, which is the rule `UnluminousApp::new` already keeps: a test
    /// must never read or write the settings of the person running it.
    fn plugin_folder(&self) -> Option<std::path::PathBuf> {
        self.store.as_ref().map(|store| store.folder().join(crate::services::plugins::FOLDER))
    }

    /// The ids of every plugin that is installed, whether or not it is switched on.
    ///
    /// Switched off as well as on, because its configuration is still its configuration -- a person
    /// switching a plugin off has not thrown away the data source they added to it, and an agent
    /// asked to set one up before turning it on should be able to.
    fn plugin_ids(&self) -> Vec<String> {
        self.plugins.all().iter().map(|plugin| plugin.id.clone()).collect()
    }

    /// `plugins.<plugin>.<key>`, when this is one. `task-1804` §4.2.
    fn plugin_setting(&self, name: &str) -> Option<crate::services::plugin_settings::Key> {
        crate::services::plugin_settings::read(name, &self.plugin_ids())
    }

    /// Every key in every installed plugin's own file, for `settings list`.
    fn plugin_settings(&self) -> Vec<(String, String)> {
        let Some(folder) = self.plugin_folder() else {
            return Vec::new();
        };
        crate::services::plugin_settings::every(&folder, &self.plugin_ids())
    }

    /// Every one of those keys, in slot order, for `settings list`.
    fn pane_settings(&self) -> Vec<(String, &'static str, String)> {
        let mut out = Vec::new();
        for (slot, key) in self.plugin_ui.pane_keys().into_iter().enumerate() {
            let label =
                self.plugin_ui.pane(slot).map(|pane| pane.label.clone()).unwrap_or_default();
            for measure in PaneMeasure::ALL {
                out.push((
                    format!("panes.{key}.{}", measure.name()),
                    measure.accepts(),
                    measure.help(&label),
                ));
            }
        }
        out
    }

    /// `update check` -- whether a newer Unluminous has been released.
    ///
    /// **It asks on this thread and waits**, unlike the window's own check, and that is the right
    /// shape here rather than a shortcut: a command line caller has asked a question and is waiting
    /// for the answer, so a reply saying "started asking" would be a reply they then have to poll
    /// for. The window's check is on a thread because a window that stops drawing looks like a
    /// crash; a command that takes a second does not.
    ///
    /// The answer is also kept on the window, so opening the About box after running this shows what
    /// it found -- one place a check's answer lives, which is `run_cli`'s rule.
    pub(crate) fn cli_update(&mut self, request: &Request, verb: &str) -> Outcome {
        if verb != "check" {
            return unknown(request);
        }
        let answer = crate::services::update::ask();
        self.message = Some(answer.sentence());
        self.update_answer = Some(answer.clone());
        let said = answer.sentence();
        match answer {
            crate::services::update::Answer::Newer(release) => ok(
                request,
                said,
                json!({
                    "current": crate::build_info::VERSION,
                    "latest": release.version,
                    "newer": true,
                    "url": release.url,
                    "notes": release.notes,
                }),
            ),
            crate::services::update::Answer::Current(version) => ok(
                request,
                said,
                json!({
                    "current": crate::build_info::VERSION,
                    "latest": version,
                    "newer": false,
                    "url": crate::services::update::RELEASES_PAGE,
                }),
            ),
            // A check that could not be made is a failure rather than "no update", which is
            // `task-1804` §7.2's rule: a caller told there is nothing newer would believe it.
            crate::services::update::Answer::Failed(problem) => no(request, code::FAILED, problem),
        }
    }

    pub(crate) fn cli_settings(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "list" => {
                // The value column is as wide as the widest value there is, so a path in
                // `debug.lldb` does not run into the help beside it: `task-1704` measured a reply
                // where the two columns had no seam at all.
                // The static ones, then the contributed panes' — which Unluminous writes into its own
                // settings file and, before `task-1794`, would not name back.
                let mut named: Vec<(String, String)> =
                    SETTINGS.iter().map(|key| (key.name.to_owned(), key.help.to_owned())).collect();
                named.extend(self.pane_settings().into_iter().map(|(name, _, help)| (name, help)));
                // And every installed plugin's own configuration, which before `task-1804` §4.2
                // was reachable from a Settings page and from nowhere else -- so an agent could open
                // the Agent-Chat pane and not configure it, and add a data source only by hand.
                let plugin_values = self.plugin_settings();
                named.extend(plugin_values.iter().map(|(name, _)| {
                    (name.clone(), "A key in this plugin's own configuration file.".to_owned())
                }));
                let values: Vec<String> = named
                    .iter()
                    .map(|(name, _)| match plugin_values.iter().find(|(key, _)| key == name) {
                        Some((_, value)) => value.clone(),
                        None => self.setting_text(name),
                    })
                    .collect();
                let widest = values.iter().map(|value| value.len()).max().unwrap_or(0).max(16);
                // **The name column is measured too**, and it is `task-1704`'s own fix applied to the
                // other column: 34 was wide enough for every key there was, and
                // `plugins.agent-chat.provider.0.program` is 37 -- so the name ran straight into its
                // value with no space at all between them. A width taken from the widest name cannot
                // go out of date the way a number chosen once did.
                let longest = named.iter().map(|(name, _)| name.len()).max().unwrap_or(0).max(34);
                let rows: Vec<String> = named
                    .iter()
                    .zip(values)
                    .map(|((name, help), value)| {
                        let padded = format!("{value:<width$}  ", width = widest);
                        format!("{name:<longest$}  {padded}{help}")
                    })
                    .collect();
                let count = named.len();
                lines(request, format!("{count} settings"), rows, self.settings_value())
            }
            "get" => {
                let Some(name) = request.text("key") else {
                    return no(request, code::USAGE, "Say which setting.");
                };
                match SETTINGS.iter().find(|key| key.name == name) {
                    Some(key) => ok(
                        request,
                        self.setting_text(key.name),
                        json!({ "key": key.name, "value": self.setting_text(key.name), "accepts": key.accepts }),
                    ),
                    None => match self.pane_setting(&name) {
                        Some((_, measure)) => ok(
                            request,
                            self.setting_text(&name),
                            json!({
                                "key": name,
                                "value": self.setting_text(&name),
                                "accepts": measure.accepts(),
                            }),
                        ),
                        None => match self.plugin_setting_value(&name) {
                            Some(value) => ok(
                                request,
                                value.clone(),
                                json!({
                                    "key": name,
                                    "value": value,
                                    "accepts": "whatever this plugin's own configuration takes",
                                }),
                            ),
                            None => no(request, code::NOT_FOUND, unknown_setting(&name)),
                        },
                    },
                }
            }
            "set" => self.cli_settings_set(request),
            "reset" => self.cli_settings_reset(request),
            "fonts" => {
                let limit = request.whole("limit").unwrap_or(100);
                let families: Vec<String> =
                    self.renderer.families().iter().take(limit).cloned().collect();
                lines(
                    request,
                    format!(
                        "{} families, {} shown",
                        self.renderer.families().len(),
                        families.len()
                    ),
                    families.clone(),
                    json!({ "families": families, "total": self.renderer.families().len() }),
                )
            }
            _ => unknown(request),
        }
    }

    /// `mcp status`: what this window is doing about the Model Context Protocol.
    ///
    /// The window is the only thing that knows, which is why it is the one `mcp` command that is not
    /// answered by the client. It reports what the settings say **and** what is actually happening,
    /// because those two come apart in the two cases that matter: a port another Unluminous is holding,
    /// and a window started with `--control off`.
    pub(crate) fn cli_mcp(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "status" => {
                let state = self
                    .mcp
                    .as_ref()
                    .map(|hosted| hosted.state().clone())
                    .unwrap_or(crate::services::mcp::State::Off);
                let shape = self.settings.mcp_tools;
                // What is really offered, not what the catalogue holds: `mcp.areas` may have cut it
                // down, and a count that ignored that would be the one number here nobody could act
                // on. `task-1804` §4.2.
                let areas = self.settings.mcp_area_filter();
                let tools = unluminous_cli::mcp::tools::tools_in(shape, &areas).len();
                let every = unluminous_cli::mcp::tools::tools(shape).len();
                let endpoint = state.port().map(unluminous_cli::mcp::endpoint);
                ok(
                    request,
                    state.message(),
                    json!({
                        "state": state.name(),
                        "enabled": self.settings.mcp_enabled,
                        "port": self.settings.mcp_port,
                        "endpoint": endpoint,
                        "tools": {
                            "shape": shape.name(),
                            "count": tools,
                            "areas": areas.names(),
                            // What it would be with every area, so the saving is a number rather
                            // than something to work out.
                            "countWithEveryArea": every,
                        },
                        "controlChannel": self.control.is_some(),
                        // What an agent that launches the server itself should be told to run. It is
                        // the answer whether or not anything is listening, which is the point of it.
                        "stdio": {
                            "command": unluminous_cli::mcp::install::unluminous_cli_program().to_string_lossy(),
                            "arguments": ["mcp", "serve"],
                        },
                    }),
                )
            }
            _ => unknown(request),
        }
    }

    /// What a plugin's own file holds for this key, when it is one and it has a value.
    fn plugin_setting_value(&self, name: &str) -> Option<String> {
        let key = self.plugin_setting(name)?;
        let folder = self.plugin_folder()?;
        crate::services::plugin_settings::values(&folder, &key.plugin)
            .text(&key.key)
            .map(str::to_owned)
    }

    /// Write one key into a plugin's own file and make the change take effect.
    ///
    /// **The plugins are read again afterwards**, which is what makes this a change rather than an
    /// edit to a file: a provider holds its configuration in memory and re-reads it when the plugin
    /// is reloaded, so without this an agent would set a value, see it in `settings get`, and find
    /// the pane still behaving the way it did. `plugins reload` is the same call a person makes.
    ///
    /// **And what that reload said is reported.** `task-1922` B14: the reload's problems were thrown
    /// away with `let _ =`, so a value that was written and then could not be read back -- which is
    /// what a manifest this write has just made invalid looks like -- answered `ok`. The write really
    /// did happen, so this is not a refusal of the write; it is the reload's own first problem,
    /// carried up so the reply says the value is in the file and the plugin did not come back.
    fn set_plugin_setting(&mut self, name: &str, value: &str) -> Result<(), String> {
        let key = self
            .plugin_setting(name)
            .ok_or_else(|| format!("{name} is not a plugin's setting."))?;
        let folder = self.plugin_folder().ok_or_else(|| {
            "This window has no settings folder, so a plugin has none.".to_owned()
        })?;
        crate::services::plugin_settings::write(&folder, &key.plugin, &key.key, value)
            .map_err(|problem| format!("{name} could not be written: {problem}"))?;
        match self.reload_the_plugins().first() {
            Some(problem) => Err(format!(
                "{name} was written, and reading the plugins again did not work: {problem}"
            )),
            None => Ok(()),
        }
    }

    fn cli_settings_set(&mut self, request: &Request) -> Outcome {
        let (Some(name), Some(value)) = (request.text("key"), request.text("value")) else {
            return no(request, code::USAGE, "Say a setting and a value.");
        };
        // A plugin's own configuration is written into the plugin's own file rather than into the
        // window's, and the plugins are read again afterwards so the change takes effect in the pane
        // rather than only on disk. `task-1804` §4.2.
        if let Some(key) = self.plugin_setting(&name) {
            let before = self.plugin_setting_value(&name).unwrap_or_default();
            if let Err(problem) = self.set_plugin_setting(&name, value.trim()) {
                return no(request, code::FAILED, problem);
            }
            let after = self.plugin_setting_value(&name).unwrap_or_default();
            return ok(
                request,
                format!("{name} is now {after} (was {before})"),
                json!({ "key": name, "plugin": key.plugin, "value": after, "was": before }),
            );
        }
        if !SETTINGS.iter().any(|key| key.name == name) && self.pane_setting(&name).is_none() {
            return no(request, code::NOT_FOUND, unknown_setting(&name));
        }
        let before = self.setting_text(&name);
        if let Err(problem) = self.apply_setting(&name, value.trim()) {
            return no(request, code::USAGE, problem);
        }
        let after = self.setting_text(&name);
        ok(
            request,
            format!("{name} is now {after} (was {before})"),
            json!({ "key": name, "value": after, "was": before }),
        )
    }

    fn cli_settings_reset(&mut self, request: &Request) -> Outcome {
        let fresh = crate::settings::Settings {
            // The family a fresh Unluminous has is decided by what this machine has installed, so it is
            // the renderer's answer rather than an empty string, which would show as nothing.
            font_family: self.renderer.default_family(),
            ..crate::settings::Settings::new()
        };
        match request.text("key") {
            Some(name) => {
                // A contributed pane goes back to what its **manifest** asked for rather than to a
                // number in Unluminous, which is what "what a new Unluminous has" means for a pane Unluminous did
                // not write. Reset has to know these keys because `settings list` names them and
                // `set` takes them: one of the three refusing would be the fault `task-1794`
                // reported, moved one level down.
                if let Some((slot, measure)) = self.pane_setting(&name) {
                    let value = match self.plugin_ui.pane(slot) {
                        Some(pane) => match measure {
                            PaneMeasure::Width => format!("{:.0}", pane.width),
                            PaneMeasure::Height => format!("{:.0}", pane.height),
                            PaneMeasure::Zoom => format!("{:.2}", settings::DEFAULT_ZOOM),
                        },
                        None => return no(request, code::NOT_FOUND, unknown_setting(&name)),
                    };
                    if let Err(problem) = self.apply_setting(&name, &value) {
                        return no(request, code::FAILED, problem);
                    }
                    return ok(
                        request,
                        format!("{name} is back to {value}"),
                        json!({ "key": name, "value": value }),
                    );
                }
                if !SETTINGS.iter().any(|key| key.name == name) {
                    return no(request, code::NOT_FOUND, unknown_setting(&name));
                }
                let value = fresh_value(&name, &fresh);
                if let Err(problem) = self.apply_setting(&name, &value) {
                    return no(request, code::FAILED, problem);
                }
                ok(
                    request,
                    format!("{name} is back to {value}"),
                    json!({ "key": name, "value": value }),
                )
            }
            None => {
                self.set_settings(fresh);
                self.panes = crate::settings::Panes::new();
                // A fresh `Panes` carries a fresh `Layout`, which has no contributed panes in it —
                // so without this the panes the plugins contribute are not reset, they are lost, and
                // every command goes on saying they are showing while nothing is drawn. `task-1794`,
                // the same fault `reset_the_panel_layout` had.
                self.place_the_plugin_panes(false);
                self.unsaved_settings = true;
                ok(
                    request,
                    "Every setting is back to what a new Unluminous has.",
                    self.settings_value(),
                )
            }
        }
    }

    /// One setting as text, which is how `settings get` and `settings list` show it and how the
    /// settings file spells it.
    fn setting_text(&self, name: &str) -> String {
        match name {
            "appearance.font.family" => self.settings.font_family.clone(),
            "appearance.font.size" => format!("{:.0}", self.settings.font_size),
            "appearance.background.opacity" => format!("{:.3}", self.settings.opacity),
            "appearance.theme" => self.settings.theme.clone(),
            "appearance.accent" => self.settings.accent.clone(),
            "appearance.icons" => self.settings.icons.clone(),
            "appearance.ui.font.family" => self.settings.ui_font_family.clone(),
            "appearance.ui.font.size" => format!("{:.1}", self.settings.ui_font_size),
            "terminal.font.size" => format!("{:.0}", self.settings.terminal_font_size),
            "terminal.shell" => self.settings.terminal_shell.clone(),
            "editor.line_numbers" => self.settings.line_numbers.to_string(),
            "editor.indent" => self.settings.indent.name(),
            "editor.auto_indent" => self.settings.auto_indent.to_string(),
            "editor.trim" => self.settings.trim_on_save.to_string(),
            "editor.suggestions" => self.settings.suggestions.name().to_owned(),
            "editor.line_ending" => self.settings.line_endings.name().to_owned(),
            "update.check" => self.settings.update_check.name().to_owned(),
            "editor.exclude" => self.settings.exclude.clone(),
            "debug.value_tooltip" => self.settings.value_tooltip.name().to_owned(),
            "plugins.chrome" => self.settings.plugin_chrome.to_string(),
            "mcp.enabled" => self.settings.mcp_enabled.to_string(),
            "mcp.port" => self.settings.mcp_port.to_string(),
            "mcp.tools" => self.settings.mcp_tools.name().to_owned(),
            "mcp.areas" => self.settings.mcp_areas.clone(),
            "debug.lldb" => self.settings.debug_adapter("lldb").unwrap_or_default().to_owned(),
            "debug.node" => self.settings.debug_adapter("node").unwrap_or_default().to_owned(),
            "panes.explorer.width" => format!("{:.0}", self.panes.explorer_width),
            "panes.terminal.height" => format!("{:.0}", self.panes.terminal_height),
            "panes.preview.fraction" => format!("{:.3}", self.panes.preview_fraction),
            "panes.find.split" => format!("{:.3}", self.panes.find_split),
            other => match self.pane_setting(other) {
                Some((slot, measure)) => measure.read(&self.panes, slot),
                None => String::new(),
            },
        }
    }

    /// Put a setting into effect, or say why the value will not do.
    ///
    /// Every value is brought inside its limits rather than refused, which is what the Settings
    /// window does with a slider and what `Settings::read_from` does with a hand edited file. What
    /// is refused is a value that is not a number at all, because that is a mistake rather than an
    /// extreme.
    fn apply_setting(&mut self, name: &str, value: &str) -> Result<(), String> {
        let number = || {
            value
                .parse::<f32>()
                .map_err(|_| format!("{name} wants a number, and {value} is not one."))
        };
        let flag = || match value.to_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Ok(true),
            "false" | "no" | "off" | "0" => Ok(false),
            _ => Err(format!("{name} wants true or false, and {value} is neither.")),
        };
        let mut settings = self.settings.clone();
        match name {
            "appearance.font.family" => {
                if !self.renderer.families().iter().any(|family| family == value) {
                    return Err(format!(
                        "This machine has no font called {value}. `settings fonts` lists them."
                    ));
                }
                settings.font_family = value.to_owned();
            }
            "appearance.font.size" => {
                settings.font_size =
                    number()?.clamp(settings::MIN_FONT_SIZE, settings::MAX_FONT_SIZE)
            }
            "appearance.theme" => {
                // Named by key or by the name on the screen, exactly as `theme set` takes it, and
                // remembered as the **key** so a settings file written by one route reads the same to the
                // other. Empty is Unluminous's own, which is what the file says by saying nothing.
                if value.trim().is_empty() {
                    settings.theme = String::new();
                } else {
                    let Some(theme) = self.plugins.theme(value) else {
                        let names: Vec<String> =
                            self.plugins.themes().into_iter().map(|theme| theme.name).collect();
                        return Err(format!(
                            "There is no theme called {value}. There is {}.",
                            names.join(", ")
                        ));
                    };
                    settings.theme = match theme.key == crate::theme::Theme::unluminous_dark().key {
                        true => String::new(),
                        false => theme.key,
                    };
                }
            }
            "appearance.accent" => {
                if value.trim().is_empty() {
                    settings.accent = String::new();
                } else if crate::services::plugins::colour(value).is_some() {
                    settings.accent = value.trim().to_uppercase();
                } else {
                    return Err(format!(
                        "{value} is not a colour. Write one as #RRGGBB, or leave it empty for the theme's own."
                    ));
                }
            }
            "appearance.icons" => {
                if value.trim().is_empty() {
                    settings.icons = String::new();
                } else if let Some(set) = crate::theme::IconSet::parse(value) {
                    settings.icons = set.name().to_owned();
                } else {
                    return Err(format!(
                        "There is no icon set called {value}. Unluminous draws {}, and empty follows the theme.",
                        crate::services::plugins::ICON_SETS.join(" and ")
                    ));
                }
            }
            "appearance.ui.font.family" => {
                if !value.trim().is_empty()
                    && !self.renderer.families().iter().any(|family| family == value)
                {
                    return Err(format!(
                        "This machine has no font called {value}. `settings fonts` lists them."
                    ));
                }
                settings.ui_font_family = value.trim().to_owned();
            }
            "appearance.ui.font.size" => {
                settings.ui_font_size =
                    number()?.clamp(settings::MIN_UI_FONT_SIZE, settings::MAX_UI_FONT_SIZE)
            }
            "appearance.background.opacity" => {
                settings.opacity = number()?.clamp(settings::MIN_OPACITY, 1.0)
            }
            "terminal.font.size" => settings.terminal_font_size = number()?.clamp(6.0, 48.0),
            // Not checked against the machine the way a font family is: a shell may be a bare name to
            // be found on the path, an absolute path, or something installed a moment from now. When
            // it is wrong the tile says so in the shell's own words, which is `Tabs::open`'s answer
            // and is a better message than one made up here.
            "terminal.shell" => settings.terminal_shell = value.trim().to_owned(),
            "editor.line_numbers" => settings.line_numbers = flag()?,
            "editor.indent" => {
                settings.indent = crate::settings::Indent::parse(value).ok_or_else(|| {
                    format!(
                        "{name} wants tabs or spaces:N with N from {} to {}, and {value} is neither.",
                        crate::settings::Indent::MIN_WIDTH,
                        crate::settings::Indent::MAX_WIDTH
                    )
                })?
            }
            "editor.auto_indent" => settings.auto_indent = flag()?,
            "editor.trim" => settings.trim_on_save = flag()?,
            "editor.suggestions" => {
                settings.suggestions =
                    crate::settings::Suggestions::parse(value).ok_or_else(|| {
                        format!("{name} wants automatic or manual, and {value} is neither.")
                    })?
            }
            "editor.line_ending" => {
                settings.line_endings =
                    crate::settings::LineEndings::parse(value).ok_or_else(|| {
                        format!("{name} wants keep, lf or crlf, and {value} is none of them.")
                    })?
            }
            "update.check" => {
                settings.update_check = crate::settings::UpdateCheck::parse(value)
                    .ok_or_else(|| format!("{name} wants off or start, and {value} is neither."))?
            }
            // Not checked, for `terminal.shell`'s reason turned round: a pattern naming nothing in
            // this project today may name something tomorrow, and a line that matches nothing costs
            // nothing. A pattern that will not parse at all is skipped by the reader with the rest
            // of the line kept, which is what a `.gitignore` comment does.
            "editor.exclude" => settings.exclude = value.trim().to_owned(),
            "debug.value_tooltip" => {
                settings.value_tooltip =
                    crate::settings::ValueTooltip::parse(value).ok_or_else(|| {
                        format!("{name} wants automatic or manual, and {value} is neither.")
                    })?
            }
            // Not checked against the machine, for `terminal.shell`'s reason: a path may name
            // something installed a moment from now, and when it is wrong the status bar says so in
            // the adapter's own words, which is a better message than one made up here.
            "debug.lldb" | "debug.node" => {
                let entry = name.trim_start_matches("debug.").to_owned();
                settings.debug_adapters.retain(|(known, _)| *known != entry);
                let path = value.trim();
                if !path.is_empty() {
                    settings.debug_adapters.push((entry, path.to_owned()));
                }
                // Kept in the order `plugins::DEBUGGERS` names them, so the settings file is written
                // the same way whichever order they were set in.
                settings.debug_adapters.sort_by_key(|(known, _)| {
                    crate::services::plugins::DEBUGGERS
                        .iter()
                        .position(|name| name == known)
                        .unwrap_or(usize::MAX)
                });
            }
            "plugins.chrome" => settings.plugin_chrome = flag()?,
            "mcp.enabled" => settings.mcp_enabled = flag()?,
            "mcp.port" => settings.mcp_port = crate::settings::clamp_port(number()?),
            "mcp.tools" => {
                settings.mcp_tools = unluminous_cli::mcp::Shape::parse(value).ok_or_else(|| {
                    format!("{name} wants grouped or every, and {value} is neither.")
                })?
            }
            // Checked here, unlike when it is read off the disk: a person or an agent setting it now
            // can be told, and `Settings::mcp_area_filter` explains why the two differ.
            "mcp.areas" => {
                unluminous_cli::mcp::tools::Areas::parse(value)
                    .map_err(|problem| format!("{name}: {problem}"))?;
                settings.mcp_areas = value.trim().to_owned();
            }
            "panes.explorer.width" => {
                self.panes.explorer_width =
                    number()?.clamp(settings::EXPLORER_MIN, settings::EXPLORER_MAX);
                self.unsaved_settings = true;
                return Ok(());
            }
            "panes.terminal.height" => {
                self.panes.terminal_height = number()?.max(settings::TERMINAL_MIN);
                self.unsaved_settings = true;
                return Ok(());
            }
            "panes.preview.fraction" => {
                self.panes.preview_fraction = number()?.clamp(0.15, 0.85);
                self.unsaved_settings = true;
                return Ok(());
            }
            "panes.find.split" => {
                self.panes.find_split = number()?.clamp(
                    crate::components::find_in_files::SPLIT_MIN,
                    crate::components::find_in_files::SPLIT_MAX,
                );
                self.unsaved_settings = true;
                return Ok(());
            }
            other => {
                let Some((slot, measure)) = self.pane_setting(other) else {
                    return Err(unknown_setting(name));
                };
                // The same clamps `Panes::read_from_with` puts on a hand edited file, so a value
                // typed here and a value written into `settings.conf` are read the same way.
                measure.write(&mut self.panes, slot, number()?);
                self.unsaved_settings = true;
                return Ok(());
            }
        }
        self.set_settings(settings);
        Ok(())
    }

    pub(crate) fn settings_value(&self) -> Value {
        let mut map = Map::new();
        for key in SETTINGS {
            map.insert(
                key.name.to_owned(),
                json!({ "value": self.setting_text(key.name), "accepts": key.accepts, "help": key.help }),
            );
        }
        Value::Object(map)
    }

    /// `theme` — what the whole window is painted in — `task-1776`.
    ///
    /// `settings set appearance.theme` reaches the same code, because `apply_the_theme` is the one place a
    /// theme becomes a change. These exist because a setting can be written and cannot be **discovered**:
    /// `settings list` can say what `appearance.theme` holds, and nothing but this can say what it will
    /// accept — and the study in `CLAUDE.md` measured what an agent does when it cannot discover a thing,
    /// which is to reach for `bash` instead.
    pub(crate) fn cli_theme(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "list" => {
                let active = crate::theme::active().key;
                let themes = self.plugins.themes();
                let rows: Vec<String> = themes
                    .iter()
                    .map(|theme| {
                        format!(
                            "{}{:<28}{:<18}{}",
                            if theme.key == active { "*" } else { " " },
                            theme.key,
                            theme.plugin,
                            theme.name
                        )
                    })
                    .collect();
                let value: Vec<Value> = themes
                    .iter()
                    .map(|theme| {
                        json!({
                            "key": theme.key,
                            "name": theme.name,
                            "plugin": theme.plugin,
                            "dark": theme.dark,
                            "icons": theme.icons.name(),
                            "active": theme.key == active,
                            // The six the Settings page draws as swatches, which is what a theme is
                            // recognised by. The whole palette is `theme show`, so a list of six themes
                            // does not answer with two hundred and forty colours.
                            "colours": json!({
                                "editor": Self::hex_colour(theme.palette.editor),
                                "explorer": Self::hex_colour(theme.palette.explorer),
                                "accent": Self::hex_colour(theme.palette.accent),
                                "added": Self::hex_colour(theme.palette.git_added),
                                "unsaved": Self::hex_colour(theme.palette.unsaved),
                                "close": Self::hex_colour(theme.palette.close),
                            }),
                            "colours_the_tokens": theme.syntax.is_some(),
                        })
                    })
                    .collect();
                lines(
                    request,
                    format!("{} themes, and {active} is the one showing", themes.len()),
                    rows,
                    json!({ "themes": value }),
                )
            }
            "show" => {
                let named = request.text("theme");
                let theme = match &named {
                    Some(wanted) => match self.plugins.theme(wanted) {
                        Some(theme) => theme,
                        None => return self.no_such_theme(request, wanted),
                    },
                    None => crate::theme::active(),
                };
                let mut colours = serde_json::Map::new();
                for role in crate::theme::Palette::NAMES {
                    if let Some(colour) = theme.palette.get(role) {
                        colours.insert((*role).to_owned(), Value::String(Self::hex_colour(colour)));
                    }
                }
                let syntax = theme.syntax.as_ref().map(|scheme| {
                    let mut named = serde_json::Map::new();
                    for token in unluminous_core::Token::ALL {
                        if let Some(colour) = scheme.colour(token) {
                            named.insert(
                                token.name().to_owned(),
                                Value::String(format!(
                                    "#{:02X}{:02X}{:02X}",
                                    colour.r, colour.g, colour.b
                                )),
                            );
                        }
                    }
                    Value::Object(named)
                });
                let rows: Vec<String> = crate::theme::Palette::NAMES
                    .iter()
                    .filter_map(|role| {
                        theme
                            .palette
                            .get(role)
                            .map(|colour| format!("{role:<18}{}", Self::hex_colour(colour)))
                    })
                    .collect();
                lines(
                    request,
                    format!(
                        "{} \u{2014} {} icons, {}",
                        theme.name,
                        theme.icons.name(),
                        match theme.syntax.is_some() {
                            true => "and it colours the tokens",
                            false => "and each language plugin colours its own files",
                        }
                    ),
                    rows,
                    json!({
                        "key": theme.key,
                        "name": theme.name,
                        "plugin": theme.plugin,
                        "dark": theme.dark,
                        "icons": theme.icons.name(),
                        "colours": Value::Object(colours),
                        "syntax": syntax,
                    }),
                )
            }
            "set" => {
                let Some(wanted) = request.text("theme") else {
                    return no(request, code::USAGE, "Say which theme.");
                };
                let Some(theme) = self.plugins.theme(&wanted) else {
                    return self.no_such_theme(request, &wanted);
                };
                // Every refusal happens before anything is changed, so a command line with a good theme
                // and a bad accent in it leaves the window exactly as it was rather than half applied.
                // That is the rule `unknown_argument_refusal` keeps for the whole command surface.
                let mut settings = self.settings.clone();
                // Unluminous's own is remembered as an empty setting rather than as its key, so a settings file
                // that has never chosen a theme goes on saying nothing — `terminal.shell`'s rule.
                settings.theme = match theme.key == crate::theme::Theme::unluminous_dark().key {
                    true => String::new(),
                    false => theme.key.clone(),
                };
                if let Some(accent) = request.text("accent") {
                    let accent = accent.trim();
                    if accent.eq_ignore_ascii_case("none") {
                        settings.accent = String::new();
                    } else if crate::services::plugins::colour(accent).is_some() {
                        settings.accent = accent.to_uppercase();
                    } else {
                        return no(
                            request,
                            code::USAGE,
                            format!("{accent} is not a colour. Write one as #RRGGBB, or `none` for the theme's own."),
                        );
                    }
                }
                if let Some(icons) = request.text("icons") {
                    let icons = icons.trim();
                    if icons.eq_ignore_ascii_case("follow") {
                        settings.icons = String::new();
                    } else if let Some(set) = crate::theme::IconSet::parse(icons) {
                        settings.icons = set.name().to_owned();
                    } else {
                        return no(
                            request,
                            code::USAGE,
                            format!(
                                "There is no icon set called {icons}. Unluminous draws {}, or `follow` for whichever the theme names.",
                                crate::services::plugins::ICON_SETS.join(" and ")
                            ),
                        );
                    }
                }
                // Down the window's own way in, so a theme chosen from the command line, one chosen in
                // Settings and one set through `settings set` are the same change — and the file is
                // written on the same terms as every other setting.
                self.set_settings(settings);
                ok(
                    request,
                    format!("The window is painted in {}", theme.name),
                    json!({
                        "key": theme.key,
                        "name": theme.name,
                        "icons": crate::theme::active().icons.name(),
                        "accent": match self.settings.accent.is_empty() {
                            true => Value::Null,
                            false => Value::String(self.settings.accent.clone()),
                        },
                    }),
                )
            }
            _ => no(
                request,
                code::UNKNOWN_COMMAND,
                format!("There is no theme command called {verb}."),
            ),
        }
    }

    /// A colour written the way a manifest and the settings file write one, which is what a caller reads
    /// back and puts into `theme set --accent`.
    fn hex_colour(colour: egui::Color32) -> String {
        format!("#{:02X}{:02X}{:02X}", colour.r(), colour.g(), colour.b())
    }

    /// A refusal that says what there is, rather than only that this is not one of them.
    fn no_such_theme(&self, request: &Request, wanted: &str) -> Outcome {
        let names: Vec<String> =
            self.plugins.themes().into_iter().map(|theme| theme.name).collect();
        no(
            request,
            code::NOT_FOUND,
            format!("There is no theme called {wanted}. There is {}.", names.join(", ")),
        )
    }
}

/// One setting the command line can read and change.
struct SettingKey {
    name: &'static str,
    accepts: &'static str,
    help: &'static str,
}

/// One of the three measurements a contributed pane keeps in the settings file.
///
/// `task-1794`: Unluminous wrote `panes.agent-chat/chat.width` itself and then refused it back, because
/// `SETTINGS` is a `const` and a pane's key is not known until the manifests are read. This is the
/// one place the three are described, so `settings list`, `get`, `set` and `reset` cannot come to
/// different conclusions about what a pane's width is or what it will accept — the rule
/// `Panes::width_of` already keeps about which of a pair a side reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaneMeasure {
    Width,
    Height,
    Zoom,
}

impl PaneMeasure {
    const ALL: [PaneMeasure; 3] = [PaneMeasure::Width, PaneMeasure::Height, PaneMeasure::Zoom];

    fn name(self) -> &'static str {
        match self {
            PaneMeasure::Width => "width",
            PaneMeasure::Height => "height",
            PaneMeasure::Zoom => "zoom",
        }
    }

    fn accepts(self) -> &'static str {
        match self {
            PaneMeasure::Width => "points, brought inside the range a column may have",
            PaneMeasure::Height => "points, brought inside the range a strip may have",
            PaneMeasure::Zoom => "0.5 to 3.0",
        }
    }

    fn help(self, label: &str) -> String {
        match self {
            PaneMeasure::Width => {
                format!("How wide {label} is when it is a column at the left or the right.")
            }
            PaneMeasure::Height => {
                format!("How tall {label} is when it is in a strip along the top or the bottom.")
            }
            PaneMeasure::Zoom => {
                format!("How much bigger than usual {label} draws everything in it.")
            }
        }
    }

    /// A pane's own panel, which is what `Panes` is keyed by.
    fn panel(slot: usize) -> dock::Panel {
        dock::Panel::Plugin(slot as u8)
    }

    fn read(self, panes: &settings::Panes, slot: usize) -> String {
        let panel = Self::panel(slot);
        match self {
            PaneMeasure::Width => format!("{:.0}", panes.width_of(panel)),
            PaneMeasure::Height => format!("{:.0}", panes.height_of(panel)),
            PaneMeasure::Zoom => format!("{:.2}", panes.zoom_of(panel)),
        }
    }

    /// Brought inside its limits rather than refused, which is `apply_setting`'s own rule and what
    /// `Panes::read_from_with` does with the same value read from a hand edited file.
    fn write(self, panes: &mut settings::Panes, slot: usize, value: f32) {
        let panel = Self::panel(slot);
        match self {
            PaneMeasure::Width => {
                let width = value.clamp(panes.min_width_of(panel), panes.max_width_of(panel));
                panes.set_width_of(panel, width);
            }
            PaneMeasure::Height => {
                panes.set_height_of(panel, value.max(panes.min_height_of(panel)));
            }
            PaneMeasure::Zoom => panes.set_zoom_of(panel, value),
        }
    }
}

/// Every setting, by the name it has in Unluminous's own settings file.
///
/// The same names, deliberately. Somebody who has looked in `settings.conf` already knows them, and
/// a second vocabulary for the same nine values would be a second thing to learn and a second thing
/// to keep in step.
const SETTINGS: &[SettingKey] = &[
    SettingKey {
        name: "appearance.font.family",
        accepts: "a family this machine has; `settings fonts` lists them",
        help: "The family the editor sets text in.",
    },
    SettingKey {
        name: "appearance.font.size",
        accepts: "6 to 144",
        help: "The point size the editor sets text in, in every tab.",
    },
    SettingKey {
        name: "appearance.background.opacity",
        accepts: "0.05 to 1.0",
        help: "How opaque the window is. Below 1 the desktop shows through.",
    },
    SettingKey {
        name: "appearance.theme",
        accepts: "a theme's key or its name; `theme list` names them, and empty is Unluminous Dark",
        help: "What every colour in the window is. A theme that names the nine token colours also colours code, in every language at once.",
    },
    SettingKey {
        name: "appearance.accent",
        accepts: "#RRGGBB, or empty for the theme's own",
        help: "One colour for everything the accent means: the caret, the open tab, an open folder.",
    },
    SettingKey {
        name: "appearance.icons",
        accepts: "material, classic, or empty for whichever the theme names",
        help: "Which drawn marks the rail buttons and the explorer's folder arrow use.",
    },
    SettingKey {
        name: "appearance.ui.font.family",
        accepts: "a family this machine has, or empty for the editor's",
        help: "The family the window's own text is set in: the menus, the rail and the status bar.",
    },
    SettingKey {
        name: "appearance.ui.font.size",
        accepts: "8 to 24",
        help: "The point size the window's own text is set in. The editing area keeps its own.",
    },
    SettingKey {
        name: "terminal.font.size",
        accepts: "6 to 48",
        help: "The point size the terminal sets its grid in.",
    },
    SettingKey {
        name: "terminal.shell",
        accepts: "a program, or empty for this machine's own",
        help: "What each terminal tab runs. Empty means PowerShell on Windows and $SHELL elsewhere.",
    },
    SettingKey {
        name: "editor.line_numbers",
        accepts: "true or false",
        help: "Whether the editing area has a column of line numbers.",
    },
    SettingKey {
        name: "editor.indent",
        accepts: "tabs or spaces:N, N from 2 to 8",
        help: "What one indent is made of, which is what the Tab key types where nothing is selected. Tabs, which is what it has always typed. Indenting a selection is still one character a line, because unluminous-core's indent unit is a character.",
    },
    SettingKey {
        name: "editor.auto_indent",
        accepts: "true or false",
        help: "Whether a new line starts with the indentation of the line it was started from.",
    },
    SettingKey {
        name: "editor.trim",
        accepts: "true or false",
        help: "Whether the trailing whitespace goes off every line when a file is written. Off. It never runs on a Markdown file, where two trailing spaces are a line break.",
    },
    SettingKey {
        name: "editor.suggestions",
        accepts: "automatic or manual",
        help: "Whether the completion popup arrives as you type. Ctrl+Space works either way.",
    },
    SettingKey {
        name: "editor.line_ending",
        accepts: "keep, lf or crlf",
        help: "What line breaks a file is written back with. `keep` writes it the way it was read, which is what leaves a one character edit as a one line diff. A new file gets the platform's own either way.",
    },
    SettingKey {
        name: "update.check",
        accepts: "off or start",
        help: "Whether Unluminous asks the releases page for a newer version when it opens. Off, and it asks nothing until somebody presses Check for Updates or runs `update check`. It never installs anything either way.",
    },
    SettingKey {
        name: "editor.exclude",
        accepts: "comma separated .gitignore patterns",
        help: "Patterns Go to File, Find in Files, completion, Go to Definition and Find References leave out, beside the project's own .gitignore, which is read already. The explorer goes on showing everything.",
    },
    SettingKey {
        name: "debug.value_tooltip",
        accepts: "automatic or manual",
        help: "Whether resting the pointer on a name while the program is stopped shows its value. Show Value on the Debug menu works either way.",
    },
    SettingKey {
        name: "plugins.chrome",
        accepts: "true or false",
        help: "Whether a plugin that asked for it draws depth: the soft shadows, gradients and pressed edges behind its own pane. Off, it draws flat.",
    },
    SettingKey {
        name: "mcp.enabled",
        accepts: "true or false",
        help: "Whether this Unluminous serves MCP over HTTP. An agent that launches the server itself needs neither this nor a port.",
    },
    SettingKey {
        name: "mcp.port",
        accepts: "1024 to 65535",
        help: "The port it serves on when it does.",
    },
    SettingKey {
        name: "mcp.areas",
        accepts: "comma separated area names, or empty for all of them",
        help: "Which areas of the catalogue the MCP server offers, so an agent is not handed the whole of it. Empty means all of them. `mcp tools --count --areas editor,git` says what a choice costs; the whole catalogue is about 18 per cent of a 96k context window before a question is asked.",
    },
    SettingKey {
        name: "mcp.tools",
        accepts: "grouped or every",
        help: "One tool an area, or one tool a command. `mcp tools --count` says what each costs.",
    },
    SettingKey {
        name: "debug.lldb",
        accepts: "a path to lldb-dap or codelldb, or empty for whatever is on PATH",
        help: "Where the LLDB adapter lives, for Rust and native code. Empty means Unluminous looks for codelldb then lldb-dap on PATH. `tools/get-debug-adapter.ps1` fetches one and prints the line.",
    },
    SettingKey {
        name: "debug.node",
        accepts: "a path to js-debug's dapDebugServer.js",
        help: "Where js-debug lives, for JavaScript and TypeScript. There is no default: js-debug is a script rather than a program, so Unluminous has nothing to look for until it is told.",
    },
    SettingKey {
        name: "panes.explorer.width",
        accepts: "150 to 620",
        help: "How wide the file explorer is.",
    },
    SettingKey {
        name: "panes.terminal.height",
        accepts: "90 upwards",
        help: "How tall the terminal tile is.",
    },
    SettingKey {
        name: "panes.preview.fraction",
        accepts: "0.15 to 0.85",
        help: "How much of the side by side view the source takes.",
    },
    SettingKey {
        name: "panes.find.split",
        accepts: "0.15 to 0.85",
        help: "How much of Find in Files the results take.",
    },
];

fn unknown_setting(name: &str) -> String {
    format!("There is no setting called {name}. `settings list` names them all.")
}

/// What a setting is in an Unluminous that has never been run.
fn fresh_value(name: &str, fresh: &crate::settings::Settings) -> String {
    let panes = crate::settings::Panes::new();
    match name {
        "appearance.font.family" => fresh.font_family.clone(),
        "appearance.font.size" => format!("{:.0}", fresh.font_size),
        "appearance.background.opacity" => format!("{:.3}", fresh.opacity),
        "appearance.theme" => fresh.theme.clone(),
        "appearance.accent" => fresh.accent.clone(),
        "appearance.icons" => fresh.icons.clone(),
        "appearance.ui.font.family" => fresh.ui_font_family.clone(),
        "appearance.ui.font.size" => format!("{:.1}", fresh.ui_font_size),
        "terminal.font.size" => format!("{:.0}", fresh.terminal_font_size),
        "terminal.shell" => fresh.terminal_shell.clone(),
        "editor.line_numbers" => fresh.line_numbers.to_string(),
        "editor.indent" => fresh.indent.name(),
        "editor.auto_indent" => fresh.auto_indent.to_string(),
        "editor.trim" => fresh.trim_on_save.to_string(),
        "editor.suggestions" => fresh.suggestions.name().to_owned(),
        "editor.line_ending" => fresh.line_endings.name().to_owned(),
        "update.check" => fresh.update_check.name().to_owned(),
        "editor.exclude" => fresh.exclude.clone(),
        "debug.value_tooltip" => fresh.value_tooltip.name().to_owned(),
        "plugins.chrome" => fresh.plugin_chrome.to_string(),
        "mcp.enabled" => fresh.mcp_enabled.to_string(),
        "mcp.port" => fresh.mcp_port.to_string(),
        "mcp.tools" => fresh.mcp_tools.name().to_owned(),
        "mcp.areas" => fresh.mcp_areas.clone(),
        "panes.explorer.width" => format!("{:.0}", panes.explorer_width),
        "panes.terminal.height" => format!("{:.0}", panes.terminal_height),
        "panes.preview.fraction" => format!("{:.3}", panes.preview_fraction),
        "panes.find.split" => format!("{:.3}", panes.find_split),
        _ => String::new(),
    }
}
