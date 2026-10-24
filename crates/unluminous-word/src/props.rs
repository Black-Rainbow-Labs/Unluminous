//! Reading and changing the properties elements, and the resolved values they come to.
//!
//! A `w:rPr` or `w:pPr` is kept as the [`El`] it was read as. The functions here read one setting out of
//! one such element (`None` when that element does not say) and write one into it, so resolving a style
//! chain is a matter of asking each element in turn, and a change touches only the child it is about.

use crate::xml::El;

/// Twentieths of a point to points.
pub fn twips(value: f64) -> f32 {
    (value / 20.0) as f32
}

/// English Metric Units (914,400 to the inch) to points.
pub fn emu(value: f64) -> f32 {
    (value / 12_700.0) as f32
}

/// A number attribute, which Word writes as an integer and other programs sometimes as `12.5` or `12pt`.
pub fn number(text: &str) -> Option<f64> {
    let text = text.trim();
    let digits = text.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let value: f64 = digits.parse().ok()?;
    Some(match &text[digits.len()..] {
        "pt" => value * 20.0,
        "in" => value * 1440.0,
        "cm" => value * 566.929,
        "mm" => value * 56.6929,
        _ => value,
    })
}

/// An attribute of a child, as a number.
pub fn child_number(el: &El, child: &str, attr: &str) -> Option<f64> {
    el.child(child).and_then(|child| child.attr(attr)).and_then(number)
}

/// An on/off property (`w:b`, `w:keepNext`): present with no value, `true`, `1` or `on` is on.
pub fn on_off(el: &El, child: &str) -> Option<bool> {
    el.child(child)
        .map(|child| !matches!(child.attr("w:val"), Some("0" | "false" | "off" | "none")))
}

/// An on/off attribute (`w:default="1"`), which is off when it is missing.
pub fn on_off_attr(value: Option<&str>) -> bool {
    matches!(value, Some("1" | "true" | "on"))
}

/// Set or clear an on/off property. `None` removes it, so the value comes from the style again.
pub fn set_on_off(el: &mut El, child: &str, value: Option<bool>) {
    match value {
        None => el.remove_children(child),
        Some(true) => el.put(El::new(child)),
        Some(false) => el.put(El::new(child).with("w:val", "0")),
    }
}

/// Set or clear a property with one `w:val`.
pub fn set_val(el: &mut El, child: &str, value: Option<&str>) {
    match value {
        None => el.remove_children(child),
        Some(value) => el.put(El::new(child).with("w:val", value)),
    }
}

/// An RGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);
    pub const WHITE: Rgb = Rgb(255, 255, 255);

    /// `RRGGBB`, as the file writes it.
    pub fn parse(hex: &str) -> Option<Rgb> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
        Some(Rgb(byte(0)?, byte(2)?, byte(4)?))
    }

    pub fn hex(self) -> String {
        format!("{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }

    /// Word's `themeShade` (towards black) and `themeTint` (towards white), each a byte where FF is none.
    pub fn shaded(self, shade: Option<u8>, tint: Option<u8>) -> Rgb {
        let mut out = self;
        if let Some(shade) = shade {
            let f = f32::from(shade) / 255.0;
            out = Rgb(
                (f32::from(out.0) * f) as u8,
                (f32::from(out.1) * f) as u8,
                (f32::from(out.2) * f) as u8,
            );
        }
        if let Some(tint) = tint {
            let f = f32::from(tint) / 255.0;
            let up = |c: u8| (f32::from(c) * f + 255.0 * (1.0 - f)).round() as u8;
            out = Rgb(up(out.0), up(out.1), up(out.2));
        }
        out
    }
}

/// The sixteen highlight colours, by the names `w:highlight` uses.
pub const HIGHLIGHTS: &[(&str, Rgb)] = &[
    ("yellow", Rgb(255, 255, 0)),
    ("green", Rgb(0, 255, 0)),
    ("cyan", Rgb(0, 255, 255)),
    ("magenta", Rgb(255, 0, 255)),
    ("blue", Rgb(0, 0, 255)),
    ("red", Rgb(255, 0, 0)),
    ("darkBlue", Rgb(0, 0, 128)),
    ("darkCyan", Rgb(0, 128, 128)),
    ("darkGreen", Rgb(0, 128, 0)),
    ("darkMagenta", Rgb(128, 0, 128)),
    ("darkRed", Rgb(128, 0, 0)),
    ("darkYellow", Rgb(128, 128, 0)),
    ("darkGray", Rgb(128, 128, 128)),
    ("lightGray", Rgb(192, 192, 192)),
    ("black", Rgb(0, 0, 0)),
    ("white", Rgb(255, 255, 255)),
];

pub fn highlight_colour(name: &str) -> Option<Rgb> {
    HIGHLIGHTS.iter().find(|(known, _)| *known == name).map(|(_, rgb)| *rgb)
}

/// How a run is underlined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Underline {
    #[default]
    None,
    Single,
    Words,
    Double,
    Thick,
    Dotted,
    Dash,
    DotDash,
    Wave,
}

impl Underline {
    pub fn parse(value: &str) -> Underline {
        match value {
            "none" => Underline::None,
            "words" => Underline::Words,
            "double" | "wavyDouble" => Underline::Double,
            "thick" | "dottedHeavy" | "dashedHeavy" | "dashLongHeavy" | "dashDotHeavy"
            | "dashDotDotHeavy" | "wavyHeavy" => Underline::Thick,
            "dotted" => Underline::Dotted,
            "dash" | "dashLong" => Underline::Dash,
            "dotDash" | "dotDotDash" => Underline::DotDash,
            "wave" => Underline::Wave,
            _ => Underline::Single,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Underline::None => "none",
            Underline::Single => "single",
            Underline::Words => "words",
            Underline::Double => "double",
            Underline::Thick => "thick",
            Underline::Dotted => "dotted",
            Underline::Dash => "dash",
            Underline::DotDash => "dotDash",
            Underline::Wave => "wave",
        }
    }
}

/// Raised, lowered or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum VertAlign {
    #[default]
    Baseline,
    Superscript,
    Subscript,
}

/// A run's formatting once the styles are applied.
#[derive(Debug, Clone, PartialEq)]
pub struct RunLook {
    pub font: std::sync::Arc<str>,
    /// Points.
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: Underline,
    pub strike: bool,
    pub double_strike: bool,
    pub caps: bool,
    pub small_caps: bool,
    pub hidden: bool,
    /// `None` is automatic: black on a light ground.
    pub color: Option<Rgb>,
    pub highlight: Option<Rgb>,
    pub shading: Option<Rgb>,
    pub vert: VertAlign,
    /// Raised (positive) or lowered, in points.
    pub position: f32,
    /// Added after every character, in points.
    pub spacing: f32,
    /// Horizontal scale, 1.0 for none.
    pub scale: f32,
    /// `w:kern`: the size in points from which pairs of letters are kerned, or `None` for no kerning.
    pub kern: Option<f32>,
}

impl Default for RunLook {
    fn default() -> Self {
        RunLook {
            font: "Times New Roman".into(),
            size: 10.0,
            bold: false,
            italic: false,
            underline: Underline::None,
            strike: false,
            double_strike: false,
            caps: false,
            small_caps: false,
            hidden: false,
            color: None,
            highlight: None,
            shading: None,
            vert: VertAlign::Baseline,
            position: 0.0,
            spacing: 0.0,
            scale: 1.0,
            kern: None,
        }
    }
}

/// A paragraph's alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

impl Align {
    pub fn parse(value: &str) -> Align {
        match value {
            "center" => Align::Center,
            "right" | "end" => Align::Right,
            "both" | "distribute" | "lowKashida" | "mediumKashida" | "highKashida"
            | "thaiDistribute" => Align::Justify,
            _ => Align::Left,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => "both",
        }
    }
}

/// How the lines of a paragraph are spaced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineRule {
    /// A multiple of single spacing.
    Auto(f32),
    /// At least this many points.
    AtLeast(f32),
    /// Exactly this many points.
    Exact(f32),
}

/// One tab stop.
#[derive(Debug, Clone, PartialEq)]
pub struct TabStop {
    /// Points from the paragraph's left indent's origin, which is the text area's left edge.
    pub position: f32,
    pub kind: TabKind,
    pub leader: Option<char>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabKind {
    Left,
    Center,
    Right,
    Decimal,
    Bar,
    Clear,
}

/// One border line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    /// Points.
    pub width: f32,
    pub color: Option<Rgb>,
    /// Distance from the text, in points.
    pub space: f32,
    pub double: bool,
}

/// A border element (`w:top`...), or `None` for `nil`, `none` or missing.
pub fn border(el: Option<&El>) -> Option<Border> {
    let el = el?;
    let style = el.attr("w:val").unwrap_or("single");
    if matches!(style, "nil" | "none") {
        return None;
    }
    // `w:sz` is in eighths of a point.
    let width =
        el.attr("w:sz").and_then(number).map_or(0.5, |eighths| (eighths / 8.0) as f32).max(0.25);
    Some(Border {
        width,
        color: el.attr("w:color").and_then(Rgb::parse),
        space: el.attr("w:space").and_then(number).unwrap_or(0.0) as f32,
        double: style.starts_with("double")
            || style.starts_with("thinThick")
            || style.starts_with("thickThin"),
    })
}

/// The four sides and the line between paragraphs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Borders {
    pub top: Option<Border>,
    pub left: Option<Border>,
    pub bottom: Option<Border>,
    pub right: Option<Border>,
    pub between: Option<Border>,
}

/// A paragraph's formatting once the styles and the list are applied.
#[derive(Debug, Clone, PartialEq)]
pub struct ParaLook {
    pub style: String,
    pub align: Align,
    /// Points.
    pub indent_left: f32,
    pub indent_right: f32,
    /// The first line's indent relative to the others: positive for a first line indent, negative for
    /// a hanging one.
    pub indent_first: f32,
    pub space_before: f32,
    pub space_after: f32,
    pub before_auto: bool,
    pub after_auto: bool,
    pub line: LineRule,
    pub keep_next: bool,
    pub keep_lines: bool,
    pub page_break_before: bool,
    pub widow_control: bool,
    /// Leave out spacing between paragraphs of the same style.
    pub contextual: bool,
    pub outline_level: Option<u8>,
    /// The list, as `(num id, level)`, when the paragraph is in one.
    pub numbering: Option<(i64, u8)>,
    pub tabs: Vec<TabStop>,
    pub borders: Borders,
    pub shading: Option<Rgb>,
    pub bidi: bool,
    /// `w:suppressAutoHyphens`: this paragraph is not hyphenated when the document is.
    pub suppress_hyphens: bool,
}

impl Default for ParaLook {
    fn default() -> Self {
        ParaLook {
            style: "Normal".into(),
            align: Align::Left,
            indent_left: 0.0,
            indent_right: 0.0,
            indent_first: 0.0,
            space_before: 0.0,
            space_after: 0.0,
            before_auto: false,
            after_auto: false,
            line: LineRule::Auto(1.0),
            keep_next: false,
            keep_lines: false,
            page_break_before: false,
            widow_control: true,
            contextual: false,
            outline_level: None,
            numbering: None,
            tabs: Vec::new(),
            borders: Borders::default(),
            shading: None,
            bidi: false,
            suppress_hyphens: false,
        }
    }
}

/// Apply one `w:pPr` (or a style's) on top of `look`. Numbering is read by the caller, because which
/// list a paragraph is in decides which level's properties come next.
pub fn apply_paragraph(look: &mut ParaLook, el: &El) {
    if let Some(value) = el.val_of("w:jc") {
        look.align = Align::parse(value);
    }
    if let Some(ind) = el.child("w:ind") {
        let get = |names: &[&str]| names.iter().find_map(|name| ind.attr(name)).and_then(number);
        if let Some(left) = get(&["w:left", "w:start"]) {
            look.indent_left = twips(left);
        }
        if let Some(right) = get(&["w:right", "w:end"]) {
            look.indent_right = twips(right);
        }
        if let Some(hanging) = get(&["w:hanging"]) {
            look.indent_first = -twips(hanging);
        } else if let Some(first) = get(&["w:firstLine"]) {
            look.indent_first = twips(first);
        }
    }
    if let Some(spacing) = el.child("w:spacing") {
        if let Some(before) = spacing.attr("w:before").and_then(number) {
            look.space_before = twips(before);
        }
        if let Some(after) = spacing.attr("w:after").and_then(number) {
            look.space_after = twips(after);
        }
        if let Some(auto) = spacing.attr("w:beforeAutospacing") {
            look.before_auto = !matches!(auto, "0" | "false" | "off");
        }
        if let Some(auto) = spacing.attr("w:afterAutospacing") {
            look.after_auto = !matches!(auto, "0" | "false" | "off");
        }
        if let Some(line) = spacing.attr("w:line").and_then(number) {
            look.line = match spacing.attr("w:lineRule") {
                Some("exact") => LineRule::Exact(twips(line.abs())),
                Some("atLeast") => LineRule::AtLeast(twips(line)),
                _ => LineRule::Auto((line / 240.0) as f32),
            };
        }
    }
    for (name, slot) in [
        ("w:keepNext", &mut look.keep_next),
        ("w:keepLines", &mut look.keep_lines),
        ("w:pageBreakBefore", &mut look.page_break_before),
        ("w:widowControl", &mut look.widow_control),
        ("w:contextualSpacing", &mut look.contextual),
        ("w:bidi", &mut look.bidi),
        ("w:suppressAutoHyphens", &mut look.suppress_hyphens),
    ] {
        if let Some(value) = on_off(el, name) {
            *slot = value;
        }
    }
    if let Some(level) = el.val_of("w:outlineLvl").and_then(number) {
        look.outline_level = (level < 9.0).then_some(level as u8);
    }
    if let Some(tabs) = el.child("w:tabs") {
        for tab in tabs.children_named("w:tab") {
            let Some(position) = tab.attr("w:pos").and_then(number) else { continue };
            let position = twips(position);
            let kind = match tab.attr("w:val").unwrap_or("left") {
                "center" => TabKind::Center,
                "right" | "end" => TabKind::Right,
                "decimal" => TabKind::Decimal,
                "bar" => TabKind::Bar,
                "clear" => TabKind::Clear,
                _ => TabKind::Left,
            };
            let leader = match tab.attr("w:leader") {
                Some("dot") => Some('.'),
                Some("hyphen") => Some('-'),
                Some("underscore" | "heavy") => Some('_'),
                Some("middleDot") => Some('·'),
                _ => None,
            };
            look.tabs.retain(|stop| (stop.position - position).abs() > 0.5);
            if kind != TabKind::Clear {
                look.tabs.push(TabStop { position, kind, leader });
            }
        }
        look.tabs.sort_by(|a, b| a.position.total_cmp(&b.position));
    }
    if let Some(borders) = el.child("w:pBdr") {
        look.borders = Borders {
            top: border(borders.child("w:top")),
            left: border(borders.child("w:left").or(borders.child("w:start"))),
            bottom: border(borders.child("w:bottom")),
            right: border(borders.child("w:right").or(borders.child("w:end"))),
            between: border(borders.child("w:between")),
        };
    }
    if let Some(shd) = el.child("w:shd") {
        look.shading = shading(shd);
    }
}

/// A `w:shd`'s fill, when it is a colour rather than `auto` or clear.
pub fn shading(shd: &El) -> Option<Rgb> {
    match shd.attr("w:fill") {
        Some("auto") | None => None,
        Some(fill) => Rgb::parse(fill),
    }
}

/// The toggle properties, whose value in a style flips the inherited one rather than setting it.
pub const TOGGLES: &[&str] =
    &["w:b", "w:i", "w:caps", "w:smallCaps", "w:strike", "w:dstrike", "w:vanish"];

/// The fonts a `w:rFonts` names, as `(ascii, theme)` where the theme name (`minorHAnsi`) wins.
pub fn fonts(el: &El) -> (Option<String>, Option<String>) {
    let Some(fonts) = el.child("w:rFonts") else { return (None, None) };
    let theme = fonts.attr("w:asciiTheme").or(fonts.attr("w:hAnsiTheme")).map(str::to_owned);
    let name = fonts.attr("w:ascii").or(fonts.attr("w:hAnsi")).map(str::to_owned);
    (name, theme)
}

/// Apply the properties of one `w:rPr` that are not toggles on top of `look`. `theme` answers a theme
/// font name and a theme colour.
pub fn apply_run(look: &mut RunLook, el: &El, theme: &crate::styles::Theme) {
    match fonts(el) {
        (_, Some(theme_font)) => look.font = theme.font(&theme_font).into(),
        (Some(name), None) => look.font = name.into(),
        _ => {}
    }
    if let Some(size) = el.val_of("w:sz").and_then(number) {
        look.size = (size / 2.0) as f32;
    }
    if let Some(color) = el.child("w:color") {
        look.color = match (color.attr("w:val"), color.attr("w:themeColor")) {
            (Some("auto"), None) => None,
            (Some(value), _) if value != "auto" => Rgb::parse(value),
            (_, Some(name)) => theme.colour(name).map(|rgb| {
                let byte =
                    |attr: &str| color.attr(attr).and_then(|hex| u8::from_str_radix(hex, 16).ok());
                rgb.shaded(byte("w:themeShade"), byte("w:themeTint"))
            }),
            _ => None,
        };
    }
    if let Some(u) = el.child("w:u") {
        look.underline = Underline::parse(u.attr("w:val").unwrap_or("single"));
    }
    if let Some(name) = el.val_of("w:highlight") {
        look.highlight = highlight_colour(name);
    }
    if let Some(shd) = el.child("w:shd") {
        look.shading = shading(shd);
    }
    if let Some(value) = el.val_of("w:vertAlign") {
        look.vert = match value {
            "superscript" => VertAlign::Superscript,
            "subscript" => VertAlign::Subscript,
            _ => VertAlign::Baseline,
        };
    }
    if let Some(position) = el.val_of("w:position").and_then(number) {
        look.position = (position / 2.0) as f32;
    }
    if let Some(spacing) = el.val_of("w:spacing").and_then(number) {
        look.spacing = twips(spacing);
    }
    if let Some(scale) = el.val_of("w:w").and_then(number) {
        look.scale = (scale / 100.0) as f32;
    }
    if let Some(kern) = el.val_of("w:kern").and_then(number) {
        look.kern = (kern > 0.0).then_some((kern / 2.0) as f32);
    }
}

/// Set the toggles of `look` that `el` sets absolutely, which is what direct formatting does.
pub fn apply_toggles_absolute(look: &mut RunLook, el: &El) {
    for name in TOGGLES {
        if let Some(value) = on_off(el, name) {
            *toggle_slot(look, name) = value;
        }
    }
}

/// The field of `look` a toggle property sets.
pub fn toggle_slot<'a>(look: &'a mut RunLook, name: &str) -> &'a mut bool {
    match name {
        "w:b" => &mut look.bold,
        "w:i" => &mut look.italic,
        "w:caps" => &mut look.caps,
        "w:smallCaps" => &mut look.small_caps,
        "w:strike" => &mut look.strike,
        "w:dstrike" => &mut look.double_strike,
        _ => &mut look.hidden,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_the_way_word_and_other_programs_write_them() {
        assert_eq!(number("240"), Some(240.0));
        assert_eq!(number("12pt"), Some(240.0));
        assert_eq!(number("1in"), Some(1440.0));
        assert_eq!(number("x"), None);
    }

    #[test]
    fn indents_and_spacing_read_in_points() {
        let el = El::new("w:pPr")
            .with_child(El::new("w:ind").with("w:left", "720").with("w:hanging", "360"))
            .with_child(
                El::new("w:spacing")
                    .with("w:after", "160")
                    .with("w:line", "259")
                    .with("w:lineRule", "auto"),
            )
            .with_child(El::new("w:jc").with("w:val", "both"));
        let mut look = ParaLook::default();
        apply_paragraph(&mut look, &el);
        assert_eq!((look.indent_left, look.indent_first), (36.0, -18.0));
        assert_eq!(look.space_after, 8.0);
        assert!(matches!(look.line, LineRule::Auto(m) if (m - 1.079).abs() < 0.001));
        assert_eq!(look.align, Align::Justify);
    }

    #[test]
    fn a_theme_shade_darkens_and_a_tint_lightens() {
        let blue = Rgb(0x44, 0x72, 0xC4);
        assert_eq!(blue.shaded(Some(0xBF), None), Rgb(0x32, 0x55, 0x92));
        assert_eq!(Rgb(0, 0, 0).shaded(None, Some(0x80)), Rgb(127, 127, 127));
    }

    #[test]
    fn on_off_reads_every_spelling_of_off() {
        for (value, expected) in [
            (None, true),
            (Some("1"), true),
            (Some("true"), true),
            (Some("0"), false),
            (Some("false"), false),
            (Some("off"), false),
        ] {
            let mut b = El::new("w:b");
            if let Some(value) = value {
                b.set_attr("w:val", value);
            }
            assert_eq!(on_off(&El::new("w:rPr").with_child(b), "w:b"), Some(expected));
        }
        assert_eq!(on_off(&El::new("w:rPr"), "w:b"), None);
    }
}
