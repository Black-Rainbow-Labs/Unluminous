//! Glass under every popup egui frames itself (`task-2281`).
//!
//! A surface drawn through `rux` is glass by `rux`'s own rule: a raised translucent surface bends
//! the picture behind the window. A dropdown, a menu, a context menu, a submenu and a tooltip are
//! not `rux` surfaces: egui paints each one's frame as a plain rounded rectangle when the popup is
//! shown, before anything in it. Under a glass theme that frame is painted in the glass card's
//! colour ([`crate::components::controls::menu_frame`] and the window fill `theme::apply` sets),
//! which is translucent, so without this a menu would be a smoky sheet with the editor's words
//! readable through it.
//!
//! So once a frame is drawn, [`glaze_the_popups`] walks the layers above the page and puts the bent
//! picture under every rectangle filled in that colour, in the same slot, so the frame's tint, its
//! rim and the menu's rows are painted over it. It finds the frames by their colour rather than by
//! a list of Unluminous's menus, which is `occluding_rects`' rule: every popup egui draws is
//! covered, including the next one somebody adds. A popup inside a modal bends the modal's
//! photograph of the window, and one over the page bends the page's picture, which is
//! `rux::render::backdrop_for`'s answer for the popup's layer.

use egui::{Color32, Shape};

/// The colour a popup frame is filled with under glass, which is what [`glaze_the_popups`] looks for.
pub fn popup_fill() -> Color32 {
    super::rux_theme().chat.card.from
}

/// Paint the bent picture over `rect` with `painter`, as glass under whatever is painted after it, and
/// answer whether there was a picture to bend.
///
/// For a surface Unluminous paints with `egui` that has to hide what is behind it: a canvas node's card
/// covers the nodes behind it, so under glass the card is the bent picture at full strength with the
/// card's tint over it. Nothing is painted, and false is answered, unless the theme is glass and the
/// window has a picture.
///
/// @param painter - what to paint with, in the layer the surface is in
/// @param rect - the surface, in that layer's points
/// @param radius - how round its corners are
pub fn paint_glass(painter: &egui::Painter, rect: egui::Rect, radius: f32) -> bool {
    let Some(backdrop) = rux::render::backdrop_for(painter.ctx(), painter.layer_id()) else {
        return false;
    };
    paint_glass_over(painter, rect, radius, backdrop)
}

/// [`paint_glass`] bending a given picture.
///
/// @param painter - what to paint with
/// @param rect - the surface, in the painter's layer's points
/// @param radius - how round its corners are
/// @param backdrop - the picture to bend
fn paint_glass_over(
    painter: &egui::Painter,
    rect: egui::Rect,
    radius: f32,
    backdrop: std::sync::Arc<rux::render::Backdrop>,
) -> bool {
    let Some(material) = super::rux_theme().glass else { return false };
    let ctx = painter.ctx();
    let to_global = ctx.layer_transform_to_global(painter.layer_id()).unwrap_or_default();
    let size = rect.size();
    let pane = rux::render::glass::Pane {
        rect,
        corners: rux::Corners::all(radius),
        bevel: material.bevel_for(size),
        strength: material.strength_for(size),
    };
    let key = Built { pane, backdrop, to_global, pixels_per_point: ctx.pixels_per_point() };
    let shapes = kept_or_built(ctx, painter.layer_id(), key, &material);
    let painted = !shapes.is_empty();
    painter.extend(shapes);
    painted
}

fn frosted_slot() -> egui::Id {
    egui::Id::new("unluminous-frosted-picture")
}

/// Say where the frosted copy of the picture behind the window is this frame, or that there is none.
///
/// @param ctx - the window's context
/// @param frosted - the frosted picture's texture and the part of it the window shows
pub fn set_frosted(ctx: &egui::Context, frosted: Option<(egui::TextureId, egui::Rect)>) {
    ctx.data_mut(|data| match frosted {
        Some(frosted) => {
            data.insert_temp(frosted_slot(), frosted);
        }
        None => data.remove::<(egui::TextureId, egui::Rect)>(frosted_slot()),
    });
}

/// The frosted copy of the picture behind the window and the part of it the window shows, under glass
/// with a picture chosen. A modal is drawn over it rather than over a photograph of the window.
pub fn frosted(ctx: &egui::Context) -> Option<(egui::TextureId, egui::Rect)> {
    if !super::is_glass() {
        return None;
    }
    ctx.data(|data| data.get_temp::<(egui::TextureId, egui::Rect)>(frosted_slot()))
}

/// How far a panel's glass stands in from the room it is given, on every side: so two panels side by
/// side have a gap of twice this between them, which is the window's glass showing.
pub const PANE_GAP: f32 = 4.0;

/// How round a panel's glass is: the window's own corner less the gap, so a panel in a corner of the
/// window sits in it concentrically, which is Apple's rule for glass inside glass.
pub const PANE_RADIUS: f32 = 10.0;

/// The window itself as one sheet of glass (`task-2281`): the picture behind it bent at the window's
/// rounded edge, under the window's own smoke. Nothing unless the theme is glass and there is a picture.
///
/// @param painter - the window's background painter
/// @param full - the whole window
/// @param radius - the window's corner
pub fn paint_the_window(painter: &egui::Painter, full: egui::Rect, radius: f32) -> bool {
    paint_glass(painter, full, radius)
}

/// A pane or a bar as a panel of glass floating on the window (`task-2281`), drawn the way a card is.
///
/// Apple's sidebar is "a pane of glass that floats above the window's content", and Jason asked for the
/// folder pane, the title bar, the status bar and the Agent-Chat pane to be glass like a card. So the
/// panel is a `rux` card surface: the card's tint over the picture bent through it, the card's shadow
/// and its soft light from the top, with no line along any edge. It stands in from `room` by
/// [`PANE_GAP`] on every side. A card or a button on the panel is glass on glass: its own tint and
/// shadow make it read as a second, nearer sheet.
///
/// @param ui - the window's `Ui`, before anything in the panel is drawn
/// @param salt - which panel, for its `rux` canvas
/// @param room - the rectangle the pane or bar was given
pub fn paint_a_panel(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    room: egui::Rect,
) {
    if room.width() < PANE_GAP * 4.0 || room.height() < PANE_GAP * 4.0 {
        return;
    }
    let panel = room.shrink(PANE_GAP);
    crate::components::chrome_kit::layer(ui, ("glass-panel", salt), panel, 48.0, |rux| {
        let chat = rux.theme().chat;
        rux.chrome.surface(
            panel,
            PANE_RADIUS,
            rux::Fill::gradient(chat.card, panel),
            chat.raised(),
        );
    });
}

/// What one surface's glass was built from.
#[derive(Clone, PartialEq)]
struct Built {
    pane: rux::render::glass::Pane,
    backdrop: std::sync::Arc<rux::render::Backdrop>,
    to_global: egui::emath::TSTransform,
    pixels_per_point: f32,
}

/// The glass built for each layer the last two passes, so a still canvas builds nothing.
#[derive(Clone, Default)]
struct Kept {
    pass: u64,
    entries: Vec<(egui::LayerId, Built, Vec<Shape>, u64)>,
}

/// The shapes for `key`, from the last pass when nothing they were built from has moved.
///
/// A canvas node's card is painted every frame, and building its glass is a few hundred points of
/// arithmetic; a node that has not moved reuses what was built. An entry not asked for this pass or the
/// one before is dropped.
fn kept_or_built(
    ctx: &egui::Context,
    layer: egui::LayerId,
    key: Built,
    material: &rux::theme::Glass,
) -> Vec<Shape> {
    let slot = egui::Id::new("unluminous-painted-glass");
    let pass = ctx.cumulative_pass_nr();
    let mut kept: Kept = ctx.data(|data| data.get_temp(slot)).unwrap_or_default();
    if kept.pass != pass {
        kept.entries.retain(|entry| entry.3 + 1 >= pass);
        kept.pass = pass;
    }
    let found = kept.entries.iter_mut().find(|entry| entry.0 == layer && entry.1 == key);
    let shapes = match found {
        Some(entry) => {
            entry.3 = pass;
            entry.2.clone()
        }
        None => {
            let shapes = rux::render::glass::shapes(
                material,
                &key.pane,
                &key.backdrop,
                key.to_global,
                key.pixels_per_point,
            );
            kept.entries.push((layer, key, shapes.clone(), pass));
            shapes
        }
    };
    ctx.data_mut(|data| data.insert_temp(slot, kept));
    shapes
}

/// Put glass under every popup frame drawn this frame. Nothing at all unless the theme is glass.
///
/// @param ctx - the window's context, once everything has been drawn
pub fn glaze_the_popups(ctx: &egui::Context) {
    let Some(material) = super::rux_theme().glass else { return };
    let marker = popup_fill();
    let layers = ctx.memory(|memory| memory.areas().visible_layer_ids());
    for layer in layers {
        if matches!(layer.order, egui::Order::Background) {
            continue;
        }
        let Some(backdrop) = rux::render::backdrop_for(ctx, layer) else { continue };
        let to_global = ctx.layer_transform_to_global(layer).unwrap_or_default();
        let pixels_per_point = ctx.pixels_per_point();
        let mut changed: Vec<(usize, Shape)> = Vec::new();
        ctx.graphics_mut(|graphics| {
            let Some(list) = graphics.get_mut(layer) else { return };
            for (index, clipped) in list.all_entries().enumerate() {
                let mut shape = clipped.shape.clone();
                let glaze = |rect: &egui::epaint::RectShape| {
                    let corners = rux::Corners {
                        nw: f32::from(rect.corner_radius.nw),
                        ne: f32::from(rect.corner_radius.ne),
                        se: f32::from(rect.corner_radius.se),
                        sw: f32::from(rect.corner_radius.sw),
                    };
                    let size = rect.rect.size();
                    let pane = rux::render::glass::Pane {
                        rect: rect.rect,
                        corners,
                        bevel: material.bevel_for(size),
                        strength: material.strength_for(size),
                    };
                    rux::render::glass::shapes(
                        &material,
                        &pane,
                        &backdrop,
                        to_global,
                        pixels_per_point,
                    )
                };
                if under(&mut shape, marker, &glaze) {
                    changed.push((index, shape));
                }
            }
        });
        if changed.is_empty() {
            continue;
        }
        ctx.graphics_mut(|graphics| {
            if let Some(list) = graphics.get_mut(layer) {
                for (index, shape) in changed {
                    list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                        clipped.shape = shape
                    });
                }
            }
        });
    }
}

/// Put `glaze`'s shapes under every rectangle in `shape` filled in `marker`, and answer whether any was.
///
/// A frame with a shadow is a group of shapes, the shadow first, so a group is looked inside. The glass
/// goes after the shadow and before the fill, which is where a backdrop filter draws in a browser.
fn under(
    shape: &mut Shape,
    marker: Color32,
    glaze: &dyn Fn(&egui::epaint::RectShape) -> Vec<Shape>,
) -> bool {
    match shape {
        Shape::Rect(rect) if rect.fill == marker && rect.rect.is_positive() => {
            let mut glass = glaze(rect);
            if glass.is_empty() {
                return false;
            }
            glass.push(Shape::Rect(rect.clone()));
            let group = glass;
            *shape = Shape::Vec(group);
            true
        }
        Shape::Vec(shapes) => {
            let mut any = false;
            for inner in shapes.iter_mut() {
                any |= under(inner, marker, glaze);
            }
            any
        }
        _ => false,
    }
}
