//! Drawing the Skills Manager: the grid of skill cards, one skill's page, the Settings page and the New Skill
//! dialog. `task-2396`.
//!
//! ## The drawing changes nothing
//!
//! Every function here takes a rectangle, draws, and reports an [`Act`]. [`apply`] is the one place an act
//! becomes a change, and it calls the same provider functions `unluminous-cli plugins run skills ...` calls, so a
//! press and a command are the same thing. The surfaces are `rux`'s, through the Designer's kit
//! (`components::designer::kit`), whose state is kept per window and given back once a frame by the window.

pub mod card;
pub mod detail;
pub mod dialogs;
pub mod pane;

use std::cell::RefCell;

use crate::services::plugin_ui::{Look, Request};
use crate::services::skills::harness::{self, Scope};
use crate::services::skills::{Filter, Modal, Skills};

pub use dialogs::{modal, settings_page};

/// What the drawing reported.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// Show a skill's page.
    Open(String),
    /// Go back to the grid.
    Back,
    /// Install for the chosen agents in the chosen scope, or for one agent in one scope.
    Install(String, Option<&'static str>, Scope),
    Remove(String, Option<&'static str>, Scope),
    /// Remove the skill from every agent's folders, for this project and globally.
    RemoveEverywhere(String),
    Edit(String),
    Reset(String),
    Update(String),
    AskToDelete(String),
    Delete(String),
    Reveal(std::path::PathBuf),
    NewSkill,
    CreateSkill(String, String),
    CloseModal,
    Check,
    Filter(Filter),
    Scope(Scope),
}

/// Draw the pane and apply what was pressed.
pub fn show(skills: &mut Skills, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    take_the_pictures_that_arrived(skills, ui.ctx());
    let acts = match skills.viewing.clone() {
        Some(id) if skills.shelf.get(&id).is_some() => detail::show(skills, &id, ui, look),
        _ => pane::show(skills, ui, look),
    };
    apply(skills, acts)
}

/// The one place an act becomes a change.
pub fn apply(skills: &mut Skills, acts: Vec<Act>) -> Vec<Request> {
    let mut requests = Vec::new();
    for act in acts {
        if let Act::Reveal(path) = act {
            requests.push(Request::Reveal(path));
            continue;
        }
        match apply_one(skills, act) {
            Ok(Some(said)) => skills.say(said),
            Ok(None) => {}
            Err(why) => skills.say(why),
        }
    }
    requests
}

/// One act, answering what to say about it.
fn apply_one(skills: &mut Skills, act: Act) -> Result<Option<String>, String> {
    match act {
        Act::Open(id) => {
            skills.viewing = Some(id);
            Ok(None)
        }
        Act::Back => {
            skills.viewing = None;
            Ok(None)
        }
        Act::Install(id, one, scope) => {
            let harnesses = targets(skills, one)?;
            skills.install(&id, &harnesses, scope, false).map(Some)
        }
        Act::Remove(id, one, scope) => {
            let harnesses = targets(skills, one)?;
            skills.remove(&id, &harnesses, scope, false).map(Some)
        }
        Act::RemoveEverywhere(id) => {
            let every: Vec<&'static harness::Harness> = harness::HARNESSES.iter().collect();
            let mut lines = Vec::new();
            for scope in [Scope::Project, Scope::Global] {
                if scope == Scope::Project && skills.project.is_none() {
                    continue;
                }
                lines.push(skills.remove(&id, &every, scope, false)?);
            }
            Ok(Some(lines.join(" ")))
        }
        Act::Edit(id) => skills.edit(&id).map(Some),
        Act::Reset(id) => run(skills, "reset", &[id]),
        Act::Update(id) => skills.update(&id, false).map(Some),
        Act::AskToDelete(id) => {
            skills.modal = Some(Modal::Delete { id });
            Ok(None)
        }
        Act::Delete(id) => {
            skills.modal = None;
            run(skills, "delete", &[id])
        }
        Act::NewSkill => {
            skills.modal =
                Some(Modal::NewSkill { name: String::new(), description: String::new() });
            Ok(None)
        }
        Act::CreateSkill(name, description) => {
            let (id, _) = skills.new_skill(&name, &description)?;
            skills.modal = None;
            Ok(Some(format!("Made {id}; its SKILL.md is open")))
        }
        Act::CloseModal => {
            skills.modal = None;
            Ok(None)
        }
        Act::Check => skills.check().map(Some),
        Act::Filter(filter) => {
            skills.filter = filter;
            Ok(None)
        }
        Act::Scope(scope) => {
            skills.scope = scope;
            Ok(None)
        }
        Act::Reveal(_) => Ok(None),
    }
}

/// Draw with `rux` into `rect` in a layer with room for the whole of a raised card's shadow.
///
/// The Designer's kit reaches [`kit::REACH`] points past a rectangle, and the lit half of a card's shadow reaches
/// further than that above it, so its top was cut off in a straight line over every card.
pub fn roomy_layer<R>(
    ui: &mut egui::Ui,
    zoom: f32,
    salt: impl std::hash::Hash + std::fmt::Debug,
    rect: egui::Rect,
    add: impl FnOnce(&mut rux::Rux<'_>) -> R,
) -> R {
    let state = crate::components::designer::kit::state(ui.ctx(), zoom);
    crate::theme::in_step(&state);
    let id = ui.id().with(("skills-layer", salt));
    rux::layer(ui, &state, id, rect.expand(ROOM * zoom), add)
}

/// How far past a card its layer reaches.
const ROOM: f32 = 64.0;

/// The harnesses an act is for: the one named, or the ones Settings chooses.
fn targets(
    skills: &Skills,
    one: Option<&'static str>,
) -> Result<Vec<&'static harness::Harness>, String> {
    let named: Vec<String> = one.map(|id| vec![id.to_owned()]).unwrap_or_default();
    skills.harnesses_for(&named)
}

/// Run a verb the way `plugins run skills` does.
fn run(skills: &mut Skills, verb: &str, words: &[String]) -> Result<Option<String>, String> {
    crate::services::skills::commands::run(skills, verb, words).map(|answer| Some(answer.message))
}

thread_local! {
    /// The pictures on the graphics card for this window, by the window and the picture's key: `None` when a
    /// skill's picture could not be found, which is drawn as the stand in.
    static PICTURES: RefCell<std::collections::HashMap<(u64, String), Option<egui::TextureHandle>>> =
        RefCell::new(std::collections::HashMap::new());
}

/// How many pictures go to the graphics card in one frame. Each is 480 by 328 pixels, about 0.6 MB, so two keep a
/// frame's uploads near the 2 MB that `services::backgrounds` measured a DX12 driver keeping for good after one
/// large upload. The rest follow on the next frames.
const PICTURES_A_FRAME: usize = 2;

/// A skill's picture on the graphics card, or `None` while it is still being read or when it has none, which the
/// card draws as the stand in.
///
/// Nothing is read or decoded here: the first time a card is drawn its picture is asked of the loader's thread
/// ([`crate::services::skills::pictures`]), and [`take_the_pictures_that_arrived`] puts it on the card when it
/// comes back - `task-2401`.
pub fn picture(skills: &Skills, ctx: &egui::Context, id: &str) -> Option<egui::TextureHandle> {
    let wanted = skills.shelf.get(id).and_then(|skill| skills.picture_wanted(skill))?;
    let key = (crate::components::chrome_kit::owner(ctx), wanted.key.clone());
    if let Some(known) = PICTURES.with(|pictures| pictures.borrow().get(&key).cloned()) {
        return known;
    }
    skills.pictures.ask(wanted);
    None
}

/// Put the pictures the loader's thread has decoded on the graphics card, a few a frame, and ask for another frame
/// while more are on their way.
pub fn take_the_pictures_that_arrived(skills: &Skills, ctx: &egui::Context) {
    let owner = crate::components::chrome_kit::owner(ctx);
    for arrived in skills.pictures.take(PICTURES_A_FRAME) {
        let texture = arrived.image.map(|image| {
            crate::services::picture::upload(
                ctx,
                format!("skill-{}", arrived.key),
                image,
                egui::TextureOptions::LINEAR,
            )
        });
        PICTURES.with(|pictures| pictures.borrow_mut().insert((owner, arrived.key), texture));
    }
    if skills.pictures.waiting() > 0 {
        ctx.request_repaint();
    }
}

/// Paint `texture` over `rect`, cropped to fill it the way CSS's `object-fit: cover` does, with the corners rounded.
pub fn paint_cover(
    painter: &egui::Painter,
    rect: egui::Rect,
    texture: &egui::TextureHandle,
    corners: egui::CornerRadius,
) {
    let size = texture.size_vec2();
    let (picture, place) = (size.x / size.y.max(1.0), rect.width() / rect.height().max(1.0));
    let uv = match picture > place {
        true => {
            let width = place / picture;
            egui::Rect::from_min_max(
                egui::pos2((1.0 - width) / 2.0, 0.0),
                egui::pos2((1.0 + width) / 2.0, 1.0),
            )
        }
        false => {
            let height = picture / place;
            egui::Rect::from_min_max(
                egui::pos2(0.0, (1.0 - height) / 2.0),
                egui::pos2(1.0, (1.0 + height) / 2.0),
            )
        }
    };
    painter.add(
        egui::epaint::RectShape::filled(rect, corners, egui::Color32::WHITE) // any ground: a picture's own colours
            .with_texture(texture.id(), uv),
    );
}

/// A radial glow round `centre` that fills `rect` with its corners rounded, as one mesh: rays from the centre to
/// points along the rounded outline, each cut into steps coloured by their distance from the centre, so the glow
/// has no visible rings and nothing of it is drawn past a rounded corner.
///
/// It used to be rings round the centre clipped to the rectangle, and a clip is square: past each rounded top
/// corner of a card a square of glow showed, which read as the card's shadow cut off (`task-2405`).
fn glow_mesh(
    rect: egui::Rect,
    corners: egui::CornerRadius,
    centre: egui::Pos2,
    reach: f32,
    glow: egui::Color32,
) -> egui::Mesh {
    // How strong the glow is at a distance from the centre, as a share of `reach`.
    let strength = |distance: f32| {
        let at = (distance / reach.max(1.0)).clamp(0.0, 1.0);
        match at < 0.4 {
            true => 0.42 + (0.16 - 0.42) * at / 0.4,
            false => 0.16 * (1.0 - (at - 0.4) / 0.6),
        }
    };
    let outline = rounded_outline(rect, corners);
    const STEPS: u32 = 16;
    let mut mesh = egui::Mesh::default();
    for edge in &outline {
        for step in 0..=STEPS {
            let at = centre + (*edge - centre) * (step as f32 / STEPS as f32);
            mesh.colored_vertex(at, glow.gamma_multiply(strength(at.distance(centre))));
        }
    }
    let rays = outline.len() as u32;
    for ray in 0..rays {
        let next = (ray + 1) % rays;
        for step in 0..STEPS {
            let (here, there) = (ray * (STEPS + 1) + step, next * (STEPS + 1) + step);
            mesh.add_triangle(here, here + 1, there + 1);
            mesh.add_triangle(here, there + 1, there);
        }
    }
    mesh
}

/// Points round `rect` with its corners rounded by `corners`, clockwise from the top left, close enough together
/// that the rays of [`glow_mesh`] between them are not seen.
fn rounded_outline(rect: egui::Rect, corners: egui::CornerRadius) -> Vec<egui::Pos2> {
    use std::f32::consts::{FRAC_PI_2, PI};
    let half = rect.width().min(rect.height()) / 2.0;
    let radius = |value: u8| f32::from(value).min(half);
    // Each corner: its centre, its radius, and the angle its arc starts at, going clockwise on the screen.
    let arcs = [
        (rect.left_top(), radius(corners.nw), egui::vec2(1.0, 1.0), PI),
        (rect.right_top(), radius(corners.ne), egui::vec2(-1.0, 1.0), -FRAC_PI_2),
        (rect.right_bottom(), radius(corners.se), egui::vec2(-1.0, -1.0), 0.0),
        (rect.left_bottom(), radius(corners.sw), egui::vec2(1.0, -1.0), FRAC_PI_2),
    ];
    let mut points = Vec::new();
    for (corner, radius, inward, start) in arcs {
        let middle = corner + inward * radius;
        let steps = match radius > 0.5 {
            true => 8,
            false => 0,
        };
        for step in 0..=steps {
            let angle = start + FRAC_PI_2 * step as f32 / steps.max(1) as f32;
            points.push(middle + egui::vec2(angle.cos(), angle.sin()) * radius);
        }
    }
    // The straight edges are long; split each into pieces a few points apart.
    let mut fine = Vec::new();
    for (index, point) in points.iter().enumerate() {
        let next = points[(index + 1) % points.len()];
        let pieces = (point.distance(next) / 12.0).ceil().max(1.0) as usize;
        for piece in 0..pieces {
            fine.push(*point + (next - *point) * (piece as f32 / pieces as f32));
        }
    }
    fine
}

/// A stand in for a skill with no picture: a dark ground with a soft glow in a colour worked out from its name, and
/// the blocks mark over it.
pub fn paint_placeholder(
    painter: &egui::Painter,
    rect: egui::Rect,
    id: &str,
    corners: egui::CornerRadius,
) {
    let seed =
        id.bytes().fold(7u32, |sum, byte| sum.wrapping_mul(31).wrapping_add(u32::from(byte)));
    let hue = (seed % 360) as f32 / 360.0;
    let glow: egui::Color32 = egui::ecolor::Hsva::new(hue, 0.65, 0.85, 1.0).into();
    painter.rect_filled(rect, corners, egui::Color32::BLACK); // any ground: it stands in for a dark picture
    let clipped = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let centre = egui::pos2(rect.left() + rect.width() * 0.62, rect.top() + rect.height() * 0.45);
    let reach = rect.width().max(rect.height()) * 0.55;
    clipped.add(glow_mesh(rect, corners, centre, reach, glow));
    let side = rect.height().min(rect.width()) * 0.34;
    crate::theme::svg_icon::paint(
        &clipped,
        centre,
        side,
        &crate::theme::svg_icon::BLOCKS,
        egui::Color32::from_white_alpha(170), // any ground: the mark on the dark stand in
    );
}
