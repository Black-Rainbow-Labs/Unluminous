//! The Designs pane: a header with `New Design`, a search, a count with a switch, and one row for each design.
//!
//! A row is a small picture of a page, the design's name, how many notes are open and resolved on it, and a
//! badge with the open count or a green `Done` when every note is resolved. The design that is showing is
//! raised out of the list. Pressing one opens it in its tab. A folder with no designs says so in one sentence
//! and offers `New Design`. The foot has the designs folder and `Reveal`.

use std::path::PathBuf;

use egui::{Pos2, Rect, Sense, Vec2};
use rux::icon::Icon;
use rux::{Fill, Rux, Style};

use super::{kit, Act};
use crate::services::designer::{DesignSummary, Designer};
use crate::services::plugin_ui::{Look, Request};

/// The room round the pane, and between its parts.
const MARGIN: f32 = 14.0;
const GAP: f32 = 10.0;
/// How tall the header is, the search well, the line of counts and the foot.
const HEADER: f32 = 44.0;
const SEARCH: f32 = 40.0;
const COUNTS: f32 = 34.0;
const FOOT: f32 = 40.0;
/// How tall one design's row is.
const ROW: f32 = 64.0;

/// Which designs the list shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shown {
    All,
    WithNotes,
}

/// Draw the pane and apply what was pressed.
///
/// @param designer - the provider
/// @param ui - where to draw
/// @param look - the window's look, zoomed by the pane's own zoom
pub fn show(designer: &mut Designer, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    let area = ui.available_rect_before_wrap();
    let scale = look.scale();
    let mut acts = Vec::new();
    if area.width() > 60.0 && area.height() > 60.0 {
        let inner = area.shrink(MARGIN * scale);
        let id = egui::Id::new("designer-pane-filter");
        let which = egui::Id::new("designer-pane-shown");
        let mut filter = ui.ctx().data(|data| data.get_temp::<String>(id)).unwrap_or_default();
        let mut with_notes = ui.ctx().data(|data| data.get_temp::<bool>(which)).unwrap_or(false);
        let header = Rect::from_min_size(inner.min, Vec2::new(inner.width(), HEADER * scale));
        let search = Rect::from_min_size(
            Pos2::new(inner.left(), header.bottom() + GAP * scale),
            Vec2::new(inner.width(), SEARCH * scale),
        );
        let counts = Rect::from_min_size(
            Pos2::new(inner.left(), search.bottom() + GAP * scale),
            Vec2::new(inner.width(), COUNTS * scale),
        );
        let foot =
            Rect::from_min_max(Pos2::new(inner.left(), inner.bottom() - FOOT * scale), inner.max);
        let list = Rect::from_min_max(
            Pos2::new(inner.left(), counts.bottom() + 4.0 * scale),
            Pos2::new(inner.right(), foot.top() - GAP * scale),
        );
        kit::layer(ui, scale, "designer-pane-header", inner, |rux| {
            the_header(rux, header, scale, &mut acts);
            kit::search_field(rux, search, &mut filter, "Search designs", "designer-search");
            the_counts(designer, rux, counts, &mut with_notes, scale);
            the_foot(designer, rux, foot, scale, &mut acts);
        });
        ui.ctx().data_mut(|data| {
            data.insert_temp(id, filter.clone());
            data.insert_temp(which, with_notes);
        });
        let shown = if with_notes { Shown::WithNotes } else { Shown::All };
        acts.extend(the_list(designer, ui, list, &filter, shown, scale));
    }
    super::apply(designer, acts)
}

/// `Designs` at the left and the `New Design` button at the right.
fn the_header(rux: &mut Rux<'_>, rect: Rect, scale: f32, acts: &mut Vec<Act>) {
    kit::heading(
        rux,
        Pos2::new(rect.left() + 4.0 * scale, rect.center().y),
        "Designs",
        18.0,
        rect.width() * 0.4,
    );
    let width = kit::button_width(rux, "New Design", true);
    let button = Rect::from_center_size(
        Pos2::new(rect.right() - width / 2.0, rect.center().y),
        Vec2::new(width, 40.0 * scale),
    );
    if kit::button(rux, button, "New Design", Some(Icon::Plus), kit::Look::Primary) {
        acts.push(Act::NewDesign);
    }
}

/// `8 designs · 11 open notes` and the switch between every design and the ones with notes.
fn the_counts(
    designer: &Designer,
    rux: &mut Rux<'_>,
    rect: Rect,
    with_notes: &mut bool,
    scale: f32,
) {
    let designs = designer.designs();
    let notes: usize = designs.iter().map(|design| design.open).sum();
    let words = format!("{} designs \u{00B7} {} open notes", designs.len(), notes);
    let switch = Rect::from_min_size(
        Pos2::new(rect.right() - 168.0 * scale, rect.top()),
        Vec2::new(168.0 * scale, rect.height()),
    );
    kit::quiet(
        rux,
        Pos2::new(rect.left() + 4.0 * scale, rect.center().y),
        &words,
        11.5,
        switch.left() - rect.left() - 8.0 * scale,
    );
    let choices = ["All".to_owned(), "With notes".to_owned()];
    if let Some(picked) = kit::pill_switch(
        rux,
        switch,
        &choices,
        Some(usize::from(*with_notes)),
        "Designs shown",
        true,
    ) {
        *with_notes = picked == 1;
    }
}

/// The designs folder as a carved chip, and `Reveal`.
fn the_foot(designer: &Designer, rux: &mut Rux<'_>, rect: Rect, scale: f32, acts: &mut Vec<Act>) {
    let folder = designer.config.folder.display().to_string();
    let reveal_width = kit::button_width(rux, "Reveal", false);
    let chip_width = (rect.width() - reveal_width - 10.0 * scale).min(150.0 * scale);
    let chip = Rect::from_min_size(rect.min, Vec2::new(chip_width.max(0.0), rect.height()));
    kit::well_surface(rux, chip, chip.height() / 2.0);
    let ink = rux.theme().ink.i500;
    rux.mark(
        rux::icon::Mark::new(Icon::Folder, 14.0 * scale),
        Pos2::new(chip.left() + 18.0 * scale, chip.center().y),
        ink,
    );
    kit::mono(
        rux,
        Pos2::new(chip.left() + 34.0 * scale, chip.center().y),
        &folder,
        11.5,
        chip.width() - 46.0 * scale,
    );
    let button = Rect::from_min_size(
        Pos2::new(rect.right() - reveal_width, rect.top()),
        Vec2::new(reveal_width, rect.height()),
    );
    if kit::button(rux, button, "Reveal", None, kit::Look::Chip) {
        acts.push(Act::Reveal(designer.config.folder.clone()));
    }
}

/// The designs that match what was typed, in a carved well that scrolls.
fn the_list(
    designer: &Designer,
    ui: &mut egui::Ui,
    list: Rect,
    filter: &str,
    shown: Shown,
    scale: f32,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let wanted = filter.trim().to_lowercase();
    let showing = designer.showing_design().map(PathBuf::from);
    let designs: Vec<&DesignSummary> = designer
        .designs()
        .iter()
        .filter(|design| wanted.is_empty() || design.name.to_lowercase().contains(&wanted))
        .filter(|design| shown == Shown::All || design.open + design.resolved > 0)
        .collect();
    if designs.is_empty() {
        acts.extend(nothing_here(designer, ui, list, !wanted.is_empty(), scale));
        return acts;
    }
    kit::well(ui, scale, list, 24.0 * scale);
    let inner = list.shrink(8.0 * scale);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical().id_salt("designer-pane-list").auto_shrink([false, false]).show(
        &mut child,
        |ui| {
            let top = ui.cursor().min;
            let width = inner.width() - 6.0 * scale;
            for (index, design) in designs.iter().enumerate() {
                let rect = Rect::from_min_size(
                    Pos2::new(inner.left(), top.y + index as f32 * (ROW + 4.0) * scale),
                    Vec2::new(width, ROW * scale),
                );
                let on = showing.as_deref() == Some(design.path.as_path());
                if one_design(ui, rect, design, on, scale) {
                    acts.push(Act::OpenDesign(design.path.clone()));
                }
            }
            ui.allocate_space(Vec2::new(inner.width(), designs.len() as f32 * (ROW + 4.0) * scale));
        },
    );
    acts
}

/// One design's row: a thumbnail, the name, how many notes, and a badge or `Done`. Answers whether it was pressed.
fn one_design(ui: &mut egui::Ui, rect: Rect, design: &DesignSummary, on: bool, scale: f32) -> bool {
    let response =
        ui.interact(rect, ui.id().with(("designer-design", &design.name)), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let name = design.name.clone();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, name.clone())
    });
    let (folder, file) = match design.name.rsplit_once('/') {
        Some((folder, file)) => (Some(folder), file),
        None => (None, design.name.as_str()),
    };
    kit::layer(ui, scale, ("designer-design-chip", &design.name), rect, |rux| {
        let chat = rux.theme().chat;
        let ink = rux.theme().ink;
        if on {
            rux.chrome.surface(
                rect,
                20.0 * scale,
                Fill::gradient(chat.chip, rect),
                chat.raised_chip(),
            );
        } else if response.hovered() {
            rux.wash(rect, 20.0 * scale, ink.i900.gamma_multiply(0.05));
        }
        let thumb = Rect::from_min_size(
            Pos2::new(rect.left() + 14.0 * scale, rect.center().y - 17.0 * scale),
            Vec2::new(46.0 * scale, 34.0 * scale),
        );
        rux.chrome.surface(thumb, 8.0 * scale, chat.well, chat.carved_sm());
        for (row, width) in [(9.0, 28.0), (17.0, 34.0), (25.0, 22.0)] {
            let line = Rect::from_min_size(
                Pos2::new(thumb.left() + 6.0 * scale, thumb.top() + (row - 1.5) * scale),
                Vec2::new(width * scale, 3.0 * scale),
            );
            let strength = if row == 9.0 { 0.9 } else { 0.5 };
            rux.painter().rect_filled(line, 1.5 * scale, ink.i500.gamma_multiply(strength));
        }
        let left = thumb.right() + 14.0 * scale;
        let badge_room = if design.open > 0 {
            40.0
        } else if design.resolved > 0 {
            64.0
        } else {
            0.0
        };
        let room = rect.right() - left - (badge_room + 12.0) * scale;
        let style = Style::sans(13.0 * scale).semibold();
        let words = rux.elided(style, file, ink.i900, room);
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(left, rect.center().y - 9.0 * scale),
            words,
            style,
            ink.i900,
        );
        let subtitle = match (design.open, design.resolved) {
            (0, 0) => folder.map_or("No notes yet".to_owned(), str::to_owned),
            (open, 0) => format!("{open} open"),
            (0, resolved) => format!("{resolved} resolved"),
            (open, resolved) => format!("{open} open \u{00B7} {resolved} resolved"),
        };
        kit::quiet(rux, Pos2::new(left, rect.center().y + 10.0 * scale), &subtitle, 11.0, room);
        the_badge(rux, rect, design, scale);
    });
    response.clicked()
}

/// The count of open notes in a carved disc, or a green `Done` when every note is resolved.
fn the_badge(rux: &mut Rux<'_>, rect: Rect, design: &DesignSummary, scale: f32) {
    let chat = rux.theme().chat;
    if design.open > 0 {
        let badge = Rect::from_center_size(
            Pos2::new(rect.right() - 24.0 * scale, rect.center().y),
            Vec2::splat(24.0 * scale),
        );
        rux.chrome.surface(badge, badge.height() / 2.0, chat.well, chat.carved_sm());
        let galley =
            rux.text(Style::sans(11.0 * scale).semibold(), &design.open.to_string(), chat.sky);
        rux::text::draw_centred(rux.painter(), badge, galley, chat.sky);
    } else if design.resolved > 0 {
        let at = Pos2::new(rect.right() - 58.0 * scale, rect.center().y);
        rux.mark(rux::icon::Mark::new(Icon::Tick, 12.0 * scale), at, chat.faster);
        let style = rux.zs(Style::sans(11.5));
        let galley = rux.elided(style, "Done", chat.faster, 40.0 * scale);
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(at.x + 12.0 * scale, at.y),
            galley,
            style,
            chat.faster,
        );
    }
}

/// What the list says when there is nothing to list.
fn nothing_here(
    designer: &Designer,
    ui: &mut egui::Ui,
    list: Rect,
    searched: bool,
    scale: f32,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let words = match (searched, designer.project().is_some()) {
        (true, _) => "No design has that name.".to_owned(),
        (false, false) => "Open a project to see its designs.".to_owned(),
        (false, true) => format!(
            "There are no designs in {} yet. A design is an HTML file, and the plus button makes one from the starter page.",
            designer.config.folder.display()
        ),
    };
    kit::layer(ui, scale, "designer-pane-empty", list, |rux| {
        let bottom = kit::paragraph(
            rux,
            Pos2::new(list.left() + 4.0 * scale, list.top() + 8.0 * scale),
            list.width() - 8.0 * scale,
            &words,
            12.0,
        );
        if !searched && designer.project().is_some() {
            let pill = rux::components::PillButton::new("Make a design").primary().mark(Icon::Plus);
            let size = pill.measure(rux);
            let rect = Rect::from_min_size(
                Pos2::new(list.left() + 4.0 * scale, bottom + 14.0 * scale),
                size,
            );
            if pill.show(rux, rect).clicked() {
                acts.push(Act::NewDesign);
            }
        }
    });
    acts
}
