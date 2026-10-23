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
pub fn insert_picture(session: &mut Session, bytes: &[u8], extension: &str, width: f32, height: f32, name: &str) -> Result<(), String> {
    let extension = if extension == "jpeg" { "jpg".to_owned() } else { extension.to_owned() };
    let bytes = bytes.to_vec();
    let name = name.to_owned();
    session.change(Scope::Whole, move |document, selection, _| {
        let part = document.package.unused_name("word/media/image", &extension);
        document.package.set_part(&part, bytes.clone());
        document.package.ensure_default(&extension, content_type(&extension));
        let main = document.main.clone();
        let target = Package::relative(&main, &part);
        let id = document.package.rels_mut(&main).add(kind::IMAGE, &target, false);
        let shape = next_shape_id(document);
        let drawing = drawing(&id, shape, &name, width, height);
        let at = selection.ordered().1.clone();
        let (props, wraps) = crate::position::paragraph(document, &at.story, &at.path).map(|p| crate::edit::props_at(p, at.offset)).unwrap_or_else(|| (no_props(), Vec::new()));
        let Some(paragraph) = crate::position::paragraph_mut(document, &at.story, &at.path) else { return selection.clone() };
        let index = paragraph.split_at(at.offset);
        let run = Run { props: Arc::clone(&props), content: Content::Object(drawing), wraps, attrs: Arc::default() };
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
                .with_child(El::new("a:prstGeom").with("prst", "rect").with_child(El::new("a:avLst"))),
        );
    El::new("w:drawing").with_child(
        El::new("wp:inline")
            .with("distT", "0")
            .with("distB", "0")
            .with("distL", "0")
            .with("distR", "0")
            .with_child(extent())
            .with_child(El::new("wp:effectExtent").with("l", "0").with("t", "0").with("r", "0").with("b", "0"))
            .with_child(El::new("wp:docPr").with("id", id.to_string()).with("name", format!("Picture {id}")).with("descr", name))
            .with_child(
                El::new("wp:cNvGraphicFramePr")
                    .with_child(El::new("a:graphicFrameLocks").with("noChangeAspect", "1")),
            )
            .with_child(
                El::new("a:graphic").with_child(
                    El::new("a:graphicData").with("uri", "http://schemas.openxmlformats.org/drawingml/2006/picture").with_child(picture),
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
        let pages = crate::layout::lay_out(&again, &unluminous_core::FixedMetrics::default(), Default::default());
        let picture = pages.pages[0].items.iter().find_map(|i| match i {
            crate::layout::Item::Picture { rect, part } => Some((rect.w, rect.h, part.clone())),
            _ => None,
        });
        assert_eq!(picture, Some((100.0, 50.0, "word/media/image1.png".to_owned())));
    }
}
