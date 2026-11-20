//! Find, Copy and the other Edit entries in a web page: a browser tab or a browser node (`task-2786`).
//!
//! *"Find needs to work inside of browser tabs, in realm, etc. e.g. CTRL+F should allow me to search in the
//! page."* and *"CMD+C on selected text on a browser tab/node should copy."*
//!
//! A page is a native child view, so nothing egui draws can read it. Both are done by asking the page with a
//! script and reading the answer when the engine gives it, a frame or more later:
//!
//! - **Find** is the page's own `window.find`, which selects the match and scrolls it into view, as a browser's
//!   own Find does. The count is the number of times the needle is in the page's visible text, worked out in
//!   the same script. The bar is the editing area's `components::find_bar`, drawn on a foreground layer over the
//!   page, so the page is cut round it like round any popup.
//! - **Copy and Cut** read the page's selection, the focused field's own when a field has the keyboard, and put
//!   it on the clipboard here. `document.execCommand('copy')` from a script is refused by WebKit, which allows a
//!   copy only inside a person's own gesture, and wry's child view answers no to every key equivalent, so the
//!   page never sees `Cmd+C` itself.

use std::sync::{Arc, Mutex};

use egui::Rect;

use crate::app::{Focus, UnluminousApp};
use crate::services::find::{Field, Find};

/// The Find bar over a page, and what the page last answered.
#[derive(Default)]
pub struct PageFind {
    /// The tab whose page is searched.
    pub id: u64,
    /// The words and the two toggles, with the page's matches stood in for. See `Find::counted_elsewhere`.
    pub find: Find,
    /// Which match the page has selected, counted from nought.
    index: usize,
    /// What the page answered, written by the engine's callback: the needle, the two toggles it was asked with,
    /// whether a match was selected, and how many there are.
    answer: Arc<Mutex<Option<PageAnswer>>>,
}

/// One answer from the page's find script.
#[derive(Debug, Clone, PartialEq)]
struct PageAnswer {
    needle: String,
    match_case: bool,
    whole_word: bool,
    found: bool,
    count: usize,
}

/// How a search is asked of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// From the top of the page, which is what a change to the words asks.
    First,
    Next,
    Previous,
}

impl Step {
    fn word(self) -> &'static str {
        match self {
            Step::First => "first",
            Step::Next => "next",
            Step::Previous => "previous",
        }
    }
}

/// The script that searches the page and counts its matches. Answers `[found, count]`.
///
/// `window.find` is in both WebKit and Chromium, and takes the needle, whether case matters, whether to search
/// backwards, whether to wrap, whether to match whole words, whether to look in frames and whether to show the
/// browser's own dialog. The first search of new words starts from the top: the selection is taken away first,
/// because `window.find` starts from wherever the selection is.
pub fn find_script(needle: &str, match_case: bool, whole_word: bool, step: Step) -> String {
    let needle = serde_json::to_string(needle).unwrap_or_else(|_| "\"\"".to_owned());
    format!(
        r#"(function(n,c,w,how){{
  if(how==='first'){{ var s=window.getSelection(); if(s) s.removeAllRanges(); }}
  var found = n ? window.find(n, c, how==='previous', true, w, true, false) : false;
  var t = (document.body && document.body.innerText) || '';
  var count = 0;
  if (n) {{
    var e = n.replace(/[.*+?^${{}}()|[\]\\]/g, '\\$&');
    if (w) e = '\\b' + e + '\\b';
    try {{ var m = t.match(new RegExp(e, c ? 'g' : 'gi')); count = m ? m.length : 0; }} catch (_) {{}}
  }}
  return [!!found, count];
}})({needle},{match_case},{whole_word},'{}')"#,
        step.word()
    )
}

/// The script that answers the page's selected words: the focused field's selection when a field or a text
/// area has the keyboard, and the document's otherwise. With `cut`, the selection is deleted afterwards.
pub fn selection_script(cut: bool) -> String {
    format!(
        r#"(function(cut){{
  var a = document.activeElement, t = '';
  if (a && (a.tagName === 'TEXTAREA' || (a.tagName === 'INPUT' && /^(text|search|url|tel|email|password)?$/i.test(a.type || ''))) && typeof a.selectionStart === 'number') {{
    t = a.value.substring(a.selectionStart, a.selectionEnd);
  }} else {{
    var s = window.getSelection(); t = s ? s.toString() : '';
  }}
  if (cut && t) document.execCommand('delete');
  return t;
}})({cut})"#
    )
}

/// The script that answers the colour the page is drawn on: the body's background, then the root element's, and
/// white when both are transparent, which is what a browser draws an unstyled page on.
pub const GROUND_SCRIPT: &str = r#"(function(){
  function pick(e){ if(!e) return null; var c = getComputedStyle(e).backgroundColor; var m = c && c.match(/rgba?\(([^)]+)\)/); if(!m) return null; var p = m[1].split(',').map(function(x){return parseFloat(x);}); if(p.length>3 && p[3]===0) return null; return [p[0]|0,p[1]|0,p[2]|0]; }
  return pick(document.body) || pick(document.documentElement) || [255,255,255];
})()"#;

/// Read `[found, count]` out of the engine's JSON answer.
fn read_the_answer(json: &str) -> Option<(bool, usize)> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let found = value.get(0)?.as_bool()?;
    let count = value.get(1)?.as_u64()? as usize;
    Some((found, count))
}

/// Read a colour `[r, g, b]` out of the engine's JSON answer.
pub fn read_a_colour(json: &str) -> Option<egui::Color32> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let part =
        |at: usize| value.get(at).and_then(|part| part.as_u64()).map(|part| part.min(255) as u8);
    Some(egui::Color32::from_rgb(part(0)?, part(1)?, part(2)?))
}

/// The JSON string the engine hands back for a script that answered a string.
fn read_a_string(json: &str) -> Option<String> {
    serde_json::from_str::<String>(json).ok()
}

impl UnluminousApp {
    /// The browser tab whose page holds the keyboard: the tab showing in the editing area while the editing area
    /// has it, or the chosen browser node while the canvas has it.
    pub(crate) fn page_with_the_keyboard(&self) -> Option<u64> {
        match self.focus {
            Focus::Editor => self.files.active().browser.as_ref().map(|tab| tab.id),
            Focus::Realm => {
                let node = self.realm.realm.chosen()?;
                self.realm.live.browser(node).map(|tab| tab.id)
            }
            _ => None,
        }
    }

    /// Open the Find bar over page `id`, or put the keyboard back in it when it is already open there.
    pub(crate) fn open_the_find_in_the_page(&mut self, id: u64) {
        match self.page_find.as_mut() {
            Some(open) if open.id == id => open.find.field = Field::Find,
            _ => {
                let mut find = Find::opened_with(None, false);
                find.read_only = true;
                self.page_find = Some(PageFind { id, find, ..PageFind::default() });
            }
        }
    }

    /// Put the page's Find bar away, leaving its last match selected in the page.
    pub(crate) fn close_the_find_in_the_page(&mut self) {
        self.page_find = None;
    }

    /// Ask the page to search, and take the answer when it comes.
    fn ask_the_page_to_find(&mut self, step: Step, ctx: &egui::Context) {
        let Some(open) = self.page_find.as_mut() else { return };
        let (needle, match_case, whole_word) =
            (open.find.needle.clone(), open.find.match_case, open.find.whole_word);
        if step == Step::First {
            open.index = 0;
        }
        // The question is written down now, so a second frame does not ask it again before the answer comes.
        open.find.counted_elsewhere(open.find.count(), open.index);
        let id = open.id;
        let answer = open.answer.clone();
        let repaint = ctx.clone();
        let asked =
            PageAnswer { needle: needle.clone(), match_case, whole_word, found: false, count: 0 };
        let script = find_script(&needle, match_case, whole_word, step);
        let result = self.browser.evaluate_with_callback(id, &script, move |json| {
            if let Some((found, count)) = read_the_answer(&json) {
                if let Ok(mut held) = answer.lock() {
                    *held = Some(PageAnswer { found, count, ..asked.clone() });
                }
            }
            repaint.request_repaint();
        });
        if let Err(problem) = result {
            self.message = Some(problem);
        }
    }

    /// Step to the next or the previous match in the page, which is what Enter, the arrows and `Cmd+G` do.
    pub(crate) fn step_the_find_in_the_page(&mut self, forward: bool) {
        let Some(ctx) = self.context.clone() else { return };
        let Some(open) = self.page_find.as_mut() else { return };
        if open.find.needle.is_empty() {
            return;
        }
        let count = open.find.count().max(1);
        open.index = match forward {
            true => (open.index + 1) % count,
            false => (open.index + count - 1) % count,
        };
        self.ask_the_page_to_find(if forward { Step::Next } else { Step::Previous }, &ctx);
    }

    /// Take what the page answered into the bar's tally.
    fn take_the_pages_answer(&mut self) {
        let Some(open) = self.page_find.as_mut() else { return };
        let Some(answer) = open.answer.lock().ok().and_then(|mut held| held.take()) else { return };
        if answer.needle != open.find.needle
            || answer.match_case != open.find.match_case
            || answer.whole_word != open.find.whole_word
        {
            return;
        }
        let count = if answer.found { answer.count.max(1) } else { 0 };
        open.index = open.index.min(count.saturating_sub(1));
        open.find.counted_elsewhere(count, open.index);
    }

    /// Draw the page's Find bar, over the top right of the page it searches, and act on what was pressed.
    ///
    /// Drawn on a foreground layer after every pane, so it is one of the things the page is cut round, and the
    /// page's keyboard is handed back to the window while one of its boxes has it.
    pub(crate) fn show_the_find_in_the_page(&mut self, ctx: &egui::Context) {
        let Some(id) = self.page_find.as_ref().map(|open| open.id) else { return };
        // A page that has gone takes its bar with it.
        let exists =
            self.files.iter().any(|file| file.browser.as_ref().is_some_and(|tab| tab.id == id))
                || self.realm.live.node_of_browser(id).is_some();
        if !exists {
            self.page_find = None;
            return;
        }
        self.take_the_pages_answer();
        let Some(page) = self.browser_placements.iter().find(|placement| placement.id == id) else {
            return;
        };
        let visible = page.visible;
        let typing = self.page_find.as_ref().is_some_and(|open| open.find.field != Field::Away);
        // The keys are taken before the bar is drawn, for the reason the editing area's bar takes them.
        let (enter, shift_enter, escape) = match typing {
            true => ctx.input_mut(|input| {
                (
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                    input.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                )
            }),
            false => (false, false, false),
        };
        let Some(open) = self.page_find.as_mut() else { return };
        let bar = crate::components::find_bar::rect_over(visible, &open.find);
        let mut outcome = crate::components::find_bar::Outcome::default();
        egui::Area::new(egui::Id::new(("page-find", id)))
            .order(egui::Order::Foreground)
            .fixed_pos(bar.min)
            .show(ctx, |ui| {
                ui.set_min_size(bar.size());
                ui.set_max_size(bar.size());
                outcome = crate::components::find_bar::show(ui, visible, &mut open.find);
            });
        outcome.next |= enter;
        outcome.previous |= shift_enter;
        outcome.close |= escape;
        if outcome.close {
            self.close_the_find_in_the_page();
            return;
        }
        let new = self.page_find.as_ref().is_some_and(|open| open.find.asks_something_new());
        if new {
            let empty = self.page_find.as_ref().is_some_and(|open| open.find.needle.is_empty());
            match empty {
                true => {
                    if let Some(open) = self.page_find.as_mut() {
                        open.find.counted_elsewhere(0, 0);
                    }
                }
                false => self.ask_the_page_to_find(Step::First, ctx),
            }
        } else if outcome.next {
            self.step_the_find_in_the_page(true);
        } else if outcome.previous {
            self.step_the_find_in_the_page(false);
        }
    }

    /// Where the page's Find bar is drawn, when one is open over a page that is placed.
    pub fn page_find_bar(&self) -> Option<Rect> {
        let open = self.page_find.as_ref()?;
        let page = self.browser_placements.iter().find(|placement| placement.id == open.id)?;
        Some(crate::components::find_bar::rect_over(page.visible, &open.find))
    }

    /// Whether one of the page Find bar's boxes holds the keyboard, which is when the page must give the operating
    /// system's keyboard back to the window.
    pub(crate) fn page_find_is_typed_into(&self) -> bool {
        self.page_find.as_ref().is_some_and(|open| open.find.field != Field::Away)
    }

    /// The page was clicked into while its Find bar was open, so the keyboard is the page's and neither box takes
    /// it back.
    pub(crate) fn the_page_took_the_keyboard_from_its_find(&mut self) {
        if let Some(open) = self.page_find.as_mut() {
            open.find.field = Field::Away;
        }
    }

    /// What `browser find` answers: the words, how many matches and which is selected.
    pub fn page_find_state(&self) -> Option<(u64, String, usize, Option<usize>)> {
        let open = self.page_find.as_ref()?;
        Some((open.id, open.find.needle.clone(), open.find.count(), open.find.index()))
    }

    /// Search page `id` for `needle`, as typing into its Find bar does. `browser find`.
    pub fn find_in_the_page(&mut self, id: u64, needle: &str, match_case: bool, whole_word: bool) {
        self.open_the_find_in_the_page(id);
        if let Some(open) = self.page_find.as_mut() {
            open.find.needle = needle.to_owned();
            open.find.match_case = match_case;
            open.find.whole_word = whole_word;
        }
        if let Some(ctx) = self.context.clone() {
            self.ask_the_page_to_find(Step::First, &ctx);
        }
    }

    /// Copy or cut what page `id` has selected onto the clipboard. Answers nothing: the words arrive a frame or
    /// more later and are put on the clipboard then.
    pub(crate) fn copy_from_the_page(&mut self, id: u64, cut: bool) {
        let result = self.browser.evaluate_with_callback(id, &selection_script(cut), |json| {
            let Some(words) = read_a_string(&json) else { return };
            if words.is_empty() {
                return;
            }
            if let Ok(mut board) = arboard::Clipboard::new() {
                let _ = board.set_text(words);
            }
        });
        if let Err(problem) = result {
            self.message = Some(problem);
        }
    }

    /// Ask page `id` what colour it is drawn on, for the stand in painted round a popup over it on a platform
    /// that cannot photograph a page. The answer is kept in `page_grounds`.
    pub(crate) fn ask_for_the_page_ground(&mut self, id: u64, ctx: &egui::Context) {
        let grounds = self.page_grounds.clone();
        let repaint = ctx.clone();
        let _ = self.browser.evaluate_with_callback(id, GROUND_SCRIPT, move |json| {
            if let Some(colour) = read_a_colour(&json) {
                if let Ok(mut held) = grounds.lock() {
                    held.insert(id, colour);
                }
                repaint.request_repaint();
            }
        });
    }

    /// The colour page `id` is drawn on, when it has said. See [`Self::ask_for_the_page_ground`].
    pub(crate) fn page_ground(&self, id: u64) -> Option<egui::Color32> {
        self.page_grounds.lock().ok().and_then(|held| held.get(&id).copied())
    }
}

/// What a window keeps of each page's background colour.
pub type PageGrounds = Arc<Mutex<std::collections::HashMap<u64, egui::Color32>>>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The find script carries the words as a string the page reads back exactly, whatever is in them.
    #[test]
    fn the_find_script_quotes_the_words() {
        let script = find_script("it's \"here\"\n", true, false, Step::Next);
        assert!(script.contains(r#""it's \"here\"\n""#), "{script}");
        assert!(script.ends_with(",true,false,'next')"), "{script}");
    }

    /// The page's answer is read as a match and a count, and anything else is no answer.
    #[test]
    fn the_pages_answer_is_read() {
        assert_eq!(read_the_answer("[true,4]"), Some((true, 4)));
        assert_eq!(read_the_answer("[false,0]"), Some((false, 0)));
        assert_eq!(read_the_answer("null"), None);
        assert_eq!(read_a_colour("[12,34,56]"), Some(egui::Color32::from_rgb(12, 34, 56)));
        assert_eq!(read_a_colour("[300,0,0]"), Some(egui::Color32::from_rgb(255, 0, 0)));
        assert_eq!(read_a_string("\"words\""), Some("words".to_owned()));
    }

    /// Counted elsewhere, the tally and the stepping are the Find bar's own.
    #[test]
    fn matches_counted_in_a_page_make_a_tally() {
        let mut find = Find::opened_with(Some("page"), false);
        assert!(find.asks_something_new());
        find.counted_elsewhere(3, 1);
        assert!(!find.asks_something_new());
        assert_eq!(find.tally().as_deref(), Some("2 of 3"));
        find.needle.push('s');
        assert!(find.asks_something_new());
        find.counted_elsewhere(0, 0);
        assert_eq!(find.tally().as_deref(), Some("No results"));
    }
}
