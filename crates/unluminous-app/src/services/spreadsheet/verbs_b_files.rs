//! The file format and read back verbs (`task-2360`, the `wiringb` lane): Save As for every format this
//! version writes (`.xlsx`, `.xlsm`, `.csv` in either encoding, `.txt`, `.ods`, `.html` and `.pdf`), the
//! message a workbook read from a legacy format gives when it is saved, `export`, and the read back of
//! links, charts and sparklines.

use std::path::PathBuf;

use serde_json::{json, Value as Json};
use unluminous_sheet::csv::Encoding;
use unluminous_sheet::model::LinkTarget;
use unluminous_sheet::{FileKind, SaveOptions};

use super::verbs_a::Out;
use super::{print, Book, Spreadsheet};
use crate::services::plugin_ui::{Answer, Request};

fn said(text: impl Into<String>) -> Answer {
    Answer::said(text)
}

/// Run a file verb, or answer `None` for one this file does not own.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "export" => export(sheet, args),
        "save-as-xlsx" => save_as_xlsx(sheet),
        "links" => links(sheet),
        "charts" => charts(sheet),
        "sparklines" => sparklines(sheet),
        _ => return None,
    })
}

/// Verbs that existed before and now take more words: `save` and `save-as`.
pub fn first(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    match verb {
        "save" => legacy_save(sheet),
        "save-as" => Some(save_as(sheet, args)),
        _ => None,
    }
}

/// A file name read against the project folder when it is relative.
pub fn resolve(sheet: &Spreadsheet, file: &str) -> PathBuf {
    let path = std::path::Path::new(file);
    match (path.is_absolute(), &sheet.project) {
        (false, Some(project)) => project.join(path),
        _ => path.to_path_buf(),
    }
}

/// What a workbook read from a format this version does not write says when it is saved.
fn legacy_message(book: &Book) -> String {
    let stem = book.path.file_stem().and_then(|s| s.to_str()).unwrap_or("workbook");
    format!(
        "{} was read from a format this version does not write. Save it as {stem}.xlsx with `save-as {stem}.xlsx`.",
        book.name()
    )
}

/// `save` on a workbook read from `.xls`, `.xlsb`, `.xml` or `.json` says it must be saved as `.xlsx`.
fn legacy_save(sheet: &mut Spreadsheet) -> Option<Out> {
    let book = sheet.current_book().ok()?;
    (!book.kind.can_save()).then(|| Err(legacy_message(book)))
}

/// `save-as-xlsx`: save the workbook as `.xlsx` beside the file it was read from.
fn save_as_xlsx(sheet: &mut Spreadsheet) -> Out {
    let book = sheet.current_book()?;
    let target = book.path.with_extension("xlsx");
    let name = target.display().to_string();
    save_as(sheet, &[name])
}

/// The encoding a `save-as` word asks for.
fn encoding_of(args: &[String]) -> Result<Option<Encoding>, String> {
    let Some(word) = args.get(1) else { return Ok(None) };
    Ok(Some(match word.to_ascii_lowercase().as_str() {
        "utf8" | "utf-8" | "utf8-bom" => Encoding::Utf8Bom,
        "utf8-no-bom" | "plain-utf8" => Encoding::Utf8,
        "plain" | "ansi" | "windows-1252" => Encoding::Windows1252,
        "unicode" | "utf16" | "utf-16" => Encoding::Utf16Le,
        other => {
            return Err(format!("an encoding is utf8, utf8-no-bom, plain or unicode; got `{other}`"))
        }
    }))
}

/// `save-as <file> [utf8|plain|unicode]`: the format is the file's extension. `.html` and `.pdf` write a
/// copy and leave the workbook where it is.
fn save_as(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let file = args.first().ok_or("save-as needs a file")?;
    let full = resolve(sheet, file);
    let ext = full.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if matches!(ext.as_str(), "pdf" | "html" | "htm") {
        return export_to(sheet, &full);
    }
    let kind = FileKind::of(&full)?;
    if !kind.can_save() {
        return Err(format!("This version reads .{ext} files but does not write them. Save as .xlsx, .xlsm, .csv, .tsv, .txt, .ods, .html or .pdf."));
    }
    let encoding = encoding_of(args)?;
    let key = super::key_of_path(&full, sheet.project.as_deref());
    let book = sheet.current_book()?;
    // An encoding belongs to the format it was chosen for, so a file saved in another format starts again.
    let same = book.kind == kind;
    book.path = full.clone();
    book.kind = kind;
    book.key = key.clone();
    book.encoding = encoding.or(if same { book.encoding } else { None });
    let message = book.save()?;
    sheet.current = Some(key);
    sheet.ask(Request::OpenFile(full));
    Ok(said(message))
}

/// `export pdf|html|ods|txt|csv|xlsx <file>`: write a copy in another format and leave the workbook as it is.
fn export(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let kind = args.first().ok_or("export takes a format and a file")?.to_ascii_lowercase();
    let book = sheet.current_book()?;
    let default = book.path.with_extension(&kind);
    let target = match args.get(1) {
        Some(file) => resolve(sheet, file),
        None => default,
    };
    export_to(sheet, &target)
}

/// Write the workbook to a file in the format its extension names, without making it the workbook's file.
fn export_to(sheet: &mut Spreadsheet, target: &std::path::Path) -> Out {
    let ext = target.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let book = sheet.current_book()?;
    let (bytes, detail) = if ext == "pdf" {
        let (bytes, layout) = print::pdf_bytes(book);
        (bytes, json!({ "pages": layout.page_count() }))
    } else {
        let kind = FileKind::of(target)?;
        book.session.book.active_sheet = book.view.sheet;
        let options = SaveOptions { encoding: book.encoding };
        (unluminous_sheet::save_with(&book.session.book, kind, &options)?, Json::Null)
    };
    if let Some(folder) = target.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    crate::services::store::write_a_source_file(target, &bytes)
        .map_err(|e| format!("{} could not be written: {e}", target.display()))?;
    Ok(said(format!("Exported {}", target.display())).with(detail))
}

// ----------------------------------------------------------------------------------------------- read back

/// `links`: the hyperlinks of every sheet.
fn links(sheet: &mut Spreadsheet) -> Out {
    let book = sheet.current_book()?;
    let mut all = Vec::new();
    for s in &book.session.book.sheets {
        for link in &s.links {
            let (kind, target) = match &link.target {
                LinkTarget::External(t) => ("external", t.clone()),
                LinkTarget::Location(t) => ("place", t.clone()),
            };
            all.push(json!({ "sheet": s.name, "range": link.range.a1(), "kind": kind, "target": target, "display": link.display, "tooltip": link.tooltip }));
        }
    }
    Ok(said(format!("{} links", all.len())).with(json!(all)))
}

/// `charts`: every chart of the workbook with its kind, title, series and place.
fn charts(sheet: &mut Spreadsheet) -> Out {
    let book = sheet.current_book()?;
    let mut all = Vec::new();
    for s in &book.session.book.sheets {
        for (i, c) in s.charts.iter().enumerate() {
            all.push(json!({
                "sheet": s.name,
                "number": i + 1,
                "kind": c.kind.name(),
                "title": c.title,
                "series": c.series.iter().map(|x| x.values.clone()).collect::<Vec<_>>(),
                "from": c.from.0.a1(),
                "to": c.to.0.a1(),
            }));
        }
    }
    Ok(said(format!("{} charts", all.len())).with(json!(all)))
}

/// `sparklines`: the sparklines the sheets hold, read out of the extension they are kept in.
fn sparklines(sheet: &mut Spreadsheet) -> Out {
    let book = sheet.current_book()?;
    let mut all = Vec::new();
    for s in &book.session.book.sheets {
        for (_, xml) in s.raw.elements.iter().filter(|(name, _)| name == "extLst") {
            all.extend(sparklines_in(&s.name, xml));
        }
    }
    Ok(said(format!("{} sparklines", all.len())).with(json!(all)))
}

/// The sparklines in the XML of a worksheet's extension list.
pub fn sparklines_in(sheet: &str, xml: &str) -> Vec<Json> {
    let mut out = Vec::new();
    for group in xml.split("<x14:sparklineGroup").skip(1) {
        let head = group.split('>').next().unwrap_or("");
        let kind = attribute(head, "type").unwrap_or_else(|| "line".to_owned());
        for one in group.split("<x14:sparkline>").skip(1) {
            let data = between(one, "<xm:f>", "</xm:f>");
            let place = between(one, "<xm:sqref>", "</xm:sqref>");
            out.push(json!({ "sheet": sheet, "kind": kind, "source": data, "location": place }));
        }
    }
    out
}

fn attribute(head: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let at = head.find(&key)? + key.len();
    Some(head[at..].split('"').next()?.to_owned())
}

fn between(text: &str, open: &str, close: &str) -> Option<String> {
    let start = text.find(open)? + open.len();
    Some(text[start..].split(close).next()?.to_owned())
}
