//! What a `w:drawing`, a `w:pict` or `mc:AlternateContent` draws: a picture, a shape with its fill and
//! outline, a text box with its text laid out inside it, or a labelled placeholder for what Unluminous
//! does not draw (task-2359, TDD 5.3).
//!
//! Ten preset shapes are drawn (`rect`, `roundRect`, `ellipse`, `line`, `straightConnector1`, the four
//! block arrows, `triangle`, `diamond`). Any other preset, a custom geometry, a group, a chart and SmartArt
//! draw the picture Word cached beside them in `mc:Fallback` when there is one, and otherwise a box with
//! the name of what it is. Nothing about a drawing is changed by drawing it.

use super::{lay_out_story, Anchor, Chunk, Ctx, Float, Item, Rect, RuleKind};
use crate::numbering::Counters;
use crate::position::StoryId;
use crate::props::{self, Rgb};
use crate::read::descendant;
use crate::xml::El;

/// The presets drawn with the painter's own shapes.
pub const PRESETS: &[&str] = &[
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

/// A drawing read: in a line or floating, its size, and how it draws.
#[derive(Debug, Clone)]
pub struct Drawing {
    pub inline: bool,
    pub float: Float,
}

/// Read what a drawing element draws. `None` when it is not a drawing.
pub fn read(ctx: &Ctx, el: &El) -> Option<Drawing> {
    // `mc:AlternateContent` holds the drawing in its choice and a VML copy in its fallback.
    let (drawing, fallback) = match el.name.as_str() {
        "mc:AlternateContent" => {
            let choice = el
                .elements()
                .filter(|branch| branch.name == "mc:Choice")
                .find_map(|branch| branch.elements().find(|e| e.name == "w:drawing"));
            let fallback = el.child("mc:Fallback");
            match choice {
                Some(choice) => (choice, fallback),
                None => (fallback?.elements().next()?, None),
            }
        }
        _ => (el, None),
    };
    if drawing.name == "w:pict" || drawing.name == "w:object" {
        return Some(vml(ctx, drawing));
    }
    let frame = drawing.elements().next()?;
    let extent = frame.child("wp:extent")?;
    let w = props::emu(extent.attr("cx").and_then(props::number).unwrap_or(0.0));
    let h = props::emu(extent.attr("cy").and_then(props::number).unwrap_or(0.0));
    let inline = frame.name == "wp:inline";
    let position = |name: &str| -> (String, Anchor) {
        let Some(axis) = frame.child(name) else { return ("column".into(), Anchor::Offset(0.0)) };
        let relative = axis.attr("relativeFrom").unwrap_or("column").to_owned();
        let anchor = match (axis.child("wp:posOffset"), axis.child("wp:align")) {
            (Some(offset), _) => {
                Anchor::Offset(props::emu(props::number(&offset.text()).unwrap_or(0.0)))
            }
            (None, Some(align)) => Anchor::Align(align.text()),
            _ => Anchor::Offset(0.0),
        };
        (relative, anchor)
    };
    let wrap_el = frame.elements().find(|child| child.local().starts_with("wrap"));
    let wrap = match wrap_el.map(|el| el.local().trim_start_matches("wrap").to_lowercase()) {
        Some(kind) if kind != "none" => kind,
        _ if frame.attr("behindDoc").is_some_and(|v| v == "1" || v == "true") => "behind".into(),
        _ => "none".into(),
    };
    let distance = |name: &str| props::emu(frame.attr(name).and_then(props::number).unwrap_or(0.0));
    // A wrap polygon is in 21600ths of the picture's size.
    let contour = wrap_el.and_then(|el| el.child("wp:wrapPolygon")).and_then(|polygon| {
        let points: Vec<(f32, f32)> = polygon
            .elements()
            .filter_map(|p| {
                Some((
                    p.attr("x").and_then(props::number)? as f32,
                    p.attr("y").and_then(props::number)? as f32,
                ))
            })
            .collect();
        if points.is_empty() {
            return None;
        }
        let fold = |f: fn(f32, f32) -> f32, pick: fn(&(f32, f32)) -> f32, start: f32| {
            points.iter().map(pick).fold(start, f) / 21600.0
        };
        Some((
            fold(f32::min, |p| p.0, f32::MAX),
            fold(f32::min, |p| p.1, f32::MAX),
            fold(f32::max, |p| p.0, f32::MIN),
            fold(f32::max, |p| p.1, f32::MIN),
        ))
    });
    let mut float = Float {
        width: w,
        height: h,
        contour,
        horizontal: position("wp:positionH"),
        vertical: position("wp:positionV"),
        part: String::new(),
        wrap,
        side: wrap_el.and_then(|el| el.attr("wrapText")).unwrap_or("bothSides").to_owned(),
        distances: (distance("distT"), distance("distB"), distance("distL"), distance("distR")),
        inside: Vec::new(),
        fill: None,
        outline: None,
        label: "Picture".into(),
        preset: None,
        crop: None,
        flip: (false, false),
        source: None,
    };
    let graphic = frame.child("a:graphic").and_then(|g| g.child("a:graphicData"));
    let uri = graphic.and_then(|g| g.attr("uri")).unwrap_or("");
    if let Some(pic) = graphic.and_then(|g| g.child("pic:pic")) {
        float.part = part_of(ctx, pic).unwrap_or_default();
        float.crop = crop(pic);
        return Some(Drawing { inline, float });
    }
    if let Some(shape) = graphic.and_then(|g| g.child("wps:wsp")) {
        shape_look(ctx, shape, &mut float);
        let id = frame.child("wp:docPr").and_then(|d| d.attr("id")).and_then(|i| i.parse().ok());
        if let (Some(id), Some(body)) = (id, shape.child("wps:bodyPr")) {
            if let Some(txbx) = shape.child("wps:txbx") {
                float.inside = text_inside(ctx, id, body, w, h);
                // The first box of a chain keeps only what fits; the rest flows on (`wps:linkedTxbx`).
                if let Some(chain) = txbx.attr("id").and_then(|c| c.parse::<i64>().ok()) {
                    if linked_boxes(ctx, chain).len() > 1 {
                        let capacity = box_room(body, h);
                        float.inside = split_into_boxes(float.inside, &[capacity]).remove(0);
                    }
                }
            } else if let Some(link) = shape.child("wps:linkedTxbx") {
                let chain = link.attr("id").and_then(|c| c.parse::<i64>().ok());
                let seq = link.attr("seq").and_then(|c| c.parse::<usize>().ok()).unwrap_or(1);
                if let Some(chain) = chain {
                    float.inside = linked_inside(ctx, chain, seq, body, h);
                }
            }
            if body.attr("fromWordArt").is_some_and(|v| v == "1") {
                float.label = "WordArt".into();
            }
        }
        if float.preset.is_none() && float.inside.is_empty() {
            fallback_picture(ctx, fallback, &mut float);
        }
        return Some(Drawing { inline, float });
    }
    float.label = if uri.ends_with("/chart") {
        "Chart"
    } else if uri.ends_with("/diagram") {
        "SmartArt"
    } else if uri.contains("wordprocessingGroup") || uri.contains("wordprocessingCanvas") {
        "Shape"
    } else {
        "Object"
    }
    .into();
    fallback_picture(ctx, fallback, &mut float);
    Some(Drawing { inline, float })
}

/// The picture part a `pic:pic` or a VML image names, by its relationship in the story's part.
fn part_of(ctx: &Ctx, el: &El) -> Option<String> {
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
    let id = embed?;
    let rel = ctx.document.package.rels.get(&ctx.part)?.by_id(&id)?.clone();
    (!rel.external).then(|| crate::package::Package::resolve(&ctx.part, &rel.target))
}

/// A picture bullet: the `w:numPicBullet` numbered `id` in the numbering part, as a picture `size`
/// points square (Word draws a picture bullet at the height of the text), found through the numbering
/// part's own relationships.
pub fn picture_bullet(ctx: &Ctx, id: &str, size: f32) -> Option<Float> {
    let part = ctx.document.numbering.as_ref()?;
    let bullet = part
        .root
        .children_named("w:numPicBullet")
        .find(|b| b.attr("w:numPicBulletId") == Some(id))?;
    let mut embed: Option<String> = None;
    bullet.walk(&mut |node| {
        if embed.is_none() {
            if node.name == "v:imagedata" {
                embed = node.attr("r:id").map(str::to_owned);
            } else if node.name == "a:blip" {
                embed = node.attr("r:embed").map(str::to_owned);
            }
        }
    });
    let rel = ctx.document.package.rels.get(&part.name)?.by_id(&embed?)?.clone();
    if rel.external {
        return None;
    }
    let mut float = vml(ctx, bullet).float;
    float.part = crate::package::Package::resolve(&part.name, &rel.target);
    float.width = size;
    float.height = size;
    float.label = "Picture".into();
    Some(float)
}

/// `a:srcRect`: how much of each side of the picture is cut off, as fractions (left, top, right, bottom).
fn crop(pic: &El) -> Option<(f32, f32, f32, f32)> {
    let rect = descendant(pic, "a:srcRect")?;
    let side =
        |name: &str| rect.attr(name).and_then(props::number).unwrap_or(0.0) as f32 / 100_000.0;
    let crop = (side("l"), side("t"), side("r"), side("b"));
    (crop != (0.0, 0.0, 0.0, 0.0)).then_some(crop)
}

/// The picture Word cached in a drawing's fallback, drawn in place of what Unluminous does not draw.
fn fallback_picture(ctx: &Ctx, fallback: Option<&El>, float: &mut Float) {
    if let Some(part) = fallback.and_then(|f| part_of(ctx, f)) {
        float.part = part;
    }
}

/// A VML picture or shape (`w:pict`): its size from its style and the picture it names.
fn vml(ctx: &Ctx, el: &El) -> Drawing {
    let mut size = None;
    el.walk(&mut |node| {
        if size.is_none() && matches!(node.name.as_str(), "v:shape" | "v:rect" | "v:image") {
            size = node.attr("style").map(vml_size);
        }
    });
    let (width, height) = size.unwrap_or((72.0, 72.0));
    let part = part_of(ctx, el).unwrap_or_default();
    Drawing {
        inline: true,
        float: Float {
            width,
            height,
            contour: None,
            horizontal: ("column".into(), Anchor::Offset(0.0)),
            vertical: ("paragraph".into(), Anchor::Offset(0.0)),
            part,
            wrap: "none".into(),
            side: "bothSides".into(),
            distances: (0.0, 0.0, 0.0, 0.0),
            inside: Vec::new(),
            fill: None,
            outline: None,
            label: "Object".into(),
            preset: None,
            crop: None,
            flip: (false, false),
            source: None,
        },
    }
}

/// `width:100pt;height:50pt` from a VML shape's style.
pub fn vml_size(style: &str) -> (f32, f32) {
    let mut size = (72.0, 72.0);
    for declaration in style.split(';') {
        let Some((key, value)) = declaration.split_once(':') else { continue };
        let value = value.trim();
        let points = if let Some(pt) = value.strip_suffix("pt") {
            pt.trim().parse::<f32>().ok()
        } else if let Some(inches) = value.strip_suffix("in") {
            inches.trim().parse::<f32>().ok().map(|i| i * 72.0)
        } else {
            props::number(value).map(|twips| (twips / 20.0) as f32)
        };
        match (key.trim(), points) {
            ("width", Some(w)) => size.0 = w,
            ("height", Some(h)) => size.1 = h,
            _ => {}
        }
    }
    size
}

/// A shape's preset, fill, outline and flips, from `wps:spPr` and, for what it does not say, `wps:style`.
fn shape_look(ctx: &Ctx, shape: &El, float: &mut Float) {
    let sp = shape.child("wps:spPr");
    let preset = sp.and_then(|s| s.child("a:prstGeom")).and_then(|g| g.attr("prst"));
    float.preset = preset.filter(|p| PRESETS.contains(p)).map(str::to_owned);
    float.label = match preset {
        Some(name) if !PRESETS.contains(&name) => "Shape".into(),
        None if sp.and_then(|s| s.child("a:custGeom")).is_some() => "Shape".into(),
        _ => "Shape".into(),
    };
    if let Some(xfrm) = sp.and_then(|s| s.child("a:xfrm")) {
        float.flip = (xfrm.attr("flipH") == Some("1"), xfrm.attr("flipV") == Some("1"));
    }
    let style = shape.child("wps:style");
    float.fill = match sp {
        Some(sp) if sp.child("a:noFill").is_some() => None,
        Some(sp) if sp.child("a:solidFill").is_some() => {
            sp.child("a:solidFill").and_then(|f| colour(ctx, f))
        }
        Some(sp) if sp.child("a:gradFill").is_some() => sp
            .child("a:gradFill")
            .and_then(|g| g.child("a:gsLst"))
            .and_then(|l| l.elements().next())
            .and_then(|stop| colour(ctx, stop)),
        _ => style
            .and_then(|s| s.child("a:fillRef"))
            .filter(|r| r.attr("idx") != Some("0"))
            .and_then(|r| colour(ctx, r)),
    };
    let line = sp.and_then(|s| s.child("a:ln"));
    let width = line.and_then(|l| l.attr("w")).and_then(props::number).map_or(0.75, props::emu);
    float.outline = match line {
        Some(line) if line.child("a:noFill").is_some() => None,
        Some(line) if line.child("a:solidFill").is_some() => {
            line.child("a:solidFill").and_then(|f| colour(ctx, f)).map(|c| (c, width))
        }
        _ => style
            .and_then(|s| s.child("a:lnRef"))
            .filter(|r| r.attr("idx") != Some("0"))
            .and_then(|r| colour(ctx, r))
            .map(|c| (c, width)),
    };
}

/// The colour a DrawingML colour holder names: `a:srgbClr`, `a:schemeClr` (from the theme), `a:prstClr`
/// or `a:sysClr`, with `a:lumMod` and `a:lumOff` applied the simple way.
fn colour(ctx: &Ctx, holder: &El) -> Option<Rgb> {
    let el = holder.elements().next()?;
    let base = match el.name.as_str() {
        "a:srgbClr" => Rgb::parse(el.attr("val")?),
        "a:sysClr" => el.attr("lastClr").and_then(Rgb::parse),
        "a:prstClr" => match el.attr("val")? {
            "black" => Some(Rgb::BLACK),
            "white" => Some(Rgb::WHITE),
            "red" => Some(Rgb(255, 0, 0)),
            "green" => Some(Rgb(0, 128, 0)),
            "blue" => Some(Rgb(0, 0, 255)),
            "yellow" => Some(Rgb(255, 255, 0)),
            "gray" | "grey" => Some(Rgb(128, 128, 128)),
            _ => None,
        },
        "a:schemeClr" => {
            let name = match el.attr("val")? {
                "tx1" => "dk1",
                "bg1" => "lt1",
                "tx2" => "dk2",
                "bg2" => "lt2",
                other => other,
            };
            ctx.resolver.theme.colour(name)
        }
        _ => None,
    }?;
    let modulate = el.child("a:lumMod").and_then(|m| m.attr("val")).and_then(props::number);
    let offset = el.child("a:lumOff").and_then(|m| m.attr("val")).and_then(props::number);
    let adjust = |c: u8| {
        let mut v = f64::from(c);
        if let Some(m) = modulate {
            v *= m / 100_000.0;
        }
        if let Some(o) = offset {
            v += 255.0 * o / 100_000.0;
        }
        v.round().clamp(0.0, 255.0) as u8
    };
    Some(Rgb(adjust(base.0), adjust(base.1), adjust(base.2)))
}

/// A text box's text laid out inside it, less the insets `wps:bodyPr` gives, and placed at the top, the
/// middle or the bottom as its `anchor` says. Vertical text is left to its placeholder.
fn text_inside(ctx: &Ctx, id: i64, body: &El, w: f32, h: f32) -> Vec<Chunk> {
    let Some(textbox) = ctx.document.textboxes.iter().find(|t| t.id == id) else {
        return Vec::new();
    };
    if body.attr("vert").is_some_and(|v| v != "horz") {
        return Vec::new();
    }
    let inset = |name: &str, fallback: f64| {
        props::emu(body.attr(name).and_then(props::number).unwrap_or(fallback))
    };
    let (left, top, right, bottom) = (
        inset("lIns", 91_440.0),
        inset("tIns", 45_720.0),
        inset("rIns", 91_440.0),
        inset("bIns", 45_720.0),
    );
    let mut counters = Counters::default();
    let chunks = lay_out_story(
        ctx,
        &mut counters,
        &textbox.blocks,
        &StoryId::TextBox(id),
        (w - left - right).max(6.0),
    );
    let content: f32 = chunks.iter().map(|c| c.height).sum();
    let free = (h - top - bottom - content).max(0.0);
    let shift = match body.attr("anchor") {
        Some("ctr") => free / 2.0,
        Some("b") => free,
        _ => 0.0,
    };
    let mut y = top + shift;
    chunks
        .into_iter()
        .map(|mut chunk| {
            chunk.shift(left, y);
            y += chunk.height;
            chunk
        })
        .collect()
}

/// The boxes of one chain of linked text boxes, by their place in it: the first holds the text
/// (`wps:txbx id`), each later one names the chain and its place (`wps:linkedTxbx id seq`). Each is its
/// `wp:docPr` id, its width and height and its `wps:bodyPr`.
fn linked_boxes(ctx: &Ctx, chain: i64) -> Vec<(usize, i64, f32, f32, El)> {
    let mut found = Vec::new();
    let mut look = |el: &El| {
        el.walk(&mut |node| {
            if node.name != "wp:anchor" && node.name != "wp:inline" {
                return;
            }
            let Some(shape) = crate::read::descendant(node, "wps:wsp") else { return };
            let place = match (shape.child("wps:txbx"), shape.child("wps:linkedTxbx")) {
                (Some(txbx), _)
                    if txbx.attr("id").and_then(|c| c.parse::<i64>().ok()) == Some(chain) =>
                {
                    Some(0)
                }
                (_, Some(link))
                    if link.attr("id").and_then(|c| c.parse::<i64>().ok()) == Some(chain) =>
                {
                    link.attr("seq").and_then(|c| c.parse::<usize>().ok())
                }
                _ => None,
            };
            let Some(place) = place else { return };
            let extent = node.child("wp:extent");
            let size = |name: &str| {
                props::emu(extent.and_then(|e| e.attr(name)).and_then(props::number).unwrap_or(0.0))
            };
            let id = node
                .child("wp:docPr")
                .and_then(|d| d.attr("id"))
                .and_then(|i| i.parse().ok())
                .unwrap_or(0);
            let body = shape.child("wps:bodyPr").cloned().unwrap_or_default();
            if !found.iter().any(|(p, ..): &(usize, i64, f32, f32, El)| *p == place) {
                found.push((place, id, size("cx"), size("cy"), body));
            }
        });
    };
    for (_, paragraph) in crate::position::paragraphs(&ctx.document.body) {
        for run in &paragraph.runs {
            if let crate::model::Content::Object(el) = &run.content {
                look(el);
            }
        }
    }
    found.sort_by_key(|b| b.0);
    found
}

/// The height a text box's text has, inside its top and bottom insets.
fn box_room(body: &El, h: f32) -> f32 {
    let inset = |name: &str, fallback: f64| {
        props::emu(body.attr(name).and_then(props::number).unwrap_or(fallback))
    };
    (h - inset("tIns", 45_720.0) - inset("bIns", 45_720.0)).max(0.0)
}

/// Lines, already placed one under another, shared out among boxes of the given heights: each box takes
/// the whole lines that fit in it, and the next box starts with the first line that did not. The lines
/// a box takes are moved up so its first line is where the first box's first line is.
pub fn split_into_boxes(chunks: Vec<Chunk>, capacities: &[f32]) -> Vec<Vec<Chunk>> {
    let mut boxes: Vec<Vec<Chunk>> = vec![Vec::new(); capacities.len() + 1];
    let mut which = 0;
    let mut used = 0.0;
    let mut placed = 0.0;
    for mut chunk in chunks {
        while which < capacities.len()
            && used + chunk.height > capacities[which] + 0.01
            && used > 0.0
        {
            which += 1;
            used = 0.0;
        }
        // The chunk was put `placed` down the first box's column; in its own box it is `used` down.
        chunk.shift(0.0, used - placed);
        placed += chunk.height;
        used += chunk.height;
        boxes[which].push(chunk);
    }
    boxes
}

/// The lines of a chain of linked text boxes that land in box `seq`, at its insets and anchored at its top.
fn linked_inside(ctx: &Ctx, chain: i64, seq: usize, body: &El, h: f32) -> Vec<Chunk> {
    let boxes = linked_boxes(ctx, chain);
    let Some((_, first_id, first_w, first_h, first_body)) = boxes.first().cloned() else {
        return Vec::new();
    };
    let chunks = text_inside(ctx, first_id, &first_body, first_w, first_h.max(1.0e6));
    let mut capacities: Vec<f32> = Vec::new();
    for (place, _, _, height, body) in &boxes {
        if *place > seq {
            break;
        }
        capacities.push(box_room(body, *height));
    }
    if capacities.len() <= seq {
        capacities.push(box_room(body, h));
    }
    let mut shared = split_into_boxes(chunks, &capacities);
    let index = boxes.iter().position(|b| b.0 == seq).unwrap_or(capacities.len() - 1);
    // A later box's insets differ from the first's only in rare documents; its lines keep the first box's.
    shared.get_mut(index).map(std::mem::take).unwrap_or_default()
}

/// What a drawing draws at `rect`: its fill, its picture or preset shape, its text, its outline, or a
/// placeholder naming it.
pub fn items(float: &Float, rect: Rect) -> Vec<Item> {
    let mut items = Vec::new();
    if let Some((story, path, offset)) = &float.source {
        items.push(Item::Object {
            rect,
            story: story.clone(),
            path: path.clone(),
            offset: *offset,
        });
    }
    match &float.preset {
        Some(preset) if preset != "rect" => items.push(Item::Shape {
            rect,
            preset: preset.clone(),
            fill: float.fill,
            outline: float.outline,
            flip: float.flip,
        }),
        _ => {
            if let Some(color) = float.fill {
                items.push(Item::Fill { rect, color });
            }
        }
    }
    if !float.part.is_empty() {
        items.push(Item::Picture { rect, part: float.part.clone(), crop: float.crop });
    } else if float.inside.is_empty() && float.fill.is_none() && float.outline.is_none() {
        items.push(Item::Placeholder { rect, label: float.label.clone() });
    }
    for chunk in &float.inside {
        let mut chunk = chunk.clone();
        chunk.shift(rect.x, rect.y);
        items.extend(chunk.items);
    }
    let outlined_rect = matches!(float.preset.as_deref(), None | Some("rect"));
    if let (true, Some((color, width))) = (outlined_rect, float.outline) {
        let rule = |x0: f32, y0: f32, x1: f32, y1: f32| Item::Rule {
            x0,
            y0,
            x1,
            y1,
            width,
            color,
            kind: RuleKind::Solid,
        };
        items.push(rule(rect.x, rect.y, rect.x + rect.w, rect.y));
        items.push(rule(rect.x, rect.bottom(), rect.x + rect.w, rect.bottom()));
        items.push(rule(rect.x, rect.y, rect.x, rect.bottom()));
        items.push(rule(rect.x + rect.w, rect.y, rect.x + rect.w, rect.bottom()));
    }
    items
}

/// The outline of a preset shape in `rect`, as points to join, for the painter and the PDF export. A
/// line and a connector are two points; every other shape is a closed polygon.
pub fn outline_points(preset: &str, rect: Rect, flip: (bool, bool)) -> Vec<(f32, f32)> {
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    let point = |fx: f32, fy: f32| {
        let fx = if flip.0 { 1.0 - fx } else { fx };
        let fy = if flip.1 { 1.0 - fy } else { fy };
        (x + w * fx, y + h * fy)
    };
    match preset {
        "line" | "straightConnector1" => vec![point(0.0, 0.0), point(1.0, 1.0)],
        "ellipse" => (0..48)
            .map(|i| {
                let a = i as f32 / 48.0 * std::f32::consts::TAU;
                point(0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin())
            })
            .collect(),
        "roundRect" => {
            let r = 0.1667 * w.min(h);
            let (rx, ry) = (r / w.max(1.0), r / h.max(1.0));
            let mut points = Vec::new();
            for (cx, cy, from) in [
                (1.0 - rx, ry, -90.0f32),
                (1.0 - rx, 1.0 - ry, 0.0),
                (rx, 1.0 - ry, 90.0),
                (rx, ry, 180.0),
            ] {
                for step in 0..=6 {
                    let a = (from + step as f32 * 15.0).to_radians();
                    points.push(point(cx + rx * a.cos(), cy + ry * a.sin()));
                }
            }
            points
        }
        "triangle" => vec![point(0.5, 0.0), point(1.0, 1.0), point(0.0, 1.0)],
        "diamond" => vec![point(0.5, 0.0), point(1.0, 0.5), point(0.5, 1.0), point(0.0, 0.5)],
        "rightArrow" => vec![
            point(0.0, 0.25),
            point(0.5, 0.25),
            point(0.5, 0.0),
            point(1.0, 0.5),
            point(0.5, 1.0),
            point(0.5, 0.75),
            point(0.0, 0.75),
        ],
        "leftArrow" => vec![
            point(1.0, 0.25),
            point(0.5, 0.25),
            point(0.5, 0.0),
            point(0.0, 0.5),
            point(0.5, 1.0),
            point(0.5, 0.75),
            point(1.0, 0.75),
        ],
        "upArrow" => vec![
            point(0.25, 1.0),
            point(0.25, 0.5),
            point(0.0, 0.5),
            point(0.5, 0.0),
            point(1.0, 0.5),
            point(0.75, 0.5),
            point(0.75, 1.0),
        ],
        "downArrow" => vec![
            point(0.25, 0.0),
            point(0.25, 0.5),
            point(0.0, 0.5),
            point(0.5, 1.0),
            point(1.0, 0.5),
            point(0.75, 0.5),
            point(0.75, 0.0),
        ],
        _ => vec![point(0.0, 0.0), point(1.0, 0.0), point(1.0, 1.0), point(0.0, 1.0)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_has_an_outline_inside_its_box() {
        let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
        for preset in PRESETS {
            let points = outline_points(preset, rect, (false, false));
            assert!(points.len() >= 2, "{preset}");
            for (x, y) in points {
                assert!(rect.contains(x, y) || (x - rect.x).abs() < 0.01, "{preset} at {x},{y}");
            }
        }
    }

    #[test]
    fn a_flipped_line_runs_the_other_way() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(outline_points("line", rect, (true, false)), vec![(10.0, 0.0), (0.0, 10.0)]);
    }
}
