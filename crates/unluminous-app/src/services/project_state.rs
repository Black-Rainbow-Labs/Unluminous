//! What Unluminous remembers about one project, kept in a `.unluminous` folder inside the project itself.
//!
//! `services::store` already remembers what belongs to the person: the font, the opacity, the recent
//! projects. This remembers what belongs to the *project*: which files were open, which of them was
//! showing, which folders in the explorer were opened out, whether the terminal was up and how many
//! tabs it had. `task-1658` asks for it, and the reference editor's `.idea` and Visual Studio Code's `.vscode` are
//! both the same idea — a folder beside the code rather than a file in the user's settings, so that
//! copying the project copies its state and two people working on the same folder do not fight over one
//! file.
//!
//! Three files, all plain text, in the format `services::store` already uses:
//!
//! - `.unluminous/workspace.conf` — the flags and the numbers, including the run widget's `run.selected`
//!   and `run.visible`. The run **configurations** are a file of their own beside these, because
//!   they belong to the project rather than to the person: `services::run_configurations`.
//! - `.unluminous/open-files.txt` — one path a line, in tab order.
//! - `.unluminous/expanded-folders.txt` — one path a line.
//!
//! The two lists are files of their own rather than numbered names in the conf, because that is what
//! `recent.txt` already does with a list of paths and because a list of paths reads better one to a line
//! than as `files.open.07 = ...`.
//!
//! **The split from `task-1664` is three more keys in the conf and nothing in `open-files.txt`**,
//! which matters: the file of paths stays a file of paths, and an Unluminous that has never heard of panes
//! reads it unchanged.
//!
//! ```text
//! files.panes = 0,0,1,1
//! files.pane-widths = 0.5,0.5
//! files.pane = 1
//! ```
//!
//! `files.panes` is one number a line of `open-files.txt`, in the same order; a file that has since
//! been deleted is dropped from both lists **in the same pass**, rather than one after the other,
//! which is how two parallel lists come apart. Everything else is defended by `OpenFiles` on the way
//! in, because a hand edited file is a file somebody may have got wrong, and the rule the whole of
//! this module keeps applies: a project that opens with nothing restored is better than a project
//! that will not open. The widths are fractions rather than points, so opening the project on a
//! screen of another size gives the same proportions rather than the same measurements.
//!
//! **Paths are written relative to the project** wherever they are inside it, so a project that is moved,
//! or checked out somewhere else on another machine, still opens the files it was left with. A path that
//! is somehow outside the project is written in full.
//!
//! A file that cannot be read is treated as a file that is not there, which is the rule the settings file
//! already keeps: a project opening with nothing restored is better than a project that will not open.

use std::path::{Path, PathBuf};

use crate::services::store::Values;

/// The folder inside the project that Unluminous keeps its state in.
pub const FOLDER: &str = ".unluminous";

const WORKSPACE_FILE: &str = "workspace.conf";
const OPEN_FILES_FILE: &str = "open-files.txt";
const EXPANDED_FILE: &str = "expanded-folders.txt";
const TERMINAL_TABS_FILE: &str = "terminal-tabs.txt";
/// The tabs a plugin drew, one `<plugin id>/<tab id>` a line.
///
/// A file of its own rather than a value in `workspace.conf`, because it is a list and the rest of that file
/// is single values, which is the same reason the open files and the expanded folders each have one.
const PLUGIN_TABS_FILE: &str = "plugin-tabs.txt";

/// How many files are remembered. A window with more tabs than this open has a problem the state file
/// is not going to fix, and a list that grows without limit is a file that grows without limit.
const OPEN_LIMIT: usize = 60;

/// What one terminal tab was doing, so it can come back doing it — `task-1945`.
///
/// The same four things a canvas terminal node has recorded since `task-1912`, said about a tab: where the
/// shell was, what was running in it, which conversation an agent was on, and the name a person gave it.
/// The screen is not here — it is kilobytes of escape sequences and lives in a file of its own, beside the
/// nodes' own, which is `services::space::store::save_a_screen`'s rule about the same bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RememberedTerminal {
    /// The name a person typed, empty for a tab that was never renamed.
    pub name: String,
    /// The folder the shell was in when the window closed, relative to the project when it is inside it.
    ///
    /// **Where the shell had got to, not where it was started**, which is the difference between "my
    /// terminal came back" and "my terminal came back where I left it": a person types `cd` and the
    /// pseudoterminal knows nothing about it. `unluminous_terminal::Session::folder` reads it off the shell
    /// process itself and answers `None` where the platform will not say, in which case this is empty and
    /// the tab opens in the project's own root as it always did.
    pub folder: String,
}

/// What was left open in one project.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectState {
    /// The files that were open, in tab order.
    pub open_files: Vec<PathBuf>,
    /// The tabs a plugin drew, by their `<plugin id>/<tab id>`.
    ///
    /// A separate list rather than an entry in `open_files`, because every list beside `open_files` is
    /// indexed with it — the pane, the scroll and the caret — and none of those means anything for a tab
    /// with no document. A plugin tab has no scroll of its own to remember either: what it is showing is the
    /// plugin's own state, kept in the plugin's own file.
    pub plugin_tabs: Vec<String>,
    /// Which of them was showing.
    pub active_file: usize,
    /// Which pane each of them was in, in the same order. All zeros for a state file written before
    /// there were panes, which is one pane holding everything.
    pub file_panes: Vec<usize>,
    /// How far each of them was scrolled, in points, in the same order.
    ///
    /// `task-1693` asks for "the same state, including open files, scroll position". It is one list
    /// beside `file_panes` and is filtered in the same pass, which is the rule that keeps two
    /// parallel lists from coming apart.
    pub file_scrolls: Vec<f32>,
    /// Where the caret was in each of them, as a byte offset, in the same order.
    ///
    /// Beside the scroll rather than instead of it, because the two answer different halves of the
    /// same question: the scroll is where you were looking and the caret is where you would type.
    /// Restoring one without the other puts the caret at the top of a file being read half way down,
    /// and the next key press throws the view away. An offset past the end of a file that has
    /// changed since is clamped rather than refused.
    pub file_carets: Vec<usize>,
    /// Each pane's share of the editing area's width, left to right. Empty means share it equally.
    pub pane_widths: Vec<f32>,
    /// Which pane had the keyboard.
    pub active_pane: usize,
    /// The folders that were opened out in the explorer.
    pub expanded_folders: Vec<PathBuf>,
    /// False when the explorer had been put away.
    pub explorer_visible: bool,
    /// Whether the editing area — the pane holding the tabs — was showing. `task-28`.
    ///
    /// True for a file that does not mention it, which is every file written before this, because a project that
    /// reopened with no editing area would look broken to somebody who never hid one.
    pub editor_visible: bool,
    /// True when the terminal tile was showing.
    pub terminal_visible: bool,
    /// True when the Base of Infinite Space was showing - `task-1904`.
    ///
    /// False for a file that does not mention it, which is every file written before this: a canvas
    /// nobody has opened is one nobody asked for, and opening a project into one would be the window
    /// deciding something it was never told.
    pub space_visible: bool,
    /// How many terminal tabs there were. The shells themselves cannot be brought back — what a
    /// program was doing when the window closed is gone — so what is restored is the same number of
    /// fresh shells in the project's own folder, which is what a person means by "my terminals were
    /// there".
    pub terminal_tabs: usize,
    /// The names somebody gave those tabs, in order, empty for a tab that was never renamed.
    ///
    /// A name a person typed is the one thing about a terminal that survives its shell — `task-1682`
    /// made it a value of its own for exactly that reason.
    ///
    /// **Kept beside [`Self::terminals`] rather than folded into it**, because a file written before
    /// `task-1945` holds only these and reads as a list of names; the loader fills the richer list in
    /// from them when that is all there is, so a project opened by an older build keeps its tab names.
    pub terminal_tab_names: Vec<String>,
    /// What each terminal tab was doing, in order — `task-1945`.
    ///
    /// *"I'm still not getting terminal state restored when I quit then re-open a project. both nodes
    /// and normal terminals should be restored to exactly where they were."* A terminal **node** has come
    /// back in its folder, running its program, showing its screen since `task-1912`; a terminal **tab**
    /// came back as a bare shell in the project's root with an empty screen, because the only things this
    /// module had ever held about one were a count and a name.
    pub terminals: Vec<RememberedTerminal>,
    /// True when the run tile was the one showing at the bottom of the window.
    ///
    /// The runs themselves are deliberately not remembered, for the reason the terminals are not:
    /// what a program was doing when the window closed is gone. Unlike the terminals, no fresh ones
    /// are started either — a shell is a place to type and a run is something that was *started*,
    /// and restarting somebody's dev server because they closed the window would be a surprise.
    pub run_visible: bool,
    /// The name of the configuration the run widget had chosen. Empty when none was.
    ///
    /// Per-person, so it is here rather than in `run-configurations.conf`: which of the project's
    /// configurations you were last working with is a fact about you, and the file the project
    /// shares holds what somebody chose to keep. `task-1683` §4.2.
    pub run_selected: String,
    /// Where the window was and how big it was, and whether it was maximised.
    ///
    /// The **project's** rather than the person's, because Unluminous's windows are one per project: a
    /// geometry kept per person would open the second window exactly on top of the first. It is read
    /// in `main.rs`, before the window is built, and applied through the `ViewportBuilder`.
    pub window: Option<WindowPlace>,
}

/// Where a window was, in points, as egui reports and takes them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowPlace {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub maximised: bool,
}

impl WindowPlace {
    /// The smallest a remembered window is allowed to be, which is the smallest `main.rs` asks the
    /// platform for. A state file naming something tinier is a state file to ignore.
    const SMALLEST: f32 = 400.0;

    /// True when this is a size a window could sensibly be opened at again.
    ///
    /// The size is checked and the position is not, deliberately: a window that opens too large is a
    /// window somebody can shrink, and there is no honest way to know from here which screens are
    /// plugged in — the platform clamps a position that is off every one of them, which is the
    /// behaviour a person expects anyway.
    pub fn is_sensible(&self) -> bool {
        [self.x, self.y, self.width, self.height].iter().all(|value| value.is_finite())
            && self.width >= Self::SMALLEST
            && self.height >= Self::SMALLEST
    }
}

impl ProjectState {
    /// The state a project that has never been opened in Unluminous has.
    pub fn new() -> Self {
        Self { explorer_visible: true, editor_visible: true, ..Self::default() }
    }

    /// Whether the only thing that changed between these two is where the window is.
    ///
    /// **The one change that arrives on every frame of a gesture.** Every other field here moves when
    /// somebody opens a tab, expands a folder or drags a divider, and is worth the three file writes
    /// at once; the geometry moves sixty times a second for as long as the window is being dragged.
    /// `UnluminousApp::window_still_since` is what waits for it and why. `task-2009`.
    ///
    /// Written as a comparison against a copy with the other's geometry in it rather than as a list of
    /// the other twenty fields, because a list is a thing to keep up to date and a field added later
    /// would be the one it forgot.
    pub fn differs_only_in_the_window(&self, other: &Self) -> bool {
        self.window != other.window && Self { window: other.window, ..self.clone() } == *other
    }
}

/// Where the state folder for `root` is.
pub fn folder(root: &Path) -> PathBuf {
    root.join(FOLDER)
}

/// Read what was left open in `root`.
pub fn load(root: &Path) -> ProjectState {
    let folder = folder(root);
    let values = match std::fs::read_to_string(folder.join(WORKSPACE_FILE)) {
        Ok(text) => Values::parse(&text),
        Err(_) => Values::new(),
    };
    let mut state = ProjectState::new();
    if let Some(on) = values.flag("explorer.visible") {
        state.explorer_visible = on;
    }
    if let Some(on) = values.flag("editor.visible") {
        state.editor_visible = on;
    }
    if let Some(on) = values.flag("terminal.visible") {
        state.terminal_visible = on;
    }
    if let Some(on) = values.flag("space.visible") {
        state.space_visible = on;
    }
    if let Some(on) = values.flag("run.visible") {
        state.run_visible = on;
    }
    if let Some(name) = values.text("run.selected") {
        state.run_selected = name.trim().to_owned();
    }
    if let Some(count) = values.number("terminal.tabs") {
        state.terminal_tabs = (count.max(0.0) as usize).min(16);
    }
    state.plugin_tabs =
        read_names(&std::fs::read_to_string(folder.join(PLUGIN_TABS_FILE)).unwrap_or_default());
    state.plugin_tabs.truncate(OPEN_LIMIT);
    state.terminals = read_terminals(
        root,
        &std::fs::read_to_string(folder.join(TERMINAL_TABS_FILE)).unwrap_or_default(),
    );
    state.terminals.resize(state.terminal_tabs, RememberedTerminal::default());
    state.terminal_tab_names =
        state.terminals.iter().map(|terminal| terminal.name.clone()).collect();
    state.window = read_window(&values);
    state.open_files = read_paths(root, &folder.join(OPEN_FILES_FILE));
    state.open_files.truncate(OPEN_LIMIT);
    let mut panes = read_numbers(values.text("files.panes"));
    panes.resize(state.open_files.len(), 0);
    let mut scrolls = read_fractions_raw(values.text("files.scrolls"));
    scrolls.resize(state.open_files.len(), 0.0);
    let mut carets = read_numbers(values.text("files.carets"));
    carets.resize(state.open_files.len(), 0);
    // A file that has since been removed or renamed is left out, so a project does not open with a
    // tab pointing at nothing — and the pane it was in goes with it, in the same pass. Filtering the
    // lists one after the other is what would slide every pane number along by one, and there are
    // four of them now.
    let (mut files, mut kept) = (Vec::new(), Vec::new());
    let (mut where_it_was, mut where_the_caret_was) = (Vec::new(), Vec::new());
    let rows =
        std::mem::take(&mut state.open_files).into_iter().zip(panes).zip(scrolls).zip(carets);
    for (((path, pane), scroll), caret) in rows {
        if path.is_file() {
            files.push(path);
            kept.push(pane);
            where_it_was.push(scroll.max(0.0));
            where_the_caret_was.push(caret);
        }
    }
    state.open_files = files;
    state.file_panes = kept;
    state.file_scrolls = where_it_was;
    state.file_carets = where_the_caret_was;
    state.pane_widths = read_fractions(values.text("files.pane-widths"));
    state.active_pane = values.number("files.pane").map(|pane| pane.max(0.0) as usize).unwrap_or(0);
    state.expanded_folders = read_paths(root, &folder.join(EXPANDED_FILE));
    state.expanded_folders.retain(|path| path.is_dir());
    state.active_file = values
        .number("files.active")
        .map(|index| index.max(0.0) as usize)
        .unwrap_or(0)
        .min(state.open_files.len().saturating_sub(1));
    state
}

/// Write what is open in `root` now.
///
/// A failure is reported on the error output and otherwise ignored, for the reason the settings file
/// already gives: a read-only folder, or a full disk, is not a reason to stop editing. A project on a
/// disk that cannot be written to still works — it just does not remember.
pub fn save(root: &Path, state: &ProjectState) {
    let folder = folder(root);
    if let Err(problem) = std::fs::create_dir_all(&folder) {
        eprintln!("Unluminous could not make {}: {problem}", folder.display());
        return;
    }
    let mut values = Values::new();
    values.set("explorer.visible", flag(state.explorer_visible));
    values.set("editor.visible", flag(state.editor_visible));
    values.set("terminal.visible", flag(state.terminal_visible));
    values.set("space.visible", flag(state.space_visible));
    values.set("terminal.tabs", state.terminal_tabs.to_string());

    if let Some(place) = state.window.filter(WindowPlace::is_sensible) {
        values.set("window.x", format!("{:.0}", place.x));
        values.set("window.y", format!("{:.0}", place.y));
        values.set("window.width", format!("{:.0}", place.width));
        values.set("window.height", format!("{:.0}", place.height));
        values.set("window.maximised", flag(place.maximised));
    }
    values.set("run.visible", flag(state.run_visible));
    // Written only once something has been chosen, so a project that has never run anything has no
    // line saying it chose nothing — the rule `terminal.shell` already keeps in the settings file.
    if !state.run_selected.is_empty() {
        values.set("run.selected", state.run_selected.clone());
    }
    values.set("files.active", state.active_file.to_string());
    values.set("files.panes", numbers_text(&state.file_panes));
    values.set(
        "files.scrolls",
        state.file_scrolls.iter().map(|at| format!("{at:.1}")).collect::<Vec<_>>().join(","),
    );
    values.set("files.carets", numbers_text(&state.file_carets));
    values.set("files.pane", state.active_pane.to_string());
    values.set(
        "files.pane-widths",
        state.pane_widths.iter().map(|share| format!("{share:.4}")).collect::<Vec<_>>().join(","),
    );
    let heading =
        "# What Unluminous left open in this project. Written by Unluminous, and safe to delete.";
    write(&folder.join(WORKSPACE_FILE), &values.to_text_headed(heading));
    write(&folder.join(OPEN_FILES_FILE), &paths_text(root, &state.open_files));
    write(&folder.join(EXPANDED_FILE), &paths_text(root, &state.expanded_folders));
    // Only written once somebody has named a tab, so a project whose terminals are all called after
    // their programs is left with no file at all — the rule `run.selected` already keeps about a
    // value nobody has chosen.
    let terminals = terminals_to_write(state);
    if terminals.iter().any(|terminal| terminal != &RememberedTerminal::default()) {
        write(&folder.join(TERMINAL_TABS_FILE), &terminals_text(root, &terminals));
    }
    // Only written once a plugin tab has been open, for the same reason: a project that has never had one is
    // left with no file at all rather than an empty one.
    if !state.plugin_tabs.is_empty() {
        write(&folder.join(PLUGIN_TABS_FILE), &names_text(&state.plugin_tabs));
    }
}

/// A comma separated list of whole numbers, for the pane a tab is in.
///
/// One line rather than a numbered key each, because it is a list as long as `open-files.txt` and
/// sixty `files.panes.07 = 1` lines would be a worse file to read than one line of sixty numbers.
fn numbers_text(numbers: &[usize]) -> String {
    numbers.iter().map(usize::to_string).collect::<Vec<_>>().join(",")
}

/// Read that list back. Anything that is not a number is read as zero rather than making the whole
/// line unreadable, which is the rule `services::store` keeps about a settings file.
fn read_numbers(text: Option<&str>) -> Vec<usize> {
    let Some(text) = text else {
        return Vec::new();
    };
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<usize>().unwrap_or(0))
        .collect()
}

/// The names of the terminal tabs, one a line, blank for a tab that was never renamed.
///
/// A file of its own rather than a comma list in the conf, which is what this module already does
/// with the two lists of paths and for the same reason: a person can call a tab anything at all, and
/// a name is the one value here that cannot be relied on not to hold the separator. A line cannot
/// hold a newline, so there is nothing to escape.
fn names_text(names: &[String]) -> String {
    names.iter().map(|name| format!("{name}\n")).collect()
}

/// Read that list back, keeping the blanks: a tab with no name of its own still counts.
fn read_names(text: &str) -> Vec<String> {
    text.lines().map(|line| line.trim().to_owned()).collect()
}

/// What to write for each tab, which is [`ProjectState::terminals`] with the names list filled in.
///
/// **The two lists are reconciled here rather than at each caller**, because the only thing that ever
/// writes a `ProjectState` by hand is a test, and a test that set the names and not the richer list would
/// otherwise silently write no names at all.
fn terminals_to_write(state: &ProjectState) -> Vec<RememberedTerminal> {
    let mut terminals = state.terminals.clone();
    terminals.resize(state.terminal_tabs.max(state.terminals.len()), RememberedTerminal::default());
    for (index, name) in state.terminal_tab_names.iter().enumerate() {
        if let Some(terminal) = terminals.get_mut(index) {
            if terminal.name.is_empty() {
                terminal.name = name.clone();
            }
        }
    }
    terminals
}

/// The terminal tabs, as the numbered key and value list `space.conf` writes a canvas with.
///
/// **A block a tab rather than a name a line**, which is what this file held before `task-1945`. A name is
/// still the one value a person types, so it keeps a line of its own that cannot need escaping — the rest
/// are a path, a command line and an identifier.
fn terminals_text(root: &Path, terminals: &[RememberedTerminal]) -> String {
    let mut values = Values::new();
    for (index, terminal) in terminals.iter().enumerate() {
        values.set(&format!("terminal.{index}.name"), terminal.name.clone());
        values.set(&format!("terminal.{index}.folder"), relative_text(root, &terminal.folder));
    }
    values.to_text_headed("Unluminous: what each terminal tab in this project was doing.")
}

/// Read that back, and read a file written before `task-1945` as the list of names it was.
///
/// **A file with no `=` in it is the old one**, which is the cheapest question that cannot be wrong: every
/// line of the new file is `terminal.N.key = value` and a name a person typed is one line of prose. A
/// project last opened by an older build therefore keeps its tab names and gains empty folders and
/// commands, which is exactly what that build knew about them.
fn read_terminals(root: &Path, text: &str) -> Vec<RememberedTerminal> {
    let old_style = !text.lines().any(|line| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with('#') && line.contains('=')
    });
    if old_style {
        return read_names(text)
            .into_iter()
            .map(|name| RememberedTerminal { name, ..RememberedTerminal::default() })
            .collect();
    }
    let values = Values::parse(text);
    let mut terminals = Vec::new();
    for index in 0..OPEN_LIMIT {
        let at = |key: &str| {
            values.text(&format!("terminal.{index}.{key}")).unwrap_or_default().to_owned()
        };
        let name = at("name");
        let folder = at("folder");
        let empty = name.is_empty() && folder.is_empty();
        // A missing block ends the list rather than leaving a hole in it, which is the rule
        // `space::store` keeps about a numbered list: the numbers are written by this file and are
        // contiguous, so the first gap is the end.
        if empty && values.text(&format!("terminal.{index}.name")).is_none() {
            break;
        }
        let folder = match folder.is_empty() {
            true => String::new(),
            false => absolute_text(root, &folder),
        };
        terminals.push(RememberedTerminal { name, folder });
    }
    terminals
}

/// A folder written the way every other path in this module is: relative when it is inside the project.
fn relative_text(root: &Path, folder: &str) -> String {
    if folder.is_empty() {
        return String::new();
    }
    let path = PathBuf::from(folder);
    match path.strip_prefix(root) {
        Ok(inside) if !inside.as_os_str().is_empty() => inside.to_string_lossy().into_owned(),
        Ok(_) => ".".to_owned(),
        Err(_) => folder.to_owned(),
    }
}

/// And read back: a relative one is inside the project, an absolute one is wherever it says.
fn absolute_text(root: &Path, folder: &str) -> String {
    let path = PathBuf::from(folder);
    match path.is_absolute() {
        true => folder.to_owned(),
        false => root.join(path).to_string_lossy().into_owned(),
    }
}

/// The same as [`read_fractions`], without the rule that throws away anything that is not positive.
///
/// A scroll position of zero is the top of a file, which is the commonest place to be, so the list of
/// them has to keep its zeros and stay as long as the list of files beside it.
fn read_fractions_raw(text: Option<&str>) -> Vec<f32> {
    let Some(text) = text else {
        return Vec::new();
    };
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<f32>().unwrap_or(0.0))
        .map(|at| if at.is_finite() { at } else { 0.0 })
        .collect()
}

/// Where the window was, when the file says.
///
/// All five values or none: a half-written geometry is a geometry to ignore, because a window put at
/// a remembered position with a default size is a window in a place nobody left it.
fn read_window(values: &Values) -> Option<WindowPlace> {
    let place = WindowPlace {
        x: values.number("window.x")?,
        y: values.number("window.y")?,
        width: values.number("window.width")?,
        height: values.number("window.height")?,
        maximised: values.flag("window.maximised").unwrap_or(false),
    };
    place.is_sensible().then_some(place)
}

/// The same for the widths, which are fractions.
fn read_fractions(text: Option<&str>) -> Vec<f32> {
    let Some(text) = text else {
        return Vec::new();
    };
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<f32>().unwrap_or(0.0))
        .filter(|share| share.is_finite() && *share > 0.0)
        .collect()
}

fn flag(on: bool) -> &'static str {
    if on {
        "true"
    } else {
        "false"
    }
}

fn write(path: &Path, text: &str) {
    if let Err(problem) = crate::services::store::write_atomically(path, text.as_bytes()) {
        eprintln!("Unluminous could not write {}: {problem}", path.display());
    }
}

/// One path a line, relative to the project where it can be.
fn paths_text(root: &Path, paths: &[PathBuf]) -> String {
    paths
        .iter()
        .take(OPEN_LIMIT)
        .map(|path| format!("{}\n", relative(root, path).display()))
        .collect()
}

/// Read one path a line, turning each back into a full path.
fn read_paths(root: &Path, file: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let path = absolute(root, Path::new(line));
        if !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// `path` as it is written down: relative to the project when it is inside it.
pub fn relative(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).map(Path::to_path_buf).unwrap_or_else(|_| path.to_path_buf())
}
/// Enough of a file's metadata to tell that it has changed, without reading it.
///
/// The modified time and the length together. The time alone is not enough on a file system whose
/// stamps have a coarse resolution, and the length alone misses an edit that kept it — neither is a
/// hash, which would mean reading every byte of every open file to answer a question that is usually
/// no.
///
/// It lives here rather than on the tab that first needed it because it is a fact about a **file**
/// rather than about a window: `task-1794` gave it a second reader in `services::breakpoint_store`,
/// which has to notice that `.unluminous/breakpoints.conf` was put back underneath a running window, and
/// a service reaching up into `app` for it would be the dependency pointing the wrong way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskStamp {
    pub modified: Option<std::time::SystemTime>,
    pub len: u64,
}

impl DiskStamp {
    /// What is on disk now, or nothing when there is nothing there to measure.
    pub fn of(path: &Path) -> Option<Self> {
        let data = std::fs::metadata(path).ok()?;
        Some(Self { modified: data.modified().ok(), len: data.len() })
    }
}

/// The other way round: what a written path means now.
pub fn absolute(root: &Path, path: &Path) -> PathBuf {
    // Through `paths::native`, because `join` does not normalise: on Windows a relative path written
    // with `/` — which is how every one of these files is written, and how an agent types one on the
    // command line — makes `C:\project\src/report.rs`. Unluminous itself cannot tell the difference, since
    // `Path` compares components, but another program can, and `task-1794` measured a debug adapter
    // silently binding no breakpoint because of it. A path is normalised where it is built, so that a
    // mixed one is never written down in the first place — `plain`'s rule about the verbatim prefix,
    // kept about the separator.
    unluminous_terminal::paths::native(&if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-2009`: the geometry is the one field that changes on every frame of a gesture, so it is
    /// the one the window waits for. Everything else is written the frame it changes.
    #[test]
    fn a_change_that_is_only_where_the_window_is_is_told_apart_from_every_other_change() {
        let here = WindowPlace { x: 10.0, y: 20.0, width: 900.0, height: 600.0, maximised: false };
        let moved = WindowPlace { x: 240.0, ..here };
        let one = ProjectState { window: Some(here), ..ProjectState::new() };
        let dragged = ProjectState { window: Some(moved), ..ProjectState::new() };
        assert!(one.differs_only_in_the_window(&dragged));
        assert!(dragged.differs_only_in_the_window(&one), "and it reads the same way round");

        let same = ProjectState { window: Some(here), ..ProjectState::new() };
        assert!(!one.differs_only_in_the_window(&same), "nothing changed at all");

        let and_a_folder = ProjectState {
            window: Some(moved),
            expanded_folders: vec![PathBuf::from("chapters")],
            ..ProjectState::new()
        };
        assert!(
            !one.differs_only_in_the_window(&and_a_folder),
            "a folder opened out is written at once, whatever the window did"
        );

        let a_folder_alone =
            ProjectState { expanded_folders: vec![PathBuf::from("chapters")], ..one.clone() };
        assert!(!one.differs_only_in_the_window(&a_folder_alone));
    }

    fn project(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(name);
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("chapters")).expect("make the project");
        std::fs::write(root.join("readme.md"), "# readme\n").expect("write readme.md");
        std::fs::write(root.join("chapters/one.md"), "# one\n").expect("write one.md");
        root
    }

    #[test]
    fn a_project_that_has_never_been_opened_has_the_explorer_showing_and_nothing_else() {
        let root = project("unluminous-project-state-fresh");
        let state = load(&root);
        assert!(state.open_files.is_empty());
        assert!(state.expanded_folders.is_empty());
        assert!(state.explorer_visible, "the explorer starts open");
        assert!(!state.terminal_visible);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn what_was_open_is_what_comes_back() {
        let root = project("unluminous-project-state-round-trip");
        let state = ProjectState {
            plugin_tabs: Vec::new(),
            open_files: vec![root.join("readme.md"), root.join("chapters/one.md")],
            active_file: 1,
            file_panes: vec![0, 0],
            file_scrolls: vec![0.0, 412.5],
            file_carets: vec![0, 1180],
            pane_widths: vec![1.0],
            active_pane: 0,
            expanded_folders: vec![root.join("chapters")],
            explorer_visible: false,
            editor_visible: true,
            space_visible: true,
            terminal_visible: true,
            terminal_tabs: 2,
            terminal_tab_names: vec!["build".to_owned(), String::new()],
            terminals: vec![
                RememberedTerminal {
                    name: "build".to_owned(),
                    folder: root.join("chapters").to_string_lossy().into_owned(),
                },
                RememberedTerminal::default(),
            ],
            run_visible: false,
            run_selected: "Dev server".to_owned(),
            window: Some(WindowPlace {
                x: 260.0,
                y: 260.0,
                width: 1400.0,
                height: 900.0,
                maximised: false,
            }),
        };
        save(&root, &state);
        assert_eq!(load(&root), state);
        std::fs::remove_dir_all(&root).ok();
    }

    /// `task-1693`: "the same state, including open files, scroll position".
    #[test]
    fn where_each_file_was_being_read_comes_back_with_it() {
        let root = project("unluminous-project-state-scroll");
        let state = ProjectState {
            plugin_tabs: Vec::new(),
            open_files: vec![root.join("readme.md"), root.join("chapters/one.md")],
            file_panes: vec![0, 0],
            file_scrolls: vec![0.0, 412.5],
            file_carets: vec![0, 1180],
            ..ProjectState::new()
        };
        save(&root, &state);
        let read = load(&root);
        assert_eq!(read.file_scrolls, vec![0.0, 412.5], "the top of a file is a place too");
        assert_eq!(read.file_carets, vec![0, 1180]);
    }

    /// The four lists are filtered in one pass, so a file that has gone takes its own scroll and
    /// caret with it rather than sliding everything else along by one.
    #[test]
    fn a_scroll_goes_with_the_file_it_belongs_to_when_that_file_has_gone() {
        let root = project("unluminous-project-state-scroll-gone");
        let state = ProjectState {
            plugin_tabs: Vec::new(),
            open_files: vec![
                root.join("gone.md"),
                root.join("readme.md"),
                root.join("chapters/one.md"),
            ],
            file_panes: vec![0, 1, 1],
            file_scrolls: vec![99.0, 412.5, 7.5],
            file_carets: vec![99, 1180, 7],
            ..ProjectState::new()
        };
        save(&root, &state);
        let read = load(&root);
        assert_eq!(read.open_files.len(), 2, "the file that is not there is left out");
        assert_eq!(read.file_panes, vec![1, 1]);
        assert_eq!(read.file_scrolls, vec![412.5, 7.5]);
        assert_eq!(read.file_carets, vec![1180, 7]);
    }

    /// A name somebody typed is the one thing about a terminal that survives its shell.
    #[test]
    fn the_names_a_person_gave_the_terminals_come_back() {
        let root = project("unluminous-project-state-terminal-names");
        let state = ProjectState {
            terminal_visible: true,
            terminal_tabs: 3,
            terminal_tab_names: vec!["build".to_owned(), String::new(), "server".to_owned()],
            ..ProjectState::new()
        };
        save(&root, &state);
        let read = load(&root);
        assert_eq!(
            read.terminal_tab_names,
            vec!["build".to_owned(), String::new(), "server".to_owned()],
            "a tab nobody named stays blank rather than being given a neighbour's name"
        );
    }

    /// `task-1945`: a terminal tab comes back in the folder its shell was in.
    ///
    /// *"both nodes and normal terminals should be restored to exactly where they were."* Until this ticket
    /// a tab's folder was not written down at all, so every tab reopened in the project's own root however
    /// far somebody had `cd`'d.
    #[test]
    fn a_terminal_tab_comes_back_in_the_folder_its_shell_was_in() {
        let root = project("unluminous-project-state-terminal-folders");
        let inside = root.join("chapters");
        let state = ProjectState {
            terminal_visible: true,
            terminal_tabs: 2,
            terminals: vec![
                RememberedTerminal {
                    name: String::new(),
                    folder: inside.to_string_lossy().into_owned(),
                },
                RememberedTerminal {
                    name: "build".to_owned(),
                    folder: "C:/somewhere/else".to_owned(),
                },
            ],
            ..ProjectState::new()
        };
        save(&root, &state);
        let read = load(&root);
        assert_eq!(read.terminals.len(), 2);
        assert_eq!(
            std::path::PathBuf::from(&read.terminals[0].folder),
            inside,
            "a folder inside the project is written relative and read back absolute"
        );
        assert_eq!(
            read.terminals[1].folder, "C:/somewhere/else",
            "and one outside it is kept in full"
        );
        assert_eq!(read.terminals[1].name, "build");
        assert_eq!(read.terminal_tab_names, vec![String::new(), "build".to_owned()]);
        std::fs::remove_dir_all(&root).ok();
    }

    /// A project written before `task-1945` reads as the list of names it was.
    ///
    /// The file held one name a line and nothing else, and a project last opened by an older build must not
    /// lose the names somebody typed just because the file grew a format.
    #[test]
    fn a_project_written_by_an_older_build_keeps_its_terminal_names() {
        let root = project("unluminous-project-state-terminal-old-file");
        let folder = root.join(FOLDER);
        std::fs::create_dir_all(&folder).expect("the state folder");
        std::fs::write(folder.join(TERMINAL_TABS_FILE), "build\n\nserver\n").expect("the old file");
        let mut values = Values::new();
        values.set("terminal.visible", "true");
        values.set("terminal.tabs", "3");
        std::fs::write(folder.join(WORKSPACE_FILE), values.to_text()).expect("the workspace file");

        let read = load(&root);
        assert_eq!(
            read.terminal_tab_names,
            vec!["build".to_owned(), String::new(), "server".to_owned()]
        );
        assert!(
            read.terminals.iter().all(|terminal| terminal.folder.is_empty()),
            "an older build knew no folders, so every tab opens where it always did"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// A project whose terminals are all named after their programs writes no file at all, which is
    /// the rule `run.selected` keeps about a value nobody has chosen.
    #[test]
    fn a_project_that_named_no_terminal_writes_no_names_file() {
        let root = project("unluminous-project-state-no-names");
        let state = ProjectState { terminal_tabs: 2, ..ProjectState::new() };
        save(&root, &state);
        assert!(!folder(&root).join(TERMINAL_TABS_FILE).exists());
        assert_eq!(load(&root).terminal_tab_names, vec![String::new(), String::new()]);
    }

    /// `task-1693`: "in the same location", which is the window's own geometry.
    #[test]
    fn where_the_window_was_comes_back_and_nonsense_does_not() {
        let root = project("unluminous-project-state-window");
        let place =
            WindowPlace { x: -20.0, y: 40.0, width: 1400.0, height: 900.0, maximised: true };
        let state = ProjectState { window: Some(place), ..ProjectState::new() };
        save(&root, &state);
        assert_eq!(
            load(&root).window,
            Some(place),
            "a negative x is a second screen, not nonsense"
        );

        // A window smaller than the platform's own minimum is a state file to ignore rather than a
        // window nobody can use.
        let tiny = ProjectState {
            window: Some(WindowPlace { width: 10.0, height: 10.0, ..place }),
            ..ProjectState::new()
        };
        save(&root, &tiny);
        assert_eq!(load(&root).window, None);
    }

    #[test]
    fn a_split_project_comes_back_split() {
        let root = project("unluminous-project-state-split");
        let state = ProjectState {
            plugin_tabs: Vec::new(),
            open_files: vec![root.join("readme.md"), root.join("chapters/one.md")],
            active_file: 1,
            file_panes: vec![0, 1],
            pane_widths: vec![0.4, 0.6],
            active_pane: 1,
            ..ProjectState::new()
        };
        save(&root, &state);
        let read = load(&root);
        assert_eq!(read.file_panes, vec![0, 1]);
        assert_eq!(read.active_pane, 1);
        assert_eq!(read.pane_widths, vec![0.4, 0.6]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_pane_goes_with_the_file_it_belongs_to_when_that_file_has_gone() {
        // The two lists are filtered in one pass. Filtering them one after the other is what would
        // leave chapters/one.md wearing the pane that belonged to the file before it.
        let root = project("unluminous-project-state-split-missing");
        let state = ProjectState {
            plugin_tabs: Vec::new(),
            open_files: vec![root.join("gone.md"), root.join("chapters/one.md")],
            file_panes: vec![0, 1],
            pane_widths: vec![0.5, 0.5],
            ..ProjectState::new()
        };
        save(&root, &state);
        let read = load(&root);
        assert_eq!(read.open_files, vec![root.join("chapters/one.md")]);
        assert_eq!(read.file_panes, vec![1], "the pane that came back is the one that file was in");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_state_file_written_before_there_were_panes_opens_in_one_pane() {
        let root = project("unluminous-project-state-no-panes");
        std::fs::create_dir_all(folder(&root)).expect("make the state folder");
        std::fs::write(folder(&root).join(WORKSPACE_FILE), "files.active = 0\n")
            .expect("write the old style file");
        std::fs::write(folder(&root).join(OPEN_FILES_FILE), "readme.md\nchapters/one.md\n")
            .expect("write the open files");
        let read = load(&root);
        assert_eq!(read.open_files.len(), 2);
        assert_eq!(read.file_panes, vec![0, 0], "no panes named means one pane holding everything");
        assert!(read.pane_widths.is_empty(), "and no widths, which means share it equally");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_paths_are_written_relative_so_a_project_that_moves_still_opens_its_files() {
        let root = project("unluminous-project-state-relative");
        save(
            &root,
            &ProjectState {
                plugin_tabs: Vec::new(),
                open_files: vec![root.join("chapters/one.md")],
                ..ProjectState::new()
            },
        );
        let written =
            std::fs::read_to_string(folder(&root).join(OPEN_FILES_FILE)).expect("read it");
        assert!(
            !written.contains(&root.display().to_string()),
            "the project's own path should not be in the file, which holds {written:?}"
        );
        assert!(written.contains("one.md"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_file_that_has_since_gone_is_left_out_rather_than_opened_as_nothing() {
        let root = project("unluminous-project-state-missing");
        save(
            &root,
            &ProjectState {
                plugin_tabs: Vec::new(),
                open_files: vec![root.join("readme.md"), root.join("gone.md")],
                active_file: 1,
                ..ProjectState::new()
            },
        );
        let state = load(&root);
        assert_eq!(state.open_files, vec![root.join("readme.md")]);
        assert_eq!(state.active_file, 0, "the index comes back inside the list that is left");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_state_folder_that_cannot_be_read_leaves_the_project_opening_normally() {
        let root = project("unluminous-project-state-rubbish");
        std::fs::create_dir_all(folder(&root)).expect("make the state folder");
        std::fs::write(folder(&root).join(WORKSPACE_FILE), "this is not a settings file")
            .expect("write rubbish");
        let state = load(&root);
        assert!(state.explorer_visible, "the defaults are what a file with nothing in it means");
        assert!(state.open_files.is_empty());
        std::fs::remove_dir_all(&root).ok();
    }
}
