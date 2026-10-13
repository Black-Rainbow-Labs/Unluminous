//! Drawing the Designer: the design tab, the Designs pane, the Settings page and the two dialogs.
//!
//! ## The drawing changes nothing
//!
//! Every function here takes a rectangle, draws, and reports an [`Act`]. Not one of them writes a note,
//! sends a request or touches a file. [`apply`] is the one place an act becomes a change, and it calls the
//! same provider functions `unluminous-cli plugins run designer ...` calls, so a press and a command are the
//! same thing. Text that is being typed is the one exception: a note being written, a reply and the prompt
//! box are fields of the design's view, edited in place by the text box that shows them.
//!
//! ## The ground is the window's
//!
//! The window paints the ground before anything here is called, so nothing here paints a second one. What is
//! painted are the plugin's own surfaces, and they are `rux` surfaces: see [`kit`].

pub mod kit;
pub mod modal;
pub mod notes;
pub mod pane;
pub mod settings_page;
pub mod tab;

use std::path::PathBuf;

use crate::services::designer::notes::NoteState;
use crate::services::designer::page::Mode;
use crate::services::designer::{Designer, Device, Filter, Modal};
use crate::services::plugin_ui::Request;

/// What the drawing reported.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// Open a design in its tab.
    OpenDesign(PathBuf),
    /// The `New Design` button, which asks what to call it.
    NewDesign,
    /// Make a design with this name.
    CreateDesign(String),
    SetMode(PathBuf, Mode),
    SetDevice(PathBuf, Device),
    SetFilter(PathBuf, Filter),
    /// Open the design's file as text.
    OpenSource(PathBuf),
    /// Load the page again.
    Reload(PathBuf),
    ShowHistory(PathBuf),
    /// Put a copy back.
    Restore(PathBuf, String),
    /// Put the dialog away.
    CloseModal,
    /// Move the chosen element to its parent or its first child.
    Walk(PathBuf, bool),
    ClearSelection(PathBuf),
    /// Start a note on the chosen element, in the middle of it.
    StartNote(PathBuf),
    OpenNote(PathBuf, u32),
    /// Turn the note being written into a note, and ask the agent about it when the flag is set.
    AddNote(PathBuf, bool),
    CancelNote(PathBuf),
    /// Send what is in the reply box as a reply.
    Reply(PathBuf, u32),
    Resolve(PathBuf, u32),
    Reopen(PathBuf, u32),
    Delete(PathBuf, u32),
    /// Pin the note to the chosen element.
    MoveToSelection(PathBuf, u32),
    Ask(PathBuf, u32),
    AskAll(PathBuf),
    /// Send what is in the prompt box.
    Prompt(PathBuf),
    /// Show a project path in the platform's file manager.
    Reveal(PathBuf),
}

/// The one place an act becomes a change.
///
/// Anything that went wrong is said in the status bar and in the design's own status line, which is where a
/// refusal belongs: a button that did nothing and said nothing is the fault a person reports as "it is broken".
///
/// @param designer - the provider
/// @param acts - what was pressed this frame
pub fn apply(designer: &mut Designer, acts: Vec<Act>) -> Vec<Request> {
    let mut requests = Vec::new();
    for act in acts {
        if let Act::Reveal(path) = act {
            requests.push(Request::Reveal(path));
            continue;
        }
        if let Err(why) = apply_one(designer, act) {
            requests.push(Request::Message(why));
        }
    }
    requests
}

/// One act.
fn apply_one(designer: &mut Designer, act: Act) -> Result<(), String> {
    match act {
        Act::OpenDesign(design) => designer.open_a_design(&design),
        Act::NewDesign => {
            designer.modal = Some(Modal::NewDesign { name: String::new() });
            Ok(())
        }
        Act::CreateDesign(name) => {
            designer.make_a_design(&name)?;
            designer.modal = None;
            Ok(())
        }
        Act::SetMode(design, mode) => designer.set_mode(&design, mode),
        Act::SetDevice(design, device) => designer.set_device(&design, device),
        Act::SetFilter(design, filter) => designer.set_filter(&design, filter),
        Act::OpenSource(design) => {
            designer.want_the_source(&design);
            Ok(())
        }
        Act::Reload(design) => {
            designer.want_a_reload(&design);
            Ok(())
        }
        Act::ShowHistory(design) => {
            designer.modal = Some(Modal::History { design });
            Ok(())
        }
        Act::Restore(design, copy) => {
            designer.restore(&design, &copy)?;
            designer.modal = None;
            Ok(())
        }
        Act::CloseModal => {
            designer.modal = None;
            Ok(())
        }
        other => apply_to_a_note(designer, other),
    }
}

/// The acts about a design's selection and notes.
fn apply_to_a_note(designer: &mut Designer, act: Act) -> Result<(), String> {
    match act {
        Act::Walk(design, up) => designer.walk_the_selection(&design, up),
        Act::ClearSelection(design) => designer.clear_the_selection(&design),
        Act::StartNote(design) => designer.start_a_note_on_the_selection(&design),
        Act::OpenNote(design, id) => designer.open_note(&design, id),
        Act::AddNote(design, ask) => designer.add_the_note(&design, ask).map(|_| ()),
        Act::CancelNote(design) => designer.cancel_the_note(&design),
        Act::Reply(design, id) => designer.reply_from_the_box(&design, id),
        Act::Resolve(design, id) => designer.set_state(&design, id, NoteState::Resolved),
        Act::Reopen(design, id) => designer.set_state(&design, id, NoteState::Open),
        Act::Delete(design, id) => designer.delete_note(&design, id),
        Act::MoveToSelection(design, id) => designer.move_note_to_the_selection(&design, id),
        Act::Ask(design, id) => designer.ask_note(&design, id),
        Act::AskAll(design) => designer.ask_every_open_note(&design).map(|_| ()),
        Act::Prompt(design) => {
            let words = designer
                .design_view(&design)
                .map(|view| view.prompt.clone())
                .ok_or_else(|| "That design is not open.".to_owned())?;
            designer.ask_about_the_design(&design, &words)
        }
        _ => Ok(()),
    }
}
