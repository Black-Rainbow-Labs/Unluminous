//! Spell check follows Word's default rules: unknown words and repeated words are underlined, and
//! capitals, words with digits, addresses, field codes and runs marked `w:noProof` are left alone.
//! `task-2341`.

use unluminous_word::session::Session;
use unluminous_word::spell::{self, Kind, Speller};

/// A blank document with `text` typed into it.
fn typed(text: &str) -> Session {
    let mut session = Session::new(unluminous_word::new::blank());
    session.type_text(text);
    session
}

/// The underlined words in a session's document, with why each one is underlined.
fn underlined(session: &Session, speller: &Speller) -> Vec<(String, Kind)> {
    spell::check_document(&session.document, speller)
        .into_iter()
        .map(|m| (m.word, m.kind))
        .collect()
}

#[test]
fn unknown_and_repeated_words_are_underlined_and_the_rest_are_not() {
    let speller = Speller::english().unwrap();
    let session = typed("This sentance has a misteak in in it. NASA and R2D2 are fine, as are https://example.com/page and jason@example.com.");
    assert_eq!(
        underlined(&session, &speller),
        [
            ("sentance".to_owned(), Kind::Spelling),
            ("misteak".to_owned(), Kind::Spelling),
            ("in".to_owned(), Kind::Repeated)
        ]
    );
}

#[test]
fn a_repeated_word_across_punctuation_is_not_a_repeat() {
    let speller = Speller::english().unwrap();
    let session = typed("I said that. That was all. Don't, don't do it.");
    assert_eq!(underlined(&session, &speller), []);
}

#[test]
fn suggestions_come_best_first_and_an_added_word_is_no_longer_underlined() {
    let mut speller = Speller::english().unwrap();
    assert!(speller.suggestions("sentance", 5).iter().any(|s| s == "sentence"));
    let session = typed("Unluminous opens Word documents.");
    assert_eq!(underlined(&session, &speller), [("Unluminous".to_owned(), Kind::Spelling)]);
    speller.add_word("Unluminous");
    assert_eq!(underlined(&session, &speller), []);
    let session = typed("A frobnicated widget.");
    speller.ignore("frobnicated");
    assert_eq!(underlined(&session, &speller), []);
}

#[test]
fn a_run_marked_no_proof_or_in_another_language_is_not_checked() {
    let speller = Speller::english().unwrap();
    let mut session = typed("Ignorr this. Bonjour mes amis, je suis contant.");
    let found = session.find("Ignorr", true, true)[0].clone();
    session.select(found.anchor, found.head);
    session.format(|rpr| unluminous_word::props::set_val(rpr, "w:noProof", Some("1")));
    let found = session.find("Bonjour mes amis, je suis contant.", true, false)[0].clone();
    session.select(found.anchor, found.head);
    session.format(|rpr| {
        rpr.child_or_insert("w:lang").set_attr("w:val", "fr-FR");
    });
    assert_eq!(underlined(&session, &speller), []);
}

#[test]
fn field_codes_and_deleted_text_are_not_checked() {
    let speller = Speller::english().unwrap();
    let mut session = typed("Page ");
    session.insert_field("PAGE \\* MERGEFORMAT", "1");
    session.set_tracking(true);
    session.type_text(" wrongg");
    let found = session.find("wrongg", true, true)[0].clone();
    session.select(found.anchor, found.head);
    session.backspace(unluminous_word::edit::Unit::Character);
    assert_eq!(underlined(&session, &speller), []);
}

#[test]
fn a_non_breaking_space_before_a_word_does_not_stop_the_check() {
    // task-2375: Word writes U+00A0 between words, which is two bytes in UTF-8, and the check that
    // looks for an address round a word sliced the text one byte after it and panicked.
    let speller = Speller::english().unwrap();
    for space in ['\u{a0}', '\u{2002}', '\u{3000}'] {
        let text = format!("Some{space}background.{space}Sylloge{space}is an internall tool{space}https://example.com/page");
        let found = spell::check_text(&text, &speller);
        let words: Vec<&str> = found.iter().map(|(range, _)| &text[range.clone()]).collect();
        assert_eq!(words, ["Sylloge", "internall"], "with {space:?} between the words");
    }
}
