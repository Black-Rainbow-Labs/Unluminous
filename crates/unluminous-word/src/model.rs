//! The document, as Unluminous holds it while it is open.
//!
//! Content is typed — paragraphs, runs, tables, cells — because that is what editing and layout act on.
//! Properties are kept as the [`El`] they were read as (`w:pPr`, `w:rPr`, `w:tblPr`...), and
//! `crate::props` reads and changes them through typed accessors. So a property Unluminous does not know
//! is still there when the file is written, in the place the schema gives it.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;

use crate::package::Package;
use crate::xml::El;

/// The character a paragraph's flat text holds for a run that is not text.
pub const OBJECT: char = '\u{FFFC}';
/// A line break, `w:br` with no type, the way Word's own text stream has it.
pub const LINE_BREAK: char = '\u{000B}';
pub const PAGE_BREAK: char = '\u{000C}';
pub const COLUMN_BREAK: char = '\u{000E}';
pub const FIELD_BEGIN: char = '\u{0013}';
pub const FIELD_SEPARATE: char = '\u{0014}';
pub const FIELD_END: char = '\u{0015}';

/// A whole document.
#[derive(Debug, Clone)]
pub struct Document {
    /// The paragraphs, tables and content controls of the body, in order.
    pub body: Vec<Block>,
    /// The body's last `w:sectPr`, which describes the last section.
    pub section: El,
    /// The document element with its attributes and namespace declarations, and no children.
    pub root: El,
    /// `w:background`, the page colour, when the document has one.
    pub background: Option<El>,
    /// The name of the main document part, `word/document.xml` in every file Word writes.
    pub main: String,
    /// `word/styles.xml`.
    pub styles: Option<XmlPart>,
    /// `word/numbering.xml`.
    pub numbering: Option<XmlPart>,
    /// `word/settings.xml`.
    pub settings: Option<XmlPart>,
    /// `docProps/core.xml`.
    pub core: Option<XmlPart>,
    /// Headers and footers, by the relationship id the main part names them with.
    pub headers: Vec<HeaderPart>,
    pub footnotes: Option<NotesPart>,
    pub endnotes: Option<NotesPart>,
    pub comments: Option<NotesPart>,
    /// Every part of the file as read, with the content types and relationships.
    pub package: Package,
    /// The next `w:id` for a revision, a comment, a bookmark or a note Unluminous writes: larger than
    /// every id in the file when it was read, and shared by the copies undo keeps so an id is never
    /// given twice (task-2359, D5).
    pub ids: Arc<AtomicI64>,
}

impl Document {
    /// A `w:id` no other revision, comment range or bookmark in the document has.
    pub fn next_id(&self) -> i64 {
        self.ids.fetch_add(1, Ordering::Relaxed)
    }
}

/// The largest `w:id="n"` in some XML, for seeding [`Document::ids`].
pub fn largest_id(xml: &[u8]) -> i64 {
    let needle = b"w:id=\"";
    let mut largest = 0i64;
    let mut at = 0;
    while let Some(found) = xml[at..].windows(needle.len()).position(|w| w == needle) {
        let start = at + found + needle.len();
        let digits: Vec<u8> =
            xml[start..].iter().take_while(|b| b.is_ascii_digit()).copied().collect();
        if let Some(value) = std::str::from_utf8(&digits).ok().and_then(|d| d.parse::<i64>().ok()) {
            largest = largest.max(value);
        }
        at = start;
    }
    largest
}

/// A part Unluminous reads and writes as a tree, written back as read until it changes.
#[derive(Debug, Clone)]
pub struct XmlPart {
    pub name: String,
    pub root: El,
    /// Set by anything that changes `root`. A part nobody changed is written as the bytes it was read as.
    pub dirty: bool,
}

/// A header or a footer.
#[derive(Debug, Clone)]
pub struct HeaderPart {
    /// The relationship id in the main part.
    pub id: String,
    pub name: String,
    pub footer: bool,
    pub story: Story,
    pub dirty: bool,
}

/// The footnotes, the endnotes or the comments: a part holding a list of stories.
#[derive(Debug, Clone)]
pub struct NotesPart {
    pub name: String,
    /// `w:footnotes`, `w:endnotes` or `w:comments`, with no children.
    pub root: El,
    pub notes: Vec<Story>,
    pub dirty: bool,
}

/// Blocks that live together outside the body: one header, one footnote, one comment.
#[derive(Debug, Clone, Default)]
pub struct Story {
    /// The element that holds the blocks (`w:hdr`, `w:footnote`, `w:comment`), with no children.
    pub el: El,
    pub blocks: Vec<Block>,
}

impl Story {
    /// `w:id`, which a footnote, an endnote and a comment have.
    pub fn id(&self) -> Option<i64> {
        self.el.attr("w:id").and_then(|id| id.parse().ok())
    }
}

/// One thing at the level of the body or of a cell.
#[derive(Debug, Clone)]
pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
    /// A block content control, whose content is ordinary blocks.
    Sdt(Sdt),
    /// Anything else at block level, written back as it was read.
    Kept(El),
}

impl Block {
    pub fn as_paragraph(&self) -> Option<&Paragraph> {
        match self {
            Block::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        }
    }

    pub fn as_paragraph_mut(&mut self) -> Option<&mut Paragraph> {
        match self {
            Block::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        }
    }
}

/// A paragraph.
#[derive(Debug, Clone, Default)]
pub struct Paragraph {
    /// The attributes of `w:p` itself (`w14:paraId`, `w:rsidR`...).
    pub attrs: Vec<(String, String)>,
    /// `w:pPr`, empty when the paragraph has none.
    pub props: El,
    pub runs: Vec<Run>,
    /// What sits between runs and takes no room: bookmarks, comment ranges, permission ranges.
    pub marks: Vec<Mark>,
}

/// One thing in a paragraph, with its formatting.
#[derive(Debug, Clone)]
pub struct Run {
    /// `w:rPr`, shared between the runs one `w:r` was split into.
    pub props: Arc<El>,
    pub content: Content,
    /// The elements this run sits inside, outermost first: `w:hyperlink`, `w:ins`, `w:del`, `w:sdt`,
    /// `w:fldSimple`, `w:customXml`, `w:smartTag`. Consecutive runs inside the same one share it.
    pub wraps: Vec<Arc<Wrap>>,
    /// The attributes of the `w:r` this run came from (`w:rsidR`...).
    pub attrs: Arc<Vec<(String, String)>>,
}

/// What a run holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    Tab,
    /// `w:br` (line, page or column, from `w:type`) or `w:cr`.
    Break(El),
    /// `w:fldChar`, whose `w:fldCharType` is begin, separate or end.
    FieldChar(El),
    /// `w:instrText`, a field's code.
    Instr(String),
    /// Everything else a run can hold: a picture, a note reference, a symbol, a comment reference,
    /// a non breaking or soft hyphen, `mc:AlternateContent`, an object.
    Object(El),
    /// An element the schema puts directly in a paragraph rather than in a run: an equation
    /// (`m:oMath`, `m:oMathPara`), a sub document (`w:subDoc`), or any element of another namespace. It
    /// is one character in the flat text and is written back as a child of `w:p`, never inside a `w:r`,
    /// which Word calls unreadable content (task-2359, D1).
    ParagraphObject(El),
}

/// An element a run sits inside.
#[derive(Debug, Clone, PartialEq)]
pub struct Wrap {
    /// Two runs are inside the same element when their wraps have the same id.
    pub id: u64,
    /// The element, without the runs inside it. A `w:sdt` keeps its `w:sdtPr` and `w:sdtEndPr`.
    pub el: El,
}

static NEXT_WRAP: AtomicU64 = AtomicU64::new(1);

impl Wrap {
    pub fn new(el: El) -> Arc<Wrap> {
        Arc::new(Wrap { id: NEXT_WRAP.fetch_add(1, Ordering::Relaxed), el })
    }
}

/// A zero width thing at an offset in a paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// The byte offset in the paragraph's flat text.
    pub offset: usize,
    pub el: El,
    /// The wraps of the run it was read beside, so it is written back inside the same hyperlink.
    pub wraps: Vec<Arc<Wrap>>,
}

/// A table.
#[derive(Debug, Clone, Default)]
pub struct Table {
    /// `w:tblPr`.
    pub props: El,
    /// `w:tblGrid`, whose `w:gridCol` widths are in twips.
    pub grid: El,
    pub rows: Vec<Row>,
    /// Other children of `w:tbl` (a bookmark between rows), with the index of the row they came before.
    pub extra: Vec<(usize, El)>,
}

impl Table {
    /// The grid's column widths in twips.
    pub fn columns(&self) -> Vec<i64> {
        self.grid
            .children_named("w:gridCol")
            .map(|col| col.attr("w:w").and_then(|w| w.parse::<f64>().ok()).unwrap_or(0.0) as i64)
            .collect()
    }
}

/// A table row.
#[derive(Debug, Clone, Default)]
pub struct Row {
    pub attrs: Vec<(String, String)>,
    /// `w:tblPrEx`.
    pub exceptions: Option<El>,
    /// `w:trPr`.
    pub props: El,
    pub cells: Vec<Cell>,
    /// Other children of `w:tr`, with the index of the cell they came before.
    pub extra: Vec<(usize, El)>,
}

/// A table cell.
#[derive(Debug, Clone, Default)]
pub struct Cell {
    pub attrs: Vec<(String, String)>,
    /// `w:tcPr`.
    pub props: El,
    pub blocks: Vec<Block>,
}

/// A block content control.
#[derive(Debug, Clone, Default)]
pub struct Sdt {
    /// `w:sdt` itself, with its `w:sdtPr` and `w:sdtEndPr` and no `w:sdtContent`.
    pub el: El,
    pub blocks: Vec<Block>,
}

impl Content {
    /// The character this content stands for in a paragraph's flat text, or `None` for text, which
    /// stands for itself.
    pub fn character(&self) -> Option<char> {
        Some(match self {
            Content::Text(_) | Content::Instr(_) => return None,
            Content::Tab => '\t',
            Content::Break(el) => match el.attr("w:type") {
                Some("page") => PAGE_BREAK,
                Some("column") => COLUMN_BREAK,
                _ => LINE_BREAK,
            },
            Content::FieldChar(el) => match el.attr("w:fldCharType") {
                Some("begin") => FIELD_BEGIN,
                Some("separate") => FIELD_SEPARATE,
                _ => FIELD_END,
            },
            Content::ParagraphObject(_) => OBJECT,
            Content::Object(el) => match el.name.as_str() {
                "w:noBreakHyphen" => '\u{2011}',
                "w:softHyphen" => '\u{00AD}',
                "w:sym" => el
                    .attr("w:char")
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    // A symbol font's characters are written in the private use area from F000.
                    .map(
                        |code| if (0xF000..=0xF0FF).contains(&code) { code - 0xF000 } else { code },
                    )
                    .and_then(char::from_u32)
                    .unwrap_or(OBJECT),
                _ => OBJECT,
            },
        })
    }

    /// How many bytes this content takes in the flat text.
    pub fn len(&self) -> usize {
        match self {
            Content::Text(text) | Content::Instr(text) => text.len(),
            other => other.character().map_or(0, char::len_utf8),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The picture element when this is one: `w:drawing`, `w:pict`, or the drawing inside
    /// `mc:AlternateContent`.
    pub fn picture(&self) -> Option<&El> {
        match self {
            Content::Object(el) if el.name == "w:drawing" || el.name == "w:pict" => Some(el),
            Content::Object(el) if el.name == "mc:AlternateContent" => el
                .elements()
                .flat_map(|branch| branch.elements())
                .find(|inner| inner.name == "w:drawing" || inner.name == "w:pict"),
            _ => None,
        }
    }
}

impl Run {
    /// A run of text with the given properties and no wraps.
    pub fn text(text: impl Into<String>, props: Arc<El>) -> Self {
        Run { props, content: Content::Text(text.into()), wraps: Vec::new(), attrs: Arc::default() }
    }

    pub fn len(&self) -> usize {
        self.content.len()
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Whether this run is inside a `w:del` or a `w:moveFrom`: tracked as deleted.
    pub fn is_deleted(&self) -> bool {
        self.wraps.iter().any(|wrap| matches!(wrap.el.name.as_str(), "w:del" | "w:moveFrom"))
    }

    /// Whether this run is inside a `w:ins` or a `w:moveTo`: tracked as inserted.
    pub fn is_inserted(&self) -> bool {
        self.wraps.iter().any(|wrap| matches!(wrap.el.name.as_str(), "w:ins" | "w:moveTo"))
    }

    /// The hyperlink this run is inside, if it is inside one.
    pub fn link(&self) -> Option<&El> {
        self.wraps.iter().find(|wrap| wrap.el.name == "w:hyperlink").map(|wrap| &wrap.el)
    }
}

impl Paragraph {
    /// A paragraph with no runs and no properties.
    pub fn new() -> Self {
        Paragraph { props: El::new("w:pPr"), ..Paragraph::default() }
    }

    /// The flat text: every run's text, with one character for each run that is not text.
    pub fn flat_text(&self) -> String {
        let mut out = String::with_capacity(self.len());
        for run in &self.runs {
            match &run.content {
                Content::Text(text) | Content::Instr(text) => out.push_str(text),
                other => out.extend(other.character()),
            }
        }
        out
    }

    /// The length of the flat text in bytes.
    pub fn len(&self) -> usize {
        self.runs.iter().map(Run::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The text a reader sees: no field codes, no field characters, no deleted text, objects left out.
    pub fn visible_text(&self) -> String {
        let mut out = String::new();
        let mut depth_in_code: Vec<bool> = Vec::new();
        for run in &self.runs {
            if let Content::FieldChar(el) = &run.content {
                match el.attr("w:fldCharType") {
                    Some("begin") => depth_in_code.push(true),
                    Some("separate") => {
                        if let Some(last) = depth_in_code.last_mut() {
                            *last = false;
                        }
                    }
                    _ => {
                        depth_in_code.pop();
                    }
                }
                continue;
            }
            if depth_in_code.iter().any(|in_code| *in_code) || run.is_deleted() {
                continue;
            }
            match &run.content {
                Content::Text(text) => out.push_str(text),
                Content::Instr(_) | Content::FieldChar(_) => {}
                Content::Tab => out.push('\t'),
                Content::Break(_) => out.extend(run.content.character()),
                Content::Object(el)
                    if el.name == "w:footnoteReference" || el.name == "w:endnoteReference" => {}
                other => {
                    if let Some(character) = other.character().filter(|c| *c != OBJECT) {
                        out.push(character);
                    }
                }
            }
        }
        out
    }

    /// The index of the run holding `offset` and how far into it `offset` is. An offset on the boundary
    /// of two runs is answered as the end of the earlier one, so typing there continues its formatting.
    pub fn run_at(&self, offset: usize) -> Option<(usize, usize)> {
        if offset == 0 {
            return (!self.runs.is_empty()).then_some((0, 0));
        }
        let mut at = 0;
        for (index, run) in self.runs.iter().enumerate() {
            let len = run.len();
            if offset > at && offset <= at + len {
                return Some((index, offset - at));
            }
            at += len;
        }
        None
    }

    /// Split runs so that `offset` falls on a run boundary, and answer the index of the first run at or
    /// after it.
    pub fn split_at(&mut self, offset: usize) -> usize {
        let mut at = 0;
        for index in 0..self.runs.len() {
            let len = self.runs[index].len();
            if offset == at {
                return index;
            }
            if offset < at + len {
                let into = offset - at;
                let tail = split_run(&mut self.runs[index], into);
                self.runs.insert(index + 1, tail);
                return index + 1;
            }
            at += len;
        }
        self.runs.len()
    }

    /// Join neighbouring text runs that have the same properties, wraps and attributes.
    pub fn merge_runs(&mut self) {
        let mut merged: Vec<Run> = Vec::with_capacity(self.runs.len());
        for run in std::mem::take(&mut self.runs) {
            if run.is_empty() && matches!(run.content, Content::Text(_)) {
                continue;
            }
            if let Some(last) = merged.last_mut() {
                if let (Content::Text(before), Content::Text(after)) =
                    (&mut last.content, &run.content)
                {
                    if same_formatting(&last.props, &run.props)
                        && same_wraps(&last.wraps, &run.wraps)
                    {
                        before.push_str(after);
                        continue;
                    }
                }
            }
            merged.push(run);
        }
        self.runs = merged;
    }
}

/// Whether two runs' properties are the same, by pointer first and then by value.
pub fn same_formatting(a: &Arc<El>, b: &Arc<El>) -> bool {
    Arc::ptr_eq(a, b) || **a == **b
}

/// Whether two runs are inside exactly the same elements.
pub fn same_wraps(a: &[Arc<Wrap>], b: &[Arc<Wrap>]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.id == y.id)
}

/// Cut `run` at `into` bytes, keep the front in place and answer the back. Only text can be cut; any
/// other run is one character and is cut before or after it by the caller.
fn split_run(run: &mut Run, into: usize) -> Run {
    let mut tail = run.clone();
    match (&mut run.content, &mut tail.content) {
        (Content::Text(front), Content::Text(back))
        | (Content::Instr(front), Content::Instr(back)) => {
            let mut cut = into.min(front.len());
            while !front.is_char_boundary(cut) {
                cut -= 1;
            }
            *back = front[cut..].to_owned();
            front.truncate(cut);
        }
        _ => {}
    }
    tail
}

/// The empty run properties every new run starts from.
pub fn no_props() -> Arc<El> {
    Arc::new(El::new("w:rPr"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph(parts: &[&str]) -> Paragraph {
        let mut paragraph = Paragraph::new();
        for part in parts {
            paragraph.runs.push(Run::text(*part, no_props()));
        }
        paragraph
    }

    #[test]
    fn flat_text_gives_each_object_one_character() {
        let mut paragraph = paragraph(&["ab"]);
        paragraph.runs.push(Run { content: Content::Tab, ..Run::text("", no_props()) });
        paragraph.runs.push(Run {
            content: Content::Break(El::new("w:br").with("w:type", "page")),
            ..Run::text("", no_props())
        });
        paragraph.runs.push(Run {
            content: Content::Object(El::new("w:drawing")),
            ..Run::text("", no_props())
        });
        assert_eq!(paragraph.flat_text(), "ab\t\u{c}\u{fffc}");
        assert_eq!(paragraph.len(), paragraph.flat_text().len());
    }

    #[test]
    fn splitting_puts_a_boundary_at_the_offset_and_merging_takes_it_away() {
        let mut paragraph = paragraph(&["héllo", " world"]);
        let index = paragraph.split_at(3);
        assert_eq!(index, 1);
        assert_eq!(paragraph.runs.len(), 3);
        assert_eq!(paragraph.flat_text(), "héllo world");
        paragraph.merge_runs();
        assert_eq!(paragraph.runs.len(), 1);
        assert_eq!(paragraph.split_at(0), 0);
        assert_eq!(paragraph.split_at(paragraph.len()), 1);
    }

    #[test]
    fn visible_text_leaves_field_codes_and_deletions_out() {
        let mut paragraph = paragraph(&["Page "]);
        let plain = || Run::text("", no_props());
        paragraph.runs.push(Run {
            content: Content::FieldChar(El::new("w:fldChar").with("w:fldCharType", "begin")),
            ..plain()
        });
        paragraph.runs.push(Run { content: Content::Instr(" PAGE ".into()), ..plain() });
        paragraph.runs.push(Run {
            content: Content::FieldChar(El::new("w:fldChar").with("w:fldCharType", "separate")),
            ..plain()
        });
        paragraph.runs.push(Run::text("3", no_props()));
        paragraph.runs.push(Run {
            content: Content::FieldChar(El::new("w:fldChar").with("w:fldCharType", "end")),
            ..plain()
        });
        let mut gone = Run::text(" gone", no_props());
        gone.wraps.push(Wrap::new(El::new("w:del")));
        paragraph.runs.push(gone);
        assert_eq!(paragraph.visible_text(), "Page 3");
    }
}
