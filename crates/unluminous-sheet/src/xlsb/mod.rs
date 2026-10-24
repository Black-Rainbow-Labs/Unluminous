//! The `.xlsb` file: SpreadsheetML as binary records in a zip. Reading only.
//!
//! The package is the same as an `.xlsx` package: relationships name the parts, and the parts are
//! `workbook.bin`, one `sheetN.bin` for each sheet, `styles.bin` and `sharedStrings.bin`. Their contents
//! are records instead of XML. What is still XML in an `.xlsb` file (the theme, drawings and charts) is
//! read by the code that reads those in an `.xlsx` file. Formulas are the same token lists as in `.xls`,
//! with wider references, and [`crate::xls::ptg`] writes them out.
//!
//! What this version reads no further (pivot tables, macros, table parts) is kept as bytes in
//! [`crate::model::Kept::legacy`].

pub mod book;
pub mod records;
pub mod sheet;
pub mod styles;
pub mod tables;

use std::collections::HashMap;
use std::sync::Arc;

use crate::model::{Note, Relationship, Sheet, Table, Workbook};
use crate::styles::{Styles, DEFAULT_THEME};
use crate::xls::ptg::{decode, Decoded, Dialect, Env, Origin, SupBook, TableInfo};
use crate::xlsx::{
    parse_rels, rels_path, resolve, REL_COMMENTS, REL_DRAWING, REL_HYPERLINK, REL_OFFICE_DOCUMENT,
    REL_SHARED_STRINGS, REL_STYLES, REL_TABLE, REL_THEME, REL_WORKSHEET,
};
use records::{id, many, split, wide_string, Reader};

/// Read a workbook from the bytes of an `.xlsb` file.
pub fn read(bytes: &[u8]) -> Result<Workbook, String> {
    if crate::cfb::is_compound(bytes) {
        return Err(crate::cfb::ENCRYPTED.to_owned());
    }
    let entries = crate::zip::read(bytes)?;
    let files: HashMap<&str, &[u8]> =
        entries.iter().map(|e| (e.name.as_str(), e.data.as_slice())).collect();
    let text = |path: &str| -> Option<String> {
        files.get(path).map(|b| String::from_utf8_lossy(b).into_owned())
    };
    let root_rels = parse_rels(&text("_rels/.rels").unwrap_or_default());
    let workbook_part = root_rels
        .iter()
        .find(|r| r.kind == REL_OFFICE_DOCUMENT)
        .map(|r| resolve("", &r.target))
        .ok_or("the file names no workbook")?;
    let workbook_bytes = files.get(workbook_part.as_str()).ok_or("the workbook part is missing")?;
    let rels = parse_rels(&text(&rels_path(&workbook_part)).unwrap_or_default());
    let target_of = |kind: &str| {
        rels.iter().find(|r| r.kind == kind).map(|r| resolve(&workbook_part, &r.target))
    };
    let mut book = Workbook { styles: Styles::default_workbook(), ..Default::default() };
    if let Some(theme) = target_of(REL_THEME).and_then(|p| text(&p)) {
        crate::xlsx::read::read_theme(&theme, &mut book.styles);
    }
    if book.styles.theme.is_empty() {
        book.styles.theme = DEFAULT_THEME.to_vec();
    }
    if let Some(data) = target_of(REL_STYLES).and_then(|p| files.get(p.as_str()).copied()) {
        styles::read(data, &mut book.styles);
    }
    let strings = target_of(REL_SHARED_STRINGS)
        .and_then(|p| files.get(p.as_str()).copied())
        .map(shared_strings)
        .unwrap_or_default();
    let info = book::read(workbook_bytes);
    let mut parsed_extra: Vec<String> = Vec::new();
    book.date1904 = info.date1904;
    book.calc = info.calc.clone();
    let sheet_names: Vec<String> = info.sheets.iter().map(|s| s.name.clone()).collect();
    let name_texts: Vec<String> = info.names.iter().map(|n| n.short_name()).collect();
    let supbooks =
        external_books(&info.supbooks, &rels, &workbook_part, &files, &text, &mut parsed_extra);
    let extern_names: Vec<Vec<String>> = supbooks.iter().map(|_| Vec::new()).collect();
    let (table_infos, tables) =
        read_tables(&info.sheets, &rels, &workbook_part, &files, &text, &mut parsed_extra);
    let env = Env {
        sheet: None,
        tables: &table_infos,
        sheets: &sheet_names,
        xti: &info.xti,
        supbooks: &supbooks,
        names: &name_texts,
        extern_names: &extern_names,
    };
    book.names = defined_names(&info, &env);
    book.tables = tables;
    let mut parsed: Vec<String> = vec![workbook_part.clone()];
    parsed.append(&mut parsed_extra);
    for (index, entry) in info.sheets.iter().enumerate() {
        let mut sheet = Sheet::new(entry.name.clone());
        sheet.state = entry.state;
        if let Some(rel) =
            rels.iter().find(|r| r.id == entry.rel_id).filter(|r| r.kind == REL_WORKSHEET)
        {
            let part = resolve(&workbook_part, &rel.target);
            parsed.push(part.clone());
            if let Some(data) = files.get(part.as_str()) {
                let sheet_env = Env { sheet: Some(index), ..env.clone() };
                read_sheet(
                    data,
                    &part,
                    &mut sheet,
                    &strings,
                    &sheet_env,
                    &text,
                    &files,
                    &mut parsed,
                );
            }
        }
        book.sheets.push(sheet);
    }
    if book.sheets.is_empty() {
        return Err("the workbook has no sheets".to_owned());
    }
    book.active_sheet = info.active_sheet.min(book.sheets.len() - 1);
    for p in [target_of(REL_STYLES), target_of(REL_SHARED_STRINGS)].into_iter().flatten() {
        parsed.push(p);
    }
    for entry in &entries {
        if !parsed.contains(&entry.name) {
            book.kept.legacy.push((entry.name.clone(), Arc::new(entry.data.clone())));
        }
    }
    Ok(book)
}

/// The shared strings: each item is a flag byte and the text, then formatting runs this reads past.
fn shared_strings(bytes: &[u8]) -> Vec<String> {
    split(bytes)
        .into_iter()
        .filter(|r| r.id == id::SST_ITEM)
        .map(|r| {
            let mut reader = Reader::new(r.data);
            let _ = reader.u8();
            wide_string(&mut reader).unwrap_or_default()
        })
        .collect()
}

/// The defined names, each with its formula written out, leaving out the names Excel makes to stand
/// for something in a formula.
fn defined_names(info: &book::Book, env: &Env) -> Vec<crate::model::DefinedName> {
    info.names
        .iter()
        .filter(|raw| raw.is_a_name_a_person_made())
        .map(|raw| {
            let formula =
                match decode(Dialect::Biff12, &raw.rgce, &raw.rgcb, env, Origin::default()) {
                    Ok(Decoded::Text(text)) => text,
                    _ => "#REF!".to_owned(),
                };
            crate::model::DefinedName {
                name: raw.file_name(),
                scope: raw.scope,
                formula,
                hidden: raw.hidden(),
                attrs: Vec::new(),
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn read_sheet(
    data: &[u8],
    part: &str,
    sheet: &mut Sheet,
    strings: &[String],
    env: &Env,
    text: &dyn Fn(&str) -> Option<String>,
    files: &HashMap<&str, &[u8]>,
    parsed: &mut Vec<String>,
) {
    let rels = parse_rels(&text(&rels_path(part)).unwrap_or_default());
    let links: Vec<(String, String)> = rels
        .iter()
        .filter(|r| r.kind == REL_HYPERLINK)
        .map(|r| (r.id.clone(), r.target.clone()))
        .collect();
    let ctx = sheet::Context { strings, env, links: &links };
    sheet::read(data, sheet, &ctx);
    parsed.push(rels_path(part));
    for rel in &rels {
        if rel.kind == REL_COMMENTS {
            let comments_part = resolve(part, &rel.target);
            if let Some(bytes) = files.get(comments_part.as_str()) {
                sheet.notes = notes(bytes);
                parsed.push(comments_part);
            }
        }
        if rel.kind == REL_DRAWING {
            sheet.charts.extend(crate::xlsx::chart::read_charts(part, &rel.target, text));
            sheet.pictures.extend(crate::xlsx::chart::read_pictures(part, &rel.target, text));
        }
    }
}

/// The notes in a comments part: the authors, then each comment's cell, author and text.
fn notes(bytes: &[u8]) -> std::collections::BTreeMap<(u32, u32), Note> {
    let mut authors = Vec::new();
    let mut out = std::collections::BTreeMap::new();
    let mut current: Option<(u32, u32, String)> = None;
    for record in split(bytes) {
        let mut r = Reader::new(record.data);
        match record.id {
            id::COMMENT_AUTHOR => authors.extend(wide_string(&mut r)),
            id::BEGIN_COMMENT => {
                if let (Some(author), Some(row), Some(_), Some(col)) =
                    (r.u32(), r.u32(), r.u32(), r.u32())
                {
                    current =
                        Some((row, col, authors.get(author as usize).cloned().unwrap_or_default()));
                }
            }
            id::COMMENT_TEXT => {
                if let Some((row, col, author)) = current.take() {
                    let _ = r.u8();
                    let text = wide_string(&mut r).unwrap_or_default();
                    out.insert((row, col), Note { author, text });
                }
            }
            _ => {}
        }
    }
    out
}

/// The books formulas refer to, with the sheet names and the address of the external ones, which are in
/// a part of their own.
fn external_books(
    supbooks: &[SupBook],
    rels: &[Relationship],
    workbook_part: &str,
    files: &HashMap<&str, &[u8]>,
    text: &dyn Fn(&str) -> Option<String>,
    parsed: &mut Vec<String>,
) -> Vec<SupBook> {
    supbooks
        .iter()
        .map(|book| {
            let SupBook::External { url: rel_id, .. } = book else { return book.clone() };
            let Some(rel) = rels.iter().find(|r| r.id == *rel_id) else { return book.clone() };
            let part = resolve(workbook_part, &rel.target);
            let sheets = files
                .get(part.as_str())
                .map(|data| {
                    split(data)
                        .into_iter()
                        .filter(|r| r.id == id::SUP_TABS)
                        .flat_map(|r| {
                            let mut reader = Reader::new(r.data);
                            let count = reader.u32().unwrap_or(0);
                            many(count, &mut reader, wide_string)
                        })
                        .collect()
                })
                .unwrap_or_default();
            let url = parse_rels(&text(&rels_path(&part)).unwrap_or_default())
                .into_iter()
                .find(|r| r.external)
                .map(|r| r.target)
                .unwrap_or_default();
            parsed.push(part.clone());
            parsed.push(rels_path(&part));
            SupBook::External { url, sheets }
        })
        .collect()
}

/// The tables of every sheet: what a formula needs to name them, and the model's table for each.
fn read_tables(
    sheets: &[book::SheetRef],
    rels: &[Relationship],
    workbook_part: &str,
    files: &HashMap<&str, &[u8]>,
    text: &dyn Fn(&str) -> Option<String>,
    parsed: &mut Vec<String>,
) -> (Vec<TableInfo>, Vec<Table>) {
    let mut infos = Vec::new();
    let mut tables = Vec::new();
    for (index, entry) in sheets.iter().enumerate() {
        let Some(rel) =
            rels.iter().find(|r| r.id == entry.rel_id).filter(|r| r.kind == REL_WORKSHEET)
        else {
            continue;
        };
        let part = resolve(workbook_part, &rel.target);
        for table_rel in parse_rels(&text(&rels_path(&part)).unwrap_or_default()) {
            if table_rel.kind != REL_TABLE {
                continue;
            }
            let table_part = resolve(&part, &table_rel.target);
            let Some(data) = files.get(table_part.as_str()) else { continue };
            if let Some((info, table)) = tables::read(data, index, tables.len() + 1) {
                infos.push(info);
                tables.push(table);
                parsed.push(table_part);
            }
        }
    }
    (infos, tables)
}
