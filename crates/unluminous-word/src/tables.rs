//! Table operations beyond one cell at the caret (task-2359, TDD 5.4 C19 to C22, M2 to M4): a selection of
//! several cells, splitting a table, converting text to a table and back, sorting rows, distributing rows
//! and columns, the Table Properties settings, table styles with their Style Options, and borders.
//!
//! Each is one [`Session`] change, so one undo step. A selection that starts in one cell and ends in
//! another of the same table selects the rectangle of cells between them, as a drag does in Word.

use crate::model::*;
use crate::position::{self, Path, Pos, Selection, StoryId};
use crate::props;
use crate::session::{Scope, Session};
use crate::xml::{El, Node};

/// A rectangle of cells in one table: its path, and the first and last row and cell index.
#[derive(Debug, Clone, PartialEq)]
pub struct CellRange {
    pub table: Path,
    pub rows: (usize, usize),
    pub cells: (usize, usize),
}

/// How sort compares a column's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Text,
    Number,
    Date,
}

impl Session {
    /// The cells the selection covers: the rectangle between the anchor's cell and the head's when both
    /// are in the same table, or the caret's cell.
    pub fn selected_cells(&self) -> Option<CellRange> {
        let story = position::story(&self.document, &self.selection.head.story)?;
        let (t1, r1, c1) = position::cell_of(story, &self.selection.anchor.path)?;
        let (t2, r2, c2) = position::cell_of(story, &self.selection.head.path)?;
        if t1 != t2 {
            return None;
        }
        Some(CellRange {
            table: t1,
            rows: (r1.min(r2) as usize, r1.max(r2) as usize),
            cells: (c1.min(c2) as usize, c1.max(c2) as usize),
        })
    }

    /// Select a cell, the row, the column or the whole table the caret is in, as Word's Select menu does.
    pub fn select_in_table(&mut self, what: &str) -> Result<(), String> {
        let range = self.selected_cells().ok_or("the caret is not in a table")?;
        let story = self.selection.head.story.clone();
        let table = table_at(&self.document, &story, &range.table)?;
        let rows = table.rows.len();
        let (r0, r1, c0, c1) = match what {
            "cell" => (range.rows.0, range.rows.0, range.cells.0, range.cells.0),
            "row" => (range.rows.0, range.rows.1, 0, usize::MAX),
            "column" => (0, rows - 1, range.cells.0, range.cells.1),
            "table" => (0, rows - 1, 0, usize::MAX),
            other => return Err(format!("select takes cell, row, column or table, not {other}")),
        };
        let last_cell = |r: usize, c: usize| c.min(table.rows[r].cells.len().saturating_sub(1));
        let start = cell_start(&story, &range.table, r0, last_cell(r0, c0));
        let end = cell_end(&self.document, &story, &range.table, r1, last_cell(r1, c1));
        self.select(start, end);
        Ok(())
    }

    /// Merge the selected cells into one: across a row by widening the first cell (`w:gridSpan`), down
    /// rows by a vertical merge (`w:vMerge`), keeping the text of every cell merged in.
    pub fn merge_cells(&mut self) -> Result<(), String> {
        let range = self.selected_cells().ok_or("the caret is not in a table")?;
        if range.rows.0 == range.rows.1 && range.cells.0 == range.cells.1 {
            return Err("select more than one cell to merge them".into());
        }
        self.table_change(&range.table.clone(), move |table| {
            for r in range.rows.0..=range.rows.1 {
                let row = &mut table.rows[r];
                let last = range.cells.1.min(row.cells.len() - 1);
                let first = range.cells.0.min(last);
                let mut span = 0;
                let mut text: Vec<Block> = Vec::new();
                for cell in row.cells.drain(first + 1..=last).collect::<Vec<_>>() {
                    span += span_of(&cell);
                    if cell.blocks.iter().any(|b| b.as_paragraph().is_none_or(|p| !p.is_empty())) {
                        text.extend(cell.blocks);
                    }
                }
                let cell = &mut row.cells[first];
                let total = span_of(cell) + span;
                if total > 1 {
                    cell.props.put(El::new("w:gridSpan").with("w:val", total.to_string()));
                }
                cell.blocks.extend(text);
                if range.rows.0 != range.rows.1 {
                    if r == range.rows.0 {
                        cell.props.put(El::new("w:vMerge").with("w:val", "restart"));
                    } else {
                        cell.props.put(El::new("w:vMerge"));
                    }
                }
                crate::order::sort(&mut cell.props);
            }
            // The text of the rows merged down goes into the first one.
            if range.rows.0 != range.rows.1 {
                let first = range.cells.0;
                let mut moved: Vec<Block> = Vec::new();
                for r in range.rows.0 + 1..=range.rows.1 {
                    if let Some(cell) = table.rows[r].cells.get_mut(first) {
                        let blocks = std::mem::replace(
                            &mut cell.blocks,
                            vec![Block::Paragraph(Paragraph::new())],
                        );
                        if blocks.iter().any(|b| b.as_paragraph().is_none_or(|p| !p.is_empty())) {
                            moved.extend(blocks);
                        }
                    }
                }
                if let Some(cell) = table.rows[range.rows.0].cells.get_mut(first) {
                    cell.blocks.extend(moved);
                }
            }
            Ok(())
        })
    }

    /// Split the table above the caret's row into two tables with an empty paragraph between them, as
    /// Word's Split Table does.
    pub fn split_table(&mut self) -> Result<(), String> {
        let range = self.selected_cells().ok_or("the caret is not in a table")?;
        if range.rows.0 == 0 {
            return Err(
                "the caret is in the first row, so there is nothing above it to split off".into()
            );
        }
        let at_row = range.rows.0;
        self.change(Scope::Story, move |document, selection, _| {
            let story = selection.head.story.clone();
            let Some(blocks) = position::story_mut(document, &story) else {
                return selection.clone();
            };
            let Some((container, index)) = position::container_mut(blocks, &range.table) else {
                return selection.clone();
            };
            let Block::Table(table) = &mut container[index] else { return selection.clone() };
            let lower_rows = table.rows.split_off(at_row);
            let lower = Table {
                props: table.props.clone(),
                grid: table.grid.clone(),
                rows: lower_rows,
                extra: Vec::new(),
            };
            container.insert(index + 1, Block::Paragraph(Paragraph::new()));
            container.insert(index + 2, Block::Table(lower));
            let mut path = range.table.clone();
            *path.last_mut().expect("a path") += 1;
            Selection::caret(Pos { story, path, offset: 0 })
        });
        Ok(())
    }

    /// Turn the selected paragraphs into a table, one row a paragraph, cells split at `separator`
    /// (`tab`, `comma` or any one character), or one cell a paragraph in one column with `paragraph`.
    pub fn text_to_table(&mut self, separator: &str) -> Result<(), String> {
        let (from, to) = {
            let (a, b) = self.selection.ordered();
            (a.clone(), b.clone())
        };
        if from.path.len() != to.path.len()
            || from.path[..from.path.len() - 1] != to.path[..to.path.len() - 1]
        {
            return Err("select paragraphs next to each other to convert them".into());
        }
        let split = match separator {
            "tab" | "tabs" => Some('\t'),
            "comma" | "commas" => Some(','),
            "paragraph" | "paragraphs" => None,
            other => other.chars().next(),
        };
        let width = self
            .section_at_caret()
            .map_or(468.0, |s| crate::layout::section::Geometry::of(&s).text_width());
        let has_grid = crate::builtin::ensure_style(&mut self.document, "TableGrid").is_ok();
        self.change(Scope::Story, move |document, selection, _| {
            let story = selection.head.story.clone();
            let Some(blocks) = position::story_mut(document, &story) else {
                return selection.clone();
            };
            let Some((container, first)) = position::container_mut(blocks, &from.path) else {
                return selection.clone();
            };
            let last = *to.path.last().expect("a path") as usize;
            let taken: Vec<Paragraph> = container
                .drain(first..=last.min(container.len() - 1))
                .filter_map(|b| match b {
                    Block::Paragraph(p) => Some(p),
                    _ => None,
                })
                .collect();
            let cells: Vec<Vec<Paragraph>> = taken
                .into_iter()
                .map(|p| match split {
                    Some(c) => split_paragraph_at(&p, c),
                    None => vec![p],
                })
                .collect();
            let columns = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
            let mut table = crate::edit::new_table(
                cells.len(),
                columns,
                width,
                has_grid.then_some("TableGrid"),
            );
            for (row, pieces) in table.rows.iter_mut().zip(cells) {
                for (cell, mut piece) in row.cells.iter_mut().zip(pieces) {
                    piece.props.remove_children("w:numPr");
                    cell.blocks = vec![Block::Paragraph(piece)];
                }
            }
            container.insert(first, Block::Table(table));
            let mut path = from.path.clone();
            path.extend([0, 0, 0]);
            Selection::caret(Pos { story, path, offset: 0 })
        });
        Ok(())
    }

    /// Turn the caret's table into paragraphs, one a row, its cells joined by `separator` (`tab`,
    /// `comma`, `paragraph` or a character).
    pub fn table_to_text(&mut self, separator: &str) -> Result<(), String> {
        let range = self.selected_cells().ok_or("the caret is not in a table")?;
        let join = match separator {
            "tab" | "tabs" => Some("\t".to_owned()),
            "comma" | "commas" => Some(",".to_owned()),
            "paragraph" | "paragraphs" => None,
            other => Some(other.to_owned()),
        };
        self.change(Scope::Story, move |document, selection, _| {
            let story = selection.head.story.clone();
            let Some(blocks) = position::story_mut(document, &story) else {
                return selection.clone();
            };
            let Some((container, index)) = position::container_mut(blocks, &range.table) else {
                return selection.clone();
            };
            let Block::Table(table) = container.remove(index) else { return selection.clone() };
            let mut out: Vec<Block> = Vec::new();
            for row in table.rows {
                let paragraphs: Vec<Paragraph> = row
                    .cells
                    .into_iter()
                    .filter(|c| {
                        c.props.child("w:vMerge").is_none_or(|m| m.attr("w:val") == Some("restart"))
                    })
                    .flat_map(|c| {
                        c.blocks.into_iter().filter_map(|b| match b {
                            Block::Paragraph(p) => Some(p),
                            _ => None,
                        })
                    })
                    .collect();
                match &join {
                    None => out.extend(paragraphs.into_iter().map(Block::Paragraph)),
                    Some(between) => {
                        let mut joined = Paragraph::new();
                        for (k, p) in paragraphs.into_iter().enumerate() {
                            if k == 0 {
                                joined.props = p.props.clone();
                            } else if between == "\t" {
                                joined.runs.push(Run {
                                    content: Content::Tab,
                                    ..Run::text("", no_props())
                                });
                            } else {
                                joined.runs.push(Run::text(between.clone(), no_props()));
                            }
                            joined.runs.extend(p.runs);
                        }
                        joined.merge_runs();
                        out.push(Block::Paragraph(joined));
                    }
                }
            }
            let count = out.len();
            for (k, block) in out.into_iter().enumerate() {
                container.insert(index + k, block);
            }
            let _ = count;
            Selection::caret(Pos { story, path: range.table.clone(), offset: 0 })
        });
        Ok(())
    }

    /// Sort the caret's table's rows by one column (`column` from 0), comparing its text as `by` says,
    /// ascending or `descending`. A header row (`w:tblHeader`, or the first row when `header` is true)
    /// stays at the top.
    pub fn sort_table(
        &mut self,
        column: usize,
        by: SortBy,
        descending: bool,
        header: bool,
    ) -> Result<(), String> {
        let range = self.selected_cells().ok_or("the caret is not in a table")?;
        self.table_change(&range.table.clone(), move |table| {
            let keep = table
                .rows
                .iter()
                .take_while(|r| props::on_off(&r.props, "w:tblHeader") == Some(true))
                .count()
                .max(usize::from(header));
            if table
                .rows
                .iter()
                .any(|r| r.cells.iter().any(|c| c.props.child("w:vMerge").is_some()))
            {
                return Err("a table with cells merged down cannot be sorted".into());
            }
            let key = |row: &Row| -> String {
                row.cells
                    .get(column)
                    .map(|c| {
                        position::paragraphs(&c.blocks)
                            .iter()
                            .map(|(_, p)| crate::session::clean_text(&p.visible_text()))
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default()
            };
            let mut body: Vec<Row> = table.rows.split_off(keep);
            body.sort_by(|a, b| {
                let (x, y) = (key(a), key(b));
                let order = match by {
                    SortBy::Text => x.to_lowercase().cmp(&y.to_lowercase()),
                    SortBy::Number => number_of(&x)
                        .partial_cmp(&number_of(&y))
                        .unwrap_or(std::cmp::Ordering::Equal),
                    SortBy::Date => date_of(&x).cmp(&date_of(&y)),
                };
                if descending {
                    order.reverse()
                } else {
                    order
                }
            });
            table.rows.extend(body);
            Ok(())
        })
    }

    /// Give the selected rows (or every row) one height, the average of their heights now; or the
    /// selected columns (or every column) one width, their average.
    pub fn distribute(&mut self, rows: bool, heights: &[f32]) -> Result<(), String> {
        let range = self.selected_cells().ok_or("the caret is not in a table")?;
        let heights = heights.to_vec();
        self.table_change(&range.table.clone(), move |table| {
            let whole = range.rows.0 == range.rows.1 && range.cells.0 == range.cells.1;
            if rows {
                let (r0, r1) = if whole { (0, table.rows.len() - 1) } else { range.rows };
                let chosen = &heights[r0.min(heights.len())..(r1 + 1).min(heights.len())];
                let average = if chosen.is_empty() {
                    0.0
                } else {
                    chosen.iter().sum::<f32>() / chosen.len() as f32
                };
                if average <= 0.0 {
                    return Err("the rows' heights are not known yet".into());
                }
                for row in &mut table.rows[r0..=r1] {
                    row.props.put(
                        El::new("w:trHeight")
                            .with("w:val", props_twips(average))
                            .with("w:hRule", "atLeast"),
                    );
                    crate::order::sort(&mut row.props);
                }
            } else {
                let mut columns = table.columns();
                let (c0, c1) =
                    if whole { (0, columns.len().saturating_sub(1)) } else { range.cells };
                let c1 = c1.min(columns.len().saturating_sub(1));
                let total: i64 = columns[c0..=c1].iter().sum();
                let each = total / (c1 - c0 + 1) as i64;
                for w in &mut columns[c0..=c1] {
                    *w = each;
                }
                set_grid(table, &columns);
                for row in &mut table.rows {
                    let mut grid = 0;
                    for cell in &mut row.cells {
                        let span = span_of(cell);
                        let width: i64 = columns.iter().skip(grid).take(span).sum();
                        cell.props.put(
                            El::new("w:tcW").with("w:w", width.to_string()).with("w:type", "dxa"),
                        );
                        crate::order::sort(&mut cell.props);
                        grid += span;
                    }
                }
            }
            Ok(())
        })
    }

    /// One Table Properties setting for the caret's table, row or selected cells: `align`
    /// (left|center|right), `indent <pt>`, `width <pt>|<n>%|auto`, `margins <top> <left> <bottom> <right>`,
    /// `row-height <pt> [atLeast|exact]`, `repeat-header on|off`, `allow-break on|off`,
    /// `valign top|center|bottom`, `direction lrTb|tbRl|btLr`.
    pub fn table_property(&mut self, name: &str, values: &[String]) -> Result<(), String> {
        let range = self.selected_cells().ok_or("the caret is not in a table")?;
        let name = name.to_owned();
        let values = values.to_vec();
        let number = |i: usize| -> Result<f32, String> {
            values
                .get(i)
                .and_then(|v| v.trim_end_matches("pt").parse::<f32>().ok())
                .ok_or_else(|| format!("{name} takes a number of points"))
        };
        let first = number(0);
        self.table_change(&range.table.clone(), move |table| {
            let on =
                |v: Option<&String>| !matches!(v.map(String::as_str), Some("off" | "false" | "0"));
            match name.as_str() {
                "align" => {
                    let jc = match values.first().map(String::as_str) {
                        Some("center" | "centre") => "center",
                        Some("right") => "right",
                        _ => "left",
                    };
                    table.props.put(El::new("w:jc").with("w:val", jc));
                }
                "indent" => {
                    table.props.put(
                        El::new("w:tblInd").with("w:w", props_twips(first?)).with("w:type", "dxa"),
                    );
                }
                "width" => {
                    let el = match values.first().map(String::as_str) {
                        Some("auto") | None => {
                            El::new("w:tblW").with("w:w", "0").with("w:type", "auto")
                        }
                        Some(v) if v.ends_with('%') => {
                            let pct: f32 = v
                                .trim_end_matches('%')
                                .parse()
                                .map_err(|_| format!("{v} is not a percentage"))?;
                            El::new("w:tblW")
                                .with("w:w", ((pct * 50.0).round() as i64).to_string())
                                .with("w:type", "pct")
                        }
                        Some(_) => {
                            El::new("w:tblW").with("w:w", props_twips(first?)).with("w:type", "dxa")
                        }
                    };
                    table.props.put(el);
                }
                "margins" => {
                    let mut mar = El::new("w:tblCellMar");
                    for (i, side) in ["w:top", "w:left", "w:bottom", "w:right"].iter().enumerate() {
                        let v = values
                            .get(i)
                            .and_then(|v| v.parse::<f32>().ok())
                            .ok_or("margins takes four numbers of points: top left bottom right")?;
                        mar.children.push(Node::El(
                            El::new(*side).with("w:w", props_twips(v)).with("w:type", "dxa"),
                        ));
                    }
                    table.props.put(mar);
                }
                "row-height" => {
                    let rule = values.get(1).map_or("atLeast", String::as_str).to_owned();
                    let height = first?;
                    for row in &mut table.rows[range.rows.0..=range.rows.1] {
                        if height <= 0.0 {
                            row.props.remove_children("w:trHeight");
                        } else {
                            row.props.put(
                                El::new("w:trHeight")
                                    .with("w:val", props_twips(height))
                                    .with("w:hRule", rule.clone()),
                            );
                        }
                        crate::order::sort(&mut row.props);
                    }
                }
                "repeat-header" | "allow-break" => {
                    let flag = if name == "repeat-header" { "w:tblHeader" } else { "w:cantSplit" };
                    let mut value = on(values.first());
                    if name == "allow-break" {
                        value = !value;
                    }
                    for row in &mut table.rows[range.rows.0..=range.rows.1] {
                        props::set_on_off(&mut row.props, flag, value.then_some(true));
                        crate::order::sort(&mut row.props);
                    }
                }
                "valign" | "direction" => {
                    let (child, value) = if name == "valign" {
                        (
                            "w:vAlign",
                            match values.first().map(String::as_str) {
                                Some("center" | "centre" | "middle") => "center",
                                Some("bottom") => "bottom",
                                _ => "top",
                            },
                        )
                    } else {
                        (
                            "w:textDirection",
                            match values.first().map(String::as_str) {
                                Some("tbRl" | "down") => "tbRl",
                                Some("btLr" | "up") => "btLr",
                                _ => "lrTb",
                            },
                        )
                    };
                    for row in &mut table.rows[range.rows.0..=range.rows.1] {
                        let last = range.cells.1.min(row.cells.len().saturating_sub(1));
                        for cell in &mut row.cells[range.cells.0.min(last)..=last] {
                            cell.props.put(El::new(child).with("w:val", value));
                            crate::order::sort(&mut cell.props);
                        }
                    }
                }
                other => return Err(format!("there is no table property `{other}`")),
            }
            crate::order::sort(&mut table.props);
            Ok(())
        })
    }

    /// Apply a table style by id, made from Word's built in definitions when the document lacks it, and
    /// the Table Style Options (`w:tblLook`): header row, total row, first column, last column, banded
    /// rows, banded columns.
    pub fn apply_table_style(&mut self, id: &str, look: Option<[bool; 6]>) -> Result<(), String> {
        let range = self.selected_cells().ok_or("the caret is not in a table")?;
        crate::builtin::ensure_style(&mut self.document, id)?;
        let id = id.to_owned();
        self.table_change(&range.table.clone(), move |table| {
            table.props.put(El::new("w:tblStyle").with("w:val", id.clone()));
            if let Some(
                [first_row, last_row, first_column, last_column, banded_rows, banded_columns],
            ) = look
            {
                let flag = |b: bool| if b { "1" } else { "0" };
                table.props.put(
                    El::new("w:tblLook")
                        .with("w:val", "04A0")
                        .with("w:firstRow", flag(first_row))
                        .with("w:lastRow", flag(last_row))
                        .with("w:firstColumn", flag(first_column))
                        .with("w:lastColumn", flag(last_column))
                        .with("w:noHBand", flag(!banded_rows))
                        .with("w:noVBand", flag(!banded_columns)),
                );
            }
            crate::order::sort(&mut table.props);
            Ok(())
        })
    }

    /// Borders on the selected cells (or the whole table when one cell is selected): `all`, `none`,
    /// `outside`, `inside`, `top`, `bottom`, `left` or `right`, a single line `width` points wide.
    pub fn set_table_borders(
        &mut self,
        which: &str,
        width: f32,
        colour: Option<props::Rgb>,
    ) -> Result<(), String> {
        let range = self.selected_cells().ok_or("the caret is not in a table")?;
        let which = which.to_owned();
        let size = ((width * 8.0).round() as i64).clamp(2, 96).to_string();
        let colour = colour.map_or_else(|| "auto".to_owned(), |c| c.hex());
        self.table_change(&range.table.clone(), move |table| {
            let line = |name: &str, on: bool| {
                if on {
                    El::new(name).with("w:val", "single").with("w:sz", size.clone()).with("w:space", "0").with("w:color", colour.clone())
                } else {
                    El::new(name).with("w:val", "nil")
                }
            };
            let sides: [(&str, bool); 6] = match which.as_str() {
                "all" => [("w:top", true), ("w:left", true), ("w:bottom", true), ("w:right", true), ("w:insideH", true), ("w:insideV", true)],
                "none" => [("w:top", false), ("w:left", false), ("w:bottom", false), ("w:right", false), ("w:insideH", false), ("w:insideV", false)],
                "outside" => [("w:top", true), ("w:left", true), ("w:bottom", true), ("w:right", true), ("w:insideH", false), ("w:insideV", false)],
                "inside" => [("w:top", false), ("w:left", false), ("w:bottom", false), ("w:right", false), ("w:insideH", true), ("w:insideV", true)],
                one @ ("top" | "bottom" | "left" | "right") => {
                    let name = format!("w:{one}");
                    let mut borders = table.props.child("w:tblBorders").cloned().unwrap_or_else(|| El::new("w:tblBorders"));
                    borders.put(line(&name, true));
                    crate::order::sort(&mut borders);
                    table.props.put(borders);
                    crate::order::sort(&mut table.props);
                    return Ok(());
                }
                other => return Err(format!("borders takes all, none, outside, inside, top, bottom, left or right, not {other}")),
            };
            let mut borders = El::new("w:tblBorders");
            for (name, on) in sides {
                borders.children.push(Node::El(line(name, on)));
            }
            table.props.put(borders);
            // Cell borders of their own would hide the table's.
            for row in &mut table.rows {
                for cell in &mut row.cells {
                    cell.props.remove_children("w:tcBorders");
                }
            }
            crate::order::sort(&mut table.props);
            Ok(())
        })
    }

    /// Run a change on the table at `path` in the caret's story, undoing it when it fails.
    fn table_change(
        &mut self,
        path: &Path,
        change: impl FnOnce(&mut Table) -> Result<(), String>,
    ) -> Result<(), String> {
        let path = path.clone();
        let mut outcome = Ok(());
        self.change(Scope::Story, |document, selection, _| {
            let story = selection.head.story.clone();
            match position::story_mut(document, &story).and_then(|b| position::block_mut(b, &path))
            {
                Some(Block::Table(table)) => outcome = change(table),
                _ => outcome = Err("the table has gone".into()),
            }
            selection.clone()
        });
        if outcome.is_err() {
            self.undo();
        }
        outcome
    }
}

fn table_at<'a>(
    document: &'a Document,
    story: &StoryId,
    path: &[u32],
) -> Result<&'a Table, String> {
    match position::story(document, story).and_then(|b| position::block(b, path)) {
        Some(Block::Table(table)) => Ok(table),
        _ => Err("the table has gone".into()),
    }
}

fn cell_start(story: &StoryId, table: &[u32], row: usize, cell: usize) -> Pos {
    let mut path = table.to_vec();
    path.extend([row as u32, cell as u32, 0]);
    Pos { story: story.clone(), path, offset: 0 }
}

fn cell_end(document: &Document, story: &StoryId, table: &[u32], row: usize, cell: usize) -> Pos {
    let mut path = table.to_vec();
    path.extend([row as u32, cell as u32]);
    let blocks = position::story(document, story)
        .and_then(|b| position::block(b, &table[..]))
        .and_then(|b| match b {
            Block::Table(t) => {
                t.rows.get(row).and_then(|r| r.cells.get(cell)).map(|c| c.blocks.len())
            }
            _ => None,
        })
        .unwrap_or(1);
    path.push(blocks.saturating_sub(1) as u32);
    let length = position::paragraph(document, story, &path).map_or(0, Paragraph::len);
    Pos { story: story.clone(), path, offset: length }
}

fn span_of(cell: &Cell) -> usize {
    cell.props.val_of("w:gridSpan").and_then(|v| v.parse().ok()).unwrap_or(1)
}

fn set_grid(table: &mut Table, columns: &[i64]) {
    table.grid.children =
        columns.iter().map(|w| Node::El(El::new("w:gridCol").with("w:w", w.to_string()))).collect();
}

fn props_twips(points: f32) -> String {
    ((points * 20.0).round() as i64).to_string()
}

/// A paragraph cut into pieces at each `separator` character, each keeping the paragraph's properties
/// and the runs' formatting.
fn split_paragraph_at(paragraph: &Paragraph, separator: char) -> Vec<Paragraph> {
    let mut out = vec![Paragraph { props: paragraph.props.clone(), ..Paragraph::new() }];
    for run in &paragraph.runs {
        let is_tab = matches!(run.content, Content::Tab) && separator == '\t';
        if is_tab {
            out.push(Paragraph { props: paragraph.props.clone(), ..Paragraph::new() });
            continue;
        }
        match &run.content {
            Content::Text(text) if text.contains(separator) => {
                for (k, piece) in text.split(separator).enumerate() {
                    if k > 0 {
                        out.push(Paragraph { props: paragraph.props.clone(), ..Paragraph::new() });
                    }
                    if !piece.is_empty() {
                        let mut r = run.clone();
                        r.content = Content::Text(piece.to_owned());
                        out.last_mut().expect("a piece").runs.push(r);
                    }
                }
            }
            _ => out.last_mut().expect("a piece").runs.push(run.clone()),
        }
    }
    out
}

/// The number at the start of a cell's text, ignoring currency signs and thousands separators.
fn number_of(text: &str) -> f64 {
    let cleaned: String =
        text.trim().chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-')).collect();
    cleaned.parse().unwrap_or(f64::MIN)
}

/// A date written `m/d/yyyy`, `yyyy-mm-dd` or `d Month yyyy`, as a sortable `(year, month, day)`.
fn date_of(text: &str) -> (i32, u32, u32) {
    const MONTHS: [&str; 12] =
        ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let text = text.trim().to_lowercase();
    let numbers: Vec<i64> =
        text.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect();
    let month = MONTHS.iter().position(|m| text.contains(m)).map(|i| i as u32 + 1);
    match (month, numbers.as_slice()) {
        (Some(m), [d, y, ..]) => (*y as i32, m, *d as u32),
        (None, [y, m, d, ..]) if *y > 31 => (*y as i32, *m as u32, *d as u32),
        (None, [m, d, y, ..]) => (*y as i32, *m as u32, *d as u32),
        _ => (i32::MIN, 0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blank document holding only a three by two table, with the caret in its first cell.
    fn session_with_table() -> Session {
        let mut session = Session::new(crate::new::blank());
        session.insert_table(3, 2);
        // The table is the only block, so its cells are at [0, row, cell, 0].
        session.document.body.retain(|b| matches!(b, Block::Table(_)));
        for (r, row) in [["Name", "Age"], ["Zoe", "9"], ["Al", "40"]].iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                session.selection = Selection::caret(Pos::new(vec![0, r as u32, c as u32, 0], 0));
                session.type_text(text);
            }
        }
        session.selection = Selection::caret(Pos::new(vec![0, 0, 0, 0], 0));
        session
    }

    fn texts(session: &Session) -> Vec<Vec<String>> {
        let Some(Block::Table(t)) =
            session.document.body.iter().find(|b| matches!(b, Block::Table(_)))
        else {
            return Vec::new();
        };
        t.rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| {
                        position::paragraphs(&c.blocks)
                            .iter()
                            .map(|(_, p)| p.flat_text())
                            .collect::<Vec<_>>()
                            .join("|")
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn sorting_keeps_the_header_and_compares_numbers_as_numbers() {
        let mut session = session_with_table();
        session.sort_table(1, SortBy::Number, false, true).unwrap();
        assert_eq!(texts(&session)[1], ["Zoe", "9"]);
        session.sort_table(0, SortBy::Text, false, true).unwrap();
        assert_eq!(texts(&session)[1], ["Al", "40"]);
        assert_eq!(texts(&session)[0], ["Name", "Age"]);
        assert!(session.undo());
        assert_eq!(texts(&session)[1], ["Zoe", "9"]);
    }

    #[test]
    fn a_table_splits_and_converts_to_text_and_back() {
        let mut session = session_with_table();
        session.selection = Selection::caret(Pos::new(vec![0, 1, 0, 0], 0));
        session.split_table().unwrap();
        assert!(matches!(session.document.body[1], Block::Paragraph(_)));
        assert!(matches!(session.document.body[2], Block::Table(_)));
        session.selection = Selection::caret(Pos::new(vec![0, 0, 0, 0], 0));
        session.table_to_text("tab").unwrap();
        assert_eq!(
            position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap().flat_text(),
            "Name\tAge"
        );
        session.document.body.truncate(1);
        session.selection = Selection::caret(Pos::new(vec![0], 0));
        session.text_to_table("tab").unwrap();
        assert_eq!(texts(&session), [["Name", "Age"]]);
    }

    #[test]
    fn a_selection_of_cells_merges_into_one() {
        let mut session = session_with_table();
        session.select(Pos::new(vec![0, 1, 0, 0], 0), Pos::new(vec![0, 1, 1, 0], 0));
        let range = session.selected_cells().unwrap();
        assert_eq!((range.rows, range.cells), ((1, 1), (0, 1)));
        session.merge_cells().unwrap();
        assert_eq!(texts(&session)[1], ["Zoe|9"]);
        session.selection = Selection::caret(Pos::new(vec![0, 0, 0, 0], 0));
        session.select_in_table("column").unwrap();
        let range = session.selected_cells().unwrap();
        assert_eq!(range.rows, (0, 2));
    }

    #[test]
    fn properties_borders_and_distribution_write_words_elements() {
        let mut session = session_with_table();
        session.table_property("align", &["center".into()]).unwrap();
        session.table_property("width", &["60%".into()]).unwrap();
        session.table_property("repeat-header", &["on".into()]).unwrap();
        session.set_table_borders("outside", 1.0, None).unwrap();
        session.distribute(false, &[]).unwrap();
        let Some(Block::Table(t)) = session.document.body.first() else { panic!() };
        assert_eq!(t.props.val_of("w:jc"), Some("center"));
        assert_eq!(t.props.child("w:tblW").and_then(|w| w.attr("w:w")), Some("3000"));
        assert_eq!(props::on_off(&t.rows[0].props, "w:tblHeader"), Some(true));
        let borders = t.props.child("w:tblBorders").unwrap();
        assert_eq!(borders.child("w:insideH").and_then(|b| b.attr("w:val")), Some("nil"));
        let columns = t.columns();
        assert_eq!(columns[0], columns[1]);
        assert!(session.table_property("nonsense", &[]).is_err());
    }

    #[test]
    fn dates_and_numbers_read_as_word_sorts_them() {
        assert!(date_of("1/2/2020") < date_of("2020-01-03"));
        assert!(date_of("3 March 2021") > date_of("12/31/2020"));
        assert!(number_of("$1,200") > number_of("950"));
    }
}
