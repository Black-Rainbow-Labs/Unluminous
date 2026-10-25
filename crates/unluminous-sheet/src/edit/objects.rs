//! Pictures, shapes and the other drawing objects of a sheet: inserting, moving, sizing, cropping, styling,
//! stacking, aligning and grouping them. Each method is one undo step and refuses what Excel refuses with
//! the workbook unchanged.

use super::Session;
use crate::coord::CellRef;
use crate::model::{Kept, Sheet};
use crate::objects::make::{self, NewShape};
use crate::objects::*;
use crate::xlsx::xmltree;

/// Where an object goes in the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZOrder {
    ToFront,
    Forward,
    Backward,
    ToBack,
}

/// How Align lines objects up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Centre,
    Right,
    Top,
    Middle,
    Bottom,
}

/// The direction Distribute spreads objects in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distribution {
    Horizontally,
    Vertically,
}

/// The format changes of Home > Font applied to a shape's text. A field left `None` is not changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextFormat {
    pub size: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub color: Option<ShapeColor>,
    pub font: Option<String>,
    pub align: Option<HAlign>,
    pub valign: Option<VAlign>,
}

const NOT_FOUND: &str = "There is no such object on this sheet.";

fn index_of(sheet: &Sheet, id: ObjectId) -> Result<usize, String> {
    sheet.drawing.index_of(id).ok_or_else(|| NOT_FOUND.to_owned())
}

/// Change one object through a copy, so a refusal leaves it as it was.
fn modify(
    sheet: &mut Sheet,
    id: ObjectId,
    f: impl FnOnce(&mut DrawingObject, &Sheet) -> Result<(), String>,
) -> Result<(), String> {
    let i = index_of(sheet, id)?;
    let mut copy = sheet.drawing.objects[i].clone();
    f(&mut copy, sheet)?;
    sheet.drawing.objects[i] = copy;
    Ok(())
}

fn all_ids(objects: &[DrawingObject], out: &mut Vec<ObjectId>) {
    for o in objects {
        out.push(o.id);
        all_ids(&o.children, out);
    }
}

/// The next object number: above every id the drawing part holds, charts and children included.
fn next_id(sheet: &Sheet, kept: &Kept) -> ObjectId {
    let mut ids = Vec::new();
    all_ids(&sheet.drawing.objects, &mut ids);
    if let Some(rel) = sheet.raw.rels.iter().find(|r| r.kind == crate::xlsx::REL_DRAWING) {
        let part = crate::xlsx::resolve(&sheet.raw.part, &rel.target);
        if let Some(root) =
            kept.get(&part).and_then(|b| xmltree::parse(&String::from_utf8_lossy(b)))
        {
            let mut found = Vec::new();
            root.find_all("cNvPr", &mut found);
            ids.extend(found.iter().filter_map(|e| e.attr("id")?.parse::<u32>().ok()));
        }
    }
    ids.into_iter().max().unwrap_or(1) + 1
}

/// A media part name not in use.
fn fresh_media(kept: &Kept, extension: &str) -> String {
    (1..)
        .map(|n| format!("xl/media/image{n}.{extension}"))
        .find(|p| kept.get(p).is_none())
        .unwrap_or_default()
}

fn add_media(kept: &mut Kept, info: &make::ImageInfo, bytes: &[u8]) -> String {
    let part = fresh_media(kept, info.extension);
    kept.set(&part, bytes.to_vec());
    if !kept.defaults.iter().any(|(e, _)| e.eq_ignore_ascii_case(info.extension)) {
        kept.defaults.push((info.extension.to_owned(), info.content_type.to_owned()));
    }
    part
}

fn object_at(
    kind: ObjectKind,
    id: ObjectId,
    name: String,
    tree: xmltree::Elem,
    anchor: Anchor,
) -> DrawingObject {
    DrawingObject {
        id,
        name,
        descr: String::new(),
        hidden: false,
        kind,
        anchor,
        frame: Rect::default(),
        rotation: 0.0,
        frame_size: (0, 0),
        flip_h: false,
        flip_v: false,
        lock_aspect: false,
        locked: true,
        prints: true,
        picture: None,
        shape: None,
        children: Vec::new(),
        fallback: None,
        child_space: Rect::default(),
        tree,
        wrapped: false,
    }
}

/// A two cell anchor covering a rectangle.
fn anchor_for(sheet: &Sheet, rect: Rect, placement: Placement) -> Anchor {
    let mut anchor = Anchor { placement, ..Anchor::default() };
    anchor.set_rect(sheet, rect);
    anchor
}

impl Session {
    /// Run a change to one sheet's drawing and the package parts beside it as one undo step. The change is
    /// tried on copies first, so a refusal leaves the workbook as it was.
    fn change_drawing<R>(
        &mut self,
        label: &str,
        sheet: usize,
        f: impl FnOnce(&mut Sheet, &mut Kept) -> Result<R, String>,
    ) -> Result<R, String> {
        self.check_sheet(sheet)?;
        let mut trial = self.book.sheets[sheet].clone();
        trial.drawing.char_px = crate::ods::pixels_per_character(&self.book);
        let mut kept = self.book.kept.clone();
        let answer = f(&mut trial, &mut kept)?;
        trial.drawing.changed = true;
        self.remember_whole(label);
        self.book.sheets[sheet] = trial;
        self.book.kept = kept;
        Ok(answer)
    }

    // ------------------------------------------------------------------------------------------ pictures

    /// Insert a picture (PNG, JPEG, GIF, BMP or SVG bytes) with its top left corner at a cell, at the size
    /// its pixels and density give. An SVG needs a PNG to stand in for it: use [`Session::insert_svg_picture`].
    pub fn insert_picture(
        &mut self,
        sheet: usize,
        bytes: &[u8],
        at: CellRef,
    ) -> Result<ObjectId, String> {
        let info = make::image_info(bytes).ok_or("This file is not a picture Excel can insert.")?;
        if info.extension == "svg" {
            return Err("An SVG picture needs a PNG to stand in for it.".to_owned());
        }
        self.change_drawing("Insert picture", sheet, |s, kept| {
            let part = add_media(kept, &info, bytes);
            place_picture(s, kept, part, None, info.natural_size(), at)
        })
    }

    /// Insert a picture from the clipboard's image bytes. The same as inserting a file.
    pub fn paste_picture(
        &mut self,
        sheet: usize,
        bytes: &[u8],
        at: CellRef,
    ) -> Result<ObjectId, String> {
        self.insert_picture(sheet, bytes, at)
    }

    /// Insert an SVG picture with the PNG that stands in for it where SVG is not drawn.
    pub fn insert_svg_picture(
        &mut self,
        sheet: usize,
        svg: &[u8],
        png: &[u8],
        at: CellRef,
    ) -> Result<ObjectId, String> {
        let svg_info = make::image_info(svg)
            .filter(|i| i.extension == "svg")
            .ok_or("This file is not an SVG picture.")?;
        let png_info = make::image_info(png)
            .filter(|i| i.extension == "png")
            .ok_or("The stand in picture must be a PNG.")?;
        self.change_drawing("Insert picture", sheet, |s, kept| {
            let png_part = add_media(kept, &png_info, png);
            let svg_part = add_media(kept, &svg_info, svg);
            place_picture(s, kept, png_part, Some(svg_part), svg_info.natural_size(), at)
        })
    }

    /// Crop a picture. The edges are thousandths of a percent of the picture's full size; the picture keeps its
    /// scale, so it gets smaller by what is cut, as Excel's crop does.
    pub fn crop_picture(&mut self, sheet: usize, id: ObjectId, crop: Crop) -> Result<(), String> {
        for edge in [crop.left, crop.top, crop.right, crop.bottom] {
            if !(0..100_000).contains(&edge) {
                return Err("A crop must leave part of the picture.".to_owned());
            }
        }
        if crop.left + crop.right >= 100_000 || crop.top + crop.bottom >= 100_000 {
            return Err("A crop must leave part of the picture.".to_owned());
        }
        self.change_drawing("Crop picture", sheet, |s, _| {
            modify(s, id, |o, sheet| {
                let before = o.picture.as_ref().ok_or("Only a picture can be cropped.")?.crop;
                let frame = o.frame_rect(sheet);
                let part = |cut: i32, old: i32| f64::from(cut - old) / 100_000.0;
                let full_w =
                    frame.w as f64 / (1.0 - f64::from(before.left + before.right) / 100_000.0);
                let full_h =
                    frame.h as f64 / (1.0 - f64::from(before.top + before.bottom) / 100_000.0);
                let w = full_w * (1.0 - f64::from(crop.left + crop.right) / 100_000.0);
                let h = full_h * (1.0 - f64::from(crop.top + crop.bottom) / 100_000.0);
                let x = frame.x as f64 + full_w * part(crop.left, before.left);
                let y = frame.y as f64 + full_h * part(crop.top, before.top);
                o.set_frame(
                    sheet,
                    Rect::new(
                        x.round() as i64,
                        y.round() as i64,
                        w.round() as i64,
                        h.round() as i64,
                    ),
                );
                if let Some(p) = &mut o.picture {
                    p.crop = crop;
                }
                Ok(())
            })
        })
    }

    /// Reset a picture: no crop, and with `size` the size its pixels give, about its top left corner.
    pub fn reset_picture(&mut self, sheet: usize, id: ObjectId, size: bool) -> Result<(), String> {
        let cropped = self
            .book
            .sheets
            .get(sheet)
            .and_then(|s| s.drawing.find(id))
            .and_then(|o| o.picture.clone());
        let natural = cropped
            .as_ref()
            .and_then(|p| self.book.kept.get(&p.part))
            .and_then(|b| make::image_info(b))
            .map(|i| i.natural_size());
        self.change_drawing("Reset picture", sheet, |s, _| {
            modify(s, id, |o, sheet| {
                let frame = o.frame_rect(sheet);
                o.picture.as_mut().ok_or("Only a picture can be reset.")?.crop = Crop::default();
                let (w, h) =
                    if size { natural.unwrap_or((frame.w, frame.h)) } else { (frame.w, frame.h) };
                o.set_frame(sheet, Rect::new(frame.x, frame.y, w, h));
                Ok(())
            })
        })
    }

    /// Place a picture in a cell (Picture > Place in Cell): the picture leaves the drawing and the cell shows it,
    /// holding `#VALUE!` as Excel leaves it. The cell must be empty.
    pub fn place_in_cell(&mut self, sheet: usize, id: ObjectId, at: CellRef) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let object = self.book.sheets[sheet].drawing.find(id).ok_or(NOT_FOUND)?;
        let picture = object.picture.clone().ok_or("Only a picture can be placed in a cell.")?;
        let alt = object.descr.clone();
        if picture.svg.is_some() {
            return Err("An SVG picture cannot be placed in a cell.".to_owned());
        }
        let busy = self.book.sheets[sheet]
            .cells
            .get(&(at.row, at.col))
            .is_some_and(|c| !c.value.is_empty() || c.formula.is_some());
        if busy {
            return Err("Place in Cell needs an empty cell.".to_owned());
        }
        self.remember_whole("Place picture in cell");
        crate::objects::richdata::place(&mut self.book, sheet, at, &picture.part, &alt)?;
        let s = &mut self.book.sheets[sheet];
        s.drawing.objects.retain(|o| o.id != id);
        s.drawing.changed = true;
        Ok(())
    }

    /// The image part a cell shows when a picture was placed in it.
    pub fn cell_image(&self, sheet: usize, at: CellRef) -> Option<String> {
        crate::objects::richdata::cell_image(&self.book, sheet, at)
    }

    // ---------------------------------------------------------------------------------------------- shapes

    /// Insert a shape with its frame at a rectangle (drawing units, from the top left of the sheet).
    pub fn insert_shape(
        &mut self,
        sheet: usize,
        kind: NewShape,
        rect: Rect,
    ) -> Result<ObjectId, String> {
        if rect.w < 0 || rect.h < 0 {
            return Err("A shape needs a size.".to_owned());
        }
        self.change_drawing("Insert shape", sheet, |s, kept| {
            let id = next_id(s, kept);
            let name = format!("{} {}", kind.base_name(), id - 1);
            let tree = make::shape_tree(kind, id, &name, (rect.w, rect.h));
            let object_kind = match kind {
                NewShape::Line | NewShape::Arrow => ObjectKind::Connector,
                NewShape::TextBox => ObjectKind::TextBox,
                _ => ObjectKind::Shape,
            };
            let mut o =
                object_at(object_kind, id, name, tree, anchor_for(s, rect, Placement::MoveAndSize));
            o.shape = crate::objects::props::read_shape(&o.tree);
            o.frame_size = (rect.w, rect.h);
            s.drawing.objects.push(o);
            Ok(id)
        })
    }

    /// Set what fills a shape.
    pub fn set_shape_fill(&mut self, sheet: usize, id: ObjectId, fill: Fill) -> Result<(), String> {
        self.change_shape("Shape fill", sheet, id, |sh| sh.fill = fill)
    }

    /// Set a shape's outline: its colour, width, dash and line ends.
    pub fn set_shape_line(&mut self, sheet: usize, id: ObjectId, line: Line) -> Result<(), String> {
        self.change_shape("Shape outline", sheet, id, |sh| sh.line = line)
    }

    /// Set how transparent a solid fill is, from 0 (opaque) to 100.
    pub fn set_shape_transparency(
        &mut self,
        sheet: usize,
        id: ObjectId,
        percent: f64,
    ) -> Result<(), String> {
        if !(0.0..=100.0).contains(&percent) {
            return Err("Transparency is between 0% and 100%.".to_owned());
        }
        self.change_shape("Shape transparency", sheet, id, |sh| {
            let colour = match &sh.fill {
                Fill::Solid(c) => c.clone(),
                _ => sh.style_fill.clone().unwrap_or_else(|| ShapeColor::scheme("accent1")),
            };
            sh.fill = Fill::Solid(colour.with_transparency(percent));
        })
    }

    /// Replace the text of a shape, keeping the format of its first run.
    pub fn set_shape_text(&mut self, sheet: usize, id: ObjectId, text: &str) -> Result<(), String> {
        self.change_shape("Shape text", sheet, id, |sh| {
            let body =
                sh.text.get_or_insert_with(|| TextBody { wrap: true, ..TextBody::default() });
            let template = body.paragraphs.iter().flat_map(|p| p.runs.first()).next().cloned();
            let align = body.paragraphs.first().map_or(HAlign::Left, |p| p.align);
            body.paragraphs = text
                .split('\n')
                .map(|line| {
                    let mut run = template.clone().unwrap_or_else(|| TextRun::plain(""));
                    run.text = line.to_owned();
                    TextParagraph {
                        align,
                        runs: if line.is_empty() { Vec::new() } else { vec![run] },
                    }
                })
                .collect();
        })
    }

    /// Change the font, size, style, colour and alignment of all the text of a shape.
    pub fn format_shape_text(
        &mut self,
        sheet: usize,
        id: ObjectId,
        format: TextFormat,
    ) -> Result<(), String> {
        self.change_shape("Format shape text", sheet, id, |sh| {
            let body =
                sh.text.get_or_insert_with(|| TextBody { wrap: true, ..TextBody::default() });
            if let Some(v) = format.valign {
                body.valign = v;
            }
            for p in &mut body.paragraphs {
                if let Some(a) = format.align {
                    p.align = a;
                }
                for r in &mut p.runs {
                    r.size = format.size.or(r.size);
                    r.bold = format.bold.unwrap_or(r.bold);
                    r.italic = format.italic.unwrap_or(r.italic);
                    r.underline = format.underline.unwrap_or(r.underline);
                    if format.color.is_some() {
                        r.color = format.color.clone();
                    }
                    if format.font.is_some() {
                        r.font = format.font.clone();
                    }
                }
            }
        })
    }

    fn change_shape(
        &mut self,
        label: &str,
        sheet: usize,
        id: ObjectId,
        f: impl FnOnce(&mut ShapeData),
    ) -> Result<(), String> {
        self.change_drawing(label, sheet, |s, _| {
            modify(s, id, |o, _| {
                f(o.shape.as_mut().ok_or("This object is not a shape this version can change.")?);
                Ok(())
            })
        })
    }

    // ------------------------------------------------------------------------------- every kind of object

    /// Move an object by a distance in drawing units.
    pub fn move_object(
        &mut self,
        sheet: usize,
        id: ObjectId,
        dx: i64,
        dy: i64,
    ) -> Result<(), String> {
        self.change_drawing("Move object", sheet, |s, _| translate(s, id, dx, dy))
    }

    /// Put an object's frame at a rectangle.
    pub fn set_object_frame(
        &mut self,
        sheet: usize,
        id: ObjectId,
        frame: Rect,
    ) -> Result<(), String> {
        if frame.w < 0 || frame.h < 0 {
            return Err("An object needs a size.".to_owned());
        }
        self.change_drawing("Size object", sheet, |s, _| {
            modify(s, id, |o, sheet| {
                o.set_frame(sheet, frame);
                Ok(())
            })
        })
    }

    /// Put an object at exactly this anchor. The window works out the cell and offsets from the pointer in
    /// the columns it draws, so a drag lands where the pointer was whatever the column widths are.
    pub fn set_object_anchor(
        &mut self,
        sheet: usize,
        id: ObjectId,
        anchor: Anchor,
    ) -> Result<(), String> {
        self.change_drawing("Move object", sheet, |s, _| {
            modify(s, id, |o, sheet| {
                let kind = o.anchor.kind;
                o.anchor = Anchor { kind, ..anchor };
                let frame = o.anchor.rect(sheet);
                o.frame_size = (frame.w, frame.h);
                Ok(())
            })
        })
    }

    /// Resize an object about its top left corner. With `keep_aspect`, or when the object locks its aspect
    /// ratio, the height follows the width.
    pub fn resize_object(
        &mut self,
        sheet: usize,
        id: ObjectId,
        width: i64,
        height: i64,
        keep_aspect: bool,
    ) -> Result<(), String> {
        self.change_drawing("Size object", sheet, |s, _| {
            modify(s, id, |o, sheet| {
                let frame = o.frame_rect(sheet);
                let h = if (keep_aspect || o.lock_aspect) && frame.w > 0 {
                    (width as f64 * frame.h as f64 / frame.w as f64).round() as i64
                } else {
                    height
                };
                o.set_frame(sheet, Rect::new(frame.x, frame.y, width.max(0), h.max(0)));
                Ok(())
            })
        })
    }

    /// Turn an object to an angle in degrees clockwise about its centre.
    pub fn rotate_object(
        &mut self,
        sheet: usize,
        id: ObjectId,
        degrees: f64,
    ) -> Result<(), String> {
        self.change_drawing("Rotate object", sheet, |s, _| {
            modify(s, id, |o, sheet| {
                let frame = o.frame_rect(sheet);
                o.rotation = degrees.rem_euclid(360.0);
                o.set_frame(sheet, frame);
                Ok(())
            })
        })
    }

    /// Flip an object across a vertical (`horizontal`) or horizontal axis.
    pub fn flip_object(
        &mut self,
        sheet: usize,
        id: ObjectId,
        horizontal: bool,
    ) -> Result<(), String> {
        self.change_drawing("Flip object", sheet, |s, _| {
            modify(s, id, |o, _| {
                if horizontal {
                    o.flip_h = !o.flip_h;
                } else {
                    o.flip_v = !o.flip_v;
                }
                Ok(())
            })
        })
    }

    /// Set the alternative text of an object.
    pub fn set_alt_text(&mut self, sheet: usize, id: ObjectId, text: &str) -> Result<(), String> {
        self.change_drawing("Alt text", sheet, |s, _| {
            modify(s, id, |o, _| {
                o.descr = text.to_owned();
                Ok(())
            })
        })
    }

    /// Rename an object, as the Name box does.
    pub fn set_object_name(
        &mut self,
        sheet: usize,
        id: ObjectId,
        name: &str,
    ) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("An object needs a name.".to_owned());
        }
        self.change_drawing("Rename object", sheet, |s, _| {
            if s.drawing.objects.iter().any(|o| o.id != id && o.name.eq_ignore_ascii_case(name)) {
                return Err("There is already an object with that name.".to_owned());
            }
            modify(s, id, |o, _| {
                o.name = name.to_owned();
                Ok(())
            })
        })
    }

    /// Choose how an object follows the cells under it.
    pub fn set_placement(
        &mut self,
        sheet: usize,
        id: ObjectId,
        placement: Placement,
    ) -> Result<(), String> {
        self.change_drawing("Object placement", sheet, |s, _| {
            modify(s, id, |o, _| {
                o.anchor.placement = placement;
                Ok(())
            })
        })
    }

    /// Lock or free the aspect ratio of an object.
    pub fn set_aspect_lock(
        &mut self,
        sheet: usize,
        id: ObjectId,
        locked: bool,
    ) -> Result<(), String> {
        self.change_drawing("Lock aspect ratio", sheet, |s, _| {
            modify(s, id, |o, _| {
                o.lock_aspect = locked;
                Ok(())
            })
        })
    }

    /// Delete an object.
    pub fn delete_object(&mut self, sheet: usize, id: ObjectId) -> Result<(), String> {
        self.change_drawing("Delete object", sheet, |s, _| {
            let i = index_of(s, id)?;
            s.drawing.objects.remove(i);
            Ok(())
        })
    }

    /// Bring an object forward or to the front, or send it backward or to the back.
    pub fn reorder_object(
        &mut self,
        sheet: usize,
        id: ObjectId,
        order: ZOrder,
    ) -> Result<(), String> {
        self.change_drawing("Arrange objects", sheet, |s, _| {
            let list = &mut s.drawing.objects;
            let i = list.iter().position(|o| o.id == id).ok_or(NOT_FOUND)?;
            let target = match order {
                ZOrder::ToFront => list.len() - 1,
                ZOrder::ToBack => 0,
                ZOrder::Forward => (i + 1).min(list.len() - 1),
                ZOrder::Backward => i.saturating_sub(1),
            };
            let o = list.remove(i);
            list.insert(target, o);
            Ok(())
        })
    }

    /// Line objects up on an edge or a centre of the group they are in.
    pub fn align_objects(
        &mut self,
        sheet: usize,
        ids: &[ObjectId],
        how: Alignment,
    ) -> Result<(), String> {
        if ids.len() < 2 {
            return Err("Select two or more objects to align them.".to_owned());
        }
        self.change_drawing("Align objects", sheet, |s, _| {
            let boxes = bounds_of(s, ids)?;
            let union = boxes.iter().fold(boxes[0].1, |u, (_, r)| u.union(r));
            for (id, r) in boxes {
                let (dx, dy) = match how {
                    Alignment::Left => (union.x - r.x, 0),
                    Alignment::Right => (union.right() - r.right(), 0),
                    Alignment::Centre => ((union.centre().0 - r.centre().0).round() as i64, 0),
                    Alignment::Top => (0, union.y - r.y),
                    Alignment::Bottom => (0, union.bottom() - r.bottom()),
                    Alignment::Middle => (0, (union.centre().1 - r.centre().1).round() as i64),
                };
                translate(s, id, dx, dy)?;
            }
            Ok(())
        })
    }

    /// Spread objects so the gaps between them are equal, keeping the first and the last where they are.
    pub fn distribute_objects(
        &mut self,
        sheet: usize,
        ids: &[ObjectId],
        how: Distribution,
    ) -> Result<(), String> {
        if ids.len() < 3 {
            return Err("Select three or more objects to distribute them.".to_owned());
        }
        self.change_drawing("Distribute objects", sheet, |s, _| {
            let mut boxes = bounds_of(s, ids)?;
            let horizontal = how == Distribution::Horizontally;
            let start = |r: &Rect| if horizontal { r.x } else { r.y };
            let size = |r: &Rect| if horizontal { r.w } else { r.h };
            boxes.sort_by_key(|(_, r)| start(r));
            let first = start(&boxes[0].1);
            let last = start(&boxes[boxes.len() - 1].1) + size(&boxes[boxes.len() - 1].1);
            let total: i64 = boxes.iter().map(|(_, r)| size(r)).sum();
            let gap = (last - first - total) as f64 / (boxes.len() - 1) as f64;
            let mut at = first as f64;
            for (id, r) in boxes {
                let delta = (at - start(&r) as f64).round() as i64;
                translate(
                    s,
                    id,
                    if horizontal { delta } else { 0 },
                    if horizontal { 0 } else { delta },
                )?;
                at += size(&r) as f64 + gap;
            }
            Ok(())
        })
    }

    /// Group objects into one. The group takes the place of the highest of them in the stack.
    pub fn group_objects(&mut self, sheet: usize, ids: &[ObjectId]) -> Result<ObjectId, String> {
        if ids.len() < 2 {
            return Err("Select two or more objects to group them.".to_owned());
        }
        self.change_drawing("Group objects", sheet, |s, kept| {
            let mut members = Vec::new();
            for &id in ids {
                let o = &s.drawing.objects[index_of(s, id)?];
                if o.wrapped {
                    return Err("This kind of object cannot be grouped.".to_owned());
                }
                members.push((index_of(s, id)?, o.clone()));
            }
            members.sort_by_key(|(i, _)| *i);
            let frames: Vec<Rect> = members.iter().map(|(_, o)| o.frame_rect(s)).collect();
            let union = frames.iter().fold(frames[0], |u, r| u.union(r));
            let at = members[members.len() - 1].0;
            let id = next_id(s, kept);
            let name = format!("Group {}", id - 1);
            let mut children = Vec::new();
            let mut trees = Vec::new();
            for ((_, mut o), frame) in members.into_iter().zip(&frames) {
                make::set_frame_in_tree(&mut o.tree, *frame);
                o.frame = *frame;
                trees.push(o.tree.clone());
                children.push(o);
            }
            s.drawing.objects.retain(|o| !ids.contains(&o.id));
            let tree = make::group_tree(id, &name, union, trees);
            let at = at + 1 - ids.len();
            let mut g = object_at(
                ObjectKind::Group,
                id,
                name,
                tree,
                anchor_for(s, union, Placement::MoveAndSize),
            );
            g.children = children;
            g.child_space = union;
            g.frame_size = (union.w, union.h);
            s.drawing.objects.insert(at.min(s.drawing.objects.len()), g);
            Ok(id)
        })
    }

    /// Ungroup a group, leaving its members where they are on the sheet. Returns their ids.
    pub fn ungroup_object(&mut self, sheet: usize, id: ObjectId) -> Result<Vec<ObjectId>, String> {
        self.change_drawing("Ungroup objects", sheet, |s, _| {
            let i = index_of(s, id)?;
            let group = s.drawing.objects[i].clone();
            if group.kind != ObjectKind::Group {
                return Err("Only a group can be ungrouped.".to_owned());
            }
            let outer = group.frame_rect(s);
            let space = group.child_space;
            let (sx, sy) = (
                if space.w == 0 { 1.0 } else { outer.w as f64 / space.w as f64 },
                if space.h == 0 { 1.0 } else { outer.h as f64 / space.h as f64 },
            );
            let mut out = Vec::new();
            for mut child in group.children {
                let f = child.frame;
                let frame = Rect::new(
                    outer.x + ((f.x - space.x) as f64 * sx).round() as i64,
                    outer.y + ((f.y - space.y) as f64 * sy).round() as i64,
                    (f.w as f64 * sx).round() as i64,
                    (f.h as f64 * sy).round() as i64,
                );
                child.anchor = anchor_for(s, frame, Placement::MoveAndSize);
                child.set_frame(s, frame);
                out.push(child);
            }
            let ids: Vec<ObjectId> = out.iter().map(|o| o.id).collect();
            s.drawing.objects.splice(i..=i, out);
            Ok(ids)
        })
    }
}

fn bounds_of(sheet: &Sheet, ids: &[ObjectId]) -> Result<Vec<(ObjectId, Rect)>, String> {
    ids.iter()
        .map(|&id| Ok((id, sheet.drawing.objects[index_of(sheet, id)?].bounds(sheet))))
        .collect()
}

/// Move one object by a distance, keeping its size.
fn translate(sheet: &mut Sheet, id: ObjectId, dx: i64, dy: i64) -> Result<(), String> {
    modify(sheet, id, |o, s| {
        let f = o.frame_rect(s);
        o.set_frame(s, Rect::new((f.x + dx).max(0), (f.y + dy).max(0), f.w, f.h));
        Ok(())
    })
}

/// Add a picture object for an image part already in the package.
fn place_picture(
    sheet: &mut Sheet,
    kept: &Kept,
    part: String,
    svg: Option<String>,
    size: (i64, i64),
    at: CellRef,
) -> Result<ObjectId, String> {
    let id = next_id(sheet, kept);
    let name = format!("Picture {}", id - 1);
    let (x, y) = geometry::point_of(sheet, (at, 0, 0));
    let tree = make::picture_tree(id, &name, "rId1", svg.as_ref().map(|_| "rId2"), size);
    let anchor = anchor_for(sheet, Rect::new(x, y, size.0, size.1), Placement::Move);
    let kind = if svg.is_some() { ObjectKind::Icon } else { ObjectKind::Picture };
    let mut o = object_at(kind, id, name, tree, anchor);
    o.picture = Some(PictureData { part, crop: Crop::default(), svg });
    o.lock_aspect = true;
    o.frame_size = size;
    sheet.drawing.objects.push(o);
    Ok(id)
}
