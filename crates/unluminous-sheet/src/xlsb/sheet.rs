//! One worksheet part of an `.xlsb` file.

use std::collections::BTreeMap;

use super::records::{id, many, nullable_string, split, wide_string, Reader, Record};
use super::styles::color;
use crate::coord::{CellRef, Range};
use crate::formula::print::number;
use crate::model::{
    AutoFilter, CfRule, Cfvo, ColumnInfo, ConditionalFormat, FilterCriteria, Hyperlink, LinkTarget,
    RowInfo, Sheet, Validation,
};
use crate::styles::Color;
use crate::value::{ErrorKind, Value};
use crate::xls::ptg::{decode, error_text, Decoded, Dialect, Env, Origin};
use crate::xls::sheet::formula_from;

/// What reading a sheet needs from the workbook.
pub struct Context<'a> {
    pub strings: &'a [String],
    pub env: &'a Env<'a>,
    /// The target of each relationship of the sheet, by id, for hyperlinks.
    pub links: &'a [(String, String)],
}

/// A shared or array formula: its tokens and the cells it covers.
struct Stored {
    rgce: Vec<u8>,
    rgcb: Vec<u8>,
    range: Range,
}

/// A rule being read: its header, and the pieces that follow it until it ends.
struct RuleInProgress {
    rule: CfRule,
    template: u32,
    kind: u32,
    flags: u16,
    param: i32,
    percent_icon: Option<(u32, u16)>,
    thresholds: Vec<Cfvo>,
    colors: Vec<Color>,
    scale: bool,
    bar: bool,
}

struct State {
    row: u32,
    default_height: u32,
    pending: Vec<(u32, u32)>,
    shared: Vec<Stored>,
    arrays: Vec<Stored>,
    blocks: Vec<(Vec<Range>, Vec<CfRule>)>,
    rule: Option<RuleInProgress>,
    filter_range: Option<Range>,
    filter_column: Option<u32>,
    filter_values: Vec<String>,
    filter_conditions: Vec<(String, String)>,
    filter_other: Option<String>,
    filter_and: bool,
    columns: BTreeMap<u32, FilterCriteria>,
    protection: Option<String>,
    frozen: bool,
    pane: Option<(u32, u32)>,
}

/// Read a sheet part.
pub fn read(bytes: &[u8], sheet: &mut Sheet, ctx: &Context) {
    let mut state = State {
        row: 0,
        default_height: 0,
        pending: Vec::new(),
        shared: Vec::new(),
        arrays: Vec::new(),
        blocks: Vec::new(),
        rule: None,
        filter_range: None,
        filter_column: None,
        filter_values: Vec::new(),
        filter_conditions: Vec::new(),
        filter_other: None,
        filter_and: false,
        columns: BTreeMap::new(),
        protection: None,
        frozen: false,
        pane: None,
    };
    for record in split(bytes) {
        record_in_sheet(&record, sheet, ctx, &mut state);
    }
    resolve_pending(sheet, ctx, &state);
    if state.frozen {
        sheet.frozen = state.pane;
    }
    if let Some(range) = state.filter_range {
        sheet.filter = Some(AutoFilter {
            range,
            columns: std::mem::take(&mut state.columns),
            sort: Default::default(),
        });
    }
    if let Some(xml) = state.protection.take() {
        sheet.raw.elements.push(("sheetProtection".to_owned(), xml));
    }
    sheet.conditional = state
        .blocks
        .into_iter()
        .map(|(ranges, rules)| ConditionalFormat { ranges, rules })
        .collect();
}

fn record_in_sheet(record: &Record, sheet: &mut Sheet, ctx: &Context, state: &mut State) {
    let data = record.data;
    match record.id {
        id::WS_PROP => sheet.tab_color = tab_color(data),
        id::BEGIN_WS_VIEW => view(data, sheet),
        id::PANE => pane(data, state),
        id::WS_FMT_INFO => format_info(data, sheet, state),
        id::COL_INFO => column(data, sheet),
        id::ROW_HDR => row_header(data, sheet, state),
        id::CELL_BLANK..=id::FMLA_ERROR => cell(record, sheet, ctx, state),
        id::SHR_FMLA => stored_formula(data, &mut state.shared, false),
        id::ARR_FMLA => stored_formula(data, &mut state.arrays, true),
        id::MERGE_CELL => {
            if let Some(range) = rfx(&mut Reader::new(data)) {
                sheet.merges.push(range);
            }
        }
        id::HLINK => sheet.links.extend(hyperlink(data, ctx)),
        id::DVAL => sheet.validations.extend(validation(data, ctx)),
        id::SHEET_PROTECTION => state.protection = protection(data),
        id::BEGIN_CF => begin_conditional_format(data, state),
        id::BEGIN_CF_RULE => state.rule = begin_rule(data, ctx, state),
        id::END_CF_RULE => end_rule(state),
        id::BEGIN_ICON_SET => {
            if let (Some(rule), Some(index), Some(flags)) =
                (state.rule.as_mut(), u32_at(data, 0), u16_at(data, 4))
            {
                rule.percent_icon = Some((index, flags));
            }
        }
        id::BEGIN_COLOR_SCALE => {
            if let Some(rule) = state.rule.as_mut() {
                rule.scale = true;
            }
        }
        id::BEGIN_DATABAR => {
            if let Some(rule) = state.rule.as_mut() {
                rule.bar = true;
            }
        }
        id::CFVO => {
            if let (Some(rule), Some(cfvo)) = (state.rule.as_mut(), threshold(data)) {
                rule.thresholds.push(cfvo);
            }
        }
        id::COLOR => {
            if let (Some(rule), Some(c)) = (state.rule.as_mut(), color(&mut Reader::new(data))) {
                rule.colors.push(c);
            }
        }
        id::BEGIN_AFILTER => {
            state.filter_range = rfx(&mut Reader::new(data));
        }
        id::BEGIN_FILTER_COLUMN => filter_column_start(data, state),
        id::FILTER => {
            if let Some(text) = wide_string(&mut Reader::new(data)) {
                state.filter_values.push(text);
            }
        }
        id::BEGIN_CUSTOM_FILTERS => state.filter_and = u32_at(data, 0).is_some_and(|v| v != 0),
        id::CUSTOM_FILTER => custom_filter(data, state),
        id::TOP10_FILTER | id::COLOR_FILTER | id::DYNAMIC_FILTER => {
            state.filter_other = other_filter(record.id, data)
        }
        id::END_FILTER_COLUMN => filter_column_end(state),
        _ => {}
    }
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

/// A range as four 32 bit numbers: the first and last row, the first and last column.
fn rfx(r: &mut Reader) -> Option<Range> {
    let (r1, r2, c1, c2) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
    Some(Range::new(CellRef::new(r1, c1), CellRef::new(r2, c2)))
}

/// A list of ranges: a count, then that many.
fn sq_rfx(r: &mut Reader) -> Option<Vec<Range>> {
    let count = r.u32()?;
    Some(many(count, r, rfx))
}

fn tab_color(data: &[u8]) -> Option<Color> {
    let mut r = Reader::new(data);
    r.skip(3)?;
    match color(&mut r)? {
        Color::Auto => None,
        other => Some(other),
    }
}

fn view(data: &[u8], sheet: &mut Sheet) {
    if let Some(flags) = u16_at(data, 0) {
        sheet.show_grid = flags & 0x04 != 0;
    }
    if let Some(zoom) = u16_at(data, 18) {
        if zoom > 0 {
            sheet.zoom = zoom as u32;
        }
    }
}

fn pane(data: &[u8], state: &mut State) {
    let mut r = Reader::new(data);
    let (Some(x), Some(y)) = (r.f64(), r.f64()) else { return };
    if r.skip(12).is_none() {
        return;
    }
    let flags = r.u8().unwrap_or(0);
    state.frozen = flags & 1 != 0;
    state.pane = Some((y as u32, x as u32));
}

fn format_info(data: &[u8], sheet: &mut Sheet, state: &mut State) {
    let mut r = Reader::new(data);
    let (Some(_), Some(width), Some(height)) = (r.u32(), r.u16(), r.u16()) else { return };
    sheet.default_col_width = (width != 8).then_some(width as f64);
    sheet.default_row_height = height as f64 / 20.0;
    state.default_height = height as u32;
}

fn column(data: &[u8], sheet: &mut Sheet) {
    let mut r = Reader::new(data);
    let (Some(first), Some(last), Some(width), Some(style), Some(flags)) =
        (r.u32(), r.u32(), r.u32(), r.u32(), r.u16())
    else {
        return;
    };
    sheet.columns.push(ColumnInfo {
        first,
        last: last.min(crate::coord::MAX_COLS - 1),
        width: Some(width as f64 / 256.0),
        hidden: flags & 1 != 0,
        style,
        custom_width: flags & 2 != 0,
        outline: ((flags >> 8) & 7) as u8,
        collapsed: flags & 0x1000 != 0,
        best_fit: flags & 4 != 0,
    });
}

fn row_header(data: &[u8], sheet: &mut Sheet, state: &mut State) {
    let mut r = Reader::new(data);
    let (Some(row), Some(style), Some(height), Some(_), Some(flags)) =
        (r.u32(), r.u32(), r.u16(), r.u8(), r.u8())
    else {
        return;
    };
    state.row = row;
    let custom_height = flags & 0x20 != 0;
    let info = RowInfo {
        height: (custom_height || height as u32 != state.default_height)
            .then(|| height as f64 / 20.0),
        hidden: flags & 0x10 != 0,
        style: (flags & 0x40 != 0).then_some(style),
        outline: flags & 7,
        collapsed: flags & 0x08 != 0,
        attrs: Vec::new(),
    };
    if info != RowInfo::default() {
        sheet.rows.insert(row, info);
    }
}

fn cell(record: &Record, sheet: &mut Sheet, ctx: &Context, state: &mut State) {
    let mut r = Reader::new(record.data);
    let Some(col) = r.u32() else { return };
    let Some(style) = r.u32().map(|v| v & 0x00FF_FFFF) else { return };
    let row = state.row;
    let (value, formula) = match record.id {
        id::CELL_BLANK => (Value::Empty, None),
        id::CELL_RK => (
            r.u32().map_or(Value::Empty, |rk| Value::Number(crate::xls::sheet::rk_value(rk))),
            None,
        ),
        id::CELL_ERROR => (r.u8().map_or(Value::Empty, error_value), None),
        id::CELL_BOOL => (r.u8().map_or(Value::Empty, |b| Value::Bool(b != 0)), None),
        id::CELL_REAL => (r.f64().map_or(Value::Empty, Value::Number), None),
        id::CELL_ST => (wide_string(&mut r).map_or(Value::Empty, Value::text), None),
        id::CELL_ISST => {
            let text = r.u32().and_then(|i| ctx.strings.get(i as usize)).cloned();
            (text.map_or(Value::Empty, Value::text), None)
        }
        id::FMLA_STRING => {
            let value = wide_string(&mut r).map_or(Value::Empty, Value::text);
            (value, formula_tokens(&mut r))
        }
        id::FMLA_NUM => {
            let value = r.f64().map_or(Value::Empty, Value::Number);
            (value, formula_tokens(&mut r))
        }
        id::FMLA_BOOL => {
            let value = r.u8().map_or(Value::Empty, |b| Value::Bool(b != 0));
            (value, formula_tokens(&mut r))
        }
        _ => {
            let value = r.u8().map_or(Value::Empty, error_value);
            (value, formula_tokens(&mut r))
        }
    };
    let mut made = crate::model::Cell { value, style, ..Default::default() };
    if let Some((rgce, rgcb)) = formula {
        match decode(Dialect::Biff12, &rgce, &rgcb, ctx.env, Origin { row, col }) {
            Ok(Decoded::Exp { .. }) | Ok(Decoded::Table) => state.pending.push((row, col)),
            other => made.formula = formula_from(other),
        }
    }
    sheet.cells.insert((row, col), made);
}

fn error_value(code: u8) -> Value {
    Value::Error(ErrorKind::parse(error_text(code)).unwrap_or(ErrorKind::Value))
}

/// After a formula cell's value: the flags, then the tokens and the data they use.
fn formula_tokens(r: &mut Reader) -> Option<(Vec<u8>, Vec<u8>)> {
    r.skip(2)?;
    let cce = r.u32()? as usize;
    let rgce = r.take(cce)?.to_vec();
    let cb = r.u32().unwrap_or(0) as usize;
    let rgcb = r.take(cb.min(r.rest().len())).map(<[u8]>::to_vec).unwrap_or_default();
    Some((rgce, rgcb))
}

fn stored_formula(data: &[u8], list: &mut Vec<Stored>, array: bool) {
    let mut r = Reader::new(data);
    let Some(range) = rfx(&mut r) else { return };
    if array && r.skip(1).is_none() {
        return;
    }
    let Some(cce) = r.u32() else { return };
    let Some(rgce) = r.take(cce as usize) else { return };
    let cb = r.u32().unwrap_or(0) as usize;
    let rgcb = r.take(cb.min(r.rest().len())).map(<[u8]>::to_vec).unwrap_or_default();
    list.push(Stored { rgce: rgce.to_vec(), rgcb, range });
}

/// Give the cells that took their formula from a shared or array one its text.
fn resolve_pending(sheet: &mut Sheet, ctx: &Context, state: &State) {
    for &(row, col) in &state.pending {
        let at = CellRef::new(row, col);
        if let Some(shared) = state.shared.iter().find(|s| s.range.contains(at)) {
            let decoded =
                decode(Dialect::Biff12, &shared.rgce, &shared.rgcb, ctx.env, Origin { row, col });
            if let Some(cell) = sheet.cells.get_mut(&(row, col)) {
                cell.formula = formula_from(decoded);
            }
        } else if let Some(array) = state.arrays.iter().find(|a| a.range.contains(at)) {
            if array.range.first != at {
                continue;
            }
            let origin = Origin { row, col };
            if let Some(mut formula) =
                formula_from(decode(Dialect::Biff12, &array.rgce, &array.rgcb, ctx.env, origin))
            {
                formula.array = Some(array.range);
                sheet.array_formulas.push(array.range);
                if let Some(cell) = sheet.cells.get_mut(&(row, col)) {
                    cell.formula = Some(formula);
                }
            }
        }
    }
}

fn hyperlink(data: &[u8], ctx: &Context) -> Option<Hyperlink> {
    let mut r = Reader::new(data);
    let range = rfx(&mut r)?;
    let rel = wide_string(&mut r)?;
    let location = wide_string(&mut r)?;
    let tooltip = wide_string(&mut r)?;
    let display = wide_string(&mut r)?;
    let target = match ctx.links.iter().find(|(id, _)| *id == rel) {
        Some((_, target)) if location.is_empty() => LinkTarget::External(target.clone()),
        Some((_, target)) => LinkTarget::External(format!("{target}#{location}")),
        None => LinkTarget::Location(location),
    };
    Some(Hyperlink {
        range,
        target,
        display: (!display.is_empty()).then_some(display),
        tooltip: (!tooltip.is_empty()).then_some(tooltip),
    })
}

/// The tokens of a formula that has a length in front of it and data after it.
fn parsed_formula(r: &mut Reader, ctx: &Context, origin: Origin) -> Option<String> {
    let cce = r.u32()? as usize;
    if cce == 0 {
        return None;
    }
    let rgce = r.take(cce)?;
    let cb = r.u32().unwrap_or(0) as usize;
    let rgcb = r.take(cb.min(r.rest().len())).unwrap_or(&[]);
    match decode(Dialect::Biff12, rgce, rgcb, ctx.env, origin) {
        Ok(Decoded::Text(text)) => Some(text),
        _ => None,
    }
}

fn validation(data: &[u8], ctx: &Context) -> Option<Validation> {
    let mut r = Reader::new(data);
    let flags = r.u32()?;
    let ranges = sq_rfx(&mut r)?;
    let error_title = nullable_string(&mut r)?;
    let error = nullable_string(&mut r)?;
    let prompt_title = nullable_string(&mut r)?;
    let prompt = nullable_string(&mut r)?;
    let first = ranges
        .first()
        .map_or(Origin::default(), |range| Origin { row: range.first.row, col: range.first.col });
    let formula1 = parsed_formula(&mut r, ctx, first);
    let formula2 = parsed_formula(&mut r, ctx, first);
    let kind = match flags & 0xF {
        1 => "whole",
        2 => "decimal",
        3 => "list",
        4 => "date",
        5 => "time",
        6 => "textLength",
        7 => "custom",
        _ => "none",
    };
    let operator = match (flags >> 20) & 0xF {
        1 => Some("notBetween"),
        2 => Some("equal"),
        3 => Some("notEqual"),
        4 => Some("greaterThan"),
        5 => Some("lessThan"),
        6 => Some("greaterThanOrEqual"),
        7 => Some("lessThanOrEqual"),
        _ => None,
    };
    let mut attrs = Vec::new();
    if flags & 0x4_0000 != 0 {
        attrs.push(("showInputMessage".to_owned(), "1".to_owned()));
    }
    if flags & 0x8_0000 != 0 {
        attrs.push(("showErrorMessage".to_owned(), "1".to_owned()));
    }
    Some(Validation {
        ranges,
        kind: kind.to_owned(),
        operator: if matches!(kind, "list" | "custom" | "none") {
            None
        } else {
            operator.map(str::to_owned)
        },
        formula1,
        formula2,
        allow_blank: flags & 0x100 != 0,
        show_dropdown: flags & 0x200 == 0,
        prompt_title,
        prompt,
        error_title,
        error,
        error_style: match (flags >> 4) & 7 {
            1 => Some("warning".to_owned()),
            2 => Some("information".to_owned()),
            _ => None,
        },
        attrs,
    })
}

/// The `sheetProtection` element for a protected sheet.
fn protection(data: &[u8]) -> Option<String> {
    const FLAGS: [(&str, bool); 15] = [
        ("objects", false),
        ("scenarios", false),
        ("formatCells", true),
        ("formatColumns", true),
        ("formatRows", true),
        ("insertColumns", true),
        ("insertRows", true),
        ("insertHyperlinks", true),
        ("deleteColumns", true),
        ("deleteRows", true),
        ("selectLockedCells", false),
        ("sort", true),
        ("autoFilter", true),
        ("pivotTables", true),
        ("selectUnlockedCells", false),
    ];
    let mut r = Reader::new(data);
    let password = r.u16()?;
    let locked = r.u32()?;
    if locked == 0 {
        return None;
    }
    let mut xml = String::from("<sheetProtection");
    if password != 0 {
        xml.push_str(&format!(" password=\"{password:04X}\""));
    }
    xml.push_str(" sheet=\"1\"");
    for (name, default_locked) in FLAGS {
        // A flag of the binary format says the action is allowed; the attribute says it is locked.
        let locked = r.u32()? == 0;
        if locked != default_locked {
            xml.push_str(&format!(" {name}=\"{}\"", u8::from(locked)));
        }
    }
    xml.push_str("/>");
    Some(xml)
}

fn begin_conditional_format(data: &[u8], state: &mut State) {
    let mut r = Reader::new(data);
    if r.skip(8).is_none() {
        return;
    }
    if let Some(ranges) = sq_rfx(&mut r) {
        state.blocks.push((ranges, Vec::new()));
    }
}

fn begin_rule(data: &[u8], ctx: &Context, state: &State) -> Option<RuleInProgress> {
    let mut r = Reader::new(data);
    let kind = r.u32()?;
    let template = r.u32()?;
    let dxf = r.i32()?;
    let priority = r.u32()?;
    let param = r.i32()?;
    r.skip(8)?;
    let flags = r.u16()?;
    let (cb1, cb2, cb3) = (r.u32()?, r.u32()?, r.u32()?);
    let text = nullable_string(&mut r)?;
    let origin = state
        .blocks
        .last()
        .and_then(|(ranges, _)| ranges.first())
        .map_or(Origin::default(), |range| Origin { row: range.first.row, col: range.first.col });
    let formulas: Vec<String> = [cb1, cb2, cb3]
        .into_iter()
        .filter(|&cb| cb > 0)
        .filter_map(|_| parsed_formula(&mut r, ctx, origin))
        .collect();
    let operator = match param {
        1 => "between",
        2 => "notBetween",
        3 => "equal",
        4 => "notEqual",
        5 => "greaterThan",
        6 => "lessThan",
        7 => "greaterThanOrEqual",
        8 => "lessThanOrEqual",
        _ => "",
    };
    let rule = CfRule {
        kind: String::new(),
        dxf: (dxf >= 0).then_some(dxf as u32),
        priority: priority as i32,
        stop: flags & 2 != 0,
        operator: (kind == 1 && !operator.is_empty()).then(|| operator.to_owned()),
        text,
        rank: None,
        percent: false,
        bottom: false,
        above_average: None,
        formulas,
        color_scale: None,
        data_bar: None,
        attrs: Vec::new(),
        raw_children: String::new(),
        std_dev: None,
        equal_average: false,
        time_period: None,
        icon_set: None,
        data_bar_options: None,
        x14: None,
    };
    Some(RuleInProgress {
        rule,
        template,
        kind,
        flags,
        param,
        percent_icon: None,
        thresholds: Vec::new(),
        colors: Vec::new(),
        scale: false,
        bar: false,
    })
}

fn cfvo_name(kind: u32) -> &'static str {
    match kind {
        1 => "num",
        2 => "min",
        3 => "max",
        4 => "percent",
        5 => "percentile",
        _ => "formula",
    }
}

fn threshold(data: &[u8]) -> Option<Cfvo> {
    let mut r = Reader::new(data);
    let kind = r.u32()?;
    let value = r.f64()?;
    Some(Cfvo {
        kind: cfvo_name(kind).to_owned(),
        value: matches!(kind, 1 | 4 | 5).then(|| number(value)),
    })
}

/// Finish the rule that was being read and add it to the block it belongs to.
fn end_rule(state: &mut State) {
    let Some(mut work) = state.rule.take() else { return };
    if work.scale {
        work.rule.kind = "colorScale".to_owned();
        work.rule.color_scale = Some(work.thresholds.into_iter().zip(work.colors).collect());
    } else if work.bar {
        work.rule.kind = "dataBar".to_owned();
        let mut points = work.thresholds.into_iter();
        if let (Some(min), Some(max)) = (points.next(), points.next()) {
            let color = work.colors.first().copied().unwrap_or(Color::rgb(0x63, 0x8E, 0xC6));
            work.rule.data_bar = Some((min, max, color));
        }
    } else if let Some((index, flags)) = work.percent_icon {
        work.rule.kind = "iconSet".to_owned();
        work.rule.raw_children = icon_set_xml(index, flags, &work.thresholds);
        crate::xlsx::cf::lift_icon_set(&mut work.rule);
    } else if work.kind == 1 {
        work.rule.kind = "cellIs".to_owned();
    } else if !work.rule.formulas.is_empty() {
        work.rule.kind = "expression".to_owned();
    } else if !formula_less_rule(&mut work) {
        return;
    }
    if let Some((_, rules)) = state.blocks.last_mut() {
        rules.push(work.rule);
    }
}

/// A rule that needs no formula: a top ten or above average. Its numbers are in the rule's header.
fn formula_less_rule(work: &mut RuleInProgress) -> bool {
    let flags = work.flags;
    match work.template {
        5 => {
            work.rule.kind = "top10".to_owned();
            work.rule.rank = u32::try_from(work.param).ok();
            work.rule.percent = flags & 0x10 != 0;
            work.rule.bottom = flags & 0x08 != 0;
            true
        }
        6 | 7 => {
            work.rule.kind =
                if work.template == 6 { "duplicateValues" } else { "uniqueValues" }.to_owned();
            true
        }
        25 => {
            work.rule.kind = "aboveAverage".to_owned();
            work.rule.above_average = (flags & 0x04 == 0).then_some(false);
            if work.param > 0 {
                work.rule.attrs.push(("stdDev".to_owned(), work.param.to_string()));
            }
            true
        }
        _ => false,
    }
}

fn icon_set_xml(index: u32, flags: u16, thresholds: &[Cfvo]) -> String {
    const NAMES: [&str; 17] = [
        "3Arrows",
        "3ArrowsGray",
        "3Flags",
        "3TrafficLights1",
        "3TrafficLights2",
        "3Signs",
        "3Symbols",
        "3Symbols2",
        "4Arrows",
        "4ArrowsGray",
        "4RedToBlack",
        "4Rating",
        "4TrafficLights",
        "5Arrows",
        "5ArrowsGray",
        "5Rating",
        "5Quarters",
    ];
    let name = NAMES.get(index as usize).copied().unwrap_or("3TrafficLights1");
    let mut xml = String::from("<iconSet");
    if name != "3TrafficLights1" {
        xml.push_str(&format!(" iconSet=\"{name}\""));
    }
    if flags & 2 != 0 {
        xml.push_str(" showValue=\"0\"");
    }
    if flags & 4 != 0 {
        xml.push_str(" reverse=\"1\"");
    }
    xml.push('>');
    for cfvo in thresholds {
        xml.push_str(&format!("<cfvo type=\"{}\"", cfvo.kind));
        if let Some(v) = &cfvo.value {
            xml.push_str(&format!(" val=\"{v}\""));
        }
        xml.push_str("/>");
    }
    xml.push_str("</iconSet>");
    xml
}

fn filter_column_start(data: &[u8], state: &mut State) {
    state.filter_column = u32_at(data, 0);
    state.filter_values.clear();
    state.filter_conditions.clear();
    state.filter_other = None;
    state.filter_and = false;
}

fn custom_filter(data: &[u8], state: &mut State) {
    let mut r = Reader::new(data);
    let (Some(kind), Some(sign), Some(body)) = (r.u8(), r.u8(), r.take(8)) else { return };
    let operator = match sign {
        1 => "lessThan",
        3 => "lessThanOrEqual",
        4 => "greaterThan",
        5 => "notEqual",
        6 => "greaterThanOrEqual",
        _ => "equal",
    };
    let value = match kind {
        4 => {
            let mut b = [0u8; 8];
            b.copy_from_slice(body);
            Some(number(f64::from_le_bytes(b)))
        }
        6 => wide_string(&mut r),
        _ => None,
    };
    if let Some(value) = value {
        state.filter_conditions.push((operator.to_owned(), value));
    }
}

fn filter_column_end(state: &mut State) {
    let Some(column) = state.filter_column.take() else { return };
    let criteria = if let Some(inner) = state.filter_other.take() {
        FilterCriteria::Raw(format!("<filterColumn colId=\"{column}\">{inner}</filterColumn>"))
    } else if !state.filter_values.is_empty() {
        FilterCriteria::Values { values: std::mem::take(&mut state.filter_values), blank: false }
    } else if !state.filter_conditions.is_empty() {
        FilterCriteria::Custom {
            conditions: std::mem::take(&mut state.filter_conditions),
            and: state.filter_and,
        }
    } else {
        return;
    };
    state.columns.insert(column, criteria);
}

/// The kinds of dynamic filter, by the number the file gives them.
const DYNAMIC_FILTERS: [&str; 23] = [
    "null",
    "aboveAverage",
    "belowAverage",
    "tomorrow",
    "today",
    "yesterday",
    "nextWeek",
    "thisWeek",
    "lastWeek",
    "nextMonth",
    "thisMonth",
    "lastMonth",
    "nextQuarter",
    "thisQuarter",
    "lastQuarter",
    "nextYear",
    "thisYear",
    "lastYear",
    "yearToDate",
    "Q1",
    "Q2",
    "Q3",
    "Q4",
];

/// A top ten, colour or dynamic filter, as the element an `.xlsx` file holds it in.
fn other_filter(kind: u16, data: &[u8]) -> Option<String> {
    let mut r = Reader::new(data);
    match kind {
        id::TOP10_FILTER => {
            let flags = r.u8()?;
            let count = r.f64()?;
            let value = r.f64()?;
            let mut xml = String::from("<top10");
            if flags & 1 == 0 {
                xml.push_str(" top=\"0\"");
            }
            if flags & 2 != 0 {
                xml.push_str(" percent=\"1\"");
            }
            xml.push_str(&format!(" val=\"{}\" filterVal=\"{}\"/>", number(count), number(value)));
            Some(xml)
        }
        id::COLOR_FILTER => {
            let dxf = r.u32()?;
            let cell = r.u32()? & 1 != 0;
            Some(format!(
                "<colorFilter dxfId=\"{dxf}\"{}/>",
                if cell { "" } else { " cellColor=\"0\"" }
            ))
        }
        _ => {
            let kind = r.u32()? as usize;
            let name = match kind {
                0..=22 => DYNAMIC_FILTERS[kind].to_owned(),
                23..=34 => format!("M{}", kind - 22),
                _ => return None,
            };
            Some(format!("<dynamicFilter type=\"{name}\"/>"))
        }
    }
}
