//! Writing a [`Document`] back to the bytes of a `.docx` file.
//!
//! The main part is always written from the model. Every other part Unluminous models is written from the
//! model only when it changed, and otherwise as the bytes it was read as; every part it does not model is
//! written back untouched. Properties elements are put in the schema's order first (`order::sort`).

use std::sync::Arc;

use crate::model::*;
use crate::order;
use crate::xml::{self, El, Node, CANONICAL};

const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

/// Write the document as the bytes of a `.docx` file.
pub fn save(document: &Document) -> Result<Vec<u8>, String> {
    let mut package = document.package.clone();
    package.set_part(&document.main, main_part(document).into_bytes());
    for part in [&document.styles, &document.numbering, &document.settings, &document.core]
        .into_iter()
        .flatten()
    {
        if part.dirty {
            package.set_part(&part.name, whole_part(&part.root).into_bytes());
        }
    }
    for header in document.headers.iter().filter(|header| header.dirty) {
        package.set_part(
            &header.name,
            story_part(&header.story.el, &header.story.blocks).into_bytes(),
        );
    }
    for notes in [&document.footnotes, &document.endnotes, &document.comments].into_iter().flatten()
    {
        if notes.dirty {
            package.set_part(&notes.name, notes_part(notes).into_bytes());
        }
    }
    package.write_housekeeping();
    let entries: Vec<crate::zip::Entry> = package
        .parts
        .iter()
        .map(|part| crate::zip::Entry { name: part.name.clone(), data: part.data.as_ref().clone() })
        .collect();
    crate::zip::write(&entries)
}

/// The content type of the main part of a file with `extension`: a document, a template, and each
/// with macros.
pub fn main_content_type(extension: &str) -> &'static str {
    match extension.to_lowercase().as_str() {
        "dotx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml",
        "docm" => "application/vnd.ms-word.document.macroEnabled.main+xml",
        "dotm" => "application/vnd.ms-word.template.macroEnabledTemplate.main+xml",
        _ => "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
    }
}

/// Make the document right for saving as a file with `extension` (`docx`, `dotx`, `docm`, `dotm`): the
/// main part's content type says which (task-2359, D3), and a file that cannot hold macros loses the
/// macro project, as Word drops it when a `.docm` is saved as a `.docx`.
pub fn prepare_for(document: &mut Document, extension: &str) {
    let main = document.main.clone();
    document.package.set_override(&main, main_content_type(extension));
    let macros = matches!(extension.to_lowercase().as_str(), "docm" | "dotm");
    if !macros {
        let project: Vec<String> = document
            .package
            .parts
            .iter()
            .map(|part| part.name.clone())
            .filter(|name| {
                let file = name.rsplit('/').next().unwrap_or(name).to_lowercase();
                file.starts_with("vbaproject") || file.starts_with("vbadata")
            })
            .filter(|name| !name.ends_with(".rels"))
            .collect();
        for name in project {
            document.package.remove_part(&name);
        }
    }
}

/// `word/document.xml`.
pub fn main_part(document: &Document) -> String {
    let mut out = String::with_capacity(64 * 1024);
    out.push_str(DECLARATION);
    root_start(&document.root, &mut out);
    if let Some(background) = &document.background {
        background.write(&mut out);
    }
    out.push_str("<w:body>");
    write_blocks(&document.body, &mut out);
    let mut section = document.section.clone();
    order::sort(&mut section);
    section.write(&mut out);
    out.push_str("</w:body></");
    out.push_str(&document.root.name);
    out.push('>');
    out
}

/// A part that is one tree: styles, numbering, settings, core properties.
pub fn whole_part(root: &El) -> String {
    let mut out = String::from(DECLARATION);
    root_start(root, &mut out);
    for node in &root.children {
        match node {
            Node::El(el) => {
                let mut el = el.clone();
                order::sort(&mut el);
                el.write(&mut out);
            }
            Node::Text(text) => xml::escape_text(text, &mut out),
        }
    }
    out.push_str("</");
    out.push_str(&root.name);
    out.push('>');
    out
}

/// A header or footer part.
pub fn story_part(root: &El, blocks: &[Block]) -> String {
    let mut out = String::from(DECLARATION);
    root_start(root, &mut out);
    write_blocks(blocks, &mut out);
    out.push_str("</");
    out.push_str(&root.name);
    out.push('>');
    out
}

/// The footnotes, endnotes or comments part.
fn notes_part(notes: &NotesPart) -> String {
    let mut out = String::from(DECLARATION);
    root_start(&notes.root, &mut out);
    for note in &notes.notes {
        open_tag(&note.el, &mut out);
        write_blocks(&note.blocks, &mut out);
        close_tag(&note.el, &mut out);
    }
    out.push_str("</");
    out.push_str(&notes.root.name);
    out.push('>');
    out
}

/// A part's document element's start tag, declaring every namespace Word's prefixes name.
///
/// Word's own prefixes are read as Word's prefixes wherever a file declared them (`xml` module), so the
/// one place they have to be declared is here. Declaring one a part does not use costs a few bytes and
/// is exactly what Word does itself.
pub fn root_start(root: &El, out: &mut String) {
    out.push('<');
    out.push_str(&root.name);
    let default = root.name.contains(':');
    for (prefix, uri) in CANONICAL {
        // A part whose elements have no prefix at all (core properties use their own) still only needs
        // the declarations it had, which `declares` holds. A Strict file declares Word's own prefixes
        // with the Strict namespaces, and it is written back in them (task-2359, D6).
        let uri = root.declares.iter().find(|(p, _)| p == prefix).map_or(*uri, |(_, u)| u.as_str());
        if default {
            out.push_str(&format!(" xmlns:{prefix}=\""));
            xml::escape_attr(uri, out);
            out.push('"');
        }
    }
    for (prefix, uri) in &root.declares {
        if CANONICAL.iter().any(|(known, _)| known == prefix) {
            continue;
        }
        out.push_str(&format!(" xmlns:{prefix}=\""));
        xml::escape_attr(uri, out);
        out.push('"');
    }
    for (key, value) in &root.attrs {
        out.push(' ');
        out.push_str(key);
        out.push_str("=\"");
        xml::escape_attr(value, out);
        out.push('"');
    }
    out.push('>');
}

/// `<name attrs>`, with the namespaces the element declared.
fn open_tag(el: &El, out: &mut String) {
    let mut shell = el.clone();
    shell.children.clear();
    let mut text = shell.to_xml();
    // `<name .../>` becomes `<name ...>`.
    text.truncate(text.len() - 2);
    out.push_str(&text);
    out.push('>');
}

fn close_tag(el: &El, out: &mut String) {
    out.push_str("</");
    out.push_str(&el.name);
    out.push('>');
}

/// Blocks, in order.
pub fn write_blocks(blocks: &[Block], out: &mut String) {
    for block in blocks {
        match block {
            Block::Paragraph(paragraph) => write_paragraph(paragraph, out),
            Block::Table(table) => write_table(table, out),
            Block::Sdt(sdt) => {
                let mut shell = sdt.el.clone();
                for child in shell.elements_mut() {
                    order::sort(child);
                }
                let mut text = El { children: Vec::new(), ..shell.clone() }.to_xml();
                text.truncate(text.len() - 2);
                out.push_str(&text);
                out.push('>');
                for child in shell.elements() {
                    child.write(out);
                }
                out.push_str("<w:sdtContent>");
                write_blocks(&sdt.blocks, out);
                out.push_str("</w:sdtContent></w:sdt>");
            }
            Block::Kept(el) => el.write(out),
        }
    }
}

fn write_table(table: &Table, out: &mut String) {
    out.push_str("<w:tbl>");
    let mut props = table.props.clone();
    order::sort(&mut props);
    props.write(out);
    table.grid.write(out);
    for (index, row) in table.rows.iter().enumerate() {
        for (_, el) in table.extra.iter().filter(|(before, _)| *before == index) {
            el.write(out);
        }
        let mut tag = El::new("w:tr");
        tag.attrs = row.attrs.clone();
        open_tag(&tag, out);
        if let Some(exceptions) = &row.exceptions {
            exceptions.write(out);
        }
        if row.props.elements().next().is_some() || !row.props.attrs.is_empty() {
            let mut props = row.props.clone();
            order::sort(&mut props);
            props.write(out);
        }
        for (index, cell) in row.cells.iter().enumerate() {
            for (_, el) in row.extra.iter().filter(|(before, _)| *before == index) {
                el.write(out);
            }
            let mut tag = El::new("w:tc");
            tag.attrs = cell.attrs.clone();
            open_tag(&tag, out);
            let mut props = cell.props.clone();
            order::sort(&mut props);
            if props.elements().next().is_some() {
                props.write(out);
            }
            write_blocks(&cell.blocks, out);
            // A cell must end with a paragraph, and Word calls a file without one damaged.
            if !matches!(cell.blocks.last(), Some(Block::Paragraph(_))) {
                out.push_str("<w:p/>");
            }
            out.push_str("</w:tc>");
        }
        for (_, el) in row.extra.iter().filter(|(before, _)| *before >= row.cells.len()) {
            el.write(out);
        }
        out.push_str("</w:tr>");
    }
    for (_, el) in table.extra.iter().filter(|(before, _)| *before >= table.rows.len()) {
        el.write(out);
    }
    out.push_str("</w:tbl>");
}

/// A paragraph: its properties, then its runs with their wraps opened and closed around them, and its
/// marks at their offsets.
pub fn write_paragraph(paragraph: &Paragraph, out: &mut String) {
    let mut tag = El::new("w:p");
    tag.attrs = paragraph.attrs.clone();
    open_tag(&tag, out);
    if paragraph.props.elements().next().is_some() || !paragraph.props.attrs.is_empty() {
        let mut props = paragraph.props.clone();
        order::sort(&mut props);
        props.write(out);
    }
    let mut open: Vec<Arc<Wrap>> = Vec::new();
    let mut marks = paragraph.marks.iter().peekable();
    let mut offset = 0;
    let mut index = 0;
    while index < paragraph.runs.len() {
        while let Some(mark) = marks.next_if(|mark| mark.offset <= offset) {
            close_to_shared(&mut open, &mark.wraps, out);
            mark.el.write(out);
        }
        let run = &paragraph.runs[index];
        close_to_shared(&mut open, &run.wraps, out);
        for wrap in &run.wraps[open.len()..] {
            open_wrap(wrap, out);
            open.push(Arc::clone(wrap));
        }
        if let Content::ParagraphObject(el) = &run.content {
            // An equation or a sub document is a child of the paragraph, not of a run.
            el.write(out);
            offset += run.len();
            index += 1;
            continue;
        }
        // Runs from one `w:r` (one properties value, one set of attributes, nothing between them) are
        // written as one `w:r` again, the way Word wrote them.
        let mut last = index + 1;
        let mut end = offset + run.len();
        while last < paragraph.runs.len() {
            let next = &paragraph.runs[last];
            let joins = Arc::ptr_eq(&next.props, &run.props)
                && Arc::ptr_eq(&next.attrs, &run.attrs)
                && same_wraps(&next.wraps, &run.wraps)
                && marks.peek().is_none_or(|mark| mark.offset > end);
            if !joins {
                break;
            }
            end += next.len();
            last += 1;
        }
        write_runs(&paragraph.runs[index..last], out);
        offset = end;
        index = last;
    }
    for mark in marks {
        close_to_shared(&mut open, &mark.wraps, out);
        mark.el.write(out);
    }
    close_to_shared(&mut open, &[], out);
    out.push_str("</w:p>");
}

/// Close the open wraps until what is left is shared with `wanted`.
fn close_to_shared(open: &mut Vec<Arc<Wrap>>, wanted: &[Arc<Wrap>], out: &mut String) {
    let shared = open.iter().zip(wanted).take_while(|(a, b)| a.id == b.id).count();
    while open.len() > shared {
        let wrap = open.pop().expect("longer than shared");
        if wrap.el.name == "w:sdt" {
            out.push_str("</w:sdtContent>");
        }
        close_tag(&wrap.el, out);
    }
}

fn open_wrap(wrap: &Wrap, out: &mut String) {
    let mut shell = wrap.el.clone();
    shell.children.clear();
    let mut text = shell.to_xml();
    text.truncate(text.len() - 2);
    out.push_str(&text);
    out.push('>');
    for child in wrap.el.elements() {
        child.write(out);
    }
    if wrap.el.name == "w:sdt" {
        out.push_str("<w:sdtContent>");
    }
}

/// Runs that share a `w:r`.
fn write_runs(runs: &[Run], out: &mut String) {
    let first = &runs[0];
    let mut tag = El::new("w:r");
    tag.attrs = first.attrs.as_ref().clone();
    open_tag(&tag, out);
    if first.props.elements().next().is_some() {
        let mut props = first.props.as_ref().clone();
        order::sort(&mut props);
        props.write(out);
    }
    let deleted = first.is_deleted();
    for run in runs {
        match &run.content {
            Content::Text(text) => {
                text_element(if deleted { "w:delText" } else { "w:t" }, text, out)
            }
            Content::Instr(text) => {
                text_element(if deleted { "w:delInstrText" } else { "w:instrText" }, text, out)
            }
            Content::Tab => out.push_str("<w:tab/>"),
            Content::Break(el)
            | Content::FieldChar(el)
            | Content::Object(el)
            | Content::ParagraphObject(el) => el.write(out),
        }
    }
    out.push_str("</w:r>");
}

/// `<w:t>`, with `xml:space="preserve"` when the text starts or ends with a space, which is the only time
/// Word writes it.
fn text_element(name: &str, text: &str, out: &mut String) {
    out.push('<');
    out.push_str(name);
    if text.starts_with(char::is_whitespace)
        || text.ends_with(char::is_whitespace)
        || text.contains("  ")
    {
        out.push_str(" xml:space=\"preserve\"");
    }
    out.push('>');
    xml::escape_text(text, out);
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}
