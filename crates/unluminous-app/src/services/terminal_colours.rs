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

thread_local! {
    /// The Termoil theme the person chose for the terminals, from the `terminal.theme` setting (`task-2597`).
    /// Empty follows the window's own theme. Per thread for the reason [`Palette::current`] is.
    static CHOSEN: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Hand this thread's terminals the palette of the active theme, or of the Termoil theme chosen for them.
pub fn follow_the_theme() {
    Palette::set_current(for_the_active_theme());
}

/// Choose the Termoil theme the terminals are drawn in, by name, and move every terminal onto it. An empty
/// name, or one that is not a theme, follows the window's own theme again. Answers whether the name was a
/// theme.
pub fn choose(name: &str) -> bool {
    let known = unluminous_terminal::themes::named(name).is_some();
    CHOSEN.with(|chosen| {
        *chosen.borrow_mut() = if known { name.trim().to_owned() } else { String::new() }
    });
    follow_the_theme();
    known
}

/// Draw the terminals in a theme for now without choosing it, which is what the picker does as its list is
/// moved through. [`follow_the_theme`] puts back the one chosen.
pub fn preview(name: &str) {
    if let Some(found) = unluminous_terminal::themes::named(name) {
        Palette::set_current(found.palette());
    }
}

/// The Termoil theme the terminals are drawn in, or empty while they follow the window's theme.
pub fn chosen() -> String {
    CHOSEN.with(|chosen| chosen.borrow().clone())
}

/// The palette a terminal is drawn in: the chosen Termoil theme's, or the active window theme's.
pub fn for_the_active_theme() -> Palette {
    match CHOSEN.with(|chosen| unluminous_terminal::themes::named(&chosen.borrow())) {
        Some(found) => found.palette(),
        None => for_the_window(),
    }
}

/// The palette the window's own theme gives a terminal, whatever Termoil theme is chosen.
pub fn for_the_window() -> Palette {
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

    /// `task-2597`: a Termoil theme chosen for the terminals wins over the window's, and an empty choice
    /// gives the window's back.
    #[test]
    fn a_chosen_termoil_theme_colors_the_terminals_and_empty_follows_the_window() {
        theme::activate(theme::Theme::unluminous_dark());
        assert!(choose("Dracula"));
        let dracula = unluminous_terminal::themes::named("Dracula").expect("bundled");
        assert_eq!(Palette::current().background, dracula.background);
        assert_eq!(chosen(), "Dracula");
        preview("Unluminous Light");
        assert!(Palette::current().light, "a preview draws in the theme without choosing it");
        follow_the_theme();
        assert_eq!(Palette::current().background, dracula.background, "and is put back");
        assert!(!choose("not a theme"));
        assert_eq!(Palette::current(), Palette::new(), "the window's own palette again");
        assert!(!choose(""));
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
