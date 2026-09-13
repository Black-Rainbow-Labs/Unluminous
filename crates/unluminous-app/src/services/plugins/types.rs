//! The data model: what a plugin is, what it can add to the window, and the value everything
//! that draws reads once the manifests have been read.
//!
//! Nothing here reads a file or a manifest. [`Plugin`] and the contribution types are plain data,
//! built by `manifest::parse` and held by `store::Plugins`; this file only says what the shapes
//! are and the small questions they can answer about themselves.

use std::path::Path;

use unluminous_core::syntax::{Grammar, Token};
use unluminous_core::Color;

/// The kind of plugin.
///
/// Three. The second is what `tasks/ui-plugin-architecture.md` widened the seam for and the third is
/// `task-1776`. A fourth is still refused rather than half-loaded, which is what the field was added for —
/// and each widening is done in the open, with a check and a test, because the whole value of the field is
/// that a manifest asking for something this version cannot do says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A description of a language: extensions, a grammar, an icon and a colour scheme.
    Language,
    /// A plugin that draws: it contributes a rail button and a pane, a tab in the editing area, a
    /// menu, and a page in Settings. The arrangement is data in the manifest and the drawing is code
    /// in Unluminous, named by `ui.provider`.
    Ui,
    /// A set of themes: what every name in `theme::color` means, what the tokens are coloured, and which
    /// of the drawn icon sets is used. It claims no file type and contributes no pane; it says what the
    /// window it is already in looks like.
    Theme,
}

impl Kind {
    /// The word the manifest, the settings file and the command line call it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Language => "language",
            Kind::Ui => "ui",
            Kind::Theme => "theme",
        }
    }
}

/// A colour scheme: one colour per kind of token.
///
/// **It colours the tokens and not the background.** Dracula's own `#282A36` is not used, and
/// Unluminous's `theme::color::editor()` stays, because the window letting the desktop show through is the
/// whole character of the product and a scheme that repaints the editing area opaque would take that
/// away in exchange for being a shade nearer a screenshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxTheme {
    pub name: String,
    /// One colour a token, in the order of [`Token::ALL`].
    colours: Vec<(Token, Color)>,
}

impl SyntaxTheme {
    /// A scheme built from a list, which is what a theme's `syntax.*` keys produce.
    pub fn of(name: impl Into<String>, colours: Vec<(Token, Color)>) -> SyntaxTheme {
        SyntaxTheme { name: name.into(), colours }
    }

    pub fn colour(&self, token: Token) -> Option<Color> {
        self.colours.iter().find(|(known, _)| *known == token).map(|(_, colour)| *colour)
    }

    pub fn is_empty(&self) -> bool {
        self.colours.is_empty()
    }
}

/// Which group of the rail a pane's button goes in.
///
/// The rail's two groups say what a panel **is** rather than where it happens to be: the top group
/// holds lists and the bottom holds tiles with a character grid in them. That is the distinction
/// `components::activity_bar` already draws, and a contributed pane joins one of the two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailGroup {
    Top,
    Bottom,
}

impl RailGroup {
    pub fn name(self) -> &'static str {
        match self {
            RailGroup::Top => "top",
            RailGroup::Bottom => "bottom",
        }
    }
}

/// A pane a plugin contributes, and the button in the rail that shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneContribution {
    /// The name the command line and the settings file call it. Lower case, one word.
    pub id: String,
    /// What a person reads in the rail's tooltip and on the pane's own header.
    pub label: String,
    /// Which drawn icon goes in the rail, from [`PANE_ICONS`].
    pub icon: String,
    pub group: RailGroup,
    /// The side it docks to the first time it is shown.
    pub side: crate::app::dock::Side,
    /// The two measurements every panel carries, because one number cannot be both: a width for when
    /// it is a column at the side, and a height for when it is in a strip.
    pub width: f32,
    pub height: f32,
    /// The condition under which the button is drawn at all, from [`PANE_CONDITIONS`].
    pub applies: String,
}

/// A tab in the editing area a plugin contributes.
///
/// It has no path on disk, is never modified and cannot be saved, which are the four answers a picture
/// tab already gives to the four questions the window asks a tab.
#[derive(Debug, Clone, PartialEq)]
pub struct TabContribution {
    pub id: String,
    pub label: String,
    /// Which drawn icon its button in the rail uses, from `activity_bar::pane_icon`'s list.
    pub icon: String,
}

/// One row of a plugin's menu: something to do, a separator, or a menu inside a menu.
///
/// Recursive, because `menu.submenu.<id>.submenu.<other>` is a submenu inside a submenu and
/// `actions::Entry::Submenu` already holds a `Vec<Entry>`.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuItem {
    /// `command=Name`: the name a person reads, and the command handed to the provider.
    Command {
        command: String,
        label: String,
    },
    /// A lone `-` in the list.
    Separator,
    Submenu {
        label: String,
        items: Vec<MenuItem>,
    },
}

/// A menu a plugin contributes, added after the six Unluminous has.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuContribution {
    pub name: String,
    pub items: Vec<MenuItem>,
}

/// A page in the Settings window a plugin contributes.
#[derive(Debug, Clone, PartialEq)]
pub struct PageContribution {
    pub name: String,
    pub icon: String,
}

/// Everything one plugin adds to the window.
///
/// A value on the plugin rather than four questions asked of it, so the rail, the dock, the menus, the
/// tab strip and the Settings window all read one thing and none of them can disagree with the others
/// about what was contributed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Contributions {
    /// The name in [`UI_PROVIDERS`] of the code that fills the pane, the tab and the page.
    pub provider: Option<String>,
    /// The name in [`CHROME`] of the renderer this plugin's decoration is drawn with, when it asks for one.
    pub chrome: Option<String>,
    pub pane: Option<PaneContribution>,
    pub tab: Option<TabContribution>,
    pub menu: Option<MenuContribution>,
    pub page: Option<PageContribution>,
}

impl Contributions {
    /// True when this manifest adds nothing at all, which is what makes a `ui` plugin unreachable and
    /// is therefore refused.
    pub fn is_empty(&self) -> bool {
        self.pane.is_none() && self.tab.is_none() && self.menu.is_none() && self.page.is_none()
    }
}

/// One contribution, with the plugin it came from.
///
/// A pane, a tab and a page are all reached by `<plugin id>/<contribution id>` rather than by an index,
/// because the set is decided when the manifests are read rather than at compile time. That is the one
/// property `dock::Panel`'s four variants could not have.
#[derive(Debug, Clone, PartialEq)]
pub struct Surface<T> {
    /// The plugin's `plugin.id`.
    pub plugin: String,
    /// The name in [`UI_PROVIDERS`] of the code that fills it.
    pub provider: String,
    pub what: T,
}

impl<T> Surface<T> {
    /// The name the settings file, the dock and the command line call this contribution.
    pub fn key(&self, id: &str) -> String {
        format!("{}/{id}", self.plugin)
    }
}

/// Everything every enabled plugin contributes, worked out once when the plugins are loaded.
///
/// One value rather than a question asked of each plugin every frame: the rail, the dock, the menus,
/// the tab strip and the Settings window all read it, so none of them can disagree with the others
/// about what is contributed. Rebuilt by [`Plugins::set_enabled`], which is what makes switching a
/// plugin off withdraw every contribution in the same frame — the rule `Plugins::renders` already
/// keeps for a Mermaid diagram.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Surfaces {
    pub panes: Vec<Surface<PaneContribution>>,
    pub tabs: Vec<Surface<TabContribution>>,
    pub menus: Vec<Surface<MenuContribution>>,
    pub pages: Vec<Surface<PageContribution>>,
    /// The renderer each plugin asked for, as `(plugin.id, the name from `CHROME`)`.
    ///
    /// **The name is kept, not thrown away for a boolean.** There is one renderer today, so a `Vec<String>`
    /// of ids would have worked and would have been a lie the moment a second name were added to
    /// [`CHROME`]: every plugin would still have gone to `vello_canvas`, and the check would have been a
    /// yes-or-no dressed up as a choice. `Surfaces::chrome_for` answers with the name, and the window
    /// matches on it.
    ///
    /// Here rather than asked of the manifest at drawing time, for the reason the four lists above are
    /// here: switching a plugin off has to withdraw its decoration in the same frame it withdraws its
    /// pane, and one value everything reads is what makes that impossible to get wrong.
    pub chrome: Vec<(String, String)>,
}

impl Surfaces {
    /// The pane named `<plugin>/<pane>`.
    pub fn pane(&self, key: &str) -> Option<&Surface<PaneContribution>> {
        self.panes.iter().find(|surface| surface.key(&surface.what.id) == key)
    }

    pub fn tab(&self, key: &str) -> Option<&Surface<TabContribution>> {
        self.tabs.iter().find(|surface| surface.key(&surface.what.id) == key)
    }

    pub fn is_empty(&self) -> bool {
        self.panes.is_empty()
            && self.tabs.is_empty()
            && self.menus.is_empty()
            && self.pages.is_empty()
    }

    /// Which renderer this plugin asked for, if it asked and is switched on.
    pub fn chrome_for(&self, plugin: &str) -> Option<&str> {
        self.chrome.iter().find(|(id, _)| id == plugin).map(|(_, renderer)| renderer.as_str())
    }

    /// The provider named by the plugin with this id, from whichever of its contributions names it.
    ///
    /// Every contribution of one plugin carries the same provider, so any of them answers; asking the
    /// panes first is only because most plugins contribute one.
    pub fn provider_of(&self, plugin: &str) -> Option<String> {
        self.panes
            .iter()
            .map(|surface| (&surface.plugin, &surface.provider))
            .chain(self.tabs.iter().map(|surface| (&surface.plugin, &surface.provider)))
            .chain(self.menus.iter().map(|surface| (&surface.plugin, &surface.provider)))
            .chain(self.pages.iter().map(|surface| (&surface.plugin, &surface.provider)))
            .find(|(id, _)| id.as_str() == plugin)
            .map(|(_, provider)| provider.clone())
    }

    /// Every plugin that contributes anything, once each, in the order the plugins are listed.
    pub fn plugins(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        for id in self
            .panes
            .iter()
            .map(|surface| &surface.plugin)
            .chain(self.tabs.iter().map(|surface| &surface.plugin))
            .chain(self.menus.iter().map(|surface| &surface.plugin))
            .chain(self.pages.iter().map(|surface| &surface.plugin))
        {
            if !found.contains(id) {
                found.push(id.clone());
            }
        }
        found
    }
}

/// One plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub vendor: String,
    pub description: String,
    /// What it does not do, which every one of these has and which is worth reading before wondering
    /// why a regular expression is coloured as division.
    pub limitations: String,
    pub kind: Kind,
    /// What this plugin adds to the window. Empty for every `language` plugin.
    pub contributions: Contributions,
    /// The extensions it claims, without the dot, in lower case.
    pub extensions: Vec<String>,
    /// The built-in renderer this language's files are drawn with, if it has one.
    pub renders: Option<String>,
    /// How one file of this language is run, with `{file}` standing for the path — `node {file}`.
    ///
    /// What puts `Run Current File` on the run widget's flyout and the `Run` menu for a file of
    /// this language, and nothing else. Off unless a manifest asks for it, which is the rule every
    /// key added since `task-1671` has followed.
    pub run_file: Option<String>,
    /// The project detector this language's projects are found by, named from [`PROJECT_RUNNERS`].
    pub run_project: Option<String>,
    /// The debugger this language's files are debugged with, named from [`DEBUGGERS`].
    ///
    /// What puts the whole debug half of the Run menu, the gutter's breakpoints and the debug tile
    /// in front of a file of this language, and nothing else. **Absent** rather than dimmed for a
    /// language that names none, which is the rule the three code-navigation entries already follow:
    /// a stylesheet has nothing to step through and never will.
    pub debug_adapter: Option<String>,
    pub grammar: Grammar,
    pub theme: SyntaxTheme,
    /// The themes this plugin carries. Empty for every `language` and `ui` plugin.
    ///
    /// A `Vec` because "a themes bundle" is several: one plugin is switched on or off, and five palettes
    /// arrive or leave with it. The order is the manifest's `themes` line rather than alphabetical, so the
    /// list in Settings is the order somebody arranged.
    pub themes: Vec<crate::theme::Theme>,
    /// The bytes of `icon.png`, when it has one.
    pub icon: Option<Vec<u8>>,
    /// True when it came from inside the binary rather than from disk.
    pub bundled: bool,
    /// True when it is switched on.
    pub enabled: bool,
}

impl Plugin {
    /// Whether this plugin claims `path`.
    pub fn claims(&self, path: &Path) -> bool {
        let Some(extension) = path.extension().and_then(|name| name.to_str()) else {
            return false;
        };
        let extension = extension.to_lowercase();
        self.extensions.contains(&extension)
    }
}
