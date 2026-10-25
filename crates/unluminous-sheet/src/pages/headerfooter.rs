//! Header and footer text, with its codes expanded.
//!
//! A header or footer is one string of text and `&` codes in three sections, left, centre and right:
//! `&L&"Arial,Bold"&14Title&C&A&RPage &P of &N`. [`expand`] reads the string and returns each section as a
//! list of runs, a run being text with the font, size and effects in force when it was written. The page
//! number, the date and the other values come in through a [`Context`], so the same text gives every page
//! its own header and nothing here reads a clock.
//!
//! The codes are Excel's: `&P` the page number, `&N` the number of pages, `&D` the date, `&T` the time,
//! `&F` the file name, `&Z` the folder, `&A` the sheet name, `&B` bold, `&I` italic, `&U` underline, `&E`
//! double underline, `&S` strikethrough, `&X` superscript, `&Y` subscript, `&nn` a size in points,
//! `&"Font,Style"` a font, `&Krrggbb` a colour, `&G` a picture, and `&&` an ampersand. The bracketed forms
//! `&[Page]`, `&[Pages]`, `&[Date]`, `&[Time]`, `&[File]`, `&[Path]`, `&[Tab]` and `&[Picture]` mean the same.

/// What the codes stand for on one page.
#[derive(Debug, Clone, PartialEq)]
pub struct Context {
    /// The number printed for this page, counting from the sheet's first page number.
    pub page: i32,
    /// How many pages the sheet prints.
    pub pages: u32,
    /// The date as it should be printed.
    pub date: String,
    /// The time as it should be printed.
    pub time: String,
    /// The file's name.
    pub file: String,
    /// The folder the file is in.
    pub path: String,
    /// The sheet's name.
    pub sheet: String,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            page: 1,
            pages: 1,
            date: String::new(),
            time: String::new(),
            file: String::new(),
            path: String::new(),
            sheet: String::new(),
        }
    }
}

/// How the text of one run is set.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunStyle {
    /// The font's name, or `None` for the workbook's default.
    pub font: Option<String>,
    /// Points, or `None` for the default (11).
    pub size: Option<f64>,
    pub bold: bool,
    pub italic: bool,
    /// 0 none, 1 single, 2 double.
    pub underline: u8,
    pub strike: bool,
    pub superscript: bool,
    pub subscript: bool,
    /// `rrggbb`.
    pub color: Option<String>,
}

/// A stretch of text set one way.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: RunStyle,
    /// A picture stands here (`&G`); the text is empty.
    pub picture: bool,
}

/// The three sections of a header or footer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sections {
    pub left: Vec<Run>,
    pub center: Vec<Run>,
    pub right: Vec<Run>,
}

impl Sections {
    /// Every run of a section as one line of plain text.
    pub fn plain(runs: &[Run]) -> String {
        runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Whether the header or footer prints nothing at all.
    pub fn is_empty(&self) -> bool {
        [&self.left, &self.center, &self.right]
            .iter()
            .all(|s| s.iter().all(|r| r.text.is_empty() && !r.picture))
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Left,
    Center,
    Right,
}

/// Read a header or footer string and give each section its runs, with the codes for this page replaced.
pub fn expand(text: &str, context: &Context) -> Sections {
    let mut out = Sections::default();
    let mut section = Section::Center;
    let mut style = RunStyle::default();
    let mut pending = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c != '&' {
            pending.push(c);
            at += 1;
            continue;
        }
        at += 1;
        let Some(&code) = chars.get(at) else {
            pending.push('&');
            break;
        };
        at += 1;
        match code {
            '&' => pending.push('&'),
            'L' | 'C' | 'R' => {
                push_run(&mut out, section, &mut pending, &style, false);
                section = match code {
                    'L' => Section::Left,
                    'C' => Section::Center,
                    _ => Section::Right,
                };
            }
            'P' => pending.push_str(&context.page.to_string()),
            'N' => pending.push_str(&context.pages.to_string()),
            'D' => pending.push_str(&context.date),
            'T' => pending.push_str(&context.time),
            'F' => pending.push_str(&context.file),
            'Z' => pending.push_str(&context.path),
            'A' => pending.push_str(&context.sheet),
            'G' => {
                push_run(&mut out, section, &mut pending, &style, false);
                push_run(&mut out, section, &mut pending, &style, true);
            }
            '[' => at = bracketed(&chars, at, context, &mut pending, &mut out, section, &style),
            _ => {
                push_run(&mut out, section, &mut pending, &style, false);
                at = style_code(&chars, at - 1, &mut style);
            }
        }
    }
    push_run(&mut out, section, &mut pending, &style, false);
    out
}

/// A code in square brackets such as `&[Page]`; returns where reading goes on.
fn bracketed(
    chars: &[char],
    at: usize,
    context: &Context,
    pending: &mut String,
    out: &mut Sections,
    section: Section,
    style: &RunStyle,
) -> usize {
    let Some(close) = chars[at..].iter().position(|&c| c == ']') else {
        pending.push_str("&[");
        return at;
    };
    let word: String = chars[at..at + close].iter().collect();
    match word.to_ascii_lowercase().as_str() {
        "page" => pending.push_str(&context.page.to_string()),
        "pages" => pending.push_str(&context.pages.to_string()),
        "date" => pending.push_str(&context.date),
        "time" => pending.push_str(&context.time),
        "file" => pending.push_str(&context.file),
        "path" => pending.push_str(&context.path),
        "tab" => pending.push_str(&context.sheet),
        "picture" => {
            push_run(out, section, pending, style, false);
            push_run(out, section, pending, style, true);
        }
        _ => {
            pending.push_str("&[");
            pending.push_str(&word);
            pending.push(']');
        }
    }
    at + close + 1
}

/// A code that changes how following text is set. `at` is the index of the code letter; returns the index
/// after everything the code consumed.
fn style_code(chars: &[char], at: usize, style: &mut RunStyle) -> usize {
    match chars[at] {
        'B' => style.bold = !style.bold,
        'I' => style.italic = !style.italic,
        'U' => style.underline = if style.underline == 1 { 0 } else { 1 },
        'E' => style.underline = if style.underline == 2 { 0 } else { 2 },
        'S' => style.strike = !style.strike,
        'X' => style.superscript = !style.superscript,
        'Y' => style.subscript = !style.subscript,
        '"' => return font_code(chars, at + 1, style),
        'K' => return color_code(chars, at + 1, style),
        c if c.is_ascii_digit() => return size_code(chars, at, style),
        _ => {}
    }
    at + 1
}

/// `&"Arial,Bold"`: a font and, after a comma, its style. An empty font keeps the one in force.
fn font_code(chars: &[char], from: usize, style: &mut RunStyle) -> usize {
    let end = chars[from..].iter().position(|&c| c == '"').map_or(chars.len(), |p| from + p);
    let spec: String = chars[from..end].iter().collect();
    let (name, kind) = match spec.split_once(',') {
        Some((n, k)) => (n.trim(), k.trim()),
        None => (spec.trim(), ""),
    };
    if !name.is_empty() {
        style.font = Some(name.to_owned());
    }
    let kind = kind.to_ascii_lowercase();
    if !kind.is_empty() {
        style.bold = kind.contains("bold");
        style.italic = kind.contains("italic");
    }
    (end + 1).min(chars.len())
}

/// `&Krrggbb`, with the six hex digits that follow.
fn color_code(chars: &[char], from: usize, style: &mut RunStyle) -> usize {
    let digits: String = chars[from..].iter().take(6).collect();
    if digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit()) {
        style.color = Some(digits.to_ascii_uppercase());
        return from + 6;
    }
    from
}

/// `&12`: a size in points, as many digits as keep it at or under Excel's largest, 409.
fn size_code(chars: &[char], from: usize, style: &mut RunStyle) -> usize {
    let mut end = from;
    let mut value = 0u32;
    while let Some(d) = chars.get(end).and_then(|c| c.to_digit(10)) {
        let next = value * 10 + d;
        if next > 409 || end - from == 3 {
            break;
        }
        value = next;
        end += 1;
    }
    if value > 0 {
        style.size = Some(f64::from(value));
    }
    end
}

/// Close the text gathered so far as a run in the current section.
fn push_run(
    out: &mut Sections,
    section: Section,
    pending: &mut String,
    style: &RunStyle,
    picture: bool,
) {
    if pending.is_empty() && !picture {
        return;
    }
    let run = Run { text: std::mem::take(pending), style: style.clone(), picture };
    match section {
        Section::Left => out.left.push(run),
        Section::Center => out.center.push(run),
        Section::Right => out.right.push(run),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        Context {
            page: 3,
            pages: 12,
            date: "10/24/2026".into(),
            time: "1:05 PM".into(),
            file: "Budget.xlsx".into(),
            path: "C:\\Work".into(),
            sheet: "Q3".into(),
        }
    }

    #[test]
    fn the_three_sections_and_their_codes() {
        let s = expand("&LPage &P of &N&C&A&R&D &T", &context());
        assert_eq!(Sections::plain(&s.left), "Page 3 of 12");
        assert_eq!(Sections::plain(&s.center), "Q3");
        assert_eq!(Sections::plain(&s.right), "10/24/2026 1:05 PM");
    }

    #[test]
    fn text_before_any_section_is_centred() {
        let s = expand("Confidential", &context());
        assert_eq!(Sections::plain(&s.center), "Confidential");
        assert!(s.left.is_empty() && s.right.is_empty());
    }

    #[test]
    fn file_and_path_and_the_bracketed_forms() {
        let s =
            expand("&L&F&C&Z&R&[Page]/&[Pages] &[Tab] &[Date] &[Time] &[File] &[Path]", &context());
        assert_eq!(Sections::plain(&s.left), "Budget.xlsx");
        assert_eq!(Sections::plain(&s.center), "C:\\Work");
        assert_eq!(Sections::plain(&s.right), "3/12 Q3 10/24/2026 1:05 PM Budget.xlsx C:\\Work");
    }

    #[test]
    fn an_ampersand_is_written_twice() {
        let s = expand("&CSmith && Sons", &context());
        assert_eq!(Sections::plain(&s.center), "Smith & Sons");
    }

    #[test]
    fn a_font_a_size_and_effects_start_new_runs() {
        let s = expand("&L&\"Arial,Bold\"&14Title&B plain&I italic", &context());
        assert_eq!(s.left.len(), 3);
        assert_eq!(s.left[0].text, "Title");
        assert_eq!(s.left[0].style.font.as_deref(), Some("Arial"));
        assert_eq!(s.left[0].style.size, Some(14.0));
        assert!(s.left[0].style.bold);
        assert_eq!(s.left[1].text, " plain");
        assert!(!s.left[1].style.bold, "&B toggles bold off again");
        assert_eq!(s.left[1].style.font.as_deref(), Some("Arial"), "the font stays in force");
        assert!(s.left[2].style.italic);
    }

    #[test]
    fn underline_strike_and_position_codes() {
        let s = expand("&U1&U &E2&E &S3&S &X4&X &Y5&Y", &context());
        let runs: Vec<(&str, u8, bool, bool, bool)> = s
            .center
            .iter()
            .map(|r| {
                (
                    r.text.as_str(),
                    r.style.underline,
                    r.style.strike,
                    r.style.superscript,
                    r.style.subscript,
                )
            })
            .collect();
        assert_eq!(runs[0], ("1", 1, false, false, false));
        assert_eq!(runs[2], ("2", 2, false, false, false));
        assert_eq!(runs[4], ("3", 0, true, false, false));
        assert_eq!(runs[6], ("4", 0, false, true, false));
        assert_eq!(runs[8], ("5", 0, false, false, true));
    }

    #[test]
    fn a_colour_and_a_picture() {
        let s = expand("&KFF0000Red&G", &context());
        assert_eq!(s.center[0].style.color.as_deref(), Some("FF0000"));
        assert!(s.center.last().unwrap().picture);
    }

    #[test]
    fn a_size_takes_digits_up_to_the_largest_size() {
        let s = expand("&12 pt &8888", &context());
        assert_eq!(s.center[0].style.size, Some(12.0));
        assert_eq!(s.center[0].text, " pt ");
        assert_eq!(s.center[1].style.size, Some(88.0));
        assert_eq!(s.center[1].text, "88");
    }

    #[test]
    fn a_trailing_ampersand_is_kept() {
        assert_eq!(Sections::plain(&expand("R&", &context()).center), "R&");
    }
}
