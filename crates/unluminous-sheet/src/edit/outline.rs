//! Grouping rows and columns into an outline, folding the groups, and the settings of the outline.
//!
//! Each method is one undo step, refuses what Excel refuses with the sheet unchanged, and leaves hidden
//! rows hidden the way Excel does. [`crate::outline`] has the rules; this wraps them for a session.

use super::Session;
use crate::coord::{Range, MAX_COLS, MAX_ROWS};
use crate::model::Sheet;
use crate::outline::{self, Dim};

impl Session {
    /// Run a change to one sheet's outline: tried on a copy first, so a refusal leaves the workbook as it
    /// was, then kept as one undo step. SUBTOTAL over visible cells reads which rows are hidden, so the
    /// sheet is calculated again.
    fn change_outline<R>(
        &mut self,
        label: &str,
        sheet: usize,
        change: impl FnOnce(&mut Sheet) -> Result<R, String>,
    ) -> Result<R, String> {
        self.check_sheet(sheet)?;
        let mut trial = self.book.sheets[sheet].clone();
        let answer = change(&mut trial)?;
        self.remember_whole(label);
        self.book.sheets[sheet] = trial;
        self.recalculate_all();
        Ok(answer)
    }

    /// Group rows (`Data`, `Group`, or Shift+Alt+Right): each row one level deeper. Excel refuses a
    /// group deeper than eight levels.
    pub fn group_rows(&mut self, sheet: usize, first: u32, last: u32) -> Result<(), String> {
        self.change_outline("Group rows", sheet, |s| outline::group(s, Dim::Rows, first, last))
    }

    /// Group columns, each one level deeper.
    pub fn group_columns(&mut self, sheet: usize, first: u32, last: u32) -> Result<(), String> {
        self.change_outline("Group columns", sheet, |s| {
            outline::group(s, Dim::Columns, first, last)
        })
    }

    /// Ungroup rows (Shift+Alt+Left): each grouped row one level out. Refused when none is grouped.
    pub fn ungroup_rows(&mut self, sheet: usize, first: u32, last: u32) -> Result<(), String> {
        self.change_outline("Ungroup rows", sheet, |s| outline::ungroup(s, Dim::Rows, first, last))
    }

    /// Ungroup columns.
    pub fn ungroup_columns(&mut self, sheet: usize, first: u32, last: u32) -> Result<(), String> {
        self.change_outline("Ungroup columns", sheet, |s| {
            outline::ungroup(s, Dim::Columns, first, last)
        })
    }

    /// Collapse (`expanded` false) or expand the group whose summary row or column is `summary`: the
    /// plus and minus buttons beside the outline. Refused when the line is not a group's summary.
    pub fn show_detail(
        &mut self,
        sheet: usize,
        dim: Dim,
        summary: u32,
        expanded: bool,
    ) -> Result<(), String> {
        let label = if expanded { "Expand group" } else { "Collapse group" };
        self.change_outline(label, sheet, |s| outline::set_expanded(s, dim, summary, expanded))
    }

    /// The level buttons: show `row_levels` levels of the rows and `column_levels` of the columns and fold
    /// the rest. A count of 0 leaves that direction as it is.
    pub fn show_levels(
        &mut self,
        sheet: usize,
        row_levels: u8,
        column_levels: u8,
    ) -> Result<(), String> {
        self.change_outline("Show outline level", sheet, |s| {
            outline::show_levels(s, Dim::Rows, row_levels);
            outline::show_levels(s, Dim::Columns, column_levels);
            Ok(())
        })
    }

    /// Auto Outline: replace the outline with groups built from the formulas that sum rows or columns
    /// next to them, and move the summary setting to the side those formulas are on. With a range only the
    /// formulas inside it count and only its rows and columns are replaced.
    pub fn auto_outline(&mut self, sheet: usize, within: Option<Range>) -> Result<(), String> {
        self.change_outline("Auto outline", sheet, |s| outline::auto_outline(s, within))
    }

    /// Clear Outline for the whole sheet: every group removed and every hidden group shown.
    pub fn clear_outline(&mut self, sheet: usize) -> Result<(), String> {
        self.change_outline("Clear outline", sheet, |s| {
            outline::clear(s, Dim::Rows, 0, MAX_ROWS - 1);
            outline::clear(s, Dim::Columns, 0, MAX_COLS - 1);
            Ok(())
        })
    }

    /// Clear the outline of some rows or columns only.
    pub fn clear_outline_lines(
        &mut self,
        sheet: usize,
        dim: Dim,
        first: u32,
        last: u32,
    ) -> Result<(), String> {
        self.change_outline("Clear outline", sheet, |s| {
            outline::clear(s, dim, first, last);
            Ok(())
        })
    }

    /// Where the summary rows (below or above their detail) and the summary columns (right or left of
    /// it) sit. `None` leaves a setting as it is.
    pub fn set_outline_summary(
        &mut self,
        sheet: usize,
        rows_below: Option<bool>,
        columns_right: Option<bool>,
    ) -> Result<(), String> {
        self.change_outline("Outline settings", sheet, |s| {
            outline::set_summary(s, rows_below, columns_right);
            Ok(())
        })
    }

    /// Show or hide the outline's buttons and level numbers (Ctrl+8).
    pub fn set_outline_symbols(&mut self, sheet: usize, shown: bool) -> Result<(), String> {
        self.change_outline("Outline symbols", sheet, |s| {
            s.outline.show_symbols = shown;
            Ok(())
        })
    }

    /// Automatic styles: bold on summary rows and italic on summary columns.
    pub fn set_outline_styles(&mut self, sheet: usize, applied: bool) -> Result<(), String> {
        self.change_outline("Outline settings", sheet, |s| {
            s.outline.apply_styles = applied;
            Ok(())
        })
    }

    /// The deepest level the rows and the columns have, which is how many level buttons to draw less one.
    pub fn outline_depth(&self, sheet: usize) -> (u8, u8) {
        let s = &self.book.sheets[sheet];
        (outline::deepest(s, Dim::Rows), outline::deepest(s, Dim::Columns))
    }
}
