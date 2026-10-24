//! Calculated fields and calculated items.

use super::cache::{CacheField, CacheValue, CalculatedItem};
use super::calcfield::{self, Expr};
use super::session::Area;
use super::table::{ItemEntry, PivotField, Subtotals};
use crate::edit::Session;

impl Session {
    /// Add a calculated field: a formula over the sums of other fields.
    pub fn pivot_add_calculated_field(&mut self, t: usize, name: &str, formula: &str) -> Result<usize, String> {
        self.check_table(t)?;
        let cache_index = self.book.pivots.tables[t].cache;
        let formula = formula.trim().trim_start_matches('=').trim().to_owned();
        let expr = calcfield::parse(&formula)?;
        let mut names = Vec::new();
        calcfield::names_in(&expr, &mut names);
        {
            let cache = &self.book.pivots.caches[cache_index];
            if cache.fields.iter().any(|f| f.name.eq_ignore_ascii_case(name)) {
                return Err("A PivotTable field name already exists.".to_owned());
            }
            for n in &names {
                if !cache.fields.iter().any(|f| f.database && f.name.eq_ignore_ascii_case(n)) {
                    return Err(format!("The formula refers to {n}, which is not a field."));
                }
            }
        }
        self.remember_whole("Add PivotTable calculated field");
        let cache = &mut self.book.pivots.caches[cache_index];
        let mut field = CacheField::new(name);
        field.database = false;
        field.formula = Some(formula);
        cache.fields.push(field);
        cache.records.iter_mut().for_each(|r| r.push(CacheValue::Blank));
        let index = cache.fields.len() - 1;
        for t2 in 0..self.book.pivots.tables.len() {
            if self.book.pivots.tables[t2].cache != cache_index {
                continue;
            }
            let form = self.book.pivots.tables[t2].options.form;
            let mut pf = PivotField::new(form);
            pf.subtotals = Subtotals::none();
            pf.show_all = false;
            for k in ["dragToRow", "dragToCol", "dragToPage"] {
                pf.attrs.push((k.to_owned(), "0".to_owned()));
            }
            self.book.pivots.tables[t2].fields.push(pf);
        }
        super::xml::sync_cache(&mut self.book, cache_index);
        super::xml::sync_table(&mut self.book, t);
        Ok(index)
    }

    /// Add a calculated item to a field: a formula over its other items.
    pub fn pivot_add_calculated_item(
        &mut self,
        t: usize,
        field: usize,
        name: &str,
        formula: &str,
    ) -> Result<(), String> {
        self.check_table(t)?;
        let cache_index = self.book.pivots.tables[t].cache;
        if field >= self.book.pivots.caches[cache_index].fields.len() {
            return Err("There is no such field.".to_owned());
        }
        self.book.pivots.caches[cache_index].ensure_shared(field);
        let field_name = self.book.pivots.caches[cache_index].fields[field].name.clone();
        let expr = calcfield::parse(formula.trim().trim_start_matches('='))?;
        let text = calcfield::print_with_field(&expr, &field_name);
        {
            let cache = &self.book.pivots.caches[cache_index];
            let exists = cache.fields[field].shared.iter().any(|v| match v {
                CacheValue::Text(s) => s.eq_ignore_ascii_case(name),
                _ => false,
            });
            if exists {
                return Err("A PivotTable item name already exists.".to_owned());
            }
        }
        self.remember_whole("Add PivotTable calculated item");
        let cache = &mut self.book.pivots.caches[cache_index];
        cache.fields[field].shared.push(CacheValue::text(name));
        let item = cache.fields[field].shared.len() - 1;
        cache.calculated_items.push(CalculatedItem { field, formula: text, item });
        for t2 in 0..self.book.pivots.tables.len() {
            if self.book.pivots.tables[t2].cache == cache_index && !self.book.pivots.tables[t2].fields[field].items.is_empty() {
                let mut entry = ItemEntry::new(item);
                entry.calculated = true;
                self.book.pivots.tables[t2].fields[field].items.push(entry);
            }
        }
        super::xml::sync_cache(&mut self.book, cache_index);
        let users: Vec<usize> = (0..self.book.pivots.tables.len())
            .filter(|i| self.book.pivots.tables[*i].cache == cache_index)
            .collect();
        for u in users {
            self.relayout(u)?;
        }
        Ok(())
    }

    /// Put a field in the value area with a function, as dragging it there does.
    pub fn pivot_drop_field(&mut self, t: usize, field: usize, area: Area) -> Result<(), String> {
        self.pivot_move_field(t, field, area, None)
    }
}

/// An item name in a calculated item's formula, written in the field's own brackets.
pub fn expr_uses_items(expr: &Expr) -> bool {
    let mut items = Vec::new();
    calcfield::items_in(expr, &mut items);
    !items.is_empty()
}
