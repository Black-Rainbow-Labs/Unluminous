//! The Settings window, opened from `Edit -> Settings`.
//!
//! ## The look is the Settings design (`task-2246`)
//!
//! The Claude Design canvas "Unluminous", page Settings, artboard `SettingsModal.dc.html`, with one artboard a
//! page beside it. One surface thirty points round on the pane's own ground with a long soft shadow under it,
//! and no title bar or footer: down the left a raised card holding the window's name, a carved search well and
//! the pages, each a pill with a line mark, grouped under small headings, with the page that is showing carved
//! into the card; on the right the page's group and its name as a breadcrumb, a round close button, and the
//! page's settings as raised cards that scroll under the breadcrumb. `components::settings_kit` is the cards
//! and every control in them, so a plugin's page is drawn with the same code.
//!
//! `components::modal::show_unframed` is the dialog, so it keeps the dragging, the resizing and the zoom every
//! modal has, and it is still a modal: the rest of the window is dimmed and takes no clicks while it is open.
//!
//! ## Changes take effect as they are made
//!
//! There is no `Apply`, so there is only a close button. A dialog with `Apply` has to hold a second copy of
//! every setting and decide what to do when the two disagree; showing the change straight away needs neither.
//! Enter closes it, as it closes every other modal.
//!
//! ## The page scrolls, and the window is one size
//!
//! A dialog that changed height as its list was walked would jump under the pointer, so the window is one
//! size for every page and a page taller than the room scrolls inside it. `task-1922` measured why: before it
//! a page that fitted exactly was a page where the next row added to it was drawn off the bottom and nobody
//! noticed. A page that fits shows no bar and cannot be scrolled.

use egui::{Pos2, Rect, Sense, Vec2};
use rux::components::ChipButton;
use rux::icon::{Icon, Mark};
use rux::{Fill, Style};

use crate::components::controls::TextBoxes;
use crate::components::mcp_page::{self, McpState};
use crate::components::modal;
use crate::components::plugins_page::{self, PluginsState};
use crate::components::scrollbar;
use crate::components::settings_kit::{self as kit, Column};
use crate::services::plugins::Plugins;
use crate::settings::{
    Indent, LineEndings, ModelKept, Page, Settings, Suggestions, UpdateCheck, ValueTooltip,
    FONT_SIZES, MIN_OPACITY, TERMINAL_FONT_SIZES, UI_FONT_SIZES,
};

/// How large the window is, before `modal::fit` shrinks it to a small Unluminous window: the design's
/// `width: 1000px; height: 700px`.
const WIDTH: f32 = 1000.0;
const HEIGHT: f32 = 700.0;
/// The room inside the dialog's edge, and how round it is.
const PAD: f32 = 20.0;
const RADIUS: f32 = 30.0;
/// The list of pages down the left, and the gap between it and the page.
const NAV_WIDTH: f32 = 248.0;
const NAV_GAP: f32 = 24.0;
/// The breadcrumb's row, the room above it, and the gap between it and the first card.
const HEADER: f32 = 36.0;
const HEADER_TOP: f32 = 4.0;
const HEADER_GAP: f32 = 18.0;
/// How far above the first card a page can still be seen, which is what keeps the top card's light edge.
const VIEW_ABOVE: f32 = kit::PAGE_MARGIN.top;
/// The room under the last card, so a page that ends there does not read as one that was cut off.
const TAIL: f32 = kit::PAGE_MARGIN.bottom;
/// What the page's scrollbar is called. `components::scrollbar` puts `Scroll` in front of it.
const SCROLLBAR: &str = "settings";

/// What a page reported when it was drawn: whether a setting changed, and how tall the page is.
///
/// The height is what decides whether there is anything to scroll. It cannot be worked out in advance,
/// because a page is cards laid one under another and where the last one ended is only known once it has
/// been drawn, so it is reported back rather than measured a second way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Drawn {
    /// Whether a setting on the page changed this frame.
    pub changed: bool,
    /// Whether the page asked for the Background grid.
    pub backgrounds: bool,
    /// How tall the page needs to be, from the top of the page's room to [`TAIL`] under its last card.
    pub height: f32,
}

/// What a page reports, from the column its cards were drawn down. `top` is where the page started.
pub(crate) fn drawn(top: f32, column: &Column, changed: bool) -> Drawn {
    Drawn { changed, backgrounds: false, height: column.pen - kit::CARD_GAP - top + TAIL }
}

/// The rectangle a page measures itself from, which is the page's room lifted by however far it is
/// scrolled.
///
/// Every page lays its cards out from `area.top()` down, so lifting the rectangle moves the whole page and no
/// page has to know that it is being scrolled. What keeps it inside the room is the clip: `Painter` clips
/// intersect, and `Ui::interact` cuts a control's interact rectangle the same way.
pub(crate) fn lifted(page_area: Rect, scroll: f32) -> Rect {
    page_area.translate(Vec2::new(0.0, -scroll))
}

/// How far a page really is scrolled, given how tall it came out and how tall the room is.
///
/// A page that fits cannot be scrolled at all, which is what leaves every page that fits drawn exactly where
/// it was.
pub(crate) fn settle_the_scroll(asked: f32, height: f32, view: f32) -> f32 {
    asked.clamp(0.0, (height - view).max(0.0))
}

/// Which page is showing, and what has been typed in the search box. Lives in the window's state, so it is
/// still there when the settings are opened again.
#[derive(Debug, Clone, Default)]
pub struct SettingsWindow {
    pub open: bool,
    pub page: Page,
    pub search: String,
    /// What the Plugins page is showing.
    pub plugins: PluginsState,
    /// What the MCP page is showing.
    pub mcp: McpState,
    /// How far the page showing is scrolled down, in points.
    pub scroll: f32,
    /// How tall the page came out when it was last drawn.
    ///
    /// Kept from one frame to the next because a page's height can only be answered by drawing it. The bar is
    /// built from this and painted from the height this frame really came out at, so the only thing a frame
    /// behind is which pointer the bar took.
    pub page_height: f32,
    /// Which page [`SettingsWindow::scroll`] and [`SettingsWindow::page_height`] are about.
    ///
    /// Opening a different page starts at its top, so this is compared rather than a position being kept per
    /// page: coming back to a page that was left half way down would put somebody somewhere they did not
    /// choose, several pages later.
    pub scrolled: Page,
}

impl SettingsWindow {
    pub fn open(&mut self) {
        self.open = true;
    }
}

/// What the page that was drawn this frame reported.
///
/// One enum rather than a `changed: bool` beside a plugin outcome, because `contents` draws exactly one page a
/// frame and only that page's own controls could have produced anything.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PageOutcome {
    /// Nothing on the page changed this frame.
    #[default]
    Nothing,
    /// An ordinary setting changed, so the window applies it and writes the settings file.
    Changed,
    /// The Plugins page asked to install a plugin, by id.
    Install(String),
    /// The Plugins page asked to uninstall a plugin, by id.
    Uninstall(String),
    /// The Plugins page asked to switch a plugin on or off, by id and the state it asked for.
    SetEnabled(String, bool),
    /// The Appearance page asked for the Background grid. `task-2004`.
    OpenBackgrounds,
    /// The AI Configuration page changed an endpoint, so the window writes the list and tells every pane.
    EndpointsChanged,
}

/// What happened in the Settings window this frame.
///
/// `closed` stays its own field rather than folding into [`PageOutcome`]: closing is asked from the round
/// cross or the modal's own `Escape`, and is independent of whatever the page drew. A plugin action has no
/// safety net besides this outcome, so folding `closed` into the same enum would mean a close arriving on the
/// same frame as an install silently dropped the install.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsOutcome {
    pub closed: bool,
    /// What the page that was showing asked for.
    pub page: PageOutcome,
}

/// What the window tells the Settings dialog about the machine it is running on.
#[derive(Clone, Copy)]
pub struct SettingsContext<'a> {
    pub families: &'a [String],
    pub project: &'a str,
    pub plugins: &'a Plugins,
    pub mcp_running: &'a crate::services::mcp::State,
    /// The client an agent is told to launch. Passed in rather than worked out in the page, because
    /// `current_exe` in the window is `unluminous.exe`, and because a screenshot test has to be able to pin it.
    pub unluminous_cli: &'a std::path::Path,
    pub installed_on_disk: &'a dyn Fn(&str) -> bool,
    pub icon_for: &'a dyn Fn(&str) -> Option<egui::TextureHandle>,
    /// The name each plugin that contributed a page calls it, in slot order. Passed in rather than read from
    /// the plugins, because a page's name is `settings.page` in its manifest and the dialog draws rows rather
    /// than reading manifests.
    pub plugin_pages: &'a [String],
    /// The mark each contributed page is listed with, in slot order: the name of the plugin's own rail mark
    /// (`chat`, `board`, `database`), or empty for a plugin with no pane.
    pub plugin_page_marks: &'a [String],
}

/// Draw the Settings window. Does nothing when it is not open.
pub fn show(
    ctx: &egui::Context,
    state: &mut SettingsWindow,
    settings: &mut Settings,
    context: SettingsContext,
    // The AI endpoints, which the AI Configuration page edits. The window's own, lent for the frame.
    endpoints: &mut crate::services::ai_endpoints::Endpoints,
    // Draws a contributed page, given the page's slot and the rectangle it may be seen in. A closure because
    // only the window can reach a plugin's provider, and because the page has to be drawn **inside** the modal:
    // the modal is an `egui::Area` of its own, so anything painted into the window underneath it is covered.
    plugin_page: &mut dyn FnMut(&mut egui::Ui, usize, Rect),
) -> SettingsOutcome {
    let mut outcome = SettingsOutcome::default();
    if !state.open {
        return outcome;
    }
    let (inner, should_close) =
        modal::show_unframed(ctx, "unluminous-settings", WIDTH, HEIGHT, |ui, area| {
            contents(ui, area, state, settings, context, endpoints, plugin_page)
        });
    outcome.page = inner.page;
    if inner.closed || should_close {
        state.open = false;
        outcome.closed = true;
    }
    outcome
}

/// Where the parts of the dialog go, worked out from the rectangle the modal gives it.
struct Layout {
    nav: Rect,
    /// The breadcrumb's row.
    header: Rect,
    /// Where the page's cards are laid out across, and where the first one starts.
    column: Rect,
    /// Where the page can be seen: the column, reaching up into the gap under the breadcrumb and out to the
    /// dialog's edge so the cards' shadows are not cut.
    view: Rect,
}

fn layout(area: Rect) -> Layout {
    let inner = area.shrink(PAD);
    let nav = Rect::from_min_size(inner.min, Vec2::new(NAV_WIDTH, inner.height()));
    let right = Rect::from_min_max(Pos2::new(nav.right() + NAV_GAP, inner.top()), inner.max);
    let header = Rect::from_min_size(
        Pos2::new(right.left(), right.top() + HEADER_TOP),
        Vec2::new(right.width(), HEADER),
    );
    let top = header.bottom() + HEADER_GAP;
    let column =
        Rect::from_min_max(Pos2::new(right.left(), top), Pos2::new(right.right(), area.bottom()));
    let view = Rect::from_min_max(
        Pos2::new(right.left() - kit::PAGE_MARGIN.left, top - VIEW_ABOVE),
        Pos2::new(area.right(), area.bottom()),
    );
    Layout { nav, header, column, view }
}

fn contents(
    ui: &mut egui::Ui,
    area: Rect,
    state: &mut SettingsWindow,
    settings: &mut Settings,
    context: SettingsContext,
    endpoints: &mut crate::services::ai_endpoints::Endpoints,
    plugin_page: &mut dyn FnMut(&mut egui::Ui, usize, Rect),
) -> SettingsOutcome {
    let mut outcome = SettingsOutcome::default();
    let parts = layout(area);
    // The dialog's surface and the card of pages, in one layer under everything else.
    kit::layer_reaching(ui, "settings-frame", area, modal::SHADOW_REACH, |rux| {
        let chat = rux.theme().chat;
        rux.chrome.surface(area, RADIUS, chat.ground, modal::shadow());
        rux.chrome.surface(
            parts.nav,
            kit::CARD_RADIUS,
            Fill::gradient(chat.card, parts.nav),
            chat.raised(),
        );
    });
    show_list(ui, parts.nav, state, context);
    if header(ui, parts.header, state.page, context.plugin_pages) {
        outcome.closed = true;
    }

    // Opening a different page starts at its top, and forgets how tall the last one was: a bar built from the
    // Editor page's height while the Terminal page is showing would offer to scroll a page that fits.
    if state.scrolled != state.page {
        state.scrolled = state.page;
        state.scroll = 0.0;
        state.page_height = 0.0;
    }

    // A contributed page scrolls itself, inside the room it is given.
    if let Page::Plugin(slot) = state.page {
        let clip = ui.clip_rect();
        ui.set_clip_rect(clip.intersect(parts.view));
        plugin_page(ui, slot as usize, parts.view);
        ui.set_clip_rect(clip);
        if modal::Confirm::Enter.pressed(ui) {
            outcome.closed = true;
        }
        return outcome;
    }

    let view = parts.view.bottom() - parts.column.top();
    let room =
        Rect::from_min_max(parts.column.min, Pos2::new(area.right() - 4.0, area.bottom() - PAD));
    let was = state.scroll;
    // The bar down the right, taken hold of **before** the page is drawn so that it wins the pointer over
    // whatever the page puts underneath it, `components::scrollbar`'s own rule.
    // The bar is measured against the same view the scroll is clamped to. It was measured against `room`,
    // which stops short of the dialog's bottom edge, so a page scrolled to its end left the thumb short of
    // the bottom of its track (`task-2247`).
    let grab = match scrollbar::Bar::new(room, was, state.page_height, view) {
        Some(bar) => scrollbar::grab(ui, &bar, SCROLLBAR),
        None => scrollbar::Grab::default(),
    };
    if let Some(to) = grab.scroll {
        state.scroll = to;
    }
    // A popup open over the page, such as a select's list, keeps its own wheel: `rect_contains_pointer` asks
    // the layer too.
    let wheel = ui.input(|input| input.smooth_scroll_delta.y);
    if wheel != 0.0 && ui.rect_contains_pointer(parts.view) {
        state.scroll -= wheel;
    }
    state.scroll = settle_the_scroll(state.scroll, state.page_height, view);
    let scroll = state.scroll;
    let page_rect = lifted(parts.column, scroll);
    let clip = ui.clip_rect();
    ui.set_clip_rect(clip.intersect(parts.view));
    let drawn = draw_the_page(ui, page_rect, state, settings, context, endpoints);
    ui.set_clip_rect(clip);
    outcome.page = drawn.0;
    let height = drawn.1;
    state.page_height = height;
    if let Some(bar) = scrollbar::Bar::new(room, scroll, height, view) {
        scrollbar::paint(ui, &bar, SCROLLBAR, grab.active || (scroll - was).abs() > 0.01);
    }
    if modal::Confirm::Enter.pressed(ui) {
        outcome.closed = true;
    }
    outcome
}

/// Draw one of Unluminous's own pages down `area`, and answer what it asked for and how tall it came out.
fn draw_the_page(
    ui: &mut egui::Ui,
    area: Rect,
    state: &mut SettingsWindow,
    settings: &mut Settings,
    context: SettingsContext,
    endpoints: &mut crate::services::ai_endpoints::Endpoints,
) -> (PageOutcome, f32) {
    let changed = |drawn: Drawn| match drawn.changed {
        true => PageOutcome::Changed,
        false => PageOutcome::Nothing,
    };
    match state.page {
        Page::Appearance => {
            let page = appearance_page(ui, area, settings, context.families);
            // **After the change**, so a frame on which both happened opens the grid: the window's own before
            // and after comparison writes a change that arrived with it anyway.
            let outcome = match page.backgrounds {
                true => PageOutcome::OpenBackgrounds,
                false => changed(page),
            };
            (outcome, page.height)
        }
        Page::Theme => {
            let page = theme_page(ui, area, settings, context.plugins);
            (changed(page), page.height)
        }
        Page::Editor => {
            let page = editor_page(ui, area, settings, context.families);
            (changed(page), page.height)
        }
        Page::Terminal => {
            let page = terminal_page(ui, area, settings);
            (changed(page), page.height)
        }
        Page::Mcp => {
            let page = mcp_page::show(
                ui,
                area,
                &mut state.mcp,
                settings,
                context.mcp_running,
                context.unluminous_cli,
            );
            let outcome = match page.changed {
                true => PageOutcome::Changed,
                false => PageOutcome::Nothing,
            };
            (outcome, page.height)
        }
        Page::Ai => {
            let page = crate::components::ai_page::show(
                ui,
                area,
                &mut endpoints.providers,
                &mut endpoints.readiness,
            );
            let outcome = match page.changed {
                true => PageOutcome::EndpointsChanged,
                false => PageOutcome::Nothing,
            };
            (outcome, page.height)
        }
        Page::Plugins => {
            let result = plugins_page::show(
                ui,
                area,
                &mut state.plugins,
                context.plugins,
                context.installed_on_disk,
                context.icon_for,
            );
            let outcome = if let Some(id) = result.install {
                PageOutcome::Install(id)
            } else if let Some(id) = result.uninstall {
                PageOutcome::Uninstall(id)
            } else if let Some((id, on)) = result.set_enabled {
                PageOutcome::SetEnabled(id, on)
            } else {
                PageOutcome::Nothing
            };
            (outcome, result.height)
        }
        Page::Plugin(_) => (PageOutcome::Nothing, 0.0),
    }
}

/// The page's group and its name, and the round close button. Answers whether it was pressed.
fn header(ui: &mut egui::Ui, area: Rect, page: Page, plugin_pages: &[String]) -> bool {
    kit::layer(ui, "settings-header", area, |rux| {
        let theme = rux.theme();
        let close = Rect::from_center_size(
            Pos2::new(area.right() - 18.0, area.center().y),
            Vec2::splat(36.0),
        );
        let closed = ChipButton::new(Icon::X, 36.0, "Close settings")
            .mark(12.0, Some(1.8))
            .show(rux, close)
            .clicked();
        let mut x = area.left() + 4.0;
        let group = page.group();
        if !group.is_empty() {
            let galley = rux.text(Style::sans(13.0), group, theme.ink.i400);
            let width = galley.size().x;
            rux::text::draw_left_capitals(
                rux.painter(),
                Pos2::new(x, area.center().y + 1.0),
                galley,
                Style::sans(13.0),
                theme.ink.i400,
            );
            x += width + 6.0;
            let arrow = rux.text(Style::sans(13.0), "\u{203A}", theme.ink.i400);
            let width = arrow.size().x;
            rux::text::draw_left_capitals(
                rux.painter(),
                Pos2::new(x, area.center().y + 1.0),
                arrow,
                Style::sans(13.0),
                theme.ink.i400,
            );
            x += width + 6.0;
        }
        let title = title_of(page, plugin_pages);
        let style = Style::sans(20.0).semibold().tracking(-0.015);
        let galley = rux.text(style, &title, theme.ink.i900);
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(x, area.center().y),
            galley,
            style,
            theme.ink.i900,
        );
        closed
    })
}

/// The window's name, the search well and the list of pages, grouped under their headings, with the line
/// under them that says changes take effect at once.
fn show_list(ui: &mut egui::Ui, area: Rect, state: &mut SettingsWindow, context: SettingsContext) {
    let plugin_pages = context.plugin_pages;
    kit::layer(ui, "settings-nav", area, |rux| {
        let theme = rux.theme();
        let chat = theme.chat;
        let left = area.left() + 12.0;
        let right = area.right() - 12.0;
        let mut pen = area.top() + 14.0 + 2.0;

        // `Settings — unluminous`, the second half quieter.
        let strong = rux.text(Style::sans(13.0).semibold(), "Settings", theme.ink.i900);
        let strong_width = strong.size().x;
        let line = strong.size().y;
        rux.painter().galley(Pos2::new(left + 12.0, pen), strong, theme.ink.i900);
        if !context.project.is_empty() {
            // Cut to the card, because a project's folder name can be longer than the card is wide.
            let quiet = rux.elided(
                Style::sans(13.0),
                &format!(" \u{2014} {}", context.project),
                theme.ink.i400,
                right - left - 24.0 - strong_width,
            );
            rux.painter().galley(Pos2::new(left + 12.0 + strong_width, pen), quiet, theme.ink.i400);
        }
        pen += line + 12.0;

        // The search well.
        let well = Rect::from_min_max(Pos2::new(left, pen), Pos2::new(right, pen + 38.0));
        rux.chrome.surface(well, 19.0, chat.well, chat.carved_sm());
        rux.mark(
            Mark::new(Icon::Search, 14.0).stroke(1.8 * 24.0 / 14.0),
            Pos2::new(well.left() + 21.0, well.center().y),
            theme.ink.i400,
        );
        let font = Style::sans(12.5).font();
        let search_id = rux.ui.id().with("settings-search");
        let text = crate::components::controls::field_takes_the_whole_rectangle_at(
            rux.ui,
            well,
            37.0,
            search_id,
            "Search field",
            &font,
        );
        let mut child = rux.ui.new_child(egui::UiBuilder::new().max_rect(text));
        let response = child.text_box(
            egui::TextEdit::singleline(&mut state.search)
                .id(search_id)
                .hint_text(crate::components::controls::placeholder(
                    "Search settings",
                    &font,
                    theme.ink.i400,
                ))
                .font(font.clone())
                .frame(egui::Frame::NONE)
                .desired_width(text.width())
                .text_color(theme.ink.i900),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Search settings")
        });
        pen = well.bottom() + 8.0 + 3.0;

        // The line at the foot of the card, measured first so the list can stop above it: in a short window,
        // or a zoomed one, the list is cut there rather than drawn over it.
        let note = rux::text::wrapped(
            rux.painter(),
            Style::sans(11.0).leading(1.5),
            "Changes take effect at once.",
            theme.ink.i400,
            right - left - 24.0,
        );
        let note_at = Pos2::new(left + 12.0, area.bottom() - 14.0 - note.size().y);
        let clip = rux.ui.clip_rect();
        rux.ui.set_clip_rect(clip.intersect(Rect::from_min_max(
            Pos2::new(area.left(), pen),
            Pos2::new(area.right(), note_at.y - 8.0),
        )));

        let mut group_drawn: Option<&str> = None;
        let mut any = false;
        let mut first = true;
        for page in Page::all(plugin_pages.len()) {
            let title = title_of(page, plugin_pages);
            if !matches_search(page, &title, &state.search) {
                continue;
            }
            any = true;
            let group = page.group();
            if group.is_empty() {
                // A page with no group of its own sits apart from the group above it.
                if !first {
                    pen += 8.0 + 3.0;
                }
                group_drawn = None;
            } else if group_drawn != Some(group) {
                group_drawn = Some(group);
                pen += match first {
                    true => 7.0,
                    false => 10.0,
                };
                let galley = rux.text(Style::sans(11.0), group, theme.ink.i400);
                let height = galley.size().y;
                rux.painter().galley(Pos2::new(left + 12.0, pen), galley, theme.ink.i400);
                pen += height + 3.0 + 3.0;
            }
            first = false;
            let row = Rect::from_min_max(Pos2::new(left, pen), Pos2::new(right, pen + 34.0));
            if page_row(rux, row, page, &title, state.page == page, context.plugin_page_marks) {
                state.page = page;
            }
            pen = row.bottom() + 3.0;
        }
        if !any {
            let galley = rux.text(Style::sans(11.5), "No setting matches", theme.ink.i400);
            rux.painter().galley(Pos2::new(left + 12.0, pen + 6.0), galley, theme.ink.i400);
        }

        rux.ui.set_clip_rect(clip);
        rux.painter().galley(note_at, note, theme.ink.i400);
    });
}

/// The mark a page is listed with.
fn mark_of(page: Page, plugin_page_marks: &[String]) -> Icon {
    match page {
        Page::Appearance => Icon::Letters,
        Page::Theme => Icon::Palette,
        Page::Editor => Icon::Document,
        Page::Plugins => Icon::Puzzle,
        Page::Terminal => Icon::Terminal,
        Page::Mcp => Icon::Plug,
        Page::Ai => Icon::Spark,
        Page::Plugin(slot) => match plugin_page_marks.get(slot as usize).map(String::as_str) {
            Some("chat") => Icon::Bubble,
            Some("board") => Icon::Board,
            Some("database") => Icon::Database,
            Some("terminal") => Icon::Terminal,
            _ => Icon::Puzzle,
        },
    }
}

/// One page in the list: a pill with its mark and its name, carved into the card while it is showing.
fn page_row(
    rux: &mut rux::Rux<'_>,
    row: Rect,
    page: Page,
    title: &str,
    chosen: bool,
    plugin_page_marks: &[String],
) -> bool {
    let theme = rux.theme();
    let chat = theme.chat;
    let response =
        rux.ui.interact(row, rux.ui.id().with(("settings-page", title.to_owned())), Sense::click());
    if chosen {
        rux.chrome.surface(row, row.height() / 2.0, chat.well, chat.carved_sm());
    } else if response.hovered() {
        rux.wash(row, row.height() / 2.0, theme.ink.i900.gamma_multiply(0.04));
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let ink = match chosen || response.hovered() {
        true => theme.ink.i900,
        false => theme.ink.i500,
    };
    rux.mark(
        Mark::new(mark_of(page, plugin_page_marks), 16.0),
        Pos2::new(row.left() + 20.0, row.center().y),
        ink,
    );
    let galley = rux.text(Style::sans(12.5), title, ink);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(row.left() + 38.0, row.center().y),
        galley,
        Style::sans(12.5),
        ink,
    );
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, chosen, title));
    response.clicked()
}

/// What a page is called in the list.
///
/// Unluminous's own six answer for themselves; a contributed page's name is `settings.page` in its manifest,
/// which is what `plugin_pages` carries.
pub fn title_of(page: Page, plugin_pages: &[String]) -> String {
    match page.plugin_slot().and_then(|slot| plugin_pages.get(slot)) {
        Some(name) => name.clone(),
        None => page.title().to_owned(),
    }
}

/// Whether a page is worth showing for what has been typed in the search box.
///
/// A contributed page is matched on its own name and its group, which is what `Page::matches` does for the
/// other six; it cannot do it here because it does not know the name.
fn matches_search(page: Page, title: &str, search: &str) -> bool {
    let needle = search.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    [title, page.group()].iter().any(|text| text.to_lowercase().contains(&needle))
        || (page.plugin_slot().is_none() && page.matches(search))
}

/// Where a list of values is, and the index of the one that is chosen: the chosen value is put at the front
/// when the list does not hold it, so a select never says nothing about a setting that has a value.
fn options_with(list: Vec<String>, current: &str) -> (Vec<String>, Option<usize>) {
    let mut list = list;
    let at = match list.iter().position(|option| option == current) {
        Some(at) => at,
        None => {
            list.insert(0, current.to_owned());
            0
        }
    };
    (list, Some(at))
}

/// `Appearance & Behavior > Appearance`: the interface's font and size, and the window's background.
///
/// The editor's font is on the Editor page since `task-2280`: *"The font size for the Editor should be in the
/// Editor section of Settings."*
fn appearance_page(
    ui: &mut egui::Ui,
    area: Rect,
    settings: &mut Settings,
    families: &[String],
) -> Drawn {
    let mut changed = false;
    let mut backgrounds = false;
    let mut column = Column::new(area.left(), area.right(), area.top());

    // The reference editor's `Appearance -> Use custom font`: the window's own text was the editor's family at
    // egui's own size, so a large editor meant large menus.
    kit::card(ui, &mut column, "Interface font", None, |card| {
        let shown = match settings.ui_font_family.is_empty() {
            true => "The editor's".to_owned(),
            false => settings.ui_font_family.clone(),
        };
        let mut list = vec!["The editor's".to_owned()];
        list.extend(families.iter().cloned());
        let (options, chosen) = options_with(list, &shown);
        card.row("Family", None, |row| {
            if let Some(index) = row.select("Interface font family", &options, chosen, 260.0) {
                settings.ui_font_family = match options[index].as_str() {
                    "The editor's" => String::new(),
                    family => family.to_owned(),
                };
                changed = true;
            }
        });
        let sizes: Vec<String> = UI_FONT_SIZES.iter().map(|size| format!("{size:.1}")).collect();
        let (sizes, chosen) = options_with(sizes, &format!("{:.1}", settings.ui_font_size));
        card.row(
            "Size",
            Some("Everything in the window grows with it: the menus, the rail, the explorer, the tabs, the status bar and the dialogs. The editor and the terminal keep their own font sizes, set on their own pages."),
            |row| {
                if let Some(index) = row.select("Interface font size", &sizes, chosen, 110.0) {
                    if let Ok(size) = sizes[index].parse::<f32>() {
                        settings.ui_font_size = size;
                        changed = true;
                    }
                }
            },
        );
    });

    kit::card(ui, &mut column, "Background", None, |card| {
        card.row(
            "Opacity",
            Some("Fades the window so what is behind shows through. Text stays fully solid at every setting."),
            |row| {
                changed |= row.slider("Background opacity", &mut settings.opacity, MIN_OPACITY, 1.0);
                row.suffix(&format!("{:.0}%", settings.opacity * 100.0));
            },
        );
        // **What is behind the window**, which is the desktop unless a picture has been chosen, `task-2004`. A
        // chip that opens a grid rather than a list, because what it chooses between is pictures and a picture
        // is chosen by looking at it.
        let showing = match settings.background_image.trim().is_empty() {
            true => "Show Contents Underneath".to_owned(),
            false => settings.background_image.clone(),
        };
        card.row(
            "Behind",
            Some("A picture behind the window, or the desktop. Pictures chosen here are kept in Unluminous's own folder, so moving the original does not lose it."),
            |row| {
                if row.chooser("Background", &showing, 260.0) {
                    backgrounds = true;
                }
            },
        );
    });

    // Here rather than on the Plugins page, which is a list of what is installed. Depth is an appearance choice
    // in exactly the way the opacity above it is.
    kit::card(ui, &mut column, "Plugin panes", None, |card| {
        changed |= card.switch(
            "Draw depth in plugin panes",
            Some("Soft shadows, gradients and pressed edges behind a plugin's own pane, drawn on the processor. Off, a plugin draws flat, which costs nothing at all."),
            &mut settings.plugin_chrome,
        );
    });
    Drawn { backgrounds, ..drawn(area.top(), &column, changed) }
}

/// `Appearance & Behavior > Theme`: which palette the window is painted in, its accent, and its icons.
///
/// Laid out the way the reference editor's own theme list is: a row per theme, its name and where it came from
/// on the left and the colours it is made of on the right, so the choice is made by looking.
fn theme_page(ui: &mut egui::Ui, area: Rect, settings: &mut Settings, plugins: &Plugins) -> Drawn {
    let mut changed = false;
    let mut column = Column::new(area.left(), area.right(), area.top());
    let themes = plugins.themes();
    // Empty means Unluminous's own, which is what the settings file says by saying nothing. Matched on the key
    // rather than remembered, so a theme whose plugin has been switched off leaves the row on Unluminous Dark.
    let chosen = match settings.theme.is_empty() {
        true => crate::theme::Theme::unluminous_dark().key,
        false => settings.theme.clone(),
    };
    kit::card(
        ui,
        &mut column,
        "Theme",
        Some("A theme says what every colour in Unluminous's own palette means. One that names the nine token colours also recolours code, in every language at once, in the same frame."),
        |card| {
            let height = 8.0 * 2.0 + themes.len() as f32 * 40.0 + themes.len().saturating_sub(1) as f32 * 4.0;
            card.custom(height, |rux, rect| {
                kit::list_well(rux, rect, 20.0);
                let theme_ink = rux.theme().ink;
                let mut top = rect.top() + 8.0;
                for theme in &themes {
                    let row = Rect::from_min_size(
                        Pos2::new(rect.left() + 8.0, top),
                        Vec2::new(rect.width() - 16.0, 40.0),
                    );
                    let on = theme.key == chosen;
                    if kit::list_row(rux, row, 20.0, &theme.name, on).clicked() && !on {
                        // Unluminous's own is written as an empty setting rather than as its key, so a settings
                        // file that has never chosen a theme keeps saying nothing.
                        settings.theme = match theme.key == crate::theme::Theme::unluminous_dark().key {
                            true => String::new(),
                            false => theme.key.clone(),
                        };
                        changed = true;
                    }
                    let name = rux.text(Style::sans(12.5), &theme.name, theme_ink.i900);
                    let name_width = name.size().x;
                    rux::text::draw_left_capitals(
                        rux.painter(),
                        Pos2::new(row.left() + 14.0, row.center().y),
                        name,
                        Style::sans(12.5),
                        theme_ink.i900,
                    );
                    let from = match theme.plugin.as_str() {
                        "" | "unluminous" => "built in".to_owned(),
                        plugin => plugin.to_owned(),
                    };
                    let from = rux.text(Style::sans(11.0), &from, theme_ink.i400);
                    rux::text::draw_left_capitals(
                        rux.painter(),
                        Pos2::new(row.left() + 14.0 + name_width + 10.0, row.center().y),
                        from,
                        Style::sans(11.0),
                        theme_ink.i400,
                    );
                    let palette = theme.palette;
                    let colours = [
                        palette.editor,
                        palette.explorer,
                        palette.accent,
                        palette.git_added,
                        palette.unsaved,
                        palette.close,
                    ];
                    for (index, colour) in colours.iter().enumerate() {
                        let centre = Pos2::new(
                            row.right() - 14.0 - 7.0 - (5 - index) as f32 * 24.0,
                            row.center().y,
                        );
                        rux.painter().circle_filled(centre, 7.0, *colour);
                        rux.painter().circle_stroke(
                            centre,
                            6.5,
                            egui::Stroke::new(1.0, theme_ink.i900.gamma_multiply(0.12)),
                        );
                    }
                    top = row.bottom() + 4.0;
                }
            });
        },
    );

    kit::card(ui, &mut column, "Accent", None, |card| {
        let active = crate::theme::active();
        // The theme's own first, then the accents this theme already has. The palette is closed and this does
        // not open it: every swatch is a colour the chosen theme names.
        let offered: Vec<(&str, Option<egui::Color32>)> = vec![
            ("The theme's own", None),
            ("Accent", Some(active.palette.accent)),
            ("Unsaved", Some(active.palette.unsaved)),
            ("Added", Some(active.palette.git_added)),
            ("Modified", Some(active.palette.git_modified)),
            ("Agent", Some(active.palette.agent)),
            ("Blame", Some(active.palette.blame_new)),
            ("Close", Some(active.palette.close)),
        ];
        let wanted = settings.accent_colour();
        let chosen = offered.iter().position(|(_, colour)| match colour {
            None => wanted.is_none(),
            Some(offered) => wanted == Some(*offered),
        });
        let names: Vec<&str> = offered.iter().map(|(name, _)| *name).collect();
        let colours: Vec<(Option<egui::Color32>, egui::Color32)> = offered
            .iter()
            .map(|(_, colour)| (*colour, colour.unwrap_or(active.palette.accent)))
            .collect();
        card.row(
            "Accent",
            Some("One colour for everything the accent means: the caret, the open tab, an open folder and the line a program is stopped on. The first is whatever the theme chose."),
            |row| {
                let at = Pos2::new(row.x, row.top);
                let (picked, rect) = kit::swatches(row.rux, at, &names, &colours, chosen);
                row.x = rect.right() + 10.0;
                if let Some(index) = picked {
                    settings.accent = match offered[index].1 {
                        None => String::new(),
                        Some(colour) => format!("#{:02X}{:02X}{:02X}", colour.r(), colour.g(), colour.b()),
                    };
                    changed = true;
                }
            },
        );
    });

    kit::card(ui, &mut column, "Icons", None, |card| {
        let mut options = vec!["Follow the theme".to_owned()];
        options.extend(crate::theme::IconSet::ALL.iter().map(|set| icon_set_name(*set).to_owned()));
        let chosen = match settings.icon_set() {
            None => 0,
            Some(set) => crate::theme::IconSet::ALL
                .iter()
                .position(|one| *one == set)
                .map(|at| at + 1)
                .unwrap_or(0),
        };
        card.row(
            "Icon set",
            Some("Which drawn marks the rail buttons, the folder arrow and the small controls use. Material draws a chevron where the classic set draws a triangle, and puts a folder in front of a folder's name."),
            |row| {
                if let Some(index) = row.select("Icon set", &options, Some(chosen), 260.0) {
                    settings.icons = match index {
                        0 => String::new(),
                        at => crate::theme::IconSet::ALL[at - 1].name().to_owned(),
                    };
                    changed = true;
                }
            },
        );
    });
    drawn(area.top(), &column, changed)
}

/// What a person reads for an icon set, which is its name with a capital letter.
fn icon_set_name(set: crate::theme::IconSet) -> &'static str {
    match set {
        crate::theme::IconSet::Classic => "Classic",
        crate::theme::IconSet::Material => "Material",
    }
}

/// What an indent is called where a person reads it.
///
/// `Indent::name` is the word the settings file and the command line are written with, `tabs` and
/// `spaces:4`, and this is the row. Two spellings of one value, as `line_ending_name` already is.
fn indent_name(indent: Indent) -> String {
    match indent {
        Indent::Tab => "Tabs".to_owned(),
        Indent::Spaces(width) => format!("{width} spaces"),
    }
}

/// `Editor > Editor`: the gutter, notebooks, suggestions, indentation, the debugger, files and updates.
fn editor_page(
    ui: &mut egui::Ui,
    area: Rect,
    settings: &mut Settings,
    families: &[String],
) -> Drawn {
    let mut changed = false;
    let mut column = Column::new(area.left(), area.right(), area.top());

    // First on the page, because it is the setting about the editor that is changed most (`task-2280`).
    kit::card(ui, &mut column, "Font", None, |card| {
        let shown = match settings.font_family.is_empty() {
            true => "System default".to_owned(),
            false => settings.font_family.clone(),
        };
        let (options, chosen) = options_with(families.to_vec(), &shown);
        card.row("Family", None, |row| {
            if let Some(index) = row.select("Editor font family", &options, chosen, 260.0) {
                if let Some(family) = families.iter().find(|family| **family == options[index]) {
                    settings.font_family = family.clone();
                    changed = true;
                }
            }
        });
        let sizes: Vec<String> = FONT_SIZES.iter().map(|size| format!("{size:.0}")).collect();
        let (sizes, chosen) = options_with(sizes, &format!("{:.0}", settings.font_size));
        card.row(
            "Size",
            Some("The font the editor sets every open file in. Bold, italic and colour stay as they were, and the size is also on the keyboard at Ctrl with plus and minus."),
            |row| {
                if let Some(index) = row.select("Editor font size", &sizes, chosen, 110.0) {
                    if let Ok(size) = sizes[index].parse::<f32>() {
                        settings.font_size = size;
                        changed = true;
                    }
                }
            },
        );
    });

    kit::card(ui, &mut column, "Gutter", None, |card| {
        changed |= card.switch(
            "Show line numbers",
            Some("A number against each line of the file. Unluminous wraps, so a paragraph that runs over several rows is numbered once, against its first row. Right clicking the gutter puts the numbers away and annotates with git blame."),
            &mut settings.line_numbers,
        );
    });
    kit::card(ui, &mut column, "Notebooks", None, |card| {
        changed |= card.switch(
            "Scroll long outputs",
            Some("An output taller than thirty lines scrolls inside its cell. Off, every output is drawn at its full height."),
            &mut settings.notebook_scroll_outputs,
        );
    });
    kit::card(ui, &mut column, "Suggestions", None, |card| {
        // A switch over a two value setting: `automatic` is on and `manual` is off, which is what the wording
        // says. The value is a named pair rather than a flag because the settings file and the command line
        // both spell it out.
        let mut automatic = settings.suggestions.is_automatic();
        if card.switch(
            "Suggest completions as you type",
            Some("A list of names appears under the caret once two letters of a word have been typed, in a file whose language a plugin claims. Off, nothing appears until you ask: Ctrl+Space, or Complete Word on the Edit menu, which work either way."),
            &mut automatic,
        ) {
            settings.suggestions = if automatic { Suggestions::Automatic } else { Suggestions::Manual };
            changed = true;
        }
        // `task-2231` §6.9. The one switch for an editor that runs nothing: off, rust-analyzer and tsserver are
        // never started and completion answers from the project's structure alone.
        let mut servers = settings.servers.is_automatic();
        if card.switch(
            "Ask language servers (rust-analyzer, tsserver)",
            Some("Rust and TypeScript files ask the language server the project already has for the members of a value, the import a name needs and a call's parameters. It costs memory and processor time: rust-analyzer typically holds 1 to 4 GB for a workspace like Unluminous's and runs cargo and rustc for a minute or more while it reads it; tsserver holds a few hundred MB. Off, the default, stops a running server at once and the structural answers stand alone."),
            &mut servers,
        ) {
            settings.servers = match servers {
                true => crate::settings::Servers::Automatic,
                false => crate::settings::Servers::Off,
            };
            changed = true;
        }
    });
    // `task-2279`, `task-2282`. How the code index loads and what it keeps in memory, each with what it costs.
    changed |= code_index_card(ui, &mut column, settings);
    // `task-1922` WP4. What a key types, which is the one thing on this page a person changes because of how
    // they were taught to write code rather than because of what Unluminous does.
    kit::card(ui, &mut column, "Indentation", None, |card| {
        let mut list = vec![Indent::Tab];
        list.extend((Indent::MIN_WIDTH..=Indent::MAX_WIDTH).map(Indent::Spaces));
        let options: Vec<String> = list.iter().map(|indent| indent_name(*indent)).collect();
        let chosen = list.iter().position(|indent| *indent == settings.indent);
        card.row(
            "One indent is",
            Some("What the Tab key types where nothing is selected. Indenting a selection still moves each line by one character, which is what Tab over a selection has always done."),
            |row| {
                if let Some(index) = row.select("One indent is", &options, chosen, 160.0) {
                    settings.indent = list[index];
                    changed = true;
                }
            },
        );
        changed |= card.switch(
            "Indent a new line like the one above it",
            Some("Pressing Enter starts the new line with the whitespace the line it was started from begins with."),
            &mut settings.auto_indent,
        );
        changed |= card.switch(
            "Trim trailing whitespace on save",
            Some("Off by default, and never on a Markdown file, where two spaces at the end of a line are a line break."),
            &mut settings.trim_on_save,
        );
    });
    kit::card(ui, &mut column, "Debugger", None, |card| {
        // `manual` is already the off switch, because Show Value and the command line work either way.
        let mut automatic = settings.value_tooltip.is_automatic();
        if card.switch(
            "Show value tooltip",
            Some("While a program is stopped, resting the pointer on a name shows what it holds, and a structure opens into its fields, which can be typed over. Off, nothing appears until you ask: Show Value on the Debug menu."),
            &mut automatic,
        ) {
            settings.value_tooltip = if automatic { ValueTooltip::Automatic } else { ValueTooltip::Manual };
            changed = true;
        }
        // `debug.lldb` and `debug.node`, walked from the registry rather than named twice, so a third adapter
        // needs no change here. Empty means what `services::debuggers` already looks for on its own.
        let adapters = crate::services::plugins::DEBUGGERS;
        for (index, name) in adapters.iter().enumerate() {
            let last = index + 1 == adapters.len();
            let help = last.then_some(
                "Leave empty to search the usual places; the debug tile offers to install one when none is found.",
            );
            let current = settings.debug_adapter(name).unwrap_or_default().to_owned();
            let mut edited = current.clone();
            card.row(&format!("{name} path"), help, |row| {
                row.code_text(&format!("{name} adapter path"), &mut edited, "", 300.0);
            });
            if edited != current {
                settings.set_debug_adapter(name, edited);
                changed = true;
            }
        }
    });
    // `task-2255`: *"a browser agent header config in settings, that allows me to spoof being different
    // browsers"*. Which browser a page is told it is talking to, from the `User-Agent` header.
    kit::card(ui, &mut column, "Web pages", None, |card| {
        let list = crate::settings::BrowserAgent::ALL;
        let options: Vec<String> = list.iter().map(|agent| agent.label().to_owned()).collect();
        let chosen = list.iter().position(|agent| *agent == settings.browser_agent);
        card.row(
            "Identify as",
            Some("The browser a page is told it is talking to, sent as the User-Agent header with every request. Default is the engine Unluminous draws pages with: Edge's WebView2 on Windows and Safari's WebKit on macOS."),
            |row| {
                if let Some(index) = row.select("Identify as", &options, chosen, 160.0) {
                    settings.browser_agent = list[index];
                    changed = true;
                }
            },
        );
    });
    // `task-1804` §7.1. What a file is written back with, and what the index leaves out.
    kit::card(ui, &mut column, "Files", None, |card| {
        let list = [LineEndings::Keep, LineEndings::Lf, LineEndings::Crlf];
        let options: Vec<String> =
            list.iter().map(|one| line_ending_name(*one).to_owned()).collect();
        let chosen = list.iter().position(|one| *one == settings.line_endings);
        card.row(
            "Line endings",
            Some("What a file is written back with. Kept as it was found is what leaves a one character edit as a one line diff; the other two bring every file that is saved into line."),
            |row| {
                if let Some(index) = row.select("Line endings", &options, chosen, 240.0) {
                    settings.line_endings = list[index];
                    changed = true;
                }
            },
        );
        // Compared rather than taken from the field's own `changed`, for the shell's reason: a field reports a
        // change on every letter and this reloads the project index.
        let before = settings.exclude.clone();
        card.row(
            "Exclude",
            Some("Patterns Go to File, Find in Files, completion and Go to Definition leave out, separated by commas, written the way a .gitignore line is.\nThe project's own .gitignore is read already; this is a list beside it. The explorer goes on showing everything either way."),
            |row| {
                row.code_text("Exclude", &mut settings.exclude, "", 300.0);
            },
        );
        if settings.exclude != before {
            changed = true;
        }
    });
    // `task-2063`: once a day by default, and the switch is how it stops. See `UpdateCheck`.
    kit::card(ui, &mut column, "Updates", None, |card| {
        let mut on = settings.update_check.is_on();
        if card.switch(
            "Check for a newer version once a day",
            Some("One request to unluminous.com a day, shared by every window. A newer version is offered in a notice with Install & Restart and Don't Ask Again. Off, Unluminous sends nothing until you ask: Check for Updates on the Unluminous menu works either way."),
            &mut on,
        ) {
            settings.update_check = if on { UpdateCheck::Daily } else { UpdateCheck::Off };
            changed = true;
        }
    });
    // `task-2271`: on by default, and this is how it stops. See `services::telemetry`.
    kit::card(ui, &mut column, "Usage statistics", None, |card| {
        if card.switch(
            "Send anonymous usage statistics",
            Some("When a window is open and for how long, which panels and kinds of tab are showing, and how often each menu entry and command is used, sent to Google Analytics about once every twenty minutes while you work. Never a path, a file name, a project name or anything you type. Off, nothing is counted or sent."),
            &mut settings.telemetry_enabled,
        ) {
            changed = true;
        }
    });
    drawn(area.top(), &column, changed)
}

/// What the Line endings list calls each value.
///
/// Words rather than [`LineEndings::name`]'s `keep`, `lf` and `crlf`, because those are what the settings file
/// and the command line spell and this is what a person reads.
fn line_ending_name(endings: LineEndings) -> &'static str {
    match endings {
        LineEndings::Keep => "Keep what the file has",
        LineEndings::Lf => "Always LF (Unix)",
        LineEndings::Crlf => "Always CRLF (Windows)",
    }
}

/// `Tools > Terminal`.
fn terminal_page(ui: &mut egui::Ui, area: Rect, settings: &mut Settings) -> Drawn {
    let mut changed = false;
    let mut column = Column::new(area.left(), area.right(), area.top());
    kit::card(ui, &mut column, "Font", None, |card| {
        let sizes: Vec<String> =
            TERMINAL_FONT_SIZES.iter().map(|size| format!("{size:.0}")).collect();
        let (sizes, chosen) = options_with(sizes, &format!("{:.0}", settings.terminal_font_size));
        card.row(
            "Size",
            Some("The size of one cell in the terminal grid. Changing it tells the running program the new size."),
            |row| {
                if let Some(index) = row.select("Terminal font size", &sizes, chosen, 110.0) {
                    if let Ok(size) = sizes[index].parse::<f32>() {
                        settings.terminal_font_size = size;
                        changed = true;
                    }
                }
            },
        );
    });
    kit::card(ui, &mut column, "Shell", None, |card| {
        // Compared rather than taken from the field's own `changed`, because a field reports a change on every
        // letter and this is written to disk: what matters is that the setting is not what it was.
        let before = settings.terminal_shell.clone();
        let help = format!(
            "The program each tab runs, started in the folder the explorer is showing. Leave it empty for {}.",
            default_shell_name()
        );
        card.row("Program", Some(&help), |row| {
            row.code_text(
                "Terminal shell",
                &mut settings.terminal_shell,
                default_shell_hint(),
                240.0,
            );
        });
        if settings.terminal_shell != before {
            changed = true;
        }
    });
    kit::card(ui, &mut column, "Where a tab reopens", None, |card| {
        changed |= card.switch(
            "Ask PowerShell where it is",
            Some("A tab reopens in the folder its shell was in, read off the shell's own process. That answers for cmd.exe, bash and zsh and cannot answer for PowerShell: Set-Location moves PowerShell's location and never the process's current directory.\nOn, Unluminous adds one line to the prompt, after your own profile has set it up, so the shell says where it is. Off, nothing about your shell is changed. A shell that already reports its folder is followed either way."),
            &mut settings.shell_integration,
        );
    });
    drawn(area.top(), &column, changed)
}

/// What an empty shell setting means on this machine, in words, so the help under the field says the name a
/// person would type rather than `$SHELL`.
fn default_shell_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "PowerShell"
    } else {
        "the shell named in $SHELL"
    }
}

/// What the empty shell field says.
fn default_shell_hint() -> &'static str {
    if cfg!(target_os = "windows") {
        "PowerShell"
    } else {
        "$SHELL"
    }
}

/// The Code index card: whether the embedding model runs, how long it stays loaded and on how many
/// threads, how long a plain English question's search index is kept, how much of the index file is kept
/// in memory, and whether file contents are. Each says what it costs, because each is memory or processor
/// time against speed (`task-2279`, `task-2282`). Answers whether anything changed.
///
/// @param ui - the page
/// @param column - where the card goes
/// @param settings - the settings being edited
fn code_index_card(ui: &mut egui::Ui, column: &mut Column, settings: &mut Settings) -> bool {
    let mut changed = false;
    kit::card(ui, column, "Code index", Some("The index behind Find in Files, Go to File, completion and the agents' searches. These trade memory and processor time for speed. The defaults keep the window small; the choices below each say what they cost. Except where a row says otherwise, a change applies to windows opened after it."), |card| {
        changed |= card.switch(
            "Rank plain English searches by meaning",
            Some("The code index embeds every passage of the project with an embedding model, so a question in plain English finds code that answers it in other words. It costs memory and processor time: the model holds about 1.1 GB while it is loaded, and the first time a project is opened every passage is embedded on two processors, which takes several minutes for a project the size of Unluminous. Off, the default, stops embedding at once, returns the model's memory ten seconds later, and plain English searches are ranked by their words. Completion, Go to Definition, references and text searches never use it."),
            &mut settings.search_embeddings,
        );
        let options: Vec<String> = ModelKept::ALL.iter().map(|kept| kept.title().to_owned()).collect();
        let chosen = ModelKept::ALL.iter().position(|kept| *kept == settings.model_kept);
        card.row(
            "Keep the embedding model loaded",
            Some("Only while ranking by meaning is on. Loading the model takes most of a second and it holds about 1.1 GB once loaded. 10 seconds, the default, gives the memory back soon after a burst of searches. 5 minutes or Always answers the next search sooner and keeps the 1.1 GB for that long. Only while it is working holds the least memory and loads the model again for every batch and every question."),
            |row| {
                if let Some(index) = row.select("Keep the embedding model loaded", &options, chosen, 230.0) {
                    settings.model_kept = ModelKept::ALL[index];
                    changed = true;
                }
            },
        );
        changed |= preset_row(
            card,
            "Embedding threads",
            "How many processor threads the model embeds on. 2, the default, keeps the machine responsive while a new project is embedded. More finishes sooner and keeps more of the processor busy until it does.",
            &mut settings.search_embedding_threads,
            &[(1.0, "1"), (2.0, "2"), (4.0, "4"), (8.0, "8")],
        );
        changed |= preset_row(
            card,
            "Keep a question's search index",
            "A plain English search builds a search index of the project's passages, about 100 MB for a project the size of Unluminous. 20 seconds, the default, answers a burst of questions from one build and then gives the memory back. Longer answers a later question sooner and holds the memory for longer; not at all holds nothing and builds it again for every question.",
            &mut settings.search_question_index_seconds,
            &[(0.0, "Not at all"), (20.0, "20 seconds"), (120.0, "2 minutes"), (600.0, "10 minutes"), (3600.0, "1 hour")],
        );
        changed |= preset_row(
            card,
            "Index file kept in memory",
            "How much of the index file stays in memory once the index is built. 4 MB, the default, is enough: the operating system keeps the rest of the file in its own cache, so a larger cache costs its size in memory and makes searches little faster.",
            &mut settings.search_page_cache_mb,
            &[(4.0, "4 MB"), (16.0, "16 MB"), (32.0, "32 MB"), (128.0, "128 MB")],
        );
        changed |= card.switch(
            "Keep file contents in memory",
            Some("Off, the default, a search reads each file it might match from the disk, the way ripgrep does, and a project the size of Unluminous holds about 11 MB less. On saves that read, which helps an agent asking many questions a second. Applies the next time a window opens the project."),
            &mut settings.search_keep_contents,
        );
    });
    changed
}

/// A row that chooses a number from a short list, each with the words the list shows. A value set some
/// other way, such as `settings set`, is listed as itself so the row still shows what is in effect.
/// Answers whether the value changed.
///
/// @param card - the card the row goes in
/// @param label - the row's name
/// @param help - what each choice costs
/// @param value - the setting
/// @param presets - each value and its words
fn preset_row(
    card: &mut kit::Card<'_, '_>,
    label: &str,
    help: &str,
    value: &mut f32,
    presets: &[(f32, &str)],
) -> bool {
    let mut choices: Vec<(f32, String)> =
        presets.iter().map(|(number, words)| (*number, (*words).to_owned())).collect();
    if !choices.iter().any(|(number, _)| (*number - *value).abs() < 0.5) {
        choices.push((*value, format!("{:.0}", value)));
    }
    let options: Vec<String> = choices.iter().map(|(_, words)| words.clone()).collect();
    let chosen = choices.iter().position(|(number, _)| (*number - *value).abs() < 0.5);
    let mut changed = false;
    card.row(label, Some(help), |row| {
        if let Some(index) = row.select(label, &options, chosen, 160.0) {
            changed = (choices[index].0 - *value).abs() >= 0.5;
            *value = choices[index].0;
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rectangle the modal gives `contents`, at the size the dialog asks for.
    fn dialog() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(WIDTH, HEIGHT))
    }

    /// A context with egui's own faces bound under the names `rux` sets its words in, which is what the window
    /// does with its real faces. A page drawn with no faces bound panics on the first word.
    fn fonted() -> egui::Context {
        let egui = egui::Context::default();
        crate::theme::install_fonts(&egui, "", None, None);
        egui
    }

    /// How tall the room a page is seen in is.
    fn view_height() -> f32 {
        let parts = layout(dialog());
        parts.view.bottom() - parts.column.top()
    }

    /// Draw the whole dialog on one page and report how tall that page came out, along with everything that
    /// was painted.
    ///
    /// It draws twice, because the bar is built from the height the page came out at last frame: the first
    /// pass is what measures the page and the second is what a person is looking at.
    fn draw(page: Page, scroll: f32) -> (f32, Vec<egui::epaint::ClippedShape>) {
        let area = dialog();
        let plugins = crate::services::plugins::Plugins::load(None).0;
        let mut settings = Settings::default();
        let mut state = SettingsWindow { open: true, page, ..SettingsWindow::default() };
        // So that the MCP page does not read the person's own agent configuration to find out what its
        // buttons should say. A test must not read the settings of whoever is running it.
        state.mcp.read = true;
        let running = crate::services::mcp::State::Off;
        let program = std::path::PathBuf::from("unluminous-cli");
        let on_disk = |_: &str| false;
        let icon_for = |_: &str| None;
        let context = SettingsContext {
            families: &[],
            project: "",
            plugins: &plugins,
            mcp_running: &running,
            unluminous_cli: &program,
            installed_on_disk: &on_disk,
            icon_for: &icon_for,
            plugin_pages: &[],
            plugin_page_marks: &[],
        };
        let egui = fonted();
        let mut shapes = Vec::new();
        for pass in 0..2 {
            if pass == 1 {
                state.scroll = scroll;
            }
            let output = egui.run_ui(egui::RawInput::default(), |ui| {
                contents(
                    ui,
                    area,
                    &mut state,
                    &mut settings,
                    context,
                    &mut Default::default(),
                    &mut |_, _, _| {},
                );
            });
            shapes = output.shapes.clone();
            output.drop_without_applying_deltas();
        }
        (state.page_height, shapes)
    }

    /// **How tall each page really is**, printed rather than written down, so that the next row added to one
    /// can be measured rather than guessed at.
    ///
    /// `cargo test -p unluminous-app --lib how_tall_every_page_is -- --nocapture` prints the table.
    #[test]
    fn how_tall_every_page_is() {
        println!("the room is {} points", view_height());
        for page in Page::ALL {
            let (height, _) = draw(page, 0.0);
            let over = height - view_height();
            println!(
                "{:<11} {height:>7.1}  {}",
                page.title(),
                if over > 0.5 {
                    format!("over by {over:.1}")
                } else {
                    format!("fits, {:.1} to spare", -over)
                }
            );
            assert!(height.is_finite() && height > 0.0, "{} measured {height}", page.title());
        }
    }

    /// **A page that fits cannot be scrolled**, which is what leaves every page that fits drawn exactly where
    /// it was.
    #[test]
    fn a_page_that_fits_cannot_be_scrolled() {
        assert_eq!(settle_the_scroll(200.0, 400.0, 582.0), 0.0);
        assert_eq!(settle_the_scroll(200.0, 582.0, 582.0), 0.0);
        assert_eq!(lifted(dialog(), 0.0), dialog());
        assert_eq!(scrollbar::Bar::new(dialog(), 0.0, 400.0, 582.0), None);
    }

    /// A page taller than the room scrolls exactly as far as the part that cannot be seen, and no further in
    /// either direction.
    #[test]
    fn a_page_that_does_not_fit_scrolls_to_its_end_and_no_further() {
        assert_eq!(settle_the_scroll(-40.0, 700.0, 582.0), 0.0);
        assert_eq!(settle_the_scroll(5000.0, 700.0, 582.0), 118.0);
        let area = Rect::from_min_size(Pos2::new(0.0, 100.0), Vec2::new(600.0, 582.0));
        assert_eq!(lifted(area, 118.0).top(), -18.0);
        assert!(scrollbar::Bar::new(area, 0.0, 700.0, 582.0).is_some());
    }

    /// **Opening a different page starts at its top.** A page left half way down and come back to would put
    /// somebody somewhere they did not choose, several pages later.
    #[test]
    fn opening_a_different_page_starts_at_its_top() {
        let area = dialog();
        let plugins = crate::services::plugins::Plugins::load(None).0;
        let mut settings = Settings::default();
        let mut state =
            SettingsWindow { open: true, page: Page::Editor, ..SettingsWindow::default() };
        state.mcp.read = true;
        let running = crate::services::mcp::State::Off;
        let program = std::path::PathBuf::from("unluminous-cli");
        let on_disk = |_: &str| false;
        let icon_for = |_: &str| None;
        let context = SettingsContext {
            families: &[],
            project: "",
            plugins: &plugins,
            mcp_running: &running,
            unluminous_cli: &program,
            installed_on_disk: &on_disk,
            icon_for: &icon_for,
            plugin_pages: &[],
            plugin_page_marks: &[],
        };
        let egui = fonted();
        let mut once = |state: &mut SettingsWindow| {
            let output = egui.run_ui(egui::RawInput::default(), |ui| {
                contents(
                    ui,
                    area,
                    state,
                    &mut settings,
                    context,
                    &mut Default::default(),
                    &mut |_, _, _| {},
                );
            });
            output.drop_without_applying_deltas();
        };
        once(&mut state);
        state.scroll = 60.0;
        once(&mut state);
        assert!(state.scroll > 0.0, "the Editor page should have somewhere to scroll to");
        state.page = Page::Terminal;
        once(&mut state);
        assert_eq!(state.scroll, 0.0);
    }

    /// **Nothing the dialog draws lands outside the dialog.** This is the fault the scrolling page was built
    /// for: before it, the Editor page ran past the body and its last tick box was painted over the window
    /// below the footer, where no picture of the dialog could show it.
    ///
    /// Checked on the Editor page at the top and at the end of its scroll, because the two put different parts
    /// of it against the two edges. The dialog's own shadow is the one thing allowed past its edge.
    #[test]
    fn nothing_a_page_draws_lands_outside_the_dialog() {
        let area = dialog();
        for scroll in [0.0, 10_000.0] {
            let (_, shapes) = draw(Page::Editor, scroll);
            assert!(!shapes.is_empty(), "nothing was drawn, so this measures nothing");
            for shape in &shapes {
                let clip = shape.clip_rect;
                if !clip.is_positive() {
                    continue;
                }
                if matches!(shape.shape, egui::Shape::Mesh(_)) {
                    // A `rux` layer's decoration, which carries the dialog's shadow past its edge.
                    continue;
                }
                let painted = shape.shape.visual_bounding_rect().intersect(clip);
                if !painted.is_positive() {
                    continue;
                }
                assert!(
                    area.expand(1.0).contains_rect(painted),
                    "something was painted at {painted:?}, outside the {area:?} the dialog has",
                );
            }
        }
    }
}
