//! Which shortcuts are pinned to the rail and to the top bar. `task-2404`.
//!
//! A shortcut is one thing that can sit on either bar: a pane's button, a plugin tab's button, or one of the
//! three tools the top bar carries (Text Options, View Modes, Run Configurations). Each has a **home**, the bar
//! it is on when nothing has been changed, and four settings record the differences from that:
//!
//! - `rail.pinned` and `title_bar.pinned`, shortcuts put on a bar that is not their home;
//! - `rail.unpinned` and `title_bar.unpinned`, shortcuts taken off their home bar.
//!
//! Recording differences rather than the whole list is what lets a plugin switched on later get its button
//! on the rail with nothing written, and lets a window nobody customised write nothing at all.
//!
//! Nothing here knows about egui. The window asks [`is_pinned`] while it draws, and every way of changing a
//! pin, the right click menus, the gallery and `window pin`, goes through [`pin`] and [`unpin`].

use crate::settings::Settings;

/// One of the two bars a shortcut can sit on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bar {
    /// The column of round buttons down the left of the window, `components::activity_bar`.
    Rail,
    /// The title bar along the top, `components::title_bar`.
    Top,
}

impl Bar {
    pub const ALL: [Bar; 2] = [Bar::Rail, Bar::Top];

    /// The word the command line and the action names use: `rail` or `top`.
    pub fn name(self) -> &'static str {
        match self {
            Bar::Rail => "rail",
            Bar::Top => "top",
        }
    }

    /// Read a bar from the command line. `left`, `side` and `title` are taken too, because those are the words
    /// a person and a model use for the two bars.
    pub fn from_name(name: &str) -> Option<Bar> {
        match name.trim().to_ascii_lowercase().as_str() {
            "rail" | "left" | "side" | "sidebar" | "side-bar" => Some(Bar::Rail),
            "top" | "title" | "titlebar" | "title-bar" | "topbar" | "top-bar" => Some(Bar::Top),
            _ => None,
        }
    }

    /// What the bar is called where a person reads it: `Side Bar` or `Top Bar`.
    pub fn label(self) -> &'static str {
        match self {
            Bar::Rail => "Side Bar",
            Bar::Top => "Top Bar",
        }
    }

    /// The other bar.
    pub fn other(self) -> Bar {
        match self {
            Bar::Rail => Bar::Top,
            Bar::Top => Bar::Rail,
        }
    }
}

/// One of the three tools whose home is the top bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    /// The `F` chip and the panel of character and paragraph formatting behind it.
    TextOptions,
    /// The carved track of the three view modes: source, side by side, preview.
    ViewModes,
    /// The run widget: the configuration dropdown, Run, Debug, and Stop while something runs.
    RunConfigurations,
}

impl Tool {
    pub const ALL: [Tool; 3] = [Tool::TextOptions, Tool::ViewModes, Tool::RunConfigurations];

    /// The shortcut's name, which is what the settings hold and what the gallery's card says.
    pub fn name(self) -> &'static str {
        match self {
            Tool::TextOptions => "Text Options",
            Tool::ViewModes => "View Modes",
            Tool::RunConfigurations => "Run Configurations",
        }
    }

    /// The tool a shortcut name stands for, ignoring case.
    pub fn from_name(name: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|tool| tool.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// What kind of thing a shortcut opens, which decides which section of the gallery it is listed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A pane, a tile, the editing area, or a plugin's tab: pressing it shows or hides something.
    Panel,
    /// One of the top bar's tools, whose controls a flyout holds when it is on the rail.
    Tool(Tool),
}

/// One shortcut the window offers, as the gallery and `window shortcuts` list it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    /// Its name: the rail button's name for a panel, [`Tool::name`] for a tool.
    pub name: String,
    pub kind: Kind,
    /// The bar it is on when nothing has been changed.
    pub home: Bar,
}

/// The names in one of the four settings, in the order written, with blanks dropped.
pub fn names(setting: &str) -> Vec<String> {
    setting.split(',').map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).collect()
}

/// A setting's value written the way [`pin`] writes one: the names separated by a comma and a space, each once.
pub fn tidy(value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for name in names(value) {
        if !out.iter().any(|one| one.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
    }
    out.join(", ")
}

/// Whether `list` names `name`, ignoring case.
fn holds(list: &str, name: &str) -> bool {
    names(list).iter().any(|one| one.eq_ignore_ascii_case(name.trim()))
}

/// `list` with `name` taken out of it.
fn without(list: &str, name: &str) -> String {
    names(list)
        .into_iter()
        .filter(|one| !one.eq_ignore_ascii_case(name.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `list` with `name` added at the end, once.
fn with(list: &str, name: &str) -> String {
    tidy(&format!("{list}, {}", name.trim()))
}

/// The two settings that belong to a bar: what was pinned to it and what was taken off it.
fn settings_of(settings: &Settings, bar: Bar) -> (&String, &String) {
    match bar {
        Bar::Rail => (&settings.rail_pinned, &settings.rail_unpinned),
        Bar::Top => (&settings.title_bar_pinned, &settings.title_bar_unpinned),
    }
}

/// The same two, to be changed.
fn settings_of_mut(settings: &mut Settings, bar: Bar) -> (&mut String, &mut String) {
    match bar {
        Bar::Rail => (&mut settings.rail_pinned, &mut settings.rail_unpinned),
        Bar::Top => (&mut settings.title_bar_pinned, &mut settings.title_bar_unpinned),
    }
}

/// Whether the shortcut `name`, whose home is `home`, is on `bar`.
///
/// On its home bar unless it was unpinned there, and on the other bar only when it was pinned there.
/// @param settings - the window's settings, which hold the four lists
/// @param name - the shortcut's name
/// @param home - the bar it is on by default
/// @param bar - the bar being asked about
pub fn is_pinned(settings: &Settings, name: &str, home: Bar, bar: Bar) -> bool {
    let (pinned, unpinned) = settings_of(settings, bar);
    match bar == home {
        true => !holds(unpinned, name),
        false => holds(pinned, name),
    }
}

/// Put the shortcut on `bar`, and answer whether that changed anything.
/// @param settings - the window's settings
/// @param name - the shortcut's name
/// @param home - the bar it is on by default
/// @param bar - the bar to put it on
pub fn pin(settings: &mut Settings, name: &str, home: Bar, bar: Bar) -> bool {
    let was = is_pinned(settings, name, home, bar);
    let (pinned, unpinned) = settings_of_mut(settings, bar);
    *unpinned = without(unpinned, name);
    *pinned = match bar == home {
        true => without(pinned, name),
        false => with(pinned, name),
    };
    !was
}

/// Take the shortcut off `bar`, and answer whether that changed anything.
/// @param settings - the window's settings
/// @param name - the shortcut's name
/// @param home - the bar it is on by default
/// @param bar - the bar to take it off
pub fn unpin(settings: &mut Settings, name: &str, home: Bar, bar: Bar) -> bool {
    let was = is_pinned(settings, name, home, bar);
    let (pinned, unpinned) = settings_of_mut(settings, bar);
    *pinned = without(pinned, name);
    *unpinned = match bar == home {
        true => with(unpinned, name),
        false => without(unpinned, name),
    };
    was
}

/// Put a bar back the way Unluminous ships it: both of its settings emptied. Answers whether anything changed.
/// @param settings - the window's settings
/// @param bar - the bar to reset
pub fn reset(settings: &mut Settings, bar: Bar) -> bool {
    let (pinned, unpinned) = settings_of_mut(settings, bar);
    let changed = !pinned.is_empty() || !unpinned.is_empty();
    pinned.clear();
    unpinned.clear();
    changed
}

/// The top bar's shortcuts in the order they are drawn, left to right. `task-2405`.
///
/// `drawn` is what is on the bar in the order it has with nothing set: the text tools, the run widget, then the
/// pinned panes. The names `order` (the `title_bar.order` setting) gives come first, in its order; a shortcut it
/// does not name goes straight after the one before it in `drawn`, so a pane pinned later lands where it would
/// have, and a name nothing on the bar has is passed over.
/// @param order - the `title_bar.order` setting
/// @param drawn - the names on the bar, in the order they have with nothing set
pub fn arranged(order: &str, drawn: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in names(order) {
        let found = drawn.iter().find(|one| one.eq_ignore_ascii_case(&name));
        if let Some(found) = found.filter(|found| !out.contains(found)) {
            out.push(found.clone());
        }
    }
    for (index, name) in drawn.iter().enumerate() {
        if out.contains(name) {
            continue;
        }
        let after =
            drawn[..index].iter().rev().find_map(|before| out.iter().position(|one| one == before));
        out.insert(after.map_or(0, |at| at + 1), name.clone());
    }
    out
}

/// The `title_bar.order` setting once `name` is put down at `place` among `drawn`, the names on the bar from left
/// to right. `place` counts the names other than `name` that stay to its left. `task-2405`.
/// @param drawn - the names on the bar from left to right, `name` among them or not
/// @param name - the shortcut put down
/// @param place - how many of the others are to its left
pub fn put_down(drawn: &[String], name: &str, place: usize) -> String {
    let mut others: Vec<String> =
        drawn.iter().filter(|one| !one.eq_ignore_ascii_case(name)).cloned().collect();
    others.insert(place.min(others.len()), name.to_owned());
    others.join(", ")
}

/// A shortcut's name as an action name writes it: lower case, with a hyphen for each space.
pub fn slug(name: &str) -> String {
    name.trim().to_ascii_lowercase().split_whitespace().collect::<Vec<_>>().join("-")
}

/// The shortcut in `shortcuts` that `name` names, ignoring case and the space round it. A name written as its
/// [`slug`], which is how an action name carries it, finds it too.
pub fn find<'a>(shortcuts: &'a [Shortcut], name: &str) -> Option<&'a Shortcut> {
    shortcuts
        .iter()
        .find(|one| one.name.eq_ignore_ascii_case(name.trim()))
        .or_else(|| shortcuts.iter().find(|one| slug(&one.name) == slug(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_nothing_set_every_shortcut_is_on_its_home_bar_and_only_there() {
        let settings = Settings::new();
        assert!(is_pinned(&settings, "Project", Bar::Rail, Bar::Rail));
        assert!(!is_pinned(&settings, "Project", Bar::Rail, Bar::Top));
        assert!(is_pinned(&settings, "Text Options", Bar::Top, Bar::Top));
        assert!(!is_pinned(&settings, "Text Options", Bar::Top, Bar::Rail));
    }

    #[test]
    fn unpinning_a_rail_button_writes_rail_unpinned_and_pinning_it_again_clears_it() {
        let mut settings = Settings::new();
        assert!(unpin(&mut settings, "Database pane", Bar::Rail, Bar::Rail));
        assert_eq!(settings.rail_unpinned, "Database pane");
        assert!(
            !is_pinned(&settings, "database pane", Bar::Rail, Bar::Rail),
            "case does not matter"
        );
        assert!(
            !unpin(&mut settings, "Database pane", Bar::Rail, Bar::Rail),
            "twice changes nothing"
        );
        assert_eq!(settings.rail_unpinned, "Database pane");
        assert!(pin(&mut settings, "Database pane", Bar::Rail, Bar::Rail));
        assert_eq!(settings.rail_unpinned, "");
        assert_eq!(settings.rail_pinned, "", "a home bar never needs a pinned entry");
    }

    #[test]
    fn a_tool_pinned_to_the_rail_stays_on_the_top_bar_until_it_is_unpinned_there() {
        let mut settings = Settings::new();
        assert!(pin(&mut settings, "Run Configurations", Bar::Top, Bar::Rail));
        assert_eq!(settings.rail_pinned, "Run Configurations");
        assert!(is_pinned(&settings, "Run Configurations", Bar::Top, Bar::Rail));
        assert!(is_pinned(&settings, "Run Configurations", Bar::Top, Bar::Top));
        assert!(unpin(&mut settings, "Run Configurations", Bar::Top, Bar::Top));
        assert_eq!(settings.title_bar_unpinned, "Run Configurations");
        assert!(unpin(&mut settings, "Run Configurations", Bar::Top, Bar::Rail));
        assert_eq!(settings.rail_pinned, "");
        assert!(!is_pinned(&settings, "Run Configurations", Bar::Top, Bar::Rail));
    }

    #[test]
    fn several_names_are_kept_in_the_order_they_were_pinned_and_each_once() {
        let mut settings = Settings::new();
        pin(&mut settings, "Text Options", Bar::Top, Bar::Rail);
        pin(&mut settings, "Run Configurations", Bar::Top, Bar::Rail);
        pin(&mut settings, "text options", Bar::Top, Bar::Rail);
        assert_eq!(settings.rail_pinned, "Text Options, Run Configurations");
        assert_eq!(tidy(" a,, b , A ,c"), "a, b, c");
    }

    #[test]
    fn reset_puts_one_bar_back_and_leaves_the_other() {
        let mut settings = Settings::new();
        unpin(&mut settings, "Project", Bar::Rail, Bar::Rail);
        pin(&mut settings, "Project", Bar::Rail, Bar::Top);
        assert!(reset(&mut settings, Bar::Rail));
        assert!(is_pinned(&settings, "Project", Bar::Rail, Bar::Rail));
        assert!(is_pinned(&settings, "Project", Bar::Rail, Bar::Top), "the top bar kept its pin");
        assert!(!reset(&mut settings, Bar::Rail), "a second reset changes nothing");
    }

    /// `task-2405`: the Database pane put between View Modes and Run Configurations stays there, and a pane pinned
    /// later goes where it would with nothing set.
    #[test]
    fn the_top_bar_keeps_the_order_it_was_dragged_into() {
        let drawn: Vec<String> =
            ["Text Options", "View Modes", "Run Configurations", "Database pane"]
                .map(str::to_owned)
                .to_vec();
        assert_eq!(arranged("", &drawn), drawn, "nothing set is the shipped order");
        let order = put_down(&drawn, "Database pane", 2);
        assert_eq!(order, "Text Options, View Modes, Database pane, Run Configurations");
        assert_eq!(
            arranged(&order, &drawn),
            ["Text Options", "View Modes", "Database pane", "Run Configurations"]
        );
        let mut more = drawn.clone();
        more.push("Realm".to_owned());
        assert_eq!(
            arranged(&order, &more),
            ["Text Options", "View Modes", "Database pane", "Realm", "Run Configurations"],
            "a shortcut the order does not name goes after the one before it with nothing set"
        );
        let without: Vec<String> =
            drawn.iter().filter(|one| *one != "View Modes").cloned().collect();
        assert_eq!(
            arranged(&order, &without),
            ["Text Options", "Database pane", "Run Configurations"],
            "a name not on the bar now is passed over"
        );
        assert_eq!(
            put_down(&drawn, "Text Options", 99),
            "View Modes, Run Configurations, Database pane, Text Options"
        );
    }

    #[test]
    fn a_bar_is_read_from_the_words_a_person_uses_for_it() {
        assert_eq!(Bar::from_name("left"), Some(Bar::Rail));
        assert_eq!(Bar::from_name("Top"), Some(Bar::Top));
        assert_eq!(Bar::from_name("title-bar"), Some(Bar::Top));
        assert_eq!(Bar::from_name("bottom"), None);
        assert_eq!(Tool::from_name("view modes"), Some(Tool::ViewModes));
    }
}
