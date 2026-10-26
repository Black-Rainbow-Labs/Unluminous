//! Threaded comments and the people who wrote them: `xl/threadedComments/threadedComment1.xml` and
//! `xl/persons/person.xml`.
//!
//! A threaded comment is two things in the file. The thread itself is in the threaded comments part, each
//! message a `threadedComment` element, the replies naming the first message as their `parentId`. Beside it
//! Excel writes an ordinary note for the cell, author `tc={id}` and `xr:uid` the thread's id, whose text
//! says what an older Excel shows in place of the thread. The note is made again every time the sheet is
//! written, so it is not in [`Sheet::notes`].

use std::collections::BTreeMap;

use roxmltree::{Document, Node};

use super::*;
use crate::coord::CellRef;
use crate::model::{Mention, Note, Person, Sheet, Thread, ThreadMessage};

pub const NS_THREADED: &str =
    "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments";
pub const REL_THREADED: &str =
    "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";
pub const REL_PERSON: &str = "http://schemas.microsoft.com/office/2017/10/relationships/person";
pub const CT_THREADED: &str = "application/vnd.ms-excel.threadedcomments+xml";
pub const CT_PERSON: &str = "application/vnd.ms-excel.person+xml";

/// What an older Excel shows in the note that stands in for a thread, ahead of the thread's own words.
const LEGACY_HEAD: &str = "[Threaded comment]\n\nYour version of Excel allows you to read this threaded comment; however, any edits to it will get removed if the file is opened in a newer version of Excel. Learn more: https://go.microsoft.com/fwlink/?linkid=870924\n\n";

/// The people in a persons part.
pub fn read_persons(xml: &str) -> Vec<Person> {
    let Ok(doc) = Document::parse(xml) else { return Vec::new() };
    doc.root_element()
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "person")
        .map(|p| Person {
            id: p.attribute("id").unwrap_or("").to_owned(),
            display_name: p.attribute("displayName").unwrap_or("").to_owned(),
            user_id: p.attribute("userId").unwrap_or("").to_owned(),
            provider_id: p.attribute("providerId").unwrap_or("None").to_owned(),
            attrs: attrs_of(p, &["id", "displayName", "userId", "providerId"]),
        })
        .collect()
}

/// A persons part holding some people.
pub fn persons_xml(persons: &[Person]) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<personList xmlns=\"{NS_THREADED}\" xmlns:x=\"{MAIN_NS}\">"
    );
    for p in persons {
        out.push_str(&format!(
            "<person displayName=\"{}\" id=\"{}\" userId=\"{}\" providerId=\"{}\"{}/>",
            escape(&p.display_name),
            escape(&p.id),
            escape(&p.user_id),
            escape(&p.provider_id),
            attrs_text(&p.attrs)
        ));
    }
    out.push_str("</personList>");
    out
}

/// The threads in a threaded comments part, by their cell.
pub fn read_threads(xml: &str) -> BTreeMap<(u32, u32), Thread> {
    let mut threads: BTreeMap<(u32, u32), Thread> = BTreeMap::new();
    let Ok(doc) = Document::parse(xml) else { return threads };
    let messages: Vec<Node> = doc
        .root_element()
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "threadedComment")
        .collect();
    for node in messages.iter().filter(|n| n.attribute("parentId").is_none()) {
        let Some(at) = node.attribute("ref").and_then(CellRef::parse) else { continue };
        let resolved = matches!(node.attribute("done"), Some("1") | Some("true"));
        let first = read_message(*node);
        threads.insert((at.row, at.col), Thread { first, replies: Vec::new(), resolved });
    }
    for node in messages.iter().filter(|n| n.attribute("parentId").is_some()) {
        let parent = node.attribute("parentId").unwrap_or("");
        if let Some(thread) = threads.values_mut().find(|t| t.first.id == parent) {
            thread.replies.push(read_message(*node));
        }
    }
    threads
}

/// One `threadedComment` element as a message.
fn read_message(node: Node) -> ThreadMessage {
    let text = node
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "text")
        .map(|t| t.text().unwrap_or("").to_owned())
        .unwrap_or_default();
    let mentions = node
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "mention")
        .map(|m| Mention {
            person: m.attribute("mentionpersonId").unwrap_or("").to_owned(),
            id: m.attribute("mentionId").unwrap_or("").to_owned(),
            start: m.attribute("startIndex").and_then(|v| v.parse().ok()).unwrap_or(0),
            length: m.attribute("length").and_then(|v| v.parse().ok()).unwrap_or(0),
        })
        .collect();
    ThreadMessage {
        id: node.attribute("id").unwrap_or("").to_owned(),
        person: node.attribute("personId").unwrap_or("").to_owned(),
        date: node.attribute("dT").unwrap_or("").to_owned(),
        text,
        mentions,
        attrs: attrs_of(node, &["ref", "dT", "personId", "id", "parentId", "done"]),
    }
}

/// A threaded comments part holding a sheet's threads, each thread's first message and then its replies.
pub fn threads_xml(threads: &BTreeMap<(u32, u32), Thread>) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<ThreadedComments xmlns=\"{NS_THREADED}\" xmlns:x=\"{MAIN_NS}\">"
    );
    for (&(row, col), thread) in threads {
        let at = CellRef::new(row, col).a1();
        for (i, message) in thread.messages().enumerate() {
            let mut attrs = format!(
                " ref=\"{at}\" dT=\"{}\" personId=\"{}\" id=\"{}\"",
                escape(&message.date),
                escape(&message.person),
                escape(&message.id)
            );
            if i > 0 {
                attrs.push_str(&format!(" parentId=\"{}\"", escape(&thread.first.id)));
            } else if thread.resolved {
                attrs.push_str(" done=\"1\"");
            }
            attrs.push_str(&attrs_text(&message.attrs));
            out.push_str(&format!(
                "<threadedComment{attrs}><text>{}</text>",
                escape(&message.text)
            ));
            if !message.mentions.is_empty() {
                out.push_str("<mentions>");
                for m in &message.mentions {
                    out.push_str(&format!(
                        "<mention mentionpersonId=\"{}\" mentionId=\"{}\" startIndex=\"{}\" length=\"{}\"/>",
                        escape(&m.person),
                        escape(&m.id),
                        m.start,
                        m.length
                    ));
                }
                out.push_str("</mentions>");
            }
            out.push_str("</threadedComment>");
        }
    }
    out.push_str("</ThreadedComments>");
    out
}

/// The words of the note Excel keeps for a thread, which an older Excel shows in the thread's place.
pub fn legacy_text(thread: &Thread) -> String {
    let mut out = String::from(LEGACY_HEAD);
    for (i, message) in thread.messages().enumerate() {
        out.push_str(if i == 0 { "Comment:\n    " } else { "\nReply:\n    " });
        out.push_str(&message.text);
    }
    out
}

/// A note made for a thread: what the writer puts in the comments part and the VML drawing.
fn legacy_note(thread: &Thread) -> Note {
    let mut note = Note::new(format!("tc={}", thread.first.id), legacy_text(thread));
    note.attrs.push(("shapeId".to_owned(), "0".to_owned()));
    note.attrs.push(("xr:uid".to_owned(), thread.first.id.clone()));
    note
}

/// Whether a note is the stand in for a thread.
pub fn is_legacy_note(note: &Note) -> bool {
    note.author.starts_with("tc=")
}

/// The notes a sheet is written with: its own and the stand ins for its threads, in cell order.
pub fn combined_notes(sheet: &Sheet) -> Vec<((u32, u32), Note)> {
    let mut all: BTreeMap<(u32, u32), Note> = sheet.notes.clone();
    for (&key, thread) in &sheet.threads {
        all.insert(key, legacy_note(thread));
    }
    all.into_iter().collect()
}

/// Take the stand in notes out of a sheet's notes after both parts were read, so only real notes stay.
pub fn drop_legacy_notes(sheet: &mut Sheet) {
    let ids: Vec<String> = sheet.threads.values().map(|t| t.first.id.clone()).collect();
    sheet.notes.retain(|_, note| {
        !(is_legacy_note(note) && ids.iter().any(|id| note.author == format!("tc={id}")))
    });
}
