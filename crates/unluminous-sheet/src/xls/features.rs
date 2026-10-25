//! The parts of a BIFF8 sheet that are not cells: hyperlinks, data validation, conditional formats,
//! filters, protection and the view.

use std::collections::BTreeMap;

use super::ptg::{decode, Decoded, Dialect, Origin};
use super::records::{Reader, Record};
use super::{id, sheet::Context};
use crate::coord::{CellRef, Range};
use crate::formula::print::number;
use crate::model::{
    AutoFilter, CfRule, Cfvo, ConditionalFormat, FilterCriteria, Hyperlink, LinkTarget, Sheet,
    Validation,
};
use crate::styles::{Color, Dxf, Underline};

/// What the sheet's feature records add up to, applied to the sheet when it ends.
#[derive(Default)]
pub struct Collector {
    /// The rows and columns a pane record splits at.
    pub pane: Option<(u32, u32)>,
    /// Whether the window freezes the panes.
    pub frozen: bool,
    links: Vec<Hyperlink>,
    validations: Vec<Validation>,
    blocks: Vec<Block>,
    /// Rules of the newer kinds that came with no group of their own.
    strays: Vec<CfRule>,
    /// How many rules of the newer kinds the group just started says follow it.
    newer_rules_to_come: u16,
    protection: Protection,
    filter_entries: Vec<(u32, FilterCriteria)>,
    has_filter: bool,
    tab_color: Option<Color>,
}

/// A group of conditional format rules over some ranges.
struct Block {
    ranges: Vec<Range>,
    rules: Vec<CfRule>,
}

/// Sheet protection as the records give it.
#[derive(Default)]
struct Protection {
    locked: bool,
    objects: bool,
    scenarios: bool,
    password: u16,
    allowed: Option<u32>,
}

impl Collector {
    /// Take one record that is not a cell, a row or a column.
    pub fn record(&mut self, record: &Record, _sheet: &mut Sheet, ctx: &Context) {
        let flag = || record.data.first().is_some_and(|b| b & 1 != 0);
        match record.id {
            id::HLINK => self.links.extend(hyperlink(record)),
            id::DV => self.validations.extend(validation(record, ctx)),
            id::CONDFMT | id::CONDFMT12 => self.start_block(record),
            id::CF => self.old_rule(record, ctx),
            id::CF12 => self.new_rule(record),
            id::AUTOFILTERINFO => self.has_filter = true,
            id::AUTOFILTER => self.filter_entries.extend(filter_column(record)),
            id::PROTECT => self.protection.locked = flag(),
            id::OBJPROTECT => self.protection.objects = flag(),
            id::SCENPROTECT => self.protection.scenarios = flag(),
            id::PASSWORD => {
                self.protection.password =
                    record.data.get(..2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
            }
            id::SHEETPROTECTION => self.feature_header(record),
            id::SHEETEXT => self.tab_color = tab_color(record, ctx),
            _ => {}
        }
    }

    /// Put what was collected on the sheet.
    pub fn finish(&mut self, sheet: &mut Sheet, ctx: &Context, sheet_index: usize) {
        if self.frozen {
            sheet.frozen = self.pane;
        }
        sheet.links = std::mem::take(&mut self.links);
        sheet.validations = std::mem::take(&mut self.validations);
        sheet.conditional = self
            .settle_conditional_formats()
            .into_iter()
            .map(|b| ConditionalFormat { ranges: b.ranges, rules: b.rules })
            .collect();
        sheet.tab_color = self.tab_color;
        if self.protection.locked {
            sheet.raw.elements.push(("sheetProtection".to_owned(), self.protection_xml()));
        }
        if self.has_filter {
            sheet.filter = self.auto_filter(ctx, sheet_index);
        }
    }

    /// The `sheetProtection` element an `.xlsx` file would hold for this sheet.
    fn protection_xml(&self) -> String {
        const ALLOWED: [(u32, &str, bool); 13] = [
            (2, "formatCells", true),
            (3, "formatColumns", true),
            (4, "formatRows", true),
            (5, "insertColumns", true),
            (6, "insertRows", true),
            (7, "insertHyperlinks", true),
            (8, "deleteColumns", true),
            (9, "deleteRows", true),
            (10, "selectLockedCells", false),
            (11, "sort", true),
            (12, "autoFilter", true),
            (13, "pivotTables", true),
            (14, "selectUnlockedCells", false),
        ];
        let mut xml = String::from("<sheetProtection");
        if self.protection.password != 0 {
            xml.push_str(&format!(" password=\"{:04X}\"", self.protection.password));
        }
        xml.push_str(" sheet=\"1\"");
        if self.protection.objects {
            xml.push_str(" objects=\"1\"");
        }
        if self.protection.scenarios {
            xml.push_str(" scenarios=\"1\"");
        }
        if let Some(bits) = self.protection.allowed {
            for (bit, name, locked_by_default) in ALLOWED {
                let allowed = bits & (1 << bit) != 0;
                match (locked_by_default, allowed) {
                    (true, true) => xml.push_str(&format!(" {name}=\"0\"")),
                    (false, false) => xml.push_str(&format!(" {name}=\"1\"")),
                    _ => {}
                }
            }
        }
        xml.push_str("/>");
        xml
    }

    /// A feature header record, which for protection holds what the protected sheet still allows.
    fn feature_header(&mut self, record: &Record) {
        let mut r = Reader::new(&record.data);
        if r.skip(12).is_none() || r.u16() != Some(2) {
            return;
        }
        let _ = (r.u8(), r.i32());
        self.protection.allowed = r.u32();
    }

    fn start_block(&mut self, record: &Record) {
        let mut r = Reader::new(&record.data);
        let newer = record.id == id::CONDFMT12;
        if newer && r.skip(12).is_none() {
            return;
        }
        let Some(rules) = r.u16() else { return };
        if r.skip(2 + 8).is_none() {
            return;
        }
        let Some(count) = r.u16() else { return };
        let ranges = (0..count).filter_map(|_| ref8(&mut r)).collect();
        self.newer_rules_to_come = if newer { rules } else { 0 };
        self.blocks.push(Block { ranges, rules: Vec::new() });
    }

    /// A rule of the older kind. Its priority is worked out when all the rules are known, because the
    /// file numbers only the rules of the newer kind.
    fn old_rule(&mut self, record: &Record, ctx: &Context) {
        let priority = 0;
        let Some(block) = self.blocks.last_mut() else { return };
        let origin = block
            .ranges
            .first()
            .map_or(Origin::default(), |r| Origin { row: r.first.row, col: r.first.col });
        if let Some((rule, dxf)) = old_conditional_rule(record, ctx, origin, priority) {
            let mut rule = rule;
            rule.dxf = Some(ctx.dxfs.push(dxf));
            block.rules.push(rule);
        }
    }

    fn new_rule(&mut self, record: &Record) {
        let Some(rule) = new_conditional_rule(record) else { return };
        match self.blocks.last_mut() {
            Some(block) if self.newer_rules_to_come > 0 => {
                self.newer_rules_to_come -= 1;
                block.rules.push(rule);
            }
            _ => self.strays.push(rule),
        }
    }

    /// The groups of rules with every rule numbered and in order. A rule of the older kind is numbered
    /// with the numbers the newer kind's rules leave free, in the order they came; a rule of the newer
    /// kind that has no group goes with the rule of the older kind before it in priority.
    fn settle_conditional_formats(&mut self) -> Vec<Block> {
        let mut blocks = std::mem::take(&mut self.blocks);
        let strays = std::mem::take(&mut self.strays);
        let mut taken: std::collections::HashSet<i32> = strays.iter().map(|r| r.priority).collect();
        taken.extend(
            blocks.iter().flat_map(|b| b.rules.iter()).map(|r| r.priority).filter(|&p| p > 0),
        );
        let mut next = 1;
        let mut older: Vec<(i32, usize)> = Vec::new();
        for (index, block) in blocks.iter_mut().enumerate() {
            for rule in block.rules.iter_mut().filter(|r| r.priority == 0) {
                while taken.contains(&next) {
                    next += 1;
                }
                rule.priority = next;
                older.push((next, index));
                next += 1;
            }
        }
        for stray in strays {
            let home = older
                .iter()
                .filter(|(priority, _)| *priority < stray.priority)
                .max()
                .map_or(0, |&(_, index)| index);
            if let Some(block) = blocks.get_mut(home) {
                block.rules.push(stray);
            }
        }
        for block in &mut blocks {
            block.rules.sort_by_key(|r| r.priority);
        }
        blocks
    }

    /// The filter over a range found in the sheet's `_FilterDatabase` name.
    fn auto_filter(&mut self, ctx: &Context, sheet_index: usize) -> Option<AutoFilter> {
        let name = ctx
            .globals
            .names
            .iter()
            .find(|n| n.builtin == Some(0x0D) && n.scope as usize == sheet_index + 1)?;
        let text = match decode(Dialect::Biff8, &name.rgce, &name.rgcb, ctx.env, Origin::default())
        {
            Ok(Decoded::Text(t)) => t,
            _ => return None,
        };
        let range = Range::parse(text.rsplit('!').next()?.replace('$', "").as_str())?;
        let columns: BTreeMap<u32, FilterCriteria> =
            std::mem::take(&mut self.filter_entries).into_iter().collect();
        Some(AutoFilter { range, columns, sort: Default::default() })
    }
}

/// A tooltip for the link over the same cells, from a `LINKTIP` record.
pub fn link_tip(record: &Record, tips: &mut Vec<(Range, String)>) {
    let mut r = Reader::new(&record.data);
    if r.skip(2).is_none() {
        return;
    }
    let Some(range) = ref8(&mut r) else { return };
    let text = utf16_nul(r.rest());
    tips.push((range, text));
}

/// Four 16 bit numbers: the first and last row and column of a range.
fn ref8(r: &mut Reader) -> Option<Range> {
    let (r1, r2, c1, c2) = (r.u16()?, r.u16()?, r.u16()?, r.u16()?);
    Some(Range::new(CellRef::new(r1 as u32, c1 as u32), CellRef::new(r2 as u32, c2 as u32)))
}

/// UTF-16 text up to its first null.
fn utf16_nul(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// A length counted in characters, then those characters as UTF-16 including the null.
fn hyperlink_string(r: &mut Reader) -> Option<String> {
    let cch = r.u32()? as usize;
    let bytes = r.take(cch.checked_mul(2)?)?;
    Some(utf16_nul(bytes))
}

const URL_MONIKER: [u8; 16] = [
    0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B,
];
const FILE_MONIKER: [u8; 16] = [
    0x03, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

/// A `HLINK` record: the cells, the text shown, the target and the place in the target.
pub fn hyperlink(record: &Record) -> Option<Hyperlink> {
    let mut r = Reader::new(&record.data);
    let range = ref8(&mut r)?;
    r.skip(16)?;
    r.skip(4)?;
    let flags = r.u32()?;
    let display = if flags & 0x10 != 0 { Some(hyperlink_string(&mut r)?) } else { None };
    if flags & 0x80 != 0 {
        hyperlink_string(&mut r)?;
    }
    let mut target = None;
    if flags & 0x01 != 0 {
        target =
            Some(if flags & 0x100 != 0 { hyperlink_string(&mut r)? } else { moniker(&mut r)? });
    }
    let location = if flags & 0x08 != 0 { Some(hyperlink_string(&mut r)?) } else { None };
    let target = match (target, location) {
        (Some(t), Some(l)) if !l.is_empty() => LinkTarget::External(format!("{t}#{l}")),
        (Some(t), _) => LinkTarget::External(t),
        (None, l) => LinkTarget::Location(l.unwrap_or_default()),
    };
    Some(Hyperlink { range, target, display, tooltip: None })
}

/// The address a moniker names: a web address, or a file path.
fn moniker(r: &mut Reader) -> Option<String> {
    let class: [u8; 16] = r.take(16)?.try_into().ok()?;
    if class == URL_MONIKER {
        let length = r.u32()? as usize;
        let bytes = r.take(length.min(r.rest().len()))?;
        return Some(utf16_nul(bytes));
    }
    if class == FILE_MONIKER {
        let up = r.u16()? as usize;
        let ansi_length = r.u32()? as usize;
        let ansi = r.take(ansi_length)?;
        let path: String = ansi.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
        r.skip(2 + 2 + 20)?;
        if let Some(size) = r.u32().filter(|&s| s > 6) {
            let _ = r.u32();
            let _ = r.u16();
            if let Some(bytes) = r.take(size as usize - 6) {
                let unicode = utf16_nul(bytes);
                if !unicode.is_empty() {
                    return Some(unicode);
                }
            }
        }
        return Some(format!("{}{path}", "..\\".repeat(up)));
    }
    None
}

fn trim_nul(text: String) -> Option<String> {
    let trimmed = text.trim_end_matches('\0').to_owned();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// A `DV` record: one validation rule over some ranges.
pub fn validation(record: &Record, ctx: &Context) -> Option<Validation> {
    let mut r = Reader::new(&record.data);
    let flags = r.u32()?;
    let prompt_title = trim_nul(r.string()?);
    let error_title = trim_nul(r.string()?);
    let prompt = trim_nul(r.string()?);
    let error = trim_nul(r.string()?);
    let cce1 = r.u16()? as usize;
    r.skip(2)?;
    let rgce1 = r.take(cce1)?.to_vec();
    let cce2 = r.u16()? as usize;
    r.skip(2)?;
    let rgce2 = r.take(cce2)?.to_vec();
    let count = r.u16()?;
    let ranges: Vec<Range> = (0..count).filter_map(|_| ref8(&mut r)).collect();
    let first = ranges
        .first()
        .map_or(Origin::default(), |range| Origin { row: range.first.row, col: range.first.col });
    let text = |rgce: &[u8]| -> Option<String> {
        if rgce.is_empty() {
            return None;
        }
        match decode(Dialect::Biff8, rgce, &[], ctx.env, first) {
            Ok(Decoded::Text(t)) => Some(t.replace('\0', ",")),
            _ => None,
        }
    };
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
    let takes_operator = !matches!(kind, "list" | "custom" | "none");
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
        operator: if takes_operator { operator.map(str::to_owned) } else { None },
        formula1: text(&rgce1),
        formula2: text(&rgce2),
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

/// The tab colour a `SHEETEXT` record holds.
fn tab_color(record: &Record, ctx: &Context) -> Option<Color> {
    let mut r = Reader::new(&record.data);
    r.skip(12 + 4 + 4 + 4)?;
    let kind = r.u32()?;
    let value = r.take(4)?;
    let tint = r.f64()?;
    match kind {
        1 => Some(ctx.palette.color(value[0] as u16)),
        2 => Some(Color::rgb(value[0], value[1], value[2])),
        3 => Some(Color::Theme(u32::from_le_bytes([value[0], value[1], value[2], value[3]]), tint)),
        _ => None,
    }
}

/// One column's criteria from an `AUTOFILTER` record: the column within the filter range and what it keeps.
fn filter_column(record: &Record) -> Option<(u32, FilterCriteria)> {
    let mut r = Reader::new(&record.data);
    let column = r.u16()? as u32;
    let flags = r.u16()?;
    let first = Doper::read(&mut r)?;
    let second = Doper::read(&mut r)?;
    if flags & 0x08 != 0 {
        let count = flags >> 7;
        let top = flags & 0x10 != 0;
        let percent = flags & 0x20 != 0;
        let mut xml = format!("<filterColumn colId=\"{column}\"><top10");
        if !top {
            xml.push_str(" top=\"0\"");
        }
        if percent {
            xml.push_str(" percent=\"1\"");
        }
        xml.push_str(&format!(" val=\"{count}\"/></filterColumn>"));
        return Some((column, FilterCriteria::Raw(xml)));
    }
    let first_text = first.text(&mut r);
    let second_text = second.text(&mut r);
    let mut conditions = Vec::new();
    for (doper, text) in [(&first, first_text), (&second, second_text)] {
        if let Some(value) = doper.value(text) {
            conditions.push((doper.operator().to_owned(), value));
        }
    }
    if conditions.is_empty() {
        return None;
    }
    let all_equal = conditions.iter().all(|(op, _)| op == "equal");
    if all_equal {
        let values = conditions.into_iter().map(|(_, v)| v).collect();
        return Some((column, FilterCriteria::Values { values, blank: false }));
    }
    Some((column, FilterCriteria::Custom { conditions, and: flags & 1 == 0 }))
}

/// One of the two conditions of an `AUTOFILTER` record.
struct Doper {
    kind: u8,
    sign: u8,
    number: f64,
    text_length: usize,
}

impl Doper {
    fn read(r: &mut Reader) -> Option<Doper> {
        let kind = r.u8()?;
        let sign = r.u8()?;
        let data = r.take(8)?;
        let number = match kind {
            2 => {
                let rk = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                super::sheet::rk_value(rk)
            }
            4 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(data);
                f64::from_le_bytes(b)
            }
            8 => data[0] as f64,
            _ => 0.0,
        };
        let text_length = if kind == 6 { data[4] as usize } else { 0 };
        Some(Doper { kind, sign, number, text_length })
    }

    /// The text of a string condition, which follows both conditions in the record.
    fn text(&self, r: &mut Reader) -> Option<String> {
        (self.kind == 6).then(|| r.string_of(self.text_length)).flatten()
    }

    fn operator(&self) -> &'static str {
        match self.sign {
            1 => "lessThan",
            3 => "lessThanOrEqual",
            4 => "greaterThan",
            5 => "notEqual",
            6 => "greaterThanOrEqual",
            _ => "equal",
        }
    }

    fn value(&self, text: Option<String>) -> Option<String> {
        match self.kind {
            2 | 4 => Some(number(self.number)),
            6 => text,
            _ => None,
        }
    }
}

/// A rule of the older kind: a comparison or a formula, with the differences it makes to a cell.
fn old_conditional_rule(
    record: &Record,
    ctx: &Context,
    origin: Origin,
    priority: i32,
) -> Option<(CfRule, Dxf)> {
    let mut r = Reader::new(&record.data);
    let kind = r.u8()?;
    let comparison = r.u8()?;
    let cce1 = r.u16()? as usize;
    let cce2 = r.u16()? as usize;
    let flags = r.u32()?;
    r.skip(2)?;
    let font = if flags & 0x0400_0000 != 0 { Some(r.take(118)?.to_vec()) } else { None };
    let border = if flags & 0x1000_0000 != 0 { Some(r.take(8)?.to_vec()) } else { None };
    let pattern = if flags & 0x2000_0000 != 0 { Some(r.take(4)?.to_vec()) } else { None };
    let text = |rgce: &[u8]| -> Option<String> {
        match decode(Dialect::Biff8, rgce, &[], ctx.env, origin) {
            Ok(Decoded::Text(t)) => Some(t),
            _ => None,
        }
    };
    let first = text(r.take(cce1)?);
    let second = if cce2 > 0 { text(r.take(cce2)?) } else { None };
    let dxf = Dxf {
        font_color: font.as_deref().and_then(|f| font_color(f, ctx)),
        bold: font.as_deref().and_then(font_bold),
        italic: font.as_deref().and_then(font_italic),
        strike: font.as_deref().and_then(font_strike),
        underline: font.as_deref().and_then(font_underline),
        fill: pattern.as_deref().and_then(|p| pattern_color(p, flags, ctx)),
        num_fmt: None,
        border: border.as_deref().and_then(|b| border_color(b, ctx)),
        raw: None,
    };
    let operator = match comparison {
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
    let is_value = kind == 1;
    let rule = CfRule {
        kind: if is_value { "cellIs" } else { "expression" }.to_owned(),
        dxf: None,
        priority,
        stop: false,
        operator: (is_value && !operator.is_empty()).then(|| operator.to_owned()),
        text: None,
        rank: None,
        percent: false,
        bottom: false,
        above_average: None,
        formulas: [first, second].into_iter().flatten().collect(),
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
    Some((rule, dxf))
}

fn font_color(font: &[u8], ctx: &Context) -> Option<Color> {
    let icv = u32::from_le_bytes(font.get(80..84)?.try_into().ok()?);
    (icv != 0xFFFF_FFFF).then(|| ctx.palette.color(icv as u16))
}

fn font_flags(font: &[u8]) -> u32 {
    font.get(88..92).map_or(u32::MAX, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn font_bold(font: &[u8]) -> Option<bool> {
    let weight = u16::from_le_bytes([*font.get(72)?, *font.get(73)?]);
    (font_flags(font) & 2 == 0).then_some(weight >= 700)
}

fn font_italic(font: &[u8]) -> Option<bool> {
    let options = u32::from_le_bytes(font.get(68..72)?.try_into().ok()?);
    (font_flags(font) & 1 == 0).then_some(options & 2 != 0)
}

fn font_strike(font: &[u8]) -> Option<bool> {
    let options = u32::from_le_bytes(font.get(68..72)?.try_into().ok()?);
    (font_flags(font) & 0x80 == 0).then_some(options & 0x80 != 0)
}

fn font_underline(font: &[u8]) -> Option<Underline> {
    let flag = u32::from_le_bytes(font.get(96..100)?.try_into().ok()?);
    (flag == 0).then(|| match font.get(76) {
        Some(1) => Underline::Single,
        Some(2) => Underline::Double,
        _ => Underline::None,
    })
}

/// The fill colour a pattern block asks for.
fn pattern_color(block: &[u8], flags: u32, ctx: &Context) -> Option<Color> {
    let colors = u16::from_le_bytes([*block.get(2)?, *block.get(3)?]);
    let fore = colors & 0x7F;
    let back = (colors >> 7) & 0x7F;
    if flags & (1 << 18) == 0 {
        Some(ctx.palette.color(back))
    } else if flags & (1 << 17) == 0 {
        Some(ctx.palette.color(fore))
    } else {
        None
    }
}

fn border_color(block: &[u8], ctx: &Context) -> Option<Color> {
    let colors = u32::from_le_bytes(block.get(4..8)?.try_into().ok()?);
    let left = (colors & 0x7F) as u16;
    (left != 0).then(|| ctx.palette.color(left))
}

/// A rule of the newer kinds, which the file keeps in a record of its own: icon sets, data bars and
/// colour scales.
fn new_conditional_rule(record: &Record) -> Option<CfRule> {
    let data = &record.data;
    let priority = u16::from_le_bytes([*data.get(27)?, *data.get(28)?]) as i32;
    let template = u16::from_le_bytes([*data.get(29)?, *data.get(30)?]);
    let mut rule = CfRule {
        kind: String::new(),
        dxf: None,
        priority,
        stop: false,
        operator: None,
        text: None,
        rank: None,
        percent: false,
        bottom: false,
        above_average: None,
        formulas: Vec::new(),
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
    match template {
        2 => rule.color_scale = Some(color_scale(data)?),
        3 => rule.data_bar = Some(data_bar(data)?),
        4 => {
            rule.raw_children = icon_set(data)?;
            crate::xlsx::cf::lift_icon_set(&mut rule);
        }
        _ => return None,
    }
    rule.kind = match template {
        2 => "colorScale",
        3 => "dataBar",
        _ => "iconSet",
    }
    .to_owned();
    Some(rule)
}

fn cfvo_name(kind: u8) -> &'static str {
    match kind {
        1 => "num",
        2 => "min",
        3 => "max",
        4 => "percent",
        5 => "percentile",
        _ => "formula",
    }
}

/// One threshold: its kind, and its value when it has one. A data bar leaves the number out of a
/// minimum and a maximum, and a colour scale or an icon set always writes it.
fn read_cfvo(r: &mut Reader, always_has_a_number: bool) -> Option<Cfvo> {
    let kind = r.u8()?;
    let cb = r.u16()? as usize;
    r.skip(cb)?;
    let has_number = always_has_a_number || matches!(kind, 1 | 4 | 5 | 6);
    let value = if has_number { r.f64()? } else { 0.0 };
    Some(Cfvo {
        kind: cfvo_name(kind).to_owned(),
        value: matches!(kind, 1 | 4 | 5).then(|| number(value)),
    })
}

fn icon_set(data: &[u8]) -> Option<String> {
    let mut r = Reader::new(data);
    r.skip(48 + 2 + 1)?;
    let count = r.u8()?;
    let icons = r.u8()?;
    let flags = r.u8()?;
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
    let name = NAMES.get(icons as usize).copied().unwrap_or("3TrafficLights1");
    let mut xml = String::from("<iconSet");
    if name != "3TrafficLights1" {
        xml.push_str(&format!(" iconSet=\"{name}\""));
    }
    if flags & 1 != 0 {
        xml.push_str(" showValue=\"0\"");
    }
    if flags & 4 != 0 {
        xml.push_str(" reverse=\"1\"");
    }
    xml.push('>');
    for _ in 0..count {
        let cfvo = read_cfvo(&mut r, true)?;
        r.skip(5)?;
        xml.push_str(&format!("<cfvo type=\"{}\"", cfvo.kind));
        if let Some(v) = cfvo.value {
            xml.push_str(&format!(" val=\"{v}\""));
        }
        xml.push_str("/>");
    }
    xml.push_str("</iconSet>");
    Some(xml)
}

fn data_bar(data: &[u8]) -> Option<(Cfvo, Cfvo, Color)> {
    let mut r = Reader::new(data);
    r.skip(58)?;
    let rgba = r.take(4)?;
    let color = Color::rgb(rgba[0], rgba[1], rgba[2]);
    r.skip(8)?;
    let min = read_cfvo(&mut r, false)?;
    let max = read_cfvo(&mut r, false)?;
    Some((min, max, color))
}

fn color_scale(data: &[u8]) -> Option<Vec<(Cfvo, Color)>> {
    let mut r = Reader::new(data);
    r.skip(48 + 3)?;
    let count = r.u8()? as usize;
    r.skip(2)?;
    let mut points = Vec::new();
    for _ in 0..count {
        let cfvo = read_cfvo(&mut r, true)?;
        if matches!(cfvo.kind.as_str(), "percent" | "percentile") {
            r.skip(8)?;
        }
        points.push(cfvo);
    }
    let mut out = Vec::new();
    for cfvo in points {
        r.skip(8 + 4)?;
        let rgba = r.take(4)?;
        r.skip(8)?;
        out.push((cfvo, Color::rgb(rgba[0], rgba[1], rgba[2])));
    }
    Some(out)
}
