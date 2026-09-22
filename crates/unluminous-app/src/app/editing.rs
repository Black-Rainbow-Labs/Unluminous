//! The editing area: the row of panes, what is drawn in one, and what is worked out about the file
//! showing in it.
//!
//! The pane loop **borrows the focus**, which is what lets everything here go on asking
//! `files.active()` and get the pane being drawn. The colouring, the layout and the marked passages
//! are here too, because all three are about the text a pane is showing rather than about the window.

use std::path::Path;

use egui::{Pos2, Rect, Vec2};
use unluminous_core::{relayout_touching, Command, Highlights, Rgba, Touched};

use crate::components::editor_view;
use crate::components::file_tabs::{self, TabView};
use crate::components::gutter::{self, Gutter};
use crate::components::scrollbar;
use crate::components::splitter;
use crate::components::text_menu;
use crate::theme::{self, color, size};

use crate::app::{
    elide_value, is_unreadable, same_file, Drag, Focus, InlineValues, UnluminousApp, ViewMode,
    ZoomClaim,
};

/// What an editing area's components are handed, read before any of them is drawn.
///
/// `task-1984` §3.6. One value rather than five locals, so `UnluminousApp::what_the_editor_draws_from`
/// can be a function rather than the first fifty lines of `UnluminousApp::show_editor`.
struct EditorReadings {
    /// Which paragraphs could fold, and which of them are folded.
    fold_marks: Vec<(usize, bool)>,
    /// What the gutter draws for each breakpoint, which is what the adapter said about it.
    breakpoint_marks: Vec<(usize, gutter::BreakpointMark)>,
    /// The paragraph the program is stopped on.
    execution_point: Option<usize>,
    /// The values to paint at the ends of the lines that bind them.
    inline_values: Vec<(usize, String)>,
    /// Every match of the Find bar but the current one, which is drawn as the selection.
    find_matches: Vec<std::ops::Range<usize>>,
}

impl UnluminousApp {
    /// Mark the selected passage in the file that is showing.
    ///
    /// The one place a passage is marked by hand: the four blocks, the four menu entries and the
    /// colour wheel all come here, so a colour chosen one way and the same colour chosen another
    /// are the same change. Nothing happens when there is no selection, and the status bar says so
    /// rather than leaving a click looking as though it did nothing.
    pub fn highlight_selection(&mut self, color: Rgba) -> bool {
        let range = self.document().selection().range();
        if range.is_empty() {
            self.message = Some("Select some text to highlight it.".to_owned());
            return false;
        }
        self.last_highlight = color;
        let marked = self.document_mut().highlight(range, color);
        if marked {
            self.message = None;
        }
        marked
    }

    /// True when there is a mark the selection touches, or one under the caret when nothing is
    /// selected. What decides whether `Clear Highlight` can be used, and what it will act on.
    pub fn marks_under_the_caret(&self) -> bool {
        let selection = self.document().selection();
        if selection.is_empty() {
            self.document().highlights().at(selection.head).is_some()
        } else {
            !self.document().highlights().overlapping(selection.range()).is_empty()
        }
    }

    /// Take away the marks the selection touches, or the one under the caret when nothing is
    /// selected.
    ///
    /// One rule rather than two, so that `Clear Highlight` means the same thing on the Edit menu, on
    /// the right click menu and from the command line. A right click outside a selection puts the
    /// caret where it was clicked before the menu opens, which is what makes "the one under the
    /// caret" the one under the pointer.
    pub fn clear_highlight_here(&mut self) -> bool {
        let selection = self.document().selection();
        if selection.is_empty() {
            self.document_mut().clear_highlight_at(selection.head)
        } else {
            self.document_mut().clear_highlight(selection.range())
        }
    }

    /// Take away every mark in the file that is showing.
    pub fn clear_highlights_here(&mut self) -> bool {
        self.document_mut().clear_highlights()
    }

    /// Change what is marked in any file of this project, whether it is open or not.
    ///
    /// The one place that choice is made, so no caller has to think about it: a file that is open is
    /// owned by its document, and every other file is owned by `services::file_marks`. Anything
    /// changed in a document is pushed into the store by [`Self::remember_the_marks`] on the same
    /// frame, so the two cannot come to disagree.
    pub fn change_highlights(&mut self, path: &Path, change: impl FnOnce(&mut Highlights)) -> bool {
        if let Some(index) = self.files.index_of(path) {
            let mut marks = self.files.at(index).document.highlights().clone();
            let before = marks.clone();
            change(&mut marks);
            if before == marks {
                return false;
            }
            self.files.at_mut(index).document.set_highlights(marks);
            return true;
        }
        self.marks.change(path, change)
    }

    /// What is marked in one file, whether it is open or not.
    pub fn highlights_of(&self, path: &Path) -> Highlights {
        if let Some(index) = self.files.index_of(path) {
            return self.files.at(index).document.highlights().clone();
        }
        self.marks.highlights(path).cloned().unwrap_or_default()
    }

    /// Push what every open document holds into the store, and write the store if it changed.
    ///
    /// Called every frame and does almost nothing: an integer comparison for each open tab, because
    /// a document that has not changed since it was last pushed cannot have new marks in it. Writing
    /// is on the same terms as the project state — only when something changed, and only once the
    /// pointer is up, so dragging a selection never writes.
    pub(crate) fn remember_the_marks(&mut self, settled: bool) {
        for index in 0..self.files.len() {
            let Some(path) = self.files.at(index).path().map(Path::to_path_buf) else {
                continue;
            };
            let revision = self.files.at(index).document.revision();
            if self.files.at(index).marked_revision == Some(revision) {
                continue;
            }
            let marks = self.files.at(index).document.highlights().clone();
            self.marks.set(&path, marks);
            self.files.at_mut(index).marked_revision = Some(revision);
        }
        if self.remembers_this_project() && settled {
            let root = self.tree.root().to_path_buf();
            self.marks.save(&root);
        }
    }

    /// The largest file that is coloured.
    ///
    /// Colouring is one linear pass over the text and it runs whenever the text changes, so on a
    /// very large file it is a pause a person can feel while typing. Two megabytes is where it is
    /// switched off, and the status bar says so rather than leaving the colours quietly missing.
    /// It is a number to be measured rather than a law: change it, and change this comment.
    pub(crate) const COLOUR_LIMIT: usize = 2 * 1024 * 1024;

    /// Colour the open file by what its text is, if a plugin claims it.
    ///
    /// This is not an edit. `Document::set_syntax` pushes nothing onto the undo history and does not
    /// mark the file as changed, for the same reasons setting the font does not: what Unluminous saves is
    /// plain text and carries no formatting.
    pub(crate) fn colour_the_open_file(&mut self) {
        for pane in 0..self.files.pane_count() {
            if let Some(index) = self.files.showing_in(pane) {
                self.colour_the_file(index);
            }
        }
    }

    /// Colour one file. One in each pane is asked about every frame, because every pane is drawing.
    fn colour_the_file(&mut self, index: usize) {
        // The text revision. Keyed on the revision, this re-tokenised the whole file and rebuilt
        // every style span on every frame in which the caret moved, which is every frame of dragging
        // a selection. See `tasks/task-1666-performance-tdd.md` section 2.
        let revision = self.files.at(index).document.text_revision();
        if self.files.at(index).coloured_revision == Some(revision) {
            return;
        }
        let Some(path) = self.files.at(index).path().map(Path::to_path_buf) else {
            self.files.at_mut(index).coloured_revision = Some(revision);
            return;
        };
        let Some(plugin) = self.plugins.for_path(&path) else {
            self.files.at_mut(index).coloured_revision = Some(revision);
            return;
        };
        let base =
            unluminous_core::Color::rgb(color::text().r(), color::text().g(), color::text().b());
        let text = self.files.at(index).document.text().to_string();
        if text.len() > Self::COLOUR_LIMIT {
            self.message = Some(format!(
                "{} is too large to colour, so it is shown as plain text.",
                path.display()
            ));
            self.files.at_mut(index).coloured_revision = Some(revision);
            return;
        }
        let theme = crate::services::plugins::scheme_of(plugin);
        // **One** reading of the file, answering two questions. Colouring it and reading it for the
        // blocks that could be collapsed both want the same tokens over the same text at the same
        // revision, and a second pass was worth 2.5 ms a keystroke on the largest file in this
        // repository. `unluminous_core::folding::Tokens` is the second answer, kept beside the first.
        //
        // **And only the part that changed is read.** `task-1804` §5.2: at 2 MB, reading the
        // whole file after every keystroke was most of a 73.6 ms frame.
        // `unluminous_core::incremental::Tokens` starts at the line the edit was on and stops once
        // the tokens agree with what was there before, and reports **every** token either way -- so
        // the two lists built below are complete lists, exactly as they were.
        let mut spans: Vec<(std::ops::Range<usize>, unluminous_core::Color)> = Vec::new();
        let mut tokens = unluminous_core::folding::Tokens::default();
        let mut embedded: Vec<unluminous_core::syntax::Embedded> = Vec::new();
        let dirt = self.files.at(index).document.syntax_dirt();
        let markup = plugin.grammar.markup;
        let update = match markup {
            // A markup grammar has no partial reading, and it is the one that produces embedded
            // spans -- so that path is left byte for byte as it was.
            true => {
                unluminous_core::syntax::scan_with_embedded(
                    &text,
                    &plugin.grammar,
                    &mut embedded,
                    |range, token| {
                        match token {
                            unluminous_core::Token::Comment => tokens.note(range.clone(), true),
                            unluminous_core::Token::String => tokens.note(range.clone(), false),
                            _ => {}
                        }
                        if token != unluminous_core::Token::Text {
                            if let Some(colour) = theme.colour(token) {
                                spans.push((range, colour));
                            }
                        }
                    },
                );
                None
            }
            false => {
                let grammar = plugin.grammar.clone();
                let cache = &mut self.files.at_mut(index).syntax_tokens;
                // **Only the stretch that changed**, and already in file order, so `spans` holds a
                // handful of entries after a keystroke rather than the file's worth and nothing is
                // sorted. It is the second half of the saving and it is the larger half: building
                // and sorting 157,000 spans was 17 ms of every keystroke on a 2 MB file.
                let update = cache.update(&text, &grammar, dirt, |range, token| {
                    if token != unluminous_core::Token::Text {
                        if let Some(colour) = theme.colour(token) {
                            spans.push((range, colour));
                        }
                    }
                });
                // The blocks that could be collapsed are a question about the **whole** file however
                // small the edit was, so they are read off the list the cache already keeps rather
                // than being reported again. One pass, no second reading of the rules.
                for (range, token) in cache.all() {
                    match token {
                        unluminous_core::Token::Comment => tokens.note(range.clone(), true),
                        unluminous_core::Token::String => tokens.note(range.clone(), false),
                        _ => {}
                    }
                }
                Some(update)
            }
        };
        // A markup file's raw text elements are other languages, and the plugin that claims each
        // one is what colours it — the same question the Markdown preview asks of a fence.
        if !embedded.is_empty() {
            self.colour_the_embedded(&text, &embedded, &mut spans, &mut tokens);
            // The embedded spans were produced after the outer pass, and `set_syntax` applies
            // them in the order given, skipping anything that starts before the previous end.
            spans.sort_by_key(|(range, _)| range.start);
            tokens.put_in_order();
        }
        let file = self.files.at_mut(index);
        match update {
            // Only the stretch whose tokens changed is painted again: `insert` and `remove_range`
            // have already shifted the style spans outside it by the edit, so they are already right.
            Some(update) => file.document.set_syntax_in(base, &spans, update.changed),
            None => file.document.set_syntax(base, &spans),
        }
        // `set_syntax` bumps the revision, so what is remembered is the revision *after* it, or the
        // next frame would colour it all over again for ever. The tokens are keyed on that same
        // number, which is what lets `fold_regions` use them instead of reading the file again.
        let now = file.document.text_revision();
        file.coloured_revision = Some(now);
        file.cached.fold_tokens = Some((now, tokens));
        file.cached.stale = true;
    }

    /// The raw text elements of a markup file, coloured by the plugin that claims each one's
    /// language.
    ///
    /// `unluminous-core` says where a `<style>` block is and what language it names and colours
    /// nothing, so this runs the ordinary scan over that stretch with that language's grammar and
    /// offsets the ranges into the file — the mirror of [`PluginHighlighter`], which asks the
    /// same question of a fence in a Markdown document. A language nothing claims answers with
    /// nothing, and the block keeps the colour the outer pass gave it; switching the CSS plugin
    /// off withdraws the colouring inside `<style>` in the same frame, because the plugin is
    /// asked at the moment of use. One level deep: the embedded scan's own embedded list is
    /// discarded. The block's own comments and strings are noted too, or a `}` inside a CSS
    /// string would fold.
    fn colour_the_embedded(
        &self,
        text: &str,
        embedded: &[unluminous_core::syntax::Embedded],
        spans: &mut Vec<(std::ops::Range<usize>, unluminous_core::Color)>,
        tokens: &mut unluminous_core::folding::Tokens,
    ) {
        for region in embedded {
            let Some(inside) = self.plugins.for_language(&region.language) else {
                continue;
            };
            let inside_theme = crate::services::plugins::scheme_of(inside);
            let start = region.range.start;
            unluminous_core::syntax::scan(
                &text[region.range.clone()],
                &inside.grammar,
                |range, token| {
                    let shifted = range.start + start..range.end + start;
                    match token {
                        unluminous_core::Token::Comment => tokens.note(shifted.clone(), true),
                        unluminous_core::Token::String => tokens.note(shifted.clone(), false),
                        _ => {}
                    }
                    if token != unluminous_core::Token::Text {
                        if let Some(colour) = inside_theme.colour(token) {
                            spans.push((shifted, colour));
                        }
                    }
                },
            );
        }
    }

    /// Throw away what was laid out for the tab that is showing, because a different document has
    /// taken it.
    pub(crate) fn forget_layout(&mut self) {
        let file = self.files.active_mut();
        file.forget_what_was_worked_out();
        file.preview_scroll = 0.0;
    }

    /// Lay the file that is showing out, if the text, the formatting or the width changed since the
    /// last time.
    ///
    /// What was worked out is kept on the tab rather than on the window, so each pane's file is laid
    /// out at that pane's width and nothing is laid out twice a frame. See `files::Cached`.
    fn refresh_layout(&mut self, width: f32) {
        // The **text** revision, not the revision. Moving the caret bumps the revision, so keying
        // this on it laid the whole document out again on every frame of dragging a selection — 82 ms
        // a frame on a file the size of `app/mod.rs`. See `tasks/task-1666-performance-tdd.md`
        // section 2.
        let revision = self.document().text_revision();
        // And the fold revision beside it, because collapsing a block changes the layout without
        // changing a byte of the text. It is a counter of its own so that a fold does not re-colour
        // the file or rebuild the preview — `tasks/task-1686-folding-tdd.md` section 5.1.
        let folded = self.document().fold_revision();
        let cached = &self.files.active().cached;
        if !cached.stale
            && revision == cached.laid_out_revision
            && folded == cached.laid_out_folds
            && (width - cached.laid_out_width).abs() < 0.5
        {
            return;
        }
        // Read before the layout is taken out of the cache, because taking it borrows the file.
        let (cached_revision, cached_folds, cached_width) =
            (cached.laid_out_revision, cached.laid_out_folds, cached.laid_out_width);
        self.layouts_built += 1;
        let index = self.files.active_index();
        let hidden = self.hidden_paragraphs(index);
        // What was laid out last time is handed over rather than thrown away: `relayout` keeps every
        // paragraph whose text and formatting are unchanged, so typing a letter costs the paragraph
        // it was typed into instead of the file.
        let previous = std::mem::take(&mut self.files.active_mut().cached.layout);
        let first_layout = previous.lines.is_empty();
        // **Which paragraphs changed, asked of the document rather than worked out by reading the
        // file** (`task-1984` C7). `relayout` used to fingerprint every paragraph and compare each
        // against the previous layout's, which is 17.2 ms of a 21.7 ms keystroke on a 2 MB file. The
        // document has known where every edit landed since `splice` started saying so, and it keeps
        // the answer per revision -- so this pane asks about the revision it itself last laid out.
        //
        // The hint is only about the **text**. A fold or a change of width alters the layout without
        // any edit having happened, so both say `whole` and read the document, which is what every
        // relayout did before this.
        let touched = match folded == cached_folds && (width - cached_width).abs() < 0.5 {
            true => self.document().touched_since(cached_revision),
            false => Touched::whole(),
        };
        let mut laid = relayout_touching(
            previous,
            self.document().text(),
            self.document().chars(),
            self.document().paragraphs(),
            &self.renderer,
            width,
            &hidden,
            touched,
        );
        if first_layout {
            laid.compact_capacity();
        }
        let cached = &mut self.files.active_mut().cached;
        cached.stale = false;
        cached.layout = laid;
        cached.laid_out_revision = revision;
        cached.laid_out_folds = folded;
        cached.laid_out_width = width;
    }

    /// Where each pane goes, left to right, from the shares the panes were left at.
    ///
    /// The last pane is taken to the right hand edge rather than measured, so rounding cannot leave a
    /// hairline of the window showing down the far side.
    pub(crate) fn pane_rects(&self, area: Rect) -> Vec<Rect> {
        let widths = self.files.pane_widths().to_vec();
        let mut out = Vec::with_capacity(widths.len());
        let mut left = area.left();
        for (at, share) in widths.iter().enumerate() {
            let right = if at + 1 == widths.len() {
                area.right()
            } else {
                (left + area.width() * share).floor()
            };
            out.push(Rect::from_min_max(
                Pos2::new(left, area.top()),
                Pos2::new(right.max(left), area.bottom()),
            ));
            left = right;
        }
        if out.is_empty() {
            out.push(area);
        }
        out
    }

    /// Draw one pane: its strip of tabs, and the editing area under it.
    ///
    /// `focused` says whether this is the pane with the keyboard, which is **not** the same question
    /// as which pane `files` is focused on while this runs — see the note in `ui`. Returns true when
    /// the pane was clicked in, which is what moves the keyboard to it.
    ///
    /// A close is reported rather than done, because closing a tab can empty a pane and renumber the
    /// ones after it, and the loop this is called from is walking those numbers.
    pub(crate) fn show_pane(
        &mut self,
        ui: &mut egui::Ui,
        pane: usize,
        focused: bool,
        area: Rect,
        close: &mut Option<usize>,
    ) -> bool {
        // **A tab in a pane is set in the window's own font.** A tab dragged out of a File Editor node was
        // given a size of its own there — `task-1905` — and a pane has no per-node size, so it is put back
        // here rather than at each of the places a tab can leave a node, which is `follow_the_open_file`'s
        // rule. `sized_at` is `None` for every tab nothing resized, so this costs one comparison.
        self.put_a_panes_tabs_back_to_the_windows_font(pane);
        let tabs_rect = Rect::from_min_size(area.min, Vec2::new(area.width(), file_tabs::HEIGHT));
        let editor_rect = Rect::from_min_max(Pos2::new(area.left(), tabs_rect.bottom()), area.max);
        let mut took_the_keyboard = false;
        // Everything in the pane is drawn into a `Ui` of its own, carrying the pane's number as its
        // id salt. egui identifies a widget by its id, and every control inside asks for one with
        // `ui.id().with(...)` — so without this the gutters of two panes, or their editing areas, or
        // two previews, would be one widget as far as egui is concerned and one click would reach
        // both. The alternative was passing a pane number into five components; one salt does it for
        // everything, including whatever is added to a pane later.
        let ui =
            &mut ui.new_child(egui::UiBuilder::new().max_rect(area).id_salt(("editor-pane", pane)));

        // The tabs in this pane, in the order they are drawn. The strip counts within itself, so what
        // it reports is turned back into an index into the open files here.
        let indices = self.files.tabs_in(pane);
        let icons: Vec<Option<egui::TextureHandle>> = indices
            .iter()
            .map(|index| self.files.at(*index).path().map(std::path::Path::to_path_buf))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|path| self.plugin_icon(ui.ctx(), path.as_deref()))
            .collect();
        let tabs: Vec<TabView> = indices
            .iter()
            .zip(icons)
            .map(|(index, icon)| {
                let file = self.files.at(*index);
                TabView {
                    name: file.name(),
                    modified: file.document.is_modified(),
                    transient: file.transient,
                    marker: file.path().map(theme::file_marker).unwrap_or(color::file_text()),
                    icon,
                }
            })
            .collect();
        let active = self
            .files
            .showing_in(pane)
            .and_then(|showing| indices.iter().position(|index| *index == showing))
            .unwrap_or(0);
        let opacity = self.settings.opacity;
        let outcome = {
            let mut tabs_ui = ui.new_child(egui::UiBuilder::new().max_rect(tabs_rect));
            file_tabs::show(&mut tabs_ui, tabs_rect, &tabs, active, pane, focused, opacity)
        };
        let at = |within: usize| indices.get(within).copied();
        // Where this strip drew itself, for the drag to be settled against once every pane has been
        // drawn. Pushed in pane order, because the loop walks the panes left to right.
        self.tab_strips.push(outcome.strip);
        if let Some((within, pointer)) = outcome.dragging {
            if let Some(file) = at(within) {
                self.tab_drag = Drag::carrying(file, pointer, outcome.dropped);
            }
        }
        if let Some(index) = outcome.show.and_then(at) {
            self.show_tab(index);
            self.focus = Focus::Editor;
            took_the_keyboard = true;
        }
        if let Some(index) = outcome.keep.and_then(at) {
            self.show_tab(index);
            self.files.make_permanent(index);
            self.focus = Focus::Editor;
            took_the_keyboard = true;
        }
        if let Some(index) = outcome.close.and_then(at) {
            *close = Some(index);
        }
        // Two presses on the empty part of the strip fill the window with the editing area, and two more
        // put the panels back. `task-1771`: every pane is maximised from its own top, and the strip is what
        // the editing area has instead of a header.
        if outcome.twice_on_the_empty_part {
            self.maximise_wanted = true;
        }
        if let Some((within, where_)) = outcome.menu {
            // The tab is shown first, so every entry in the menu can be about "the tab that is
            // showing" and so be an action with no argument. See `actions::tab_menu`.
            if let Some(index) = at(within) {
                self.show_tab(index);
                self.focus = Focus::Editor;
                took_the_keyboard = true;
            }
            self.tab_menu = Some((where_, pane));
        }

        // The editing area: the picture, the source, the preview, or both side by side.
        took_the_keyboard |= self.show_editing_area(ui, editor_rect, focused);
        took_the_keyboard
    }

    /// Put any tab in this pane that was sized inside a node back to the window's own font.
    ///
    /// See the call in [`Self::show_pane`] for why it lives there rather than beside every place a tab can
    /// leave a node.
    fn put_a_panes_tabs_back_to_the_windows_font(&mut self, pane: usize) {
        let sized: Vec<usize> = self
            .files
            .tabs_in(pane)
            .into_iter()
            .filter(|index| self.files.at(*index).sized_at.is_some())
            .collect();
        if sized.is_empty() {
            return;
        }
        let change = self.settings.as_style_change();
        for index in sized {
            self.files.at_mut(index).document.set_base_style(change.clone());
            self.files.at_mut(index).cached.stale = true;
            self.files.at_mut(index).sized_at = None;
        }
    }

    /// Draw whatever the open tab holds into `area`.
    ///
    /// A picture, the Markdown source, the preview, or the source and the preview side by side with a
    /// draggable divider between them. Split out of [`Self::ui`] because it is the one place the four
    /// answers are chosen between, and `ui` has enough to do laying the window out.
    fn show_editing_area(&mut self, ui: &mut egui::Ui, area: Rect, focused: bool) -> bool {
        // A tab a plugin draws, which is the fourth answer and the newest. It is asked before the
        // picture for no reason but that a plugin tab has no path and the questions below all start by
        // reading one.
        if let Some(tab) = self.files.active().plugin.clone() {
            if focused {
                self.editor_area = area;
            }
            return self.show_plugin_tab(ui, area, &tab);
        }
        if self.files.active().is_browser() {
            if focused {
                self.editor_area = area;
            }
            return self.show_browser(ui, area, focused);
        }
        if self.files.active().is_picture() {
            if focused {
                self.editor_area = area;
            }
            return self.show_picture(ui, area);
        }
        match self.view_mode() {
            ViewMode::Raw => return self.show_editor(ui, area, focused),
            ViewMode::Preview => {
                if focused {
                    self.editor_area = area;
                }
                self.show_preview(ui, area);
            }
            ViewMode::SideBySide => {
                let fraction = self.panes.preview_fraction.clamp(0.15, 0.85);
                let split = (area.width() * fraction).floor();
                let left = Rect::from_min_size(area.min, Vec2::new(split, area.height()));
                let right =
                    Rect::from_min_max(Pos2::new(area.left() + split, area.top()), area.max);
                let before = self.where_both_halves_are();
                let took = self.show_editor(ui, left, focused);
                self.show_preview(ui, right);
                self.scroll_the_two_halves_together(before, area);
                // The split between the source and the preview is a pane like any other, so it is dragged.
                let edge = Rect::from_min_size(
                    Pos2::new(right.left(), right.top()),
                    Vec2::new(1.0, right.height()),
                );
                let drag = splitter::show(ui, edge, "preview", splitter::Axis::Upright);
                if drag.delta != 0.0 && area.width() > 0.0 {
                    self.panes.preview_fraction =
                        (fraction + drag.delta / area.width()).clamp(0.15, 0.85);
                    self.unsaved_settings = true;
                }
                if drag.reset {
                    self.panes.preview_fraction = 0.5;
                    self.unsaved_settings = true;
                }
                return took;
            }
        }
        false
    }

    /// What the gutter is showing for the file that is open.
    fn gutter<'a>(
        &'a self,
        folds: &'a [(usize, bool)],
        breakpoints: &'a [(usize, gutter::BreakpointMark)],
    ) -> Gutter<'a> {
        let file = self.files.active();
        Gutter {
            numbers: self.settings.line_numbers,
            blame: file.blame.as_deref(),
            changes: &file.line_changes,
            // Worked out before this is called rather than here, because reading a file for what it
            // could fold wants `&mut self` for the cache and a component is handed what it draws.
            folds,
            breakpoints,
            can_debug: self.debug_applies_to(file.path()),
            execution_point: self.execution_paragraph(file.path()),
            // What the gutter's own type follows, so the numbers grow with the text rather than
            // staying a fixed eleven and a half points beside forty point letters.
            //
            // **The size this file is really set in, not the window's setting.** An editor node on the canvas
            // gives its own tab a size through `set_base_style` and records it as `OpenFile::sized_at`, so a
            // gutter reading the setting drew eleven point numbers beside twenty point letters — which is
            // `task-1907`'s report. `None` is every tab in a pane, which is the setting, so nothing outside the
            // canvas changes by a pixel. Every other field here already reads `file`.
            font_size: file.sized_at.unwrap_or(self.settings.font_size),
        }
    }

    /// Which paragraph of `path` the program is stopped on, when it is stopped in that file.
    ///
    /// Zero-based, which is what `unluminous-core` calls a source line everywhere and is one less than
    /// the number the gutter draws — the adapter's answer is one-based, so the conversion happens
    /// here rather than at each of the places that read it.
    fn execution_paragraph(&self, path: Option<&Path>) -> Option<usize> {
        let (stopped_in, line) = self.debug.as_ref()?.location()?;
        let path = path?;
        (same_file(path, &stopped_in)).then(|| line.saturating_sub(1))
    }

    /// The values to paint at the ends of the lines that name them, while the program is paused.
    ///
    /// **DAP has no request for this; it is the client matching names**, and Unluminous already owns both
    /// halves of the machinery: `FileSymbols` has the file's identifiers sorted by position, and the
    /// paused frame's first level of variables has already been fetched because the tile shows it.
    /// So the match is a walk over the file's words against one map.
    ///
    /// Worked out **once per stop** and cached, keyed on the text revision and the frame that is
    /// showing, which is `symbols::Hover`'s own key made once more: a frame in which neither moved
    /// costs two comparisons. It is only ever computed for the file the program is stopped in, so a
    /// project with fifty tabs open pays for one of them.
    fn inline_values(&mut self, index: usize) -> Vec<(usize, String)> {
        let Some(debug) = self.debug.as_ref() else {
            return Vec::new();
        };
        if !debug.is_paused() {
            return Vec::new();
        }
        let Some(path) = self.files.at(index).path().map(Path::to_path_buf) else {
            return Vec::new();
        };
        // Only the file the program is stopped in: a local of the paused frame means nothing at the
        // end of a line of another file that happens to use the same word.
        let stopped_in = debug.location().map(|(path, _)| path);
        if !stopped_in.is_some_and(|stopped| same_file(&path, &stopped)) {
            return Vec::new();
        }
        let revision = self.files.at(index).document.text_revision();
        let frame = debug.frame;
        // **And what the debugger has read**, which is the third thing this depends on and was
        // missing until `task-1696`: the variables arrive a round trip after the stop, so a key made
        // of the text and the frame alone cached the empty answer the first ask produced.
        let reads = debug.reads();
        if let Some(cached) = self.inline_cache.as_ref() {
            if cached.revision == revision
                && cached.frame == frame
                && cached.reads == reads
                && cached.path == path
            {
                return cached.values.clone();
            }
        }
        let values = debug.top_frame_values();
        let text = self.files.at(index).document.text().to_string();
        let read = &self.tab_symbols(index).read;
        let mut rows: Vec<(usize, String)> = Vec::new();
        for word in read.word_ranges() {
            let Some(value) = values.get(&text[word.clone()]) else {
                continue;
            };
            let paragraph = text[..word.start].bytes().filter(|byte| *byte == b'\n').count();
            // One value a line, the **first** name on it: a line that names three locals would
            // otherwise carry three values and be unreadable, and the first is the one the line is
            // most about.
            if rows.last().is_some_and(|(known, _)| *known == paragraph) {
                continue;
            }
            // A value the debugger could not read is **not painted**. It is still in the tree, in the
            // debugger's own words, which is where the honest full answer belongs — but at the end of
            // a line of code `step = <variable not available>` is the debugger declining to answer
            // dressed as information, and the reference editor paints nothing there either. Seen on the released
            // 0.14.0 build against a real CodeLLDB: two of three inline values on a seven-line program
            // were this.
            if is_unreadable(value) {
                continue;
            }
            rows.push((paragraph, format!("{} = {}", &text[word.clone()], elide_value(value))));
        }
        self.inline_cache =
            Some(InlineValues { revision, frame, reads, path, values: rows.clone() });
        rows
    }

    /// The inline values of the file that is showing, for a test to look at without drawing a frame.
    ///
    /// It deliberately does **not** throw the cache away first, which it used to: what a test wants
    /// to see is what the window is really drawing, and clearing the cache hid `task-1696`'s finding
    /// that the key was missing the one thing these are built from — the debugger's own answers.
    pub fn inline_values_for_test(&mut self) -> Vec<(usize, String)> {
        let index = self.files.active_index();
        self.inline_values(index)
    }

    /// What the gutter draws for each breakpoint in one file: which paragraph it is on, and how.
    ///
    /// The document is the authority for **where** they are and the session for **whether the
    /// debugger agreed to stop there**, which is the two-authority split §6.3 asks for: Unluminous draws
    /// the adapter's answer rather than its own hope. With no session running there is nobody to have
    /// said a breakpoint is unbound, so every one of them is drawn solid.
    pub fn breakpoint_marks(&self, index: usize) -> Vec<(usize, gutter::BreakpointMark)> {
        let file = self.files.at(index);
        let document = &file.document;
        if document.breakpoints().is_empty() {
            return Vec::new();
        }
        let path = file.path().map(Path::to_path_buf);
        let mut rows: Vec<(usize, gutter::BreakpointMark)> = document
            .breakpoints()
            .iter()
            .map(|breakpoint| {
                let answered = path
                    .as_deref()
                    .and_then(|path| self.debug.as_ref()?.verified(path, breakpoint.offset));
                let mark = gutter::BreakpointMark {
                    enabled: breakpoint.enabled,
                    verified: answered.map(|answer| answer.verified).unwrap_or(true),
                    conditional: breakpoint.is_conditional(),
                };
                // The adapter's own line wins where it gave one: a breakpoint it moved to the next
                // statement is drawn where the program will really stop, for the life of the session.
                let paragraph = match answered.and_then(|answer| answer.line) {
                    Some(line) => line.saturating_sub(1),
                    None => document.text().byte_to_line(breakpoint.offset),
                };
                (paragraph, mark)
            })
            .collect();
        // The gutter binary searches this list, so it has to be sorted — and it may not be, because
        // an adapter is free to move a breakpoint to the next statement and two that were in offset
        // order can come back out of it. Two on one line would break the search as well, so the
        // later of them gives way, which is the rule `Breakpoints` itself keeps about two at one
        // offset.
        rows.sort_by_key(|(paragraph, _)| *paragraph);
        rows.dedup_by_key(|(paragraph, _)| *paragraph);
        rows
    }

    /// Draw the source of the file that is showing into `area`.
    ///
    /// `focused` is whether this pane has the keyboard. Only that pane draws a caret and only that
    /// pane reads the keyboard, or every pane would take the same key presses. Returns true when the
    /// pane was clicked in, which is what moves the keyboard to it.
    /// What the pointer is over in this pane while the platform's modifier is held.
    ///
    /// `Modifiers::command` is the Apple key on macOS and the control key on Windows, which is what
    /// a person means by the modifier in either place. Nothing at all is worked out while it is not
    /// held, and letting go of it forgets what was: an underline that outlived the modifier would be
    /// an affordance promising something the next click would not do.
    ///
    /// **Every pane is asked, not only the one with the keyboard** (`task-2063`). The reference editor jumps on
    /// the first `Ctrl`/`Cmd`+Click in an editor whatever had the keyboard before; here the first
    /// click after using the explorer or the terminal only moved the keyboard, which read as the
    /// feature not working. There is one pointer, so only the pane under it has a hover position and
    /// only that pane draws an underline.
    fn symbol_under_the_pointer(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        origin: Pos2,
    ) -> editor_view::SymbolPointer {
        let held = ui.input(|input| input.modifiers.command);
        if !held || !self.definitions_apply_here() {
            self.forget_the_hover();
            return editor_view::SymbolPointer::default();
        }
        let Some(at) = response.hover_pos() else {
            return editor_view::SymbolPointer::default();
        };
        let local = at - origin;
        let offset = self.layout().offset_at(local.x, local.y);
        let hover = self.resolve_under_the_pointer(offset);
        editor_view::SymbolPointer { word: hover.map(|hover| hover.word) }
    }

    /// Everything an editing area's components are handed, read before any of them is drawn.
    ///
    /// **`task-1984` §3.6.** `show_editor` was 369 lines and its first fifty were this: five separate
    /// readings, each with the same reason written out beside it — the cache or the search wants
    /// `&mut self`, and every component below is handed what it draws rather than being given the
    /// window. Said once here instead of five times there.
    ///
    /// The order is the order it was in, because these read each other's caches; and it is all before
    /// the first `ui.interact`, because **egui hands a pointer to the last widget that asked for the
    /// point**, so anything moved across that line is a behaviour change.
    fn what_the_editor_draws_from(&mut self, index: usize, focused: bool) -> EditorReadings {
        // What this file could fold and what of it is folded.
        let fold_marks: Vec<(usize, bool)> = self.fold_marks(index).to_vec();
        // What the gutter draws for each breakpoint: it asks the session what the adapter said.
        let breakpoint_marks = self.breakpoint_marks(index);
        // The line the program is stopped on, and the values to paint at the ends of the lines that
        // bind them.
        let execution_point = self.execution_paragraph(self.files.at(index).path());
        let inline_values = self.inline_values(index);
        // Where the Find bar's matches are. Empty when the bar is shut or this pane does not have the
        // keyboard, so a split view paints the bands in the pane being searched and not in the other
        // one. `task-1804`.
        let find_matches: Vec<std::ops::Range<usize>> = match (focused, self.find.as_mut()) {
            (true, Some(_)) => {
                let text = self.files.at(index).document.text().to_string();
                let revision = self.files.at(index).document.text_revision();
                let find = self.find.as_mut().expect("it is there");
                // **Selected as you type**, which is what makes the Find box an incremental search
                // rather than a box you press Enter on. Only when the *search* changed, never when
                // the document did -- see `Find::refresh` for why the two are told apart.
                let search_changed = find.refresh(&text, revision);
                let current = find.current();
                let others = find.others();
                if search_changed {
                    if let Some(range) = current {
                        self.select_the_match(range);
                    }
                }
                others
            }
            _ => Vec::new(),
        };
        EditorReadings {
            fold_marks,
            breakpoint_marks,
            execution_point,
            inline_values,
            find_matches,
        }
    }

    pub(crate) fn show_editor(&mut self, ui: &mut egui::Ui, area: Rect, focused: bool) -> bool {
        let index = self.files.active_index();
        let EditorReadings {
            fold_marks,
            breakpoint_marks,
            execution_point,
            inline_values,
            find_matches,
        } = self.what_the_editor_draws_from(index, focused);
        // Set by an arrow in the gutter or a badge in the text, and by a click in the gutter's
        // breakpoint column, and both acted on at the **end** of the frame: a fold and a breakpoint
        // each change the layout, and changing it half way through drawing this pane would leave the
        // rest of the frame drawing from a layout that no longer matches.
        let mut folded: Option<usize> = None;
        let mut toggled_breakpoint: Option<usize> = None;
        // The gutter takes the left of the editing area, and the text starts after it. With no
        // gutter the text keeps the padding it always had, so putting the numbers away leaves the
        // window looking exactly as it did before there were any.
        let mut gutter = self.gutter(&fold_marks, &breakpoint_marks);
        let lines = self.document().text().len_lines();
        // **The type is reduced only if the column would take more than its share of the pane**, which is what
        // replaced the ceiling `LARGEST_TYPE` used to put on it — see `gutter::fitted_size`. At every ordinary
        // size this answers with the size it was given and nothing changes.
        gutter.font_size = gutter::fitted_size(ui, &gutter, lines, area.width());
        let gutter_width = gutter::width(ui, &gutter, lines);
        let gutter_rect = Rect::from_min_size(area.min, Vec2::new(gutter_width, area.height()));
        let area = Rect::from_min_max(Pos2::new(area.left() + gutter_width, area.top()), area.max);
        let padding =
            if gutter_width > 0.0 { editor_view::PADDING } else { size::EDITOR_PADDING_X };

        if focused {
            // Only the focused pane, because this is what the status bar and `editor status` read on
            // the frame after and they mean the pane that is being typed into.
            self.editor_area = area;
        }
        let response = ui.interact(area, ui.id().with("editor"), egui::Sense::click_and_drag());
        let mut took_the_keyboard = false;
        if response.clicked() || response.drag_started() || response.secondary_clicked() {
            self.focus = Focus::Editor;
            // A press in the source takes the copy back from the preview beside it. See
            // `UnluminousApp::reading_preview`.
            self.reading_preview = false;
            took_the_keyboard = true;
        }
        // Over the writing, the pointer is a vertical bar rather than an arrow, which is what it is in
        // every editor and what `task-1658` asks for. Only over the text itself: the gutter is a
        // rectangle of its own and the divider beside it sets its own pointer, and both are drawn after
        // this, so the last one to speak wins where they overlap.
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        // **Who holds the keyboard is `Focus`, and an editing area is in one of two places.**
        //
        // A pane's editing area has it while `Focus::Editor` stands; a File Editor node's has it while the
        // canvas holds it and this node is the chosen one, which is what `focused` already answered — see
        // `show_an_editor_node`, which borrows `OpenFiles::focus` to the node for as long as it is drawn.
        //
        // Asking only for `Focus::Editor` is what `task-1914` reports as *"Im unable to edit files in file
        // editor"*: clicking in a node put the caret there, `take_the_keyboard_for_the_space` set
        // `Focus::Space`, and from the next frame on every key was dropped. The click frame worked, which is
        // why it read as the caret being drawn and nothing being typed.
        //
        // The two are told apart by where the tab being drawn lives rather than by a flag, so a pane drawn
        // while the canvas holds the keyboard answers no without either caller having to remember.
        let has_keyboard = focused
            && match self.focus {
                Focus::Editor => true,
                Focus::Space => self.files.focus().node().is_some(),
                _ => false,
            };
        let text_width = (area.width() - padding - size::EDITOR_PADDING_X).max(50.0);
        // A caret is never inside a hidden paragraph. `reveal_caret` is set by everything that puts
        // the caret somewhere without a click — a jump to a definition, a search hit, `unluminous-cli
        // editor caret --line`, `Navigate Back` — so asking here is one place rather than one per
        // jump, which is `follow_the_open_file`'s rule: the next jump added would be the one that
        // forgot. Before the layout, so the frame that reveals it is the frame that draws it.
        if self.reveal_caret && focused {
            self.reveal_the_caret_from_a_fold();
        }
        self.refresh_layout(text_width);
        let view_height = area.height() - size::EDITOR_PADDING_Y * 2.0;
        // Straight after the layout, so the rest of the frame — the wheel, the caret, the painter
        // — sees the scroll position the zoom asked for rather than the one it was left at.
        self.keep_the_place_through_a_zoom(view_height);

        let (was, bar_name, grab) = self.take_hold_of_the_scrollbar(ui, area, view_height);

        let scroll = self.files.active().scroll;
        let origin = Pos2::new(area.left() + padding, area.top() + size::EDITOR_PADDING_Y - scroll);

        // What the pointer is over while the modifier is held, worked out **before** the click and
        // cached against the text revision and the word. Resolve first is VS Code's model and it is
        // what makes the click feel instantaneous: the answer is already in hand, and only a word
        // that really has somewhere to go is underlined.
        let symbol = self.symbol_under_the_pointer(ui, &response, origin);
        // The other half of that gesture, and its opposite number: with the modifier **up**, a
        // pointer resting on a name while the program is paused asks the debugger what it holds.
        // Two affordances on one word would be two promises the one click cannot both keep, so
        // `value_under_the_pointer` does nothing at all while the modifier is held. `task-1696`.
        self.value_under_the_pointer(ui, &response, origin, area, focused);
        self.remember_where_the_value_tooltip_hangs(origin, area);
        if symbol.resolved() {
            // A hand rather than the writing bar, which is what says the word is a link.
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let taken = self.take_the_editors_input(
            ui,
            &response,
            origin,
            &symbol,
            EditorInput { has_keyboard, focused, text_width },
        );
        if taken.jumped {
            return true;
        }

        self.open_the_text_menu_on_a_right_click(&response, origin);
        self.claim_a_zoom_over_the_editor(ui, area, has_keyboard);

        let wheel = ui.input(|input| input.smooth_scroll_delta.y);
        let mut scroll = self.files.active().scroll;
        // `contains_pointer` rather than `hovered`: the scrollbar is a widget over this one and
        // takes the hover from it, and a wheel turned with the pointer resting on the bar is still
        // about the page the bar belongs to.
        if wheel != 0.0 && response.contains_pointer() {
            scroll -= wheel;
        }
        if taken.scroll_to_caret || (self.reveal_caret && focused) {
            let caret = self.layout().caret_at(self.document().selection().head);
            if caret.y < scroll {
                scroll = caret.y;
            } else if caret.y + caret.height > scroll + view_height {
                scroll = caret.y + caret.height - view_height;
            }
        }
        if focused {
            self.reveal_caret = false;
        }
        let overflow = (self.layout().height - view_height).max(0.0);
        let scroll = scroll.clamp(0.0, overflow);
        self.files.active_mut().scroll = scroll;

        let origin = Pos2::new(area.left() + padding, area.top() + size::EDITOR_PADDING_Y - scroll);

        // The two brackets around the caret, and **only in the pane that has the keyboard**: they
        // are about where somebody is typing, and painting them in three panes at once would say
        // that three carets are being watched. It is a binary search over the comments and strings
        // the colouring already read, bounded at `BRACKET_SEARCH_LIMIT` bytes each way, which is why
        // it can be asked once a frame. `task-1922` WP4.
        let bracket_pair = focused.then(|| self.bracket_pair_at_the_caret()).flatten();

        // Where the completion popup hangs, worked out from the caret's own box at the position the
        // frame settled on. Recorded rather than drawn here: the window draws it after the whole row
        // of panes, so it sits over the dividers rather than under one.
        if focused {
            self.remember_where_the_completion_hangs(origin, area);
        }

        if gutter_width > 0.0 {
            let outcome =
                self.show_the_gutter(ui, gutter_rect, origin.y, &fold_marks, &breakpoint_marks);
            folded = outcome.toggle_fold.or(folded);
            toggled_breakpoint = outcome.toggle_breakpoint.or(toggled_breakpoint);
        }

        if let Some(line) = self.paint_the_editor(
            ui,
            area,
            focused,
            origin,
            EditorPainting {
                has_keyboard,
                underline: symbol.word.clone(),
                execution_point,
                inline_values: &inline_values,
                find_matches: &find_matches,
                bracket_pair,
                fold_marks: &fold_marks,
                scroll,
                was,
                view_height,
                bar_name: &bar_name,
                bar_active: grab.active,
            },
        ) {
            folded = Some(line);
        }
        if let Some(line) = folded {
            self.toggle_fold_at_line(line);
        }
        if let Some(line) = toggled_breakpoint {
            self.toggle_breakpoint_at_line(line);
        }
        took_the_keyboard
    }

    /// Hand the frame's pointer and keys to the document. `true` when the pane jumped somewhere.
    ///
    /// **`task-1984` §3.6, out of `show_editor`.** A jump answers `true` because it opens another
    /// file, which leaves everything below this — the layout, the origin, the painter — describing
    /// the file that was showing a moment ago.
    fn take_the_editors_input(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        origin: Pos2,
        symbol: &editor_view::SymbolPointer,
        input: EditorInput,
    ) -> EditorTyped {
        let EditorInput { has_keyboard, focused, text_width } = input;
        let formatting = self.formatting_applies_here();
        // Whether a character reached the document this frame, which is the one thing the automatic
        // trigger fires on. Read before the input is handled, because handling it is what consumes
        // the events. A paste, an undo and a command line edit are all deliberately not typing.
        let typed = has_keyboard
            && ui.input(|input| {
                input.events.iter().any(|event| {
                    matches!(event, egui::Event::Text(text) if !text.chars().any(char::is_control))
                })
            });
        // Read before the tab is borrowed, because both come off the settings and the borrow below
        // takes the whole window otherwise. `task-1922` WP4.
        let typing = editor_view::Typing {
            indent: self.indent_text(),
            auto_indent: self.settings.auto_indent,
        };
        // Taken apart by field, because the input handlers want the document mutably while the
        // layout they measure against is borrowed at the same time, and a method on `self` would
        // borrow the whole window. Both live on the same tab, and the two are separate fields of it,
        // which is a borrow the compiler allows through one reference.
        let file = self.files.active_mut();
        let laid = &file.cached.layout;
        let document = &mut file.document;
        let pointer = editor_view::handle_pointer(response, document, laid, origin, symbol);
        let pointer_changed = pointer.changed;
        let outcome =
            editor_view::handle_input(ui, document, laid, has_keyboard, formatting, &typing);
        // The window decides what a jump means, which is the rule every component follows.
        if let Some(offset) = pointer.jump {
            self.focus = Focus::Editor;
            self.go_to_definition(offset);
            return EditorTyped { jumped: true, scroll_to_caret: false };
        }
        let scroll_to_caret = outcome.scroll_to_caret;
        if let Some(text) = outcome.copy {
            ui.ctx().copy_text(text);
        }
        if outcome.changed {
            // Typing into a file you were only glancing at plainly means you meant to open it, so
            // the transient tab stops being one a single click will take away.
            let active = self.files.active_index();
            self.files.make_permanent(active);
        }
        if outcome.changed || pointer_changed {
            self.refresh_layout(text_width);
        }
        // Open, refilter or close the completion popup, now that the letter just typed is in the
        // file. Only the pane with the keyboard, because there is one popup and it belongs to
        // whichever pane is being typed into.
        if focused {
            self.keep_the_completion_fresh(typed);
        }
        EditorTyped { jumped: false, scroll_to_caret }
    }

    /// Let the bar down the right hand edge be dragged, and answer with where the frame opened.
    ///
    /// **Taken hold of here rather than at the end of the frame**: the editing area asks for drags
    /// over the whole of its rectangle and egui hands a point to the last widget that asked for it,
    /// so a bar added after the text is a bar that can be dragged. It is *drawn* at the end, once the
    /// wheel and the caret have had their say — see `components::scrollbar` and
    /// [`Self::paint_the_editor`]. `task-1984` §3.6, out of `show_editor`.
    fn take_hold_of_the_scrollbar(
        &mut self,
        ui: &mut egui::Ui,
        area: Rect,
        view_height: f32,
    ) -> (f32, String, scrollbar::Grab) {
        let was = self.files.active().scroll;
        // Named after the file rather than after the half, because two panes each have one and two
        // controls must not share a name — the same reason the gutter's blame cells and a diagram
        // carry the file's name. Two panes cannot be showing one file, so the name is unique.
        let bar_name = self.files.active().name();
        let bar = scrollbar::Bar::new(area, was, self.layout().height, view_height);
        let grab = match &bar {
            Some(bar) => scrollbar::grab(ui, bar, &bar_name),
            None => scrollbar::Grab::default(),
        };
        if let Some(to) = grab.scroll {
            self.files.active_mut().scroll = to;
        }
        (was, bar_name, grab)
    }

    /// Draw the column of line numbers, fold arrows and breakpoint dots.
    ///
    /// **Drawn from the same origin as the text**, so a number cannot drift away from the line it
    /// belongs to. `task-1984` §3.6, out of `show_editor`.
    fn show_the_gutter(
        &mut self,
        ui: &mut egui::Ui,
        gutter_rect: Rect,
        origin_y: f32,
        fold_marks: &[(usize, bool)],
        breakpoint_marks: &[(usize, gutter::BreakpointMark)],
    ) -> gutter::GutterOutcome {
        let caret_line = self.document().text().byte_to_line(self.document().selection().head);
        let home = match self.files.focus() {
            crate::app::files::Home::Pane(pane) => format!("pane {pane}"),
            crate::app::files::Home::Node(node) => format!("node {node}"),
        };
        let outcome = gutter::show(
            ui,
            gutter_rect,
            &self.gutter(fold_marks, breakpoint_marks),
            self.layout(),
            origin_y,
            caret_line,
            &home,
        );
        if let Some(at) = outcome.context_menu {
            self.gutter_menu = Some(at);
            // Which row it was over, so the menu's breakpoint entries are about the line under the
            // pointer rather than about the caret — the rule the text menu already follows.
            self.gutter_menu_line = outcome.menu_paragraph;
        }
        outcome
    }

    /// A right click opens the editing area's own menu.
    ///
    /// Inside a selection it leaves the selection alone — a menu that opened with nothing selected
    /// would be a menu with nothing to mark in it, which is the whole point of it — and anywhere else
    /// it puts the caret there first, which is what every editor does.
    fn open_the_text_menu_on_a_right_click(&mut self, response: &egui::Response, origin: Pos2) {
        if !response.secondary_clicked() {
            return;
        }
        let Some(at) = response.interact_pointer_pos() else { return };
        let local = at - origin;
        let offset = self.layout().offset_at(local.x, local.y);
        let selection = self.document().selection().range();
        if !selection.contains(&offset) {
            self.document_mut().apply(Command::PlaceCaret { offset, extend: false });
        }
        self.text_menu = Some(text_menu::TextMenu::new(at, offset));
        self.focus = Focus::Editor;
    }

    /// Decide whether a pinch, or the wheel with the zoom modifier held, is this pane's.
    ///
    /// Either the pointer is demonstrably over this pane, or this is the pane with the keyboard and
    /// no pane has the pointer.
    ///
    /// Neither of those is `response.hovered()`, and it took measuring the real window to find out
    /// why it must not be. A two notch gesture produced thirty eight frames, eleven of them carrying
    /// a zoom — and on every one of those eleven `hovered()` was false and `pointer.hover_pos()` was
    /// `None`, because egui reports no pointer at all on a frame whose only input is a wheel event.
    /// Gating on either alone threw the whole gesture away and the text never moved, which is exactly
    /// what the first version of this did. So the last place the pointer was seen is asked for as
    /// well, which is what `latest_pos` is, and it is what says which pane a gesture with no pointer
    /// on this frame is still about.
    fn claim_a_zoom_over_the_editor(&mut self, ui: &egui::Ui, area: Rect, has_keyboard: bool) {
        if self.zoom == ZoomClaim::Taken {
            return;
        }
        let pointer = ui
            .input(|input| input.pointer.hover_pos().or_else(|| input.pointer.latest_pos()))
            .filter(|at| area.contains(*at));
        match pointer {
            // Over this pane, so the gesture is this pane's, about the text it is over.
            Some(at) => {
                self.zoom = ZoomClaim::Taken;
                let top = area.top() + size::EDITOR_PADDING_Y;
                self.zoom_the_text(ui, (at.y - top).max(0.0));
            }
            // Not over this pane. The pane with the keyboard takes it at the end of the frame if no
            // pane turns out to have the pointer, keeping the top of its view still, because there is
            // then no point on the screen for the gesture to be about.
            None if has_keyboard => self.zoom = ZoomClaim::OfferedToTheKeyboard,
            None => {}
        }
    }

    /// Draw the text, and everything that goes over it. Answers with a fold badge that was pressed.
    ///
    /// **`task-1984` §3.6.** The last phase of [`Self::show_editor`], which was 369 lines. Everything
    /// here is drawn **after** the editing area asked for its rectangle, and every comment in it says
    /// why: egui hands a point to the last widget that asked for it, so the Find bar, the fold badges
    /// and the scrollbar all have to come after the text or they could not be clicked. A line moved
    /// out of this function and back into the one above it is a behaviour change.
    fn paint_the_editor(
        &mut self,
        ui: &mut egui::Ui,
        area: Rect,
        focused: bool,
        origin: Pos2,
        painting: EditorPainting<'_>,
    ) -> Option<usize> {
        let mut folded = None;
        let mut painter_ui = ui.new_child(egui::UiBuilder::new().max_rect(area));
        painter_ui.set_clip_rect(ui.painter().clip_rect().intersect(area));
        editor_view::paint(
            &painter_ui,
            &self.renderer,
            self.document(),
            self.layout(),
            origin,
            editor_view::PaintStyle {
                selection: color::text_selection(),
                caret: color::accent(),
                show_caret: painting.has_keyboard,
                underline: painting.underline,
                execution_point: painting.execution_point,
                inline_values: painting.inline_values,
                find_matches: painting.find_matches,
                bracket_pair: painting.bracket_pair,
            },
        );
        // The Find bar, over the text at the top right. After the editing area for the reason the
        // fold badges below are: egui hands a point to the last widget that asked for it, and the
        // editing area asks for the whole of its rectangle.
        if focused && self.find.is_some() {
            self.show_the_find_bar(&mut painter_ui, area);
        }
        // The badge standing for each collapsed block, over the text and after the end of its head
        // line. It takes clicks, so it is added after the editing area rather than before it: egui
        // hands a point to the last widget that asked for it and the editing area asks for all of
        // its rectangle, which is the same ordering the scrollbar and the pane dividers follow.
        let visible = editor_view::visible_lines(&painter_ui, self.layout(), origin);
        if let Some(line) = editor_view::fold_badges(
            &mut painter_ui,
            self.layout(),
            origin,
            painting.fold_marks,
            visible,
        ) {
            folded = Some(line);
        }
        // Drawn last, at the position the frame settled on rather than the one it opened with, or
        // the thumb is a frame behind the writing — which on a fast scroll can be seen.
        let height = self.layout().height;
        if let Some(bar) = scrollbar::Bar::new(area, painting.scroll, height, painting.view_height)
        {
            let moved = (painting.scroll - painting.was).abs() > 0.01;
            scrollbar::paint(ui, &bar, painting.bar_name, painting.bar_active || moved);
        }
        folded
    }
}

/// What [`UnluminousApp::paint_the_editor`] draws over the text, gathered so the call is readable.
///
/// `task-1984` §3.6. A dozen values that the frame has already settled on by the time anything is
/// painted; one value rather than a dozen arguments, which is the shape `editor_view::PaintStyle`
/// beside it already has.
/// The three things [`UnluminousApp::take_the_editors_input`] needs beside the pointer and the keys.
///
/// `task-1984` §3.6. Whether this pane draws a caret, whether it is the focused one, and how wide the
/// text is — one value rather than three `bool` and `f32` arguments in a row, which is the kind of
/// call site a reader cannot check.
/// What handing the frame's input to the document left behind.
///
/// `task-1984` §3.6. Two answers the rest of the frame needs: whether the pane jumped somewhere,
/// which makes everything below it describe a file that is no longer showing, and whether the caret
/// has to be brought into view.
#[derive(Debug, Clone, Copy)]
struct EditorTyped {
    jumped: bool,
    scroll_to_caret: bool,
}

#[derive(Debug, Clone, Copy)]
struct EditorInput {
    /// Whether keys reach the document, which is whether this pane has the keyboard.
    has_keyboard: bool,
    /// Whether this is the pane the window considers current, which the popup and the Find bar follow.
    focused: bool,
    /// How wide the text is laid out, which an edit has to lay out again against.
    text_width: f32,
}

struct EditorPainting<'a> {
    /// Whether a caret is drawn, which is whether this pane has the keyboard.
    has_keyboard: bool,
    /// The word under the pointer with the modifier held, drawn underlined.
    underline: Option<std::ops::Range<usize>>,
    /// The paragraph the program is stopped on.
    execution_point: Option<usize>,
    /// The values to paint at the ends of the lines that bind them.
    inline_values: &'a [(usize, String)],
    /// Every match of the Find bar but the current one.
    find_matches: &'a [std::ops::Range<usize>],
    /// The brackets either side of the caret.
    bracket_pair: Option<(usize, usize)>,
    /// Which paragraphs could fold, and which of them are folded.
    fold_marks: &'a [(usize, bool)],
    /// Where the frame settled, and where it opened, which is what says the bar is moving.
    scroll: f32,
    was: f32,
    /// How tall the text area is, which is what the thumb's size is a share of.
    view_height: f32,
    /// The tab's name, which is the scrollbar's name in the accessibility tree.
    bar_name: &'a str,
    /// Whether the bar is being used, which is what it fades in for.
    bar_active: bool,
}
