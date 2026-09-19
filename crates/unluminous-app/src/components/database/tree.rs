//! The data source tree: The reference editor's Database tool window, cut to what applies.
//!
//! `_agent_output/task-1777-database-plugin/reference/db_database_tool_window.png` is the picture. Its
//! toolbar has eight buttons; five of them are things Unluminous can do, and the other three — bookmarks,
//! diagrams and the diagnostic menu — are §11 of the TDD. A control that cannot apply is absent, so
//! `Disconnect` is not drawn while nothing is connected and `Edit data` is not drawn unless a table is
//! chosen.
//!
//! **A row is 28 points**, which is `size::ROW` and what every list in Unluminous uses, multiplied by
//! `Look::scale` so it follows the editor's own font size.
//!
//! **Only the rows that intersect the clip rectangle are drawn.** A schema with four thousand tables
//! in it is one `Vec` and twenty painted rows, which is `task-1666`'s rule and what keeps the
//! decoration's canvas the size of the pane rather than the size of the tree.

use egui::{Pos2, Rect, Sense, Vec2};

use unluminous_db::catalog::Kind;

use crate::components::database::{along, card, text, waiting, well, Act, PAD, RADIUS, TOOLBAR};
use crate::services::database::{Aimed, DatabaseExplorer};
use crate::services::plugin_ui::Look;
use crate::theme::{color, icon};

/// One row of the tree, worked out before anything is drawn.
///
/// A flat list rather than a recursive draw, because only the rows that are on the screen are painted
/// and a recursive draw would have to walk the whole tree to find them.
#[derive(Debug, Clone)]
pub struct Line {
    pub depth: usize,
    pub what: What,
}

#[derive(Debug, Clone)]
pub enum What {
    Source {
        name: String,
        where_it_points: String,
        connected: bool,
        open: bool,
        busy: bool,
    },
    Problem {
        source: String,
        said: String,
    },
    Schema {
        source: String,
        name: String,
        open: bool,
    },
    Folder {
        source: String,
        schema: String,
        name: String,
        count: usize,
        open: bool,
    },
    Item {
        source: String,
        schema: String,
        name: String,
        kind: Kind,
        open: bool,
        /// What a search index declared about itself, drawn after its name.
        ///
        /// Empty for everything else. It is on the row rather than a level below it because it is not
        /// a thing to open - it is what the row *is*, and a person deciding whether to trust a result
        /// wants it in front of them rather than one press away.
        declared: String,
    },
    Column {
        name: String,
        type_name: String,
        in_key: bool,
        not_null: bool,
    },
    /// A schema that has been opened and has nothing in it, which is different from one still loading.
    Empty {
        said: String,
    },
}

/// Every row the tree would draw, in order, **built once per change** rather than once a frame.
///
/// `task-1984` S16: only the drawing was virtualised, so a tree with two hundred rows in it built
/// two hundred `Line` values with their strings on every frame whether or not anything had moved.
/// The key is `DatabaseExplorer::revision` -- bumped by `apply`, which is the one place an act
/// becomes a change, and by `take_the_replies`, which is the one place a worker's answer arrives --
/// and the filter, which is typed rather than acted.
pub fn lines_cached(explorer: &DatabaseExplorer) -> std::rc::Rc<Vec<Line>> {
    let key = (explorer.revision(), explorer.filter.clone());
    let mut kept = explorer.drawn_lines.borrow_mut();
    if kept.as_ref().map(|(was, _)| was) != Some(&key) {
        *kept = Some((key, std::rc::Rc::new(lines(explorer))));
    }
    kept.as_ref().map(|(_, rows)| std::rc::Rc::clone(rows)).unwrap_or_default()
}

/// Every row the tree would draw, in order.
pub fn lines(explorer: &DatabaseExplorer) -> Vec<Line> {
    let filter = explorer.filter.trim().to_lowercase();
    let mut out = Vec::new();
    for source in explorer.sources() {
        let open = explorer.open_sources.contains(&source.name);
        out.push(Line {
            depth: 0,
            what: What::Source {
                name: source.name.clone(),
                where_it_points: crate::services::paths::where_a_source_points(source),
                connected: explorer.is_connected(&source.name),
                open,
                busy: explorer.is_busy(&source.name),
            },
        });
        if !open {
            continue;
        }
        let Some(loaded) = explorer.loaded.get(&source.name) else { continue };
        if let Some(said) = &loaded.problem {
            out.push(Line {
                depth: 1,
                what: What::Problem { source: source.name.clone(), said: said.clone() },
            });
            continue;
        }
        for schema in &loaded.schemas {
            let schema_open = loaded.open_schemas.contains(schema);
            out.push(Line {
                depth: 1,
                what: What::Schema {
                    source: source.name.clone(),
                    name: schema.clone(),
                    open: schema_open,
                },
            });
            if !schema_open {
                continue;
            }
            let Some(items) = loaded.items.get(schema) else {
                out.push(Line { depth: 2, what: What::Empty { said: "reading…".to_owned() } });
                continue;
            };
            folders(explorer, &source.name, schema, items, &filter, &mut out);
        }
    }
    out
}

/// The `tables`, `views`, `routines` and `sequences` folders of one schema, and what is in them.
fn folders(
    explorer: &DatabaseExplorer,
    source: &str,
    schema: &str,
    items: &[unluminous_db::Item],
    filter: &str,
    out: &mut Vec<Line>,
) {
    let Some(loaded) = explorer.loaded.get(source) else { return };
    // The order the folders appear in, which is the reference editor's: what somebody looks for most, first.
    // `search` sits second because on an Inillucent database it is what somebody came to look at, and
    // `shadow` sits last because it is the search index's own plumbing rather than somewhere to work.
    for folder in ["tables", "search", "views", "routines", "sequences", "indexes", "shadow"] {
        let inside: Vec<&unluminous_db::Item> = items
            .iter()
            .filter(|item| item.kind.folder() == folder)
            .filter(|item| filter.is_empty() || item.name.to_lowercase().contains(filter))
            .collect();
        // A folder with nothing in it is absent rather than drawn empty, which is the same rule the
        // toolbar's buttons keep — and is the reference editor's own `Show Elements | Empty Groups` set to off.
        if inside.is_empty() {
            continue;
        }
        let open = loaded.open_folders.contains(&(schema.to_owned(), folder.to_owned()))
            // A filter that matches opens the folders it matched in, because a filter nobody can see
            // the results of is a filter that looks broken.
            || !filter.is_empty();
        out.push(Line {
            depth: 2,
            what: What::Folder {
                source: source.to_owned(),
                schema: schema.to_owned(),
                name: folder.to_owned(),
                count: inside.len(),
                open,
            },
        });
        if !open {
            continue;
        }
        for item in inside {
            let item_open = loaded.open_tables.contains(&(schema.to_owned(), item.name.clone()));
            out.push(Line {
                depth: 3,
                what: What::Item {
                    source: source.to_owned(),
                    schema: schema.to_owned(),
                    name: item.name.clone(),
                    kind: item.kind,
                    open: item_open,
                    declared: loaded
                        .searches
                        .get(&item.name)
                        .map(unluminous_db::SearchIndex::summary)
                        .unwrap_or_default(),
                },
            });
            if !item_open {
                continue;
            }
            match loaded.columns.get(&(schema.to_owned(), item.name.clone())) {
                Some(table) => out.extend(table.columns.iter().map(|column| Line {
                    depth: 4,
                    what: What::Column {
                        name: column.name.clone(),
                        type_name: column.type_name.clone(),
                        in_key: column.in_key,
                        not_null: column.not_null,
                    },
                })),
                None => {
                    out.push(Line { depth: 4, what: What::Empty { said: "reading…".to_owned() } })
                }
            }
        }
    }
}

/// Draw the pane.
pub fn show(
    explorer: &mut DatabaseExplorer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let panel = area.shrink(PAD * scale);
    card(ui, look, panel, look.palette.board_lane, RADIUS * scale);

    let inner = panel.shrink(PAD * scale);
    let mut pen = inner.top();
    let bar = Rect::from_min_size(
        Pos2::new(inner.left(), pen),
        Vec2::new(inner.width(), TOOLBAR * scale),
    );
    acts.extend(toolbar(explorer, ui, look, bar));
    pen = bar.bottom() + 4.0 * scale;

    let field =
        Rect::from_min_size(Pos2::new(inner.left(), pen), Vec2::new(inner.width(), 24.0 * scale));
    well(ui, look, field, 6.0 * scale);
    crate::components::controls::search_field_over(
        ui,
        field,
        "Filter database objects",
        "Filter",
        &mut explorer.filter,
        false,
    );
    pen = field.bottom() + 6.0 * scale;

    let rows = Rect::from_min_max(Pos2::new(inner.left(), pen), inner.max);
    acts.extend(the_rows(explorer, ui, look, rows));

    if let Some(problem) = &explorer.problem {
        let said = problem.clone();
        let strip = Rect::from_min_max(
            Pos2::new(panel.left(), panel.bottom() - 22.0 * scale),
            Pos2::new(panel.right(), panel.bottom()),
        );
        text(
            &ui.painter_at(strip),
            Pos2::new(strip.left() + 8.0 * scale, strip.center().y),
            &said,
            color::text_faint(),
            11.0 * scale,
            strip.width() - 16.0 * scale,
        );
    }
    acts
}

/// The five buttons that apply, of the reference editor's eight.
fn toolbar(explorer: &DatabaseExplorer, ui: &mut egui::Ui, look: &Look<'_>, bar: Rect) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let step = 26.0 * scale;
    let mut at = bar.left();
    let chosen = explorer.chosen.clone();
    let chosen_source = chosen
        .as_ref()
        .map(|chosen| chosen.source.clone())
        .unwrap_or_else(|| explorer.configuration.chosen.clone());

    if crate::components::controls::icon_button(
        ui,
        along(bar, &mut at, step),
        "New data source",
        icon::plus,
    ) {
        acts.push(Act::NewSource);
    }
    if !chosen_source.is_empty()
        && crate::components::controls::icon_button(
            ui,
            along(bar, &mut at, step),
            "Refresh",
            icon::rerun,
        )
    {
        acts.push(Act::Refresh(chosen_source.clone()));
    }
    // Absent while nothing is connected: a control that cannot apply is not drawn.
    if explorer.is_connected(&chosen_source)
        && crate::components::controls::icon_button(
            ui,
            along(bar, &mut at, step),
            "Disconnect",
            icon::stop,
        )
    {
        acts.push(Act::Disconnect(chosen_source.clone()));
    }
    if !chosen_source.is_empty()
        && crate::components::controls::icon_button(
            ui,
            along(bar, &mut at, step),
            "Open console",
            icon::run,
        )
    {
        acts.push(Act::OpenConsole(chosen_source.clone()));
    }
    if let Some(chosen) = chosen.filter(|chosen| !chosen.name.is_empty()) {
        if crate::components::controls::icon_button(
            ui,
            along(bar, &mut at, step),
            "Edit data",
            icon::table,
        ) {
            acts.push(Act::OpenTable(
                chosen.source.clone(),
                chosen.schema.clone(),
                chosen.name.clone(),
            ));
        }
        if crate::components::controls::icon_button(
            ui,
            along(bar, &mut at, step),
            "Show DDL",
            icon::copy,
        ) {
            acts.push(Act::Ddl(chosen.source, chosen.schema, chosen.name));
        }
    }
    acts
}

/// The rows themselves, in a scrolling area, drawing only what is on the screen.
fn the_rows(
    explorer: &mut DatabaseExplorer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
) -> Vec<Act> {
    let scale = look.scale();
    let row_height = look.row_height;
    let lines = lines_cached(explorer);
    let mut acts = Vec::new();
    let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(area).id_salt("database-tree"));
    let mut scrolled = egui::ScrollArea::vertical()
        .id_salt("database-tree-rows")
        .max_height(area.height())
        .auto_shrink([false, false]);
    if let Some(to) = explorer.scroll_to.take() {
        scrolled = scrolled.vertical_scroll_offset(to);
    }
    let out = scrolled.show_rows(&mut inside, row_height, lines.len(), |ui, range| {
        let mut acts = Vec::new();
        for index in range {
            let Some(line) = lines.get(index) else { continue };
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height), Sense::hover());
            acts.extend(one(explorer, ui, look, rect, line, index, scale));
        }
        acts
    });
    explorer.scrolled = out.state.offset.y;
    acts.extend(out.inner);
    acts
}

/// One row.
fn one(
    explorer: &DatabaseExplorer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    line: &Line,
    index: usize,
    scale: f32,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let indent = rect.left() + 6.0 * scale + line.depth as f32 * 12.0 * scale;
    let name = row_name(line);
    let chosen = is_chosen(explorer, line);
    let response = ui.interact(rect, ui.id().with(("database-tree-row", index)), Sense::click());
    let pill = rect.shrink2(Vec2::new(4.0 * scale, 1.0));
    if chosen {
        ui.painter().rect_filled(pill, egui::CornerRadius::same(5), look.palette.selected_row);
    } else if response.hovered() {
        ui.painter().rect_filled(pill, egui::CornerRadius::same(5), look.palette.control);
    }
    let painter = ui.painter();
    let middle = rect.center().y;
    let mut at = indent;

    // The disclosure, for a row that has anything under it.
    if let Some(open) = opens(line) {
        icon::disclosure_at(
            painter,
            Pos2::new(at + 5.0 * scale, middle),
            open,
            color::text_dim(),
            scale,
        );
    }
    at += 14.0 * scale;
    if let Some(draw) = mark(line) {
        draw(painter, Pos2::new(at + 6.0 * scale, middle), tint(line));
        at += 16.0 * scale;
    }
    let room = rect.right() - at - 8.0 * scale;
    let drawn =
        text(painter, Pos2::new(at, middle), &name, tint(line), look.font_size * 0.85, room);
    after(
        painter,
        line,
        Pos2::new(at + drawn + 8.0 * scale, middle),
        look,
        rect.right() - 8.0 * scale,
        scale,
    );
    if let What::Source { busy: true, .. } = &line.what {
        waiting(
            painter,
            Pos2::new(rect.right() - 16.0 * scale, middle),
            color::accent(),
            ui.input(|input| input.time),
        );
    }
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, chosen, &name));

    if response.clicked() {
        acts.extend(clicked(line));
    }
    if response.double_clicked() {
        if let What::Item { source, schema, name, kind, .. } = &line.what {
            if kind.holds_rows() {
                acts.push(Act::OpenTable(source.clone(), schema.clone(), name.clone()));
            }
        }
    }
    // A right click opens the row’s own menu, which is where New Table is — `task-1795`. It also
    // chooses the row, because a menu acting on something other than the row under the pointer is
    // the one thing a context menu must never do.
    if response.secondary_clicked() {
        if let Some(aimed) = aimed_at(line) {
            acts.extend(clicked(line).into_iter().filter(|act| matches!(act, Act::Choose(..))));
            let at = response.interact_pointer_pos().unwrap_or_else(|| rect.center());
            acts.push(Act::OpenMenu(at, aimed));
        }
    }
    acts
}

/// What a row is, for the menu.
fn aimed_at(line: &Line) -> Option<Aimed> {
    match &line.what {
        What::Source { name, .. } => Some(Aimed::Source(name.clone())),
        What::Schema { source, name, .. } => Some(Aimed::Schema(source.clone(), name.clone())),
        What::Item { source, schema, name, .. } => {
            Some(Aimed::Item(source.clone(), schema.clone(), name.clone()))
        }
        What::Column { name, .. } => Some(Aimed::Column(name.clone())),
        // A folder, a problem and a `reading…` row have nothing to offer, and a menu with one
        // dimmed row in it is worse than no menu.
        _ => None,
    }
}

/// The tree’s own menu, drawn where the pointer was.
///
/// `components::context_menu` is the window’s: it takes an `actions::Entry` and answers an
/// `actions::Action`, which is the window’s vocabulary and not a plugin’s. So this draws its own
/// rows and answers its own [`Act`], which is the split every other part of this plugin keeps — and
/// it uses the same `egui::Popup` and the same frame, so it looks like the explorer’s.
pub fn menu(explorer: &mut DatabaseExplorer, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Act> {
    let Some((at, aimed)) = explorer.menu.clone() else { return Vec::new() };
    let _ = look;
    let mut acts = Vec::new();
    let mut close = false;
    let rows: Vec<(&str, Act)> = match &aimed {
        Aimed::Source(source) => vec![
            ("New Table…", Act::NewTable(source.clone(), first_schema_of(explorer, source))),
            ("Open Query Console", Act::OpenConsole(source.clone())),
            ("Refresh", Act::Refresh(source.clone())),
            ("Edit Data Source…", Act::EditSource(source.clone())),
            ("Remove Data Source", Act::RemoveSource(source.clone())),
        ],
        Aimed::Schema(source, schema) => vec![
            ("New Table…", Act::NewTable(source.clone(), schema.clone())),
            ("Refresh", Act::Refresh(source.clone())),
            ("Copy Name", Act::Copy(schema.clone())),
        ],
        Aimed::Item(source, schema, name) => vec![
            ("Open Data", Act::OpenTable(source.clone(), schema.clone(), name.clone())),
            ("New Table…", Act::NewTable(source.clone(), schema.clone())),
            ("Show DDL", Act::Ddl(source.clone(), schema.clone(), name.clone())),
            ("Copy Name", Act::Copy(name.clone())),
            ("Drop Table", Act::DropTable(source.clone(), schema.clone(), name.clone())),
        ],
        Aimed::Column(name) => vec![("Copy Name", Act::Copy(name.clone()))],
    };
    let popup = egui::Popup::new(
        egui::Id::new("unluminous-database-tree-menu"),
        ui.ctx().clone(),
        at,
        ui.layer_id(),
    )
    .kind(egui::PopupKind::Menu)
    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
    .layout(egui::Layout::top_down_justified(egui::Align::Min))
    .frame(
        egui::Frame::popup(ui.style())
            .fill(color::menu())
            .stroke(egui::Stroke::new(1.0, color::control_border()))
            .inner_margin(6),
    )
    .width(220.0);
    if let Some(shown) = popup.show(|ui| {
        let mut chosen: Option<Act> = None;
        for (name, act) in rows {
            if crate::components::controls::menu_row(ui, name, "", true, false, 0.0) {
                chosen = Some(act);
            }
        }
        chosen
    }) {
        if let Some(act) = shown.inner {
            acts.push(act);
            close = true;
        }
        if shown.response.should_close() {
            close = true;
        }
    } else {
        close = true;
    }
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if close {
        explorer.menu = None;
    }
    acts
}

/// The schema a new table goes in when the menu was opened on the data source itself.
fn first_schema_of(explorer: &DatabaseExplorer, source: &str) -> String {
    explorer
        .loaded
        .get(source)
        .and_then(|loaded| loaded.schemas.first().cloned())
        .unwrap_or_default()
}

/// What one press of a row does.
fn clicked(line: &Line) -> Vec<Act> {
    match &line.what {
        What::Source { name, .. } => vec![
            Act::Choose(name.clone(), String::new(), String::new()),
            Act::ToggleSource(name.clone()),
        ],
        What::Schema { source, name, .. } => vec![
            Act::Choose(source.clone(), name.clone(), String::new()),
            Act::ToggleSchema(source.clone(), name.clone()),
        ],
        What::Folder { source, schema, name, .. } => {
            vec![Act::ToggleFolder(source.clone(), schema.clone(), name.clone())]
        }
        What::Item { source, schema, name, kind, .. } => {
            let mut acts = vec![Act::Choose(source.clone(), schema.clone(), name.clone())];
            if kind.holds_rows() {
                acts.push(Act::ToggleTable(source.clone(), schema.clone(), name.clone()));
            }
            acts
        }
        What::Problem { source, .. } => vec![Act::Connect(source.clone())],
        _ => Vec::new(),
    }
}

fn row_name(line: &Line) -> String {
    match &line.what {
        What::Source { name, .. } => name.clone(),
        What::Problem { said, .. } => said.clone(),
        What::Schema { name, .. } => name.clone(),
        What::Folder { name, .. } => name.clone(),
        What::Item { name, .. } => name.clone(),
        What::Column { name, .. } => name.clone(),
        What::Empty { said } => said.clone(),
    }
}

/// What is drawn after the name: where a source points, a folder's count, a column's type.
fn after(painter: &egui::Painter, line: &Line, at: Pos2, look: &Look<'_>, right: f32, scale: f32) {
    let room = right - at.x;
    if room < 20.0 {
        return;
    }
    let size = look.font_size * 0.75;
    match &line.what {
        What::Source { where_it_points, connected, .. } => {
            let said = match connected {
                true => where_it_points.clone(),
                false => format!("{where_it_points} · not connected"),
            };
            text(painter, at, &said, color::text_faint(), size, room);
        }
        // The count the reference editor puts on a folder, which is how an empty schema is told from one that has
        // not been read yet.
        What::Folder { count, .. } => {
            text(painter, at, &count.to_string(), color::text_faint(), size, room);
        }
        // A search index says what it is: how wide its vectors are, whether its answers are exact,
        // which distance they are exact about, and which analysis produced its terms.
        What::Item { declared, .. } if !declared.is_empty() => {
            text(painter, at, declared, color::text_faint(), size, room);
        }
        What::Column { type_name, not_null, .. } => {
            let said = match not_null {
                true => format!("{type_name} not null"),
                false => type_name.clone(),
            };
            text(painter, at, &said, color::text_faint(), size, room);
        }
        _ => {
            let _ = scale;
        }
    }
}

fn opens(line: &Line) -> Option<bool> {
    match &line.what {
        What::Source { open, .. } | What::Schema { open, .. } | What::Folder { open, .. } => {
            Some(*open)
        }
        What::Item { open, kind, .. } => kind.holds_rows().then_some(*open),
        _ => None,
    }
}

/// The drawn icon in front of a row.
fn mark(line: &Line) -> Option<fn(&egui::Painter, Pos2, egui::Color32)> {
    match &line.what {
        What::Source { .. } => Some(icon::database),
        What::Schema { .. } => Some(icon::folder),
        What::Folder { .. } => Some(icon::folder),
        What::Item { kind, .. } => Some(match kind {
            Kind::Routine => icon::run,
            Kind::Sequence => icon::clock,
            // A search index is drawn with the mark the window already uses for finding something,
            // which is what it is: `icon::table` would make it one more table in a list of tables.
            Kind::Search => icon::magnifier,
            _ => icon::table,
        }),
        What::Column { in_key: true, .. } => Some(icon::key),
        _ => None,
    }
}

fn tint(line: &Line) -> egui::Color32 {
    match &line.what {
        What::Source { connected: true, .. } => color::text_strong(),
        What::Source { .. } => color::text(),
        What::Problem { .. } => color::unsaved(),
        What::Empty { .. } => color::text_faint(),
        What::Column { in_key: true, .. } => color::accent(),
        _ => color::text(),
    }
}

fn is_chosen(explorer: &DatabaseExplorer, line: &Line) -> bool {
    let Some(chosen) = &explorer.chosen else { return false };
    match &line.what {
        What::Source { name, .. } => {
            chosen.source == *name && chosen.name.is_empty() && chosen.schema.is_empty()
        }
        What::Schema { source, name, .. } => {
            chosen.source == *source && chosen.schema == *name && chosen.name.is_empty()
        }
        What::Item { source, schema, name, .. } => {
            chosen.source == *source && chosen.schema == *schema && chosen.name == *name
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database::{Chosen, Loaded};
    use unluminous_db::{Column, Source, Table};

    /// One source, `db`, opened with one schema, `public`, opened, holding one table, `users`, with
    /// its `tables` folder open and the table itself open — so `lines` walks every depth it can walk.
    fn a_tree() -> DatabaseExplorer {
        let mut explorer = DatabaseExplorer::new();
        explorer.configuration.sources.push(Source { name: "db".to_owned(), ..Default::default() });
        explorer.open_sources.insert("db".to_owned());
        let mut loaded = Loaded::default();
        loaded.schemas.push("public".to_owned());
        loaded.open_schemas.insert("public".to_owned());
        loaded.items.insert(
            "public".to_owned(),
            vec![
                unluminous_db::Item { name: "users".to_owned(), kind: Kind::Table },
                unluminous_db::Item { name: "get_user".to_owned(), kind: Kind::Routine },
            ],
        );
        loaded.open_folders.insert(("public".to_owned(), "tables".to_owned()));
        loaded.open_folders.insert(("public".to_owned(), "routines".to_owned()));
        loaded.open_tables.insert(("public".to_owned(), "users".to_owned()));
        loaded.columns.insert(
            ("public".to_owned(), "users".to_owned()),
            Table {
                schema: "public".to_owned(),
                name: "users".to_owned(),
                columns: vec![
                    Column {
                        name: "id".to_owned(),
                        type_name: "integer".to_owned(),
                        numeric: true,
                        not_null: true,
                        in_key: true,
                    },
                    Column {
                        name: "email".to_owned(),
                        type_name: "text".to_owned(),
                        numeric: false,
                        not_null: false,
                        in_key: false,
                    },
                ],
                key: vec!["id".to_owned()],
                ..Default::default()
            },
        );
        explorer.loaded.insert("db".to_owned(), loaded);
        explorer
    }

    #[test]
    fn every_row_the_tree_would_draw_is_in_order_at_the_right_depth() {
        let rows = lines(&a_tree());
        let shapes: Vec<(usize, String)> =
            rows.iter().map(|line| (line.depth, row_name(line))).collect();
        assert_eq!(
            shapes,
            vec![
                (0, "db".to_owned()),
                (1, "public".to_owned()),
                (2, "tables".to_owned()),
                (3, "users".to_owned()),
                (4, "id".to_owned()),
                (4, "email".to_owned()),
                (2, "routines".to_owned()),
                (3, "get_user".to_owned()),
            ]
        );
    }

    /// A folder the filter's text does not match in anything it holds is not drawn at all — the
    /// reference editor's own `Show Elements | Empty Groups` set to off.
    #[test]
    fn a_folder_with_nothing_matching_the_filter_is_left_out_entirely() {
        let mut explorer = a_tree();
        explorer.filter = "nothing matches this".to_owned();
        let rows = lines(&explorer);
        let names: Vec<String> = rows.iter().map(row_name).collect();
        assert_eq!(names, vec!["db".to_owned(), "public".to_owned()], "no folder had a match");
    }

    /// A folder a filter matched inside is opened even though nobody clicked it, because a filter
    /// nobody can see the results of is a filter that looks broken. The filter is matched against an
    /// item's own name, not its columns.
    #[test]
    fn a_matching_filter_opens_the_folder_it_matched_in() {
        let mut explorer = a_tree();
        explorer.filter = "users".to_owned();
        let rows = lines(&explorer);
        let names: Vec<String> = rows.iter().map(row_name).collect();
        assert!(names.contains(&"tables".to_owned()));
        assert!(names.contains(&"users".to_owned()));
        assert!(!names.contains(&"routines".to_owned()), "nothing in it matched `users`");
    }

    #[test]
    fn a_closed_source_shows_nothing_beneath_it() {
        let mut explorer = a_tree();
        explorer.open_sources.clear();
        let rows = lines(&explorer);
        assert_eq!(rows.len(), 1, "only the source's own row");
    }

    #[test]
    fn only_a_source_a_schema_and_an_item_can_be_aimed_at() {
        assert_eq!(
            aimed_at(&Line {
                depth: 0,
                what: What::Source {
                    name: "db".to_owned(),
                    where_it_points: String::new(),
                    connected: false,
                    open: true,
                    busy: false,
                }
            }),
            Some(Aimed::Source("db".to_owned()))
        );
        assert_eq!(
            aimed_at(&Line {
                depth: 1,
                what: What::Schema {
                    source: "db".to_owned(),
                    name: "public".to_owned(),
                    open: true,
                }
            }),
            Some(Aimed::Schema("db".to_owned(), "public".to_owned()))
        );
        assert_eq!(
            aimed_at(&Line {
                depth: 2,
                what: What::Folder {
                    source: "db".to_owned(),
                    schema: "public".to_owned(),
                    name: "tables".to_owned(),
                    count: 1,
                    open: true,
                }
            }),
            None,
            "a menu with one dimmed row in it is worse than no menu"
        );
        assert_eq!(
            aimed_at(&Line { depth: 2, what: What::Empty { said: "reading…".to_owned() } }),
            None
        );
    }

    #[test]
    fn clicking_a_source_chooses_it_and_toggles_it_open() {
        let line = Line {
            depth: 0,
            what: What::Source {
                name: "db".to_owned(),
                where_it_points: String::new(),
                connected: false,
                open: false,
                busy: false,
            },
        };
        assert_eq!(
            clicked(&line),
            vec![
                Act::Choose("db".to_owned(), String::new(), String::new()),
                Act::ToggleSource("db".to_owned())
            ]
        );
    }

    #[test]
    fn clicking_a_view_chooses_it_but_does_not_toggle_it_open() {
        // A view holds rows, so clicking it also toggles it — but a routine does not, so clicking a
        // routine only chooses it, which is what leaves it with nothing to expand.
        let opens_and_toggles = Line {
            depth: 3,
            what: What::Item {
                source: "db".to_owned(),
                schema: "public".to_owned(),
                name: "users".to_owned(),
                kind: Kind::Table,
                open: false,
                declared: String::new(),
            },
        };
        assert_eq!(
            clicked(&opens_and_toggles).len(),
            2,
            "a table holds rows, so it can be toggled"
        );

        let chooses_only = Line {
            depth: 3,
            what: What::Item {
                source: "db".to_owned(),
                schema: "public".to_owned(),
                name: "get_user".to_owned(),
                kind: Kind::Routine,
                open: false,
                declared: String::new(),
            },
        };
        assert_eq!(
            clicked(&chooses_only),
            vec![Act::Choose("db".to_owned(), "public".to_owned(), "get_user".to_owned())],
            "a routine holds no rows, so there is nothing to toggle"
        );
    }

    #[test]
    fn a_problem_row_is_clicked_to_try_the_connection_again() {
        let line = Line {
            depth: 1,
            what: What::Problem { source: "db".to_owned(), said: "refused".to_owned() },
        };
        assert_eq!(clicked(&line), vec![Act::Connect("db".to_owned())]);
        assert_eq!(row_name(&line), "refused");
    }

    #[test]
    fn opens_answers_for_the_rows_that_can_be_opened_and_nothing_for_the_rest() {
        let table = Line {
            depth: 3,
            what: What::Item {
                source: "db".to_owned(),
                schema: "public".to_owned(),
                name: "users".to_owned(),
                kind: Kind::Table,
                open: true,
                declared: String::new(),
            },
        };
        assert_eq!(opens(&table), Some(true));

        let routine = Line {
            depth: 3,
            what: What::Item {
                source: "db".to_owned(),
                schema: "public".to_owned(),
                name: "get_user".to_owned(),
                kind: Kind::Routine,
                open: true,
                declared: String::new(),
            },
        };
        assert_eq!(opens(&routine), None, "a routine holds no rows, so it cannot be opened either");

        assert_eq!(
            opens(&Line {
                depth: 4,
                what: What::Column {
                    name: "id".to_owned(),
                    type_name: "integer".to_owned(),
                    in_key: true,
                    not_null: true,
                }
            }),
            None
        );
    }

    #[test]
    fn a_column_in_the_key_is_marked_and_every_other_row_kind_has_its_own_mark_or_none() {
        let key_column = Line {
            depth: 4,
            what: What::Column {
                name: "id".to_owned(),
                type_name: "integer".to_owned(),
                in_key: true,
                not_null: true,
            },
        };
        assert!(mark(&key_column).is_some());

        let plain_column = Line {
            depth: 4,
            what: What::Column {
                name: "email".to_owned(),
                type_name: "text".to_owned(),
                in_key: false,
                not_null: false,
            },
        };
        assert!(mark(&plain_column).is_none(), "an ordinary column carries no icon of its own");

        assert!(
            mark(&Line { depth: 2, what: What::Empty { said: "reading…".to_owned() } }).is_none()
        );
    }

    #[test]
    fn a_search_index_is_marked_with_the_magnifier_rather_than_a_table_icon() {
        let table = What::Item {
            source: "db".to_owned(),
            schema: "public".to_owned(),
            name: "docs".to_owned(),
            kind: Kind::Table,
            open: false,
            declared: String::new(),
        };
        let search = What::Item {
            source: "db".to_owned(),
            schema: "public".to_owned(),
            name: "docs".to_owned(),
            kind: Kind::Search,
            open: false,
            declared: String::new(),
        };
        // Compared by address rather than with `assert_ne!`, because two `fn` items are not
        // guaranteed distinct addresses in general but are here — this is the one place in the
        // window that answers "is a search index drawn differently from a table", so it is asked
        // directly rather than through equality on values that merely happen to be function pointers.
        let table_mark = mark(&Line { depth: 3, what: table }).expect("a table has a mark");
        let search_mark =
            mark(&Line { depth: 3, what: search }).expect("a search index has a mark");
        assert!(
            !std::ptr::fn_addr_eq(table_mark, search_mark),
            "a search index does not wear the same mark as an ordinary table"
        );
    }

    #[test]
    fn a_problem_is_tinted_in_the_colour_unluminous_uses_for_something_wrong() {
        let problem = Line {
            depth: 1,
            what: What::Problem { source: "db".to_owned(), said: "refused".to_owned() },
        };
        assert_eq!(tint(&problem), color::unsaved());
    }

    #[test]
    fn a_connected_source_reads_stronger_than_one_that_is_not() {
        let connected = Line {
            depth: 0,
            what: What::Source {
                name: "db".to_owned(),
                where_it_points: String::new(),
                connected: true,
                open: true,
                busy: false,
            },
        };
        let not_connected = Line {
            depth: 0,
            what: What::Source {
                name: "db".to_owned(),
                where_it_points: String::new(),
                connected: false,
                open: true,
                busy: false,
            },
        };
        assert_eq!(tint(&connected), color::text_strong());
        assert_eq!(tint(&not_connected), color::text());
    }

    #[test]
    fn the_schema_a_new_table_goes_in_is_the_sources_first_when_nothing_is_named() {
        let explorer = a_tree();
        assert_eq!(first_schema_of(&explorer, "db"), "public");
        assert_eq!(first_schema_of(&explorer, "nothing-like-this"), "");
    }

    #[test]
    fn only_the_row_that_is_really_chosen_says_so() {
        let mut explorer = a_tree();
        let table = Line {
            depth: 3,
            what: What::Item {
                source: "db".to_owned(),
                schema: "public".to_owned(),
                name: "users".to_owned(),
                kind: Kind::Table,
                open: true,
                declared: String::new(),
            },
        };
        assert!(!is_chosen(&explorer, &table), "nothing is chosen yet");

        explorer.chosen = Some(Chosen {
            source: "db".to_owned(),
            schema: "public".to_owned(),
            name: "users".to_owned(),
        });
        assert!(is_chosen(&explorer, &table));

        let other_table = Line {
            depth: 3,
            what: What::Item {
                source: "db".to_owned(),
                schema: "public".to_owned(),
                name: "orders".to_owned(),
                kind: Kind::Table,
                open: true,
                declared: String::new(),
            },
        };
        assert!(!is_chosen(&explorer, &other_table));

        let schema = Line {
            depth: 1,
            what: What::Schema { source: "db".to_owned(), name: "public".to_owned(), open: true },
        };
        assert!(!is_chosen(&explorer, &schema), "a table is chosen, not the schema it is in");
    }
}
