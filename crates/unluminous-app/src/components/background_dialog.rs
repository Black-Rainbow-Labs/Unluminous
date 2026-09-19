//! The grid of backgrounds: the desktop, the pictures Unluminous is keeping, and a way to add one.
//!
//! `task-2004`: *"Allow there to be Background Selector button that opens a new Modal with a grid of
//! options. The first option is Show Contents Underneath, which is what our current behavior is. The
//! other option should be to select a background image from disk … Previous selected background images
//! should be shown … each image should show a trashcan icon at the top right so I can remove it. Any
//! changes made/selected should immediately reflect in the IDE."*
//!
//! **It decides nothing**, which is every component in Unluminous: it says which cell was pressed and
//! `UnluminousApp` writes the setting, copies the file in or deletes it. "Immediately" needs no
//! machinery at all — the setting is what the window paints from and the window paints every frame.
//!
//! **The first cell is drawn rather than labelled.** It is the window's own ground over a chequerboard,
//! which is the one way of showing *transparency* that everybody already reads; a cell saying only
//! `Show Contents Underneath` would be a word where every other cell is a picture.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use crate::components::modal;
use crate::theme::crisp::CrispPainter;
use crate::theme::{color, icon, size};

const WIDTH: f32 = 620.0;
const HEIGHT: f32 = 460.0;

/// How wide a cell is, and how tall. Four to a row at the dialog's own width.
const CELL: Vec2 = Vec2::new(136.0, 96.0);
/// The gap between cells, sideways and down.
const GAP: f32 = 12.0;
/// How tall the strip under a cell holding its name is.
const CAPTION: f32 = 18.0;

/// What the dialog reported this frame.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// A cell was pressed: the file name, or an empty string for the desktop.
    pub chosen: Option<String>,
    /// A trashcan was pressed.
    pub remove: Option<String>,
    /// `Add a picture...` was pressed, which opens the platform's file picker.
    pub add: bool,
    /// Cancel, Escape or the close cross.
    pub closed: bool,
}

/// Everything the dialog draws with. A value rather than six arguments, which is `explorer::View`'s
/// reason.
pub struct Look<'a> {
    /// The pictures Unluminous is keeping, by name.
    pub names: &'a [String],
    /// Which one the setting names, or empty for the desktop.
    pub chosen: &'a str,
    /// The texture for a name, when one has been decoded. The dialog asks and never decodes.
    pub texture: &'a dyn Fn(&str) -> Option<egui::TextureHandle>,
    /// How opaque the window is, so the first cell shows what the desktop really looks like through it.
    pub opacity: f32,
    /// Why the last thing asked for was refused.
    pub problem: Option<&'a str>,
}

/// Draw the dialog.
pub fn show(ctx: &egui::Context, look: Look<'_>) -> Outcome {
    let (mut outcome, closed) = modal::show(
        ctx,
        "unluminous-background",
        WIDTH,
        HEIGHT,
        |ui, area| {
            let mut outcome = Outcome::default();
            if modal::header(ui, area, "Background") {
                outcome.closed = true;
            }
            let body = modal::body(area);
            let mut pen = modal::note(
                ui,
                body,
                body.top() + 2.0,
                "What is behind the window. Choosing one takes effect at once; the opacity slider on the Appearance page is how much of it shows through.",
            );
            pen += 6.0;

            // The cells, in rows across the body. The desktop first, then the pictures, then the one
            // that opens the file picker — which is last for the reason a plus is last in a tab strip:
            // the things that are there come before the way to add another.
            let across = ((body.width() + GAP) / (CELL.x + GAP)).floor().max(1.0) as usize;
            let mut index = 0usize;
            let cell_at = |index: usize| {
                let row = index / across;
                let column = index % across;
                Rect::from_min_size(
                    Pos2::new(
                        body.left() + column as f32 * (CELL.x + GAP),
                        pen + row as f32 * (CELL.y + CAPTION + GAP),
                    ),
                    CELL,
                )
            };

            if the_desktop(ui, cell_at(index), look.chosen.is_empty(), look.opacity) {
                outcome.chosen = Some(String::new());
            }
            index += 1;
            for name in look.names {
                let at = cell_at(index);
                index += 1;
                let pressed = a_picture(ui, at, name, name == look.chosen, (look.texture)(name));
                if pressed.chosen {
                    outcome.chosen = Some(name.clone());
                }
                if pressed.remove {
                    outcome.remove = Some(name.clone());
                }
            }
            if add_one(ui, cell_at(index)) {
                outcome.add = true;
            }

            if let Some(problem) = look.problem {
                let rows = index / across + 1;
                let under = pen + rows as f32 * (CELL.y + CAPTION + GAP);
                modal::label(
                    &ui.painter().clone(),
                    Rect::from_min_size(
                        Pos2::new(body.left(), under),
                        Vec2::new(body.width(), 20.0),
                    ),
                    body.left(),
                    problem,
                    color::close(),
                    12.0,
                );
            }

            if modal::footer(ui, area, &[("Done", true)]).is_some() {
                outcome.closed = true;
            }
            outcome
        },
    );
    outcome.closed |= closed;
    outcome
}

/// What one picture's cell reported.
struct Pressed {
    chosen: bool,
    remove: bool,
}

/// The cell that means "let the desktop show through", which is what Unluminous has always done.
fn the_desktop(ui: &mut egui::Ui, at: Rect, chosen: bool, opacity: f32) -> bool {
    let response = ui.interact(at, ui.id().with("background-desktop"), Sense::click());
    let painter = ui.painter().clone();
    // A chequerboard, which is how transparency is drawn everywhere, with the window's own ground over
    // it at the opacity the window really uses — so the cell shows what choosing it looks like.
    //
    // **Two colours the palette already has**, rather than two greys chosen here: `theme::color` is
    // closed, and a chequerboard is only asking for two surfaces that differ a little — which is what a
    // control and a field are everywhere else in the window.
    let squares = 8.0;
    let step = Vec2::new(at.width() / squares, at.height() / squares);
    for down in 0..squares as usize {
        for across in 0..squares as usize {
            let dark = (across + down) % 2 == 0;
            let square = Rect::from_min_size(
                Pos2::new(at.left() + across as f32 * step.x, at.top() + down as f32 * step.y),
                step,
            );
            let tint = match dark {
                true => color::control(),
                false => color::field(),
            };
            painter.rect_filled(square, CornerRadius::ZERO, tint);
        }
    }
    painter.rect_filled(
        at,
        CornerRadius::same(size::CONTROL_CORNER),
        crate::theme::faded(color::editor(), opacity),
    );
    frame_and_caption(ui, at, "Show Contents Underneath", chosen, response.hovered());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            true,
            chosen,
            "Background: Show Contents Underneath",
        )
    });
    response.clicked()
}

/// One picture's cell, with the trashcan at its top right.
fn a_picture(
    ui: &mut egui::Ui,
    at: Rect,
    name: &str,
    chosen: bool,
    texture: Option<egui::TextureHandle>,
) -> Pressed {
    let response = ui.interact(at, ui.id().with(("background", name)), Sense::click());
    let painter = ui.painter().clone();
    match &texture {
        Some(texture) => {
            let taken = crate::services::backgrounds::cover(texture.size_vec2(), at.size());
            painter.add(egui::Shape::Rect(
                egui::epaint::RectShape::filled(
                    at,
                    CornerRadius::same(size::CONTROL_CORNER),
                    Color32::WHITE,
                )
                .with_texture(texture.id(), taken),
            ));
        }
        // A picture that has not been decoded yet, or will not decode at all: the well it would fill,
        // rather than a hole. The name under it still says which one it is.
        None => {
            painter.rect_filled(at, CornerRadius::same(size::CONTROL_CORNER), color::field());
        }
    }
    frame_and_caption(ui, at, name, chosen, response.hovered());

    // The trashcan, over the picture's top right corner. Added **after** the cell, so it takes the
    // points it covers: egui gives a pointer to the last widget that asked for it — `components::dock`'s
    // own ordering rule, used here to keep a press on the bin from also choosing the picture.
    let bin = Rect::from_min_size(Pos2::new(at.right() - 26.0, at.top() + 4.0), Vec2::splat(22.0));
    let over = ui.interact(bin, ui.id().with(("background-remove", name)), Sense::click());
    if over.hovered() || response.hovered() {
        painter.rect_filled(bin, CornerRadius::same(4), Color32::from_black_alpha(140));
        icon::bin(
            &painter,
            bin.center(),
            match over.hovered() {
                true => color::close(),
                false => color::text_dim(),
            },
        );
        if over.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    over.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Remove {name}"))
    });
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            true,
            chosen,
            format!("Background: {name}"),
        )
    });
    Pressed { chosen: response.clicked(), remove: over.clicked() }
}

/// The cell that opens the file picker.
fn add_one(ui: &mut egui::Ui, at: Rect) -> bool {
    let response = ui.interact(at, ui.id().with("background-add"), Sense::click());
    let painter = ui.painter().clone();
    painter.rect(
        at,
        CornerRadius::same(size::CONTROL_CORNER),
        match response.hovered() {
            true => color::control(),
            false => color::field(),
        },
        Stroke::new(1.0, color::control_border()),
        egui::StrokeKind::Inside,
    );
    icon::plus(&painter, at.center(), color::text_dim());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    frame_and_caption(ui, at, "Add a picture...", false, false);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Add a picture"));
    response.clicked()
}

/// The ring round a cell and the name under it, which every cell has.
///
/// The chosen one carries the accent ring every chosen row in Unluminous has, so a grid and a list say
/// the same thing the same way.
fn frame_and_caption(ui: &mut egui::Ui, at: Rect, name: &str, chosen: bool, hovered: bool) {
    let painter = ui.painter().clone();
    let stroke = match (chosen, hovered) {
        (true, _) => Stroke::new(2.0, color::accent()),
        (false, true) => Stroke::new(1.0, color::text_dim()),
        (false, false) => Stroke::new(1.0, color::control_border()),
    };
    painter.rect_stroke(
        at,
        CornerRadius::same(size::CONTROL_CORNER),
        stroke,
        egui::StrokeKind::Inside,
    );
    let tint = match chosen {
        true => color::text_strong(),
        false => color::text_dim(),
    };
    // Cut short rather than run into the cell beside it: a wallpaper's file name is often long.
    let shortened = crate::components::controls::truncate_chars(name, 22, 21);
    painter.crisp_text(
        Pos2::new(at.center().x, at.bottom() + CAPTION / 2.0),
        egui::Align2::CENTER_CENTER,
        &shortened,
        egui::FontId::proportional(11.0),
        tint,
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}
