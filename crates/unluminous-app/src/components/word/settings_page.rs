//! `Settings -> Word`: the name on comments and tracked changes, the zoom a document opens at, and whether
//! formatting marks are shown.

use egui::RichText;

use crate::services::plugin_ui::{Look, Request};
use crate::services::word::Word;

pub fn show(word: &mut Word, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    let _ = look;
    let mut requests = Vec::new();
    let mut changed = false;
    ui.label(RichText::new("Word").strong().size(18.0));
    ui.label(RichText::new("Microsoft Word documents open in tabs of their own, drawn as pages. These settings are kept in this machine's settings folder.").weak());
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label("Author");
        let field = word.author_field.get_or_insert_with(|| word.config.author.clone());
        let response =
            ui.add(egui::TextEdit::singleline(field).hint_text("Your name").desired_width(220.0));
        if response.lost_focus() && *field != word.config.author {
            word.config.author = field.clone();
            changed = true;
        }
    });
    ui.label(
        RichText::new("The name written on comments and tracked changes, and on a new document.")
            .weak(),
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Open documents at");
        let mut percent = word.config.zoom * 100.0;
        if ui
            .add(egui::DragValue::new(&mut percent).range(25.0..=500.0).speed(1.0).suffix("%"))
            .changed()
        {
            word.config.zoom = percent / 100.0;
            changed = true;
        }
    });
    if ui
        .checkbox(&mut word.config.marks, "Show formatting marks in a document that opens")
        .changed()
    {
        changed = true;
    }
    if changed {
        for doc in &mut word.docs {
            doc.session.author = word.config.author.clone();
        }
        if let Some(folder) = &word.folder {
            if let Err(problem) = word.config.write(folder) {
                requests.push(Request::Message(problem));
            }
        }
    }
    requests
}
