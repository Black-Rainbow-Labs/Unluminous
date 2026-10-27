//! The page setup, print and PDF verbs (`task-2360`, the `wiringb` lane): the Page Layout level's margins,
//! orientation, paper size, print area, breaks, print titles, scaling, header and footer, gridlines and
//! headings; the print preview; Print; and the read back of all of it. Each verb calls the engine's
//! `Session` method of the same name and the print preview shows the pages `print::layout_of` lays out.

use serde_json::{json, Value as Json};
use unluminous_sheet::coord::{column_name, CellRef, Range};
use unluminous_sheet::edit::Session;
use unluminous_sheet::pages::paper::PAPERS;
use unluminous_sheet::pages::{
    HeaderFooterPart, Margins, Orientation, PageOrder, PageSetup, PrintComments, PrintErrors,
};

use super::commands::{parse_range, take_range};
use super::verbs_a::{flag, on_off, Out};
use super::view_b::Preview;
use super::{print, Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

fn said(text: impl Into<String>) -> Answer {
    Answer::said(text)
}

/// Run a page verb, or answer `None` for a verb this file does not own.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "page-setup" => page_setup(sheet, args),
        "print-area" => print_area(sheet, args),
        "print-titles" => print_titles(sheet, args),
        "page-break" => page_break(sheet, args),
        "print-preview" => print_preview(sheet, args),
        "print" => print_now(sheet, args),
        _ => return None,
    })
}

/// A change to the sheet showing, noted so the grid and the preview draw it again.
fn change(
    sheet: &mut Spreadsheet,
    f: impl FnOnce(&mut Session, usize) -> Result<(), String>,
) -> Result<(), String> {
    let book = sheet.current_book()?;
    let index = book.view.sheet;
    f(&mut book.session, index)?;
    book.changed();
    book.view.axes = None;
    Ok(())
}

/// `page-setup`, which reads the setup back, or `page-setup <what> <words>`, which changes one thing.
fn page_setup(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let Some(what) = args.first().map(|w| w.to_ascii_lowercase()) else {
        return read_back(sheet);
    };
    let rest = &args[1..];
    let message = match what.as_str() {
        "orientation" => orientation(sheet, rest)?,
        "paper" | "paper-size" | "size" => paper(sheet, rest)?,
        "margins" => margins(sheet, rest)?,
        "scale" => scale(sheet, rest)?,
        "fit" | "fit-to" => fit(sheet, rest)?,
        "gridlines" => switch(sheet, rest, "Print gridlines", |s, i, on| s.set_print_gridlines(i, on))?,
        "headings" => switch(sheet, rest, "Print headings", |s, i, on| s.set_print_headings(i, on))?,
        "order" | "page-order" => order(sheet, rest)?,
        "center" | "centre" => center(sheet, rest)?,
        "first-page" | "first-page-number" => first_page(sheet, rest)?,
        "header" | "footer" => header_footer(sheet, &what, rest)?,
        "different-first" | "different-odd-even" | "scale-with-document" | "align-with-margins" => {
            header_option(sheet, &what, rest)?
        }
        "comments" | "notes" => comments(sheet, rest)?,
        "errors" => errors(sheet, rest)?,
        "quality" | "black-and-white" | "draft" => quality(sheet, &what, rest)?,
        other => {
            return Err(format!(
                "page-setup takes orientation, paper, margins, scale, fit, gridlines, headings, order, center, first-page, header, footer, comments, errors or quality; got `{other}`"
            ))
        }
    };
    Ok(said(message))
}

fn orientation(sheet: &mut Spreadsheet, args: &[String]) -> Result<String, String> {
    let how = match args.first().map(String::as_str) {
        Some("portrait") => Orientation::Portrait,
        Some("landscape") => Orientation::Landscape,
        _ => return Err("orientation takes portrait or landscape".to_owned()),
    };
    change(sheet, |s, i| s.set_orientation(i, how))?;
    Ok(format!("Orientation {}", args[0]))
}

/// The paper a word names: its Excel number, or its name with the spaces left out.
fn paper_code(word: &str) -> Option<u32> {
    if let Ok(code) = word.parse::<u32>() {
        return Some(code);
    }
    let wanted = word.to_ascii_lowercase().replace([' ', '-', '_'], "");
    PAPERS
        .iter()
        .find(|p| p.name.to_ascii_lowercase().replace([' ', '-', '_'], "") == wanted)
        .map(|p| p.code)
}

fn paper(sheet: &mut Spreadsheet, args: &[String]) -> Result<String, String> {
    let word = args.join(" ");
    let code = paper_code(&word).ok_or_else(|| {
        format!("there is no paper size `{word}`; try Letter, Legal, A3, A4 or A5")
    })?;
    change(sheet, |s, i| s.set_paper_size(i, code))?;
    Ok(format!("Paper size {word}"))
}

fn inches(word: &str) -> Result<f64, String> {
    word.parse::<f64>().map_err(|_| format!("`{word}` is not a margin in inches"))
}

fn margins(sheet: &mut Spreadsheet, args: &[String]) -> Result<String, String> {
    let m = match args.first().map(String::as_str) {
        Some("normal") => Margins::default(),
        Some("narrow") => Margins::narrow(),
        Some("wide") => Margins::wide(),
        Some(_) if args.len() >= 4 => {
            let mut m = Margins {
                left: inches(&args[0])?,
                right: inches(&args[1])?,
                top: inches(&args[2])?,
                bottom: inches(&args[3])?,
                ..Margins::default()
            };
            if let Some(header) = args.get(4) {
                m.header = inches(header)?;
            }
            if let Some(footer) = args.get(5) {
                m.footer = inches(footer)?;
            }
            m
        }
        _ => return Err(
            "margins take normal, narrow, wide, or left right top bottom [header footer] in inches"
                .to_owned(),
        ),
    };
    change(sheet, |s, i| s.set_margins(i, m))?;
    Ok("Margins set".to_owned())
}

fn scale(sheet: &mut Spreadsheet, args: &[String]) -> Result<String, String> {
    let percent: u32 = args
        .first()
        .and_then(|w| w.trim_end_matches('%').parse().ok())
        .ok_or("scale takes a percentage from 10 to 400")?;
    change(sheet, |s, i| s.set_scale(i, percent))?;
    Ok(format!("Scale {percent}%"))
}

fn fit(sheet: &mut Spreadsheet, args: &[String]) -> Result<String, String> {
    let count = |n: usize| -> Result<u32, String> {
        args.get(n)
            .map(|w| w.parse().map_err(|_| format!("`{w}` is not a number of pages")))
            .unwrap_or(Ok(1))
    };
    let (wide, tall) = (count(0)?, count(1)?);
    change(sheet, |s, i| s.set_fit_to_pages(i, wide, tall))?;
    Ok(format!("Fit to {wide} wide by {tall} tall"))
}

fn switch(
    sheet: &mut Spreadsheet,
    args: &[String],
    name: &str,
    f: impl FnOnce(&mut Session, usize, bool) -> Result<(), String>,
) -> Result<String, String> {
    let on = on_off(args.first().map(String::as_str))?;
    change(sheet, |s, i| f(s, i, on))?;
    Ok(format!("{name} {}", if on { "on" } else { "off" }))
}

fn order(sheet: &mut Spreadsheet, args: &[String]) -> Result<String, String> {
    let how = match args.first().map(String::as_str) {
        Some("down") | Some("down-then-over") => PageOrder::DownThenOver,
        Some("over") | Some("over-then-down") => PageOrder::OverThenDown,
        _ => return Err("order takes down (down, then over) or over (over, then down)".to_owned()),
    };
    change(sheet, |s, i| s.set_page_order(i, how))?;
    Ok("Page order set".to_owned())
}

fn center(sheet: &mut Spreadsheet, args: &[String]) -> Result<String, String> {
    let (h, v) = match args.first().map(String::as_str) {
        Some("horizontal") | Some("horizontally") => (true, false),
        Some("vertical") | Some("vertically") => (false, true),
        Some("both") | Some("on") => (true, true),
        Some("off") | Some("none") => (false, false),
        _ => return Err("center takes horizontal, vertical, both or off".to_owned()),
    };
    change(sheet, |s, i| s.set_centered(i, h, v))?;
    Ok("Centering set".to_owned())
}

fn first_page(sheet: &mut Spreadsheet, args: &[String]) -> Result<String, String> {
    let number = match args.first().map(String::as_str) {
        Some("auto") | None => None,
        Some(w) => Some(w.parse::<i32>().map_err(|_| format!("`{w}` is not a page number"))?),
    };
    change(sheet, |s, i| s.set_first_page_number(i, number))?;
    Ok("First page number set".to_owned())
}

/// The three sections of a header string, left, centre and right.
pub fn split_sections(raw: &str) -> [String; 3] {
    let mut out = [String::new(), String::new(), String::new()];
    let mut at = 1;
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&' {
            match chars.get(i + 1) {
                Some('L') => {
                    at = 0;
                    i += 2;
                    continue;
                }
                Some('C') => {
                    at = 1;
                    i += 2;
                    continue;
                }
                Some('R') => {
                    at = 2;
                    i += 2;
                    continue;
                }
                Some(next) => {
                    out[at].push('&');
                    out[at].push(*next);
                    i += 2;
                    continue;
                }
                None => {}
            }
        }
        out[at].push(chars[i]);
        i += 1;
    }
    out
}

/// A header string from its three sections.
pub fn join_sections(parts: &[String; 3]) -> String {
    let mut out = String::new();
    for (code, text) in ["&L", "&C", "&R"].iter().zip(parts) {
        if !text.is_empty() {
            out.push_str(code);
            out.push_str(text);
        }
    }
    out
}

/// `page-setup header|footer [first|even] [left|center|right] <text>`: with a section named, only that
/// section changes; without one the words are the whole string with its `&` codes.
fn header_footer(sheet: &mut Spreadsheet, what: &str, args: &[String]) -> Result<String, String> {
    let (which, rest) = match args.first().map(String::as_str) {
        Some("first") => ("first", &args[1..]),
        Some("even") => ("even", &args[1..]),
        _ => ("odd", args),
    };
    let part = match (what, which) {
        ("header", "first") => HeaderFooterPart::FirstHeader,
        ("header", "even") => HeaderFooterPart::EvenHeader,
        ("header", _) => HeaderFooterPart::OddHeader,
        (_, "first") => HeaderFooterPart::FirstFooter,
        (_, "even") => HeaderFooterPart::EvenFooter,
        _ => HeaderFooterPart::OddFooter,
    };
    let (section, words) = match rest.first().map(String::as_str) {
        Some("left") => (Some(0), &rest[1..]),
        Some("center") | Some("centre") => (Some(1), &rest[1..]),
        Some("right") => (Some(2), &rest[1..]),
        _ => (None, rest),
    };
    let text = words.join(" ");
    let book = sheet.current_book()?;
    let old = book.session.book.sheets[book.view.sheet].page.header_footer.text(part).to_owned();
    let new = match section {
        Some(n) => {
            let mut parts = split_sections(&old);
            parts[n] = text;
            join_sections(&parts)
        }
        None if text.contains('&') => text,
        None => join_sections(&[String::new(), text, String::new()]),
    };
    change(sheet, |s, i| s.set_header_footer(i, part, &new))?;
    Ok(format!("{what} set"))
}

fn header_option(sheet: &mut Spreadsheet, what: &str, args: &[String]) -> Result<String, String> {
    let on = on_off(args.first().map(String::as_str))?;
    let book = sheet.current_book()?;
    let hf = &book.session.book.sheets[book.view.sheet].page.header_footer;
    let (mut first, mut even, mut scale, mut align) =
        (hf.different_first, hf.different_odd_even, hf.scale_with_doc, hf.align_with_margins);
    match what {
        "different-first" => first = on,
        "different-odd-even" => even = on,
        "scale-with-document" => scale = on,
        _ => align = on,
    }
    change(sheet, |s, i| s.set_header_footer_options(i, first, even, scale, align))?;
    Ok(format!("{what} {}", if on { "on" } else { "off" }))
}

fn comments(sheet: &mut Spreadsheet, args: &[String]) -> Result<String, String> {
    let how = match args.first().map(String::as_str) {
        Some("none") => PrintComments::None,
        Some("end") | Some("at-end") => PrintComments::AtEnd,
        Some("displayed") | Some("as-displayed") => PrintComments::AsDisplayed,
        _ => return Err("comments take none, end or displayed".to_owned()),
    };
    change(sheet, |s, i| s.set_print_comments(i, how))?;
    Ok("Print comments set".to_owned())
}

fn errors(sheet: &mut Spreadsheet, args: &[String]) -> Result<String, String> {
    let how = match args.first().map(String::as_str) {
        Some("displayed") => PrintErrors::Displayed,
        Some("blank") => PrintErrors::Blank,
        Some("dashes") | Some("dash") => PrintErrors::Dash,
        Some("na") | Some("n/a") => PrintErrors::NotAvailable,
        _ => return Err("errors take displayed, blank, dashes or na".to_owned()),
    };
    change(sheet, |s, i| s.set_print_errors(i, how))?;
    Ok("Print errors set".to_owned())
}

fn quality(sheet: &mut Spreadsheet, what: &str, args: &[String]) -> Result<String, String> {
    let (key, rest) = if what == "quality" {
        (args.first().map(String::as_str).unwrap_or(""), args.get(1..).unwrap_or(&[]))
    } else {
        (what, args)
    };
    let on = on_off(rest.first().map(String::as_str))?;
    let book = sheet.current_book()?;
    let page = &book.session.book.sheets[book.view.sheet].page;
    let (mut bw, mut draft) = (page.black_and_white, page.draft);
    match key {
        "black-and-white" => bw = on,
        "draft" => draft = on,
        _ => return Err("quality takes black-and-white or draft, then on or off".to_owned()),
    }
    change(sheet, |s, i| s.set_print_quality(i, bw, draft))?;
    Ok("Print quality set".to_owned())
}

// ------------------------------------------------------------------------------ area, titles and breaks

/// `print-area set|clear|add [range]`.
fn print_area(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let how = args.first().map(String::as_str).unwrap_or("set");
    if how == "list" || args.is_empty() {
        return Ok(said("Print area").with(json!(print::area_text(book))));
    }
    let rest = if matches!(how, "set" | "add" | "clear") { &args[1..] } else { args };
    take_range(book, rest);
    let range = book.view.range();
    let message = match how {
        "clear" => {
            change(sheet, |s, i| s.clear_print_area(i))?;
            "Print area cleared".to_owned()
        }
        "add" => {
            change(sheet, |s, i| s.add_to_print_area(i, range))?;
            format!("Added {} to the print area", range.a1())
        }
        _ => {
            change(sheet, |s, i| s.set_print_area(i, &[range]))?;
            format!("Print area {}", range.a1())
        }
    };
    Ok(said(message))
}

/// `print-titles rows 1:2 | cols A:B | clear`, or the titles read back with no words.
fn print_titles(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let current = unluminous_sheet::pages::print_titles(&book.session.book, book.view.sheet);
    if args.is_empty() {
        return Ok(said("Print titles").with(titles_json(current)));
    }
    let (mut rows, mut cols) = (current.rows, current.cols);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "clear" => (rows, cols) = (None, None),
            "rows" | "columns" | "cols" => {
                let word =
                    args.get(i + 1).ok_or("rows and columns take a range such as 1:2 or A:B")?;
                let range =
                    Range::parse(word).ok_or_else(|| format!("`{word}` is not rows or columns"))?;
                if args[i] == "rows" {
                    rows = Some((range.first.row, range.last.row));
                } else {
                    cols = Some((range.first.col, range.last.col));
                }
                i += 1;
            }
            other => {
                return Err(format!("print-titles takes rows, columns or clear; got `{other}`"))
            }
        }
        i += 1;
    }
    change(sheet, |s, i| s.set_print_titles(i, rows, cols))?;
    Ok(said("Print titles set"))
}

fn titles_json(t: unluminous_sheet::pages::PrintTitles) -> Json {
    json!({
        "rows": t.rows.map(|(a, b)| format!("{}:{}", a + 1, b + 1)),
        "columns": t.cols.map(|(a, b)| format!("{}:{}", column_name(a), column_name(b))),
    })
}

/// `page-break insert|remove [cell]` or `page-break reset`.
fn page_break(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let how = args.first().map(String::as_str).unwrap_or("list");
    if how == "list" {
        let layout = print::layout_of(book);
        return Ok(said("Page breaks")
            .with(json!({ "rows": layout.row_break_rows(), "columns": layout.col_break_cols() })));
    }
    let rest = &args[1..];
    let at = match rest.first() {
        Some(w) => parse_range(book, w)
            .map(|(_, r)| r.first)
            .ok_or_else(|| format!("`{w}` is not a cell"))?,
        None => book.view.cursor,
    };
    match how {
        "insert" => change(sheet, |s, i| s.insert_page_break(i, at))?,
        "remove" => change(sheet, |s, i| s.remove_page_break(i, at))?,
        "reset" => change(sheet, |s, i| s.reset_page_breaks(i))?,
        other => return Err(format!("page-break takes insert, remove or reset; got `{other}`")),
    }
    Ok(said(format!("Page break {how}")))
}

// ------------------------------------------------------------------------------------- preview and print

/// `print-preview [open|close|next|previous|page <n>]`: the pages as the paginator lays them out.
fn print_preview(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let layout = print::layout_of(book);
    let total = layout.page_count();
    let how = args.first().map(String::as_str).unwrap_or("open");
    let state = book.view.lane_b.preview.get_or_insert_with(Preview::default);
    state.pages = total;
    match how {
        "close" => {
            book.view.lane_b.preview = None;
            return Ok(said("Print preview closed"));
        }
        "next" => state.page = (state.page + 1).min(total.saturating_sub(1)),
        "previous" => state.page = state.page.saturating_sub(1),
        "first" => state.page = 0,
        "page" => {
            let n: usize =
                args.get(1).and_then(|w| w.parse().ok()).ok_or("page takes a page number")?;
            state.page = n.saturating_sub(1).min(total.saturating_sub(1));
        }
        "open" | "show" => {}
        other => {
            return Err(format!(
                "print-preview takes open, close, next, previous or page; got `{other}`"
            ))
        }
    }
    let page = state.page;
    let mut measurer = print::Measurer::new();
    let pages = print::pdf_pages(book, &layout, &mut measurer);
    let texts = print::page_texts(&pages);
    let summary: Vec<Json> = layout
        .pages
        .iter()
        .map(|p| json!({ "number": p.number, "range": p.body.a1(), "header": plain_of(&p.header), "footer": plain_of(&p.footer) }))
        .collect();
    Ok(said(format!("Page {} of {}", page + 1, total)).with(json!({
        "page": page + 1,
        "pages": total,
        "scale": layout.scale,
        "paper": { "width": layout.paper.width, "height": layout.paper.height },
        "list": summary,
        "text": texts.get(page).cloned().unwrap_or_default(),
    })))
}

/// A header or footer as one line, its sections joined by two spaces.
fn plain_of(s: &unluminous_sheet::pages::headerfooter::Sections) -> String {
    use unluminous_sheet::pages::headerfooter::Sections;
    [&s.left, &s.center, &s.right]
        .iter()
        .map(|runs| Sections::plain(runs))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("  ")
}

/// `print [file]`: write the PDF of the sheet and send it to the system's printer, or only write it when a
/// file is named.
fn print_now(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let (bytes, layout) = print::pdf_bytes(book);
    let target = match super::verbs_b_files::file_words(args) {
        Some(file) => super::verbs_b_files::resolve(sheet, &file),
        None => std::env::temp_dir()
            .join("unluminous-print")
            .join(format!("{}.pdf", sheet.current_book()?.name())),
    };
    if let Some(folder) = target.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    crate::services::store::write_a_source_file(&target, &bytes)
        .map_err(|e| format!("{} could not be written: {e}", target.display()))?;
    if args.is_empty() {
        send_to_the_printer(&target);
    }
    Ok(said(format!("{} pages", layout.page_count()))
        .with(json!({ "file": target.display().to_string(), "pages": layout.page_count() })))
}

/// Ask the operating system to print a PDF. A test never prints, and `UNLUMINOUS_PRINT=off` stops it.
fn send_to_the_printer(file: &std::path::Path) {
    if cfg!(test) || std::env::var("UNLUMINOUS_PRINT").is_ok_and(|v| v == "off") {
        return;
    }
    #[cfg(windows)]
    {
        let command = format!("Start-Process -FilePath '{}' -Verb Print", file.display());
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &command])
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("lp").arg(file).spawn();
    }
}

// ----------------------------------------------------------------------------------------------- read back

/// `page-setup` with no words: everything about printing the sheet showing.
fn read_back(sheet: &mut Spreadsheet) -> Out {
    let book = sheet.current_book()?;
    let index = book.view.sheet;
    let s = &book.session.book.sheets[index];
    let layout = print::layout_of(book);
    let value = setup_json(book, &s.page, &layout);
    Ok(said(format!("{} pages", layout.page_count())).with(value))
}

fn setup_json(
    book: &Book,
    page: &PageSetup,
    layout: &unluminous_sheet::pages::layout::PageLayout,
) -> Json {
    let hf = &page.header_footer;
    let paper = unluminous_sheet::pages::paper::paper(page.paper_size);
    json!({
        "sheet": book.session.book.sheets[book.view.sheet].name,
        "orientation": match page.orientation_or_default() { Orientation::Portrait => "portrait", Orientation::Landscape => "landscape" },
        "paper": { "code": page.paper_size, "name": paper.map(|p| p.name) },
        "margins": { "left": page.margins.left, "right": page.margins.right, "top": page.margins.top, "bottom": page.margins.bottom, "header": page.margins.header, "footer": page.margins.footer },
        "scale": page.scale,
        "fitToPage": page.fit_to_page,
        "fitToWidth": page.fit_to_width,
        "fitToHeight": page.fit_to_height,
        "gridlines": page.options.grid_lines,
        "headings": page.options.headings,
        "centerHorizontally": page.options.horizontal_centered,
        "centerVertically": page.options.vertical_centered,
        "pageOrder": match page.page_order { PageOrder::DownThenOver => "down", PageOrder::OverThenDown => "over" },
        "firstPageNumber": page.use_first_page_number.then_some(page.first_page_number),
        "header": { "odd": hf.odd_header, "even": hf.even_header, "first": hf.first_header },
        "footer": { "odd": hf.odd_footer, "even": hf.even_footer, "first": hf.first_footer },
        "differentFirst": hf.different_first,
        "differentOddEven": hf.different_odd_even,
        "comments": format!("{:?}", page.comments),
        "errors": format!("{:?}", page.errors),
        "blackAndWhite": page.black_and_white,
        "draft": page.draft,
        "printArea": print::area_text(book),
        "printTitles": titles_json(unluminous_sheet::pages::print_titles(&book.session.book, book.view.sheet)),
        "rowBreaks": layout.row_break_rows(),
        "columnBreaks": layout.col_break_cols(),
        "manualRowBreaks": page.row_break_rows(),
        "manualColumnBreaks": page.col_break_cols(),
        "pages": layout.page_count(),
    })
}

/// A cell that a verb's words name, used by the keys and tests.
pub fn cell_of(book: &Book, word: &str) -> Option<CellRef> {
    parse_range(book, word).map(|(_, r)| r.first)
}

/// Whether the words ask for the list rather than a change, for the verbs that read back with no words.
pub fn asks_for_list(args: &[String]) -> bool {
    args.is_empty() || flag(args, "list")
}
