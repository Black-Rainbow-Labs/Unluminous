//! A notebook tab inside the editing area: the room each cell is given, the keys of the two modes, and
//! everything drawn round the cells' text.
//!
//! The text of a notebook is drawn by `show_editor`, exactly as a file's is. This module is the four
//! places a notebook changes that function's behaviour, each called from it by name:
//!
//! 1. **Before the layout**, [`UnluminousApp::measure_the_notebook`] lays every output and every
//!    rendered Markdown cell out at the width of the pane, and records the room each cell needs. Then
//!    [`UnluminousApp::notebook_layout_inputs`] turns that room into `space_above`, `space_below`,
//!    `replaced` and hidden paragraphs on a copy of the document's paragraph styles
//!    (`tasks/task-2220-jupyter-notebooks-tdd.md` §3).
//! 2. **Before the editor reads the keyboard**, [`UnluminousApp::take_the_notebooks_keys`] takes the
//!    keys that mean something to a notebook, and in command mode all of them.
//! 3. **After it**, [`UnluminousApp::keep_the_caret_in_a_cell`] moves a caret that landed on a marker
//!    line off it, because a letter typed there would stop the line being a marker.
//! 4. **Around the painting**, [`UnluminousApp::paint_the_cells_behind`] and
//!    [`UnluminousApp::paint_the_cells_over`] draw the blocks, the status lines, the outputs and the
//!    buttons, measured against the same layout the text was drawn from.

use std::ops::Range;

use egui::{CornerRadius, Pos2, Rect, Stroke, Vec2};
use unluminous_core::{Hidden, Layout, ParagraphStyles, Preview};
use unluminous_jupyter::nbformat::CellKind;
use unluminous_jupyter::text::CellSpan;

use crate::app::actions::Action;
use crate::app::notebook::{self, Mode, NotebookTab};
use crate::app::notebook_actions::NotebookAction;
use crate::app::UnluminousApp;
use crate::components::notebook_view::{self, CellButton, DrawnKey, Metrics};
use crate::services::text_renderer::TextRenderer;
use crate::theme::color;

/// A Markdown cell rendered, at one width and size, kept between frames by cell id.
pub struct Rendered {
    pub source: String,
    pub width: f32,
    pub size: f32,
    pub preview: Preview,
    pub layout: Layout,
}

/// The room one cell is given round its text, worked out by [`UnluminousApp::measure_the_notebook`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Room {
    /// Above the cell's first line: the gap from the cell before, and the block's own padding.
    pub above: f32,
    /// Below the cell's last line: padding, the status line, the outputs, an input field, and after
    /// the last cell the add buttons.
    pub below: f32,
    /// How tall the cell is drawn in place of its source, when it is rendered or collapsed.
    pub replaced: Option<f32>,
    /// Whether the cell has a status line.
    pub status: bool,
    /// Whether the cell is a heading whose section is collapsed, with a line under it saying so.
    pub section_note: bool,
    /// How tall its outputs are shown, which is zero for none.
    pub outputs: f32,
}

/// What a cell is drawn as this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// Its source, in a block: a code cell, a raw cell, or a Markdown cell being edited.
    Source,
    /// Rendered Markdown in place of its source.
    Rendered,
    /// One line saying its source is collapsed.
    Collapsed,
    /// Not drawn at all, because it is inside a collapsed section.
    Hidden,
}

impl NotebookTab {
    /// What cell `cell` is drawn as.
    fn shape(&self, cell: usize) -> Shape {
        let Some(found) = self.model.cells.get(cell) else { return Shape::Source };
        if self.in_a_collapsed_section(cell) {
            return Shape::Hidden;
        }
        if self.collapsed.contains(&found.id) {
            return Shape::Collapsed;
        }
        match found.kind == CellKind::Markdown && !self.editing.contains(&found.id) {
            true => Shape::Rendered,
            false => Shape::Source,
        }
    }
}

/// Where one cell is on the page, in the layout's own coordinates.
#[derive(Debug, Clone, Copy)]
struct Place {
    /// The top of its block, which is a padding's height above its first line of text.
    top: f32,
    /// The bottom of its last line of text.
    text_bottom: f32,
    /// The bottom of everything belonging to it, outputs included.
    bottom: f32,
    /// The top of its first line of text.
    first_text: f32,
}

/// Where cell `span` is, or `None` when the layout does not hold it yet.
fn place_of(
    layout: &Layout,
    span: &CellSpan,
    shape: Shape,
    metrics: Metrics,
    last: bool,
) -> Option<Place> {
    if layout.lines.is_empty() || shape == Shape::Hidden {
        return None;
    }
    let first_at = layout.line_of_offset(span.body_bytes.start);
    let first = layout.lines.get(first_at)?;
    let last_line = match shape {
        Shape::Source => layout.lines.get(layout.line_of_offset(span.body_bytes.end))?,
        _ => first,
    };
    let pad = match shape {
        Shape::Source => metrics.pad(),
        _ => 0.0,
    };
    let tail = if last { metrics.tail() } else { 0.0 };
    let first_text = first.text_top();
    let text_bottom = last_line.text_top() + last_line.text_height();
    Some(Place {
        top: first_text - pad,
        text_bottom,
        bottom: last_line.bottom() - tail,
        first_text,
    })
}

impl UnluminousApp {
    /// The editor's font size for the tab that is showing, which everything in a notebook scales with.
    fn notebook_metrics(&self) -> Metrics {
        Metrics {
            size: self.files.active().sized_at.unwrap_or(self.settings.font_size),
            scroll: self.settings.notebook_scroll_outputs,
        }
    }

    /// Lay every output and every rendered Markdown cell out at `width`, and record the room each cell
    /// needs. Bumps the tab's bands revision when any cell's room changed, which is what lays the text
    /// out again.
    pub(crate) fn measure_the_notebook(&mut self, ctx: &egui::Context, width: f32) {
        let index = self.files.active_index();
        self.refresh_the_notebook(index);
        let metrics = self.notebook_metrics();
        let stale = self.render_the_stale_markdown(index, width, metrics);
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        tab.rendered.extend(stale);
        measure_the_cells(ctx, tab, width, metrics);
    }

    /// Render the Markdown cells of the tab at `index` whose rendering is missing or was made at
    /// another source, width or size. Answers each with its cell's id, for the tab to keep.
    fn render_the_stale_markdown(
        &self,
        index: usize,
        width: f32,
        metrics: Metrics,
    ) -> Vec<(String, Rendered)> {
        let Some(tab) = self.files.at(index).notebook.as_deref() else { return Vec::new() };
        let mut stale = Vec::new();
        for cell in 0..tab.len() {
            let Some(found) = tab.model.cells.get(cell) else { continue };
            if tab.shape(cell) != Shape::Rendered {
                continue;
            }
            let fresh = tab.rendered.get(&found.id).is_some_and(|kept| {
                kept.source == found.source
                    && (kept.width - width).abs() < 0.5
                    && kept.size == metrics.size
            });
            if !fresh {
                stale.push((
                    found.id.clone(),
                    self.render_a_markdown_cell(&found.source, width, metrics),
                ));
            }
        }
        stale
    }

    /// Render the source of one Markdown cell at `width`, and lay the result out.
    fn render_a_markdown_cell(&self, source: &str, width: f32, metrics: Metrics) -> Rendered {
        let shown = if source.trim().is_empty() {
            "*Empty Markdown cell. Double click to write in it.*"
        } else {
            source
        };
        let preview = self.render_markdown(shown, width, metrics.size);
        let layout = unluminous_core::layout(
            &preview.text,
            &preview.chars,
            &preview.paragraphs,
            &self.renderer,
            width,
        );
        Rendered { source: source.to_owned(), width, size: metrics.size, preview, layout }
    }

    /// The paragraph styles and hidden paragraphs a notebook tab is laid out with, or `None` for a tab
    /// that is not a notebook. `folds` is what the document's own folds hide, which is kept.
    pub(crate) fn notebook_layout_inputs(
        &mut self,
        index: usize,
        folds: &Hidden,
    ) -> Option<(ParagraphStyles, Hidden, Vec<Option<usize>>)> {
        let file = self.files.at(index);
        let tab = file.notebook.as_deref()?;
        let mut styles = file.document.paragraphs().clone();
        let mut hidden: Vec<Range<usize>> = folds.ranges().to_vec();
        let lines = file.document.text().len_lines();
        let mut numbers: Vec<Option<usize>> = (0..lines).map(Some).collect();
        for (cell, span) in tab.spans.iter().enumerate() {
            if let Some(marker) = span.marker {
                hidden.push(marker..marker + 1);
                numbers[marker] = None;
            }
            if tab.shape(cell) == Shape::Hidden {
                hidden.push(span.body.clone());
                span.body.clone().for_each(|line| numbers[line] = None);
                continue;
            }
            if span.body.is_empty() {
                continue;
            }
            for (counted, line) in span.body.clone().enumerate() {
                numbers[line] = Some(counted);
            }
            let room =
                tab.id_of(cell).and_then(|id| tab.rooms.get(&id).copied()).unwrap_or_default();
            apply_room(&mut styles, &mut hidden, &mut numbers, span, &room);
        }
        Some((styles, Hidden::of(hidden), numbers))
    }

    /// The bands revision of the tab at `index`, which is the layout's third key for a notebook.
    pub(crate) fn notebook_bands(&self, index: usize) -> Option<u64> {
        self.files.at(index).notebook.as_deref().map(|tab| tab.bands_revision)
    }
}

/// Record the room every cell of `tab` needs at `width`. Bumps the tab's bands revision when any
/// cell's room changed, which is what lays the text out again.
fn measure_the_cells(ctx: &egui::Context, tab: &mut NotebookTab, width: f32, metrics: Metrics) {
    let count = tab.len();
    let mut changed = false;
    for cell in 0..count {
        let Some(id) = tab.id_of(cell) else { continue };
        let room = room_of_a_cell(ctx, tab, cell, width, metrics);
        if tab.rooms.get(&id) != Some(&room) {
            tab.rooms.insert(id, room);
            changed = true;
        }
    }
    if changed {
        tab.bands_revision += 1;
    }
}

/// The room cell `cell` needs, gaps included, from what it is drawn as.
fn room_of_a_cell(
    ctx: &egui::Context,
    tab: &mut NotebookTab,
    cell: usize,
    width: f32,
    metrics: Metrics,
) -> Room {
    let count = tab.len();
    let shape = tab.shape(cell);
    let room = match (shape, tab.spans[cell].kind) {
        (Shape::Hidden, _) => Room::default(),
        (Shape::Rendered, _) => Room {
            section_note: tab.id_of(cell).is_some_and(|id| tab.sections_collapsed.contains(&id)),
            ..room_for_markdown(tab, cell, metrics)
        },
        (Shape::Collapsed, _) => Room { replaced: Some(metrics.status()), ..Room::default() },
        (Shape::Source, CellKind::Code) => room_for_code(ctx, tab, cell, width, metrics),
        (Shape::Source, _) => Room::default(),
    };
    match shape {
        Shape::Hidden => room,
        _ => with_gaps(room, shape, metrics, cell == 0, cell + 1 == count),
    }
}

/// The room a rendered Markdown cell needs: the height of its rendering, which
/// [`UnluminousApp::measure_the_notebook`] has already laid out.
fn room_for_markdown(tab: &NotebookTab, cell: usize, metrics: Metrics) -> Room {
    let id = tab.model.cells[cell].id.as_str();
    let height = tab.rendered.get(id).map(|kept| kept.layout.height).unwrap_or(0.0);
    Room { replaced: Some(height.max(metrics.status())), ..Room::default() }
}

/// The room a code cell needs: its status line, its outputs at `width`, and an input field when the
/// kernel is waiting on one.
fn room_for_code(
    ctx: &egui::Context,
    tab: &mut NotebookTab,
    cell: usize,
    width: f32,
    metrics: Metrics,
) -> Room {
    let found = &tab.model.cells[cell];
    let id = found.id.clone();
    let status = tab.runs.contains_key(&id) || found.execution_count.is_some();
    let outputs_width = (width - metrics.indent()).max(40.0);
    let key = DrawnKey {
        revision: tab.output_revisions.get(&id).copied().unwrap_or(0),
        width: outputs_width,
        size: metrics.size,
        traceback_open: tab.tracebacks_open.contains(&id),
        sort: tab.sorts.get(&id).copied(),
    };
    let fresh = tab.drawn.get(&id).is_some_and(|drawn| drawn.key == key);
    if !fresh {
        let drawn = notebook_view::lay_out_outputs(ctx, found, key, metrics);
        tab.drawn.insert(id.clone(), drawn);
    }
    let mut outputs = match (found.outputs.is_empty(), tab.outputs_collapsed.contains(&id)) {
        (true, _) => 0.0,
        (false, true) => metrics.status(),
        (false, false) => tab
            .drawn
            .get(&id)
            .map(|drawn| notebook_view::shown_height(drawn, metrics))
            .unwrap_or(0.0),
    };
    if tab.waiting.as_ref().is_some_and(|waiting| waiting.cell == id) {
        outputs += metrics.status() + 8.0;
    }
    Room { status, outputs, ..Room::default() }
}

/// Add the gaps every cell has: the space from the cell before, a code block's padding, the status
/// line and the outputs under it, and the add buttons after the last cell.
fn with_gaps(room: Room, shape: Shape, metrics: Metrics, first: bool, last: bool) -> Room {
    let gap = if first { metrics.gap() / 2.0 } else { metrics.gap() };
    let pad = if shape == Shape::Source { metrics.pad() } else { 0.0 };
    let status = if room.status { metrics.status() } else { 0.0 };
    let note = if room.section_note { metrics.status() } else { 0.0 };
    let outputs = if room.outputs > 0.0 { room.outputs + metrics.pad() } else { 0.0 };
    let tail = if last { metrics.tail() } else { 0.0 };
    Room { above: gap + pad, below: pad + note + status + outputs + tail, ..room }
}

/// Write one cell's room into the copy of the paragraph styles a notebook is laid out with.
fn apply_room(
    styles: &mut ParagraphStyles,
    hidden: &mut Vec<Range<usize>>,
    numbers: &mut [Option<usize>],
    span: &CellSpan,
    room: &Room,
) {
    let first = span.body.start;
    let last = span.body.end - 1;
    styles.set(first..first + 1, |style| style.space_above = room.above);
    match room.replaced {
        Some(height) => {
            styles.set(first..first + 1, |style| {
                style.replaced = true;
                style.min_height = height;
                style.space_below = room.below;
            });
            if last > first {
                hidden.push(first + 1..last + 1);
            }
            for line in span.body.clone() {
                numbers[line] = None;
            }
        }
        None => styles.set(last..last + 1, |style| style.space_below = room.below),
    }
}

impl UnluminousApp {
    /// Take the keys that mean something to the notebook that is showing, before the editor reads the
    /// keyboard. Answers whether the editor should still take the keys, which in command mode it never
    /// does: a letter there is a command, and must not be typed into a cell.
    pub(crate) fn take_the_notebooks_keys(
        &mut self,
        ui: &mut egui::Ui,
        has_keyboard: bool,
    ) -> bool {
        let index = self.files.active_index();
        let Some(mode) = self.files.active().notebook.as_deref().map(|tab| tab.mode) else {
            return has_keyboard;
        };
        if !has_keyboard
            || crate::app::text_box_has_the_keyboard(ui.ctx())
            || self.completion.is_some()
        {
            return has_keyboard;
        }
        let now = ui.input(|input| input.time);
        if mode == Mode::Edit {
            self.keep_a_key_from_joining_a_cell_to_its_marker(ui);
            if self.take_a_tab_that_completes(ui) {
                self.complete_word();
                return has_keyboard;
            }
        }
        let presses = take_notebook_presses(ui, mode, self.find.is_some());
        for press in presses {
            if let Some(what) = self.notebook_key(index, press, now) {
                self.notebook_wanted = Some(Action::Notebook(what));
            }
        }
        let mode = self.files.active().notebook.as_deref().map(|tab| tab.mode).unwrap_or_default();
        mode == Mode::Edit && self.notebook_wanted.is_none()
    }

    /// Take a bare Tab out of the frame's input when it means *complete* rather than *indent*, which is
    /// Jupyter's own rule: in a code cell, with nothing selected, straight after a letter, a digit, an
    /// `_`, a `.` or a `:`. Anywhere else Tab still indents. Answers whether one was taken. `task-2229`.
    fn take_a_tab_that_completes(&mut self, ui: &mut egui::Ui) -> bool {
        let bare_tab = |event: &egui::Event| matches!(event, egui::Event::Key { key: egui::Key::Tab, pressed: true, modifiers, .. } if modifiers.is_none());
        if !ui.input(|input| input.events.iter().any(bare_tab)) {
            return false;
        }
        let file = self.files.active();
        let Some(tab) = file.notebook.as_deref() else { return false };
        let selection = file.document.selection();
        let Some(cell) = tab.cell_at_offset(selection.head) else { return false };
        let span = &tab.spans[cell];
        let in_code = span.kind == CellKind::Code
            && selection.head > span.body_bytes.start
            && selection.head <= span.body_bytes.end;
        if !selection.is_empty() || !in_code {
            return false;
        }
        // From the cell's start, which is always a character boundary, where four bytes back might not be.
        let before =
            file.document.text().byte_slice(span.body_bytes.start..selection.head).to_string();
        let completes = before.chars().last().is_some_and(|last| {
            last.is_alphanumeric() || matches!(last, '_' | '.' | ':' | '%' | '!')
        });
        if completes {
            ui.input_mut(|input| input.events.retain(|event| !bare_tab(event)));
        }
        completes
    }

    /// Take Backspace at a cell's first character and Delete at its last out of the frame's input.
    ///
    /// Either would take the line break between the cell and the marker next to it, which joins the
    /// cell's text onto the marker line and so stops that line being a marker. The reference editor does
    /// nothing at those two places, and neither does this.
    fn keep_a_key_from_joining_a_cell_to_its_marker(&mut self, ui: &mut egui::Ui) {
        let file = self.files.active();
        let Some(tab) = file.notebook.as_deref() else { return };
        let selection = file.document.selection();
        if !selection.is_empty() {
            return;
        }
        let Some(cell) = tab.cell_at_offset(selection.head) else { return };
        let span = &tab.spans[cell];
        let at_start = selection.head == span.body_bytes.start && span.marker.is_some();
        let at_end = selection.head == span.body_bytes.end && cell + 1 < tab.len();
        ui.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => !at_start,
                egui::Event::Key { key: egui::Key::Delete, pressed: true, .. } => !at_end,
                _ => true,
            })
        });
    }

    /// What one key press means, given the mode and the key pressed before it.
    fn notebook_key(&mut self, index: usize, press: Press, now: f64) -> Option<NotebookAction> {
        use NotebookAction as Do;
        if let Some(what) = press.both_modes() {
            return Some(what);
        }
        let tab = self.files.at_mut(index).notebook.as_deref_mut()?;
        let first = tab.first_key.take();
        let twice = |key: egui::Key| {
            first.is_some_and(|(was, at)| was == key && now - at < notebook::SECOND_KEY)
        };
        match press {
            Press::Edit(what) => Some(what),
            Press::Command(egui::Key::D) if twice(egui::Key::D) => Some(Do::Delete),
            Press::Command(egui::Key::I) if twice(egui::Key::I) => Some(Do::Interrupt),
            Press::Command(egui::Key::Num0) if twice(egui::Key::Num0) => Some(Do::Restart),
            Press::Command(key @ (egui::Key::D | egui::Key::I | egui::Key::Num0)) => {
                tab.first_key = Some((key, now));
                None
            }
            Press::Command(key) => command_key(key),
            Press::CommandShift(key) => command_shift_key(key),
            Press::Both(_) => None,
        }
    }

    /// Move a caret that landed on a marker line, or in a rendered Markdown cell, to where a person
    /// meant it to go. `before` is where the caret was before the editor read the keyboard.
    pub(crate) fn keep_the_caret_in_a_cell(&mut self, before: usize, pressed: bool) {
        let file = self.files.active_mut();
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        if pressed {
            tab.mode = Mode::Edit;
        }
        let selection = file.document.selection();
        let head = selection.head;
        let Some(cell) = tab.cell_at_offset(head) else { return };
        let span = &tab.spans[cell];
        if span.marker.is_some() && head < span.body_bytes.start {
            // On the marker: forwards into this cell's text, backwards to the end of the cell above.
            let target = match (head < before, cell) {
                (true, cell) if cell > 0 => notebook::span_end(&tab.spans[cell - 1]),
                _ => span.body_bytes.start,
            };
            let extend = !selection.is_empty();
            file.document.apply(unluminous_core::Command::PlaceCaret { offset: target, extend });
        }
        if let (Some(id), Some(found)) = (tab.id_of(cell), tab.model.cells.get(cell)) {
            if found.kind == CellKind::Markdown
                && !tab.editing.contains(&id)
                && tab.mode == Mode::Edit
                && head != before
            {
                tab.editing.insert(id);
                tab.bands_revision += 1;
            }
        }
    }

    /// Fill each cell's block, and mark the chosen cells, before the text is painted over them.
    pub(crate) fn paint_the_cells_behind(&mut self, ui: &egui::Ui, origin: Pos2, area: Rect) {
        let metrics = self.notebook_metrics();
        let caret = self.document().selection().head;
        let has_keys = self.focus == crate::app::Focus::Editor;
        let Some(tab) = self.files.active().notebook.as_deref() else { return };
        let layout = &self.files.active().cached.layout;
        let chosen = tab.chosen(caret);
        let painter = ui.painter();
        let count = tab.len();
        for (cell, span) in tab.spans.iter().enumerate() {
            let shape = tab.shape(cell);
            let room =
                tab.id_of(cell).and_then(|id| tab.rooms.get(&id).copied()).unwrap_or_default();
            let Some(place) = place_of(layout, span, shape, metrics, cell + 1 == count) else {
                continue;
            };
            let block = Rect::from_min_max(
                Pos2::new(area.left() + 6.0, origin.y + place.top),
                Pos2::new(
                    area.right() - 10.0,
                    origin.y
                        + place.text_bottom
                        + metrics.pad()
                        + if room.status { metrics.status() } else { 0.0 },
                ),
            );
            if block.bottom() < area.top() || block.top() > area.bottom() {
                continue;
            }
            paint_a_block(painter, block, span.kind, shape);
            if chosen.contains(&cell) {
                let whole = Rect::from_min_max(
                    block.min,
                    Pos2::new(block.right(), (origin.y + place.bottom).max(block.bottom())),
                );
                paint_the_choice(painter, block, whole, tab.mode, has_keys);
            }
        }
    }
}

/// A cell's ground: code and raw cells sit in a block a step lighter than the page, which is how
/// The reference editor draws a code cell. A rendered Markdown cell has no block and reads as part of the page.
fn paint_a_block(painter: &egui::Painter, block: Rect, kind: CellKind, shape: Shape) {
    match (kind, shape) {
        (_, Shape::Rendered) => {}
        (CellKind::Raw, _) => {
            painter.rect_stroke(
                block,
                CornerRadius::same(4),
                Stroke::new(1.0, color::divider()),
                egui::StrokeKind::Inside,
            );
        }
        _ => {
            painter.rect_filled(block, CornerRadius::same(4), color::code_panel());
        }
    }
}

/// The chosen cells: a bar down the left of the whole cell, outputs included, and a thin frame round
/// its block. The bar is solid in command mode and lighter while typing, so the two modes can be told
/// apart at a glance.
fn paint_the_choice(painter: &egui::Painter, block: Rect, whole: Rect, mode: Mode, has_keys: bool) {
    let accent = if has_keys { color::accent() } else { color::text_faint() };
    let bar_tint = match mode {
        Mode::Command { .. } => accent,
        Mode::Edit => accent.gamma_multiply(0.55),
    };
    let bar = Rect::from_min_max(
        Pos2::new(whole.left() - 5.0, whole.top()),
        Pos2::new(whole.left() - 2.0, whole.bottom()),
    );
    painter.rect_filled(bar, CornerRadius::same(1), bar_tint);
    painter.rect_stroke(
        block,
        CornerRadius::same(4),
        Stroke::new(1.0, accent.gamma_multiply(0.8)),
        egui::StrokeKind::Inside,
    );
}

/// One key press a notebook reads.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Press {
    /// A chord that means the same thing in both modes.
    Both(NotebookAction),
    /// A chord that is only a notebook's in edit mode.
    Edit(NotebookAction),
    /// A key in command mode with no modifier, or with the command key.
    Command(egui::Key),
    /// A key in command mode with shift.
    CommandShift(egui::Key),
}

impl Press {
    /// The action this press stands for when it means the same in both modes, and `None` for a
    /// press that depends on the mode.
    fn both_modes(&self) -> Option<NotebookAction> {
        match self {
            Press::Both(what) => Some(*what),
            _ => None,
        }
    }
}

/// Take the frame's key presses that a notebook reads out of its input, leaving the rest. In command
/// mode every key press and every typed character is taken, because none of them is typing.
fn take_notebook_presses(ui: &mut egui::Ui, mode: Mode, finding: bool) -> Vec<Press> {
    let mut presses = Vec::new();
    let command = matches!(mode, Mode::Command { .. });
    ui.input_mut(|input| {
        input.events.retain(|event| match event {
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                match read_press(*key, *modifiers, command, finding) {
                    Some(press) => {
                        presses.push(press);
                        false
                    }
                    None => !command || keep_in_command_mode(*key, *modifiers),
                }
            }
            egui::Event::Text(_) => !command,
            egui::Event::Copy if command => {
                presses.push(Press::Edit(NotebookAction::Copy));
                false
            }
            egui::Event::Cut if command => {
                presses.push(Press::Edit(NotebookAction::Cut));
                false
            }
            egui::Event::Paste(_) if command => {
                presses.push(Press::Edit(NotebookAction::PasteBelow));
                false
            }
            _ => true,
        });
    });
    presses
}

/// Keys command mode leaves for the rest of the window: saving, closing, the menus' own chords.
fn keep_in_command_mode(_key: egui::Key, modifiers: egui::Modifiers) -> bool {
    modifiers.command && !modifiers.shift && !modifiers.alt
}

/// Which notebook press a key is, if it is one.
fn read_press(
    key: egui::Key,
    modifiers: egui::Modifiers,
    command: bool,
    finding: bool,
) -> Option<Press> {
    use NotebookAction as Do;
    let (ctrl, shift, alt) = (modifiers.command, modifiers.shift, modifiers.alt);
    if ctrl && shift && !alt {
        match key {
            egui::Key::ArrowUp => return Some(Press::Both(Do::MoveUp)),
            egui::Key::ArrowDown => return Some(Press::Both(Do::MoveDown)),
            _ => {}
        }
    }
    if key == egui::Key::A && ctrl && !shift && !alt {
        return Some(Press::Both(Do::SelectCell));
    }
    if key == egui::Key::Enter {
        return match (ctrl, shift, alt) {
            (true, false, false) => Some(Press::Both(Do::RunCell)),
            (false, true, false) => Some(Press::Both(Do::RunCellSelectBelow)),
            (false, false, true) => Some(Press::Both(Do::RunCellInsertBelow)),
            (true, true, true) => Some(Press::Both(Do::RunAll)),
            (false, true, true) => Some(Press::Both(Do::DebugCell)),
            (false, false, false) if command => Some(Press::Both(Do::EditMode)),
            _ => None,
        };
    }
    if !command {
        return read_edit_press(key, ctrl, shift, alt, finding);
    }
    match (ctrl, shift, alt) {
        (false, false, false) => Some(Press::Command(key)),
        (false, true, false) => Some(Press::CommandShift(key)),
        (false, false, true) if matches!(key, egui::Key::ArrowUp | egui::Key::ArrowDown) => {
            Some(Press::CommandShift(key))
        }
        (true, false, true) if key == egui::Key::ArrowUp => Some(Press::Both(Do::PreviousSection)),
        (true, false, true) if key == egui::Key::ArrowDown => Some(Press::Both(Do::NextSection)),
        (true, false, false) if key == egui::Key::Home => Some(Press::Both(Do::SelectFirst)),
        (true, false, false) if key == egui::Key::End => Some(Press::Both(Do::SelectLast)),
        (true, false, false) if key == egui::Key::Slash => Some(Press::Both(Do::CommentCells)),
        _ => None,
    }
}

/// The chords a notebook takes in edit mode. Everything else is the editor's.
fn read_edit_press(
    key: egui::Key,
    ctrl: bool,
    shift: bool,
    alt: bool,
    finding: bool,
) -> Option<Press> {
    use NotebookAction as Do;
    match (key, ctrl, shift, alt) {
        // Escape belongs to the Find bar while it is open.
        (egui::Key::Escape, false, false, false) if !finding => Some(Press::Edit(Do::CommandMode)),
        (egui::Key::A, false, true, true) => Some(Press::Edit(Do::AddAbove(CellKind::Code))),
        (egui::Key::B, false, true, true) => Some(Press::Edit(Do::AddBelow(CellKind::Code))),
        (egui::Key::Minus, true, true, false) => Some(Press::Edit(Do::Split)),
        (egui::Key::Home, true, false, false) => Some(Press::Edit(Do::CellStart)),
        (egui::Key::End, true, false, false) => Some(Press::Edit(Do::CellEnd)),
        _ => None,
    }
}

/// A key with no modifier in command mode.
fn command_key(key: egui::Key) -> Option<NotebookAction> {
    use NotebookAction as Do;
    Some(match key {
        egui::Key::ArrowUp | egui::Key::K => Do::SelectAbove,
        egui::Key::ArrowDown | egui::Key::J => Do::SelectBelow,
        egui::Key::A => Do::AddAbove(CellKind::Code),
        egui::Key::B => Do::AddBelow(CellKind::Code),
        egui::Key::M => Do::Convert(CellKind::Markdown),
        egui::Key::Y => Do::Convert(CellKind::Code),
        egui::Key::R => Do::Convert(CellKind::Raw),
        egui::Key::C => Do::Copy,
        egui::Key::X => Do::Cut,
        egui::Key::V => Do::PasteBelow,
        egui::Key::Delete => Do::Delete,
        egui::Key::Z => Do::UndoDelete,
        egui::Key::O => Do::CollapseOutput,
        egui::Key::L => Do::ToggleLineNumbers,
        egui::Key::Escape => Do::CommandMode,
        _ => return None,
    })
}

/// A key with shift in command mode, or Alt and an arrow, which also extend the choice.
fn command_shift_key(key: egui::Key) -> Option<NotebookAction> {
    use NotebookAction as Do;
    Some(match key {
        egui::Key::ArrowUp | egui::Key::K => Do::ExtendAbove,
        egui::Key::ArrowDown | egui::Key::J => Do::ExtendBelow,
        egui::Key::V => Do::PasteAbove,
        egui::Key::M => Do::MergeSelected,
        _ => return None,
    })
}

impl UnluminousApp {
    /// Draw everything that goes round the cells' text, after the text: status lines, outputs,
    /// rendered Markdown, the buttons on the cell under the pointer, and the add buttons.
    ///
    /// The tab stays in its file while this runs. What the painting finds out is gathered in an
    /// [`Asked`], and acted on once the painting has finished.
    pub(crate) fn paint_the_cells_over(
        &mut self,
        ui: &mut egui::Ui,
        origin: Pos2,
        area: Rect,
        gutter: Rect,
    ) {
        let frame = Frame { origin, area, gutter, metrics: self.notebook_metrics() };
        let caret = self.document().selection().head;
        let index = self.files.active_index();
        let file = self.files.active_mut();
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let asked = paint_the_cells(ui, &self.renderer, &file.cached.layout, tab, &frame, caret);
        self.act_on_what_the_cells_asked(index, asked);
    }

    /// Act on what a press on the cells asked for, once the painting is done.
    fn act_on_what_the_cells_asked(&mut self, index: usize, asked: Asked) {
        if asked.changed {
            self.files.active_mut().document.note_a_change_outside_the_text();
        }
        if let Some(cell) = asked.select {
            self.choose_a_cell(index, cell, false);
            self.focus = crate::app::Focus::Editor;
        }
        if let Some(cell) = asked.edit_markdown {
            self.choose_a_cell(index, cell, true);
            self.focus = crate::app::Focus::Editor;
        }
        if let Some((cell, what)) = asked.wanted {
            self.choose_a_cell(index, cell, false);
            self.focus = crate::app::Focus::Editor;
            self.notebook_wanted = Some(Action::Notebook(what));
        }
        if let Some((cell, at)) = asked.menu_at {
            self.choose_a_cell(index, cell, false);
            self.notebook_menu = Some(at);
        }
        if let Some(html) = asked.open_html {
            self.open_an_output_in_a_browser_tab(&html);
        }
        self.notebook_input = asked.input.or(self.notebook_input.take());
        if let Some((cell, gap)) = asked.moved {
            self.move_cells_into(index, cell..cell + 1, gap);
        }
    }
}

/// Where a frame of a notebook is drawn: the origin of the text, the area on the screen, the gutter
/// beside it, and the notebook's measurements at the editor's font size.
#[derive(Clone, Copy)]
struct Frame {
    origin: Pos2,
    area: Rect,
    gutter: Rect,
    metrics: Metrics,
}

/// What the cells asked for while they were painted, which is acted on afterwards.
#[derive(Debug, Default)]
struct Asked {
    /// An action to run on a cell, from one of its buttons.
    wanted: Option<(usize, NotebookAction)>,
    /// A rendered Markdown cell that was double clicked, to be edited.
    edit_markdown: Option<usize>,
    /// A cell that was pressed on, to be chosen.
    select: Option<usize>,
    /// A cell whose menu was asked for, and where to put the menu.
    menu_at: Option<(usize, Pos2)>,
    /// Some HTML an output asked to open in a browser tab.
    open_html: Option<String>,
    /// What was typed into an `input()` field and sent.
    input: Option<String>,
    /// A cell dropped after being dragged, and the gap it goes into.
    moved: Option<(usize, usize)>,
    /// Something saved in the file changed, such as a cell's tags.
    changed: bool,
}

impl Asked {
    /// Note what the painting of cell `cell` found out.
    fn take_in(&mut self, painted: Painted, cell: usize) {
        if painted.pressed {
            self.select = Some(cell);
        }
        if painted.edit_markdown {
            self.edit_markdown = Some(cell);
        }
        self.open_html = painted.open_html.or(self.open_html.take());
        self.changed |= painted.changed;
        if painted.input.is_some() {
            self.input = painted.input;
        }
    }
}

/// One cell that is on the screen this frame: what it is, how it is drawn, and where.
struct CellDrawing {
    cell: usize,
    id: String,
    kind: CellKind,
    shape: Shape,
    room: Room,
    place: Place,
    /// The top and bottom of everything belonging to the cell, in the window.
    top: f32,
    bottom: f32,
    /// Whether it is the last cell, which has the add buttons after it.
    last: bool,
}

/// What is known while the cells of one frame are being painted, one after another.
struct Pass {
    /// The cells the keys act on.
    chosen: Range<usize>,
    pointer: Option<Pos2>,
    hovered: Option<usize>,
    /// Each painted cell's number, top and bottom, for finding where a dragged cell goes.
    tops: Vec<(usize, f32, f32)>,
    /// The cell being dragged, the pointer's height, and whether it was let go this frame.
    dragged: Option<(usize, f32, bool)>,
    asked: Asked,
}

/// Paint every cell of `tab` that is on the screen, and answer what they asked for. `caret` is the
/// editor's caret, which says which cell is chosen in edit mode.
fn paint_the_cells(
    ui: &mut egui::Ui,
    renderer: &TextRenderer,
    layout: &Layout,
    tab: &mut NotebookTab,
    frame: &Frame,
    caret: usize,
) -> Asked {
    let mut pass = Pass {
        chosen: tab.chosen(caret),
        pointer: ui.input(|input| input.pointer.hover_pos()),
        hovered: None,
        tops: Vec::new(),
        dragged: None,
        asked: Asked::default(),
    };
    for cell in 0..tab.len() {
        if let Some(drawing) = locate_a_cell(tab, layout, cell, frame) {
            paint_a_cell(ui, renderer, tab, &drawing, frame, &mut pass);
        }
    }
    tab.hovered = pass.hovered;
    pass.asked.moved = drop_a_dragged_cell(ui, &pass, frame.area, tab.len());
    pass.asked
}

/// Where cell `cell` is drawn this frame, or `None` when it is hidden, is not in the layout yet, or is
/// wholly off the screen.
fn locate_a_cell(
    tab: &NotebookTab,
    layout: &Layout,
    cell: usize,
    frame: &Frame,
) -> Option<CellDrawing> {
    let shape = tab.shape(cell);
    let span = &tab.spans[cell];
    let id = tab.id_of(cell)?;
    let room = tab.rooms.get(&id).copied().unwrap_or_default();
    let last = cell + 1 == tab.len();
    let place = place_of(layout, span, shape, frame.metrics, last)?;
    let top = frame.origin.y + place.top - room.above + frame.metrics.pad();
    let bottom = frame.origin.y + place.bottom;
    if bottom < frame.area.top() || top > frame.area.bottom() {
        return None;
    }
    Some(CellDrawing { cell, id, kind: span.kind, shape, room, place, top, bottom, last })
}

/// Paint one cell and the controls that belong to it, and note in `pass` what they asked for.
fn paint_a_cell(
    ui: &mut egui::Ui,
    renderer: &TextRenderer,
    tab: &mut NotebookTab,
    drawing: &CellDrawing,
    frame: &Frame,
    pass: &mut Pass,
) {
    let cell = drawing.cell;
    let whole = Rect::from_min_max(
        Pos2::new(frame.area.left(), drawing.top),
        Pos2::new(frame.area.right(), drawing.bottom),
    );
    if pass.pointer.is_some_and(|at| whole.contains(at)) {
        pass.hovered = Some(cell);
    }
    pass.tops.push((cell, drawing.top, drawing.bottom));
    if pass.hovered == Some(cell) || tab.dragging.as_deref() == Some(drawing.id.as_str()) {
        let handle = Pos2::new(frame.gutter.right() - 6.0, (drawing.top + drawing.bottom) / 2.0);
        if let Some((y, released)) = drag_handle(ui, tab, &drawing.id, handle, cell) {
            pass.dragged = Some((cell, y, released));
        }
    }
    let painted = paint_one_cell(ui, renderer, tab, drawing, frame);
    pass.asked.take_in(painted, cell);
    let debuggable = crate::app::notebook_debug::can_be_debugged(&tab.model.metadata);
    paint_the_cell_buttons(ui, drawing, frame, debuggable, pass);
    paint_the_run_button(ui, drawing, frame, pass);
    paint_the_add_buttons(ui, drawing, frame, &mut pass.asked);
}

/// The buttons over a cell's top right corner, on the cell under the pointer and on the one chosen cell.
fn paint_the_cell_buttons(
    ui: &mut egui::Ui,
    drawing: &CellDrawing,
    frame: &Frame,
    debuggable: bool,
    pass: &mut Pass,
) {
    let cell = drawing.cell;
    let alone = pass.chosen.contains(&cell) && pass.chosen.len() == 1;
    if pass.hovered != Some(cell) && !alone {
        return;
    }
    let corner = Pos2::new(frame.area.right() - 14.0, frame.origin.y + drawing.place.top - 14.0);
    let rendered = drawing.shape == Shape::Rendered;
    let salt = format!("cell {}", cell + 1);
    let Some(button) =
        notebook_view::cell_buttons(ui, corner, drawing.kind, rendered, debuggable, &salt)
    else {
        return;
    };
    match button {
        CellButton::More => {
            pass.asked.menu_at = Some((cell, Pos2::new(corner.x - 160.0, corner.y + 26.0)))
        }
        other => pass.asked.wanted = button_action(other).map(|what| (cell, what)),
    }
}

/// The run button in the gutter beside a code cell that is under the pointer or chosen.
fn paint_the_run_button(ui: &mut egui::Ui, drawing: &CellDrawing, frame: &Frame, pass: &mut Pass) {
    let cell = drawing.cell;
    if drawing.kind != CellKind::Code
        || !(pass.hovered == Some(cell) || pass.chosen.contains(&cell))
    {
        return;
    }
    let at = Pos2::new(
        frame.gutter.left() + 10.0,
        frame.origin.y + drawing.place.first_text + frame.metrics.size * 0.6,
    );
    let place = Rect::from_center_size(at, Vec2::splat(18.0));
    let name = format!("Run cell {}", cell + 1);
    if frame.gutter.width() > 12.0
        && crate::components::controls::icon_button(ui, place, &name, crate::theme::icon::run)
    {
        pass.asked.wanted = Some((cell, NotebookAction::RunCell));
    }
}

/// The buttons that add a cell, under the last cell.
fn paint_the_add_buttons(
    ui: &mut egui::Ui,
    drawing: &CellDrawing,
    frame: &Frame,
    asked: &mut Asked,
) {
    if !drawing.last {
        return;
    }
    let centre = Pos2::new(
        frame.area.center().x,
        frame.origin.y + drawing.place.bottom + frame.metrics.tail() / 2.0,
    );
    if let Some(CellButton::Add(kind)) =
        notebook_view::add_buttons(ui, centre, "after the last cell")
    {
        asked.wanted = Some((drawing.cell, NotebookAction::AddBelow(kind)));
    }
}

/// The line where a dragged cell would drop, and the cell and gap when it was let go this frame.
fn drop_a_dragged_cell(
    ui: &egui::Ui,
    pass: &Pass,
    area: Rect,
    count: usize,
) -> Option<(usize, usize)> {
    let (cell, y, released) = pass.dragged?;
    let gap = gap_at(&pass.tops, y, count);
    paint_the_drop_line(ui, &pass.tops, gap, area);
    released.then_some((cell, gap))
}

/// The handle a cell is dragged by: six dots at `centre`, in the gutter beside the cell. Answers the
/// pointer's height while it is being dragged, and whether it was let go this frame.
fn drag_handle(
    ui: &mut egui::Ui,
    tab: &mut NotebookTab,
    id: &str,
    centre: Pos2,
    cell: usize,
) -> Option<(f32, bool)> {
    let rect = Rect::from_center_size(centre, Vec2::new(10.0, 18.0));
    let response = ui
        .interact(rect, ui.id().with(("drag-cell", id)), egui::Sense::drag())
        .on_hover_cursor(egui::CursorIcon::Grab);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Drag cell {}", cell + 1))
    });
    let tint =
        if response.hovered() || response.dragged() { color::text() } else { color::text_faint() };
    for row in [-5.0, 0.0, 5.0] {
        for column in [-2.0, 2.0] {
            ui.painter().circle_filled(centre + Vec2::new(column, row), 1.2, tint);
        }
    }
    if response.drag_started() {
        tab.dragging = Some(id.to_owned());
    }
    let pointer = response.interact_pointer_pos().map(|at| at.y);
    if response.drag_stopped() {
        tab.dragging = None;
        return Some((pointer.unwrap_or(centre.y), true));
    }
    response.dragged().then(|| (pointer.unwrap_or(centre.y), false))
}

/// The gap a cell dropped at height `y` goes into: before the first cell whose middle is below it, or
/// after the last. `tops` is each drawn cell's number, top and bottom.
fn gap_at(tops: &[(usize, f32, f32)], y: f32, count: usize) -> usize {
    tops.iter()
        .find(|(_, top, bottom)| y < (top + bottom) / 2.0)
        .map(|(cell, _, _)| *cell)
        .unwrap_or(count)
}

/// The line across the page where a dragged cell would go.
fn paint_the_drop_line(ui: &egui::Ui, tops: &[(usize, f32, f32)], gap: usize, area: Rect) {
    let y = match tops.iter().find(|(cell, _, _)| *cell == gap) {
        Some((_, top, _)) => *top,
        None => tops.last().map(|(_, _, bottom)| *bottom).unwrap_or(area.top()),
    };
    let line = Rect::from_min_max(
        Pos2::new(area.left() + 6.0, y - 1.0),
        Pos2::new(area.right() - 10.0, y + 1.0),
    );
    ui.painter().rect_filled(line, CornerRadius::same(1), color::accent());
}

/// A cell's tags as small labels at the right of its first line, or the field they are being edited
/// in. Answers whether the tags changed.
fn paint_the_tags(
    ui: &mut egui::Ui,
    tab: &mut NotebookTab,
    cell: usize,
    id: &str,
    row: Rect,
    metrics: Metrics,
) -> bool {
    if tab.editing_tags.as_ref().is_some_and(|(editing, _)| editing == id) {
        return edit_the_tags(ui, tab, cell, row, metrics);
    }
    let tags = tab.model.cells.get(cell).map(|found| found.tags()).unwrap_or_default();
    let mut right = row.right();
    for tag in tags.iter().rev() {
        let wide = crate::components::controls::chip(
            ui.painter(),
            Pos2::new(right, row.center().y),
            false,
            tag,
            metrics.size * 0.72,
            color::text_dim(),
            crate::components::controls::ChipFill::Tint(color::control()),
            Vec2::new(12.0, 4.0),
        );
        right -= wide + 4.0;
    }
    false
}

/// The field a cell's tags are typed into, separated by commas. Enter or clicking away keeps them,
/// Escape leaves them as they were. Answers whether the tags changed.
fn edit_the_tags(
    ui: &mut egui::Ui,
    tab: &mut NotebookTab,
    cell: usize,
    row: Rect,
    metrics: Metrics,
) -> bool {
    let Some((_, typed)) = tab.editing_tags.as_mut() else { return false };
    let left = (row.right() - 280.0).max(row.left());
    let rect = Rect::from_min_max(Pos2::new(left, row.top()), row.right_bottom());
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let edit = egui::TextEdit::singleline(typed)
        .hint_text(crate::components::controls::placeholder(
            "Tags, separated by commas",
            &metrics.output_font(),
            color::text_faint(),
        ))
        .desired_width(rect.width())
        .font(metrics.output_font());
    let response = child.add(edit);
    if !response.has_focus() && !response.lost_focus() {
        response.request_focus();
    }
    if !response.lost_focus() {
        return false;
    }
    let kept = !ui.input(|input| input.key_pressed(egui::Key::Escape));
    let typed = tab.editing_tags.take().map(|(_, typed)| typed).unwrap_or_default();
    let Some(found) = tab.model.cells.get_mut(cell) else { return false };
    let tags = notebook::tags_typed(&typed);
    if !kept || tags == found.tags() {
        return false;
    }
    found.set_tags(&tags);
    true
}

/// The line under a collapsed heading saying how many cells it holds. Clicking it opens the section.
fn paint_a_section_note(
    ui: &mut egui::Ui,
    tab: &mut NotebookTab,
    cell: usize,
    id: &str,
    at: Pos2,
    width: f32,
    metrics: Metrics,
) {
    let hidden = tab.hidden_by(cell);
    let rect = Rect::from_min_size(at, Vec2::new(width, metrics.status()));
    let words =
        format!("\u{25B8} {hidden} cells in this section are collapsed. Click to show them.");
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        words,
        metrics.output_font(),
        color::text_dim(),
    );
    let response = ui.interact(rect, ui.id().with(("section-collapsed", id)), egui::Sense::click());
    if response.clicked() {
        tab.sections_collapsed.remove(id);
        tab.bands_revision += 1;
    }
}

/// What a press on one cell's own drawing asked for.
#[derive(Debug, Default)]
struct Painted {
    pressed: bool,
    edit_markdown: bool,
    open_html: Option<String>,
    /// Something saved in the file changed, such as the cell's tags.
    changed: bool,
    /// What was typed into the cell's `input()` field and sent.
    input: Option<String>,
}

/// What a button on a cell does.
fn button_action(button: CellButton) -> Option<NotebookAction> {
    Some(match button {
        CellButton::Run => NotebookAction::RunCell,
        CellButton::Debug => NotebookAction::DebugCell,
        CellButton::MoveUp => NotebookAction::MoveUp,
        CellButton::MoveDown => NotebookAction::MoveDown,
        CellButton::Delete => NotebookAction::Delete,
        CellButton::ToggleMarkdown => NotebookAction::RenderMarkdown,
        CellButton::Add(kind) => NotebookAction::AddBelow(kind),
        CellButton::More => return None,
    })
}

/// Draw one cell's own parts: its rendered Markdown or collapsed line, its status line, its
/// outputs, and the field an `input()` is typed into.
fn paint_one_cell(
    ui: &mut egui::Ui,
    renderer: &TextRenderer,
    tab: &mut NotebookTab,
    drawing: &CellDrawing,
    frame: &Frame,
) -> Painted {
    let mut painted = Painted::default();
    let left = frame.origin.x;
    let width = (frame.area.right() - 16.0 - left).max(40.0);
    match drawing.shape {
        Shape::Rendered => {
            paint_rendered_markdown(ui, renderer, tab, drawing, frame, width, &mut painted)
        }
        Shape::Collapsed => paint_the_collapsed_line(ui, tab, drawing, frame, width),
        Shape::Source | Shape::Hidden => {}
    }
    if drawing.room.section_note {
        let at = Pos2::new(left, frame.origin.y + drawing.place.text_bottom);
        paint_a_section_note(ui, tab, drawing.cell, &drawing.id, at, width, frame.metrics);
    }
    let first_line = Rect::from_min_size(
        Pos2::new(left, frame.origin.y + drawing.place.first_text),
        Vec2::new(width, frame.metrics.status()),
    );
    painted.changed |=
        paint_the_tags(ui, tab, drawing.cell, &drawing.id, first_line, frame.metrics);
    paint_the_status_and_outputs(ui, tab, drawing, frame, width, &mut painted);
    painted
}

/// A Markdown cell that is not being edited, drawn from its rendering in place of its source. A click
/// on it chooses the cell and a double click edits it.
fn paint_rendered_markdown(
    ui: &mut egui::Ui,
    renderer: &TextRenderer,
    tab: &NotebookTab,
    drawing: &CellDrawing,
    frame: &Frame,
    width: f32,
    painted: &mut Painted,
) {
    let at = Pos2::new(frame.origin.x, frame.origin.y + drawing.place.first_text);
    let Some(rendered) = tab.rendered.get(&drawing.id) else { return };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(frame.area));
    child.set_clip_rect(ui.clip_rect());
    crate::components::editor_view::paint_text(
        &child,
        renderer,
        &rendered.preview.text,
        &rendered.layout,
        at,
    );
    let hit = Rect::from_min_size(
        at,
        Vec2::new(width, rendered.layout.height.max(frame.metrics.status())),
    );
    let response =
        ui.interact(hit, ui.id().with(("rendered-markdown", &drawing.id)), egui::Sense::click());
    painted.pressed = response.clicked();
    painted.edit_markdown = response.double_clicked();
}

/// The one line standing in for a cell whose source is collapsed. Clicking it shows the source.
fn paint_the_collapsed_line(
    ui: &mut egui::Ui,
    tab: &mut NotebookTab,
    drawing: &CellDrawing,
    frame: &Frame,
    width: f32,
) {
    let lines = tab.spans[drawing.cell].body.len();
    let at = Pos2::new(frame.origin.x, frame.origin.y + drawing.place.first_text);
    let words = format!("\u{22EF} {lines} lines collapsed. Click to show them.");
    let rect = Rect::from_min_size(at, Vec2::new(width, frame.metrics.status()));
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        words,
        frame.metrics.output_font(),
        color::text_dim(),
    );
    if ui.interact(rect, ui.id().with(("collapsed", &drawing.id)), egui::Sense::click()).clicked() {
        tab.collapsed.remove(&drawing.id);
        tab.bands_revision += 1;
    }
}

/// The status line under a code cell, and the outputs under that.
fn paint_the_status_and_outputs(
    ui: &mut egui::Ui,
    tab: &mut NotebookTab,
    drawing: &CellDrawing,
    frame: &Frame,
    width: f32,
    painted: &mut Painted,
) {
    let (metrics, left, id) = (frame.metrics, frame.origin.x, drawing.id.as_str());
    let mut y = frame.origin.y + drawing.place.text_bottom + metrics.pad();
    if drawing.room.status {
        let rect =
            Rect::from_min_size(Pos2::new(left - 4.0, y), Vec2::new(width, metrics.status()));
        let count = tab.model.cells.get(drawing.cell).and_then(|found| found.execution_count);
        let run = tab.runs.get(id);
        let words = notebook::status_words(run, count);
        notebook_view::paint_status(
            ui.painter(),
            rect,
            notebook::status_mark(run),
            &words,
            metrics.size,
        );
        y += metrics.status();
    }
    if drawing.room.outputs > 0.0 {
        y += metrics.pad();
        let at = Pos2::new(left + metrics.indent(), y);
        paint_a_cells_outputs(ui, tab, id, at, width - metrics.indent(), metrics, painted);
    }
}

/// The outputs of one cell, collapsed or in full, and the field for an `input()`.
fn paint_a_cells_outputs(
    ui: &mut egui::Ui,
    tab: &mut NotebookTab,
    id: &str,
    at: Pos2,
    width: f32,
    metrics: Metrics,
    painted: &mut Painted,
) {
    let used = match tab.outputs_collapsed.contains(id) {
        true => paint_the_collapsed_outputs(ui, tab, id, at, width, metrics),
        false => paint_the_shown_outputs(ui, tab, id, at, width, metrics, painted),
    };
    if tab.waiting.as_ref().is_some_and(|waiting| waiting.cell == id) {
        paint_the_input_field(ui, tab, Pos2::new(at.x, at.y + used + 4.0), width, metrics, painted);
    }
}

/// The line standing in for outputs that are collapsed. Clicking it shows them. Answers its height.
fn paint_the_collapsed_outputs(
    ui: &mut egui::Ui,
    tab: &mut NotebookTab,
    id: &str,
    at: Pos2,
    width: f32,
    metrics: Metrics,
) -> f32 {
    let rect = Rect::from_min_size(at, Vec2::new(width, metrics.status()));
    let count = tab
        .index_of(id)
        .and_then(|cell| tab.model.cells.get(cell))
        .map(|cell| cell.outputs.len())
        .unwrap_or(0);
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        format!("\u{22EF} {count} outputs collapsed. Click to show them."),
        metrics.output_font(),
        color::text_dim(),
    );
    if ui.interact(rect, ui.id().with(("outputs-collapsed", id)), egui::Sense::click()).clicked() {
        tab.outputs_collapsed.remove(id);
        tab.bands_revision += 1;
    }
    metrics.status()
}

/// The outputs of a cell drawn in full, scrolled where they were left. Answers how tall they are.
fn paint_the_shown_outputs(
    ui: &mut egui::Ui,
    tab: &mut NotebookTab,
    id: &str,
    at: Pos2,
    width: f32,
    metrics: Metrics,
    painted: &mut Painted,
) -> f32 {
    let Some(drawn) = tab.drawn.get(id) else { return 0.0 };
    let mut scroll = tab.output_scroll.get(id).copied().unwrap_or(0.0);
    let outcome = notebook_view::paint_outputs(ui, drawn, at, width, metrics, &mut scroll, id);
    let height = notebook_view::shown_height(drawn, metrics);
    tab.output_scroll.insert(id.to_owned(), scroll);
    painted.pressed |= outcome.pressed;
    painted.open_html = outcome.open_html.clone();
    apply_the_output_outcome(tab, id, &outcome);
    height
}

/// Act on a press in a cell's outputs: open or close a traceback, or sort a table by a column. Each
/// changes how tall the outputs are, so the layout is made again.
fn apply_the_output_outcome(
    tab: &mut NotebookTab,
    id: &str,
    outcome: &notebook_view::OutputOutcome,
) {
    if outcome.toggle_traceback {
        if !tab.tracebacks_open.remove(id) {
            tab.tracebacks_open.insert(id.to_owned());
        }
        tab.bands_revision += 1;
    }
    if let Some(column) = outcome.sort_by {
        let next = match tab.sorts.get(id) {
            Some((sorted, false)) if *sorted == column => Some((column, true)),
            Some((sorted, true)) if *sorted == column => None,
            _ => Some((column, false)),
        };
        match next {
            Some(sort) => tab.sorts.insert(id.to_owned(), sort),
            None => tab.sorts.remove(id),
        };
        tab.bands_revision += 1;
    }
}

/// The field under a cell whose code is waiting on `input()`. Enter sends what was typed.
fn paint_the_input_field(
    ui: &mut egui::Ui,
    tab: &mut NotebookTab,
    at: Pos2,
    width: f32,
    metrics: Metrics,
    painted: &mut Painted,
) {
    let Some(waiting) = tab.waiting.as_mut() else { return };
    let rect = Rect::from_min_size(at, Vec2::new(width.min(520.0), metrics.status()));
    let prompt =
        if waiting.prompt.is_empty() { "Input".to_owned() } else { waiting.prompt.clone() };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let edit = egui::TextEdit::singleline(&mut waiting.typed)
        .hint_text(crate::components::controls::placeholder(
            prompt,
            &metrics.output_font(),
            color::text_faint(),
        ))
        .password(waiting.password)
        .desired_width(rect.width())
        .font(metrics.output_font());
    let response = child.add(edit);
    if !response.has_focus() && !response.lost_focus() {
        response.request_focus();
    }
    let sent = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    if sent {
        painted.input = Some(waiting.typed.clone());
    }
}

impl UnluminousApp {
    /// Open some HTML a cell produced in a browser tab, written to a file in the temporary folder.
    pub(crate) fn open_an_output_in_a_browser_tab(&mut self, html: &str) {
        let page = match html.trim_start().starts_with("<svg") {
            true => format!(
                "<!doctype html><meta charset=\"utf-8\"><body style=\"margin:0\">{html}</body>"
            ),
            false => format!("<!doctype html><meta charset=\"utf-8\"><body>{html}</body>"),
        };
        let name = format!("unluminous-output-{}.html", unluminous_jupyter::nbformat::new_id());
        let path = std::env::temp_dir().join(name);
        match crate::services::store::write_atomically(&path, page.as_bytes()) {
            Ok(()) => self
                .run_action(Action::OpenInBrowser(path), &self.context.clone().unwrap_or_default()),
            Err(problem) => {
                self.message =
                    Some(format!("The output could not be written to open it: {problem}"))
            }
        }
    }

    /// Send what was typed into an `input()` field, once the tab is no longer borrowed.
    pub(crate) fn send_the_notebook_input(&mut self) {
        if let Some(value) = self.notebook_input.take() {
            let index = self.files.active_index();
            self.answer_the_kernel(index, &value);
        }
    }
}

impl UnluminousApp {
    /// Colour a notebook a cell at a time: each code cell with the grammar of the kernel's language,
    /// each Markdown cell with Markdown's, and the marker lines faint, since they are drawn over.
    pub(crate) fn colour_the_notebook(&mut self, index: usize) {
        let file = self.files.at(index);
        let Some(tab) = file.notebook.as_deref() else { return };
        let text = file.document.text().to_string();
        let extension = notebook_extension(&tab.model.metadata);
        let code = self.plugins.for_path(std::path::Path::new(&format!("cell{extension}")));
        let prose = self.plugins.for_path(std::path::Path::new("cell.md"));
        let mut coloured: Vec<(Range<usize>, unluminous_core::Color)> = Vec::new();
        let faint = color::text_faint();
        for span in unluminous_jupyter::text::spans(&text) {
            if span.marker.is_some() {
                coloured.push((
                    span.marker_bytes.clone(),
                    unluminous_core::Color::rgb(faint.r(), faint.g(), faint.b()),
                ));
            }
            let plugin = match span.kind {
                CellKind::Code => code,
                CellKind::Markdown => prose,
                CellKind::Raw => None,
            };
            let Some(plugin) = plugin else { continue };
            let theme = crate::services::plugins::scheme_of(plugin);
            let body = &text[span.body_bytes.clone()];
            let offset = span.body_bytes.start;
            unluminous_core::syntax::scan(body, &plugin.grammar, |range, token| {
                if token != unluminous_core::Token::Text {
                    if let Some(colour) = theme.colour(token) {
                        coloured.push((range.start + offset..range.end + offset, colour));
                    }
                }
            });
        }
        let base =
            unluminous_core::Color::rgb(color::text().r(), color::text().g(), color::text().b());
        let file = self.files.at_mut(index);
        file.document.set_syntax(base, &coloured);
        let now = file.document.text_revision();
        file.coloured_revision = Some(now);
        file.syntax_tokens_revision = None;
        file.cached.fold_tokens = None;
        file.cached.stale = true;
    }
}

/// The file extension of a notebook's language, from its metadata, which picks the grammar its code
/// cells are coloured with. A notebook that says nothing is Python, which nearly every notebook is.
pub fn notebook_extension(metadata: &serde_json::Value) -> String {
    let info = metadata.get("language_info");
    if let Some(extension) =
        info.and_then(|info| info.get("file_extension")).and_then(|value| value.as_str())
    {
        return extension.to_owned();
    }
    let name = info
        .and_then(|info| info.get("name"))
        .and_then(|value| value.as_str())
        .or_else(|| metadata.get("kernelspec")?.get("language")?.as_str());
    match name.unwrap_or("python").to_ascii_lowercase().as_str() {
        "r" => ".r".to_owned(),
        "julia" => ".jl".to_owned(),
        "javascript" => ".js".to_owned(),
        "typescript" => ".ts".to_owned(),
        "rust" => ".rs".to_owned(),
        _ => ".py".to_owned(),
    }
}

/// Cells by id, for the tests below.
#[cfg(test)]
fn rooms_by_id(rooms: &[(&str, Room)]) -> std::collections::HashMap<String, Room> {
    rooms.iter().map(|(id, room)| (id.to_string(), *room)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans_of(text: &str) -> Vec<CellSpan> {
        unluminous_jupyter::text::spans(text)
    }

    #[test]
    fn a_cells_room_goes_above_its_first_line_and_below_its_last() {
        let text = "# %% id=a\none\ntwo\n# %% id=b\nthree";
        let spans = spans_of(text);
        let mut styles = ParagraphStyles::new(5);
        let mut hidden = Vec::new();
        let mut numbers = vec![Some(0); 5];
        let rooms = rooms_by_id(&[("a", Room { above: 10.0, below: 40.0, ..Room::default() })]);
        apply_room(&mut styles, &mut hidden, &mut numbers, &spans[0], &rooms["a"]);
        assert_eq!(styles.get(1).space_above, 10.0);
        assert_eq!(styles.get(1).space_below, 0.0);
        assert_eq!(styles.get(2).space_below, 40.0);
        assert!(hidden.is_empty());
    }

    #[test]
    fn a_rendered_cell_is_its_first_line_replaced_and_the_rest_hidden() {
        let text = "# %% [markdown] id=a\n# Title\nmore\nand more";
        let spans = spans_of(text);
        let mut styles = ParagraphStyles::new(4);
        let mut hidden = Vec::new();
        let mut numbers = vec![Some(0); 4];
        let room = Room { above: 10.0, below: 5.0, replaced: Some(80.0), ..Room::default() };
        apply_room(&mut styles, &mut hidden, &mut numbers, &spans[0], &room);
        assert!(styles.get(1).replaced);
        assert_eq!(styles.get(1).min_height, 80.0);
        assert_eq!(hidden, vec![2..4]);
        assert_eq!(numbers[1..], [None, None, None]);
    }

    #[test]
    fn command_mode_reads_letters_and_edit_mode_leaves_them_for_typing() {
        assert_eq!(
            read_press(egui::Key::A, egui::Modifiers::NONE, true, false),
            Some(Press::Command(egui::Key::A))
        );
        assert_eq!(read_press(egui::Key::A, egui::Modifiers::NONE, false, false), None);
        let shift_enter = egui::Modifiers { shift: true, ..egui::Modifiers::NONE };
        assert_eq!(
            read_press(egui::Key::Enter, shift_enter, false, false),
            Some(Press::Both(NotebookAction::RunCellSelectBelow))
        );
        assert_eq!(
            read_press(egui::Key::Enter, egui::Modifiers::NONE, false, false),
            None,
            "Enter types a new line"
        );
        assert_eq!(
            read_press(egui::Key::Enter, egui::Modifiers::NONE, true, false),
            Some(Press::Both(NotebookAction::EditMode))
        );
        assert_eq!(
            read_press(egui::Key::Escape, egui::Modifiers::NONE, false, true),
            None,
            "Escape closes the Find bar first"
        );
    }

    #[test]
    fn the_command_mode_letters_are_jupyters() {
        assert_eq!(command_key(egui::Key::M), Some(NotebookAction::Convert(CellKind::Markdown)));
        assert_eq!(command_key(egui::Key::Y), Some(NotebookAction::Convert(CellKind::Code)));
        assert_eq!(command_key(egui::Key::B), Some(NotebookAction::AddBelow(CellKind::Code)));
        assert_eq!(command_shift_key(egui::Key::M), Some(NotebookAction::MergeSelected));
        assert_eq!(command_shift_key(egui::Key::V), Some(NotebookAction::PasteAbove));
    }
}
