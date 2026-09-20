//! The small controls more than one part of the window needs: a dropdown, a menu row, an icon button
//! and a divider.
//!
//! They live here rather than in the toolbar because the toolbar is no longer the only thing that needs
//! them: the Settings window has dropdowns, and the menu bar has menu rows. One copy means the dropdown
//! in Settings and the dropdown in the toolbar cannot drift apart.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use crate::app::actions::{Action, Entry};
use crate::theme::crisp::CrispPainter;
use crate::theme::{color, icon, size};

/// Where the pointer is in this `Ui`'s own points.
///
/// `Context::pointer_interact_pos` answers in the **window's** points. A canvas node draws into a layer
/// of its own carrying the camera, so a rectangle measured inside a node is in the canvas's points and
/// the two are different spaces the moment the canvas is panned or zoomed at all — comparing one
/// against the other answers about somewhere else entirely. `task-2003` reports that as an Agent-Tasks
/// node whose lanes would not scroll.
///
/// In a pane there is no transform and this is the pointer position unchanged, so one call is right in
/// both places and a component does not have to know which one it is drawing in.
pub fn pointer_in(ui: &egui::Ui) -> Option<Pos2> {
    let at = ui.ctx().pointer_interact_pos()?;
    match ui.ctx().layer_transform_to_global(ui.layer_id()) {
        Some(to_global) => Some(to_global.inverse() * at),
        None => Some(at),
    }
}

/// How much of a field's height the letters inside it may occupy.
///
/// 0.52, which is chosen so that the explorer's 24 point filter box asks for 12.5 — exactly the size
/// the file names under it are drawn at, which is what `task-2004` asks for in as many words: *"the
/// 'Filter files' text is about the same size as the folder/file name text"*.
const FIELD_TEXT: f32 = 0.52;

/// The point size a field of `height` points sets its text in.
///
/// **A field's box is a fixed number of points tall and its text was the interface's size**, and those
/// are two unrelated numbers. `appearance.ui.font.size` is 12.5 by default and the machine `task-2004`
/// was reported from has it far larger, so the explorer's filter drew `Filter files` more than twice the
/// height of the file names beside it — and the caret, whose height is the row that font occupies, stood
/// proud of the 24 point box at both ends. `components::settings_dialog`'s 26 point search box is the
/// same pair, and so is every field built from [`search_field`] and from `modal::field`.
///
/// So **a field sets its text at a fixed fraction of its own height**, and the interface's size is not
/// asked about at all. That is the rule rather than a cap on the interface for one reason worth writing
/// down: a field that is zoomed carries the zoom in its height. The explorer's filter box is
/// `view.at(24.0)` points tall and its rows are `view.at(12.5)` — so a fraction of the height is the
/// row size at every zoom, where a cap on `appearance.ui.font.size` would have been right at a zoom of
/// one and too small at every other.
///
/// The clamp is a floor and a ceiling rather than a preference. A field dragged to nothing would ask for
/// a font of no size, which egui lays out as an empty galley with no caret in it; and a well several rows
/// tall — a commit message, a chat composer — is not asking for letters half its own height, so those
/// pass the size they really draw in to [`field_takes_the_whole_rectangle_at`] instead.
pub fn field_font_size(height: f32) -> f32 {
    (height * FIELD_TEXT).clamp(6.0, 24.0)
}

/// The same as a `FontId`, which is what a `TextEdit` and a measurement both take.
pub fn field_font(height: f32) -> egui::FontId {
    egui::FontId::proportional(field_font_size(height))
}

/// Where a field's `TextEdit` goes and what it sets its text in.
///
/// **The two are one answer, so a caller cannot take one and forget the other.** A strip measured for one
/// size holding text set in another is the fault `task-1914` fixed by hand at the browser's address bar
/// and `task-2004` found at every field in the window; returning them together is what stops the next
/// field being the next chance to get it wrong.
pub struct FieldText {
    /// The rectangle to lay the box out in: one text row, centred in the field.
    pub rect: Rect,
    /// The font that row was measured for, which is the font the box has to be given.
    pub font: egui::FontId,
}

/// The strip inside a field, and the font it is measured for.
///
/// Every field in Unluminous draws its own frame — `FIELD` with a one point stroke, the corner radius the
/// style guide gives — and puts an `egui::TextEdit` inside it with `Frame::NONE`, because egui's own
/// frame is not the one the design shows. egui then lays that box out at the **top** of the rectangle it
/// is given, and with no frame there is no margin to push it down, so a rectangle the height of the whole
/// field left the words sitting against its top edge: `Filter files` was about three points high in a 24
/// point box, on a different line from the magnifier beside it.
///
/// So a field hands its box one text row, centred in the field, set in [`field_font_size`]'s own answer.
/// One function rather than a number repeated in five components, because a fifth field added later would
/// otherwise be the fifth chance to get it wrong.
///
/// `left` is how far in from the field's left edge the text starts — 26 points where there is a magnifier
/// in front of it, 8 where there is not.
pub fn field_text(ui: &egui::Ui, field: Rect, left: f32) -> FieldText {
    let font = field_font(field.height());
    let row = ui.ctx().fonts_mut(|fonts| fonts.row_height(&font));
    FieldText { rect: field_text_rect_at(field, left, row), font }
}

/// The same, for a field that sets its text in a size of its own rather than in [`field_font_size`]'s.
///
/// **A field centres the row it is going to draw, and the size that row will be set in is therefore an
/// argument rather than an assumption.** This used to be the exception and [`field_text`] used
/// `TextStyle::Body` — `appearance.ui.font.size`, 12.5 by default and 24 on the machine `task-1914` was
/// reported from. A caller that then drew at a fixed size got a strip measured for one size holding text
/// set in another: at 24 the browser's address bar was handed a 28 point strip, drew its 12 point words
/// at the **top** of it, and so put them about six points above the middle of a 22 point field and over
/// its own border.
///
/// Since `task-2004` the pair without `_at` derives the size from the field's own height instead, so this
/// is for the two shapes that genuinely know better: a box whose text follows a pane's own font — the
/// Agent-Tasks board, the chat composer — and a well several rows tall, which is not asking for letters
/// half its own height.
///
/// `row` is the height the text will really occupy, which is `FontId::proportional(n)`'s own row —
/// `Ui::fonts(|f| f.row_height(&font))` — so the box and the letters are measured the same way.
pub fn field_text_rect_at(field: Rect, left: f32, row: f32) -> Rect {
    let width = (field.width() - left - 8.0).max(1.0);
    Rect::from_min_size(
        Pos2::new(field.left() + left, (field.center().y - row / 2.0).round()),
        Vec2::new(width, row),
    )
}

/// The same rectangle, having first given the **whole** of the field to the box that will go in it.
///
/// That one text row is what stops the words sitting against the top edge, and it was also the only
/// part of the control a pointer could hit. A 24 point field has nine points of dead height and
/// eight points of dead width down its left hand side, and a click in any of it took the keyboard
/// nowhere at all — so the pane behind went on holding the keys, and `Ctrl+V` in the New Data Source
/// dialog put the path into the file behind the window. Measured in `task-1795`:
///
/// ```text
/// text rect = [[491.0 244.0] - [803.0 259.0]]   // 15 points tall inside a 24 point field
/// after a click in the padding, text_edit_focused = false
/// document = "helloPASTED"
/// ```
///
/// The same miss is why `Ctrl/Cmd+Enter` in a SQL console did nothing: that chord is read only while
/// the box has the keyboard.
///
/// The claim is made **before** the box is added, which is what keeps it from taking anything away:
/// egui gives a pointer to the last widget that wanted it, so a press inside the box's own strip is
/// still the box's, and this catches only the padding round it. `id` is the id the caller then gives
/// its `TextEdit`, so this hands the keyboard to that box and not to whichever one egui's auto
/// counter happened to name.
pub fn field_takes_the_whole_rectangle(
    ui: &egui::Ui,
    field: Rect,
    left: f32,
    id: egui::Id,
    name: &str,
) -> FieldText {
    claim_the_field(ui, field, id, name);
    field_text(ui, field, left)
}

/// The same claim, for a field whose text is set in a size of its own. See [`field_text_rect_at`].
pub fn field_takes_the_whole_rectangle_at(
    ui: &egui::Ui,
    field: Rect,
    left: f32,
    id: egui::Id,
    name: &str,
    font: &egui::FontId,
) -> Rect {
    claim_the_field(ui, field, id, name);
    let row = ui.ctx().fonts_mut(|fonts| fonts.row_height(font));
    field_text_rect_at(field, left, row)
}

/// The claim on its own, for a box that is laid out over the whole of its field rather than in a
/// strip: the commit message, a ticket's description, a console's SQL.
///
/// Those have the same fault in a milder form — the margin between the drawn frame and the box is
/// dead — and the same answer. See [`field_takes_the_whole_rectangle`] for what it is for.
pub fn claim_the_field(ui: &egui::Ui, field: Rect, id: egui::Id, name: &str) -> egui::Response {
    // Every field has a right click menu, drawn from here so that a field written later has one too.
    // See [`field_menu`]. `task-2009`.
    field_menu(ui, field, id);
    let response = ui.interact(field, id.with("field-ground"), Sense::click());
    // **Named, like every other control** (`task-1984` §3.6). It is a widget that takes a press and
    // can be found, and `design/style-guide.md` has required a name on one of those since
    // `task-1655` — this was the one shape that had none, at every field in the window. The name says
    // what the field is and ends in `field`, so it cannot collide with the box inside it, which
    // carries its own.
    let named = name.to_owned();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), named.clone())
    });
    if response.clicked() {
        ui.ctx().data_mut(|data| data.insert_temp(wants_the_keyboard(), id));
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    response
}

/// What a field's right click menu asked for, which is applied on the **next** frame.
///
/// The four things a person expects from a text box, and every one of them is something
/// `egui::TextEdit` already does for itself when it has the keyboard and the event arrives — so what
/// this carries is the *event to send*, not a second implementation of cutting text. See
/// [`field_menu`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldEdit {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl FieldEdit {
    /// The row's name, which is what the menu draws and what a test asks for.
    pub fn name(self) -> &'static str {
        match self {
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::SelectAll => "Select All",
        }
    }
}

/// What a field's menu remembers while it is open: where it was opened, and what was selected then.
///
/// **The selection has to be remembered rather than read**, because the right click that opens the
/// menu is a press inside the box, and a press inside a box moves its caret. A click is reported on
/// the **release**, which is a frame after the press, so by the time the menu opens the selection is
/// already gone — [`what_the_box_had_last_frame`] is what keeps it. It is written back into the box
/// before the event is sent, which is what makes `Copy` copy the words a person had selected rather
/// than nothing at all. `task-2009`.
#[derive(Debug, Clone, Copy)]
pub struct FieldMenu {
    at: Pos2,
    selected: Option<egui::text::CCursorRange>,
}

/// Where a field records what its menu asked for, and which box asked, for the **next** frame.
///
/// The next frame for exactly the reason [`wants_the_keyboard`] is: the press that chose the row is a
/// press outside the box, so egui surrenders the box's focus at the end of that frame — and an event
/// sent to a box that is about to lose the keyboard is an event nothing reads.
/// `app::hold_the_keyboard` is where both are acted on, together, because the box has to be given the
/// keyboard on the same frame the event is pushed.
pub fn wants_an_edit() -> egui::Id {
    egui::Id::new("unluminous-field-wants-an-edit")
}

/// What one asked-for edit carries: the box, the row that was chosen, and what it had selected.
pub type AskedEdit = (egui::Id, FieldEdit, Option<egui::text::CCursorRange>);

/// The right click menu every text field in Unluminous has: cut, copy, paste, select all.
///
/// `task-2009`: *"Urls when i open an html file should be selectable and copy pasteable. e.g. right
/// click to see menu of copy/paste/cut/etc."* The address bar is a real `egui::TextEdit` and always
/// was, so selecting and `Ctrl+C` worked; what there was no way to reach at all was the menu.
///
/// It is here rather than in the browser's toolbar for the reason [`field_takes_the_whole_rectangle`]
/// is: nineteen fields in this window are built the same way, and a menu written at one of them would
/// be a menu the other eighteen do not have. [`claim_the_field`] calls this, so a field added later
/// gets it without asking.
///
/// **The pointer is read rather than a widget's `secondary_clicked`**, because a field is two widgets
/// — the ground that claims the padding and the box that holds the words — and which of them a right
/// click lands on depends on where in the field the pointer was. [`pointer_in`] is what makes that
/// right inside a canvas node as well as in a pane.
///
/// Nothing here changes any text. Each row records a [`FieldEdit`] against this field's own id, and
/// `app::hold_the_keyboard` turns it into the `egui::Event` that `TextEdit` already answers.
pub fn field_menu(ui: &egui::Ui, field: Rect, id: egui::Id) {
    let menu = id.with("field-menu");
    let before = what_the_box_had_last_frame(ui, id);
    let opened = ui.input(|input| input.pointer.secondary_clicked())
        && pointer_in(ui).is_some_and(|at| field.contains(at));
    if opened {
        if let Some(at) = ui.ctx().pointer_interact_pos() {
            ui.ctx().data_mut(|data| data.insert_temp(menu, FieldMenu { at, selected: before }));
        }
    }
    let Some(FieldMenu { at, selected }) = ui.ctx().data(|data| data.get_temp::<FieldMenu>(menu))
    else {
        return;
    };
    // A row is dimmed rather than absent when there is nothing to act on, which is the style guide's
    // distinction: `Copy` with nothing selected is a control that will apply the moment something is.
    let selected = selected.filter(|range| !range.is_empty());
    let has_a_selection = selected.is_some();
    let rows = [
        (FieldEdit::Cut, has_a_selection),
        (FieldEdit::Copy, has_a_selection),
        (FieldEdit::Paste, true),
        (FieldEdit::SelectAll, true),
    ];
    let mut chosen = None;
    let popup = egui::Popup::new(menu, ui.ctx().clone(), at, ui.layer_id())
        .kind(egui::PopupKind::Menu)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .frame(
            egui::Frame::popup(ui.style())
                .fill(color::menu())
                .stroke(Stroke::new(1.0, color::control_border()))
                .inner_margin(6),
        )
        .width(FIELD_MENU_WIDTH);
    let mut close = false;
    if let Some(response) = popup.show(|ui| {
        for (edit, enabled) in rows {
            if menu_row(ui, edit.name(), "", enabled, false, 0.0) {
                chosen = Some(edit);
            }
        }
    }) {
        close = response.response.should_close();
    }
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if let Some(edit) = chosen {
        let asked: AskedEdit = (id, edit, selected);
        ui.ctx().data_mut(|data| data.insert_temp(wants_an_edit(), asked));
        ui.ctx().data_mut(|data| data.insert_temp(wants_the_keyboard(), id));
        close = true;
    }
    if close {
        ui.ctx().data_mut(|data| data.remove::<FieldMenu>(menu));
    }
}

/// How wide a field's right click menu is. Four short words, so it is narrower than a context menu.
const FIELD_MENU_WIDTH: f32 = 180.0;

/// What this box had selected at the end of the frame before this one, and remember what it has now.
///
/// **One frame of history, because a right click takes two.** `Response::secondary_clicked` is
/// reported on the release, and the press a frame earlier already went through the `TextEdit` and
/// moved its caret — so the state this reads on the frame the menu opens is the state *after* the
/// selection was thrown away. What was there before it is one frame further back.
///
/// Called from [`field_menu`], which runs at the top of [`claim_the_field`] and therefore before the
/// box is created, so what it reads each frame is where the box was left at the end of the last one.
fn what_the_box_had_last_frame(ui: &egui::Ui, id: egui::Id) -> Option<egui::text::CCursorRange> {
    let now = egui::text_edit::TextEditState::load(ui.ctx(), id)
        .and_then(|state| state.cursor.char_range())
        .filter(|range| !range.is_empty());
    let slot = id.with("field-was-selected");
    ui.ctx().data_mut(|data| {
        let was = data.get_temp::<Option<egui::text::CCursorRange>>(slot).flatten();
        data.insert_temp(slot, now);
        was.or(now)
    })
}

/// Where a field records the box it wants the keyboard given to, for the **next** frame.
///
/// It has to be the next frame, and that is not a stylistic choice. egui surrenders a widget's focus
/// when a press lands anywhere that widget is not hovered:
///
/// ```ignore
/// let pointer_clicked_elsewhere = should_surrender_focus && !res.hovered();
/// if pointer_clicked_elsewhere && memory.has_focus(id) { memory.surrender_focus(id); }
/// ```
///
/// A press in a field's padding is not on the box inside it, so a focus handed over at the moment of
/// the press was taken straight back a few lines later, when the box itself was created in the same
/// frame. Measured: the claim reported `clicked=true` and `text_edit_focused` was still false
/// afterwards. `app::hold_the_keyboard` is where this is acted on, because it runs before anything
/// is drawn and is already the one place the window arranges who holds the keys.
pub fn wants_the_keyboard() -> egui::Id {
    egui::Id::new("unluminous-field-wants-the-keyboard")
}

/// A box to search in: the field, a magnifier in front of it, and the words to show while it is
/// empty.
///
/// The explorer's filter, `Go to File` and `Find in Files` are all this shape, and the field's own
/// frame is `FIELD` with a one point stroke and the style guide's corner radius wherever it appears.
/// `name` is what the box is called, which is what a test asks for and what assistive technology
/// reads out — egui names a text box after whatever has been typed into it, so every field in Unluminous
/// says its own name.
pub fn search_field(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    hint: &str,
    value: &mut String,
) -> egui::Response {
    search_field_over(ui, area, name, hint, value, true)
}

/// The same field with its own ground drawn or left alone, for a field on a decoration canvas.
///
/// See [`choice_button_over`]: the well the picture shows is drawn behind the whole pane, and a flat
/// rectangle drawn here would fill it in.
pub fn search_field_over(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    hint: &str,
    value: &mut String,
    ground: bool,
) -> egui::Response {
    let painter = ui.painter().clone();
    if ground {
        painter.rect(
            area,
            CornerRadius::same(size::CONTROL_CORNER),
            color::field(),
            Stroke::new(1.0, color::control_border()),
            egui::StrokeKind::Inside,
        );
    }
    icon::magnifier(&painter, Pos2::new(area.left() + 15.0, area.center().y), color::text_faint());
    let id = ui.id().with(("search-field", name));
    let inside = field_takes_the_whole_rectangle(ui, area, 28.0, id, &format!("{name} field"));
    let mut field = ui.new_child(egui::UiBuilder::new().max_rect(inside.rect));
    let response = field.add(
        egui::TextEdit::singleline(value)
            .id(id)
            .hint_text(egui::RichText::new(hint).color(color::text_faint()).size(inside.font.size))
            .font(inside.font.clone())
            .frame(egui::Frame::NONE)
            .desired_width(inside.rect.width())
            .text_color(color::text_control()),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, name));
    response
}

/// Text with some of its characters picked out in the accent colour, which is how a search result
/// shows what it matched.
///
/// `marks` are character positions, not byte positions, because that is what a matcher counting
/// letters produces. One `LayoutJob` rather than one galley per letter, so the text is still laid
/// out as text: painting each character at a position worked out by adding up widths loses the
/// kerning between them, which is visible at any size worth reading.
pub fn marked_text(
    painter: &egui::Painter,
    text: &str,
    marks: &[usize],
    tint: Color32,
    font: egui::FontId,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    let plain = egui::TextFormat { font_id: font.clone(), color: tint, ..Default::default() };
    let marked = egui::TextFormat { font_id: font, color: color::accent(), ..Default::default() };
    for (index, character) in text.chars().enumerate() {
        let format = if marks.contains(&index) { marked.clone() } else { plain.clone() };
        job.append(&character.to_string(), 0.0, format);
    }
    painter.layout_job(job)
}

/// The fill behind a chosen row in any list: `SELECTED_ROW`, at the radius the caller's row is drawn
/// with.
///
/// `design/style-guide.md` calls this "a row in a list": the same pill draws the open file in the
/// explorer, the chosen page in Settings, the chosen plugin, and the chosen commit in the history.
/// The radius is given rather than fixed, because it is not the same number everywhere — a
/// completion row is 4, a modal row is 5, a matched line in a preview is 3, a file tab is square —
/// and unifying it would change what is drawn, which the screenshot tests compare pixel for pixel.
pub fn pill(painter: &egui::Painter, rect: Rect, radius: u8) {
    painter.rect_filled(rect, CornerRadius::same(radius), color::selected_row());
}

/// The same pill with a stroke traced round its own edge in one shape, for a tab strip's own
/// "chosen" mark rather than a plain row.
///
/// It is one call to `Painter::rect`, exactly as every site that needs it already made, rather than
/// this function's fill followed by a second, stroke-only shape: two shapes stacked would very
/// likely paint the same pixels, but the screenshot tests compare images rather than reasoning about
/// tessellation, so the one call that was always made is kept as one call.
pub fn pill_with_stroke(painter: &egui::Painter, rect: Rect, radius: u8, stroke: Stroke) {
    painter.rect(
        rect,
        CornerRadius::same(radius),
        color::selected_row(),
        stroke,
        egui::StrokeKind::Inside,
    );
}

/// A colour part of the way between two others, linear per channel.
///
/// The gutter's blame column uses it to fade an entry's tint by how old the commit is; the
/// scrollbar uses it to fade the thumb between its quiet and its used colour. Both wrote out the
/// same three lines of arithmetic under a different name (`mix`, `along`/`amount`) with no shared
/// version.
pub fn mix(from: Color32, to: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
    Color32::from_rgb(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
    )
}

/// `text` unchanged, unless it is longer than `threshold` characters, in which case the first `keep`
/// characters are followed by an ellipsis.
///
/// This is the shape the debug tile's value column and the run widget's name button both wrote out
/// by hand, as a character count rather than a measured width, because each needs the length before
/// anything is drawn — the debug tile flattens a whole value onto one row, and the run widget's
/// button is sized by the title bar before it is painted. `threshold` and `keep` are given
/// separately rather than as one number because the two callers do not agree on what "at the limit"
/// means: the run widget wants the ellipsis included in its own limit, so it keeps one character
/// fewer than the threshold it cuts at; the debug tile wants everything up to its limit kept
/// whichever way, so its ellipsis is one character past it. Collapsing that difference into a single
/// number would move the cut point by one character for whichever caller did not get to keep its own
/// convention.
pub fn truncate_chars(text: &str, threshold: usize, keep: usize) -> String {
    if text.chars().count() <= threshold {
        return text.to_owned();
    }
    format!("{}\u{2026}", text.chars().take(keep).collect::<String>())
}

/// How a [`chip`] is filled: a stroke round its own outline with nothing behind it, or its own
/// colour painted underneath the text.
pub enum ChipFill {
    Outline(Stroke),
    Tint(Color32),
}

/// A short label in a chip rounded fully into a pill by its own height — the Agent-Tasks board's
/// sprint status and epic marks are both this shape, one outlined and one filled.
///
/// `at` is the point the caller already has to place it from: the chip's own left edge when `left`
/// is true, its right edge otherwise, because a row places some marks working left to right and some
/// working right to left. `pad` is the room either side of the measured text, both padding numbers
/// answered separately because the two callers on the board do not use the same ones, and unifying
/// them would change what is drawn. Returns how wide the chip came out, which is what a caller
/// subtracts to place the next mark along the row.
#[allow(clippy::too_many_arguments)]
pub fn chip(
    painter: &egui::Painter,
    at: Pos2,
    left: bool,
    text: &str,
    font_size: f32,
    tint: Color32,
    fill: ChipFill,
    pad: Vec2,
) -> f32 {
    let galley =
        painter.crisp_layout_no_wrap(text.to_owned(), egui::FontId::proportional(font_size), tint);
    let top_left = Pos2::new(
        if left { at.x } else { at.x - galley.size().x - pad.x },
        at.y - galley.size().y / 2.0 - pad.y / 2.0,
    );
    let chip = Rect::from_min_size(top_left, galley.size() + pad);
    let radius = CornerRadius::same((chip.height() / 2.0) as u8);
    match fill {
        ChipFill::Outline(stroke) => {
            painter.rect(chip, radius, Color32::TRANSPARENT, stroke, egui::StrokeKind::Inside);
        }
        ChipFill::Tint(colour) => {
            painter.rect_filled(chip, radius, colour);
        }
    }
    painter.crisp_galley(
        Pos2::new(chip.min.x + pad.x / 2.0, at.y - galley.size().y / 2.0),
        galley,
        tint,
    );
    chip.width()
}

/// A line of text centred horizontally in `area`, its top at `y`.
///
/// The one piece of the Agent-Chat pane's empty conversation screen that is genuinely the same
/// wherever it appears — the greeting, the subtitle, a starter's own label and the "no
/// conversations yet" notice are all one line, centred, at a size and a colour the caller chooses.
/// The empty states in the other two provider panes are not this shape: the board's own notices are
/// left-anchored rather than centred, and the database pane's wraps inside a fixed width and sits
/// above a button, so unifying either of those into this would change what they draw rather than
/// only where the code for it lives.
pub fn centred_line(
    painter: &egui::Painter,
    area: Rect,
    y: f32,
    text: &str,
    size: f32,
    tint: Color32,
) {
    let galley =
        painter.crisp_layout_no_wrap(text.to_owned(), egui::FontId::proportional(size), tint);
    let at = Pos2::new(area.center().x - galley.size().x / 2.0, y);
    painter.crisp_galley(at, galley, tint);
}

/// A button showing the current value, which opens a list when clicked.
///
/// `contents` draws the list and returns what was chosen, so the caller decides what a choice is: the
/// toolbar returns a `unluminous_core::Command` and the Settings window returns a font size.
pub fn dropdown<T>(
    ui: &mut egui::Ui,
    area: Rect,
    value: &str,
    name: &str,
    draw: Option<fn(&egui::Painter, Pos2, Color32)>,
    contents: impl FnOnce(&mut egui::Ui) -> Option<T>,
) -> Option<T> {
    dropdown_over(ui, area, value, name, draw, true, contents)
}

/// The same dropdown with its own ground drawn or left alone, for one on a decoration canvas.
///
/// See [`choice_button_over`] and [`search_field_over`], which exist for the same reason and are the two
/// controls the board already needed it for: the well a value sits in on the Agent-Tasks ticket is drawn
/// **behind** the whole modal by `services::vello_canvas`, and a flat rectangle painted here would fill it
/// in. Everything else about the control — the words, the chevron, the popup, the name — is one function.
#[allow(clippy::too_many_arguments)]
pub fn dropdown_over<T>(
    ui: &mut egui::Ui,
    area: Rect,
    value: &str,
    name: &str,
    draw: Option<fn(&egui::Painter, Pos2, Color32)>,
    ground: bool,
    contents: impl FnOnce(&mut egui::Ui) -> Option<T>,
) -> Option<T> {
    let id = ui.id().with(("dropdown", name));
    let response = ui.interact(area, id, Sense::click()).on_hover_text(name);
    let painter = ui.painter();
    if ground {
        painter.rect(
            area,
            CornerRadius::same(size::CONTROL_CORNER),
            color::control(),
            Stroke::new(1.0, color::control_border()),
            egui::StrokeKind::Inside,
        );
    }
    let mut text_left = area.left() + 10.0;
    if let Some(draw) = draw {
        draw(painter, Pos2::new(text_left + 4.0, area.center().y), color::text_dim());
        text_left += 16.0;
    }
    let galley = painter.crisp_layout_no_wrap(
        value.to_owned(),
        egui::FontId::proportional(12.5),
        color::text_control(),
    );
    painter.crisp_galley(
        Pos2::new(text_left, area.center().y - galley.size().y / 2.0),
        galley,
        color::text_control(),
    );
    icon::chevron_down(painter, Pos2::new(area.right() - 11.0, area.center().y), color::text_dim());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), name)
    });

    // `Popup::from_toggle_button_response` opens and closes on clicks of this button and holds the state
    // itself, under `Popup::default_response_id(&response)` — the button's own id with `"popup"` joined
    // on, not the button's id alone. Closing with the bare id closed a popup nothing was tracked under,
    // so the real one stayed open: a value chosen from `Model` stayed on screen, floating over whatever
    // was drawn underneath it, and ate the next field's click as "outside" instead of opening it.
    let popup_id = egui::Popup::default_response_id(&response);
    let chosen = egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            egui::Frame::popup(ui.style())
                .fill(color::control())
                .stroke(Stroke::new(1.0, color::control_border())),
        )
        .width(area.width().max(120.0))
        .show(contents)
        .and_then(|inner| inner.inner);
    if chosen.is_some() {
        egui::Popup::close_id(ui.ctx(), popup_id);
    }
    chosen
}

/// A small icon button that opens a panel under itself and stays open until it is clicked away from.
///
/// A sibling of [`dropdown`] rather than a setting on it, because the two are different things. A
/// dropdown is a value picker: it shows the value it holds, and choosing one closes it. A flyout is
/// a panel of controls that is used several times in a row — bold, then a colour, then an alignment
/// — so it closes only when the pointer goes elsewhere, and `contents` draws whatever it likes into
/// the rectangle it is given rather than returning one choice.
///
/// `contents` is handed the `Ui` inside the panel and returns whatever the caller wants out of it.
/// The return is `None` while the panel is shut.
///
/// One thing a flyout must not hold is another popup. egui keeps at most one popup open at a time,
/// so a dropdown inside this panel would shut the panel the moment it opened.
pub fn flyout<T>(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    draw: fn(&egui::Painter, Pos2, Color32),
    width: f32,
    contents: impl FnOnce(&mut egui::Ui) -> T,
) -> Option<T> {
    let response =
        ui.interact(area, ui.id().with(("flyout", name)), Sense::click()).on_hover_text(name);
    // What the panel will be by the time it is drawn: the click this frame is what toggles it, and
    // the button has to be tinted for the state it is going into rather than the one it is leaving.
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response))
        != response.clicked();
    let painter = ui.painter();
    if open {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::accent());
    } else if response.hovered() {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::control());
    }
    let tint = if open { color::text_strong() } else { color::text_control() };
    draw(painter, area.center(), tint);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), open, name)
    });
    egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            egui::Frame::popup(ui.style())
                .fill(color::menu())
                .stroke(Stroke::new(1.0, color::control_border())),
        )
        .width(width)
        .show(contents)
        .map(|inner| inner.inner)
}

/// A flyout whose button carries a word and a chevron rather than an icon.
///
/// The run widget's name is the only one — see [`flyout`] for what a flyout is and why it is not a
/// [`dropdown`]. It is here rather than in the widget for the reason everything else in this file
/// is: a second control that almost agreed with `flyout` about how a popup opens and closes would
/// be a second chance to get it wrong.
///
/// `label` is what is drawn and `name` is what the control is called, because the label is a
/// configuration's name and changes, and a control whose accessible name changed with its value
/// could not be asked for by a test.
pub fn labelled_flyout<T>(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    label: &str,
    width: f32,
    contents: impl FnOnce(&mut egui::Ui) -> T,
) -> Option<T> {
    labelled_flyout_with_icon(ui, area, name, label, None, width, contents)
}

/// The same, with a drawn mark in front of the word.
///
/// **`flyout` is the wrong helper for a button that carries a word**, and that is what this exists to
/// stop being rediscovered: `flyout` draws its icon at `area.center()`, which is right for the square
/// icon buttons it was written for and puts the mark on top of the word on a button wide enough to hold
/// one. Measured — `components::branch_widget` drew `main` with the branch icon over the middle of it.
///
/// So the mark goes at a fixed offset from the left and the word starts clear of it, and a caller passing
/// `None` gets exactly what `labelled_flyout` always drew.
pub fn labelled_flyout_with_icon<T>(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    label: &str,
    mark: Option<fn(&egui::Painter, Pos2, Color32)>,
    width: f32,
    contents: impl FnOnce(&mut egui::Ui) -> T,
) -> Option<T> {
    let response = ui
        .interact(area, ui.id().with(("labelled-flyout", name)), Sense::click())
        .on_hover_text(name);
    // What the panel will be by the time it is drawn: the click this frame is what toggles it.
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response))
        != response.clicked();
    let painter = ui.painter();
    if open || response.hovered() {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::control());
    }
    let tint = if open { color::text_strong() } else { color::text_control() };
    // The word starts after the mark when there is one, and where it always did when there is not.
    let words_from = match mark {
        Some(draw) => {
            draw(painter, Pos2::new(area.left() + 11.0, area.center().y), tint);
            area.left() + 22.0
        }
        None => area.left() + 9.0,
    };
    let galley =
        painter.crisp_layout_no_wrap(label.to_owned(), egui::FontId::proportional(12.5), tint);
    painter.crisp_galley(
        Pos2::new(words_from, area.center().y - galley.size().y / 2.0),
        galley,
        tint,
    );
    icon::chevron_down(painter, Pos2::new(area.right() - 10.0, area.center().y), color::text_dim());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), open, name)
    });
    egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            egui::Frame::popup(ui.style())
                .fill(color::menu())
                .stroke(Stroke::new(1.0, color::control_border())),
        )
        .width(width)
        .show(contents)
        .map(|inner| inner.inner)
}

/// A button carrying a word rather than a picture, filled when what it stands for is switched on.
///
/// The three line spacings are the only ones. They were a dropdown when they sat in the toolbar and
/// cannot stay one inside a flyout, because egui keeps one popup open at a time and opening the list
/// would shut the panel it was in. Three buttons in a row are better in a panel anyway: which
/// spacing is on can be seen without opening anything, which is how the alignments beside them
/// already work.
pub fn choice_button(ui: &mut egui::Ui, area: Rect, label: &str, active: bool) -> bool {
    choice_button_named(ui, area, label, label, active)
}

/// A choice button that announces a name other than the word drawn on it.
///
/// For a button whose word alone does not say what pressing it does, and for one whose word already appears
/// somewhere else on the same screen. The agent chooser under the New lane is both: it draws `claude`, which is
/// also the word on every card assigned to that agent, so a test or a screen reader asking for `claude` finds
/// several things and cannot tell which is the chooser.
pub fn choice_button_named(
    ui: &mut egui::Ui,
    area: Rect,
    label: &str,
    announced: &str,
    active: bool,
) -> bool {
    choice_button_over(ui, area, label, announced, active, true)
}

/// The same button with its own ground drawn or left alone.
///
/// **`ground: false` is for a button whose surface something else has already drawn**, which is what a
/// plugin's decoration canvas does: the gradient, the shadows and the pressed edge are painted into one
/// texture behind the whole pane, and a flat rectangle drawn here would cover them. What is left is the word
/// and the click, which is all this ever really was. See `services::vello_canvas`.
pub fn choice_button_over(
    ui: &mut egui::Ui,
    area: Rect,
    label: &str,
    announced: &str,
    active: bool,
    ground: bool,
) -> bool {
    let response = ui.interact(area, ui.id().with(("choice", announced)), Sense::click());
    let painter = ui.painter();
    if !ground {
        // A hover still has to answer, or a button on a canvas would be the one control in Unluminous that never
        // says it was reached. A wash rather than a fill, so the gradient under it still shows.
        if response.hovered() {
            painter.rect_filled(
                area,
                CornerRadius::same(size::CONTROL_CORNER),
                Color32::from_white_alpha(14),
            );
        }
    } else if active {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::accent());
    } else if response.hovered() {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::control());
    } else {
        painter.rect(
            area,
            CornerRadius::same(size::CONTROL_CORNER),
            Color32::TRANSPARENT,
            Stroke::new(1.0, color::control_border()),
            egui::StrokeKind::Inside,
        );
    }
    let tint = if active { color::text_strong() } else { color::text_control() };
    let galley =
        painter.crisp_layout_no_wrap(label.to_owned(), egui::FontId::proportional(12.5), tint);
    painter.crisp_galley(area.center() - galley.size() / 2.0, galley, tint);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), active, announced)
    });
    response.clicked()
}

/// A heading beside a row of controls in a flyout, naming what the row is for.
pub fn row_label(painter: &egui::Painter, at: Pos2, name: &str) {
    let galley = painter.crisp_layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(11.5),
        color::text_dim(),
    );
    painter.crisp_galley(Pos2::new(at.x, at.y - galley.size().y / 2.0), galley, color::text_dim());
}

/// The rows of one menu, whether it hangs from the bar or from a right click.
///
/// A menu inside a menu is drawn as a heading with its entries indented under it rather than as a
/// second list that opens sideways. Recent Projects and the explorer's `Git` submenu are the only
/// ones, both hold a short list, and a heading with rows under it needs no hovering to reach. The
/// macOS menu bar does have a real submenu there, because that is what the platform draws.
///
/// This lives here rather than in `menu_bar` because there are three menus in Unluminous now — the bar
/// inside the window, the explorer's context menu and the gutter's — and one renderer is what stops
/// them growing three row heights.
pub fn menu_rows(ui: &mut egui::Ui, entries: &[Entry], indent: f32) -> Option<Action> {
    // A menu taller than the window scrolls rather than running off the bottom of it. The Git menu
    // has twenty-two entries and does not fit in a small window; before this, its last few could
    // not be reached at all.
    let room = (ui.ctx().content_rect().height() - 120.0).max(180.0);
    // egui puts `item_spacing.y` between every row, so a count of row heights alone comes out short
    // by a third and the menu is decided to fit when it does not.
    let gap = ui.spacing().item_spacing.y;
    let height = tall(entries, gap);
    if height > room {
        return egui::ScrollArea::vertical()
            .max_height(room)
            // Without this the box comes out about two thirds of what it was allowed, because a
            // scroll area inside a popup measures itself against the popup's own idea of how much
            // room there is rather than against the number it was given.
            .min_scrolled_height(room)
            .id_salt("unluminous-menu-scroll")
            .show(ui, |ui| rows(ui, entries, indent))
            .inner;
    }
    rows(ui, entries, indent)
}

/// The rows themselves, once it has been decided whether they scroll.
fn rows(ui: &mut egui::Ui, entries: &[Entry], indent: f32) -> Option<Action> {
    let mut chosen = None;
    for entry in entries {
        match entry {
            Entry::Separator => {
                ui.separator();
            }
            Entry::Item { name, action, shortcut, enabled, checked, .. } => {
                let keys = shortcut.map(|shortcut| shortcut.label()).unwrap_or_default();
                if menu_row(ui, name, &keys, *enabled, *checked, indent) {
                    chosen = Some(action.clone());
                }
            }
            Entry::Submenu { name, entries } => {
                menu_heading(ui, name, indent);
                if let Some(action) = rows(ui, entries, indent + 14.0) {
                    chosen = Some(action);
                }
            }
        }
    }
    chosen
}

/// How tall a run of entries is drawn, in points, counting the gap egui puts between rows.
///
/// **It recurses, because a submenu can hold a submenu.** `task-1848` put every plugin's entries under one
/// `Plugins` menu, so the rows under that heading are themselves headings with rows beneath them — and a
/// measurement that counted a submenu's entries as plain rows undercounted by a heading and a gap for each
/// one nested. Undercounting here is not a cosmetic fault: it is what decides whether the menu is given a
/// `ScrollArea`, so a menu measured as fitting when it does not is a menu whose last entries cannot be
/// reached at all.
fn tall(entries: &[Entry], gap: f32) -> f32 {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Separator => 8.0 + gap,
            Entry::Item { .. } => 24.0 + gap,
            Entry::Submenu { entries, .. } => 22.0 + gap + tall(entries, gap),
        })
        .sum()
}

/// One row of a menu: a tick when it is switched on, its name, and its keyboard shortcut on the right.
///
/// A row that cannot be used just now is drawn dimmed and takes no clicks, which is how a menu says that
/// there is nothing to undo. The accessible name is the plain wording, with no tick and no padding in it,
/// so a test can ask for `Open Folder` by name however the row happens to be decorated.
pub fn menu_row(
    ui: &mut egui::Ui,
    name: &str,
    shortcut: &str,
    enabled: bool,
    checked: bool,
    indent: f32,
) -> bool {
    let height = 24.0;
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), sense);
    if response.hovered() && enabled {
        pill(ui.painter(), rect, 4);
    }
    let painter = ui.painter();
    let tint =
        if enabled { color::text_control() } else { color::text_faint().gamma_multiply(0.6) };
    let left = rect.left() + 8.0 + indent;
    if checked {
        // Drawn, not the character at U+2713. No font in the stack Unluminous hands egui has a shape for
        // it, so it came out as the empty box a missing glyph renders as — visible against
        // `Raw Markdown` on the View menu in any capture of it, and the exact fault the style guide
        // already records for the shift symbol. `icon::tick` is the same tick every tick box in
        // Unluminous draws.
        icon::tick(painter, Pos2::new(left + 6.0, rect.center().y), color::accent());
    }
    let label =
        painter.crisp_layout_no_wrap(name.to_owned(), egui::FontId::proportional(12.5), tint);
    painter.crisp_galley(
        Pos2::new(left + 18.0, rect.center().y - label.size().y / 2.0),
        label,
        tint,
    );
    if !shortcut.is_empty() {
        let keys = painter.crisp_layout_no_wrap(
            shortcut.to_owned(),
            egui::FontId::proportional(11.5),
            color::text_faint(),
        );
        painter.crisp_galley(
            Pos2::new(rect.right() - 8.0 - keys.size().x, rect.center().y - keys.size().y / 2.0),
            keys,
            color::text_faint(),
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    response.clicked()
}

/// A heading inside a menu, which is what a menu inside a menu is drawn as inside the window.
pub fn menu_heading(ui: &mut egui::Ui, name: &str, indent: f32) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    // **A heading has a name too**, which `task-1848` is what made necessary: with every plugin's entries
    // moved under one `Plugins` menu, the plugin's own name is a heading rather than a menu in the bar —
    // so without this there is no way to ask whether `Agent-Tasks` is on the screen at all. It is a label
    // rather than a button, because a heading takes no clicks; `CLAUDE.md`'s rule is that a control with
    // no name cannot be tested, and a heading is what a submenu's title is drawn as here.
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name));
    let painter = ui.painter();
    let label = painter.crisp_layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(11.0),
        color::text_dim(),
    );
    painter.crisp_galley(
        Pos2::new(rect.left() + 8.0 + indent, rect.center().y - label.size().y / 2.0),
        label,
        color::text_dim(),
    );
}

/// A small square button holding a drawn icon.
pub fn icon_button(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    draw: fn(&egui::Painter, Pos2, Color32),
) -> bool {
    let response =
        ui.interact(area, ui.id().with(("icon-button", name)), Sense::click()).on_hover_text(name);
    if response.hovered() {
        ui.painter().rect_filled(area, CornerRadius::same(4), color::control());
    }
    draw(ui.painter(), area.center(), color::text_dim());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
    response.clicked()
}

/// A word in a bar that opens a menu when clicked, which is what `Unluminous`, `File`, `Edit` and `View` are.
pub fn bar_button(ui: &mut egui::Ui, area: Rect, name: &str, strong: bool) -> egui::Response {
    let response = ui.interact(area, ui.id().with(("bar-button", name)), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(area, CornerRadius::same(4), color::control());
    }
    let tint = if strong { color::text_strong() } else { color::text_control() };
    let painter = ui.painter();
    let label =
        painter.crisp_layout_no_wrap(name.to_owned(), egui::FontId::proportional(12.5), tint);
    painter.crisp_galley(
        Pos2::new(area.center().x - label.size().x / 2.0, area.center().y - label.size().y / 2.0),
        label,
        tint,
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
    response
}

#[cfg(test)]
mod field_sizing {
    use super::*;

    /// `task-2004`: *"ensure that the cursor fits the height of the input and that the 'Filter files'
    /// text is about the same size as the folder/file name text"*.
    ///
    /// The explorer's filter box is `view.at(24.0)` points tall and its rows are `view.at(12.5)`, so
    /// one number says both: a field's text is a fraction of its own height, and at 24 that fraction is
    /// the row size.
    #[test]
    fn the_filter_box_asks_for_exactly_the_size_the_file_names_are_drawn_at() {
        assert!((field_font_size(24.0) - 12.5).abs() < 0.1, "{}", field_font_size(24.0));
    }

    /// And at every zoom, which is why the interface's size is not asked about at all: the box carries
    /// the zoom in its height and `appearance.ui.font.size` does not.
    #[test]
    fn it_goes_on_matching_them_at_every_zoom() {
        for zoom in [0.6_f32, 1.0, 1.5, 2.0, 3.0] {
            let rows = 12.5 * zoom;
            let asked = field_font_size(24.0 * zoom);
            assert!(
                (asked - rows).abs() < 0.2 || asked >= 24.0,
                "at a zoom of {zoom} the rows are {rows} and the box asked for {asked}"
            );
        }
    }

    /// The caret is the row the font occupies, so a box that asked for a font its own height could not
    /// hold is a caret standing proud of the field at both ends — which is the other half of the report.
    #[test]
    fn the_row_a_field_asks_for_fits_inside_the_field() {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            for height in [20.0_f32, 22.0, 24.0, 26.0, 30.0, 48.0] {
                let field = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, height));
                let inside = field_text(ui, field, 8.0);
                assert!(
                    inside.rect.height() <= height - 2.0,
                    "a {height} point field asked for a {} point row",
                    inside.rect.height()
                );
                assert!(
                    field.contains_rect(inside.rect),
                    "and the strip is inside the field it was measured from"
                );
            }
        });
        output.textures_delta.clear();
    }

    /// A well several rows tall is not asking for letters half its own height, which is why the ceiling
    /// is there and why those callers pass their own font to [`field_takes_the_whole_rectangle_at`].
    #[test]
    fn a_very_tall_well_is_capped_rather_than_asking_for_enormous_letters() {
        assert_eq!(field_font_size(400.0), 24.0);
        assert_eq!(field_font_size(0.0), 6.0, "and a field dragged to nothing still has a caret");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field centres the row it is going to draw, and the size that row will be set in is an argument.
    ///
    /// `task-1914`: *"the web browser node url text is not vertically centered in the bar, so its up too
    /// high and partially clipped."* The numbers here are that report. The address bar's field is 22
    /// points tall and its words are set at 12, which is a row of about 16 — but the strip was measured
    /// with `TextStyle::Body`, which is `appearance.ui.font.size` and was **24** on the machine it was
    /// reported from, a row of 28. So the field was handed a strip taller than itself, `egui` laid the 12
    /// point text out at the **top** of it, and the address sat above centre with its top cut off by the
    /// field's own border.
    #[test]
    fn a_fields_text_row_is_measured_at_the_size_the_text_will_be_set_in() {
        let field = Rect::from_min_max(Pos2::new(100.0, 200.0), Pos2::new(400.0, 222.0));

        // The interface's own row at 24 point text, which is what the old reckoning used.
        let interface = field_text_rect_at(field, 9.0, 28.0);
        assert!(
            interface.top() < field.top() && interface.bottom() > field.bottom(),
            "a 28 point strip cannot fit in a 22 point field: {interface:?} in {field:?}",
        );

        // The size the address is really set in.
        let real = field_text_rect_at(field, 9.0, 16.0);
        assert!(field.contains_rect(real), "the strip is inside the field: {real:?} in {field:?}");
        assert!(
            (real.center().y - field.center().y).abs() <= 0.5,
            "and centred on it: {} against {}",
            real.center().y,
            field.center().y,
        );
        assert_eq!(real.left(), field.left() + 9.0, "with the caller's own inset in front of it");
    }
}
