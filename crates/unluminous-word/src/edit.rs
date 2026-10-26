//! Every change to a document, as a function of the document and where the change is.
//!
//! Each function takes positions and answers the position the caret should go to. None of them knows
//! about undo; `session` records what a change touched before it calls one. With `Tracking` given, an
//! insertion is wrapped in `w:ins` and a deletion marks text `w:del` rather than removing it, which is what
//! Word's Track Changes is in the file.

use std::sync::Arc;

use crate::model::*;
use crate::position::{self, Pos, StoryId};
use crate::xml::El;

/// Who is making tracked changes, and when.
#[derive(Debug, Clone)]
pub struct Tracking {
    pub author: String,
    /// `2026-10-23T16:25:00Z`.
    pub date: String,
}

impl Tracking {
    /// A `w:ins` or `w:del` for this author, with an id no other revision in the document has.
    fn wrap(&self, name: &str, document: &Document) -> Arc<Wrap> {
        Wrap::new(
            El::new(name)
                .with("w:id", document.next_id().to_string())
                .with("w:author", self.author.clone())
                .with("w:date", self.date.clone()),
        )
    }
}

/// The run properties text typed at `offset` takes: the character before it, or after it at the start of
/// a paragraph, or the paragraph mark's in an empty paragraph. Field characters and codes are skipped,
/// because typing after a field is not typing inside its code.
pub fn props_at(paragraph: &Paragraph, offset: usize) -> (Arc<El>, Vec<Arc<Wrap>>) {
    let mut at = 0;
    let mut before: Option<&Run> = None;
    let mut after: Option<&Run> = None;
    for run in &paragraph.runs {
        let len = run.len();
        let field = matches!(run.content, Content::FieldChar(_) | Content::Instr(_));
        // The run holding the character before `offset` is the one that starts before it, not only one
        // that ends at or before it (`task-2341`). With only the second, typing in the middle of a run
        // took the formatting of the run before that one: a spelling suggestion replacing a word after
        // a bold word came out bold. Inside a run, the run is on both sides, so its wraps are kept too.
        if (at < offset || at + len <= offset) && !field {
            before = Some(run);
        }
        if at < offset && offset < at + len && !field {
            after = Some(run);
        }
        if at >= offset
            && after.is_none()
            && !matches!(run.content, Content::FieldChar(_) | Content::Instr(_))
        {
            after = Some(run);
        }
        at += len;
    }
    let source = before.or(after);
    let props = source.map(|run| Arc::clone(&run.props)).unwrap_or_else(|| {
        let mut mark = paragraph.props.child("w:rPr").cloned().unwrap_or_else(|| El::new("w:rPr"));
        mark.remove_children("w:ins");
        mark.remove_children("w:del");
        Arc::new(mark)
    });
    // A run keeps the wraps both of its neighbours share, so typing in the middle of a hyperlink extends
    // it and typing just after one does not. Tracked change wraps are never inherited this way.
    let shared: Vec<Arc<Wrap>> = match (before, after) {
        (Some(b), Some(a)) => b
            .wraps
            .iter()
            .zip(&a.wraps)
            .take_while(|(x, y)| x.id == y.id)
            .map(|(x, _)| Arc::clone(x))
            .filter(|wrap| {
                !matches!(
                    wrap.el.name.as_str(),
                    "w:ins" | "w:del" | "w:moveFrom" | "w:moveTo" | "w:fldSimple"
                )
            })
            .collect(),
        _ => Vec::new(),
    };
    (props, shared)
}

/// Insert text into one paragraph, without line ends. Tabs become tab runs and U+000B becomes a line
/// break. Answers the offset after the inserted text.
pub fn insert_in_paragraph(
    document: &mut Document,
    story: &StoryId,
    path: &[u32],
    offset: usize,
    text: &str,
    props: Option<Arc<El>>,
    tracking: Option<&Tracking>,
) -> usize {
    let insertion = tracking.map(|t| t.wrap("w:ins", document));
    let Some(paragraph) = position::paragraph_mut(document, story, path) else { return offset };
    let offset = offset.min(paragraph.len());
    let (inherited, mut wraps) = props_at(paragraph, offset);
    let props = props.unwrap_or(inherited);
    if let Some(insertion) = insertion {
        // Joining the insertion just before this one, by the same author, keeps one revision.
        let previous = paragraph.run_at(offset).map(|(i, _)| &paragraph.runs[i]);
        let joined = previous.and_then(|run| {
            run.wraps
                .iter()
                .find(|w| {
                    w.el.name == "w:ins" && w.el.attr("w:author") == insertion.el.attr("w:author")
                })
                .cloned()
        });
        wraps.push(joined.unwrap_or(insertion));
    }
    let index = paragraph.split_at(offset);
    let mut new_runs: Vec<Run> = Vec::new();
    let mut length = 0;
    let mut pending = String::new();
    let push = |runs: &mut Vec<Run>, content: Content| {
        runs.push(Run {
            props: Arc::clone(&props),
            content,
            wraps: wraps.clone(),
            attrs: Arc::default(),
        });
    };
    for character in text.chars() {
        let special = match character {
            '\t' => Some(Content::Tab),
            LINE_BREAK => Some(Content::Break(El::new("w:br"))),
            PAGE_BREAK => Some(Content::Break(El::new("w:br").with("w:type", "page"))),
            COLUMN_BREAK => Some(Content::Break(El::new("w:br").with("w:type", "column"))),
            '\r' | '\n' => continue,
            _ => None,
        };
        match special {
            Some(content) => {
                if !pending.is_empty() {
                    length += pending.len();
                    push(&mut new_runs, Content::Text(std::mem::take(&mut pending)));
                }
                length += content.len();
                push(&mut new_runs, content);
            }
            None => pending.push(character),
        }
    }
    if !pending.is_empty() {
        length += pending.len();
        push(&mut new_runs, Content::Text(pending));
    }
    paragraph.runs.splice(index..index, new_runs);
    for mark in &mut paragraph.marks {
        if mark.offset > offset {
            mark.offset += length;
        }
    }
    paragraph.merge_runs();
    offset + length
}

/// Insert text that may hold line ends, which split paragraphs. Answers where the caret goes.
pub fn insert_text(
    document: &mut Document,
    at: &Pos,
    text: &str,
    props: Option<Arc<El>>,
    tracking: Option<&Tracking>,
) -> Pos {
    let normalised = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut pos = at.clone();
    for (index, line) in normalised.split('\n').enumerate() {
        if index > 0 {
            pos = split_paragraph(document, &pos, tracking);
        }
        let offset = insert_in_paragraph(
            document,
            &pos.story,
            &pos.path,
            pos.offset,
            line,
            props.clone(),
            tracking,
        );
        pos = pos.at(offset);
    }
    pos
}

/// Split the paragraph at a position, as Enter does. The new paragraph after it keeps the properties,
/// except that a heading is followed by its style's `w:next` (`Normal` after `Heading 1`). Answers the
/// start of the new paragraph.
pub fn split_paragraph(document: &mut Document, at: &Pos, tracking: Option<&Tracking>) -> Pos {
    let next_style = {
        let Some(paragraph) = position::paragraph(document, &at.story, &at.path) else {
            return at.clone();
        };
        let at_end = at.offset >= paragraph.len();
        let style = paragraph.props.val_of("w:pStyle").map(str::to_owned);
        let next = style.as_deref().and_then(|id| {
            let resolver = crate::styles::Resolver::new(document);
            resolver.style(id).and_then(|s| s.val_of("w:next")).map(str::to_owned)
        });
        if at_end {
            next.filter(|n| Some(n) != style.as_ref())
        } else {
            None
        }
    };
    let mark_id = document.next_id();
    let blocks = position::story_mut(document, &at.story).expect("the story");
    let Some((container, index)) = position::container_mut(blocks, &at.path) else {
        return at.clone();
    };
    let Some(Block::Paragraph(paragraph)) = container.get_mut(index) else { return at.clone() };
    let offset = at.offset.min(paragraph.len());
    let cut = paragraph.split_at(offset);
    let tail_runs = paragraph.runs.split_off(cut);
    let mut tail_marks = Vec::new();
    paragraph.marks.retain(|mark| {
        if mark.offset > offset
            || (mark.offset == offset && mark.el.name.ends_with("End")) && offset > 0
        {
            tail_marks.push(Mark { offset: mark.offset - offset, ..mark.clone() });
            false
        } else {
            true
        }
    });
    let mut props = paragraph.props.clone();
    // A section ends at the paragraph that carries it, which is now the second half.
    paragraph.props.remove_children("w:sectPr");
    if let Some(next) = next_style {
        props.put(El::new("w:pStyle").with("w:val", next));
        props.remove_children("w:numPr");
    }
    if tracking.is_some() {
        // Word records an inserted paragraph mark on the paragraph that was split, each with an id of
        // its own (task-2359, D5).
        let mark_rpr = paragraph.props.child_or_insert("w:rPr");
        mark_rpr.put(
            El::new("w:ins")
                .with("w:id", mark_id.to_string())
                .with("w:author", tracking.map_or("", |t| t.author.as_str()))
                .with("w:date", tracking.map_or("", |t| t.date.as_str())),
        );
    }
    let attrs: Vec<(String, String)> = paragraph
        .attrs
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "w14:paraId" | "w14:textId"))
        .cloned()
        .collect();
    let mut tail = Paragraph { attrs, props, runs: tail_runs, marks: tail_marks };
    tail.merge_runs();
    paragraph.merge_runs();
    container.insert(index + 1, Block::Paragraph(tail));
    let mut path = at.path.clone();
    *path.last_mut().expect("a path") += 1;
    Pos { story: at.story.clone(), path, offset: 0 }
}

/// Remove `[from, to)` inside one paragraph. With tracking, runs not inserted by this change are marked
/// deleted instead. Answers the offset the caret goes to.
fn delete_in_paragraph(
    document: &mut Document,
    story: &StoryId,
    path: &[u32],
    from: usize,
    to: usize,
    tracking: Option<&Tracking>,
) -> usize {
    let deletion = tracking.map(|t| t.wrap("w:del", document));
    let Some(paragraph) = position::paragraph_mut(document, story, path) else { return from };
    let (from, to) = (from.min(paragraph.len()), to.min(paragraph.len()));
    if from >= to {
        return from;
    }
    let first = paragraph.split_at(from);
    let last = paragraph.split_at(to);
    match deletion {
        Some(deletion) => {
            for run in &mut paragraph.runs[first..last] {
                if run.is_deleted() {
                    continue;
                }
                if run.is_inserted() {
                    // Deleting what this author inserted takes it away for real.
                    run.content = Content::Text(String::new());
                    continue;
                }
                run.wraps.push(Arc::clone(&deletion));
            }
            paragraph
                .runs
                .retain(|run| !(matches!(&run.content, Content::Text(t) if t.is_empty())));
            // The caret stays before the deleted text when deleting backwards; the caller decides.
            from
        }
        None => {
            paragraph.runs.drain(first..last);
            let length = to - from;
            for mark in &mut paragraph.marks {
                if mark.offset >= to {
                    mark.offset -= length;
                } else if mark.offset > from {
                    mark.offset = from;
                }
            }
            paragraph.merge_runs();
            from
        }
    }
}

/// Join the paragraph after `path` onto the end of the one at `path`, as deleting a paragraph mark does.
/// The first paragraph keeps its properties, unless it is empty, when the second's are kept. Answers
/// the offset where the two met.
fn join_with_next(document: &mut Document, story: &StoryId, path: &[u32]) -> Option<usize> {
    let blocks = position::story_mut(document, story)?;
    let (container, index) = position::container_mut(blocks, path)?;
    if !matches!(container.get(index + 1), Some(Block::Paragraph(_))) {
        return None;
    }
    let Block::Paragraph(next) = container.remove(index + 1) else { unreachable!() };
    let Some(Block::Paragraph(paragraph)) = container.get_mut(index) else { return None };
    let offset = paragraph.len();
    if paragraph.runs.is_empty() && paragraph.marks.is_empty() {
        paragraph.props = next.props.clone();
    } else if let Some(section) = next.props.child("w:sectPr") {
        // The section the second paragraph ended now ends here.
        paragraph.props.put(section.clone());
    }
    paragraph.runs.extend(next.runs);
    paragraph
        .marks
        .extend(next.marks.into_iter().map(|mark| Mark { offset: mark.offset + offset, ..mark }));
    paragraph.merge_runs();
    Some(offset)
}

/// Delete everything between two positions in one story. Answers where the caret goes.
pub fn delete_range(document: &mut Document, a: &Pos, b: &Pos, tracking: Option<&Tracking>) -> Pos {
    let (from, to) = if a <= b { (a.clone(), b.clone()) } else { (b.clone(), a.clone()) };
    if from.path == to.path {
        let gone = if tracking.is_none() {
            comments_referenced_between(document, &from, &to)
        } else {
            Vec::new()
        };
        remove_comments(document, &gone);
        let offset = delete_in_paragraph(
            document,
            &from.story,
            &from.path,
            from.offset,
            to.offset,
            tracking,
        );
        return from.at(offset);
    }
    let Some(blocks) = position::story(document, &from.story) else { return from };
    let all = position::paragraphs(blocks);
    let start = all.iter().position(|(p, _)| *p == from.path);
    let end = all.iter().position(|(p, _)| *p == to.path);
    let (Some(start), Some(end)) = (start, end) else { return from };
    let between: Vec<(Vec<u32>, usize)> =
        all[start..=end].iter().map(|(p, para)| (p.clone(), para.len())).collect();
    let same_container = from.path.len() == to.path.len()
        && from.path[..from.path.len() - 1] == to.path[..to.path.len() - 1];
    if tracking.is_some() || !same_container {
        // Text is deleted from every paragraph in the range and the structure is kept: a tracked deletion
        // is shown, not performed, and a range that crosses into a table clears the cells it covers.
        for (index, (path, len)) in between.iter().enumerate().rev() {
            let lo = if index == 0 { from.offset } else { 0 };
            let hi = if index + 1 == between.len() { to.offset } else { *len };
            delete_in_paragraph(document, &from.story, path, lo, hi, tracking);
        }
        return from;
    }
    // One container: take the tail of the first paragraph and the head of the last, remove the blocks in
    // between, and join what is left. The bookmark and comment marks of the paragraphs removed move to
    // where the two halves meet, so no range is left with a start and no end (task-2359, D4).
    let gone = comments_referenced_between(document, &from, &to);
    delete_in_paragraph(document, &from.story, &to.path, 0, to.offset, None);
    let first_index = *from.path.last().expect("a path");
    let last_index = *to.path.last().expect("a path");
    let mut carried: Vec<Mark> = Vec::new();
    {
        let blocks = position::story_mut(document, &from.story).expect("the story");
        let (container, _) = position::container_mut(blocks, &from.path).expect("the container");
        for block in container.drain(first_index as usize + 1..last_index as usize) {
            match block {
                Block::Paragraph(paragraph) => carried.extend(paragraph.marks),
                other => collect_marks(&other, &mut carried),
            }
        }
    }
    let len = position::paragraph(document, &from.story, &from.path).map_or(0, Paragraph::len);
    delete_in_paragraph(document, &from.story, &from.path, from.offset, len, None);
    if let Some(paragraph) = position::paragraph_mut(document, &from.story, &from.path) {
        let at = paragraph.len();
        paragraph.marks.extend(carried.into_iter().map(|mark| Mark { offset: at, ..mark }));
    }
    join_with_next(document, &from.story, &from.path);
    if let Some(paragraph) = position::paragraph_mut(document, &from.story, &from.path) {
        paragraph.marks.sort_by_key(|mark| mark.offset);
    }
    remove_comments(document, &gone);
    from
}

/// The marks inside a block that is not a paragraph (a table or a content control being deleted whole).
fn collect_marks(block: &Block, out: &mut Vec<Mark>) {
    for (_, paragraph) in position::paragraphs(std::slice::from_ref(block)) {
        out.extend(paragraph.marks.iter().cloned());
    }
}

/// The ids of the comments whose reference lies between two positions, which go when the text does.
fn comments_referenced_between(document: &Document, from: &Pos, to: &Pos) -> Vec<String> {
    let Some(blocks) = position::story(document, &from.story) else { return Vec::new() };
    let mut ids = Vec::new();
    for (path, paragraph) in position::paragraphs(blocks) {
        if path < from.path || path > to.path {
            continue;
        }
        let mut at = 0;
        for run in &paragraph.runs {
            let inside =
                (path > from.path || at >= from.offset) && (path < to.path || at < to.offset);
            if let Content::Object(el) = &run.content {
                if inside && el.name == "w:commentReference" {
                    ids.extend(el.attr("w:id").map(str::to_owned));
                }
            }
            at += run.len();
        }
    }
    ids
}

/// Take comments out of the document: their range marks wherever they are and their text in the
/// comments part.
pub fn remove_comments(document: &mut Document, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    let is_range = |mark: &Mark| {
        matches!(mark.el.name.as_str(), "w:commentRangeStart" | "w:commentRangeEnd")
            && mark.el.attr("w:id").is_some_and(|id| ids.iter().any(|gone| gone == id))
    };
    fn visit(blocks: &mut [Block], is_range: &dyn Fn(&Mark) -> bool) {
        for block in blocks {
            match block {
                Block::Paragraph(paragraph) => paragraph.marks.retain(|mark| !is_range(mark)),
                Block::Table(table) => {
                    for row in &mut table.rows {
                        for cell in &mut row.cells {
                            visit(&mut cell.blocks, is_range);
                        }
                    }
                }
                Block::Sdt(sdt) => visit(&mut sdt.blocks, is_range),
                Block::Kept(_) => {}
            }
        }
    }
    visit(&mut document.body, &is_range);
    if let Some(part) = document.comments.as_mut() {
        let before = part.notes.len();
        part.notes.retain(|note| {
            note.el.attr("w:id").is_none_or(|id| !ids.iter().any(|gone| gone == id))
        });
        part.dirty |= part.notes.len() != before;
    }
}

/// What one press of Backspace or Delete removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Character,
    Word,
}

/// Backspace at a caret: the grapheme before it, or the word, or the paragraph mark before it.
pub fn delete_backward(
    document: &mut Document,
    at: &Pos,
    unit: Unit,
    tracking: Option<&Tracking>,
) -> Pos {
    if at.offset == 0 {
        let Some(blocks) = position::story(document, &at.story) else { return at.clone() };
        let Some(previous) = position::previous_paragraph(blocks, &at.path) else {
            // At the very start: Backspace in a list item takes it out of the list, as Word does.
            if let Some(paragraph) = position::paragraph_mut(document, &at.story, &at.path) {
                paragraph.props.remove_children("w:numPr");
            }
            return at.clone();
        };
        // Backspace at the start of a list item first takes it out of the list.
        if let Some(paragraph) = position::paragraph_mut(document, &at.story, &at.path) {
            if paragraph.props.child("w:numPr").is_some() {
                paragraph.props.remove_children("w:numPr");
                return at.clone();
            }
        }
        let same_container = previous.len() == at.path.len()
            && previous[..previous.len() - 1] == at.path[..at.path.len() - 1];
        if !same_container || tracking.is_some() {
            let len = position::paragraph(document, &at.story, &previous).map_or(0, Paragraph::len);
            return Pos { story: at.story.clone(), path: previous, offset: len };
        }
        let offset = join_with_next(document, &at.story, &previous).unwrap_or(0);
        return Pos { story: at.story.clone(), path: previous, offset };
    }
    let Some(paragraph) = position::paragraph(document, &at.story, &at.path) else {
        return at.clone();
    };
    let text = paragraph.flat_text();
    let from = match unit {
        Unit::Character => previous_boundary(&text, at.offset),
        Unit::Word => previous_word(&text, at.offset),
    };
    let offset = delete_in_paragraph(document, &at.story, &at.path, from, at.offset, tracking);
    at.at(offset)
}

/// Delete at a caret: the grapheme after it, or the word, or the paragraph mark after it.
pub fn delete_forward(
    document: &mut Document,
    at: &Pos,
    unit: Unit,
    tracking: Option<&Tracking>,
) -> Pos {
    let Some(paragraph) = position::paragraph(document, &at.story, &at.path) else {
        return at.clone();
    };
    let text = paragraph.flat_text();
    if at.offset >= text.len() {
        let Some(blocks) = position::story(document, &at.story) else { return at.clone() };
        let Some(next) = position::next_paragraph(blocks, &at.path) else { return at.clone() };
        let same_container =
            next.len() == at.path.len() && next[..next.len() - 1] == at.path[..at.path.len() - 1];
        if same_container && tracking.is_none() {
            join_with_next(document, &at.story, &at.path);
        }
        return at.clone();
    }
    let to = match unit {
        Unit::Character => next_boundary(&text, at.offset),
        Unit::Word => next_word(&text, at.offset),
    };
    let offset = delete_in_paragraph(document, &at.story, &at.path, at.offset, to, tracking);
    at.at(offset)
}

/// The grapheme boundary before `offset`.
pub fn previous_boundary(text: &str, offset: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    text[..offset.min(text.len())].grapheme_indices(true).next_back().map_or(0, |(at, _)| at)
}

/// The grapheme boundary after `offset`.
pub fn next_boundary(text: &str, offset: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    let offset = offset.min(text.len());
    text[offset..].graphemes(true).next().map_or(offset, |g| offset + g.len())
}

/// The start of the word before `offset`, skipping the spaces before it first, as Ctrl+Backspace does.
pub fn previous_word(text: &str, offset: usize) -> usize {
    let before = &text[..offset.min(text.len())];
    let trimmed = before.trim_end_matches(|c: char| c.is_whitespace());
    let start = trimmed
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '\'')
        .map_or(0, |at| at + trimmed[at..].chars().next().map_or(1, char::len_utf8));
    if start == trimmed.len() && !trimmed.is_empty() {
        // The character before is punctuation: one character.
        return previous_boundary(text, trimmed.len());
    }
    start
}

/// The start of the next word after `offset`, as Ctrl+Delete and Ctrl+Right go.
pub fn next_word(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    let rest = &text[offset..];
    let word_end =
        rest.find(|c: char| !c.is_alphanumeric() && c != '_' && c != '\'').unwrap_or(rest.len());
    let word_end =
        if word_end == 0 { rest.chars().next().map_or(0, char::len_utf8) } else { word_end };
    let after = &rest[word_end..];
    let spaces = after.len() - after.trim_start_matches(' ').len();
    offset + word_end + spaces
}

/// Change the run properties of every run in `[from, to)` of one paragraph with `change`.
pub fn format_runs_in_paragraph(
    document: &mut Document,
    story: &StoryId,
    path: &[u32],
    from: usize,
    to: usize,
    change: &dyn Fn(&mut El),
) {
    let Some(paragraph) = position::paragraph_mut(document, story, path) else { return };
    if from >= to && !paragraph.runs.is_empty() {
        return;
    }
    let first = paragraph.split_at(from);
    let last = paragraph.split_at(to);
    // Runs that shared one properties value before still share one after, so the file keeps one `w:r`.
    let mut done: Vec<(*const El, Arc<El>)> = Vec::new();
    for run in &mut paragraph.runs[first..last] {
        let key = Arc::as_ptr(&run.props);
        if let Some((_, new)) = done.iter().find(|(old, _)| *old == key) {
            run.props = Arc::clone(new);
            continue;
        }
        let mut props = run.props.as_ref().clone();
        change(&mut props);
        let new = Arc::new(props);
        done.push((key, Arc::clone(&new)));
        run.props = new;
    }
    paragraph.merge_runs();
}

/// Change the run properties of everything between two positions, and of the paragraph marks of the
/// paragraphs it covers entirely, as Word formats a selection.
pub fn format_range(document: &mut Document, a: &Pos, b: &Pos, change: &dyn Fn(&mut El)) {
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    for (path, len, whole) in covered(document, from, to) {
        let lo = if path == from.path { from.offset } else { 0 };
        let hi = if path == to.path { to.offset } else { len };
        format_runs_in_paragraph(document, &from.story, &path, lo, hi, change);
        if whole {
            if let Some(paragraph) = position::paragraph_mut(document, &from.story, &path) {
                change(paragraph.props.child_or_insert("w:rPr"));
            }
        }
    }
}

/// The paragraphs a range covers, with their lengths and whether the range covers each one to its end.
pub fn covered(document: &Document, from: &Pos, to: &Pos) -> Vec<(Vec<u32>, usize, bool)> {
    let Some(blocks) = position::story(document, &from.story) else { return Vec::new() };
    let all = position::paragraphs(blocks);
    let start = all.iter().position(|(p, _)| *p == from.path);
    let end = all.iter().position(|(p, _)| *p == to.path);
    match (start, end) {
        (Some(s), Some(e)) if s <= e => all[s..=e]
            .iter()
            .map(|(path, paragraph)| {
                let len = paragraph.len();
                let whole = (*path != to.path || to.offset >= len)
                    && (*path != from.path || from.offset == 0 || from.path != to.path);
                (path.clone(), len, whole && (s != e || (from.offset == 0 && to.offset >= len)))
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Change the paragraph properties of every paragraph a range touches.
pub fn format_paragraphs(document: &mut Document, a: &Pos, b: &Pos, change: &dyn Fn(&mut El)) {
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    for (path, _, _) in covered(document, from, to) {
        if let Some(paragraph) = position::paragraph_mut(document, &from.story, &path) {
            change(&mut paragraph.props);
        }
    }
}

/// Insert a block (a table, or a paragraph made elsewhere) after the paragraph a position is in,
/// splitting that paragraph first when the position is in its middle. Answers the path of the new block.
pub fn insert_block(document: &mut Document, at: &Pos, block: Block) -> Option<Vec<u32>> {
    let len = position::paragraph(document, &at.story, &at.path)?.len();
    let mut anchor = at.clone();
    let empty = len == 0;
    if at.offset > 0 && at.offset < len {
        anchor = split_paragraph(document, at, None);
        let mut path = anchor.path.clone();
        *path.last_mut()? -= 1;
        anchor = Pos { path, ..anchor };
    }
    let blocks = position::story_mut(document, &at.story)?;
    let (container, index) = position::container_mut(blocks, &anchor.path)?;
    let at_index = if at.offset == 0 && !empty { index } else { index + 1 };
    container.insert(at_index, block);
    // A table must have a paragraph after it in a cell or at the end of the body.
    if container.get(at_index + 1).is_none() {
        container.push(Block::Paragraph(Paragraph::new()));
    }
    let mut path = anchor.path;
    *path.last_mut()? = at_index as u32;
    Some(path)
}

/// A new table of `rows` by `columns` at `width` points, styled `Table Grid` when the document has it.
pub fn new_table(rows: usize, columns: usize, width: f32, style: Option<&str>) -> Table {
    let columns = columns.max(1);
    let column = ((width / columns as f32) * 20.0).round() as i64;
    let mut props = El::new("w:tblPr");
    if let Some(style) = style {
        props.put(El::new("w:tblStyle").with("w:val", style));
    } else {
        let mut borders = El::new("w:tblBorders");
        for side in ["top", "left", "bottom", "right", "insideH", "insideV"] {
            borders.put(
                El::new(format!("w:{side}"))
                    .with("w:val", "single")
                    .with("w:sz", "4")
                    .with("w:space", "0")
                    .with("w:color", "auto"),
            );
        }
        props.put(borders);
    }
    props.put(El::new("w:tblW").with("w:w", "0").with("w:type", "auto"));
    props.put(
        El::new("w:tblLook")
            .with("w:val", "04A0")
            .with("w:firstRow", "1")
            .with("w:lastRow", "0")
            .with("w:firstColumn", "1")
            .with("w:lastColumn", "0")
            .with("w:noHBand", "0")
            .with("w:noVBand", "1"),
    );
    let mut grid = El::new("w:tblGrid");
    for _ in 0..columns {
        grid.children
            .push(crate::xml::Node::El(El::new("w:gridCol").with("w:w", column.to_string())));
    }
    let rows = (0..rows.max(1))
        .map(|_| Row {
            props: El::new("w:trPr"),
            cells: (0..columns)
                .map(|_| Cell {
                    attrs: Vec::new(),
                    props: El::new("w:tcPr").with_child(
                        El::new("w:tcW").with("w:w", column.to_string()).with("w:type", "dxa"),
                    ),
                    blocks: vec![Block::Paragraph(Paragraph::new())],
                })
                .collect(),
            ..Row::default()
        })
        .collect();
    Table { props, grid, rows, extra: Vec::new() }
}

/// The paragraphs a range covers, cut to the range, with their formatting, for a copy that keeps it.
/// Marks are left out, so a pasted copy brings no bookmark or comment range with it.
pub fn copy_fragment(document: &Document, a: &Pos, b: &Pos) -> Vec<Paragraph> {
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    let mut out = Vec::new();
    for (path, len, _) in covered(document, from, to) {
        let Some(paragraph) = position::paragraph(document, &from.story, &path) else { continue };
        let lo = if path == from.path { from.offset } else { 0 };
        let hi = if path == to.path { to.offset } else { len };
        let mut copy = paragraph.clone();
        copy.marks.clear();
        copy.attrs.retain(|(key, _)| !matches!(key.as_str(), "w14:paraId" | "w14:textId"));
        copy.props.remove_children("w:sectPr");
        let end = copy.split_at(hi);
        copy.runs.truncate(end);
        let start = copy.split_at(lo);
        copy.runs.drain(..start);
        // A field cut in two by the range is no longer a field; its pieces paste as the text they show.
        copy.runs.retain(|run| !matches!(run.content, Content::FieldChar(_) | Content::Instr(_)));
        for run in &mut copy.runs {
            run.wraps.retain(|w| {
                !matches!(w.el.name.as_str(), "w:ins" | "w:del" | "w:moveFrom" | "w:moveTo")
            });
            if run.is_deleted() {
                run.content = Content::Text(String::new());
            }
        }
        copy.merge_runs();
        out.push(copy);
    }
    out
}

/// Paste paragraphs copied with [`copy_fragment`] at a position: the first joins the paragraph there, the
/// last takes the rest of it, and any between are paragraphs of their own, each with its own formatting.
/// Answers where the caret goes.
pub fn paste_fragment(
    document: &mut Document,
    at: &Pos,
    fragment: &[Paragraph],
    tracking: Option<&Tracking>,
) -> Pos {
    let Some((first, rest)) = fragment.split_first() else { return at.clone() };
    let insertion = tracking.map(|t| t.wrap("w:ins", document));
    let tag = |runs: &[Run]| -> Vec<Run> {
        runs.iter()
            .cloned()
            .map(|mut run| {
                if let Some(wrap) = &insertion {
                    run.wraps.push(Arc::clone(wrap));
                }
                run
            })
            .collect()
    };
    // One paragraph: its runs go in at the caret.
    if rest.is_empty() {
        let Some(paragraph) = position::paragraph_mut(document, &at.story, &at.path) else {
            return at.clone();
        };
        let offset = at.offset.min(paragraph.len());
        let index = paragraph.split_at(offset);
        let runs = tag(&first.runs);
        let length: usize = runs.iter().map(Run::len).sum();
        paragraph.runs.splice(index..index, runs);
        for mark in &mut paragraph.marks {
            if mark.offset > offset {
                mark.offset += length;
            }
        }
        paragraph.merge_runs();
        return at.at(offset + length);
    }
    // Several: split where the caret is, fill the two halves, and put the middle between them.
    let tail = split_paragraph(document, at, tracking);
    let mut head_path = tail.path.clone();
    *head_path.last_mut().expect("a path") -= 1;
    if let Some(head) = position::paragraph_mut(document, &at.story, &head_path) {
        let was_empty = head.is_empty();
        head.runs.extend(tag(&first.runs));
        if was_empty {
            head.props = first.props.clone();
        }
        head.merge_runs();
    }
    let (middle, last) = rest.split_at(rest.len() - 1);
    let mut inserted = 0u32;
    if let Some(blocks) = position::story_mut(document, &at.story) {
        if let Some((container, index)) = position::container_mut(blocks, &tail.path) {
            for (k, paragraph) in middle.iter().enumerate() {
                let mut paragraph = paragraph.clone();
                paragraph.runs = tag(&paragraph.runs);
                container.insert(index + k, Block::Paragraph(paragraph));
                inserted += 1;
            }
        }
    }
    let mut last_path = tail.path.clone();
    *last_path.last_mut().expect("a path") += inserted;
    let last = &last[0];
    let mut offset = 0;
    if let Some(paragraph) = position::paragraph_mut(document, &at.story, &last_path) {
        let runs = tag(&last.runs);
        offset = runs.iter().map(Run::len).sum();
        for mark in &mut paragraph.marks {
            mark.offset += offset;
        }
        paragraph.runs.splice(0..0, runs);
        // The pasted last paragraph's own formatting, as Word gives the paragraph that ends a paste.
        let section = paragraph.props.child("w:sectPr").cloned();
        paragraph.props = last.props.clone();
        if let Some(section) = section {
            paragraph.props.put(section);
        }
        paragraph.merge_runs();
    }
    Pos { story: at.story.clone(), path: last_path, offset }
}

/// Accept or reject every tracked change in a list of blocks. Answers how many were resolved.
pub fn resolve_revisions(
    blocks: &mut [Block],
    accept: bool,
    only: Option<&dyn Fn(&Run) -> bool>,
) -> usize {
    let mut count = 0;
    for block in blocks.iter_mut() {
        match block {
            Block::Paragraph(paragraph) => count += resolve_in_paragraph(paragraph, accept, only),
            Block::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        count += resolve_revisions(&mut cell.blocks, accept, only);
                    }
                }
            }
            Block::Sdt(sdt) => count += resolve_revisions(&mut sdt.blocks, accept, only),
            Block::Kept(_) => {}
        }
    }
    count
}

fn resolve_in_paragraph(
    paragraph: &mut Paragraph,
    accept: bool,
    only: Option<&dyn Fn(&Run) -> bool>,
) -> usize {
    let mut count = 0;
    let mut kept: Vec<Run> = Vec::with_capacity(paragraph.runs.len());
    for mut run in std::mem::take(&mut paragraph.runs) {
        let chosen = only.is_none_or(|f| f(&run));
        let inserted = run.is_inserted();
        let deleted = run.is_deleted();
        if chosen && (inserted || deleted) {
            count += 1;
            let keep = if inserted { accept } else { !accept };
            if !keep {
                continue;
            }
            run.wraps.retain(|w| {
                !matches!(w.el.name.as_str(), "w:ins" | "w:del" | "w:moveFrom" | "w:moveTo")
            });
        }
        if chosen && run.props.child("w:rPrChange").is_some() {
            count += 1;
            let mut props = run.props.as_ref().clone();
            if !accept {
                if let Some(old) =
                    props.child("w:rPrChange").and_then(|c| c.child("w:rPr")).cloned()
                {
                    props = El { name: "w:rPr".into(), ..old };
                }
            }
            props.remove_children("w:rPrChange");
            run.props = Arc::new(props);
        }
        kept.push(run);
    }
    paragraph.runs = kept;
    if let Some(mark) = paragraph.props.child_mut("w:rPr") {
        if mark.child("w:ins").is_some() || mark.child("w:del").is_some() {
            mark.remove_children("w:ins");
            mark.remove_children("w:del");
            count += 1;
        }
    }
    if paragraph.props.child("w:pPrChange").is_some() {
        count += 1;
        if !accept {
            if let Some(old) =
                paragraph.props.child("w:pPrChange").and_then(|c| c.child("w:pPr")).cloned()
            {
                let keep_rpr = paragraph.props.child("w:rPr").cloned();
                let keep_sect = paragraph.props.child("w:sectPr").cloned();
                paragraph.props = El { name: "w:pPr".into(), ..old };
                if let Some(rpr) = keep_rpr {
                    paragraph.props.put(rpr);
                }
                if let Some(sect) = keep_sect {
                    paragraph.props.put(sect);
                }
            }
        }
        paragraph.props.remove_children("w:pPrChange");
    }
    paragraph.merge_runs();
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(paragraphs: &[&str]) -> Document {
        let mut document = crate::new::blank();
        document.body = paragraphs
            .iter()
            .map(|text| {
                let mut p = Paragraph::new();
                if !text.is_empty() {
                    p.runs.push(Run::text(*text, no_props()));
                }
                Block::Paragraph(p)
            })
            .collect();
        document
    }

    fn texts(document: &Document) -> Vec<String> {
        position::paragraphs(&document.body).into_iter().map(|(_, p)| p.flat_text()).collect()
    }

    #[test]
    fn typing_inserts_and_enter_splits() {
        let mut doc = document(&["Hello world"]);
        let pos = insert_text(&mut doc, &Pos::new(vec![0], 5), ",", None, None);
        assert_eq!(texts(&doc), ["Hello, world"]);
        let pos = insert_text(&mut doc, &pos, "\nthere", None, None);
        assert_eq!(texts(&doc), ["Hello,", "there world"]);
        assert_eq!(pos, Pos::new(vec![1], 5));
    }

    /// Typing in the middle of a plain run that follows a bold one stays plain (`task-2341`).
    #[test]
    fn typing_inside_a_run_takes_that_runs_formatting() {
        let mut doc = document(&[""]);
        let bold = Arc::new(El::new("w:rPr").with_child(El::new("w:b")));
        let Block::Paragraph(p) = &mut doc.body[0] else { panic!() };
        p.runs = vec![Run::text("bold", bold), Run::text(" plain words", no_props())];
        insert_text(&mut doc, &Pos::new(vec![0], 8), "er", None, None);
        assert_eq!(texts(&doc), ["bold plaerin words"]);
        let Block::Paragraph(p) = &doc.body[0] else { panic!() };
        assert_eq!(p.runs.len(), 2, "the typed letters join the plain run");
        assert!(p.runs[1].props.child("w:b").is_none());
    }

    #[test]
    fn a_tab_in_typed_text_is_a_tab_run() {
        let mut doc = document(&[""]);
        insert_text(&mut doc, &Pos::new(vec![0], 0), "a\tb", None, None);
        let Block::Paragraph(p) = &doc.body[0] else { panic!() };
        assert_eq!(p.runs.len(), 3);
        assert_eq!(p.runs[1].content, Content::Tab);
    }

    #[test]
    fn deleting_across_paragraphs_joins_them() {
        let mut doc = document(&["one two", "three", "four five"]);
        let pos = delete_range(&mut doc, &Pos::new(vec![0], 3), &Pos::new(vec![2], 4), None);
        assert_eq!(texts(&doc), ["one five"]);
        assert_eq!(pos, Pos::new(vec![0], 3));
    }

    #[test]
    fn backspace_at_a_paragraph_start_joins_it_to_the_one_before() {
        let mut doc = document(&["ab", "cd"]);
        let pos = delete_backward(&mut doc, &Pos::new(vec![1], 0), Unit::Character, None);
        assert_eq!(texts(&doc), ["abcd"]);
        assert_eq!(pos, Pos::new(vec![0], 2));
        let pos = delete_backward(&mut doc, &pos, Unit::Character, None);
        assert_eq!(texts(&doc), ["acd"]);
        assert_eq!(pos.offset, 1);
    }

    #[test]
    fn ctrl_backspace_takes_the_word() {
        assert_eq!(previous_word("one two  ", 9), 4);
        assert_eq!(previous_word("one two", 7), 4);
        assert_eq!(next_word("one two", 0), 4);
        assert_eq!(next_word("one, two", 3), 5);
    }

    #[test]
    fn formatting_a_range_splits_runs_and_shares_the_new_properties() {
        let mut doc = document(&["make this bold"]);
        format_range(&mut doc, &Pos::new(vec![0], 5), &Pos::new(vec![0], 9), &|rpr: &mut El| {
            crate::props::set_on_off(rpr, "w:b", Some(true))
        });
        let Block::Paragraph(p) = &doc.body[0] else { panic!() };
        assert_eq!(p.runs.len(), 3);
        assert!(p.runs[1].props.child("w:b").is_some());
        assert!(p.runs[0].props.child("w:b").is_none());
    }

    #[test]
    fn a_tracked_deletion_keeps_the_text_marked_and_rejecting_brings_it_back() {
        let mut doc = document(&["keep this text"]);
        let tracking = Tracking { author: "Ann".into(), date: "2026-10-23T00:00:00Z".into() };
        delete_range(&mut doc, &Pos::new(vec![0], 4), &Pos::new(vec![0], 9), Some(&tracking));
        assert_eq!(texts(&doc), ["keep this text"]);
        let Block::Paragraph(p) = &doc.body[0] else { panic!() };
        assert_eq!(p.visible_text(), "keep text");
        let mut rejected = doc.clone();
        assert_eq!(resolve_revisions(&mut rejected.body, false, None), 1);
        assert_eq!(texts(&rejected), ["keep this text"]);
        assert_eq!(resolve_revisions(&mut doc.body, true, None), 1);
        assert_eq!(texts(&doc), ["keep text"]);
    }

    #[test]
    fn a_tracked_insertion_is_wrapped_and_deleting_it_removes_it() {
        let mut doc = document(&["ab"]);
        let tracking = Tracking { author: "Ann".into(), date: "2026-10-23T00:00:00Z".into() };
        let pos = insert_text(&mut doc, &Pos::new(vec![0], 1), "XY", None, Some(&tracking));
        let Block::Paragraph(p) = &doc.body[0] else { panic!() };
        assert!(p.runs.iter().any(|r| r.is_inserted()));
        delete_backward(&mut doc, &pos, Unit::Character, Some(&tracking));
        assert_eq!(texts(&doc), ["aXb"]);
    }

    #[test]
    fn a_copied_range_pastes_with_its_formatting_and_its_paragraphs() {
        let mut doc = document(&["one two", "three four"]);
        format_range(&mut doc, &Pos::new(vec![0], 4), &Pos::new(vec![0], 7), &|rpr: &mut El| {
            crate::props::set_on_off(rpr, "w:b", Some(true))
        });
        let fragment = copy_fragment(&doc, &Pos::new(vec![0], 4), &Pos::new(vec![1], 5));
        assert_eq!(fragment.len(), 2);
        assert_eq!(fragment[0].flat_text(), "two");
        assert!(fragment[0].runs[0].props.child("w:b").is_some(), "the bold came with it");
        let pos = paste_fragment(&mut doc, &Pos::new(vec![1], 10), &fragment, None);
        assert_eq!(texts(&doc), ["one two", "three fourtwo", "three"]);
        assert_eq!(pos, Pos::new(vec![2], 5));
        let single = copy_fragment(&doc, &Pos::new(vec![0], 0), &Pos::new(vec![0], 3));
        paste_fragment(&mut doc, &Pos::new(vec![2], 0), &single, None);
        assert_eq!(texts(&doc)[2], "onethree");
    }

    #[test]
    fn a_table_goes_after_the_paragraph_and_leaves_a_paragraph_after_it() {
        let mut doc = document(&["before"]);
        let path = insert_block(
            &mut doc,
            &Pos::new(vec![0], 6),
            Block::Table(new_table(2, 3, 468.0, None)),
        )
        .unwrap();
        assert_eq!(path, vec![1]);
        assert!(matches!(doc.body[1], Block::Table(_)));
        assert!(matches!(doc.body[2], Block::Paragraph(_)));
    }
}
