//! Formulas > Create from Selection: names made from the labels along the edges of the chosen cells, the
//! way Excel's `Range.CreateNames` makes them.

use unluminous_sheet::coord::{column_name, CellRef, Range};
use unluminous_sheet::edit::{check_defined_name, Session};

/// Make names from the labels in the top row, left column, bottom row and right column of `range`
/// (`sides` in that order). Each label names the cells beside it, inside the range and past the other
/// label rows and columns. Answers the names made.
pub fn from_selection(
    session: &mut Session,
    sheet: usize,
    range: Range,
    sides: [bool; 4],
) -> Result<Vec<String>, String> {
    let [top, left, bottom, right] = sides;
    if !(top || left || bottom || right) {
        return Err(
            "Choose at least one of top row, left column, bottom row or right column.".to_owned()
        );
    }
    // The cells the names cover: the range without the label rows and columns.
    let first_row = range.first.row + u32::from(top);
    let last_row = range.last.row.saturating_sub(u32::from(bottom));
    let first_col = range.first.col + u32::from(left);
    let last_col = range.last.col.saturating_sub(u32::from(right));
    if first_row > last_row || first_col > last_col {
        return Err("Cannot create names from this selection.".to_owned());
    }
    let mut wanted: Vec<(String, Range)> = Vec::new();
    for (on, label_row) in [(top, range.first.row), (bottom, range.last.row)] {
        if !on {
            continue;
        }
        for col in first_col..=last_col {
            let covered = Range::new(CellRef::new(first_row, col), CellRef::new(last_row, col));
            wanted.push((label(session, sheet, CellRef::new(label_row, col)), covered));
        }
    }
    for (on, label_col) in [(left, range.first.col), (right, range.last.col)] {
        if !on {
            continue;
        }
        for row in first_row..=last_row {
            let covered = Range::new(CellRef::new(row, first_col), CellRef::new(row, last_col));
            wanted.push((label(session, sheet, CellRef::new(row, label_col)), covered));
        }
    }
    let sheet_name = unluminous_sheet::formula::print::sheet_name(&session.book.sheets[sheet].name);
    let mut made = Vec::new();
    for (text, covered) in wanted {
        let Some(name) = legal_name(&text) else { continue };
        session.set_name(&name, &format!("{sheet_name}!{}", absolute(covered)), None)?;
        made.push(name);
    }
    Ok(made)
}

/// The shown text of a label cell.
fn label(session: &Session, sheet: usize, at: CellRef) -> String {
    let book = &session.book;
    let s = &book.sheets[sheet];
    let style = s.cell(at).map_or(0, |c| c.style);
    let code = book.styles.format_code(style);
    unluminous_sheet::numfmt::Format::parse(&code).render(&s.value_at(at), book.date1904, 255).text
}

/// A label made into a name as Excel makes one: spaces and characters a name cannot hold become `_`, and a
/// name that would start with a digit or read as a cell reference gets `_` in front. An empty label makes
/// no name.
pub fn legal_name(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut name: String = text
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?') { c } else { '_' })
        .collect();
    let starts_badly =
        name.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '?');
    if starts_badly || check_defined_name(&name).is_err() {
        name.insert(0, '_');
    }
    check_defined_name(&name).ok().map(|_| name)
}

fn absolute(r: Range) -> String {
    let a = |c: CellRef| format!("${}${}", column_name(c.col), c.row + 1);
    if r.first == r.last {
        a(r.first)
    } else {
        format!("{}:{}", a(r.first), a(r.last))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unluminous_sheet::Workbook;

    #[test]
    fn labels_along_the_top_and_the_left_name_the_cells_beside_them() {
        let mut s = Session::new(Workbook::new(), 0.0);
        for (at, text) in
            [("B1", "Jan"), ("C1", "Feb"), ("A2", "North"), ("A3", "South"), ("B2", "1")]
        {
            s.set_input(0, CellRef::parse(at).unwrap(), text).unwrap();
        }
        let range = Range::parse("A1:C3").unwrap();
        let made = from_selection(&mut s, 0, range, [true, true, false, false]).unwrap();
        assert_eq!(made, vec!["Jan", "Feb", "North", "South"]);
        let jan = s.book.names.iter().find(|n| n.name == "Jan").unwrap();
        assert_eq!(jan.formula, "Sheet1!$B$2:$B$3");
        let north = s.book.names.iter().find(|n| n.name == "North").unwrap();
        assert_eq!(north.formula, "Sheet1!$B$2:$C$2");
    }

    #[test]
    fn a_label_becomes_a_legal_name() {
        assert_eq!(legal_name("Sales 2026").as_deref(), Some("Sales_2026"));
        assert_eq!(legal_name("2026").as_deref(), Some("_2026"));
        assert_eq!(legal_name("A1").as_deref(), Some("_A1"));
        assert_eq!(legal_name("   "), None);
    }
}
