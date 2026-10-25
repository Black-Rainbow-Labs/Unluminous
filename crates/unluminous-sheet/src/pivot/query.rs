//! Reading a figure out of a pivot table by the names of its fields and items: what `GETPIVOTDATA` does.

use super::cache::CacheValue;
use super::items::{Line, LineKind, Seg};
use super::table::{AxisEntry, PivotTable};
use super::Pivots;
use crate::coord::CellRef;
use crate::model::Workbook;
use crate::value::{ErrorKind, Value};

impl Pivots {
    /// The figure of a value field at the crossing of some items, which `GETPIVOTDATA(data, cell, field1,
    /// item1, ...)` answers. With no field it is the grand total.
    pub fn get_pivot_data(
        &self,
        book: &Workbook,
        sheet: usize,
        at: CellRef,
        data: &str,
        pairs: &[(String, Value)],
    ) -> Result<Value, ErrorKind> {
        let t = self.at(sheet, at).ok_or(ErrorKind::Ref)?;
        let table = &self.tables[t];
        let cache = &self.caches[table.cache];
        let d = find_data(table, cache, data).ok_or(ErrorKind::Ref)?;
        let mut row_items: Vec<Option<usize>> = Vec::new();
        let mut col_items: Vec<Option<usize>> = Vec::new();
        let row_fields = real_fields(&table.row_axis());
        let col_fields = real_fields(&table.col_axis());
        row_items.resize(row_fields.len(), None);
        col_items.resize(col_fields.len(), None);
        for (name, item) in pairs {
            let f = find_field(table, cache, name).ok_or(ErrorKind::Ref)?;
            let pos = find_item(table, cache, f, item).ok_or(ErrorKind::Ref)?;
            if let Some(i) = row_fields.iter().position(|x| *x == f) {
                row_items[i] = Some(pos);
            } else if let Some(i) = col_fields.iter().position(|x| *x == f) {
                col_items[i] = Some(pos);
            } else if let Some(p) = table.pages.iter().find(|p| p.field == f) {
                if p.item != Some(pos) {
                    return Err(ErrorKind::Ref);
                }
            } else {
                return Err(ErrorKind::Ref);
            }
        }
        let row = find_line(
            &table.lines.rows,
            &prefix(&row_items)?,
            d,
            table.row_axis().contains(&AxisEntry::Values),
            !row_fields.is_empty(),
        )
        .ok_or(ErrorKind::Ref)?;
        let col = find_line(
            &table.lines.cols,
            &prefix(&col_items)?,
            d,
            table.col_axis().contains(&AxisEntry::Values),
            !col_fields.is_empty(),
        )
        .ok_or(ErrorKind::Ref)?;
        let r = table.location.first.row + table.first_data_row + row as u32;
        let c = table.location.first.col + table.first_data_col + col as u32;
        match book.sheets[table.sheet].value_at(CellRef::new(r, c)) {
            Value::Empty => Ok(Value::Number(0.0)),
            v => Ok(v),
        }
    }
}

fn real_fields(axis: &[AxisEntry]) -> Vec<usize> {
    axis.iter()
        .filter_map(|e| match e {
            AxisEntry::Field(f) => Some(*f),
            AxisEntry::Values => None,
        })
        .collect()
}

/// The items given for an axis must be the first fields of it, in order.
fn prefix(items: &[Option<usize>]) -> Result<Vec<usize>, ErrorKind> {
    let mut out = Vec::new();
    let mut gap = false;
    for item in items {
        match item {
            Some(p) if !gap => out.push(*p),
            Some(_) => return Err(ErrorKind::Ref),
            None => gap = true,
        }
    }
    Ok(out)
}

fn find_data(table: &PivotTable, cache: &super::cache::PivotCache, name: &str) -> Option<usize> {
    let same = |a: &str| a.eq_ignore_ascii_case(name.trim());
    table
        .data
        .iter()
        .position(|d| same(&d.name))
        .or_else(|| table.data.iter().position(|d| same(&table.caption(cache, d.field))))
        .or_else(|| table.data.iter().position(|d| same(&cache.fields[d.field].name)))
}

fn find_field(table: &PivotTable, cache: &super::cache::PivotCache, name: &str) -> Option<usize> {
    let same = |a: &str| a.eq_ignore_ascii_case(name.trim());
    (0..cache.fields.len())
        .find(|f| same(&table.caption(cache, *f)))
        .or_else(|| (0..cache.fields.len()).find(|f| same(&cache.fields[*f].name)))
}

/// The position among a field's items of the one a value names.
fn find_item(
    table: &PivotTable,
    cache: &super::cache::PivotCache,
    field: usize,
    item: &Value,
) -> Option<usize> {
    let items = cache.fields[field].items();
    table.fields[field].items.iter().position(|entry| {
        if let Some(name) = &entry.name {
            if let Value::Text(t) = item {
                return t.eq_ignore_ascii_case(name);
            }
        }
        match (items.get(entry.x), item) {
            (Some(CacheValue::Text(a)), Value::Text(b)) => a.eq_ignore_ascii_case(b),
            (Some(CacheValue::Blank), Value::Text(b)) => b.eq_ignore_ascii_case("(blank)"),
            (Some(CacheValue::Number(a) | CacheValue::Date(a)), Value::Number(b)) => {
                (a - b).abs() < 1e-9
            }
            (Some(CacheValue::Bool(a)), Value::Bool(b)) => a == b,
            (Some(CacheValue::Date(a)), Value::Text(b)) => {
                crate::input::date_time_from_text(b, false).is_some_and(|d| (a - d).abs() < 1e-9)
            }
            (Some(CacheValue::Number(a)), Value::Text(b)) => {
                crate::value::number_to_text(*a) == **b
            }
            _ => false,
        }
    })
}

/// The line that adds up the records of some items: its position in the axis.
fn find_line(
    lines: &[Line],
    items: &[usize],
    d: usize,
    has_values: bool,
    has_fields: bool,
) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    let item_path = |l: &Line| -> Vec<usize> {
        l.path
            .iter()
            .filter_map(|s| match s {
                Seg::Item(p) => Some(*p),
                Seg::Data(_) => None,
            })
            .collect()
    };
    let wanted_data = |l: &Line| -> bool { !has_values || l.data.is_none() || l.data == Some(d) };
    let matches = |l: &Line| -> bool {
        if !l.values || !wanted_data(l) {
            return false;
        }
        match l.kind {
            LineKind::Grand => items.is_empty(),
            LineKind::Subtotal => item_path(l) == items,
            LineKind::Leaf | LineKind::Header => {
                let p = item_path(l);
                p == items
                    && (!items.is_empty() || l.path.iter().all(|s| matches!(s, Seg::Data(_))))
            }
            LineKind::Blank => false,
        }
    };
    // The figure shown for the items: the Automatic subtotal first, then a custom one.
    let best = lines
        .iter()
        .position(|l| matches(l) && l.func.is_none())
        .or_else(|| lines.iter().position(matches));
    if best.is_none() && items.is_empty() && !has_fields {
        // An axis with no fields has one line that holds the figures.
        return lines.iter().position(|l| l.kind == LineKind::Leaf && wanted_data(l));
    }
    best
}
