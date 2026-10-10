//! Positions: a byte offset in a text, and the line and column each protocol names it by.
//!
//! LSP with `positionEncodings: ["utf-8"]` counts a column in bytes; tsserver counts a column in UTF-16
//! code units (and starts both line and column at 1, which the adapter adds). Every function clamps
//! rather than panics: a server that answers a position past the end of a line or past the end of the
//! text is a server that is a revision behind, and the right answer to it is the nearest real place.

use std::ops::Range;

/// The largest char boundary at or before `at`, clamped to the text's length.
///
/// @param text - the text
/// @param at - a byte offset, possibly past the end or inside a character
fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The byte range of one line, without its `\n`. A line past the end is the empty range at the end.
///
/// @param text - the text, with `\n` line breaks
/// @param line - a 0 based line
fn line_bounds(text: &str, line: u32) -> (usize, usize) {
    let mut start = 0;
    for _ in 0..line {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return (text.len(), text.len()),
        }
    }
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    (start, end)
}

/// The start of the line a (clamped) offset is on, and the line number.
fn line_of(text: &str, offset: usize) -> (u32, usize, usize) {
    let at = floor_boundary(text, offset);
    let head = &text[..at];
    let line = head.bytes().filter(|b| *b == b'\n').count() as u32;
    let start = head.rfind('\n').map_or(0, |i| i + 1);
    (line, start, at)
}

/// A byte offset as a 0 based line and a 0 based column counted in bytes.
///
/// @param text - the text
/// @param offset - a byte offset; clamped into the text and down to a character boundary
pub fn line_and_utf8_column(text: &str, offset: usize) -> (u32, u32) {
    let (line, start, at) = line_of(text, offset);
    (line, (at - start) as u32)
}

/// The byte offset of a 0 based line and a byte column. A column past the line's end is the line's end.
///
/// @param text - the text
/// @param line - a 0 based line
/// @param column - a column in bytes
pub fn byte_of_utf8_column(text: &str, line: u32, column: u32) -> usize {
    let (start, end) = line_bounds(text, line);
    start + floor_boundary(&text[start..end], column as usize)
}

/// A byte offset as a 0 based line and a 0 based column counted in UTF-16 code units.
///
/// @param text - the text
/// @param offset - a byte offset; clamped into the text and down to a character boundary
pub fn line_and_utf16(text: &str, offset: usize) -> (u32, u32) {
    let (line, start, at) = line_of(text, offset);
    (line, text[start..at].encode_utf16().count() as u32)
}

/// The byte offset of a 0 based line and a UTF-16 column. A column inside a surrogate pair is the start
/// of that character, and one past the line's end is the line's end.
///
/// @param text - the text
/// @param line - a 0 based line
/// @param column - a column in UTF-16 code units
pub fn byte_of_utf16(text: &str, line: u32, column: u32) -> usize {
    let (start, end) = line_bounds(text, line);
    let mut seen = 0u32;
    for (i, c) in text[start..end].char_indices() {
        let width = c.len_utf16() as u32;
        if seen + width > column {
            return start + i;
        }
        seen += width;
    }
    end
}

/// The one change that turns `old` into `new`: the bytes of `old` that go, and the text that replaces
/// them. Found by the longest common prefix and suffix, both kept on character boundaries of both texts.
///
/// @param old - the text the server has
/// @param new - the text the editor has
pub(crate) fn single_change<'a>(old: &str, new: &'a str) -> (Range<usize>, &'a str) {
    let mut prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    while !(old.is_char_boundary(prefix) && new.is_char_boundary(prefix)) {
        prefix -= 1;
    }
    let room = old.len().min(new.len()) - prefix;
    let mut suffix =
        old.bytes().rev().zip(new.bytes().rev()).take(room).take_while(|(a, b)| a == b).count();
    while !(old.is_char_boundary(old.len() - suffix) && new.is_char_boundary(new.len() - suffix)) {
        suffix -= 1;
    }
    (prefix..old.len() - suffix, &new[prefix..new.len() - suffix])
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "ab\n\u{1F600}x = 1\nlast";

    #[test]
    fn utf8_columns_round_trip_and_clamp() {
        let at = TEXT.find('x').unwrap();
        assert_eq!(line_and_utf8_column(TEXT, at), (1, 4));
        assert_eq!(byte_of_utf8_column(TEXT, 1, 4), at);
        assert_eq!(byte_of_utf8_column(TEXT, 1, 2), 3, "inside the emoji goes down to its start");
        assert_eq!(byte_of_utf8_column(TEXT, 0, 99), 2);
        assert_eq!(byte_of_utf8_column(TEXT, 9, 0), TEXT.len());
        assert_eq!(line_and_utf8_column(TEXT, 999), (2, 4));
    }

    #[test]
    fn utf16_counts_an_emoji_as_two() {
        let at = TEXT.find('x').unwrap();
        assert_eq!(line_and_utf16(TEXT, at), (1, 2));
        assert_eq!(byte_of_utf16(TEXT, 1, 2), at);
        assert_eq!(byte_of_utf16(TEXT, 1, 1), 3, "between the surrogates is the character's start");
        assert_eq!(byte_of_utf16(TEXT, 1, 99), TEXT.find("\nlast").unwrap());
        assert_eq!(byte_of_utf16(TEXT, 7, 0), TEXT.len());
    }

    #[test]
    fn a_change_is_the_prefix_and_suffix_difference() {
        let (range, text) = single_change("hello world", "hello brave world");
        assert_eq!((range, text), (6..6, "brave "));
        let (range, text) = single_change("same", "same");
        assert_eq!((range, text), (4..4, ""));
        let (range, text) = single_change("abcabc", "abc");
        assert_eq!(&"abcabc"[range.clone()], "abc");
        assert_eq!(text, "");
    }

    #[test]
    fn a_change_stays_on_character_boundaries() {
        let old = "a\u{00e9}b";
        let new = "a\u{00e8}b";
        let (range, text) = single_change(old, new);
        assert_eq!(&old[range.clone()], "\u{00e9}");
        assert_eq!(text, "\u{00e8}");
        let old = "x\u{1F600}y\u{1F600}z";
        let new = "x\u{1F600}y\u{1F601}z";
        let (range, text) = single_change(old, new);
        assert_eq!(&old[range], "\u{1F600}");
        assert_eq!(text, "\u{1F601}");
    }
}
