//! The Agent-Chat page in the Settings window.
//!
//! Three cards from `components::settings_kit`, which every page of the Settings window is drawn with:
//! Endpoint, Answering and Conversation. It scrolls inside the room the Settings window gives a page.
//!
//! **Which endpoint answers is a choice from a list, and the list is somewhere else** (`task-2247`). The
//! endpoints were the first card of this page, every field of every one of them; they are
//! `Settings -> AI Configuration` now, where Agent Tasks picks from the same list, and this page keeps the one
//! question that is the chat's own: which of them it talks to.
//!
//! **The System box is several lines tall and can be made taller**, because a system prompt is a paragraph
//! and a one line field showed a few words of it at a time.

use crate::components::settings_kit::{self as kit, Column};
use crate::services::agent_chat::AgentChat;
use crate::services::folder_choice;
use crate::services::plugin_ui::{Look, Request};

/// Draw the page inside the rectangle every page gets.
pub fn show(chat: &mut AgentChat, ui: &mut egui::Ui, _look: &Look<'_>) -> Vec<Request> {
    kit::plugin_page(ui, "agent-chat-settings", |ui, column| cards(chat, ui, column))
}

/// The page's three cards.
fn cards(chat: &mut AgentChat, ui: &mut egui::Ui, column: &mut Column) -> Vec<Request> {
    let mut requests = Vec::new();
    // **Asked once a frame at most, and cached for five seconds under that.** See `AgentChat::readiness`.
    let refusals = chat.readiness();
    let mut configuration = chat.configuration().clone();
    let mut changed = false;
    let known = chat.known_folders();
    let project_said = match known.first() {
        Some(project) => format!(
            "This window's project ({})",
            folder_choice::named(&project.display().to_string())
        ),
        None => "The folder this window has open".to_owned(),
    };

    kit::card(ui, column, "Endpoint", None, |card| {
        let names: Vec<String> =
            configuration.providers.iter().map(|provider| provider.name.clone()).collect();
        let chosen_name = configuration.provider().map(|one| one.name.clone()).unwrap_or_default();
        let chosen = names.iter().position(|name| *name == chosen_name);
        let said = match chosen.and_then(|index| refusals.get(index).cloned().flatten()) {
            Some(why) => format!("{why} Settings -> AI Configuration is where the endpoints are."),
            None => "Settings -> AI Configuration is where the endpoints are: what each one runs or where \
                     it sends, its model, and where its key is."
                .to_owned(),
        };
        card.row("Talks to", Some(&said), |row| {
            if let Some(index) = row.select("Endpoint", &names, chosen, 240.0) {
                configuration.chosen = names[index].clone();
                changed = true;
            }
        });
    });

    // **Which card is drawn depends on what the chosen row is**, because the two transports ask opposite
    // questions. An agent is a whole program with its own tools and its own sandbox, so the only question is how
    // much it may do; an endpoint is a model on the end of a socket, so the question is which of Unluminous's
    // own commands it may ask for.
    let an_agent = configuration.provider().is_some_and(|one| one.is_a_program());
    match an_agent {
        true => kit::card(
            ui,
            column,
            "Answering",
            Some(
                "The chosen endpoint is a program on this machine, so it answers with its own tools, its own \
                 sandbox and its own account. It runs in the folder chosen below, which is the project this \
                 window has open unless you choose another, and reads that folder's own instructions.",
            ),
            |card| {
                // **Where the agent runs** (`task-2270`): the folder this window has open, a folder opened
                // before, or any folder through `Other folder...`, which asks the window for its folder picker.
                let chosen = configuration.folder.as_ref().map(|one| one.display().to_string());
                let rows = folder_choice::rows(chosen.as_deref(), &known);
                let words = folder_choice::said(&rows, &project_said);
                let selected = folder_choice::selected(&rows, chosen.as_deref());
                card.row(
                    "Folder",
                    Some("The folder the agent starts in, which is where it reads the project's own instructions."),
                    |row| {
                        if let Some(index) = row.select("Folder", &words, selected, 300.0) {
                            match &rows[index] {
                                folder_choice::Row::Default => {
                                    configuration.folder = None;
                                    changed = true;
                                }
                                folder_choice::Row::Folder(folder) => {
                                    configuration.folder = Some(std::path::PathBuf::from(folder));
                                    changed = true;
                                }
                                folder_choice::Row::Other => requests.push(Request::PickFolder {
                                    id: format!("{}settings", crate::services::agent_chat::PICK_FOLDER),
                                    title: "The folder the agent starts in".to_owned(),
                                    start: configuration.folder.clone(),
                                }),
                            }
                        }
                    },
                );
                let said = permission_said(configuration.permission);
                let names: Vec<&str> = unluminous_chat::PERMISSIONS.to_vec();
                let chosen = names.iter().position(|named| configuration.permission.name() == *named);
                card.row("May", Some(said), |row| {
                    if let Some(index) = row.segmented("May", &names, chosen, 72.0, false) {
                        if let Some(permission) = unluminous_chat::Permission::from_name(names[index]) {
                            configuration.permission = permission;
                            changed = true;
                        }
                    }
                });
            },
        ),
        false => kit::card(ui, column, "Answering", None, |card| {
            changed |= card.switch(
                "Let the model use Unluminous's own commands",
                Some(
                    "On unless you turn it off. The model is offered Unluminous's whole command catalog as \
                     tools, so it can open a file, read the git status or run a search, through exactly the code \
                     a menu entry runs. The commands that run a program of the model's choosing are the switch \
                     below, and that one is off. The same switch is the first button in the composer.",
                ),
                &mut configuration.tools,
            );
            changed |= card.switch(
                "\u{2026}including the ones that run a program",
                Some(
                    "A second switch, because typing into a terminal, starting a run configuration and installing \
                     a debug adapter hand this machine's shell to whatever is on the far end of that URL.",
                ),
                &mut configuration.shell,
            );
            let mut limit = configuration.tool_limit.to_string();
            card.row(
                "Tool rounds",
                Some("How many times in one turn the model may ask for tools before the pane stops asking again."),
                |row| {
                    if row.text("Tool rounds", &mut limit, "100", 96.0) {
                        if let Ok(rounds) = limit.trim().parse::<u32>() {
                            if rounds > 0 {
                                configuration.tool_limit =
                                    rounds.min(crate::services::agent_chat::LARGEST_TOOL_LIMIT);
                                changed = true;
                            }
                        }
                    }
                },
            );
        }),
    }

    kit::card(ui, column, "Conversation", None, |card| {
        let mut system = configuration.system.clone();
        if card.text_area(
            "System",
            Some(
                "Added after Unluminous's own line, which says which file is showing. The file's text is never \
                 sent by Unluminous. Drag the bottom edge to make the box taller.",
            ),
            &mut system,
            "nothing",
        ) {
            configuration.system = system;
            changed = true;
        }
        let mut history = configuration.history.to_string();
        card.row("History", Some("How many conversations are kept."), |row| {
            if row.text("History", &mut history, "20", 96.0) {
                if let Ok(count) = history.trim().parse::<usize>() {
                    if count > 0 {
                        configuration.history = count.min(500);
                        changed = true;
                    }
                }
            }
        });
    });

    if changed {
        *chat.configuration_mut() = configuration;
        chat.readiness_may_have_changed();
        if let Err(problem) = chat.save_the_configuration() {
            requests.push(Request::Message(problem));
        }
    }
    requests
}

/// What each agent's answer to a permission really is, because the two are not the same strength and one
/// label over both would promise something `claude` does not enforce: `codex` is put in an operating system
/// sandbox, and `claude` is told a policy about its own tools while its process, its hooks and its plugins are
/// not limited at all.
fn permission_said(permission: unluminous_chat::Permission) -> &'static str {
    match permission {
        unluminous_chat::Permission::Read => {
            "Look and answer, changing nothing. codex runs in a read-only sandbox; claude refuses any tool of its \
             own that would change something."
        }
        unluminous_chat::Permission::Edit => {
            "Change files in the project this window has open. codex is sandboxed to it; claude accepts its own \
             edits without asking."
        }
        unluminous_chat::Permission::Full => {
            "Anything at all: edit any file and run any command, with no sandbox and nothing to ask. This is what \
             ships, because the agent on the other end is the one already signed in on this machine and trusted \
             with this checkout."
        }
    }
}
