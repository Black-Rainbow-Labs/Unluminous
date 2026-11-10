//! Termoil's pane features over every terminal grid in the window (`task-2597`).
//!
//! *"We want the termoil functionality for menu, themes, split screen etc to be available in Unluminous
//! terminal, realm, agent tasks, etc. When Termoil has focus, its keyboard shortcuts should work."*
//!
//! Every grid is drawn through [`show`], whether it belongs to a terminal tab, a run, a node on the canvas or a
//! ticket's agent, so all four get the same things from here:
//!
//! - **Splits.** A grid's owner holds one session. The panes split from it are kept here against that
//!   session's serial, laid out by `unluminous_terminal::panes`, and let go of on the frame the owner's
//!   session is dropped ([`forget_the_dead`]). Pane `0` is always the owner's session.
//! - **Termoil's keys**, from `services::termoil_keys`, read before the grid sends anything to the program.
//! - **A right click menu** of the same commands, and the **command palette** and **theme picker**, which the
//!   window opens because they are its own modals ([`Request`]).
//!
//! What a command does is [`run`], whichever way it was asked for: a key, the right click menu, the palette,
//! a `View` menu row, `unluminous-cli action run termoil-<name>`, or a `terminal` command.
//!
//! Everything here is per thread, for the reason the terminal palette is: a window is one thread, and the
//! window tests run many windows in one process at once.

use std::cell::RefCell;
use std::collections::HashMap;

use egui::{Pos2, Rect, Sense, Stroke, Vec2};
use unluminous_terminal::panes::{Direction, Frac, Layout, PaneId, HOST};
use unluminous_terminal::{Session, SessionSettings, Size, Waker};

use crate::app::actions::{Action, Entry};
use crate::components::context_menu;
use crate::components::terminal_panel::{pane_grid, GridOutcome, TabAsk};
use crate::services::termoil_keys::{self, TermoilCommand};
use crate::services::text_renderer::TextRenderer;
use crate::theme::color;

/// How much room is left between two panes, half on each side, which is where the divider is drawn and
/// grabbed.
const GAP: f32 = 3.0;

/// One terminal's extra panes.
struct Split {
    /// Answers for as long as the owner's session lives.
    owner: std::sync::Weak<()>,
    layout: Layout,
    /// The sessions of every pane but the owner's, and whether a drag is selecting in each.
    panes: Vec<(PaneId, Session, bool)>,
}

impl Split {
    fn new(host: &Session) -> Self {
        Self { owner: host.liveness(), layout: Layout::new(), panes: Vec::new() }
    }

    fn session_mut(&mut self, pane: PaneId) -> Option<&mut Session> {
        self.panes.iter_mut().find(|(id, _, _)| *id == pane).map(|(_, session, _)| session)
    }
}

/// Something the window has to do for a grid, because it is the window's modal and not the grid's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Open the terminal command palette for the grid with this serial.
    Palette(u64),
    /// Open the theme picker for the grid with this serial.
    Themes(u64),
    /// Open the shader picker for the grid with this serial.
    Shaders(u64),
    /// Open the appearance picker on its cursor effects, for the grid with this serial (`task-2601`).
    CursorEffects(u64),
}

/// How a pane is started: the program the terminal setting names, and what wakes the window when it writes.
#[derive(Clone, Default)]
struct Spawning {
    shell: Option<String>,
    waker: Option<Waker>,
}

thread_local! {
    static SPLITS: RefCell<HashMap<u64, Split>> = RefCell::new(HashMap::new());
    /// Commands for a grid to run when it is next drawn, and when each was asked for.
    static PENDING: RefCell<Vec<(u64, TermoilCommand, std::time::Instant)>> = const { RefCell::new(Vec::new()) };
    static REQUESTS: RefCell<Vec<Request>> = const { RefCell::new(Vec::new()) };
    /// The right click menu that is open: the serial of its terminal, where it is, and the pass it was opened on.
    static MENU: RefCell<Option<(u64, Pos2, u64)>> = const { RefCell::new(None) };
    static SPAWNING: RefCell<Spawning> = RefCell::new(Spawning::default());
    /// The serial of the grid that read the keyboard, and the pass it did it on.
    static WITH_KEYBOARD: std::cell::Cell<Option<(u64, u64)>> = const { std::cell::Cell::new(None) };
}

/// Say what a new pane runs and how it wakes the window. The window calls this as it starts each frame.
pub fn set_spawning(shell: &str, waker: Waker) {
    let shell = (!shell.trim().is_empty()).then(|| shell.trim().to_owned());
    SPAWNING.with(|spawning| *spawning.borrow_mut() = Spawning { shell, waker: Some(waker) });
}

/// Let go of the panes of every terminal whose owner has gone, which ends their programs.
pub fn forget_the_dead() {
    SPLITS.with(|splits| splits.borrow_mut().retain(|_, split| split.owner.upgrade().is_some()));
    // A command for a grid that is not drawn any more, because its node or ticket was closed or is out of
    // view, is not run later by surprise.
    PENDING.with(|pending| pending.borrow_mut().retain(|(_, _, asked)| asked.elapsed() < STALE));
}

/// How long a command waits for its grid to be drawn before it is dropped.
const STALE: std::time::Duration = std::time::Duration::from_secs(2);

/// Ask for `command` to be run on the grid with this serial the next time it is drawn.
pub fn queue(serial: u64, command: TermoilCommand) {
    PENDING.with(|pending| pending.borrow_mut().push((serial, command, std::time::Instant::now())));
}

/// What the grids asked the window for since the last time this was asked.
pub fn take_requests() -> Vec<Request> {
    REQUESTS.with(|requests| std::mem::take(&mut *requests.borrow_mut()))
}

/// The serial of the grid that has the keyboard, if one read it on this pass or the one before.
pub fn with_keyboard(ctx: &egui::Context) -> Option<u64> {
    let pass = ctx.cumulative_pass_nr();
    WITH_KEYBOARD
        .with(std::cell::Cell::get)
        .filter(|(_, noted)| pass.saturating_sub(*noted) <= 1)
        .map(|(serial, _)| serial)
}

/// One pane of a terminal, for `terminal panes`.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneInfo {
    pub id: PaneId,
    pub focused: bool,
    pub rows: usize,
    pub columns: usize,
    pub name: String,
}

/// Every pane of the terminal whose owner is `host`, in reading order.
pub fn panes_of(host: &Session) -> (Vec<PaneInfo>, bool) {
    SPLITS.with(|splits| {
        let splits = splits.borrow();
        let Some(split) = splits.get(&host.serial()) else {
            let size = host.size();
            let only = PaneInfo {
                id: HOST,
                focused: true,
                rows: size.rows,
                columns: size.columns,
                name: host.name().to_owned(),
            };
            return (vec![only], false);
        };
        let infos = split
            .layout
            .panes()
            .into_iter()
            .filter_map(|id| {
                let session = match id {
                    HOST => host,
                    _ => split
                        .panes
                        .iter()
                        .find(|(pane, _, _)| *pane == id)
                        .map(|(_, session, _)| session)?,
                };
                let size = session.size();
                Some(PaneInfo {
                    id,
                    focused: split.layout.focused() == id,
                    rows: size.rows,
                    columns: size.columns,
                    name: session.name().to_owned(),
                })
            })
            .collect();
        (infos, split.layout.is_zoomed())
    })
}

/// Run `act` on one pane of the terminal whose owner is `host`: the owner's own session for pane `0`.
pub fn with_pane<R>(
    host: &mut Session,
    pane: PaneId,
    act: impl FnOnce(&mut Session) -> R,
) -> Option<R> {
    if pane == HOST {
        return Some(act(host));
    }
    let mut split = take(host.serial())?;
    let answer = split.session_mut(pane).map(act);
    put_back(host.serial(), split);
    answer
}

/// Read one pane of the terminal whose owner is `host`, without changing anything: the owner's own session for
/// pane `0`. `None` when there is no such pane.
pub fn read_pane<R>(host: &Session, pane: PaneId, read: impl FnOnce(&Session) -> R) -> Option<R> {
    if pane == HOST {
        return Some(read(host));
    }
    SPLITS.with(|splits| {
        let splits = splits.borrow();
        let split = splits.get(&host.serial())?;
        split.panes.iter().find(|(id, _, _)| *id == pane).map(|(_, session, _)| read(session))
    })
}

/// Split the focused pane of the terminal whose owner is `host`, the new pane going on side `direction`. Left
/// and up have no key of their own, and `terminal split --direction` reaches them through this.
pub fn split_toward(host: &mut Session, direction: Direction) -> Ran {
    let serial = host.serial();
    let mut split = take(serial).unwrap_or_else(|| Split::new(host));
    let ran = split_pane(host, &mut split, direction);
    put_back(serial, split);
    ran
}

/// Give the keyboard to one pane of the terminal whose owner is `host`.
pub fn focus_pane(host: &Session, pane: PaneId) -> bool {
    SPLITS.with(|splits| {
        let mut splits = splits.borrow_mut();
        match splits.get_mut(&host.serial()) {
            Some(split) => split.layout.focus(pane),
            None => pane == HOST,
        }
    })
}

/// Close one pane of the terminal whose owner is `host`, ending its program. The owner's pane is not closed.
pub fn close_pane(host: &Session, pane: PaneId) -> bool {
    let Some(mut split) = take(host.serial()) else { return false };
    let closed = split.layout.close(pane);
    if closed {
        split.panes.retain(|(id, _, _)| *id != pane);
    }
    put_back(host.serial(), split);
    closed
}

fn take(serial: u64) -> Option<Split> {
    SPLITS.with(|splits| splits.borrow_mut().remove(&serial))
}

/// Keep a split for next frame, unless it has gone back to one pane, which needs nothing kept.
fn put_back(serial: u64, split: Split) {
    if split.layout.is_split() {
        SPLITS.with(|splits| splits.borrow_mut().insert(serial, split));
    }
}

/// What running a command did.
#[derive(Debug, Default, PartialEq)]
pub struct Ran {
    /// What the status bar or the command line says about it.
    pub message: String,
    /// False when it could not be done, and `message` says why.
    pub done: bool,
    /// Text to put on the clipboard.
    pub copy: Option<String>,
    /// A command for the terminal tile's tabs, which only the tile can do.
    pub tabs: Option<TabAsk>,
}

impl Ran {
    fn done(message: impl Into<String>) -> Self {
        Self { message: message.into(), done: true, ..Self::default() }
    }

    fn refused(message: impl Into<String>) -> Self {
        Self { message: message.into(), done: false, ..Self::default() }
    }
}

/// Run a Termoil command on the terminal whose owner is `host`. `owns_tabs` says whether the owner is a tab of
/// the terminal tile, which is the only owner the tab commands mean anything to.
pub fn run(host: &mut Session, command: TermoilCommand, owns_tabs: bool) -> Ran {
    let serial = host.serial();
    let mut split = take(serial).unwrap_or_else(|| Split::new(host));
    let ran = run_on(host, &mut split, command, owns_tabs);
    put_back(serial, split);
    ran
}

fn run_on(host: &mut Session, split: &mut Split, command: TermoilCommand, owns_tabs: bool) -> Ran {
    use TermoilCommand as C;
    let direction = |command: C| match command {
        C::SplitRight | C::FocusRight | C::ResizeRight => Direction::Right,
        C::SplitDown | C::FocusDown | C::ResizeDown => Direction::Down,
        C::FocusLeft | C::ResizeLeft => Direction::Left,
        _ => Direction::Up,
    };
    match command {
        C::SplitRight | C::SplitDown => split_pane(host, split, direction(command)),
        C::ClosePane => close_focused(split, owns_tabs),
        C::FocusLeft | C::FocusRight | C::FocusUp | C::FocusDown => {
            match split.layout.focus_toward(direction(command)) {
                true => Ran::done(format!("Pane {} has the keyboard", split.layout.focused())),
                false => Ran::refused(format!("There is no pane {}", direction(command).name())),
            }
        }
        C::NextPane | C::PreviousPane => {
            let pane = split.layout.cycle(command == C::NextPane);
            Ran::done(format!("Pane {pane} has the keyboard"))
        }
        C::ZoomPane => match (split.layout.is_split(), split.layout.toggle_zoom()) {
            (false, _) => Ran::refused("There is one pane, so there is nothing to zoom"),
            (true, true) => Ran::done("The pane fills the terminal"),
            (true, false) => Ran::done("The panes are side by side again"),
        },
        C::ResizeLeft | C::ResizeRight | C::ResizeUp | C::ResizeDown => {
            match split.layout.resize(direction(command)) {
                true => Ran::done(format!("Moved the divider {}", direction(command).name())),
                false => Ran::refused("There is no divider that way"),
            }
        }
        C::NewTab | C::NextTab | C::PreviousTab if !owns_tabs => {
            Ran::refused("Only a terminal tab has tabs beside it")
        }
        C::NewTab => Ran { tabs: Some(TabAsk::New), ..Ran::done("Opened a terminal tab") },
        C::NextTab => {
            Ran { tabs: Some(TabAsk::Next), ..Ran::done("Showing the next terminal tab") }
        }
        C::PreviousTab => {
            Ran { tabs: Some(TabAsk::Previous), ..Ran::done("Showing the previous terminal tab") }
        }
        C::CommandPalette => {
            REQUESTS.with(|requests| requests.borrow_mut().push(Request::Palette(host.serial())));
            Ran::done("Opened the terminal command palette")
        }
        C::ChooseTheme => {
            REQUESTS.with(|requests| requests.borrow_mut().push(Request::Themes(host.serial())));
            Ran::done("Opened the terminal theme picker")
        }
        C::ChooseShader => {
            REQUESTS.with(|requests| requests.borrow_mut().push(Request::Shaders(host.serial())));
            Ran::done("Opened the terminal shader picker")
        }
        C::ChooseCursorEffect => {
            REQUESTS
                .with(|requests| requests.borrow_mut().push(Request::CursorEffects(host.serial())));
            Ran::done("Opened the terminal cursor effect picker")
        }
        _ => {
            let focused = split.layout.focused();
            let session = match focused {
                HOST => host,
                pane => match split.session_mut(pane) {
                    Some(session) => session,
                    None => host,
                },
            };
            run_on_the_session(session, command)
        }
    }
}

/// The commands that act on one pane's own terminal.
fn run_on_the_session(session: &mut Session, command: TermoilCommand) -> Ran {
    use TermoilCommand as C;
    let page = session.rows().max(2) as i32 - 1;
    match command {
        C::PreviousPrompt | C::NextPrompt => {
            let forward = command == C::NextPrompt;
            if session.prompt_count() == 0 {
                return Ran::refused("The shell in this terminal marks no prompts (OSC 133)");
            }
            match (session.jump_to_prompt(forward), forward) {
                (true, false) => Ran::done("Scrolled to the previous prompt"),
                (true, true) => Ran::done("Scrolled to the next prompt"),
                (false, false) => Ran::refused("There is no earlier prompt"),
                (false, true) => Ran::refused("Already at the newest output"),
            }
        }
        C::ScrollPageUp => {
            session.scroll(page);
            Ran::done("Scrolled up a page")
        }
        C::ScrollPageDown => {
            session.scroll(-page);
            Ran::done("Scrolled down a page")
        }
        C::ScrollToTop => {
            session.scroll_to_top();
            Ran::done("Scrolled to the oldest line")
        }
        C::ScrollToBottom => {
            session.scroll_to_bottom();
            Ran::done("Scrolled to the newest output")
        }
        C::ClearScrollback => {
            session.clear_history();
            Ran::done("Cleared the scrollback")
        }
        C::ResetTerminal => {
            session.reset();
            Ran::done("Reset the terminal")
        }
        C::SelectAll => {
            session.select_all();
            Ran::done("Selected everything in the terminal")
        }
        C::Copy => match session.selected_text() {
            Some(text) => {
                session.clear_selection();
                Ran { copy: Some(text), ..Ran::done("Copied the selection") }
            }
            None => Ran::refused("Nothing is selected"),
        },
        C::Paste => {
            let text = arboard::Clipboard::new()
                .and_then(|mut clipboard| clipboard.get_text())
                .unwrap_or_default();
            if text.is_empty() {
                return Ran::refused("The clipboard holds no text");
            }
            session.scroll_to_bottom();
            session.send(unluminous_terminal::keys::paste(&text, session.mode()));
            Ran::done("Pasted")
        }
        _ => Ran::refused("That command is not about one pane"),
    }
}

/// Start a pane beside the focused one, running the shell a new terminal runs, in the focused pane's folder.
fn split_pane(host: &mut Session, split: &mut Split, direction: Direction) -> Ran {
    let (size, detached, folder) = {
        let beside = match split.layout.focused() {
            HOST => &*host,
            pane => split
                .panes
                .iter()
                .find(|(id, _, _)| *id == pane)
                .map(|(_, session, _)| session)
                .unwrap_or(host),
        };
        (beside.size(), beside.is_detached(), beside.folder())
    };
    let size = match direction {
        Direction::Left | Direction::Right => Size::new(size.rows, (size.columns / 2).max(2)),
        Direction::Up | Direction::Down => Size::new((size.rows / 2).max(2), size.columns),
    }
    .with_cell(f32::from(size.cell_width), f32::from(size.cell_height));
    let session = match detached {
        // A test's terminal has no shell behind it, and neither does a pane split from one.
        true => Session::detached(size),
        false => {
            let spawning = SPAWNING.with(|spawning| spawning.borrow().clone());
            let settings = SessionSettings {
                shell: spawning.shell,
                working_directory: folder,
                ..SessionSettings::default()
            };
            let waker = spawning.waker.unwrap_or_else(|| std::sync::Arc::new(|| {}));
            match Session::spawn(&settings, size, waker) {
                Ok(session) => session,
                Err(problem) => {
                    return Ran::refused(format!(
                        "Unluminous could not start a shell for the pane: {problem}"
                    ))
                }
            }
        }
    };
    let pane = split.layout.split(direction);
    split.panes.push((pane, session, false));
    Ran::done(format!("Split the terminal {}: pane {pane} has the keyboard", direction.name()))
}

fn close_focused(split: &mut Split, owns_tabs: bool) -> Ran {
    let pane = split.layout.focused();
    if pane != HOST && split.layout.close(pane) {
        split.panes.retain(|(id, _, _)| *id != pane);
        return Ran::done(format!("Closed pane {pane}"));
    }
    match owns_tabs {
        // Termoil closes the tab with its last pane. The first pane is the tab's own terminal, so closing it
        // closes the tab, and every pane split from it goes with it.
        true => Ran { tabs: Some(TabAsk::Close), ..Ran::done("Closed the terminal tab") },
        false => Ran::refused(
            "The first pane is this terminal's own; close the panes split from it instead",
        ),
    }
}

/// Whether the keys this frame are the grid's to read: it has the keyboard, and neither a text box nor
/// another modal has taken it. The grid's own `handle_input` asks the same two questions.
fn keys_are_the_grids(ui: &egui::Ui, focused: bool) -> bool {
    focused
        && !crate::app::text_box_has_the_keyboard(ui.ctx())
        && !crate::app::another_modal_has_the_keyboard(ui.ctx(), ui.layer_id())
}

/// Whether a Termoil key means anything to this grid just now. A key that does not is left for the program:
/// moving between panes, resizing, zooming and closing a pane need a split (closing is also a tab's), and the
/// tab keys need the terminal tile.
fn the_key_applies(command: TermoilCommand, split: bool, owns_tabs: bool) -> bool {
    use TermoilCommand as C;
    match command {
        C::FocusLeft
        | C::FocusRight
        | C::FocusUp
        | C::FocusDown
        | C::NextPane
        | C::PreviousPane => split,
        C::ResizeLeft | C::ResizeRight | C::ResizeUp | C::ResizeDown | C::ZoomPane => split,
        C::ClosePane => split || owns_tabs,
        C::NewTab | C::NextTab | C::PreviousTab => owns_tabs,
        _ => true,
    }
}

/// Take every Termoil chord that applies out of this frame's key presses, so the program does not get it as
/// well, and answer the commands they were.
fn take_the_chords(ui: &egui::Ui, split: bool, owns_tabs: bool) -> Vec<TermoilCommand> {
    let mut commands = Vec::new();
    ui.ctx().input_mut(|input| {
        input.events.retain(|event| match event {
            egui::Event::Key { key, pressed, modifiers, .. } => {
                match termoil_keys::command_for(*key, modifiers)
                    .filter(|command| the_key_applies(*command, split, owns_tabs))
                {
                    Some(command) => {
                        if *pressed {
                            commands.push(command);
                        }
                        false
                    }
                    None => true,
                }
            }
            _ => true,
        });
    });
    commands
}

/// The commands the right click menu offers, with Termoil's key beside each.
pub fn menu_entries(owns_tabs: bool, split: bool) -> Vec<Entry> {
    use TermoilCommand as C;
    let row = |command: C| {
        let entry = match termoil_keys::chord_of(command) {
            Some(chord) => Entry::with_shortcut(command.label(), Action::Termoil(command), chord),
            None => Entry::item(command.label(), Action::Termoil(command)),
        };
        entry.not_from_the_keyboard()
    };
    let mut entries = vec![row(C::Copy), row(C::Paste), row(C::SelectAll), Entry::Separator];
    entries.push(row(C::SplitRight));
    entries.push(row(C::SplitDown));
    if split {
        entries.push(row(C::ClosePane));
        entries.push(row(C::ZoomPane));
    } else if owns_tabs {
        entries.push(row(C::ClosePane));
    }
    if owns_tabs {
        entries.push(row(C::NewTab));
    }
    entries.extend([
        Entry::Separator,
        row(C::CommandPalette),
        row(C::ChooseTheme),
        row(C::ChooseShader),
        row(C::ChooseCursorEffect),
        Entry::Separator,
        row(C::ClearScrollback),
        row(C::ResetTerminal),
    ]);
    entries
}

/// Put what running a command did into what the grid tells its owner.
fn report(ran: Ran, outcome: &mut GridOutcome) {
    if let Some(text) = ran.copy {
        outcome.copy = Some(text);
    }
    if ran.tabs.is_some() {
        outcome.tabs = ran.tabs;
    }
    if !ran.message.is_empty() {
        outcome.message = Some(ran.message);
    }
}

/// Everything about one grid the layer draws: the owner's session and what [`pane_grid`] is given for it.
pub(crate) struct LayerGrid<'a> {
    pub host: &'a mut Session,
    pub selecting: &'a mut bool,
    pub focused: bool,
    pub id: &'a str,
    pub renderer: &'a TextRenderer,
    pub font_size: f32,
    pub opacity: f32,
    pub owns_tabs: bool,
}

/// Draw a terminal and its panes into `area`, read Termoil's keys and the right click, and run what was asked.
pub(crate) fn show(ui: &mut egui::Ui, area: Rect, grid: LayerGrid<'_>) -> GridOutcome {
    let LayerGrid { host, selecting, focused, id, renderer, font_size, opacity, owns_tabs } = grid;
    let serial = host.serial();
    let mut outcome = GridOutcome::default();
    let mut split = take(serial).unwrap_or_else(|| Split::new(host));
    let menu_open = MENU.with(|menu| menu.borrow().is_some_and(|(owner, _, _)| owner == serial));

    // What the menu bar, the palette or the command line asked of this terminal.
    let pending: Vec<TermoilCommand> = PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        let (mine, rest): (Vec<_>, Vec<_>) =
            pending.drain(..).partition(|(owner, _, _)| *owner == serial);
        *pending = rest;
        mine.into_iter().map(|(_, command, _)| command).collect()
    });
    let mut commands = pending;
    if keys_are_the_grids(ui, focused) && !menu_open {
        commands.extend(take_the_chords(ui, split.layout.is_split(), owns_tabs));
    }
    for command in commands {
        let ran = run_on(host, &mut split, command, owns_tabs);
        report(ran, &mut outcome);
    }

    // A right click anywhere over the terminal opens its menu, on the pane under the pointer.
    let right_click = ui.input(|input| {
        input.pointer.secondary_clicked().then(|| input.pointer.interact_pos()).flatten()
    });
    if let Some(at) = right_click.filter(|at| area.contains(*at) && ui.rect_contains_pointer(area))
    {
        if let Some((pane, _)) =
            pane_rects(&split.layout, area).into_iter().find(|(_, rect)| rect.contains(at))
        {
            split.layout.focus(pane);
        }
        let pass = ui.ctx().cumulative_pass_nr();
        MENU.with(|menu| *menu.borrow_mut() = Some((serial, at, pass)));
        ui.ctx().request_repaint();
        outcome.take_focus = true;
    }

    let keyboard = focused && !menu_open;
    if split.layout.is_split() {
        draw_the_panes(
            ui,
            area,
            host,
            &mut split,
            selecting,
            keyboard,
            id,
            renderer,
            font_size,
            opacity,
            &mut outcome,
        );
    } else {
        let drawn = pane_grid(
            ui,
            area,
            Some(&mut *host),
            selecting,
            keyboard,
            id,
            "",
            renderer,
            font_size,
            opacity,
        );
        outcome.cursor = drawn.cursor;
        merge(drawn, &mut outcome);
    }
    if focused {
        WITH_KEYBOARD.with(|holder| holder.set(Some((serial, ui.ctx().cumulative_pass_nr()))));
    }
    // Termoil's shader and cursor effect over the whole terminal, panes and all, as Termoil draws them over its
    // window. Not while the right click menu or another modal is open, because a modal's backdrop is a picture of
    // the window that already has the shader in it, unless the appearance picker is the modal and is previewing.
    // Whatever is drawn above the terminal is cut out of the shader either way (`task-2601`).
    let covered = menu_open || crate::app::another_modal_has_the_keyboard(ui.ctx(), ui.layer_id());
    let previewing = crate::services::terminal_shader::previewing()
        || crate::services::terminal_cursor_effect::previewing();
    if !covered || previewing {
        crate::services::terminal_shader::shade(ui, area, outcome.cursor);
    }
    show_the_menu(ui, serial, host, &mut split, owns_tabs, &mut outcome);
    put_back(serial, split);
    outcome
}

/// The right click menu, while it is open on this terminal.
fn show_the_menu(
    ui: &mut egui::Ui,
    serial: u64,
    host: &mut Session,
    split: &mut Split,
    owns_tabs: bool,
    outcome: &mut GridOutcome,
) {
    let Some((at, opened)) = MENU.with(|menu| {
        menu.borrow().filter(|(owner, _, _)| *owner == serial).map(|(_, at, opened)| (at, opened))
    }) else {
        return;
    };
    // Drawn from the pass after the right click, which would otherwise count as a click outside the menu.
    if opened == ui.ctx().cumulative_pass_nr() {
        return;
    }
    let entries = menu_entries(owns_tabs, split.layout.is_split());
    let shown = context_menu::show(ui, &format!("termoil-{serial}"), at, &entries);
    if let Some(Action::Termoil(command)) = shown.chosen {
        let ran = run_on(host, split, command, owns_tabs);
        report(ran, outcome);
    }
    if shown.close {
        MENU.with(|menu| *menu.borrow_mut() = None);
    }
}

/// Where each pane goes in `area`, with room between them for the dividers.
fn pane_rects(layout: &Layout, area: Rect) -> Vec<(PaneId, Rect)> {
    let (panes, _) = layout.lay_out(Frac::WHOLE);
    panes
        .into_iter()
        .map(|(pane, frac)| {
            let mut rect = to_rect(area, frac);
            if frac.x > 0.0 {
                rect.min.x += GAP;
            }
            if frac.x + frac.w < 0.999 {
                rect.max.x -= GAP;
            }
            if frac.y > 0.0 {
                rect.min.y += GAP;
            }
            if frac.y + frac.h < 0.999 {
                rect.max.y -= GAP;
            }
            (pane, rect)
        })
        .collect()
}

fn to_rect(area: Rect, frac: Frac) -> Rect {
    Rect::from_min_size(
        Pos2::new(area.left() + frac.x * area.width(), area.top() + frac.y * area.height()),
        Vec2::new(frac.w * area.width(), frac.h * area.height()),
    )
}

/// Draw every pane, the dividers between them and the edge of the one with the keyboard.
#[allow(clippy::too_many_arguments)]
fn draw_the_panes(
    ui: &mut egui::Ui,
    area: Rect,
    host: &mut Session,
    split: &mut Split,
    selecting: &mut bool,
    keyboard: bool,
    id: &str,
    renderer: &TextRenderer,
    font_size: f32,
    opacity: f32,
    outcome: &mut GridOutcome,
) {
    // The ground behind the gaps is the terminal's own, so a split looks like one terminal with lines in it.
    let ground = host.snapshot().background;
    let ground =
        crate::theme::pane_ground(egui::Color32::from_rgb(ground.r, ground.g, ground.b), opacity);
    ui.painter_at(area).rect_filled(area, egui::CornerRadius::ZERO, ground);

    // A pane whose program ended is closed, the way Termoil closes one.
    let ended: Vec<PaneId> = split
        .panes
        .iter()
        .filter(|(_, session, _)| !session.is_detached() && !session.is_running())
        .map(|(id, _, _)| *id)
        .collect();
    for pane in ended {
        split.layout.close(pane);
        split.panes.retain(|(id, _, _)| *id != pane);
    }
    if !split.layout.is_split() {
        let drawn = pane_grid(
            ui,
            area,
            Some(&mut *host),
            selecting,
            keyboard,
            id,
            "",
            renderer,
            font_size,
            opacity,
        );
        outcome.cursor = drawn.cursor;
        merge(drawn, outcome);
        return;
    }

    let focused = split.layout.focused();
    for (pane, rect) in pane_rects(&split.layout, area) {
        let has_keys = keyboard && pane == focused;
        let drawn = match pane {
            HOST => pane_grid(
                ui,
                rect,
                Some(&mut *host),
                selecting,
                has_keys,
                id,
                "",
                renderer,
                font_size,
                opacity,
            ),
            _ => {
                let Some((_, session, selecting)) =
                    split.panes.iter_mut().find(|(held, _, _)| *held == pane)
                else {
                    continue;
                };
                let pane_id = format!("{id}-pane-{pane}");
                pane_grid(
                    ui,
                    rect,
                    Some(session),
                    selecting,
                    has_keys,
                    &pane_id,
                    "",
                    renderer,
                    font_size,
                    opacity,
                )
            }
        };
        if drawn.take_focus {
            split.layout.focus(pane);
        }
        if pane == focused {
            outcome.cursor = drawn.cursor;
        }
        merge(drawn, outcome);
        if pane == focused && !split.layout.is_zoomed() {
            let edge = if keyboard { color::accent() } else { color::divider() };
            ui.painter_at(area).rect_stroke(
                rect.expand(1.0),
                0.0,
                Stroke::new(1.0, edge),
                egui::StrokeKind::Outside,
            );
        }
    }
    drag_the_dividers(ui, area, split, id);
}

/// Each divider can be dragged, which is what changes how the two sides share the split.
fn drag_the_dividers(ui: &mut egui::Ui, area: Rect, split: &mut Split, id: &str) {
    let (_, dividers) = split.layout.lay_out(Frac::WHOLE);
    for divider in dividers {
        let line = to_rect(area, divider.at);
        let grab = match divider.side_by_side {
            true => Rect::from_center_size(line.center(), Vec2::new(GAP * 2.0, line.height())),
            false => Rect::from_center_size(line.center(), Vec2::new(line.width(), GAP * 2.0)),
        };
        let response =
            ui.interact(grab, ui.id().with((id, "termoil-divider", divider.split)), Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(match divider.side_by_side {
                true => egui::CursorIcon::ResizeHorizontal,
                false => egui::CursorIcon::ResizeVertical,
            });
        }
        if let Some(at) = response.interact_pointer_pos().filter(|_| response.dragged()) {
            let within = to_rect(area, divider.within);
            let share = match divider.side_by_side {
                true => (at.x - within.left()) / within.width().max(1.0),
                false => (at.y - within.top()) / within.height().max(1.0),
            };
            split.layout.set_share(divider.split, share);
        }
    }
}

/// Add what one pane's grid reported to what the whole terminal reports.
fn merge(drawn: GridOutcome, outcome: &mut GridOutcome) {
    outcome.take_focus |= drawn.take_focus;
    if drawn.copy.is_some() {
        outcome.copy = drawn.copy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detached() -> Session {
        Session::detached(Size::new(20, 80).with_cell(8.0, 16.0))
    }

    #[test]
    fn a_split_starts_a_detached_pane_beside_a_detached_terminal_and_closes_again() {
        let mut host = detached();
        let ran = run(&mut host, TermoilCommand::SplitRight, false);
        assert!(ran.done, "{}", ran.message);
        let (panes, zoomed) = panes_of(&host);
        assert_eq!(panes.len(), 2);
        assert!(!zoomed);
        assert!(panes[1].focused, "the new pane has the keyboard");
        assert_eq!(panes[1].columns, 40, "half the width until it is drawn");
        assert!(with_pane(&mut host, panes[1].id, |session| session.is_detached()).expect("a pane"));

        assert!(run(&mut host, TermoilCommand::FocusLeft, false).done);
        assert!(panes_of(&host).0[0].focused);
        assert!(!run(&mut host, TermoilCommand::ClosePane, false).done, "the owner's pane stays");
        assert!(run(&mut host, TermoilCommand::FocusRight, false).done);
        assert!(run(&mut host, TermoilCommand::ClosePane, false).done);
        assert_eq!(panes_of(&host).0.len(), 1);
    }

    #[test]
    fn closing_the_first_pane_of_a_tab_asks_the_tile_to_close_the_tab() {
        let mut host = detached();
        let ran = run(&mut host, TermoilCommand::ClosePane, true);
        assert_eq!(ran.tabs, Some(TabAsk::Close));
        assert!(!run(&mut host, TermoilCommand::NextTab, false).done, "a node has no tabs");
        assert_eq!(run(&mut host, TermoilCommand::NextTab, true).tabs, Some(TabAsk::Next));
    }

    #[test]
    fn the_palette_and_the_theme_picker_are_asked_of_the_window() {
        let mut host = detached();
        take_requests();
        run(&mut host, TermoilCommand::CommandPalette, false);
        run(&mut host, TermoilCommand::ChooseTheme, false);
        assert_eq!(
            take_requests(),
            vec![Request::Palette(host.serial()), Request::Themes(host.serial())]
        );
    }

    #[test]
    fn the_panes_go_when_their_owner_does() {
        let mut host = detached();
        run(&mut host, TermoilCommand::SplitDown, false);
        let serial = host.serial();
        assert!(SPLITS.with(|splits| splits.borrow().contains_key(&serial)));
        drop(host);
        forget_the_dead();
        assert!(!SPLITS.with(|splits| splits.borrow().contains_key(&serial)));
    }

    #[test]
    fn a_pane_key_with_nothing_to_do_is_left_for_the_program() {
        use TermoilCommand as C;
        assert!(
            !the_key_applies(C::FocusLeft, false, true),
            "Alt+Left in an unsplit terminal is the program's"
        );
        assert!(the_key_applies(C::FocusLeft, true, false));
        assert!(!the_key_applies(C::NextTab, true, false), "a node has no tabs");
        assert!(
            the_key_applies(C::ClosePane, false, true),
            "closing a tab's only pane closes the tab"
        );
        assert!(!the_key_applies(C::ClosePane, false, false));
        assert!(the_key_applies(C::SplitRight, false, false));
    }

    #[test]
    fn the_right_click_menu_offers_the_split_and_the_theme() {
        let names: Vec<String> = menu_entries(true, true)
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Item { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        for wanted in [
            "Split Pane Right",
            "Split Pane Down",
            "Close Pane",
            "Zoom Pane",
            "Choose Terminal Theme",
            "Terminal Command Palette",
        ] {
            assert!(names.iter().any(|name| name == wanted), "{wanted} is missing from {names:?}");
        }
    }
}
