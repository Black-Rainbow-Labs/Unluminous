//! Formatting in a BIFF8 workbook: the `FONT`, `FORMAT`, `PALETTE` and `XF` records, turned into the
//! same [`Styles`] a `styles.xml` part gives.
//!
//! The cell formats keep the order of the file, with one change: the formats cells use come first, so the
//! plain cell format (`XF` 15) is index 0 as it is in an `.xlsx` file, and the style formats a few cells
//! name directly follow them.

use std::collections::BTreeMap;

use super::records::{Reader, Record};
use crate::styles::{
    Alignment, Border, Color, Edge, Fill, Font, Styles, Underline, Xf, DEFAULT_THEME,
};

/// An `XF` record as stored.
#[derive(Debug, Clone, Default)]
pub struct RawXf {
    pub font: u16,
    pub format: u16,
    pub locked: bool,
    pub hidden: bool,
    pub is_style: bool,
    pub quote_prefix: bool,
    pub parent: u16,
    pub horizontal: u8,
    pub wrap: bool,
    pub vertical: u8,
    pub rotation: u8,
    pub indent: u8,
    pub shrink: bool,
    pub left: (u8, u16),
    pub right: (u8, u16),
    pub top: (u8, u16),
    pub bottom: (u8, u16),
    pub diagonal: (u8, u16),
    pub diagonal_flags: u8,
    pub pattern: u8,
    pub fore: u16,
    pub back: u16,
}

/// Read an `XF` record.
pub fn parse_xf(data: &[u8]) -> Option<RawXf> {
    let mut r = Reader::new(data);
    let font = r.u16()?;
    let format = r.u16()?;
    let flags = r.u16()?;
    let alignment = r.u8()?;
    let rotation = r.u8()?;
    let indent = r.u8()?;
    let _used = r.u8()?;
    let borders = r.u16()?;
    let colors = r.u16()?;
    let more = r.u32()?;
    let fill = r.u16()?;
    Some(RawXf {
        font,
        format,
        locked: flags & 1 != 0,
        hidden: flags & 2 != 0,
        is_style: flags & 4 != 0,
        quote_prefix: flags & 8 != 0,
        parent: flags >> 4,
        horizontal: alignment & 7,
        wrap: alignment & 8 != 0,
        vertical: (alignment >> 4) & 7,
        rotation,
        indent: indent & 0x0F,
        shrink: indent & 0x10 != 0,
        left: ((borders & 0xF) as u8, colors & 0x7F),
        right: (((borders >> 4) & 0xF) as u8, (colors >> 7) & 0x7F),
        top: (((borders >> 8) & 0xF) as u8, (more & 0x7F) as u16),
        bottom: (((borders >> 12) & 0xF) as u8, ((more >> 7) & 0x7F) as u16),
        diagonal: (((more >> 21) & 0xF) as u8, ((more >> 14) & 0x7F) as u16),
        diagonal_flags: ((colors >> 14) & 3) as u8,
        pattern: ((more >> 26) & 0x3F) as u8,
        fore: fill & 0x7F,
        back: (fill >> 7) & 0x7F,
    })
}

/// Read a `FONT` record.
pub fn parse_font(data: &[u8], palette: &Palette) -> Option<Font> {
    let mut r = Reader::new(data);
    let height = r.u16()?;
    let flags = r.u16()?;
    let color = r.u16()?;
    let weight = r.u16()?;
    let script = r.u16()?;
    let underline = r.u8()?;
    let family = r.u8()?;
    let charset = r.u8()?;
    r.skip(1)?;
    let name = r.short_string()?;
    Some(Font {
        name,
        size: height as f64 / 20.0,
        bold: weight >= 700,
        italic: flags & 2 != 0,
        underline: match underline {
            1 => Underline::Single,
            2 => Underline::Double,
            0x21 => Underline::SingleAccounting,
            0x22 => Underline::DoubleAccounting,
            _ => Underline::None,
        },
        strike: flags & 8 != 0,
        color: palette.color(color),
        vert_align: match script {
            1 => Some("superscript".to_owned()),
            2 => Some("subscript".to_owned()),
            _ => None,
        },
        family: Some(family as u32),
        scheme: None,
        charset: Some(charset as u32),
    })
}

/// The colours a workbook names by index: the 56 of the palette, which a `PALETTE` record can change.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    pub custom: Option<Vec<u32>>,
}

impl Palette {
    /// Read a `PALETTE` record: a count and then red, green, blue and a spare byte for each colour.
    pub fn from_record(record: &Record) -> Palette {
        let mut r = Reader::new(&record.data);
        let Some(count) = r.u16() else { return Palette::default() };
        let mut colors = Vec::new();
        for _ in 0..count {
            let Some(b) = r.take(4) else { break };
            colors.push(0xFF00_0000 | (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32);
        }
        Palette { custom: Some(colors) }
    }

    /// A colour index as the model holds it. A workbook with its own palette gets its real colours,
    /// because the model's indexed colours are the default palette.
    pub fn color(&self, icv: u16) -> Color {
        match icv {
            0x7FFF => Color::Auto,
            64 | 65 => Color::Indexed(icv as u32),
            8..=63 => match self.custom.as_ref().and_then(|c| c.get(icv as usize - 8)) {
                Some(&argb) => Color::Rgb(argb),
                None => Color::Indexed(icv as u32),
            },
            other => Color::Indexed(other as u32),
        }
    }
}

/// The workbook's formatting records, as read.
#[derive(Debug, Default)]
pub struct Raw {
    pub fonts: Vec<Font>,
    pub formats: BTreeMap<u32, String>,
    pub xfs: Vec<RawXf>,
    pub palette: Palette,
}

fn pattern_name(code: u8) -> &'static str {
    match code {
        0 => "none",
        1 => "solid",
        2 => "mediumGray",
        3 => "darkGray",
        4 => "lightGray",
        5 => "darkHorizontal",
        6 => "darkVertical",
        7 => "darkDown",
        8 => "darkUp",
        9 => "darkGrid",
        10 => "darkTrellis",
        11 => "lightHorizontal",
        12 => "lightVertical",
        13 => "lightDown",
        14 => "lightUp",
        15 => "lightGrid",
        16 => "lightTrellis",
        17 => "gray125",
        18 => "gray0625",
        _ => "none",
    }
}

fn line_name(code: u8) -> &'static str {
    match code {
        1 => "thin",
        2 => "medium",
        3 => "dashed",
        4 => "dotted",
        5 => "thick",
        6 => "double",
        7 => "hair",
        8 => "mediumDashed",
        9 => "dashDot",
        10 => "mediumDashDot",
        11 => "dashDotDot",
        12 => "mediumDashDotDot",
        13 => "slantDashDot",
        _ => "",
    }
}

fn edge((style, icv): (u8, u16), palette: &Palette) -> Edge {
    if style == 0 {
        return Edge::default();
    }
    Edge { style: line_name(style).to_owned(), color: palette.color(icv) }
}

fn intern<T: PartialEq>(list: &mut Vec<T>, item: T) -> u32 {
    if let Some(index) = list.iter().position(|existing| *existing == item) {
        return index as u32;
    }
    list.push(item);
    (list.len() - 1) as u32
}

/// The model's font index for a stored one: font 4 does not exist, and later fonts are numbered from 5.
fn font_slot(stored: u16, count: usize) -> u32 {
    let index = if stored > 4 { stored as usize - 1 } else { stored as usize };
    if index < count {
        index as u32
    } else {
        0
    }
}

/// Build the model's styles from the records, and the map from a stored format index to the model's.
pub fn build(raw: &Raw) -> (Styles, Vec<u32>) {
    let mut styles = Styles::default_workbook();
    styles.theme = DEFAULT_THEME.to_vec();
    styles.fonts = if raw.fonts.is_empty() { vec![Font::default()] } else { raw.fonts.clone() };
    styles.num_fmts = raw.formats.clone();
    if let Some(custom) = &raw.palette.custom {
        let mut xml = String::from("<colors><indexedColors>");
        for argb in custom {
            xml.push_str(&format!("<rgbColor rgb=\"{argb:08X}\"/>"));
        }
        xml.push_str("</indexedColors></colors>");
        styles.raw_colors = Some(xml);
    }
    styles.xfs.clear();
    let mut map = vec![0u32; raw.xfs.len()];
    // The formats cells use first, then the style formats that a cell happens to name.
    for pass in [false, true] {
        for (stored, xf) in raw.xfs.iter().enumerate() {
            if xf.is_style != pass {
                continue;
            }
            let model = model_xf(xf, &mut styles, &raw.palette);
            map[stored] = styles.xfs.len() as u32;
            styles.xfs.push(model);
        }
    }
    if styles.xfs.is_empty() {
        styles.xfs.push(Xf::default());
    }
    (styles, map)
}

/// One stored format as a model format, adding the fill and border it names to the lists.
fn model_xf(xf: &RawXf, styles: &mut Styles, palette: &Palette) -> Xf {
    let fill = Fill {
        pattern: pattern_name(xf.pattern).to_owned(),
        fg: if xf.pattern == 0 { Color::Auto } else { palette.color(xf.fore) },
        bg: if xf.pattern == 0 { Color::Auto } else { palette.color(xf.back) },
        raw: None,
    };
    let border = Border {
        left: edge(xf.left, palette),
        right: edge(xf.right, palette),
        top: edge(xf.top, palette),
        bottom: edge(xf.bottom, palette),
        diagonal: edge(xf.diagonal, palette),
        diagonal_down: xf.diagonal_flags & 1 != 0,
        diagonal_up: xf.diagonal_flags & 2 != 0,
    };
    let horizontal = match xf.horizontal {
        1 => Some("left"),
        2 => Some("center"),
        3 => Some("right"),
        4 => Some("fill"),
        5 => Some("justify"),
        6 => Some("centerContinuous"),
        7 => Some("distributed"),
        _ => None,
    };
    let vertical = match xf.vertical {
        0 => Some("top"),
        1 => Some("center"),
        3 => Some("justify"),
        4 => Some("distributed"),
        _ => None,
    };
    // Rotation 255 is the stacked letters of a vertical text.
    let align = Alignment {
        horizontal: horizontal.map(str::to_owned),
        vertical: vertical.map(str::to_owned),
        wrap: xf.wrap,
        indent: xf.indent as u32,
        rotation: xf.rotation as i32,
        shrink: xf.shrink,
    };
    let fill_index = if xf.pattern == 0 { 0 } else { fill_slot(&mut styles.fills, fill) };
    Xf {
        num_fmt: xf.format as u32,
        font: font_slot(xf.font, styles.fonts.len()),
        fill: fill_index,
        border: intern(&mut styles.borders, border),
        xf_id: 0,
        align,
        locked: (!xf.locked).then_some(false),
        hidden: xf.hidden.then_some(true),
        quote_prefix: xf.quote_prefix,
    }
}

/// The index of a fill, with `none` and `gray125` kept as the first two as a workbook always has them.
fn fill_slot(fills: &mut Vec<Fill>, fill: Fill) -> u32 {
    if fills.len() < 2 {
        fills.clear();
        fills.push(Fill::none());
        fills.push(Fill { pattern: "gray125".to_owned(), ..Default::default() });
    }
    if fill.pattern == "gray125" {
        return 1;
    }
    intern(fills, fill)
}
