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

/// Whether `address` may be opened from a click: only `http` and `https`.
///
/// The rule the Markdown preview keeps, for its reason: a document must not be able to reach this machine because
/// somebody clicked a word in it, and a `javascript:` address handed to a browser runs whatever the document said.
pub fn may_be_opened(address: &str) -> bool {
    let lower = address.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Open `address` in the person's own default browser, outside Unluminous (`task-2263`).
///
/// *"I should be able to right click any link and see options to open in Unluminous Browser, or in default system
/// browser."* On Windows this is `ShellExecuteW` with the `open` verb, which is what Explorer does with an address,
/// so the browser the person chose in Windows' settings opens it. On macOS it is `open`, and elsewhere `xdg-open`.
/// Only `http` and `https`, by [`may_be_opened`].
pub fn open_in_the_default_browser(address: &str) -> Result<(), String> {
    let address = address.trim();
    if !may_be_opened(address) {
        return Err(format!("{address} is not an http or https address, so it is not opened"));
    }
    open_with_the_system(address)
}

#[cfg(windows)]
fn open_with_the_system(address: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |text: &str| text.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let verb = wide("open");
    let file = wide(address);
    // Safe: every pointer is null or into a buffer that outlives the call, and the shell copies what it keeps.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // The function answers a number above 32 when it worked, and an error code at or below it when it did not.
    match result as isize > 32 {
        true => Ok(()),
        false => Err(format!("Windows could not open {address} (error {})", result as isize)),
    }
}

#[cfg(not(windows))]
fn open_with_the_system(address: &str) -> Result<(), String> {
    let program = match cfg!(target_os = "macos") {
        true => "open",
        false => "xdg-open",
    };
    std::process::Command::new(program)
        .arg(address)
        .spawn()
        .map(|_| ())
        .map_err(|problem| format!("{program} could not open {address}: {problem}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_addresses_may_be_opened_in_the_default_browser() {
        assert!(may_be_opened("https://example.com/docs"));
        assert!(may_be_opened(" HTTP://example.com "));
        for refused in ["javascript:alert(1)", "file:///C:/Windows", "mailto:a@b.c", "example.com"]
        {
            assert!(!may_be_opened(refused), "{refused}");
            assert!(open_in_the_default_browser(refused).is_err(), "{refused}");
        }
    }

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
