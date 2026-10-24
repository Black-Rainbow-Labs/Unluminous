//! `xl/workbook.bin`: the sheet list, the names, the books formulas refer to, and the calculation settings.

use super::records::{id, split, wide_string, Reader};
use crate::model::{CalcSettings, Visibility};
use crate::xls::ptg::{SupBook, Xti};

/// A sheet as the workbook lists it: its name, whether it shows, and the relationship that names its part.
#[derive(Debug, Clone)]
pub struct SheetRef {
    pub name: String,
    pub state: Visibility,
    pub rel_id: String,
}

/// A defined name as stored; its formula is decoded once every name and book is known.
#[derive(Debug, Clone, Default)]
pub struct RawName {
    pub flags: u32,
    /// The sheet it belongs to, or `None` for the workbook.
    pub scope: Option<usize>,
    pub name: String,
    pub rgce: Vec<u8>,
    pub rgcb: Vec<u8>,
}

impl RawName {
    pub fn hidden(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Whether a person made this name, as opposed to Excel making it to stand for something in a formula.
    pub fn is_a_name_a_person_made(&self) -> bool {
        let future_function = self.flags & 0x4000 != 0;
        let stands_in = self.name.starts_with("_xlpm.") || self.name.starts_with("_xlfn.");
        let builtin = self.name.starts_with("_xlnm.") || self.flags & 0x20 != 0;
        !(future_function || stands_in || (self.rgce.is_empty() && !builtin))
    }

    /// The name as a person writes it, without the prefix of a built in name.
    pub fn short_name(&self) -> String {
        self.name.strip_prefix("_xlnm.").unwrap_or(&self.name).to_owned()
    }

    /// The name as the `.xlsx` file writes it: a built in name carries the `_xlnm.` prefix.
    pub fn file_name(&self) -> String {
        if self.flags & 0x20 != 0 && !self.name.starts_with("_xlnm.") {
            format!("_xlnm.{}", self.name)
        } else {
            self.name.clone()
        }
    }
}

/// What the workbook part holds.
#[derive(Debug, Default)]
pub struct Book {
    pub date1904: bool,
    pub sheets: Vec<SheetRef>,
    pub names: Vec<RawName>,
    pub supbooks: Vec<SupBook>,
    pub xti: Vec<Xti>,
    pub active_sheet: usize,
    pub calc: CalcSettings,
}

/// Read the workbook part.
pub fn read(bytes: &[u8]) -> Book {
    let mut book = Book { calc: CalcSettings::default(), ..Default::default() };
    for record in split(bytes) {
        let mut r = Reader::new(record.data);
        match record.id {
            id::WB_PROP => book.date1904 = record.data.first().is_some_and(|b| b & 1 != 0),
            id::BOOK_VIEW => {
                book.active_sheet = record
                    .data
                    .get(24..28)
                    .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
            }
            id::BUNDLE_SH => {
                if let Some(sheet) = bundle_sheet(&mut r) {
                    book.sheets.push(sheet);
                }
            }
            id::CALC_PROP => calc_properties(&mut r, &mut book.calc),
            id::SUP_SELF | id::SUP_SAME => book.supbooks.push(SupBook::This),
            id::SUP_ADDIN => book.supbooks.push(SupBook::AddIn),
            id::SUP_BOOK_SRC => {
                let rel = wide_string(&mut r).unwrap_or_default();
                book.supbooks.push(SupBook::External { url: rel, sheets: Vec::new() })
            }
            id::SUP_TABS => {
                if let (Some(SupBook::External { sheets, .. }), Some(count)) =
                    (book.supbooks.last_mut(), r.u32())
                {
                    *sheets = (0..count).filter_map(|_| wide_string(&mut r)).collect();
                }
            }
            id::EXTERN_SHEET => book.xti = extern_sheet(&mut r),
            id::NAME => book.names.extend(name(&mut r)),
            _ => {}
        }
    }
    book
}

fn bundle_sheet(r: &mut Reader) -> Option<SheetRef> {
    let state = match r.u32()? {
        1 => Visibility::Hidden,
        2 => Visibility::VeryHidden,
        _ => Visibility::Visible,
    };
    r.skip(4)?;
    let rel_id = wide_string(r)?;
    let name = wide_string(r)?;
    Some(SheetRef { name, state, rel_id })
}

fn calc_properties(r: &mut Reader, calc: &mut CalcSettings) {
    let (Some(_), Some(auto), Some(_count), Some(_delta), Some(_threads), Some(flags)) =
        (r.u32(), r.u32(), r.u32(), r.f64(), r.i32(), r.u16())
    else {
        return;
    };
    calc.automatic = auto != 0;
    if flags & 0x04 != 0 {
        calc.attrs.push(("iterate".to_owned(), "1".to_owned()));
    }
    if flags & 0x08 == 0 {
        calc.attrs.push(("fullPrecision".to_owned(), "0".to_owned()));
    }
    if flags & 0x02 == 0 {
        calc.attrs.push(("refMode".to_owned(), "R1C1".to_owned()));
    }
}

fn extern_sheet(r: &mut Reader) -> Vec<Xti> {
    let count = r.u32().unwrap_or(0);
    (0..count)
        .filter_map(|_| {
            let book = r.u32()?;
            let first = r.i32()?;
            let last = r.i32()?;
            Some(Xti { book, first, last })
        })
        .collect()
}

fn name(r: &mut Reader) -> Option<RawName> {
    let flags = r.u32()?;
    r.skip(1)?;
    let scope = r.u32()?;
    let text = wide_string(r)?;
    let cce = r.u32()? as usize;
    let rgce = r.take(cce)?.to_vec();
    let cb = r.u32().unwrap_or(0) as usize;
    let rgcb = r.take(cb.min(r.rest().len())).map(<[u8]>::to_vec).unwrap_or_default();
    Some(RawName {
        flags,
        scope: (scope != 0xFFFF_FFFF).then_some(scope as usize),
        name: text,
        rgce,
        rgcb,
    })
}
