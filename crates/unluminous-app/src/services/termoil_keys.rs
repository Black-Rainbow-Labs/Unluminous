//! Termoil's commands and their keys, inside Unluminous's terminals (`task-2597`).
//!
//! Every terminal grid in the window, whether in the terminal tile, the run tile, a node on the canvas or a
//! ticket's agent, answers the commands Termoil's own window answers for a pane: splitting it, moving between
//! the panes, zooming one, the command palette, the theme picker, the scroll and prompt keys, clearing and
//! resetting. The keys are Termoil's defaults, from `termoil-app/src/config/keys.rs`, so a person who uses
//! Termoil already knows them. While a grid has the keyboard they come before the window's menu chords, which
//! is what makes `Ctrl+Shift+P` the terminal's palette there and Unluminous's everywhere else.
//!
//! Termoil's keys that Unluminous does not take, and why:
//!
//! - `Ctrl+Shift+C` and `Ctrl+Shift+V` (`Cmd+C` and `Cmd+V` on macOS) are copy and paste, which reach the grid
//!   as clipboard events rather than key presses and which the grid already answers (`clipboard_key`). They are
//!   listed so the palette shows them.
//! - The font size keys are Unluminous's pane zoom, which already walks the terminal font size.
//! - `Ctrl+Shift+N`, `F11` and `Ctrl+,` are a new window, full screen and the settings, which Unluminous's own
//!   menus already do for the whole window.
//! - Search, link hints and selecting the last command's output are not built yet.

use crate::app::actions::Shortcut;

/// One command a terminal grid answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TermoilCommand {
    SplitRight,
    SplitDown,
    ClosePane,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    NextPane,
    PreviousPane,
    ZoomPane,
    ResizeLeft,
    ResizeRight,
    ResizeUp,
    ResizeDown,
    NewTab,
    NextTab,
    PreviousTab,
    CommandPalette,
    ChooseTheme,
    PreviousPrompt,
    NextPrompt,
    ScrollPageUp,
    ScrollPageDown,
    ScrollToTop,
    ScrollToBottom,
    ClearScrollback,
    ResetTerminal,
    SelectAll,
    Copy,
    Paste,
}

use TermoilCommand as C;

impl TermoilCommand {
    /// Every command, in the order the palette and the right click menu list them.
    pub const ALL: &'static [TermoilCommand] = &[
        C::SplitRight,
        C::SplitDown,
        C::ClosePane,
        C::ZoomPane,
        C::FocusLeft,
        C::FocusRight,
        C::FocusUp,
        C::FocusDown,
        C::NextPane,
        C::PreviousPane,
        C::ResizeLeft,
        C::ResizeRight,
        C::ResizeUp,
        C::ResizeDown,
        C::NewTab,
        C::NextTab,
        C::PreviousTab,
        C::CommandPalette,
        C::ChooseTheme,
        C::Copy,
        C::Paste,
        C::SelectAll,
        C::PreviousPrompt,
        C::NextPrompt,
        C::ScrollPageUp,
        C::ScrollPageDown,
        C::ScrollToTop,
        C::ScrollToBottom,
        C::ClearScrollback,
        C::ResetTerminal,
    ];

    /// The command's name, which is Termoil's action name where Termoil has one.
    pub fn name(self) -> &'static str {
        match self {
            C::SplitRight => "split-right",
            C::SplitDown => "split-down",
            C::ClosePane => "close-pane",
            C::FocusLeft => "focus-left",
            C::FocusRight => "focus-right",
            C::FocusUp => "focus-up",
            C::FocusDown => "focus-down",
            C::NextPane => "next-pane",
            C::PreviousPane => "previous-pane",
            C::ZoomPane => "zoom-pane",
            C::ResizeLeft => "resize-left",
            C::ResizeRight => "resize-right",
            C::ResizeUp => "resize-up",
            C::ResizeDown => "resize-down",
            C::NewTab => "new-tab",
            C::NextTab => "next-tab",
            C::PreviousTab => "previous-tab",
            C::CommandPalette => "command-palette",
            C::ChooseTheme => "choose-theme",
            C::PreviousPrompt => "previous-prompt",
            C::NextPrompt => "next-prompt",
            C::ScrollPageUp => "scroll-page-up",
            C::ScrollPageDown => "scroll-page-down",
            C::ScrollToTop => "scroll-to-top",
            C::ScrollToBottom => "scroll-to-bottom",
            C::ClearScrollback => "clear-scrollback",
            C::ResetTerminal => "reset-terminal",
            C::SelectAll => "select-all",
            C::Copy => "copy",
            C::Paste => "paste",
        }
    }

    /// What the palette and the menus call it.
    pub fn label(self) -> &'static str {
        match self {
            C::SplitRight => "Split Pane Right",
            C::SplitDown => "Split Pane Down",
            C::ClosePane => "Close Pane",
            C::FocusLeft => "Focus Pane Left",
            C::FocusRight => "Focus Pane Right",
            C::FocusUp => "Focus Pane Above",
            C::FocusDown => "Focus Pane Below",
            C::NextPane => "Next Pane",
            C::PreviousPane => "Previous Pane",
            C::ZoomPane => "Zoom Pane",
            C::ResizeLeft => "Move Divider Left",
            C::ResizeRight => "Move Divider Right",
            C::ResizeUp => "Move Divider Up",
            C::ResizeDown => "Move Divider Down",
            C::NewTab => "New Terminal Tab",
            C::NextTab => "Next Terminal Tab",
            C::PreviousTab => "Previous Terminal Tab",
            C::CommandPalette => "Terminal Command Palette",
            C::ChooseTheme => "Choose Terminal Theme",
            C::PreviousPrompt => "Previous Prompt",
            C::NextPrompt => "Next Prompt",
            C::ScrollPageUp => "Scroll Page Up",
            C::ScrollPageDown => "Scroll Page Down",
            C::ScrollToTop => "Scroll to Top",
            C::ScrollToBottom => "Scroll to Bottom",
            C::ClearScrollback => "Clear Scrollback",
            C::ResetTerminal => "Reset Terminal",
            C::SelectAll => "Select All",
            C::Copy => "Copy",
            C::Paste => "Paste",
        }
    }

    /// The command with this name, ignoring case. Termoil's own `close` is accepted for `close-pane`.
    pub fn from_name(name: &str) -> Option<Self> {
        let wanted = name.trim().to_lowercase();
        if wanted == "close" {
            return Some(C::ClosePane);
        }
        C::ALL.iter().copied().find(|command| command.name() == wanted)
    }

    /// True for the three commands that only a terminal tab can answer: a new tab and moving between tabs.
    pub fn is_about_tabs(self) -> bool {
        matches!(self, C::NewTab | C::NextTab | C::PreviousTab)
    }
}

/// A chord with the control key held (the Apple key's counterpart is never asked for here).
const fn ctrl_shift(key: egui::Key) -> Shortcut {
    Shortcut { key, command: false, shift: true, alt: false, ctrl: true }
}

/// A chord for the platform this was built for. On macOS `command` is the Apple key.
const fn cmd(key: egui::Key, shift: bool, alt: bool, ctrl: bool) -> Shortcut {
    Shortcut { key, command: true, shift, alt, ctrl }
}

/// Termoil's default keys on Windows and Linux, from `default_keybindings` in Termoil.
fn bindings_elsewhere() -> Vec<(Shortcut, TermoilCommand)> {
    use egui::Key as K;
    let alt = |key| Shortcut { key, command: false, shift: false, alt: true, ctrl: false };
    let resize = |key| Shortcut { key, command: false, shift: true, alt: true, ctrl: true };
    let plain_shift = |key| Shortcut { key, command: false, shift: true, alt: false, ctrl: false };
    vec![
        (ctrl_shift(K::T), C::NewTab),
        (ctrl_shift(K::D), C::SplitRight),
        (ctrl_shift(K::E), C::SplitDown),
        (ctrl_shift(K::W), C::ClosePane),
        (Shortcut::control(K::Tab), C::NextTab),
        (ctrl_shift(K::Tab), C::PreviousTab),
        (alt(K::ArrowLeft), C::FocusLeft),
        (alt(K::ArrowRight), C::FocusRight),
        (alt(K::ArrowUp), C::FocusUp),
        (alt(K::ArrowDown), C::FocusDown),
        (ctrl_shift(K::P), C::CommandPalette),
        (ctrl_shift(K::ArrowUp), C::PreviousPrompt),
        (ctrl_shift(K::ArrowDown), C::NextPrompt),
        (ctrl_shift(K::C), C::Copy),
        (ctrl_shift(K::V), C::Paste),
        (plain_shift(K::PageUp), C::ScrollPageUp),
        (plain_shift(K::PageDown), C::ScrollPageDown),
        (ctrl_shift(K::Home), C::ScrollToTop),
        (ctrl_shift(K::End), C::ScrollToBottom),
        (ctrl_shift(K::K), C::ClearScrollback),
        (resize(K::ArrowLeft), C::ResizeLeft),
        (resize(K::ArrowRight), C::ResizeRight),
        (resize(K::ArrowUp), C::ResizeUp),
        (resize(K::ArrowDown), C::ResizeDown),
        // Not one of Termoil's defaults, which leaves both commands to the palette. Unluminous gives them the
        // same chord as the split with `Alt` added, so the theme picker and the zoom are a key away.
        (Shortcut { key: K::Z, command: false, shift: true, alt: false, ctrl: true }, C::ZoomPane),
        (
            Shortcut { key: K::Y, command: false, shift: true, alt: false, ctrl: true },
            C::ChooseTheme,
        ),
    ]
}

/// Termoil's default keys on macOS, from `default_keybindings_macos` in Termoil, with `Option` added to the six
/// that are key equivalents of Unluminous's own menus, and `Cmd+Ctrl+Z` for zoom, because `Cmd+Option+Z` is Rollback. AppKit hands a key equivalent to the menu before the grid
/// sees it, so Termoil's `Cmd+D`, `Cmd+W`, `Cmd+K`, `Ctrl+Tab` and `Ctrl+Shift+Tab` would have been Show Diff,
/// Close Window, Commit and the editor's tabs there (`no_termoil_key_on_macos_is_a_menu_key_equivalent`).
fn bindings_macos() -> Vec<(Shortcut, TermoilCommand)> {
    use egui::Key as K;
    let tab = |shift: bool| Shortcut { key: K::Tab, command: false, shift, alt: true, ctrl: true };
    vec![
        (cmd(K::T, false, false, false), C::NewTab),
        (cmd(K::D, false, true, false), C::SplitRight),
        (cmd(K::D, true, false, false), C::SplitDown),
        (cmd(K::W, false, true, false), C::ClosePane),
        (tab(false), C::NextTab),
        (tab(true), C::PreviousTab),
        (cmd(K::ArrowLeft, false, true, false), C::FocusLeft),
        (cmd(K::ArrowRight, false, true, false), C::FocusRight),
        (cmd(K::ArrowUp, false, true, false), C::FocusUp),
        (cmd(K::ArrowDown, false, true, false), C::FocusDown),
        (cmd(K::P, true, false, false), C::CommandPalette),
        (cmd(K::ArrowUp, false, false, false), C::PreviousPrompt),
        (cmd(K::ArrowDown, false, false, false), C::NextPrompt),
        (cmd(K::C, false, false, false), C::Copy),
        (cmd(K::V, false, false, false), C::Paste),
        (
            Shortcut { key: K::PageUp, command: false, shift: true, alt: false, ctrl: false },
            C::ScrollPageUp,
        ),
        (
            Shortcut { key: K::PageDown, command: false, shift: true, alt: false, ctrl: false },
            C::ScrollPageDown,
        ),
        (cmd(K::Home, false, false, false), C::ScrollToTop),
        (cmd(K::End, false, false, false), C::ScrollToBottom),
        (cmd(K::K, false, true, false), C::ClearScrollback),
        (cmd(K::ArrowLeft, false, false, true), C::ResizeLeft),
        (cmd(K::ArrowRight, false, false, true), C::ResizeRight),
        (cmd(K::ArrowUp, false, false, true), C::ResizeUp),
        (cmd(K::ArrowDown, false, false, true), C::ResizeDown),
        (cmd(K::Z, false, false, true), C::ZoomPane),
        (cmd(K::Y, true, false, false), C::ChooseTheme),
    ]
}

/// The keys for the platform this was built for.
pub fn bindings() -> Vec<(Shortcut, TermoilCommand)> {
    if cfg!(target_os = "macos") {
        bindings_macos()
    } else {
        bindings_elsewhere()
    }
}

/// True for the two commands a grid hears as clipboard events and never as key presses.
fn arrives_as_a_clipboard_event(command: TermoilCommand) -> bool {
    matches!(command, C::Copy | C::Paste)
}

/// The command a key press is, while a grid has the keyboard. Copy and paste are never answered here, because
/// the platform hands them over as clipboard events.
pub fn command_for(key: egui::Key, modifiers: &egui::Modifiers) -> Option<TermoilCommand> {
    bindings()
        .into_iter()
        .filter(|(_, command)| !arrives_as_a_clipboard_event(*command))
        .find(|(chord, _)| chord.matches(key, modifiers))
        .map(|(_, command)| command)
}

/// The key a command is on, for the palette and the menus to show beside it.
pub fn chord_of(command: TermoilCommand) -> Option<Shortcut> {
    bindings().into_iter().find(|(_, held)| *held == command).map(|(chord, _)| chord)
}

/// A chord written the way the window writes one, such as `Ctrl+Shift+D`.
pub fn chord_text(chord: Shortcut) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mac = cfg!(target_os = "macos");
    if chord.ctrl || (chord.command && !mac) {
        parts.push("Ctrl".to_owned());
    }
    if chord.command && mac {
        parts.push("Cmd".to_owned());
    }
    if chord.alt {
        parts.push(if mac { "Option" } else { "Alt" }.to_owned());
    }
    if chord.shift {
        parts.push("Shift".to_owned());
    }
    let key = match chord.key {
        egui::Key::ArrowLeft => "Left".to_owned(),
        egui::Key::ArrowRight => "Right".to_owned(),
        egui::Key::ArrowUp => "Up".to_owned(),
        egui::Key::ArrowDown => "Down".to_owned(),
        other => other.name().to_owned(),
    };
    parts.push(key);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What Windows sends for a chord with control held: both flags, because control is the command key there.
    #[cfg(not(target_os = "macos"))]
    fn windows(ctrl: bool, shift: bool, alt: bool) -> egui::Modifiers {
        egui::Modifiers { ctrl, command: ctrl, shift, alt, mac_cmd: false }
    }

    #[test]
    fn every_command_has_a_name_that_reads_back() {
        for command in TermoilCommand::ALL {
            assert_eq!(TermoilCommand::from_name(command.name()), Some(*command));
            assert!(!command.label().is_empty());
        }
        assert_eq!(TermoilCommand::from_name("close"), Some(TermoilCommand::ClosePane));
        assert_eq!(TermoilCommand::from_name("nothing"), None);
    }

    #[test]
    fn no_two_commands_share_a_key() {
        let held = bindings();
        for (at, (chord, command)) in held.iter().enumerate() {
            for (other_chord, other) in &held[at + 1..] {
                assert!(chord != other_chord, "{command:?} and {other:?} share a key");
            }
        }
        let other = if cfg!(target_os = "macos") { bindings_elsewhere() } else { bindings_macos() };
        assert_eq!(other.len(), held.len(), "both platforms bind the same commands");
    }

    /// On macOS a menu entry's key is a key equivalent, and AppKit gives it to the menu before the grid ever
    /// sees a key press. So no Termoil key on macOS may be a key one of the menus holds, or the menu would
    /// win in the terminal. Checked on every platform, because the menus are built the same everywhere.
    #[test]
    fn no_termoil_key_on_macos_is_a_menu_key_equivalent() {
        use crate::app::actions::{menus, Entry, MenuState};
        fn walk(entries: &[Entry], out: &mut Vec<(String, Shortcut)>) {
            for entry in entries {
                match entry {
                    Entry::Item { name, shortcut: Some(shortcut), .. } => {
                        out.push((name.clone(), *shortcut))
                    }
                    Entry::Submenu { entries, .. } => walk(entries, out),
                    _ => {}
                }
            }
        }
        let mut held = Vec::new();
        for menu in menus(&MenuState::default()) {
            walk(&menu.entries, &mut held);
        }
        let clashes: Vec<String> = bindings_macos()
            .into_iter()
            .filter(|(_, command)| !arrives_as_a_clipboard_event(*command))
            .filter_map(|(chord, command)| {
                let same = |other: &Shortcut| {
                    other.key == chord.key
                        && other.command == chord.command
                        && other.shift == chord.shift
                        && other.alt == chord.alt
                        && other.ctrl == chord.ctrl
                };
                held.iter()
                    .find(|(_, other)| same(other))
                    .map(|(name, _)| format!("{command:?} and {name}"))
            })
            .collect();
        assert!(clashes.is_empty(), "these Termoil keys are menu keys on macOS: {clashes:?}");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn termoil_s_keys_on_windows_and_linux() {
        use egui::Key as K;
        assert_eq!(command_for(K::D, &windows(true, true, false)), Some(C::SplitRight));
        assert_eq!(command_for(K::E, &windows(true, true, false)), Some(C::SplitDown));
        assert_eq!(command_for(K::W, &windows(true, true, false)), Some(C::ClosePane));
        assert_eq!(command_for(K::P, &windows(true, true, false)), Some(C::CommandPalette));
        assert_eq!(command_for(K::ArrowLeft, &windows(false, false, true)), Some(C::FocusLeft));
        assert_eq!(command_for(K::ArrowRight, &windows(true, true, true)), Some(C::ResizeRight));
        assert_eq!(command_for(K::Tab, &windows(true, false, false)), Some(C::NextTab));
        assert_eq!(
            command_for(K::D, &windows(true, false, false)),
            None,
            "Ctrl+D is the program's"
        );
        assert_eq!(
            command_for(K::C, &windows(true, true, false)),
            None,
            "copy is a clipboard event"
        );
        assert_eq!(chord_text(chord_of(C::SplitRight).expect("bound")), "Ctrl+Shift+D");
        assert_eq!(chord_text(chord_of(C::ResizeLeft).expect("bound")), "Ctrl+Alt+Shift+Left");
    }
}
