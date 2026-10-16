//! `Settings -> Plugins`: the marketplace, and what is installed.
//!
//! Laid out the way the Settings design draws it (`task-2246`): a carved track holding `Marketplace` and
//! `Installed` with a count on the second, a search well beside it, the list of plugins in a carved well at the
//! left with the chosen one raised out of it, and the chosen plugin's own card at the right with its vendor, a
//! switch that turns it on and off, a button, what it does and what it does not do.
//!
//! **The catalogue is the bundled set, and there is no network call.** Fetching a plugin over the network
//! means signature checking, a trust decision and a story about downloaded code, none of which a format that
//! executes nothing has earned yet.

use egui::{Pos2, Rect, Sense, Vec2};
use rux::{Fill, Style};

use crate::components::controls::TextBoxes;
use crate::components::scrollbar;
use crate::components::settings_kit::{self as kit, Kind as ButtonKind};

/// What the list's scrollbar is called.
const LIST_SCROLLBAR: &str = "plugins list";
use crate::services::plugins::{Plugin, Plugins};

/// How wide the list at the left is: `width: 240px`.
const LIST: f32 = 240.0;
/// Between the list and the card, and between the tabs and the search well.
const GAP: f32 = 18.0;
/// How tall the row of tabs and the search well is.
const BAR: f32 = 40.0;
/// What is left under the list, above the dialog's bottom edge.
const LIST_MARGIN: f32 = 24.0;
/// The list is never shorter than this, in a dialog made very small.
const LIST_SHORTEST: f32 = 160.0;
/// How tall one plugin's row is, and the gap between two.
const ROW: f32 = 50.0;
const ROW_GAP: f32 = 4.0;

/// Which of the two tabs is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Marketplace,
    Installed,
}

/// What the page is showing.
#[derive(Debug, Default, Clone)]
pub struct PluginsState {
    pub tab: Tab,
    pub search: String,
    /// The plugin whose card is showing, by id.
    pub chosen: Option<String>,
    /// How far the list at the left is scrolled, in points.
    pub list_scroll: f32,
}

/// What the page asked for.
#[derive(Debug, Default, PartialEq)]
pub struct PluginsOutcome {
    /// Write this plugin out to the settings folder and load it back from there.
    pub install: Option<String>,
    /// Take the folder away again, and go back to the copy that shipped in the binary.
    pub uninstall: Option<String>,
    /// Switch a plugin on or off.
    pub set_enabled: Option<(String, bool)>,
    /// How tall the page came out, which is what decides whether the dialog gives it a scrollbar.
    pub height: f32,
}

/// Draw the page down `area`.
pub fn show(
    ui: &mut egui::Ui,
    area: Rect,
    state: &mut PluginsState,
    plugins: &Plugins,
    installed_on_disk: &dyn Fn(&str) -> bool,
    _icon_for: &dyn Fn(&str) -> Option<egui::TextureHandle>,
) -> PluginsOutcome {
    let mut outcome = PluginsOutcome::default();
    let bar = Rect::from_min_size(area.min, Vec2::new(area.width(), BAR));
    tabs_and_search(ui, bar, state, plugins);
    let top = bar.bottom() + 16.0;
    // **The list is as tall as the room under the tabs and scrolls on its own** (`task-2247`). Grown to hold
    // every plugin, the marketplace made the whole page scroll, the card at the right included.
    let room = (area.bottom() - top - LIST_MARGIN).max(LIST_SHORTEST);
    let list = Rect::from_min_size(Pos2::new(area.left(), top), Vec2::new(LIST, room));
    let (showing, list_bottom) = list_of_plugins(ui, list, state, plugins);
    if state.chosen.as_ref().is_none_or(|chosen| !showing.contains(chosen)) {
        state.chosen = showing.first().cloned();
    }
    let mut bottom = list_bottom;
    if let Some(id) = state.chosen.clone() {
        if let Some(plugin) = plugins.get(&id) {
            let card = Rect::from_min_max(
                Pos2::new(list.right() + GAP, top),
                Pos2::new(area.right(), top),
            );
            bottom = bottom.max(detail(ui, card, plugin, installed_on_disk(&id), &mut outcome));
        }
    }
    outcome.height = bottom - area.top() + 20.0;
    outcome
}

/// The two tabs and the search well.
fn tabs_and_search(ui: &mut egui::Ui, bar: Rect, state: &mut PluginsState, plugins: &Plugins) {
    kit::layer(ui, "plugins-bar", bar, |rux| {
        let theme = rux.theme();
        let chat = theme.chat;
        let style = Style::sans(12.0);
        let count = plugins.enabled_count().to_string();
        let count_width = (rux.measure(Style::sans(10.5), &count).x + 12.0).max(20.0);
        let widths = [
            rux.measure(style, "Marketplace").x + 32.0,
            rux.measure(style, "Installed").x + 16.0 + 8.0 + count_width + 12.0,
        ];
        let track = Rect::from_min_size(
            Pos2::new(bar.left(), bar.center().y - 20.0),
            Vec2::new(widths[0] + widths[1] + 4.0 + 8.0, 40.0),
        );
        rux.chrome.surface(track, 20.0, chat.well, chat.carved_sm());
        let mut x = track.left() + 4.0;
        for (index, (tab, name)) in
            [(Tab::Marketplace, "Marketplace"), (Tab::Installed, "Installed")]
                .into_iter()
                .enumerate()
        {
            let pill = Rect::from_min_size(
                Pos2::new(x, track.center().y - 16.0),
                Vec2::new(widths[index], 32.0),
            );
            let on = state.tab == tab;
            let response =
                rux.ui.interact(pill, rux.ui.id().with(("plugins-tab", name)), Sense::click());
            if on {
                rux.chrome.surface(pill, 16.0, Fill::gradient(chat.chip, pill), chat.raised_chip());
            } else if response.hovered() {
                rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let ink = match on {
                true => theme.ink.i900,
                false => theme.ink.i500,
            };
            let galley = rux.text(style, name, ink);
            rux::text::draw_left_capitals(
                rux.painter(),
                Pos2::new(pill.left() + 16.0, pill.center().y),
                galley,
                style,
                ink,
            );
            if tab == Tab::Installed {
                let badge = Rect::from_min_size(
                    Pos2::new(pill.right() - 12.0 - count_width, pill.center().y - 10.0),
                    Vec2::new(count_width, 20.0),
                );
                rux.chrome.surface(badge, 10.0, chat.well, chat.carved_sm());
                let galley = rux.text(Style::sans(10.5), &count, theme.ink.i500);
                rux::text::draw_centred(rux.painter(), badge, galley, theme.ink.i500);
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, name)
            });
            if response.clicked() && !on {
                state.tab = tab;
                state.chosen = None;
            }
            x = pill.right() + 4.0;
        }

        let well = Rect::from_min_max(
            Pos2::new(track.right() + 14.0, bar.center().y - 18.0),
            Pos2::new(bar.right(), bar.center().y + 18.0),
        );
        rux.chrome.surface(well, 18.0, chat.well, chat.carved_sm());
        rux.mark(
            rux::Mark::new(rux::Icon::Search, 14.0).stroke(1.8 * 24.0 / 14.0),
            Pos2::new(well.left() + 21.0, well.center().y),
            theme.ink.i400,
        );
        let font = Style::sans(12.5).font();
        let id = rux.ui.id().with("plugins-search");
        let text = crate::components::controls::field_takes_the_whole_rectangle_at(
            rux.ui,
            well,
            37.0,
            id,
            "Search field",
            &font,
        );
        let mut child = rux.ui.new_child(egui::UiBuilder::new().max_rect(text));
        let response = child.text_box(
            egui::TextEdit::singleline(&mut state.search)
                .id(id)
                .hint_text(crate::components::controls::placeholder(
                    "Search plugins",
                    &font,
                    theme.ink.i400,
                ))
                .font(font.clone())
                .frame(egui::Frame::NONE)
                .desired_width(text.width())
                .text_color(theme.ink.i900),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Search plugins")
        });
    });
}

/// The list at the left, in a carved well. Answers the ids it showed, in order, and where it ended.
fn list_of_plugins(
    ui: &mut egui::Ui,
    area: Rect,
    state: &mut PluginsState,
    plugins: &Plugins,
) -> (Vec<String>, f32) {
    let needle = state.search.trim().to_lowercase();
    let showing: Vec<&Plugin> = plugins
        .all()
        .iter()
        .filter(|plugin| state.tab == Tab::Marketplace || plugin.enabled)
        .filter(|plugin| {
            needle.is_empty()
                || plugin.name.to_lowercase().contains(&needle)
                || plugin.description.to_lowercase().contains(&needle)
        })
        .collect();
    let ids: Vec<String> = showing.iter().map(|plugin| plugin.id.clone()).collect();
    let rows = showing.len().max(1) as f32;
    let well = area;
    let content = 16.0 + rows * ROW + (rows - 1.0) * ROW_GAP;
    let was = state.list_scroll;
    let bar = scrollbar::Bar::new(well, was, content, well.height());
    let grab = match &bar {
        Some(bar) => scrollbar::grab(ui, bar, LIST_SCROLLBAR),
        None => scrollbar::Grab::default(),
    };
    if let Some(to) = grab.scroll {
        state.list_scroll = to;
    }
    if ui.rect_contains_pointer(well) {
        // Taken out of the frame, so the dialog does not also scroll the page under the list.
        let wheel = ui.input_mut(|input| std::mem::take(&mut input.smooth_scroll_delta.y));
        state.list_scroll -= wheel;
    }
    state.list_scroll = state.list_scroll.clamp(0.0, (content - well.height()).max(0.0));
    let scroll = state.list_scroll;
    let mut chosen = None;
    kit::layer(ui, "plugins-list-well", well, |rux| kit::list_well(rux, well, 22.0));
    let clip = ui.clip_rect();
    ui.set_clip_rect(clip.intersect(well.shrink(2.0)));
    kit::layer(ui, "plugins-list", well, |rux| {
        let theme = rux.theme();
        if showing.is_empty() {
            let galley = rux.text(Style::sans(11.5), "Nothing matches.", theme.ink.i400);
            rux::text::draw_left_capitals(
                rux.painter(),
                Pos2::new(well.left() + 22.0, well.top() + 8.0 + ROW / 2.0),
                galley,
                Style::sans(11.5),
                theme.ink.i400,
            );
        }
        let mut top = well.top() + 8.0 - scroll;
        for plugin in &showing {
            let row = Rect::from_min_size(
                Pos2::new(well.left() + 8.0, top),
                Vec2::new(well.width() - 16.0, ROW),
            );
            let picked = state.chosen.as_deref() == Some(plugin.id.as_str());
            if kit::list_row(rux, row, 16.0, &plugin.name, picked).clicked() {
                chosen = Some(plugin.id.clone());
            }
            let name =
                rux.elided(Style::sans(12.5), &plugin.name, theme.ink.i900, row.width() - 28.0);
            rux.painter().galley(
                Pos2::new(row.left() + 14.0, row.top() + 9.0),
                name,
                theme.ink.i900,
            );
            let note = format!(
                "{} \u{00B7} {}",
                match plugin.bundled {
                    true => "Bundled",
                    false => "Installed",
                },
                match plugin.enabled {
                    true => "enabled",
                    false => "disabled",
                },
            );
            let note = rux.elided(Style::sans(11.0), &note, theme.ink.i400, row.width() - 28.0);
            rux.painter().galley(
                Pos2::new(row.left() + 14.0, row.top() + 27.0),
                note,
                theme.ink.i400,
            );
            top = row.bottom() + ROW_GAP;
        }
    });
    ui.set_clip_rect(clip);
    if let Some(bar) = scrollbar::Bar::new(well, scroll, content, well.height()) {
        scrollbar::paint(ui, &bar, LIST_SCROLLBAR, grab.active || (scroll - was).abs() > 0.01);
    }
    if let Some(id) = chosen {
        state.chosen = Some(id);
    }
    (ids, well.bottom())
}

/// The chosen plugin's own card. Answers where it ended.
fn detail(
    ui: &mut egui::Ui,
    area: Rect,
    plugin: &Plugin,
    on_disk: bool,
    outcome: &mut PluginsOutcome,
) -> f32 {
    let mut column = kit::Column::new(area.left(), area.right(), area.top());
    // **Two controls, because one button used to mean three things.** `task-1795`: *"The Install button
    // should be Uninstall if the plugin is already installed."* The word on the button is what pressing it
    // **does**. A bundled plugin is already installed in every sense a person means, so what pressing it does
    // is write the folder out so it can be edited by hand, which is `Customise`. `Install` is kept for the one
    // state where it is the true word: a plugin that is neither bundled nor on disk. Switching one on and off
    // is a different question and is the switch beside it.
    let word = match (on_disk, plugin.bundled) {
        (true, _) => "Uninstall",
        (false, true) => "Customise",
        (false, false) => "Install",
    };
    let where_from = match (on_disk, plugin.bundled) {
        (true, true) => "Its folder is under the settings folder and can be edited by hand. Uninstalling takes that folder away and puts the plugin back to the one Unluminous ships.",
        (true, false) => "Its folder is under the settings folder and can be edited by hand. Uninstalling removes it.",
        (false, _) => "Installed with Unluminous and working. Customising writes its folder out where it can be edited by hand.",
    };
    let vendor = format!("{} \u{00B7} version {}", plugin.vendor, plugin.version);
    let overview = overview_of(plugin);
    // A card with no heading of its own: the name is set larger than a card's title, with the vendor under
    // it and the switch and the button at its right.
    kit::card(ui, &mut column, "", None, |card| {
        card.grow(|rux, column| {
            let theme = rux.theme();
            let name_style = Style::sans(17.0).semibold();
            let name = rux.elided(name_style, &plugin.name, theme.ink.i900, column.width() - 220.0);
            let name_height = name.size().y;
            rux.painter().galley(Pos2::new(column.left, column.pen), name, theme.ink.i900);
            let vendor_galley = rux.text(Style::sans(11.5), &vendor, theme.ink.i400);
            let vendor_height = vendor_galley.size().y;
            rux.painter().galley(
                Pos2::new(column.left, column.pen + name_height + 2.0),
                vendor_galley,
                theme.ink.i400,
            );
            let header = name_height + 2.0 + vendor_height;
            let button_width = kit::button_width(rux, word, false);
            let button = Rect::from_min_size(
                Pos2::new(column.right - button_width, column.pen + (header - kit::BUTTON) / 2.0),
                Vec2::new(button_width, kit::BUTTON),
            );
            if kit::button(rux, button, word, ButtonKind::Chip) {
                match on_disk {
                    true => outcome.uninstall = Some(plugin.id.clone()),
                    false => outcome.install = Some(plugin.id.clone()),
                }
            }
            let track = Rect::from_min_size(
                Pos2::new(
                    button.left() - 12.0 - kit::SWITCH.x,
                    button.center().y - kit::SWITCH.y / 2.0,
                ),
                kit::SWITCH,
            );
            let mut enabled = plugin.enabled;
            if kit::switch(rux, track, "Enabled", &mut enabled, track) {
                outcome.set_enabled = Some((plugin.id.clone(), enabled));
            }
            let mut pen = column.pen + header + 14.0;
            pen = kit::paragraph(rux, Pos2::new(column.left, pen), column.width(), where_from);
            pen = section(rux, column, pen + 14.0, "Overview", &overview);
            if !plugin.limitations.is_empty() {
                pen = section(rux, column, pen + 14.0, "What it does not do", &plugin.limitations);
            }
            ((), pen)
        });
    });
    column.pen - kit::CARD_GAP
}

/// A heading and a carved well of words under it.
fn section(
    rux: &mut rux::Rux<'_>,
    column: kit::Column,
    top: f32,
    heading: &str,
    said: &str,
) -> f32 {
    let theme = rux.theme();
    let style = Style::sans(12.5).semibold();
    let galley = rux.text(style, heading, theme.ink.i900);
    let height = galley.size().y;
    rux.painter().galley(Pos2::new(column.left, top), galley, theme.ink.i900);
    let words = rux::text::wrapped(
        rux.painter(),
        kit::help_style(),
        said,
        theme.ink.i500,
        column.width() - 32.0,
    );
    let well = Rect::from_min_size(
        Pos2::new(column.left, top + height + 10.0),
        Vec2::new(column.width(), words.size().y + 28.0),
    );
    kit::list_well(rux, well, 16.0);
    rux.painter().galley(well.min + Vec2::new(16.0, 14.0), words, theme.ink.i500);
    well.bottom()
}

/// What a plugin is and what it claims, as one paragraph for its Overview well.
///
/// What a plugin claims depends on which kind it is: a language claims file types and brings a colour
/// scheme, and a plugin that draws claims neither and adds to the window instead. Saying `Colour scheme:
/// Dracula` about a plugin that names no colours would be a claim no plugin is allowed to make.
fn overview_of(plugin: &Plugin) -> String {
    let mut lines = vec![plugin.description.clone()];
    match plugin.kind {
        crate::services::plugins::Kind::Language => {
            let extensions: Vec<String> =
                plugin.extensions.iter().map(|extension| format!(".{extension}")).collect();
            lines.push(format!("Claims {}.", extensions.join(", ")));
            // The scheme that is actually colouring this language's files, which since `task-1776` is the
            // theme's when the theme names the nine tokens.
            let scheme = crate::services::plugins::scheme_of(plugin);
            let from = match crate::theme::syntax().is_some() {
                true => ", from the theme, which colours every language at once",
                false => "",
            };
            lines.push(format!(
                "Colour scheme: {}{from}. A scheme colours the tokens and not the editing area, so the window still lets the desktop through.",
                scheme.name
            ));
        }
        crate::services::plugins::Kind::Ui => {
            lines.push(format!("Adds {}", contributions_of(plugin)));
            lines.push(
                "It draws with Unluminous's own controls, so it takes the font and the transparency from Appearance and cannot name a colour of its own.".to_owned(),
            );
        }
        crate::services::plugins::Kind::Theme => {
            let names: Vec<&str> = plugin.themes.iter().map(|theme| theme.name.as_str()).collect();
            lines.push(format!("Carries {}.", names.join(", ")));
            lines.push(
                "A theme says what every name in Unluminous's own palette means, so the list of colours stays closed while what they are changes. Choose one in Appearance & Behavior, Theme.".to_owned(),
            );
        }
    }
    lines.retain(|line| !line.trim().is_empty());
    lines.join("\n")
}

/// What a plugin that draws adds to the window, as a sentence for its card.
///
/// Read from the manifest rather than written down here, so a plugin that later contributes a second thing
/// says so with no change to this page.
fn contributions_of(plugin: &Plugin) -> String {
    let mut added: Vec<String> = Vec::new();
    if let Some(pane) = &plugin.contributions.pane {
        added.push(format!("a pane called {} with a button in the rail", pane.label));
    }
    if let Some(tab) = &plugin.contributions.tab {
        added.push(format!("the {} tab in the editing area", tab.label));
    }
    if let Some(menu) = &plugin.contributions.menu {
        added.push(format!("the {} menu", menu.name));
    }
    if plugin.contributions.page.is_some() {
        added.push("a page in these settings".to_owned());
    }
    match added.is_empty() {
        true => "nothing to the window.".to_owned(),
        false => format!("{}.", added.join(", ")),
    }
}
