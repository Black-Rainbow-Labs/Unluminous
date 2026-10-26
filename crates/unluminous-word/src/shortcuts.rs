//! The edits Word gives a keyboard chord and nothing else (task-2359, TDD 5.5): hanging indent, resetting
//! a paragraph's own formatting, the 12 point space before, promoting and demoting a heading, moving a
//! paragraph up or down, stepping to the next field, the Unicode toggle (Alt+X), the special characters
//! and the Symbol font.
//!
//! Each is one [`Session`] change, so one undo step.

use std::sync::Arc;

use crate::model::*;
use crate::position::{self, Pos, Selection};
use crate::session::{Scope, Session};
use crate::xml::El;

/// Word's hanging indent step: one default tab stop, half an inch.
const STEP_TWIPS: i64 = 720;

/// Where every outermost field of a story starts and ends: a complex field from its begin character to
/// its end character, and a `w:fldSimple` over the runs it wraps. Field shading and Next Field read it.
pub fn field_ranges(document: &Document, story: &crate::position::StoryId) -> Vec<(Pos, Pos)> {
    let Some(blocks) = position::story(document, story) else { return Vec::new() };
    let mut fields: Vec<(Pos, Pos)> = Vec::new();
    for (path, paragraph) in position::paragraphs(blocks) {
        let mut offset = 0;
        let mut open: Vec<usize> = Vec::new();
        let mut simple: Option<(u64, usize, usize)> = None;
        let at = |offset: usize| Pos { story: story.clone(), path: path.clone(), offset };
        for run in &paragraph.runs {
            match run.content.character() {
                Some(FIELD_BEGIN) => open.push(offset),
                Some(FIELD_END) => {
                    if let Some(start) = open.pop() {
                        if open.is_empty() {
                            fields.push((at(start), at(offset + run.len())));
                        }
                    }
                }
                _ => {}
            }
            let wrap = run.wraps.iter().find(|w| w.el.name == "w:fldSimple").map(|w| w.id);
            match (wrap, simple) {
                (Some(id), Some((open_id, start, _))) if id == open_id => {
                    simple = Some((id, start, offset + run.len()))
                }
                (wrap, previous) => {
                    if let Some((_, start, end)) = previous {
                        fields.push((at(start), at(end)));
                    }
                    simple = wrap.map(|id| (id, offset, offset + run.len()));
                }
            }
            offset += run.len();
        }
        if let Some((_, start, end)) = simple {
            fields.push((at(start), at(end)));
        }
    }
    fields.sort_by(|a, b| a.0.cmp(&b.0));
    fields
}

impl Session {
    /// Ctrl+T and Ctrl+Shift+T: indent every line but the first one more step, or one less.
    pub fn hanging_indent(&mut self, more: bool) {
        self.format_paragraphs(move |ppr| {
            let ind = ppr.child_or_insert("w:ind");
            let read = |el: &El, name: &str| {
                el.attr(name).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0)
            };
            let left = read(ind, "w:left").max(read(ind, "w:start"));
            let hanging = read(ind, "w:hanging");
            let (left, hanging) = if more {
                (left + STEP_TWIPS, hanging + STEP_TWIPS)
            } else {
                ((left - STEP_TWIPS).max(0), (hanging - STEP_TWIPS).max(0))
            };
            ind.remove_attr("w:start");
            ind.remove_attr("w:firstLine");
            ind.set_attr("w:left", left.to_string());
            if hanging > 0 {
                ind.set_attr("w:hanging", hanging.to_string());
            } else {
                ind.remove_attr("w:hanging");
            }
        });
    }

    /// Ctrl+Q: take away a paragraph's own formatting, leaving its style, its list and its section.
    pub fn reset_paragraph(&mut self) {
        self.format_paragraphs(|ppr| {
            let keep = ["w:pStyle", "w:numPr", "w:sectPr", "w:rPr", "w:pPrChange"];
            ppr.children.retain(
                |n| matches!(n, crate::xml::Node::El(e) if keep.contains(&e.name.as_str())),
            );
        });
    }

    /// Ctrl+0: twelve points before the paragraph, or none when it already has twelve.
    pub fn toggle_space_before(&mut self) {
        let has = self.paragraph_at_caret().space_before >= 11.9;
        self.format_paragraphs(move |ppr| {
            let spacing = ppr.child_or_insert("w:spacing");
            spacing.set_attr("w:before", if has { "0" } else { "240" });
            spacing.remove_attr("w:beforeAutospacing");
        });
    }

    /// Alt+Shift+Left and Right: a heading one level up or down. Body text promoted takes the level of
    /// the heading above it, and Heading 9 demoted becomes body text, as Word's outline does.
    pub fn promote_heading(&mut self, promote: bool) -> Result<(), String> {
        let next = self.promoted_style(promote)?;
        self.apply_style(&next)
    }

    /// The heading level of the paragraph at the caret, `None` for body text (`task-2366`).
    pub fn heading_level_at_caret(&self) -> Option<u8> {
        let at = &self.selection.head;
        let resolver = crate::styles::Resolver::new(&self.document);
        position::paragraph(&self.document, &at.story, &at.path)
            .and_then(|p| resolver.paragraph_style_id(&p.props).and_then(heading_level))
    }

    /// The style promoting or demoting the paragraph at the caret gives it, or why it cannot be. The menus
    /// ask this before they offer Promote and Demote, so neither is offered where it would fail.
    pub fn promoted_style(&self, promote: bool) -> Result<String, String> {
        let at = self.selection.head.clone();
        let resolver = crate::styles::Resolver::new(&self.document);
        let style = position::paragraph(&self.document, &at.story, &at.path)
            .and_then(|p| resolver.paragraph_style_id(&p.props).map(str::to_owned))
            .unwrap_or_else(|| "Normal".into());
        let level = heading_level(&style);
        let above = || -> u8 {
            let all = position::paragraphs(&self.document.body);
            all.iter()
                .take_while(|(path, _)| *path != at.path)
                .filter_map(|(_, p)| resolver.paragraph_style_id(&p.props).and_then(heading_level))
                .last()
                .unwrap_or(1)
        };
        let next = match (level, promote) {
            (Some(1), true) => return Err("Heading 1 is the top level".into()),
            (Some(n), true) => format!("Heading{}", n - 1),
            (Some(9), false) => "Normal".into(),
            (Some(n), false) => format!("Heading{}", n + 1),
            (None, true) => format!("Heading{}", above()),
            (None, false) => return Err("body text is already the lowest level".into()),
        };
        Ok(next)
    }

    /// Alt+Shift+Up and Down: move the paragraph at the caret above the one before it or below the one
    /// after it, inside the same body, cell or content control. The caret goes with it.
    pub fn move_paragraph(&mut self, up: bool) -> Result<(), String> {
        let at = self.selection.head.clone();
        if at.path.is_empty() {
            return Err("the caret is not in a paragraph".into());
        }
        self.move_inside(at, up)
    }

    /// Whether the paragraph at the caret can move up or down inside its body, cell or content control, or
    /// why not.
    pub fn paragraph_can_move(&self, up: bool) -> Result<(), String> {
        let at = &self.selection.head;
        let Some(&last) = at.path.last() else {
            return Err("the caret is not in a paragraph".into());
        };
        let blocks = position::story(&self.document, &at.story).ok_or("the story has gone")?;
        let mut other = at.path.clone();
        let moves = match up {
            true => last > 0,
            false => {
                *other.last_mut().expect("checked") = last + 1;
                position::block(blocks, &other).is_some()
            }
        };
        match (moves, up) {
            (true, _) => Ok(()),
            (false, true) => Err("this is the first paragraph here".into()),
            (false, false) => Err("this is the last paragraph here".into()),
        }
    }

    fn move_inside(&mut self, at: Pos, up: bool) -> Result<(), String> {
        let index = *at.path.last().expect("checked") as usize;
        self.paragraph_can_move(up)?;
        self.change(Scope::Story, move |document, selection, _| {
            let Some(blocks) = position::story_mut(document, &at.story) else {
                return selection.clone();
            };
            let Some((container, _)) = position::container_mut(blocks, &at.path) else {
                return selection.clone();
            };
            let other = if up { index - 1 } else { index + 1 };
            container.swap(index, other);
            let moved = |p: &Pos| {
                let mut p = p.clone();
                if let Some(last) = p.path.last_mut() {
                    *last = other as u32;
                }
                p
            };
            Selection { anchor: moved(&selection.anchor), head: moved(&selection.head) }
        });
        Ok(())
    }

    /// Outline view's Move Up and Move Down on a collapsed heading: the heading and everything under it,
    /// up to the next heading of its level or higher, swapped with the section of the same level before
    /// or after it. The caret goes with it.
    pub fn move_section(&mut self, up: bool) -> Result<(), String> {
        let (start, middle, end) = self.section_bounds(up)?;
        self.change(Scope::Story, move |document, selection, _| {
            document.body[start..end].rotate_left(middle - start);
            // The moved heading is now at `start` when it went up, and after the other section when down.
            let at = if up { start } else { start + (end - middle) };
            let moved =
                |p: &Pos| Pos { story: p.story.clone(), path: vec![at as u32], offset: p.offset };
            Selection { anchor: moved(&selection.anchor), head: moved(&selection.head) }
        });
        Ok(())
    }

    /// The blocks a move of the section at the caret rotates, as the start, the block that comes first
    /// after the move and the end, or why the section cannot move (`task-2366`).
    pub fn section_bounds(&self, up: bool) -> Result<(usize, usize, usize), String> {
        let head = self.selection.head.clone();
        if head.story != crate::position::StoryId::Body || head.path.len() != 1 {
            return Err("Outline view moves headings in the body".into());
        }
        let resolver = crate::styles::Resolver::new(&self.document);
        let levels: Vec<u8> = self
            .document
            .body
            .iter()
            .map(|block| match block {
                Block::Paragraph(p) => {
                    resolver.paragraph(&p.props, None).outline_level.map_or(10, |l| l + 1)
                }
                _ => 10,
            })
            .collect();
        let i = head.path[0] as usize;
        let level = levels[i];
        let end_of = |from: usize| {
            (from + 1..levels.len()).find(|k| levels[*k] <= level).unwrap_or(levels.len())
        };
        let j = end_of(i);
        let (start, middle, end) = if up {
            let p =
                (0..i).rev().find(|k| levels[*k] <= level).ok_or("this is the first section")?;
            if levels[p] < level {
                return Err("the heading above is a higher level".into());
            }
            (p, i, j)
        } else {
            if j >= levels.len() || levels[j] < level {
                return Err("this is the last section at its level".into());
            }
            (i, j, end_of(j))
        };
        Ok((start, middle, end))
    }

    /// Sort the selected paragraphs of one body, cell or content control by their text, A to Z or Z to A,
    /// as Home's Sort does for paragraphs. Numbers sort by value when every paragraph starts with one.
    pub fn sort_paragraphs(&mut self, ascending: bool) -> Result<(), String> {
        let (from, to) = (self.selection.ordered().0.clone(), self.selection.ordered().1.clone());
        if from.story != to.story
            || from.path.len() != to.path.len()
            || from.path[..from.path.len() - 1] != to.path[..to.path.len() - 1]
        {
            return Err("select paragraphs next to each other to sort them".into());
        }
        let (first, last) = (
            *from.path.last().expect("a path") as usize,
            *to.path.last().expect("a path") as usize,
        );
        if last <= first {
            return Err("select two paragraphs or more to sort them".into());
        }
        self.change(Scope::Story, move |document, selection, _| {
            let Some(blocks) = position::story_mut(document, &from.story) else {
                return selection.clone();
            };
            let Some((container, _)) = position::container_mut(blocks, &from.path) else {
                return selection.clone();
            };
            let range = first..(last + 1).min(container.len());
            if !container[range.clone()].iter().all(|b| matches!(b, Block::Paragraph(_))) {
                return selection.clone();
            }
            let text = |b: &Block| b.as_paragraph().map(|p| p.visible_text()).unwrap_or_default();
            let number = |t: &str| {
                t.trim()
                    .split(|c: char| !c.is_ascii_digit() && c != '.')
                    .next()
                    .and_then(|n| n.parse::<f64>().ok())
            };
            let numeric = container[range.clone()].iter().all(|b| number(&text(b)).is_some());
            let mut part: Vec<Block> = container.drain(range.clone()).collect();
            part.sort_by(|a, b| {
                let order = if numeric {
                    number(&text(a)).unwrap_or(0.0).total_cmp(&number(&text(b)).unwrap_or(0.0))
                } else {
                    text(a).to_lowercase().cmp(&text(b).to_lowercase())
                };
                if ascending {
                    order
                } else {
                    order.reverse()
                }
            });
            for (i, block) in part.into_iter().enumerate() {
                container.insert(first + i, block);
            }
            selection.clone()
        });
        Ok(())
    }

    /// What a cross reference can point at: every heading, every caption and every paragraph holding a
    /// bookmark a person named, with the words Word lists it by.
    pub fn cross_reference_targets(&self) -> Vec<(Vec<u32>, String)> {
        let resolver = crate::styles::Resolver::new(&self.document);
        let mut out = Vec::new();
        for (path, paragraph) in position::paragraphs(&self.document.body) {
            let look = resolver.paragraph(&paragraph.props, None);
            let text: String = paragraph.visible_text().chars().take(60).collect();
            if let Some(level) = look.outline_level {
                out.push((path.clone(), format!("Heading {}: {text}", level + 1)));
            } else if look.style == "Caption" {
                out.push((path.clone(), format!("Caption: {text}")));
            }
            for mark in &paragraph.marks {
                if mark.el.name == "w:bookmarkStart" {
                    if let Some(name) = mark.el.attr("w:name").filter(|n| !n.starts_with('_')) {
                        out.push((path.clone(), format!("Bookmark {name}: {text}")));
                    }
                }
            }
        }
        out
    }

    /// Whether the run at the caret sets `name` itself, such as `w14:ligatures`.
    pub fn look_at_caret_rpr_has(&self, name: &str) -> bool {
        let head = &self.selection.head;
        position::paragraph(&self.document, &head.story, &head.path)
            .is_some_and(|p| crate::edit::props_at(p, head.offset).0.child(name).is_some())
    }

    /// A cover page at the top of the document: the title in Title, the subtitle in Subtitle, the
    /// author and the date, then a page break, as Word's plainest cover page has them.
    pub fn insert_cover_page(
        &mut self,
        title: &str,
        subtitle: &str,
        author: &str,
    ) -> Result<(), String> {
        for style in ["Title", "Subtitle"] {
            crate::builtin::ensure_style(&mut self.document, style)?;
        }
        let lines: Vec<(Option<&str>, String)> = vec![
            (Some("Title"), title.to_owned()),
            (Some("Subtitle"), subtitle.to_owned()),
            (None, author.to_owned()),
            (
                None,
                crate::fields::format_date(&crate::session::now_iso(), Some("MMMM d, yyyy"), false),
            ),
        ];
        self.change(Scope::Story, move |document, selection, _| {
            let mut blocks: Vec<Block> = Vec::new();
            for (style, text) in &lines {
                let mut paragraph = Paragraph::new();
                if let Some(style) = style {
                    paragraph.props.put(El::new("w:pStyle").with("w:val", *style));
                }
                if !text.is_empty() {
                    paragraph.runs.push(Run::text(text.clone(), no_props()));
                }
                blocks.push(Block::Paragraph(paragraph));
            }
            let mut breaker = Paragraph::new();
            breaker.runs.push(Run {
                content: Content::Break(El::new("w:br").with("w:type", "page")),
                ..Run::text("", no_props())
            });
            blocks.push(Block::Paragraph(breaker));
            for (i, block) in blocks.into_iter().enumerate() {
                document.body.insert(i, block);
            }
            let _ = selection;
            // The caret goes to the title, to type it.
            Selection::caret(Pos::new(vec![0], 0))
        });
        Ok(())
    }

    /// F11 and Shift+F11: select the next field after the caret, or the one before it, in the story.
    pub fn select_next_field(&mut self, forward: bool) -> Result<(), String> {
        let head = self.selection.ordered().1.clone();
        let tail = self.selection.ordered().0.clone();
        let fields = field_ranges(&self.document, &head.story);
        let chosen = if forward {
            fields.into_iter().find(|(start, _)| *start > tail || (*start == tail && head == tail))
        } else {
            fields.into_iter().rev().find(|(start, _)| *start < tail)
        };
        let (start, end) = chosen.ok_or("there is no field there")?;
        self.select(start, end);
        Ok(())
    }

    /// Alt+X: the hexadecimal code before the caret turned into its character, or the character before
    /// the caret turned into its code, as Word does. A selection is converted whole.
    pub fn toggle_unicode(&mut self) -> Result<(), String> {
        let (from, to) = (self.selection.ordered().0.clone(), self.selection.ordered().1.clone());
        let paragraph = position::paragraph(&self.document, &to.story, &to.path)
            .ok_or("the caret is not in text")?;
        let text = paragraph.flat_text();
        let end = to.offset.min(text.len());
        let start = if from.path == to.path && from.offset < to.offset {
            from.offset
        } else {
            // Up to six hexadecimal digits before the caret, after an optional U+.
            let digits =
                text[..end].chars().rev().take_while(|c| c.is_ascii_hexdigit()).take(6).count();
            let mut start = end - digits;
            if digits > 0 && text[..start].ends_with("U+") {
                start -= 2;
            }
            if digits == 0 {
                start = text[..end].char_indices().last().map_or(end, |(i, _)| i);
            }
            start
        };
        let chosen = &text[start..end];
        let hex = chosen.trim_start_matches("U+");
        let replacement = match u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
            Some(c) if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) => {
                c.to_string()
            }
            _ => chosen.chars().map(|c| format!("{:04X}", c as u32)).collect(),
        };
        if replacement.is_empty() {
            return Err("there is nothing before the caret".into());
        }
        self.select(to.at(start), to.at(end));
        self.type_text(&replacement);
        Ok(())
    }

    /// A non breaking hyphen (Ctrl+Shift+Hyphen) or an optional hyphen (Ctrl+Hyphen) at the caret, as
    /// the elements Word writes for them.
    pub fn insert_hyphen(&mut self, optional: bool) {
        let el = El::new(if optional { "w:softHyphen" } else { "w:noBreakHyphen" });
        self.change(Scope::Story, move |document, selection, _| {
            let at = selection.ordered().1.clone();
            let (props, wraps) = position::paragraph(document, &at.story, &at.path)
                .map(|p| crate::edit::props_at(p, at.offset))
                .unwrap_or_default();
            let Some(paragraph) = position::paragraph_mut(document, &at.story, &at.path) else {
                return selection.clone();
            };
            let index = paragraph.split_at(at.offset);
            let run = Run {
                props: Arc::clone(&props),
                content: Content::Object(el),
                wraps,
                attrs: Arc::default(),
            };
            let len = run.len();
            paragraph.runs.insert(index, run);
            for mark in &mut paragraph.marks {
                if mark.offset > at.offset {
                    mark.offset += len;
                }
            }
            Selection::caret(at.at(at.offset + len))
        });
    }

    /// Ctrl+Shift+Q: the selection, or what is typed next, in the Symbol font.
    pub fn symbol_font(&mut self) {
        self.format(|rpr| {
            let fonts = El::new("w:rFonts").with("w:ascii", "Symbol").with("w:hAnsi", "Symbol");
            rpr.put(fonts);
        });
    }
}

/// The level of a heading style, `Heading1` to `Heading9`.
fn heading_level(style: &str) -> Option<u8> {
    style.strip_prefix("Heading").and_then(|n| n.parse().ok()).filter(|n| (1..=9).contains(n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::StoryId;

    #[test]
    fn field_ranges_cover_each_outermost_field() {
        let mut session = typed("Page ");
        session.insert_field("PAGE", "1");
        session.type_text(" of ");
        session.insert_field("NUMPAGES", "3");
        let fields = field_ranges(&session.document, &StoryId::Body);
        assert_eq!(fields.len(), 2);
        let text = position::paragraphs(&session.document.body)[0].1.flat_text();
        assert!(
            fields[0].0.offset >= "Page ".len() && fields[0].1.offset < fields[1].0.offset,
            "{fields:?} in {text:?}"
        );
        session.selection = crate::position::Selection::caret(Pos::new(vec![0], 0));
        session.select_next_field(true).unwrap();
        assert_eq!(session.selection.ordered().0.offset, fields[0].0.offset);
    }

    fn typed(text: &str) -> Session {
        let mut session = Session::new(crate::new::blank());
        session.type_text(text);
        session
    }

    fn text_of(session: &Session, n: u32) -> String {
        position::paragraph(&session.document, &StoryId::Body, &[n])
            .map(|p| p.flat_text())
            .unwrap_or_default()
    }

    #[test]
    fn alt_x_turns_a_code_into_its_character_and_back() {
        let mut session = typed("Pi is 03C0");
        session.toggle_unicode().unwrap();
        assert_eq!(text_of(&session, 0), "Pi is \u{3C0}");
        session.toggle_unicode().unwrap();
        assert_eq!(text_of(&session, 0), "Pi is 03C0");
    }

    #[test]
    fn a_paragraph_moves_down_and_back_up() {
        let mut session = typed("one\ntwo\nthree");
        session.select(Pos::new(vec![0], 1), Pos::new(vec![0], 1));
        session.move_paragraph(false).unwrap();
        assert_eq!((text_of(&session, 0), text_of(&session, 1)), ("two".into(), "one".into()));
        assert_eq!(session.selection.head.path, vec![1], "the caret went with it");
        session.move_paragraph(true).unwrap();
        assert_eq!(text_of(&session, 0), "one");
        session.select(Pos::new(vec![0], 0), Pos::new(vec![0], 0));
        assert!(session.move_paragraph(true).is_err());
    }

    #[test]
    fn a_heading_is_promoted_and_demoted_and_body_text_takes_the_level_above() {
        let mut session = typed("Title\nbody");
        session.select(Pos::new(vec![0], 0), Pos::new(vec![0], 0));
        session.apply_style("Heading2").unwrap();
        session.promote_heading(true).unwrap();
        assert_eq!(session.paragraph_at_caret().style, "Heading1");
        assert!(session.promote_heading(true).is_err());
        session.promote_heading(false).unwrap();
        assert_eq!(session.paragraph_at_caret().style, "Heading2");
        session.select(Pos::new(vec![1], 0), Pos::new(vec![1], 0));
        session.promote_heading(true).unwrap();
        assert_eq!(session.paragraph_at_caret().style, "Heading2");
    }

    #[test]
    fn a_section_moves_with_its_heading() {
        let mut session = typed(
            "One
under one
Two
under two",
        );
        for (n, style) in [(0, "Heading1"), (2, "Heading1")] {
            session.select(Pos::new(vec![n], 0), Pos::new(vec![n], 0));
            session.apply_style(style).unwrap();
        }
        session.select(Pos::new(vec![2], 0), Pos::new(vec![2], 0));
        session.move_section(true).unwrap();
        let texts: Vec<String> = (0..4).map(|n| text_of(&session, n)).collect();
        assert_eq!(texts, ["Two", "under two", "One", "under one"]);
        assert_eq!(session.selection.head.path, vec![0]);
        session.move_section(false).unwrap();
        assert_eq!(text_of(&session, 0), "One");
    }

    #[test]
    fn hanging_indent_and_reset_paragraph() {
        let mut session = typed("A long paragraph");
        session.hanging_indent(true);
        let look = session.paragraph_at_caret();
        assert_eq!((look.indent_left, look.indent_first), (36.0, -36.0));
        session.reset_paragraph();
        let look = session.paragraph_at_caret();
        assert_eq!((look.indent_left, look.indent_first), (0.0, 0.0));
        session.toggle_space_before();
        assert_eq!(session.paragraph_at_caret().space_before, 12.0);
        session.toggle_space_before();
        assert_eq!(session.paragraph_at_caret().space_before, 0.0);
    }
}
