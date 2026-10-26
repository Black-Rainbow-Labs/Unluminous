//! `Settings -> Word`: the name on comments and tracked changes, the zoom a document opens at, whether
//! formatting marks are shown, and whether spelling is checked as you type.
//!
//! Drawn with the Settings window's own cards (`settings_kit`), as every other page is (`task-2366`).

use crate::components::settings_kit::{self as kit, Column};
use crate::services::plugin_ui::{Look, Request};
use crate::services::word::Word;

pub fn show(word: &mut Word, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    let _ = look;
    kit::plugin_page(ui, "word-settings", |ui, column| cards(word, ui, column))
}

/// The page's two cards: the document and proofing.
fn cards(word: &mut Word, ui: &mut egui::Ui, column: &mut Column) -> Vec<Request> {
    let mut requests = Vec::new();
    let mut changed = false;
    let help = Some(
        "Microsoft Word documents open in tabs of their own, drawn as pages. These settings are kept in this \
         machine's settings folder.",
    );
    kit::card(ui, column, "Word", help, |card| {
        let field = word.author_field.get_or_insert_with(|| word.config.author.clone());
        card.row(
            "Author",
            Some("The name written on comments and tracked changes, and on a new document."),
            |row| {
                if row.text("Author", field, "Your name", 260.0) {
                    word.config.author = field.clone();
                    changed = true;
                }
            },
        );
        card.row("Open documents at", Some("The zoom a document opens at."), |row| {
            let mut zoom = word.config.zoom;
            if row.slider("Open documents at", &mut zoom, 0.25, 5.0) {
                word.config.zoom = (zoom * 20.0).round() / 20.0;
                changed = true;
            }
            row.suffix(&format!("{:.0}%", word.config.zoom * 100.0));
        });
        let mut marks = word.config.marks;
        if card.switch("Show formatting marks in a document that opens", None, &mut marks) {
            word.config.marks = marks;
            changed = true;
        }
    });
    let help = Some(
        "Misspelled and repeated words are underlined in red. Words added to the dictionary are kept in \
         words.dic in this machine's settings folder, and words added in Microsoft Word are known too.",
    );
    kit::card(ui, column, "Proofing", help, |card| {
        let mut spelling = word.config.spelling;
        if card.switch("Check spelling as you type", None, &mut spelling) {
            word.config.spelling = spelling;
            changed = true;
        }
    });
    if changed {
        for doc in &mut word.docs {
            doc.session.author = word.config.author.clone();
            doc.view.spelling = word.config.spelling;
            if !word.config.spelling {
                doc.view.spell_menu = None;
            }
        }
        if let Some(folder) = &word.folder {
            if let Err(problem) = word.config.write(folder) {
                requests.push(Request::Message(problem));
            }
        }
    }
    requests
}
