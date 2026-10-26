//! Outlines, subtotals and the conditional format rules manager as verbs.

use serde_json::{json, Value as Json};
use unluminous_sheet::coord::{column_name, Range};
use unluminous_sheet::edit::{RuleEntry, RuleMove, SubtotalFunction, SubtotalSpec};
use unluminous_sheet::model::{icon_count, CfRule, Cfvo, DataBarExt, IconSetRule};
use unluminous_sheet::outline::{self, Dim};
use unluminous_sheet::styles::Color;

use super::commands::rule_from_words;
use super::verbs_a::{after, chosen, column_of, edit, flag, keyword, on_off, Out};
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// Run an outline, subtotal or rules verb.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "group" => group(sheet, args, true),
        "ungroup" => group(sheet, args, false),
        "outline" => outline_verb(sheet, args),
        "subtotal" => subtotal(sheet, args),
        "rules" => rules(sheet, args),
        _ => return None,
    })
}

// ------------------------------------------------------------------------------------------------ outline

/// Which way a `group` or `outline` verb works, from its first word or from what is chosen: whole
/// columns are columns and everything else is rows.
fn dimension<'a>(book: &Book, args: &'a [String]) -> (Dim, &'a [String]) {
    match keyword(args, &["rows", "columns"]) {
        Some(("rows", rest)) => (Dim::Rows, rest),
        Some((_, rest)) => (Dim::Columns, rest),
        None => {
            let r = book.view.range();
            let whole_columns =
                r.first.row == 0 && r.last.row >= unluminous_sheet::coord::MAX_ROWS - 1;
            (if whole_columns { Dim::Columns } else { Dim::Rows }, args)
        }
    }
}

/// `group [rows|columns] [range]` and `ungroup [rows|columns] [range]`: Shift+Alt+Right and Shift+Alt+Left.
fn group(sheet: &mut Spreadsheet, args: &[String], grouping: bool) -> Out {
    edit(sheet, |book| {
        let (dim, rest) = dimension(book, args);
        let (range, _) = chosen(book, rest);
        let (first, last) = match dim {
            Dim::Rows => (range.first.row, range.last.row),
            Dim::Columns => (range.first.col, range.last.col),
        };
        let index = book.view.sheet;
        match (dim, grouping) {
            (Dim::Rows, true) => book.session.group_rows(index, first, last)?,
            (Dim::Columns, true) => book.session.group_columns(index, first, last)?,
            (Dim::Rows, false) => book.session.ungroup_rows(index, first, last)?,
            (Dim::Columns, false) => book.session.ungroup_columns(index, first, last)?,
        }
        let what = if dim == Dim::Rows { "rows" } else { "columns" };
        Ok(format!("{} {what}", if grouping { "Grouped" } else { "Ungrouped" }))
    })
}

/// A line of the outline named by a number from 1 for a row or letters for a column.
fn line_index(dim: Dim, word: &str) -> Result<u32, String> {
    match dim {
        Dim::Rows => word
            .parse::<u32>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .ok_or_else(|| format!("`{word}` is not a row")),
        Dim::Columns => column_of(word),
    }
}

/// `outline` reads the outline back; the other words change it:
/// `level <rows> [columns]`, `show|hide rows|columns <line>`, `auto [range]`, `clear [rows|columns] [range]`,
/// `summary rows below|above` or `summary columns right|left`, `symbols on|off`, `styles on|off`.
fn outline_verb(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    if args.is_empty() {
        let book = sheet.current_book()?;
        let value = outline_json(book);
        return Ok(Answer::said("Outline").with(value));
    }
    let word = args[0].clone();
    let rest = args[1..].to_vec();
    edit(sheet, move |book| outline_change(book, &word, &rest))
}

fn outline_change(book: &mut Book, word: &str, rest: &[String]) -> Result<String, String> {
    let index = book.view.sheet;
    match word {
        "level" => {
            let level = |i: usize| rest.get(i).map_or(Ok(0u8), |w| w.parse::<u8>().map_err(|_| format!("`{w}` is not a level")));
            book.session.show_levels(index, level(0)?, level(1)?)?;
            Ok("Outline level shown".to_owned())
        }
        "show" | "hide" => {
            let (dim, tail) = match keyword(rest, &["rows", "columns"]) {
                Some(("rows", t)) => (Dim::Rows, t),
                Some((_, t)) => (Dim::Columns, t),
                None => return Err(format!("outline {word} takes rows or columns and a line")),
            };
            let line = line_index(dim, tail.first().ok_or("name the summary line")?)?;
            book.session.show_detail(index, dim, line, word == "show")?;
            Ok(format!("Detail {}", if word == "show" { "shown" } else { "hidden" }))
        }
        "auto" => {
            let within = rest.first().and_then(|w| Range::parse(&w.replace('$', "")));
            book.session.auto_outline(index, within)?;
            Ok("Outline made".to_owned())
        }
        "clear" => clear_outline(book, rest),
        "summary" => {
            let (dim, tail) = keyword(rest, &["rows", "columns"]).ok_or("summary takes rows or columns")?;
            let after_it = matches!(tail.first().map(String::as_str), Some("below") | Some("right"));
            if dim == "rows" {
                book.session.set_outline_summary(index, Some(after_it), None)?;
            } else {
                book.session.set_outline_summary(index, None, Some(after_it))?;
            }
            Ok("Summary position set".to_owned())
        }
        "symbols" => {
            let now = book.session.book.sheets[index].outline.show_symbols;
            let on = match rest.first().map(String::as_str) {
                Some("toggle") => !now,
                other => on_off(other)?,
            };
            book.session.set_outline_symbols(index, on)?;
            Ok("Outline symbols set".to_owned())
        }
        "styles" => {
            book.session.set_outline_styles(index, on_off(rest.first().map(String::as_str))?)?;
            Ok("Automatic styles set".to_owned())
        }
        other => Err(format!("outline takes level, show, hide, auto, clear, summary, symbols or styles, not `{other}`")),
    }
}

fn clear_outline(book: &mut Book, rest: &[String]) -> Result<String, String> {
    let index = book.view.sheet;
    if rest.is_empty() {
        book.session.clear_outline(index)?;
        return Ok("Outline cleared".to_owned());
    }
    let (dim, tail) = dimension(book, rest);
    let (range, _) = chosen(book, tail);
    let (first, last) = if dim == Dim::Rows {
        (range.first.row, range.last.row)
    } else {
        (range.first.col, range.last.col)
    };
    book.session.clear_outline_lines(index, dim, first, last)?;
    Ok("Outline cleared".to_owned())
}

/// The outline as data: the deepest levels, the settings, and every line that is in a group.
pub fn outline_json(book: &Book) -> Json {
    let session = &book.session;
    let index = book.view.sheet;
    let s = &session.book.sheets[index];
    let (rows, columns) = session.outline_depth(index);
    let lines = |dim: Dim, indexes: Vec<u32>| -> Vec<Json> {
        indexes
            .into_iter()
            .filter_map(|i| {
                let l = outline::line(s, dim, i);
                (l.level > 0 || l.collapsed).then(|| {
                    let name = if dim == Dim::Rows { (i + 1).to_string() } else { column_name(i) };
                    json!({ "line": name, "level": l.level, "hidden": l.hidden, "collapsed": l.collapsed })
                })
            })
            .collect()
    };
    let row_ids: Vec<u32> = s.rows.keys().copied().collect();
    let col_ids: Vec<u32> = (0..s.used_range().map_or(0, |r| r.last.col + 1)).collect();
    json!({
        "rowLevels": rows, "columnLevels": columns,
        "summaryBelow": s.outline.summary_below, "summaryRight": s.outline.summary_right,
        "symbols": s.outline.show_symbols, "automaticStyles": s.outline.apply_styles,
        "rows": lines(Dim::Rows, row_ids), "columns": lines(Dim::Columns, col_ids),
    })
}

// ----------------------------------------------------------------------------------------------- subtotal

/// `subtotal [range] at <column> function <name> add <columns> [nest] [page-breaks] [summary-above]`, or
/// `subtotal remove [range]`. The range holds the list with its header row first.
fn subtotal(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    edit(sheet, |book| {
        let index = book.view.sheet;
        if let Some(("remove", rest)) = keyword(args, &["remove"]) {
            let (range, _) = chosen(book, rest);
            book.session.remove_subtotal(index, range)?;
            return Ok("Subtotals removed".to_owned());
        }
        let (range, rest) = chosen(book, args);
        let offset = |word: &str| column_of(word).map(|c| c.saturating_sub(range.first.col));
        let group_by = offset(after(rest, "at").ok_or("subtotal needs `at <column>`")?)?;
        let function = match after(rest, "function") {
            Some(name) => SubtotalFunction::parse(name)
                .ok_or_else(|| format!("`{name}` is not a subtotal function"))?,
            None => SubtotalFunction::Sum,
        };
        let total_columns: Vec<u32> = after(rest, "add")
            .ok_or("subtotal needs `add <columns>`")?
            .split(',')
            .map(offset)
            .collect::<Result<_, _>>()?;
        let spec = SubtotalSpec {
            range,
            group_by,
            function,
            total_columns,
            replace: !flag(rest, "nest"),
            page_breaks: flag(rest, "page-breaks"),
            summary_below: !flag(rest, "summary-above"),
        };
        book.session.subtotal(index, &spec)?;
        Ok("Subtotals made".to_owned())
    })
}

// -------------------------------------------------------------------------------------------------- rules

/// The Manage Rules verb. `rules [range]` reads them back; `add`, `edit`, `delete`, `move`, `applies` and
/// `stop` change them.
fn rules(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    match args.first().map(String::as_str) {
        Some("add") | Some("edit") | Some("delete") | Some("move") | Some("applies")
        | Some("stop") => {}
        _ => {
            let within = args.first().and_then(|w| Range::parse(&w.replace('$', "")));
            let value = rules_json(book, within);
            return Ok(
                Answer::said(format!("{} rules", value.as_array().map_or(0, Vec::len))).with(value)
            );
        }
    }
    let word = args[0].clone();
    let rest = args[1..].to_vec();
    edit(sheet, move |book| rules_change(book, &word, &rest))
}

fn priority(rest: &[String]) -> Result<i32, String> {
    rest.first()
        .and_then(|w| w.parse().ok())
        .ok_or_else(|| "name the rule by its priority number".to_owned())
}

fn rules_change(book: &mut Book, word: &str, rest: &[String]) -> Result<String, String> {
    let index = book.view.sheet;
    match word {
        "add" => {
            let (range, tail) = chosen(book, rest);
            let mut ranges = book.view.extra.clone();
            ranges.push(range);
            let rule = rule_for(book, tail, range)?;
            book.session.add_conditional(index, ranges, rule)?;
            let top = book
                .session
                .conditional_rules(index, None)
                .into_iter()
                .map(|e| e.priority)
                .max()
                .unwrap_or(1);
            book.session.move_conditional_rule(index, top, RuleMove::First)?;
            Ok("Rule added".to_owned())
        }
        "edit" => {
            let p = priority(rest)?;
            let area = book
                .session
                .conditional_rules(index, None)
                .into_iter()
                .find(|e| e.priority == p)
                .and_then(|e| e.ranges.first().copied())
                .ok_or("There is no such rule.")?;
            let rule = rule_for(book, &rest[1..], area)?;
            book.session.edit_conditional_rule(index, p, rule)?;
            Ok(format!("Rule {p} changed"))
        }
        "delete" => {
            book.session.delete_conditional_rule(index, priority(rest)?)?;
            Ok("Rule deleted".to_owned())
        }
        "move" => {
            let movement = match rest.get(1).map(String::as_str) {
                Some("up") => RuleMove::Up,
                Some("down") => RuleMove::Down,
                Some("first") => RuleMove::First,
                Some("last") => RuleMove::Last,
                _ => return Err("move takes up, down, first or last".to_owned()),
            };
            book.session.move_conditional_rule(index, priority(rest)?, movement)?;
            Ok("Rule moved".to_owned())
        }
        "applies" => {
            let ranges =
                super::verbs_a::ranges_on(book, rest.get(1).ok_or("applies needs ranges")?)?;
            book.session.set_conditional_ranges(index, priority(rest)?, ranges)?;
            Ok("Rule range changed".to_owned())
        }
        _ => {
            book.session.set_conditional_stop(
                index,
                priority(rest)?,
                on_off(rest.get(1).map(String::as_str))?,
            )?;
            Ok("Stop if true set".to_owned())
        }
    }
}

/// A rule from words: icon sets, data bars with every option, dates, standard deviations, or any of the
/// older kinds `conditional` takes.
fn rule_for(book: &mut Book, words: &[String], area: Range) -> Result<CfRule, String> {
    match words.first().map(String::as_str) {
        Some("icons") => icon_rule(words),
        Some("bar") => bar_rule(words),
        Some("date") => {
            let period = words.get(1).ok_or("date needs a period such as lastWeek")?;
            let mut rule = CfRule::new("timePeriod");
            rule.time_period = Some(period.clone());
            rule.formulas =
                unluminous_sheet::conditional::time_period_formula(period, &area.first.a1())
                    .into_iter()
                    .collect();
            with_look(book, rule, words)
        }
        Some("stddev") => {
            let mut rule = CfRule::new("aboveAverage");
            rule.std_dev =
                Some(words.get(1).and_then(|w| w.parse().ok()).ok_or("stddev takes 1, 2 or 3")?);
            rule.above_average = Some(words.get(2).map(String::as_str) != Some("below"));
            with_look(book, rule, words)
        }
        _ => rule_from_words(words, &mut book.session.book.styles.dxfs),
    }
}

fn with_look(book: &mut Book, mut rule: CfRule, words: &[String]) -> Result<CfRule, String> {
    let mut sample = vec!["x".to_owned()];
    sample.extend(words.iter().skip(1).cloned());
    let mut dxf = unluminous_sheet::styles::Dxf::default();
    if let Some(c) = after(words, "fill").and_then(Color::parse_hex) {
        dxf.fill = Some(c);
    }
    if let Some(c) = after(words, "color").and_then(Color::parse_hex) {
        dxf.font_color = Some(c);
    }
    if dxf == unluminous_sheet::styles::Dxf::default() {
        dxf.fill = Some(Color::rgb(0xFF, 0xC7, 0xCE));
        dxf.font_color = Some(Color::rgb(0x9C, 0x00, 0x06));
    }
    book.session.book.styles.dxfs.push(dxf);
    rule.dxf = Some((book.session.book.styles.dxfs.len() - 1) as u32);
    Ok(rule)
}

fn cfvo(kind: &str, value: Option<&str>) -> Cfvo {
    Cfvo { kind: kind.to_owned(), value: value.map(str::to_owned) }
}

/// `icons <set> [reverse] [icon-only] [percent|num|percentile <v>...]`: the thresholds for every icon but the
/// lowest, which Excel always starts at 0 percent.
fn icon_rule(words: &[String]) -> Result<CfRule, String> {
    let name = words.get(1).ok_or("icons needs a set such as 3Arrows")?.clone();
    let count = icon_count(&name);
    let kind = words
        .iter()
        .find(|w| matches!(w.as_str(), "percent" | "num" | "percentile"))
        .cloned()
        .unwrap_or_else(|| "percent".to_owned());
    let numbers: Vec<&String> = words.iter().filter(|w| w.parse::<f64>().is_ok()).collect();
    let mut thresholds = vec![(cfvo("percent", Some("0")), true)];
    for i in 1..count {
        let value = match numbers.get(i - 1) {
            Some(v) => (*v).clone(),
            None => format!("{}", (100 * i) / count),
        };
        thresholds.push((cfvo(&kind, Some(&value)), true));
    }
    let mut rule = CfRule::new("iconSet");
    rule.icon_set = Some(IconSetRule {
        name,
        thresholds,
        reverse: flag(words, "reverse"),
        show_value: !flag(words, "icon-only"),
        custom: Vec::new(),
    });
    Ok(rule)
}

/// `bar <color> [border <color>] [solid] [negative <color>] [axis automatic|middle|none] [rtl] [no-value]`
fn bar_rule(words: &[String]) -> Result<CfRule, String> {
    let colour =
        words.get(1).and_then(|w| Color::parse_hex(w)).unwrap_or(Color::rgb(0x63, 0x8E, 0xC6));
    let mut rule = CfRule::new("dataBar");
    rule.data_bar = Some((cfvo("min", None), cfvo("max", None), colour));
    let mut options = DataBarExt::default();
    if let Some(c) = after(words, "border").and_then(Color::parse_hex) {
        options.border = true;
        options.border_color = Some(c);
    }
    options.gradient = !flag(words, "solid");
    if let Some(c) = after(words, "negative").and_then(Color::parse_hex) {
        options.negative_same_fill = false;
        options.negative_fill = Some(c);
    }
    if let Some(a) = after(words, "axis") {
        options.axis_position = a.to_owned();
    }
    if flag(words, "rtl") {
        options.direction = "rightToLeft".to_owned();
    }
    options.show_value = !flag(words, "no-value");
    rule.data_bar_options = Some(options);
    Ok(rule)
}

/// Extra words for `conditional`: `conditional icons <set>`, `bar` with options, `date`, `stddev`.
pub fn conditional_more(sheet: &mut Spreadsheet, args: &[String]) -> Option<Out> {
    let book = sheet.current_book().ok()?;
    let probe = super::commands::take_range(book, args);
    let more =
        matches!(probe.first().map(String::as_str), Some("icons") | Some("date") | Some("stddev"))
            || (probe.first().map(String::as_str) == Some("bar") && probe.len() > 2);
    if !more {
        return None;
    }
    let words = std::iter::once("add".to_owned()).chain(probe.iter().cloned()).collect::<Vec<_>>();
    Some(edit(sheet, move |book| rules_change(book, "add", &words[1..])))
}

// ----------------------------------------------------------------------------------------------- read back

/// Every rule, in the order they apply, with what it looks at and the look it applies.
pub fn rules_json(book: &Book, within: Option<Range>) -> Json {
    let index = book.view.sheet;
    let entries = book.session.conditional_rules(index, within);
    Json::Array(entries.iter().map(|e| rule_json(book, e)).collect())
}

fn rule_json(book: &Book, entry: &RuleEntry) -> Json {
    let rule = &entry.rule;
    let styles = &book.session.book.styles;
    let look = rule.dxf.and_then(|d| styles.dxfs.get(d as usize)).map(|d| {
        json!({ "fill": d.fill.map(|c| c.to_hex(&styles.theme)), "color": d.font_color.map(|c| c.to_hex(&styles.theme)),
            "bold": d.bold, "italic": d.italic })
    });
    json!({
        "priority": entry.priority,
        "ranges": entry.ranges.iter().map(Range::a1).collect::<Vec<_>>(),
        "kind": rule.kind, "operator": rule.operator, "text": rule.text, "formulas": rule.formulas,
        "stop": rule.stop, "rank": rule.rank, "percent": rule.percent, "bottom": rule.bottom,
        "look": look,
        "iconSet": rule.icon_set.as_ref().map(|i| json!({ "name": i.name, "reverse": i.reverse, "showValue": i.show_value })),
        "dataBar": rule.data_bar.as_ref().map(|(_, _, c)| c.to_hex(&styles.theme)),
        "colorScale": rule.color_scale.as_ref().map(|s| s.iter().map(|(_, c)| c.to_hex(&styles.theme)).collect::<Vec<_>>()),
    })
}
