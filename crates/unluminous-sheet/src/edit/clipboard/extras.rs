//! What a paste carries besides cells: notes, data validation, conditional formats, merged cells, links,
//! column widths and row heights.

use super::paste::{Cx, Plan};
use super::snapshot::import_dxfs;
use super::{ClipRules, PasteKind};
use crate::coord::{CellRef, Range};
use crate::edit::{Clip, Session};
use crate::formula;
use crate::model::{CfRule, ConditionalFormat, Hyperlink, Validation};

impl Session {
    /// Apply the parts of a paste that are not cells, as far as the kind of paste takes them.
    pub(super) fn paste_extras(&mut self, cx: &mut Cx) {
        let kind = cx.opts.kind;
        let everything = kind.is_everything();
        if everything || kind == PasteKind::Comments {
            self.paste_notes(cx);
        }
        if everything || kind == PasteKind::Validation {
            self.paste_validation(cx);
        }
        if everything || kind == PasteKind::Formats {
            self.paste_conditional(cx);
            self.paste_merges(cx);
        }
        if everything {
            self.paste_links(cx);
        }
        if kind == PasteKind::ColumnWidths || (everything && cx.clip.extras.whole_columns) {
            self.paste_widths(cx);
        }
        if everything && cx.clip.extras.whole_rows {
            self.paste_heights(cx);
        }
    }

    /// Notes under the destination go, and the notes copied take their place.
    fn paste_notes(&mut self, cx: &Cx) {
        let (plan, clip) = (cx.plan, cx.clip);
        let s = &mut self.book.sheets[cx.sheet];
        s.notes.retain(|(r, c), _| !plan.target.contains(CellRef::new(*r, *c)));
        for origin in plan.origins() {
            for (r, c, note) in &clip.extras.notes {
                let at = plan.map_range(origin, Range::cell(CellRef::new(*r, *c))).first;
                s.notes.insert((at.row, at.col), note.clone());
            }
        }
        s.notes_changed = true;
    }

    /// Data validation under the destination goes, and what was copied takes its place.
    fn paste_validation(&mut self, cx: &Cx) {
        let (plan, clip) = (cx.plan, cx.clip);
        let s = &mut self.book.sheets[cx.sheet];
        for v in &mut s.validations {
            v.ranges = cut_out(&v.ranges, plan.target);
        }
        s.validations.retain(|v| !v.ranges.is_empty());
        for origin in plan.origins() {
            let (dr, dc) = offset(clip, origin);
            for (rel, validation) in &clip.extras.validations {
                let range = plan.map_range(origin, *rel);
                s.validations.push(moved_validation(validation, range, dr, dc));
            }
        }
    }

    /// Conditional formats under the destination are replaced by the ones copied, or added to for the
    /// kind of paste that merges them.
    fn paste_conditional(&mut self, cx: &Cx) {
        let (plan, clip) = (cx.plan, cx.clip);
        let dxf_map = import_dxfs(&mut self.book.styles, &clip.extras);
        let s = &mut self.book.sheets[cx.sheet];
        if cx.opts.kind != PasteKind::AllMergingConditionalFormats {
            for cf in &mut s.conditional {
                cf.ranges = cut_out(&cf.ranges, plan.target);
            }
            s.conditional.retain(|cf| !cf.ranges.is_empty());
        }
        let mut priority = s.conditional.iter().flat_map(|c| &c.rules).map(|r| r.priority).max().unwrap_or(0);
        for origin in plan.origins() {
            let (dr, dc) = offset(clip, origin);
            for rules in &clip.extras.conditional {
                priority += 1;
                s.conditional.push(moved_rules(rules, plan, origin, (dr, dc), &dxf_map, &mut priority));
            }
        }
    }

    /// Merged cells under the destination go, and the merges copied take their place.
    fn paste_merges(&mut self, cx: &Cx) {
        let (plan, clip) = (cx.plan, cx.clip);
        let s = &mut self.book.sheets[cx.sheet];
        s.merges.retain(|m| !m.intersects(&plan.target));
        for origin in plan.origins() {
            for rel in &clip.extras.merges {
                s.merges.push(plan.map_range(origin, *rel));
            }
        }
    }

    fn paste_links(&mut self, cx: &Cx) {
        let (plan, clip) = (cx.plan, cx.clip);
        let s = &mut self.book.sheets[cx.sheet];
        s.links.retain(|l| !plan.target.intersects(&l.range));
        for origin in plan.origins() {
            for link in &clip.extras.links {
                s.links.push(Hyperlink { range: plan.map_range(origin, link.range), ..link.clone() });
            }
        }
    }

    /// The columns under the destination take the widths of the columns copied.
    fn paste_widths(&mut self, cx: &Cx) {
        let (plan, clip) = (cx.plan, cx.clip);
        let origin = plan.target.first;
        let (_, width) = clip.block_size();
        for offset in 0..width {
            let col = origin.col + offset;
            if col >= crate::coord::MAX_COLS {
                break;
            }
            let source = clip.extras.widths.iter().find(|(o, _)| *o == offset).map(|(_, i)| i.clone());
            self.book.sheets[cx.sheet].edit_column(col, |info| match &source {
                Some(s) => {
                    info.width = s.width;
                    info.custom_width = s.custom_width;
                    info.hidden = s.hidden;
                }
                None => {
                    info.width = None;
                    info.custom_width = false;
                    info.hidden = false;
                }
            });
        }
    }

    /// The rows under the destination take the heights of the rows copied.
    fn paste_heights(&mut self, cx: &Cx) {
        let (plan, clip) = (cx.plan, cx.clip);
        let origin = plan.target.first;
        for (offset, height) in &clip.extras.heights {
            let row = origin.row + offset;
            if row < crate::coord::MAX_ROWS {
                self.book.sheets[cx.sheet].rows.entry(row).or_default().height = Some(*height);
            }
        }
    }
}

/// How far a copy of the block moved from where it was copied, in rows and columns.
fn offset(clip: &Clip, origin: CellRef) -> (i64, i64) {
    (origin.row as i64 - clip.range.first.row as i64, origin.col as i64 - clip.range.first.col as i64)
}

/// The ranges with a hole cut out of them.
fn cut_out(ranges: &[Range], hole: Range) -> Vec<Range> {
    let mut out = Vec::new();
    for r in ranges {
        out.extend(subtract(*r, hole));
    }
    out
}

/// The part of a rectangle that a hole leaves: up to four rectangles.
pub(super) fn subtract(r: Range, hole: Range) -> Vec<Range> {
    let Some(cut) = r.intersection(&hole) else { return vec![r] };
    let mut out = Vec::new();
    if r.first.row < cut.first.row {
        out.push(Range::new(r.first, CellRef::new(cut.first.row - 1, r.last.col)));
    }
    if cut.last.row < r.last.row {
        out.push(Range::new(CellRef::new(cut.last.row + 1, r.first.col), r.last));
    }
    if r.first.col < cut.first.col {
        out.push(Range::new(
            CellRef::new(cut.first.row, r.first.col),
            CellRef::new(cut.last.row, cut.first.col - 1),
        ));
    }
    if cut.last.col < r.last.col {
        out.push(Range::new(
            CellRef::new(cut.first.row, cut.last.col + 1),
            CellRef::new(cut.last.row, r.last.col),
        ));
    }
    out
}

/// A formula's text with its relative references moved, for the formulas data validation and conditional
/// formats carry. Text that is not a formula, such as a list typed in, is left as it is.
fn shifted_text(text: &str, dr: i64, dc: i64) -> String {
    if (dr == 0 && dc == 0) || text.trim_start().starts_with('"') {
        return text.to_owned();
    }
    match formula::parser::parse(text) {
        Ok(expr) => formula::print::print(&formula::shifted(&expr, dr, dc), false),
        Err(_) => text.to_owned(),
    }
}

fn moved_validation(v: &Validation, range: Range, dr: i64, dc: i64) -> Validation {
    Validation {
        ranges: vec![range],
        formula1: v.formula1.as_deref().map(|f| shifted_text(f, dr, dc)),
        formula2: v.formula2.as_deref().map(|f| shifted_text(f, dr, dc)),
        ..v.clone()
    }
}

fn moved_rules(
    rules: &ClipRules,
    plan: &Plan,
    origin: CellRef,
    (dr, dc): (i64, i64),
    dxf_map: &std::collections::BTreeMap<u32, u32>,
    priority: &mut i32,
) -> ConditionalFormat {
    let ranges = rules.ranges.iter().map(|r| plan.map_range(origin, *r)).collect();
    let rules = rules
        .rules
        .iter()
        .map(|rule| {
            *priority += 1;
            CfRule {
                dxf: rule.dxf.and_then(|d| dxf_map.get(&d).copied()).or(rule.dxf),
                priority: *priority,
                formulas: rule.formulas.iter().map(|f| shifted_text(f, dr, dc)).collect(),
                ..rule.clone()
            }
        })
        .collect();
    ConditionalFormat { ranges, rules }
}
