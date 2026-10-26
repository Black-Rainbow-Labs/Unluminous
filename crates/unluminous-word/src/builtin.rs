//! Word's built in styles and lists, for a document that has not used them yet.
//!
//! A style is copied from a document Word made with every built in style applied
//! (`templates/builtin-styles.docx`, made by `tools/word-oracle/make-fixtures.ps1`), together with the
//! styles it is based on and links to, so applying `Heading 2` adds exactly what Word would have added.
//! The bullet and numbered lists are Word's own default definitions: a bullet of the Symbol font's
//! U+F0B7 at each level, and `1.`, `a.`, `i.` repeating, each level indented half an inch further.

use crate::model::{Document, XmlPart};
use crate::package::{kind, Package};
use crate::xml::{El, Node};

const BUILTIN: &[u8] = include_bytes!("../templates/builtin-styles.docx");

/// The styles part of the built in template.
fn template_styles() -> El {
    let document =
        crate::read::open(BUILTIN).expect("the built in styles template is a document Word wrote");
    document.styles.expect("the template has styles").root
}

/// Word's own definition of a built in style, from the template, as a `w:style` element.
pub fn definition(id: &str) -> Option<El> {
    template_styles().children_named("w:style").find(|s| s.attr("w:styleId") == Some(id)).cloned()
}

/// Make sure the style with this id is defined, copying it from Word's own definition when it is not.
pub fn ensure_style(document: &mut Document, id: &str) -> Result<(), String> {
    if document.styles.as_ref().is_some_and(|part| {
        part.root.children_named("w:style").any(|s| s.attr("w:styleId") == Some(id))
    }) {
        return Ok(());
    }
    let template = template_styles();
    let find = |id: &str| {
        template.children_named("w:style").find(|s| s.attr("w:styleId") == Some(id)).cloned()
    };
    let Some(_) = find(id) else {
        return Err(format!(
            "there is no style `{id}` in this document or among Word's built in styles"
        ));
    };
    ensure_styles_part(document);
    let mut wanted = vec![id.to_owned()];
    while let Some(next) = wanted.pop() {
        let present = document.styles.as_ref().is_some_and(|p| {
            p.root.children_named("w:style").any(|s| s.attr("w:styleId") == Some(next.as_str()))
        });
        if present {
            continue;
        }
        let Some(mut style) = find(&next) else { continue };
        for link in ["w:basedOn", "w:link", "w:next"] {
            if let Some(other) = style.val_of(link) {
                wanted.push(other.to_owned());
            }
        }
        // A list style (List Bullet, List Number) names a numbering definition in the template's own
        // numbering part; that definition comes with it, under a number this document has free.
        let template_num = style
            .child("w:pPr")
            .and_then(|p| p.child("w:numPr"))
            .and_then(|n| n.val_of("w:numId"))
            .and_then(|v| v.parse::<i64>().ok());
        if let Some(num) = template_num {
            if let Some(new_num) = copy_numbering(document, num) {
                if let Some(num_pr) = style.child_mut("w:pPr").and_then(|p| p.child_mut("w:numPr"))
                {
                    num_pr.put(El::new("w:numId").with("w:val", new_num.to_string()));
                }
            }
        }
        let part = document.styles.as_mut().expect("made above");
        part.root.children.push(Node::El(style));
        part.dirty = true;
    }
    Ok(())
}

/// Copy the template's numbering definition `num` (its `w:num` and the `w:abstractNum` it points at)
/// into the document, numbered after the document's own. Answers the new `w:numId`.
fn copy_numbering(document: &mut Document, num: i64) -> Option<i64> {
    let template = crate::read::open(BUILTIN).ok()?;
    let source = template.numbering.as_ref()?;
    let num_el = source
        .root
        .children_named("w:num")
        .find(|n| n.attr("w:numId") == Some(&num.to_string()))?
        .clone();
    let abstract_id = num_el.val_of("w:abstractNumId")?.to_owned();
    let mut abstract_el = source
        .root
        .children_named("w:abstractNum")
        .find(|a| a.attr("w:abstractNumId") == Some(abstract_id.as_str()))?
        .clone();
    let part = numbering(document);
    let next_abstract = part
        .root
        .children_named("w:abstractNum")
        .filter_map(|a| a.attr("w:abstractNumId").and_then(|v| v.parse::<i64>().ok()))
        .max()
        .map_or(0, |n| n + 1);
    let next_num = part
        .root
        .children_named("w:num")
        .filter_map(|n| n.attr("w:numId").and_then(|v| v.parse::<i64>().ok()))
        .max()
        .map_or(1, |n| n + 1);
    abstract_el.set_attr("w:abstractNumId", next_abstract.to_string());
    let mut new_num = El::new("w:num").with("w:numId", next_num.to_string());
    new_num
        .children
        .push(Node::El(El::new("w:abstractNumId").with("w:val", next_abstract.to_string())));
    // Every w:abstractNum comes before every w:num in numbering.xml.
    let first_num =
        part.root.children.iter().position(|n| matches!(n, Node::El(el) if el.name == "w:num"));
    match first_num {
        Some(at) => part.root.children.insert(at, Node::El(abstract_el)),
        None => part.root.children.push(Node::El(abstract_el)),
    }
    part.root.children.push(Node::El(new_num));
    part.dirty = true;
    Some(next_num)
}

/// A styles part, made from the template's when the document has none.
fn ensure_styles_part(document: &mut Document) {
    if document.styles.is_some() {
        return;
    }
    let mut root = template_styles();
    root.children.retain(|node| !matches!(node, Node::El(el) if el.name == "w:style"));
    let name = "word/styles.xml".to_owned();
    add_part(
        document,
        &name,
        kind::STYLES,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
    );
    document.styles = Some(XmlPart { name, root, dirty: true });
}

/// Register a new part the main part points at.
fn add_part(document: &mut Document, name: &str, rel_kind: &str, content_type: &str) {
    let main = document.main.clone();
    let target = Package::relative(&main, name);
    document.package.rels_mut(&main).add(rel_kind, &target, false);
    document.package.set_override(name, content_type);
}

/// The numbering part, made when the document has none.
fn numbering(document: &mut Document) -> &mut XmlPart {
    if document.numbering.is_none() {
        let name = "word/numbering.xml".to_owned();
        add_part(
            document,
            &name,
            kind::NUMBERING,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
        );
        let root = El::new("w:numbering").with("mc:Ignorable", "w14 w15 wp14");
        document.package.set_part(&name, Vec::new());
        document.numbering = Some(XmlPart { name, root, dirty: true });
    }
    document.numbering.as_mut().expect("made above")
}

/// What kind of list to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Bullet,
    Number,
}

/// Word's default definition of one level of a list.
fn level(kind: ListKind, level: u8) -> El {
    let indent = 720 * (i64::from(level) + 1);
    let mut lvl = El::new("w:lvl").with("w:ilvl", level.to_string());
    lvl.put(El::new("w:start").with("w:val", "1"));
    let (format, text) = match kind {
        ListKind::Bullet => (
            "bullet",
            match level % 3 {
                0 => "\u{F0B7}".to_owned(),
                1 => "o".to_owned(),
                _ => "\u{F0A7}".to_owned(),
            },
        ),
        ListKind::Number => (
            match level % 3 {
                0 => "decimal",
                1 => "lowerLetter",
                _ => "lowerRoman",
            },
            format!("%{}.", level + 1),
        ),
    };
    lvl.put(El::new("w:numFmt").with("w:val", format));
    lvl.put(El::new("w:lvlText").with("w:val", text));
    lvl.put(
        El::new("w:lvlJc").with(
            "w:val",
            if kind == ListKind::Number && level % 3 == 2 { "right" } else { "left" },
        ),
    );
    lvl.put(
        El::new("w:pPr").with_child(
            El::new("w:ind").with("w:left", indent.to_string()).with("w:hanging", "360"),
        ),
    );
    if kind == ListKind::Bullet {
        let font = match level % 3 {
            0 => "Symbol",
            1 => "Courier New",
            _ => "Wingdings",
        };
        lvl.put(
            El::new("w:rPr").with_child(
                El::new("w:rFonts")
                    .with("w:ascii", font)
                    .with("w:hAnsi", font)
                    .with("w:hint", "default"),
            ),
        );
    }
    lvl
}

/// Add a new list of `kind` and answer its `w:numId`.
pub fn new_list(document: &mut Document, kind: ListKind) -> i64 {
    let part = numbering(document);
    let abstract_id = part
        .root
        .children_named("w:abstractNum")
        .filter_map(|el| el.attr("w:abstractNumId")?.parse::<i64>().ok())
        .max()
        .map_or(0, |n| n + 1);
    let num_id = part
        .root
        .children_named("w:num")
        .filter_map(|el| el.attr("w:numId")?.parse::<i64>().ok())
        .max()
        .map_or(1, |n| n + 1);
    let mut abstract_num =
        El::new("w:abstractNum").with("w:abstractNumId", abstract_id.to_string());
    abstract_num.put(El::new("w:multiLevelType").with("w:val", "hybridMultilevel"));
    for n in 0..9 {
        abstract_num.children.push(Node::El(level(kind, n)));
    }
    // Every `w:abstractNum` comes before every `w:num` in the schema.
    let first_num = part
        .root
        .children
        .iter()
        .position(|node| matches!(node, Node::El(el) if el.name == "w:num"));
    match first_num {
        Some(at) => part.root.children.insert(at, Node::El(abstract_num)),
        None => part.root.children.push(Node::El(abstract_num)),
    }
    let num = El::new("w:num")
        .with("w:numId", num_id.to_string())
        .with_child(El::new("w:abstractNumId").with("w:val", abstract_id.to_string()));
    let last_num = part
        .root
        .children
        .iter()
        .rposition(|node| matches!(node, Node::El(el) if el.name == "w:num"));
    // `w:numIdMacAtCleanup` and anything else after the lists stays last.
    match last_num {
        Some(at) => part.root.children.insert(at + 1, Node::El(num)),
        None => {
            let after = part.root.children.iter().position(|node| matches!(node, Node::El(el) if el.name != "w:abstractNum" && el.name != "w:num")).unwrap_or(part.root.children.len());
            part.root.children.insert(after, Node::El(num));
        }
    }
    part.dirty = true;
    num_id
}

/// Which kind a list is, from its first level's format.
pub fn kind_of(document: &Document, num: i64) -> Option<ListKind> {
    let root = document.numbering.as_ref().map(|p| &p.root)?;
    let numbering = crate::numbering::Numbering::new(Some(root));
    let format = numbering.level(num, 0)?.val_of("w:numFmt").unwrap_or("decimal").to_owned();
    Some(if format == "bullet" { ListKind::Bullet } else { ListKind::Number })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_style_brings_its_numbering() {
        let mut document = crate::new::blank();
        ensure_style(&mut document, "ListBullet").unwrap();
        let resolver = crate::styles::Resolver::new(&document);
        let ppr = El::new("w:pPr").with_child(El::new("w:pStyle").with("w:val", "ListBullet"));
        let look = resolver.paragraph(&ppr, None);
        let (num, level) = look.numbering.expect("List Bullet is a list");
        assert!(
            resolver.numbering.level(num, level).is_some(),
            "its numbering definition is in the document"
        );
    }

    #[test]
    fn a_style_the_document_lacks_comes_with_the_styles_it_needs() {
        let mut document = crate::new::blank();
        if let Some(part) = document.styles.as_mut() {
            part.root
                .children
                .retain(|n| !matches!(n, Node::El(el) if el.attr("w:styleId") == Some("Heading2")));
        }
        ensure_style(&mut document, "Heading2").unwrap();
        let resolver = crate::styles::Resolver::new(&document);
        assert!(resolver.style("Heading2").is_some());
        assert!(ensure_style(&mut document, "NoSuchStyle").is_err());
    }

    #[test]
    fn a_new_list_numbers_its_paragraphs() {
        let mut document = crate::new::blank();
        let num = new_list(&mut document, ListKind::Number);
        let root = document.numbering.as_ref().unwrap().root.clone();
        let numbering = crate::numbering::Numbering::new(Some(&root));
        let mut counters = crate::numbering::Counters::default();
        assert_eq!(counters.next(&numbering, num, 0).unwrap().text, "1.");
        assert_eq!(counters.next(&numbering, num, 1).unwrap().text, "a.");
        assert_eq!(kind_of(&document, num), Some(ListKind::Number));
        let bullets = new_list(&mut document, ListKind::Bullet);
        assert_ne!(bullets, num);
        // Written and read back, the numbering part is still there and still says the same.
        let again = crate::read::open(&crate::write::save(&document).unwrap()).unwrap();
        assert_eq!(kind_of(&again, bullets), Some(ListKind::Bullet));
    }
}
