//! A Word document in a tab of the editing area (`task-2341`).
//!
//! **A Word tab is a plugin tab of the `word` plugin, keyed by the document's path**, so the window's own
//! rules for a tab a plugin draws apply to it unchanged: it has no gutter and no preview, and its key
//! names the file so it comes back when the project is opened again. What a plugin tab did not have before
//! it is asked of the provider: whether it has unsaved changes (`UiProvider::modified`), saving it
//! (`UiProvider::save`), and that it closed (`UiProvider::tab_closed`).

use std::path::Path;

use crate::app::{files, Focus, UnluminousApp};

/// The plugin that draws Word documents.
pub(crate) const WORD: &str = "word";

impl UnluminousApp {
    /// Open the Word document at `path` in a tab, or show the tab already open on it.
    pub(crate) fn open_a_word_document(&mut self, path: &Path) -> Result<(), String> {
        let key = crate::services::word::tab_key(path);
        if let Some(index) = self.files.index_of_plugin_tab(&key) {
            self.files.show(index);
            self.show_the_editing_area_for_a_tab();
            self.focus = Focus::Plugin;
            self.plugin_with_the_keyboard = Some(WORD.to_owned());
            return Ok(());
        }
        let provider = self.plugin_ui.opened(WORD, WORD)?;
        provider.command("load", &[key.clone(), path.display().to_string()])?;
        let label = path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned());
        self.open_a_word_tab(&key, &label);
        Ok(())
    }

    /// Open the tab for a document the provider already holds.
    pub(crate) fn open_a_word_tab(&mut self, key: &str, label: &str) {
        self.files.open_plugin_tab(
            unluminous_core::Document::new(),
            files::PluginTab { key: key.to_owned(), plugin: WORD.to_owned(), label: label.to_owned() },
        );
        self.show_the_editing_area_for_a_tab();
        self.focus = Focus::Plugin;
        self.plugin_with_the_keyboard = Some(WORD.to_owned());
    }

    /// The key of the plugin tab at `index` when a provider draws one tab a file, as Word does.
    pub(crate) fn keyed_plugin_tab(&self, index: usize) -> Option<(String, String)> {
        let tab = self.files.get(index)?.plugin.as_ref()?;
        (tab.plugin == WORD).then(|| (tab.plugin.clone(), tab.key.clone()))
    }

    /// Whether the plugin tab at `index` has changes that are not saved.
    pub(crate) fn plugin_tab_is_modified(&mut self, index: usize) -> bool {
        let Some((plugin, key)) = self.keyed_plugin_tab(index) else { return false };
        self.plugin_ui.provider(&plugin).is_some_and(|provider| provider.modified(&key))
    }

    /// Save the plugin tab at `index`, to `to` when it is Save As.
    pub(crate) fn save_a_plugin_tab(&mut self, index: usize, to: Option<&Path>) -> Result<(), String> {
        let Some((plugin, key)) = self.keyed_plugin_tab(index) else { return Ok(()) };
        let Some(provider) = self.plugin_ui.provider(&plugin) else { return Ok(()) };
        provider.save(&key, to)
    }

    /// Tell the provider a keyed tab closed, so it can let its document go.
    pub(crate) fn a_plugin_tab_closed(&mut self, index: usize) {
        let Some((plugin, key)) = self.keyed_plugin_tab(index) else { return };
        if let Some(provider) = self.plugin_ui.provider(&plugin) {
            provider.tab_closed(&key);
        }
    }

    /// The tab a plugin asked to rename, after a Save As.
    pub(crate) fn rename_a_plugin_tab(&mut self, key: &str, label: &str) {
        if let Some(index) = self.files.index_of_plugin_tab(key) {
            if let Some(tab) = self.files.at_mut(index).plugin.as_mut() {
                tab.label = label.to_owned();
            }
        }
    }

    /// Close the keyed plugin tab a provider asked to close.
    pub(crate) fn close_a_plugin_tab(&mut self, key: &str) {
        if let Some(index) = self.files.index_of_plugin_tab(key) {
            self.close_tab(index);
        }
    }

    /// Whether the window's menus should leave this chord for the plugin tab that holds the keyboard.
    pub(crate) fn a_plugin_tab_claims(&mut self, key: egui::Key, modifiers: &egui::Modifiers) -> bool {
        if self.focus != Focus::Plugin {
            return false;
        }
        let index = self.files.active_index();
        let Some((plugin, _)) = self.keyed_plugin_tab(index) else { return false };
        if self.plugin_with_the_keyboard.as_deref() != Some(plugin.as_str()) {
            return false;
        }
        self.plugin_ui.provider(&plugin).is_some_and(|provider| provider.claims_key(key, modifiers))
    }
}
