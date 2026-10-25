//! Perform a scenario step with the engine.

use serde_json::Value as Json;
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::{
    ColumnFormat, ConsolidateFunction, Consolidation, DateOrder, Delimiters, HeaderMode, Session,
    SortLevel, SortOn, SortOptions, SplitMode, TableOptions, TextToColumns, TotalFunction,
};
use unluminous_sheet::styles::Color;

use super::{range_of, step_sheet, typed, TrafficLights};

/// What the steps of a scenario share.
#[derive(Default)]
pub struct Context {
    pub lights: Option<TrafficLights>,
}

fn rgb(hex: &str) -> Color {
    Color::Rgb(0xFF00_0000 | u32::from_str_radix(hex, 16).unwrap())
}

fn header_mode(text: &str) -> HeaderMode {
    match text {
        "yes" => HeaderMode::Yes,
        "no" => HeaderMode::No,
        _ => HeaderMode::Guess,
    }
}

pub fn run_step(s: &mut Session, step: &Json, ctx: &mut Context) -> Result<(), String> {
    let sheet = step_sheet(s, step);
    match step["kind"].as_str().unwrap() {
        "set" => {
            let inputs: Vec<(CellRef, String)> = step["cells"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(a, v)| (CellRef::parse(a).unwrap(), typed(v)))
                .collect();
            s.set_inputs(sheet, &inputs)
        }
        "format" => super::apply_format(s, step),
        "note" => s.set_note(
            sheet,
            CellRef::parse(step["cell"].as_str().unwrap()).unwrap(),
            Some(step["text"].as_str().unwrap()),
            "Author",
        ),
        "hide-rows" => {
            let r = range_of(step["range"].as_str().unwrap());
            s.hide_rows(sheet, r.first.row, r.last.row, true)
        }
        "table" => {
            let range = range_of(step["range"].as_str().unwrap());
            let options = TableOptions {
                headers: step["headers"].as_bool().unwrap_or(true),
                style: step["style"].as_str().map(str::to_owned),
            };
            s.add_table_with(sheet, range, step["name"].as_str(), &options).map(|_| ())
        }
        "table-header" => {
            s.set_table_header_row(step["table"].as_str().unwrap(), step["show"].as_bool().unwrap())
        }
        "table-totals" => {
            s.set_table_totals_row(step["table"].as_str().unwrap(), step["show"].as_bool().unwrap())
        }
        "table-total-fn" => {
            let function = match step["function"].as_str().unwrap() {
                "sum" => TotalFunction::Sum,
                "average" => TotalFunction::Average,
                "count" => TotalFunction::Count,
                "countnums" => TotalFunction::CountNumbers,
                "max" => TotalFunction::Max,
                "min" => TotalFunction::Min,
                "stddev" => TotalFunction::StdDev,
                "var" => TotalFunction::Var,
                _ => TotalFunction::None,
            };
            let column = step["column"].as_u64().unwrap() as usize - 1;
            s.set_table_total_function(step["table"].as_str().unwrap(), column, function)
        }
        "table-style" => {
            let name = step["table"].as_str().unwrap();
            let mut style = s.table_style(name)?;
            if let Some(n) = step["style"].as_str() {
                style.name = Some(n.to_owned());
            }
            if let Some(b) = step["rowStripes"].as_bool() {
                style.row_stripes = b;
            }
            if let Some(b) = step["columnStripes"].as_bool() {
                style.column_stripes = b;
            }
            if let Some(b) = step["firstColumn"].as_bool() {
                style.first_column = b;
            }
            if let Some(b) = step["lastColumn"].as_bool() {
                style.last_column = b;
            }
            if let Some(b) = step["filterButton"].as_bool() {
                style.filter_buttons = b;
            }
            s.set_table_style(name, &style)
        }
        "table-style-name" => {
            let name = step["table"].as_str().unwrap();
            let mut style = s.table_style(name)?;
            style.name = step["style"].as_str().filter(|n| !n.is_empty()).map(str::to_owned);
            s.set_table_style(name, &style)
        }
        "table-rename" => {
            s.rename_table(step["table"].as_str().unwrap(), step["name"].as_str().unwrap())
        }
        "table-column-name" => s.rename_table_column(
            step["table"].as_str().unwrap(),
            step["column"].as_u64().unwrap() as usize - 1,
            step["name"].as_str().unwrap(),
        ),
        "table-add-column" => {
            s.add_table_column(step["table"].as_str().unwrap(), step["name"].as_str())
        }
        "delete-table-rows" => s.delete_table_rows(
            step["table"].as_str().unwrap(),
            step["row"].as_u64().unwrap() as u32,
            1,
        ),
        "add-table-row" => s.add_table_row(step["table"].as_str().unwrap()),
        "table-resize" => s.resize_table(
            step["table"].as_str().unwrap(),
            range_of(step["range"].as_str().unwrap()),
        ),
        "table-unlist" => s.table_to_range(step["table"].as_str().unwrap()),
        "type-in-table" => {
            s.table_auto_fill = step["autofill"].as_bool().unwrap_or(true);
            let result = s.set_input(
                sheet,
                CellRef::parse(step["cell"].as_str().unwrap()).unwrap(),
                &typed(&step["formula"]),
            );
            s.table_auto_fill = true;
            result
        }
        "filter-on" => s.set_filter(sheet, Some(range_of(step["range"].as_str().unwrap()))),
        "filter-off" => s.set_filter(sheet, None),
        "filter" => super::filters::run_filter(s, sheet, step, ctx),
        "reapply" => {
            s.apply_filter(sheet);
            Ok(())
        }
        "sort" => run_sort(s, sheet, step, ctx),
        "remove-duplicates" => {
            let range = range_of(step["range"].as_str().unwrap());
            let columns: Vec<u32> = step["columns"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_u64().unwrap() as u32 - 1)
                .collect();
            s.remove_duplicates_with(
                sheet,
                range,
                &columns,
                header_mode(step["header"].as_str().unwrap_or("no")),
            )
            .map(|_| ())
        }
        "text-to-columns" => run_text_to_columns(s, sheet, step),
        "consolidate" => run_consolidate(s, sheet, step),
        "advanced-filter" => {
            let list = range_of(step["range"].as_str().unwrap());
            let criteria = step["criteria"].as_str().map(range_of);
            let copy_to = step["copyTo"].as_str().map(range_of);
            s.advanced_filter(
                sheet,
                list,
                criteria,
                copy_to,
                step["unique"].as_bool().unwrap_or(false),
            )
        }
        "show-all" => {
            if s.book.sheets[sheet].filter.is_some() {
                s.clear_filter(sheet)
            } else {
                s.show_all_rows(sheet)
            }
        }
        other => Err(format!("the test does not know the step {other}")),
    }
}

fn column_format(code: u64) -> ColumnFormat {
    match code {
        2 => ColumnFormat::Text,
        3 => ColumnFormat::Date(DateOrder::Mdy),
        4 => ColumnFormat::Date(DateOrder::Dmy),
        5 => ColumnFormat::Date(DateOrder::Ymd),
        6 => ColumnFormat::Date(DateOrder::Myd),
        7 => ColumnFormat::Date(DateOrder::Dym),
        8 => ColumnFormat::Date(DateOrder::Ydm),
        9 => ColumnFormat::Skip,
        _ => ColumnFormat::General,
    }
}

fn run_text_to_columns(s: &mut Session, sheet: usize, step: &Json) -> Result<(), String> {
    let range = range_of(step["range"].as_str().unwrap());
    let info: Vec<(u64, u64)> = step["fieldInfo"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| (p[0].as_u64().unwrap(), p[1].as_u64().unwrap()))
        .collect();
    let flag = |name: &str| step[name].as_bool().unwrap_or(false);
    let (mode, formats) = if step["type"].as_str() == Some("fixed") {
        let breaks: Vec<usize> = info.iter().map(|(start, _)| *start as usize).collect();
        (SplitMode::FixedWidth(breaks), info.iter().map(|(_, f)| column_format(*f)).collect())
    } else {
        let mut formats: Vec<ColumnFormat> = Vec::new();
        for (column, format) in &info {
            let at = *column as usize - 1;
            if formats.len() <= at {
                formats.resize(at + 1, ColumnFormat::General);
            }
            formats[at] = column_format(*format);
        }
        let d = Delimiters {
            tab: flag("tab"),
            semicolon: flag("semicolon"),
            comma: flag("comma"),
            space: flag("space"),
            other: if flag("other") {
                step["otherChar"].as_str().unwrap_or("").to_owned()
            } else {
                String::new()
            },
            consecutive_as_one: flag("consecutive"),
            qualifier: match step["qualifier"].as_str() {
                Some("double") => Some('"'),
                Some("single") => Some('\''),
                _ => None,
            },
        };
        (SplitMode::Delimited(d), formats)
    };
    let destination = step["destination"].as_str().map(|d| CellRef::parse(d).unwrap());
    s.text_to_columns_with(sheet, range, &TextToColumns { mode, formats, destination })
}

/// A source such as `Sheet1!R1C1:R4C3`, which is how Excel's Consolidate method is given a range.
fn source_of(s: &mut Session, text: &str) -> (usize, Range) {
    let (name, rest) = text.split_once('!').unwrap();
    let nums: Vec<u32> = rest
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse().unwrap())
        .collect();
    let range =
        Range::new(CellRef::new(nums[0] - 1, nums[1] - 1), CellRef::new(nums[2] - 1, nums[3] - 1));
    (super::sheet_of(s, name.trim_matches('\'')), range)
}

fn run_consolidate(s: &mut Session, sheet: usize, step: &Json) -> Result<(), String> {
    let function = match step["function"].as_str().unwrap_or("sum") {
        "count" => ConsolidateFunction::Count,
        "average" => ConsolidateFunction::Average,
        "max" => ConsolidateFunction::Max,
        "min" => ConsolidateFunction::Min,
        "product" => ConsolidateFunction::Product,
        "countnums" => ConsolidateFunction::CountNumbers,
        "stdev" => ConsolidateFunction::StdDev,
        "stdevp" => ConsolidateFunction::StdDevP,
        "var" => ConsolidateFunction::Var,
        "varp" => ConsolidateFunction::VarP,
        _ => ConsolidateFunction::Sum,
    };
    let sources: Vec<(usize, Range)> = step["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| source_of(s, t.as_str().unwrap()))
        .collect();
    let request = Consolidation {
        destination: CellRef::parse(step["destination"].as_str().unwrap()).unwrap(),
        sources,
        function,
        top_row: step["topRow"].as_bool().unwrap_or(false),
        left_column: step["leftColumn"].as_bool().unwrap_or(false),
        create_links: step["links"].as_bool().unwrap_or(false),
        book_name: "Book1".to_owned(),
    };
    s.consolidate(sheet, &request)
}

fn level_of(step: &Json, lv: &Json, columns: bool) -> SortLevel {
    let key = CellRef::parse(lv["key"].as_str().unwrap().split(':').next().unwrap()).unwrap();
    let key = if columns { key.row } else { key.col };
    let color = lv["color"].as_str().map(rgb);
    let on = match lv["on"].as_str().unwrap_or("values") {
        "cellcolor" => SortOn::CellColor(color),
        "fontcolor" => SortOn::FontColor(color),
        "icon" => {
            SortOn::Icon(Some(("3Arrows".to_owned(), lv["iconId"].as_u64().unwrap() as u32 - 1)))
        }
        _ => SortOn::Values,
    };
    let _ = step;
    SortLevel {
        key,
        on,
        descending: lv["order"].as_str() == Some("desc"),
        custom_list: lv["custom"]
            .as_str()
            .map(|l| l.split(',').map(|i| i.trim().to_owned()).collect()),
    }
}

fn run_sort(s: &mut Session, sheet: usize, step: &Json, ctx: &mut Context) -> Result<(), String> {
    let columns = step["orientation"].as_str() == Some("columns");
    let levels: Vec<SortLevel> =
        step["levels"].as_array().unwrap().iter().map(|l| level_of(step, l, columns)).collect();
    let mut options = SortOptions {
        header: header_mode(step["header"].as_str().unwrap_or("guess")),
        case_sensitive: step["matchCase"].as_bool().unwrap_or(false),
        left_to_right: columns,
        remember: false,
    };
    let range: Range = if let Some(name) = step["table"].as_str() {
        options.header = HeaderMode::No;
        options.remember = true;
        let t = s.book.table(name).ok_or("no such table")?;
        t.data_range().ok_or("table has no data")?
    } else if step["via"].as_str() == Some("filter") {
        options.header = HeaderMode::No;
        options.remember = true;
        let f = s.book.sheets[sheet].filter.as_ref().ok_or("no filter")?;
        Range::new(CellRef::new(f.range.first.row + 1, f.range.first.col), f.range.last)
    } else {
        range_of(step["range"].as_str().unwrap())
    };
    match &ctx.lights {
        Some(lights) => s.sort_with(sheet, range, &levels, &options, lights),
        None => s.sort_levels(sheet, range, &levels, &options),
    }
}
