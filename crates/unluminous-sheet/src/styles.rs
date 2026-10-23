//! Formatting: fonts, fills, borders, alignment and number formats, and the cell formats that combine
//! them, which is what `xl/styles.xml` holds.
//!
//! A cell names one cell format by index, and a cell format names a font, a fill, a border and a number
//! format by index. Changing one property of one cell therefore makes a new font (or fill, or border)
//! and a new cell format, unless an equal one exists, which is what [`Styles::derive`] does.

use std::collections::BTreeMap;

/// A colour as the file writes it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Color {
    #[default]
    Auto,
    /// `AARRGGBB`.
    Rgb(u32),
    /// One of the theme's twelve colours, lightened or darkened by a tint in -1 to 1.
    Theme(u32, f64),
    /// One of the 64 legacy indexed colours.
    Indexed(u32),
}

impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color::Rgb(0xFF00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32)
    }

    /// Read `#RRGGBB`, `RRGGBB` or `AARRGGBB`.
    pub fn parse_hex(text: &str) -> Option<Self> {
        let hex = text.trim().trim_start_matches('#');
        let value = u32::from_str_radix(hex, 16).ok()?;
        match hex.len() {
            6 => Some(Color::Rgb(0xFF00_0000 | value)),
            8 => Some(Color::Rgb(value)),
            _ => None,
        }
    }

    /// The colour as red, green and blue, given the workbook's theme. `None` for automatic.
    pub fn resolve(&self, theme: &[u32]) -> Option<(u8, u8, u8)> {
        match *self {
            Color::Auto => None,
            Color::Rgb(argb) => Some(((argb >> 16) as u8, (argb >> 8) as u8, argb as u8)),
            Color::Indexed(index) => {
                if index == 64 || index == 65 {
                    return None;
                }
                let argb = INDEXED.get(index as usize).copied().unwrap_or(0xFF00_0000);
                Some(((argb >> 16) as u8, (argb >> 8) as u8, argb as u8))
            }
            Color::Theme(index, tint) => {
                // The file lists dk1 lt1 dk2 lt2 but theme index 0 is lt1 and 1 is dk1, as Excel reads them.
                let swapped = match index {
                    0 => 1,
                    1 => 0,
                    2 => 3,
                    3 => 2,
                    other => other,
                };
                let base = theme.get(swapped as usize).copied().or_else(|| DEFAULT_THEME.get(swapped as usize).copied())?;
                Some(tinted(base, tint))
            }
        }
    }

    /// `#RRGGBB`, or `auto`.
    pub fn to_hex(&self, theme: &[u32]) -> String {
        match self.resolve(theme) {
            Some((r, g, b)) => format!("#{r:02X}{g:02X}{b:02X}"),
            None => "auto".to_owned(),
        }
    }
}

/// Apply Excel's tint to a colour, through hue, luminance and saturation.
fn tinted(rgb: u32, tint: f64) -> (u8, u8, u8) {
    let (r, g, b) = (((rgb >> 16) & 0xFF) as f64 / 255.0, ((rgb >> 8) & 0xFF) as f64 / 255.0, (rgb & 0xFF) as f64 / 255.0);
    if tint == 0.0 {
        return ((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8);
    }
    let (h, l, s) = rgb_to_hls(r, g, b);
    let l = if tint < 0.0 { l * (1.0 + tint) } else { l * (1.0 - tint) + tint };
    let (r, g, b) = hls_to_rgb(h, l, s);
    ((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
}

fn rgb_to_hls(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, l, 0.0);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, l, s)
}

fn hls_to_rgb(h: f64, l: f64, s: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
}

/// Office's default theme colours, in file order: dk1 lt1 dk2 lt2 accent1 to accent6 hlink folHlink.
pub const DEFAULT_THEME: [u32; 12] = [
    0x000000, 0xFFFFFF, 0x0E2841, 0xE8E8E8, 0x156082, 0xE97132, 0x196B24, 0x0F9ED5, 0xA02B93, 0x4EA72E, 0x467886,
    0x96607D,
];

/// The legacy indexed palette.
pub const INDEXED: [u32; 64] = [
    0xFF000000, 0xFFFFFFFF, 0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0xFFFFFF00, 0xFFFF00FF, 0xFF00FFFF, 0xFF000000,
    0xFFFFFFFF, 0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0xFFFFFF00, 0xFFFF00FF, 0xFF00FFFF, 0xFF800000, 0xFF008000,
    0xFF000080, 0xFF808000, 0xFF800080, 0xFF008080, 0xFFC0C0C0, 0xFF808080, 0xFF9999FF, 0xFF993366, 0xFFFFFFCC,
    0xFFCCFFFF, 0xFF660066, 0xFFFF8080, 0xFF0066CC, 0xFFCCCCFF, 0xFF000080, 0xFFFF00FF, 0xFFFFFF00, 0xFF00FFFF,
    0xFF800080, 0xFF800000, 0xFF008080, 0xFF0000FF, 0xFF00CCFF, 0xFFCCFFFF, 0xFFCCFFCC, 0xFFFFFF99, 0xFF99CCFF,
    0xFFFF99CC, 0xFFCC99FF, 0xFFFFCC99, 0xFF3366FF, 0xFF33CCCC, 0xFF99CC00, 0xFFFFCC00, 0xFFFF9900, 0xFFFF6600,
    0xFF666699, 0xFF969696, 0xFF003366, 0xFF339966, 0xFF003300, 0xFF333300, 0xFF993300, 0xFF993366, 0xFF333399,
    0xFF333333,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Underline {
    #[default]
    None,
    Single,
    Double,
    SingleAccounting,
    DoubleAccounting,
}

impl Underline {
    pub fn name(&self) -> Option<&'static str> {
        match self {
            Underline::None => None,
            Underline::Single => Some("single"),
            Underline::Double => Some("double"),
            Underline::SingleAccounting => Some("singleAccounting"),
            Underline::DoubleAccounting => Some("doubleAccounting"),
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "double" => Underline::Double,
            "singleAccounting" => Underline::SingleAccounting,
            "doubleAccounting" => Underline::DoubleAccounting,
            "none" => Underline::None,
            _ => Underline::Single,
        }
    }
}

/// A font.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    pub name: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: Underline,
    pub strike: bool,
    pub color: Color,
    /// `superscript` or `subscript`.
    pub vert_align: Option<String>,
    pub family: Option<u32>,
    pub scheme: Option<String>,
    pub charset: Option<u32>,
}

impl Default for Font {
    fn default() -> Self {
        Self {
            name: "Calibri".to_owned(),
            size: 11.0,
            bold: false,
            italic: false,
            underline: Underline::None,
            strike: false,
            color: Color::Theme(1, 0.0),
            vert_align: None,
            family: Some(2),
            scheme: Some("minor".to_owned()),
            charset: None,
        }
    }
}

/// A cell's background.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fill {
    /// `none`, `solid`, `gray125` and the other pattern names, or `gradient`.
    pub pattern: String,
    pub fg: Color,
    pub bg: Color,
    /// A gradient fill's XML, kept and drawn as its first stop's colour.
    pub raw: Option<String>,
}

impl Fill {
    pub fn none() -> Self {
        Self { pattern: "none".to_owned(), ..Default::default() }
    }

    pub fn solid(color: Color) -> Self {
        Self { pattern: "solid".to_owned(), fg: color, bg: Color::Indexed(64), raw: None }
    }

    /// The colour to paint, if any.
    pub fn paint(&self) -> Option<Color> {
        match self.pattern.as_str() {
            "none" | "" | "gray125" => None,
            "solid" => Some(self.fg),
            _ => Some(if matches!(self.fg, Color::Auto) { self.bg } else { self.fg }),
        }
    }
}

/// One side of a border.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Edge {
    /// Empty for no line, otherwise `thin`, `medium`, `thick`, `dashed`, `dotted`, `double`, `hair` and the
    /// rest of the schema's names.
    pub style: String,
    pub color: Color,
}

impl Edge {
    pub fn is_none(&self) -> bool {
        self.style.is_empty() || self.style == "none"
    }

    pub fn line(style: &str, color: Color) -> Self {
        Self { style: style.to_owned(), color }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Border {
    pub left: Edge,
    pub right: Edge,
    pub top: Edge,
    pub bottom: Edge,
    pub diagonal: Edge,
    pub diagonal_up: bool,
    pub diagonal_down: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Alignment {
    /// `left`, `center`, `right`, `fill`, `justify`, `centerContinuous`, `distributed`, or none for general.
    pub horizontal: Option<String>,
    /// `top`, `center`, `bottom`, `justify`, `distributed`, or none for bottom.
    pub vertical: Option<String>,
    pub wrap: bool,
    pub indent: u32,
    pub rotation: i32,
    pub shrink: bool,
}

impl Alignment {
    pub fn is_default(&self) -> bool {
        *self == Alignment::default()
    }
}

/// A cell format.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Xf {
    pub num_fmt: u32,
    pub font: u32,
    pub fill: u32,
    pub border: u32,
    pub xf_id: u32,
    pub align: Alignment,
    /// `None` is Excel's default: locked and not hidden.
    pub locked: Option<bool>,
    pub hidden: Option<bool>,
    pub quote_prefix: bool,
}

/// A differential format, which conditional formats apply over a cell's own.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dxf {
    pub font_color: Option<Color>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub strike: Option<bool>,
    pub underline: Option<Underline>,
    pub fill: Option<Color>,
    pub num_fmt: Option<String>,
    pub border: Option<Color>,
    /// As read, written back unchanged when nothing above was set by Unluminous.
    pub raw: Option<String>,
}

/// Everything `xl/styles.xml` holds.
#[derive(Debug, Clone, Default)]
pub struct Styles {
    /// Custom number formats by id, 164 and up. Ids under 164 are built in.
    pub num_fmts: BTreeMap<u32, String>,
    pub fonts: Vec<Font>,
    pub fills: Vec<Fill>,
    pub borders: Vec<Border>,
    pub xfs: Vec<Xf>,
    pub dxfs: Vec<Dxf>,
    /// `cellStyleXfs`, `cellStyles`, `tableStyles`, `colors` and `extLst`, kept as XML.
    pub raw_cell_style_xfs: Option<String>,
    pub raw_cell_styles: Option<String>,
    pub raw_table_styles: Option<String>,
    pub raw_colors: Option<String>,
    pub raw_ext: Option<String>,
    pub namespaces: Vec<(String, String)>,
    /// The theme's twelve colours in file order, read from `xl/theme/theme1.xml`.
    pub theme: Vec<u32>,
    /// The theme's minor (body) font, the font a new workbook's cells are in.
    pub theme_font: Option<String>,
}

impl Styles {
    /// The styles of a new workbook: Calibri 11, no fill, no border, the General format.
    pub fn default_workbook() -> Self {
        Self {
            fonts: vec![Font::default()],
            fills: vec![Fill::none(), Fill { pattern: "gray125".to_owned(), ..Default::default() }],
            borders: vec![Border::default()],
            xfs: vec![Xf::default()],
            theme: DEFAULT_THEME.to_vec(),
            ..Default::default()
        }
    }

    pub fn xf(&self, index: u32) -> Xf {
        self.xfs.get(index as usize).cloned().unwrap_or_default()
    }

    pub fn font_of(&self, xf: u32) -> Font {
        let index = self.xf(xf).font;
        self.fonts.get(index as usize).cloned().or_else(|| self.fonts.first().cloned()).unwrap_or_default()
    }

    pub fn fill_of(&self, xf: u32) -> Fill {
        let index = self.xf(xf).fill;
        self.fills.get(index as usize).cloned().unwrap_or_else(Fill::none)
    }

    pub fn border_of(&self, xf: u32) -> Border {
        let index = self.xf(xf).border;
        self.borders.get(index as usize).cloned().unwrap_or_default()
    }

    /// The format code a cell format shows its value with.
    pub fn format_code(&self, xf: u32) -> String {
        let id = self.xf(xf).num_fmt;
        self.code_for_id(id)
    }

    pub fn code_for_id(&self, id: u32) -> String {
        if let Some(code) = self.num_fmts.get(&id) {
            return code.clone();
        }
        builtin_format(id).unwrap_or("General").to_owned()
    }

    /// The id of a format code, adding it when it is new.
    pub fn format_id(&mut self, code: &str) -> u32 {
        if let Some(id) = (0..50).find(|id| builtin_format(*id) == Some(code)) {
            return id;
        }
        if let Some((id, _)) = self.num_fmts.iter().find(|(_, c)| c.as_str() == code) {
            return *id;
        }
        let id = self.num_fmts.keys().next_back().map_or(164, |last| (last + 1).max(164));
        self.num_fmts.insert(id, code.to_owned());
        id
    }

    fn intern<T: PartialEq>(list: &mut Vec<T>, item: T) -> u32 {
        if let Some(index) = list.iter().position(|existing| *existing == item) {
            return index as u32;
        }
        list.push(item);
        (list.len() - 1) as u32
    }

    /// The cell format that is `xf` with one change applied, made when it does not exist yet.
    pub fn derive(&mut self, xf: u32, change: &StyleChange) -> u32 {
        if let StyleChange::Copy(other) = change {
            return *other;
        }
        let mut next = self.xf(xf);
        match change {
            StyleChange::NumberFormat(code) => next.num_fmt = self.format_id(code),
            StyleChange::Font(edit) => {
                let mut font = self.font_of(xf);
                edit.apply(&mut font);
                next.font = Self::intern(&mut self.fonts, font);
            }
            StyleChange::Fill(color) => {
                let fill = match color {
                    Some(color) => Fill::solid(*color),
                    None => Fill::none(),
                };
                next.fill = Self::intern(&mut self.fills, fill);
            }
            StyleChange::Border(edit) => {
                let mut border = self.border_of(xf);
                edit.apply(&mut border);
                next.border = Self::intern(&mut self.borders, border);
            }
            StyleChange::Align(edit) => edit.apply(&mut next.align),
            StyleChange::Locked(locked) => next.locked = Some(*locked),
            StyleChange::Copy(_) => {}
        }
        Self::intern(&mut self.xfs, next)
    }
}

/// One change to a cell's format, which the Format panel and the command line both make.
#[derive(Debug, Clone, PartialEq)]
pub enum StyleChange {
    NumberFormat(String),
    Font(FontEdit),
    /// A solid fill, or none.
    Fill(Option<Color>),
    Border(BorderEdit),
    Align(AlignEdit),
    Locked(bool),
    /// Take another cell format whole: what Paste Format does.
    Copy(u32),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FontEdit {
    pub name: Option<String>,
    pub size: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<Underline>,
    pub strike: Option<bool>,
    pub color: Option<Color>,
}

impl FontEdit {
    fn apply(&self, font: &mut Font) {
        if let Some(name) = &self.name {
            font.name = name.clone();
            font.scheme = None;
        }
        if let Some(size) = self.size {
            font.size = size;
        }
        if let Some(v) = self.bold {
            font.bold = v;
        }
        if let Some(v) = self.italic {
            font.italic = v;
        }
        if let Some(v) = self.underline {
            font.underline = v;
        }
        if let Some(v) = self.strike {
            font.strike = v;
        }
        if let Some(v) = self.color {
            font.color = v;
        }
    }
}

/// Which sides a border change applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sides {
    Left,
    Right,
    Top,
    Bottom,
    /// All four sides of each cell.
    All,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BorderEdit {
    pub sides: Sides,
    pub edge: Edge,
}

impl BorderEdit {
    fn apply(&self, border: &mut Border) {
        match self.sides {
            Sides::Left => border.left = self.edge.clone(),
            Sides::Right => border.right = self.edge.clone(),
            Sides::Top => border.top = self.edge.clone(),
            Sides::Bottom => border.bottom = self.edge.clone(),
            Sides::All => {
                border.left = self.edge.clone();
                border.right = self.edge.clone();
                border.top = self.edge.clone();
                border.bottom = self.edge.clone();
            }
            Sides::None => *border = Border::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AlignEdit {
    /// `Some(None)` sets general.
    pub horizontal: Option<Option<String>>,
    pub vertical: Option<Option<String>>,
    pub wrap: Option<bool>,
    pub indent: Option<u32>,
    pub rotation: Option<i32>,
    pub shrink: Option<bool>,
}

impl AlignEdit {
    fn apply(&self, align: &mut Alignment) {
        if let Some(v) = &self.horizontal {
            align.horizontal = v.clone();
        }
        if let Some(v) = &self.vertical {
            align.vertical = v.clone();
        }
        if let Some(v) = self.wrap {
            align.wrap = v;
        }
        if let Some(v) = self.indent {
            align.indent = v;
        }
        if let Some(v) = self.rotation {
            align.rotation = v;
        }
        if let Some(v) = self.shrink {
            align.shrink = v;
        }
    }
}

/// The built in number formats, which a file names by id and does not write out.
pub fn builtin_format(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        5 => "\"$\"#,##0_);\\(\"$\"#,##0\\)",
        6 => "\"$\"#,##0_);[Red]\\(\"$\"#,##0\\)",
        7 => "\"$\"#,##0.00_);\\(\"$\"#,##0.00\\)",
        8 => "\"$\"#,##0.00_);[Red]\\(\"$\"#,##0.00\\)",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "m/d/yyyy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yyyy h:mm",
        37 => "#,##0_);(#,##0)",
        38 => "#,##0_);[Red](#,##0)",
        39 => "#,##0.00_);(#,##0.00)",
        40 => "#,##0.00_);[Red](#,##0.00)",
        41 => "_(* #,##0_);_(* \\(#,##0\\);_(* \"-\"_);_(@_)",
        42 => "_(\"$\"* #,##0_);_(\"$\"* \\(#,##0\\);_(\"$\"* \"-\"_);_(@_)",
        43 => "_(* #,##0.00_);_(* \\(#,##0.00\\);_(* \"-\"??_);_(@_)",
        44 => "_(\"$\"* #,##0.00_);_(\"$\"* \\(#,##0.00\\);_(\"$\"* \"-\"??_);_(@_)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deriving_the_same_change_twice_reuses_the_format() {
        let mut styles = Styles::default_workbook();
        let bold = StyleChange::Font(FontEdit { bold: Some(true), ..Default::default() });
        let a = styles.derive(0, &bold);
        let b = styles.derive(0, &bold);
        assert_eq!(a, b);
        assert_eq!(styles.fonts.len(), 2);
        assert!(styles.font_of(a).bold);
        let money = styles.derive(a, &StyleChange::NumberFormat("$#,##0.00".to_owned()));
        assert_eq!(styles.format_code(money), "$#,##0.00");
        assert!(styles.font_of(money).bold);
        assert_eq!(styles.format_id("0.00"), 2);
    }

    #[test]
    fn theme_colours_tint_the_way_excel_does() {
        // Accent 1 lightened by 80% is the pale blue Excel offers in the first row under it.
        let theme = DEFAULT_THEME.to_vec();
        assert_eq!(Color::Theme(1, 0.0).to_hex(&theme), "#000000");
        assert_eq!(Color::Theme(0, 0.0).to_hex(&theme), "#FFFFFF");
        assert_eq!(Color::Theme(0, -0.15).to_hex(&theme), "#D9D9D9");
        assert_eq!(Color::parse_hex("#ff0000"), Some(Color::Rgb(0xFFFF0000)));
    }
}
