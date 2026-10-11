//! The plugin's modals. The New Data Source dialog is drawn in the Settings design's look, because it is
//! opened from the Database page of the Settings window and is drawn over it; the others are built from
//! `components::modal`'s own furniture. Every one keeps the dragging, the resizing and the zoom that
//! `modal::show` and `modal::show_unframed` give every dialog without asking.
//!
//! - **New Data Source** — the reference editor's General tab, cut to what applies: name, engine, host, port,
//!   database, user, where the password is, ssl mode, and Test Connection reporting the server's own
//!   version string. `task-2246` drew it as the design's `DataSourceDialog.dc.html`: one raised card on
//!   the dialog's ground, carved wells, the engine and the connection security as carved tracks.
//! - **Preview pending changes** — the actual statements Submit will send, from the same call Submit
//!   makes, so the preview cannot drift from what happens.
//! - **DDL** — a `CREATE` statement.
//! - **Confirm** — a statement from a console that changes rows.

use egui::{Pos2, Rect, Vec2};

use unluminous_db::source::{Engine, Secret, SslMode};

use rux::components::ChipButton;
use rux::icon::Icon;
use rux::{Fill, Style};

use crate::components::modal;
use crate::components::settings_kit::{self as kit, Kind as ButtonKind};
use crate::services::database::{ColumnForm, DatabaseExplorer, Modal, SourceForm, TableForm};
use crate::services::plugin_ui::{Look, Request};
use crate::theme::color;

/// How wide the label column is, so every field on the dialog starts at the same x.
///
/// A label column rather than a name inside each field, which is the reference editor's own arrangement and the
/// one thing the first version of this dialog got wrong: five unlabelled boxes told a person nothing
/// about which was the database and which was the user.
const LABEL: f32 = 112.0;
/// A field's own height, and the gap under a row.
const FIELD: f32 = 24.0;
const GAP: f32 = 6.0;

/// A label at the left, and the rectangle the field goes in, which starts past it.
fn labelled(ui: &egui::Ui, body: Rect, pen: f32, name: &str) -> Rect {
    let row = Rect::from_min_size(Pos2::new(body.left(), pen), Vec2::new(body.width(), FIELD));
    crate::components::modal::label(ui.painter(), row, row.left(), name, color::text_dim(), 12.0);
    Rect::from_min_max(Pos2::new(body.left() + LABEL, pen), Pos2::new(body.right(), pen + FIELD))
}

/// Draw whichever modal is open. Answers what it asked for and whether it closed.
pub fn show(
    explorer: &mut DatabaseExplorer,
    ctx: &egui::Context,
    look: &Look<'_>,
) -> (Vec<Request>, bool) {
    let Some(open) = explorer.modal.clone() else { return (Vec::new(), false) };
    match open {
        Modal::Source(form) => source_modal(explorer, ctx, look, form),
        Modal::Preview { page } => preview(explorer, ctx, page),
        Modal::Ddl { title, text } => reading(explorer, ctx, &format!("DDL — {title}"), &text),
        Modal::NewTable(form) => new_table(explorer, ctx, look, form),
        Modal::Vector { title, vector } => vector_modal(explorer, ctx, look, &title, &vector),
    }
}

/// One cell's vector: the figures, every component as a bar, and the numbers.
///
/// **The bars are the point.** Everything else here could be read off the summary in the cell, but
/// nobody can tell whether an embedding is sensible from four decimal places of its first three
/// components - what says so is the shape: whether it is dense or sparse, centred or offset, and
/// whether one component dwarfs the rest. A row of bars answers all three at a glance and costs one
/// line each.
fn vector_modal(
    explorer: &mut DatabaseExplorer,
    ctx: &egui::Context,
    look: &Look<'_>,
    title: &str,
    vector: &unluminous_db::Vector,
) -> (Vec<Request>, bool) {
    let mut requests = Vec::new();
    let mut close = false;
    let mut copy = false;
    let numbers = numbered(vector);
    let (_, escaped) = modal::show(ctx, "unluminous-database-vector", 640.0, 470.0, |ui, area| {
        if modal::header(ui, area, &format!("Vector - {title}")) {
            close = true;
        }
        let body = modal::body(area);
        let scale = look.scale();
        let figures = format!(
            "{} dimensions   |v| {:.4}   min {:.4}   max {:.4}   mean {:.4}",
            vector.dimensions(),
            vector.norm(),
            vector.smallest(),
            vector.largest(),
            vector.mean()
        );
        let line = 18.0 * scale;
        super::text(
            ui.painter(),
            Pos2::new(body.left(), body.top() + line * 0.5),
            &figures,
            color::text(),
            look.font_size * 0.85,
            body.width(),
        );
        // The strip, and then the numbers under it.
        let strip = Rect::from_min_size(
            Pos2::new(body.left(), body.top() + line * 1.4),
            Vec2::new(body.width(), 96.0 * scale),
        );
        draw_the_components(ui.painter(), strip, vector, look);
        let rest =
            Rect::from_min_max(Pos2::new(body.left(), strip.bottom() + 8.0 * scale), body.max);
        modal::monospaced(ui, rest, "database-vector-numbers", &numbers);
        match modal::footer(ui, area, &[("Copy", true), ("Done", true)]) {
            Some(0) => copy = true,
            Some(1) => close = true,
            _ => {}
        }
    });
    if copy {
        // JSON rather than the engine's bytes, because what somebody does next with a copied
        // embedding is paste it into something that reads numbers.
        requests.push(Request::Copy(vector.as_json()));
    }
    if close || escaped {
        explorer.modal = None;
    }
    (requests, close || escaped)
}

/// Every component as a bar, on a zero line.
///
/// **One bar per pixel column when there are more components than pixels**, taking the largest
/// magnitude in each bucket rather than sampling one of them: a sampled strip of a 768-dimension
/// vector hides exactly the outlier somebody is looking for, and drawing 768 bars into 400 pixels
/// draws most of them on top of each other anyway.
fn draw_the_components(
    painter: &egui::Painter,
    area: Rect,
    vector: &unluminous_db::Vector,
    look: &Look<'_>,
) {
    painter.rect_filled(area, egui::CornerRadius::same(4), look.palette.field);
    if vector.values.is_empty() {
        return;
    }
    let middle = area.center().y;
    painter.line_segment(
        [Pos2::new(area.left(), middle), Pos2::new(area.right(), middle)],
        egui::Stroke::new(1.0, color::text_faint().gamma_multiply(0.5)),
    );
    let largest = vector
        .values
        .iter()
        .fold(0.0f32, |so_far, value| so_far.max(value.abs()))
        .max(f32::MIN_POSITIVE);
    let room = (area.width() - 8.0).max(1.0);
    let buckets = (vector.values.len() as f32).min(room.floor()).max(1.0) as usize;
    let per = (vector.values.len() as f32 / buckets as f32).max(1.0);
    let width = room / buckets as f32;
    let height = area.height() * 0.5 - 6.0;
    for bucket in 0..buckets {
        let from = (bucket as f32 * per) as usize;
        let to = (((bucket + 1) as f32 * per) as usize).min(vector.values.len()).max(from + 1);
        let value = vector.values[from..to].iter().fold(0.0f32, |so_far, value| {
            match value.abs() > so_far.abs() {
                true => *value,
                false => so_far,
            }
        });
        let tall = (value / largest) * height;
        let x = area.left() + 4.0 + bucket as f32 * width + width * 0.5;
        painter.line_segment(
            [Pos2::new(x, middle), Pos2::new(x, middle - tall)],
            egui::Stroke::new((width * 0.8).clamp(1.0, 3.0), color::accent()),
        );
    }
}

/// The numbers, indexed, so a person can find the component they are looking at in the strip.
fn numbered(vector: &unluminous_db::Vector) -> String {
    let mut out = String::new();
    for (at, value) in vector.values.iter().enumerate() {
        if at > 0 && at % 6 == 0 {
            out.push('\n');
        }
        out.push_str(&format!("{at:>5}: {value:>11.6}   "));
    }
    out
}

/// What the New Data Source dialog reported.
#[derive(Default)]
struct Outcome {
    close: bool,
    save: bool,
    test: bool,
}

/// The modal's id, which is what its place, its zoom and its measured height are kept against.
const SOURCE_ID: &str = "unluminous-database-source";
/// The dialog's width, padding and corner: `width: 560px; padding: 22px 24px; border-radius: 28px`.
const SOURCE_WIDTH: f32 = 560.0;
const SOURCE_PAD_X: f32 = 24.0;
const SOURCE_PAD_Y: f32 = 22.0;
const SOURCE_RADIUS: f32 = 28.0;
/// The General card's padding and corner, and the gaps the dialog lays its parts out with.
const CARD_PAD_X: f32 = 20.0;
const CARD_PAD_Y: f32 = 18.0;
const CARD_RADIUS: f32 = 22.0;
const SOURCE_GAP: f32 = 18.0;
/// A field's height, the gap between two rows, and the label column: `grid-template-columns: 130px 1fr;
/// gap: 12px 14px`.
const SOURCE_FIELD: f32 = 36.0;
const SOURCE_ROW_GAP: f32 = 12.0;
const SOURCE_LABEL: f32 = 130.0 + 14.0;

/// The New Data Source dialog, in the Settings design's look (`task-2246`, `DataSourceDialog.dc.html`): a
/// heading and a round close button, one raised card holding the General settings with `Test Connection` at
/// its top right, and `Cancel` and `Save` under it.
fn source_modal(
    explorer: &mut DatabaseExplorer,
    ctx: &egui::Context,
    look: &Look<'_>,
    mut form: SourceForm,
) -> (Vec<Request>, bool) {
    let _ = look;
    let mut requests = Vec::new();
    let mut outcome = Outcome::default();
    let heading = match form.was.is_empty() {
        true => "New Data Source",
        false => "Data Source",
    };
    // **The height is what the rows came out at last frame.** The rows are laid out one under another with no
    // scroll, so a budget that was guessed drew the last of them over the buttons. Measured instead, and a
    // first frame for an engine that has not been measured yet asks for the height that engine usually takes.
    let measured = egui::Id::new(SOURCE_ID).with(("height", form.source.engine.title()));
    let height =
        ctx.data(|data| data.get_temp::<f32>(measured)).unwrap_or(match form.source.engine {
            Engine::Postgres => 660.0,
            Engine::Sqlite | Engine::Inillucent => 430.0,
        });
    let testing = explorer.is_testing();
    let (needed, escaped) =
        modal::show_unframed(ctx, SOURCE_ID, SOURCE_WIDTH, height, |ui, area| {
            kit::layer(ui, "database-source-surface", area, |rux| {
                let chat = rux.theme().chat;
                rux.chrome.surface(area, SOURCE_RADIUS, chat.ground, chat.floating());
            });
            let inner = Rect::from_min_max(
                Pos2::new(area.left() + SOURCE_PAD_X, area.top() + SOURCE_PAD_Y),
                Pos2::new(area.right() - SOURCE_PAD_X, area.bottom() - SOURCE_PAD_Y),
            );
            let needed = kit::layer(ui, "database-source", inner, |rux| {
                let theme = rux.theme();
                // The heading and the round close button.
                let close = Rect::from_center_size(
                    Pos2::new(inner.right() - 17.0, inner.top() + 17.0),
                    Vec2::splat(34.0),
                );
                if ChipButton::new(Icon::X, 34.0, "Close")
                    .mark(12.0, Some(1.8))
                    .show(rux, close)
                    .clicked()
                {
                    outcome.close = true;
                }
                let style = Style::sans(18.0).semibold().tracking(-0.015);
                let galley = rux.text(style, heading, theme.ink.i900);
                rux::text::draw_left_capitals(
                    rux.painter(),
                    Pos2::new(inner.left(), close.center().y),
                    galley,
                    style,
                    theme.ink.i900,
                );
                let card_top = close.bottom() + SOURCE_GAP;
                let card_bottom = general(rux, inner, card_top, &mut form, testing, &mut outcome);
                // `Cancel` with no surface, and `Save` on the action's gradient.
                let footer = card_bottom + SOURCE_GAP;
                let save_width = rux.measure(Style::sans(13.0).semibold(), "Save").x + 44.0;
                let save = Rect::from_min_size(
                    Pos2::new(inner.right() - save_width, footer),
                    Vec2::new(save_width, 38.0),
                );
                if primary(rux, save, "Save") {
                    outcome.save = true;
                }
                let cancel_width = kit::button_width(rux, "Cancel", false) + 2.0;
                let cancel = Rect::from_min_size(
                    Pos2::new(save.left() - 8.0 - cancel_width, footer),
                    Vec2::new(cancel_width, 38.0),
                );
                if kit::button(rux, cancel, "Cancel", ButtonKind::Ghost) {
                    outcome.close = true;
                }
                save.bottom() + SOURCE_PAD_Y - area.top()
            });
            // Enter is `Save`, as the last button of every modal's footer is.
            if modal::Confirm::Enter.pressed(ui) {
                outcome.save = true;
            }
            needed
        });
    if (needed - height).abs() > 0.5 {
        ctx.data_mut(|data| data.insert_temp(measured, needed));
        ctx.request_repaint();
    }
    // **Both of these are changes to the world, and neither happens in a draw any more** (`task-1984` S2 and
    // S9). Test Connection opened a connection synchronously, about twenty one seconds of TCP against a host
    // that is down with the window drawing nothing, and the password was written to the credential store from
    // here. The explorer owns both now.
    if outcome.test {
        form.tested = None;
        explorer.test_the_source(&form);
    }
    if outcome.save {
        let mut source = form.source.clone();
        let (secret, said) = explorer.keep_the_password(&form);
        source.secret = secret;
        if let Some(said) = said {
            requests.push(Request::Message(said));
        }
        match explorer.save_source(&form.was, source) {
            Ok(()) => outcome.close = true,
            Err(why) => {
                form.tested = Some(Err(why.clone()));
                requests.push(Request::Message(why));
            }
        }
    }
    let closed = outcome.close || escaped;
    match closed {
        true => explorer.modal = None,
        false => explorer.modal = Some(Modal::Source(form)),
    }
    (requests, closed)
}

/// The button that does the thing: the action's gradient, lifted, with white words.
fn primary(rux: &mut rux::Rux<'_>, rect: Rect, label: &str) -> bool {
    let chat = rux.theme().chat;
    let response = rux.ui.interact(
        rect,
        rux.ui.id().with(("database-source-primary", label)),
        egui::Sense::click(),
    );
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let (gradient, elevation) = match (response.is_pointer_button_down_on(), response.hovered()) {
        (true, _) => (chat.action, chat.action_pressed()),
        (false, true) => (chat.action_hover, chat.action_raised()),
        (false, false) => (chat.action, chat.action_raised()),
    };
    rux.chrome.surface(rect, rect.height() / 2.0, Fill::gradient(gradient, rect), elevation);
    let ink = color::on_accent();
    let galley = rux.text(Style::sans(13.0).semibold(), label, ink);
    rux::text::draw_centred(rux.painter(), rect, galley, ink);
    let named = label.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone()));
    response.clicked()
}

/// The General card: what the last test said and `Test Connection`, then the fields. Answers where it ended.
fn general(
    rux: &mut rux::Rux<'_>,
    inner: Rect,
    top: f32,
    form: &mut SourceForm,
    testing: bool,
    outcome: &mut Outcome,
) -> f32 {
    let theme = rux.theme();
    let chat = theme.chat;
    // The card's surface first, at the height its rows came out at, which is only known once they have been
    // drawn: the rows go into a layer of their own, reserved after this one, so the card is under them.
    let left = inner.left() + CARD_PAD_X;
    let right = inner.right() - CARD_PAD_X;
    let state = rux.state;
    crate::theme::in_step(state);
    let rows_area =
        Rect::from_min_max(Pos2::new(inner.left(), top), Pos2::new(inner.right(), top + 2_000.0));
    let id = rux.ui.id().with("database-source-rows");
    let bottom = rux::layer(&mut *rux.ui, state, id, rows_area.expand(kit::CARD_REACH), |rows| {
        let theme = rows.theme();
        let mut pen = top + CARD_PAD_Y;
        // `General`, what the last test said, and the button that asks.
        let head = Rect::from_min_max(Pos2::new(left, pen), Pos2::new(right, pen + 32.0));
        let test_width = kit::button_width(rows, "Test Connection", false);
        let test = Rect::from_min_size(
            Pos2::new(right - test_width, head.top()),
            Vec2::new(test_width, 32.0),
        );
        if kit::button(rows, test, "Test Connection", ButtonKind::Chip) {
            outcome.test = true;
        }
        let galley = rows.text(Style::sans(13.0).semibold(), "General", theme.ink.i900);
        rux::text::draw_left_capitals(
            rows.painter(),
            Pos2::new(left, head.center().y),
            galley,
            Style::sans(13.0).semibold(),
            theme.ink.i900,
        );
        // What the server called itself, or why it would not answer, beside the button that asked. While the
        // question is still out it says so, because the connection is on a thread and an answer that simply
        // appears seconds later reads as a button that did nothing (`task-1984` S2).
        let (said, tint) = match (&form.tested, testing) {
            (_, true) => ("Testing\u{2026}".to_owned(), theme.ink.i400),
            (Some(Ok(version)), _) => (version.clone(), theme.chat.faster),
            (Some(Err(why)), _) => (why.clone(), theme.chat.slower),
            (None, false) => ("Not tested yet".to_owned(), theme.ink.i400),
        };
        let room = test.left() - 12.0 - (left + 80.0);
        let galley = rows.elided(Style::sans(11.5), &said, tint, room);
        let width = galley.size().x;
        rux::text::draw_left_capitals(
            rows.painter(),
            Pos2::new(test.left() - 12.0 - width, head.center().y),
            galley,
            Style::sans(11.5),
            tint,
        );
        pen = head.bottom() + 14.0;
        pen = fields(rows, left, right, pen, form);
        pen + CARD_PAD_Y
    });
    let card = Rect::from_min_max(Pos2::new(inner.left(), top), Pos2::new(inner.right(), bottom));
    rux.chrome.surface(card, CARD_RADIUS, Fill::gradient(chat.card, card), chat.raised());
    bottom
}

/// One labelled row of the General card: the label at the left, centred on the first line. Answers where the
/// controls start.
fn label(rux: &mut rux::Rux<'_>, left: f32, pen: f32, name: &str) -> f32 {
    let theme = rux.theme();
    let galley = rux.text(Style::sans(12.5), name, theme.ink.i500);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(left, pen + SOURCE_FIELD / 2.0),
        galley,
        Style::sans(12.5),
        theme.ink.i500,
    );
    left + SOURCE_LABEL
}

/// A line of help under the controls, across the control column. Answers where it ended.
fn help(rux: &mut rux::Rux<'_>, left: f32, right: f32, pen: f32, said: &str) -> f32 {
    kit::paragraph(
        rux,
        Pos2::new(left + SOURCE_LABEL, pen + 8.0),
        right - left - SOURCE_LABEL,
        said,
    )
}

/// The fields of the General card.
fn fields(rux: &mut rux::Rux<'_>, left: f32, right: f32, top: f32, form: &mut SourceForm) -> f32 {
    let mut pen = top;
    let at = label(rux, left, pen, "Name");
    let field = Rect::from_min_max(Pos2::new(at, pen), Pos2::new(right, pen + SOURCE_FIELD));
    kit::text_well(rux, field, "Name", &mut form.source.name, "", false, false);
    pen = field.bottom() + SOURCE_ROW_GAP;

    // A track of three rather than a list, because which engine is chosen can then be seen without opening
    // anything.
    let at = label(rux, left, pen, "Engine");
    let engines = [Engine::Postgres, Engine::Sqlite, Engine::Inillucent];
    let names: Vec<&str> = engines.iter().map(|engine| engine.title()).collect();
    let width = kit::segmented_width(rux, &names, 90.0, false, 12.0);
    let track = Rect::from_min_size(Pos2::new(at, pen), Vec2::new(width, SOURCE_FIELD));
    let chosen = engines.iter().position(|engine| *engine == form.source.engine);
    if let Some(index) =
        kit::segmented_sized(rux, track, "Engine", &names, chosen, 90.0, false, 28.0, 12.0)
    {
        let engine = engines[index];
        form.source.engine = engine;
        if engine.is_a_file() {
            form.source.host = String::new();
            form.source.port = 0;
        } else if form.source.port == 0 {
            form.source.host = "localhost".to_owned();
            form.source.port = 5432;
        }
    }
    pen = track.bottom() + SOURCE_ROW_GAP;

    match form.source.engine {
        Engine::Sqlite | Engine::Inillucent => {
            // The field and a picker beside it. A path is often quicker pasted or typed than walked to, and an
            // agent cannot press a Browse button at all, so the field is not replaced by the picker.
            let at = label(rux, left, pen, "File");
            let browse_width = kit::button_width(rux, "Browse\u{2026}", false);
            let browse = Rect::from_min_size(
                Pos2::new(right - browse_width, pen + (SOURCE_FIELD - kit::BUTTON) / 2.0),
                Vec2::new(browse_width, kit::BUTTON),
            );
            let field = Rect::from_min_max(
                Pos2::new(at, pen),
                Pos2::new(browse.left() - 8.0, pen + SOURCE_FIELD),
            );
            kit::text_well(rux, field, "File", &mut form.source.database, "", true, false);
            if kit::button(rux, browse, "Browse\u{2026}", ButtonKind::Chip) {
                if let Some(chosen) = choose_a_sqlite_file(&form.source.database) {
                    form.source.database = chosen;
                    // A file that was just picked plainly exists, so whatever the last test said about the
                    // last path no longer applies.
                    form.tested = None;
                }
            }
            pen = help(
                rux,
                left,
                right,
                field.bottom(),
                "The path of a SQLite file. It is opened, never created: a mistyped path says so rather than \
                 quietly making an empty database.",
            );
        }
        Engine::Postgres => {
            let at = label(rux, left, pen, "Host and port");
            let port =
                Rect::from_min_size(Pos2::new(right - 84.0, pen), Vec2::new(84.0, SOURCE_FIELD));
            let host = Rect::from_min_max(
                Pos2::new(at, pen),
                Pos2::new(port.left() - 8.0, pen + SOURCE_FIELD),
            );
            kit::text_well(rux, host, "Host", &mut form.source.host, "", true, false);
            let mut said = form.source.port.to_string();
            if kit::text_well(rux, port, "Port", &mut said, "", true, false).changed() {
                form.source.port = said.parse().unwrap_or(form.source.port);
            }
            pen = host.bottom() + SOURCE_ROW_GAP;
            for (name, value) in
                [("Database", &mut form.source.database), ("User", &mut form.source.user)]
            {
                let at = label(rux, left, pen, name);
                let field =
                    Rect::from_min_max(Pos2::new(at, pen), Pos2::new(right, pen + SOURCE_FIELD));
                kit::text_well(rux, field, name, value, "", true, false);
                pen = field.bottom() + SOURCE_ROW_GAP;
            }
            // **The password sits under the user**, which is where `task-1795` asks for it. A password typed
            // here is written to this machine's own credential store when Save is pressed, and what the settings
            // file records is the *name* of the entry. `egui`'s own password mode draws dots in the galley
            // rather than in the value, so a screenshot of this dialog carries no secret.
            let at = label(rux, left, pen, "Password");
            let field =
                Rect::from_min_max(Pos2::new(at, pen), Pos2::new(right, pen + SOURCE_FIELD));
            kit::text_well(rux, field, "Password", &mut form.typed, "", false, true);
            pen = help(rux, left, right, field.bottom(), secret_note(&form.source.secret))
                + SOURCE_ROW_GAP;

            // **`Connection security`, not `Encryption`.** `task-1795`: "No idea what encryption option
            // means." The row is `sslmode`, the values it stores are unchanged, and the words are the question a
            // person can answer.
            let at = label(rux, left, pen, "Connection security");
            let modes = [SslMode::Disable, SslMode::Prefer, SslMode::Require];
            let names: Vec<&str> = modes.iter().map(|mode| plain_words_for(*mode)).collect();
            let width = kit::segmented_width(rux, &names, 80.0, false, 12.0);
            let track = Rect::from_min_size(Pos2::new(at, pen), Vec2::new(width, SOURCE_FIELD));
            let chosen = modes.iter().position(|mode| *mode == form.source.ssl);
            if let Some(index) = kit::segmented_sized(
                rux,
                track,
                "Connection security",
                &names,
                chosen,
                80.0,
                false,
                28.0,
                12.0,
            ) {
                form.source.ssl = modes[index];
            }
            pen = help(
                rux,
                left,
                right,
                track.bottom(),
                "Whether the connection to the server is encrypted. Required refuses to connect without it.",
            );
        }
    }
    pen
}

/// What each `sslmode` is called on the dialog.
///
/// The stored value is unchanged - `disable`, `prefer` and `require` are what a settings file
/// already holds and what a server is told - so this is a label and nothing more.
fn plain_words_for(mode: SslMode) -> &'static str {
    match mode {
        SslMode::Disable => "Off",
        SslMode::Prefer => "If offered",
        SslMode::Require => "Required",
    }
}

/// Ask for a SQLite file, starting wherever the field already points.
///
/// The same `rfd` dialog `Open File` uses. It blocks inside a frame, which is what a native file
/// dialog is on every platform and what the window already does for `Open File`.
fn choose_a_sqlite_file(said: &str) -> Option<String> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Choose a SQLite database")
        .add_filter("SQLite database", &["db", "sqlite", "sqlite3", "db3"])
        .add_filter("Every file", &["*"]);
    // Start where the field points, which on a mistyped path is its folder rather than nowhere.
    let said = std::path::Path::new(said.trim());
    if let Some(folder) = said.parent().filter(|folder| folder.is_dir()) {
        dialog = dialog.set_directory(folder);
    }
    if let Some(name) = said.file_name().and_then(|name| name.to_str()) {
        dialog = dialog.set_file_name(name);
    }
    dialog.pick_file().map(|path| path.display().to_string())
}

/// What the line under the password field says, which depends on where the password is now.
fn secret_note(secret: &Secret) -> &'static str {
    match secret {
        Secret::Keychain(_) | Secret::Typed(_) => {
            "There is a password kept for this data source in this machine’s own credential store. \
             Type a new one to replace it, or leave this empty to keep the one that is there."
        }
        Secret::Environment(_) => {
            "This data source reads its password from an environment variable, which is how it was \
             set up. Type one here to move it into this machine’s own credential store instead."
        }
        Secret::None => {
            "Kept in this machine’s own credential store. Unluminous writes no password to a file, ever: \
             what the settings file records is the name of the entry."
        }
    }
}

/// Where the password for this form is, as the settings file will record it.
///
/// How wide the column list is, leaving the rest of the body for the statement beside it.
const COLUMNS: f32 = 400.0;
/// A row of the column list.
const COLUMN_ROW: f32 = 26.0;

/// The New Table dialog: a name, a list of columns, and the statement they compose beside them.
///
/// **The statement on the right is the one that will be sent**, from `unluminous_db::sql::create_table`,
/// which is the same call Create makes — the rule the pending-changes preview already keeps, so what
/// is read and what happens cannot drift apart.
fn new_table(
    explorer: &mut DatabaseExplorer,
    ctx: &egui::Context,
    look: &Look<'_>,
    mut form: TableForm,
) -> (Vec<Request>, bool) {
    let _ = look;
    let mut requests = Vec::new();
    let mut close = false;
    let mut create = false;
    let engine = explorer
        .configuration
        .source(&form.source)
        .map(|source| source.engine)
        .unwrap_or(Engine::Postgres);
    if form.columns.is_empty() {
        form.columns.push(a_first_column(engine));
    }
    let statement =
        unluminous_db::sql::create_table(&form.schema, &form.name, &form.columns, engine);

    let (_, escaped) =
        modal::show(ctx, "unluminous-database-new-table", 760.0, 520.0, |ui, area| {
            if modal::header(ui, area, "New Table") {
                close = true;
            }
            let body = modal::body(area);
            let mut pen = body.top() + 2.0;

            // Which data source and schema, said rather than chosen: the dialog is opened from a row of
            // the tree, and a chooser here would be a second way to say a thing the click already said.
            let where_it_goes = format!(
                "{} · {}",
                form.source,
                match form.schema.is_empty() {
                    true => "no schema".to_owned(),
                    false => form.schema.clone(),
                }
            );
            modal::label(
                ui.painter(),
                Rect::from_min_size(Pos2::new(body.left(), pen), Vec2::new(body.width(), FIELD)),
                body.left(),
                &where_it_goes,
                color::text_faint(),
                11.5,
            );
            pen += FIELD;

            modal::field(ui, labelled(ui, body, pen, "Name"), "Table name", &mut form.name);
            pen += FIELD + GAP * 2.0;

            let left = Rect::from_min_max(
                Pos2::new(body.left(), pen),
                Pos2::new(body.left() + COLUMNS, body.bottom() - 8.0),
            );
            let right = Rect::from_min_max(
                Pos2::new(left.right() + 16.0, pen),
                Pos2::new(body.right(), body.bottom() - 8.0),
            );
            let after = modal::section(ui, left, pen, "Columns");
            columns(ui, left, after, &mut form, engine);
            let after_right = modal::section(ui, right, pen, "SQL");
            let block = Rect::from_min_max(Pos2::new(right.left(), after_right), right.max);
            let said = match (&statement, &form.problem) {
                (_, Some(problem)) => problem.clone(),
                (Ok(sql), None) => format!("{sql};"),
                (Err(why), None) => why.clone(),
            };
            modal::monospaced(ui, block, "database-new-table-sql", &said);

            match modal::footer(ui, area, &[("Cancel", true), ("Create", statement.is_ok())]) {
                Some(0) => close = true,
                Some(1) => create = true,
                _ => {}
            }
        });

    if create {
        match statement {
            Ok(sql) => match explorer.run_the_ddl(&form.source, &form.schema, &sql) {
                Ok(said) => {
                    requests.push(Request::Message(said));
                    close = true;
                }
                Err(why) => form.problem = Some(why),
            },
            Err(why) => form.problem = Some(why),
        }
    }
    let closed = close || escaped;
    match closed {
        true => explorer.modal = None,
        false => explorer.modal = Some(Modal::NewTable(form)),
    }
    (requests, closed)
}

/// The column a new table starts with: a key called `id`, of whichever integer type counts itself up.
fn a_first_column(engine: Engine) -> ColumnForm {
    ColumnForm {
        name: "id".to_owned(),
        type_name: match engine {
            Engine::Sqlite | Engine::Inillucent => "INTEGER".to_owned(),
            Engine::Postgres => "integer".to_owned(),
        },
        in_key: true,
        not_null: true,
    }
}

/// The list of columns: a name, a type, `PK`, `NN`, and a cross to take the row away.
fn columns(ui: &mut egui::Ui, area: Rect, top: f32, form: &mut TableForm, engine: Engine) {
    let mut pen = top;
    let mut removing: Option<usize> = None;
    // What is left after the type field, the two tick boxes and the cross. The tick boxes need 46
    // apiece rather than 34: `modal::check` draws its word ten points past a sixteen point box, so
    // at 34 the `PK` label was drawn under the `NN` box beside it. `task-1795`.
    let name_width = area.width() - 286.0;
    for index in 0..form.columns.len() {
        let row =
            Rect::from_min_size(Pos2::new(area.left(), pen), Vec2::new(area.width(), COLUMN_ROW));
        let name = Rect::from_min_size(row.min, Vec2::new(name_width.max(90.0), FIELD));
        modal::field(
            ui,
            name,
            &format!("Column {} name", index + 1),
            &mut form.columns[index].name,
        );

        let kind =
            Rect::from_min_size(Pos2::new(name.right() + 6.0, row.top()), Vec2::new(140.0, FIELD));
        // A dropdown that also takes typing: the field is the value and the chevron beside it offers
        // the engine’s own list. See `Engine::column_types` for why the list is not closed.
        let typed = Rect::from_min_max(kind.min, Pos2::new(kind.right() - 22.0, kind.bottom()));
        modal::field(
            ui,
            typed,
            &format!("Column {} type", index + 1),
            &mut form.columns[index].type_name,
        );
        let chevron =
            Rect::from_min_size(Pos2::new(kind.right() - 22.0, kind.top()), Vec2::new(22.0, FIELD));
        let chosen = crate::components::controls::dropdown(
            ui,
            chevron,
            "",
            &format!("Column {} type list", index + 1),
            None,
            |ui| {
                let mut chosen = None;
                for offered in engine.column_types() {
                    if ui.selectable_label(false, *offered).clicked() {
                        chosen = Some((*offered).to_owned());
                    }
                }
                chosen
            },
        );
        if let Some(chosen) = chosen {
            form.columns[index].type_name = chosen;
        }

        // Named per column and drawn as two letters: every row draws the same pair, so a name of its
        // own is what keeps two controls from sharing one — the rule the grid's cells already keep by
        // being `title row 1` — and it is what gives each its own id.
        let key = Rect::from_min_size(
            Pos2::new(kind.right() + 8.0, row.top() + 2.0),
            Vec2::new(46.0, 20.0),
        );
        modal::check_named(
            ui,
            key,
            "PK",
            &format!("Column {} PK", index + 1),
            &mut form.columns[index].in_key,
        );
        let not_null = Rect::from_min_size(
            Pos2::new(key.right() + 8.0, row.top() + 2.0),
            Vec2::new(46.0, 20.0),
        );
        modal::check_named(
            ui,
            not_null,
            "NN",
            &format!("Column {} NN", index + 1),
            &mut form.columns[index].not_null,
        );

        // Absent on the only row there is: a control that cannot apply is not drawn, and a table with
        // no columns at all is not a table.
        if form.columns.len() > 1 {
            let cross = Rect::from_center_size(
                Pos2::new(row.right() - 12.0, row.center().y),
                Vec2::splat(18.0),
            );
            if crate::components::controls::icon_button(
                ui,
                cross,
                &format!("Remove column {}", index + 1),
                crate::theme::icon::cross,
            ) {
                removing = Some(index);
            }
        }
        pen = row.bottom() + 4.0;
    }
    if let Some(index) = removing {
        form.columns.remove(index);
    }
    let add = Rect::from_min_size(Pos2::new(area.left(), pen + 4.0), Vec2::new(120.0, 26.0));
    if modal::button(ui, add, "Add column", true, false) {
        form.columns.push(ColumnForm {
            type_name: engine
                .column_types()
                .first()
                .map(|first| (*first).to_owned())
                .unwrap_or_default(),
            ..ColumnForm::default()
        });
    }
}

/// The statements Submit will send.
fn preview(
    explorer: &mut DatabaseExplorer,
    ctx: &egui::Context,
    page: u64,
) -> (Vec<Request>, bool) {
    let text = match explorer.preview(page) {
        Ok(statements) if statements.is_empty() => "There is nothing pending.".to_owned(),
        Ok(statements) => statements
            .iter()
            .map(|statement| {
                let values: Vec<String> = statement
                    .values
                    .iter()
                    .enumerate()
                    .map(|(at, value)| match value.is_null() {
                        true => format!("  ${} = NULL", at + 1),
                        false => format!("  ${} = {}", at + 1, value.display()),
                    })
                    .collect();
                format!("-- {}\n{};\n{}", statement.what, statement.sql, values.join("\n"))
            })
            .collect::<Vec<String>>()
            .join("\n\n"),
        Err(why) => why,
    };
    reading(explorer, ctx, "Pending changes", &text)
}

/// A modal that shows a block of text and nothing else.
fn reading(
    explorer: &mut DatabaseExplorer,
    ctx: &egui::Context,
    heading: &str,
    text: &str,
) -> (Vec<Request>, bool) {
    let mut requests = Vec::new();
    let mut close = false;
    let mut copy = false;
    let (_, escaped) = modal::show(ctx, "unluminous-database-reading", 640.0, 470.0, |ui, area| {
        if modal::header(ui, area, heading) {
            close = true;
        }
        let body = modal::body(area);
        modal::monospaced(ui, body, "database-reading-text", text);
        match modal::footer(ui, area, &[("Copy", true), ("Done", true)]) {
            Some(0) => copy = true,
            Some(1) => close = true,
            _ => {}
        }
    });
    if copy {
        // The window owns the one handle to the clipboard, so a provider asks rather than reaching —
        // `Request::Copy`'s own reason.
        requests.push(Request::Copy(text.to_owned()));
    }
    let closed = close || escaped;
    if closed {
        explorer.modal = None;
    }
    (requests, closed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a vector's components look like written out, six to a line, aligned.
    ///
    /// **`task-1984` S18.** This file is 792 lines with no test in it, and the reason it had none is
    /// that nearly all of it draws — which is what the screenshot tests are for. What is here and is
    /// **not** drawing is the text a dialog puts in front of somebody, and that is what these check:
    /// the words are the thing a person reads and the thing a wrong change would silently alter.
    ///
    /// `numbered` in particular is the one place a vector is written out at full precision, which is
    /// the whole point of the modal: `task-1814` asks for *"ways to see our vectors"*, and a grid cell
    /// showing three of several hundred components is not one of them.
    #[test]
    fn a_vector_is_written_out_six_components_to_a_line_and_aligned() {
        let vector = unluminous_db::Vector { values: (0..8).map(|at| at as f32 / 8.0).collect() };
        let written = numbered(&vector);
        let rows: Vec<&str> = written.split('\n').collect();
        assert_eq!(rows.len(), 2, "eight components are two rows of six: {written:?}");
        assert!(rows[0].starts_with("    0:"), "the index is right aligned: {:?}", rows[0]);
        assert!(rows[0].contains("0.125000"), "and six decimal places: {:?}", rows[0]);
        // Every row is the same width, which is what makes a column of them readable at all.
        assert_eq!(
            rows[0].len(),
            6 * 21,
            "a full row of six, each five for the index, eleven for the value and the gaps"
        );
        assert_eq!(rows[1].len(), 2 * 21, "and a short last row of two");
    }

    /// An empty vector is an empty block rather than a row of nothing.
    ///
    /// `task-1984` S18. `unluminous-db::vector` is deliberately permissive — a PNG's first eight
    /// bytes decode to two perfectly finite floats — so a value with nothing in it really does reach
    /// here, and what it must not do is produce a line of padding that looks like data.
    #[test]
    fn a_vector_with_nothing_in_it_is_written_as_nothing() {
        let empty = unluminous_db::Vector { values: Vec::new() };
        assert_eq!(numbered(&empty), "");
    }

    /// Connection security is named in words a person can choose between.
    ///
    /// **`task-1984` S18, and `task-1795`'s own report**: *"No idea what encryption option means."* It
    /// was `sslmode`, which is a PostgreSQL setting rather than a sentence. Every value has to keep
    /// plain words, and none of them may be the protocol's own spelling.
    #[test]
    fn every_connection_security_has_plain_words() {
        for mode in [SslMode::Disable, SslMode::Prefer, SslMode::Require] {
            let said = plain_words_for(mode);
            assert!(!said.is_empty(), "{mode:?} says nothing");
            assert!(
                !said.to_lowercase().contains("ssl"),
                "{mode:?} says {said:?}, which is the setting's name rather than what it does"
            );
        }
    }

    /// Every state a password can be in says where it is kept, and none of them says the password.
    ///
    /// **`task-1984` S18.** `services::agent_tasks::keychain` says a secret must not go in a settings
    /// file, and the Settings page says `set` or `not set` and never the value. This is the sentence
    /// under the field that explains it, and the thing it must never do is grow a case that names a
    /// value instead of a place.
    #[test]
    fn every_password_state_says_where_it_is_kept() {
        let states = [
            Secret::None,
            Secret::Keychain("unluminous-db-one".to_owned()),
            Secret::Typed("hunter2".to_owned()),
            Secret::Environment("PGPASSWORD".to_owned()),
        ];
        for secret in &states {
            let said = secret_note(secret);
            assert!(said.len() > 40, "{secret:?} says too little: {said:?}");
            assert!(!said.contains("hunter2"), "{secret:?} put the password in the sentence");
        }
        // And the three that are not the same thing say different things.
        assert_ne!(secret_note(&states[0]), secret_note(&states[1]));
        assert_ne!(secret_note(&states[1]), secret_note(&states[3]));
    }
}
