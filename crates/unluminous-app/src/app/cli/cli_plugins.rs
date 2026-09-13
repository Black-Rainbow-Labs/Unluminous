//! `plugins` -- installing, enabling, and driving what the plugins folder holds: a language's
//! grammar, and whatever a plugin's own provider contributes as a pane, a tab or a run command.
//!
//! `plugin_arguments` is reached as `request.text("arguments").map(plugin_arguments)` inside
//! `cli_plugins`'s `"run"` arm -- a function passed by name rather than called with parens, which is
//! why a plain `grep 'plugin_arguments('` finds only its own definition and its tests.

use super::*;

impl UnluminousApp {
    pub(crate) fn cli_plugins(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "list" => {
                let rows: Vec<String> = self
                    .plugins
                    .all()
                    .iter()
                    .map(|plugin| {
                        format!(
                            "{}{:<14}{:<10}{}",
                            if plugin.enabled { "*" } else { " " },
                            plugin.id,
                            plugin.version,
                            plugin.name
                        )
                    })
                    .collect();
                let value: Vec<Value> = self
                    .plugins
                    .all()
                    .iter()
                    .map(|plugin| {
                        json!({
                            "id": plugin.id,
                            "name": plugin.name,
                            "version": plugin.version,
                            "vendor": plugin.vendor,
                            "enabled": plugin.enabled,
                            "bundled": plugin.bundled,
                            "extensions": plugin.extensions,
                            "kind": plugin.kind.name(),
                            "provider": plugin.contributions.provider,
                            "contributes": contributes(plugin),
                        })
                    })
                    .collect();
                lines(
                    request,
                    format!(
                        "{} plugins, {} switched on",
                        self.plugins.all().len(),
                        self.plugins.enabled_count()
                    ),
                    rows,
                    json!({ "plugins": value }),
                )
            }
            "install" => match request.text("id") {
                Some(id) if self.plugins.get(&id).is_some() => {
                    self.install_plugin(&id);
                    ok(
                        request,
                        self.message.clone().unwrap_or_else(|| format!("Installed {id}")),
                        json!({ "id": id }),
                    )
                }
                Some(id) => {
                    no(request, code::NOT_FOUND, format!("There is no plugin called {id}."))
                }
                None => no(request, code::USAGE, "Say which plugin."),
            },
            "enable" | "disable" => {
                let Some(id) = request.text("id") else {
                    return no(request, code::USAGE, "Say which plugin.");
                };
                if self.plugins.get(&id).is_none() {
                    return no(
                        request,
                        code::NOT_FOUND,
                        format!("There is no plugin called {id}."),
                    );
                }
                let on = verb == "enable";
                // Through the window's own way in, so switching a plugin off from the command line
                // and switching it off in the Plugins page are the same thing.
                self.set_plugin_enabled(&id, on);
                ok(
                    request,
                    format!("{id} is switched {}", if on { "on" } else { "off" }),
                    json!({ "id": id, "enabled": on }),
                )
            }
            "show" => {
                let Some(id) = request.text("id") else {
                    return no(request, code::USAGE, "Say which plugin.");
                };
                let Some(plugin) = self.plugins.get(&id).cloned() else {
                    return no(
                        request,
                        code::NOT_FOUND,
                        format!("There is no plugin called {id}."),
                    );
                };
                // Asked of the provider rather than of a list here, so what is printed is what the plugin
                // will actually answer. **Built rather than opened**: `commands` is a question about the
                // code, and opening Agent-Tasks creates a folder and a database file. A read only command
                // that made a database would be a read only command that changed the machine, and it would
                // also break the promise that a provider is opened when its pane, tab or page is first
                // shown.
                let commands: Vec<Value> = plugin
                    .contributions
                    .provider
                    .as_deref()
                    .and_then(crate::services::plugin_ui::provider)
                    .map(|built| {
                        built
                            .commands()
                            .into_iter()
                            .map(|(name, summary)| json!({"command": name, "summary": summary}))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut rows = vec![
                    format!("{:<14}{}", "id", plugin.id),
                    format!("{:<14}{}", "name", plugin.name),
                    format!("{:<14}{}", "kind", plugin.kind.name()),
                    format!("{:<14}{}", "version", plugin.version),
                    format!("{:<14}{}", "vendor", plugin.vendor),
                    format!("{:<14}{}", "enabled", plugin.enabled),
                    format!("{:<14}{}", "contributes", contributes(&plugin).join(", ")),
                ];
                if let Some(problem) = self.plugin_ui.problem_with(&plugin.id) {
                    rows.push(format!("{:<14}{problem}", "problem"));
                }
                for command in &commands {
                    rows.push(format!(
                        "  {:<14}{}",
                        command["command"].as_str().unwrap_or_default(),
                        command["summary"].as_str().unwrap_or_default()
                    ));
                }
                lines(
                    request,
                    format!("{} \u{2014} {}", plugin.id, plugin.description),
                    rows,
                    json!({
                        "id": plugin.id,
                        "name": plugin.name,
                        "kind": plugin.kind.name(),
                        "version": plugin.version,
                        "vendor": plugin.vendor,
                        "description": plugin.description,
                        "limitations": plugin.limitations,
                        "enabled": plugin.enabled,
                        "bundled": plugin.bundled,
                        "extensions": plugin.extensions,
                        "provider": plugin.contributions.provider,
                        "contributes": contributes(&plugin),
                        "commands": commands,
                        "problem": self.plugin_ui.problem_with(&plugin.id),
                    }),
                )
            }
            "reload" => {
                let problems = self.reload_the_plugins();
                let said = match problems.is_empty() {
                    true => format!("{} plugins read again", self.plugins.all().len()),
                    false => format!(
                        "{} plugins read again, {} refused",
                        self.plugins.all().len(),
                        problems.len()
                    ),
                };
                lines(
                    request,
                    said,
                    problems.clone(),
                    json!({ "plugins": self.plugins.all().len(), "refused": problems }),
                )
            }
            "pane" => {
                let Some(pane) = request.text("pane") else {
                    return no(request, code::USAGE, "Say which pane, as <plugin id>/<pane id>.");
                };
                let Some(slot) = self.plugin_ui.slot_of(&pane) else {
                    return no(
                        request,
                        code::NOT_FOUND,
                        format!("There is no {pane} pane. `plugins list` says what each plugin contributes."),
                    );
                };
                let panel = dock::Panel::Plugin(slot as u8);
                if let Some(named) = request.text("side") {
                    let Some(side) = dock::Side::from_name(named.trim()) else {
                        return no(
                            request,
                            code::USAGE,
                            format!("{named} is not a side. Say left, right, top or bottom."),
                        );
                    };
                    self.panes.dock.dock(panel, side, None);
                    self.unsaved_settings = true;
                }
                if request.switch("show") {
                    self.show_the_plugin_pane(&pane, true);
                } else if request.switch("hide") {
                    self.show_the_plugin_pane(&pane, false);
                }
                if let Some(problem) =
                    self.plugin_ui.problem_with(&self.plugin_ui.plugin_of(slot).unwrap_or_default())
                {
                    return no(request, code::FAILED, problem.to_owned());
                }
                // **Switched on is not the same as on the screen**, and answering with the first
                // while meaning the second is what `task-1794` reports: the pane painted nothing at
                // all — no ground, no divider, no composer, no rail highlight — while this said it
                // was showing on the right, and no question an agent could ask reported the
                // difference. So the reply is the two together, and asking for a pane that cannot be
                // drawn is a **refusal** rather than a success about nothing. The cause that was
                // found is fixed; this is what makes the next one say so instead of being silent.
                if !self.plugin_pane_is_reachable(slot) {
                    return no(
                        request,
                        code::FAILED,
                        format!(
                            "{pane} is switched on but the window has no room laid out for it, so it \
                             would draw nothing. `panel reset` puts the panels back."
                        ),
                    );
                }
                let showing = self.plugin_pane_is_showing(slot);
                ok(
                    request,
                    format!(
                        "{pane} is {} on the {}",
                        if showing { "showing" } else { "put away" },
                        self.panes.dock.side_of(panel).name()
                    ),
                    json!({
                        "pane": pane,
                        "showing": showing,
                        "side": self.panes.dock.side_of(panel).name(),
                    }),
                )
            }
            "tab" => {
                let Some(tab) = request.text("tab") else {
                    return no(request, code::USAGE, "Say which tab, as <plugin id>/<tab id>.");
                };
                if self.plugin_ui.surfaces().tab(&tab).is_none() {
                    return no(request, code::NOT_FOUND, format!("There is no {tab} tab."));
                }
                if request.switch("close") {
                    if let Some(index) = self.files.index_of_plugin_tab(&tab) {
                        self.close_tab(index);
                    }
                    return ok(
                        request,
                        format!("{tab} is closed"),
                        json!({"tab": tab, "open": false}),
                    );
                }
                self.open_the_plugin_tab(&tab);
                let open = self.files.index_of_plugin_tab(&tab).is_some();
                match open {
                    true => {
                        ok(request, format!("{tab} is open"), json!({"tab": tab, "open": true}))
                    }
                    false => no(
                        request,
                        code::FAILED,
                        self.message
                            .clone()
                            .unwrap_or_else(|| format!("{tab} could not be opened")),
                    ),
                }
            }
            "run" => {
                let Some(id) = request.text("id") else {
                    return no(request, code::USAGE, "Say which plugin.");
                };
                let Some(command) = request.text("command") else {
                    return no(
                        request,
                        code::USAGE,
                        "Say which command. `plugins show` lists them.",
                    );
                };
                // **Split on spaces only, and do not collapse runs of them.** `split_whitespace` threw
                // away every newline and every repeated space before the plugin saw them, and the
                // provider's own `rest` closure joins the words back with single spaces — so a comment
                // holding a markdown document arrived as one line. Markdown block structure is line
                // based, so a heading swallowed the whole body, and no list, table, fence or quote could
                // survive. An agent asked to post one found it and said so on the ticket rather than
                // being able to do it.
                //
                // A run of n spaces becomes n-1 empty words here and n spaces again when `rest` rejoins
                // them, so indentation is exact rather than nearly right — which is what a nested list
                // needs. Newlines and tabs are inside the words and are not touched at all. The ends are
                // trimmed of spaces so a line with a trailing one does not produce an empty argument,
                // and newlines at the ends are kept because they are the caller's text.
                let arguments: Vec<String> =
                    request.text("arguments").map(plugin_arguments).unwrap_or_default();
                match self.run_plugin_command(&id, &command, &arguments) {
                    Ok(answer) => {
                        let said = match answer.message.is_empty() {
                            true => format!("{id} {command}"),
                            false => answer.message.clone(),
                        };
                        ok(request, said, answer.value)
                    }
                    Err(problem) => no(request, code::FAILED, problem),
                }
            }
            "view" => {
                let Some(id) = request.text("id") else {
                    return no(request, code::USAGE, "Say which plugin.");
                };
                let Some(provider) = self.plugin_ui.surfaces().provider_of(&id) else {
                    return no(
                        request,
                        code::NOT_FOUND,
                        format!("{id} is not a plugin that draws, or it is switched off."),
                    );
                };
                // Opened rather than refused when it has not been looked at yet, because "what is on the
                // board" is a fair question to ask of a board nobody has opened in this window.
                if let Err(problem) = self.plugin_ui.opened(&id, &provider) {
                    return no(request, code::FAILED, problem);
                }
                match self.plugin_ui.view_of(&id) {
                    Some(value) => ok(request, id.to_string(), value),
                    None => no(request, code::FAILED, format!("{id} has nothing to show.")),
                }
            }
            _ => unknown(request),
        }
    }
}

/// What one plugin adds to the window, as short words, for `plugins list` and `plugins show`.
///
/// A language adds none of them and answers `language`, so a reader can tell at a glance which kind of
/// plugin a row is without reading the `kind` column beside it.
fn contributes(plugin: &crate::services::plugins::Plugin) -> Vec<String> {
    let mut found = Vec::new();
    if plugin.contributions.pane.is_some() {
        found.push("pane".to_owned());
    }
    if plugin.contributions.tab.is_some() {
        found.push("tab".to_owned());
    }
    if plugin.contributions.menu.is_some() {
        found.push("menu".to_owned());
    }
    if plugin.contributions.page.is_some() {
        found.push("settings page".to_owned());
    }
    if found.is_empty() && !plugin.extensions.is_empty() {
        found.push("language".to_owned());
    }
    found
}

/// The words of a `plugins run` argument line, with everything a body needs left intact.
///
/// **Split on spaces only, and runs of them are not collapsed.** `split_whitespace` threw away every
/// newline and every repeated space before the plugin saw them, and a provider's own `rest` closure
/// joins the words back with single spaces — so a comment holding a markdown document arrived as one
/// line. Markdown block structure is line based, so a heading swallowed the whole body, and no list,
/// table, fence or blockquote could survive. An agent asked to post one found this and wrote the
/// diagnosis on the ticket, because there was no way round it from the command line.
///
/// A run of n spaces becomes n-1 empty words here and n spaces again when the provider rejoins them, so
/// indentation comes back exactly rather than nearly — which is what a nested list needs. Newlines and
/// tabs sit inside the words and are not touched. The ends are trimmed of spaces so a line with a
/// trailing one does not produce an empty argument, and newlines at the ends are kept because they are
/// the caller's own text.
fn plugin_arguments(line: String) -> Vec<String> {
    let trimmed = line.trim_matches(' ');
    match trimmed.is_empty() {
        true => Vec::new(),
        false => trimmed.split(' ').map(str::to_owned).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_argument_line_keeps_the_newlines_and_the_indentation_a_body_needs() {
        // The words a command reads still come out as words.
        assert_eq!(
            plugin_arguments("task-4 --as claude hello".to_owned()),
            vec!["task-4", "--as", "claude", "hello"]
        );
        // And a body survives being taken apart and put back together by the provider's `rest`, which
        // joins with a single space. That round trip is the thing that has to hold.
        let body = "## Heading\n\n- one\n  - nested\n\n| a | b |\n| --- | --- |\n\n```rust\nfn main() {}\n```";
        let line = format!("task-4 --as claude {body}");
        let words = plugin_arguments(line);
        assert_eq!(words[0], "task-4");
        assert_eq!(words[2], "claude");
        assert_eq!(words[3..].join(" "), body, "the body comes back byte for byte");
        assert_eq!(plugin_arguments("   ".to_owned()), Vec::<String>::new());
    }
}
