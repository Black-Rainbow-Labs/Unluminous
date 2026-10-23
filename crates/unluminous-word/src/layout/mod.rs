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
    Fill { rect: Rect, color: Rgb },
    /// A horizontal or vertical line from `(x0, y0)` to `(x1, y1)`.
    Rule { x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: Rgb, kind: RuleKind },
    /// A picture, by the package part it is in.
    Picture { rect: Rect, part: String },
    /// Something Unluminous does not draw, shown as a box with a word in it.
    Placeholder { rect: Rect, label: String },
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
            Item::Fill { rect, .. } | Item::Picture { rect, .. } | Item::Placeholder { rect, .. } => {
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
        offset >= self.start && (offset < self.end || (self.last && offset == self.end) || self.start == self.end)
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
        let line = page
            .carets
            .iter()
            .filter(|line| stories.contains(&line.story))
            .min_by(|a, b| {
                let distance = |line: &CaretLine| {
                    let dy = if y < line.top { line.top - y } else if y > line.top + line.height { y - line.top - line.height } else { 0.0 };
                    let left = line.stops.first().map_or(0.0, |s| s.1);
                    let right = line.stops.last().map_or(0.0, |s| s.1);
                    let dx = if x < left { left - x } else if x > right { x - right } else { 0.0 };
                    dy * 1000.0 + dx
                };
                distance(a).total_cmp(&distance(b))
            })?;
        Some(Pos { story: line.story.clone(), path: line.path.clone(), offset: line.offset_at(x) })
    }
}

/// Number the footnotes and endnotes in order of first reference.
fn number_notes(document: &Document) -> (HashMap<i64, u32>, HashMap<i64, u32>) {
    let mut footnotes = HashMap::new();
    let mut endnotes = HashMap::new();
    for (_, paragraph) in crate::position::paragraphs(&document.body) {
        for run in &paragraph.runs {
            if let crate::model::Content::Object(el) = &run.content {
                let id = el.attr("w:id").and_then(|id| id.parse::<i64>().ok());
                // A reference with `w:customMarkFollows` shows its own mark and is not counted.
                let custom = el.attr("w:customMarkFollows").is_some_and(|v| v == "1" || v == "true");
                match (el.name.as_str(), id) {
                    ("w:footnoteReference", Some(id)) if !custom => {
                        let next = footnotes.len() as u32 + 1;
                        footnotes.entry(id).or_insert(next);
                    }
                    ("w:endnoteReference", Some(id)) if !custom => {
                        let next = endnotes.len() as u32 + 1;
                        endnotes.entry(id).or_insert(next);
                    }
                    _ => {}
                }
            }
        }
    }
    (footnotes, endnotes)
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
    let (footnote_numbers, endnote_numbers) = number_notes(document);
    let mut ctx = Ctx {
        document,
        resolver: &resolver,
        metrics,
        default_tab: default_tab(document),
        show_marks: options.show_marks,
        footnote_numbers,
        endnote_numbers,
        part: document.main.clone(),
        page: None,
    };
    let mut flow = Flow::new(document);
    flow.note_heights = measure_footnotes(&ctx);
    let mut counters = Counters::default();
    let sections = sections(document);
    for (index, (blocks, geometry)) in sections.iter().enumerate() {
        flow.start_section(index, geometry.clone());
        for (offset, block) in blocks.iter() {
            flow_block(&mut flow, &ctx, &mut counters, block, vec![*offset as u32]);
        }
    }
    flow.place_endnotes(&ctx);
    let mut pages = flow.finish();
    decorate_pages(&mut pages, &mut ctx, &sections);
    Pages { pages }
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
        let (before_headers, before_footers) = (out[index - 1].1.headers.clone(), out[index - 1].1.footers.clone());
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
            let laid = paragraph::lay_out(ctx, counters, paragraph, &StoryId::Body, &path, width, None);
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
    /// The footnotes on the current page, laid out, and their total height.
    notes_height: f32,
    page_note_ids: Vec<i64>,
    /// Each footnote's height, measured once at the text width.
    note_heights: HashMap<i64, f32>,
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
            page_note_ids: Vec::new(),
            note_heights: HashMap::new(),
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
        self.geometry.height - self.geometry.bottom - self.notes_height
    }

    fn new_page(&mut self) {
        let number = self.pages.len() as u32 + 1;
        let geometry = &self.geometry;
        let body = Rect::new(
            geometry.left + geometry.gutter,
            geometry.top,
            geometry.text_width(),
            geometry.height - geometry.top - geometry.bottom,
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
        });
        self.column = 0;
        self.y = geometry.top;
        self.column_top = geometry.top;
        self.pending_after = 0.0;
        self.notes_height = 0.0;
        self.page_note_ids.clear();
    }

    fn start_section(&mut self, index: usize, geometry: Geometry) {
        let start = geometry.start;
        let continuous = start == Start::Continuous && !self.pages.is_empty()
            && geometry.width == self.geometry.width && geometry.height == self.geometry.height;
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

    /// The height a piece's footnotes need on this page, beyond those already here.
    fn notes_for(&self, chunk: &Chunk) -> f32 {
        let new: Vec<i64> = chunk.notes.iter().copied().filter(|id| !self.page_note_ids.contains(id)).collect();
        if new.is_empty() {
            return 0.0;
        }
        // Measured before the body was laid out, at the width of the text.
        let separator = if self.page_note_ids.is_empty() { NOTE_SEPARATOR } else { 0.0 };
        separator + new.iter().map(|id| self.note_heights.get(id).copied().unwrap_or(14.0)).sum::<f32>()
    }

    /// Put one piece at the current place, moving to the next column first when it does not fit.
    fn put(&mut self, mut chunk: Chunk) {
        let need = chunk.height + self.notes_for(&chunk);
        if self.y + need > self.bottom() + 0.01 && !self.at_top() {
            self.next_column();
        }
        let (x, y, column_width) = (self.column_left(), self.y, self.column_width());
        chunk.shift(x, y);
        self.notes_height += self.notes_for(&chunk);
        self.page_note_ids.extend(chunk.notes.iter().copied());
        let page = self.pages.last_mut().expect("a page");
        if page.first.is_none() {
            if let Some(line) = chunk.carets.iter().find(|line| line.story == StoryId::Body) {
                page.first = Some(Pos { story: StoryId::Body, path: line.path.clone(), offset: line.start });
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

    fn place_paragraph(&mut self, laid: LaidParagraph) {
        if laid.page_break_before && !self.at_top() {
            self.new_page();
        }
        let same = self.last_style.as_ref().is_some_and(|(style, contextual)| *style == laid.style && *contextual)
            && laid.contextual;
        let after = if same { 0.0 } else { self.pending_after };
        let before = if same { 0.0 } else { laid.space_before };
        // Space before a paragraph at the top of a page is dropped, except on the first page.
        // Word keeps the larger of the space after one paragraph and the space before the next, rather
        // than both: measured against Word 16, a 6 line paragraph and the heading under it are 8 points
        // apart, which is the larger of the two 8s and not their sum.
        let gap = if self.at_top() && self.pages.len() > 1 { 0.0 } else { after.max(before) };
        let count = laid.lines.len();
        let first_lines: f32 = laid.lines.iter().take(if laid.widow_control { 2 } else { 1 }).map(|l| l.height).sum();
        let whole: f32 = laid.lines.iter().map(|l| l.height).sum();
        // Keep the whole paragraph together when asked, and at least its first two lines when widow
        // control is on, by starting it on the next column when they would not fit here.
        let must_fit = if laid.keep_lines { whole } else { first_lines };
        if !self.at_top() && self.y + gap + must_fit > self.bottom() + 0.01 {
            let fits_fresh = must_fit <= self.geometry.height - self.geometry.top - self.geometry.bottom;
            if fits_fresh {
                self.next_column();
            }
        }
        let gap = if self.at_top() && self.pages.len() > 1 { 0.0 } else { gap };
        self.y += gap;
        let mut lines = laid.lines.into_iter().enumerate().peekable();
        while let Some((index, line)) = lines.next() {
            // Widow control: the last line is not left alone at the top of a page, so when the
            // second last line is the last one that fits, it moves with it.
            if laid.widow_control && count >= 3 && index == count - 2 {
                let next_height = lines.peek().map_or(0.0, |(_, l)| l.height);
                if self.y + line.height <= self.bottom() + 0.01 && self.y + line.height + next_height > self.bottom() + 0.01 {
                    self.next_column();
                }
            }
            self.put(line);
        }
        self.pending_after = laid.space_after;
        self.last_style = Some((laid.style, laid.contextual));
    }

    fn place_table(&mut self, rows: Vec<table::LaidRow>) {
        self.y += self.pending_after;
        self.pending_after = 0.0;
        let headers: Vec<Chunk> = rows.iter().take_while(|row| row.header).map(|row| row.chunk.clone()).collect();
        for (index, row) in rows.into_iter().enumerate() {
            if !self.at_top() && self.y + row.chunk.height > self.bottom() + 0.01 {
                self.next_column();
                // Header rows repeat at the top of each page the table continues on.
                if !row.header && index > 0 {
                    for header in &headers {
                        let mut copy = header.clone();
                        copy.carets.clear();
                        self.put(copy);
                    }
                }
            }
            self.put(row.chunk);
        }
        self.last_style = None;
    }

    fn place_endnotes(&mut self, ctx: &Ctx) {
        let Some(part) = &self.document.endnotes else { return };
        let mut numbered: Vec<(u32, i64)> = ctx.endnote_numbers.iter().map(|(id, n)| (*n, *id)).collect();
        numbered.sort();
        if numbered.is_empty() {
            return;
        }
        let mut counters = Counters::default();
        let width = self.column_width();
        let mut separator = Chunk { height: 12.0, ..Chunk::default() };
        separator.items.push(Item::Rule { x0: 0.0, y0: 6.0, x1: 144.0, y1: 6.0, width: 0.5, color: Rgb::BLACK, kind: RuleKind::Solid });
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

    fn finish(self) -> Vec<Page> {
        self.pages
    }
}

/// The height of the line above the footnotes.
const NOTE_SEPARATOR: f32 = 10.0;

/// Every footnote's height at the first section's text width.
fn measure_footnotes(ctx: &Ctx) -> HashMap<i64, f32> {
    let Some(part) = &ctx.document.footnotes else { return HashMap::new() };
    let width = sections(ctx.document).first().map_or(468.0, |(_, g)| g.text_width());
    let mut counters = Counters::default();
    part.notes
        .iter()
        .filter_map(|note| {
            let id = note.id()?;
            paragraph::CURRENT_NOTE.with(|current| current.set(Some(id)));
            let chunks = lay_out_story(ctx, &mut counters, &note.blocks, &StoryId::Footnote(id), width);
            paragraph::CURRENT_NOTE.with(|current| current.set(None));
            Some((id, chunks.iter().map(|c| c.height).sum()))
        })
        .collect()
}

/// Where a float lands on the page, from its anchor.
fn place_float(float: &Float, page: &Page, column_x: f32, line_y: f32, column_width: f32) -> Item {
    let resolve = |(relative, anchor): &(String, Anchor), page_size: f32, margin_start: f32, margin_size: f32, line: f32, size: f32| -> f32 {
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
pub fn lay_out_story(ctx: &Ctx, counters: &mut Counters, blocks: &[Block], story: &StoryId, width: f32) -> Vec<Chunk> {
    let mut out = Vec::new();
    let mut pending_after = 0.0;
    lay_out_blocks(ctx, counters, blocks, story, &mut Vec::new(), width, None, &mut out, &mut pending_after);
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
                let laid = paragraph::lay_out(ctx, counters, paragraph, story, prefix, width, table_style);
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
                lay_out_blocks(ctx, counters, &sdt.blocks, story, prefix, width, table_style, out, pending_after);
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
fn decorate_pages(pages: &mut [Page], ctx: &mut Ctx, sections: &[(Vec<(usize, &Block)>, Geometry)]) {
    let total = pages.len() as u32;
    let even_odd = ctx
        .document
        .settings
        .as_ref()
        .and_then(|s| crate::props::on_off(&s.root, "w:evenAndOddHeaders"))
        .unwrap_or(false);
    let mut first_of_section: HashMap<usize, usize> = HashMap::new();
    let mut number = 0u32;
    for index in 0..pages.len() {
        let section = pages[index].section;
        let geometry = &sections.get(section).map_or_else(|| sections[0].1.clone(), |(_, g)| g.clone());
        let is_first = !first_of_section.contains_key(&section);
        first_of_section.entry(section).or_insert(index);
        number = match (is_first, geometry.page_number_start) {
            (true, Some(start)) => start,
            _ => number + 1,
        };
        pages[index].number = number;
        let kind = if is_first && geometry.title_page {
            "first"
        } else if even_odd && number % 2 == 0 {
            "even"
        } else {
            "default"
        };
        ctx.page = Some((number, total));
        let page = &mut pages[index];
        for (footer, references) in [(false, &geometry.headers), (true, &geometry.footers)] {
            let Some((_, id)) = references.iter().find(|(k, _)| k == kind) else { continue };
            let Some(header) = ctx.document.headers.iter().find(|h| &h.id == id) else { continue };
            let saved = std::mem::replace(&mut ctx.part, header.name.clone());
            let mut counters = Counters::default();
            let story = StoryId::Header(id.clone());
            let chunks = lay_out_story(ctx, &mut counters, &header.story.blocks, &story, geometry.text_width());
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

/// The footnotes referenced on a page, at its foot.
fn lay_out_footnotes(page: &mut Page, ctx: &Ctx, geometry: &Geometry) {
    let Some(part) = &ctx.document.footnotes else { return };
    let mut ids: Vec<i64> = Vec::new();
    for line in page.carets.iter().filter(|line| line.story == StoryId::Body) {
        let Some(paragraph) = crate::position::paragraph(ctx.document, &line.story, &line.path) else { continue };
        let mut at = 0;
        for run in &paragraph.runs {
            let len = run.len();
            if at >= line.start && at < line.end.max(line.start + 1) {
                if let crate::model::Content::Object(el) = &run.content {
                    if el.name == "w:footnoteReference" {
                        if let Some(id) = el.attr("w:id").and_then(|id| id.parse().ok()) {
                            if !ids.contains(&id) {
                                ids.push(id);
                            }
                        }
                    }
                }
            }
            at += len;
        }
    }
    if ids.is_empty() {
        return;
    }
    let mut counters = Counters::default();
    let mut chunks = Vec::new();
    let mut separator = Chunk { height: NOTE_SEPARATOR, ..Chunk::default() };
    separator.items.push(Item::Rule { x0: 0.0, y0: 5.0, x1: 144.0, y1: 5.0, width: 0.5, color: Rgb::BLACK, kind: RuleKind::Solid });
    chunks.push(separator);
    for id in ids {
        let Some(note) = part.notes.iter().find(|n| n.id() == Some(id)) else { continue };
        paragraph::CURRENT_NOTE.with(|current| current.set(Some(id)));
        chunks.extend(lay_out_story(ctx, &mut counters, &note.blocks, &StoryId::Footnote(id), geometry.text_width()));
        paragraph::CURRENT_NOTE.with(|current| current.set(None));
    }
    let mut block = stack(chunks);
    let y = geometry.height - geometry.bottom - block.height;
    block.shift(geometry.left + geometry.gutter, y);
    page.items.extend(block.items);
    page.carets.extend(block.carets);
}
