//! The first part of the workbook stream: everything about the workbook rather than one sheet.

use super::id;
use super::ptg::{SupBook, Xti};
use super::records::{shared_strings, Reader, Record};
use super::styles::{parse_font, parse_xf, Palette, Raw};
use crate::model::{CalcSettings, Visibility};

/// One sheet as the sheet list names it.
#[derive(Debug, Clone)]
pub struct SheetEntry {
    pub name: String,
    /// Where the sheet's records begin in the stream.
    pub offset: usize,
    pub state: Visibility,
    /// 0 worksheet, 1 macro sheet, 2 chart, 6 Visual Basic module.
    pub kind: u8,
}

/// A defined name as stored: its formula is decoded once every name and book is known.
#[derive(Debug, Clone, Default)]
pub struct RawName {
    pub flags: u16,
    /// The sheet it belongs to, counted from 1, or 0 for the workbook.
    pub scope: u16,
    pub name: String,
    /// The built in name this stands for, when it is one.
    pub builtin: Option<u8>,
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
        let parameter = self.name.starts_with("_xlpm.") || self.name.starts_with("_xlfn.");
        let undefined = self.builtin.is_none() && self.rgce.is_empty();
        !(future_function || parameter || undefined)
    }

    /// The name as a person writes it, which for a built in name is the plain word.
    pub fn short_name(&self) -> String {
        match self.builtin {
            Some(code) => builtin_name(code).to_owned(),
            None => self.name.clone(),
        }
    }

    /// The name as the `.xlsx` file writes it: a built in name carries the `_xlnm.` prefix.
    pub fn file_name(&self) -> String {
        match self.builtin {
            Some(code) => format!("_xlnm.{}", builtin_name(code)),
            None => self.name.clone(),
        }
    }
}

/// The built in names, by the code the name record holds.
pub fn builtin_name(code: u8) -> &'static str {
    match code {
        0x00 => "Consolidate_Area",
        0x01 => "Auto_Open",
        0x02 => "Auto_Close",
        0x03 => "Extract",
        0x04 => "Database",
        0x05 => "Criteria",
        0x06 => "Print_Area",
        0x07 => "Print_Titles",
        0x08 => "Recorder",
        0x09 => "Data_Form",
        0x0A => "Auto_Activate",
        0x0B => "Auto_Deactivate",
        0x0C => "Sheet_Title",
        0x0D => "_FilterDatabase",
        _ => "Unknown",
    }
}

/// What the globals part holds.
#[derive(Debug, Default)]
pub struct Globals {
    pub date1904: bool,
    pub sheets: Vec<SheetEntry>,
    pub strings: Vec<String>,
    pub styles: Raw,
    pub supbooks: Vec<SupBook>,
    pub extern_names: Vec<Vec<String>>,
    pub xti: Vec<Xti>,
    pub names: Vec<RawName>,
    pub active_sheet: usize,
    pub calc: CalcSettings,
    /// The workbook structure and windows are protected.
    pub lock_structure: bool,
    pub lock_windows: bool,
}

/// Read the globals from the records, which start at the workbook's `BOF`.
pub fn read(records: &[Record]) -> Result<Globals, String> {
    let mut g = Globals { calc: CalcSettings::default(), ..Default::default() };
    let mut palette = Palette::default();
    let mut font_records: Vec<&Record> = Vec::new();
    let mut depth = 0;
    for record in records {
        match record.id {
            id::BOF => {
                depth += 1;
            }
            id::EOF => {
                depth -= 1;
                if depth <= 0 {
                    break;
                }
            }
            id::DATEMODE => g.date1904 = record.data.first().is_some_and(|b| b & 1 != 0),
            id::BOUNDSHEET => {
                if let Some(entry) = bound_sheet(record) {
                    g.sheets.push(entry);
                }
            }
            id::SST => g.strings = shared_strings(record),
            id::FONT => font_records.push(record),
            id::FORMAT => {
                let mut r = Reader::new(&record.data);
                if let (Some(index), Some(code)) = (r.u16(), r.string()) {
                    g.styles.formats.insert(index as u32, code);
                }
            }
            id::PALETTE => palette = Palette::from_record(record),
            id::XF => {
                if let Some(xf) = parse_xf(&record.data) {
                    g.styles.xfs.push(xf);
                }
            }
            id::SUPBOOK => {
                g.supbooks.push(sup_book(record));
                g.extern_names.push(Vec::new());
            }
            id::EXTERNNAME => {
                let mut r = Reader::new(&record.data);
                let name = r.u16().and_then(|_| r.skip(4)).and_then(|_| r.short_string());
                if let (Some(name), Some(list)) = (name, g.extern_names.last_mut()) {
                    list.push(name);
                }
            }
            id::EXTERNSHEET => g.xti = extern_sheet(record),
            id::NAME => {
                if let Some(name) = raw_name(record) {
                    g.names.push(name);
                }
            }
            id::WINDOW1 => {
                g.active_sheet = record
                    .data
                    .get(10..12)
                    .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize);
            }
            id::CALCMODE => {
                let mode = record.data.get(..2).map_or(1, |b| i16::from_le_bytes([b[0], b[1]]));
                g.calc.automatic = mode != 0;
            }
            id::ITERATION => {
                if record.data.first().is_some_and(|b| b & 1 != 0) {
                    g.calc.attrs.push(("iterate".to_owned(), "1".to_owned()));
                }
            }
            id::CALCCOUNT => {
                if let Some(b) = record.data.get(..2) {
                    let n = u16::from_le_bytes([b[0], b[1]]);
                    if n != 100 {
                        g.calc.attrs.push(("iterateCount".to_owned(), n.to_string()));
                    }
                }
            }
            id::DELTA => {
                if let Some(v) = Reader::new(&record.data).f64() {
                    if (v - 0.001).abs() > 1e-12 {
                        g.calc.attrs.push(("iterateDelta".to_owned(), v.to_string()));
                    }
                }
            }
            id::PRECISION => {
                if record.data.first() == Some(&0) {
                    g.calc.attrs.push(("fullPrecision".to_owned(), "0".to_owned()));
                }
            }
            id::PROTECT => g.lock_structure = record.data.first().is_some_and(|b| b & 1 != 0),
            id::WINDOWPROTECT => g.lock_windows = record.data.first().is_some_and(|b| b & 1 != 0),
            _ => {}
        }
    }
    g.styles.palette = palette.clone();
    g.styles.fonts =
        font_records.iter().map(|r| parse_font(&r.data, &palette).unwrap_or_default()).collect();
    Ok(g)
}

fn bound_sheet(record: &Record) -> Option<SheetEntry> {
    let mut r = Reader::new(&record.data);
    let offset = r.u32()? as usize;
    let state = match r.u8()? & 3 {
        1 => Visibility::Hidden,
        2 => Visibility::VeryHidden,
        _ => Visibility::Visible,
    };
    let kind = r.u8()?;
    let name = r.short_string()?;
    Some(SheetEntry { name, offset, state, kind })
}

fn sup_book(record: &Record) -> SupBook {
    let mut r = Reader::new(&record.data);
    let (Some(count), Some(cch)) = (r.u16(), r.u16()) else { return SupBook::This };
    match cch {
        0x0401 => SupBook::This,
        0x3A01 => SupBook::AddIn,
        cch => {
            let url = r.string_of(cch as usize).unwrap_or_default();
            let sheets = (0..count).filter_map(|_| r.string()).collect();
            SupBook::External { url, sheets }
        }
    }
}

fn extern_sheet(record: &Record) -> Vec<Xti> {
    let mut r = Reader::new(&record.data);
    let count = r.u16().unwrap_or(0);
    (0..count)
        .filter_map(|_| {
            let book = r.u16()? as u32;
            let first = r.i16()? as i32;
            let last = r.i16()? as i32;
            Some(Xti { book, first, last })
        })
        .collect()
}

fn raw_name(record: &Record) -> Option<RawName> {
    let mut r = Reader::new(&record.data);
    let flags = r.u16()?;
    r.skip(1)?;
    let cch = r.u8()? as usize;
    let cce = r.u16()? as usize;
    r.skip(2)?;
    let scope = r.u16()?;
    let custom_menu = r.u8()? as usize;
    let description = r.u8()? as usize;
    let help = r.u8()? as usize;
    let status = r.u8()? as usize;
    let builtin = flags & 0x20 != 0;
    let name = if cch == 0 { String::new() } else { r.string_of(cch)? };
    let rgce = r.take(cce)?.to_vec();
    let rgcb = r.rest().to_vec();
    let _ = (custom_menu, description, help, status);
    let code = if builtin { name.chars().next().map(|c| c as u32 as u8) } else { None };
    Some(RawName { flags, scope, name, builtin: code, rgce, rgcb })
}
