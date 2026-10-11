//! The Database page in the Settings window.
//!
//! Two cards from `components::settings_kit` in the Settings design's layout (`task-2246`): the data sources,
//! each in a carved well with its own `Edit` and `Remove`, and the page size of a grid.
//!
//! **No password is drawn and no password is written.** A row says where its password is: `environment
//! UNLUMINOUS_DB_AI`, `keychain …`, `typed, until this window closes`, or `none`, and never what it is, because a
//! page that showed one is a page somebody screenshots.

use egui::{Pos2, Rect, Vec2};
use rux::{Rux, Style};

use crate::components::database::Act;
use crate::components::settings_kit::{self as kit, Column, Kind as ButtonKind};
use crate::services::database::DatabaseExplorer;
use crate::services::plugin_ui::{Look, Request};

/// How tall one data source's well is, and the gap under it.
const SOURCE: f32 = 58.0;
const SOURCE_GAP: f32 = 10.0;

/// Draw the page inside the rectangle every page gets.
pub fn show(explorer: &mut DatabaseExplorer, ui: &mut egui::Ui, _look: &Look<'_>) -> Vec<Request> {
    kit::plugin_page(ui, "database-settings", |ui, column| cards(explorer, ui, column))
}

/// The page's two cards.
fn cards(explorer: &mut DatabaseExplorer, ui: &mut egui::Ui, column: &mut Column) -> Vec<Request> {
    let mut requests = Vec::new();
    let mut acts = Vec::new();
    let mut removing: Option<String> = None;
    let sources: Vec<unluminous_db::Source> = explorer.sources().to_vec();

    kit::card(
        ui,
        column,
        "Data sources",
        Some(
            "Where each one points, and where its password is kept. Unluminous never writes a password into a \
             file: it goes into this machine's own credential store (the keychain on macOS, the Secret Service \
             on Linux, Credential Manager on Windows), and what is written down is the name of the entry.",
        ),
        |card| {
            card.grow(|rux, column| {
                let mut pen = column.pen;
                if sources.is_empty() {
                    let well = Rect::from_min_size(Pos2::new(column.left, pen), Vec2::new(column.width(), 64.0));
                    kit::list_well(rux, well, 18.0);
                    let theme = rux.theme();
                    let galley = rux.text(Style::sans(12.0), "There are none yet.", theme.ink.i400);
                    rux::text::draw_centred(rux.painter(), well, galley, theme.ink.i400);
                    pen = well.bottom() + 12.0;
                }
                for (index, source) in sources.iter().enumerate() {
                    // A child `Ui` a source, so the `Edit` and `Remove` of one row are not the same widgets as the
                    // next row's.
                    let mut child =
                        rux.ui.new_child(egui::UiBuilder::new().id_salt(("database-source", index)));
                    let mut inner = Rux { ui: &mut child, state: rux.state, chrome: rux.chrome };
                    let row = Rect::from_min_size(Pos2::new(column.left, pen), Vec2::new(column.width(), SOURCE));
                    match one_source(explorer, &mut inner, row, source) {
                        Some(SourceAct::Edit) => acts.push(Act::EditSource(source.name.clone())),
                        Some(SourceAct::Remove) => removing = Some(source.name.clone()),
                        None => {}
                    }
                    pen = row.bottom() + SOURCE_GAP;
                }
                if !sources.is_empty() {
                    pen += 2.0;
                }
                let (pressed, rect) = kit::add_button(rux, Pos2::new(column.left, pen), "New data source");
                if pressed {
                    acts.push(Act::NewSource);
                }
                ((), rect.bottom())
            });
        },
    );

    kit::card(ui, column, "Rows", None, |card| {
        let mut said = explorer.configuration.page_size.to_string();
        card.row(
            "Page size",
            Some(
                "One more than this is asked for and thrown away, which is what makes 1-200 of 200+ honest: nobody \
                 counted the rest.",
            ),
            |row| {
                if row.text("Rows a page", &mut said, "", 96.0) {
                    if let Ok(size) = said.parse::<usize>() {
                        explorer.configuration.page_size = size.clamp(1, 100_000);
                        let _ = explorer.write_the_configuration();
                    }
                }
                row.suffix("rows a page, in a grid and in a console result");
            },
        );
    });

    if let Some(name) = removing {
        if let Err(why) = explorer.remove_source(&name) {
            requests.push(Request::Message(why));
        }
    }
    requests.extend(crate::components::database::apply(explorer, acts));
    requests
}

/// What one data source's row asked for.
enum SourceAct {
    Edit,
    Remove,
}

/// One data source, in a carved well: its name and buttons on one line, where it points on the next.
///
/// Two lines rather than one, because a SQLite file's path is long and a single line ran it underneath the
/// buttons. Everything is cut to the room there is.
fn one_source(
    explorer: &DatabaseExplorer,
    rux: &mut Rux<'_>,
    row: Rect,
    source: &unluminous_db::Source,
) -> Option<SourceAct> {
    let theme = rux.theme();
    kit::list_well(rux, row, 18.0);
    let mut act = None;
    let remove_width = kit::button_width(rux, "Remove", false);
    let remove = Rect::from_min_size(
        Pos2::new(row.right() - 14.0 - remove_width, row.center().y - 15.0),
        Vec2::new(remove_width, 30.0),
    );
    if kit::button(rux, remove, "Remove", ButtonKind::Ghost) {
        act = Some(SourceAct::Remove);
    }
    let edit_width = kit::button_width(rux, "Edit", false);
    let edit = Rect::from_min_size(
        Pos2::new(remove.left() - 8.0 - edit_width, remove.top()),
        Vec2::new(edit_width, 30.0),
    );
    if kit::button(rux, edit, "Edit", ButtonKind::Chip) {
        act = Some(SourceAct::Edit);
    }
    let room = edit.left() - row.left() - 32.0;
    let name = rux.elided(Style::sans(12.5).semibold(), &source.name, theme.ink.i900, room);
    rux.painter().galley(Pos2::new(row.left() + 16.0, row.top() + 11.0), name, theme.ink.i900);
    let said = format!(
        "{} \u{00B7} {} \u{00B7} password {}",
        crate::services::paths::where_a_source_points(source),
        match (explorer.is_connected(&source.name), source.read_only) {
            (true, true) => "connected, read only",
            (true, false) => "connected",
            (false, true) => "read only",
            (false, false) => "not connected",
        },
        source.secret.describe(),
    );
    let under = rux.elided(Style::sans(11.5), &said, theme.ink.i400, room);
    rux.painter().galley(Pos2::new(row.left() + 16.0, row.top() + 31.0), under, theme.ink.i400);
    act
}
