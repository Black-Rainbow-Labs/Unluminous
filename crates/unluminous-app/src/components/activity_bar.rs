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
/// Where a button's middle is, from the rail's left edge: the middle of the rail left of its groove, which
/// sits two points in from the right hand edge (`task-2282`). The buttons used to start where the window's
/// resize grip stops, six points in, which left six points beside them on the left and two on the right.
const CENTRE: f32 = (size::ACTIVITY_BAR - 2.0) / 2.0;
/// How much of a button's left side the window's resize grip covers, which the button leaves to the grip
/// so a press there still resizes the window.
const UNDER_THE_GRIP: f32 =
    (crate::components::resize_edges::EDGE - (CENTRE - BUTTON / 2.0)).max(0.0);
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
    /// A button was carried to another place and let go: the `rail.order` setting that says the rail as it
    /// now is (`task-2340`).
    pub order: Option<String>,
}

/// Draw the rail into `area`, and report what was pressed.
pub fn show(ui: &mut egui::Ui, area: Rect, state: RailState, opacity: f32) -> RailOutcome {
    show_with(ui, area, state, opacity, &[], "")
}

/// The same, with the buttons the plugins that are switched on contributed, in the order the `rail.order`
/// setting asks for.
pub fn show_with(
    ui: &mut egui::Ui,
    area: Rect,
    state: RailState,
    opacity: f32,
    plugins: &[PluginButton],
    order: &str,
) -> RailOutcome {
    let painter = ui.painter_at(area);
    painter.rect_filled(
        area,
        CornerRadius::ZERO,
        crate::theme::pane_ground(color::title_bar(), opacity),
    );
    // The groove down the rail's right hand edge: a dark line and a lit one beside it. Not under glass
    // (`task-2287`), where the rail sits on the window's glass and the panels beside it are what mark
    // where it ends.
    if !crate::theme::is_glass() {
        crate::components::chrome_kit::groove(
            &painter,
            Pos2::new(area.right() - 2.0, area.top()),
            Pos2::new(area.right() - 2.0, area.bottom()),
        );
    }
    crate::components::chrome_kit::layer(ui, "rail", area, 0.0, |rux| {
        buttons(rux, area, state, plugins, order)
    })
}

/// One button on the rail, whichever group it is in and whoever contributed it: its name, its icon,
/// whether it is showing, whether it can be pressed, the action pressing it performs, and the panel its
/// right click menu is about. The panel is `None` for a button with nowhere to be moved to.
struct Entry {
    name: String,
    draw: IconDrawer,
    on: bool,
    enabled: bool,
    action: Action,
    panel: Option<crate::app::dock::Panel>,
}

/// Every button in the rail, drawn into one `rux` layer.
fn buttons(
    rux: &mut rux::Rux<'_>,
    area: Rect,
    state: RailState,
    plugins: &[PluginButton],
    order: &str,
) -> RailOutcome {
    let mut outcome = RailOutcome::default();
    // Centred on the rail left of its groove (`task-2282`). The outer six points belong to
    // `components::resize_edges`, so the part of a button under them is drawn and not pressed: see
    // `UNDER_THE_GRIP`.
    let centre_x = area.left() + CENTRE;
    let (top, bottom) = arranged_entries(state, plugins, order);
    let names = |group: &[Entry]| group.iter().map(|one| one.name.clone()).collect::<Vec<_>>();
    let (top_names, bottom_names) = (names(&top), names(&bottom));

    // **A button can be carried to another place on the rail** (`task-2340`). The one being carried is drawn
    // under the pointer, last, so it is over the rest; the others stay where they are, and an accent line
    // says where it would land.
    let carried = rux.ui.ctx().dragged_id();
    let pointer = rux.ui.ctx().pointer_latest_pos();
    let mut in_the_air: Option<(&Entry, Pos2)> = None;
    let mut dropped: Option<(String, f32)> = None;
    let centres = places(area, top.len(), bottom.len());
    for (entry, y) in top.iter().chain(bottom.iter()).zip(centres) {
        let centre = Pos2::new(centre_x, y);
        // A button that would not fit is not drawn: the rail is as tall as the window and a button half off
        // the end reads as a fault rather than as a full rail.
        if centre.y - BUTTON / 2.0 < area.top() || centre.y + BUTTON / 2.0 > area.bottom() {
            continue;
        }
        if carried.is_some_and(|id| id == button_id(rux, &entry.name)) {
            in_the_air = Some((entry, centre));
            continue;
        }
        press(rux, entry, centre, &mut outcome, &mut dropped);
    }
    if let Some((entry, rest)) = in_the_air {
        let y = pointer
            .map_or(rest.y, |at| at.y)
            .clamp(area.top() + MARGIN + BUTTON / 2.0, area.bottom() - MARGIN - BUTTON / 2.0);
        press(rux, entry, Pos2::new(centre_x, y), &mut outcome, &mut dropped);
        if let Some(at) = pointer {
            let slot = landing(area, &top_names, &bottom_names, &entry.name, at.y);
            insertion_mark(rux.ui.painter(), area, slot.mark);
        }
    }
    if let Some((name, y)) = dropped {
        let slot = landing(area, &top_names, &bottom_names, &name, y);
        outcome.order = Some(order_text(&slot.top, &slot.bottom));
    }
    outcome
}

/// The id a rail button's response has, which is how the button being carried is known before it is drawn.
fn button_id(rux: &rux::Rux<'_>, name: &str) -> egui::Id {
    rux.ui.id().with(("activity", name))
}

/// Draw one button at `centre` and take in what was done to it.
fn press(
    rux: &mut rux::Rux<'_>,
    entry: &Entry,
    centre: Pos2,
    outcome: &mut RailOutcome,
    dropped: &mut Option<(String, f32)>,
) {
    let pressed = rail_button(rux, centre, &entry.name, entry.draw, entry.on, entry.enabled);
    if pressed.clicked {
        outcome.chosen = Some(entry.action.clone());
    }
    if let (Some(at), Some(panel)) = (pressed.menu, entry.panel) {
        outcome.menu = Some((at, panel));
    }
    if let Some(at) = pressed.dropped {
        *dropped = Some((entry.name.clone(), at.y));
    }
}

/// The rail's buttons in the two groups, top to bottom in each, in the order `order` asks for.
///
/// The default order is the one the rail always had. The top group is the panes at the side, then what
/// plugins contributed to it. The bottom group is the three tiles along the bottom of the window with the
/// terminal in the corner, and the plugins' buttons above them.
fn arranged_entries(
    state: RailState,
    plugins: &[PluginButton],
    order: &str,
) -> (Vec<Entry>, Vec<Entry>) {
    let (mut top, bottom) = built_in_entries(state);
    let mut plugin_bottom = Vec::new();
    for button in plugins {
        let entry = Entry {
            name: button.label.clone(),
            draw: rail_mark(&button.icon),
            on: button.on,
            enabled: true,
            action: match button.opens {
                Opens::Pane => Action::PluginPane { pane: button.key.clone() },
                Opens::Tab => Action::PluginTab { tab: button.key.clone() },
            },
            // A tab has no side to be moved to, so no menu. See [`Opens`].
            panel: match button.opens {
                Opens::Pane => Some(crate::app::dock::Panel::Plugin(button.slot as u8)),
                Opens::Tab => None,
            },
        };
        match button.bottom {
            true => plugin_bottom.push(entry),
            false => top.push(entry),
        }
    }
    // A plugin's bottom button was counted up from the corner after Unluminous's own, so the first one is the
    // nearest the tiles.
    plugin_bottom.reverse();
    plugin_bottom.extend(bottom);
    let bottom = plugin_bottom;
    let names = |group: &[Entry]| group.iter().map(|one| one.name.clone()).collect::<Vec<_>>();
    let (top_names, bottom_names) = arranged(&names(&top), &names(&bottom), order);
    let mut every: Vec<Entry> = top.into_iter().chain(bottom).collect();
    let mut take = |wanted: Vec<String>| {
        wanted
            .iter()
            .filter_map(|name| {
                let at = every.iter().position(|one| one.name == *name)?;
                Some(every.remove(at))
            })
            .collect::<Vec<Entry>>()
    };
    let top = take(top_names);
    let bottom = take(bottom_names);
    (top, bottom)
}

/// Unluminous's own buttons in their default groups, top to bottom in each.
fn built_in_entries(state: RailState) -> (Vec<Entry>, Vec<Entry>) {
    let entry = |name: &str,
                 draw: IconDrawer,
                 on: bool,
                 enabled: bool,
                 action: Action,
                 panel: Option<crate::app::dock::Panel>| Entry {
        name: name.to_owned(),
        draw,
        on,
        enabled,
        action,
        panel,
    };
    let mut top = vec![
        entry(
            "Project",
            IconDrawer::Line(rux::Icon::Project),
            state.explorer_visible,
            true,
            Action::ToggleExplorer,
            Some(crate::app::dock::Panel::Explorer),
        ),
        // **Under the folder icon**, which is where `task-28` asks for it. Named `Editing Area` rather than
        // `Editor` or `Files`: `Editor` is the Settings window's own page for the same reason `Project` above is
        // the explorer, and no two controls in one window may share a name.
        //
        // It is not a `Panel`, so a right click on it opens no menu: the editing area is what is left when the
        // panels have taken their room, and there is no edge to move it to.
        entry(
            "Editing Area",
            IconDrawer::Line(rux::Icon::Document),
            state.editor_visible,
            true,
            Action::ToggleEditor,
            None,
        ),
        // Not `Git`: the menu bar already has a `Git`, and no two controls in one window may share a name. This
        // button opens the commit panel, which is a modal rather than a docked panel, so it has no side to be
        // moved to and its button opens no menu.
        entry(
            "Version Control",
            IconDrawer::Line(rux::Icon::Git),
            state.git_open,
            state.in_repository,
            Action::Git(GitAction::Commit),
            None,
        ),
    ];
    // The Realm - `task-1904`. In the **top** group, because a canvas is a surface with several character grids
    // inside it rather than one. Absent while its plugin is off, which is the rule a plugin's own rail button
    // keeps (`task-2202`).
    if state.realm_available {
        top.push(entry(
            "Realm",
            IconDrawer::Line(rux::Icon::Realm),
            state.realm_visible,
            true,
            Action::Realm(crate::app::actions::RealmAction::Toggle),
            Some(crate::app::dock::Panel::Realm),
        ));
    }
    // The three tiles that live along the bottom of the window, top to bottom, with the terminal in the corner,
    // which `task-1658`'s capture put it in. `Terminal tile` rather than `Terminal`, because `Edit -> Settings`
    // has a page called `Terminal` and the `View` menu has an entry called `Terminal`; `Run tile` and `Debug
    // tile` are named to match.
    let bottom = vec![
        entry(
            "Debug tile",
            DEBUG_MARK,
            state.debug_visible,
            true,
            Action::ToggleDebugTile,
            Some(crate::app::dock::Panel::Debug),
        ),
        entry(
            "Run tile",
            IconDrawer::Line(rux::Icon::RunTile),
            state.run_visible,
            true,
            Action::ToggleRunTile,
            Some(crate::app::dock::Panel::Run),
        ),
        entry(
            "Terminal tile",
            IconDrawer::Line(rux::Icon::Terminal),
            state.terminal_visible,
            true,
            Action::ToggleTerminal,
            Some(crate::app::dock::Panel::Terminal),
        ),
    ];
    (top, bottom)
}

/// The middle of each button, top group first and then the bottom group, both top to bottom. The top group
/// is counted down from the top of the rail and the bottom group up from its bottom, so the last button of
/// the bottom group is always in the corner however many there are.
fn places(area: Rect, top: usize, bottom: usize) -> Vec<f32> {
    let downwards = (0..top).map(|index| area.top() + MARGIN + BUTTON / 2.0 + index as f32 * STEP);
    let upwards = (0..bottom)
        .map(|index| area.bottom() - MARGIN - BUTTON / 2.0 - (bottom - 1 - index) as f32 * STEP);
    downwards.chain(upwards).collect()
}

/// The two groups' names in the order `order` asks for, given the default groups.
///
/// `order` is the `rail.order` setting: names from the top of the rail down, the top group, then a `|`, then
/// the bottom group. A name it gives that is not a button is passed over. A button it does not name stays in
/// its default group: after the named ones at the top, and above them at the bottom, so a button that is new
/// to the rail never takes a place somebody chose for another.
pub fn arranged(top: &[String], bottom: &[String], order: &str) -> (Vec<String>, Vec<String>) {
    if order.trim().is_empty() {
        return (top.to_vec(), bottom.to_vec());
    }
    let (asked_top, asked_bottom) = order.split_once('|').unwrap_or((order, ""));
    let every: Vec<&String> = top.iter().chain(bottom).collect();
    let mut used: Vec<String> = Vec::new();
    let mut named = |asked: &str| {
        let mut found = Vec::new();
        for name in asked.split(',').map(str::trim) {
            let known = every.iter().any(|one| one.as_str() == name);
            if known && !used.iter().any(|one| one == name) {
                used.push(name.to_owned());
                found.push(name.to_owned());
            }
        }
        found
    };
    let mut new_top = named(asked_top);
    let new_bottom = named(asked_bottom);
    let unnamed = |group: &[String]| -> Vec<String> {
        group.iter().filter(|name| !used.contains(name)).cloned().collect()
    };
    new_top.extend(unnamed(top));
    let mut new_bottom_group = unnamed(bottom);
    new_bottom_group.extend(new_bottom);
    (new_top, new_bottom_group)
}

/// The `rail.order` setting that says these two groups, top to bottom in each.
pub fn order_text(top: &[String], bottom: &[String]) -> String {
    format!("{} | {}", top.join(", "), bottom.join(", "))
}

/// Where a button carried to some height lands: the two groups as they would be, and the height of the line
/// that says so.
pub struct Landing {
    pub top: Vec<String>,
    pub bottom: Vec<String>,
    pub mark: f32,
}

/// Work out where the button `carried` lands when it is let go at height `y`.
///
/// Every gap a button could go into is a candidate, in both groups, and the one nearest the pointer wins: the
/// gap above each button, and the one after the last button of each group. So a button carried down past the
/// top group's last one, into the empty middle of the rail, joins whichever group is nearer.
///
/// The gaps are the ones on the rail as it is drawn while the button is carried, which is with every other
/// button where it was and the carried one's own place still counted, because those are the gaps a person
/// sees. Taking the button out first would put every gap below it one place higher than the line drawn.
pub fn landing(area: Rect, top: &[String], bottom: &[String], carried: &str, y: f32) -> Landing {
    let half_gap = (STEP - BUTTON) / 2.0;
    // The gap above top button `index`, and after the last one at `index == top.len()`.
    let top_gaps = (0..=top.len())
        .map(|index| (false, index, area.top() + MARGIN + index as f32 * STEP - half_gap));
    // The gap above bottom button `index`, and below the corner one at `index == bottom.len()`.
    let count = bottom.len() as f32;
    let bottom_gaps = (0..=bottom.len()).map(|index| {
        (true, index, area.bottom() - MARGIN - (count - index as f32) * STEP + half_gap)
    });
    let (in_bottom, mut index, mark) = top_gaps
        .chain(bottom_gaps)
        .min_by(|one, other| (one.2 - y).abs().total_cmp(&(other.2 - y).abs()))
        .unwrap_or((false, top.len(), area.top() + MARGIN));
    // A gap below the carried button's own place in the same group moves up one once it is taken out.
    let was = match in_bottom {
        true => bottom.iter().position(|name| name == carried),
        false => top.iter().position(|name| name == carried),
    };
    if was.is_some_and(|was| was < index) {
        index -= 1;
    }
    let mut top: Vec<String> = top.iter().filter(|name| *name != carried).cloned().collect();
    let mut bottom: Vec<String> = bottom.iter().filter(|name| *name != carried).cloned().collect();
    match in_bottom {
        true => bottom.insert(index.min(bottom.len()), carried.to_owned()),
        false => top.insert(index.min(top.len()), carried.to_owned()),
    }
    Landing { top, bottom, mark }
}

/// The two point accent line across the rail that says where a carried button would land, which is the mark
/// a carried tab draws between two tabs (`file_tabs::insertion_mark`).
fn insertion_mark(painter: &egui::Painter, area: Rect, y: f32) {
    let y = y.clamp(area.top() + 2.0, area.bottom() - 2.0);
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(area.left() + 4.0, y - 1.0),
            Vec2::new(area.width() - 10.0, 2.0),
        ),
        CornerRadius::same(1),
        color::accent(),
    );
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
        "document" => icon::document,
        "blocks" => icon::blocks,
        _ => icon::board,
    }
}

/// The mark a plugin's button draws: the design's line mark where it has one for the name, and Unluminous's own
/// drawing for the rest.
fn rail_mark(name: &str) -> IconDrawer {
    match name {
        "board" => IconDrawer::Svg(&svg_icon::LIST_TODO, MARK),
        "design" => IconDrawer::Svg(&svg_icon::PAINTBRUSH_VERTICAL, MARK),
        "blocks" => IconDrawer::Svg(&svg_icon::BLOCKS, MARK),
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
    /// Where the pointer was when the button was let go after being carried. `task-2340`.
    dropped: Option<Pos2>,
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
    let pressable = Rect::from_min_max(Pos2::new(hit.left() + UNDER_THE_GRIP, hit.top()), hit.max);
    // **Every button can be carried**, a dimmed one too, because where it sits is not about whether it can be
    // pressed now (`task-2340`). egui does not count a press that moved as a click, so carrying one never
    // presses it.
    let sense = if enabled { Sense::click_and_drag() } else { Sense::drag() };
    let response =
        rux.ui.interact(pressable, rux.ui.id().with(("activity", name)), sense).with_hint(name);
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
        dropped: match response.drag_stopped() {
            true => response.interact_pointer_pos().or_else(|| rux.ui.ctx().pointer_latest_pos()),
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

/// The order of the rail's buttons, `task-2340`.
#[cfg(test)]
mod order_tests {
    use super::{arranged, landing, order_text, BUTTON, MARGIN, STEP};

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|one| (*one).to_owned()).collect()
    }

    fn defaults() -> (Vec<String>, Vec<String>) {
        (
            names(&["Project", "Editing Area", "Version Control", "Realm"]),
            names(&["Debug tile", "Run tile", "Terminal tile"]),
        )
    }

    #[test]
    fn an_empty_order_is_the_order_the_rail_always_had() {
        let (top, bottom) = defaults();
        assert_eq!(arranged(&top, &bottom, ""), (top.clone(), bottom.clone()));
        assert_eq!(arranged(&top, &bottom, "  "), (top, bottom));
    }

    #[test]
    fn an_order_moves_buttons_between_groups_and_unnamed_ones_keep_their_group() {
        let (top, bottom) = defaults();
        let (new_top, new_bottom) =
            arranged(&top, &bottom, "Terminal tile, Realm | Project, Nothing Like This");
        assert_eq!(new_top, names(&["Terminal tile", "Realm", "Editing Area", "Version Control"]));
        // An unnamed bottom button is above the named ones, so the named one keeps the corner.
        assert_eq!(new_bottom, names(&["Debug tile", "Run tile", "Project"]));
    }

    #[test]
    fn a_name_given_twice_is_placed_once() {
        let (top, bottom) = defaults();
        let (new_top, new_bottom) = arranged(&top, &bottom, "Realm, Realm | Realm");
        assert_eq!(new_top.iter().filter(|one| *one == "Realm").count(), 1);
        assert!(!new_bottom.contains(&"Realm".to_owned()));
    }

    #[test]
    fn the_order_text_reads_back_as_the_same_order() {
        let top = names(&["Realm", "Project", "Editing Area", "Version Control"]);
        let bottom = names(&["Terminal tile", "Debug tile", "Run tile"]);
        let (all_top, all_bottom) = defaults();
        let text = order_text(&top, &bottom);
        assert_eq!(
            text,
            "Realm, Project, Editing Area, Version Control | Terminal tile, Debug tile, Run tile"
        );
        assert_eq!(arranged(&all_top, &all_bottom, &text), (top, bottom));
    }

    #[test]
    fn a_button_lands_in_the_gap_nearest_where_it_was_let_go() {
        let area = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(36.0, 600.0));
        let (top, bottom) = defaults();
        let centre = |index: usize| MARGIN + BUTTON / 2.0 + index as f32 * STEP;
        // Down past Version Control: the gap under it, which is above Realm.
        let down = landing(area, &top, &bottom, "Project", centre(2) + STEP / 2.0);
        assert_eq!(down.top, names(&["Editing Area", "Version Control", "Project", "Realm"]));
        assert_eq!(down.bottom, bottom);
        // Up to the very top.
        let up = landing(area, &top, &bottom, "Realm", 0.0);
        assert_eq!(up.top, names(&["Realm", "Project", "Editing Area", "Version Control"]));
        // Into the corner of the bottom group.
        let corner = landing(area, &top, &bottom, "Project", area.bottom());
        assert_eq!(corner.top, names(&["Editing Area", "Version Control", "Realm"]));
        assert_eq!(corner.bottom, names(&["Debug tile", "Run tile", "Terminal tile", "Project"]));
        // Let go where it already is, and nothing moves.
        let still = landing(area, &top, &bottom, "Editing Area", centre(1));
        assert_eq!(still.top, top);
    }
}
