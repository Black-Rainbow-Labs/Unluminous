//! `xl/styles.bin`: fonts, fills, borders, number formats and cell formats as binary records.

use super::records::{id, split, wide_string, Reader, Record};
use crate::styles::{Alignment, Border, Color, Dxf, Edge, Fill, Font, Styles, Underline, Xf};

/// A colour as the binary format stores it: a kind, an index or three channels, and a tint.
pub fn color(r: &mut Reader) -> Option<Color> {
    let flags = r.u8()?;
    let index = r.u8()?;
    let tint = r.i16()?;
    let rgba = r.take(4)?;
    Some(match flags >> 1 {
        1 => Color::Indexed(index as u32),
        2 => Color::rgb(rgba[0], rgba[1], rgba[2]),
        3 => Color::Theme(index as u32, tint as f64 / 32767.0),
        _ => Color::Auto,
    })
}

fn font(data: &[u8]) -> Option<Font> {
    let mut r = Reader::new(data);
    let height = r.u16()?;
    let flags = r.u16()?;
    let weight = r.u16()?;
    let script = r.u16()?;
    let underline = r.u8()?;
    let family = r.u8()?;
    let charset = r.u8()?;
    r.skip(1)?;
    let color = color(&mut r)?;
    let scheme = r.u8()?;
    let name = wide_string(&mut r)?;
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
        color,
        vert_align: match script {
            1 => Some("superscript".to_owned()),
            2 => Some("subscript".to_owned()),
            _ => None,
        },
        family: Some(family as u32),
        scheme: match scheme {
            1 => Some("major".to_owned()),
            2 => Some("minor".to_owned()),
            _ => None,
        },
        charset: Some(charset as u32),
    })
}

fn pattern_name(code: u32) -> &'static str {
    const NAMES: [&str; 19] = [
        "none",
        "solid",
        "mediumGray",
        "darkGray",
        "lightGray",
        "darkHorizontal",
        "darkVertical",
        "darkDown",
        "darkUp",
        "darkGrid",
        "darkTrellis",
        "lightHorizontal",
        "lightVertical",
        "lightDown",
        "lightUp",
        "lightGrid",
        "lightTrellis",
        "gray125",
        "gray0625",
    ];
    NAMES.get(code as usize).copied().unwrap_or("none")
}

fn fill(data: &[u8]) -> Option<Fill> {
    let mut r = Reader::new(data);
    let pattern = r.u32()?;
    let fg = color(&mut r)?;
    let bg = color(&mut r)?;
    Some(Fill { pattern: pattern_name(pattern).to_owned(), fg, bg, raw: None })
}

fn line_name(code: u16) -> &'static str {
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

fn edge(r: &mut Reader) -> Option<Edge> {
    let style = r.u16()?;
    let color = color(r)?;
    Some(if style == 0 {
        Edge::default()
    } else {
        Edge { style: line_name(style).to_owned(), color }
    })
}

fn border(data: &[u8]) -> Option<Border> {
    let mut r = Reader::new(data);
    let flags = r.u8()?;
    let top = edge(&mut r)?;
    let bottom = edge(&mut r)?;
    let left = edge(&mut r)?;
    let right = edge(&mut r)?;
    let diagonal = edge(&mut r)?;
    Some(Border {
        left,
        right,
        top,
        bottom,
        diagonal,
        diagonal_down: flags & 1 != 0,
        diagonal_up: flags & 2 != 0,
    })
}

fn xf(data: &[u8]) -> Option<Xf> {
    let mut r = Reader::new(data);
    let parent = r.u16()?;
    let format = r.u16()?;
    let font = r.u16()?;
    let fill = r.u16()?;
    let border = r.u16()?;
    let rotation = r.u8()?;
    let indent = r.u8()?;
    let flags = r.u16()?;
    let horizontal = match flags & 7 {
        1 => Some("left"),
        2 => Some("center"),
        3 => Some("right"),
        4 => Some("fill"),
        5 => Some("justify"),
        6 => Some("centerContinuous"),
        7 => Some("distributed"),
        _ => None,
    };
    let vertical = match (flags >> 3) & 7 {
        0 => Some("top"),
        1 => Some("center"),
        3 => Some("justify"),
        4 => Some("distributed"),
        _ => None,
    };
    Some(Xf {
        num_fmt: format as u32,
        font: font as u32,
        fill: fill as u32,
        border: border as u32,
        xf_id: if parent == 0xFFFF { 0 } else { parent as u32 },
        align: Alignment {
            horizontal: horizontal.map(str::to_owned),
            vertical: vertical.map(str::to_owned),
            wrap: flags & 0x40 != 0,
            indent: indent as u32,
            rotation: rotation as i32,
            shrink: flags & 0x100 != 0,
            ..Default::default()
        },
        locked: (flags & 0x1000 == 0).then_some(false),
        hidden: (flags & 0x2000 != 0).then_some(true),
        quote_prefix: flags & 0x8000 != 0,
        ..Default::default()
    })
}

/// A differential format: a list of properties, each with a type and a size.
fn dxf(data: &[u8]) -> Option<Dxf> {
    let mut r = Reader::new(data);
    r.skip(4)?;
    let count = r.u16()?;
    let mut out = Dxf::default();
    let mut fore = None;
    let mut back = None;
    for _ in 0..count {
        let kind = r.u16()?;
        let size = r.u16()? as usize;
        let body = r.take(size.checked_sub(4)?)?;
        let mut p = Reader::new(body);
        match kind {
            0x01 => fore = color(&mut p),
            0x02 => back = color(&mut p),
            0x05 => out.font_color = color(&mut p),
            0x19 => out.bold = p.u16().map(|w| w >= 700),
            0x1B => {
                out.underline =
                    p.u16().map(|u| if u == 0 { Underline::None } else { Underline::Single })
            }
            0x1C => out.italic = p.u8().map(|v| v != 0),
            0x1D => out.strike = p.u8().map(|v| v != 0),
            0x26 => out.num_fmt = counted_string(&mut p),
            _ => {}
        }
    }
    out.fill = back.or(fore);
    Some(out)
}

/// A number format string in a differential format: a length in characters, then UTF-16.
fn counted_string(r: &mut Reader) -> Option<String> {
    let cch = r.u16()? as usize;
    r.chars(cch, true)
}

/// Read `xl/styles.bin` into the styles, leaving the theme to the caller.
pub fn read(bytes: &[u8], styles: &mut Styles) {
    let records = split(bytes);
    styles.fonts.clear();
    styles.fills.clear();
    styles.borders.clear();
    styles.xfs.clear();
    let mut section = 0u16;
    for Record { id: kind, data } in records {
        match kind {
            id::BEGIN_FMTS | id::BEGIN_FONTS | id::BEGIN_FILLS | id::BEGIN_BORDERS => {
                section = kind
            }
            id::BEGIN_CELL_XFS | id::BEGIN_CELL_STYLE_XFS | id::BEGIN_DXFS => section = kind,
            id::FMT => {
                let mut r = Reader::new(data);
                if let (Some(index), Some(code)) = (r.u16(), wide_string(&mut r)) {
                    styles.num_fmts.insert(index as u32, code);
                }
            }
            id::FONT => styles.fonts.extend(font(data)),
            id::FILL => styles.fills.extend(fill(data)),
            id::BORDER => styles.borders.extend(border(data)),
            id::XF if section == id::BEGIN_CELL_XFS => styles.xfs.extend(xf(data)),
            id::DXF => styles.dxfs.extend(dxf(data)),
            _ => {}
        }
    }
    if styles.fonts.is_empty() {
        styles.fonts.push(Font::default());
    }
    if styles.fills.len() < 2 {
        styles.fills =
            vec![Fill::none(), Fill { pattern: "gray125".to_owned(), ..Default::default() }];
    }
    if styles.borders.is_empty() {
        styles.borders.push(Border::default());
    }
    if styles.xfs.is_empty() {
        styles.xfs.push(Xf::default());
    }
}
