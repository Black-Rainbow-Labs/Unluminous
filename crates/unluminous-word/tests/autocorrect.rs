//! Autocorrect through the session (task-2359, TDD 5.4): a correction is its own undo step, so the
//! `Ctrl+Z` straight after it gives back what was typed.

use unluminous_word::position::{self, StoryId};
use unluminous_word::session::Session;

fn text(session: &Session) -> String {
    position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap().flat_text()
}

/// Type a string a key at a time, correcting after each as the window does.
fn type_keys(session: &mut Session, keys: &str) {
    for key in keys.chars() {
        let key = key.to_string();
        session.type_text(&key);
        session.autocorrect(&key);
    }
}

#[test]
fn a_correction_is_undone_on_its_own() {
    let mut session = Session::new(unluminous_word::new::blank());
    type_keys(&mut session, "Read teh ");
    assert_eq!(text(&session), "Read the ");
    assert!(session.undo());
    assert_eq!(text(&session), "Read teh ", "the undo gives back what was typed");
    type_keys(&mut session, "book");
    assert_eq!(text(&session), "Read teh book");
}

#[test]
fn quotes_dashes_and_capitals_as_a_sentence_is_typed() {
    let mut session = Session::new(unluminous_word::new::blank());
    type_keys(&mut session, "she said \"it's fine\" -- then left. the end ");
    assert_eq!(text(&session), "She said “it’s fine” – then left. The end ");
}

#[test]
fn an_ordinal_is_superscript_and_an_address_is_a_link() {
    let mut session = Session::new(unluminous_word::new::blank());
    type_keys(&mut session, "The 2nd www.example.com ");
    let paragraph = position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap();
    let superscript: String = paragraph
        .runs
        .iter()
        .filter(|r| r.props.val_of("w:vertAlign") == Some("superscript"))
        .filter_map(|r| match &r.content {
            unluminous_word::model::Content::Text(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(superscript, "nd");
    assert!(paragraph.runs.iter().any(|r| r.link().is_some()));
    let at = position::Pos::new(vec![0], 10);
    assert_eq!(
        session.hyperlink_at(&at).map(|(a, _)| a).as_deref(),
        Some("http://www.example.com")
    );
}

#[test]
fn a_line_of_equals_signs_becomes_a_double_border_on_enter() {
    let mut session = Session::new(unluminous_word::new::blank());
    session.type_text("===");
    assert!(session.border_line_on_enter());
    let paragraph = position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap();
    assert!(paragraph.is_empty());
    let bottom = paragraph.props.child("w:pBdr").and_then(|b| b.child("w:bottom")).unwrap();
    assert_eq!(bottom.attr("w:val"), Some("double"));
}
