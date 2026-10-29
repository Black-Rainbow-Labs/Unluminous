//! One skill card: the Agent-Tasks card's raised surface, the picture across its top (or the stand in for a skill
//! with none, which every skill of the person's own and every one found on the machine is), the name, the author,
//! three lines of what it does, and a round button that installs it.

use egui::{CornerRadius, Pos2, Rect, Sense, Vec2};
use rux::icon::Icon;
use rux::{Fill, Style};

use super::Act;
use crate::components::designer::kit;
use crate::services::skills::harness::Scope;
use crate::services::skills::model::{Skill, Source};
use crate::services::skills::Skills;

/// How round a card is, the Agent-Tasks card's 20 points.
const RADIUS: f32 = 20.0;
/// How much of a card's width its picture is tall.
const PICTURE: f32 = 0.5;
/// The card under the picture: room, name, author, two lines, the foot.
const BODY: f32 = 150.0;

/// How tall a card `width` wide is. A card wider than [`WIDE`] lays its picture beside its words rather than over
/// them, so one card a row in a narrow pane is a row and not a poster.
pub fn height(width: f32, scale: f32) -> f32 {
    match is_wide(width, scale) {
        true => 168.0 * scale,
        false => width * PICTURE + BODY * scale,
    }
}

/// The width past which a card is laid out sideways.
const WIDE: f32 = 420.0;

fn is_wide(width: f32, scale: f32) -> bool {
    width > WIDE * scale
}

/// Draw one card in `rect` and report what was pressed.
pub fn show(skills: &Skills, ui: &mut egui::Ui, rect: Rect, skill: &Skill, scale: f32) -> Vec<Act> {
    let mut acts = Vec::new();
    let response = ui.interact(rect, ui.id().with(("skill-card", &skill.id)), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let name = skill.entry.name.clone();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Skill: {name}"))
    });
    let scope = skills.scope_now();
    let picture = super::picture(skills, ui.ctx(), &skill.id);
    let installed = installed_for_the_chosen(skills, skill, scope);
    super::roomy_layer(ui, scale, ("skill-card-surface", &skill.id), rect, |rux| {
        let chat = rux.theme().chat;
        let ink = rux.theme().ink;
        let radius = RADIUS * scale;
        // The small raised shadow: the full one's lit half reaches further than rux keeps it, and was cut off in a
        // straight pale line above the dark picture.
        rux.chrome.surface(rect, radius, Fill::gradient(chat.card, rect), chat.raised_sm());
        let wide = is_wide(rect.width(), scale);
        let (top, corners) = match wide {
            true => (
                Rect::from_min_size(
                    rect.min,
                    Vec2::new((rect.width() * 0.36).min(260.0 * scale), rect.height()),
                ),
                CornerRadius { nw: radius as u8, ne: 0, sw: radius as u8, se: 0 },
            ),
            false => (
                Rect::from_min_size(rect.min, Vec2::new(rect.width(), rect.width() * PICTURE)),
                CornerRadius { nw: radius as u8, ne: radius as u8, sw: 0, se: 0 },
            ),
        };
        match &picture {
            Some(texture) => super::paint_cover(rux.painter(), top, texture, corners),
            None => super::paint_placeholder(rux.painter(), top, &skill.id, corners),
        }
        if response.hovered() {
            rux.painter().rect_filled(
                rect,
                radius,
                crate::theme::color::hover_wash().gamma_multiply(8.0 / 255.0),
            );
        }
        the_corner_label(rux, top, skill, scale);
        let inner = match wide {
            true => Rect::from_min_max(
                Pos2::new(top.right() + 18.0 * scale, rect.top() + 14.0 * scale),
                Pos2::new(rect.right() - 16.0 * scale, rect.bottom() - 12.0 * scale),
            ),
            false => Rect::from_min_max(
                Pos2::new(rect.left() + 16.0 * scale, top.bottom() + 14.0 * scale),
                Pos2::new(rect.right() - 16.0 * scale, rect.bottom() - 12.0 * scale),
            ),
        };
        let room = inner.width();
        let title = rux.zs(Style::sans(14.0).semibold());
        let galley = rux.elided(title, &skill.entry.name, ink.i900, room);
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(inner.left(), inner.top() + 8.0 * scale),
            galley,
            title,
            ink.i900,
        );
        let by = match skill.source {
            Source::Found => "Found on this machine".to_owned(),
            _ if skill.entry.author.is_empty() => skill.entry.category.clone(),
            _ if skill.entry.stars > 0 && skill.source == Source::Catalogue => format!(
                "{} \u{00B7} {} stars on GitHub",
                skill.entry.author,
                crate::services::skills::catalogue::stars_said(skill.entry.stars)
            ),
            _ => format!("{} \u{00B7} {}", skill.entry.author, skill.entry.category),
        };
        kit::quiet(rux, Pos2::new(inner.left(), inner.top() + 27.0 * scale), &by, 11.0, room);
        summary(rux, inner, &skill.entry.summary, scale);
        let foot = inner.bottom() - 15.0 * scale;
        if installed {
            let words = format!("Installed \u{00B7} {}", scope.label());
            kit::quiet(
                rux,
                Pos2::new(inner.left(), foot),
                &words,
                11.0,
                inner.width() - 50.0 * scale,
            );
        }
        let button = Rect::from_center_size(
            Pos2::new(inner.right() - 15.0 * scale, foot),
            Vec2::splat(30.0 * scale),
        );
        let (icon, label) = match (skill.update, installed) {
            (true, _) => (Icon::Download, format!("Update {}", skill.entry.name)),
            (false, true) => {
                (Icon::Tick, format!("Remove {} from {}", skill.entry.name, scope.label()))
            }
            (false, false) => {
                (Icon::Plus, format!("Install {} for {}", skill.entry.name, scope.label()))
            }
        };
        let pressed = rux::components::ChipButton::new(icon, 30.0, &label)
            .mark(14.0, Some(1.8))
            .show(rux, button)
            .clicked();
        if pressed {
            acts.push(match (skill.update, installed) {
                (true, _) => Act::Update(skill.id.clone()),
                (false, true) => Act::Remove(skill.id.clone(), None, scope),
                (false, false) => Act::Install(skill.id.clone(), None, scope),
            });
        }
    });
    if acts.is_empty() && response.clicked() {
        acts.push(Act::Open(skill.id.clone()));
    }
    acts
}

/// Whether the agents an install is for all read the skill in `scope`.
pub fn installed_for_the_chosen(skills: &Skills, skill: &Skill, scope: Scope) -> bool {
    let chosen = skills.config.chosen_harnesses(&skills.places);
    !chosen.is_empty()
        && chosen
            .iter()
            .all(|harness| skill.seen_by(&skills.places, harness, scope, skills.project.as_deref()))
}

/// The pill at the top left of the picture: the rank, `Unluminous`, `Mine`, `Edited` or `Update`.
fn the_corner_label(rux: &mut rux::Rux<'_>, top: Rect, skill: &Skill, scale: f32) {
    let words = match (skill.update, skill.edited, skill.source) {
        (true, _, _) => "Update".to_owned(),
        (_, true, _) => "Edited".to_owned(),
        (_, _, Source::Unluminous) => "Unluminous".to_owned(),
        (_, _, Source::Mine) => "Mine".to_owned(),
        (_, _, Source::Found) => "Found".to_owned(),
        (_, _, Source::Catalogue) => skill.entry.category.clone(),
    };
    let style = rux.zs(Style::mono(10.0));
    let colour = egui::Color32::WHITE; // any ground: the words sit on the dark pill drawn over a picture
    let galley = rux.text(style, &words.to_uppercase(), colour);
    let pill = Rect::from_min_size(
        Pos2::new(top.left() + 12.0 * scale, top.top() + 12.0 * scale),
        Vec2::new(galley.size().x + 18.0 * scale, 22.0 * scale),
    );
    let ground = match skill.update {
        true => rux.theme().chat.sky.gamma_multiply(0.85),
        false => egui::Color32::from_black_alpha(150), // any ground: a pill over a picture, whatever the theme
    };
    rux.painter().rect_filled(pill, pill.height() / 2.0, ground);
    rux::text::draw_centred(rux.painter(), pill, galley, colour);
}

/// Two lines of what a skill does, ending with `…` when it is longer.
fn summary(rux: &mut rux::Rux<'_>, inner: Rect, words: &str, scale: f32) {
    let ink = rux.theme().ink.i700;
    let style = rux.zs(Style::sans(12.0).leading(1.4));
    let mut job = egui::text::LayoutJob::single_section(
        words.to_owned(),
        egui::TextFormat::simple(style.font(), ink),
    );
    job.wrap = egui::text::TextWrapping {
        max_width: inner.width().max(1.0),
        max_rows: 3,
        break_anywhere: false,
        overflow_character: Some('\u{2026}'),
    };
    let galley = rux.painter().layout_job(job);
    rux.painter().galley(Pos2::new(inner.left(), inner.top() + 42.0 * scale), galley, ink);
}
