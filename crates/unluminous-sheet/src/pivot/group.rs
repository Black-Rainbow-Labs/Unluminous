//! Grouping: dates by period, numbers by range, and items by hand.
//!
//! A grouped field has `groupItems` instead of the shared items, and each record belongs to the group item
//! its value falls in. A group field made from another field (a field of months made from a date field) has
//! no column in the records: its item is worked out from the base field's value.

use std::collections::HashMap;

use super::cache::{CacheValue, PivotCache};

/// For each record, the item it has in a field: an index into the field's group items when it has them,
/// into its shared items otherwise.
pub fn record_items(cache: &PivotCache, field: usize) -> Vec<Option<usize>> {
    let f = &cache.fields[field];
    if f.has_group_items() {
        return grouped_items(cache, field);
    }
    let index: HashMap<String, usize> =
        f.shared.iter().enumerate().map(|(i, v)| (v.key(), i)).collect();
    cache.records.iter().map(|r| index.get(&r[field].key()).copied()).collect()
}

/// The items of a field that has group items.
fn grouped_items(cache: &PivotCache, field: usize) -> Vec<Option<usize>> {
    let f = &cache.fields[field];
    let Some(group) = &f.group else { return vec![None; cache.records.len()] };
    let base = group.base.unwrap_or(field);
    if let Some(map) = &group.discrete {
        let shared: HashMap<String, usize> =
            cache.fields[base].shared.iter().enumerate().map(|(i, v)| (v.key(), i)).collect();
        return cache
            .records
            .iter()
            .map(|r| shared.get(&r[base].key()).and_then(|i| map.get(*i).copied()))
            .collect();
    }
    let Some(range) = &group.range else { return vec![None; cache.records.len()] };
    cache
        .records
        .iter()
        .map(|r| match r[base].number() {
            Some(n) => Some(range_index(range, n, &group.items)),
            None => (r[base].is_blank() && range.group_by == super::cache::GroupBy::Range)
                .then_some(0)
                .or_else(|| r[base].is_blank().then_some(0)),
        })
        .collect()
}

/// Which group item a number or a date falls in.
fn range_index(range: &super::cache::RangePr, n: f64, items: &[CacheValue]) -> usize {
    use super::cache::GroupBy;
    let count = items.len();
    let (start, end) = match range.group_by {
        GroupBy::Range => (range.start_num.unwrap_or(0.0), range.end_num.unwrap_or(0.0)),
        _ => (range.start_date.unwrap_or(0.0), range.end_date.unwrap_or(0.0)),
    };
    if n < start && !range.auto_start {
        return 0;
    }
    if n > end && !range.auto_end {
        return count - 1;
    }
    if range.group_by == GroupBy::Years {
        // A year is found by its name, since years added by a refresh come after the others.
        let (y, _, _) = crate::dates::date(n, false).unwrap_or((1900, 1, 1));
        if let Some(at) = items.iter().position(|v| item_text(v) == y.to_string()) {
            return at;
        }
    }
    match range.group_by {
        GroupBy::Range => {
            let bin = ((n - start) / range.interval).floor() as usize;
            (1 + bin).min(count - 2)
        }
        GroupBy::Years => {
            let (y0, _, _) = crate::dates::date(start, false).unwrap_or((1900, 1, 1));
            let (y, _, _) = crate::dates::date(n, false).unwrap_or((1900, 1, 1));
            ((1 + y - y0) as usize).min(count - 1)
        }
        GroupBy::Months => {
            let (_, m, _) = crate::dates::date(n, false).unwrap_or((1900, 1, 1));
            m as usize
        }
        GroupBy::Quarters => {
            let (_, m, _) = crate::dates::date(n, false).unwrap_or((1900, 1, 1));
            ((m - 1) / 3 + 1) as usize
        }
        GroupBy::Days => {
            if range.interval <= 1.0 {
                let (y, m, d) = crate::dates::date(n, false).unwrap_or((1900, 1, 1));
                let first = crate::dates::serial(2000, 1, 1, false).unwrap_or(0.0);
                let day = crate::dates::serial(2000, m as i64, d as i64, false).unwrap_or(first);
                let _ = y;
                (1 + (day - first) as usize).min(count - 1)
            } else {
                (1 + ((n - start) / range.interval).floor() as usize).min(count - 2)
            }
        }
        GroupBy::Hours => {
            let (h, _, _, _) = crate::dates::time(n);
            1 + h as usize
        }
        GroupBy::Minutes => {
            let (_, mi, _, _) = crate::dates::time(n);
            1 + mi as usize
        }
        GroupBy::Seconds => {
            let (_, _, s, _) = crate::dates::time(n);
            1 + s as usize
        }
    }
}

/// The value of a group item as text, for a label.
pub fn item_text(value: &CacheValue) -> String {
    match value {
        CacheValue::Text(t) => t.to_string(),
        CacheValue::Blank => "(blank)".to_owned(),
        other => crate::eval::to_text(&other.to_value()).unwrap_or_default(),
    }
}
