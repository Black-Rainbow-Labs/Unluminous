//! The Skills Manager pane: a header with the mark, the count, `Check for Updates` and `New Skill`; a search well
//! and a switch for which skills are listed; a switch for where an install goes; and the cards, in a grid as wide
//! as the pane allows, under a heading for each place a skill comes from.

use egui::{Pos2, Rect, Vec2};
use rux::icon::Icon;
use rux::{Rux, Style};

use super::{card, Act};
use crate::components::designer::kit;
use crate::services::plugin_ui::Look;
use crate::services::skills::harness::Scope;
use crate::services::skills::model::{Skill, Source};
use crate::services::skills::{Filter, Skills};

const MARGIN: f32 = 16.0;
const GAP: f32 = 12.0;
const HEADER: f32 = 48.0;
const SEARCH: f32 = 40.0;
const SCOPE: f32 = 34.0;
const NOTE: f32 = 22.0;
/// The narrowest a card is drawn before the grid has one column fewer.
const CARD_MIN: f32 = 230.0;
/// The room a group's heading takes.
const HEADING: f32 = 34.0;

/// Draw the pane and report what was pressed.
pub fn show(skills: &mut Skills, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Act> {
    let area = ui.available_rect_before_wrap();
    let scale = look.scale();
    let mut acts = Vec::new();
    if area.width() < 80.0 || area.height() < 80.0 {
        return acts;
    }
    let inner = area.shrink(MARGIN * scale);
    let header = Rect::from_min_size(inner.min, Vec2::new(inner.width(), HEADER * scale));
    let search = Rect::from_min_size(
        Pos2::new(inner.left(), header.bottom() + GAP * scale),
        Vec2::new(inner.width(), SEARCH * scale),
    );
    let controls = Rect::from_min_size(
        Pos2::new(inner.left(), search.bottom() + GAP * scale),
        Vec2::new(inner.width(), SCOPE * scale),
    );
    let note = Rect::from_min_size(
        Pos2::new(inner.left(), controls.bottom() + 4.0 * scale),
        Vec2::new(inner.width(), NOTE * scale),
    );
    let list = Rect::from_min_max(Pos2::new(inner.left(), note.bottom() + 6.0 * scale), inner.max);
    let mut query = skills.search.clone();
    kit::layer(ui, scale, "skills-pane-header", inner, |rux| {
        the_header(skills, rux, header, scale, &mut acts);
        rux::components::TextInput::search(&mut query)
            .hint("Search skills")
            .label("Search skills")
            .id_salt("skills-search")
            .show(rux, search);
        the_controls(skills, rux, controls, scale, &mut acts);
        let names: Vec<&str> = skills
            .config
            .chosen_harnesses(&skills.places)
            .iter()
            .map(|harness| harness.name)
            .collect();
        let words = format!(
            "Installs into {}. Settings chooses the agents; a skill's page installs for any one.",
            names.join(", ")
        );
        kit::quiet(
            rux,
            Pos2::new(note.left() + 4.0 * scale, note.center().y),
            &words,
            11.0,
            note.width() - 8.0 * scale,
        );
    });
    skills.search = query;
    acts.extend(the_list(skills, ui, list, scale));
    acts
}

/// The blocks mark in a carved disc, `Skills`, how many, the status line, and the two buttons.
fn the_header(skills: &Skills, rux: &mut Rux<'_>, rect: Rect, scale: f32, acts: &mut Vec<Act>) {
    let disc = Rect::from_center_size(
        Pos2::new(rect.left() + 20.0 * scale, rect.center().y),
        Vec2::splat(40.0 * scale),
    );
    rux::components::chip_disc(rux, disc);
    let sky = rux.theme().chat.sky;
    crate::theme::svg_icon::paint(
        rux.painter(),
        disc.center(),
        20.0 * scale,
        &crate::theme::svg_icon::BLOCKS,
        sky,
    );
    let new_width = kit::button_width(rux, "New Skill", true);
    let new = Rect::from_center_size(
        Pos2::new(rect.right() - new_width / 2.0, rect.center().y),
        Vec2::new(new_width, 38.0 * scale),
    );
    let check = Rect::from_center_size(
        Pos2::new(new.left() - 10.0 * scale - 19.0 * scale, rect.center().y),
        Vec2::splat(38.0 * scale),
    );
    let left = disc.right() + 12.0 * scale;
    let room = (check.left() - left - 12.0 * scale).max(0.0);
    kit::heading(rux, Pos2::new(left, rect.center().y - 9.0 * scale), "Skills", 18.0, room);
    let updates = skills.shelf.skills.iter().filter(|skill| skill.update).count();
    let installed = skills.shelf.skills.iter().filter(|skill| skill.is_installed()).count();
    let line = match (&skills.busy, &skills.status) {
        (Some(busy), _) => format!("{busy}\u{2026}"),
        _ => format!(
            "{} skills \u{00B7} {} installed{}",
            skills.shelf.skills.len(),
            installed,
            match updates {
                0 => String::new(),
                many => format!(" \u{00B7} {many} updates"),
            }
        ),
    };
    kit::quiet(rux, Pos2::new(left, rect.center().y + 11.0 * scale), &line, 11.5, room);
    if kit::chip_button(rux, check, Icon::Rerun, 38.0, "Check for Updates") {
        acts.push(Act::Check);
    }
    if kit::button(rux, new, "New Skill", Some(Icon::Plus), kit::Look::Primary) {
        acts.push(Act::NewSkill);
    }
}

/// Where an install goes on the left, and which skills are listed on the right, each a pill switch as wide as its
/// words.
fn the_controls(skills: &Skills, rux: &mut Rux<'_>, rect: Rect, scale: f32, acts: &mut Vec<Act>) {
    let scopes = vec![Scope::Project.label().to_owned(), Scope::Global.label().to_owned()];
    let filters: Vec<String> = Filter::ALL.iter().map(|filter| filter.title().to_owned()).collect();
    let scope_width = switch_width(rux, &scopes);
    let filter_width = switch_width(rux, &filters);
    let fits = scope_width + filter_width + 12.0 * scale <= rect.width();
    let (scope_width, filter_width) = match fits {
        true => (scope_width, filter_width),
        false => ((rect.width() - 12.0 * scale) * 0.42, (rect.width() - 12.0 * scale) * 0.58),
    };
    let scope_rect = Rect::from_min_size(rect.min, Vec2::new(scope_width, rect.height()));
    let filter_rect = Rect::from_min_size(
        Pos2::new(rect.right() - filter_width, rect.top()),
        Vec2::new(filter_width, rect.height()),
    );
    let chosen = match skills.scope_now() {
        Scope::Project => 0,
        Scope::Global => 1,
    };
    if let Some(picked) =
        kit::pill_switch(rux, scope_rect, &scopes, Some(chosen), "Install for", true)
    {
        acts.push(Act::Scope(if picked == 0 { Scope::Project } else { Scope::Global }));
    }
    let chosen = Filter::ALL.iter().position(|filter| *filter == skills.filter);
    if let Some(picked) = kit::pill_switch(rux, filter_rect, &filters, chosen, "Skills shown", true)
    {
        acts.push(Act::Filter(Filter::ALL[picked]));
    }
}

/// How wide a small pill switch is for its words.
fn switch_width(rux: &Rux<'_>, choices: &[String]) -> f32 {
    let style = rux.zs(Style::sans(11.5).medium());
    choices.iter().map(|choice| rux.measure(style, choice).x + rux.z(44.0)).sum::<f32>()
        + rux.z(12.0)
}

/// The groups of cards, scrolled.
fn the_list(skills: &Skills, ui: &mut egui::Ui, list: Rect, scale: f32) -> Vec<Act> {
    let mut acts = Vec::new();
    let listed: Vec<&Skill> = skills.listed();
    if listed.is_empty() {
        kit::layer(ui, scale, "skills-pane-empty", list, |rux| {
            let words = match skills.filter {
                Filter::Updates => "Every skill is up to date.",
                Filter::Installed => "No skill is installed for these agents yet. Press + on a card to install one.",
                Filter::Mine => "You have no skills of your own yet. New Skill makes one and opens its SKILL.md.",
                Filter::All => "No skill matches that search.",
            };
            kit::paragraph(
                rux,
                Pos2::new(list.left() + 4.0 * scale, list.top() + 8.0 * scale),
                list.width() - 8.0 * scale,
                words,
                12.5,
            );
        });
        return acts;
    }
    let width = list.width() - 8.0 * scale;
    let columns = (((width + GAP * scale) / ((CARD_MIN + GAP) * scale)).floor() as usize).max(1);
    let card_width = (width - GAP * scale * (columns - 1) as f32) / columns as f32;
    let card_height = card::height(card_width, scale);
    let groups = grouped(&listed);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
    child.set_clip_rect(list.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical().id_salt("skills-pane-list").auto_shrink([false, false]).show(
        &mut child,
        |ui| {
            let mut y = ui.cursor().min.y;
            let left = ui.cursor().min.x;
            for (source, members) in &groups {
                let heading =
                    Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, HEADING * scale));
                kit::layer(ui, scale, ("skills-heading", source.name()), heading, |rux| {
                    let style = rux.zs(Style::mono(11.0));
                    let ink = rux.theme().ink.i500;
                    let words =
                        format!("{}  \u{00B7}  {}", source.heading().to_uppercase(), members.len());
                    let galley = rux.text(style, &words, ink);
                    rux::text::draw_left_capitals(
                        rux.painter(),
                        Pos2::new(heading.left() + 2.0 * scale, heading.center().y),
                        galley,
                        style,
                        ink,
                    );
                });
                y += HEADING * scale;
                // A skill of the person's own, or one found on the machine, has no picture: it is a row rather
                // than a card, so the pictures the catalogue has are what the grid is made of.
                if matches!(source, Source::Mine | Source::Found) {
                    for skill in members {
                        let rect = Rect::from_min_size(
                            Pos2::new(left, y),
                            Vec2::new(width, card::ROW * scale),
                        );
                        if ui.is_rect_visible(rect.expand(kit::REACH * scale)) {
                            acts.extend(card::row(skills, ui, rect, skill, scale));
                        }
                        y += (card::ROW + 8.0) * scale;
                    }
                    y += GAP * scale;
                    continue;
                }
                for (index, skill) in members.iter().enumerate() {
                    let (row, column) = (index / columns, index % columns);
                    let rect = Rect::from_min_size(
                        Pos2::new(
                            left + column as f32 * (card_width + GAP * scale),
                            y + row as f32 * (card_height + GAP * scale),
                        ),
                        Vec2::new(card_width, card_height),
                    );
                    if ui.is_rect_visible(rect.expand(kit::REACH * scale)) {
                        acts.extend(card::show(skills, ui, rect, skill, scale));
                    }
                }
                let rows = members.len().div_ceil(columns);
                y += rows as f32 * (card_height + GAP * scale) + GAP * scale;
            }
            let top = ui.cursor().min.y;
            ui.allocate_space(Vec2::new(width, (y - top).max(0.0)));
        },
    );
    acts
}

/// The skills under their headings, in the order the headings are drawn.
fn grouped<'a>(listed: &[&'a Skill]) -> Vec<(Source, Vec<&'a Skill>)> {
    let mut groups: Vec<(Source, Vec<&'a Skill>)> = Vec::new();
    for source in [Source::Unluminous, Source::Catalogue, Source::Mine, Source::Found] {
        let members: Vec<&Skill> =
            listed.iter().copied().filter(|skill| skill.source == source).collect();
        if !members.is_empty() {
            groups.push((source, members));
        }
    }
    groups
}
