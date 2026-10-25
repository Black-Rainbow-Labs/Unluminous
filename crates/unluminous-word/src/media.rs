//! Pictures: putting one in a document the way Word does.
//!
//! The picture's bytes become a part under `word/media/`, the main part gets a relationship to it, its
//! extension gets a content type, and the run holds a `w:drawing` with a `wp:inline` frame of Word's own
//! shape, so Word opens it as a picture it made itself.

use std::sync::Arc;

use crate::model::*;
use crate::package::{kind, Package};
use crate::position::Selection;
use crate::session::{Scope, Session};
use crate::xml::El;

/// The content type of a picture by its extension.
pub fn content_type(extension: &str) -> &'static str {
    match extension {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        _ => "image/png",
    }
}

/// Insert a picture at the caret, `width` by `height` points.
pub fn insert_picture(
    session: &mut Session,
    bytes: &[u8],
    extension: &str,
    width: f32,
    height: f32,
    name: &str,
) -> Result<(), String> {
    let extension = if extension == "jpeg" { "jpg".to_owned() } else { extension.to_owned() };
    let bytes = bytes.to_vec();
    let name = name.to_owned();
    session.change(Scope::Whole, move |document, selection, _| {
        let drawing = add_picture(document, &bytes, &extension, width, height, &name);
        let at = selection.ordered().1.clone();
        let (props, wraps) = crate::position::paragraph(document, &at.story, &at.path)
            .map(|p| crate::edit::props_at(p, at.offset))
            .unwrap_or_else(|| (no_props(), Vec::new()));
        let Some(paragraph) = crate::position::paragraph_mut(document, &at.story, &at.path) else {
            return selection.clone();
        };
        let index = paragraph.split_at(at.offset);
        let run = Run {
            props: Arc::clone(&props),
            content: Content::Object(drawing),
            wraps,
            attrs: Arc::default(),
        };
        let len = run.len();
        paragraph.runs.insert(index, run);
        for mark in &mut paragraph.marks {
            if mark.offset > at.offset {
                mark.offset += len;
            }
        }
        Selection::caret(at.at(at.offset + len))
    });
    Ok(())
}

/// Put a picture's bytes in the package as a media part the main part points at, and answer the inline
/// `w:drawing` that shows it, `width` by `height` points.
pub fn add_picture(
    document: &mut Document,
    bytes: &[u8],
    extension: &str,
    width: f32,
    height: f32,
    name: &str,
) -> El {
    let part = document.package.unused_name("word/media/image", extension);
    document.package.set_part(&part, bytes.to_vec());
    document.package.ensure_default(extension, content_type(extension));
    let main = document.main.clone();
    let target = Package::relative(&main, &part);
    let id = document.package.rels_mut(&main).add(kind::IMAGE, &target, false);
    let shape = next_shape_id(document);
    drawing(&id, shape, name, width, height)
}

/// A picture's size in pixels, read from the header of a PNG, a GIF, a BMP or a JPEG.
pub fn picture_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let be32 =
        |at: usize| bytes.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let le16 = |at: usize| bytes.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32);
    let le32 = |at: usize| {
        bytes.get(at..at + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]).unsigned_abs())
    };
    if bytes.starts_with(b"\x89PNG") {
        return Some((be32(16)?, be32(20)?));
    }
    if bytes.starts_with(b"GIF8") {
        return Some((le16(6)?, le16(8)?));
    }
    if bytes.starts_with(b"BM") {
        return Some((le32(18)?, le32(22)?));
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut at = 2;
        while at + 9 < bytes.len() {
            if bytes[at] != 0xff {
                at += 1;
                continue;
            }
            let marker = bytes[at + 1];
            let length = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
            if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
                let height = u16::from_be_bytes([bytes[at + 5], bytes[at + 6]]) as u32;
                let width = u16::from_be_bytes([bytes[at + 7], bytes[at + 8]]) as u32;
                return Some((width, height));
            }
            at += 2 + length;
        }
    }
    None
}

/// A `wp:docPr` id no shape in the document has.
fn next_shape_id(document: &Document) -> u32 {
    let mut largest = 0u32;
    for (_, paragraph) in crate::position::paragraphs(&document.body) {
        for run in &paragraph.runs {
            if let Content::Object(el) = &run.content {
                el.walk(&mut |node| {
                    if node.name == "wp:docPr" {
                        if let Some(id) = node.attr("id").and_then(|id| id.parse::<u32>().ok()) {
                            largest = largest.max(id);
                        }
                    }
                });
            }
        }
    }
    largest + 1
}

/// The `w:drawing` Word writes for an inline picture.
pub fn drawing(rel: &str, id: u32, name: &str, width: f32, height: f32) -> El {
    let cx = ((width * 12_700.0).round() as i64).to_string();
    let cy = ((height * 12_700.0).round() as i64).to_string();
    let extent = || El::new("wp:extent").with("cx", cx.clone()).with("cy", cy.clone());
    let picture = El::new("pic:pic")
        .with_child(
            El::new("pic:nvPicPr")
                .with_child(El::new("pic:cNvPr").with("id", "0").with("name", name))
                .with_child(El::new("pic:cNvPicPr")),
        )
        .with_child(
            El::new("pic:blipFill")
                .with_child(El::new("a:blip").with("r:embed", rel))
                .with_child(El::new("a:stretch").with_child(El::new("a:fillRect"))),
        )
        .with_child(
            El::new("pic:spPr")
                .with_child(
                    El::new("a:xfrm")
                        .with_child(El::new("a:off").with("x", "0").with("y", "0"))
                        .with_child(El::new("a:ext").with("cx", cx.clone()).with("cy", cy.clone())),
                )
                .with_child(
                    El::new("a:prstGeom").with("prst", "rect").with_child(El::new("a:avLst")),
                ),
        );
    El::new("w:drawing").with_child(
        El::new("wp:inline")
            .with("distT", "0")
            .with("distB", "0")
            .with("distL", "0")
            .with("distR", "0")
            .with_child(extent())
            .with_child(
                El::new("wp:effectExtent")
                    .with("l", "0")
                    .with("t", "0")
                    .with("r", "0")
                    .with("b", "0"),
            )
            .with_child(
                El::new("wp:docPr")
                    .with("id", id.to_string())
                    .with("name", format!("Picture {id}"))
                    .with("descr", name),
            )
            .with_child(
                El::new("wp:cNvGraphicFramePr")
                    .with_child(El::new("a:graphicFrameLocks").with("noChangeAspect", "1")),
            )
            .with_child(
                El::new("a:graphic").with_child(
                    El::new("a:graphicData")
                        .with("uri", "http://schemas.openxmlformats.org/drawingml/2006/picture")
                        .with_child(picture),
                ),
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_becomes_a_part_a_relationship_and_an_inline_drawing() {
        let mut session = Session::new(crate::new::blank());
        session.type_text("see: ");
        let png = b"\x89PNG\r\n\x1a\nfake".to_vec();
        insert_picture(&mut session, &png, "png", 100.0, 50.0, "chart.png").unwrap();
        let again = crate::read::open(&crate::write::save(&session.document).unwrap()).unwrap();
        assert_eq!(again.package.part("word/media/image1.png"), Some(png.as_slice()));
        assert_eq!(again.package.content_type("word/media/image1.png"), Some("image/png"));
        let pages = crate::layout::lay_out(
            &again,
            &unluminous_core::FixedMetrics::default(),
            Default::default(),
        );
        let picture = pages.pages[0].items.iter().find_map(|i| match i {
            crate::layout::Item::Picture { rect, part, .. } => Some((rect.w, rect.h, part.clone())),
            _ => None,
        });
        assert_eq!(picture, Some((100.0, 50.0, "word/media/image1.png".to_owned())));
    }
}
