//! What Agent-Tasks does differently when its board is agent-tasks-service's (`board-source = service`).
//!
//! `tasks/agent-tasks-through-the-service-tdd.md` is the design. The service runs the agents and the watchdog, so
//! Start, Resume, Send, Escape and Pause are requests to it, and the ticket's terminal in the modal is a view of
//! the service's terminal through [`super::board_terminal`].

use std::path::{Path, PathBuf};

use serde_json::json;

use super::board_store::BoardStore;
use super::configuration::BoardSource;
use super::model::Task;
use super::service_board::ServiceBoard;
use super::store::Store;
use super::{board_terminal, clock, AgentTasks, TicketTerminal, FILES_EVERY, SUPERVISOR};
use crate::services::pairing;
use crate::services::plugin_ui::Answer;

/// The Unluminous settings folder a plugin folder is in: `<settings>/plugins/agent-tasks` is two levels down.
/// @param plugin_folder - the plugin's own folder
fn settings_folder_of(plugin_folder: &Path) -> Option<PathBuf> {
    plugin_folder.parent()?.parent().map(Path::to_path_buf)
}

/// Open the board the configuration names: the plugin's file, or the service found through `boards/<port>.conf`.
///
/// A window with no settings folder, which is a test's window, has its file board in memory, and finds the
/// service through `conf` or `UNLUMINOUS_BOARD_CONF`.
/// @param configuration - the plugin's settings, which say which board
/// @param plugin_folder - the plugin's own folder, or `None` for a window with no settings folder
/// @param conf - one `boards/<port>.conf` to use instead of looking in the settings folder
/// @param wake - asks the window to draw again when the service says the board changed
pub fn open_a_board(configuration: &super::Configuration, plugin_folder: Option<&Path>, conf: Option<&Path>, wake: Option<unluminous_terminal::Waker>) -> Result<BoardStore, String> {
    match (configuration.board_source, plugin_folder) {
        (BoardSource::File, Some(folder)) => {
            let path = configuration.database_path(Some(folder)).expect("a folder was named, so there is a path");
            Ok(BoardStore::File(Store::open(path)?))
        }
        (BoardSource::File, None) => Ok(BoardStore::File(Store::in_memory()?)),
        (BoardSource::Service, folder) => {
            let mut source = pairing::Source::for_the_settings_folder(folder.and_then(settings_folder_of).as_deref());
            if let Some(conf) = conf {
                source.file = Some(conf.to_path_buf());
            }
            let found = pairing::find_board(&source).map_err(|why| format!("Agent-Tasks is set to read agent-tasks-service, and it could not be reached. {why}"))?;
            Ok(BoardStore::Service(ServiceBoard::connect(found.port, &found.token, wake)?))
        }
    }
}

impl AgentTasks {
    /// Open the board this provider's configuration names.
    pub(super) fn open_the_board(&self) -> Result<BoardStore, String> {
        open_a_board(&self.configuration, self.folder.as_deref(), self.board_conf.as_deref(), self.waker.clone())
    }

    /// Find agent-tasks-service through this one conf file rather than the settings folder, which is how a test
    /// points a window with no settings folder at a service it started.
    /// @param conf - a `boards/<port>.conf`
    pub fn find_the_service_through(&mut self, conf: PathBuf) {
        self.board_conf = Some(conf);
    }

    /// The service's board, when that is the one open.
    pub fn service_board(&self) -> Option<&ServiceBoard> {
        self.store.as_ref().and_then(BoardStore::service)
    }

    /// Where the board comes from now.
    pub fn board_source(&self) -> BoardSource {
        self.configuration.board_source
    }

    /// Once a second, read the service's board again when it said something changed. Answers whether the board
    /// was read, so the window draws it.
    pub(super) fn notice_the_service(&mut self) -> bool {
        let now = std::time::Instant::now();
        if self.files_asked.is_some_and(|asked| now.duration_since(asked) < FILES_EVERY) {
            return false;
        }
        self.files_asked = Some(now);
        self.follow_the_configuration_file();
        let Some(board) = self.service_board() else { return true };
        if !board.has_changed() {
            return false;
        }
        if let Err(problem) = board.read_again().and_then(|()| self.refresh()) {
            self.message = problem;
        }
        true
    }

    /// Choose where the board comes from, write it to `settings.conf`, and open that board now.
    ///
    /// Refused while an agent runs in this window, because a ticket id on one board names a different ticket on
    /// the other, and the terminals are kept by ticket id.
    /// @param source - the board to read
    pub fn set_board_source(&mut self, source: BoardSource) -> Result<Answer, String> {
        if source == self.configuration.board_source {
            return Ok(Answer::said(format!("the board is already {}", source.label())));
        }
        if self.terminals.iter().any(|terminal| terminal.session.is_running() && !terminal.is_a_view) {
            return Err("an agent is running in this window. Stop it before choosing another board, because a ticket's id names a different ticket on the other board".to_owned());
        }
        let mut changed = self.configuration.clone();
        changed.board_source = source;
        self.change_the_configuration(changed)?;
        self.reopen_the_board()?;
        Ok(Answer::said(format!("Agent-Tasks now reads {}", source.label())).with(json!({"boardSource": source.name(), "board": self.board_where()})))
    }

    /// Open the board the configuration now names, dropping the terminals and the open ticket of the old one.
    pub(super) fn reopen_the_board(&mut self) -> Result<(), String> {
        self.terminals.retain(|terminal| terminal.task_id == SUPERVISOR);
        self.ticket_chat = None;
        self.close_detail();
        self.modal_open = false;
        self.store = None;
        self.store = Some(self.open_the_board()?);
        self.refresh()
    }

    /// Start or resume a ticket's agent on the service, which reattaches a live session rather than starting a
    /// second one, and show its terminal.
    /// @param task - the ticket
    pub(super) fn start_on_the_service(&mut self, task: &Task) -> Result<Answer, String> {
        let board = self.service_board().ok_or("the board is not agent-tasks-service's")?;
        let state = board.client().start_work(task.id).map_err(|problem| format!("{} could not be started on agent-tasks-service: {problem}", task.key))?;
        board.read_again()?;
        self.refresh()?;
        self.show_the_service_terminal(task.id)?;
        self.message = format!("{} is running on agent-tasks-service", task.key);
        Ok(Answer::said(self.message.clone()).with(json!({"task": task.key, "session": state.session_id, "alive": state.alive, "board": "service"})))
    }

    /// Type a line into a ticket's agent on the service, which starts a stopped agent first.
    /// @param task - the ticket
    /// @param line - what to type
    pub(super) fn send_on_the_service(&mut self, task: &Task, line: &str) -> Result<Answer, String> {
        let board = self.service_board().ok_or("the board is not agent-tasks-service's")?;
        board.client().send_terminal_message(task.id, line).map_err(|problem| format!("the line could not be sent to {}: {problem}", task.key))?;
        self.message = format!("sent to {}", task.key);
        Ok(Answer::said(self.message.clone()).with(json!({"task": task.key, "queued": false})))
    }

    /// Press a key in a ticket's agent on the service when this window has no view of its terminal: `escape` and
    /// `interrupt` with no terminal open.
    /// @param task - the ticket
    /// @param interrupt - Ctrl+C when true, Escape when false
    pub(super) fn press_on_the_service(&mut self, task: &Task, interrupt: bool) -> Result<Answer, String> {
        let board = self.service_board().ok_or("the board is not agent-tasks-service's")?;
        let sent = match interrupt {
            true => board.client().interrupt_terminal(task.id),
            false => board.client().type_into_terminal(task.id, "\u{1b}", false),
        };
        sent.map_err(|problem| format!("{} could not be reached on agent-tasks-service: {problem}", task.key))?;
        Ok(Answer::said(match interrupt {
            true => format!("interrupted {}", task.key),
            false => format!("pressed Escape in {}", task.key),
        }))
    }

    /// Show a ticket's terminal on the service in this window, unless it is already showing.
    ///
    /// The terminal runs `unluminous --board-terminal <id>`, which joins the service's relay to it.
    /// @param task_id - the ticket
    pub(super) fn show_the_service_terminal(&mut self, task_id: i64) -> Result<(), String> {
        if self.terminal_for(task_id).is_some_and(|terminal| terminal.session.is_running()) {
            return Ok(());
        }
        let board = self.service_board().ok_or("the board is not agent-tasks-service's")?;
        let program = std::env::current_exe().map_err(|problem| format!("Unluminous could not find its own program: {problem}"))?;
        let settings = unluminous_terminal::SessionSettings {
            shell: Some(program.display().to_string()),
            args: vec![board_terminal::SWITCH.to_owned(), task_id.to_string()],
            working_directory: std::env::var_os("HOME").map(PathBuf::from),
            env: vec![(board_terminal::URL_VARIABLE.to_owned(), board.address().to_owned()), (board_terminal::TOKEN_VARIABLE.to_owned(), board.token().to_owned())],
            name: None,
        };
        let waker = self.waker.clone().unwrap_or_else(|| std::sync::Arc::new(|| {}));
        let session = unluminous_terminal::Session::spawn(&settings, unluminous_terminal::Size::new(40, 120), waker).map_err(|problem| format!("the ticket's terminal could not be opened: {problem}"))?;
        let session_id = self.store()?.task(task_id)?.and_then(|task| task.session_id).unwrap_or_default();
        self.terminals.retain(|terminal| terminal.task_id != task_id);
        self.terminals.push(TicketTerminal::a_view(task_id, session, session_id, clock::now()));
        Ok(())
    }

    /// When a ticket is opened on the service's board and its agent has a session, show its terminal.
    /// @param task - the ticket that was opened
    pub(super) fn show_the_terminal_of_an_opened_ticket(&mut self, task: &Task) {
        if self.service_board().is_none() || task.session_id.is_none() {
            return;
        }
        if let Err(problem) = self.show_the_service_terminal(task.id) {
            self.message = problem;
        }
    }
}
