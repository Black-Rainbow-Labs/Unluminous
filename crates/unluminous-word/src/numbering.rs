//! Lists: which level definition a paragraph's list uses, and the label each paragraph is given.
//!
//! A paragraph names a `w:num` by id and a level. The `w:num` names a `w:abstractNum`, which holds nine
//! `w:lvl` definitions, and can override a level's start or the whole level. An abstract list can instead
//! link to a numbering style, which names another `w:num`. Counting is per abstract list, the way Word
//! counts: two `w:num`s sharing an abstract list continue each other unless one restarts.

use std::collections::{HashMap, HashSet};

use crate::xml::El;

/// What the numbering part says, indexed.
pub struct Numbering<'a> {
    nums: HashMap<i64, &'a El>,
    abstracts: HashMap<i64, &'a El>,
    /// Numbering styles' `w:numPr/w:numId`, by style id, for `w:numStyleLink`.
    style_nums: HashMap<String, i64>,
}

impl<'a> Numbering<'a> {
    pub fn new(root: Option<&'a El>) -> Numbering<'a> {
        let mut numbering = Numbering {
            nums: HashMap::new(),
            abstracts: HashMap::new(),
            style_nums: HashMap::new(),
        };
        let Some(root) = root else { return numbering };
        for el in root.elements() {
            match el.name.as_str() {
                "w:num" => {
                    if let Some(id) = el.attr("w:numId").and_then(|id| id.parse().ok()) {
                        numbering.nums.insert(id, el);
                    }
                }
                "w:abstractNum" => {
                    if let Some(id) = el.attr("w:abstractNumId").and_then(|id| id.parse().ok()) {
                        numbering.abstracts.insert(id, el);
                    }
                }
                _ => {}
            }
        }
        numbering
    }

    /// Tell the numbering which `w:num` each numbering style names, from the styles part.
    pub fn with_styles(mut self, styles: Option<&El>) -> Self {
        let Some(styles) = styles else { return self };
        for style in
            styles.children_named("w:style").filter(|s| s.attr("w:type") == Some("numbering"))
        {
            let num = style
                .child("w:pPr")
                .and_then(|ppr| ppr.child("w:numPr"))
                .and_then(|n| n.val_of("w:numId"));
            if let (Some(id), Some(num)) =
                (style.attr("w:styleId"), num.and_then(|n| n.parse().ok()))
            {
                self.style_nums.insert(id.to_owned(), num);
            }
        }
        self
    }

    /// The abstract list a `w:num` uses, following a numbering style link once.
    pub fn abstract_of(&self, num: i64) -> Option<(i64, &'a El)> {
        let id: i64 = self.nums.get(&num)?.val_of("w:abstractNumId")?.parse().ok()?;
        let abs = self.abstracts.get(&id)?;
        if let Some(link) = abs.val_of("w:numStyleLink") {
            if let Some(linked) = self.style_nums.get(link).and_then(|n| self.nums.get(n)) {
                let id: i64 = linked.val_of("w:abstractNumId")?.parse().ok()?;
                return Some((id, self.abstracts.get(&id)?));
            }
        }
        Some((id, abs))
    }

    /// The `w:lvl` definition a paragraph in list `num` at `level` uses.
    pub fn level(&self, num: i64, level: u8) -> Option<&'a El> {
        let level_text = level.to_string();
        if let Some(over) = self.override_of(num, level) {
            if let Some(lvl) = over.child("w:lvl") {
                return Some(lvl);
            }
        }
        let (_, abs) = self.abstract_of(num)?;
        abs.children_named("w:lvl").find(|lvl| lvl.attr("w:ilvl") == Some(level_text.as_str()))
    }

    fn override_of(&self, num: i64, level: u8) -> Option<&'a El> {
        let level_text = level.to_string();
        self.nums
            .get(&num)?
            .children_named("w:lvlOverride")
            .find(|o| o.attr("w:ilvl") == Some(level_text.as_str()))
    }

    /// The level of list `num` whose `w:pStyle` is `style`, for a paragraph that is in the list only
    /// because its style is.
    pub fn level_for_style(&self, num: i64, style: &str) -> Option<u8> {
        let (_, abs) = self.abstract_of(num)?;
        abs.children_named("w:lvl")
            .find(|lvl| lvl.val_of("w:pStyle") == Some(style))
            .and_then(|lvl| lvl.attr("w:ilvl")?.parse().ok())
    }

    fn start_of(&self, num: i64, level: u8) -> i64 {
        self.level(num, level)
            .and_then(|lvl| lvl.val_of("w:start"))
            .and_then(|start| start.parse().ok())
            .unwrap_or(1)
    }
}

/// The label of one list paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub text: String,
    /// What follows the label: a tab to the next stop, a space, or nothing.
    pub suffix: Suffix,
    /// `w:lvlJc`: where the label sits relative to the hanging indent.
    pub align: crate::props::Align,
    /// The level's own run properties, applied over the paragraph mark's for the label.
    pub rpr: Option<El>,
    /// `w:lvlPicBulletId`: the picture bullet (`w:numPicBullet`) drawn instead of the text.
    pub picture: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suffix {
    Tab,
    Space,
    Nothing,
}

/// Counting through a document, one paragraph at a time, in order.
#[derive(Debug, Default, Clone)]
pub struct Counters {
    counts: HashMap<i64, [i64; 9]>,
    /// The `w:num`s whose start overrides have already been applied, which happens once, at first use.
    started: HashSet<(i64, u8)>,
    /// The last level used in each abstract list, which decides which levels restart.
    last_level: HashMap<i64, u8>,
}

impl Counters {
    /// Count a paragraph in list `num` at `level` and answer its label.
    pub fn next(&mut self, numbering: &Numbering, num: i64, level: u8) -> Option<Label> {
        let (abstract_id, _) = numbering.abstract_of(num)?;
        let lvl = numbering.level(num, level)?;
        let counts = self.counts.entry(abstract_id).or_insert([i64::MIN; 9]);
        let level = usize::from(level.min(8));
        if !self.started.contains(&(num, level as u8)) {
            self.started.insert((num, level as u8));
            if let Some(start) = numbering
                .override_of(num, level as u8)
                .and_then(|over| over.val_of("w:startOverride"))
                .and_then(|start| start.parse::<i64>().ok())
            {
                counts[level] = start - 1;
            }
        }
        if counts[level] == i64::MIN {
            counts[level] = numbering.start_of(num, level as u8) - 1;
        }
        counts[level] += 1;
        // Deeper levels start again, unless a level's `w:lvlRestart` says it restarts only after a
        // shallower one.
        for (deeper, count) in counts.iter_mut().enumerate().skip(level + 1) {
            let restart_after = numbering
                .level(num, deeper as u8)
                .and_then(|l| l.val_of("w:lvlRestart"))
                .and_then(|r| r.parse::<usize>().ok());
            if restart_after.is_none_or(|after| after > level) {
                *count = i64::MIN;
            }
        }
        self.last_level.insert(abstract_id, level as u8);
        let format = lvl.val_of("w:numFmt").unwrap_or("decimal");
        let legal = crate::props::on_off(lvl, "w:isLgl") == Some(true);
        let pattern = lvl.val_of("w:lvlText").unwrap_or("");
        let mut text = String::new();
        let mut characters = pattern.chars().peekable();
        while let Some(character) = characters.next() {
            if character == '%' {
                if let Some(digit) = characters.peek().and_then(|d| d.to_digit(10)) {
                    characters.next();
                    let at = (digit as usize).saturating_sub(1).min(8);
                    let value = match counts[at] {
                        i64::MIN => numbering.start_of(num, at as u8),
                        value => value,
                    };
                    let at_format = numbering
                        .level(num, at as u8)
                        .and_then(|l| l.val_of("w:numFmt"))
                        .unwrap_or("decimal");
                    let at_format = if legal && at != level {
                        "decimal"
                    } else if at == level {
                        format
                    } else {
                        at_format
                    };
                    text.push_str(&format_number(value, at_format));
                    continue;
                }
            }
            text.push(character);
        }
        if format == "bullet" {
            text = text.chars().map(symbol_to_unicode).collect();
        }
        Some(Label {
            text,
            suffix: match lvl.val_of("w:suff") {
                Some("space") => Suffix::Space,
                Some("nothing") => Suffix::Nothing,
                _ => Suffix::Tab,
            },
            align: lvl.val_of("w:lvlJc").map(crate::props::Align::parse).unwrap_or_default(),
            rpr: lvl.child("w:rPr").cloned(),
            picture: lvl.val_of("w:lvlPicBulletId").map(str::to_owned),
        })
    }
}

/// A number in one of Word's numbering formats.
pub fn format_number(value: i64, format: &str) -> String {
    match format {
        "none" => String::new(),
        "bullet" => String::new(),
        "decimalZero" if (0..10).contains(&value) => format!("0{value}"),
        "upperRoman" => roman(value).to_uppercase(),
        "lowerRoman" => roman(value),
        "upperLetter" => letters(value).to_uppercase(),
        "lowerLetter" => letters(value),
        "ordinal" => format!("{value}{}", ordinal_suffix(value)),
        "cardinalText" => capitalise(&cardinal_text(value)),
        "ordinalText" => capitalise(&ordinal_text(value)),
        "decimalEnclosedParen" => format!("({value})"),
        // ① to ⑳, then plain numbers, as Word draws past twenty.
        "decimalEnclosedCircle" if (1..=20).contains(&value) => {
            char::from_u32(0x2460 + value as u32 - 1)
                .map_or_else(|| value.to_string(), String::from)
        }
        "decimalEnclosedFullstop" if (1..=20).contains(&value) => {
            char::from_u32(0x2488 + value as u32 - 1)
                .map_or_else(|| value.to_string(), String::from)
        }
        "decimalFullWidth" => value
            .to_string()
            .chars()
            .map(|c| char::from_u32(c as u32 - '0' as u32 + 0xFF10).unwrap_or(c))
            .collect(),
        "hex" => format!("{value:X}"),
        _ => value.to_string(),
    }
}

fn roman(mut value: i64) -> String {
    if value <= 0 {
        return value.to_string();
    }
    let table = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (amount, letters) in table {
        while value >= amount {
            out.push_str(letters);
            value -= amount;
        }
    }
    out
}

/// Word's letters: a to z, then aa to zz, then aaa, repeating one letter rather than counting in base 26.
fn letters(value: i64) -> String {
    if value <= 0 {
        return value.to_string();
    }
    let index = (value - 1) % 26;
    let repeat = (value - 1) / 26 + 1;
    let letter = char::from(b'a' + index as u8);
    std::iter::repeat_n(letter, repeat as usize).collect()
}

fn ordinal_suffix(value: i64) -> &'static str {
    match (value % 10, value % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    }
}

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] =
    ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];

fn cardinal_text(value: i64) -> String {
    match value {
        v if v < 0 => v.to_string(),
        v if v < 20 => ONES[v as usize].to_owned(),
        v if v < 100 => match v % 10 {
            0 => TENS[(v / 10) as usize].to_owned(),
            ones => format!("{}-{}", TENS[(v / 10) as usize], ONES[ones as usize]),
        },
        v if v < 1000 => match v % 100 {
            0 => format!("{} hundred", ONES[(v / 100) as usize]),
            rest => format!("{} hundred {}", ONES[(v / 100) as usize], cardinal_text(rest)),
        },
        // Word spells a field's number out to 999,999 (`\* CardText`, `\* DollarText`).
        v if v < 1_000_000 => match v % 1000 {
            0 => format!("{} thousand", cardinal_text(v / 1000)),
            rest => format!("{} thousand {}", cardinal_text(v / 1000), cardinal_text(rest)),
        },
        v => v.to_string(),
    }
}

fn ordinal_text(value: i64) -> String {
    let cardinal = cardinal_text(value);
    let irregular = [
        ("one", "first"),
        ("two", "second"),
        ("three", "third"),
        ("five", "fifth"),
        ("eight", "eighth"),
        ("nine", "ninth"),
        ("twelve", "twelfth"),
    ];
    for (end, replacement) in irregular {
        if let Some(front) = cardinal.strip_suffix(end) {
            return format!("{front}{replacement}");
        }
    }
    match cardinal.strip_suffix('y') {
        Some(front) => format!("{front}ieth"),
        None => format!("{cardinal}th"),
    }
}

fn capitalise(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

/// A bullet written in a symbol font's private use area, as the character it is drawn as.
pub fn symbol_to_unicode(character: char) -> char {
    match character as u32 {
        0xF0B7 | 0xF0B0 => '•',
        0xF0A7 => '▪',
        0xF0A8 => '◆',
        0xF076 => '❖',
        0xF0D8 => '➢',
        0xF0FC => '✓',
        0xF06E => '■',
        0xF071 => '❑',
        0xF0E0 => '➢',
        code @ 0xF020..=0xF0FF => char::from_u32(code - 0xF000).unwrap_or('•'),
        _ => character,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_word_offers() {
        let cases = [
            (4, "decimal", "4"),
            (4, "decimalZero", "04"),
            (4, "upperRoman", "IV"),
            (14, "lowerRoman", "xiv"),
            (3, "upperLetter", "C"),
            (28, "lowerLetter", "bb"),
            (22, "ordinal", "22nd"),
            (13, "ordinal", "13th"),
            (21, "cardinalText", "Twenty-one"),
            (12, "ordinalText", "Twelfth"),
            (40, "ordinalText", "Fortieth"),
            (101, "cardinalText", "One hundred one"),
        ];
        for (value, format, expected) in cases {
            assert_eq!(format_number(value, format), expected, "{value} as {format}");
        }
    }

    fn numbering_xml() -> El {
        crate::xml::parse(&format!(
            r#"<w:numbering xmlns:w="{}">
                <w:abstractNum w:abstractNumId="0">
                  <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl>
                  <w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="%1.%2)"/></w:lvl>
                </w:abstractNum>
                <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
                <w:num w:numId="2"><w:abstractNumId w:val="0"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="5"/></w:lvlOverride></w:num>
              </w:numbering>"#,
            crate::xml::W
        ))
        .unwrap()
    }

    #[test]
    fn levels_count_and_restart_the_way_word_numbers_an_outline() {
        let root = numbering_xml();
        let numbering = Numbering::new(Some(&root));
        let mut counters = Counters::default();
        let labels: Vec<String> = [(1, 0), (1, 1), (1, 1), (1, 0), (1, 1)]
            .iter()
            .map(|(num, level)| counters.next(&numbering, *num, *level).unwrap().text)
            .collect();
        assert_eq!(labels, ["1.", "1.a)", "1.b)", "2.", "2.a)"]);
    }

    #[test]
    fn a_start_override_restarts_the_shared_list_once() {
        let root = numbering_xml();
        let numbering = Numbering::new(Some(&root));
        let mut counters = Counters::default();
        let labels: Vec<String> = [(1, 0), (1, 0), (2, 0), (2, 0), (1, 0)]
            .iter()
            .map(|(num, level)| counters.next(&numbering, *num, *level).unwrap().text)
            .collect();
        assert_eq!(labels, ["1.", "2.", "5.", "6.", "7."]);
    }

    #[test]
    fn a_symbol_font_bullet_is_drawn_as_a_bullet() {
        assert_eq!(symbol_to_unicode('\u{F0B7}'), '•');
        assert_eq!(symbol_to_unicode('o'), 'o');
    }
}
