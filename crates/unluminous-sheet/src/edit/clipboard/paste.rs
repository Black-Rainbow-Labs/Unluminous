//! Pasting a clip: where it lands, what each cell receives, and the formulas it carries.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::operate::{self, Content};
use super::snapshot::{import_style, intern_xf};
use super::{PasteKind, PasteOperation, PasteSpecial};
use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::edit::{Clip, Session};
use crate::formula::{self, Expr, Formula, Point, Reference, Shape};
use crate::model::Cell;
use crate::value::Value;

/// Excel's refusal of a paste whose destination is a different size from what was copied.
const SIZE_MISMATCH: &str = "You can't paste this here because the Copy area and paste area aren't the same size.\n\nSelect just one cell in the paste area or an area that's the same size, and try pasting again.";

/// Where a paste lands and how many times the copied block repeats there.
pub(super) struct Plan {
    /// Rows and columns of one block as pasted, after any transposing.
    pub rows: u32,
    pub cols: u32,
    pub repeat_rows: u32,
    pub repeat_cols: u32,
    pub target: Range,
    pub transpose: bool,
}

impl Plan {
    /// Work out the destination: a single cell takes one copy of the block, and a selection that is a whole
    /// multiple of the block takes as many copies as fit.
    pub(super) fn new(clip: &Clip, to: Range, transpose: bool) -> Result<Plan, String> {
        let (h, w) = clip.block_size();
        let (rows, cols) = if transpose { (w, h) } else { (h, w) };
        let repeat_rows = if to.rows() > rows && to.rows() % rows == 0 { to.rows() / rows } else { 1 };
        let repeat_cols = if to.cols() > cols && to.cols() % cols == 0 { to.cols() / cols } else { 1 };
        let last = CellRef::new(
            (to.first.row + rows * repeat_rows - 1).min(MAX_ROWS - 1),
            (to.first.col + cols * repeat_cols - 1).min(MAX_COLS - 1),
        );
        Ok(Plan { rows, cols, repeat_rows, repeat_cols, target: Range::new(to.first, last), transpose })
    }

    /// The top left cell of every copy of the block.
    pub(super) fn origins(&self) -> Vec<CellRef> {
        let mut out = Vec::new();
        for rr in 0..self.repeat_rows {
            for rc in 0..self.repeat_cols {
                let (row, col) = (self.target.first.row + rr * self.rows, self.target.first.col + rc * self.cols);
                if row < MAX_ROWS && col < MAX_COLS {
                    out.push(CellRef::new(row, col));
                }
            }
        }
        out
    }

    /// The cells one copy of the block covers.
    pub(super) fn region(&self, origin: CellRef) -> Range {
        Range::new(
            origin,
            CellRef::new(
                (origin.row + self.rows - 1).min(MAX_ROWS - 1),
                (origin.col + self.cols - 1).min(MAX_COLS - 1),
            ),
        )
    }

    /// A range counted from the source block's top left, as it is pasted at `origin`.
    pub(super) fn map_range(&self, origin: CellRef, rel: Range) -> Range {
        let at = |r: u32, c: u32| {
            let (r, c) = if self.transpose { (c, r) } else { (r, c) };
            CellRef::new((origin.row + r).min(MAX_ROWS - 1), (origin.col + c).min(MAX_COLS - 1))
        };
        Range::new(at(rel.first.row, rel.first.col), at(rel.last.row, rel.last.col))
    }
}

/// What a paste is doing, kept while it runs.
pub(super) struct Cx<'a> {
    pub clip: &'a Clip,
    pub sheet: usize,
    pub plan: &'a Plan,
    pub opts: &'a PasteSpecial,
    /// The format in this workbook of each cell format the clip carries.
    styles: BTreeMap<u32, u32>,
    /// Legacy array formulas pasted whole.
    arrays: Vec<Range>,
}

impl<'a> Cx<'a> {
    pub(super) fn new(clip: &'a Clip, sheet: usize, plan: &'a Plan, opts: &'a PasteSpecial) -> Self {
        Self { clip, sheet, plan, opts, styles: BTreeMap::new(), arrays: Vec::new() }
    }
}

impl Session {
    /// Paste copied cells with every choice of Paste Special. A paste into a selection that is a whole
    /// multiple of the copied block repeats it, as Excel does. Returns the cells written.
    pub fn paste_special(
        &mut self,
        clip: &Clip,
        sheet: usize,
        to: Range,
        opts: &PasteSpecial,
    ) -> Result<Range, String> {
        self.check_sheet(sheet)?;
        if clip.cut && *opts == PasteSpecial::default() {
            return self.paste_cut(clip, sheet, to);
        }
        let plan = Plan::new(clip, to, opts.transpose)?;
        // Cells left out by a filter, pasted over the filtered range they came from, do not line up.
        let compacted = !clip.extras.rows.is_empty();
        if compacted && clip.sheet == sheet && to.len() > 1 && to.intersects(&clip.range)
            && (to.rows() != plan.rows || to.cols() != plan.cols)
        {
            return Err(SIZE_MISMATCH.to_owned());
        }
        self.refuse_array_member(sheet, plan.target)?;
        self.refuse_locked(sheet, plan.target)?;
        self.remember_for_paste(sheet, &plan, opts);
        self.run_paste(clip, sheet, &plan, opts);
        Ok(plan.target)
    }

    /// Carry out a paste whose undo step is already recorded.
    pub(super) fn run_paste(&mut self, clip: &Clip, sheet: usize, plan: &Plan, opts: &PasteSpecial) {
        let mut cx = Cx::new(clip, sheet, plan, opts);
        if !matches!(
            opts.kind,
            PasteKind::Comments | PasteKind::Validation | PasteKind::ColumnWidths
        ) {
            self.paste_cells(&mut cx);
        }
        self.paste_extras(&mut cx);
        self.recalculate();
    }

    /// Cut and paste: the cells move and every formula that named them follows.
    fn paste_cut(&mut self, clip: &Clip, sheet: usize, to: Range) -> Result<Range, String> {
        let (rows, cols) = clip.block_size();
        let target = Range::new(
            to.first,
            CellRef::new(
                (to.first.row + rows - 1).min(MAX_ROWS - 1),
                (to.first.col + cols - 1).min(MAX_COLS - 1),
            ),
        );
        self.refuse_array_member(sheet, target)?;
        self.refuse_locked(sheet, target)?;
        self.remember_whole("Move cells");
        self.move_cells(clip, sheet, target.first);
        Ok(target)
    }

    /// Record what undo needs: the cells, or the whole workbook when the paste reaches past the cells.
    fn remember_for_paste(&mut self, sheet: usize, plan: &Plan, opts: &PasteSpecial) {
        let cells_only = matches!(
            opts.kind,
            PasteKind::Formulas
                | PasteKind::Values
                | PasteKind::FormulasAndNumberFormats
                | PasteKind::ValuesAndNumberFormats
        );
        if cells_only {
            self.remember_cells("Paste", sheet, &[plan.target], true);
        } else {
            self.remember_whole("Paste");
        }
    }

    /// Write the cells of the block into every copy of it.
    fn paste_cells(&mut self, cx: &mut Cx) {
        let clip = cx.clip;
        let mut source: BTreeMap<(u32, u32), &Cell> = BTreeMap::new();
        for (r, c, cell) in &clip.cells {
            let key = if cx.plan.transpose { (*c, *r) } else { (*r, *c) };
            source.insert(key, cell);
        }
        if cx.opts.kind.keeps_formulas() {
            self.forget_arrays_under(cx);
        }
        for origin in cx.plan.origins() {
            let region = cx.plan.region(origin);
            let mut keys = self.keys_in(cx.sheet, region);
            keys.extend(source.keys().filter_map(|(r, c)| {
                let (row, col) = (origin.row + r, origin.col + c);
                region.contains(CellRef::new(row, col)).then_some((row, col))
            }));
            for (row, col) in keys {
                let rel = (row - origin.row, col - origin.col);
                self.paste_cell(cx, origin, CellRef::new(row, col), rel, source.get(&rel).copied());
            }
        }
        self.record_arrays(cx);
    }

    /// The places in a range that hold a cell.
    fn keys_in(&self, sheet: usize, region: Range) -> BTreeSet<(u32, u32)> {
        let s = &self.book.sheets[sheet];
        s.cells
            .range((region.first.row, 0)..=(region.last.row, u32::MAX))
            .filter(|((_, c), _)| *c >= region.first.col && *c <= region.last.col)
            .map(|(k, _)| *k)
            .collect()
    }

    /// Array formulas under the destination go, because what is pasted replaces them whole.
    fn forget_arrays_under(&mut self, cx: &Cx) {
        let target = cx.plan.target;
        self.book.sheets[cx.sheet].array_formulas.retain(|a| !a.intersects(&target));
    }

    fn record_arrays(&mut self, cx: &mut Cx) {
        let s = &mut self.book.sheets[cx.sheet];
        s.array_formulas.append(&mut cx.arrays);
    }

    /// Write one destination cell from the source cell that lands on it (`None` for a blank one).
    fn paste_cell(
        &mut self,
        cx: &mut Cx,
        origin: CellRef,
        at: CellRef,
        rel: (u32, u32),
        src: Option<&Cell>,
    ) {
        let blank = src.is_none_or(|c| c.value.is_empty() && c.formula.is_none());
        if cx.opts.skip_blanks && blank {
            return;
        }
        let existing =
            self.book.sheets[cx.sheet].cells.get(&(at.row, at.col)).cloned().unwrap_or_default();
        let from = source_position(cx, rel);
        let (content, written) = if cx.opts.kind.writes_content() {
            (self.pasted_content(cx, &existing, src, from, at, origin), true)
        } else {
            (Content::keep(&existing), false)
        };
        let style = self.pasted_style(cx, &existing, src);
        let extra = if written { None } else { existing.extra.clone() };
        let cell = Cell { value: content.value, formula: content.formula, style, rich: content.rich, extra };
        self.book.sheets[cx.sheet].set(at, cell);
        self.engine.changed(&self.book, cx.sheet, at);
    }

    /// The value, formula and rich text a destination cell ends up with.
    fn pasted_content(
        &mut self,
        cx: &mut Cx,
        existing: &Cell,
        src: Option<&Cell>,
        from: CellRef,
        at: CellRef,
        origin: CellRef,
    ) -> Content {
        let kind = cx.opts.kind;
        let formula = if kind.keeps_formulas() {
            src.and_then(|c| c.formula.as_ref()).map(|f| moved_formula(cx, f, from, at, origin))
        } else {
            None
        };
        if cx.opts.operation != PasteOperation::None {
            let operand = operate::operand(src, formula.as_ref());
            return match operand.and_then(|o| operate::combine(cx.opts.operation, existing, &o)) {
                Some(content) => content,
                None => Content::keep(existing),
            };
        }
        if let Some(f) = formula.as_ref().filter(|f| f.array.is_some()) {
            cx.arrays.extend(f.array);
        }
        let rich = if kind.keeps_formulas() { src.and_then(|c| c.rich.clone()) } else { None };
        Content { value: src.map_or(Value::Empty, |c| c.value.clone()), formula, rich }
    }

    /// The cell format a destination cell ends up with.
    fn pasted_style(&mut self, cx: &mut Cx, existing: &Cell, src: Option<&Cell>) -> u32 {
        let kind = cx.opts.kind;
        let from_clip = |this: &mut Session, cx: &mut Cx| match src {
            Some(c) => this.clip_style(cx, c.style),
            None => 0,
        };
        match kind {
            PasteKind::All
            | PasteKind::AllUsingSourceTheme
            | PasteKind::AllMergingConditionalFormats
            | PasteKind::Formats => from_clip(self, cx),
            PasteKind::AllExceptBorders => {
                let whole = from_clip(self, cx);
                self.compose_style(whole, Some(existing.style), None)
            }
            PasteKind::FormulasAndNumberFormats | PasteKind::ValuesAndNumberFormats => {
                let number = from_clip(self, cx);
                self.compose_style(existing.style, None, Some(number))
            }
            _ => existing.style,
        }
    }

    /// The format in this workbook of a format a clip carries.
    pub(super) fn clip_style(&mut self, cx: &mut Cx, index: u32) -> u32 {
        if let Some(done) = cx.styles.get(&index) {
            return *done;
        }
        let source_theme = cx.opts.kind == PasteKind::AllUsingSourceTheme;
        let made = import_style(&mut self.book.styles, &cx.clip.extras, index, source_theme);
        cx.styles.insert(index, made);
        made
    }

    /// A cell format made of one, with the border of another or the number format of another.
    fn compose_style(&mut self, base: u32, border_from: Option<u32>, number_from: Option<u32>) -> u32 {
        let styles = &mut self.book.styles;
        let mut xf = styles.xf(base);
        if let Some(b) = border_from {
            xf.border = styles.xf(b).border;
        }
        if let Some(n) = number_from {
            xf.num_fmt = styles.xf(n).num_fmt;
        }
        intern_xf(styles, xf)
    }
}

/// Where a cell counted in the pasted block was copied from on the source sheet.
fn source_position(cx: &Cx, rel: (u32, u32)) -> CellRef {
    let (r, c) = if cx.plan.transpose { (rel.1, rel.0) } else { rel };
    cx.clip.source_cell(r, c)
}

/// A formula moved to where it is pasted: relative references move with it, and a transposed paste turns
/// the offsets round as well.
fn moved_formula(cx: &Cx, f: &Formula, from: CellRef, at: CellRef, origin: CellRef) -> Formula {
    let array_whole = f
        .array
        .filter(|a| !cx.plan.transpose && cx.clip.range.contains(a.first) && cx.clip.range.contains(a.last));
    let expr = if cx.plan.transpose {
        transposed_expr(&f.expr, from, at, cx.clip.range, origin)
    } else {
        formula::shifted(&f.expr, at.row as i64 - from.row as i64, at.col as i64 - from.col as i64)
    };
    let mut out = Formula::from_expr(expr, f.dynamic && array_whole.is_none());
    if let Some(a) = array_whole {
        let (dr, dc) = (at.row as i64 - from.row as i64, at.col as i64 - from.col as i64);
        let shift = |p: CellRef| CellRef::new((p.row as i64 + dr) as u32, (p.col as i64 + dc) as u32);
        out.array = Some(Range::new(shift(a.first), shift(a.last)));
        out.dynamic = false;
    }
    out
}

/// A formula copied to a transposed place. A reference to cells inside the copied block follows the block
/// to where it lands, and any other relative reference turns its row and column offsets round.
fn transposed_expr(expr: &Expr, from: CellRef, to: CellRef, block: Range, origin: CellRef) -> Expr {
    expr.map_refs(&mut |r| transposed_reference(r, from, to, block, origin))
}

fn transposed_reference(
    r: &Reference,
    from: CellRef,
    to: CellRef,
    block: Range,
    origin: CellRef,
) -> Option<Reference> {
    if matches!(r.shape, Shape::Columns | Shape::Rows) {
        return Some(r.clone());
    }
    let inside = r.sheet.is_none()
        && block.contains(r.first.at())
        && block.contains(r.last.at());
    let map = |p: &Point| -> Option<Point> {
        let (row, col, row_abs, col_abs) = if inside {
            (
                origin.row as i64 + p.col as i64 - block.first.col as i64,
                origin.col as i64 + p.row as i64 - block.first.row as i64,
                p.col_abs,
                p.row_abs,
            )
        } else if p.row_abs && p.col_abs {
            (p.row as i64, p.col as i64, true, true)
        } else if !p.row_abs && !p.col_abs {
            (
                to.row as i64 + p.col as i64 - from.col as i64,
                to.col as i64 + p.row as i64 - from.row as i64,
                false,
                false,
            )
        } else {
            let dr = to.row as i64 - from.row as i64;
            let dc = to.col as i64 - from.col as i64;
            (
                if p.row_abs { p.row as i64 } else { p.row as i64 + dr },
                if p.col_abs { p.col as i64 } else { p.col as i64 + dc },
                p.row_abs,
                p.col_abs,
            )
        };
        if row < 0 || col < 0 || row >= MAX_ROWS as i64 || col >= MAX_COLS as i64 {
            return None;
        }
        Some(Point { row: row as u32, col: col as u32, row_abs, col_abs })
    };
    let (mut a, mut b) = (map(&r.first)?, map(&r.last)?);
    if a.row > b.row {
        std::mem::swap(&mut a.row, &mut b.row);
        std::mem::swap(&mut a.row_abs, &mut b.row_abs);
    }
    if a.col > b.col {
        std::mem::swap(&mut a.col, &mut b.col);
        std::mem::swap(&mut a.col_abs, &mut b.col_abs);
    }
    let mut out = r.clone();
    out.first = a;
    out.last = b;
    out.sheet = r.sheet.as_ref().map(Arc::clone);
    Some(out)
}
