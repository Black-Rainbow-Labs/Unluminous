//! Word's Find and Replace language, searched over one paragraph's plain text.
//!
//! Two modes are read. The plain mode matches the text typed, with Word's `^` special
//! characters (`^t`, `^p`, `^#` and the rest). The wildcard mode is Word's own pattern
//! language and is not regex syntax: `?`, `*`, `[a-z]`, `[!x]`, `{n,m}`, `@`, `<`, `>` and
//! groups written with plain parentheses, with `\(` for a literal parenthesis.
//!
//! A paragraph has no paragraph mark inside its text, so `^p` in a find pattern matches an
//! empty string at the end of the text, and in a replacement it becomes a line feed.
//! A picture is the character U+FFFC, which `^g` matches. The matcher is a small
//! backtracking search over characters, and every offset it returns is a byte offset on a
//! character boundary of the searched text.

/// What to look for and how.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    /// The pattern as typed in the Find box.
    pub text: String,
    /// Upper and lower case letters are different. Wildcard searches are always case sensitive.
    pub match_case: bool,
    /// A match may not touch a letter or digit on either side. Ignored for wildcards.
    pub whole_word: bool,
    /// Read `text` as Word's wildcard language.
    pub wildcards: bool,
}

/// One match: the byte range in the searched text, and the text each group (wildcards only) captured.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// Byte offset where the match starts.
    pub start: usize,
    /// Byte offset just after the match.
    pub end: usize,
    /// The text of each parenthesised group, in the order the groups open. Empty in plain mode.
    pub groups: Vec<String>,
}

/// A class of single characters.
#[derive(Debug, Clone)]
enum Atom {
    Char(char),
    Any,
    Digit,
    Letter,
    Space,
    Class { negate: bool, ranges: Vec<(char, char)> },
}

/// A condition on the position, consuming nothing.
#[derive(Debug, Clone, Copy)]
enum Assert {
    ParagraphEnd,
    WordStart,
    WordEnd,
}

/// One step of a compiled pattern.
#[derive(Debug, Clone)]
enum Item {
    Atom { atom: Atom, min: usize, max: usize },
    Star,
    Open(usize),
    Close(usize),
    Assert(Assert),
}

/// A compiled pattern.
struct Pattern {
    items: Vec<Item>,
    groups: usize,
    has_paragraph_end: bool,
}

/// The text being searched, as characters, and how to compare them.
struct Subject<'a> {
    chars: &'a [char],
    match_case: bool,
}

/// The character a `^x` special stands for, where it is one fixed character.
fn special_char(c: char) -> Option<char> {
    Some(match c {
        't' => '\t',
        'l' => '\u{b}',
        'm' => '\u{c}',
        'n' => '\u{e}',
        's' => '\u{a0}',
        '~' => '\u{2011}',
        '-' => '\u{ad}',
        '+' => '\u{2014}',
        '=' => '\u{2013}',
        '^' => '^',
        'g' => '\u{fffc}',
        _ => return None,
    })
}

/// Compiles a pattern, reading Word's wildcard language when `wildcards` is set.
fn compile(text: &str, wildcards: bool) -> Result<Pattern, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut items: Vec<Item> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut groups = 0usize;
    let mut has_paragraph_end = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        let one = |atom: Atom| Item::Atom { atom, min: 1, max: 1 };
        if c == '^' && i < chars.len() {
            let s = chars[i];
            let item = match s {
                'p' => {
                    has_paragraph_end = true;
                    Some(Item::Assert(Assert::ParagraphEnd))
                }
                '#' => Some(one(Atom::Digit)),
                '$' => Some(one(Atom::Letter)),
                '?' => Some(one(Atom::Any)),
                'w' => Some(Item::Atom { atom: Atom::Space, min: 1, max: usize::MAX }),
                other => special_char(other).map(|x| one(Atom::Char(x))),
            };
            if let Some(item) = item {
                items.push(item);
                i += 1;
                continue;
            }
        }
        if !wildcards {
            items.push(one(Atom::Char(c)));
            continue;
        }
        match c {
            '\\' if i < chars.len() => {
                items.push(one(Atom::Char(chars[i])));
                i += 1;
            }
            '?' => items.push(one(Atom::Any)),
            '*' => items.push(Item::Star),
            '<' => items.push(Item::Assert(Assert::WordStart)),
            '>' => items.push(Item::Assert(Assert::WordEnd)),
            '(' => {
                open.push(groups);
                items.push(Item::Open(groups));
                groups += 1;
            }
            ')' => match open.pop() {
                Some(k) => items.push(Item::Close(k)),
                None => {
                    return Err("The search text has a closing parenthesis with no opening one."
                        .to_string())
                }
            },
            '[' => {
                let (atom, next) = parse_class(&chars, i)?;
                items.push(one(atom));
                i = next;
            }
            '{' => {
                let (min, max, next) = parse_count(&chars, i)?;
                set_repeat(&mut items, min, max)?;
                i = next;
            }
            '@' => set_repeat(&mut items, 1, usize::MAX)?,
            other => items.push(one(Atom::Char(other))),
        }
    }
    if !open.is_empty() {
        return Err("The search text has an opening parenthesis that is never closed.".to_string());
    }
    Ok(Pattern { items, groups, has_paragraph_end })
}

/// Reads a `[...]` class whose `[` was just before `i`; returns the class and the index after `]`.
fn parse_class(chars: &[char], mut i: usize) -> Result<(Atom, usize), String> {
    let unclosed = || "The search text has a square bracket that is never closed.".to_string();
    let mut negate = false;
    if chars.get(i) == Some(&'!') {
        negate = true;
        i += 1;
    }
    let mut ranges = Vec::new();
    loop {
        let c = *chars.get(i).ok_or_else(unclosed)?;
        i += 1;
        if c == ']' {
            break;
        }
        let lo = if c == '\\' {
            let n = *chars.get(i).ok_or_else(unclosed)?;
            i += 1;
            n
        } else {
            c
        };
        if chars.get(i) == Some(&'-') && chars.get(i + 1).is_some_and(|&x| x != ']') {
            let hi = chars[i + 1];
            i += 2;
            if hi < lo {
                return Err("A range in square brackets runs backwards.".to_string());
            }
            ranges.push((lo, hi));
        } else {
            ranges.push((lo, lo));
        }
    }
    if ranges.is_empty() {
        return Err("The square brackets in the search text are empty.".to_string());
    }
    Ok((Atom::Class { negate, ranges }, i))
}

/// Reads the inside of a `{n}`, `{n,}` or `{n,m}` whose `{` was just before `i`.
fn parse_count(chars: &[char], i: usize) -> Result<(usize, usize, usize), String> {
    let close = chars[i..]
        .iter()
        .position(|&c| c == '}')
        .ok_or_else(|| "The search text has a curly bracket that is never closed.".to_string())?;
    let inside: String = chars[i..i + close].iter().collect();
    let bad = || {
        format!("The count {{{inside}}} in the search text is not a number or a range of numbers.")
    };
    let (min, max) = match inside.split_once(',') {
        None => {
            let n: usize = inside.trim().parse().map_err(|_| bad())?;
            (n, n)
        }
        Some((a, b)) => {
            let min: usize = a.trim().parse().map_err(|_| bad())?;
            let max = if b.trim().is_empty() {
                usize::MAX
            } else {
                b.trim().parse().map_err(|_| bad())?
            };
            (min, max)
        }
    };
    if min > max {
        return Err(bad());
    }
    Ok((min, max, i + close + 1))
}

/// Applies a repeat count to the item just before it.
fn set_repeat(items: &mut [Item], new_min: usize, new_max: usize) -> Result<(), String> {
    match items.last_mut() {
        Some(Item::Atom { min, max, .. }) => {
            *min = new_min;
            *max = new_max;
            Ok(())
        }
        _ => {
            Err("A count or @ in the search text has no character before it to repeat.".to_string())
        }
    }
}

/// True when a character counts as part of a word.
fn is_word(c: char) -> bool {
    c.is_alphanumeric()
}

/// Compares two characters, folding case when the search is not case sensitive.
fn same(a: char, b: char, match_case: bool) -> bool {
    a == b || (!match_case && a.to_lowercase().eq(b.to_lowercase()))
}

/// True when one character is accepted by an atom.
fn atom_accepts(atom: &Atom, c: char, match_case: bool) -> bool {
    match atom {
        Atom::Char(x) => same(*x, c, match_case),
        Atom::Any => true,
        Atom::Digit => c.is_ascii_digit(),
        Atom::Letter => c.is_alphabetic(),
        Atom::Space => c == ' ' || c == '\t' || c == '\u{a0}',
        Atom::Class { negate, ranges } => {
            ranges.iter().any(|&(lo, hi)| c >= lo && c <= hi) != *negate
        }
    }
}

/// True when a position satisfies an assertion.
fn assertion_holds(a: Assert, s: &Subject, p: usize) -> bool {
    let before = p.checked_sub(1).map(|q| s.chars[q]);
    let after = s.chars.get(p).copied();
    match a {
        Assert::ParagraphEnd => p == s.chars.len(),
        Assert::WordStart => before.map_or(true, |c| !is_word(c)) && after.is_some_and(is_word),
        Assert::WordEnd => before.is_some_and(is_word) && after.map_or(true, |c| !is_word(c)),
    }
}

/// Matches `items[i..]` at character position `p`; returns the end position of the whole match.
fn run(
    items: &[Item],
    i: usize,
    p: usize,
    s: &Subject,
    caps: &mut Vec<Option<(usize, usize)>>,
    starts: &mut Vec<usize>,
) -> Option<usize> {
    let Some(item) = items.get(i) else { return Some(p) };
    match item {
        Item::Assert(a) => {
            if assertion_holds(*a, s, p) {
                run(items, i + 1, p, s, caps, starts)
            } else {
                None
            }
        }
        Item::Open(k) => {
            let old = starts[*k];
            starts[*k] = p;
            let r = run(items, i + 1, p, s, caps, starts);
            if r.is_none() {
                starts[*k] = old;
            }
            r
        }
        Item::Close(k) => {
            let old = caps[*k];
            caps[*k] = Some((starts[*k], p));
            let r = run(items, i + 1, p, s, caps, starts);
            if r.is_none() {
                caps[*k] = old;
            }
            r
        }
        Item::Star => {
            for k in 0..=(s.chars.len() - p) {
                if let Some(e) = run(items, i + 1, p + k, s, caps, starts) {
                    return Some(e);
                }
            }
            None
        }
        Item::Atom { atom, min, max } => {
            let mut n = 0;
            while n < *max
                && p + n < s.chars.len()
                && atom_accepts(atom, s.chars[p + n], s.match_case)
            {
                n += 1;
            }
            if n < *min {
                return None;
            }
            for k in (*min..=n).rev() {
                if let Some(e) = run(items, i + 1, p + k, s, caps, starts) {
                    return Some(e);
                }
            }
            None
        }
    }
}

/// Every match of `query` in `text`, left to right, not overlapping.
pub fn find_all(query: &Query, text: &str) -> Result<Vec<Found>, String> {
    let pattern = compile(&query.text, query.wildcards)?;
    if query.text.is_empty() {
        return Ok(Vec::new());
    }
    let marks: Vec<(usize, char)> = text.char_indices().collect();
    let chars: Vec<char> = marks.iter().map(|&(_, c)| c).collect();
    let subject = Subject { chars: &chars, match_case: query.match_case || query.wildcards };
    let byte = |p: usize| marks.get(p).map_or(text.len(), |&(b, _)| b);
    let whole_word = query.whole_word && !query.wildcards;
    let mut found = Vec::new();
    let mut start = 0;
    while start <= chars.len() {
        let mut caps = vec![None; pattern.groups];
        let mut starts = vec![0; pattern.groups];
        let end = run(&pattern.items, 0, start, &subject, &mut caps, &mut starts);
        let Some(end) = end else {
            start += 1;
            continue;
        };
        let empty = end == start;
        let edge_ok = !whole_word
            || (start.checked_sub(1).map_or(true, |q| !is_word(chars[q]))
                && chars.get(end).map_or(true, |&c| !is_word(c)));
        if (empty && !pattern.has_paragraph_end) || !edge_ok {
            start += 1;
            continue;
        }
        let groups = caps
            .iter()
            .map(|c| c.map_or(String::new(), |(a, b)| chars[a..b].iter().collect()))
            .collect();
        found.push(Found { start: byte(start), end: byte(end), groups });
        start = if empty { start + 1 } else { end };
    }
    Ok(found)
}

/// The replacement text for one match: `with` with its special characters and, for wildcards, `\1`..`\9` and `^&` filled in from `found` and `text`.
pub fn replacement(query: &Query, with: &str, text: &str, found: &Found) -> String {
    let matched = text.get(found.start..found.end).unwrap_or("");
    let chars: Vec<char> = with.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if c == '^' && i < chars.len() {
            let s = chars[i];
            if s == '&' {
                out.push_str(matched);
                i += 1;
                continue;
            }
            if s == 'p' {
                out.push('\n');
                i += 1;
                continue;
            }
            if let Some(x) = special_char(s) {
                out.push(x);
                i += 1;
                continue;
            }
        }
        if c == '\\' && query.wildcards && i < chars.len() {
            let s = chars[i];
            i += 1;
            match s.to_digit(10) {
                Some(d) if d >= 1 => {
                    if let Some(g) = found.groups.get(d as usize - 1) {
                        out.push_str(g);
                    }
                }
                _ => out.push(s),
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> Query {
        Query { text: text.to_string(), ..Query::default() }
    }

    fn wild(text: &str) -> Query {
        Query { text: text.to_string(), wildcards: true, ..Query::default() }
    }

    fn spans(q: &Query, text: &str) -> Vec<(usize, usize)> {
        find_all(q, text).unwrap().iter().map(|f| (f.start, f.end)).collect()
    }

    fn words<'a>(q: &Query, text: &'a str) -> Vec<&'a str> {
        spans(q, text).iter().map(|&(a, b)| &text[a..b]).collect()
    }

    #[test]
    fn plain_text_ignores_case_by_default() {
        assert_eq!(spans(&plain("hello"), "Say HELLO, hello"), vec![(4, 9), (11, 16)]);
    }

    #[test]
    fn match_case_respects_case() {
        let q = Query { match_case: true, ..plain("hello") };
        assert_eq!(spans(&q, "Say HELLO, hello"), vec![(11, 16)]);
    }

    #[test]
    fn matches_do_not_overlap() {
        assert_eq!(spans(&plain("aa"), "aaaaa"), vec![(0, 2), (2, 4)]);
    }

    #[test]
    fn whole_word_rejects_matches_inside_words() {
        let q = Query { whole_word: true, ..plain("cat") };
        assert_eq!(spans(&q, "cat concat cats cat"), vec![(0, 3), (16, 19)]);
    }

    #[test]
    fn tab_special_matches_a_tab() {
        assert_eq!(spans(&plain("^t"), "a\tb"), vec![(1, 2)]);
    }

    #[test]
    fn digit_special_matches_digits() {
        assert_eq!(words(&plain("^#"), "a1b22"), vec!["1", "2", "2"]);
    }

    #[test]
    fn letter_special_matches_letters() {
        assert_eq!(words(&plain("^$"), "a1b"), vec!["a", "b"]);
    }

    #[test]
    fn any_character_special_matches_everything() {
        assert_eq!(spans(&plain("^?"), "ab").len(), 2);
        assert_eq!(words(&plain("a^?c"), "abc axc"), vec!["abc", "axc"]);
    }

    #[test]
    fn white_space_special_matches_a_run() {
        assert_eq!(words(&plain("a^wb"), "a  \u{a0}\tb ab"), vec!["a  \u{a0}\tb"]);
    }

    #[test]
    fn paragraph_mark_matches_only_at_the_end() {
        assert_eq!(spans(&plain("^p"), "abc"), vec![(3, 3)]);
        assert_eq!(spans(&plain("c^p"), "abc"), vec![(2, 3)]);
        assert!(spans(&plain("b^p"), "abc").is_empty());
    }

    #[test]
    fn caret_caret_is_a_literal_caret() {
        assert_eq!(spans(&plain("^^"), "a^b"), vec![(1, 2)]);
    }

    #[test]
    fn other_fixed_specials_match_their_characters() {
        assert_eq!(spans(&plain("^s"), "a\u{a0}"), vec![(1, 3)]);
        assert_eq!(spans(&plain("^l"), "a\u{b}"), vec![(1, 2)]);
        assert_eq!(spans(&plain("^+^="), "x\u{2014}\u{2013}"), vec![(1, 7)]);
        assert_eq!(spans(&plain("^g"), "a\u{fffc}b"), vec![(1, 4)]);
    }

    #[test]
    fn wildcard_question_mark_matches_one_character() {
        assert_eq!(words(&wild("c?t"), "cat cut ct"), vec!["cat", "cut"]);
    }

    #[test]
    fn wildcard_star_is_lazy() {
        assert_eq!(words(&wild("b*d"), "bad bead"), vec!["bad", "bead"]);
    }

    #[test]
    fn wildcard_range_class() {
        assert_eq!(words(&wild("[a-c]"), "abcd"), vec!["a", "b", "c"]);
    }

    #[test]
    fn wildcard_negated_class() {
        assert_eq!(words(&wild("[!0-9]"), "a1b"), vec!["a", "b"]);
    }

    #[test]
    fn wildcard_exact_count() {
        assert_eq!(spans(&wild("a{2}"), "a aa aaa"), vec![(2, 4), (5, 7)]);
    }

    #[test]
    fn wildcard_range_count() {
        assert_eq!(words(&wild("a{1,3}"), "aaaa"), vec!["aaa", "a"]);
        assert_eq!(words(&wild("a{2,}"), "a aaaa"), vec!["aaaa"]);
    }

    #[test]
    fn wildcard_at_means_one_or_more() {
        assert_eq!(words(&wild("[0-9]@"), "ab 123 4"), vec!["123", "4"]);
    }

    #[test]
    fn wildcard_word_edges() {
        assert_eq!(spans(&wild("<the>"), "the other the"), vec![(0, 3), (10, 13)]);
        assert_eq!(words(&wild("<t?e"), "the other"), vec!["the"]);
        assert_eq!(words(&wild("e>"), "the other"), vec!["e"]);
    }

    #[test]
    fn wildcard_is_always_case_sensitive() {
        assert_eq!(words(&wild("abc"), "ABC abc"), vec!["abc"]);
    }

    #[test]
    fn groups_are_captured_and_swapped_in_a_replacement() {
        let q = wild("([a-z]@) ([a-z]@)");
        let text = "hello world";
        let found = find_all(&q, text).unwrap();
        assert_eq!(found[0].groups, vec!["hello", "world"]);
        assert_eq!(replacement(&q, r"\2 \1", text, &found[0]), "world hello");
    }

    #[test]
    fn escaped_parentheses_are_literal() {
        assert_eq!(spans(&wild(r"\(a\)"), "f(a)"), vec![(1, 4)]);
    }

    #[test]
    fn nested_groups_number_by_opening_order() {
        let q = wild("((a)b)");
        let found = find_all(&q, "ab").unwrap();
        assert_eq!(found[0].groups, vec!["ab", "a"]);
    }

    #[test]
    fn ampersand_special_inserts_the_match() {
        let q = plain("foo");
        let found = find_all(&q, "a foo").unwrap();
        assert_eq!(replacement(&q, "<^&>", "a foo", &found[0]), "<foo>");
    }

    #[test]
    fn replacement_specials_are_expanded() {
        let q = plain("x");
        let found = Found { start: 0, end: 1, groups: vec![] };
        assert_eq!(replacement(&q, "a^tb^pc^^d^+", "x", &found), "a\tb\nc^d\u{2014}");
    }

    #[test]
    fn backslash_is_literal_in_a_plain_replacement() {
        let q = plain("x");
        let found = Found { start: 0, end: 1, groups: vec![] };
        assert_eq!(replacement(&q, r"\1", "x", &found), r"\1");
    }

    #[test]
    fn multibyte_text_gives_byte_offsets() {
        let text = "café naïve";
        assert_eq!(spans(&plain("naïve"), text), vec![(6, 12)]);
        assert_eq!(spans(&plain("CAFÉ"), text), vec![(0, 5)]);
        assert_eq!(spans(&wild("na?ve"), text), vec![(6, 12)]);
        assert_eq!(words(&wild("<*>"), text), vec!["café", "naïve"]);
    }

    #[test]
    fn unclosed_square_bracket_is_an_error() {
        let e = find_all(&wild("[abc"), "abc").unwrap_err();
        assert!(e.contains("square bracket"), "{e}");
    }

    #[test]
    fn unclosed_curly_bracket_and_group_are_errors() {
        assert!(find_all(&wild("a{2"), "aa").is_err());
        assert!(find_all(&wild("(a"), "a").is_err());
        assert!(find_all(&wild("a)"), "a").is_err());
        assert!(find_all(&wild("{2}"), "a").is_err());
    }

    #[test]
    fn empty_query_finds_nothing() {
        assert!(spans(&plain(""), "abc").is_empty());
    }
}
