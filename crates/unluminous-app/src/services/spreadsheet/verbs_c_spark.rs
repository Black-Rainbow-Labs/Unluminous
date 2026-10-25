//! The verbs for sparklines (`task-2360`, the `wiringc` lane): insert them from a data range into a location
//! range, change every option a group has, group and ungroup them, and read them back. A sparkline is chosen
//! by a cell; the active cell is the default.

use serde_json::{json, Value};
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::SparklinePoints;
use unluminous_sheet::objects::sparkline::{
    AxisBound, EmptyCells, SparklineColors, SparklineGroup, SparklineKind,
};
use unluminous_sheet::styles::Color;

use super::commands::parse_range;
use super::verbs_a::{edit, flag, on_off, take_cell, Out};
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// Run a verb this module knows, or answer `None` for one it does not.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    match verb {
        "sparklines" => Some(list(sheet, args)),
        "sparkline" => Some(sparkline(sheet, args)),
        _ => None,
    }
}

const WORDS: &str =
    "sparkline takes insert, kind, points, color, weight, axis, haxis, dates, empty, \
    hidden, data, group, ungroup, clear or clear-group";

/// The word for a kind.
fn kind_word(k: SparklineKind) -> &'static str {
    match k {
        SparklineKind::Line => "line",
        SparklineKind::Column => "column",
        SparklineKind::WinLoss => "winloss",
    }
}

/// A kind from its word.
fn kind_of(word: &str) -> Result<SparklineKind, String> {
    match word {
        "line" => Ok(SparklineKind::Line),
        "column" => Ok(SparklineKind::Column),
        "winloss" | "win-loss" => Ok(SparklineKind::WinLoss),
        other => Err(format!("a sparkline is line, column or winloss; got `{other}`")),
    }
}

/// One bound of the axis as a word.
fn bound_word(b: AxisBound) -> Value {
    match b {
        AxisBound::Automatic => json!("auto"),
        AxisBound::SameForAll => json!("same"),
        AxisBound::Custom(v) => json!(v),
    }
}

/// A colour of a group as `#RRGGBB`, when it has one.
fn colour_word(c: Option<Color>) -> Value {
    match c {
        Some(Color::Rgb(v)) => json!(format!("#{:06X}", v & 0xFF_FFFF)),
        Some(_) => json!("theme"),
        None => Value::Null,
    }
}

/// One group as data.
fn group_value(index: usize, g: &SparklineGroup) -> Value {
    let c = &g.colors;
    json!({
        "group": index + 1, "kind": kind_word(g.kind), "weight": g.weight,
        "empty": match g.empty { EmptyCells::Gap => "gap", EmptyCells::Zero => "zero", EmptyCells::Span => "span" },
        "markers": g.markers, "high": g.high, "low": g.low, "first": g.first, "last": g.last,
        "negative": g.negative, "axis": g.show_axis, "hiddenCells": g.hidden_cells,
        "rightToLeft": g.right_to_left, "min": bound_word(g.min), "max": bound_word(g.max),
        "dates": g.date_axis,
        "colors": {"series": colour_word(c.series), "negative": colour_word(c.negative),
            "axis": colour_word(c.axis), "markers": colour_word(c.markers),
            "first": colour_word(c.first), "last": colour_word(c.last),
            "high": colour_word(c.high), "low": colour_word(c.low)},
        "sparklines": g.sparklines.iter().map(|s| json!({"data": s.data, "location": s.location.a1()})).collect::<Vec<_>>(),
    })
}

/// `sparklines`: every group on the sheet showing.
fn list(sheet: &mut Spreadsheet, _args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let s = &book.session.book.sheets[book.view.sheet];
    let rows: Vec<Value> =
        s.sparklines.iter().enumerate().map(|(i, g)| group_value(i, g)).collect();
    Ok(Answer::said(format!("{} sparkline groups", rows.len())).with(json!(rows)))
}

/// The group and the sparkline the cell holds.
fn found_at(book: &Book, at: CellRef) -> Result<(usize, usize), String> {
    book.session
        .sparkline_at(book.view.sheet, at)
        .ok_or_else(|| format!("There is no sparkline in {}", at.a1()))
}

/// The group of the sparkline at the cell an argument names, or the active cell, with the words left.
fn group_of<'a>(book: &Book, args: &'a [String]) -> Result<(usize, CellRef, &'a [String]), String> {
    let (at, rest) = take_cell(book, args);
    let (g, _) = found_at(book, at)?;
    Ok((g, at, rest))
}

/// `sparkline <what> ...`.
fn sparkline(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let Some(what) = args.first() else { return Err(WORDS.to_owned()) };
    let rest = &args[1..];
    match what.as_str() {
        "insert" => insert(sheet, rest),
        "kind" => change(sheet, rest, "Sparkline type set", |b, g, _, w| {
            let k = kind_of(w.first().ok_or("kind is line, column or winloss")?)?;
            b.session.set_sparkline_kind(b.view.sheet, g, k)
        }),
        "points" => change(sheet, rest, "Sparkline points set", set_points),
        "color" | "colour" => change(sheet, rest, "Sparkline colour set", set_color),
        "weight" => change(sheet, rest, "Sparkline weight set", |b, g, _, w| {
            let n =
                w.first().and_then(|v| v.parse().ok()).ok_or("weight needs a number of points")?;
            b.session.set_sparkline_weight(b.view.sheet, g, n)
        }),
        "axis" => change(sheet, rest, "Sparkline axis set", set_axis),
        "haxis" => change(sheet, rest, "Sparkline axis set", |b, g, _, w| {
            let show = on_off(w.first().map(String::as_str))?;
            let rtl = flag(w, "rtl") || flag(w, "right-to-left");
            b.session.set_sparkline_horizontal_axis(b.view.sheet, g, show, rtl)
        }),
        "dates" => change(sheet, rest, "Sparkline date axis set", |b, g, _, w| {
            let dates = w.first().filter(|d| d.as_str() != "off");
            b.session.set_sparkline_date_axis(b.view.sheet, g, dates.map(String::as_str))
        }),
        "empty" => change(sheet, rest, "Empty cells set", set_empty),
        "hidden" => change(sheet, rest, "Hidden cells set", |b, g, _, w| {
            let on = on_off(w.first().map(String::as_str))?;
            let empty = b.session.book.sheets[b.view.sheet].sparklines[g].empty;
            b.session.set_sparkline_hidden_and_empty(b.view.sheet, g, empty, on)
        }),
        "data" => set_data(sheet, rest),
        "group" => group(sheet, rest),
        "ungroup" => ungroup(sheet, rest),
        "clear" => clear(sheet, rest),
        "clear-group" => change(sheet, rest, "Sparkline group cleared", |b, _, at, _| {
            b.session.clear_sparkline_group(b.view.sheet, at)
        }),
        other => Err(format!("{WORDS}; got `{other}`")),
    }
}

/// `sparkline insert line|column|winloss <data> <location>`.
fn insert(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let kind =
        kind_of(args.first().ok_or("insert needs a type, a data range and a location range")?)?;
    let data = args.get(1).ok_or("insert needs a data range")?.clone();
    let place = args.get(2).ok_or("insert needs a location range")?.clone();
    edit(sheet, move |b| {
        let (data_sheet, data) =
            parse_range(b, &data).ok_or_else(|| format!("`{data}` is a range"))?;
        let (_, place) = parse_range(b, &place).ok_or_else(|| format!("`{place}` is a range"))?;
        let g = b.session.insert_sparklines(b.view.sheet, kind, data_sheet, data, place)?;
        b.view.cursor = place.first;
        Ok(format!("Inserted sparkline group {}", g + 1))
    })
}

/// Change the group of the sparkline in the cell the first word names, or the active cell.
fn change(
    sheet: &mut Spreadsheet,
    args: &[String],
    say: &str,
    change: impl FnOnce(&mut Book, usize, CellRef, &[String]) -> Result<(), String>,
) -> Out {
    let (g, at, rest) = {
        let book = sheet.current_book()?;
        let (g, at, rest) = group_of(book, args)?;
        (g, at, rest.to_vec())
    };
    let say = say.to_owned();
    edit(sheet, move |b| {
        change(b, g, at, &rest)?;
        Ok(say)
    })
}

/// `sparkline points markers on high off ...` or `sparkline points high` to turn those on.
fn set_points(b: &mut Book, g: usize, _: CellRef, w: &[String]) -> Result<(), String> {
    let cur = &b.session.book.sheets[b.view.sheet].sparklines[g];
    let mut p = SparklinePoints {
        markers: cur.markers,
        high: cur.high,
        low: cur.low,
        first: cur.first,
        last: cur.last,
        negative: cur.negative,
    };
    let mut i = 0;
    while i < w.len() {
        let state = w.get(i + 1).filter(|n| matches!(n.as_str(), "on" | "off"));
        let on = state.map(|s| s == "on").unwrap_or(true);
        let flag = match w[i].as_str() {
            "markers" => &mut p.markers,
            "high" => &mut p.high,
            "low" => &mut p.low,
            "first" => &mut p.first,
            "last" => &mut p.last,
            "negative" => &mut p.negative,
            other => {
                return Err(format!(
                    "points are markers, high, low, first, last and negative; got `{other}`"
                ))
            }
        };
        *flag = on;
        i += if state.is_some() { 2 } else { 1 };
    }
    b.session.set_sparkline_points(b.view.sheet, g, p)
}

/// `sparkline color series|negative|axis|markers|first|last|high|low #RRGGBB`.
fn set_color(b: &mut Book, g: usize, _: CellRef, w: &[String]) -> Result<(), String> {
    let part = w.first().ok_or("color needs a part and #RRGGBB")?;
    let hex = w.get(1).ok_or("color needs #RRGGBB")?;
    let c = Color::parse_hex(hex).ok_or_else(|| format!("`{hex}` is a colour written #RRGGBB"))?;
    let mut colors = SparklineColors::default();
    let slot = match part.as_str() {
        "series" => &mut colors.series,
        "negative" => &mut colors.negative,
        "axis" => &mut colors.axis,
        "markers" => &mut colors.markers,
        "first" => &mut colors.first,
        "last" => &mut colors.last,
        "high" => &mut colors.high,
        "low" => &mut colors.low,
        other => return Err(format!("a colour is for series, negative, axis, markers, first, last, high or low; got `{other}`")),
    };
    *slot = Some(c);
    b.session.set_sparkline_colors(b.view.sheet, g, colors)
}

/// A bound from `auto`, `same` or a number.
fn bound(word: &str) -> Result<AxisBound, String> {
    match word {
        "auto" | "automatic" => Ok(AxisBound::Automatic),
        "same" => Ok(AxisBound::SameForAll),
        n => n
            .parse()
            .map(AxisBound::Custom)
            .map_err(|_| format!("a bound is auto, same or a number; got `{n}`")),
    }
}

/// `sparkline axis min <auto|same|n> max <auto|same|n>`.
fn set_axis(b: &mut Book, g: usize, _: CellRef, w: &[String]) -> Result<(), String> {
    let cur = &b.session.book.sheets[b.view.sheet].sparklines[g];
    let (mut min, mut max) = (cur.min, cur.max);
    for pair in w.windows(2) {
        match pair[0].as_str() {
            "min" => min = bound(&pair[1])?,
            "max" => max = bound(&pair[1])?,
            _ => {}
        }
    }
    b.session.set_sparkline_axis(b.view.sheet, g, min, max)
}

/// `sparkline empty gap|zero|span`.
fn set_empty(b: &mut Book, g: usize, _: CellRef, w: &[String]) -> Result<(), String> {
    let empty = match w.first().map(String::as_str) {
        Some("gap") => EmptyCells::Gap,
        Some("zero") => EmptyCells::Zero,
        Some("span") => EmptyCells::Span,
        _ => return Err("empty cells are shown as gap, zero or span".to_owned()),
    };
    let hidden = b.session.book.sheets[b.view.sheet].sparklines[g].hidden_cells;
    b.session.set_sparkline_hidden_and_empty(b.view.sheet, g, empty, hidden)
}

/// `sparkline data [cell] <range>`: new data for one sparkline.
fn set_data(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let (at, word) = {
        let book = sheet.current_book()?;
        let (at, rest) = take_cell(book, args);
        let word = rest.first().cloned().ok_or("data needs a range")?;
        (at, word)
    };
    edit(sheet, move |b| {
        let (data_sheet, range) =
            parse_range(b, &word).ok_or_else(|| format!("`{word}` is a range"))?;
        b.session.set_sparkline_data(b.view.sheet, at, data_sheet, range)?;
        Ok("Sparkline data set".to_owned())
    })
}

/// The cells of a range, in reading order.
fn cells_of(range: Range) -> Vec<CellRef> {
    let mut out = Vec::new();
    for row in range.first.row..=range.last.row {
        for col in range.first.col..=range.last.col {
            out.push(CellRef::new(row, col));
        }
    }
    out
}

/// `sparkline group <range>`: one group, so the looks are shared.
fn group(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let word = args.first().cloned();
    edit(sheet, move |b| {
        let range = match word.as_deref() {
            Some(w) => parse_range(b, w).ok_or_else(|| format!("`{w}` is a range"))?.1,
            None => b.view.range(),
        };
        b.session.group_sparklines(b.view.sheet, &cells_of(range))?;
        Ok("Sparklines grouped".to_owned())
    })
}

/// `sparkline ungroup [cell]`.
fn ungroup(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    change(sheet, args, "Sparkline ungrouped", |b, _, at, _| {
        b.session.ungroup_sparkline(b.view.sheet, at).map(|_| ())
    })
}

/// `sparkline clear [range]`: the chosen cells by default.
fn clear(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let word = args.first().cloned();
    edit(sheet, move |b| {
        let range = match word.as_deref() {
            Some(w) => parse_range(b, w).ok_or_else(|| format!("`{w}` is a range"))?.1,
            None => b.view.range(),
        };
        let n = b.session.clear_sparklines(b.view.sheet, range)?;
        Ok(format!("Cleared {n} sparklines"))
    })
}
