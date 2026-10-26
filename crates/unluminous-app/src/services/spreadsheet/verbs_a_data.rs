//! The data tools as verbs: sort by levels, every kind of filter, Advanced Filter, Remove Duplicates,
//! Text to Columns, Consolidate and the table commands. Each one reaches the engine's `Session` method the
//! menu's entry reaches, so a person and an agent change the workbook the same way.

use serde_json::json;
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::review::special::current_region;
use unluminous_sheet::edit::{
    CellDisplay, ColumnFormat, ConsolidateFunction, Consolidation, DateOrder, Delimiters,
    DirectFormats, HeaderMode, SortLevel, SortOn, SortOptions, SplitMode, TableStyle,
    TextToColumns, TotalFunction,
};
use unluminous_sheet::model::{DateGroup, DateGrouping, FilterCriteria};
use unluminous_sheet::styles::{Color, Dxf};
use unluminous_sheet::Workbook;

use super::conditional::Rules;
use super::verbs_a::{after, chosen, column_of, edit, flag, keyword, Out};
use super::view_a::LevelDraft;
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// Run a data tools verb.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "sort-levels" => sort_levels(sheet, args),
        "sort-draft" => sort_draft(sheet, args),
        "filter-column" => filter_column(sheet, args),
        "filter-reapply" => filter_reapply(sheet),
        "filter-clear" => filter_clear(sheet),
        "advanced-filter" => advanced_filter(sheet, args),
        "text-to-columns" => text_to_columns(sheet, args),
        "consolidate" => consolidate(sheet, args),
        _ => return None,
    })
}

// ----------------------------------------------------------------------------------------------- display

/// What a cell shows with its conditional formats laid on, so a sort or a filter by colour or icon sees what
/// the grid draws.
pub struct RuleDisplay {
    rules: Rules,
    now: f64,
}

impl RuleDisplay {
    /// The rules of one sheet, prepared now.
    pub fn of(book: &Book, sheet: usize) -> RuleDisplay {
        let rules = Rules::prepare(&book.session.book, sheet, book.revision);
        RuleDisplay { rules, now: book.session.now }
    }
}

impl CellDisplay for RuleDisplay {
    fn fill(&self, book: &Workbook, sheet: usize, at: CellRef) -> Option<Color> {
        if !self.rules.is_empty() {
            if let Some((r, g, b)) = self.rules.effect(book, self.now, at).fill {
                return Some(Color::rgb(r, g, b));
            }
        }
        DirectFormats.fill(book, sheet, at)
    }

    fn font_color(&self, book: &Workbook, sheet: usize, at: CellRef) -> Option<Color> {
        if !self.rules.is_empty() {
            if let Some((r, g, b)) = self.rules.effect(book, self.now, at).color {
                return Some(Color::rgb(r, g, b));
            }
        }
        DirectFormats.font_color(book, sheet, at)
    }

    fn icon(&self, book: &Workbook, _sheet: usize, at: CellRef) -> Option<(String, u32)> {
        if self.rules.is_empty() {
            return None;
        }
        let pick = self.rules.effect(book, self.now, at).icon?;
        (!pick.hidden).then_some((pick.set, pick.index as u32))
    }
}

// ---------------------------------------------------------------------------------------------- sorting

/// A colour word: `#RRGGBB`, or `none` for the cells with no colour.
fn colour_word(word: &str) -> Result<Option<Color>, String> {
    if word.eq_ignore_ascii_case("none") || word.eq_ignore_ascii_case("automatic") {
        return Ok(None);
    }
    Color::parse_hex(word)
        .map(Some)
        .ok_or_else(|| format!("`{word}` is not a colour such as #FF0000"))
}

/// An icon word, `3Arrows/1` with the lowest icon 1, or `none`, as the set and the index from 0.
pub fn icon_word(word: &str) -> Result<Option<(String, u32)>, String> {
    if word.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    let (set, n) = word.rsplit_once('/').ok_or("an icon is written Set/N, such as 3Arrows/1")?;
    let n: u32 = n.parse().map_err(|_| format!("`{n}` is not an icon number"))?;
    if n == 0 {
        return Err("icons are counted from 1, the lowest".to_owned());
    }
    Ok(Some((set.to_owned(), n - 1)))
}

/// One level written `B`, `B:desc`, `B:asc:cell-color=#FF0000`, `C:font-color=none`, `D:icon=3Arrows/1`
/// or `E:list=Low|Medium|High`. A column is a letter, or a row number when sorting left to right.
pub fn parse_level(text: &str, left_to_right: bool) -> Result<LevelDraft, String> {
    let mut parts = text.split(':');
    let key = parts.next().unwrap_or_default();
    let key = if left_to_right {
        key.parse::<u32>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .ok_or_else(|| format!("`{key}` is not a row"))?
    } else {
        column_of(key)?
    };
    let mut level = LevelDraft::values(key);
    for part in parts {
        match part.split_once('=') {
            None if part.eq_ignore_ascii_case("desc")
                || part.eq_ignore_ascii_case("descending") =>
            {
                level.descending = true
            }
            None if part.eq_ignore_ascii_case("asc") || part.eq_ignore_ascii_case("ascending") => {
                level.descending = false
            }
            None if part.eq_ignore_ascii_case("values") => {}
            Some(("cell-color", v)) | Some(("cell-colour", v)) => {
                set_on(&mut level, "cell-color", v)
            }
            Some(("font-color", v)) | Some(("font-colour", v)) => {
                set_on(&mut level, "font-color", v)
            }
            Some(("icon", v)) => set_on(&mut level, "icon", v),
            Some(("list", v)) => level.list = v.to_owned(),
            _ => return Err(format!("`{part}` is not part of a sort level")),
        }
    }
    Ok(level)
}

fn set_on(level: &mut LevelDraft, on: &str, with: &str) {
    level.on = on.to_owned();
    level.with = with.to_owned();
}

/// The engine's level for a drafted one.
pub fn engine_level(level: &LevelDraft) -> Result<SortLevel, String> {
    let on = match level.on.as_str() {
        "cell-color" => SortOn::CellColor(colour_word(&level.with)?),
        "font-color" => SortOn::FontColor(colour_word(&level.with)?),
        "icon" => SortOn::Icon(icon_word(&level.with)?),
        _ => SortOn::Values,
    };
    let custom_list = (!level.list.is_empty())
        .then(|| level.list.split('|').map(|s| s.trim().to_owned()).collect::<Vec<_>>());
    Ok(SortLevel { key: level.key, on, descending: level.descending, custom_list })
}

/// Sort the range by the drafted levels with the drafted options.
fn sort_with_levels(
    book: &mut Book,
    range: Range,
    levels: &[LevelDraft],
    header: &str,
    case_sensitive: bool,
    left_to_right: bool,
) -> Result<String, String> {
    let engine: Vec<SortLevel> = levels.iter().map(engine_level).collect::<Result<_, _>>()?;
    let options = SortOptions {
        header: match header {
            "header" => HeaderMode::Yes,
            "noheader" => HeaderMode::No,
            _ => HeaderMode::Guess,
        },
        case_sensitive,
        left_to_right,
        remember: true,
    };
    let sheet = book.view.sheet;
    let display = RuleDisplay::of(book, sheet);
    book.session.sort_with(sheet, range, &engine, &options, &display)?;
    Ok(format!("Sorted {} by {} levels", range.a1(), levels.len()))
}

/// `sort-levels [range] [header|noheader] [case] [left-to-right] <level>...`
fn sort_levels(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    edit(sheet, |book| {
        let (range, rest) = chosen(book, args);
        let left = flag(rest, "left-to-right");
        let levels: Vec<LevelDraft> = rest
            .iter()
            .filter(|w| !matches!(w.as_str(), "header" | "noheader" | "case" | "left-to-right"))
            .map(|w| parse_level(w, left))
            .collect::<Result<_, _>>()?;
        if levels.is_empty() {
            return Err("sort-levels needs a level such as B:desc".to_owned());
        }
        let header = if flag(rest, "header") {
            "header"
        } else if flag(rest, "noheader") {
            "noheader"
        } else {
            "guess"
        };
        sort_with_levels(book, range, &levels, header, flag(rest, "case"), left)
    })
}

/// The Sort popup: `sort-draft` shows it, and `add`, `delete`, `copy`, `up`, `down`, `set`, `options`,
/// `reset` and `apply` change what is drafted. `apply` sorts the selection.
fn sort_draft(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let verb = args.first().map(String::as_str).unwrap_or("show");
    let rest = if args.is_empty() { args } else { &args[1..] };
    if verb == "apply" {
        return apply_draft(sheet);
    }
    let first_column = book.view.range().first.col;
    let draft = &mut book.view.audit.sort;
    if draft.levels.is_empty() && verb != "reset" {
        draft.levels.push(LevelDraft::values(first_column));
    }
    let number = |i: usize| -> Result<usize, String> {
        let n: usize = rest
            .get(i)
            .and_then(|w| w.parse().ok())
            .ok_or("name the level by its number from 1")?;
        (1..=draft.levels.len())
            .contains(&n)
            .then_some(n - 1)
            .ok_or_else(|| format!("there is no level {n}"))
    };
    match verb {
        "show" | "open" => draft.open = true,
        "close" => draft.open = false,
        "reset" => {
            *draft = Default::default();
            draft.levels.push(LevelDraft::values(first_column));
        }
        "add" => {
            let key = rest.first().map(|w| column_of(w)).transpose()?.unwrap_or(first_column);
            if draft.levels.len() >= 64 {
                return Err("A sort has at most 64 levels.".to_owned());
            }
            draft.levels.push(LevelDraft::values(key));
        }
        "delete" => {
            let n = number(0)?;
            draft.levels.remove(n);
        }
        "copy" => {
            let n = number(0)?;
            let copy = draft.levels[n].clone();
            draft.levels.insert(n + 1, copy);
        }
        "up" => {
            let n = number(0)?;
            if n > 0 {
                draft.levels.swap(n, n - 1);
            }
        }
        "down" => {
            let n = number(0)?;
            if n + 1 < draft.levels.len() {
                draft.levels.swap(n, n + 1);
            }
        }
        "set" => {
            let n = number(0)?;
            set_level_fields(&mut draft.levels[n], &rest[1..])?;
        }
        "options" => set_draft_options(draft, rest)?,
        other => return Err(format!("sort-draft takes show, add, delete, copy, up, down, set, options, reset or apply, not `{other}`")),
    }
    Ok(Answer::said(format!("{} sort levels", draft.levels.len())).with(draft_json(draft)))
}

/// `set <n> column B on cell-color with #FF0000 order desc list a|b`
fn set_level_fields(level: &mut LevelDraft, words: &[String]) -> Result<(), String> {
    let mut i = 0;
    while i < words.len() {
        let value = words.get(i + 1).cloned().unwrap_or_default();
        match words[i].as_str() {
            "column" | "row" => {
                level.key = column_of(&value).or_else(|_| {
                    value
                        .parse::<u32>()
                        .map(|n| n.saturating_sub(1))
                        .map_err(|_| format!("`{value}` is not a column"))
                })?
            }
            "on" => level.on = value,
            "with" => level.with = value,
            "order" => level.descending = value.starts_with("desc"),
            "list" => level.list = value,
            other => {
                return Err(format!("a level has column, on, with, order and list, not `{other}`"))
            }
        }
        i += 2;
    }
    Ok(())
}

fn set_draft_options(draft: &mut super::view_a::SortDraft, words: &[String]) -> Result<(), String> {
    for word in words {
        match word.as_str() {
            "header" | "noheader" | "guess" => draft.header = word.clone(),
            "case" => draft.case_sensitive = true,
            "no-case" => draft.case_sensitive = false,
            "left-to-right" => draft.left_to_right = true,
            "top-to-bottom" => draft.left_to_right = false,
            other => {
                return Err(format!(
                    "sort options are header, noheader, case, left-to-right, not `{other}`"
                ))
            }
        }
    }
    Ok(())
}

fn draft_json(draft: &super::view_a::SortDraft) -> serde_json::Value {
    let levels: Vec<_> = draft
        .levels
        .iter()
        .map(|l| {
            json!({
                "column": unluminous_sheet::coord::column_name(l.key),
                "on": l.on, "with": l.with,
                "order": if l.descending { "descending" } else { "ascending" }, "list": l.list,
            })
        })
        .collect();
    json!({ "levels": levels, "header": if draft.header.is_empty() { "guess" } else { &draft.header },
        "case": draft.case_sensitive, "leftToRight": draft.left_to_right })
}

fn apply_draft(sheet: &mut Spreadsheet) -> Out {
    edit(sheet, |book| {
        let range = book.view.range();
        let draft = book.view.audit.sort.clone();
        let header = if draft.header.is_empty() { "guess" } else { &draft.header };
        let message = sort_with_levels(
            book,
            range,
            &draft.levels,
            header,
            draft.case_sensitive,
            draft.left_to_right,
        )?;
        book.view.audit.sort.open = false;
        Ok(message)
    })
}

// --------------------------------------------------------------------------------------------- filtering

/// The comparison a word names, as the file writes it.
fn operator(word: &str) -> Option<&'static str> {
    Some(match word {
        "=" | "equals" | "equal" => "equal",
        "<>" | "not-equal" | "not-equals" => "notEqual",
        ">" | "greater" => "greaterThan",
        ">=" => "greaterThanOrEqual",
        "<" | "less" => "lessThan",
        "<=" => "lessThanOrEqual",
        _ => return None,
    })
}

/// A text filter: the condition Excel writes for `begins with` and the others, with wildcards.
fn text_condition(kind: &str, text: &str) -> Result<(String, String), String> {
    let (op, value) = match kind {
        "equals" => ("equal", text.to_owned()),
        "not-equals" => ("notEqual", text.to_owned()),
        "begins" => ("equal", format!("{text}*")),
        "not-begins" => ("notEqual", format!("{text}*")),
        "ends" => ("equal", format!("*{text}")),
        "not-ends" => ("notEqual", format!("*{text}")),
        "contains" => ("equal", format!("*{text}*")),
        "not-contains" => ("notEqual", format!("*{text}*")),
        other => {
            return Err(format!(
            "a text filter is equals, begins, ends, contains or their not- forms, not `{other}`"
        ))
        }
    };
    Ok((op.to_owned(), value))
}

/// A date filter word: a year `2024`, a month `2024-03`, a day `2024-03-05`, or a dynamic period.
fn date_criteria(words: &[String]) -> Result<FilterCriteria, String> {
    let mut groups = Vec::new();
    let mut kind = None;
    for w in words {
        let parts: Vec<&str> = w.split('-').collect();
        let numbers: Option<Vec<i32>> = parts.iter().map(|p| p.parse().ok()).collect();
        match numbers {
            Some(n) if (1..=3).contains(&n.len()) => groups.push(DateGroup {
                year: n[0],
                month: n.get(1).map(|m| *m as u32),
                day: n.get(2).map(|d| *d as u32),
                hour: None,
                minute: None,
                second: None,
                grouping: [DateGrouping::Year, DateGrouping::Month, DateGrouping::Day][n.len() - 1],
            }),
            _ => kind = Some(w.clone()),
        }
    }
    if let Some(kind) = kind {
        return Ok(FilterCriteria::Dynamic { kind, value: None, max: None });
    }
    Ok(FilterCriteria::Dates { values: Vec::new(), groups, blank: false })
}

/// A colour filter, which needs a differential format holding the colour.
fn colour_criteria(book: &mut Book, cell: bool, word: &str) -> Result<FilterCriteria, String> {
    let colour = colour_word(word)?;
    let dxf = match (cell, colour) {
        (true, c) => Dxf { fill: c, ..Dxf::default() },
        (false, c) => Dxf { font_color: c, ..Dxf::default() },
    };
    let id = book.session.add_differential_format(dxf);
    Ok(FilterCriteria::Color { cell, dxf: id })
}

/// The criteria the words after the column name describe, or `None` for `clear`.
fn criteria_from(book: &mut Book, words: &[String]) -> Result<Option<FilterCriteria>, String> {
    let kind = words.first().map(String::as_str).ok_or("filter-column needs a kind of filter")?;
    let rest = &words[1..];
    let value = |i: usize| rest.get(i).cloned().ok_or_else(|| format!("`{kind}` needs a value"));
    Ok(Some(match kind {
        "clear" | "all" => return Ok(None),
        "values" => {
            let list: Vec<String> =
                rest.join(" ").split(',').map(|v| v.trim().to_owned()).collect();
            let blank = list.iter().any(|v| v == "(blank)");
            FilterCriteria::Values {
                values: list.into_iter().filter(|v| v != "(blank)").collect(),
                blank,
            }
        }
        "equals" | "not-equals" | "begins" | "not-begins" | "ends" | "not-ends" | "contains"
        | "not-contains" => FilterCriteria::Custom {
            conditions: vec![text_condition(kind, &value(0)?)?],
            and: true,
        },
        "number" => number_criteria(rest)?,
        "between" | "not-between" => {
            let (a, b) = (value(0)?, value(1)?);
            if kind == "between" {
                FilterCriteria::Custom {
                    conditions: vec![
                        ("greaterThanOrEqual".into(), a),
                        ("lessThanOrEqual".into(), b),
                    ],
                    and: true,
                }
            } else {
                FilterCriteria::Custom {
                    conditions: vec![("lessThan".into(), a), ("greaterThan".into(), b)],
                    and: false,
                }
            }
        }
        "top" | "bottom" => {
            let n: f64 = value(0)?
                .trim_end_matches('%')
                .parse()
                .map_err(|_| "top and bottom take a count".to_owned())?;
            let percent = rest.iter().any(|w| w == "percent" || w.ends_with('%'));
            FilterCriteria::Top10 { top: kind == "top", percent, value: n, threshold: None }
        }
        "above-average" => {
            FilterCriteria::Dynamic { kind: "aboveAverage".into(), value: None, max: None }
        }
        "below-average" => {
            FilterCriteria::Dynamic { kind: "belowAverage".into(), value: None, max: None }
        }
        "dates" => date_criteria(rest)?,
        "color" | "colour" => match value(0)?.as_str() {
            "cell" => colour_criteria(book, true, &value(1)?)?,
            "font" => colour_criteria(book, false, &value(1)?)?,
            _ => return Err("filter by color takes cell or font, then the colour".to_owned()),
        },
        "icon" => {
            let (set, id) = match icon_word(&value(0)?)? {
                Some((set, n)) => (set, n as i32),
                None => (String::new(), -1),
            };
            FilterCriteria::Icon { set, id }
        }
        other => return Err(format!("`{other}` is not a kind of filter")),
    }))
}

/// `number > 10 [and|or <= 20]` or `number 10` for equals.
fn number_criteria(words: &[String]) -> Result<FilterCriteria, String> {
    let mut conditions = Vec::new();
    let mut and = true;
    let mut i = 0;
    while i < words.len() {
        match words[i].as_str() {
            "and" => and = true,
            "or" => and = false,
            w => {
                let (op, value) = match operator(w) {
                    Some(op) => {
                        (op, words.get(i + 1).cloned().ok_or("a comparison needs a number")?)
                    }
                    None => ("equal", w.to_owned()),
                };
                conditions.push((op.to_owned(), value));
                i += usize::from(operator(w).is_some());
            }
        }
        i += 1;
    }
    if conditions.is_empty() || conditions.len() > 2 {
        return Err("a number filter has one or two comparisons".to_owned());
    }
    Ok(FilterCriteria::Custom { conditions, and })
}

/// Make sure the sheet has a filter, over the region around the active cell when it has none.
fn ensure_filter(book: &mut Book) -> Result<(), String> {
    let sheet = book.view.sheet;
    if book.session.book.sheets[sheet].filter.is_some() {
        return Ok(());
    }
    let region = current_region(&book.session.book.sheets[sheet], book.view.cursor);
    book.session.set_filter(sheet, Some(region))
}

/// `filter-column <column> <kind> ...` sets one column's criteria and applies the filter.
fn filter_column(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    edit(sheet, |book| {
        let col =
            column_of(args.first().ok_or("filter-column needs a column and a kind of filter")?)?;
        let criteria = criteria_from(book, &args[1..])?;
        ensure_filter(book)?;
        let index = book.view.sheet;
        let display = RuleDisplay::of(book, index);
        book.session.filter_column_with(index, col, criteria, &display)?;
        Ok(format!("Filtered column {}", unluminous_sheet::coord::column_name(col)))
    })
}

/// Reapply: filter again, since the cells may have changed.
fn filter_reapply(sheet: &mut Spreadsheet) -> Out {
    edit(sheet, |book| {
        let index = book.view.sheet;
        let display = RuleDisplay::of(book, index);
        book.session.apply_filter_with(index, &display);
        Ok("Filter applied again".to_owned())
    })
}

/// Clear: show every row and drop the criteria, leaving the filter buttons.
fn filter_clear(sheet: &mut Spreadsheet) -> Out {
    edit(sheet, |book| {
        let index = book.view.sheet;
        book.session.clear_filter(index)?;
        Ok("Filter cleared".to_owned())
    })
}

/// `advanced-filter <list> [criteria <range>] [copy-to <cell>] [unique]`, or `advanced-filter show-all`.
fn advanced_filter(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    edit(sheet, |book| {
        let index = book.view.sheet;
        if args.first().map(String::as_str) == Some("show-all") {
            book.session.show_all_rows(index)?;
            return Ok("Every row is showing".to_owned());
        }
        let (list, rest) = chosen(book, args);
        let criteria =
            after(rest, "criteria").map(|w| super::verbs_a::ranges_on(book, w)).transpose()?;
        let copy_to = match after(rest, "copy-to") {
            Some(w) => {
                let first = super::verbs_a::ranges_on(book, w)?[0];
                Some(first)
            }
            None => None,
        };
        let criteria = criteria.map(|r| r[0]);
        book.session.advanced_filter(index, list, criteria, copy_to, flag(rest, "unique"))?;
        Ok("Advanced filter applied".to_owned())
    })
}

// ----------------------------------------------------------------------------------- duplicates, split

/// `remove-duplicates [range] [columns A,C] [header|noheader]`; the old words alone are left to the old verb.
pub fn remove_duplicates_more(sheet: &mut Spreadsheet, args: &[String]) -> Option<Out> {
    if !(flag(args, "columns") || flag(args, "header") || flag(args, "noheader")) {
        return None;
    }
    Some(edit(sheet, |book| {
        let (range, rest) = chosen(book, args);
        let columns: Vec<u32> = match after(rest, "columns") {
            Some(list) => list
                .split(',')
                .map(|c| column_of(c).map(|n| n.saturating_sub(range.first.col)))
                .collect::<Result<_, _>>()?,
            None => (0..range.cols()).collect(),
        };
        let mode = if flag(rest, "header") {
            HeaderMode::Yes
        } else if flag(rest, "noheader") {
            HeaderMode::No
        } else {
            HeaderMode::Guess
        };
        let index = book.view.sheet;
        let removed = book.session.remove_duplicates_with(index, range, &columns, mode)?;
        Ok(format!("{removed} duplicate rows removed"))
    }))
}

/// A column data format word: `general`, `text`, `skip`, or `date:mdy`.
fn column_format(word: &str) -> Result<ColumnFormat, String> {
    Ok(match word {
        "general" => ColumnFormat::General,
        "text" => ColumnFormat::Text,
        "skip" => ColumnFormat::Skip,
        w if w.starts_with("date") => {
            ColumnFormat::Date(match w.split(':').nth(1).unwrap_or("mdy") {
                "mdy" => DateOrder::Mdy,
                "dmy" => DateOrder::Dmy,
                "ymd" => DateOrder::Ymd,
                "myd" => DateOrder::Myd,
                "dym" => DateOrder::Dym,
                "ydm" => DateOrder::Ydm,
                o => return Err(format!("`{o}` is not a date order such as mdy")),
            })
        }
        other => {
            return Err(format!(
                "`{other}` is not a column format: general, text, skip or date:mdy"
            ))
        }
    })
}

/// The delimiters the words name: `tab`, `semicolon`, `comma`, `space`, or other characters in quotes-free form.
fn delimiters_from(words: &[String]) -> Delimiters {
    let mut d = Delimiters::default();
    for w in words {
        match w.as_str() {
            "tab" => d.tab = true,
            "semicolon" => d.semicolon = true,
            "comma" => d.comma = true,
            "space" => d.space = true,
            "consecutive" => d.consecutive_as_one = true,
            _ => {}
        }
    }
    if let Some(other) = after(words, "other") {
        d.other = other.to_owned();
    }
    d.qualifier = match after(words, "qualifier") {
        Some("none") => None,
        Some(q) => q.chars().next(),
        None => Some('"'),
    };
    d
}

/// `text-to-columns [range] fixed 3,8 | delimited comma semicolon ... [qualifier "] [consecutive]
/// [formats text,date:dmy,skip] [to <cell>]`
fn text_to_columns(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    edit(sheet, |book| {
        let (range, rest) = chosen(book, args);
        let (mode, rest) = keyword(rest, &["fixed", "delimited"])
            .ok_or("text-to-columns takes fixed or delimited")?;
        let split = if mode == "fixed" {
            let breaks =
                rest.first().ok_or("fixed needs the positions of the breaks, such as 3,8,12")?;
            let at: Vec<usize> = breaks
                .split(',')
                .map(|b| b.trim().parse().map_err(|_| format!("`{b}` is not a position")))
                .collect::<Result<_, _>>()?;
            SplitMode::FixedWidth(at)
        } else {
            SplitMode::Delimited(delimiters_from(rest))
        };
        let formats = match after(rest, "formats") {
            Some(list) => list.split(',').map(column_format).collect::<Result<_, _>>()?,
            None => Vec::new(),
        };
        let destination =
            after(rest, "to").map(|w| super::verbs_a::cell_or_active(book, Some(w))).transpose()?;
        let request = TextToColumns { mode: split, formats, destination };
        let index = book.view.sheet;
        book.session.text_to_columns_with(index, range, &request)?;
        Ok("Text split into columns".to_owned())
    })
}

// -------------------------------------------------------------------------------------------- consolidate

fn consolidate_function(word: &str) -> Result<ConsolidateFunction, String> {
    Ok(match word {
        "sum" => ConsolidateFunction::Sum,
        "count" => ConsolidateFunction::Count,
        "average" => ConsolidateFunction::Average,
        "max" => ConsolidateFunction::Max,
        "min" => ConsolidateFunction::Min,
        "product" => ConsolidateFunction::Product,
        "count-numbers" => ConsolidateFunction::CountNumbers,
        "stddev" => ConsolidateFunction::StdDev,
        "stddevp" => ConsolidateFunction::StdDevP,
        "var" => ConsolidateFunction::Var,
        "varp" => ConsolidateFunction::VarP,
        other => return Err(format!("`{other}` is not a consolidation function")),
    })
}

/// `consolidate <destination> <function> <source>... [top-row] [left-column] [links] [book <name>]`
fn consolidate(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    edit(sheet, |book| {
        let destination = super::verbs_a::cell_or_active(book, args.first().map(String::as_str))?;
        let function = consolidate_function(args.get(1).ok_or("consolidate needs a function")?)?;
        let mut sources = Vec::new();
        for w in &args[2..] {
            if matches!(w.as_str(), "top-row" | "left-column" | "links") {
                continue;
            }
            if w == "book" {
                break;
            }
            if let Some(found) = super::commands::parse_range(book, w) {
                sources.push(found);
            }
        }
        if sources.is_empty() {
            return Err("consolidate needs at least one source range".to_owned());
        }
        let request = Consolidation {
            destination,
            sources,
            function,
            top_row: flag(args, "top-row"),
            left_column: flag(args, "left-column"),
            create_links: flag(args, "links"),
            book_name: after(args, "book").unwrap_or("Book1").to_owned(),
        };
        let index = book.view.sheet;
        book.session.consolidate(index, &request)?;
        Ok("Consolidated".to_owned())
    })
}

// ------------------------------------------------------------------------------------------------ tables

/// The table a word names, or the one the active cell is in.
fn table_named(book: &Book, rest: &[String]) -> Result<(String, usize), String> {
    let tables = &book.session.book.tables;
    if let Some(w) = rest.first() {
        if let Some(t) = tables.iter().find(|t| t.name.eq_ignore_ascii_case(w)) {
            return Ok((t.name.clone(), 1));
        }
    }
    let cursor = book.view.cursor;
    tables
        .iter()
        .find(|t| t.sheet == book.view.sheet && t.range.contains(cursor))
        .map(|t| (t.name.clone(), 0))
        .ok_or_else(|| "There is no table here. Name one, or choose a cell in it.".to_owned())
}

fn total_function(word: &str) -> Result<TotalFunction, String> {
    TotalFunction::parse(word)
        .or(match word {
            "count-numbers" => Some(TotalFunction::CountNumbers),
            "stddev" => Some(TotalFunction::StdDev),
            _ => None,
        })
        .ok_or_else(|| format!("`{word}` is not a totals function: none, sum, average, count, count-numbers, max, min, stddev, var"))
}

fn set_flag(args: &[String], word: &str, now: bool) -> Result<bool, String> {
    match after(args, word) {
        Some(w) => super::verbs_a::on_off(Some(w)),
        None => Ok(now),
    }
}

/// The table commands round 2 added: `table resize|style|totals|total|header|to-range|rename|column|row|list`.
pub fn table_more(sheet: &mut Spreadsheet, args: &[String]) -> Option<Out> {
    const WORDS: &[&str] = &[
        "resize",
        "style",
        "totals",
        "total",
        "header",
        "to-range",
        "rename",
        "column",
        "row",
        "list",
        "auto-fill",
    ];
    let (word, rest) = keyword(args, WORDS)?;
    if word == "list" {
        return Some(sheet.current_book().map(|book| {
            let list: Vec<_> = book.session.book.tables.iter().map(|t| json!({ "name": t.name, "sheet": t.sheet, "range": t.range.a1(), "columns": t.columns, "header": t.header_row, "totals": t.totals_row })).collect();
            Answer::said(format!("{} tables", list.len())).with(json!(list))
        }));
    }
    let word = word.to_owned();
    let rest = rest.to_vec();
    Some(edit(sheet, move |book| table_edit(book, &word, &rest)))
}

fn table_edit(book: &mut Book, word: &str, rest: &[String]) -> Result<String, String> {
    if word == "auto-fill" {
        book.session.table_auto_fill = super::verbs_a::on_off(rest.first().map(String::as_str))?;
        return Ok("Calculated columns fill automatically".to_owned());
    }
    let (name, used) = table_named(book, rest)?;
    let rest = &rest[used..];
    let session = &mut book.session;
    match word {
        "resize" => {
            let range = Range::parse(rest.first().ok_or("resize needs the new range")?)
                .ok_or("that is not a range")?;
            session.resize_table(&name, range)?;
        }
        "style" => {
            let mut style: TableStyle = session.table_style(&name)?;
            if let Some(s) = after(rest, "style") {
                style.name = (s != "none").then(|| s.to_owned());
            }
            style.row_stripes = set_flag(rest, "row-stripes", style.row_stripes)?;
            style.column_stripes = set_flag(rest, "column-stripes", style.column_stripes)?;
            style.first_column = set_flag(rest, "first-column", style.first_column)?;
            style.last_column = set_flag(rest, "last-column", style.last_column)?;
            style.filter_buttons = set_flag(rest, "filter-buttons", style.filter_buttons)?;
            session.set_table_style(&name, &style)?;
        }
        "totals" => session.set_table_totals_row(
            &name,
            super::verbs_a::on_off(rest.first().map(String::as_str))?,
        )?,
        "header" => session.set_table_header_row(
            &name,
            super::verbs_a::on_off(rest.first().map(String::as_str))?,
        )?,
        "total" => {
            let column = table_column(session, &name, rest.first())?;
            let function = total_function(rest.get(1).ok_or("total needs a function")?)?;
            session.set_table_total_function(&name, column, function)?;
        }
        "to-range" => session.table_to_range(&name)?,
        "rename" => {
            session.rename_table(&name, rest.first().ok_or("rename needs the new name")?)?
        }
        "column" => table_column_edit(session, &name, rest)?,
        "row" => match rest.first().map(String::as_str) {
            Some("add") | None => session.add_table_row(&name)?,
            Some("delete") => {
                let first: u32 = rest
                    .get(1)
                    .and_then(|w| w.parse().ok())
                    .ok_or("delete needs the first row from 1")?;
                let count: u32 = rest.get(2).and_then(|w| w.parse().ok()).unwrap_or(1);
                session.delete_table_rows(&name, first, count)?;
            }
            Some(other) => return Err(format!("table row takes add or delete, not `{other}`")),
        },
        _ => unreachable!("table_more only passes the words it listed"),
    }
    Ok(format!("Table {name} changed"))
}

/// A column of a table, by number from 1 or by its name.
fn table_column(
    session: &unluminous_sheet::edit::Session,
    table: &str,
    word: Option<&String>,
) -> Result<usize, String> {
    let word = word.ok_or("name the column by its number from 1 or its name")?;
    let t =
        session.book.tables.iter().find(|t| t.name == table).ok_or("There is no such table.")?;
    if let Ok(n) = word.parse::<usize>() {
        return n
            .checked_sub(1)
            .filter(|i| *i < t.columns.len())
            .ok_or_else(|| format!("there is no column {n}"));
    }
    t.columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case(word))
        .ok_or_else(|| format!("there is no column named {word}"))
}

fn table_column_edit(
    session: &mut unluminous_sheet::edit::Session,
    name: &str,
    rest: &[String],
) -> Result<(), String> {
    match rest.first().map(String::as_str) {
        Some("add") => session.add_table_column(name, rest.get(1).map(String::as_str)),
        Some("rename") => {
            let column = table_column(session, name, rest.get(1))?;
            session.rename_table_column(
                name,
                column,
                rest.get(2).ok_or("rename needs the new name")?,
            )
        }
        _ => Err("table column takes add or rename".to_owned()),
    }
}
