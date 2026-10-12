//! Drawing the Database plugin: the tree in the pane, and the consoles and grids in the tab.
//!
//! `_agent_output/task-1777-database-plugin/reference/` is what this is measured against — the
//! Database tool window, the query console and the data editor, downloaded from its makers rather than
//! remembered — wearing the dark neumorphic palette the Agent-Tasks board is drawn in.
//!
//! ## The drawing changes nothing
//!
//! Every function here takes a rectangle, draws, and reports an [`Act`]. Not one of them changes a
//! connection, sends a statement or edits a row. [`pane`] and [`tab`] are the two places the acts are
//! applied, after everything has been drawn — which is `components::agent_chat`'s arrangement and the
//! rule `components::activity_bar` set long before either.
//!
//! ## The ground is the window's
//!
//! `show_the_plugin_panes` fills the pane before this is called, so nothing here paints a second ground.
//! What is painted here are the plugin's own surfaces, and since `task-2250` they are `rux` surfaces in
//! the Settings design's language: see `kit`.

pub mod console;
pub mod grid;
pub mod kit;
pub mod modal;
pub mod settings_page;
pub mod tree;
pub mod workspace;

use egui::{Color32, Pos2, Rect, Vec2};

use crate::services::database::{commands, Aimed, DatabaseExplorer, Modal};
use crate::services::plugin_ui::{Look, Request};

/// The gap round a surface, which is the gap the explorer already leaves.
pub const PAD: f32 = 8.0;
/// A toolbar's height, which holds a 24 point icon button with 4 points either side.
pub const TOOLBAR: f32 = 32.0;
/// A corner radius for a card, from the board.
pub const RADIUS: f32 = 12.0;

/// What the drawing reported.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// Open or close a data source's row in the tree.
    ToggleSource(String),
    ToggleSchema(String, String),
    ToggleFolder(String, String, String),
    ToggleTable(String, String, String),
    /// Choose a row, which is what the toolbar's buttons act on.
    Choose(String, String, String),
    /// Open a grid on a table.
    OpenTable(String, String, String),
    OpenConsole(String),
    /// The `New query` button: a console on the data source the page showing is on, or the one the
    /// tree is pointed at.
    NewQuery,
    /// Point a console at another data source, which is the dropdown at the right of its toolbar.
    ConsoleSource(u64, String),
    Connect(String),
    Disconnect(String),
    Refresh(String),
    /// Show the `CREATE` statement for the chosen table.
    Ddl(String, String, String),
    /// The New Data Source modal, on a new one or on an existing one.
    NewSource,
    EditSource(String),
    RemoveSource(String),
    /// The New Table modal, on a source and the schema the tree filed the row under.
    NewTable(String, String),
    /// Drop a table. Asked about first, because it is the one thing on the menu that cannot be
    /// undone.
    DropTable(String, String, String),
    /// Open the tree’s own menu at a point, for whichever row was pressed.
    OpenMenu(Pos2, Aimed),
    /// Put something on the clipboard.
    Copy(String),
    /// Which page of the workspace is showing.
    ShowPage(u64),
    /// Open the vector in a cell, which is what a double click on one does.
    ShowVector(u64, usize, usize),
    ClosePage(u64),
    /// A console.
    Execute(u64),
    Stop(u64),
    /// A grid.
    Reload(u64),
    Page(u64, usize),
    AddRow(u64),
    DeleteRow(u64, usize),
    RevertPending(u64),
    Preview(u64),
    Submit(u64),
    /// A cell was chosen, or typed into.
    ChooseCell(u64, usize, usize),
    /// Open a cell for typing, which is what a double click does.
    EditCell(u64, usize, usize),
    /// What has been typed into the cell that is open, so far.
    TypeIntoCell(u64, String),
    /// Take what has been typed and record it as a pending change.
    CommitCell(u64),
    /// The box has taken the keyboard, so the one shot that asked for it is spent.
    OpenedTheCell(u64),
    /// Close the box and keep nothing.
    CancelCell(u64),
    /// Put NULL in the chosen cell, which an empty box deliberately cannot mean.
    NullTheCell(u64),
    SetCell(u64, usize, usize, unluminous_db::Value),
    /// Sort by a column, which sends a new `ORDER BY` rather than sorting the page.
    SortBy(u64, String),
    /// Say something in the status bar.
    Said(String),
}

/// Draw the pane, and act on what was pressed.
///
/// **The consoles and grids on the left and the data sources on the right** (`task-2250`): *"The db tables
/// should be on the right, and the query console tabs/content should be to the left of it."* The workspace is
/// what somebody opened the pane to read, so it takes the larger share and the reading side of the pane, and
/// the tree is the column beside it that a table is picked out of. [`arrange`] decides where each goes, and a
/// pane too narrow for two columns stacks them instead.
pub fn pane(explorer: &mut DatabaseExplorer, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    let area = ui.available_rect_before_wrap();
    let mut acts = Vec::new();
    if area.width() > 40.0 && area.height() > 40.0 {
        let arranged = arrange(area, look.scale());
        if let Some(workspace_at) = arranged.workspace {
            acts.extend(workspace::show(explorer, ui, look, workspace_at));
        }
        acts.extend(tree::show(explorer, ui, look, arranged.tree));
        // After the rows, so the popup is over them and takes the pointer first — the rule
        // `components::resize_edges` gives for anything added last.
        acts.extend(tree::menu(explorer, ui, look));
    }
    kit::end_frame(ui.ctx());
    apply(explorer, acts)
}

/// Where the tree's card and the workspace go in a pane of this size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arranged {
    /// The data source tree's card.
    pub tree: Rect,
    /// The page strip and the page under it, or `None` when the pane is the tree alone.
    pub workspace: Option<Rect>,
    /// True when the two are side by side, the tree on the right.
    pub side_by_side: bool,
}

/// The room round everything in the pane, which keeps the cards' shadows inside it.
pub const MARGIN: f32 = 14.0;
/// The gap between the workspace and the tree.
pub const GUTTER: f32 = 16.0;
/// How wide a pane has to be before the tree goes beside the workspace rather than above it.
///
/// 560 points, so the 618 point pane this was reported from is two columns: a tree of 220 points and a
/// workspace of about 350 beside it, which holds a strip of page names and a console with its data source
/// chosen at the right.
pub const SIDE_BY_SIDE: f32 = 560.0;
/// How narrow and how wide the tree's column may be, and its share of a pane between the two.
pub const TREE_NARROWEST: f32 = 220.0;
pub const TREE_WIDEST: f32 = 340.0;
const TREE_SHARE_ACROSS: f32 = 0.3;

/// Where the tree and the workspace go.
///
/// **Side by side** once the pane is [`SIDE_BY_SIDE`] wide: the tree a column at the right, between
/// [`TREE_NARROWEST`] and [`TREE_WIDEST`], and the workspace everything to the left of it. **Stacked**
/// below that, with the tree on top taking [`TREE_SHARE`] of the height, because a column of 150 points is
/// a column every table name is cut short in. **The tree alone** below `TREE_ALONE`, because a console in
/// eighty points of height is a console nobody can read a result in.
pub fn arrange(area: Rect, scale: f32) -> Arranged {
    let inner = area.shrink(MARGIN * scale);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return Arranged { tree: area, workspace: None, side_by_side: false };
    }
    if area.width() >= SIDE_BY_SIDE * scale && area.height() >= 200.0 * scale {
        let width = (inner.width() * TREE_SHARE_ACROSS)
            .clamp(TREE_NARROWEST * scale, TREE_WIDEST * scale)
            .min(inner.width() * 0.5);
        let tree = Rect::from_min_max(Pos2::new(inner.right() - width, inner.top()), inner.max);
        let workspace =
            Rect::from_min_max(inner.min, Pos2::new(tree.left() - GUTTER * scale, inner.bottom()));
        return Arranged { tree, workspace: Some(workspace), side_by_side: true };
    }
    if area.height() >= TREE_ALONE * scale {
        let tree_height =
            (inner.height() * TREE_SHARE).clamp(120.0 * scale, inner.height() - 160.0 * scale);
        let tree = Rect::from_min_size(inner.min, Vec2::new(inner.width(), tree_height));
        let workspace =
            Rect::from_min_max(Pos2::new(inner.left(), tree.bottom() + GUTTER * scale), inner.max);
        return Arranged { tree, workspace: Some(workspace), side_by_side: false };
    }
    Arranged { tree: inner, workspace: None, side_by_side: false }
}

/// How much of a stacked pane the tree takes, and the height below which the pane is the tree alone.
///
/// A third, so the consoles and the grids get the two thirds they need: a result is the thing somebody
/// opened the pane to read, and the tree is how they got to it.
const TREE_SHARE: f32 = 0.34;
const TREE_ALONE: f32 = 360.0;

/// Draw the workspace tab, and act on what was pressed.
pub fn tab(explorer: &mut DatabaseExplorer, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    let area = ui.available_rect_before_wrap();
    let mut acts = Vec::new();
    if area.width() > 80.0 && area.height() > 60.0 {
        acts = workspace::show(explorer, ui, look, area);
    }
    apply(explorer, acts)
}

/// The one place an act becomes a change.
pub fn apply(explorer: &mut DatabaseExplorer, acts: Vec<Act>) -> Vec<Request> {
    let mut requests = Vec::new();
    // One of the two doors the tree's rows change through -- see `DatabaseExplorer::revision`.
    if !acts.is_empty() {
        explorer.changed();
    }
    for act in acts {
        match act {
            Act::ToggleSource(name) => explorer.toggle_source(&name),
            Act::ToggleSchema(source, schema) => explorer.toggle_schema(&source, &schema),
            Act::ToggleFolder(source, schema, folder) => {
                explorer.toggle_folder(&source, &schema, &folder)
            }
            Act::ToggleTable(source, schema, name) => {
                explorer.toggle_table(&source, &schema, &name)
            }
            Act::Choose(source, schema, name) => {
                explorer.chosen = Some(crate::services::database::Chosen { source, schema, name });
            }
            Act::OpenTable(source, schema, name) => {
                if let Err(why) = explorer.open_table(&source, &schema, &name) {
                    requests.push(Request::Message(why));
                }
                requests.push(Request::ShowTab);
            }
            Act::OpenConsole(source) => match explorer.open_console(&source) {
                Ok(_) => requests.push(Request::ShowTab),
                Err(why) => requests.push(Request::Message(why)),
            },
            Act::NewQuery => match explorer.source_for_a_new_query() {
                Some(source) => match explorer.open_console(&source) {
                    Ok(_) => requests.push(Request::ShowTab),
                    Err(why) => requests.push(Request::Message(why)),
                },
                // Nothing to query yet, so the button does the one thing that would give it something.
                None => explorer.modal = Some(Modal::Source(commands::a_new_source(explorer))),
            },
            Act::ConsoleSource(id, source) => {
                if let Err(why) = explorer.point_the_console_at(id, &source) {
                    requests.push(Request::Message(why));
                }
            }
            Act::Connect(name) => {
                if let Err(why) = explorer.connect(&name) {
                    requests.push(Request::Message(why));
                }
            }
            Act::Disconnect(name) => explorer.disconnect(&name),
            Act::Refresh(name) => {
                // A refresh is a disconnect and a reconnect, which is the honest form of "read it
                // again": every cached schema, item list and column list came down that connection.
                let was_open = explorer.open_sources.contains(&name);
                explorer.disconnect(&name);
                if was_open {
                    if let Err(why) = explorer.connect(&name) {
                        requests.push(Request::Message(why));
                    }
                }
            }
            Act::Ddl(source, schema, name) => {
                let kind = kind_of(explorer, &source, &schema, &name);
                if let Err(why) = explorer.ask_for_ddl(&source, &schema, &name, kind, true) {
                    requests.push(Request::Message(why));
                }
            }
            Act::NewSource => {
                explorer.modal = Some(Modal::Source(commands::a_new_source(explorer)))
            }
            Act::EditSource(name) => {
                if let Some(source) = explorer.configuration.source(&name) {
                    explorer.modal = Some(Modal::Source(commands::a_form_for(source)));
                }
            }
            Act::RemoveSource(name) => {
                if let Err(why) = explorer.remove_source(&name) {
                    requests.push(Request::Message(why));
                }
            }
            Act::NewTable(source, schema) => {
                explorer.modal =
                    Some(Modal::NewTable(commands::a_new_table(explorer, &source, &schema)));
            }
            Act::DropTable(source, schema, name) => {
                let sql = match unluminous_db::sql::drop_table(&schema, &name) {
                    Ok(sql) => sql,
                    Err(why) => {
                        requests.push(Request::Message(why));
                        continue;
                    }
                };
                match explorer.run_the_ddl(&source, &schema, &sql) {
                    Ok(_) => requests.push(Request::Message(format!("`{name}` is gone"))),
                    Err(why) => requests.push(Request::Message(why)),
                }
            }
            Act::OpenMenu(at, aimed) => explorer.menu = Some((at, aimed)),
            Act::Copy(text) => requests.push(Request::Copy(text)),
            Act::ShowVector(id, row, column) => {
                // Read from the cell rather than carried on the act, so what opens is what the grid
                // is drawing at the moment it is asked - the same rule the DDL modal keeps.
                if let Some(grid) = explorer.grid(id) {
                    let name = grid.rows.columns.get(column).map(|column| column.name.clone());
                    let (value, _) = grid.cell(row, column);
                    let title = match name {
                        Some(name) => format!("{} row {}", name, row + 1),
                        None => format!("row {}", row + 1),
                    };
                    if let Some(vector) = value.bytes().and_then(unluminous_db::Vector::decode) {
                        explorer.modal = Some(Modal::Vector { title, vector });
                    }
                }
            }
            Act::ShowPage(id) => {
                if let Some(at) = explorer.pages.iter().position(|page| page.id == id) {
                    explorer.current = at;
                }
            }
            Act::ClosePage(id) => explorer.close_page(id),
            Act::Execute(id) => match explorer.execute(id) {
                Ok(said) => requests.push(Request::Message(said)),
                Err(why) => requests.push(Request::Message(why)),
            },
            Act::Stop(id) => {
                if let Err(why) = explorer.stop(id) {
                    requests.push(Request::Message(why));
                }
            }
            Act::Reload(id) => explorer.reload(id),
            Act::Page(id, at) => {
                if let Some(grid) = explorer.grid_mut(id) {
                    grid.at = at;
                }
                explorer.reload(id);
            }
            Act::AddRow(id) => with_grid(explorer, id, |grid| {
                grid.pending.add();
                // Straight into the first cell of it, because a row that appears and then has to be
                // found and double clicked is a row that reads as having done nothing.
                let at = grid.row_count() - 1;
                grid.chosen = Some((at, 0));
                grid.editing = Some(crate::services::database::Editing {
                    at,
                    column: 0,
                    text: String::new(),
                    opened: true,
                });
            }),
            Act::DeleteRow(id, at) => with_grid(explorer, id, |grid| {
                if let Some(row) = grid.row_of(at) {
                    grid.pending.delete(row);
                }
            }),
            Act::RevertPending(id) => with_grid(explorer, id, |grid| grid.pending.clear()),
            Act::Preview(id) => explorer.modal = Some(Modal::Preview { page: id }),
            Act::Submit(id) => {
                if let Err(why) = explorer.submit(id) {
                    requests.push(Request::Message(why));
                }
            }
            Act::ChooseCell(id, at, column) => with_grid(explorer, id, |grid| {
                grid.chosen = Some((at, column));
                grid.editing = None;
            }),
            Act::EditCell(id, at, column) => with_grid(explorer, id, |grid| {
                grid.chosen = Some((at, column));
                grid.editing = Some(crate::services::database::Editing {
                    at,
                    column,
                    text: grid.text_of(at, column),
                    opened: true,
                });
            }),
            Act::TypeIntoCell(id, text) => with_grid(explorer, id, |grid| {
                if let Some(editing) = grid.editing.as_mut() {
                    editing.text = text;
                }
            }),
            Act::CommitCell(id) => with_grid(explorer, id, |grid| {
                let Some(editing) = grid.editing.take() else { return };
                let Some(name) =
                    grid.rows.columns.get(editing.column).map(|column| column.name.clone())
                else {
                    return;
                };
                let Some(row) = grid.row_of(editing.at) else { return };
                // **A box nobody typed in records nothing.** Committing happens when the keyboard
                // moves on, so every cell an added row is opened at and stepped over would otherwise
                // be written down as an edit that changes nothing — and on an added row that is
                // worse than noise: it turns *not supplied* into the empty string, which SQLite
                // refuses outright on an `integer primary key` with `datatype mismatch`. Pressing
                // `Add row` lands on the first cell, so this was every added row with a generated
                // key. `task-1795`.
                if editing.text == grid.text_of(editing.at, editing.column) {
                    return;
                }
                // An empty box on a cell that was NULL leaves it NULL, and on any other cell means
                // the empty string. The two cannot both be what an empty box means, and this is the
                // half a person can see: `Set NULL` on the toolbar is the other.
                let (was, _) = grid.cell(editing.at, editing.column);
                let value = match editing.text.is_empty() && was.is_null() {
                    true => unluminous_db::Value::Null,
                    false => unluminous_db::Value::Text(editing.text.clone()),
                };
                grid.pending.set(row, &name, value);
            }),
            Act::OpenedTheCell(id) => with_grid(explorer, id, |grid| {
                if let Some(editing) = grid.editing.as_mut() {
                    editing.opened = false;
                }
            }),
            Act::CancelCell(id) => with_grid(explorer, id, |grid| grid.editing = None),
            Act::NullTheCell(id) => with_grid(explorer, id, |grid| {
                let Some((at, column)) = grid.chosen else { return };
                let Some(name) = grid.rows.columns.get(column).map(|column| column.name.clone())
                else {
                    return;
                };
                if let Some(row) = grid.row_of(at) {
                    grid.pending.set(row, &name, unluminous_db::Value::Null);
                }
                grid.editing = None;
            }),
            Act::SetCell(id, at, column, value) => with_grid(explorer, id, |grid| {
                let Some(name) = grid.rows.columns.get(column).map(|column| column.name.clone())
                else {
                    return;
                };
                if let Some(row) = grid.row_of(at) {
                    grid.pending.set(row, &name, value);
                }
            }),
            Act::SortBy(id, column) => {
                with_grid(explorer, id, |grid| {
                    // Click once for ascending, again for descending, again for none — which is what
                    // every grid does and what the chevron in the header draws.
                    let ascending = format!("{column} asc");
                    let descending = format!("{column} desc");
                    grid.order_by = match grid.order_by.trim() {
                        was if was == ascending => descending,
                        was if was == descending => String::new(),
                        _ => ascending,
                    };
                    grid.at = 0;
                });
                explorer.reload(id);
            }
            Act::Said(said) => requests.push(Request::Message(said)),
        }
    }
    requests
}

fn with_grid(
    explorer: &mut DatabaseExplorer,
    id: u64,
    work: impl FnOnce(&mut crate::services::database::Grid),
) {
    if let Some(grid) = explorer.grid_mut(id) {
        work(grid);
    }
}

/// What kind of thing the tree says this is, defaulting to a table.
fn kind_of(
    explorer: &DatabaseExplorer,
    source: &str,
    schema: &str,
    name: &str,
) -> unluminous_db::Kind {
    explorer
        .loaded
        .get(source)
        .and_then(|loaded| loaded.items.get(schema))
        .and_then(|items| items.iter().find(|item| item.name == name))
        .map(|item| item.kind)
        .unwrap_or(unluminous_db::Kind::Table)
}

/// A raised card in the Settings design's values. See `kit`.
///
/// The colour of the surface is the design's card gradient, worked out from the theme, rather than a
/// colour the caller chooses: every card in the pane is the same card.
pub fn card(ui: &mut egui::Ui, look: &Look<'_>, area: Rect, radius: f32) {
    kit::card(ui, look.scale(), area, radius);
}

/// A carved well: a field, a results area.
pub fn well(ui: &mut egui::Ui, look: &Look<'_>, area: Rect, radius: f32) {
    kit::well(ui, look.scale(), area, radius);
}

/// Text at a position, vertically centred in a row, cut to a width with an ellipsis.
///
/// Every list in this plugin draws its rows with it, so a long table name is cut the same way
/// everywhere rather than being clipped in one place and overflowing in another.
pub fn text(
    painter: &egui::Painter,
    at: Pos2,
    words: &str,
    tint: Color32,
    size: f32,
    width: f32,
) -> f32 {
    let mut galley =
        painter.layout_no_wrap(words.to_owned(), egui::FontId::proportional(size), tint);
    if galley.size().x > width && width > 12.0 {
        // Cut by characters rather than by bytes, so a name with an accent in it is not cut in half.
        let mut kept: String = words.to_owned();
        while !kept.is_empty() {
            kept.pop();
            let shorter = format!("{kept}…");
            galley = painter.layout_no_wrap(shorter, egui::FontId::proportional(size), tint);
            if galley.size().x <= width {
                break;
            }
        }
    }
    let height = galley.size().y;
    let width_drawn = galley.size().x;
    painter.galley(Pos2::new(at.x, at.y - height / 2.0), galley, tint);
    width_drawn
}

/// The same in the code font, for a value, a type or a statement.
pub fn code(
    painter: &egui::Painter,
    at: Pos2,
    words: &str,
    tint: Color32,
    size: f32,
    width: f32,
) -> f32 {
    let mut galley = painter.layout_no_wrap(words.to_owned(), egui::FontId::monospace(size), tint);
    if galley.size().x > width && width > 12.0 {
        let per = galley.size().x / words.chars().count().max(1) as f32;
        let fits = ((width - per) / per).max(0.0) as usize;
        let kept: String = words.chars().take(fits).collect();
        galley = painter.layout_no_wrap(format!("{kept}…"), egui::FontId::monospace(size), tint);
    }
    let height = galley.size().y;
    let drawn = galley.size().x;
    painter.galley(Pos2::new(at.x, at.y - height / 2.0), galley, tint);
    drawn
}

/// A spinner: three dots that fill in, for a source that is still answering.
///
/// Drawn rather than animated with a rotation, because the decoration canvas is only rasterised on a
/// changed frame and a spinning arc would rasterise on every one. `egui` paints these on top.
pub fn waiting(painter: &egui::Painter, centre: Pos2, tint: Color32, time: f64) {
    for index in 0..3 {
        let phase = (time * 2.0 + index as f64 * 0.3).sin() as f32;
        let alpha = (0.35 + 0.65 * (phase * 0.5 + 0.5)).clamp(0.0, 1.0);
        painter.circle_filled(
            Pos2::new(centre.x - 5.0 + index as f32 * 5.0, centre.y),
            1.6,
            tint.gamma_multiply(alpha),
        );
    }
}

/// The size of a rectangle, laid out left to right, for a toolbar.
pub fn along(area: Rect, at: &mut f32, width: f32) -> Rect {
    let rect = Rect::from_min_size(Pos2::new(*at, area.top()), Vec2::new(width, area.height()));
    *at += width;
    rect
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database::Chosen;

    #[test]
    fn along_lays_out_left_to_right_and_moves_the_pen_on() {
        let toolbar = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(300.0, 32.0));
        let mut at = toolbar.left();
        let first = along(toolbar, &mut at, 24.0);
        assert_eq!(first, Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(24.0, 32.0)));
        assert_eq!(at, 34.0);
        let second = along(toolbar, &mut at, 40.0);
        assert_eq!(second, Rect::from_min_size(Pos2::new(34.0, 20.0), Vec2::new(40.0, 32.0)));
        assert_eq!(at, 74.0);
    }

    /// Wide, the tree is a column at the right and the workspace everything left of it; narrow, the tree
    /// is on top; short, the tree is alone. `task-2250`.
    #[test]
    fn a_wide_pane_puts_the_tree_at_the_right_of_the_workspace() {
        let wide = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(760.0, 600.0));
        let arranged = arrange(wide, 1.0);
        let workspace = arranged.workspace.expect("a workspace beside the tree");
        assert!(arranged.side_by_side);
        assert!(workspace.right() < arranged.tree.left(), "{workspace:?} then {:?}", arranged.tree);
        assert_eq!(workspace.top(), arranged.tree.top(), "the two columns start together");
        assert!(arranged.tree.width() >= TREE_NARROWEST && arranged.tree.width() <= TREE_WIDEST);
        assert!(workspace.width() > arranged.tree.width(), "the workspace is the larger share");
        assert!(wide.contains_rect(workspace) && wide.contains_rect(arranged.tree));

        let narrow = Rect::from_min_size(Pos2::ZERO, Vec2::new(420.0, 600.0));
        let arranged = arrange(narrow, 1.0);
        let workspace = arranged.workspace.expect("a workspace under the tree");
        assert!(!arranged.side_by_side);
        assert!(arranged.tree.bottom() < workspace.top());

        let short = Rect::from_min_size(Pos2::ZERO, Vec2::new(420.0, 300.0));
        assert_eq!(
            arrange(short, 1.0).workspace,
            None,
            "a console in this little height is unreadable"
        );
    }

    /// The arrangement is measured in the pane's own points, so a zoomed pane needs more room before it
    /// splits into two columns.
    #[test]
    fn a_zoomed_pane_needs_more_room_for_two_columns() {
        let pane = Rect::from_min_size(Pos2::ZERO, Vec2::new(700.0, 700.0));
        assert!(arrange(pane, 1.0).side_by_side);
        assert!(!arrange(pane, 1.5).side_by_side, "560 points at 1.5 is 840");
    }

    #[test]
    fn a_name_the_tree_has_not_read_yet_defaults_to_a_table() {
        let mut explorer = DatabaseExplorer::new();
        let mut loaded = crate::services::database::Loaded::default();
        loaded.items.insert(
            "public".to_owned(),
            vec![unluminous_db::Item {
                name: "docs".to_owned(),
                kind: unluminous_db::Kind::Search,
            }],
        );
        explorer.loaded.insert("db".to_owned(), loaded);
        assert_eq!(kind_of(&explorer, "db", "public", "docs"), unluminous_db::Kind::Search);
        assert_eq!(
            kind_of(&explorer, "db", "public", "nothing-like-this"),
            unluminous_db::Kind::Table
        );
        assert_eq!(
            kind_of(&explorer, "no-such-source", "public", "docs"),
            unluminous_db::Kind::Table
        );
    }

    #[test]
    fn toggling_an_unknown_source_records_the_refusal_rather_than_panicking() {
        let mut explorer = DatabaseExplorer::new();
        let requests = apply(&mut explorer, vec![Act::ToggleSource("missing".to_owned())]);
        assert!(requests.is_empty(), "toggling records the problem on the explorer, not a request");
        assert!(
            explorer.open_sources.contains("missing"),
            "it opens even though it could not connect"
        );
        assert!(explorer.problem.is_some(), "why it could not connect is kept somewhere");
    }

    #[test]
    fn connecting_or_removing_a_source_that_is_not_there_is_a_message_not_a_panic() {
        let mut explorer = DatabaseExplorer::new();
        assert_eq!(
            apply(&mut explorer, vec![Act::Connect("missing".to_owned())]),
            vec![Request::Message("there is no data source called `missing`.".to_owned())]
        );
        assert_eq!(
            apply(&mut explorer, vec![Act::RemoveSource("missing".to_owned())]),
            vec![Request::Message("there is no data source called `missing`.".to_owned())]
        );
    }

    #[test]
    fn choosing_a_row_sets_what_the_tree_and_a_new_console_point_at() {
        let mut explorer = DatabaseExplorer::new();
        let requests = apply(
            &mut explorer,
            vec![Act::Choose("db".to_owned(), "public".to_owned(), "users".to_owned())],
        );
        assert!(requests.is_empty());
        assert_eq!(
            explorer.chosen,
            Some(Chosen {
                source: "db".to_owned(),
                schema: "public".to_owned(),
                name: "users".to_owned()
            })
        );
    }

    #[test]
    fn opening_the_trees_menu_and_copying_text_both_reach_the_explorer() {
        let mut explorer = DatabaseExplorer::new();
        let at = Pos2::new(4.0, 8.0);
        let aimed = Aimed::Source("db".to_owned());
        let requests = apply(&mut explorer, vec![Act::OpenMenu(at, aimed.clone())]);
        assert!(requests.is_empty());
        assert_eq!(explorer.menu, Some((at, aimed)));

        let requests = apply(&mut explorer, vec![Act::Copy("a value".to_owned())]);
        assert_eq!(requests, vec![Request::Copy("a value".to_owned())]);
    }
}
