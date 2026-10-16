//! What a person sees and does while a panel is being moved to another edge of the window.
//!
//! `task-1697` asks for two things a person can see: a panel's header has to be draggable, and while
//! one is in the air there have to be *"blue highlighted regions to indicate where I can drag to"*.
//! Both are here. Where a panel would land, and where it does land, are `app::dock`'s to decide —
//! this component **reports and decides nothing**, which is the rule every component in Unluminous follows.
//!
//! ## The handle is the header, and it is added first on purpose
//!
//! A tile's header already holds tabs that are dragged along their own strip, and buttons. So the
//! handle is added to the `Ui` **before** them, over the whole header, and they are added after it:
//! egui gives a pointer to the last widget that asked for the point, which is the rule
//! `components::splitter` and `components::resize_edges` are both written around, used here the other
//! way up. What the handle is left with is exactly the part of the header nothing else wanted — the
//! heading word and the empty space beside it, which is the reference editor's own handle for the same gesture.
//!
//! ## The blue rectangle is the layout, not a picture of it
//!
//! The four bands are drawn faintly, because the ask is that a person can see there are four places
//! rather than only the one they happen to be over. The strong rectangle is not a guess about where
//! the panel will go: it is `app::dock::regions` run over the layout **as it would be after the
//! drop**, so the preview and the drop are one function applied to one value and cannot disagree.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use crate::app::dock::{Aim, Panel, Side, Zone};
use crate::theme::color;

/// How much of the accent each part of the overlay is painted at.
const BAND_FILL: f32 = 0.14;
const BAND_EDGE: f32 = 0.55;
const LANDING_FILL: f32 = 0.28;

/// What a panel's header reported this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Grab {
    /// Where the pointer is while the panel is in the air.
    pub carrying: Option<Pos2>,
    /// True on the frame it was let go.
    pub dropped: bool,
    /// The header was right clicked, which opens the panel's own menu.
    pub menu: Option<Pos2>,
    /// The header was double clicked, which fills the window with this panel and puts it back.
    ///
    /// `task-1771`. The handle is already the whole strip, so this is one more thing the same
    /// `Response` answers rather than a second widget over it.
    pub twice: bool,
}

/// Make `header` the handle a panel is carried by.
///
/// Call it **before** anything else in the header — see the note at the top of the file.
pub fn handle(ui: &mut egui::Ui, header: Rect, panel: Panel) -> Grab {
    let mut grab = Grab::default();
    if header.width() <= 0.0 || header.height() <= 0.0 {
        return grab;
    }
    // `Sense::CLICK | Sense::DRAG` and not `Sense::click_and_drag()`: the same presses, but the
    // handle cannot take egui's keyboard focus. It covers the whole of a panel's header, so a bare
    // `Tab` press would land on it very easily — and `app::hold_the_keyboard` records what a widget
    // holding the focus is worth, which is that `Space` then presses it.
    let response = ui.interact(
        header,
        ui.id().with(("dock-handle", panel.name())),
        Sense::CLICK | Sense::DRAG,
    );
    if response.dragged() || response.drag_stopped() {
        if let Some(pointer) = response.interact_pointer_pos() {
            grab.carrying = Some(pointer);
            grab.dropped = response.drag_stopped();
        }
        // No cursor change while the panel is carried: the four bands and the strong rectangle the
        // panel would land in are on screen for the whole of the drag, and a grabbing hand under the
        // pointer is a second channel saying the same thing — which is the one `task-1747` does not
        // want. The header is the normal arrow from hover through the drop.
    }
    if response.secondary_clicked() {
        grab.menu = response.interact_pointer_pos().or_else(|| response.hover_pos());
    }
    grab.twice = response.double_clicked();
    // Named, because every control in Unluminous has a plain name and a test finds one by it.
    let name = format!("Move {}", panel.label());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), name.clone())
    });
    grab
}

/// Draw the four places a panel can be dropped, and the one it would land in.
///
/// `landing` is the rectangle the panel would occupy — worked out by the window from
/// `app::dock::regions`, so it is the real one. `Rect::ZERO` while the pointer is over none of the
/// bands, which is a drag that can still be thought better of.
///
/// Each band also carries its `Fill whole side` target, the part of the band nearest the window's edge
/// (`task-2198`, made the whole outer part of the band by `task-2280`). It is drawn over everything else so it
/// can be seen over the strong rectangle too, and it says what it does in words: `Fill whole bottom`. Its label
/// runs along the band, which on the left and the right means up the window's edge.
pub fn zones(ui: &egui::Ui, bands: &[Zone; 4], aimed: Option<Aim>, landing: Rect, carrying: Panel) {
    let painter = ui.painter();
    let chosen = aimed.map(|aim| aim.side);
    for zone in bands {
        if Some(zone.side) == chosen {
            continue;
        }
        painter.rect_filled(zone.band, CornerRadius::ZERO, fade(BAND_FILL));
        painter.rect_stroke(
            zone.band.shrink(1.0),
            CornerRadius::same(2),
            Stroke::new(1.0, fade(BAND_EDGE)),
            egui::StrokeKind::Inside,
        );
    }
    let filling = aimed.filter(|aim| aim.fill).map(|aim| aim.side);
    if let Some(side) = chosen.filter(|_| landing.width() > 1.0 && landing.height() > 1.0) {
        landing_plate(painter, landing, carrying, side, filling.is_some());
    }
    for zone in bands {
        fill_target(painter, zone, filling == Some(zone.side));
    }
}

/// One band's `Fill whole side` target: the outer part of the band, strong while it is being aimed at.
///
/// Outlined with a dashed line, so the two parts of a band read as two places before the pointer reaches either:
/// the outer one fills the side, the inner one puts the panel on it.
fn fill_target(painter: &egui::Painter, zone: &Zone, aimed: bool) {
    let target = zone.whole.shrink(3.0);
    if target.width() < 8.0 || target.height() < 8.0 {
        return;
    }
    let (fill, ink) = match aimed {
        true => (color::accent(), color::on_accent()),
        false => (fade(BAND_FILL), color::text_strong()),
    };
    painter.rect_filled(target, CornerRadius::same(6), fill);
    let corners = [
        target.left_top(),
        target.right_top(),
        target.right_bottom(),
        target.left_bottom(),
        target.left_top(),
    ];
    for pair in corners.windows(2) {
        painter.extend(egui::Shape::dashed_line(&[pair[0], pair[1]], Stroke::new(1.0, color::accent()), 6.0, 4.0));
    }
    let label = painter.layout_no_wrap(fill_label(zone.side), egui::FontId::proportional(12.0), ink);
    let size = label.size();
    let upright = zone.side.is_a_column();
    let (room, across) = match upright {
        true => (target.height(), target.width()),
        false => (target.width(), target.height()),
    };
    if size.x + 8.0 > room || size.y > across {
        return;
    }
    // Up the edge on the left and the right, which is a quarter turn back about the galley's top left corner:
    // what was its width runs upwards from there and what was its height runs to the right.
    let shape = match upright {
        true => egui::epaint::TextShape::new(
            Pos2::new(target.center().x - size.y / 2.0, target.center().y + size.x / 2.0),
            label,
            ink,
        )
        .with_angle(-std::f32::consts::FRAC_PI_2),
        false => egui::epaint::TextShape::new(target.center() - size / 2.0, label, ink),
    };
    painter.add(shape);
}

/// What the `Fill whole side` target on `side` says: `Fill whole left`, `Fill whole bottom`.
pub fn fill_label(side: Side) -> String {
    format!("Fill whole {}", side.name())
}

/// The strong rectangle a panel would land in, with a plate in the middle saying what is about to happen:
/// `Terminal · bottom`, or `Terminal · whole bottom` when the side would fill.
fn landing_plate(painter: &egui::Painter, landing: Rect, carrying: Panel, side: Side, filling: bool) {
    painter.rect_filled(landing, CornerRadius::ZERO, fade(LANDING_FILL));
    painter.rect_stroke(
        landing.shrink(1.0),
        CornerRadius::same(3),
        Stroke::new(2.0, color::accent()),
        egui::StrokeKind::Inside,
    );
    // The panel's own name in the middle of where it is going, so a preview over an empty editing
    // area still says what is about to happen there.
    let name = match filling {
        true => format!("{} \u{b7} whole {}", carrying.label(), side.name()),
        false => format!("{} \u{b7} {}", carrying.label(), side.name()),
    };
    let label = painter.layout_no_wrap(name, egui::FontId::proportional(12.5), color::on_accent());
    let size = label.size();
    if size.x + 20.0 < landing.width() && size.y + 12.0 < landing.height() {
        let plate = Rect::from_center_size(landing.center(), size + Vec2::new(20.0, 12.0));
        painter.rect_filled(plate, CornerRadius::same(4), color::accent());
        painter.galley(plate.center() - size / 2.0, label, color::on_accent());
    }
}

/// The accent at a fraction of its opacity.
///
/// Written out rather than using `gamma_multiply`, because the overlay is painted over a window that
/// is already translucent and the alpha wanted here is a flat one a reader can check against the
/// numbers above.
fn fade(amount: f32) -> Color32 {
    let alpha = (255.0 * amount).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(
        color::accent().r(),
        color::accent().g(),
        color::accent().b(),
        alpha,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_overlay_is_the_accent_and_nothing_else() {
        // The palette is closed. A drop zone is the blue the ask names, which is the one blue the
        // design has, at three strengths rather than three colours.
        for amount in [BAND_FILL, BAND_EDGE, LANDING_FILL] {
            // Asked unmultiplied, because `Color32` keeps its alpha premultiplied and the question
            // here is which colour was chosen rather than what it comes out as over black.
            let [red, green, blue, alpha] = fade(amount).to_srgba_unmultiplied();
            // Within a few units of the accent: going through egui's premultiplied storage and back
            // again rounds, and the question is which colour was chosen rather than the last bit of it.
            for (painted, wanted) in [
                (red, color::accent().r()),
                (green, color::accent().g()),
                (blue, color::accent().b()),
            ] {
                assert!(painted.abs_diff(wanted) <= 4, "{painted} is not the accent's {wanted}");
            }
            assert!(alpha > 0 && alpha < 255);
        }
        assert!(
            fade(BAND_FILL).a() < fade(LANDING_FILL).a(),
            "the one being aimed at is the stronger"
        );
    }
}
