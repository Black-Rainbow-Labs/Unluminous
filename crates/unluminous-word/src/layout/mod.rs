//! Turning a document into pages.
//!
//! [`lay_out`] walks the body section by section. Each paragraph becomes lines ([`paragraph`]), each
//! table row becomes one piece that is never split ([`table`]), and the pieces are stacked into the
//! columns of each page, with Word's keep, widow and break rules deciding where a page ends. Headers,
//! footers and footnotes are laid out for each page once the body knows which pages there are.
//!
//! Everything is in points, with each page's origin at its top left corner. What comes out is what the
//! window draws ([`Item`]) and what the caret and the pointer need ([`CaretLine`]): for every line, which
//! paragraph it is in, which offsets it holds and where each one is.

pub mod fields;
pub mod notes;
pub mod paragraph;
pub mod section;
pub mod table;

use std::collections::HashMap;

use unluminous_core::{CharStyle, FontMetrics};

use crate::model::{Block, Document};
use crate::numbering::Counters;
use crate::position::{Path, Pos, StoryId};
use crate::props::Rgb;
use crate::styles::Resolver;
use section::{Geometry, Start};

/// A rectangle in page points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// Letters in one style, each at its own place.
#[derive(Debug, Clone, PartialEq)]
pub struct TextItem {
    pub baseline: f32,
    pub style: CharStyle,
    pub text: String,
    /// The left edge of each character of `text`, in page points.
    pub xs: Vec<f32>,
}

/// How a rule is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    Solid,
    Double,
    Dotted,
    Dashed,
    Wave,
}

/// One thing drawn on a page.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Text(TextItem),
    Fill {
        rect: Rect,
        color: Rgb,
    },
    /// A horizontal or vertical line from `(x0, y0)` to `(x1, y1)`.
    Rule {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        width: f32,
        color: Rgb,
        kind: RuleKind,
    },
    /// A picture, by the package part it is in.
    Picture {
        rect: Rect,
        part: String,
    },
    /// Something Unluminous does not draw, shown as a box with a word in it.
    Placeholder {
        rect: Rect,
        label: String,
    },
}

impl Item {
    /// Move the item by `(dx, dy)`.
    pub fn shift(&mut self, dx: f32, dy: f32) {
        match self {
            Item::Text(text) => {
                text.baseline += dy;
                for x in &mut text.xs {
                    *x += dx;
                }
            }
            Item::Fill { rect, .. }
            | Item::Picture { rect, .. }
            | Item::Placeholder { rect, .. } => {
                rect.x += dx;
                rect.y += dy;
            }
            Item::Rule { x0, y0, x1, y1, .. } => {
                *x0 += dx;
                *x1 += dx;
                *y0 += dy;
                *y1 += dy;
            }
        }
    }
}

/// One line of one paragraph, for the caret and the pointer.
#[derive(Debug, Clone, PartialEq)]
pub struct CaretLine {
    pub story: StoryId,
    pub path: Path,
    /// The first offset on the line and the offset after its last character.
    pub start: usize,
    pub end: usize,
    pub top: f32,
    pub height: f32,
    pub baseline: f32,
    /// Every offset the caret can stand at on this line and its x, in order.
    pub stops: Vec<(usize, f32)>,
    /// Whether this is the paragraph's last line, where its end offset belongs.
    pub last: bool,
}

impl CaretLine {
    fn shift(&mut self, dx: f32, dy: f32) {
        self.top += dy;
        self.baseline += dy;
        for stop in &mut self.stops {
            stop.1 += dx;
        }
    }

    /// The x of an offset on this line, from the nearest stop at or before it.
    pub fn x_of(&self, offset: usize) -> f32 {
        let mut x = self.stops.first().map_or(0.0, |stop| stop.1);
        for (at, stop_x) in &self.stops {
            if *at > offset {
                break;
            }
            x = *stop_x;
        }
        x
    }

    /// The offset nearest an x on this line.
    pub fn offset_at(&self, x: f32) -> usize {
        let mut best = (f32::MAX, self.start);
        for (at, stop_x) in &self.stops {
            let distance = (stop_x - x).abs();
            if distance < best.0 {
                best = (distance, *at);
            }
        }
        best.1
    }

    /// Whether `offset` is on this line: from its start up to but not including its end, except on a
    /// paragraph's last line, which owns the end too.
    pub fn holds(&self, offset: usize) -> bool {
        offset >= self.start
            && (offset < self.end || (self.last && offset == self.end) || self.start == self.end)
    }
}

/// A picture placed by its anchor rather than in a line.
#[derive(Debug, Clone, PartialEq)]
pub struct Float {
    pub width: f32,
    pub height: f32,
    pub horizontal: (String, Anchor),
    pub vertical: (String, Anchor),
    pub part: String,
    /// `square`, `tight`, `through`, `topAndBottom`, `none`, `behind`.
    pub wrap: String,
}

/// Where a float sits along one direction.
#[derive(Debug, Clone, PartialEq)]
pub enum Anchor {
    Offset(f32),
    Align(String),
}

/// A piece that is placed whole: one line of a paragraph, or one table row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chunk {
    pub height: f32,
    pub items: Vec<Item>,
    pub carets: Vec<CaretLine>,
    /// The footnotes this piece refers to, which go at the foot of the page it lands on.
    pub notes: Vec<i64>,
    pub floats: Vec<Float>,
    /// A page or column break at the end of this piece.
    pub break_after: Option<BreakAfter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakAfter {
    Page,
    Column,
}

impl Chunk {
    /// Move everything in the piece.
    pub fn shift(&mut self, dx: f32, dy: f32) {
        for item in &mut self.items {
            item.shift(dx, dy);
        }
        for caret in &mut self.carets {
            caret.shift(dx, dy);
        }
    }

    /// Add another piece's content below this one's top at `dy`.
    pub fn absorb(&mut self, mut other: Chunk, dx: f32, dy: f32) {
        other.shift(dx, dy);
        self.items.extend(other.items);
        self.carets.extend(other.carets);
        self.notes.extend(other.notes);
        self.floats.extend(other.floats);
    }
}

/// One laid out page.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    /// The number Word prints on it, which a section can restart.
    pub number: u32,
    /// Which section it belongs to, counting from zero.
    pub section: usize,
    /// The body's rectangle: inside the margins.
    pub body: Rect,
    pub items: Vec<Item>,
    pub carets: Vec<CaretLine>,
    /// The first body position on the page, for `pages` and for agents.
    pub first: Option<Pos>,
    /// The page colour, when the document has one.
    pub background: Option<Rgb>,
    /// The footnotes at the foot of this page, in order, with the lines of each that are here.
    pub notes: Vec<NotePiece>,
}

/// Some lines of one footnote at the foot of a page.
#[derive(Debug, Clone, PartialEq)]
pub struct NotePiece {
    pub id: i64,
    /// The note's lines `from..to` are on this page.
    pub from: usize,
    pub to: usize,
    /// It began on an earlier page, so it is under the continuation separator.
    pub continued: bool,
}

/// Everything the layout needs besides the document.
pub struct Ctx<'a> {
    pub document: &'a Document,
    pub resolver: &'a Resolver<'a>,
    pub metrics: &'a dyn FontMetrics,
    /// Points between default tab stops.
    pub default_tab: f32,
    /// Draw hidden text and the paragraph marks.
    pub show_marks: bool,
    /// Each footnote's number, by its id, in order of first reference.
    pub footnote_numbers: HashMap<i64, u32>,
    pub endnote_numbers: HashMap<i64, u32>,
    /// The mark each note shows, in its numbering format.
    pub footnote_marks: HashMap<i64, String>,
    pub endnote_marks: HashMap<i64, String>,
    /// The part the story being laid out lives in, which its pictures' relationships belong to.
    pub part: String,
    /// The page being laid out, for PAGE and NUMPAGES in a header: `(number, pages)`.
    pub page: Option<(u32, u32)>,
}

/// How the layout was asked to run.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub show_marks: bool,
}

/// The pages of a document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pages {
    pub pages: Vec<Page>,
}

impl Pages {
    /// The page and line a position is on.
    pub fn find(&self, pos: &Pos) -> Option<(usize, &CaretLine)> {
        let mut fallback = None;
        for (index, page) in self.pages.iter().enumerate() {
            for line in &page.carets {
                if line.story == pos.story && line.path == pos.path {
                    if line.holds(pos.offset) {
                        return Some((index, line));
                    }
                    if pos.offset >= line.start {
                        fallback = Some((index, line));
                    }
                }
            }
        }
        fallback
    }

    /// The position under a point on a page, in the body or a header, the nearest line on that page.
    pub fn hit(&self, page: usize, x: f32, y: f32, stories: &[StoryId]) -> Option<Pos> {
        let page = self.pages.get(page)?;
        let line =
            page.carets.iter().filter(|line| stories.contains(&line.story)).min_by(|a, b| {
                let distance = |line: &CaretLine| {
                    let dy = if y < line.top {
                        line.top - y
                    } else if y > line.top + line.height {
                        y - line.top - line.height
                    } else {
                        0.0
                    };
                    let left = line.stops.first().map_or(0.0, |s| s.1);
                    let right = line.stops.last().map_or(0.0, |s| s.1);
                    let dx = if x < left {
                        left - x
                    } else if x > right {
                        x - right
                    } else {
                        0.0
                    };
                    dy * 1000.0 + dx
                };
                distance(a).total_cmp(&distance(b))
            })?;
        Some(Pos { story: line.story.clone(), path: line.path.clone(), offset: line.offset_at(x) })
    }
}

/// The document's default tab stop, from its settings.
fn default_tab(document: &Document) -> f32 {
    document
        .settings
        .as_ref()
        .and_then(|settings| settings.root.val_of("w:defaultTabStop"))
        .and_then(crate::props::number)
        .map_or(36.0, crate::props::twips)
        .max(1.0)
}

/// Lay the whole document out into pages.
pub fn lay_out(document: &Document, metrics: &dyn FontMetrics, options: Options) -> Pages {
    let resolver = Resolver::new(document);
    let references = notes::references(document);
    let footnote_options = notes::NoteOptions::of(document, false);
    let endnote_options = notes::NoteOptions::of(document, true);
    let (footnote_numbers, footnote_marks) =
        notes::number(&references, false, &footnote_options, None);
    let (endnote_numbers, endnote_marks) = notes::number(&references, true, &endnote_options, None);
    let mut ctx = Ctx {
        document,
        resolver: &resolver,
        metrics,
        default_tab: default_tab(document),
        show_marks: options.show_marks,
        footnote_numbers,
        endnote_numbers,
        footnote_marks,
        endnote_marks,
        part: document.main.clone(),
        page: None,
    };
    let sections = sections(document);
    let mut pages = flow_document(&mut ctx, &sections, &references, endnote_options.section_end);
    // Footnotes numbered again on each page can only be numbered once the pages are known, and a mark
    // of another width can move a line, so the body is laid out a second time with those numbers.
    if footnote_options.restart == notes::Restart::EachPage {
        let mut on: HashMap<i64, usize> = HashMap::new();
        for (index, page) in pages.iter().enumerate() {
            for piece in page.notes.iter().filter(|p| !p.continued) {
                on.entry(piece.id).or_insert(index);
            }
        }
        let (_, marks) = notes::number(&references, false, &footnote_options, Some(&on));
        if marks != ctx.footnote_marks {
            ctx.footnote_marks = marks;
            pages = flow_document(&mut ctx, &sections, &references, endnote_options.section_end);
        }
    }
    decorate_pages(&mut pages, &mut ctx, &sections);
    Pages { pages }
}

/// The body flowed into pages, with the endnotes at the end of each section or of the document.
fn flow_document(
    ctx: &mut Ctx,
    sections: &[(Vec<(usize, &Block)>, Geometry)],
    references: &[(bool, i64, usize)],
    endnotes_by_section: bool,
) -> Vec<Page> {
    let document = ctx.document;
    let mut flow = Flow::new(document);
    flow.note_lines = measure_footnotes(ctx);
    flow.bands = measure_bands(ctx);
    let mut counters = Counters::default();
    for (index, (blocks, geometry)) in sections.iter().enumerate() {
        flow.start_section(index, geometry.clone());
        for (offset, block) in blocks.iter() {
            flow_block(&mut flow, ctx, &mut counters, block, vec![*offset as u32]);
        }
        if endnotes_by_section {
            let ids: Vec<i64> = references
                .iter()
                .filter(|(endnote, _, section)| *endnote && *section == index)
                .map(|(_, id, _)| *id)
                .collect();
            flow.place_endnotes(ctx, &ids);
        }
    }
    if !endnotes_by_section {
        let ids: Vec<i64> =
            references.iter().filter(|(endnote, ..)| *endnote).map(|(_, id, _)| *id).collect();
        flow.place_endnotes(ctx, &ids);
    }
    flow.finish()
}

/// The height of every section's header and footer of each kind, which push the body down or up when
/// they reach past the margins.
fn measure_bands(ctx: &mut Ctx) -> HashMap<(usize, String), (f32, f32)> {
    let mut out = HashMap::new();
    for (index, (_, geometry)) in sections(ctx.document).iter().enumerate() {
        for kind in ["default", "first", "even"] {
            let mut heights = [0.0f32; 2];
            for (slot, references) in [&geometry.headers, &geometry.footers].iter().enumerate() {
                let Some((_, id)) = references.iter().find(|(k, _)| k == kind) else { continue };
                let Some(header) = ctx.document.headers.iter().find(|h| &h.id == id) else {
                    continue;
                };
                let saved = std::mem::replace(&mut ctx.part, header.name.clone());
                ctx.page = Some((1, 1));
                let mut counters = Counters::default();
                let chunks = lay_out_story(
                    ctx,
                    &mut counters,
                    &header.story.blocks,
                    &StoryId::Header(id.clone()),
                    geometry.text_width(),
                );
                ctx.page = None;
                ctx.part = saved;
                heights[slot] = chunks.iter().map(|c| c.height).sum();
            }
            out.insert((index, kind.to_owned()), (heights[0], heights[1]));
        }
    }
    out
}

/// The body split into its sections: the blocks of each, by their index in the body, and its page.
fn sections(document: &Document) -> Vec<(Vec<(usize, &Block)>, Geometry)> {
    let mut out = Vec::new();
    let mut current: Vec<(usize, &Block)> = Vec::new();
    for (index, block) in document.body.iter().enumerate() {
        current.push((index, block));
        if let Block::Paragraph(paragraph) = block {
            if let Some(sect) = paragraph.props.child("w:sectPr") {
                out.push((std::mem::take(&mut current), Geometry::of(sect)));
            }
        }
    }
    out.push((current, Geometry::of(&document.section)));
    // A section that names no header inherits the previous section's, which is what Word does.
    for index in 1..out.len() {
        let (before_headers, before_footers) =
            (out[index - 1].1.headers.clone(), out[index - 1].1.footers.clone());
        let geometry = &mut out[index].1;
        for (kind, id) in before_headers {
            if !geometry.headers.iter().any(|(k, _)| *k == kind) {
                geometry.headers.push((kind, id));
            }
        }
        for (kind, id) in before_footers {
            if !geometry.footers.iter().any(|(k, _)| *k == kind) {
                geometry.footers.push((kind, id));
            }
        }
    }
    out
}

/// Lay one body block out into the flow.
fn flow_block(flow: &mut Flow, ctx: &Ctx, counters: &mut Counters, block: &Block, path: Path) {
    match block {
        Block::Paragraph(paragraph) => {
            let width = flow.column_width();
            let laid =
                paragraph::lay_out(ctx, counters, paragraph, &StoryId::Body, &path, width, None);
            flow.place_paragraph(laid);
        }
        Block::Table(table) => {
            let width = flow.column_width();
            let rows = table::lay_out(ctx, counters, table, &StoryId::Body, &path, width);
            flow.place_table(rows);
        }
        Block::Sdt(sdt) => {
            for (index, inner) in sdt.blocks.iter().enumerate() {
                let mut inner_path = path.clone();
                inner_path.push(index as u32);
                flow_block(flow, ctx, counters, inner, inner_path);
            }
        }
        Block::Kept(_) => {}
    }
}

/// A paragraph laid out, ready to be placed.
#[derive(Debug, Clone)]
pub struct LaidParagraph {
    pub lines: Vec<Chunk>,
    pub space_before: f32,
    pub space_after: f32,
    pub keep_next: bool,
    pub keep_lines: bool,
    pub page_break_before: bool,
    pub widow_control: bool,
    pub contextual: bool,
    pub style: String,
}

/// Placing pieces down the columns of pages.
struct Flow<'a> {
    document: &'a Document,
    pages: Vec<Page>,
    geometry: Geometry,
    section: usize,
    column: usize,
    /// Where the next piece goes on the current page.
    y: f32,
    /// The top of the current column, which moves down after a continuous section break.
    column_top: f32,
    /// The space after the last paragraph placed, added before the next.
    pending_after: f32,
    last_style: Option<(String, bool)>,
    /// The height the footnotes at the foot of the current page take.
    notes_height: f32,
    /// The footnotes on the current page and the lines of each that are here.
    page_notes: Vec<NotePiece>,
    /// The lines of footnotes that did not fit on their page, which go first at the foot of the next.
    carry: Vec<(i64, usize)>,
    /// Each footnote's line heights, measured once at the text width.
    note_lines: HashMap<i64, Vec<f32>>,
    /// Paragraphs with keep with next, waiting for the block after them.
    keep_chain: Vec<LaidParagraph>,
    /// The height of each section's header and footer by kind (`default`, `first`, `even`).
    bands: HashMap<(usize, String), (f32, f32)>,
    even_and_odd: bool,
    /// The current page's number as Word prints it, and how many pages its section has so far.
    page_number: u32,
    section_pages: u32,
    /// Where the current page's body starts, and how far above the foot it ends.
    page_top: f32,
    page_bottom: f32,
}

impl<'a> Flow<'a> {
    fn new(document: &'a Document) -> Flow<'a> {
        Flow {
            document,
            pages: Vec::new(),
            geometry: Geometry::of(&document.section),
            section: 0,
            column: 0,
            y: 0.0,
            column_top: 0.0,
            pending_after: 0.0,
            last_style: None,
            notes_height: 0.0,
            page_notes: Vec::new(),
            carry: Vec::new(),
            note_lines: HashMap::new(),
            keep_chain: Vec::new(),
            bands: HashMap::new(),
            even_and_odd: document
                .settings
                .as_ref()
                .and_then(|s| crate::props::on_off(&s.root, "w:evenAndOddHeaders"))
                .unwrap_or(false),
            page_number: 0,
            section_pages: 0,
            page_top: 72.0,
            page_bottom: 72.0,
        }
    }

    fn column_width(&self) -> f32 {
        self.geometry.columns.get(self.column).map_or(self.geometry.text_width(), |c| c.width)
    }

    fn column_left(&self) -> f32 {
        let mut x = self.geometry.left + self.geometry.gutter;
        for column in self.geometry.columns.iter().take(self.column) {
            x += column.width + column.space;
        }
        x
    }

    fn bottom(&self) -> f32 {
        self.geometry.height - self.page_bottom - self.notes_height
    }

    /// The height a page's body has between its header and its footer.
    fn room(&self) -> f32 {
        self.geometry.height - self.page_top - self.page_bottom
    }

    /// Where the body starts and how far from the foot it ends on the next page: the margins, or the
    /// header and footer when they reach past them (task-2359, C4).
    fn bands_of_next_page(&self) -> (f32, f32) {
        let number = match (self.section_pages, self.geometry.page_number_start) {
            (0, Some(start)) => start,
            _ => self.page_number + 1,
        };
        let kind = if self.section_pages == 0 && self.geometry.title_page {
            "first"
        } else if self.even_and_odd && number % 2 == 0 {
            "even"
        } else {
            "default"
        };
        let (header, footer) =
            self.bands.get(&(self.section, kind.to_owned())).copied().unwrap_or((0.0, 0.0));
        let top = self.geometry.top.max(self.geometry.header + header);
        let bottom = self.geometry.bottom.max(self.geometry.footer + footer);
        (top, bottom)
    }

    fn new_page(&mut self) {
        let (top, bottom) = self.bands_of_next_page();
        self.page_number = match (self.section_pages, self.geometry.page_number_start) {
            (0, Some(start)) => start,
            _ => self.page_number + 1,
        };
        self.section_pages += 1;
        self.page_top = top;
        self.page_bottom = bottom;
        let number = self.pages.len() as u32 + 1;
        let geometry = &self.geometry;
        let body = Rect::new(
            geometry.left + geometry.gutter,
            top,
            geometry.text_width(),
            geometry.height - top - bottom,
        );
        let background = self
            .document
            .background
            .as_ref()
            .and_then(|bg| bg.attr("w:color"))
            .and_then(Rgb::parse);
        self.pages.push(Page {
            width: geometry.width,
            height: geometry.height,
            number,
            section: self.section,
            body,
            items: Vec::new(),
            carets: Vec::new(),
            first: None,
            background,
            notes: Vec::new(),
        });
        self.column = 0;
        self.y = top;
        self.column_top = top;
        self.pending_after = 0.0;
        self.notes_height = 0.0;
        self.page_notes.clear();
        // What a footnote on the last page could not fit goes first at the foot of this one, under the
        // continuation separator (task-2359, C8).
        let carried = std::mem::take(&mut self.carry);
        for (index, (id, from)) in carried.into_iter().enumerate() {
            let separator = if index == 0 { NOTE_SEPARATOR } else { 0.0 };
            self.take_note_lines(id, from, separator, true);
        }
    }

    fn start_section(&mut self, index: usize, geometry: Geometry) {
        // A section break ends a keep with next chain: nothing after it keeps with what is before it.
        self.flush_keeps(None);
        self.section_pages = 0;
        let start = geometry.start;
        let continuous = start == Start::Continuous
            && !self.pages.is_empty()
            && geometry.width == self.geometry.width
            && geometry.height == self.geometry.height;
        self.section = index;
        self.geometry = geometry;
        if self.pages.is_empty() {
            self.new_page();
            return;
        }
        match start {
            _ if continuous => {
                // The new section's columns start below what is already on the page.
                self.column = 0;
                self.column_top = self.y;
            }
            Start::NextColumn if self.column + 1 < self.geometry.columns.len() => {
                self.column += 1;
                self.y = self.column_top;
            }
            Start::EvenPage | Start::OddPage => {
                self.new_page();
                let want_even = start == Start::EvenPage;
                if (self.pages.len() % 2 == 0) != want_even {
                    self.new_page();
                }
            }
            _ => self.new_page(),
        }
        if let Some(page) = self.pages.last_mut() {
            page.section = index;
        }
    }

    /// Whether nothing has been placed in the current column yet.
    fn at_top(&self) -> bool {
        (self.y - self.column_top).abs() < 0.01
    }

    fn next_column(&mut self) {
        if self.column + 1 < self.geometry.columns.len() {
            self.column += 1;
            self.y = self.column_top;
            self.pending_after = 0.0;
        } else {
            self.new_page();
        }
    }

    /// The footnotes a piece refers to that are not on this page yet.
    fn new_notes(&self, chunk: &Chunk) -> Vec<i64> {
        let mut new: Vec<i64> = Vec::new();
        for id in &chunk.notes {
            let here = self.page_notes.iter().any(|piece| piece.id == *id);
            let waiting = self.carry.iter().any(|(carried, _)| carried == id);
            if !here && !waiting && !new.contains(id) {
                new.push(*id);
            }
        }
        new
    }

    /// The height a piece's new footnotes need on this page: all of them, and the least Word accepts,
    /// which is every one but the last whole and the first line of the last.
    fn notes_for(&self, chunk: &Chunk) -> (f32, f32) {
        let new = self.new_notes(chunk);
        if new.is_empty() {
            return (0.0, 0.0);
        }
        let separator = if self.page_notes.is_empty() { NOTE_SEPARATOR } else { 0.0 };
        let lines = |id: &i64| self.note_lines.get(id).cloned().unwrap_or_else(|| vec![14.0]);
        let full = separator + new.iter().map(|id| lines(id).iter().sum::<f32>()).sum::<f32>();
        let least = separator
            + new[..new.len() - 1].iter().map(|id| lines(id).iter().sum::<f32>()).sum::<f32>()
            + lines(new.last().expect("one")).first().copied().unwrap_or(0.0);
        (full, least)
    }

    /// Put as many lines of a footnote as fit at the foot of this page, from line `from`, and keep the
    /// rest for the next. At least one line goes here.
    fn take_note_lines(&mut self, id: i64, from: usize, separator: f32, continued: bool) {
        let lines = self.note_lines.get(&id).cloned().unwrap_or_else(|| vec![14.0]);
        let room = self.bottom() - self.y - separator;
        let mut used = 0.0;
        let mut to = from;
        while to < lines.len() && (used + lines[to] <= room + 0.01 || to == from) {
            used += lines[to];
            to += 1;
        }
        self.notes_height += separator + used;
        self.page_notes.push(NotePiece { id, from, to, continued });
        if let Some(page) = self.pages.last_mut() {
            page.notes.push(NotePiece { id, from, to, continued });
        }
        if to < lines.len() {
            self.carry.push((id, to));
        }
    }

    /// Put one piece at the current place, moving to the next column first when it does not fit with
    /// its footnotes. A piece whose last footnote does not fit whole stays when the note's first line
    /// does, and the note continues on the next page.
    fn put(&mut self, mut chunk: Chunk) {
        let (full, least) = self.notes_for(&chunk);
        if self.y + chunk.height + full > self.bottom() + 0.01 && !self.at_top() {
            let continues = full > least && self.y + chunk.height + least <= self.bottom() + 0.01;
            if !continues {
                self.next_column();
            }
        }
        let (x, y, column_width) = (self.column_left(), self.y, self.column_width());
        chunk.shift(x, y);
        let new = self.new_notes(&chunk);
        self.y += chunk.height;
        for id in new {
            let separator = if self.page_notes.is_empty() { NOTE_SEPARATOR } else { 0.0 };
            self.take_note_lines(id, 0, separator, false);
        }
        self.y -= chunk.height;
        let page = self.pages.last_mut().expect("a page");
        if page.first.is_none() {
            if let Some(line) = chunk.carets.iter().find(|line| line.story == StoryId::Body) {
                page.first =
                    Some(Pos { story: StoryId::Body, path: line.path.clone(), offset: line.start });
            }
        }
        for float in &chunk.floats {
            let item = place_float(float, page, x, y, column_width);
            page.items.push(item);
        }
        page.items.extend(chunk.items);
        page.carets.extend(chunk.carets);
        self.y += chunk.height;
        match chunk.break_after {
            Some(BreakAfter::Page) => self.new_page(),
            Some(BreakAfter::Column) => self.next_column(),
            None => {}
        }
    }

    /// The space Word leaves between two paragraphs: none between two of the same style that ask for
    /// contextual spacing, otherwise the larger of the space after the first and before the second.
    /// Measured against Word 16, a 6 line paragraph and the heading under it are 8 points apart, which is
    /// the larger of the two 8s and not their sum.
    fn gap(after: f32, last: &Option<(String, bool)>, laid: &LaidParagraph) -> f32 {
        let same = last
            .as_ref()
            .is_some_and(|(style, contextual)| *style == laid.style && *contextual)
            && laid.contextual;
        if same {
            0.0
        } else {
            after.max(laid.space_before)
        }
    }

    /// How much of a paragraph has to be on one page with what comes before it: its first line, its first
    /// two with widow control, or all of it when its lines are kept together.
    fn start_height(laid: &LaidParagraph) -> f32 {
        let lines = if laid.keep_lines {
            laid.lines.len()
        } else if laid.widow_control {
            2
        } else {
            1
        };
        laid.lines.iter().take(lines).map(|l| l.height).sum()
    }

    /// Place a paragraph. One with keep with next waits for the block after it, so the two can be put on
    /// one page together (task-2359, C3).
    fn place_paragraph(&mut self, laid: LaidParagraph) {
        if laid.keep_next && !laid.lines.is_empty() {
            self.keep_chain.push(laid);
            return;
        }
        if !laid.page_break_before {
            let after = self.keep_chain.last().map_or(0.0, |p| p.space_after);
            let last = self.keep_chain.last().map(|p| (p.style.clone(), p.contextual));
            let next = Self::gap(after, &last, &laid) + Self::start_height(&laid);
            self.flush_keeps(Some(next));
        } else {
            self.flush_keeps(None);
        }
        self.place_paragraph_now(laid, None);
    }

    /// Place the paragraphs waiting with keep with next, now that the block after them is known: `next`
    /// is the height of that block's start, with the gap above it, or `None` when nothing has to come with
    /// them. Word moves the chain to the next page when the last lines of its first paragraph, the rest
    /// of the chain and the start of the next block do not fit on this one, and gives up when they would
    /// not fit on any page.
    fn flush_keeps(&mut self, next: Option<f32>) {
        let chain = std::mem::take(&mut self.keep_chain);
        if chain.is_empty() {
            return;
        }
        let Some(next) = next else {
            for laid in chain {
                self.place_paragraph_now(laid, None);
            }
            return;
        };
        let first = &chain[0];
        let tail = if first.keep_lines {
            first.lines.len()
        } else if first.widow_control {
            first.lines.len().min(2)
        } else {
            1
        };
        let mut together: f32 =
            first.lines.iter().rev().take(tail).map(|l| l.height).sum::<f32>() + next;
        for pair in chain.windows(2) {
            let last = Some((pair[0].style.clone(), pair[0].contextual));
            together += Self::gap(pair[0].space_after, &last, &pair[1]);
            together += pair[1].lines.iter().map(|l| l.height).sum::<f32>();
        }
        let room = self.room();
        let keep = (together <= room + 0.01).then_some((first.lines.len() - tail, together));
        for (index, laid) in chain.into_iter().enumerate() {
            self.place_paragraph_now(laid, if index == 0 { keep } else { None });
        }
    }

    /// Place a paragraph's lines. `keep` is the line from which the rest of a keep with next chain has
    /// to fit on the page, and how tall that rest is.
    fn place_paragraph_now(&mut self, laid: LaidParagraph, keep: Option<(usize, f32)>) {
        if laid.page_break_before && !self.at_top() {
            self.new_page();
        }
        let gap = Self::gap(self.pending_after, &self.last_style, &laid);
        // Space before a paragraph at the top of a page is dropped, except on the first page.
        let gap = if self.at_top() && self.pages.len() > 1 { 0.0 } else { gap };
        let count = laid.lines.len();
        let first_lines: f32 =
            laid.lines.iter().take(if laid.widow_control { 2 } else { 1 }).map(|l| l.height).sum();
        let whole: f32 = laid.lines.iter().map(|l| l.height).sum();
        // Keep the whole paragraph together when asked, and at least its first two lines when widow
        // control is on, by starting it on the next column when they would not fit here.
        let mut must_fit = if laid.keep_lines { whole } else { first_lines };
        if let Some((0, together)) = keep {
            must_fit = must_fit.max(together);
        }
        if !self.at_top() && self.y + gap + must_fit > self.bottom() + 0.01 {
            let fits_fresh =
                must_fit <= self.room();
            if fits_fresh {
                self.next_column();
            }
        }
        let gap = if self.at_top() && self.pages.len() > 1 { 0.0 } else { gap };
        self.y += gap;
        let mut lines = laid.lines.into_iter().enumerate().peekable();
        while let Some((index, line)) = lines.next() {
            // The last lines of a paragraph kept with the next go to the next page with it.
            if let Some((from, together)) = keep {
                if from > 0 && index == from && !self.at_top() && self.y + together > self.bottom() + 0.01
                {
                    self.next_column();
                }
            }
            // Widow control: the last line is not left alone at the top of a page, so when the
            // second last line is the last one that fits, it moves with it.
            if laid.widow_control && count >= 3 && index == count - 2 {
                let next_height = lines.peek().map_or(0.0, |(_, l)| l.height);
                if self.y + line.height <= self.bottom() + 0.01
                    && self.y + line.height + next_height > self.bottom() + 0.01
                {
                    self.next_column();
                }
            }
            self.put(line);
        }
        self.pending_after = laid.space_after;
        self.last_style = Some((laid.style, laid.contextual));
    }

    fn place_table(&mut self, rows: Vec<table::LaidRow>) {
        let first = rows.first().map_or(0.0, |row| row.chunk.height);
        let after = self.keep_chain.last().map_or(self.pending_after, |p| p.space_after);
        self.flush_keeps(Some(after + first));
        self.y += self.pending_after;
        self.pending_after = 0.0;
        let headers: Vec<Chunk> =
            rows.iter().take_while(|row| row.header).map(|row| row.chunk.clone()).collect();
        for (index, row) in rows.into_iter().enumerate() {
            let mut row = row;
            // Whether the column was just started for this table, with only repeated headers in it.
            let mut fresh = self.at_top();
            loop {
                if self.y + row.chunk.height <= self.bottom() + 0.01 {
                    self.put(row.chunk);
                    break;
                }
                // A row that does not fit is cut between its cells' lines, unless it may not be cut
                // and a fresh page would hold it whole (task-2359, C1).
                let room = self.bottom() - self.y;
                let page = self.room();
                let may_cut = !row.cant_split || row.chunk.height > page;
                if may_cut {
                    if let Some((first, rest)) = row.split(room) {
                        self.put(first);
                        self.next_column();
                        self.repeat_headers(&headers);
                        row = rest;
                        fresh = true;
                        continue;
                    }
                }
                if fresh || self.at_top() {
                    // Nothing fits, not even a line: the row overflows this page as Word lets it.
                    self.put(row.chunk);
                    break;
                }
                self.next_column();
                // Header rows repeat at the top of each page the table continues on.
                if !row.header && index > 0 {
                    self.repeat_headers(&headers);
                }
                fresh = true;
            }
        }
        self.last_style = None;
    }

    /// Put copies of a table's header rows at the top of a page it continues on.
    fn repeat_headers(&mut self, headers: &[Chunk]) {
        for header in headers {
            let mut copy = header.clone();
            copy.carets.clear();
            self.put(copy);
        }
    }

    /// The endnotes `ids` after the text placed so far, under a separator.
    fn place_endnotes(&mut self, ctx: &Ctx, ids: &[i64]) {
        self.flush_keeps(None);
        let Some(part) = &self.document.endnotes else { return };
        let numbered: Vec<(u32, i64)> = ids.iter().map(|id| (0, *id)).collect();
        if numbered.is_empty() {
            return;
        }
        let mut counters = Counters::default();
        let width = self.column_width();
        let mut separator = Chunk { height: 12.0, ..Chunk::default() };
        separator.items.push(Item::Rule {
            x0: 0.0,
            y0: 6.0,
            x1: 144.0,
            y1: 6.0,
            width: 0.5,
            color: Rgb::BLACK,
            kind: RuleKind::Solid,
        });
        self.put(separator);
        for (_, id) in numbered {
            let Some(note) = part.notes.iter().find(|n| n.id() == Some(id)) else { continue };
            let story = StoryId::Endnote(id);
            paragraph::CURRENT_NOTE.with(|current| current.set(Some(id)));
            let chunks = lay_out_story(ctx, &mut counters, &note.blocks, &story, width);
            paragraph::CURRENT_NOTE.with(|current| current.set(None));
            for chunk in chunks {
                self.put(chunk);
            }
        }
    }

    fn finish(mut self) -> Vec<Page> {
        self.flush_keeps(None);
        // Footnote lines still waiting get pages of their own.
        while !self.carry.is_empty() {
            self.new_page();
        }
        self.pages
    }
}

/// The height of the line above the footnotes.
const NOTE_SEPARATOR: f32 = 10.0;

/// Every footnote's line heights at the first section's text width.
fn measure_footnotes(ctx: &Ctx) -> HashMap<i64, Vec<f32>> {
    let Some(part) = &ctx.document.footnotes else { return HashMap::new() };
    let width = sections(ctx.document).first().map_or(468.0, |(_, g)| g.text_width());
    let mut counters = Counters::default();
    part.notes
        .iter()
        .filter_map(|note| {
            let id = note.id()?;
            paragraph::CURRENT_NOTE.with(|current| current.set(Some(id)));
            let chunks =
                lay_out_story(ctx, &mut counters, &note.blocks, &StoryId::Footnote(id), width);
            paragraph::CURRENT_NOTE.with(|current| current.set(None));
            Some((id, chunks.iter().map(|c| c.height).collect()))
        })
        .collect()
}

/// Where a float lands on the page, from its anchor.
fn place_float(float: &Float, page: &Page, column_x: f32, line_y: f32, column_width: f32) -> Item {
    let resolve = |(relative, anchor): &(String, Anchor),
                   page_size: f32,
                   margin_start: f32,
                   margin_size: f32,
                   line: f32,
                   size: f32|
     -> f32 {
        let (origin, extent) = match relative.as_str() {
            "page" => (0.0, page_size),
            "margin" | "column" => (margin_start, margin_size),
            "paragraph" | "line" | "character" => (line, margin_size),
            "leftMargin" | "topMargin" => (0.0, margin_start),
            _ => (margin_start, margin_size),
        };
        match anchor {
            Anchor::Offset(offset) => origin + offset,
            Anchor::Align(align) => match align.as_str() {
                "center" => origin + (extent - size) / 2.0,
                "right" | "bottom" | "outside" => origin + extent - size,
                _ => origin,
            },
        }
    };
    let x = resolve(&float.horizontal, page.width, column_x, column_width, column_x, float.width);
    let y = resolve(&float.vertical, page.height, page.body.y, page.body.h, line_y, float.height);
    Item::Picture { rect: Rect::new(x, y, float.width, float.height), part: float.part.clone() }
}

/// Lay a story out as a stack of pieces, for a header, a footer, a note or a cell.
pub fn lay_out_story(
    ctx: &Ctx,
    counters: &mut Counters,
    blocks: &[Block],
    story: &StoryId,
    width: f32,
) -> Vec<Chunk> {
    let mut out = Vec::new();
    let mut pending_after = 0.0;
    lay_out_blocks(
        ctx,
        counters,
        blocks,
        story,
        &mut Vec::new(),
        width,
        None,
        &mut out,
        &mut pending_after,
    );
    out
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lay_out_blocks(
    ctx: &Ctx,
    counters: &mut Counters,
    blocks: &[Block],
    story: &StoryId,
    prefix: &mut Path,
    width: f32,
    table_style: Option<&str>,
    out: &mut Vec<Chunk>,
    pending_after: &mut f32,
) {
    for (index, block) in blocks.iter().enumerate() {
        prefix.push(index as u32);
        match block {
            Block::Paragraph(paragraph) => {
                let laid =
                    paragraph::lay_out(ctx, counters, paragraph, story, prefix, width, table_style);
                let gap = pending_after.max(laid.space_before);
                for (line_index, mut line) in laid.lines.into_iter().enumerate() {
                    if line_index == 0 && gap > 0.0 {
                        line.shift(0.0, gap);
                        line.height += gap;
                    }
                    out.push(line);
                }
                *pending_after = laid.space_after;
            }
            Block::Table(table) => {
                for row in table::lay_out(ctx, counters, table, story, prefix, width) {
                    out.push(row.chunk);
                }
                *pending_after = 0.0;
            }
            Block::Sdt(sdt) => {
                lay_out_blocks(
                    ctx,
                    counters,
                    &sdt.blocks,
                    story,
                    prefix,
                    width,
                    table_style,
                    out,
                    pending_after,
                );
            }
            Block::Kept(_) => {}
        }
        prefix.pop();
    }
}

/// Stack pieces into one, top to bottom.
pub fn stack(chunks: Vec<Chunk>) -> Chunk {
    let mut whole = Chunk::default();
    for chunk in chunks {
        let y = whole.height;
        whole.height += chunk.height;
        whole.absorb(chunk, 0.0, y);
    }
    whole
}

/// Headers, footers and footnotes, once the pages are known.
fn decorate_pages(
    pages: &mut [Page],
    ctx: &mut Ctx,
    sections: &[(Vec<(usize, &Block)>, Geometry)],
) {
    let total = pages.len() as u32;
    let even_odd = ctx
        .document
        .settings
        .as_ref()
        .and_then(|s| crate::props::on_off(&s.root, "w:evenAndOddHeaders"))
        .unwrap_or(false);
    let mut first_of_section: HashMap<usize, usize> = HashMap::new();
    let mut number = 0u32;
    for (index, page) in pages.iter_mut().enumerate() {
        let section = page.section;
        let geometry =
            &sections.get(section).map_or_else(|| sections[0].1.clone(), |(_, g)| g.clone());
        let is_first = !first_of_section.contains_key(&section);
        first_of_section.entry(section).or_insert(index);
        number = match (is_first, geometry.page_number_start) {
            (true, Some(start)) => start,
            _ => number + 1,
        };
        page.number = number;
        let kind = if is_first && geometry.title_page {
            "first"
        } else if even_odd && number % 2 == 0 {
            "even"
        } else {
            "default"
        };
        ctx.page = Some((number, total));
        for (footer, references) in [(false, &geometry.headers), (true, &geometry.footers)] {
            let Some((_, id)) = references.iter().find(|(k, _)| k == kind) else { continue };
            let Some(header) = ctx.document.headers.iter().find(|h| &h.id == id) else { continue };
            let saved = std::mem::replace(&mut ctx.part, header.name.clone());
            let mut counters = Counters::default();
            let story = StoryId::Header(id.clone());
            let chunks = lay_out_story(
                ctx,
                &mut counters,
                &header.story.blocks,
                &story,
                geometry.text_width(),
            );
            ctx.part = saved;
            let block = stack(chunks);
            let y = match footer {
                false => geometry.header,
                true => geometry.height - geometry.footer - block.height,
            };
            let mut block = block;
            block.shift(geometry.left + geometry.gutter, y);
            page.items.extend(block.items);
            page.carets.extend(block.carets);
        }
        lay_out_footnotes(page, ctx, geometry);
    }
    ctx.page = None;
}

/// The footnotes the flow put on a page, at its foot: the lines of each that are on this page, under the
/// separator, or under the continuation separator, the whole width of the text, when the first note
/// began on an earlier page.
fn lay_out_footnotes(page: &mut Page, ctx: &Ctx, geometry: &Geometry) {
    let Some(part) = &ctx.document.footnotes else { return };
    if page.notes.is_empty() {
        return;
    }
    let mut counters = Counters::default();
    let mut chunks = Vec::new();
    let mut separator = Chunk { height: NOTE_SEPARATOR, ..Chunk::default() };
    let length = if page.notes[0].continued { geometry.text_width() } else { 144.0 };
    separator.items.push(Item::Rule {
        x0: 0.0,
        y0: 5.0,
        x1: length,
        y1: 5.0,
        width: 0.5,
        color: Rgb::BLACK,
        kind: RuleKind::Solid,
    });
    chunks.push(separator);
    for piece in page.notes.clone() {
        let Some(note) = part.notes.iter().find(|n| n.id() == Some(piece.id)) else { continue };
        paragraph::CURRENT_NOTE.with(|current| current.set(Some(piece.id)));
        let lines = lay_out_story(
            ctx,
            &mut counters,
            &note.blocks,
            &StoryId::Footnote(piece.id),
            geometry.text_width(),
        );
        paragraph::CURRENT_NOTE.with(|current| current.set(None));
        chunks.extend(
            lines.into_iter().skip(piece.from).take(piece.to.saturating_sub(piece.from)),
        );
    }
    let mut block = stack(chunks);
    // At the foot of the body, which a tall footer can have moved up from the margin.
    let y = page.body.bottom() - block.height;
    block.shift(geometry.left + geometry.gutter, y);
    page.items.extend(block.items);
    page.carets.extend(block.carets);
}
