//! Table parts (`xl/tables/tableN.bin`) of an `.xlsb` file.
//!
//! A binary table part is read into the same two things an `.xlsx` table part gives: the model's table
//! with a table part written as XML (so saving as `.xlsx` writes a table Excel reads), and the name and
//! columns a structured reference in a formula needs.

use super::records::{id, split, wide_string, Reader};
use crate::coord::{CellRef, Range};
use crate::model::Table;
use crate::xls::ptg::TableInfo;
use crate::xlsx::{escape, MAIN_NS};

const BEGIN_LIST: u16 = 343;
const BEGIN_LIST_COL: u16 = 347;
const TABLE_STYLE: u16 = 513;

/// Read one table part. `sheet` is the sheet the table is on and `number` counts the tables of the
/// workbook from 1, which names the part it is written to.
pub fn read(bytes: &[u8], sheet: usize, number: usize) -> Option<(TableInfo, Table)> {
    let mut range = None;
    let mut list_id = 0;
    let mut headers = 1;
    let mut totals = 0;
    let mut name = String::new();
    let mut columns = Vec::new();
    let mut style = None;
    for record in split(bytes) {
        let mut r = Reader::new(record.data);
        match record.id {
            BEGIN_LIST => {
                let (r1, r2, c1, c2) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
                range = Some(Range::new(CellRef::new(r1, c1), CellRef::new(r2, c2)));
                r.skip(4)?;
                list_id = r.u32()?;
                headers = r.u32()?;
                totals = r.u32()?;
                r.skip(4 + 24 + 4)?;
                name = wide_string(&mut r)?;
            }
            BEGIN_LIST_COL => {
                r.skip(28)?;
                columns.push(wide_string(&mut r)?);
            }
            TABLE_STYLE => {
                let flags = r.u16()?;
                style = Some((flags, wide_string(&mut r)?));
            }
            id::BEGIN_AFILTER => {}
            _ => {}
        }
    }
    let range = range?;
    let part = format!("xl/tables/table{number}.xml");
    let xml = table_xml(list_id, &name, range, &columns, headers > 0, totals > 0, style.as_ref());
    let info =
        TableInfo { id: list_id, name: name.clone(), columns: columns.clone(), sheet, range };
    let table = Table {
        sheet,
        name,
        range,
        columns,
        header_row: headers > 0,
        totals_row: totals > 0,
        part,
        xml,
    };
    Some((info, table))
}

/// The XML of a table part for a table.
fn table_xml(
    number: u32,
    name: &str,
    range: Range,
    columns: &[String],
    header_row: bool,
    totals_row: bool,
    style: Option<&(u16, String)>,
) -> String {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<table xmlns=\"{MAIN_NS}\" id=\"{number}\" name=\"{n}\" displayName=\"{n}\" ref=\"{}\"",
        range.a1(),
        n = escape(name)
    );
    if !header_row {
        xml.push_str(" headerRowCount=\"0\"");
    }
    if totals_row {
        xml.push_str(" totalsRowCount=\"1\"");
    }
    xml.push('>');
    if header_row {
        let last = if totals_row { range.last.row.saturating_sub(1) } else { range.last.row };
        let body = Range::new(range.first, CellRef::new(last, range.last.col));
        xml.push_str(&format!("<autoFilter ref=\"{}\"/>", body.a1()));
    }
    xml.push_str(&format!("<tableColumns count=\"{}\">", columns.len()));
    for (i, column) in columns.iter().enumerate() {
        xml.push_str(&format!("<tableColumn id=\"{}\" name=\"{}\"/>", i + 1, escape(column)));
    }
    xml.push_str("</tableColumns>");
    if let Some((flags, style_name)) = style {
        xml.push_str(&format!(
            "<tableStyleInfo name=\"{}\" showFirstColumn=\"{}\" showLastColumn=\"{}\" showRowStripes=\"{}\" showColumnStripes=\"{}\"/>",
            escape(style_name),
            flags & 1,
            (flags >> 1) & 1,
            (flags >> 2) & 1,
            (flags >> 3) & 1
        ));
    }
    xml.push_str("</table>");
    xml
}
