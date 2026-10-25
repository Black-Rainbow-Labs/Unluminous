//! Insert Copied Cells and Insert Cut Cells, and Paste Link.

use super::paste::Plan;
use super::PasteSpecial;
use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::edit::{Clip, Session, Shift};
use crate::formula::{Axis, Expr, Formula, Reference};
use crate::model::Cell;
use crate::value::Value;

impl Session {
    /// Paste Link: each cell pasted holds a reference to the cell it was copied from, in a sheet other than
    /// the source's with the sheet named. The formats under the destination stay as they are.
    pub fn paste_link(&mut self, clip: &Clip, sheet: usize, at: CellRef) -> Result<Range, String> {
        self.check_sheet(sheet)?;
        let plan = Plan::new(clip, Range::cell(at), false)?;
        self.refuse_array_member(sheet, plan.target)?;
        self.refuse_locked(sheet, plan.target)?;
        self.remember_cells("Paste link", sheet, &[plan.target], false);
        let name = (clip.sheet != sheet).then(|| self.book.sheets[clip.sheet].name.as_str().into());
        for r in 0..plan.rows.min(plan.target.rows()) {
            for c in 0..plan.cols.min(plan.target.cols()) {
                let from = clip.source_cell(r, c);
                let reference = Reference::from_range(Range::cell(from), name.clone());
                let dest = CellRef::new(at.row + r, at.col + c);
                let style = self.book.sheets[sheet].cell(dest).map_or(0, |x| x.style);
                let formula = Formula::from_expr(Expr::Ref(reference), true);
                let cell = Cell {
                    value: Value::Empty,
                    formula: Some(formula),
                    style,
                    ..Default::default()
                };
                self.book.sheets[sheet].set(dest, cell);
                self.engine.changed(&self.book, sheet, dest);
            }
        }
        self.recalculate();
        Ok(plan.target)
    }

    /// Insert the copied cells at a place, shifting the cells there down or right to make room; cut cells
    /// are moved there instead, and the references to them follow. Returns the cells that were inserted.
    pub fn insert_copied(
        &mut self,
        clip: &Clip,
        sheet: usize,
        at: CellRef,
        shift: Shift,
    ) -> Result<Range, String> {
        self.check_sheet(sheet)?;
        if !matches!(shift, Shift::Down | Shift::Right) {
            return Err("Insert Copied Cells shifts cells down or right".to_owned());
        }
        let (rows, cols) = clip.block_size();
        let target = Range::new(
            at,
            CellRef::new(
                (at.row + rows - 1).min(MAX_ROWS - 1),
                (at.col + cols - 1).min(MAX_COLS - 1),
            ),
        );
        if clip.cut && clip.sheet == sheet && clip.range.intersects(&target) {
            return Err("This action won't work on overlapping selections.".to_owned());
        }
        let (axis, position, count, band) = line_of(shift, target);
        self.refuse_structural(sheet, axis, position, count, band)?;
        self.refuse_locked(sheet, target)?;
        self.remember_whole(if clip.cut { "Insert cut cells" } else { "Insert copied cells" });
        if clip.cut {
            self.insert_cut(clip, sheet, target, shift);
        } else {
            self.structural(sheet, axis, position, count, band);
            let plan = Plan::new(clip, Range::cell(at), false)?;
            self.run_paste(clip, sheet, &plan, &PasteSpecial::default());
        }
        Ok(target)
    }

    /// Insert Cut Cells: the cut cells move out of the way first, the gap they leave closes when it is in
    /// the same rows (or columns) as where they go, room opens, and they move in.
    fn insert_cut(&mut self, clip: &Clip, sheet: usize, target: Range, shift: Shift) {
        let (rows, cols) = clip.block_size();
        // Park the block in the far corner so closing a gap and opening room cannot touch it. The
        // references to its cells follow it there.
        let park = CellRef::new(MAX_ROWS - rows, MAX_COLS - cols);
        self.move_cells(clip, sheet, park);
        let mut target = target;
        if clip.sheet == sheet {
            target = self.close_the_gap(clip.range, target, shift, sheet);
        }
        let (axis, position, count, band) = line_of(shift, target);
        self.structural(sheet, axis, position, count, band);
        let parked = Range::new(park, CellRef::new(park.row + rows - 1, park.col + cols - 1));
        let moved = Clip { sheet, range: parked, cut: true, ..clip.clone() };
        self.move_cells(&moved, sheet, target.first);
    }

    /// Delete the place the cut cells left when it lies in the same rows (columns) as where they go, and
    /// say where the target is after that.
    fn close_the_gap(&mut self, source: Range, target: Range, shift: Shift, sheet: usize) -> Range {
        let aligned = match shift {
            Shift::Right => {
                source.first.row == target.first.row && source.last.row == target.last.row
            }
            _ => source.first.col == target.first.col && source.last.col == target.last.col,
        };
        if !aligned {
            return target;
        }
        let (axis, position, count, band, before) = match shift {
            Shift::Right => (
                Axis::Columns,
                source.first.col,
                -(source.cols() as i64),
                Some((source.first.row, source.last.row)),
                source.last.col < target.first.col,
            ),
            _ => (
                Axis::Rows,
                source.first.row,
                -(source.rows() as i64),
                Some((source.first.col, source.last.col)),
                source.last.row < target.first.row,
            ),
        };
        self.structural(sheet, axis, position, count, band);
        if !before {
            return target;
        }
        match shift {
            Shift::Right => Range::new(
                CellRef::new(target.first.row, target.first.col - source.cols()),
                CellRef::new(target.last.row, target.last.col - source.cols()),
            ),
            _ => Range::new(
                CellRef::new(target.first.row - source.rows(), target.first.col),
                CellRef::new(target.last.row - source.rows(), target.last.col),
            ),
        }
    }
}

/// The structural edit an insert of a block makes: the axis, where, how many lines, and the band of
/// columns (rows) it covers.
fn line_of(shift: Shift, target: Range) -> (Axis, u32, i64, Option<(u32, u32)>) {
    match shift {
        Shift::Right => (
            Axis::Columns,
            target.first.col,
            target.cols() as i64,
            Some((target.first.row, target.last.row)),
        ),
        _ => (
            Axis::Rows,
            target.first.row,
            target.rows() as i64,
            Some((target.first.col, target.last.col)),
        ),
    }
}
