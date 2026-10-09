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
}

impl NotebookTab {
    /// What cell `cell` is drawn as.
    fn shape(&self, cell: usize) -> Shape {
        let Some(found) = self.model.cells.get(cell) else { return Shape::Source };
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
    room: &Room,
    shape: Shape,
    metrics: Metrics,
    last: bool,
) -> Option<Place> {
    if layout.lines.is_empty() {
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
    let _ = room;
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
        Metrics { size: self.files.active().sized_at.unwrap_or(self.settings.font_size) }
    }

    /// Lay every output and every rendered Markdown cell out at `width`, and record the room each cell
    /// needs. Bumps the tab's bands revision when any cell's room changed, which is what lays the text
    /// out again.
    pub(crate) fn measure_the_notebook(&mut self, ctx: &egui::Context, width: f32) {
        let index = self.files.active_index();
        self.refresh_the_notebook(index);
        let metrics = self.notebook_metrics();
        let Some(mut tab) = self.files.at_mut(index).notebook.take() else { return };
        let count = tab.len();
        let mut changed = false;
        for cell in 0..count {
            let Some(id) = tab.id_of(cell) else { continue };
            let shape = tab.shape(cell);
            let room = match (shape, tab.spans[cell].kind) {
                (Shape::Rendered, _) => self.room_for_markdown(&mut tab, cell, width, metrics),
                (Shape::Collapsed, _) => {
                    Room { replaced: Some(metrics.status()), ..Room::default() }
                }
                (Shape::Source, CellKind::Code) => {
                    room_for_code(ctx, &mut tab, cell, width, metrics)
                }
                (Shape::Source, _) => Room::default(),
            };
            let room = with_gaps(room, shape, metrics, cell == 0, cell + 1 == count);
            if tab.rooms.get(&id) != Some(&room) {
                tab.rooms.insert(id, room);
                changed = true;
            }
        }
        if changed {
            tab.bands_revision += 1;
        }
        self.files.at_mut(index).notebook = Some(tab);
    }

    /// The room a rendered Markdown cell needs: its rendered height, laid out now if it has changed.
    fn room_for_markdown(
        &self,
        tab: &mut NotebookTab,
        cell: usize,
        width: f32,
        metrics: Metrics,
    ) -> Room {
        let found = &tab.model.cells[cell];
        let source = found.source.clone();
        let id = found.id.clone();
        let fresh = tab.rendered.get(&id).is_some_and(|kept| {
            kept.source == source && (kept.width - width).abs() < 0.5 && kept.size == metrics.size
        });
        if !fresh {
            let shown = if source.trim().is_empty() {
                "*Empty Markdown cell. Double click to write in it.*"
            } else {
                &source
            };
            let preview = self.render_markdown(shown, width, metrics.size);
            let layout = unluminous_core::layout(
                &preview.text,
                &preview.chars,
                &preview.paragraphs,
                &self.renderer,
                width,
            );
            tab.rendered.insert(
                id.clone(),
                Rendered { source, width, size: metrics.size, preview, layout },
            );
        }
        let height = tab.rendered.get(&id).map(|kept| kept.layout.height).unwrap_or(0.0);
        Room { replaced: Some(height.max(metrics.status())), ..Room::default() }
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
    let outputs = if room.outputs > 0.0 { room.outputs + metrics.pad() } else { 0.0 };
    let tail = if last { metrics.tail() } else { 0.0 };
    Room { above: gap + pad, below: pad + status + outputs + tail, ..room }
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
            let Some(place) = place_of(layout, span, &room, shape, metrics, cell + 1 == count)
            else {
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
        (true, true, false) if matches!(key, egui::Key::ArrowUp | egui::Key::ArrowDown) => {
            Some(Press::CommandShift(key))
        }
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

/// A key with shift in command mode, and the two chords that move cells.
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
    pub(crate) fn paint_the_cells_over(
        &mut self,
        ui: &mut egui::Ui,
        origin: Pos2,
        area: Rect,
        gutter: Rect,
    ) {
        let metrics = self.notebook_metrics();
        let caret = self.document().selection().head;
        let index = self.files.active_index();
        let Some(mut tab) = self.files.active_mut().notebook.take() else { return };
        let pointer = ui.input(|input| input.pointer.hover_pos());
        let chosen = tab.chosen(caret);
        let count = tab.len();
        let mut hovered = None;
        let mut wanted: Option<(usize, NotebookAction)> = None;
        let mut edit_markdown: Option<usize> = None;
        let mut menu_at: Option<(usize, Pos2)> = None;
        let mut open_html: Option<String> = None;
        let mut select: Option<usize> = None;
        for cell in 0..count {
            let shape = tab.shape(cell);
            let span = tab.spans[cell].clone();
            let Some(id) = tab.id_of(cell) else { continue };
            let room = tab.rooms.get(&id).copied().unwrap_or_default();
            let layout = &self.files.active().cached.layout;
            let Some(place) = place_of(layout, &span, &room, shape, metrics, cell + 1 == count)
            else {
                continue;
            };
            let (top, bottom) =
                (origin.y + place.top - room.above + metrics.pad(), origin.y + place.bottom);
            if bottom < area.top() || top > area.bottom() {
                continue;
            }
            let whole =
                Rect::from_min_max(Pos2::new(area.left(), top), Pos2::new(area.right(), bottom));
            if pointer.is_some_and(|at| whole.contains(at)) {
                hovered = Some(cell);
            }
            let painted = self
                .paint_one_cell(ui, &mut tab, cell, &id, place, room, shape, origin, area, metrics);
            if painted.pressed {
                select = Some(cell);
            }
            if painted.edit_markdown {
                edit_markdown = Some(cell);
            }
            open_html = painted.open_html.or(open_html);
            let shows_buttons =
                hovered == Some(cell) || (chosen.contains(&cell) && chosen.len() == 1);
            if shows_buttons {
                let corner = Pos2::new(area.right() - 14.0, origin.y + place.top - 14.0);
                let rendered = shape == Shape::Rendered;
                if let Some(button) = notebook_view::cell_buttons(
                    ui,
                    corner,
                    span.kind,
                    rendered,
                    &format!("cell {}", cell + 1),
                ) {
                    match button {
                        CellButton::More => {
                            menu_at = Some((cell, Pos2::new(corner.x - 160.0, corner.y + 26.0)))
                        }
                        other => wanted = button_action(other).map(|what| (cell, what)),
                    }
                }
            }
            if span.kind == CellKind::Code && (hovered == Some(cell) || chosen.contains(&cell)) {
                let at = Pos2::new(
                    gutter.left() + 10.0,
                    origin.y + place.first_text + metrics.size * 0.6,
                );
                let place = Rect::from_center_size(at, Vec2::splat(18.0));
                if gutter.width() > 12.0
                    && crate::components::controls::icon_button(
                        ui,
                        place,
                        &format!("Run cell {}", cell + 1),
                        crate::theme::icon::run,
                    )
                {
                    wanted = Some((cell, NotebookAction::RunCell));
                }
            }
            if cell + 1 == count {
                let centre =
                    Pos2::new(area.center().x, origin.y + place.bottom + metrics.tail() / 2.0);
                if let Some(CellButton::Add(kind)) =
                    notebook_view::add_buttons(ui, centre, "after the last cell")
                {
                    wanted = Some((cell, NotebookAction::AddBelow(kind)));
                }
            }
        }
        tab.hovered = hovered;
        self.files.active_mut().notebook = Some(tab);
        self.act_on_what_the_cells_asked(index, wanted, edit_markdown, select, menu_at, open_html);
    }

    /// Act on what a press on the cells asked for, once the tab is back in its place.
    fn act_on_what_the_cells_asked(
        &mut self,
        index: usize,
        wanted: Option<(usize, NotebookAction)>,
        edit_markdown: Option<usize>,
        select: Option<usize>,
        menu_at: Option<(usize, Pos2)>,
        open_html: Option<String>,
    ) {
        if let Some(cell) = select {
            self.choose_a_cell(index, cell, false);
            self.focus = crate::app::Focus::Editor;
        }
        if let Some(cell) = edit_markdown {
            self.choose_a_cell(index, cell, true);
            self.focus = crate::app::Focus::Editor;
        }
        if let Some((cell, what)) = wanted {
            self.choose_a_cell(index, cell, false);
            self.focus = crate::app::Focus::Editor;
            self.notebook_wanted = Some(Action::Notebook(what));
        }
        if let Some((cell, at)) = menu_at {
            self.choose_a_cell(index, cell, false);
            self.notebook_menu = Some(at);
        }
        if let Some(html) = open_html {
            self.open_an_output_in_a_browser_tab(&html);
        }
    }
}

/// What a press on one cell's own drawing asked for.
#[derive(Debug, Default)]
struct Painted {
    pressed: bool,
    edit_markdown: bool,
    open_html: Option<String>,
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

impl UnluminousApp {
    /// Draw one cell's own parts: its rendered Markdown or collapsed line, its status line, its
    /// outputs, and the field an `input()` is typed into.
    #[allow(clippy::too_many_arguments)]
    fn paint_one_cell(
        &mut self,
        ui: &mut egui::Ui,
        tab: &mut NotebookTab,
        cell: usize,
        id: &str,
        place: Place,
        room: Room,
        shape: Shape,
        origin: Pos2,
        area: Rect,
        metrics: Metrics,
    ) -> Painted {
        let mut painted = Painted::default();
        let left = origin.x;
        let width = (area.right() - 16.0 - left).max(40.0);
        match shape {
            Shape::Rendered => {
                let at = Pos2::new(left, origin.y + place.first_text);
                if let Some(rendered) = tab.rendered.get(id) {
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
                    child.set_clip_rect(ui.clip_rect());
                    crate::components::editor_view::paint_text(
                        &child,
                        &self.renderer,
                        &rendered.preview.text,
                        &rendered.layout,
                        at,
                    );
                    let hit = Rect::from_min_size(
                        at,
                        Vec2::new(width, rendered.layout.height.max(metrics.status())),
                    );
                    let response = ui.interact(
                        hit,
                        ui.id().with(("rendered-markdown", id)),
                        egui::Sense::click(),
                    );
                    painted.pressed = response.clicked();
                    painted.edit_markdown = response.double_clicked();
                }
            }
            Shape::Collapsed => {
                let lines = tab.spans[cell].body.len();
                let at = Pos2::new(left, origin.y + place.first_text);
                let words = format!("\u{22EF} {lines} lines collapsed. Click to show them.");
                let rect = Rect::from_min_size(at, Vec2::new(width, metrics.status()));
                ui.painter().text(
                    rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    words,
                    notebook_view::Metrics::output_font(&metrics),
                    color::text_dim(),
                );
                if ui
                    .interact(rect, ui.id().with(("collapsed", id)), egui::Sense::click())
                    .clicked()
                {
                    tab.collapsed.remove(id);
                    tab.bands_revision += 1;
                }
            }
            Shape::Source => {}
        }
        let mut y = origin.y + place.text_bottom + metrics.pad();
        if room.status {
            let rect =
                Rect::from_min_size(Pos2::new(left - 4.0, y), Vec2::new(width, metrics.status()));
            let count = tab.model.cells.get(cell).and_then(|found| found.execution_count);
            notebook_view::paint_status(ui.painter(), rect, tab.runs.get(id), count, metrics.size);
            y += metrics.status();
        }
        if room.outputs > 0.0 {
            y += metrics.pad();
            self.paint_a_cells_outputs(
                ui,
                tab,
                id,
                Pos2::new(left + metrics.indent(), y),
                width - metrics.indent(),
                metrics,
                &mut painted,
            );
        }
        painted
    }

    /// The outputs of one cell, collapsed or in full, and the field for an `input()`.
    #[allow(clippy::too_many_arguments)]
    fn paint_a_cells_outputs(
        &mut self,
        ui: &mut egui::Ui,
        tab: &mut NotebookTab,
        id: &str,
        at: Pos2,
        width: f32,
        metrics: Metrics,
        painted: &mut Painted,
    ) {
        let mut y = at.y;
        if tab.outputs_collapsed.contains(id) {
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
            if ui
                .interact(rect, ui.id().with(("outputs-collapsed", id)), egui::Sense::click())
                .clicked()
            {
                tab.outputs_collapsed.remove(id);
                tab.bands_revision += 1;
            }
            y += metrics.status();
        } else if let Some(drawn) = tab.drawn.get(id) {
            let mut scroll = tab.output_scroll.get(id).copied().unwrap_or(0.0);
            let outcome =
                notebook_view::paint_outputs(ui, drawn, at, width, metrics, &mut scroll, id);
            y += notebook_view::shown_height(drawn, metrics);
            tab.output_scroll.insert(id.to_owned(), scroll);
            painted.pressed |= outcome.pressed;
            painted.open_html = outcome.open_html;
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
        if tab.waiting.as_ref().is_some_and(|waiting| waiting.cell == id) {
            self.paint_the_input_field(ui, tab, Pos2::new(at.x, y + 4.0), width, metrics);
        }
    }

    /// The field under a cell whose code is waiting on `input()`. Enter sends what was typed.
    fn paint_the_input_field(
        &mut self,
        ui: &mut egui::Ui,
        tab: &mut NotebookTab,
        at: Pos2,
        width: f32,
        metrics: Metrics,
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
            let value = waiting.typed.clone();
            self.notebook_input = Some(value);
        }
    }

    /// Open some HTML a cell produced in a browser tab, written to a file in the temporary folder.
    fn open_an_output_in_a_browser_tab(&mut self, html: &str) {
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
