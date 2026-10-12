//! A query console: the toolbar, the SQL editor, and what the last statement answered.
//!
//! The reference screenshots’ `db_query_console_overview.png` is the picture. Its toolbar is Execute, history, an
//! in-editor-results toggle, settings, `Tx: Auto`, stop, the session chooser and — pinned right — the
//! schema switcher; what applies here is Execute, Stop, the row limit and the schema switcher.
//!
//! **Execute runs the statement under the caret**, which is the reference editor's behaviour and the only one that
//! makes a console holding six statements usable. `unluminous_db::sql::at` is what decides which, and it
//! knows about the four things a `;` hides inside — a string, a quoted identifier, a comment and a
//! dollar-quoted body.
//!
//! **The editor is an `egui::TextEdit` with a layouter that colours through the window's own
//! highlighter**, not a second copy of Unluminous's editor. It has selection, undo and the clipboard, and
//! it does not have folding, multiple carets, the gutter or find-in-file — which `plugin.limitations`
//! says rather than leaving it to be discovered. A provider draws inside the rectangle it is handed
//! and cannot reach `components::editor_view`; that is `tasks/ui-plugin-architecture.md`'s own rule.

use egui::{Pos2, Rect, Vec2};
use rux::icon::Icon;
use rux::Style;

use crate::components::database::{code, kit, well, Act};
use crate::services::database::{DatabaseExplorer, Page, Sheet};
use crate::services::plugin_ui::Look;
use crate::theme::color;

/// How tall the results area is, as a share of the page.
const RESULTS: f32 = 0.45;
/// The same when the result is a grid, whose toolbar, `WHERE` and `ORDER BY` fields and paging strip take
/// about 130 points before the first row.
const RESULTS_AS_A_GRID: f32 = 0.62;
/// How tall the console's toolbar is: a 38 point chip and the Run pill.
pub const TOOLBAR: f32 = 38.0;
/// The widest the data source dropdown is drawn.
const SOURCE_WIDEST: f32 = 220.0;

/// Draw one console.
pub fn show(
    explorer: &mut DatabaseExplorer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
    id: u64,
) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let bar = Rect::from_min_size(area.min, Vec2::new(area.width(), TOOLBAR * scale));
    acts.extend(toolbar(explorer, ui, look, bar, id));

    let (has_result, as_a_grid) = match explorer.page(id) {
        Some(Page { sheet: Sheet::Console(console), .. }) => (
            console.grid.is_some()
                || console.result.is_some()
                || console.failure.is_some()
                || !console.output.is_empty(),
            console.grid.is_some(),
        ),
        _ => (false, false),
    };
    let share = match as_a_grid {
        true => RESULTS_AS_A_GRID,
        false => RESULTS,
    };
    let split = match has_result {
        true => area.bottom() - area.height() * share,
        false => area.bottom(),
    };
    let editor = Rect::from_min_max(
        Pos2::new(area.left(), bar.bottom() + 12.0 * scale),
        Pos2::new(area.right(), split - 6.0 * scale),
    );
    if editor.height() > 20.0 {
        acts.extend(sql_editor(explorer, ui, look, editor, id));
    }
    if has_result {
        let results = Rect::from_min_max(Pos2::new(area.left(), split + 6.0 * scale), area.max);
        acts.extend(the_results(explorer, ui, look, results, id));
    }
    acts
}

/// Run and Stop at the left, and the data source the console is on, as a dropdown, at the right.
///
/// `task-2250`: *"The query console should allow me to select which db to use in a dropdown on the right.
/// Right now it just shows the db currently referenced."* The dropdown is the Settings design's chip, which is
/// `settings_kit::select`, so it is the same control the Settings window chooses a value from a list with.
fn toolbar(
    explorer: &DatabaseExplorer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    bar: Rect,
    id: u64,
) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let Some(Page { sheet: Sheet::Console(console), .. }) = explorer.page(id) else { return acts };
    let running = console.running.is_some();
    // Stop is absent unless something is running **and this engine can stop one**: a control that
    // cannot apply is not drawn, which is the same rule that leaves the `F` button off a `.rs` file.
    let can_stop = running && explorer.can_stop(&console.source);
    let names: Vec<String> = explorer.sources().iter().map(|source| source.name.clone()).collect();
    let chosen = names.iter().position(|name| *name == console.source);
    let schema = console.schema.clone();
    let time = ui.input(|input| input.time);
    kit::layer(ui, scale, ("database-console-bar", id), bar, |rux| {
        // The one button on the page that does the thing the page is for, so it is the action's pill
        // rather than an icon in a row of icons.
        let run = rux::components::PillButton::new("Execute")
            .primary()
            .mark(Icon::Play)
            .enabled(!running);
        let size = run.measure(rux);
        let run_rect =
            Rect::from_min_size(Pos2::new(bar.left(), bar.center().y - size.y / 2.0), size);
        if run.show(rux, run_rect).clicked() {
            acts.push(Act::Execute(id));
        }
        let mut pen = run_rect.right() + 10.0 * scale;
        if can_stop {
            let stop = Rect::from_center_size(
                Pos2::new(pen + 16.0 * scale, bar.center().y),
                Vec2::splat(32.0 * scale),
            );
            if kit::chip_button(rux, stop, Icon::Pause, 32.0, "Stop") {
                acts.push(Act::Stop(id));
            }
            pen = stop.right() + 10.0 * scale;
        }
        if running {
            crate::components::database::waiting(
                rux.painter(),
                Pos2::new(pen + 10.0 * scale, bar.center().y),
                color::accent(),
                time,
            );
            pen += 28.0 * scale;
        }
        // The data source, pinned to the right, which is where the reference editor's schema switcher is.
        let room = bar.right() - pen;
        let width = (room * 0.6).min(SOURCE_WIDEST * scale);
        if width < 90.0 * scale || names.is_empty() {
            return;
        }
        let height = crate::components::settings_kit::CONTROL * scale;
        let rect = Rect::from_min_size(
            Pos2::new(bar.right() - width, bar.center().y - height / 2.0),
            Vec2::new(width, height),
        );
        if let Some(picked) =
            crate::components::settings_kit::select(rux, rect, "Data source", &names, chosen)
        {
            if let Some(name) = names.get(picked) {
                acts.push(Act::ConsoleSource(id, name.clone()));
            }
        }
        // The schema the console reads, in front of the dropdown, when there is room to name it.
        if !schema.is_empty() && rect.left() - pen > 80.0 * scale {
            let words = format!("schema {schema}");
            let style = rux.zs(Style::sans(11.5));
            let width = rux.measure(style, &words).x.min(rect.left() - pen - 12.0 * scale);
            kit::quiet(
                rux,
                Pos2::new(rect.left() - 12.0 * scale - width, bar.center().y),
                &words,
                11.5,
                width,
            );
        }
    });
    acts
}

/// The editor, with SQL coloured through the window's own highlighter.
fn sql_editor(
    explorer: &mut DatabaseExplorer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
    id: u64,
) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    well(ui, look, area, kit::WELL_RADIUS * scale);
    let inner = area.shrink2(Vec2::new(14.0 * scale, 12.0 * scale));
    let Some(page) = explorer.pages.iter_mut().find(|page| page.id == id) else { return acts };
    let Sheet::Console(console) = &mut page.sheet else { return acts };
    // The whole well takes a click, not only the box inset eight points inside it. A press in that
    // margin used to leave the console without the keyboard, which is why `Ctrl/Cmd+Enter` below did
    // nothing and why a paste went into the file behind the tab — `task-1795`.
    let sql_id = egui::Id::new(("database-console-sql", id));
    crate::components::controls::claim_the_field(ui, area, sql_id, "Sql field");
    let mut child =
        ui.new_child(egui::UiBuilder::new().max_rect(inner).id_salt(("database-console", id)));
    // **The colouring comes from the window's own plugins**, through the same `CodeHighlighter` the
    // Markdown preview colours a fenced block with — so the console and a `.sql` file agree by
    // construction rather than by two lists being kept in step. With no highlighter, which is every
    // window with no plugins loaded, the text keeps the one code colour it always had.
    let size = look.monospace_size;
    let highlighter = look.highlighter;
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
        let mut job = colour_the_sql(text.as_str(), size, highlighter);
        job.wrap.max_width = width;
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };
    let response = child.add(
        egui::TextEdit::multiline(&mut console.text)
            .id(sql_id)
            .frame(egui::Frame::NONE)
            .code_editor()
            .font(egui::FontId::monospace(size))
            .desired_width(inner.width())
            .desired_rows((inner.height() / (size * 1.4)).max(1.0) as usize)
            .layouter(&mut layouter)
            .text_color(color::text_control()),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "SQL"));
    // Where the caret is decides which statement Execute runs, so it is read back every frame rather
    // than being guessed at from the end of the text.
    if let Some(state) = egui::TextEdit::load_state(child.ctx(), response.id) {
        if let Some(range) = state.cursor.char_range() {
            // egui counts the caret in **characters** and `unluminous_db::sql` reads **bytes**, so the two
            // have to be converted rather than assumed equal: they differ the moment anything above
            // ASCII is in a statement, which for a database is any value at all.
            let at = range.primary.index.0;
            console.caret = console
                .text
                .char_indices()
                .nth(at)
                .map(|(byte, _)| byte)
                .unwrap_or(console.text.len());
        }
    }
    // `Ctrl`/`Cmd + Enter` is Execute, which is what every console in every tool binds it to. It is
    // read here rather than as a menu shortcut because a plugin's manifest cannot claim a chord —
    // `tasks/ui-plugin-architecture.md` §10 — and because it must only apply while this field has the
    // keyboard.
    if response.has_focus()
        && child.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter))
    {
        acts.push(Act::Execute(id));
    }
    acts
}

/// One statement, coloured by whatever plugin claims SQL.
///
/// The ranges come back as byte ranges into the text, and anything between two of them is drawn in
/// the plain code colour — which is what happens with no highlighter at all, so the fallback and the
/// gaps are the same code path rather than two that agree.
fn colour_the_sql(
    text: &str,
    size: f32,
    highlighter: Option<&dyn unluminous_core::CodeHighlighter>,
) -> egui::text::LayoutJob {
    let font = egui::FontId::monospace(size);
    let mut job = egui::text::LayoutJob::default();
    let coloured =
        highlighter.map(|highlighter| highlighter.colour("sql", text)).unwrap_or_default();
    let mut at = 0;
    for (range, colour) in coloured {
        if range.start < at
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            continue;
        }
        if range.start > at {
            job.append(&text[at..range.start], 0.0, format(font.clone(), color::text_control()));
        }
        let tint = egui::Color32::from_rgb(colour.r, colour.g, colour.b);
        job.append(&text[range.start..range.end], 0.0, format(font.clone(), tint));
        at = range.end;
    }
    if at < text.len() {
        job.append(&text[at..], 0.0, format(font, color::text_control()));
    }
    job
}

fn format(font: egui::FontId, tint: egui::Color32) -> egui::text::TextFormat {
    egui::text::TextFormat { font_id: font, color: tint, ..egui::text::TextFormat::default() }
}

/// `Result` and the grid a `SELECT` answered in.
fn the_grid(
    explorer: &mut DatabaseExplorer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
    id: u64,
) -> Vec<Act> {
    let scale = look.scale();
    let detail = match explorer.grid(id) {
        Some(grid) if grid.running.is_some() => "reading".to_owned(),
        Some(grid) => grid.rows.summary(),
        None => String::new(),
    };
    let heading = Rect::from_min_size(area.min, Vec2::new(area.width(), 22.0 * scale));
    kit::layer(ui, scale, ("database-console-heading", id), heading, |rux| {
        let at = Pos2::new(heading.left() + 2.0 * scale, heading.center().y);
        let drawn = kit::heading(rux, at, "Result", 13.0, heading.width());
        if !detail.is_empty() {
            kit::quiet(
                rux,
                Pos2::new(heading.left() + drawn + 12.0 * scale, heading.center().y),
                &detail,
                11.5,
                heading.width() - drawn - 12.0 * scale,
            );
        }
    });
    let body = Rect::from_min_max(Pos2::new(area.left(), heading.bottom() + 6.0 * scale), area.max);
    crate::components::database::grid::show(explorer, ui, look, body, id)
}

/// `Output` and `Result`, under the editor: a heading with what came back, and a carved well holding it.
fn the_results(
    explorer: &mut DatabaseExplorer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
    id: u64,
) -> Vec<Act> {
    let scale = look.scale();
    let acts = Vec::new();
    let Some(Page { sheet: Sheet::Console(console), .. }) = explorer.page(id) else { return acts };
    // **A `SELECT` answers in the grid a table opens in** (`task-2253`): *"That should be the same result format
    // as when I double click on a table, so i can edit, sort, save, etc."* The grid draws its own toolbar,
    // fields, wells and paging, so the console only puts its heading above it.
    if console.grid.is_some() {
        return the_grid(explorer, ui, look, area, id);
    }
    let output = console.output.clone();
    let result = console.result.clone();
    let failure = console.failure.clone();

    let heading = Rect::from_min_size(area.min, Vec2::new(area.width(), 22.0 * scale));
    let (said, detail) = match (&result, &failure) {
        (_, Some(_)) => ("Output".to_owned(), "the statement failed".to_owned()),
        (Some(rows), None) => ("Result".to_owned(), rows.summary()),
        (None, None) => ("Output".to_owned(), String::new()),
    };
    kit::layer(ui, scale, ("database-console-heading", id), heading, |rux| {
        let at = Pos2::new(heading.left() + 2.0 * scale, heading.center().y);
        let drawn = kit::heading(rux, at, &said, 13.0, heading.width());
        if !detail.is_empty() {
            kit::quiet(
                rux,
                Pos2::new(heading.left() + drawn + 12.0 * scale, heading.center().y),
                &detail,
                11.5,
                heading.width() - drawn - 12.0 * scale,
            );
        }
    });
    let body = Rect::from_min_max(Pos2::new(area.left(), heading.bottom() + 8.0 * scale), area.max);
    well(ui, look, body, kit::WELL_RADIUS * scale);
    let inner = body.shrink2(Vec2::new(10.0 * scale, 8.0 * scale));

    match (&result, &failure) {
        (_, Some(why)) => {
            // The server's own words, verbatim, with its `SQLSTATE`, its detail and its hint — the
            // rule `unluminous-git` keeps about quoting a program rather than summarising it.
            let painter = ui.painter_at(inner);
            let galley = painter.layout(
                why.to_string(),
                egui::FontId::monospace(look.monospace_size * 0.95),
                color::unsaved(),
                inner.width(),
            );
            painter.galley(inner.min, galley, color::unsaved());
        }
        (Some(rows), None) => {
            crate::components::database::grid::rows_only(ui, look, inner, rows, id);
        }
        (None, None) => {
            let painter = ui.painter_at(inner);
            let mut pen = inner.top() + 8.0 * scale;
            // Newest last, which is what a console log is: an `UPDATE`'s count, a `NOTICE`, a
            // `CREATE TABLE` that said nothing else.
            for line in output.iter().rev().take(12).collect::<Vec<&String>>().into_iter().rev() {
                code(
                    &painter,
                    Pos2::new(inner.left(), pen),
                    line,
                    color::text(),
                    look.monospace_size * 0.95,
                    inner.width(),
                );
                pen += look.monospace_size * 1.5;
            }
        }
    }
    acts
}
