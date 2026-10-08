//! Which colours a terminal is drawn in, worked out from the window's theme.
//!
//! `unluminous-terminal` holds no user interface dependency and knows nothing about themes, so it is handed a
//! [`Palette`] and keeps it per thread. This is the one place the window works that palette out, called from
//! `UnluminousApp::prepare` and from `apply_the_theme`, the two places a theme becomes a change. Every
//! session moves onto it the next time it is drawn. `task-2215`.
//!
//! The sixteen named colours come from the darkness of the theme, because a set chosen to be read on a dark
//! ground is barely there on a light one. The ground, the ordinary text and the cursor come from the theme
//! itself, so a terminal belongs to the window it is in: Unluminous Dark's three are exactly the numbers the
//! terminal always had, and a theme plugin's terminal now has that theme's editor behind it.

use unluminous_terminal::{Palette, Rgb};

use crate::theme::{self, color};

/// Hand this thread's terminals the palette of the active theme.
pub fn follow_the_theme() {
    Palette::set_current(for_the_active_theme());
}

/// The palette a terminal is drawn in under the active theme.
pub fn for_the_active_theme() -> Palette {
    let mut palette = match theme::is_dark() {
        true => Palette::new(),
        false => Palette::light(),
    };
    let rgb = |colour: egui::Color32| Rgb::new(colour.r(), colour.g(), colour.b());
    palette.background = rgb(color::editor());
    palette.foreground = rgb(color::text());
    palette.cursor = rgb(color::accent());
    palette
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dark terminal is the one that shipped, number for number, so no screenshot of a terminal moved.
    #[test]
    fn unluminous_dark_gives_the_terminal_it_always_had() {
        theme::activate(theme::Theme::unluminous_dark());
        let palette = for_the_active_theme();
        assert_eq!(palette, Palette::new());
    }

    #[test]
    fn a_light_theme_gives_a_light_terminal() {
        theme::activate(theme::Theme::unluminous_light());
        let palette = for_the_active_theme();
        assert!(palette.light);
        assert_eq!(palette.background, Rgb::new(0xFB, 0xFC, 0xFD), "the light editor's ground");
        let black = palette.indexed(0);
        assert!(black.r < 0x40, "black is the dark ink a light terminal writes in");
        theme::activate(theme::Theme::unluminous_dark());
    }
}
