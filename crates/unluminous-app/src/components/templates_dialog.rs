//! The Templates manager: every file template and include, made, changed, copied, removed and put back
//! (`task-2336`).
//!
//! *"There should be a templates manager that allows me to create new templates, edit existing ones, etc with
//! variable name placement."* It is the reference editor's `File and Code Templates` page in a dialog: the
//! person's templates (which hold the built in ones) and the project's on two tabs, templates and includes on two
//! more, a list with `New`, `Copy`, `Remove` and `Reset`, and on the right the chosen one's name, extension, file
//! name pattern and text, with the variables a template can use listed under it. Pressing a variable puts it in
//! the text at the caret.
//!
//! Nothing is written until `Apply` or `OK`, so `Cancel` throws every change away. The dialog edits a list of
//! `file_templates::Draft` and the window writes it with `file_templates::apply`, which is also what
//! `unluminous-cli explorer template-set` writes with.

use egui::{Pos2, Rect, Vec2};

use crate::components::controls::TextBoxes;
use crate::components::modal;
use crate::services::file_templates::{self, Draft, Scope};
use crate::theme::color;

/// The id the dialog's placement and zoom are kept under.
pub const ID: &str = "unluminous-templates";

const WIDTH: f32 = 1000.0;
const HEIGHT: f32 = 700.0;
const LIST: f32 = 300.0;
const BODY_ID: &str = "templates-body";

/// What the window keeps while the manager is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplatesDialog {
    pub drafts: Vec<Draft>,
    /// The tab showing: the person's templates or the project's.
    pub scope: Scope,
    /// The tab showing: includes rather than templates.
    pub include: bool,
    /// The draft chosen, by its place in `drafts`.
    pub chosen: Option<usize>,
    /// Why the last `Apply` wrote nothing, or what it did.
    pub said: Option<(String, bool)>,
    /// Give the name field the keyboard on the next frame, after `New` or `Copy`.
    pub name_wants_the_keyboard: bool,
}

/// What was pressed in the footer this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Cancel,
    Apply,
    Ok,
}

impl TemplatesDialog {
    /// A manager over `drafts`, on the person's templates, with the first one chosen.
    ///
    /// @param drafts - what `file_templates::drafts` read
    pub fn new(drafts: Vec<Draft>) -> Self {
        let mut dialog = Self {
            drafts,
            scope: Scope::User,
            include: false,
            chosen: None,
            said: None,
            name_wants_the_keyboard: false,
        };
        dialog.chosen = dialog.showing().first().copied();
        dialog
    }

    /// The drafts the tabs showing hold, by their place in `drafts`, built in ones first.
    pub fn showing(&self) -> Vec<usize> {
        (0..self.drafts.len())
            .filter(|&at| {
                let draft = &self.drafts[at];
                let mine = matches!(draft.template.scope, Scope::User | Scope::BuiltIn);
                !draft.removed
                    && draft.include == self.include
                    && (mine == (self.scope == Scope::User))
            })
            .collect()
    }

    /// Move to a tab, choosing its first draft.
    ///
    /// @param scope - the person's or the project's
    /// @param include - includes rather than templates
    pub fn show_the_tab(&mut self, scope: Scope, include: bool) {
        self.scope = scope;
        self.include = include;
        self.chosen = self.showing().first().copied();
    }

    /// Choose the draft called `name` on any tab, moving to its tab. Answers whether there was one.
    ///
    /// @param name - its name, in any case
    pub fn choose(&mut self, name: &str) -> bool {
        let Some(at) = self.drafts.iter().position(|draft| {
            !draft.removed && draft.template.name.eq_ignore_ascii_case(name.trim())
        }) else {
            return false;
        };
        let draft = &self.drafts[at];
        self.scope = match draft.template.scope {
            Scope::Project => Scope::Project,
            _ => Scope::User,
        };
        self.include = draft.include;
        self.chosen = Some(at);
        true
    }

    /// `New`: an empty template or include on the tab showing, with a name no other has.
    pub fn add(&mut self) {
        let what = if self.include { "New Include" } else { "New Template" };
        let name = self.free_name(what);
        self.drafts.push(Draft::new(&name, self.scope, self.include));
        self.chosen = Some(self.drafts.len() - 1);
        self.name_wants_the_keyboard = true;
    }

    /// `Copy`: the chosen draft again, as one of the tab's own, called `<name> copy`.
    pub fn copy(&mut self) {
        let Some(at) = self.chosen else { return };
        let mut copy = self.drafts[at].clone();
        copy.template.name = self.free_name(&format!("{} copy", copy.template.name));
        copy.template.scope = self.scope;
        copy.file = None;
        self.drafts.push(copy);
        self.chosen = Some(self.drafts.len() - 1);
        self.name_wants_the_keyboard = true;
    }

    /// Whether `Remove` applies: to any draft but a built in template, which can only be reset.
    pub fn can_remove(&self) -> bool {
        self.chosen.is_some_and(|at| !self.drafts[at].is_built_in())
    }

    /// `Remove`: mark the chosen draft to be taken out, and choose the next one.
    pub fn remove(&mut self) {
        let Some(at) = self.chosen.filter(|_| self.can_remove()) else { return };
        self.drafts[at].removed = true;
        self.chosen = self.showing().first().copied();
    }

    /// Whether `Reset` applies: to a built in template that says something other than what ships.
    pub fn can_reset(&self) -> bool {
        self.chosen.is_some_and(|at| self.drafts[at].differs_from_the_original())
    }

    /// `Reset`: put the chosen built in template back as Unluminous ships it.
    pub fn reset(&mut self) {
        let Some(at) = self.chosen.filter(|_| self.can_reset()) else { return };
        if let Some(shipped) = file_templates::original(&self.drafts[at].template.name) {
            self.drafts[at].template = shipped;
        }
    }

    /// `name`, or `name 2`, `name 3` and so on, whichever no draft on this tab has.
    fn free_name(&self, name: &str) -> String {
        let taken = |wanted: &str| {
            self.showing()
                .iter()
                .any(|&at| self.drafts[at].template.name.eq_ignore_ascii_case(wanted))
        };
        (1..)
            .map(|n| if n == 1 { name.to_owned() } else { format!("{name} {n}") })
            .find(|candidate| !taken(candidate))
            .unwrap_or_else(|| name.to_owned())
    }
}

/// Draw the manager. Answers what was pressed in its footer, if anything.
///
/// @param ctx - the window's context
/// @param dialog - the manager's state
pub fn show(ctx: &egui::Context, dialog: &mut TemplatesDialog) -> Option<Answer> {
    let mut answer = None;
    let (_, closed) = modal::show(ctx, ID, WIDTH, HEIGHT, |ui, area| {
        if modal::header(ui, area, "File Templates") {
            answer = Some(Answer::Cancel);
        }
        let body = modal::body(area);
        let below = show_the_tabs(ui, body, dialog);
        let room = Rect::from_min_max(Pos2::new(body.left(), below), body.max);
        let list = Rect::from_min_max(room.min, Pos2::new(room.left() + LIST, room.bottom() - 6.0));
        show_the_list(ui, list, dialog);
        let detail = Rect::from_min_max(Pos2::new(list.right() + 22.0, room.top()), room.max);
        show_the_chosen(ui, detail, dialog);
        let refusal = file_templates::refusal(&dialog.drafts);
        let line = Rect::from_min_size(
            Pos2::new(area.left() + 30.0, area.bottom() - 44.0),
            Vec2::new(500.0, 18.0),
        );
        let said = refusal.clone().map(|why| (why, true)).or_else(|| dialog.said.clone());
        if let Some((words, problem)) = said {
            let tint = if problem { color::close() } else { color::text_dim() };
            modal::label(ui.painter(), line, line.left(), &words, tint, 11.5);
        }
        let ready = refusal.is_none();
        let pressed = modal::footer_confirmed_by(
            ui,
            area,
            &[("Cancel", true), ("Apply", ready), ("OK", ready)],
            modal::Confirm::CommandEnter,
        );
        answer = match pressed {
            Some(0) => Some(Answer::Cancel),
            Some(1) => Some(Answer::Apply),
            Some(_) => Some(Answer::Ok),
            None => answer,
        };
    });
    if closed {
        answer = Some(Answer::Cancel);
    }
    answer
}

/// The two pairs of tabs along the top. Answers where the room under them starts.
fn show_the_tabs(ui: &mut egui::Ui, body: Rect, dialog: &mut TemplatesDialog) -> f32 {
    let scope = if dialog.scope == Scope::Project { 1 } else { 0 };
    let (picked, track) = modal::tabs(
        ui,
        body.min,
        "Template scope",
        &["Yours, in every project", "This project only"],
        scope,
    );
    if let Some(picked) = picked {
        let scope = if picked == 1 { Scope::Project } else { Scope::User };
        dialog.show_the_tab(scope, dialog.include);
    }
    let at = Pos2::new(track.right() + 18.0, body.top());
    let (picked, _) =
        modal::tabs(ui, at, "Template kind", &["Templates", "Includes"], dialog.include as usize);
    if let Some(picked) = picked {
        dialog.show_the_tab(dialog.scope, picked == 1);
    }
    track.bottom() + 16.0
}

/// The toolbar and the list down the left.
fn show_the_list(ui: &mut egui::Ui, area: Rect, dialog: &mut TemplatesDialog) {
    let buttons = [
        ("New", true),
        ("Copy", dialog.chosen.is_some()),
        ("Remove", dialog.can_remove()),
        ("Reset", dialog.can_reset()),
    ];
    let width = (area.width() - 3.0 * 8.0) / 4.0;
    for (at, (name, enabled)) in buttons.iter().enumerate() {
        let rect = Rect::from_min_size(
            Pos2::new(area.left() + at as f32 * (width + 8.0), area.top()),
            Vec2::new(width, modal::BUTTON),
        );
        if modal::button(ui, rect, name, *enabled, false) {
            match at {
                0 => dialog.add(),
                1 => dialog.copy(),
                2 => dialog.remove(),
                _ => dialog.reset(),
            }
        }
    }
    let well =
        Rect::from_min_max(Pos2::new(area.left(), area.top() + modal::BUTTON + 12.0), area.max);
    modal::well(ui, well);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(well.shrink(6.0)));
    child.set_clip_rect(well.shrink(2.0));
    let mut chosen = None;
    egui::ScrollArea::vertical().id_salt("templates-list").show(&mut child, |ui| {
        for at in dialog.showing() {
            let draft = &dialog.drafts[at];
            let name = draft.template.name.clone();
            let extension = match draft.template.extension.is_empty() {
                true => String::new(),
                false => format!(".{}", draft.template.extension),
            };
            let tag = match (draft.is_built_in(), draft.differs_from_the_original()) {
                (true, true) => "changed",
                (true, false) => "built in",
                _ => "",
            };
            let label = format!("Template {name}");
            let response = modal::row(
                ui,
                ("template-row", at),
                &label,
                dialog.chosen == Some(at),
                |painter, row| {
                    let after = modal::label(
                        painter,
                        row,
                        row.left() + 14.0,
                        &name,
                        color::text_control(),
                        12.5,
                    );
                    modal::label(painter, row, after + 2.0, &extension, color::text_dim(), 12.5);
                    if !tag.is_empty() {
                        let width = painter
                            .layout_no_wrap(
                                tag.to_owned(),
                                egui::FontId::proportional(10.5),
                                color::text_faint(),
                            )
                            .size()
                            .x;
                        modal::label(
                            painter,
                            row,
                            row.right() - width - 14.0,
                            tag,
                            color::text_faint(),
                            10.5,
                        );
                    }
                },
            );
            if response.clicked() {
                chosen = Some(at);
            }
        }
        if dialog.showing().is_empty() {
            ui.add_space(8.0);
            let words = if dialog.include {
                "  No includes yet. Press New."
            } else {
                "  No templates yet. Press New."
            };
            ui.label(egui::RichText::new(words).size(11.5).color(color::text_faint()));
        }
    });
    if let Some(at) = chosen {
        dialog.chosen = Some(at);
    }
}

/// The chosen draft's fields, its text, and the variables under it.
fn show_the_chosen(ui: &mut egui::Ui, area: Rect, dialog: &mut TemplatesDialog) {
    let Some(at) = dialog.chosen else {
        let line = Rect::from_min_size(area.min, Vec2::new(area.width(), 20.0));
        modal::label(
            ui.painter(),
            line,
            line.left(),
            "Choose a template, or press New to make one.",
            color::text_dim(),
            12.5,
        );
        return;
    };
    let built_in = dialog.drafts[at].is_built_in();
    let include = dialog.drafts[at].include;
    let caption = |ui: &egui::Ui, x: f32, y: f32, words: &str| {
        let line = Rect::from_min_size(Pos2::new(x + 4.0, y), Vec2::new(300.0, 16.0));
        modal::label(ui.painter(), line, line.left(), words, color::text_dim(), 11.0);
    };
    let extension_width = 130.0;
    let name_rect = Rect::from_min_size(
        Pos2::new(area.left(), area.top() + 18.0),
        Vec2::new(area.width() - extension_width - 14.0, 30.0),
    );
    let extension_rect = Rect::from_min_size(
        Pos2::new(name_rect.right() + 14.0, name_rect.top()),
        Vec2::new(extension_width, 30.0),
    );
    caption(ui, name_rect.left(), area.top(), "Name");
    caption(ui, extension_rect.left(), area.top(), "Extension");
    let template = &mut dialog.drafts[at].template;
    if built_in {
        // A built in template keeps its name and type, which is what the menu lists it by; its text and its file
        // name pattern can change, and Reset puts them back.
        modal::well(ui, name_rect);
        modal::label(
            ui.painter(),
            name_rect,
            name_rect.left() + 14.0,
            &template.name,
            color::text_dim(),
            12.5,
        );
        modal::well(ui, extension_rect);
        modal::label(
            ui.painter(),
            extension_rect,
            extension_rect.left() + 14.0,
            &template.extension,
            color::text_dim(),
            12.5,
        );
    } else {
        let name = modal::field(ui, name_rect, "Template name", &mut template.name);
        if dialog.name_wants_the_keyboard {
            name.request_focus();
            dialog.name_wants_the_keyboard = false;
        }
        modal::field(ui, extension_rect, "Template extension", &mut template.extension);
    }
    let mut top = name_rect.bottom() + 14.0;
    if !include {
        caption(
            ui,
            area.left(),
            top,
            "File name, optional: ${NAME}_test, or ../${NAME}. Empty asks for a name.",
        );
        let rect =
            Rect::from_min_size(Pos2::new(area.left(), top + 18.0), Vec2::new(area.width(), 30.0));
        modal::field(ui, rect, "Template file name", &mut template.file_name);
        top = rect.bottom() + 14.0;
    }
    let variables_height = 172.0;
    caption(ui, area.left(), top, "Text");
    let text = Rect::from_min_max(
        Pos2::new(area.left(), top + 18.0),
        Pos2::new(area.right(), area.bottom() - variables_height - 30.0),
    );
    let body_id = ui.id().with(BODY_ID);
    show_the_text(ui, text, body_id, &mut template.body);
    let asked = file_templates::variables_asked(&template.body, None);
    let words = match asked.is_empty() {
        true => "Variables. Press one to put it in the text at the caret.".to_owned(),
        false => format!("Variables. Asked for when a file is made: {}.", asked.join(", ")),
    };
    caption(ui, area.left(), text.bottom() + 10.0, &words);
    let list = Rect::from_min_max(
        Pos2::new(area.left(), text.bottom() + 28.0),
        Pos2::new(area.right(), area.bottom() - 6.0),
    );
    if let Some(variable) = show_the_variables(ui, list) {
        insert_at_the_caret(
            ui.ctx(),
            body_id,
            &mut dialog.drafts[at].template.body,
            &format!("${{{variable}}}"),
        );
    }
}

/// The template's text, in the code face, in a well that scrolls.
fn show_the_text(ui: &mut egui::Ui, area: Rect, id: egui::Id, body: &mut String) {
    modal::well(ui, area);
    let inside = area.shrink2(Vec2::new(14.0, 10.0));
    crate::components::controls::claim_the_field(ui, area, id, "Template text field");
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inside));
    child.set_clip_rect(child.clip_rect().intersect(inside));
    egui::ScrollArea::vertical().id_salt("templates-body-scroll").auto_shrink([false, false]).show(
        &mut child,
        |ui| {
            let response = ui.text_box(
                egui::TextEdit::multiline(body)
                    .id(id)
                    .code_editor()
                    .font(egui::FontId::monospace(12.5))
                    .frame(egui::Frame::NONE)
                    .desired_width(inside.width())
                    .desired_rows(8)
                    .text_color(color::text_control()),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Template text")
            });
        },
    );
}

/// The variables a template can use, one a row with what it holds. Answers the one pressed.
fn show_the_variables(ui: &mut egui::Ui, area: Rect) -> Option<&'static str> {
    modal::well(ui, area);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area.shrink(6.0)));
    child.set_clip_rect(area.shrink(2.0));
    let mut pressed = None;
    egui::ScrollArea::vertical().id_salt("templates-variables").show(&mut child, |ui| {
        for (name, meaning) in file_templates::VARIABLES {
            let written = format!("${{{name}}}");
            let label = format!("Insert {written}");
            let response =
                modal::row(ui, ("template-variable", name), &label, false, |painter, row| {
                    let after = modal::label(
                        painter,
                        row,
                        row.left() + 14.0,
                        &written,
                        color::text_control(),
                        12.0,
                    );
                    modal::label(
                        painter,
                        row,
                        after.max(row.left() + 190.0),
                        meaning,
                        color::text_dim(),
                        11.5,
                    );
                });
            if response.clicked() {
                pressed = Some(name);
            }
        }
    });
    pressed
}

/// Put `words` into `body` where the text box's caret was, or at the end when it has none, and leave the caret
/// after them with the keyboard in the box.
fn insert_at_the_caret(ctx: &egui::Context, id: egui::Id, body: &mut String, words: &str) {
    let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    let characters = body.chars().count();
    let at = state
        .cursor
        .char_range()
        .map(|range| range.primary.index.0.min(characters))
        .unwrap_or(characters);
    let byte = body.char_indices().nth(at).map(|(byte, _)| byte).unwrap_or(body.len());
    body.insert_str(byte, words);
    let after = egui::text::CCursor::new(at + words.chars().count());
    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(after)));
    state.store(ctx, id);
    ctx.memory_mut(|memory| memory.request_focus(id));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_manager() -> TemplatesDialog {
        let mut drafts: Vec<Draft> = file_templates::built_in()
            .into_iter()
            .map(|template| Draft { template, include: false, file: None, removed: false })
            .collect();
        drafts.push(Draft::new("Card", Scope::Project, false));
        TemplatesDialog::new(drafts)
    }

    #[test]
    fn the_tabs_show_the_persons_templates_with_the_built_in_ones_and_the_projects_apart() {
        let mut dialog = a_manager();
        assert_eq!(dialog.showing().len(), file_templates::built_in().len());
        assert_eq!(dialog.chosen, Some(0));
        dialog.show_the_tab(Scope::Project, false);
        assert_eq!(dialog.showing(), vec![file_templates::built_in().len()]);
        dialog.show_the_tab(Scope::User, true);
        assert!(dialog.showing().is_empty() && dialog.chosen.is_none());
        assert!(dialog.choose("card"));
        assert_eq!(
            (dialog.scope, dialog.chosen),
            (Scope::Project, Some(file_templates::built_in().len()))
        );
    }

    #[test]
    fn new_copy_remove_and_reset_change_the_drafts_and_nothing_else() {
        let mut dialog = a_manager();
        assert!(!dialog.can_remove(), "a built in template is reset, not removed");
        assert!(!dialog.can_reset(), "nothing to reset yet");
        dialog.drafts[0].template.body = "// changed\n".to_owned();
        assert!(dialog.can_reset());
        dialog.reset();
        assert_eq!(dialog.drafts[0].template.body, "");
        dialog.copy();
        assert_eq!(dialog.drafts.last().unwrap().template.name, "Rust File copy");
        assert!(dialog.can_remove());
        dialog.add();
        dialog.add();
        let names: Vec<String> =
            dialog.drafts.iter().map(|draft| draft.template.name.clone()).collect();
        assert!(
            names.contains(&"New Template".to_owned())
                && names.contains(&"New Template 2".to_owned())
        );
        dialog.remove();
        assert!(dialog.drafts.last().unwrap().removed);
        assert!(dialog.showing().iter().all(|&at| !dialog.drafts[at].removed));
    }
}
