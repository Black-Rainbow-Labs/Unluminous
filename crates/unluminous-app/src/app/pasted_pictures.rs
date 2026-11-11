//! A picture pasted into a Markdown file: written under `images/` beside the file and linked at the caret
//! (`task-2634`).
//!
//! Three ways in, one function behind them: the paste chord's key going up in the editor
//! (`editor_view::ViewOutcome::paste_released`), `Edit -> Paste` (which is what `Cmd+V` reaches on macOS), and
//! `unluminous-cli editor paste-picture` for an agent. `services::pasted_picture` reads the clipboard and writes
//! the file; this is the half that knows which tab is showing and types into it.

use std::path::{Path, PathBuf};

use super::*;
use crate::services::pasted_picture;

impl UnluminousApp {
    /// Paste the clipboard's picture into the Markdown file that is showing, and answer whether anything was
    /// done: a picture pasted, or a picture that could not be read and said so in the status bar.
    ///
    /// Nothing happens, and `false` comes back, when the tab is not a Markdown file, when the clipboard holds
    /// text (that was a text paste), or when it holds no picture.
    pub(crate) fn paste_a_picture_from_the_clipboard(&mut self) -> bool {
        let Some(document) = self.markdown_file_showing() else {
            return false;
        };
        let bytes = match pasted_picture::from_the_clipboard() {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return false,
            Err(problem) => {
                self.message = Some(format!("The picture was not pasted: {problem}"));
                return true;
            }
        };
        if let Err(problem) = self.paste_a_picture(&document, &bytes, "png") {
            self.message = Some(format!("The picture was not pasted: {problem}"));
        }
        true
    }

    /// The Markdown file the editing area is showing, if it is one. A rendered page and a picture tab are not.
    pub(crate) fn markdown_file_showing(&self) -> Option<PathBuf> {
        let file = self.files.active();
        if file.is_browser() || file.is_picture() {
            return None;
        }
        file.path().filter(|path| pasted_picture::is_markdown(path)).map(Path::to_path_buf)
    }

    /// Write a picture beside `document` and type its link at the caret, as a paragraph of its own, as one undo
    /// step.
    ///
    /// @param document - the Markdown file showing in the editing area
    /// @param bytes - the picture, in the format `extension` names
    /// @param extension - the extension to write it under, such as `png`
    pub(crate) fn paste_a_picture(
        &mut self,
        document: &Path,
        bytes: &[u8],
        extension: &str,
    ) -> Result<pasted_picture::Saved, String> {
        let saved = pasted_picture::save_beside(document, bytes, extension)?;
        let (before, after) = self.the_lines_round_the_selection();
        let typed = pasted_picture::on_a_line_of_its_own(&saved.link, &before, &after);
        self.document_mut().apply(unluminous_core::Command::Insert(typed));
        // Pasting into a file you were only glancing at means you meant to open it, as typing does.
        let active = self.files.active_index();
        self.files.make_permanent(active);
        self.forget_layout();
        self.reveal_caret = true;
        let shown = saved
            .path
            .strip_prefix(document.parent().unwrap_or(Path::new("")))
            .unwrap_or(&saved.path);
        self.message = Some(format!("Pasted the picture as {}", shown.display()));
        Ok(saved)
    }

    /// The text before the selection from the start of the line above it, and after the selection to the end of
    /// the line below it, which is enough to tell whether a blank line is already there on either side.
    fn the_lines_round_the_selection(&self) -> (String, String) {
        let text = self.document().text();
        let range = self.document().selection().range();
        let start = text.line_to_byte(text.byte_to_line(range.start).saturating_sub(1));
        let below = text.byte_to_line(range.end) + 2;
        let end = match below < text.len_lines() {
            true => text.line_to_byte(below),
            false => text.len_bytes(),
        };
        (text.byte_slice(start..range.start), text.byte_slice(range.end..end))
    }
}
