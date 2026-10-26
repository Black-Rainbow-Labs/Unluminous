//! Autocorrect and AutoFormat as you type (task-2359, TDD 5.4): what Word changes when a key ends a word.
//!
//! Each correction is applied after the key that caused it has been typed, as a change of its own, so the
//! `Ctrl+Z` straight after it gives back exactly what was typed, which is how Word's own undo behaves. The
//! rules are switched one by one (`Rules`), the Proofing level shows them as tick boxes and
//! `autocorrect <rule> on|off` is the agent's half.

use std::sync::Arc;

use crate::edit;
use crate::model::*;
use crate::position::{self, Selection};
use crate::session::{Scope, Session};
use crate::xml::El;

/// Which corrections are on. Every one is on by default, as Word ships them.
#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    pub smart_quotes: bool,
    pub dashes: bool,
    pub ordinals: bool,
    pub fractions: bool,
    pub capitalize_sentences: bool,
    pub capitalize_cells: bool,
    pub two_initials: bool,
    pub days: bool,
    pub replacements: bool,
    pub links: bool,
    pub borders: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            smart_quotes: true,
            dashes: true,
            ordinals: true,
            fractions: true,
            capitalize_sentences: true,
            capitalize_cells: true,
            two_initials: true,
            days: true,
            replacements: true,
            links: true,
            borders: true,
        }
    }
}

/// The rule names, in the order the Proofing level lists them, with what each says.
pub const RULES: &[(&str, &str)] = &[
    ("smart-quotes", "\"Straight quotes\" with “smart quotes”"),
    ("dashes", "Hyphens (--) with dashes (—)"),
    ("ordinals", "Ordinals (1st) with superscript"),
    ("fractions", "Fractions (1/2) with fraction characters (½)"),
    ("capitalize-sentences", "Capitalize first letter of sentences"),
    ("capitalize-cells", "Capitalize first letter of table cells"),
    ("two-initials", "Correct TWo INitial CApitals"),
    ("days", "Capitalize names of days"),
    ("replacements", "Replace text as you type"),
    ("links", "Internet and network paths with hyperlinks"),
    ("borders", "Border lines"),
];

impl Rules {
    /// The switch a rule name names.
    pub fn slot(&mut self, name: &str) -> Option<&mut bool> {
        Some(match name {
            "smart-quotes" => &mut self.smart_quotes,
            "dashes" => &mut self.dashes,
            "ordinals" => &mut self.ordinals,
            "fractions" => &mut self.fractions,
            "capitalize-sentences" => &mut self.capitalize_sentences,
            "capitalize-cells" => &mut self.capitalize_cells,
            "two-initials" => &mut self.two_initials,
            "days" => &mut self.days,
            "replacements" => &mut self.replacements,
            "links" => &mut self.links,
            "borders" => &mut self.borders,
            _ => return None,
        })
    }

    pub fn get(&self, name: &str) -> Option<bool> {
        self.clone().slot(name).map(|on| *on)
    }
}

/// Word's English replacement list, the entries that are corrections rather than symbols first: the
/// symbols, then common misspellings. Matched against the whole word, case kept where it can be.
pub const REPLACEMENTS: &[(&str, &str)] = &[
    ("(c)", "©"),
    ("(r)", "®"),
    ("(tm)", "™"),
    ("(e)", "€"),
    ("...", "…"),
    (":)", "☺"),
    (":(", "☹"),
    ("-->", "→"),
    ("<--", "←"),
    ("==>", "⇒"),
    ("<==", "⇐"),
    ("<=>", "⇔"),
    ("abbout", "about"),
    ("abotu", "about"),
    ("accomodate", "accommodate"),
    ("acheive", "achieve"),
    ("acn", "can"),
    ("adn", "and"),
    ("agian", "again"),
    ("alot", "a lot"),
    ("alwasy", "always"),
    ("andt he", "and the"),
    ("aparent", "apparent"),
    ("aquire", "acquire"),
    ("arguement", "argument"),
    ("assistent", "assistant"),
    ("becuase", "because"),
    ("beggining", "beginning"),
    ("beleive", "believe"),
    ("buisness", "business"),
    ("calender", "calendar"),
    ("cant", "can't"),
    ("comittee", "committee"),
    ("concious", "conscious"),
    ("definately", "definitely"),
    ("didnt", "didn't"),
    ("doesnt", "doesn't"),
    ("dont", "don't"),
    ("embarass", "embarrass"),
    ("enviroment", "environment"),
    ("existance", "existence"),
    ("familar", "familiar"),
    ("foriegn", "foreign"),
    ("freind", "friend"),
    ("goverment", "government"),
    ("hte", "the"),
    ("i", "I"),
    ("im", "I'm"),
    ("independant", "independent"),
    ("isnt", "isn't"),
    ("knowlege", "knowledge"),
    ("liason", "liaison"),
    ("libary", "library"),
    ("maintainance", "maintenance"),
    ("neccessary", "necessary"),
    ("occured", "occurred"),
    ("occurence", "occurrence"),
    ("ocurred", "occurred"),
    ("peice", "piece"),
    ("posession", "possession"),
    ("prefered", "preferred"),
    ("recieve", "receive"),
    ("recomend", "recommend"),
    ("refered", "referred"),
    ("relevent", "relevant"),
    ("seperate", "separate"),
    ("succesful", "successful"),
    ("taht", "that"),
    ("teh", "the"),
    ("thier", "their"),
    ("tomorow", "tomorrow"),
    ("tommorow", "tomorrow"),
    ("truely", "truly"),
    ("untill", "until"),
    ("wasnt", "wasn't"),
    ("wierd", "weird"),
    ("wich", "which"),
    ("wont", "won't"),
    ("wouldnt", "wouldn't"),
    ("yuo", "you"),
];

const DAYS: &[&str] =
    &["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

/// The characters that end a word, after which the word before them is corrected.
pub fn ends_a_word(text: &str) -> bool {
    text.chars().count() == 1
        && text
            .chars()
            .all(|c| c.is_whitespace() || matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')'))
}

/// What a correction does to a paragraph: replace the bytes `from..to` with `with`, and optionally make
/// `with`'s last `superscript` bytes superscript, or link it to an address.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    pub from: usize,
    pub to: usize,
    pub with: String,
    pub superscript: usize,
    pub link: Option<String>,
    /// The rule that made it, for the status bar.
    pub rule: &'static str,
}

/// The correction to make once `typed` has been typed and the caret is at `caret` in a paragraph whose
/// text is `text`. `first_in_cell` says the paragraph is the first of a table cell.
pub fn correction(
    rules: &Rules,
    text: &str,
    caret: usize,
    typed: &str,
    first_in_cell: bool,
) -> Option<Correction> {
    if caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    let before = &text[..caret];
    // A quote is made smart as it is typed.
    if rules.smart_quotes && (typed == "\"" || typed == "'") && before.ends_with(typed) {
        let at = caret - 1;
        let prior = text[..at].chars().next_back();
        let opens = prior.is_none_or(|c| c.is_whitespace() || "([{“‘—–".contains(c));
        let with = match (typed, opens) {
            ("\"", true) => "“",
            ("\"", false) => "”",
            ("'", true) => "‘",
            _ => "’",
        };
        return Some(Correction {
            from: at,
            to: caret,
            with: with.into(),
            superscript: 0,
            link: None,
            rule: "smart-quotes",
        });
    }
    if !ends_a_word(typed) || !before.ends_with(typed) {
        return None;
    }
    let end = caret - typed.len();
    let start = text[..end]
        .rfind(char::is_whitespace)
        .map_or(0, |i| i + text[i..].chars().next().map_or(1, char::len_utf8));
    let word = &text[start..end];
    if word.is_empty() {
        return dash_between(rules, text, end);
    }
    let make = |with: String, superscript: usize, link: Option<String>, rule: &'static str| {
        Some(Correction { from: start, to: end, with, superscript, link, rule })
    };
    if rules.links && is_address(word) {
        let address =
            if word.starts_with("www.") { format!("http://{word}") } else { word.to_owned() };
        return make(word.to_owned(), 0, Some(address), "links");
    }
    if rules.replacements {
        let lower = word.to_lowercase();
        if let Some((_, with)) = REPLACEMENTS.iter().find(|(from, _)| *from == lower) {
            let with = keep_case(word, with);
            if with != word {
                return make(with, 0, None, "replacements");
            }
        }
    }
    if rules.fractions {
        let with = match word {
            "1/2" => Some("½"),
            "1/4" => Some("¼"),
            "3/4" => Some("¾"),
            _ => None,
        };
        if let Some(with) = with {
            return make(with.into(), 0, None, "fractions");
        }
    }
    if rules.ordinals {
        if let Some(suffix) = ordinal_suffix(word) {
            return make(word.to_owned(), suffix, None, "ordinals");
        }
    }
    if rules.dashes {
        if let Some(found) = dash_inside(word) {
            return make(found, 0, None, "dashes");
        }
        if let Some(correction) = dash_between(rules, text, start) {
            return Some(correction);
        }
    }
    if rules.two_initials {
        let chars: Vec<char> = word.chars().collect();
        if chars.len() >= 3
            && chars[0].is_uppercase()
            && chars[1].is_uppercase()
            && chars[2..].iter().all(|c| c.is_lowercase())
        {
            let mut with = String::new();
            with.push(chars[0]);
            with.extend(chars[1].to_lowercase());
            with.extend(&chars[2..]);
            return make(with, 0, None, "two-initials");
        }
    }
    if rules.days && DAYS.contains(&word) {
        return make(capital(word), 0, None, "days");
    }
    let starts_sentence = {
        let prior = text[..start].trim_end();
        prior.is_empty() || prior.ends_with(['.', '!', '?'])
    };
    let in_cell = first_in_cell && text[..start].trim().is_empty();
    let allowed = if in_cell { rules.capitalize_cells } else { rules.capitalize_sentences };
    if allowed
        && starts_sentence
        && word.chars().next().is_some_and(char::is_lowercase)
        && word.chars().all(char::is_alphabetic)
        && !text[..start].trim_end().ends_with("e.g.")
        && !text[..start].trim_end().ends_with("i.e.")
    {
        return make(
            capital(word),
            0,
            None,
            if in_cell { "capitalize-cells" } else { "capitalize-sentences" },
        );
    }
    None
}

/// `with` in the case `word` was typed in: all capitals stays all capitals, a capital first letter stays.
fn keep_case(word: &str, with: &str) -> String {
    let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return with.to_uppercase();
    }
    if letters.first().is_some_and(|c| c.is_uppercase()) {
        return capital(with);
    }
    with.to_owned()
}

fn capital(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// For `1st`, `22nd`, `3rd`, `4th`: the length of the suffix that is made superscript.
fn ordinal_suffix(word: &str) -> Option<usize> {
    let digits = word.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits + 2 != word.len() {
        return None;
    }
    let number: u64 = word[..digits].parse().ok()?;
    let suffix = &word[digits..];
    let right = match (number % 100, number % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    (suffix.eq_ignore_ascii_case(right)).then_some(2)
}

/// `a--b` and `a---b` inside one word become an em dash between the letters.
fn dash_inside(word: &str) -> Option<String> {
    for dashes in ["---", "--"] {
        if let Some(at) = word.find(dashes) {
            let (left, right) = (&word[..at], &word[at + dashes.len()..]);
            if !left.is_empty()
                && !right.is_empty()
                && left.chars().last().is_some_and(char::is_alphanumeric)
                && right.chars().next().is_some_and(char::is_alphanumeric)
            {
                return Some(format!("{left}—{right}"));
            }
        }
    }
    None
}

/// `a -- b`: a double hyphen with a space each side and a word after it becomes an en dash once the word
/// after it ends, which is the moment Word changes it. `word_start` is where that word begins.
fn dash_between(rules: &Rules, text: &str, word_start: usize) -> Option<Correction> {
    if !rules.dashes {
        return None;
    }
    let before = &text[..word_start];
    let trimmed = before.strip_suffix(' ')?;
    let (dashes, with) = if trimmed.ends_with("---") {
        ("---", "—")
    } else if trimmed.ends_with("--") {
        ("--", "–")
    } else {
        return None;
    };
    let at = trimmed.len() - dashes.len();
    let left = &text[..at];
    if !left.ends_with(' ') || left.trim().is_empty() {
        return None;
    }
    Some(Correction {
        from: at,
        to: at + dashes.len(),
        with: with.into(),
        superscript: 0,
        link: None,
        rule: "dashes",
    })
}

/// A web address or a mail address typed as a word.
fn is_address(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    (lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("www.")
        || lower.starts_with("mailto:"))
        && word.len() > 7
        && !word.ends_with('.')
}

/// The border a paragraph of only these characters becomes on Enter, as Word's AutoFormat makes it:
/// the `w:pBdr/w:bottom` value and its size in eighths of a point.
pub fn border_line(text: &str) -> Option<(&'static str, u32)> {
    if text.chars().count() < 3 {
        return None;
    }
    let first = text.chars().next()?;
    if !text.chars().all(|c| c == first) {
        return None;
    }
    Some(match first {
        '-' => ("single", 6),
        '_' => ("single", 18),
        '=' => ("double", 6),
        '*' => ("dotted", 18),
        '~' => ("wave", 6),
        '#' => ("thinThickThinSmallGap", 18),
        _ => return None,
    })
}

impl Session {
    /// Correct the word just ended by `typed`, which has already been typed, as a change of its own.
    /// Answers the rule that applied.
    pub fn autocorrect(&mut self, typed: &str) -> Option<&'static str> {
        if !self.selection.is_empty() {
            return None;
        }
        let head = self.selection.head.clone();
        let paragraph = position::paragraph(&self.document, &head.story, &head.path)?;
        let text = paragraph.flat_text();
        let first_in_cell =
            position::cell_of(position::story(&self.document, &head.story)?, &head.path)
                .is_some_and(|_| head.path.last() == Some(&0));
        let correction =
            correction(&self.autocorrect_rules, &text, head.offset, typed, first_in_cell)?;
        Some(self.apply_correction(correction))
    }

    /// Correct the word before the caret at the end of a paragraph, as Enter does before it splits it.
    pub fn autocorrect_before_enter(&mut self) -> Option<&'static str> {
        if !self.selection.is_empty() {
            return None;
        }
        let head = self.selection.head.clone();
        let paragraph = position::paragraph(&self.document, &head.story, &head.path)?;
        let text = paragraph.flat_text();
        if head.offset != text.len() {
            return None;
        }
        let first_in_cell =
            position::cell_of(position::story(&self.document, &head.story)?, &head.path)
                .is_some_and(|_| head.path.last() == Some(&0));
        let spaced = format!("{text} ");
        let correction =
            correction(&self.autocorrect_rules, &spaced, spaced.len(), " ", first_in_cell)
                .filter(|c| c.to <= text.len())?;
        Some(self.apply_correction(correction))
    }

    fn apply_correction(&mut self, correction: Correction) -> &'static str {
        let rule = correction.rule;
        self.change(Scope::Story, move |document, selection, tracking| {
            let caret = &selection.head;
            let Some(paragraph) = position::paragraph(document, &caret.story, &caret.path) else {
                return selection.clone();
            };
            let (props, _) = edit::props_at(paragraph, correction.from + 1);
            let from = caret.at(correction.from);
            let to = caret.at(correction.to);
            let at = edit::delete_range(document, &from, &to, tracking);
            let end = edit::insert_text(
                document,
                &at,
                &correction.with,
                Some(Arc::clone(&props)),
                tracking,
            );
            if correction.superscript > 0 {
                let start = end.at(end.offset - correction.superscript);
                edit::format_range(document, &start, &end, &|rpr: &mut El| {
                    crate::props::set_val(rpr, "w:vertAlign", Some("superscript"))
                });
            }
            if let Some(address) = &correction.link {
                let main = document.main.clone();
                let id = document.package.rels_mut(&main).add(
                    crate::package::kind::HYPERLINK,
                    address,
                    true,
                );
                let wrap =
                    Wrap::new(El::new("w:hyperlink").with("r:id", id).with("w:history", "1"));
                if let Some(p) = position::paragraph_mut(document, &caret.story, &caret.path) {
                    wrap_range(p, at.offset, end.offset, &wrap);
                }
            }
            let shift = correction.with.len() as isize - (correction.to - correction.from) as isize;
            Selection::caret(caret.at((caret.offset as isize + shift).max(0) as usize))
        });
        rule
    }

    /// AutoFormat on Enter: a paragraph of three or more `-`, `_`, `=`, `*`, `~` or `#` becomes an empty
    /// paragraph with the matching bottom border, as a change of its own before the Enter.
    pub fn border_line_on_enter(&mut self) -> bool {
        if !self.autocorrect_rules.borders || !self.selection.is_empty() {
            return false;
        }
        let head = self.selection.head.clone();
        let Some(paragraph) = position::paragraph(&self.document, &head.story, &head.path) else {
            return false;
        };
        let text = paragraph.flat_text();
        if head.offset != text.len() {
            return false;
        }
        let Some((kind, size)) = border_line(&text) else { return false };
        self.change(Scope::Story, move |document, selection, _| {
            let at = &selection.head;
            if let Some(p) = position::paragraph_mut(document, &at.story, &at.path) {
                p.runs.clear();
                p.marks.retain(|m| m.offset == 0);
                let mut borders = El::new("w:pBdr");
                borders.children.push(crate::xml::Node::El(
                    El::new("w:bottom")
                        .with("w:val", kind)
                        .with("w:sz", size.to_string())
                        .with("w:space", "1")
                        .with("w:color", "auto"),
                ));
                p.props.put(borders);
            }
            Selection::caret(at.at(0))
        });
        true
    }
}

/// Wrap the runs covering `from..to` of a paragraph in `wrap`, splitting runs at both ends.
fn wrap_range(paragraph: &mut Paragraph, from: usize, to: usize, wrap: &Arc<Wrap>) {
    let first = paragraph.split_at(from);
    let last = paragraph.split_at(to);
    for run in &mut paragraph.runs[first..last] {
        run.wraps.push(Arc::clone(wrap));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(text: &str, typed: &str) -> Option<String> {
        let rules = Rules::default();
        let c = correction(&rules, text, text.len(), typed, false)?;
        Some(format!("{}{}{}", &text[..c.from], c.with, &text[c.to..]))
    }

    #[test]
    fn each_rule_makes_words_correction() {
        assert_eq!(fix("He said \"", "\"").as_deref(), Some("He said “"));
        assert_eq!(fix("He said “hi\"", "\"").as_deref(), Some("He said “hi”"));
        assert_eq!(fix("don'", "'").as_deref(), Some("don’"));
        assert_eq!(fix("Read teh ", " ").as_deref(), Some("Read the "));
        assert_eq!(fix("Teh ", " ").as_deref(), Some("The "));
        assert_eq!(fix("Made (c) ", " ").as_deref(), Some("Made © "));
        assert_eq!(fix("Add 1/2 ", " ").as_deref(), Some("Add ½ "));
        assert_eq!(fix("A well--known ", " ").as_deref(), Some("A well—known "));
        assert_eq!(fix("Now -- then ", " ").as_deref(), Some("Now – then "));
        assert_eq!(fix("THe ", " ").as_deref(), Some("The "));
        assert_eq!(fix("See you monday ", " ").as_deref(), Some("See you Monday "));
        assert_eq!(fix("Done. next ", " ").as_deref(), Some("Done. Next "));
        assert_eq!(fix("hello ", " ").as_deref(), Some("Hello "));
        assert_eq!(fix("Ok e.g. this ", " "), None);
        let rules = Rules::default();
        let ordinal = correction(&rules, "The 21st ", 9, " ", false).expect("an ordinal");
        assert_eq!((ordinal.superscript, ordinal.rule), (2, "ordinals"));
        assert!(correction(&rules, "The 21th ", 9, " ", false).is_none());
        let link = correction(&rules, "Go to www.example.com ", 22, " ", false).expect("a link");
        assert_eq!(link.link.as_deref(), Some("http://www.example.com"));
        assert_eq!(border_line("==="), Some(("double", 6)));
        assert_eq!(border_line("-=-"), None);
    }

    #[test]
    fn a_rule_switched_off_does_nothing() {
        let mut rules = Rules::default();
        *rules.slot("replacements").unwrap() = false;
        *rules.slot("capitalize-sentences").unwrap() = false;
        assert!(correction(&rules, "teh ", 4, " ", false).is_none());
        assert_eq!(RULES.len(), 11);
        for (name, _) in RULES {
            assert!(rules.get(name).is_some(), "{name}");
        }
    }
}
