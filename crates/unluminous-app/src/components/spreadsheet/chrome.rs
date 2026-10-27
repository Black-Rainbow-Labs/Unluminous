//! The spreadsheet tab's own furniture, in the window's design (`task-2382`): the bar across the top with the
//! name box, Insert Function, the formula well, `Format` and the workbook menu, and the strip along the
//! bottom with the sheets as pills in one carved track.
//!
//! Every part is drawn with `components::word::kit`, which the Word tab is drawn with too, so the two tabs
//! hold the same round chips, wells and pills as the title bar and the Settings window. Every size is the
//! design's points times the tab's own zoom, never the editor's font.

use egui::{Pos2, Rect, Sense, Stroke, Vec2};

use super::editor;
use super::keys;
use super::Asked;
use crate::components::controls::{self, TextBoxes, WithHint};
use crate::components::word::kit::{self, Mark};
use crate::services::plugin_ui::{Look, Request};
use crate::services::spreadsheet::actions::Act;
use crate::services::spreadsheet::view::PanelPath;
use crate::services::spreadsheet::{commands, display, Book};
use crate::theme::crisp::CrispPainter;
use crate::theme::svg_icon;

/// The bar's height, before the tab's zoom.
pub const BAR: f32 = 44.0;
/// The sheet strip's height.
pub const SHEETS: f32 = 40.0;
/// How wide the name box is.
const NAME_BOX: f32 = 132.0;
/// How tall a well in the bar is.
const WELL: f32 = 30.0;

/// The size words in the bar are set at.
fn text_size(z: f32) -> f32 {
    12.5 * z
}

/// A carved well with round ends, drawn behind whatever is put in it; `ring` draws the accent round it,
/// for the formula well while it is typed into.
fn well(ui: &mut egui::Ui, salt: &str, rect: Rect, ring: bool) {
    kit::layer(ui, ("sheet-well", salt), rect, |rux| {
        kit::well_surface(rux, rect, rect.height() / 2.0);
    });
    if ring {
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same((rect.height() / 2.0).round().clamp(0.0, 255.0) as u8),
            Stroke::new(1.2, kit::chat().sky),
            egui::StrokeKind::Inside,
        );
    }
}

/// The bar: the name box, Insert Function, the formula well, then `Format` and the workbook menu at the
/// right.
pub(super) fn bar(
    ui: &mut egui::Ui,
    rect: Rect,
    book: &mut Book,
    look: &Look<'_>,
    focus_name_box: bool,
    asked: &mut Asked,
) {
    let z = kit::zoom(ui.ctx());
    ui.painter().rect_filled(rect, 0.0, look.ground(look.palette.panel));
    crate::components::chrome_kit::groove(ui.painter(), rect.left_bottom(), rect.right_bottom());
    let middle = rect.center().y;
    let pad = 10.0 * z;
    let round = Vec2::splat(kit::ROUND * z);
    let at_middle = |x: f32, width: f32, height: f32| {
        Rect::from_min_size(Pos2::new(x, middle - height / 2.0), Vec2::new(width, height))
    };
    // The two round chips at the right first, so the wells know how much room is theirs.
    let format = at_middle(rect.right() - pad - 2.0 * round.x - 8.0 * z, round.x, round.y);
    let menu = at_middle(format.right() + 8.0 * z, round.x, round.y);
    let name_rect = at_middle(rect.left() + pad, NAME_BOX * z, WELL * z);
    name_box(ui, name_rect, book, focus_name_box, asked);
    let fx = at_middle(name_rect.right() + 8.0 * z, round.x, round.y);
    let insert = kit::round_chip(
        ui,
        fx,
        "Insert function",
        "Insert Function (Shift+F3)",
        Mark::Svg(&svg_icon::SQUARE_FUNCTION),
        book.view
            .panel
            .as_ref()
            .is_some_and(|p| p.0.last().map(String::as_str) == Some("Function")),
    );
    if insert.clicked() {
        asked.words.push(vec!["insert-function".to_owned()]);
    }
    let formula = Rect::from_min_max(
        Pos2::new(fx.right() + 8.0 * z, middle - WELL * z / 2.0),
        Pos2::new(format.left() - 12.0 * z, middle + WELL * z / 2.0),
    );
    formula_well(ui, formula, book, look, asked);
    let lit = book.view.panel.is_some();
    let response = kit::round_chip(
        ui,
        format,
        "Format",
        "Number, font, fill, borders, alignment, rules and data (Ctrl+1)",
        Mark::Rux(rux::Icon::Palette),
        lit,
    );
    if response.clicked() {
        book.view.panel = if lit { None } else { Some(PanelPath(Vec::new())) };
    }
    let open = book.view.book_menu.is_some();
    let response = kit::round_chip(
        ui,
        menu,
        "Workbook menu",
        "Every command, with a search (Alt+Q)",
        Mark::Svg(&svg_icon::MENU),
        open,
    );
    if response.clicked() {
        book.view.book_menu = if open { None } else { Some(String::new()) };
        book.view.menu = None;
    }
}

/// The name box: the address of what is chosen, or its defined name, typed into to go somewhere or to name
/// what is chosen. With defined names in the workbook, a chevron at its right lists them.
fn name_box(ui: &mut egui::Ui, rect: Rect, book: &mut Book, focus: bool, asked: &mut Asked) {
    let z = kit::zoom(ui.ctx());
    let names: Vec<String> = book
        .session
        .book
        .names
        .iter()
        .filter(|n| !n.name.starts_with("_xlnm") && !n.hidden)
        .map(|n| n.name.clone())
        .collect();
    let id = egui::Id::new(("spreadsheet-name-box", &book.key));
    let box_id = id.with("box");
    let mut typed: String = ui.ctx().data(|d| d.get_temp::<String>(id)).unwrap_or_default();
    let focused = ui.ctx().memory(|m| m.has_focus(box_id));
    if !focused {
        typed = super::name_of(book).unwrap_or_else(|| book.view.range().a1());
    }
    well(ui, "name-box", rect, focused);
    let chevron = (!names.is_empty()).then(|| {
        Rect::from_center_size(
            Pos2::new(rect.right() - 15.0 * z, rect.center().y),
            Vec2::new(24.0 * z, rect.height()),
        )
    });
    let font = egui::FontId::proportional(text_size(z));
    let right = chevron.map_or(rect.right() - 10.0 * z, |c| c.left());
    let field = Rect::from_min_max(rect.min, Pos2::new(right, rect.max.y));
    let inside = controls::field_takes_the_whole_rectangle_at(
        ui,
        field,
        13.0 * z,
        box_id,
        "Name box",
        &font,
    );
    let response = ui.text_box_at(
        inside,
        egui::TextEdit::singleline(&mut typed)
            .id(box_id)
            .frame(egui::Frame::NONE)
            .font(font.clone())
            .text_color(kit::ink().i900),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Name box"));
    if focus {
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
    ui.ctx().data_mut(|d| d.insert_temp(id, typed));
    let Some(chevron) = chevron else { return };
    let response =
        ui.interact(chevron, id.with("names"), Sense::click()).with_hint("Defined names");
    let popup = egui::Popup::default_response_id(&response);
    let open = egui::Popup::is_id_open(ui.ctx(), popup) != response.clicked();
    let tint = if response.hovered() || open { kit::chat().sky } else { kit::ink().i400 };
    kit::layer(ui, "name-box-chevron", chevron, |rux| {
        rux.mark(
            rux::Mark::new(rux::Icon::ChevDown, 10.0 * z).stroke(1.6 * 2.4),
            chevron.center(),
            tint,
        );
    });
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Defined names"));
    let chosen = egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(controls::menu_frame(ui))
        .width(rect.width().max(200.0 * z))
        .show(|ui| {
            controls::set_in_the_list_size(ui);
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut chosen = None;
            for name in &names {
                if controls::menu_row(ui, name, "", true, false, 0.0) {
                    chosen = Some(name.clone());
                }
            }
            chosen
        })
        .and_then(|inner| inner.inner);
    if let Some(name) = chosen {
        egui::Popup::close_id(ui.ctx(), popup);
        asked.words.push(vec!["select".to_owned(), name]);
        asked.requests.push(Request::TakeTheKeyboard(true));
    }
}

/// The formula well: what the active cell holds, which a click starts typing into, and the box typed into
/// while the formula bar has the caret. A formula longer than the well, or one with line breaks, grows the
/// well down over the grid while it is typed in, as Excel's formula bar does.
fn formula_well(
    ui: &mut egui::Ui,
    rect: Rect,
    book: &mut Book,
    look: &Look<'_>,
    asked: &mut Asked,
) {
    let z = kit::zoom(ui.ctx());
    let ink = kit::ink();
    let font = egui::FontId::proportional(text_size(z));
    let in_bar = book.view.edit.as_ref().is_some_and(|e| e.in_bar);
    well(ui, "formula", rect, in_bar);
    let inside = rect.shrink2(Vec2::new(14.0 * z, 0.0));
    let row = ui.ctx().fonts_mut(|fonts| fonts.row_height(&font));
    let text_rect = controls::field_text_rect_at(rect, 14.0 * z, row);
    let bar_id = editor::bar_box_id(&book.key);
    match book.view.edit.as_mut() {
        Some(edit) => {
            let width =
                ui.painter().layout_no_wrap(edit.text.clone(), font.clone(), ink.i900).size().x;
            let grows = edit.in_bar && (width > inside.width() || edit.text.contains('\n'));
            let (layout_font, layout_ink) = (font.clone(), ink.i900);
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
                let mut job =
                    editor::coloured_formula(text.as_str(), layout_font.clone(), layout_ink);
                job.wrap.max_width = width;
                ui.fonts_mut(|fonts| fonts.layout_job(job))
            };
            let response = if grows {
                let lines = (width / inside.width().max(1.0))
                    .ceil()
                    .max(edit.text.split('\n').count() as f32)
                    .clamp(2.0, 10.0);
                let tall = Rect::from_min_size(
                    rect.min,
                    Vec2::new(rect.width(), lines * (font.size * 1.35) + 14.0 * z),
                );
                egui::Area::new(egui::Id::new(("spreadsheet-tall-bar", &book.key)))
                    .order(egui::Order::Foreground)
                    .fixed_pos(tall.min)
                    .show(ui.ctx(), |ui| {
                        let chat = kit::chat();
                        ui.painter().rect(
                            tall,
                            egui::CornerRadius::same((15.0 * z).round().clamp(0.0, 255.0) as u8),
                            chat.well,
                            Stroke::new(1.2, chat.sky),
                            egui::StrokeKind::Inside,
                        );
                        ui.text_box_at(
                            tall.shrink2(Vec2::new(14.0 * z, 7.0 * z)),
                            egui::TextEdit::multiline(&mut edit.text)
                                .id(bar_id)
                                .frame(egui::Frame::NONE)
                                .font(font.clone())
                                .layouter(&mut layouter)
                                .lock_focus(true)
                                .desired_width(tall.width() - 28.0 * z)
                                .return_key(Some(egui::KeyboardShortcut::new(
                                    egui::Modifiers::ALT,
                                    egui::Key::Enter,
                                ))),
                        )
                    })
                    .inner
            } else {
                ui.text_box_at(
                    text_rect,
                    egui::TextEdit::singleline(&mut edit.text)
                        .id(bar_id)
                        .frame(egui::Frame::NONE)
                        .font(font.clone())
                        .layouter(&mut layouter)
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
                ui.interact(rect, ui.id().with(("formula-bar", &book.key)), Sense::click());
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }
            let text = controls::truncate_chars(&shown.replace('\n', " "), 400, 396);
            let size = text_size(z);
            let galley = kit::words(ui, &text, size, ink.i900, inside.width());
            kit::draw_by_capitals(
                ui.painter(),
                inside.left(),
                rect.center().y,
                galley,
                size,
                ink.i900,
            );
            let _ = look;
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
}

/// The sheet strip: a round chip that adds a sheet, then the sheets as pills in one carved track, the one
/// showing raised out of it. A sheet's tab colour is a dot before its name. A double click renames it in
/// place, a drag along the strip moves it, and a right click opens its menu.
pub(super) fn sheet_strip(
    ui: &mut egui::Ui,
    strip: Rect,
    book: &mut Book,
    look: &Look<'_>,
    asked: &mut Asked,
) {
    let z = kit::zoom(ui.ctx());
    let ink = kit::ink();
    ui.painter().rect_filled(strip, 0.0, look.ground(look.palette.panel));
    crate::components::chrome_kit::groove(ui.painter(), strip.left_top(), strip.right_top());
    let middle = strip.center().y;
    let chip = 26.0 * z;
    let add = Rect::from_center_size(
        Pos2::new(strip.left() + 10.0 * z + chip / 2.0, middle),
        Vec2::splat(chip),
    );
    if kit::round_chip(
        ui,
        add,
        "Add a sheet",
        "Add a sheet (Shift+F11)",
        Mark::Rux(rux::Icon::Plus),
        false,
    )
    .clicked()
    {
        asked.acts.push(Act::AddSheet);
    }
    let size = 12.0 * z;
    let font = egui::FontId::proportional(size);
    let pill_height = 24.0 * z;
    let theme = book.session.book.styles.theme.clone();
    let sheets: Vec<(usize, String, Option<egui::Color32>)> = book
        .session
        .book
        .sheets
        .iter()
        .enumerate()
        .filter(|(_, s)| s.state == unluminous_sheet::model::Visibility::Visible)
        .map(|(i, s)| {
            let colour = s
                .tab_color
                .and_then(|c| c.resolve(&theme))
                .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b));
            (i, s.name.clone(), colour)
        })
        .collect();
    // Each pill's width: its name, room for a colour dot, and the pill's own padding.
    let widths: Vec<f32> = sheets
        .iter()
        .map(|(_, name, colour)| {
            let words =
                ui.fonts_mut(|f| f.layout_no_wrap(name.clone(), font.clone(), ink.i900).size().x);
            words + 28.0 * z + if colour.is_some() { 12.0 * z } else { 0.0 }
        })
        .collect();
    let track_left = add.right() + 10.0 * z;
    let limit = strip.right() - 12.0 * z;
    let gap = 3.0 * z;
    let mut shown = 0;
    let mut used = 6.0 * z;
    for width in &widths {
        if track_left + used + width + 6.0 * z > limit {
            break;
        }
        used += width + gap;
        shown += 1;
    }
    let track = Rect::from_min_size(
        Pos2::new(track_left, middle - (pill_height + 6.0 * z) / 2.0),
        Vec2::new(used - gap + 6.0 * z, pill_height + 6.0 * z),
    );
    if shown > 0 {
        kit::layer(ui, "sheet-track", track, |rux| {
            kit::well_surface(rux, track, track.height() / 2.0);
        });
    }
    let mut x = track.left() + 3.0 * z;
    let mut placed: Vec<(usize, Rect)> = Vec::new();
    let mut dropped: Option<(usize, f32)> = None;
    for ((i, name, colour), width) in sheets.into_iter().zip(widths).take(shown) {
        let rect = Rect::from_min_size(
            Pos2::new(x, middle - pill_height / 2.0),
            Vec2::new(width, pill_height),
        );
        x += width + gap;
        let showing = i == book.view.sheet;
        if let Some((_, text)) = book.view.renaming.as_mut().filter(|(r, _)| *r == i) {
            let id = egui::Id::new(("spreadsheet-rename", &book.key, i));
            kit::layer(ui, ("sheet-rename", i), rect, |rux| {
                kit::chip_surface(rux, rect, rect.height() / 2.0, false)
            });
            let response = ui.text_box_at(
                rect.shrink2(Vec2::new(10.0 * z, 2.0 * z)),
                egui::TextEdit::singleline(text).id(id).frame(egui::Frame::NONE).font(font.clone()),
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
        let hovered = response.hovered();
        if hovered && !showing {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if showing {
            kit::layer(ui, ("sheet-pill", i), rect, |rux| {
                kit::chip_surface(rux, rect, rect.height() / 2.0, false)
            });
        }
        // A tab dragged along the strip moves its sheet to where it is let go.
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                ui.painter().line_segment(
                    [
                        Pos2::new(p.x, track.top() + 3.0 * z),
                        Pos2::new(p.x, track.bottom() - 3.0 * z),
                    ],
                    Stroke::new(2.0, kit::chat().sky),
                );
            }
        }
        if response.drag_stopped() {
            if let Some(p) = response.interact_pointer_pos() {
                dropped = Some((i, p.x));
            }
        }
        let tint = match (showing, hovered) {
            (true, _) => ink.i900,
            (false, true) => kit::chat().sky,
            (false, false) => ink.i500,
        };
        let mut left = rect.left() + 14.0 * z;
        if let Some(colour) = colour {
            ui.painter().circle_filled(Pos2::new(left + 3.0 * z, middle), 3.5 * z, colour);
            left += 12.0 * z;
        }
        let galley = ui.painter().crisp_layout_no_wrap(name.clone(), font.clone(), tint);
        kit::draw_by_capitals(ui.painter(), left, middle, galley, size, tint);
        let named = format!("Sheet {name}");
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, showing, &named)
        });
        if response.clicked() && !showing {
            let caret = book.view.edit.as_ref().map(|e| {
                let id = editor::box_with_the_caret(ui.ctx(), book);
                keys::caret_of(ui.ctx(), id).unwrap_or(e.text.chars().count())
            });
            if caret.is_some_and(|caret| super::can_point(book, caret)) {
                super::point_on_another_sheet(book, i, caret.unwrap_or(0));
            } else {
                super::commit_open_edit(book, asked);
                asked.acts.push(Act::ShowSheet(i));
            }
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
        let target = if before >= placed.len() {
            placed.last().map_or(from, |(index, _)| *index)
        } else {
            let target = placed[before].0;
            if target > from {
                target.saturating_sub(1)
            } else {
                target
            }
        };
        if target != from {
            asked.acts.push(Act::MoveSheet(from, target));
        }
    }
}

/// The mark each of the Format panel's first levels carries, by its name.
pub fn mark_of(level: &str) -> Mark {
    match level {
        "Number" => Mark::Svg(&svg_icon::HASH),
        "Font" => Mark::Svg(&svg_icon::TYPE),
        "Fill" => Mark::Svg(&svg_icon::PAINT_BUCKET),
        "Borders" => Mark::Svg(&svg_icon::GRID),
        "Alignment" => Mark::Svg(&svg_icon::ALIGN_LEFT),
        "Styles" => Mark::Svg(&svg_icon::HEADING),
        "Cell size" => Mark::Svg(&svg_icon::COLUMNS),
        "Rules" => Mark::Svg(&svg_icon::HIGHLIGHTER),
        "Data" => Mark::Rux(rux::Icon::Database),
        "Sort and Filter" => Mark::Svg(&svg_icon::FUNNEL),
        "Data tools" => Mark::Svg(&svg_icon::WRENCH),
        "Group and Outline" => Mark::Svg(&svg_icon::INDENT),
        "Manage rules" => Mark::Svg(&svg_icon::LIST),
        "Review" => Mark::Rux(rux::Icon::Chat),
        "Protection" | "Protect" => Mark::Svg(&svg_icon::LOCK),
        "Styles and Themes" | "Themes" => Mark::Rux(rux::Icon::Palette),
        "Sparklines" | "Sparkline" => Mark::Svg(&svg_icon::ACTIVITY),
        "Chart" | "Charts" => Mark::Svg(&svg_icon::CHART_COLUMN),
        "Page Layout" | "Page layout" | "Page" => Mark::Rux(rux::Icon::Document),
        "PivotTable" => Mark::Svg(&svg_icon::GRID),
        "View" => Mark::Svg(&svg_icon::EYE),
        "Sheet" => Mark::Svg(&svg_icon::SHEET),
        "Object" => Mark::Rux(rux::Icon::Layers),
        "Function" | "Insert" => Mark::Svg(&svg_icon::SQUARE_FUNCTION),
        _ => Mark::Svg(&svg_icon::LIST),
    }
}

/// What a press in the panel's head asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
    Back,
    Close,
}

/// The panel's head, as the Word panel has it: a back chip when deeper than the first level, the level's
/// name in the design's 15 point semibold with the path above it (or, at the first level, what the panel
/// acts on under it), and a close chip at the right.
pub fn panel_head(ui: &mut egui::Ui, head: Rect, path: &[String], what: &str) -> Option<Head> {
    let z = kit::zoom(ui.ctx());
    let ink = kit::ink();
    let chip = 28.0 * z;
    let middle = head.center().y;
    let mut asked = None;
    let mut left = head.left() + 14.0 * z;
    if !path.is_empty() {
        let back = Rect::from_min_size(Pos2::new(left, middle - chip / 2.0), Vec2::splat(chip));
        if kit::round_chip(ui, back, "Back", "Back", Mark::Rux(rux::Icon::ChevLeft), false)
            .clicked()
        {
            asked = Some(Head::Back);
        }
        left = back.right() + 10.0 * z;
    }
    let close = Rect::from_min_size(
        Pos2::new(head.right() - 14.0 * z - chip, middle - chip / 2.0),
        Vec2::splat(chip),
    );
    let cross = Mark::Rux(rux::Icon::Cross);
    if kit::round_chip(
        ui,
        close,
        "Close the Format panel",
        "Close the Format panel (Esc)",
        cross,
        false,
    )
    .clicked()
    {
        asked = Some(Head::Close);
    }
    let room = close.left() - 10.0 * z - left;
    let title = path.last().map_or("Format", String::as_str);
    let title_size = 15.0 * z;
    let small = 10.5 * z;
    let galley = kit::words(ui, title, title_size, ink.i900, room);
    let (above, below) = match path.len() {
        0 => (None, Some(what.to_owned())),
        1 => (Some("Format".to_owned()), None),
        _ => (Some(format!("Format  /  {}", path[..path.len() - 1].join("  /  "))), None),
    };
    let title_y = match (&above, &below) {
        (Some(_), _) => middle + 7.0 * z,
        (_, Some(_)) => middle - 7.0 * z,
        _ => middle,
    };
    if let Some(above) = above {
        let crumbs = kit::words(ui, &above, small, ink.i400, room);
        kit::draw_by_capitals(ui.painter(), left, middle - 9.0 * z, crumbs, small, ink.i400);
    }
    kit::draw_by_capitals(ui.painter(), left, title_y, galley.clone(), title_size, ink.i900);
    // The interface face may have no semibold, so the title is drawn a second time half a point over.
    kit::draw_by_capitals(ui.painter(), left + 0.5, title_y, galley, title_size, ink.i900);
    if let Some(below) = below {
        let line = kit::words(ui, &below, small, ink.i400, room);
        kit::draw_by_capitals(ui.painter(), left, middle + 10.0 * z, line, small, ink.i400);
    }
    let named = title.to_owned();
    let response = ui.interact(
        Rect::from_min_max(Pos2::new(left, head.top()), Pos2::new(close.left(), head.bottom())),
        ui.id().with("sheet-panel-title"),
        Sense::hover(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &named));
    asked
}

/// The carved search well at the top of the panel's first level, which finds any command by name.
pub fn search_well(ui: &mut egui::Ui, rect: Rect, id: egui::Id, hint: &str, text: &mut String) {
    let z = kit::zoom(ui.ctx());
    let ink = kit::ink();
    well(ui, "panel-search", rect, ui.ctx().memory(|m| m.has_focus(id)));
    let mark = Pos2::new(rect.left() + 16.0 * z, rect.center().y);
    kit::layer(
        ui,
        "panel-search-mark",
        Rect::from_center_size(mark, Vec2::splat(16.0 * z)),
        |rux| {
            rux.mark(rux::Mark::new(rux::Icon::Search, 13.0 * z), mark, ink.i400);
        },
    );
    let font = egui::FontId::proportional(text_size(z));
    let inside = controls::field_takes_the_whole_rectangle_at(ui, rect, 30.0 * z, id, hint, &font);
    let response = ui.text_box_at(
        inside,
        egui::TextEdit::singleline(text)
            .id(id)
            .frame(egui::Frame::NONE)
            .font(font.clone())
            .text_color(ink.i900)
            .hint_text(controls::placeholder(hint, &font, ink.i400)),
    );
    let named = hint.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, &named));
}

/// One of the panel's first levels: a raised row with the level's mark in a carved disc, its name, what it
/// says about the chosen cells now at the right, and a chevron. The whole row is a button.
pub fn level_row(ui: &mut egui::Ui, name: &str, detail: Option<&str>, mark: Mark) -> bool {
    let z = kit::zoom(ui.ctx());
    let height = 42.0 * z;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let rect = rect.shrink2(Vec2::new(2.0 * z, 2.5 * z));
    let hovered = response.hovered();
    let held = response.is_pointer_button_down_on();
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let ink = kit::ink();
    let disc = Rect::from_center_size(
        Pos2::new(rect.left() + 8.0 * z + 13.0 * z, rect.center().y),
        Vec2::splat(26.0 * z),
    );
    kit::layer(ui, ("sheet-level-row", name), rect, |rux| {
        match held {
            true => kit::well_surface(rux, rect, 13.0 * z),
            false => kit::card_surface(rux, rect, 13.0 * z),
        }
        kit::well_surface(rux, disc, disc.width() / 2.0);
        let chat = rux.theme().chat;
        let tint = if hovered { chat.sky } else { ink.i700 };
        kit::paint_mark(rux, mark, disc.center(), 14.0 * z, tint);
        let chevron = Pos2::new(rect.right() - 14.0 * z, rect.center().y);
        rux.mark(
            rux::Mark::new(rux::Icon::ChevRight, 10.0 * z),
            chevron,
            if hovered { chat.sky } else { ink.i400 },
        );
    });
    let left = disc.right() + 11.0 * z;
    let right = rect.right() - 28.0 * z;
    let name_size = 12.5 * z;
    let small = 11.0 * z;
    let detail_width = detail.map_or(0.0, |d| {
        let w = ui.fonts_mut(|f| {
            f.layout_no_wrap(d.to_owned(), egui::FontId::proportional(small), ink.i400).size().x
        });
        w.min((right - left) * 0.55)
    });
    let title = kit::words(ui, name, name_size, ink.i900, right - left - detail_width - 8.0 * z);
    kit::draw_by_capitals(ui.painter(), left, rect.center().y, title, name_size, ink.i900);
    if let Some(detail) = detail {
        let line = kit::words(ui, detail, small, ink.i400, detail_width);
        let x = right - line.size().x;
        kit::draw_by_capitals(ui.painter(), x, rect.center().y, line, small, ink.i400);
    }
    let named = name.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &named));
    let hint = match detail {
        Some(detail) => format!("{name}: {detail}"),
        None => name.to_owned(),
    };
    response.with_hint(hint).clicked()
}

/// The mark a button whose word is a single symbol is drawn with: no mark is a character, because a face
/// without that character draws an empty box.
fn mark_for_symbol(label: &str) -> Option<Mark> {
    match label {
        "\u{2715}" => Some(Mark::Rux(rux::Icon::Cross)),
        "\u{25B2}" => Some(Mark::Svg(&svg_icon::CHEVRON_UP)),
        "\u{25BC}" => Some(Mark::Rux(rux::Icon::ChevDown)),
        "\u{25C0}" => Some(Mark::Rux(rux::Icon::ChevLeft)),
        "\u{25B6}" => Some(Mark::Rux(rux::Icon::ChevRight)),
        _ => None,
    }
}

/// A button in one of the cards that float over the grid: a pill with its word, or for a symbol a small round
/// chip with the symbol drawn as a mark. `name` is its accessible name. Answers whether it was pressed.
pub fn button(ui: &mut egui::Ui, label: &str, name: &str) -> bool {
    let z = kit::zoom(ui.ctx());
    match mark_for_symbol(label) {
        Some(mark) => {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0 * z), Sense::hover());
            kit::round_chip(ui, rect, name, name, mark, false).clicked()
        }
        None => kit::named_pill(ui, label, name).clicked(),
    }
}

/// A small round chip floating at the corner of cells, such as Paste Options after a paste: a disc raised
/// over the grid with a mark. Answers its response.
pub fn floating_chip(
    ui: &egui::Ui,
    rect: Rect,
    id: egui::Id,
    name: &str,
    mark: Mark,
) -> egui::Response {
    let z = kit::zoom(ui.ctx());
    let response = ui.interact(rect, id, Sense::click()).with_hint(name);
    let hovered = response.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    // A `Ui` of its own over the chip in the same layer, because the callers hold the grid's `Ui` only to read.
    let mut child = egui::Ui::new(
        ui.ctx().clone(),
        id.with("layer"),
        egui::UiBuilder::new().layer_id(ui.layer_id()).max_rect(rect),
    );
    kit::layer(&mut child, ("floating-chip", name), rect, |rux| {
        rux::components::chip_disc(rux, rect);
        let tint = if hovered { rux.theme().chat.sky } else { kit::ink().i700 };
        kit::paint_mark(rux, mark, rect.center(), 12.0 * z, tint);
    });
    let named = name.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &named));
    response
}

/// A card floating over the grid, in the window's menu card, with its egui widgets set in the design's
/// style.
pub fn card(ui: &egui::Ui, id: egui::Id, at: Pos2, add: impl FnOnce(&mut egui::Ui)) -> Rect {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            controls::menu_frame(ui).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                kit::style_the_panel(ui);
                add(ui)
            });
        })
        .response
        .rect
}
