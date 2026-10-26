//! List operations beyond starting and leaving a list (task-2359, TDD 5.4 C16, C17, M25): Continue
//! Numbering, Set Numbering Value, Define New Bullet, Define New Number Format, Define New Multilevel
//! List and Adjust List Indents. Each changes `numbering.xml` and is one undo step.
//!
//! A list paragraph names a `w:num`, which names the `w:abstractNum` that holds the nine levels. A new
//! definition is a new abstract list and a new `w:num` for it, so other lists that shared the old
//! definition keep their look, which is what Word does when Define New Bullet is used on one list.

use crate::model::*;
use crate::position;
use crate::session::{Scope, Session};
use crate::xml::{El, Node};

/// One level of a list being defined: its number format (`decimal`, `lowerLetter`, `bullet`...), its
/// text with `%1`..`%9` for the levels' numbers, where it starts, and its indents in points.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelDefinition {
    pub format: String,
    pub text: String,
    pub start: i64,
    pub indent: f32,
    pub hanging: f32,
    /// For a bullet: the font its character is in (`Symbol`, `Wingdings`).
    pub font: Option<String>,
}

impl Session {
    /// The list the caret's paragraph is in: its `w:numId` and level.
    fn caret_list(&self) -> Option<(i64, u8)> {
        let head = &self.selection.head;
        let paragraph = position::paragraph(&self.document, &head.story, &head.path)?;
        crate::styles::Resolver::new(&self.document).numbering_of(&paragraph.props)
    }

    /// Continue Numbering: the caret's list joins the list of the same kind before it, so its numbers
    /// carry on from that list's.
    pub fn continue_numbering(&mut self) -> Result<(), String> {
        let (num, _) = self.caret_list().ok_or("the caret is not in a list")?;
        let story = self.selection.head.story.clone();
        let here = self.selection.head.path.clone();
        let resolver = crate::styles::Resolver::new(&self.document);
        let kind_of = |n: i64| {
            resolver.numbering.level(n, 0).and_then(|l| l.val_of("w:numFmt")).map(|f| f == "bullet")
        };
        let mine = kind_of(num);
        let blocks = position::story(&self.document, &story).ok_or("no story")?;
        let previous = position::paragraphs(blocks)
            .into_iter()
            .take_while(|(path, _)| *path != here)
            .filter_map(|(_, p)| resolver.numbering_of(&p.props).map(|(n, _)| n))
            .filter(|n| *n != num && kind_of(*n) == mine)
            .last()
            .ok_or("there is no list of this kind before this one to continue")?;
        self.change(Scope::Story, move |document, selection, _| {
            let story = selection.head.story.clone();
            let paths: Vec<Vec<u32>> = position::story(document, &story)
                .map(|b| position::paragraphs(b).into_iter().map(|(p, _)| p).collect())
                .unwrap_or_default();
            for path in paths {
                let Some(p) = position::paragraph_mut(document, &story, &path) else { continue };
                let Some(num_pr) = p.props.child_mut("w:numPr") else { continue };
                if num_pr.val_of("w:numId").and_then(|v| v.parse::<i64>().ok()) == Some(num) {
                    num_pr.put(El::new("w:numId").with("w:val", previous.to_string()));
                }
            }
            selection.clone()
        });
        Ok(())
    }

    /// Set Numbering Value: the caret's paragraph is numbered `value`, and those after it follow on.
    pub fn set_numbering_value(&mut self, value: i64) -> Result<(), String> {
        self.caret_list().ok_or("the caret is not in a list")?;
        self.restart_list(value);
        Ok(())
    }

    /// Define New Bullet: the caret's list level shows `symbol` in `font` (no font for an ordinary
    /// character).
    pub fn define_bullet(&mut self, symbol: char, font: Option<&str>) -> Result<(), String> {
        let font = font.map(str::to_owned);
        self.change_caret_level(move |level| {
            level.put(El::new("w:numFmt").with("w:val", "bullet"));
            level.put(El::new("w:lvlText").with("w:val", symbol.to_string()));
            let mut rpr = level.child("w:rPr").cloned().unwrap_or_else(|| El::new("w:rPr"));
            match &font {
                Some(font) => rpr.put(
                    El::new("w:rFonts")
                        .with("w:ascii", font.clone())
                        .with("w:hAnsi", font.clone())
                        .with("w:hint", "default"),
                ),
                None => rpr.remove_children("w:rFonts"),
            }
            level.put(rpr);
        })
    }

    /// Define New Number Format: the caret's list level is numbered in `format`, with `before` and
    /// `after` around the number (`(`, `)`; `Step `, `.`).
    pub fn define_number_format(
        &mut self,
        format: &str,
        before: &str,
        after: &str,
    ) -> Result<(), String> {
        let (_, level) = self.caret_list().ok_or("the caret is not in a list")?;
        let text = format!("{before}%{}{after}", level + 1);
        let format = format.to_owned();
        self.change_caret_level(move |lvl| {
            lvl.put(El::new("w:numFmt").with("w:val", format.clone()));
            lvl.put(El::new("w:lvlText").with("w:val", text.clone()));
            if let Some(rpr) = lvl.child_mut("w:rPr") {
                rpr.remove_children("w:rFonts");
            }
        })
    }

    /// Adjust List Indents: where the caret's list level's text starts, and how far its number hangs
    /// to the left of it, in points.
    pub fn set_list_indents(&mut self, indent: f32, hanging: f32) -> Result<(), String> {
        let twips = |p: f32| ((p * 20.0).round() as i64).to_string();
        self.change_caret_level(move |lvl| {
            let mut ppr = lvl.child("w:pPr").cloned().unwrap_or_else(|| El::new("w:pPr"));
            ppr.put(
                El::new("w:ind").with("w:left", twips(indent)).with("w:hanging", twips(hanging)),
            );
            lvl.put(ppr);
        })
    }

    /// Define New Multilevel List: a new list with these levels (up to nine), applied to the selected
    /// paragraphs at their current levels, or at the first level when they are not in a list.
    pub fn define_multilevel(&mut self, levels: &[LevelDefinition]) -> Result<(), String> {
        if levels.is_empty() || levels.len() > 9 {
            return Err("a multilevel list has one to nine levels".into());
        }
        let levels = levels.to_vec();
        self.change(Scope::Whole, move |document, selection, _| {
            let mut abstract_el = El::new("w:abstractNum");
            abstract_el
                .children
                .push(Node::El(El::new("w:multiLevelType").with("w:val", "multilevel")));
            for (i, def) in levels.iter().enumerate() {
                abstract_el.children.push(Node::El(level_element(i as u8, def)));
            }
            let num = add_list(document, abstract_el);
            let (from, to) = selection.ordered();
            crate::edit::format_paragraphs(document, from, to, &|ppr| {
                let level = ppr
                    .child("w:numPr")
                    .and_then(|n| n.val_of("w:ilvl"))
                    .and_then(|v| v.parse::<u8>().ok())
                    .unwrap_or(0);
                ppr.put(
                    El::new("w:numPr")
                        .with_child(El::new("w:ilvl").with("w:val", level.to_string()))
                        .with_child(El::new("w:numId").with("w:val", num.to_string())),
                );
                crate::order::sort(ppr);
            });
            selection.clone()
        });
        Ok(())
    }

    /// Change the caret's list level: its abstract list is copied to a new one first, so other lists
    /// that used the same definition are not changed, and the caret's whole list moves to the copy.
    fn change_caret_level(&mut self, change: impl Fn(&mut El) + 'static) -> Result<(), String> {
        let (num, level) = self.caret_list().ok_or("the caret is not in a list")?;
        self.change(Scope::Whole, move |document, selection, _| {
            let Some(part) = document.numbering.as_ref() else { return selection.clone() };
            let numbering = crate::numbering::Numbering::new(Some(&part.root));
            let Some((_, abstract_el)) = numbering.abstract_of(num) else {
                return selection.clone();
            };
            let mut copy = abstract_el.clone();
            for node in &mut copy.children {
                if let Node::El(lvl) = node {
                    if lvl.name == "w:lvl" && lvl.attr("w:ilvl") == Some(&level.to_string()) {
                        change(lvl);
                        crate::order::sort(lvl);
                    }
                }
            }
            copy.remove_children("w:nsid");
            let new_num = add_list(document, copy);
            let story = selection.head.story.clone();
            let paths: Vec<Vec<u32>> = position::story(document, &story)
                .map(|b| position::paragraphs(b).into_iter().map(|(p, _)| p).collect())
                .unwrap_or_default();
            for path in paths {
                let Some(p) = position::paragraph_mut(document, &story, &path) else { continue };
                let Some(num_pr) = p.props.child_mut("w:numPr") else { continue };
                if num_pr.val_of("w:numId").and_then(|v| v.parse::<i64>().ok()) == Some(num) {
                    num_pr.put(El::new("w:numId").with("w:val", new_num.to_string()));
                }
            }
            selection.clone()
        });
        Ok(())
    }
}

/// A `w:lvl` from a definition.
fn level_element(index: u8, def: &LevelDefinition) -> El {
    let twips = |p: f32| ((p * 20.0).round() as i64).to_string();
    let mut lvl = El::new("w:lvl").with("w:ilvl", index.to_string());
    lvl.children.push(Node::El(El::new("w:start").with("w:val", def.start.to_string())));
    lvl.children.push(Node::El(El::new("w:numFmt").with("w:val", def.format.clone())));
    lvl.children.push(Node::El(El::new("w:lvlText").with("w:val", def.text.clone())));
    lvl.children.push(Node::El(El::new("w:lvlJc").with("w:val", "left")));
    lvl.children.push(Node::El(El::new("w:pPr").with_child(
        El::new("w:ind").with("w:left", twips(def.indent)).with("w:hanging", twips(def.hanging)),
    )));
    if let Some(font) = &def.font {
        lvl.children.push(Node::El(
            El::new("w:rPr").with_child(
                El::new("w:rFonts")
                    .with("w:ascii", font.clone())
                    .with("w:hAnsi", font.clone())
                    .with("w:hint", "default"),
            ),
        ));
    }
    lvl
}

/// Put an abstract list in the numbering part with a new id and a `w:num` for it. Answers the
/// `w:numId`.
fn add_list(document: &mut Document, mut abstract_el: El) -> i64 {
    if document.numbering.is_none() {
        let _ = crate::builtin::new_list(document, crate::builtin::ListKind::Bullet);
    }
    let part = document.numbering.as_mut().expect("made above");
    let next_abstract = part
        .root
        .children_named("w:abstractNum")
        .filter_map(|a| a.attr("w:abstractNumId").and_then(|v| v.parse::<i64>().ok()))
        .max()
        .map_or(0, |n| n + 1);
    let next_num = part
        .root
        .children_named("w:num")
        .filter_map(|n| n.attr("w:numId").and_then(|v| v.parse::<i64>().ok()))
        .max()
        .map_or(1, |n| n + 1);
    abstract_el.set_attr("w:abstractNumId", next_abstract.to_string());
    let first_num =
        part.root.children.iter().position(|n| matches!(n, Node::El(el) if el.name == "w:num"));
    match first_num {
        Some(at) => part.root.children.insert(at, Node::El(abstract_el)),
        None => part.root.children.push(Node::El(abstract_el)),
    }
    part.root.children.push(Node::El(
        El::new("w:num")
            .with("w:numId", next_num.to_string())
            .with_child(El::new("w:abstractNumId").with("w:val", next_abstract.to_string())),
    ));
    part.dirty = true;
    next_num
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::ListKind;
    use crate::layout::{lay_out, Item, Options};
    use crate::position::{Pos, Selection};
    use unluminous_core::FixedMetrics;

    fn labels(session: &Session) -> Vec<String> {
        let pages = lay_out(&session.document, &FixedMetrics::default(), Options::default());
        let mut out = Vec::new();
        for item in &pages.pages[0].items {
            if let Item::Text(t) = item {
                out.push(t.text.clone());
            }
        }
        out
    }

    fn numbered(items: &[&str]) -> Session {
        let mut session = Session::new(crate::new::blank());
        for (i, text) in items.iter().enumerate() {
            if i > 0 {
                session.enter();
            }
            session.type_text(text);
        }
        session.select_all();
        session.toggle_list(ListKind::Number);
        session
    }

    #[test]
    fn a_number_format_and_a_start_value_change_the_labels() {
        let mut session = numbered(&["one", "two", "three"]);
        session.selection = Selection::caret(Pos::new(vec![0], 0));
        session.define_number_format("upperRoman", "(", ")").unwrap();
        let text = labels(&session).join(" ");
        assert!(text.contains("(I)") && text.contains("(III)"), "{text}");
        session.selection = Selection::caret(Pos::new(vec![1], 0));
        session.set_numbering_value(7).unwrap();
        let text = labels(&session).join(" ");
        assert!(text.contains("(VII)") && text.contains("(VIII)"), "{text}");
    }

    #[test]
    fn a_new_bullet_and_a_multilevel_list_are_defined() {
        let mut session = Session::new(crate::new::blank());
        session.type_text("item");
        session.toggle_list(ListKind::Bullet);
        session.define_bullet('➤', None).unwrap();
        assert!(labels(&session).iter().any(|t| t.contains('➤')));
        session.select_all();
        let level = |format: &str, text: &str| LevelDefinition {
            format: format.into(),
            text: text.into(),
            start: 1,
            indent: 36.0,
            hanging: 18.0,
            font: None,
        };
        session
            .define_multilevel(&[level("decimal", "%1."), level("lowerLetter", "%1.%2")])
            .unwrap();
        assert!(labels(&session).iter().any(|t| t.contains("1.")));
        session.set_list_indents(54.0, 27.0).unwrap();
        assert_eq!(session.paragraph_at_caret().indent_left, 54.0);
    }

    #[test]
    fn continue_numbering_joins_the_list_before() {
        let mut session = numbered(&["a", "b"]);
        let last = position::paragraphs(&session.document.body).last().unwrap().0.clone();
        session.selection = Selection::caret(Pos::new(last, 1));
        session.enter();
        session.enter();
        session.type_text("c");
        session.toggle_list(ListKind::Number);
        // Starting at 1 again makes it a list of its own.
        session.set_numbering_value(1).unwrap();
        let before = labels(&session).join(" ");
        assert!(before.contains("1. c") || !before.contains("3."), "a new list: {before}");
        session.continue_numbering().unwrap();
        let after = labels(&session).join(" ");
        assert!(after.contains("3."), "it carries on: {after}");
    }
}
