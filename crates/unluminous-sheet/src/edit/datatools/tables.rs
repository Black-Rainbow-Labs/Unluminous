//! Table design operations: create, resize, convert to a range, header and totals rows, the function of
//! each totals cell, style options, names and calculated columns.
//!
//! A table keeps its definition as the XML of its part, which is edited in place with
//! [`xmltree`](super::xmltree) so everything Excel put there stays. The model fields (`range`, `columns`,
//! `header_row`, `totals_row`) are read back from that XML after every change, so the two never differ.

use super::displayed;
use super::xmltree::{Document, Element, Node};
use crate::coord::{CellRef, Range};
use crate::edit::{check_defined_name, Session};
use std::sync::Arc;

use crate::formula::{Axis, Expr, Formula, Reference};
use crate::model::*;
use crate::value::Value;

/// What the cell of a column in the totals row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotalFunction {
    None,
    Sum,
    Average,
    Count,
    CountNumbers,
    Max,
    Min,
    StdDev,
    Var,
    /// A formula of the person's own.
    Custom,
}

impl TotalFunction {
    /// The name the table part gives it (`totalsRowFunction`).
    pub fn xml_name(self) -> &'static str {
        match self {
            TotalFunction::None => "none",
            TotalFunction::Sum => "sum",
            TotalFunction::Average => "average",
            TotalFunction::Count => "count",
            TotalFunction::CountNumbers => "countNums",
            TotalFunction::Max => "max",
            TotalFunction::Min => "min",
            TotalFunction::StdDev => "stdDev",
            TotalFunction::Var => "var",
            TotalFunction::Custom => "custom",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "none" => TotalFunction::None,
            "sum" => TotalFunction::Sum,
            "average" => TotalFunction::Average,
            "count" => TotalFunction::Count,
            "countNums" => TotalFunction::CountNumbers,
            "max" => TotalFunction::Max,
            "min" => TotalFunction::Min,
            "stdDev" => TotalFunction::StdDev,
            "var" => TotalFunction::Var,
            "custom" => TotalFunction::Custom,
            _ => return None,
        })
    }

    /// The first argument of `SUBTOTAL` that does the same, which ignores rows a filter hides.
    fn subtotal_code(self) -> Option<u32> {
        Some(match self {
            TotalFunction::Average => 101,
            TotalFunction::CountNumbers => 102,
            TotalFunction::Count => 103,
            TotalFunction::Max => 104,
            TotalFunction::Min => 105,
            TotalFunction::StdDev => 107,
            TotalFunction::Sum => 109,
            TotalFunction::Var => 110,
            TotalFunction::None | TotalFunction::Custom => return None,
        })
    }
}

/// How a table looks, as the Table Design tab sets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableStyle {
    /// The name of the style such as `TableStyleMedium2`, or `None` for no style.
    pub name: Option<String>,
    pub row_stripes: bool,
    pub column_stripes: bool,
    pub first_column: bool,
    pub last_column: bool,
    /// The buttons of the filter in the header row.
    pub filter_buttons: bool,
}

/// What a new table is made with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableOptions {
    /// The first row of the range holds the column names. Without it a header row is made above the range.
    pub headers: bool,
    /// The style by name; `Some("")` for none, and `None` for the default.
    pub style: Option<String>,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self { headers: true, style: None }
    }
}

const DEFAULT_STYLE: &str = "TableStyleMedium2";

const RANGE_MESSAGE: &str = "This range won't work because it doesn't align with the existing table. The table requires headers to remain in the same row and include at least one row of data, and the new range should overlap the existing table.";
const OVERLAP_MESSAGE: &str = "Microsoft Excel can't complete this action because it affects cells inside and outside of a table or XML mapping, or over multiple tables. Please select cells inside a single table or XML mapping and try again.";

/// Put the levels of a sort into the table's XML, where Excel keeps them (`sortState` after `autoFilter`).
pub(crate) fn set_sort_state(table: &mut Table, state: &SortState) {
    let Some(mut doc) = Document::parse(&table.xml) else { return };
    let Some(fragment) = Document::parse(&crate::xlsx::write::sort_state_xml(state)) else {
        return;
    };
    doc.root.put_child(fragment.root, &["autoFilter"]);
    table.xml = doc.to_xml();
}

/// A formula's text with the `[@Column]` of the formula bar written the way the file writes it,
/// `Table[[#This Row],[Column]]`.
fn this_row_in_file_form(text: &str, table: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("[@") {
        let before = &rest[..at];
        let unqualified = !before.ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '.');
        let after = &rest[at + 2..];
        let (column, used) = match after.strip_prefix('[') {
            Some(inner) => match inner.find("]]") {
                Some(end) => (inner[..end].to_owned(), end + 4),
                None => break,
            },
            None => match after.find(']') {
                Some(end) => (after[..end].to_owned(), end + 1),
                None => break,
            },
        };
        out.push_str(before);
        if unqualified {
            out.push_str(&format!("{table}[[#This Row],[{column}]]"));
        } else {
            out.push_str(&format!("[[#This Row],[{column}]]"));
        }
        rest = &rest[at + 2 + used..];
        if rest.is_empty() {
            break;
        }
    }
    out.push_str(rest);
    out
}

/// The reverse of `this_row_in_file_form`: `Table[[#This Row],[Column]]` as `[@Column]`, with or without the
/// table name in front.
fn this_row_in_bar_form(text: &str) -> String {
    let marker = "[[#This Row],[";
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(marker) {
        let after = &rest[at + marker.len()..];
        let Some(end) = after.find("]]") else { break };
        let before =
            rest[..at].trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '.');
        out.push_str(before);
        out.push_str(&format!("[@{}]", &after[..end]));
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// The name a formula uses for a table: characters a name cannot hold become `_`, and a name that reads as a
/// cell reference gets a `_` after it.
fn display_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '.' { c } else { '_' })
        .collect();
    if out.starts_with(|c: char| c.is_ascii_digit()) || CellRef::parse(&out).is_some() {
        out.push('_');
    }
    out
}

/// The text of a column in a structured reference, with the characters that need an apostrophe.
fn structured_column(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if matches!(c, '[' | ']' | '#' | '\'') {
            out.push('\'');
        }
        out.push(c);
    }
    out
}

/// The name of a column not yet used: `Column1`, `Column2` and so on.
fn fresh_column_name(used: &[String]) -> String {
    (1..)
        .map(|n| format!("Column{n}"))
        .find(|c| !used.iter().any(|u| u.eq_ignore_ascii_case(c)))
        .unwrap_or_default()
}

/// A column name that differs from the ones before it: the name followed by 2, then 3, and so on.
fn unique_column_name(name: &str, used: &[String]) -> String {
    if !used.iter().any(|u| u.eq_ignore_ascii_case(name)) {
        return name.to_owned();
    }
    (2..)
        .map(|n| format!("{name}{n}"))
        .find(|c| !used.iter().any(|u| u.eq_ignore_ascii_case(c)))
        .unwrap_or_default()
}

impl Session {
    /// Make a table over a range, with a header row of names.
    pub fn add_table(
        &mut self,
        sheet: usize,
        range: Range,
        name: Option<&str>,
    ) -> Result<String, String> {
        self.add_table_with(sheet, range, name, &TableOptions::default())
    }

    /// Make a table over a range. Without headers a header row is made above the range, moving the cells
    /// below it down.
    pub fn add_table_with(
        &mut self,
        sheet: usize,
        range: Range,
        name: Option<&str>,
        options: &TableOptions,
    ) -> Result<String, String> {
        self.check_sheet(sheet)?;
        if self.book.tables.iter().any(|t| t.sheet == sheet && t.range.intersects(&range)) {
            return Err("a table cannot overlap another table".to_owned());
        }
        // Merged cells under a new table are unmerged, as Excel does.
        self.book.sheets[sheet].merges.retain(|m| !m.intersects(&range));
        let name = self.new_table_name(name)?;
        self.refuse_array_member(sheet, range)?;
        self.refuse_locked(sheet, range)?;
        self.remember_whole("Table");
        let mut range = range;
        if !options.headers {
            let band = Some((range.first.col, range.last.col));
            self.structural(sheet, Axis::Rows, range.first.row, 1, band);
            range = Range::new(range.first, CellRef::new(range.last.row + 1, range.last.col));
        }
        let columns = self.header_names(sheet, range, options.headers);
        let style = options.style.clone().unwrap_or_else(|| DEFAULT_STYLE.to_owned());
        let id = self.book.tables.iter().map(|t| self.table_id(t)).max().unwrap_or(0) + 1;
        let xml = new_table_xml(id, &name, range, &columns, &style);
        for (i, column) in columns.iter().enumerate() {
            let at = CellRef::new(range.first.row, range.first.col + i as u32);
            self.put_text(sheet, at, column);
        }
        self.book.tables.push(Table {
            sheet,
            name: name.clone(),
            range,
            columns,
            header_row: true,
            totals_row: false,
            part: String::new(),
            xml,
        });
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(name)
    }

    /// The name a new table gets: the one given when it is valid and free, otherwise `Table1`, `Table2`...
    fn new_table_name(&self, name: Option<&str>) -> Result<String, String> {
        match name {
            Some(n) => {
                check_defined_name(n)?;
                if self.book.table(n).is_some()
                    || self.book.names.iter().any(|d| d.name.eq_ignore_ascii_case(n))
                {
                    return Err("That name is already in use. Try a different name.".to_owned());
                }
                Ok(n.to_owned())
            }
            None => Ok((1..)
                .map(|n| format!("Table{n}"))
                .find(|n| self.book.table(n).is_none())
                .unwrap_or_default()),
        }
    }

    /// The `id` a table part carries.
    fn table_id(&self, table: &Table) -> usize {
        Document::parse(&table.xml)
            .and_then(|d| d.root.attr("id"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    /// The column names of a range: the header cells as text, with a name made for each one that is empty
    /// or repeats an earlier one.
    fn header_names(&self, sheet: usize, range: Range, headers: bool) -> Vec<String> {
        let s = &self.book.sheets[sheet];
        let texts: Vec<String> = (range.first.col..=range.last.col)
            .map(|c| {
                let cell = s.cell(CellRef::new(range.first.row, c)).cloned().unwrap_or_default();
                if headers {
                    displayed(&self.book, &cell)
                } else {
                    String::new()
                }
            })
            .collect();
        // A name made up must also differ from the headers that come later, which keep their own.
        let mut names: Vec<String> = Vec::new();
        // With a repeated header in the row, the names made up for blanks start counting at the column count.
        let repeats = texts
            .iter()
            .enumerate()
            .any(|(i, t)| !t.is_empty() && texts[..i].iter().any(|u| u.eq_ignore_ascii_case(t)));
        let blank_start = if repeats { texts.len() } else { 1 };
        for text in texts.iter() {
            let taken = |n: &str, names: &[String]| {
                names
                    .iter()
                    .chain(texts.iter().filter(|t| !t.is_empty()))
                    .any(|u| u.eq_ignore_ascii_case(n))
            };
            let name = if text.is_empty() {
                (blank_start..)
                    .map(|n| format!("Column{n}"))
                    .find(|c| !taken(c, &names))
                    .unwrap_or_default()
            } else if names.iter().any(|u| u.eq_ignore_ascii_case(text)) {
                (2..).map(|n| format!("{text}{n}")).find(|c| !taken(c, &names)).unwrap_or_default()
            } else {
                text.clone()
            };
            names.push(name);
        }
        names
    }

    // --------------------------------------------------------------------------------------------------
    // Reading and writing a table's definition.

    pub(crate) fn table_index(&self, name: &str) -> Result<usize, String> {
        self.book
            .tables
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("There is no table named {name}."))
    }

    /// Change a table's XML and read the model back from it.
    fn edit_table(
        &mut self,
        index: usize,
        change: impl FnOnce(&mut Document, &Table) -> Result<(), String>,
    ) -> Result<(), String> {
        let table = self.book.tables[index].clone();
        let mut doc = Document::parse(&table.xml)
            .ok_or("The table definition could not be read.".to_owned())?;
        change(&mut doc, &table)?;
        let t = &mut self.book.tables[index];
        t.xml = doc.to_xml();
        sync_model(t, &doc);
        Ok(())
    }

    // --------------------------------------------------------------------------------------------------
    // Style.

    /// How a table looks.
    pub fn table_style(&self, name: &str) -> Result<TableStyle, String> {
        let index = self.table_index(name)?;
        let doc = Document::parse(&self.book.tables[index].xml).ok_or("unreadable table")?;
        let info = doc.root.child("tableStyleInfo");
        let flag = |n: &str| info.and_then(|i| i.attr(n)).is_some_and(|v| v == "1" || v == "true");
        let hidden = doc
            .root
            .child("autoFilter")
            .map(|a| {
                a.elements().any(|c| {
                    c.name == "filterColumn" && c.attr("hiddenButton").is_some_and(|v| v == "1")
                })
            })
            .unwrap_or(false);
        Ok(TableStyle {
            name: info.and_then(|i| i.attr("name")),
            row_stripes: flag("showRowStripes"),
            column_stripes: flag("showColumnStripes"),
            first_column: flag("showFirstColumn"),
            last_column: flag("showLastColumn"),
            filter_buttons: !hidden,
        })
    }

    /// Set the style, its options and whether the header row shows the filter buttons.
    pub fn set_table_style(&mut self, name: &str, style: &TableStyle) -> Result<(), String> {
        let index = self.table_index(name)?;
        self.remember_whole("Table style");
        self.edit_table(index, |doc, _| {
            let mut info = Element::new("tableStyleInfo");
            if let Some(n) = style.name.as_ref().filter(|n| !n.is_empty()) {
                info.set_attr("name", n);
            }
            let flag = |b: bool| if b { "1" } else { "0" };
            info.set_attr("showFirstColumn", flag(style.first_column));
            info.set_attr("showLastColumn", flag(style.last_column));
            info.set_attr("showRowStripes", flag(style.row_stripes));
            info.set_attr("showColumnStripes", flag(style.column_stripes));
            if let Some(old) = doc.root.child("tableStyleInfo") {
                // Keep the attributes this version does not model.
                for (k, v) in &old.attrs {
                    if !info.attrs.iter().any(|(n, _)| n == k) && k != "name" {
                        info.attrs.push((k.clone(), v.clone()));
                    }
                }
            }
            doc.root.put_child(info, &["autoFilter", "sortState", "tableColumns"]);
            set_filter_buttons(doc, style.filter_buttons);
            Ok(())
        })?;
        self.modified = true;
        Ok(())
    }

    // --------------------------------------------------------------------------------------------------
    // Header row, totals row and the function of each total.

    /// Show or hide the header row. Hiding it clears the header cells; showing it makes a row above the
    /// data and writes the column names into it.
    pub fn set_table_header_row(&mut self, name: &str, show: bool) -> Result<(), String> {
        let index = self.table_index(name)?;
        let table = self.book.tables[index].clone();
        if table.header_row == show {
            return Ok(());
        }
        self.remember_whole(if show { "Show header row" } else { "Hide header row" });
        let sheet = table.sheet;
        if show {
            let (first, last) = (table.range.first, table.range.last);
            let above_is_empty =
                first.row > 0 && self.row_is_empty(sheet, first.row - 1, first.col, last.col);
            let top = if above_is_empty {
                first.row - 1
            } else {
                self.structural(sheet, Axis::Rows, first.row, 1, Some((first.col, last.col)));
                first.row
            };
            let range = Range::new(
                CellRef::new(top, first.col),
                CellRef::new(last.row + u32::from(!above_is_empty), last.col),
            );
            self.edit_table(index, |doc, t| {
                write_ref(doc, range, true, t.totals_row);
                Ok(())
            })?;
            for (i, c) in table.columns.iter().enumerate() {
                self.put_text(sheet, CellRef::new(range.first.row, range.first.col + i as u32), c);
            }
        } else {
            for c in table.range.first.col..=table.range.last.col {
                self.clear_cell(sheet, CellRef::new(table.range.first.row, c));
            }
            let range = Range::new(
                CellRef::new(table.range.first.row + 1, table.range.first.col),
                table.range.last,
            );
            self.edit_table(index, |doc, t| {
                write_ref(doc, range, false, t.totals_row);
                Ok(())
            })?;
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Show or hide the totals row. Showing it adds a row below the data, moving the cells below it down:
    /// the first column says Total and the last adds up, unless the table remembers what it had before.
    pub fn set_table_totals_row(&mut self, name: &str, show: bool) -> Result<(), String> {
        let index = self.table_index(name)?;
        let table = self.book.tables[index].clone();
        if table.totals_row == show {
            return Ok(());
        }
        self.remember_whole(if show { "Show totals row" } else { "Hide totals row" });
        let sheet = table.sheet;
        if show {
            // The totals row is a row of cells inserted under the table, whatever is below.
            let band = Some((table.range.first.col, table.range.last.col));
            self.structural(sheet, Axis::Rows, table.range.last.row + 1, 1, band);
            let range = Range::new(
                table.range.first,
                CellRef::new(table.range.last.row + 1, table.range.last.col),
            );
            self.edit_table(index, |doc, _| {
                write_ref(doc, range, table.header_row, true);
                default_totals(doc);
                Ok(())
            })?;
            self.write_totals_row(index);
        } else {
            for c in table.range.first.col..=table.range.last.col {
                self.clear_cell(sheet, CellRef::new(table.range.last.row, c));
            }
            let range = Range::new(
                table.range.first,
                CellRef::new(table.range.last.row - 1, table.range.last.col),
            );
            self.edit_table(index, |doc, _| {
                write_ref(doc, range, table.header_row, false);
                Ok(())
            })?;
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// The function a column of the totals row shows.
    pub fn table_total_function(&self, name: &str, column: usize) -> Result<TotalFunction, String> {
        let index = self.table_index(name)?;
        let doc = Document::parse(&self.book.tables[index].xml).ok_or("unreadable table")?;
        let columns = doc.root.child("tableColumns").ok_or("unreadable table")?;
        let c = columns.elements().nth(column).ok_or("There is no such column.")?;
        Ok(c.attr("totalsRowFunction")
            .and_then(|f| TotalFunction::parse(&f))
            .unwrap_or(TotalFunction::None))
    }

    /// Set the function of a column of the totals row, which must be showing.
    pub fn set_table_total_function(
        &mut self,
        name: &str,
        column: usize,
        function: TotalFunction,
    ) -> Result<(), String> {
        let index = self.table_index(name)?;
        let table = self.book.tables[index].clone();
        if !table.totals_row {
            return Err("The table has no totals row.".to_owned());
        }
        if column >= table.columns.len() {
            return Err("There is no such column.".to_owned());
        }
        if function == TotalFunction::Custom {
            return Err("A custom total is a formula typed in the cell.".to_owned());
        }
        self.remember_whole("Total function");
        self.edit_table(index, |doc, _| {
            let columns = doc.root.child_mut("tableColumns").ok_or("unreadable table")?;
            let c = columns.elements_mut().nth(column).ok_or("There is no such column.")?;
            c.remove_attr("totalsRowLabel");
            c.remove_children("totalsRowFormula");
            if function == TotalFunction::None {
                c.remove_attr("totalsRowFunction");
            } else {
                c.set_attr("totalsRowFunction", function.xml_name());
            }
            Ok(())
        })?;
        let at = CellRef::new(table.range.last.row, table.range.first.col + column as u32);
        self.write_total_cell(index, column, at);
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Write every cell of the totals row from the definition.
    fn write_totals_row(&mut self, index: usize) {
        let table = self.book.tables[index].clone();
        for column in 0..table.columns.len() {
            let at = CellRef::new(table.range.last.row, table.range.first.col + column as u32);
            self.write_total_cell(index, column, at);
        }
    }

    /// Write one cell of the totals row: its label, its `SUBTOTAL`, its own formula, or nothing.
    fn write_total_cell(&mut self, index: usize, column: usize, at: CellRef) {
        let table = self.book.tables[index].clone();
        let Some(doc) = Document::parse(&table.xml) else { return };
        let Some(c) = doc.root.child("tableColumns").and_then(|t| t.elements().nth(column)) else {
            return;
        };
        let sheet = table.sheet;
        if let Some(label) = c.attr("totalsRowLabel") {
            self.put_text(sheet, at, &label);
            return;
        }
        let function = c.attr("totalsRowFunction").and_then(|f| TotalFunction::parse(&f));
        let name = &table.columns[column];
        match function {
            Some(TotalFunction::Custom) => {
                let formula = c.child("totalsRowFormula").map(|f| f.text()).unwrap_or_default();
                self.put_formula(sheet, at, &formula);
            }
            Some(f) => match f.subtotal_code() {
                Some(code) => {
                    let text =
                        format!("SUBTOTAL({code},{}[{}])", table.name, structured_column(name));
                    self.put_formula(sheet, at, &text);
                }
                None => self.clear_cell(sheet, at),
            },
            None => self.clear_cell(sheet, at),
        }
    }

    /// The table's definition after a cell of its totals row was typed into: a label, or a formula that is
    /// one of the standard totals or a custom one.
    pub(crate) fn note_typing_in_totals(&mut self, sheet: usize, at: CellRef) {
        let Some(index) = self.book.tables.iter().position(|t| {
            t.sheet == sheet && t.totals_row && t.range.contains(at) && at.row == t.range.last.row
        }) else {
            return;
        };
        let table = self.book.tables[index].clone();
        let column = (at.col - table.range.first.col) as usize;
        let cell = self.book.sheets[sheet].cell(at).cloned().unwrap_or_default();
        let result = self.edit_table(index, |doc, _| {
            let columns = doc.root.child_mut("tableColumns").ok_or("unreadable table")?;
            let c = columns.elements_mut().nth(column).ok_or("There is no such column.")?;
            c.remove_attr("totalsRowLabel");
            c.remove_attr("totalsRowFunction");
            c.remove_children("totalsRowFormula");
            match &cell.formula {
                Some(f) => {
                    let text = f.display();
                    let body = text.trim_start_matches('=').to_owned();
                    let standard = standard_total(&body, &table.name, &table.columns[column]);
                    match standard {
                        Some(function) => c.set_attr("totalsRowFunction", function.xml_name()),
                        None => {
                            c.set_attr("totalsRowFunction", "custom");
                            let mut formula = Element::new("totalsRowFormula");
                            formula.set_text(&body);
                            c.children.push(Node::Element(formula));
                        }
                    }
                }
                None => {
                    if let Value::Text(t) = &cell.value {
                        c.set_attr("totalsRowLabel", t);
                    }
                }
            }
            Ok(())
        });
        let _ = result;
    }

    // --------------------------------------------------------------------------------------------------
    // Resize, convert to a range, names and columns.

    /// Make a table cover a different range: the header row stays in its row, at least one data row stays,
    /// and the range overlaps the table it was. A column added takes its name from its header cell.
    pub fn resize_table(&mut self, name: &str, range: Range) -> Result<(), String> {
        let index = self.table_index(name)?;
        let table = self.book.tables[index].clone();
        let sheet = table.sheet;
        let first_data = range.first.row + u32::from(table.header_row);
        let aligned = (!table.header_row || range.first.row == table.range.first.row)
            && range.last.row >= first_data
            && range.intersects(&table.range);
        if !aligned {
            return Err(RANGE_MESSAGE.to_owned());
        }
        if self
            .book
            .tables
            .iter()
            .enumerate()
            .any(|(i, t)| i != index && t.sheet == sheet && t.range.intersects(&range))
        {
            return Err(OVERLAP_MESSAGE.to_owned());
        }
        self.remember_whole("Resize table");
        let totals = table.totals_row;
        // The totals row moves to the new last row, cells and all.
        let carried = if totals { self.take_totals_cells(&table) } else { Vec::new() };
        let body_last =
            if totals { range.last.row.saturating_sub(1).max(first_data) } else { range.last.row };
        let data = Range::new(range.first, CellRef::new(body_last, range.last.col));
        self.set_table_columns(index, data);
        let calculated = self.calculated_formulas(index);
        self.fill_new_rows(
            index,
            &calculated,
            table.range.last.row - u32::from(totals),
            data.last.row,
        );
        if totals {
            let whole = Range::new(
                range.first,
                CellRef::new(range.last.row.max(body_last + 1), range.last.col),
            );
            let _ = self.edit_table(index, |doc, t| {
                write_ref(doc, whole, t.header_row, true);
                Ok(())
            });
            for (col, cell) in carried {
                if col <= whole.last.col {
                    self.book.sheets[sheet].set(CellRef::new(whole.last.row, col), cell);
                }
            }
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Lift the cells of a table's totals row out of the sheet, with their columns.
    fn take_totals_cells(&mut self, table: &Table) -> Vec<(u32, Cell)> {
        let row = table.range.last.row;
        let s = &mut self.book.sheets[table.sheet];
        (table.range.first.col..=table.range.last.col)
            .filter_map(|c| s.cells.remove(&(row, c)).map(|cell| (c, cell)))
            .collect()
    }

    /// Make the table's definition and header cells match a new data range, with the totals row off.
    fn set_table_columns(&mut self, index: usize, range: Range) {
        let table = self.book.tables[index].clone();
        let sheet = table.sheet;
        let mut names: Vec<String> = Vec::new();
        for (i, c) in (range.first.col..=range.last.col).enumerate() {
            let existing = (c >= table.range.first.col && c <= table.range.last.col)
                .then(|| table.columns.get((c - table.range.first.col) as usize).cloned())
                .flatten();
            let name = match existing {
                Some(n) => n,
                None => {
                    let cell =
                        self.book.sheets[sheet].cell(CellRef::new(range.first.row, c)).cloned();
                    let text = if table.header_row {
                        cell.map(|x| displayed(&self.book, &x)).unwrap_or_default()
                    } else {
                        String::new()
                    };
                    if text.is_empty() {
                        fresh_column_name(&names)
                    } else {
                        unique_column_name(&text, &names)
                    }
                }
            };
            names.push(name.clone());
            if table.header_row {
                self.put_text(sheet, CellRef::new(range.first.row, c), &name);
            }
            let _ = i;
        }
        let kept: Vec<usize> = (range.first.col..=range.last.col)
            .map(|c| {
                (c >= table.range.first.col && c <= table.range.last.col)
                    .then(|| (c - table.range.first.col) as usize)
            })
            .map(|o| o.unwrap_or(usize::MAX))
            .collect();
        let _ = self.edit_table(index, |doc, t| {
            write_ref(doc, range, t.header_row, false);
            rebuild_columns(doc, &names, &kept);
            Ok(())
        });
    }

    /// Turn a table into a plain range: the cells stay, formulas that named the table now name its cells,
    /// and the table goes.
    pub fn table_to_range(&mut self, name: &str) -> Result<(), String> {
        let index = self.table_index(name)?;
        self.remember_whole("Convert to range");
        let table = self.book.tables[index].clone();
        self.rewrite_structured_references(&table);
        // The table's filter goes with it, so the rows it hid are shown.
        for row in table.range.first.row..=table.range.last.row {
            if let Some(info) = self.book.sheets[table.sheet].rows.get_mut(&row) {
                info.hidden = false;
            }
        }
        self.book.tables.remove(index);
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Give a table another name, in its definition and in every formula that uses it.
    pub fn rename_table(&mut self, name: &str, new_name: &str) -> Result<(), String> {
        let index = self.table_index(name)?;
        let display = display_name(new_name);
        if display.is_empty() {
            return Err("That name is not valid.".to_owned());
        }
        if self
            .book
            .tables
            .iter()
            .enumerate()
            .any(|(i, t)| i != index && t.name.eq_ignore_ascii_case(new_name))
            || self.book.names.iter().any(|d| d.name.eq_ignore_ascii_case(new_name))
        {
            return Err("That name is already in use. Try a different name.".to_owned());
        }
        self.remember_whole("Rename table");
        let old = Document::parse(&self.book.tables[index].xml)
            .and_then(|d| d.root.attr("displayName"))
            .unwrap_or_else(|| self.book.tables[index].name.clone());
        self.edit_table(index, |doc, _| {
            doc.root.set_attr("name", new_name);
            doc.root.set_attr("displayName", &display);
            Ok(())
        })?;
        self.book.tables[index].name = new_name.to_owned();
        self.rename_in_formulas(&old, &display);
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Give a column another name: its header cell, its definition and the formulas that use it.
    pub fn rename_table_column(
        &mut self,
        name: &str,
        column: usize,
        new_name: &str,
    ) -> Result<(), String> {
        let index = self.table_index(name)?;
        let table = self.book.tables[index].clone();
        let old = table.columns.get(column).cloned().ok_or("There is no such column.")?;
        if table
            .columns
            .iter()
            .enumerate()
            .any(|(i, c)| i != column && c.eq_ignore_ascii_case(new_name))
        {
            return Err("That name is already in use. Try a different name.".to_owned());
        }
        self.remember_whole("Rename column");
        self.edit_table(index, |doc, _| {
            let columns = doc.root.child_mut("tableColumns").ok_or("unreadable table")?;
            let c = columns.elements_mut().nth(column).ok_or("There is no such column.")?;
            c.set_attr("name", new_name);
            Ok(())
        })?;
        if table.header_row {
            self.put_text(
                table.sheet,
                CellRef::new(table.range.first.row, table.range.first.col + column as u32),
                new_name,
            );
        }
        self.rename_column_in_formulas(&table.name, &old, new_name);
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Add a column at the right of the table, called `name` or the first `ColumnN` that is free.
    pub fn add_table_column(
        &mut self,
        name: &str,
        column_name: Option<&str>,
    ) -> Result<(), String> {
        let index = self.table_index(name)?;
        let table = self.book.tables[index].clone();
        let new_name = match column_name {
            // A name that is taken is replaced by a made up one, as Excel does.
            Some(n) if !table.columns.iter().any(|c| c.eq_ignore_ascii_case(n)) => n.to_owned(),
            _ => fresh_column_name(&table.columns),
        };
        self.remember_whole("Add column");
        let range = Range::new(
            table.range.first,
            CellRef::new(table.range.last.row, table.range.last.col + 1),
        );
        let band = Some((range.last.col, range.last.col));
        let _ = band;
        let mut names = table.columns.clone();
        names.push(new_name.clone());
        let kept: Vec<usize> = (0..names.len())
            .map(|i| if i < table.columns.len() { i } else { usize::MAX })
            .collect();
        self.edit_table(index, |doc, t| {
            write_ref(doc, range, t.header_row, t.totals_row);
            rebuild_columns(doc, &names, &kept);
            Ok(())
        })?;
        if table.header_row {
            self.put_text(table.sheet, CellRef::new(range.first.row, range.last.col), &new_name);
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Add a row below the last data row, with the calculated columns filled in.
    pub fn add_table_row(&mut self, name: &str) -> Result<(), String> {
        let index = self.table_index(name)?;
        let table = self.book.tables[index].clone();
        let sheet = table.sheet;
        self.remember_whole("Add table row");
        let at = table.range.last.row + 1 - u32::from(table.totals_row);
        self.push_down_blocks(sheet, at, table.range.first.col, table.range.last.col);
        let range = Range::new(
            table.range.first,
            CellRef::new(table.range.last.row + 1, table.range.last.col),
        );
        self.edit_table(index, |doc, t| {
            write_ref(doc, range, t.header_row, t.totals_row);
            Ok(())
        })?;
        let calculated = self.calculated_formulas(index);
        self.fill_new_rows(index, &calculated, at - 1, at);
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Delete data rows of a table (counted from 1), moving the cells below them up in the table's columns.
    pub fn delete_table_rows(
        &mut self,
        name: &str,
        first_row: u32,
        count: u32,
    ) -> Result<(), String> {
        let index = self.table_index(name)?;
        let table = self.book.tables[index].clone();
        let data = table.data_range().ok_or("The table has no data rows.")?;
        if first_row == 0 || count == 0 || first_row - 1 + count > data.rows() {
            return Err("There is no such row.".to_owned());
        }
        if count == data.rows() && table.header_row {
            // The last data row stays, emptied, because a table keeps at least one.
        }
        self.remember_whole("Delete table rows");
        let at = data.first.row + first_row - 1;
        let band = Some((table.range.first.col, table.range.last.col));
        let keep_one = count == data.rows();
        let removed = if keep_one { count - 1 } else { count };
        if keep_one {
            for c in table.range.first.col..=table.range.last.col {
                self.clear_cell(table.sheet, CellRef::new(data.first.row, c));
            }
        }
        if removed > 0 {
            self.structural(
                table.sheet,
                Axis::Rows,
                at + u32::from(keep_one),
                -(removed as i64),
                band,
            );
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    // --------------------------------------------------------------------------------------------------
    // Calculated columns.

    /// The formula of each calculated column, as text without the equals sign, by column offset.
    fn calculated_formulas(&self, index: usize) -> Vec<(usize, String)> {
        let table = &self.book.tables[index];
        let Some(doc) = Document::parse(&table.xml) else { return Vec::new() };
        let Some(columns) = doc.root.child("tableColumns") else { return Vec::new() };
        columns
            .elements()
            .enumerate()
            .filter_map(|(i, c)| c.child("calculatedColumnFormula").map(|f| (i, f.text())))
            .collect()
    }

    /// Put the calculated formulas into rows `after + 1` to `last` of the table.
    fn fill_new_rows(&mut self, index: usize, formulas: &[(usize, String)], after: u32, last: u32) {
        let table = self.book.tables[index].clone();
        for row in after + 1..=last {
            for (offset, formula) in formulas {
                let at = CellRef::new(row, table.range.first.col + *offset as u32);
                self.put_formula(table.sheet, at, &this_row_in_bar_form(formula));
            }
        }
    }

    /// After text was typed into a table's header cell, the column takes that name and the formulas that
    /// use the old name follow it.
    fn note_typing_in_header(&mut self, sheet: usize, at: CellRef) {
        let Some(index) = self.book.tables.iter().position(|t| {
            t.sheet == sheet
                && t.header_row
                && t.range.first.row == at.row
                && (t.range.first.col..=t.range.last.col).contains(&at.col)
        }) else {
            return;
        };
        let table = self.book.tables[index].clone();
        let column = (at.col - table.range.first.col) as usize;
        let Some(cell) = self.book.sheets[sheet].cell(at).cloned() else { return };
        let typed = displayed(&self.book, &cell);
        let others: Vec<String> = table
            .columns
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != column)
            .map(|(_, c)| c.clone())
            .collect();
        let new_name = if typed.is_empty() {
            fresh_column_name(&others)
        } else {
            unique_column_name(&typed, &others)
        };
        let old = table.columns[column].clone();
        if new_name == old {
            return;
        }
        let _ = self.edit_table(index, |doc, _| {
            let columns = doc.root.child_mut("tableColumns").ok_or("unreadable table")?;
            let c = columns.elements_mut().nth(column).ok_or("There is no such column.")?;
            c.set_attr("name", &new_name);
            Ok(())
        });
        if new_name != typed {
            self.put_text(sheet, at, &new_name);
        }
        self.rename_column_in_formulas(&table.name, &old, &new_name);
    }

    /// After a formula was typed into a cell of a table's data, fill it down the column when the column
    /// has nothing else in it, and remember it as the column's calculated formula.
    pub(crate) fn note_typing_in_table(&mut self, sheet: usize, at: CellRef) {
        self.note_typing_in_header(sheet, at);
        if !self.table_auto_fill {
            return;
        }
        let Some(index) = self
            .book
            .tables
            .iter()
            .position(|t| t.sheet == sheet && t.data_range().is_some_and(|d| d.contains(at)))
        else {
            return;
        };
        let table = self.book.tables[index].clone();
        let Some(data) = table.data_range() else { return };
        let Some(formula) = self.book.sheets[sheet].cell(at).and_then(|c| c.formula.clone()) else {
            return;
        };
        let others_empty = (data.first.row..=data.last.row).filter(|r| *r != at.row).all(|r| {
            let cell = self.book.sheets[sheet].cell(CellRef::new(r, at.col));
            cell.is_none_or(|c| c.value.is_empty() && c.formula.is_none())
        });
        if !others_empty {
            return;
        }
        let column = (at.col - table.range.first.col) as usize;
        let body =
            this_row_in_file_form(&crate::formula::print::print(&formula.expr, true), &table.name);
        let data_format = self.dxf_with(crate::styles::Dxf {
            num_fmt: Some("General".to_owned()),
            ..Default::default()
        });
        for r in data.first.row..=data.last.row {
            if r == at.row {
                continue;
            }
            let moved = crate::formula::Formula::from_expr(
                crate::formula::shifted(&formula.expr, r as i64 - at.row as i64, 0),
                formula.dynamic,
            );
            self.put_formula(sheet, CellRef::new(r, at.col), &moved.display());
        }
        let _ = self.edit_table(index, |doc, _| {
            let columns = doc.root.child_mut("tableColumns").ok_or("unreadable table")?;
            let c = columns.elements_mut().nth(column).ok_or("There is no such column.")?;
            c.remove_children("calculatedColumnFormula");
            c.set_attr("dataDxfId", &data_format.to_string());
            let mut f = Element::new("calculatedColumnFormula");
            f.set_text(&body);
            c.children.insert(0, Node::Element(f));
            Ok(())
        });
    }

    // --------------------------------------------------------------------------------------------------
    // Cells written without a step of their own.

    /// Whether the cells of a row between two columns are all empty.
    fn row_is_empty(&self, sheet: usize, row: u32, first: u32, last: u32) -> bool {
        (first..=last).all(|c| {
            self.book.sheets[sheet]
                .cell(CellRef::new(row, c))
                .is_none_or(|x| x.value.is_empty() && x.formula.is_none())
        })
    }

    /// Make room for one more row at `row` in some columns: in each column the cells from `row` down to the
    /// first empty cell move down one, and the empty cell takes up the move.
    fn push_down_blocks(&mut self, sheet: usize, row: u32, first: u32, last: u32) {
        for col in first..=last {
            let filled = |s: &Session, r: u32| {
                s.book.sheets[sheet]
                    .cell(CellRef::new(r, col))
                    .is_some_and(|x| !x.value.is_empty() || x.formula.is_some())
            };
            let mut run = 0;
            while filled(self, row + run) {
                run += 1;
            }
            if run == 0 {
                continue;
            }
            self.structural(sheet, Axis::Rows, row, 1, Some((col, col)));
            self.structural(sheet, Axis::Rows, row + run + 1, -1, Some((col, col)));
        }
    }

    fn put_text(&mut self, sheet: usize, at: CellRef, text: &str) {
        let style = self.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
        self.book.sheets[sheet]
            .set(at, Cell { value: Value::text(text), style, ..Default::default() });
        self.engine.changed(&self.book, sheet, at);
    }

    fn clear_cell(&mut self, sheet: usize, at: CellRef) {
        let style = self.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
        if style == 0 {
            self.book.sheets[sheet].cells.remove(&(at.row, at.col));
        } else {
            self.book.sheets[sheet].set(at, Cell { style, ..Default::default() });
        }
        self.engine.changed(&self.book, sheet, at);
    }

    fn put_formula(&mut self, sheet: usize, at: CellRef, text: &str) {
        let Ok(formula) = Formula::parse(text) else { return };
        let style = self.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
        self.book.sheets[sheet]
            .set(at, Cell { formula: Some(formula), style, ..Default::default() });
        self.engine.changed(&self.book, sheet, at);
    }
}

// ------------------------------------------------------------------------------------------------------
// The table part.

/// The XML of a new table.
fn new_table_xml(id: usize, name: &str, range: Range, columns: &[String], style: &str) -> String {
    let cols: String = columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!("<tableColumn id=\"{}\" name=\"{}\"/>", i + 1, crate::xlsx::escape(c))
        })
        .collect();
    let style = if style.is_empty() {
        String::new()
    } else {
        format!(" name=\"{}\"", crate::xlsx::escape(style))
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<table xmlns=\"{}\" id=\"{id}\" name=\"{name}\" displayName=\"{name}\" ref=\"{r}\" totalsRowShown=\"0\"><autoFilter ref=\"{r}\"/><tableColumns count=\"{}\">{cols}</tableColumns><tableStyleInfo{style} showFirstColumn=\"0\" showLastColumn=\"0\" showRowStripes=\"1\" showColumnStripes=\"0\"/></table>",
        crate::xlsx::MAIN_NS,
        columns.len(),
        r = range.a1()
    )
}

/// Read the model fields of a table from its definition.
fn sync_model(table: &mut Table, doc: &Document) {
    if let Some(r) = doc.root.attr("ref").and_then(|r| Range::parse(&r)) {
        table.range = r;
    }
    table.header_row = doc.root.attr("headerRowCount").is_none_or(|v| v != "0");
    table.totals_row = doc.root.attr("totalsRowCount").is_some_and(|v| v != "0");
    if let Some(columns) = doc.root.child("tableColumns") {
        table.columns = columns.elements().map(|c| c.attr("name").unwrap_or_default()).collect();
    }
}

/// Write the range, the header and totals flags, and the range of the filter.
fn write_ref(doc: &mut Document, range: Range, header: bool, totals: bool) {
    doc.root.set_attr("ref", &range.a1());
    if header {
        doc.root.remove_attr("headerRowCount");
    } else {
        doc.root.set_attr("headerRowCount", "0");
    }
    if totals {
        doc.root.remove_attr("totalsRowShown");
        doc.root.set_attr("totalsRowCount", "1");
    } else {
        doc.root.remove_attr("totalsRowCount");
        if doc.root.attr("totalsRowShown").is_none() {
            // A table that has had a totals row and lost it carries neither attribute.
        }
    }
    let last = range.last.row - u32::from(totals);
    let filter = Range::new(range.first, CellRef::new(last, range.last.col));
    if !header {
        doc.root.remove_children("autoFilter");
        return;
    }
    match doc.root.child_mut("autoFilter") {
        Some(f) => f.set_attr("ref", &filter.a1()),
        None => {
            let mut f = Element::new("autoFilter");
            f.set_attr("ref", &filter.a1());
            doc.root.children.insert(0, Node::Element(f));
        }
    }
}

/// Show or hide the filter buttons by writing a `hiddenButton` for each column of the filter.
fn set_filter_buttons(doc: &mut Document, shown: bool) {
    let count = doc.root.child("tableColumns").map_or(0, |c| c.elements().count());
    let Some(filter) = doc.root.child_mut("autoFilter") else { return };
    if shown {
        for c in filter.elements_mut().filter(|c| c.name == "filterColumn") {
            c.remove_attr("hiddenButton");
        }
        filter.children.retain(|n| !matches!(n, Node::Element(e) if e.name == "filterColumn" && e.attrs.is_empty() && e.children.is_empty()));
        return;
    }
    for column in 0..count {
        let id = column.to_string();
        let existing = filter
            .elements_mut()
            .find(|c| c.name == "filterColumn" && c.attr("colId").as_deref() == Some(&id));
        match existing {
            Some(c) => c.set_attr("hiddenButton", "1"),
            None => {
                let mut c = Element::new("filterColumn");
                c.set_attr("colId", &id);
                c.set_attr("hiddenButton", "1");
                let at = filter
                    .children
                    .iter()
                    .position(|n| matches!(n, Node::Element(e) if e.name == "sortState"))
                    .unwrap_or(filter.children.len());
                filter.children.insert(at, Node::Element(c));
            }
        }
    }
}

/// The labels and functions a table gets the first time its totals row is shown: Total at the left and a
/// sum at the right, unless a column already has something.
fn default_totals(doc: &mut Document) {
    let Some(columns) = doc.root.child_mut("tableColumns") else { return };
    let remembered = columns
        .elements()
        .any(|c| c.attr("totalsRowLabel").is_some() || c.attr("totalsRowFunction").is_some());
    if remembered {
        return;
    }
    let last = columns.elements().count().saturating_sub(1);
    for (i, c) in columns.elements_mut().enumerate() {
        if i == 0 && last > 0 {
            c.set_attr("totalsRowLabel", "Total");
        }
        if i == last {
            c.set_attr("totalsRowFunction", "sum");
        }
    }
}

/// Rebuild the `tableColumns` for new names. `kept[i]` is the old position of the new column `i`, or
/// `usize::MAX` for a column that is new.
fn rebuild_columns(doc: &mut Document, names: &[String], kept: &[usize]) {
    let Some(columns) = doc.root.child_mut("tableColumns") else { return };
    let old: Vec<Element> = columns.elements().cloned().collect();
    let next_id = old
        .iter()
        .filter_map(|c| c.attr("id").and_then(|v| v.parse::<usize>().ok()))
        .max()
        .unwrap_or(0);
    let mut fresh = 0;
    let mut rebuilt: Vec<Node> = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let element = match kept.get(i).and_then(|k| old.get(*k)) {
            Some(existing) => {
                let mut e = existing.clone();
                e.set_attr("name", name);
                e
            }
            None => {
                fresh += 1;
                let mut e = Element::new("tableColumn");
                e.set_attr("id", &(next_id + fresh).to_string());
                e.set_attr("name", name);
                e
            }
        };
        rebuilt.push(Node::Element(element));
    }
    columns.children = rebuilt;
    columns.set_attr("count", &names.len().to_string());
}

/// The standard total whose formula is `body`, if it is one: `SUBTOTAL(109,Table[Column])`.
fn standard_total(body: &str, table: &str, column: &str) -> Option<TotalFunction> {
    let upper = body.replace(' ', "");
    let reference = format!("{table}[{}]", structured_column(column));
    for f in [
        TotalFunction::Sum,
        TotalFunction::Average,
        TotalFunction::Count,
        TotalFunction::CountNumbers,
        TotalFunction::Max,
        TotalFunction::Min,
        TotalFunction::StdDev,
        TotalFunction::Var,
    ] {
        let code = f.subtotal_code()?;
        if upper.eq_ignore_ascii_case(&format!("SUBTOTAL({code},{reference})")) {
            return Some(f);
        }
    }
    None
}

/// A formula tree with `change` applied to every node first: the node it answers replaces the node and its
/// children are left alone, and `None` leaves the node and goes on to its children.
fn rewrite(expr: &Expr, change: &mut impl FnMut(&Expr) -> Option<Expr>) -> Expr {
    if let Some(replacement) = change(expr) {
        return replacement;
    }
    let mut again = |e: &Expr| rewrite(e, change);
    match expr {
        Expr::Neg(e) => Expr::Neg(Box::new(again(e))),
        Expr::Plus(e) => Expr::Plus(Box::new(again(e))),
        Expr::Percent(e) => Expr::Percent(Box::new(again(e))),
        Expr::Spill(e) => Expr::Spill(Box::new(again(e))),
        Expr::Implicit(e) => Expr::Implicit(Box::new(again(e))),
        Expr::Paren(e) => Expr::Paren(Box::new(again(e))),
        Expr::Binary(op, a, b) => Expr::Binary(*op, Box::new(again(a)), Box::new(again(b))),
        Expr::Intersect(a, b) => Expr::Intersect(Box::new(again(a)), Box::new(again(b))),
        Expr::Call(name, args) => Expr::Call(name.clone(), args.iter().map(&mut again).collect()),
        Expr::Union(args) => Expr::Union(args.iter().map(&mut again).collect()),
        Expr::Invoke(f, args) => {
            Expr::Invoke(Box::new(again(f)), args.iter().map(&mut again).collect())
        }
        other => other.clone(),
    }
}

impl Session {
    /// Replace every structured reference to a table in the workbook with the cells it names. A name Excel
    /// would write with the sheet gets one, and the columns and rows are absolute except the row of `[@Column]`.
    fn rewrite_structured_references(&mut self, table: &Table) {
        let book = self.book.clone();
        let names: Vec<String> = book.sheets.iter().map(|s| s.name.clone()).collect();
        for (si, sheet) in self.book.sheets.iter_mut().enumerate() {
            for (&(r, c), cell) in sheet.cells.iter_mut() {
                let Some(f) = &cell.formula else { continue };
                let at = CellRef::new(r, c);
                let mut changed = false;
                let expr = rewrite(&f.expr, &mut |e| {
                    let replaced = table_to_cells(&book, table, e, si, at, &names)?;
                    changed = true;
                    Some(replaced)
                });
                if changed {
                    cell.formula = Some(Formula::from_expr(expr, f.dynamic));
                }
            }
        }
    }

    /// Every formula of the workbook with `change` applied: the cells' and the defined names'.
    fn change_formulas(&mut self, change: &mut impl FnMut(&Expr) -> Option<Expr>) {
        for sheet in &mut self.book.sheets {
            for cell in sheet.cells.values_mut() {
                let Some(f) = &cell.formula else { continue };
                let expr = rewrite(&f.expr, change);
                if expr != *f.expr {
                    cell.formula = Some(Formula::from_expr(expr, f.dynamic));
                }
            }
        }
        for name in &mut self.book.names {
            let Ok(f) = Formula::parse(&name.formula) else { continue };
            let expr = rewrite(&f.expr, change);
            if expr != *f.expr {
                name.formula = Formula::from_expr(expr, f.dynamic)
                    .display()
                    .trim_start_matches('=')
                    .to_owned();
            }
        }
    }

    fn rename_in_formulas(&mut self, old: &str, new: &str) {
        self.change_formulas(&mut |e| match e {
            Expr::Table(t) if t.table.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(old)) => {
                let mut t = t.clone();
                t.table = Some(Arc::from(new));
                Some(Expr::Table(t))
            }
            Expr::Name(n) if n.eq_ignore_ascii_case(old) => Some(Expr::Name(Arc::from(new))),
            _ => None,
        });
    }

    fn rename_column_in_formulas(&mut self, table: &str, old: &str, new: &str) {
        let (old_text, new_text) = (structured_column(old), structured_column(new));
        self.change_formulas(&mut |e| match e {
            Expr::Table(t) if t.table.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(table)) => {
                let mut t = t.clone();
                for column in [&mut t.first_column, &mut t.last_column].into_iter().flatten() {
                    if column.eq_ignore_ascii_case(old) {
                        *column = Arc::from(new);
                    }
                }
                t.text = Arc::from(t.text.replace(&old_text, &new_text).as_str());
                Some(Expr::Table(t))
            }
            _ => None,
        });
    }
}

/// The cells a structured reference to `table` names, as the reference Excel writes when the table goes.
fn table_to_cells(
    book: &Workbook,
    table: &Table,
    e: &Expr,
    sheet: usize,
    at: CellRef,
    names: &[String],
) -> Option<Expr> {
    let reference = match e {
        Expr::Table(t) => {
            let belongs = match &t.table {
                Some(n) => n.eq_ignore_ascii_case(&table.name),
                None => book.table_at(sheet, at).is_some_and(|x| x.name == table.name),
            };
            if !belongs {
                return None;
            }
            let this_row = t.items.iter().any(|i| &**i == "this row");
            let (target, range) = crate::eval::table_area(book, t, sheet, Some(at)).ok()?;
            (target, range, this_row)
        }
        Expr::Name(n) if n.eq_ignore_ascii_case(&table.name) => {
            let data = table.data_range()?;
            (table.sheet, data, false)
        }
        _ => return None,
    };
    let (target, range, this_row) = reference;
    let mut r = Reference::from_range(range, Some(Arc::from(names[target].as_str())));
    for p in [&mut r.first, &mut r.last] {
        p.col_abs = true;
        p.row_abs = !this_row;
    }
    Some(Expr::Ref(r))
}
