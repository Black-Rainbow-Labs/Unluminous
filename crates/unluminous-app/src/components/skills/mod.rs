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
    /// The pictures decoded for this window, by skill: `None` when a skill has none.
    static PICTURES: RefCell<std::collections::HashMap<(u64, String), Option<egui::TextureHandle>>> =
        RefCell::new(std::collections::HashMap::new());
}

/// A skill's picture on the graphics card, decoded the first time it is drawn.
pub fn picture(skills: &Skills, ctx: &egui::Context, id: &str) -> Option<egui::TextureHandle> {
    let owner = crate::components::chrome_kit::owner(ctx);
    let key = (owner, id.to_owned());
    if let Some(known) = PICTURES.with(|pictures| pictures.borrow().get(&key).cloned()) {
        return known;
    }
    let made =
        skills.shelf.get(id).and_then(|skill| skills.picture_bytes(skill)).and_then(|bytes| {
            let image = crate::services::picture::decode_bytes(&bytes).ok()?;
            Some(crate::services::picture::upload(
                ctx,
                format!("skill-{id}"),
                image,
                egui::TextureOptions::LINEAR,
            ))
        });
    PICTURES.with(|pictures| pictures.borrow_mut().insert(key, made.clone()));
    made
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
    // A radial glow as one mesh whose colours are blended between three rings, so it has no visible steps.
    let rings = [(0.0, 0.42), (0.4, 0.16), (1.0, 0.0)];
    let around = 72;
    let mut mesh = egui::Mesh::default();
    for (radius, strength) in rings {
        for index in 0..around {
            let angle = index as f32 / around as f32 * std::f32::consts::TAU;
            let at = centre + egui::vec2(angle.cos(), angle.sin()) * reach * radius;
            mesh.colored_vertex(at, glow.gamma_multiply(strength));
        }
    }
    for ring in 0..rings.len() as u32 - 1 {
        for index in 0..around as u32 {
            let next = (index + 1) % around as u32;
            let (inner, outer) = (ring * around as u32, (ring + 1) * around as u32);
            mesh.add_triangle(inner + index, outer + index, outer + next);
            mesh.add_triangle(inner + index, outer + next, inner + next);
        }
    }
    clipped.add(mesh);
    let side = rect.height().min(rect.width()) * 0.34;
    crate::theme::svg_icon::paint(
        &clipped,
        centre,
        side,
        &crate::theme::svg_icon::BLOCKS,
        egui::Color32::from_white_alpha(170), // any ground: the mark on the dark stand in
    );
}
