//! `File -> Create Project...`: a name, where it goes, and whether it starts a git repository.
//!
//! `task-2004` asks for the reference editor's New Project dialog — *"allows me to type in a project
//! name, show/set the default location, and checkbox to git init"* — and that is three controls where
//! `components::prompt_dialog` is one field. So it is a modal of its own, built out of
//! `components::modal` like every other dialog, which is what gives it the frame, the header, the
//! footer, dragging, resizing and `Enter` with nothing written here.
//!
//! **It decides nothing.** The window holds a [`NewProject`] and `UnluminousApp::run_action` is what
//! makes the folder, runs git and opens the window — the split `components::prompt_dialog` already
//! keeps, and it is what lets `unluminous-cli project new` reach the same code without a dialog.
//!
//! The line under the two fields is the reference dialog's own, and it is what makes them legible
//! together: two boxes saying `unluminous` and `C:\jason\dev` do not obviously add up to a path, and
//! `Project will be created in: C:\jason\dev\unluminous` does.

use std::path::{Path, PathBuf};

use egui::{Pos2, Rect, Vec2};

use crate::components::modal;
use crate::theme::color;

const WIDTH: f32 = 520.0;
const HEIGHT: f32 = 268.0;

/// What is typed into the dialog, which is the whole of its state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProject {
    /// The folder's name.
    pub name: String,
    /// The folder it goes in.
    pub location: String,
    /// Whether to run `git init` in it.
    pub git: bool,
    /// Why the last attempt was refused, drawn under the fields until something is typed.
    pub problem: Option<String>,
}

impl NewProject {
    /// A dialog starting beside the project that is open, which is where a second one usually goes.
    ///
    /// `untitled` counts up past whatever is already there, so pressing Create twice in a row makes two
    /// projects rather than refusing the second — the reference dialog's own `untitled1`, `untitled2`.
    pub fn beside(project: &Path) -> Self {
        let location = project.parent().unwrap_or(project).to_path_buf();
        let mut name = String::new();
        for number in 1..1000 {
            name = format!("untitled{number}");
            if !location.join(&name).exists() {
                break;
            }
        }
        Self { name, location: location.display().to_string(), git: true, problem: None }
    }

    /// The folder this would make, which is what the line under the fields says.
    pub fn folder(&self) -> PathBuf {
        Path::new(self.location.trim()).join(self.name.trim())
    }

    /// Why it cannot be made, or nothing when it can.
    ///
    /// **Refusals are sentences rather than a button that does nothing.** Each one names the thing that
    /// is wrong rather than saying the form is invalid, which is `unluminous-git`'s rule about never
    /// inventing an error message applied to a dialog that has no program behind it to quote.
    pub fn why_not(&self) -> Option<String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Some("Type a name for the project.".to_owned());
        }
        if name.contains('/') || name.contains('\\') {
            return Some(
                "A project name is a folder name, so it cannot hold a path separator.".to_owned(),
            );
        }
        if name == "." || name == ".." {
            return Some("That is not a name a folder can have.".to_owned());
        }
        let location = Path::new(self.location.trim());
        if self.location.trim().is_empty() {
            return Some("Say which folder the project goes in.".to_owned());
        }
        if !location.is_dir() {
            return Some(format!("{} is not a folder on this machine.", location.display()));
        }
        let folder = self.folder();
        if folder.is_file() {
            return Some(format!("{} is already a file.", folder.display()));
        }
        // An existing folder is only refused when there is something in it: a person who made the
        // folder a moment ago in the file manager means that folder.
        if folder.is_dir()
            && folder.read_dir().map(|mut entries| entries.next().is_some()).unwrap_or(false)
        {
            return Some(format!("{} already has something in it.", folder.display()));
        }
        None
    }
}

/// What the dialog reported this frame.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Create was pressed, or Enter typed, and nothing refuses it.
    pub create: bool,
    /// Cancel, Escape or the close cross.
    pub cancelled: bool,
    /// The folder button was pressed, which is what opens the platform's folder picker.
    pub browse: bool,
}

/// Draw the dialog. The caller owns whether there is one at all.
pub fn show(ctx: &egui::Context, project: &mut NewProject) -> Outcome {
    let mut outcome = Outcome::default();
    let (inner, closed) = modal::show(ctx, "unluminous-new-project", WIDTH, HEIGHT, |ui, area| {
        let mut outcome = Outcome::default();
        if modal::header(ui, area, "Create Project") {
            outcome.cancelled = true;
        }
        let body = modal::body(area);
        let mut pen = body.top() + 6.0;

        let name_row = Row { label: "Name:", name: "Project name", browse: false };
        pen = row(ui, body, pen, name_row, &mut project.name, &mut outcome);
        pen += 12.0;
        let where_row = Row { label: "Location:", name: "Project location", browse: true };
        pen = row(ui, body, pen, where_row, &mut project.location, &mut outcome);

        // The line that makes the two fields add up to something. It says the folder rather than
        // repeating the two values, because the folder is the thing a person is deciding about.
        let said = match project.name.trim().is_empty() {
            true => "Project will be created in: —".to_owned(),
            false => format!("Project will be created in: {}", project.folder().display()),
        };
        pen = modal::note(ui, body, pen + 8.0, &said);

        let tick = Rect::from_min_size(
            Pos2::new(body.left() + LABEL, pen + 10.0),
            Vec2::new(body.width() - LABEL, 22.0),
        );
        modal::check(ui, tick, "Create Git repository", &mut project.git);
        pen = tick.bottom() + 6.0;

        // Why the last attempt was refused, in the one red the palette has — which is what the close
        // button and a breakpoint are drawn in, so this opens no colour of its own.
        if let Some(problem) = &project.problem {
            let painter = ui.painter().clone();
            modal::label(
                &painter,
                Rect::from_min_size(Pos2::new(body.left(), pen), Vec2::new(body.width(), 20.0)),
                body.left(),
                problem,
                color::close(),
                12.0,
            );
        }

        let ready = project.why_not().is_none();
        if let Some(pressed) = modal::footer(ui, area, &[("Cancel", true), ("Create", ready)]) {
            match pressed {
                0 => outcome.cancelled = true,
                _ => outcome.create = true,
            }
        }
        outcome
    });
    outcome.create |= inner.create;
    outcome.cancelled |= inner.cancelled || closed;
    outcome.browse |= inner.browse;
    outcome
}

/// How far in the fields start, which is how wide the widest label is.
const LABEL: f32 = 78.0;

/// What one row of the dialog is: the word in front of the field, and what the field is called.
///
/// A value rather than two more arguments, which is `explorer::View`'s reason said about a smaller list:
/// `row` had reached the length at which a caller starts passing them in the wrong order.
struct Row<'a> {
    /// The word drawn in front of the field.
    label: &'a str,
    /// What the field is called, which is what a test and an agent ask for.
    name: &'a str,
    /// Whether the folder button goes after it.
    browse: bool,
}

/// One labelled row: the word, the field, and — for the location — the folder button after it.
fn row(
    ui: &mut egui::Ui,
    body: Rect,
    top: f32,
    which: Row<'_>,
    value: &mut String,
    outcome: &mut Outcome,
) -> f32 {
    let Row { label, name, browse } = which;
    let line = Rect::from_min_size(Pos2::new(body.left(), top), Vec2::new(body.width(), 28.0));
    let painter = ui.painter().clone();
    modal::label(&painter, line, body.left(), label, color::text_dim(), 12.5);
    let after = match browse {
        true => 34.0,
        false => 0.0,
    };
    let field = Rect::from_min_max(
        Pos2::new(body.left() + LABEL, line.top()),
        Pos2::new(body.right() - after, line.bottom()),
    );
    modal::field(ui, field, name, value);
    if browse {
        let button = Rect::from_min_size(
            Pos2::new(field.right() + 6.0, line.top() + 2.0),
            Vec2::splat(24.0),
        );
        if crate::components::controls::icon_button(
            ui,
            button,
            "Choose a folder",
            crate::theme::icon::folder,
        ) {
            outcome.browse = true;
        }
    }
    line.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_dialog_starts_beside_the_project_that_is_open() {
        let folder = std::env::temp_dir().join("unluminous-new-project-beside");
        let project = folder.join("a-project");
        std::fs::create_dir_all(&project).expect("make the folder");
        let fresh = NewProject::beside(&project);
        assert_eq!(Path::new(fresh.location.trim()), folder, "beside it, not inside it");
        assert!(fresh.git, "a git repository is the usual answer, so it is the one already ticked");
        assert!(fresh.why_not().is_none(), "and a fresh dialog can be pressed straight away");
    }

    #[test]
    fn a_name_that_is_not_a_folder_name_is_refused_with_a_sentence() {
        let folder = std::env::temp_dir().join("unluminous-new-project-refusals");
        std::fs::create_dir_all(&folder).expect("make the folder");
        let at = folder.display().to_string();
        let one = |name: &str| NewProject {
            name: name.to_owned(),
            location: at.clone(),
            git: false,
            problem: None,
        };
        assert!(one("").why_not().is_some(), "an empty name");
        assert!(one("  ").why_not().is_some(), "and one that is only spaces");
        assert!(one("a/b").why_not().is_some(), "a path rather than a name");
        assert!(one("a\\b").why_not().is_some(), "on either platform's separator");
        assert!(one("..").why_not().is_some());
        assert!(one("ordinary").why_not().is_none());
    }

    #[test]
    fn a_location_that_is_not_there_and_a_folder_that_already_holds_something_are_both_refused() {
        let folder = std::env::temp_dir().join("unluminous-new-project-existing");
        std::fs::create_dir_all(folder.join("taken")).expect("make the folder");
        std::fs::write(folder.join("taken").join("a-file.txt"), "hello").expect("write");
        std::fs::create_dir_all(folder.join("empty")).expect("make the folder");
        let at = folder.display().to_string();
        let one = |name: &str, location: &str| NewProject {
            name: name.to_owned(),
            location: location.to_owned(),
            git: false,
            problem: None,
        };
        assert!(one("anything", "/no/such/folder/here").why_not().is_some());
        assert!(one("taken", &at).why_not().is_some(), "a folder with something in it");
        assert!(
            one("empty", &at).why_not().is_none(),
            "an empty folder somebody made a moment ago is the folder they meant"
        );
    }

    #[test]
    fn the_folder_is_the_location_and_the_name_joined() {
        let project = NewProject {
            name: "  spaced  ".to_owned(),
            location: "  /somewhere  ".to_owned(),
            git: true,
            problem: None,
        };
        assert_eq!(project.folder(), Path::new("/somewhere").join("spaced"));
    }
}
