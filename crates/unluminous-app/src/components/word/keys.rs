//! The keyboard in a Word tab: typing, the caret, the selection, and Word's own chords.
//!
//! Typing arrives as `egui::Event::Text` and an input method's commit; the clipboard as `Copy`, `Cut`
//! and `Paste`. The chords listed in [`claims`] are left in the frame by the window's menus while a Word
//! tab holds the keyboard (`UiProvider::claims_key`), so `Ctrl+B` makes text bold here rather than doing
//! whatever a keymap gave it to.

use egui::{Event, Key, Modifiers};
use unluminous_word::edit::{self, Unit};
use unluminous_word::model::{COLUMN_BREAK, LINE_BREAK, PAGE_BREAK};
use unluminous_word::position::{self, Pos, Selection, StoryId};

use crate::services::plugin_ui::Request;
use crate::services::word::Doc;

/// The chords a Word tab answers itself: Word's own formatting, editing and navigation keys.
pub fn claims(key: Key, modifiers: &Modifiers) -> bool {
    if !modifiers.command {
        return matches!(key, Key::Tab | Key::F4 | Key::F7 | Key::F9 | Key::F11 | Key::Escape);
    }
    // Saving, closing a tab, quitting and switching tabs stay the window's.
    !matches!(key, Key::S | Key::W | Key::Q | Key::O | Key::N | Key::Tab | Key::P | Key::Comma)
        || (key == Key::N && modifiers.shift)
}

/// Every key event this frame.
pub fn handle(ui: &egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    let events = ui.input(|i| i.events.clone());
    for event in events {
        match event {
            Event::Text(text) => type_text(doc, &text),
            Event::Ime(egui::ImeEvent::Commit(text)) => type_text(doc, &text),
            Event::Copy => copy(doc, requests),
            Event::Cut => {
                copy(doc, requests);
                if !doc.session.selection.is_empty() {
                    doc.session.backspace(Unit::Character);
                }
            }
            Event::Paste(text) => {
                // A paste of exactly what a Word tab copied keeps its formatting. Anything else asks the
                // window for the clipboard's HTML and RTF, and pastes when it answers; Ctrl+Shift+V pastes
                // the text alone, which is what Word gives that chord.
                let text = text.replace("\r\n", "\n");
                let only_text = ui.input(|i| i.modifiers.shift);
                match copied_fragment(&text) {
                    Some(fragment) if !only_text => {
                        let at = doc.session.selection.ordered().0.clone();
                        doc.session.paste_fragment(fragment.clone());
                        crate::services::word::remember_the_paste(doc, fragment, text, at);
                    }
                    _ if only_text => doc.session.type_text(&text),
                    _ => crate::services::word::ask_for_the_clipboard(doc, text, requests),
                }
                doc.view.reveal = true;
            }
            Event::Key { key, pressed: true, modifiers, .. } => {
                key_press(doc, key, &modifiers, requests)
            }
            _ => {}
        }
    }
}

/// What a Word tab last copied, as paragraphs with their formatting, beside the text that went on the
/// clipboard. Kept for the whole window, so a copy from one document pastes into another formatted.
static COPIED: std::sync::Mutex<Option<(String, Vec<unluminous_word::model::Paragraph>)>> =
    std::sync::Mutex::new(None);

pub(crate) fn copy(doc: &Doc, requests: &mut Vec<Request>) {
    if doc.session.selection.is_empty() {
        return;
    }
    let (text, fragment) = doc.session.copy();
    let html = unluminous_word::export::fragment_html(&doc.session.document, &fragment);
    let rtf = unluminous_word::clipboard::to_rtf(&doc.session.document, &fragment);
    if let Ok(mut slot) = COPIED.lock() {
        *slot = Some((text.clone(), fragment));
    }
    requests.push(Request::CopyRich { text, html, rtf });
}

/// The paragraphs a Word tab copied, when the clipboard still holds exactly their text.
fn copied_fragment(text: &str) -> Option<Vec<unluminous_word::model::Paragraph>> {
    let slot = COPIED.lock().ok()?;
    let (copied, fragment) = slot.as_ref()?;
    (copied.replace("\r\n", "\n") == text).then(|| fragment.clone())
}

/// Type as a keyboard does, for a test that drives the Markdown habits.
#[cfg(test)]
pub fn type_for_test(doc: &mut Doc, text: &str) {
    type_text(doc, text);
}

/// Typed text, with the `/` menu, the slash menu's filter, and Markdown habits.
fn type_text(doc: &mut Doc, text: &str) {
    // While the `/` menu is open, typing filters it.
    if let Some((_, query, chosen)) = doc.view.slash.as_mut() {
        if text.chars().all(|c| !c.is_whitespace()) {
            query.push_str(text);
            *chosen = 0;
            doc.session.type_text(text);
            return;
        }
        doc.view.slash = None;
    }
    if doc.view.palette.is_some() {
        return;
    }
    let head = doc.session.selection.head.clone();
    let paragraph = position::paragraph(&doc.session.document, &head.story, &head.path);
    let before = paragraph.map(|p| p.flat_text()).unwrap_or_default();
    let at_start = doc.session.selection.is_empty()
        && before[..head.offset.min(before.len())].trim().is_empty();
    if text == "/" && at_start && before.trim().is_empty() {
        doc.session.type_text("/");
        doc.view.slash = Some((head, String::new(), 0));
        return;
    }
    doc.session.type_text(text);
    doc.view.reveal = true;
    if text == " " && markdown_habit(doc) {
        return;
    }
    // Autocorrect after the key that ends a word, as a step of its own that Ctrl+Z gives back.
    if doc.session.autocorrect(text).is_some() {
        doc.view.reveal = true;
    }
}

/// `# `, `## `, `- `, `* `, `1. ` and `> ` at the start of a paragraph become a heading, a list or a quote,
/// and `---` then Enter a line. Each is its own undo step, so `Ctrl+Z` gives the characters back.
fn markdown_habit(doc: &mut Doc) -> bool {
    let head = doc.session.selection.head.clone();
    let Some(paragraph) = position::paragraph(&doc.session.document, &head.story, &head.path)
    else {
        return false;
    };
    let text = paragraph.flat_text();
    let typed = &text[..head.offset.min(text.len())];
    let action = match typed {
        "# " => Some("heading-1"),
        "## " => Some("heading-2"),
        "### " => Some("heading-3"),
        "#### " => Some("heading-4"),
        "- " | "* " => Some("bullets"),
        "1. " | "1) " => Some("numbering"),
        "> " => Some("quote"),
        _ => None,
    };
    let Some(action) = action else { return false };
    let marker = typed.len();
    doc.session.select(head.at(0), head.at(marker));
    doc.session.backspace(Unit::Character);
    let mut ignored = Vec::new();
    super::run_action(doc, action, &mut ignored);
    true
}

fn key_press(doc: &mut Doc, key: Key, modifiers: &Modifiers, requests: &mut Vec<Request>) {
    let command = modifiers.command;
    let shift = modifiers.shift;
    // The `/` menu and the search take the arrows and Enter while open.
    if super::overlays::takes_key(doc, key, requests) {
        return;
    }
    if command {
        let id = match (key, shift) {
            // Word's heading chords are Ctrl+Alt with the number, and come before Ctrl+1 and Ctrl+2.
            (Key::Num1, _) if modifiers.alt => Some("heading-1"),
            (Key::Num2, _) if modifiers.alt => Some("heading-2"),
            (Key::Num3, _) if modifiers.alt => Some("heading-3"),
            // Ctrl+F9 opens the Insert level at its field box, where a code is typed and inserted.
            (Key::F9, false) => {
                doc.view.panel =
                    vec![crate::services::word::Level::Root, crate::services::word::Level::Insert];
                doc.view.fields.insert("field", String::new());
                None
            }
            (Key::F9, true) => Some("unlink-fields"),
            (Key::F11, false) => Some("lock-fields"),
            (Key::F11, true) => Some("unlock-fields"),
            (Key::B, false) => Some("bold"),
            (Key::I, false) => Some("italic"),
            (Key::U, false) => Some("underline"),
            (Key::E, false) => Some("align-center"),
            (Key::L, false) => Some("align-left"),
            (Key::R, false) => Some("align-right"),
            (Key::J, false) => Some("justify"),
            (Key::Num1, false) => Some("line-single"),
            (Key::Num2, false) => Some("line-double"),
            (Key::Num5, false) => Some("line-15"),
            (Key::M, false) => Some("indent-more"),
            (Key::M, true) => Some("indent-less"),
            (Key::Z, false) => Some("undo"),
            (Key::Z, true) | (Key::Y, false) => Some("redo"),
            (Key::A, false) => Some("select-all"),
            (Key::F, false) => Some("find"),
            (Key::G, false) => {
                doc.view.goto = Some(String::new());
                None
            }
            (Key::H, false) => Some("replace"),
            (Key::Space, false) => Some("clear-formatting"),
            (Key::N, true) => Some("normal"),
            (Key::Period, true) | (Key::CloseBracket, false) => Some("grow-font"),
            (Key::Comma, true) | (Key::OpenBracket, false) => Some("shrink-font"),
            (Key::Equals, false) => Some("subscript"),
            (Key::Equals, true) | (Key::Plus, _) => Some("superscript"),
            (Key::Num8, true) => Some("formatting-marks"),
            (Key::C, true) => Some("copy-formatting"),
            (Key::V, true) => Some("paste-formatting"),
            (Key::K, false) => {
                doc.view.panel =
                    vec![crate::services::word::Level::Root, crate::services::word::Level::Insert];
                doc.view.fields.insert("link", String::new());
                None
            }
            (Key::Slash, _) => {
                doc.view.palette = Some((String::new(), 0));
                None
            }
            (Key::Enter, false) => {
                doc.session.insert_break(PAGE_BREAK);
                None
            }
            (Key::Enter, true) => {
                doc.session.insert_break(COLUMN_BREAK);
                None
            }
            (Key::Backspace, _) => {
                doc.session.backspace(Unit::Word);
                None
            }
            (Key::Delete, _) => {
                doc.session.delete(Unit::Word);
                None
            }
            (Key::Home, _) => {
                move_to(doc, story_edge(doc, false), shift);
                None
            }
            (Key::End, _) => {
                move_to(doc, story_edge(doc, true), shift);
                None
            }
            (Key::ArrowLeft, _) => {
                move_to(doc, word_step(doc, false), shift);
                None
            }
            (Key::ArrowRight, _) => {
                move_to(doc, word_step(doc, true), shift);
                None
            }
            (Key::ArrowUp, _) => {
                move_to(doc, paragraph_step(doc, false), shift);
                None
            }
            (Key::ArrowDown, _) => {
                move_to(doc, paragraph_step(doc, true), shift);
                None
            }
            _ => None,
        };
        if let Some(id) = id {
            super::run_action(doc, id, requests);
        }
        doc.view.reveal = true;
        return;
    }
    match key {
        Key::Enter if shift => doc.session.insert_break(LINE_BREAK),
        Key::Enter => {
            // `---` and Enter makes a line, the last of the Markdown habits.
            let head = doc.session.selection.head.clone();
            let text = position::paragraph(&doc.session.document, &head.story, &head.path)
                .map(|p| p.flat_text())
                .unwrap_or_default();
            if text == "---" && head.offset == 3 {
                doc.session.select(head.at(0), head.at(3));
                doc.session.backspace(Unit::Character);
                super::run_action(doc, "horizontal-line", requests);
            } else if !doc.session.border_line_on_enter() {
                doc.session.autocorrect_before_enter();
            }
            doc.session.enter();
        }
        Key::Backspace => doc.session.backspace(Unit::Character),
        Key::Delete => doc.session.delete(Unit::Character),
        Key::Tab => {
            // Tab in a list changes the level, in a table moves to the next cell, and otherwise types a tab.
            if doc.session.change_list_level(!shift) {
            } else if let Some((table, row, cell)) = doc.session.cell_at_caret() {
                next_cell(doc, table, row, cell, !shift);
            } else if !shift {
                doc.session.type_text("\t");
            }
        }
        Key::Escape if doc.view.spell_menu.is_some() => doc.view.spell_menu = None,
        Key::Escape => {
            doc.view.slash = None;
            doc.view.palette = None;
            if doc.view.find.take().is_none() && !doc.view.panel.is_empty() {
                doc.view.panel.clear();
            }
            if doc.session.selection.head.story != StoryId::Body {
                // Escape leaves a header or a note, back to the body.
                let first = position::paragraphs(&doc.session.document.body)
                    .first()
                    .map(|(p, _)| p.clone())
                    .unwrap_or_default();
                doc.session.select(Pos::new(first.clone(), 0), Pos::new(first, 0));
            }
        }
        Key::F7 => super::run_action(doc, "spelling", requests),
        Key::F4 => {
            if let Some(id) = doc.view.last_action {
                super::run_action(doc, id, requests);
            }
        }
        // Alt+F9 and Shift+F9 show the field codes instead of the results, F9 updates the fields.
        Key::F9 if modifiers.alt || shift => super::run_action(doc, "field-codes", requests),
        Key::F9 => super::run_action(doc, "update-fields", requests),
        Key::ArrowLeft => move_to(doc, char_step(doc, false), shift),
        Key::ArrowRight => move_to(doc, char_step(doc, true), shift),
        Key::ArrowUp => {
            let to = line_step(doc, false);
            move_to(doc, to, shift);
        }
        Key::ArrowDown => {
            let to = line_step(doc, true);
            move_to(doc, to, shift);
        }
        Key::Home => move_to(doc, line_edge(doc, false), shift),
        Key::End => move_to(doc, line_edge(doc, true), shift),
        Key::PageUp => {
            let to = screen_step(doc, false);
            move_to(doc, to, shift);
        }
        Key::PageDown => {
            let to = screen_step(doc, true);
            move_to(doc, to, shift);
        }
        _ => return,
    }
    doc.view.reveal = true;
}

/// Put the caret at `to`, or extend the selection to it with Shift. A move with nothing selected and no
/// Shift collapses a selection to the end it moved towards, which is what Word does with Left and Right.
fn move_to(doc: &mut Doc, to: Option<Pos>, extend: bool) {
    let Some(to) = to else { return };
    if extend {
        let anchor = doc.session.selection.anchor.clone();
        if anchor.story == to.story {
            doc.session.select(anchor, to);
        }
    } else {
        doc.session.select(to.clone(), to);
    }
}

/// Every offset the caret may stand at in a paragraph, from the layout, in order.
fn stops(doc: &Doc, pos: &Pos) -> Vec<usize> {
    let mut stops: Vec<usize> = doc
        .pages
        .pages
        .iter()
        .flat_map(|p| p.carets.iter())
        .filter(|l| l.story == pos.story && l.path == pos.path)
        .flat_map(|l| l.stops.iter().map(|s| s.0))
        .collect();
    stops.sort_unstable();
    stops.dedup();
    stops
}

fn char_step(doc: &Doc, forward: bool) -> Option<Pos> {
    let selection = &doc.session.selection;
    if !selection.is_empty() {
        let (from, to) = selection.ordered();
        return Some(if forward { to.clone() } else { from.clone() });
    }
    let head = &selection.head;
    let blocks = position::story(&doc.session.document, &head.story)?;
    let stops = stops(doc, head);
    let next = if forward {
        stops.iter().find(|s| **s > head.offset).copied()
    } else {
        stops.iter().rev().find(|s| **s < head.offset).copied()
    };
    if let Some(offset) = next {
        return Some(head.at(offset));
    }
    if stops.is_empty() {
        let text = position::paragraph(&doc.session.document, &head.story, &head.path)?.flat_text();
        if forward && head.offset < text.len() {
            return Some(head.at(edit::next_boundary(&text, head.offset)));
        }
        if !forward && head.offset > 0 {
            return Some(head.at(edit::previous_boundary(&text, head.offset)));
        }
    }
    // Across the paragraph mark.
    if forward {
        let next = position::next_paragraph(blocks, &head.path)?;
        Some(Pos { story: head.story.clone(), path: next, offset: 0 })
    } else {
        let previous = position::previous_paragraph(blocks, &head.path)?;
        let len = position::paragraph(&doc.session.document, &head.story, &previous)?.len();
        Some(Pos { story: head.story.clone(), path: previous, offset: len })
    }
}

fn word_step(doc: &Doc, forward: bool) -> Option<Pos> {
    let head = &doc.session.selection.head;
    let text = position::paragraph(&doc.session.document, &head.story, &head.path)?.flat_text();
    if forward && head.offset >= text.len() || !forward && head.offset == 0 {
        // At a paragraph's end, a word step crosses the paragraph mark, which a character step does.
        let blocks = position::story(&doc.session.document, &head.story)?;
        return if forward {
            Some(Pos {
                story: head.story.clone(),
                path: position::next_paragraph(blocks, &head.path)?,
                offset: 0,
            })
        } else {
            let previous = position::previous_paragraph(blocks, &head.path)?;
            let len = position::paragraph(&doc.session.document, &head.story, &previous)?.len();
            Some(Pos { story: head.story.clone(), path: previous, offset: len })
        };
    }
    Some(head.at(if forward {
        edit::next_word(&text, head.offset)
    } else {
        edit::previous_word(&text, head.offset)
    }))
}

fn paragraph_step(doc: &Doc, forward: bool) -> Option<Pos> {
    let head = &doc.session.selection.head;
    let blocks = position::story(&doc.session.document, &head.story)?;
    if !forward && head.offset > 0 {
        return Some(head.at(0));
    }
    let path = if forward {
        position::next_paragraph(blocks, &head.path)?
    } else {
        position::previous_paragraph(blocks, &head.path)?
    };
    Some(Pos { story: head.story.clone(), path, offset: 0 })
}

fn story_edge(doc: &Doc, end: bool) -> Option<Pos> {
    let story = doc.session.selection.head.story.clone();
    let all = position::paragraphs(position::story(&doc.session.document, &story)?);
    let (path, paragraph) = if end { all.last()? } else { all.first()? };
    Some(Pos { story, path: path.clone(), offset: if end { paragraph.len() } else { 0 } })
}

/// The caret's line in the layout, and its page.
fn current_line(doc: &Doc) -> Option<(usize, &unluminous_word::layout::CaretLine)> {
    doc.pages.find(&doc.session.selection.head)
}

fn line_edge(doc: &Doc, end: bool) -> Option<Pos> {
    let head = &doc.session.selection.head;
    let (_, line) = current_line(doc)?;
    let offset = if end {
        // Before the space a line wrapped at, so the caret shows at this line's end rather than the next's start.
        if line.last {
            line.end
        } else {
            line.stops.iter().rev().nth(1).map_or(line.end, |s| s.0).max(line.start)
        }
    } else {
        line.start
    };
    Some(head.at(offset))
}

/// Up or Down: the line above or below at the x the caret keeps.
fn line_step(doc: &mut Doc, down: bool) -> Option<Pos> {
    let head = doc.session.selection.head.clone();
    let (page, middle, x) = {
        let (page, line) = current_line(doc)?;
        (page, line.top + line.height / 2.0, line.x_of(head.offset))
    };
    let goal = doc.view.goal_x.unwrap_or(x);
    doc.view.goal_x = Some(goal);
    let mut best: Option<(f32, usize, &unluminous_word::layout::CaretLine)> = None;
    for (index, p) in doc.pages.pages.iter().enumerate() {
        if (down && index < page) || (!down && index > page) {
            continue;
        }
        for candidate in p.carets.iter().filter(|l| l.story == head.story) {
            let centre = candidate.top + candidate.height / 2.0;
            let further = if index == page {
                if down {
                    centre > middle + 0.5
                } else {
                    centre < middle - 0.5
                }
            } else {
                true
            };
            if !further {
                continue;
            }
            let left = candidate.stops.first().map_or(0.0, |s| s.1);
            let right = candidate.stops.last().map_or(0.0, |s| s.1);
            let dx = if goal < left {
                left - goal
            } else if goal > right {
                goal - right
            } else {
                0.0
            };
            let dy = if index == page {
                (centre - middle).abs()
            } else if down {
                10_000.0 * (index - page) as f32 + candidate.top
            } else {
                10_000.0 * (page - index) as f32 + (2000.0 - candidate.top)
            };
            let score = dy + dx * 0.25;
            if best.as_ref().is_none_or(|(s, _, _)| score < *s) {
                best = Some((score, index, candidate));
            }
        }
    }
    let (_, _, target) = best?;
    let offset = target.offset_at(goal);
    let pos = Pos { story: head.story.clone(), path: target.path.clone(), offset };
    Some(pos)
}

fn screen_step(doc: &mut Doc, down: bool) -> Option<Pos> {
    let mut pos = None;
    for _ in 0..30 {
        let next = line_step(doc, down)?;
        doc.session.selection = Selection::caret(next.clone());
        pos = Some(next);
    }
    pos
}

/// Tab and Shift+Tab in a table: the next cell, or a new row after the last one.
fn next_cell(doc: &mut Doc, table: Vec<u32>, row: u32, cell: u32, forward: bool) {
    let Some(unluminous_word::model::Block::Table(t)) =
        position::block(&doc.session.document.body, &table)
    else {
        return;
    };
    let cells = t.rows.get(row as usize).map_or(0, |r| r.cells.len()) as u32;
    let rows = t.rows.len() as u32;
    let (r, c) = if forward {
        if cell + 1 < cells {
            (row, cell + 1)
        } else {
            (row + 1, 0)
        }
    } else if cell > 0 {
        (row, cell - 1)
    } else if row > 0 {
        let previous = t.rows[(row - 1) as usize].cells.len() as u32;
        (row - 1, previous.saturating_sub(1))
    } else {
        return;
    };
    if forward && r >= rows {
        let _ = doc.session.table_op("row-below");
        return;
    }
    let mut path = table;
    path.extend([r, c, 0]);
    let len =
        position::paragraph(&doc.session.document, &StoryId::Body, &path).map_or(0, |p| p.len());
    doc.session.select(Pos::new(path.clone(), 0), Pos::new(path, len));
}
