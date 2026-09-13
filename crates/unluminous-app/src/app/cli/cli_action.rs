//! `action` and `project` -- running a menu entry by name, listing every one there is, and the
//! two things a project itself can be asked: open a folder, or list the recent ones.

use super::*;

/// How many rows `action find` prints when the caller does not say.
///
/// Twenty, because the rows are already best first and a twenty-first-best match for two words of a
/// command's name is not an answer anybody was going to read. `action list` is still there for the
/// whole of it. `task-1704`'s rule about a payload proportionate to the question.
const ACTIONS_FOUND: usize = 20;

impl UnluminousApp {
    pub(crate) fn cli_action(
        &mut self,
        request: &Request,
        verb: &str,
        ctx: &egui::Context,
    ) -> Outcome {
        match verb {
            "list" => {
                let all = self.every_menu_entry();
                let menus = match action_menus(request, &all) {
                    Ok(menus) => menus,
                    Err(message) => return no(request, code::USAGE, message),
                };
                let listed: Vec<&MenuEntry> = match &menus {
                    Some(menus) => all.iter().filter(|entry| menus.contains(&entry.menu)).collect(),
                    None => all.iter().collect(),
                };
                let rows: Vec<String> = listed
                    .iter()
                    .map(|entry| {
                        format!(
                            "{}{:<24}{:<18}{}",
                            if entry.enabled { " " } else { "-" },
                            entry.name,
                            entry.shortcut,
                            entry.menu
                        )
                    })
                    .collect();
                let value: Vec<Value> = listed
                    .iter()
                    .map(|entry| {
                        json!({
                            "name": entry.name,
                            "menu": entry.menu,
                            "label": entry.label,
                            "shortcut": entry.shortcut,
                            "enabled": entry.enabled,
                            "checked": entry.checked,
                        })
                    })
                    .collect();
                let sentence = match &menus {
                    Some(menus) => format!(
                        "{} entr{} on {}",
                        listed.len(),
                        if listed.len() == 1 { "y" } else { "ies" },
                        menus.join(", ")
                    ),
                    None => format!("{} menu entries", listed.len()),
                };
                lines(request, sentence, rows, json!({ "actions": value }))
            }
            "find" => self.cli_action_find(request),
            "run" => self.cli_action_run(request, ctx),
            _ => unknown(request),
        }
    }

    /// `unluminous-cli action find <text>` -- the palette's own list, asked for as data.
    ///
    /// **The same ranking the palette draws**, from `command_palette::rank`, so a row an agent is
    /// given and a row a person sees are in the same order. `action list` is still the whole list;
    /// this is the answer to *what is the command called* without reading all of it, which is
    /// `task-1704`'s rule about a payload proportionate to the question.
    fn cli_action_find(&mut self, request: &Request) -> Outcome {
        use crate::components::command_palette;
        let text = request.text("text").unwrap_or_default();
        let limit = match request.whole("limit") {
            Some(0) => usize::MAX,
            Some(asked) => asked,
            None => ACTIONS_FOUND,
        };
        let commands = self.every_menu_command();
        let found = command_palette::rank(&commands, &text, limit);
        let rows: Vec<String> = found
            .iter()
            .map(|row| {
                format!(
                    "{}{:<24}{:<18}{}",
                    if row.command.enabled { " " } else { "-" },
                    row.command.name,
                    row.command.shortcut,
                    row.command.menu
                )
            })
            .collect();
        let value: Vec<Value> = found
            .iter()
            .map(|row| {
                json!({
                    "name": row.command.name,
                    "label": row.command.label,
                    "menu": row.command.menu,
                    "shortcut": row.command.shortcut,
                    "enabled": row.command.enabled,
                })
            })
            .collect();
        lines(
            request,
            match text.trim().is_empty() {
                true => format!("{} menu entries", found.len()),
                false => format!(
                    "{} entr{} matching {}",
                    found.len(),
                    if found.len() == 1 { "y" } else { "ies" },
                    text.trim()
                ),
            },
            rows,
            json!({ "actions": value }),
        )
    }

    fn cli_action_run(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        let Some(name) = request.text("name") else {
            return no(request, code::USAGE, "Say which menu entry.");
        };
        if let Some(instead) = Action::instead_of_a_file_chooser(&name) {
            return no(
                request,
                code::NOT_APPLICABLE,
                format!(
                    "{name} opens the platform's file chooser, which nobody can click from a script. \
                     Use `unluminous-cli {instead}` instead."
                ),
            );
        }
        let path = self.cli_path_argument(request, "path");
        if Action::wants_a_path(&name) && path.is_none() {
            return no(request, code::USAGE, format!("{name} needs --path."));
        }
        // A plugin's own entries name themselves — `plugin-run:<plugin>:<command>` — because their names
        // come from a manifest rather than from the list `Action::from_name` reads. Resolving them here is
        // what makes `action list` and `action run` agree about a contributed entry, which is the whole
        // point of the naming machinery: an entry is reachable the day the manifest is written.
        let action = match plugin_action(&name) {
            Some(action) => Some(action),
            None => Action::from_name(&name, path),
        };
        let Some(action) = action else {
            return no(
                request,
                code::NOT_FOUND,
                format!("There is no menu entry called {name}. `action list` names them all."),
            );
        };
        self.run_action(action, ctx);
        ok(
            request,
            self.message.clone().unwrap_or_else(|| format!("Ran {name}")),
            json!({ "ran": name, "message": self.message }),
        )
    }

    /// Every entry on every menu, as it stands right now.
    ///
    /// Built by walking the real menus rather than from a list of its own, which is the point: a
    /// menu entry added later is on the command line the day it is added. **The walk itself is
    /// `UnluminousApp::every_menu_command`**, which the `Find Action` palette and `action find` also
    /// read -- one walk rather than three that agree today. `task-1922` WP4.
    fn every_menu_entry(&self) -> Vec<MenuEntry> {
        self.every_menu_command()
            .into_iter()
            .map(|command| MenuEntry {
                name: command.name,
                menu: command.menu,
                label: command.label,
                shortcut: command.shortcut,
                enabled: command.enabled,
                checked: command.checked,
            })
            .collect()
    }

    pub(crate) fn cli_project(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "open" => {
                let Some(folder) = self.cli_path_argument(request, "folder") else {
                    return no(request, code::USAGE, "Say which folder to show.");
                };
                if !folder.is_dir() {
                    return no(
                        request,
                        code::NOT_FOUND,
                        format!("{} is not a folder.", folder.display()),
                    );
                }
                self.open_folder(&folder);
                ok(
                    request,
                    format!("Showing {}", folder.display()),
                    json!({ "project": folder.to_string_lossy(), "files": self.tree.all_files().len() }),
                )
            }
            "recent" => {
                let rows: Vec<String> =
                    self.recent.iter().map(|path| path.display().to_string()).collect();
                lines(
                    request,
                    format!("{} recent projects", rows.len()),
                    rows.clone(),
                    json!({ "recent": rows }),
                )
            }
            _ => unknown(request),
        }
    }
}

/// One row of `action list`.
struct MenuEntry {
    name: String,
    menu: String,
    label: String,
    shortcut: String,
    enabled: bool,
    checked: bool,
}

/// Read `--menu` and check it against the menus the list actually has.
///
/// Comma-separated and case-insensitive, the way `status --section` takes its list, because an
/// agent writes `view` where the menu is `View`. A submenu name names its own rows, because the
/// walk records the submenu's name rather than the menu it sits under. Nothing given is every
/// menu, which is what `action list` has always been.
fn action_menus(request: &Request, all: &[MenuEntry]) -> Result<Option<Vec<String>>, String> {
    let Some(text) = request.text("menu") else {
        return Ok(None);
    };
    let mut wanted: Vec<String> = Vec::new();
    for part in text.split(',') {
        let name = part.trim().to_lowercase();
        if name.is_empty() {
            continue;
        }
        if !wanted.contains(&name) {
            wanted.push(name);
        }
    }
    if wanted.is_empty() {
        return Ok(None);
    }
    let mut have: Vec<String> = Vec::new();
    for entry in all {
        if !have.iter().any(|known| known.to_lowercase() == entry.menu.to_lowercase()) {
            have.push(entry.menu.clone());
        }
    }
    let mut found: Vec<String> = Vec::new();
    for name in wanted {
        match have.iter().find(|menu| menu.to_lowercase() == name) {
            Some(menu) => {
                if !found.iter().any(|known| known.to_lowercase() == menu.to_lowercase()) {
                    found.push(menu.clone());
                }
            }
            None => {
                return Err(format!(
                    "There is no menu called `{}`. The menus are: {}.",
                    name,
                    have.join(", ")
                ));
            }
        }
    }
    Ok(Some(found))
}

/// The action a plugin's own entry name stands for, or `None` when the name is not one of theirs.
///
/// Three shapes, matching what `Action::name` writes: `plugin-pane:<plugin>/<pane>`,
/// `plugin-tab:<plugin>/<tab>` and `plugin-run:<plugin>:<command>`.
fn plugin_action(name: &str) -> Option<crate::app::actions::Action> {
    use crate::app::actions::Action;
    if let Some(pane) = name.strip_prefix("plugin-pane:") {
        return Some(Action::PluginPane { pane: pane.to_owned() });
    }
    if let Some(tab) = name.strip_prefix("plugin-tab:") {
        return Some(Action::PluginTab { tab: tab.to_owned() });
    }
    if let Some(rest) = name.strip_prefix("plugin-run:") {
        let (plugin, command) = rest.split_once(':')?;
        if plugin.is_empty() || command.is_empty() {
            return None;
        }
        return Some(Action::PluginCommand {
            plugin: plugin.to_owned(),
            command: command.to_owned(),
        });
    }
    None
}
