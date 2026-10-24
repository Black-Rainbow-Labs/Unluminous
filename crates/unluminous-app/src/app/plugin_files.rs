//! Files a `ui` plugin draws in a tab of their own (`task-2338`).
//!
//! The Spreadsheet plugin claims `.xlsx`, `.xlsm`, `.csv` and `.tsv` through `ui.extensions`, and each such
//! file opens in a plugin tab keyed `<plugin>:<path>`, drawn by the provider's `tab_keyed`. The window
//! asks the provider whether the tab has unsaved changes (the dot in the tab strip), has it save the tab
//! (`Ctrl+S`, `File -> Save`, closing the tab, closing the window), and tells it when the tab closes.
//!
//! The Realm plugin claims `.realm` the same way and is the one exception: its canvas is the window's own,
//! so it keeps the path it always had through `realm_files`.

use std::path::{Path, PathBuf};

use super::{files, Focus, UnluminousApp};

impl UnluminousApp {
    /// The plugin that draws `path` in a tab of its own, and its provider, when a switched on plugin claims it.
    pub(crate) fn plugin_that_draws(&self, path: &Path) -> Option<(String, String)> {
        let provider = self.plugins.opens_elsewhere(path)?;
        if provider == "realm" {
            return None;
        }
        let extension = path.extension()?.to_str()?.to_lowercase();
        let plugin = self
            .plugins
            .all()
            .iter()
            .find(|p| p.enabled && p.contributions.extensions.contains(&extension))
            .map(|p| p.id.clone())?;
        Some((plugin, provider.to_owned()))
    }

    /// Open a file in the tab its plugin draws, or show that tab when it is already open.
    pub(crate) fn open_a_file_in_a_plugin_tab(
        &mut self,
        plugin: &str,
        provider: &str,
        path: &Path,
    ) -> Result<(), String> {
        // A Word document is read under the key the Word plugin gives it. `task-2341`.
        if plugin == super::word_files::WORD {
            return self.open_a_word_document(path);
        }
        let opened = self.plugin_ui.opened(plugin, provider)?;
        let words = vec![path.to_string_lossy().into_owned()];
        // `use` reads the file into the provider and makes it the one its commands act on, without asking for a
        // tab, which is what is being made here.
        opened.command("use", &words)?;
        let key = plugin_tab_key(plugin, path, self.tree.root());
        self.show_the_editing_area_for_a_tab();
        let label = path.file_name().and_then(|n| n.to_str()).unwrap_or("workbook").to_owned();
        self.files.open_plugin_tab(
            unluminous_core::Document::new(),
            files::PluginTab { key, plugin: plugin.to_owned(), label },
        );
        self.focus = Focus::Plugin;
        self.plugin_with_the_keyboard = Some(plugin.to_owned());
        if let Some(provider) = self.plugin_ui.provider(plugin) {
            provider.keyboard(true);
        }
        self.message = None;
        Ok(())
    }

    /// What to say about a file a plugin that is switched off would open: `The Spreadsheet plugin opens .xlsx
    /// files, and it is switched off.`
    pub(crate) fn plugin_that_would_draw(&self, path: &Path) -> Option<String> {
        let extension = path.extension()?.to_str()?.to_lowercase();
        let plugin = self
            .plugins
            .all()
            .iter()
            .find(|p| !p.enabled && p.contributions.extensions.contains(&extension))?;
        Some(format!("The {} plugin opens .{extension} files, and it is switched off. Settings, Plugins switches it on.", plugin.name))
    }

    /// Whether a plugin tab holds changes that are not saved.
    pub(crate) fn plugin_tab_is_modified(&self, tab: &files::PluginTab) -> bool {
        self.plugin_ui.modified(&tab.plugin, &tab.key)
    }

    /// Save a plugin tab through its provider, answering the line for the status bar.
    pub(crate) fn save_a_plugin_tab(&mut self, tab: &files::PluginTab) -> Result<String, String> {
        match self.plugin_ui.provider(&tab.plugin) {
            Some(provider) => provider.save(&tab.key),
            None => Err(format!("{} is not open", tab.label)),
        }
    }

    /// Tell a plugin that one of its tabs closed.
    pub(crate) fn a_plugin_tab_closed(&mut self, tab: &files::PluginTab) {
        if let Some(provider) = self.plugin_ui.provider(&tab.plugin) {
            provider.closed(&tab.key);
        }
    }

    /// The plugin whose tab is showing and holds the keyboard, which reads its own key chords before the menus.
    pub(crate) fn the_plugin_tab_with_the_keyboard(&self) -> Option<String> {
        if !matches!(self.focus, Focus::Plugin | Focus::Editor) {
            return None;
        }
        let tab = self.files.active().plugin.as_ref()?;
        if self.focus == Focus::Plugin
            && self.plugin_with_the_keyboard.as_deref().is_some_and(|p| p != tab.plugin)
        {
            return None;
        }
        Some(tab.plugin.clone())
    }

    /// Whether a tab key names a file a switched on plugin draws, so the tab is kept while that plugin is on.
    pub(crate) fn a_file_tab_is_still_drawn(&self, tab: &files::PluginTab) -> bool {
        file_of_a_plugin_tab_key(&tab.key, self.tree.root()).is_some_and(|(plugin, path)| {
            self.plugin_that_draws(&path).is_some_and(|(p, _)| p == plugin)
        })
    }

    /// Reopen a file tab a project remembered.
    pub(crate) fn reopen_a_file_tab(&mut self, key: &str) -> bool {
        let Some((plugin, path)) = file_of_a_plugin_tab_key(key, self.tree.root()) else {
            return false;
        };
        let Some((owner, provider)) = self.plugin_that_draws(&path) else { return false };
        if owner != plugin || !path.is_file() {
            return false;
        }
        self.open_a_file_in_a_plugin_tab(&plugin, &provider, &path).is_ok()
    }
}

/// The key of the tab that draws a file: `<plugin>:<path>`, the path relative to the project when inside it.
pub fn plugin_tab_key(plugin: &str, path: &Path, project: &Path) -> String {
    let shown = match path.strip_prefix(project) {
        Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
        Err(_) => path.to_string_lossy().into_owned(),
    };
    format!("{plugin}:{shown}")
}

/// The plugin and the file a file tab's key names.
pub fn file_of_a_plugin_tab_key(key: &str, project: &Path) -> Option<(String, PathBuf)> {
    let (plugin, rest) = key.split_once(':')?;
    // A key with a drive letter after the plugin's name, `spreadsheet:C:/x.xlsx`, splits on its first colon.
    if plugin.is_empty() || plugin.contains(['/', '\\']) || rest.is_empty() {
        return None;
    }
    let path = PathBuf::from(rest);
    Some((plugin.to_owned(), if path.is_absolute() { path } else { project.join(path) }))
}
