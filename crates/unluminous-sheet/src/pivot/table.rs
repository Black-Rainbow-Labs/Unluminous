//! The pivot table definition: what is on the rows, the columns and the filters, what the value fields
//! add up, and how each field is laid out.

use super::cache::PivotCache;
use super::{Form, Func, ShowAs, SortType};
use crate::coord::{CellRef, Range};

/// One entry of the row or the column area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisEntry {
    /// A field of the cache.
    Field(usize),
    /// The place of the value fields, which Excel shows as the pseudo field Values.
    Values,
}

/// One item of a pivot field.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemEntry {
    /// Which item of the cache field this is: an index into its shared or group items.
    pub x: usize,
    pub hidden: bool,
    /// A name the person gave the item.
    pub name: Option<String>,
    /// A calculated item.
    pub calculated: bool,
}

impl ItemEntry {
    pub fn new(x: usize) -> ItemEntry {
        ItemEntry { x, hidden: false, name: None, calculated: false }
    }
}

/// The subtotals a field shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Subtotals {
    /// The Automatic subtotal, which summarises each value field the way the value field does.
    pub automatic: bool,
    /// The functions ticked instead of, or as well as, Automatic.
    pub funcs: Vec<Func>,
}

impl Default for Subtotals {
    fn default() -> Self {
        Subtotals { automatic: true, funcs: Vec::new() }
    }
}

impl Subtotals {
    pub fn none() -> Subtotals {
        Subtotals { automatic: false, funcs: Vec::new() }
    }

    pub fn is_on(&self) -> bool {
        self.automatic || !self.funcs.is_empty()
    }

    /// Only the Automatic subtotal, which can sit on the item's own row.
    pub fn is_plain(&self) -> bool {
        self.automatic && self.funcs.is_empty()
    }
}

/// Sorting a field by the figures of a value field.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoSort {
    /// An index into the table's value fields.
    pub data: usize,
}

/// What a filter on a field asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    CaptionEqual,
    CaptionNotEqual,
    CaptionBeginsWith,
    CaptionNotBeginsWith,
    CaptionEndsWith,
    CaptionNotEndsWith,
    CaptionContains,
    CaptionNotContains,
    CaptionGreaterThan,
    CaptionGreaterThanOrEqual,
    CaptionLessThan,
    CaptionLessThanOrEqual,
    CaptionBetween,
    CaptionNotBetween,
    ValueEqual,
    ValueNotEqual,
    ValueGreaterThan,
    ValueGreaterThanOrEqual,
    ValueLessThan,
    ValueLessThanOrEqual,
    ValueBetween,
    ValueNotBetween,
    /// Top or Bottom N items.
    Count,
    /// Top or Bottom N percent.
    Percent,
    /// Top or Bottom items adding up to N.
    Sum,
}

impl FilterKind {
    pub const ALL: [FilterKind; 25] = [
        FilterKind::CaptionEqual,
        FilterKind::CaptionNotEqual,
        FilterKind::CaptionBeginsWith,
        FilterKind::CaptionNotBeginsWith,
        FilterKind::CaptionEndsWith,
        FilterKind::CaptionNotEndsWith,
        FilterKind::CaptionContains,
        FilterKind::CaptionNotContains,
        FilterKind::CaptionGreaterThan,
        FilterKind::CaptionGreaterThanOrEqual,
        FilterKind::CaptionLessThan,
        FilterKind::CaptionLessThanOrEqual,
        FilterKind::CaptionBetween,
        FilterKind::CaptionNotBetween,
        FilterKind::ValueEqual,
        FilterKind::ValueNotEqual,
        FilterKind::ValueGreaterThan,
        FilterKind::ValueGreaterThanOrEqual,
        FilterKind::ValueLessThan,
        FilterKind::ValueLessThanOrEqual,
        FilterKind::ValueBetween,
        FilterKind::ValueNotBetween,
        FilterKind::Count,
        FilterKind::Percent,
        FilterKind::Sum,
    ];

    pub fn xml(&self) -> &'static str {
        match self {
            FilterKind::CaptionEqual => "captionEqual",
            FilterKind::CaptionNotEqual => "captionNotEqual",
            FilterKind::CaptionBeginsWith => "captionBeginsWith",
            FilterKind::CaptionNotBeginsWith => "captionNotBeginsWith",
            FilterKind::CaptionEndsWith => "captionEndsWith",
            FilterKind::CaptionNotEndsWith => "captionNotEndsWith",
            FilterKind::CaptionContains => "captionContains",
            FilterKind::CaptionNotContains => "captionNotContains",
            FilterKind::CaptionGreaterThan => "captionGreaterThan",
            FilterKind::CaptionGreaterThanOrEqual => "captionGreaterThanOrEqual",
            FilterKind::CaptionLessThan => "captionLessThan",
            FilterKind::CaptionLessThanOrEqual => "captionLessThanOrEqual",
            FilterKind::CaptionBetween => "captionBetween",
            FilterKind::CaptionNotBetween => "captionNotBetween",
            FilterKind::ValueEqual => "valueEqual",
            FilterKind::ValueNotEqual => "valueNotEqual",
            FilterKind::ValueGreaterThan => "valueGreaterThan",
            FilterKind::ValueGreaterThanOrEqual => "valueGreaterThanOrEqual",
            FilterKind::ValueLessThan => "valueLessThan",
            FilterKind::ValueLessThanOrEqual => "valueLessThanOrEqual",
            FilterKind::ValueBetween => "valueBetween",
            FilterKind::ValueNotBetween => "valueNotBetween",
            FilterKind::Count => "count",
            FilterKind::Percent => "percent",
            FilterKind::Sum => "sum",
        }
    }

    pub fn from_xml(text: &str) -> Option<FilterKind> {
        FilterKind::ALL.into_iter().find(|k| k.xml() == text)
    }

    /// Whether the filter looks at the figures of a value field.
    pub fn is_value(&self) -> bool {
        !matches!(
            self,
            FilterKind::CaptionEqual
                | FilterKind::CaptionNotEqual
                | FilterKind::CaptionBeginsWith
                | FilterKind::CaptionNotBeginsWith
                | FilterKind::CaptionEndsWith
                | FilterKind::CaptionNotEndsWith
                | FilterKind::CaptionContains
                | FilterKind::CaptionNotContains
                | FilterKind::CaptionGreaterThan
                | FilterKind::CaptionGreaterThanOrEqual
                | FilterKind::CaptionLessThan
                | FilterKind::CaptionLessThanOrEqual
                | FilterKind::CaptionBetween
                | FilterKind::CaptionNotBetween
        )
    }

    pub fn is_top(&self) -> bool {
        matches!(self, FilterKind::Count | FilterKind::Percent | FilterKind::Sum)
    }
}

/// A label, value or top N filter on a field.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub kind: FilterKind,
    /// For a value or top N filter, the value field it looks at.
    pub data: Option<usize>,
    /// The text or number the filter compares with, and the upper limit of Between.
    pub value1: String,
    pub value2: Option<String>,
    /// A top N filter keeps the largest (true) or the smallest (false).
    pub top: bool,
    /// The number of the filter in the `filters` element.
    pub id: u32,
}

/// One field of the source as the table uses it.
#[derive(Debug, Clone, PartialEq)]
pub struct PivotField {
    /// A name the person gave the field.
    pub name: Option<String>,
    /// The items, in the order they are shown. Empty until the field is put on rows, columns or filters.
    pub items: Vec<ItemEntry>,
    pub sort: SortType,
    pub auto_sort: Option<AutoSort>,
    pub subtotals: Subtotals,
    pub form: Form,
    /// Subtotals sit on the item's own row (true) or after its group.
    pub subtotal_top: bool,
    pub repeat_labels: bool,
    pub blank_row: bool,
    pub show_all: bool,
    pub num_fmt: Option<String>,
    pub multi_page: bool,
    pub filters: Vec<Filter>,
    /// The field was in the table with these attributes this version does not model.
    pub attrs: Vec<(String, String)>,
}

impl PivotField {
    pub fn new(form: Form) -> PivotField {
        PivotField {
            name: None,
            items: Vec::new(),
            sort: SortType::Manual,
            auto_sort: None,
            subtotals: Subtotals::default(),
            form,
            subtotal_top: true,
            repeat_labels: false,
            blank_row: false,
            show_all: false,
            num_fmt: None,
            multi_page: false,
            filters: Vec::new(),
            attrs: Vec::new(),
        }
    }

    /// The position of the item made from this shared or group item.
    pub fn position_of(&self, x: usize) -> Option<usize> {
        self.items.iter().position(|i| i.x == x)
    }
}

/// A field in the filter area.
#[derive(Debug, Clone, PartialEq)]
pub struct PageField {
    pub field: usize,
    /// The shown item, as a position in the field's items. `None` is (All), or (Multiple Items) when some
    /// items are hidden.
    pub item: Option<usize>,
}

/// A value field.
#[derive(Debug, Clone, PartialEq)]
pub struct DataField {
    pub name: String,
    pub field: usize,
    pub func: Func,
    pub show: ShowAs,
    pub base_field: usize,
    /// A position among the base field's items, or [`super::BASE_PREVIOUS`] and [`super::BASE_NEXT`].
    pub base_item: u32,
    pub num_fmt: Option<String>,
}

impl DataField {
    pub fn new(name: &str, field: usize, func: Func) -> DataField {
        DataField {
            name: name.to_owned(),
            field,
            func,
            show: ShowAs::Normal,
            base_field: 0,
            base_item: 0,
            num_fmt: None,
        }
    }
}

/// Settings of the table as a whole.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// The layout new fields get.
    pub form: Form,
    /// A grand total row at the bottom.
    pub grand_total_row: bool,
    /// A grand total column at the right.
    pub grand_total_col: bool,
    pub data_caption: String,
    pub grand_total_caption: Option<String>,
    pub row_header_caption: Option<String>,
    pub col_header_caption: Option<String>,
    /// What a cell with an error shows, when the person chose something.
    pub error_caption: Option<String>,
    /// What an empty cell shows, when the person chose something.
    pub missing_caption: Option<String>,
    pub show_headers: bool,
    pub page_over_then_down: bool,
    pub page_wrap: u32,
    /// The attributes of the definition this version does not model, kept.
    pub attrs: Vec<(String, String)>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            form: Form::Compact,
            grand_total_row: true,
            grand_total_col: true,
            data_caption: "Values".to_owned(),
            grand_total_caption: None,
            row_header_caption: None,
            col_header_caption: None,
            error_caption: None,
            missing_caption: None,
            show_headers: true,
            page_over_then_down: false,
            page_wrap: 0,
            attrs: [
                ("applyNumberFormats", "0"),
                ("applyBorderFormats", "0"),
                ("applyFontFormats", "0"),
                ("applyPatternFormats", "0"),
                ("applyAlignmentFormats", "0"),
                ("applyWidthHeightFormats", "1"),
                ("updatedVersion", "8"),
                ("minRefreshableVersion", "3"),
                ("useAutoFormatting", "1"),
                ("itemPrintTitles", "1"),
                ("createdVersion", "8"),
                ("indent", "0"),
                ("multipleFieldFilters", "0"),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        }
    }
}

/// `pivotTableStyleInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleInfo {
    pub name: Option<String>,
    pub show_row_headers: bool,
    pub show_col_headers: bool,
    pub show_row_stripes: bool,
    pub show_col_stripes: bool,
    pub show_last_column: bool,
}

impl Default for StyleInfo {
    fn default() -> Self {
        StyleInfo {
            name: Some("PivotStyleLight16".to_owned()),
            show_row_headers: true,
            show_col_headers: true,
            show_row_stripes: false,
            show_col_stripes: false,
            show_last_column: true,
        }
    }
}

/// Parts of the definition this version keeps as XML and writes back while the fields stay as they were.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeptXml {
    pub formats: Option<String>,
    pub conditional_formats: Option<String>,
    pub chart_formats: Option<String>,
    pub ext_lst: Option<String>,
}

/// The pivot table.
#[derive(Debug, Clone, PartialEq)]
pub struct PivotTable {
    pub name: String,
    pub sheet: usize,
    pub cache: usize,
    pub part: String,
    /// The cells of the labels and the figures, without the filter area above them.
    pub location: Range,
    pub first_header_row: u32,
    pub first_data_row: u32,
    pub first_data_col: u32,
    pub row_page_count: u32,
    pub col_page_count: u32,
    /// One entry for each field of the cache.
    pub fields: Vec<PivotField>,
    pub rows: Vec<AxisEntry>,
    pub cols: Vec<AxisEntry>,
    pub pages: Vec<PageField>,
    pub data: Vec<DataField>,
    pub options: Options,
    pub style: StyleInfo,
    pub kept: KeptXml,
    /// The lines and columns of the last layout, which `rowItems` and `colItems` are written from.
    pub lines: super::items::Lines,
    /// Changed since it was read, so its parts are written again.
    pub dirty: bool,
    /// Whether the sheet holds the cells of a layout (an empty table has none).
    pub has_cells: bool,
}

impl PivotTable {
    /// A table over a cache with nothing on it yet, placed at a cell.
    pub fn new(name: &str, sheet: usize, cache: usize, at: CellRef, cache_ref: &PivotCache) -> PivotTable {
        let form = Form::Compact;
        PivotTable {
            name: name.to_owned(),
            sheet,
            cache,
            part: String::new(),
            location: Range::new(at, at),
            first_header_row: 1,
            first_data_row: 1,
            first_data_col: 1,
            row_page_count: 0,
            col_page_count: 0,
            fields: (0..cache_ref.fields.len()).map(|_| PivotField::new(form)).collect(),
            rows: Vec::new(),
            cols: Vec::new(),
            pages: Vec::new(),
            data: Vec::new(),
            options: Options::default(),
            style: StyleInfo::default(),
            kept: KeptXml::default(),
            lines: super::items::Lines::default(),
            dirty: true,
            has_cells: false,
        }
    }

    /// Whether a cell is one of the pivot's, the filter area included.
    pub fn occupies(&self, at: CellRef) -> bool {
        self.full_range().contains(at)
    }

    /// The range with the filter fields above the table.
    pub fn full_range(&self) -> Range {
        if self.pages.is_empty() {
            return self.location;
        }
        let rows = self.pages.len() as u32 + 1;
        let first = self.location.first.row.saturating_sub(rows);
        let last_col = self.location.last.col.max(self.location.first.col + 1);
        Range::new(
            CellRef::new(first, self.location.first.col),
            CellRef::new(self.location.last.row, last_col),
        )
    }

    /// The name the person sees for a field.
    pub fn caption(&self, cache: &PivotCache, field: usize) -> String {
        self.fields[field].name.clone().unwrap_or_else(|| cache.fields[field].name.clone())
    }

    /// Whether the Values entry is in the row area (otherwise it is in the column area when there are two
    /// value fields).
    pub fn values_on_rows(&self) -> bool {
        self.rows.contains(&AxisEntry::Values)
    }

    /// Where the Values entry is among the entries of its area, when it is not the last one.
    pub fn values_position(&self) -> Option<usize> {
        let axis = if self.values_on_rows() { self.row_axis() } else { self.col_axis() };
        let at = axis.iter().position(|e| *e == AxisEntry::Values)?;
        (at + 1 != axis.len()).then_some(at)
    }

    /// The row area, with the Values entry put in when there are two value fields and none is there.
    pub fn row_axis(&self) -> Vec<AxisEntry> {
        self.axis_with_values(&self.rows, self.values_on_rows())
    }

    pub fn col_axis(&self) -> Vec<AxisEntry> {
        self.axis_with_values(&self.cols, !self.values_on_rows())
    }

    fn axis_with_values(&self, axis: &[AxisEntry], wanted_here: bool) -> Vec<AxisEntry> {
        let mut out: Vec<AxisEntry> = axis.to_vec();
        let has = out.contains(&AxisEntry::Values);
        if self.data.len() >= 2 {
            if wanted_here && !has {
                out.push(AxisEntry::Values);
            }
        } else {
            out.retain(|e| *e != AxisEntry::Values);
        }
        out
    }

    /// The fields on any axis or in the filter area.
    pub fn used_fields(&self) -> Vec<usize> {
        let mut used: Vec<usize> = Vec::new();
        for e in self.rows.iter().chain(self.cols.iter()) {
            if let AxisEntry::Field(f) = e {
                used.push(*f);
            }
        }
        used.extend(self.pages.iter().map(|p| p.field));
        used
    }

    /// Whether a field is a value field of this table.
    pub fn is_data_field(&self, field: usize) -> bool {
        self.data.iter().any(|d| d.field == field)
    }

    /// The default name of a value field: `Sum of Units`, made unique among the table's fields.
    pub fn default_data_name(&self, cache: &PivotCache, field: usize, func: Func) -> String {
        let base = format!("{} of {}", func.label(), self.caption(cache, field));
        let taken = |n: &str| {
            self.data.iter().any(|d| d.name.eq_ignore_ascii_case(n))
                || (0..cache.fields.len()).any(|f| self.caption(cache, f).eq_ignore_ascii_case(n))
        };
        if !taken(&base) {
            return base;
        }
        (2..).map(|n| format!("{base}{n}")).find(|n| !taken(n)).unwrap_or(base)
    }
}
