//! The chart verbs (`task-2360`, the `chartwire` lane): insert a chart of any type Excel offers, a chart
//! sheet, the recommended charts, and the changes that act on the chart as a whole (its data, its place, its
//! name, title, legend, data table, quick layout and style). The series, labels, trendlines, error bars and
//! group options are in [`super::verbs_charts_series`], the axes and element formats in
//! [`super::verbs_charts_axis`] and the read back in [`super::verbs_charts_info`].
//!
//! A chart is named by its number from 1 on the sheet showing, as `charts` lists them. A menu entry, a
//! pointer drag and an agent's call all end in these words, so they change the workbook the same way.

use serde_json::{json, Value};
use unluminous_sheet::chartdata::PlotBy;
use unluminous_sheet::chartspec::{Blanks, ChartType, DataTable, LegendPosition};
use unluminous_sheet::coord::{column_name, CellRef, Range};
use unluminous_sheet::objects::geometry;
use unluminous_sheet::objects::EMU_PER_POINT;

use super::actions::block;
use super::commands::{parse_range, take_range};
use super::verbs_a::{edit, Out};
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// Run a chart verb, or answer `None` for a verb that is not about charts.
pub fn first(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "chart" => chart(sheet, args),
        "chart-types" => super::verbs_charts_info::types(),
        "chart-info" => super::verbs_charts_info::info(sheet, args),
        "chart-data" => chart_on(sheet, args, data),
        "chart-move" => chart_on(sheet, args, relocate),
        "chart-name" => chart_on(sheet, args, name),
        "chart-title" => chart_on(sheet, args, title),
        "chart-legend" => chart_on(sheet, args, legend),
        "chart-table" => chart_on(sheet, args, table),
        "chart-look" => chart_on(sheet, args, look),
        _ => {
            return super::verbs_charts_series::run(sheet, verb, args)
                .or_else(|| super::verbs_charts_axis::run(sheet, verb, args))
        }
    })
}

// ------------------------------------------------------------------------------------------ the words

/// The words after a chart's number, read one at a time.
pub struct Words<'a> {
    args: &'a [String],
    at: usize,
}

impl<'a> Words<'a> {
    /// Read these words from the first.
    pub fn new(args: &'a [String]) -> Self {
        Self { args, at: 0 }
    }

    /// The next word, taken.
    pub fn take(&mut self) -> Option<&'a str> {
        let word = self.args.get(self.at)?;
        self.at += 1;
        Some(word.as_str())
    }

    /// The next word, left where it is.
    pub fn peek(&self) -> Option<&'a str> {
        self.args.get(self.at).map(String::as_str)
    }

    /// Whether every word has been read.
    pub fn done(&self) -> bool {
        self.at >= self.args.len()
    }

    /// The next word, refused when there is none.
    pub fn need(&mut self, what: &str) -> Result<&'a str, String> {
        self.take().ok_or_else(|| format!("that needs {what}"))
    }

    /// The next word as a number.
    pub fn number(&mut self, what: &str) -> Result<f64, String> {
        let word = self.need(what)?;
        word.parse::<f64>().map_err(|_| format!("`{word}` is not a number for {what}"))
    }

    /// The next word as a whole number.
    pub fn whole(&mut self, what: &str) -> Result<u32, String> {
        let word = self.need(what)?;
        word.parse::<u32>().map_err(|_| format!("`{word}` is not a whole number for {what}"))
    }

    /// The next word as a position counted from 1, answered counted from 0.
    pub fn place(&mut self, what: &str) -> Result<usize, String> {
        let n = self.whole(what)?;
        n.checked_sub(1).map(|n| n as usize).ok_or_else(|| format!("{what} counts from 1"))
    }

    /// The next word as `on` or `off`.
    pub fn onoff(&mut self, what: &str) -> Result<bool, String> {
        let word = self.need(what)?;
        on_off(word)
    }

    /// The words up to one of the stop words, joined by spaces.
    pub fn text(&mut self, stops: &[&str]) -> String {
        let mut parts = Vec::new();
        while let Some(word) = self.peek() {
            if stops.iter().any(|s| word.eq_ignore_ascii_case(s)) {
                break;
            }
            parts.push(word);
            self.at += 1;
        }
        parts.join(" ")
    }
}

/// `on` or `off` as a switch.
pub fn on_off(word: &str) -> Result<bool, String> {
    match word.to_ascii_lowercase().as_str() {
        "on" | "yes" | "true" | "show" => Ok(true),
        "off" | "no" | "false" | "hide" => Ok(false),
        other => Err(format!("`{other}` is not on or off")),
    }
}

/// A colour written `#RRGGBB`.
pub fn colour(word: &str) -> Result<(u8, u8, u8), String> {
    let hex = word.strip_prefix('#').filter(|h| h.len() == 6);
    let value = hex.and_then(|h| u32::from_str_radix(h, 16).ok());
    value
        .map(|v| ((v >> 16) as u8, (v >> 8) as u8, v as u8))
        .ok_or_else(|| format!("`{word}` is a colour written #RRGGBB"))
}

/// A line's colour and width in points.
pub type Line = ((u8, u8, u8), f64);

/// A colour for a fill, `none` taking it away.
pub fn fill_word(word: &str) -> Result<Option<(u8, u8, u8)>, String> {
    if word == "none" {
        Ok(None)
    } else {
        colour(word).map(Some)
    }
}

/// A line from `#RRGGBB [width]` or `none`; the width in points is `plain` when none follows.
pub fn line_word(w: &mut Words, plain: f64) -> Result<Option<Line>, String> {
    let Some(rgb) = fill_word(w.need("a colour or none")?)? else { return Ok(None) };
    let width = match w.peek().and_then(|p| p.parse::<f64>().ok()) {
        Some(width) => {
            w.take();
            width
        }
        None => plain,
    };
    Ok(Some((rgb, width)))
}
/// A cell range written as the chart reads it: the sheet's name and the range with dollar signs.
pub fn reference(book: &Book, word: &str) -> Result<String, String> {
    if word.starts_with('"') {
        return Ok(word.to_owned());
    }
    let (sheet, range) =
        parse_range(book, word).ok_or_else(|| format!("`{word}` is not a range"))?;
    let name = &book.session.book.sheets[sheet].name;
    let quoted = if name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        name.clone()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    };
    let a = |at: CellRef| format!("${}${}", column_name(at.col), at.row + 1);
    let cells = if range.len() == 1 {
        a(range.first)
    } else {
        format!("{}:{}", a(range.first), a(range.last))
    };
    Ok(format!("{quoted}!{cells}"))
}

/// A series name given as a word: a cell, or the words themselves in quotation marks.
pub fn name_source(book: &Book, text: &str) -> Result<String, String> {
    match reference(book, text) {
        Ok(found) if !text.starts_with('"') => Ok(found),
        _ => Ok(format!("\"{}\"", text.trim_matches('"').replace('"', "\"\""))),
    }
}

/// The chart number a verb starts with, counted from 1, answered counted from 0 with the words after it.
pub fn chart_number(args: &[String]) -> Result<(usize, &[String]), String> {
    let word = args.first().ok_or("name the chart by its number, from 1; `charts` lists them")?;
    let n: usize =
        word.parse().map_err(|_| format!("`{word}` is not a chart number; `charts` lists them"))?;
    if n == 0 {
        return Err("charts are numbered from 1".to_owned());
    }
    Ok((n - 1, &args[1..]))
}

/// Run one change on the chart a verb names, marking the workbook as changed when it worked.
pub fn chart_on(
    sheet: &mut Spreadsheet,
    args: &[String],
    change: impl FnOnce(&mut Book, usize, usize, &[String]) -> Result<String, String>,
) -> Out {
    edit(sheet, |book| {
        let (index, rest) = chart_number(args)?;
        let at = book.view.sheet;
        if book.session.book.sheets[at].charts.get(index).is_none() {
            return Err(format!(
                "there is no chart {} on this sheet; `charts` lists them",
                index + 1
            ));
        }
        change(book, at, index, rest)
    })
}

// ------------------------------------------------------------------------------------------ inserting

/// Run a change that answers with data as well as a sentence, and mark the workbook as changed.
fn edit_json(
    sheet: &mut Spreadsheet,
    change: impl FnOnce(&mut Book) -> Result<(String, Value), String>,
) -> Out {
    let book = sheet.current_book()?;
    let (message, data) = change(book)?;
    book.changed();
    book.view.axes = None;
    Ok(Answer::said(message).with(data))
}

/// `chart <type> [range] [title]`, `chart sheet [type] [range]`, `chart recommend [range]` and
/// `chart delete <n>`.
fn chart(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    match args.first().map(String::as_str) {
        Some("delete") => chart_on(sheet, &args[1..], |book, at, index, _| {
            book.session.delete_chart(at, index)?;
            book.view.chart = None;
            Ok(format!("Deleted chart {}", index + 1))
        }),
        Some("recommend") => recommend(sheet, &args[1..]),
        Some("sheet") => make(sheet, &args[1..], true),
        Some(_) => make(sheet, args, false),
        None => Err("chart needs a type; `chart-types` lists them".to_owned()),
    }
}

/// The type a word names, or the default when a chart sheet is asked for with none.
fn type_of(args: &[String], default: Option<ChartType>) -> Result<(ChartType, &[String]), String> {
    match args.first().and_then(|w| ChartType::parse(w)) {
        Some(ty) => Ok((ty, &args[1..])),
        None => default
            .map(|ty| (ty, args))
            .ok_or_else(|| "chart needs a type; `chart-types` lists them".to_owned()),
    }
}

/// Insert a chart over the range the words name, or over the cells chosen.
fn make(sheet: &mut Spreadsheet, args: &[String], on_sheet: bool) -> Out {
    let default = on_sheet.then_some(ChartType::ColumnClustered);
    let (ty, rest) = type_of(args, default)?;
    edit_json(sheet, |book| {
        let rest = take_range(book, rest);
        let range = block(book);
        let at = book.view.sheet;
        let title = (!rest.is_empty()).then(|| rest.join(" "));
        let (number, sheet_name) = if on_sheet {
            let made = book.session.insert_chart_sheet(at, range, ty)?;
            book.view.sheet = made;
            (1, book.session.book.sheets[made].name.clone())
        } else if !book.view.extra.is_empty() {
            // Several areas chosen, as `B4:B9,D4:D9`: the first one's names are the categories.
            let mut areas = book.view.extra.clone();
            areas.push(book.view.range());
            let index = book.session.insert_chart_areas(at, &areas, ty, title)?;
            book.view.chart = Some(index);
            (index + 1, book.session.book.sheets[at].name.clone())
        } else {
            let index = book.session.insert_chart(at, range, ty, title)?;
            book.view.chart = Some(index);
            (index + 1, book.session.book.sheets[at].name.clone())
        };
        let chosen = match book.view.extra.is_empty() {
            true => range.a1(),
            false => {
                let mut all: Vec<String> = book.view.extra.iter().map(|r| r.a1()).collect();
                all.push(book.view.range().a1());
                all.join(",")
            }
        };
        let message = format!("Made a {} chart of {chosen} (chart {number})", ty.name());
        Ok((message, json!({ "chart": number, "sheet": sheet_name, "type": ty.name() })))
    })
}

/// `chart recommend [range]`: the five charts Excel offers for the data, best first.
fn recommend(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    take_range(book, args);
    let range = block(book);
    let found = book.session.recommended_charts(book.view.sheet, range)?;
    let names: Vec<&str> = found.iter().map(|t| t.name()).collect();
    Ok(Answer::said(format!("Recommended for {}: {}", range.a1(), names.join(", ")))
        .with(json!({ "range": range.a1(), "types": names })))
}

// ------------------------------------------------------------------------------------------ the data

/// `chart-data <n> range <range> [rows|columns]`, `switch`, `cells [hidden|visible] [gap|zero|span]` and
/// `hide-categories <positions>|none`.
fn data(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    match w.need("range, switch, cells or hide-categories")? {
        "range" => {
            let word = w.need("a range")?;
            let (found, range) =
                parse_range(book, word).ok_or_else(|| format!("`{word}` is not a range"))?;
            if found != at {
                return Err("the data range must be on the sheet the chart is on".to_owned());
            }
            let by = match w.take() {
                Some("rows") => Some(PlotBy::Rows),
                Some("columns") => Some(PlotBy::Columns),
                Some(other) => return Err(format!("`{other}` is not rows or columns")),
                None => None,
            };
            book.session.set_chart_source(at, index, range, by)?;
            Ok(format!("Chart {} now reads {}", index + 1, range.a1()))
        }
        "switch" => {
            book.session.switch_row_column(at, index)?;
            Ok(format!("Switched the rows and columns of chart {}", index + 1))
        }
        "cells" => hidden_and_empty(book, at, index, &mut w),
        "hide-categories" => {
            let hidden = categories_hidden(&mut w)?;
            book.session.filter_categories(at, index, hidden)?;
            Ok(format!("Chart {} filtered", index + 1))
        }
        other => {
            Err(format!("chart-data takes range, switch, cells or hide-categories, not `{other}`"))
        }
    }
}

/// `cells hidden|visible gap|zero|span`: whether hidden cells are plotted and how empty cells are.
fn hidden_and_empty(
    book: &mut Book,
    at: usize,
    index: usize,
    w: &mut Words,
) -> Result<String, String> {
    let chart = &book.session.book.sheets[at].charts.get(index).ok_or("there is no such chart")?;
    let mut show_hidden = !chart.spec.visible_only();
    let mut blanks = chart.spec.blanks_as();
    while let Some(word) = w.take() {
        match word {
            "hidden" => show_hidden = true,
            "visible" => show_hidden = false,
            "gap" => blanks = Blanks::Gap,
            "zero" => blanks = Blanks::Zero,
            "span" => blanks = Blanks::Span,
            other => return Err(format!("`{other}` is not hidden, visible, gap, zero or span")),
        }
    }
    book.session.set_hidden_and_empty_cells(at, index, show_hidden, blanks)?;
    Ok(format!("Chart {} plots hidden cells: {show_hidden}", index + 1))
}

/// The categories to hide, written as positions from 1 joined by commas, or `none`.
fn categories_hidden(w: &mut Words) -> Result<Vec<u32>, String> {
    let word = w.need("positions such as 1,3, or none")?;
    if word == "none" {
        return Ok(Vec::new());
    }
    word.split(',')
        .map(|p| {
            p.trim()
                .parse::<u32>()
                .ok()
                .and_then(|n| n.checked_sub(1))
                .ok_or_else(|| format!("`{p}` is not a position counted from 1"))
        })
        .collect()
}

// ------------------------------------------------------------------------------------------ the place

/// A distance in points as drawing units.
fn emu(points: f64) -> i64 {
    (points * EMU_PER_POINT as f64).round() as i64
}

/// `chart-move <n> <left> <top> [width height]` in points from the sheet's top left corner,
/// `chart-move <n> sheet <name>` or `chart-move <n> new-sheet [name]`.
fn relocate(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    match w.peek() {
        Some("sheet") => {
            w.take();
            let name = w.text(&[]);
            let target = book
                .session
                .book
                .sheet_index(&name)
                .ok_or_else(|| format!("there is no sheet called {name}"))?;
            let made = book.session.move_chart_to_sheet(at, index, target)?;
            book.view.sheet = target;
            book.view.chart = Some(made);
            Ok(format!("Moved the chart to {name} as chart {}", made + 1))
        }
        Some("new-sheet") => {
            w.take();
            let name = w.text(&[]);
            let made = book.session.move_chart_to_new_sheet(
                at,
                index,
                Some(name.as_str()).filter(|n| !n.is_empty()),
            )?;
            book.view.sheet = made;
            book.view.chart = None;
            Ok(format!(
                "Moved the chart to the chart sheet {}",
                book.session.book.sheets[made].name
            ))
        }
        _ => place(book, at, index, &mut w),
    }
}

/// Move a chart to a place in points, and size it too when a width and height follow.
fn place(book: &mut Book, at: usize, index: usize, w: &mut Words) -> Result<String, String> {
    let left = w.number("a left position in points")?;
    let top = w.number("a top position in points")?;
    let sheet = &book.session.book.sheets[at];
    let chart = sheet.charts.get(index).ok_or("there is no such chart")?;
    let (x0, y0) = geometry::point_of(sheet, chart.from);
    let (x1, y1) = geometry::point_of(sheet, chart.to);
    let (width, height) = if w.done() {
        (x1 - x0, y1 - y0)
    } else {
        (emu(w.number("a width in points")?), emu(w.number("a height in points")?))
    };
    let from = geometry::cell_at(sheet, emu(left), emu(top));
    let to = geometry::cell_at(sheet, emu(left) + width, emu(top) + height);
    book.session.move_chart(at, index, from, to)?;
    Ok(format!("Chart {} is at {}", index + 1, Range::new(from.0, to.0).a1()))
}

// ------------------------------------------------------------------------------------------ the whole

/// `chart-name <n> name <words>` and `alt <words>|none`.
fn name(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    let (mut name, mut alt) = (None, None);
    while let Some(key) = w.take() {
        match key {
            "name" => name = Some(w.text(&["alt"])),
            "alt" => {
                let text = w.text(&["name"]);
                alt = Some(Some(text).filter(|t| t != "none"));
            }
            other => return Err(format!("chart-name takes name or alt, not `{other}`")),
        }
    }
    if name.is_none() && alt.is_none() {
        return Err("chart-name needs name <words> or alt <words>".to_owned());
    }
    book.session.set_chart_name(at, index, name, alt)?;
    Ok(format!("Named chart {}", index + 1))
}

/// `chart-title <n> <words>|off [overlay on|off]`.
fn title(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    let text = w.text(&["overlay"]);
    let overlay =
        if w.take() == Some("overlay") { Some(w.onoff("overlay on or off")?) } else { None };
    if text.is_empty() && overlay.is_none() {
        return Err("chart-title needs the words of the title, or off".to_owned());
    }
    let current = book.session.book.sheets[at]
        .charts
        .get(index)
        .ok_or("there is no such chart")?
        .title
        .clone();
    let words = match text.as_str() {
        "" => current,
        "off" => None,
        _ => Some(text),
    };
    book.session.set_chart_title(at, index, words, overlay)?;
    Ok(format!("Chart {} title set", index + 1))
}

/// `chart-legend <n> off|bottom|top|left|right|top-right [overlay]`.
fn legend(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    let position = match w.need("a position or off")? {
        "off" => None,
        "bottom" => Some(LegendPosition::Bottom),
        "top" => Some(LegendPosition::Top),
        "left" => Some(LegendPosition::Left),
        "right" => Some(LegendPosition::Right),
        "top-right" => Some(LegendPosition::TopRight),
        other => {
            return Err(format!("`{other}` is not off, bottom, top, left, right or top-right"))
        }
    };
    let overlay = w.take() == Some("overlay");
    book.session.set_legend(at, index, position, overlay)?;
    Ok(format!("Chart {} legend set", index + 1))
}

/// `chart-table <n> on|off [horizontal] [vertical] [outline] [keys]`: the data table under the chart.
fn table(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    let table = if w.onoff("on or off")? {
        let flags: Vec<&str> = std::iter::from_fn(|| w.take()).collect();
        let all = flags.is_empty();
        let has = |f: &str| all || flags.contains(&f);
        Some(DataTable {
            horizontal_borders: has("horizontal"),
            vertical_borders: has("vertical"),
            outline: has("outline"),
            legend_keys: has("keys"),
            ..DataTable::default()
        })
    } else {
        None
    };
    book.session.set_data_table(at, index, table)?;
    Ok(format!("Chart {} data table set", index + 1))
}

/// `chart-look <n> type <type>`, `layout <1 to 12>` or `style <1 to 48>`.
fn look(book: &mut Book, at: usize, index: usize, rest: &[String]) -> Result<String, String> {
    let mut w = Words::new(rest);
    match w.need("type, layout or style")? {
        "type" => {
            let word = w.need("a chart type")?;
            let ty = ChartType::parse(word)
                .ok_or_else(|| format!("`{word}` is not a chart type; `chart-types` lists them"))?;
            book.session.change_chart_type(at, index, ty)?;
            Ok(format!("Chart {} is now a {} chart", index + 1, ty.name()))
        }
        "layout" => {
            let n = w.whole("a layout number")?;
            book.session.apply_quick_layout(at, index, n)?;
            Ok(format!("Chart {} uses layout {n}", index + 1))
        }
        "style" => {
            let n = w.whole("a style number")?;
            book.session.set_chart_style(at, index, n)?;
            Ok(format!("Chart {} uses style {n}", index + 1))
        }
        other => Err(format!("chart-look takes type, layout or style, not `{other}`")),
    }
}
