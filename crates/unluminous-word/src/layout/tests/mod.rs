//! Unit tests for the page layout rules (task-2359, TDD section 8). Each file is one rule and each test
//! is named for the Word fixture it mirrors. They use `FixedMetrics` (a cluster 6 points wide, a line
//! 12 points tall with 9 above the baseline), so every expected number can be worked out by hand.

mod column_balance;
mod footnote_continuation;
mod header_push;
mod hyphenation;
mod keep_next;
mod page_field;
mod ptab;
mod row_splitting;
mod spacing;
mod table_widths;
mod textbox;
mod wrapping;

use unluminous_core::FixedMetrics;

use crate::layout::{CaretLine, Item, Options, Page, Pages, TextItem, lay_out};
use crate::model::Document;

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The metrics every test uses: a cluster is 6 points wide and a line is 12 points tall.
pub fn metrics() -> FixedMetrics {
    FixedMetrics { cluster_width: 6.0, ascent: 9.0, descent: 3.0 }
}

/// A section for a small page: 200 points wide and 120 points tall, 20 points of margin at the left and
/// right and none at the top and bottom, so the body is 160 points wide and ten lines tall.
pub fn small_page() -> String {
    page(200.0, 120.0, "")
}

/// A `w:sectPr` for a page of the given size in points with no margin above or below, and `extra` inside it.
pub fn page(width: f32, height: f32, extra: &str) -> String {
    format!(
        "<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"/><w:pgMar w:top=\"0\" w:right=\"400\" w:bottom=\"0\" w:left=\"400\" w:header=\"0\" w:footer=\"0\" w:gutter=\"0\"/>{extra}</w:sectPr>",
        (width * 20.0) as i32,
        (height * 20.0) as i32
    )
}

/// A document whose body is `body` (paragraphs and tables as XML) ending with the section `sect`.
pub fn document(body: &str, sect: &str) -> Document {
    let xml = format!(
        "<w:body xmlns:w=\"{W_NS}\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">{body}</w:body>"
    );
    let parsed = crate::xml::parse(&xml).expect("the test body is well formed");
    let mut document = crate::new::blank();
    document.body = crate::read::blocks(&parsed);
    let sect = crate::xml::parse(&sect.replacen("<w:sectPr", &format!("<w:sectPr xmlns:w=\"{W_NS}\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\""), 1))
        .expect("the test section is well formed");
    document.section = sect;
    single_spaced(&mut document);
    document
}

/// Make the document's default paragraph single spaced with nothing after it, so a paragraph that sets
/// no spacing of its own (a header's, a note's, a text box's) is as tall as its lines and no more.
fn single_spaced(document: &mut Document) {
    let spacing = document
        .styles
        .as_mut()
        .and_then(|part| part.root.child_mut("w:docDefaults"))
        .and_then(|defaults| defaults.child_mut("w:pPrDefault"))
        .and_then(|default| default.child_mut("w:pPr"))
        .and_then(|ppr| ppr.child_mut("w:spacing"));
    if let Some(spacing) = spacing {
        spacing.set_attr("w:after", "0");
        spacing.set_attr("w:line", "240");
    }
}

/// The properties every test paragraph has: no space before or after and single spacing, so a line is
/// exactly the 12 points `FixedMetrics` gives it.
pub const PLAIN: &str =
    "<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/>";

/// A paragraph of single spaced text with `extra` properties added to the plain ones.
pub fn para_with(extra: &str, text: &str) -> String {
    format!(
        "<w:p><w:pPr>{extra}{PLAIN}</w:pPr><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>"
    )
}

/// A plain paragraph.
pub fn para(text: &str) -> String {
    para_with("", text)
}

/// `n` plain one line paragraphs named `<prefix><number>`, as XML.
pub fn paras(prefix: &str, n: usize) -> String {
    (0..n).map(|i| para(&format!("{prefix}{i}"))).collect()
}

/// Lay a document out with the fixed metrics.
pub fn laid(document: &Document) -> Pages {
    lay_out(document, &metrics(), Options::default())
}

/// Lay out body XML on the small page.
pub fn laid_small(body: &str) -> Pages {
    laid(&document(body, &small_page()))
}

/// Every caret line of the body paragraph (or table cell paragraph) at `path`, with its page index.
pub fn lines_of<'a>(pages: &'a Pages, path: &[u32]) -> Vec<(usize, &'a CaretLine)> {
    let mut out = Vec::new();
    for (index, page) in pages.pages.iter().enumerate() {
        for line in &page.carets {
            if line.story == crate::position::StoryId::Body && line.path == path {
                out.push((index, line));
            }
        }
    }
    out
}

/// The page index of the first line of the body block `index`, panicking when it has none.
pub fn page_of(pages: &Pages, index: u32) -> usize {
    lines_of(pages, &[index]).first().map(|(page, _)| *page).expect("the paragraph has a line")
}

/// The top of the first line of the body paragraph `index`.
pub fn top_of(pages: &Pages, index: u32) -> f32 {
    lines_of(pages, &[index]).first().map(|(_, line)| line.top).expect("the paragraph has a line")
}

/// Every text item on a page.
pub fn texts(page: &Page) -> Vec<&TextItem> {
    page.items
        .iter()
        .filter_map(|item| match item {
            Item::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

/// The text item on `page` whose letters start with `prefix`.
pub fn text_starting<'a>(page: &'a Page, prefix: &str) -> &'a TextItem {
    texts(page)
        .into_iter()
        .find(|t| t.text.starts_with(prefix))
        .unwrap_or_else(|| panic!("no text starting {prefix:?}"))
}

/// Text that takes `n` lines of the small page: `n` words of 20 letters, since two of them (41 clusters,
/// 246 points) are wider than the 160 point column and each word plus its space (21 clusters, 126
/// points) fits alone.
pub fn words(n: usize) -> String {
    vec!["abcdefghijklmnopqrst"; n].join(" ")
}

/// The pages the lines of body block `index` are on, in order.
pub fn pages_of_lines(pages: &Pages, index: u32) -> Vec<usize> {
    lines_of(pages, &[index]).into_iter().map(|(page, _)| page).collect()
}

/// A table with the given grid column widths (in twips) and rows, as XML.
pub fn table(props: &str, grid: &[u32], rows: &str) -> String {
    let columns: String = grid.iter().map(|w| format!("<w:gridCol w:w=\"{w}\"/>")).collect();
    format!("<w:tbl><w:tblPr>{props}</w:tblPr><w:tblGrid>{columns}</w:tblGrid>{rows}</w:tbl>")
}

/// A row with the given row properties and cells.
pub fn row(props: &str, cells: &str) -> String {
    format!("<w:tr><w:trPr>{props}</w:trPr>{cells}</w:tr>")
}

/// A cell `width` twips wide holding `content` (paragraphs as XML).
pub fn cell(width: u32, props: &str, content: &str) -> String {
    format!("<w:tc><w:tcPr><w:tcW w:w=\"{width}\" w:type=\"dxa\"/>{props}</w:tcPr>{content}</w:tc>")
}

/// The lines of the paragraphs in the cell `(row, column)` of the table at body block `index`, with the
/// page of each.
pub fn cell_lines<'a>(
    pages: &'a Pages,
    index: u32,
    row: u32,
    column: u32,
) -> Vec<(usize, &'a CaretLine)> {
    let mut out = Vec::new();
    for (page_index, page) in pages.pages.iter().enumerate() {
        for line in &page.carets {
            if line.story == crate::position::StoryId::Body
                && line.path.len() == 4
                && line.path[..3] == [index, row, column]
            {
                out.push((page_index, line));
            }
        }
    }
    out
}

/// A run holding a floating drawing with no picture part, `w` by `h` points, anchored to its column at
/// `x` points and to its paragraph at `y` points, with `wrap` as the wrapping element and `dist_right`
/// points kept clear on its right. Its box is all the layout needs.
pub fn floating(wrap: &str, x: f32, y: f32, w: f32, h: f32, dist_right: f32) -> String {
    let emu = |points: f32| (points * 12700.0) as i64;
    format!(
        "<w:r><w:drawing><wp:anchor distT=\"0\" distB=\"0\" distL=\"0\" distR=\"{}\" simplePos=\"0\" relativeHeight=\"1\" behindDoc=\"0\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\"><wp:simplePos x=\"0\" y=\"0\"/><wp:positionH relativeFrom=\"column\"><wp:posOffset>{}</wp:posOffset></wp:positionH><wp:positionV relativeFrom=\"paragraph\"><wp:posOffset>{}</wp:posOffset></wp:positionV><wp:extent cx=\"{}\" cy=\"{}\"/>{wrap}<wp:docPr id=\"1\" name=\"Picture 1\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"/></a:graphic></wp:anchor></w:drawing></w:r>",
        emu(dist_right),
        emu(x),
        emu(y),
        emu(w),
        emu(h)
    )
}

/// A plain paragraph holding `drawing` (runs as XML) and then `text`.
pub fn para_with_drawing(drawing: &str, text: &str) -> String {
    format!(
        "<w:p><w:pPr>{PLAIN}</w:pPr>{drawing}<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>"
    )
}

/// `n` words of ten letters, 60 points wide each; two with the space between them are 126 points.
pub fn short_words(n: usize) -> String {
    vec!["abcdefghij"; n].join(" ")
}

/// The top and the first stop's x of every line of body block `index`.
pub fn line_starts(pages: &Pages, index: u32) -> Vec<(f32, f32)> {
    lines_of(pages, &[index])
        .into_iter()
        .map(|(_, l)| (l.top, l.stops.first().map_or(f32::NAN, |s| s.1)))
        .collect()
}

/// A `w:sectPr` for a 200 by 192 point page with 20 points at each side, `top` and `bottom` margins and
/// the header and footer `header` and `footer` points from the page's edges (all in points).
pub fn banded_page(top: f32, bottom: f32, header: f32, footer: f32, extra: &str) -> String {
    let t = |points: f32| (points * 20.0) as i32;
    format!(
        "<w:sectPr><w:pgSz w:w=\"4000\" w:h=\"3840\"/><w:pgMar w:top=\"{}\" w:right=\"400\" w:bottom=\"{}\" w:left=\"400\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"0\"/>{extra}</w:sectPr>",
        t(top),
        t(bottom),
        t(header),
        t(footer)
    )
}

/// How many body paragraphs (blocks `0..count`) start on page `page`.
pub fn paragraphs_on(pages: &Pages, count: u32, page: usize) -> usize {
    (0..count).filter(|i| lines_of(pages, &[*i]).first().is_some_and(|(p, _)| *p == page)).count()
}
