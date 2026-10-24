//! Protecting sheets and the workbook, and refusing what a protected sheet or a locked structure refuses.
//!
//! [`Guard`] names every kind of change a protected sheet treats differently, and [`Session::allowed`]
//! answers whether a change is allowed with Excel's own message when it is not. The editing methods call
//! it before they change anything, so a refused change leaves the workbook as it was. The rules were
//! measured against Excel 16: `tools/excel-oracle/protect-matrix.ps1` tries 43 operations on a protected
//! sheet with each allow flag on its own, and `tests/protection.rs` holds the engine to the same table.

use crate::coord::{CellRef, Range, MAX_COLS};
use crate::protect::{
    ProtectedRange, Secret, SheetProtection, WorkbookProtection, COMMAND_REFUSED, LOCKED_CELL,
    STRUCTURE_LOCKED, WRONG_PASSWORD,
};

use super::Session;

/// What Excel says when Allow Edit Ranges is given a title that is already used.
const TITLE_IN_USE: &str = "This title is already used by another range. Enter a different title.";

/// A change a protected sheet may refuse, with the cells it touches where that matters.
#[derive(Debug, Clone, Copy)]
pub enum Guard<'a> {
    /// Typing into cells, clearing them, filling, pasting values: every cell must be unlocked.
    Write(&'a [Range]),
    /// Formatting cells (the Format Cells dialog, the Format panel): needs `formatCells`.
    Format,
    /// Changing a column's width or hiding columns: needs `formatColumns`.
    FormatColumns,
    /// Changing a row's height or hiding rows: needs `formatRows`.
    FormatRows,
    InsertRows,
    InsertColumns,
    /// Deleting whole rows: needs `deleteRows` and rows with no locked cell.
    DeleteRows(Range),
    /// Deleting whole columns: needs `deleteColumns` and columns with no locked cell.
    DeleteColumns(Range),
    /// Sorting: needs `sort` and a range with no locked cell.
    Sort(Range),
    /// Putting a link on cells: needs `insertHyperlinks` and unlocked cells.
    Hyperlink(Range),
    /// Notes, charts, pictures and shapes: refused while objects are protected.
    Objects,
    /// Using the buttons of a filter that exists: needs `autoFilter`.
    Filter,
    /// Pivot tables: needs `pivotTables`.
    Pivot,
    /// Showing a scenario: refused while scenarios are protected.
    Scenarios,
    /// Creating a table, which Excel refuses with a message of its own.
    Table,
    /// A command a protected sheet never allows: merging, validation, grouping, text to columns,
    /// removing duplicates, creating a filter, adding a scenario.
    Forbidden,
}

impl Session {
    /// Whether a change is allowed on a sheet, and Excel's message when it is not.
    pub fn allowed(&self, sheet: usize, guard: Guard) -> Result<(), String> {
        let Some(p) = self.book.sheets.get(sheet).and_then(|s| s.protection()) else { return Ok(()) };
        if !p.sheet && !p.objects && !p.scenarios {
            return Ok(());
        }
        match guard {
            Guard::Objects if p.objects => Err(LOCKED_CELL.to_owned()),
            Guard::Scenarios if p.scenarios => Err(COMMAND_REFUSED.to_owned()),
            Guard::Objects | Guard::Scenarios => Ok(()),
            _ if !p.sheet => Ok(()),
            Guard::Write(ranges) => self.refuse_if_locked(sheet, ranges),
            Guard::Format => flag(p.allow.format_cells),
            Guard::FormatColumns => flag(p.allow.format_columns),
            Guard::FormatRows => flag(p.allow.format_rows),
            Guard::InsertRows => flag(p.allow.insert_rows),
            Guard::InsertColumns => flag(p.allow.insert_columns),
            Guard::DeleteRows(r) => {
                flag(p.allow.delete_rows)?;
                self.refuse_if_locked(sheet, &[whole_rows(r)])
            }
            Guard::DeleteColumns(r) => {
                flag(p.allow.delete_columns)?;
                self.refuse_if_locked(sheet, &[whole_columns(r)])
            }
            Guard::Sort(r) => {
                flag(p.allow.sort)?;
                self.refuse_if_locked(sheet, &[r])
            }
            Guard::Hyperlink(r) => {
                flag(p.allow.insert_hyperlinks)?;
                self.refuse_if_locked(sheet, &[r])
            }
            Guard::Filter => flag(p.allow.auto_filter),
            Guard::Pivot => flag(p.allow.pivot_tables),
            Guard::Table => Err("Table features aren't available because the sheet is protected.".to_owned()),
            Guard::Forbidden => Err(COMMAND_REFUSED.to_owned()),
        }
    }

    /// Refuse a change to the sheets of a workbook whose structure is locked.
    pub fn allow_structure_change(&self) -> Result<(), String> {
        if self.book.structure_locked() {
            return Err(STRUCTURE_LOCKED.to_owned());
        }
        Ok(())
    }

    /// Whether a cell can be selected on its sheet. A protected sheet may forbid selecting locked cells,
    /// unlocked cells or both.
    pub fn can_select(&self, sheet: usize, at: CellRef) -> bool {
        let Some(p) = self.book.sheets.get(sheet).and_then(|s| s.protection()).filter(|p| p.sheet)
        else {
            return true;
        };
        if self.cell_is_locked(sheet, at.row, at.col, &[]) {
            p.allow.select_locked_cells
        } else {
            p.allow.select_unlocked_cells
        }
    }

    /// The edit ranges of a sheet a person may edit now: those with no password, and those whose
    /// password was given in this session.
    fn open_edit_ranges(&self, sheet: usize) -> Vec<Range> {
        self.book.sheets[sheet]
            .protected_ranges()
            .into_iter()
            .filter(|p| p.secret.is_none() || self.edit_unlocked.contains(&(sheet, p.name.clone())))
            .flat_map(|p| p.ranges)
            .collect()
    }

    fn refuse_if_locked(&self, sheet: usize, ranges: &[Range]) -> Result<(), String> {
        let open = self.open_edit_ranges(sheet);
        if ranges.iter().any(|r| self.any_locked(sheet, *r, &open)) {
            return Err(LOCKED_CELL.to_owned());
        }
        Ok(())
    }

    /// Whether a cell is locked: its format says so (cells are locked unless a format unlocks them) and no
    /// open edit range covers it.
    fn cell_is_locked(&self, sheet: usize, row: u32, col: u32, open: &[Range]) -> bool {
        if open.iter().any(|r| r.contains(CellRef::new(row, col))) {
            return false;
        }
        let s = &self.book.sheets[sheet];
        let style = s
            .cells
            .get(&(row, col))
            .map(|c| c.style)
            .unwrap_or_else(|| super::inherited_style(s, (row, col)));
        self.book.styles.xf(style).locked.unwrap_or(true)
    }

    /// Whether any cell of a range is locked. A range of ordinary size is walked cell by cell; a whole
    /// row or column is answered from the cells that exist and from the formats of its rows and columns.
    fn any_locked(&self, sheet: usize, range: Range, open: &[Range]) -> bool {
        if open.iter().any(|r| r.contains(range.first) && r.contains(range.last)) {
            return false;
        }
        if range.len() <= 200_000 {
            return range.cells().any(|at| self.cell_is_locked(sheet, at.row, at.col, open));
        }
        let s = &self.book.sheets[sheet];
        let locked = |style: u32| self.book.styles.xf(style).locked.unwrap_or(true);
        let rows_with_style =
            s.rows.range(range.first.row..=range.last.row).filter(|(_, i)| i.style.is_some()).count();
        for (&(row, col), cell) in s.cells.range((range.first.row, 0)..=(range.last.row, MAX_COLS)) {
            let inside = col >= range.first.col && col <= range.last.col;
            if inside && locked(cell.style) && !open.iter().any(|r| r.contains(CellRef::new(row, col))) {
                return true;
            }
        }
        let implicit_rows = (range.rows() as usize) > rows_with_style;
        for col in range.first.col..=range.last.col {
            let style = s.column(col).map_or(0, |c| c.style);
            if implicit_rows && locked(style) {
                return true;
            }
        }
        for (&row, info) in s.rows.range(range.first.row..=range.last.row) {
            let Some(style) = info.style else { continue };
            let explicit = s.cells.range((row, range.first.col)..=(row, range.last.col)).count();
            if locked(style) && (explicit as u32) < range.cols() {
                return true;
            }
        }
        false
    }

    // ------------------------------------------------------------------------------------------------
    // Sheet protection.

    /// Protect a sheet. `options` carries what Excel's Protect Sheet dialog offers: whether contents,
    /// objects and scenarios are protected and which actions are allowed. The password is hashed the way
    /// Excel hashes it and is never kept.
    pub fn protect_sheet(
        &mut self,
        sheet: usize,
        password: Option<&str>,
        options: &SheetProtection,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if self.book.sheets[sheet].is_protected() {
            return Err("The sheet is already protected.".to_owned());
        }
        self.remember_whole("Protect sheet");
        let mut p = options.clone();
        p.secret = password.filter(|p| !p.is_empty()).map(Secret::with_random_salt);
        self.book.sheets[sheet].set_protection(Some(&p));
        self.edit_unlocked.retain(|(s, _)| *s != sheet);
        Ok(())
    }

    /// Unprotect a sheet. A sheet with a password refuses a wrong one with Excel's message.
    pub fn unprotect_sheet(&mut self, sheet: usize, password: Option<&str>) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let Some(p) = self.book.sheets[sheet].protection() else {
            return Err("The sheet is not protected.".to_owned());
        };
        if let Some(secret) = &p.secret {
            if !secret.matches(password.unwrap_or("")) {
                return Err(WRONG_PASSWORD.to_owned());
            }
        }
        self.remember_whole("Unprotect sheet");
        self.book.sheets[sheet].set_protection(None);
        self.edit_unlocked.retain(|(s, _)| *s != sheet);
        Ok(())
    }

    /// The protection a sheet has, if any.
    pub fn sheet_protection(&self, sheet: usize) -> Option<SheetProtection> {
        self.book.sheets.get(sheet).and_then(|s| s.protection())
    }

    // ------------------------------------------------------------------------------------------------
    // Allow Edit Ranges.

    /// Add a range people may edit on a protected sheet, with a password of its own or none.
    pub fn add_edit_range(
        &mut self,
        sheet: usize,
        title: &str,
        ranges: &[Range],
        password: Option<&str>,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.refuse_while_protected(sheet)?;
        let mut list = self.book.sheets[sheet].protected_ranges();
        if title.trim().is_empty() {
            return Err("Enter a title for the range.".to_owned());
        }
        if list.iter().any(|p| p.name.eq_ignore_ascii_case(title)) {
            return Err(TITLE_IN_USE.to_owned());
        }
        if ranges.is_empty() {
            return Err("Enter the cells the range refers to.".to_owned());
        }
        self.remember_whole("Allow edit range");
        list.push(ProtectedRange {
            name: title.to_owned(),
            ranges: ranges.to_vec(),
            secret: password.filter(|p| !p.is_empty()).map(Secret::with_random_salt),
            security_descriptor: None,
            other: Vec::new(),
        });
        self.book.sheets[sheet].set_protected_ranges(&list);
        Ok(())
    }

    /// Remove an edit range by title. Removing one that has a password asks for it, as Excel does.
    pub fn remove_edit_range(
        &mut self,
        sheet: usize,
        title: &str,
        password: Option<&str>,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.refuse_while_protected(sheet)?;
        let mut list = self.book.sheets[sheet].protected_ranges();
        let Some(index) = list.iter().position(|p| p.name.eq_ignore_ascii_case(title)) else {
            return Err(format!("There is no range titled {title}."));
        };
        if let Some(secret) = &list[index].secret {
            if !secret.matches(password.unwrap_or("")) {
                return Err(WRONG_PASSWORD.to_owned());
            }
        }
        self.remember_whole("Remove edit range");
        list.remove(index);
        self.book.sheets[sheet].set_protected_ranges(&list);
        Ok(())
    }

    /// Give the password of an edit range so its cells can be edited until the sheet is protected again.
    pub fn unlock_edit_range(
        &mut self,
        sheet: usize,
        title: &str,
        password: &str,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let list = self.book.sheets[sheet].protected_ranges();
        let Some(range) = list.iter().find(|p| p.name.eq_ignore_ascii_case(title)) else {
            return Err(format!("There is no range titled {title}."));
        };
        if let Some(secret) = &range.secret {
            if !secret.matches(password) {
                return Err(WRONG_PASSWORD.to_owned());
            }
        }
        let key = (sheet, range.name.clone());
        if !self.edit_unlocked.contains(&key) {
            self.edit_unlocked.push(key);
        }
        Ok(())
    }

    /// The edit ranges of a sheet.
    pub fn edit_ranges(&self, sheet: usize) -> Vec<ProtectedRange> {
        self.book.sheets.get(sheet).map(|s| s.protected_ranges()).unwrap_or_default()
    }

    /// Excel changes the list of edit ranges only while the sheet is unprotected.
    fn refuse_while_protected(&self, sheet: usize) -> Result<(), String> {
        if self.book.sheets[sheet].is_protected() {
            return Err(COMMAND_REFUSED.to_owned());
        }
        Ok(())
    }

    // ------------------------------------------------------------------------------------------------
    // Workbook structure.

    /// Protect the structure of the workbook (and its windows, when asked) with an optional password.
    pub fn protect_workbook(
        &mut self,
        password: Option<&str>,
        lock_structure: bool,
        lock_windows: bool,
    ) -> Result<(), String> {
        if self.book.protection().is_some_and(|p| p.lock_structure || p.lock_windows) {
            return Err("The workbook is already protected.".to_owned());
        }
        if !lock_structure && !lock_windows {
            return Err("Choose Structure, Windows or both.".to_owned());
        }
        self.remember_whole("Protect workbook");
        let p = WorkbookProtection {
            secret: password.filter(|p| !p.is_empty()).map(Secret::with_random_salt),
            lock_structure,
            lock_windows,
            ..Default::default()
        };
        self.book.set_protection(Some(&p));
        Ok(())
    }

    /// Unprotect the workbook. A password refuses a wrong one with Excel's message.
    pub fn unprotect_workbook(&mut self, password: Option<&str>) -> Result<(), String> {
        let Some(p) = self.book.protection() else {
            return Err("The workbook is not protected.".to_owned());
        };
        if let Some(secret) = &p.secret {
            if !secret.matches(password.unwrap_or("")) {
                return Err(WRONG_PASSWORD.to_owned());
            }
        }
        self.remember_whole("Unprotect workbook");
        self.book.set_protection(None);
        Ok(())
    }

    pub fn workbook_protection(&self) -> Option<WorkbookProtection> {
        self.book.protection()
    }
}

/// Whether an allow flag is on, with the message that says it is not.
fn flag(allowed: bool) -> Result<(), String> {
    if allowed {
        Ok(())
    } else {
        Err(COMMAND_REFUSED.to_owned())
    }
}

/// A range stretched to every column of its rows.
fn whole_rows(r: Range) -> Range {
    Range::new(CellRef::new(r.first.row, 0), CellRef::new(r.last.row, MAX_COLS - 1))
}

/// A range stretched to every row of its columns.
fn whole_columns(r: Range) -> Range {
    Range::new(
        CellRef::new(0, r.first.col),
        CellRef::new(crate::coord::MAX_ROWS - 1, r.last.col),
    )
}
