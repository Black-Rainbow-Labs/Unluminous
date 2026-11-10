//! Termoil's 894 color themes, for the terminals in Unluminous (`task-2597`).
//!
//! `themes/themes.txt` is a copy of Termoil's own data file, one theme per line, and `themes/README` says
//! which Termoil commit it came from. It is copied rather than depended on for the reason `sprite.rs` is:
//! the crate that owns it, `termoil-app`, brings `winit`, `wgpu` and an `objc2` that `unluminous-app` cannot
//! share. A theme added in Termoil is copied here.
//!
//! A theme only chooses colors. Which theme the terminals use is the window's `terminal.theme` setting, and
//! an empty setting keeps the palette the window's own theme gives, which is what every terminal had before.

use std::sync::OnceLock;

use crate::palette::{Palette, Rgb};

/// The data file, one theme per line.
const THEME_DATA: &str = include_str!("../themes/themes.txt");

/// The themes Unluminous ships, which the theme picker lists before the rest, in this order. They are the
/// themes Termoil's `chrome.txt` names, so the two programs put the same ones first.
pub const FEATURED: &[&str] = &[
    "Unluminous Dark",
    "Unluminous Light",
    "Islands Dracula Colorful",
    "Material Palenight",
    "Material Deep Ocean",
    "Monokai Pro",
    "One Dark",
];

/// One color theme: its name, its default colors and its sixteen named colors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub selection_background: Rgb,
    pub named: [Rgb; 16],
}

impl Theme {
    /// The palette a terminal is drawn in under this theme.
    pub fn palette(&self) -> Palette {
        Palette::from_colors(self.foreground, self.background, self.cursor, self.named)
    }
}

/// Six hex digits as a color.
fn hex(text: &str) -> Option<Rgb> {
    if text.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(text, 16).ok()?;
    Some(Rgb::new((value >> 16) as u8, (value >> 8) as u8, value as u8))
}

/// One line of the data file: `name|foreground|background|cursor|selection background|selection
/// foreground|` and the sixteen named colors. The selection foreground may be empty and is not used.
pub fn parse_line(line: &str) -> Option<Theme> {
    let fields: Vec<&str> = line.split('|').collect();
    if fields.len() != 22 || fields[0].is_empty() {
        return None;
    }
    let mut named = [Rgb::default(); 16];
    for (slot, text) in named.iter_mut().zip(&fields[6..]) {
        *slot = hex(text)?;
    }
    Some(Theme {
        name: fields[0].to_owned(),
        foreground: hex(fields[1])?,
        background: hex(fields[2])?,
        cursor: hex(fields[3])?,
        selection_background: hex(fields[4])?,
        named,
    })
}

/// Every theme, in the order the data file has them, which is by name. Read once.
pub fn all() -> &'static [Theme] {
    static THEMES: OnceLock<Vec<Theme>> = OnceLock::new();
    THEMES.get_or_init(|| THEME_DATA.lines().filter_map(parse_line).collect())
}

/// The theme with this name, ignoring case and the spaces around it.
pub fn named(name: &str) -> Option<&'static Theme> {
    let wanted = name.trim().to_lowercase();
    all().iter().find(|theme| theme.name.to_lowercase() == wanted)
}

/// Every theme name in the order the picker lists them: the ones Unluminous ships, then the rest by name.
pub fn names_in_picker_order() -> Vec<&'static str> {
    let featured = FEATURED.iter().filter_map(|name| named(name)).map(|theme| theme.name.as_str());
    let rest = all()
        .iter()
        .map(|theme| theme.name.as_str())
        .filter(|name| !FEATURED.iter().any(|featured| featured.eq_ignore_ascii_case(name)));
    featured.chain(rest).collect()
}

/// The theme names that contain `filter`, ignoring case, in picker order. An empty filter is every name.
pub fn matching(filter: &str) -> Vec<&'static str> {
    let wanted = filter.trim().to_lowercase();
    names_in_picker_order()
        .into_iter()
        .filter(|name| wanted.is_empty() || name.to_lowercase().contains(&wanted))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_of_the_data_file_is_a_theme() {
        let lines = THEME_DATA.lines().filter(|line| !line.trim().is_empty()).count();
        assert_eq!(all().len(), lines, "a line that did not parse");
        assert_eq!(all().len(), 894, "the count Termoil's README states");
    }

    #[test]
    fn the_featured_themes_exist_and_come_first() {
        let order = names_in_picker_order();
        for (at, name) in FEATURED.iter().enumerate() {
            assert!(named(name).is_some(), "{name} is not in the data file");
            assert_eq!(order[at], *name);
        }
        assert_eq!(order.len(), all().len(), "each theme once");
    }

    #[test]
    fn unluminous_dark_is_the_palette_the_terminal_already_had() {
        let theme = named("unluminous dark").expect("shipped");
        let palette = theme.palette();
        assert_eq!(palette.background, Palette::new().background);
        assert_eq!(palette.foreground, Palette::new().foreground);
        assert!(!palette.light);
        assert!(named("Unluminous Light").expect("shipped").palette().light);
    }

    #[test]
    fn a_theme_gives_its_sixteen_colors_to_the_palette() {
        let theme = named("Dracula").expect("in the iTerm2 set");
        let palette = theme.palette();
        for index in 0..16_u8 {
            assert_eq!(palette.indexed(index), theme.named[index as usize]);
        }
        assert_eq!(matching("dracula").first().copied(), Some("Islands Dracula Colorful"));
        assert!(matching("dracula").contains(&"Dracula"));
    }
}
