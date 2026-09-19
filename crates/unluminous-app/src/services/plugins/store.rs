//! Loading the plugins that ship inside the binary and the ones a person installed, and switching
//! them on and off.
//!
//! [`Plugins`] is the one thing that changes which plugins there are: it reads the bundled ones,
//! then anything on disk shadowing them, remembers which are switched on, and works out
//! [`Grammars`](super::grammar::Grammars) and [`Surfaces`](super::types::Surfaces) again whenever
//! that list moves. Nothing here reads a single manifest's keys; that is `manifest::parse`'s job.

use std::path::{Path, PathBuf};

use unluminous_core::syntax::Grammar;

use super::bundled;
use super::grammar::Grammars;
use super::manifest::parse;
use super::types::{Kind, Plugin, Surface, Surfaces};
use crate::services::store::{Store, Values};

/// The folder under the settings folder that installed plugins live in.
pub const FOLDER: &str = "plugins";
/// The file inside a plugin folder that describes it.
const MANIFEST: &str = "plugin.conf";
/// The picture a plugin puts in front of its files.
const ICON: &str = "icon.png";

/// Everything installed, and which of them are switched on.
#[derive(Debug, Clone, Default)]
pub struct Plugins {
    installed: Vec<Plugin>,
    /// The grammars of the plugins that are switched on, worked out whenever that list changes.
    ///
    /// The one thing here that is remembered rather than derived on demand, and [`Plugins::grammars`]
    /// says what that is worth. Every function that changes `installed` calls [`Plugins::settle`],
    /// and all of them are in this file.
    grammars: Grammars,
}

impl Plugins {
    /// Read the bundled plugins, then anything on disk, which shadows a bundled one of the same id.
    ///
    /// A plugin that will not parse is skipped, and the reason is returned rather than thrown away.
    /// Unluminous starting with one plugin fewer is better than Unluminous refusing to start — the same rule
    /// `store.rs` already keeps for a settings file with a stray line in it.
    pub fn load(store: Option<&Store>) -> (Self, Vec<String>) {
        let mut installed: Vec<Plugin> = Vec::new();
        let mut problems: Vec<String> = Vec::new();
        for (id, manifest, icon) in bundled::ALL {
            match parse(&Values::parse(manifest), true) {
                Ok(mut plugin) => {
                    plugin.icon = icon.map(<[u8]>::to_vec);
                    installed.push(plugin);
                }
                Err(reason) => problems.push(format!("{id}: {reason}")),
            }
        }
        if let Some(store) = store {
            let folder = store.folder().join(FOLDER);
            if let Ok(entries) = std::fs::read_dir(&folder) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    // **A folder with no manifest at all is not a plugin that failed to load.**
                    // `services::plugin_settings` keeps a *bundled* plugin's own configuration at
                    // `<store>/plugins/<id>/settings.conf`, so configuring Agent-Chat's provider row
                    // or the Database plugin's sources makes a folder here that has never held a
                    // manifest and never will. Reading it as a broken plugin put
                    // `A plugin could not be read -- ...: plugin.conf could not be read` in the
                    // status bar after every reload, for ever, on any machine where somebody had
                    // configured a plugin that ships in the binary. Found by `task-1922` B14, which
                    // made the reload's problems reach the reply that had been discarding them.
                    //
                    // A manifest that is there and will not parse is still a refusal with its reason,
                    // which is the half this must not take away.
                    if !path.join(MANIFEST).exists() {
                        continue;
                    }
                    match read_folder(&path) {
                        Ok(plugin) => {
                            // A plugin on disk shadows the bundled one of the same id, so a bundled
                            // one can be corrected by hand without rebuilding Unluminous.
                            installed.retain(|known| known.id != plugin.id);
                            installed.push(plugin);
                        }
                        Err(reason) => {
                            problems.push(format!("{}: {reason}", path.display()));
                        }
                    }
                }
            }
            let disabled = store.read_values();
            if let Some(list) = disabled.text("plugins.disabled") {
                for id in list.split(',').map(str::trim).filter(|id| !id.is_empty()) {
                    if let Some(plugin) = installed.iter_mut().find(|plugin| plugin.id == id) {
                        plugin.enabled = false;
                    }
                }
            }
        }
        installed.sort_by_key(|plugin| plugin.name.to_lowercase());
        let mut plugins = Self { installed, grammars: Grammars::default() };
        plugins.settle();
        (plugins, problems)
    }

    /// Work out again everything that is kept rather than derived. Called by every function here
    /// that changes which plugins there are or which of them are on.
    fn settle(&mut self) {
        let mut by_extension: Vec<(String, Grammar)> = Vec::new();
        for plugin in self.installed.iter().filter(|plugin| plugin.enabled) {
            for extension in &plugin.extensions {
                if by_extension.iter().any(|(known, _)| known == extension) {
                    continue; // the first plugin that claims an extension is the one `for_path` gives
                }
                by_extension.push((extension.clone(), plugin.grammar.clone()));
            }
        }
        self.grammars = Grammars::of(by_extension);
    }

    pub fn all(&self) -> &[Plugin] {
        &self.installed
    }

    pub fn get(&self, id: &str) -> Option<&Plugin> {
        self.installed.iter().find(|plugin| plugin.id == id)
    }

    /// Everything every plugin that is switched on contributes.
    ///
    /// Worked out from the manifests rather than remembered, so it is right after `set_enabled`,
    /// after `install` and after a reload, with nothing to invalidate. The plugins are already sorted
    /// by name, so the rail's buttons and the menus are in the same order every time.
    pub fn surfaces(&self) -> Surfaces {
        let mut surfaces = Surfaces::default();
        for plugin in self.installed.iter().filter(|plugin| plugin.enabled) {
            let Some(provider) = plugin.contributions.provider.clone() else {
                continue;
            };
            // One closure per list rather than one generic one, because a closure in Rust is not
            // generic over its argument and four contributions are four types.
            fn made<T>(plugin: &Plugin, provider: &str, what: T) -> Surface<T> {
                Surface { plugin: plugin.id.clone(), provider: provider.to_owned(), what }
            }
            if let Some(pane) = plugin.contributions.pane.clone() {
                surfaces.panes.push(made(plugin, &provider, pane));
            }
            if let Some(tab) = plugin.contributions.tab.clone() {
                surfaces.tabs.push(made(plugin, &provider, tab));
            }
            if let Some(menu) = plugin.contributions.menu.clone() {
                surfaces.menus.push(made(plugin, &provider, menu));
            }
            if let Some(page) = plugin.contributions.page.clone() {
                surfaces.pages.push(made(plugin, &provider, page));
            }
            if let Some(renderer) = plugin.contributions.chrome.clone() {
                surfaces.chrome.push((plugin.id.clone(), renderer));
            }
        }
        surfaces
    }

    /// The plugins that draw, whether or not they are switched on, for the Plugins page's own list.
    pub fn ui_plugins(&self) -> Vec<&Plugin> {
        self.installed.iter().filter(|plugin| plugin.kind == Kind::Ui).collect()
    }

    /// Every theme that can be chosen: Unluminous's own, then each theme plugin that is switched on.
    ///
    /// Worked out from the manifests each time rather than remembered, which is what `surfaces` does and
    /// for the same reason: switching a theme plugin off has to withdraw its themes in the same frame it
    /// withdraws anything else, and one value everything reads is what makes that impossible to get wrong.
    /// `UnluminousApp::apply_the_theme` falls back to `unluminous/dark` when the active one is no longer in this
    /// list, so a plugin switched off can never leave the window in a palette nothing can name.
    pub fn themes(&self) -> Vec<crate::theme::Theme> {
        let mut found = vec![crate::theme::Theme::unluminous_dark()];
        for plugin in self.installed.iter().filter(|plugin| plugin.enabled) {
            found.extend(plugin.themes.iter().cloned());
        }
        found
    }

    /// The theme this key names, by its `<plugin>/<theme>` key or by the name on the screen.
    ///
    /// Both, because the key is what the settings file holds and the name is what a person and an agent
    /// read — `theme set "Deep Ocean"` is what somebody types. That is `split_off_a_name`'s rule kept
    /// again: a thing is named on the command line by what is on the screen.
    pub fn theme(&self, wanted: &str) -> Option<crate::theme::Theme> {
        let wanted = wanted.trim();
        let all = self.themes();
        all.iter()
            .find(|theme| theme.key.eq_ignore_ascii_case(wanted))
            .or_else(|| all.iter().find(|theme| theme.name.eq_ignore_ascii_case(wanted)))
            .cloned()
    }

    pub fn enabled_count(&self) -> usize {
        self.installed.iter().filter(|plugin| plugin.enabled).count()
    }

    /// The plugin that claims `path`, if one does and it is switched on.
    pub fn for_path(&self, path: &Path) -> Option<&Plugin> {
        self.installed.iter().find(|plugin| plugin.enabled && plugin.claims(path))
    }

    /// True when some plugin that is switched on asks for the built-in renderer called `name`.
    ///
    /// The window asks this before it draws a diagram anywhere — a `.mmd` file's preview, and every
    /// mermaid block in a Markdown document — so switching the plugin off withdraws the feature in
    /// the same frame rather than at the next restart.
    /// The plugin that reads a language named on a fence in a Markdown document.
    ///
    /// A fence says `rust`, `rs`, `js` or `TypeScript`, and all four are the same request. So the
    /// word is matched against the plugin's id, its name and every extension it claims, which is
    /// what makes ```` ```rs ```` and ```` ```rust ```` one question without Unluminous holding a table
    /// of aliases that a plugin somebody writes later could not add to.
    pub fn for_language(&self, name: &str) -> Option<&Plugin> {
        let wanted = name.trim().trim_start_matches('.').to_lowercase();
        if wanted.is_empty() {
            return None;
        }
        self.installed.iter().filter(|plugin| plugin.enabled).find(|plugin| {
            plugin.id == wanted
                || plugin.name.to_lowercase() == wanted
                || plugin.extensions.contains(&wanted)
        })
    }

    pub fn renders(&self, name: &str) -> bool {
        self.installed
            .iter()
            .any(|plugin| plugin.enabled && plugin.renders.as_deref() == Some(name))
    }

    /// How one file of `path`'s language is run, when a plugin that is switched on says.
    ///
    /// Asked at the moment of use, exactly as [`Plugins::renders`] is, so switching the JavaScript
    /// plugin off withdraws `Run Current File` from `.js` files in the same frame rather than at
    /// the next restart.
    pub fn run_file(&self, path: &Path) -> Option<&str> {
        self.for_path(path)?.run_file.as_deref()
    }

    /// The debugger `path`'s language names, when a plugin that is switched on names one.
    ///
    /// **The one question the menus, the title bar, the gutter and the command line all ask**, so
    /// none of them can disagree about whether a file can be debugged — which is the rule
    /// `file_kind::definitions_apply` set and the reason there is a function here rather than four
    /// readings of `for_path`. Asked at the moment of use, exactly as [`Plugins::renders`] is, so
    /// switching the Rust plugin off withdraws debugging from `.rs` files in the same frame rather
    /// than at the next restart.
    pub fn debugger_for(&self, path: &Path) -> Option<&str> {
        self.for_path(path)?.debug_adapter.as_deref()
    }

    /// The languages this debugger debugs, as the plugins that are switched on say.
    ///
    /// The other direction of [`Plugins::debugger_for`], and it exists for `unluminous-cli debug
    /// adapters`: an agent asking whether it can debug wants to know what `lldb` is *for* here,
    /// which is a question only the manifests can answer.
    pub fn languages_debugged_by(&self, adapter: &str) -> Vec<String> {
        self.installed
            .iter()
            .filter(|plugin| plugin.enabled && plugin.debug_adapter.as_deref() == Some(adapter))
            .map(|plugin| plugin.name.clone())
            .collect()
    }

    /// True when any plugin that is switched on names a debugger at all, which is what decides
    /// whether the debug tile can ever be reached in this project.
    pub fn any_debugger(&self) -> bool {
        self.installed.iter().any(|plugin| plugin.enabled && plugin.debug_adapter.is_some())
    }

    /// The project detectors the plugins that are switched on have asked for, each named once.
    ///
    /// JavaScript and TypeScript both say `npm`, and both being installed is not two projects, so
    /// the list is deduplicated here rather than in the detector.
    pub fn project_runners(&self) -> Vec<&str> {
        let mut runners: Vec<&str> = Vec::new();
        for plugin in self.installed.iter().filter(|plugin| plugin.enabled) {
            if let Some(runner) = plugin.run_project.as_deref() {
                if !runners.contains(&runner) {
                    runners.push(runner);
                }
            }
        }
        runners
    }

    /// Switch a plugin on or off, and remember it.
    pub fn set_enabled(&mut self, store: Option<&Store>, id: &str, on: bool) {
        if let Some(plugin) = self.installed.iter_mut().find(|plugin| plugin.id == id) {
            plugin.enabled = on;
        }
        self.settle();
        let Some(store) = store else {
            return;
        };
        let disabled: Vec<&str> = self
            .installed
            .iter()
            .filter(|plugin| !plugin.enabled)
            .map(|plugin| plugin.id.as_str())
            .collect();
        let mut values = store.read_values();
        values.set("plugins.disabled", disabled.join(", "));
        store.write_values(&values);
    }

    /// Write a bundled plugin out to the settings folder and read it back from there.
    ///
    /// Reading it back from disk rather than simply marking it installed is the point: it is what
    /// proves the loader works on real files and not only on what was baked into the binary.
    pub fn install(&mut self, store: &Store, id: &str) -> std::io::Result<()> {
        let Some((_, manifest, icon)) = bundled::ALL.iter().find(|(known, _, _)| *known == id)
        else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("there is no plugin called {id}"),
            ));
        };
        let folder = store.folder().join(FOLDER).join(id);
        std::fs::create_dir_all(&folder)?;
        // Through a temporary and a rename, for `store::write_atomically`'s own reason: a manifest
        // left half written is a plugin that is refused at the next start, and a plugin folder is
        // read at every one.
        crate::services::store::write_atomically(&folder.join(MANIFEST), manifest.as_bytes())?;
        if let Some(icon) = icon {
            crate::services::store::write_atomically(&folder.join(ICON), icon)?;
        }
        let plugin = read_folder(&folder)
            .map_err(|reason| std::io::Error::new(std::io::ErrorKind::InvalidData, reason))?;
        self.installed.retain(|known| known.id != plugin.id);
        self.installed.push(plugin);
        self.installed.sort_by_key(|plugin| plugin.name.to_lowercase());
        self.settle();
        Ok(())
    }

    /// Take an installed plugin’s folder away, and go back to the copy that shipped in the binary.
    ///
    /// **Uninstalling a bundled plugin does not remove the feature**, and that is the honest meaning
    /// of the button: `install` writes a folder out so it can be edited by hand, so `uninstall`
    /// throws that folder away and leaves the plugin as it shipped. One that has no bundled copy
    /// behind it is simply gone.
    ///
    /// The folder is `store.folder()/plugins/<id>`, built from the id and never from anything typed,
    /// and the id has to name a plugin that is actually installed — which is what keeps a recursive
    /// delete pointed where it is meant to be.
    pub fn uninstall(&mut self, store: &Store, id: &str) -> std::io::Result<()> {
        if !self.installed.iter().any(|known| known.id == id) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{id} is not installed"),
            ));
        }
        let folder = Self::folder(store, id);
        if folder.is_dir() {
            std::fs::remove_dir_all(&folder)?;
        }
        self.installed.retain(|known| known.id != id);
        // The bundled copy comes back, so the plugin goes on working — read from the manifest that
        // shipped rather than marked as present, which is `install`’s own rule the other way round.
        if let Some((_, manifest, icon)) = bundled::ALL.iter().find(|(known, _, _)| *known == id) {
            if let Ok(mut plugin) = parse(&Values::parse(manifest), true) {
                plugin.icon = icon.map(<[u8]>::to_vec);
                self.installed.push(plugin);
            }
        }
        self.installed.sort_by_key(|plugin| plugin.name.to_lowercase());
        self.settle();
        Ok(())
    }

    /// Where a plugin's folder is, so the marketplace can say whether it is on disk.
    pub fn folder(store: &Store, id: &str) -> PathBuf {
        store.folder().join(FOLDER).join(id)
    }

    /// The grammars of the plugins that are switched on.
    ///
    /// **Borrowed, and worked out only when the plugins change.** It used to build the whole set on
    /// every call, deep-cloning each plugin's `Grammar` — its keywords, its builtins, its types, its
    /// definers and its import words, several hundred `String`s for a language like TypeScript —
    /// **once per extension that plugin claims**. Eleven plugins claiming two dozen extensions
    /// between them made that two dozen full copies a call, and `UnluminousApp::menu_state` asks
    /// three times a frame, to answer three questions each of which is "does this file's language
    /// name a definer". `task-1805` measured it at **0.43 ms of a 1.4 ms frame** — the largest single
    /// thing left in an idle one, and a tax on every interactive frame as well.
    ///
    /// A thread still takes a **copy**, and that has not changed: `services::symbol_index` and the
    /// reference mode of `services::text_search` outlive the frame that started them, and a plugin
    /// switched off while one is running must not change what it is half way through answering. They
    /// clone it themselves, which is the caller that needs a snapshot paying for one.
    pub fn grammars(&self) -> &Grammars {
        &self.grammars
    }
}

/// Read one plugin folder.
fn read_folder(folder: &Path) -> Result<Plugin, String> {
    let manifest = folder.join(MANIFEST);
    let text = std::fs::read_to_string(&manifest)
        .map_err(|problem| format!("{MANIFEST} could not be read: {problem}"))?;
    let mut plugin = parse(&Values::parse(&text), false)?;
    plugin.icon = std::fs::read(folder.join(ICON)).ok();
    Ok(plugin)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-28` asked for the board to be a tab and not a pane, so it contributes four things and one of
    /// them is the provider. A pane is still something a manifest may ask for — `a_manifest_may_contribute_a_pane`
    /// below is what keeps the reader honest about that, since no bundled plugin asks for one any more.
    #[test]
    fn the_agent_tasks_plugin_contributes_a_pane_a_menu_and_a_page_and_no_tab() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let board = plugins.get("agent-tasks").expect("the agent-tasks plugin");
        assert_eq!(board.kind, Kind::Ui);
        assert_eq!(board.contributions.provider.as_deref(), Some("agent-tasks"));
        // **A pane and no tab**, which is `task-1848`: "Agent tasks should be its own pane, rather than a
        // tab." This test asserted the opposite, because `task-28` had moved it the other way; it is
        // changed rather than deleted, so the shape stays pinned either way round.
        let pane = board.contributions.pane.as_ref().expect("the board is a pane");
        assert!(board.contributions.tab.is_none(), "and not a tab as well");
        assert!(
            pane.width >= 600.0,
            "wide enough for four lanes rather than the 420 a side pane starts at: {}",
            pane.width
        );
        assert!(board.contributions.menu.is_some());
        assert!(board.contributions.page.is_some());
        assert!(!board.limitations.is_empty(), "it says what it does not do");
        assert!(
            board.limitations.contains("is a pane"),
            "and it says the board is a pane, since that decides where somebody looks for it: {}",
            board.limitations
        );
    }

    /// `task-1765`: the board asks for the decoration renderer, and the key is checked like every other one.
    #[test]
    fn the_agent_tasks_plugin_asks_for_the_decoration_renderer_and_a_language_plugin_does_not() {
        let (plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let board = plugins.get("agent-tasks").expect("the agent-tasks plugin");
        assert_eq!(board.contributions.chrome.as_deref(), Some("vello"));
        // And it is in the one value everything reads, so switching the plugin off withdraws the decoration
        // in the same frame it withdraws the tab. The **name** is what comes back, not a yes: there is one
        // renderer today and the day there are two this is where the second one is chosen.
        assert_eq!(plugins.surfaces().chrome_for("agent-tasks"), Some("vello"));
        assert_eq!(plugins.surfaces().chrome_for("mermaid"), None);
        for plugin in plugins.all().iter().filter(|plugin| plugin.kind == Kind::Language) {
            assert!(plugin.contributions.chrome.is_none(), "{} asks for a renderer", plugin.id);
        }
    }

    #[test]
    fn the_surfaces_are_what_the_enabled_plugins_contribute_and_nothing_else() {
        let (mut plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let surfaces = plugins.surfaces();
        // **All three contribute a pane and none contributes a tab**, which is `task-1848`: the board and
        // the database's query console were both asked for as panes rather than as surfaces in the editing
        // area. A conversation is a column, a board is four lanes, and a database is a tree with its
        // consoles under it — and each of those is a shape a pane can be given by dragging it, which is
        // what `task-1697`'s docking bought and what makes this one manifest line rather than a rewrite.
        assert_eq!(surfaces.panes.len(), 3, "every plugin that draws asks for a pane");
        assert!(surfaces.pane("agent-chat/chat").is_some());
        assert!(surfaces.pane("database/explorer").is_some());
        assert!(surfaces.pane("agent-tasks/board").is_some());
        assert!(surfaces.pane("agent-chat/nothing").is_none());
        assert!(surfaces.tabs.is_empty(), "and nothing contributes a tab any more");
        assert!(surfaces.tab("agent-tasks/board").is_none());
        assert!(surfaces.tab("database/workspace").is_none());
        assert_eq!(surfaces.menus.len(), 3);
        assert_eq!(surfaces.pages.len(), 3);
        // Switching one off withdraws every contribution of that plugin at once, which is the rule
        // `Plugins::renders` already keeps for a Mermaid diagram: the window asks before it draws.
        plugins.set_enabled(None, "agent-tasks", false);
        plugins.set_enabled(None, "agent-chat", false);
        plugins.set_enabled(None, "database", false);
        assert!(plugins.surfaces().is_empty(), "a plugin that is off contributes nothing");
        plugins.set_enabled(None, "agent-tasks", true);
        plugins.set_enabled(None, "agent-chat", true);
        plugins.set_enabled(None, "database", true);
        assert_eq!(plugins.surfaces().panes.len(), 3, "and switching it back on is one frame too");
        assert!(plugins.surfaces().tabs.is_empty());
    }

    /// Installing writes the folder out; uninstalling takes it away and the bundled copy comes back.
    ///
    /// The second half is the point. `task-1795` asks for the button to say `Uninstall` once a plugin
    /// is installed, and the honest meaning of that here is *stop keeping an editable copy on disk* —
    /// not *remove the feature*, which a bundled plugin's button could not do anyway.
    #[test]
    fn installing_then_uninstalling_leaves_the_bundled_plugin() {
        let folder =
            std::env::temp_dir().join(format!("unluminous-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let store = Store::at(folder.clone());
        let (mut plugins, _) = Plugins::load(None);

        plugins.install(&store, "database").expect("install");
        assert!(Plugins::folder(&store, "database").is_dir(), "install writes the folder out");
        assert!(
            plugins.all().iter().any(|p| p.id == "database" && !p.bundled),
            "the copy on disk shadows the bundled one",
        );

        plugins.uninstall(&store, "database").expect("uninstall");
        assert!(!Plugins::folder(&store, "database").exists(), "uninstall takes the folder away");
        let back = plugins
            .all()
            .iter()
            .find(|p| p.id == "database")
            .expect("the bundled copy comes back rather than the plugin disappearing");
        assert!(back.bundled, "and it is the bundled one again");
        assert!(back.icon.is_some(), "with its icon, read from the manifest that shipped");

        assert!(
            plugins.uninstall(&store, "not-a-plugin").is_err(),
            "and only a real id is removed"
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The grammars are kept rather than worked out on every call, and the thing a kept answer can
    /// do that a derived one cannot is go stale. `task-1805`: `grammars()` deep-cloned every
    /// plugin's word lists once per extension it claims, three times a frame, so it became a field —
    /// and every function that changes which plugins are on calls `settle`. This is what says so.
    #[test]
    fn switching_a_plugin_off_changes_the_grammars_it_offers() {
        let (mut plugins, _) = Plugins::load(None);
        let rust = || Path::new("main.rs");
        assert!(
            plugins.grammars().for_path(rust()).is_some(),
            "the bundled Rust plugin claims .rs and is on"
        );
        plugins.set_enabled(None, "rust", false);
        assert!(
            plugins.grammars().for_path(rust()).is_none(),
            "a plugin that is switched off must stop claiming its extensions, kept answer or not"
        );
        plugins.set_enabled(None, "rust", true);
        assert!(
            plugins.grammars().for_path(rust()).is_some(),
            "and start again when it comes back"
        );
    }

    /// **A bundled plugin's own settings folder is not read as a broken plugin.** `task-1922`.
    ///
    /// `plugin_settings::write` keeps a bundled plugin's configuration at
    /// `<store>/plugins/<id>/settings.conf`, and the same folder is where an *installed* plugin's
    /// manifest goes -- so configuring Agent-Chat's provider row made a folder with no manifest in
    /// it, and the loader reported that as a plugin it could not read. The status bar then said
    /// `A plugin could not be read` after every reload, for ever, on any machine where somebody had
    /// configured a plugin that ships in the binary.
    ///
    /// The half this must not take away is the other one: a manifest that is there and will not
    /// parse is still refused with its reason.
    #[test]
    fn a_folder_with_no_manifest_in_the_plugin_store_is_not_a_broken_plugin() {
        let folder = std::env::temp_dir().join("unluminous-plugins-settings-only");
        std::fs::remove_dir_all(&folder).ok();
        let store = crate::services::store::Store::at(&folder);
        let plugins = folder.join("plugins");

        // What configuring a bundled plugin leaves behind: a folder holding settings and nothing else.
        std::fs::create_dir_all(plugins.join("agent-chat")).expect("make the folder");
        std::fs::write(plugins.join("agent-chat/settings.conf"), "provider.0.program = claude\n")
            .expect("write the settings");
        let (_, problems) = Plugins::load(Some(&store));
        assert!(problems.is_empty(), "a settings folder is not a plugin: {problems:?}");

        // And a manifest that is there and will not parse still says so.
        std::fs::create_dir_all(plugins.join("broken")).expect("make the folder");
        std::fs::write(plugins.join("broken/plugin.conf"), "plugin.name = No id here\n")
            .expect("write the manifest");
        let (_, problems) = Plugins::load(Some(&store));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("plugin.id"), "{problems:?}");
    }

    /// Switching the plugin off withdraws its themes, which is `Plugins::renders`' rule for colour.
    #[test]
    fn switching_a_theme_plugin_off_withdraws_its_themes() {
        let (mut plugins, _) = Plugins::load(None);
        assert!(plugins.theme("Monokai Pro").is_some(), "it is there to begin with");
        assert_eq!(plugins.themes().len(), 6, "Unluminous's own, then the bundle's five");
        plugins.set_enabled(None, "themes-bundle-1", false);
        assert!(plugins.theme("Monokai Pro").is_none(), "and gone when the plugin is off");
        assert_eq!(plugins.themes().len(), 1, "leaving only Unluminous's own");
    }

    /// A theme is found by its key or by the name that is on the screen, which is `split_off_a_name`'s
    /// rule: a thing is named on the command line by what a person reads.
    #[test]
    fn a_theme_is_found_by_its_key_or_by_its_name() {
        let (plugins, _) = Plugins::load(None);
        assert_eq!(
            plugins.theme("themes-bundle-1/deep-ocean").map(|theme| theme.name),
            Some("Material Deep Ocean".to_owned())
        );
        assert_eq!(
            plugins.theme("material deep ocean").map(|theme| theme.key),
            Some("themes-bundle-1/deep-ocean".to_owned()),
            "and by name, whatever the case"
        );
        assert_eq!(
            plugins.theme("unluminous/dark").map(|theme| theme.name),
            Some("Unluminous Dark".to_owned())
        );
        assert!(plugins.theme("solarized").is_none());
    }

    /// The body of a `<style>` block is coloured by the plugin that claims its language, asked at
    /// the moment of use. That is the seam `colour_the_embedded` reads, and it is what makes the
    /// withdrawal happen in the same frame rather than at the next restart.
    #[test]
    fn switching_the_css_plugin_off_withdraws_the_colouring_inside_a_style_block() {
        let (mut plugins, problems) = Plugins::load(None);
        assert!(problems.is_empty(), "{problems:?}");
        let html = plugins.get("html").expect("the html plugin");
        assert!(
            html.grammar
                .raw_text
                .iter()
                .any(|(el, lang)| el == "style" && lang.as_deref() == Some("css")),
            "the style block names css, which is what the window asks for"
        );
        assert!(plugins.for_language("css").is_some(), "css is on, so the block is coloured");
        plugins.set_enabled(None, "css", false);
        assert!(plugins.for_language("css").is_none(), "and off, so it is not");
    }

    /// Asked at the moment of use, exactly as `renders` is: switching the plugin off withdraws
    /// debugging from its files in the same frame rather than at the next restart.
    #[test]
    fn switching_a_plugin_off_withdraws_debugging_from_its_files() {
        let (mut plugins, _) = Plugins::load(None);
        assert_eq!(plugins.debugger_for(Path::new("a.rs")), Some("lldb"));
        plugins.set_enabled(None, "rust", false);
        assert_eq!(plugins.debugger_for(Path::new("a.rs")), None);
    }

    #[test]
    fn switching_a_plugin_off_withdraws_its_running_in_the_same_frame() {
        // The rule `Plugins::renders` already keeps, which is what makes this a plugin rather than
        // a feature with a plugin painted on it.
        let (mut plugins, _) = Plugins::load(None);
        assert!(plugins.run_file(Path::new("server.js")).is_some());
        plugins.set_enabled(None, "javascript", false);
        assert!(plugins.run_file(Path::new("server.js")).is_none());
        // TypeScript still asks for `npm`, so the suggestions do not withdraw until it goes too.
        assert!(plugins.project_runners().contains(&"npm"));
        plugins.set_enabled(None, "typescript", false);
        assert_eq!(plugins.project_runners(), vec!["cargo"]);
    }

    #[test]
    fn a_plugin_that_is_switched_off_claims_nothing() {
        let (mut plugins, _) = Plugins::load(None);
        assert!(plugins.for_path(Path::new("a.rs")).is_some());
        plugins.set_enabled(None, "rust", false);
        assert!(plugins.for_path(Path::new("a.rs")).is_none());
        assert_eq!(plugins.enabled_count(), bundled::ALL.len() - 1);
    }

    #[test]
    fn switching_the_mermaid_plugin_off_withdraws_the_renderer() {
        // This is what makes it a plugin rather than a feature with a plugin painted on it: the
        // window asks `renders` before it draws a diagram anywhere, so this is the whole of it.
        let (mut plugins, _) = Plugins::load(None);
        assert!(plugins.renders("mermaid"));
        plugins.set_enabled(None, "mermaid", false);
        assert!(!plugins.renders("mermaid"));
        assert!(plugins.for_path(Path::new("a.mmd")).is_none());
    }

    #[test]
    fn installing_writes_the_folder_and_reads_it_back_from_disk() {
        let folder = std::env::temp_dir().join("unluminous-plugins-install");
        std::fs::remove_dir_all(&folder).ok();
        let store = Store::at(&folder);
        let (mut plugins, _) = Plugins::load(None);
        plugins.install(&store, "rust").expect("install");
        let written = Plugins::folder(&store, "rust");
        assert!(
            written.join(MANIFEST).is_file(),
            "the manifest is written where a person can read it"
        );
        assert!(written.join(ICON).is_file());
        // What is loaded now came off disk, which is what proves the loader works on real files.
        let (loaded, problems) = Plugins::load(Some(&store));
        assert!(problems.is_empty(), "{problems:?}");
        let rust = loaded.get("rust").expect("rust");
        assert!(!rust.bundled, "the one on disk shadows the bundled one");
        assert_eq!(
            loaded.all().len(),
            bundled::ALL.len(),
            "shadowing replaces rather than adding a second one"
        );
    }

    #[test]
    fn a_plugin_that_is_switched_off_is_remembered() {
        let folder = std::env::temp_dir().join("unluminous-plugins-disabled");
        std::fs::remove_dir_all(&folder).ok();
        let store = Store::at(&folder);
        let (mut plugins, _) = Plugins::load(Some(&store));
        plugins.set_enabled(Some(&store), "javascript", false);
        let (again, _) = Plugins::load(Some(&store));
        assert!(!again.get("javascript").expect("javascript").enabled);
        assert!(again.get("rust").expect("rust").enabled);
    }

    #[test]
    fn a_folder_with_a_broken_manifest_is_skipped_and_reported() {
        let folder = std::env::temp_dir().join("unluminous-plugins-broken");
        std::fs::remove_dir_all(&folder).ok();
        let store = Store::at(&folder);
        let broken = store.folder().join(FOLDER).join("broken");
        std::fs::create_dir_all(&broken).expect("make the folder");
        std::fs::write(broken.join(MANIFEST), "this is not a manifest").expect("write it");
        let (plugins, problems) = Plugins::load(Some(&store));
        assert_eq!(problems.len(), 1, "the reason is reported rather than thrown away");
        assert_eq!(
            plugins.all().len(),
            bundled::ALL.len(),
            "Unluminous still has every one of its bundled plugins"
        );
    }
}
