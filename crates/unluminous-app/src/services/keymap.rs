//! Which chord each menu entry is on: the preset a person chose, their own changes on top of it, and what
//! that does to the menus. `task-2286`, and `tasks/task-2286-keymap-tdd.md` is the design.
//!
//! **Every shortcut in Unluminous is a menu entry**, and every menu is built by `app::actions::menus`, so a
//! keymap is one walk over the menus that replaces the shortcut of each entry it says something about. The
//! menu watcher, the macOS menu bar, the command palette, `action list` and the title bar all read the
//! menus, so none of them needed a change of its own.
//!
//! A chord is chosen for an action in this order, and the first that says anything wins (§5.2):
//!
//! 1. the person's own binding, which may be a chord or `none`;
//! 2. the preset's row, on this platform;
//! 3. the chord the menu entry was built with, which is Unluminous's own.
//!
//! Then a chord given by a higher step takes the chord away from any action that got it from a lower one,
//! because the menu watcher fires the first entry that matches and a chord on two entries is a chord one of
//! them can never use. That is IntelliJ's "remove other assignments", made the only answer.
//!
//! Nothing here runs once a frame. [`resolve`] is asked when the settings change, and what it answers is the
//! short list of actions whose chord is not Unluminous's own, which [`apply`] reads per menu entry.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use crate::app::actions::{self, Action, Entry, Menu, MenuState, Shortcut};

/// A named set of chords a person can start from. Only what a preset changes is written down: an action it
/// does not name keeps Unluminous's chord, unless the preset gave that chord to another action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    /// The chords Unluminous ships with, which is the empty table.
    #[default]
    Unluminous,
    /// VS Code's default `keybindings.json`, for the actions Unluminous has.
    VsCode,
    /// IntelliJ's `Windows` keymap, and its `macOS` keymap on a Mac.
    IntelliJ,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Unluminous, Preset::VsCode, Preset::IntelliJ];

    /// The word the settings file and the command line spell it with.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Unluminous => "unluminous",
            Preset::VsCode => "vscode",
            Preset::IntelliJ => "intellij",
        }
    }

    /// What the page calls it.
    pub fn label(self) -> &'static str {
        match self {
            Preset::Unluminous => "Unluminous",
            Preset::VsCode => "VS Code",
            Preset::IntelliJ => "IntelliJ",
        }
    }

    /// Read a value, or nothing when the file holds something this version does not have.
    pub fn parse(name: &str) -> Option<Self> {
        let squashed: String =
            name.trim().to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        match squashed.as_str() {
            "unluminous" | "default" => Some(Preset::Unluminous),
            "vscode" | "code" => Some(Preset::VsCode),
            "intellij" | "intellijidea" | "jetbrains" | "idea" => Some(Preset::IntelliJ),
            _ => None,
        }
    }

    /// The rows of the table, in the order §4 of the design lists them.
    pub fn rows(self) -> &'static [Row] {
        match self {
            Preset::Unluminous => &[],
            Preset::VsCode => VS_CODE,
            Preset::IntelliJ => INTELLIJ,
        }
    }

    /// The chord this preset gives `action` on this platform: `None` when the preset says nothing about it,
    /// `Some(None)` when it takes the chord away.
    pub fn chord_for(self, action: &str, mac: bool) -> Option<Option<Shortcut>> {
        let row = self.rows().iter().find(|row| row.action == action)?;
        let text = if mac { row.mac } else { row.windows };
        if text.is_empty() {
            return None;
        }
        Some(parse_for(text, mac).ok())
    }
}

/// One action's chords in a preset. An empty chord means the preset says nothing about the action on that
/// platform, and `none` means it takes the chord away.
#[derive(Debug, Clone, Copy)]
pub struct Row {
    pub action: &'static str,
    pub windows: &'static str,
    pub mac: &'static str,
}

const fn row(action: &'static str, windows: &'static str, mac: &'static str) -> Row {
    Row { action, windows, mac }
}

/// IntelliJ's `Windows` and `macOS` keymaps, for the actions Unluminous has. §4.1 of the design.
///
/// `git-show-diff` is `none` because IntelliJ's own keymap puts `Ctrl+D` on Duplicate and on Compare and
/// lets the context decide, and Unluminous has no context, so Duplicate keeps it. `new-window` and
/// `reopen-closed-tab` have no chord in IntelliJ, and `Ctrl+Shift+N` is IntelliJ's Go to File.
const INTELLIJ: &[Row] = &[
    row("command-palette", "Ctrl+Shift+A", "Cmd+Shift+A"),
    row("go-to-file", "Ctrl+Shift+N", "Cmd+Shift+O"),
    row("new-window", "none", "none"),
    row("settings", "Ctrl+Alt+S", "Cmd+,"),
    row("find", "Ctrl+F", "Cmd+F"),
    row("replace", "Ctrl+R", "Cmd+R"),
    row("find-next", "F3", "Cmd+G"),
    row("find-previous", "Shift+F3", "Cmd+Shift+G"),
    row("find-in-files", "Ctrl+Shift+F", "Cmd+Shift+F"),
    row("go-to-line", "Ctrl+G", "Cmd+L"),
    row("go-to-definition", "Ctrl+B", "Cmd+B"),
    row("find-references", "Alt+F7", "Option+F7"),
    row("rename-symbol", "Shift+F6", "Shift+F6"),
    row("complete-word", "Ctrl+Space", "Ctrl+Space"),
    row("navigate-back", "Ctrl+Alt+Left", "Cmd+["),
    row("navigate-forward", "Ctrl+Alt+Right", "Cmd+]"),
    row("toggle-line-comment", "Ctrl+/", "Cmd+/"),
    row("toggle-block-comment", "Ctrl+Shift+/", "Cmd+Option+/"),
    row("duplicate-line", "Ctrl+D", "Cmd+D"),
    row("move-line-up", "Alt+Shift+Up", "Option+Shift+Up"),
    row("move-line-down", "Alt+Shift+Down", "Option+Shift+Down"),
    row("join-lines", "Ctrl+Shift+J", "Ctrl+Shift+J"),
    row("go-to-matching-bracket", "Ctrl+Shift+M", "Ctrl+M"),
    row("reopen-closed-tab", "none", "none"),
    row("close-tab", "Ctrl+F4", "Cmd+W"),
    row("next-tab", "Alt+Right", "Ctrl+Shift+]"),
    row("previous-tab", "Alt+Left", "Ctrl+Shift+["),
    row("toggle-explorer", "Alt+1", "Cmd+1"),
    row("toggle-terminal", "Alt+F12", "Option+F12"),
    row("toggle-maximised-pane", "Ctrl+Shift+F12", "Cmd+Shift+F12"),
    row("fold-toggle", "Ctrl+.", "Cmd+."),
    row("fold-all", "Ctrl+Shift+Minus", "Cmd+Shift+Minus"),
    row("fold-none", "Ctrl+Shift+Plus", "Cmd+Shift+Plus"),
    row("run-start", "Shift+F10", "Ctrl+R"),
    row("run-stop", "Ctrl+F2", "Cmd+F2"),
    row("debug-start", "Shift+F9", "Ctrl+D"),
    row("debug-resume", "F9", "Cmd+Option+R"),
    row("debug-step-over", "F8", "F8"),
    row("debug-step-into", "F7", "F7"),
    row("debug-step-out", "Shift+F8", "Shift+F8"),
    row("debug-run-to-cursor", "Alt+F9", "Option+F9"),
    row("debug-toggle-breakpoint", "Ctrl+F8", "Cmd+F8"),
    row("debug-evaluate", "Alt+F8", "Option+F8"),
    row("git-commit", "Ctrl+K", "Cmd+K"),
    row("git-push", "Ctrl+Shift+K", "Cmd+Shift+K"),
    row("git-rollback", "Ctrl+Alt+Z", "Cmd+Option+Z"),
    row("git-show-diff", "none", "none"),
    row("redo", "Ctrl+Shift+Z", "Cmd+Shift+Z"),
    row("save", "Ctrl+S", "Cmd+S"),
    row("quit", "none", "Cmd+Q"),
];

/// VS Code's default keybindings, for the actions Unluminous has. §4.2 of the design.
///
/// A chord sequence such as `Ctrl+K Ctrl+O` is left out, because every shortcut here is one key press.
const VS_CODE: &[Row] = &[
    row("command-palette", "Ctrl+Shift+P", "Cmd+Shift+P"),
    row("go-to-file", "Ctrl+P", "Cmd+P"),
    row("new-window", "Ctrl+Shift+N", "Cmd+Shift+N"),
    row("open-file", "Ctrl+O", "Cmd+O"),
    row("settings", "Ctrl+,", "Cmd+,"),
    row("save", "Ctrl+S", "Cmd+S"),
    row("save-as", "Ctrl+Shift+S", "Cmd+Shift+S"),
    row("close-window", "Ctrl+Shift+W", "Cmd+Shift+W"),
    row("close-tab", "Ctrl+W", "Cmd+W"),
    row("reopen-closed-tab", "Ctrl+Shift+T", "Cmd+Shift+T"),
    row("undo", "Ctrl+Z", "Cmd+Z"),
    row("redo", "Ctrl+Y", "Cmd+Shift+Z"),
    row("find", "Ctrl+F", "Cmd+F"),
    row("replace", "Ctrl+H", "Cmd+Option+F"),
    row("find-next", "F3", "Cmd+G"),
    row("find-previous", "Shift+F3", "Cmd+Shift+G"),
    row("find-in-files", "Ctrl+Shift+F", "Cmd+Shift+F"),
    row("go-to-line", "Ctrl+G", "Ctrl+G"),
    row("go-to-definition", "F12", "F12"),
    row("find-references", "Shift+F12", "Shift+F12"),
    row("rename-symbol", "F2", "F2"),
    row("complete-word", "Ctrl+Space", "Ctrl+Space"),
    row("navigate-back", "Alt+Left", "Ctrl+Minus"),
    row("navigate-forward", "Alt+Right", "Ctrl+Shift+Minus"),
    row("toggle-line-comment", "Ctrl+/", "Cmd+/"),
    row("toggle-block-comment", "Shift+Alt+A", "Shift+Option+A"),
    row("duplicate-line", "Shift+Alt+Down", "Shift+Option+Down"),
    row("move-line-up", "Alt+Up", "Option+Up"),
    row("move-line-down", "Alt+Down", "Option+Down"),
    row("join-lines", "", "Ctrl+J"),
    row("go-to-matching-bracket", "Ctrl+Shift+\\", "Cmd+Shift+\\"),
    row("toggle-explorer", "Ctrl+B", "Cmd+B"),
    row("toggle-terminal", "Ctrl+`", "Ctrl+`"),
    row("next-tab", "Ctrl+Tab", "Ctrl+Tab"),
    row("previous-tab", "Ctrl+Shift+Tab", "Ctrl+Shift+Tab"),
    row("split-right", "Ctrl+\\", "Cmd+\\"),
    row("increase-font-size", "Ctrl+Plus", "Cmd+Plus"),
    row("decrease-font-size", "Ctrl+Minus", "Cmd+Minus"),
    row("view-preview", "Ctrl+Shift+V", "Cmd+Shift+V"),
    row("fold-toggle", "Ctrl+Shift+[", "Cmd+Option+["),
    row("fold-none", "Ctrl+Shift+]", "Cmd+Option+]"),
    row("run-start", "Ctrl+F5", "Ctrl+F5"),
    row("debug-start", "F5", "F5"),
    row("debug-resume", "F5", "F5"),
    row("debug-stop", "Shift+F5", "Shift+F5"),
    row("debug-step-over", "F10", "F10"),
    row("debug-step-into", "F11", "F11"),
    row("debug-step-out", "Shift+F11", "Shift+F11"),
    row("debug-toggle-breakpoint", "F9", "F9"),
    row("quit", "Ctrl+Q", "Cmd+Q"),
];

/// The pairs a preset may put on one chord, because the menus make sure only one of the two is ever
/// enabled: `Start Debugging` is dimmed while a session runs and `Resume` is dimmed while none is. VS Code's
/// `F5` is the one, and a test checks that every other chord in every preset is used once.
pub const SHARED_BY_STATE: [(&str, &str); 1] = [("debug-start", "debug-resume")];

/// The actions whose menu entry is not read from the keyboard by the menu watcher, but whose chord is read
/// somewhere else that asks the keymap: `app::code_editing::route_the_line_move_keys`.
pub const READ_ELSEWHERE: [&str; 2] = ["move-line-up", "move-line-down"];

/// True on the platform where `Cmd` and `Ctrl` are two keys.
pub fn is_mac() -> bool {
    cfg!(target_os = "macos")
}

/// Read a chord as people write it, for this platform. See [`parse_for`].
pub fn parse(text: &str) -> Result<Shortcut, String> {
    parse_for(text, is_mac())
}

/// Read a chord: modifiers then the key, joined with `+` or `-`, in any case.
///
/// `Cmd`, `Command`, `Ctrl` and `Control` are all the one control key off macOS, which is how every chord in
/// Unluminous is held, so a settings file written on a Mac means the same on Windows. On macOS `Cmd` is the
/// Apple key and `Ctrl` the control key.
pub fn parse_for(text: &str, mac: bool) -> Result<Shortcut, String> {
    let parts = split_chord(text);
    let Some((key_word, modifiers)) = parts.split_last() else {
        return Err("A shortcut needs a key, such as Ctrl+Shift+K.".to_owned());
    };
    let key = key_from_word(key_word)
        .ok_or_else(|| format!("`{key_word}` is not a key Unluminous knows."))?;
    let mut chord = Shortcut::plain(key);
    for word in modifiers {
        match word.to_lowercase().as_str() {
            "cmd" | "command" | "super" | "meta" | "win" => chord.command = true,
            "ctrl" | "control" | "ctl" => match mac {
                true => chord.ctrl = true,
                false => chord.command = true,
            },
            "alt" | "option" | "opt" => chord.alt = true,
            "shift" => chord.shift = true,
            other => return Err(format!("`{other}` is not a modifier. Use Ctrl, Cmd, Alt or Shift.")),
        }
    }
    Ok(chord)
}

/// The words of a chord. `+` joins them when there is one, else `-`, and a doubled joiner at the end is the
/// joiner itself as the key: `Ctrl++` is plus and `ctrl--` is minus.
fn split_chord(text: &str) -> Vec<String> {
    let text = text.trim();
    let joiner = if text.contains('+') { '+' } else { '-' };
    let mut parts: Vec<String> = text.split(joiner).map(|part| part.trim().to_owned()).collect();
    if parts.len() >= 2 && parts.last().is_some_and(String::is_empty) {
        parts.pop();
        if let Some(last) = parts.last_mut() {
            if last.is_empty() {
                *last = joiner.to_string();
            }
        }
    }
    parts.retain(|part| !part.is_empty());
    parts
}

/// A key by any name egui, a menu or a person gives it.
fn key_from_word(word: &str) -> Option<egui::Key> {
    let word = word.trim();
    if let Some(key) = egui::Key::from_name(word) {
        return Some(key);
    }
    if let Some(key) = crate::services::input::key_named(word) {
        return Some(key);
    }
    // `ctrl+k` and `PAGEDOWN`: egui's names are capitalised, so try the word the way egui spells it.
    let lower = word.to_lowercase();
    egui::Key::ALL.iter().copied().find(|key| {
        key.name().to_lowercase() == lower || actions::key_name(*key).to_lowercase() == lower
    })
}

/// How a chord is written down: the way the menus show it on this platform.
pub fn text(chord: &Shortcut) -> String {
    chord.label()
}

/// True when two chords would match the same key press on this platform.
pub fn same(one: &Shortcut, other: &Shortcut) -> bool {
    one.normalised_for(is_mac()) == other.normalised_for(is_mac())
}

/// Something other than a menu entry that reads this chord, so it cannot be bound: §8 of the design.
pub fn reserved(chord: &Shortcut) -> Option<&'static str> {
    reserved_for(chord, is_mac())
}

/// [`reserved`], for a platform named.
pub fn reserved_for(chord: &Shortcut, mac: bool) -> Option<&'static str> {
    use egui::Key;
    let chord = chord.normalised_for(mac);
    let control = chord.command;
    let bare = !chord.command && !chord.ctrl && !chord.alt;
    if control && !chord.alt && !chord.ctrl && !chord.shift && matches!(chord.key, Key::C | Key::X | Key::V)
    {
        return Some("the clipboard, which egui delivers as its own events");
    }
    let typing = matches!(
        chord.key,
        Key::Tab
            | Key::Enter
            | Key::Escape
            | Key::Backspace
            | Key::Delete
            | Key::ArrowUp
            | Key::ArrowDown
            | Key::ArrowLeft
            | Key::ArrowRight
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
            | Key::Space
    );
    if bare && typing {
        return Some("typing, moving the caret and extending a selection");
    }
    let word_keys = matches!(
        chord.key,
        Key::ArrowLeft | Key::ArrowRight | Key::Backspace | Key::Delete | Key::Home | Key::End
    );
    if control && !chord.alt && word_keys {
        return Some("moving and deleting a word, and the start and end of the file");
    }
    if bare && !is_a_function_key(chord.key) {
        return Some("typing: a key with no modifier would fire while you type");
    }
    None
}

/// What else a chord does in a Markdown file, where the editing area reads the formatting chords itself.
/// Bound to a menu entry, the entry wins and the formatting chord stops working there.
pub fn formatting_chord(chord: &Shortcut) -> Option<&'static str> {
    use egui::Key;
    let chord = chord.normalised_for(is_mac());
    if !chord.command || chord.alt || chord.ctrl {
        return None;
    }
    Some(match (chord.key, chord.shift) {
        (Key::B, false) => "bold",
        (Key::I, false) => "italic",
        (Key::U, false) => "underline",
        (Key::X, true) => "strikethrough",
        (Key::L, false) => "align left",
        (Key::E, _) => "centre",
        (Key::R, _) => "align right",
        (Key::J, false) => "justify",
        _ => return None,
    })
}

fn is_a_function_key(key: egui::Key) -> bool {
    key.name().strip_prefix('F').is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

/// One action a chord can be given, as the Keymap page lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Bindable {
    /// `Action::name`, which is what the settings file and the command line call it.
    pub name: String,
    /// What its menu row says.
    pub label: String,
    /// The menus that lead to it, `Edit › Duplicate Line` without the last part.
    pub menu: String,
    /// The chord its menu entry is built with.
    pub default: Option<Shortcut>,
    /// Why it cannot be changed, for Cut, Copy and Paste and the notebook's own keys.
    pub fixed: Option<&'static str>,
}

impl Bindable {
    /// The menu path and the label together, as a row is read out.
    pub fn path(&self) -> String {
        format!("{} \u{203A} {}", self.menu, self.label)
    }
}

/// Every action a chord can be given, built once from the menus with everything switched on, so the page
/// lists `Step Over` while nothing is being debugged.
pub fn bindables() -> &'static [Bindable] {
    static FOUND: OnceLock<Vec<Bindable>> = OnceLock::new();
    FOUND.get_or_init(|| bindables_in(&actions::menus(&everything_applies())))
}

/// The menus as they are when every entry that can ever appear does.
fn everything_applies() -> MenuState {
    MenuState {
        line_comment_applies: true,
        block_comment_applies: true,
        line_edits_apply: true,
        trimming_applies: true,
        can_preview: true,
        folding_applies: true,
        definitions_apply: true,
        symbols_apply: true,
        completion_applies: true,
        run_file_applies: true,
        debug_applies: true,
        notebook_showing: true,
        realm_available: true,
        realm_visible: true,
        in_repository: true,
        has_file: true,
        ..MenuState::default()
    }
}

/// The bindable actions in a set of menus, each once, under the first menu it is found in.
pub fn bindables_in(menus: &[Menu]) -> Vec<Bindable> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for menu in menus {
        walk(&menu.entries, &menu.name, &mut found, &mut seen);
    }
    found
}

fn walk(
    entries: &[Entry],
    path: &str,
    found: &mut Vec<Bindable>,
    seen: &mut std::collections::HashSet<String>,
) {
    for entry in entries {
        match entry {
            Entry::Item { name, action, shortcut, keyboard, .. } => {
                let key = action.name();
                if !reads_back(action, &key) {
                    continue;
                }
                // An action on two menus is listed once. When only the later entry carries a chord, as
                // `Settings` does on `Edit` and not on `Unluminous`, the later one is the one listed, or
                // the page would say Settings has no shortcut.
                if !seen.insert(key.clone()) {
                    let earlier = found.iter_mut().find(|b| b.name == key);
                    if let Some(earlier) = earlier.filter(|b| b.default.is_none() && shortcut.is_some()) {
                        earlier.default = *shortcut;
                        earlier.label = name.clone();
                        earlier.menu = path.to_owned();
                    }
                    continue;
                }
                let fixed = match *keyboard || READ_ELSEWHERE.contains(&key.as_str()) {
                    true => None,
                    false => Some(why_fixed(&key)),
                };
                if fixed.is_some() && shortcut.is_none() {
                    continue;
                }
                found.push(Bindable {
                    name: key,
                    label: name.clone(),
                    menu: path.to_owned(),
                    default: *shortcut,
                    fixed,
                });
            }
            Entry::Submenu { name, entries } => {
                walk(entries, &format!("{path} \u{203A} {name}"), found, seen);
            }
            Entry::Separator => {}
        }
    }
}

/// True when an action is the same whatever the window is showing: its name reads back as itself with no
/// path, so a shortcut can stand for it.
fn reads_back(action: &Action, name: &str) -> bool {
    Action::from_name(name, None).as_ref() == Some(action)
}

fn why_fixed(name: &str) -> &'static str {
    match name {
        "cut" | "copy" | "paste" => "The clipboard: egui delivers these as their own events.",
        _ if name.starts_with("notebook-") => "The notebook reads this key itself, in command mode.",
        _ => "The pane this belongs to reads the key itself.",
    }
}

/// Where an action's chord came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The chord the menu entry is built with.
    Unluminous,
    /// The preset's row.
    Preset,
    /// The person's own binding.
    Yours,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Unluminous => "unluminous",
            Source::Preset => "preset",
            Source::Yours => "yours",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Source::Unluminous => 0,
            Source::Preset => 1,
            Source::Yours => 2,
        }
    }
}

/// One action's chord after everything is settled.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub chord: Option<Shortcut>,
    pub source: Source,
    /// The action whose chord took this one's away, when one did.
    pub lost_to: Option<String>,
}

/// Every bindable action's chord, worked out from a preset and the person's own bindings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub rows: BTreeMap<String, Binding>,
    /// The person's own bindings that name an action this build has not got, or a chord it cannot read.
    pub unknown: Vec<(String, String)>,
}

/// Work out every action's chord. §5.2 of the design.
pub fn table(preset: Preset, yours: &[(String, String)], bindables: &[Bindable]) -> Table {
    table_for(preset, yours, bindables, is_mac())
}

/// [`table`], for a platform named.
pub fn table_for(
    preset: Preset,
    yours: &[(String, String)],
    bindables: &[Bindable],
    mac: bool,
) -> Table {
    let mut table = Table::default();
    let mut own: HashMap<&str, Option<Shortcut>> = HashMap::new();
    for (name, chord) in yours {
        let known = bindables.iter().any(|b| &b.name == name && b.fixed.is_none());
        let read = read_binding(chord, mac);
        match (known, read) {
            (true, Ok(read)) => {
                own.insert(name.as_str(), read);
            }
            _ => table.unknown.push((name.clone(), chord.clone())),
        }
    }
    for bindable in bindables.iter().filter(|b| b.fixed.is_none()) {
        let (chord, source) = match own.get(bindable.name.as_str()) {
            Some(chord) => (*chord, Source::Yours),
            None => match preset.chord_for(&bindable.name, mac) {
                Some(chord) => (chord, Source::Preset),
                None => (bindable.default, Source::Unluminous),
            },
        };
        table.rows.insert(bindable.name.clone(), Binding { chord, source, lost_to: None });
    }
    settle_conflicts(&mut table, mac);
    table
}

/// A binding's text: a chord, or `none`.
fn read_binding(text: &str, mac: bool) -> Result<Option<Shortcut>, String> {
    match text.trim().to_lowercase().as_str() {
        "none" | "" => Ok(None),
        _ => parse_for(text, mac).map(Some),
    }
}

/// Take a chord away from every action that has it from a lower source than another action, and between two
/// of one source keep the one whose name sorts first, unless the pair is in [`SHARED_BY_STATE`].
fn settle_conflicts(table: &mut Table, mac: bool) {
    let mut holders: HashMap<Shortcut, Vec<String>> = HashMap::new();
    for (name, binding) in &table.rows {
        if let Some(chord) = binding.chord {
            holders.entry(chord.normalised_for(mac)).or_default().push(name.clone());
        }
    }
    for names in holders.values().filter(|names| names.len() > 1) {
        let winner = names
            .iter()
            .max_by(|a, b| {
                let rank = |name: &String| table.rows[name].source.rank();
                rank(a).cmp(&rank(b)).then_with(|| b.cmp(a))
            })
            .cloned()
            .unwrap_or_default();
        for name in names.iter().filter(|name| **name != winner) {
            let same_source = table.rows[name].source == table.rows[&winner].source;
            if same_source && shared_by_state(name, &winner) {
                continue;
            }
            if let Some(binding) = table.rows.get_mut(name) {
                binding.chord = None;
                binding.lost_to = Some(winner.clone());
            }
        }
    }
}

fn shared_by_state(one: &str, other: &str) -> bool {
    SHARED_BY_STATE.iter().any(|(a, b)| (*a == one && *b == other) || (*a == other && *b == one))
}

/// The actions whose chord is not the one their menu entry is built with, and what it is instead. This is
/// what the window keeps, and what [`apply`] reads.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Resolved {
    pub changed: HashMap<String, Option<Shortcut>>,
}

impl Resolved {
    /// The chord an action is on, given the one its menu entry is built with.
    pub fn chord(&self, name: &str, built_with: Option<Shortcut>) -> Option<Shortcut> {
        match self.changed.get(name) {
            Some(chord) => *chord,
            None => built_with,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changed.is_empty()
    }
}

/// Settle a preset and the person's own bindings against the bindable actions.
pub fn resolve(preset: Preset, yours: &[(String, String)]) -> Resolved {
    resolved_from(&table(preset, yours, bindables()), bindables())
}

/// The part of a [`Table`] that differs from the menus as built.
pub fn resolved_from(table: &Table, bindables: &[Bindable]) -> Resolved {
    let mut changed = HashMap::new();
    for bindable in bindables {
        if let Some(binding) = table.rows.get(&bindable.name) {
            let differs = match (binding.chord, bindable.default) {
                (Some(one), Some(other)) => one != other,
                (None, None) => false,
                _ => true,
            };
            if differs {
                changed.insert(bindable.name.clone(), binding.chord);
            }
        }
    }
    Resolved { changed }
}

/// Give every menu entry the chord the keymap says it is on.
pub fn apply(menus: &mut [Menu], resolved: &Resolved) {
    if resolved.is_empty() {
        return;
    }
    for menu in menus {
        apply_entries(&mut menu.entries, resolved);
    }
}

/// [`apply`], for a list of entries such as a right click menu.
pub fn apply_entries(entries: &mut [Entry], resolved: &Resolved) {
    if resolved.is_empty() {
        return;
    }
    for entry in entries {
        match entry {
            Entry::Item { action, shortcut, .. } => {
                if let Some(chord) = resolved.changed.get(&action.name()) {
                    *shortcut = *chord;
                }
            }
            Entry::Submenu { entries, .. } => apply_entries(entries, resolved),
            Entry::Separator => {}
        }
    }
}

/// The menus with every chord taken off, for the macOS bar while a chord is being recorded.
pub fn without_chords(menus: &[Menu]) -> Vec<Menu> {
    fn strip(entries: &mut [Entry]) {
        for entry in entries {
            match entry {
                Entry::Item { shortcut, .. } => *shortcut = None,
                Entry::Submenu { entries, .. } => strip(entries),
                Entry::Separator => {}
            }
        }
    }
    let mut menus = menus.to_vec();
    for menu in &mut menus {
        strip(&mut menu.entries);
    }
    menus
}

/// True when an entry's chord is shown in a menu but must not be a macOS key equivalent, because the window
/// reads it itself only while the editing area has the keyboard: [`READ_ELSEWHERE`].
pub fn shown_but_not_a_key_equivalent(action: &Action) -> bool {
    READ_ELSEWHERE.contains(&action.name().as_str())
}

/// What binding a chord to an action did to the person's own bindings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bound {
    /// The actions the chord was taken from, by name.
    pub taken_from: Vec<String>,
}

/// Bind `chord` to `action` in the person's own bindings, taking the chord from whatever has it. `None`
/// takes the action's chord away. Refused for an action that cannot be bound and a reserved chord.
pub fn bind(
    yours: &mut Vec<(String, String)>,
    preset: Preset,
    action: &str,
    chord: Option<Shortcut>,
) -> Result<Bound, String> {
    let bindables = bindables();
    check_bindable(bindables, action)?;
    if let Some(chord) = &chord {
        if let Some(owner) = reserved(chord) {
            return Err(format!("{} belongs to {owner}, so it cannot be a shortcut.", text(chord)));
        }
    }
    let now = table(preset, yours, bindables);
    let mut bound = Bound::default();
    if let Some(chord) = &chord {
        for (name, binding) in &now.rows {
            if name != action && binding.chord.is_some_and(|held| same(&held, chord)) {
                set_own(yours, name, None, preset, bindables);
                bound.taken_from.push(name.clone());
            }
        }
    }
    set_own(yours, action, chord, preset, bindables);
    Ok(bound)
}

fn check_bindable(bindables: &[Bindable], action: &str) -> Result<(), String> {
    match bindables.iter().find(|b| b.name == action) {
        None => Err(format!(
            "`{action}` is not an action a shortcut can be given. `settings keymap` lists them."
        )),
        Some(found) => match found.fixed {
            Some(why) => Err(format!("`{action}` cannot be changed. {why}")),
            None => Ok(()),
        },
    }
}

/// Write one of the person's own bindings, or take it out when it says what the preset already says.
fn set_own(
    yours: &mut Vec<(String, String)>,
    action: &str,
    chord: Option<Shortcut>,
    preset: Preset,
    bindables: &[Bindable],
) {
    yours.retain(|(name, _)| name != action);
    let without = table(preset, yours, bindables);
    let would_be = without.rows.get(action).and_then(|binding| binding.chord);
    let already = match (would_be, chord) {
        (Some(one), Some(other)) => same(&one, &other),
        (None, None) => true,
        _ => false,
    };
    if !already {
        let written = chord.map(|chord| text(&chord)).unwrap_or_else(|| "none".to_owned());
        yours.push((action.to_owned(), written));
    }
}

/// Take the person's own binding for `action` away, so it is on the preset's chord again.
pub fn reset(yours: &mut Vec<(String, String)>, action: &str) {
    yours.retain(|(name, _)| name != action);
}

#[cfg(test)]
mod tests;
