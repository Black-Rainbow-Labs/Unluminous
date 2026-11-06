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

/// The words of `text` the checker underlines, as written.
fn flagged(text: &str, speller: &Speller) -> Vec<String> {
    spell::check_text(text, speller).into_iter().map(|(range, _)| text[range].to_owned()).collect()
}

#[test]
fn places_brands_and_recent_words_are_not_underlined() {
    // task-2517: SCOWL's size 60 list underlined every one of these.
    let speller = Speller::english().unwrap();
    let text = "After pilates we flew to the Dolomites, then Phuket, Mykonos, Santorini, Positano and the \
                Amalfi coast. Machu Picchu was next. I drank kombucha, did burpees with a kettlebell at \
                CrossFit, booked an Airbnb, played Spotify over the Wi-Fi, and tried Zumba and a T-shirt.";
    assert_eq!(flagged(text, &speller), Vec::<String>::new());
}

#[test]
fn common_misspellings_are_still_underlined() {
    // task-2517: the added words were chosen so that these stay underlined. A word one typo away from a
    // much more common word, a contraction written without its apostrophe, and anything on Wikipedia's list
    // of common misspellings were left out.
    let speller = Speller::english().unwrap();
    let wrong = [
        "recieve", "definately", "seperate", "occured", "untill", "wierd", "accomodate", "goverment",
        "beleive", "tommorow", "begining", "neccessary", "acheive", "embarass", "existance", "foriegn",
        "freind", "grammer", "harrass", "independant", "liason", "millenium", "noticable", "occassion",
        "persistant", "posession", "publically", "realy", "reccomend", "relevent", "sieze", "succesful",
        "supercede", "threshhold", "truely", "alot", "teh", "theres", "becuase", "thier", "wich",
    ];
    let accepted: Vec<&str> = wrong.iter().copied().filter(|w| speller.is_correct(w)).collect();
    assert_eq!(accepted, Vec::<&str>::new());
}

#[test]
fn an_added_name_is_only_accepted_capitalized() {
    // A capitalized entry is accepted as written and in capitals, never in lowercase, so a place name cannot
    // hide a typo of an ordinary word.
    let speller = Speller::english().unwrap();
    assert!(speller.is_correct("Phuket"));
    assert!(speller.is_correct("PHUKET"));
    assert!(!speller.is_correct("phuket"));
    assert!(!speller.is_correct("spotify"));
}

#[test]
fn a_possessive_is_correct_when_its_word_is() {
    let mut speller = Speller::english().unwrap();
    assert_eq!(
        flagged("Spotify's app, the Dolomites' peaks and Phuket’s beaches.", &speller),
        Vec::<String>::new()
    );
    assert_eq!(flagged("Frobnicator's output.", &speller), ["Frobnicator's"]);
    speller.add_word("Frobnicator");
    assert_eq!(flagged("Frobnicator's output.", &speller), Vec::<String>::new());
}

#[test]
fn a_hyphenated_word_is_read_whole_when_the_dictionary_has_it_and_by_its_parts_otherwise() {
    let speller = Speller::english().unwrap();
    assert_eq!(flagged("Wi-Fi and a T-shirt.", &speller), Vec::<String>::new());
    assert_eq!(flagged("A well-knwon fact.", &speller), ["knwon"]);
    assert_eq!(flagged("A Wi-Fii network.", &speller), ["Fii"]);
    // Word's non-breaking hyphen joins a word the same way.
    assert_eq!(flagged("Wi\u{2011}Fi works.", &speller), Vec::<String>::new());
}
