//! Sparklines: the `x14:sparklineGroups` extension of a worksheet, modelled completely.
//!
//! A group has a type, the colours and switches that apply to all its sparklines, the vertical and
//! horizontal axis options, and one data range and one location cell for each sparkline. The groups are
//! taken out of the sheet's `extLst` when the file is read and put back into it when it is written, so
//! the rest of the extension list is kept as it was.

use crate::coord::{CellRef, Range};
use crate::model::{Sheet, Workbook};
use crate::styles::Color;
use crate::value::Value;
use crate::xlsx::xmltree::{self, Elem};

pub const EXT_URI: &str = "{05C60535-1F16-4fd2-B633-F4F36F0B64E0}";
const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const XM: &str = "http://schemas.microsoft.com/office/excel/2006/main";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SparklineKind {
    #[default]
    Line,
    Column,
    /// Wins and losses: a block up for a positive value and down for a negative one.
    WinLoss,
}

impl SparklineKind {
    fn attr(&self) -> Option<&'static str> {
        match self {
            SparklineKind::Line => None,
            SparklineKind::Column => Some("column"),
            SparklineKind::WinLoss => Some("stacked"),
        }
    }
}

/// How empty cells are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmptyCells {
    #[default]
    Gap,
    Zero,
    /// Connect the data points with a line.
    Span,
}

/// One end of the vertical axis.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AxisBound {
    /// Automatic for each sparkline.
    #[default]
    Automatic,
    /// The same for all sparklines of the group.
    SameForAll,
    Custom(f64),
}

/// One sparkline: where its numbers are and the cell it is drawn in.
#[derive(Debug, Clone, PartialEq)]
pub struct Sparkline {
    /// The data range as a formula: `Sheet1!B2:E2`.
    pub data: String,
    pub location: CellRef,
}

/// The colours of a group. `None` is the colour the file does not name, which Excel draws in its default.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SparklineColors {
    pub series: Option<Color>,
    pub negative: Option<Color>,
    pub axis: Option<Color>,
    pub markers: Option<Color>,
    pub first: Option<Color>,
    pub last: Option<Color>,
    pub high: Option<Color>,
    pub low: Option<Color>,
}

/// A group of sparklines that share their looks.
#[derive(Debug, Clone, PartialEq)]
pub struct SparklineGroup {
    pub kind: SparklineKind,
    /// The line width in points; `None` is Excel's 0.75.
    pub weight: Option<f64>,
    pub empty: EmptyCells,
    pub markers: bool,
    pub high: bool,
    pub low: bool,
    pub first: bool,
    pub last: bool,
    pub negative: bool,
    /// Show the horizontal axis.
    pub show_axis: bool,
    /// Plot cells in hidden rows and columns.
    pub hidden_cells: bool,
    pub right_to_left: bool,
    pub min: AxisBound,
    pub max: AxisBound,
    /// The custom minimum and maximum the file holds even when the axis is not set to use them.
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    /// A date axis: the range holding the dates, as a formula.
    pub date_axis: Option<String>,
    pub colors: SparklineColors,
    pub sparklines: Vec<Sparkline>,
    /// Attributes this version does not model (`xr2:uid`), kept.
    pub extra: Vec<(String, String)>,
}

impl Default for SparklineGroup {
    fn default() -> Self {
        Self {
            kind: SparklineKind::Line,
            weight: None,
            empty: EmptyCells::Gap,
            markers: false,
            high: false,
            low: false,
            first: false,
            last: false,
            negative: false,
            show_axis: false,
            hidden_cells: false,
            right_to_left: false,
            min: AxisBound::Automatic,
            max: AxisBound::Automatic,
            min_value: None,
            max_value: None,
            date_axis: None,
            colors: SparklineColors::default(),
            sparklines: Vec::new(),
            extra: Vec::new(),
        }
    }
}

// ------------------------------------------------------------------------------------------------ reading

fn flag(e: &Elem, name: &str) -> bool {
    matches!(e.attr(name), Some("1") | Some("true"))
}

fn read_color(e: &Elem) -> Option<Color> {
    if flag(e, "auto") {
        return Some(Color::Auto);
    }
    if let Some(rgb) = e.attr("rgb") {
        return u32::from_str_radix(rgb, 16).ok().map(Color::Rgb);
    }
    if let Some(theme) = e.attr("theme").and_then(|v| v.parse().ok()) {
        return Some(Color::Theme(theme, e.attr("tint").and_then(|v| v.parse().ok()).unwrap_or(0.0)));
    }
    e.attr("indexed").and_then(|v| v.parse().ok()).map(Color::Indexed)
}

fn read_bound(e: &Elem, kind_attr: &str, manual_attr: &str) -> AxisBound {
    match e.attr(kind_attr) {
        Some("group") => AxisBound::SameForAll,
        Some("custom") => AxisBound::Custom(e.attr(manual_attr).and_then(|v| v.parse().ok()).unwrap_or(0.0)),
        _ => AxisBound::Automatic,
    }
}

fn read_group(e: &Elem) -> SparklineGroup {
    let mut g = SparklineGroup {
        kind: match e.attr("type") {
            Some("column") => SparklineKind::Column,
            Some("stacked") => SparklineKind::WinLoss,
            _ => SparklineKind::Line,
        },
        weight: e.attr("lineWeight").and_then(|v| v.parse().ok()),
        empty: match e.attr("displayEmptyCellsAs") {
            Some("zero") => EmptyCells::Zero,
            Some("span") => EmptyCells::Span,
            _ => EmptyCells::Gap,
        },
        markers: flag(e, "markers"),
        high: flag(e, "high"),
        low: flag(e, "low"),
        first: flag(e, "first"),
        last: flag(e, "last"),
        negative: flag(e, "negative"),
        show_axis: flag(e, "displayXAxis"),
        hidden_cells: flag(e, "displayHidden"),
        right_to_left: flag(e, "rightToLeft"),
        min: read_bound(e, "minAxisType", "manualMin"),
        max: read_bound(e, "maxAxisType", "manualMax"),
        min_value: e.attr("manualMin").and_then(|v| v.parse().ok()),
        max_value: e.attr("manualMax").and_then(|v| v.parse().ok()),
        ..SparklineGroup::default()
    };
    const MODELLED: &[&str] = &[
        "type", "lineWeight", "displayEmptyCellsAs", "markers", "high", "low", "first", "last", "negative",
        "displayXAxis", "displayHidden", "rightToLeft", "minAxisType", "maxAxisType", "manualMin",
        "manualMax", "dateAxis",
    ];
    g.extra = e.attrs.iter().filter(|(n, _)| !MODELLED.contains(&n.as_str()) && !n.starts_with("xmlns")).cloned().collect();
    for c in e.elems() {
        match c.local() {
            "colorSeries" => g.colors.series = read_color(c),
            "colorNegative" => g.colors.negative = read_color(c),
            "colorAxis" => g.colors.axis = read_color(c),
            "colorMarkers" => g.colors.markers = read_color(c),
            "colorFirst" => g.colors.first = read_color(c),
            "colorLast" => g.colors.last = read_color(c),
            "colorHigh" => g.colors.high = read_color(c),
            "colorLow" => g.colors.low = read_color(c),
            "f" if flag(e, "dateAxis") => g.date_axis = Some(c.text()),
            "sparklines" => {
                for s in c.elems().filter(|s| s.local() == "sparkline") {
                    let data = s.child("f").map(Elem::text).unwrap_or_default();
                    let at = s.child("sqref").map(Elem::text).and_then(|t| CellRef::parse(t.trim()));
                    if let Some(location) = at {
                        g.sparklines.push(Sparkline { data, location });
                    }
                }
            }
            _ => {}
        }
    }
    g
}

/// Take the sparkline groups out of a sheet's `extLst`, leaving the rest of it where it was.
pub fn extract(sheet: &mut Sheet) {
    let Some(at) = sheet.raw.elements.iter().position(|(n, _)| n == "extLst") else { return };
    let Some(mut list) = xmltree::parse(&sheet.raw.elements[at].1) else { return };
    let is_ours = |e: &Elem| e.local() == "ext" && e.attr("uri") == Some(EXT_URI);
    let groups: Vec<SparklineGroup> = list
        .elems()
        .filter(|e| is_ours(e))
        .filter_map(|e| e.child("sparklineGroups"))
        .flat_map(|g| g.elems().filter(|e| e.local() == "sparklineGroup").map(read_group).collect::<Vec<_>>())
        .collect();
    if groups.is_empty() {
        return;
    }
    list.children.retain(|n| !matches!(n, xmltree::Node::Elem(e) if is_ours(e)));
    if list.elems().next().is_none() {
        sheet.raw.elements.remove(at);
    } else {
        sheet.raw.elements[at].1 = list.to_xml();
    }
    sheet.sparklines = groups;
}

// ------------------------------------------------------------------------------------------------ writing

fn color_elem(name: &str, c: &Color) -> Elem {
    let mut e = Elem::new(&format!("x14:{name}"));
    match c {
        Color::Auto => e.set_attr("auto", "1"),
        Color::Rgb(argb) => e.set_attr("rgb", &format!("{argb:08X}")),
        Color::Theme(index, tint) => {
            e.set_attr("theme", &index.to_string());
            if *tint != 0.0 {
                e.set_attr("tint", &crate::xlsx::write::number_text(*tint));
            }
        }
        Color::Indexed(i) => e.set_attr("indexed", &i.to_string()),
    }
    e
}

fn bound_attrs(e: &mut Elem, kind_attr: &str, manual_attr: &str, b: AxisBound, kept: Option<f64>) {
    let keep = |e: &mut Elem| {
        if let Some(v) = kept {
            e.set_attr(manual_attr, &crate::xlsx::write::number_text(v));
        }
    };
    match b {
        AxisBound::Automatic => keep(e),
        AxisBound::SameForAll => {
            e.set_attr(kind_attr, "group");
            keep(e);
        }
        AxisBound::Custom(v) => {
            e.set_attr(kind_attr, "custom");
            e.set_attr(manual_attr, &crate::xlsx::write::number_text(v));
        }
    }
}

fn group_elem(g: &SparklineGroup, declared: &dyn Fn(&str) -> bool) -> Elem {
    let mut e = Elem::new("x14:sparklineGroup");
    if let Some(t) = g.kind.attr() {
        e.set_attr("type", t);
    }
    if let Some(w) = g.weight {
        e.set_attr("lineWeight", &crate::xlsx::write::number_text(w));
    }
    match g.empty {
        EmptyCells::Gap => e.set_attr("displayEmptyCellsAs", "gap"),
        EmptyCells::Zero => e.set_attr("displayEmptyCellsAs", "zero"),
        EmptyCells::Span => e.set_attr("displayEmptyCellsAs", "span"),
    }
    if g.date_axis.is_some() {
        e.set_attr("dateAxis", "1");
    }
    bound_attrs(&mut e, "minAxisType", "manualMin", g.min, g.min_value);
    bound_attrs(&mut e, "maxAxisType", "manualMax", g.max, g.max_value);
    for (name, on) in [
        ("markers", g.markers),
        ("high", g.high),
        ("low", g.low),
        ("first", g.first),
        ("last", g.last),
        ("negative", g.negative),
        ("displayXAxis", g.show_axis),
        ("displayHidden", g.hidden_cells),
        ("rightToLeft", g.right_to_left),
    ] {
        if on {
            e.set_attr(name, "1");
        }
    }
    for (n, v) in &g.extra {
        let prefix = n.split_once(':').map_or("", |(p, _)| p);
        if prefix.is_empty() || declared(prefix) {
            e.set_attr(n, v);
        }
    }
    for (name, c) in [
        ("colorSeries", &g.colors.series),
        ("colorNegative", &g.colors.negative),
        ("colorAxis", &g.colors.axis),
        ("colorMarkers", &g.colors.markers),
        ("colorFirst", &g.colors.first),
        ("colorLast", &g.colors.last),
        ("colorHigh", &g.colors.high),
        ("colorLow", &g.colors.low),
    ] {
        if let Some(c) = c {
            e.push(color_elem(name, c));
        }
    }
    if let Some(range) = &g.date_axis {
        let mut f = Elem::new("xm:f");
        f.set_text(range);
        e.push(f);
    }
    let mut list = Elem::new("x14:sparklines");
    for s in &g.sparklines {
        let mut sp = Elem::new("x14:sparkline");
        let mut f = Elem::new("xm:f");
        f.set_text(&s.data);
        let mut q = Elem::new("xm:sqref");
        q.set_text(&s.location.a1());
        sp.push(f);
        sp.push(q);
        list.push(sp);
    }
    e.push(list);
    e
}

/// The sheet's `extLst` element with its sparkline groups in it: the kept extensions, then ours.
pub fn ext_list(sheet: &Sheet, kept: &str) -> String {
    if sheet.sparklines.is_empty() {
        return kept.to_owned();
    }
    let declared = |prefix: &str| sheet.raw.namespaces.iter().any(|(p, _)| p == prefix);
    let mut groups = Elem::new("x14:sparklineGroups");
    groups.set_attr("xmlns:xm", XM);
    for g in &sheet.sparklines {
        groups.push(group_elem(g, &declared));
    }
    let mut ext = Elem::new("ext");
    ext.set_attr("uri", EXT_URI);
    ext.set_attr("xmlns:x14", X14);
    ext.push(groups);
    let mut list = xmltree::parse(kept).unwrap_or_else(|| Elem::new("extLst"));
    list.push(ext);
    list.to_xml()
}

// --------------------------------------------------------------------------------------------- the numbers

/// What a drawing needs to draw one sparkline.
#[derive(Debug, Clone, PartialEq)]
pub struct SparklineData {
    /// One entry for each cell of the data range, `None` where it is empty or not a number.
    pub values: Vec<Option<f64>>,
    /// The bottom and top of the vertical axis.
    pub axis_min: f64,
    pub axis_max: f64,
    /// Indexes into `values` of the points the group highlights.
    pub high: Option<usize>,
    pub low: Option<usize>,
    pub first: Option<usize>,
    pub last: Option<usize>,
    pub negatives: Vec<usize>,
    /// Where bars start: zero, held inside the axis.
    pub baseline: f64,
}

/// The cells a data formula names, as a sheet index and a range.
fn data_cells(book: &Workbook, home: usize, formula: &str) -> Option<(usize, Range)> {
    let text = formula.trim().trim_start_matches('=');
    let (sheet, range) = match text.rsplit_once('!') {
        Some((name, range)) => {
            let name = name.trim_matches('\'').replace("''", "'");
            (book.sheet_index(&name)?, range)
        }
        None => (home, text),
    };
    Some((sheet, Range::parse(&range.replace('$', ""))?))
}

fn read_values(book: &Workbook, home: usize, group: &SparklineGroup, sp: &Sparkline) -> Vec<Option<f64>> {
    let Some((si, range)) = data_cells(book, home, &sp.data) else { return Vec::new() };
    let sheet = &book.sheets[si];
    let mut out = Vec::new();
    for at in range.cells() {
        if !group.hidden_cells && (sheet.is_row_hidden(at.row) || sheet.is_column_hidden(at.col)) {
            continue;
        }
        out.push(match sheet.value_at(at) {
            Value::Number(n) => Some(n),
            Value::Bool(_) | Value::Text(_) | Value::Error(_) | Value::Empty => None,
            #[allow(unreachable_patterns)]
            _ => None,
        });
    }
    out
}

/// The axis bounds Excel picks when the group leaves them automatic, measured against Excel's own drawing of
/// column and line sparklines: from the lowest value to the highest, whatever the kind. A series of one value
/// is drawn at the top of an axis one unit tall.
pub fn automatic_bounds(_kind: SparklineKind, low: f64, high: f64) -> (f64, f64) {
    if low == high {
        (low - 1.0, high)
    } else {
        (low, high)
    }
}

impl SparklineData {
    /// Where a point is down the plot: 0 at the top of the axis and 1 at the bottom.
    pub fn fraction_down(&self, value: f64) -> f64 {
        (self.axis_max - value) / (self.axis_max - self.axis_min)
    }

    /// The span of a column sparkline's bar for value `index`, as fractions down the plot from the top, or
    /// `None` for a value that is not there or is zero, which Excel draws no bar for. A bar is a bar even when
    /// the value is the baseline, so a drawing gives it at least one pixel.
    pub fn bar_span(&self, index: usize) -> Option<(f64, f64)> {
        let v = (*self.values.get(index)?)?;
        if v == 0.0 {
            return None;
        }
        let (a, b) = (self.fraction_down(v), self.fraction_down(self.baseline));
        Some((a.min(b), a.max(b)))
    }
}

/// The numbers a drawing needs for the sparkline at `index` of group `group`, or `None` when its data cannot
/// be found.
pub fn sparkline_data(book: &Workbook, sheet: usize, group: usize, index: usize) -> Option<SparklineData> {
    let g = book.sheets.get(sheet)?.sparklines.get(group)?;
    let sp = g.sparklines.get(index)?;
    let mut values = read_values(book, sheet, g, sp);
    if g.empty == EmptyCells::Zero {
        for v in values.iter_mut().filter(|v| v.is_none()) {
            *v = Some(0.0);
        }
    }
    let extent = |vals: &[Option<f64>]| {
        let numbers: Vec<f64> = vals.iter().flatten().copied().collect();
        let lo = numbers.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (!numbers.is_empty()).then_some((lo, hi))
    };
    let own = extent(&values)?;
    let all = || {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for other in &g.sparklines {
            if let Some((l, h)) = extent(&read_values(book, sheet, g, other)) {
                lo = lo.min(l);
                hi = hi.max(h);
            }
        }
        (lo, hi)
    };
    let (auto_lo, auto_hi) = automatic_bounds(g.kind, own.0, own.1);
    let (group_lo, group_hi) = if matches!(g.min, AxisBound::SameForAll) || matches!(g.max, AxisBound::SameForAll) {
        let (l, h) = all();
        automatic_bounds(g.kind, l, h)
    } else {
        (auto_lo, auto_hi)
    };
    let axis_min = match g.min {
        AxisBound::Automatic => auto_lo,
        AxisBound::SameForAll => group_lo,
        AxisBound::Custom(v) => v,
    };
    let axis_max = match g.max {
        AxisBound::Automatic => auto_hi,
        AxisBound::SameForAll => group_hi,
        AxisBound::Custom(v) => v,
    };
    let position_of = |target: f64| values.iter().position(|v| *v == Some(target));
    let last_of = |target: f64| values.iter().rposition(|v| *v == Some(target));
    let first = values.iter().position(Option::is_some);
    let last = values.iter().rposition(Option::is_some);
    Some(SparklineData {
        high: position_of(own.1).or_else(|| last_of(own.1)),
        low: position_of(own.0),
        negatives: values.iter().enumerate().filter(|(_, v)| v.is_some_and(|n| n < 0.0)).map(|(i, _)| i).collect(),
        values,
        axis_min,
        axis_max,
        baseline: 0.0_f64.clamp(axis_min.min(axis_max), axis_max.max(axis_min)),
        first,
        last,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXT: &str = "<extLst><ext uri=\"{05C60535-1F16-4fd2-B633-F4F36F0B64E0}\" xmlns:x14=\"http://schemas.microsoft.com/office/spreadsheetml/2009/9/main\"><x14:sparklineGroups xmlns:xm=\"http://schemas.microsoft.com/office/excel/2006/main\"><x14:sparklineGroup type=\"column\" manualMin=\"-5\" minAxisType=\"custom\" displayEmptyCellsAs=\"zero\" markers=\"1\" negative=\"1\"><x14:colorSeries rgb=\"FF376092\"/><x14:colorNegative theme=\"5\" tint=\"-0.25\"/><x14:sparklines><x14:sparkline><xm:f>Spark!B2:E2</xm:f><xm:sqref>F2</xm:sqref></x14:sparkline></x14:sparklines></x14:sparklineGroup></x14:sparklineGroups></ext><ext uri=\"{OTHER}\"/></extLst>";

    fn sheet_with_ext() -> Sheet {
        let mut s = Sheet::new("Spark");
        s.raw.elements.push(("extLst".to_owned(), EXT.to_owned()));
        extract(&mut s);
        s
    }

    #[test]
    fn a_group_is_read_out_of_the_extension_and_the_rest_stays() {
        let s = sheet_with_ext();
        let g = &s.sparklines[0];
        assert_eq!(g.kind, SparklineKind::Column);
        assert_eq!(g.min, AxisBound::Custom(-5.0));
        assert_eq!(g.empty, EmptyCells::Zero);
        assert!(g.markers && g.negative && !g.high);
        assert_eq!(g.colors.series, Some(Color::Rgb(0xFF376092)));
        assert_eq!(g.colors.negative, Some(Color::Theme(5, -0.25)));
        assert_eq!(g.sparklines, vec![Sparkline { data: "Spark!B2:E2".to_owned(), location: CellRef::parse("F2").unwrap() }]);
        assert_eq!(s.raw.elements[0].1, "<extLst><ext uri=\"{OTHER}\"/></extLst>");
    }

    #[test]
    fn writing_the_groups_back_reads_the_same() {
        let s = sheet_with_ext();
        let kept = s.raw.elements[0].1.clone();
        let mut again = Sheet::new("Spark");
        again.raw.elements.push(("extLst".to_owned(), ext_list(&s, &kept)));
        extract(&mut again);
        assert_eq!(again.sparklines, s.sparklines);
        assert_eq!(again.raw.elements[0].1, kept);
    }

    #[test]
    fn the_automatic_axis_runs_from_the_lowest_value_to_the_highest() {
        assert_eq!(automatic_bounds(SparklineKind::Line, 5.0, 9.0), (5.0, 9.0));
        assert_eq!(automatic_bounds(SparklineKind::Column, 5.0, 9.0), (5.0, 9.0));
        assert_eq!(automatic_bounds(SparklineKind::Line, 2.0, 2.0), (1.0, 2.0));
    }
}
