//! The verbs of `unluminous-cli plugins run designer ...`, each through the function its button calls.
//!
//! A verb about "the showing design" takes `--design <name>` as its first two words instead, so an agent can
//! work on a design nobody is looking at, and `--as <name>` says who is writing a note or a reply (`agent`
//! when it does not say). A verb that needs the page, which is `select`, `device` and `mode`, answers with a
//! refusal naming `open` when the design is not showing, because there is no page to tell.

use std::path::PathBuf;

use serde_json::json;

use super::notes::NoteState;
use super::page::Mode;
use super::{library, Designer, Device, Modal};
use crate::services::plugin_ui::{argument, rest, Answer};

/// Every verb, with one line each, for `plugins show designer`.
pub const LIST: &[(&str, &str)] = unluminous_cli::plugins::DESIGNER;

/// The words of a command line once the leading flags are taken off.
struct Parsed<'a> {
    design: Option<String>,
    author: String,
    words: &'a [String],
}

/// Read `--design <name>` and `--as <name>` off the front of a command's arguments.
fn parse(arguments: &[String]) -> Parsed<'_> {
    let mut at = 0;
    let (mut design, mut author) = (None, "agent".to_owned());
    while let (Some(flag), Some(value)) = (argument(arguments, at), argument(arguments, at + 1)) {
        match flag {
            "--design" => design = Some(value.to_owned()),
            "--as" => author = value.to_owned(),
            _ => break,
        }
        at += 2;
    }
    Parsed { design, author, words: &arguments[at..] }
}

/// Answer one verb.
///
/// @param designer - the provider
/// @param command - the verb
/// @param arguments - the words after it
pub fn run(designer: &mut Designer, command: &str, arguments: &[String]) -> Result<Answer, String> {
    let parsed = parse(arguments);
    match command {
        "open-pane" => Ok(Answer::said("showing the Designs pane")),
        "designs" => Ok(list_the_designs(designer)),
        "new" => make_one(designer, &parsed),
        "open" => open_one(designer, &parsed),
        "tabs" | "new-tab" | "show-tab" | "close-tab" => about_the_tabs(designer, command, &parsed),
        "device" | "mode" | "mode-browse" | "mode-comment" | "select" => {
            about_the_page(designer, command, &parsed)
        }
        "selection" | "comments" | "comment" | "reply" | "resolve" | "reopen" | "delete"
        | "move" => about_the_notes(designer, command, &parsed),
        "ask" | "ask-all" | "prompt" | "history" | "restore" | "reload" | "source" => {
            about_the_agent(designer, command, &parsed)
        }
        other => {
            let known: Vec<&str> = LIST.iter().map(|(name, _)| *name).collect();
            Err(format!("there is no `{other}` command on `designer`: {}", known.join(", ")))
        }
    }
}

/// The design a command is about: the one named, or else the one showing.
fn which(designer: &mut Designer, named: &Option<String>) -> Result<PathBuf, String> {
    match named {
        Some(name) => {
            let project = designer.project().ok_or("No project is open.")?.to_path_buf();
            let found = library::find(&project, &designer.config.folder.clone(), name)?;
            designer.loaded(&found)?;
            Ok(found)
        }
        None => designer.showing_design().map(PathBuf::from).ok_or_else(|| {
            "No design is showing. Name one with --design <name>, or open one with `open <name>`."
                .to_owned()
        }),
    }
}

/// The design a command is about, which has to be showing because the command needs its page.
fn which_showing(designer: &mut Designer, named: &Option<String>) -> Result<PathBuf, String> {
    let design = which(designer, named)?;
    match designer.showing_design() == Some(design.as_path()) {
        true => Ok(design),
        false => Err(format!(
            "`{}` is not showing, so there is no page to tell. Open it first with `open {}`.",
            designer.name_of(&design),
            designer.name_of(&design)
        )),
    }
}

/// A note's number, written `3`, `#3` or `note-3`.
fn note_id(word: Option<&str>) -> Result<u32, String> {
    let word = word.ok_or("Say which note, by its number.")?;
    let digits = word.trim_start_matches("note-").trim_start_matches('#');
    digits.parse().map_err(|_| format!("`{word}` is not a note number."))
}

/// `designs`: every design with its note counts.
fn list_the_designs(designer: &Designer) -> Answer {
    let designs: Vec<_> = designer
        .designs()
        .iter()
        .map(|design| json!({ "name": design.name, "open": design.open, "resolved": design.resolved }))
        .collect();
    let said = counted(designs.len(), "design");
    Answer::said(said)
        .with(json!({ "folder": designer.config.folder.display().to_string(), "designs": designs }))
}

/// `new`: make a design, or ask for its name when none was given.
fn make_one(designer: &mut Designer, parsed: &Parsed<'_>) -> Result<Answer, String> {
    let name = rest(parsed.words, 0);
    if name.trim().is_empty() {
        designer.modal = Some(Modal::NewDesign { name: String::new() });
        return Ok(Answer::said("Name the new design").with(json!({ "asked": true })));
    }
    let made = designer.make_a_design(&name)?;
    let shown = designer.name_of(&made);
    Ok(Answer::said(format!("Made {shown}"))
        .with(json!({ "design": shown, "path": super::config::slashed(&made) })))
}

/// `open`: ask the window to open a design in its tab.
fn open_one(designer: &mut Designer, parsed: &Parsed<'_>) -> Result<Answer, String> {
    let name = rest(parsed.words, 0);
    let named = Some(name).filter(|name| !name.trim().is_empty()).or_else(|| parsed.design.clone());
    let design = which(designer, &named)?;
    designer.open_a_design(&design)?;
    let shown = designer.name_of(&design);
    Ok(Answer::said(format!("Opening {shown}")).with(json!({ "design": shown })))
}

/// The verbs about the Designer pane's own tabs, each answering with the tabs as they are afterwards.
fn about_the_tabs(
    designer: &mut Designer,
    command: &str,
    parsed: &Parsed<'_>,
) -> Result<Answer, String> {
    let named = rest(parsed.words, 0);
    match command {
        "new-tab" => designer.new_tab(),
        "show-tab" => {
            if named.trim().is_empty() {
                return Err("say which tab: show-tab <number|name>".to_owned());
            }
            let index = designer.tab_named(&named)?;
            designer.show_tab(index)?;
        }
        "close-tab" => {
            let index = match named.trim().is_empty() {
                true => designer.active_tab(),
                false => designer.tab_named(&named)?,
            };
            designer.close_tab(index)?;
        }
        _ => {}
    }
    let tabs: Vec<serde_json::Value> = designer
        .tabs()
        .iter()
        .map(|tab| match tab {
            Some(design) => json!(designer.name_of(design)),
            None => serde_json::Value::Null,
        })
        .collect();
    let active = designer.active_tab();
    let titles: Vec<String> = designer.tabs().iter().map(|tab| designer.tab_title(tab)).collect();
    Ok(Answer::said(format!("{} is showing, of {}", titles[active], counted(titles.len(), "tab")))
        .with(json!({ "tabs": tabs, "active": active })))
}

/// The verbs that tell the page something.
fn about_the_page(
    designer: &mut Designer,
    command: &str,
    parsed: &Parsed<'_>,
) -> Result<Answer, String> {
    let design = which_showing(designer, &parsed.design)?;
    match command {
        "device" => {
            let wanted = argument(parsed.words, 0).unwrap_or_default();
            let device = Device::from_name(wanted).ok_or_else(|| {
                let names: Vec<&str> = Device::ALL.iter().map(|device| device.name()).collect();
                format!("`{wanted}` is not a device. The devices are: {}.", names.join(", "))
            })?;
            designer.set_device(&design, device)?;
            Ok(Answer::said(format!("Device: {}", device.title()))
                .with(json!({ "device": device.name() })))
        }
        "select" => {
            let selector = rest(parsed.words, 0);
            designer.select_element(&design, &selector)?;
            Ok(Answer::said("Asked the page").with(json!({ "asked": true, "selector": selector })))
        }
        _ => {
            let word = command
                .strip_prefix("mode-")
                .or_else(|| argument(parsed.words, 0))
                .unwrap_or_default();
            let mode = Mode::from_name(word).ok_or_else(|| {
                let names: Vec<&str> = Mode::ALL.iter().map(|mode| mode.name()).collect();
                format!("`{word}` is not a mode. The modes are: {}.", names.join(", "))
            })?;
            designer.set_mode(&design, mode)?;
            Ok(Answer::said(format!("Mode: {}", mode.title())).with(json!({ "mode": mode.name() })))
        }
    }
}

/// The verbs that read or change a design's notes.
fn about_the_notes(
    designer: &mut Designer,
    command: &str,
    parsed: &Parsed<'_>,
) -> Result<Answer, String> {
    let design = which(designer, &parsed.design)?;
    let words = parsed.words;
    let author = parsed.author.as_str();
    match command {
        "selection" => {
            let view = designer.loaded(&design)?;
            let value = view.selection.as_ref().map(super::actions::element_value);
            Ok(Answer::said(
                value.as_ref().map_or("Nothing is chosen".to_owned(), |_| "Chosen".to_owned()),
            )
            .with(json!({ "selection": value })))
        }
        "comments" => the_comments(designer, &design, argument(words, 0).unwrap_or("open")),
        "comment" => {
            let (selector, text) = selector_and_text(words)?;
            let id = designer.comment(&design, &selector, &text, author)?;
            Ok(Answer::said(format!("Note {id} made")).with(json!({ "note": id })))
        }
        "reply" => {
            let id = note_id(argument(words, 0))?;
            designer.reply(&design, id, &rest(words, 1), author)?;
            Ok(Answer::said(format!("Replied to note {id}")).with(json!({ "note": id })))
        }
        "resolve" | "reopen" => {
            let id = note_id(argument(words, 0))?;
            let state = if command == "resolve" { NoteState::Resolved } else { NoteState::Open };
            designer.set_state(&design, id, state)?;
            Ok(Answer::said(format!("Note {id} is {}", state.name()))
                .with(json!({ "note": id, "state": state.name() })))
        }
        "delete" => {
            let id = note_id(argument(words, 0))?;
            designer.delete_note(&design, id)?;
            Ok(Answer::said(format!("Note {id} deleted")).with(json!({ "note": id })))
        }
        _ => {
            let id = note_id(argument(words, 0))?;
            designer.move_note(&design, id, &rest(words, 1))?;
            Ok(Answer::said(format!("Note {id} moved")).with(json!({ "note": id })))
        }
    }
}

/// `comments`: the notes, filtered by state.
fn the_comments(
    designer: &mut Designer,
    design: &std::path::Path,
    which: &str,
) -> Result<Answer, String> {
    let name = designer.name_of(design);
    let view = designer.loaded(design)?;
    let wanted = match which {
        "open" => Some(NoteState::Open),
        "resolved" => Some(NoteState::Resolved),
        "all" => None,
        other => return Err(format!("`{other}` is not open, resolved or all.")),
    };
    let notes: Vec<_> = view
        .notes
        .notes
        .iter()
        .filter(|note| wanted.is_none_or(|state| note.state == state))
        .map(|note| super::actions::note_value(note, view))
        .collect();
    Ok(Answer::said(format!("{} on {name}", counted(notes.len(), "note")))
        .with(json!({ "design": name, "notes": notes })))
}

/// The verbs that hand the design to the agent, or put it back.
fn about_the_agent(
    designer: &mut Designer,
    command: &str,
    parsed: &Parsed<'_>,
) -> Result<Answer, String> {
    let design = which(designer, &parsed.design)?;
    let words = parsed.words;
    match command {
        "ask" => {
            let id = note_id(argument(words, 0))?;
            designer.ask_note(&design, id)?;
            Ok(Answer::said(format!("Sent note {id} to the agent")).with(json!({ "note": id })))
        }
        "ask-all" => {
            let sent = designer.ask_every_open_note(&design)?;
            Ok(Answer::said(format!("Sent {sent} notes to the agent"))
                .with(json!({ "notes": sent })))
        }
        "prompt" => {
            designer.ask_about_the_design(&design, &rest(words, 0))?;
            Ok(Answer::said("Sent to the agent").with(json!({ "sent": true })))
        }
        "history" => {
            let copies = designer.copies(&design);
            if designer.showing_design() == Some(design.as_path()) {
                designer.modal = Some(Modal::History { design: design.clone() });
            }
            let rows: Vec<_> = copies
                .iter()
                .map(|copy| json!({ "name": copy.name, "when": copy.when(), "words": copy.words }))
                .collect();
            Ok(Answer::said(format!("{} copies", rows.len())).with(json!({ "copies": rows })))
        }
        "restore" => {
            let chosen = designer.restore(&design, &rest(words, 0))?;
            Ok(Answer::said(format!("Restored {}", chosen.when()))
                .with(json!({ "copy": chosen.name })))
        }
        "reload" => {
            designer.want_a_reload(&design);
            Ok(Answer::said("Reloading"))
        }
        _ => {
            designer.want_the_source(&design);
            Ok(Answer::said("Opening the source"))
        }
    }
}

/// The selector and the note's words from `comment`'s arguments.
///
/// A selector often has spaces in it, `main > section.hero > h1`, and a command line is split on spaces with no
/// way to quote, so `--` ends a selector that has some. With no `--` the selector is the first word, which is
/// every selector that is an id, a class or a tag.
///
/// @param words - what followed `comment`
fn selector_and_text(words: &[String]) -> Result<(String, String), String> {
    let (selector, text) = match words.iter().position(|word| word == "--") {
        Some(at) => (words[..at].join(" "), rest(words, at + 1)),
        None => (argument(words, 0).unwrap_or_default().to_owned(), rest(words, 1)),
    };
    match selector.trim().is_empty() {
        true => Err("Say which element, as a CSS selector.".to_owned()),
        false => Ok((selector, text)),
    }
}

/// A count and its noun, with the noun made plural when the count is not one: `1 note`, `3 notes`.
///
/// @param count - how many
/// @param noun - the noun for one
fn counted(count: usize, noun: &str) -> String {
    match count {
        1 => format!("1 {noun}"),
        _ => format!("{count} {noun}s"),
    }
}
