//! Fields worked out again, as Word's Update Field does (task-2359, TDD 5.4).
//!
//! A field is a code (`PAGEREF _Ref123 \h`, `= SUM(ABOVE) \# "0.00"`, `DATE \@ "dddd, MMMM d, yyyy"`)
//! and a result the file stores. [`evaluate`] answers the new result for a code in a document, or `None`
//! for a field it does not work out, which keeps its stored result as Word keeps the result of a field it
//! cannot update. The general switches (`\*`, `\#`, `\@`) apply to every kind.
//!
//! Kinds: `PAGE`, `NUMPAGES`, `SECTION`, `SECTIONPAGES`, `REF`, `PAGEREF`, `NOTEREF`, `SEQ`, `STYLEREF`,
//! `NUMWORDS`, `NUMCHARS`, `AUTHOR`, `TITLE`, `SUBJECT`, `KEYWORDS`, `COMMENTS`, `LASTSAVEDBY`,
//! `FILENAME`, `FILESIZE`, `DOCPROPERTY`, `DATE`, `TIME`, `CREATEDATE`, `SAVEDATE`, `PRINTDATE`,
//! `HYPERLINK`, `=`, `IF` and `COMPARE`.

use std::collections::HashMap;

use crate::model::{Block, Content, Document, Paragraph};
use crate::position::Path;

/// A field code read into its kind, its arguments and its switches.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Code {
    pub kind: String,
    pub arguments: Vec<String>,
    /// Each switch and its argument when it takes one: `("\\*", "Upper")`, `("\\h", "")`.
    pub switches: Vec<(String, String)>,
}

impl Code {
    /// The argument of a switch, when the code has it.
    pub fn switch(&self, name: &str) -> Option<&str> {
        self.switches.iter().find(|(s, _)| s.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn has(&self, name: &str) -> bool {
        self.switch(name).is_some()
    }
}

/// The switches that take an argument after them.
const WITH_ARGUMENT: &[&str] =
    &["\\*", "\\#", "\\@", "\\c", "\\r", "\\s", "\\l", "\\o", "\\t", "\\b", "\\d", "\\f", "\\a"];

/// Read a field code: words split at spaces outside quotes, a quoted word kept whole without its
/// quotes, `\x` a switch.
pub fn parse(code: &str) -> Code {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in code.trim().chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
                any = false;
            }
            c => current.push(c),
        }
    }
    if any || !current.is_empty() {
        words.push(current);
    }
    let mut code = Code::default();
    let mut words = words.into_iter().peekable();
    let Some(first) = words.next() else { return code };
    // `=2+3` has no space after its `=`.
    if let Some(rest) = first.strip_prefix('=').filter(|r| !r.is_empty()) {
        code.kind = "=".into();
        code.arguments.push(rest.to_owned());
    } else {
        code.kind = first.to_uppercase();
    }
    while let Some(word) = words.next() {
        if word.starts_with('\\') && word.len() >= 2 {
            let name = word[..2].to_owned();
            let attached = word[2..].to_owned();
            let value = if !attached.is_empty() {
                attached
            } else if WITH_ARGUMENT.iter().any(|w| w.eq_ignore_ascii_case(&name)) {
                words.next_if(|next| !next.starts_with('\\')).unwrap_or_default()
            } else {
                String::new()
            };
            code.switches.push((name, value));
        } else {
            code.arguments.push(word);
        }
    }
    code
}

/// Everything a field may need besides its code.
#[derive(Default)]
pub struct Env<'a> {
    pub document: Option<&'a Document>,
    /// The time to update to, `2026-10-24T14:30:00Z`.
    pub now: String,
    /// The file's name and full path, for `FILENAME`, and its size in bytes.
    pub file: Option<(String, String)>,
    pub file_size: Option<u64>,
    /// The page number each body paragraph starts on, by its path, and the page count.
    pub pages: HashMap<Path, u32>,
    pub page_count: u32,
    /// The paragraph the field is in and the page it is on.
    pub at: Path,
    pub page: u32,
    /// `SEQ` counters, by identifier, as the update walks the document in order.
    pub sequences: HashMap<String, i64>,
    /// Words and characters, for `NUMWORDS` and `NUMCHARS`.
    pub words: usize,
    pub characters: usize,
    /// The values of the cells of the table the field is in, by row and column, for `= SUM(ABOVE)`.
    pub cells: Option<(Vec<Vec<String>>, usize, usize)>,
}

/// The result of a field, or `None` when it keeps the one stored.
pub fn evaluate(code: &str, env: &mut Env) -> Option<String> {
    let code = parse(code);
    let raw = evaluate_kind(&code, env)?;
    Some(general_switches(&code, raw))
}

fn evaluate_kind(code: &Code, env: &mut Env) -> Option<String> {
    let document = env.document;
    match code.kind.as_str() {
        "PAGE" => Some(env.page.to_string()),
        "NUMPAGES" | "SECTIONPAGES" => Some(env.page_count.to_string()),
        "SECTION" => Some(section_of(document?, &env.at).to_string()),
        "NUMWORDS" => Some(env.words.to_string()),
        "NUMCHARS" => Some(env.characters.to_string()),
        "AUTHOR" => property(document?, "dc:creator"),
        "TITLE" => property(document?, "dc:title"),
        "SUBJECT" => property(document?, "dc:subject"),
        "KEYWORDS" => property(document?, "cp:keywords"),
        "COMMENTS" => property(document?, "dc:description"),
        "LASTSAVEDBY" => property(document?, "cp:lastModifiedBy"),
        "DOCPROPERTY" => docproperty(document?, code.arguments.first()?),
        "FILENAME" => {
            let (name, path) = env.file.clone()?;
            Some(if code.has("\\p") { path } else { name })
        }
        "FILESIZE" => {
            let bytes = env.file_size?;
            Some(if code.has("\\k") {
                (bytes / 1024).to_string()
            } else if code.has("\\m") {
                (bytes / (1024 * 1024)).to_string()
            } else {
                bytes.to_string()
            })
        }
        "DATE" | "TIME" | "PRINTDATE" => {
            Some(format_date(&env.now, code.switch("\\@"), code.kind == "TIME"))
        }
        "CREATEDATE" => {
            let when = property(document?, "dcterms:created")?;
            Some(format_date(&when, code.switch("\\@"), false))
        }
        "SAVEDATE" => {
            let when = property(document?, "dcterms:modified")?;
            Some(format_date(&when, code.switch("\\@"), false))
        }
        "SEQ" => {
            let name = code.arguments.first()?.clone();
            let counter = env.sequences.entry(name).or_insert(0);
            if let Some(reset) = code.switch("\\r").and_then(|r| r.parse::<i64>().ok()) {
                *counter = reset;
            } else if !code.has("\\c") {
                *counter += 1;
            }
            let value = *counter;
            if code.has("\\h") {
                return Some(String::new());
            }
            Some(value.to_string())
        }
        "REF" => {
            let name = code.arguments.first()?;
            let (path, from, to) = bookmark(document?, name)?;
            let paragraph =
                crate::position::paragraph(document?, &crate::position::StoryId::Body, &path)?;
            if code.has("\\n") || code.has("\\r") || code.has("\\w") {
                return Some(paragraph_number(document?, &path).unwrap_or_default());
            }
            if code.has("\\p") {
                return Some(above_or_below(&path, &env.at));
            }
            Some(crate::session::clean_text(&slice(paragraph, from, to)))
        }
        "PAGEREF" => {
            let (path, _, _) = bookmark(document?, code.arguments.first()?)?;
            if code.has("\\p") {
                return Some(format!("on page {}", env.pages.get(&path).copied().unwrap_or(1)));
            }
            Some(env.pages.get(&path).copied().unwrap_or(1).to_string())
        }
        "NOTEREF" => {
            let (path, from, to) = bookmark(document?, code.arguments.first()?)?;
            let paragraph =
                crate::position::paragraph(document?, &crate::position::StoryId::Body, &path)?;
            note_number_in(document?, paragraph, from, to)
        }
        "STYLEREF" => {
            let style = code.arguments.first()?;
            styleref(document?, style, &env.at)
        }
        "=" => {
            let expression = code.arguments.join(" ");
            let value = Formula { env: &*env, depth: 0 }.value(&expression)?;
            Some(match code.switch("\\#") {
                Some(picture) => numeric_picture(value, picture),
                None => plain_number(value),
            })
        }
        "IF" => {
            let (left, op, right, yes, no) = (
                code.arguments.first()?,
                code.arguments.get(1)?,
                code.arguments.get(2)?,
                code.arguments.get(3).cloned().unwrap_or_default(),
                code.arguments.get(4).cloned().unwrap_or_default(),
            );
            Some(if compare(left, op, right) { yes } else { no })
        }
        "COMPARE" => {
            let (left, op, right) =
                (code.arguments.first()?, code.arguments.get(1)?, code.arguments.get(2)?);
            Some(if compare(left, op, right) { "1".into() } else { "0".into() })
        }
        _ => None,
    }
}

/// A core property's text.
fn property(document: &Document, name: &str) -> Option<String> {
    Some(crate::new::property(document, name).unwrap_or_default())
}

/// A document property by its Word name: the built in ones and the custom ones in `docProps/custom.xml`.
fn docproperty(document: &Document, name: &str) -> Option<String> {
    let core = match name.to_lowercase().as_str() {
        "title" => Some("dc:title"),
        "subject" => Some("dc:subject"),
        "author" => Some("dc:creator"),
        "keywords" => Some("cp:keywords"),
        "comments" => Some("dc:description"),
        "lastsavedby" => Some("cp:lastModifiedBy"),
        "category" => Some("cp:category"),
        _ => None,
    };
    if let Some(core) = core {
        return property(document, core);
    }
    for part in ["docProps/custom.xml", "docProps/app.xml"] {
        let Some(bytes) = document.package.part(part) else { continue };
        let Ok(root) = crate::xml::parse_bytes(bytes) else { continue };
        for el in root.elements() {
            let matches = el.attr("name").is_some_and(|n| n.eq_ignore_ascii_case(name))
                || el.local().eq_ignore_ascii_case(name);
            if matches {
                return Some(el.text());
            }
        }
    }
    None
}

/// The section a body paragraph is in, counting from 1.
fn section_of(document: &Document, at: &[u32]) -> usize {
    let index = at.first().copied().unwrap_or(0) as usize;
    1 + document
        .body
        .iter()
        .take(index)
        .filter(|b| matches!(b, Block::Paragraph(p) if p.props.child("w:sectPr").is_some()))
        .count()
}

/// Where a bookmark is in the body: its paragraph and the range of its text there.
pub fn bookmark(document: &Document, name: &str) -> Option<(Path, usize, usize)> {
    let mut start: Option<(Path, usize, String)> = None;
    for (path, paragraph) in crate::position::paragraphs(&document.body) {
        for mark in &paragraph.marks {
            match mark.el.name.as_str() {
                "w:bookmarkStart"
                    if mark.el.attr("w:name").is_some_and(|n| n.eq_ignore_ascii_case(name)) =>
                {
                    start = Some((
                        path.clone(),
                        mark.offset,
                        mark.el.attr("w:id").unwrap_or("").to_owned(),
                    ));
                }
                "w:bookmarkEnd" => {
                    if let Some((first, offset, id)) = &start {
                        if mark.el.attr("w:id") == Some(id.as_str()) {
                            let end = if *first == path {
                                mark.offset
                            } else {
                                paragraph_len(document, first)
                            };
                            return Some((first.clone(), *offset, end));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    start.map(|(path, offset, _)| {
        let end = paragraph_len(document, &path);
        (path, offset, end)
    })
}

fn paragraph_len(document: &Document, path: &[u32]) -> usize {
    crate::position::paragraph(document, &crate::position::StoryId::Body, path)
        .map_or(0, Paragraph::len)
}

/// The visible text of a paragraph between two offsets of its flat text.
pub fn slice(paragraph: &Paragraph, from: usize, to: usize) -> String {
    let mut out = String::new();
    let mut at = 0;
    let mut in_code: Vec<bool> = Vec::new();
    for run in &paragraph.runs {
        let start = at;
        at += run.len();
        if let Content::FieldChar(el) = &run.content {
            match el.attr("w:fldCharType") {
                Some("begin") => in_code.push(true),
                Some("separate") => {
                    if let Some(last) = in_code.last_mut() {
                        *last = false;
                    }
                }
                _ => {
                    in_code.pop();
                }
            }
            continue;
        }
        if in_code.iter().any(|c| *c) || run.is_deleted() || at <= from || start >= to {
            continue;
        }
        match &run.content {
            Content::Text(text) => {
                let lo = from.saturating_sub(start).min(text.len());
                let hi = (to - start).min(text.len());
                out.push_str(text.get(lo..hi).unwrap_or(""));
            }
            Content::Tab => out.push('\t'),
            _ => {}
        }
    }
    out
}

/// A paragraph's list number as Word gives it for `REF \n`: its label without the separator after it.
fn paragraph_number(document: &Document, path: &[u32]) -> Option<String> {
    let resolver = crate::styles::Resolver::new(document);
    let mut counters = crate::numbering::Counters::default();
    for (p, paragraph) in crate::position::paragraphs(&document.body) {
        let look = resolver.paragraph(&paragraph.props, None);
        let label =
            look.numbering.and_then(|(num, level)| counters.next(&resolver.numbering, num, level));
        if p == path {
            return label.map(|l| l.text.trim_end_matches(['.', ')']).to_owned());
        }
    }
    None
}

fn above_or_below(target: &[u32], at: &[u32]) -> String {
    if target < at {
        "above".into()
    } else {
        "below".into()
    }
}

/// The number of the footnote or endnote whose reference lies in a range, for `NOTEREF`.
fn note_number_in(
    document: &Document,
    paragraph: &Paragraph,
    from: usize,
    to: usize,
) -> Option<String> {
    let mut at = 0;
    let mut target = None;
    for run in &paragraph.runs {
        if at >= from && at <= to.max(from + 1) {
            if let Content::Object(el) = &run.content {
                if matches!(el.name.as_str(), "w:footnoteReference" | "w:endnoteReference") {
                    target = Some((el.name.clone(), el.attr("w:id")?.parse::<i64>().ok()?));
                    break;
                }
            }
        }
        at += run.len();
    }
    let (kind, id) = target?;
    let endnote = kind == "w:endnoteReference";
    let references = crate::layout::notes::references(document);
    let options = crate::layout::notes::NoteOptions::of(document, endnote);
    let (_, marks) = crate::layout::notes::number(&references, endnote, &options, None);
    marks.get(&id).cloned()
}

/// The text of the nearest paragraph before `at` in a style, by its name or id, for `STYLEREF`.
fn styleref(document: &Document, style: &str, at: &[u32]) -> Option<String> {
    let resolver = crate::styles::Resolver::new(document);
    let id = resolver.find(style)?.to_owned();
    let paragraphs = crate::position::paragraphs(&document.body);
    let before = paragraphs.iter().rev().filter(|(p, _)| p.as_slice() <= at);
    let after = paragraphs.iter().filter(|(p, _)| p.as_slice() > at);
    for (_, paragraph) in before.chain(after) {
        if resolver.paragraph_style_id(&paragraph.props) == Some(id.as_str()) {
            return Some(crate::session::clean_text(&paragraph.visible_text()).trim().to_owned());
        }
    }
    None
}

/// `\*` formats on a result: case (`Upper`, `Lower`, `FirstCap`, `Caps`) and number forms (`Arabic`,
/// `Roman`, `roman`, `ALPHABETIC`, `alphabetic`, `Ordinal`, `CardText`, `OrdText`, `Hex`,
/// `DollarText`). `MERGEFORMAT` and `CHARFORMAT` say how the result is formatted, not what it is.
pub fn general_switches(code: &Code, mut value: String) -> String {
    for (switch, argument) in &code.switches {
        if switch != "\\*" {
            continue;
        }
        let number = value.trim().replace(',', "").parse::<f64>().ok();
        value = match (argument.as_str(), number) {
            ("Upper", _) | ("UPPER", _) => value.to_uppercase(),
            ("Lower", _) | ("lower", _) => value.to_lowercase(),
            ("FirstCap", _) => {
                let mut chars = value.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => value,
                }
            }
            ("Caps", _) => value
                .split(' ')
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            ("Arabic", Some(n)) => (n as i64).to_string(),
            ("Roman", Some(n)) => crate::numbering::format_number(n as i64, "upperRoman"),
            ("roman", Some(n)) => crate::numbering::format_number(n as i64, "lowerRoman"),
            ("ROMAN", Some(n)) => crate::numbering::format_number(n as i64, "upperRoman"),
            ("ALPHABETIC", Some(n)) => crate::numbering::format_number(n as i64, "upperLetter"),
            ("alphabetic", Some(n)) => crate::numbering::format_number(n as i64, "lowerLetter"),
            ("Ordinal", Some(n)) | ("ordinal", Some(n)) => {
                crate::numbering::format_number(n as i64, "ordinal")
            }
            ("CardText", Some(n)) | ("Cardtext", Some(n)) | ("cardtext", Some(n)) => {
                crate::numbering::format_number(n as i64, "cardinalText").to_lowercase()
            }
            ("OrdText", Some(n)) | ("Ordtext", Some(n)) | ("ordtext", Some(n)) => {
                crate::numbering::format_number(n as i64, "ordinalText").to_lowercase()
            }
            ("Hex", Some(n)) | ("hex", Some(n)) | ("HEX", Some(n)) => format!("{:X}", n as i64),
            ("DollarText", Some(n)) | ("Dollartext", Some(n)) => dollar_text(n),
            _ => value,
        };
    }
    value
}

/// `123.45` as Word's `\* DollarText` writes it: "one hundred twenty-three and 45/100".
fn dollar_text(value: f64) -> String {
    let whole = value.trunc() as i64;
    let cents = ((value - value.trunc()) * 100.0).round() as i64;
    let words = crate::numbering::format_number(whole, "cardinalText").to_lowercase();
    format!("{words} and {cents:02}/100")
}

/// A number as Word writes a formula's result with no picture: no trailing zeros, at most 13 decimals.
fn plain_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let text = format!("{value:.13}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A number through a `\#` picture: `0`, `#`, `,` grouping, `.` decimals, `%`, literal text, and a
/// second section after `;` for negative numbers.
pub fn numeric_picture(value: f64, picture: &str) -> String {
    let sections: Vec<&str> = picture.split(';').collect();
    let (picture, value, sign) = match sections.as_slice() {
        [positive, negative, ..] if value < 0.0 => (*negative, -value, ""),
        [only] if value < 0.0 => (*only, -value, "-"),
        [first, ..] => (*first, value, ""),
        [] => return plain_number(value),
    };
    let picture = picture.trim_matches('\'');
    let percent = picture.contains('%');
    let value = if percent { value * 100.0 } else { value };
    let digits_at = picture.find(['0', '#']).unwrap_or(0);
    let last_digit = picture.rfind(['0', '#']).map_or(picture.len(), |i| i + 1);
    let prefix = &picture[..digits_at];
    let suffix = &picture[last_digit..];
    let body = &picture[digits_at..last_digit];
    let (whole_part, decimal_part) = body.split_once('.').unwrap_or((body, ""));
    let decimals = decimal_part.chars().filter(|c| matches!(c, '0' | '#')).count();
    let required = decimal_part.chars().filter(|c| *c == '0').count();
    let rounded = format!("{value:.decimals$}");
    let (whole, fraction) = rounded.split_once('.').unwrap_or((&rounded, ""));
    let mut fraction = fraction.to_owned();
    while fraction.len() > required && fraction.ends_with('0') {
        fraction.pop();
    }
    let minimum = whole_part.chars().filter(|c| *c == '0').count();
    let mut whole = whole.to_owned();
    while whole.len() < minimum {
        whole.insert(0, '0');
    }
    if minimum == 0 && whole == "0" && !fraction.is_empty() {
        whole.clear();
    }
    if whole_part.contains(',') {
        let mut grouped = String::new();
        for (n, c) in whole.chars().rev().enumerate() {
            if n > 0 && n % 3 == 0 {
                grouped.insert(0, ',');
            }
            grouped.insert(0, c);
        }
        whole = grouped;
    }
    let number = if fraction.is_empty() { whole } else { format!("{whole}.{fraction}") };
    format!("{sign}{prefix}{number}{suffix}")
}

/// `IF` and `COMPARE`: numbers compared as numbers, anything else as text, with `*` and `?` as wildcards
/// when comparing text for equality.
fn compare(left: &str, op: &str, right: &str) -> bool {
    if let (Ok(a), Ok(b)) = (left.trim().parse::<f64>(), right.trim().parse::<f64>()) {
        return match op {
            "=" => a == b,
            "<>" => a != b,
            "<" => a < b,
            "<=" => a <= b,
            ">" => a > b,
            ">=" => a >= b,
            _ => false,
        };
    }
    let matches = wildcard(right, left);
    match op {
        "=" => matches,
        "<>" => !matches,
        "<" => left < right,
        "<=" => left <= right,
        ">" => left > right,
        ">=" => left >= right,
        _ => false,
    }
}

fn wildcard(pattern: &str, text: &str) -> bool {
    fn go(p: &[char], t: &[char]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], t) || (!t.is_empty() && go(p, &t[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &t[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &t[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    go(&p, &t)
}

/// A date in the form a `\@` picture asks for. Without a picture, Word's short date (`M/d/yyyy`) or,
/// for a time, `h:mm am/pm`.
pub fn format_date(iso: &str, picture: Option<&str>, time: bool) -> String {
    let number = |range: std::ops::Range<usize>| {
        iso.get(range).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0)
    };
    let (year, month, day) = (number(0..4), number(5..7).max(1), number(8..10).max(1));
    let (hour, minute, second) = (number(11..13), number(14..16), number(17..19));
    let picture = picture.map(str::to_owned).unwrap_or_else(|| {
        if time {
            "h:mm am/pm".into()
        } else {
            "M/d/yyyy".into()
        }
    });
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const DAYS: [&str; 7] =
        ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    let weekday = weekday(year, month, day);
    let twelve = |h: u32| if h % 12 == 0 { 12 } else { h % 12 };
    let chars: Vec<char> = picture.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    // `m` after an hour is minutes, otherwise it is a month.
    let mut after_hour = false;
    while i < chars.len() {
        let c = chars[i];
        let run = chars[i..].iter().take_while(|x| **x == c).count();
        let rest: String = chars[i..].iter().collect();
        match c {
            '\'' => {
                let end = chars[i + 1..]
                    .iter()
                    .position(|x| *x == '\'')
                    .map_or(chars.len(), |p| i + 1 + p);
                out.extend(&chars[i + 1..end]);
                i = end + 1;
                continue;
            }
            'y' | 'Y' => {
                out.push_str(&if run >= 4 {
                    format!("{year:04}")
                } else {
                    format!("{:02}", year % 100)
                });
            }
            'M' => {
                out.push_str(&match run {
                    1 => month.to_string(),
                    2 => format!("{month:02}"),
                    3 => MONTHS[month as usize - 1][..3].to_owned(),
                    _ => MONTHS[month as usize - 1].to_owned(),
                });
                after_hour = false;
            }
            'd' | 'D' => {
                out.push_str(&match run {
                    1 => day.to_string(),
                    2 => format!("{day:02}"),
                    3 => DAYS[weekday][..3].to_owned(),
                    _ => DAYS[weekday].to_owned(),
                });
            }
            'H' => {
                out.push_str(&if run >= 2 { format!("{hour:02}") } else { hour.to_string() });
                after_hour = true;
            }
            'h' => {
                out.push_str(&if run >= 2 {
                    format!("{:02}", twelve(hour))
                } else {
                    twelve(hour).to_string()
                });
                after_hour = true;
            }
            'm' if after_hour || run <= 2 => {
                out.push_str(&if run >= 2 { format!("{minute:02}") } else { minute.to_string() });
            }
            's' | 'S' => {
                out.push_str(&if run >= 2 { format!("{second:02}") } else { second.to_string() })
            }
            'a' | 'A' if rest.to_lowercase().starts_with("am/pm") => {
                let (am, pm) = if c == 'A' { ("AM", "PM") } else { ("am", "pm") };
                out.push_str(if hour < 12 { am } else { pm });
                i += 5;
                continue;
            }
            other => {
                for _ in 0..run {
                    out.push(other);
                }
            }
        }
        i += run;
    }
    out
}

/// The day of the week, 0 for Sunday, by Zeller's rule.
fn weekday(year: u32, month: u32, day: u32) -> usize {
    let (y, m) =
        if month < 3 { (year as i64 - 1, month as i64 + 12) } else { (year as i64, month as i64) };
    let k = y % 100;
    let j = y / 100;
    let h = (day as i64 + 13 * (m + 1) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    ((h + 6) % 7) as usize
}

/// A formula of the `=` field: arithmetic, comparisons, the functions Word has, table ranges and
/// bookmarks.
struct Formula<'e, 'a> {
    env: &'e Env<'a>,
    depth: usize,
}

impl Formula<'_, '_> {
    fn value(&mut self, text: &str) -> Option<f64> {
        let tokens = tokenize(text);
        let mut at = 0;
        let value = self.comparison(&tokens, &mut at)?;
        (at == tokens.len()).then_some(value)
    }

    fn comparison(&mut self, t: &[Token], at: &mut usize) -> Option<f64> {
        let left = self.sum(t, at)?;
        if let Some(Token::Op(op)) = t.get(*at) {
            if matches!(op.as_str(), "=" | "<>" | "<" | ">" | "<=" | ">=") {
                let op = op.clone();
                *at += 1;
                let right = self.sum(t, at)?;
                let truth = match op.as_str() {
                    "=" => left == right,
                    "<>" => left != right,
                    "<" => left < right,
                    ">" => left > right,
                    "<=" => left <= right,
                    _ => left >= right,
                };
                return Some(if truth { 1.0 } else { 0.0 });
            }
        }
        Some(left)
    }

    fn sum(&mut self, t: &[Token], at: &mut usize) -> Option<f64> {
        let mut value = self.product(t, at)?;
        while let Some(Token::Op(op)) = t.get(*at) {
            match op.as_str() {
                "+" => {
                    *at += 1;
                    value += self.product(t, at)?;
                }
                "-" => {
                    *at += 1;
                    value -= self.product(t, at)?;
                }
                _ => break,
            }
        }
        Some(value)
    }

    fn product(&mut self, t: &[Token], at: &mut usize) -> Option<f64> {
        let mut value = self.power(t, at)?;
        while let Some(Token::Op(op)) = t.get(*at) {
            match op.as_str() {
                "*" => {
                    *at += 1;
                    value *= self.power(t, at)?;
                }
                "/" => {
                    *at += 1;
                    value /= self.power(t, at)?;
                }
                _ => break,
            }
        }
        Some(value)
    }

    fn power(&mut self, t: &[Token], at: &mut usize) -> Option<f64> {
        let base = self.unary(t, at)?;
        if let Some(Token::Op(op)) = t.get(*at) {
            if op == "^" {
                *at += 1;
                return Some(base.powf(self.power(t, at)?));
            }
        }
        Some(base)
    }

    fn unary(&mut self, t: &[Token], at: &mut usize) -> Option<f64> {
        match t.get(*at)? {
            Token::Op(op) if op == "-" => {
                *at += 1;
                Some(-self.unary(t, at)?)
            }
            Token::Op(op) if op == "+" => {
                *at += 1;
                self.unary(t, at)
            }
            _ => {
                let value = self.atom(t, at)?;
                if let Some(Token::Op(op)) = t.get(*at) {
                    if op == "%" {
                        *at += 1;
                        return Some(value / 100.0);
                    }
                }
                Some(value)
            }
        }
    }

    fn atom(&mut self, t: &[Token], at: &mut usize) -> Option<f64> {
        match t.get(*at)?.clone() {
            Token::Number(n) => {
                *at += 1;
                Some(n)
            }
            Token::Open => {
                *at += 1;
                let value = self.comparison(t, at)?;
                matches!(t.get(*at), Some(Token::Close)).then(|| *at += 1)?;
                Some(value)
            }
            Token::Word(word) => {
                *at += 1;
                if let Some(Token::Open) = t.get(*at) {
                    *at += 1;
                    let mut arguments: Vec<Vec<f64>> = Vec::new();
                    loop {
                        if let Some(Token::Close) = t.get(*at) {
                            *at += 1;
                            break;
                        }
                        arguments.push(self.argument(t, at)?);
                        match t.get(*at) {
                            Some(Token::Comma) => *at += 1,
                            Some(Token::Close) => {
                                *at += 1;
                                break;
                            }
                            _ => return None,
                        }
                    }
                    return function(&word, &arguments);
                }
                match word.to_uppercase().as_str() {
                    "TRUE" => Some(1.0),
                    "FALSE" => Some(0.0),
                    _ => self.reference(&word).and_then(|v| v.first().copied()),
                }
            }
            _ => None,
        }
    }

    /// One argument of a function: a range of cells (`ABOVE`, `A1:B3`) gives every number in it.
    fn argument(&mut self, t: &[Token], at: &mut usize) -> Option<Vec<f64>> {
        if let Some(Token::Word(word)) = t.get(*at) {
            let next = t.get(*at + 1);
            if !matches!(next, Some(Token::Open)) && !matches!(next, Some(Token::Op(_))) {
                if let Some(values) = self.reference(word) {
                    *at += 1;
                    return Some(values);
                }
            }
        }
        if self.depth > 16 {
            return None;
        }
        self.depth += 1;
        let value = self.comparison(t, at);
        self.depth -= 1;
        value.map(|v| vec![v])
    }

    /// The numbers a word names: `ABOVE`, `BELOW`, `LEFT`, `RIGHT` in the table the field is in, a cell
    /// (`B2`) or a range (`A1:B3`), or a bookmark's text.
    fn reference(&self, word: &str) -> Option<Vec<f64>> {
        let upper = word.to_uppercase();
        if let Some((cells, row, column)) = &self.env.cells {
            let number = |text: &str| -> Option<f64> {
                let cleaned: String =
                    text.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-')).collect();
                cleaned.parse().ok()
            };
            let picked: Vec<f64> = match upper.as_str() {
                "ABOVE" => (0..*row)
                    .filter_map(|r| cells.get(r)?.get(*column))
                    .filter_map(|t| number(t))
                    .collect(),
                "BELOW" => (row + 1..cells.len())
                    .filter_map(|r| cells.get(r)?.get(*column))
                    .filter_map(|t| number(t))
                    .collect(),
                "LEFT" => cells
                    .get(*row)
                    .map(|r| r[..(*column).min(r.len())].iter().filter_map(|t| number(t)).collect())
                    .unwrap_or_default(),
                "RIGHT" => cells
                    .get(*row)
                    .map(|r| r.iter().skip(column + 1).filter_map(|t| number(t)).collect())
                    .unwrap_or_default(),
                _ => match cell_range(&upper) {
                    Some(((r0, c0), (r1, c1))) => {
                        let mut out = Vec::new();
                        for r in r0.min(r1)..=r0.max(r1) {
                            for c in c0.min(c1)..=c0.max(c1) {
                                if let Some(v) =
                                    cells.get(r).and_then(|row| row.get(c)).and_then(|t| number(t))
                                {
                                    out.push(v);
                                }
                            }
                        }
                        out
                    }
                    None => Vec::new(),
                },
            };
            if matches!(upper.as_str(), "ABOVE" | "BELOW" | "LEFT" | "RIGHT")
                || cell_range(&upper).is_some()
            {
                return Some(picked);
            }
        }
        let document = self.env.document?;
        let (path, from, to) = bookmark(document, word)?;
        let paragraph =
            crate::position::paragraph(document, &crate::position::StoryId::Body, &path)?;
        let text = slice(paragraph, from, to);
        let cleaned: String =
            text.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-')).collect();
        cleaned.parse::<f64>().ok().map(|v| vec![v])
    }
}

/// `A1` or `A1:C3`, as zero based (row, column) pairs.
fn cell_range(text: &str) -> Option<((usize, usize), (usize, usize))> {
    let cell = |s: &str| -> Option<(usize, usize)> {
        let letters: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        let digits = &s[letters.len()..];
        if letters.is_empty() || letters.len() > 2 || digits.is_empty() {
            return None;
        }
        let column =
            letters.chars().fold(0usize, |n, c| n * 26 + (c as usize - 'A' as usize + 1)) - 1;
        Some((digits.parse::<usize>().ok()?.checked_sub(1)?, column))
    };
    match text.split_once(':') {
        Some((a, b)) => Some((cell(a)?, cell(b)?)),
        None => cell(text).map(|c| (c, c)),
    }
}

fn function(name: &str, arguments: &[Vec<f64>]) -> Option<f64> {
    let all: Vec<f64> = arguments.iter().flatten().copied().collect();
    let first = |n: usize| arguments.get(n).and_then(|v| v.first()).copied();
    Some(match name.to_uppercase().as_str() {
        "SUM" => all.iter().sum(),
        "AVERAGE" => all.iter().sum::<f64>() / all.len().max(1) as f64,
        "COUNT" => all.len() as f64,
        "MAX" => all.iter().copied().fold(f64::MIN, f64::max),
        "MIN" => all.iter().copied().fold(f64::MAX, f64::min),
        "PRODUCT" => all.iter().product(),
        "ABS" => first(0)?.abs(),
        "INT" => first(0)?.trunc(),
        "MOD" => first(0)? % first(1)?,
        "ROUND" => {
            let factor = 10f64.powi(first(1)? as i32);
            (first(0)? * factor).round() / factor
        }
        "SIGN" => first(0)?.signum(),
        "IF" => {
            if first(0)? != 0.0 {
                first(1)?
            } else {
                first(2)?
            }
        }
        "AND" => (first(0)? != 0.0 && first(1)? != 0.0) as i32 as f64,
        "OR" => (first(0)? != 0.0 || first(1)? != 0.0) as i32 as f64,
        "NOT" => (first(0)? == 0.0) as i32 as f64,
        "DEFINED" => 1.0,
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Word(String),
    Op(String),
    Open,
    Close,
    Comma,
}

fn tokenize(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            // A comma is a thousands separator only between digits with three after it.
            let grouping = |i: usize| {
                chars[i] == ','
                    && chars.get(i + 1..i + 4).is_some_and(|d| d.iter().all(char::is_ascii_digit))
                    && !chars.get(i + 4).is_some_and(char::is_ascii_digit)
            };
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || grouping(i)) {
                i += 1;
            }
            let number: String = chars[start..i].iter().filter(|c| **c != ',').collect();
            out.push(Token::Number(number.parse().unwrap_or(0.0)));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | ':')) {
                i += 1;
            }
            out.push(Token::Word(chars[start..i].iter().collect()));
        } else {
            i += 1;
            match c {
                '(' => out.push(Token::Open),
                ')' => out.push(Token::Close),
                ',' | ';' => out.push(Token::Comma),
                '<' if chars.get(i) == Some(&'>') => {
                    i += 1;
                    out.push(Token::Op("<>".into()))
                }
                '<' | '>' if chars.get(i) == Some(&'=') => {
                    i += 1;
                    out.push(Token::Op(format!("{c}=")))
                }
                other => out.push(Token::Op(other.to_string())),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(code: &str) -> Option<String> {
        evaluate(
            code,
            &mut Env {
                now: "2026-10-24T14:05:09Z".into(),
                page: 3,
                page_count: 9,
                ..Env::default()
            },
        )
    }

    #[test]
    fn a_code_is_read_into_its_kind_arguments_and_switches() {
        let code = parse(r#" PAGEREF _Ref1234 \h \* MERGEFORMAT "#);
        assert_eq!(code.kind, "PAGEREF");
        assert_eq!(code.arguments, vec!["_Ref1234"]);
        assert_eq!(code.switch("\\*"), Some("MERGEFORMAT"));
        assert!(code.has("\\h"));
        let code = parse(r#"DATE \@ "dddd, MMMM d, yyyy""#);
        assert_eq!(code.switch("\\@"), Some("dddd, MMMM d, yyyy"));
    }

    #[test]
    fn dates_follow_every_picture_word_offers() {
        let now = "2026-10-24T14:05:09Z";
        assert_eq!(
            format_date(now, Some("dddd, MMMM d, yyyy"), false),
            "Saturday, October 24, 2026"
        );
        assert_eq!(format_date(now, Some("ddd, MMM dd, yy"), false), "Sat, Oct 24, 26");
        assert_eq!(format_date(now, Some("h:mm am/pm"), true), "2:05 pm");
        assert_eq!(format_date(now, Some("HH:mm:ss"), true), "14:05:09");
        assert_eq!(format_date(now, Some("'Today is' d MMMM"), false), "Today is 24 October");
        assert_eq!(format_date(now, Some("h:mm AM/PM"), true), "2:05 PM");
        assert_eq!(format_date(now, None, false), "10/24/2026");
    }

    #[test]
    fn number_switches_and_case_switches_change_the_result() {
        assert_eq!(run(r"PAGE \* roman").as_deref(), Some("iii"));
        assert_eq!(run(r"PAGE \* ALPHABETIC").as_deref(), Some("C"));
        assert_eq!(run(r"PAGE \* Ordinal").as_deref(), Some("3rd"));
        assert_eq!(run(r"NUMPAGES \* CardText").as_deref(), Some("nine"));
        assert_eq!(run(r"NUMPAGES \* OrdText").as_deref(), Some("ninth"));
        assert_eq!(run(r"= 255 \* Hex").as_deref(), Some("FF"));
        assert_eq!(
            run(r"= 123.45 \* DollarText").as_deref(),
            Some("one hundred twenty-three and 45/100")
        );
        assert_eq!(run(r#"IF 1 = 1 "yes please" "no" \* Upper"#).as_deref(), Some("YES PLEASE"));
        assert_eq!(run(r#"IF 2 > 3 "bigger" "smaller" \* FirstCap"#).as_deref(), Some("Smaller"));
    }

    #[test]
    fn formulas_compute_with_pictures() {
        assert_eq!(run("=2+3*4").as_deref(), Some("14"));
        assert_eq!(run(r#"= (10 - 4) / 4 \# "0.00""#).as_deref(), Some("1.50"));
        assert_eq!(run(r##"= 1234567.891 \# "#,##0.00""##).as_deref(), Some("1,234,567.89"));
        assert_eq!(run(r#"= -42 \# "$#,##0.00;($#,##0.00)""#).as_deref(), Some("($42.00)"));
        assert_eq!(run(r#"= 0.25 \# "0%""#).as_deref(), Some("25%"));
        assert_eq!(run("= MAX(1, 7, 3) + MIN(4, 2)").as_deref(), Some("9"));
        assert_eq!(run("= ROUND(2.567, 1)").as_deref(), Some("2.6"));
        assert_eq!(run("= 2^10").as_deref(), Some("1024"));
        assert_eq!(run(r"COMPARE 5 >= 4").as_deref(), Some("1"));
    }

    #[test]
    fn table_ranges_sum_the_cells_above_and_to_the_left() {
        let cells = vec![
            vec!["Item".to_owned(), "Cost".to_owned()],
            vec!["Pens".to_owned(), "1,200".to_owned()],
            vec!["Paper".to_owned(), "$34.50".to_owned()],
            vec!["Total".to_owned(), String::new()],
        ];
        let mut env = Env { cells: Some((cells, 3, 1)), ..Env::default() };
        assert_eq!(evaluate(r#"=SUM(ABOVE) \# "0.00""#, &mut env).as_deref(), Some("1234.50"));
        assert_eq!(evaluate("=SUM(B2:B3)", &mut env).as_deref(), Some("1234.5"));
        assert_eq!(evaluate("=COUNT(ABOVE)", &mut env).as_deref(), Some("2"));
    }

    #[test]
    fn sequences_count_in_order_and_restart() {
        let mut env = Env::default();
        assert_eq!(evaluate("SEQ Figure", &mut env).as_deref(), Some("1"));
        assert_eq!(evaluate("SEQ Figure", &mut env).as_deref(), Some("2"));
        assert_eq!(evaluate(r"SEQ Figure \c", &mut env).as_deref(), Some("2"));
        assert_eq!(evaluate("SEQ Table", &mut env).as_deref(), Some("1"));
        assert_eq!(evaluate(r"SEQ Figure \r 10", &mut env).as_deref(), Some("10"));
        assert_eq!(evaluate(r"SEQ Figure \* roman", &mut env).as_deref(), Some("xi"));
    }

    #[test]
    fn a_field_it_does_not_know_keeps_its_result() {
        assert_eq!(run("MERGEFIELD Name"), None);
        assert_eq!(run("INDEX \\e"), None);
    }
}
