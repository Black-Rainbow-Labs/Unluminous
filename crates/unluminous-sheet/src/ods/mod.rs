//! OpenDocument spreadsheets (`.ods`): the file Excel's `Save As` writes for "OpenDocument Spreadsheet" and
//! LibreOffice and many other programs write.
//!
//! The file is a zip holding `content.xml` (the sheets, their cells, and the styles those cells name),
//! `styles.xml` (the named styles and number formats), `settings.xml` (what a window showed: frozen
//! panes, zoom, the active sheet), `meta.xml` and `META-INF/manifest.xml`, with an uncompressed
//! `mimetype` entry first. [`read`] turns it into a [`crate::Workbook`] and [`write`] turns a workbook
//! into one.
//!
//! - [`formula`] translates `of:` formulas to and from Excel's syntax.
//! - [`numstyle`] translates number formats (data styles) to and from Excel format codes.
//! - [`cellstyle`] translates cell formatting.
//!
//! Excel is the oracle: `tests/fixtures/ods/` holds the files Excel writes from the feature workbooks of
//! `tools/excel-oracle/save-ods.ps1`, beside the workbooks they were saved from.

pub mod cellstyle;
pub mod formula;
pub mod numstyle;
mod read;
mod read_features;
mod write;
mod write_features;

pub use read::read;
pub(crate) use read::{date_serial, kept_formula};
pub use write::write;

pub const NS_TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
pub const NS_OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
pub const NS_TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
pub const NS_XLINK: &str = "http://www.w3.org/1999/xlink";
pub const NS_DC: &str = "http://purl.org/dc/elements/1.1/";
pub const NS_CONFIG: &str = "urn:oasis:names:tc:opendocument:xmlns:config:1.0";
pub const NS_CALCEXT: &str = "urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0";
pub const MIME: &str = "application/vnd.oasis.opendocument.spreadsheet";

/// How many pixels one character of the workbook's default font is wide, which turns the column widths a
/// file stores in characters into lengths. Measured with Excel: with Aptos Narrow, a column stored as 10.125
/// characters is written to an `.ods` file as 2.143125 cm, which is 81 pixels at 96 per inch, and 29.75
/// characters is 238 pixels. Calibri, the font of workbooks made before Excel 2023, is 7 pixels a character.
pub fn pixels_per_character(book: &crate::model::Workbook) -> f64 {
    let font = book.styles.fonts.first().map_or("", |f| f.name.as_str());
    if font.starts_with("Aptos") {
        8.0
    } else {
        7.0
    }
}

/// How many pixels wide a column is when its width is not set: 72 with Aptos Narrow and 64 with Calibri.
pub fn default_column_pixels(book: &crate::model::Workbook) -> f64 {
    if pixels_per_character(book) == 8.0 {
        72.0
    } else {
        64.0
    }
}

/// A width in centimetres as the number of characters of the default font.
pub fn characters_from_cm(cm: f64, pixels: f64) -> f64 {
    cm / 2.54 * 96.0 / pixels
}

/// A width in characters as centimetres.
pub fn cm_from_characters(characters: f64, pixels: f64) -> f64 {
    characters * pixels / 96.0 * 2.54
}

/// An attribute of an element in a namespace.
fn attr<'a>(node: roxmltree::Node<'a, '_>, ns: &str, name: &str) -> Option<&'a str> {
    node.attribute((ns, name))
}

/// The first child element with a local name.
fn child<'a, 'b>(node: roxmltree::Node<'a, 'b>, name: &str) -> Option<roxmltree::Node<'a, 'b>> {
    node.children().find(|n| n.is_element() && n.tag_name().name() == name)
}

/// A sheet name as it appears in an `.ods` address: with the quotes removed.
fn unquote(name: &str) -> String {
    let name = name.trim_start_matches('$');
    match name.strip_prefix('\'').and_then(|n| n.strip_suffix('\'')) {
        Some(inner) => inner.replace("''", "'"),
        None => name.to_owned(),
    }
}

/// Split `Sheet.A1:Sheet.B2` or `'My Sheet'.A1` into the sheet and the cell text of its first address.
fn split_address(address: &str) -> (String, String) {
    let chars: Vec<char> = address.chars().collect();
    let mut i = 0;
    let mut quoted = false;
    while i < chars.len() {
        match chars[i] {
            '\'' => quoted = !quoted,
            '.' if !quoted => break,
            _ => {}
        }
        i += 1;
    }
    let sheet: String = chars[..i.min(chars.len())].iter().collect();
    let rest: String = chars[(i + 1).min(chars.len())..].iter().collect();
    (unquote(&sheet), rest)
}
