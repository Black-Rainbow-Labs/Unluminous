//! Grouping from the sheet: the Group Field and Group Selection commands, and Ungroup.

use super::cache::GroupBy;
use super::groups;
use super::table::{AxisEntry, ItemEntry, PivotField};
use crate::edit::Session;

impl Session {
    /// Group a date field by periods. Each period becomes a field that takes the field's place.
    pub fn pivot_group_dates(
        &mut self,
        t: usize,
        field: usize,
        by: &[GroupBy],
        start: Option<f64>,
        end: Option<f64>,
        step: Option<f64>,
    ) -> Result<(), String> {
        self.check_table(t)?;
        let cache_index = self.book.pivots.tables[t].cache;
        let mut cache = self.book.pivots.caches[cache_index].clone();
        if field >= cache.fields.len() {
            return Err("There is no such field.".to_owned());
        }
        let added =
            groups::group_dates(&mut cache, field, by, start, end, step, self.book.date1904)?;
        self.remember_whole("Group PivotTable field");
        self.book.pivots.caches[cache_index] = cache;
        self.add_group_fields(cache_index, added.len());
        let first_new = self.book.pivots.caches[cache_index].fields.len() - added.len();
        for t2 in self.users_of(cache_index) {
            for &g in &added {
                self.fill_items(t2, g);
            }
            let table = &mut self.book.pivots.tables[t2];
            replace_entry(&mut table.rows, field, &added);
            replace_entry(&mut table.cols, field, &added);
            if table.pages.iter().any(|p| p.field == field) {
                table.pages.retain(|p| p.field != field);
            }
        }
        let _ = first_new;
        self.refresh_after_group(cache_index)
    }

    /// Group a number field by ranges.
    pub fn pivot_group_numbers(
        &mut self,
        t: usize,
        field: usize,
        start: Option<f64>,
        end: Option<f64>,
        step: f64,
    ) -> Result<(), String> {
        self.check_table(t)?;
        let cache_index = self.book.pivots.tables[t].cache;
        let mut cache = self.book.pivots.caches[cache_index].clone();
        if field >= cache.fields.len() {
            return Err("There is no such field.".to_owned());
        }
        groups::group_numbers(&mut cache, field, start, end, step, self.book.date1904)?;
        self.remember_whole("Group PivotTable field");
        self.book.pivots.caches[cache_index] = cache;
        for t2 in self.users_of(cache_index) {
            self.book.pivots.tables[t2].fields[field].items.clear();
            self.fill_items(t2, field);
        }
        self.refresh_after_group(cache_index)
    }

    /// Group items of a field by hand. A group holds the items at these positions of the field's list.
    pub fn pivot_group_items(
        &mut self,
        t: usize,
        field: usize,
        chosen: &[(Option<String>, Vec<usize>)],
    ) -> Result<(), String> {
        self.check_table(t)?;
        let cache_index = self.book.pivots.tables[t].cache;
        let mut cache = self.book.pivots.caches[cache_index].clone();
        let mut members: Vec<(Option<String>, Vec<usize>)> = Vec::new();
        for (name, positions) in chosen {
            let mut xs = Vec::new();
            for &p in positions {
                let item = self.book.pivots.tables[t].fields[field]
                    .items
                    .get(p)
                    .ok_or("There is no such item.")?;
                xs.push(item.x);
            }
            members.push((name.clone(), xs));
        }
        let before = cache.fields.len();
        let group_field = groups::group_items(&mut cache, field, &members)?;
        self.remember_whole("Group PivotTable items");
        self.book.pivots.caches[cache_index] = cache;
        if self.book.pivots.caches[cache_index].fields.len() > before {
            self.add_group_fields(cache_index, 1);
            for t2 in self.users_of(cache_index) {
                let table = &mut self.book.pivots.tables[t2];
                add_before(&mut table.rows, field, group_field);
                add_before(&mut table.cols, field, group_field);
            }
        }
        for t2 in self.users_of(cache_index) {
            self.book.pivots.tables[t2].fields[group_field].items.clear();
            self.fill_items(t2, group_field);
            self.order_group_items(t2, field, group_field);
        }
        self.refresh_after_group(cache_index)
    }

    /// Take the grouping of a field away.
    pub fn pivot_ungroup(&mut self, t: usize, field: usize) -> Result<(), String> {
        self.check_table(t)?;
        let cache_index = self.book.pivots.tables[t].cache;
        let mut cache = self.book.pivots.caches[cache_index].clone();
        if field >= cache.fields.len() {
            return Err("There is no such field.".to_owned());
        }
        let removed = groups::ungroup(&mut cache, field)?;
        self.remember_whole("Ungroup PivotTable field");
        self.book.pivots.caches[cache_index] = cache;
        for t2 in self.users_of(cache_index) {
            let table = &mut self.book.pivots.tables[t2];
            // Where the groups were on an axis, the field they were made from comes back.
            let spot = |axis: &Vec<AxisEntry>| {
                axis.iter().position(|e| matches!(e, AxisEntry::Field(f) if removed.contains(f)))
            };
            let (row_spot, col_spot) = (spot(&table.rows), spot(&table.cols));
            for &g in removed.iter().rev() {
                remove_field(table, g);
            }
            let base = field - removed.iter().filter(|g| **g < field).count();
            if let Some(at) = row_spot {
                table.rows.insert(at.min(table.rows.len()), AxisEntry::Field(base));
            } else if let Some(at) = col_spot {
                table.cols.insert(at.min(table.cols.len()), AxisEntry::Field(base));
            }
            if removed.is_empty() {
                table.fields[field].items.clear();
                self.fill_items(t2, field);
            }
        }
        self.refresh_after_group(cache_index)
    }

    fn users_of(&self, cache_index: usize) -> Vec<usize> {
        (0..self.book.pivots.tables.len())
            .filter(|i| self.book.pivots.tables[*i].cache == cache_index)
            .collect()
    }

    /// Every table over a cache has a field for each field of the cache.
    fn add_group_fields(&mut self, cache_index: usize, count: usize) {
        for t in self.users_of(cache_index) {
            let form = self.book.pivots.tables[t].options.form;
            let table = &mut self.book.pivots.tables[t];
            for _ in 0..count {
                table.fields.push(PivotField::new(form));
            }
        }
    }

    /// List all the items of a field in a table, the way a field put on an axis does.
    pub(crate) fn fill_items(&mut self, t: usize, field: usize) {
        let cache_index = self.book.pivots.tables[t].cache;
        self.book.pivots.caches[cache_index].ensure_shared(field);
        let count = self.book.pivots.caches[cache_index].fields[field].items().len();
        let form = self.book.pivots.tables[t].options.form;
        let table = &mut self.book.pivots.tables[t];
        if table.fields[field].items.is_empty() {
            table.fields[field].items = (0..count).map(ItemEntry::new).collect();
        }
        table.fields[field].form = form;
    }

    /// The items of a group field follow the place of their first member among the field's items.
    fn order_group_items(&mut self, t: usize, base: usize, group_field: usize) {
        let cache_index = self.book.pivots.tables[t].cache;
        let map = self.book.pivots.caches[cache_index].fields[group_field]
            .group
            .as_ref()
            .and_then(|g| g.discrete.clone())
            .unwrap_or_default();
        let base_items: Vec<usize> =
            self.book.pivots.tables[t].fields[base].items.iter().map(|i| i.x).collect();
        let table = &mut self.book.pivots.tables[t];
        let first_place = |group_item: usize| -> usize {
            base_items.iter().position(|x| map.get(*x) == Some(&group_item)).unwrap_or(usize::MAX)
        };
        table.fields[group_field].items.sort_by_key(|i| first_place(i.x));
    }

    fn refresh_after_group(&mut self, cache_index: usize) -> Result<(), String> {
        super::xml::sync_cache(&mut self.book, cache_index);
        for t in self.users_of(cache_index) {
            self.relayout(t)?;
        }
        Ok(())
    }
}

/// Where a field was on an axis, put these fields, the coarsest first.
fn replace_entry(axis: &mut Vec<AxisEntry>, field: usize, added: &[usize]) {
    if let Some(at) = axis.iter().position(|e| *e == AxisEntry::Field(field)) {
        axis.remove(at);
        for (k, g) in added.iter().rev().enumerate() {
            axis.insert(at + k, AxisEntry::Field(*g));
        }
    }
}

/// Put a field in front of another one on the axis they are on.
fn add_before(axis: &mut Vec<AxisEntry>, field: usize, new: usize) {
    if let Some(at) = axis.iter().position(|e| *e == AxisEntry::Field(field)) {
        axis.insert(at, AxisEntry::Field(new));
    }
}

/// Take a field out of the table, which renumbers the fields after it.
fn remove_field(table: &mut super::table::PivotTable, g: usize) {
    table.fields.remove(g);
    let fix = |f: usize| if f > g { f - 1 } else { f };
    table.rows.retain(|e| *e != AxisEntry::Field(g));
    table.cols.retain(|e| *e != AxisEntry::Field(g));
    for e in table.rows.iter_mut().chain(table.cols.iter_mut()) {
        if let AxisEntry::Field(f) = e {
            *f = fix(*f);
        }
    }
    table.pages.retain(|p| p.field != g);
    table.pages.iter_mut().for_each(|p| p.field = fix(p.field));
    table.data.retain(|d| d.field != g);
    for d in &mut table.data {
        d.field = fix(d.field);
        d.base_field = fix(d.base_field);
    }
}
