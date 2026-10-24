//! Threaded comments and notes against Excel.
//!
//! The fixtures in `fixtures/review/comments-*.xlsx` were made by Excel through
//! `tools/excel-oracle/make-review-comments.ps1`, and `comments-*.json` is what Excel says each one holds.
//! The tests read the files and compare, replay each scenario with the engine and compare, and ask Excel
//! to open what the engine wrote.

mod review_support;

use review_support::*;
use serde_json::{json, Value};
use unluminous_sheet::edit::review::notebox::Geometry;
use unluminous_sheet::model::Workbook;

const AUTHOR: &str = "Probe Author";

/// The comments of a workbook in the shape `review-read.ps1` prints them. Thread authors are left out when
/// `authors` is false, because Excel names the signed in user for a thread read from a file.
fn comments_of(book: &Workbook, authors: bool) -> Value {
    let person = |id: &str| {
        book.persons.iter().find(|p| p.id == id).map(|p| p.display_name.clone()).unwrap_or_default()
    };
    let sheets: Vec<Value> = book
        .sheets
        .iter()
        .map(|s| {
            let threads: Vec<Value> = s
                .threads
                .iter()
                .map(|(&(r, c), t)| {
                    let replies: Vec<Value> = t
                        .replies
                        .iter()
                        .map(|m| {
                            if authors {
                                json!({ "author": person(&m.person), "text": m.text })
                            } else {
                                json!({ "text": m.text })
                            }
                        })
                        .collect();
                    let mut v = json!({
                        "cell": unluminous_sheet::CellRef::new(r, c).a1(),
                        "text": t.first.text,
                        "resolved": t.resolved,
                        "replies": replies,
                    });
                    if authors {
                        v["author"] = json!(person(&t.first.person));
                    }
                    v
                })
                .collect();
            let notes: Vec<Value> = s
                .notes
                .iter()
                .map(|(&(r, c), n)| {
                    json!({
                        "cell": unluminous_sheet::CellRef::new(r, c).a1(),
                        "author": n.author,
                        "text": n.text,
                        "visible": n.visible,
                    })
                })
                .collect();
            json!({ "name": s.name, "threads": threads, "notes": notes })
        })
        .collect();
    json!({ "sheets": sheets })
}

/// The same shape out of Excel's JSON, with only the fields `comments_of` writes.
fn excel_comments(excel: &Value, authors: bool) -> Value {
    let sheets: Vec<Value> = excel["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let threads: Vec<Value> = s["threads"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| {
                    let replies: Vec<Value> = t["replies"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|r| {
                            if authors {
                                json!({ "author": r["author"], "text": r["text"] })
                            } else {
                                json!({ "text": r["text"] })
                            }
                        })
                        .collect();
                    let mut v = json!({
                        "cell": t["cell"], "text": t["text"], "resolved": t["resolved"], "replies": replies,
                    });
                    if authors {
                        v["author"] = t["author"].clone();
                    }
                    v
                })
                .collect();
            let notes: Vec<Value> = s["notes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| json!({ "cell": n["cell"], "author": n["author"], "text": n["text"], "visible": n["visible"] }))
                .collect();
            json!({ "name": s["name"], "threads": threads, "notes": notes })
        })
        .collect();
    json!({ "sheets": sheets })
}

/// Replay one scenario of `make-review-comments.ps1` on the start workbook.
fn replay(scenario: &str) -> unluminous_sheet::edit::Session {
    let mut s = fixture_session("comments-start.xlsx");
    match scenario {
        "threads" => {
            s.add_thread(0, at("A1"), "First thread", AUTHOR).unwrap();
            s.reply_to_thread(0, at("A1"), "Reply one", AUTHOR).unwrap();
            s.reply_to_thread(0, at("A1"), "Reply two\nsecond line", AUTHOR).unwrap();
            s.resolve_thread(0, at("A1"), true).unwrap();
            s.add_thread(0, at("B2"), "Second thread", AUTHOR).unwrap();
            s.edit_thread_message(0, at("B2"), 0, "Second thread edited").unwrap();
            s.reply_to_thread(0, at("B2"), "Dropped reply", AUTHOR).unwrap();
            s.reply_to_thread(0, at("B2"), "Kept reply", AUTHOR).unwrap();
            s.delete_thread_message(0, at("B2"), 1).unwrap();
            s.add_thread(0, at("C3"), "Third thread, deleted", AUTHOR).unwrap();
            s.delete_thread(0, at("C3")).unwrap();
            s.add_thread(0, at("D4"), "Fourth thread, reopened", AUTHOR).unwrap();
            s.resolve_thread(0, at("D4"), true).unwrap();
            s.resolve_thread(0, at("D4"), false).unwrap();
            s.add_thread(1, at("A2"), "On the second sheet", AUTHOR).unwrap();
        }
        "notes" => {
            s.set_note(0, at("A1"), Some("First note"), AUTHOR).unwrap();
            s.show_note(0, at("A1"), true).unwrap();
            s.set_note_size(0, at("A1"), 150.0, 80.0).unwrap();
            s.set_note(0, at("B2"), Some("Second note\nwith two lines"), AUTHOR).unwrap();
            s.set_note(0, at("D4"), Some("Third note"), AUTHOR).unwrap();
            s.show_note(0, at("D4"), true).unwrap();
            s.set_note_size(0, at("D4"), 200.0, 45.0).unwrap();
            s.set_note(0, at("F6"), Some("Deleted note"), AUTHOR).unwrap();
            s.set_note(0, at("F6"), None, AUTHOR).unwrap();
        }
        "mixed" => {
            s.add_thread(0, at("A1"), "A thread", AUTHOR).unwrap();
            s.set_note(0, at("B2"), Some("A note"), AUTHOR).unwrap();
            s.show_note(0, at("B2"), true).unwrap();
            s.show_note(0, at("B2"), false).unwrap();
            s.add_thread(0, at("C3"), "Another thread", AUTHOR).unwrap();
            s.reply_to_thread(0, at("C3"), "With a reply", AUTHOR).unwrap();
        }
        other => panic!("no scenario {other}"),
    }
    s
}

#[test]
fn what_excel_wrote_reads_as_what_excel_says_it_holds() {
    for scenario in ["threads", "notes", "mixed"] {
        let book = fixture_book(&format!("comments-{scenario}.xlsx"));
        let excel = fixture_json(&format!("comments-{scenario}.json"));
        assert_eq!(comments_of(&book, false), excel_comments(&excel, false), "{scenario}");
    }
}

#[test]
fn a_thread_keeps_its_people_dates_and_ids_as_excel_stored_them() {
    let book = fixture_book("comments-threads.xlsx");
    let thread = &book.sheets[0].threads[&(0, 0)];
    assert!(thread.resolved);
    assert_eq!(thread.replies.len(), 2);
    assert!(thread.first.id.starts_with('{') && thread.first.id.ends_with('}'));
    assert!(thread.first.date.starts_with("20") && thread.first.date.contains('T'));
    assert_eq!(thread.first.person, thread.replies[1].person);
    assert!(book.persons.iter().any(|p| p.id == thread.first.person));
    // The stand in note Excel keeps beside the thread is not one of the sheet's notes.
    assert!(book.sheets[0].notes.is_empty());
}

#[test]
fn an_unchanged_workbook_comes_back_with_every_comment_part_byte_for_byte() {
    for scenario in ["threads", "notes", "mixed"] {
        let name = format!("comments-{scenario}.xlsx");
        let original = fixture_bytes(&name);
        let again = unluminous_sheet::xlsx::write(&fixture_book(&name)).unwrap();
        let before = unluminous_sheet::zip::read(&original).unwrap();
        let after = unluminous_sheet::zip::read(&again).unwrap();
        for entry in before.iter().filter(|e| {
            e.name.contains("omment") || e.name.contains("person") || e.name.contains("vml")
        }) {
            let kept = after.iter().find(|e| e.name == entry.name);
            assert_eq!(kept.map(|e| &e.data), Some(&entry.data), "{scenario}: {}", entry.name);
        }
    }
}

#[test]
fn the_engine_replays_each_scenario_to_the_comments_excel_made() {
    for scenario in ["threads", "notes", "mixed"] {
        let excel = fixture_json(&format!("comments-{scenario}.json"));
        let replayed = replay(scenario);
        assert_eq!(
            comments_of(&replayed.book, true),
            excel_comments(&excel, true),
            "{scenario}: the engine's comments differ from Excel's"
        );
    }
}

#[test]
fn what_the_engine_writes_reads_back_the_same() {
    for scenario in ["threads", "notes", "mixed"] {
        let replayed = replay(scenario);
        let bytes = unluminous_sheet::xlsx::write(&replayed.book).unwrap();
        let back = unluminous_sheet::xlsx::read(&bytes).unwrap();
        assert_eq!(comments_of(&back, true), comments_of(&replayed.book, true), "{scenario}");
        assert_eq!(back.sheets[0].threads, replayed.book.sheets[0].threads, "{scenario}");
    }
}

#[test]
fn excel_opens_what_the_engine_writes_and_reads_the_same_comments() {
    if !excel_available("what Excel makes of the comments written here") {
        return;
    }
    let scratch = Scratch::new("comments");
    for scenario in ["threads", "notes", "mixed"] {
        let replayed = replay(scenario);
        let path = scratch.file(&format!("{scenario}.xlsx"));
        std::fs::write(&path, unluminous_sheet::xlsx::write(&replayed.book).unwrap()).unwrap();
        let theirs = excel_script("review-read.ps1", &["-Path", path.to_str().unwrap()]);
        assert_eq!(theirs["repaired"], false, "{scenario}: Excel repaired the file");
        let ours = comments_of(&replayed.book, false);
        assert_eq!(excel_comments(&theirs, false), ours, "{scenario}");
    }
}

#[test]
fn deleting_a_first_comment_deletes_the_thread_and_a_cell_has_one_kind_of_comment() {
    let mut s = replay("mixed");
    assert!(s.add_thread(0, at("B2"), "x", AUTHOR).is_err(), "a thread on a note");
    assert!(s.set_note(0, at("A1"), Some("x"), AUTHOR).is_err(), "a note on a thread");
    assert!(s.add_thread(0, at("A1"), "x", AUTHOR).is_err(), "a second thread");
    s.delete_thread_message(0, at("C3"), 0).unwrap();
    assert!(s.thread_at(0, at("C3")).is_none());
    s.undo();
    assert_eq!(s.thread_at(0, at("C3")).unwrap().replies.len(), 1);
}

#[test]
fn threads_move_with_their_cells_when_rows_are_inserted_and_deleted() {
    use unluminous_sheet::edit::Shift;
    use unluminous_sheet::Range;
    let mut s = replay("threads");
    s.insert(0, Range::parse("A1:A2").unwrap(), Shift::WholeRows).unwrap();
    assert!(s.thread_at(0, at("A3")).is_some() && s.thread_at(0, at("A1")).is_none());
    assert!(s.thread_at(0, at("B4")).is_some());
    s.delete(0, Range::parse("A4").unwrap(), Shift::WholeRows).unwrap();
    assert!(s.thread_at(0, at("B4")).is_none(), "the thread went with its row");
    assert!(s.thread_at(0, at("D5")).is_some());
    let bytes = unluminous_sheet::xlsx::write(&s.book).unwrap();
    let back = unluminous_sheet::xlsx::read(&bytes).unwrap();
    assert_eq!(back.sheets[0].threads.keys().copied().collect::<Vec<_>>(), vec![(2, 0), (4, 3)]);
}

#[test]
fn notes_can_be_shown_hidden_walked_and_converted() {
    let mut s = fixture_session("comments-notes.xlsx");
    assert_eq!(s.show_all_notes(Some(0), true).unwrap(), 1, "one note was hidden");
    assert!(s.book.sheets[0].notes.values().all(|n| n.visible));
    assert_eq!(s.show_all_notes(None, false).unwrap(), 3);
    assert_eq!(s.neighbouring_note(0, at("A1"), true), Some(at("B2")));
    assert_eq!(s.neighbouring_note(0, at("D4"), true), Some(at("A1")), "wraps to the first");
    assert_eq!(s.neighbouring_note(0, at("A1"), false), Some(at("D4")), "wraps to the last");
    assert_eq!(
        s.convert_notes_to_threads(0, Some(unluminous_sheet::Range::parse("A1:B2").unwrap()))
            .unwrap(),
        2
    );
    assert_eq!(s.thread_at(0, at("A1")).unwrap().first.text, "First note");
    assert!(s.book.sheets[0].notes.contains_key(&(3, 3)), "D4 is outside the range");
    let rows = s.thread_rows(None);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].author, AUTHOR);
    let bytes = unluminous_sheet::xlsx::write(&s.book).unwrap();
    let back = unluminous_sheet::xlsx::read(&bytes).unwrap();
    assert_eq!(back.sheets[0].threads.len(), 2);
    assert_eq!(back.sheets[0].notes.len(), 1);
}

#[test]
fn a_mention_is_stored_as_excel_stores_it() {
    let mut s = replay("threads");
    s.add_thread(0, at("H8"), "@Probe Author please look", "Someone Else").unwrap();
    assert!(s.add_mention(0, at("H8"), 0, "Nobody", 0).is_err());
    assert!(s.add_mention(0, at("H8"), 0, AUTHOR, 3).is_err(), "the words do not read that there");
    s.add_mention(0, at("H8"), 0, AUTHOR, 0).unwrap();
    let message = &s.thread_at(0, at("H8")).unwrap().first;
    assert_eq!((message.mentions[0].start, message.mentions[0].length), (0, 13));
    let xml = unluminous_sheet::xlsx::threads::threads_xml(&s.book.sheets[0].threads);
    assert!(xml.contains("<mentions><mention mentionpersonId="), "{xml}");
    let back = unluminous_sheet::xlsx::write(&s.book).unwrap();
    let again = unluminous_sheet::xlsx::read(&back).unwrap();
    assert_eq!(again.sheets[0].threads[&(7, 7)].first.mentions, message.mentions);
    // Editing the words so the mention no longer reads makes it go.
    s.edit_thread_message(0, at("H8"), 0, "please look").unwrap();
    assert!(s.thread_at(0, at("H8")).unwrap().first.mentions.is_empty());
}

#[test]
fn a_note_box_is_worked_out_from_points_the_way_excel_does() {
    // Excel's own boxes, from the notes workbook, on the machine it was made on: ten digit columns of
    // 63.75 points, which is a digit eight pixels wide.
    let book = fixture_book("comments-notes.xlsx");
    let sheet = &book.sheets[0];
    let geometry = Geometry { sheet, digit_width: 8.0 };
    for (cell, size) in
        [((0, 0), (150.0, 80.25)), ((1, 1), (108.0, 59.25)), ((3, 3), (200.25, 45.0))]
    {
        let note = &sheet.notes[&cell];
        let bounds = note.bounds.clone().expect("the box was read");
        let rebuilt =
            geometry.box_from_points(bounds.left, bounds.top, bounds.width, bounds.height);
        assert_eq!(rebuilt.anchor, bounds.anchor, "{cell:?}");
        assert_eq!((bounds.width, bounds.height), size, "{cell:?}");
        assert_eq!(geometry.points_of(bounds.anchor).2, bounds.width, "{cell:?}");
    }
    // A new note's box is where Excel puts it: one column to the right, a row above.
    let new = geometry.new_box(at("B2"));
    let excel = sheet.notes[&(1, 1)].bounds.clone().unwrap();
    assert_eq!(new.anchor, excel.anchor);
    assert_eq!((new.left, new.top), (excel.left, excel.top));
}
