//! What a kernel's `complete_reply` means, turned into what an editor can insert.
//!
//! A kernel answers a completion with a list of matches and the range of the cell they replace,
//! counted in code points from `cursor_start` to `cursor_end`. Two things about that answer are not
//! what an editor's completion list expects, and this module is where they are put right. Nothing here
//! knows there is a window, so all of it is tested with plain strings. `task-2229`.
//!
//! **The range is the kernel's, not the editor's.** The editor completes the word in front of the
//! caret, and decides what a word is from the language's grammar. A kernel decides for itself:
//! ipykernel answers `%ti` with `%timeit` replacing the `%` as well, and a path in a string with the
//! whole path. [`fit`] turns a match for the kernel's range into the text to insert over the editor's.
//!
//! **A match can carry its arguments.** evcxr, the Rust kernel, answers `checked_add(rhs)` and
//! `println!()`. Inserted as they are, `rhs` would become text in the cell. [`split`] separates the
//! name to insert from the arguments, which the list shows beside it.

use serde_json::Value;

/// One match from a kernel, as an editor offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// What is inserted: the match with its arguments taken off.
    pub insert: String,
    /// What the kernel says the match is, such as `function`, `module` or `instance`, when it says.
    pub kind: Option<String>,
    /// The arguments the kernel showed, such as `rhs` in `checked_add(rhs)`, when there were any.
    pub arguments: Option<String>,
    /// The match is a macro, such as `println!` in Rust.
    pub is_macro: bool,
}

impl Match {
    /// What a completion list shows beside the name: the kind and the arguments, such as
    /// `function (rhs)` or `macro`.
    pub fn detail(&self) -> String {
        let kind = match (self.is_macro, self.kind.as_deref()) {
            (true, _) => "macro".to_owned(),
            (false, Some(kind)) => kind.to_owned(),
            (false, None) => "kernel".to_owned(),
        };
        match self.arguments.as_deref() {
            Some(arguments) if !arguments.is_empty() => format!("{kind} ({arguments})"),
            _ => kind,
        }
    }
}

/// The matches of a `complete_reply`, each with the type the kernel gave it.
///
/// The types are in `metadata._jupyter_types_experimental`, a list beside `matches` whose entries
/// have the match's `text` and its `type`. ipykernel's `text` is the match itself and evcxr's is the
/// match with its arguments shortened to `(…)`, so an entry is paired with its match by position when
/// the two lists are the same length, and otherwise by text.
pub fn matches_of(matches: &[String], metadata: &Value) -> Vec<Match> {
    let types = metadata
        .get("_jupyter_types_experimental")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let same_length = types.len() == matches.len();
    matches
        .iter()
        .enumerate()
        .map(|(at, text)| {
            let entry = match same_length {
                true => types.get(at),
                false => types.iter().find(|entry| entry["text"].as_str() == Some(text.as_str())),
            };
            let kind = entry
                .and_then(|entry| entry["type"].as_str())
                .filter(|kind| !kind.is_empty() && *kind != "<unknown>")
                .map(str::to_owned);
            let (insert, arguments, is_macro) = split(text);
            Match { insert, kind, arguments, is_macro }
        })
        .collect()
}

/// Take the arguments off a match: `checked_add(rhs)` is `checked_add` with `rhs`, `iter()` is `iter`
/// with none, and `println!()` is the macro `println!`.
///
/// Only a match that ends with the closing bracket of the first opening one, and whose part before the
/// bracket is a name, is split. Anything else, such as a path, a string or `collections::`, is
/// inserted as it is.
pub fn split(text: &str) -> (String, Option<String>, bool) {
    let whole = || (text.to_owned(), None, false);
    let Some(open) = text.find('(') else { return whole() };
    if !text.ends_with(')') || open == 0 {
        return whole();
    }
    let name = &text[..open];
    let bare = name.strip_suffix('!').unwrap_or(name);
    let is_name = !bare.is_empty()
        && bare.chars().all(|character| character.is_alphanumeric() || character == '_');
    if !is_name {
        return whole();
    }
    let arguments = text[open + 1..text.len() - 1].trim();
    let arguments = (!arguments.is_empty()).then(|| arguments.to_owned());
    (name.to_owned(), arguments, name.ends_with('!'))
}

/// The text to put over the editor's stem for a match the kernel made for its own range.
///
/// `text` is what both ranges are byte offsets into, `kernel_start` is where the kernel's range
/// starts and `stem_start` where the editor's does. When they start at the same byte the match is
/// inserted as it is. When the kernel's starts first, the text between the two is already in the
/// document, so it is taken off the front of the match, and a match that does not begin with it is
/// `None`: inserting it would change text in front of the word, which accepting a completion must not
/// do. When the kernel's starts later, the text between the two is put back in front of the match.
pub fn fit(insert: &str, text: &str, kernel_start: usize, stem_start: usize) -> Option<String> {
    let between = |from: usize, to: usize| text.get(from..to);
    match kernel_start.cmp(&stem_start) {
        std::cmp::Ordering::Equal => Some(insert.to_owned()),
        std::cmp::Ordering::Less => {
            let already = between(kernel_start, stem_start)?;
            insert.strip_prefix(already).map(str::to_owned)
        }
        std::cmp::Ordering::Greater => {
            let kept = between(stem_start, kernel_start)?;
            Some(format!("{kept}{insert}"))
        }
    }
}

/// The byte in `source` at a position counted in Unicode code points, which is how Jupyter counts a
/// cursor. A position past the end is the end.
pub fn byte_of(source: &str, code_points: usize) -> usize {
    source.char_indices().nth(code_points).map(|(at, _)| at).unwrap_or(source.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_match_with_arguments_inserts_only_its_name() {
        assert_eq!(split("checked_add(rhs)"), ("checked_add".to_owned(), Some("rhs".to_owned()), false));
        assert_eq!(split("iter()"), ("iter".to_owned(), None, false));
        assert_eq!(split("println!()"), ("println!".to_owned(), None, true));
        assert_eq!(split("clamp(min, max)"), ("clamp".to_owned(), Some("min, max".to_owned()), false));
    }

    #[test]
    fn anything_that_is_not_a_call_is_inserted_as_it_is() {
        for text in ["collections::", "data/file.csv", "%timeit", "df", "(x)", "a(b)c", "a.b()"] {
            assert_eq!(split(text), (text.to_owned(), None, false), "{text}");
        }
    }

    #[test]
    fn a_match_for_the_same_range_is_inserted_as_it_is() {
        assert_eq!(fit("checked_add", "x.ch", 2, 2), Some("checked_add".to_owned()));
    }

    #[test]
    fn text_already_in_front_of_the_stem_is_taken_off_the_match() {
        // ipykernel answers `%ti` from the `%`, and the editor's stem is `ti`.
        assert_eq!(fit("%timeit", "%ti", 0, 1), Some("timeit".to_owned()));
        // A path is answered whole, and the editor's stem is the last part.
        assert_eq!(fit("data/file.csv", "open('data/fi", 6, 11), Some("file.csv".to_owned()));
    }

    #[test]
    fn a_match_that_would_change_the_text_in_front_of_the_stem_is_dropped() {
        assert_eq!(fit("np.zeros", "%ti", 0, 1), None);
    }

    #[test]
    fn a_kernel_range_that_starts_later_keeps_the_text_between() {
        assert_eq!(fit("bar", "foo.ba", 4, 0), Some("foo.bar".to_owned()));
    }

    #[test]
    fn each_match_takes_the_type_the_kernel_gave_it() {
        let matches = vec!["checked_add(rhs)".to_owned(), "collections::".to_owned()];
        let metadata = json!({"_jupyter_types_experimental": [
            {"start": 2, "end": 4, "text": "checked_add(…)", "type": "function"},
            {"start": 5, "end": 9, "text": "collections::", "type": "module"},
        ]});
        let found = matches_of(&matches, &metadata);
        assert_eq!(found[0].insert, "checked_add");
        assert_eq!(found[0].detail(), "function (rhs)");
        assert_eq!(found[1].insert, "collections::");
        assert_eq!(found[1].detail(), "module");
    }

    #[test]
    fn a_match_with_no_type_says_it_came_from_the_kernel() {
        let found = matches_of(&["df".to_owned()], &json!({}));
        assert_eq!(found[0].detail(), "kernel");
        let found = matches_of(
            &["df".to_owned()],
            &json!({"_jupyter_types_experimental": [{"text": "df", "type": "<unknown>"}]}),
        );
        assert_eq!(found[0].kind, None);
    }

    #[test]
    fn a_cursor_in_code_points_is_found_as_a_byte() {
        assert_eq!(byte_of("déjà.x", 4), 6);
        assert_eq!(byte_of("ab", 9), 2);
    }
}
