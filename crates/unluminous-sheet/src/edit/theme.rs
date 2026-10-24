//! Applying a theme, and changing a theme's colours and fonts. A cell that names a theme colour then
//! draws the new colour, and a font that follows the theme takes the new font name, as in Excel.

use crate::model::Relationship;
use crate::theme::{
    built_in, color_scheme, font_scheme, read_colors, read_fonts, write_colors, write_fonts,
    ThemeColor, ThemeColors, ThemeFonts, SLOTS,
};
use crate::xlsx::{next_id, resolve, write::DEFAULT_THEME_XML, CT_THEME, REL_THEME};

use super::Session;

impl Session {
    /// The path of the workbook's theme part, made when the workbook has none.
    fn ensure_theme_part(&mut self) -> String {
        let workbook_part = if self.book.part.is_empty() {
            "xl/workbook.xml".to_owned()
        } else {
            self.book.part.clone()
        };
        if let Some(rel) = self.book.kept.workbook_rels.iter().find(|r| r.kind == REL_THEME) {
            return resolve(&workbook_part, &rel.target);
        }
        let path = (1..)
            .map(|n| format!("xl/theme/theme{n}.xml"))
            .find(|p| self.book.kept.get(p).is_none())
            .unwrap_or_default();
        let id = next_id(&self.book.kept.workbook_rels);
        self.book.kept.workbook_rels.push(Relationship {
            id,
            kind: REL_THEME.to_owned(),
            target: crate::xlsx::relative(&workbook_part, &path),
            external: false,
        });
        self.book.kept.overrides.push((format!("/{path}"), CT_THEME.to_owned()));
        self.book.kept.set(&path, DEFAULT_THEME_XML.as_bytes().to_vec());
        path
    }

    /// The theme part as text: the workbook's, or Excel's Office theme when the workbook has none yet.
    pub fn theme_xml(&self) -> String {
        let part = self.book.kept.workbook_rels.iter().find(|r| r.kind == REL_THEME).map(|r| {
            resolve(
                if self.book.part.is_empty() { "xl/workbook.xml" } else { &self.book.part },
                &r.target,
            )
        });
        part.and_then(|p| self.book.kept.get(&p).map(|b| String::from_utf8_lossy(b).into_owned()))
            .unwrap_or_else(|| DEFAULT_THEME_XML.to_owned())
    }

    /// The twelve colours of the theme.
    pub fn theme_colors(&self) -> Option<ThemeColors> {
        read_colors(&self.theme_xml())
    }

    /// The heading font and body font of the theme.
    pub fn theme_fonts(&self) -> Option<ThemeFonts> {
        read_fonts(&self.theme_xml())
    }

    /// Put a theme part in the workbook and make the formatting follow it.
    fn put_theme(&mut self, xml: String) {
        let part = self.ensure_theme_part();
        crate::xlsx::read::read_theme(&xml, &mut self.book.styles);
        self.book.styles.refresh_scheme_fonts();
        // `defaultThemeVersion` says the theme is the Office theme of that year, and Excel shows that
        // theme whatever the part holds. Excel drops the attribute when a workbook's theme is changed.
        self.book.workbook_pr.retain(|(k, _)| k != "defaultThemeVersion");
        self.book.kept.set(&part, xml.into_bytes());
    }

    /// Apply a theme from Page Layout, Themes: its colours, its fonts and its effects. Every colour a cell
    /// names by theme index and every font that follows the theme change with it.
    pub fn apply_theme(&mut self, name: &str) -> Result<(), String> {
        let theme = built_in(name).ok_or_else(|| format!("There is no theme named {name}."))?;
        self.remember_whole("Theme");
        self.put_theme(theme.xml.to_owned());
        Ok(())
    }

    /// Take a colour scheme from Page Layout, Colors (`Blue Warm`, `Grayscale`), or the colours of a theme.
    pub fn apply_theme_colors(&mut self, from: &str) -> Result<(), String> {
        let colors =
            color_scheme(from).ok_or_else(|| format!("There is no colour scheme named {from}."))?;
        self.remember_whole("Theme colors");
        let xml = write_colors(&self.theme_xml(), &colors);
        self.put_theme(xml);
        Ok(())
    }

    /// Take a font scheme from Page Layout, Fonts (`Arial`, `Calibri-Cambria`), or the fonts of a theme.
    pub fn apply_theme_fonts(&mut self, from: &str) -> Result<(), String> {
        let fonts =
            font_scheme(from).ok_or_else(|| format!("There is no font scheme named {from}."))?;
        self.remember_whole("Theme fonts");
        let xml = write_fonts(
            &self.theme_xml(),
            Some(&fonts.major),
            Some(&fonts.minor),
            Some(&fonts.name),
        );
        self.put_theme(xml);
        Ok(())
    }

    /// Change colours of the theme by slot (0 is Dark 1, 4 is Accent 1, 10 is Hyperlink; see
    /// [`SLOTS`]), as `RRGGBB`. A name for the changed colour scheme is optional.
    pub fn set_theme_colors(
        &mut self,
        changes: &[(usize, u32)],
        name: Option<&str>,
    ) -> Result<(), String> {
        let mut colors = self.theme_colors().ok_or("the workbook theme has no colours")?;
        for (slot, rgb) in changes {
            let Some(target) = colors.slots.get_mut(*slot) else {
                return Err(format!("A theme has {} colours, numbered from 0.", SLOTS.len()));
            };
            *target = ThemeColor::Rgb(*rgb & 0xFF_FFFF);
        }
        if let Some(name) = name {
            colors.name = name.to_owned();
        }
        self.remember_whole("Theme colors");
        let xml = write_colors(&self.theme_xml(), &colors);
        self.put_theme(xml);
        Ok(())
    }

    /// Change the heading font and the body font of the theme (Page Layout, Fonts, Customize Fonts).
    pub fn set_theme_fonts(
        &mut self,
        major: Option<&str>,
        minor: Option<&str>,
    ) -> Result<(), String> {
        if major.is_none() && minor.is_none() {
            return Err("Name a heading font, a body font or both.".to_owned());
        }
        self.remember_whole("Theme fonts");
        let xml = write_fonts(&self.theme_xml(), major, minor, None);
        self.put_theme(xml);
        Ok(())
    }
}
