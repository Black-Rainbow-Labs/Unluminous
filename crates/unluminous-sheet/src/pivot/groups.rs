//! Making and taking away groups: dates by period, numbers by range, and items by hand.

use super::cache::{CacheField, CacheValue, FieldGroup, GroupBy, PivotCache, RangePr};
use crate::numfmt::Format;
use crate::value::Value;

const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The text of a date or a number in the format of a field, for the names of group items.
fn shown(cache: &PivotCache, field: usize, n: f64, date1904: bool) -> String {
    let f = &cache.fields[field];
    let code = match &f.num_fmt_code {
        Some(code) => code.clone(),
        None => crate::styles::builtin_format(f.num_fmt_id).unwrap_or("General").to_owned(),
    };
    Format::parse(&code).render(&Value::Number(n), date1904, 255).text
}

/// The name of the field a period makes: `Months (Date)`.
fn group_name(cache: &PivotCache, base: usize, by: GroupBy) -> String {
    format!("{} ({})", by.field_word(), cache.fields[base].name)
}

fn rank(g: GroupBy) -> u8 {
    match g {
        GroupBy::Range => 0,
        GroupBy::Seconds => 1,
        GroupBy::Minutes => 2,
        GroupBy::Hours => 3,
        GroupBy::Days => 4,
        GroupBy::Months => 5,
        GroupBy::Quarters => 6,
        GroupBy::Years => 7,
    }
}

/// The group items of a grouping by a period of a date.
fn date_items(
    by: GroupBy,
    start: f64,
    end: f64,
    interval: f64,
    first: &str,
    last: &str,
    date1904: bool,
) -> Vec<CacheValue> {
    let mut items = vec![CacheValue::text(&format!("<{first}"))];
    match by {
        GroupBy::Years => {
            let (y0, _, _) = crate::dates::date(start, date1904).unwrap_or((1900, 1, 1));
            let (y1, _, _) = crate::dates::date(end, date1904).unwrap_or((1900, 1, 1));
            (y0..=y1).for_each(|y| items.push(CacheValue::text(&y.to_string())));
        }
        GroupBy::Quarters => (1..=4).for_each(|q| items.push(CacheValue::text(&format!("Qtr{q}")))),
        GroupBy::Months => MONTHS.iter().for_each(|m| items.push(CacheValue::text(m))),
        GroupBy::Days if interval <= 1.0 => {
            for m in 1..=12u32 {
                for d in 1..=crate::dates::days_in_month(2000, m) {
                    items.push(CacheValue::text(&format!("{d}-{}", MONTHS[m as usize - 1])));
                }
            }
        }
        GroupBy::Hours => (0..24).for_each(|h| items.push(CacheValue::text(&h.to_string()))),
        GroupBy::Minutes | GroupBy::Seconds => {
            (0..60).for_each(|h| items.push(CacheValue::text(&h.to_string())))
        }
        GroupBy::Days | GroupBy::Range => {}
    }
    items.push(CacheValue::text(&format!(">{last}")));
    items
}

/// Group a date field by some periods. Each period becomes a field of its own, the finest first, and the
/// base field keeps its items and points at the coarsest. Answers the new fields, finest first.
pub fn group_dates(
    cache: &mut PivotCache,
    field: usize,
    by: &[GroupBy],
    start: Option<f64>,
    end: Option<f64>,
    step: Option<f64>,
    date1904: bool,
) -> Result<Vec<usize>, String> {
    if !cache.fields[field].database || !cache.is_date_field(field) {
        return Err("Cannot group that selection.".to_owned());
    }
    let (lo, hi) = cache.number_bounds(field).ok_or("The field has no dates.")?;
    let auto_start = start.is_none();
    let auto_end = end.is_none();
    let start = start.unwrap_or(lo.floor());
    let end = end.unwrap_or(hi.floor() + 1.0);
    cache.ensure_shared(field);
    let mut order: Vec<GroupBy> = by.to_vec();
    order.sort_by_key(|g| rank(*g));
    order.dedup();
    let mut added = Vec::new();
    for g in order {
        let interval = if g == GroupBy::Days { step.unwrap_or(1.0) } else { 1.0 };
        let (first, last) =
            (shown(cache, field, start, date1904), shown(cache, field, end, date1904));
        let mut items = date_items(g, start, end, interval, &first, &last, date1904);
        if g == GroupBy::Days && interval > 1.0 {
            let mut at = start;
            while at <= end {
                let next = (at + interval - 1.0).min(end);
                let label = format!(
                    "{} - {}",
                    shown(cache, field, at, date1904),
                    shown(cache, field, next, date1904)
                );
                items.insert(items.len() - 1, CacheValue::text(&label));
                at += interval;
            }
        }
        let mut group_field = CacheField::new(&group_name(cache, field, g));
        group_field.database = false;
        group_field.group = Some(FieldGroup {
            par: None,
            base: Some(field),
            range: Some(RangePr {
                group_by: g,
                start_date: Some(start),
                end_date: Some(end),
                start_num: None,
                end_num: None,
                interval,
                auto_start,
                auto_end,
            }),
            discrete: None,
            items,
        });
        cache.fields.push(group_field);
        cache.records.iter_mut().for_each(|r| r.push(CacheValue::Blank));
        added.push(cache.fields.len() - 1);
    }
    let coarsest = *added.last().ok_or("Choose at least one period.")?;
    cache.fields[field].group = Some(FieldGroup { par: Some(coarsest), ..Default::default() });
    Ok(added)
}

/// Group a number field by ranges, in place: the field's items become the ranges.
pub fn group_numbers(
    cache: &mut PivotCache,
    field: usize,
    start: Option<f64>,
    end: Option<f64>,
    step: f64,
    date1904: bool,
) -> Result<(), String> {
    if !cache.fields[field].database || step <= 0.0 {
        return Err("Cannot group that selection.".to_owned());
    }
    let (lo, hi) = cache.number_bounds(field).ok_or("The field has no numbers.")?;
    let auto_start = start.is_none();
    let auto_end = end.is_none();
    let start = start.unwrap_or(lo);
    let end = end.unwrap_or(hi);
    cache.ensure_shared(field);
    let has_blank = cache.records.iter().any(|r| r[field].is_blank());
    let blank_note = if has_blank { " or (blank)" } else { "" };
    // With the start left to Excel the first range is the blanks, if there are any.
    let first = if auto_start && has_blank {
        "(blank)".to_owned()
    } else {
        format!("<{}{blank_note}", shown(cache, field, start, date1904))
    };
    let mut items = vec![CacheValue::text(&first)];
    let whole = step.fract() == 0.0 && start.fract() == 0.0;
    let mut at = start;
    while at <= end {
        let upper = if whole { at + step - 1.0 } else { at + step };
        // The last range runs up to an end that was given, and a whole range when it was not.
        let upper = if !auto_end && at + step >= end { end } else { upper };
        let label = format!(
            "{}-{}",
            shown(cache, field, at, date1904),
            shown(cache, field, upper, date1904)
        );
        items.push(CacheValue::text(&label));
        at += step;
    }
    items.push(CacheValue::text(&format!(">{}", shown(cache, field, end, date1904))));
    cache.fields[field].group = Some(FieldGroup {
        par: None,
        base: Some(field),
        range: Some(RangePr {
            group_by: GroupBy::Range,
            start_date: None,
            end_date: None,
            start_num: Some(start),
            end_num: Some(end),
            interval: step,
            auto_start,
            auto_end,
        }),
        discrete: None,
        items,
    });
    Ok(())
}

/// Group chosen items of a field into named groups, in a field of its own that sits outside the field. The
/// members are indexes among the field's shared items. Answers the group field.
pub fn group_items(
    cache: &mut PivotCache,
    field: usize,
    groups: &[(Option<String>, Vec<usize>)],
) -> Result<usize, String> {
    cache.ensure_shared(field);
    let shared_len = cache.fields[field].shared.len();
    let existing = (0..cache.fields.len()).find(|&i| {
        cache.fields[i]
            .group
            .as_ref()
            .is_some_and(|g| g.base == Some(field) && g.discrete.is_some())
    });
    let target = match existing {
        Some(i) => i,
        None => {
            let mut group_field = CacheField::new(&format!("{}2", cache.fields[field].name));
            group_field.database = false;
            group_field.group = Some(FieldGroup {
                par: None,
                base: Some(field),
                range: None,
                discrete: Some((0..shared_len).collect()),
                items: cache.fields[field].shared.clone(),
            });
            cache.fields.push(group_field);
            cache.records.iter_mut().for_each(|r| r.push(CacheValue::Blank));
            let index = cache.fields.len() - 1;
            let base_group = cache.fields[field].group.take().unwrap_or_default();
            cache.fields[field].group = Some(FieldGroup { par: Some(index), ..base_group });
            index
        }
    };
    let group = cache.fields[target].group.as_mut().ok_or("Cannot group that selection.")?;
    let mut map = group.discrete.clone().unwrap_or_default();
    let mut items = group.items.clone();
    for (name, members) in groups {
        let name = match name {
            Some(n) => n.clone(),
            None => (1..)
                .map(|n| format!("Group{n}"))
                .find(|n| {
                    !items
                        .iter()
                        .any(|i| matches!(i, CacheValue::Text(t) if t.eq_ignore_ascii_case(n)))
                })
                .unwrap_or_default(),
        };
        items.push(CacheValue::text(&name));
        let new = items.len() - 1;
        for &m in members {
            if m >= shared_len {
                return Err("Cannot group that selection.".to_owned());
            }
            map[m] = new;
        }
    }
    // Items nothing maps to any more are dropped, and the rest are renumbered.
    let mut used = vec![false; items.len()];
    map.iter().for_each(|&m| used[m] = true);
    let mut renumber = vec![0usize; items.len()];
    let mut kept = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        if used[i] {
            renumber[i] = kept.len();
            kept.push(item);
        }
    }
    map.iter_mut().for_each(|m| *m = renumber[*m]);
    group.discrete = Some(map);
    group.items = kept;
    Ok(target)
}

/// Take the grouping of a field away. Answers the group fields that went, which the caller takes out of
/// its tables too.
pub fn ungroup(cache: &mut PivotCache, field: usize) -> Result<Vec<usize>, String> {
    let group = cache.fields[field].group.clone().ok_or("The selection is not grouped.")?;
    if group.base == Some(field) {
        cache.fields[field].group = None;
        return Ok(Vec::new());
    }
    let base = group.base.unwrap_or(field);
    let removed: Vec<usize> = (0..cache.fields.len())
        .filter(|&i| {
            i != base && cache.fields[i].group.as_ref().is_some_and(|g| g.base == Some(base))
        })
        .collect();
    for &i in removed.iter().rev() {
        cache.fields.remove(i);
        cache.records.iter_mut().for_each(|r| {
            r.remove(i);
        });
    }
    cache.fields[base].group = None;
    Ok(removed)
}
