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

use std::path::Path;

use egui::{Pos2, Vec2};

use crate::services::project_state;
use crate::services::store::Values;

use super::node::{Browser, Camera, Edge, Editor, Folder, Kind, Node, Pipe, State, Terminal};
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
    std::fs::write(
        &file,
        values.to_text_headed("Unluminous: the Base of Infinite Space in this project."),
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
            let expanded: Vec<String> =
                folder.expanded.iter().map(|path| written(root, path)).collect();
            values.set_or_clear(&format!("{key}.expanded"), &expanded.join("|"));
            if (folder.zoom - 1.0).abs() > 0.001 {
                values.set(&format!("{key}.zoom"), format!("{:.2}", folder.zoom));
            }
        }
        State::Editor(editor) => {
            if let Some(file) = &editor.path {
                values.set(&format!("{key}.path"), written(root, file));
            }
            values.set(&format!("{key}.caret"), editor.caret.to_string());
            values.set(&format!("{key}.scroll"), format!("{:.1}", editor.scroll));
            if editor.font_size > 0.0 {
                values.set(&format!("{key}.font"), format!("{:.0}", editor.font_size));
            }
        }
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
        let pipe = values
            .text(&format!("{key}.pipe"))
            .and_then(Pipe::from_name)
            .unwrap_or_default();
        // The id is not written down: an edge is named by the two nodes it joins, and a fresh number
        // is handed out below by `Space::adopt`, which is also what stops a hand written file giving
        // two edges one id.
        edges.push(Edge { id: 0, from, to, pipe });
    }
    View { id, name, nodes, edges, camera, chosen: None }
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
            expanded: values
                .text(&format!("{key}.expanded"))
                .unwrap_or_default()
                .split('|')
                .filter(|part| !part.trim().is_empty())
                .map(|part| project_state::absolute(root, Path::new(part)))
                .collect(),
            filter: String::new(),
            // A zoom a person set is worth coming back with — it is how big they wanted this node's rows,
            // which is what `panes.<panel>.zoom` is for the panel. `task-1905`.
            zoom: match values.number(&format!("{key}.zoom")).unwrap_or(0.0) {
                asked if asked > 0.0 => asked,
                _ => 1.0,
            },
        }),
        Kind::Editor => State::Editor(Editor {
            path: values
                .text(&format!("{key}.path"))
                .map(|file| project_state::absolute(root, Path::new(file))),
            caret: values.number(&format!("{key}.caret")).unwrap_or(0.0).max(0.0) as usize,
            scroll: values.number(&format!("{key}.scroll")).unwrap_or(0.0),
            // `0.0` means "follow `appearance.font.size`", which is the convention a terminal node's own
            // size already uses.
            font_size: values.number(&format!("{key}.font")).unwrap_or(0.0).max(0.0),
        }),
    };
    Some(Node { id, at, size, title, state })
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
        space.change(terminal, |state| {
            if let State::Terminal(terminal) = state {
                terminal.command = "claude".to_owned();
                terminal.session = "6f1c0b0e".to_owned();
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
                editor.path = Some(project.join("src").join("main.rs"));
                editor.caret = 4821;
            }
        });
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
        assert_eq!(now.edges.len(), 2);
        assert_eq!(now.edges[0].from, was.edges[0].from);
        assert_eq!(now.edges[0].to, was.edges[0].to);
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
        assert_eq!(editor.path, Some(moved.join("src").join("main.rs")));
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
