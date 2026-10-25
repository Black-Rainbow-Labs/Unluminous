//! Slicers and timelines (`task-2360`): the buttons and the date bar that filter a pivot table.
//!
//! A slicer is three things in a file: a `slicerCacheDefinition` part (which field of which pivot table it
//! filters and which items are selected), a `slicers` part on a sheet (the slicer's name and caption) and a
//! graphic frame in the sheet's drawing (where it sits). The model holds the first two; the third is an
//! ordinary [`crate::objects::DrawingObject`], so a slicer follows an insert, a delete and a move as any
//! object does, and a slicer lives only while that object does. A timeline is the same with a
//! `timelineCacheDefinition` and a `timelines` part.
//!
//! **The pivot table is the truth for what is selected.** An item is selected when the pivot table shows it,
//! so a slicer cannot disagree with the table after the table's items are changed by hand or refreshed.
//! The cache's `<i s="1"/>` marks are worked out from the table when the file is written.
//!
//! - [`xml`] reads and writes the four kinds of part.
//! - [`sync`] writes the parts, relationships, extension entries and defined names into the workbook.
//! - [`session`] is the editing: make, select, rename and delete.

pub mod session;
pub mod sync;
pub mod xml;

pub const REL_SLICER_CACHE: &str = "http://schemas.microsoft.com/office/2007/relationships/slicerCache";
pub const REL_SLICER: &str = "http://schemas.microsoft.com/office/2007/relationships/slicer";
pub const REL_TIMELINE_CACHE: &str = "http://schemas.microsoft.com/office/2011/relationships/timelineCache";
pub const REL_TIMELINE: &str = "http://schemas.microsoft.com/office/2011/relationships/timeline";
pub const CT_SLICER_CACHE: &str = "application/vnd.ms-excel.slicerCache+xml";
pub const CT_SLICER: &str = "application/vnd.ms-excel.slicer+xml";
pub const CT_TIMELINE_CACHE: &str = "application/vnd.ms-excel.timelineCache+xml";
pub const CT_TIMELINE: &str = "application/vnd.ms-excel.timeline+xml";

/// What a slicer filters: one field of one or more pivot tables over a pivot cache.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlicerCache {
    /// `Slicer_Region`, which a defined name of the same name also carries.
    pub name: String,
    /// The pivot field it filters, by its name in the source.
    pub source: String,
    /// The pivot tables it filters, by name.
    pub pivots: Vec<String>,
    /// The part it was read from, empty for a cache made here.
    pub part: String,
    /// `sortOrder`, `customListSort`, `showMissing` and the other attributes of `tabular`, kept.
    pub tabular: Vec<(String, String)>,
    /// `xr10:uid`.
    pub uid: Option<String>,
    /// The items marked as having no data, by their shared item number.
    pub no_data: Vec<usize>,
}

/// One slicer: a view of a cache with a caption and a look.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Slicer {
    pub name: String,
    /// The name of its [`SlicerCache`].
    pub cache: String,
    pub caption: Option<String>,
    /// `rowHeight`, `columnCount`, `style`, `startItem` and the other attributes, kept.
    pub attrs: Vec<(String, String)>,
    pub uid: Option<String>,
    /// The sheet it was read from or made on, used only when no drawing object names it.
    pub sheet: usize,
    /// Elements inside `slicer` this version does not model, kept as written.
    pub extra: String,
}

/// What a timeline filters.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimelineCache {
    pub name: String,
    pub source: String,
    pub pivots: Vec<String>,
    pub part: String,
    /// The attributes of `state` other than the filter type (`minimalRefreshVersion` and the rest), kept.
    pub state: Vec<(String, String)>,
    /// The dates the bar keeps, as the file writes them (`2026-02-01T00:00:00`), or `None` for every date.
    pub selection: Option<(String, String)>,
    /// The dates the bar spans.
    pub bounds: Option<(String, String)>,
    pub uid: Option<String>,
}

/// One timeline bar.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Timeline {
    pub name: String,
    pub cache: String,
    pub caption: Option<String>,
    /// `level`, `selectionLevel`, `scrollPosition`, `style` and the rest, kept.
    pub attrs: Vec<(String, String)>,
    pub uid: Option<String>,
    pub sheet: usize,
    pub extra: String,
}

/// Every slicer and timeline of a workbook.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Slicers {
    pub caches: Vec<SlicerCache>,
    pub slicers: Vec<Slicer>,
    pub timeline_caches: Vec<TimelineCache>,
    pub timelines: Vec<Timeline>,
    /// Set by any edit, so the parts are written again.
    pub changed: bool,
}

impl Slicers {
    /// Whether the workbook has no slicer and no timeline.
    pub fn is_empty(&self) -> bool {
        self.slicers.is_empty() && self.timelines.is_empty()
    }

    pub fn cache(&self, name: &str) -> Option<&SlicerCache> {
        self.caches.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn slicer(&self, name: &str) -> Option<&Slicer> {
        self.slicers.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn timeline(&self, name: &str) -> Option<&Timeline> {
        self.timelines.iter().find(|t| t.name.eq_ignore_ascii_case(name))
    }
}
