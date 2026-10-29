//! A `SKILL.md`'s frontmatter: the YAML block between the first two `---` lines.
//!
//! Every harness reads the same two keys, `name` and `description`, and ignores a key it does not know, which is
//! what lets [`MARK`] say that Unluminous wrote a copy. Only the shape skills are written in is read here: one
//! `key: value` a line, a value that may be quoted, and a folded or literal block (`>`, `|`, `>-`, `|-`) whose
//! lines are indented under its key. Anything else is kept as it was and never rewritten.

/// The line Unluminous adds to the frontmatter of every copy it installs, so a copy can be told from a skill
/// somebody put there themselves. `mcp::skill` writes `installed-by: unluminous-cli` for the same reason, and that
/// copy is recognised too.
pub const MARK: &str = "installed-by: unluminous";

/// The frontmatter's keys and values, in the order they were written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frontmatter {
    pub fields: Vec<(String, String)>,
}

impl Frontmatter {
    /// The value of `key`, if the frontmatter has it.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
    }

    pub fn name(&self) -> Option<&str> {
        self.get("name").filter(|name| !name.is_empty())
    }

    pub fn description(&self) -> Option<&str> {
        self.get("description").filter(|words| !words.is_empty())
    }
}

/// The text with Windows line breaks made `\n`, which is how every function here reads it.
fn plain(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// The frontmatter's lines and the body after it, or `None` when the text does not open with `---`.
fn split(text: &str) -> Option<(Vec<&str>, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let mut lines = Vec::new();
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        at += line.len();
        let trimmed = line.trim_end_matches('\n');
        if trimmed.trim_end() == "---" {
            return Some((lines, &rest[at..]));
        }
        lines.push(trimmed);
    }
    None
}

/// Read the frontmatter of a `SKILL.md`. Empty when there is none.
pub fn parse(text: &str) -> Frontmatter {
    let text = plain(text);
    let Some((lines, _)) = split(&text) else { return Frontmatter::default() };
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        index += 1;
        if line.starts_with([' ', '\t', '#']) || line.trim().is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        let block = matches!(value, ">" | "|" | ">-" | "|-" | ">+" | "|+");
        let value = match block {
            true => {
                let mut gathered = Vec::new();
                while index < lines.len()
                    && (lines[index].starts_with([' ', '\t']) || lines[index].trim().is_empty())
                {
                    gathered.push(lines[index].trim());
                    index += 1;
                }
                let joiner = if value.starts_with('|') { "\n" } else { " " };
                gathered.join(joiner).trim().to_owned()
            }
            false => unquote(value),
        };
        fields.push((key.trim().to_owned(), value));
    }
    Frontmatter { fields }
}

/// A value without the quotes round it.
fn unquote(value: &str) -> String {
    let quoted = |mark: char| value.len() >= 2 && value.starts_with(mark) && value.ends_with(mark);
    match () {
        _ if quoted('"') => value[1..value.len() - 1].replace("\\\"", "\""),
        _ if quoted('\'') => value[1..value.len() - 1].replace("''", "'"),
        _ => value.to_owned(),
    }
}

/// Whether Unluminous wrote this copy: its frontmatter carries [`MARK`] or the line `mcp::skill` writes.
pub fn has_mark(text: &str) -> bool {
    let text = plain(text);
    let Some((lines, _)) = split(&text) else { return false };
    lines.iter().any(|line| {
        let line = line.trim();
        line == MARK || line == unluminous_cli::mcp::skill::MARK
    })
}

/// The text with [`MARK`] added as the first key of its frontmatter, once. A `SKILL.md` with no frontmatter is
/// given one naming the skill, since a harness refuses a skill without it.
pub fn with_mark(text: &str, name: &str) -> String {
    let text = plain(text);
    if has_mark(&text) {
        return text;
    }
    match text.strip_prefix("---\n") {
        Some(rest) if split(&text).is_some() => format!("---\n{MARK}\n{rest}"),
        _ => format!("---\nname: {name}\ndescription: {name}\n{MARK}\n---\n\n{text}"),
    }
}

/// The text with the mark lines taken out, which is what a copy is compared with its source by.
pub fn without_mark(text: &str) -> String {
    let text = plain(text);
    let Some((lines, body)) = split(&text) else { return text };
    let kept: Vec<&str> = lines
        .into_iter()
        .filter(|line| {
            let line = line.trim();
            line != MARK && line != unluminous_cli::mcp::skill::MARK
        })
        .collect();
    let mut out = String::from("---\n");
    for line in kept {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("---\n");
    out.push_str(body);
    out
}

/// The body of a `SKILL.md`, after its frontmatter.
pub fn body(text: &str) -> String {
    let text = plain(text);
    match split(&text) {
        Some((_, body)) => body.trim_start_matches('\n').to_owned(),
        None => text,
    }
}

/// A new skill's `SKILL.md`: the frontmatter and a starting body that says what goes where.
pub fn starter(name: &str, description: &str) -> String {
    let description = match description.trim() {
        "" => format!("Describe when an agent should use {name}, in one or two sentences."),
        words => words.to_owned(),
    };
    let title = name
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut letters = part.chars();
            letters
                .next()
                .map(|first| first.to_uppercase().chain(letters).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ");
    format!(
        "---\nname: {name}\ndescription: {description}\n---\n\n# {title}\n\n\
         Write what the agent should do, step by step. The `description` above is what the agent reads to decide \
         whether to use this skill, so say plainly when it applies.\n\n\
         ## Steps\n\n1. \n\n## Rules\n\n- \n"
    )
}

/// Whether `name` is a name every harness accepts: lowercase letters, digits and `-`, 1 to 64 of them, not
/// starting or ending with `-`.
pub fn is_a_good_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .chars()
            .all(|letter| letter.is_ascii_lowercase() || letter.is_ascii_digit() || letter == '-')
}

/// `name` made into a good name: lowercase, spaces and other characters turned into `-`.
pub fn tidy_name(name: &str) -> String {
    let mut out = String::new();
    for letter in name.trim().chars() {
        match letter {
            'a'..='z' | '0'..='9' => out.push(letter),
            'A'..='Z' => out.push(letter.to_ascii_lowercase()),
            _ if !out.ends_with('-') && !out.is_empty() => out.push('-'),
            _ => {}
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    out.chars().take(64).collect::<String>().trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_quoted_and_folded_values() {
        let text = "---\r\nname: caveman\r\ndescription: >-\r\n  Talk like a caveman.\r\n  Use few words.\r\nlicense: \"MIT\"\r\n---\r\n# Body\r\n";
        let front = parse(text);
        assert_eq!(front.name(), Some("caveman"));
        assert_eq!(front.description(), Some("Talk like a caveman. Use few words."));
        assert_eq!(front.get("license"), Some("MIT"));
        assert_eq!(body(text), "# Body\n");
    }

    #[test]
    fn the_mark_is_added_once_and_taken_out_again() {
        let text = "---\nname: x\ndescription: y\n---\nbody\n";
        let marked = with_mark(text, "x");
        assert!(has_mark(&marked));
        assert_eq!(with_mark(&marked, "x"), marked, "added once");
        assert_eq!(parse(&marked).name(), Some("x"), "the mark does not hide the other keys");
        assert_eq!(without_mark(&marked), text);
        assert!(!has_mark(text));
    }

    #[test]
    fn the_cli_mark_counts_as_ours() {
        let text = format!("---\n{}\nname: unluminous\n---\n", unluminous_cli::mcp::skill::MARK);
        assert!(has_mark(&text));
    }

    #[test]
    fn a_skill_with_no_frontmatter_is_given_one() {
        let marked = with_mark("# Just words\n", "plain");
        assert_eq!(parse(&marked).name(), Some("plain"));
        assert!(has_mark(&marked));
        assert!(marked.ends_with("# Just words\n"));
    }

    #[test]
    fn names_are_checked_and_tidied() {
        assert!(is_a_good_name("my-skill-2"));
        assert!(!is_a_good_name("My Skill"));
        assert!(!is_a_good_name("-x"));
        assert_eq!(tidy_name("  My Great Skill!! "), "my-great-skill");
        assert!(is_a_good_name(&tidy_name("Ünïcode name")));
    }

    #[test]
    fn a_starter_names_itself_and_says_when_it_applies() {
        let text = starter("release-notes", "");
        let front = parse(&text);
        assert_eq!(front.name(), Some("release-notes"));
        assert!(front.description().unwrap().contains("release-notes"));
        assert!(text.contains("# Release Notes"));
    }
}
