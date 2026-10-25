//! Print (task-2359, TDD 5.5): the pages written as a PDF with the options the Print level offers, and
//! the PDF handed to the operating system, which shows its own print dialog. On Windows that is the
//! `print` verb of the default PDF program through `ShellExecuteW`; on macOS and elsewhere the PDF opens
//! in the system's viewer, whose Print is one key away.
//!
//! The options the hand over cannot carry are applied to the PDF first: which pages, odd or even, copies
//! (collated) and pages per sheet, which shrinks two or four pages onto each sheet.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use unluminous_word::layout::{Page, Pages};

use super::Doc;

/// Whether this process may hand a PDF to the operating system. Only the released binary sets it, so a
/// test never opens a print dialog on the machine running it.
static MAY_PRINT: AtomicBool = AtomicBool::new(false);

/// Let printing reach the operating system: the released binary calls this once.
pub fn allow_printing() {
    MAY_PRINT.store(true, Ordering::Relaxed);
}

/// Which pages of the document are printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Parity {
    #[default]
    All,
    Odd,
    Even,
}

/// What the Print level asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintOptions {
    /// Page numbers from 1, in the order they are printed; empty for every page.
    pub pages: Vec<usize>,
    pub parity: Parity,
    pub copies: u32,
    /// 1, 2 or 4 pages on each sheet.
    pub per_sheet: u32,
}

impl Default for PrintOptions {
    fn default() -> Self {
        PrintOptions { pages: Vec::new(), parity: Parity::All, copies: 1, per_sheet: 1 }
    }
}

/// Read a page range as people write it: `1-3, 5, 8-`. Answers page numbers from 1.
pub fn parse_range(text: &str, count: usize) -> Result<Vec<usize>, String> {
    let mut pages = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (from, to) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let number = |t: &str, fallback: usize| -> Result<usize, String> {
            if t.is_empty() {
                return Ok(fallback);
            }
            t.parse::<usize>().map_err(|_| format!("{part} is not a page range, such as 1-3, 5"))
        };
        let (from, to) = (number(from, 1)?, number(to, count)?);
        if from == 0 || from > to {
            return Err(format!("{part} is not a page range, such as 1-3, 5"));
        }
        pages.extend(from..=to.min(count));
    }
    Ok(pages)
}

/// The sheets that print: the pages chosen, odd or even, each copy in turn, and two or four pages
/// shrunk onto a sheet.
pub fn sheets(pages: &Pages, options: &PrintOptions) -> Pages {
    let chosen: Vec<&Page> = pages
        .pages
        .iter()
        .enumerate()
        .filter(|(i, _)| options.pages.is_empty() || options.pages.contains(&(i + 1)))
        .filter(|(i, _)| match options.parity {
            Parity::All => true,
            Parity::Odd => (i + 1) % 2 == 1,
            Parity::Even => (i + 1) % 2 == 0,
        })
        .map(|(_, p)| p)
        .collect();
    let mut out = pages.clone();
    out.pages.clear();
    for _ in 0..options.copies.max(1) {
        match options.per_sheet {
            2 | 4 => {
                for group in chosen.chunks(options.per_sheet as usize) {
                    out.pages.push(impose(group, options.per_sheet));
                }
            }
            _ => out.pages.extend(chosen.iter().map(|p| (*p).clone())),
        }
    }
    out
}

/// Two pages side by side on a sheet turned landscape, or four in a two by two grid on the same sheet.
fn impose(group: &[&Page], per_sheet: u32) -> Page {
    let first = group[0];
    let (w, h) = (first.width, first.height);
    let (sheet_w, sheet_h, columns) = if per_sheet == 2 { (h, w, 2) } else { (w, h, 2) };
    let rows = if per_sheet == 2 { 1 } else { 2 };
    let cell_w = sheet_w / columns as f32;
    let cell_h = sheet_h / rows as f32;
    let scale = (cell_w / w).min(cell_h / h);
    let mut sheet = first.clone();
    sheet.width = sheet_w;
    sheet.height = sheet_h;
    sheet.items.clear();
    sheet.carets.clear();
    sheet.background = None;
    for (index, page) in group.iter().enumerate() {
        let (column, row) = (index % columns, index / columns);
        let dx = column as f32 * cell_w + (cell_w - w * scale) / 2.0;
        let dy = row as f32 * cell_h + (cell_h - h * scale) / 2.0;
        if let Some(color) = page.background {
            sheet.items.push(unluminous_word::layout::Item::Fill {
                rect: unluminous_word::layout::Rect::new(dx, dy, w * scale, h * scale),
                color,
            });
        }
        for item in &page.items {
            let mut item = item.clone();
            item.scale(scale, dx, dy);
            sheet.items.push(item);
        }
    }
    sheet
}

/// Write the sheets as a PDF beside the system's temporary files, and answer where.
pub fn write_pdf(doc: &mut Doc, options: &PrintOptions) -> Result<PathBuf, String> {
    doc.ensure_pages();
    let printed = sheets(&doc.pages, options);
    if printed.pages.is_empty() {
        return Err("no page is chosen to print".into());
    }
    let folder = std::env::temp_dir().join("unluminous-print");
    std::fs::create_dir_all(&folder).map_err(|e| format!("{} could not be made: {e}", folder.display()))?;
    let name: String = doc.label().chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    let path = folder.join(format!("{}.pdf", name.trim_end_matches(".docx").trim()));
    let bytes = super::commands::pdf_bytes(doc, &printed);
    crate::services::store::write_atomically(&path, &bytes)
        .map_err(|e| format!("{} could not be written: {e}", path.display()))?;
    Ok(path)
}

/// Print: the PDF written and handed to the operating system. Answers what was done.
pub fn print(doc: &mut Doc, options: &PrintOptions) -> Result<String, String> {
    let path = write_pdf(doc, options)?;
    if !MAY_PRINT.load(Ordering::Relaxed) {
        return Ok(format!("Wrote {} to print", path.display()));
    }
    hand_over(&path)?;
    Ok(format!("Sent {} to the printer", path.display()))
}

#[cfg(windows)]
fn hand_over(path: &std::path::Path) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |text: &str| text.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let verb = wide("print");
    let file = wide(&path.display().to_string());
    // Safe: every pointer is null or into a buffer that outlives the call.
    let result = unsafe {
        ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL)
    };
    match result as isize > 32 {
        true => Ok(()),
        // No program says it prints a PDF: open it instead, so the viewer's own Print is there.
        false => crate::services::links::open_a_file_with_the_system(path),
    }
}

#[cfg(not(windows))]
fn hand_over(path: &std::path::Path) -> Result<(), String> {
    crate::services::links::open_a_file_with_the_system(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_reads_the_way_people_write_it() {
        assert_eq!(parse_range("1-3, 5", 9).unwrap(), vec![1, 2, 3, 5]);
        assert_eq!(parse_range("8-", 9).unwrap(), vec![8, 9]);
        assert!(parse_range("4-2", 9).is_err());
    }

    #[test]
    fn odd_pages_copies_and_two_a_sheet() {
        let mut pages = unluminous_word::layout::Pages::default();
        for number in 1..=5 {
            let mut page = unluminous_word::layout::Page::default();
            page.width = 612.0;
            page.height = 792.0;
            page.number = number;
            pages.pages.push(page);
        }
        let odd = sheets(&pages, &PrintOptions { parity: Parity::Odd, copies: 2, ..Default::default() });
        let numbers: Vec<u32> = odd.pages.iter().map(|p| p.number).collect();
        assert_eq!(numbers, vec![1, 3, 5, 1, 3, 5], "collated copies of the odd pages");
        let two = sheets(&pages, &PrintOptions { per_sheet: 2, ..Default::default() });
        assert_eq!(two.pages.len(), 3);
        assert_eq!((two.pages[0].width, two.pages[0].height), (792.0, 612.0), "two a sheet is landscape");
    }
}
