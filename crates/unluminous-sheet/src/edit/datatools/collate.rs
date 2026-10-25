//! The order Excel gives text when it sorts.
//!
//! Excel compares text with the operating system's word sort: punctuation comes first in the order of its
//! character codes, then the symbols `+ < = >`, then the digits, then letters. The hyphen and the apostrophe
//! are not compared at all unless everything else is equal, and then a word without them comes first. A
//! letter with an accent comes after the same letter without it, and a ligature such as `æ` or `ß` is
//! compared as the letters it stands for. Case is ignored unless the sort asks for it, and then a lower
//! case letter comes first. Every one of those rules was measured against Excel 16; `tests/datatools.rs`
//! holds the corpus.

use std::cmp::Ordering;

/// One letter of a word as the sort sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Unit {
    /// The weight compared first.
    primary: u32,
    /// Which accent the letter carries, zero for none.
    accent: u8,
    /// False for a letter written as itself, true for one made by expanding a ligature, which sorts first when
    /// case counts.
    expanded: bool,
    upper: bool,
}

/// The comparison key of a text: its letters and the places where an ignored mark stood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Key {
    units: Vec<Unit>,
    /// The hyphens (weight 2) and apostrophes (weight 1) as `(letters before it, weight)`.
    marks: Vec<(usize, u8)>,
}

/// Punctuation in the order the sort puts it, from the space to the symbols that follow it.
const PUNCTUATION: &str = " !\"#$%&()*,./:;?@[\\]^_`{|}~+<=>";

/// The base letter and accent of the accented letters a sort meets in Western text. The accent numbers give
/// the order of the accents when the letters are otherwise the same, as Excel orders them: acute, grave,
/// circumflex, diaeresis, caron, macron, breve, tilde, ring, ogonek, cedilla, dot, stroke, double acute.
const ACCENTED: &[(char, char, u8)] = &[
    ('á', 'a', 1),
    ('à', 'a', 2),
    ('â', 'a', 3),
    ('ä', 'a', 4),
    ('ǎ', 'a', 5),
    ('ā', 'a', 6),
    ('ă', 'a', 7),
    ('ã', 'a', 8),
    ('å', 'a', 9),
    ('ą', 'a', 10),
    ('ć', 'c', 1),
    ('ĉ', 'c', 3),
    ('č', 'c', 5),
    ('ç', 'c', 11),
    ('ċ', 'c', 12),
    ('ď', 'd', 5),
    ('đ', 'd', 13),
    ('é', 'e', 1),
    ('è', 'e', 2),
    ('ê', 'e', 3),
    ('ë', 'e', 4),
    ('ě', 'e', 5),
    ('ē', 'e', 6),
    ('ĕ', 'e', 7),
    ('ę', 'e', 10),
    ('ė', 'e', 12),
    ('ĝ', 'g', 3),
    ('ğ', 'g', 7),
    ('ġ', 'g', 12),
    ('ģ', 'g', 11),
    ('ĥ', 'h', 3),
    ('ħ', 'h', 13),
    ('í', 'i', 1),
    ('ì', 'i', 2),
    ('î', 'i', 3),
    ('ï', 'i', 4),
    ('ī', 'i', 6),
    ('ĭ', 'i', 7),
    ('ĩ', 'i', 8),
    ('į', 'i', 10),
    ('ı', 'i', 14),
    ('ĵ', 'j', 3),
    ('ķ', 'k', 11),
    ('ĺ', 'l', 1),
    ('ľ', 'l', 5),
    ('ļ', 'l', 11),
    ('ł', 'l', 13),
    ('ń', 'n', 1),
    ('ň', 'n', 5),
    ('ñ', 'n', 8),
    ('ņ', 'n', 11),
    ('ó', 'o', 1),
    ('ò', 'o', 2),
    ('ô', 'o', 3),
    ('ö', 'o', 4),
    ('ō', 'o', 6),
    ('ŏ', 'o', 7),
    ('õ', 'o', 8),
    ('ø', 'o', 13),
    ('ő', 'o', 15),
    ('ŕ', 'r', 1),
    ('ř', 'r', 5),
    ('ŗ', 'r', 11),
    ('ś', 's', 1),
    ('ŝ', 's', 3),
    ('š', 's', 5),
    ('ş', 's', 11),
    ('ť', 't', 5),
    ('ţ', 't', 11),
    ('ŧ', 't', 13),
    ('ú', 'u', 1),
    ('ù', 'u', 2),
    ('û', 'u', 3),
    ('ü', 'u', 4),
    ('ū', 'u', 6),
    ('ŭ', 'u', 7),
    ('ũ', 'u', 8),
    ('ů', 'u', 9),
    ('ų', 'u', 10),
    ('ű', 'u', 15),
    ('ŵ', 'w', 3),
    ('ý', 'y', 1),
    ('ŷ', 'y', 3),
    ('ÿ', 'y', 4),
    ('ź', 'z', 1),
    ('ž', 'z', 5),
    ('ż', 'z', 12),
];

/// Ligatures that are compared as the letters they stand for.
const LIGATURES: &[(char, &str)] =
    &[('æ', "ae"), ('œ', "oe"), ('ß', "ss"), ('þ', "th"), ('ĳ', "ij")];

fn punctuation_weight(c: char) -> Option<u32> {
    PUNCTUATION.find(c).map(|i| 1 + i as u32)
}

/// The weights of a character that is not a letter with an accent.
fn primary(c: char) -> u32 {
    if let Some(w) = punctuation_weight(c) {
        return w;
    }
    if c.is_ascii_digit() {
        return 100 + (c as u32 - '0' as u32);
    }
    if c.is_ascii_alphabetic() {
        return 200 + (c.to_ascii_lowercase() as u32 - 'a' as u32);
    }
    // Everything else follows the Latin letters, in the order of its character codes. A control character
    // or another kind of space comes before them all.
    if c.is_control() || c.is_whitespace() {
        return 1;
    }
    1000 + c as u32
}

fn units_of(c: char, out: &mut Vec<Unit>) {
    if c == 'İ' {
        out.push(Unit { primary: primary('i'), accent: 15, expanded: false, upper: true });
        return;
    }
    let lower = c.to_lowercase().next().unwrap_or(c);
    let upper = c != lower;
    if let Some((_, expansion)) = LIGATURES.iter().find(|(l, _)| *l == lower) {
        for e in expansion.chars() {
            out.push(Unit { primary: primary(e), accent: 0, expanded: true, upper });
        }
        return;
    }
    if let Some((_, base, accent)) = ACCENTED.iter().find(|(l, _, _)| *l == lower) {
        out.push(Unit { primary: primary(*base), accent: *accent, expanded: false, upper });
        return;
    }
    out.push(Unit { primary: primary(lower), accent: 0, expanded: false, upper });
}

/// The key to compare a text by.
pub(crate) fn key(text: &str) -> Key {
    let mut units = Vec::with_capacity(text.len());
    let mut marks = Vec::new();
    for c in text.chars() {
        match c {
            '\'' | '\u{2019}' => marks.push((units.len(), 1)),
            '-' | '\u{2010}' | '\u{2011}' => marks.push((units.len(), 2)),
            _ => units_of(c, &mut units),
        }
    }
    Key { units, marks }
}

/// Compare two keys. `case_sensitive` adds the last level, where a lower case letter sorts first.
pub(crate) fn compare_keys(a: &Key, b: &Key, case_sensitive: bool) -> Ordering {
    let primary = a.units.iter().map(|u| u.primary).cmp(b.units.iter().map(|u| u.primary));
    if primary != Ordering::Equal {
        return primary;
    }
    let accents = a.units.iter().map(|u| u.accent).cmp(b.units.iter().map(|u| u.accent));
    if accents != Ordering::Equal {
        return accents;
    }
    if case_sensitive {
        let tertiary = |k: &Key| k.units.iter().map(|u| u.upper).collect::<Vec<_>>();
        let case = tertiary(a).cmp(&tertiary(b));
        if case != Ordering::Equal {
            return case;
        }
    }
    a.marks.cmp(&b.marks)
}

/// Compare two texts the way Excel's sort does.
pub(crate) fn compare(a: &str, b: &str, case_sensitive: bool) -> Ordering {
    compare_keys(&key(a), &key(b), case_sensitive)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut words: Vec<&str>, case: bool) -> Vec<&str> {
        words.sort_by(|a, b| compare(a, b, case));
        words
    }

    #[test]
    fn punctuation_digits_and_letters_come_in_that_order() {
        assert_eq!(sorted(vec!["a", "1", "~", "+", "!"], false), vec!["!", "~", "+", "1", "a"]);
    }

    #[test]
    fn hyphens_and_apostrophes_only_break_ties() {
        assert_eq!(
            sorted(vec!["co-op", "coop", "co op", "a-b", "a'b", "ab"], false),
            vec!["ab", "a'b", "a-b", "co op", "coop", "co-op"]
        );
    }

    #[test]
    fn an_accent_sorts_after_the_plain_letter_and_case_is_the_last_level() {
        assert_eq!(
            sorted(vec!["résumé", "Resume", "resume"], false),
            vec!["Resume", "resume", "résumé"]
        );
        assert_eq!(sorted(vec!["B", "a", "A", "b"], true), vec!["a", "A", "b", "B"]);
    }
}
