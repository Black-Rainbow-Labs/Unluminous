//! The laid out pages as a PDF file.
//!
//! What is drawn on screen is what goes in the file: every text item at its own place, every fill, rule
//! and picture. Each TrueType face is embedded whole (`FontFile2`) as a CID font with `Identity-H`, so any
//! character the font has prints, and a `ToUnicode` map makes the text searchable and copyable. A face
//! that is part of a font collection cannot be embedded as one file and is replaced by Helvetica, which
//! every PDF reader has.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write as _;

use ab_glyph::{Font, FontRef};
use unluminous_core::CharStyle;

use crate::layout::{Item, Pages, RuleKind};

/// The pixels of a picture, as a PDF wants them.
pub enum Picture {
    /// A JPEG file's bytes, which a PDF holds as they are.
    Jpeg { width: u32, height: u32, bytes: Vec<u8> },
    /// Eight bit RGB, three bytes a pixel, row by row.
    Rgb { width: u32, height: u32, pixels: Vec<u8> },
}

/// A font file's bytes and the face inside it.
pub type FaceBytes = (std::sync::Arc<Vec<u8>>, u32);

struct EmbeddedFont {
    name: String,
    bytes: Option<std::sync::Arc<Vec<u8>>>,
    index: u32,
    /// Glyph ids used, with the character each stands for and its advance in thousandths of an em.
    used: HashMap<u16, (char, f32)>,
}

/// Write the pages as a PDF. `face` answers the font file a style is drawn with, and `picture` the
/// pixels of a package part.
pub fn write(
    pages: &Pages,
    title: &str,
    face: &dyn Fn(&CharStyle) -> Option<FaceBytes>,
    picture: &dyn Fn(&str) -> Option<Picture>,
) -> Vec<u8> {
    let mut fonts: Vec<EmbeddedFont> = Vec::new();
    let mut font_of: HashMap<(String, bool, bool), usize> = HashMap::new();
    let mut images: Vec<(String, Picture)> = Vec::new();
    let mut image_of: HashMap<String, usize> = HashMap::new();
    let mut contents: Vec<String> = Vec::new();
    for page in &pages.pages {
        let mut c = String::new();
        let h = page.height;
        if let Some(bg) = page.background {
            let _ =
                writeln!(c, "{} {} {} rg 0 0 {} {} re f", f(bg.0), f(bg.1), f(bg.2), page.width, h);
        }
        for item in &page.items {
            match item {
                Item::Fill { rect, color } => {
                    let _ = writeln!(
                        c,
                        "{} {} {} rg {:.2} {:.2} {:.2} {:.2} re f",
                        f(color.0),
                        f(color.1),
                        f(color.2),
                        rect.x,
                        h - rect.y - rect.h,
                        rect.w,
                        rect.h
                    );
                }
                Item::Rule { x0, y0, x1, y1, width, color, kind } => {
                    let dash = match kind {
                        RuleKind::Dotted => "[1 1] 0 d",
                        RuleKind::Dashed => "[3 2] 0 d",
                        _ => "[] 0 d",
                    };
                    let _ = writeln!(
                        c,
                        "{} {} {} RG {:.2} w {dash} {:.2} {:.2} m {:.2} {:.2} l S",
                        f(color.0),
                        f(color.1),
                        f(color.2),
                        width.max(0.1),
                        x0,
                        h - y0,
                        x1,
                        h - y1
                    );
                    if *kind == RuleKind::Double {
                        let gap = width * 2.0;
                        let _ = writeln!(
                            c,
                            "{:.2} {:.2} m {:.2} {:.2} l S",
                            x0,
                            h - y0 - gap,
                            x1,
                            h - y1 - gap
                        );
                    }
                }
                Item::Picture { rect, part, crop } => {
                    let index = match image_of.get(part) {
                        Some(index) => Some(*index),
                        None => picture(part).map(|p| {
                            images.push((part.clone(), p));
                            image_of.insert(part.clone(), images.len() - 1);
                            images.len() - 1
                        }),
                    };
                    if let Some(index) = index {
                        // A cropped picture is drawn whole, larger than its box, and clipped to the box.
                        let (l, t, r, b) = crop.unwrap_or((0.0, 0.0, 0.0, 0.0));
                        let full_w = rect.w / (1.0 - l - r).max(0.01);
                        let full_h = rect.h / (1.0 - t - b).max(0.01);
                        let _ = writeln!(
                            c,
                            "q {:.2} {:.2} {:.2} {:.2} re W n {:.2} 0 0 {:.2} {:.2} {:.2} cm /Im{index} Do Q",
                            rect.x,
                            h - rect.y - rect.h,
                            rect.w,
                            rect.h,
                            full_w,
                            full_h,
                            rect.x - full_w * l,
                            h - rect.y - rect.h - full_h * b
                        );
                    }
                }
                Item::Shape { rect, preset, fill, outline, flip } => {
                    let points = crate::layout::drawing::outline_points(preset, *rect, *flip);
                    let open = points.len() == 2;
                    let mut path = String::new();
                    for (n, (x, y)) in points.iter().enumerate() {
                        let op = if n == 0 { "m" } else { "l" };
                        let _ = write!(path, "{x:.2} {:.2} {op} ", h - y);
                    }
                    if let (Some(color), false) = (fill, open) {
                        let _ = writeln!(
                            c,
                            "{} {} {} rg {path}h f",
                            f(color.0),
                            f(color.1),
                            f(color.2)
                        );
                    }
                    if let Some((color, width)) = outline {
                        let close = if open { "" } else { "h " };
                        let _ = writeln!(
                            c,
                            "{} {} {} RG {width:.2} w [] 0 d {path}{close}S",
                            f(color.0),
                            f(color.1),
                            f(color.2)
                        );
                    }
                }
                Item::Object { .. } => {}
                Item::Placeholder { rect, .. } => {
                    let _ = writeln!(
                        c,
                        "0.6 0.6 0.6 RG 0.5 w [] 0 d {:.2} {:.2} {:.2} {:.2} re S",
                        rect.x,
                        h - rect.y - rect.h,
                        rect.w,
                        rect.h
                    );
                }
                Item::Text(text) => {
                    let key = (text.style.family.to_string(), text.style.bold, text.style.italic);
                    let index = *font_of.entry(key).or_insert_with(|| {
                        let found = face(&text.style);
                        let collection = found
                            .as_ref()
                            .is_some_and(|(bytes, index)| *index > 0 || bytes.starts_with(b"ttcf"));
                        let (bytes, index) = match found {
                            Some((bytes, index))
                                if !collection && FontRef::try_from_slice(&bytes).is_ok() =>
                            {
                                (Some(bytes), index)
                            }
                            _ => (None, 0),
                        };
                        fonts.push(EmbeddedFont {
                            name: format!("F{}", fonts.len()),
                            bytes,
                            index,
                            used: HashMap::new(),
                        });
                        fonts.len() - 1
                    });
                    let font = &mut fonts[index];
                    let color = text.style.color;
                    let _ = write!(
                        c,
                        "BT /{} {:.2} Tf {} {} {} rg ",
                        font.name,
                        text.style.size,
                        f(color.r),
                        f(color.g),
                        f(color.b)
                    );
                    match &font.bytes {
                        Some(bytes) => {
                            let parsed = FontRef::try_from_slice_and_index(bytes, font.index)
                                .expect("checked above");
                            let units = parsed.units_per_em().unwrap_or(1000.0);
                            for (character, x) in text.text.chars().zip(&text.xs) {
                                if character.is_control() {
                                    continue;
                                }
                                let glyph = parsed.glyph_id(character);
                                let advance = parsed.h_advance_unscaled(glyph) / units * 1000.0;
                                font.used.insert(glyph.0, (character, advance));
                                let _ = write!(
                                    c,
                                    "1 0 0 1 {:.2} {:.2} Tm <{:04X}> Tj ",
                                    x,
                                    h - text.baseline,
                                    glyph.0
                                );
                            }
                        }
                        None => {
                            for (character, x) in text.text.chars().zip(&text.xs) {
                                let byte = if (character as u32) < 256 && !character.is_control() {
                                    character as u32 as u8
                                } else {
                                    b'?'
                                };
                                let _ = write!(
                                    c,
                                    "1 0 0 1 {:.2} {:.2} Tm <{:02X}> Tj ",
                                    x,
                                    h - text.baseline,
                                    byte
                                );
                            }
                        }
                    }
                    c.push_str("ET\n");
                }
            }
        }
        contents.push(c);
    }
    assemble(pages, title, &contents, &fonts, &images)
}

fn f(channel: u8) -> String {
    format!("{:.3}", f32::from(channel) / 255.0)
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
    let _ = encoder.write_all(bytes);
    encoder.finish().unwrap_or_default()
}

/// Lay the objects out, with the cross reference table.
fn assemble(
    pages: &Pages,
    title: &str,
    contents: &[String],
    fonts: &[EmbeddedFont],
    images: &[(String, Picture)],
) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    // Object numbers: 1 catalog, 2 pages, 3 info, then fonts, images, page objects and contents.
    let mut next = 4usize;
    let mut font_ids = Vec::new();
    for font in fonts {
        font_ids.push(next);
        next += if font.bytes.is_some() { 5 } else { 1 };
    }
    let image_ids: Vec<usize> = images
        .iter()
        .map(|_| {
            next += 1;
            next - 1
        })
        .collect();
    let page_ids: Vec<usize> = pages
        .pages
        .iter()
        .map(|_| {
            next += 2;
            next - 2
        })
        .collect();
    let mut objects: Vec<(usize, Vec<u8>)> = Vec::new();
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    objects.push((1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()));
    objects.push((
        2,
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), page_ids.len())
            .into_bytes(),
    ));
    objects.push((
        3,
        format!("<< /Title ({}) /Producer (Unluminous) >>", pdf_string(title)).into_bytes(),
    ));
    for (font, id) in fonts.iter().zip(&font_ids) {
        match &font.bytes {
            None => objects.push((*id, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec())),
            Some(bytes) => {
                let parsed = FontRef::try_from_slice_and_index(bytes, font.index).expect("checked");
                let units = parsed.units_per_em().unwrap_or(1000.0);
                let scale = |v: f32| (v / units * 1000.0).round() as i64;
                let base = format!("Unluminous{}+{}", id, font.name);
                let (descendant, descriptor, file, cmap) = (id + 1, id + 2, id + 3, id + 4);
                objects.push((*id, format!("<< /Type /Font /Subtype /Type0 /BaseFont /{base} /Encoding /Identity-H /DescendantFonts [{descendant} 0 R] /ToUnicode {cmap} 0 R >>").into_bytes()));
                let mut widths: Vec<(&u16, &(char, f32))> = font.used.iter().collect();
                widths.sort_by_key(|(gid, _)| **gid);
                let w: String = widths.iter().map(|(gid, (_, adv))| format!("{gid} [{}]", adv.round() as i64)).collect::<Vec<_>>().join(" ");
                objects.push((descendant, format!("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{base} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /W [{w}] /CIDToGIDMap /Identity >>").into_bytes()));
                objects.push((descriptor, format!(
                    "<< /Type /FontDescriptor /FontName /{base} /Flags 32 /FontBBox [-500 {} 1500 {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile2 {file} 0 R >>",
                    scale(parsed.descent_unscaled()), scale(parsed.ascent_unscaled()), scale(parsed.ascent_unscaled()), scale(parsed.descent_unscaled()), scale(parsed.ascent_unscaled() * 0.7)
                ).into_bytes()));
                let packed = deflate(bytes);
                let mut stream = format!("<< /Length {} /Length1 {} /Filter /FlateDecode >>\nstream\n", packed.len(), bytes.len()).into_bytes();
                stream.extend(&packed);
                stream.extend(b"\nendstream");
                objects.push((file, stream));
                let mut map = String::from("/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Adobe-Identity-UCS def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange\n");
                for chunk in widths.chunks(100) {
                    let _ = writeln!(map, "{} beginbfchar", chunk.len());
                    for (gid, (character, _)) in chunk {
                        let mut units = [0u16; 2];
                        let encoded: String = character.encode_utf16(&mut units).iter().map(|u| format!("{u:04X}")).collect();
                        let _ = writeln!(map, "<{gid:04X}> <{encoded}>");
                    }
                    map.push_str("endbfchar\n");
                }
                map.push_str("endcmap CMapName currentdict /CMap defineresource pop end end");
                let packed = deflate(map.as_bytes());
                let mut stream = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", packed.len()).into_bytes();
                stream.extend(&packed);
                stream.extend(b"\nendstream");
                objects.push((cmap, stream));
            }
        }
    }
    for ((_, picture), id) in images.iter().zip(&image_ids) {
        let mut stream = match picture {
            Picture::Jpeg { width, height, bytes } => {
                let mut s = format!("<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", bytes.len()).into_bytes();
                s.extend(bytes);
                s
            }
            Picture::Rgb { width, height, pixels } => {
                let packed = deflate(pixels);
                let mut s = format!("<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", packed.len()).into_bytes();
                s.extend(packed);
                s
            }
        };
        stream.extend(b"\nendstream");
        objects.push((*id, stream));
    }
    let font_resources: String = fonts
        .iter()
        .zip(&font_ids)
        .map(|(font, id)| format!("/{} {id} 0 R", font.name))
        .collect::<Vec<_>>()
        .join(" ");
    let image_resources: String = image_ids
        .iter()
        .enumerate()
        .map(|(i, id)| format!("/Im{i} {id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    for ((page, id), content) in pages.pages.iter().zip(&page_ids).zip(contents) {
        objects.push((*id, format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /Font << {font_resources} >> /XObject << {image_resources} >> >> /Contents {} 0 R >>",
            page.width, page.height, id + 1
        ).into_bytes()));
        let packed = deflate(content.as_bytes());
        let mut stream =
            format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", packed.len()).into_bytes();
        stream.extend(packed);
        stream.extend(b"\nendstream");
        objects.push((id + 1, stream));
    }
    objects.sort_by_key(|(id, _)| *id);
    offsets.resize(next, 0);
    for (id, body) in &objects {
        offsets[*id] = out.len();
        out.extend(format!("{id} 0 obj\n").as_bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {next}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets.iter().skip(1) {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!("trailer\n<< /Size {next} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n")
            .as_bytes(),
    );
    out
}

fn pdf_string(text: &str) -> String {
    text.chars()
        .filter(|c| (*c as u32) < 128 && !c.is_control())
        .map(|c| match c {
            '(' => "\\(".to_owned(),
            ')' => "\\)".to_owned(),
            '\\' => "\\\\".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_becomes_a_pdf_with_a_page_for_each_page() {
        let mut session = crate::session::Session::new(crate::new::blank());
        session.type_text("Hello (world)\u{c}second page");
        let pages = crate::layout::lay_out(
            &session.document,
            &unluminous_core::FixedMetrics::default(),
            Default::default(),
        );
        let pdf = write(&pages, "A (title)", &|_| None, &|_| None);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.7"));
        assert!(text.contains("/Count 2"));
        assert!(text.contains("/Helvetica"));
        assert!(text.trim_end().ends_with("%%EOF"));
        // Every object the cross reference table names starts where it says.
        let xref_at: usize =
            text.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        let table = &text[xref_at..];
        for (line, id) in table.lines().skip(3).take_while(|l| l.ends_with(" n ")).zip(1..) {
            let offset: usize = line[..10].parse().unwrap();
            assert!(pdf[offset..].starts_with(format!("{id} 0 obj").as_bytes()), "object {id}");
        }
    }
}
