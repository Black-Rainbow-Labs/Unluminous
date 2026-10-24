//! Pivot tables: the cache of the source data, the table definition, the calculation that fills it and
//! the layout that writes it into the sheet, the way Excel does.
//!
//! - [`cache`] is `pivotCacheDefinition` and `pivotCacheRecords`: the fields of the source with their
//!   shared items, and a copy of every record. A pivot never reads the sheet it summarises; refreshing
//!   reads it again.
//! - [`table`] is `pivotTableDefinition`: which fields are on rows, columns and filters, the value fields
//!   with their functions and Show Values As settings, and how each field is laid out.
//! - [`group`] groups dates by period, numbers by range and items by hand.
//! - [`acc`] and [`compute`] add up the records into the cells of the pivot, and [`show`] applies Show
//!   Values As to them.
//! - [`layout`] turns those cells into the rectangle of the sheet the pivot occupies.
//! - [`xml`] reads and writes the parts, [`session`] is every change a person makes, and [`items`] is the
//!   description of the lines and columns a layout produced, which is also what `rowItems` and `colItems`
//!   are written from.
//!
//! Excel is the oracle for all of it: `tests/pivot.rs` compares the cells of every case with the cells
//! Excel laid out for the same settings (`tools/excel-oracle/pivot-check.ps1`).

pub mod acc;
pub mod cache;
pub mod calcfield;
pub mod compute;
pub mod group;
pub mod groups;
pub mod items;
pub mod layout;
pub mod query;
pub mod session;
pub mod session_calc;
pub mod session_group;
pub mod show;
pub mod table;
pub mod xml;
pub mod xml_read;
pub mod xml_write;

pub use cache::{CacheField, CacheSource, CacheValue, PivotCache};
pub use table::{DataField, PivotField, PivotTable};

/// Every pivot cache and pivot table of a workbook.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pivots {
    pub caches: Vec<PivotCache>,
    pub tables: Vec<PivotTable>,
    /// Parts of tables and caches that were deleted, to be taken out of the file.
    pub removed_parts: Vec<String>,
}

impl Pivots {
    /// The tables on one sheet.
    pub fn on_sheet(&self, sheet: usize) -> impl Iterator<Item = (usize, &PivotTable)> {
        self.tables.iter().enumerate().filter(move |(_, t)| t.sheet == sheet)
    }

    /// The table with this name, which Excel keeps unique in a workbook.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.tables.iter().position(|t| t.name.eq_ignore_ascii_case(name))
    }

    /// The index of the pivot table whose cells include this one.
    pub fn at(&self, sheet: usize, at: crate::coord::CellRef) -> Option<usize> {
        self.tables.iter().position(|t| t.sheet == sheet && t.occupies(at))
    }
}

/// How a value field summarises the records of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Func {
    Sum,
    /// Excel's Count: every record with something in the field.
    Count,
    Average,
    Max,
    Min,
    Product,
    /// Excel's Count Numbers: the records whose value is a number.
    CountNums,
    StdDev,
    StdDevP,
    Var,
    VarP,
}

impl Func {
    pub const ALL: [Func; 11] = [
        Func::Sum,
        Func::Count,
        Func::Average,
        Func::Max,
        Func::Min,
        Func::Product,
        Func::CountNums,
        Func::StdDev,
        Func::StdDevP,
        Func::Var,
        Func::VarP,
    ];

    /// The `subtotal` attribute of a `dataField`: `sum`, `count`, `average` and the rest. Excel's own Count
    /// is `count` there and Count Numbers is `countNums`.
    pub fn xml(&self) -> &'static str {
        match self {
            Func::Sum => "sum",
            Func::Count => "count",
            Func::Average => "average",
            Func::Max => "max",
            Func::Min => "min",
            Func::Product => "product",
            Func::CountNums => "countNums",
            Func::StdDev => "stdDev",
            Func::StdDevP => "stdDevp",
            Func::Var => "var",
            Func::VarP => "varp",
        }
    }

    pub fn from_xml(text: &str) -> Option<Func> {
        Func::ALL.into_iter().find(|f| f.xml() == text)
    }

    /// The word in a default value field name: `Sum of Units`, `StdDev of Units`.
    pub fn label(&self) -> &'static str {
        match self {
            Func::Sum => "Sum",
            Func::Count | Func::CountNums => "Count",
            Func::Average => "Average",
            Func::Max => "Max",
            Func::Min => "Min",
            Func::Product => "Product",
            Func::StdDev => "StdDev",
            Func::StdDevP => "StdDevp",
            Func::Var => "Var",
            Func::VarP => "Varp",
        }
    }

    /// The word in a subtotal's label: `East Sum`, `East Count Nums`, `East StdDev`.
    pub fn subtotal_label(&self) -> &'static str {
        match self {
            Func::CountNums => "Count Nums",
            other => other.label(),
        }
    }

    /// The `t` of an item or a row item that is this function's subtotal.
    pub fn item_type(&self) -> &'static str {
        match self {
            Func::Sum => "sum",
            Func::Count => "countA",
            Func::Average => "avg",
            Func::Max => "max",
            Func::Min => "min",
            Func::Product => "product",
            Func::CountNums => "count",
            Func::StdDev => "stdDev",
            Func::StdDevP => "stdDevP",
            Func::Var => "var",
            Func::VarP => "varP",
        }
    }

    pub fn from_item_type(text: &str) -> Option<Func> {
        Func::ALL.into_iter().find(|f| f.item_type() == text)
    }

    /// The `...Subtotal` attribute of a `pivotField`.
    pub fn subtotal_attr(&self) -> &'static str {
        match self {
            Func::Sum => "sumSubtotal",
            Func::Count => "countASubtotal",
            Func::Average => "avgSubtotal",
            Func::Max => "maxSubtotal",
            Func::Min => "minSubtotal",
            Func::Product => "productSubtotal",
            Func::CountNums => "countSubtotal",
            Func::StdDev => "stdDevSubtotal",
            Func::StdDevP => "stdDevPSubtotal",
            Func::Var => "varSubtotal",
            Func::VarP => "varPSubtotal",
        }
    }
}

/// What a value field shows in place of the summary: Show Values As.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowAs {
    Normal,
    Difference,
    Percent,
    PercentDiff,
    RunTotal,
    PercentOfRow,
    PercentOfCol,
    PercentOfTotal,
    Index,
    PercentOfParentRow,
    PercentOfParentCol,
    PercentOfParent,
    PercentRunTotal,
    RankAscending,
    RankDescending,
}

impl ShowAs {
    pub const ALL: [ShowAs; 15] = [
        ShowAs::Normal,
        ShowAs::Difference,
        ShowAs::Percent,
        ShowAs::PercentDiff,
        ShowAs::RunTotal,
        ShowAs::PercentOfRow,
        ShowAs::PercentOfCol,
        ShowAs::PercentOfTotal,
        ShowAs::Index,
        ShowAs::PercentOfParentRow,
        ShowAs::PercentOfParentCol,
        ShowAs::PercentOfParent,
        ShowAs::PercentRunTotal,
        ShowAs::RankAscending,
        ShowAs::RankDescending,
    ];

    /// The `showDataAs` attribute, for the settings the file format has always had.
    pub fn xml(&self) -> &'static str {
        match self {
            ShowAs::Normal => "normal",
            ShowAs::Difference => "difference",
            ShowAs::Percent => "percent",
            ShowAs::PercentDiff => "percentDiff",
            ShowAs::RunTotal => "runTotal",
            ShowAs::PercentOfRow => "percentOfRow",
            ShowAs::PercentOfCol => "percentOfCol",
            ShowAs::PercentOfTotal => "percentOfTotal",
            ShowAs::Index => "index",
            ShowAs::PercentOfParentRow => "percentOfParentRow",
            ShowAs::PercentOfParentCol => "percentOfParentCol",
            ShowAs::PercentOfParent => "percentOfParent",
            ShowAs::PercentRunTotal => "percentOfRunningTotal",
            ShowAs::RankAscending => "rankAscending",
            ShowAs::RankDescending => "rankDescending",
        }
    }

    pub fn from_xml(text: &str) -> Option<ShowAs> {
        ShowAs::ALL.into_iter().find(|s| s.xml() == text)
    }

    /// Whether the setting is written in the extension of the `dataField` rather than `showDataAs`.
    pub fn is_extension(&self) -> bool {
        matches!(
            self,
            ShowAs::PercentOfParentRow
                | ShowAs::PercentOfParentCol
                | ShowAs::PercentOfParent
                | ShowAs::PercentRunTotal
                | ShowAs::RankAscending
                | ShowAs::RankDescending
        )
    }

    /// Whether the setting compares a cell with the cells of a base field.
    pub fn needs_base_field(&self) -> bool {
        matches!(
            self,
            ShowAs::Difference
                | ShowAs::Percent
                | ShowAs::PercentDiff
                | ShowAs::RunTotal
                | ShowAs::PercentOfParent
                | ShowAs::PercentRunTotal
                | ShowAs::RankAscending
                | ShowAs::RankDescending
        )
    }

    /// Whether the setting also needs a base item.
    pub fn needs_base_item(&self) -> bool {
        matches!(self, ShowAs::Difference | ShowAs::Percent | ShowAs::PercentDiff)
    }

    /// The number format Excel gives a value field when this setting is chosen, if it changes it.
    pub fn default_format(&self) -> Option<u32> {
        match self {
            ShowAs::Percent
            | ShowAs::PercentDiff
            | ShowAs::PercentOfRow
            | ShowAs::PercentOfCol
            | ShowAs::PercentOfTotal
            | ShowAs::PercentOfParentRow
            | ShowAs::PercentOfParentCol
            | ShowAs::PercentOfParent
            | ShowAs::PercentRunTotal => Some(10),
            _ => None,
        }
    }
}

/// How a field's items are laid out down the row area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Form {
    #[default]
    Compact,
    Outline,
    Tabular,
}

/// The order a field's items are shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortType {
    /// The order of the item list, which a person can change by dragging.
    #[default]
    Manual,
    Ascending,
    Descending,
}

/// Where the base item of Show Values As is, when it is not a named item.
pub const BASE_PREVIOUS: u32 = 1_048_828;
pub const BASE_NEXT: u32 = 1_048_829;
