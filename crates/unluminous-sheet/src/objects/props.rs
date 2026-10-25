//! Reading what a person can change out of a drawing object's XML, and writing a change back into it.
//!
//! Each property has a reader and a writer. A writer touches only its own property's elements, so
//! everything else in the tree is written back as it was read. [`patch`] calls the writer of a property
//! only when the object's field differs from what the reader finds in the tree.

use super::*;
use crate::xlsx::xmltree::{Elem, Node};

/// The children of `xdr:spPr`, in the order the schema requires.
const SP_PR_ORDER: &[&str] = &[
    "xfrm", "custGeom", "prstGeom", "noFill", "solidFill", "gradFill", "blipFill", "pattFill",
    "grpFill", "ln", "effectLst", "effectDag", "scene3d", "sp3d", "extLst",
];
const FILL_NAMES: &[&str] = &["noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"];
/// The children of `a:ln`, in the order the schema requires.
const LN_ORDER: &[&str] = &[
    "noFill", "solidFill", "gradFill", "pattFill", "prstDash", "custDash", "round", "bevel", "miter",
    "headEnd", "tailEnd", "extLst",
];

/// The `a:` prefix this element's family uses, found from a child that must be in that namespace.
fn drawing_prefix(sp_pr: &Elem) -> String {
    let p = sp_pr.elems().next().map(|e| e.prefix().to_owned()).unwrap_or_default();
    if p.is_empty() || p == "xdr" {
        "a".to_owned()
    } else {
        p
    }
}

fn name(prefix: &str, local: &str) -> String {
    if prefix.is_empty() {
        local.to_owned()
    } else {
        format!("{prefix}:{local}")
    }
}

/// The `spPr` (or `grpSpPr`) of a content element.
pub fn shape_properties(e: &Elem) -> Option<&Elem> {
    e.child("spPr").or_else(|| e.child("grpSpPr"))
}

fn shape_properties_mut(e: &mut Elem) -> Option<&mut Elem> {
    if e.child("spPr").is_some() {
        e.child_mut("spPr")
    } else {
        e.child_mut("grpSpPr")
    }
}

/// The `cNvPr` of a content element.
pub fn non_visual(e: &Elem) -> Option<&Elem> {
    e.elems().find(|c| c.local().starts_with("nv")).and_then(|nv| nv.child("cNvPr"))
}

/// The transform of a content element, as the numbers in it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Xfrm {
    pub off: (i64, i64),
    pub ext: (i64, i64),
    pub ch_off: (i64, i64),
    pub ch_ext: (i64, i64),
    pub rot: i64,
    pub flip_h: bool,
    pub flip_v: bool,
}

fn pair(e: Option<&Elem>, a: &str, b: &str) -> (i64, i64) {
    let get = |n: &str| e.and_then(|e| e.attr(n)).and_then(|v| v.parse().ok()).unwrap_or(0);
    (get(a), get(b))
}

/// The transform an element holds.
pub fn read_xfrm(content: &Elem) -> Xfrm {
    let x = shape_properties(content).and_then(|p| p.child("xfrm"));
    let Some(x) = x else { return Xfrm::default() };
    Xfrm {
        off: pair(x.child("off"), "x", "y"),
        ext: pair(x.child("ext"), "cx", "cy"),
        ch_off: pair(x.child("chOff"), "x", "y"),
        ch_ext: pair(x.child("chExt"), "cx", "cy"),
        rot: x.attr("rot").and_then(|v| v.parse().ok()).unwrap_or(0),
        flip_h: x.attr("flipH") == Some("1"),
        flip_v: x.attr("flipV") == Some("1"),
    }
}

/// Write a transform into an element, leaving the child space of a group as it is when `child_space` is
/// `None`.
pub fn write_xfrm(content: &mut Elem, t: &Xfrm, child_space: Option<((i64, i64), (i64, i64))>) {
    let Some(sp_pr) = shape_properties_mut(content) else { return };
    let prefix = drawing_prefix(sp_pr);
    if sp_pr.child("xfrm").is_none() {
        sp_pr.put_in_order(Elem::new(&name(&prefix, "xfrm")), SP_PR_ORDER);
    }
    let Some(x) = sp_pr.child_mut("xfrm") else { return };
    set_flag(x, "rot", if t.rot == 0 { None } else { Some(t.rot.to_string()) });
    set_flag(x, "flipH", t.flip_h.then(|| "1".to_owned()));
    set_flag(x, "flipV", t.flip_v.then(|| "1".to_owned()));
    set_pair(x, &prefix, "off", ("x", "y"), t.off);
    set_pair(x, &prefix, "ext", ("cx", "cy"), t.ext);
    if let Some((off, ext)) = child_space {
        set_pair(x, &prefix, "chOff", ("x", "y"), off);
        set_pair(x, &prefix, "chExt", ("cx", "cy"), ext);
    }
}

fn set_flag(e: &mut Elem, attr: &str, value: Option<String>) {
    match value {
        Some(v) => e.set_attr(attr, &v),
        None => e.remove_attr(attr),
    }
}

/// Set a child with two integer attributes, creating it after the children the schema puts before it.
fn set_pair(parent: &mut Elem, prefix: &str, local: &str, attrs: (&str, &str), v: (i64, i64)) {
    const XFRM_ORDER: &[&str] = &["off", "ext", "chOff", "chExt"];
    if parent.child(local).is_none() {
        parent.put_in_order(Elem::new(&name(prefix, local)), XFRM_ORDER);
    }
    if let Some(e) = parent.child_mut(local) {
        e.set_attr(attrs.0, &v.0.to_string());
        e.set_attr(attrs.1, &v.1.to_string());
    }
}

// ---------------------------------------------------------------------------------------------- colours

fn mod_value(e: &Elem) -> i64 {
    e.attr("val").and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// A colour from its element (`srgbClr`, `schemeClr`, `sysClr`, `prstClr`, `scrgbClr`).
pub fn read_color(e: &Elem) -> Option<ShapeColor> {
    let hex = |v: &str| {
        let n = u32::from_str_radix(v, 16).ok()?;
        Some(ColorBase::Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
    };
    let base = match e.local() {
        "srgbClr" => hex(e.attr("val")?)?,
        "schemeClr" => ColorBase::Scheme(e.attr("val")?.to_owned()),
        "sysClr" => match e.attr("lastClr") {
            Some(v) => hex(v)?,
            None => ColorBase::Rgb(0, 0, 0),
        },
        "prstClr" => match e.attr("val")? {
            "white" => ColorBase::Rgb(255, 255, 255),
            "red" => ColorBase::Rgb(255, 0, 0),
            "green" => ColorBase::Rgb(0, 128, 0),
            "blue" => ColorBase::Rgb(0, 0, 255),
            "yellow" => ColorBase::Rgb(255, 255, 0),
            "gray" | "grey" => ColorBase::Rgb(128, 128, 128),
            _ => ColorBase::Rgb(0, 0, 0),
        },
        "scrgbClr" => {
            let part = |n: &str| {
                let v = e.attr(n).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / 100_000.0;
                (v.clamp(0.0, 1.0) * 255.0).round() as u8
            };
            ColorBase::Rgb(part("r"), part("g"), part("b"))
        }
        _ => return None,
    };
    let mut color = ShapeColor { base, alpha: 100_000, mods: Vec::new() };
    for m in e.elems() {
        match m.local() {
            "alpha" => color.alpha = mod_value(m).clamp(0, 100_000) as u32,
            other => color.mods.push((other.to_owned(), mod_value(m))),
        }
    }
    Some(color)
}

/// The element for a colour.
pub fn color_elem(prefix: &str, c: &ShapeColor) -> Elem {
    let mut e = match &c.base {
        ColorBase::Rgb(r, g, b) => {
            let mut e = Elem::new(&name(prefix, "srgbClr"));
            e.set_attr("val", &format!("{r:02X}{g:02X}{b:02X}"));
            e
        }
        ColorBase::Scheme(s) => {
            let mut e = Elem::new(&name(prefix, "schemeClr"));
            e.set_attr("val", s);
            e
        }
    };
    for (m, v) in &c.mods {
        let mut child = Elem::new(&name(prefix, m));
        child.set_attr("val", &v.to_string());
        e.push(child);
    }
    if c.alpha != 100_000 {
        let mut child = Elem::new(&name(prefix, "alpha"));
        child.set_attr("val", &c.alpha.to_string());
        e.push(child);
    }
    e
}

/// The colour in a `solidFill`-like element: the first colour child.
fn color_in(fill: &Elem) -> Option<ShapeColor> {
    fill.elems().find_map(read_color)
}

// ---------------------------------------------------------------------------------------------- shapes

fn read_fill(sp_pr: &Elem) -> Fill {
    for e in sp_pr.elems() {
        match e.local() {
            "noFill" => return Fill::None,
            "solidFill" => return color_in(e).map_or(Fill::Other, Fill::Solid),
            "gradFill" | "blipFill" | "pattFill" | "grpFill" => return Fill::Other,
            _ => {}
        }
    }
    Fill::Inherit
}

fn read_line(sp_pr: &Elem) -> Line {
    let Some(ln) = sp_pr.child("ln") else { return Line::default() };
    let mut line = Line { width: ln.attr("w").and_then(|v| v.parse().ok()), ..Line::default() };
    for e in ln.elems() {
        match e.local() {
            "noFill" => line.stroke = Stroke::None,
            "solidFill" => {
                if let Some(c) = color_in(e) {
                    line.stroke = Stroke::Solid(c);
                }
            }
            "prstDash" => line.dash = Dash::parse(e.attr("val").unwrap_or("solid")),
            "headEnd" => line.head = LineEnd::parse(e.attr("type").unwrap_or("none")),
            "tailEnd" => line.tail = LineEnd::parse(e.attr("type").unwrap_or("none")),
            _ => {}
        }
    }
    line
}

/// The colour a `style` reference names, such as the `fillRef` of a shape Excel inserted.
fn style_color(style: Option<&Elem>, reference: &str) -> Option<ShapeColor> {
    let r = style?.child(reference)?;
    // An index of 0 means no fill or line comes from the theme.
    if r.attr("idx") == Some("0") {
        return None;
    }
    r.elems().find_map(read_color)
}

fn read_runs(p: &Elem) -> Vec<TextRun> {
    let mut runs = Vec::new();
    for r in p.elems() {
        match r.local() {
            "r" => {
                let mut run = TextRun::plain(&r.child("t").map(Elem::text).unwrap_or_default());
                if let Some(pr) = r.child("rPr") {
                    run.size = pr.attr("sz").and_then(|v| v.parse::<f64>().ok()).map(|v| v / 100.0);
                    run.bold = pr.attr("b") == Some("1");
                    run.italic = pr.attr("i") == Some("1");
                    run.underline = pr.attr("u").is_some_and(|u| u != "none");
                    run.color = pr.child("solidFill").and_then(color_in);
                    run.font = pr.child("latin").and_then(|l| l.attr("typeface")).map(str::to_owned);
                }
                runs.push(run);
            }
            "br" => runs.push(TextRun::plain("\n")),
            "fld" => runs.push(TextRun::plain(&r.child("t").map(Elem::text).unwrap_or_default())),
            _ => {}
        }
    }
    runs
}

fn read_text(content: &Elem) -> Option<TextBody> {
    let body = content.child("txBody")?;
    let props = body.child("bodyPr");
    let mut text = TextBody {
        paragraphs: Vec::new(),
        valign: VAlign::parse(props.and_then(|b| b.attr("anchor")).unwrap_or("t")),
        wrap: props.and_then(|b| b.attr("wrap")) != Some("none"),
    };
    for p in body.elems().filter(|e| e.local() == "p") {
        let align = p.child("pPr").and_then(|pp| pp.attr("algn")).map_or(HAlign::Left, HAlign::parse);
        text.paragraphs.push(TextParagraph { align, runs: read_runs(p) });
    }
    Some(text)
}

/// What a `sp` or `cxnSp` element says about the shape.
pub fn read_shape(content: &Elem) -> Option<ShapeData> {
    let sp_pr = content.child("spPr")?;
    let geometry = sp_pr.child("prstGeom");
    let preset = geometry.and_then(|g| g.attr("prst")).unwrap_or("rect").to_owned();
    let adjust = geometry
        .and_then(|g| g.child("avLst"))
        .and_then(|l| l.elems().next())
        .and_then(|gd| gd.attr("fmla"))
        .and_then(|f| f.strip_prefix("val "))
        .and_then(|v| v.trim().parse().ok());
    let style = content.child("style");
    Some(ShapeData {
        preset,
        adjust,
        fill: read_fill(sp_pr),
        line: read_line(sp_pr),
        text: read_text(content),
        style_fill: style_color(style, "fillRef"),
        style_line: style_color(style, "lnRef"),
        style_text: style.and_then(|s| s.child("fontRef")).and_then(|f| f.elems().find_map(read_color)),
    })
}

fn write_fill(sp_pr: &mut Elem, prefix: &str, fill: &Fill) {
    match fill {
        Fill::Other => {}
        Fill::Inherit => {
            for n in FILL_NAMES {
                sp_pr.remove_children(n);
            }
        }
        Fill::None => {
            for n in FILL_NAMES {
                sp_pr.remove_children(n);
            }
            sp_pr.put_in_order(Elem::new(&name(prefix, "noFill")), SP_PR_ORDER);
        }
        Fill::Solid(c) => {
            for n in FILL_NAMES {
                sp_pr.remove_children(n);
            }
            let mut solid = Elem::new(&name(prefix, "solidFill"));
            solid.push(color_elem(prefix, c));
            sp_pr.put_in_order(solid, SP_PR_ORDER);
        }
    }
}

fn write_line(sp_pr: &mut Elem, prefix: &str, line: &Line) {
    if sp_pr.child("ln").is_none() {
        sp_pr.put_in_order(Elem::new(&name(prefix, "ln")), SP_PR_ORDER);
    }
    let Some(ln) = sp_pr.child_mut("ln") else { return };
    set_flag(ln, "w", line.width.map(|w| w.to_string()));
    for n in ["noFill", "solidFill", "prstDash", "headEnd", "tailEnd"] {
        ln.remove_children(n);
    }
    match &line.stroke {
        Stroke::Inherit => {}
        Stroke::None => ln.put_in_order(Elem::new(&name(prefix, "noFill")), LN_ORDER),
        Stroke::Solid(c) => {
            let mut solid = Elem::new(&name(prefix, "solidFill"));
            solid.push(color_elem(prefix, c));
            ln.put_in_order(solid, LN_ORDER);
        }
    }
    if line.dash != Dash::Solid {
        let mut d = Elem::new(&name(prefix, "prstDash"));
        d.set_attr("val", line.dash.attr());
        ln.put_in_order(d, LN_ORDER);
    }
    for (local, end) in [("headEnd", line.head), ("tailEnd", line.tail)] {
        if end != LineEnd::None {
            let mut e = Elem::new(&name(prefix, local));
            e.set_attr("type", end.attr());
            ln.put_in_order(e, LN_ORDER);
        }
    }
}

fn write_geometry(sp_pr: &mut Elem, prefix: &str, shape: &ShapeData) {
    if sp_pr.child("prstGeom").is_none() {
        sp_pr.put_in_order(Elem::new(&name(prefix, "prstGeom")), SP_PR_ORDER);
    }
    let Some(g) = sp_pr.child_mut("prstGeom") else { return };
    g.set_attr("prst", &shape.preset);
    let mut list = Elem::new(&name(prefix, "avLst"));
    if let Some(v) = shape.adjust {
        let mut gd = Elem::new(&name(prefix, "gd"));
        gd.set_attr("name", "adj");
        gd.set_attr("fmla", &format!("val {v}"));
        list.push(gd);
    }
    g.children = vec![Node::Elem(list)];
}

fn run_elem(prefix: &str, run: &TextRun) -> Elem {
    let mut r = Elem::new(&name(prefix, "r"));
    let mut pr = Elem::new(&name(prefix, "rPr"));
    pr.set_attr("lang", "en-US");
    if let Some(s) = run.size {
        pr.set_attr("sz", &((s * 100.0).round() as i64).to_string());
    }
    if run.bold {
        pr.set_attr("b", "1");
    }
    if run.italic {
        pr.set_attr("i", "1");
    }
    if run.underline {
        pr.set_attr("u", "sng");
    }
    if let Some(c) = &run.color {
        let mut f = Elem::new(&name(prefix, "solidFill"));
        f.push(color_elem(prefix, c));
        pr.push(f);
    }
    if let Some(font) = &run.font {
        let mut l = Elem::new(&name(prefix, "latin"));
        l.set_attr("typeface", font);
        pr.push(l);
    }
    r.push(pr);
    let mut t = Elem::new(&name(prefix, "t"));
    t.set_text(&run.text);
    if run.text.starts_with(' ') || run.text.ends_with(' ') {
        t.set_attr("xml:space", "preserve");
    }
    r.push(t);
    r
}

fn write_text(content: &mut Elem, xdr: &str, a: &str, text: &TextBody) {
    if content.child("txBody").is_none() {
        let mut body = Elem::new(&name(xdr, "txBody"));
        body.push(Elem::new(&name(a, "bodyPr")));
        body.push(Elem::new(&name(a, "lstStyle")));
        content.push(body);
    }
    let Some(body) = content.child_mut("txBody") else { return };
    if let Some(props) = body.child_mut("bodyPr") {
        props.set_attr("anchor", text.valign.attr());
        set_flag(props, "wrap", (!text.wrap).then(|| "none".to_owned()));
    }
    body.remove_children("p");
    for p in &text.paragraphs {
        let mut pe = Elem::new(&name(a, "p"));
        let mut ppr = Elem::new(&name(a, "pPr"));
        ppr.set_attr("algn", p.align.attr());
        pe.push(ppr);
        for run in &p.runs {
            pe.push(run_elem(a, run));
        }
        body.push(pe);
    }
}

/// Write the fields of a shape that differ from what its tree says.
pub fn patch_shape(content: &mut Elem, shape: &ShapeData) {
    let Some(before) = read_shape(content) else { return };
    let (xdr, a) = {
        let xdr = content.prefix().to_owned();
        let a = content.child("spPr").map(drawing_prefix).unwrap_or_else(|| "a".to_owned());
        (xdr, a)
    };
    if before.preset != shape.preset || before.adjust != shape.adjust {
        if let Some(sp_pr) = content.child_mut("spPr") {
            write_geometry(sp_pr, &a, shape);
        }
    }
    if before.fill != shape.fill {
        if let Some(sp_pr) = content.child_mut("spPr") {
            write_fill(sp_pr, &a, &shape.fill);
        }
    }
    if before.line != shape.line {
        if let Some(sp_pr) = content.child_mut("spPr") {
            write_line(sp_pr, &a, &shape.line);
        }
    }
    if before.text != shape.text {
        if let Some(text) = &shape.text {
            write_text(content, &xdr, &a, text);
        }
    }
}

// ---------------------------------------------------------------------------------------------- pictures

/// The relationship id of a picture's image and its crop.
pub fn read_picture(content: &Elem) -> Option<(String, Crop, Option<String>)> {
    let blip_fill = content.child("blipFill")?;
    let blip = blip_fill.child("blip")?;
    let embed = blip.attr_local("embed")?.to_owned();
    let crop = blip_fill.child("srcRect").map_or(Crop::default(), |r| {
        let get = |n: &str| r.attr(n).and_then(|v| v.parse().ok()).unwrap_or(0);
        Crop { left: get("l"), top: get("t"), right: get("r"), bottom: get("b") }
    });
    let svg = blip
        .find("svgBlip")
        .and_then(|s| s.attr_local("embed"))
        .map(str::to_owned);
    Some((embed, crop, svg))
}

/// Write a picture's crop and the relationship ids of its image and SVG.
pub fn patch_picture(content: &mut Elem, crop: Crop, embed: &str, svg: Option<&str>) {
    let Some(blip_fill) = content.child_mut("blipFill") else { return };
    let a = blip_fill.elems().next().map(|e| e.prefix().to_owned()).unwrap_or_else(|| "a".to_owned());
    if let Some(blip) = blip_fill.child_mut("blip") {
        if let Some((n, _)) = blip.attrs.iter().find(|(n, _)| super::super::xlsx::xmltree::local_name(n) == "embed").cloned() {
            blip.set_attr(&n, embed);
        }
        if let (Some(id), Some(s)) = (svg, blip.find_mut("svgBlip")) {
            if let Some((n, _)) = s.attrs.iter().find(|(n, _)| super::super::xlsx::xmltree::local_name(n) == "embed").cloned() {
                s.set_attr(&n, id);
            }
        }
    }
    blip_fill.remove_children("srcRect");
    if !crop.is_none() {
        let mut r = Elem::new(&name(&a, "srcRect"));
        for (n, v) in [("l", crop.left), ("t", crop.top), ("r", crop.right), ("b", crop.bottom)] {
            if v != 0 {
                r.set_attr(n, &v.to_string());
            }
        }
        let at = blip_fill
            .children
            .iter()
            .position(|c| matches!(c, Node::Elem(e) if e.local() == "blip"))
            .map_or(0, |i| i + 1);
        blip_fill.children.insert(at, Node::Elem(r));
    }
}

/// The ids and names every object in a tree carries: `cNvPr` of the content and of every child.
pub fn set_identity(content: &mut Elem, id: ObjectId, obj_name: &str, descr: &str, hidden: bool) {
    let Some(nv) = content.elems_mut().find(|c| c.local().starts_with("nv")) else { return };
    let Some(c) = nv.child_mut("cNvPr") else { return };
    c.set_attr("id", &id.to_string());
    c.set_attr("name", obj_name);
    set_flag(c, "descr", (!descr.is_empty()).then(|| descr.to_owned()));
    set_flag(c, "hidden", hidden.then(|| "1".to_owned()));
}

/// The aspect lock of a picture, shape or group.
pub fn read_lock(content: &Elem) -> bool {
    let nv = content.elems().find(|c| c.local().starts_with("nv"));
    let locks = nv.and_then(|n| {
        n.elems().find(|c| c.local().starts_with("cNv") && c.local() != "cNvPr").and_then(|c| {
            c.elems().find(|l| l.local().ends_with("Locks"))
        })
    });
    locks.is_some_and(|l| l.attr("noChangeAspect") == Some("1"))
}

/// Write the aspect lock into the `Locks` element of the object, making it when needed.
pub fn write_lock(content: &mut Elem, locked: bool) {
    let a = content.child("spPr").map(drawing_prefix).unwrap_or_else(|| "a".to_owned());
    let Some(nv) = content.elems_mut().find(|c| c.local().starts_with("nv")) else { return };
    let Some(cnv) = nv.elems_mut().find(|c| c.local().starts_with("cNv") && c.local() != "cNvPr") else { return };
    let locks_name = match cnv.local() {
        "cNvPicPr" => "picLocks",
        "cNvGrpSpPr" => "grpSpLocks",
        "cNvCxnSpPr" => "cxnSpLocks",
        _ => "spLocks",
    };
    if cnv.child(locks_name).is_none() {
        if !locked {
            return;
        }
        cnv.push(Elem::new(&name(&a, locks_name)));
    }
    if let Some(l) = cnv.child_mut(locks_name) {
        set_flag(l, "noChangeAspect", locked.then(|| "1".to_owned()));
    }
}

/// Apply every field of an object that differs from its tree to the tree.
pub fn patch(obj: &mut DrawingObject, embed_for: &mut dyn FnMut(&str) -> String) {
    let (id, nm, descr, hidden) = (obj.id, obj.name.clone(), obj.descr.clone(), obj.hidden);
    let mut tree = std::mem::take(&mut obj.tree);
    set_identity(&mut tree, id, &nm, &descr, hidden);
    if read_lock(&tree) != obj.lock_aspect {
        write_lock(&mut tree, obj.lock_aspect);
    }
    if let Some(shape) = &obj.shape {
        patch_shape(&mut tree, shape);
    }
    if let Some(p) = &obj.picture {
        let embed = embed_for(&p.part);
        let svg = p.svg.as_deref().map(&mut *embed_for);
        patch_picture(&mut tree, p.crop, &embed, svg.as_deref());
    }
    obj.tree = tree;
}
