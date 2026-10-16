//! The furniture every modal in Unluminous is built from.
//!
//! `design/style-guide.md` says what a modal is: an `egui::Modal` over a backdrop of
//! `from_black_alpha(120)`, filled `EXPLORER`, a one point `CONTROL_BORDER` stroke, corner radius
//! 10, a 46 point header filled `TITLE_BAR` with the title at the left and a close cross at the
//! right, and a 52 point footer with its buttons at the right.
//!
//! The Settings window is where that shape came from. It is written down here because the commit
//! panel, the seven git dialogs, the text prompt and the confirmation are all the same shape, and
//! ten copies of it would be ten modals that almost agree.
//!
//! ## Every modal is moved and resized the same way
//!
//! `task-1659` asks for modals that can be dragged and resized, and both live in [`show`] rather
//! than in any one dialog, so a dialog written later gets them without asking. The header is the
//! handle: dragging it moves the modal, and double clicking it puts the modal back in the middle at
//! the size its dialog asked for. Eight invisible grips round the edge resize it — the same four
//! edges and four corners `components::resize_edges` gives the window itself.
//!
//! Where it has been dragged to and how much bigger it has been made are kept in egui's own memory
//! under the modal's id rather than in `UnluminousApp`. A modal's geometry belongs to the modal: the
//! window has no decision to make about it, nothing is written to disk, and a dialog closed and
//! opened again is where it was left, which is what every other application does.
//!
//! ## Every modal zooms
//!
//! `task-2198`: *"I should be able to zoom modals, such as the agent tasks modal."* Control or command with
//! the wheel, a pinch, or control or command with plus and minus, while a modal is open, make the modal
//! bigger or smaller; `unluminous-cli modal zoom` is the same thing for an agent. It is the way a canvas node
//! is zoomed: the modal is drawn into its own layer through a `TSTransform` about its top left corner, and
//! it is laid out in a rectangle the zoom times smaller, so it fills the same place on the screen with
//! everything in it larger. The dialog itself knows nothing about it. Its rectangle is smaller, and a
//! dialog already lays itself out in whatever rectangle it is given.
//!
//! The zoom is kept in egui's memory under the modal's id, beside where it has been dragged to and for the
//! same reason: it belongs to the modal, and a modal closed and opened again comes back at it. Text in the
//! layer is laid out again at the size it is seen at once the modal has been drawn
//! (`crisp::sharpen_the_text_in`), which is what keeps a zoomed modal's words from being magnified bitmaps.
//!
//! Two rules about the order things are added in, and both are why they work at all. The **drag
//! strip is added before the contents**, so the close cross the header draws sits over it and a
//! click on the cross closes the modal rather than starting a drag. The **grips are added after the
//! contents**, for the reason `components::resize_edges` records about the window's own grips: egui
//! gives a pointer to the last widget that wants it, and a list or a field reaching the modal's edge
//! would otherwise take a drag meant for the edge.

use egui::emath::TSTransform;
use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use rux::components::{ChipButton, PillButton};
use rux::icon::{Icon, Mark};
use rux::{Rux, Style};

use crate::theme::{color, size};

pub const HEADER: f32 = 46.0;
pub const FOOTER: f32 = 52.0;

/// The smallest a modal can be resized down to, whatever size its dialog asked for.
///
/// Small enough to be useful on a laptop screen and large enough that the header, a row of the body
/// and the footer's buttons all still fit, which is what stops a modal being dragged into a shape it
/// cannot be read in.
pub const MIN_WIDTH: f32 = 320.0;
pub const MIN_HEIGHT: f32 = 220.0;

/// How far in from a modal's edge it can be grabbed, and how far along each edge a corner reaches.
///
/// The same numbers `components::resize_edges` uses for the window, because they answer the same
/// question: a one point target cannot be hit with a mouse, and a corner has to win where it
/// overlaps an edge.
/// How far past a dialog's edge [`shadow`] reaches: twenty four points down and a blur radius of forty
/// eight, which is a Gaussian of twenty four that has faded to nothing three of them out.
pub const SHADOW_REACH: f32 = 100.0;

/// The shadow under a dialog that paints its own surface: one long soft dark shadow and nothing else.
///
/// `rux`'s `floating` elevation also carries a pale shadow up and to the left, which is neumorphism's way
/// of saying a surface is lifted out of the ground it is made of. A dialog is not made of the window under
/// it, it is a layer over it, so the pale half read as a light source behind the dialog (`task-2247`).
pub fn shadow() -> rux::Elevation {
    let dark = Color32::from_black_alpha(140); // any ground: a shadow under a layer is dark on a light ground too
    rux::Elevation::of([rux::Shadow::outer(0.0, 24.0, 48.0, dark)])
}

const EDGE: f32 = 6.0;
const CORNER: f32 = 16.0;

// `CORNER` and `EDGE` are both constants, so a runtime `assert!` of one against the other can only
// ever pass or fail the same way. Checked at compile time instead: a build fails before it ever
// draws a corner grip that would lose to an edge grip where they overlap.
const _: () = assert!(CORNER > EDGE, "the corners have to reach further than the edges");

/// Where a modal has been dragged to, and how much bigger it has been made than its dialog asked.
///
/// Held as a difference rather than as a rectangle so that a modal follows the window: making the
/// Unluminous window larger moves a modal that was dragged to one side along with the middle it was
/// dragged from, and the size a dialog asks for is still the size it gets the first time it opens.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Placement {
    /// How far from the middle of the window the modal has been dragged.
    pub offset: Vec2,
    /// How much wider and taller than its dialog asked the modal has been made.
    pub grown: Vec2,
}

impl Placement {
    /// True while the modal is where a modal starts: in the middle, at the size it asked for.
    pub fn is_untouched(&self) -> bool {
        *self == Self::default()
    }
}

fn placement_id(id: &str) -> egui::Id {
    egui::Id::new(id).with("placement")
}

/// Where the modal with this id has been dragged to and how much it has been resized.
pub fn placement(ctx: &egui::Context, id: &str) -> Placement {
    ctx.data(|data| data.get_temp::<Placement>(placement_id(id))).unwrap_or_default()
}

/// Put a modal back in the middle at the size its dialog asks for, which is what double clicking its
/// header does.
pub fn reset_placement(ctx: &egui::Context, id: &str) {
    ctx.data_mut(|data| data.remove::<Placement>(placement_id(id)));
}

fn remember_placement(ctx: &egui::Context, id: &str, placement: Placement) {
    ctx.data_mut(|data| data.insert_temp(placement_id(id), placement));
}

/// Put the modal with this id where a caller says, which is what `unluminous-cli modal move` and
/// `modal size` do. Dragging the header and the grips go through the same value.
pub fn set_placement(ctx: &egui::Context, id: &str, placement: Placement) {
    remember_placement(ctx, id, placement);
}

fn zoom_id(modal: egui::Id) -> egui::Id {
    modal.with("zoom")
}

/// How much bigger than its own size the modal with this egui id is drawn. One until somebody zooms it.
pub fn zoom_of(ctx: &egui::Context, modal: egui::Id) -> f32 {
    ctx.data(|data| data.get_temp::<f32>(zoom_id(modal))).unwrap_or(crate::settings::DEFAULT_ZOOM)
}

/// Draw the modal with this egui id at `zoom`, which is kept between [`crate::settings::MIN_ZOOM`] and
/// [`crate::settings::MAX_ZOOM`], the range every pane's zoom has.
pub fn set_zoom(ctx: &egui::Context, modal: egui::Id, zoom: f32) {
    let zoom = zoom.clamp(crate::settings::MIN_ZOOM, crate::settings::MAX_ZOOM);
    ctx.data_mut(|data| match (zoom - crate::settings::DEFAULT_ZOOM).abs() < 0.001 {
        true => data.remove::<f32>(zoom_id(modal)),
        false => {
            data.insert_temp(zoom_id(modal), zoom);
        }
    });
    ctx.request_repaint();
}

/// Take `steps` up or down the list of zooms every pane walks, which is what one notch of the wheel and
/// one press of control and plus are each worth.
pub fn step_zoom(ctx: &egui::Context, modal: egui::Id, steps: i32) {
    let mut zoom = zoom_of(ctx, modal);
    for _ in 0..steps.abs() {
        zoom = crate::settings::step_zoom(zoom, steps > 0);
    }
    set_zoom(ctx, modal, zoom);
}

/// The egui id of the modal in front, from the frame before this one, or `None` when no modal is open.
///
/// egui's own answer, because egui is what keeps the modals in order: every modal Unluminous draws goes
/// through [`show`], and [`show`] gives its layer the modal's own id, so the id of the top modal layer is
/// the id its zoom is kept under.
pub fn the_open_one(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.memory(|memory| memory.top_modal_layer()).map(|layer| layer.id)
}

/// The transform a modal at `zoom` is drawn through: a scale about `corner`, its top left corner on the
/// screen, so the corner stays where [`place`] put it and everything else grows away from it.
fn zoomed_about(corner: Pos2, zoom: f32) -> TSTransform {
    TSTransform { scaling: zoom, translation: corner.to_vec2() * (1.0 - zoom) }
}

fn drawn_id(id: &str) -> egui::Id {
    egui::Id::new(id).with("drawn")
}

/// The rectangle the modal with this id last filled, in window points.
///
/// Remembered by [`show`] on every frame, because where a modal really is depends on the size of
/// the window as well as on how far it has been dragged, and a caller asking to move it to a place
/// needs the answer rather than the arithmetic behind it. `None` before the modal has been drawn
/// once.
pub fn drawn(ctx: &egui::Context, id: &str) -> Option<Rect> {
    ctx.data(|data| data.get_temp::<Rect>(drawn_id(id)))
}

/// The same, for the modal whose egui id is `modal`, which is what [`the_open_one`] answers with.
///
/// The dialog's own rectangle and not its layer's: egui's `Modal` puts a backdrop the size of the window in
/// the same layer, so `Context::layer_id_at` answers with the modal's layer everywhere in the window, the
/// title bar included (`task-2255`).
pub fn drawn_by_id(ctx: &egui::Context, modal: egui::Id) -> Option<Rect> {
    ctx.data(|data| data.get_temp::<Rect>(modal.with("drawn")))
}

fn remember_drawn(ctx: &egui::Context, id: &str, rect: Rect) {
    ctx.data_mut(|data| data.insert_temp(drawn_id(id), rect));
}

/// What a modal's own controls are called, worked out from its id.
///
/// Every control in Unluminous has a plain name, and the drag strip and the eight grips are controls. The
/// id is what [`show`] is given, so `unluminous-find-in-files` names them `Move find in files` and
/// `Resize find in files: bottom right`, which is what a test asks for and what assistive technology
/// reads out.
fn plain_name(id: &str) -> String {
    id.trim_start_matches("unluminous-").replace('-', " ")
}

/// Open a modal of a given size and draw `contents` into it.
///
/// The size is the one the dialog asks for plus whatever it has been resized by, capped to the
/// window, so a modal in a small Unluminous window is smaller rather than running off the edges. Where it
/// sits is the middle of the window plus whatever it has been dragged by, clamped so that a modal
/// cannot be dragged out of the window and lost.
pub fn show<R>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    height: f32,
    contents: impl FnOnce(&mut egui::Ui, Rect) -> R,
) -> (R, bool) {
    show_framed(ctx, id, width, height, true, contents)
}

/// The same, with no frame: the dialog paints its own surface, and may let its shadow fall past its edge.
///
/// The Agent-Tasks ticket (`task-2241`) is a `rux` surface thirty points round with a long soft shadow
/// under it, which the window's own ten point frame would show through at the corners and its clip would
/// cut off.
pub fn show_unframed<R>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    height: f32,
    contents: impl FnOnce(&mut egui::Ui, Rect) -> R,
) -> (R, bool) {
    show_framed(ctx, id, width, height, false, contents)
}

fn show_framed<R>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    height: f32,
    framed: bool,
    contents: impl FnOnce(&mut egui::Ui, Rect) -> R,
) -> (R, bool) {
    let mut placement = placement(ctx, id);
    let (position, size) = place(ctx, width, height, &mut placement);
    remember_drawn(ctx, id, Rect::from_min_size(position, size));
    // **Zoomed the way a canvas node is**: through a transform on the modal's own layer, with the dialog laid
    // out in a rectangle `zoom` times smaller so it fills the same place on the screen. egui leaves a layer
    // with the identity transform alone, so a modal nobody has zoomed is drawn exactly as it always was.
    let zoom = zoom_of(ctx, egui::Id::new(id));
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new(id));
    let to_screen = zoomed_about(position, zoom);
    ctx.set_transform_layer(layer, to_screen);
    let laid_out = size / zoom;
    // The whole window, in the layer's own points, which is what the backdrop has to cover.
    let window = to_screen.inverse().mul_rect(ctx.content_rect());
    let backdrop = blurred_backdrop(ctx, egui::Id::new(id));
    // Glass in the dialog bends the photograph of the window behind it (`task-2281`), which is the
    // design's frosted modal: the blur is already in the picture, and what bends is the window's own
    // contents rather than the picture behind them.
    if let Some(texture) = backdrop.texture.as_ref().filter(|_| crate::theme::is_glass()) {
        rux::render::set_backdrop(
            ctx,
            rux::render::Plane::Above,
            Some(rux::render::Backdrop::single(
                texture.id(),
                ctx.content_rect(),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                1.0,
            )),
        );
    }

    let response = egui::Modal::new(egui::Id::new(id))
        .area(
            // `Modal::default_area` anchors the area to the middle of the window, and egui applies
            // an anchor *after* a fixed position rather than instead of it, so a modal that can be
            // dragged has to build its own area. This is that area with the anchor left off.
            egui::Area::new(egui::Id::new(id))
                .kind(egui::UiKind::Modal)
                .sense(Sense::hover())
                .order(egui::Order::Foreground)
                .interactable(true)
                // `fit` has already kept the modal inside the window, in the window's points. egui's own
                // clamp would measure the zoomed layer's points against the window and move it.
                .constrain(false)
                .fixed_pos(position),
        )
        // **The backdrop and the frame are painted here rather than by egui**, the same shapes egui paints, so
        // that a modal zoomed out still darkens the whole window. egui paints its backdrop over the window's
        // rectangle in the layer's own points, which a zoom below one would shrink into a dark patch in the
        // middle. A frame with nothing but its one point of margin keeps the dialog's rectangle exactly where
        // egui's frame put it.
        .backdrop_color(Color32::TRANSPARENT)
        .frame(egui::Frame::NONE.inner_margin(1))
        .show(ctx, |ui| {
            if backdrop.hide {
                // The frame the window is photographed in, so the photograph has nothing of the dialog.
                ui.set_opacity(0.0);
            }
            // **Assigned rather than intersected** (`task-2253`: *"When i zoom in/out of a modal, the black
            // background stops filling the window"*). `Painter::with_clip_rect` intersects with the area's own
            // clip, which is the window measured in the window's points; at a zoom below one the window in the
            // layer's points is larger than that, so the intersection left the edges of the window uncovered.
            let mut covering = ui.painter().clone();
            covering.set_clip_rect(window);
            if let Some(texture) = &backdrop.texture {
                covering.image(
                    texture.id(),
                    window,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE, // any ground: a white tint draws the picture in its own colours
                );
            }
            // Clipped to the window rather than to the area, whose clip is the window measured in the
            // layer's own points: at a zoom below one that left the top and the left of the window uncovered.
            covering.rect_filled(window, CornerRadius::ZERO, color::scrim());
            let (area, _) = ui.allocate_exact_size(laid_out, Sense::hover());
            let window_clip = ui.clip_rect();
            // **The surface is the redesign's** (`task-2276`): the pane's own ground, thirty points round, with
            // the one long shadow a layer over the window casts, which is what the dialogs that paint their own
            // surface already draw. Reserved before the contents, so everything a dialog draws is over it.
            if framed {
                ui.set_clip_rect(area.expand(SHADOW_REACH).intersect(window_clip));
                crate::components::settings_kit::layer_reaching(
                    ui,
                    ("modal-surface", id),
                    area,
                    SHADOW_REACH,
                    |rux| {
                        let ground = rux.theme().chat.ground;
                        rux.chrome.surface(area, RADIUS, ground, shadow());
                    },
                );
            }
            // **Nothing a dialog draws reaches past its own frame.** The area's clip was the whole window, so a
            // section that ran out of room painted over the backdrop under the modal, which a zoom makes easy to
            // reach: a modal zoomed in has less room in its own points. `task-2198`. A dialog with no frame
            // keeps room round it for the shadow it paints; a framed one keeps room for the shadow of a chip or a
            // well at its edge.
            let reach = match framed {
                true => 12.0,
                false => SHADOW_REACH,
            };
            ui.set_clip_rect(area.expand(reach).intersect(window_clip));
            // Before the contents, so the close cross the header draws sits over the strip.
            let moved = drag_strip(ui, area, id, &mut placement, zoom);
            let inner = contents(ui, area);
            // After the contents, for the reason `components::resize_edges` gives.
            let resized = grips(ui, area, id, &mut placement, zoom);
            (inner, moved || resized)
        });
    // A modal's zoom moves in steps rather than gliding, so its words are laid out at exactly the zoom.
    crate::theme::crisp::sharpen_the_text_in(
        ctx,
        &[layer],
        crate::services::text_renderer::Crispness::ladder(zoom, true),
    );

    // Escape closes any modal, which is the one thing every dialog on every platform agrees about. Read
    // before `should_close`, which takes the key out of the frame.
    let escaped = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    // A press on the title bar moves the window rather than dismissing the dialog (`task-2247`): with a
    // modal open the bar starts the window's drag itself, and a press there that does not move is not a
    // click outside the dialog that means "close".
    let pointer = ctx.input(|input| input.pointer.interact_pos());
    let on_the_title_bar = pointer.is_some_and(|at| {
        at.y < size::TITLE_BAR && drawn(ctx, id).is_none_or(|dialog| !dialog.contains(at))
    });
    let should_close = response.should_close() && !on_the_title_bar;
    let (inner, changed) = response.inner;
    if changed {
        remember_placement(ctx, id, placement);
    }
    (inner, should_close || escaped)
}

/// What a modal's backdrop draws on this frame: the blurred picture of the window once there is one, and
/// whether this is the frame the picture is being taken in.
struct Backdrop {
    texture: Option<egui::TextureHandle>,
    hide: bool,
}

/// What [`blurred_backdrop`] keeps between frames.
#[derive(Clone)]
struct BlurState {
    /// The theme's colours when it was taken. A change of theme takes it again.
    ///
    /// **Not the window's size** (`task-2255`: *"When a modal is open and I resize, there are many flashes and
    /// glitchiness of the modal content"*). Every frame of a resize changed the size, so every frame asked for
    /// a new picture and drew the modal at no opacity while it was taken, and blurred a whole window on the
    /// next. The picture is stretched to the window and is blurred past reading, so the one taken when the
    /// modal opened serves at any size.
    ///
    /// And which background picture the window was drawn on, from `backgrounds::changes`, so a picture that
    /// arrives or is chosen while a modal is open is in the backdrop too (`task-2279`).
    key: ([u8; 12], u64),
    /// When the modal was last drawn, in egui's seconds, so a modal opened again is photographed again.
    ///
    /// A time rather than a pass number: egui can run more than one pass in a frame, and a gap of two passes
    /// read as the modal having been closed, so a screenshot was asked for on every frame. In a test window
    /// that is a second a frame, which is what `every_git_operation_can_be_driven_from_the_window` found.
    drawn: f64,
    /// When the picture was asked for, in egui's seconds; `None` once it has arrived or been given up on.
    asked: Option<f64>,
    texture: Option<egui::TextureHandle>,
}

/// How long a screenshot is waited for before the backdrop settles for the plain dark one. A test's window
/// never answers one.
const BLUR_WAIT: f64 = 1.0;

/// How long a modal can go undrawn and still be the same opening of it. A test's frame is a quarter of a
/// second, so this is several frames.
const GAP: f64 = 1.5;

/// The blurred picture of what is behind the modal with `modal` as its id (`task-2247`). See
/// `services::backdrop_blur` for why it is a photograph.
fn blurred_backdrop(ctx: &egui::Context, modal: egui::Id) -> Backdrop {
    // **One for each modal**, because two can be open at once, a confirmation over the commit panel. Kept in
    // one slot they each read the other's as a modal that had just opened, and asked for a screenshot and
    // drew themselves invisible on every frame.
    let slot = egui::Id::new("unluminous-modal-backdrop").with(modal);
    let now = ctx.input(|input| input.time);
    let mut tones = [0u8; 12];
    for (at, colour) in [color::editor(), color::text(), color::accent()].iter().enumerate() {
        tones[at * 4..at * 4 + 4].copy_from_slice(&colour.to_array());
    }
    let key = (tones, crate::services::backgrounds::changes(ctx));
    let kept: Option<BlurState> = ctx.data(|data| data.get_temp(slot));
    let mut hide = false;
    let mut state = match kept {
        Some(state) if state.key == key && now - state.drawn <= GAP => state,
        _ => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                crate::services::backdrop_blur::Marker,
            )));
            ctx.request_repaint();
            hide = true;
            BlurState { key, drawn: now, asked: Some(now), texture: None }
        }
    };
    state.drawn = now;
    if let Some(asked) = state.asked.filter(|_| !hide) {
        let picture = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, user_data, .. }
                    if crate::services::backdrop_blur::is_ours(user_data) =>
                {
                    Some(image.clone())
                }
                _ => None,
            })
        });
        if let Some(picture) = picture {
            let blurred = crate::services::backdrop_blur::blurred(&picture);
            state.texture = Some(ctx.load_texture(
                "unluminous-modal-backdrop",
                blurred,
                egui::TextureOptions::LINEAR,
            ));
            state.asked = None;
        } else if now - asked > BLUR_WAIT {
            state.asked = None;
        } else {
            ctx.request_repaint();
        }
    }
    let texture = state.texture.clone();
    ctx.data_mut(|data| data.insert_temp(slot, state));
    Backdrop { texture, hide }
}

/// Where a modal of this size sits, and how big it really comes out.
///
/// Split from [`show`] so that the placement it settles on can be read back by a caller, and so the
/// clamping is one function rather than four numbers repeated inside the closure.
pub fn place(
    ctx: &egui::Context,
    width: f32,
    height: f32,
    placement: &mut Placement,
) -> (Pos2, Vec2) {
    let available = ctx.content_rect();
    let asked = Vec2::new(width, height);
    let (size, offset) = fit(available.size(), asked, placement.grown, placement.offset);
    placement.grown = size - asked;
    placement.offset = offset;
    (available.center() - size / 2.0 + offset, size)
}

/// The arithmetic behind [`place`], with no context in it so a test can check it.
///
/// `asked` is the size the dialog wants, `grown` how much it has been resized by and `offset` how
/// far it has been dragged. Returns the size it really gets and the offset it really sits at: never
/// larger than the room there is, never smaller than [`MIN_WIDTH`] by [`MIN_HEIGHT`], and never
/// dragged so far that any of it leaves the window.
pub fn fit(window: Vec2, asked: Vec2, grown: Vec2, offset: Vec2) -> (Vec2, Vec2) {
    let widest = (window.x - 40.0).max(120.0);
    let tallest = (window.y - 40.0).max(120.0);
    // A dialog that asks for less than the floor still gets what it asked for; the floor is about how
    // far a modal can be dragged *down* to. The text prompt is 190 points tall on purpose, and the
    // smallest useful size for a dialog with a list in it is not a reason to make it taller.
    let floor = Vec2::new(MIN_WIDTH.min(asked.x).min(widest), MIN_HEIGHT.min(asked.y).min(tallest));
    let size = Vec2::new(
        (asked.x + grown.x).clamp(floor.x, widest),
        (asked.y + grown.y).clamp(floor.y, tallest),
    );
    let room = Vec2::new(
        ((window.x - size.x) / 2.0 - 8.0).max(0.0),
        ((window.y - size.y) / 2.0 - 8.0).max(0.0),
    );
    let offset = Vec2::new(offset.x.clamp(-room.x, room.x), offset.y.clamp(-room.y, room.y));
    (size, offset)
}

/// The header, as something to drag the modal by. Returns true when it moved.
///
/// A double click puts the modal back where it started at the size it started, which is what double
/// clicking a divider does to a pane in `components::splitter`.
///
/// `zoom` is what the modal is drawn at. A drag is reported in the layer's own points and a placement is
/// kept in the window's, so a drag across a modal at twice its size moves it twice as far as it reports.
fn drag_strip(
    ui: &mut egui::Ui,
    area: Rect,
    id: &str,
    placement: &mut Placement,
    zoom: f32,
) -> bool {
    let strip = Rect::from_min_size(area.min, Vec2::new(area.width(), HEADER));
    let response = ui.interact(strip, ui.id().with(("modal-move", id)), Sense::click_and_drag());
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    let name = format!("Move {}", plain_name(id));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), name.clone())
    });
    if response.double_clicked() {
        *placement = Placement::default();
        return true;
    }
    if response.dragged() && response.drag_delta() != Vec2::ZERO {
        placement.offset += response.drag_delta() * zoom;
        return true;
    }
    false
}

/// The eight grips round a modal's edge. Returns true when one of them was dragged.
///
/// Dragging an edge keeps the opposite edge where it is, which is what resizing means everywhere
/// else. The modal is positioned from its middle, so growing it by `d` moves that middle by `d / 2`
/// towards the edge being dragged; that is the whole of the arithmetic here.
///
/// `zoom` is what the modal is drawn at, for the reason [`drag_strip`] gives.
fn grips(ui: &mut egui::Ui, area: Rect, id: &str, placement: &mut Placement, zoom: f32) -> bool {
    let mut moved = false;
    for (name, side, rect, cursor) in edges_and_corners(area) {
        let response = ui.interact(rect, ui.id().with(("modal-resize", id, name)), Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        let label = format!("Resize {}: {name}", plain_name(id));
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), label.clone())
        });
        if !response.dragged() {
            continue;
        }
        let drag = response.drag_delta() * zoom;
        let grown = Vec2::new(side.0 as f32 * drag.x, side.1 as f32 * drag.y);
        if grown == Vec2::ZERO {
            continue;
        }
        placement.grown += grown;
        placement.offset += Vec2::new(side.0 as f32 * grown.x, side.1 as f32 * grown.y) / 2.0;
        moved = true;
    }
    moved
}

/// The eight rectangles a modal is resized by, each with which way dragging it grows the modal.
///
/// The side is `-1`, `0` or `1` on each axis, and it means "dragging this way makes the modal
/// larger": the right edge is `(1, 0)` and the top left corner is `(-1, -1)`. The corners come last
/// so they win where they overlap an edge, exactly as they do for the window itself.
fn edges_and_corners(area: Rect) -> [(&'static str, (i32, i32), Rect, egui::CursorIcon); 8] {
    let width = area.width();
    let height = area.height();
    [
        (
            "top",
            (0, -1),
            Rect::from_min_size(area.left_top(), Vec2::new(width, EDGE)),
            egui::CursorIcon::ResizeNorth,
        ),
        (
            "bottom",
            (0, 1),
            Rect::from_min_size(
                Pos2::new(area.left(), area.bottom() - EDGE),
                Vec2::new(width, EDGE),
            ),
            egui::CursorIcon::ResizeSouth,
        ),
        (
            "left",
            (-1, 0),
            Rect::from_min_size(area.left_top(), Vec2::new(EDGE, height)),
            egui::CursorIcon::ResizeWest,
        ),
        (
            "right",
            (1, 0),
            Rect::from_min_size(
                Pos2::new(area.right() - EDGE, area.top()),
                Vec2::new(EDGE, height),
            ),
            egui::CursorIcon::ResizeEast,
        ),
        (
            "top left",
            (-1, -1),
            Rect::from_min_size(area.left_top(), Vec2::splat(CORNER)),
            egui::CursorIcon::ResizeNorthWest,
        ),
        (
            "top right",
            (1, -1),
            Rect::from_min_size(area.right_top() - Vec2::new(CORNER, 0.0), Vec2::splat(CORNER)),
            egui::CursorIcon::ResizeNorthEast,
        ),
        (
            "bottom left",
            (-1, 1),
            Rect::from_min_size(area.left_bottom() - Vec2::new(0.0, CORNER), Vec2::splat(CORNER)),
            egui::CursorIcon::ResizeSouthWest,
        ),
        (
            "bottom right",
            (1, 1),
            Rect::from_min_size(area.right_bottom() - Vec2::splat(CORNER), Vec2::splat(CORNER)),
            egui::CursorIcon::ResizeSouthEast,
        ),
    ]
}

/// How round a dialog is, and the round close chip at its top right.
///
/// `task-2276` drew every dialog built from this furniture in the redesign: the surface is the New Data
/// Source dialog's and the realm manager's, the pane's own ground thirty points round with one long soft
/// shadow under it, and the parts inside it are the Settings window's.
pub const RADIUS: f32 = 28.0;
const CLOSE: f32 = 30.0;

/// The design's words: a heading, a button, a label, a note and a section's name.
fn heading_style() -> Style {
    Style::sans(16.0).semibold().tracking(-0.01)
}

fn section_style() -> Style {
    Style::sans(10.5).semibold().tracking(0.08)
}

/// Draw with `rux` over `rect`, with the window's state. Every part of a dialog that has depth goes
/// through here, so a dialog that is not a `rux` dialog still gets the same surfaces.
fn decorate<R>(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    rect: Rect,
    add: impl FnOnce(&mut Rux<'_>) -> R,
) -> R {
    crate::components::settings_kit::layer(ui, ("modal-furniture", salt), rect, add)
}

/// The top of a dialog: the title at the left and the round close chip at the right. Returns true when
/// the chip was pressed.
///
/// The top is also what the modal is dragged by. That is not added here: [`show`] adds the drag strip
/// before the contents so that this function's close chip sits over it.
pub fn header(ui: &mut egui::Ui, area: Rect, title: &str) -> bool {
    header_of(ui, area, None, title)
}

/// The same, with a **key** in front of the title.
///
/// `task-1771` asks for the ticket modal to read like the page it is modelled on, and that page's header is
/// `task-1766` in a dim monospaced face and then the ticket's title beside it. Everything else about a
/// modal's header — the cross, the drag handle, the name a test finds it by — is unchanged, so this is one
/// dialog carrying one extra word rather than a second kind of header.
pub fn header_of(ui: &mut egui::Ui, area: Rect, key: Option<&str>, title: &str) -> bool {
    let bar = Rect::from_min_size(area.min, Vec2::new(area.width(), HEADER));
    // The title and the chip sit a little below the middle of the 46 points, because the surface's corner is
    // thirty points round and words centred in the band look pushed into it.
    let middle = bar.center().y + 4.0;
    let close = Rect::from_center_size(Pos2::new(area.right() - 30.0, middle), Vec2::splat(CLOSE));
    let mut title_rect = Rect::NOTHING;
    let pressed = decorate(ui, ("header", title), bar, |rux| {
        let pressed = ChipButton::new(Icon::X, CLOSE, "Close")
            .mark(11.0, Some(1.8))
            .show(rux, close)
            .clicked();
        let ink = rux.theme().ink;
        let mut pen = area.left() + 26.0;
        if let Some(key) = key {
            let style = Style::mono(12.0);
            let galley = rux.text(style, key, ink.i400);
            let width = galley.size().x;
            rux::text::draw_left_capitals(
                rux.painter(),
                Pos2::new(pen, middle),
                galley,
                style,
                ink.i400,
            );
            pen += width + 12.0;
        }
        let galley =
            rux.elided(heading_style(), title, ink.i900, (close.left() - 16.0 - pen).max(40.0));
        title_rect =
            Rect::from_min_size(Pos2::new(pen, middle - 9.0), Vec2::new(galley.size().x, 18.0));
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(pen, middle),
            galley,
            heading_style(),
            ink.i900,
        );
        pressed
    });
    // The title is **named** as well as drawn, so a test and assistive technology can say which modal is open.
    // Every control in Unluminous has a plain name and a test finds one by it; a modal's title is what says which of
    // the dialogs it is, and painted text alone is invisible to both. The drag handle is under this and takes
    // the presses, so naming it costs the header nothing.
    let named = ui.interact(title_rect, ui.id().with(("modal-title", title)), Sense::hover());
    let name = title.to_owned();
    named.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name.clone()));
    pressed
}

/// The rectangle between the header and the footer, inset by the usual margin.
pub fn body(area: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(area.left() + 24.0, area.top() + HEADER + 14.0),
        Pos2::new(area.right() - 24.0, area.bottom() - FOOTER - 8.0),
    )
}

/// What presses a modal's primary button from the keyboard.
///
/// `task-1682` asks that a modal be answerable without reaching for the pointer: somebody who has
/// just typed a name into a field should not have to move their hand to press `Create`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    /// Enter, which is what every modal in Unluminous but one uses.
    Enter,
    /// The command key with Enter, for a modal whose **body** owns Enter: the commit message is a
    /// multiline field where Enter is a new line, and a new line is what a person pressing it there
    /// means. The reference editor's commit dialog makes the same choice for the same reason.
    CommandEnter,
}

impl Confirm {
    /// Whether the key press that presses the primary button happened this frame.
    ///
    /// The press is **taken out of the frame's input**, so a modal that is confirmed from the
    /// keyboard cannot also be read as an ordinary Enter by anything drawn after it. The list
    /// dialogs — `Go to File`, `Find in Files` and the references modal — take Enter for
    /// themselves before their footer is drawn, which is what stops it meaning two things there:
    /// in those, Enter opens the row that is chosen and the button at the bottom right is the same
    /// thing said twice.
    pub fn pressed(&self, ui: &egui::Ui) -> bool {
        // Asked of what is really held rather than through `consume_key`, which matches by
        // `Modifiers::matches_logically`: that only asks whether the modifiers the *pattern* names
        // are held, so a pattern of `NONE` matches `Command+Enter` too and the commit dialog would
        // commit on both. `task-1678`'s completion popup had to make the same comparison.
        //
        // `command_only` rather than an equality test, because the command key is not one flag:
        // on Windows `Ctrl+Enter` arrives with **both** `ctrl` and `command` set, so a modal that
        // compared against `Modifiers::COMMAND` would work in a test and never in the window.
        let wanted = |held: &egui::Modifiers| match self {
            Confirm::Enter => held.is_none(),
            Confirm::CommandEnter => held.command_only(),
        };
        ui.input_mut(|input| {
            let mut pressed = false;
            input.events.retain(|event| match event {
                egui::Event::Key {
                    key: egui::Key::Enter, pressed: true, modifiers: held, ..
                } if wanted(held) => {
                    pressed = true;
                    false
                }
                _ => true,
            });
            pressed
        })
    }
}

/// The foot of the dialog, with its buttons at the right.
///
/// `buttons` is given in the order they read, left to right; the last one is the one that does the
/// thing and is drawn on the action's gradient. Returns which one was pressed — by the pointer, or by
/// Enter, which presses that last button when it is enabled.
pub fn footer(ui: &mut egui::Ui, area: Rect, buttons: &[(&str, bool)]) -> Option<usize> {
    footer_confirmed_by(ui, area, buttons, Confirm::Enter)
}

/// The same footer, for a modal whose body owns Enter. See [`Confirm`].
pub fn footer_confirmed_by(
    ui: &mut egui::Ui,
    area: Rect,
    buttons: &[(&str, bool)],
    confirm: Confirm,
) -> Option<usize> {
    let bar = Rect::from_min_max(Pos2::new(area.left(), area.bottom() - FOOTER), area.max);
    let mut pressed = decorate(ui, "footer", bar, |rux| {
        let mut pressed = None;
        let mut right = bar.right() - 22.0;
        let middle = bar.center().y - 4.0;
        for (index, (name, enabled)) in buttons.iter().enumerate().rev() {
            let primary = index + 1 == buttons.len();
            let pill = pill(name, *enabled, primary);
            let width = pill.measure(rux).x.max(84.0);
            let rect = Rect::from_min_size(
                Pos2::new(right - width, middle - BUTTON / 2.0),
                Vec2::new(width, BUTTON),
            );
            if pill.show(rux, rect).clicked() && *enabled {
                pressed = Some(index);
            }
            right = rect.left() - 10.0;
        }
        pressed
    });
    // The keyboard presses the last button, which is the one that does the thing. A footer whose
    // last button is dimmed — no name typed, nothing chosen — is a modal there is nothing to
    // confirm, so the key press is left alone rather than doing nothing loudly.
    let primary = buttons.len().checked_sub(1);
    if let Some(primary) = primary {
        if buttons[primary].1 && confirm.pressed(ui) {
            pressed = Some(primary);
        }
    }
    pressed
}

/// How tall a button is: the Settings window's 34 points.
pub const BUTTON: f32 = 34.0;

/// A button of the design: the action's gradient for the one that does the thing, a raised card pill
/// for the others.
fn pill(name: &str, enabled: bool, primary: bool) -> PillButton<'_> {
    let pill = PillButton::new(name).enabled(enabled);
    match primary {
        true => pill.primary(),
        false => pill,
    }
}

/// A button with a word in it, anywhere in a dialog. The one that does the thing is on the action's
/// gradient, so the answers to a question do not all look the same.
pub fn button(ui: &mut egui::Ui, area: Rect, name: &str, enabled: bool, primary: bool) -> bool {
    decorate(ui, ("button", name), area, |rux| {
        pill(name, enabled, primary).show(rux, area).clicked() && enabled
    })
}

/// A heading inside a page: its name in small spaced capitals in the quiet ink, and a groove running to
/// the right edge, which is how the ticket modal names its sections.
pub fn section(ui: &mut egui::Ui, area: Rect, top: f32, name: &str) -> f32 {
    let y = top + 8.0;
    let line =
        Rect::from_min_max(Pos2::new(area.left(), y - 8.0), Pos2::new(area.right(), y + 8.0));
    let clip = down_the_page(ui, area);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(line));
    child.set_clip_rect(clip);
    decorate(&mut child, ("section", name), line, |rux| {
        let ink = rux.theme().ink;
        let style = section_style();
        let galley = rux.text(style, &name.to_uppercase(), ink.i500);
        let width = galley.size().x;
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(area.left(), y),
            galley,
            style,
            ink.i500,
        );
        let chat = rux.theme().chat;
        let painter = rux.painter().clone();
        let from = area.left() + width + 12.0;
        painter.line_segment(
            [Pos2::new(from, y), Pos2::new(area.right(), y)],
            Stroke::new(1.0, chat.dark.gamma_multiply(0.6)),
        );
        painter.line_segment(
            [Pos2::new(from, y + 1.0), Pos2::new(area.right(), y + 1.0)],
            Stroke::new(1.0, chat.light.gamma_multiply(0.5)),
        );
    });
    y + 18.0
}

/// A line of explanation, in the quiet ink. Returns the y below it.
pub fn note(ui: &mut egui::Ui, area: Rect, top: f32, text: &str) -> f32 {
    let painter = ui.painter_at(down_the_page(ui, area));
    let ink = crate::theme::rux_theme().ink.i400;
    let galley =
        rux::text::wrapped(&painter, Style::sans(11.5).leading(1.55), text, ink, area.width());
    let height = galley.size().y;
    painter.galley(Pos2::new(area.left(), top), galley, ink);
    top + height + 8.0
}

/// `area` widened a little sideways and left alone downwards, which is the clip a page's own painting wants.
///
/// **A settings page that scrolls paints a row well below the rectangle it was given**, and `area.expand(20)`
/// clipped everything past the first twenty points of that away. It is not obvious because of where the
/// rectangle comes from: inside an `egui::ScrollArea` a page reads `Ui::available_rect_before_wrap`, which is
/// the viewport moved **up** by however far the page is scrolled — so scrolling down by 300 points threw away
/// the bottom 300 points of the window. What survived was the fields and the buttons, because those paint
/// through `ui.painter()` and are clipped by the scrolling area itself; what vanished was every heading,
/// label and note. `task-2003` reports it on the Agent-Chat page as controls with nothing beside them.
///
/// So the sideways clip stays — a heading must not draw past the page's own margins — and downwards the
/// `Ui`'s own clip rectangle is used, which is the scrolling area's and is the right answer in a page that
/// does not scroll too.
fn down_the_page(ui: &egui::Ui, area: Rect) -> Rect {
    Rect::from_x_y_ranges(area.x_range().expand(20.0), ui.clip_rect().y_range())
}

/// A tick box with its label to the right of it. Returns true when it was changed.
pub fn check(ui: &mut egui::Ui, row: Rect, name: &str, value: &mut bool) -> bool {
    check_named(ui, row, name, name, value)
}

/// A tick box whose drawn word and whose name are not the same thing.
///
/// A row that repeats — a column of the New Table dialog — draws `PK` on every one of them, and two
/// controls in Unluminous must not share a name. So the word is what a person reads and the name is what a
/// test and an agent ask for, and it is also what gives each row's box an id of its own.
pub fn check_named(
    ui: &mut egui::Ui,
    row: Rect,
    drawn: &str,
    name: &str,
    value: &mut bool,
) -> bool {
    let box_rect =
        Rect::from_min_size(Pos2::new(row.left(), row.center().y - 9.0), Vec2::splat(18.0));
    let response = ui.interact(row, ui.id().with(("modal-check", name)), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let on = *value;
    decorate(ui, ("check", name), box_rect, |rux| tick_box(rux, box_rect, on));
    let ink = crate::theme::rux_theme().ink;
    let tint = match response.hovered() {
        true => ink.i900,
        false => ink.i700,
    };
    let galley = ui.painter().layout_no_wrap(drawn.to_owned(), Style::sans(12.5).font(), tint);
    ui.painter().galley(
        Pos2::new(box_rect.right() + 10.0, row.center().y - galley.size().y / 2.0),
        galley,
        tint,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), on, name)
    });
    if response.clicked() {
        *value = !*value;
        return true;
    }
    false
}

/// A square tick box in the design: a raised chip while it is not ticked, and the action's gradient with
/// a white tick once it is.
pub fn tick_box(rux: &mut Rux<'_>, rect: Rect, on: bool) {
    let chat = rux.theme().chat;
    match on {
        true => {
            rux.chrome.surface(
                rect,
                6.0,
                rux::Fill::gradient(chat.action, rect),
                rux::Elevation::of([rux::Shadow::outer(0.0, 1.0, 3.0, chat.dark)]),
            );
            rux.mark(
                Mark::new(Icon::Tick, 11.0).stroke(1.9),
                rect.center(),
                Color32::WHITE, // any ground: the tick is on the action's gradient, which is the same on both
            );
        }
        false => rux.chrome.surface(rect, 6.0, chat.well, chat.carved_sm()),
    }
}

/// A field to type into: a carved well with round ends, as every field in the Settings window is.
pub fn field(ui: &mut egui::Ui, area: Rect, name: &str, value: &mut String) -> egui::Response {
    let id = ui.id().with(("modal-field", name));
    let font = Style::sans(12.5).font();
    let ink = crate::theme::rux_theme().ink;
    decorate(ui, ("field", name), area, |rux| {
        let chat = rux.theme().chat;
        rux.chrome.surface(area, area.height() / 2.0, chat.well, chat.carved_sm());
    });
    let inset = (area.height() * 0.45).min(14.0);
    let text = crate::components::controls::field_takes_the_whole_rectangle_at(
        ui,
        area,
        inset,
        id,
        "Modal text field",
        &font,
    );
    let text = Rect::from_min_max(text.min, Pos2::new(area.right() - inset, text.max.y));
    let mut edit = ui.new_child(egui::UiBuilder::new().max_rect(text));
    let response = edit.add(
        egui::TextEdit::singleline(value)
            .id(id)
            .frame(egui::Frame::NONE)
            .font(font)
            .desired_width(text.width())
            .text_color(ink.i900),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, name));
    response
}

/// A carved well a list or a block of text sits in, sixteen points round.
pub fn well(ui: &mut egui::Ui, area: Rect) {
    decorate(ui, ("well", area.min.x.to_bits(), area.min.y.to_bits()), area, |rux| {
        let chat = rux.theme().chat;
        rux.chrome.surface(area, 16.0, chat.well, chat.carved_sm());
    });
}

/// A row of tabs: a carved track with the chosen one raised out of it, the Settings window's segmented
/// control. Answers the index pressed, when it is not the one already chosen.
pub fn tabs(
    ui: &mut egui::Ui,
    at: Pos2,
    name: &str,
    words: &[&str],
    chosen: usize,
) -> (Option<usize>, Rect) {
    decorate(ui, ("tabs", name), Rect::from_min_size(at, Vec2::new(400.0, 38.0)), |rux| {
        let width = crate::components::settings_kit::segmented_width(rux, words, 86.0, false, 14.0);
        let track = Rect::from_min_size(
            at,
            Vec2::new(width, crate::components::settings_kit::SEGMENT_TRACK),
        );
        let picked = crate::components::settings_kit::segmented(
            rux,
            track,
            name,
            words,
            Some(chosen),
            86.0,
            false,
        );
        (picked, track)
    })
}

/// A chip raised out of whatever it is on, which is how a chosen row is drawn. For a row a dialog lays
/// out itself rather than through [`row`].
pub fn raised(ui: &mut egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug, rect: Rect) {
    decorate(ui, ("raised", salt), rect, |rux| {
        let chat = rux.theme().chat;
        rux.chrome.surface(
            rect,
            (rect.height() / 2.0).min(16.0),
            rux::Fill::gradient(chat.chip, rect),
            chat.raised_chip(),
        );
    });
}

/// The wash under the pointer on a row a dialog lays out itself.
pub fn washed(ui: &mut egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug, rect: Rect) {
    decorate(ui, ("washed", salt), rect, |rux| {
        let wash = rux.theme().ink.i900.gamma_multiply(0.05);
        rux.wash(rect, (rect.height() / 2.0).min(16.0), wash);
    });
}

/// One row of a list: raised out of the list when it is chosen, washed under the pointer, and whatever
/// `draw` puts in it.
///
/// A list can hold a thousand rows, so the decoration is drawn only for the row that is chosen and the row
/// under the pointer: a `rux` layer is a canvas, and a canvas a row would be a thousand of them.
pub fn row(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    chosen: bool,
    draw: impl FnOnce(&egui::Painter, Rect),
) -> egui::Response {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, size::ROW + 2.0), Sense::hover());
    let response = ui.interact(rect, ui.id().with(("modal-row", &id)), Sense::click());
    let pill = rect.shrink2(Vec2::new(4.0, 1.0));
    if chosen || response.hovered() {
        let hovered = response.hovered();
        decorate(ui, ("row", id), pill, |rux| {
            let chat = rux.theme().chat;
            match chosen {
                true => rux.chrome.surface(
                    pill,
                    pill.height() / 2.0,
                    rux::Fill::gradient(chat.chip, pill),
                    chat.raised_chip(),
                ),
                false if hovered => {
                    let wash = rux.theme().ink.i900.gamma_multiply(0.05);
                    rux.wash(pill, pill.height() / 2.0, wash);
                }
                false => {}
            }
        });
    }
    draw(ui.painter(), rect);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), chosen, name)
    });
    response
}

/// Text at a position, at the ordinary size, vertically centred in `row`.
pub fn label(
    painter: &egui::Painter,
    row: Rect,
    x: f32,
    text: &str,
    tint: Color32,
    size: f32,
) -> f32 {
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(size), tint);
    painter.galley(Pos2::new(x, row.center().y - galley.size().y / 2.0), galley.clone(), tint);
    x + galley.size().x
}

/// A monospaced block of text that scrolls, in a carved well, which is what a diff and a commit are shown in.
///
/// **What it is showing is not reachable from outside the window**, and that is worth knowing before
/// trying: the lines are plain labels inside a scrolling area, and a `widget_info` carrying the whole
/// text does not reach the tree egui hands out either. So a dialog whose point is the text in one of
/// these — the DDL, the pending statements, the New Table dialog — reports that text through its own
/// plugin `view` as well, which is where an agent reads it and what a test asserts on. `task-1795`.
pub fn monospaced(ui: &mut egui::Ui, area: Rect, id: &str, text: &str) {
    well(ui, area);
    let inner = area.shrink2(Vec2::new(14.0, 10.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(inner);
    egui::ScrollArea::both().id_salt(id).show(&mut child, |ui| {
        for line in text.lines() {
            // A diff reads by colour before it reads by sign, which is the whole point of one.
            let tint = match line.chars().next() {
                Some('+') if !line.starts_with("+++") => color::git_added(),
                Some('-') if !line.starts_with("---") => color::close(),
                Some('@') => color::accent(),
                _ => color::text_control(),
            };
            ui.label(egui::RichText::new(line).monospace().size(11.5).color(tint));
        }
        if text.trim().is_empty() {
            ui.label(egui::RichText::new("Nothing to show.").size(11.5).color(color::text_faint()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Vec2 = Vec2::new(1180.0, 740.0);

    #[test]
    fn a_modal_that_has_not_been_touched_gets_the_size_its_dialog_asked_for() {
        let (size, offset) = fit(WINDOW, Vec2::new(560.0, 420.0), Vec2::ZERO, Vec2::ZERO);
        assert_eq!(size, Vec2::new(560.0, 420.0));
        assert_eq!(offset, Vec2::ZERO, "and sits in the middle");
    }

    #[test]
    fn a_modal_is_never_larger_than_the_window_it_is_in() {
        let (size, _) =
            fit(Vec2::new(600.0, 400.0), Vec2::new(900.0, 560.0), Vec2::ZERO, Vec2::ZERO);
        assert_eq!(size, Vec2::new(560.0, 360.0), "twenty points of margin either side");
    }

    #[test]
    fn a_modal_cannot_be_resized_below_what_can_be_read() {
        let (size, _) = fit(WINDOW, Vec2::new(560.0, 420.0), Vec2::new(-900.0, -900.0), Vec2::ZERO);
        assert_eq!(size, Vec2::new(MIN_WIDTH, MIN_HEIGHT));
    }

    #[test]
    fn a_modal_cannot_be_dragged_out_of_the_window() {
        let asked = Vec2::new(560.0, 420.0);
        let (_, offset) = fit(WINDOW, asked, Vec2::ZERO, Vec2::new(4000.0, -4000.0));
        // Half the room either side, less the eight points that keep it off the window's own edge.
        assert_eq!(offset.x, (WINDOW.x - asked.x) / 2.0 - 8.0);
        assert_eq!(offset.y, -((WINDOW.y - asked.y) / 2.0 - 8.0));
    }

    #[test]
    fn dragging_an_edge_keeps_the_opposite_edge_where_it_was() {
        // The right edge dragged 100 points to the right: 100 wider, and the middle 50 to the right,
        // which leaves the left edge exactly where it was.
        let mut placement = Placement::default();
        let drag = Vec2::new(100.0, 0.0);
        let side = (1, 0);
        let grown = Vec2::new(side.0 as f32 * drag.x, side.1 as f32 * drag.y);
        placement.grown += grown;
        placement.offset += Vec2::new(side.0 as f32 * grown.x, side.1 as f32 * grown.y) / 2.0;
        assert_eq!(placement.grown, Vec2::new(100.0, 0.0));
        assert_eq!(placement.offset, Vec2::new(50.0, 0.0));
    }

    #[test]
    fn a_modals_controls_are_named_after_it() {
        assert_eq!(plain_name("unluminous-find-in-files"), "find in files");
        assert_eq!(plain_name("unluminous-settings"), "settings");
    }
}
