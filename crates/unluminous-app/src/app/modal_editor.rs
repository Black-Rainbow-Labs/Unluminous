//! The window's own editor, drawn inside a plugin's modal (`task-2556`).
//!
//! *"I want the edit skill in the modal to be an actual file editor, with line numbers etc, just like in file editor
//! pane."* The supervisor modal reserves a rectangle in its Skill view and names the skill file
//! ([`UiProvider::take_the_embedded_file`]), and this draws the tab holding that file there with
//! [`UnluminousApp::show_a_document_in`], the function the editing area and a canvas note use. So the gutter, the
//! line numbers, coloring, folding, find, undo and the preview are the editing area's, with nothing copied.
//!
//! The tab lives in `Home::Modal`, which is in no pane, the way a File Editor node's tab lives on the canvas. While
//! the modal's editor has the keyboard the focus stays on that home between frames, so the window's own `Save`,
//! `Undo` and `Find` act on the skill file. It leaves when the modal draws no editor, a press lands outside it, or
//! `Open in a tab` moves the tab into a pane.
//!
//! [`UiProvider::take_the_embedded_file`]: crate::services::plugin_ui::UiProvider::take_the_embedded_file

use egui::Pos2;

use crate::app::files::Home;
use crate::app::{Focus, UnluminousApp, ViewMode};
use crate::services::plugin_ui::{EmbeddedFile, EmbeddedView};

/// Where the frame number is kept while the modal's editor holds the keyboard, which
/// [`crate::app::hold_the_keyboard`] reads so egui's focus stays off the modal's buttons while somebody types.
fn holding_id() -> egui::Id {
    egui::Id::new("unluminous-modal-editor-holds-the-keyboard")
}

/// Whether the editor in a modal held the keyboard on this frame or the one before.
///
/// @param ctx - the window's context
pub fn the_modal_editor_holds_the_keyboard(ctx: &egui::Context) -> bool {
    let frame = ctx.cumulative_frame_nr();
    ctx.data(|data| data.get_temp::<u64>(holding_id()))
        .is_some_and(|made| frame.saturating_sub(made) <= 1)
}

/// The editing area's view mode for the modal's view.
fn view_mode_for(view: EmbeddedView) -> ViewMode {
    match view {
        EmbeddedView::Source => ViewMode::Raw,
        EmbeddedView::Split => ViewMode::SideBySide,
        EmbeddedView::Preview => ViewMode::Preview,
    }
}

impl UnluminousApp {
    /// Draw the editor a modal asked for, or put it away when no modal asked this frame. Answers whether the
    /// file it drew has changes not saved, for the provider that asked.
    ///
    /// @param ctx - the window's context
    /// @param wanted - what the modal asked for this frame
    pub(crate) fn show_the_modal_editor(
        &mut self,
        ctx: &egui::Context,
        wanted: Option<EmbeddedFile>,
    ) -> Option<bool> {
        let Some(wanted) = wanted else {
            self.put_the_modal_editor_away();
            return None;
        };
        let index = match self.the_tab_for_the_modal(&wanted.path) {
            Ok(index) => index,
            Err(problem) => {
                self.message = Some(problem);
                return None;
            }
        };
        self.files.at_mut(index).view_mode = view_mode_for(wanted.view);
        self.notice_a_press_outside_the_modal_editor(ctx, &wanted);
        let focused = self.modal_editor_has_the_keyboard;
        let mut ui = egui::Ui::new(
            ctx.clone(),
            egui::Id::new("unluminous-modal-editor"),
            egui::UiBuilder::new().layer_id(wanted.layer).max_rect(wanted.area),
        );
        ui.set_clip_rect(wanted.area);
        // The editor's glyphs are drawn by Unluminous's own renderer, which is told the modal's transform, as the
        // ticket modal's terminal does.
        let transform = ctx.layer_transform_to_global(wanted.layer).unwrap_or_default();
        let was_compositing = self.renderer.composite_through(transform, true);
        let was = self.files.focus();
        self.files.focus_modal();
        let took =
            self.show_a_document_in(&mut ui, wanted.area, focused, "supervisor skill preview");
        self.renderer.restore_compositing(was_compositing);
        if took {
            self.modal_editor_has_the_keyboard = true;
            self.focus = Focus::Editor;
        }
        if self.modal_editor_has_the_keyboard {
            let frame = ctx.cumulative_frame_nr();
            ctx.data_mut(|data| data.insert_temp(holding_id(), frame));
        } else {
            // A focus borrowed only to draw goes back where it was.
            let back = match was {
                Home::Modal => Home::Pane(self.files.focused_pane()),
                other => other,
            };
            self.files.restore_focus(back);
        }
        Some(self.files.at(index).document.is_modified())
    }

    /// The tab holding `path` in the modal: the one already there, the file's tab moved in from a pane, or the file
    /// opened.
    fn the_tab_for_the_modal(&mut self, path: &std::path::Path) -> Result<usize, String> {
        if let Some(index) = self.files.tab_in_modal() {
            if self.files.at(index).path() == Some(path) {
                return Ok(index);
            }
        }
        if self.files.index_of(path).is_none() {
            let focus = self.focus;
            let home = self.files.focus();
            self.open_path_without_showing_the_editing_area(path, true, false)?;
            self.focus = focus;
            self.files.restore_focus(home);
        }
        let index =
            self.files.index_of(path).ok_or_else(|| format!("{} did not open.", path.display()))?;
        self.files.move_to_modal(index);
        self.files.tab_in_modal().ok_or_else(|| format!("{} did not open.", path.display()))
    }

    /// A press anywhere outside the modal's editor takes the keyboard from it.
    fn notice_a_press_outside_the_modal_editor(
        &mut self,
        ctx: &egui::Context,
        wanted: &EmbeddedFile,
    ) {
        if !self.modal_editor_has_the_keyboard {
            return;
        }
        let pressed: Option<Pos2> = ctx.input(|input| {
            input.pointer.any_pressed().then(|| input.pointer.press_origin()).flatten()
        });
        let Some(at) = pressed else { return };
        let transform = ctx.layer_transform_to_global(wanted.layer).unwrap_or_default();
        let inside = wanted.area.contains(transform.inverse() * at);
        if !inside {
            self.let_the_modal_editor_go();
        }
    }

    /// The modal drew no editor this frame: the keyboard leaves it, and the tab stays in `Home::Modal` with
    /// whatever was typed into it, so opening the modal again shows the same draft.
    fn put_the_modal_editor_away(&mut self) {
        self.let_the_modal_editor_go();
    }

    /// Take the keyboard out of the modal's editor and put the focus back in a pane.
    fn let_the_modal_editor_go(&mut self) {
        self.modal_editor_has_the_keyboard = false;
        if self.files.focus() == Home::Modal {
            self.files.restore_focus(Home::Pane(self.files.focused_pane()));
        }
    }

    /// Save the tab holding `path`, as `File -> Save` saves the tab that is showing. A tab with no changes is
    /// left alone.
    pub(crate) fn save_the_tab_holding(&mut self, path: &std::path::Path) {
        let Some(index) = self.files.index_of(path) else { return };
        if !self.files.at(index).document.is_modified() {
            return;
        }
        let was = self.files.focus();
        let home = self.files.home_of(index);
        self.files.restore_focus(home);
        self.files.show(index);
        self.save();
        self.files.restore_focus(was);
    }

    /// `Open in a tab` on a file the modal holds: the tab leaves the modal for a pane, and the editing area is
    /// shown with it. Answers whether the file was in the modal.
    pub(crate) fn move_the_modal_tab_to_a_pane(&mut self, path: &std::path::Path) -> bool {
        let Some(index) = self.files.tab_in_modal() else { return false };
        if self.files.at(index).path() != Some(path) {
            return false;
        }
        self.modal_editor_has_the_keyboard = false;
        if let Some(moved) = self.files.move_out_of_the_modal() {
            self.files.show(moved);
        }
        self.focus = Focus::Editor;
        self.show_the_editing_area_for_a_tab();
        true
    }
}
