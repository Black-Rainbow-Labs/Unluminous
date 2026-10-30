//! One skill's page in the pane: a back button, the picture as a banner, the name, the author and what it does,
//! a row of buttons (View / Edit, Update, Reset, Reveal, and the trash), and the agents on this machine with a
//! switch between the project and global and one Install or Remove button an agent.
//!
//! The `SKILL.md` is not drawn here: View / Edit opens it in a tab, where it can be read and changed, and
//! saving it reaches every installed copy.
//!
//! A skill that calls others with the Skill tool has a `Calls` row under its buttons, one chip a skill it calls,
//! which opens that skill's page (`task-2402`). Grill Me's own `SKILL.md` is one line; what it does is in
//! `grilling`.

use egui::{CornerRadius, Pos2, Rect, Vec2};
use rux::icon::Icon;
use rux::Style;

use super::Act;
use crate::components::designer::kit;
use crate::services::plugin_ui::Look;
use crate::services::skills::harness::{self, Scope, HARNESSES};
use crate::services::skills::model::{Skill, Source};
use crate::services::skills::Skills;

const MARGIN: f32 = 16.0;
const ROW: f32 = 40.0;
const BUTTONS: f32 = 40.0;
/// The room above the first agent's row, for the heading and the switch.
const HEAD: f32 = 52.0;

/// Draw the page of the skill `id` and report what was pressed.
pub fn show(skills: &Skills, id: &str, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Act> {
    let Some(skill) = skills.shelf.get(id) else { return vec![Act::Back] };
    let area = ui.available_rect_before_wrap();
    let scale = look.scale();
    let mut acts = Vec::new();
    let inner = area.shrink(MARGIN * scale);
    let width = inner.width() - 10.0 * scale;
    // A skill with no picture, the person's own or one found on the machine, has the stand in its card has.
    let picture = super::picture(skills, ui.ctx(), &skill.id);
    let banner_height = (width * 0.34).min(230.0 * scale);
    let description_height = description_height(ui, skill, width, scale);
    let rows = agents_here(skills).len() as f32;
    let called: Vec<(String, String)> = skill
        .entry
        .requires
        .iter()
        .map(|id| {
            let name = skills.shelf.get(id).map(|other| other.entry.name.clone());
            (id.clone(), name.unwrap_or_else(|| id.clone()))
        })
        .collect();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical().id_salt(("skills-detail", id)).auto_shrink([false, false]).show(
        &mut child,
        |ui| {
            let top = ui.cursor().min;
            let back = Rect::from_min_size(
                Pos2::new(top.x, top.y),
                Vec2::new(120.0 * scale, 34.0 * scale),
            );
            let banner = Rect::from_min_size(
                Pos2::new(top.x, back.bottom() + 12.0 * scale),
                Vec2::new(width, banner_height),
            );
            let words = Rect::from_min_size(
                Pos2::new(top.x, banner.bottom() + 18.0 * scale),
                Vec2::new(width, 56.0 * scale + description_height),
            );
            let buttons = Rect::from_min_size(
                Pos2::new(top.x, words.bottom() + 12.0 * scale),
                Vec2::new(width, BUTTONS * scale),
            );
            let calls_height = match called.is_empty() {
                true => 0.0,
                false => (BUTTONS + 10.0) * scale,
            };
            let calls = Rect::from_min_size(
                Pos2::new(top.x, buttons.bottom() + 10.0 * scale),
                Vec2::new(width, if called.is_empty() { 0.0 } else { BUTTONS * scale }),
            );
            let agents = Rect::from_min_size(
                Pos2::new(top.x, buttons.bottom() + calls_height + 22.0 * scale),
                Vec2::new(width, (rows * ROW + HEAD + 8.0) * scale),
            );
            kit::layer(
                ui,
                scale,
                ("skills-detail-top", id),
                Rect::from_min_max(back.min, buttons.max.max(calls.max)),
                |rux| {
                    if kit::button(rux, back, "All skills", Some(Icon::ChevLeft), kit::Look::Ghost)
                    {
                        acts.push(Act::Back);
                    }
                    let radius = 20.0 * scale;
                    rux.chrome.surface(
                        banner,
                        radius,
                        rux.theme().chat.well,
                        rux.theme().chat.carved_sm(),
                    );
                    let corners = CornerRadius::same(radius as u8);
                    match &picture {
                        Some(texture) => {
                            super::paint_cover(rux.painter(), banner, texture, corners)
                        }
                        None => super::paint_placeholder(rux.painter(), banner, &skill.id, corners),
                    }
                    the_words(rux, words, skill, scale);
                    acts.extend(the_buttons(rux, buttons, skill, scale));
                    acts.extend(the_calls(rux, calls, &called, scale));
                },
            );
            kit::layer(ui, scale, ("skills-detail-agents", id), agents, |rux| {
                acts.extend(the_agents(skills, rux, agents, skill, scale));
            });
            ui.allocate_space(Vec2::new(width, agents.bottom() + 20.0 * scale - top.y));
        },
    );
    acts
}

/// How tall the description comes out at `width`, measured in the face, size and leading [`kit::paragraph`] draws
/// it in. It was measured in egui's own face at a leading of 1.25 and drawn at 1.45, so a long description ran
/// under the buttons below it (`task-2405`).
fn description_height(ui: &egui::Ui, skill: &Skill, width: f32, scale: f32) -> f32 {
    let style = Style::sans(12.5 * scale).leading(1.45);
    let ink = egui::Color32::WHITE; // any ground: measured, never painted
    let galley =
        rux::text::wrapped(ui.painter(), style, &description_of(skill), ink, width.max(40.0));
    galley.size().y
}

fn description_of(skill: &Skill) -> String {
    match skill.entry.description.is_empty() {
        true => skill.entry.summary.clone(),
        false => skill.entry.description.clone(),
    }
}

/// The name, who made it, and what it does.
fn the_words(rux: &mut rux::Rux<'_>, rect: Rect, skill: &Skill, scale: f32) {
    kit::heading(
        rux,
        Pos2::new(rect.left(), rect.top() + 12.0 * scale),
        &skill.entry.name,
        22.0,
        rect.width(),
    );
    // Who made it, where it came from and how many stars, and nothing else: the licence, the `Most used` heading
    // and the catalogue's popularity note, which carries a date, ran the line off the end of the page and said
    // nothing a person choosing a skill needs (`task-2405`).
    let mut by = vec![];
    if !skill.entry.author.is_empty() {
        by.push(format!("by {}", skill.entry.author));
    }
    if skill.source != Source::Catalogue {
        by.push(skill.source.heading().to_owned());
    }
    if skill.entry.stars > 0 && skill.source == Source::Catalogue {
        by.push(format!("{} GitHub stars", group_digits(skill.entry.stars)));
    }
    kit::quiet(
        rux,
        Pos2::new(rect.left(), rect.top() + 36.0 * scale),
        &by.join("  \u{00B7}  "),
        11.5,
        rect.width(),
    );
    kit::paragraph(
        rux,
        Pos2::new(rect.left(), rect.top() + 54.0 * scale),
        rect.width(),
        &description_of(skill),
        12.5,
    );
}

/// `184549` as `184,549`.
fn group_digits(number: u64) -> String {
    let digits = number.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The buttons: View / Edit, Update and Reset, then Reveal and the trash as round marks at the end; the trash removes the skill from every
/// agent (and deletes one of the person's own). The trash is dimmed while the skill is installed nowhere, unless
/// it is one of the person's own, which can always be deleted. Installing is the agent rows' job alone.
fn the_buttons(rux: &mut rux::Rux<'_>, rect: Rect, skill: &Skill, scale: f32) -> Vec<Act> {
    let mut acts = Vec::new();
    let mut buttons: Vec<(String, Option<Icon>, kit::Look, Act)> = vec![(
        "View / Edit".into(),
        Some(Icon::Edit),
        kit::Look::Chip,
        Act::Edit(skill.id.clone()),
    )];
    if skill.update {
        buttons.push((
            "Update".into(),
            Some(Icon::Download),
            kit::Look::Chip,
            Act::Update(skill.id.clone()),
        ));
    }
    if skill.edited && matches!(skill.source, Source::Unluminous | Source::Catalogue) {
        buttons.push((
            "Reset".into(),
            Some(Icon::Rerun),
            kit::Look::Chip,
            Act::Reset(skill.id.clone()),
        ));
    }
    let trash = Rect::from_center_size(
        Pos2::new(rect.right() - rect.height() / 2.0, rect.center().y),
        Vec2::splat(rect.height()),
    );
    let mut left = rect.left();
    for (label, icon, look, act) in buttons {
        let width = kit::button_width(rux, &label, icon.is_some());
        if left + width > reveal_left(trash, rect, scale) - 8.0 * scale {
            break;
        }
        let button =
            Rect::from_min_size(Pos2::new(left, rect.top()), Vec2::new(width, rect.height()));
        if kit::button(rux, button, &label, icon, look) {
            acts.push(act);
        }
        left += width + 8.0 * scale;
    }
    // Reveal, the same round button as the trash, beside it: dimmed while there is no folder to show.
    let reveal = trash.translate(Vec2::new(-(rect.height() + 8.0 * scale), 0.0));
    let folder =
        skill.library.clone().or_else(|| skill.copies.first().map(|copy| copy.folder.clone()));
    let reveal_name = format!("Show {} in the file manager", skill.entry.name);
    if icon_button(rux, reveal, Icon::Folder, &reveal_name, folder.is_some()) {
        if let Some(folder) = folder {
            acts.push(Act::Reveal(folder));
        }
    }
    let mine = matches!(skill.source, Source::Mine | Source::Found);
    let usable = mine || skill.is_installed();
    let name = match mine {
        true => format!("Delete {}", skill.entry.name),
        false => format!("Remove {} from every agent", skill.entry.name),
    };
    if icon_button(rux, trash, Icon::Trash, &name, usable) {
        acts.push(match mine {
            true => Act::AskToDelete(skill.id.clone()),
            false => Act::RemoveEverywhere(skill.id.clone()),
        });
    }
    acts
}

/// The `Calls` row: the word, then one chip a skill this one calls with the Skill tool, each opening that skill's
/// page. A chip is named by the skill's name, which is what a person and a test call it.
fn the_calls(
    rux: &mut rux::Rux<'_>,
    rect: Rect,
    called: &[(String, String)],
    scale: f32,
) -> Vec<Act> {
    let mut acts = Vec::new();
    if called.is_empty() {
        return acts;
    }
    let style = rux.zs(Style::mono(11.0));
    let ink = rux.theme().ink;
    let galley = rux.text(style, "CALLS", ink.i500);
    let label_width = galley.size().x;
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(rect.left() + 4.0 * scale, rect.center().y),
        galley,
        style,
        ink.i500,
    );
    let mut left = rect.left() + label_width + 18.0 * scale;
    for (id, name) in called {
        let width = kit::button_width(rux, name, false);
        if left + width > rect.right() {
            break;
        }
        let chip =
            Rect::from_min_size(Pos2::new(left, rect.top()), Vec2::new(width, rect.height()));
        if kit::button(rux, chip, name, None, kit::Look::Chip) {
            acts.push(Act::Open(id.clone()));
        }
        left += width + 8.0 * scale;
    }
    acts
}

/// Where the Reveal button starts, which the words buttons stop short of.
fn reveal_left(trash: Rect, rect: Rect, scale: f32) -> f32 {
    trash.left() - rect.height() - 8.0 * scale
}

/// A round button with a mark, dimmed and not pressable while `usable` is false. `name` is what a person and a test
/// call it.
fn icon_button(rux: &mut rux::Rux<'_>, rect: Rect, icon: Icon, name: &str, usable: bool) -> bool {
    let chat = rux.theme().chat;
    let sense = if usable { egui::Sense::click() } else { egui::Sense::hover() };
    let response = rux.ui.interact(rect, rux.ui.id().with(("skills-icon-button", name)), sense);
    if usable && response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let radius = rect.height() / 2.0;
    let ink = match (usable, response.hovered()) {
        (false, _) => {
            rux.chrome.surface(rect, radius, chat.well, chat.carved_sm());
            rux.theme().ink.i300
        }
        (true, hovered) => {
            rux.chrome.surface(
                rect,
                radius,
                rux::Fill::gradient(chat.chip, rect),
                chat.raised_chip(),
            );
            if hovered {
                rux.theme().ink.i900
            } else {
                rux.theme().ink.i700
            }
        }
    };
    rux.mark(rux::icon::Mark::new(icon, rux.z(15.0)), rect.center(), ink);
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, usable, named.clone()));
    usable && response.clicked()
}

/// The agents a skill's page lists: the ones found on this machine, or the ones an install goes to when none is.
pub fn agents_here(skills: &Skills) -> Vec<&'static harness::Harness> {
    let here: Vec<&'static harness::Harness> =
        HARNESSES.iter().filter(|agent| skills.places.is_here(agent)).collect();
    match here.is_empty() {
        true => skills.config.chosen_harnesses(&skills.places),
        false => here,
    }
}

/// The agents on this machine, a switch between this project and global, and one row an agent: its name, the
/// folder the switch points at, and one button that installs it there or removes it.
fn the_agents(
    skills: &Skills,
    rux: &mut rux::Rux<'_>,
    rect: Rect,
    skill: &Skill,
    scale: f32,
) -> Vec<Act> {
    let mut acts = Vec::new();
    kit::card_surface(rux, rect, 20.0 * scale);
    let style = rux.zs(Style::mono(11.0));
    let ink = rux.theme().ink;
    let switch = Rect::from_min_size(
        Pos2::new(rect.right() - 16.0 * scale - 190.0 * scale, rect.top() + 10.0 * scale),
        Vec2::new(190.0 * scale, 32.0 * scale),
    );
    // Cut short before the switch, which it ran under in a narrow pane (`task-2405`).
    let room = (switch.left() - rect.left() - 28.0 * scale).max(0.0);
    let galley = rux.elided(style, "AGENTS ON THIS MACHINE", ink.i500, room);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(rect.left() + 16.0 * scale, rect.top() + 26.0 * scale),
        galley,
        style,
        ink.i500,
    );
    let scope = skills.scope_now();
    let choices = vec![Scope::Project.label().to_owned(), Scope::Global.label().to_owned()];
    let chosen = if scope == Scope::Project { 0 } else { 1 };
    if let Some(picked) = kit::pill_switch(rux, switch, &choices, Some(chosen), "Install for", true)
    {
        acts.push(Act::Scope(if picked == 0 { Scope::Project } else { Scope::Global }));
    }
    let project = skills.project.as_deref();
    for (index, agent) in agents_here(skills).into_iter().enumerate() {
        let y = rect.top() + (HEAD + index as f32 * ROW + ROW / 2.0) * scale;
        let name_style = rux.zs(Style::sans(12.5).medium());
        let name = rux.elided(name_style, agent.name, ink.i900, 150.0 * scale);
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(rect.left() + 16.0 * scale, y),
            name,
            name_style,
            ink.i900,
        );
        let button = Rect::from_center_size(
            Pos2::new(rect.right() - 16.0 * scale - 50.0 * scale, y),
            Vec2::new(100.0 * scale, 30.0 * scale),
        );
        let folder_room = button.left() - rect.left() - 186.0 * scale;
        let own = skill.own_copy(&skills.places, agent, scope, project).is_some();
        let seen = skill.seen_by(&skills.places, agent, scope, project);
        let shared = seen && !own;
        let note = match shared {
            true => {
                let reads = skills
                    .places
                    .read_folders(agent, scope, project)
                    .into_iter()
                    .find(|folder| folder.join(&skill.id).join("SKILL.md").exists())
                    .map(|folder| harness::shown(&skills.places, &folder, project))
                    .unwrap_or_default();
                format!("reads it from {reads}")
            }
            false => skills
                .places
                .write_folder(agent, scope, project)
                .map(|folder| harness::shown(&skills.places, &folder, project))
                .unwrap_or_default(),
        };
        kit::mono(rux, Pos2::new(rect.left() + 176.0 * scale, y), &note, 10.5, folder_room);
        if shared {
            continue;
        }
        let (words, primary) = if own { ("Remove", false) } else { ("Install", true) };
        let named = format!("{words} for {}", agent.name);
        if row_button(rux, button, words, &named, primary) {
            acts.push(match own {
                true => Act::Remove(skill.id.clone(), Some(agent.id), scope),
                false => Act::Install(skill.id.clone(), Some(agent.id), scope),
            });
        }
    }
    acts
}

/// A row's button: the action's gradient for `Install`, a chip for `Remove`. `name` is what a person and a test
/// call it, which differs from row to row though the words are the same.
fn row_button(rux: &mut rux::Rux<'_>, rect: Rect, words: &str, name: &str, primary: bool) -> bool {
    let chat = rux.theme().chat;
    let response =
        rux.ui.interact(rect, rux.ui.id().with(("skills-row-button", name)), egui::Sense::click());
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let radius = rect.height() / 2.0;
    let ink = match primary {
        true => {
            rux.chrome.surface(
                rect,
                radius,
                rux::Fill::gradient(chat.action, rect),
                chat.action_raised(),
            );
            crate::theme::color::on_accent()
        }
        false => {
            rux.chrome.surface(
                rect,
                radius,
                rux::Fill::gradient(chat.chip, rect),
                chat.raised_chip(),
            );
            match response.hovered() {
                true => rux.theme().ink.i900,
                false => rux.theme().ink.i700,
            }
        }
    };
    let style = rux.zs(Style::sans(11.5).medium());
    let galley = rux.text(style, words, ink);
    rux::text::draw_centred(rux.painter(), rect, galley, ink);
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone()));
    response.clicked()
}
