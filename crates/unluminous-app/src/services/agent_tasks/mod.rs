//! The Agent-Tasks plugin: the board, its tickets, its terminals and its agents.
//!
//! `tasks/agent-tasks-plugin-tdd.md` is the design and `tasks/ui-plugin-architecture.md` is the plugin
//! system it sits in. This file is the [`crate::services::plugin_ui::UiProvider`] the manifest names:
//! it owns the state, answers the commands, and hands the drawing to `components::agent_tasks`.
//!
//! ## What is in each file
//!
//! `store.rs` is the SQLite file and every query there is. `model.rs` is what a ticket is. `board.rs`
//! is the arithmetic a drag and a search need. `watchdog.rs` is the two ways an agent stops.
//! `agent.rs` builds the command line Claude or Codex is launched with. `clock.rs` is the instant as
//! text. **None of them draws and none of them reads the window**, so all of it is tested with no
//! window, no database and no clock.
//!
//! ## Nothing is read once a frame
//!
//! The board is read when it is opened, when a command changed something, and when the two minute tick
//! fires. A frame in which nothing changed costs no query at all, which is `task-1666`'s rule kept the
//! way `symbols::Hover` keeps it. [`AgentTasks::refresh`] is the one place a read happens, so there is
//! no second path that could start reading per frame by accident.

mod configuration;
mod terminal;

pub use configuration::{Configuration, View};
pub use terminal::TicketTerminal;
use terminal::{AGENT_SESSION_MARKERS, PROMPT_CEILING};

pub mod agent;
pub mod board;
pub mod clock;
pub mod commands;
pub mod db;
pub mod folder_trust;
pub mod keychain;
pub mod model;
pub mod native_session;
pub mod screen_choice;
pub mod store;
pub mod ticket_chat;
pub mod watchdog;

use std::path::PathBuf;

use serde_json::json;

use crate::services::plugin_ui::{Answer, Context, Look, Request, UiProvider};
use model::{Assignee, Author, Board, Epic, Priority, Sprint, SprintStatus, Status, Task};
use store::{Store, TaskEdit};

/// What `Create task` says when the ticket has no title. The modal shows it under the title field.
pub const TITLE_MISSING: &str = "A ticket needs a title. Type one in the field outlined in red.";

impl AgentTasks {
    /// Read the open ticket's description as markdown, or as its source.
    ///
    /// One function so the two buttons and `plugins run agent-tasks show` reach the same change by the same path,
    /// which is the rule every control on this board keeps. The scroll goes back to the top when the view
    /// changes, because a scroll measured in one view means nothing in the other.
    pub fn show_the_description_rendered(&mut self, rendered: bool) {
        self.detail.description_rendered = rendered;
        self.description_scroll = 0.0;
    }

    /// Read one comment as its source, or as markdown.
    pub fn show_the_comment_raw(&mut self, id: i64, raw: bool) {
        match raw {
            true => self.detail.comments_raw.insert(id),
            false => self.detail.comments_raw.remove(&id),
        };
    }

    /// Whether one comment is being read as its source. Comments start rendered, so this is false by default.
    pub fn the_comment_is_raw(&self, id: i64) -> bool {
        self.detail.comments_raw.contains(&id)
    }

    /// The folders a ticket's `Project` dropdown offers: this window's own, then the recent ones.
    ///
    /// `holding` is what the ticket's row already says, and it is kept wherever it is, for the reason
    /// `agent::models_for` keeps a model it does not know: a ticket may name a folder this window has never
    /// opened, and a dropdown that dropped it would change what the ticket says just by being drawn.
    pub fn known_projects(&self, holding: Option<&str>) -> Vec<String> {
        let mut offered: Vec<String> = Vec::new();
        let mut add = |path: String| {
            if !path.trim().is_empty() && !offered.contains(&path) {
                offered.push(path);
            }
        };
        if let Some(holding) = holding {
            add(holding.trim().to_owned());
        }
        if let Some(project) = &self.project {
            add(project.display().to_string());
        }
        for recent in &self.recent_projects {
            add(recent.display().to_string());
        }
        offered
    }
}

/// A program that ships beside the running one, by its full path.
///
/// `unluminous-cli` is not on anybody's `PATH`: on macOS it is inside the application bundle next to `unluminous`
/// and on Windows it sits in the installation folder. So an agent told to run `unluminous-cli` would answer
/// that there is no such command, which is what the handoff line's [`agent::ENV_CLI`] exists to prevent.
/// The name as given is the fallback, so a build with no discoverable executable path still produces a
/// line somebody can fix by hand rather than one holding nothing.
pub fn beside_this_program(name: &str) -> String {
    let spelled = match cfg!(windows) {
        true => format!("{name}.exe"),
        false => name.to_owned(),
    };
    std::env::current_exe()
        .ok()
        .and_then(|program| program.parent().map(|folder| folder.join(&spelled)))
        .filter(|beside| beside.is_file())
        .map(|beside| beside.display().to_string())
        .unwrap_or(spelled)
}

/// How a child of this window reaches `unluminous-cli`: on its `PATH`, and by name and instance.
///
/// **`unluminous-cli` is on nobody's `PATH`.** On macOS it is inside the application bundle beside
/// `unluminous` and on Windows in the installation folder, so an agent told to run
/// `unluminous-cli realm here` answers `command not found`. The answer until `task-2004` was three
/// variables — [`agent::ENV_CLI`], [`agent::ENV_INSTANCE`] and a sentence in `UNLUMINOUS_REALM_HINT`
/// saying to use them — and the report is what that was worth: *"Agent's in the base of infinite space
/// don't seem to have the cli, or don't understand the unluminous cli."* Nothing makes an agent run
/// `env`, so the sentence was never read, and the name it guesses was the name that did not work.
///
/// So the folder goes on the child's `PATH`, **in front**, so the client beside *this* window wins over
/// another Unluminous installed elsewhere. The three variables stay: a program that wants the path wants
/// the path, and `--instance` still has to name a window when several are open — which is what
/// `unluminous_cli::parse` reads [`agent::ENV_INSTANCE`] for.
///
/// `over` is the environment the child is being given, which is where the `PATH` it will really have is:
/// an Unluminous started from the Dock has launchd's four folders and the login shell's list is the one
/// `services::login_shell::for_a_child` has just read. This process's own is the fallback. A child's
/// environment is laid **over** the inherited one rather than replacing it, so the answer has to carry
/// the whole of whichever list it extended.
pub fn how_to_reach_this_window(over: &[(String, String)]) -> Vec<(String, String)> {
    let cli = beside_this_program("unluminous-cli");
    let mut carried = vec![
        (agent::ENV_CLI.to_owned(), cli.clone()),
        (agent::ENV_INSTANCE.to_owned(), std::process::id().to_string()),
    ];
    let existing = over
        .iter()
        .rev()
        .find(|(name, _)| name.eq_ignore_ascii_case("PATH"))
        .map(|(_, value)| value.clone())
        .or_else(|| std::env::var("PATH").ok());
    if let Some(path) = path_with_the_cli_in_front(&cli, existing) {
        carried.push(("PATH".to_owned(), path));
    }
    carried
}

/// `PATH` with the folder holding `unluminous-cli` in front of it, or nothing when it is there already.
///
/// A separate function because it is the whole of the arithmetic and a test can check it with no window,
/// no installation and no child process. `cli` is the answer [`beside_this_program`] gave, which is a bare
/// name when there is nothing beside this program to find — and a bare name has no folder, so there is
/// nothing to add.
pub fn path_with_the_cli_in_front(cli: &str, existing: Option<String>) -> Option<String> {
    let folder = std::path::Path::new(cli).parent()?;
    if folder.as_os_str().is_empty() {
        return None;
    }
    let folder = folder.display().to_string();
    let existing = existing.unwrap_or_default();
    // Already in front means nothing to do, which keeps a node restarted in its own shell from growing a
    // `PATH` with the same folder on it twice.
    let separator = if cfg!(windows) { ';' } else { ':' };
    if existing.split(separator).any(|part| part == folder) {
        return None;
    }
    match existing.is_empty() {
        true => Some(folder),
        false => Some(format!("{folder}{separator}{existing}")),
    }
}

#[cfg(test)]
mod reaching_this_window {
    use super::path_with_the_cli_in_front;

    fn separator() -> char {
        if cfg!(windows) {
            ';'
        } else {
            ':'
        }
    }

    /// `task-2004`: the folder holding `unluminous-cli` goes in **front**, so the client beside this
    /// window wins over another Unluminous installed elsewhere.
    #[test]
    fn the_folder_holding_the_client_goes_in_front_of_what_was_there() {
        let cli = format!("{0}tools{0}unluminous-cli", std::path::MAIN_SEPARATOR);
        let existing = format!("{0}usr{0}bin", std::path::MAIN_SEPARATOR);
        let answered = path_with_the_cli_in_front(&cli, Some(existing.clone())).expect("a path");
        let folder = format!("{0}tools", std::path::MAIN_SEPARATOR);
        assert_eq!(answered, format!("{folder}{}{existing}", separator()));
    }

    /// A folder already in front is nothing to do, so a node restarted in its own shell does not grow
    /// a `PATH` with the same folder on it twice.
    #[test]
    fn a_folder_that_is_already_there_is_left_alone() {
        let cli = format!("{0}tools{0}unluminous-cli", std::path::MAIN_SEPARATOR);
        let folder = format!("{0}tools", std::path::MAIN_SEPARATOR);
        let elsewhere = format!("{0}usr{0}bin", std::path::MAIN_SEPARATOR);
        let existing = format!("{folder}{}{elsewhere}", separator());
        assert_eq!(path_with_the_cli_in_front(&cli, Some(existing)), None);
    }

    /// A bare name has no folder, which is what `beside_this_program` answers when there is nothing
    /// beside this binary to find — so there is nothing to put on a `PATH`.
    #[test]
    fn a_bare_name_adds_nothing() {
        assert_eq!(path_with_the_cli_in_front("unluminous-cli", Some("/usr/bin".to_owned())), None);
    }

    /// And an empty `PATH` becomes the folder alone rather than a path with a stray separator on it.
    #[test]
    fn an_empty_path_becomes_the_folder_on_its_own() {
        let cli = format!("{0}tools{0}unluminous-cli", std::path::MAIN_SEPARATOR);
        let folder = format!("{0}tools", std::path::MAIN_SEPARATOR);
        assert_eq!(path_with_the_cli_in_front(&cli, None), Some(folder));
    }
}

/// One ticket, open in the detail.
#[derive(Debug, Clone, Default)]
pub struct Detail {
    pub task: Option<Task>,
    pub todos: Vec<model::Todo>,
    pub comments: Vec<model::Comment>,
    /// What is being typed into the comment box.
    pub draft: String,
    /// What is being typed into the new todo box.
    pub todo_draft: String,
    /// Which comment is being edited, and what is being typed into it.
    ///
    /// Two fields rather than one, because cancelling has to be able to leave the comment as it was: the draft is
    /// separate from the comment, so the block on screen is the draft while this is set and the stored body when
    /// it is not.
    pub editing_comment: Option<i64>,
    pub comment_edit: String,
    /// True while this is a ticket that was created by `+ Add Task` and has not been named.
    ///
    /// What the modal reads to know it is an editor rather than a view: its footer says `Discard` and `Done`
    /// rather than `Close`, and `Discard` deletes the row, because the row was created before anybody typed.
    pub is_new: bool,
    /// What is in the title field, which saves as it is typed.
    ///
    /// Held beside the ticket rather than read from it, because the field is what somebody is typing and the
    /// row is what has been saved. Reading the row into the field every frame would fight the keyboard.
    pub title_draft: String,
    /// True after `Create task` was pressed on a new ticket with no title, until a title is typed.
    ///
    /// `task-2263`: *"When I create a task with no title, it just dissappears. there should be a validation
    /// message that highlights the required input in red."* The modal draws the title field outlined in the
    /// design's failed colour with a sentence under it while this is set.
    pub title_missing: bool,
    /// True while the description is being read as markdown rather than written as its source.
    ///
    /// `task-28`. **Raw is the default**, because the description is the field somebody writes in and a
    /// description that had to be switched into an editable state before it could be typed into would be worse
    /// than one that is simply open. Rendered is a read of it.
    ///
    /// Not written to the database: this is how somebody is looking at a ticket right now, not a property of the
    /// ticket.
    pub description_rendered: bool,
    /// The comments being read as their source rather than as markdown, by id.
    ///
    /// The other way round from the description, and for the matching reason: a comment is read far more often
    /// than it is written, and an agent's comments are markdown with headings, lists and code in them. So a
    /// comment starts **rendered** and this holds the ones somebody has asked to see the source of.
    pub comments_raw: std::collections::HashSet<i64>,
}

/// One group in the Backlog or the Completed view: a sprint, or the backlog itself, and its tickets.
///
/// A value rather than a pair of parallel lists, so that "which group is this row in" is one field and the
/// drawing cannot pair the wrong heading with the wrong tickets.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// The sprint, or `None` for the backlog — the tickets that are in no sprint at all.
    pub sprint: Option<Sprint>,
    pub tasks: Vec<Task>,
}

impl Group {
    /// The id a ticket dropped in this group is given, which is `None` for the backlog.
    pub fn id(&self) -> Option<i64> {
        self.sprint.as_ref().map(|sprint| sprint.id)
    }

    pub fn name(&self) -> &str {
        self.sprint.as_ref().map(|sprint| sprint.name.as_str()).unwrap_or("Backlog")
    }
}

/// Split `arguments` into the longest run at the start that names something, and the rest.
///
/// **A sprint and an epic are named by name, and a name has spaces in it.** `August 2nd Half` is three
/// arguments, so a command taking `<sprint> <name>` cannot simply read the first of them — and asking
/// somebody to look an id up first would be asking them to read the database to rename a fortnight. So the
/// longest prefix that names one wins.
///
/// **The longest prefix is a guess, and it is a guess with a way out.** With sprints called `August` and
/// `August 2nd Half`, `sprint-rename August 2nd Half Planning` renames the longer one and there is no way to
/// spell the other reading — the `task-1771` review is right about that. What there is instead is the **id**,
/// which is one argument, cannot be a prefix of anything, and is tried first: `sprint-rename 3 2nd Half
/// Planning` says exactly one thing. `plugins view agent-tasks` is where an id is read off, and it is what
/// the board's own buttons pass. A quoting rule would be the alternative and would be a second grammar in a
/// command line that has none.
///
/// `found` says whether a run of words names something, and answers which one. `None` when nothing does.
fn split_off_a_name(
    arguments: &[String],
    found: impl Fn(&str) -> Option<usize>,
) -> Option<(usize, String)> {
    // **An id is one argument and settles it outright**, which is what the board's own buttons pass and
    // what a caller with two sprints of the same name reaches for. It is tried first because it cannot be
    // ambiguous: a name is words and an id is a number, and no sprint is called `7`.
    if let Some(first) = arguments.first() {
        if first.parse::<i64>().is_ok() {
            if let Some(at) = found(first) {
                return Some((at, arguments[1..].join(" ")));
            }
        }
    }
    for taken in (1..=arguments.len()).rev() {
        let said = arguments[..taken].join(" ");
        if let Some(at) = found(&said) {
            return Some((at, arguments[taken..].join(" ")));
        }
    }
    None
}

/// The colours an epic can be given.
///
/// **The browser board's own seven**, from `EpicsView.tsx`, so that a board read in one place and the same
/// board read in the other are the same colours. They are data rather than palette: an epic's colour is the
/// one thing on this board that comes from the database, which is the allowance the file icons already have.
pub const SWATCHES: &[&str] =
    &["#2F6BFF", "#8B6BFF", "#FF6B5B", "#2FCFA6", "#FFB648", "#FF4F7A", "#6B7686"];

/// What the ticket modal's `rux` controls keep between frames.
///
/// `rux` keeps a component's decoration canvases and its icon marks in a `RuxState`, and a dropdown's open
/// flag in a `SelectState`; both belong to the caller, which is what lets several dropdowns sit in one dialog.
/// Held on the provider because a component in Unluminous holds nothing — the rule
/// `services::agent_chat::ModelSelect` keeps for the chat pane's own dropdown.
pub struct TicketKit {
    pub rux: rux::RuxState,
    /// Each dropdown's state, by the name of the field it sets.
    pub selects: std::collections::HashMap<&'static str, rux::components::SelectState>,
    /// The text boxes the modal drew this frame, in the order `Tab` walks them. `task-2214`: *"Tab press should
    /// navigate fields/inputs."* Written while the modal is drawn and read once it has been, so a box that is not
    /// drawn, such as the description while it is read rendered, is not in the walk.
    pub tab_order: Vec<egui::Id>,
    /// The frame the last `Tab` was acted on in, so a frame egui draws twice moves the keyboard once.
    pub tab_frame: Option<u64>,
}

impl TicketKit {
    /// Drawn in `rux`'s dark theme, deterministically in a test, for the reason `ModelSelect::new` gives: a
    /// screenshot of the modal must be the same picture on every machine.
    pub fn new() -> Self {
        let theme = crate::theme::rux_theme();
        Self {
            rux: crate::theme::rux_state(theme),
            selects: Default::default(),
            tab_order: Vec::new(),
            tab_frame: None,
        }
    }
}

impl Default for TicketKit {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TicketKit {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("TicketKit").field("selects", &self.selects.len()).finish()
    }
}

/// The Agent-Tasks provider.
#[derive(Default)]
pub struct AgentTasks {
    store: Option<Store>,
    configuration: Configuration,
    /// The plugin's own folder, once it has been opened.
    folder: Option<PathBuf>,
    /// The window's project, which is where an agent is launched when a ticket names none.
    project: Option<PathBuf>,
    /// The folders this machine has had open, from `plugin_ui::Context`, which are the choices a ticket's
    /// `Project` dropdown offers.
    recent_projects: Vec<PathBuf>,
    board: Board,
    view: View,
    /// The ticket showing in the detail, or nothing while the lanes are showing.
    detail: Detail,
    /// What is typed in the search box, and what it found.
    query: String,
    results: Vec<Task>,
    /// What the pane last said, drawn under the header until the next command.
    message: String,
    /// The card being dragged, and which lane the pointer is over.
    pub dragging: Option<i64>,
    /// The Backlog and Completed views, read when one of them is showing and a command changed something.
    backlog: Vec<Task>,
    completed: Vec<Task>,
    /// The Backlog and Completed views, as the groups they are drawn in.
    ///
    /// `task-1771` asks that these two look like the page this board is modelled on, and that page groups by
    /// **sprint**: every sprint that is not finished, then the backlog itself, each with its own heading and
    /// its own count, and a ticket dragged from one to another changes which sprint it is in. A flat column
    /// of every ticket that has no sprint, which is what this was, answers none of that.
    ///
    /// Read when the view is chosen and again after anything changes, which is the rule the lanes already
    /// keep: a query inside the drawing is the one thing the design says the board never does.
    groups: Vec<Group>,
    /// Which sprints are folded shut in the Completed view, by id.
    collapsed: Vec<i64>,
    /// How many tickets name each epic, by epic id — counted in the database, because the board holds only
    /// the active sprint and an epic used entirely in the backlog would otherwise read as zero.
    epic_counts: Vec<(i64, i64)>,
    /// The ticket being carried in the Backlog view, and the group the pointer is over.
    carrying: Option<i64>,
    over_group: Option<Option<i64>>,
    /// What is being typed into the Backlog view's `New sprint` box, and whether it is open.
    new_sprint: String,
    naming_a_sprint: bool,
    /// The same for the Epics view, with the colour chosen beside the box.
    new_epic: String,
    new_epic_colour: String,
    /// The epic being renamed in place, and the name so far.
    renaming_epic: Option<i64>,
    epic_name_draft: String,
    /// Where the ticket modal left room for its decoration on the frame just drawn. See
    /// `plugin_ui::ChromeSlot`.
    modal_canvas: Option<crate::services::plugin_ui::ChromeSlot>,
    /// A sprint or an epic whose delete button has been pressed once.
    ///
    /// **Asked rather than done**: the browser board puts a `confirm()` in the way of both of these, and a
    /// plugin cannot open the window's own confirmation, so the row asks in place. A ticket's own `Delete task`
    /// does not ask any more (`task-2198`).
    sprint_to_delete: Option<i64>,
    epic_to_delete: Option<i64>,
    /// How far the lanes are scrolled sideways.
    lane_scroll: f32,
    /// How far each lane is scrolled down, by lane. Four at most, so a list rather than a map.
    lane_downs: Vec<(Status, f32)>,
    /// How far the Backlog or Completed listing is scrolled down. One of the two is showing at a time.
    listing_down: f32,
    /// How far the open ticket's todo list is scrolled down.
    pub todo_scroll: f32,
    /// How far the rendered description is scrolled down.
    ///
    /// Beside `todo_scroll`, which is the same kind of value for the same reason: the painting is at absolute
    /// positions, so a scroll is a number this holds rather than something an `egui::ScrollArea` remembers.
    pub description_scroll: f32,
    /// The rendered markdown the open ticket is showing, kept between frames.
    ///
    /// Here rather than on `Detail` because a laid out page is not `Clone` and `Detail` is. It is drawing state,
    /// which is what `todo_scroll` and `modal_open` beside it already are.
    pub markdown: crate::components::markdown_text::Cache,
    /// The AI endpoints from `Settings -> AI Configuration`, which a ticket is given to by name
    /// (`task-2247`). Empty for a board nobody gave any to, which is a test's: it then offers the kinds of agent
    /// by their own names.
    pub endpoints: Vec<unluminous_chat::Provider>,
    /// Which card the keyboard is on: a lane and how far down it.
    ///
    /// A lane and a row rather than a ticket's id, because the board changes under it — a card moved, a ticket
    /// deleted, a search narrowing the lanes — and a row that no longer exists is clamped to the last one there
    /// is, where an id that no longer exists would leave the keyboard on nothing. `None` until an arrow key is
    /// pressed, so a board nobody has touched with the keyboard draws no ring.
    pub chosen: Option<(Status, usize)>,
    /// Whether the ticket's Todos section has been opened.
    ///
    /// **Shut until somebody opens it** (`task-2263`: *"The todos are for agents. Todos should be collapsed by
    /// default."*). Named for what open means so that the default, `false`, which `#[derive(Default)]` gives every
    /// field on this provider, is shut. Held on the provider rather than in egui's memory because the pane and the
    /// modal are two places showing one ticket and they have to agree about it.
    pub todos_open: bool,
    /// Whether the ticket's Agent section has been shut. Open until somebody shuts it, which is what the browser
    /// board does: a person who wanted more room for the description or the comments could not get it, and on a
    /// short window its fixed budget pushed the other sections past the bottom edge.
    pub terminal_shut: bool,
    /// Whether the ticket's Description section has been shut, by the chevron before its heading (`task-2269`:
    /// *"The description and comments of task modal in agent chat should have a collapse toggle similar to Agent
    /// chat and todos."*). Open until somebody shuts it, as the Agent section is.
    pub description_shut: bool,
    /// What the window last said about dictation into the ticket's description (`task-2293`), which draws the
    /// microphone beside the Description heading.
    pub voice: crate::services::plugin_ui::VoiceView,
    /// The dictation filling the description, while one is.
    pub dictating: Option<crate::services::agent_chat::Dictating>,
    /// Whether the ticket's Comments section, the list and the box a comment is written in, has been shut
    /// (`task-2269`). Open until somebody shuts it.
    pub comments_shut: bool,
    /// How tall the ticket's description well is, once somebody has dragged its bottom edge.
    ///
    /// `task-2214`: *"The description in the modal needs to be resizable so it can take less height."* `None` is
    /// the height the modal gives it on its own, and a double click on the edge puts it back there. Held on the
    /// provider for the reason the two folds are: the drag and `plugins run agent-tasks description-height` are
    /// two ways of setting one number.
    pub description_height: Option<f32>,
    /// How tall the agent's terminal and the comments are, once somebody has dragged their bottom edges
    /// (`task-2255`). `None` is the height the modal gives each on its own, as for the description.
    pub terminal_height: Option<f32>,
    pub comments_height: Option<f32>,
    /// How tall the ticket modal's comment box is, once somebody has dragged the mark in its corner (`task-2260`).
    /// `None` is three lines.
    pub comment_box_height: Option<f32>,
    /// True while the ticket modal is open.
    ///
    /// Separate from whether a ticket is open in the detail, because the pane shows a ticket in place and the
    /// modal shows the same ticket over everything: one ticket, two ways of looking at it.
    pub modal_open: bool,
    /// What the ticket modal's `rux` controls keep between frames: their decoration and which dropdown
    /// is open. Made the first time the modal is drawn. `task-2193`.
    pub ticket_kit: Option<TicketKit>,
    /// What the board's own `rux` parts keep between frames: the decoration of the header and of each
    /// lane. Made the first time the board is drawn. `task-2241`.
    pub board_rux: Option<rux::RuxState>,
    /// True while a selection is being dragged out in a ticket's terminal.
    pub terminal_selecting: bool,
    /// True when the ticket's terminal has the keyboard, so typing goes to the agent rather than to the
    /// board's own fields.
    pub terminal_focused: bool,
    /// A terminal for each ticket that has one, keyed by ticket id.
    terminals: Vec<TicketTerminal>,
    /// How a terminal asks the window to draw again, handed over when the provider is opened.
    ///
    /// `None` in a test, where nothing is drawing and a waker that does nothing is the right waker.
    waker: Option<unluminous_terminal::Waker>,
    /// What the board's files looked like when this window last read the board, and when they were last
    /// looked at. See [`Self::notice_another_window`].
    files_read: Option<FilesStamp>,
    files_asked: Option<std::time::Instant>,
    /// The open ticket's conversation, read from its agent's transcript for the modal's Chat tab (`task-2261`).
    /// `None` while no ticket with a session is open.
    pub ticket_chat: Option<ticket_chat::TicketChat>,
    /// Which tab of the ticket modal's Agent section somebody chose. `None` until one is chosen, and then
    /// [`ticket_chat::AgentTab::default_for`] the ticket's agent decides (`task-2261`).
    pub agent_tab: Option<ticket_chat::AgentTab>,
    /// What the board asked the window to run while nobody was looking, drained by [`UiProvider::asking`]. The
    /// `To Side Pane` button's two commands to Agent-Chat go this way (`task-2269`).
    asking: Vec<Request>,
    /// Whether the card of fields down the right of the ticket modal has been put away, which gives the
    /// description, the agent and the comments the whole width (`task-2261`).
    pub fields_shut: bool,
    /// True while the supervisor's modal is open: its terminal, the box a message is typed into and `Send`.
    pub supervisor_open: bool,
    /// What is typed in the supervisor modal's message box.
    pub supervisor_draft: String,
}

/// The terminal the supervisor runs in is kept with the tickets' terminals, under an id no ticket has: a ticket's
/// id is a row id, and those start at one.
pub const SUPERVISOR: i64 = 0;

/// The newest modified time, the total size and the number of the board's own files: `board.rdb` and the
/// `board.rdb-wal.*` files the engine writes beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FilesStamp {
    newest: std::time::SystemTime,
    bytes: u64,
    count: usize,
}

/// How often a window looks at the board's files for a change another window made.
const FILES_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

/// The stamp of the board's files now, or `None` when the folder cannot be read.
///
/// @param database - the path of `board.rdb`
fn stamp_the_files(database: &std::path::Path) -> Option<FilesStamp> {
    let folder = database.parent()?;
    let name = database.file_name()?.to_string_lossy().into_owned();
    let mut stamp = FilesStamp { newest: std::time::SystemTime::UNIX_EPOCH, bytes: 0, count: 0 };
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if file != name && !file.starts_with(&format!("{name}-")) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else { continue };
        stamp.bytes += metadata.len();
        stamp.count += 1;
        if let Ok(modified) = metadata.modified() {
            stamp.newest = stamp.newest.max(modified);
        }
    }
    Some(stamp)
}

impl std::fmt::Debug for AgentTasks {
    /// Written by hand because a `Waker` is a function and a `Session` holds a thread, and neither has
    /// a `Debug`. What is printed is what a test wants to see when an assertion fails.
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("AgentTasks")
            .field("open", &self.is_open())
            .field("database", &self.configuration.database_said(self.folder.as_deref()))
            .field("view", &self.view)
            .field("cards", &self.board.total())
            .field("terminals", &self.terminals.len())
            .finish()
    }
}

impl AgentTasks {
    pub fn new() -> Self {
        Self::default()
    }

    /// The board, for the drawing and for the tests.
    pub fn board(&self) -> &Board {
        &self.board
    }

    /// What the board is showing. Named `current_view` because [`UiProvider::view`] is the answer to a
    /// different question — what the pane holds, as data — and two functions called `view` on one type
    /// would be two things a reader has to tell apart.
    pub fn current_view(&self) -> View {
        self.view
    }

    /// Show one of the four views, reading whatever that view needs.
    pub fn set_view(&mut self, view: View) {
        self.view = view;
        // The listings are read here rather than while drawing, so choosing the view is what costs the query.
        if let Err(problem) = self.read_the_listings() {
            self.message = problem;
        }
    }

    /// Read the Backlog and the Completed views, which are two queries the lanes do not need.
    ///
    /// Only for the view that is showing: a board looking at its lanes has no reason to know what is in its
    /// backlog, and reading both on every refresh would be two queries nobody asked for.
    fn read_the_listings(&mut self) -> Result<(), String> {
        match self.view {
            View::Backlog => {
                self.backlog = self.store()?.backlog()?;
                self.groups = self.read_the_groups(false)?;
            }
            View::Completed => {
                self.completed = self.store()?.completed()?;
                self.groups = self.read_the_groups(true)?;
            }
            View::Epics => self.epic_counts = self.store()?.epic_counts()?,
            _ => {}
        }
        Ok(())
    }

    /// The groups one of the two listings is drawn in.
    ///
    /// `finished` picks which: the Completed view is every sprint that has been closed, newest first, and
    /// the Backlog view is every sprint that has not been, in the order they were made, with the backlog
    /// itself last — which is where the page this is modelled on puts it, because the backlog is what is
    /// left over rather than a sprint of its own.
    fn read_the_groups(&self, finished: bool) -> Result<Vec<Group>, String> {
        let store = self.store()?;
        let mut sprints: Vec<Sprint> = store
            .sprints()?
            .into_iter()
            .filter(|sprint| (sprint.status == SprintStatus::Completed) == finished)
            .collect();
        if finished {
            sprints.reverse();
        }
        let mut groups = Vec::with_capacity(sprints.len() + 1);
        for sprint in sprints {
            let tasks = store.tasks_of_sprint(sprint.id)?;
            groups.push(Group { sprint: Some(sprint), tasks });
        }
        if !finished {
            groups.push(Group { sprint: None, tasks: store.backlog()? });
        }
        Ok(groups)
    }

    /// The groups the view that is showing is drawn in.
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    /// How many tickets name this epic, anywhere on the board.
    pub fn epic_count(&self, epic: i64) -> i64 {
        self.epic_counts.iter().find(|(id, _)| *id == epic).map(|(_, count)| *count).unwrap_or(0)
    }

    /// Keep room for the decoration behind the ticket modal, and say where it went.
    ///
    /// Called by `components::agent_tasks::ticket_modal` as the first thing it draws, so the canvas lands
    /// **under** every widget on the modal — which is the order `show_the_plugin_panes` keeps for a pane and
    /// the reason a provider must never paint its own ground.
    pub fn reserve_the_modals_canvas(&mut self, ui: &mut egui::Ui, area: egui::Rect) {
        self.modal_canvas = Some(crate::services::plugin_ui::ChromeSlot {
            id: egui::Id::new("agent-tasks-modal-canvas"),
            area,
            painter: ui.painter().clone(),
            shape: ui.painter().add(egui::Shape::Noop),
        });
    }

    /// Whether a completed sprint is folded shut, and the toggle for it.
    pub fn is_collapsed(&self, sprint: i64) -> bool {
        self.collapsed.contains(&sprint)
    }

    pub fn toggle_collapsed(&mut self, sprint: i64) {
        match self.collapsed.iter().position(|id| *id == sprint) {
            Some(at) => {
                self.collapsed.remove(at);
            }
            None => self.collapsed.push(sprint),
        }
    }

    /// The ticket being carried between groups in the Backlog view, and the group under the pointer.
    ///
    /// Reported by the drawing and settled here, which is the same three-pass shape a card dragged between
    /// lanes already follows: a row cannot know which group the pointer ended up over, because each is drawn
    /// before the ones below it.
    pub fn carry(&mut self, task: Option<i64>) {
        self.carrying = task;
    }

    pub fn carrying(&self) -> Option<i64> {
        self.carrying
    }

    pub fn hover_group(&mut self, group: Option<Option<i64>>) {
        self.over_group = group;
    }

    pub fn hovered_group(&self) -> Option<Option<i64>> {
        self.over_group
    }

    /// Put the ticket that was being carried into the group it was let go over.
    pub fn drop_the_carried_row(&mut self) -> Result<(), String> {
        let (Some(task), Some(sprint)) = (self.carrying.take(), self.over_group.take()) else {
            self.carrying = None;
            self.over_group = None;
            return Ok(());
        };
        let now = clock::now();
        self.store()?.set_sprint_of(task, sprint, &now)?;
        self.refresh()
    }

    /// The boxes the two listings type into, which the drawing borrows and the commands set.
    pub fn new_sprint_draft(&mut self) -> &mut String {
        &mut self.new_sprint
    }

    pub fn naming_a_sprint(&self) -> bool {
        self.naming_a_sprint
    }

    pub fn name_a_sprint(&mut self, naming: bool) {
        self.naming_a_sprint = naming;
        if !naming {
            self.new_sprint.clear();
        }
    }

    pub fn new_epic_draft(&mut self) -> &mut String {
        &mut self.new_epic
    }

    /// The colour a new epic would be given, which is the first swatch until one is pressed.
    ///
    /// The fallback is here rather than in a constructor, because this provider is `#[derive(Default)]` and
    /// a `String` defaults to empty — and a default written in two places is a default that drifts.
    pub fn new_epic_colour(&self) -> &str {
        match self.new_epic_colour.is_empty() {
            true => SWATCHES[0],
            false => &self.new_epic_colour,
        }
    }

    pub fn choose_a_colour(&mut self, colour: &str) {
        self.new_epic_colour = colour.to_owned();
    }

    pub fn renaming_epic(&self) -> Option<i64> {
        self.renaming_epic
    }

    pub fn rename_epic_from(&mut self, epic: Option<i64>, name: &str) {
        self.renaming_epic = epic;
        self.epic_name_draft = name.to_owned();
    }

    pub fn epic_name_draft(&mut self) -> &mut String {
        &mut self.epic_name_draft
    }

    pub fn sprint_to_delete(&self) -> Option<i64> {
        self.sprint_to_delete
    }

    pub fn ask_about_a_sprint(&mut self, sprint: Option<i64>) {
        self.sprint_to_delete = sprint;
    }

    pub fn epic_to_delete(&self) -> Option<i64> {
        self.epic_to_delete
    }

    pub fn ask_about_an_epic(&mut self, epic: Option<i64>) {
        self.epic_to_delete = epic;
    }

    pub fn detail(&self) -> &Detail {
        &self.detail
    }

    pub fn detail_mut(&mut self) -> &mut Detail {
        &mut self.detail
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn results(&self) -> &[Task] {
        &self.results
    }

    /// Where this board's file is, and `None` for a board in memory — which is a window with no
    /// settings folder, so a test's window. See [`store::Store::default_path_in`].
    pub fn database_path(&self) -> Option<PathBuf> {
        self.configuration.database_path(self.folder.as_deref())
    }

    /// The same as text, with `in memory` for a board that has no file.
    pub fn database_said(&self) -> String {
        self.configuration.database_said(self.folder.as_deref())
    }

    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    /// The board that is actually open, as a person reads it.
    ///
    /// The **open** board rather than the configured path, because those differ in the case that matters:
    /// a window with no settings folder — which is every window a test builds — opens its board in memory,
    /// and a page that named a file on disk there would be naming a file nothing had written. It is also
    /// what keeps this machine's own path out of an accepted screenshot.
    pub fn board_where(&self) -> String {
        match self.store.as_ref() {
            Some(store) if store.path() == std::path::Path::new(":memory:") => {
                "in memory \u{2014} this window has no settings folder, so nothing is written"
                    .to_owned()
            }
            Some(store) => store.path().display().to_string(),
            None => "not open".to_owned(),
        }
    }

    /// The store, or a sentence saying the board is not open.
    ///
    /// Every command goes through this rather than unwrapping, because a provider whose `open` failed is
    /// a provider whose pane draws the reason rather than one that panics on the first click.
    fn store(&self) -> Result<&Store, String> {
        self.store.as_ref().ok_or_else(|| "the board is not open".to_owned())
    }

    /// The sprint a command named, by its name or by its id.
    ///
    /// By name first, because that is what is on the screen and what somebody reading the board would type;
    /// the id is the answer for two sprints called the same thing. The refusal lists what there is, which is
    /// what a caller who guessed wrong needs — the rule every refusal on this board keeps.
    fn sprint_named(&self, said: &str) -> Result<Sprint, String> {
        let sprints = self.store()?.sprints()?;
        let found = sprints
            .iter()
            .find(|sprint| sprint.name.eq_ignore_ascii_case(said.trim()))
            .or_else(|| {
                said.trim().parse::<i64>().ok().and_then(|id| sprints.iter().find(|s| s.id == id))
            });
        found.cloned().ok_or_else(|| {
            format!(
                "there is no `{}` sprint: this board has {}",
                said,
                match sprints.is_empty() {
                    true => "none".to_owned(),
                    false => sprints
                        .iter()
                        .map(|sprint| sprint.name.clone())
                        .collect::<Vec<String>>()
                        .join(", "),
                }
            )
        })
    }

    /// The same, for an epic.
    fn epic_named(&self, said: &str) -> Result<Epic, String> {
        let epics = self.store()?.epics()?;
        let found =
            epics.iter().find(|epic| epic.name.eq_ignore_ascii_case(said.trim())).or_else(|| {
                said.trim().parse::<i64>().ok().and_then(|id| epics.iter().find(|e| e.id == id))
            });
        found.cloned().ok_or_else(|| {
            format!(
                "there is no `{}` epic: this board has {}",
                said,
                match epics.is_empty() {
                    true => "none".to_owned(),
                    false => epics
                        .iter()
                        .map(|epic| epic.name.clone())
                        .collect::<Vec<String>>()
                        .join(", "),
                }
            )
        })
    }

    /// Read the board again. The one place a query happens outside a command.
    pub fn refresh(&mut self) -> Result<(), String> {
        let board = self.store()?.board()?;
        self.board = board;
        self.files_read = self.store.as_ref().and_then(|store| stamp_the_files(store.path()));
        if let Some(open) = self.detail.task.as_ref().map(|task| task.id) {
            self.open_detail(open)?;
        }
        if !self.query.is_empty() {
            self.results = self.store()?.search(&self.query)?;
        }
        self.read_the_listings()
    }

    /// Show one ticket in the detail, reading its todos and comments.
    pub fn open_detail(&mut self, id: i64) -> Result<(), String> {
        let store = self.store()?;
        let task = store.task(id)?;
        let (todos, comments) = match task.as_ref() {
            Some(task) => (store.todos(task.id)?, store.comments(task.id)?),
            None => (Vec::new(), Vec::new()),
        };
        let draft = std::mem::take(&mut self.detail.draft);
        let todo_draft = std::mem::take(&mut self.detail.todo_draft);
        // The title field keeps what is in it while the same ticket is open, and takes the row's title when a
        // different one is opened. Otherwise a refresh would overwrite what somebody was typing.
        let same =
            self.detail.task.as_ref().map(|open| open.id) == task.as_ref().map(|read| read.id);
        let title_draft = match same {
            true => std::mem::take(&mut self.detail.title_draft),
            false => task.as_ref().map(|read| read.title.clone()).unwrap_or_default(),
        };
        let is_new = self.detail.is_new && same;
        let title_missing = self.detail.title_missing && same;
        // A comment being edited survives a refresh of the same ticket and is dropped when a different one is
        // opened, on the same terms as the title field: a refresh happens every time anything on the board
        // changes, and one of those must not throw away what somebody has typed.
        let (editing_comment, comment_edit) = match same {
            true => (self.detail.editing_comment, std::mem::take(&mut self.detail.comment_edit)),
            false => (None, String::new()),
        };
        // How somebody is looking at this ticket, carried across a refresh the way the drafts above are: a refresh
        // happens whenever anything on the board changes, and a description that flipped back to its source
        // every time a todo was ticked would be unusable. Cleared when the ticket itself changes.
        let same_ticket =
            self.detail.task.as_ref().map(|open| open.id) == task.as_ref().map(|next| next.id);
        let (description_rendered, comments_raw) = match same_ticket {
            true => {
                (self.detail.description_rendered, std::mem::take(&mut self.detail.comments_raw))
            }
            false => {
                self.markdown.forget();
                (false, std::collections::HashSet::new())
            }
        };
        // **The ring is clamped here, where the board changes**, not only when an arrow key moves it. A person who
        // ringed the last card in a lane and then deleted it, or typed a search that hid it, was left with a ring
        // on nothing: it vanished from the screen and Enter did nothing until another arrow was pressed.
        self.keep_the_ring_on_a_card();
        self.detail = Detail {
            task,
            todos,
            comments,
            draft,
            todo_draft,
            title_draft,
            is_new,
            title_missing,
            editing_comment,
            comment_edit,
            description_rendered,
            comments_raw,
        };
        Ok(())
    }

    pub fn close_detail(&mut self) {
        self.detail = Detail::default();
    }

    /// Search, or clear the results when the query is empty.
    pub fn search(&mut self, query: &str) -> Result<(), String> {
        self.query = query.to_owned();
        self.results = match query.trim().is_empty() {
            true => Vec::new(),
            false => self.store()?.search(query)?,
        };
        Ok(())
    }

    /// The ticket a key names, or a sentence saying it is not on the board.
    fn by_key(&self, key: &str) -> Result<Task, String> {
        self.store()?.task_by_key(key)?.ok_or_else(|| format!("{key} is not on this board"))
    }

    /// Where an agent for this ticket is launched.
    ///
    /// The ticket's own project, then the one the settings name, then the folder this window has open.
    /// A window with no project and a ticket with none is refused rather than launching in whatever
    /// folder Unluminous happens to have started in.
    pub fn working_folder(&self, task: &Task) -> Result<PathBuf, String> {
        let named = task
            .project
            .as_ref()
            .map(PathBuf::from)
            .or_else(|| self.configuration.project.clone())
            .or_else(|| self.project.clone());
        let folder = named.ok_or_else(|| {
            format!(
                "{} names no project and this window has no folder open, so there is nowhere to launch an agent",
                task.key
            )
        })?;
        match folder.is_dir() {
            true => Ok(folder),
            false => Err(format!("{} is not a folder", folder.display())),
        }
    }

    /// The session id a ticket's agent answers to, or `None` when there is none to hand back.
    ///
    /// A Claude ticket's recorded id is the one Claude was started with. Codex and OpenCode choose their own,
    /// so a ticket of theirs carries a `pending-` marker until the session is found in the agent's own
    /// records (`native_session`), and the id found is then written to the ticket so it is looked up once.
    /// `task-2285`.
    fn native_session_for(&mut self, task: &Task, agent: Assignee) -> Option<String> {
        let recorded = task.session_id.clone()?;
        let found = match agent {
            Assignee::Claude => return Some(recorded),
            Assignee::Human => return None,
            Assignee::Codex => {
                let home = native_session::codex_home()?;
                if native_session::codex_knows(&home, &recorded) {
                    return Some(recorded);
                }
                let folder = self.working_folder(task).ok()?;
                // An older marker carries no time, and the ticket key in the first message is enough on its own.
                native_session::codex_thread(
                    &home,
                    &folder,
                    &task.key,
                    native_session::started_at(&recorded),
                )?
            }
            Assignee::OpenCode => {
                let data = native_session::opencode_data()?;
                if recorded.starts_with("ses_") && native_session::opencode_knows(&data, &recorded)
                {
                    return Some(recorded);
                }
                let folder = self.working_folder(task).ok()?;
                native_session::opencode_session(
                    &data,
                    &folder,
                    native_session::started_at(&recorded)?,
                )?
            }
        };
        if let Ok(store) = self.store() {
            let _ = store.set_session(task.id, &found, &clock::now());
        }
        Some(found)
    }

    /// Start an agent on a ticket and hand it over.
    ///
    /// The claim is a guarded update, so two callers pressing start on one card do not both get it. The
    /// session id is chosen here and written to the row, which is what makes the conversation resumable
    /// after this Unluminous has closed.
    pub fn start(&mut self, key: &str) -> Result<Answer, String> {
        let task = self.by_key(key)?;
        let (agent, endpoint) = match task.assignee.is_an_agent() {
            true => (task.assignee, task.endpoint.clone()),
            // A ticket the JIRA sync wrote is assigned to a person. Handing it to the configured endpoint
            // is what the board being replaced does, and it is the only way such a ticket reaches an
            // agent without somebody opening the detail and changing the assignee by hand.
            false => self.default_assignee(),
        };
        let folder = self.working_folder(&task)?;
        // A ticket that already has a session is already being worked, and starting it again would be a
        // second agent on one ticket. Which of the two things a person meant is not a guess: `Resume
        // session` is the other button, and it says so.
        // A ticket whose agent is **running** cannot be started again: that would be two agents on one ticket.
        if self.terminal_for(task.id).is_some_and(|terminal| terminal.session.is_running()) {
            return Err(format!("{key} already has an agent running"));
        }
        // A ticket whose agent has exited can be started again, and for a Codex or OpenCode ticket whose session
        // cannot be found that is the **only** way on, because Resume refuses it. Refusing Start as well left such
        // a ticket trapped between two refusals with nothing to press.
        //
        // Start on a ticket that has a session hands the same conversation back rather than beginning a second
        // one, because losing it would lose the work. For Codex and OpenCode that is the session found in the
        // agent's own records (`task-2285`); when none is found, a new conversation begins as it always did.
        let native = match task.session_id.is_some() {
            true => self.native_session_for(&task, agent),
            false => None,
        };
        let resuming = native.is_some();
        let session = native.unwrap_or_else(|| match agent {
            Assignee::Claude => new_session_id(),
            // Codex and OpenCode name their own sessions, so the ticket carries a marker with the time it
            // started, which is what finds the session they chose when the ticket is resumed.
            _ => native_session::pending_marker(&new_session_id()),
        });
        self.terminals
            .retain(|terminal| terminal.task_id != task.id || terminal.session.is_running());
        let plan = self.plan_for(&task, agent, endpoint.as_deref(), session.clone(), resuming);
        let launch = agent::launch(&plan)?;
        let now = clock::now();
        // A fresh ticket is claimed; one being started again is already claimed and is recorded instead, which is
        // what keeps `claim` strict — only an unclaimed ticket can be claimed, so two windows cannot both take
        // one — while still letting a ticket whose agent exited be started again.
        if task.session_id.is_none() {
            let claimed = self.store()?.claim(task.id, &session, agent, &owner(), &now)?;
            if !claimed {
                return Err(format!(
                    "{key} was claimed by another window while this one was starting it"
                ));
            }
        } else {
            self.store()?.set_session(task.id, &session, &now)?;
            self.store()?.heartbeat(task.id, None, &now)?;
        }
        // The claim moved the ticket and the spawn can still fail, so a spawn that fails gives the claim
        // back. Without that the ticket would sit in In Progress naming a process that never existed, and
        // the watchdog would strike it three times before handing it on.
        let handoff = match resuming {
            true => agent::resumed(agent, key),
            false => agent::handoff(agent, key),
        };
        if let Err(problem) = self.spawn(task.id, &session, &launch, &folder, handoff) {
            // Only a claim this call took is given back. A ticket that was already claimed keeps its session:
            // the work behind it is real and the failure was the spawn.
            if task.session_id.is_none() {
                self.store()?.release(task.id, &now)?;
            }
            self.refresh()?;
            return Err(problem);
        }
        self.refresh()?;
        self.message = format!("{key} handed to {} in {}", agent.name(), folder.display());
        Ok(Answer::said(self.message.clone()).with(json!({
            "task": key,
            "agent": agent.name(),
            "session": session,
            "resuming": resuming,
            "folder": folder.display().to_string(),
            "command": launch.line(),
        })))
    }

    /// Start the board's supervisor: one agent that works the tickets in QA Failed and New one at a time.
    ///
    /// `task-2253`: *"We are missing a chat with supervisor button on agent tasks. when I click that, a modal should
    /// open that has the terminal window, along with a message input and send button."* The board being replaced
    /// has a supervisor that drains its sprint, and this one did not exist: the design left it out as "a second
    /// protocol". It is the agent the board's endpoint names, started in the board's project folder exactly the way
    /// a ticket's agent is ([`Self::spawn`]), and told the board's commands by [`agent::supervisor_handoff`].
    pub fn start_the_supervisor(&mut self) -> Result<Answer, String> {
        if self.supervisor_is_running() {
            return Ok(Answer::said("the supervisor is already running"));
        }
        let (agent, endpoint) = self.default_assignee();
        if !agent.is_an_agent() {
            return Err(
                "the board's agent is a person, so there is no supervisor to start: choose an AI endpoint in Settings -> Agent-Tasks"
                    .to_owned(),
            );
        }
        let folder = self
            .configuration
            .project
            .clone()
            .or_else(|| self.project.clone())
            .ok_or("there is no project folder to start the supervisor in: open a folder, or set the board's project in Settings -> Agent-Tasks")?;
        let chosen = self.endpoint_for(agent, endpoint.as_deref());
        let plan = agent::Plan {
            agent,
            session: new_session_id(),
            model: chosen
                .as_ref()
                .map(|one| one.model.trim().to_owned())
                .filter(|model| !model.is_empty()),
            effort: None,
            resuming: false,
            command: self.configuration.command_for(agent),
            program: chosen
                .map(|one| one.command.trim().to_owned())
                .filter(|program| !program.is_empty()),
        };
        let launch = agent::launch(&plan)?;
        self.terminals.retain(|terminal| terminal.task_id != SUPERVISOR);
        self.spawn(SUPERVISOR, &plan.session, &launch, &folder, agent::supervisor_handoff(agent))?;
        self.message = format!("the supervisor is {} in {}", agent.name(), folder.display());
        Ok(Answer::said(self.message.clone()).with(json!({
            "agent": agent.name(),
            "session": plan.session,
            "folder": folder.display().to_string(),
            "command": launch.line(),
        })))
    }

    /// Whether the supervisor's agent is running.
    pub fn supervisor_is_running(&self) -> bool {
        self.terminal_for(SUPERVISOR).is_some_and(|terminal| terminal.session.is_running())
    }

    /// Type a line into the supervisor, which is what the modal's `Send` does. Queued behind its prompt when it
    /// has only just started, the way a ticket's handoff is.
    pub fn send_to_the_supervisor(&mut self, line: &str) -> Result<Answer, String> {
        let line = line.trim();
        if line.is_empty() {
            return Err("there is nothing to send".to_owned());
        }
        let terminal = self
            .terminals
            .iter_mut()
            .find(|terminal| terminal.task_id == SUPERVISOR && terminal.session.is_running())
            .ok_or("the supervisor is not running: start it first")?;
        let queued = terminal.queue(line);
        Ok(Answer::said(match queued {
            true => "sent; the supervisor will read it once its prompt is ready",
            false => "sent to the supervisor",
        })
        .with(json!({ "queued": queued })))
    }

    /// Close the supervisor's terminal, which ends its agent. The tickets it started keep their own agents.
    pub fn stop_the_supervisor(&mut self) -> Answer {
        let had = self.terminals.iter().any(|terminal| terminal.task_id == SUPERVISOR);
        self.terminals.retain(|terminal| terminal.task_id != SUPERVISOR);
        Answer::said(match had {
            true => "the supervisor was stopped",
            false => "the supervisor was not running",
        })
    }

    /// What the supervisor has written, the last `lines` of it, for the command line.
    pub fn supervisor_screen(&self, lines: usize) -> Option<String> {
        let terminal = self.terminal_for(SUPERVISOR)?;
        let text = terminal.session.written_text(Some(lines));
        Some(text)
    }

    /// The modal's `Send`: what is in its box goes to the supervisor and the box is emptied.
    pub fn send_the_supervisor_draft(&mut self) -> Result<String, String> {
        let draft = self.supervisor_draft.clone();
        let answer = self.send_to_the_supervisor(&draft)?;
        self.supervisor_draft.clear();
        Ok(answer.message)
    }

    /// The menu the agent in a ticket's terminal is waiting on, when it is waiting on one.
    pub fn choice_for(&self, task_id: i64) -> Option<screen_choice::Choice> {
        self.terminal_for(task_id)?.choice()
    }

    /// Say the menu a ticket's agent is waiting on, or press option `option` of it, counted from 1.
    ///
    /// What the buttons the Chat tab draws over a waiting menu do, and the `choice` command (`task-2263`).
    pub fn answer_a_choice(&mut self, key: &str, option: &str) -> Result<Answer, String> {
        let task = self.by_key(key)?;
        let terminal = self
            .terminals
            .iter_mut()
            .find(|terminal| terminal.task_id == task.id && terminal.session.is_running())
            .ok_or_else(|| format!("{key} has no agent running"))?;
        let described = |choice: &screen_choice::Choice| json!({"task": task.key, "question": choice.question, "options": choice.options, "cursor": choice.cursor + 1});
        if option.trim().is_empty() {
            return Ok(match terminal.choice() {
                Some(choice) => Answer::said(format!(
                    "{key} is waiting on: {} ({})",
                    choice.question,
                    choice.options.join(" / ")
                ))
                .with(described(&choice)),
                None => Answer::said(format!("{key} is not waiting on a menu"))
                    .with(json!({"task": task.key, "options": []})),
            });
        }
        let number = option
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|number| *number > 0)
            .ok_or_else(|| format!("`{option}` is not an option number: they start at 1"))?;
        let choice = terminal.choose(number - 1)?;
        self.message = format!("{key}: chose {}", choice.options[number - 1]);
        Ok(Answer::said(self.message.clone()).with(described(&choice)))
    }

    /// Bring back a session that has been retired, without changing which lane the ticket is in.
    ///
    /// This is what `Resume session` means: a ticket in Agent Done whose session is resumed stays in
    /// Agent Done, because resuming a conversation is not a claim on the work.
    pub fn resume(&mut self, key: &str) -> Result<Answer, String> {
        let task = self.by_key(key)?;
        let agent = match task.assignee.is_an_agent() {
            true => task.assignee,
            false => return Err(format!("{key} is assigned to a person, so there is no session")),
        };
        if task.session_id.is_none() {
            return Err(format!("{key} has never had a session, so there is nothing to resume"));
        }
        // Codex and OpenCode name their own sessions, so the id they answer to is found in their own records
        // (`task-2285`). Starting a fresh agent and calling it a resumed one would be the one outcome every
        // check on this board exists to prevent, so a session that cannot be found is refused.
        let session = self
            .native_session_for(&task, agent)
            .ok_or_else(|| agent::why_it_cannot_resume(agent, key))?;
        // A terminal object that is **running**, not merely one that is there. A retired session leaves its object
        // behind so its last screen can still be read, and refusing on that meant a Claude ticket whose agent had
        // exited could not be resumed at all — which is the one thing Resume session exists for.
        if self.terminal_for(task.id).is_some_and(|terminal| terminal.session.is_running()) {
            return Ok(Answer::said(format!("{key} already has an agent running")));
        }
        // The dead one is dropped first, or two objects would claim one ticket.
        self.terminals
            .retain(|terminal| terminal.task_id != task.id || terminal.session.is_running());
        let folder = self.working_folder(&task)?;
        let launch = agent::launch(&self.plan_for(
            &task,
            agent,
            task.endpoint.as_deref(),
            session.clone(),
            true,
        ))?;
        // The task protocol requires a resumed run to re-read the ticket and take its newest human comments
        // as the specification, so that is what it is told. An empty handoff was a resumed agent that had
        // been told nothing and would carry on from whatever it last remembered.
        self.spawn(task.id, &session, &launch, &folder, agent::resumed(agent, key))?;
        self.store()?.set_session(task.id, &session, &clock::now())?;
        self.message = format!("{key} resumed on session {session}");
        Ok(Answer::said(self.message.clone()).with(json!({
            "task": key,
            "session": session,
            "command": launch.line(),
        })))
    }

    /// Type a line into a ticket's agent, starting one first when none is running.
    ///
    /// Typing at a process that has exited writes to nothing, and the comment would look sent, so an agent is
    /// brought up first. A ticket that still has its session is resumed, which keeps its lane, as
    /// `Resume session` does; for Codex and OpenCode that is the session found in the agent's own records
    /// (`task-2285`), and a ticket whose session cannot be found is started instead. Any other ticket is
    /// started: one the watchdog moved back to New for inactivity has had its session cleared.
    ///
    /// `task-2268`: *"a task got moved back to new due to inactivity. I then added a comment and sent to
    /// terminal ... It should wake/start the agent."* It used to call `resume` only, so that send was refused
    /// with "has never had a session" and the comment was not posted.
    pub fn send(&mut self, key: &str, line: &str) -> Result<Answer, String> {
        let task = self.by_key(key)?;
        // **Running**, not merely present. Typing at a session that has exited wrote into a closed pipe and
        // reported `sent`, which is the one thing a send must never do.
        let alive =
            self.terminal_for(task.id).is_some_and(|terminal| terminal.session.is_running());
        if !alive {
            let resumable = task.assignee.is_an_agent()
                && agent::can_resume(task.assignee, task.session_id.as_deref());
            match resumable {
                // A Codex or OpenCode ticket whose session cannot be found in the agent's own records is
                // started instead, which is what Send did for those tickets before `task-2285`.
                true if task.assignee != Assignee::Claude => {
                    if self.resume(key).is_err() {
                        self.start(key)?;
                    }
                }
                true => {
                    self.resume(key)?;
                }
                false => {
                    self.start(key)?;
                }
            };
        }
        let terminal = self
            .terminals
            .iter_mut()
            .find(|terminal| terminal.task_id == task.id && terminal.session.is_running())
            .ok_or_else(|| format!("{key} has no agent running to type into"))?;
        // Queued rather than written, and the queue is what the handoff already waits in. An agent that has
        // just been started has not drawn its prompt, and characters typed before it does go nowhere: a
        // line written straight in would look sent and would not be.
        let queued = terminal.queue(line);
        self.message = match queued {
            true => format!("{key} is still starting, so this is queued behind its handoff"),
            false => format!("sent to {key}"),
        };
        Ok(Answer::said(self.message.clone()).with(json!({"task": key, "queued": queued})))
    }

    fn spawn(
        &mut self,
        task_id: i64,
        session_id: &str,
        launch: &agent::Launch,
        folder: &std::path::Path,
        handoff: String,
    ) -> Result<(), String> {
        // Which agent this is, so the queue waits as long as that agent takes to draw its prompt.
        let named = std::path::Path::new(&launch.program)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let agent_kind = match named.as_str() {
            "codex" => Assignee::Codex,
            "opencode" => Assignee::OpenCode,
            _ => Assignee::Claude,
        };
        // Which kind of agent a ticket started, and nothing about the ticket. `task-2271`.
        crate::services::telemetry::note(
            crate::services::telemetry::Category::AgentTask,
            agent_kind.name(),
        );
        // **The agent is started by its full path, not by its name.** `claude` installs itself into
        // `~/.local/bin` and `codex` into a package manager's folder, and neither is on the `PATH` a
        // Unluminous started from the Finder or the Dock has — launchd gives it `/usr/bin:/bin:/usr/sbin:
        // /sbin` and nothing else, so `claude` was spawned and failed with `No such file or
        // directory` while typing `claude` in the terminal tile beside it worked, because the tile
        // starts the person's shell and the shell reads their profile.
        //
        // `services::login_shell` reads that same profile, so what is looked on here is the `PATH`
        // the person really has, and `required` is what says which program is missing and where it
        // was looked for rather than leaving `No such file or directory` as the whole answer.
        let program = crate::services::login_shell::required(&launch.program)?;
        // **The folder is trusted before the agent sees it** (`task-2263`). Both agents ask whether to trust a
        // folder the first time they meet one, and every ticket worktree is a folder they have never met: the
        // question sat in the terminal, and the handoff typed a few seconds later went into it. The program's own
        // stem decides which agent's file is written, so a wrapper named in the configuration is left alone.
        let stem = program
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if let Err(problem) = folder_trust::trust(&stem, folder) {
            eprintln!(
                "agent-tasks: could not record that {} is trusted: {problem}",
                folder.display()
            );
        }
        let settings = unluminous_terminal::SessionSettings {
            shell: Some(program.display().to_string()),
            args: launch.arguments.clone(),
            working_directory: Some(folder.to_path_buf()),
            // The board passes the two variables an agent needs to reach it. They are not secrets: the
            // board is a file this process already has open, and the agent reads it through the command
            // line rather than over a socket.
            // The board's own file, so an agent can find the board it is working on, plus whatever the
            // configuration says about where the agent connects and what key it uses. The key is read from the
            // keychain here and handed straight to the child: it is in this process for as long as that takes.
            env: {
                // **Everything the person's shell would have given the command**, underneath the
                // board's own values. An agent started with launchd's dozen variables found no
                // `claude` on its `PATH` and, once that was mended, said it was not logged in: the
                // gateway, the key, the custom header the gateway wants and the certificate bundle
                // are all set by `~/.zshrc` and none of them reached the child. `services::login_shell`
                // is that profile read once, so a ticket's agent runs with what a command typed in
                // the terminal tile runs with.
                let mut environment = crate::services::login_shell::for_a_child();
                // **A ticket's agent is a new conversation, so nothing naming another one may reach
                // it.** An Unluminous started from a terminal that was itself running Claude Code has that
                // session's markers in its environment, and they are inherited: measured on a real
                // window, the ticket's agent said `Transcript saving is off — inherited
                // CLAUDE_CODE_...`, wrote no transcript of its own, and showed the parent's
                // conversation instead of the ticket it had been handed — so the session id on the row
                // named nothing and `Resume session` had nothing to resume. Cleared rather than unset,
                // because `SessionSettings::env` is laid *over* what this process has and an empty value
                // is what both agents read as absent.
                for named in AGENT_SESSION_MARKERS {
                    environment.push(((*named).to_owned(), String::new()));
                }
                environment.push((
                    "UNLUMINOUS_AGENT_TASKS".to_owned(),
                    self.configuration.database_said(self.folder.as_deref()),
                ));
                // The two the handoff line names, and `unluminous-cli` on the `PATH` beside them.
                // Without them the line would have to hold an absolute path and a process id, which is a
                // line nobody could read and nobody could retype — and without the `PATH` an agent that
                // simply types the name is told there is no such command. See
                // [`how_to_reach_this_window`].
                //
                // The `PATH` it goes in front of is the **login shell's**, which is what
                // `login_shell::for_a_child` has just put in `environment` and is the one this child will
                // really have; this process's own is a different and shorter list when Unluminous was
                // started from the Dock.
                environment.extend(how_to_reach_this_window(&environment));
                environment
            },
            // The agent is the program that is started, so the tab is named after it in the ordinary way.
            // `SessionSettings::name` is the canvas's, for a node being restored.
            name: None,
        };
        let waker = self.waker.clone().unwrap_or_else(|| std::sync::Arc::new(|| {}));
        let session = unluminous_terminal::Session::spawn(
            &settings,
            unluminous_terminal::Size::new(40, 120),
            waker,
        )
        .map_err(|problem| format!("{} could not be started: {problem}", launch.program))?;
        let pending = match handoff.is_empty() {
            true => Vec::new(),
            false => vec![handoff],
        };
        self.terminals.retain(|terminal| terminal.task_id != task_id);
        self.terminals.push(TicketTerminal {
            task_id,
            session,
            session_id: session_id.to_owned(),
            last_output_at: clock::now(),
            paused: false,
            pending,
            ready_at: std::time::Instant::now() + agent::ready_after(agent_kind),
            give_up_at: std::time::Instant::now() + PROMPT_CEILING,
            submit_at: None,
            written: 0,
            has_printed: false,
            quiet_since: None,
            keys: Vec::new(),
            next_key_at: None,
        });
        Ok(())
    }

    /// Run one command and keep its message, which is what a button in the pane does.
    ///
    /// The same `command` the menu entry and `unluminous-cli plugin run` go through, so a button and an agent
    /// reach the same code rather than two paths that agree today. The arguments are owned because a
    /// button has just cloned a key out of the board it is drawing.
    pub fn command_now(&mut self, command: &str, arguments: &[String]) -> Result<Answer, String> {
        let answer = UiProvider::command(self, command, arguments);
        match &answer {
            Ok(said) if !said.message.is_empty() => self.message = said.message.clone(),
            Ok(_) => {}
            Err(problem) => self.message = problem.clone(),
        }
        answer
    }

    /// Open one ticket as a new one, which is what `+ Add Task` does.
    ///
    /// In the modal, because a new ticket needs six fields and a description and none of those fits in a pane
    /// 420 points wide. That is also the browser board's own answer.
    pub fn open_a_new_ticket(&mut self, id: i64) -> Result<(), String> {
        self.open_detail(id)?;
        self.detail.is_new = true;
        self.modal_open = true;
        Ok(())
    }

    /// Open one ticket in the modal, which is what a click on a card does when there is room for one.
    pub fn open_the_modal(&mut self, id: i64) -> Result<(), String> {
        self.open_detail(id)?;
        // **Not** a new ticket, whatever the detail last held. Opening a ticket that exists is a different thing
        // from `+ Add Task`, and the difference is what the footer says: `Close` rather than `Discard` and
        // `Done`. Without this, a ticket opened after a new one had been closed still read as an editor.
        self.detail.is_new = false;
        self.modal_open = true;
        Ok(())
    }

    /// The description of the ticket that is open, as text.
    pub fn description_text(&self) -> String {
        self.detail.task.as_ref().map(|task| task.description.clone()).unwrap_or_default()
    }

    /// Write a changed description onto the ticket.
    ///
    /// Every keystroke, which is what "saves as you type" means. There is no debounce because there is no
    /// network: writing one column of a local row is a hundred microseconds.
    pub fn save_the_description(&mut self, text: &str) -> Result<(), String> {
        let Some(id) = self.detail.task.as_ref().map(|task| task.id) else {
            return Ok(());
        };
        let edit = TaskEdit { description: Some(text.to_owned()), ..TaskEdit::default() };
        self.store()?.edit_task(id, &edit, &clock::now())?;
        self.refresh()
    }

    /// Delete the ticket that is open, which is what `Discard` and `Delete` both do.
    pub fn discard_the_ticket(&mut self) -> Result<(), String> {
        let Some(id) = self.detail.task.as_ref().map(|task| task.id) else {
            return Ok(());
        };
        self.store()?.delete_task(id)?;
        // The modal goes with the ticket. Clearing the detail and leaving `modal_open` set drew a modal over a
        // ticket that was not there.
        self.modal_open = false;
        self.close_detail();
        self.refresh()
    }

    /// Write one field of a ticket.
    ///
    /// **One function for every control in the modal's right column**, so seven dropdowns are seven calls to one
    /// place rather than seven paths that agree today. An empty value clears the field where the field can be
    /// cleared, which is what pressing the chosen option in a row of choices means.
    pub fn edit_field(&mut self, id: i64, field: Field) -> Result<(), String> {
        let empty = |value: &str| value.trim().is_empty();
        let mut edit = TaskEdit::default();
        match &field {
            Field::Assignee(value) => {
                let (kind, endpoint) = self.assignee_named(value)?;
                edit.assignee = Some(kind);
                edit.endpoint = Some(endpoint);
            }
            Field::Priority(value) => {
                edit.priority = Some(Priority::parse(value).ok_or_else(|| {
                    format!("there is no `{value}` priority: low, medium or high")
                })?);
            }
            Field::Model(value) => {
                edit.model = Some((!empty(value)).then(|| value.trim().to_owned()));
            }
            Field::Effort(value) => {
                if !empty(value) && !EFFORTS.contains(&value.as_str()) {
                    return Err(format!("there is no `{value}` effort: {}", EFFORTS.join(", ")));
                }
                edit.effort = Some((!empty(value)).then(|| value.trim().to_owned()));
            }
            Field::Project(value) => {
                edit.project = Some((!empty(value)).then(|| value.trim().to_owned()));
            }
            Field::Epic(value) => {
                let epic = match empty(value) {
                    true => None,
                    false => Some(value.trim().parse::<i64>().map_err(|_| {
                        format!("`{value}` is not an epic: an epic is named by its number")
                    })?),
                };
                edit.epic_id = Some(epic);
            }
        }
        self.store()?.edit_task(id, &edit, &clock::now())?;
        self.refresh()
    }

    /// Write what is in the title field onto the ticket.
    ///
    /// Called on every keystroke, which is one small update per character and is what "saves as you type"
    /// means. The board being replaced does the same, and for the same reason: `+ Add task` creates the row
    /// before anybody has named it, so there is no Create button to forget to press.
    pub fn save_the_title(&mut self) -> Result<(), String> {
        let Some(id) = self.detail.task.as_ref().map(|task| task.id) else {
            return Ok(());
        };
        let title = self.detail.title_draft.clone();
        if !title.trim().is_empty() {
            self.detail.title_missing = false;
        }
        let edit = TaskEdit { title: Some(title.clone()), ..TaskEdit::default() };
        self.store()?.edit_task(id, &edit, &clock::now())?;
        // The board is read again so the card's title follows the field, and the open ticket keeps the field.
        self.refresh()
    }

    /// Finish creating the new ticket that is open, which is what `Create task` and the `create` command do.
    ///
    /// A ticket with no title is refused and stays open with its title field marked (`task-2263`): it used to be
    /// closed and thrown away in one press, so the ticket somebody had just filled in disappeared with nothing said.
    /// The title is saved once more before the modal closes, so a failure to write it is reported rather than lost.
    pub fn create_the_new_ticket(&mut self) -> Result<Answer, String> {
        let Some(task) = self.detail.task.clone() else {
            return Err("there is no new ticket open to create".to_owned());
        };
        if !self.detail.is_new {
            return Ok(Answer::said(format!("{} was already created", task.key)));
        }
        if self.detail.title_draft.trim().is_empty() {
            self.detail.title_missing = true;
            return Err(TITLE_MISSING.to_owned());
        }
        self.save_the_title()?;
        self.detail.is_new = false;
        self.modal_open = false;
        self.message = format!("{} was created", task.key);
        Ok(Answer::said(self.message.clone()).with(json!({ "task": task.key })))
    }

    /// Remove one todo.
    pub fn remove_the_todo(&mut self, id: i64) -> Result<(), String> {
        self.store()?.delete_todo(id)?;
        self.refresh()
    }

    /// Send a comment that is already on the board to the ticket's agent.
    ///
    /// What the browser's `Send to terminal` on each comment does. It writes nothing new: the comment is already
    /// there, so this is only the delivery, which is the difference from `comment-send`.
    pub fn send_a_comment(&mut self, body: &str) -> Result<String, String> {
        let Some(task) = self.detail.task.clone() else {
            return Ok(String::new());
        };
        let line = agent::comment_handoff(&task.key, body);
        let answer = self.send(&task.key, &line)?;
        Ok(answer.message)
    }

    /// How far the todo list is scrolled, taking this frame's wheel into account.
    pub fn scroll_the_todos(&mut self, ui: &egui::Ui, area: egui::Rect, most: f32) -> f32 {
        if most <= 0.0 {
            self.todo_scroll = 0.0;
            return 0.0;
        }
        if crate::components::controls::pointer_in(ui).is_some_and(|at| area.contains(at)) {
            self.todo_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y);
        }
        self.todo_scroll = self.todo_scroll.clamp(0.0, most);
        self.todo_scroll
    }

    /// Add the todo that is in the box, and clear it.
    pub fn post_the_todo(&mut self) -> Result<(), String> {
        let Some(id) = self.detail.task.as_ref().map(|task| task.id) else {
            return Ok(());
        };
        let text = self.detail.todo_draft.trim().to_owned();
        if text.is_empty() {
            return Ok(());
        }
        self.store()?.add_todo(id, &text, &clock::now())?;
        self.detail.todo_draft.clear();
        self.refresh()
    }

    /// Start editing a comment, putting what it says into the draft.
    ///
    /// Only a person's own comment can be edited, and the store refuses the rest, so this does not check: a
    /// button that is only drawn on a human's comment and a store that refuses an agent's is one rule kept in
    /// the place that cannot be got round.
    pub fn edit_the_comment(&mut self, id: i64) {
        let said = self
            .detail
            .comments
            .iter()
            .find(|comment| comment.id == id)
            .map(|comment| comment.body.clone())
            .unwrap_or_default();
        self.detail.comment_edit = said;
        self.detail.editing_comment = Some(id);
    }

    /// Leave the comment as it was.
    pub fn stop_editing_the_comment(&mut self) {
        self.detail.editing_comment = None;
        self.detail.comment_edit.clear();
    }

    /// Write what was typed into the comment being edited.
    pub fn save_the_comment(&mut self) -> Result<String, String> {
        let Some(id) = self.detail.editing_comment else {
            return Ok(String::new());
        };
        let body = self.detail.comment_edit.trim().to_owned();
        if body.is_empty() {
            return Err(
                "a comment cannot be emptied; press Cancel to leave it as it was".to_owned()
            );
        }
        self.store()?.edit_comment(id, &body, &clock::now())?;
        self.stop_editing_the_comment();
        self.refresh()?;
        Ok("the comment was changed".to_owned())
    }

    /// Post the comment that is in the box, and optionally type it into the ticket's agent.
    ///
    /// `to_the_agent` is the difference between the two buttons: posting writes it on the board, and sending
    /// writes it on the board **and** types it into the terminal, resuming a retired session first. Sending
    /// goes through the same `comment-send` command an agent uses, so a failed delivery posts nothing and says
    /// so rather than leaving a comment that looks sent.
    pub fn post_the_comment(&mut self, to_the_agent: bool) -> Result<String, String> {
        let Some(task) = self.detail.task.clone() else {
            return Ok(String::new());
        };
        let body = self.detail.draft.trim().to_owned();
        if body.is_empty() {
            return Ok(String::new());
        }
        let command = match to_the_agent {
            true => "comment-send",
            false => "comment",
        };
        let arguments = vec![task.key.clone(), body];
        let answer = self.command_now(command, &arguments)?;
        self.detail.draft.clear();
        Ok(answer.message)
    }

    /// Write a changed configuration to the plugin's own settings file.
    ///
    /// One function, so seven controls on the Settings page are seven calls to one place. Nothing here is a
    /// secret: the key is in the keychain and this file names it.
    pub fn change_the_configuration(&mut self, changed: Configuration) -> Result<(), String> {
        self.configuration = changed;
        match self.folder.clone() {
            Some(folder) => self.configuration.write(&folder),
            // A window with no settings folder — every window a test builds — keeps the change in memory and
            // writes nothing, which is the rule that stops a test touching the settings of the person running
            // it.
            None => Ok(()),
        }
    }

    /// The kind of agent and the endpoint a name on a ticket means.
    ///
    /// `human` is a person. Any other name is one of the endpoints that run an agent, and an endpoint that
    /// sends to an address is refused because it cannot be launched in a terminal. With no endpoints at all,
    /// which is a board a test opened, the kinds answer to their own names.
    pub fn assignee_named(&self, name: &str) -> Result<(Assignee, Option<String>), String> {
        let name = name.trim();
        if name == Assignee::Human.name() {
            return Ok((Assignee::Human, None));
        }
        if let Some(endpoint) = self.endpoints.iter().find(|endpoint| endpoint.name == name) {
            return match Assignee::of_wire(endpoint.wire) {
                Some(kind) => Ok((kind, Some(endpoint.name.clone()))),
                None => Err(format!(
                    "`{name}` sends to an address, and a ticket can only be given to an endpoint that runs an \
                     agent on this machine: {}",
                    self.assignee_choices().join(", ")
                )),
            };
        }
        match Assignee::parse(name) {
            Some(kind) if self.endpoints.is_empty() => Ok((kind, None)),
            _ => Err(format!(
                "there is no `{name}` assignee: {}",
                self.assignee_choices().join(", ")
            )),
        }
    }

    /// The names a ticket can be given to: every endpoint that runs an agent, then `human`.
    pub fn assignee_choices(&self) -> Vec<String> {
        let mut choices: Vec<String> = match self.endpoints.is_empty() {
            true => Assignee::ALL
                .iter()
                .filter(|kind| kind.is_an_agent())
                .map(|kind| kind.name().to_owned())
                .collect(),
            false => crate::services::ai_endpoints::agents(&self.endpoints)
                .map(|endpoint| endpoint.name.clone())
                .collect(),
        };
        choices.push(Assignee::Human.name().to_owned());
        choices
    }

    /// What a ticket says it is assigned to: its endpoint's name, or its kind when it names none.
    pub fn assignee_said(task: &Task) -> String {
        task.endpoint.clone().unwrap_or_else(|| task.assignee.name().to_owned())
    }

    /// The endpoint a ticket of this kind is launched through: the one it names, or the first one of its
    /// kind. `None` when there is none, and the agent is then launched the way its kind always was.
    fn endpoint_for(
        &self,
        kind: Assignee,
        named: Option<&str>,
    ) -> Option<unluminous_chat::Provider> {
        let of_kind =
            |endpoint: &&unluminous_chat::Provider| Assignee::of_wire(endpoint.wire) == Some(kind);
        named
            .and_then(|name| {
                self.endpoints.iter().filter(of_kind).find(|endpoint| endpoint.name == name)
            })
            .or_else(|| self.endpoints.iter().find(of_kind))
            .cloned()
    }

    /// The kind and the endpoint a new ticket is given to, which is the Settings page's `Endpoint`.
    pub fn default_assignee(&self) -> (Assignee, Option<String>) {
        if let Some(named) = self.configuration.endpoint.as_deref() {
            if let Ok(found) = self.assignee_named(named) {
                return found;
            }
        }
        let endpoint = self.endpoint_for(self.configuration.agent, None).map(|one| one.name);
        (self.configuration.agent, endpoint)
    }

    /// The plan a ticket is launched with: its own model and effort, and the endpoint's program and model
    /// where the ticket names none.
    fn plan_for(
        &self,
        task: &Task,
        agent: Assignee,
        endpoint: Option<&str>,
        session: String,
        resuming: bool,
    ) -> agent::Plan {
        let endpoint = self.endpoint_for(agent, endpoint);
        let model = task.model.clone().or_else(|| {
            endpoint
                .as_ref()
                .map(|one| one.model.trim().to_owned())
                .filter(|model| !model.is_empty())
        });
        agent::Plan {
            agent,
            session,
            model,
            effort: task.effort.clone(),
            resuming,
            command: self.configuration.command_for(agent),
            program: endpoint
                .map(|one| one.command.trim().to_owned())
                .filter(|program| !program.is_empty()),
        }
    }

    /// Choose the next agent, which is what the chooser under the New lane's heading does.    /// Choose the next agent, which is what the chooser under the New lane's heading does.
    ///
    /// A cycle rather than a dropdown, because egui keeps one popup open at a time and there are two agents
    /// that can be launched. It writes the same `agent` the Settings page writes, so the chooser on the board
    /// and the setting in the window can never disagree.
    pub fn use_the_next_agent(&mut self) -> Result<(), String> {
        let mut agents = self.assignee_choices();
        agents.retain(|name| name != Assignee::Human.name());
        if agents.is_empty() {
            return Err(
                "there is no endpoint that runs an agent: Settings -> AI Configuration".to_owned()
            );
        }
        let (_, now) = self.default_assignee();
        let at = now.and_then(|name| agents.iter().position(|one| *one == name)).unwrap_or(0);
        self.use_the_endpoint(&agents[(at + 1) % agents.len()])
    }

    /// Give new tickets to the endpoint named `name`, which is the Settings page's `Endpoint`.
    pub fn use_the_endpoint(&mut self, name: &str) -> Result<(), String> {
        let (kind, endpoint) = self.assignee_named(name)?;
        if !kind.is_an_agent() {
            return Err("a new ticket is given to an agent, so a person is not one of the choices"
                .to_owned());
        }
        let mut changed = self.configuration.clone();
        changed.agent = kind;
        changed.endpoint = endpoint;
        self.change_the_configuration(changed)
    }

    /// Move the keyboard's ring to another card, and say which ticket it is on now.
    ///
    /// `across` is lanes and `down` is rows. The counts come from the drawing, because they are the counts
    /// **after** the search has narrowed each lane: a ring that could land on a card the search had hidden would
    /// be a ring nobody could see.
    pub fn move_the_choice(&mut self, counts: &[(Status, usize)], across: i64, down: i64) {
        // Somewhere to start. The first lane that holds anything, so the first arrow press lands on a card rather
        // than on an empty New lane while three tickets sit in Agent Done.
        let Some(first) = counts.iter().find(|(_, held)| *held > 0).map(|(lane, _)| *lane) else {
            self.chosen = None;
            return;
        };
        // **The first press lands on the first card rather than moving from it.** A board with no ring on it has
        // nowhere to move from, so the press that starts is the press that chooses.
        let Some((lane, row)) = self.chosen else {
            self.chosen = Some((first, 0));
            return;
        };
        let mut at = Status::ALL.iter().position(|one| *one == lane).unwrap_or(0) as i64;
        let mut row = row as i64;
        if across != 0 {
            // Past the last lane stops at the last lane rather than wrapping. Wrapping in a row of four lanes
            // means a press that looks like it goes right sends the ring back to the left, which nobody expects.
            let held = |at: i64| {
                Status::ALL
                    .get(at.clamp(0, Status::ALL.len() as i64 - 1) as usize)
                    .and_then(|lane| counts.iter().find(|(one, _)| one == lane))
                    .map(|(_, held)| *held)
                    .unwrap_or(0)
            };
            let mut moved = at;
            // Empty lanes are stepped over, because stopping the ring on a lane with no cards in it would mean
            // pressing right twice to get past `QA FAILED 0`.
            loop {
                let next = moved + across;
                if next < 0 || next >= Status::ALL.len() as i64 {
                    break;
                }
                moved = next;
                if held(moved) > 0 {
                    at = moved;
                    break;
                }
            }
            row = row.min(held(at) as i64 - 1).max(0);
        }
        let lane = Status::ALL[at.clamp(0, Status::ALL.len() as i64 - 1) as usize];
        let held =
            counts.iter().find(|(one, _)| *one == lane).map(|(_, held)| *held).unwrap_or(0) as i64;
        if held == 0 {
            self.chosen = None;
            return;
        }
        row = (row + down).clamp(0, held - 1);
        self.chosen = Some((lane, row as usize));
    }

    /// Put the ring back on a card that exists, or take it off the board.
    ///
    /// Called from `refresh`, so every change to the board goes through it: a ticket deleted, a card moved to
    /// another lane, a search typed. The row is clamped to the last one the lane has rather than dropped, because
    /// somebody who deletes the card they were on means to carry on from there.
    fn keep_the_ring_on_a_card(&mut self) {
        let Some((lane, row)) = self.chosen else {
            return;
        };
        let counts = self.lane_counts();
        let held = counts.iter().find(|(one, _)| *one == lane).map(|(_, held)| *held).unwrap_or(0);
        self.chosen = match held {
            // The lane emptied, so the ring moves to the first lane that holds anything rather than going out
            // altogether: a board with cards on it should always be one Enter away from opening one.
            0 => counts.iter().find(|(_, held)| *held > 0).map(|(lane, _)| (*lane, 0)),
            held => Some((lane, row.min(held - 1))),
        };
    }

    /// Which ticket the ring is on, if it is on one.
    pub fn the_chosen_ticket(&self, counts: &[(Status, usize)]) -> Option<i64> {
        let (lane, row) = self.chosen?;
        let held = counts.iter().find(|(one, _)| *one == lane).map(|(_, held)| *held)?;
        if row >= held {
            return None;
        }
        // Filtered by the search, the same way the lane is drawn. Reading the unfiltered lane would put the ring
        // on the card at that row and open a different ticket, because a search hides cards without removing them.
        self.board
            .lane(lane)
            .and_then(|found| {
                found.tasks.iter().filter(|task| board::matches(task, &self.query)).nth(row)
            })
            .map(|task| task.id)
    }

    /// Give the keyboard to the ticket's terminal, or take it back.
    ///
    /// One flag rather than a guess per frame, because the board has fields of its own — a title, a comment
    /// box — and a keystroke belongs to exactly one of them.
    pub fn focus_the_terminal(&mut self, focused: bool) {
        self.terminal_focused = focused;
    }

    /// Tick or untick one todo and read the ticket again.
    pub fn set_todo(&mut self, id: i64, done: bool) -> Result<(), String> {
        let now = clock::now();
        self.store()?.set_todo_done(id, done, &now)?;
        self.refresh()
    }

    /// Move a card to a lane and a place in it, from a drag or from the command line.
    ///
    /// One move does one more thing: **a card sent back from Agent Done to QA Failed resumes its session**,
    /// because a person rejecting finished work wants to tell the agent why, and the agent then decides
    /// whether to claim the ticket again — which is what moves it to In Progress. That is the board being
    /// replaced's own rule, and it is here rather than in the drag so that the command line does it too.
    pub fn move_card(&mut self, id: i64, status: Status, position: i64) -> Result<(), String> {
        let now = clock::now();
        let was = self.store()?.task(id)?;
        self.store()?.move_task(id, status, position, &now)?;
        self.refresh()?;
        let sent_back = was
            .as_ref()
            .is_some_and(|task| task.status == Status::AgentDone && status == Status::QaFailed);
        self.message.clear();
        if sent_back {
            if let Some(task) = was {
                // A resume that cannot happen — a Codex ticket, or one whose agent has never run — says so
                // in the status bar rather than failing the move. The move is what was asked for.
                match self.resume(&task.key) {
                    Ok(answer) => self.message = answer.message,
                    Err(problem) => self.message = problem,
                }
            }
        }
        Ok(())
    }

    /// The tickets one of the listing views shows, as they were last read.
    ///
    /// **Read on a refresh rather than while drawing.** Querying the database inside the draw is the one thing the
    /// design says the board never does, and it was doing it twice: once a frame for Backlog and once for
    /// Completed. They are read when the view is chosen and when a command changes something, which is when
    /// they can have changed.
    pub fn listing(&self, view: View) -> &[Task] {
        match view {
            View::Backlog => &self.backlog,
            View::Completed => &self.completed,
            _ => &[],
        }
    }

    /// How far the lanes have been scrolled sideways, taking this frame's scroll into account.
    ///
    /// The lanes are wider than a 420 point pane, so the board scrolls rather than squeezing a lane too
    /// thin for a card's title. Held on the provider rather than in egui's memory because the pane and the
    /// tab are two places drawing the same board and both should be where they were left.
    pub fn scroll_the_lanes(&mut self, ui: &egui::Ui, area: egui::Rect, most: f32) -> f32 {
        if most <= 0.0 {
            self.lane_scroll = 0.0;
            return 0.0;
        }
        let over = crate::components::controls::pointer_in(ui).is_some_and(|at| area.contains(at));
        if over {
            let wheel = ui.ctx().input(|input| {
                // A trackpad gives a sideways delta of its own; a wheel gives a vertical one.
                match input.smooth_scroll_delta.x.abs() > 0.5 {
                    true => input.smooth_scroll_delta.x,
                    // **A vertical wheel is left to the lane under the pointer.** Both this and
                    // `scroll_a_lane` used to read the same vertical delta, so one gesture over a long lane
                    // moved the cards down and slid the whole row of lanes sideways at the same time. A lane
                    // scrolls down; the board scrolls sideways; and the only gesture that means sideways is a
                    // sideways one. What is left for a board with no trackpad is the bar along the bottom and
                    // the arrow keys, both of which move it.
                    false => 0.0,
                }
            });
            self.lane_scroll -= wheel;
        }
        self.lane_scroll = self.lane_scroll.clamp(0.0, most);
        self.lane_scroll
    }

    /// Scroll the row of lanes to `offset` points from the left, kept between none and `most`, which is what
    /// dragging the bar along the bottom of the board does (`task-2281`).
    ///
    /// @param offset - how far the lanes are scrolled, in the board's points
    /// @param most - how far they can be scrolled
    pub fn put_the_lanes_at(&mut self, offset: f32, most: f32) {
        self.lane_scroll = offset.clamp(0.0, most.max(0.0));
    }

    /// Bring a lane into view sideways, which is what an arrow key crossing to an off screen lane needs.
    ///
    /// The board is a row of four lanes and a pane 420 points wide holds one and a half of them, so the ring
    /// moving right had to be able to move the board under it. Without this, the ring could only reach a lane
    /// that happened to be showing, and pressing right on the last visible one did nothing.
    pub fn show_the_lane(&mut self, lane: Status, lane_width: f32, gap: f32, room: f32, most: f32) {
        let index = Status::ALL.iter().position(|one| *one == lane).unwrap_or(0) as f32;
        let left = index * (lane_width + gap);
        let right = left + lane_width;
        // Scrolled only as far as it takes, and in whichever direction is needed: a lane off the left edge comes
        // to the left edge, one off the right comes to the right edge, and one already showing does not move.
        if left < self.lane_scroll {
            self.lane_scroll = left;
        } else if right > self.lane_scroll + room {
            self.lane_scroll = right - room;
        }
        self.lane_scroll = self.lane_scroll.clamp(0.0, most.max(0.0));
    }

    /// How many cards each lane holds after the search has narrowed it, which is what the ring moves over.
    ///
    /// All four lanes, not the ones on screen. Counting only what was drawn is what stopped the ring reaching a
    /// lane the board had scrolled past.
    pub fn lane_counts(&self) -> Vec<(Status, usize)> {
        Status::ALL
            .into_iter()
            .map(|status| {
                let held = self
                    .board
                    .lane(status)
                    .map(|lane| {
                        lane.tasks.iter().filter(|task| board::matches(task, &self.query)).count()
                    })
                    .unwrap_or(0);
                (status, held)
            })
            .collect()
    }

    /// How far one lane has been scrolled down, taking this frame's scroll into account.
    ///
    /// Per lane rather than one number for the board, because the lanes hold different numbers of cards and a
    /// shared number would scroll an empty lane as far as a full one. Held on the provider for the reason the
    /// sideways scroll is: the pane and the tab are two places drawing the same board.
    pub fn scroll_a_lane(
        &mut self,
        ui: &egui::Ui,
        lane: Status,
        area: egui::Rect,
        most: f32,
    ) -> f32 {
        let at = self.lane_downs.iter().position(|(known, _)| *known == lane);
        let mut down = at.map(|at| self.lane_downs[at].1).unwrap_or(0.0);
        if most <= 0.0 {
            down = 0.0;
        } else if crate::components::controls::pointer_in(ui).is_some_and(|at| area.contains(at)) {
            // A lane is a column of cards, so a wheel means further down it. The sideways scroll of the board
            // as a whole reads the horizontal delta, so the two do not fight over one gesture.
            down -= ui.ctx().input(|input| input.smooth_scroll_delta.y);
        }
        down = down.clamp(0.0, most);
        match at {
            Some(at) => self.lane_downs[at].1 = down,
            None => self.lane_downs.push((lane, down)),
        }
        down
    }

    /// How far the Backlog or Completed listing is scrolled down.
    ///
    /// One number for the two, because one of them is showing at a time.
    pub fn listing_scroll(&mut self, ui: &egui::Ui, area: egui::Rect, most: f32) -> f32 {
        if most <= 0.0 {
            self.listing_down = 0.0;
            return 0.0;
        }
        if crate::components::controls::pointer_in(ui).is_some_and(|at| area.contains(at)) {
            self.listing_down -= ui.ctx().input(|input| input.smooth_scroll_delta.y);
        }
        self.listing_down = self.listing_down.clamp(0.0, most);
        self.listing_down
    }

    /// Keep the point the pointer was over still through a zoom of the board. `task-1771`.
    ///
    /// The listings and the epics scroll on `listing_down`, which this board keeps itself, and every row in
    /// them is laid out at `Look::scale` - so the whole column's height is proportional to the zoom and the
    /// point at `offset + above` lands at `(offset + above) * ratio`. It is clamped on the next frame by
    /// [`Self::listing_scroll`], which knows how much there is to scroll and this does not.
    ///
    /// The lanes scroll sideways rather than down and are not corrected: a board's four lanes are as wide
    /// as the pane whatever the zoom, so there is no point under the pointer that moves away from it.
    pub fn zoom_the_listing(&mut self, ratio: f32, above: f32) {
        if !ratio.is_finite() || ratio <= 0.0 {
            return;
        }
        self.listing_down = ((self.listing_down + above) * ratio - above).max(0.0);
    }

    /// Pump every terminal, and say whether any of them moved.
    ///
    /// Called once a frame while the board is open and once per watchdog tick. Moved means printed something or
    /// still has a line waiting for its agent's prompt: those are the two reasons the window has to draw again
    /// on its own rather than waiting for the session's waker.
    pub fn pump(&mut self) -> bool {
        let now = clock::now();
        let mut moved = false;
        for terminal in &mut self.terminals {
            moved |= terminal.pump(&now);
        }
        moved |= self.follow_the_ticket_chat();
        moved | self.notice_another_window()
    }

    /// The agent that works `task`: its own assignee, or for a ticket assigned to a person the endpoint the
    /// board hands such a ticket to, which is what [`Self::start`] launches.
    pub fn agent_of(&self, task: &Task) -> Assignee {
        match task.assignee.is_an_agent() {
            true => task.assignee,
            false => self.default_assignee().0,
        }
    }

    /// The tab of the ticket modal's Agent section that shows for `task`.
    pub fn agent_tab_for(&self, task: &Task) -> ticket_chat::AgentTab {
        self.agent_tab.unwrap_or_else(|| ticket_chat::AgentTab::default_for(self.agent_of(task)))
    }

    /// The Agent-Chat plugin's own folder, beside this one, so a ticket's chat looks the way the side panel does.
    fn chat_folder(&self) -> Option<PathBuf> {
        self.folder
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(|plugins| plugins.join("agent-chat"))
            .filter(|folder| folder.is_dir())
    }

    /// A chat reading `task`'s transcript, or `None` when it has no session or its agent's transcript cannot be
    /// read.
    fn a_chat_for(&self, task: &Task) -> Option<ticket_chat::TicketChat> {
        let session = task.session_id.clone()?;
        let agent = self.agent_of(task);
        if !ticket_chat::chat_is_readable(agent) {
            return None;
        }
        let folder = self.working_folder(task).ok()?;
        Some(ticket_chat::TicketChat::new(
            task,
            agent,
            &session,
            &folder,
            self.chat_folder().as_deref(),
        ))
    }

    /// Keep the open ticket's chat in step with its agent's transcript, and type what its composer asked for into
    /// the agent's terminal (`task-2261`). Answers whether anything changed, so the window draws it.
    ///
    /// Words go through [`Self::send`], so a ticket whose agent has exited is resumed first, exactly as a comment
    /// sent to the agent is. A stop is `Escape`, which is Claude Code's own key for interrupting a turn: `Ctrl+C`
    /// twice in a row asks it to exit.
    fn follow_the_ticket_chat(&mut self) -> bool {
        let Some(task) = self.detail.task.clone() else {
            self.ticket_chat = None;
            return false;
        };
        let current = self.ticket_chat.as_ref().is_some_and(|chat| {
            chat.task_id == task.id && Some(chat.session.as_str()) == task.session_id.as_deref()
        });
        if !current {
            self.ticket_chat = self.a_chat_for(&task);
        }
        let Some(mut chat) = self.ticket_chat.take() else { return false };
        let running =
            self.terminal_for(task.id).is_some_and(|terminal| terminal.session.is_running());
        let now = std::time::Instant::now();
        let mut changed = chat.follow(running, now);
        for ask in chat.chat.take_what_the_mirror_asked() {
            changed = true;
            match ask {
                crate::services::agent_chat::MirrorAsk::Say(words) => {
                    match self.send(&task.key, &words) {
                        Ok(_) => chat.typed(now),
                        Err(problem) => {
                            chat.chat.problem = Some(problem.clone());
                            self.message = problem;
                        }
                    }
                }
                crate::services::agent_chat::MirrorAsk::Interrupt => {
                    match self.terminal_for_mut(task.id) {
                        Some(terminal) => terminal.session.send(vec![0x1b]),
                        None => self.message = format!("{} has no agent running to stop", task.key),
                    }
                }
            }
        }
        self.ticket_chat = Some(chat);
        changed
    }

    /// Show `key`'s conversation in the Agent-Chat side pane and close the ticket modal (`task-2269`).
    ///
    /// *"In agent task modal, in agent chat section, add a "To Side Pane" button ... when clicked, opens the agent
    /// chat for the task in the Agent Chat side pane and closes the modal. That way I can continue talking with
    /// the same agent/task, while files/etc are shown."* The side pane follows the same transcript the modal's
    /// Chat tab does and types into the same terminal through [`Self::send`], so it is the same agent and the
    /// same conversation, not a second agent resuming the session. Two commands go to the window: Agent-Chat's
    /// `ticket`, with everything it needs to find the transcript, and its `open-pane`.
    pub fn to_the_side_pane(&mut self, key: &str) -> Result<Answer, String> {
        let task = self.by_key(key)?;
        let agent = self.agent_of(&task);
        if !ticket_chat::chat_is_readable(agent) {
            return Err(ticket_chat::why_the_chat_is_empty(agent));
        }
        let session = task.session_id.clone().ok_or_else(|| {
            format!("{} has no conversation yet: `start` launches its agent", task.key)
        })?;
        let folder = self.working_folder(&task)?;
        let speaker = task.endpoint.clone().unwrap_or_else(|| agent.name().to_owned());
        let ticket = json!({
            "key": task.key,
            "task": task.id,
            "session": session,
            "folder": folder.display().to_string(),
            "speaker": speaker,
            "agent": agent.name(),
        });
        let run = |command: &str, words: Option<String>| {
            let mut arguments = serde_json::Map::new();
            arguments.insert("id".to_owned(), json!("agent-chat"));
            arguments.insert("command".to_owned(), json!(command));
            if let Some(words) = words {
                arguments.insert("arguments".to_owned(), json!(words));
            }
            Request::RunCommand {
                id: format!("to-side-pane-{command}"),
                command: "plugins.run".to_owned(),
                arguments,
            }
        };
        self.asking.push(run("ticket", Some(ticket.to_string())));
        self.asking.push(run("open-pane", None));
        self.modal_open = false;
        self.detail.is_new = false;
        Ok(Answer::said(format!("{}'s conversation is in the Agent-Chat pane", task.key))
            .with(json!({"task": task.key, "session": ticket["session"]})))
    }

    /// Press `Escape` in a ticket's agent, which is how Claude Code is told to stop the turn it is on. The
    /// side pane's Stop button, through `plugins run agent-tasks escape` (`task-2269`), and the same byte the
    /// modal's Chat tab sends.
    pub fn escape(&mut self, key: &str) -> Result<Answer, String> {
        let task = self.by_key(key)?;
        let terminal = self
            .terminal_for_mut(task.id)
            .ok_or_else(|| format!("{} has no agent running to stop", task.key))?;
        terminal.session.send(vec![0x1b]);
        Ok(Answer::said(format!("pressed Escape in {}", task.key)))
    }

    /// The last `count` messages of a ticket's conversation, read from its agent's transcript: the open ticket's
    /// chat when it is that one, and a fresh reading otherwise.
    pub fn chat_of(&self, task: &Task, count: usize) -> Result<serde_json::Value, String> {
        let agent = self.agent_of(task);
        if !ticket_chat::chat_is_readable(agent) {
            return Err(ticket_chat::why_the_chat_is_empty(agent));
        }
        if task.session_id.is_none() {
            return Err(format!(
                "{} has no conversation yet: `start` launches its agent",
                task.key
            ));
        }
        // The open ticket's chat while its first read is still on its thread holds nothing yet, so a fresh reading
        // answers instead.
        let open =
            self.ticket_chat.as_ref().filter(|chat| chat.task_id == task.id && !chat.is_reading());
        let mut fresh = None;
        let chat = match open {
            Some(chat) => chat,
            None => {
                let mut made = self.a_chat_for(task).ok_or_else(|| {
                    format!("{} has no folder to find its transcript in", task.key)
                })?;
                made.read_now();
                made.follow(false, std::time::Instant::now());
                &*fresh.insert(made)
            }
        };
        let conversation = &chat.chat.session().chat;
        Ok(json!({
            "task": task.key,
            "transcript": chat.path().map(|path| path.display().to_string()),
            "busy": chat.chat.session().is_busy(),
            "model": chat.model(),
            "messages": ticket_chat::messages_json(conversation, count),
            "total": conversation.messages.len(),
        }))
    }

    /// Read the board again when another window has written it.
    ///
    /// `task-2253`: *"when I add a new task, it's not showing up on the board."* Every window on a machine opens
    /// the same `board.rdb`, and a window read the board only when it changed something itself. Measured with
    /// two windows on one settings folder (`_agent_output/task-2253-board/two-windows.ps1`): a ticket made in
    /// one window was in the file and on that window's board, and the other window's board went on without it
    /// until something was changed there. So the board's files are looked at once every [`FILES_EVERY`], which is
    /// one listing of the plugin's folder, and the board is read again when they differ from what they were the
    /// last time it was read. True when it was read again, so the window draws it.
    fn notice_another_window(&mut self) -> bool {
        let Some(path) = self.store.as_ref().map(|store| store.path().to_path_buf()) else {
            return false;
        };
        let now = std::time::Instant::now();
        if self.files_asked.is_some_and(|asked| now.duration_since(asked) < FILES_EVERY) {
            return false;
        }
        self.files_asked = Some(now);
        let stamp = stamp_the_files(&path);
        if stamp.is_none() || stamp == self.files_read {
            return false;
        }
        match self.refresh() {
            Ok(()) => true,
            Err(problem) => {
                self.message = problem;
                self.files_read = stamp;
                true
            }
        }
    }

    /// How the terminals ask the window to draw again. Set once, when the provider is opened.
    pub fn set_waker(&mut self, waker: unluminous_terminal::Waker) {
        self.waker = Some(waker);
    }

    /// Every terminal this board has open.
    ///
    /// What a test waiting on an agent reads to tell a running agent from one that has exited, and to print the
    /// screen when it fails. The window itself always asks about one ticket, which is `terminal_for`.
    pub fn terminals(&self) -> &[TicketTerminal] {
        &self.terminals
    }

    pub fn terminal_for(&self, task_id: i64) -> Option<&TicketTerminal> {
        self.terminals.iter().find(|terminal| terminal.task_id == task_id)
    }

    pub fn terminal_for_mut(&mut self, task_id: i64) -> Option<&mut TicketTerminal> {
        self.terminals.iter_mut().find(|terminal| terminal.task_id == task_id)
    }

    /// What the watchdog sees of one ticket's terminal.
    pub fn terminal_state(&self, task_id: i64, now: &str) -> watchdog::Terminal {
        match self.terminal_for(task_id) {
            None => watchdog::Terminal::Gone,
            Some(terminal) if terminal.paused => watchdog::Terminal::Paused,
            Some(terminal) if !terminal.session.is_running() => watchdog::Terminal::Gone,
            Some(terminal) => watchdog::Terminal::Alive {
                silent_minutes: clock::minutes_between(&terminal.last_output_at, now),
            },
        }
    }

    /// One watchdog tick. Called every two minutes by the window, and by a test with a fixed instant.
    ///
    /// The decisions come from `watchdog::decide`, which has no clock and no database, so this function
    /// is only the two things that need one: reading the candidates, and acting on what was decided.
    pub fn watchdog_tick(
        &mut self,
        now: &str,
    ) -> Result<Vec<(String, watchdog::Decision)>, String> {
        let thresholds = watchdog::Thresholds {
            lease_minutes: self.configuration.lease_minutes,
            ..watchdog::Thresholds::default()
        };
        let candidates = self.store()?.watchdog_candidates(now, thresholds.lease_minutes)?;
        let mut acted = Vec::new();
        for card in candidates {
            // A card another window owns is that window's business. Without this, a window with no terminal
            // for a card reads it as a card whose worker is gone, and once the lease expires it strikes and
            // reclaims work that is still running somewhere else. A card whose owning window has gone is
            // nobody's, so it is picked up here.
            let owned_elsewhere = match self.store()?.owner_of(card.id)? {
                Some(recorded) => recorded != owner() && !owner_is_gone(Some(&recorded)),
                None => false,
            };
            if owned_elsewhere {
                continue;
            }
            let terminal = self.terminal_state(card.id, now);
            let decision = watchdog::decide(&card, terminal, thresholds, false);
            match &decision {
                watchdog::Decision::Leave => continue,
                watchdog::Decision::ClearCounters => {
                    self.store()?.heartbeat(card.id, None, now)?;
                }
                watchdog::Decision::Strike { first } => {
                    let strikes = self.store()?.strike(card.id)?;
                    if *first {
                        let said = watchdog::strike_comment(
                            &card.key,
                            strikes,
                            thresholds.strikes_before_reclaim,
                        );
                        self.store()?.add_comment(card.id, Author::System, &said, now)?;
                    }
                }
                watchdog::Decision::Reclaim => {
                    let said = watchdog::reclaim_comment(&card.key);
                    self.store()?.add_comment(card.id, Author::System, &said, now)?;
                    self.store()?.reclaim(card.id, now)?;
                    self.terminals.retain(|terminal| terminal.task_id != card.id);
                }
                watchdog::Decision::Nudge { instruction, .. } => {
                    let line = instruction.clone();
                    self.store()?.nudge(card.id, now)?;
                    if let Some(terminal) = self.terminal_for_mut(card.id) {
                        terminal.type_line(&line);
                    }
                }
            }
            acted.push((card.key.clone(), decision));
        }
        if !acted.is_empty() {
            self.refresh()?;
        }
        Ok(acted)
    }
}

/// One field of a ticket, as the modal's controls report it.
///
/// A value per control rather than a `TaskEdit` built by each of them, so the checking — that an assignee is
/// one of three, that an effort is one of five, that an epic is a number — happens in one place and says the
/// same thing however it was asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    Assignee(String),
    Priority(String),
    Model(String),
    Effort(String),
    Project(String),
    Epic(String),
}

/// The effort levels a ticket may name.
///
/// Claude's five. Codex knows nothing above `high` and `agent::codex_effort` is what collapses the top two,
/// so the list here is the wider of the two and the narrowing happens where the command line is built.
pub const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// How many lines of a terminal are read to tell whether it printed anything.
///
/// One screenful. Reading the whole scrollback once a frame per ticket is what this number exists to avoid:
/// on a busy agent that is megabytes rebuilt sixty times a second, and the question is only whether anything
/// moved.
const TAIL_LINES: usize = 60;

/// Which window owns a claim: this process.
///
/// Two Unluminous windows can have one board file open, and each runs its own watchdog over the same rows. A
/// window that has no terminal for a card would otherwise read it as a card whose worker is gone, and after
/// the lease expired it would strike and reclaim work still running in the other window. The process id is
/// enough to tell the two apart, and [`owner_is_gone`] is what asks whether the window that took a claim is
/// still running.
fn owner() -> String {
    format!("pid:{}", std::process::id())
}

/// True when the window that took this claim is no longer running.
///
/// A card owned by a window that has gone is a card whose worker is gone, whoever asks. Whether a
/// process with this id is still there is a question the operating system answers with no signal and
/// no side effect on either platform. An owner this function cannot read at all is treated as gone,
/// because a card nobody can account for should be recoverable rather than stuck.
fn owner_is_gone(recorded: Option<&str>) -> bool {
    let Some(pid) = recorded.and_then(|owner| owner.strip_prefix("pid:")) else {
        return true;
    };
    let Ok(pid) = pid.parse::<i32>() else {
        return true;
    };
    if pid == std::process::id() as i32 {
        return false;
    }
    // The one answer to "is that process still there", which lives beside the instance files that
    // ask it about a window. There were two copies of this — one here and one written in
    // `unluminous-cli` for `task-1805`, where a stale instance file was costing 414 ms of every
    // startup — and two answers to one question would be one too many.
    !unluminous_cli::instances::is_running(pid as u32)
}

/// A conversation id: a hyphenated hexadecimal string of the shape both agents accept.
///
/// Built from the clock and the address of a local value rather than from a random number generator,
/// because Unluminous has none and adding one to name a session would be a dependency for a string. Two ids
/// made in the same nanosecond by the same process would collide; the claim is a guarded update, so a
/// collision is refused rather than silently sharing a conversation.
/// A fresh conversation id.
///
/// **`pub` since `task-1906`, because a terminal node on the canvas needs one too** — a node running
/// `claude` is given `--session-id <uuid>` on its first run and `--resume <uuid>` after a restart, which is
/// exactly what a ticket's agent is given. One generator rather than a second copy of the same arithmetic.
pub fn new_session_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let here = &nanos as *const u128 as usize;
    // **A counter, so two ids made in one process are never the same one.** The clock is the only thing that
    // varies between two calls, and a nanosecond is not long enough to rely on: `task-1906` gives every
    // terminal node on a canvas an id, and bringing a view to life asks for them in a loop. A ticket's agent
    // was protected by the guarded claim that writes it down — a collision there is refused — and a canvas
    // node has no such guard, so it would have shared a conversation in silence. The Codex Sol review found
    // it. One counter here answers it for both callers rather than a second guard beside the first.
    static MADE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let count = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        (nanos >> 64) as u32 ^ here as u32,
        (nanos >> 48) as u16,
        (nanos >> 32) as u16 & 0x0fff,
        (nanos >> 16) as u16 & 0x0fff,
        (nanos as u64 ^ count.rotate_left(24)) & 0xffff_ffff_ffff
    )
}

impl AgentTasks {
    /// The request the description's microphone makes: `voice dictate --into description`, on or off.
    /// @param on - whether to start dictating
    pub fn dictation_request(on: bool) -> Request {
        let mut arguments = serde_json::Map::new();
        arguments.insert("state".to_owned(), serde_json::json!(if on { "on" } else { "off" }));
        arguments.insert("into".to_owned(), serde_json::json!("description"));
        Request::RunCommand {
            id: "agent-tasks-voice".to_owned(),
            command: "voice.dictate".to_owned(),
            arguments,
        }
    }

    /// Words dictation has for the open ticket's description, written onto the ticket as they arrive, which is
    /// what typing into it does.
    /// @param note - what the window heard
    fn hear(&mut self, note: &crate::services::plugin_ui::Dictation) {
        use crate::services::plugin_ui::Dictation;
        let words = match note {
            Dictation::Began => {
                self.dictating = Some(crate::services::agent_chat::Dictating::starting(
                    &self.description_text(),
                ));
                return;
            }
            Dictation::Words(words) => words,
            Dictation::Ended { words, .. } => words,
        };
        let current = self.description_text();
        let next = match self.dictating.as_mut() {
            Some(dictating) => dictating.put(&current, words),
            None => return,
        };
        if matches!(note, Dictation::Ended { .. }) {
            self.dictating = None;
        }
        if next != current {
            if let Err(problem) = self.save_the_description(&next) {
                self.message = problem;
            }
        }
    }
}

impl UiProvider for AgentTasks {
    fn id(&self) -> &'static str {
        "agent-tasks"
    }

    fn open(&mut self, context: &Context) -> Result<(), String> {
        self.project = context.project.clone();
        self.recent_projects = context.recent_projects.clone();
        self.endpoints = context.endpoints.clone();
        // How a terminal asks for a frame. Without one, a terminal that printed while nobody was pointing at
        // the window would wait for the pointer to move, and asking for a frame on a timer instead would keep
        // the graphics card busy while an agent sits at its prompt.
        if let Some(wake) = context.wake.clone() {
            self.waker = Some(wake);
        }
        // **No folder means a board in memory**, and that is what stops a test reading or writing the
        // board somebody is using. The released binary always has one, because `UnluminousApp::load_settings`
        // is what hands it over; a window a test builds has none, exactly as it has no settings file and
        // no `.unluminous` folder. The rule is `UnluminousApp::load_settings`' own, kept once more.
        match context.folder.clone() {
            Some(folder) => {
                self.configuration = Configuration::read(&folder);
                let path = self
                    .configuration
                    .database_path(Some(&folder))
                    .expect("a folder was named, so there is a path");
                self.folder = Some(folder);
                self.store = Some(Store::open(path)?);
            }
            None => {
                self.configuration = Configuration::default();
                self.folder = None;
                self.store = Some(Store::in_memory()?);
            }
        }
        self.refresh()
    }

    /// The board draws the decoration `egui` cannot: soft shadows, inset shadows, gradients and a rounded
    /// clip. `task-1765` is why, and `tasks/agent-tasks-vello-ui-tdd.md` is the design.
    fn zoomed(&mut self, ratio: f32, above: f32) {
        self.zoom_the_listing(ratio, above);
    }

    /// The endpoints changed in `Settings -> AI Configuration`. A ticket keeps the name of the one it was
    /// given to, so a renamed endpoint leaves it launching the first one of its kind.
    fn endpoints(&mut self, endpoints: &[unluminous_chat::Provider]) {
        self.endpoints = endpoints.to_vec();
    }

    fn draws_chrome(&self) -> bool {
        true
    }
    fn is_open(&self) -> bool {
        self.store.is_some()
    }

    fn pane(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::agent_tasks::pane(self, ui, look)
    }

    fn tab(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::agent_tasks::tab(self, ui, look)
    }

    fn settings(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::agent_tasks::settings(self, ui, look)
    }

    fn command(&mut self, command: &str, arguments: &[String]) -> Result<Answer, String> {
        commands::run(self, command, arguments)
    }

    fn commands(&self) -> Vec<(&'static str, &'static str)> {
        commands::LIST.to_vec()
    }

    /// How many tickets are on the board, which is what its pane header draws beside its name.
    ///
    /// `task-1984` A7: the header used to ask `view` and read `total` out of it, which for this
    /// provider is every card on the board serialised to JSON, twice a second at idle.
    fn badge(&self) -> Option<String> {
        Some(self.board.total().to_string())
    }

    fn view(&self) -> serde_json::Value {
        json!({
            "open": self.is_open(),
            "database": self.configuration.database_said(self.folder.as_deref()),
            "view": self.view.name(),
            "modal": self.modal_open,
            // Which way the open ticket is being read, so an agent can see what a person is looking at rather
            // than only being able to change it. `task-28`.
            "showing": {
                "description": match self.detail.description_rendered {
                    true => "markdown",
                    false => "raw",
                },
                "comments_as_source": self
                    .detail
                    .comments_raw
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<i64>>(),
            },
            "message": self.message,
            "sprint": self.board.sprint.as_ref().map(|sprint| json!({
                "name": sprint.name,
                "status": sprint.status.name(),
            })),
            "total": self.board.total(),
            "lanes": self
                .board
                .lanes
                .iter()
                .map(|lane| json!({
                    "status": lane.status.name(),
                    "label": lane.status.label(),
                    "count": lane.count(),
                    "cards": lane.tasks.iter().map(card_json).collect::<Vec<serde_json::Value>>(),
                }))
                .collect::<Vec<serde_json::Value>>(),
            "epics": self
                .board
                .epics
                .iter()
                .map(|epic| json!({"name": epic.name, "color": epic.color}))
                .collect::<Vec<serde_json::Value>>(),
            "detail": self.detail.task.as_ref().map(|_| self.detail_json()),
            "terminals": self
                .terminals
                .iter()
                .map(|terminal| json!({
                    "task": terminal.task_id,
                    "session": terminal.session_id,
                    "alive": terminal.session.is_running(),
                    "paused": terminal.paused,
                }))
                .collect::<Vec<serde_json::Value>>(),
        })
    }

    fn modal(&mut self, ctx: &egui::Context, look: &Look<'_>) -> (Vec<Request>, bool) {
        // **Cleared before anything else, on every frame.** A `ShapeIdx` belongs to one frame's shape list
        // and means nothing on the next, so a slot left behind by a modal that has since closed is a slot
        // the window would paint into on some later frame. It is filled in again below when a modal really
        // is drawn.
        self.modal_canvas = None;
        // The supervisor's modal, when the ticket modal is not in front of it: one modal at a time.
        if self.supervisor_open && !(self.modal_open && self.detail.task.is_some()) {
            let outcome = crate::components::agent_tasks::supervisor_modal::show(self, ctx, look);
            if outcome.closed {
                self.supervisor_open = false;
            }
            return (outcome.requests, outcome.closed);
        }
        if !self.modal_open || self.detail.task.is_none() {
            return (Vec::new(), false);
        }
        let outcome = crate::components::agent_tasks::ticket_modal::show(self, ctx, look);
        if outcome.closed {
            self.modal_open = false;
            self.close_detail();
        }
        (outcome.requests, outcome.closed)
    }

    fn take_the_modals_canvas(&mut self) -> Option<crate::services::plugin_ui::ChromeSlot> {
        self.modal_canvas.take()
    }

    /// The ticket and the supervisor step aside for a browser tab a link in them opened (`task-2263`). A new
    /// ticket that has a title is kept, as closing it any other way keeps it.
    fn put_the_modal_away(&mut self) {
        if self.modal_open && self.detail.is_new && self.detail.title_draft.trim().is_empty() {
            return;
        }
        self.modal_open = false;
        self.detail.is_new = false;
        self.supervisor_open = false;
    }

    fn voice(&mut self, view: &crate::services::plugin_ui::VoiceView) {
        self.voice = view.clone();
    }

    fn dictation(&mut self, note: &crate::services::plugin_ui::Dictation) {
        self.hear(note);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn keyboard(&mut self, has_it: bool) {
        // **Losing the keys clears the terminal's focus, and gaining them does not set it.** The board asks for the
        // keyboard for two different reasons — somebody clicked into a ticket's terminal, and somebody clicked the
        // lanes so the arrow keys reach them — and this cannot tell which. So the terminal's own click is what
        // sets its focus, in `components::agent_tasks::detail`, and what is left here is the half that is true
        // whatever the reason: a board that has lost the keyboard has no terminal typing into it.
        if !has_it {
            self.terminal_focused = false;
        }
    }

    fn asking(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.asking)
    }

    fn catch_up(&mut self) -> bool {
        // Answers whether something is still **moving**, not whether something is running. A terminal that is
        // alive and quiet needs no frame: its own waker asks for one when it prints. Answering yes for a live
        // terminal kept the window drawing for ever while an agent sat at its prompt.
        self.pump()
    }

    fn close(&mut self) {
        // The sessions go with the provider, and each one's agent keeps its conversation, so closing the
        // board loses no work: `Resume session` brings any of them back on the id in the database.
        self.terminals.clear();
        self.ticket_chat = None;
        self.store = None;
        self.board = Board::default();
        self.detail = Detail::default();
        self.results.clear();
        self.backlog.clear();
        self.completed.clear();
    }
}

impl AgentTasks {
    /// One ticket in full, read from the store and changing nothing about what the board is showing.
    ///
    /// The same shape [`Self::detail_json`] answers with, which is what lets `task` and `open` be told
    /// apart by what they *do* rather than by what they say. See the `task` command.
    fn read_a_ticket(&self, id: i64) -> Result<serde_json::Value, String> {
        let store = self.store()?;
        let task = store.task(id)?;
        let (todos, comments) = match task.as_ref() {
            Some(task) => (store.todos(task.id)?, store.comments(task.id)?),
            None => (Vec::new(), Vec::new()),
        };
        Ok(json!({
            "task": task.as_ref().map(card_json),
            "description": task.as_ref().map(|task| task.description.clone()),
            "todos": todos
                .iter()
                .map(|todo| json!({"text": todo.text, "done": todo.done}))
                .collect::<Vec<serde_json::Value>>(),
            "comments": comments
                .iter()
                .map(|comment| json!({
                    "id": comment.id,
                    "author": comment.author.name(),
                    "body": comment.body,
                    "at": comment.created_at,
                }))
                .collect::<Vec<serde_json::Value>>(),
        }))
    }

    fn detail_json(&self) -> serde_json::Value {
        json!({
            "agentTab": self.detail.task.as_ref().map(|task| self.agent_tab_for(task).name()),
            "isNew": self.detail.is_new,
            "choice": self.detail.task.as_ref().and_then(|task| self.choice_for(task.id)).map(|choice| json!({
                "question": choice.question,
                "options": choice.options,
                "cursor": choice.cursor + 1,
            })),
            "titleMissing": self.detail.title_missing,
            "fieldsShut": self.fields_shut,
            "descriptionShut": self.description_shut,
            "commentsShut": self.comments_shut,
            "chat": self.ticket_chat.as_ref().map(|chat| json!({
                "reading": chat.is_reading(),
                "transcript": chat.path().map(|path| path.display().to_string()),
                "messages": chat.chat.session().chat.messages.len(),
                "queued": chat.chat.queued().len(),
                "busy": chat.chat.session().is_busy(),
            })),
            "task": self.detail.task.as_ref().map(card_json),
            "description": self.detail.task.as_ref().map(|task| task.description.clone()),
            "todos": self
                .detail
                .todos
                .iter()
                .map(|todo| json!({"text": todo.text, "done": todo.done}))
                .collect::<Vec<serde_json::Value>>(),
            "comments": self
                .detail
                .comments
                .iter()
                .map(|comment| json!({
                    // The id, because `comment-edit` names a comment by it and there was no way to learn one.
                    "id": comment.id,
                    "author": comment.author.name(),
                    "body": comment.body,
                    "at": comment.created_at,
                }))
                .collect::<Vec<serde_json::Value>>(),
        })
    }
}

fn card_json(task: &Task) -> serde_json::Value {
    json!({
        "key": task.key,
        "id": task.id,
        "title": task.display_title(),
        "status": task.status.name(),
        "priority": task.priority.name(),
        "assignee": AgentTasks::assignee_said(task),
        "kind": task.assignee.name(),
        "model": task.model,
        "effort": task.effort,
        "todos": format!("{}/{}", task.todo_done_count, task.todo_count),
        "comments": task.comment_count,
        "session": task.session_id,
        "strikes": task.watchdog_strikes,
        "nudges": task.watchdog_nudges,
    })
}

#[cfg(test)]
mod tests_task_28 {
    use super::*;

    /// Two conversation ids made one after another are never the same id.
    ///
    /// The clock is the only thing that varies between two calls, and a nanosecond is not long enough to rely
    /// on. `task-1906` gives every terminal node on a canvas an id and asks for them in a loop while bringing a
    /// view to life, so two nodes would have shared one conversation in silence — a ticket's agent was
    /// protected by the guarded claim that writes its id down, and a canvas node has no such guard. Found by
    /// the Codex Sol review.
    #[test]
    fn two_conversation_ids_made_in_a_row_are_different() {
        let many: Vec<String> = (0..2000).map(|_| new_session_id()).collect();
        let distinct: std::collections::HashSet<&String> = many.iter().collect();
        assert_eq!(
            distinct.len(),
            many.len(),
            "two of {} ids made in a row are the same conversation",
            many.len(),
        );
        // And they are still the shape both agents accept, which is what the arithmetic is for.
        for id in many.iter().take(5) {
            let parts: Vec<&str> = id.split('-').collect();
            assert_eq!(parts.len(), 5, "{id}");
            assert_eq!(
                parts.iter().map(|part| part.len()).collect::<Vec<_>>(),
                vec![8, 4, 4, 4, 12],
                "{id}",
            );
            assert!(id.chars().all(|at| at.is_ascii_hexdigit() || at == '-'), "{id}");
        }
    }

    /// `task-28`: "We don't need a Schedule field."
    ///
    /// What there was, was `View::Schedule`, the fifth board view, listing the rows in the `task_schedule`
    /// table with when each next runs. Nothing on this board ever writes such a row, so it was a view of a
    /// table that is always empty.
    #[test]
    fn the_board_has_four_views_and_schedule_is_not_one_of_them() {
        assert_eq!(View::ALL.len(), 4);
        let names: Vec<&str> = View::ALL.iter().map(|view| view.name()).collect();
        assert_eq!(names, ["board", "backlog", "completed", "epics"]);
        assert!(View::parse("schedule").is_none(), "there is no schedule view to ask for");
        for name in names {
            assert!(View::parse(name).is_some(), "{name} is asked for by its own name");
        }
    }

    /// `task-28`: "Prefill my settings with the base url for Iliad."
    ///
    /// A configuration that has never been written points at Iliad, and so does one read from a settings file
    /// `task-2247`: the Iliad gateway and key are gone. A settings file written before keeps every other setting
    /// it had, and once written again names no gateway at all.
    #[test]
    fn a_settings_file_naming_the_old_gateway_is_read_and_written_back_without_it() {
        let folder = std::env::temp_dir()
            .join(format!("unluminous-board-oldgateway-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder");
        std::fs::write(
            folder.join("settings.conf"),
            "agent = codex\nkey-name = something\nbase-url = https://gateway\nlease = 30\n",
        )
        .expect("a file from the version before this one");
        let read = Configuration::read(&folder);
        assert_eq!(read.agent, Assignee::Codex, "the settings it does know are still read");
        assert_eq!(read.lease_minutes, 30);
        read.write(&folder).expect("written back");
        let text = std::fs::read_to_string(folder.join("settings.conf")).expect("the file");
        for gone in ["base-url", "key-name", "gateway"] {
            assert!(!text.contains(gone), "{gone} is not a setting any more: {text}");
        }
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// `task-2247`: a ticket is given to an endpoint by name, and the endpoint decides the kind of agent, the
    /// program and the model.
    #[test]
    fn a_ticket_is_given_to_an_endpoint_and_launched_with_its_program_and_model() {
        let mut board = AgentTasks::new();
        let mut local = unluminous_chat::Provider::defaults()[3].clone();
        local.name = "local-agent".to_owned();
        local.model = "llama-local/local".to_owned();
        board.endpoints = vec![unluminous_chat::Provider::defaults()[0].clone(), local];
        assert_eq!(board.assignee_choices(), ["claude", "local-agent", "human"]);
        assert_eq!(
            board.assignee_named("local-agent").expect("an endpoint"),
            (Assignee::OpenCode, Some("local-agent".to_owned()))
        );
        assert_eq!(board.assignee_named("human").expect("a person"), (Assignee::Human, None));
        let refused = board.assignee_named("codex").expect_err("not one of these endpoints");
        assert!(refused.contains("claude, local-agent, human"), "{refused}");

        let mut task = Task::for_tests("task-1");
        task.assignee = Assignee::OpenCode;
        task.endpoint = Some("local-agent".to_owned());
        let plan =
            board.plan_for(&task, Assignee::OpenCode, Some("local-agent"), "s".to_owned(), false);
        let launched = agent::launch(&plan).expect("launched");
        assert_eq!(launched.program, "opencode");
        assert!(
            launched.arguments.windows(2).any(|pair| pair == ["--model", "llama-local/local"]),
            "{launched:?}"
        );
        // A model the ticket names wins over the endpoint's.
        task.model = Some("llama-local/qwen-27b".to_owned());
        let plan =
            board.plan_for(&task, Assignee::OpenCode, Some("local-agent"), "s".to_owned(), false);
        assert!(agent::launch(&plan)
            .expect("launched")
            .arguments
            .contains(&"llama-local/qwen-27b".to_owned()));
    }

    /// `task-2293`: the description's microphone. Words dictated go after what the description already says,
    /// each pass replacing the last one's provisional words, and are written onto the ticket as they arrive.
    #[test]
    fn dictated_words_fill_the_open_tickets_description() {
        use crate::services::plugin_ui::{Context, Dictation, UiProvider};
        let mut board = AgentTasks::new();
        board.open(&Context::default()).expect("a board in memory");
        board.command_now("new-task", &["A".to_owned(), "ticket".to_owned()]).expect("made");
        board.command_now("open", &["task-1".to_owned()]).expect("opened");
        board.save_the_description("Fix the parser.").expect("described");
        board.dictation(&Dictation::Began);
        board.dictation(&Dictation::Words("also the".to_owned()));
        assert_eq!(board.description_text(), "Fix the parser. also the");
        board.dictation(&Dictation::Ended { words: "also the lexer".to_owned(), send: false });
        assert_eq!(board.description_text(), "Fix the parser. also the lexer");
        assert!(board.dictating.is_none(), "the dictation is over");
        // Words after the end are not for this description.
        board.dictation(&Dictation::Words("stray".to_owned()));
        assert_eq!(board.description_text(), "Fix the parser. also the lexer");
    }

    /// `task-28`: the `Project` field was free text. The dropdown offers this window's folder and the recent
    /// ones, handed over in `plugin_ui::Context`, and keeps whatever the ticket already says.
    #[test]
    fn the_project_dropdown_offers_this_window_and_the_recent_ones() {
        use crate::services::plugin_ui::{Context, UiProvider};
        let mut board = AgentTasks::new();
        board
            .open(&Context {
                project: Some(PathBuf::from("/here/now")),
                showing: None,
                recent_projects: vec![PathBuf::from("/older"), PathBuf::from("/here/now")],
                folder: None,
                wake: None,
                endpoints: Vec::new(),
            })
            .expect("a board with no settings folder opens in memory");

        // This window first, then the recent ones, and a folder listed twice is listed once.
        assert_eq!(board.known_projects(None), vec!["/here/now".to_owned(), "/older".to_owned()]);

        // A ticket naming a folder this window has never opened keeps it, and it is the first choice, for the
        // reason `agent::models_for` keeps a model it does not know.
        assert_eq!(
            board.known_projects(Some("/somewhere/else")),
            vec!["/somewhere/else".to_owned(), "/here/now".to_owned(), "/older".to_owned()]
        );
        // And one it has opened is not listed twice.
        assert_eq!(
            board.known_projects(Some("/older")),
            vec!["/older".to_owned(), "/here/now".to_owned()]
        );
    }

    /// And the table it read is still there, because dropping a table is deleting data.
    #[test]
    fn the_schedule_table_is_still_read_even_though_no_view_draws_it() {
        let store = Store::in_memory().expect("a board in memory");
        assert!(store.schedules().expect("the schedules").is_empty(), "empty, which is the point");
    }

    /// The rule `agent_chat` and `database` already keep for their own dispatch, kept here for the
    /// first time: one listed with no arm behind it is a command `plugins show` offers and
    /// `plugins run` refuses, and nothing before this caught that. `commands::run`'s 670 line match
    /// had no test that drove it directly; this is that test.
    ///
    /// A verb that needs an argument this call does not give it is still an answer — `there is no
    /// ticket ''` is a real refusal from a real handler. What this catches is the other kind: a verb
    /// that reaches none of the seven groups in `commands.rs` and falls all the way through to
    /// [`UiProvider::refuse`], which is the one string this test can name in advance and check for.
    #[test]
    fn every_command_it_lists_is_a_command_it_answers() {
        use crate::services::plugin_ui::{Context, UiProvider};
        let mut board = AgentTasks::new();
        board.open(&Context::default()).expect("a board with no settings folder opens in memory");
        for (name, summary) in commands::LIST {
            assert!(!summary.is_empty(), "{name} says nothing about itself");
            let answered = board.command(name, &[]);
            if let Err(problem) = answered {
                let unknown = board.refuse(name);
                assert_ne!(&problem, &unknown, "{name} is listed and not answered: {problem}");
            }
        }
        // And a verb genuinely not on the board is refused with the list, which is what the test
        // above would have to tell apart from a real answer.
        let refused = board.command("levitate", &[]).expect_err("refused");
        assert!(refused.contains("board"), "the refusal lists the commands: {refused}");
    }
}

#[cfg(test)]
mod tests_task_2285;
