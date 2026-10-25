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

impl Session {
    /// Ctrl+T and Ctrl+Shift+T: indent every line but the first one more step, or one less.
    pub fn hanging_indent(&mut self, more: bool) {
        self.format_paragraphs(move |ppr| {
            let ind = ppr.child_or_insert("w:ind");
            let read = |el: &El, name: &str| el.attr(name).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
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
            ppr.children.retain(|n| matches!(n, crate::xml::Node::El(e) if keep.contains(&e.name.as_str())));
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
        self.apply_style(&next)
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

    fn move_inside(&mut self, at: Pos, up: bool) -> Result<(), String> {
        let index = *at.path.last().expect("checked") as usize;
        let check = {
            let mut copy = self.document.clone();
            let blocks = position::story_mut(&mut copy, &at.story).ok_or("the story has gone")?;
            let (container, _) = position::container_mut(blocks, &at.path).ok_or("the caret is not in a paragraph")?;
            let other = if up { index.checked_sub(1) } else { Some(index + 1) };
            other.filter(|o| *o < container.len()).is_some()
        };
        if !check {
            return Err(if up { "this is the first paragraph here" } else { "this is the last paragraph here" }.into());
        }
        self.change(Scope::Story, move |document, selection, _| {
            let Some(blocks) = position::story_mut(document, &at.story) else { return selection.clone() };
            let Some((container, _)) = position::container_mut(blocks, &at.path) else { return selection.clone() };
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

    /// F11 and Shift+F11: select the next field after the caret, or the one before it, in the story.
    pub fn select_next_field(&mut self, forward: bool) -> Result<(), String> {
        let head = self.selection.ordered().1.clone();
        let tail = self.selection.ordered().0.clone();
        let blocks = position::story(&self.document, &head.story).ok_or("the story has gone")?;
        let mut fields: Vec<(Pos, Pos)> = Vec::new();
        for (path, paragraph) in position::paragraphs(blocks) {
            let mut offset = 0;
            let mut open: Vec<usize> = Vec::new();
            for run in &paragraph.runs {
                match run.content.character() {
                    Some(FIELD_BEGIN) => open.push(offset),
                    Some(FIELD_END) => {
                        if let Some(start) = open.pop() {
                            if open.is_empty() {
                                let pos = Pos { story: head.story.clone(), path: path.clone(), offset: start };
                                fields.push((pos.clone(), pos.at(offset + run.len())));
                            }
                        }
                    }
                    _ => {}
                }
                offset += run.len();
            }
        }
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
        let paragraph = position::paragraph(&self.document, &to.story, &to.path).ok_or("the caret is not in text")?;
        let text = paragraph.flat_text();
        let end = to.offset.min(text.len());
        let start = if from.path == to.path && from.offset < to.offset {
            from.offset
        } else {
            // Up to six hexadecimal digits before the caret, after an optional U+.
            let digits = text[..end].chars().rev().take_while(|c| c.is_ascii_hexdigit()).take(6).count();
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
            Some(c) if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) => c.to_string(),
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
            let run = Run { props: Arc::clone(&props), content: Content::Object(el), wraps, attrs: Arc::default() };
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

    fn typed(text: &str) -> Session {
        let mut session = Session::new(crate::new::blank());
        session.type_text(text);
        session
    }

    fn text_of(session: &Session, n: u32) -> String {
        position::paragraph(&session.document, &StoryId::Body, &[n]).map(|p| p.flat_text()).unwrap_or_default()
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
