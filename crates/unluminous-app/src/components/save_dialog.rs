//! The Save dialog: a folder tree of the project, a name, and where the file will be written (`task-2336`).
//!
//! *"When I save the file, it should prompt me with a pop up that allows me to see a folder explorer, like our
//! pane, and let me choose which dir to save in. If I close the tab before saving where to save it, it should
//! prompt me with the pop up."*
//!
//! It is shown for a tab with no file: `Save` opens it, and so does closing such a tab with changes in it, in
//! which case it has a third answer, `Don't Save`. The tree lists folders only, drawn the way the explorer draws
//! its rows, and reads each folder's children the first time it is opened. The window holds a [`SaveDialog`]
//! and decides what an answer means, in `app::saving`, so the dialog itself writes nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use egui::{Pos2, Rect, Vec2};

use crate::components::modal;
use crate::theme::{color, icon, size};

/// The id the dialog's placement and zoom are kept under.
pub const ID: &str = "unluminous-save-dialog";

const WIDTH: f32 = 560.0;
const HEIGHT: f32 = 540.0;

/// What the window keeps while the dialog is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveDialog {
    /// The tab being saved, by index, and the name it had when the dialog opened, which is checked again
    /// before anything is written in case the tabs moved underneath.
    pub tab: usize,
    pub tab_name: String,
    /// The project folder, which is the top of the tree.
    pub root: PathBuf,
    /// The folder chosen, which the file is written into.
    pub folder: PathBuf,
    /// The file's name as typed.
    pub name: String,
    /// The folders opened in the tree.
    pub expanded: BTreeSet<PathBuf>,
    /// Each opened folder's own folders, read once.
    pub children: BTreeMap<PathBuf, Vec<PathBuf>>,
    /// Whether the dialog was opened by closing the tab, which adds `Don't Save`.
    pub closing: bool,
    /// Why the last press of Save did not save, shown under the name.
    pub problem: Option<String>,
    /// Whether the name field has been given the keyboard yet.
    pub focused: bool,
}

/// What was pressed this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Save,
    DontSave,
    Cancel,
}

impl SaveDialog {
    /// A dialog for the tab at `tab`, offering `name`, opened at the project folder.
    ///
    /// @param tab - the tab's index
    /// @param tab_name - what the tab is called now
    /// @param root - the project folder
    /// @param name - the name offered, the tab's own when it was given one
    /// @param closing - whether the tab is being closed
    pub fn new(tab: usize, tab_name: &str, root: &Path, name: &str, closing: bool) -> Self {
        let mut dialog = Self {
            tab,
            tab_name: tab_name.to_owned(),
            root: root.to_path_buf(),
            folder: root.to_path_buf(),
            name: name.to_owned(),
            expanded: BTreeSet::new(),
            children: BTreeMap::new(),
            closing,
            problem: None,
            focused: false,
        };
        dialog.open(root);
        dialog
    }

    /// Where the file will be written.
    pub fn target(&self) -> PathBuf {
        self.folder.join(self.name.trim())
    }

    /// Open a folder in the tree, reading its folders the first time.
    pub fn open(&mut self, folder: &Path) {
        if !self.children.contains_key(folder) {
            self.children.insert(folder.to_path_buf(), folders_in(folder));
        }
        self.expanded.insert(folder.to_path_buf());
    }

    /// Choose a folder, opening every folder above it so the row can be seen. A folder outside the project
    /// is refused, because the tree only reaches what is inside it.
    pub fn choose(&mut self, folder: &Path) -> Result<(), String> {
        if !folder.starts_with(&self.root) || !folder.is_dir() {
            return Err(format!("{} is not a folder in this project.", folder.display()));
        }
        let mut above = folder.parent();
        let mut chain = Vec::new();
        while let Some(step) = above {
            if !step.starts_with(&self.root) {
                break;
            }
            chain.push(step.to_path_buf());
            above = step.parent();
        }
        for step in chain.into_iter().rev() {
            self.open(&step);
        }
        self.folder = folder.to_path_buf();
        self.problem = None;
        Ok(())
    }

    /// Why Save cannot be pressed with what is in the dialog now, or `None` when it can.
    pub fn refusal(&self) -> Option<String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Some("Type a name for the file.".to_owned());
        }
        if name.contains(['/', '\\']) || name == "." || name == ".." {
            return Some(
                "A name is one file name, with no folder in it; choose the folder above."
                    .to_owned(),
            );
        }
        if self.target().exists() {
            return Some(format!("{name} is already in that folder. Type another name."));
        }
        None
    }

    /// The rows the tree shows, top to bottom: each folder with how deep it is.
    pub fn rows(&self) -> Vec<(PathBuf, usize)> {
        let mut rows = vec![(self.root.clone(), 0)];
        self.rows_under(&self.root, 1, &mut rows);
        rows
    }

    fn rows_under(&self, folder: &Path, depth: usize, rows: &mut Vec<(PathBuf, usize)>) {
        if !self.expanded.contains(folder) {
            return;
        }
        for child in self.children.get(folder).map(Vec::as_slice).unwrap_or_default() {
            rows.push((child.clone(), depth));
            self.rows_under(child, depth + 1, rows);
        }
    }
}

/// The folders directly inside `folder`, sorted the way the explorer sorts them, without `.git`.
fn folders_in(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| entry.file_name() != ".git")
        .map(|entry| entry.path())
        .collect();
    folders.sort_by_key(|path| {
        path.file_name().map(|name| name.to_string_lossy().to_lowercase()).unwrap_or_default()
    });
    folders
}

/// Draw the dialog. Answers what was pressed, if anything.
///
/// @param ctx - the window's context
/// @param dialog - the dialog's state
pub fn show(ctx: &egui::Context, dialog: &mut SaveDialog) -> Option<Answer> {
    let mut answer = None;
    let title = if dialog.closing { "Save Changes?" } else { "Save As" };
    let (_, closed) = modal::show(ctx, ID, WIDTH, HEIGHT, |ui, area| {
        if modal::header(ui, area, title) {
            answer = Some(Answer::Cancel);
        }
        let body = modal::body(area);
        let note = match dialog.closing {
            true => format!(
                "{} has changes that are not saved in any file. Choose a folder and a name to save it, or close it without saving.",
                dialog.tab_name
            ),
            false => format!("Choose the folder to save {} in, and its name.", dialog.tab_name),
        };
        let mut top = modal::note(ui, body, body.top(), &note);
        top = modal::section(ui, body, top, "Folder");
        let name_top = body.bottom() - 92.0;
        let tree = Rect::from_min_max(
            Pos2::new(body.left(), top + 4.0),
            Pos2::new(body.right(), name_top - 14.0),
        );
        show_the_tree(ui, tree, dialog);
        show_the_name(ui, body, name_top, dialog);
        let refusal = dialog.refusal();
        let buttons: Vec<(&str, bool)> = match dialog.closing {
            true => vec![("Cancel", true), ("Don't Save", true), ("Save", refusal.is_none())],
            false => vec![("Cancel", true), ("Save", refusal.is_none())],
        };
        let pressed = modal::footer(ui, area, &buttons);
        answer = match (pressed, dialog.closing) {
            (Some(0), _) => Some(Answer::Cancel),
            (Some(1), true) => Some(Answer::DontSave),
            (Some(_), _) => Some(Answer::Save),
            (None, _) => answer,
        };
    });
    if closed {
        answer = Some(Answer::Cancel);
    }
    answer
}

/// The folder tree, in a well that scrolls.
fn show_the_tree(ui: &mut egui::Ui, area: Rect, dialog: &mut SaveDialog) {
    modal::well(ui, area);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area.shrink(6.0)));
    child.set_clip_rect(area.shrink(2.0));
    let mut clicked: Option<PathBuf> = None;
    let mut toggled: Option<PathBuf> = None;
    egui::ScrollArea::vertical().id_salt("save-dialog-folders").show(&mut child, |ui| {
        for (folder, depth) in dialog.rows() {
            let name = match depth {
                0 => folder
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| folder.display().to_string()),
                _ => {
                    folder.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                }
            };
            let open = dialog.expanded.contains(&folder);
            let chosen = dialog.folder == folder;
            let label = name.clone();
            let mut arrow = Rect::NOTHING;
            let response =
                modal::row(ui, ("save-folder", &folder), &name, chosen, |painter, row| {
                    let x = row.left() + 14.0 + depth as f32 * size::INDENT;
                    let tint = if open { color::folder_open() } else { color::folder() };
                    icon::disclosure_at(painter, Pos2::new(x, row.center().y), open, tint, 1.0);
                    arrow = Rect::from_center_size(Pos2::new(x, row.center().y), Vec2::splat(18.0));
                    let mark = icon::folder_mark_width(1.0);
                    let mut left = x + 12.0;
                    if mark > 0.0 {
                        icon::folder_mark(
                            painter,
                            Pos2::new(left + mark / 2.0, row.center().y),
                            open,
                            tint,
                            1.0,
                        );
                        left += mark + 6.0;
                    }
                    modal::label(painter, row, left, &label, color::text_control(), 12.5);
                });
            if response.clicked() {
                let on_the_arrow =
                    response.interact_pointer_pos().is_some_and(|at| arrow.contains(at));
                if on_the_arrow {
                    toggled = Some(folder.clone());
                } else {
                    clicked = Some(folder.clone());
                }
            }
            if response.double_clicked() {
                toggled = Some(folder.clone());
            }
        }
    });
    if let Some(folder) = toggled {
        match dialog.expanded.contains(&folder) {
            true => {
                dialog.expanded.remove(&folder);
            }
            false => dialog.open(&folder),
        }
    }
    if let Some(folder) = clicked {
        dialog.folder = folder.clone();
        dialog.open(&folder);
        dialog.problem = None;
    }
}

/// The name field, and under it where the file will go or why it cannot.
fn show_the_name(ui: &mut egui::Ui, body: Rect, top: f32, dialog: &mut SaveDialog) {
    let field = Rect::from_min_size(Pos2::new(body.left(), top), Vec2::new(body.width(), 30.0));
    let entry = modal::field(ui, field, "Name", &mut dialog.name);
    // The name has the keyboard when the dialog opens, with its stem chosen so typing replaces it and keeps
    // the extension.
    if !dialog.focused {
        entry.request_focus();
        select_the_stem(ui.ctx(), entry.id, &dialog.name);
        dialog.focused = true;
    }
    if entry.changed() {
        dialog.problem = None;
    }
    let shown = match (&dialog.problem, dialog.refusal()) {
        (Some(problem), _) => (problem.clone(), color::close()),
        (None, Some(refusal)) if !dialog.name.trim().is_empty() => (refusal, color::close()),
        _ => {
            let target = dialog.target();
            let shown = target.strip_prefix(&dialog.root).map(Path::to_path_buf).unwrap_or(target);
            let root = dialog
                .root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            (
                format!("Saves as {root}/{}", shown.to_string_lossy().replace('\\', "/")),
                color::text_dim(),
            )
        }
    };
    let painter = ui.painter();
    let line = Rect::from_min_size(
        Pos2::new(body.left() + 14.0, field.bottom() + 6.0),
        Vec2::new(body.width(), 18.0),
    );
    modal::label(painter, line, line.left(), &shown.0, shown.1, 11.5);
}

/// Choose everything in the field up to the extension, the way a Save dialog does.
fn select_the_stem(ctx: &egui::Context, id: egui::Id, name: &str) {
    let stem = match name.rfind('.') {
        Some(at) if at > 0 => name[..at].chars().count(),
        _ => name.chars().count(),
    };
    let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    let range =
        egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(stem));
    state.cursor.set_char_range(Some(range));
    state.store(ctx, id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("unluminous-save-dialog-{name}"));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("src/ui")).unwrap();
        std::fs::create_dir_all(folder.join("docs")).unwrap();
        std::fs::create_dir_all(folder.join(".git/objects")).unwrap();
        std::fs::write(folder.join("docs/taken.md"), "").unwrap();
        folder
    }

    #[test]
    fn the_tree_lists_folders_only_and_opens_down_to_a_chosen_one() {
        let root = scratch("tree");
        let mut dialog = SaveDialog::new(0, "notes.md", &root, "notes.md", false);
        let names = |dialog: &SaveDialog| -> Vec<(String, usize)> {
            dialog
                .rows()
                .into_iter()
                .map(|(path, depth)| {
                    (path.file_name().unwrap().to_string_lossy().to_string(), depth)
                })
                .collect()
        };
        let top = root.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(names(&dialog), vec![(top.clone(), 0), ("docs".into(), 1), ("src".into(), 1)]);
        dialog.choose(&root.join("src/ui")).unwrap();
        assert_eq!(
            names(&dialog),
            vec![(top, 0), ("docs".into(), 1), ("src".into(), 1), ("ui".into(), 2)]
        );
        assert_eq!(dialog.target(), root.join("src/ui/notes.md"));
        assert!(dialog.choose(&std::env::temp_dir()).is_err(), "outside the project");
    }

    #[test]
    fn save_is_refused_for_no_name_a_path_and_a_file_that_is_there() {
        let root = scratch("refusals");
        let mut dialog = SaveDialog::new(0, "taken.md", &root, "taken.md", true);
        assert_eq!(dialog.refusal(), None);
        dialog.choose(&root.join("docs")).unwrap();
        assert!(dialog.refusal().unwrap().contains("already in that folder"));
        dialog.name = "  ".to_owned();
        assert!(dialog.refusal().is_some());
        dialog.name = "a/b.md".to_owned();
        assert!(dialog.refusal().unwrap().contains("no folder in it"));
    }
}
