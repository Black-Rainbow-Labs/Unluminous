//! One skill's page in the pane: a back button, the picture as a banner, the name, the author and what it does,
//! the buttons for what can be done with it, a row for every agent with a switch for this project and one for
//! everyone, and its `SKILL.md` drawn as Markdown.

use std::cell::RefCell;

use egui::{CornerRadius, Pos2, Rect, Vec2};
use rux::icon::Icon;
use rux::Style;

use super::Act;
use crate::components::designer::kit;
use crate::components::markdown_text::{self, Rendered};
use crate::services::plugin_ui::Look;
use crate::services::skills::harness::{self, Scope, HARNESSES};
use crate::services::skills::model::{Skill, Source};
use crate::services::skills::{commands, Skills};

const MARGIN: f32 = 16.0;
const ROW: f32 = 40.0;
const BUTTONS: f32 = 40.0;

/// Which SKILL.md was laid out: the skill, its hash, the width and the type size.
type TextKey = (String, String, u32, u32);

thread_local! {
    /// The `SKILL.md` laid out last, by skill, hash and width, so a frame that changed nothing lays out nothing.
    static TEXT: RefCell<Option<(TextKey, Rendered)>> = const { RefCell::new(None) };
}

/// Draw the page of the skill `id` and report what was pressed.
pub fn show(skills: &Skills, id: &str, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Act> {
    let Some(skill) = skills.shelf.get(id) else { return vec![Act::Back] };
    let area = ui.available_rect_before_wrap();
    let scale = look.scale();
    let mut acts = Vec::new();
    let inner = area.shrink(MARGIN * scale);
    let width = inner.width() - 10.0 * scale;
    let text = skill_text(skills, skill);
    let rendered_height = laid_out(skill, &text, look, width - 32.0 * scale);
    let banner_height = (width * 0.34).min(230.0 * scale);
    let description_height = description_height(ui, skill, width, scale);
    let rows = HARNESSES.len() as f32;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical().id_salt(("skills-detail", id)).auto_shrink([false, false]).show(
        &mut child,
        |ui| {
            let top = ui.cursor().min;
            let mut y = top.y;
            let back =
                Rect::from_min_size(Pos2::new(top.x, y), Vec2::new(120.0 * scale, 34.0 * scale));
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
            let agents = Rect::from_min_size(
                Pos2::new(top.x, buttons.bottom() + 22.0 * scale),
                Vec2::new(width, (rows * ROW + 40.0) * scale),
            );
            let markdown = Rect::from_min_size(
                Pos2::new(top.x, agents.bottom() + 22.0 * scale),
                Vec2::new(width, rendered_height + 72.0 * scale),
            );
            y = markdown.bottom() + 20.0 * scale - y;
            let picture = super::picture(skills, ui.ctx(), &skill.id);
            kit::layer(
                ui,
                scale,
                ("skills-detail-top", id),
                Rect::from_min_max(back.min, buttons.max),
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
                    match &picture {
                        Some(texture) => super::paint_cover(
                            rux.painter(),
                            banner,
                            texture,
                            CornerRadius::same(radius as u8),
                        ),
                        None => super::paint_placeholder(
                            rux.painter(),
                            banner,
                            &skill.id,
                            CornerRadius::same(radius as u8),
                        ),
                    }
                    the_words(rux, words, skill, scale);
                    acts.extend(the_buttons(skills, rux, buttons, skill, scale));
                },
            );
            kit::layer(ui, scale, ("skills-detail-agents", id), agents, |rux| {
                acts.extend(the_agents(skills, rux, agents, skill, scale));
            });
            // A long SKILL.md makes a card taller than the largest texture a graphics card takes, so only the part of
            // the card near the screen is drawn; its cut edges are past the clip, where nobody sees them.
            let near = ui.clip_rect().expand(60.0 * scale);
            let drawn = markdown.intersect(near);
            if drawn.height() > 0.0 {
                kit::card(ui, scale, drawn, 20.0 * scale);
            }
            kit::layer(ui, scale, ("skills-detail-md", id), markdown, |rux| {
                let style = rux.zs(Style::mono(11.0));
                let ink = rux.theme().ink.i500;
                let label =
                    format!("SKILL.md{}", if skill.edited { "  \u{00B7}  EDITED" } else { "" });
                let galley = rux.text(style, &label, ink);
                rux::text::draw_left_capitals(
                    rux.painter(),
                    Pos2::new(markdown.left() + 16.0 * scale, markdown.top() + 22.0 * scale),
                    galley,
                    style,
                    ink,
                );
            });
            let body = Rect::from_min_size(
                Pos2::new(markdown.left() + 16.0 * scale, markdown.top() + 44.0 * scale),
                Vec2::new(width - 32.0 * scale, rendered_height + 8.0 * scale),
            );
            TEXT.with(|kept| {
                if let Some((_, rendered)) = kept.borrow().as_ref() {
                    markdown_text::show(ui, body, rendered, look.renderer, 0.0);
                }
            });
            ui.allocate_space(Vec2::new(width, y));
        },
    );
    acts
}

/// The `SKILL.md` as it is now.
fn skill_text(_skills: &Skills, skill: &Skill) -> String {
    commands::skill_md(skill)
}

/// Lay the `SKILL.md` out at `width` into [`TEXT`], unless it is there already for this text and width, and
/// answer how tall it is.
fn laid_out(skill: &Skill, text: &str, look: &Look<'_>, width: f32) -> f32 {
    let key = (
        skill.id.clone(),
        skill.hash.clone(),
        width.round() as u32,
        (look.font_size * 10.0) as u32,
    );
    let kept = TEXT.with(|kept| {
        kept.borrow()
            .as_ref()
            .filter(|(known, _)| *known == key)
            .map(|(_, rendered)| rendered.height())
    });
    if let Some(height) = kept {
        return height;
    }
    let palette = &look.palette;
    let colors = markdown_text::Colors {
        text: palette.text,
        strong: palette.text_strong,
        code: palette.text,
        link: palette.accent,
        quiet: palette.text_dim,
        rule: palette.text_dim,
    };
    let body = crate::services::skills::frontmatter::body(text);
    let rendered = markdown_text::render(
        &body,
        look.renderer,
        &look.font_family,
        13.0 * look.scale(),
        colors,
        width.max(80.0),
        look.highlighter,
    );
    let height = rendered.height();
    TEXT.with(|kept| *kept.borrow_mut() = Some((key, rendered)));
    height
}

/// How tall the description comes out at `width`.
fn description_height(ui: &egui::Ui, skill: &Skill, width: f32, scale: f32) -> f32 {
    let words = description_of(skill);
    let font = egui::FontId::proportional(12.5 * scale);
    let galley = ui.painter().layout(words, font, egui::Color32::WHITE, width); // any ground: measured, never painted
    galley.size().y * 1.25
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
    let mut by = vec![];
    if !skill.entry.author.is_empty() {
        by.push(format!("by {}", skill.entry.author));
    }
    by.push(skill.source.heading().to_owned());
    if !skill.entry.license.is_empty() {
        by.push(skill.entry.license.clone());
    }
    if !skill.entry.popularity.is_empty() && skill.source == Source::Catalogue {
        by.push(skill.entry.popularity.clone());
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

/// The buttons: install or remove for the chosen agents, edit, update, reset or delete, and reveal.
fn the_buttons(
    skills: &Skills,
    rux: &mut rux::Rux<'_>,
    rect: Rect,
    skill: &Skill,
    scale: f32,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let scope = skills.scope_now();
    let installed = super::card::installed_for_the_chosen(skills, skill, scope);
    let install = match installed {
        true => format!("Remove from {}", scope.label()),
        false => format!("Install for {}", scope.label()),
    };
    let mut buttons: Vec<(String, Option<Icon>, kit::Look, Act)> = vec![(
        install,
        Some(if installed { Icon::Cross } else { Icon::Plus }),
        kit::Look::Primary,
        match installed {
            true => Act::Remove(skill.id.clone(), None, scope),
            false => Act::Install(skill.id.clone(), None, scope),
        },
    )];
    buttons.push((
        "Edit SKILL.md".into(),
        Some(Icon::Edit),
        kit::Look::Chip,
        Act::Edit(skill.id.clone()),
    ));
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
    if matches!(skill.source, Source::Mine | Source::Found) {
        buttons.push((
            "Delete".into(),
            Some(Icon::Trash),
            kit::Look::Chip,
            Act::AskToDelete(skill.id.clone()),
        ));
    }
    if let Some(folder) =
        skill.library.clone().or_else(|| skill.copies.first().map(|copy| copy.folder.clone()))
    {
        buttons.push(("Reveal".into(), Some(Icon::Folder), kit::Look::Ghost, Act::Reveal(folder)));
    }
    let mut left = rect.left();
    for (label, icon, look, act) in buttons {
        let width = kit::button_width(rux, &label, icon.is_some());
        if left + width > rect.right() {
            break;
        }
        let button =
            Rect::from_min_size(Pos2::new(left, rect.top()), Vec2::new(width, rect.height()));
        if kit::button(rux, button, &label, icon, look) {
            acts.push(act);
        }
        left += width + 8.0 * scale;
    }
    acts
}

/// A row for every agent: its name, its folder, and a switch for this project and one for everyone.
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
    let galley = rux.text(style, "AGENTS", ink.i500);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(rect.left() + 16.0 * scale, rect.top() + 22.0 * scale),
        galley,
        style,
        ink.i500,
    );
    let project = skills.project.as_deref();
    let chosen = skills.config.chosen_harnesses(&skills.places);
    let mut ordered: Vec<&'static harness::Harness> = chosen.clone();
    ordered.extend(HARNESSES.iter().filter(|harness| !chosen.contains(harness)));
    for (index, agent) in ordered.into_iter().enumerate() {
        let y = rect.top() + (40.0 + index as f32 * ROW + ROW / 2.0) * scale;
        let here = skills.places.is_here(agent);
        let name_ink = if here { ink.i900 } else { ink.i500 };
        let name_style = rux.zs(Style::sans(12.5).medium());
        let name = rux.elided(name_style, agent.name, name_ink, 150.0 * scale);
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(rect.left() + 16.0 * scale, y),
            name,
            name_style,
            name_ink,
        );
        let switches = 2.0 * 104.0 * scale + 8.0 * scale;
        let folder_room = rect.width() - 32.0 * scale - 160.0 * scale - switches - 10.0 * scale;
        let folder = skills
            .places
            .write_folder(agent, Scope::Global, project)
            .map(|folder| harness::shown(&skills.places, &folder, project))
            .unwrap_or_default();
        let note = if here { folder } else { format!("{folder}  \u{00B7}  not on this machine") };
        kit::mono(rux, Pos2::new(rect.left() + 176.0 * scale, y), &note, 10.5, folder_room);
        let mut right = rect.right() - 16.0 * scale;
        for scope in [Scope::Global, Scope::Project] {
            let seen = skill.seen_by(&skills.places, agent, scope, project);
            let own = skill.own_copy(&skills.places, agent, scope, project).is_some();
            let label = format!(
                "{} {}",
                agent.name,
                if scope == Scope::Global { "everyone" } else { "project" }
            );
            let words = match scope {
                Scope::Project => "Project",
                Scope::Global => "Everyone",
            };
            let button = Rect::from_center_size(
                Pos2::new(right - 52.0 * scale, y),
                Vec2::new(100.0 * scale, 30.0 * scale),
            );
            right -= 108.0 * scale;
            let usable = scope == Scope::Global || project.is_some();
            if !usable {
                continue;
            }
            if toggle(rux, button, words, &label, seen) {
                acts.push(match (seen, own) {
                    (true, true) => Act::Remove(skill.id.clone(), Some(agent.id), scope),
                    _ => Act::Install(skill.id.clone(), Some(agent.id), scope),
                });
            }
        }
    }
    acts
}

/// A switch drawn as a small pill, lit with a tick while `on`. `name` is what a person and a test call it, which
/// has to differ from row to row though the words are the same.
fn toggle(rux: &mut rux::Rux<'_>, rect: Rect, words: &str, name: &str, on: bool) -> bool {
    let chat = rux.theme().chat;
    let response =
        rux.ui.interact(rect, rux.ui.id().with(("skills-toggle", name)), egui::Sense::click());
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let radius = rect.height() / 2.0;
    let ink = match on {
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
            rux.chrome.surface(rect, radius, chat.well, chat.carved_sm());
            match response.hovered() {
                true => rux.theme().ink.i900,
                false => rux.theme().ink.i500,
            }
        }
    };
    let style = rux.zs(Style::sans(11.5).medium());
    let galley = rux.text(style, words, ink);
    let mark = if on { rux.z(16.0) } else { 0.0 };
    let width = galley.size().x + mark;
    let left = rect.center().x - width / 2.0;
    if on {
        rux.mark(
            rux::icon::Mark::new(Icon::Tick, rux.z(11.0)),
            Pos2::new(left + rux.z(5.5), rect.center().y),
            ink,
        );
    }
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(left + mark, rect.center().y),
        galley,
        style,
        ink,
    );
    let named = name.to_owned();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, named.clone())
    });
    response.clicked()
}
