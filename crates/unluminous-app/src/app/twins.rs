//! A file open in two places at once: the editing area, and the File Editor node it was sent to.
//!
//! `task-2320`: *"When I send a file or browser to realm, it should stay open in the file editor, and also
//! show up in the realm."* Asked which of three ways, Jason chose **one document, two views**: typing in
//! either shows in both, there is one undo history and one save.
//!
//! Unluminous holds a document in its tab, and everything about editing reads `files.active().document`, so
//! two tabs cannot share one `Document` without changing every one of those reads. What is done instead:
//!
//! - **One view holds the document**, with its undo history. The other holds a copy with no history,
//!   [`unluminous_core::Document::copy_for_another_view`], and is marked [`OpenFile::follows`].
//! - **The copy is made again whenever the original's text, saved state or folds change**, once a frame,
//!   in [`UnluminousApp::keep_the_twins_in_step`]. A caret moving in the original copies nothing.
//! - **The view with the keyboard holds the document.** When the keyboard moves to the copy, the two
//!   exchange documents first, so the keys typed there go into the document with the history and undo
//!   undoes them. A copy edited some other way takes the document over in the same way.
//! - **Closing one view closes nothing else and writes nothing**, because the other view still has the
//!   document. A closing original hands the document to the copy first.
//!
//! Only a plain text document has a second view. A notebook, a picture or a tab with no file still moves
//! to the node, as it did before.
//!
//! [`OpenFile::follows`]: crate::app::files::OpenFile::follows

use crate::app::files::{Followed, OpenFile};
use crate::app::UnluminousApp;

impl UnluminousApp {
    /// The other view of tab `index`'s file, when it is open in two places.
    /// @param index - a tab
    pub(crate) fn twin_of(&self, index: usize) -> Option<usize> {
        let path = self.files.get(index)?.path()?.to_path_buf();
        (0..self.files.len())
            .find(|other| *other != index && self.files.at(*other).path() == Some(path.as_path()))
    }

    /// Whether tab `index` may have a second view: a text document saved somewhere, not a notebook.
    /// @param index - a tab
    pub(crate) fn may_have_a_second_view(&self, index: usize) -> bool {
        self.files.get(index).is_some_and(|file| {
            file.is_a_document()
                && file.path().is_some()
                && file.notebook.is_none()
                && file.picture.is_none()
                && file.unrenderable.is_none()
        })
    }

    /// Show tab `index`'s file on `node` as well, as a second view of the same document. Answers the new
    /// tab's index. The tab stays where it is.
    /// @param index - the tab that stays
    /// @param node - the File Editor node the second view goes on
    pub(crate) fn put_a_second_view_on_a_node(&mut self, index: usize, node: u64) -> usize {
        let source = self.files.at(index);
        let copy = source.document.copy_for_another_view(source.document.selection());
        let mut view = OpenFile::new(copy);
        view.view_mode = source.view_mode;
        view.disk = source.disk.clone();
        view.follows = Some(followed(source));
        self.files.add_to_node(view, node)
    }

    /// Once a frame: hand the document to the view with the keyboard, and copy it into the other view when
    /// it changed. Costs one walk of the tabs and nothing more while no file is open in two places.
    pub(crate) fn keep_the_twins_in_step(&mut self) {
        if !self.files.iter().any(|file| file.follows.is_some()) {
            return;
        }
        let keyboard = self.files.showing_at(self.files.focus());
        for index in 0..self.files.len() {
            let Some(seen) = self.files.at(index).follows else { continue };
            let Some(original) =
                self.twin_of(index).filter(|other| self.files.at(*other).follows.is_none())
            else {
                // The other view has gone, so this one is the only view and holds what it has.
                self.files.at_mut(index).follows = None;
                continue;
            };
            let edited_here = self.files.at(index).document.text_revision() != seen.text;
            if keyboard == Some(index) || edited_here {
                self.hand_the_document_to(index, original);
            } else if followed(self.files.at(original)) != seen {
                self.copy_the_document(original, index);
                // The copy is drawn on the next frame, which an idle window would not draw for half a second.
                if let Some(ctx) = &self.context {
                    ctx.request_repaint();
                }
            }
        }
    }

    /// Before tab `index` closes: when its file is open in another view too, make sure that view holds the
    /// document. Answers whether there is such a view, in which case closing this one writes nothing.
    /// @param index - the tab about to close
    pub(crate) fn hand_over_before_closing(&mut self, index: usize) -> bool {
        let Some(other) = self.twin_of(index) else {
            return false;
        };
        if self.files.at(index).follows.is_none() {
            self.hand_the_document_to(other, index);
        }
        self.files.at_mut(other).follows = None;
        true
    }

    /// Make tab `to` hold the document and tab `from` a copy of it, each keeping its own caret.
    fn hand_the_document_to(&mut self, to: usize, from: usize) {
        let caret_to = self.files.at(to).document.selection();
        let caret_from = self.files.at(from).document.selection();
        self.files.swap_documents(to, from);
        let copy_for_from = self.files.at(to).document.copy_for_another_view(caret_from);
        let followed_now = followed(self.files.at(to));
        self.files.at_mut(to).document.place_the_selection(caret_to);
        self.files.at_mut(to).follows = None;
        let from_tab = self.files.at_mut(from);
        from_tab.document = copy_for_from;
        from_tab.follows = Some(followed_now);
    }

    /// Copy tab `from`'s document into tab `to`, which follows it, keeping `to`'s caret.
    fn copy_the_document(&mut self, from: usize, to: usize) {
        let caret = self.files.at(to).document.selection();
        let source = self.files.at(from);
        let copy = source.document.copy_for_another_view(caret);
        let seen = followed(source);
        let tab = self.files.at_mut(to);
        tab.document = copy;
        tab.follows = Some(seen);
    }
}

/// What a copy of `file`'s document would record about it.
fn followed(file: &OpenFile) -> Followed {
    Followed {
        text: file.document.text_revision(),
        modified: file.document.is_modified(),
        folds: file.document.fold_revision(),
    }
}
