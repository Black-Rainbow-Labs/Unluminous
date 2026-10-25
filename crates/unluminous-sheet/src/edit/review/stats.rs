//! The Watch Window and Workbook Statistics.

use crate::coord::{CellRef, Range};
use crate::edit::Session;
use crate::model::CellWatch;
use crate::value::Value;

/// One row of the Watch Window: Excel's Book, Sheet, Name, Cell, Value and Formula columns.
#[derive(Debug, Clone, PartialEq)]
pub struct WatchRow {
    pub sheet: usize,
    pub sheet_name: String,
    /// The defined name that refers to just this cell, or empty.
    pub name: String,
    pub at: CellRef,
    pub value: Value,
    /// The formula with its `=`, or empty for a cell with none.
    pub formula: String,
}

/// What Workbook Statistics counts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkbookStatistics {
    pub sheets: usize,
    /// Cells holding a value or a formula, spilled cells included.
    pub cells_with_data: usize,
    pub formulas: usize,
    pub tables: usize,
    pub charts: usize,
    pub pivot_tables: usize,
    pub external_links: usize,
    /// Defined names a person sees: not the hidden ones Excel keeps for itself.
    pub names: usize,
    pub notes: usize,
    pub comments: usize,
}

impl Session {
    /// Put a cell in the Watch Window. A cell watched already is not added twice.
    pub fn add_watch(&mut self, sheet: usize, at: CellRef) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if self.book.sheets[sheet].watches.iter().any(|w| w.at == at) {
            return Ok(());
        }
        self.remember_whole("Add watch");
        self.book.sheets[sheet].watches.push(CellWatch { at });
        Ok(())
    }

    /// Take a cell out of the Watch Window.
    pub fn delete_watch(&mut self, sheet: usize, at: CellRef) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if !self.book.sheets[sheet].watches.iter().any(|w| w.at == at) {
            return Err("That cell is not being watched.".to_owned());
        }
        self.remember_whole("Delete watch");
        self.book.sheets[sheet].watches.retain(|w| w.at != at);
        Ok(())
    }

    /// The Watch Window's rows, sheet by sheet in the order the cells were added.
    pub fn watch_rows(&self) -> Vec<WatchRow> {
        let mut rows = Vec::new();
        for (si, s) in self.book.sheets.iter().enumerate() {
            for watch in &s.watches {
                let cell = s.cell(watch.at);
                rows.push(WatchRow {
                    sheet: si,
                    sheet_name: s.name.clone(),
                    name: self.name_of_cell(si, watch.at),
                    at: watch.at,
                    value: s.value_at(watch.at),
                    formula: cell
                        .and_then(|c| c.formula.as_ref())
                        .map(|f| f.display())
                        .unwrap_or_default(),
                });
            }
        }
        rows
    }

    /// The defined name that refers to exactly one cell, if there is one.
    fn name_of_cell(&self, sheet: usize, at: CellRef) -> String {
        let sheet_name = &self.book.sheets[sheet].name;
        for n in self.book.names.iter().filter(|n| n.scope.is_none_or(|s| s == sheet)) {
            let formula = n.formula.trim_start_matches('=');
            let Some((on, cells)) = formula.rsplit_once('!') else { continue };
            let on = on.trim_matches('\'').replace("''", "'");
            if on.eq_ignore_ascii_case(sheet_name)
                && Range::parse(&cells.replace('$', "")) == Some(Range::cell(at))
            {
                return n.name.clone();
            }
        }
        String::new()
    }

    /// Workbook Statistics.
    pub fn workbook_statistics(&self) -> WorkbookStatistics {
        let book = &self.book;
        let mut stats = WorkbookStatistics {
            sheets: book.sheets.len(),
            tables: book.tables.len(),
            ..Default::default()
        };
        for s in &book.sheets {
            stats.cells_with_data +=
                s.cells.values().filter(|c| c.formula.is_some() || !c.value.is_empty()).count()
                    + s.spills.values.values().filter(|v| !v.is_empty()).count();
            stats.formulas += s.cells.values().filter(|c| c.formula.is_some()).count();
            stats.charts += s.charts.len();
            stats.notes += s.notes.len();
            stats.comments += s.threads.len();
        }
        let kept_parts = |prefix: &str| {
            book.kept
                .parts
                .iter()
                .filter(|(p, _)| p.starts_with(prefix) && p.ends_with(".xml"))
                .count()
        };
        stats.pivot_tables = kept_parts("xl/pivotTables/pivotTable");
        stats.external_links = kept_parts("xl/externalLinks/externalLink");
        stats.charts += book
            .sheets
            .iter()
            .filter(|s| s.kept_part.as_ref().is_some_and(|k| k.kind.ends_with("/chartsheet")))
            .count();
        stats.names =
            book.names.iter().filter(|n| !n.hidden && !n.name.starts_with("_xlnm.")).count();
        stats
    }
}
