//! The thin rail down the far left of the window.
//!
//! `task-1658` asks for what the reference editor has: a narrow strip holding one button for each pane, so a pane can
//! be put away and brought back without going to a menu, and the buttons stay where they are whether the
//! pane is showing or not. Unluminous's is narrower than the reference editor's — 36 points against about 40 — because it
//! holds three buttons rather than a dozen and does not need the room.
//!
//! Two groups. The panes that live at the side are at the top, in the order they are in the window: the
//! explorer, then git. The terminal lives along the bottom of the window, so its button is at the bottom
//! of the rail, which is where `task-1658`'s reference capture puts it.
//!
//! `task-1683` added a second button to the bottom group, `Run tile`, **above** `Terminal tile`, and
//! `task-1687` a third, `Debug tile`, above that. The bottom of the window shows **one** of the three
//! and never two stacked, so pressing any of them shows its own tile and puts the others away —
//! which the window settles, not this file.
//!
//! Since `task-1697` a panel is not always where its button says it is: the terminal can be dragged
//! to the right and the explorer to the bottom. **The rail does not follow it**, and that is a
//! decision rather than an oversight — the reference editor moves a tool window's stripe button to the side the
//! window is on, and Unluminous has one rail on one edge, so its two groups say what a panel *is* (a
//! list, or a tile with a grid in it) rather than where it happens to be. A rail that reshuffled
//! itself on every drag would be a second thing moving while somebody was moving the first.
//!
//! What the rail did gain is a **right click**: it opens the panel's own menu, which is the only way
//! to move a panel that has been put away, since a panel that is not showing has no header to grab.
//!
//! A button that is on is a soft wash of the accent with its mark in the accent and a short accent bar
//! against the rail's left edge (`task-2219`). It used to be the pill every list draws for its chosen
//! row, at the full size of the button with the mark in near black, and three of those down a rail that
//! is nearly always in that state read as a highlighter. A filled `ACCENT` square would be louder still;
//! the pane being open is a state rather than a press.
//!
//! The rail is also the only way back once a pane has been put away. There used to be a small
//! disclosure button floating over the top left of the editing area for exactly that, and it is gone:
//! the rail does the job, always in the same place whether the explorer is showing or not, which is the
//! point of having one.
//!
//! Nothing here changes anything. Each button reports the `Action` it stands for, so the rail, the menus
//! and the keyboard all go down the one path in `UnluminousApp::run_action`.

use egui::{CornerRadius, Pos2, Rect, Sense, Vec2};

use crate::app::actions::{Action, GitAction};
use crate::components::controls::WithHint as _;
use crate::theme::{color, icon, size, svg_icon};

/// How big one button in the rail is, and how far apart two of them are: round buttons 34 points across
/// with four between them, which is the Editing code design (`task-2241`).
const BUTTON: f32 = 34.0;
const STEP: f32 = 38.0;
/// Space between the top or bottom edge of the rail and the first button.
const MARGIN: f32 = 8.0;
/// How large a mark is drawn in a button: `width="19"` on a 24 unit view box.
const MARK: f32 = 19.0;

/// The Debug button's mark, the codicon `debug-continue`, on the rail and in the run widget. `task-2280`.
const DEBUG_MARK: IconDrawer = IconDrawer::Svg(&svg_icon::DEBUG_CONTINUE, 17.0);

/// The icon a rail button draws: one of the design's line marks, or a drawing of Unluminous's own for a plugin
/// that names a mark the design has no line version of.
#[derive(Clone, Copy)]
enum IconDrawer {
    Line(rux::Icon),
    Drawn(fn(&egui::Painter, Pos2, egui::Color32)),
    /// A mark taken from another icon set as SVG path data, and the points it is drawn across. `task-2280`.
    Svg(&'static crate::theme::svg_icon::SvgMark, f32),
}

/// One row of the panes that live at the side of the rail: its name, its icon, whether it is
/// showing, whether it can be pressed, the action pressing it performs, and the panel its right
/// click menu is about. The panel is `None` for a button with nowhere to be moved to, such as the
/// editing area.
type SideButtonRow =
    (&'static str, IconDrawer, bool, bool, Action, Option<crate::app::dock::Panel>);

/// One row of the tiles along the bottom of the rail: its name, its icon, whether it is showing,
/// the action pressing it performs, and the panel its right click menu is about.
type BottomButtonRow = (&'static str, IconDrawer, bool, Action, crate::app::dock::Panel);

/// What the rail needs to know to draw itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RailState {
    pub explorer_visible: bool,
    /// Whether the editing area — the pane holding the tabs — is showing. `task-28` asks for its button under
    /// the folder one, so it is the second in the top group.
    pub editor_visible: bool,
    /// True when the commit panel is open.
    pub git_open: bool,
    /// False outside a repository, which dims the git button rather than removing it — the same rule
    /// the Git menu already follows.
    pub in_repository: bool,
    pub terminal_visible: bool,
    /// True when the Realm is showing - `task-1904`.
    pub realm_visible: bool,
    /// Whether the Realm plugin is switched on. Off, the button is absent. `task-2202`.
    pub realm_available: bool,
    /// True when the run tile is the one showing at the bottom of the window.
    pub run_visible: bool,
    /// True when the debug tile is the one showing along the bottom.
    pub debug_visible: bool,
}

/// A button the rail draws for a pane a plugin contributed.
///
/// The rail's own two groups say what a panel **is** — a list at the top, a character grid at the
/// bottom — and a contributed button joins the group its manifest named. It is drawn after Unluminous's own,
/// so installing a plugin never moves a button somebody's hand already knows.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginButton {
    /// What the tooltip says, from `pane.label`.
    pub label: String,
    /// Which drawn icon, from `pane.icon`.
    pub icon: String,
    /// True when the pane is showing.
    pub on: bool,
    /// Which of the rail's two groups it is in.
    pub bottom: bool,
    /// The pane's `<plugin id>/<pane id>`, or a tab's own key, which is what the action carries.
    pub key: String,
    /// Which dock slot it is, so a right click opens the right panel's menu.
    pub slot: usize,
    /// What pressing it opens.
    pub opens: Opens,
}

/// Whether a plugin's rail button opens a docked pane or a tab in the editing area.
///
/// **A tab needs a button too**, which it did not have: the rail was built from the panes alone, so
/// Agent-Tasks — which contributes a tab rather than a pane, because a board needs the whole editing
/// area and not a 420 point column — could only be reached from the `Plugins` menu. `task-28` asks for
/// it on the rail under the chat icon, which is where a `top` group button lands.
///
/// A tab has no side and no dock slot, so a right click on one opens **no** menu: there is nothing to
/// move it to, which is the same reason the `Version Control` and `Editing Area` buttons open none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opens {
    Pane,
    Tab,
}

/// What the rail reported this frame.
///
/// Two things rather than one since `task-1697`: a **left** click is still the action the button
/// stands for, and a **right** click opens that panel's own menu — the four `Move to` rows and
/// `Reset Panel Layout`. The rail is a second place to reach a panel's menu because it is the one
/// place a panel can be reached when it is put away, and because a header that is being used as a
/// handle is a header somebody is already pointing at.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RailOutcome {
    pub chosen: Option<Action>,
    /// A button was right clicked: where the pointer was, and which panel it is about. The git
    /// button opens no menu, because the commit panel is a modal rather than a docked panel.
    pub menu: Option<(Pos2, crate::app::dock::Panel)>,
}

/// Draw the rail into `area`, and report what was pressed.
pub fn show(ui: &mut egui::Ui, area: Rect, state: RailState, opacity: f32) -> RailOutcome {
    show_with(ui, area, state, opacity, &[])
}

/// The same, with the buttons the plugins that are switched on contributed.
pub fn show_with(
    ui: &mut egui::Ui,
    area: Rect,
    state: RailState,
    opacity: f32,
    plugins: &[PluginButton],
) -> RailOutcome {
    let painter = ui.painter_at(area);
    painter.rect_filled(
        area,
        CornerRadius::ZERO,
        crate::theme::pane_ground(color::title_bar(), opacity),
    );
    // The groove down the rail's right hand edge: a dark line and a lit one beside it.
    crate::components::chrome_kit::groove(
        &painter,
        Pos2::new(area.right() - 2.0, area.top()),
        Pos2::new(area.right() - 2.0, area.bottom()),
    );
    crate::components::chrome_kit::layer(ui, "rail", area, 0.0, |rux| {
        buttons(rux, area, state, plugins)
    })
}

/// Every button in the rail, drawn into one `rux` layer.
fn buttons(
    rux: &mut rux::Rux<'_>,
    area: Rect,
    state: RailState,
    plugins: &[PluginButton],
) -> RailOutcome {
    let mut outcome = RailOutcome::default();
    // Six points in from the left rather than centred in the rail, because the outer six belong to
    // `components::resize_edges` and a button that reached into them would lose those clicks to the
    // window's own resize grip. The six left at the right hand side make it look centred anyway.
    let centre_x = area.left() + crate::components::resize_edges::EDGE + BUTTON / 2.0;

    // The panes at the side, from the top.
    let top: [SideButtonRow; 4] = [
        (
            "Project",
            IconDrawer::Line(rux::Icon::Project),
            state.explorer_visible,
            true,
            Action::ToggleExplorer,
            Some(crate::app::dock::Panel::Explorer),
        ),
        (
            // **Under the folder icon**, which is where `task-28` asks for it. Named `Editing Area` rather
            // than `Editor` or `Files`: `Editor` is the Settings window's own page for the same reason
            // `Project` above is the explorer, and no two controls in one window may share a name.
            //
            // It is not a `Panel`, so a right click on it opens no menu: the editing area is what is left when
            // the panels have taken their room, and there is no edge to move it to.
            "Editing Area",
            IconDrawer::Line(rux::Icon::Document),
            state.editor_visible,
            true,
            Action::ToggleEditor,
            None,
        ),
        (
            // Not `Git`: the menu bar already has a `Git`, and no two controls in one window may share
            // a name — a test cannot tell them apart and neither can anybody reading them out. This
            // button opens the commit panel, which is the reference editor's Version Control tool window.
            "Version Control",
            IconDrawer::Line(rux::Icon::Git),
            state.git_open,
            state.in_repository,
            Action::Git(GitAction::Commit),
            // The commit panel is a modal rather than a docked panel, so it has no side to be moved
            // to and its button opens no menu.
            None,
        ),
        (
            // The Realm - `task-1904`. In the **top** group, because the rail's two
            // groups say what a panel *is*: the bottom one holds the things that are a character
            // grid, and a canvas is a surface with several of them inside it rather than one.
            "Realm",
            IconDrawer::Line(rux::Icon::Realm),
            state.realm_visible,
            true,
            Action::Realm(crate::app::actions::RealmAction::Toggle),
            Some(crate::app::dock::Panel::Realm),
        ),
    ];
    // The Realm's button is absent while its plugin is off, which is the rule a plugin's own rail button
    // keeps. `task-2202`.
    let top = top.into_iter().filter(|(name, ..)| *name != "Realm" || state.realm_available);
    for (index, (name, draw, on, enabled, action, panel)) in top.enumerate() {
        let centre = Pos2::new(centre_x, area.top() + MARGIN + BUTTON / 2.0 + index as f32 * STEP);
        let pressed = rail_button(rux, centre, name, draw, on, enabled);
        if pressed.clicked {
            outcome.chosen = Some(action);
        }
        if let (Some(at), Some(panel)) = (pressed.menu, panel) {
            outcome.menu = Some((at, panel));
        }
    }

    // The three tiles that live along the bottom of the window, so their buttons are at the bottom
    // of the rail, counted **up** from it so the last one is always in the corner however many there
    // are — which is what made adding the third one free. `Terminal tile` rather than `Terminal`,
    // because `Edit -> Settings` has a page called `Terminal` and the `View` menu has an entry
    // called `Terminal`, and no two controls in one window may share a name; `Run tile` and
    // `Debug tile` are named to match, and for the same reason — the `Run` menu is a control too.
    // **The terminal stays in the corner.** The list is read bottom upwards, so the first entry is
    // the one in the corner of the window — which `task-1658`'s capture put the terminal in and
    // which a dozen accepted screenshots are of. The design's §9 says the debug button goes "below
    // the run tile's", and that would have taken the corner; the corner is the older promise, so the
    // new button goes above the run one instead.
    let bottom: [BottomButtonRow; 3] = [
        (
            "Terminal tile",
            IconDrawer::Line(rux::Icon::Terminal),
            state.terminal_visible,
            Action::ToggleTerminal,
            crate::app::dock::Panel::Terminal,
        ),
        (
            "Run tile",
            IconDrawer::Line(rux::Icon::RunTile),
            state.run_visible,
            Action::ToggleRunTile,
            crate::app::dock::Panel::Run,
        ),
        (
            "Debug tile",
            DEBUG_MARK,
            state.debug_visible,
            Action::ToggleDebugTile,
            crate::app::dock::Panel::Debug,
        ),
    ];
    for (index, (name, draw, on, action, panel)) in bottom.into_iter().enumerate() {
        let centre =
            Pos2::new(centre_x, area.bottom() - MARGIN - BUTTON / 2.0 - index as f32 * STEP);
        let pressed = rail_button(rux, centre, name, draw, on, true);
        if pressed.clicked {
            outcome.chosen = Some(action);
        }
        if let Some(at) = pressed.menu {
            outcome.menu = Some((at, panel));
        }
    }

    // The plugins' buttons, after Unluminous's own in whichever group each manifest named. A button that
    // would not fit is not drawn: the rail is as tall as the window and a button half off the end reads
    // as a fault rather than as a full rail.
    let mut top_next = 4;
    let mut bottom_next = 3;
    for button in plugins {
        let centre = match button.bottom {
            true => {
                let at = Pos2::new(
                    centre_x,
                    area.bottom() - MARGIN - BUTTON / 2.0 - bottom_next as f32 * STEP,
                );
                bottom_next += 1;
                at
            }
            false => {
                let at = Pos2::new(
                    centre_x,
                    area.top() + MARGIN + BUTTON / 2.0 + top_next as f32 * STEP,
                );
                top_next += 1;
                at
            }
        };
        if centre.y - BUTTON / 2.0 < area.top() || centre.y + BUTTON / 2.0 > area.bottom() {
            continue;
        }
        let pressed =
            rail_button(rux, centre, &button.label, rail_mark(&button.icon), button.on, true);
        if pressed.clicked {
            outcome.chosen = Some(match button.opens {
                Opens::Pane => Action::PluginPane { pane: button.key.clone() },
                Opens::Tab => Action::PluginTab { tab: button.key.clone() },
            });
        }
        // A tab has no side to be moved to, so no menu. See [`Opens`].
        if let (Some(at), Opens::Pane) = (pressed.menu, button.opens) {
            outcome.menu = Some((at, crate::app::dock::Panel::Plugin(button.slot as u8)));
        }
    }

    outcome
}

/// The drawing behind a `pane.icon` name.
///
/// The names are `plugins::PANE_ICONS`, and a manifest naming one Unluminous cannot draw was refused when it
/// was read, so the fallback here is never reached by a manifest that loaded.
pub fn pane_icon(name: &str) -> fn(&egui::Painter, Pos2, egui::Color32) {
    match name {
        "board" => icon::board,
        "folder" => icon::folder,
        "terminal" => icon::terminal,
        "run" => icon::run,
        "bug" => icon::bug,
        "clock" => icon::clock,
        "branch" => icon::branch,
        "tick" => icon::tick,
        "plus" => icon::plus,
        "image" => icon::image,
        "chat" => icon::chat,
        "database" => icon::database,
        "table" => icon::table,
        "design" => icon::design,
        _ => icon::board,
    }
}

/// The mark a plugin's button draws: the design's line mark where it has one for the name, and Unluminous's own
/// drawing for the rest.
fn rail_mark(name: &str) -> IconDrawer {
    match name {
        "board" => IconDrawer::Svg(&svg_icon::LIST_TODO, MARK),
        "design" => IconDrawer::Svg(&svg_icon::PAINTBRUSH_VERTICAL, MARK),
        "folder" => IconDrawer::Line(rux::Icon::Project),
        "terminal" => IconDrawer::Line(rux::Icon::Terminal),
        "run" => IconDrawer::Line(rux::Icon::RunTile),
        "bug" => DEBUG_MARK,
        "branch" => IconDrawer::Line(rux::Icon::Git),
        "chat" => IconDrawer::Line(rux::Icon::Bubble),
        "database" => IconDrawer::Line(rux::Icon::Database),
        other => IconDrawer::Drawn(pane_icon(other)),
    }
}

/// What one button in the rail reported.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Pressed {
    clicked: bool,
    /// Where the pointer was when it was right clicked, which opens the panel's own menu.
    menu: Option<Pos2>,
}

/// One button in the rail: round, carved into the rail while what it opens is open, and a line mark.
///
/// `task-2241`, the Editing code design: `width: 34px; height: 34px; border-radius: 50%`, carved with
/// `--in-sm` and the mark in the strong ink while the pane is showing, no surface and the quiet ink while it
/// is not. The pointer turns the mark the sky colour.
fn rail_button(
    rux: &mut rux::Rux<'_>,
    centre: Pos2,
    name: &str,
    draw: IconDrawer,
    on: bool,
    enabled: bool,
) -> Pressed {
    let hit = Rect::from_center_size(centre, Vec2::splat(BUTTON));
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response =
        rux.ui.interact(hit, rux.ui.id().with(("activity", name)), sense).with_hint(name);
    let chat = crate::components::chrome_kit::chat();
    let ink = rux.theme().ink;
    if on {
        rux.chrome.surface(hit, BUTTON / 2.0, chat.well, chat.carved_sm());
    }
    let tint = if !enabled {
        ink.i400.gamma_multiply(0.45)
    } else if response.hovered() {
        chat.sky
    } else if on {
        ink.i900
    } else {
        ink.i400
    };
    match draw {
        IconDrawer::Line(mark) => rux.mark(rux::Mark::new(mark, MARK), centre, tint),
        IconDrawer::Drawn(paint) => icon::crisp(rux.ui.painter(), centre, tint, 1.2, paint),
        IconDrawer::Svg(mark, size) => svg_icon::paint(rux.ui.painter(), centre, size, mark, tint),
    }
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, name));
    Pressed {
        clicked: response.clicked(),
        menu: match response.secondary_clicked() {
            true => response.interact_pointer_pos().or_else(|| response.hover_pos()),
            false => None,
        },
    }
}

// The rail's own constants are checked against each other here rather than in a test, because
// every value involved is known at compile time: clippy is right that a runtime `assert!` on
// them can only ever pass or fail the same way, so a build fails before it ever draws a rail
// that could not fit its own buttons.
const _: () = assert!(size::ACTIVITY_BAR >= BUTTON + 4.0, "a button has to fit inside it");
// The button starts where the window's own left resize grip stops, so the two never overlap.
const _: () = assert!(
    crate::components::resize_edges::EDGE + BUTTON <= size::ACTIVITY_BAR,
    "the button has to fit clear of the resize grip"
);

#[cfg(test)]
mod icon_tests {
    use super::pane_icon;
    use crate::services::plugins::PANE_ICONS;
    use crate::theme::icon;

    #[test]
    fn every_named_icon_is_actually_drawn_rather_than_falling_back_to_the_board() {
        // The registry and the drawing are two lists, and a name added to one and not the other is a
        // rail button that quietly draws the wrong picture — which is what happened the first time
        // `database` and `table` were added, and it is invisible in every test that does not open the
        // image. This is that check as an assertion.
        // A function item cast to an integer is the whole of the question here -- two names draw
        // the same picture when they are the same function -- so the lint is answered rather than
        // worked around. `fn_addr_comparisons` is about *comparing* addresses across crates, which
        // this is not: both are `fn(&Painter, Pos2, Color32)` from this module.
        #[allow(clippy::fn_to_numeric_cast_any)]
        let address = |drawing: fn(&egui::Painter, egui::Pos2, egui::Color32)| drawing as usize;
        let fallback = address(pane_icon("nothing-like-this"));
        assert_eq!(fallback, address(icon::board), "the fallback is still the board");
        for name in PANE_ICONS {
            let drawn = address(pane_icon(name));
            if *name == "board" {
                continue;
            }
            assert_ne!(drawn, fallback, "`{name}` is in PANE_ICONS with no drawing behind it");
        }
    }
}
