//! The outline of a sheet: groups of rows and columns that fold away behind their summary row or column.
//!
//! A row or column belongs to as many groups as its outline level says, one to seven. The summary line of
//! a group is the line next to its detail on the side the sheet's settings name (below or above for rows,
//! right or left for columns). A group is collapsed by hiding its detail lines and setting `collapsed` on
//! its summary line, and that flag is all that remembers it: a group is not an object, it is a run of
//! lines whose level is deeper than the summary line's.
//!
//! These functions change a [`Sheet`] only. `Session` in [`crate::edit`] wraps each in an undo step.

use std::collections::BTreeSet;

use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::formula::Expr;
use crate::model::{ColumnInfo, RowInfo, Sheet};

/// The deepest level a row or a column has. Excel calls the levels one to eight because level one is the
/// summary at the top; the deepest detail is outline level seven.
pub const DEEPEST: u8 = 7;

/// The most lines one grouping step walks, so a click on a header of the whole sheet cannot stall.
const MOST_LINES: u32 = 100_000;

/// Rows or columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dim {
    Rows,
    Columns,
}

impl Dim {
    fn limit(self) -> u32 {
        match self {
            Dim::Rows => MAX_ROWS,
            Dim::Columns => MAX_COLS,
        }
    }
}

/// What the outline knows about one row or column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Line {
    pub level: u8,
    pub hidden: bool,
    pub collapsed: bool,
}

/// The outline state of a row or column.
pub fn line(sheet: &Sheet, dim: Dim, index: u32) -> Line {
    match dim {
        Dim::Rows => sheet.rows.get(&index).map_or(Line::default(), |r| Line {
            level: r.outline,
            hidden: r.hidden,
            collapsed: r.collapsed,
        }),
        Dim::Columns => sheet.column(index).map_or(Line::default(), |c| Line {
            level: c.outline,
            hidden: c.hidden,
            collapsed: c.collapsed,
        }),
    }
}

/// Change the outline state of one row or column.
fn change(sheet: &mut Sheet, dim: Dim, index: u32, apply: impl FnOnce(&mut Line)) {
    let mut state = line(sheet, dim, index);
    apply(&mut state);
    match dim {
        Dim::Rows => {
            let info = sheet.rows.entry(index).or_default();
            info.outline = state.level;
            info.hidden = state.hidden;
            info.collapsed = state.collapsed;
            if *info == RowInfo::default() {
                sheet.rows.remove(&index);
            }
        }
        Dim::Columns => sheet.edit_column(index, |c| {
            c.outline = state.level;
            c.hidden = state.hidden;
            c.collapsed = state.collapsed;
        }),
    }
}

/// The indexes of the rows or columns that hold any outline information, in order.
fn marked(sheet: &Sheet, dim: Dim) -> Vec<u32> {
    match dim {
        Dim::Rows => sheet
            .rows
            .iter()
            .filter(|(_, r)| r.outline > 0 || r.collapsed || r.hidden)
            .map(|(i, _)| *i)
            .collect(),
        Dim::Columns => sheet
            .columns
            .iter()
            .filter(|c| c.outline > 0 || c.collapsed || c.hidden)
            .flat_map(|c| c.first..=c.last)
            .collect(),
    }
}

/// The deepest level in use among the rows or the columns.
pub fn deepest(sheet: &Sheet, dim: Dim) -> u8 {
    match dim {
        Dim::Rows => sheet.rows.values().map(|r| r.outline).max().unwrap_or(0),
        Dim::Columns => sheet.columns.iter().map(|c| c.outline).max().unwrap_or(0),
    }
}

/// Whether the summary line is after its detail: below the rows, to the right of the columns.
pub fn summary_after(sheet: &Sheet, dim: Dim) -> bool {
    match dim {
        Dim::Rows => sheet.outline.summary_below,
        Dim::Columns => sheet.outline.summary_right,
    }
}

/// Merge neighbouring column entries that say the same thing, so a file lists a run once.
pub fn merge_column_runs(sheet: &mut Sheet) {
    sheet.columns.sort_by_key(|c| c.first);
    let mut merged: Vec<ColumnInfo> = Vec::with_capacity(sheet.columns.len());
    for c in sheet.columns.drain(..) {
        match merged.last_mut() {
            Some(last)
                if last.last + 1 == c.first
                    && ColumnInfo { first: 0, last: 0, ..last.clone() }
                        == ColumnInfo { first: 0, last: 0, ..c.clone() } =>
            {
                last.last = c.last;
            }
            _ => merged.push(c),
        }
    }
    sheet.columns = merged;
}

// ---------------------------------------------------------------------------------------------------
// Grouping.

/// Group some rows or columns, one level deeper than they are now.
pub fn group(sheet: &mut Sheet, dim: Dim, first: u32, last: u32) -> Result<(), String> {
    if first > last || last >= dim.limit() {
        return Err("Cannot group that selection.".to_owned());
    }
    if last - first >= MOST_LINES {
        return Err("That selection is too large to group.".to_owned());
    }
    if (first..=last).any(|i| line(sheet, dim, i).level >= DEEPEST) {
        return Err("Cannot group that selection: an outline has at most eight levels.".to_owned());
    }
    for i in first..=last {
        change(sheet, dim, i, |l| l.level += 1);
    }
    finish(sheet, dim);
    Ok(())
}

/// Take some rows or columns one level out of their groups. Hidden rows stay hidden and a summary line
/// keeps its collapsed mark, as Excel leaves them.
pub fn ungroup(sheet: &mut Sheet, dim: Dim, first: u32, last: u32) -> Result<(), String> {
    let grouped: Vec<u32> = marked(sheet, dim)
        .into_iter()
        .filter(|i| (first..=last).contains(i) && line(sheet, dim, *i).level > 0)
        .collect();
    if grouped.is_empty() {
        return Err("Cannot ungroup that selection.".to_owned());
    }
    for i in grouped {
        change(sheet, dim, i, |l| l.level -= 1);
    }
    finish(sheet, dim);
    Ok(())
}

/// Tidy the columns after a change.
fn finish(sheet: &mut Sheet, dim: Dim) {
    if dim == Dim::Columns {
        merge_column_runs(sheet);
    }
}

/// The detail lines of the group whose summary line is `summary`: the run of lines next to it, on the
/// detail side, whose level is deeper than its own.
pub fn extent(sheet: &Sheet, dim: Dim, summary: u32) -> Option<(u32, u32)> {
    let depth = line(sheet, dim, summary).level;
    if summary_after(sheet, dim) {
        let mut j = summary;
        while j > 0 && line(sheet, dim, j - 1).level > depth {
            j -= 1;
        }
        (j < summary).then(|| (j, summary - 1))
    } else {
        let mut j = summary;
        while j + 1 < dim.limit() && line(sheet, dim, j + 1).level > depth {
            j += 1;
        }
        (j > summary).then(|| (summary + 1, j))
    }
}

/// Collapse or expand the group whose summary line is `summary`.
pub fn set_expanded(
    sheet: &mut Sheet,
    dim: Dim,
    summary: u32,
    expanded: bool,
) -> Result<(), String> {
    let Some((first, last)) = extent(sheet, dim, summary) else {
        return Err("Select a single summary row or column of a group.".to_owned());
    };
    change(sheet, dim, summary, |l| l.collapsed = !expanded);
    if !expanded {
        for i in first..=last {
            change(sheet, dim, i, |l| l.hidden = true);
        }
    } else {
        // A line stays hidden when another collapsed group inside this one covers it.
        let covering = collapsed_extents(sheet, dim);
        for i in first..=last {
            let kept = covering.iter().any(|(a, b)| (*a..=*b).contains(&i));
            change(sheet, dim, i, |l| l.hidden = kept);
        }
    }
    finish(sheet, dim);
    Ok(())
}

/// The detail lines of every group that is collapsed.
fn collapsed_extents(sheet: &Sheet, dim: Dim) -> Vec<(u32, u32)> {
    marked(sheet, dim)
        .into_iter()
        .filter(|i| line(sheet, dim, *i).collapsed)
        .filter_map(|i| extent(sheet, dim, i))
        .collect()
}

/// Show the first `levels` levels and fold the rest: level 1 shows only the lines outside every group.
/// Lines outside the outline keep whatever hidden state they have.
pub fn show_levels(sheet: &mut Sheet, dim: Dim, levels: u8) {
    if levels == 0 {
        return;
    }
    let marks = marked(sheet, dim);
    for i in &marks {
        change(sheet, dim, *i, |l| {
            l.collapsed = false;
            if l.level > 0 {
                l.hidden = l.level >= levels;
            }
        });
    }
    let folded: BTreeSet<u32> =
        marks.into_iter().filter(|i| line(sheet, dim, *i).level >= levels).collect();
    for (first, last) in runs(&folded) {
        let summary =
            if summary_after(sheet, dim) { last.checked_add(1) } else { first.checked_sub(1) };
        if let Some(s) = summary.filter(|s| *s < dim.limit()) {
            change(sheet, dim, s, |l| l.collapsed = true);
        }
    }
    finish(sheet, dim);
}

/// Runs of consecutive numbers in a set.
fn runs(set: &BTreeSet<u32>) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &i in set {
        match out.last_mut() {
            Some((_, last)) if *last + 1 == i => *last = i,
            _ => out.push((i, i)),
        }
    }
    out
}

/// Remove the grouping of some rows or columns, showing the lines it hid.
pub fn clear(sheet: &mut Sheet, dim: Dim, first: u32, last: u32) {
    let inside: Vec<u32> =
        marked(sheet, dim).into_iter().filter(|i| (first..=last).contains(i)).collect();
    for i in inside {
        change(sheet, dim, i, |l| {
            if l.level > 0 {
                l.hidden = false;
            }
            l.level = 0;
            l.collapsed = false;
        });
    }
    finish(sheet, dim);
}

// ---------------------------------------------------------------------------------------------------
// Auto outline.

/// A formula's reference on the sheet that holds it: the rows and columns it covers.
#[derive(Debug, Clone, Copy)]
struct Reach {
    first_row: u32,
    last_row: u32,
    first_col: u32,
    last_col: u32,
}

/// Every same sheet reference of every formula, with the cell the formula is in.
fn references(sheet: &Sheet, within: Option<Range>) -> Vec<((u32, u32), Reach)> {
    let mut out = Vec::new();
    for (&at, cell) in &sheet.cells {
        let Some(f) = &cell.formula else { continue };
        if within.is_some_and(|w| !w.contains(CellRef::new(at.0, at.1))) {
            continue;
        }
        f.expr.walk(&mut |e: &Expr| {
            let Expr::Ref(r) = e else { return };
            let own = r.sheet.as_ref().is_none_or(|n| n.eq_ignore_ascii_case(&sheet.name));
            if !own || r.sheet_last.is_some() {
                return;
            }
            let range = r.range();
            out.push((
                at,
                Reach {
                    first_row: range.first.row,
                    last_row: range.last.row,
                    first_col: range.first.col,
                    last_col: range.last.col,
                },
            ));
        });
    }
    out
}

/// A summary line the formulas imply and the lines it sums.
#[derive(Debug, Clone, Copy)]
struct Summary {
    line: u32,
    first: u32,
    last: u32,
}

/// Whether a row or column holds a formula, which makes it a summary of its own and not a gap.
fn holds_formula(sheet: &Sheet, dim: Dim, index: u32) -> bool {
    sheet.cells.iter().any(|((r, c), cell)| {
        cell.formula.is_some()
            && match dim {
                Dim::Rows => *r == index,
                Dim::Columns => *c == index,
            }
    })
}

/// The summary rows or columns the formulas imply, as lines with the lines they sum.
fn find_summaries(
    sheet: &Sheet,
    refs: &[((u32, u32), Reach)],
    dim: Dim,
) -> (Vec<Summary>, Vec<Summary>) {
    use std::collections::BTreeMap;
    // For each formula line: the nearest and the farthest line summed, on each side, and the lines seen.
    #[derive(Default)]
    struct Seen {
        before: Option<(u32, u32)>,
        after: Option<(u32, u32)>,
        lines: BTreeSet<u32>,
    }
    let mut seen: BTreeMap<u32, Seen> = BTreeMap::new();
    for &((row, col), r) in refs {
        let (at, lo, hi) = match dim {
            Dim::Rows => (row, r.first_row, r.last_row),
            Dim::Columns => (col, r.first_col, r.last_col),
        };
        if hi - lo > 20_000 || (lo..=hi).contains(&at) {
            continue;
        }
        let entry = seen.entry(at).or_default();
        let side = if hi < at { &mut entry.before } else { &mut entry.after };
        *side = Some(match *side {
            Some((a, b)) => (a.min(lo), b.max(hi)),
            None => (lo, hi),
        });
        entry.lines.extend(lo..=hi);
    }
    let (mut below, mut above) = (Vec::new(), Vec::new());
    for (at, s) in seen {
        if s.lines.len() < 2 {
            continue;
        }
        if let Some((lo, hi)) = s.before {
            if (hi + 1..at).all(|g| !holds_formula(sheet, dim, g)) {
                below.push(Summary { line: at, first: lo, last: at - 1 });
            }
        }
        if let Some((lo, hi)) = s.after {
            if (at + 1..lo).all(|g| !holds_formula(sheet, dim, g)) {
                above.push(Summary { line: at, first: at + 1, last: hi });
            }
        }
    }
    (below, above)
}

/// Widen each group to take in the groups of the summary lines inside it, which are wider than their
/// own summary's reach (a total of subtotals takes in the detail of the subtotals).
fn widen_groups(groups: &mut [Summary], after: bool) {
    loop {
        let mut changed = false;
        for i in 0..groups.len() {
            for j in 0..groups.len() {
                let inner = groups[j];
                let outer = groups[i];
                if i == j || !(outer.first..=outer.last).contains(&inner.line) {
                    continue;
                }
                if after && inner.first < outer.first {
                    groups[i].first = inner.first;
                    changed = true;
                } else if !after && inner.last > outer.last {
                    groups[i].last = inner.last;
                    changed = true;
                }
            }
        }
        if !changed {
            return;
        }
    }
}

/// Replace the outline with the one the formulas imply: a group for every row or column that sums the
/// lines before it (or after it), and the setting for where the summary sits made to match.
pub fn auto_outline(sheet: &mut Sheet, within: Option<Range>) -> Result<(), String> {
    let refs = references(sheet, within);
    let mut plan = Vec::new();
    for dim in [Dim::Rows, Dim::Columns] {
        let (below, above) = find_summaries(sheet, &refs, dim);
        let after = match (below.is_empty(), above.is_empty()) {
            (false, true) => true,
            (true, false) => false,
            _ => summary_after(sheet, dim),
        };
        let mut groups = if after { below } else { above };
        widen_groups(&mut groups, after);
        match dim {
            Dim::Rows if !groups.is_empty() => sheet.outline.summary_below = after,
            Dim::Columns if !groups.is_empty() => sheet.outline.summary_right = after,
            _ => {}
        }
        plan.push((dim, groups));
    }
    let before = sheet.clone();
    let area = within.unwrap_or_else(|| {
        Range::new(CellRef::new(0, 0), CellRef::new(MAX_ROWS - 1, MAX_COLS - 1))
    });
    clear(sheet, Dim::Rows, area.first.row, area.last.row);
    clear(sheet, Dim::Columns, area.first.col, area.last.col);
    for (dim, groups) in plan {
        for g in groups {
            if let Err(e) = group(sheet, dim, g.first, g.last) {
                *sheet = before;
                return Err(e);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------
// Settings.

/// Move the summary rows and columns, as the Settings dialog of the outline does. Collapsed groups keep
/// their detail hidden, and each gains the mark on the line that is now their summary.
pub fn set_summary(sheet: &mut Sheet, rows_below: Option<bool>, columns_right: Option<bool>) {
    if let Some(below) = rows_below {
        if sheet.outline.summary_below != below {
            sheet.outline.summary_below = below;
            mark_folded_groups(sheet, Dim::Rows);
        }
    }
    if let Some(right) = columns_right {
        if sheet.outline.summary_right != right {
            sheet.outline.summary_right = right;
            mark_folded_groups(sheet, Dim::Columns);
        }
    }
}

/// Put `collapsed` on the line that is the summary of every run of hidden grouped lines.
fn mark_folded_groups(sheet: &mut Sheet, dim: Dim) {
    let hidden: BTreeSet<u32> = marked(sheet, dim)
        .into_iter()
        .filter(|i| {
            let l = line(sheet, dim, *i);
            l.hidden && l.level > 0
        })
        .collect();
    for (first, last) in runs(&hidden) {
        let summary =
            if summary_after(sheet, dim) { last.checked_add(1) } else { first.checked_sub(1) };
        if let Some(s) = summary.filter(|s| *s < dim.limit()) {
            change(sheet, dim, s, |l| l.collapsed = true);
        }
    }
    finish(sheet, dim);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(sheet: &Sheet, first: u32, last: u32) -> Vec<u8> {
        (first..=last).map(|i| line(sheet, Dim::Rows, i).level).collect()
    }

    #[test]
    fn groups_nest_and_stop_at_the_deepest_level() {
        let mut s = Sheet::new("S");
        group(&mut s, Dim::Rows, 1, 7).unwrap();
        group(&mut s, Dim::Rows, 2, 4).unwrap();
        assert_eq!(levels(&s, 1, 7), vec![1, 2, 2, 2, 1, 1, 1]);
        for _ in 0..5 {
            group(&mut s, Dim::Rows, 3, 3).unwrap();
        }
        assert_eq!(line(&s, Dim::Rows, 3).level, 7);
        assert!(group(&mut s, Dim::Rows, 3, 3).is_err());
        assert_eq!(line(&s, Dim::Rows, 3).level, 7);
    }

    #[test]
    fn collapsing_hides_the_detail_and_marks_the_summary() {
        let mut s = Sheet::new("S");
        group(&mut s, Dim::Rows, 2, 4).unwrap();
        set_expanded(&mut s, Dim::Rows, 5, false).unwrap();
        assert!(line(&s, Dim::Rows, 3).hidden);
        assert!(line(&s, Dim::Rows, 5).collapsed);
        set_expanded(&mut s, Dim::Rows, 5, true).unwrap();
        assert!(!line(&s, Dim::Rows, 3).hidden);
        assert!(!line(&s, Dim::Rows, 5).collapsed);
        assert!(set_expanded(&mut s, Dim::Rows, 9, false).is_err());
    }

    #[test]
    fn expanding_an_outer_group_keeps_an_inner_one_folded() {
        let mut s = Sheet::new("S");
        group(&mut s, Dim::Rows, 1, 7).unwrap();
        group(&mut s, Dim::Rows, 2, 4).unwrap();
        set_expanded(&mut s, Dim::Rows, 5, false).unwrap();
        set_expanded(&mut s, Dim::Rows, 8, false).unwrap();
        set_expanded(&mut s, Dim::Rows, 8, true).unwrap();
        assert!(!line(&s, Dim::Rows, 1).hidden);
        assert!(line(&s, Dim::Rows, 3).hidden);
        assert!(line(&s, Dim::Rows, 5).collapsed);
    }

    #[test]
    fn showing_a_level_folds_what_is_deeper() {
        let mut s = Sheet::new("S");
        group(&mut s, Dim::Rows, 1, 7).unwrap();
        group(&mut s, Dim::Rows, 2, 4).unwrap();
        show_levels(&mut s, Dim::Rows, 2);
        assert_eq!(
            (1..=7).map(|i| line(&s, Dim::Rows, i).hidden).collect::<Vec<_>>(),
            vec![false, true, true, true, false, false, false]
        );
        assert!(line(&s, Dim::Rows, 5).collapsed);
        show_levels(&mut s, Dim::Rows, 9);
        assert!(
            (0..=9).all(|i| !line(&s, Dim::Rows, i).hidden && !line(&s, Dim::Rows, i).collapsed)
        );
    }

    #[test]
    fn columns_group_and_merge_into_runs() {
        let mut s = Sheet::new("S");
        group(&mut s, Dim::Columns, 1, 3).unwrap();
        assert_eq!(s.columns.len(), 1);
        assert_eq!((s.columns[0].first, s.columns[0].last, s.columns[0].outline), (1, 3, 1));
        ungroup(&mut s, Dim::Columns, 1, 3).unwrap();
        assert!(s.columns.is_empty());
    }
}
