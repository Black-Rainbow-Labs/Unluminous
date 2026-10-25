//! The keys of the popups of the clipboard and print features (`task-2360`, the `wiringb` lane): the letters
//! after Ctrl+Alt+V that choose a Paste Special option, and the keys that turn the pages of the print
//! preview. They are read before the grid reads its own, so a letter meant for the popup never types into a
//! cell.

use egui::{Event, Key};

use super::keys::Keys;
use crate::services::spreadsheet::Book;

/// Take the keys the open popup or preview wants. Answers `true` when it took any.
pub fn intercept(ctx: &egui::Context, book: &mut Book, keys: &mut Keys) -> bool {
    if book.view.lane_b.paste_draft.is_some() {
        return paste_special_keys(ctx, book, keys);
    }
    if book.view.lane_b.preview.is_some() {
        return preview_keys(ctx, keys);
    }
    false
}

/// Letters choose, Enter pastes, Escape puts the popup away.
fn paste_special_keys(ctx: &egui::Context, book: &mut Book, keys: &mut Keys) -> bool {
    let (mut letters, mut enter, mut escape) = (Vec::new(), false, false);
    ctx.input_mut(|input| {
        input.events.retain(|event| match event {
            Event::Text(text) => {
                letters.extend(text.chars().filter(|c| c.is_ascii_alphabetic()));
                false
            }
            Event::Key { key: Key::Enter, pressed: true, .. } => {
                enter = true;
                false
            }
            Event::Key { key: Key::Escape, pressed: true, .. } => {
                escape = true;
                false
            }
            _ => true,
        });
    });
    let Some(mut draft) = book.view.lane_b.paste_draft else { return false };
    for letter in &letters {
        super::panes_b::press_letter(&mut draft, *letter);
    }
    book.view.lane_b.paste_draft = Some(draft);
    if escape {
        book.view.lane_b.paste_draft = None;
    } else if enter {
        keys.words.push(draft.words());
        book.view.lane_b.paste_draft = None;
    }
    !letters.is_empty() || enter || escape
}

/// Page Down and the right and down arrows show the next page, Page Up the previous, Escape closes.
fn preview_keys(ctx: &egui::Context, keys: &mut Keys) -> bool {
    let mut took = false;
    ctx.input_mut(|input| {
        input.events.retain(|event| {
            let Event::Key { key, pressed: true, .. } = event else { return true };
            let how = match key {
                Key::PageDown | Key::ArrowRight | Key::ArrowDown => "next",
                Key::PageUp | Key::ArrowLeft | Key::ArrowUp => "previous",
                Key::Home => "first",
                Key::Escape => "close",
                _ => return true,
            };
            keys.words.push(super::panes_b::preview_words(how));
            took = true;
            false
        });
    });
    took
}
