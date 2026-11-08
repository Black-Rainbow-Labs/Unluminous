//! The Agent-Tasks page in the Settings window.
//!
//! Four cards from `components::settings_kit` in the Settings design's layout (`task-2246`): the board, what a
//! new ticket gets, how an agent is launched, and what the supervisor is started on (`task-2556`). It scrolls inside the room the Settings window gives a page, as
//! every page does.
//!
//! **Which agent a ticket goes to is an endpoint** (`task-2247`), chosen from the list in
//! `Settings -> AI Configuration`. The card that named the Iliad gateway and held its key is gone: the agents
//! the board launches are the command-line agents installed on this machine, and they hold their own accounts.
//!
//! ## A value can be copied where it is something to paste somewhere else
//!
//! The board's path has `Copy` and `Reveal`, because it is what a backup script or a terminal is given, and
//! the default project has `Copy` for the same reason.
//!
use crate::components::settings_kit::{self as kit, Column};
use crate::services::agent_tasks::{AgentTasks, EFFORTS};
use crate::services::plugin_ui::{Look, Request};

/// What a select says for a setting that is left to the agent.
const THE_AGENTS_OWN: &str = "The agent's own default";

/// Draw the page inside the rectangle every page gets.
pub fn show(board: &mut AgentTasks, ui: &mut egui::Ui, _look: &Look<'_>) -> Vec<Request> {
    kit::plugin_page(ui, "agent-tasks-settings", |ui, column| cards(board, ui, column))
}

/// The page's four cards.
fn cards(board: &mut AgentTasks, ui: &mut egui::Ui, column: &mut Column) -> Vec<Request> {
    let mut requests = Vec::new();
    let configuration = board.configuration().clone();
    let mut changed = configuration.clone();

    kit::card(ui, column, "The board", None, |card| {
        // The board file. Read only here, because moving a database is a thing to do to a file rather than a
        // thing to type: what this offers is the path, so it can be pasted into a backup script or a terminal.
        let where_it_is = board.board_where();
        let (copied, revealed) = card.row(
            "Board file",
            Some(
                "One Inillucent file. Copy the path to back it up, Reveal to open the folder it is in, or set \
                 database in this plugin's own settings.conf to move it.",
            ),
            |row| {
                row.path("Board file", &where_it_is, 300.0);
                (row.button("Copy"), row.button("Reveal"))
            },
        );
        if copied {
            requests.push(Request::Copy(where_it_is.clone()));
            requests.push(Request::Message("copied".to_owned()));
        }
        // A board in memory has no file to show anybody, so there is nothing to reveal.
        if revealed {
            if let Some(path) = board.database_path() {
                requests.push(Request::Reveal(path));
            }
        }

        let mut project = configuration
            .project
            .clone()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let copy = card.row(
            "Default project",
            Some("Where an agent is launched when a ticket names no project of its own."),
            |row| {
                if row.text(
                    "Default project",
                    &mut project,
                    "the folder this window has open",
                    300.0,
                ) {
                    changed.project = Some(project.clone())
                        .filter(|value: &String| !value.trim().is_empty())
                        .map(std::path::PathBuf::from);
                }
                row.ghost("Copy")
            },
        );
        if copy {
            requests.push(Request::Copy(project.clone()));
            requests.push(Request::Message("copied".to_owned()));
        }

        let mut lease = configuration.lease_minutes.to_string();
        card.row(
            "Lease",
            Some(
                "How many minutes the board waits to hear from an agent before the watchdog calls its lease \
                 expired and starts nudging it.",
            ),
            |row| {
                if row.text("Lease", &mut lease, "45", 96.0) {
                    if let Ok(minutes) = lease.trim().parse::<i64>() {
                        if minutes > 0 {
                            changed.lease_minutes = minutes;
                        }
                    }
                }
                row.suffix("minutes");
            },
        );
    });

    kit::card(ui, column, "What a new ticket gets", None, |card| {
        // A list of the endpoints that run an agent, which is the same list a ticket's own assignee is chosen
        // from. `task-2247`.
        let mut names = board.assignee_choices();
        names.retain(|name| name != "human");
        let (_, now) = board.default_assignee();
        let chosen = now
            .or_else(|| Some(configuration.agent.name().to_owned()))
            .and_then(|name| names.iter().position(|one| *one == name));
        let picked = card.row(
            "Endpoint",
            Some(
                "Which endpoint a new ticket is given to, and which one takes a ticket assigned to a person. The \
                 model integrations are in Settings -> AI Configuration; the ones that run an agent on this machine are \
                 offered here.",
            ),
            |row| row.select("Default endpoint", &names, chosen, 240.0),
        );
        if let Some(index) = picked {
            if let Ok((kind, endpoint)) = board.assignee_named(&names[index]) {
                changed.agent = kind;
                changed.endpoint = endpoint;
            }
        }

        // A list rather than a field, and the **same** list a ticket's own `Model` is chosen from, so the value a
        // new ticket gets and the value a ticket holds are chosen from one list. `task-28`.
        let models = crate::services::agent_tasks::agent::models_for(
            configuration.agent,
            configuration.model.as_deref(),
        );
        if let Some(picked) = choose(
            card,
            "Default model",
            "Passed as --model. The models the chosen agent has, and whatever this setting already names if it \
             is not one of them.",
            &models,
            configuration.model.as_deref().unwrap_or(""),
        ) {
            changed.model = Some(picked).filter(|value: &String| !value.trim().is_empty());
        }
        let efforts: Vec<String> = EFFORTS.iter().map(|level| (*level).to_owned()).collect();
        if let Some(picked) = choose(
            card,
            "Default effort",
            "Passed as --effort. Codex knows nothing above high, so xhigh and max collapse onto it.",
            &efforts,
            configuration.effort.as_deref().unwrap_or(""),
        ) {
            changed.effort = Some(picked).filter(|value: &String| !value.trim().is_empty());
        }
    });

    kit::card(ui, column, "How an agent is launched", None, |card| {
        // **A whole command rather than an extra flags field**, so the program itself can be named: a wrapper
        // script, a particular version under a version manager, or `claude` by its full path. Split the way a
        // run configuration is split, so **no shell runs it** and what is typed is what runs.
        let mut claude = configuration.claude_command.clone().unwrap_or_default();
        card.row(
            "Claude command",
            Some(
                "The program and the flags in front of it, for a ticket assigned to Claude. The ticket's own model \
                 and effort and the session flag are added after it. No shell runs this, so nothing expands and a \
                 path with spaces in it goes in \"quotes\".",
            ),
            |row| {
                if row.code_text("Claude command", &mut claude, "claude --dangerously-skip-permissions", 360.0) {
                    changed.claude_command = Some(claude.clone()).filter(|value| !value.trim().is_empty());
                }
            },
        );
        let mut codex = configuration.codex_command.clone().unwrap_or_default();
        card.row(
            "Codex command",
            Some(
                "The same for a ticket assigned to Codex. resume <id> stays the first thing after the program, \
                 because it is a subcommand rather than a flag.",
            ),
            |row| {
                if row.code_text("Codex command", &mut codex, "codex", 360.0) {
                    changed.codex_command = Some(codex.clone()).filter(|value| !value.trim().is_empty());
                }
            },
        );
    });

    supervisor_card(board, ui, column, &configuration, &mut changed);

    // Written once anything above changed, on the same terms the window's own settings are.
    if changed != configuration {
        if let Err(problem) = board.change_the_configuration(changed) {
            requests.push(Request::Message(problem));
        }
    }
    requests
}

/// The fourth card: what the board's supervisor is started on (`task-2556`). The same three fields the gear in the
/// supervisor modal shows, written to the same three settings.
fn supervisor_card(
    board: &AgentTasks,
    ui: &mut egui::Ui,
    column: &mut Column,
    configuration: &crate::services::agent_tasks::Configuration,
    changed: &mut crate::services::agent_tasks::Configuration,
) {
    kit::card(ui, column, "The supervisor", None, |card| {
        let mut names = vec![THE_BOARDS_ENDPOINT.to_owned()];
        names.extend(board.assignee_choices().into_iter().filter(|name| name != "human"));
        let current = configuration.supervisor_endpoint.clone().unwrap_or_default();
        let chosen = match current.is_empty() {
            true => Some(0),
            false => names.iter().position(|name| *name == current),
        };
        let picked = card.row(
            "Supervisor endpoint",
            Some(
                "Which endpoint Chat with supervisor starts the board's supervisor on. How it works the board is the \
                 skill file in the supervisor's Skill tab.",
            ),
            |row| row.select("Supervisor endpoint", &names, chosen, 240.0),
        );
        if let Some(index) = picked {
            let named = match index {
                0 => None,
                at => board
                    .assignee_named(&names[at])
                    .ok()
                    .map(|(kind, endpoint)| endpoint.unwrap_or_else(|| kind.name().to_owned())),
            };
            if named != changed.supervisor_endpoint {
                changed.supervisor_endpoint = named;
                changed.supervisor_model = None;
            }
        }
        if let Some(picked) = choose(
            card,
            "Supervisor model",
            "Passed as --model when the supervisor starts. Empty is the endpoint's own model.",
            &board.supervisor_models(),
            configuration.supervisor_model.as_deref().unwrap_or(""),
        ) {
            changed.supervisor_model =
                Some(picked).filter(|value: &String| !value.trim().is_empty());
        }
        let efforts: Vec<String> = EFFORTS.iter().map(|level| (*level).to_owned()).collect();
        if let Some(picked) = choose(
            card,
            "Supervisor effort",
            "Passed as --effort when the supervisor starts.",
            &efforts,
            configuration.supervisor_effort.as_deref().unwrap_or(""),
        ) {
            changed.supervisor_effort =
                Some(picked).filter(|value: &String| !value.trim().is_empty());
        }
    });
}

/// What the supervisor's endpoint select says for the endpoint a new ticket is given to.
const THE_BOARDS_ENDPOINT: &str = "The board's endpoint";

/// A setting chosen from a list that may be left to the agent, which is the list's first row. Answers what was
/// chosen, empty for the agent's own, when it changed.
fn choose(
    card: &mut kit::Card<'_, '_>,
    name: &str,
    help: &str,
    values: &[String],
    current: &str,
) -> Option<String> {
    let mut options = vec![THE_AGENTS_OWN.to_owned()];
    options.extend(values.iter().filter(|value| !value.is_empty()).cloned());
    let chosen = match current.is_empty() {
        true => Some(0),
        false => match options.iter().position(|option| option == current) {
            Some(at) => Some(at),
            None => {
                options.push(current.to_owned());
                Some(options.len() - 1)
            }
        },
    };
    card.row(name, Some(help), |row| {
        row.select(name, &options, chosen, 300.0).map(|index| match index {
            0 => String::new(),
            at => options[at].clone(),
        })
    })
}
