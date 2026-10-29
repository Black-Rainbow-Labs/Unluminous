//! The Skills Manager's Settings page and its two dialogs: naming a new skill, and making sure before one is
//! deleted.
//!
//! The page is cards from `components::settings_kit`, written to the plugin's `settings.conf` the moment
//! something changes, which is the file `settings set plugins.skills.<key>` writes too. The dialogs are
//! `components::modal::show_unframed` drawn with `components::realm::dialog`'s parts, as the Designer's are.

use egui::{Pos2, Rect, Vec2};

use super::Act;
use crate::components::modal;
use crate::components::realm::dialog;
use crate::components::settings_kit::{self as kit, Column};
use crate::services::plugin_ui::{Look, Request};
use crate::services::skills::harness::{self, Scope, HARNESSES};
use crate::services::skills::{Modal, Skills};

const NEW_ID: &str = "unluminous-skills-new";
const DELETE_ID: &str = "unluminous-skills-delete";
const WIDTH: f32 = 480.0;

/// The Skills page in the Settings window.
pub fn settings_page(skills: &mut Skills, ui: &mut egui::Ui, _look: &Look<'_>) -> Vec<Request> {
    kit::plugin_page(ui, "skills-settings", |ui, column| cards(skills, ui, column))
}

/// The page's three cards, then writing what changed.
fn cards(skills: &mut Skills, ui: &mut egui::Ui, column: &mut Column) -> Vec<Request> {
    let changed = the_installs_card(skills, ui, column)
        | the_agents_card(skills, ui, column)
        | the_site_card(skills, ui, column);
    let mut requests = Vec::new();
    if changed {
        match skills.folder.clone() {
            Some(folder) => {
                if let Err(why) = skills.config.write(&folder) {
                    requests.push(Request::Message(why));
                }
            }
            None => requests.push(Request::Message(
                "This window keeps no settings, so the change lasts until it closes.".into(),
            )),
        }
        skills.refresh();
    }
    requests
}

/// `Where installs go`: this project or global.
fn the_installs_card(skills: &mut Skills, ui: &mut egui::Ui, column: &mut Column) -> bool {
    let mut changed = false;
    let help = Some("A skill installed for this project is written into the project's own folder for each agent, such as .claude/skills, and is read only there. One installed globally goes in your home folder, for every project.");
    kit::card(ui, column, "Where installs go", help, |card| {
        let choices = [Scope::Project.label(), Scope::Global.label()];
        let chosen = Some(match skills.config.scope {
            Scope::Project => 0,
            Scope::Global => 1,
        });
        card.row("Install for", Some("What the pane's switch starts on."), |row| {
            if let Some(picked) = row.segmented("Install for", &choices, chosen, 110.0, false) {
                skills.config.scope = if picked == 0 { Scope::Project } else { Scope::Global };
                skills.scope = skills.config.scope;
                changed = true;
            }
        });
    });
    changed
}

/// `Agents`: which agents an install is for when none is named.
fn the_agents_card(skills: &mut Skills, ui: &mut egui::Ui, column: &mut Column) -> bool {
    let mut changed = false;
    let help = Some("With none chosen, an install is for every agent found on this machine. A skill page can still install for any one agent.");
    kit::card(ui, column, "Agents", help, |card| {
        for agent in HARNESSES {
            let mut on = skills.config.harnesses.iter().any(|id| id == agent.id);
            let found = skills.places.is_here(agent);
            let note = format!(
                "{}{}",
                harness::shown(&skills.places, &skills.places.resolve(&agent.global), None),
                if found { "" } else { "  \u{00B7}  not on this machine" }
            );
            if card.switch(agent.name, Some(note.as_str()), &mut on) {
                skills.config.harnesses.retain(|id| id != agent.id);
                if on {
                    skills.config.harnesses.push(agent.id.to_owned());
                }
                changed = true;
            }
        }
    });
    changed
}

/// `Catalogue and folders`: where the site's list is read from, and more skills folders to look in.
fn the_site_card(skills: &mut Skills, ui: &mut egui::Ui, column: &mut Column) -> bool {
    let mut changed = false;
    let help = Some("The list is read when the pane is shown, at most every six hours, and a skill's files only when it is installed or updated.");
    kit::card(ui, column, "Catalogue and folders", help, |card| {
        let mut address = skills.config.catalogue.clone();
        card.row("Catalogue", Some("Where the skills list is read from."), |row| {
            if row.text("Catalogue", &mut address, crate::services::skills::catalogue::SITE, 300.0)
            {
                skills.config.catalogue = address.clone();
                changed = true;
            }
        });
        let mut folders = skills
            .config
            .folders
            .iter()
            .map(|folder| folder.display().to_string())
            .collect::<Vec<String>>()
            .join(" | ");
        card.row(
            "More folders",
            Some("Skills folders of agents not in the list, separated by |."),
            |row| {
                if row.text("More skills folders", &mut folders, "D:/tools/skills", 300.0) {
                    skills.config.folders = folders
                        .split('|')
                        .map(str::trim)
                        .filter(|folder| !folder.is_empty())
                        .map(std::path::PathBuf::from)
                        .collect();
                    changed = true;
                }
            },
        );
    });
    changed
}

/// Draw whichever dialog is open. Answers what it asked for and whether it closed.
pub fn modal(skills: &mut Skills, ctx: &egui::Context, _look: &Look<'_>) -> (Vec<Request>, bool) {
    let Some(open) = skills.modal.clone() else { return (Vec::new(), false) };
    let acts = match open {
        Modal::NewSkill { name, description } => new_skill(skills, ctx, name, description),
        Modal::Delete { id } => delete(ctx, &id),
    };
    let closed = acts
        .iter()
        .any(|act| matches!(act, Act::CloseModal | Act::CreateSkill(..) | Act::Delete(_)));
    (super::apply(skills, acts), closed)
}

/// The room inside the dialog's padding.
fn inner_of(area: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(area.left() + dialog::PAD_X, area.top() + dialog::PAD_Y),
        Pos2::new(area.right() - dialog::PAD_X, area.bottom() - dialog::PAD_Y),
    )
}

/// Naming a new skill and saying when it applies.
fn new_skill(
    skills: &mut Skills,
    ctx: &egui::Context,
    mut name: String,
    mut description: String,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let height = dialog::PAD_Y * 2.0
        + dialog::CLOSE
        + (dialog::GAP + dialog::SEARCH) * 2.0
        + dialog::GAP
        + dialog::BUTTON;
    let (_, escaped) = modal::show_unframed(ctx, NEW_ID, WIDTH, height, |ui, area| {
        dialog::surface(ui, "skills-new", area);
        let inner = inner_of(area);
        crate::components::settings_kit::layer(ui, "skills-new", inner, |rux| {
            if dialog::heading(rux, inner, "New Skill") {
                acts.push(Act::CloseModal);
            }
            let first = Rect::from_min_size(
                Pos2::new(inner.left(), inner.top() + dialog::CLOSE + dialog::GAP),
                Vec2::new(inner.width(), dialog::SEARCH),
            );
            let second = first.translate(Vec2::new(0.0, dialog::SEARCH + dialog::GAP));
            let response =
                dialog::search(rux, first, "Skill name", "Name, such as release-notes", &mut name);
            if !response.has_focus() && rux.ui.memory(|memory| memory.focused().is_none()) {
                response.request_focus();
            }
            dialog::search(
                rux,
                second,
                "When to use it",
                "When an agent should use it",
                &mut description,
            );
            let ready = !name.trim().is_empty();
            match dialog::footer(rux, inner, &[("Cancel", false)], ("Create", ready)) {
                Some(0) => acts.push(Act::CloseModal),
                Some(_) if ready => acts.push(Act::CreateSkill(name.clone(), description.clone())),
                _ => {}
            }
        });
        if modal::Confirm::Enter.pressed(ui) && !name.trim().is_empty() {
            acts.push(Act::CreateSkill(name.clone(), description.clone()));
        }
    });
    if escaped {
        acts.push(Act::CloseModal);
    }
    if !acts.iter().any(|act| matches!(act, Act::CloseModal | Act::CreateSkill(..))) {
        skills.modal = Some(Modal::NewSkill { name, description });
    }
    acts
}

/// Making sure before a skill is deleted.
fn delete(ctx: &egui::Context, id: &str) -> Vec<Act> {
    let mut acts = Vec::new();
    let height =
        dialog::PAD_Y * 2.0 + dialog::CLOSE + dialog::GAP + 60.0 + dialog::GAP + dialog::BUTTON;
    let (_, escaped) = modal::show_unframed(ctx, DELETE_ID, WIDTH, height, |ui, area| {
        dialog::surface(ui, "skills-delete", area);
        let inner = inner_of(area);
        crate::components::settings_kit::layer(ui, "skills-delete", inner, |rux| {
            if dialog::heading(rux, inner, "Delete Skill") {
                acts.push(Act::CloseModal);
            }
            let words = format!(
                "Delete {id} and every copy of it in an agent's skills folder? {}",
                crate::services::recycle::destination().reassurance()
            );
            crate::components::designer::kit::paragraph(
                rux,
                Pos2::new(inner.left(), inner.top() + dialog::CLOSE + dialog::GAP),
                inner.width(),
                &words,
                12.5,
            );
            match dialog::footer(rux, inner, &[("Cancel", false)], ("Delete", true)) {
                Some(0) => acts.push(Act::CloseModal),
                Some(_) => acts.push(Act::Delete(id.to_owned())),
                None => {}
            }
        });
    });
    if escaped {
        acts.push(Act::CloseModal);
    }
    acts
}
