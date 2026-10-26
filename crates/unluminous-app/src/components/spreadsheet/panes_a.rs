//! The popups and panes of the data, review and what if features (`task-2360`): the Sort levels popup, the
//! Evaluate Formula stepper, the Scenario Manager, the Watch Window, the Comments pane, notes drawn at their
//! stored box, and the button offered after a calculated column is filled. Each one asks for a change by
//! pushing the words of a verb, as a menu entry does, so a press and an agent's call are the same call.

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use super::grid::Frame;
use crate::services::plugin_ui::Look;
use crate::services::spreadsheet::Book;

type Words = Vec<Vec<String>>;

fn say(text: &str) -> Vec<String> {
    text.split(' ').map(str::to_owned).collect()
}

/// Draw every popup and pane that is open.
pub fn show(ui: &egui::Ui, book: &mut Book, frame: Frame, look: &Look<'_>, words: &mut Words) {
    notes_at_their_box(ui, book, frame, words);
    if book.view.audit.sort.open {
        sort_popup(ui, book, frame, look, words);
    }
    if book.view.audit.evaluator.is_some() {
        evaluate_popup(ui, book, frame, look, words);
    }
    if book.view.audit.watch_open {
        side_pane(ui, book, frame, look, "Watch Window", 0, words, watch_body);
    }
    if book.view.audit.comments_open {
        side_pane(ui, book, frame, look, "Comments", 1, words, comments_body);
    }
    if book.view.audit.scenarios_open {
        side_pane(ui, book, frame, look, "Scenario Manager", 2, words, scenario_body);
    }
    calculated_column_button(ui, book, frame, look, words);
}

/// A floating card with the palette's menu colours.
fn card(
    ui: &egui::Ui,
    look: &Look<'_>,
    id: egui::Id,
    at: Pos2,
    add: impl FnOnce(&mut egui::Ui),
) -> Rect {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .fill(look.palette.menu)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::same(10))
                .show(ui, add);
        })
        .response
        .rect
}

fn button(ui: &mut egui::Ui, label: &str, name: &str) -> bool {
    let r = ui.button(label);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    r.clicked()
}

// ------------------------------------------------------------------------------------------- sort levels

const SORT_ON: [(&str, &str); 4] = [
    ("Cell values", "values"),
    ("Cell color", "cell-color"),
    ("Font color", "font-color"),
    ("Cell icon", "icon"),
];

/// The Sort levels popup under the selection: a row for each level with its column, what it sorts on and its
/// order, and buttons to add, delete, copy and move levels.
fn sort_popup(ui: &egui::Ui, book: &Book, frame: Frame, look: &Look<'_>, words: &mut Words) {
    let anchor = frame.range(book, book.view.range()).left_bottom() + Vec2::new(0.0, 4.0);
    let draft = book.view.audit.sort.clone();
    let first = book.view.range().first.col;
    let last = book.view.range().last.col.max(first);
    let id = egui::Id::new(("spreadsheet-sort-popup", &book.key));
    card(ui, look, id, anchor, |ui| {
        ui.label(egui::RichText::new("Sort").strong().color(look.palette.text_strong));
        for (i, level) in draft.levels.iter().enumerate() {
            let n = i + 1;
            ui.horizontal(|ui| {
                ui.label(if i == 0 { "Sort by" } else { "Then by" });
                let column = unluminous_sheet::coord::column_name(level.key);
                egui::ComboBox::from_id_salt((id, "column", i)).selected_text(&column).show_ui(
                    ui,
                    |ui| {
                        for c in first..=last {
                            let name = unluminous_sheet::coord::column_name(c);
                            if ui.selectable_label(c == level.key, &name).clicked() {
                                words.push(say(&format!("sort-draft set {n} column {name}")));
                            }
                        }
                    },
                );
                let on =
                    SORT_ON.iter().find(|(_, w)| *w == level.on).map_or("Cell values", |(l, _)| *l);
                egui::ComboBox::from_id_salt((id, "on", i)).selected_text(on).show_ui(ui, |ui| {
                    for (label, word) in SORT_ON {
                        if ui.selectable_label(level.on == word, label).clicked() {
                            words.push(say(&format!("sort-draft set {n} on {word}")));
                        }
                    }
                });
                let order = if level.descending { "Z to A" } else { "A to Z" };
                egui::ComboBox::from_id_salt((id, "order", i)).selected_text(order).show_ui(
                    ui,
                    |ui| {
                        for (label, word) in [("A to Z", "asc"), ("Z to A", "desc")] {
                            if ui.selectable_label(false, label).clicked() {
                                words.push(say(&format!("sort-draft set {n} order {word}")));
                            }
                        }
                    },
                );
                for (label, name, verb) in [
                    ("\u{25B2}", "Move level up", "up"),
                    ("\u{25BC}", "Move level down", "down"),
                    ("Copy", "Copy level", "copy"),
                    ("\u{2715}", "Delete level", "delete"),
                ] {
                    if button(ui, label, &format!("{name} {n}")) {
                        words.push(say(&format!("sort-draft {verb} {n}")));
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            if button(ui, "Add level", "Add level") {
                words.push(say("sort-draft add"));
            }
            let mut header = draft.header == "header";
            if ui.checkbox(&mut header, "My data has headers").changed() {
                words.push(say(if header {
                    "sort-draft options header"
                } else {
                    "sort-draft options noheader"
                }));
            }
            let mut case = draft.case_sensitive;
            if ui.checkbox(&mut case, "Case sensitive").changed() {
                words.push(say(if case {
                    "sort-draft options case"
                } else {
                    "sort-draft options no-case"
                }));
            }
        });
        ui.horizontal(|ui| {
            if button(ui, "Sort", "Sort by the levels") {
                words.push(say("sort-draft apply"));
            }
            if button(ui, "Cancel", "Close the sort levels") {
                words.push(say("sort-draft close"));
            }
        });
    });
}

// ----------------------------------------------------------------------------------------------- evaluate

/// The Evaluate Formula popup: the formula with the part about to be evaluated underlined, and its buttons.
fn evaluate_popup(ui: &egui::Ui, book: &Book, frame: Frame, look: &Look<'_>, words: &mut Words) {
    let Some(ev) = &book.view.audit.evaluator else { return };
    let view = ev.view(&book.session.book);
    let anchor = frame.cell(book, book.view.cursor).left_bottom() + Vec2::new(0.0, 4.0);
    let id = egui::Id::new(("spreadsheet-evaluate", &book.key));
    card(ui, look, id, anchor, |ui| {
        ui.label(egui::RichText::new(format!("Evaluate {}", view.reference)).strong());
        let mut job = egui::text::LayoutJob::default();
        let font = egui::FontId::monospace(13.0);
        let chars: Vec<char> = view.text.chars().collect();
        let (a, b) = view.underlined.unwrap_or((0, 0));
        for (i, part) in [(0, a), (a, b), (b, chars.len())].iter().enumerate() {
            let text: String =
                chars[part.0.min(chars.len())..part.1.min(chars.len())].iter().collect();
            let mut format = egui::TextFormat::simple(font.clone(), look.palette.text);
            if i == 1 {
                format.underline = Stroke::new(1.5, look.palette.text_strong);
            }
            job.append(&text, 0.0, format);
        }
        ui.label(job);
        if let Some(result) = &view.result {
            ui.label(format!("Result: {}", result.to_text()));
        }
        ui.horizontal(|ui| {
            for (label, enabled, verb) in [
                ("Evaluate", view.can_evaluate, "evaluate evaluate"),
                ("Step In", view.can_step_in, "evaluate in"),
                ("Step Out", view.can_step_out, "evaluate out"),
                ("Restart", true, "evaluate restart"),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    words.push(say(verb));
                }
            }
            if button(ui, "Close", "Close Evaluate Formula") {
                words.push(say("evaluate close"));
            }
        });
    });
}

// ------------------------------------------------------------------------------------------- side panes

/// A pane at the right of the grid, one of three stacked from the top.
#[allow(clippy::too_many_arguments)]
fn side_pane(
    ui: &egui::Ui,
    book: &Book,
    frame: Frame,
    look: &Look<'_>,
    title: &str,
    slot: usize,
    words: &mut Words,
    body: fn(&mut egui::Ui, &Book, &mut Words),
) {
    let at = Pos2::new(
        frame.cells.right() - 330.0 - slot as f32 * 14.0,
        frame.cells.top() + 8.0 + slot as f32 * 14.0,
    );
    let id = egui::Id::new(("spreadsheet-pane", &book.key, title));
    card(ui, look, id, at, |ui| {
        ui.set_width(310.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).strong().color(look.palette.text_strong));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if button(ui, "\u{2715}", &format!("Close {title}")) {
                    words.push(match slot {
                        0 => say("watch hide"),
                        1 => say("comment pane off"),
                        _ => say("scenario pane off"),
                    });
                }
            });
        });
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| body(ui, book, words));
    });
}

fn watch_body(ui: &mut egui::Ui, book: &Book, words: &mut Words) {
    if button(ui, "Add watch for the active cell", "Add watch") {
        words.push(say(&format!("watch add {}", book.view.cursor.a1())));
    }
    for w in book.session.watch_rows() {
        ui.horizontal(|ui| {
            ui.label(format!("{}  {}  {}", w.sheet_name, w.at.a1(), w.value.to_text()));
            ui.label(egui::RichText::new(&w.formula).weak());
            if w.sheet == book.view.sheet
                && button(ui, "\u{2715}", &format!("Stop watching {}", w.at.a1()))
            {
                words.push(say(&format!("watch remove {}", w.at.a1())));
            }
        });
    }
}

fn comments_body(ui: &mut egui::Ui, book: &Book, words: &mut Words) {
    let index = book.view.sheet;
    for row in book.session.thread_rows(Some(index)) {
        let cell = row.at.a1();
        ui.group(|ui| {
            if ui
                .link(format!(
                    "{cell}  {}{}",
                    row.author,
                    if row.resolved { "  (resolved)" } else { "" }
                ))
                .clicked()
            {
                words.push(vec!["select".to_owned(), cell.clone()]);
            }
            ui.label(&row.text);
            if row.replies > 0 {
                ui.label(egui::RichText::new(format!("{} replies", row.replies)).weak());
            }
            let key = egui::Id::new(("reply", &book.key, &cell));
            let mut reply: String = ui.data_mut(|d| d.get_temp(key)).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut reply)
                        .hint_text(crate::components::controls::placeholder(
                            "Reply",
                            &egui::TextStyle::Body.resolve(ui.style()),
                            ui.visuals().weak_text_color(),
                        ))
                        .desired_width(150.0),
                );
                if button(ui, "Send", &format!("Send reply on {cell}")) && !reply.trim().is_empty()
                {
                    words.push(vec![
                        "comment".to_owned(),
                        "reply".to_owned(),
                        cell.clone(),
                        std::mem::take(&mut reply),
                    ]);
                }
                let (label, verb) =
                    if row.resolved { ("Reopen", "reopen") } else { ("Resolve", "resolve") };
                if button(ui, label, &format!("{label} {cell}")) {
                    words.push(say(&format!("comment {verb} {cell}")));
                }
                if button(ui, "Delete", &format!("Delete thread {cell}")) {
                    words.push(say(&format!("comment delete {cell}")));
                }
            });
            ui.data_mut(|d| d.insert_temp(key, reply));
        });
    }
    let key = egui::Id::new(("new-comment", &book.key));
    let mut text: String = ui.data_mut(|d| d.get_temp(key)).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut text)
                .hint_text(crate::components::controls::placeholder(
                    "New comment on the active cell",
                    &egui::TextStyle::Body.resolve(ui.style()),
                    ui.visuals().weak_text_color(),
                ))
                .desired_width(190.0),
        );
        if button(ui, "New", "New comment") && !text.trim().is_empty() {
            words.push(vec![
                "comment".to_owned(),
                "new".to_owned(),
                book.view.cursor.a1(),
                std::mem::take(&mut text),
            ]);
        }
    });
    ui.data_mut(|d| d.insert_temp(key, text));
}

fn scenario_body(ui: &mut egui::Ui, book: &Book, words: &mut Words) {
    let index = book.view.sheet;
    for s in book.session.scenarios(index).list {
        ui.horizontal(|ui| {
            ui.label(&s.name);
            if button(ui, "Show", &format!("Show scenario {}", s.name)) {
                words.push(vec!["scenario".to_owned(), "show".to_owned(), s.name.clone()]);
            }
            if button(ui, "Delete", &format!("Delete scenario {}", s.name)) {
                words.push(vec!["scenario".to_owned(), "delete".to_owned(), s.name.clone()]);
            }
        });
    }
    ui.separator();
    let key = egui::Id::new(("new-scenario", &book.key));
    let (mut name, mut values): (String, String) =
        ui.data_mut(|d| d.get_temp(key)).unwrap_or_default();
    ui.add(
        egui::TextEdit::singleline(&mut name)
            .hint_text(crate::components::controls::placeholder(
                "Scenario name",
                &egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            ))
            .desired_width(280.0),
    );
    ui.add(
        egui::TextEdit::singleline(&mut values)
            .hint_text(crate::components::controls::placeholder(
                "Values for the selected cells, separated by commas",
                &egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            ))
            .desired_width(280.0),
    );
    ui.horizontal(|ui| {
        if button(ui, "Add for the selection", "Add scenario") && !name.trim().is_empty() {
            let mut w =
                vec!["scenario".to_owned(), "add".to_owned(), name.clone(), book.view.range().a1()];
            if !values.trim().is_empty() {
                w.push("values".to_owned());
                w.push(values.clone());
            }
            words.push(w);
        }
        if button(ui, "Summary report", "Scenario summary report") {
            words.push(vec!["scenario".to_owned(), "summary".to_owned(), book.view.range().a1()]);
        }
    });
    ui.data_mut(|d| d.insert_temp(key, (name, values)));
}

// ------------------------------------------------------------------------------------------------ notes

/// Notes that are showing, drawn at their stored box. The box moves when its body is dragged and is resized
/// from its bottom right corner; the new place is written when the pointer lets go.
fn notes_at_their_box(ui: &egui::Ui, book: &Book, frame: Frame, words: &mut Words) {
    let index = book.view.sheet;
    let s = &book.session.book.sheets[index];
    for ((row, col), note) in s.notes.iter().filter(|(_, n)| n.visible) {
        let at = unluminous_sheet::coord::CellRef::new(*row, *col);
        let Some(b) = book.session.note_box(index, at) else { continue };
        let id = egui::Id::new(("spreadsheet-note-box", &book.key, row, col));
        let (dm, dr): (Vec2, Vec2) = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        let z = frame.zoom;
        let origin = frame.cells.min - Vec2::new(frame.scroll.x, frame.scroll.y) * z;
        let rect = Rect::from_min_size(
            origin + Vec2::new(b.left as f32, b.top as f32) * z + dm,
            Vec2::new(b.width as f32, b.height as f32) * z + dr,
        )
        .intersect(Rect::EVERYTHING);
        if !frame.cells.intersects(rect) {
            continue;
        }
        let painter = ui.painter_at(frame.cells);
        let cell = frame.cell(book, at);
        painter.line_segment(
            [cell.right_top(), rect.left_top()],
            Stroke::new(1.0, crate::services::spreadsheet::excel_ink::NOTE_LEADER),
        );
        painter.rect_filled(rect, 0, crate::services::spreadsheet::excel_ink::NOTE_PAPER);
        painter.rect_stroke(
            rect,
            0,
            Stroke::new(1.0, crate::services::spreadsheet::excel_ink::NOTE_EDGE),
            egui::StrokeKind::Inside,
        );
        let text = if note.author.is_empty() {
            note.text.clone()
        } else {
            format!("{}:\n{}", note.author, note.text)
        };
        painter.with_clip_rect(rect.shrink(2.0)).text(
            rect.left_top() + Vec2::splat(4.0),
            Align2::LEFT_TOP,
            text,
            egui::FontId::proportional((11.0 * z).clamp(8.0, 20.0)),
            crate::services::spreadsheet::excel_ink::NOTE_INK,
        );
        let grip = Rect::from_min_size(rect.right_bottom() - Vec2::splat(10.0), Vec2::splat(10.0));
        painter.rect_filled(grip, 0, crate::services::spreadsheet::excel_ink::NOTE_GRIP);
        let body = ui.interact(rect, id.with("body"), Sense::drag());
        let corner = ui.interact(grip, id.with("grip"), Sense::drag());
        body.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Note box"));
        corner.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Resize note box")
        });
        let (mut dm, mut dr) = (dm, dr);
        if corner.dragged() {
            dr += corner.drag_delta();
        } else if body.dragged() {
            dm += body.drag_delta();
        }
        if corner.drag_stopped() || body.drag_stopped() {
            let f = |v: f32| format!("{:.1}", v / z);
            let left = b.left as f32 + dm.x / z;
            let top = b.top as f32 + dm.y / z;
            let width = (b.width as f32 + dr.x / z).max(20.0);
            let height = (b.height as f32 + dr.y / z).max(14.0);
            let w =
                ["note", &at.a1(), "box", &f(left * z), &f(top * z), &f(width * z), &f(height * z)];
            words.push(w.iter().map(|x| (*x).to_owned()).collect());
            (dm, dr) = (Vec2::ZERO, Vec2::ZERO);
        }
        ui.data_mut(|d| d.insert_temp(id, (dm, dr)));
    }
}

// ------------------------------------------------------------------------------ calculated column offer

/// The columns of tables whose data cells all hold formulas.
fn formula_columns(book: &Book) -> Vec<(String, usize)> {
    let index = book.view.sheet;
    let s = &book.session.book.sheets[index];
    let mut out = Vec::new();
    for t in book.session.book.tables.iter().filter(|t| t.sheet == index) {
        let Some(data) = t.data_range() else { continue };
        if data.rows() < 2 {
            continue;
        }
        for (i, col) in (data.first.col..=data.last.col).enumerate() {
            let all = (data.first.row..=data.last.row).all(|r| {
                s.cell(unluminous_sheet::coord::CellRef::new(r, col))
                    .is_some_and(|c| c.formula.is_some())
            });
            if all {
                out.push((t.name.clone(), i));
            }
        }
    }
    out
}

/// After a formula fills a table column, a small button beside its first cell offers to undo the fill or to
/// stop creating calculated columns, as Excel's AutoCorrect Options button does.
fn calculated_column_button(
    ui: &egui::Ui,
    book: &Book,
    frame: Frame,
    look: &Look<'_>,
    words: &mut Words,
) {
    let id = egui::Id::new(("spreadsheet-calculated", &book.key));
    let now = formula_columns(book);
    type Remembered = (Vec<(String, usize)>, u64, Option<(String, usize)>);
    let (before, seen, offer): Remembered = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let mut offer = offer;
    if seen != book.revision {
        offer =
            now.iter().find(|c| !before.contains(c)).cloned().or(offer.filter(|o| now.contains(o)));
        if now.iter().all(|c| before.contains(c)) && before.len() == now.len() {
            // Nothing new: keep any offer still showing.
        }
    }
    ui.data_mut(|d| d.insert_temp(id, (now, book.revision, offer.clone())));
    let Some((name, column)) = offer else { return };
    let Some(t) = book.session.book.tables.iter().find(|t| t.name == name) else { return };
    let Some(data) = t.data_range() else { return };
    let cell = frame.cell(
        book,
        unluminous_sheet::coord::CellRef::new(data.first.row, data.first.col + column as u32),
    );
    let spot = Rect::from_min_size(cell.right_top() + Vec2::new(2.0, 0.0), Vec2::splat(16.0));
    if !frame.cells.intersects(spot) {
        return;
    }
    let menu_id = id.with("menu");
    let r = ui.interact(spot, id.with("button"), Sense::click());
    r.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "AutoCorrect Options")
    });
    ui.painter().rect_filled(spot, 3, look.palette.menu);
    ui.painter().text(
        spot.center(),
        Align2::CENTER_CENTER,
        "\u{26A1}",
        egui::FontId::proportional(11.0),
        look.palette.text,
    );
    egui::Popup::from_toggle_button_response(&r).id(menu_id).show(|ui| {
        if button(ui, "Undo Calculated Column", "Undo Calculated Column") {
            words.push(say("undo"));
        }
        if button(
            ui,
            "Stop Automatically Creating Calculated Columns",
            "Stop creating calculated columns",
        ) {
            words.push(say("table auto-fill off"));
            words.push(say("undo"));
        }
    });
}
