//! The conversation with the script in the page: the calls the host makes, and reading what comes back.
//!
//! `designer.js` is evaluated in the page and gives the page nothing to call. The host asks it what happened
//! (`take`) and tells it what to draw (`setMode`, `setPins`, `select`, `focus`, `clear`), each as a short
//! script built here. A page can write `window.__unluminousDesigner` itself, so what comes back is trusted for
//! its shape and no further: [`read_reports`] ignores anything it does not recognise, and cuts every string
//! to its stated length again.

use serde_json::Value;

use crate::services::browser::{Watch, PIN_SCRIPT};

/// The script that draws the overlay and reports what happens in it.
pub const DESIGNER_SCRIPT: &str = include_str!("designer.js");

/// The longest each kind of string is allowed to be when it comes back from the page.
const SELECTOR_LONGEST: usize = 600;
const TAG_LONGEST: usize = 80;
const EXCERPT_LONGEST: usize = 120;
const HTML_LONGEST: usize = 1200;
const STYLE_LONGEST: usize = 200;
const URL_LONGEST: usize = 500;

/// How many pins and styles one report may carry.
const MOST_ITEMS: usize = 200;

/// What a click in the page does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// The page behaves as a page: links and buttons work.
    #[default]
    Browse,
    /// Hovering outlines an element and a click chooses it.
    Select,
    /// As select, and the click opens a note on the element.
    Comment,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Browse, Mode::Select, Mode::Comment];

    /// The word the script, the command line and the toolbar use.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Browse => "browse",
            Mode::Select => "select",
            Mode::Comment => "comment",
        }
    }

    /// The word as a person reads it, with a capital.
    pub fn title(self) -> &'static str {
        match self {
            Mode::Browse => "Browse",
            Mode::Select => "Select",
            Mode::Comment => "Comment",
        }
    }

    /// What a typed name stands for, ignoring case, or nothing when it names none of them.
    pub fn from_name(name: &str) -> Option<Mode> {
        let wanted = name.trim().to_lowercase();
        Mode::ALL.into_iter().find(|mode| mode.name() == wanted)
    }
}

/// The box an element had in the page, in page coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PageRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// An element, as the page described it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    /// A selector that finds this element and nothing else.
    pub selector: String,
    /// The element named as the picker names it: `h1.hero-title`.
    pub tag: String,
    /// The start of its text on one line.
    pub excerpt: String,
    /// The start of its markup.
    pub html: String,
    /// Eight computed properties, in the order the page listed them.
    pub styles: Vec<(String, String)>,
    pub rect: PageRect,
}

/// A pin the script found, with where it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub id: u32,
    /// The pin's place in page coordinates.
    pub x: f32,
    pub y: f32,
    pub element: Option<Element>,
}

/// Something the script reported.
#[derive(Debug, Clone, PartialEq)]
pub enum Report {
    /// An element was chosen.
    Selected(Element),
    /// A click in comment mode, at fractions of the element's box.
    Comment { element: Element, fx: f32, fy: f32 },
    /// A pin was clicked.
    Pin(u32),
    /// The choice was put away.
    Cleared,
    /// Where each pin ended up, and which could not be found.
    Anchors { found: Vec<Found>, missing: Vec<u32> },
    /// A document has the script, which is the moment to send the mode and the pins again.
    Loaded { url: String, scroll_y: f32 },
}

/// One pin to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct PinSpec {
    pub id: u32,
    pub selector: String,
    pub fx: f32,
    pub fy: f32,
    /// Where the element was last seen, in page coordinates.
    pub page: (f32, f32),
    pub active: bool,
    pub detached: bool,
}

/// The script the host keeps in the page and the question it asks it.
pub fn watch() -> Watch {
    Watch {
        install: format!("{PIN_SCRIPT}\n{DESIGNER_SCRIPT}"),
        take: "window.__unluminousDesigner ? window.__unluminousDesigner.take() : { reports: [] }"
            .to_owned(),
    }
}

/// Text as a quoted script string, with the quotes and backslashes in it escaped.
///
/// @param text - any text
pub fn quoted(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

/// A call to the overlay, which does nothing in a page that does not have it yet.
fn call(name: &str, argument: &str) -> String {
    format!("window.__unluminousDesigner && window.__unluminousDesigner.{name}({argument});")
}

/// The script that changes what a click does.
///
/// @param mode - the mode
pub fn set_mode(mode: Mode) -> String {
    call("setMode", &quoted(mode.name()))
}

/// The script that replaces the pins.
///
/// @param pins - every pin to draw
pub fn set_pins(pins: &[PinSpec]) -> String {
    let list: Vec<Value> = pins
        .iter()
        .map(|pin| {
            serde_json::json!({
                "id": pin.id,
                "n": pin.id,
                "selector": pin.selector,
                "fx": pin.fx,
                "fy": pin.fy,
                "page": { "x": pin.page.0, "y": pin.page.1 },
                "active": pin.active,
                "detached": pin.detached,
            })
        })
        .collect();
    call("setPins", &Value::Array(list).to_string())
}

/// The script that chooses an element as a click would.
///
/// @param selector - what finds the element
pub fn select(selector: &str) -> String {
    call("select", &quoted(selector))
}

/// The script that sets how much bigger than a CSS pixel the overlay is drawn.
///
/// @param scale - one over the page's zoom; anything not a positive number is read as one
pub fn set_scale(scale: f64) -> String {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    call("setScale", &format!("{scale:.4}"))
}

/// The script that scrolls a pin's element into view and pulses its pin.
///
/// @param id - the note
pub fn focus(id: u32) -> String {
    call("focus", &id.to_string())
}

/// The script that forgets the choice.
pub fn clear() -> String {
    call("clear", "")
}

/// The script that moves the choice to the element's parent.
pub fn parent() -> String {
    call("parent", "")
}

/// The script that moves the choice to the element's first child.
pub fn child() -> String {
    call("child", "")
}

/// Read what the script answered `take` with, and nothing it does not recognise.
///
/// @param json - the answer, as the engine gave it
pub fn read_reports(json: &str) -> Vec<Report> {
    let Ok(answer) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    let Some(all) = answer["reports"].as_array() else { return Vec::new() };
    all.iter().take(MOST_ITEMS).filter_map(read_report).collect()
}

/// One report, or nothing when it is not one of the six.
fn read_report(report: &Value) -> Option<Report> {
    match report["kind"].as_str()? {
        "selected" => Some(Report::Selected(read_element(&report["element"])?)),
        "comment" => Some(Report::Comment {
            element: read_element(&report["element"])?,
            fx: fraction(&report["fx"]),
            fy: fraction(&report["fy"]),
        }),
        "pin" => Some(Report::Pin(u32::try_from(report["id"].as_u64()?).ok()?)),
        "cleared" => Some(Report::Cleared),
        "anchors" => Some(Report::Anchors {
            found: report["found"]
                .as_array()
                .map(|all| all.iter().take(MOST_ITEMS).filter_map(read_found).collect())
                .unwrap_or_default(),
            missing: report["missing"]
                .as_array()
                .map(|all| {
                    all.iter()
                        .take(MOST_ITEMS)
                        .filter_map(|id| u32::try_from(id.as_u64()?).ok())
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "loaded" => Some(Report::Loaded {
            url: cut(report["url"].as_str().unwrap_or_default(), URL_LONGEST),
            scroll_y: number(&report["scrollY"]),
        }),
        _ => None,
    }
}

/// One pin the script found, when it has a number.
fn read_found(value: &Value) -> Option<Found> {
    Some(Found {
        id: u32::try_from(value["id"].as_u64()?).ok()?,
        x: number(&value["x"]),
        y: number(&value["y"]),
        element: read_element(&value["element"]),
    })
}

/// An element, when the report has the one thing an element cannot be without: a selector.
fn read_element(value: &Value) -> Option<Element> {
    let selector = cut(value["selector"].as_str()?, SELECTOR_LONGEST);
    if selector.trim().is_empty() {
        return None;
    }
    let styles = value["styles"]
        .as_object()
        .map(|all| {
            all.iter()
                .take(16)
                .map(|(name, value)| {
                    (cut(name, 40), cut(value.as_str().unwrap_or_default(), STYLE_LONGEST))
                })
                .collect()
        })
        .unwrap_or_default();
    let rect = &value["rect"];
    Some(Element {
        selector,
        tag: cut(value["tag"].as_str().unwrap_or_default(), TAG_LONGEST),
        excerpt: cut(value["excerpt"].as_str().unwrap_or_default(), EXCERPT_LONGEST),
        html: cut(value["html"].as_str().unwrap_or_default(), HTML_LONGEST),
        styles,
        rect: PageRect {
            x: number(&rect["x"]),
            y: number(&rect["y"]),
            w: number(&rect["w"]),
            h: number(&rect["h"]),
        },
    })
}

/// A number from the page, or nothing at all when it is not a finite one.
fn number(value: &Value) -> f32 {
    value.as_f64().filter(|number| number.is_finite()).unwrap_or(0.0) as f32
}

/// A fraction of an element's box, kept between nothing and all of it.
fn fraction(value: &Value) -> f32 {
    number(value).clamp(0.0, 1.0)
}

/// Text cut to at most `most` characters.
///
/// @param text - any text
/// @param most - the most characters it may have
pub fn cut(text: &str, most: usize) -> String {
    match text.chars().count() > most {
        true => text.chars().take(most).collect(),
        false => text.to_owned(),
    }
}
