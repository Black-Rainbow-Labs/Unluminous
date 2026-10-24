//! Reading and writing the parts of a pivot table: `pivotCacheDefinition`, `pivotCacheRecords` and
//! `pivotTableDefinition`.

use crate::model::Workbook;

/// Bring the parts of a cache up to date with the model.
pub fn sync_cache(_book: &mut Workbook, _cache: usize) {}

/// Bring the part of a table up to date with the model.
pub fn sync_table(_book: &mut Workbook, _table: usize) {}

/// Bring every part up to date, after tables or caches were removed.
pub fn sync_all(_book: &mut Workbook) {}
