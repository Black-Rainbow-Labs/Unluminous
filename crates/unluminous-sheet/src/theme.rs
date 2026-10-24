//! The workbook theme: the twelve colours and two fonts that cells name instead of a colour or a font.
//!
//! A cell that is coloured `Accent 1, lighter 80%` holds `theme="4" tint="0.8"`, and a font set in the
//! body font holds `scheme="minor"`. Changing the theme changes what those draw as. The theme is the part
//! `xl/theme/theme1.xml`; this module reads and writes its colour scheme and its font scheme in place and
//! ships the themes Excel offers on the Page Layout tab as theme XML.

use roxmltree::Document;

use crate::xlsx::DRAWING_NS;

/// The names of the twelve colour slots in file order.
pub const SLOTS: [&str; 12] = [
    "dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6",
    "hlink", "folHlink",
];

/// The names Excel's Theme Colors dialog gives the slots.
pub const SLOT_NAMES: [&str; 12] = [
    "Text/Background - Dark 1",
    "Text/Background - Light 1",
    "Text/Background - Dark 2",
    "Text/Background - Light 2",
    "Accent 1",
    "Accent 2",
    "Accent 3",
    "Accent 4",
    "Accent 5",
    "Accent 6",
    "Hyperlink",
    "Followed Hyperlink",
];

/// A theme Excel offers, as the theme part it writes into a workbook.
#[derive(Debug, Clone, Copy)]
pub struct BuiltInTheme {
    pub name: &'static str,
    pub xml: &'static str,
}

/// Excel's Office theme as a new workbook carries it, then the themes of Excel's Themes gallery that need
/// no picture: the Office theme of 2023, the Office theme of 2013 to 2022, and eight others. The files
/// are the `theme1.xml` of Excel's own `.thmx` files.
pub const BUILT_IN: &[BuiltInTheme] = &[
    BuiltInTheme { name: "Office", xml: include_str!("xlsx/theme.xml") },
    BuiltInTheme { name: "Office Theme", xml: include_str!("themes/office-theme.xml") },
    BuiltInTheme {
        name: "Office 2013 - 2022 Theme",
        xml: include_str!("themes/office-2013-2022.xml"),
    },
    BuiltInTheme { name: "Facet", xml: include_str!("themes/facet.xml") },
    BuiltInTheme { name: "Retrospect", xml: include_str!("themes/retrospect.xml") },
    BuiltInTheme { name: "Slice", xml: include_str!("themes/slice.xml") },
    BuiltInTheme { name: "Wisp", xml: include_str!("themes/wisp.xml") },
    BuiltInTheme { name: "Gallery", xml: include_str!("themes/gallery.xml") },
    BuiltInTheme { name: "Frame", xml: include_str!("themes/frame.xml") },
    BuiltInTheme { name: "Dividend", xml: include_str!("themes/dividend.xml") },
    BuiltInTheme { name: "Modulo", xml: include_str!("themes/modulo.xml") },
];

/// The built in theme with a name, ignoring case.
pub fn built_in(name: &str) -> Option<&'static BuiltInTheme> {
    BUILT_IN.iter().find(|t| t.name.eq_ignore_ascii_case(name.trim()))
}

/// One colour of a colour scheme: a colour, or a system colour with the colour it was last seen as.
#[derive(Debug, Clone, PartialEq)]
pub enum ThemeColor {
    Rgb(u32),
    /// `windowText` and `window`, the system colours Office's dark 1 and light 1 are.
    System { name: String, last: u32 },
}

impl ThemeColor {
    /// The colour as `RRGGBB`.
    pub fn rgb(&self) -> u32 {
        match self {
            ThemeColor::Rgb(v) => *v,
            ThemeColor::System { last, .. } => *last,
        }
    }
}

/// The colour scheme of a theme.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeColors {
    pub name: String,
    pub slots: Vec<ThemeColor>,
}

/// The font scheme of a theme: the heading font and the body font, by their Latin typeface.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeFonts {
    pub name: String,
    pub major: String,
    pub minor: String,
}

/// The name of a theme (`<a:theme name="...">`).
pub fn theme_name(xml: &str) -> Option<String> {
    let doc = Document::parse(xml).ok()?;
    doc.root_element().attribute("name").map(str::to_owned)
}

/// Read the colour scheme of a theme part.
pub fn read_colors(xml: &str) -> Option<ThemeColors> {
    let doc = Document::parse(xml).ok()?;
    let scheme = doc.descendants().find(|n| n.has_tag_name((DRAWING_NS, "clrScheme")))?;
    let mut slots = Vec::new();
    for slot in SLOTS {
        let value = scheme
            .children()
            .find(|n| n.has_tag_name((DRAWING_NS, slot)))?
            .children()
            .find(|n| n.is_element())?;
        let parse = |v: Option<&str>| v.and_then(|v| u32::from_str_radix(v, 16).ok()).unwrap_or(0);
        slots.push(match value.tag_name().name() {
            "sysClr" => ThemeColor::System {
                name: value.attribute("val").unwrap_or("").to_owned(),
                last: parse(value.attribute("lastClr")),
            },
            _ => ThemeColor::Rgb(parse(value.attribute("val"))),
        });
    }
    Some(ThemeColors { name: scheme.attribute("name").unwrap_or("").to_owned(), slots })
}

/// Write a colour scheme into a theme part, replacing the one there.
pub fn write_colors(xml: &str, colors: &ThemeColors) -> String {
    let Some(start) = xml.find("<a:clrScheme") else { return xml.to_owned() };
    let Some(end) = xml[start..].find("</a:clrScheme>").map(|e| start + e + "</a:clrScheme>".len())
    else {
        return xml.to_owned();
    };
    let mut out = format!("<a:clrScheme name=\"{}\">", crate::xlsx::escape(&colors.name));
    for (slot, color) in SLOTS.iter().zip(&colors.slots) {
        let inner = match color {
            ThemeColor::Rgb(v) => format!("<a:srgbClr val=\"{v:06X}\"/>"),
            ThemeColor::System { name, last } => {
                format!("<a:sysClr val=\"{}\" lastClr=\"{last:06X}\"/>", crate::xlsx::escape(name))
            }
        };
        out.push_str(&format!("<a:{slot}>{inner}</a:{slot}>"));
    }
    out.push_str("</a:clrScheme>");
    format!("{}{out}{}", &xml[..start], &xml[end..])
}

/// Read the font scheme of a theme part.
pub fn read_fonts(xml: &str) -> Option<ThemeFonts> {
    let doc = Document::parse(xml).ok()?;
    let scheme = doc.descendants().find(|n| n.has_tag_name((DRAWING_NS, "fontScheme")))?;
    let latin = |group: &str| {
        scheme
            .children()
            .find(|n| n.has_tag_name((DRAWING_NS, group)))
            .and_then(|f| f.children().find(|n| n.has_tag_name((DRAWING_NS, "latin"))))
            .and_then(|l| l.attribute("typeface"))
            .map(str::to_owned)
    };
    Some(ThemeFonts {
        name: scheme.attribute("name").unwrap_or("").to_owned(),
        major: latin("majorFont")?,
        minor: latin("minorFont")?,
    })
}

/// Replace the Latin typeface of the heading font or the body font, and the font scheme's name. The
/// typeface's panose number describes the old font, so it is dropped.
pub fn write_fonts(xml: &str, major: Option<&str>, minor: Option<&str>, name: Option<&str>) -> String {
    let mut out = xml.to_owned();
    for (group, face) in [("majorFont", major), ("minorFont", minor)] {
        let Some(face) = face else { continue };
        let Some(group_at) = out.find(&format!("<a:{group}>")) else { continue };
        let Some(latin_at) = out[group_at..].find("<a:latin ").map(|i| group_at + i) else { continue };
        let Some(end) = out[latin_at..].find("/>").map(|i| latin_at + i + 2) else { continue };
        out.replace_range(latin_at..end, &format!("<a:latin typeface=\"{}\"/>", crate::xlsx::escape(face)));
    }
    if let Some(name) = name {
        if let Some(at) = out.find("<a:fontScheme name=\"") {
            let from = at + "<a:fontScheme name=\"".len();
            if let Some(len) = out[from..].find('"') {
                out.replace_range(from..from + len, &crate::xlsx::escape(name));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_theme_has_twelve_colours_and_two_fonts() {
        for theme in BUILT_IN {
            let colors = read_colors(theme.xml).unwrap_or_else(|| panic!("{} has colours", theme.name));
            assert_eq!(colors.slots.len(), 12, "{}", theme.name);
            let fonts = read_fonts(theme.xml).unwrap_or_else(|| panic!("{} has fonts", theme.name));
            assert!(!fonts.major.is_empty() && !fonts.minor.is_empty(), "{}", theme.name);
        }
        assert!(BUILT_IN.len() >= 6);
    }

    #[test]
    fn facet_is_the_theme_excel_applies() {
        let facet = built_in("facet").unwrap();
        let colors = read_colors(facet.xml).unwrap();
        // Accent 1 of Facet and its fonts, as Excel reported them after applying Facet.thmx.
        assert_eq!(colors.slots[4], ThemeColor::Rgb(0x90C226));
        let fonts = read_fonts(facet.xml).unwrap();
        assert_eq!((fonts.major.as_str(), fonts.minor.as_str()), ("Trebuchet MS", "Trebuchet MS"));
    }

    #[test]
    fn colours_and_fonts_are_written_in_place() {
        let office = built_in("Office").unwrap();
        let mut colors = read_colors(office.xml).unwrap();
        colors.slots[4] = ThemeColor::Rgb(0xFF0000);
        let changed = write_colors(office.xml, &colors);
        assert_eq!(read_colors(&changed).unwrap().slots[4], ThemeColor::Rgb(0xFF0000));
        assert_eq!(read_colors(&changed).unwrap().slots[0], colors.slots[0], "the system colour stays");
        let fonts = write_fonts(&changed, Some("Georgia"), Some("Courier New"), None);
        let read = read_fonts(&fonts).unwrap();
        assert_eq!((read.major.as_str(), read.minor.as_str()), ("Georgia", "Courier New"));
        assert!(roxmltree::Document::parse(&fonts).is_ok());
    }
}
