//! Where a canvas is written down, and what it reads back as.
//!
//! `task-1904`: *"I should be able to have multiple projects/views that are saved on edit, and
//! restored on project open. Positions of nodes, zoom level, state, terminal session, agent session,
//! etc should all be retained and restored."*
//!
//! It is `.unluminous/space.conf` **inside the project**, beside the three files
//! `services::project_state` already writes and for the same reason: copying the project copies its
//! canvases, and two people on one folder do not fight over one file in somebody's settings.
//!
//! The format is the key and value store the settings file uses, with a list numbered the way
//! `run-configurations.conf` numbers one:
//!
//! ```text
//! space.current          = 1
//! space.view.0.id        = 1
//! space.view.0.name      = Main
//! space.view.0.camera.x  = -240
//! space.view.0.node.0.id = 7
//! space.view.0.node.0.kind = terminal
//! space.view.0.edge.0.from = 7
//! ```
//!
//! Three rules it keeps, each of them one `project_state` already keeps.
//!
//! **Paths are written relative to the project** wherever they are inside it, so a project that moves
//! still opens the canvas it was left with.
//!
//! **A file that cannot be read is a file that is not there.** A project that opens with an empty
//! canvas is better than a project that will not open, so every value is defended on the way in and a
//! node whose kind this version does not know is dropped rather than refused.
//!
//! **What a program was doing cannot be brought back.** A terminal node comes back as a fresh session
//! running the same command in the same folder, which is exactly what `project_state` promises for the
//! terminal tile. What is added is the **session id** an agent named, so a node can offer to resume the
//! conversation rather than only the program.

use std::path::{Path, PathBuf};

use egui::{Pos2, Vec2};

use crate::services::project_state;
use crate::services::store::Values;

use super::node::{
    Browser, Camera, Chat, Edge, Editor, Folder, Kind, Node, Pipe, State, Tasks, Terminal,
};
use super::{Space, View};

/// The file inside `.unluminous`.
pub const FILE: &str = "space.conf";

/// How many views and how many nodes a view are believed.
///
/// A sanity bound on a hand edited or corrupted file, the way `PANEL_MAX_WIDTH` is one on a width. A
/// project with more canvases than this has a problem the state file will not fix.
const VIEW_LIMIT: usize = 64;
const NODE_LIMIT: usize = 256;

/// Where the file is.
pub fn path(root: &Path) -> std::path::PathBuf {
    project_state::folder(root).join(FILE)
}

/// Read a project's canvases. A project with none gets a fresh one, which is one empty view.
pub fn load(root: &Path) -> Space {
    let Ok(text) = std::fs::read_to_string(path(root)) else { return Space::new() };
    read(&Values::parse(&text), root)
}

/// Write a project's canvases, making `.unluminous` if it is not there.
///
/// **It says whether it wrote.** A write that quietly failed and was treated as a write is a canvas
/// somebody arranged and lost: nothing marks it as needing writing again, so the next thing to happen
/// is the window closing. Found by the `task-1904` review.
pub fn save(root: &Path, space: &Space) -> Result<(), String> {
    let folder = project_state::folder(root);
    std::fs::create_dir_all(&folder)
        .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
    let mut values = Values::new();
    write(space, root, &mut values);
    let file = path(root);
    crate::services::store::write_atomically(
        &file,
        values.to_text_headed("Unluminous: the Base of Infinite Space in this project.").as_bytes(),
    )
    .map_err(|problem| format!("{} could not be written: {problem}", file.display()))
}

/// Turn a canvas into values.
pub fn write(space: &Space, root: &Path, values: &mut Values) {
    values.set("space.current", space.current_id().to_string());
    for (at, view) in space.views().iter().take(VIEW_LIMIT).enumerate() {
        write_a_view(view, at, root, values);
    }
}

/// One view, its nodes and its edges.
fn write_a_view(view: &View, at: usize, root: &Path, values: &mut Values) {
    let key = format!("space.view.{at}");
    values.set(&format!("{key}.id"), view.id.to_string());
    values.set(&format!("{key}.name"), view.name.clone());
    values.set(&format!("{key}.camera.x"), format!("{:.1}", view.camera.at.x));
    values.set(&format!("{key}.camera.y"), format!("{:.1}", view.camera.at.y));
    values.set(&format!("{key}.camera.zoom"), format!("{:.3}", view.camera.zoom));
    // **Which node was chosen**, because the first key press after a project opens has to have
    // somewhere to go. `task-1914`: *"In base of infinite space, i cant type in a terminal."* A canvas
    // that came back with nothing chosen answered every key with nothing, on a window whose editing
    // area may not even be showing — and to a person that reads as the node being broken.
    if let Some(chosen) = view.chosen {
        values.set(&format!("{key}.chosen"), chosen.to_string());
    }
    for (index, node) in view.nodes.iter().take(NODE_LIMIT).enumerate() {
        write_a_node(node, &format!("{key}.node.{index}"), root, values);
    }
    for (index, edge) in view.edges.iter().enumerate() {
        let key = format!("{key}.edge.{index}");
        values.set(&format!("{key}.from"), edge.from.to_string());
        values.set(&format!("{key}.to"), edge.to.to_string());
        values.set(&format!("{key}.pipe"), edge.pipe.name());
    }
}

/// One node: where it is, how big it is, and whatever its kind has to remember.
fn write_a_node(node: &Node, key: &str, root: &Path, values: &mut Values) {
    values.set(&format!("{key}.id"), node.id.to_string());
    values.set(&format!("{key}.kind"), node.kind().name());
    values.set(&format!("{key}.x"), format!("{:.1}", node.at.x));
    values.set(&format!("{key}.y"), format!("{:.1}", node.at.y));
    values.set(&format!("{key}.width"), format!("{:.1}", node.size.x));
    values.set(&format!("{key}.height"), format!("{:.1}", node.size.y));
    values.set_or_clear(&format!("{key}.title"), &node.title);
    match &node.state {
        State::Terminal(terminal) => {
            values.set_or_clear(&format!("{key}.command"), &terminal.command);
            values.set_or_clear(&format!("{key}.session"), &terminal.session);
            // **What was running, which is not the command.** `task-1907`: a node's command is what it was
            // given, and a person types `claude` into a plain shell — so a canvas that wrote only the command
            // came back as a shell whatever had been running in it.
            values.set_or_clear(&format!("{key}.running"), &terminal.running);
            if terminal.font_size > 0.0 {
                values.set(&format!("{key}.font"), format!("{:.0}", terminal.font_size));
            }
            if let Some(folder) = &terminal.folder {
                values.set(&format!("{key}.folder"), written(root, folder));
            }
        }
        // `Browser::typed` is deliberately not written: a half-typed address is not state a project
        // should come back with. `task-1905`.
        State::Browser(browser) => values.set_or_clear(&format!("{key}.url"), &browser.url),
        State::Folder(folder) => {
            if let Some(at) = &folder.root {
                values.set(&format!("{key}.root"), written(root, at));
            }
            write_a_list(values, &format!("{key}.expanded"), &folder.expanded, root);
            if (folder.zoom - 1.0).abs() > 0.001 {
                values.set(&format!("{key}.zoom"), format!("{:.2}", folder.zoom));
            }
            if folder.scroll > 0.5 {
                values.set(&format!("{key}.scroll"), format!("{:.1}", folder.scroll));
            }
        }
        State::Editor(editor) => {
            // **Every tab, `|` separated**, which is `Folder::expanded`'s own shape and `open-files.txt`'s.
            // `task-1906`: a node holds a strip of tabs since `task-1905`, and one path brought one back.
            write_a_list(values, &format!("{key}.paths"), &editor.paths, root);
            if editor.showing > 0 {
                values.set(&format!("{key}.showing"), editor.showing.to_string());
            }
            values.set(&format!("{key}.caret"), editor.caret.to_string());
            values.set(&format!("{key}.scroll"), format!("{:.1}", editor.scroll));
            if editor.font_size > 0.0 {
                values.set(&format!("{key}.font"), format!("{:.0}", editor.font_size));
            }
        }
        State::Chat(chat) => {
            // **Which conversation, so each agent comes back on its own.** The pane reopens the newest
            // because there is one of it; a canvas of chats that all reopened the newest would be several
            // views of one conversation, which is the thing `Kind::Chat` exists not to be.
            values.set_or_clear(&format!("{key}.conversation"), &chat.conversation);
            if (chat.zoom - 1.0).abs() > 0.001 {
                values.set(&format!("{key}.zoom"), format!("{:.2}", chat.zoom));
            }
        }
        State::Tasks(tasks) => {
            if (tasks.zoom - 1.0).abs() > 0.001 {
                values.set(&format!("{key}.zoom"), format!("{:.2}", tasks.zoom));
            }
        }
    }
}

/// How much bigger or smaller a node was left drawing, or 1.0 when it was never changed.
///
/// A zoom a person set is worth coming back with — it is how big they wanted this node — and the three
/// kinds that keep a multiplier rather than a point size all read it the same way.
fn read_a_zoom(values: &Values, key: &str) -> f32 {
    match values.number(&format!("{key}.zoom")).unwrap_or(0.0) {
        asked if asked > 0.0 => asked,
        _ => 1.0,
    }
}

/// A list of paths, one numbered key each.
///
/// **A `|` is a legal character in a filename.** Both of these lists were one value with the paths joined by
/// `|`, which is the shape `Folder::expanded` arrived with — and `/project/a|b.rs` then came back as two paths
/// that do not exist, so a node lost a tab and a tree lost an open folder. The Codex Sol review found it.
///
/// Numbered keys are the shape this repository already writes a list in: `run-configurations.conf` numbers its
/// configurations `run.N.*` and `files.panes` numbers its panes, precisely so no character in a value can be
/// the separator. `count` is written as well, so the reader stops where the writer stopped rather than at the
/// first gap.
fn write_a_list(values: &mut Values, key: &str, paths: &[std::path::PathBuf], root: &Path) {
    values.set(&format!("{key}.count"), paths.len().to_string());
    for (index, path) in paths.iter().enumerate() {
        values.set(&format!("{key}.{index}"), written(root, path));
    }
}

/// The paths under a numbered list, or the older `|` separated value where a file was written by an earlier
/// version.
///
/// The fallback is the rule `Layout::read_from` keeps about a settings file written before the panels could be
/// moved: a `space.conf` on somebody's disk goes on opening. It is only read when there is no `count`, so a
/// list that is genuinely empty is not filled in from a stale value.
fn read_a_list(values: &Values, key: &str, root: &Path) -> Vec<std::path::PathBuf> {
    match values.number(&format!("{key}.count")) {
        Some(many) => (0..many.max(0.0) as usize)
            .filter_map(|index| values.text(&format!("{key}.{index}")))
            .filter(|part| !part.trim().is_empty())
            .map(|part| project_state::absolute(root, Path::new(part)))
            .collect(),
        None => values
            .text(key)
            .unwrap_or_default()
            .split('|')
            .filter(|part| !part.trim().is_empty())
            .map(|part| project_state::absolute(root, Path::new(part)))
            .collect(),
    }
}

/// A path as it is written down: relative to the project when it is inside it, with `/` separators so
/// the file reads the same on both platforms.
fn written(root: &Path, path: &Path) -> String {
    project_state::relative(root, path).display().to_string().replace('\\', "/")
}

/// Read a canvas back out of values.
///
/// Everything is defended: a view with no id is skipped, a node of a kind this version has not got is
/// dropped, and `Space::tidy` takes away an edge naming a node that is not there. A project that opens
/// with an empty canvas is better than one that will not open.
pub fn read(values: &Values, root: &Path) -> Space {
    let mut space = Space::new();
    let mut views: Vec<View> = Vec::new();
    for at in 0..VIEW_LIMIT {
        let key = format!("space.view.{at}");
        let Some(id) = values.number(&format!("{key}.id")).map(|id| id as u64) else { continue };
        if id == 0 {
            continue;
        }
        views.push(read_a_view(values, &key, id, root));
    }
    if views.is_empty() {
        return space;
    }
    let current = values.number("space.current").map(|id| id as u64).unwrap_or(views[0].id);
    space.adopt(views, current);
    space
}

/// One view, with its nodes and edges.
fn read_a_view(values: &Values, key: &str, id: u64, root: &Path) -> View {
    let name = values.text(&format!("{key}.name")).unwrap_or("View").to_owned();
    let camera = Camera {
        at: Pos2::new(
            values.number(&format!("{key}.camera.x")).unwrap_or(0.0),
            values.number(&format!("{key}.camera.y")).unwrap_or(0.0),
        ),
        zoom: values
            .number(&format!("{key}.camera.zoom"))
            .unwrap_or(1.0)
            .clamp(super::node::MIN_ZOOM, super::node::MAX_ZOOM),
    };
    let mut nodes = Vec::new();
    for index in 0..NODE_LIMIT {
        if let Some(node) = read_a_node(values, &format!("{key}.node.{index}"), root) {
            nodes.push(node);
        }
    }
    let mut edges = Vec::new();
    for index in 0..NODE_LIMIT * 2 {
        let key = format!("{key}.edge.{index}");
        let (Some(from), Some(to)) = (
            values.number(&format!("{key}.from")).map(|id| id as u64),
            values.number(&format!("{key}.to")).map(|id| id as u64),
        ) else {
            continue;
        };
        let pipe =
            values.text(&format!("{key}.pipe")).and_then(Pipe::from_name).unwrap_or_default();
        // The id is not written down: an edge is named by the two nodes it joins, and a fresh number
        // is handed out below by `Space::adopt`, which is also what stops a hand written file giving
        // two edges one id.
        edges.push(Edge { id: 0, from, to, pipe });
    }
    // A node that is not on this canvas any more is not chosen. `Space::tidy` would not catch it:
    // it takes away an edge naming a node that has gone and says nothing about the choice.
    let chosen = values
        .number(&format!("{key}.chosen"))
        .map(|id| id as u64)
        .filter(|id| nodes.iter().any(|node| node.id == *id));
    View { id, name, nodes, edges, camera, chosen }
}

/// One node, or nothing when its kind is one this version has not got.
fn read_a_node(values: &Values, key: &str, root: &Path) -> Option<Node> {
    let id = values.number(&format!("{key}.id")).map(|id| id as u64)?;
    let kind = Kind::from_name(values.text(&format!("{key}.kind"))?.trim())?;
    let at = Pos2::new(
        values.number(&format!("{key}.x")).unwrap_or(0.0),
        values.number(&format!("{key}.y")).unwrap_or(0.0),
    );
    let smallest = kind.smallest();
    let size = Vec2::new(
        values.number(&format!("{key}.width")).unwrap_or(kind.opens_at().x).max(smallest.x),
        values.number(&format!("{key}.height")).unwrap_or(kind.opens_at().y).max(smallest.y),
    );
    let title = values.text(&format!("{key}.title")).unwrap_or_default().to_owned();
    let state = match kind {
        Kind::Terminal => State::Terminal(Terminal {
            command: values.text(&format!("{key}.command")).unwrap_or_default().to_owned(),
            folder: values
                .text(&format!("{key}.folder"))
                .map(|folder| project_state::absolute(root, Path::new(folder))),
            font_size: values.number(&format!("{key}.font")).unwrap_or(0.0).max(0.0),
            session: values.text(&format!("{key}.session")).unwrap_or_default().to_owned(),
            // Absent in a `space.conf` written before `task-1907`, which reads as a node that was at a prompt
            // — the same thing `Layout::read_from` does for a settings file written before the panels could be
            // moved, and what makes an existing canvas open unchanged.
            running: values.text(&format!("{key}.running")).unwrap_or_default().to_owned(),
        }),
        Kind::Browser => State::Browser(Browser {
            url: values.text(&format!("{key}.url")).unwrap_or_default().to_owned(),
            // **What a project comes back with is the address the node is on**, so a node opens with its
            // own address in its bar rather than with an empty one. `Browser::typed` is not written down —
            // see the note on it — and this is where it starts. `task-1905`.
            typed: values.text(&format!("{key}.url")).unwrap_or_default().to_owned(),
            // A project comes back showing where the page is, so the bar is the page's.
            editing: false,
        }),
        Kind::Folder => State::Folder(Folder {
            root: values
                .text(&format!("{key}.root"))
                .map(|at| project_state::absolute(root, Path::new(at))),
            expanded: read_a_list(values, &format!("{key}.expanded"), root),
            filter: String::new(),
            scroll: values.number(&format!("{key}.scroll")).unwrap_or(0.0).max(0.0),
            // A zoom a person set is worth coming back with — it is how big they wanted this node's rows,
            // which is what `panes.<panel>.zoom` is for the panel. `task-1905`.
            zoom: match values.number(&format!("{key}.zoom")).unwrap_or(0.0) {
                asked if asked > 0.0 => asked,
                _ => 1.0,
            },
        }),
        Kind::Editor => State::Editor(Editor {
            // **`paths` first, and the older `path` after it.** A `space.conf` written before `task-1906`
            // names one file under `path`, and reading it as the node's one tab is what makes a canvas
            // written by the previous version open unchanged — the rule `Layout::read_from` keeps about a
            // settings file written before the panels could be moved.
            paths: match read_a_list(values, &format!("{key}.paths"), root) {
                found if !found.is_empty() => found,
                _ => values
                    .text(&format!("{key}.path"))
                    .map(|file| vec![project_state::absolute(root, Path::new(file))])
                    .unwrap_or_default(),
            },
            showing: values.number(&format!("{key}.showing")).unwrap_or(0.0).max(0.0) as usize,
            caret: values.number(&format!("{key}.caret")).unwrap_or(0.0).max(0.0) as usize,
            scroll: values.number(&format!("{key}.scroll")).unwrap_or(0.0),
            // `0.0` means "follow `appearance.font.size`", which is the convention a terminal node's own
            // size already uses.
            font_size: values.number(&format!("{key}.font")).unwrap_or(0.0).max(0.0),
        }),
        Kind::Chat => State::Chat(Chat {
            conversation: values
                .text(&format!("{key}.conversation"))
                .unwrap_or_default()
                .to_owned(),
            zoom: read_a_zoom(values, key),
        }),
        Kind::Tasks => State::Tasks(Tasks { zoom: read_a_zoom(values, key) }),
    };
    Some(Node { id, at, size, title, state })
}

/// Where a terminal node's screen is kept, so it can come back showing what was on it.
///
/// **A file per node rather than a key in `space.conf`**, because a screen is kilobytes of escape sequences and
/// `space.conf` is a settings file somebody reads and edits by hand. `task-1908`.
pub fn screen_path(root: &Path, node: crate::services::space::NodeId) -> std::path::PathBuf {
    project_state::folder(root).join("terminals").join(format!("{node}.bytes"))
}

/// Write down what is on a terminal node's screen.
///
/// **Called when the window closes and at no other time.** A screen changes on every keystroke, and
/// `Space::is_dirty` exists so the canvas is not written sixty times a second; what somebody wants back is the
/// last state, so it is written once. `None` removes whatever was there, which is what a node drawing its own
/// full screen answers — see `Session::screen_to_replay`.
pub fn save_a_screen(
    root: &Path,
    node: crate::services::space::NodeId,
    bytes: Option<&[u8]>,
) -> Result<(), String> {
    let file = screen_path(root, node);
    let Some(bytes) = bytes else {
        // Removed rather than left, so a node that came back at a prompt does not replay yesterday's screen the
        // time after that.
        let _ = std::fs::remove_file(&file);
        return Ok(());
    };
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
    }
    crate::services::store::write_atomically(&file, bytes)
        .map_err(|problem| format!("{} could not be written: {problem}", file.display()))
}

/// The file a terminal node is to come back showing, when there is one worth showing.
///
/// **The path rather than the bytes**, because what reads them is no longer this process: `task-1912` measured
/// that a screen written into a terminal from outside is erased by the console host on Windows, so what is
/// restored is printed by a program *inside* the node's own console — `unluminous_terminal::restore`. The file
/// is taken away by whoever printed it.
///
/// An empty file answers `None` and is removed, so a node whose screen was written down as nothing does not
/// start a shim to print nothing.
pub fn a_screen_to_print(root: &Path, node: crate::services::space::NodeId) -> Option<PathBuf> {
    let file = screen_path(root, node);
    let worth_it = std::fs::metadata(&file).map(|about| about.len() > 0).unwrap_or(false);
    if !worth_it {
        let _ = std::fs::remove_file(&file);
        return None;
    }
    Some(file)
}

/// Throw away whatever a node had written down, because it is starting without it.
///
/// **What keeps `task-1908`'s rule true now that the reading has moved**: a canvas that failed to come back
/// must not replay a week-old screen for ever. The shim deletes the file once it has printed it, and this is
/// the other way a file stops existing — a node started with nothing to restore.
pub fn forget_a_screen(root: &Path, node: crate::services::space::NodeId) {
    let _ = std::fs::remove_file(screen_path(root, node));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::space::Kind;

    /// A canvas with one of everything on it, wired, at a camera nobody would arrive at by accident.
    fn a_canvas(project: &Path) -> Space {
        let mut space = Space::new();
        let terminal = space.add_node(Kind::Terminal, Pos2::new(120.0, 40.0), Some(project));
        let browser = space.add_node(Kind::Browser, Pos2::new(800.0, 40.0), Some(project));
        let folder = space.add_node(Kind::Folder, Pos2::new(120.0, 500.0), Some(project));
        let editor = space.add_node(Kind::Editor, Pos2::new(500.0, 500.0), Some(project));
        let chat = space.add_node(Kind::Chat, Pos2::new(900.0, 500.0), Some(project));
        let tasks = space.add_node(Kind::Tasks, Pos2::new(1400.0, 500.0), Some(project));
        space.change(terminal, |state| {
            if let State::Terminal(terminal) = state {
                terminal.command = "claude".to_owned();
                terminal.session = "6f1c0b0e".to_owned();
                // What was really in the foreground of it, which is not the command — `task-1907`.
                terminal.running = "claude".to_owned();
                terminal.font_size = 13.0;
            }
        });
        space.change(browser, |state| {
            if let State::Browser(browser) = state {
                browser.url = "https://example.com/a page".to_owned();
            }
        });
        space.change(folder, |state| {
            if let State::Folder(folder) = state {
                folder.expanded = vec![project.join("src"), project.join("src").join("deep")];
            }
        });
        space.change(editor, |state| {
            if let State::Editor(editor) = state {
                editor.paths = vec![project.join("src").join("main.rs")];
                editor.caret = 4821;
            }
        });
        // **Which conversation each agent is on and how big it draws**, which is what `task-1914` asks a
        // canvas of agents to come back with: every one of them reopening the newest would be several
        // views of one conversation.
        space.change(chat, |state| {
            if let State::Chat(chat) = state {
                chat.conversation = "1730492811".to_owned();
                chat.zoom = 1.25;
            }
        });
        space.change(tasks, |state| {
            if let State::Tasks(tasks) = state {
                tasks.zoom = 0.75;
            }
        });
        space.connect(chat, terminal, Pipe::Off).expect("wired");
        space.connect(terminal, browser, Pipe::Off).expect("wired");
        space.connect(terminal, folder, Pipe::Off).expect("wired");
        space.title_node(terminal, "the agent");
        space.current_mut().camera = Camera { at: Pos2::new(-317.5, 208.0), zoom: 0.75 };
        let second = space.add_view("Rendering");
        space.show_view(second);
        space
    }

    #[test]
    fn a_canvas_written_down_and_read_back_is_the_same_canvas() {
        // Everything the ticket asks to be retained: the places, the sizes, the zoom, the wiring, the
        // command a terminal runs, the session it may resume, the address a browser is on, the folders
        // a tree has open, and the file and caret an editor is at.
        let project = Path::new("/projects/thing");
        let space = a_canvas(project);
        let mut values = Values::new();
        write(&space, project, &mut values);
        let back = read(&values, project);

        assert_eq!(back.views().len(), space.views().len());
        assert_eq!(back.current_id(), space.current_id(), "the view that was showing is showing");
        let was = &space.views()[0];
        let now = &back.views()[0];
        assert_eq!(now.name, was.name);
        assert_eq!(now.camera, was.camera);
        assert_eq!(now.nodes.len(), was.nodes.len());
        for (before, after) in was.nodes.iter().zip(&now.nodes) {
            assert_eq!(after.id, before.id);
            assert_eq!(after.at, before.at);
            assert_eq!(after.size, before.size);
            assert_eq!(after.title, before.title);
            match (&before.state, &after.state) {
                // **A browser node's `typed` is not written down and comes back as its address**, which
                // is what puts the node's own address in its bar when a project opens. A half-typed
                // address is not state a project should come back with — `task-1905`, and the note on
                // `Browser::typed`.
                (State::Browser(was), State::Browser(now)) => {
                    assert_eq!(now.url, was.url, "a node's own state came back");
                    assert_eq!(now.typed, was.url, "and its bar opens on the address it is on");
                }
                (before, after) => assert_eq!(after, before, "a node's own state came back"),
            }
        }
        assert_eq!(now.edges.len(), was.edges.len());
        assert_eq!(now.edges[0].from, was.edges[0].from);
        assert_eq!(now.edges[0].to, was.edges[0].to);
    }

    /// A `space.conf` written before `task-1907` opens unchanged.
    ///
    /// **Every one of these on a real machine is that file**, so the absent key has to read as something rather
    /// than refusing: a node with no `running` is a node that was at a prompt, which is what a canvas written by
    /// the previous version means. It is `Layout::read_from`'s own rule about a settings file written before the
    /// panels could be moved.
    #[test]
    fn a_space_conf_written_before_the_running_program_was_recorded_opens_unchanged() {
        let project = Path::new("/projects/thing");
        let mut values = Values::new();
        values.set("space.current", "1");
        values.set("space.view.0.id", "1");
        values.set("space.view.0.name", "Main");
        values.set("space.view.0.node.0.id", "5");
        values.set("space.view.0.node.0.kind", "terminal");
        values.set("space.view.0.node.0.command", "zsh");
        let back = read(&values, project);
        let node = &back.views()[0].nodes[0];
        let State::Terminal(terminal) = &node.state else { panic!("a terminal node") };
        assert_eq!(terminal.command, "zsh", "what it was given still comes back");
        assert_eq!(
            terminal.running, "",
            "and what was running reads as a prompt rather than refusing"
        );
    }

    /// What was running is written down beside the command, and they are different things.
    ///
    /// `task-1907`: a person adds a plain terminal node — `command` empty — and types `claude` into the shell,
    /// which is the case the whole section exists for. Asserted on the values rather than on a round trip,
    /// because what the file holds is the thing a later window reads.
    #[test]
    fn what_a_terminal_was_running_is_written_beside_what_it_was_given() {
        let project = Path::new("/projects/thing");
        let mut space = Space::new();
        let node = space.add_node(Kind::Terminal, Pos2::new(0.0, 0.0), Some(project));
        space.change(node, |state| {
            if let State::Terminal(terminal) = state {
                terminal.running = "claude".to_owned();
            }
        });
        let mut values = Values::new();
        write(&space, project, &mut values);
        assert_eq!(values.text("space.view.0.node.0.running"), Some("claude"));
        assert_eq!(
            values.text("space.view.0.node.0.command"),
            None,
            "a plain shell node was given no command, which is the case this is for"
        );
    }

    /// A hand edited file cannot ask for a tab that is not there.
    ///
    /// `showing` is an index into `paths`, and `space.conf` is a text file a person can edit — so an index past
    /// the end, or one on a canvas whose node has no tabs at all, has to answer with something rather than
    /// panicking. `Editor::showing()` is the one place that is decided, which is why the readers all go
    /// through it. `task-1906`.
    #[test]
    fn a_showing_index_past_the_end_is_the_last_tab_rather_than_a_panic() {
        use crate::services::space::node::Editor;
        let one = std::path::PathBuf::from("/a/one.rs");
        let two = std::path::PathBuf::from("/a/two.rs");

        let held =
            Editor { paths: vec![one.clone(), two.clone()], showing: 1, ..Editor::default() };
        assert_eq!(held.showing(), Some(two.as_path()));

        // Past the end, which a hand edited file can ask for.
        let held =
            Editor { paths: vec![one.clone(), two.clone()], showing: 99, ..Editor::default() };
        assert_eq!(held.showing(), Some(two.as_path()), "the last one rather than nothing");

        // And a node with no tabs at all answers with nothing rather than reaching into an empty list.
        let held = Editor { paths: Vec::new(), showing: 3, ..Editor::default() };
        assert_eq!(held.showing(), None);
    }

    /// A `|` in a filename is a character, not a separator.
    ///
    /// Both lists a node writes — its tabs and its open folders — were one value with the paths joined by
    /// `|`, and `|` is legal in a Unix filename. So `a|b.rs` came back as two paths that do not exist, and the
    /// node quietly lost a tab. They are numbered keys now, which is what `run-configurations.conf` already
    /// does with a list for exactly this reason. `task-1906`, found by the Codex Sol review.
    #[test]
    fn a_pipe_in_a_filename_is_part_of_the_name_rather_than_a_separator() {
        use crate::services::space::node::{Editor, Folder};
        let project = Path::new("/projects/thing");
        let awkward = project.join("a|b.rs");
        let ordinary = project.join("plain.rs");

        let mut space = a_canvas(project);
        let editor = space.add_node(Kind::Editor, egui::Pos2::ZERO, None);
        space.change(editor, |state| {
            if let State::Editor(held) = state {
                *held =
                    Editor { paths: vec![awkward.clone(), ordinary.clone()], ..Editor::default() };
            }
        });
        let folder = space.add_node(Kind::Folder, egui::Pos2::ZERO, None);
        space.change(folder, |state| {
            if let State::Folder(held) = state {
                *held = Folder { expanded: vec![project.join("with|a|pipe")], ..Folder::default() };
            }
        });

        let mut values = Values::new();
        write(&space, project, &mut values);
        let back = read(&Values::parse(&values.to_text_headed("x")), project);

        match &back.current().node(editor).expect("the node came back").state {
            State::Editor(held) => {
                assert_eq!(
                    held.paths,
                    vec![awkward, ordinary],
                    "the name with a pipe in it came back as two paths that do not exist",
                );
            }
            other => panic!("{other:?}"),
        }
        match &back.current().node(folder).expect("the node came back").state {
            State::Folder(held) => {
                assert_eq!(held.expanded, vec![project.join("with|a|pipe")]);
            }
            other => panic!("{other:?}"),
        }
    }

    /// A `space.conf` written by the version before this one still opens.
    ///
    /// The lists were `|` separated values, so a file already on somebody's disk names its paths that way.
    /// This is the rule `Layout::read_from` keeps about a settings file written before the panels could be
    /// moved: the older spelling is read, and only the newer one is written.
    #[test]
    fn a_canvas_written_the_older_way_still_opens() {
        let project = Path::new("/projects/thing");
        let text = "\
space.current = 1
space.view.0.id = 1
space.view.0.name = Main
space.view.0.node.0.id = 2
space.view.0.node.0.kind = editor
space.view.0.node.0.paths = one.rs|two.rs
space.view.0.node.1.id = 3
space.view.0.node.1.kind = folder
space.view.0.node.1.expanded = src|src/deeper
";
        let back = read(&Values::parse(text), project);
        match &back.current().node(2).expect("the editor node").state {
            State::Editor(held) => {
                assert_eq!(held.paths, vec![project.join("one.rs"), project.join("two.rs")]);
            }
            other => panic!("{other:?}"),
        }
        match &back.current().node(3).expect("the folder node").state {
            State::Folder(held) => {
                assert_eq!(
                    held.expanded,
                    vec![project.join("src"), project.join("src").join("deeper")],
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_path_inside_the_project_is_written_relative_so_a_project_that_moves_still_opens() {
        let project = Path::new("/projects/thing");
        let space = a_canvas(project);
        let mut values = Values::new();
        write(&space, project, &mut values);
        let written = values.to_text();
        assert!(written.contains("src/main.rs"), "written relative: {written}");
        assert!(!written.contains("/projects/thing/src/main.rs"), "and not in full");

        // Read back against a different root, which is the project having been moved or checked out
        // somewhere else, and the file is found in the new place.
        let moved = Path::new("/elsewhere/thing");
        let back = read(&values, moved);
        let editor = back.views()[0]
            .nodes
            .iter()
            .find(|node| node.kind() == Kind::Editor)
            .expect("the editor node came back");
        let State::Editor(editor) = &editor.state else { panic!("it is an editor") };
        assert_eq!(editor.paths, vec![moved.join("src").join("main.rs")]);
    }

    #[test]
    fn a_canvas_written_to_a_folder_that_cannot_be_made_says_so_rather_than_pretending() {
        // A path under a file rather than under a folder, which no platform will make a directory in.
        let file = std::env::temp_dir().join("unluminous-space-not-a-folder");
        std::fs::write(&file, "not a folder").expect("write the file");
        let refusal = save(&file, &Space::new()).expect_err("it cannot be written there");
        assert!(refusal.contains("could not be"), "{refusal}");
    }

    #[test]
    fn a_file_that_is_not_there_opens_an_empty_canvas_rather_than_refusing() {
        let space = load(Path::new("/nothing/like/this"));
        assert_eq!(space.views().len(), 1);
        assert!(space.current().nodes.is_empty());
    }

    #[test]
    fn a_node_of_a_kind_this_version_has_not_got_is_dropped_rather_than_refusing_the_file() {
        let mut values = Values::new();
        values.set("space.current", "1");
        values.set("space.view.0.id", "1");
        values.set("space.view.0.name", "Main");
        values.set("space.view.0.node.0.id", "5");
        values.set("space.view.0.node.0.kind", "terminal");
        values.set("space.view.0.node.1.id", "6");
        values.set("space.view.0.node.1.kind", "hologram");
        values.set("space.view.0.edge.0.from", "5");
        values.set("space.view.0.edge.0.to", "6");
        let space = read(&values, Path::new("/projects/thing"));
        assert_eq!(space.current().nodes.len(), 1, "the one it knows about");
        assert!(space.current().edges.is_empty(), "and the edge to the one it does not");
    }

    #[test]
    fn a_camera_out_of_range_in_a_hand_edited_file_is_clamped_rather_than_believed() {
        let mut values = Values::new();
        values.set("space.current", "1");
        values.set("space.view.0.id", "1");
        values.set("space.view.0.camera.zoom", "9000");
        let space = read(&values, Path::new("/projects/thing"));
        assert_eq!(space.current().camera.zoom, super::super::node::MAX_ZOOM);
    }

    #[test]
    fn ids_carry_on_from_what_was_read_so_a_new_node_never_takes_one_that_is_in_use() {
        // The counter is in the file's own numbers rather than reset to one, or the first node added
        // after a project opened would be given an id an edge already names.
        let project = Path::new("/projects/thing");
        let space = a_canvas(project);
        let mut values = Values::new();
        write(&space, project, &mut values);
        let mut back = read(&values, project);
        let used: Vec<u64> = back
            .views()
            .iter()
            .flat_map(|view| view.nodes.iter().map(|node| node.id))
            .chain(back.views().iter().map(|view| view.id))
            .collect();
        let fresh = back.add_node(Kind::Terminal, Pos2::ZERO, Some(project));
        assert!(!used.contains(&fresh), "{fresh} was already in use");
    }
}
