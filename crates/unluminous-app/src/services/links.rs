//! Finding the web address under the pointer in plain text: a terminal's row, a line of a source file.
//!
//! `task-2255`: *"Many of the spot where links are rendered ... should all be clickable and open the link in a
//! new unluminous browser tab."* Rendered Markdown knows where its links are, because the parser recorded them
//! (`components::markdown_text`). A terminal's screen and a file's text are only characters, so an address is
//! found by reading outwards from the pointer to the nearest space, quote or bracket and asking whether what is
//! there starts `http://` or `https://`. Only those two schemes, for the reason the Markdown preview keeps:
//! clicking a word must not be able to reach this machine.

use std::ops::Range;

/// The characters an address cannot hold, so reading outwards stops at them.
fn ends_an_address(character: char) -> bool {
    character.is_whitespace() || matches!(character, '"' | '\'' | '<' | '>' | '`' | '|')
}

/// Characters that end a sentence an address was written in rather than the address, taken off its end.
fn trailing(character: char) -> bool {
    matches!(character, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}')
}

/// The address in `line` that covers byte `at`, with the bytes it occupies.
///
/// A closing bracket at the end is kept when the address opened one, so a Wikipedia address with
/// `(disambiguation)` in it survives being written in a sentence.
///
/// @param line - one line of text
/// @param at - a byte offset into it
pub fn address_at(line: &str, at: usize) -> Option<(Range<usize>, String)> {
    if at > line.len() || !line.is_char_boundary(at) {
        return None;
    }
    let start = line[..at]
        .char_indices()
        .rev()
        .find(|(_, character)| ends_an_address(*character))
        .map(|(index, character)| index + character.len_utf8())
        .unwrap_or(0);
    let end = line[at..]
        .char_indices()
        .find(|(_, character)| ends_an_address(*character))
        .map(|(index, _)| at + index)
        .unwrap_or(line.len());
    let mut word = &line[start..end];
    // An address written in brackets or after one: `(https://example.com)` reads from the scheme.
    let scheme = word.find("https://").or_else(|| word.find("http://"))?;
    word = &word[scheme..];
    let begins = start + scheme;
    while let Some(last) = word.chars().last() {
        let opened = |open: char| word.matches(open).count() >= word.matches(last).count();
        let kept = match last {
            ')' => opened('('),
            ']' => opened('['),
            '}' => opened('{'),
            _ => false,
        };
        if !trailing(last) || kept {
            break;
        }
        word = &word[..word.len() - last.len_utf8()];
    }
    let bytes = begins..begins + word.len();
    let worth_it = word.len() > "https://".len() && at >= bytes.start && at <= bytes.end;
    worth_it.then(|| (bytes, word.to_owned()))
}

/// The same, for a column of a terminal's row, where a column is one character.
///
/// @param row - the row's text
/// @param column - which character
pub fn address_in_column(row: &str, column: usize) -> Option<String> {
    let at = row.char_indices().nth(column).map(|(index, _)| index)?;
    address_at(row, at).map(|(_, address)| address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_found_wherever_in_it_the_pointer_is() {
        let line = "see https://example.com/a?b=1 for more";
        for at in [4, 10, 28] {
            assert_eq!(
                address_at(line, at).map(|(_, address)| address).as_deref(),
                Some("https://example.com/a?b=1"),
                "at {at}"
            );
        }
        assert_eq!(address_at(line, 0), None, "the word before it is not an address");
        assert_eq!(address_at(line, 33), None, "nor the word after it");
    }

    #[test]
    fn the_end_of_a_sentence_is_not_part_of_the_address() {
        let line = "Go to https://example.com/page.";
        assert_eq!(address_at(line, 10).unwrap().1, "https://example.com/page");
        let line = "(https://example.com), then";
        assert_eq!(address_at(line, 5).unwrap().1, "https://example.com");
        let line = "https://en.wikipedia.org/wiki/Rust_(programming_language)";
        assert_eq!(address_at(line, 3).unwrap().1, line, "a bracket the address opened stays");
    }

    #[test]
    fn only_web_addresses_are_found() {
        assert_eq!(address_at("file:///etc/passwd", 3), None);
        assert_eq!(address_at("javascript:alert(1)", 3), None);
        assert_eq!(
            address_at("http://", 3),
            None,
            "a scheme with nothing after it is not an address"
        );
    }

    #[test]
    fn a_terminal_column_is_a_character() {
        let row = "→ Local:   http://localhost:5173/";
        assert_eq!(address_in_column(row, 14).as_deref(), Some("http://localhost:5173/"));
        assert_eq!(address_in_column(row, 0), None);
    }
}
