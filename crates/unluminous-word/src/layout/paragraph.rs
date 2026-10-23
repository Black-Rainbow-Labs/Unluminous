//! One paragraph into lines.
//!
//! The paragraph's runs become atoms: a grapheme, a space, a tab, a break, a picture, a list label. Atoms
//! are measured through `FontMetrics` in each run's resolved style. Lines are filled greedily from
//! segments, a segment being the atoms between two places a line may break, which is how Word breaks:
//! after a space, after a hyphen, and around an ideograph. A segment wider than a whole line is broken
//! between graphemes. Then each line is placed: tabs are resolved against the tab stops, the line is
//! aligned or justified, and its height comes from the tallest font on it and the paragraph's line rule.

use unicode_segmentation::UnicodeSegmentation;
use unluminous_core::{CharStyle, Color, LineMetrics};

use super::{fields, BreakAfter, CaretLine, Chunk, Ctx, Float, Anchor, Item, LaidParagraph, Rect, RuleKind, TextItem};
use crate::model::{Content, Paragraph, Run};
use crate::numbering::{Counters, Suffix};
use crate::position::StoryId;
use crate::props::{self, Align, LineRule, ParaLook, Rgb, RunLook, TabKind, Underline, VertAlign};
use crate::xml::El;

/// The colour Word draws a tracked change in for the first author.
pub const REVISION: Rgb = Rgb(0xC0, 0x00, 0x00);

/// A run's look and what it is drawn and measured with.
#[derive(Debug, Clone)]
struct Look {
    run: RunLook,
    style: CharStyle,
    metrics: LineMetrics,
    /// The style lower case letters are drawn in under small caps.
    small: Option<(CharStyle, LineMetrics)>,
    /// Drawn struck through as a tracked deletion, or underlined as an insertion.
    deleted: bool,
    inserted: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Glyph(String),
    Space(String),
    Tab,
    Line,
    Page,
    Column,
    Picture { w: f32, h: f32, part: String },
    Float(Float),
    Label(String),
    /// A footnote or endnote reference mark.
    NoteMark(String),
}

#[derive(Debug, Clone)]
struct Atom {
    start: usize,
    end: usize,
    kind: Kind,
    width: f32,
    look: usize,
    /// Drawn in the small caps style.
    small: bool,
    /// A line may break after this atom.
    opportunity: bool,
}

impl Atom {
    fn addressable(&self) -> bool {
        !matches!(self.kind, Kind::Label(_))
    }

    fn is_special(&self) -> bool {
        matches!(self.kind, Kind::Tab | Kind::Line | Kind::Page | Kind::Column)
    }
}

fn char_style(run: &RunLook, size: f32, color: Option<Rgb>) -> CharStyle {
    let rgb = color.or(run.color).unwrap_or(Rgb::BLACK);
    CharStyle {
        family: run.font.clone(),
        size: size.max(1.0),
        bold: run.bold,
        italic: run.italic,
        underline: false,
        strikethrough: false,
        color: Color::rgb(rgb.0, rgb.1, rgb.2),
    }
}

fn make_look(ctx: &Ctx, run: RunLook, deleted: bool, inserted: bool) -> Look {
    let tracked = (deleted || inserted).then_some(REVISION);
    let style = char_style(&run, run.drawn_size(), tracked);
    let metrics = ctx.metrics.line_metrics(&style);
    let small = run.small_caps.then(|| {
        let small = char_style(&run, run.drawn_size() * 0.8, tracked);
        let metrics = ctx.metrics.line_metrics(&small);
        (small, metrics)
    });
    Look { run, style, metrics, small, deleted, inserted }
}

/// Lay a paragraph out at `width`, the width of the column or cell it is in.
pub fn lay_out(
    ctx: &Ctx,
    counters: &mut Counters,
    paragraph: &Paragraph,
    story: &StoryId,
    path: &[u32],
    width: f32,
    table_style: Option<&str>,
) -> LaidParagraph {
    let look = ctx.resolver.paragraph(&paragraph.props, table_style);
    let empty = El::new("w:rPr");
    let mark_rpr = paragraph.props.child("w:rPr").unwrap_or(&empty);
    let mark = make_look(ctx, ctx.resolver.run(&paragraph.props, mark_rpr, table_style), false, false);
    let mut looks: Vec<Look> = vec![mark];
    let mut atoms: Vec<Atom> = Vec::new();
    // The list label, first.
    if let Some((num, level)) = look.numbering {
        if let Some(label) = counters.next(&ctx.resolver.numbering, num, level) {
            let mut label_rpr = mark_rpr.clone();
            if let Some(level_rpr) = &label.rpr {
                for child in level_rpr.elements() {
                    label_rpr.put(child.clone());
                }
            }
            let mut run = ctx.resolver.run(&paragraph.props, &label_rpr, table_style);
            run.underline = Underline::None;
            looks.push(make_look(ctx, run, false, false));
            let index = looks.len() - 1;
            let width = measure(ctx, &looks[index], &label.text, false);
            atoms.push(Atom { start: 0, end: 0, kind: Kind::Label(label.text.clone()), width, look: index, small: false, opportunity: false });
            match label.suffix {
                Suffix::Tab => atoms.push(Atom { start: 0, end: 0, kind: Kind::Tab, width: 0.0, look: index, small: false, opportunity: false }),
                Suffix::Space => {
                    let space = measure(ctx, &looks[index], " ", false);
                    atoms.push(Atom { start: 0, end: 0, kind: Kind::Label(" ".into()), width: space, look: index, small: false, opportunity: false });
                }
                Suffix::Nothing => {}
            }
        }
    }
    collect_atoms(ctx, paragraph, table_style, &mut looks, &mut atoms);
    let lines = break_lines(ctx, &look, &atoms, width);
    let mut chunks: Vec<Chunk> = Vec::with_capacity(lines.len());
    let count = lines.len();
    for (index, line) in lines.into_iter().enumerate() {
        let first = index == 0;
        let last = index + 1 == count;
        chunks.push(place_line(ctx, &look, &looks, &atoms, &line, width, first, last, story, path, paragraph.len()));
    }
    decorate_paragraph(&look, &mut chunks, width);
    let auto = 14.0;
    LaidParagraph {
        lines: chunks,
        space_before: if look.before_auto { auto } else { look.space_before },
        space_after: if look.after_auto { auto } else { look.space_after },
        keep_next: look.keep_next,
        keep_lines: look.keep_lines,
        page_break_before: look.page_break_before,
        widow_control: look.widow_control,
        contextual: look.contextual,
        style: look.style.clone(),
    }
}

/// The width of `text` in a look, grapheme by grapheme.
fn measure(ctx: &Ctx, look: &Look, text: &str, small: bool) -> f32 {
    let style = match (small, &look.small) {
        (true, Some((style, _))) => style,
        _ => &look.style,
    };
    text.graphemes(true).map(|g| ctx.metrics.advance(g, style) * look.run.scale + look.run.spacing).sum()
}

/// What a field is doing at each point in the paragraph.
struct FieldState {
    /// Each open field: its code so far, whether its code is still being read, and a result to show
    /// in place of the stored one.
    open: Vec<(String, bool, Option<String>, bool)>,
}

fn collect_atoms(ctx: &Ctx, paragraph: &Paragraph, table_style: Option<&str>, looks: &mut Vec<Look>, atoms: &mut Vec<Atom>) {
    let mut offset = 0usize;
    let mut fields = FieldState { open: Vec::new() };
    let mut last_props: Option<(*const El, bool, bool, usize)> = None;
    // A `w:fldSimple`'s result, once its first run has shown it.
    let mut simple_done: Option<u64> = None;
    for run in &paragraph.runs {
        let len = run.len();
        let start = offset;
        offset += len;
        if let Content::FieldChar(el) = &run.content {
            match el.attr("w:fldCharType") {
                Some("begin") => fields.open.push((String::new(), true, None, false)),
                Some("separate") => {
                    if let Some(field) = fields.open.last_mut() {
                        field.1 = false;
                        field.2 = fields::page_result(&field.0, ctx.page);
                    }
                }
                _ => {
                    fields.open.pop();
                }
            }
            continue;
        }
        if let Content::Instr(code) = &run.content {
            if let Some(field) = fields.open.last_mut() {
                field.0.push_str(code);
            }
            continue;
        }
        if fields.open.iter().any(|field| field.1) {
            continue;
        }
        let look = {
            let deleted = run.is_deleted();
            let inserted = run.is_inserted();
            let key = (std::sync::Arc::as_ptr(&run.props), deleted, inserted);
            match last_props {
                Some((ptr, d, i, index)) if ptr == key.0 && d == deleted && i == inserted => index,
                _ => {
                    let mut resolved = ctx.resolver.run(&paragraph.props, &run.props, table_style);
                    if run.link().is_some() && run.props.val_of("w:rStyle").is_none() && resolved.color.is_none() {
                        resolved.color = ctx.resolver.theme.colour("hyperlink");
                    }
                    looks.push(make_look(ctx, resolved, deleted, inserted));
                    let index = looks.len() - 1;
                    last_props = Some((key.0, deleted, inserted, index));
                    index
                }
            }
        };
        if looks[look].run.hidden && !ctx.show_marks {
            continue;
        }
        // A result worked out for this page replaces the stored result: the first run of it shows the
        // whole value and the rest show nothing.
        let mut replaced: Option<String> = None;
        if let Some(field) = fields.open.last_mut() {
            if let Some(value) = &field.2 {
                if field.3 {
                    continue;
                }
                field.3 = true;
                replaced = Some(value.clone());
            }
        }
        if let Some(simple) = run.wraps.iter().find(|w| w.el.name == "w:fldSimple") {
            if let Some(value) = simple.el.attr("w:instr").and_then(|code| fields::page_result(code, ctx.page)) {
                if simple_done == Some(simple.id) {
                    continue;
                }
                simple_done = Some(simple.id);
                replaced = Some(value);
            }
        }
        if let Some(text) = replaced {
            push_text(ctx, &looks[look], look, &text, start, start + len, atoms, true);
            continue;
        }
        match &run.content {
            Content::Text(text) => push_text(ctx, &looks[look], look, text, start, start + len, atoms, false),
            Content::Tab => atoms.push(Atom { start, end: start + len, kind: Kind::Tab, width: 0.0, look, small: false, opportunity: true }),
            Content::Break(el) => {
                let kind = match el.attr("w:type") {
                    Some("page") => Kind::Page,
                    Some("column") => Kind::Column,
                    _ => Kind::Line,
                };
                atoms.push(Atom { start, end: start + len, kind, width: 0.0, look, small: false, opportunity: true });
            }
            Content::Object(el) => object_atom(ctx, run, el, &looks[look], look, start, start + len, atoms),
            Content::FieldChar(_) | Content::Instr(_) => {}
        }
    }
}

/// The atoms of a stretch of text, one grapheme each.
#[allow(clippy::too_many_arguments)]
fn push_text(ctx: &Ctx, look: &Look, index: usize, text: &str, start: usize, end: usize, atoms: &mut Vec<Atom>, replaced: bool) {
    let caps = look.run.caps || look.run.small_caps;
    let graphemes: Vec<(usize, &str)> = text.grapheme_indices(true).collect();
    let count = graphemes.len();
    for (position, (at, grapheme)) in graphemes.iter().enumerate() {
        // A replaced result covers the run's whole range with text that has its own length, so its
        // offsets are pinned to the run's two ends.
        let (atom_start, atom_end) = match replaced {
            true => (if position == 0 { start } else { end }, end),
            false => (start + at, start + at + grapheme.len()),
        };
        let small = look.run.small_caps && grapheme.chars().any(char::is_lowercase);
        let drawn = if caps { grapheme.to_uppercase() } else { (*grapheme).to_owned() };
        let width = measure(ctx, look, &drawn, small);
        let space = *grapheme == " " || *grapheme == "\u{3000}";
        let next = graphemes.get(position + 1).map(|(_, g)| *g);
        let opportunity = space
            || (matches!(*grapheme, "-" | "\u{2013}" | "\u{2014}" | "/") && next.is_some_and(|n| n.chars().all(char::is_alphanumeric)))
            || is_ideograph(grapheme)
            || next.is_some_and(is_ideograph)
            || *grapheme == "\u{200B}";
        let kind = if space { Kind::Space(drawn) } else { Kind::Glyph(drawn) };
        atoms.push(Atom { start: atom_start, end: atom_end, kind, width, look: index, small, opportunity: opportunity && position + 1 <= count });
    }
}

fn is_ideograph(grapheme: &str) -> bool {
    grapheme.chars().next().is_some_and(|c| {
        matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
    })
}

/// The atom for a run that holds something other than text.
#[allow(clippy::too_many_arguments)]
fn object_atom(ctx: &Ctx, run: &Run, el: &El, look: &Look, index: usize, start: usize, end: usize, atoms: &mut Vec<Atom>) {
    match el.name.as_str() {
        "w:footnoteReference" | "w:endnoteReference" => {
            let id = el.attr("w:id").and_then(|id| id.parse::<i64>().ok());
            let numbers = if el.name == "w:footnoteReference" { &ctx.footnote_numbers } else { &ctx.endnote_numbers };
            let mark = id.and_then(|id| numbers.get(&id)).map_or(String::new(), |n| match el.name.as_str() {
                "w:endnoteReference" => crate::numbering::format_number(i64::from(*n), "lowerRoman"),
                _ => n.to_string(),
            });
            let width = measure(ctx, look, &mark, false);
            atoms.push(Atom { start, end, kind: Kind::NoteMark(mark), width, look: index, small: false, opportunity: false });
        }
        // The mark at the start of a note's own text: its number, drawn the way the reference is.
        "w:footnoteRef" | "w:endnoteRef" => {
            let mark = note_number_for_ref(ctx, el.name == "w:endnoteRef");
            let width = measure(ctx, look, &mark, false);
            atoms.push(Atom { start, end, kind: Kind::NoteMark(mark), width, look: index, small: false, opportunity: false });
        }
        "w:separator" | "w:continuationSeparator" => {
            atoms.push(Atom { start, end, kind: Kind::Glyph(String::new()), width: 144.0, look: index, small: false, opportunity: false });
        }
        "w:noBreakHyphen" => {
            let width = measure(ctx, look, "-", false);
            atoms.push(Atom { start, end, kind: Kind::Glyph("-".into()), width, look: index, small: false, opportunity: false });
        }
        "w:softHyphen" | "w:commentReference" | "w:annotationRef" => {
            atoms.push(Atom { start, end, kind: Kind::Glyph(String::new()), width: 0.0, look: index, small: false, opportunity: true });
        }
        "w:sym" => {
            let character = run.content.character().unwrap_or('□').to_string();
            let width = measure(ctx, look, &character, false);
            atoms.push(Atom { start, end, kind: Kind::Glyph(character), width, look: index, small: false, opportunity: false });
        }
        _ => {
            let Some(picture) = run.content.picture() else {
                atoms.push(Atom { start, end, kind: Kind::Glyph(String::new()), width: 0.0, look: index, small: false, opportunity: true });
                return;
            };
            match read_picture(ctx, picture) {
                Some(PictureKind::Inline { w, h, part }) => {
                    atoms.push(Atom { start, end, kind: Kind::Picture { w, h, part }, width: w, look: index, small: false, opportunity: true });
                }
                Some(PictureKind::Float(float)) => {
                    atoms.push(Atom { start, end, kind: Kind::Float(float), width: 0.0, look: index, small: false, opportunity: false });
                }
                None => atoms.push(Atom { start, end, kind: Kind::Glyph(String::new()), width: 0.0, look: index, small: false, opportunity: true }),
            }
        }
    }
}

/// The number of the note being laid out, which a note's own `w:footnoteRef` shows.
fn note_number_for_ref(ctx: &Ctx, endnote: bool) -> String {
    let id = CURRENT_NOTE.with(|current| current.get());
    let numbers = if endnote { &ctx.endnote_numbers } else { &ctx.footnote_numbers };
    match id.and_then(|id| numbers.get(&id)) {
        Some(n) if endnote => crate::numbering::format_number(i64::from(*n), "lowerRoman"),
        Some(n) => n.to_string(),
        None => String::new(),
    }
}

thread_local! {
    /// The note whose text is being laid out, set by the caller that lays notes out.
    pub static CURRENT_NOTE: std::cell::Cell<Option<i64>> = const { std::cell::Cell::new(None) };
}

enum PictureKind {
    Inline { w: f32, h: f32, part: String },
    Float(Float),
}

/// A picture's size and the part its pixels are in, from a `w:drawing` or a `w:pict`.
fn read_picture(ctx: &Ctx, el: &El) -> Option<PictureKind> {
    let mut embed: Option<String> = None;
    el.walk(&mut |node| {
        if embed.is_none() {
            if node.name == "a:blip" {
                embed = node.attr("r:embed").or(node.attr("r:link")).map(str::to_owned);
            } else if node.name == "v:imagedata" {
                embed = node.attr("r:id").map(str::to_owned);
            }
        }
    });
    let part = embed
        .and_then(|id| {
            let rel = ctx.document.package.rels.get(&ctx.part)?.by_id(&id)?.clone();
            (!rel.external).then(|| crate::package::Package::resolve(&ctx.part, &rel.target))
        })
        .unwrap_or_default();
    if el.name == "w:pict" {
        let mut size = None;
        el.walk(&mut |node| {
            if size.is_none() && matches!(node.name.as_str(), "v:shape" | "v:rect" | "v:image") {
                size = node.attr("style").map(vml_size);
            }
        });
        let (w, h) = size.unwrap_or((72.0, 72.0));
        return Some(PictureKind::Inline { w, h, part });
    }
    let frame = el.elements().next()?;
    let extent = frame.child("wp:extent")?;
    let w = props::emu(extent.attr("cx").and_then(props::number).unwrap_or(0.0));
    let h = props::emu(extent.attr("cy").and_then(props::number).unwrap_or(0.0));
    if frame.name == "wp:inline" {
        return Some(PictureKind::Inline { w, h, part });
    }
    let position = |name: &str| -> (String, Anchor) {
        let Some(axis) = frame.child(name) else { return ("column".into(), Anchor::Offset(0.0)) };
        let relative = axis.attr("relativeFrom").unwrap_or("column").to_owned();
        let anchor = match (axis.child("wp:posOffset"), axis.child("wp:align")) {
            (Some(offset), _) => Anchor::Offset(props::emu(props::number(&offset.text()).unwrap_or(0.0))),
            (None, Some(align)) => Anchor::Align(align.text()),
            _ => Anchor::Offset(0.0),
        };
        (relative, anchor)
    };
    let wrap = frame
        .elements()
        .find_map(|child| child.local().strip_prefix("wrap").map(|kind| kind.to_owned()))
        .unwrap_or_else(|| if frame.attr("behindDoc") == Some("1") { "behind".into() } else { "none".into() });
    Some(PictureKind::Float(Float {
        width: w,
        height: h,
        horizontal: position("wp:positionH"),
        vertical: position("wp:positionV"),
        part,
        wrap: wrap.to_lowercase(),
    }))
}

/// `width:100pt;height:50pt` from a VML shape's style.
fn vml_size(style: &str) -> (f32, f32) {
    let mut size = (72.0, 72.0);
    for declaration in style.split(';') {
        let Some((key, value)) = declaration.split_once(':') else { continue };
        let points = props::number(value.trim()).map(|twips| (twips / 20.0) as f32);
        match (key.trim(), points) {
            ("width", Some(w)) => size.0 = w,
            ("height", Some(h)) => size.1 = h,
            _ => {}
        }
    }
    size
}

/// A line before it is placed: which atoms it holds and why it ended.
#[derive(Debug, Clone, Default)]
struct LineBuild {
    atoms: Vec<usize>,
    /// It ended with a line break the person typed, so a justified line is not stretched.
    forced: bool,
}

/// The left edge of a line, from the paragraph's indents.
fn line_left(look: &ParaLook, first: bool) -> f32 {
    if first {
        look.indent_left + look.indent_first
    } else {
        look.indent_left
    }
}

/// The position the next tab moves to, and how it aligns: the next custom stop after `x`, the hanging
/// indent when the first line starts before it, or the next default stop.
fn next_tab(ctx: &Ctx, look: &ParaLook, x: f32, first: bool) -> (f32, TabKind, Option<char>) {
    for stop in &look.tabs {
        if stop.position > x + 0.01 && stop.kind != TabKind::Bar {
            return (stop.position, stop.kind, stop.leader);
        }
    }
    if first && look.indent_first < 0.0 && look.indent_left > x + 0.01 {
        return (look.indent_left, TabKind::Left, None);
    }
    let step = ctx.default_tab;
    let next = ((x + 0.01) / step).floor() * step + step;
    (next, TabKind::Left, None)
}

/// Fill lines greedily from segments.
fn break_lines(ctx: &Ctx, look: &ParaLook, atoms: &[Atom], width: f32) -> Vec<LineBuild> {
    let mut lines: Vec<LineBuild> = Vec::new();
    let mut line = LineBuild::default();
    let right = width - look.indent_right;
    let mut pen = line_left(look, true);
    let mut has_content = false;
    let mut index = 0;
    let finish = |lines: &mut Vec<LineBuild>, line: &mut LineBuild, forced: bool| {
        line.forced = forced;
        lines.push(std::mem::take(line));
    };
    while index < atoms.len() {
        let atom = &atoms[index];
        if atom.is_special() {
            match atom.kind {
                Kind::Tab => {
                    let (stop, _, _) = next_tab(ctx, look, pen, lines.is_empty());
                    if stop > right + 0.01 && has_content {
                        finish(&mut lines, &mut line, false);
                        pen = line_left(look, false);
                        has_content = false;
                    }
                    let (stop, _, _) = next_tab(ctx, look, pen, lines.is_empty());
                    line.atoms.push(index);
                    pen = stop.max(pen);
                }
                Kind::Line => {
                    line.atoms.push(index);
                    finish(&mut lines, &mut line, true);
                    pen = line_left(look, false);
                    has_content = false;
                }
                _ => {
                    line.atoms.push(index);
                    finish(&mut lines, &mut line, true);
                    pen = line_left(look, false);
                    has_content = false;
                }
            }
            index += 1;
            continue;
        }
        // A segment: up to and including the next atom a line may break after.
        let mut end = index;
        while end < atoms.len() && !atoms[end].is_special() {
            end += 1;
            if atoms[end - 1].opportunity {
                break;
            }
        }
        let segment = &atoms[index..end];
        let total: f32 = segment.iter().map(|a| a.width).sum();
        let trailing: f32 = segment.iter().rev().take_while(|a| matches!(a.kind, Kind::Space(_))).map(|a| a.width).sum();
        let fits = pen + total - trailing <= right + 0.01;
        if !fits && has_content {
            finish(&mut lines, &mut line, false);
            pen = line_left(look, false);
            has_content = false;
        }
        if pen + total - trailing > right + 0.01 && !has_content {
            // Wider than a whole line: break between graphemes, at least one on each line.
            for at in index..end {
                let atom = &atoms[at];
                if has_content && pen + atom.width > right + 0.01 && !matches!(atom.kind, Kind::Space(_)) {
                    finish(&mut lines, &mut line, false);
                    pen = line_left(look, false);
                    has_content = false;
                }
                line.atoms.push(at);
                pen += atom.width;
                has_content |= atom.addressable();
            }
        } else {
            for at in index..end {
                line.atoms.push(at);
                has_content |= atoms[at].addressable();
            }
            pen += total;
        }
        index = end;
    }
    if !line.atoms.is_empty() || lines.is_empty() || lines.last().is_some_and(|l| l.forced) {
        lines.push(line);
    }
    lines
}

/// Place one line: tabs, alignment, height, and what is drawn.
#[allow(clippy::too_many_arguments)]
fn place_line(
    ctx: &Ctx,
    look: &ParaLook,
    looks: &[Look],
    atoms: &[Atom],
    line: &LineBuild,
    width: f32,
    first: bool,
    last: bool,
    story: &StoryId,
    path: &[u32],
    paragraph_len: usize,
) -> Chunk {
    let left = line_left(look, first);
    let right = width - look.indent_right;
    // x of each atom on the line and its width, with tabs resolved.
    let mut xs: Vec<(f32, f32)> = Vec::with_capacity(line.atoms.len());
    let mut leaders: Vec<(f32, f32, char, usize)> = Vec::new();
    let mut pen = left;
    for (position, &index) in line.atoms.iter().enumerate() {
        let atom = &atoms[index];
        if atom.kind == Kind::Tab {
            let (stop, kind, leader) = next_tab(ctx, look, pen, first);
            let following: f32 = line.atoms[position + 1..]
                .iter()
                .take_while(|&&i| atoms[i].kind != Kind::Tab)
                .map(|&i| atoms[i].width)
                .sum();
            let decimal: f32 = line.atoms[position + 1..]
                .iter()
                .take_while(|&&i| atoms[i].kind != Kind::Tab && !matches!(&atoms[i].kind, Kind::Glyph(g) if g == "." || g == ","))
                .map(|&i| atoms[i].width)
                .sum();
            let target = match kind {
                TabKind::Right => stop - following,
                TabKind::Center => stop - following / 2.0,
                TabKind::Decimal => stop - decimal,
                _ => stop,
            };
            let w = (target - pen).max(0.0);
            if let Some(leader) = leader {
                leaders.push((pen, w, leader, atom.look));
            }
            xs.push((pen, w));
            pen += w;
        } else {
            xs.push((pen, atom.width));
            pen += atom.width;
        }
    }
    // Trailing spaces hang past the margin and are not aligned.
    let content_end = line
        .atoms
        .iter()
        .zip(&xs)
        .rev()
        .find(|(&i, _)| !matches!(atoms[i].kind, Kind::Space(_) | Kind::Line | Kind::Page | Kind::Column))
        .map_or(left, |(_, (x, w))| x + w);
    let last_tab = line.atoms.iter().rposition(|&i| atoms[i].kind == Kind::Tab);
    let extra = (right - content_end).max(0.0);
    match look.align {
        Align::Center => xs.iter_mut().for_each(|(x, _)| *x += extra / 2.0),
        Align::Right => xs.iter_mut().for_each(|(x, _)| *x += extra),
        Align::Justify if !last && !line.forced => {
            let trailing_from = line
                .atoms
                .iter()
                .rposition(|&i| !matches!(atoms[i].kind, Kind::Space(_) | Kind::Line | Kind::Page | Kind::Column))
                .unwrap_or(0);
            let from = last_tab.map_or(0, |t| t + 1);
            let spaces: Vec<usize> = (from..trailing_from)
                .filter(|&p| matches!(atoms[line.atoms[p]].kind, Kind::Space(_)))
                .collect();
            if !spaces.is_empty() {
                let per = extra / spaces.len() as f32;
                let mut shift = 0.0;
                for position in 0..xs.len() {
                    xs[position].0 += shift;
                    if spaces.contains(&position) {
                        xs[position].1 += per;
                        shift += per;
                    }
                }
            }
        }
        _ => {}
    }
    for leader in &mut leaders {
        let shift = match look.align {
            Align::Center => extra / 2.0,
            Align::Right => extra,
            _ => 0.0,
        };
        leader.0 += shift;
    }
    // Height: the tallest ascent above the baseline and the deepest descent below it.
    let mut above: f32 = 0.0;
    let mut below: f32 = 0.0;
    let mut measured = false;
    for &index in &line.atoms {
        let atom = &atoms[index];
        let look_of = &looks[atom.look];
        match &atom.kind {
            Kind::Picture { h, .. } => {
                above = above.max(*h);
                measured = true;
            }
            Kind::Float(_) | Kind::Line | Kind::Page | Kind::Column => {}
            _ => {
                let metrics = look_of.metrics;
                let raise = raise_of(&look_of.run);
                above = above.max(metrics.ascent + metrics.line_gap + raise.max(0.0));
                below = below.max(metrics.descent + (-raise).max(0.0));
                measured = true;
            }
        }
    }
    if !measured {
        let metrics = looks[0].metrics;
        above = metrics.ascent + metrics.line_gap;
        below = metrics.descent;
    }
    let natural = above + below;
    let (height, baseline) = match look.line {
        LineRule::Auto(multiple) => {
            let height = natural * multiple.max(0.06);
            (height, height - below * multiple.clamp(0.06, 1.0))
        }
        LineRule::AtLeast(at_least) => {
            let height = natural.max(at_least);
            (height, height - below)
        }
        LineRule::Exact(exact) => (exact, exact - below.min(exact * 0.25)),
    };
    let mut chunk = Chunk { height, ..Chunk::default() };
    emit(ctx, looks, atoms, line, &xs, baseline, height, &mut chunk);
    for (x, w, leader, look_index) in leaders {
        let look_of = &looks[look_index];
        let dot = measure(ctx, look_of, &leader.to_string(), false).max(1.0);
        let count = ((w - dot) / dot).floor().max(0.0) as usize;
        if count > 0 {
            let start = x + (w - count as f32 * dot) - dot * 0.5;
            let xs: Vec<f32> = (0..count).map(|n| start + n as f32 * dot).collect();
            chunk.items.push(Item::Text(TextItem { baseline, style: look_of.style.clone(), text: leader.to_string().repeat(count), xs }));
        }
    }
    // The caret's stops.
    let mut stops: Vec<(usize, f32)> = Vec::new();
    for (position, &index) in line.atoms.iter().enumerate() {
        let atom = &atoms[index];
        if !atom.addressable() || matches!(atom.kind, Kind::Float(_)) && atom.start == atom.end {
            continue;
        }
        let (x, w) = xs[position];
        if stops.last().is_none_or(|(at, _)| *at != atom.start) {
            stops.push((atom.start, x));
        }
        let after = if matches!(atom.kind, Kind::Line | Kind::Page | Kind::Column) { x } else { x + w };
        stops.push((atom.end, after));
    }
    stops.dedup_by(|b, a| a.0 == b.0);
    let start = stops.first().map_or(0, |(at, _)| *at);
    let mut end = stops.last().map_or(start, |(at, _)| *at);
    if last {
        let end_x = stops.last().map_or_else(
            || match look.align {
                Align::Center => (left + right) / 2.0,
                Align::Right => right,
                _ => left,
            },
            |(_, x)| *x,
        );
        if stops.last().is_none_or(|(at, _)| *at < paragraph_len) {
            stops.push((paragraph_len, end_x));
        }
        end = paragraph_len;
    }
    // A line that ends in a break stands its caret before the break, and the next line owns what follows.
    if !last && line.forced {
        if let Some(&index) = line.atoms.last() {
            end = atoms[index].end;
        }
    }
    let start = if stops.is_empty() { 0 } else { start };
    chunk.carets.push(CaretLine {
        story: story.clone(),
        path: path.to_vec(),
        start,
        end,
        top: 0.0,
        height,
        baseline,
        stops,
        last,
    });
    if let Some(&index) = line.atoms.last() {
        chunk.break_after = match atoms[index].kind {
            Kind::Page => Some(BreakAfter::Page),
            Kind::Column => Some(BreakAfter::Column),
            _ => None,
        };
    }
    for &index in &line.atoms {
        if let Kind::Float(float) = &atoms[index].kind {
            chunk.floats.push(float.clone());
        }
    }
    chunk
}

/// How far a run's letters are raised (positive) or lowered.
fn raise_of(run: &RunLook) -> f32 {
    run.position
        + match run.vert {
            VertAlign::Superscript => run.size * 0.33,
            VertAlign::Subscript => -run.size * 0.14,
            VertAlign::Baseline => 0.0,
        }
}

/// The items a placed line draws: text grouped by look, rules, fills and pictures.
#[allow(clippy::too_many_arguments)]
fn emit(ctx: &Ctx, looks: &[Look], atoms: &[Atom], line: &LineBuild, xs: &[(f32, f32)], baseline: f32, height: f32, chunk: &mut Chunk) {
    let mut fills: Vec<Item> = Vec::new();
    let mut rules: Vec<Item> = Vec::new();
    let mut texts: Vec<Item> = Vec::new();
    let mut pictures: Vec<Item> = Vec::new();
    // Text runs, one item per look and small caps state.
    let mut current: Option<(usize, bool, TextItem)> = None;
    let flush = |current: &mut Option<(usize, bool, TextItem)>, texts: &mut Vec<Item>| {
        if let Some((_, _, item)) = current.take() {
            if !item.text.is_empty() {
                texts.push(Item::Text(item));
            }
        }
    };
    let trailing_from = line
        .atoms
        .iter()
        .rposition(|&i| !matches!(atoms[i].kind, Kind::Space(_) | Kind::Line | Kind::Page | Kind::Column))
        .map_or(0, |p| p + 1);
    for (position, &index) in line.atoms.iter().enumerate() {
        let atom = &atoms[index];
        let (x, w) = xs[position];
        let look = &looks[atom.look];
        let raise = raise_of(&look.run);
        let text = match &atom.kind {
            Kind::Glyph(text) | Kind::Space(text) | Kind::Label(text) | Kind::NoteMark(text) => Some(text),
            Kind::Picture { w, h, part } => {
                let rect = Rect::new(x, baseline - h, *w, *h);
                if part.is_empty() {
                    pictures.push(Item::Placeholder { rect, label: "picture".into() });
                } else {
                    pictures.push(Item::Picture { rect, part: part.clone() });
                }
                None
            }
            _ => None,
        };
        if let Some(text) = text {
            let style = match (atom.small, &look.small) {
                (true, Some((style, _))) => style.clone(),
                _ => look.style.clone(),
            };
            let same = current.as_ref().is_some_and(|(l, s, _)| *l == atom.look && *s == atom.small);
            if !same {
                flush(&mut current, &mut texts);
                current = Some((atom.look, atom.small, TextItem { baseline: baseline - raise, style, text: String::new(), xs: Vec::new() }));
            }
            if let Some((_, _, item)) = current.as_mut() {
                for character in text.chars() {
                    item.text.push(character);
                    item.xs.push(x);
                }
                // Each character of a grapheme after the first sits where the grapheme does; a
                // grapheme of several letters that are not combining (a ligature written out) is spaced
                // across its width instead.
                let count = text.chars().count();
                if count > 1 && text.chars().skip(1).all(|c| !is_combining(c)) {
                    let step = w / count as f32;
                    let n = item.xs.len();
                    for (k, slot) in item.xs[n - count..].iter_mut().enumerate() {
                        *slot = x + step * k as f32;
                    }
                }
            }
        } else {
            flush(&mut current, &mut texts);
        }
        // Decoration.
        let decorated = !matches!(atom.kind, Kind::Label(_) | Kind::Line | Kind::Page | Kind::Column | Kind::Float(_));
        if !decorated || w <= 0.0 {
            continue;
        }
        let metrics = &look.metrics;
        let top = baseline - raise - metrics.ascent - metrics.line_gap * 0.0;
        let band = Rect::new(x, top.max(0.0), w, (metrics.ascent + metrics.descent).min(height));
        if let Some(color) = look.run.highlight {
            fills.push(Item::Fill { rect: band, color });
        } else if let Some(color) = look.run.shading {
            fills.push(Item::Fill { rect: band, color });
        }
        let in_trailing = position >= trailing_from;
        let space = matches!(atom.kind, Kind::Space(_));
        let ink = look.run.color.unwrap_or(Rgb::BLACK);
        let ink = if look.deleted || look.inserted { REVISION } else { ink };
        let underline = if look.inserted && look.run.underline == Underline::None { Underline::Single } else { look.run.underline };
        if underline != Underline::None && !in_trailing && !(space && underline == Underline::Words) {
            let thickness = ctx.metrics.rule_thickness(&look.style);
            let y = baseline - raise + ctx.metrics.underline_offset(&look.style);
            let (width, kind) = match underline {
                Underline::Thick => (thickness * 2.0, RuleKind::Solid),
                Underline::Double => (thickness, RuleKind::Double),
                Underline::Dotted => (thickness, RuleKind::Dotted),
                Underline::Dash | Underline::DotDash => (thickness, RuleKind::Dashed),
                Underline::Wave => (thickness, RuleKind::Wave),
                _ => (thickness, RuleKind::Solid),
            };
            rules.push(Item::Rule { x0: x, y0: y, x1: x + w, y1: y, width, color: ink, kind });
        }
        if (look.run.strike || look.run.double_strike || look.deleted) && !in_trailing {
            let thickness = ctx.metrics.rule_thickness(&look.style);
            let y = baseline - raise - ctx.metrics.strikethrough_offset(&look.style);
            let kind = if look.run.double_strike { RuleKind::Double } else { RuleKind::Solid };
            rules.push(Item::Rule { x0: x, y0: y, x1: x + w, y1: y, width: thickness, color: ink, kind });
        }
    }
    flush(&mut current, &mut texts);
    chunk.items.extend(fills);
    chunk.items.extend(pictures);
    chunk.items.extend(texts);
    chunk.items.extend(rules);
}

fn is_combining(character: char) -> bool {
    matches!(character as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F | 0x200D | 0xFE0F)
}

/// A paragraph's shading and borders, drawn on its lines so they split across pages with them.
fn decorate_paragraph(look: &ParaLook, chunks: &mut [Chunk], width: f32) {
    let borders = look.borders;
    if look.shading.is_none() && borders.top.is_none() && borders.bottom.is_none() && borders.left.is_none() && borders.right.is_none() {
        return;
    }
    let left = look.indent_left - borders.left.map_or(0.0, |b| b.space + b.width);
    let right = width - look.indent_right + borders.right.map_or(0.0, |b| b.space + b.width);
    let count = chunks.len();
    for (index, chunk) in chunks.iter_mut().enumerate() {
        let h = chunk.height;
        if let Some(color) = look.shading {
            chunk.items.insert(0, Item::Fill { rect: Rect::new(left, 0.0, right - left, h), color });
        }
        let rule = |x0: f32, y0: f32, x1: f32, y1: f32, b: props::Border| Item::Rule {
            x0,
            y0,
            x1,
            y1,
            width: b.width,
            color: b.color.unwrap_or(Rgb::BLACK),
            kind: if b.double { RuleKind::Double } else { RuleKind::Solid },
        };
        if index == 0 {
            if let Some(b) = borders.top {
                chunk.items.push(rule(left, 0.0, right, 0.0, b));
            }
        }
        if index + 1 == count {
            if let Some(b) = borders.bottom {
                chunk.items.push(rule(left, h, right, h, b));
            }
        }
        if let Some(b) = borders.left {
            chunk.items.push(rule(left, 0.0, left, h, b));
        }
        if let Some(b) = borders.right {
            chunk.items.push(rule(right, 0.0, right, h, b));
        }
    }
}
