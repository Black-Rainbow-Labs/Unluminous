//! The PivotTable Fields pane, the slicers and the timelines (`task-2360`, the `wiringb` lane). The pane lists
//! the fields with a search box and the four areas; a field is moved by dragging it to an area or from its
//! menu. Each press pushes the words of a `pivot`, `slicer` or `timeline` verb, as a menu entry does.

use egui::{Pos2, Rect, Vec2};
use unluminous_sheet::coord::CellRef;
use unluminous_sheet::pivot::cache::CacheValue;
use unluminous_sheet::pivot::table::AxisEntry;

use super::grid::Frame;
use super::panes_b::{button, card};
use crate::services::plugin_ui::Look;
use crate::services::spreadsheet::catalogue::Item;
use crate::services::spreadsheet::verbs_b_pivot::{area_of_field, func_word};
use crate::services::spreadsheet::view::Prompt;
use crate::services::spreadsheet::{catalogue_b, verbs_b_pivot_more, Book};

type Words = Vec<Vec<String>>;

/// What a press in the pane asked for, applied once the pane is drawn.
enum Ask {
    Words(Vec<String>),
    Form(Prompt),
}

fn say(text: &str) -> Vec<String> {
    text.split(' ').filter(|w| !w.is_empty()).map(str::to_owned).collect()
}

/// Draw the pane, the slicers and the timelines that are showing.
pub fn show(ui: &egui::Ui, book: &mut Book, frame: Frame, look: &Look<'_>, words: &mut Words) {
    slicers(ui, book, frame, look, words);
    timelines(ui, book, frame, look, words);
    if book.view.lane_b.pivot_pane {
        fields_pane(ui, book, frame, look, words);
    }
}

/// The pivot table the pane is about: the one under the active cell, else one on this sheet, else the first.
pub fn current_table(book: &Book) -> Option<usize> {
    let pivots = &book.session.book.pivots;
    pivots
        .at(book.view.sheet, book.view.cursor)
        .or_else(|| pivots.on_sheet(book.view.sheet).next().map(|(i, _)| i))
        .or((!pivots.tables.is_empty()).then_some(0))
}

// ----------------------------------------------------------------------------------- the Fields pane

fn fields_pane(ui: &egui::Ui, book: &mut Book, frame: Frame, look: &Look<'_>, words: &mut Words) {
    let at = Pos2::new(frame.cells.right() - 290.0, frame.cells.top() + 8.0);
    let id = egui::Id::new(("spreadsheet-pivot-fields", &book.key));
    let mut asks: Vec<Ask> = Vec::new();
    let mut search = book.view.lane_b.pivot_search.clone();
    let table = current_table(book);
    card(ui, look, id, at, |ui| {
        ui.set_width(270.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("PivotTable Fields").strong().color(look.palette.text_strong),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if button(ui, "\u{2715}", "Close PivotTable Fields") {
                    asks.push(Ask::Words(say("pivot pane off")));
                }
            });
        });
        match table {
            None => {
                ui.label("There is no PivotTable. Choose cells with data, then Insert PivotTable.");
                if button(ui, "Insert PivotTable", "Insert PivotTable") {
                    asks.push(Ask::Words(say("pivot create")));
                }
            }
            Some(t) => table_body(ui, book, t, look, &mut search, &mut asks),
        }
    });
    book.view.lane_b.pivot_search = search;
    for ask in asks {
        match ask {
            Ask::Words(w) => words.push(w),
            Ask::Form(prompt) => book.view.prompt = Some(prompt),
        }
    }
}

fn table_body(
    ui: &mut egui::Ui,
    book: &Book,
    t: usize,
    look: &Look<'_>,
    search: &mut String,
    asks: &mut Vec<Ask>,
) {
    let table = &book.session.book.pivots.tables[t];
    let cache = &book.session.book.pivots.caches[table.cache];
    let at = format!("in {}", table.name.replace(' ', "_"));
    ui.label(egui::RichText::new(&table.name).color(look.palette.text_dim));
    ui.add(egui::TextEdit::singleline(search).hint_text("Search").desired_width(f32::INFINITY));
    ui.label("Choose fields to add to report:");
    egui::ScrollArea::vertical().id_salt("pivot-field-list").max_height(150.0).show(ui, |ui| {
        for f in 0..cache.fields.len() {
            let name = table.caption(cache, f);
            if !search.is_empty() && !name.to_lowercase().contains(&search.to_lowercase()) {
                continue;
            }
            field_row(ui, book, t, f, &name, &at, asks);
        }
    });
    ui.separator();
    ui.label("Drag fields between the areas below:");
    ui.columns(2, |cols| {
        area_box(&mut cols[0], book, t, "Filters", "filters", &at, asks);
        area_box(&mut cols[1], book, t, "Columns", "columns", &at, asks);
    });
    ui.columns(2, |cols| {
        area_box(&mut cols[0], book, t, "Rows", "rows", &at, asks);
        area_box(&mut cols[1], book, t, "Values", "values", &at, asks);
    });
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        if button(ui, "Refresh", "Refresh PivotTable") {
            asks.push(Ask::Words(say(&format!("pivot refresh {at}"))));
        }
        for (label, how) in [("Compact", "compact"), ("Outline", "outline"), ("Tabular", "tabular")]
        {
            if button(ui, label, &format!("Report layout {label}")) {
                asks.push(Ask::Words(say(&format!("pivot layout {how} {at}"))));
            }
        }
    });
}

/// One field in the list: a tick that adds or removes it, its name as a handle to drag, and a menu.
fn field_row(
    ui: &mut egui::Ui,
    book: &Book,
    t: usize,
    f: usize,
    name: &str,
    at: &str,
    asks: &mut Vec<Ask>,
) {
    let table = &book.session.book.pivots.tables[t];
    let cache = &book.session.book.pivots.caches[table.cache];
    let (area, _) = area_of_field(table, f);
    let mut used = area != "none";
    ui.horizontal(|ui| {
        let tick = ui.checkbox(&mut used, "");
        tick.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Checkbox, true, format!("Field {name}"))
        });
        if tick.changed() {
            let numeric = cache.number_bounds(f).is_some() && !cache.is_date_field(f);
            let words = if used {
                if numeric {
                    format!("pivot add #{} values sum {at}", f + 1)
                } else {
                    format!("pivot add #{} rows {at}", f + 1)
                }
            } else {
                format!("pivot remove #{} {at}", f + 1)
            };
            asks.push(Ask::Words(say(&words)));
        }
        ui.dnd_drag_source(egui::Id::new(("pivot-field", &table.name, f)), f, |ui| {
            ui.label(name);
        });
        ui.menu_button("\u{25BE}", |ui| {
            for (label, to) in [
                ("Add to Filters", "filters"),
                ("Add to Columns", "columns"),
                ("Add to Rows", "rows"),
            ] {
                if button(ui, label, &format!("{label} {name}")) {
                    asks.push(Ask::Words(say(&format!("pivot add #{} {to} {at}", f + 1))));
                    ui.close();
                }
            }
            if button(ui, "Add to Values", &format!("Add to Values {name}")) {
                asks.push(Ask::Words(say(&format!("pivot add #{} values sum {at}", f + 1))));
                ui.close();
            }
        });
    });
}

/// One of the four areas as a drop zone listing its fields, each with a menu to move or remove it.
fn area_box(
    ui: &mut egui::Ui,
    book: &Book,
    t: usize,
    title: &str,
    area: &str,
    at: &str,
    asks: &mut Vec<Ask>,
) {
    let table = &book.session.book.pivots.tables[t];
    let cache = &book.session.book.pivots.caches[table.cache];
    ui.label(egui::RichText::new(title).strong());
    let (_, dropped) = ui.dnd_drop_zone::<usize, _>(egui::Frame::group(ui.style()), |ui| {
        ui.set_min_size(Vec2::new(100.0, 44.0));
        for (place, entry) in area_entries(table, area).into_iter().enumerate() {
            area_entry(ui, book, t, area, place, entry, at, asks);
        }
        let _ = cache;
    });
    if let Some(f) = dropped {
        let words = if area == "values" {
            format!("pivot add #{} values sum {at}", *f + 1)
        } else {
            format!("pivot move #{} {area} {at}", *f + 1)
        };
        asks.push(Ask::Words(say(&words)));
    }
}

/// What an area holds: fields, the Values entry for rows and columns, or value fields for Values.
#[derive(Clone, Copy)]
enum Entry {
    Field(usize),
    Values,
    Data(usize),
}

fn area_entries(table: &unluminous_sheet::pivot::PivotTable, area: &str) -> Vec<Entry> {
    let axis = |list: &[AxisEntry]| {
        list.iter()
            .map(|e| match e {
                AxisEntry::Field(f) => Entry::Field(*f),
                AxisEntry::Values => Entry::Values,
            })
            .collect()
    };
    match area {
        "rows" => axis(&table.rows),
        "columns" => axis(&table.cols),
        "filters" => table.pages.iter().map(|p| Entry::Field(p.field)).collect(),
        _ => (0..table.data.len()).map(Entry::Data).collect(),
    }
}

/// One entry of an area with its menu: move it up, to another area, remove it, or open its settings.
#[allow(clippy::too_many_arguments)]
fn area_entry(
    ui: &mut egui::Ui,
    book: &Book,
    t: usize,
    area: &str,
    place: usize,
    entry: Entry,
    at: &str,
    asks: &mut Vec<Ask>,
) {
    let table = &book.session.book.pivots.tables[t];
    let cache = &book.session.book.pivots.caches[table.cache];
    let name = match entry {
        Entry::Field(f) => table.caption(cache, f),
        Entry::Values => "\u{03A3} Values".to_owned(),
        Entry::Data(d) => table.data[d].name.clone(),
    };
    ui.horizontal(|ui| {
        ui.label(&name);
        ui.menu_button("\u{25BE}", |ui| match entry {
            Entry::Field(f) => field_menu(ui, area, place, f, &name, at, asks),
            Entry::Values => {
                let to = if area == "rows" { "columns" } else { "rows" };
                if button(ui, &format!("Move Values to {to}"), "Move Values") {
                    asks.push(Ask::Words(say(&format!("pivot move-values {to} {at}"))));
                }
            }
            Entry::Data(d) => {
                if button(ui, "Value Field Settings...", &format!("Value Field Settings {name}")) {
                    if let Item::Form { title, words, fields, .. } =
                        catalogue_b::value_form(book, t, d, at)
                    {
                        asks.push(Ask::Form(Prompt::form(words, title, fields)));
                    }
                }
                let f = table.data[d].field;
                if button(ui, "Remove Field", &format!("Remove Field {name}")) {
                    asks.push(Ask::Words(say(&format!("pivot remove #{} {at}", f + 1))));
                }
                ui.label(egui::RichText::new(func_word(table.data[d].func)).weak());
            }
        });
    });
}

/// The menu of a field in the rows, columns or filters.
fn field_menu(
    ui: &mut egui::Ui,
    area: &str,
    place: usize,
    f: usize,
    name: &str,
    at: &str,
    asks: &mut Vec<Ask>,
) {
    let mut ask = |text: String| asks.push(Ask::Words(say(&text)));
    if place > 0 && button(ui, "Move Up", &format!("Move Up {name}")) {
        ask(format!("pivot move #{} {area} at {place} {at}", f + 1));
    }
    for (label, to) in
        [("Move to Filters", "filters"), ("Move to Columns", "columns"), ("Move to Rows", "rows")]
    {
        if to != area && button(ui, label, &format!("{label} {name}")) {
            ask(format!("pivot move #{} {to} {at}", f + 1));
        }
    }
    if button(ui, "Remove Field", &format!("Remove Field {name}")) {
        ask(format!("pivot remove #{} {at}", f + 1));
    }
}

// ------------------------------------------------------------------------------------------- slicers

fn slicers(ui: &egui::Ui, book: &mut Book, frame: Frame, look: &Look<'_>, words: &mut Words) {
    if book.view.axes.is_none() {
        return;
    }
    let all = book.view.lane_b.slicers.clone();
    for s in all.iter().filter(|s| s.sheet == book.view.sheet) {
        let spot = frame.cell(book, s.at);
        if !frame.cells.contains(spot.min) {
            continue;
        }
        let data = verbs_b_pivot_more::slicer_json(book, s);
        let items: Vec<(String, bool)> = data["items"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|i| {
                        (
                            i["name"].as_str().unwrap_or("").to_owned(),
                            i["selected"].as_bool().unwrap_or(true),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let id = egui::Id::new(("spreadsheet-slicer", &book.key, &s.name));
        let ctrl = ui.input(|i| i.modifiers.command);
        card(ui, look, id, spot.min, |ui| {
            ui.set_width(150.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&s.name).strong().color(look.palette.text_strong));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if button(ui, "\u{2715}", &format!("Clear filter {}", s.name)) {
                        words.push(vec![
                            "slicer".into(),
                            "select".into(),
                            s.name.clone(),
                            "all".into(),
                        ]);
                    }
                });
            });
            for (name, selected) in &items {
                if ui.selectable_label(*selected, name).clicked() {
                    let chosen = chosen_after_click(&items, name, ctrl);
                    words.push(vec!["slicer".into(), "select".into(), s.name.clone(), chosen]);
                }
            }
        });
    }
}

/// The items a slicer keeps after a click: only the one clicked, or with Ctrl that one switched.
pub fn chosen_after_click(items: &[(String, bool)], clicked: &str, ctrl: bool) -> String {
    let mut keep: Vec<&str> = if ctrl {
        items.iter().filter(|(n, on)| *on && n != clicked).map(|(n, _)| n.as_str()).collect()
    } else {
        Vec::new()
    };
    let was_on = items.iter().any(|(n, on)| n == clicked && *on);
    if !ctrl || !was_on {
        keep.push(clicked);
    }
    if keep.is_empty() || keep.len() == items.len() {
        "all".to_owned()
    } else {
        keep.join(",")
    }
}

// ----------------------------------------------------------------------------------------- timelines

fn timelines(ui: &egui::Ui, book: &mut Book, frame: Frame, look: &Look<'_>, words: &mut Words) {
    if book.view.axes.is_none() {
        return;
    }
    let all = book.view.lane_b.timelines.clone();
    for x in all.iter().filter(|x| x.sheet == book.view.sheet) {
        let spot = frame.cell(book, x.at);
        if !frame.cells.contains(spot.min) {
            continue;
        }
        let months = months_of(book, x);
        let id = egui::Id::new(("spreadsheet-timeline", &book.key, &x.name));
        card(ui, look, id, spot.min, |ui| {
            ui.set_width(220.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&x.name).strong().color(look.palette.text_strong));
                if button(ui, "All periods", &format!("All periods {}", x.name)) {
                    words.push(vec![
                        "timeline".into(),
                        "select".into(),
                        x.name.clone(),
                        "all".into(),
                    ]);
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (label, first, last) in &months {
                    let on = x.span.is_some_and(|(a, b)| a <= *first && b >= *last);
                    if ui.selectable_label(on, label).clicked() {
                        words.push(vec![
                            "timeline".into(),
                            "select".into(),
                            x.name.clone(),
                            first.to_string(),
                            last.to_string(),
                        ]);
                    }
                }
            });
        });
    }
}

/// The months the dates of a timeline's field fall in: a label and the first and last serial of each.
fn months_of(
    book: &Book,
    x: &crate::services::spreadsheet::view_b::Timeline,
) -> Vec<(String, f64, f64)> {
    let pivots = &book.session.book.pivots;
    let Some(t) = pivots.find(&x.pivot) else { return Vec::new() };
    let Ok(f) = crate::services::spreadsheet::verbs_b_pivot::field_in(book, t, &x.field) else {
        return Vec::new();
    };
    let cache = &pivots.caches[pivots.tables[t].cache];
    let mut months: Vec<(i64, u32)> = cache.fields[f]
        .items()
        .iter()
        .filter_map(|v| match v {
            CacheValue::Date(n) | CacheValue::Number(n) => {
                unluminous_sheet::dates::date(*n, false).map(|(y, m, _)| (y, m))
            }
            _ => None,
        })
        .collect();
    months.sort_unstable();
    months.dedup();
    months
        .into_iter()
        .filter_map(|(y, m)| {
            let first = unluminous_sheet::dates::serial(y, m as i64, 1, false)?;
            let last = first + unluminous_sheet::dates::days_in_month(y, m) as f64 - 1.0;
            Some((format!("{} {y}", MONTHS[m as usize - 1]), first, last))
        })
        .collect()
}

const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The rectangle a card at a cell takes.
pub fn spot_of(book: &Book, frame: Frame, at: CellRef) -> Rect {
    frame.cell(book, at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(on: &[bool]) -> Vec<(String, bool)> {
        ["East", "West", "North"].iter().zip(on).map(|(n, o)| ((*n).to_owned(), *o)).collect()
    }

    #[test]
    fn a_click_on_a_slicer_keeps_one_item_and_ctrl_adds_or_takes_away() {
        let all = items(&[true, true, true]);
        assert_eq!(chosen_after_click(&all, "West", false), "West");
        let one = items(&[false, true, false]);
        assert_eq!(chosen_after_click(&one, "North", true), "West,North");
        assert_eq!(
            chosen_after_click(&one, "West", true),
            "all",
            "taking the last one away shows everything"
        );
        assert_eq!(chosen_after_click(&items(&[true, true, false]), "North", true), "all");
    }
}
