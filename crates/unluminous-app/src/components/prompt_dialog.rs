//! The one text prompt, and the one confirmation, that the rest of Unluminous asks with.
//!
//! New File, Rename, New Branch, New Tag, Stash Changes and Clone all need a line of text and a
//! name for what they are about to do, and Rollback, a hard Reset and dropping a stash all need a
//! question with two answers. Six prompts and three confirmations would be nine dialogs that
//! almost agree; these are two, laid out the way `design/style-guide.md` says a modal is laid out.
//!
//! Neither knows what it is for. The window holds a [`Prompt`] with a [`Purpose`] on it, and
//! [`UnluminousApp::run_action`] decides what a confirmed prompt means, so a prompt cannot do anything by
//! itself.

use std::path::PathBuf;

use egui::{Pos2, Rect, Vec2};

use crate::components::modal;

const WIDTH: f32 = 460.0;
const HEIGHT: f32 = 190.0;
const HEADER: f32 = 46.0;
const FOOTER: f32 = 52.0;

/// What a prompt is for, which is what the window looks at when it is confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Purpose {
    /// Render the typed HTTP address or HTML path in a browser tab.
    OpenWebAddress,
    /// Make an empty file in this folder, with the name that is typed.
    NewFile(PathBuf),
    /// Make a folder inside this one, with the name that is typed.
    NewFolder(PathBuf),
    /// Make a file in this folder from the template with this name, called what is typed. `task-2336`.
    NewFromTemplate { folder: PathBuf, template: String },
    /// Open a tab with no file from the template with this name (none when empty), called what is typed.
    NewUntitledTab(String),
    /// The value of one variable a template asks for, on the way to making what [`Made`] says.
    TemplateVariable {
        made: Made,
        template: String,
        /// The values given so far, the variable's name and what was typed.
        given: Vec<(String, String)>,
        /// The variable this prompt asks for.
        asking: String,
        /// The ones still to ask for after it.
        remaining: Vec<String>,
    },
    /// Make a template from this file, called what is typed.
    SaveAsTemplate(PathBuf),
    /// Rename this file or folder to the name that is typed.
    Rename(PathBuf),
    /// Start a branch with the name that is typed, from the current one.
    NewBranch,
    /// Tag the current commit with the name that is typed.
    NewTag,
    /// Put the changes away with the message that is typed.
    Stash,
    /// Clone the repository at the address that is typed.
    Clone,
    /// Show what changed against the revision that is typed.
    CompareWithRevision(PathBuf),
    /// Move the branch to the revision that is typed, in the mode already chosen.
    ResetTo(&'static str),
    /// Call this terminal tab, counting from 0, whatever is typed.
    ///
    /// The number rather than "the tab that is showing", because a prompt is answered on some later
    /// frame and the tab that is showing is a thing that can change in between.
    RenameTerminalTab(usize),
    /// Call this node on the Realm whatever is typed - `task-1904`.
    ///
    /// The node's id rather than "the node that is chosen", which is `RenameTerminalTab`'s own reason:
    /// a prompt is answered on some later frame and what is chosen can change in between.
    RenameRealmNode(u64),
    /// Make a note called whatever is typed, at this place on the canvas in world points. `task-2202`.
    NewNote(i32, i32),
    /// Rename the file this note node shows to whatever is typed. `task-2202`.
    RenameNote(u64),
    /// Rename the realm file at this path in the project to whatever is typed. `task-2202`.
    RenameRealmView(std::path::PathBuf),
    /// Make a realm called whatever is typed, in `.realm-files/`. `task-2202`.
    NewRealm,
    /// Put the caret on the line that is typed, which may be `42` or `42:5` — `task-1922` WP4.
    ///
    /// The one purpose here that is not a name for something. It is this prompt rather than a modal
    /// of its own because it is exactly what this modal is: one line of text, a note saying what
    /// will happen, and a button that does it.
    GoToLine,
}

/// What a template is being expanded into once its variables are known. `task-2336`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Made {
    /// A file at this path.
    File(PathBuf),
    /// A tab with no file, going to be called this.
    Tab(String),
}

/// A prompt the window is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub title: String,
    /// A line under the title saying what will happen.
    pub note: String,
    /// What is in the field. Seeded with something sensible, and selected when the prompt opens.
    pub value: String,
    /// The word on the button that does it.
    pub confirm: String,
    pub purpose: Purpose,
}

impl Prompt {
    pub fn new(title: &str, note: &str, value: &str, confirm: &str, purpose: Purpose) -> Self {
        Self {
            title: title.to_owned(),
            note: note.to_owned(),
            value: value.to_owned(),
            confirm: confirm.to_owned(),
            purpose,
        }
    }
}

/// What happened in the prompt this frame.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PromptOutcome {
    /// The button was pressed, or Enter typed, with this in the field.
    pub confirmed: bool,
    /// Cancel, Escape, or the close cross.
    pub cancelled: bool,
}

/// Draw the prompt. The caller owns whether there is one at all.
pub fn show(ctx: &egui::Context, prompt: &mut Prompt) -> PromptOutcome {
    let mut outcome = PromptOutcome::default();
    let closed = modal("unluminous-prompt", ctx, |ui, area| {
        if modal::header(ui, area, &prompt.title) {
            outcome.cancelled = true;
        }
        let body = body_rect(area);
        note(ui, body, &prompt.note);

        let field = Rect::from_min_size(
            Pos2::new(body.left(), body.top() + 40.0),
            Vec2::new(body.width(), 30.0),
        );
        let entry = modal::field(ui, field, "Name", &mut prompt.value);
        // The field has the keyboard as soon as the prompt opens, because a prompt that has to be
        // clicked before it can be typed into is a prompt that gets typed past.
        if !entry.has_focus() {
            entry.request_focus();
        }
        if entry.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            outcome.confirmed = true;
        }

        buttons(ui, area, &prompt.confirm, !prompt.value.trim().is_empty(), &mut outcome);
    });
    if closed {
        outcome.cancelled = true;
    }
    outcome
}

/// A question with two answers, for anything that cannot be undone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmation {
    pub title: String,
    pub note: String,
    pub confirm: String,
    /// What to do when it is confirmed. The window holds the action rather than the dialog.
    pub purpose: String,
}

/// Draw a confirmation.
pub fn confirm(ctx: &egui::Context, question: &Confirmation) -> PromptOutcome {
    let mut outcome = PromptOutcome::default();
    let closed = modal("unluminous-confirm", ctx, |ui, area| {
        if modal::header(ui, area, &question.title) {
            outcome.cancelled = true;
        }
        note(ui, body_rect(area), &question.note);
        buttons(ui, area, &question.confirm, true, &mut outcome);
    });
    if closed {
        outcome.cancelled = true;
    }
    outcome
}

/// The frame both of them are drawn in, which is the frame `design/style-guide.md` describes.
///
/// `components::modal` owns that frame, the header and the footer, and owns the dragging and the
/// resizing with it, so the prompt and the confirmation are moved, resized and headed exactly as
/// the Settings window and the git dialogs are. What is left here is the two sizes, and the name the
/// placement is remembered under.
fn modal(id: &str, ctx: &egui::Context, contents: impl FnOnce(&mut egui::Ui, Rect)) -> bool {
    let (_, close) = modal::show(ctx, id, WIDTH, HEIGHT, |ui, area| contents(ui, area));
    close
}

fn body_rect(area: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(area.left() + 24.0, area.top() + HEADER + 14.0),
        Pos2::new(area.right() - 24.0, area.bottom() - FOOTER),
    )
}

fn note(ui: &mut egui::Ui, body: Rect, text: &str) {
    modal::note(ui, body, body.top(), text);
}

/// `Cancel` and the button that does the thing, which are the shared footer's: Enter presses the second,
/// which is what answers the **confirmation**, which has no field to lose focus and so had no other way of
/// being answered from the keyboard at all.
fn buttons(
    ui: &mut egui::Ui,
    area: Rect,
    confirm: &str,
    enabled: bool,
    outcome: &mut PromptOutcome,
) {
    match modal::footer(ui, area, &[("Cancel", true), (confirm, enabled)]) {
        Some(0) => outcome.cancelled = true,
        Some(_) => outcome.confirmed = true,
        None => {}
    }
}
