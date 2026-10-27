//! The right click menu over the pages (task-2359, TDD 5.5): what Word offers for the thing under the
//! pointer. Over text it is the clipboard, Font, Paragraph, the lists, Styles, a link, a comment and the
//! synonyms of the word; over a table cell, the rows, columns and cells; over a picture or a shape, how
//! text wraps round it, its size and place, crop, alt text and its order; over a header, a field or a
//! note reference, what those take.
//!
//! Every entry but the clipboard's is an id in the action table, so the menu runs exactly what the `/`
//! search, the panel and `plugins run word action <id>` run. `Shift+F10` opens it at the caret.

use egui::{Pos2, Rect};
use unluminous_word::model::{FIELD_BEGIN, FIELD_END};
use unluminous_word::position::{self, Pos, StoryId};

use crate::app::actions::{Action, Entry, Shortcut};
use crate::components::controls;
use crate::services::plugin_ui::{Look, Request};
use crate::services::word::{ContextMenu, Doc, MenuEntry};

/// A row that runs the action `id`.
fn item(label: &str, id: &str) -> MenuEntry {
    MenuEntry { label: label.to_owned(), id: id.to_owned(), children: Vec::new() }
}

/// A line between two groups.
fn line() -> MenuEntry {
    item("-", "")
}

/// A menu inside the menu, opened to the side.
fn group(label: &str, children: Vec<MenuEntry>) -> MenuEntry {
    MenuEntry { label: label.to_owned(), id: String::new(), children }
}

/// What the pointer was over when the menu was asked for.
///
/// Grouped the way Word for Mac groups it (`task-2383`): the clipboard first, then what belongs to the thing
/// under the pointer, then formatting, then links and comments, then the words that could stand in for the one
/// pressed on. A family of entries that differ by one word (the seven ways text wraps, the four places a row
/// or column goes, the synonyms) is one row that opens to the side, so the menu stays short enough to read.
fn entries(doc: &Doc, at: &Pos) -> Vec<MenuEntry> {
    let mut out = vec![item("Cut", "cut"), item("Copy", "copy"), item("Paste", "paste")];
    // A picture, a shape or a text box the press selected.
    if doc.session.selected_object().is_some() {
        out.push(line());
        out.push(group(
            "Wrap Text",
            vec![
                item("In Line with Text", "wrap-inline"),
                item("Square", "wrap-square"),
                item("Tight", "wrap-tight"),
                item("Through", "wrap-through"),
                item("Top and Bottom", "wrap-top-and-bottom"),
                item("Behind Text", "wrap-behind"),
                item("In Front of Text", "wrap-front"),
            ],
        ));
        out.push(group(
            "Arrange",
            vec![item("Bring Forward", "bring-forward"), item("Send Backward", "send-backward")],
        ));
        out.push(line());
        out.push(item("Size and Position...", "picture-properties"));
        if doc.session.object_info().is_some_and(|i| i.kind == "Picture") {
            out.push(item("Crop", "crop"));
            out.push(item("Reset Picture", "reset-picture"));
        }
        out.push(item("Edit Alt Text...", "alt-text"));
        return out;
    }
    if doc.view.pasted.is_some() {
        out.push(item("Paste Text Only", "paste-text-only"));
    }
    out.extend(what_is_under(doc, at));
    out.push(line());
    out.extend([item("Font...", "font-dialog"), item("Paragraph...", "paragraph-dialog")]);
    out.extend([
        item("Bullets", "bullets"),
        item("Numbering", "numbering"),
        item("Styles", "styles-level"),
    ]);
    out.push(line());
    match doc.session.hyperlink_at(at) {
        Some(_) => out.extend([
            item("Edit Hyperlink...", "edit-link"),
            item("Open Hyperlink", "open-link"),
            item("Copy Hyperlink", "copy-link"),
            item("Remove Hyperlink", "remove-link"),
        ]),
        None => out.push(item("Link...", "insert-link")),
    }
    if !matches!(at.story, StoryId::Header(_)) {
        out.push(item("New Comment", "comment"));
    }
    let synonyms = synonyms_of(&word_at(doc, at));
    if !synonyms.is_empty() {
        out.push(line());
        out.push(group(
            "Synonyms",
            synonyms.iter().map(|s| item(s, &format!("synonym:{s}"))).collect(),
        ));
    }
    out
}

/// The entries that belong to what is under the pointer: a header, a field, a note reference or a table
/// cell, each after a line of its own.
fn what_is_under(doc: &Doc, at: &Pos) -> Vec<MenuEntry> {
    let mut out = Vec::new();
    if let StoryId::Header(_) = at.story {
        out.push(line());
        out.extend([
            item("Remove Header or Footer", "remove-header"),
            item("Page Number", "page-number"),
            item("Close Header and Footer", "close-header"),
        ]);
    }
    if field_at(doc, at) {
        out.push(line());
        out.extend([
            item("Update Field", "update-field"),
            item("Toggle Field Codes", "field-codes"),
            item("Edit Field...", "edit-field"),
        ]);
    }
    if let Some(note) = doc.session.note_at(at) {
        let endnote = matches!(note, StoryId::Endnote(_));
        out.push(line());
        out.push(item(if endnote { "Go to Endnote" } else { "Go to Footnote" }, "go-to-note"));
        out.push(item(
            if endnote { "Convert to Footnote" } else { "Convert to Endnote" },
            "convert-note",
        ));
    }
    if doc.session.cell_at_caret().is_some() {
        out.push(line());
        out.push(group(
            "Insert Rows and Columns",
            vec![
                item("Rows Above", "insert-row-above"),
                item("Rows Below", "insert-row-below"),
                item("Columns to the Left", "insert-column-left"),
                item("Columns to the Right", "insert-column-right"),
            ],
        ));
        out.push(group(
            "Delete Rows and Columns",
            vec![
                item("Rows", "delete-row"),
                item("Columns", "delete-column"),
                item("Entire Table", "delete-table"),
            ],
        ));
        out.extend([item("Merge Cells", "merge-cells"), item("Split Cells", "split-cells")]);
        out.push(group(
            "Distribute",
            vec![
                item("Rows Evenly", "distribute-rows"),
                item("Columns Evenly", "distribute-columns"),
            ],
        ));
        out.extend([
            item("Borders and Shading...", "borders-and-shading"),
            item("Table Properties...", "table-properties"),
        ]);
    }
    out
}

/// The words a synonym list offers for `word`: the most common meaning's, eight at most, as Word's Synonyms
/// list offers. Every sense together put a rare or anatomical sense of a word beside its everyday one.
fn synonyms_of(word: &str) -> Vec<String> {
    if word.is_empty() {
        return Vec::new();
    }
    let meanings = unluminous_word::thesaurus::synonyms(word);
    let mut synonyms: Vec<String> =
        meanings.into_iter().map(|m| m.synonyms).find(|s| !s.is_empty()).unwrap_or_default();
    synonyms.dedup();
    synonyms.truncate(8);
    synonyms
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

/// How wide the menu is: room for `Columns to the Right` beside its chevron, and for `Paragraph...`
/// beside its chord.
const WIDTH: f32 = 260.0;

/// Draw the menu when it is open, and run what is chosen.
///
/// `task-2383` redrew it with the window's own menu parts: the raised card of `controls::menu_frame`, and
/// `controls::menu_rows`, which carves the row under the pointer, draws the lines as grooves, puts the
/// chord in a quiet column on the right and opens a family of entries to the side. It was egui's own
/// buttons, labels and separators before, which is the one menu in the window that looked like none of
/// the others.
pub fn show(
    ui: &mut egui::Ui,
    _look: &Look<'_>,
    doc: &mut Doc,
    area: Rect,
    requests: &mut Vec<Request>,
) {
    let Some(menu) = doc.view.context_menu.clone() else { return };
    let rows = menu_entries(doc, &menu.entries);
    let gap = ui.spacing().item_spacing.y;
    let height = rows
        .iter()
        .map(|e| match e {
            Entry::Separator => 11.0 + gap,
            _ => controls::MENU_ROW + gap,
        })
        .sum::<f32>()
        + 16.0;
    // Kept inside the pages, as Word keeps its menu inside its window.
    let mut at = menu.at;
    at.x = at.x.min(area.right() - WIDTH - 4.0).max(area.left());
    at.y = at.y.min(area.bottom() - height - 4.0).max(area.top());
    let popup = egui::Popup::new(
        egui::Id::new(("word-context-menu", &doc.key)),
        ui.ctx().clone(),
        at,
        ui.layer_id(),
    )
    .kind(egui::PopupKind::Menu)
    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
    .layout(egui::Layout::top_down_justified(egui::Align::Min))
    .frame(controls::menu_frame(ui))
    .width(WIDTH);
    let mut chosen: Option<String> = None;
    let mut close = false;
    if let Some(response) = popup.show(|ui| controls::menu_rows(ui, &rows, 0.0)) {
        super::overlays::remember_popup(ui, response.response.rect);
        close = response.response.should_close();
        if let Some(Action::PluginCommand { command, .. }) = response.inner {
            chosen = Some(command);
        }
    }
    if let Some(id) = chosen {
        doc.view.context_menu = None;
        run(doc, &id, &menu, requests);
    } else if close || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        doc.view.context_menu = None;
    }
}

/// The menu's rows as the window's menus hold theirs: each a plugin command with the chord that runs it
/// beside it, dimmed when it cannot be run now.
fn menu_entries(doc: &Doc, entries: &[MenuEntry]) -> Vec<Entry> {
    entries
        .iter()
        .map(|entry| match entry {
            e if e.label == "-" => Entry::Separator,
            e if !e.children.is_empty() => {
                Entry::Submenu { name: e.label.clone(), entries: menu_entries(doc, &e.children) }
            }
            e => Entry::Item {
                name: e.label.clone(),
                action: Action::PluginCommand { plugin: "word".to_owned(), command: e.id.clone() },
                shortcut: chord_of(&e.id),
                enabled: enabled(doc, &e.id),
                checked: false,
                keyboard: false,
            },
        })
        .collect()
}

/// The chord shown beside an entry: the clipboard's own, or the one Word's chord table gives its action.
fn chord_of(id: &str) -> Option<Shortcut> {
    match id {
        "cut" => Some(Shortcut::command(egui::Key::X)),
        "copy" => Some(Shortcut::command(egui::Key::C)),
        "paste" => Some(Shortcut::command(egui::Key::V)),
        id => super::keys::chord_of(id),
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
