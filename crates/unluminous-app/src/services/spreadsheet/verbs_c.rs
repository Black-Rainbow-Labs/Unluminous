//! The verbs for pictures, shapes, text boxes and every object on a sheet (`task-2360`, the `wiringc` lane).
//! [`super::verbs::run`] hands any verb it does not know to [`run`] here. A pointer drag, a menu entry and an
//! agent's call all end in these words, so they change the workbook the same way. Sizes and places are in
//! points from the top left corner of the sheet, as Excel's Format Shape pane shows them.

use serde_json::{json, Value};
use unluminous_sheet::edit::{Alignment, Distribution, TextFormat, ZOrder};
use unluminous_sheet::model::Sheet;
use unluminous_sheet::objects::make::NewShape;
use unluminous_sheet::objects::{
    Crop, Dash, DrawingObject, Fill, HAlign, LineEnd, ObjectId, Placement, Rect, ShapeColor,
    Stroke, VAlign, EMU_PER_POINT,
};
use unluminous_sheet::styles::Color;

use super::verbs_a::{cell_or_active, edit, flag, Out};
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// Run a verb this module knows, or answer `None` for one it does not.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    match verb {
        "objects" => Some(list(sheet, args)),
        "object" => Some(object(sheet, args)),
        "picture" => Some(picture(sheet, args)),
        "shape" => Some(shape(sheet, args)),
        _ => super::verbs_c_spark::run(sheet, verb, args),
    }
}

// ------------------------------------------------------------------------------------------------ helpers

/// A number of points as drawing units.
pub fn emu(points: f64) -> i64 {
    (points * EMU_PER_POINT as f64).round() as i64
}

/// A drawing length as points, to two places.
pub fn pt(emu: i64) -> f64 {
    (emu as f64 / EMU_PER_POINT as f64 * 100.0).round() / 100.0
}

/// A number from a word.
fn number(word: Option<&String>, what: &str) -> Result<f64, String> {
    let w = word.ok_or_else(|| format!("{what} needs a number"))?;
    w.parse::<f64>().map_err(|_| format!("`{w}` is not a number for {what}"))
}

/// A colour word as a shape colour.
fn shape_color(word: &str) -> Result<ShapeColor, String> {
    match Color::parse_hex(word) {
        Some(Color::Rgb(v)) => Ok(ShapeColor::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)),
        _ => Err(format!("`{word}` is a colour written #RRGGBB")),
    }
}

/// The objects a verb is about: the leading words that are numbers or object names, or the chosen objects.
fn take_ids<'a>(book: &Book, args: &'a [String]) -> (Vec<ObjectId>, &'a [String]) {
    let s = &book.session.book.sheets[book.view.sheet];
    let mut ids = Vec::new();
    for word in args {
        let found = word
            .parse::<ObjectId>()
            .ok()
            .filter(|id| s.drawing.find(*id).is_some())
            .or_else(|| s.drawing.objects.iter().find(|o| &o.name == word).map(|o| o.id));
        match found {
            Some(id) => ids.push(id),
            None => break,
        }
    }
    let used = ids.len();
    if ids.is_empty() {
        ids = book.view.objects.selected.clone();
    }
    (ids, &args[used..])
}

/// The ids, refused when there are none.
fn need_ids(book: &Book, args: &[String]) -> Result<(Vec<ObjectId>, Vec<String>), String> {
    let (ids, rest) = take_ids(book, args);
    if ids.is_empty() {
        return Err("Choose an object first, or give its number".to_owned());
    }
    Ok((ids, rest.to_vec()))
}

/// Run one engine change on each object, each an undo step of its own.
fn each(
    sheet: &mut Spreadsheet,
    args: &[String],
    say: &str,
    mut change: impl FnMut(&mut Book, usize, ObjectId, &[String]) -> Result<(), String>,
) -> Out {
    let (ids, rest) = need_ids(sheet.current_book()?, args)?;
    let say = say.to_owned();
    edit(sheet, move |book| {
        let at = book.view.sheet;
        for id in &ids {
            change(book, at, *id, &rest)?;
        }
        Ok(format!("{say} ({})", ids.len()))
    })
}

// ------------------------------------------------------------------------------------------------ reading

/// The text a shape holds, paragraphs joined by a line break.
fn text_of(o: &DrawingObject) -> String {
    let Some(body) = o.shape.as_ref().and_then(|s| s.text.as_ref()) else { return String::new() };
    let lines: Vec<String> =
        body.paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect()).collect();
    lines.join("\n")
}

/// The word for how an object follows the cells.
fn placement_word(p: Placement) -> &'static str {
    match p {
        Placement::MoveAndSize => "move-size",
        Placement::Move => "move",
        Placement::Fixed => "fixed",
    }
}

/// One object as data: its place and size in points, its kind and what a person can set on it.
pub fn describe(o: &DrawingObject, sheet: &Sheet, z: usize, chosen: bool) -> Value {
    let f = o.frame_rect(sheet);
    let crop = o.picture.as_ref().map(|p| p.crop);
    json!({
        "id": o.id, "name": o.name, "kind": o.kind.label(), "z": z, "chosen": chosen,
        "left": pt(f.x), "top": pt(f.y), "width": pt(f.w), "height": pt(f.h),
        "rotation": o.rotation, "flipH": o.flip_h, "flipV": o.flip_v,
        "alt": o.descr, "hidden": o.hidden, "lockAspect": o.lock_aspect,
        "placement": placement_word(o.anchor.placement),
        "text": text_of(o), "preset": o.shape.as_ref().map(|s| s.preset.clone()),
        "crop": crop.map(|c| json!({"left": c.left as f64 / 1000.0, "top": c.top as f64 / 1000.0,
            "right": c.right as f64 / 1000.0, "bottom": c.bottom as f64 / 1000.0})),
        "members": o.children.len(),
    })
}

/// `objects [selected]`: every object of the sheet showing, bottom first.
fn list(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let s = &book.session.book.sheets[book.view.sheet];
    let only = flag(args, "selected");
    let rows: Vec<Value> = s
        .drawing
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| !only || book.view.objects.has(o.id))
        .map(|(z, o)| {
            let mut row = describe(o, s, z, book.view.objects.has(o.id));
            let at = book.view.objects.shown.iter().find(|(id, _)| *id == o.id);
            row["screen"] = at.map_or(
                Value::Null,
                |(_, r)| json!({"x": r[0], "y": r[1], "width": r[2], "height": r[3]}),
            );
            row
        })
        .collect();
    Ok(Answer::said(format!("{} objects", rows.len())).with(json!(rows)))
}

// ------------------------------------------------------------------------------------------------ object

const OBJECT_WORDS: &str =
    "object takes select, delete, move, frame, resize, rotate, flip, alt, name, \
    placement, lock-aspect, order, align, distribute, group, ungroup, fill, outline, text, \
    text-format, transparency, crop or reset";

/// `object <what> ...`: choose, move, size, turn, format, arrange or delete the objects.
fn object(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let Some(what) = args.first() else { return Err(OBJECT_WORDS.to_owned()) };
    let rest = &args[1..];
    match what.as_str() {
        "select" => select(sheet, rest),
        "delete" => each(sheet, rest, "Deleted", |b, at, id, _| {
            b.view.objects.selected.retain(|s| *s != id);
            b.session.delete_object(at, id)
        }),
        "move" | "nudge" => each(sheet, rest, "Moved", |b, at, id, w| {
            let dx = emu(number(w.first(), "the horizontal move")?);
            let dy = emu(number(w.get(1), "the vertical move")?);
            b.session.move_object(at, id, dx, dy)
        }),
        "frame" => each(sheet, rest, "Placed", set_frame),
        "resize" => each(sheet, rest, "Resized", |b, at, id, w| {
            let width = emu(number(w.first(), "width")?);
            let height = emu(number(w.get(1), "height")?);
            b.session.resize_object(at, id, width, height, flag(w, "keep"))
        }),
        "rotate" => each(sheet, rest, "Rotated", |b, at, id, w| {
            b.session.rotate_object(at, id, number(w.first(), "rotate")?)
        }),
        "flip" => each(sheet, rest, "Flipped", |b, at, id, w| {
            let horizontal = !matches!(w.first().map(String::as_str), Some("v") | Some("vertical"));
            b.session.flip_object(at, id, horizontal)
        }),
        "alt" => each(sheet, rest, "Alt text set", |b, at, id, w| {
            b.session.set_alt_text(at, id, &w.join(" "))
        }),
        "name" => each(sheet, rest, "Named", |b, at, id, w| {
            b.session.set_object_name(at, id, &w.join(" "))
        }),
        "placement" => each(sheet, rest, "Placement set", set_placement),
        "lock-aspect" => each(sheet, rest, "Aspect lock set", |b, at, id, w| {
            let on = super::verbs_a::on_off(w.first().map(String::as_str))?;
            b.session.set_aspect_lock(at, id, on)
        }),
        "order" => each(sheet, rest, "Arranged", set_order),
        "align" => align(sheet, rest),
        "distribute" => distribute(sheet, rest),
        "group" => group(sheet, rest),
        "ungroup" => ungroup(sheet, rest),
        "fill" => each(sheet, rest, "Filled", set_fill),
        "transparency" => each(sheet, rest, "Transparency set", |b, at, id, w| {
            b.session.set_shape_transparency(at, id, number(w.first(), "transparency")?)
        }),
        "outline" => each(sheet, rest, "Outlined", set_outline),
        "text" => each(sheet, rest, "Text set", |b, at, id, w| {
            b.session.set_shape_text(at, id, &w.join(" "))
        }),
        "text-format" => each(sheet, rest, "Text formatted", format_text),
        "crop" => each(sheet, rest, "Cropped", set_crop),
        "reset" => each(sheet, rest, "Reset", |b, at, id, w| {
            b.session.reset_picture(at, id, flag(w, "size"))
        }),
        other => Err(format!("{OBJECT_WORDS}; got `{other}`")),
    }
}

/// `object select none|all|[add] <id or name>...`: choose objects.
fn select(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let s = &book.session.book.sheets[book.view.sheet];
    let add = flag(args, "add");
    let words: Vec<&String> = args.iter().filter(|w| !w.eq_ignore_ascii_case("add")).collect();
    let chosen: Vec<ObjectId> = match words.first().map(|w| w.as_str()) {
        Some("none") | None => Vec::new(),
        Some("all") => s.drawing.objects.iter().filter(|o| !o.hidden).map(|o| o.id).collect(),
        _ => {
            let mut found = Vec::new();
            for w in &words {
                let o = w
                    .parse::<ObjectId>()
                    .ok()
                    .and_then(|id| s.drawing.find(id))
                    .or_else(|| s.drawing.objects.iter().find(|o| &&o.name == w))
                    .ok_or_else(|| format!("There is no object `{w}` on this sheet"))?;
                found.push(o.id);
            }
            found
        }
    };
    let state = &mut book.view.objects;
    if !add {
        state.selected.clear();
    }
    for id in chosen {
        if !state.has(id) {
            state.selected.push(id);
        }
    }
    book.view.chart = None;
    Ok(Answer::said(format!("{} objects chosen", book.view.objects.selected.len()))
        .with(json!(book.view.objects.selected)))
}

/// `object frame <left> <top> <width> <height>` in points.
fn set_frame(b: &mut Book, at: usize, id: ObjectId, w: &[String]) -> Result<(), String> {
    let n = |i: usize, what: &str| number(w.get(i), what);
    let rect =
        Rect::new(emu(n(0, "left")?), emu(n(1, "top")?), emu(n(2, "width")?), emu(n(3, "height")?));
    b.session.set_object_frame(at, id, rect)
}

/// `object placement move-size|move|fixed`.
fn set_placement(b: &mut Book, at: usize, id: ObjectId, w: &[String]) -> Result<(), String> {
    let p = match w.first().map(String::as_str) {
        Some("move-size") | Some("move-and-size") => Placement::MoveAndSize,
        Some("move") => Placement::Move,
        Some("fixed") | Some("free") => Placement::Fixed,
        _ => return Err("placement is move-size, move or fixed".to_owned()),
    };
    b.session.set_placement(at, id, p)
}

/// `object order front|forward|backward|back`.
fn set_order(b: &mut Book, at: usize, id: ObjectId, w: &[String]) -> Result<(), String> {
    let order = match w.first().map(String::as_str) {
        Some("front") => ZOrder::ToFront,
        Some("forward") => ZOrder::Forward,
        Some("backward") => ZOrder::Backward,
        Some("back") => ZOrder::ToBack,
        _ => return Err("order is front, forward, backward or back".to_owned()),
    };
    b.session.reorder_object(at, id, order)
}

/// `object align left|centre|right|top|middle|bottom [ids]`.
fn align(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let how = match args.first().map(String::as_str) {
        Some("left") => Alignment::Left,
        Some("centre") | Some("center") => Alignment::Centre,
        Some("right") => Alignment::Right,
        Some("top") => Alignment::Top,
        Some("middle") => Alignment::Middle,
        Some("bottom") => Alignment::Bottom,
        _ => return Err("align is left, centre, right, top, middle or bottom".to_owned()),
    };
    let (ids, _) = need_ids(sheet.current_book()?, &args[1..])?;
    edit(sheet, move |b| {
        b.session.align_objects(b.view.sheet, &ids, how)?;
        Ok("Aligned".to_owned())
    })
}

/// `object distribute horizontal|vertical [ids]`.
fn distribute(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let how = match args.first().map(String::as_str) {
        Some("horizontal") | Some("horizontally") => Distribution::Horizontally,
        Some("vertical") | Some("vertically") => Distribution::Vertically,
        _ => return Err("distribute is horizontal or vertical".to_owned()),
    };
    let (ids, _) = need_ids(sheet.current_book()?, &args[1..])?;
    edit(sheet, move |b| {
        b.session.distribute_objects(b.view.sheet, &ids, how)?;
        Ok("Distributed".to_owned())
    })
}

/// `object group [ids]`: the group is chosen.
fn group(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let (ids, _) = need_ids(sheet.current_book()?, args)?;
    edit(sheet, move |b| {
        let id = b.session.group_objects(b.view.sheet, &ids)?;
        b.view.objects.choose(Some(id));
        Ok(format!("Grouped as {id}"))
    })
}

/// `object ungroup [id]`: the members are chosen.
fn ungroup(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let (ids, _) = need_ids(sheet.current_book()?, args)?;
    edit(sheet, move |b| {
        let mut all = Vec::new();
        for id in ids {
            all.extend(b.session.ungroup_object(b.view.sheet, id)?);
        }
        b.view.objects.selected = all;
        Ok("Ungrouped".to_owned())
    })
}

/// `object fill <#RRGGBB|none|inherit> [transparency]`.
fn set_fill(b: &mut Book, at: usize, id: ObjectId, w: &[String]) -> Result<(), String> {
    let word = w.first().ok_or("fill needs #RRGGBB, none or inherit")?;
    let fill = match word.as_str() {
        "none" => Fill::None,
        "inherit" => Fill::Inherit,
        hex => {
            let mut c = shape_color(hex)?;
            if let Some(t) = w.get(1) {
                c = c.with_transparency(number(Some(t), "transparency")?);
            }
            Fill::Solid(c)
        }
    };
    b.session.set_shape_fill(at, id, fill)
}

/// `object outline <#RRGGBB|none|inherit> [width <pt>] [dash <style>] [head <end>] [tail <end>]`.
fn set_outline(b: &mut Book, at: usize, id: ObjectId, w: &[String]) -> Result<(), String> {
    let mut line = b.session.book.sheets[at]
        .drawing
        .find(id)
        .and_then(|o| o.shape.as_ref())
        .map(|s| s.line.clone())
        .ok_or("Only a shape has an outline")?;
    let keyword = |f: &str| matches!(f, "width" | "dash" | "head" | "tail");
    if let Some(first) = w.first().filter(|f| !keyword(f)) {
        line.stroke = match first.as_str() {
            "none" => Stroke::None,
            "inherit" => Stroke::Inherit,
            hex => Stroke::Solid(shape_color(hex)?),
        };
    }
    for pair in w.windows(2) {
        match pair[0].as_str() {
            "width" => line.width = Some(emu(number(Some(&pair[1]), "width")?)),
            "dash" => line.dash = Dash::parse(&pair[1]),
            "head" => line.head = LineEnd::parse(&pair[1]),
            "tail" => line.tail = LineEnd::parse(&pair[1]),
            _ => {}
        }
    }
    b.session.set_shape_line(at, id, line)
}

/// `object text-format size <n> bold on italic on underline on color #RRGGBB font <name> align <a> valign <v>`.
fn format_text(b: &mut Book, at: usize, id: ObjectId, w: &[String]) -> Result<(), String> {
    let mut f = TextFormat::default();
    let on = |v: &String| v == "on" || v == "true";
    for pair in w.windows(2) {
        let v = &pair[1];
        match pair[0].as_str() {
            "size" => f.size = Some(number(Some(v), "size")?),
            "bold" => f.bold = Some(on(v)),
            "italic" => f.italic = Some(on(v)),
            "underline" => f.underline = Some(on(v)),
            "color" => f.color = Some(shape_color(v)?),
            "font" => f.font = Some(v.clone()),
            "align" => f.align = Some(halign(v)?),
            "valign" => f.valign = Some(valign(v)?),
            _ => {}
        }
    }
    b.session.format_shape_text(at, id, f)
}

/// A horizontal alignment word.
fn halign(word: &str) -> Result<HAlign, String> {
    match word {
        "left" => Ok(HAlign::Left),
        "centre" | "center" => Ok(HAlign::Centre),
        "right" => Ok(HAlign::Right),
        "justify" => Ok(HAlign::Justify),
        other => Err(format!("align is left, centre, right or justify; got `{other}`")),
    }
}

/// A vertical alignment word.
fn valign(word: &str) -> Result<VAlign, String> {
    match word {
        "top" => Ok(VAlign::Top),
        "middle" => Ok(VAlign::Middle),
        "bottom" => Ok(VAlign::Bottom),
        other => Err(format!("valign is top, middle or bottom; got `{other}`")),
    }
}

/// `object crop <left> <top> <right> <bottom>` as percents of the picture.
fn set_crop(b: &mut Book, at: usize, id: ObjectId, w: &[String]) -> Result<(), String> {
    let side = |i: usize, name: &str| -> Result<i32, String> {
        Ok((number(w.get(i), name)? * 1000.0).round() as i32)
    };
    let crop = Crop {
        left: side(0, "left")?,
        top: side(1, "top")?,
        right: side(2, "right")?,
        bottom: side(3, "bottom")?,
    };
    b.session.crop_picture(at, id, crop)
}

// ------------------------------------------------------------------------------------------------ picture

/// A path against the project folder when it is relative.
fn resolve(sheet: &Spreadsheet, word: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(word);
    match (&sheet.project, path.is_absolute()) {
        (Some(project), false) => project.join(path),
        _ => path,
    }
}

thread_local! {
    /// A file a test chooses in place of the platform file picker.
    static CHOSEN: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Make the next `picture pick` choose this file without showing the platform picker. For tests.
pub fn choose_picture_file(path: Option<std::path::PathBuf>) {
    CHOSEN.with(|c| *c.borrow_mut() = path);
}

/// The file the person chooses in the platform file picker, or the one a test chose.
fn pick_picture_file() -> Option<std::path::PathBuf> {
    if let Some(path) = CHOSEN.with(|c| c.borrow_mut().take()) {
        return Some(path);
    }
    // A test never shows the platform picker, which would wait for a person.
    if cfg!(test) {
        return None;
    }
    rfd::FileDialog::new()
        .set_title("Insert Picture")
        .add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "bmp"])
        .pick_file()
}

/// `picture insert <file> [cell]`, `picture pick [cell]`, `picture paste [cell]`, `picture in-cell [id]`.
fn picture(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let what = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[args.len().min(1)..];
    match what {
        "insert" => {
            let word = rest.first().ok_or("picture insert needs a file")?;
            let bytes = std::fs::read(resolve(sheet, word))
                .map_err(|e| format!("{word} could not be read: {e}"))?;
            put_picture(sheet, bytes, rest.get(1))
        }
        "pick" => match pick_picture_file() {
            Some(path) => {
                let bytes = std::fs::read(&path)
                    .map_err(|e| format!("{} could not be read: {e}", path.display()))?;
                put_picture(sheet, bytes, rest.first())
            }
            None => Ok(Answer::said("No picture chosen")),
        },
        "paste" => put_picture(sheet, clipboard_picture()?, rest.first()),
        "in-cell" => {
            let (ids, tail) = need_ids(sheet.current_book()?, rest)?;
            let word = tail.first().cloned();
            edit(sheet, move |b| {
                let at = cell_or_active(b, word.as_deref())?;
                b.session.place_in_cell(b.view.sheet, ids[0], at)?;
                Ok("Picture placed in the cell".to_owned())
            })
        }
        "crop" | "reset" => {
            let words: Vec<String> =
                std::iter::once(what.to_owned()).chain(rest.to_vec()).collect();
            object(sheet, &words)
        }
        _ => {
            Err("picture takes insert <file> [cell], pick, paste, in-cell, crop or reset"
                .to_owned())
        }
    }
}

/// The image the clipboard holds, as PNG bytes.
fn clipboard_picture() -> Result<Vec<u8>, String> {
    let held = crate::services::picture::from_the_clipboard()?;
    let data = held["data"].as_str().ok_or("There is no picture on the clipboard")?;
    unluminous_chat::base64::decode(data)
        .ok_or_else(|| "The clipboard picture could not be read".to_owned())
}

/// Put picture bytes on the sheet at a cell, or the active cell, and choose the new picture.
fn put_picture(sheet: &mut Spreadsheet, bytes: Vec<u8>, cell: Option<&String>) -> Out {
    let word = cell.cloned();
    edit(sheet, move |b| {
        let at = cell_or_active(b, word.as_deref())?;
        let id = b.session.insert_picture(b.view.sheet, &bytes, at)?;
        b.view.objects.choose(Some(id));
        Ok(format!("Inserted picture {id}"))
    })
}

// ------------------------------------------------------------------------------------------------ shapes

/// `shape draw <kind>` makes the next drag on the grid draw that shape. `shape <kind> [left top width
/// height]` inserts one in points, by default at the active cell.
fn shape(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let word = args
        .first()
        .ok_or("shape takes a kind: rectangle, rounded, ellipse, line, arrow or textbox")?;
    if word == "draw" {
        return draw_mode(sheet, args.get(1));
    }
    let kind =
        NewShape::parse(if word == "rounded" { "roundrect" } else { word }).ok_or_else(|| {
            format!("`{word}` is rectangle, rounded, ellipse, line, arrow or textbox")
        })?;
    let nums: Vec<f64> = args[1..].iter().filter_map(|w| w.parse().ok()).collect();
    edit(sheet, move |b| {
        let rect = match nums.as_slice() {
            [l, t, w, h] => Rect::new(emu(*l), emu(*t), emu(*w), emu(*h)),
            _ => at_the_active_cell(b),
        };
        let id = b.session.insert_shape(b.view.sheet, kind, rect)?;
        b.view.objects.choose(Some(id));
        Ok(format!("Inserted shape {id}"))
    })
}

/// A shape of a default size whose corner is the active cell's.
fn at_the_active_cell(b: &Book) -> Rect {
    let s = &b.session.book.sheets[b.view.sheet];
    let at = b.view.cursor;
    let x = unluminous_sheet::objects::geometry::column_x(s, at.col);
    let y = unluminous_sheet::objects::geometry::row_y(s, at.row);
    Rect::new(x, y, emu(120.0), emu(60.0))
}

/// `shape draw <kind>|off`.
fn draw_mode(sheet: &mut Spreadsheet, kind: Option<&String>) -> Out {
    let book = sheet.current_book()?;
    match kind.map(String::as_str) {
        None | Some("off") => {
            book.view.objects.drawing = None;
            Ok(Answer::said("Drawing off"))
        }
        Some(word) => {
            let word = if word == "rounded" { "roundrect" } else { word };
            NewShape::parse(word).ok_or_else(|| format!("`{word}` is not a shape kind"))?;
            book.view.objects.drawing = Some(word.to_owned());
            Ok(Answer::said(format!("Drag on the sheet to draw a {word}")))
        }
    }
}
