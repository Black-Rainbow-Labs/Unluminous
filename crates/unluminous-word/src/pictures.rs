//! Pictures and shapes as objects a person changes (task-2359, TDD 5.3): the object the selection is on,
//! its size, how text wraps round it, where a floating one sits, its crop, its alt text and its order in
//! the stack, and inserting a text box or one of the ten preset shapes.
//!
//! An object is one character in a paragraph's flat text, a [`Content::Object`] run holding a
//! `w:drawing`. The selection is on an object when it covers exactly that character, or when the caret
//! is just before one. A drawing inside `mc:AlternateContent` is changed in its DrawingML `mc:Choice` and
//! the VML fallback is dropped, because Word reads the Choice and a fallback left behind would describe
//! the old picture.
//!
//! Every change is one [`Session`] change, so one undo step.

use std::sync::Arc;

use crate::model::*;
use crate::position::{self, Pos, Selection, StoryId};
use crate::session::{Scope, Session};
use crate::xml::{El, Node};

/// The wrap kinds Word offers, by the name the agent and the panel use.
pub const WRAPS: [&str; 7] =
    ["inline", "square", "tight", "through", "top-and-bottom", "behind", "front"];

/// The preset shapes Insert offers (`a:prstGeom`).
pub const SHAPES: [&str; 11] = [
    "rect",
    "roundRect",
    "ellipse",
    "line",
    "straightConnector1",
    "rightArrow",
    "leftArrow",
    "upArrow",
    "downArrow",
    "triangle",
    "diamond",
];

/// What the panel and `picture` show about the selected object.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectInfo {
    /// Where the object's character is.
    pub at: Pos,
    pub width: f32,
    pub height: f32,
    /// One of [`WRAPS`].
    pub wrap: String,
    /// `wp:positionH` and `wp:positionV`: what each offset is measured from, and the offset in points
    /// (`None` when the object is aligned rather than offset).
    pub horizontal: (String, Option<f32>),
    pub vertical: (String, Option<f32>),
    /// `wp:docPr` `descr`.
    pub alt: String,
    /// `wp:docPr` `name`.
    pub name: String,
    /// Picture, Shape or Text box.
    pub kind: &'static str,
    /// `a:srcRect` as fractions cut off the left, top, right and bottom.
    pub crop: (f32, f32, f32, f32),
}

impl Session {
    /// The object the selection is on: the one character it covers, or the character after the caret.
    pub fn selected_object(&self) -> Option<Pos> {
        let (from, to) = self.selection.ordered();
        let paragraph = position::paragraph(&self.document, &from.story, &from.path)?;
        let (_, len) = object_run(paragraph, from.offset)?;
        let covers_one = from.path == to.path && to.offset == from.offset + len;
        (covers_one || self.selection.is_empty()).then(|| from.clone())
    }

    /// Select the object whose character is at `at`.
    pub fn select_object(&mut self, at: Pos) {
        let len = position::paragraph(&self.document, &at.story, &at.path)
            .and_then(|p| object_run(p, at.offset))
            .map_or(1, |(_, len)| len);
        let after = at.at(at.offset + len);
        self.select(at, after);
    }

    /// What the selected object is.
    pub fn object_info(&self) -> Option<ObjectInfo> {
        let at = self.selected_object()?;
        let el = object_el(&self.document, &at)?;
        let frame = frame(el)?;
        Some(info_of(frame, at))
    }

    /// Change the selected object's size, in points. With no height, the height keeps the ratio.
    pub fn set_object_size(&mut self, width: f32, height: Option<f32>) -> Result<(), String> {
        let info = self.object_info().ok_or("the selection is not on a picture or shape")?;
        if width <= 0.0 || height.is_some_and(|h| h <= 0.0) {
            return Err("a size is more than nothing".into());
        }
        let height = height.unwrap_or(info.height * width / info.width.max(0.01));
        self.change_object(move |frame| {
            set_extent(frame, width, height);
            Ok(())
        })
    }

    /// How text wraps round the selected object: one of [`WRAPS`]. Inline turns a floating object into
    /// one that sits in the line; every other kind makes an inline one float where it was.
    pub fn set_object_wrap(&mut self, kind: &str) -> Result<(), String> {
        let kind = kind.to_ascii_lowercase().replace(['_', ' '], "-");
        let kind = match kind.as_str() {
            "topandbottom" | "top-bottom" => "top-and-bottom".to_owned(),
            "in-front" | "in-front-of-text" => "front".to_owned(),
            "behind-text" => "behind".to_owned(),
            "in-line" | "inline-with-text" => "inline".to_owned(),
            _ => kind,
        };
        if !WRAPS.contains(&kind.as_str()) {
            return Err(format!("wrap takes {}, not {kind}", WRAPS.join(", ")));
        }
        self.change_object(move |frame| {
            if kind == "inline" {
                to_inline(frame);
            } else {
                to_anchor(frame);
                set_wrap(frame, &kind);
            }
            Ok(())
        })
    }

    /// Where the selected floating object sits: `relative` is what both offsets are measured from
    /// (`page`, `margin`, `column` or `paragraph`, or one per axis as `page/paragraph`), and `x`, `y`
    /// are points. An inline object starts floating, square wrapped.
    pub fn set_object_position(&mut self, relative: &str, x: f32, y: f32) -> Result<(), String> {
        let (h, v) = relative.split_once('/').unwrap_or((relative, relative));
        let h = horizontal_relative(h)?;
        let v = vertical_relative(v)?;
        self.change_object(move |frame| {
            if frame.name == "wp:inline" {
                to_anchor(frame);
                set_wrap(frame, "square");
            }
            set_axis(frame, "wp:positionH", h, x);
            set_axis(frame, "wp:positionV", v, y);
            Ok(())
        })
    }

    /// Cut the selected picture: the percent of each side cut off, measured on the whole picture. The
    /// drawn size shrinks with the crop, as Word's crop handles do.
    pub fn crop_object(
        &mut self,
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    ) -> Result<(), String> {
        let info = self.object_info().ok_or("the selection is not on a picture")?;
        if info.kind != "Picture" {
            return Err("only a picture can be cropped".into());
        }
        let fractions = [left, top, right, bottom].map(|p| (p / 100.0).clamp(0.0, 0.99));
        if fractions[0] + fractions[2] >= 1.0 || fractions[1] + fractions[3] >= 1.0 {
            return Err("a crop leaves some of the picture".into());
        }
        let (l0, t0, r0, b0) = info.crop;
        let full_w = info.width / (1.0 - l0 - r0).max(0.01);
        let full_h = info.height / (1.0 - t0 - b0).max(0.01);
        self.change_object(move |frame| {
            let [l, t, r, b] = fractions;
            set_extent(frame, full_w * (1.0 - l - r), full_h * (1.0 - t - b));
            let fill = find_mut(frame, "pic:blipFill").ok_or("this picture has no fill to crop")?;
            fill.remove_children("a:srcRect");
            if [l, t, r, b].iter().any(|f| *f > 0.0) {
                let per = |f: f32| ((f * 100_000.0).round() as i64).to_string();
                let rect = El::new("a:srcRect")
                    .with("l", per(l))
                    .with("t", per(t))
                    .with("r", per(r))
                    .with("b", per(b));
                let at = fill
                    .children
                    .iter()
                    .position(|n| matches!(n, Node::El(e) if e.name == "a:blip"));
                fill.children.insert(at.map_or(0, |i| i + 1), Node::El(rect));
            }
            Ok(())
        })
    }

    /// The selected object's alternative text, which a screen reader says and the accessibility check asks for.
    pub fn set_object_alt_text(&mut self, text: &str) -> Result<(), String> {
        let text = text.to_owned();
        self.change_object(move |frame| {
            let doc_pr = frame
                .child_mut("wp:docPr")
                .ok_or("this object has no properties to hold alt text")?;
            if text.is_empty() {
                doc_pr.remove_attr("descr");
            } else {
                doc_pr.set_attr("descr", text);
            }
            Ok(())
        })
    }

    /// Put the selected picture back as it was inserted: its own size at 96 pixels an inch and no crop.
    pub fn reset_object(&mut self) -> Result<(), String> {
        let at = self.selected_object().ok_or("the selection is not on a picture")?;
        let part = picture_part(&self.document, &at).ok_or("only a picture can be reset")?;
        let bytes = self.document.package.part(&part).ok_or("the picture's part is missing")?;
        let (w, h) =
            crate::media::picture_size(bytes).ok_or("the picture's size cannot be read")?;
        let (w, h) = (w as f32 * 0.75, h as f32 * 0.75);
        self.change_object(move |frame| {
            if let Some(fill) = find_mut(frame, "pic:blipFill") {
                fill.remove_children("a:srcRect");
            }
            set_extent(frame, w, h);
            Ok(())
        })
    }

    /// Move the selected floating object one step up (`forward`) or down the stack, or to the very
    /// front or back (`front`, `back`), by its `relativeHeight`.
    pub fn order_object(&mut self, how: &str) -> Result<(), String> {
        let heights = every_relative_height(&self.document);
        let top = heights.iter().copied().max().unwrap_or(251_659_264);
        let bottom = heights.iter().copied().min().unwrap_or(251_659_264);
        let how = how.to_owned();
        self.change_object(move |frame| {
            if frame.name != "wp:anchor" {
                return Err(
                    "an inline picture is in the line; float it before changing its order".into()
                );
            }
            let now: i64 = frame.attr("relativeHeight").and_then(|h| h.parse().ok()).unwrap_or(top);
            let next = match how.as_str() {
                "forward" | "bring-forward" => now + 1024,
                "backward" | "send-backward" => (now - 1024).max(0),
                "front" | "bring-to-front" => top + 1024,
                "back" | "send-to-back" => (bottom - 1024).max(0),
                other => {
                    return Err(format!(
                        "order takes forward, backward, front or back, not {other}"
                    ))
                }
            };
            frame.set_attr("relativeHeight", next.to_string());
            if how.contains("front") || how.contains("back") && !how.contains("backward") {
                frame.set_attr("behindDoc", if how.contains("back") { "1" } else { "0" });
            }
            Ok(())
        })
    }

    /// Move the selected floating object so its top left corner is `x`, `y` points from the page's top
    /// left, which is how a drag ends.
    pub fn move_object_on_page(&mut self, x: f32, y: f32) -> Result<(), String> {
        self.change_object(move |frame| {
            if frame.name == "wp:inline" {
                return Err("an inline picture moves with the text".into());
            }
            set_axis(frame, "wp:positionH", "page", x);
            set_axis(frame, "wp:positionV", "page", y);
            Ok(())
        })
    }

    /// Move the selected inline object to `to`, as a drag and drop of it does.
    pub fn move_object_to(&mut self, to: Pos) -> Result<(), String> {
        let at = self.selected_object().ok_or("the selection is not on a picture")?;
        if to.story != at.story {
            return Err("a picture is moved within its own story".into());
        }
        self.change(Scope::Story, move |document, selection, _| {
            let Some(paragraph) = position::paragraph_mut(document, &at.story, &at.path) else {
                return selection.clone();
            };
            let Some((start, len)) = object_run(paragraph, at.offset) else {
                return selection.clone();
            };
            let run = paragraph.runs.remove(start);
            for mark in &mut paragraph.marks {
                if mark.offset > at.offset {
                    mark.offset -= len;
                }
            }
            paragraph.merge_runs();
            let mut to = to.clone();
            if to.path == at.path && to.offset > at.offset {
                to.offset = to.offset.saturating_sub(len).max(at.offset);
            }
            let Some(target) = position::paragraph_mut(document, &to.story, &to.path) else {
                return selection.clone();
            };
            let offset = to.offset.min(target.len());
            let index = target.split_at(offset);
            target.runs.insert(index, run);
            for mark in &mut target.marks {
                if mark.offset > offset {
                    mark.offset += len;
                }
            }
            let at = to.at(offset);
            Selection { anchor: at.clone(), head: at.at(offset + len) }
        });
        Ok(())
    }

    /// Insert a text box at the caret, `width` by `height` points, floating square wrapped beside the
    /// paragraph, and put the caret in it.
    pub fn insert_textbox(&mut self, width: f32, height: f32) -> Result<(), String> {
        let id = next_doc_pr(&self.document);
        let mut wsp = shape_el("rect", width, height, false);
        wsp.children.push(Node::El(
            El::new("wps:txbx").with_child(El::new("w:txbxContent").with_child(El::new("w:p"))),
        ));
        wsp.children.push(Node::El(body_pr(true)));
        let drawing = anchored(wsp, id, "Text Box", width, height, true);
        self.insert_object(drawing)?;
        self.document.textboxes.push(TextBox {
            id: id as i64,
            blocks: vec![Block::Paragraph(Paragraph::new())],
            dirty: true,
        });
        self.selection =
            Selection::caret(Pos { story: StoryId::TextBox(id as i64), path: vec![0], offset: 0 });
        Ok(())
    }

    /// Insert one of the preset [`SHAPES`] at the caret, `width` by `height` points, floating in front of
    /// the text.
    pub fn insert_shape(&mut self, preset: &str, width: f32, height: f32) -> Result<(), String> {
        let preset = SHAPES
            .iter()
            .find(|s| {
                s.eq_ignore_ascii_case(preset) || preset.eq_ignore_ascii_case(&s.replace("1", ""))
            })
            .ok_or_else(|| format!("shape takes {}, not {preset}", SHAPES.join(", ")))?;
        let id = next_doc_pr(&self.document);
        let line = preset == &"line" || preset == &"straightConnector1";
        let mut wsp = shape_el(preset, width, height, line);
        wsp.children.push(Node::El(body_pr(false)));
        let drawing = anchored(wsp, id, &format!("{preset} {id}"), width, height, false);
        self.insert_object(drawing)
    }

    /// Put an object at the caret and select it.
    fn insert_object(&mut self, drawing: El) -> Result<(), String> {
        let at = self.selection.ordered().1.clone();
        if position::paragraph(&self.document, &at.story, &at.path).is_none() {
            return Err("the caret is not in a paragraph".into());
        }
        self.change(Scope::Story, move |document, selection, _| {
            let (props, wraps) = position::paragraph(document, &at.story, &at.path)
                .map(|p| crate::edit::props_at(p, at.offset))
                .unwrap_or_default();
            let Some(paragraph) = position::paragraph_mut(document, &at.story, &at.path) else {
                return selection.clone();
            };
            let index = paragraph.split_at(at.offset);
            let run = Run {
                props: Arc::clone(&props),
                content: Content::Object(drawing),
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
            Selection { anchor: at.clone(), head: at.at(at.offset + len) }
        });
        Ok(())
    }

    /// Change the selected object's frame (`wp:inline` or `wp:anchor`) in one undo step.
    fn change_object(
        &mut self,
        change: impl FnOnce(&mut El) -> Result<(), String> + 'static,
    ) -> Result<(), String> {
        let at = self.selected_object().ok_or("the selection is not on a picture or shape")?;
        // Try the change on a copy first, so a refusal leaves no undo step behind.
        let mut el = object_el(&self.document, &at).cloned().ok_or("the object has gone")?;
        let el =
            choice_drawing(&el).unwrap_or_else(|| std::mem::replace(&mut el, El::new("w:drawing")));
        let mut changed = el;
        change(frame_mut(&mut changed).ok_or("the object has no frame")?)?;
        self.change(Scope::Story, move |document, selection, _| {
            if let Some(paragraph) = position::paragraph_mut(document, &at.story, &at.path) {
                if let Some((index, _)) = object_run(paragraph, at.offset) {
                    paragraph.runs[index].content = Content::Object(changed);
                }
            }
            selection.clone()
        });
        Ok(())
    }
}

/// The `w:drawing` the character at `at` holds.
fn object_el<'a>(document: &'a Document, at: &Pos) -> Option<&'a El> {
    let paragraph = position::paragraph(document, &at.story, &at.path)?;
    let (index, _) = object_run(paragraph, at.offset)?;
    match &paragraph.runs[index].content {
        Content::Object(el) => Some(el),
        _ => None,
    }
}

/// The run that starts at `offset` when it is a drawing, with its length in the flat text.
fn object_run(paragraph: &Paragraph, offset: usize) -> Option<(usize, usize)> {
    let mut at = 0;
    for (index, run) in paragraph.runs.iter().enumerate() {
        if at == offset && !run.is_empty() {
            return matches!(&run.content, Content::Object(el) if frame(el).is_some())
                .then(|| (index, run.len()));
        }
        at += run.len();
        if at > offset {
            return None;
        }
    }
    None
}

/// The picture part the object at `at` shows, when it is a picture.
fn picture_part(document: &Document, at: &Pos) -> Option<String> {
    let el = object_el(document, at)?;
    let blip = crate::read::descendant(frame(el)?, "a:blip")?;
    let id = blip.attr("r:embed")?;
    let part = match &at.story {
        StoryId::Header(rid) => document.headers.iter().find(|h| &h.id == rid)?.name.clone(),
        _ => document.main.clone(),
    };
    let rel = document.package.rels.get(&part)?.by_id(id)?;
    (!rel.external).then(|| crate::package::Package::resolve(&part, &rel.target))
}

/// The `w:drawing` inside an `mc:AlternateContent`'s Choice.
fn choice_drawing(el: &El) -> Option<El> {
    if el.name != "mc:AlternateContent" {
        return None;
    }
    el.children_named("mc:Choice").find_map(|c| c.child("w:drawing")).cloned()
}

/// A drawing's `wp:inline` or `wp:anchor`, in the Choice when there is one.
pub fn frame(el: &El) -> Option<&El> {
    if el.name == "wp:inline" || el.name == "wp:anchor" {
        return Some(el);
    }
    el.elements().find_map(frame)
}

fn frame_mut(el: &mut El) -> Option<&mut El> {
    if el.name == "wp:inline" || el.name == "wp:anchor" {
        return Some(el);
    }
    el.elements_mut().find_map(frame_mut)
}

/// The first descendant named `name`, to change.
fn find_mut<'a>(el: &'a mut El, name: &str) -> Option<&'a mut El> {
    if el.name == name {
        return Some(el);
    }
    el.elements_mut().find_map(|child| find_mut(child, name))
}

fn info_of(frame: &El, at: Pos) -> ObjectInfo {
    let emu = |v: Option<&str>| v.and_then(crate::props::number).map_or(0.0, crate::props::emu);
    let extent = frame.child("wp:extent");
    let axis = |name: &str| -> (String, Option<f32>) {
        let Some(axis) = frame.child(name) else { return (String::new(), None) };
        let relative = axis.attr("relativeFrom").unwrap_or("column").to_owned();
        (relative, axis.child("wp:posOffset").map(|o| emu(Some(o.text().trim()))))
    };
    let doc_pr = frame.child("wp:docPr");
    let graphic = crate::read::descendant(frame, "a:graphicData");
    let kind = if graphic.and_then(|g| g.child("pic:pic")).is_some() {
        "Picture"
    } else if crate::read::descendant(frame, "wps:txbx").is_some() {
        "Text box"
    } else {
        "Shape"
    };
    let crop = crate::read::descendant(frame, "a:srcRect").map_or((0.0, 0.0, 0.0, 0.0), |r| {
        let f = |name: &str| {
            r.attr(name).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 100_000.0
        };
        (f("l"), f("t"), f("r"), f("b"))
    });
    ObjectInfo {
        at,
        width: emu(extent.and_then(|e| e.attr("cx"))),
        height: emu(extent.and_then(|e| e.attr("cy"))),
        wrap: wrap_of(frame),
        horizontal: axis("wp:positionH"),
        vertical: axis("wp:positionV"),
        alt: doc_pr.and_then(|d| d.attr("descr")).unwrap_or_default().to_owned(),
        name: doc_pr.and_then(|d| d.attr("name")).unwrap_or_default().to_owned(),
        kind,
        crop,
    }
}

/// The wrap kind a frame has, as one of [`WRAPS`].
pub fn wrap_of(frame: &El) -> String {
    if frame.name == "wp:inline" {
        return "inline".into();
    }
    let wrap =
        frame.elements().find(|c| c.local().starts_with("wrap")).map(|c| c.local().to_owned());
    match wrap.as_deref() {
        Some("wrapSquare") => "square",
        Some("wrapTight") => "tight",
        Some("wrapThrough") => "through",
        Some("wrapTopAndBottom") => "top-and-bottom",
        _ if frame.attr("behindDoc").is_some_and(|v| v == "1" || v == "true") => "behind",
        _ => "front",
    }
    .into()
}

fn emu_text(points: f32) -> String {
    ((points * 12_700.0).round() as i64).to_string()
}

/// Write a size into `wp:extent` and the shape's own `a:xfrm/a:ext`.
fn set_extent(frame: &mut El, width: f32, height: f32) {
    let (cx, cy) = (emu_text(width), emu_text(height));
    let extent = frame.child_or_insert("wp:extent");
    extent.set_attr("cx", cx.clone());
    extent.set_attr("cy", cy.clone());
    if let Some(xfrm) = find_mut(frame, "a:xfrm") {
        let ext = xfrm.child_or_insert("a:ext");
        ext.set_attr("cx", cx);
        ext.set_attr("cy", cy);
    }
}

/// The children of a frame, without the ones `wp:anchor` has and `wp:inline` does not.
const ANCHOR_ONLY: [&str; 8] = [
    "wp:simplePos",
    "wp:positionH",
    "wp:positionV",
    "wp:wrapNone",
    "wp:wrapSquare",
    "wp:wrapTight",
    "wp:wrapThrough",
    "wp:wrapTopAndBottom",
];

/// Turn a floating frame into an inline one, keeping its size, its properties and its graphic.
fn to_inline(frame: &mut El) {
    if frame.name == "wp:inline" {
        return;
    }
    frame.name = "wp:inline".into();
    frame.children.retain(|n| !matches!(n, Node::El(e) if ANCHOR_ONLY.contains(&e.name.as_str()) || e.name.starts_with("wp14:")));
    let keep = ["distT", "distB", "distL", "distR"];
    frame.attrs.retain(|(k, _)| keep.contains(&k.as_str()));
    for k in keep {
        if frame.attr(k).is_none() {
            frame.set_attr(k, "0");
        }
    }
}

/// Turn an inline frame into a floating one at the column's left and the paragraph's top, with no
/// wrap yet. The children are put in the order the schema gives a `wp:anchor`.
fn to_anchor(frame: &mut El) {
    if frame.name == "wp:anchor" {
        return;
    }
    frame.name = "wp:anchor".into();
    let mut children: Vec<Node> = Vec::new();
    children.push(Node::El(El::new("wp:simplePos").with("x", "0").with("y", "0")));
    children.push(Node::El(position_el("wp:positionH", "column", 0.0)));
    children.push(Node::El(position_el("wp:positionV", "paragraph", 0.0)));
    let old = std::mem::take(&mut frame.children);
    for name in ["wp:extent", "wp:effectExtent"] {
        if let Some(n) = old.iter().find(|n| matches!(n, Node::El(e) if e.name == name)) {
            children.push(n.clone());
        }
    }
    children.push(Node::El(El::new("wp:wrapNone")));
    for n in old {
        if !matches!(&n, Node::El(e) if e.name == "wp:extent" || e.name == "wp:effectExtent") {
            children.push(n);
        }
    }
    frame.children = children;
    let dist = |frame: &El, k: &str, d: &str| frame.attr(k).unwrap_or(d).to_owned();
    let attrs = vec![
        ("distT".to_owned(), dist(frame, "distT", "0")),
        ("distB".to_owned(), dist(frame, "distB", "0")),
        ("distL".to_owned(), "114300".to_owned()),
        ("distR".to_owned(), "114300".to_owned()),
        ("simplePos".to_owned(), "0".to_owned()),
        ("relativeHeight".to_owned(), "251659264".to_owned()),
        ("behindDoc".to_owned(), "0".to_owned()),
        ("locked".to_owned(), "0".to_owned()),
        ("layoutInCell".to_owned(), "1".to_owned()),
        ("allowOverlap".to_owned(), "1".to_owned()),
    ];
    frame.attrs = attrs;
}

fn position_el(name: &str, relative: &str, offset: f32) -> El {
    El::new(name)
        .with("relativeFrom", relative)
        .with_child(El::new("wp:posOffset").with_text(emu_text(offset)))
}

/// Replace a floating frame's wrap element with the one for `kind`.
fn set_wrap(frame: &mut El, kind: &str) {
    let at = frame
        .children
        .iter()
        .position(|n| matches!(n, Node::El(e) if e.name.starts_with("wp:wrap")))
        .unwrap_or(frame.children.len());
    frame.children.retain(|n| !matches!(n, Node::El(e) if e.name.starts_with("wp:wrap")));
    let polygon = || {
        let point = |name: &str, x: &str, y: &str| El::new(name).with("x", x).with("y", y);
        El::new("wp:wrapPolygon")
            .with("edited", "0")
            .with_child(point("wp:start", "0", "0"))
            .with_child(point("wp:lineTo", "0", "21600"))
            .with_child(point("wp:lineTo", "21600", "21600"))
            .with_child(point("wp:lineTo", "21600", "0"))
            .with_child(point("wp:lineTo", "0", "0"))
    };
    let el = match kind {
        "square" => El::new("wp:wrapSquare").with("wrapText", "bothSides"),
        "tight" => El::new("wp:wrapTight").with("wrapText", "bothSides").with_child(polygon()),
        "through" => El::new("wp:wrapThrough").with("wrapText", "bothSides").with_child(polygon()),
        "top-and-bottom" => El::new("wp:wrapTopAndBottom"),
        _ => El::new("wp:wrapNone"),
    };
    frame.children.insert(at.min(frame.children.len()), Node::El(el));
    frame.set_attr("behindDoc", if kind == "behind" { "1" } else { "0" });
}

fn set_axis(frame: &mut El, name: &str, relative: &str, offset: f32) {
    let new = position_el(name, relative, offset);
    match frame.children.iter_mut().find_map(|n| match n {
        Node::El(e) if e.name == name => Some(e),
        _ => None,
    }) {
        Some(axis) => *axis = new,
        None => {
            let after = if name == "wp:positionH" { "wp:simplePos" } else { "wp:positionH" };
            let at =
                frame.children.iter().position(|n| matches!(n, Node::El(e) if e.name == after));
            frame.children.insert(at.map_or(0, |i| i + 1), Node::El(new));
        }
    }
}

fn horizontal_relative(name: &str) -> Result<&'static str, String> {
    Ok(match name {
        "page" => "page",
        "margin" => "margin",
        "column" => "column",
        "character" => "character",
        "left-margin" | "leftMargin" => "leftMargin",
        "right-margin" | "rightMargin" => "rightMargin",
        other => {
            return Err(format!(
                "a horizontal position is from page, margin, column or character, not {other}"
            ))
        }
    })
}

fn vertical_relative(name: &str) -> Result<&'static str, String> {
    Ok(match name {
        "page" => "page",
        "margin" => "margin",
        "paragraph" | "column" => "paragraph",
        "line" => "line",
        "top-margin" | "topMargin" => "topMargin",
        "bottom-margin" | "bottomMargin" => "bottomMargin",
        other => {
            return Err(format!(
                "a vertical position is from page, margin, paragraph or line, not {other}"
            ))
        }
    })
}

/// Every floating object's `relativeHeight` in the body.
fn every_relative_height(document: &Document) -> Vec<i64> {
    let mut heights = Vec::new();
    for (_, paragraph) in position::paragraphs(&document.body) {
        for run in &paragraph.runs {
            if let Content::Object(el) = &run.content {
                el.walk(&mut |node| {
                    if node.name == "wp:anchor" {
                        if let Some(h) = node.attr("relativeHeight").and_then(|h| h.parse().ok()) {
                            heights.push(h);
                        }
                    }
                });
            }
        }
    }
    heights
}

/// A `wp:docPr` id nothing in the document has, counting every story.
fn next_doc_pr(document: &Document) -> u32 {
    let mut largest = 0u32;
    let mut look = |blocks: &[Block]| {
        for (_, paragraph) in position::paragraphs(blocks) {
            for run in &paragraph.runs {
                if let Content::Object(el) = &run.content {
                    el.walk(&mut |node| {
                        if node.name == "wp:docPr" {
                            if let Some(id) = node.attr("id").and_then(|id| id.parse::<u32>().ok())
                            {
                                largest = largest.max(id);
                            }
                        }
                    });
                }
            }
        }
    };
    look(&document.body);
    for header in &document.headers {
        look(&header.story.blocks);
    }
    for t in &document.textboxes {
        largest = largest.max(t.id as u32);
    }
    largest + 1
}

/// A `wps:wsp` with the preset geometry, Word's default blue fill and a darker outline. A line has
/// no fill.
fn shape_el(preset: &str, width: f32, height: f32, line: bool) -> El {
    let solid =
        |hex: &str| El::new("a:solidFill").with_child(El::new("a:srgbClr").with("val", hex));
    let mut sp_pr = El::new("wps:spPr")
        .with_child(
            El::new("a:xfrm")
                .with_child(El::new("a:off").with("x", "0").with("y", "0"))
                .with_child(
                    El::new("a:ext").with("cx", emu_text(width)).with("cy", emu_text(height)),
                ),
        )
        .with_child(El::new("a:prstGeom").with("prst", preset).with_child(El::new("a:avLst")));
    let fill = if line { El::new("a:noFill") } else { solid("156082") };
    sp_pr.children.push(Node::El(fill));
    sp_pr.children.push(Node::El(El::new("a:ln").with("w", "12700").with_child(solid("0F4761"))));
    let mut wsp = El::new("wps:wsp");
    if line {
        wsp.children.push(Node::El(El::new("wps:cNvCnPr")));
    } else {
        wsp.children.push(Node::El(El::new("wps:cNvSpPr")));
    }
    wsp.children.push(Node::El(sp_pr));
    wsp
}

/// `wps:bodyPr` with Word's insets; a text box's is anchored at the top.
fn body_pr(textbox: bool) -> El {
    let mut body = El::new("wps:bodyPr")
        .with("rot", "0")
        .with("vert", "horz")
        .with("wrap", "square")
        .with("lIns", "91440")
        .with("tIns", "45720")
        .with("rIns", "91440")
        .with("bIns", "45720")
        .with("anchor", if textbox { "t" } else { "ctr" })
        .with("anchorCtr", "0");
    body.children.push(Node::El(El::new(if textbox { "a:spAutoFit" } else { "a:noAutofit" })));
    body
}

/// A floating `w:drawing` holding a shape, square wrapped for a text box and in front of the text
/// otherwise.
fn anchored(wsp: El, id: u32, name: &str, width: f32, height: f32, textbox: bool) -> El {
    let mut frame = El::new("wp:inline")
        .with_child(El::new("wp:extent").with("cx", emu_text(width)).with("cy", emu_text(height)))
        .with_child(
            El::new("wp:effectExtent").with("l", "0").with("t", "0").with("r", "0").with("b", "0"),
        )
        .with_child(El::new("wp:docPr").with("id", id.to_string()).with("name", name))
        .with_child(El::new("wp:cNvGraphicFramePr"))
        .with_child(
            El::new("a:graphic").with_child(
                El::new("a:graphicData")
                    .with(
                        "uri",
                        "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
                    )
                    .with_child(wsp),
            ),
        );
    to_anchor(&mut frame);
    set_wrap(&mut frame, if textbox { "square" } else { "front" });
    El::new("w:drawing").with_child(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_picture() -> Session {
        let mut session = Session::new(crate::new::blank());
        session.type_text("before after");
        session.select(
            Pos { story: StoryId::Body, path: vec![0], offset: 7 },
            Pos { story: StoryId::Body, path: vec![0], offset: 7 },
        );
        // A one pixel PNG.
        let png: &[u8] = &[
            0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0,
            40, 0, 0, 0, 20, 8, 6, 0, 0, 0,
        ];
        crate::media::insert_picture(&mut session, png, "png", 100.0, 50.0, "Picture 1").unwrap();
        session.select_object(Pos { story: StoryId::Body, path: vec![0], offset: 7 });
        session
    }

    #[test]
    fn a_picture_is_resized_wrapped_positioned_and_cropped() {
        let mut session = session_with_picture();
        let info = session.object_info().expect("the picture is selected");
        assert_eq!((info.width, info.height, info.wrap.as_str()), (100.0, 50.0, "inline"));
        session.set_object_size(200.0, None).unwrap();
        assert_eq!(session.object_info().unwrap().height, 100.0, "the ratio is kept");
        session.set_object_wrap("tight").unwrap();
        let info = session.object_info().unwrap();
        assert_eq!(info.wrap, "tight");
        session.set_object_position("page", 72.0, 144.0).unwrap();
        let info = session.object_info().unwrap();
        assert_eq!(info.horizontal, ("page".to_owned(), Some(72.0)));
        assert_eq!(info.vertical, ("page".to_owned(), Some(144.0)));
        session.crop_object(25.0, 0.0, 25.0, 0.0).unwrap();
        let info = session.object_info().unwrap();
        assert_eq!(info.width, 100.0, "half the width is cut off");
        assert!((info.crop.0 - 0.25).abs() < 0.001);
        session.set_object_alt_text("A chart of sales").unwrap();
        assert_eq!(session.object_info().unwrap().alt, "A chart of sales");
        session.set_object_wrap("inline").unwrap();
        assert_eq!(session.object_info().unwrap().wrap, "inline");
        session.reset_object().unwrap();
        let info = session.object_info().unwrap();
        assert_eq!(
            (info.width, info.height, info.crop.0),
            (30.0, 15.0, 0.0),
            "40 by 20 pixels at 96 an inch"
        );
        // Each of the seven changes was one undo step.
        for _ in 0..7 {
            assert!(session.undo());
        }
        let info = session.object_info().unwrap();
        assert_eq!(
            (info.width, info.height, info.wrap.as_str(), info.alt.as_str()),
            (100.0, 50.0, "inline", "Picture 1")
        );
    }

    #[test]
    fn a_text_box_and_a_shape_are_inserted() {
        let mut session = Session::new(crate::new::blank());
        session.insert_textbox(144.0, 72.0).unwrap();
        assert!(matches!(session.selection.head.story, StoryId::TextBox(_)));
        session.type_text("inside");
        let saved = crate::write::save(&session.document).unwrap();
        let back = crate::read::open(&saved).unwrap();
        assert_eq!(back.textboxes.len(), 1);
        let text: String = position::paragraphs(&back.textboxes[0].blocks)
            .iter()
            .map(|(_, p)| p.flat_text())
            .collect();
        assert_eq!(text, "inside");
        let mut session = Session::new(crate::new::blank());
        session.insert_shape("ellipse", 72.0, 72.0).unwrap();
        let info = session.object_info().unwrap();
        assert_eq!((info.kind, info.wrap.as_str()), ("Shape", "front"));
        assert!(session.insert_shape("star", 1.0, 1.0).is_err());
    }

    #[test]
    fn an_inline_picture_moves_to_where_it_is_dropped() {
        let mut session = session_with_picture();
        session.move_object_to(Pos { story: StoryId::Body, path: vec![0], offset: 0 }).unwrap();
        let paragraph = position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap();
        assert!(matches!(paragraph.runs[0].content, Content::Object(_)));
        assert_eq!(session.selected_object().map(|p| p.offset), Some(0));
    }
}
