//! Whether a transcript starts with "Sable", and the transcript with it taken off. Sable's server check
//! (`crates/sable-server/src/utils/phonetic.rs`): one of the first three words is a known spelling, or a
//! word whose Double Metaphone code is `SPL` one edit from "sable", with only lead ins such as "hey" before
//! it. The words most often confused with it (table, cable, fable) never match.
//!
//! A wake the detector fired on is dictation only when the transcript agrees, or the detector's score was
//! well over its threshold; see [`confirm`]. That second check is what keeps "set the table" from opening
//! the chat.

use rphonetic::DoubleMetaphone;

/// Spellings a speech model writes for "Sable".
pub const WAKE_SPELLINGS: &[&str] =
    &["sable", "sabel", "sabal", "sable's", "saybl", "sabol", "sables", "sabil"];

/// Words that sound close to "Sable" and must never count as it.
pub const CONFUSABLE: &[&str] = &[
    "table",
    "cable",
    "stable",
    "fable",
    "able",
    "label",
    "saber",
    "sabre",
    "sabrina",
    "sabbatical",
];

/// Words people put before the name ("hey Sable"). They are removed with it.
const LEAD_INS: &[&str] = &["hey", "hi", "okay", "ok", "o.k.", "yo", "hello", "um", "uh", "so"];

/// How many leading words may hold the wake word.
const WAKE_WINDOW: usize = 3;

/// How far over its threshold the detector's score must be for a wake to stand without the transcript.
const STRICTER_BY: f32 = 0.1;

/// Lower case, letters, digits and apostrophes only.
/// @param word - one word as written
fn clean(word: &str) -> String {
    word.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect::<String>().to_lowercase()
}

/// Levenshtein distance between two short words.
/// @param a - first word
/// @param b - second word
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut row = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            row[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(row[j] + 1);
        }
        prev = row;
    }
    prev[b.len()]
}

/// Whether one word is a spelling of "Sable".
/// @param word - one word, in any case, with or without punctuation
pub fn is_wake_word(word: &str) -> bool {
    let w = clean(word);
    if w.is_empty() || CONFUSABLE.contains(&w.as_str()) {
        return false;
    }
    if WAKE_SPELLINGS.contains(&w.as_str()) {
        return true;
    }
    DoubleMetaphone::default().double_metaphone(&w).primary() == "SPL"
        && edit_distance(&w, "sable") <= 1
}

/// The transcript with the wake word, the lead ins before it and the punctuation after it taken off, or
/// `None` when none of its first three words is "Sable". "Sable" later in a sentence is left alone.
/// @param text - a transcript
pub fn strip_wake(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let index = words.iter().take(WAKE_WINDOW).position(|w| is_wake_word(w))?;
    if !words[..index].iter().all(|w| LEAD_INS.contains(&clean(w).as_str()) || clean(w).is_empty())
    {
        return None;
    }
    Some(capitalise(&rest_after(&words, index)))
}

/// What a wake turns into, given the transcript of the audio from the pre roll on and the detector's score.
///
/// `Some(text)` with the wake word taken off when the transcript says "Sable", or when the detector was sure
/// enough on its own; then a first word that is a near miss of "Sable" (the model wrote "Fable") is taken off
/// too. `None` is a false wake, which is dropped without a word in the prompt.
/// @param text - the transcript so far
/// @param score - the detector's score for the wake
/// @param threshold - the detector's threshold
pub fn confirm(text: &str, score: f32, threshold: f32) -> Option<String> {
    if let Some(stripped) = strip_wake(text) {
        return Some(stripped);
    }
    if score < (threshold + STRICTER_BY).min(0.95) {
        return None;
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    match words.first() {
        Some(first)
            if CONFUSABLE.contains(&clean(first).as_str())
                || edit_distance(&clean(first), "sable") <= 2 =>
        {
            Some(capitalise(&rest_after(&words, 0)))
        }
        _ => Some(text.trim().to_owned()),
    }
}

/// The words after `index`, without the punctuation that followed the word at `index`.
/// @param words - the transcript's words
/// @param index - the wake word's place
fn rest_after(words: &[&str], index: usize) -> String {
    let rest = words[index + 1..].join(" ");
    rest.trim_start_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace()).to_owned()
}

/// Upper cases the first letter.
/// @param text - the text
fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings_of_sable_match_and_confusable_words_never_do() {
        for w in WAKE_SPELLINGS {
            assert!(is_wake_word(w), "{w} should match");
        }
        assert!(is_wake_word("Sable,"));
        for w in CONFUSABLE {
            assert!(!is_wake_word(w), "{w} must not match");
        }
    }

    #[test]
    fn the_wake_word_is_taken_off_and_the_rest_kept() {
        assert_eq!(
            strip_wake("Sable, fix the failing test.").as_deref(),
            Some("Fix the failing test.")
        );
        assert_eq!(
            strip_wake("Hey Sable what changed in git").as_deref(),
            Some("What changed in git")
        );
        assert_eq!(strip_wake("Sable.").as_deref(), Some(""));
        assert_eq!(strip_wake("the table is set"), None);
        assert_eq!(strip_wake("I like sable fur"), None);
    }

    #[test]
    fn a_wake_the_transcript_does_not_back_needs_a_sure_detector() {
        // At the default threshold of 0.6, 0.65 is not sure enough and 0.9 is.
        assert_eq!(confirm("set the table for dinner", 0.65, 0.6), None);
        assert_eq!(confirm("Fable, open the readme", 0.9, 0.6).as_deref(), Some("Open the readme"));
        assert_eq!(confirm("Sable open the readme", 0.61, 0.6).as_deref(), Some("Open the readme"));
    }
}
