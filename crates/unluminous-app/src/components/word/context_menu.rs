//! The right click menu over the pages (task-2359, TDD 5.5): what Word offers for the thing under the
//! pointer. Over text it is the clipboard, Font, Paragraph, the lists, Styles, a link, a comment and the
//! synonyms of the word; over a table cell, the rows, columns and cells; over a picture or a shape, how
//! text wraps round it, its size and place, crop, alt text and its order; over a header, a field or a
//! note reference, what those take.
//!
//! Every entry but the clipboard's is an id in the action table, so the menu runs exactly what the `/`
//! search, the panel and `plugins run word action <id>` run. `Shift+F10` opens it at the caret.

use egui::{Pos2, Rect, Vec2};
use unluminous_word::model::{FIELD_BEGIN, FIELD_END};
use unluminous_word::position::{self, Pos, StoryId};

use crate::services::plugin_ui::{Look, Request};
use crate::services::word::{ContextMenu, Doc, MenuEntry};

/// What the pointer was over when the menu was asked for.
fn entries(doc: &Doc, at: &Pos) -> Vec<MenuEntry> {
    let entry = |label: &str, id: &str| MenuEntry { label: label.to_owned(), id: id.to_owned() };
    let line = || entry("-", "");
    let mut out = Vec::new();
    // A picture, a shape or a text box the press selected.
    if doc.session.selected_object().is_some() {
        out.extend([entry("Cut", "cut"), entry("Copy", "copy"), entry("Paste", "paste"), line()]);
        for (label, id) in [
            ("Wrap Text: In Line with Text", "wrap-inline"),
            ("Wrap Text: Square", "wrap-square"),
            ("Wrap Text: Tight", "wrap-tight"),
            ("Wrap Text: Through", "wrap-through"),
            ("Wrap Text: Top and Bottom", "wrap-top-and-bottom"),
            ("Wrap Text: Behind Text", "wrap-behind"),
            ("Wrap Text: In Front of Text", "wrap-front"),
        ] {
            out.push(entry(label, id));
        }
        out.push(line());
        out.push(entry("Size and Position...", "picture-properties"));
        if doc.session.object_info().is_some_and(|i| i.kind == "Picture") {
            out.push(entry("Crop", "crop"));
            out.push(entry("Reset Picture", "reset-picture"));
        }
        out.push(entry("Edit Alt Text...", "alt-text"));
        out.extend([
            entry("Bring Forward", "bring-forward"),
            entry("Send Backward", "send-backward"),
        ]);
        return out;
    }
    out.extend([entry("Cut", "cut"), entry("Copy", "copy"), entry("Paste", "paste")]);
    if doc.view.pasted.is_some() {
        out.push(entry("Paste Options: Keep Text Only", "paste-text-only"));
    }
    out.push(line());
    if let StoryId::Header(_) = at.story {
        out.extend([
            entry("Remove Header or Footer", "remove-header"),
            entry("Page Number", "page-number"),
        ]);
        out.push(entry("Close Header and Footer", "close-header"));
        out.push(line());
    }
    if field_at(doc, at) {
        out.extend([
            entry("Update Field", "update-field"),
            entry("Toggle Field Codes", "field-codes"),
            entry("Edit Field...", "edit-field"),
            line(),
        ]);
    }
    if doc.session.note_at(at).is_some() {
        let endnote = matches!(doc.session.note_at(at), Some(StoryId::Endnote(_)));
        out.push(entry(if endnote { "Go to Endnote" } else { "Go to Footnote" }, "go-to-note"));
        out.push(entry(
            if endnote { "Convert to Footnote" } else { "Convert to Endnote" },
            "convert-note",
        ));
        out.push(line());
    }
    if doc.session.cell_at_caret().is_some() {
        for (label, id) in [
            ("Insert Rows Above", "insert-row-above"),
            ("Insert Rows Below", "insert-row-below"),
            ("Insert Columns to the Left", "insert-column-left"),
            ("Insert Columns to the Right", "insert-column-right"),
            ("Delete Rows", "delete-row"),
            ("Delete Columns", "delete-column"),
            ("Delete Table", "delete-table"),
            ("Merge Cells", "merge-cells"),
            ("Split Cells", "split-cells"),
            ("Distribute Rows Evenly", "distribute-rows"),
            ("Distribute Columns Evenly", "distribute-columns"),
            ("Borders and Shading...", "borders-and-shading"),
            ("Table Properties...", "table-properties"),
        ] {
            out.push(entry(label, id));
        }
        out.push(line());
    }
    out.extend([entry("Font...", "font-dialog"), entry("Paragraph...", "paragraph-dialog")]);
    out.extend([
        entry("Bullets", "bullets"),
        entry("Numbering", "numbering"),
        entry("Styles", "styles-level"),
    ]);
    match doc.session.hyperlink_at(at) {
        Some(_) => out.extend([
            entry("Edit Hyperlink...", "edit-link"),
            entry("Open Hyperlink", "open-link"),
            entry("Copy Hyperlink", "copy-link"),
            entry("Remove Hyperlink", "remove-link"),
        ]),
        None => out.push(entry("Link...", "insert-link")),
    }
    if !matches!(at.story, StoryId::Header(_)) {
        out.push(entry("New Comment", "comment"));
    }
    let word = word_at(doc, at);
    if !word.is_empty() {
        let meanings = unluminous_word::thesaurus::synonyms(&word);
        // The most common meaning's words, as Word's Synonyms list offers: every sense together put a
        // rare or anatomical sense of a word beside its everyday one.
        let mut synonyms: Vec<String> =
            meanings.into_iter().map(|m| m.synonyms).find(|s| !s.is_empty()).unwrap_or_default();
        synonyms.dedup();
        if !synonyms.is_empty() {
            out.push(line());
            out.push(entry(&format!("Synonyms of {word}"), ""));
            for synonym in synonyms.into_iter().take(8) {
                out.push(MenuEntry {
                    label: format!("    {synonym}"),
                    id: format!("synonym:{synonym}"),
                });
            }
        }
    }
    out
}

/// Whether `at` is inside a field: between a field's begin and end, or in a simple field.
fn field_at(doc: &Doc, at: &Pos) -> bool {
    let Some(paragraph) = position::paragraph(&doc.session.document, &at.story, &at.path) else {
        return false;
    };
    let mut depth = 0i32;
    let mut offset = 0;
    for run in &paragraph.runs {
        if offset >= at.offset && depth > 0 {
            return true;
        }
        if offset < at.offset + 1
            && offset + run.len() >= at.offset
            && run.wraps.iter().any(|w| w.el.name == "w:fldSimple")
        {
            return true;
        }
        match run.content.character() {
            Some(FIELD_BEGIN) => depth += 1,
            Some(FIELD_END) => depth -= 1,
            _ => {}
        }
        offset += run.len();
        if offset > at.offset {
            return depth > 0;
        }
    }
    false
}

/// The word around `at`, letters only.
fn word_at(doc: &Doc, at: &Pos) -> String {
    let Some(paragraph) = position::paragraph(&doc.session.document, &at.story, &at.path) else {
        return String::new();
    };
    let text = paragraph.flat_text();
    let offset = at.offset.min(text.len());
    let is_letter = |c: char| c.is_alphabetic() || c == '\'';
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_letter(*c))
        .last()
        .map_or(offset, |(i, _)| i);
    let end = text[offset..]
        .char_indices()
        .find(|(_, c)| !is_letter(*c))
        .map_or(text.len(), |(i, _)| offset + i);
    text[start..end].to_owned()
}

/// Open the menu at a screen point for what is at `at`, putting the caret there first unless the press
/// was inside the selection, as Word does.
pub fn open(doc: &mut Doc, screen: Pos2, at: Pos) {
    doc.view.slash = None;
    doc.view.spell_menu = None;
    if doc.session.selected_object().is_none() {
        let (from, to) = doc.session.selection.ordered();
        let inside = at.story == from.story && *from <= at && at <= *to;
        if !inside {
            doc.session.select(at.clone(), at.clone());
        }
    }
    let entries = entries(doc, &at);
    doc.view.context_menu =
        Some(ContextMenu { at: screen, entries, word: word_at(doc, &at), pos: at });
}

/// `Shift+F10` and the menu key: the menu at the caret.
pub fn open_at_caret(doc: &mut Doc, area: Rect) {
    let pos = doc.session.selection.head.clone();
    let point = super::page_view::caret_point(doc, area).unwrap_or(area.center());
    open(doc, point, pos);
}

/// Draw the menu when it is open, and run what is chosen.
pub fn show(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    area: Rect,
    requests: &mut Vec<Request>,
) {
    let Some(menu) = doc.view.context_menu.clone() else { return };
    // Sized as the window's own menus are, by their row height, not by the document's font: a large editor
    // font made every row of this menu taller than the lines of the page it was opened over.
    let row = look.menu_row_height;
    let text = row * 0.48;
    let width = row * 8.5;
    let height =
        menu.entries.iter().map(|e| if e.label == "-" { 7.0 } else { row }).sum::<f32>() + 8.0;
    let mut at = menu.at;
    at.x = at.x.min(area.right() - width - 4.0).max(area.left());
    at.y = at.y.min(area.bottom() - height - 4.0).max(area.top());
    let rect = Rect::from_min_size(at, Vec2::new(width, height));
    super::overlays::remember_popup(ui, rect);
    let mut chosen: Option<String> = None;
    egui::Area::new(ui.id().with(("word-context-menu", &doc.key)))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            crate::components::controls::menu_frame(ui).show(ui, |ui| {
                ui.set_width(width - 16.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                for entry in &menu.entries {
                    if entry.label == "-" {
                        ui.separator();
                        continue;
                    }
                    if entry.id.is_empty() {
                        ui.label(egui::RichText::new(&entry.label).weak().size(text * 0.92));
                        continue;
                    }
                    let enabled = enabled(doc, &entry.id);
                    let button = egui::Button::new(egui::RichText::new(&entry.label).size(text))
                        .frame(false)
                        .min_size(Vec2::new(width - 16.0, row));
                    let response = ui.add_enabled(enabled, button);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &entry.label)
                    });
                    if response.clicked() {
                        chosen = Some(entry.id.clone());
                    }
                }
            });
        });
    let pressed_outside = ui.input(|i| i.pointer.any_pressed())
        && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !rect.contains(p));
    if let Some(id) = chosen {
        doc.view.context_menu = None;
        run(doc, &id, &menu, requests);
    } else if pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        doc.view.context_menu = None;
    }
}

/// Whether an entry can be run now.
fn enabled(doc: &Doc, id: &str) -> bool {
    match id {
        "cut" | "copy" => !doc.session.selection.is_empty(),
        id if id.starts_with("synonym:") => true,
        "paste" | "open-link" | "copy-link" | "paste-text-only" => true,
        id => crate::services::word::actions::applies(id, doc),
    }
}

/// Run an entry: the clipboard ones here, a synonym by replacing the word, and every other through the
/// action table.
pub fn run(doc: &mut Doc, id: &str, menu: &ContextMenu, requests: &mut Vec<Request>) {
    match id {
        "cut" => {
            super::keys::copy(doc, requests);
            if !doc.session.selection.is_empty() {
                doc.session.backspace(unluminous_word::edit::Unit::Character);
            }
        }
        "copy" => super::keys::copy(doc, requests),
        "paste" => crate::services::word::ask_for_the_clipboard(doc, String::new(), requests),
        "open-link" => {
            if let Some((address, _)) = doc.session.hyperlink_at(&menu.pos) {
                super::page_view::follow_link(doc, &address, requests);
            }
        }
        "copy-link" => {
            if let Some((address, _)) = doc.session.hyperlink_at(&menu.pos) {
                requests.push(Request::CopyRich {
                    text: address.clone(),
                    html: String::new(),
                    rtf: String::new(),
                });
            }
        }
        "paste-text-only" => {
            let _ =
                crate::services::word::paste_again(doc, unluminous_word::clipboard::PasteAs::Text);
        }
        id if id.starts_with("synonym:") => {
            let synonym = id.trim_start_matches("synonym:").to_owned();
            replace_word(doc, &menu.pos, &menu.word, &synonym);
        }
        id => super::run_action(doc, id, requests),
    }
}

/// Put `with` in place of the word `word` round `at`.
fn replace_word(doc: &mut Doc, at: &Pos, word: &str, with: &str) {
    let Some(paragraph) = position::paragraph(&doc.session.document, &at.story, &at.path) else {
        return;
    };
    let text = paragraph.flat_text();
    let offset = at.offset.min(text.len());
    let start = (0..=offset).rev().find(|&i| {
        text.is_char_boundary(i) && text[i..].starts_with(word) && i + word.len() >= offset
    });
    if let Some(start) = start {
        doc.session.select(at.at(start), at.at(start + word.len()));
        doc.session.type_text(with);
    }
}
