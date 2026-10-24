//! A new, empty document.
//!
//! **The template is a document Word itself made** (`templates/blank.docx`, from `Documents.Add` in Word 16
//! through `tools/word-oracle/make-fixtures.ps1`), so a new Unluminous document has exactly the styles,
//! theme, settings and font table a new Word document has, and nothing Unluminous made up about them.

use crate::model::Document;
use crate::xml::{El, Node};

const BLANK: &[u8] = include_bytes!("../templates/blank.docx");

/// A blank document, with no author and no dates yet.
pub fn blank() -> Document {
    let mut document =
        crate::read::open(BLANK).expect("the blank template is a document Word wrote");
    set_property(&mut document, "dc:creator", "");
    set_property(&mut document, "cp:lastModifiedBy", "");
    document
}

/// Set one core property (`dc:title`, `dc:creator`, `cp:keywords`, `dcterms:modified`...), making it
/// when it is missing.
pub fn set_property(document: &mut Document, name: &str, value: &str) {
    let Some(core) = document.core.as_mut() else { return };
    let el = match core.root.child_mut(name) {
        Some(el) => el,
        None => {
            let mut el = El::new(name);
            if name.starts_with("dcterms:") {
                el.set_attr("xsi:type", "dcterms:W3CDTF");
            }
            core.root.children.push(Node::El(el));
            core.root.child_mut(name).expect("just added")
        }
    };
    el.children = if value.is_empty() { Vec::new() } else { vec![Node::Text(value.to_owned())] };
    core.dirty = true;
}

/// One core property's text.
pub fn property(document: &Document, name: &str) -> Option<String> {
    document.core.as_ref()?.root.child(name).map(El::text).filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_document_has_one_empty_paragraph_and_words_styles() {
        let document = blank();
        assert_eq!(document.body.len(), 1);
        let resolver = crate::styles::Resolver::new(&document);
        assert!(resolver.style("Normal").is_some());
        assert!(resolver.style("Heading1").is_some());
        assert_eq!(property(&document, "dc:creator"), None);
    }

    #[test]
    fn a_property_set_is_written_and_read_back() {
        let mut document = blank();
        set_property(&mut document, "dc:title", "Plans & schemes");
        let again = crate::read::open(&crate::write::save(&document).unwrap()).unwrap();
        assert_eq!(property(&again, "dc:title").as_deref(), Some("Plans & schemes"));
    }
}
