//! Zooming a pane, and keeping the line you were reading where it was.
//!
//! A gesture belongs to the **window** rather than to a pane, because the size is one setting for the
//! whole window and the pointer says which pane it is about. What one step means differs by pane:
//! where a pane already has a point size a person chooses the zoom walks that setting, and where it
//! has none the zoom is a multiplier over everything it draws.
//!
//! A scroll position means something different at a different size, so what is remembered across one
//! is an `Anchor` — the line the point fell on and how far down it sat — rather than a number of
//! points.

use egui::Rect;

use crate::settings::{self};
use crate::theme::size;

use crate::app::{dock, files};
use crate::app::{UnluminousApp, ZoomClaim, ZOOM_STEP};

impl UnluminousApp {
    /// A pinch on the trackpad, or the wheel with the zoom modifier held, over the editing area.
    ///
    /// `zoom_delta` reports both as one multiplier, which is the reference editor's control and mouse wheel for
    /// nothing, and egui holds the scroll back while the modifier is down so the document does not
    /// slide about while it is being zoomed.
    ///
    /// It walks the same sizes the Settings window offers and the keyboard steps through, rather
    /// than setting whatever size the multiplier works out to. Two reasons. A size the dialog cannot
    /// show is a size a person cannot get back to, and one step per notch of a wheel is what every
    /// other editor does.
    ///
    /// The gesture is accumulated rather than applied a frame at a time, because it arrives as a
    /// stream of multipliers a fraction over one: a step is taken each time what has been asked for
    /// reaches [`ZOOM_STEP`], and the remainder is carried into the next frame, so a slow pinch and
    /// a fast one both end up where the fingers say. Nothing here needs to know what one notch of a
    /// wheel is worth in points, which is a platform's business and differs between mice.
    ///
    /// `above` is how far below the top of the view the point the gesture is about sits: where the
    /// pointer is, or the top of the view when the gesture arrived with the pointer somewhere else.
    /// Whatever text is there is what the zoom is not allowed to move, which is what `task-1672`
    /// asks for — a person zooming in is zooming in on the line they are looking at, and having to
    /// scroll back to it afterwards is the whole complaint.
    ///
    /// What comes out is written to the settings file once the pointer is up rather than on every
    /// frame, by the rule `ui` already keeps for a dragged divider.
    pub(crate) fn zoom_the_text(&mut self, ui: &egui::Ui, above: f32) {
        let steps = self.zoom_steps(ui);
        if steps == 0 {
            return;
        }
        // Counted first and taken afterwards, so the point that is to stay put is read off the
        // layout as it is now — before `set_font_size` marks it stale — however many sizes one
        // frame of the gesture turns out to be worth.
        self.anchor_the_view(above);
        for _ in 0..steps.abs() {
            self.set_font_size(settings::step_font_size(self.settings.font_size, steps > 0));
        }
    }

    /// How many whole steps this frame's gesture is worth, and nothing else.
    ///
    /// Split out from [`Self::zoom_the_text`] because `task-1771` makes **every** pane zoomable and what
    /// one step means differs by pane: a size off the Settings window's list for the editing area and for a
    /// terminal, a multiplier for the explorer and for a pane a plugin contributed. The accumulation is the
    /// same for all of them and lives here, once, so a wheel turned the same distance is worth the same
    /// number of steps wherever the pointer happens to be.
    ///
    /// Called at most once a frame, by whichever pane claimed the gesture — which is what [`ZoomClaim`] is
    /// for. Calling it twice would spend the same notch twice.
    pub(crate) fn zoom_steps(&mut self, ui: &egui::Ui) -> i32 {
        let gesture = ui.input(|input| input.zoom_delta());
        if (gesture - 1.0).abs() < f32::EPSILON {
            return 0;
        }
        self.zoom_pending *= gesture;
        let mut steps = 0i32;
        while self.zoom_pending >= ZOOM_STEP {
            self.zoom_pending /= ZOOM_STEP;
            steps += 1;
        }
        while self.zoom_pending <= 1.0 / ZOOM_STEP {
            self.zoom_pending *= ZOOM_STEP;
            steps -= 1;
        }
        steps
    }

    /// A pinch, or the wheel with the zoom modifier held, over a panel that is not the editing area.
    ///
    /// `task-1771`: *"we want every pane (file panel, folders, terminal, agent chat, agent tasks, etc) to
    /// be zoomable with Ctrl/Cmd + or Ctrl/Cmd scroll wheel."* The claim is the editing area's own — the
    /// pointer decides whose the gesture is, and the first pane to find the pointer inside itself takes it
    /// — so a wheel turned over the explorer cannot also step the editor's font, which is the fault
    /// the claim was written for in the first place.
    ///
    /// The pointer is asked for as `hover_pos().or(latest_pos())` for the reason the editing area records:
    /// `egui` reports no pointer at all on a frame whose only input is a wheel event, so a gesture gated on
    /// the hover alone is a gesture thrown away.
    pub(crate) fn zoom_over_a_panel(&mut self, ui: &egui::Ui, panel: dock::Panel, area: Rect) {
        if self.zoom == ZoomClaim::Taken || area.width() < 1.0 || area.height() < 1.0 {
            return;
        }
        let Some(at) = ui
            .input(|input| input.pointer.hover_pos().or_else(|| input.pointer.latest_pos()))
            .filter(|at| area.contains(*at))
        else {
            return;
        };
        self.zoom = ZoomClaim::Taken;
        let steps = self.zoom_steps(ui);
        if steps == 0 {
            return;
        }
        self.step_the_zoom_of(panel, steps, Some(at.y - area.top()));
    }

    /// Take `steps` off or on to what `panel` is drawn at.
    ///
    /// **Where a panel already has a point size a person chooses, the zoom walks that setting.** The three
    /// tiles are character grids drawn at `terminal.font.size`, which the Settings window offers a list for,
    /// so the wheel walks that list: one number says how big a terminal is rather than a setting and a
    /// multiplier that can disagree. The explorer and a contributed pane have no such number, so theirs is a
    /// multiplier over everything they draw — `settings::ZOOMS`.
    ///
    /// `above` is how far below the top of the panel the pointer was, when there was one. A list's content
    /// scales with its zoom, so the point at `offset + above` moves to `(offset + above) * ratio`; putting
    /// it back under the pointer is one subtraction. Only the explorer keeps its scroll where the window can
    /// reach it; a provider is told the ratio instead and corrects its own, which is what `UiProvider::zoomed`
    /// is for.
    pub(crate) fn step_the_zoom_of(&mut self, panel: dock::Panel, steps: i32, above: Option<f32>) {
        let up = steps > 0;
        match panel {
            dock::Panel::Terminal | dock::Panel::Run | dock::Panel::Debug => {
                let mut size = self.settings.terminal_font_size;
                for _ in 0..steps.abs() {
                    size = settings::step_terminal_font_size(size, up);
                }
                if (size - self.settings.terminal_font_size).abs() < 0.01 {
                    return;
                }
                self.settings.terminal_font_size = size;
                self.unsaved_settings = true;
            }
            // **The canvas's zoom is its camera's.** A pane multiplier on top of a camera would be
            // two numbers meaning one thing, which is the same reason the three tiles walk the
            // terminal's font size instead of having one - `task-1904`. The keys zoom about the
            // middle of the canvas, because the keyboard has no pointer; the wheel zooms about the
            // pointer, in `take_the_canvas_input`.
            dock::Panel::Space => {
                let body = self.space.body;
                // **Aimed rather than set**, so the keys and the modifier wheel glide the way the plain
                // wheel does — see `UnluminousApp::aim_the_zoom_at`. `task-1945`.
                let wanted = self.aimed_zoom() * 1.1_f32.powi(steps);
                self.aim_the_zoom_at(wanted, body.center());
            }
            dock::Panel::Explorer | dock::Panel::Plugin(_) => {
                let was = self.panes.zoom_of(panel);
                let mut zoom = was;
                for _ in 0..steps.abs() {
                    zoom = settings::step_zoom(zoom, up);
                }
                if (zoom - was).abs() < 0.001 {
                    return;
                }
                self.panes.set_zoom_of(panel, zoom);
                self.unsaved_settings = true;
                self.keep_the_place_through_a_panels_zoom(panel, zoom / was, above);
            }
        }
        if let Some(context) = &self.context {
            context.request_repaint();
        }
    }

    /// Put the point the pointer was over back under the pointer, at the panel's new size.
    pub(crate) fn keep_the_place_through_a_panels_zoom(
        &mut self,
        panel: dock::Panel,
        ratio: f32,
        above: Option<f32>,
    ) {
        let Some(above) = above else {
            return;
        };
        match panel {
            dock::Panel::Explorer => {
                // **Measured against the layout the pointer was in.** The rows start below a heading and a
                // filter box that scale with the zoom, so `above` — which was taken in the old layout — has
                // to have the **old** header taken off it, not the new one. Reading the new one moved the
                // row about half a row, which the `task-1771` review measured. The header scales by the same
                // ratio as everything else, so the old one is the new one divided by it.
                let header = self.explorer_scroll_offset_to_the_rows() / ratio.max(f32::EPSILON);
                let into_the_list = (above - header).max(0.0);
                let put = (self.explorer_scroll + into_the_list) * ratio - into_the_list;
                self.explorer_scroll_to = Some(put.max(0.0));
            }
            dock::Panel::Plugin(slot) => {
                let above = (above - crate::components::agent_tasks::PANE_HEADER).max(0.0);
                if let Some(plugin) = self.plugin_ui.plugin_of(slot as usize) {
                    if let Some(provider) = self.plugin_ui.provider(&plugin) {
                        provider.zoomed(ratio, above);
                    }
                }
            }
            _ => {}
        }
    }

    /// Which panel the zoom keys are about, or `None` when they are about the editing area.
    pub(crate) fn the_pane_the_keys_zoom(&self) -> Option<dock::Panel> {
        // **A picture is the one exception, and it is not a nicety.** `task-1658` asks that control and
        // plus zoom an image, and an image is opened by clicking it in the tree - which leaves the keyboard
        // in the **explorer**, because a single click there is a way of looking through a folder. Routing by
        // the keyboard alone would therefore mean the one gesture that ticket exists for could never be made
        // without clicking somewhere else first. A picture has no text in it to be typing at, so there is
        // nothing to be lost by saying the keys are always its own.
        //
        // It is the **zoom's** exception and nobody else's, which is why this is not the function
        // [`Self::the_pane_the_keys_hold`] is: maximising a pane while a picture happens to be open in a tab
        // is still about the pane holding the keyboard.
        if self.files.active().picture.is_some() {
            return None;
        }
        self.the_pane_the_keys_hold()
    }

    /// Put a panel back to the size it draws at until somebody changes it, which is what `Reset Font Size`
    /// means when the keys belong to a panel rather than to the editing area.
    pub(crate) fn reset_the_zoom_of(&mut self, panel: dock::Panel) {
        match panel {
            dock::Panel::Terminal | dock::Panel::Run | dock::Panel::Debug => {
                self.settings.terminal_font_size = settings::Settings::new().terminal_font_size;
            }
            dock::Panel::Space => {
                let body = self.space.body;
                self.aim_the_zoom_at(1.0, body.center());
            }
            dock::Panel::Explorer | dock::Panel::Plugin(_) => {
                let was = self.panes.zoom_of(panel);
                self.panes.set_zoom_of(panel, settings::DEFAULT_ZOOM);
                self.keep_the_place_through_a_panels_zoom(
                    panel,
                    settings::DEFAULT_ZOOM / was,
                    None,
                );
            }
        }
        self.unsaved_settings = true;
    }

    /// How far below the top of the explorer its scrolling rows start, at the zoom it is drawn at.
    ///
    /// The heading, the filter box and the gap under it, which is what `components::explorer` lays out
    /// before the list. Written here rather than exported from there because it is the one number the
    /// window needs and a second copy of the whole layout would be worse than one number with a name.
    fn explorer_scroll_offset_to_the_rows(&self) -> f32 {
        (36.0 + 24.0 + 12.0) * self.panes.zoom_of(dock::Panel::Explorer)
    }

    /// Remember the text `above` points below the top of the editing area, so the zoom can put it
    /// back there once the file has been laid out at its new size.
    ///
    /// Set before the size changes, and before [`Self::set_the_font_everywhere`] takes the top of
    /// the view for every other file, so this one — the pane being zoomed — keeps the point the
    /// person is actually looking at.
    fn anchor_the_view(&mut self, above: f32) {
        let file = self.files.active_mut();
        let at = file.cached.layout.anchor_at_y(file.scroll + above);
        file.zoom_anchor = Some(files::ViewAnchor { at, above });
    }

    /// The same, for a zoom from the keyboard, which has no pointer to be about.
    ///
    /// The caret is what a person is working on, so that is the point kept still — clamped into the
    /// view, so a caret that is off the top or the bottom of the window anchors the edge nearest it
    /// rather than scrolling the file to somewhere nobody asked to be.
    pub(crate) fn anchor_the_view_at_the_caret(&mut self) {
        let view_height = (self.editor_area.height() - size::EDITOR_PADDING_Y * 2.0).max(0.0);
        let file = self.files.active();
        let caret = file.cached.layout.caret_at(file.document.selection().head);
        let above = (caret.y - file.scroll).clamp(0.0, view_height);
        self.anchor_the_view(above);
    }

    /// Scroll a view back to the place it was anchored at before the font changed.
    ///
    /// Called on the frame the file is laid out again, after `refresh_layout` and before the scroll
    /// position is read for anything else, so the wheel and the caret still have the last word in
    /// the ordinary way. Clamped to what there is to scroll, because a larger font can leave a file
    /// that overflowed the window no longer overflowing it.
    pub(crate) fn keep_the_place_through_a_zoom(&mut self, view_height: f32) {
        let file = self.files.active_mut();
        let Some(anchor) = file.zoom_anchor.take() else {
            return;
        };
        let overflow = (file.cached.layout.height - view_height).max(0.0);
        file.scroll =
            (file.cached.layout.y_of_anchor(anchor.at) - anchor.above).clamp(0.0, overflow);
    }

    /// The same for the Markdown preview, which scrolls on its own and is laid out from the same
    /// base style, so a change of size moves it in exactly the same way.
    pub(crate) fn keep_the_previews_place_through_a_zoom(&mut self, view_height: f32) {
        let file = self.files.active_mut();
        let Some(anchor) = file.preview_anchor.take() else {
            return;
        };
        let overflow = (file.cached.preview_layout.height - view_height).max(0.0);
        file.preview_scroll =
            (file.cached.preview_layout.y_of_anchor(anchor.at) - anchor.above).clamp(0.0, overflow);
    }

    /// Set the editor's font size, from the keyboard or from a pinch.
    ///
    /// The one setting the Settings window holds, so a size reached with the keyboard is the size
    /// the dialog shows, reaches every open tab, and is written to the settings file and still there
    /// next time Unluminous starts — which is what a person means by zooming an editor rather than
    /// zooming a view of it.
    ///
    /// Public so a test can drive it without pressing keys.
    pub fn set_font_size(&mut self, size: f32) {
        let size = size.clamp(settings::MIN_FONT_SIZE, settings::MAX_FONT_SIZE);
        if (self.settings.font_size - size).abs() < 0.01 {
            return;
        }
        self.settings.font_size = size;
        self.set_the_font_everywhere();
        self.unsaved_settings = true;
    }

    /// Show every open file in the font the settings name.
    ///
    /// The editor's font is one setting for the whole window, the way the reference editor has one editor font,
    /// so a change reaches every tab rather than only the one that happens to be showing. It used to
    /// reach `document_mut()` alone, which meant that opening three files and then changing the font
    /// left two of them in the old one until Unluminous was restarted, and left the Markdown preview in
    /// it as well, because the preview is laid out from the source's own base style.
    ///
    /// This is not an edit: it pushes nothing onto any document's undo history and marks no file as
    /// having unsaved changes, because what Unluminous saves is plain text and carries no formatting.
    pub(crate) fn set_the_font_everywhere(&mut self) {
        let change = self.settings.as_style_change();
        // Before anything is changed, because an anchor describes the layout the reader can still
        // see. Every file rather than the one showing, for the same reason the font itself reaches
        // every file: the other tabs are laid out again too, and a tab that came back scrolled
        // somewhere else would be a tab that had moved while nobody was looking at it.
        for file in self.files.iter_mut() {
            file.anchor_the_views();
        }
        for file in self.files.iter_mut() {
            file.document.set_base_style(change.clone());
        }
        // Every file has to be laid out again, and every preview thrown away so it is built from
        // the new base style rather than the one it was made with. Every file rather than only the
        // one showing, which is what this used to do: with panes there is more than one on the
        // screen, and a cache now belongs to the tab it describes.
        for file in self.files.iter_mut() {
            file.cached.stale = true;
            file.cached.preview = None;
        }
        // The frame that puts each view back where it was anchored is the frame after this one, and
        // an idle window draws nothing — the last notch of a gesture would otherwise be left
        // showing the text at its new size in the old place until something else woke the window.
        if let Some(context) = &self.context {
            context.request_repaint();
        }
    }
}
