//! Changing the styles themselves (task-2359, TDD 5.4 C23 to C25, M24): Modify Style, New Style from the
//! selection, Update to Match Selection, Delete, Revert to Word's own definition, and the linked style
//! Word applies when a paragraph style is chosen for a few words inside a paragraph.
//!
//! Every change writes `styles.xml` through its part, marked dirty, as one undo step.

use crate::edit;
use crate::model::*;
use crate::position;
use crate::props;
use crate::session::{Scope, Session};
use crate::xml::{El, Node};

impl Session {
    /// Change one property of a style: `font <name>`, `size <pt>`, `bold|italic|underline on|off`,
    /// `color <RRGGBB|auto>`, `align left|center|right|justify`, `before|after <pt>`, `line <multiple>`,
    /// `indent-left|first-line|hanging <pt>`, `keep-next on|off`, `based-on <id>`, `next <id>`, `name
    /// <text>`, or `theme-font major|minor`.
    pub fn modify_style(&mut self, id: &str, property: &str, value: &str) -> Result<(), String> {
        crate::builtin::ensure_style(&mut self.document, id)?;
        let (id, property, value) = (id.to_owned(), property.to_owned(), value.to_owned());
        let points = || {
            value
                .trim_end_matches("pt")
                .parse::<f32>()
                .map_err(|_| format!("{value} is not a number of points"))
        };
        let twips = |p: f32| ((p * 20.0).round() as i64).to_string();
        let on = !matches!(value.as_str(), "off" | "false" | "0" | "no");
        // Work out the change before the closure, so a bad value refuses without an undo step.
        enum Edit {
            Run(El),
            RunOff(&'static str),
            Para(El),
            Attr(&'static str, String),
            Name(String),
        }
        let change = match property.as_str() {
            "font" => Edit::Run(
                El::new("w:rFonts")
                    .with("w:ascii", value.clone())
                    .with("w:hAnsi", value.clone())
                    .with("w:cs", value.clone()),
            ),
            "theme-font" => {
                let theme = if value == "major" { "majorHAnsi" } else { "minorHAnsi" };
                Edit::Run(
                    El::new("w:rFonts").with("w:asciiTheme", theme).with("w:hAnsiTheme", theme),
                )
            }
            "size" => Edit::Run(
                El::new("w:sz").with("w:val", ((points()? * 2.0).round() as i64).to_string()),
            ),
            "bold" if on => Edit::Run(El::new("w:b")),
            "bold" => Edit::Run(El::new("w:b").with("w:val", "0")),
            "italic" if on => Edit::Run(El::new("w:i")),
            "italic" => Edit::Run(El::new("w:i").with("w:val", "0")),
            "underline" => {
                Edit::Run(El::new("w:u").with("w:val", if on { "single" } else { "none" }))
            }
            "color" | "colour" if value == "auto" => Edit::RunOff("w:color"),
            "color" | "colour" => {
                let rgb =
                    props::Rgb::parse(&value).ok_or_else(|| format!("{value} is not a colour"))?;
                Edit::Run(El::new("w:color").with("w:val", rgb.hex()))
            }
            "align" => Edit::Para(El::new("w:jc").with(
                "w:val",
                match value.as_str() {
                    "center" | "centre" => "center",
                    "right" => "right",
                    "justify" | "both" => "both",
                    _ => "left",
                },
            )),
            "before" => Edit::Para(El::new("w:spacing").with("w:before", twips(points()?))),
            "after" => Edit::Para(El::new("w:spacing").with("w:after", twips(points()?))),
            "line" => {
                let multiple: f32 =
                    value.parse().map_err(|_| format!("{value} is not a line spacing"))?;
                Edit::Para(
                    El::new("w:spacing")
                        .with("w:line", ((multiple * 240.0).round() as i64).to_string())
                        .with("w:lineRule", "auto"),
                )
            }
            "indent-left" => Edit::Para(El::new("w:ind").with("w:left", twips(points()?))),
            "first-line" => Edit::Para(El::new("w:ind").with("w:firstLine", twips(points()?))),
            "hanging" => Edit::Para(El::new("w:ind").with("w:hanging", twips(points()?))),
            "keep-next" if on => Edit::Para(El::new("w:keepNext")),
            "keep-next" => Edit::Para(El::new("w:keepNext").with("w:val", "0")),
            "based-on" => Edit::Attr("w:basedOn", value.clone()),
            "next" => Edit::Attr("w:next", value.clone()),
            "name" => Edit::Name(value.clone()),
            other => return Err(format!("{other} is not a style property this can change")),
        };
        self.change(Scope::Whole, move |document, selection, _| {
            let Some(part) = document.styles.as_mut() else { return selection.clone() };
            let Some(style) = style_mut(&mut part.root, &id) else { return selection.clone() };
            match change {
                Edit::Run(el) => merge_into(style, "w:rPr", el),
                Edit::RunOff(name) => {
                    if let Some(rpr) = style.child_mut("w:rPr") {
                        rpr.remove_children(name);
                    }
                }
                Edit::Para(el) => merge_into(style, "w:pPr", el),
                Edit::Attr(name, value) => {
                    style.put(El::new(name).with("w:val", value));
                }
                Edit::Name(name) => {
                    style.put(El::new("w:name").with("w:val", name));
                }
            }
            crate::order::sort(style);
            part.dirty = true;
            selection.clone()
        });
        Ok(())
    }

    /// A new paragraph style named `name`, from the caret's paragraph: its style is the new one's base,
    /// and the formatting on the paragraph and its first run go into the new style. Answers the new id.
    pub fn new_style_from_selection(&mut self, name: &str) -> Result<String, String> {
        let id: String = name.chars().filter(|c| c.is_alphanumeric()).collect();
        if id.is_empty() {
            return Err("a style needs a name with a letter or a digit in it".into());
        }
        if crate::styles::Resolver::new(&self.document).style(&id).is_some() {
            return Err(format!("there is already a style called {name}"));
        }
        let head = self.selection.head.clone();
        let paragraph =
            position::paragraph(&self.document, &head.story, &head.path).ok_or("no paragraph")?;
        let base = paragraph.props.val_of("w:pStyle").unwrap_or("Normal").to_owned();
        let direct_ppr = without(&paragraph.props, &["w:pStyle", "w:numPr", "w:sectPr", "w:rPr"]);
        let direct_rpr = paragraph
            .runs
            .iter()
            .find(|r| matches!(r.content, Content::Text(_)))
            .map(|r| without(&r.props, &["w:rStyle"]))
            .unwrap_or_else(|| El::new("w:rPr"));
        let (name, new_id) = (name.to_owned(), id.clone());
        self.change(Scope::Whole, move |document, selection, _| {
            if document.styles.is_none() {
                let _ = crate::builtin::ensure_style(document, "Normal");
            }
            let Some(part) = document.styles.as_mut() else { return selection.clone() };
            let mut style = El::new("w:style")
                .with("w:type", "paragraph")
                .with("w:customStyle", "1")
                .with("w:styleId", new_id.clone());
            style.children.push(Node::El(El::new("w:name").with("w:val", name.clone())));
            style.children.push(Node::El(El::new("w:basedOn").with("w:val", base.clone())));
            style.children.push(Node::El(El::new("w:qFormat")));
            if direct_ppr.elements().next().is_some() {
                style.children.push(Node::El(direct_ppr.clone()));
            }
            if direct_rpr.elements().next().is_some() {
                style.children.push(Node::El(direct_rpr.clone()));
            }
            crate::order::sort(&mut style);
            part.root.children.push(Node::El(style));
            part.dirty = true;
            // The paragraph takes the new style, and the formatting now in the style comes off it.
            if let Some(p) =
                position::paragraph_mut(document, &selection.head.story, &selection.head.path)
            {
                p.props =
                    El::new("w:pPr").with_child(El::new("w:pStyle").with("w:val", new_id.clone()));
                for run in &mut p.runs {
                    let mut rpr = (*run.props).clone();
                    for child in direct_rpr.elements() {
                        if rpr.child(&child.name) == Some(child) {
                            rpr.remove_children(&child.name);
                        }
                    }
                    run.props = std::sync::Arc::new(rpr);
                }
            }
            selection.clone()
        });
        Ok(id)
    }

    /// Update to Match Selection: the style of the caret's paragraph takes the formatting the paragraph
    /// and its first run have on them, which then comes off every paragraph the selection covers.
    pub fn update_style_to_match(&mut self) -> Result<String, String> {
        let head = self.selection.head.clone();
        let paragraph =
            position::paragraph(&self.document, &head.story, &head.path).ok_or("no paragraph")?;
        let id = paragraph.props.val_of("w:pStyle").unwrap_or("Normal").to_owned();
        crate::builtin::ensure_style(&mut self.document, &id)?;
        let paragraph =
            position::paragraph(&self.document, &head.story, &head.path).ok_or("no paragraph")?;
        let direct_ppr = without(&paragraph.props, &["w:pStyle", "w:numPr", "w:sectPr", "w:rPr"]);
        let direct_rpr = paragraph
            .runs
            .iter()
            .find(|r| matches!(r.content, Content::Text(_)))
            .map(|r| without(&r.props, &["w:rStyle"]))
            .unwrap_or_else(|| El::new("w:rPr"));
        let style_id = id.clone();
        self.change(Scope::Whole, move |document, selection, _| {
            if let Some(part) = document.styles.as_mut() {
                if let Some(style) = style_mut(&mut part.root, &style_id) {
                    for child in direct_ppr.elements() {
                        merge_into(style, "w:pPr", child.clone());
                    }
                    for child in direct_rpr.elements() {
                        merge_into(style, "w:rPr", child.clone());
                    }
                    crate::order::sort(style);
                    part.dirty = true;
                }
            }
            let (from, to) = selection.ordered();
            edit::format_paragraphs(document, from, to, &|ppr| {
                for child in direct_ppr.elements() {
                    ppr.remove_children(&child.name);
                }
            });
            for (path, _, _) in edit::covered(document, from, to) {
                if let Some(p) = position::paragraph_mut(document, &from.story, &path) {
                    for run in &mut p.runs {
                        let mut rpr = (*run.props).clone();
                        for child in direct_rpr.elements() {
                            if rpr.child(&child.name) == Some(child) {
                                rpr.remove_children(&child.name);
                            }
                        }
                        run.props = std::sync::Arc::new(rpr);
                    }
                }
            }
            selection.clone()
        });
        Ok(id)
    }

    /// Delete a style. Paragraphs and runs that used it take its base style, as Word's Delete does.
    /// Word's built in styles cannot be deleted, only changed back with [`Self::revert_style`].
    pub fn delete_style(&mut self, id: &str) -> Result<(), String> {
        let resolver = crate::styles::Resolver::new(&self.document);
        let style = resolver.style(id).ok_or_else(|| format!("there is no style {id}"))?;
        if style.attr("w:customStyle") != Some("1") {
            return Err(format!(
                "{} is one of Word's built in styles, which can be changed back but not deleted",
                resolver.name_of(id)
            ));
        }
        let base = style.val_of("w:basedOn").map(str::to_owned);
        let kind = style.attr("w:type").unwrap_or("paragraph").to_owned();
        let id = id.to_owned();
        self.change(Scope::Whole, move |document, selection, _| {
            if let Some(part) = document.styles.as_mut() {
                part.root.children.retain(|n| !matches!(n, Node::El(el) if el.name == "w:style" && el.attr("w:styleId") == Some(id.as_str())));
                // A style based on the deleted one is based on its base now.
                for node in &mut part.root.children {
                    if let Node::El(el) = node {
                        if el.name == "w:style" && el.val_of("w:basedOn") == Some(id.as_str()) {
                            match &base {
                                Some(b) => el.put(El::new("w:basedOn").with("w:val", b.clone())),
                                None => el.remove_children("w:basedOn"),
                            }
                        }
                    }
                }
                part.dirty = true;
            }
            for story in crate::search::every_story(document) {
                let Some(blocks) = position::story_mut(document, &story) else { continue };
                let paths: Vec<Vec<u32>> = position::paragraphs(blocks).into_iter().map(|(p, _)| p).collect();
                for path in paths {
                    let Some(p) = position::paragraph_mut(document, &story, &path) else { continue };
                    if kind == "paragraph" && p.props.val_of("w:pStyle") == Some(id.as_str()) {
                        match base.as_deref() {
                            Some(b) if b != "Normal" => props::set_val(&mut p.props, "w:pStyle", Some(b)),
                            _ => p.props.remove_children("w:pStyle"),
                        }
                    }
                    if kind == "character" {
                        for run in &mut p.runs {
                            if run.props.val_of("w:rStyle") == Some(id.as_str()) {
                                let mut rpr = (*run.props).clone();
                                rpr.remove_children("w:rStyle");
                                run.props = std::sync::Arc::new(rpr);
                            }
                        }
                    }
                }
            }
            selection.clone()
        });
        Ok(())
    }

    /// Put one of Word's built in styles back as Word defines it.
    pub fn revert_style(&mut self, id: &str) -> Result<(), String> {
        let original = crate::builtin::definition(id)
            .ok_or_else(|| format!("{id} is not one of Word's built in styles this knows"))?;
        let id = id.to_owned();
        self.change(Scope::Whole, move |document, selection, _| {
            if let Some(part) = document.styles.as_mut() {
                match style_mut(&mut part.root, &id) {
                    Some(style) => *style = original.clone(),
                    None => part.root.children.push(Node::El(original.clone())),
                }
                part.dirty = true;
            }
            selection.clone()
        });
        Ok(())
    }

    /// Apply a paragraph style, or its linked character style (`w:link`) when the selection is a few
    /// words inside one paragraph, which is what Word does with a linked style such as Heading 1.
    pub fn apply_style_where_it_fits(&mut self, id: &str) -> Result<(), String> {
        crate::builtin::ensure_style(&mut self.document, id)?;
        let (from, to) = {
            let (a, b) = self.selection.ordered();
            (a.clone(), b.clone())
        };
        let inside_one = !self.selection.is_empty()
            && from.path == to.path
            && position::paragraph(&self.document, &from.story, &from.path)
                .is_some_and(|p| from.offset > 0 || to.offset < p.len());
        let linked = crate::styles::Resolver::new(&self.document)
            .style(id)
            .filter(|s| s.attr("w:type") == Some("paragraph"))
            .and_then(|s| s.val_of("w:link"))
            .map(str::to_owned);
        match linked {
            Some(character) if inside_one => self.apply_style(&character),
            _ => self.apply_style(id),
        }
    }
}

fn style_mut<'a>(root: &'a mut El, id: &str) -> Option<&'a mut El> {
    root.children.iter_mut().find_map(|n| match n {
        Node::El(el) if el.name == "w:style" && el.attr("w:styleId") == Some(id) => Some(el),
        _ => None,
    })
}

/// Put `el` into the style's `w:pPr` or `w:rPr`, merging its attributes into an element of the same
/// name that is already there (so `w:spacing w:after` keeps `w:before`).
fn merge_into(style: &mut El, part: &str, el: El) {
    if style.child(part).is_none() {
        style.children.push(Node::El(El::new(part)));
    }
    let Some(properties) = style.child_mut(part) else { return };
    match properties.child_mut(&el.name) {
        Some(existing) if !el.attrs.is_empty() && el.name != "w:rFonts" => {
            for (key, value) in el.attrs {
                existing.set_attr(&key, value);
            }
        }
        _ => properties.put(el),
    }
    crate::order::sort(properties);
}

/// A copy of properties without the children named.
fn without(properties: &El, names: &[&str]) -> El {
    let mut copy = properties.clone();
    for name in names {
        copy.remove_children(name);
    }
    copy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::{Pos, Selection, StoryId};

    fn look(session: &Session) -> props::RunLook {
        session.look_at_caret()
    }

    #[test]
    fn modifying_a_style_changes_every_paragraph_in_it() {
        let mut session = Session::new(crate::new::blank());
        session.type_text("Title text");
        session.apply_style("Heading1").unwrap();
        session.modify_style("Heading1", "size", "30").unwrap();
        session.modify_style("Heading1", "color", "FF0000").unwrap();
        session.modify_style("Heading1", "after", "20").unwrap();
        session.selection = Selection::caret(Pos::new(vec![0], 3));
        assert_eq!(look(&session).size, 30.0);
        assert_eq!(look(&session).color.map(|c| c.hex()), Some("FF0000".to_owned()));
        assert_eq!(session.paragraph_at_caret().space_after, 20.0);
        assert!(session.modify_style("Heading1", "size", "big").is_err());
        assert!(session.undo());
        assert!((session.paragraph_at_caret().space_after - 20.0).abs() > 0.01);
    }

    #[test]
    fn a_new_style_takes_the_formatting_and_can_be_deleted() {
        let mut session = Session::new(crate::new::blank());
        session.toggle("w:b");
        session.type_text("Bold words");
        let id = session.new_style_from_selection("My Emphasis").unwrap();
        assert_eq!(id, "MyEmphasis");
        let p = position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap();
        assert_eq!(p.props.val_of("w:pStyle"), Some("MyEmphasis"));
        session.selection = Selection::caret(Pos::new(vec![0], 3));
        assert!(look(&session).bold, "the bold is in the style now");
        session.delete_style("MyEmphasis").unwrap();
        let p = position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap();
        assert!(p.props.val_of("w:pStyle").is_none());
        assert!(session.delete_style("Heading1").is_err());
    }

    #[test]
    fn a_linked_style_applies_to_words_inside_a_paragraph() {
        let mut session = Session::new(crate::new::blank());
        session.type_text("Some words here");
        session.select(Pos::new(vec![0], 5), Pos::new(vec![0], 10));
        session.apply_style_where_it_fits("Heading1").unwrap();
        let p = position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap();
        assert!(p.props.val_of("w:pStyle").is_none(), "the paragraph keeps its style");
        assert!(p.runs.iter().any(|r| r.props.val_of("w:rStyle") == Some("Heading1Char")));
    }

    #[test]
    fn update_to_match_moves_the_formatting_into_the_style() {
        let mut session = Session::new(crate::new::blank());
        session.type_text("Quoted");
        session.apply_style("Quote").unwrap();
        session.select_all();
        session.toggle("w:b");
        session.update_style_to_match().unwrap();
        let p = position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap();
        assert!(p.runs.iter().all(|r| r.props.child("w:b").is_none()));
        session.selection = Selection::caret(Pos::new(vec![0], 2));
        assert!(look(&session).bold);
    }
}
