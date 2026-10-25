//! Keeping pivot tables right when the sheet around them changes: rows and columns inserted or deleted
//! and sheets renamed, moved or deleted.

use super::cache::CacheSource;
use super::Pivots;
use crate::coord::{CellRef, Range};

impl Pivots {
    /// The reason Excel refuses to insert or delete some rows or columns, when a pivot table is in the way:
    /// one cannot change part of a table, only all of it or none of it.
    pub fn refuse_edit(
        &self,
        sheet: usize,
        rows: bool,
        at: u32,
        count: i64,
        band: Option<(u32, u32)>,
    ) -> Option<String> {
        let message = "Cannot change this part of a PivotTable report.";
        for t in self.tables.iter().filter(|t| t.sheet == sheet) {
            let r = t.full_range();
            let (first, last, other_first, other_last) = if rows {
                (r.first.row, r.last.row, r.first.col, r.last.col)
            } else {
                (r.first.col, r.last.col, r.first.row, r.last.row)
            };
            if let Some((lo, hi)) = band {
                if other_last < lo || other_first > hi {
                    continue;
                }
            }
            if count > 0 {
                if first < at && at <= last {
                    return Some(message.to_owned());
                }
            } else {
                let (from, to) = (at, at + (-count) as u32 - 1);
                let touches = from <= last && to >= first;
                let covers = from <= first && to >= last && band.is_none();
                if touches && !covers {
                    return Some(message.to_owned());
                }
            }
        }
        None
    }

    /// Follow an insert or a delete: the source ranges and the places of the tables move with the cells. A
    /// table whose cells were all deleted is removed. Answers the tables and caches whose parts changed.
    pub fn shift(
        &mut self,
        sheet: usize,
        sheet_name: &str,
        shift: &dyn Fn(Range) -> Option<Range>,
    ) -> Vec<usize> {
        let mut caches = Vec::new();
        for (i, c) in self.caches.iter_mut().enumerate() {
            if let CacheSource::Range { sheet: s, range } = &mut c.source {
                if s.eq_ignore_ascii_case(sheet_name) {
                    if let Some(r) = shift(*range) {
                        if r != *range {
                            *range = r;
                            c.dirty = true;
                            caches.push(i);
                        }
                    }
                }
            }
        }
        let mut gone = Vec::new();
        for (i, t) in self.tables.iter_mut().enumerate() {
            if t.sheet != sheet {
                continue;
            }
            match shift(t.full_range()) {
                None => gone.push(i),
                Some(full) => {
                    let page_rows = t.full_range().first.row.abs_diff(t.location.first.row);
                    let anchor = CellRef::new(full.first.row + page_rows, full.first.col);
                    let size = (t.location.rows(), t.location.cols());
                    let moved = Range::new(
                        anchor,
                        CellRef::new(anchor.row + size.0 - 1, anchor.col + size.1 - 1),
                    );
                    if moved != t.location {
                        t.location = moved;
                        t.dirty = true;
                    }
                }
            }
        }
        for i in gone.into_iter().rev() {
            self.remove_table(i);
        }
        caches
    }

    /// Take a table out, and its cache when no other table reads it.
    pub fn remove_table(&mut self, t: usize) {
        let table = self.tables.remove(t);
        self.removed_parts.push(table.part);
        let cache = table.cache;
        if !self.tables.iter().any(|x| x.cache == cache) {
            let gone = self.caches.remove(cache);
            self.removed_parts.push(gone.definition_part);
            self.removed_parts.push(gone.records_part);
            for x in &mut self.tables {
                if x.cache > cache {
                    x.cache -= 1;
                }
            }
        }
    }

    /// A sheet was renamed: caches that read it follow.
    pub fn sheet_renamed(&mut self, old: &str, new: &str) {
        for c in &mut self.caches {
            if let CacheSource::Range { sheet, .. } = &mut c.source {
                if sheet.eq_ignore_ascii_case(old) {
                    *sheet = new.to_owned();
                    c.dirty = true;
                }
            }
        }
    }

    /// A sheet was deleted: the tables on it go with it.
    pub fn sheet_deleted(&mut self, sheet: usize) {
        while let Some(i) = self.tables.iter().position(|t| t.sheet == sheet) {
            self.remove_table(i);
        }
    }

    /// Sheets were reordered, inserted or removed: tables follow their sheet.
    pub fn sheets_moved(&mut self, map: &dyn Fn(usize) -> usize) {
        for t in &mut self.tables {
            let new = map(t.sheet);
            if new != t.sheet {
                t.sheet = new;
            }
        }
    }
}
