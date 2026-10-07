//! Where a realm is written down, and what it reads back as.
//!
//! `task-2202` is the implementation of `tasks/task-2199-realm-tdd.md`, and §5.1 and §5.2 of it are this
//! file. A realm is a `.realm` file **in the project**, by default under `.realm-files/`, so it can be
//! committed with the code it describes. What a person was doing in it on this machine — where the camera
//! is, which node is chosen, a terminal's conversation, a caret, a scroll, where a sound was paused — is a
//! **sidecar** under `.unluminous/realms/`, which git ignores, because two people on two machines want
//! different values for every one of those and a pan should not be a change to a committed file.
//!
//! Both are the `name = value` text every Unluminous state file is, through
//! [`crate::services::store::Values`]:
//!
//! ```text
//! Unluminous realm: an infinite canvas in this project.
//! realm.format = 1
//! realm.reader = 1
//! realm.writer = 1
//! realm.needs =
//! realm.name = Architecture
//! node.3f9a1c2d.kind = note
//! node.3f9a1c2d.x = 120.0
//! node.3f9a1c2d.z = 4
//! node.3f9a1c2d.file = .realm-files/architecture/plan.md
//! edge.e9a8b7c6.from = 3f9a1c2d
//! ```
//!
//! ## The rules, each of which has a test below
//!
//! **Records are keyed by id**, eight lower case hex digits, so a node's lines sit together, a change to
//! one node changes that node's lines only, and two branches that each added a node merge cleanly.
//!
//! **Nothing this build does not understand is lost.** A node of a kind it does not know is read as
//! [`State::Unknown`] holding every key; a key it does not read under a node, under an edge or at the top
//! goes to that record's `extra` map; both are written back exactly as they came.
//!
//! **A file says which Unluminous may read it and which may write it.** `realm.reader` above
//! [`READS_UP_TO`] refuses to open; `realm.writer` above [`super::FORMAT`], or a `realm.needs` name this build
//! lacks, opens read only.
//!
//! **A path is relative to the project, with `/`**, and a `file`, a folder's `root` or an editor's `paths`
//! that leaves the project is refused on read: the node loads with nothing to show and says why, and the
//! key is kept as it was.
//!
//! **A save that changes nothing writes nothing**, and one that does writes the same bytes for everything
//! it did not change: keys are sorted and every number is written the one way.
//!
//! **A file that cannot be read is said so**, rather than opened as an empty canvas and written over.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use egui::{Pos2, Vec2};

use crate::services::project_state;
use crate::services::store::Values;

use super::node::{
    Audio, Browser, Camera, Chat, Edge, Editor, Fit, Folder, Image, Kind, Node, NodeId, Note,
    NoteView, Pipe, State, Tasks, Terminal, Unknown, Video,
};
use super::{Access, Format, Realm, READS_UP_TO};

/// The extension a realm file has, without the dot.
pub const EXTENSION: &str = "realm";

/// Where a new realm goes, inside the project.
pub const FOLDER: &str = ".realm-files";

/// The realm a project with none is given the first time the panel shows.
pub const FIRST: &str = ".realm-files/main.realm";

/// The heading on the first line of a realm file. Prose: the reader needs nothing from it.
const HEADING: &str = "Unluminous realm: an infinite canvas in this project.";

/// How many nodes and edges a realm is believed to hold.
///
/// A sanity bound on a hand edited or corrupted file, the way `PANEL_MAX_WIDTH` is one on a width. The
/// numbers `space.conf` had.
pub const NODE_LIMIT: usize = 256;
pub const EDGE_LIMIT: usize = 512;

/// The top level keys the format itself uses.
const FORMAT_KEYS: [&str; 5] =
    ["realm.format", "realm.reader", "realm.writer", "realm.needs", "realm.name"];

/// An id as a realm file writes one: eight lower case hex digits.
pub fn hex(id: u64) -> String {
    format!("{id:08x}")
}

/// An id as a realm file wrote one. Any hex is accepted, so a hand edited `A` reads as `0000000a`.
fn parse_id(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() || text.len() > 16 {
        return None;
    }
    u64::from_str_radix(text, 16).ok().filter(|id| *id != 0)
}

// ------------------------------------------------------------------------------- reading

/// Read a realm file's text. `path` is where it is, relative to the project; `root` is the project.
///
/// Refused, with a sentence a person can act on, when the text is not a realm file or needs a newer
/// Unluminous to read.
pub fn read(text: &str, root: &Path, path: &Path) -> Result<Realm, String> {
    let values = Values::parse(text);
    let Some(format) = values.number("realm.format").map(|format| format as u32) else {
        return Err(format!("{} is not a realm file: it has no realm.format.", path.display()));
    };
    let reader = values.number("realm.reader").map(|reader| reader as u32).unwrap_or(1);
    if reader > READS_UP_TO {
        return Err(format!(
            "{} needs an Unluminous that reads realm format {reader}. This one reads up to {READS_UP_TO}.",
            path.display()
        ));
    }
    let values = migrate(values, format);
    let format = Format {
        format,
        reader,
        writer: values.number("realm.writer").map(|writer| writer as u32).unwrap_or(1),
        needs: values
            .text("realm.needs")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|need| !need.is_empty())
            .map(str::to_owned)
            .collect(),
    };
    let mut realm = Realm::new(path, values.text("realm.name").unwrap_or_default());
    if realm.name.is_empty() {
        realm.name = realm.title();
    }
    realm.access = format.access();
    realm.format = format;

    // Every key, sorted into the record it belongs to. A `Values` is a sorted map, so a node's keys arrive
    // together and the order of the records is the order of their ids.
    let mut nodes: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut edges: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (name, value) in values.starting_with("") {
        if let Some(rest) = name.strip_prefix("node.") {
            if let Some((id, key)) = rest.split_once('.') {
                nodes.entry(id.to_owned()).or_default().insert(key.to_owned(), value);
                continue;
            }
        }
        if let Some(rest) = name.strip_prefix("edge.") {
            if let Some((id, key)) = rest.split_once('.') {
                edges.entry(id.to_owned()).or_default().insert(key.to_owned(), value);
                continue;
            }
        }
        if !FORMAT_KEYS.contains(&name.as_str()) {
            realm.extra.insert(name, value);
        }
    }

    // **A second node with an id already in use gets a fresh one**, which is what a bad merge or a hand
    // edit that spelled one id two ways leaves. The edges that named the id keep pointing at the first
    // node, and the file is not rewritten until something else changes. §5.2 rule 6.
    let mut by_spelling: BTreeMap<String, NodeId> = BTreeMap::new();
    let mut ids_taken: BTreeSet<u64> = BTreeSet::new();
    for (spelling, keys) in nodes {
        if realm.nodes.len() >= NODE_LIMIT {
            eprintln!("{} holds more than {NODE_LIMIT} nodes; the rest are not read", path.display());
            break;
        }
        let Some(mut id) = parse_id(&spelling) else {
            eprintln!("{}: node.{spelling} is not an id, so it is not read", path.display());
            continue;
        };
        if !ids_taken.insert(id) {
            let fresh = super::fresh_id(|taken| ids_taken.contains(&taken));
            eprintln!(
                "{}: node {} is a second node with id {}, so it is called {} now",
                path.display(),
                spelling,
                hex(id),
                hex(fresh)
            );
            ids_taken.insert(fresh);
            id = fresh;
        } else {
            by_spelling.insert(spelling.clone(), id);
        }
        realm.nodes.push(read_a_node(id, keys, root, path));
    }
    // Drawing order: by z, and by id where two share one.
    realm.nodes.sort_by(|left, right| left.z.cmp(&right.z).then(left.id.cmp(&right.id)));

    let named = |spelling: &str| -> Option<NodeId> {
        by_spelling.get(spelling).copied().or_else(|| parse_id(spelling))
    };
    for (spelling, mut keys) in edges {
        if realm.edges.len() >= EDGE_LIMIT {
            eprintln!("{} holds more than {EDGE_LIMIT} edges; the rest are not read", path.display());
            break;
        }
        let (Some(from), Some(to)) = (
            keys.remove("from").and_then(|from| named(&from)),
            keys.remove("to").and_then(|to| named(&to)),
        ) else {
            continue;
        };
        let pipe = keys.remove("pipe").and_then(|pipe| Pipe::from_name(pipe.trim())).unwrap_or_default();
        let id = match parse_id(&spelling) {
            Some(id) if !ids_taken.contains(&id) => id,
            _ => super::fresh_id(|taken| ids_taken.contains(&taken)),
        };
        ids_taken.insert(id);
        realm.edges.push(Edge { id, from, to, pipe, extra: keys });
    }
    realm.tidy();
    realm.on_disk.realm = Some(text.to_owned());
    realm.written();
    Ok(realm)
}

/// Bring a file written in an older format up to [`super::FORMAT`], one step at a time.
///
/// **Each step is a pure function of the values**, tested against a fixture of the format it starts from
/// in `tests/fixtures/realm/`. Format 1 is the first, so there is no step yet; `space.conf` is a different
/// file and is converted by [`import`], not migrated.
fn migrate(values: Values, from: u32) -> Values {
    // Format 1 is the first, so a file is never older than it and there is nothing to do. A file from a
    // newer format that this build may still read (its `realm.reader` allowed it) is read as it is.
    // The day format 2 exists, `if from < 2 { values = to_2(values) }` goes here.
    let _ = from;
    values
}

/// Take a key out of a node's map.
fn take(keys: &mut BTreeMap<String, String>, key: &str) -> Option<String> {
    keys.remove(key)
}

fn number(keys: &mut BTreeMap<String, String>, key: &str) -> Option<f32> {
    keys.get(key).and_then(|value| value.trim().parse::<f32>().ok()).inspect(|_| {
        keys.remove(key);
    })
}

fn flag(keys: &mut BTreeMap<String, String>, key: &str) -> Option<bool> {
    let found = match keys.get(key)?.trim() {
        "true" | "yes" | "1" => true,
        "false" | "no" | "0" => false,
        _ => return None,
    };
    keys.remove(key);
    Some(found)
}

/// A path a realm names, when it is inside the project.
///
/// **Refused when it could reach anything else**: an absolute path, a drive letter, a share, or a `..`
/// segment anywhere, even one that would come back inside. A realm file is shared with the project, and a
/// picture node pointing at somebody's home folder is a file the realm should not be able to read.
pub fn inside(root: &Path, written: &str) -> Result<PathBuf, String> {
    let written = written.trim().replace('\\', "/");
    if written.is_empty() {
        return Err("names no file".to_owned());
    }
    let absolute = written.starts_with('/')
        || written.as_bytes().get(1) == Some(&b':')
        || Path::new(&written).is_absolute();
    if absolute {
        return Err(format!("{written} is outside the project"));
    }
    if written.split('/').any(|part| part == "..") {
        return Err(format!("{written} leaves the project"));
    }
    Ok(project_state::absolute(root, Path::new(&written)))
}

/// A `file`, `root` or list entry, read through [`inside`]. A refusal keeps the key as it was in the
/// node's `extra`, so the file is written back unchanged, and says why on the node.
fn a_path(
    keys: &mut BTreeMap<String, String>,
    key: &str,
    root: &Path,
    extra: &mut BTreeMap<String, String>,
    refused: &mut Option<String>,
) -> Option<PathBuf> {
    let written = keys.remove(key)?;
    match inside(root, &written) {
        Ok(path) => Some(path),
        Err(why) => {
            eprintln!("a realm node's {key} was refused: {why}");
            extra.insert(key.to_owned(), written);
            refused.get_or_insert(format!("The file {why}."));
            None
        }
    }
}

/// A numbered list of paths, `<key>.count` and `<key>.N`.
///
/// **A `|` is a legal character in a filename**, which is why this is numbered rather than joined — the
/// Codex Sol review of `task-1906` found a node losing a tab to it.
fn a_list(
    keys: &mut BTreeMap<String, String>,
    key: &str,
    root: &Path,
    checked: bool,
    extra: &mut BTreeMap<String, String>,
    refused: &mut Option<String>,
) -> Vec<PathBuf> {
    let Some(many) = number(keys, &format!("{key}.count")) else { return Vec::new() };
    let mut found = Vec::new();
    let mut raw: Vec<(String, String)> = Vec::new();
    let mut any_refused = false;
    for index in 0..many.max(0.0) as usize {
        let name = format!("{key}.{index}");
        let Some(written) = keys.remove(&name) else { continue };
        raw.push((name, written.clone()));
        if written.trim().is_empty() {
            continue;
        }
        match checked {
            true => match inside(root, &written) {
                Ok(path) => found.push(path),
                Err(why) => {
                    any_refused = true;
                    refused.get_or_insert(format!("A file {why}."));
                }
            },
            false => found.push(project_state::absolute(root, Path::new(written.trim()))),
        }
    }
    // A list with a refused entry is kept whole, as it was written, so writing it back cannot reorder or
    // drop the entry this build would not open.
    if any_refused {
        extra.insert(format!("{key}.count"), raw.len().to_string());
        for (name, written) in raw {
            extra.insert(name, written);
        }
    }
    found
}

/// A multiplier that was written as a positive number, or 1.
fn a_zoom(keys: &mut BTreeMap<String, String>, key: &str) -> f32 {
    match number(keys, key).unwrap_or(0.0) {
        asked if asked > 0.0 => asked,
        _ => 1.0,
    }
}

/// One node, from every key that was under `node.<id>.` in the file.
fn read_a_node(id: NodeId, mut keys: BTreeMap<String, String>, root: &Path, path: &Path) -> Node {
    let kind_name = take(&mut keys, "kind").unwrap_or_default().trim().to_owned();
    let kind = Kind::from_name(&kind_name);
    let shape = kind.unwrap_or(Kind::Unknown);
    let at = Pos2::new(number(&mut keys, "x").unwrap_or(0.0), number(&mut keys, "y").unwrap_or(0.0));
    let smallest = shape.smallest();
    let size = Vec2::new(
        number(&mut keys, "width").unwrap_or(shape.opens_at().x).max(smallest.x),
        number(&mut keys, "height").unwrap_or(shape.opens_at().y).max(smallest.y),
    );
    let z = number(&mut keys, "z").map(|z| z.max(0.0) as u32).unwrap_or(0);
    let title = take(&mut keys, "title").unwrap_or_default();
    let mut extra = BTreeMap::new();
    let mut refused = None;
    let state = match kind {
        None => {
            eprintln!("{}: node {} is a {kind_name}, which this Unluminous does not know", path.display(), hex(id));
            // Every key that is left is the kind's own, and all of them go back out unchanged.
            State::Unknown(Unknown { kind: kind_name, keys: std::mem::take(&mut keys) })
        }
        Some(Kind::Unknown) => State::Unknown(Unknown { kind: kind_name, keys: std::mem::take(&mut keys) }),
        Some(Kind::Terminal) => State::Terminal(Terminal {
            command: take(&mut keys, "command").unwrap_or_default(),
            // A terminal's folder is not refused: starting a shell somewhere reads nothing, and a node
            // pointed at a sibling checkout is an ordinary thing to have.
            folder: take(&mut keys, "folder").map(|folder| project_state::absolute(root, Path::new(folder.trim()))),
            font_size: number(&mut keys, "font").unwrap_or(0.0).max(0.0),
            session: String::new(),
            running: String::new(),
        }),
        Some(Kind::Browser) => {
            let url = take(&mut keys, "url").unwrap_or_default();
            // **What a realm comes back with is the address the node is on**, so a node opens with its own
            // address in its bar rather than with an empty one. `task-1905`.
            State::Browser(Browser { typed: url.clone(), url, editing: false })
        }
        Some(Kind::Folder) => State::Folder(Folder {
            // An empty `root`, or none, is the project itself, which is what a folder node starts on.
            root: match keys.get("root").map(|written| written.trim().is_empty()) {
                Some(true) | None => {
                    keys.remove("root");
                    Some(root.to_path_buf())
                }
                Some(false) => a_path(&mut keys, "root", root, &mut extra, &mut refused),
            },
            expanded: Vec::new(),
            filter: String::new(),
            scroll: 0.0,
            zoom: a_zoom(&mut keys, "zoom"),
        }),
        Some(Kind::Editor) => State::Editor(Editor {
            paths: a_list(&mut keys, "paths", root, true, &mut extra, &mut refused),
            showing: 0,
            caret: 0,
            scroll: 0.0,
            font_size: number(&mut keys, "font").unwrap_or(0.0).max(0.0),
        }),
        Some(Kind::Chat) => State::Chat(Chat {
            conversation: take(&mut keys, "conversation").unwrap_or_default(),
            zoom: a_zoom(&mut keys, "zoom"),
        }),
        Some(Kind::Tasks) => State::Tasks(Tasks { zoom: a_zoom(&mut keys, "zoom") }),
        Some(Kind::Image) => State::Image(Image {
            file: a_path(&mut keys, "file", root, &mut extra, &mut refused),
            fit: take(&mut keys, "fit").and_then(|fit| Fit::from_name(&fit)).unwrap_or_default(),
            ..Image::default()
        }),
        Some(Kind::Audio) => State::Audio(Audio {
            file: a_path(&mut keys, "file", root, &mut extra, &mut refused),
            volume: number(&mut keys, "volume").unwrap_or(1.0).clamp(0.0, 1.0),
            looping: flag(&mut keys, "loop").unwrap_or(false),
            position: 0.0,
        }),
        Some(Kind::Video) => State::Video(Video {
            file: a_path(&mut keys, "file", root, &mut extra, &mut refused),
            volume: number(&mut keys, "volume").unwrap_or(1.0).clamp(0.0, 1.0),
            looping: flag(&mut keys, "loop").unwrap_or(false),
            muted: flag(&mut keys, "muted").unwrap_or(false),
            position: 0.0,
        }),
        Some(Kind::Note) => State::Note(Note {
            file: a_path(&mut keys, "file", root, &mut extra, &mut refused),
            view: take(&mut keys, "view").and_then(|view| NoteView::from_name(&view)).unwrap_or_default(),
            font_size: number(&mut keys, "font").unwrap_or(0.0).max(0.0),
            caret: 0,
            scroll: 0.0,
        }),
    };
    // Whatever the kind's reader did not take is a key a newer Unluminous wrote. §5.2 rule 3.
    extra.extend(keys);
    Node { id, at, size, z, title, state, extra, refused }
}

// ------------------------------------------------------------------------------- writing

/// The text of the realm file: the format's own keys, every node and edge, and every key that was kept.
///
/// **Nothing here starts from the file on disk.** A node deleted in the window has to disappear from the
/// file, which a merge over what is there would undo — the reason `settings::save_with` merges and this
/// does not. §5.2 rule 8.
pub fn write(realm: &Realm, root: &Path) -> String {
    let mut values = Values::new();
    for (key, value) in &realm.extra {
        values.set(key, value.clone());
    }
    values.set("realm.format", realm.format.format.to_string());
    values.set("realm.reader", realm.format.reader.to_string());
    values.set("realm.writer", realm.format.writer.to_string());
    values.set("realm.needs", realm.format.needs.join(", "));
    values.set("realm.name", realm.name.clone());
    for node in realm.nodes.iter().take(NODE_LIMIT) {
        write_a_node(node, root, &mut values);
    }
    for edge in realm.edges.iter().take(EDGE_LIMIT) {
        let key = format!("edge.{}", hex(edge.id));
        values.set(&format!("{key}.from"), hex(edge.from));
        values.set(&format!("{key}.to"), hex(edge.to));
        values.set(&format!("{key}.pipe"), edge.pipe.name());
        for (name, value) in &edge.extra {
            values.set(&format!("{key}.{name}"), value.clone());
        }
    }
    values.to_text_headed(HEADING)
}

/// A path as it is written down: relative to the project when it is inside it, with `/` separators so
/// the file reads the same on both platforms.
fn written(root: &Path, path: &Path) -> String {
    super::slashed(&project_state::relative(root, path))
}

fn write_a_list(values: &mut Values, key: &str, paths: &[PathBuf], root: &Path) {
    values.set(&format!("{key}.count"), paths.len().to_string());
    for (index, path) in paths.iter().enumerate() {
        values.set(&format!("{key}.{index}"), written(root, path));
    }
}

/// A number that is a multiple, written only when it is not 1.
fn a_multiple(values: &mut Values, key: &str, multiple: f32) {
    if (multiple - 1.0).abs() > 0.001 {
        values.set(key, format!("{multiple:.2}"));
    }
}

/// One node's lines in the realm file.
fn write_a_node(node: &Node, root: &Path, values: &mut Values) {
    let key = format!("node.{}", hex(node.id));
    let set = |values: &mut Values, name: &str, value: String| values.set(&format!("{key}.{name}"), value);
    set(values, "kind", node.state.kind_name().to_owned());
    set(values, "x", format!("{:.1}", node.at.x));
    set(values, "y", format!("{:.1}", node.at.y));
    set(values, "width", format!("{:.1}", node.size.x));
    set(values, "height", format!("{:.1}", node.size.y));
    set(values, "z", node.z.to_string());
    values.set_or_clear(&format!("{key}.title"), &node.title);
    let file = |values: &mut Values, file: &Option<PathBuf>| {
        if let Some(file) = file {
            values.set(&format!("{key}.file"), written(root, file));
        }
    };
    match &node.state {
        State::Terminal(terminal) => {
            values.set_or_clear(&format!("{key}.command"), &terminal.command);
            if terminal.font_size > 0.0 {
                set(values, "font", format!("{:.0}", terminal.font_size));
            }
            if let Some(folder) = &terminal.folder {
                set(values, "folder", written(root, folder));
            }
        }
        // `Browser::typed` is deliberately not written: a half-typed address is not state a realm should
        // come back with. `task-1905`.
        State::Browser(browser) => values.set_or_clear(&format!("{key}.url"), &browser.url),
        State::Folder(folder) => {
            // The project itself is written as no `root` at all, which is what reads back as the project.
            if let Some(at) = folder.root.as_ref().filter(|at| !written(root, at).is_empty()) {
                set(values, "root", written(root, at));
            }
            a_multiple(values, &format!("{key}.zoom"), folder.zoom);
        }
        State::Editor(editor) => {
            write_a_list(values, &format!("{key}.paths"), &editor.paths, root);
            if editor.font_size > 0.0 {
                set(values, "font", format!("{:.0}", editor.font_size));
            }
        }
        State::Chat(chat) => {
            // **Which conversation, so each agent comes back on its own.** The pane reopens the newest
            // because there is one of it; a canvas of chats that all reopened the newest would be several
            // views of one conversation, which is the thing `Kind::Chat` exists not to be.
            values.set_or_clear(&format!("{key}.conversation"), &chat.conversation);
            a_multiple(values, &format!("{key}.zoom"), chat.zoom);
        }
        State::Tasks(tasks) => a_multiple(values, &format!("{key}.zoom"), tasks.zoom),
        State::Image(image) => {
            file(values, &image.file);
            set(values, "fit", image.fit.name().to_owned());
        }
        State::Audio(audio) => {
            file(values, &audio.file);
            set(values, "volume", format!("{:.2}", audio.volume));
            set(values, "loop", audio.looping.to_string());
        }
        State::Video(video) => {
            file(values, &video.file);
            set(values, "volume", format!("{:.2}", video.volume));
            set(values, "loop", video.looping.to_string());
            set(values, "muted", video.muted.to_string());
        }
        State::Note(note) => {
            file(values, &note.file);
            set(values, "view", note.view.name().to_owned());
            if note.font_size > 0.0 {
                set(values, "font", format!("{:.0}", note.font_size));
            }
        }
        State::Unknown(unknown) => {
            for (name, value) in &unknown.keys {
                set(values, name, value.clone());
            }
        }
    }
    // After the known keys, so a key kept because it was refused is written exactly as it was read.
    for (name, value) in &node.extra {
        set(values, name, value.clone());
    }
}

// ------------------------------------------------------------------------------- the sidecar

/// Where a realm's sidecar is: `.unluminous/realms/<path with every / as __>.conf`.
pub fn sidecar_path(root: &Path, realm: &Path) -> PathBuf {
    let flat = super::slashed(realm).replace('/', "__");
    project_state::folder(root).join("realms").join(format!("{flat}.conf"))
}

/// The text of the sidecar: what a person was doing in this realm on this machine.
pub fn write_the_sidecar(realm: &Realm, root: &Path) -> String {
    let mut values = Values::new();
    values.set("camera.x", format!("{:.1}", realm.camera.at.x));
    values.set("camera.y", format!("{:.1}", realm.camera.at.y));
    values.set("camera.zoom", format!("{:.3}", realm.camera.zoom));
    if let Some(chosen) = realm.chosen {
        values.set("chosen", hex(chosen));
    }
    for node in &realm.nodes {
        let key = format!("node.{}", hex(node.id));
        match &node.state {
            State::Terminal(terminal) => {
                values.set_or_clear(&format!("{key}.session"), &terminal.session);
                // **What was running, which is not the command.** `task-1907`.
                values.set_or_clear(&format!("{key}.running"), &terminal.running);
            }
            State::Folder(folder) => {
                write_a_list(&mut values, &format!("{key}.expanded"), &folder.expanded, root);
                if folder.scroll > 0.5 {
                    values.set(&format!("{key}.scroll"), format!("{:.1}", folder.scroll));
                }
            }
            State::Editor(editor) => {
                if editor.showing > 0 {
                    values.set(&format!("{key}.showing"), editor.showing.to_string());
                }
                if editor.caret > 0 {
                    values.set(&format!("{key}.caret"), editor.caret.to_string());
                }
                if editor.scroll.abs() > 0.05 {
                    values.set(&format!("{key}.scroll"), format!("{:.1}", editor.scroll));
                }
            }
            State::Note(note) => {
                if note.caret > 0 {
                    values.set(&format!("{key}.caret"), note.caret.to_string());
                }
                if note.scroll.abs() > 0.05 {
                    values.set(&format!("{key}.scroll"), format!("{:.1}", note.scroll));
                }
            }
            State::Image(image) => {
                a_multiple(&mut values, &format!("{key}.zoom"), image.zoom);
                if image.scroll != Vec2::ZERO {
                    values.set(&format!("{key}.scroll.x"), format!("{:.1}", image.scroll.x));
                    values.set(&format!("{key}.scroll.y"), format!("{:.1}", image.scroll.y));
                }
            }
            State::Audio(audio) if audio.position > 0.05 => {
                values.set(&format!("{key}.position"), format!("{:.1}", audio.position));
            }
            State::Video(video) if video.position > 0.05 => {
                values.set(&format!("{key}.position"), format!("{:.1}", video.position));
            }
            _ => {}
        }
    }
    values.to_text_headed(&format!(
        "Unluminous realm state: what you were doing in {} on this machine.",
        super::slashed(&realm.path)
    ))
}

/// Put what the sidecar says back onto a realm read from its file.
///
/// **A missing sidecar is not a fault**: the camera stays at the origin and nothing is chosen. A sidecar
/// naming a node that is not there is ignored, because the realm file is the one that says what exists.
pub fn read_the_sidecar(realm: &mut Realm, text: &str, root: &Path) {
    let values = Values::parse(text);
    realm.camera = Camera {
        at: Pos2::new(
            values.number("camera.x").unwrap_or(0.0),
            values.number("camera.y").unwrap_or(0.0),
        ),
        zoom: values
            .number("camera.zoom")
            .unwrap_or(1.0)
            .clamp(super::node::MIN_ZOOM, super::node::MAX_ZOOM),
    };
    realm.chosen = values
        .text("chosen")
        .and_then(parse_id)
        .filter(|id| realm.nodes.iter().any(|node| node.id == *id));
    for node in &mut realm.nodes {
        let key = format!("node.{}", hex(node.id));
        let text = |name: &str| values.text(&format!("{key}.{name}")).unwrap_or_default().to_owned();
        let number = |name: &str| values.number(&format!("{key}.{name}"));
        match &mut node.state {
            State::Terminal(terminal) => {
                terminal.session = text("session");
                // Absent in a canvas written before `task-1907`, which reads as a node that was at a prompt.
                terminal.running = text("running");
            }
            State::Folder(folder) => {
                let mut keys: BTreeMap<String, String> = values
                    .starting_with(&format!("{key}."))
                    .into_iter()
                    .collect();
                let (mut ignored, mut none) = (BTreeMap::new(), None);
                folder.expanded = a_list(&mut keys, "expanded", root, false, &mut ignored, &mut none);
                folder.scroll = number("scroll").unwrap_or(0.0).max(0.0);
            }
            State::Editor(editor) => {
                editor.showing = number("showing").unwrap_or(0.0).max(0.0) as usize;
                editor.caret = number("caret").unwrap_or(0.0).max(0.0) as usize;
                editor.scroll = number("scroll").unwrap_or(0.0);
            }
            State::Note(note) => {
                note.caret = number("caret").unwrap_or(0.0).max(0.0) as usize;
                note.scroll = number("scroll").unwrap_or(0.0);
            }
            State::Image(image) => {
                image.zoom = match number("zoom").unwrap_or(0.0) {
                    asked if asked > 0.0 => asked,
                    _ => 1.0,
                };
                image.scroll = Vec2::new(number("scroll.x").unwrap_or(0.0), number("scroll.y").unwrap_or(0.0));
            }
            State::Audio(audio) => audio.position = number("position").unwrap_or(0.0).max(0.0),
            State::Video(video) => video.position = number("position").unwrap_or(0.0).max(0.0),
            _ => {}
        }
    }
    realm.on_disk.sidecar = Some(text.to_owned());
}

// ------------------------------------------------------------------------------- on disk

/// Read the realm at `path`, relative to the project, with its sidecar.
pub fn load(root: &Path, path: &Path) -> Result<Realm, String> {
    let file = root.join(path);
    let text = std::fs::read_to_string(&file)
        .map_err(|problem| format!("{} could not be read: {problem}", super::slashed(path)))?;
    let mut realm = read(&text, root, path)?;
    if let Ok(sidecar) = std::fs::read_to_string(sidecar_path(root, path)) {
        read_the_sidecar(&mut realm, &sidecar, root);
    }
    realm.written();
    Ok(realm)
}

/// Write a realm and its sidecar, each only when what it would write differs from what is there.
///
/// **A read only realm's file is never written**; its sidecar is, so the camera and the choice survive.
/// It says whether it wrote, because a write that quietly failed and was treated as a write is a canvas
/// somebody arranged and lost. Found by the `task-1904` review.
pub fn save(root: &Path, realm: &mut Realm) -> Result<(), String> {
    if realm.editable() {
        let text = write(realm, root);
        if realm.on_disk.realm.as_deref() != Some(text.as_str()) {
            let file = root.join(&realm.path);
            write_a_file(&file, &text)?;
            realm.on_disk.realm = Some(text);
        }
    }
    let sidecar = write_the_sidecar(realm, root);
    if realm.on_disk.sidecar.as_deref() != Some(sidecar.as_str()) {
        write_a_file(&sidecar_path(root, &realm.path), &sidecar)?;
        realm.on_disk.sidecar = Some(sidecar);
    }
    Ok(())
}

/// Write `text` to `file` atomically, making its folder first.
fn write_a_file(file: &Path, text: &str) -> Result<(), String> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
    }
    crate::services::store::write_atomically(file, text.as_bytes())
        .map_err(|problem| format!("{} could not be written: {problem}", file.display()))
}

/// Make a new, empty realm called `name` in `.realm-files/`, and answer with where it is.
///
/// Refused when a realm by that name is there already, because this never writes over a file.
pub fn create(root: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':']) || name.starts_with('.') {
        return Err(format!("{name:?} cannot be a realm's name."));
    }
    let path = Path::new(FOLDER).join(format!("{name}.{EXTENSION}"));
    if root.join(&path).exists() {
        return Err(format!("There is a realm called {name} already."));
    }
    let mut realm = Realm::new(&path, name);
    save(root, &mut realm)?;
    Ok(path)
}

/// Copy a realm under new ids to `<name> copy.realm` beside it, with no sidecar.
pub fn duplicate(root: &Path, realm: &Realm) -> Result<PathBuf, String> {
    let folder = realm.path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = realm.title();
    let path = (1..)
        .map(|number| match number {
            1 => folder.join(format!("{stem} copy.{EXTENSION}")),
            more => folder.join(format!("{stem} copy {more}.{EXTENSION}")),
        })
        .find(|path| !root.join(path).exists())
        .expect("one of the names is free");
    let mut copy = realm.duplicated(&path, &super::title_of(&path));
    copy.access = Access::Edit;
    let text = write(&copy, root);
    write_a_file(&root.join(&path), &text)?;
    Ok(path)
}

/// Every realm file in the project, relative to it, sorted.
///
/// **A walk of its own rather than the explorer's**, because the explorer hides every dot folder and
/// `.realm-files/` is one. It skips what a build writes and the folders that are never a project's own:
/// `.git`, `.unluminous`, `target`, `node_modules`, and every other dot folder, which is where tools keep
/// their state. Bounded, so a project the size of a home folder answers quickly with what it found.
pub fn list(root: &Path) -> Vec<PathBuf> {
    const DEEPEST: usize = 12;
    const MOST_ENTRIES: usize = 50_000;
    let mut found = Vec::new();
    let mut seen = 0usize;
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((folder, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MOST_ENTRIES {
                break;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                let skipped = (name.starts_with('.') && name != FOLDER)
                    || matches!(name.as_str(), "target" | "node_modules" | "__pycache__");
                if !skipped && depth < DEEPEST {
                    stack.push((path, depth + 1));
                }
            } else if path.extension().and_then(|ext| ext.to_str()) == Some(EXTENSION) {
                found.push(project_state::relative(root, &path));
            }
        }
    }
    found.sort_by_key(|path| super::slashed(path).to_lowercase());
    found
}

/// How many nodes and edges the realm file at `path` holds, without reading the rest of it.
pub fn counts(root: &Path, path: &Path) -> Option<(usize, usize)> {
    let text = std::fs::read_to_string(root.join(path)).ok()?;
    let values = Values::parse(&text);
    let mut nodes = BTreeSet::new();
    let mut edges = BTreeSet::new();
    for (name, _) in values.starting_with("") {
        if let Some((id, _)) = name.strip_prefix("node.").and_then(|rest| rest.split_once('.')) {
            nodes.insert(id.to_owned());
        } else if let Some((id, _)) = name.strip_prefix("edge.").and_then(|rest| rest.split_once('.')) {
            edges.insert(id.to_owned());
        }
    }
    Some((nodes.len(), edges.len()))
}

/// Move a realm's sidecar along with it, after its file was renamed.
pub fn move_the_sidecar(root: &Path, from: &Path, to: &Path) {
    let (old, new) = (sidecar_path(root, from), sidecar_path(root, to));
    if old.exists() && !new.exists() {
        if let Some(folder) = new.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let _ = std::fs::rename(old, new);
    }
}

// ------------------------------------------------------------------------------- the import

/// The file inside `.unluminous` the canvas lived in before `task-2202`.
pub const LEGACY_FILE: &str = "space.conf";

/// What [`import`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Imported {
    /// Every realm file it wrote, relative to the project.
    pub written: Vec<PathBuf>,
    /// Every realm file it would have written and did not, because one was there already.
    pub skipped: Vec<PathBuf>,
    /// The file the view that was showing became, which is the realm to open.
    pub current: Option<PathBuf>,
}

/// Turn `.unluminous/space.conf` into realm files, one per view, under `.realm-files/`.
///
/// **It never deletes and never writes over anything.** `space.conf` is left exactly where it is, and a
/// view whose realm file is already there is skipped and named in [`Imported::skipped`]. A view's camera,
/// its chosen node, its terminals' conversations and its carets go to that realm's sidecar. Ids are kept,
/// because they were unique across the whole project and are valid eight hex digit ids once formatted.
pub fn import(root: &Path) -> Result<Imported, String> {
    let legacy = project_state::folder(root).join(LEGACY_FILE);
    let text = std::fs::read_to_string(&legacy)
        .map_err(|problem| format!("{} could not be read: {problem}", legacy.display()))?;
    let (views, current) = legacy::read(&Values::parse(&text), root);
    let mut imported = Imported::default();
    let mut names_taken: BTreeSet<String> = BTreeSet::new();
    for (view_id, name, mut realm) in views {
        let slug = slug_of(&name);
        let chosen = (1..)
            .map(|number| match number {
                1 => slug.clone(),
                more => format!("{slug} {more}"),
            })
            .find(|tried| !names_taken.contains(&tried.to_lowercase()))
            .expect("one of the names is free");
        names_taken.insert(chosen.to_lowercase());
        let path = Path::new(FOLDER).join(format!("{chosen}.{EXTENSION}"));
        if current == Some(view_id) {
            imported.current = Some(path.clone());
        }
        if root.join(&path).exists() {
            imported.skipped.push(path);
            continue;
        }
        realm.path = path.clone();
        realm.name = name;
        save(root, &mut realm)?;
        imported.written.push(path);
    }
    Ok(imported)
}

/// A view's name as a file name: lower case, with anything that is not a letter or a digit as a dash.
pub fn slug_of(name: &str) -> String {
    let mut slug = String::new();
    for character in name.trim().chars() {
        match character.is_alphanumeric() {
            true => slug.extend(character.to_lowercase()),
            false if !slug.ends_with('-') => slug.push('-'),
            false => {}
        }
    }
    match slug.trim_matches('-') {
        "" => "realm".to_owned(),
        trimmed => trimmed.to_owned(),
    }
}

/// Reading `space.conf`, the file the canvas was kept in from `task-1904` to `task-2202`.
///
/// **Kept only for the import.** Nothing writes this format any more. It reads every spelling a file of
/// that era can hold — the `|` separated lists written before numbered ones, the single `path` an editor
/// node had before `paths` — because the copy on somebody's disk may be any of them.
pub mod legacy {
    use super::*;

    const VIEW_LIMIT: usize = 64;

    /// Every view, as `(view id, name, realm)`, and the id of the view that was showing.
    pub fn read(values: &Values, root: &Path) -> (Vec<(u64, String, Realm)>, Option<u64>) {
        let mut views = Vec::new();
        for at in 0..VIEW_LIMIT {
            let key = format!("space.view.{at}");
            let Some(id) = values.number(&format!("{key}.id")).map(|id| id as u64) else { continue };
            if id == 0 {
                continue;
            }
            let name = values.text(&format!("{key}.name")).unwrap_or("View").to_owned();
            views.push((id, name.clone(), read_a_view(values, &key, &name, root)));
        }
        let current = values.number("space.current").map(|id| id as u64);
        (views, current)
    }

    fn read_a_view(values: &Values, key: &str, name: &str, root: &Path) -> Realm {
        let mut realm = Realm::new(Path::new(""), name);
        realm.camera = Camera {
            at: Pos2::new(
                values.number(&format!("{key}.camera.x")).unwrap_or(0.0),
                values.number(&format!("{key}.camera.y")).unwrap_or(0.0),
            ),
            zoom: values
                .number(&format!("{key}.camera.zoom"))
                .unwrap_or(1.0)
                .clamp(super::super::node::MIN_ZOOM, super::super::node::MAX_ZOOM),
        };
        for index in 0..NODE_LIMIT {
            if let Some(mut node) = read_a_node(values, &format!("{key}.node.{index}"), root) {
                // The place in the list was the drawing order, so it becomes the z.
                node.z = index as u32;
                realm.nodes.push(node);
            }
        }
        for index in 0..EDGE_LIMIT {
            let key = format!("{key}.edge.{index}");
            let (Some(from), Some(to)) = (
                values.number(&format!("{key}.from")).map(|id| id as u64),
                values.number(&format!("{key}.to")).map(|id| id as u64),
            ) else {
                continue;
            };
            let pipe =
                values.text(&format!("{key}.pipe")).and_then(Pipe::from_name).unwrap_or_default();
            // An edge had no id in `space.conf`, so it is given one now.
            let id = realm.fresh_id();
            realm.edges.push(Edge::new(id, from, to, pipe));
        }
        realm.chosen = values
            .number(&format!("{key}.chosen"))
            .map(|id| id as u64)
            .filter(|id| realm.nodes.iter().any(|node| node.id == *id));
        realm.tidy();
        realm.touch();
        realm
    }

    /// The paths under a numbered list, or the older `|` separated value.
    fn read_a_list(values: &Values, key: &str, root: &Path) -> Vec<PathBuf> {
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

    fn a_zoom(values: &Values, key: &str) -> f32 {
        match values.number(&format!("{key}.zoom")).unwrap_or(0.0) {
            asked if asked > 0.0 => asked,
            _ => 1.0,
        }
    }

    /// One node, or nothing when its kind is one this version has not got — which in `space.conf` it
    /// never could be, because that file only ever held the six kinds `task-1914` left it with.
    fn read_a_node(values: &Values, key: &str, root: &Path) -> Option<Node> {
        let id = values.number(&format!("{key}.id")).map(|id| id as u64)?;
        let kind = Kind::from_name(values.text(&format!("{key}.kind"))?.trim())?;
        let mut node = Node::new(
            id,
            kind,
            Pos2::new(
                values.number(&format!("{key}.x")).unwrap_or(0.0),
                values.number(&format!("{key}.y")).unwrap_or(0.0),
            ),
            None,
        );
        let smallest = kind.smallest();
        node.size = Vec2::new(
            values.number(&format!("{key}.width")).unwrap_or(kind.opens_at().x).max(smallest.x),
            values.number(&format!("{key}.height")).unwrap_or(kind.opens_at().y).max(smallest.y),
        );
        node.title = values.text(&format!("{key}.title")).unwrap_or_default().to_owned();
        let text = |name: &str| values.text(&format!("{key}.{name}")).unwrap_or_default().to_owned();
        node.state = match kind {
            Kind::Terminal => State::Terminal(Terminal {
                command: text("command"),
                folder: values
                    .text(&format!("{key}.folder"))
                    .map(|folder| project_state::absolute(root, Path::new(folder))),
                font_size: values.number(&format!("{key}.font")).unwrap_or(0.0).max(0.0),
                session: text("session"),
                running: text("running"),
            }),
            Kind::Browser => State::Browser(Browser { url: text("url"), typed: text("url"), editing: false }),
            Kind::Folder => State::Folder(Folder {
                root: values
                    .text(&format!("{key}.root"))
                    .map(|at| project_state::absolute(root, Path::new(at))),
                expanded: read_a_list(values, &format!("{key}.expanded"), root),
                filter: String::new(),
                scroll: values.number(&format!("{key}.scroll")).unwrap_or(0.0).max(0.0),
                zoom: a_zoom(values, key),
            }),
            Kind::Editor => State::Editor(Editor {
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
                font_size: values.number(&format!("{key}.font")).unwrap_or(0.0).max(0.0),
            }),
            Kind::Chat => State::Chat(Chat { conversation: text("conversation"), zoom: a_zoom(values, key) }),
            Kind::Tasks => State::Tasks(Tasks { zoom: a_zoom(values, key) }),
            _ => return None,
        };
        Some(node)
    }
}

// ------------------------------------------------------------------------------- terminal screens

/// Which terminal a screen belongs to: a node on the canvas, or a tab in the terminal tile.
///
/// **One enum rather than two functions**, because the two are the same bytes kept the same way and
/// `task-1945` is the ticket where the tile learnt what `task-1908` gave the canvas. A node and a tab
/// cannot disagree about where a screen lives, because [`screen_path`] is the only place the name is
/// decided and it takes this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// A terminal node on the Realm, named by its node id.
    Node(crate::services::realm::NodeId),
    /// A terminal tab in the tile, named by where it is in the strip.
    Tab(usize),
}

impl Screen {
    /// The file name this screen is kept under, which is what keeps the two kinds apart in one folder.
    fn file_name(self) -> String {
        match self {
            Screen::Node(node) => format!("{node}.bytes"),
            Screen::Tab(index) => format!("tab-{index}.bytes"),
        }
    }
}

/// Where a terminal's screen is kept, so it can come back showing what was on it.
///
/// **A file per terminal rather than a key in the realm**, because a screen is kilobytes of escape
/// sequences and a realm file is text somebody reads and edits by hand. `task-1908`.
pub fn screen_path(root: &Path, screen: Screen) -> std::path::PathBuf {
    project_state::folder(root).join("terminals").join(screen.file_name())
}

/// Write down what is on a terminal node's screen.
///
/// **Called when the window closes and at no other time.** A screen changes on every keystroke, and
/// `Realm::is_dirty` exists so the canvas is not written sixty times a second; what somebody wants back is the
/// last state, so it is written once. `None` removes whatever was there, which is what a node drawing its own
/// full screen answers — see `Session::screen_to_replay`.
pub fn save_a_screen(root: &Path, screen: Screen, bytes: Option<&[u8]>) -> Result<(), String> {
    let file = screen_path(root, screen);
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
pub fn a_screen_to_print(root: &Path, screen: Screen) -> Option<PathBuf> {
    let file = screen_path(root, screen);
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
pub fn forget_a_screen(root: &Path, screen: Screen) {
    let _ = std::fs::remove_file(screen_path(root, screen));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own for one test, under the system's temporary folder.
    fn a_project(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("unluminous-realm-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("make the project");
        folder
    }

    /// A realm with one of every kind on it, wired, at a camera nobody would arrive at by accident.
    fn every_kind(project: &Path) -> Realm {
        let mut realm = Realm::new(Path::new(".realm-files/everything.realm"), "everything");
        let mut ids = Vec::new();
        for (at, kind) in Kind::ALL.into_iter().enumerate() {
            ids.push(realm.add_node(kind, Pos2::new(at as f32 * 700.0, 40.0), Some(project)));
        }
        let [terminal, browser, folder, editor, chat, tasks, image, audio, video, note] =
            ids[..] else { panic!("ten kinds") };
        realm.change(terminal, |state| {
            if let State::Terminal(terminal) = state {
                terminal.command = "claude".to_owned();
                terminal.session = "6f1c0b0e".to_owned();
                terminal.running = "claude".to_owned();
                terminal.font_size = 13.0;
            }
        });
        realm.change(browser, |state| {
            if let State::Browser(browser) = state {
                browser.url = "https://example.com/a page".to_owned();
                browser.typed = browser.url.clone();
            }
        });
        realm.change(folder, |state| {
            if let State::Folder(folder) = state {
                folder.expanded = vec![project.join("src"), project.join("src").join("deep")];
                folder.scroll = 120.0;
                folder.zoom = 1.25;
            }
        });
        realm.change(editor, |state| {
            if let State::Editor(editor) = state {
                editor.paths = vec![project.join("src").join("main.rs"), project.join("a|b.rs")];
                editor.showing = 1;
                editor.caret = 4821;
                editor.scroll = 300.5;
            }
        });
        realm.change(chat, |state| {
            if let State::Chat(chat) = state {
                chat.conversation = "1730492811".to_owned();
                chat.zoom = 1.25;
            }
        });
        realm.change(tasks, |state| {
            if let State::Tasks(tasks) = state {
                tasks.zoom = 0.75;
            }
        });
        realm.change(image, |state| {
            if let State::Image(image) = state {
                image.file = Some(project.join("design").join("cover.png"));
                image.fit = Fit::Cover;
            }
        });
        realm.change(audio, |state| {
            if let State::Audio(audio) = state {
                audio.file = Some(project.join(".realm-files/everything/intro.mp3"));
                audio.volume = 0.8;
                audio.position = 42.5;
            }
        });
        realm.change(video, |state| {
            if let State::Video(video) = state {
                video.file = Some(project.join("clip.mp4"));
                video.muted = true;
                video.looping = true;
            }
        });
        realm.change(note, |state| {
            if let State::Note(note) = state {
                note.file = Some(project.join(".realm-files/everything/plan.md"));
                note.view = NoteView::Preview;
                note.caret = 12;
            }
        });
        realm.connect(chat, terminal, Pipe::Off).expect("wired");
        realm.connect(terminal, browser, Pipe::Off).expect("wired");
        realm.connect(note, image, Pipe::Off).expect("wired");
        realm.title_node(terminal, "the agent");
        realm.camera = Camera { at: Pos2::new(-317.5, 208.0), zoom: 0.75 };
        realm.choose(Some(note));
        realm
    }

    /// The file and its sidecar, read back the way `load` reads them.
    fn round_trip(realm: &Realm, project: &Path) -> Realm {
        let text = write(realm, project);
        let mut back = read(&text, project, &realm.path).expect("it reads back");
        read_the_sidecar(&mut back, &write_the_sidecar(realm, project), project);
        back
    }

    /// Item 1 of §8: a realm with every kind on it, written and read, is the same realm, and writing it
    /// again gives the same bytes.
    #[test]
    fn a_realm_with_every_kind_round_trips_and_writes_the_same_bytes_twice() {
        let project = Path::new("/projects/thing");
        let realm = every_kind(project);
        let back = round_trip(&realm, project);
        assert_eq!(back.nodes.len(), realm.nodes.len());
        for (was, now) in realm.nodes.iter().zip(&back.nodes) {
            assert_eq!(now, was, "node {} came back changed", hex(was.id));
        }
        // A file lists edges by id, so they come back in that order; the order of edges means nothing.
        let sorted = |edges: &[Edge]| {
            let mut edges = edges.to_vec();
            edges.sort_by_key(|edge| edge.id);
            edges
        };
        assert_eq!(sorted(&back.edges), sorted(&realm.edges));
        assert_eq!(back.camera, realm.camera);
        assert_eq!(back.chosen, realm.chosen);
        assert_eq!(write(&back, project), write(&realm, project), "the second write is byte identical");
        assert_eq!(write_the_sidecar(&back, project), write_the_sidecar(&realm, project));
    }

    /// The split of §5.1: what two people would disagree about is in the sidecar and nowhere else.
    #[test]
    fn the_camera_the_choice_and_where_somebody_was_go_to_the_sidecar_and_not_the_file() {
        let project = Path::new("/projects/thing");
        let realm = every_kind(project);
        let file = write(&realm, project);
        for kept_out in ["camera", "chosen", "session", "caret", "running", "position", "expanded", "showing"] {
            assert!(!file.contains(&format!(".{kept_out} =")) && !file.contains(&format!("{kept_out}.")),
                "{kept_out} is in the realm file:\n{file}");
        }
        let sidecar = write_the_sidecar(&realm, project);
        assert!(sidecar.contains("camera.zoom = 0.750"), "{sidecar}");
        assert!(sidecar.contains(".session = 6f1c0b0e"), "{sidecar}");
        assert!(sidecar.contains(".position = 42.5"), "{sidecar}");
        assert!(file.contains(".view = preview"), "how a note is read is the realm's: {file}");
        assert!(file.contains(".file = design/cover.png"), "paths are relative, with /: {file}");
        assert!(file.starts_with(HEADING));
        assert!(file.contains("realm.format = 1\n"));
    }

    /// Item 2: a node of a kind this build has never heard of is kept, moved, and written back with every
    /// key it had.
    #[test]
    fn a_node_of_an_unknown_kind_is_kept_and_written_back_with_every_key() {
        let project = Path::new("/projects/thing");
        let text = "\
realm.format = 1
node.0000abcd.kind = hologram
node.0000abcd.beam = violet
node.0000abcd.x = 10.0
node.0000abcd.y = 20.0
node.0000abcd.depth.count = 2
";
        let mut realm = read(text, project, Path::new("a.realm")).expect("it reads");
        assert_eq!(realm.unknown_kinds(), vec!["hologram".to_owned()]);
        let node = realm.node(0xabcd).expect("the node is there");
        assert_eq!(node.kind(), Kind::Unknown);
        assert!(realm.move_node(0xabcd, Pos2::new(300.0, 20.0)), "it can be moved");
        let out = write(&realm, project);
        assert!(out.contains("node.0000abcd.kind = hologram"), "{out}");
        assert!(out.contains("node.0000abcd.beam = violet"), "{out}");
        assert!(out.contains("node.0000abcd.depth.count = 2"), "{out}");
        assert!(out.contains("node.0000abcd.x = 300.0"), "and the move is in it: {out}");
    }

    /// Item 3: a key this build does not read survives a load and a save, on a node, on an edge and at
    /// the top.
    #[test]
    fn keys_this_build_does_not_read_survive_on_a_node_an_edge_and_the_file() {
        let project = Path::new("/projects/thing");
        let text = "\
realm.format = 1
realm.colour = teal
node.00000001.kind = image
node.00000001.file = a.png
node.00000001.glow = soft
node.00000002.kind = image
node.00000002.file = b.png
edge.00000003.from = 00000001
edge.00000003.to = 00000002
edge.00000003.label = feeds
";
        let realm = read(text, project, Path::new("a.realm")).expect("it reads");
        assert_eq!(realm.extra.get("realm.colour").map(String::as_str), Some("teal"));
        let out = write(&realm, project);
        for kept in ["realm.colour = teal", "node.00000001.glow = soft", "edge.00000003.label = feeds"] {
            assert!(out.contains(kept), "{kept} was lost:\n{out}");
        }
    }

    /// Item 4: a file that needs a newer reader is refused by name; a newer writer or an unknown need opens
    /// read only and a save writes nothing to it.
    #[test]
    fn a_newer_file_is_refused_or_opened_read_only_and_never_written() {
        let project = a_project("newer");
        let refused = read("realm.format = 2\nrealm.reader = 2\n", &project, Path::new("a.realm"))
            .expect_err("a reader of format 2 is needed");
        assert!(refused.contains('2'), "the refusal names the version: {refused}");

        for (name, text) in [
            ("writer.realm", "realm.format = 1\nrealm.writer = 2\nnode.00000001.kind = browser\n"),
            ("needs.realm", "realm.format = 1\nrealm.needs = groups\nnode.00000001.kind = browser\n"),
        ] {
            std::fs::write(project.join(name), text).expect("write the file");
            let mut realm = load(&project, Path::new(name)).expect("it opens");
            assert!(!realm.editable(), "{name} opens read only");
            assert!(!realm.title_node(1, "renamed"), "a change is refused");
            assert!(!realm.is_dirty());
            realm.pan_by(Vec2::new(5.0, 5.0));
            save(&project, &mut realm).expect("the sidecar is written");
            assert_eq!(std::fs::read_to_string(project.join(name)).expect("read"), text, "{name} untouched");
            assert!(sidecar_path(&project, Path::new(name)).exists(), "the camera still goes somewhere");
        }
        let _ = std::fs::remove_dir_all(&project);
    }

    /// Item 5: no `realm.format` is not a realm file; a missing `z` is 0; a kind's missing keys take their
    /// defaults.
    #[test]
    fn a_missing_format_refuses_and_missing_keys_take_their_defaults() {
        let project = Path::new("/projects/thing");
        let refused = read("node.00000001.kind = audio\n", project, Path::new("x.realm"))
            .expect_err("no realm.format");
        assert!(refused.contains("not a realm file"), "{refused}");
        let realm = read("realm.format = 1\nnode.00000001.kind = audio\n", project, Path::new("x.realm"))
            .expect("it reads");
        let node = realm.node(1).expect("there");
        assert_eq!(node.z, 0);
        assert_eq!(node.size, Kind::Audio.opens_at());
        assert_eq!(node.state, State::Audio(Audio::default()));
    }

    /// Item 6: two nodes with one id, which a merge or a hand edit can leave — the second gets a fresh id
    /// and the edge keeps pointing at the first.
    #[test]
    fn a_second_node_with_an_id_already_in_use_gets_a_fresh_one() {
        let project = Path::new("/projects/thing");
        let text = "\
realm.format = 1
node.0000000a.kind = browser
node.0000000a.url = https://first/
node.A.kind = browser
node.A.url = https://second/
node.00000002.kind = terminal
edge.00000003.from = 00000002
edge.00000003.to = 0000000a
";
        let realm = read(text, project, Path::new("a.realm")).expect("it reads");
        assert_eq!(realm.nodes.len(), 3, "both are kept");
        let ids: BTreeSet<NodeId> = realm.nodes.iter().map(|node| node.id).collect();
        assert_eq!(ids.len(), 3, "and no two share an id");
        let first = realm.node(10).expect("the first keeps its id");
        assert_eq!(first.state, State::Browser(Browser { url: "https://first/".into(), typed: "https://first/".into(), editing: false }));
        assert_eq!(realm.edges[0].to, 10, "the edge points at the first");
        assert!(!realm.is_dirty(), "and nothing is rewritten until something changes");
    }

    /// Item 7: a path that leaves the project is refused on read, the key is kept, and the node says why.
    #[test]
    fn a_path_outside_the_project_is_refused_and_kept() {
        let project = Path::new("/projects/thing");
        for written in ["../x.png", "C:/x.png", "/x.png", "a/../b.png", "\\\\share\\x.png"] {
            let text = format!("realm.format = 1\nnode.00000001.kind = image\nnode.00000001.file = {written}\n");
            let realm = read(&text, project, Path::new("a.realm")).expect("it reads");
            let node = realm.node(1).expect("the node is kept");
            assert_eq!(node.state.file(), None, "{written} was believed");
            assert!(node.refused.is_some(), "{written} was refused without saying so");
            let out = write(&realm, project);
            assert!(out.contains(&format!("node.00000001.file = {}", written.trim())), "{written} was not kept:\n{out}");
        }
        let inside_it = read("realm.format = 1\nnode.00000001.kind = image\nnode.00000001.file = design/a.png\n", project, Path::new("a.realm"))
            .expect("it reads");
        assert_eq!(inside_it.node(1).expect("there").state.file(), Some(project.join("design").join("a.png")).as_deref());
    }

    /// Item 8: `space.conf` with three views becomes three realm files with their cameras in their
    /// sidecars, `space.conf` is left alone, and a second import changes nothing.
    #[test]
    fn importing_space_conf_makes_one_realm_a_view_and_touches_nothing_else() {
        let project = a_project("import");
        let legacy = project_state::folder(&project).join(LEGACY_FILE);
        std::fs::create_dir_all(legacy.parent().expect("a folder")).expect("make .unluminous");
        let original = std::fs::read_to_string(fixture("space.conf")).expect("the fixture");
        std::fs::write(&legacy, &original).expect("write space.conf");

        let imported = import(&project).expect("it imports");
        let names: Vec<String> = imported.written.iter().map(|path| super::super::slashed(path)).collect();
        assert_eq!(names, vec![".realm-files/main.realm", ".realm-files/rendering.realm", ".realm-files/main 2.realm"]);
        assert_eq!(imported.current.as_deref(), Some(Path::new(".realm-files/rendering.realm")));
        assert_eq!(std::fs::read_to_string(&legacy).expect("still there"), original, "space.conf is untouched");

        let main = load(&project, Path::new(".realm-files/main.realm")).expect("main opens");
        assert_eq!(main.camera, Camera { at: Pos2::new(-750.0, -741.2), zoom: 0.777 });
        assert_eq!(main.nodes.len(), 3);
        assert_eq!(main.edges.len(), 1);
        assert_eq!(main.node(3).expect("ids are kept").kind(), Kind::Tasks);
        let rendering = load(&project, Path::new(".realm-files/rendering.realm")).expect("it opens");
        match &rendering.node(10).expect("the terminal").state {
            State::Terminal(terminal) => assert_eq!(terminal.session, "abc-123", "its conversation came too"),
            other => panic!("{other:?}"),
        }
        assert_eq!(rendering.chosen, Some(10));

        let before: Vec<String> = imported.written.iter().map(|path| std::fs::read_to_string(project.join(path)).expect("read")).collect();
        let again = import(&project).expect("it runs again");
        assert!(again.written.is_empty(), "nothing is written over: {:?}", again.written);
        assert_eq!(again.skipped.len(), 3);
        let after: Vec<String> = imported.written.iter().map(|path| std::fs::read_to_string(project.join(path)).expect("read")).collect();
        assert_eq!(before, after);
        let _ = std::fs::remove_dir_all(&project);
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("realm").join(name)
    }

    /// Item 9: every format fixture reads and, written back by this build, is the expected file. There is
    /// one now; this is the contract for the next format.
    #[test]
    fn every_format_fixture_reads_and_writes_back_as_expected() {
        let project = Path::new("/projects/thing");
        for format in 1..=super::super::FORMAT {
            let text = std::fs::read_to_string(fixture(&format!("format-{format}.realm"))).expect("the fixture");
            let realm = read(&text, project, Path::new(".realm-files/architecture.realm")).expect("it reads");
            let written = write(&realm, project);
            let expected_file = fixture(&format!("format-{format}.expected.realm"));
            // `UPDATE_SNAPSHOTS=1` accepts what this build writes, which is the switch the pictures use. Read
            // the file before committing it: it is the contract the next format is held to.
            if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
                std::fs::write(&expected_file, &written).expect("write the expected file");
            }
            let expected = std::fs::read_to_string(&expected_file).expect("the expected file");
            assert_eq!(written.replace("\r\n", "\n"), expected.replace("\r\n", "\n"), "format {format}");
        }
    }

    #[test]
    fn a_saved_realm_is_written_once_and_a_save_with_no_change_writes_nothing() {
        let project = a_project("save");
        let path = create(&project, "plan").expect("made");
        assert_eq!(path, Path::new(".realm-files/plan.realm"));
        assert!(create(&project, "plan").is_err(), "never written over");
        let mut realm = load(&project, &path).expect("it opens");
        realm.add_node(Kind::Browser, Pos2::ZERO, None);
        save(&project, &mut realm).expect("saved");
        let file = project.join(&path);
        let stamp = std::fs::metadata(&file).and_then(|about| about.modified()).expect("stamp");
        std::thread::sleep(std::time::Duration::from_millis(20));
        save(&project, &mut realm).expect("saved again");
        assert_eq!(std::fs::metadata(&file).and_then(|about| about.modified()).expect("stamp"), stamp);
        assert_eq!(list(&project), vec![path.clone()]);
        let copy = duplicate(&project, &realm).expect("copied");
        assert_eq!(copy, Path::new(".realm-files/plan copy.realm"));
        let copied = load(&project, &copy).expect("it opens");
        assert_ne!(copied.nodes[0].id, realm.nodes[0].id, "under new ids");
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn a_view_name_becomes_a_file_name() {
        assert_eq!(slug_of("Main"), "main");
        assert_eq!(slug_of("Two Words!"), "two-words");
        assert_eq!(slug_of("  "), "realm");
        assert_eq!(sidecar_path(Path::new("/p"), Path::new(".realm-files/a.realm")),
            Path::new("/p").join(".unluminous").join("realms").join(".realm-files__a.realm.conf"));
    }

    #[test]
    fn a_file_that_cannot_be_read_says_so_rather_than_opening_empty() {
        let refusal = load(Path::new("/nothing/like/this"), Path::new("a.realm")).expect_err("not there");
        assert!(refusal.contains("could not be read"), "{refusal}");
    }
}
