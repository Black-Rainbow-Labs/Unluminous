//! Find in the Markdown preview (`task-2786`).
//!
//! *"Find for MD preview should work just like in edit mode. currently no find box appears when I do CMD+F."*
//! The Find bar was drawn only over the source, and in the Preview view mode the source is not drawn, so the bar
//! was opened and nothing showed it. Here the bar is drawn over the preview and searches the preview's own text,
//! which is what the person is reading: a heading without its hashes, a table as it is laid out.
//!
//! **It is the same `services::find::Find`**, marked `read_only`, because the preview is worked out from the
//! source and a byte range in it is not a byte range in the file: there is no Replace. The current match is the
//! preview's selection, so `Cmd+C` copies it and Escape leaves it selected, and every other match is painted
//! behind the words in the find match colour, as the source does.

use egui::Pos2;

use crate::app::{UnluminousApp, ViewMode};
use crate::services::file_kind;

impl UnluminousApp {
    /// Whether Find is about the preview of the tab that is showing: a Markdown document shown as its preview
    /// alone, in a pane or on a node.
    pub(crate) fn find_is_in_the_preview(&self) -> bool {
        let file = self.files.active();
        file.is_a_document()
            && file.notebook.is_none()
            && self.view_mode() == ViewMode::Preview
            && self.preview_applies_here()
            && !file_kind::is_mermaid(self.document().path())
    }

    /// A number that moves whenever the preview's text could have, which is what `Find::refresh` is keyed on.
    /// The text revision of the source alone is not enough, because a table is laid out to the width it is
    /// drawn at and so the preview's text changes with the width.
    fn preview_find_revision(&self) -> u64 {
        let cached = &self.files.active().cached;
        let length =
            cached.preview.as_ref().map(|preview| preview.text.len_bytes()).unwrap_or(0) as u64;
        cached.preview_revision.wrapping_mul(1_000_003)
            ^ (cached.preview_width.to_bits() as u64).rotate_left(17)
            ^ length.rotate_left(41)
    }

    /// Open the Find bar over the preview, seeded with what the preview has selected.
    pub(crate) fn open_the_find_in_the_preview(&mut self) {
        let selected = self.preview_selected_text();
        let mut find = crate::services::find::Find::opened_with(selected.as_deref(), false);
        find.read_only = true;
        if let Some(text) =
            self.files.active().cached.preview.as_ref().map(|preview| preview.text.to_string())
        {
            find.refresh(&text, self.preview_find_revision());
            find.start_from(self.files.active().preview_selection.start());
        }
        let current = find.current();
        self.find = Some(find);
        if let Some(range) = current {
            self.select_a_preview_match(range);
        }
    }

    /// Select `range` of the preview's text and scroll it into view on the next drawing.
    pub(crate) fn select_a_preview_match(&mut self, range: std::ops::Range<usize>) {
        let length = self
            .files
            .active()
            .cached
            .preview
            .as_ref()
            .map(|preview| preview.text.len_bytes())
            .unwrap_or(0);
        if range.end > length {
            return;
        }
        self.files.active_mut().preview_selection =
            unluminous_core::Selection::new(range.start, range.end);
        self.reading_preview = true;
        self.preview_reveal = true;
    }

    /// Work the matches out against the preview as it is drawn now, and answer the ones that are not current.
    /// Called while the preview is drawn, after it was built for this frame.
    pub(crate) fn find_in_the_preview(&mut self) -> Vec<std::ops::Range<usize>> {
        let revision = self.preview_find_revision();
        let Some(text) =
            self.files.active().cached.preview.as_ref().map(|preview| preview.text.to_string())
        else {
            return Vec::new();
        };
        let Some(find) = self.find.as_mut() else { return Vec::new() };
        find.read_only = true;
        let changed = find.refresh(&text, revision);
        let current = find.current();
        let others = find.others();
        if changed {
            if let Some(range) = current {
                self.select_a_preview_match(range);
            }
        }
        others
    }

    /// Scroll the preview so its selection is on the screen, when a match asked for it.
    ///
    /// @param view_height - how tall the part of the preview the pane shows is
    pub(crate) fn reveal_the_preview_selection(&mut self, view_height: f32) {
        if !std::mem::take(&mut self.preview_reveal) {
            return;
        }
        let range = self.files.active().preview_selection.range();
        let Some(first) = self.preview_layout().selection_rects(range).into_iter().next() else {
            return;
        };
        let scroll = self.files.active().preview_scroll;
        let (top, bottom) = (first.y, first.y + first.height);
        // The least distance that brings the match into view, with a little room above it, which is what
        // stepping through matches in the source does.
        let room = (view_height * 0.25).min(80.0);
        let wanted = if top < scroll + room {
            top - room
        } else if bottom > scroll + view_height - room {
            bottom - view_height + room
        } else {
            scroll
        };
        self.files.active_mut().preview_scroll = wanted.max(0.0);
    }

    /// Paint the matches that are not current behind the preview's words.
    pub(crate) fn paint_the_preview_matches(
        &self,
        ui: &egui::Ui,
        origin: Pos2,
        others: &[std::ops::Range<usize>],
    ) {
        for range in others {
            crate::components::editor_view::paint_behind(
                ui,
                self.preview_layout(),
                origin,
                range.clone(),
                crate::theme::color::find_match(),
                2.0,
            );
        }
    }
}
