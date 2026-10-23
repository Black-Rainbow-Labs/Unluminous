//! Saving a tab that has no file: the Save dialog's window half (`task-2336`).
//!
//! `Save` on a tab with no file opened nothing: it wrote `untitled.md` into the project folder. Closing such a
//! tab threw its text away and said so. Both now open `components::save_dialog`, and this is what its answers
//! do. `tab save-as <path>` is still the way to save one with no dialog, and it goes through
//! [`UnluminousApp::save_a_tab_with_no_file_as`] too.

use std::path::Path;

use crate::app::UnluminousApp;
use crate::components::save_dialog::{self, Answer, SaveDialog};

impl UnluminousApp {
    /// Open the Save dialog for the tab at `index`, offering the name it was given or `untitled` with the
    /// extension a plain tab is saved with.
    ///
    /// @param index - the tab
    /// @param closing - whether the tab is being closed, which adds `Don't Save`
    pub(crate) fn ask_where_to_save(&mut self, index: usize, closing: bool) {
        let Some(file) = self.files.get(index) else { return };
        let offered = file.proposed.clone().unwrap_or_else(|| "untitled.md".to_owned());
        let dialog = SaveDialog::new(index, &file.name(), self.tree.root(), &offered, closing);
        self.close_every_modal();
        self.save_dialog = Some(dialog);
    }

    /// Draw the Save dialog when it is open, and act on what was pressed.
    pub(crate) fn show_the_save_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.save_dialog.take() else { return };
        match save_dialog::show(ctx, &mut dialog) {
            None => self.save_dialog = Some(dialog),
            Some(answer) => {
                if let Err(problem) = self.answer_the_save_dialog(dialog, answer) {
                    self.message = Some(problem);
                }
            }
        }
    }

    /// Do what the Save dialog was answered with. A save that is refused leaves the dialog open with the
    /// reason under the name. Answers what was done, in words.
    ///
    /// @param dialog - the dialog as it was answered
    /// @param answer - which button
    pub fn answer_the_save_dialog(
        &mut self,
        mut dialog: SaveDialog,
        answer: Answer,
    ) -> Result<String, String> {
        let still_there = self
            .files
            .get(dialog.tab)
            .is_some_and(|file| file.path().is_none() && file.name() == dialog.tab_name);
        if !still_there {
            return Err(format!(
                "{} is no longer open, so there is nothing to save.",
                dialog.tab_name
            ));
        }
        match answer {
            Answer::Cancel => Ok("Cancelled".to_owned()),
            Answer::DontSave => {
                self.close_tab_without_saving(dialog.tab);
                Ok(format!("Closed {} without saving", dialog.tab_name))
            }
            Answer::Save => {
                if let Some(refusal) = dialog.refusal() {
                    dialog.problem = Some(refusal.clone());
                    self.save_dialog = Some(dialog);
                    return Err(refusal);
                }
                let target = dialog.target();
                if let Err(problem) = self.save_a_tab_with_no_file_as(dialog.tab, &target) {
                    dialog.problem = Some(problem.clone());
                    self.save_dialog = Some(dialog);
                    return Err(problem);
                }
                if dialog.closing {
                    self.close_tab_without_saving(dialog.tab);
                }
                let said = format!("Saved {}", target.display());
                self.message = Some(said.clone());
                Ok(said)
            }
        }
    }

    /// Write a tab to `target` and make that its file: the tab is named after it, the explorer shows it, and
    /// the next Save writes there with no question.
    ///
    /// @param index - the tab
    /// @param target - where it is written
    pub fn save_a_tab_with_no_file_as(
        &mut self,
        index: usize,
        target: &Path,
    ) -> Result<(), String> {
        if let Some(folder) = target.parent() {
            std::fs::create_dir_all(folder).map_err(|problem| {
                format!("Unluminous could not make {}: {problem}", folder.display())
            })?;
        }
        self.write_a_tab(index, Some(target)).map_err(|problem| {
            format!("Unluminous could not save {}: {problem}", target.display())
        })?;
        let file = self.files.at_mut(index);
        file.proposed = None;
        file.note_what_is_on_disk();
        file.coloured_revision = None;
        self.tree.reload();
        if let Some(folder) = target.parent() {
            self.tree.expand(folder);
        }
        self.the_project_changed_on_disk();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::app::UnluminousApp;
    use crate::components::save_dialog::Answer;

    /// A project with a `src` folder in it, and a window on it.
    fn a_window(name: &str) -> (std::path::PathBuf, UnluminousApp) {
        let folder = std::env::temp_dir().join(format!("unluminous-saving-{name}"));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("src")).expect("make the folder");
        std::fs::write(folder.join("src/taken.rs"), "").expect("write a file");
        (folder.clone(), UnluminousApp::new(&folder))
    }

    /// `task-2336`: a new Rust tab from the editor's menu has no file, is named and coloured by the name it
    /// was given, and Save asks where it goes rather than writing `untitled.md`.
    #[test]
    fn a_new_tab_is_named_and_saving_it_asks_where_and_writes_there() {
        let (folder, mut app) = a_window("save-asks");
        let index = app.open_an_untitled_tab("main", Some("Rust File"), &[]).expect("a tab");
        assert_eq!(app.files.at(index).name(), "main.rs");
        assert_eq!(app.files.at(index).path(), None);
        assert_eq!(app.files.at(index).language_path(), Some("main.rs".into()));
        app.save();
        assert!(!folder.join("untitled.md").exists(), "nothing is written without asking");
        let mut dialog = app.save_dialog.take().expect("Save opens the dialog");
        assert_eq!(dialog.name, "main.rs");
        assert!(!dialog.closing);
        dialog.choose(&folder.join("src")).expect("src is in the project");
        dialog.name = "taken.rs".to_owned();
        assert!(
            app.answer_the_save_dialog(dialog.clone(), Answer::Save).is_err(),
            "the name is taken"
        );
        let mut dialog = app.save_dialog.take().expect("a refused save leaves the dialog open");
        assert!(dialog.problem.is_some());
        dialog.name = "main.rs".to_owned();
        app.answer_the_save_dialog(dialog, Answer::Save).expect("saved");
        assert_eq!(app.files.at(index).path(), Some(folder.join("src/main.rs").as_path()));
        assert_eq!(app.files.at(index).proposed, None);
        assert!(folder.join("src/main.rs").is_file());
    }

    /// `task-2336`: closing a tab with no file and changes in it asks, `Don't Save` closes it without
    /// writing, and closing one with no changes asks nothing.
    #[test]
    fn closing_a_changed_tab_with_no_file_asks_and_dont_save_closes_it() {
        let (folder, mut app) = a_window("close-asks");
        let index = app.open_an_untitled_tab("page", Some("HTML File"), &[]).expect("a tab");
        assert!(app.files.at(index).document.is_modified(), "the template is a change");
        let tabs = app.files.len();
        app.close_tab(index);
        assert_eq!(app.files.len(), tabs, "the tab stays while the question is asked");
        let dialog = app.save_dialog.take().expect("closing asks");
        assert!(dialog.closing);
        app.answer_the_save_dialog(dialog, Answer::DontSave).expect("closed");
        assert!(app.files.iter().all(|file| file.name() != "page.html"));
        assert!(!folder.join("page.html").exists());
        let empty = app.open_an_untitled_tab("scratch.txt", None, &[]).expect("a tab");
        app.close_tab(empty);
        assert!(app.save_dialog.is_none(), "an empty tab closes without a question");
    }

    /// `task-2336`: a file made from a template is written, expanded, and refused when it is already there.
    #[test]
    fn a_file_from_a_template_is_written_and_a_variable_it_asks_for_is_needed() {
        let (folder, mut app) = a_window("from-template");
        let made = app
            .make_a_file_from_a_template(&folder.join("site/index.html"), "html-file", &[])
            .expect("made");
        let text = std::fs::read_to_string(&made).expect("read it back");
        assert!(text.contains("<title>index</title>"), "{text}");
        assert!(app.make_a_file_from_a_template(&made, "HTML File", &[]).is_err(), "already there");
        let templates = crate::services::file_templates::project_folder(&folder);
        std::fs::create_dir_all(&templates).expect("templates folder");
        std::fs::write(templates.join("Note.md.ft"), "# ${NAME} by ${Author}\n").expect("template");
        assert!(app.a_template_variable_not_given("Note", &[]).unwrap().contains("Author"));
        let given = [("Author".to_owned(), "Jason".to_owned())];
        assert_eq!(app.a_template_variable_not_given("note", &given), None);
        let note = app
            .make_a_file_from_a_template(&folder.join("today.md"), "note", &given)
            .expect("made");
        assert_eq!(std::fs::read_to_string(note).unwrap(), "# today by Jason\n");
        assert!(app.custom_template_names().contains(&"Note".to_owned()));
    }
}
