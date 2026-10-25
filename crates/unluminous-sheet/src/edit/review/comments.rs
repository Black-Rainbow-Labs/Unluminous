//! Notes and threaded comments.
//!
//! A cell has either a note (the yellow box) or a thread (a comment with replies, which can be resolved),
//! never both, as in Excel. Every change here is one undo step and leaves the workbook as it was when it
//! refuses.

use super::notebox::Geometry;
use crate::coord::{CellRef, Range};
use crate::edit::{Clip, Session};
use crate::model::{Mention, Note, NoteBox, Person, Thread, ThreadMessage};

/// One row of the Comments pane: a thread and where it is.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadRow {
    pub sheet: usize,
    pub at: CellRef,
    pub resolved: bool,
    /// The name of the person who started the thread.
    pub author: String,
    pub text: String,
    pub replies: usize,
}

impl Session {
    // ------------------------------------------------------------------------------------------------
    // People, ids and dates.

    /// A fresh braced GUID, the way Excel writes an id.
    fn new_guid(&mut self) -> String {
        let (a, b) = (self.seed(), self.seed());
        format!(
            "{{{:08X}-{:04X}-4{:03X}-{:04X}-{:012X}}}",
            (a >> 32) as u32,
            ((a >> 16) & 0xFFFF) as u16,
            a & 0xFFF,
            (((b >> 48) & 0x3FFF) | 0x8000) as u16,
            b & 0xFFFF_FFFF_FFFF
        )
    }

    /// The date and time of the session's clock as a threaded comment writes it: `2026-10-24T19:33:38.78`.
    /// A message made in the same hundredth of a second as the newest one in the workbook is dated a
    /// hundredth later, because Excel orders the replies of a thread by their dates.
    fn comment_date(&self) -> String {
        let latest = self
            .book
            .sheets
            .iter()
            .flat_map(|s| s.threads.values())
            .flat_map(|t| t.messages())
            .filter_map(|m| comment_serial(&m.date))
            .fold(f64::MIN, f64::max);
        let now = self.now.max(latest + 0.01 / 86_400.0);
        let (year, month, day) =
            crate::dates::date(now, self.book.date1904).unwrap_or((1900, 1, 1));
        let (h, m, s, ms) = crate::dates::time(now);
        format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{:02}", ms / 10)
    }

    /// The id of the person with a display name, adding the person when there is none.
    pub fn person_for(&mut self, name: &str) -> String {
        if let Some(p) = self.book.persons.iter().find(|p| p.display_name == name) {
            return p.id.clone();
        }
        let id = self.new_guid();
        self.book.persons.push(Person {
            id: id.clone(),
            display_name: name.to_owned(),
            user_id: name.to_owned(),
            provider_id: "None".to_owned(),
            attrs: Vec::new(),
        });
        self.book.persons_changed = true;
        id
    }

    /// The display name of a person, or the id when the persons part has no such person.
    pub fn person_name(&self, id: &str) -> String {
        self.book
            .persons
            .iter()
            .find(|p| p.id == id)
            .map_or_else(|| id.to_owned(), |p| p.display_name.clone())
    }

    /// A message by a person now.
    fn new_message(&mut self, text: &str, author: &str) -> ThreadMessage {
        let person = self.person_for(author);
        ThreadMessage {
            id: self.new_guid(),
            person,
            date: self.comment_date(),
            text: text.to_owned(),
            mentions: Vec::new(),
            attrs: Vec::new(),
        }
    }

    // ------------------------------------------------------------------------------------------------
    // Threaded comments.

    /// The thread on a cell.
    pub fn thread_at(&self, sheet: usize, at: CellRef) -> Option<&Thread> {
        self.book.sheets.get(sheet)?.threads.get(&(at.row, at.col))
    }

    /// Every thread of a sheet, or of the whole workbook, in sheet and reading order: the Comments pane.
    pub fn thread_rows(&self, sheet: Option<usize>) -> Vec<ThreadRow> {
        let mut rows = Vec::new();
        for (si, s) in self.book.sheets.iter().enumerate() {
            if sheet.is_some_and(|only| only != si) {
                continue;
            }
            for (&(row, col), thread) in &s.threads {
                rows.push(ThreadRow {
                    sheet: si,
                    at: CellRef::new(row, col),
                    resolved: thread.resolved,
                    author: self.person_name(&thread.first.person),
                    text: thread.first.text.clone(),
                    replies: thread.replies.len(),
                });
            }
        }
        rows
    }

    /// Start a threaded comment on a cell.
    pub fn add_thread(
        &mut self,
        sheet: usize,
        at: CellRef,
        text: &str,
        author: &str,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if text.trim().is_empty() {
            return Err("A comment needs some text.".to_owned());
        }
        let key = (at.row, at.col);
        let s = &self.book.sheets[sheet];
        if s.notes.contains_key(&key) {
            return Err("This cell has a note. Delete the note or convert it to a comment first."
                .to_owned());
        }
        if s.threads.contains_key(&key) {
            return Err("This cell already has a comment. Reply to it instead.".to_owned());
        }
        self.remember_whole("New comment");
        let first = self.new_message(text, author);
        let s = &mut self.book.sheets[sheet];
        s.threads.insert(key, Thread { first, replies: Vec::new(), resolved: false });
        s.notes_changed = true;
        Ok(())
    }

    /// Reply to the thread on a cell.
    pub fn reply_to_thread(
        &mut self,
        sheet: usize,
        at: CellRef,
        text: &str,
        author: &str,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if text.trim().is_empty() {
            return Err("A reply needs some text.".to_owned());
        }
        self.thread_or_err(sheet, at)?;
        self.remember_whole("Reply to comment");
        let reply = self.new_message(text, author);
        let s = &mut self.book.sheets[sheet];
        if let Some(thread) = s.threads.get_mut(&(at.row, at.col)) {
            thread.replies.push(reply);
        }
        s.notes_changed = true;
        Ok(())
    }

    /// Change the words of one message of a thread: 0 is the comment that started it, 1 the first reply.
    /// A mention whose `@name` is no longer in the words goes.
    pub fn edit_thread_message(
        &mut self,
        sheet: usize,
        at: CellRef,
        index: usize,
        text: &str,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if text.trim().is_empty() {
            return Err("A comment needs some text.".to_owned());
        }
        self.message_or_err(sheet, at, index)?;
        self.remember_whole("Edit comment");
        let names: Vec<String> = self.book.persons.iter().map(|p| p.display_name.clone()).collect();
        let ids: Vec<String> = self.book.persons.iter().map(|p| p.id.clone()).collect();
        let s = &mut self.book.sheets[sheet];
        if let Some(thread) = s.threads.get_mut(&(at.row, at.col)) {
            let message = match index {
                0 => &mut thread.first,
                n => &mut thread.replies[n - 1],
            };
            message.text = text.to_owned();
            message.mentions.retain(|m| mention_still_reads(text, m, &names, &ids));
        }
        s.notes_changed = true;
        Ok(())
    }

    /// Resolve a thread, or open it again.
    pub fn resolve_thread(
        &mut self,
        sheet: usize,
        at: CellRef,
        resolved: bool,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.thread_or_err(sheet, at)?;
        self.remember_whole(if resolved { "Resolve comment" } else { "Reopen comment" });
        let s = &mut self.book.sheets[sheet];
        if let Some(thread) = s.threads.get_mut(&(at.row, at.col)) {
            thread.resolved = resolved;
        }
        s.notes_changed = true;
        Ok(())
    }

    /// Delete one message of a thread. Deleting the first message deletes the whole thread, as Excel does.
    pub fn delete_thread_message(
        &mut self,
        sheet: usize,
        at: CellRef,
        index: usize,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.message_or_err(sheet, at, index)?;
        self.remember_whole(if index == 0 { "Delete comment" } else { "Delete reply" });
        let s = &mut self.book.sheets[sheet];
        if index == 0 {
            s.threads.remove(&(at.row, at.col));
        } else if let Some(thread) = s.threads.get_mut(&(at.row, at.col)) {
            thread.replies.remove(index - 1);
        }
        s.notes_changed = true;
        Ok(())
    }

    /// Delete the thread on a cell with all its replies.
    pub fn delete_thread(&mut self, sheet: usize, at: CellRef) -> Result<(), String> {
        self.delete_thread_message(sheet, at, 0)
    }

    /// Mark the `@name` that starts at a character of a message as a mention of a person the workbook
    /// knows. The words at that place must read `@` and the person's display name, as Excel writes them.
    pub fn add_mention(
        &mut self,
        sheet: usize,
        at: CellRef,
        index: usize,
        person: &str,
        start: usize,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let message = self.message_or_err(sheet, at, index)?.clone();
        let Some(found) = self.book.persons.iter().find(|p| p.display_name == person) else {
            return Err(format!("{person} is not someone who has commented in this workbook."));
        };
        let wanted = format!("@{}", found.display_name);
        let length = wanted.chars().count();
        let there: String = message.text.chars().skip(start).take(length).collect();
        if there != wanted {
            return Err(format!("The comment does not read {wanted} at character {start}."));
        }
        let person_id = found.id.clone();
        self.remember_whole("Mention");
        let mention = Mention { person: person_id, id: self.new_guid(), start, length };
        let s = &mut self.book.sheets[sheet];
        if let Some(thread) = s.threads.get_mut(&(at.row, at.col)) {
            let target = match index {
                0 => &mut thread.first,
                n => &mut thread.replies[n - 1],
            };
            target.mentions.push(mention);
        }
        s.notes_changed = true;
        Ok(())
    }

    fn thread_or_err(&self, sheet: usize, at: CellRef) -> Result<&Thread, String> {
        self.thread_at(sheet, at).ok_or_else(|| "There is no comment on this cell.".to_owned())
    }

    fn message_or_err(
        &self,
        sheet: usize,
        at: CellRef,
        index: usize,
    ) -> Result<&ThreadMessage, String> {
        let thread = self.thread_or_err(sheet, at)?;
        thread.messages().nth(index).ok_or_else(|| "There is no such reply.".to_owned())
    }

    /// Give every thread of a sheet new ids, which a copy of the sheet needs.
    pub(crate) fn renew_thread_ids(&mut self, sheet: usize) {
        let keys: Vec<(u32, u32)> = self.book.sheets[sheet].threads.keys().copied().collect();
        for key in keys {
            let count = 1 + self.book.sheets[sheet].threads[&key].replies.len();
            let ids: Vec<String> = (0..count).map(|_| self.new_guid()).collect();
            if let Some(thread) = self.book.sheets[sheet].threads.get_mut(&key) {
                thread.first.id = ids[0].clone();
                for (reply, id) in thread.replies.iter_mut().zip(&ids[1..]) {
                    reply.id = id.clone();
                }
            }
        }
    }

    /// Threads travel with their cells when a block is cut and pasted.
    pub(crate) fn move_threads(&mut self, clip: &Clip, sheet: usize, shift: (i64, i64)) {
        let moving: Vec<((u32, u32), Thread)> = self.book.sheets[clip.sheet]
            .threads
            .iter()
            .filter(|((r, c), _)| clip.range.contains(CellRef::new(*r, *c)))
            .map(|(k, t)| (*k, t.clone()))
            .collect();
        for (k, thread) in moving {
            self.book.sheets[clip.sheet].threads.remove(&k);
            let to = ((k.0 as i64 + shift.0) as u32, (k.1 as i64 + shift.1) as u32);
            self.book.sheets[sheet].threads.insert(to, thread);
            self.book.sheets[sheet].notes_changed = true;
            self.book.sheets[clip.sheet].notes_changed = true;
        }
    }

    // ------------------------------------------------------------------------------------------------
    // Notes.

    /// Show a note all the time, or only when the pointer is over its cell.
    pub fn show_note(&mut self, sheet: usize, at: CellRef, shown: bool) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if !self.book.sheets[sheet].notes.contains_key(&(at.row, at.col)) {
            return Err("There is no note on this cell.".to_owned());
        }
        self.remember_whole(if shown { "Show note" } else { "Hide note" });
        let s = &mut self.book.sheets[sheet];
        if let Some(note) = s.notes.get_mut(&(at.row, at.col)) {
            note.visible = shown;
        }
        s.notes_changed = true;
        Ok(())
    }

    /// Show or hide every note of a sheet, or of the workbook when `sheet` is `None`.
    pub fn show_all_notes(&mut self, sheet: Option<usize>, shown: bool) -> Result<usize, String> {
        if let Some(only) = sheet {
            self.check_sheet(only)?;
        }
        self.remember_whole(if shown { "Show all notes" } else { "Hide all notes" });
        let mut changed = 0;
        for (si, s) in self.book.sheets.iter_mut().enumerate() {
            if sheet.is_some_and(|only| only != si) {
                continue;
            }
            for note in s.notes.values_mut().filter(|n| n.visible != shown) {
                note.visible = shown;
                changed += 1;
            }
            s.notes_changed = s.notes_changed || !s.notes.is_empty();
        }
        Ok(changed)
    }

    /// The next note after a cell in reading order, wrapping to the first, or the previous one. Excel's
    /// Next Note and Previous Note.
    pub fn neighbouring_note(&self, sheet: usize, from: CellRef, forward: bool) -> Option<CellRef> {
        let s = self.book.sheets.get(sheet)?;
        let keys: Vec<(u32, u32)> = s.notes.keys().copied().collect();
        let here = (from.row, from.col);
        let found = if forward {
            keys.iter().find(|k| **k > here).or(keys.first())
        } else {
            keys.iter().rev().find(|k| **k < here).or(keys.last())
        };
        found.map(|&(row, col)| CellRef::new(row, col))
    }

    /// Where a note's box sits and how big it is. A note that was never given a place has the box Excel
    /// would give it.
    pub fn note_box(&self, sheet: usize, at: CellRef) -> Option<NoteBox> {
        let s = self.book.sheets.get(sheet)?;
        let note = s.notes.get(&(at.row, at.col))?;
        Some(note.bounds.clone().unwrap_or_else(|| super::notebox::default_box(s, at)))
    }

    /// Put a note's box at a place and size in points from the top left of the sheet.
    pub fn set_note_box(
        &mut self,
        sheet: usize,
        at: CellRef,
        left: f64,
        top: f64,
        width: f64,
        height: f64,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if !self.book.sheets[sheet].notes.contains_key(&(at.row, at.col)) {
            return Err("There is no note on this cell.".to_owned());
        }
        if width < 1.0 || height < 1.0 {
            return Err("A note's box must be at least a point wide and tall.".to_owned());
        }
        self.remember_whole("Resize note");
        let s = &mut self.book.sheets[sheet];
        let bounds = Geometry::standard(s).box_from_points(left, top, width, height);
        if let Some(note) = s.notes.get_mut(&(at.row, at.col)) {
            note.bounds = Some(bounds);
        }
        s.notes_changed = true;
        Ok(())
    }

    /// Change the size of a note's box and keep its top left corner where it is.
    pub fn set_note_size(
        &mut self,
        sheet: usize,
        at: CellRef,
        width: f64,
        height: f64,
    ) -> Result<(), String> {
        let current = self.note_box(sheet, at).ok_or("There is no note on this cell.")?;
        self.set_note_box(sheet, at, current.left, current.top, width, height)
    }

    /// Turn the notes in a range, or on the whole sheet, into threaded comments. The note's author becomes
    /// the person who started the thread and its words the comment, as Excel's Convert to Comments does.
    /// Answers how many notes were converted.
    pub fn convert_notes_to_threads(
        &mut self,
        sheet: usize,
        range: Option<Range>,
    ) -> Result<usize, String> {
        self.check_sheet(sheet)?;
        let keys: Vec<(u32, u32)> = self.book.sheets[sheet]
            .notes
            .keys()
            .filter(|(r, c)| range.is_none_or(|rg| rg.contains(CellRef::new(*r, *c))))
            .copied()
            .collect();
        if keys.is_empty() {
            return Ok(0);
        }
        self.remember_whole("Convert notes to comments");
        for key in &keys {
            let Some(note) = self.book.sheets[sheet].notes.remove(key) else { continue };
            let author = if note.author.is_empty() { "Author" } else { note.author.as_str() };
            let first = self.new_message(&clean_note_text(&note), author);
            let s = &mut self.book.sheets[sheet];
            s.threads.insert(*key, Thread { first, replies: Vec::new(), resolved: false });
        }
        self.book.sheets[sheet].notes_changed = true;
        Ok(keys.len())
    }
}

/// The serial a threaded comment's date names: `2026-10-24T19:33:38.78`.
fn comment_serial(date: &str) -> Option<f64> {
    let (day, time) = date.split_once('T')?;
    let mut parts = day.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let d = parts.next()?.parse().ok()?;
    let serial = crate::dates::serial(year, month, d, false)?;
    let mut clock = time.split(':');
    let hours: f64 = clock.next()?.parse().ok()?;
    let minutes: f64 = clock.next()?.parse().ok()?;
    let seconds: f64 = clock.next()?.parse().ok()?;
    Some(serial + (hours * 3600.0 + minutes * 60.0 + seconds) / 86_400.0)
}

/// The words of a note without the `Author:` line Excel puts in the first line of a note a person typed.
fn clean_note_text(note: &Note) -> String {
    let prefix = format!("{}:", note.author);
    match note.text.strip_prefix(&prefix) {
        Some(rest) if !note.author.is_empty() => rest.trim_start_matches(['\n', '\r']).to_owned(),
        _ => note.text.clone(),
    }
}

/// Whether the words of a message still read as the mention says.
fn mention_still_reads(text: &str, m: &Mention, names: &[String], ids: &[String]) -> bool {
    let Some(name) = ids.iter().position(|id| *id == m.person).map(|i| &names[i]) else {
        return false;
    };
    let there: String = text.chars().skip(m.start).take(m.length).collect();
    there == format!("@{name}")
}
