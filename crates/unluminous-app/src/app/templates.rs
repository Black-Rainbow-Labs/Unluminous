//! New files from templates, and new tabs with no file behind them (`task-2336`).
//!
//! The explorer's `New` submenu makes a file from a template, and the editor's `New Tab` submenu opens a tab
//! holding one. Both ask for a name, then for each variable the template asks for, and then expand it through
//! `services::file_templates`. The command line reaches the same functions: `explorer new-file --template`
//! is [`UnluminousApp::make_a_file_from_a_template`] and `tab new` is [`UnluminousApp::open_an_untitled_tab`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use unluminous_core::{Command, Document};

use crate::app::files::OpenFile;
use crate::app::UnluminousApp;
use crate::components::prompt_dialog::{Made, Prompt, Purpose};
use crate::components::templates_dialog::{self, Answer, TemplatesDialog};
use crate::services::file_templates::{self, Template};

impl UnluminousApp {
    /// Unluminous's settings folder, which a person's own templates are kept under.
    pub(crate) fn templates_settings_folder(&self) -> Option<PathBuf> {
        self.store.as_ref().map(|store| store.folder().to_path_buf())
    }

    /// Every template there is for this project: the built in types, the person's own, and the project's.
    pub fn file_templates(&self) -> Vec<Template> {
        file_templates::all(self.tree.root(), self.templates_settings_folder().as_deref())
    }

    /// The names of the templates that are not one of the built in types, which the menus list.
    pub(crate) fn custom_template_names(&self) -> Vec<String> {
        self.file_templates()
            .into_iter()
            .filter(|template| !template.is_a_common_type())
            .map(|template| template.name)
            .collect()
    }

    /// The template a menu entry or a command names, or why there is none.
    fn template_called(&self, name: &str) -> Result<Template, String> {
        let templates = self.file_templates();
        file_templates::find(&templates, name).cloned().ok_or_else(|| {
            let known: Vec<String> =
                templates.iter().map(|template| template.name.clone()).collect();
            format!("There is no template called {name}. There are: {}.", known.join(", "))
        })
    }

    /// `New -> <type>` on the explorer's menu: ask for the file's name, with the type's extension added.
    ///
    /// @param folder - the folder the file goes in
    /// @param template - the template's name
    pub(crate) fn ask_for_a_file_from_a_template(&mut self, folder: PathBuf, template: &str) {
        let template = match self.template_called(template) {
            Ok(template) => template,
            Err(problem) => {
                self.message = Some(problem);
                return;
            }
        };
        // A file name pattern that names the file without `${NAME}` needs nothing typed. `task-2336`.
        if !file_templates::asks_for_a_name(&template) {
            let target = folder.join(file_templates::file_name_for(&template, ""));
            self.go_on_with_a_template(Made::File(target), template, Vec::new());
            return;
        }
        let place = crate::services::paths::the_useful_end_of(&folder.display().to_string());
        let note = match template.extension.is_empty() {
            true => format!("A new {} in {place}.", template.name),
            false => format!(
                "A new {} in {place}. .{} is added when it is not typed.",
                template.name, template.extension
            ),
        };
        self.prompt = Some(Prompt::new(
            &format!("New {}", template.name),
            &note,
            "untitled",
            "Create",
            Purpose::NewFromTemplate { folder, template: template.name },
        ));
    }

    /// `New Tab -> <type>` on the editor's menu: ask for the tab's name.
    ///
    /// @param template - the template's name, or empty for a tab with nothing in it
    pub(crate) fn ask_for_an_untitled_tab(&mut self, template: &str) {
        let (title, note) = match template.is_empty() {
            true => (
                "New Untitled File".to_owned(),
                "A tab with no file. Name it with its type, such as notes.md or main.rs; Save asks where it goes."
                    .to_owned(),
            ),
            false => match self.template_called(template) {
                Ok(found) => (
                    format!("New {}", found.name),
                    format!("A tab with no file, holding a new {}. Save asks where it goes.", found.name),
                ),
                Err(problem) => {
                    self.message = Some(problem);
                    return;
                }
            },
        };
        self.prompt = Some(Prompt::new(
            &title,
            &note,
            "untitled",
            "Open",
            Purpose::NewUntitledTab(template.to_owned()),
        ));
    }

    /// `Save File as Template...`: ask what the template is called.
    ///
    /// @param file - the file it is made from
    pub(crate) fn ask_to_save_as_template(&mut self, file: PathBuf) {
        if !file.is_file() {
            self.message = Some("Right click a file to make a template from it.".to_owned());
            return;
        }
        let name =
            file.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
        self.prompt = Some(Prompt::new(
            "Save File as Template",
            "A template made from this file, listed under New -> Templates. It is kept with your settings, so every project has it.",
            &name,
            "Save",
            Purpose::SaveAsTemplate(file),
        ));
    }

    /// `Manage Templates...` and `New Template...`: open the Templates manager over every template there is, on
    /// a new one when `new` is set. `task-2336`: *"I want to be able to manage the templates, create new ones, see
    /// my existing ones, etc from options in that template menu."*
    ///
    /// @param new - start a new template in it
    /// @param chosen - the template to choose, when one is named
    pub(crate) fn open_the_template_manager(&mut self, new: bool, chosen: Option<&str>) {
        let drafts =
            file_templates::drafts(self.tree.root(), self.templates_settings_folder().as_deref());
        let mut dialog = TemplatesDialog::new(drafts);
        if let Some(name) = chosen {
            dialog.choose(name);
        }
        if new {
            dialog.add();
        }
        self.close_every_modal();
        self.template_manager = Some(dialog);
    }

    /// Draw the Templates manager when it is open, and act on its footer.
    pub(crate) fn show_the_template_manager(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.template_manager.take() else { return };
        match templates_dialog::show(ctx, &mut dialog) {
            None => self.template_manager = Some(dialog),
            Some(answer) => self.answer_the_template_manager(dialog, answer),
        }
    }

    /// What `Cancel`, `Apply` and `OK` do: nothing, write and stay open, write and close. A write that is refused
    /// leaves the manager open with the reason in it.
    ///
    /// @param dialog - the manager as it was answered
    /// @param answer - which button
    pub(crate) fn answer_the_template_manager(
        &mut self,
        mut dialog: TemplatesDialog,
        answer: Answer,
    ) {
        if answer == Answer::Cancel {
            return;
        }
        let settings = self.templates_settings_folder();
        match file_templates::apply(&dialog.drafts, self.tree.root(), settings.as_deref()) {
            Ok(changed) => {
                self.tree.reload();
                let said = match changed {
                    0 => "Nothing had changed.".to_owned(),
                    1 => "Saved one template.".to_owned(),
                    n => format!("Saved {n} templates."),
                };
                if answer == Answer::Apply {
                    let chosen = dialog.chosen.map(|at| dialog.drafts[at].template.name.clone());
                    let mut fresh = TemplatesDialog::new(file_templates::drafts(
                        self.tree.root(),
                        settings.as_deref(),
                    ));
                    fresh.show_the_tab(dialog.scope, dialog.include);
                    if let Some(name) = chosen {
                        fresh.choose(&name);
                    }
                    fresh.said = Some((said, false));
                    self.template_manager = Some(fresh);
                } else {
                    self.message = Some(said);
                }
            }
            Err(problem) => {
                dialog.said = Some((problem, true));
                self.template_manager = Some(dialog);
            }
        }
    }

    /// Do what a template prompt asked, now it has been answered. Answers whether the purpose was one of
    /// these, so `run_prompt` can leave every other purpose to its own arm.
    ///
    /// @param purpose - what the prompt was for
    /// @param value - what was typed, trimmed and not empty
    pub(crate) fn answer_a_template_prompt(&mut self, purpose: &Purpose, value: &str) -> bool {
        let outcome = match purpose.clone() {
            Purpose::NewFromTemplate { folder, template } => {
                self.template_or(&template).map(|found| {
                    let target = crate::services::file_clipboard::free_name(
                        &folder,
                        &file_templates::file_name_for(&found, value),
                    );
                    self.go_on_with_a_template(Made::File(target), found, Vec::new());
                })
            }
            Purpose::NewUntitledTab(template) if template.is_empty() => {
                self.open_an_untitled_tab(value, None, &[]).map(|_| ())
            }
            Purpose::NewUntitledTab(template) => self.template_or(&template).map(|found| {
                let name = file_templates::file_name_for(&found, value);
                self.go_on_with_a_template(Made::Tab(name), found, Vec::new());
            }),
            Purpose::TemplateVariable { made, template, mut given, asking, .. } => {
                given.push((asking, value.to_owned()));
                self.template_or(&template)
                    .map(|found| self.go_on_with_a_template(made, found, given))
            }
            Purpose::SaveAsTemplate(file) => {
                self.save_a_file_as_a_template(&file, value, false).map(|saved| {
                    self.message = Some(format!("Saved the template {}", saved.display()));
                })
            }
            _ => return false,
        };
        if let Err(problem) = outcome {
            self.message = Some(problem);
        }
        true
    }

    /// The template called `name`, or the reason there is none.
    fn template_or(&self, name: &str) -> Result<Template, String> {
        self.template_called(name)
    }

    /// Ask for the next variable the template needs, or make what it is for when none is left.
    fn go_on_with_a_template(
        &mut self,
        made: Made,
        template: Template,
        given: Vec<(String, String)>,
    ) {
        let includes = self.includes_for(&template);
        let mut remaining: Vec<String> =
            file_templates::variables_asked(&template.body, includes.as_deref())
                .into_iter()
                .filter(|name| !given.iter().any(|(asked, _)| asked == name))
                .collect();
        if !remaining.is_empty() {
            let asking = remaining.remove(0);
            let what = match &made {
                Made::File(path) => {
                    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                }
                Made::Tab(name) => name.clone(),
            };
            let title = asking.clone();
            let note = format!("The {} template asks for {asking}, for {what}.", template.name);
            self.prompt = Some(Prompt::new(
                &title,
                &note,
                "",
                if remaining.is_empty() { "Create" } else { "Next" },
                Purpose::TemplateVariable {
                    made,
                    template: template.name,
                    given,
                    asking,
                    remaining,
                },
            ));
            return;
        }
        let outcome = match made {
            Made::File(target) => {
                self.make_a_file_from_a_template(&target, &template.name, &given).map(|_| ())
            }
            Made::Tab(name) => {
                self.open_an_untitled_tab(&name, Some(&template.name), &given).map(|_| ())
            }
        };
        if let Err(problem) = outcome {
            self.message = Some(problem);
        }
    }

    /// The folder a template's `#parse` reads from.
    fn includes_for(&self, template: &Template) -> Option<PathBuf> {
        file_templates::includes_for(
            template.scope,
            self.tree.root(),
            self.templates_settings_folder().as_deref(),
        )
    }

    /// The template's text with every variable filled in, for a file that will be at `file`.
    fn expand_a_template(
        &self,
        template: &Template,
        file: &Path,
        given: &[(String, String)],
    ) -> Result<String, String> {
        let mut values: BTreeMap<String, String> =
            file_templates::standard_values(file, self.tree.root());
        values.extend(given.iter().cloned());
        let includes = self.includes_for(template);
        file_templates::expand(&template.body, &values, includes.as_deref())
    }

    /// Why a template cannot be expanded with only the values `given`: the first variable it asks for that was
    /// not given, in the words a command answers with. `None` when nothing is missing, or the template is not
    /// there, which the call that uses it says itself.
    ///
    /// @param template - the template's name or slug
    /// @param given - the values given
    pub(crate) fn a_template_variable_not_given(
        &self,
        template: &str,
        given: &[(String, String)],
    ) -> Option<String> {
        let found = self.template_called(template).ok()?;
        let includes = self.includes_for(&found);
        let missing: Vec<String> =
            file_templates::variables_asked(&found.body, includes.as_deref())
                .into_iter()
                .filter(|name| !given.iter().any(|(key, _)| key == name))
                .collect();
        (!missing.is_empty()).then(|| {
            let sets: Vec<String> = missing.iter().map(|name| format!("{name}=...")).collect();
            format!(
                "The {} template asks for {}. Give them with --set \"{}\".",
                found.name,
                missing.join(", "),
                sets.join(";")
            )
        })
    }

    /// Write a file made from a template, show it in the explorer and open it. What `New -> <type>` ends
    /// with, and what `explorer new-file --template` does with no dialog.
    ///
    /// A file that is already there is refused, because a template is for a new file.
    ///
    /// @param target - where the file goes
    /// @param template - the template's name or slug
    /// @param given - the values of the variables the template asks for
    pub fn make_a_file_from_a_template(
        &mut self,
        target: &Path,
        template: &str,
        given: &[(String, String)],
    ) -> Result<PathBuf, String> {
        let template = self.template_called(template)?;
        if target.exists() {
            return Err(format!("{} is already there.", target.display()));
        }
        let text = self.expand_a_template(&template, target, given)?;
        if let Some(folder) = target.parent() {
            std::fs::create_dir_all(folder).map_err(|problem| {
                format!("Unluminous could not make {}: {problem}", folder.display())
            })?;
        }
        crate::services::store::write_a_source_file(target, text.as_bytes()).map_err(
            |problem| format!("Unluminous could not make {}: {problem}", target.display()),
        )?;
        self.tree.reload();
        if let Some(folder) = target.parent() {
            self.tree.expand(folder);
        }
        let _ = self.open_path_permanently(target);
        self.the_project_changed_on_disk();
        Ok(target.to_path_buf())
    }

    /// Open a tab with no file, called `name`, holding a template expanded or nothing. What `New Tab` ends
    /// with, and what `tab new` does. Answers the tab's index.
    ///
    /// The text goes in as one edit, so the tab counts as changed and closing it asks where to save it, and
    /// one undo empties it.
    ///
    /// @param name - what the tab is going to be called, which decides its language
    /// @param template - the template's name or slug, or none for an empty tab
    /// @param given - the values of the variables the template asks for
    pub fn open_an_untitled_tab(
        &mut self,
        name: &str,
        template: Option<&str>,
        given: &[(String, String)],
    ) -> Result<usize, String> {
        let name = name.trim();
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err("A new tab's name is a file name, such as notes.md, with no folder in it."
                .to_owned());
        }
        let text = match template {
            Some(template) => {
                let template = self.template_called(template)?;
                let name = file_templates::file_name_for(&template, name);
                let body =
                    self.expand_a_template(&template, &self.tree.root().join(&name), given)?;
                return self.open_a_named_tab(&name, &body);
            }
            None => String::new(),
        };
        self.open_a_named_tab(name, &text)
    }

    /// Open the tab itself, in the pane with the keyboard, and give it the keyboard.
    fn open_a_named_tab(&mut self, name: &str, text: &str) -> Result<usize, String> {
        let mut document = Document::new();
        if !text.is_empty() {
            document.apply(Command::Insert(text.to_owned()));
            document.apply(Command::MoveDocumentStart { extend: false });
        }
        let mut file = OpenFile::new(document);
        file.proposed = Some(name.to_owned());
        let index = self.files.open_file(file, true);
        self.forget_layout();
        self.focus = crate::app::Focus::Editor;
        Ok(index)
    }

    /// Make a template from a file and keep it with the person's settings, or with the project.
    ///
    /// @param file - the file it is made from
    /// @param name - what the template is called
    /// @param project - keep it in the project's `.unluminous/templates` rather than with the settings
    pub fn save_a_file_as_a_template(
        &mut self,
        file: &Path,
        name: &str,
        project: bool,
    ) -> Result<PathBuf, String> {
        let folder = match project {
            true => file_templates::project_folder(self.tree.root()),
            false => {
                let settings = self
                    .templates_settings_folder()
                    .ok_or("Unluminous has no settings folder to keep templates in.")?;
                file_templates::user_folder(&settings)
            }
        };
        let saved = file_templates::save_as_template(file, name, &folder)?;
        if project {
            self.tree.reload();
        }
        Ok(saved)
    }
}
