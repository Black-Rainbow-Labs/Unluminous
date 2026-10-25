//! Every change a person makes to a pivot table, each one undoable.
//!
//! A change edits the definition, calculates the table again, clears the cells it used to fill and writes
//! the cells it fills now, as one undo step. The parts of the file are brought up to date at the same time
//! (see [`super::xml`]), so saving needs nothing more.

use std::collections::HashMap;

use super::acc::CellVal;
use super::cache::{CacheSource, PivotCache};
use super::compute::Calc;
use super::layout::{lay_out, Laid};
use super::table::{
    AutoSort, AxisEntry, DataField, Filter, FilterKind, ItemEntry, PageField, PivotTable,
    StyleInfo, Subtotals,
};
use super::{Form, Func, ShowAs, SortType};
use crate::coord::{CellRef, Range};
use crate::edit::Session;
use crate::model::Cell;
use crate::styles::StyleChange;
use crate::value::Value;

/// Where the data of a new pivot table comes from.
#[derive(Debug, Clone)]
pub enum PivotSource {
    /// A range of a sheet whose first row names the fields.
    Range { sheet: usize, range: Range },
    /// A table or a defined name.
    Name(String),
    /// The cache of a pivot table that exists, as Excel shares it when the source is the same.
    Cache(usize),
}

/// The areas of the field list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Rows,
    Cols,
    Pages,
    /// Out of the table.
    Hidden,
}

/// What can be changed about a value field. A `None` leaves the setting as it is.
#[derive(Debug, Clone, Default)]
pub struct ValueSettings {
    pub name: Option<String>,
    pub func: Option<Func>,
    pub show: Option<ShowAs>,
    pub base_field: Option<usize>,
    pub base_item: Option<u32>,
    /// `Some(None)` takes the number format away.
    pub num_fmt: Option<Option<String>>,
}

/// What can be changed about a field. A `None` leaves the setting as it is.
#[derive(Debug, Clone, Default)]
pub struct FieldSettings {
    pub name: Option<Option<String>>,
    pub form: Option<Form>,
    pub subtotals: Option<Subtotals>,
    pub subtotal_top: Option<bool>,
    pub repeat_labels: Option<bool>,
    pub blank_row: Option<bool>,
    pub show_all: Option<bool>,
    pub num_fmt: Option<Option<String>>,
    pub multi_page: Option<bool>,
}

impl Session {
    // ------------------------------------------------------------------------------------------------
    // Making and removing.

    /// Make a pivot table with no fields yet, at a cell of a sheet.
    pub fn pivot_create(
        &mut self,
        source: PivotSource,
        sheet: usize,
        dest: CellRef,
        name: Option<&str>,
    ) -> Result<usize, String> {
        self.check_sheet(sheet)?;
        if self.book.pivots.at(sheet, dest).is_some() {
            return Err("A PivotTable report cannot overlap another PivotTable report.".to_owned());
        }
        let name = match name {
            Some(n) => {
                if self.book.pivots.find(n).is_some() {
                    return Err(format!("The PivotTable name {n} is already in use."));
                }
                n.to_owned()
            }
            None => (1..)
                .map(|n| format!("PivotTable{n}"))
                .find(|n| self.book.pivots.find(n).is_none())
                .unwrap_or_default(),
        };
        let cache_index = self.cache_for(&source)?;
        self.remember_whole("Create PivotTable");
        let cache_index = match cache_index {
            Ok(index) => index,
            Err(cache) => {
                self.book.pivots.caches.push(cache);
                self.book.pivots.caches.len() - 1
            }
        };
        let table = {
            let cache = &self.book.pivots.caches[cache_index];
            let mut table = PivotTable::new(&name, sheet, cache_index, dest, cache);
            table.location = empty_location(dest);
            table
        };
        self.book.pivots.tables.push(table);
        let t = self.book.pivots.tables.len() - 1;
        super::xml::sync_cache(&mut self.book, cache_index);
        super::xml::sync_table(&mut self.book, t);
        Ok(t)
    }

    /// The cache a source names: one that exists, or a new one.
    fn cache_for(&self, source: &PivotSource) -> Result<Result<usize, PivotCache>, String> {
        match source {
            PivotSource::Cache(index) => {
                if *index >= self.book.pivots.caches.len() {
                    return Err("There is no such pivot cache.".to_owned());
                }
                Ok(Ok(*index))
            }
            PivotSource::Range { sheet, range } => {
                let name = self.book.sheets[*sheet].name.clone();
                let id = self.next_cache_id();
                let cache = PivotCache::from_range(
                    &self.book,
                    *sheet,
                    *range,
                    CacheSource::Range { sheet: name, range: *range },
                    id,
                )?;
                Ok(Err(cache))
            }
            PivotSource::Name(name) => {
                let src = CacheSource::Name(name.clone());
                let (sheet, range) = src.resolve(&self.book)?;
                let id = self.next_cache_id();
                Ok(Err(PivotCache::from_range(&self.book, sheet, range, src, id)?))
            }
        }
    }

    fn next_cache_id(&self) -> u32 {
        self.book.pivots.caches.iter().map(|c| c.id).max().map_or(1, |m| m + 1)
    }

    /// Remove a pivot table and the cells it filled.
    pub fn pivot_delete(&mut self, t: usize) -> Result<(), String> {
        self.check_table(t)?;
        self.remember_whole("Delete PivotTable");
        let table = self.book.pivots.tables.remove(t);
        self.clear_cells(table.sheet, table.full_range(), table.has_cells);
        self.book.pivots.removed_parts.push(table.part.clone());
        let cache = table.cache;
        if !self.book.pivots.tables.iter().any(|x| x.cache == cache) {
            let gone = self.book.pivots.caches.remove(cache);
            self.book.pivots.removed_parts.push(gone.definition_part);
            self.book.pivots.removed_parts.push(gone.records_part);
            for x in &mut self.book.pivots.tables {
                if x.cache > cache {
                    x.cache -= 1;
                }
            }
        }
        super::xml::sync_all(&mut self.book);
        self.recalculate();
        Ok(())
    }

    /// Move a pivot table to another place, on this sheet or another.
    pub fn pivot_move(&mut self, t: usize, sheet: usize, to: CellRef) -> Result<(), String> {
        self.check_table(t)?;
        self.check_sheet(sheet)?;
        self.remember_whole("Move PivotTable");
        let table = self.book.pivots.tables[t].clone();
        self.clear_cells(table.sheet, table.full_range(), table.has_cells);
        let t_mut = &mut self.book.pivots.tables[t];
        t_mut.sheet = sheet;
        t_mut.location = Range::new(to, to);
        t_mut.has_cells = false;
        t_mut.part.clear();
        self.relayout(t)
    }

    /// Point a pivot table at other data.
    pub fn pivot_change_source(&mut self, t: usize, source: PivotSource) -> Result<(), String> {
        self.check_table(t)?;
        let cache = match self.cache_for(&source)? {
            Ok(index) => return self.pivot_use_cache(t, index),
            Err(cache) => cache,
        };
        self.remember_whole("Change PivotTable data source");
        let old = self.book.pivots.tables[t].cache;
        let shared_with_others =
            self.book.pivots.tables.iter().enumerate().any(|(i, x)| i != t && x.cache == old);
        let index = if shared_with_others {
            self.book.pivots.caches.push(cache);
            self.book.pivots.caches.len() - 1
        } else {
            self.book.pivots.caches[old] = cache;
            old
        };
        self.book.pivots.tables[t].cache = index;
        self.reset_fields_to_cache(t, index);
        super::xml::sync_cache(&mut self.book, index);
        self.relayout(t)
    }

    fn pivot_use_cache(&mut self, t: usize, index: usize) -> Result<(), String> {
        self.remember_whole("Change PivotTable data source");
        self.book.pivots.tables[t].cache = index;
        self.reset_fields_to_cache(t, index);
        self.relayout(t)
    }

    /// After the source changed: keep the fields that have the same name, drop the rest.
    fn reset_fields_to_cache(&mut self, t: usize, cache_index: usize) {
        let cache = self.book.pivots.caches[cache_index].clone();
        let old_cache_fields: Vec<String> = {
            let table = &self.book.pivots.tables[t];
            // The names the old fields had, from the table's own captions when it set them.
            table
                .fields
                .iter()
                .enumerate()
                .map(|(i, f)| f.name.clone().unwrap_or_else(|| format!("#{i}")))
                .collect()
        };
        let _ = old_cache_fields;
        let table = &mut self.book.pivots.tables[t];
        let form = table.options.form;
        let mut fresh =
            PivotTable::new(&table.name, table.sheet, cache_index, table.location.first, &cache);
        fresh.options = table.options.clone();
        fresh.style = table.style.clone();
        fresh.part = table.part.clone();
        fresh.has_cells = table.has_cells;
        fresh.location = table.location;
        let _ = form;
        *table = fresh;
    }

    // ------------------------------------------------------------------------------------------------
    // Refreshing.

    /// Read the source again and calculate the table again.
    pub fn pivot_refresh(&mut self, t: usize) -> Result<(), String> {
        self.check_table(t)?;
        self.remember_whole("Refresh PivotTable");
        let cache_index = self.book.pivots.tables[t].cache;
        self.refresh_cache(cache_index)?;
        let users: Vec<usize> = (0..self.book.pivots.tables.len())
            .filter(|i| self.book.pivots.tables[*i].cache == cache_index)
            .collect();
        for u in users {
            self.relayout(u)?;
        }
        Ok(())
    }

    /// Refresh every pivot table, as Refresh All does.
    pub fn pivot_refresh_all(&mut self) -> Result<(), String> {
        if self.book.pivots.tables.is_empty() {
            return Ok(());
        }
        self.remember_whole("Refresh All");
        for c in 0..self.book.pivots.caches.len() {
            self.refresh_cache(c)?;
        }
        for t in 0..self.book.pivots.tables.len() {
            self.relayout(t)?;
        }
        Ok(())
    }

    fn refresh_cache(&mut self, cache_index: usize) -> Result<(), String> {
        let mut cache = self.book.pivots.caches[cache_index].clone();
        let remaps = cache.refresh(&self.book)?;
        self.book.pivots.caches[cache_index] = cache;
        for t in 0..self.book.pivots.tables.len() {
            if self.book.pivots.tables[t].cache != cache_index {
                continue;
            }
            self.follow_remaps(t, &remaps);
            self.add_new_items(t);
        }
        super::xml::sync_cache(&mut self.book, cache_index);
        Ok(())
    }

    /// Items of the cache moved or went when it was read again: follow them in the table.
    fn follow_remaps(&mut self, t: usize, remaps: &[super::cache::ItemRemap]) {
        let table = &mut self.book.pivots.tables[t];
        for remap in remaps {
            let field = &mut table.fields[remap.field];
            let mut kept: Vec<ItemEntry> = Vec::new();
            let mut old_position: Vec<Option<usize>> = vec![None; field.items.len()];
            for (i, entry) in field.items.iter().enumerate() {
                if let Some(Some(new_x)) = remap.moved.get(entry.x) {
                    let mut e = entry.clone();
                    e.x = *new_x;
                    old_position[i] = Some(kept.len());
                    kept.push(e);
                }
            }
            field.items = kept;
            for p in &mut table.pages {
                if p.field == remap.field {
                    p.item = p.item.and_then(|i| old_position.get(i).copied().flatten());
                }
            }
            for d in &mut table.data {
                if d.base_field == remap.field && d.base_item < super::BASE_PREVIOUS {
                    d.base_item = old_position
                        .get(d.base_item as usize)
                        .copied()
                        .flatten()
                        .map_or(0, |p| p as u32);
                }
            }
        }
    }

    /// Items the source has now that the table does not list yet: they follow the others, shown.
    fn add_new_items(&mut self, t: usize) {
        let cache_index = self.book.pivots.tables[t].cache;
        let used = self.book.pivots.tables[t].used_fields();
        for f in used {
            let count = self.book.pivots.caches[cache_index].fields[f].items().len();
            let table = &mut self.book.pivots.tables[t];
            for x in 0..count {
                if table.fields[f].position_of(x).is_none() {
                    table.fields[f].items.push(ItemEntry::new(x));
                }
            }
        }
    }

    // ------------------------------------------------------------------------------------------------
    // Fields.

    /// Put a field in an area, at a position among the fields already there.
    pub fn pivot_move_field(
        &mut self,
        t: usize,
        field: usize,
        area: Area,
        position: Option<usize>,
    ) -> Result<(), String> {
        self.check_table(t)?;
        if field >= self.book.pivots.tables[t].fields.len() {
            return Err("There is no such field.".to_owned());
        }
        self.remember_whole("Move PivotTable field");
        self.take_off_axes(t, field);
        if area != Area::Hidden {
            self.make_items(t, field);
            let table = &mut self.book.pivots.tables[t];
            match area {
                Area::Rows => insert_at(&mut table.rows, AxisEntry::Field(field), position),
                Area::Cols => insert_at(&mut table.cols, AxisEntry::Field(field), position),
                Area::Pages => {
                    let page = PageField { field, item: None };
                    let at = position.unwrap_or(table.pages.len()).min(table.pages.len());
                    table.pages.insert(at, page);
                }
                Area::Hidden => {}
            }
        }
        self.relayout(t)
    }

    fn take_off_axes(&mut self, t: usize, field: usize) {
        let table = &mut self.book.pivots.tables[t];
        table.rows.retain(|e| *e != AxisEntry::Field(field));
        table.cols.retain(|e| *e != AxisEntry::Field(field));
        table.pages.retain(|p| p.field != field);
    }

    /// A field that goes on an axis lists its items: all of them, in ascending order.
    fn make_items(&mut self, t: usize, field: usize) {
        let cache_index = self.book.pivots.tables[t].cache;
        self.book.pivots.caches[cache_index].ensure_shared(field);
        let cache = &self.book.pivots.caches[cache_index];
        let count = cache.fields[field].items().len();
        let table = &mut self.book.pivots.tables[t];
        if table.fields[field].items.is_empty() {
            let mut xs: Vec<usize> = (0..count).collect();
            let items = cache.fields[field].items();
            if !cache.fields[field].has_group_items() {
                xs.sort_by(|a, b| items[*a].order(&items[*b]));
            }
            table.fields[field].items = xs.into_iter().map(ItemEntry::new).collect();
        }
        table.fields[field].form = table.options.form;
    }

    /// Where the Values entry is: in the row area or the column area, at a place among the fields.
    pub fn pivot_move_values(
        &mut self,
        t: usize,
        area: Area,
        position: Option<usize>,
    ) -> Result<(), String> {
        self.check_table(t)?;
        self.remember_whole("Move PivotTable values");
        let table = &mut self.book.pivots.tables[t];
        table.rows.retain(|e| *e != AxisEntry::Values);
        table.cols.retain(|e| *e != AxisEntry::Values);
        match area {
            Area::Rows => insert_at(&mut table.rows, AxisEntry::Values, position),
            _ => insert_at(&mut table.cols, AxisEntry::Values, position),
        }
        self.relayout(t)
    }

    /// Add a value field.
    pub fn pivot_add_value(
        &mut self,
        t: usize,
        field: usize,
        func: Func,
        name: Option<&str>,
    ) -> Result<usize, String> {
        self.check_table(t)?;
        if field >= self.book.pivots.tables[t].fields.len() {
            return Err("There is no such field.".to_owned());
        }
        self.remember_whole("Add PivotTable value field");
        let cache = self.book.pivots.tables[t].cache;
        let name = match name {
            Some(n) => n.to_owned(),
            None => self.book.pivots.tables[t].default_data_name(
                &self.book.pivots.caches[cache],
                field,
                func,
            ),
        };
        let table = &mut self.book.pivots.tables[t];
        let mut data = DataField::new(&name, field, func);
        if let Some(f) = data.show.default_format() {
            data.num_fmt = Some(crate::styles::builtin_format(f).unwrap_or("General").to_owned());
        } else {
            data.num_fmt = inherited_format(&self.book.pivots.caches[cache], field);
        }
        table.data.push(data);
        let index = table.data.len() - 1;
        self.relayout(t)?;
        Ok(index)
    }

    /// Take a value field out.
    pub fn pivot_remove_value(&mut self, t: usize, data: usize) -> Result<(), String> {
        self.check_table(t)?;
        if data >= self.book.pivots.tables[t].data.len() {
            return Err("There is no such value field.".to_owned());
        }
        self.remember_whole("Remove PivotTable value field");
        let table = &mut self.book.pivots.tables[t];
        table.data.remove(data);
        for f in &mut table.fields {
            f.auto_sort = match f.auto_sort.take() {
                Some(a) if a.data == data => None,
                Some(a) if a.data > data => Some(AutoSort { data: a.data - 1 }),
                other => other,
            };
            f.filters.retain(|x| x.data != Some(data));
        }
        self.relayout(t)
    }

    /// Change a value field's name, function, Show Values As setting or number format.
    pub fn pivot_value_settings(
        &mut self,
        t: usize,
        data: usize,
        s: ValueSettings,
    ) -> Result<(), String> {
        self.check_table(t)?;
        if data >= self.book.pivots.tables[t].data.len() {
            return Err("There is no such value field.".to_owned());
        }
        if let Some(name) = &s.name {
            let table = &self.book.pivots.tables[t];
            let cache = &self.book.pivots.caches[table.cache];
            let clash = table
                .data
                .iter()
                .enumerate()
                .any(|(i, d)| i != data && d.name.eq_ignore_ascii_case(name))
                || (0..cache.fields.len())
                    .any(|f| table.caption(cache, f).eq_ignore_ascii_case(name));
            if clash {
                return Err("A PivotTable field name already exists.".to_owned());
            }
        }
        self.remember_whole("Change PivotTable value field");
        let table = &mut self.book.pivots.tables[t];
        let inherited =
            inherited_format(&self.book.pivots.caches[table.cache], table.data[data].field);
        let d = &mut table.data[data];
        if let Some(func) = s.func {
            let old_default = format!("{} of ", d.func.label());
            if d.name.starts_with(&old_default) && s.name.is_none() {
                d.name = format!("{} of {}", func.label(), &d.name[old_default.len()..]);
            }
            d.func = func;
        }
        if let Some(name) = s.name {
            d.name = name;
        }
        if let Some(show) = s.show {
            d.show = show;
            if let Some(id) = show.default_format() {
                if d.num_fmt.is_none() || d.num_fmt == inherited {
                    d.num_fmt =
                        Some(crate::styles::builtin_format(id).unwrap_or("General").to_owned());
                }
            }
        }
        if let Some(b) = s.base_field {
            d.base_field = b;
        }
        if let Some(b) = s.base_item {
            d.base_item = b;
        }
        if let Some(f) = s.num_fmt {
            d.num_fmt = f;
        }
        self.relayout(t)
    }

    /// Change how a field is laid out and what it shows.
    pub fn pivot_field_settings(
        &mut self,
        t: usize,
        field: usize,
        s: FieldSettings,
    ) -> Result<(), String> {
        self.check_table(t)?;
        if field >= self.book.pivots.tables[t].fields.len() {
            return Err("There is no such field.".to_owned());
        }
        self.remember_whole("Change PivotTable field");
        let pf = &mut self.book.pivots.tables[t].fields[field];
        if let Some(v) = s.name {
            pf.name = v;
        }
        if let Some(v) = s.form {
            pf.form = v;
        }
        if let Some(v) = s.subtotals {
            pf.subtotals = v;
        }
        if let Some(v) = s.subtotal_top {
            pf.subtotal_top = v;
        }
        if let Some(v) = s.repeat_labels {
            pf.repeat_labels = v;
        }
        if let Some(v) = s.blank_row {
            pf.blank_row = v;
        }
        if let Some(v) = s.show_all {
            pf.show_all = v;
        }
        if let Some(v) = s.num_fmt {
            pf.num_fmt = v;
        }
        if let Some(v) = s.multi_page {
            pf.multi_page = v;
        }
        self.relayout(t)
    }

    /// Lay every field out in one form, which is what Report Layout does.
    pub fn pivot_set_form(&mut self, t: usize, form: Form) -> Result<(), String> {
        self.check_table(t)?;
        self.remember_whole("Change PivotTable layout");
        let table = &mut self.book.pivots.tables[t];
        table.options.form = form;
        for f in &mut table.fields {
            f.form = form;
        }
        self.relayout(t)
    }

    /// Show or hide the grand totals of rows and of columns.
    pub fn pivot_set_grand_totals(&mut self, t: usize, row: bool, col: bool) -> Result<(), String> {
        self.check_table(t)?;
        self.remember_whole("Change PivotTable grand totals");
        let table = &mut self.book.pivots.tables[t];
        table.options.grand_total_row = row;
        table.options.grand_total_col = col;
        self.relayout(t)
    }

    /// Set the captions and the other options of the table.
    pub fn pivot_set_options(
        &mut self,
        t: usize,
        change: impl FnOnce(&mut super::table::Options),
    ) -> Result<(), String> {
        self.check_table(t)?;
        self.remember_whole("Change PivotTable options");
        change(&mut self.book.pivots.tables[t].options);
        self.relayout(t)
    }

    /// Choose the pivot style and its options.
    pub fn pivot_set_style(&mut self, t: usize, style: StyleInfo) -> Result<(), String> {
        self.check_table(t)?;
        self.remember_whole("Change PivotTable style");
        self.book.pivots.tables[t].style = style;
        self.book.pivots.tables[t].dirty = true;
        super::xml::sync_table(&mut self.book, t);
        Ok(())
    }

    // ------------------------------------------------------------------------------------------------
    // Sorting and filtering.

    /// Sort a field's items by their labels, or by a value field.
    pub fn pivot_sort(
        &mut self,
        t: usize,
        field: usize,
        order: SortType,
        by_value: Option<usize>,
    ) -> Result<(), String> {
        self.check_table(t)?;
        if field >= self.book.pivots.tables[t].fields.len() {
            return Err("There is no such field.".to_owned());
        }
        self.remember_whole("Sort PivotTable");
        let cache_index = self.book.pivots.tables[t].cache;
        let table = &mut self.book.pivots.tables[t];
        let pf = &mut table.fields[field];
        pf.sort = order;
        pf.auto_sort = by_value.map(|data| AutoSort { data });
        if by_value.is_none() && order != SortType::Manual {
            let items = self.book.pivots.caches[cache_index].fields[field].items().to_vec();
            pf.items.sort_by(|a, b| {
                let ord = items[a.x].order(&items[b.x]);
                if order == SortType::Descending {
                    ord.reverse()
                } else {
                    ord
                }
            });
        }
        self.relayout(t)
    }

    /// Put an item first, last or at a place in its field, which is what dragging it does.
    pub fn pivot_move_item(
        &mut self,
        t: usize,
        field: usize,
        from: usize,
        to: usize,
    ) -> Result<(), String> {
        self.check_table(t)?;
        let len = self.book.pivots.tables[t].fields.get(field).map_or(0, |f| f.items.len());
        if from >= len || to >= len {
            return Err("There is no such item.".to_owned());
        }
        self.remember_whole("Move PivotTable item");
        let pf = &mut self.book.pivots.tables[t].fields[field];
        let item = pf.items.remove(from);
        pf.items.insert(to, item);
        pf.sort = SortType::Manual;
        pf.auto_sort = None;
        self.relayout(t)
    }

    /// Show or hide an item.
    pub fn pivot_set_item_visible(
        &mut self,
        t: usize,
        field: usize,
        item: usize,
        visible: bool,
    ) -> Result<(), String> {
        self.check_table(t)?;
        let ok = self.book.pivots.tables[t].fields.get(field).is_some_and(|f| item < f.items.len());
        if !ok {
            return Err("There is no such item.".to_owned());
        }
        self.remember_whole("Filter PivotTable");
        let table = &mut self.book.pivots.tables[t];
        table.fields[field].items[item].hidden = !visible;
        table.fields[field].filters.clear();
        // A filter field that shows one item shows all of them now.
        for p in &mut table.pages {
            if p.field == field {
                p.item = None;
            }
        }
        self.relayout(t)
    }

    /// Show only one item of a filter field.
    pub fn pivot_set_page_item(
        &mut self,
        t: usize,
        field: usize,
        item: Option<usize>,
    ) -> Result<(), String> {
        self.check_table(t)?;
        self.remember_whole("Filter PivotTable");
        let table = &mut self.book.pivots.tables[t];
        let Some(page) = table.pages.iter_mut().find(|p| p.field == field) else {
            return Err("The field is not a filter field.".to_owned());
        };
        page.item = item;
        for e in &mut table.fields[field].items {
            e.hidden = false;
        }
        self.relayout(t)
    }

    /// Put a label, value or top N filter on a field. Showing every item again clears it.
    pub fn pivot_set_filter(
        &mut self,
        t: usize,
        field: usize,
        filter: Option<Filter>,
    ) -> Result<(), String> {
        self.check_table(t)?;
        if field >= self.book.pivots.tables[t].fields.len() {
            return Err("There is no such field.".to_owned());
        }
        self.remember_whole("Filter PivotTable");
        let pf = &mut self.book.pivots.tables[t].fields[field];
        pf.filters.clear();
        for e in &mut pf.items {
            e.hidden = false;
        }
        if let Some(mut f) = filter {
            f.id = 1;
            if f.kind.is_value() && f.data.is_none() {
                f.data = Some(0);
            }
            pf.filters.push(f);
        }
        self.relayout(t)
    }

    // ------------------------------------------------------------------------------------------------
    // Laying out.

    pub(crate) fn check_table(&self, t: usize) -> Result<(), String> {
        if t < self.book.pivots.tables.len() {
            Ok(())
        } else {
            Err("There is no such PivotTable.".to_owned())
        }
    }

    /// Calculate a table, clear the cells it filled and write the cells it fills now.
    pub fn relayout(&mut self, t: usize) -> Result<(), String> {
        let (laid, empty) = {
            let table = &self.book.pivots.tables[t];
            let cache = &self.book.pivots.caches[table.cache];
            if table.rows.is_empty()
                && table.cols.is_empty()
                && table.pages.is_empty()
                && table.data.is_empty()
            {
                (Laid::default(), true)
            } else {
                let calc = Calc::build(table, cache, self.book.date1904);
                (lay_out(&calc), false)
            }
        };
        let (sheet, old_range, had_cells) = {
            let table = &self.book.pivots.tables[t];
            (table.sheet, table.full_range(), table.has_cells)
        };
        let anchor = self.book.pivots.tables[t].location.first;
        let top = anchor.row.saturating_sub(laid.page_rows);
        let (top, anchor) = if anchor.row < laid.page_rows {
            (0, CellRef::new(laid.page_rows, anchor.col))
        } else {
            (top, anchor)
        };
        let new_range = if empty {
            empty_location(anchor)
        } else {
            Range::new(
                CellRef::new(top, anchor.col),
                CellRef::new(
                    top + laid.rows.max(1) - 1,
                    anchor.col + laid.cols.max(laid.page_cols()).max(1) - 1,
                ),
            )
        };
        self.refuse_overlap(t, sheet, new_range)?;
        self.clear_cells(sheet, old_range, had_cells);
        if !empty {
            self.write_cells(sheet, CellRef::new(top, anchor.col), &laid);
        }
        let table = &mut self.book.pivots.tables[t];
        table.has_cells = !empty;
        table.location = if empty {
            new_range
        } else {
            Range::new(
                anchor,
                CellRef::new(
                    anchor.row + (laid.rows - laid.page_rows).max(1) - 1,
                    anchor.col + laid.cols.max(1) - 1,
                ),
            )
        };
        table.first_header_row = laid.first_header_row;
        table.first_data_row = laid.first_data_row;
        table.first_data_col = laid.first_data_col;
        table.row_page_count = if table.pages.is_empty() { 0 } else { table.pages.len() as u32 };
        table.col_page_count = if table.pages.is_empty() { 0 } else { 1 };
        table.lines = laid.lines.clone();
        table.dirty = true;
        super::xml::sync_table(&mut self.book, t);
        self.pivot_cells_changed(sheet, old_range.union(&new_range));
        Ok(())
    }

    fn refuse_overlap(&self, t: usize, sheet: usize, range: Range) -> Result<(), String> {
        let clash = self
            .book
            .pivots
            .tables
            .iter()
            .enumerate()
            .any(|(i, x)| i != t && x.sheet == sheet && x.full_range().intersects(&range));
        if clash {
            return Err("A PivotTable report cannot overlap another PivotTable report.".to_owned());
        }
        Ok(())
    }

    fn clear_cells(&mut self, sheet: usize, range: Range, had_cells: bool) {
        if !had_cells {
            return;
        }
        let s = &mut self.book.sheets[sheet];
        let keys: Vec<(u32, u32)> = s
            .cells
            .range((range.first.row, 0)..=(range.last.row, u32::MAX))
            .map(|(k, _)| *k)
            .filter(|(_, c)| *c >= range.first.col && *c <= range.last.col)
            .collect();
        for key in keys {
            s.cells.remove(&key);
        }
    }

    fn write_cells(&mut self, sheet: usize, origin: CellRef, laid: &Laid) {
        let mut styles: HashMap<String, u32> = HashMap::new();
        for cell in &laid.cells {
            let style = match &cell.format {
                Some(code) if code != "General" => {
                    *styles.entry(code.clone()).or_insert_with(|| {
                        self.book.styles.derive(0, &StyleChange::NumberFormat(code.clone()))
                    })
                }
                _ => 0,
            };
            let at = CellRef::new(origin.row + cell.row, origin.col + cell.col);
            self.book.sheets[sheet].cells.insert(
                (at.row, at.col),
                Cell { value: cell.value.clone(), formula: None, style, rich: None, extra: None },
            );
        }
    }

    /// Tell the calculation that cells of a range changed.
    fn pivot_cells_changed(&mut self, sheet: usize, range: Range) {
        let limit = 20_000u64;
        if range.len() <= limit {
            for at in range.cells() {
                self.engine.changed(&self.book, sheet, at);
            }
        } else {
            self.engine.rebuild(&self.book);
            self.engine.mark_all();
        }
        self.recalculate();
    }
}

/// The room Excel reserves for an empty pivot table.
fn empty_location(at: CellRef) -> Range {
    Range::new(at, CellRef::new(at.row + 17, at.col + 2))
}

fn insert_at(list: &mut Vec<AxisEntry>, entry: AxisEntry, position: Option<usize>) {
    let at = position.unwrap_or(list.len()).min(list.len());
    list.insert(at, entry);
}

/// The text a cell of a laid out table shows, for tests and for the agent verbs.
pub fn cell_value_text(v: &CellVal) -> String {
    match v {
        CellVal::Empty => String::new(),
        CellVal::Num(n) => crate::value::number_to_text(*n),
        CellVal::Err(e) => e.text().to_owned(),
    }
}

/// A value field's figure as a sheet value.
pub fn sheet_value(v: CellVal) -> Value {
    match v {
        CellVal::Empty => Value::Empty,
        CellVal::Num(n) => Value::Number(n),
        CellVal::Err(e) => Value::Error(e),
    }
}

/// A filter that keeps the items whose label begins with some text.
pub fn label_filter(kind: FilterKind, value: &str) -> Filter {
    Filter { kind, data: None, value1: value.to_owned(), value2: None, top: true, id: 1 }
}

/// The number format a value field shows its figures in when nobody chose one: the source column's, unless
/// that is General or shows dates or times.
fn inherited_format(cache: &super::cache::PivotCache, field: usize) -> Option<String> {
    let source = &cache.fields[field];
    let code = match &source.num_fmt_code {
        Some(code) => code.clone(),
        None => crate::styles::builtin_format(source.num_fmt_id).unwrap_or("General").to_owned(),
    };
    let format = crate::numfmt::Format::parse(&code);
    (!format.is_general() && !format.is_date()).then_some(code)
}
