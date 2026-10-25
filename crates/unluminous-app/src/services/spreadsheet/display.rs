//! How a cell is shown: its text as its number format makes it, and its font, fill, borders and
//! alignment resolved to colours and sizes the grid can draw. Kept apart from the drawing so a test can
//! ask what a cell shows with no window.

use std::sync::Arc;

use unluminous_sheet::coord::CellRef;
use unluminous_sheet::model::Cell;
use unluminous_sheet::numfmt::Format;
use unluminous_sheet::styles::{Color, Edge, Underline, INDEXED};
use unluminous_sheet::value::{number_to_text, Value};
use unluminous_sheet::{Sheet, Workbook};

/// A colour as red, green and blue.
pub type Rgb = (u8, u8, u8);

/// The white page a sheet is drawn on with `paper = light`, as Excel draws one.
pub const LIGHT_PAPER: Rgb = (255, 255, 255);
/// Automatic text on a light page or a light fill.
pub const LIGHT_TEXT: Rgb = (0, 0, 0);
/// Excel's gridlines on a white page.
pub const LIGHT_GRID: Rgb = (0xD9, 0xD9, 0xD9);

/// The colours the references in a formula being typed are outlined in, in turn: Excel's own order.
pub const REFERENCE_COLOURS: [Rgb; 7] = [
    (0x44, 0x72, 0xC4),
    (0xC0, 0x50, 0x4D),
    (0x80, 0x64, 0xA2),
    (0x4B, 0xAC, 0xC6),
    (0x9B, 0xBB, 0x59),
    (0xF7, 0x96, 0x46),
    (0x2C, 0x4D, 0x75),
];

/// The colours the Format panel offers for text, fills, borders and tabs: a light row, Excel's ten
/// standard colours, and a row of dark and grey.
pub const SWATCHES: [&str; 30] = [
    "#FFFFFF", "#F2F2F2", "#DDEBF7", "#E2EFDA", "#FFF2CC", "#FCE4D6", "#E4DFEC", "#DDD9C4",
    "#D9E1F2", "#FFE7E7", "#C00000", "#FF0000", "#FFC000", "#FFFF00", "#92D050", "#00B050",
    "#00B0F0", "#0070C0", "#002060", "#7030A0", "#000000", "#404040", "#595959", "#808080",
    "#A6A6A6", "#BFBFBF", "#D9D9D9", "#1F4E79", "#375623", "#833C0B",
];

/// Where text sits across a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Across {
    Left,
    Center,
    Right,
    /// Repeat the text to fill the cell.
    Fill,
    Justify,
    /// Centred across the empty cells beside it.
    CenterAcross,
}

/// Where text sits up and down a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Down {
    Top,
    Center,
    Bottom,
}

/// One side of a cell's border.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    /// How many points wide: 1 for thin, hair, dotted and dashed, 2 for medium, 3 for thick, 3 for double.
    pub width: f32,
    pub color: Option<Rgb>,
    pub dashed: bool,
    pub double: bool,
}

impl Line {
    fn of(edge: &Edge, theme: &[u32]) -> Option<Line> {
        if edge.is_none() {
            return None;
        }
        let style = edge.style.as_str();
        Some(Line {
            width: match style {
                "medium" | "mediumDashed" | "mediumDashDot" | "mediumDashDotDot"
                | "slantDashDot" => 2.0,
                "thick" | "double" => 3.0,
                _ => 1.0,
            },
            color: edge.color.resolve(theme),
            dashed: style.contains("ash") || style.contains("otted") || style == "hair",
            double: style == "double",
        })
    }
}

/// Everything the grid needs to draw one cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub text: String,
    /// Numbers and dates, which General aligns to the right and which become `####` when too wide.
    pub numeric: bool,
    /// The font's colour, or the number format's, when either names one rather than automatic.
    pub color: Option<Rgb>,
    pub fill: Option<Rgb>,
    pub family: Arc<str>,
    /// In points, as the file has it.
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub across: Across,
    pub down: Down,
    pub wrap: bool,
    pub indent: u32,
    /// Shrink the text to fit the cell.
    pub shrink: bool,
    pub left: Option<Line>,
    pub right: Option<Line>,
    pub top: Option<Line>,
    pub bottom: Option<Line>,
    /// The character a `*` in the format repeats to fill the cell, and where it goes in `text`.
    pub repeat: Option<(char, usize)>,
    /// An error value, drawn in its own colour on a dark ground so it can be told from text.
    pub error: bool,
}

/// The style index a cell is drawn with: its own, or its row's, or its column's.
pub fn style_at(sheet: &Sheet, at: CellRef) -> u32 {
    if let Some(cell) = sheet.cell(at) {
        return cell.style;
    }
    if let Some(style) = sheet.rows.get(&at.row).and_then(|row| row.style) {
        return style;
    }
    sheet.column(at.col).map_or(0, |column| column.style)
}

/// The value a cell shows, including one a spill put there.
pub fn value_at(sheet: &Sheet, at: CellRef) -> Value {
    sheet.value_at(at)
}

/// A colour a number format section names: `[Red]` or `[Color12]`.
fn section_color(name: &str) -> Option<Rgb> {
    let rgb = |argb: u32| ((argb >> 16) as u8, (argb >> 8) as u8, argb as u8);
    Some(match name.to_ascii_lowercase().as_str() {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 255, 0),
        "blue" => (0, 0, 255),
        "yellow" => (255, 255, 0),
        "magenta" => (255, 0, 255),
        "cyan" => (0, 255, 255),
        other => {
            let n: usize = other.strip_prefix("color")?.parse().ok()?;
            rgb(*INDEXED.get(n + 7)?)
        }
    })
}

/// How a cell shows, given how many characters wide its column is (for General's rounding).
pub fn shown(book: &Workbook, sheet: &Sheet, at: CellRef, chars: usize) -> Shown {
    let style = style_at(sheet, at);
    let value = value_at(sheet, at);
    let mut shown = shown_with(book, style, &value, chars);
    adjust_for_the_view(book, sheet, at, &value, &mut shown);
    shown
}

/// What a sheet's view changes about how a cell shows: Show Formulas shows what was typed rather than what
/// it computes, and a sheet that hides zeros shows a zero as an empty cell, as Excel does for both.
pub fn adjust_for_the_view(
    book: &Workbook,
    sheet: &Sheet,
    at: CellRef,
    value: &Value,
    shown: &mut Shown,
) {
    if sheet.view.show_formulas {
        let typed = input_text(book, sheet, at);
        if !typed.is_empty() || value.is_empty() {
            let formula = typed.starts_with('=');
            shown.text = typed;
            shown.numeric = false;
            shown.repeat = None;
            if formula {
                shown.across = Across::Left;
            }
        }
    } else if !sheet.view.show_zeros && matches!(value, Value::Number(n) if *n == 0.0) {
        shown.text = String::new();
    }
}

/// How a value shows in a given style.
pub fn shown_with(book: &Workbook, style: u32, value: &Value, chars: usize) -> Shown {
    let format = Format::parse(&book.styles.format_code(style));
    shown_in(book, style, value, chars, &format)
}

/// How a value shows in a given style, with its number format already read, which the grid keeps a frame.
pub fn shown_in(
    book: &Workbook,
    style: u32,
    value: &Value,
    chars: usize,
    format: &Format,
) -> Shown {
    let styles = &book.styles;
    let theme = &styles.theme;
    let xf = styles.xf(style);
    let font = styles.font_of(style);
    let rendered = format.render(value, book.date1904, chars.clamp(1, 255));
    let fill = styles.fill_of(style).paint().and_then(|c| c.resolve(theme));
    let border = styles.border_of(style);
    // The default text colour is the theme's dark one, which is automatic for every purpose here.
    let automatic = matches!(font.color, Color::Auto | Color::Indexed(64) | Color::Theme(1, 0.0));
    let color = rendered
        .color
        .as_deref()
        .and_then(section_color)
        .or_else(|| (!automatic).then(|| font.color.resolve(theme)).flatten());
    let align = &xf.align;
    let numeric = rendered.numeric;
    let across = match align.horizontal.as_deref() {
        Some("left") => Across::Left,
        Some("center") | Some("distributed") => Across::Center,
        Some("right") => Across::Right,
        Some("fill") => Across::Fill,
        Some("justify") => Across::Justify,
        Some("centerContinuous") => Across::CenterAcross,
        // General: numbers right, text left, logical values and errors centred.
        _ => match value {
            Value::Number(_) => Across::Right,
            Value::Bool(_) | Value::Error(_) => Across::Center,
            _ => Across::Left,
        },
    };
    let down = match align.vertical.as_deref() {
        Some("top") => Down::Top,
        Some("center") | Some("distributed") | Some("justify") => Down::Center,
        _ => Down::Bottom,
    };
    Shown {
        text: rendered.text,
        numeric,
        color,
        fill,
        family: Arc::from(font.name.as_str()),
        size: font.size as f32,
        bold: font.bold,
        italic: font.italic,
        underline: font.underline != Underline::None,
        strike: font.strike,
        across,
        down,
        wrap: align.wrap,
        indent: align.indent,
        shrink: align.shrink,
        left: Line::of(&border.left, theme),
        right: Line::of(&border.right, theme),
        top: Line::of(&border.top, theme),
        bottom: Line::of(&border.bottom, theme),
        repeat: rendered.fill,
        error: value.is_error(),
    }
}

/// The text a cell holds for typing over: its formula, or its value as it would be typed.
pub fn input_text(book: &Workbook, sheet: &Sheet, at: CellRef) -> String {
    let anchor = sheet.spills.owner.get(&(at.row, at.col)).copied();
    let cell: Option<&Cell> =
        sheet.cell(at).or_else(|| anchor.and_then(|(r, c)| sheet.cell(CellRef::new(r, c))));
    let Some(cell) = cell else { return String::new() };
    if let Some(formula) = &cell.formula {
        return formula.display();
    }
    let code = book.styles.format_code(cell.style);
    let format = Format::parse(&code);
    match &cell.value {
        Value::Empty => String::new(),
        Value::Text(text) => {
            if book.styles.xf(cell.style).quote_prefix {
                format!("'{text}")
            } else {
                text.to_string()
            }
        }
        Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
        Value::Error(e) => e.text().to_owned(),
        Value::Number(n) if format.is_date() => date_input(*n, book.date1904),
        Value::Number(n) if code.contains('%') => format!("{}%", number_to_text(n * 100.0)),
        Value::Number(n) => number_to_text(*n),
    }
}

/// A date as it is typed: `10/23/2026`, `10/23/2026 1:30:00 PM`, or `1:30:00 PM` for a time alone.
fn date_input(n: f64, date1904: bool) -> String {
    let day = n.floor();
    let (h, m, s, _) = unluminous_sheet::dates::time(n);
    let time = || {
        let (hour, half) = match h {
            0 => (12, "AM"),
            1..=11 => (h, "AM"),
            12 => (12, "PM"),
            _ => (h - 12, "PM"),
        };
        format!("{hour}:{m:02}:{s:02} {half}")
    };
    if day == 0.0 && n > 0.0 {
        return time();
    }
    let Some((y, mo, d)) = unluminous_sheet::dates::date(n, date1904) else {
        return number_to_text(n);
    };
    if n == day {
        format!("{mo}/{d}/{y}")
    } else {
        format!("{mo}/{d}/{y} {}", time())
    }
}

/// How many characters General may use in a column of so many points: the seven point digit.
pub fn chars_in(points: f32) -> usize {
    (((points - 5.0) / 7.0).floor().max(1.0)) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use unluminous_sheet::edit::Session;

    #[test]
    fn a_cell_shows_its_format_and_types_back_as_it_was_typed() {
        let mut session = Session::new(Workbook::new(), 46318.5);
        let at = CellRef::new(0, 0);
        session.set_input(0, at, "10/23/2026").unwrap();
        let book = &session.book;
        assert_eq!(shown(book, &book.sheets[0], at, 10).text, "10/23/2026");
        assert_eq!(input_text(book, &book.sheets[0], at), "10/23/2026");
        let next = CellRef::new(1, 0);
        session.set_input(0, next, "12.5%").unwrap();
        let book = &session.book;
        assert_eq!(input_text(book, &book.sheets[0], next), "12.5%");
        assert_eq!(shown(book, &book.sheets[0], next, 10).across, Across::Right);
        session.set_input(0, at, "=1/3").unwrap();
        let book = &session.book;
        assert_eq!(input_text(book, &book.sheets[0], at), "=1/3");
    }

    #[test]
    fn general_rounds_to_the_width_of_the_column() {
        let mut session = Session::new(Workbook::new(), 46318.5);
        let at = CellRef::new(0, 0);
        session.set_input(0, at, "=1/3").unwrap();
        let book = &session.book;
        assert_eq!(shown(book, &book.sheets[0], at, chars_in(64.0)).text, "0.333333");
    }
}
