//! A Word document in a tab of the editing area (`task-2341`).
//!
//! **A Word tab is a plugin tab of the `word` plugin, keyed by the document's path**, so the window's own
//! rules for a tab a plugin draws apply to it unchanged: it has no gutter and no preview, and its key
//! names the file so it comes back when the project is opened again. Unsaved changes, saving and closing go
//! through `plugin_files`, which the Spreadsheet plugin (`task-2338`) uses the same way; what is here is what a
//! Word document needs beyond that: it is read with the `load` command under its own key, it can be saved to
//! another file, and its provider can ask for a tab to be opened, renamed or closed.

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
        let label = path
            .file_name()
            .map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned());
        self.open_a_word_tab(&key, &label);
        Ok(())
    }

    /// Open the tab for a document the provider already holds.
    pub(crate) fn open_a_word_tab(&mut self, key: &str, label: &str) {
        self.files.open_plugin_tab(
            unluminous_core::Document::new(),
            files::PluginTab {
                key: key.to_owned(),
                plugin: WORD.to_owned(),
                label: label.to_owned(),
            },
        );
        self.show_the_editing_area_for_a_tab();
        self.focus = Focus::Plugin;
        self.plugin_with_the_keyboard = Some(WORD.to_owned());
    }

    /// The tab a plugin asked to rename, after a Save As.
    pub(crate) fn rename_a_plugin_tab(&mut self, key: &str, label: &str) {
        if let Some(index) = self.files.index_of_plugin_tab(key) {
            if let Some(tab) = self.files.at_mut(index).plugin.as_mut() {
                tab.label = label.to_owned();
            }
        }
    }

    /// Save the active plugin tab to another file, which is `File -> Save As` for a Word document.
    pub(crate) fn save_a_plugin_tab_as(&mut self, to: &Path) -> Result<String, String> {
        let Some(tab) = self.files.active().plugin.clone() else {
            return Err("this tab has no file to save".to_owned());
        };
        match self.plugin_ui.provider(&tab.plugin) {
            Some(provider) => provider.save_as(&tab.key, to),
            None => Err(format!("{} is not open", tab.label)),
        }
    }

    /// Close the keyed plugin tab a provider asked to close.
    pub(crate) fn close_a_plugin_tab(&mut self, key: &str) {
        if let Some(index) = self.files.index_of_plugin_tab(key) {
            self.close_tab(index);
        }
    }
}
