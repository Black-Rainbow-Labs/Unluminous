//! A spreadsheet tab: one bar across the top, the grid, the sheet tabs along the bottom, and the Format
//! panel down the right when it is open. There is no ribbon. The bar holds the name box, the formula bar,
//! `Format` and `...`; everything else is in the Format panel's levels, the right click menu and the
//! workbook menu, which are all drawn from one tree (`services::spreadsheet::catalogue`).
//!
//! What a person does here is either an `Act` (the keyboard, the pointer on the grid) or the words of a
//! `plugins run spreadsheet` command (every menu entry), and both are made after drawing, through the same
//! two functions an agent's commands reach.

pub mod chart;
pub mod chart_drag;
pub mod chart_paint;
pub mod editor;
pub mod grid;
pub mod keys;
pub mod keys_b;
pub mod menus;
pub mod objects;
pub mod objects_c;
pub mod overlay_a;
pub mod panes_a;
pub mod panes_b;
pub mod panes_b_pivot;
pub mod preview;
pub mod sparkline;

use egui::{CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use unluminous_sheet::coord::{column_name, CellRef};

use crate::components::controls::{self, TextBoxes, WithHint};
use crate::components::settings_kit::{self as kit, Column};
use crate::services::plugin_ui::{Look, Request};
use crate::services::spreadsheet::actions::{self, Act};
use crate::services::spreadsheet::catalogue;
use crate::services::spreadsheet::conditional::Rules;
use crate::services::spreadsheet::config::Paper;
use crate::services::spreadsheet::view::{Edit, PanelPath};
use crate::services::spreadsheet::{commands, display, Book, Spreadsheet};
use crate::theme::icon;
use keys::Finish;
use menus::Picked;

/// The bar's height, before the tab's zoom.
const BAR: f32 = 38.0;
/// The sheet tabs' height.
const SHEETS: f32 = 30.0;
/// The Format panel's width.
const PANEL: f32 = 290.0;

/// What a tab asked for this frame, made after drawing.
#[derive(Default)]
struct Asked {
    acts: Vec<Act>,
    words: Vec<Vec<String>>,
    requests: Vec<Request>,
    /// A cell finished being typed into: write it, then move this way.
    commit: Option<(Edit, Finish)>,
}

/// Draw a workbook's tab.
pub fn show(
    sheet: &mut Spreadsheet,
    key: &str,
    ui: &mut egui::Ui,
    look: &Look<'_>,
) -> Vec<Request> {
    let area = ui.max_rect();
    if let Err(problem) = sheet.book_for_key(key) {
        let galley = ui.painter().layout(
            problem,
            egui::FontId::proportional(13.0),
            look.palette.text_dim,
            area.width() - 48.0,
        );
        ui.painter().galley(area.min + Vec2::splat(24.0), galley, look.palette.text_dim);
        return Vec::new();
    }
    sheet.follow_the_settings_file();
    let index = sheet.books.iter().position(|b| b.key == key).expect("just read");
    let copied_text = sheet.clip.as_ref().map(|c| c.text.clone());
    let list_on = sheet.config.function_list;
    let paper = sheet.config.paper;
    let enter = sheet.config.enter;
    let aggregates = sheet.config.aggregates.clone();
    let autocomplete = sheet.config.autocomplete;
    let mut asked = Asked::default();
    {
        let book = &mut sheet.books[index];
        book.view.paper = paper;
        book.view.enter = enter;
        book.view.aggregates = aggregates;
        book.view.autocomplete = autocomplete;
        draw(ui, area, book, look, copied_text.as_deref(), list_on, &mut asked);
    }
    finish(sheet, key, asked)
}

/// Lay the tab out and draw it, collecting what was asked for.
fn draw(
    ui: &mut egui::Ui,
    area: Rect,
    book: &mut Book,
    look: &Look<'_>,
    copied: Option<&str>,
    list_on: bool,
    asked: &mut Asked,
) {
    let s = look.scale();
    let bar = Rect::from_min_size(area.min, Vec2::new(area.width(), BAR * s));
    let sheets = Rect::from_min_max(Pos2::new(area.left(), area.bottom() - SHEETS * s), area.max);
    let panel_width =
        if book.view.panel.is_some() { (PANEL * s).min(area.width() * 0.45) } else { 0.0 };
    let grid_area = Rect::from_min_max(
        Pos2::new(area.left(), bar.bottom()),
        Pos2::new(area.right() - panel_width, sheets.top()),
    );
    let panel_area = Rect::from_min_max(
        Pos2::new(grid_area.right(), bar.bottom()),
        Pos2::new(area.right(), sheets.top()),
    );
    ui.painter().rect_filled(area, 0, look.ground(look.palette.editor));
    // The conditional formats, prepared again when the workbook or the sheet changed.
    if book.rules.as_ref().is_none_or(|r| r.revision != book.revision || r.sheet != book.view.sheet)
    {
        book.rules = Some(Rules::prepare(&book.session.book, book.view.sheet, book.revision));
    }
    let rules = book.rules.take().unwrap_or_default();
    let ctx = ui.ctx().clone();
    let offered = editor::offered(&ctx, book, list_on);
    // The keys first, so a key this tab reads is taken before any box draws.
    let a_box_is_ours = book.view.edit.is_some() && {
        let cell = editor::cell_box_id(&book.key);
        let bar = editor::bar_box_id(&book.key);
        ctx.memory(|m| m.has_focus(cell) || m.has_focus(bar))
    };
    let mut name_box = false;
    if look.has_the_keyboard && book.view.prompt.is_none() && book.view.book_menu.is_none() {
        if a_box_is_ours {
            let id = editor::box_with_the_caret(&ctx, book);
            let finished = keys::typing(&ctx, book, id, &offered);
            match finished {
                Finish::Typing => {}
                Finish::Cancel => book.view.edit = None,
                other => {
                    if let Some(edit) = book.view.edit.take() {
                        asked.commit = Some((edit, other));
                    }
                }
            }
        } else if !ctx.text_edit_focused() && objects_c::keys(&ctx, book, &mut asked.words) {
            // A chosen object took the key: nudged, deleted or let go.
        } else if let Some(index) =
            book.view.chart.filter(|_| ctx.input(|i| i.key_pressed(egui::Key::Delete)))
        {
            // A chosen chart is deleted with Delete, rather than the cells under it being cleared.
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete));
            asked.words.push(vec![
                "chart".to_owned(),
                "delete".to_owned(),
                (index + 1).to_string(),
            ]);
            book.view.chart = None;
        } else if !ctx.text_edit_focused() && book.view.edit.is_none() {
            let mut k = keys::Keys::default();
            keys::grid(&ctx, book, copied, &mut k);
            asked.acts.extend(k.acts);
            asked.words.extend(k.words);
            name_box = k.name_box;
        }
    }
    show_bar(ui, bar, book, look, name_box, asked);
    if is_a_chart_sheet(book) {
        show_a_chart_sheet(ui, grid_area, book, look, asked);
        show_sheet_tabs(ui, sheets, book, look, asked);
        show_floating(ui, grid_area, book, look, asked);
        return;
    }
    let pointing = book.view.edit.as_ref().is_some_and(|e| {
        let id = editor::box_with_the_caret(&ctx, book);
        let caret = keys::caret_of(&ctx, id).unwrap_or(e.text.chars().count());
        e.text.starts_with('=')
            && (e.pointing.is_some_and(|(_, end, _)| end == caret)
                || crate::services::spreadsheet::formula_text::points_at(&e.text, caret))
    });
    let mut out = grid::GridOut::default();
    grid::show(ui, grid_area, book, &rules, look, pointing, &mut out);
    book.rules = Some(rules);
    let frame = out.frame.expect("the grid always answers its frame");
    let ink = grid::Ink::of(look, book.view.paper);
    if out.pressed {
        book.view.chart = None;
    }
    let charts = chart::show(ui, book, frame, &ink, look, book.view.chart);
    if let Some(chosen) = charts.pressed {
        book.view.chart = Some(chosen);
        asked.requests.push(Request::TakeTheKeyboard(true));
    }
    if charts.menu.is_some() {
        book.view.menu = charts.menu;
    }
    asked.words.extend(charts.words);
    after_the_grid(ui, book, look, &mut out, asked);
    if let Some(col) = book.view.audit.open_filter.take() {
        book.filter_values = filter_values(book, col);
        let header = unluminous_sheet::coord::CellRef::new(
            book.session.book.sheets[book.view.sheet]
                .filter
                .as_ref()
                .map_or(0, |f| f.range.first.row),
            col,
        );
        book.view.filter_menu = Some((col, grid::filter_button(book, frame, header).left_bottom()));
    }
    if book.view.edit.as_ref().is_some_and(|e| !e.in_bar) {
        editor::show_cell_box(ui, book, frame, &ink);
    }
    editor::show_reference_outlines(ui, book, frame);
    if let Some(i) = editor::show_function_list(ui, book, frame, look, &offered) {
        complete_with(&ctx, book, &offered, i);
    }
    if let Some(edit) = &book.view.edit {
        let id = editor::box_with_the_caret(&ctx, book);
        let caret = keys::caret_of(&ctx, id).unwrap_or(edit.text.chars().count());
        editor::show_argument_hint(ui, book, frame, look, caret);
    }
    show_list_button(ui, book, frame, look, asked);
    show_input_message(ui, book, frame, look);
    show_note_under_the_pointer(ui, book, frame, look);
    panes_a::show(ui, book, frame, look, &mut asked.words);
    panes_b::show(ui, book, frame, look, &mut asked.words);
    show_sheet_tabs(ui, sheets, book, look, asked);
    // The panel is drawn from the frame after `Format` opened it, when the grid has made room for it.
    if book.view.panel.is_some() && panel_area.width() > 40.0 {
        show_panel(ui, panel_area, book, look, asked);
    }
    show_floating(ui, grid_area, book, look, asked);
    if book.view.reveal {
        grid::reveal(book, frame.cells, frame.zoom);
        book.view.reveal = false;
    }
    if out.pressed && !look.has_the_keyboard {
        asked.requests.push(Request::TakeTheKeyboard(true));
    }
    asked.requests.extend(out.requests);
}

/// Whether the sheet showing is a chart sheet: a sheet that is one chart and no cells.
fn is_a_chart_sheet(book: &Book) -> bool {
    let sheet = &book.session.book.sheets[book.view.sheet];
    let kept = sheet.kept_part.as_ref();
    kept.is_some_and(|k| k.kind == unluminous_sheet::xlsx::chart_drawing::REL_CHART_SHEET)
        && !sheet.charts.is_empty()
}

/// Draw a chart sheet's one chart over the whole area. A press chooses the chart and a right click opens
/// its menu, so every chart command is on a chart sheet as it is on a chart over cells.
fn show_a_chart_sheet(
    ui: &egui::Ui,
    area: Rect,
    book: &mut Book,
    look: &Look<'_>,
    asked: &mut Asked,
) {
    let ink = grid::Ink::of(look, book.view.paper);
    let (pressed, menu) = chart::show_chart_sheet(ui, book, area, &ink, look);
    if pressed {
        book.view.chart = Some(0);
        asked.requests.push(Request::TakeTheKeyboard(true));
    }
    if menu.is_some() {
        book.view.menu = menu;
    }
}
/// What the pointer did on the grid: start or steer typing, open menus, and finish a cell being typed into
/// when another cell is clicked.
fn after_the_grid(
    ui: &egui::Ui,
    book: &mut Book,
    _look: &Look<'_>,
    out: &mut grid::GridOut,
    asked: &mut Asked,
) {
    let ctx = ui.ctx();
    asked.acts.append(&mut out.acts);
    if let Some(range) = out.point {
        // Pointing at a figure of a PivotTable writes a GETPIVOTDATA call, as Excel does.
        let pivot_call = (range.len() == 1)
            .then(|| {
                crate::services::spreadsheet::verbs_b_pivot_more::getpivotdata_text(
                    book,
                    book.view.sheet,
                    range.first,
                )
            })
            .flatten();
        if let Some(edit) = book.view.edit.as_mut() {
            let id = if edit.in_bar {
                editor::bar_box_id(&book.key)
            } else {
                editor::cell_box_id(&book.key)
            };
            let caret = keys::caret_of(ctx, id).unwrap_or(edit.text.chars().count());
            let (start, end) = match (edit.pointing, out.point_how) {
                // The same drag moved on, or Shift grew it: in place of what it put in.
                (Some((start, end, _)), grid::PointHow::Again) => (start, end),
                // Ctrl adds another reference after the one pointed at, with a comma between.
                (Some((_, end, _)), grid::PointHow::Add) => {
                    let at = keys::put_text(edit, end, end, ",");
                    (at, at)
                }
                (Some((start, end, _)), _) if end == caret => (start, end),
                _ => (caret, caret),
            };
            let new_end = match &pivot_call {
                Some(call) => keys::put_text(edit, start, end, call),
                None => keys::put_reference(edit, start, end, range),
            };
            edit.focus = true;
            edit.caret = Some(new_end);
        }
        return;
    }
    if out.pressed {
        // A press on another cell finishes the one being typed into, as Enter would without moving.
        if let Some(edit) = book.view.edit.take() {
            asked.commit = Some((edit, Finish::Commit { rows: 0, cols: 0 }));
        }
        book.view.close_the_menus();
    }
    if let Some(at) = out.edit {
        book.view.select_cell(at);
        keys::start_editing(book, false);
    }
    if let Some(p) = out.menu {
        book.view.menu = Some(p);
    }
    asked.words.append(&mut out.words);
    if let Some((p, items)) = out.drop_menu.take() {
        book.view.drop_menu = Some((p, items));
    }
    if let Some((col, p)) = out.filter_menu {
        book.filter_values = filter_values(book, col);
        book.view.filter_menu = Some((col, p));
    }
}

/// The values in a filtered column, each ticked when the filter keeps it.
fn filter_values(book: &Book, col: u32) -> Vec<(String, bool)> {
    let s = &book.session.book.sheets[book.view.sheet];
    let Some(filter) = &s.filter else { return Vec::new() };
    let criteria = filter.columns.get(&(col - filter.range.first.col));
    let mut values: Vec<String> = Vec::new();
    for row in filter.range.first.row + 1..=filter.range.last.row {
        let at = CellRef::new(row, col);
        let text = display::shown(&book.session.book, s, at, 255).text;
        if !values.contains(&text) {
            values.push(text);
        }
    }
    values.sort_by_key(|v| v.to_lowercase());
    values
        .into_iter()
        .map(|v| {
            let keep = match criteria {
                Some(unluminous_sheet::model::FilterCriteria::Values { values, blank }) => {
                    if v.is_empty() {
                        *blank
                    } else {
                        values.iter().any(|x| x.eq_ignore_ascii_case(&v))
                    }
                }
                _ => true,
            };
            (v, keep)
        })
        .collect()
}

/// Put the chosen function into the formula being typed.
fn complete_with(ctx: &egui::Context, book: &mut Book, offered: &keys::Offered, index: usize) {
    let id = editor::box_with_the_caret(ctx, book);
    let Some(edit) = book.view.edit.as_mut() else { return };
    let Some(name) = offered.names.get(index) else { return };
    let caret = keys::caret_of(ctx, id).unwrap_or(edit.text.chars().count());
    let chars: Vec<char> = edit.text.chars().collect();
    let mut text: String = chars[..offered.start.min(chars.len())].iter().collect();
    text.push_str(name);
    text.push('(');
    let new_caret = text.chars().count();
    text.extend(chars[caret.min(chars.len())..].iter());
    edit.text = text;
    edit.focus = true;
    edit.caret = Some(new_caret);
}

/// The bar: the name box, the formula bar, `Format` and `...`.
fn show_bar(
    ui: &mut egui::Ui,
    bar: Rect,
    book: &mut Book,
    look: &Look<'_>,
    focus_name_box: bool,
    asked: &mut Asked,
) {
    let painter = ui.painter_at(bar);
    painter.rect_filled(bar, 0, look.ground(look.palette.panel));
    painter.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        Stroke::new(1.0, look.palette.divider),
    );
    let pad = 6.0;
    let height = bar.height() - 2.0 * pad;
    let font = controls::field_font(height);
    // The name box.
    let name_rect = Rect::from_min_size(bar.min + Vec2::new(pad, pad), Vec2::new(110.0, height));
    let name_id = egui::Id::new(("spreadsheet-name-box", &book.key));
    let mut typed: String = ui.ctx().data(|d| d.get_temp::<String>(name_id)).unwrap_or_default();
    let focused = ui.ctx().memory(|m| m.has_focus(name_id.with("box")));
    if !focused {
        typed = book.view.range().a1();
        if let Some(name) = name_of(book) {
            typed = name;
        }
    }
    painter.rect(
        name_rect,
        CornerRadius::same(8),
        look.palette.field,
        Stroke::new(1.0, look.palette.control_border),
        egui::StrokeKind::Inside,
    );
    let response = ui.text_box_at(
        name_rect.shrink2(Vec2::new(8.0, 2.0)),
        egui::TextEdit::singleline(&mut typed)
            .id(name_id.with("box"))
            .frame(egui::Frame::NONE)
            .font(font.clone()),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Name box"));
    if focus_name_box {
        response.request_focus();
    }
    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        let text = typed.trim().to_owned();
        if !text.is_empty() {
            // A range or a name goes there; anything else names the chosen cells.
            if commands::parse_range(book, &text).is_some() {
                asked.words.push(vec!["select".to_owned(), text]);
            } else {
                asked.words.push(vec!["name".to_owned(), text]);
            }
        }
        asked.requests.push(Request::TakeTheKeyboard(true));
    }
    ui.ctx().data_mut(|d| d.insert_temp(name_id, typed));
    // `fx`, then the formula bar.
    let buttons = 132.0;
    let fx = Rect::from_min_size(
        Pos2::new(name_rect.right() + 6.0, name_rect.top()),
        Vec2::new(28.0, height),
    );
    painter.text(
        fx.center(),
        egui::Align2::CENTER_CENTER,
        "fx",
        egui::FontId::proportional(13.0).clone(),
        look.palette.text_dim,
    );
    let formula = Rect::from_min_max(
        Pos2::new(fx.right() + 4.0, name_rect.top()),
        Pos2::new(bar.right() - buttons, name_rect.bottom()),
    );
    painter.rect(
        formula,
        CornerRadius::same(8),
        look.palette.field,
        Stroke::new(
            1.0,
            if book.view.edit.as_ref().is_some_and(|e| e.in_bar) {
                look.palette.accent
            } else {
                look.palette.control_border
            },
        ),
        egui::StrokeKind::Inside,
    );
    let inside = formula.shrink2(Vec2::new(10.0, 2.0));
    let bar_id = editor::bar_box_id(&book.key);
    match book.view.edit.as_mut() {
        Some(edit) => {
            // A formula longer than the bar, or one with line breaks, grows the bar down over the grid while it
            // is typed in, as Excel's formula bar does.
            let width = ui
                .painter()
                .layout_no_wrap(edit.text.clone(), font.clone(), look.palette.text)
                .size()
                .x;
            let grows = edit.in_bar && (width > inside.width() || edit.text.contains('\n'));
            let response = if grows {
                let lines = (width / inside.width().max(1.0))
                    .ceil()
                    .max(edit.text.split('\n').count() as f32)
                    .clamp(2.0, 10.0);
                let tall = Rect::from_min_size(
                    formula.min,
                    Vec2::new(formula.width(), lines * (font.size * 1.35) + 10.0),
                );
                egui::Area::new(egui::Id::new(("spreadsheet-tall-bar", &book.key)))
                    .order(egui::Order::Foreground)
                    .fixed_pos(tall.min)
                    .show(ui.ctx(), |ui| {
                        ui.painter().rect(
                            tall,
                            CornerRadius::same(8),
                            look.palette.field,
                            Stroke::new(1.0, look.palette.accent),
                            egui::StrokeKind::Inside,
                        );
                        ui.text_box_at(
                            tall.shrink2(Vec2::new(10.0, 5.0)),
                            egui::TextEdit::multiline(&mut edit.text)
                                .id(bar_id)
                                .frame(egui::Frame::NONE)
                                .font(font.clone())
                                .lock_focus(true)
                                .desired_width(tall.width() - 20.0)
                                .return_key(Some(egui::KeyboardShortcut::new(
                                    egui::Modifiers::ALT,
                                    egui::Key::Enter,
                                ))),
                        )
                    })
                    .inner
            } else {
                ui.text_box_at(
                    inside,
                    egui::TextEdit::singleline(&mut edit.text)
                        .id(bar_id)
                        .frame(egui::Frame::NONE)
                        .font(font.clone())
                        .lock_focus(true),
                )
            };
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Formula bar")
            });
            if response.gained_focus() {
                edit.in_bar = true;
                edit.typed = false;
            }
            if edit.focus && edit.in_bar {
                response.request_focus();
                keys::set_caret(
                    ui.ctx(),
                    bar_id,
                    edit.caret.take().unwrap_or(edit.text.chars().count()),
                );
                edit.focus = false;
            }
        }
        None => {
            let s = &book.session.book;
            let shown = display::input_text(s, &s.sheets[book.view.sheet], book.view.cursor);
            let response =
                ui.interact(formula, ui.id().with(("formula-bar", &book.key)), Sense::click());
            let text = controls::truncate_chars(&shown.replace('\n', " "), 400, 396);
            painter.text(
                Pos2::new(inside.left(), inside.center().y),
                egui::Align2::LEFT_CENTER,
                text,
                font.clone(),
                look.palette.text_strong,
            );
            let response = response.with_hint("The active cell's contents. Click to change them.");
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Formula bar")
            });
            if response.clicked() {
                keys::start_editing(book, true);
                asked.requests.push(Request::TakeTheKeyboard(true));
            }
        }
    }
    // `Format`, which opens the panel, and `...`, which opens the workbook menu.
    let format = Rect::from_min_size(
        Pos2::new(bar.right() - buttons + 8.0, name_rect.top()),
        Vec2::new(80.0, height),
    );
    let lit = book.view.panel.is_some();
    let response = ui.interact(format, ui.id().with(("format-button", &book.key)), Sense::click());
    let fill = if lit {
        look.palette.selected_row
    } else if response.hovered() {
        look.palette.control
    } else {
        look.ground(look.palette.panel)
    };
    painter.rect(
        format,
        CornerRadius::same(13),
        fill,
        Stroke::new(1.0, look.palette.control_border),
        egui::StrokeKind::Inside,
    );
    painter.text(
        format.center(),
        egui::Align2::CENTER_CENTER,
        "Format",
        egui::FontId::proportional(12.5),
        look.palette.text_strong,
    );
    let response =
        response.with_hint("Number, font, fill, borders, alignment, rules and data (Ctrl+1)");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Format"));
    if response.clicked() {
        book.view.panel = if lit { None } else { Some(PanelPath(Vec::new())) };
    }
    let more = Rect::from_min_size(
        Pos2::new(format.right() + 8.0, name_rect.top()),
        Vec2::new(height, height),
    );
    if controls::icon_button(ui, more, "Workbook menu", icon::more) {
        book.view.book_menu = Some(String::new());
        book.view.menu = None;
    }
}

/// The defined name that is exactly the chosen range, which the name box shows instead of the address.
fn name_of(book: &Book) -> Option<String> {
    let range = book.view.range();
    let sheet = &book.session.book.sheets[book.view.sheet].name;
    book.session.book.names.iter().find_map(|n| {
        let (s, area) = n.formula.rsplit_once('!')?;
        (s.trim_matches('\'') == sheet
            && unluminous_sheet::coord::Range::parse(&area.replace('$', "")) == Some(range))
        .then(|| n.name.clone())
    })
}

/// The arrow beside a cell with a list of allowed values, and the list it opens.
fn show_list_button(
    ui: &mut egui::Ui,
    book: &mut Book,
    frame: grid::Frame,
    look: &Look<'_>,
    asked: &mut Asked,
) {
    let at = book.view.cursor;
    let has_list =
        book.session.validation_at(book.view.sheet, at).is_some_and(|v| v.kind == "list");
    if (!has_list && !book.view.choices) || book.view.edit.is_some() {
        return;
    }
    let cell = frame.cell(book, at);
    let button = Rect::from_min_size(
        Pos2::new(cell.right() + 2.0, cell.top()),
        Vec2::new(18.0, cell.height().max(16.0)),
    );
    let response = ui.interact(button, ui.id().with(("list-button", &book.key)), Sense::click());
    if has_list {
        ui.painter().rect(
            button,
            CornerRadius::same(3),
            look.palette.control,
            Stroke::new(1.0, look.palette.control_border),
            egui::StrokeKind::Inside,
        );
        icon::chevron_down(ui.painter(), button.center(), look.palette.text_dim);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Allowed values")
        });
        if response.clicked() {
            book.view.choices = !book.view.choices;
        }
    }
    if book.view.choices {
        let values = if has_list {
            book.session.list_choices(book.view.sheet, at)
        } else {
            let sheet = &book.session.book.sheets[book.view.sheet];
            crate::services::spreadsheet::entry::column_entries(sheet, at)
        };
        let id = egui::Id::new(("spreadsheet-choices", &book.key));
        match menus::choices(
            ui.ctx(),
            id,
            cell.left_bottom() + Vec2::new(0.0, 2.0),
            cell.width(),
            &values,
        ) {
            Some(Some(value)) => {
                book.view.choices = false;
                asked.acts.push(Act::Type { at, text: value });
            }
            Some(None) => book.view.choices = false,
            None => {}
        }
    }
}

/// The message a validation rule shows while its cell is the active one, as Excel's input message does.
fn show_input_message(ui: &egui::Ui, book: &Book, frame: grid::Frame, look: &Look<'_>) {
    if book.view.edit.is_some() || book.view.choices {
        return;
    }
    let at = book.view.cursor;
    let Some(rule) = book.session.validation_at(book.view.sheet, at) else { return };
    let Some(message) = rule.prompt.as_ref().filter(|m| !m.is_empty()) else { return };
    let cell = frame.cell(book, at);
    egui::Area::new(egui::Id::new(("spreadsheet-input-message", &book.key)))
        .order(egui::Order::Tooltip)
        .fixed_pos(cell.left_bottom() + Vec2::new(12.0, 6.0))
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .fill(look.palette.menu)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_max_width(240.0);
                    if let Some(title) = rule.prompt_title.as_ref().filter(|t| !t.is_empty()) {
                        ui.label(
                            egui::RichText::new(title)
                                .strong()
                                .size(12.0)
                                .color(look.palette.text_strong),
                        );
                    }
                    ui.label(egui::RichText::new(message).size(12.0).color(look.palette.text));
                });
        });
}

/// A cell's note, shown while the pointer rests on the cell.
fn show_note_under_the_pointer(ui: &egui::Ui, book: &Book, frame: grid::Frame, look: &Look<'_>) {
    let Some(p) = ui.ctx().pointer_hover_pos() else { return };
    if !frame.cells.contains(p) || book.view.edit.is_some() || book.view.menu.is_some() {
        return;
    }
    let at = frame.at(book, p);
    let s = &book.session.book.sheets[book.view.sheet];
    let Some(note) = s.notes.get(&(at.row, at.col)) else { return };
    let cell = frame.cell(book, at);
    egui::Area::new(egui::Id::new(("spreadsheet-note", &book.key)))
        .order(egui::Order::Tooltip)
        .fixed_pos(cell.right_top() + Vec2::new(8.0, 0.0))
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .fill(look.palette.menu)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_max_width(260.0);
                    if !note.author.is_empty() {
                        ui.label(
                            egui::RichText::new(&note.author)
                                .strong()
                                .size(12.0)
                                .color(look.palette.text_strong),
                        );
                    }
                    ui.label(egui::RichText::new(&note.text).size(12.0).color(look.palette.text));
                });
        });
}

/// The sheet tabs, the button that adds one, and the selection's sum, average and count.
fn show_sheet_tabs(
    ui: &mut egui::Ui,
    strip: Rect,
    book: &mut Book,
    look: &Look<'_>,
    asked: &mut Asked,
) {
    let painter = ui.painter_at(strip);
    painter.rect_filled(strip, 0, look.ground(look.palette.panel));
    painter.line_segment(
        [strip.left_top(), strip.right_top()],
        Stroke::new(1.0, look.palette.divider),
    );
    let add =
        Rect::from_center_size(Pos2::new(strip.left() + 18.0, strip.center().y), Vec2::splat(22.0));
    if controls::icon_button(ui, add, "Add a sheet", icon::plus) {
        asked.acts.push(Act::AddSheet);
    }
    let font = egui::FontId::proportional(12.5);
    let mut x = add.right() + 8.0;
    let summary = summary_text(book);
    let summary_width = ui
        .painter()
        .layout_no_wrap(summary.clone(), egui::FontId::proportional(11.5), look.palette.text_dim)
        .size()
        .x;
    let limit = strip.right() - summary_width - 24.0;
    let sheets: Vec<(usize, String, Option<unluminous_sheet::styles::Color>)> = book
        .session
        .book
        .sheets
        .iter()
        .enumerate()
        .filter(|(_, s)| s.state == unluminous_sheet::model::Visibility::Visible)
        .map(|(i, s)| (i, s.name.clone(), s.tab_color))
        .collect();
    let theme = book.session.book.styles.theme.clone();
    let mut placed: Vec<(usize, Rect)> = Vec::new();
    let mut dropped: Option<(usize, f32)> = None;
    for (i, name, color) in sheets {
        let width =
            ui.painter().layout_no_wrap(name.clone(), font.clone(), look.palette.text).size().x
                + 26.0;
        if x + width > limit {
            break;
        }
        let rect = Rect::from_min_size(
            Pos2::new(x, strip.top() + 3.0),
            Vec2::new(width, strip.height() - 6.0),
        );
        x += width + 4.0;
        let showing = i == book.view.sheet;
        if let Some((renaming, text)) = book.view.renaming.as_mut().filter(|(r, _)| *r == i) {
            let _ = renaming;
            let id = egui::Id::new(("spreadsheet-rename", &book.key, i));
            let response = ui.text_box_at(
                rect.shrink2(Vec2::new(6.0, 2.0)),
                egui::TextEdit::singleline(text).id(id).font(font.clone()),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Sheet name")
            });
            if !response.has_focus() && !response.lost_focus() {
                response.request_focus();
            }
            if response.lost_focus() {
                let new_name = text.trim().to_owned();
                if ui.input(|i| !i.key_pressed(egui::Key::Escape))
                    && !new_name.is_empty()
                    && new_name != name
                {
                    asked.acts.push(Act::RenameSheet(i, new_name));
                }
                book.view.renaming = None;
            }
            continue;
        }
        placed.push((i, rect));
        let response =
            ui.interact(rect, ui.id().with(("sheet-tab", &book.key, i)), Sense::click_and_drag());
        // A tab dragged along the strip moves its sheet to where it is let go.
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                ui.painter().line_segment(
                    [Pos2::new(p.x, strip.top() + 4.0), Pos2::new(p.x, strip.bottom() - 4.0)],
                    Stroke::new(2.0, look.palette.accent),
                );
            }
        }
        if response.drag_stopped() {
            if let Some(p) = response.interact_pointer_pos() {
                dropped = Some((i, p.x));
            }
        }
        let fill = if showing {
            look.ground(look.palette.editor)
        } else if response.hovered() {
            look.palette.control
        } else {
            look.ground(look.palette.panel)
        };
        ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
        if showing {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(7),
                Stroke::new(1.0, look.palette.control_border),
                egui::StrokeKind::Inside,
            );
        }
        if let Some((r, g, b)) = color.and_then(|c| c.resolve(&theme)) {
            let stripe = Rect::from_min_max(
                Pos2::new(rect.left() + 6.0, rect.bottom() - 3.0),
                Pos2::new(rect.right() - 6.0, rect.bottom() - 1.0),
            );
            ui.painter().rect_filled(stripe, 1, egui::Color32::from_rgb(r, g, b));
        }
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &name,
            font.clone(),
            if showing { look.palette.text_strong } else { look.palette.text_dim },
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Sheet {name}"))
        });
        if response.clicked() && !showing {
            commit_open_edit(book, asked);
            asked.acts.push(Act::ShowSheet(i));
        }
        if response.double_clicked() {
            book.view.renaming = Some((i, name.clone()));
        }
        if response.secondary_clicked() {
            book.view.sheet_menu =
                Some((i, response.interact_pointer_pos().unwrap_or(rect.left_top())));
        }
    }
    if let Some((from, x)) = dropped {
        let before = placed.iter().filter(|(_, r)| r.center().x < x).count();
        let target = placed
            .get(before.min(placed.len().saturating_sub(1)))
            .map(|(index, _)| *index)
            .unwrap_or(from);
        let target = if before >= placed.len() {
            placed.last().map_or(from, |(index, _)| *index)
        } else if target > from {
            target.saturating_sub(1)
        } else {
            target
        };
        if target != from {
            asked.acts.push(Act::MoveSheet(from, target));
        }
    }
    let summary_rect =
        Rect::from_min_max(Pos2::new(strip.right() - summary_width - 16.0, strip.top()), strip.max);
    let response = ui.interact(summary_rect, ui.id().with(("summary", &book.key)), Sense::click());
    ui.painter().text(
        Pos2::new(strip.right() - 12.0, strip.center().y),
        egui::Align2::RIGHT_CENTER,
        summary,
        egui::FontId::proportional(11.5),
        if response.hovered() { look.palette.text_strong } else { look.palette.text_dim },
    );
    let response = response.with_hint("Copy the sum");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Selection summary")
    });
    if response.clicked() {
        let total = book.session.summary(book.view.sheet, &book.view.ranges());
        if total.numbers > 0 {
            asked.requests.push(Request::Copy(unluminous_sheet::value::number_to_text(total.sum)));
            asked.requests.push(Request::Message(format!(
                "Copied the sum, {}",
                unluminous_sheet::value::number_to_text(total.sum)
            )));
        }
    }
}

/// Finish the cell being typed into without moving, as switching sheets does.
fn commit_open_edit(book: &mut Book, asked: &mut Asked) {
    if let Some(edit) = book.view.edit.take() {
        asked.commit = Some((edit, Finish::Commit { rows: 0, cols: 0 }));
    }
}

/// The status area: the mode (`Ready`, `Enter`, `Edit`, `Point`, and `Extend Selection` or `Add to
/// Selection`), then for a selection of more than one cell the figures the settings choose, `Average 4
/// Count 3 Sum 12`, as Excel's status bar shows, then the zoom.
pub fn summary_text(book: &Book) -> String {
    let mut parts = vec![mode_text(book)];
    let ranges = book.view.ranges();
    if ranges.iter().map(|r| r.len()).sum::<u64>() > 1 {
        let summary = book.session.summary(book.view.sheet, &ranges);
        let n = |v: f64| unluminous_sheet::numfmt::general(v, 11);
        for name in &book.view.aggregates {
            let figure = match name.as_str() {
                "average" => summary.average().map(|a| format!("Average {}", n(a))),
                "count" => (summary.count > 0).then(|| format!("Count {}", summary.count)),
                "numbers" => {
                    (summary.numbers > 0).then(|| format!("Numerical Count {}", summary.numbers))
                }
                "min" => summary.min.map(|m| format!("Min {}", n(m))),
                "max" => summary.max.map(|m| format!("Max {}", n(m))),
                "sum" => (summary.numbers > 0).then(|| format!("Sum {}", n(summary.sum))),
                _ => None,
            };
            parts.extend(figure);
        }
    }
    parts.push(format!("{}%", (book.view.zoom * 100.0).round()));
    parts.join("    ")
}

/// Excel's cell mode: what typing does now.
pub fn mode_text(book: &Book) -> String {
    use crate::services::spreadsheet::view::SelectMode;
    let mode = book.view.mode();
    match book.view.selecting {
        SelectMode::Extend => format!("{mode}    Extend Selection"),
        SelectMode::Add => format!("{mode}    Add to Selection"),
        SelectMode::Normal if book.view.end_mode => format!("{mode}    End Mode"),
        SelectMode::Normal => mode.to_owned(),
    }
}

/// The Format panel at its level.
fn show_panel(ui: &mut egui::Ui, area: Rect, book: &mut Book, look: &Look<'_>, asked: &mut Asked) {
    let root = catalogue::panel_root(book);
    let path = book.view.panel.clone().map(|p| p.0).unwrap_or_default();
    let what = format!(
        "{} on {}",
        book.view.ranges().iter().map(|r| r.a1()).collect::<Vec<_>>().join(", "),
        book.session.book.sheets[book.view.sheet].name
    );
    let details = catalogue::details(book);
    let everything = catalogue::everything(book);
    match menus::panel(ui, area, look, &root, &path, &what, &details, &everything) {
        Some(Picked::Close) => book.view.panel = None,
        Some(Picked::Enter(label)) => {
            let mut path = path;
            if label == ".." {
                path.pop();
            } else {
                path.push(label);
            }
            book.view.panel = Some(PanelPath(path));
        }
        Some(Picked::Words(words)) => asked.words.push(words),
        Some(Picked::Ask(prompt)) => book.view.prompt = Some(prompt),
        None => {}
    }
}

/// The menus and boxes that float over the grid.
fn show_floating(
    ui: &mut egui::Ui,
    grid_area: Rect,
    book: &mut Book,
    look: &Look<'_>,
    asked: &mut Asked,
) {
    let ctx = ui.ctx().clone();
    let mut picked = None;
    if let Some(at) = book.view.menu {
        let items = catalogue::cell_menu(book);
        picked = menus::popup(
            &ctx,
            look,
            egui::Id::new(("spreadsheet-cell-menu", &book.key)),
            at,
            &items,
            240.0,
        );
        if picked.is_some() {
            book.view.menu = None;
        }
    }
    if let Some((at, items)) = book.view.drop_menu.clone() {
        let got = menus::popup(
            &ctx,
            look,
            egui::Id::new(("spreadsheet-drop-menu", &book.key)),
            at,
            &items,
            240.0,
        );
        if got.is_some() {
            book.view.drop_menu = None;
            picked = got;
        }
    }
    if let Some((index, at)) = book.view.sheet_menu {
        let items = catalogue::sheet_menu(book, index);
        let got = menus::popup(
            &ctx,
            look,
            egui::Id::new(("spreadsheet-sheet-menu", &book.key)),
            at - Vec2::new(0.0, items.len() as f32 * controls::MENU_ROW + 24.0),
            &items,
            220.0,
        );
        if got.is_some() {
            book.view.sheet_menu = None;
            picked = got;
        }
    }
    if let Some(mut search) = book.view.book_menu.clone() {
        let items = catalogue::everything(book);
        let at = Pos2::new(grid_area.right() - 300.0, grid_area.top() + 2.0);
        let got = menus::book_menu(
            &ctx,
            look,
            egui::Id::new(("spreadsheet-book-menu", &book.key)),
            at,
            &items,
            &mut search,
        );
        book.view.book_menu = Some(search);
        if got.is_some() {
            book.view.book_menu = None;
            picked = got;
        }
    }
    if let Some((col, at)) = book.view.filter_menu {
        let mut values = std::mem::take(&mut book.filter_values);
        let got = menus::filter_menu(
            &ctx,
            look,
            egui::Id::new(("spreadsheet-filter-menu", &book.key)),
            at,
            &mut values,
            &column_name(col),
        );
        book.filter_values = values;
        if got.is_some() {
            book.view.filter_menu = None;
            picked = got;
        }
    }
    match picked {
        Some(Picked::Words(words)) => asked.words.push(words),
        Some(Picked::Ask(prompt)) => book.view.prompt = Some(prompt),
        _ => {}
    }
    if let Some(mut prompt) = book.view.prompt.take() {
        let at = Pos2::new(grid_area.center().x - 170.0, grid_area.top() + 40.0);
        match menus::prompt_box(
            &ctx,
            look,
            egui::Id::new(("spreadsheet-prompt", &book.key)),
            at,
            &mut prompt,
        ) {
            Some(menus::Answered::Ok(words)) => {
                asked.words.push(words);
                asked.requests.push(Request::TakeTheKeyboard(true));
            }
            Some(menus::Answered::Cancel) => {}
            None => book.view.prompt = Some(prompt),
        }
    }
    if let Some(mut find) = book.view.find.take() {
        let height = if find.replace.is_some() { 72.0 } else { 40.0 };
        let area = Rect::from_min_size(
            Pos2::new(grid_area.right() - 420.0, grid_area.top() + 30.0),
            Vec2::new(400.0, height),
        );
        let matches = if find.text.is_empty() {
            Vec::new()
        } else {
            book.session.find(Some(book.view.sheet), &find.text, true, false, find.case)
        };
        let mut keep = true;
        match menus::find_bar(ui, area, look, &mut find, matches.len()) {
            Some(menus::Found::Next(forward)) if !matches.is_empty() => {
                let count = matches.len();
                find.at =
                    if forward { (find.at + 1) % count } else { (find.at + count - 1) % count };
                let (_, at) = matches[find.at.min(count - 1)];
                book.view.select_cell(at);
            }
            Some(menus::Found::Replace) => {
                if let (Some(with), Some((_, at))) = (
                    find.replace.clone(),
                    matches.get(find.at.min(matches.len().saturating_sub(1))),
                ) {
                    let s = &book.session.book;
                    let text = display::input_text(s, &s.sheets[book.view.sheet], *at);
                    let replaced = replace_insensitive(&text, &find.text, &with, find.case);
                    asked.acts.push(Act::Type { at: *at, text: replaced });
                }
            }
            Some(menus::Found::ReplaceAll) => {
                if let Some(with) = find.replace.clone() {
                    asked.acts.push(Act::Replace {
                        find: find.text.clone(),
                        with,
                        case: find.case,
                        whole_book: find.whole_book,
                    });
                }
            }
            Some(menus::Found::Close) => {
                keep = false;
                asked.requests.push(Request::TakeTheKeyboard(true));
            }
            _ => {}
        }
        if keep {
            book.view.find = Some(find);
        }
    }
}

/// Replace every match of `find` in `text`, ignoring case unless asked not to.
fn replace_insensitive(text: &str, find: &str, with: &str, case: bool) -> String {
    if find.is_empty() {
        return text.to_owned();
    }
    if case {
        return text.replace(find, with);
    }
    let lower = text.to_lowercase();
    let needle = find.to_lowercase();
    let mut out = String::new();
    let mut at = 0;
    while let Some(found) = lower[at..].find(&needle) {
        out.push_str(&text[at..at + found]);
        out.push_str(with);
        at += found + needle.len();
    }
    out.push_str(&text[at..]);
    out
}

/// Make what was asked for, after the drawing: the keyboard's and the pointer's changes, then the menus'
/// commands.
fn finish(sheet: &mut Spreadsheet, key: &str, asked: Asked) -> Vec<Request> {
    let mut requests = asked.requests;
    let Some(index) = sheet.books.iter().position(|b| b.key == key) else { return requests };
    if let Some((edit, how)) = asked.commit {
        let config = sheet.config.clone();
        let book = &mut sheet.books[index];
        let mut copied = sheet.clip.take();
        let typed = typed_with_options(book, &edit, &config);
        let act = match how {
            Finish::CommitEverywhere => Act::TypeEverywhere(edit.text.clone()),
            Finish::CommitArray => Act::ArrayFormula(edit.text.clone()),
            _ => Act::Type { at: edit.at, text: typed.clone() },
        };
        let link = (config.auto_links && matches!(how, Finish::Commit { .. }))
            .then(|| crate::services::spreadsheet::entry::typed_link(&typed))
            .flatten();
        match actions::apply(book, &mut copied, act) {
            Ok(_) => {
                if let Some(unluminous_sheet::model::LinkTarget::External(target)) = link {
                    let at = book.view.cursor;
                    book.view.select_cell(edit.at);
                    let _ = actions::apply(
                        book,
                        &mut copied,
                        Act::Link { target: Some(target), text: None },
                    );
                    book.view.select_cell(at);
                }
                if let Finish::Commit { rows, cols } = how {
                    if rows != 0 || cols != 0 {
                        book.view.select_cell(edit.at);
                        keys::step_within(book, rows, cols);
                    }
                }
            }
            Err(problem) => {
                // The cell stays open with what was typed, so it can be put right.
                book.view.edit = Some(Edit { focus: true, ..edit });
                requests.push(Request::Notice {
                    text: problem,
                    kind: crate::components::toast::Kind::Problem,
                });
            }
        }
        sheet.clip = copied;
    }
    for act in asked.acts {
        let book = &mut sheet.books[index];
        let mut copied = sheet.clip.take();
        let is_copy = matches!(act, Act::Copy { .. });
        match actions::apply(book, &mut copied, act) {
            Ok(said) if is_copy => requests.push(Request::Copy(said)),
            Ok(said) if !said.is_empty() => requests.push(Request::Message(said)),
            Ok(_) => {}
            Err(problem) => requests.push(Request::Notice {
                text: problem,
                kind: crate::components::toast::Kind::Problem,
            }),
        }
        sheet.clip = copied;
    }
    for words in asked.words {
        let Some((verb, rest)) = words.split_first() else { continue };
        sheet.current = Some(key.to_owned());
        match commands::run(sheet, verb, rest) {
            Ok(answer) if !answer.message.is_empty() => {
                requests.push(Request::Message(answer.message))
            }
            Ok(_) => {}
            Err(problem) => requests.push(Request::Notice {
                text: problem,
                kind: crate::components::toast::Kind::Problem,
            }),
        }
    }
    requests.extend(sheet.asking());
    requests
}

/// What typing into a cell writes once Excel's options have had their turn: AutoComplete finishes the
/// one entry of the column the text starts, and AutoCorrect fixes the common slips.
fn typed_with_options(
    book: &Book,
    edit: &Edit,
    config: &crate::services::spreadsheet::config::Config,
) -> String {
    use crate::services::spreadsheet::entry;
    let sheet = &book.session.book.sheets[edit.sheet.min(book.session.book.sheets.len() - 1)];
    let mut text = edit.text.clone();
    if config.autocomplete && !edit.no_complete {
        if let Some(full) = entry::complete(sheet, edit.at, &text) {
            text = full;
        }
    }
    if config.autocorrect {
        text = entry::autocorrect(&text);
    }
    text
}

/// The list the plugin's pane would hold, if a pane is ever asked for: the workbooks open.
pub fn books_list(sheet: &mut Spreadsheet, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    let mut requests = Vec::new();
    let area = ui.max_rect();
    let mut y = area.top() + 12.0;
    if sheet.books.is_empty() {
        ui.painter().text(
            Pos2::new(area.left() + 12.0, y),
            egui::Align2::LEFT_TOP,
            "No workbook is open. Open a .xlsx or .csv file from the explorer.",
            egui::FontId::proportional(12.5),
            look.palette.text_dim,
        );
        return requests;
    }
    for book in &sheet.books {
        let rect = Rect::from_min_size(
            Pos2::new(area.left() + 6.0, y),
            Vec2::new(area.width() - 12.0, look.row_height),
        );
        let response = ui.interact(rect, ui.id().with(("book", &book.key)), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(8), look.palette.control);
        }
        ui.painter().text(
            rect.left_center() + Vec2::new(8.0, 0.0),
            egui::Align2::LEFT_CENTER,
            book.name(),
            egui::FontId::proportional(12.5),
            look.palette.text,
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, book.name()));
        if response.clicked() {
            requests.push(Request::OpenFile(book.path.clone()));
        }
        y += look.row_height;
    }
    requests
}

/// The Spreadsheet page in Settings: what a sheet is drawn on, and whether the function list opens.
pub fn settings_page(sheet: &mut Spreadsheet, ui: &mut egui::Ui, _look: &Look<'_>) -> Vec<Request> {
    kit::plugin_page(ui, "spreadsheet-settings", |ui, column| settings_cards(sheet, ui, column))
}

fn settings_cards(sheet: &mut Spreadsheet, ui: &mut egui::Ui, column: &mut Column) -> Vec<Request> {
    let mut requests = Vec::new();
    let mut changed = false;
    let help = Some("How a workbook is drawn and what opens while a formula is typed.");
    kit::card(ui, column, "Spreadsheets", help, |card| {
        let options = ["Theme", "Light paper"];
        let chosen = Some(if sheet.config.paper == Paper::Light { 1 } else { 0 });
        card.row("Sheet background", Some("Theme draws the cells on the window's own ground and lightens dark colors to read on it. Light paper is white, as Excel draws a sheet."), |row| {
            if let Some(picked) = row.segmented("Sheet background", &options, chosen, 90.0, false) {
                sheet.config.paper = if picked == 1 { Paper::Light } else { Paper::Theme };
                changed = true;
            }
        });
        let mut list = sheet.config.function_list;
        if card.switch(
            "List functions while a formula is typed",
            Some("Tab or Enter puts the chosen one in."),
            &mut list,
        ) {
            sheet.config.function_list = list;
            changed = true;
        }
    });
    if changed {
        for book in &mut sheet.books {
            book.view.paper = sheet.config.paper;
        }
        if let Err(problem) = sheet.write_the_settings() {
            requests.push(Request::Message(problem));
        }
    }
    if let Some(problem) = sheet.problem.take() {
        requests.push(Request::Message(problem));
    }
    requests
}
