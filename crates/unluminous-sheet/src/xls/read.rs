//! Reading an `.xls` file: the compound file, the workbook stream, and a [`Workbook`] out of it.

use std::collections::HashMap;
use std::sync::Arc;

use super::globals::{self, Globals};
use super::ptg::{decode, Decoded, Dialect, Env, Origin};
use super::records::{split, Record};
use super::{crypto, id, sheet, styles};
use crate::cfb::{Compound, EntryKind, ENCRYPTED};
use crate::model::{DefinedName, Sheet, Workbook};

/// The streams that hold the workbook, newest format first.
const WORKBOOK_STREAMS: [&str; 2] = ["Workbook", "Book"];

/// Read a workbook from the bytes of an `.xls` file.
pub fn read(bytes: &[u8]) -> Result<Workbook, String> {
    let compound = Compound::parse(bytes)?;
    if compound.has("EncryptionInfo") || compound.has("EncryptedPackage") {
        return Err(ENCRYPTED.to_owned());
    }
    let mut stream = WORKBOOK_STREAMS
        .iter()
        .find_map(|name| compound.stream(name))
        .ok_or("the file has no workbook stream, so it is not an Excel workbook")??;
    let mut records = split(&stream);
    check_version(&records)?;
    if let Some(filepass) = records.iter().find(|r| r.id == id::FILEPASS) {
        crypto::decrypt_with_default_password(&mut stream, filepass.offset)?;
        records = split(&stream);
    }
    let globals = globals::read(&records)?;
    let mut book = build(&records, &globals)?;
    keep_other_streams(&compound, &mut book);
    Ok(book)
}

/// Only BIFF8, the format of Excel 97 and every later version that saves `.xls`, is read.
fn check_version(records: &[Record]) -> Result<(), String> {
    let first = records
        .first()
        .filter(|r| r.id == id::BOF)
        .ok_or("the workbook stream does not start like one")?;
    let version = first.data.get(..2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    if version != 0x0600 {
        return Err("this is an Excel 5.0 or 95 workbook, which this version does not read. Excel can save it as .xlsx".to_owned());
    }
    Ok(())
}

/// The model out of the records.
fn build(records: &[Record], g: &Globals) -> Result<Workbook, String> {
    let (styles, xf_map) = styles::build(&g.styles);
    let mut book =
        Workbook { styles, date1904: g.date1904, calc: g.calc.clone(), ..Default::default() };
    let sheet_names: Vec<String> = g.sheets.iter().map(|s| s.name.clone()).collect();
    let name_texts: Vec<String> = g.names.iter().map(|n| n.short_name()).collect();
    let env = Env {
        sheet: None,
        tables: &[],
        sheets: &sheet_names,
        xti: &g.xti,
        supbooks: &g.supbooks,
        names: &name_texts,
        extern_names: &g.extern_names,
    };
    book.names = defined_names(g, &env);
    let by_offset: HashMap<usize, usize> =
        records.iter().enumerate().map(|(i, r)| (r.offset, i)).collect();
    let dxfs = sheet::DxfPool::default();
    let ctx = sheet::Context {
        globals: g,
        env: &env,
        xf_map: &xf_map,
        palette: &g.styles.palette,
        dxfs: &dxfs,
    };
    for (index, entry) in g.sheets.iter().enumerate() {
        let mut sheet = Sheet::new(entry.name.clone());
        sheet.state = entry.state;
        if entry.kind == 0 {
            if let Some(&start) = by_offset.get(&entry.offset) {
                sheet::read(&records[start..], &mut sheet, &ctx, index);
            }
        }
        book.sheets.push(sheet);
    }
    if book.sheets.is_empty() {
        return Err("the workbook has no sheets".to_owned());
    }
    book.styles.dxfs = dxfs.into_inner();
    book.active_sheet = g.active_sheet.min(book.sheets.len() - 1);
    protect_workbook(g, &mut book);
    Ok(book)
}

/// The defined names, each with its formula written out. A name Excel made for a function added after
/// the file format was fixed (`_xlfn.XLOOKUP`), for a parameter of a `LET` or `LAMBDA`, or for a name a
/// formula uses without defining, stands in the formula and is not a name a person made.
fn defined_names(g: &Globals, env: &Env) -> Vec<DefinedName> {
    g.names
        .iter()
        .filter(|raw| raw.is_a_name_a_person_made())
        .map(|raw| {
            let formula = match decode(Dialect::Biff8, &raw.rgce, &raw.rgcb, env, Origin::default())
            {
                Ok(Decoded::Text(text)) => text,
                _ => "#REF!".to_owned(),
            };
            DefinedName {
                name: raw.file_name(),
                scope: (raw.scope > 0).then(|| raw.scope as usize - 1),
                formula,
                hidden: raw.hidden(),
                attrs: Vec::new(),
            }
        })
        .collect()
}

/// Structure and window protection are kept as the element an `.xlsx` file would hold them in.
fn protect_workbook(g: &Globals, book: &mut Workbook) {
    if g.lock_structure || g.lock_windows {
        let mut xml = String::from("<workbookProtection");
        if g.lock_structure {
            xml.push_str(" lockStructure=\"1\"");
        }
        if g.lock_windows {
            xml.push_str(" lockWindows=\"1\"");
        }
        xml.push_str("/>");
        book.raw.push(("workbookProtection".to_owned(), xml));
    }
}

/// Every stream other than the workbook is kept as bytes: the macro project, the document properties,
/// and the pictures. They are not written to an `.xlsx` file.
fn keep_other_streams(compound: &Compound, book: &mut Workbook) {
    for entry in compound.entries() {
        if entry.kind != EntryKind::Stream || WORKBOOK_STREAMS.contains(&entry.path.as_str()) {
            continue;
        }
        if let Ok(bytes) = compound.read_entry(entry) {
            book.kept.legacy.push((entry.path.clone(), Arc::new(bytes)));
        }
    }
}
