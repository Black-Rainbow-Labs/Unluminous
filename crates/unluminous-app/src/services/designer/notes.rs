//! A design's notes: pinned to elements, in threads, open or resolved, kept in `<name>.comments.json`
//! beside the design.
//!
//! The file is plain JSON so it travels with the design, is committed with it, and can be read or edited by
//! an agent with its own tools. Three rules hold for it.
//!
//! - **Unknown fields are kept and written back.** Every struct holds the keys it did not read in `extra`, so
//!   a note file written by a newer Unluminous, or annotated by hand, comes back as it went in. That is the
//!   rule `services::realm::store` keeps for a key it does not know.
//! - **Nothing is written when nothing changed.** [`NotesFile::write`] compares the text it would write with
//!   what is on disk, so opening a design and looking at it leaves `git status` clean.
//! - **A note's number never repeats inside a design** (`next`), because a person and an agent both say
//!   "note 3" and a deleted note's number must not mean a different note a day later.
//!
//! The reading is by hand over `serde_json::Value` rather than derived, for the first rule: a derive would
//! need a flattened map on every struct, and a field of the wrong type would refuse the whole file where
//! this reads it as missing.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

/// The version of the file this writes.
pub const VERSION: u32 = 1;

/// Whether a note still needs doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteState {
    Open,
    Resolved,
}

impl NoteState {
    /// The word the file and the command line use.
    pub fn name(self) -> &'static str {
        match self {
            NoteState::Open => "open",
            NoteState::Resolved => "resolved",
        }
    }
}

/// Where a note is pinned.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    /// A selector that finds the element and nothing else.
    pub selector: String,
    /// The element named as the picker names it: `h1.hero-title`.
    pub tag: String,
    /// The start of the element's text, so a person can find it again.
    pub excerpt: String,
    /// Where the click was inside the element, as fractions of its box.
    pub fx: f32,
    pub fy: f32,
    /// Where the element was last seen, in page coordinates at `device`.
    pub page_x: f32,
    pub page_y: f32,
    pub device: String,
    pub extra: Map<String, Value>,
}

impl Anchor {
    /// An anchor with only a selector, which is what a note made from the command line has.
    ///
    /// @param selector - the element's selector
    pub fn on(selector: &str) -> Self {
        Self {
            selector: selector.to_owned(),
            tag: selector.rsplit('>').next().unwrap_or(selector).trim().to_owned(),
            excerpt: String::new(),
            fx: 1.0,
            fy: 0.0,
            page_x: 0.0,
            page_y: 0.0,
            device: String::new(),
            extra: Map::new(),
        }
    }

    /// Read one from the JSON the file holds, keeping every field it does not know in `extra`.
    fn read(value: &Value) -> Self {
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_owned();
        let number =
            |key: &str, otherwise: f32| value[key].as_f64().map_or(otherwise, |n| n as f32);
        let page = &value["page"];
        Self {
            selector: text("selector"),
            tag: text("tag"),
            excerpt: text("excerpt"),
            fx: number("fx", 1.0),
            fy: number("fy", 0.0),
            page_x: page["x"].as_f64().unwrap_or(0.0) as f32,
            page_y: page["y"].as_f64().unwrap_or(0.0) as f32,
            device: text("device"),
            extra: leftovers(value, &["selector", "tag", "excerpt", "fx", "fy", "page", "device"]),
        }
    }

    /// The JSON the file is written as, with the fields it did not know written back.
    fn write(&self) -> Value {
        let mut out = self.extra.clone();
        out.insert("selector".into(), json!(self.selector));
        out.insert("tag".into(), json!(self.tag));
        out.insert("excerpt".into(), json!(self.excerpt));
        out.insert("fx".into(), json!(rounded(self.fx)));
        out.insert("fy".into(), json!(rounded(self.fy)));
        out.insert("page".into(), json!({ "x": rounded(self.page_x), "y": rounded(self.page_y) }));
        out.insert("device".into(), json!(self.device));
        Value::Object(out)
    }
}

/// One message in a note's thread.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// `you` for a note made in the window, or the name an agent gave.
    pub author: String,
    pub body: String,
    pub at: String,
    pub extra: Map<String, Value>,
}

impl Message {
    /// A message written now.
    ///
    /// @param author - who wrote it
    /// @param body - what it says
    pub fn now(author: &str, body: &str) -> Self {
        Self { author: author.to_owned(), body: body.to_owned(), at: iso_now(), extra: Map::new() }
    }

    /// Read one from the JSON the file holds, keeping every field it does not know in `extra`.
    fn read(value: &Value) -> Self {
        Self {
            author: value["author"].as_str().unwrap_or("agent").to_owned(),
            body: value["body"].as_str().unwrap_or_default().to_owned(),
            at: value["at"].as_str().unwrap_or_default().to_owned(),
            extra: leftovers(value, &["author", "body", "at"]),
        }
    }

    /// The JSON the file is written as, with the fields it did not know written back.
    fn write(&self) -> Value {
        let mut out = self.extra.clone();
        out.insert("author".into(), json!(self.author));
        out.insert("body".into(), json!(self.body));
        out.insert("at".into(), json!(self.at));
        Value::Object(out)
    }
}

/// A note and its thread.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: u32,
    pub state: NoteState,
    pub anchor: Anchor,
    pub messages: Vec<Message>,
    pub created: String,
    pub resolved: Option<String>,
    pub extra: Map<String, Value>,
}

impl Note {
    /// The first message, which is what the note is about.
    pub fn first_words(&self) -> &str {
        self.messages.first().map(|message| message.body.as_str()).unwrap_or_default()
    }

    /// Read one from the JSON the file holds, keeping every field it does not know in `extra`.
    fn read(value: &Value) -> Option<Self> {
        let id = u32::try_from(value["id"].as_u64()?).ok()?;
        Some(Self {
            id,
            state: match value["state"].as_str() {
                Some("resolved") => NoteState::Resolved,
                _ => NoteState::Open,
            },
            anchor: Anchor::read(&value["anchor"]),
            messages: value["messages"]
                .as_array()
                .map(|all| all.iter().map(Message::read).collect())
                .unwrap_or_default(),
            created: value["created"].as_str().unwrap_or_default().to_owned(),
            resolved: value["resolved"].as_str().map(str::to_owned),
            extra: leftovers(value, &["id", "state", "anchor", "messages", "created", "resolved"]),
        })
    }

    /// The JSON the file is written as, with the fields it did not know written back.
    fn write(&self) -> Value {
        let mut out = self.extra.clone();
        out.insert("id".into(), json!(self.id));
        out.insert("state".into(), json!(self.state.name()));
        out.insert("anchor".into(), self.anchor.write());
        out.insert(
            "messages".into(),
            Value::Array(self.messages.iter().map(Message::write).collect()),
        );
        out.insert("created".into(), json!(self.created));
        out.insert("resolved".into(), json!(self.resolved));
        Value::Object(out)
    }
}

/// The whole file.
#[derive(Debug, Clone, PartialEq)]
pub struct NotesFile {
    /// The design's file name, written so the file says what it belongs to.
    pub design: String,
    /// The number the next note gets.
    pub next: u32,
    pub notes: Vec<Note>,
    pub extra: Map<String, Value>,
}

impl NotesFile {
    /// A file with no notes.
    ///
    /// @param design - the design's file name
    pub fn empty(design: &str) -> Self {
        Self { design: design.to_owned(), next: 1, notes: Vec::new(), extra: Map::new() }
    }

    /// Read a file's text. A text that is not JSON is refused with a sentence.
    ///
    /// @param text - the file's contents
    /// @param design - the design's file name, used when the file does not say
    pub fn parse(text: &str, design: &str) -> Result<Self, String> {
        if text.trim().is_empty() {
            return Ok(Self::empty(design));
        }
        let value: Value = serde_json::from_str(text)
            .map_err(|problem| format!("The notes file could not be read: {problem}"))?;
        let notes: Vec<Note> = value["notes"]
            .as_array()
            .map(|all| all.iter().filter_map(Note::read).collect())
            .unwrap_or_default();
        let after_the_last = notes.iter().map(|note| note.id).max().map_or(1, |id| id + 1);
        let written = u32::try_from(value["next"].as_u64().unwrap_or(1)).unwrap_or(1);
        Ok(Self {
            design: value["design"].as_str().unwrap_or(design).to_owned(),
            next: written.max(after_the_last),
            notes,
            extra: leftovers(&value, &["version", "design", "next", "notes"]),
        })
    }

    /// Read the file at a path. A file that is not there is a file with no notes.
    ///
    /// @param path - the notes file
    /// @param design - the design's file name
    pub fn read(path: &Path, design: &str) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text, design),
            Err(problem) if problem.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::empty(design))
            }
            Err(problem) => Err(format!("{} could not be read: {problem}", path.display())),
        }
    }

    /// The text this would be written as.
    pub fn text(&self) -> String {
        let mut out = self.extra.clone();
        out.insert("version".into(), json!(VERSION));
        out.insert("design".into(), json!(self.design));
        out.insert("next".into(), json!(self.next));
        out.insert("notes".into(), Value::Array(self.notes.iter().map(Note::write).collect()));
        let mut text = serde_json::to_string_pretty(&Value::Object(out)).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Write it, unless the file already says exactly this. Answers whether anything was written.
    ///
    /// A file with no notes and no file on disk is not written either: a design that has never been
    /// commented on has no notes file, and looking at one does not make it.
    ///
    /// @param path - the notes file
    pub fn write(&self, path: &Path) -> Result<bool, String> {
        let text = self.text();
        let on_disk = std::fs::read_to_string(path).ok();
        if on_disk.as_deref() == Some(text.as_str()) {
            return Ok(false);
        }
        if on_disk.is_none() && self.notes.is_empty() && self.next <= 1 {
            return Ok(false);
        }
        crate::services::store::write_atomically(path, text.as_bytes())
            .map_err(|problem| format!("{} could not be written: {problem}", path.display()))?;
        Ok(true)
    }

    /// The note with this number.
    pub fn get(&self, id: u32) -> Option<&Note> {
        self.notes.iter().find(|note| note.id == id)
    }

    /// The note with this number, to change it.
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Note> {
        self.notes.iter_mut().find(|note| note.id == id)
    }

    /// Add a note with its first message, and answer its number.
    ///
    /// @param anchor - what it is pinned to
    /// @param author - who wrote it
    /// @param body - what it says
    pub fn add(&mut self, anchor: Anchor, author: &str, body: &str) -> u32 {
        let id = self.next;
        self.next += 1;
        let first = Message::now(author, body);
        self.notes.push(Note {
            id,
            state: NoteState::Open,
            anchor,
            created: first.at.clone(),
            messages: vec![first],
            resolved: None,
            extra: Map::new(),
        });
        id
    }

    /// Add a message to a note's thread.
    ///
    /// @param id - the note
    /// @param author - who wrote it
    /// @param body - what it says
    pub fn reply(&mut self, id: u32, author: &str, body: &str) -> Result<(), String> {
        let note = self.get_mut(id).ok_or_else(|| no_such_note(id))?;
        note.messages.push(Message::now(author, body));
        Ok(())
    }

    /// Change a note between open and resolved.
    ///
    /// @param id - the note
    /// @param state - what it becomes
    pub fn set_state(&mut self, id: u32, state: NoteState) -> Result<(), String> {
        let note = self.get_mut(id).ok_or_else(|| no_such_note(id))?;
        note.resolved = match state {
            NoteState::Resolved => Some(iso_now()),
            NoteState::Open => None,
        };
        note.state = state;
        Ok(())
    }

    /// Take a note away. Its number is never used again.
    ///
    /// @param id - the note
    pub fn remove(&mut self, id: u32) -> Result<(), String> {
        let at =
            self.notes.iter().position(|note| note.id == id).ok_or_else(|| no_such_note(id))?;
        self.notes.remove(at);
        Ok(())
    }

    /// How many notes are open and how many are resolved.
    pub fn counts(&self) -> (usize, usize) {
        let open = self.notes.iter().filter(|note| note.state == NoteState::Open).count();
        (open, self.notes.len() - open)
    }
}

/// The sentence for a note number that is not in the file.
pub fn no_such_note(id: u32) -> String {
    format!("There is no note {id} on this design.")
}

/// The keys of an object that are not in `known`.
fn leftovers(value: &Value, known: &[&str]) -> Map<String, Value> {
    match value.as_object() {
        Some(all) => all
            .iter()
            .filter(|(key, _)| !known.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        None => Map::new(),
    }
}

/// A number kept to two decimals, so a pin's place does not make the file change by a rounding error.
fn rounded(number: f32) -> f64 {
    (f64::from(number) * 100.0).round() / 100.0
}

/// The time now as `2026-10-12T23:40:12Z`.
pub fn iso_now() -> String {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |gap| gap.as_secs());
    iso_from_unix(seconds)
}

/// A time in seconds since 1970 as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// @param seconds - seconds since the start of 1970, UTC
pub fn iso_from_unix(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let in_the_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        in_the_day / 3600,
        in_the_day % 3600 / 60,
        in_the_day % 60
    )
}

/// The calendar date of a day counted from 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = (if month_index < 10 { month_index + 3 } else { month_index - 9 }) as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Seconds since 1970 for a time written `YYYY-MM-DDTHH:MM:SSZ`, or nothing when it is not written that way.
///
/// @param text - a time as the notes file writes it
pub fn seconds_from_iso(text: &str) -> Option<u64> {
    let (date, time) = text.trim_end_matches('Z').split_once('T')?;
    let mut day = date.split('-').map(|part| part.parse::<i64>().ok());
    let (year, month, of_month) = (day.next()??, day.next()??, day.next()??);
    let mut clock = time.split(':').map(|part| part.parse::<i64>().ok());
    let (hour, minute, second) = (clock.next()??, clock.next()??, clock.next()??);
    let days = days_from_civil(year, month, of_month);
    u64::try_from(days * 86_400 + hour * 3600 + minute * 60 + second).ok()
}

/// Days since 1970-01-01 for a calendar date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let shifted = (month + 9) % 12;
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// How long ago a time was, in the words a thread says it in: `just now`, `5 min ago`, `3 h ago`, or the date.
///
/// @param at - the time, as the notes file writes it
/// @param now - the time now, in seconds since 1970
pub fn time_ago(at: &str, now: u64) -> String {
    let Some(then) = seconds_from_iso(at) else { return String::new() };
    let gap = now.saturating_sub(then);
    match gap {
        0..=59 => "just now".to_owned(),
        60..=3599 => format!("{} min ago", gap / 60),
        3600..=86_399 => format!("{} h ago", gap / 3600),
        _ => {
            let months = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            let iso = iso_from_unix(then);
            let month: usize = iso[5..7].parse().unwrap_or(1);
            let day: u32 = iso[8..10].parse().unwrap_or(1);
            format!("{day} {}", months[month.clamp(1, 12) - 1])
        }
    }
}

/// The time now in seconds since 1970.
pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |gap| gap.as_secs())
}
