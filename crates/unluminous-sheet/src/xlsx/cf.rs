//! Conditional formats in the file: the `conditionalFormatting` elements of the worksheet and the
//! `x14:conditionalFormattings` extension Excel 2010 and later add beside them.
//!
//! Excel splits a rule between the two. A data bar has its twin in the extension, paired by a GUID, which
//! holds its border, axis, direction and negative colours. A rule the 2007 schema cannot say, such as the
//! stars, triangles and boxes icon sets, custom icons, and formulas that name another sheet, is only in the
//! extension. Both are read into the same [`CfRule`], and written back where Excel puts them.

use roxmltree::Node;

use super::read::{bool_attr, child, color, read_dxf};
use super::{attrs_of, attrs_text, escape, raw};
use crate::coord::Range;
use crate::model::{CfRule, Cfvo, ConditionalFormat, DataBarExt, IconSetRule, Sheet, X14Payload};
use crate::styles::{Color, Styles};

pub const X14_NS: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
pub const XM_NS: &str = "http://schemas.microsoft.com/office/excel/2006/main";
/// The `uri` of the extension that holds the conditional formats.
pub const CF_EXT_URI: &str = "{78C0D931-6437-407d-A8EE-F0AAD7539E65}";
/// The `uri` of the extension inside a rule that names its twin in the worksheet's extension.
pub const RULE_EXT_URI: &str = "{B025F937-C7B1-47D3-B67F-A62EFF666E3E}";

/// The rule attributes the model holds in fields; every other one is kept.
const MODELLED: &[&str] = &[
    "type",
    "dxfId",
    "priority",
    "stopIfTrue",
    "operator",
    "text",
    "rank",
    "percent",
    "bottom",
    "aboveAverage",
    "equalAverage",
    "stdDev",
    "timePeriod",
    "id",
];

fn number<T: std::str::FromStr>(n: Node, name: &str) -> Option<T> {
    n.attribute(name).and_then(|v| v.parse().ok())
}

/// A flag that is true unless the file says otherwise.
fn flag_default_true(n: Node, name: &str) -> bool {
    !matches!(n.attribute(name), Some("0") | Some("false"))
}

// ---------------------------------------------------------------------------------------------------
// Reading.

/// One `conditionalFormatting` element of the worksheet.
pub(super) fn read_conditional_formatting(xml: &str, section: Node) -> ConditionalFormat {
    let ranges: Vec<Range> = section
        .attribute("sqref")
        .unwrap_or("")
        .split_whitespace()
        .filter_map(Range::parse)
        .collect();
    let rules = section
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "cfRule")
        .map(|r| read_rule(xml, r))
        .collect();
    ConditionalFormat { ranges, rules }
}

/// A threshold: its type and value, and for an icon its `gte` flag. The value is an attribute in the main
/// part and an `xm:f` child in the extension.
fn read_cfvo(n: Node) -> (Cfvo, bool) {
    let value = n
        .attribute("val")
        .map(str::to_owned)
        .or_else(|| child(n, "f").map(|f| f.text().unwrap_or("").to_owned()));
    (
        Cfvo { kind: n.attribute("type").unwrap_or("min").to_owned(), value },
        flag_default_true(n, "gte"),
    )
}

fn cfvos(n: Node) -> Vec<(Cfvo, bool)> {
    n.children()
        .filter(|c| c.is_element() && c.tag_name().name() == "cfvo")
        .map(read_cfvo)
        .collect()
}

fn read_icon_set(n: Node) -> IconSetRule {
    let name = n.attribute("iconSet").unwrap_or("3TrafficLights1").to_owned();
    let custom = n
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "cfIcon")
        .map(|c| {
            let set = c.attribute("iconSet").unwrap_or("NoIcons");
            (set != "NoIcons").then(|| (set.to_owned(), number(c, "iconId").unwrap_or(0)))
        })
        .collect();
    IconSetRule {
        name,
        thresholds: cfvos(n),
        reverse: bool_attr(n, "reverse"),
        show_value: flag_default_true(n, "showValue"),
        custom,
    }
}

fn read_data_bar_main(db: Node) -> DataBarExt {
    DataBarExt {
        show_value: flag_default_true(db, "showValue"),
        main_min_length: number(db, "minLength").unwrap_or(10),
        main_max_length: number(db, "maxLength").unwrap_or(90),
        has_extension: false,
        ..Default::default()
    }
}

/// Move an icon set that a reader of another format wrote as XML in `raw_children` into the model.
pub fn lift_icon_set(rule: &mut CfRule) {
    if !rule.raw_children.starts_with("<iconSet") {
        return;
    }
    let wrapped = format!("<r>{}</r>", rule.raw_children);
    if let Ok(doc) = roxmltree::Document::parse(&wrapped) {
        if let Some(n) = doc.root_element().first_element_child() {
            rule.icon_set = Some(read_icon_set(n));
            rule.raw_children.clear();
        }
    }
}

/// A main-part `cfRule`.
pub(super) fn read_rule(xml: &str, r: Node) -> CfRule {
    let mut rule = CfRule::new(r.attribute("type").unwrap_or(""));
    read_rule_attributes(r, &mut rule);
    rule.formulas = r
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "formula")
        .map(|f| f.text().unwrap_or("").to_owned())
        .collect();
    rule.color_scale = child(r, "colorScale").map(|cs| {
        let points: Vec<Cfvo> = cfvos(cs).into_iter().map(|(c, _)| c).collect();
        let colors: Vec<Color> =
            cs.children().filter(|n| n.tag_name().name() == "color").map(color).collect();
        points.into_iter().zip(colors).collect()
    });
    if let Some(db) = child(r, "dataBar") {
        let points: Vec<Cfvo> = cfvos(db).into_iter().map(|(c, _)| c).collect();
        let c = child(db, "color").map_or(Color::rgb(0x63, 0x8E, 0xC6), color);
        if points.len() >= 2 {
            rule.data_bar = Some((points[0].clone(), points[1].clone(), c));
            rule.data_bar_options = Some(read_data_bar_main(db));
        }
    }
    rule.icon_set = child(r, "iconSet").map(read_icon_set);
    rule.x14 = child(r, "extLst")
        .and_then(|e| e.descendants().find(|n| n.is_element() && n.tag_name().name() == "id"))
        .and_then(|id| id.text())
        .map(|id| X14Payload { id: id.trim().to_owned(), only: false });
    for c in r.children().filter(Node::is_element) {
        match c.tag_name().name() {
            "formula" | "colorScale" | "dataBar" | "iconSet" | "extLst" => {}
            _ => rule.raw_children.push_str(raw(xml, c)),
        }
    }
    rule
}

fn read_rule_attributes(r: Node, rule: &mut CfRule) {
    rule.dxf = number(r, "dxfId");
    rule.priority = number(r, "priority").unwrap_or(1);
    rule.stop = bool_attr(r, "stopIfTrue");
    rule.operator = r.attribute("operator").map(str::to_owned);
    rule.text = r.attribute("text").map(str::to_owned);
    rule.rank = number(r, "rank");
    rule.percent = bool_attr(r, "percent");
    rule.bottom = bool_attr(r, "bottom");
    rule.above_average = r.attribute("aboveAverage").map(|v| v != "0" && v != "false");
    rule.equal_average = bool_attr(r, "equalAverage");
    rule.std_dev = number(r, "stdDev");
    rule.time_period = r.attribute("timePeriod").map(str::to_owned);
    rule.attrs = attrs_of(r, MODELLED);
}

fn read_colour_child(n: Node, name: &str) -> Option<Color> {
    child(n, name).map(color)
}

/// The `x14:dataBar` of a paired rule, applied to the options its main twin gave.
fn read_data_bar_ext(db: Node, main: Option<DataBarExt>) -> DataBarExt {
    let main = main.unwrap_or_default();
    let kinds: Vec<String> = cfvos(db).into_iter().map(|(c, _)| c.kind).collect();
    DataBarExt {
        auto_min: kinds.first().is_some_and(|k| k == "autoMin"),
        auto_max: kinds.get(1).is_some_and(|k| k == "autoMax"),
        min_length: number(db, "minLength").unwrap_or(0),
        max_length: number(db, "maxLength").unwrap_or(100),
        border: bool_attr(db, "border"),
        gradient: flag_default_true(db, "gradient"),
        direction: db.attribute("direction").unwrap_or("context").to_owned(),
        negative_same_fill: bool_attr(db, "negativeBarColorSameAsPositive"),
        negative_same_border: flag_default_true(db, "negativeBarBorderColorSameAsPositive"),
        axis_position: db.attribute("axisPosition").unwrap_or("automatic").to_owned(),
        border_color: read_colour_child(db, "borderColor"),
        negative_fill: read_colour_child(db, "negativeFillColor"),
        negative_border: read_colour_child(db, "negativeBorderColor"),
        axis_color: read_colour_child(db, "axisColor"),
        has_extension: true,
        ..main
    }
}

/// A rule that lives only in the extension, with its inline format added to the workbook's.
fn read_only_rule(xml: &str, r: Node, styles: &mut Styles) -> CfRule {
    let mut rule = CfRule::new(r.attribute("type").unwrap_or(""));
    read_rule_attributes(r, &mut rule);
    rule.formulas = r
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "f")
        .map(|f| f.text().unwrap_or("").to_owned())
        .collect();
    rule.icon_set = child(r, "iconSet").map(read_icon_set);
    if let Some(db) = child(r, "dataBar") {
        let points: Vec<Cfvo> = cfvos(db).into_iter().map(|(c, _)| c).collect();
        let fill = read_colour_child(db, "color").unwrap_or(Color::rgb(0x63, 0x8E, 0xC6));
        if points.len() >= 2 {
            rule.data_bar = Some((points[0].clone(), points[1].clone(), fill));
        }
        rule.data_bar_options = Some(read_data_bar_ext(db, None));
    }
    if let Some(dxf) = child(r, "dxf") {
        let mut format = read_dxf(xml, dxf);
        format.raw = format.raw.map(|text| unprefix_dxf(&text));
        styles.dxfs.push(format);
        rule.dxf = Some(styles.dxfs.len() as u32 - 1);
    }
    rule.x14 = Some(X14Payload { id: r.attribute("id").unwrap_or("").to_owned(), only: true });
    rule
}

/// A `x14:dxf` element as a plain `dxf`, so it can be kept in the workbook's list of formats.
fn unprefix_dxf(text: &str) -> String {
    let start = text.find('>').map_or(0, |i| i + 1);
    let end = text.rfind("</").unwrap_or(text.len());
    format!("<dxf>{}</dxf>", &text[start..end])
}

/// Take the conditional formats out of a worksheet's `extLst` into the sheet, and give back the XML of the
/// extensions that remain, or nothing when none do.
pub(super) fn read_extension_list(
    xml: &str,
    list: Node,
    sheet: &mut Sheet,
    styles: &mut Styles,
) -> Option<String> {
    let mut kept = String::new();
    for ext in list.children().filter(|n| n.is_element() && n.tag_name().name() == "ext") {
        if ext.attribute("uri") == Some(CF_EXT_URI) {
            read_conditional_extension(xml, ext, sheet, styles);
        } else {
            kept.push_str(raw(xml, ext));
        }
    }
    (!kept.is_empty()).then(|| format!("<extLst>{kept}</extLst>"))
}

fn read_conditional_extension(xml: &str, ext: Node, sheet: &mut Sheet, styles: &mut Styles) {
    let blocks = ext
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "conditionalFormatting");
    for block in blocks {
        let ranges: Vec<Range> = child(block, "sqref")
            .and_then(|s| s.text())
            .unwrap_or("")
            .split_whitespace()
            .filter_map(Range::parse)
            .collect();
        let mut only = Vec::new();
        for r in block.children().filter(|n| n.is_element() && n.tag_name().name() == "cfRule") {
            let id = r.attribute("id").unwrap_or("");
            match find_twin(sheet, id) {
                Some(twin) => {
                    if let Some(db) = child(r, "dataBar") {
                        twin.data_bar_options =
                            Some(read_data_bar_ext(db, twin.data_bar_options.take()));
                    }
                }
                None => only.push(read_only_rule(xml, r, styles)),
            }
        }
        if !only.is_empty() {
            sheet.conditional.push(ConditionalFormat { ranges, rules: only });
        }
    }
}

/// The main-part rule whose `x14:id` is `id`.
fn find_twin<'a>(sheet: &'a mut Sheet, id: &str) -> Option<&'a mut CfRule> {
    if id.is_empty() {
        return None;
    }
    sheet
        .conditional
        .iter_mut()
        .flat_map(|cf| cf.rules.iter_mut())
        .find(|r| r.x14.as_ref().is_some_and(|x| !x.only && x.id == id))
}

// ---------------------------------------------------------------------------------------------------
// Writing.

/// Whether a rule is written in the extension only.
pub fn is_extension_only(rule: &CfRule) -> bool {
    rule.x14.as_ref().is_some_and(|x| x.only)
        || rule.icon_set.as_ref().is_some_and(IconSetRule::needs_extension)
        || mentions_another_sheet(rule)
}

/// A rule's formulas name a cell on a sheet, which the 2007 schema does not allow.
fn mentions_another_sheet(rule: &CfRule) -> bool {
    rule.formulas.iter().any(|f| f.contains('!'))
}

/// Whether a rule has a twin in the extension: a data bar that has options beyond the 2007 element.
fn has_twin(rule: &CfRule) -> bool {
    rule.data_bar.is_some() && rule.data_bar_options.as_ref().is_some_and(|o| o.has_extension)
}

fn color_attrs(c: &Color) -> String {
    super::write::color_attrs(c)
}

fn write_cfvo(c: &Cfvo, gte: Option<bool>) -> String {
    let gte = match gte {
        Some(false) => " gte=\"0\"",
        _ => "",
    };
    format!(
        "<cfvo type=\"{}\"{}{}/>",
        escape(&c.kind),
        c.value.as_ref().map(|v| format!(" val=\"{}\"", escape(v))).unwrap_or_default(),
        gte
    )
}

fn write_icon_set(set: &IconSetRule) -> String {
    let mut a = String::new();
    if set.name != "3TrafficLights1" {
        a.push_str(&format!(" iconSet=\"{}\"", escape(&set.name)));
    }
    if !set.show_value {
        a.push_str(" showValue=\"0\"");
    }
    if set.reverse {
        a.push_str(" reverse=\"1\"");
    }
    let points: String = set.thresholds.iter().map(|(c, gte)| write_cfvo(c, Some(*gte))).collect();
    format!("<iconSet{a}>{points}</iconSet>")
}

fn write_data_bar_main(rule: &CfRule) -> String {
    let Some((lo, hi, fill)) = &rule.data_bar else { return String::new() };
    let mut a = String::new();
    if let Some(o) = &rule.data_bar_options {
        if o.main_min_length != 10 {
            a.push_str(&format!(" minLength=\"{}\"", o.main_min_length));
        }
        if o.main_max_length != 90 {
            a.push_str(&format!(" maxLength=\"{}\"", o.main_max_length));
        }
        if !o.show_value {
            a.push_str(" showValue=\"0\"");
        }
    }
    format!(
        "<dataBar{a}>{}{}<color{}/></dataBar>",
        write_cfvo(lo, None),
        write_cfvo(hi, None),
        color_attrs(fill)
    )
}

/// The attributes of a rule, in the order Excel writes them.
fn rule_attributes(rule: &CfRule, id: Option<&str>) -> String {
    let mut a = vec![("type".to_owned(), rule.kind.clone())];
    let mut push = |k: &str, v: String| a.push((k.to_owned(), v));
    if let Some(d) = rule.dxf {
        push("dxfId", d.to_string());
    }
    push("priority", rule.priority.to_string());
    if rule.stop {
        push("stopIfTrue", "1".to_owned());
    }
    if rule.above_average == Some(false) {
        push("aboveAverage", "0".to_owned());
    }
    if rule.percent {
        push("percent", "1".to_owned());
    }
    if rule.bottom {
        push("bottom", "1".to_owned());
    }
    if let Some(op) = &rule.operator {
        push("operator", op.clone());
    }
    if let Some(t) = &rule.text {
        push("text", t.clone());
    }
    if let Some(t) = &rule.time_period {
        push("timePeriod", t.clone());
    }
    if let Some(r) = rule.rank {
        push("rank", r.to_string());
    }
    if let Some(s) = rule.std_dev {
        push("stdDev", s.to_string());
    }
    if rule.equal_average {
        push("equalAverage", "1".to_owned());
    }
    a.extend(rule.attrs.iter().cloned());
    if let Some(id) = id {
        a.push(("id".to_owned(), id.to_owned()));
    }
    attrs_text(&a)
}

/// The `conditionalFormatting` elements for a format's rules that the main part holds.
pub(super) fn write_main(cf: &ConditionalFormat, sheet: &str, format: usize) -> String {
    let rules: Vec<(usize, &CfRule)> =
        cf.rules.iter().enumerate().filter(|(_, r)| !is_extension_only(r)).collect();
    if rules.is_empty() {
        return String::new();
    }
    let sqref: Vec<String> = cf.ranges.iter().map(Range::a1).collect();
    let mut out = format!("<conditionalFormatting sqref=\"{}\">", sqref.join(" "));
    for (index, rule) in rules {
        out.push_str(&format!("<cfRule{}>", rule_attributes(rule, None)));
        for f in &rule.formulas {
            out.push_str(&format!("<formula>{}</formula>", escape(f)));
        }
        if let Some(scale) = &rule.color_scale {
            out.push_str("<colorScale>");
            scale.iter().for_each(|(c, _)| out.push_str(&write_cfvo(c, None)));
            scale.iter().for_each(|(_, c)| out.push_str(&format!("<color{}/>", color_attrs(c))));
            out.push_str("</colorScale>");
        }
        out.push_str(&write_data_bar_main(rule));
        if let Some(set) = &rule.icon_set {
            out.push_str(&write_icon_set(set));
        }
        out.push_str(&rule.raw_children);
        if has_twin(rule) {
            let id = rule_id(rule, sheet, format, index);
            out.push_str(&format!(
                "<extLst><ext uri=\"{RULE_EXT_URI}\" xmlns:x14=\"{X14_NS}\"><x14:id>{id}</x14:id></ext></extLst>"
            ));
        }
        out.push_str("</cfRule>");
    }
    out.push_str("</conditionalFormatting>");
    out
}

fn write_ext_cfvo(c: &Cfvo, gte: Option<bool>) -> String {
    let gte = if gte == Some(false) { " gte=\"0\"" } else { "" };
    match &c.value {
        Some(v) => format!(
            "<x14:cfvo type=\"{}\"{gte}><xm:f>{}</xm:f></x14:cfvo>",
            escape(&c.kind),
            escape(v)
        ),
        None => format!("<x14:cfvo type=\"{}\"{gte}/>", escape(&c.kind)),
    }
}

fn write_ext_data_bar(rule: &CfRule) -> String {
    let (Some((lo, hi, _)), Some(o)) = (&rule.data_bar, &rule.data_bar_options) else {
        return String::new();
    };
    let mut a = format!(" minLength=\"{}\" maxLength=\"{}\"", o.min_length, o.max_length);
    if o.border {
        a.push_str(" border=\"1\"");
    }
    if !o.gradient {
        a.push_str(" gradient=\"0\"");
    }
    if o.direction != "context" {
        a.push_str(&format!(" direction=\"{}\"", escape(&o.direction)));
    }
    if o.negative_same_fill {
        a.push_str(" negativeBarColorSameAsPositive=\"1\"");
    }
    if !o.negative_same_border {
        a.push_str(" negativeBarBorderColorSameAsPositive=\"0\"");
    }
    if o.axis_position != "automatic" {
        a.push_str(&format!(" axisPosition=\"{}\"", escape(&o.axis_position)));
    }
    // The 2007 element can only say `min` and `max`; the extension says when they are the automatic ones.
    let auto = |c: &Cfvo, on: bool, from: &str, to: &str| match on && c.kind == from {
        true => Cfvo { kind: to.to_owned(), value: None },
        false => c.clone(),
    };
    let (lo, hi) = (auto(lo, o.auto_min, "min", "autoMin"), auto(hi, o.auto_max, "max", "autoMax"));
    let mut children = format!("{}{}", write_ext_cfvo(&lo, None), write_ext_cfvo(&hi, None));
    for (name, c) in [
        ("borderColor", &o.border_color),
        ("negativeFillColor", &o.negative_fill),
        ("negativeBorderColor", &o.negative_border),
        ("axisColor", &o.axis_color),
    ] {
        if let Some(c) = c {
            children.push_str(&format!("<x14:{name}{}/>", color_attrs(c)));
        }
    }
    format!("<x14:dataBar{a}>{children}</x14:dataBar>")
}

fn write_ext_icon_set(set: &IconSetRule) -> String {
    let mut a = String::new();
    if set.name != "3TrafficLights1" {
        a.push_str(&format!(" iconSet=\"{}\"", escape(&set.name)));
    }
    if !set.show_value {
        a.push_str(" showValue=\"0\"");
    }
    if !set.custom.is_empty() {
        a.push_str(" custom=\"1\"");
    }
    if set.reverse {
        a.push_str(" reverse=\"1\"");
    }
    let mut body: String =
        set.thresholds.iter().map(|(c, gte)| write_ext_cfvo(c, Some(*gte))).collect();
    for icon in &set.custom {
        match icon {
            Some((name, id)) => body
                .push_str(&format!("<x14:cfIcon iconSet=\"{}\" iconId=\"{id}\"/>", escape(name))),
            None => body.push_str("<x14:cfIcon iconSet=\"NoIcons\" iconId=\"0\"/>"),
        }
    }
    format!("<x14:iconSet{a}>{body}</x14:iconSet>")
}

/// The inline format of an extension rule, from the workbook's list of formats.
fn write_ext_dxf(rule: &CfRule, styles: &Styles) -> String {
    let Some(dxf) = rule.dxf.and_then(|i| styles.dxfs.get(i as usize)) else {
        return String::new();
    };
    let text = match &dxf.raw {
        Some(raw) => raw.clone(),
        None => super::write::dxf_xml(dxf),
    };
    let start = text.find('>').map_or(0, |i| i + 1);
    let end = text.rfind("</").unwrap_or(text.len());
    format!("<x14:dxf>{}</x14:dxf>", &text[start..end])
}

fn write_ext_rule(rule: &CfRule, id: &str, only: bool, styles: &Styles) -> String {
    let mut out = String::new();
    if only {
        out.push_str(&format!("<x14:cfRule{}>", rule_attributes(rule, Some(id))));
        for f in &rule.formulas {
            out.push_str(&format!("<xm:f>{}</xm:f>", escape(f)));
        }
    } else {
        out.push_str(&format!(
            "<x14:cfRule type=\"{}\" id=\"{}\">",
            escape(&rule.kind),
            escape(id)
        ));
    }
    out.push_str(&write_ext_data_bar(rule));
    if let Some(set) = &rule.icon_set {
        out.push_str(&write_ext_icon_set(set));
    }
    if only {
        out.push_str(&write_ext_dxf(rule, styles));
    }
    out.push_str("</x14:cfRule>");
    out
}

/// The `ext` element holding every rule that needs the extension, or an empty string when none does.
pub(super) fn write_extension(sheet: &Sheet, styles: &Styles) -> String {
    let mut blocks: Vec<(i32, String)> = Vec::new();
    for (format, cf) in sheet.conditional.iter().enumerate() {
        let mut body = String::new();
        let mut first = i32::MAX;
        for (index, rule) in cf.rules.iter().enumerate() {
            let only = is_extension_only(rule);
            if !only && !has_twin(rule) {
                continue;
            }
            let id = rule_id(rule, &sheet.name, format, index);
            first = first.min(rule.priority);
            body.push_str(&write_ext_rule(rule, &id, only, styles));
        }
        if body.is_empty() {
            continue;
        }
        let sqref: Vec<String> = cf.ranges.iter().map(Range::a1).collect();
        blocks.push((
            first,
            format!(
                "<x14:conditionalFormatting xmlns:xm=\"{XM_NS}\">{body}<xm:sqref>{}</xm:sqref></x14:conditionalFormatting>",
                sqref.join(" ")
            ),
        ));
    }
    if blocks.is_empty() {
        return String::new();
    }
    blocks.sort_by_key(|(priority, _)| *priority);
    let inner: String = blocks.into_iter().map(|(_, b)| b).collect();
    format!(
        "<ext uri=\"{CF_EXT_URI}\" xmlns:x14=\"{X14_NS}\"><x14:conditionalFormattings>{inner}</x14:conditionalFormattings></ext>"
    )
}

/// The GUID that pairs a rule with its twin in the extension: the one it has, or one made from the sheet's
/// name and the rule's place, so writing a workbook twice gives the same file.
fn rule_id(rule: &CfRule, sheet: &str, format: usize, index: usize) -> String {
    match &rule.x14 {
        Some(x) if !x.id.is_empty() => x.id.clone(),
        _ => guid(&format!("{sheet}|{format}|{index}|{}", rule.priority)),
    }
}

/// A GUID in Excel's braced upper case form, made from some text.
fn guid(seed: &str) -> String {
    let mut h: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for b in seed.bytes() {
        h ^= u128::from(b);
        h = h.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013B);
    }
    let hex = format!("{h:032X}");
    format!(
        "{{{}-{}-4{}-8{}-{}}}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    )
}
