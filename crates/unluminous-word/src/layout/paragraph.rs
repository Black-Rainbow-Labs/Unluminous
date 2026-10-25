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

use super::{
    fields, BreakAfter, CaretLine, Chunk, Ctx, Float, Item, LaidParagraph, Rect, RuleKind, TextItem,
};
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
    /// Part of a tracked change, whether or not it is drawn as one, for Simple Markup's margin bar.
    revised: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Glyph(String),
    Space(String),
    Tab,
    Line,
    Page,
    Column,
    /// A picture or a shape in the line, drawn as `drawing::items` draws it.
    Picture(Box<Float>),
    Float(Float),
    Label(String),
    /// A footnote or endnote reference mark, with the footnote's id when it is a reference to one in the
    /// body, whose note goes at the foot of the page the line lands on.
    NoteMark(String, Option<i64>),
    /// An absolute position tab (`w:ptab`): to the left, centre or right of the margins or the indents,
    /// with what lies after it aligned there (task-2359, D7).
    PositionTab {
        align: TabKind,
        to_indent: bool,
        leader: Option<char>,
    },
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
        matches!(
            self.kind,
            Kind::Tab | Kind::Line | Kind::Page | Kind::Column | Kind::PositionTab { .. }
        )
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
    let revised = deleted || inserted;
    // Only All Markup draws a change as a change; the other views draw the text as it is or was.
    let (deleted, inserted) = match ctx.markup {
        crate::review::Markup::All => (deleted, inserted),
        _ => (false, false),
    };
    let tracked = (deleted || inserted).then_some(REVISION);
    let style = char_style(&run, run.drawn_size(), tracked);
    let metrics = ctx.metrics.line_metrics(&style);
    let small = run.small_caps.then(|| {
        let small = char_style(&run, run.drawn_size() * 0.8, tracked);
        let metrics = ctx.metrics.line_metrics(&small);
        (small, metrics)
    });
    Look { run, style, metrics, small, deleted, inserted, revised }
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
    prepare(ctx, counters, paragraph, story, path, width, table_style).laid(ctx)
}

/// A paragraph measured and ready to be broken into lines: its look, the looks of its runs and its atoms.
/// The page flow breaks it one line at a time when the room a line has depends on where on the page it
/// lands, beside a floating picture (task-2359, C2).
#[derive(Debug, Clone)]
pub struct Prepared {
    pub look: ParaLook,
    looks: Vec<Look>,
    atoms: Vec<Atom>,
    story: StoryId,
    path: Vec<u32>,
    pub width: f32,
    length: usize,
}

/// Measure a paragraph into atoms, its list label first.
pub fn prepare(
    ctx: &Ctx,
    counters: &mut Counters,
    paragraph: &Paragraph,
    story: &StoryId,
    path: &[u32],
    width: f32,
    table_style: Option<&str>,
) -> Prepared {
    let mut look = ctx.resolver.paragraph(&paragraph.props, table_style);
    if let Some(align) = display_equation_alignment(paragraph) {
        look.align = align;
    }
    let empty = El::new("w:rPr");
    let mark_rpr = paragraph.props.child("w:rPr").unwrap_or(&empty);
    let mark =
        make_look(ctx, ctx.resolver.run(&paragraph.props, mark_rpr, table_style), false, false);
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
            atoms.push(Atom {
                start: 0,
                end: 0,
                kind: Kind::Label(label.text.clone()),
                width,
                look: index,
                small: false,
                opportunity: false,
            });
            match label.suffix {
                Suffix::Tab => atoms.push(Atom {
                    start: 0,
                    end: 0,
                    kind: Kind::Tab,
                    width: 0.0,
                    look: index,
                    small: false,
                    opportunity: false,
                }),
                Suffix::Space => {
                    let space = measure(ctx, &looks[index], " ", false);
                    atoms.push(Atom {
                        start: 0,
                        end: 0,
                        kind: Kind::Label(" ".into()),
                        width: space,
                        look: index,
                        small: false,
                        opportunity: false,
                    });
                }
                Suffix::Nothing => {}
            }
        }
    }
    let page = ctx.page.or_else(|| {
        // A page field in the body shows the page its paragraph landed on in the pass before.
        (*story == StoryId::Body).then(|| ctx.body_pages.get(path).copied()).flatten()
    });
    collect_atoms(ctx, paragraph, table_style, page, &mut looks, &mut atoms);
    Prepared {
        look,
        looks,
        atoms,
        story: story.clone(),
        path: path.to_vec(),
        width,
        length: paragraph.len(),
    }
}

impl Prepared {
    /// Every line, each at the paragraph's own indents, as one piece the flow places.
    pub fn laid(self, ctx: &Ctx) -> LaidParagraph {
        let hyphen_width = measure(ctx, &self.looks[0], "-", false);
        let lines = break_lines(ctx, &self.look, &self.atoms, self.width, hyphen_width);
        let mut chunks: Vec<Chunk> = Vec::with_capacity(lines.len());
        let count = lines.len();
        for (index, line) in lines.into_iter().enumerate() {
            let first = index == 0;
            let bounds = self.bounds(first, None);
            chunks.push(self.place(ctx, &line, first, index + 1 == count, bounds));
        }
        decorate_paragraph(&self.look, &mut chunks, self.width);
        self.spacing(chunks)
    }

    /// The paragraph's spacing and keep rules around some placed lines.
    pub fn spacing(&self, lines: Vec<Chunk>) -> LaidParagraph {
        let look = &self.look;
        let auto = 14.0;
        LaidParagraph {
            lines,
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

    /// The pictures anchored in this paragraph that float.
    pub fn floats(&self) -> Vec<Float> {
        self.atoms
            .iter()
            .filter_map(|atom| match &atom.kind {
                Kind::Float(float) => Some(float.clone()),
                _ => None,
            })
            .collect()
    }

    /// How tall a line of this paragraph is, near enough to ask where it fits beside a picture.
    pub fn line_height(&self) -> f32 {
        let metrics = self.looks.get(1).unwrap_or(&self.looks[0]).metrics;
        let natural = metrics.ascent + metrics.line_gap + metrics.descent;
        match self.look.line {
            LineRule::Auto(multiple) => natural * multiple.max(0.06),
            LineRule::AtLeast(at_least) => natural.max(at_least),
            LineRule::Exact(exact) => exact,
        }
    }

    /// The left and right edge a line has: the paragraph's indents, narrowed to `room` (the part of the
    /// column a floating picture leaves free) when there is one.
    pub fn bounds(&self, first: bool, room: Option<(f32, f32)>) -> (f32, f32) {
        let left = line_left(&self.look, first);
        let right = self.width - self.look.indent_right;
        match room {
            Some((from, to)) => (left.max(from), right.min(to)),
            None => (left, right),
        }
    }

    /// The next line, from atom `from`, within `bounds`: its atoms, the atom after it, and whether it is
    /// the paragraph's last.
    pub fn next_line(
        &self,
        ctx: &Ctx,
        from: usize,
        first: bool,
        bounds: (f32, f32),
        hyphens: u32,
    ) -> (LineBuild, usize, bool) {
        let hyphen_width = measure(ctx, &self.looks[0], "-", false);
        let (line, next) = next_line(
            ctx,
            &self.look,
            &self.atoms,
            from,
            first,
            bounds,
            self.width,
            (hyphens, hyphen_width),
        );
        // A paragraph that ends in a line break has one more, empty line after it.
        let last = next >= self.atoms.len() && !line.forced;
        (line, next, last)
    }

    /// Whether every atom has been put on a line.
    pub fn done(&self, next: usize) -> bool {
        next >= self.atoms.len()
    }

    /// Place a line within `bounds`.
    pub fn place(
        &self,
        ctx: &Ctx,
        line: &LineBuild,
        first: bool,
        last: bool,
        bounds: (f32, f32),
    ) -> Chunk {
        place_line(
            ctx,
            &self.look,
            &self.looks,
            &self.atoms,
            line,
            self.width,
            first,
            last,
            &self.story,
            &self.path,
            self.length,
            bounds,
        )
    }
}

/// The alignment of a paragraph that holds a display equation (`m:oMathPara`) and nothing else: its
/// `m:jc`, which is centred as a group when it says nothing, as Word draws it.
fn display_equation_alignment(paragraph: &Paragraph) -> Option<Align> {
    let mut display = None;
    for run in &paragraph.runs {
        match &run.content {
            Content::ParagraphObject(el) if el.name == "m:oMathPara" => display = Some(el),
            Content::Text(text) if text.trim().is_empty() => {}
            _ => return None,
        }
    }
    let jc =
        display?.child("m:oMathParaPr").and_then(|p| p.child("m:jc")).and_then(|j| j.attr("m:val"));
    Some(match jc {
        Some("left") => Align::Left,
        Some("right") => Align::Right,
        _ => Align::Center,
    })
}

/// The width of `text` in a look, grapheme by grapheme.
fn measure(ctx: &Ctx, look: &Look, text: &str, small: bool) -> f32 {
    let style = match (small, &look.small) {
        (true, Some((style, _))) => style,
        _ => &look.style,
    };
    text.graphemes(true)
        .map(|g| ctx.metrics.advance(g, style) * look.run.scale + look.run.spacing)
        .sum()
}

/// What a field is doing at each point in the paragraph.
struct FieldState {
    /// Each open field: its code so far, whether its code is still being read, and a result to show
    /// in place of the stored one.
    open: Vec<(String, bool, Option<String>, bool)>,
}

fn collect_atoms(
    ctx: &Ctx,
    paragraph: &Paragraph,
    table_style: Option<&str>,
    page: Option<(u32, u32)>,
    looks: &mut Vec<Look>,
    atoms: &mut Vec<Atom>,
) {
    let mut offset = 0usize;
    let mut fields = FieldState { open: Vec::new() };
    let mut last_props: Option<(*const El, bool, bool, usize)> = None;
    // A `w:fldSimple`'s result, once its first run has shown it.
    let mut simple_done: Option<u64> = None;
    for run in &paragraph.runs {
        let len = run.len();
        let start = offset;
        offset += len;
        if ctx.show_codes {
            let shown = show_code(
                ctx,
                paragraph,
                run,
                table_style,
                looks,
                atoms,
                start,
                &mut fields,
                &mut simple_done,
            );
            if shown.is_some() {
                continue;
            }
        }
        if let Content::FieldChar(el) = &run.content {
            match el.attr("w:fldCharType") {
                Some("begin") => fields.open.push((String::new(), true, None, false)),
                Some("separate") => {
                    if let Some(field) = fields.open.last_mut() {
                        field.1 = false;
                        field.2 = fields::page_result(&field.0, page);
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
        // No Markup and Simple Markup show the text as it will be, Original as it was.
        let hidden_by_markup = match ctx.markup {
            crate::review::Markup::All => false,
            crate::review::Markup::Original => run.is_inserted(),
            _ => run.is_deleted(),
        };
        if hidden_by_markup {
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
                    if run.link().is_some()
                        && run.props.val_of("w:rStyle").is_none()
                        && resolved.color.is_none()
                    {
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
            if let Some(value) =
                simple.el.attr("w:instr").and_then(|code| fields::page_result(code, page))
            {
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
            Content::Text(text) => {
                push_text(ctx, &looks[look], look, text, start, start + len, atoms, false)
            }
            Content::Tab => atoms.push(Atom {
                start,
                end: start + len,
                kind: Kind::Tab,
                width: 0.0,
                look,
                small: false,
                opportunity: true,
            }),
            Content::Break(el) => {
                let kind = match el.attr("w:type") {
                    Some("page") => Kind::Page,
                    Some("column") => Kind::Column,
                    _ => Kind::Line,
                };
                atoms.push(Atom {
                    start,
                    end: start + len,
                    kind,
                    width: 0.0,
                    look,
                    small: false,
                    opportunity: true,
                });
            }
            Content::Object(el) => {
                object_atom(ctx, run, el, &looks[look], look, start, start + len, atoms)
            }
            Content::ParagraphObject(el) if el.name.starts_with("m:") => {
                // An equation, drawn as its linear text in the equation font at the run's size.
                let mut run_look = looks[look].run.clone();
                run_look.font = crate::math::FONT.into();
                looks.push(make_look(ctx, run_look, run.is_deleted(), run.is_inserted()));
                let index = looks.len() - 1;
                let text = crate::math::linear_text(el).replace('\u{000B}', " ");
                push_text(ctx, &looks[index], index, &text, start, start + len, atoms, true);
            }
            Content::ParagraphObject(_) => atoms.push(Atom {
                start,
                end: start + len,
                kind: Kind::Glyph(String::new()),
                width: 0.0,
                look,
                small: false,
                opportunity: true,
            }),
            Content::FieldChar(_) | Content::Instr(_) => {}
        }
    }
}

/// The atoms of a stretch of text, one grapheme each.
#[allow(clippy::too_many_arguments)]
/// With field codes showing: a field's begin draws `{ `, its code is drawn as text, its result is left
/// out and its end draws ` }`; a simple field draws its whole code in braces once. Answers `Some` when the
/// run was dealt with here, `None` for a run that is drawn as usual.
#[allow(clippy::too_many_arguments)]
fn show_code(
    ctx: &Ctx,
    paragraph: &Paragraph,
    run: &crate::model::Run,
    table_style: Option<&str>,
    looks: &mut Vec<Look>,
    atoms: &mut Vec<Atom>,
    start: usize,
    fields: &mut FieldState,
    simple_done: &mut Option<u64>,
) -> Option<()> {
    let look = |looks: &mut Vec<Look>| {
        let resolved = ctx.resolver.run(&paragraph.props, &run.props, table_style);
        looks.push(make_look(ctx, resolved, false, false));
        looks.len() - 1
    };
    match &run.content {
        Content::FieldChar(el) => {
            let index = look(looks);
            match el.attr("w:fldCharType") {
                Some("begin") => {
                    fields.open.push((String::new(), true, None, false));
                    push_text(ctx, &looks[index], index, "{ ", start, start, atoms, true);
                }
                Some("separate") => {
                    if let Some(field) = fields.open.last_mut() {
                        field.1 = false;
                    }
                }
                _ => {
                    if fields.open.pop().is_some() {
                        push_text(ctx, &looks[index], index, " }", start, start, atoms, true);
                    }
                }
            }
            Some(())
        }
        Content::Instr(code) => {
            let index = look(looks);
            push_text(
                ctx,
                &looks[index],
                index,
                code.trim(),
                start,
                start + run.len(),
                atoms,
                true,
            );
            Some(())
        }
        // A result is not drawn while its code is.
        _ if fields.open.iter().any(|field| !field.1) => Some(()),
        _ => {
            let simple = run.wraps.iter().find(|w| w.el.name == "w:fldSimple")?;
            if *simple_done != Some(simple.id) {
                *simple_done = Some(simple.id);
                let index = look(looks);
                let code = format!("{{ {} }}", simple.el.attr("w:instr").unwrap_or("").trim());
                push_text(ctx, &looks[index], index, &code, start, start, atoms, true);
            }
            Some(())
        }
    }
}

fn push_text(
    ctx: &Ctx,
    look: &Look,
    index: usize,
    text: &str,
    start: usize,
    end: usize,
    atoms: &mut Vec<Atom>,
    replaced: bool,
) {
    let caps = look.run.caps || look.run.small_caps;
    let graphemes: Vec<(usize, &str)> = text.grapheme_indices(true).collect();
    for (position, (at, grapheme)) in graphemes.iter().enumerate() {
        // A replaced result covers the run's whole range with text that has its own length, so its
        // offsets are pinned to the run's two ends.
        let (atom_start, atom_end) = match replaced {
            true => (if position == 0 { start } else { end }, end),
            false => (start + at, start + at + grapheme.len()),
        };
        let small = look.run.small_caps && grapheme.chars().any(char::is_lowercase);
        let drawn = if caps { grapheme.to_uppercase() } else { (*grapheme).to_owned() };
        let mut width = measure(ctx, look, &drawn, small);
        // A pair is kerned when the run asks for it at its size, which Word's default style does from one
        // point up. The adjustment belongs to the left letter of the pair.
        if look.run.kern.is_some_and(|from| look.run.size >= from) {
            if let (Some(left), Some(next)) = (drawn.chars().last(), graphemes.get(position + 1)) {
                let right =
                    if caps { next.1.to_uppercase().chars().next() } else { next.1.chars().next() };
                if let Some(right) = right {
                    width += ctx.metrics.kerning(left, right, &look.style) * look.run.scale;
                }
            }
        }
        let space = *grapheme == " " || *grapheme == "\u{3000}";
        let next = graphemes.get(position + 1).map(|(_, g)| *g);
        let opportunity = space
            || (matches!(*grapheme, "-" | "\u{2013}" | "\u{2014}" | "/")
                && next.is_some_and(|n| n.chars().all(char::is_alphanumeric)))
            || is_ideograph(grapheme)
            || next.is_some_and(is_ideograph)
            || *grapheme == "\u{200B}";
        let kind = if space { Kind::Space(drawn) } else { Kind::Glyph(drawn) };
        atoms.push(Atom {
            start: atom_start,
            end: atom_end,
            kind,
            width,
            look: index,
            small,
            opportunity,
        });
    }
}

fn is_ideograph(grapheme: &str) -> bool {
    grapheme.chars().next().is_some_and(|c| {
        matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
    })
}

/// The atom for a run that holds something other than text.
#[allow(clippy::too_many_arguments)]
fn object_atom(
    ctx: &Ctx,
    run: &Run,
    el: &El,
    look: &Look,
    index: usize,
    start: usize,
    end: usize,
    atoms: &mut Vec<Atom>,
) {
    match el.name.as_str() {
        "w:footnoteReference" | "w:endnoteReference" => {
            let id = el.attr("w:id").and_then(|id| id.parse::<i64>().ok());
            let marks = if el.name == "w:footnoteReference" {
                &ctx.footnote_marks
            } else {
                &ctx.endnote_marks
            };
            // A custom mark is the text typed after the reference, so the reference shows nothing.
            let mark = id.and_then(|id| marks.get(&id)).cloned().unwrap_or_default();
            let width = measure(ctx, look, &mark, false);
            let footnote = id.filter(|_| el.name == "w:footnoteReference");
            atoms.push(Atom {
                start,
                end,
                kind: Kind::NoteMark(mark, footnote),
                width,
                look: index,
                small: false,
                opportunity: false,
            });
        }
        // The mark at the start of a note's own text: its number, drawn the way the reference is.
        "w:footnoteRef" | "w:endnoteRef" => {
            let mark = note_number_for_ref(ctx, el.name == "w:endnoteRef");
            let width = measure(ctx, look, &mark, false);
            atoms.push(Atom {
                start,
                end,
                kind: Kind::NoteMark(mark, None),
                width,
                look: index,
                small: false,
                opportunity: false,
            });
        }
        "w:separator" | "w:continuationSeparator" => {
            atoms.push(Atom {
                start,
                end,
                kind: Kind::Glyph(String::new()),
                width: 144.0,
                look: index,
                small: false,
                opportunity: false,
            });
        }
        "w:ptab" => {
            let align = match el.attr("w:alignment") {
                Some("center") => TabKind::Center,
                Some("right") => TabKind::Right,
                _ => TabKind::Left,
            };
            let leader = match el.attr("w:leader") {
                Some("dot") => Some('.'),
                Some("hyphen") => Some('-'),
                Some("underscore") => Some('_'),
                Some("middleDot") => Some('·'),
                _ => None,
            };
            atoms.push(Atom {
                start,
                end,
                kind: Kind::PositionTab {
                    align,
                    to_indent: el.attr("w:relativeTo") == Some("indent"),
                    leader,
                },
                width: 0.0,
                look: index,
                small: false,
                opportunity: false,
            });
        }
        "w:ruby" => {
            // Phonetic guide text: the base is the text a reader sees; the guide above it is left out.
            let mut base = String::new();
            if let Some(el) = el.child("w:rubyBase") {
                el.walk(&mut |node| {
                    if node.name == "w:t" {
                        base.push_str(&node.text());
                    }
                });
            }
            push_text(ctx, look, index, &base, start, end, atoms, true);
        }
        "w:noBreakHyphen" => {
            let width = measure(ctx, look, "-", false);
            atoms.push(Atom {
                start,
                end,
                kind: Kind::Glyph("-".into()),
                width,
                look: index,
                small: false,
                opportunity: false,
            });
        }
        "w:softHyphen" | "w:commentReference" | "w:annotationRef" => {
            atoms.push(Atom {
                start,
                end,
                kind: Kind::Glyph(String::new()),
                width: 0.0,
                look: index,
                small: false,
                opportunity: true,
            });
        }
        "w:sym" => {
            let character = run.content.character().unwrap_or('□').to_string();
            let width = measure(ctx, look, &character, false);
            atoms.push(Atom {
                start,
                end,
                kind: Kind::Glyph(character),
                width,
                look: index,
                small: false,
                opportunity: false,
            });
        }
        _ => {
            let drawing =
                run.content.picture().map(|_| el).and_then(|el| super::drawing::read(ctx, el));
            match drawing {
                Some(drawing) if drawing.inline => atoms.push(Atom {
                    start,
                    end,
                    kind: Kind::Picture(Box::new(drawing.float.clone())),
                    width: drawing.float.width,
                    look: index,
                    small: false,
                    opportunity: true,
                }),
                Some(drawing) => atoms.push(Atom {
                    start,
                    end,
                    kind: Kind::Float(drawing.float),
                    width: 0.0,
                    look: index,
                    small: false,
                    opportunity: false,
                }),
                None => atoms.push(Atom {
                    start,
                    end,
                    kind: Kind::Glyph(String::new()),
                    width: 0.0,
                    look: index,
                    small: false,
                    opportunity: true,
                }),
            }
        }
    }
}

/// The number of the note being laid out, which a note's own `w:footnoteRef` shows.
fn note_number_for_ref(ctx: &Ctx, endnote: bool) -> String {
    let id = CURRENT_NOTE.with(|current| current.get());
    let marks = if endnote { &ctx.endnote_marks } else { &ctx.footnote_marks };
    id.and_then(|id| marks.get(&id)).cloned().unwrap_or_default()
}

thread_local! {
    /// The note whose text is being laid out, set by the caller that lays notes out.
    pub static CURRENT_NOTE: std::cell::Cell<Option<i64>> = const { std::cell::Cell::new(None) };
}

/// How a document hyphenates: `w:autoHyphenation` with its zone, the most lines in a row that may end
/// in a hyphen, and whether words in capitals are left whole.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hyphenation {
    /// Words are hyphenated only when the space a line would otherwise leave is wider than this.
    pub zone: f32,
    /// The most lines in a row ending in a hyphen; 0 for no limit.
    pub limit: u32,
    pub capitals: bool,
}

impl Hyphenation {
    /// The document's hyphenation settings, `None` when it does not hyphenate.
    pub fn of(document: &crate::model::Document) -> Option<Hyphenation> {
        let settings = &document.settings.as_ref()?.root;
        if props::on_off(settings, "w:autoHyphenation") != Some(true) {
            return None;
        }
        // Word 2013 and later (compatibility mode 15) has no hyphenation zone: it hyphenates a word
        // whenever part of it fits, and reports the zone as undefined. Older documents keep theirs.
        let zone = if super::compatibility_mode(document) >= 15 {
            0.0
        } else {
            settings.val_of("w:hyphenationZone").and_then(props::number).map_or(18.0, props::twips)
        };
        Some(Hyphenation {
            zone,
            limit: settings
                .val_of("w:consecutiveHyphenLimit")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            capitals: props::on_off(settings, "w:doNotHyphenateCaps") != Some(true),
        })
    }
}

/// The places a word may be hyphenated, by the en-US patterns, as byte offsets into it.
fn hyphenation_points(word: &str) -> Vec<usize> {
    use hyphenation::{Hyphenator, Language, Load, Standard};
    static DICTIONARY: std::sync::OnceLock<Option<Standard>> = std::sync::OnceLock::new();
    let Some(dictionary) =
        DICTIONARY.get_or_init(|| Standard::from_embedded(Language::EnglishUS).ok())
    else {
        return Vec::new();
    };
    dictionary.hyphenate(word).breaks
}

/// A line before it is placed: which atoms it holds and why it ended.
#[derive(Debug, Clone, Default)]
pub struct LineBuild {
    atoms: Vec<usize>,
    /// It ended with a line break the person typed, so a justified line is not stretched.
    forced: bool,
    /// It ends inside a word that was hyphenated, so a hyphen is drawn after its last letter.
    pub hyphen: bool,
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

/// Where an absolute position tab stops and how it aligns what follows: the left edge, the middle or the
/// right edge of the margins (the column) or of the paragraph's indents.
fn position_tab(look: &ParaLook, align: TabKind, to_indent: bool, width: f32) -> (f32, TabKind) {
    let (left, right) =
        if to_indent { (look.indent_left, width - look.indent_right) } else { (0.0, width) };
    let stop = match align {
        TabKind::Center => (left + right) / 2.0,
        TabKind::Right => right,
        _ => left,
    };
    (stop, align)
}

/// Where to hyphenate the word a segment holds so its first part and a hyphen fit before `right`: the
/// atom the line ends before, or `None` when the word is not hyphenated (the document does not hyphenate,
/// the gap at the end of the line is inside the zone, too many lines in a row end in a hyphen, it is in
/// capitals and those are left whole, or no break point fits).
#[allow(clippy::too_many_arguments)]
fn hyphenate(
    ctx: &Ctx,
    look: &ParaLook,
    atoms: &[Atom],
    index: usize,
    end: usize,
    pen: f32,
    right: f32,
    (hyphens, hyphen_width): (u32, f32),
) -> Option<usize> {
    let rules = ctx.hyphenation?;
    if look.suppress_hyphens
        || right - pen <= rules.zone
        || (rules.limit > 0 && hyphens >= rules.limit)
    {
        return None;
    }
    // The word is the segment's letters, without the space or punctuation after it.
    let mut word = String::new();
    let mut offsets = Vec::new();
    for (at, atom) in atoms.iter().enumerate().take(end).skip(index) {
        match &atom.kind {
            Kind::Glyph(text) if text.chars().all(char::is_alphabetic) && !text.is_empty() => {
                offsets.push((word.len(), at));
                word.push_str(text);
            }
            _ => break,
        }
    }
    if word.chars().count() < 5 || (!rules.capitals && word.chars().all(char::is_uppercase)) {
        return None;
    }
    let mut best = None;
    for point in hyphenation_points(&word.to_lowercase()) {
        let Some(&(_, at)) = offsets.iter().find(|(offset, _)| *offset == point) else { continue };
        let used: f32 = atoms[index..at].iter().map(|a| a.width).sum();
        if pen + used + hyphen_width <= right + 0.01 {
            best = Some(at);
        }
    }
    best
}

/// Fill lines greedily from segments, each line at the paragraph's indents.
fn break_lines(
    ctx: &Ctx,
    look: &ParaLook,
    atoms: &[Atom],
    width: f32,
    hyphen_width: f32,
) -> Vec<LineBuild> {
    let mut lines: Vec<LineBuild> = Vec::new();
    let mut index = 0;
    let mut hyphens = 0;
    loop {
        let first = lines.is_empty();
        let bounds = (line_left(look, first), width - look.indent_right);
        let (line, next) =
            next_line(ctx, look, atoms, index, first, bounds, width, (hyphens, hyphen_width));
        hyphens = if line.hyphen { hyphens + 1 } else { 0 };
        let forced = line.forced;
        lines.push(line);
        index = next;
        if index >= atoms.len() {
            // A paragraph that ends in a line break has one more, empty line after it.
            if forced {
                lines.push(LineBuild::default());
            }
            return lines;
        }
    }
}

/// One line filled greedily from segments, from atom `from`, between `left` and `right`: the line and the
/// atom the next line starts at. A segment is the atoms up to and including the next place a line may
/// break; one wider than a whole line is broken between graphemes.
#[allow(clippy::too_many_arguments)]
fn next_line(
    ctx: &Ctx,
    look: &ParaLook,
    atoms: &[Atom],
    from: usize,
    first: bool,
    (left, right): (f32, f32),
    width: f32,
    hyphens: (u32, f32),
) -> (LineBuild, usize) {
    let mut line = LineBuild::default();
    let mut pen = left;
    let mut has_content = false;
    let mut index = from;
    // The width of the spaces since the last tab, which a justified line may shrink.
    let shrinks = ctx.shrink_justified && look.align == Align::Justify;
    let mut spaces = 0.0f32;
    // After a right, centre or decimal tab the text that follows ends at (or centres on) the stop, so it
    // fits the line as long as it fits between the text before the tab and the right edge; the pen stays
    // where the tab started, and the stop is where the next tab is measured from.
    let mut aligned_to: Option<f32> = None;
    while index < atoms.len() {
        let atom = &atoms[index];
        if atom.is_special() {
            match atom.kind {
                Kind::Tab => {
                    spaces = 0.0;
                    if let Some(stop) = aligned_to.take() {
                        pen = pen.max(stop);
                    }
                    let (stop, kind, _) = next_tab(ctx, look, pen, first);
                    if stop > right + 0.01 && has_content {
                        return (line, index);
                    }
                    line.atoms.push(index);
                    match kind {
                        TabKind::Right | TabKind::Center | TabKind::Decimal => {
                            aligned_to = Some(stop)
                        }
                        _ => pen = stop.max(pen),
                    }
                }
                Kind::PositionTab { align, to_indent, .. } => {
                    spaces = 0.0;
                    if let Some(stop) = aligned_to.take() {
                        pen = pen.max(stop);
                    }
                    line.atoms.push(index);
                    let (stop, kind) = position_tab(look, align, to_indent, width);
                    match kind {
                        TabKind::Right | TabKind::Center => aligned_to = Some(stop),
                        _ => pen = stop.max(pen),
                    }
                }
                _ => {
                    line.atoms.push(index);
                    line.forced = true;
                    return (line, index + 1);
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
        let trailing: f32 = segment
            .iter()
            .rev()
            .take_while(|a| matches!(a.kind, Kind::Space(_)))
            .map(|a| a.width)
            .sum();
        let inner: f32 = segment
            .iter()
            .filter(|a| matches!(a.kind, Kind::Space(_)))
            .map(|a| a.width)
            .sum::<f32>()
            - trailing;
        // The space after the word counts too, as it does in Word: measured on the paragraphs fixture,
        // a line Word fits needs its thirteen inner spaces 20.1% narrower.
        let room = if shrinks { right + SHRINK * (spaces + inner + trailing) } else { right };
        let fits = pen + total - trailing <= room + 0.01;
        if !fits && has_content {
            // The part of a hyphenated word fits the line its spaces may shrink to, as a whole word does.
            let room = if shrinks { right + SHRINK * (spaces + inner + trailing) } else { right };
            if let Some(cut) = hyphenate(ctx, look, atoms, index, end, pen, room, hyphens) {
                line.atoms.extend(index..cut);
                line.hyphen = true;
                return (line, cut);
            }
            return (line, index);
        }
        if !fits {
            // Wider than a whole line: break between graphemes, at least one on each line.
            for (at, atom) in atoms.iter().enumerate().take(end).skip(index) {
                if has_content
                    && pen + atom.width > right + 0.01
                    && !matches!(atom.kind, Kind::Space(_))
                {
                    return (line, at);
                }
                line.atoms.push(at);
                pen += atom.width;
                has_content |= atom.addressable();
            }
        } else {
            for (at, atom) in atoms.iter().enumerate().take(end).skip(index) {
                line.atoms.push(at);
                has_content |= atom.addressable();
                if matches!(atom.kind, Kind::Space(_)) {
                    spaces += atom.width;
                }
            }
            pen += total;
        }
        index = end;
    }
    (line, index)
}

/// How much of its spaces' width a justified line may give up to take another word, in Word's 2013
/// layout. Measured on the hyphenation and paragraphs fixtures, Word fits lines whose spaces had to
/// shrink by 16 and 17 percent; a fifth is the limit LibreOffice found for the same behaviour.
const SHRINK: f32 = 0.2;

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
    (left, right): (f32, f32),
) -> Chunk {
    // x of each atom on the line and its width, with tabs resolved.
    let mut xs: Vec<(f32, f32)> = Vec::with_capacity(line.atoms.len());
    let mut leaders: Vec<(f32, f32, char, usize)> = Vec::new();
    let mut pen = left;
    for (position, &index) in line.atoms.iter().enumerate() {
        let atom = &atoms[index];
        let tab = match &atom.kind {
            Kind::Tab => Some(next_tab(ctx, look, pen, first)),
            Kind::PositionTab { align, to_indent, leader } => {
                let (stop, _) = position_tab(look, *align, *to_indent, width);
                Some((stop, *align, *leader))
            }
            _ => None,
        };
        if let Some((stop, kind, leader)) = tab {
            let is_tab = |i: usize| matches!(atoms[i].kind, Kind::Tab | Kind::PositionTab { .. });
            let following: f32 = line.atoms[position + 1..]
                .iter()
                .take_while(|&&i| !is_tab(i))
                .map(|&i| atoms[i].width)
                .sum();
            let decimal: f32 = line.atoms[position + 1..]
                .iter()
                .take_while(|&&i| {
                    !is_tab(i) && !matches!(&atoms[i].kind, Kind::Glyph(g) if g == "." || g == ",")
                })
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
        .find(|(&i, _)| {
            !matches!(atoms[i].kind, Kind::Space(_) | Kind::Line | Kind::Page | Kind::Column)
        })
        .map_or(left, |(_, (x, w))| x + w);
    let last_tab = line
        .atoms
        .iter()
        .rposition(|&i| matches!(atoms[i].kind, Kind::Tab | Kind::PositionTab { .. }));
    // A hyphenated line ends with a hyphen after its last letter, which takes room like a letter.
    let hyphen = line
        .hyphen
        .then(|| {
            line.atoms
                .last()
                .map(|&i| (atoms[i].look, measure(ctx, &looks[atoms[i].look], "-", false)))
        })
        .flatten();
    // A justified line may be wider than the column by what its spaces shrink, so its own extra can be
    // below nothing; every other alignment only ever moves text right.
    let raw = right - content_end - hyphen.map_or(0.0, |(_, w)| w);
    let extra = raw.max(0.0);
    match look.align {
        Align::Center => xs.iter_mut().for_each(|(x, _)| *x += extra / 2.0),
        Align::Right => xs.iter_mut().for_each(|(x, _)| *x += extra),
        Align::Justify if !last && !line.forced => {
            let trailing_from = line
                .atoms
                .iter()
                .rposition(|&i| {
                    !matches!(
                        atoms[i].kind,
                        Kind::Space(_) | Kind::Line | Kind::Page | Kind::Column
                    )
                })
                .unwrap_or(0);
            let from = last_tab.map_or(0, |t| t + 1);
            let spaces: Vec<usize> = (from..trailing_from)
                .filter(|&p| matches!(atoms[line.atoms[p]].kind, Kind::Space(_)))
                .collect();
            if !spaces.is_empty() {
                let per = raw / spaces.len() as f32;
                let mut shift = 0.0;
                for (position, slot) in xs.iter_mut().enumerate() {
                    slot.0 += shift;
                    if spaces.contains(&position) {
                        slot.1 += per;
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
    // An inline picture stands on the baseline at its own height. Word does not scale it by the line
    // spacing: measured on the pictures fixture, a line holding only a 90, 60 or 135 point picture is
    // the picture's height and the paragraph mark's descent, at 1.16 line spacing.
    let mut picture: f32 = 0.0;
    for &index in &line.atoms {
        let atom = &atoms[index];
        let look_of = &looks[atom.look];
        match &atom.kind {
            Kind::Picture(float) => {
                picture = picture.max(float.height);
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
        LineRule::Auto(multiple) if picture + below > natural * multiple.max(0.06) => {
            (picture + below, picture)
        }
        LineRule::Auto(multiple) => {
            let height = natural * multiple.max(0.06);
            (height, height - below * multiple.clamp(0.06, 1.0))
        }
        LineRule::AtLeast(at_least) => {
            let height = natural.max(picture + below).max(at_least);
            (height, height - below)
        }
        LineRule::Exact(exact) => (exact, exact - below.min(exact * 0.25)),
    };
    let slack = match look.line {
        LineRule::Auto(multiple) if multiple > 1.0 && picture + below <= natural * multiple => {
            height - natural
        }
        _ => 0.0,
    };
    let mut chunk = Chunk { height, slack, ..Chunk::default() };
    emit(ctx, looks, atoms, line, &xs, baseline, height, &mut chunk);
    // Simple Markup marks a changed line with a bar in the left margin.
    if ctx.markup == crate::review::Markup::Simple
        && line.atoms.iter().any(|&i| looks[atoms[i].look].revised)
    {
        chunk.items.push(Item::Rule {
            x0: -10.0,
            y0: 0.0,
            x1: -10.0,
            y1: height,
            width: 1.0,
            color: REVISION,
            kind: RuleKind::Solid,
        });
    }
    if let (Some((look_index, _)), Some((x, w))) = (hyphen, xs.last()) {
        let look_of = &looks[look_index];
        chunk.items.push(Item::Text(TextItem {
            baseline: baseline - raise_of(&look_of.run),
            style: look_of.style.clone(),
            text: "-".into(),
            xs: vec![x + w],
        }));
    }
    for (x, w, leader, look_index) in leaders {
        let look_of = &looks[look_index];
        let dot = measure(ctx, look_of, &leader.to_string(), false).max(1.0);
        let count = ((w - dot) / dot).floor().max(0.0) as usize;
        if count > 0 {
            let start = x + (w - count as f32 * dot) - dot * 0.5;
            let xs: Vec<f32> = (0..count).map(|n| start + n as f32 * dot).collect();
            chunk.items.push(Item::Text(TextItem {
                baseline,
                style: look_of.style.clone(),
                text: leader.to_string().repeat(count),
                xs,
            }));
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
        let after =
            if matches!(atom.kind, Kind::Line | Kind::Page | Kind::Column) { x } else { x + w };
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
        match &atoms[index].kind {
            Kind::Float(float) => chunk.floats.push(float.clone()),
            Kind::NoteMark(_, Some(id)) if *story == StoryId::Body => chunk.notes.push(*id),
            _ => {}
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
fn emit(
    ctx: &Ctx,
    looks: &[Look],
    atoms: &[Atom],
    line: &LineBuild,
    xs: &[(f32, f32)],
    baseline: f32,
    height: f32,
    chunk: &mut Chunk,
) {
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
        .rposition(|&i| {
            !matches!(atoms[i].kind, Kind::Space(_) | Kind::Line | Kind::Page | Kind::Column)
        })
        .map_or(0, |p| p + 1);
    for (position, &index) in line.atoms.iter().enumerate() {
        let atom = &atoms[index];
        let (x, w) = xs[position];
        let look = &looks[atom.look];
        let raise = raise_of(&look.run);
        let text = match &atom.kind {
            Kind::Glyph(text) | Kind::Space(text) | Kind::Label(text) | Kind::NoteMark(text, _) => {
                Some(text)
            }
            Kind::Picture(float) => {
                let rect = Rect::new(x, baseline - float.height, float.width, float.height);
                pictures.extend(super::drawing::items(float, rect));
                None
            }
            _ => None,
        };
        if let Some(text) = text {
            let style = match (atom.small, &look.small) {
                (true, Some((style, _))) => style.clone(),
                _ => look.style.clone(),
            };
            let same =
                current.as_ref().is_some_and(|(l, s, _)| *l == atom.look && *s == atom.small);
            if !same {
                flush(&mut current, &mut texts);
                current = Some((
                    atom.look,
                    atom.small,
                    TextItem {
                        baseline: baseline - raise,
                        style,
                        text: String::new(),
                        xs: Vec::new(),
                    },
                ));
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
        let decorated = !matches!(
            atom.kind,
            Kind::Label(_) | Kind::Line | Kind::Page | Kind::Column | Kind::Float(_)
        );
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
        let underline = if look.inserted && look.run.underline == Underline::None {
            Underline::Single
        } else {
            look.run.underline
        };
        if underline != Underline::None && !in_trailing && !(space && underline == Underline::Words)
        {
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
            rules.push(Item::Rule {
                x0: x,
                y0: y,
                x1: x + w,
                y1: y,
                width: thickness,
                color: ink,
                kind,
            });
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
    if look.shading.is_none()
        && borders.top.is_none()
        && borders.bottom.is_none()
        && borders.left.is_none()
        && borders.right.is_none()
    {
        return;
    }
    let left = look.indent_left - borders.left.map_or(0.0, |b| b.space + b.width);
    let right = width - look.indent_right + borders.right.map_or(0.0, |b| b.space + b.width);
    let count = chunks.len();
    for (index, chunk) in chunks.iter_mut().enumerate() {
        let h = chunk.height;
        if let Some(color) = look.shading {
            chunk
                .items
                .insert(0, Item::Fill { rect: Rect::new(left, 0.0, right - left, h), color });
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
