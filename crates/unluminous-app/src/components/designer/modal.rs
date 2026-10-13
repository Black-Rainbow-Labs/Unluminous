//! The Designer's two dialogs: naming a new design, and choosing a copy to put back.
//!
//! Both are `components::modal::show_unframed`, so they have the dragging, the resizing and the zoom every
//! dialog has, and both are drawn with the parts of `components::realm::dialog`, which are the New Data
//! Source dialog's. The history dialog lists the copies newest first, each a chip with when it was taken and
//! the words of the request it was taken for, and `Restore` puts the chosen one back.

use std::path::{Path, PathBuf};

use egui::{Pos2, Rect, Vec2};
use rux::icon::Icon;
use rux::Style;

use super::{kit, Act};
use crate::components::modal;
use crate::components::realm::dialog;
use crate::services::designer::history::Backup;
use crate::services::designer::{Designer, Modal};
use crate::services::plugin_ui::{Look, Request};

const NEW_ID: &str = "unluminous-designer-new";
const HISTORY_ID: &str = "unluminous-designer-history";
const NEW_WIDTH: f32 = 440.0;
const HISTORY_WIDTH: f32 = 520.0;
const ROW: f32 = 62.0;
const HEAD: f32 = 40.0;
const INTRO: f32 = 56.0;
const NOW: f32 = 62.0;
const FOOT: f32 = 50.0;
const HISTORY_ROWS: f32 = 7.0;

/// Draw whichever dialog is open. Answers what it asked for and whether it closed.
///
/// @param designer - the provider
/// @param ctx - the window's context
pub fn show(
    designer: &mut Designer,
    ctx: &egui::Context,
    _look: &Look<'_>,
) -> (Vec<Request>, bool) {
    let Some(open) = designer.modal.clone() else { return (Vec::new(), false) };
    let acts = match open {
        Modal::NewDesign { name } => new_design(designer, ctx, name),
        Modal::History { design } => history(designer, ctx, &design),
    };
    let closed = acts.contains(&Act::CloseModal);
    (super::apply(designer, acts), closed)
}

/// The room inside the dialog's padding.
fn inner_of(area: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(area.left() + dialog::PAD_X, area.top() + dialog::PAD_Y),
        Pos2::new(area.right() - dialog::PAD_X, area.bottom() - dialog::PAD_Y),
    )
}

/// Naming a new design.
fn new_design(designer: &mut Designer, ctx: &egui::Context, mut name: String) -> Vec<Act> {
    let mut acts = Vec::new();
    let height = dialog::PAD_Y * 2.0
        + dialog::CLOSE
        + dialog::GAP
        + dialog::SEARCH
        + dialog::GAP
        + dialog::BUTTON;
    let (_, escaped) = modal::show_unframed(ctx, NEW_ID, NEW_WIDTH, height, |ui, area| {
        dialog::surface(ui, "designer-new", area);
        let inner = inner_of(area);
        crate::components::settings_kit::layer(ui, "designer-new", inner, |rux| {
            if dialog::heading(rux, inner, "New Design") {
                acts.push(Act::CloseModal);
            }
            let well = Rect::from_min_size(
                Pos2::new(inner.left(), inner.top() + dialog::CLOSE + dialog::GAP),
                Vec2::new(inner.width(), dialog::SEARCH),
            );
            let response =
                dialog::search(rux, well, "Design name", "Design name, such as landing", &mut name);
            if !response.has_focus() && rux.ui.memory(|memory| memory.focused().is_none()) {
                response.request_focus();
            }
            let ready = !name.trim().is_empty();
            match dialog::footer(rux, inner, &[("Cancel", false)], ("Create", ready)) {
                Some(0) => acts.push(Act::CloseModal),
                Some(_) if ready => acts.push(Act::CreateDesign(name.clone())),
                _ => {}
            }
        });
        if modal::Confirm::Enter.pressed(ui) && !name.trim().is_empty() {
            acts.push(Act::CreateDesign(name.clone()));
        }
    });
    if escaped {
        acts.push(Act::CloseModal);
    }
    designer.modal = Some(Modal::NewDesign { name });
    acts
}

/// The copies of a design: a line about what they are, the file as it is now, and each copy with `Restore`.
fn history(designer: &mut Designer, ctx: &egui::Context, design: &Path) -> Vec<Act> {
    let mut acts = Vec::new();
    let copies = designer.copies(design);
    let name = designer.name_of(design);
    let shown = (copies.len() as f32).clamp(1.0, HISTORY_ROWS);
    let height = dialog::PAD_Y * 2.0 + HEAD + INTRO + NOW + ROW * shown + FOOT;
    let owner: PathBuf = design.to_path_buf();
    let folder = designer
        .project()
        .map(|project| {
            crate::services::designer::history::folder_for(project, &designer.config.folder, design)
        })
        .unwrap_or_default();
    let (_, escaped) = modal::show_unframed(ctx, HISTORY_ID, HISTORY_WIDTH, height, |ui, area| {
        dialog::surface(ui, "designer-history", area);
        let inner = inner_of(area);
        crate::components::settings_kit::layer(ui, "designer-history", inner, |rux| {
            let title = format!("History \u{00B7} {name}");
            if dialog::heading(rux, inner, &title) {
                acts.push(Act::CloseModal);
            }
            the_intro(rux, inner, copies.len());
            let list = Rect::from_min_max(
                Pos2::new(inner.left(), inner.top() + HEAD + INTRO),
                Pos2::new(inner.right(), inner.bottom() - FOOT),
            );
            the_rows(rux, list, &copies, &owner, &mut acts);
            let foot = Rect::from_min_max(
                Pos2::new(inner.left(), inner.bottom() - dialog::BUTTON),
                inner.max,
            );
            let width = kit::button_width(rux, "Show copies in the folder", true);
            let show = Rect::from_min_size(
                Pos2::new(foot.left(), foot.top() + 4.0),
                Vec2::new(width, 30.0),
            );
            if kit::button(
                rux,
                show,
                "Show copies in the folder",
                Some(Icon::Folder),
                kit::Look::Ghost,
            ) {
                acts.push(Act::Reveal(folder.clone()));
            }
            if dialog::footer(rux, inner, &[], ("Done", true)).is_some() {
                acts.push(Act::CloseModal);
            }
        });
    });
    if escaped {
        acts.push(Act::CloseModal);
    }
    acts
}

/// What a copy is, and how many are kept, under the title.
fn the_intro(rux: &mut rux::Rux<'_>, inner: Rect, copies: usize) {
    let theme = rux.theme();
    let kept = format!("{copies} copies kept");
    let style = rux.zs(Style::sans(11.5));
    let galley = rux.text(style, &kept, theme.ink.i400);
    let width = galley.size().x;
    let at =
        Pos2::new(inner.right() - dialog::CLOSE - 12.0 - width, inner.top() + dialog::CLOSE / 2.0);
    rux::text::draw_left_capitals(rux.painter(), at, galley, style, theme.ink.i400);
    let words = "A copy is taken before each request is sent. Restoring keeps the current file as a copy first.";
    kit::paragraph(
        rux,
        Pos2::new(inner.left(), inner.top() + dialog::CLOSE + 12.0),
        inner.width(),
        words,
        12.0,
    );
    let line = inner.top() + HEAD + INTRO - 8.0;
    rux.painter().hline(
        inner.x_range(),
        line,
        egui::Stroke::new(1.0, theme.ink.i500.gamma_multiply(0.25)),
    );
}

/// The current file, then one row a copy: when it was taken, the words of the request, and `Restore`.
fn the_rows(
    rux: &mut rux::Rux<'_>,
    list: Rect,
    copies: &[Backup],
    design: &Path,
    acts: &mut Vec<Act>,
) {
    let theme = rux.theme();
    let now_row = Rect::from_min_size(list.min, Vec2::new(list.width(), NOW - 6.0));
    kit::quiet(rux, Pos2::new(now_row.left() + 4.0, now_row.top() + 12.0), "Now", 11.5, 100.0);
    let current = rux.elided(Style::sans(13.0), "Current file", theme.ink.i900, 300.0);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(now_row.left() + 4.0, now_row.top() + 32.0),
        current,
        Style::sans(13.0),
        theme.ink.i900,
    );
    let pill = Rect::from_center_size(
        Pos2::new(now_row.right() - 46.0, now_row.center().y),
        Vec2::new(86.0, 26.0),
    );
    kit::well_surface(rux, pill, 13.0);
    rux.mark(
        rux::icon::Mark::new(Icon::Tick, 12.0),
        Pos2::new(pill.left() + 14.0, pill.center().y),
        theme.chat.faster,
    );
    let showing = rux.elided(Style::sans(11.5).medium(), "Showing", theme.chat.faster, 60.0);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(pill.left() + 26.0, pill.center().y),
        showing,
        Style::sans(11.5).medium(),
        theme.chat.faster,
    );
    if copies.is_empty() {
        let words = "No copies yet. One is taken before each request to the agent.";
        kit::quiet(
            rux,
            Pos2::new(list.left() + 4.0, list.top() + NOW + 16.0),
            words,
            12.0,
            list.width(),
        );
        return;
    }
    let today = crate::services::designer::history::local_now();
    for (index, copy) in copies.iter().take(HISTORY_ROWS as usize).enumerate() {
        let row = Rect::from_min_size(
            Pos2::new(list.left(), list.top() + NOW + index as f32 * ROW),
            Vec2::new(list.width(), ROW - 6.0),
        );
        dialog::row(rux, row, &copy.name, false);
        let left = row.left() + 4.0;
        let room = row.width() - 120.0;
        kit::quiet(rux, Pos2::new(left, row.top() + 14.0), &copy.friendly(today), 11.5, room);
        let words = capitalised(&copy.words);
        let line = rux.elided(Style::sans(13.0), &words, theme.ink.i900, room);
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(left, row.top() + 34.0),
            line,
            Style::sans(13.0),
            theme.ink.i900,
        );
        let restore = Rect::from_center_size(
            Pos2::new(row.right() - 46.0, row.center().y),
            Vec2::new(86.0, 32.0),
        );
        if kit::button(rux, restore, "Restore", None, kit::Look::Chip) {
            acts.push(Act::Restore(design.to_path_buf(), copy.name.clone()));
        }
    }
}

/// The words with a capital letter first, which is how a request reads as a title.
fn capitalised(words: &str) -> String {
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}
