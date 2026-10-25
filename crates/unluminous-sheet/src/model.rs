//! The workbook as Unluminous holds it: sheets of cells, the formatting they refer to, and everything in
//! the file that is kept rather than understood.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::coord::{CellRef, Range};
use crate::formula::Formula;
use crate::styles::Styles;
use crate::value::Value;

/// One workbook.
#[derive(Debug, Clone, Default)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    pub names: Vec<DefinedName>,
    pub styles: Styles,
    pub tables: Vec<Table>,
    /// Dates count from 1904 rather than 1900, which old Mac workbooks do.
    pub date1904: bool,
    pub calc: CalcSettings,
    /// Every part of the file this version does not regenerate, by its path in the zip.
    pub kept: Kept,
    /// Which sheet is showing.
    pub active_sheet: usize,
    /// The workbook part's elements this version does not model, as XML, by local name.
    pub raw: Vec<(String, String)>,
    /// The workbook root element's namespace declarations, so a kept element that uses a prefix still
    /// has it declared.
    pub namespaces: Vec<(String, String)>,
    /// Attributes of `workbookPr` other than `date1904`, written back as they were.
    pub workbook_pr: Vec<(String, String)>,
    /// The first cell of a circular reference found by the last calculation.
    pub circular: Option<(usize, CellRef)>,
    /// The macros of an `.xlsm`, which are kept and never run.
    pub has_macros: bool,
    /// The attributes of the first `workbookView`, written back with `activeTab` updated.
    pub view_attrs: Vec<(String, String)>,
    /// The `cm` index that marks a dynamic array formula, when the file's own metadata part is kept
    /// because it holds more than that.
    pub dynamic_cm: Option<u32>,
    /// The path of the workbook part, `xl/workbook.xml` in every file Excel writes.
    pub part: String,
    /// The pivot tables and the caches of data they summarise.
    pub pivots: crate::pivot::Pivots,
}

/// The calculation settings in `calcPr`.
#[derive(Debug, Clone, PartialEq)]
pub struct CalcSettings {
    pub automatic: bool,
    /// Iterative calculation (`iterate`): a circular reference is calculated over and over until it settles,
    /// instead of being left at 0.
    pub iterate: bool,
    /// The most passes iterative calculation makes (`iterateCount`).
    pub iterate_count: u32,
    /// Iteration stops once no cell of a circular reference changes by this much (`iterateDelta`).
    pub iterate_delta: f64,
    /// False for "precision as displayed" (`fullPrecision="0"`): a number is stored as it is shown.
    pub full_precision: bool,
    /// The other `calcPr` attributes, kept.
    pub attrs: Vec<(String, String)>,
}

/// How many passes iterative calculation makes when the file does not say.
pub const DEFAULT_ITERATE_COUNT: u32 = 100;
/// The largest change that ends iteration when the file does not say.
pub const DEFAULT_ITERATE_DELTA: f64 = 0.001;

impl Default for CalcSettings {
    fn default() -> Self {
        Self {
            automatic: true,
            iterate: false,
            iterate_count: DEFAULT_ITERATE_COUNT,
            iterate_delta: DEFAULT_ITERATE_DELTA,
            full_precision: true,
            attrs: Vec::new(),
        }
    }
}

/// Parts of the file kept as bytes, with their relationships and content types.
#[derive(Debug, Clone, Default)]
pub struct Kept {
    /// Path in the zip, without a leading slash, and its bytes.
    pub parts: Vec<(String, Arc<Vec<u8>>)>,
    /// Content type overrides, by path with a leading slash.
    pub overrides: Vec<(String, String)>,
    /// Default content types, by extension.
    pub defaults: Vec<(String, String)>,
    /// The package relationships other than the workbook's (core and app properties, thumbnails).
    pub package_rels: Vec<Relationship>,
    /// The workbook's relationships other than sheets, styles, shared strings, theme and metadata.
    pub workbook_rels: Vec<Relationship>,
    /// What a binary workbook (`.xls`, `.xlsb`) holds that this version reads no further: the macro
    /// project, the pictures and charts, and the document properties, by their path in the file. They are
    /// kept so a later version can use them, and they are not written to an `.xlsx` file.
    pub legacy: Vec<(String, Arc<Vec<u8>>)>,
}

impl Kept {
    pub fn get(&self, path: &str) -> Option<&Arc<Vec<u8>>> {
        self.parts.iter().find(|(p, _)| p == path).map(|(_, bytes)| bytes)
    }

    pub fn set(&mut self, path: &str, bytes: Vec<u8>) {
        match self.parts.iter_mut().find(|(p, _)| p == path) {
            Some((_, b)) => *b = Arc::new(bytes),
            None => self.parts.push((path.to_owned(), Arc::new(bytes))),
        }
    }

    pub fn remove(&mut self, path: &str) {
        self.parts.retain(|(p, _)| p != path);
        let slashed = format!("/{path}");
        self.overrides.retain(|(p, _)| *p != slashed);
    }
}

/// One relationship in a `.rels` part.
#[derive(Debug, Clone, PartialEq)]
pub struct Relationship {
    pub id: String,
    pub kind: String,
    pub target: String,
    pub external: bool,
}

/// A name in Name Manager.
#[derive(Debug, Clone, PartialEq)]
pub struct DefinedName {
    pub name: String,
    /// The sheet it belongs to, or `None` for the whole workbook.
    pub scope: Option<usize>,
    /// What it refers to, as formula text without the `=`.
    pub formula: String,
    pub hidden: bool,
    /// Its other attributes, kept.
    pub attrs: Vec<(String, String)>,
}

/// An Excel table (`Insert -> Table`).
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub sheet: usize,
    pub name: String,
    pub range: Range,
    pub columns: Vec<String>,
    pub header_row: bool,
    pub totals_row: bool,
    /// The part it lives in, and its XML as read, which is written back with `ref` updated.
    pub part: String,
    pub xml: String,
}

impl Table {
    /// The rows that hold data: the range without its header and totals.
    pub fn data_range(&self) -> Option<Range> {
        let first = self.range.first.row + u32::from(self.header_row);
        let last = self.range.last.row.checked_sub(u32::from(self.totals_row))?;
        (first <= last).then(|| {
            Range::new(
                CellRef::new(first, self.range.first.col),
                CellRef::new(last, self.range.last.col),
            )
        })
    }
}

/// Whether a sheet's tab is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    VeryHidden,
}

/// One cell: its value, the formula that computes it, and its format.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    pub value: Value,
    pub formula: Option<Formula>,
    /// Index into [`Styles::xfs`].
    pub style: u32,
    /// Rich text runs as read, written back while the text is unchanged.
    pub rich: Option<Arc<str>>,
    /// Cell attributes this version does not model, such as `vm` for a picture in a cell, kept.
    pub extra: Option<Arc<str>>,
}

impl Cell {
    pub fn value(value: Value) -> Self {
        Self { value, ..Default::default() }
    }

    /// Nothing in it and no format: a cell that need not be stored.
    pub fn is_blank(&self) -> bool {
        self.value.is_empty() && self.formula.is_none() && self.style == 0
    }
}

/// A run of columns that share a width, a format or being hidden.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnInfo {
    pub first: u32,
    pub last: u32,
    /// In characters of the default font, as the file stores it.
    pub width: Option<f64>,
    pub hidden: bool,
    pub style: u32,
    pub custom_width: bool,
    pub outline: u8,
    pub collapsed: bool,
    pub best_fit: bool,
}

/// One row's height, format and outline.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RowInfo {
    /// In points.
    pub height: Option<f64>,
    pub hidden: bool,
    pub style: Option<u32>,
    pub outline: u8,
    pub collapsed: bool,
    /// Excel's `thickBot`, `thickTop`, `ph`, `dyDescent` and the like, kept.
    pub attrs: Vec<(String, String)>,
}

/// A note on a cell: the yellow box Excel calls a note and older versions a comment.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub author: String,
    pub text: String,
    /// Shown all the time rather than when the pointer is over the cell.
    pub visible: bool,
    /// The `comment` element's other attributes, such as the `xr:uid` a threaded comment is matched by.
    pub attrs: Vec<(String, String)>,
}

impl Note {
    pub fn new(author: impl Into<String>, text: impl Into<String>) -> Self {
        Self { author: author.into(), text: text.into(), visible: false, attrs: Vec::new() }
    }
}

/// A link on a cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Hyperlink {
    pub range: Range,
    pub target: LinkTarget,
    pub display: Option<String>,
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    /// A web address, a file or a mail address.
    External(String),
    /// A place in the workbook: `Sheet2!A1` or a name.
    Location(String),
}

/// The filter buttons on a header row.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoFilter {
    pub range: Range,
    /// Criteria by column offset within the range.
    pub columns: BTreeMap<u32, FilterCriteria>,
    /// How the filtered range was last sorted, which Excel shows in the Sort dialog and reapplies.
    pub sort: Option<SortState>,
}

/// The sort levels a filter or a table remembers (`sortState`).
#[derive(Debug, Clone, PartialEq)]
pub struct SortState {
    pub range: Range,
    pub conditions: Vec<SortCondition>,
    /// Sort left to right rather than top to bottom.
    pub columns: bool,
    pub case_sensitive: bool,
    /// Other attributes, such as the sort method, kept.
    pub attrs: Vec<(String, String)>,
}

/// One sort level: the column (or row) it sorts by and how.
#[derive(Debug, Clone, PartialEq)]
pub struct SortCondition {
    pub range: Range,
    pub descending: bool,
    pub by: SortBy,
    /// A custom list such as `Low,Medium,High`, sorting in its order.
    pub custom_list: Option<String>,
    /// The differential format whose fill or font colour is sorted on top.
    pub dxf: Option<u32>,
    pub icon_set: Option<String>,
    pub icon_id: Option<u32>,
}

/// What a sort level compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortBy {
    #[default]
    Value,
    CellColor,
    FontColor,
    Icon,
}

impl SortBy {
    pub fn name(self) -> &'static str {
        match self {
            SortBy::Value => "value",
            SortBy::CellColor => "cellColor",
            SortBy::FontColor => "fontColor",
            SortBy::Icon => "icon",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "value" | "values" => SortBy::Value,
            "cellColor" | "cell-color" | "fill" => SortBy::CellColor,
            "fontColor" | "font-color" | "color" => SortBy::FontColor,
            "icon" => SortBy::Icon,
            _ => return None,
        })
    }
}

/// What one filtered column keeps.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterCriteria {
    /// Keep the rows whose text is one of these; `blank` keeps empty rows too.
    Values { values: Vec<String>, blank: bool },
    /// One or two conditions such as `>= 10`, joined by and or or.
    Custom { conditions: Vec<(String, String)>, and: bool },
    /// Kept as read: whatever [`FilterCriteria`] has no variant for.
    Raw(String),
    /// A value list that also names items of the date tree: every date in a year, month or day.
    Dates { values: Vec<String>, groups: Vec<DateGroup>, blank: bool },
    /// The top or bottom items (or percent) of the column. `threshold` is the value the last item kept has,
    /// which Excel writes as `filterVal` and which is computed again each time the filter is applied.
    Top10 { top: bool, percent: bool, value: f64, threshold: Option<f64> },
    /// One of Excel's dynamic filters, by the name the file gives it: `today`, `thisWeek`, `nextMonth`,
    /// `yearToDate`, `Q1`, `M3`, `aboveAverage` and the rest. `value` and `max` are the serial numbers (or
    /// the average) Excel computed when the filter was applied.
    Dynamic { kind: String, value: Option<f64>, max: Option<f64> },
    /// Keep the cells filled with a colour, or whose text is a colour, which differential format `dxf` holds.
    Color { cell: bool, dxf: u32 },
    /// Keep the cells showing one icon of an icon set. `id` is -1 for the cells with no icon.
    Icon { set: String, id: i32 },
}

/// One conditional format: the cells it covers and its rules.
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalFormat {
    pub ranges: Vec<Range>,
    pub rules: Vec<CfRule>,
}

/// One rule of a conditional format.
#[derive(Debug, Clone, PartialEq)]
pub struct CfRule {
    pub kind: String,
    /// The differential format it applies, an index into [`Styles::dxfs`].
    pub dxf: Option<u32>,
    pub priority: i32,
    pub stop: bool,
    pub operator: Option<String>,
    pub text: Option<String>,
    pub rank: Option<u32>,
    pub percent: bool,
    pub bottom: bool,
    pub above_average: Option<bool>,
    pub formulas: Vec<String>,
    pub color_scale: Option<Vec<(Cfvo, crate::styles::Color)>>,
    pub data_bar: Option<(Cfvo, Cfvo, crate::styles::Color)>,
    /// How many standard deviations from the average an `aboveAverage` rule counts (1, 2 or 3).
    pub std_dev: Option<u32>,
    /// An `aboveAverage` rule that also takes the cells equal to the average.
    pub equal_average: bool,
    /// The period a `timePeriod` rule names, such as `lastWeek`.
    pub time_period: Option<String>,
    /// The icon set of an `iconSet` rule.
    pub icon_set: Option<IconSetRule>,
    /// What a data bar has besides its colour and its two thresholds.
    pub data_bar_options: Option<DataBarExt>,
    /// How the rule is stored in the `x14` extension, when it is.
    pub x14: Option<X14Payload>,
    /// Other attributes, and children this version draws nothing for, kept.
    pub attrs: Vec<(String, String)>,
    pub raw_children: String,
}

impl CfRule {
    /// A rule of a kind with nothing set, to fill in.
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            dxf: None,
            priority: 1,
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
            std_dev: None,
            equal_average: false,
            time_period: None,
            icon_set: None,
            data_bar_options: None,
            x14: None,
            attrs: Vec::new(),
            raw_children: String::new(),
        }
    }
}

/// The icon set of a rule: which set, where each icon begins, and how it is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct IconSetRule {
    /// `3Arrows`, `4Rating`, `3Stars` and the rest of Excel's twenty sets.
    pub name: String,
    /// One threshold for each icon, the first being the lowest. The flag is whether a value equal to the
    /// threshold earns the icon (`>=`) rather than only a value above it (`>`).
    pub thresholds: Vec<(Cfvo, bool)>,
    pub reverse: bool,
    pub show_value: bool,
    /// An icon chosen for each position instead of the set's own: the set it comes from and its index
    /// there, or `None` for no icon (`NoIcons`).
    pub custom: Vec<Option<(String, u32)>>,
}

impl IconSetRule {
    /// A set with the thresholds Excel gives it: percentages that split the range evenly, the thirds
    /// written as 33 and 67.
    pub fn new(name: &str) -> Self {
        let count = icon_count(name);
        let thresholds = (0..count)
            .map(|i| {
                let value = match (count, i) {
                    (3, 1) => 33,
                    (3, 2) => 67,
                    _ => i * 100 / count,
                };
                (Cfvo { kind: "percent".to_owned(), value: Some(value.to_string()) }, true)
            })
            .collect();
        Self {
            name: name.to_owned(),
            thresholds,
            reverse: false,
            show_value: true,
            custom: Vec::new(),
        }
    }

    /// Whether the set can be stored in the main part of the file; three of Excel's sets and any custom
    /// choice of icons need the extension.
    pub fn needs_extension(&self) -> bool {
        !self.custom.is_empty() || matches!(self.name.as_str(), "3Stars" | "3Triangles" | "5Boxes")
    }
}

/// How many icons an icon set has: the first digit of its name.
pub fn icon_count(name: &str) -> usize {
    name.chars().next().and_then(|c| c.to_digit(10)).map_or(3, |d| d as usize)
}

/// Everything about a data bar other than its colour and its two thresholds: the options of the 2007
/// element and of the Excel 2010 extension.
#[derive(Debug, Clone, PartialEq)]
pub struct DataBarExt {
    /// The number is shown beside the bar (`showValue`).
    pub show_value: bool,
    /// The shortest and longest bar as a percentage of the cell's width, in the 2007 element.
    pub main_min_length: u32,
    pub main_max_length: u32,
    /// The same in the extension, which Excel 2010 writes as 0 and 100.
    pub min_length: u32,
    pub max_length: u32,
    pub border: bool,
    /// A gradient fill rather than a solid one.
    pub gradient: bool,
    /// `context`, `leftToRight` or `rightToLeft`.
    pub direction: String,
    /// Negative bars use the fill and the border of the positive ones.
    pub negative_same_fill: bool,
    pub negative_same_border: bool,
    /// `automatic`, `middle` or `none`.
    pub axis_position: String,
    pub border_color: Option<crate::styles::Color>,
    pub negative_fill: Option<crate::styles::Color>,
    pub negative_border: Option<crate::styles::Color>,
    pub axis_color: Option<crate::styles::Color>,
    /// The extension is present in the file, so a rule written back gets one.
    pub has_extension: bool,
}

impl Default for DataBarExt {
    /// What Excel 2010 and later give a new data bar: a gradient fill with no border, the axis automatic.
    fn default() -> Self {
        Self {
            show_value: true,
            main_min_length: 10,
            main_max_length: 90,
            min_length: 0,
            max_length: 100,
            border: false,
            gradient: true,
            direction: "context".to_owned(),
            negative_same_fill: false,
            negative_same_border: true,
            axis_position: "automatic".to_owned(),
            border_color: None,
            negative_fill: None,
            negative_border: None,
            axis_color: None,
            has_extension: true,
        }
    }
}

/// How a rule is kept in the worksheet's `x14` extension.
#[derive(Debug, Clone, PartialEq)]
pub struct X14Payload {
    /// The GUID that pairs the extension's rule with its twin in the main part, or names a rule that has none.
    pub id: String,
    /// The rule exists only in the extension: Excel keeps there what the 2007 schema cannot say, such as
    /// the stars, triangles and boxes icon sets, custom icons, and formulas that name another sheet.
    pub only: bool,
}

/// A threshold of a colour scale or a data bar.
#[derive(Debug, Clone, PartialEq)]
pub struct Cfvo {
    /// `min`, `max`, `num`, `percent`, `percentile` or `formula`.
    pub kind: String,
    pub value: Option<String>,
}

/// One data validation rule over some cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Validation {
    pub ranges: Vec<Range>,
    /// `list`, `whole`, `decimal`, `date`, `time`, `textLength` or `custom`.
    pub kind: String,
    pub operator: Option<String>,
    pub formula1: Option<String>,
    pub formula2: Option<String>,
    pub allow_blank: bool,
    pub show_dropdown: bool,
    pub prompt_title: Option<String>,
    pub prompt: Option<String>,
    pub error_title: Option<String>,
    pub error: Option<String>,
    /// `stop`, `warning` or `information`.
    pub error_style: Option<String>,
    pub attrs: Vec<(String, String)>,
}

/// A chart drawn on a sheet, read from its drawing and chart parts.
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    /// Where it sits: the top left and bottom right cells, with offsets in EMUs.
    pub from: (CellRef, i64, i64),
    pub to: (CellRef, i64, i64),
    pub kind: ChartKind,
    pub title: Option<String>,
    pub series: Vec<ChartSeries>,
    /// The chart part's path, so a new chart knows where it was written.
    pub part: String,
    /// Made in this session rather than read: written as new drawing and chart parts.
    pub new: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Column,
    Bar,
    Line,
    Area,
    Pie,
    Doughnut,
    Scatter,
    /// A type this version does not draw, named.
    Other(&'static str),
}

impl ChartKind {
    pub fn name(&self) -> &'static str {
        match self {
            ChartKind::Column => "column",
            ChartKind::Bar => "bar",
            ChartKind::Line => "line",
            ChartKind::Area => "area",
            ChartKind::Pie => "pie",
            ChartKind::Doughnut => "doughnut",
            ChartKind::Scatter => "scatter",
            ChartKind::Other(name) => name,
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "column" => ChartKind::Column,
            "bar" => ChartKind::Bar,
            "line" => ChartKind::Line,
            "area" => ChartKind::Area,
            "pie" => ChartKind::Pie,
            "doughnut" => ChartKind::Doughnut,
            "scatter" => ChartKind::Scatter,
            _ => return None,
        })
    }
}

/// A picture placed over the cells: where it sits and the package part holding its image.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub from: (CellRef, i64, i64),
    pub to: (CellRef, i64, i64),
    /// The image part's path, such as `xl/media/image1.png`, whose bytes are in [`Kept::parts`].
    pub part: String,
}

/// One series of a chart: formulas for its name, categories and values.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSeries {
    pub name: Option<String>,
    pub categories: Option<String>,
    pub values: String,
}

/// A sheet.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    pub name: String,
    pub cells: BTreeMap<(u32, u32), Cell>,
    pub columns: Vec<ColumnInfo>,
    pub rows: BTreeMap<u32, RowInfo>,
    pub merges: Vec<Range>,
    /// Rows and columns frozen at the top and left.
    pub frozen: Option<(u32, u32)>,
    pub conditional: Vec<ConditionalFormat>,
    pub validations: Vec<Validation>,
    pub notes: BTreeMap<(u32, u32), Note>,
    /// The notes changed since the file was read, so their parts are written again.
    pub notes_changed: bool,
    pub links: Vec<Hyperlink>,
    pub filter: Option<AutoFilter>,
    pub charts: Vec<Chart>,
    /// The pictures, shapes and other objects the sheet's drawing places over the cells, bottom first.
    pub drawing: crate::objects::Drawing,
    /// The sparkline groups of the sheet.
    pub sparklines: Vec<crate::objects::sparkline::SparklineGroup>,
    pub state: Visibility,
    pub tab_color: Option<crate::styles::Color>,
    pub zoom: u32,
    pub show_grid: bool,
    /// Everything else `sheetView` says: split panes, the selection, the scroll position, headings.
    pub view: SheetView,
    /// In characters of the default font.
    pub default_col_width: Option<f64>,
    /// In points.
    pub default_row_height: f64,
    pub spills: Spills,
    pub raw: SheetRaw,
    /// How the sheet prints: page setup, margins, headers and footers, manual page breaks.
    pub page: crate::pages::PageSetup,
    /// The legacy array formulas: a range sharing one formula entered with Ctrl+Shift+Enter.
    pub array_formulas: Vec<Range>,
    /// What If data tables (`TABLE()`): the results range and its input cells.
    pub data_tables: Vec<DataTable>,
    /// A chart sheet, a dialog sheet or a macro sheet, kept as its part and shown as a tab with no grid.
    pub kept_part: Option<KeptSheet>,
    /// Where the summary rows and columns of the outline sit, and whether its symbols show.
    pub outline: OutlineSettings,
}

/// The outline settings of a sheet (`sheetPr/outlinePr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutlineSettings {
    /// The summary row sits below its detail rows rather than above them.
    pub summary_below: bool,
    /// The summary column sits to the right of its detail columns rather than the left.
    pub summary_right: bool,
    /// Automatic styles: bold and italic on the summary rows and columns.
    pub apply_styles: bool,
    /// The outline symbols (buttons and level numbers) are shown.
    pub show_symbols: bool,
}

impl Default for OutlineSettings {
    fn default() -> Self {
        Self { summary_below: true, summary_right: true, apply_styles: false, show_symbols: true }
    }
}

/// What a sheet's `sheetView` says beyond the frozen panes, the zoom and the gridlines, which [`Sheet`]
/// holds itself because the grid reads them on every frame.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetView {
    pub kind: ViewKind,
    /// The zoom of each view kind, as `zoomScaleNormal`, `zoomScalePageLayoutView` and
    /// `zoomScaleSheetLayoutView` say it.
    pub zoom_normal: Option<u32>,
    pub zoom_layout: Option<u32>,
    pub zoom_page_break: Option<u32>,
    pub show_headings: bool,
    pub show_zeros: bool,
    pub show_formulas: bool,
    pub right_to_left: bool,
    /// The first cell showing at the top left: the scroll position.
    pub top_left: Option<CellRef>,
    /// Panes split by the split bars rather than frozen.
    pub split: Option<Split>,
    /// The first cell showing in the bottom right pane of frozen panes, and which pane is active.
    pub pane_top_left: Option<CellRef>,
    pub active_pane: Option<String>,
    /// Frozen panes that were split first (`frozenSplit`).
    pub frozen_split: bool,
    pub selections: Vec<Selection>,
    /// Other `sheetView` attributes (`colorId`, `defaultGridColor`, `showRuler` and the like), kept.
    pub extra: Vec<(String, String)>,
    /// The sheet's views for other workbook windows, kept as XML.
    pub others: Vec<String>,
}

impl Default for SheetView {
    fn default() -> Self {
        Self {
            kind: ViewKind::Normal,
            zoom_normal: None,
            zoom_layout: None,
            zoom_page_break: None,
            show_headings: true,
            show_zeros: true,
            show_formulas: false,
            right_to_left: false,
            top_left: None,
            split: None,
            pane_top_left: None,
            active_pane: None,
            frozen_split: false,
            selections: Vec::new(),
            extra: Vec::new(),
            others: Vec::new(),
        }
    }
}

/// Which of Excel's three views a sheet shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewKind {
    #[default]
    Normal,
    PageBreakPreview,
    PageLayout,
}

impl ViewKind {
    pub fn name(self) -> &'static str {
        match self {
            ViewKind::Normal => "normal",
            ViewKind::PageBreakPreview => "pageBreakPreview",
            ViewKind::PageLayout => "pageLayout",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "normal" => ViewKind::Normal,
            "pageBreakPreview" | "page-break" => ViewKind::PageBreakPreview,
            "pageLayout" | "layout" => ViewKind::PageLayout,
            _ => return None,
        })
    }
}

/// Panes split by the split bars: the split's position in twentieths of a point from the top left of the
/// grid, and the first cell showing in the bottom right pane.
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    pub x: f64,
    pub y: f64,
    pub top_left: Option<CellRef>,
}

/// The cells chosen in one pane, and the active cell among them.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub pane: Option<String>,
    pub active: CellRef,
    pub ranges: Vec<Range>,
}

/// A What If data table: Excel fills `range` by putting each row and column header value into the input
/// cells and recomputing the formulas along the top row or left column.
#[derive(Debug, Clone, PartialEq)]
pub struct DataTable {
    /// The cells that hold the results, written as one `dataTable` formula on the first of them.
    pub range: Range,
    /// Two input cells, one taking the values down the left and one along the top.
    pub two_d: bool,
    /// With one input cell: the values run along a row rather than down a column.
    pub row_input: bool,
    pub r1: Option<CellRef>,
    pub r2: Option<CellRef>,
    /// Other attributes (`del1`, `del2`, `aca`), kept.
    pub attrs: Vec<(String, String)>,
}

/// A sheet that is not a worksheet: its part, its relationship type and its content type.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptSheet {
    pub part: String,
    pub kind: String,
    pub content_type: String,
}

/// What a sheet's formulas spilled: each anchor cell and the size of what it spilled.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spills {
    pub anchors: BTreeMap<(u32, u32), (u32, u32)>,
    /// The values of every cell a spill covers other than its anchor.
    pub values: BTreeMap<(u32, u32), Value>,
    /// For each covered cell, its anchor.
    pub owner: BTreeMap<(u32, u32), (u32, u32)>,
}

impl Spills {
    pub fn clear_anchor(&mut self, anchor: (u32, u32)) -> Option<Range> {
        let (rows, cols) = self.anchors.remove(&anchor)?;
        let range = Range::new(
            CellRef::new(anchor.0, anchor.1),
            CellRef::new(anchor.0 + rows - 1, anchor.1 + cols - 1),
        );
        for at in range.cells() {
            let key = (at.row, at.col);
            if self.owner.get(&key) == Some(&anchor) {
                self.owner.remove(&key);
                self.values.remove(&key);
            }
        }
        Some(range)
    }

    pub fn range_of(&self, anchor: (u32, u32)) -> Option<Range> {
        let (rows, cols) = *self.anchors.get(&anchor)?;
        Some(Range::new(
            CellRef::new(anchor.0, anchor.1),
            CellRef::new(anchor.0 + rows - 1, anchor.1 + cols - 1),
        ))
    }
}

/// A worksheet's XML this version does not model, written back in the place the schema puts it.
#[derive(Debug, Clone, Default)]
pub struct SheetRaw {
    /// Elements by local name, as XML.
    pub elements: Vec<(String, String)>,
    /// The root element's namespace declarations and `mc:Ignorable`.
    pub namespaces: Vec<(String, String)>,
    /// `sheetFormatPr` attributes other than the defaults modelled.
    pub format_attrs: Vec<(String, String)>,
    /// The sheet's relationships that are kept: drawings, tables, printer settings, pivot tables.
    pub rels: Vec<Relationship>,
    /// The part's path in the zip.
    pub part: String,
}

impl Sheet {
    /// The pictures of the drawing as the grid draws them: where each is and the part holding its image.
    pub fn pictures(&self) -> Vec<Picture> {
        use crate::objects::geometry::cell_at;
        self.drawing
            .objects
            .iter()
            .filter(|o| !o.hidden)
            .filter_map(|o| {
                let part = o.picture.as_ref()?.part.clone();
                let r = o.frame_rect(self);
                let end = cell_at(self, r.right(), r.bottom());
                let start = cell_at(self, r.x, r.y);
                Some(Picture { from: start, to: end, part })
            })
            .collect()
    }

    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            zoom: 100,
            show_grid: true,
            default_row_height: 14.25,
            ..Default::default()
        }
    }

    pub fn cell(&self, at: CellRef) -> Option<&Cell> {
        self.cells.get(&(at.row, at.col))
    }

    /// What a reader sees at a place: the cell's value, or a value a spill put there.
    pub fn value_at(&self, at: CellRef) -> Value {
        match self.cells.get(&(at.row, at.col)) {
            Some(cell) => cell.value.clone(),
            None => self.spills.values.get(&(at.row, at.col)).cloned().unwrap_or(Value::Empty),
        }
    }

    /// Put a cell, removing it when it holds nothing.
    pub fn set(&mut self, at: CellRef, cell: Cell) {
        if cell.is_blank() {
            self.cells.remove(&(at.row, at.col));
        } else {
            self.cells.insert((at.row, at.col), cell);
        }
    }

    /// The rectangle from A1 to the last row and column holding anything.
    pub fn used_range(&self) -> Option<Range> {
        let mut last_row = None;
        let mut last_col = 0;
        let mut first_row = u32::MAX;
        let mut first_col = u32::MAX;
        for (&(row, col), cell) in &self.cells {
            if cell.value.is_empty() && cell.formula.is_none() {
                continue;
            }
            last_row = Some(last_row.map_or(row, |r: u32| r.max(row)));
            last_col = last_col.max(col);
            first_row = first_row.min(row);
            first_col = first_col.min(col);
        }
        for &(row, col) in self.spills.values.keys() {
            last_row = Some(last_row.map_or(row, |r: u32| r.max(row)));
            last_col = last_col.max(col);
            first_row = first_row.min(row);
            first_col = first_col.min(col);
        }
        last_row
            .map(|row| Range::new(CellRef::new(first_row, first_col), CellRef::new(row, last_col)))
    }

    /// The column info covering a column, if any.
    pub fn column(&self, col: u32) -> Option<&ColumnInfo> {
        self.columns.iter().find(|info| info.first <= col && col <= info.last)
    }

    /// The merge a cell is part of.
    pub fn merge_at(&self, at: CellRef) -> Option<Range> {
        self.merges.iter().copied().find(|range| range.contains(at))
    }

    pub fn is_row_hidden(&self, row: u32) -> bool {
        self.rows.get(&row).is_some_and(|info| info.hidden)
    }

    pub fn is_column_hidden(&self, col: u32) -> bool {
        self.column(col).is_some_and(|info| info.hidden)
    }

    /// Set something about one column, splitting the run it is in.
    pub fn edit_column(&mut self, col: u32, change: impl FnOnce(&mut ColumnInfo)) {
        let mut infos: Vec<ColumnInfo> = Vec::new();
        let mut found = false;
        for info in self.columns.drain(..) {
            if info.first <= col && col <= info.last {
                found = true;
                if info.first < col {
                    infos.push(ColumnInfo { last: col - 1, ..info.clone() });
                }
                infos.push(ColumnInfo { first: col, last: col, ..info.clone() });
                if col < info.last {
                    infos.push(ColumnInfo { first: col + 1, ..info });
                }
            } else {
                infos.push(info);
            }
        }
        if !found {
            infos.push(ColumnInfo {
                first: col,
                last: col,
                width: None,
                hidden: false,
                style: 0,
                custom_width: false,
                outline: 0,
                collapsed: false,
                best_fit: false,
            });
        }
        infos.sort_by_key(|info| info.first);
        if let Some(target) = infos.iter_mut().find(|info| info.first == col && info.last == col) {
            change(target);
        }
        infos.retain(|info| {
            info.width.is_some()
                || info.hidden
                || info.style != 0
                || info.outline != 0
                || info.collapsed
        });
        self.columns = infos;
    }
}

impl Workbook {
    /// A new workbook with one empty sheet, the way Excel's `Book1` starts.
    pub fn new() -> Self {
        Self {
            sheets: vec![Sheet::new("Sheet1")],
            styles: Styles::default_workbook(),
            ..Default::default()
        }
    }

    /// A number as a cell with this style stores it under precision as displayed: rounded to what its
    /// number format shows.
    pub fn shown_number(&self, style: u32, n: f64, date1904: bool) -> f64 {
        let code = self.styles.format_code(style);
        if code == "General" {
            return n;
        }
        crate::numfmt::Format::parse(&code).shown_value(n, date1904)
    }

    pub fn sheet_index(&self, name: &str) -> Option<usize> {
        self.sheets.iter().position(|sheet| sheet.name.eq_ignore_ascii_case(name))
    }

    /// The name a new sheet gets: `Sheet2`, `Sheet3` and so on, skipping names in use.
    pub fn next_sheet_name(&self) -> String {
        (1..)
            .map(|n| format!("Sheet{n}"))
            .find(|name| self.sheet_index(name).is_none())
            .unwrap_or_default()
    }

    /// The defined name visible from `sheet`, preferring the sheet's own.
    pub fn name(&self, name: &str, sheet: usize) -> Option<&DefinedName> {
        self.names
            .iter()
            .find(|n| n.scope == Some(sheet) && n.name.eq_ignore_ascii_case(name))
            .or_else(|| {
                self.names.iter().find(|n| n.scope.is_none() && n.name.eq_ignore_ascii_case(name))
            })
    }

    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|table| table.name.eq_ignore_ascii_case(name))
    }

    /// The table a cell is inside.
    pub fn table_at(&self, sheet: usize, at: CellRef) -> Option<&Table> {
        self.tables.iter().find(|table| table.sheet == sheet && table.range.contains(at))
    }
}

/// One item of the year, month, day tree a filter on a date column offers (`dateGroupItem`): every date in
/// the year, the month of the year or the day, according to `grouping`.
#[derive(Debug, Clone, PartialEq)]
pub struct DateGroup {
    pub year: i32,
    pub month: Option<u32>,
    pub day: Option<u32>,
    pub hour: Option<u32>,
    pub minute: Option<u32>,
    pub second: Option<u32>,
    pub grouping: DateGrouping,
}

/// How much of a date a [`DateGroup`] fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateGrouping {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

impl DateGrouping {
    /// The name the file gives it (`dateTimeGrouping`).
    pub fn name(self) -> &'static str {
        match self {
            DateGrouping::Year => "year",
            DateGrouping::Month => "month",
            DateGrouping::Day => "day",
            DateGrouping::Hour => "hour",
            DateGrouping::Minute => "minute",
            DateGrouping::Second => "second",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "year" => DateGrouping::Year,
            "month" => DateGrouping::Month,
            "day" => DateGrouping::Day,
            "hour" => DateGrouping::Hour,
            "minute" => DateGrouping::Minute,
            "second" => DateGrouping::Second,
            _ => return None,
        })
    }
}
