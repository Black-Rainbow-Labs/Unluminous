//! Reading a `.docx` file into a [`Document`].
//!
//! The main part is found the way the packaging conventions say, through the package's own
//! relationships, and the parts it names are found through its relationships in turn. Nothing is assumed
//! about where a part lives or what it is called.

use std::sync::Arc;

use crate::model::*;
use crate::package::{kind, Package};
use crate::xml::{self, El, Node};

/// Read a document from the bytes of a `.docx`, `.dotx` or `.docm` file.
pub fn open(bytes: &[u8]) -> Result<Document, String> {
    let entries = crate::zip::read(bytes)?;
    let package = Package::read(entries)?;
    let main = package
        .rels
        .get("")
        .and_then(|rels| rels.of_kind(kind::OFFICE_DOCUMENT).next())
        .map(|rel| Package::resolve("", &rel.target))
        .ok_or("the file names no main document part, so it is not a Word document")?;
    let text = package.part(&main).ok_or_else(|| format!("the main part {main} is missing"))?;
    let root = xml::parse_bytes(text)?;
    if root.local() != "document" {
        return Err(format!(
            "the main part is a {} rather than a Word document; this may be a workbook or a presentation",
            root.local()
        ));
    }
    let mut document = Document {
        body: Vec::new(),
        section: El::new("w:sectPr"),
        root: shell(&root),
        background: root.child("w:background").cloned(),
        main: main.clone(),
        styles: None,
        numbering: None,
        settings: None,
        core: None,
        headers: Vec::new(),
        footnotes: None,
        endnotes: None,
        comments: None,
        ids: Arc::new(std::sync::atomic::AtomicI64::new(1)),
        textboxes: Vec::new(),
        package,
    };
    let largest = document
        .package
        .parts
        .iter()
        .filter(|part| part.name.ends_with(".xml"))
        .map(|part| largest_id(&part.data))
        .max()
        .unwrap_or(0);
    document.ids = Arc::new(std::sync::atomic::AtomicI64::new(largest + 1));
    if let Some(body) = root.child("w:body") {
        let (blocks, section) = blocks_with_section(body);
        document.body = blocks;
        if let Some(section) = section {
            document.section = section;
        }
    }
    read_related_parts(&mut document)?;
    document.textboxes = textboxes(&document);
    Ok(document)
}

/// Every text box in the body, the headers and the notes, with its content read as blocks.
fn textboxes(document: &Document) -> Vec<TextBox> {
    let mut out: Vec<TextBox> = Vec::new();
    let mut stories: Vec<&[Block]> = vec![&document.body];
    stories.extend(document.headers.iter().map(|h| h.story.blocks.as_slice()));
    for part in [&document.footnotes, &document.endnotes].into_iter().flatten() {
        stories.extend(part.notes.iter().map(|n| n.blocks.as_slice()));
    }
    for blocks in stories {
        for (_, paragraph) in crate::position::paragraphs(blocks) {
            for run in &paragraph.runs {
                if let Content::Object(el) = &run.content {
                    find_textboxes(el, &mut out);
                }
            }
        }
    }
    out
}

/// The first element named `name` under `el`, in document order.
pub fn descendant<'a>(el: &'a El, name: &str) -> Option<&'a El> {
    for child in el.elements() {
        if child.name == name {
            return Some(child);
        }
        if let Some(found) = descendant(child, name) {
            return Some(found);
        }
    }
    None
}

/// The text boxes inside a drawing: each `wp:docPr` with a `w:txbxContent` under its graphic. A drawing
/// in `mc:AlternateContent` has the same box twice, as a DrawingML shape and as its VML fallback; the
/// first one read is the one kept.
fn find_textboxes(el: &El, out: &mut Vec<TextBox>) {
    if el.name == "wp:anchor" || el.name == "wp:inline" {
        let id =
            el.child("wp:docPr").and_then(|d| d.attr("id")).and_then(|i| i.parse::<i64>().ok());
        if let (Some(id), Some(content)) = (id, descendant(el, "w:txbxContent")) {
            if !out.iter().any(|t| t.id == id) {
                out.push(TextBox { id, blocks: blocks(content), dirty: false });
            }
        }
        return;
    }
    for child in el.elements() {
        find_textboxes(child, out);
    }
}

/// The parts the main part points at.
fn read_related_parts(document: &mut Document) -> Result<(), String> {
    let main = document.main.clone();
    let rels = document.package.rels.get(&main).cloned().unwrap_or_default();
    let xml_part = |document: &Document, kind: &str| -> Result<Option<XmlPart>, String> {
        let Some(rel) = rels.of_kind(kind).next() else { return Ok(None) };
        let name = Package::resolve(&main, &rel.target);
        match document.package.part(&name) {
            Some(bytes) => Ok(Some(XmlPart { name, root: xml::parse_bytes(bytes)?, dirty: false })),
            None => Ok(None),
        }
    };
    document.styles = xml_part(document, kind::STYLES)?;
    document.numbering = xml_part(document, kind::NUMBERING)?;
    document.settings = xml_part(document, kind::SETTINGS)?;
    document.footnotes = notes_part(document, &rels, kind::FOOTNOTES)?;
    document.endnotes = notes_part(document, &rels, kind::ENDNOTES)?;
    document.comments = notes_part(document, &rels, kind::COMMENTS)?;
    for rel in rels.rels.iter().filter(|rel| rel.is(kind::HEADER) || rel.is(kind::FOOTER)) {
        let name = Package::resolve(&main, &rel.target);
        let Some(bytes) = document.package.part(&name) else { continue };
        let root = xml::parse_bytes(bytes)?;
        let story = Story { el: shell(&root), blocks: blocks(&root) };
        document.headers.push(HeaderPart {
            id: rel.id.clone(),
            name,
            footer: rel.is(kind::FOOTER),
            story,
            dirty: false,
        });
    }
    let core =
        document.package.rels.get("").and_then(|rels| rels.of_kind(kind::CORE).next().cloned());
    if let Some(rel) = core {
        let name = Package::resolve("", &rel.target);
        if let Some(bytes) = document.package.part(&name) {
            document.core = Some(XmlPart { name, root: xml::parse_bytes(bytes)?, dirty: false });
        }
    }
    Ok(())
}

/// The footnotes, endnotes or comments part.
fn notes_part(
    document: &Document,
    rels: &crate::package::Rels,
    kind: &str,
) -> Result<Option<NotesPart>, String> {
    let Some(rel) = rels.of_kind(kind).next() else { return Ok(None) };
    let name = Package::resolve(&document.main, &rel.target);
    let Some(bytes) = document.package.part(&name) else { return Ok(None) };
    let root = xml::parse_bytes(bytes)?;
    let notes =
        root.elements().map(|note| Story { el: shell(note), blocks: blocks(note) }).collect();
    Ok(Some(NotesPart { name, root: shell(&root), notes, dirty: false }))
}

/// An element with its attributes and declarations and no children.
fn shell(el: &El) -> El {
    El {
        name: el.name.clone(),
        attrs: el.attrs.clone(),
        children: Vec::new(),
        declares: el.declares.clone(),
    }
}

/// The blocks inside a container: a body, a cell, a header, a note, a content control's content.
pub fn blocks(container: &El) -> Vec<Block> {
    blocks_with_section(container).0
}

/// The blocks inside a container, and its own `w:sectPr` when it ends with one, as a body does.
fn blocks_with_section(container: &El) -> (Vec<Block>, Option<El>) {
    let mut out = Vec::new();
    let mut section = None;
    for el in container.elements() {
        match el.name.as_str() {
            "w:p" => out.push(Block::Paragraph(paragraph(el))),
            "w:tbl" => out.push(Block::Table(table(el))),
            "w:sdt" => match el.child("w:sdtContent") {
                Some(content) => {
                    let mut shell = shell(el);
                    shell.children = el
                        .elements()
                        .filter(|child| child.name != "w:sdtContent")
                        .cloned()
                        .map(Node::El)
                        .collect();
                    out.push(Block::Sdt(Sdt { el: shell, blocks: blocks(content) }));
                }
                None => out.push(Block::Kept(el.clone())),
            },
            "w:sectPr" => section = Some(el.clone()),
            // A cell's own properties, which `cell` reads.
            "w:tcPr" => {}
            _ => out.push(Block::Kept(el.clone())),
        }
    }
    (out, section)
}

/// A table.
fn table(el: &El) -> Table {
    let mut table =
        Table { props: El::new("w:tblPr"), grid: El::new("w:tblGrid"), ..Table::default() };
    for child in el.elements() {
        match child.name.as_str() {
            "w:tblPr" => table.props = child.clone(),
            "w:tblGrid" => table.grid = child.clone(),
            "w:tr" => table.rows.push(row(child)),
            _ => table.extra.push((table.rows.len(), child.clone())),
        }
    }
    table
}

fn row(el: &El) -> Row {
    let mut row = Row { attrs: el.attrs.clone(), props: El::new("w:trPr"), ..Row::default() };
    for child in el.elements() {
        match child.name.as_str() {
            "w:tblPrEx" => row.exceptions = Some(child.clone()),
            "w:trPr" => row.props = child.clone(),
            "w:tc" => row.cells.push(cell(child)),
            _ => row.extra.push((row.cells.len(), child.clone())),
        }
    }
    row
}

fn cell(el: &El) -> Cell {
    Cell {
        attrs: el.attrs.clone(),
        props: el.child("w:tcPr").cloned().unwrap_or_else(|| El::new("w:tcPr")),
        blocks: blocks(el),
    }
}

/// The elements that wrap runs, which become a run's wraps.
const WRAPPERS: &[&str] = &[
    "w:hyperlink",
    "w:ins",
    "w:del",
    "w:moveFrom",
    "w:moveTo",
    "w:fldSimple",
    "w:smartTag",
    "w:customXml",
    "w:dir",
    "w:bdo",
    "w:sdt",
];

/// The elements that take no room in the text, which become marks.
const MARKS: &[&str] = &[
    "w:bookmarkStart",
    "w:bookmarkEnd",
    "w:commentRangeStart",
    "w:commentRangeEnd",
    "w:permStart",
    "w:permEnd",
    "w:moveFromRangeStart",
    "w:moveFromRangeEnd",
    "w:moveToRangeStart",
    "w:moveToRangeEnd",
    "w:customXmlInsRangeStart",
    "w:customXmlInsRangeEnd",
    "w:customXmlDelRangeStart",
    "w:customXmlDelRangeEnd",
    "w:customXmlMoveFromRangeStart",
    "w:customXmlMoveFromRangeEnd",
    "w:customXmlMoveToRangeStart",
    "w:customXmlMoveToRangeEnd",
];

/// A paragraph.
pub fn paragraph(el: &El) -> Paragraph {
    let mut paragraph = Paragraph { attrs: el.attrs.clone(), ..Paragraph::new() };
    for child in el.elements() {
        if child.name == "w:pPr" {
            paragraph.props = child.clone();
        } else {
            inline(child, &[], &mut paragraph);
        }
    }
    paragraph
}

/// One child of a paragraph or of an element that wraps runs.
fn inline(el: &El, wraps: &[Arc<Wrap>], paragraph: &mut Paragraph) {
    let name = el.name.as_str();
    if name == "w:r" {
        run(el, wraps, paragraph);
    } else if WRAPPERS.contains(&name) {
        let (shell, content) = split_wrapper(el);
        let mut inner = wraps.to_vec();
        inner.push(Wrap::new(shell));
        let (runs, marks) = (paragraph.runs.len(), paragraph.marks.len());
        for child in content {
            inline(child, &inner, paragraph);
        }
        // A wrapper with nothing inside it, an empty hyperlink or a content control with no content, has
        // no run to carry it, so it is kept whole where it was.
        if paragraph.runs.len() == runs {
            paragraph.marks.truncate(marks);
            let offset = paragraph.len();
            paragraph.marks.push(Mark { offset, el: el.clone(), wraps: wraps.to_vec() });
        }
    } else if MARKS.contains(&name) {
        let offset = paragraph.len();
        paragraph.marks.push(Mark { offset, el: el.clone(), wraps: wraps.to_vec() });
    } else if name == "w:proofErr" {
        // Where the spelling checker last stopped, which Word works out again on opening.
    } else {
        // `m:oMath`, `m:oMathPara`, `w:subDoc` and anything newer: one character, written back whole and
        // where it was, as a child of the paragraph rather than of a run.
        paragraph.runs.push(Run {
            props: no_props(),
            content: Content::ParagraphObject(el.clone()),
            wraps: wraps.to_vec(),
            attrs: Arc::default(),
        });
    }
}

/// A wrapper's own element (its properties children kept) and the children that are its content.
fn split_wrapper(el: &El) -> (El, Vec<&El>) {
    let mut shell = shell(el);
    let mut content = Vec::new();
    if el.name == "w:sdt" {
        for child in el.elements() {
            match child.name.as_str() {
                "w:sdtContent" => content.extend(child.elements()),
                _ => shell.children.push(Node::El(child.clone())),
            }
        }
        return (shell, content);
    }
    for child in el.elements() {
        // `w:smartTagPr` and `w:customXmlPr` belong to the wrapper; everything else is inside it.
        if child.name.ends_with("Pr") && child.name != "w:rPr" {
            shell.children.push(Node::El(child.clone()));
        } else {
            content.push(child);
        }
    }
    (shell, content)
}

/// A `w:r`, which becomes one run for each thing it holds.
fn run(el: &El, wraps: &[Arc<Wrap>], paragraph: &mut Paragraph) {
    let props = Arc::new(el.child("w:rPr").cloned().unwrap_or_else(|| El::new("w:rPr")));
    let attrs = Arc::new(el.attrs.clone());
    let first = paragraph.runs.len();
    let push = |paragraph: &mut Paragraph, content: Content| {
        paragraph.runs.push(Run {
            props: Arc::clone(&props),
            content,
            wraps: wraps.to_vec(),
            attrs: Arc::clone(&attrs),
        });
    };
    for child in el.elements() {
        match child.name.as_str() {
            "w:rPr" | "w:lastRenderedPageBreak" => {}
            "w:t" | "w:delText" => {
                let text = child.text();
                // Two `w:t` in one run are one stretch of text.
                if paragraph.runs.len() > first {
                    if let Some(Run { content: Content::Text(before), .. }) =
                        paragraph.runs.last_mut()
                    {
                        before.push_str(&text);
                        continue;
                    }
                }
                push(paragraph, Content::Text(text));
            }
            "w:instrText" | "w:delInstrText" => push(paragraph, Content::Instr(child.text())),
            "w:tab" => push(paragraph, Content::Tab),
            "w:br" | "w:cr" => push(paragraph, Content::Break(child.clone())),
            "w:fldChar" => push(paragraph, Content::FieldChar(child.clone())),
            _ => push(paragraph, Content::Object(child.clone())),
        }
    }
}
