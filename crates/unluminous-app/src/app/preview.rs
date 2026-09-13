//! The Markdown preview, and the pictures and diagrams inside it.
//!
//! The preview is not a second renderer: `unluminous_core::markdown` turns the source into the same
//! three things a document holds, and the ordinary layout and painter draw it. What is here is the
//! two passes a picture needs — how tall it is drawn depends on how wide the pane is and on how
//! large the picture turns out to be, and `unluminous-core` can know neither — and the selection,
//! which is a byte range into the preview's own text.

use egui::{Color32, Pos2, Rect, Vec2};
use unluminous_core::{layout, Layout};

use crate::components::diagram_view;
use crate::components::editor_view;
use crate::components::picture_view;
use crate::components::scrollbar;
use crate::services::file_kind;
use crate::theme::{color, size};

use crate::app::{
    Focus, PlacedDiagram, PlacedPicture, PluginHighlighter, UnluminousApp, PANEL_PADDING,
    PICTURE_GAP, PROBLEM_HEIGHT,
};

impl UnluminousApp {
    /// The Markdown preview as it was last laid out, which the tests assert against.
    pub fn preview_layout(&self) -> &Layout {
        &self.files.active().cached.preview_layout
    }

    /// The preview's text, for a test that wants to check what the parser produced.
    pub fn preview_text(&self) -> String {
        self.files.active().cached.preview.as_ref().map(|p| p.text.to_string()).unwrap_or_default()
    }

    /// Which line of the source each line of the preview came from, for a test. See
    /// `unluminous_core::Preview::source_lines`, which is what the two halves of the side by side view
    /// are scrolled together through.
    pub fn preview_source_lines(&self) -> Vec<usize> {
        self.files
            .active()
            .cached
            .preview
            .as_ref()
            .map(|preview| preview.source_lines.clone())
            .unwrap_or_default()
    }

    /// The style covering one byte of the preview's text, for a test.
    pub fn preview_style_at(&self, at: usize) -> unluminous_core::CharStyle {
        self.files
            .active()
            .cached
            .preview
            .as_ref()
            .map(|preview| preview.chars.style_at(at).clone())
            .unwrap_or_default()
    }

    /// The panels the preview asked for, for a test.
    pub fn preview_panels(&self) -> Vec<unluminous_core::PreviewPanel> {
        self.files
            .active()
            .cached
            .preview
            .as_ref()
            .map(|preview| preview.panels.clone())
            .unwrap_or_default()
    }

    pub fn preview_links(&self) -> Vec<unluminous_core::PreviewLink> {
        self.files
            .active()
            .cached
            .preview
            .as_ref()
            .map(|preview| preview.links.clone())
            .unwrap_or_default()
    }

    pub fn preview_code_spans(&self) -> Vec<std::ops::Range<usize>> {
        self.files
            .active()
            .cached
            .preview
            .as_ref()
            .map(|preview| preview.code_spans.clone())
            .unwrap_or_default()
    }

    /// The pictures the preview is drawing, for a test.
    pub fn preview_pictures(&self) -> &[PlacedPicture] {
        &self.files.active().cached.preview_pictures
    }

    /// Work the preview out again if the source or the width changed.
    ///
    /// The preview is produced by `unluminous_core::markdown`, which turns the source into the same three
    /// things a document holds, so the ordinary layout engine and the ordinary painter draw it. Nothing
    /// here knows how to render Markdown.
    ///
    /// Pictures are the one thing that takes two passes. `markdown` says which paragraph stands in
    /// for a picture and what file it names, but how tall that paragraph has to be depends on how
    /// wide the pane is and on how large the picture turns out to be — neither of which that crate
    /// can know, because it has no window and cannot decode an image. So the pictures are read here,
    /// each one asks its paragraph to be at least as tall as it is drawn, and only then is the
    /// preview laid out.
    pub(crate) fn refresh_preview(&mut self, ctx: &egui::Context, width: f32) {
        // The text revision, for the reason `refresh_layout` records: the preview is built from the
        // source, and moving the caret does not change the source.
        let revision = self.document().text_revision();
        let cached = &self.files.active().cached;
        if cached.preview.is_some()
            && !cached.stale
            && revision == cached.preview_revision
            && (width - cached.preview_width).abs() < 0.5
        {
            return;
        }
        let base = unluminous_core::CharStyle {
            family: self.settings.font_family.clone(),
            size: self.document().active_style().size,
            color: unluminous_core::Color::rgb(
                color::text().r(),
                color::text().g(),
                color::text().b(),
            ),
            ..unluminous_core::CharStyle::default()
        };
        let colors = unluminous_core::PreviewColors {
            text: unluminous_core::Color::rgb(
                color::text_strong().r(),
                color::text_strong().g(),
                color::text_strong().b(),
            ),
            code: unluminous_core::Color::rgb(0x7E, 0xD3, 0x9B),
            link: unluminous_core::Color::rgb(
                color::accent().r(),
                color::accent().g(),
                color::accent().b(),
            ),
            quiet: unluminous_core::Color::rgb(
                color::text_dim().r(),
                color::text_dim().g(),
                color::text_dim().b(),
            ),
            rule: unluminous_core::Color::rgb(
                color::divider().r(),
                color::divider().g(),
                color::divider().b(),
            ),
        };
        let mono = self.renderer.monospaced_family();
        // How many characters of the code font fit across the pane, which is the one measurement a
        // table takes. Everything else about a table is integer arithmetic over characters, which is
        // what `markdown::table` is for and why it is testable with no fonts.
        let mut preview = {
            let highlighter = PluginHighlighter { plugins: &self.plugins };
            let code = unluminous_core::CharStyle {
                family: mono.clone().unwrap_or_else(|| base.family.clone()),
                size: base.size * 0.95,
                ..unluminous_core::CharStyle::default()
            };
            let advance =
                unluminous_core::FontMetrics::advance(&self.renderer, "M", &code).max(1.0);
            let options = unluminous_core::PreviewOptions {
                base: base.clone(),
                colors,
                mono,
                columns: (width / advance).floor().max(16.0) as usize,
                highlighter: Some(&highlighter),
            };
            unluminous_core::markdown::render(&self.document().text().to_string(), &options)
        };
        let pictures = self.read_the_pictures(ctx, &mut preview, width);
        let diagrams = self.lay_the_diagrams_out(ctx, &mut preview, width);
        let laid =
            layout(&preview.text, &preview.chars, &preview.paragraphs, &self.renderer, width);
        // A byte range into text that has been rebuilt means nothing, so a selection in the preview
        // does not survive an edit. It does survive a scroll and a resize, which is where a person
        // actually loses one — and the clamp is what makes a resize safe, since a table laid out at
        // a new width is a different number of bytes.
        let rebuilt = revision != self.files.active().cached.preview_revision;
        let length = preview.text.len_bytes();
        let file = self.files.active_mut();
        if rebuilt {
            file.preview_selection = unluminous_core::Selection::caret(0);
        } else {
            file.preview_selection.anchor = file.preview_selection.anchor.min(length);
            file.preview_selection.head = file.preview_selection.head.min(length);
        }
        let cached = &mut self.files.active_mut().cached;
        cached.preview_pictures = pictures;
        cached.preview_diagrams = diagrams;
        cached.preview_layout = laid;
        cached.preview = Some(preview);
        cached.preview_revision = revision;
        cached.preview_width = width;
    }

    /// Read every picture the preview names, and give each one's paragraph the room it needs.
    ///
    /// A picture is drawn at its own size, or scaled down to the width of the pane when it is wider
    /// than that — never blown up, which is what `services::picture` decided for a picture in a tab
    /// and is what "fit" means to anybody. A picture that will not decode leaves its paragraph the
    /// height of a line of text and its alt text is drawn there instead, which is what the preview
    /// did before there were pictures at all.
    fn read_the_pictures(
        &mut self,
        ctx: &egui::Context,
        preview: &mut unluminous_core::Preview,
        width: f32,
    ) -> Vec<PlacedPicture> {
        let folder =
            self.document().path().and_then(|path| path.parent()).map(std::path::Path::to_path_buf);
        let mut placed = Vec::new();
        for image in preview.images.clone() {
            let ready = self.preview_images.ready(ctx, folder.as_deref(), &image.source);
            let size = match &ready {
                Some(ready) => {
                    let (pixels_across, pixels_down) = (ready.size[0] as f32, ready.size[1] as f32);
                    let scale =
                        if pixels_across > 0.0 { (width / pixels_across).min(1.0) } else { 1.0 };
                    Vec2::new(pixels_across * scale, pixels_down * scale)
                }
                None => Vec2::ZERO,
            };
            if size.y > 0.0 {
                let room = size.y + PICTURE_GAP;
                preview
                    .paragraphs
                    .set(image.paragraph..image.paragraph + 1, |style| style.min_height = room);
            }
            placed.push(PlacedPicture {
                paragraph: image.paragraph,
                size,
                texture: ready.map(|ready| ready.texture),
                alt: image.alt.clone(),
            });
        }
        placed
    }

    /// Lay every diagram the preview names out, and give each one's paragraph the room it needs.
    ///
    /// Exactly the two passes the pictures take, and for the same reason: `unluminous_core::markdown`
    /// cannot know how wide the pane is, so it says where a diagram goes and this works out how tall
    /// it turns out to be. A diagram wider than the pane is scaled down to fit — never blown up —
    /// which is what `fit` means everywhere else in Unluminous.
    ///
    /// **A diagram that will not draw keeps its room and says why.** Losing the whole document
    /// because one fence has a typo in it would be far worse than a panel where a picture should be,
    /// and the panel names the line so the typo can be found.
    fn lay_the_diagrams_out(
        &mut self,
        ctx: &egui::Context,
        preview: &mut unluminous_core::Preview,
        width: f32,
    ) -> Vec<PlacedDiagram> {
        // The plugin decides whether a diagram is drawn at all. With it switched off, a mermaid
        // fence stays the code it was before `task-1660`, in the same frame.
        if preview.diagrams.is_empty() || !self.mermaid_is_enabled() {
            return Vec::new();
        }
        let base = self.diagram_style();
        let theme = crate::services::mermaid_scene::theme();
        let metrics =
            crate::services::mermaid_scene::EguiMetrics::new(ctx, self.bold_family.clone());
        let mut placed = Vec::with_capacity(preview.diagrams.len());
        for diagram in preview.diagrams.clone() {
            let laid = self.mermaid_scenes.scene(&diagram.source, &base, &metrics, &theme);
            let size = match &laid {
                Ok(scene) if scene.size.width > 0.0 => {
                    let scale = (width / scene.size.width).min(1.0);
                    Vec2::new(scene.size.width * scale, scene.size.height * scale)
                }
                // A problem panel takes a fixed height: enough for the reason and a few lines of the
                // source under it.
                _ => Vec2::new(width, PROBLEM_HEIGHT),
            };
            if size.y > 0.0 {
                let room = size.y + PICTURE_GAP;
                preview
                    .paragraphs
                    .set(diagram.paragraph..diagram.paragraph + 1, |style| style.min_height = room);
            }
            placed.push(PlacedDiagram {
                paragraph: diagram.paragraph,
                size,
                laid,
                source: diagram.source.clone(),
            });
        }
        placed
    }

    /// The family and the size a diagram's text is set in.
    ///
    /// The **size** follows the editor's, so a diagram grows and shrinks with command and plus
    /// exactly as the Markdown preview does.
    ///
    /// The **family** is the one `theme::install_fonts` put into egui, which is not necessarily the
    /// editor's. A diagram is the one thing in the window that is measured by `unluminous-core` and drawn
    /// by `egui`, and those two have to be looking at the same face or a box comes out the wrong size
    /// for the words in it. Measuring in the settings font while drawing in egui's left a requirement
    /// diagram's fields hanging over the right edge of their boxes, which is what the screenshot
    /// showed and what no assertion about the scene could have caught.
    pub(crate) fn diagram_style(&self) -> unluminous_core::CharStyle {
        unluminous_core::CharStyle {
            family: self.renderer.default_family(),
            size: self.document().active_style().size * 0.9,
            ..unluminous_core::CharStyle::default()
        }
    }

    /// How many diagrams have been laid out and kept, for a test.
    pub fn mermaid_scene_count(&self) -> usize {
        self.mermaid_scenes.len()
    }

    /// Whether the Mermaid plugin is switched on.
    ///
    /// Asked before a diagram is laid out and before a `.mmd` file is drawn as one, so switching the
    /// plugin off in `Plugins` withdraws every diagram in the window in the same frame. That is what
    /// makes it a plugin rather than a feature with a plugin painted on it.
    pub fn mermaid_is_enabled(&self) -> bool {
        self.plugins.renders("mermaid")
    }

    /// The diagrams the preview is drawing, for a test.
    pub fn preview_diagrams(&self) -> &[PlacedDiagram] {
        &self.files.active().cached.preview_diagrams
    }

    /// How far each half of the side by side view is scrolled, taken before either is drawn.
    pub(crate) fn where_both_halves_are(&self) -> (f32, f32) {
        let file = self.files.active();
        (file.scroll, file.preview_scroll)
    }

    /// Scroll the half that was not moved to show what the half that was moved is showing.
    ///
    /// `task-1673` asks that the source and the preview scroll together. The two pages are nothing
    /// like the same height — a heading is one line of source and three times a line on the page,
    /// and a fence's backticks are two lines of source and nothing at all — so the crossing is done
    /// through the text rather than through a proportion of the height. `unluminous_core::scroll_sync`
    /// is the arithmetic, and `Preview::source_lines` is what makes it possible.
    ///
    /// **Which half drives is decided by which one moved**, compared against where they both were
    /// before the frame drew anything. That is what stops the two of them chasing each other: the
    /// crossing snaps to a paragraph, so a position taken across and back is not quite the position
    /// it started at, and a rule that moved both halves every frame would creep down the file on its
    /// own. Only one half is written to, and only when the other actually moved.
    ///
    /// The follower is settled after both halves are drawn, so it lands on the next frame. egui
    /// paints continuously while a wheel is turning or a thumb is being dragged, so that frame is
    /// sixteen milliseconds later and nobody can see it.
    pub(crate) fn scroll_the_two_halves_together(&mut self, before: (f32, f32), area: Rect) {
        let (was_source, was_preview) = before;
        let file = self.files.active();
        let (source_moved, preview_moved) = (
            (file.scroll - was_source).abs() > 0.01,
            (file.preview_scroll - was_preview).abs() > 0.01,
        );
        if source_moved == preview_moved {
            // Neither moved, or a change of font size moved both. Nothing to follow either way.
            return;
        }
        self.follow_the_other_half(
            source_moved,
            (area.height() - size::EDITOR_PADDING_Y * 2.0).max(0.0),
        );
    }

    /// Move the half of the side by side view that was not scrolled so that it shows what the other
    /// half is showing. `source_drives` says which way round; `room` is how tall each half is, which
    /// is the same for both because they stand side by side.
    ///
    /// Split from [`Self::scroll_the_two_halves_together`] so the command line can ask for it: a
    /// scroll set by `unluminous-cli editor scroll` is applied before the frame draws anything, so the
    /// frame's own before-and-after comparison would see nothing move and the other half would sit
    /// where it was.
    pub fn follow_the_other_half(&mut self, source_drives: bool, room: f32) {
        let file = self.files.active();
        let Some(preview) = file.cached.preview.as_ref() else {
            return;
        };
        let source_page = &file.cached.layout;
        let preview_page = &file.cached.preview_layout;
        let map = &preview.source_lines;
        let (scroll, preview_scroll) = if source_drives {
            let to = unluminous_core::preview_y_for_source_y(
                source_page,
                preview_page,
                map,
                file.scroll,
            );
            (file.scroll, to.clamp(0.0, (preview_page.height - room).max(0.0)))
        } else {
            let to = unluminous_core::source_y_for_preview_y(
                source_page,
                preview_page,
                map,
                file.preview_scroll,
            );
            (to.clamp(0.0, (source_page.height - room).max(0.0)), file.preview_scroll)
        };
        let file = self.files.active_mut();
        file.scroll = scroll;
        file.preview_scroll = preview_scroll;
    }

    pub(crate) fn show_picture(&mut self, ui: &mut egui::Ui, area: Rect) -> bool {
        let name = self.files.active().name();
        let Some(picture) = self.files.active_mut().picture.as_mut() else {
            return false;
        };
        let outcome = picture_view::show(ui, area, picture, &name);
        if outcome.take_focus {
            self.focus = Focus::Editor;
        }
        outcome.take_focus
    }

    /// Draw whatever the open file's preview is: a Markdown page, or a drawn diagram.
    ///
    /// One function rather than branches spread through `show_editing_area`, so that the three view
    /// modes are the same three modes whichever kind of file is open and `SideBySide` needs no
    /// special case of its own at all.
    pub(crate) fn show_preview(&mut self, ui: &mut egui::Ui, area: Rect) {
        if file_kind::is_mermaid(self.document().path()) {
            self.show_diagram(ui, area);
            return;
        }
        self.show_markdown_preview(ui, area);
    }

    /// Draw the whole file as one diagram, which is what a `.mmd` file's preview is.
    fn show_diagram(&mut self, ui: &mut egui::Ui, area: Rect) {
        if !self.mermaid_is_enabled() {
            let problem = unluminous_core::mermaid::Problem::whole(
                "The Mermaid plugin is switched off, so this file is not drawn as a diagram. Switch it on in Plugins.",
            );
            diagram_view::show_problem(ui, area, &problem, "");
            return;
        }
        let source = self.document().text().to_string();
        let base = self.diagram_style();
        let theme = crate::services::mermaid_scene::theme();
        let metrics =
            crate::services::mermaid_scene::EguiMetrics::new(ui.ctx(), self.bold_family.clone());
        let laid = self.mermaid_scenes.scene(&source, &base, &metrics, &theme);
        let name = self.files.active().name();
        match laid {
            Ok(scene) => {
                // Taken apart by field, because the view has to be borrowed mutably while the window
                // is still needed for the focus that follows.
                let Self { files, .. } = self;
                let view = &mut files.active_mut().diagram;
                let outcome = diagram_view::show(ui, area, &scene, view, &name);
                if outcome.take_focus {
                    self.focus = Focus::Editor;
                }
            }
            Err(problem) => diagram_view::show_problem(ui, area, &problem, &source),
        }
    }

    /// Draw the Markdown preview into `area`. It is read only, so it has no caret and no selection: there
    /// is nothing to type into, because what is shown is worked out from the source.
    fn show_markdown_preview(&mut self, ui: &mut egui::Ui, area: Rect) {
        // `click_and_drag` rather than `hover`: the preview is read only, but reading includes
        // taking a copy of what you are reading. See `UnluminousApp::reading_preview`.
        let response = ui.interact(area, ui.id().with("preview"), egui::Sense::click_and_drag());
        let text_width = (area.width() - size::EDITOR_PADDING_X * 2.0).max(50.0);
        let ctx = ui.ctx().clone();
        self.refresh_preview(&ctx, text_width);
        let view_height = area.height() - size::EDITOR_PADDING_Y * 2.0;
        self.keep_the_previews_place_through_a_zoom(view_height);

        let was = self.files.active().preview_scroll;
        // The bar down the right, taken hold of before the wheel is read so that it wins the pointer
        // over the page underneath it. See `components::scrollbar`.
        let bar_name = format!("{} preview", self.files.active().name());
        let bar = scrollbar::Bar::new(area, was, self.preview_layout().height, view_height);
        let grab = match &bar {
            Some(bar) => scrollbar::grab(ui, bar, &bar_name),
            None => scrollbar::Grab::default(),
        };
        if let Some(to) = grab.scroll {
            self.files.active_mut().preview_scroll = to;
        }

        // The preview scrolls on its own, so reading the rendered page does not move the caret.
        let wheel = ui.input(|input| input.smooth_scroll_delta.y);
        // `contains_pointer` rather than `hovered`, because the scrollbar is a widget over this one
        // and takes the hover from it: a wheel turned with the pointer resting on the bar is still
        // plainly about the page the bar belongs to.
        if wheel != 0.0 && response.contains_pointer() {
            self.files.active_mut().preview_scroll -= wheel;
        }
        let overflow = (self.preview_layout().height - view_height).max(0.0);
        let scroll = self.files.active().preview_scroll.clamp(0.0, overflow);
        self.files.active_mut().preview_scroll = scroll;

        let origin = Pos2::new(
            area.left() + size::EDITOR_PADDING_X,
            area.top() + size::EDITOR_PADDING_Y - scroll,
        );
        let mut painter_ui = ui.new_child(egui::UiBuilder::new().max_rect(area));
        painter_ui.set_clip_rect(ui.painter().clip_rect().intersect(area));
        // The pointer is read before anything is drawn, so the selection painted below is the one
        // this frame's drag made rather than the one it started with.
        self.select_in_the_preview(&response, origin);
        if response.hovered() {
            painter_ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        // **After the text cursor, so the hand wins where a link is.** egui keeps the last cursor asked
        // for in a frame, and a link is a few words inside a page that is otherwise selectable text; the
        // other way round the hand was set and then immediately replaced, which is a link that cannot be
        // seen and can still be clicked. It reads the pointer rather than being a widget of its own,
        // because a widget over the words would take the drag that selects them.
        self.open_a_link_in_the_preview(&painter_ui, &response, origin);
        self.paint_the_panels(&painter_ui, origin, text_width);
        self.paint_the_code_chips(&painter_ui, origin);
        editor_view::paint_behind(
            &painter_ui,
            self.preview_layout(),
            origin,
            self.files.active().preview_selection.range(),
            color::text_selection(),
            2.0,
        );
        let preview = self.files.active().cached.preview.as_ref().expect("preview was refreshed");
        editor_view::paint_text(
            &painter_ui,
            &self.renderer,
            &preview.text,
            self.preview_layout(),
            origin,
        );
        self.paint_the_pictures(&painter_ui, origin);
        self.paint_the_diagrams(&painter_ui, origin, text_width);
        // Drawn last, at the position the frame settled on rather than the one it opened with.
        if let Some(bar) =
            scrollbar::Bar::new(area, scroll, self.preview_layout().height, view_height)
        {
            scrollbar::paint(ui, &bar, &bar_name, grab.active || (scroll - was).abs() > 0.01);
        }
    }

    /// Take `Ctrl/Cmd+C` for the preview when the preview is what is being read.
    ///
    /// egui delivers a copy as an `Event::Copy` rather than as a key press — which is why `Copy` is
    /// marked in `actions::menus` as not coming from the keyboard — so the event is what has to be
    /// claimed. Removing it from the frame's input is what stops the source pane copying its own
    /// selection a moment later.
    pub(crate) fn route_the_preview_copy(&mut self, ui: &egui::Ui) {
        if !self.view_mode().shows_preview() || !self.preview_holds_the_selection() {
            return;
        }
        let took = ui.input_mut(|input| {
            let before = input.events.len();
            input.events.retain(|event| !matches!(event, egui::Event::Copy));
            before != input.events.len()
        });
        if took {
            if let Some(text) = self.preview_selected_text() {
                ui.ctx().copy_text(text);
            }
        }
    }

    /// Read the pointer in the preview and keep what it selected on the tab.
    ///
    /// The component works out the selection and changes nothing, which is the rule every component
    /// in Unluminous follows; the choice made here is that a press claims the copy for the preview
    /// without taking the keyboard from the source beside it.
    fn select_in_the_preview(&mut self, response: &egui::Response, origin: Pos2) {
        let was = self.files.active().preview_selection;
        let Some(preview) = self.files.active().cached.preview.as_ref() else { return };
        let text = preview.text.clone();
        let selection =
            editor_view::read_pointer(response, self.preview_layout(), &text, origin, was);
        if let Some(selection) = selection {
            self.files.active_mut().preview_selection = selection;
            self.reading_preview = true;
        }
    }

    /// Where the link under the pointer goes, and `None` when the pointer is not on one.
    ///
    /// `unluminous_core::markdown::Preview::links` is a sorted list of byte ranges, so this is a binary
    /// search: `partition_point` finds the first range that could hold the offset and one comparison
    /// settles it. The pointer is read while it moves, so it costs no more than the hover in a source
    /// file does.
    fn link_under_the_pointer(&self, response: &egui::Response, origin: Pos2) -> Option<String> {
        let at = response.hover_pos()?;
        let preview = self.files.active().cached.preview.as_ref()?;
        if preview.links.is_empty() {
            return None;
        }
        let local = at - origin;
        let offset = self.preview_layout().offset_at(local.x, local.y);
        let first = preview.links.partition_point(|link| link.bytes.end <= offset);
        let link = preview.links.get(first)?;
        (link.bytes.contains(&offset)).then(|| link.target.clone())
    }

    /// Open the link under the pointer, if the modifier is held and there is one there.
    ///
    /// `task-1848`: "Links in markdown should allow me to CMD/Ctrl+Click to open them in a new browser
    /// window." Three decisions in it are rules rather than choices:
    ///
    /// **The modifier is Go to Definition's**, which `task-1696` set: `Ctrl/Cmd` held means "take me to
    /// the thing this names". A plain click keeps the meaning it has in a preview, which is placing a
    /// selection — so nothing a person could already do changes.
    ///
    /// **The pointer says so before the click**, with the hand cursor an underlined address already
    /// implies. Without it, a link is indistinguishable from underlined text and the feature is one
    /// nobody finds.
    ///
    /// **Only `http` and `https` open.** A `file:`, a `javascript:` or a `mailto:` is refused by name in
    /// the status bar. This is the rule `services::preview_images` already keeps about a picture with a
    /// scheme in it: a document must not be able to reach this machine because somebody clicked a word
    /// in it, and a `javascript:` address handed to a web view would run whatever the document said.
    fn open_a_link_in_the_preview(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        origin: Pos2,
    ) {
        let held = ui.input(|input| input.modifiers.command);
        if !held {
            return;
        }
        let Some(target) = self.link_under_the_pointer(response, origin) else { return };
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if response.clicked() {
            // **Noted rather than done here.** Opening a tab changes which file is active, and this runs
            // half way through drawing the preview — the rest of which reads `active().cached.preview`
            // and found it missing, which is a panic rather than a wrong-looking frame. It is the same
            // reason a fold is acted on at the end of the frame that asked for one.
            self.link_to_open = Some(target);
        }
    }

    /// Open the link a `Ctrl/Cmd+Click` in the preview asked for, at the end of the frame.
    ///
    /// **Only `http` and `https` open.** A `file:`, a `javascript:` or a `mailto:` is refused by name in
    /// the status bar. This is the rule `services::preview_images` already keeps about a picture with a
    /// scheme in it: a document must not be able to reach this machine because somebody clicked a word
    /// in it, and a `javascript:` address handed to a web view would run whatever the document said.
    pub(crate) fn settle_the_link_click(&mut self) {
        let Some(target) = self.link_to_open.take() else { return };
        match target.split_once(':') {
            Some(("http" | "https", _)) => match self.open_browser(&target) {
                Ok(_) => {}
                Err(said) => self.message = Some(said),
            },
            // A bare address the document wrote without a scheme — `example.com/a` in angle brackets, or
            // a relative path. `https` is what a browser assumes, and it is the safe half of the two.
            None => match self.open_browser(&format!("https://{target}")) {
                Ok(_) => {}
                Err(said) => self.message = Some(said),
            },
            Some((scheme, _)) => {
                self.message = Some(format!(
                    "{scheme}: links are not opened from a preview. Only http and https are."
                ));
            }
        }
    }

    /// The text the preview has selected, which is what `Copy` means while the preview is being read.
    pub fn preview_selected_text(&self) -> Option<String> {
        let file = self.files.active();
        let range = file.preview_selection.range();
        if range.is_empty() {
            return None;
        }
        let preview = file.cached.preview.as_ref()?;
        Some(preview.text.byte_slice(range))
    }

    /// True when a copy would take what the preview has selected rather than what the document has.
    pub fn preview_holds_the_selection(&self) -> bool {
        self.reading_preview && !self.files.active().preview_selection.is_empty()
    }

    /// Whether a copy would be about the preview rather than about the source beside it.
    ///
    /// This is the flag a press in a preview sets and a press in an editing area clears, and it is a
    /// different question from `preview_holds_the_selection`: a plain click places a caret and
    /// selects nothing, so the preview can own the click while holding an empty selection.
    pub fn is_reading_the_preview(&self) -> bool {
        self.reading_preview
    }

    /// Select the whole of the preview, which is what `Select All` means while it is being read.
    pub fn select_the_whole_preview(&mut self) {
        let length = self
            .files
            .active()
            .cached
            .preview
            .as_ref()
            .map(|preview| preview.text.len_bytes())
            .unwrap_or(0);
        self.files.active_mut().preview_selection = unluminous_core::Selection::new(0, length);
        self.reading_preview = true;
    }

    /// Draw a panel behind the code blocks, the tables and the front matter.
    ///
    /// `unluminous-core` says which paragraphs want a ground under them and this decides what one looks
    /// like, which is the same seam the pictures and the diagrams already sit on. The panel is drawn
    /// the whole width of the text rather than the width of the words, because a block of code is a
    /// block: a ragged right edge would read as a quotation.
    fn paint_the_panels(&self, ui: &egui::Ui, origin: Pos2, width: f32) {
        let Some(preview) = self.files.active().cached.preview.as_ref() else { return };
        if preview.panels.is_empty() {
            return;
        }
        let layout = self.preview_layout();
        let clip = ui.painter().clip_rect();
        for panel in &preview.panels {
            let Some((top, _)) = layout.paragraph_band(panel.paragraphs.start) else { continue };
            let Some((last, height)) = layout.paragraph_band(panel.paragraphs.end - 1) else {
                continue;
            };
            let rect = Rect::from_min_max(
                Pos2::new(origin.x - PANEL_PADDING, origin.y + top - PANEL_PADDING),
                Pos2::new(origin.x + width, origin.y + last + height + PANEL_PADDING),
            );
            if rect.bottom() < clip.top() || rect.top() > clip.bottom() {
                continue;
            }
            ui.painter().rect_filled(rect, 4.0, color::code_panel());
        }
    }

    /// Draw a chip behind each piece of inline code, which is what makes it read as a thing rather
    /// than as green prose.
    ///
    /// Only the pieces on the screen are asked for. The ranges are in order, so finding them is a
    /// pair of binary searches — the rule `task-1666` set for anything that runs once a frame.
    fn paint_the_code_chips(&self, ui: &egui::Ui, origin: Pos2) {
        let Some(preview) = self.files.active().cached.preview.as_ref() else { return };
        if preview.code_spans.is_empty() {
            return;
        }
        let layout = self.preview_layout();
        let clip = ui.painter().clip_rect();
        let bytes = layout.visible_bytes(clip.top() - origin.y, clip.bottom() - origin.y);
        let first = preview.code_spans.partition_point(|span| span.end <= bytes.start);
        for span in &preview.code_spans[first..] {
            if span.start >= bytes.end {
                break;
            }
            editor_view::paint_behind(ui, layout, origin, span.clone(), color::code_chip(), 3.0);
        }
    }

    /// Draw the pictures into the room their paragraphs were given.
    ///
    /// Drawn after the text rather than before it, so a picture cannot be hidden behind the letters
    /// of the empty line it sits on, and at the left of the text where every other block starts.
    fn paint_the_pictures(&self, ui: &egui::Ui, origin: Pos2) {
        let painter = ui.painter();
        for picture in self.preview_pictures() {
            let Some(line) =
                self.preview_layout().lines.iter().find(|line| line.paragraph == picture.paragraph)
            else {
                continue;
            };
            let at = Pos2::new(origin.x, origin.y + line.y);
            match &picture.texture {
                Some(texture) => {
                    let rect = Rect::from_min_size(at, picture.size);
                    painter.image(
                        texture.id(),
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                None => {
                    // Nothing to draw, so say what should have been there. The same words the
                    // preview showed for a picture before it could draw one.
                    let words = if picture.alt.trim().is_empty() {
                        "a picture that could not be read".to_owned()
                    } else {
                        picture.alt.clone()
                    };
                    let galley = painter.layout_no_wrap(
                        words,
                        egui::FontId::proportional(13.0),
                        color::text_dim(),
                    );
                    painter.galley(at, galley, color::text_dim());
                }
            }
        }
    }

    /// Draw the diagrams into the room their paragraphs were given.
    ///
    /// After the text, like the pictures, so a diagram cannot be hidden behind the letters of the
    /// empty line it sits on.
    fn paint_the_diagrams(&self, ui: &egui::Ui, origin: Pos2, width: f32) {
        for diagram in self.preview_diagrams() {
            let Some(line) =
                self.preview_layout().lines.iter().find(|line| line.paragraph == diagram.paragraph)
            else {
                continue;
            };
            let at = Pos2::new(origin.x, origin.y + line.y);
            match &diagram.laid {
                Ok(scene) => {
                    let scale = if scene.size.width > 0.0 {
                        (width / scene.size.width).min(1.0)
                    } else {
                        1.0
                    };
                    diagram_view::paint(ui, scene, at, scale);
                }
                Err(problem) => {
                    let panel = Rect::from_min_size(at, Vec2::new(width, diagram.size.y));
                    diagram_view::show_problem(ui, panel, problem, &diagram.source);
                }
            }
        }
    }
}
