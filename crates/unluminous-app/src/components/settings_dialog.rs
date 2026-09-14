//! The Settings window, opened from `Edit -> Settings`.
//!
//! It is laid out the way `tasks/img.png` shows the reference editor's: a search box and a list of pages down the
//! left grouped under headings, a breadcrumb across the top of the right hand side saying where you are,
//! and the chosen page's sections under it. It is a modal, so the rest of the window is dimmed and does
//! not take clicks while it is open, which is what `tasks/improvements.md` asks for.
//!
//! Every change takes effect as it is made rather than when the window is closed, so there is one button
//! and it says `Close`. A dialog with `Apply` has to hold a second copy of every setting and decide what
//! to do when the two disagree; showing the change straight away needs neither.

use egui::{CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use crate::components::controls;
use crate::components::mcp_page::{self, McpState};
use crate::components::modal;
use crate::components::plugins_page::{self, PluginsState};
use crate::components::scrollbar;
use crate::services::plugins::Plugins;
use crate::settings::{
    Indent, LineEndings, Page, Settings, Suggestions, UpdateCheck, ValueTooltip, FONT_SIZES,
    MIN_OPACITY, TERMINAL_FONT_SIZES, UI_FONT_SIZES,
};
use crate::theme::{color, icon, size};

/// How large the window is, before it is shrunk to fit a small Unluminous window.
///
/// It grew by eighty points when `task-1679` added the MCP page, and by forty more when `task-1776`
/// added an Interface section to Appearance. Each of those was the same move: a page had outgrown the
/// window, so the window was made taller. `task-1922` is where that stopped, because it was measured
/// and it had already failed — the Editor page ran past the body in the accepted pictures and its
/// `Check for a newer version at startup` tick box was drawn outside the dialog altogether, over the
/// window below the footer, and nobody noticed. Forty more points would have moved the same fault to
/// whichever page the next row is added to.
///
/// So the window is still one size for every page — a dialog that changed height as its list was
/// walked would jump under the pointer — and the **page** scrolls instead. A page that fits shows no
/// bar and is drawn exactly where it was; a page that does not is cut to the body and can be scrolled
/// down it. `modal::fit` still shrinks the whole thing to whatever room a small Unluminous window has,
/// and a page that no longer fits the shrunken body now scrolls rather than being cut off.
const WIDTH: f32 = 900.0;
const HEIGHT: f32 = 680.0;
/// How wide the list of pages is.
const LIST_WIDTH: f32 = 258.0;
const HEADER: f32 = 46.0;
const FOOTER: f32 = 52.0;

/// How much room is left under the last thing on a page.
///
/// A page whose last line of explanation ended exactly on the body's bottom edge would read as a page
/// that had been cut off, whether or not it had been.
const TAIL: f32 = 16.0;
/// What the page's scrollbar is called. `components::scrollbar` puts `Scroll` in front of it, so the
/// control's name is `Scroll settings`, which nothing else in the window answers to.
const SCROLLBAR: &str = "settings";

/// What a page reported when it was drawn: whether a setting changed, and how tall the page is.
///
/// The height is what decides whether there is anything to scroll. It cannot be worked out in advance
/// — a page is a pen running down the rectangle it was given, and where the pen stopped is only known
/// once the last thing has been drawn — so it is reported back rather than measured a second way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Drawn {
    /// Whether a setting on the page changed this frame.
    pub changed: bool,
    /// How tall the page needs to be, from the top of the page area to [`TAIL`] under its last thing.
    pub height: f32,
}

/// What a page reports, from where its pen stopped.
pub(crate) fn drawn(area: Rect, pen: f32, changed: bool) -> Drawn {
    Drawn { changed, height: pen - area.top() + TAIL }
}

/// The rectangle a page measures itself from, which is the page area lifted by however far it is
/// scrolled.
///
/// Every page positions what it draws from `area.left()` across and `area.top()` down, so lifting the
/// rectangle moves the whole page and no page has to know that it is being scrolled. What keeps it
/// inside the body is the clip rather than the rectangle: `Painter::with_clip_rect` intersects, so a
/// page's own `ui.painter_at(area)` is cut to the body by the clip the dialog set before it drew, and
/// `Ui::interact` cuts a control's interact rectangle the same way.
pub(crate) fn lifted(page_area: Rect, scroll: f32) -> Rect {
    page_area.translate(Vec2::new(0.0, -scroll))
}

/// How far a page really is scrolled, given how tall it came out and how tall the body is.
///
/// A page that fits cannot be scrolled at all, which is what leaves every page that already fitted
/// drawn exactly where it was.
pub(crate) fn settle_the_scroll(asked: f32, height: f32, view: f32) -> f32 {
    asked.clamp(0.0, (height - view).max(0.0))
}

/// Which page is showing, and what has been typed in the search box. Lives in the window's state, so it
/// is still there when the settings are opened again.
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
    /// Kept from one frame to the next because a page's height can only be answered by drawing it: a
    /// page is a pen running down the rectangle it was given, and where the pen stopped is not known
    /// until the last thing has been drawn. The bar is built from this and painted from the height
    /// this frame really came out at, so the only thing a frame behind is which pointer the bar took.
    pub page_height: f32,
    /// Which page [`SettingsWindow::scroll`] and [`SettingsWindow::page_height`] are about.
    ///
    /// Opening a different page starts at its top, so this is compared rather than a position being
    /// kept per page: coming back to a page that was left half way down would put somebody somewhere
    /// they did not choose, several pages later.
    pub scrolled: Page,
}

impl SettingsWindow {
    pub fn open(&mut self) {
        self.open = true;
    }
}

/// What the page that was drawn this frame reported.
///
/// One enum rather than a `changed: bool` beside a `plugins: PluginsOutcome`, because `contents` draws
/// exactly one page a frame and only that page's own controls could have produced anything: an
/// ordinary setting changing and the Plugins page asking to install something cannot both be true of
/// one frame, so they are alternatives of one value rather than two fields that would need to agree.
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
}

/// What happened in the Settings window this frame.
///
/// `closed` stays its own field rather than folding into [`PageOutcome`]: closing is asked from the
/// footer or the window's own corner cross, answered by the modal's own `Escape` and drag handling,
/// and is genuinely independent of whatever the page drew -- a person can close the window from any
/// page, having changed nothing on it or having just changed something. A settings change has a
/// safety net besides this outcome (the window's own caller also compares the settings before and
/// after), a plugin action has none, so folding `closed` into the same enum would mean a `Closed`
/// arriving on the same frame as an `Install` silently drops the install with nothing to catch it.
/// Kept apart, neither can be lost.
///
/// There is no field for which contributed page slot was drawn any more. `contents` already draws it
/// through the `plugin_page` closure `show` is given -- that is where the real work happens -- and
/// nothing outside this module ever read the `Option<(usize, Rect)>` this struct used to carry for
/// it. A field nothing reaches is not a state worth keeping.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsOutcome {
    pub closed: bool,
    /// What the page that was showing asked for.
    pub page: PageOutcome,
}

/// What the window tells the Settings dialog about the machine it is running on.
///
/// `show` and `contents` share almost this whole list, and every field here is read rather than
/// written — what changes as the dialog runs is `SettingsWindow` and `Settings`, both taken
/// separately.
#[derive(Clone, Copy)]
pub struct SettingsContext<'a> {
    pub families: &'a [String],
    pub project: &'a str,
    pub plugins: &'a Plugins,
    pub mcp_running: &'a crate::services::mcp::State,
    /// The client an agent is told to launch. It is passed in rather than worked out in the page,
    /// because `current_exe` in the window is `unluminous.exe`, and because a screenshot test has to
    /// be able to pin it: a picture holding this machine's own path is a picture no other machine can
    /// match.
    pub unluminous_cli: &'a std::path::Path,
    pub installed_on_disk: &'a dyn Fn(&str) -> bool,
    pub icon_for: &'a dyn Fn(&str) -> Option<egui::TextureHandle>,
    /// The name each plugin that contributed a page calls it, in slot order. Passed in rather than
    /// read from the plugins, because a page's name is `settings.page` in its manifest and the
    /// dialog draws rows rather than reading manifests.
    pub plugin_pages: &'a [String],
}

/// Draw the Settings window. Does nothing when it is not open.
pub fn show(
    ctx: &egui::Context,
    state: &mut SettingsWindow,
    settings: &mut Settings,
    context: SettingsContext,
    // Draws a contributed page, given the page's slot and the rectangle every page gets. A closure
    // because only the window can reach a plugin's provider, and because the page has to be drawn
    // **inside** the modal: the modal is an `egui::Area` of its own, so anything painted into the window
    // underneath it is covered by its own background.
    plugin_page: &mut dyn FnMut(&mut egui::Ui, usize, Rect),
) -> SettingsOutcome {
    let mut outcome = SettingsOutcome::default();
    if !state.open {
        return outcome;
    }

    // The window is drawn into one rectangle, the way every other part of Unluminous is, rather than
    // through egui's own layout, so the columns line up with the design. `components::modal` is what
    // decides how large that rectangle is and where it sits, which is also what makes the Settings
    // window draggable and resizable along with every other modal.
    let (inner, should_close) =
        modal::show(ctx, "unluminous-settings", WIDTH, HEIGHT, |ui, area| {
            contents(ui, area, state, settings, context, plugin_page)
        });

    outcome.page = inner.page;
    if inner.closed || should_close {
        state.open = false;
        outcome.closed = true;
    }
    outcome
}

fn contents(
    ui: &mut egui::Ui,
    area: Rect,
    state: &mut SettingsWindow,
    settings: &mut Settings,
    context: SettingsContext,
    plugin_page: &mut dyn FnMut(&mut egui::Ui, usize, Rect),
) -> SettingsOutcome {
    let mut outcome = SettingsOutcome::default();

    // The heading, which names the project the way the reference editor's does.
    let header = Rect::from_min_size(area.min, Vec2::new(area.width(), HEADER));
    let painter = ui.painter_at(area);
    painter.rect_filled(header, CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 }, color::title_bar());
    let title = if context.project.is_empty() {
        "Settings".to_owned()
    } else {
        format!("Settings \u{2014} {}", context.project)
    };
    let galley =
        painter.layout_no_wrap(title, egui::FontId::proportional(13.0), color::text_strong());
    painter.galley(
        Pos2::new(area.left() + 20.0, header.center().y - galley.size().y / 2.0),
        galley,
        color::text_strong(),
    );
    let close = Rect::from_center_size(
        Pos2::new(area.right() - 24.0, header.center().y),
        Vec2::splat(22.0),
    );
    if controls::icon_button(ui, close, "Close settings", icon::cross) {
        outcome.closed = true;
    }
    line(ui, Pos2::new(header.left(), header.bottom()), Pos2::new(header.right(), header.bottom()));

    let body = Rect::from_min_max(
        Pos2::new(area.left(), header.bottom()),
        Pos2::new(area.right(), area.bottom() - FOOTER),
    );
    let list = Rect::from_min_size(body.min, Vec2::new(LIST_WIDTH, body.height()));
    let page_area = Rect::from_min_max(Pos2::new(list.right(), body.top()), body.max);
    ui.painter_at(area).rect_filled(list, CornerRadius::ZERO, color::explorer_footer());
    line(ui, Pos2::new(list.right(), list.top()), Pos2::new(list.right(), list.bottom()));

    show_list(ui, list, state, context.plugin_pages);

    // Opening a different page starts at its top, and forgets how tall the last one was: a bar built
    // from the Editor page's height while the Terminal page is showing would offer to scroll a page
    // that fits.
    if state.scrolled != state.page {
        state.scrolled = state.page;
        state.scroll = 0.0;
        state.page_height = 0.0;
    }
    let view = page_area.height();
    let was = state.scroll;
    // The bar down the right, taken hold of **before** the page is drawn so that it wins the pointer
    // over whatever the page puts underneath it -- `components::scrollbar`'s own rule. It is built
    // from the height the page came out at last frame, which is the only height there is until this
    // one has been drawn; the bar painted at the end of this function is built from the height this
    // frame really came out at.
    let grab = match scrollbar::Bar::new(page_area, was, state.page_height, view) {
        Some(bar) => scrollbar::grab(ui, &bar, SCROLLBAR),
        None => scrollbar::Grab::default(),
    };
    if let Some(to) = grab.scroll {
        state.scroll = to;
    }
    // `rect_contains_pointer` rather than a widget of its own: a widget over the whole page would be a
    // control with nothing to do and no honest name, and the pointer is the only thing being asked
    // about. It asks the layer too, so a dropdown's popup open over the page keeps its own wheel.
    let wheel = ui.input(|input| input.smooth_scroll_delta.y);
    if wheel != 0.0 && ui.rect_contains_pointer(page_area) {
        state.scroll -= wheel;
    }
    state.scroll = settle_the_scroll(state.scroll, state.page_height, view);
    let scroll = state.scroll;
    let page_rect = lifted(page_area, scroll);
    // The clip is what keeps a scrolled page inside the body, and it is put back afterwards so the
    // footer under it is drawn with the clip the dialog had. Saved and restored rather than drawn into
    // a child `Ui`, because a child has an id of its own and every control on every page is named from
    // the id of the `Ui` it is drawn into: a page drawn into a child would be the same controls under
    // different ids, for nothing.
    let clip = ui.clip_rect();
    ui.set_clip_rect(clip.intersect(page_area));
    let height = match state.page {
        Page::Appearance => {
            let page = appearance_page(ui, page_rect, settings, context.families);
            if page.changed {
                outcome.page = PageOutcome::Changed;
            }
            page.height
        }
        Page::Theme => {
            let page = theme_page(ui, page_rect, settings, context.plugins);
            if page.changed {
                outcome.page = PageOutcome::Changed;
            }
            page.height
        }
        Page::Editor => {
            let page = editor_page(ui, page_rect, settings);
            if page.changed {
                outcome.page = PageOutcome::Changed;
            }
            page.height
        }
        Page::Plugins => {
            let result = plugins_page::show(
                ui,
                page_rect,
                &mut state.plugins,
                context.plugins,
                context.installed_on_disk,
                context.icon_for,
            );
            outcome.page = if let Some(id) = result.install {
                PageOutcome::Install(id)
            } else if let Some(id) = result.uninstall {
                PageOutcome::Uninstall(id)
            } else if let Some((id, on)) = result.set_enabled {
                PageOutcome::SetEnabled(id, on)
            } else {
                PageOutcome::Nothing
            };
            result.height
        }
        Page::Terminal => {
            let page = terminal_page(ui, page_rect, settings);
            if page.changed {
                outcome.page = PageOutcome::Changed;
            }
            page.height
        }
        Page::Mcp => {
            let page = mcp_page::show(
                ui,
                page_rect,
                &mut state.mcp,
                settings,
                context.mcp_running,
                context.unluminous_cli,
            );
            if page.changed {
                outcome.page = PageOutcome::Changed;
            }
            page.height
        }
        // A contributed page is drawn by its own plugin. The window hands over a closure that can reach
        // the provider, and it is called here rather than after this function returns, because the modal
        // is an area of its own and anything painted into the window underneath it is covered. Nothing
        // to report back through `outcome` -- the drawing already happened.
        //
        // It reports the body's own height, so it is never scrolled and the rectangle it is handed is
        // the one it always was. What a provider does about a page taller than the room it has is the
        // provider's, which is the line `UiProvider::zoomed` already draws about a pane's scrolling.
        Page::Plugin(slot) => {
            plugin_page(ui, slot as usize, page_rect);
            view
        }
    };
    ui.set_clip_rect(clip);
    state.page_height = height;
    // Drawn last, at the position the frame settled on rather than the one it opened with, which is
    // what `app::preview` already does with the same bar.
    if let Some(bar) = scrollbar::Bar::new(page_area, scroll, height, view) {
        scrollbar::paint(ui, &bar, SCROLLBAR, grab.active || (scroll - was).abs() > 0.01);
    }

    // The footer, holding the one button.
    let footer = Rect::from_min_max(Pos2::new(area.left(), body.bottom()), area.max);
    line(ui, Pos2::new(footer.left(), footer.top()), Pos2::new(footer.right(), footer.top()));
    let button = Rect::from_min_size(
        Pos2::new(footer.right() - 20.0 - 96.0, footer.center().y - 14.0),
        Vec2::new(96.0, 28.0),
    );
    // Named `Done` rather than `Close`, because the window's own close button is called Close and two
    // controls with one name cannot be told apart, by a person reading them out or by a test.
    // Enter is `Done`, as it is in every other modal. `components::modal::footer` is where that is
    // decided for the ones built from it; the Settings window draws its own footer, so it asks the
    // same question rather than answering it a second way.
    if wide_button(ui, button, "Done") || modal::Confirm::Enter.pressed(ui) {
        outcome.closed = true;
    }
    let note = ui.painter_at(area).layout_no_wrap(
        "Changes take effect at once.".to_owned(),
        egui::FontId::proportional(11.0),
        color::text_faint(),
    );
    ui.painter_at(area).galley(
        Pos2::new(footer.left() + 20.0, footer.center().y - note.size().y / 2.0),
        note,
        color::text_faint(),
    );

    outcome
}

/// The search box and the list of pages, grouped under their headings.
fn show_list(ui: &mut egui::Ui, area: Rect, state: &mut SettingsWindow, plugin_pages: &[String]) {
    let search = Rect::from_min_size(
        Pos2::new(area.left() + 12.0, area.top() + 12.0),
        Vec2::new(area.width() - 24.0, 26.0),
    );
    let painter = ui.painter_at(area);
    painter.rect(
        search,
        CornerRadius::same(size::CONTROL_CORNER),
        color::field(),
        Stroke::new(1.0, color::divider()),
        egui::StrokeKind::Inside,
    );
    icon::magnifier(
        &painter,
        Pos2::new(search.left() + 13.0, search.center().y),
        color::text_faint(),
    );
    let search_id = ui.id().with("settings-search");
    let text_rect =
        crate::components::controls::field_takes_the_whole_rectangle(ui, search, 26.0, search_id);
    let mut field = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
    field.add(
        egui::TextEdit::singleline(&mut state.search)
            .id(search_id)
            .hint_text(egui::RichText::new("Search settings").color(color::text_faint()))
            .frame(egui::Frame::NONE)
            .desired_width(text_rect.width())
            .text_color(color::text_control()),
    );

    let mut pen = search.bottom() + 14.0;
    let mut group_drawn: Option<&str> = None;
    let mut any = false;
    for page in Page::all(plugin_pages.len()) {
        let title = title_of(page, plugin_pages);
        if !matches_search(page, &title, &state.search) {
            continue;
        }
        any = true;
        if !page.group().is_empty() && group_drawn != Some(page.group()) {
            group_drawn = Some(page.group());
            let row = Rect::from_min_size(
                Pos2::new(area.left(), pen),
                Vec2::new(area.width(), size::ROW),
            );
            icon::disclosure(
                &ui.painter_at(area),
                Pos2::new(row.left() + 18.0, row.center().y),
                true,
                color::text_dim(),
            );
            let galley = ui.painter_at(area).layout_no_wrap(
                page.group().to_owned(),
                egui::FontId::proportional(12.5),
                color::text_control(),
            );
            ui.painter_at(area).galley(
                Pos2::new(row.left() + 30.0, row.center().y - galley.size().y / 2.0),
                galley,
                color::text_control(),
            );
            pen += size::ROW;
        }
        let row =
            Rect::from_min_size(Pos2::new(area.left(), pen), Vec2::new(area.width(), size::ROW));
        // A page with no group of its own is not indented under one.
        let indent = if page.group().is_empty() { 16.0 } else { 46.0 };
        if page_row(ui, row, &title, state.page == page, indent) {
            state.page = page;
        }
        pen += size::ROW;
    }
    if !any {
        let galley = ui.painter_at(area).layout_no_wrap(
            "No setting matches".to_owned(),
            egui::FontId::proportional(11.5),
            color::text_faint(),
        );
        ui.painter_at(area).galley(
            Pos2::new(area.left() + 30.0, pen + 4.0),
            galley,
            color::text_faint(),
        );
    }
}

/// One page in the list. The chosen one is drawn as a filled row, the way the open file is in the
/// explorer, so the two lists in the application look like each other.
/// What a page is called in the list.
///
/// Unluminous's own five answer for themselves; a contributed page's name is `settings.page` in its manifest,
/// which is what `plugin_pages` carries.
pub fn title_of(page: Page, plugin_pages: &[String]) -> String {
    match page.plugin_slot().and_then(|slot| plugin_pages.get(slot)) {
        Some(name) => name.clone(),
        None => page.title().to_owned(),
    }
}

/// Whether a page is worth showing for what has been typed in the search box.
///
/// A contributed page is matched on its own name and its group, which is what `Page::matches` does for
/// the other five; it cannot do it here because it does not know the name.
fn matches_search(page: Page, title: &str, search: &str) -> bool {
    let needle = search.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    [title, page.group()].iter().any(|text| text.to_lowercase().contains(&needle))
}

fn page_row(ui: &mut egui::Ui, row: Rect, title: &str, chosen: bool, indent: f32) -> bool {
    let response =
        ui.interact(row, ui.id().with(("settings-page", title.to_owned())), Sense::click());
    let pill = row.shrink2(Vec2::new(8.0, 1.0));
    if chosen {
        controls::pill(ui.painter(), pill, 5);
    } else if response.hovered() {
        ui.painter().rect_filled(pill, CornerRadius::same(5), color::control());
    }
    let tint = if chosen { color::text_strong() } else { color::text_control() };
    let galley =
        ui.painter().layout_no_wrap(title.to_owned(), egui::FontId::proportional(12.5), tint);
    ui.painter().galley(
        Pos2::new(row.left() + indent, row.center().y - galley.size().y / 2.0),
        galley,
        tint,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), chosen, title)
    });
    response.clicked()
}

/// `Appearance & Behavior > Appearance`: the editor's font and the window's background.
fn appearance_page(
    ui: &mut egui::Ui,
    area: Rect,
    settings: &mut Settings,
    families: &[String],
) -> Drawn {
    let mut changed = false;
    let mut pen = breadcrumb(ui, area, Page::Appearance);

    pen = section(ui, area, pen, "Font");
    let font_row = row_at(area, pen);
    label(ui, area, font_row, "Family:");
    let family = if settings.font_family.is_empty() {
        "System default".to_owned()
    } else {
        settings.font_family.clone()
    };
    if let Some(chosen) = controls::dropdown(
        ui,
        Rect::from_min_size(Pos2::new(area.left() + 130.0, font_row.top()), Vec2::new(240.0, 28.0)),
        &family,
        "Editor font family",
        None,
        |ui| {
            let mut chosen = None;
            for family in families {
                if ui.selectable_label(*family == settings.font_family, family).clicked() {
                    chosen = Some(family.clone());
                }
            }
            chosen
        },
    ) {
        settings.font_family = chosen;
        changed = true;
    }
    pen += 38.0;

    let size_row = row_at(area, pen);
    label(ui, area, size_row, "Size:");
    if let Some(chosen) = controls::dropdown(
        ui,
        Rect::from_min_size(Pos2::new(area.left() + 130.0, size_row.top()), Vec2::new(96.0, 28.0)),
        &format!("{:.0}", settings.font_size),
        "Editor font size",
        None,
        |ui| {
            let mut chosen = None;
            for option in FONT_SIZES {
                let selected = (settings.font_size - option).abs() < 0.01;
                if ui.selectable_label(selected, format!("{option:.0}")).clicked() {
                    chosen = Some(*option);
                }
            }
            chosen
        },
    ) {
        settings.font_size = chosen;
        changed = true;
    }
    pen += 34.0;
    pen = note(
        ui,
        area,
        pen,
        "The font the editor sets every open file in. Bold, italic and colour stay as they were,",
    );
    pen = note(
        ui,
        area,
        pen,
        "and the size is also on the keyboard at command or control with plus and minus.",
    );

    // The reference editor's `Appearance -> Use custom font`, which Unluminous had no equivalent of: the window's own text
    // was the editor's family at egui's own size, so a large editor meant large menus and there was no way
    // to ask for a compact window round a big document.
    pen = section(ui, area, pen + 10.0, "Interface");
    let ui_font_row = row_at(area, pen);
    label(ui, area, ui_font_row, "Family:");
    let ui_family = if settings.ui_font_family.is_empty() {
        "The editor's".to_owned()
    } else {
        settings.ui_font_family.clone()
    };
    if let Some(chosen) = controls::dropdown(
        ui,
        Rect::from_min_size(
            Pos2::new(area.left() + 130.0, ui_font_row.top()),
            Vec2::new(240.0, 28.0),
        ),
        &ui_family,
        "Interface font family",
        None,
        |ui| {
            let mut chosen = None;
            if ui.selectable_label(settings.ui_font_family.is_empty(), "The editor's").clicked() {
                chosen = Some(String::new());
            }
            for family in families {
                if ui.selectable_label(*family == settings.ui_font_family, family).clicked() {
                    chosen = Some(family.clone());
                }
            }
            chosen
        },
    ) {
        settings.ui_font_family = chosen;
        changed = true;
    }
    pen += 34.0;
    let ui_size_row = row_at(area, pen);
    label(ui, area, ui_size_row, "Size:");
    if let Some(chosen) = controls::dropdown(
        ui,
        Rect::from_min_size(
            Pos2::new(area.left() + 130.0, ui_size_row.top()),
            Vec2::new(96.0, 28.0),
        ),
        &format!("{:.1}", settings.ui_font_size),
        "Interface font size",
        None,
        |ui| {
            let mut chosen = None;
            for option in UI_FONT_SIZES {
                let selected = (settings.ui_font_size - option).abs() < 0.01;
                if ui.selectable_label(selected, format!("{option:.1}")).clicked() {
                    chosen = Some(*option);
                }
            }
            chosen
        },
    ) {
        settings.ui_font_size = chosen;
        changed = true;
    }
    pen += 34.0;
    pen = note(
        ui,
        area,
        pen,
        "The menus, the rail, the explorer and the status bar. The editing area keeps its own font above.",
    );

    pen = section(ui, area, pen + 10.0, "Background");
    let opacity_row = row_at(area, pen);
    label(ui, area, opacity_row, "Opacity:");
    let slider_rect = Rect::from_min_size(
        Pos2::new(area.left() + 130.0, opacity_row.top() + 2.0),
        Vec2::new(300.0, 24.0),
    );
    let mut slider_ui = ui.new_child(egui::UiBuilder::new().max_rect(slider_rect));
    slider_ui.spacing_mut().slider_width = 220.0;
    let percent = format!("{:.0}%", settings.opacity * 100.0);
    let response = slider_ui.add(
        egui::Slider::new(&mut settings.opacity, MIN_OPACITY..=1.0).show_value(false).text(percent),
    );
    // The slider's own accessible name is the number, so it is named here for a test to find it.
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, "Background opacity")
    });
    changed |= response.changed();
    pen += 34.0;
    note(
        ui,
        area,
        pen,
        "Fades the window so the desktop shows through. Text stays fully solid at every setting.",
    );
    pen += 44.0;

    // Here rather than on the Plugins page, which is a list of what is installed and has no settings behind
    // it. Depth is an appearance choice in exactly the way the opacity above it is.
    pen = section(ui, area, pen, "Plugin panes");
    let row = row_at(area, pen);
    let mut decorated = settings.plugin_chrome;
    if checkbox(ui, row, "Draw depth in plugin panes", &mut decorated) {
        settings.plugin_chrome = decorated;
        changed = true;
    }
    pen += 32.0;
    pen = note(
        ui,
        area,
        pen,
        "Soft shadows, gradients and pressed edges behind a plugin's own pane, drawn on the processor. Off, a plugin draws flat, which costs nothing at all.",
    );
    drawn(area, pen, changed)
}

/// `Appearance & Behavior > Theme`: which palette the window is painted in, its accent, and its icons.
///
/// A page of its own rather than a section on Appearance, because Appearance already fills the body on
/// its own — see `Page::Theme`. It is laid out the way the reference editor's own theme list is: a row
/// per theme, its name on the left and the colours it is made of on the right, so the choice can be
/// made by looking rather than by choosing a name and then seeing what happened. It is also the page
/// that grows with what is installed, since every theme a plugin carries is a row here, which is why
/// it can run past the body however tall the window is made.
fn theme_page(
    ui: &mut egui::Ui,
    area: Rect,
    settings: &mut Settings,
    plugins: &crate::services::plugins::Plugins,
) -> Drawn {
    let mut changed = false;
    let mut pen = breadcrumb(ui, area, Page::Theme);

    pen = section(ui, area, pen, "Theme");
    let themes = plugins.themes();
    // Empty means Unluminous's own, which is what the settings file says by saying nothing. Matched on the key
    // rather than remembered, so a theme whose plugin has been switched off leaves the row on Unluminous Dark
    // rather than on a name that is no longer in the list.
    let chosen = match settings.theme.is_empty() {
        true => crate::theme::Theme::unluminous_dark().key,
        false => settings.theme.clone(),
    };
    for theme in &themes {
        let row = row_at(area, pen);
        let on = theme.key == chosen;
        if theme_row(ui, row, &theme.name, &theme.plugin, theme.palette, on) && !on {
            // Unluminous's own is written as an empty setting rather than as its key, so a settings file that
            // has never chosen a theme keeps saying nothing — `terminal_shell`'s rule.
            settings.theme = match theme.key == crate::theme::Theme::unluminous_dark().key {
                true => String::new(),
                false => theme.key.clone(),
            };
            changed = true;
        }
        pen += 30.0;
    }
    pen = note(
        ui,
        area,
        pen + 6.0,
        "A theme says what every colour in Unluminous's own palette means. One that names the nine token colours also recolours code, in every language at once, in the same frame.",
    );

    pen = section(ui, area, pen + 6.0, "Accent");
    let accent_row = row_at(area, pen);
    label(ui, area, accent_row, "Accent:");
    let active = crate::theme::active();
    // The theme's own first, then the accents this theme already has. The palette is closed and this does
    // not open it: every swatch here is a colour the chosen theme names, so an accent can never be a
    // forty-first colour that nothing else in the window is drawn in.
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
    for (index, (name, colour)) in offered.iter().enumerate() {
        let at = Pos2::new(area.left() + 130.0 + index as f32 * 30.0, accent_row.center().y);
        let shown = colour.unwrap_or(active.palette.accent);
        let on = match colour {
            None => wanted.is_none(),
            Some(offered) => wanted == Some(*offered),
        };
        if swatch_button(ui, at, shown, name, on, colour.is_none()) {
            settings.accent = match colour {
                None => String::new(),
                Some(colour) => format!("#{:02X}{:02X}{:02X}", colour.r(), colour.g(), colour.b()),
            };
            changed = true;
        }
    }
    pen += 34.0;
    pen = note(
        ui,
        area,
        pen,
        "One colour for everything the accent means: the caret, the open tab, an open folder and the line a program is stopped on. The first is whatever the theme chose.",
    );

    pen = section(ui, area, pen + 6.0, "Icons");
    let icons_row = row_at(area, pen);
    label(ui, area, icons_row, "Icon set:");
    let shown = match settings.icon_set() {
        None => "Follow the theme".to_owned(),
        Some(set) => icon_set_name(set).to_owned(),
    };
    if let Some(picked) = controls::dropdown(
        ui,
        Rect::from_min_size(
            Pos2::new(area.left() + 130.0, icons_row.top()),
            Vec2::new(200.0, 28.0),
        ),
        &shown,
        "Icon set",
        None,
        |ui| {
            let mut picked = None;
            if ui.selectable_label(settings.icons.is_empty(), "Follow the theme").clicked() {
                picked = Some(String::new());
            }
            for set in crate::theme::IconSet::ALL {
                if ui
                    .selectable_label(settings.icon_set() == Some(set), icon_set_name(set))
                    .clicked()
                {
                    picked = Some(set.name().to_owned());
                }
            }
            picked
        },
    ) {
        settings.icons = picked;
        changed = true;
    }
    pen += 34.0;
    pen = note(
        ui,
        area,
        pen,
        "Which drawn marks the rail buttons, the folder arrow and the small controls use. Material draws a chevron where the classic set draws a triangle, and puts a folder in front of a folder's name.",
    );
    drawn(area, pen, changed)
}

/// What a person reads for an icon set, which is its name with a capital letter.
fn icon_set_name(set: crate::theme::IconSet) -> &'static str {
    match set {
        crate::theme::IconSet::Classic => "Classic",
        crate::theme::IconSet::Material => "Material",
    }
}

/// One theme in the list: the pill every list in Unluminous draws for its chosen row, the name, where it came
/// from, and the six colours the theme is most recognisable by.
fn theme_row(
    ui: &mut egui::Ui,
    row: Rect,
    name: &str,
    plugin: &str,
    palette: crate::theme::Palette,
    chosen: bool,
) -> bool {
    let response = ui.interact(row, ui.id().with(("theme", name)), Sense::click());
    let painter = ui.painter_at(row);
    if chosen {
        controls::pill(&painter, row, 5);
    } else if response.hovered() {
        painter.rect_filled(row, CornerRadius::same(5), color::control());
    }
    let tint = if chosen { color::text_strong() } else { color::text_control() };
    let galley = painter.layout_no_wrap(name.to_owned(), egui::FontId::proportional(12.5), tint);
    painter.galley(
        Pos2::new(row.left() + 10.0, row.center().y - galley.size().y / 2.0),
        galley.clone(),
        tint,
    );
    // Where it came from, in the faintest colour, so a bundle's five and Unluminous's own are told apart.
    let from = painter.layout_no_wrap(
        plugin.to_owned(),
        egui::FontId::proportional(11.0),
        color::text_faint(),
    );
    painter.galley(
        Pos2::new(row.left() + 20.0 + galley.size().x, row.center().y - from.size().y / 2.0),
        from,
        color::text_faint(),
    );
    // The colours it is made of, right aligned, so the list can be read as a set of palettes.
    let swatches = [
        palette.editor,
        palette.explorer,
        palette.accent,
        palette.git_added,
        palette.unsaved,
        palette.close,
    ];
    for (index, colour) in swatches.iter().enumerate() {
        let at = Rect::from_center_size(
            Pos2::new(row.right() - 14.0 - (5 - index) as f32 * 18.0, row.center().y),
            Vec2::splat(14.0),
        );
        painter.rect(
            at,
            CornerRadius::same(3),
            *colour,
            Stroke::new(1.0, color::control_border()),
            egui::StrokeKind::Inside,
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), chosen, name)
    });
    response.clicked()
}

/// One accent swatch: a disc of the colour, ringed when it is the one in use.
///
/// `follow` draws the disc hollow, because "the theme's own" is not a colour of its own — it is whichever
/// colour the row above it landed on, and a filled disc would claim to be a seventh choice.
fn swatch_button(
    ui: &mut egui::Ui,
    centre: Pos2,
    colour: egui::Color32,
    name: &str,
    chosen: bool,
    follow: bool,
) -> bool {
    let area = Rect::from_center_size(centre, Vec2::splat(24.0));
    let response =
        ui.interact(area, ui.id().with(("accent", name)), Sense::click()).on_hover_text(name);
    let painter = ui.painter_at(area);
    if follow {
        painter.circle_stroke(centre, 8.0, Stroke::new(2.0, colour));
    } else {
        painter.circle_filled(centre, 8.0, colour);
    }
    if chosen {
        painter.circle_stroke(centre, 11.0, Stroke::new(1.5, color::text_strong()));
    } else if response.hovered() {
        painter.circle_stroke(centre, 11.0, Stroke::new(1.0, color::text_faint()));
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), chosen, name)
    });
    response.clicked()
}

/// `Editor > Editor`: what the gutter down the left of the editing area shows, and whether
/// completions arrive unasked.
/// What an indent is called where a person reads it.
///
/// `Indent::name` is the word the settings file and the command line are written with -- `tabs`,
/// `spaces:4` -- and this is the row. Two spellings of one value, as `line_ending_name` already is.
fn indent_name(indent: Indent) -> String {
    match indent {
        Indent::Tab => "Tabs".to_owned(),
        Indent::Spaces(width) => format!("{width} spaces"),
    }
}

fn editor_page(ui: &mut egui::Ui, area: Rect, settings: &mut Settings) -> Drawn {
    let mut changed = false;
    let mut pen = breadcrumb(ui, area, Page::Editor);
    pen = section(ui, area, pen, "Gutter");
    let row = row_at(area, pen);
    changed |= checkbox(ui, row, "Show line numbers", &mut settings.line_numbers);
    pen += 32.0;
    note(
        ui,
        area,
        pen,
        "A number against each line of the file. Unluminous wraps, so a paragraph that runs over several rows is numbered once, against its first row. Right clicking the gutter puts the numbers away and annotates with git blame.",
    );
    pen += 44.0;
    pen = section(ui, area, pen, "Suggestions");
    // A tick box over a two-value setting: `automatic` is ticked and `manual` is not, which is what
    // the wording says. The value itself is a named pair rather than a flag because the settings
    // file and the command line both spell it out, and because a third value would be a change to
    // the pair rather than to the meaning of a `true`.
    let row = row_at(area, pen);
    let mut automatic = settings.suggestions.is_automatic();
    if checkbox(ui, row, "Suggest completions as you type", &mut automatic) {
        settings.suggestions = if automatic { Suggestions::Automatic } else { Suggestions::Manual };
        changed = true;
    }
    pen += 32.0;
    note(
        ui,
        area,
        pen,
        "A list of names appears under the caret once two letters of a word have been typed, in a file whose language a plugin claims. Off, nothing appears until you ask: Ctrl+Space, or Complete Word on the Edit menu, which work either way.",
    );
    pen += 44.0;
    // `task-1922` WP4. Three rows about what a key types, which is the one thing on this page a
    // person changes because of how they were taught to write code rather than because of what
    // Unluminous does.
    pen = section(ui, area, pen, "Indentation");
    let indent_row = row_at(area, pen);
    label(ui, area, indent_row, "One indent is:");
    if let Some(chosen) = controls::dropdown(
        ui,
        Rect::from_min_size(
            Pos2::new(area.left() + 130.0, indent_row.top()),
            Vec2::new(180.0, 28.0),
        ),
        &indent_name(settings.indent),
        "One indent is",
        None,
        |ui| {
            let mut chosen = None;
            let widths = (Indent::MIN_WIDTH..=Indent::MAX_WIDTH).map(Indent::Spaces);
            for option in std::iter::once(Indent::Tab).chain(widths) {
                let selected = settings.indent == option;
                if ui.selectable_label(selected, indent_name(option)).clicked() {
                    chosen = Some(option);
                }
            }
            chosen
        },
    ) {
        settings.indent = chosen;
        changed = true;
    }
    pen += 34.0;
    pen = note(
        ui,
        area,
        pen,
        "What the Tab key types where nothing is selected. Indenting a selection still moves each line by one character, which is what Tab over a selection has always done.",
    );
    let row = row_at(area, pen);
    changed |=
        checkbox(ui, row, "Indent a new line like the one above it", &mut settings.auto_indent);
    pen += 32.0;
    pen = note(
        ui,
        area,
        pen,
        "Pressing Enter starts the new line with the whitespace the line it was started from begins with.",
    );
    let row = row_at(area, pen);
    changed |= checkbox(ui, row, "Trim trailing whitespace on save", &mut settings.trim_on_save);
    pen += 32.0;
    pen = note(
        ui,
        area,
        pen,
        "Off by default, and never on a Markdown file, where two spaces at the end of a line are a line break.",
    );
    pen += 12.0;
    pen = section(ui, area, pen, "Debugger");
    // The same shape as the pair above, for the same reason: `manual` is already the off switch,
    // because Show Value and the command line work either way.
    let row = row_at(area, pen);
    let mut automatic = settings.value_tooltip.is_automatic();
    if checkbox(ui, row, "Show value tooltip", &mut automatic) {
        settings.value_tooltip =
            if automatic { ValueTooltip::Automatic } else { ValueTooltip::Manual };
        changed = true;
    }
    pen += 32.0;
    pen = note(
        ui,
        area,
        pen,
        "While a program is stopped, resting the pointer on a name shows what it holds, and a structure opens into its fields, which can be typed over. Off, nothing appears until you ask: Show Value on the Debug menu.",
    );

    // `debug.lldb` / `debug.node`, walked from the registry rather than named twice, so a third
    // adapter needs no change here. Empty means what `services::debuggers` already looks for on its
    // own; a path here is only for the machine that keeps one somewhere that search would not find.
    for name in crate::services::plugins::DEBUGGERS {
        let row = row_at(area, pen);
        label(ui, area, row, &format!("{name} path:"));
        let current = settings.debug_adapter(name).unwrap_or_default().to_owned();
        let mut edited = current.clone();
        modal::field(
            ui,
            Rect::from_min_size(Pos2::new(area.left() + 130.0, row.top()), Vec2::new(300.0, 28.0)),
            &format!("{name} adapter path"),
            &mut edited,
        );
        if edited != current {
            settings.set_debug_adapter(name, edited);
            changed = true;
        }
        pen += 34.0;
    }
    pen = note(
        ui,
        area,
        pen,
        "Leave empty to search the usual places; the debug tile offers to install one when none is found.",
    );

    // `task-1804` §7.1. What a file is written back with, and what the index leaves out.
    pen = section(ui, area, pen + 12.0, "Files");
    let ending_row = row_at(area, pen);
    label(ui, area, ending_row, "Line endings:");
    if let Some(chosen) = controls::dropdown(
        ui,
        Rect::from_min_size(
            Pos2::new(area.left() + 130.0, ending_row.top()),
            Vec2::new(180.0, 28.0),
        ),
        line_ending_name(settings.line_endings),
        "Line endings",
        None,
        |ui| {
            let mut chosen = None;
            for option in [LineEndings::Keep, LineEndings::Lf, LineEndings::Crlf] {
                let selected = settings.line_endings == option;
                if ui.selectable_label(selected, line_ending_name(option)).clicked() {
                    chosen = Some(option);
                }
            }
            chosen
        },
    ) {
        settings.line_endings = chosen;
        changed = true;
    }
    pen += 34.0;
    pen = note(
        ui,
        area,
        pen,
        "What a file is written back with. Kept as it was found is what leaves a one character edit as a one line diff; the other two bring every file that is saved into line.",
    );

    let exclude_row = row_at(area, pen + 8.0);
    label(ui, area, exclude_row, "Exclude:");
    let before = settings.exclude.clone();
    modal::field(
        ui,
        Rect::from_min_size(
            Pos2::new(area.left() + 130.0, exclude_row.top()),
            Vec2::new(300.0, 28.0),
        ),
        "Exclude",
        &mut settings.exclude,
    );
    // Compared rather than taken from the field's own `changed`, for `terminal_shell`'s reason: a
    // field reports a change on every letter and this reloads the project index.
    if settings.exclude != before {
        changed = true;
    }
    pen += 34.0;
    pen = note(
        ui,
        area,
        pen,
        "Patterns Go to File, Find in Files, completion and Go to Definition leave out, separated by commas, written the way a .gitignore line is.",
    );
    pen = note(
        ui,
        area,
        pen + 8.0,
        "The project's own .gitignore is read already; this is a list beside it. The explorer goes on showing everything either way.",
    );

    // `task-1804` §6. Off, and the tick box is how it stops being off -- see `UpdateCheck`.
    pen = section(ui, area, pen + 12.0, "Updates");
    let row = row_at(area, pen);
    let mut at_start = settings.update_check.at_start();
    if checkbox(ui, row, "Check for a newer version at startup", &mut at_start) {
        settings.update_check = if at_start { UpdateCheck::Start } else { UpdateCheck::Off };
        changed = true;
    }
    pen += 32.0;
    pen = note(
        ui,
        area,
        pen,
        "One request to the releases page as the window opens. Off, Unluminous sends nothing at all until you ask: Check for Updates on the Unluminous menu works either way, and nothing is ever installed for you.",
    );
    drawn(area, pen, changed)
}

/// What the Line endings dropdown calls each value.
///
/// Words rather than [`LineEndings::name`]'s `keep`/`lf`/`crlf`, because those are what the settings
/// file and the command line spell and this is what a person reads.
fn line_ending_name(endings: LineEndings) -> &'static str {
    match endings {
        LineEndings::Keep => "Keep what the file has",
        LineEndings::Lf => "Always LF (Unix)",
        LineEndings::Crlf => "Always CRLF (Windows)",
    }
}

/// A tick box with its label to the right of it, drawn the way every other control here is.
pub(crate) fn checkbox(ui: &mut egui::Ui, row: Rect, name: &str, value: &mut bool) -> bool {
    let box_rect =
        Rect::from_min_size(Pos2::new(row.left(), row.center().y - 8.0), Vec2::splat(16.0));
    let response = ui.interact(row, ui.id().with(("settings-check", name)), Sense::click());
    let painter = ui.painter();
    painter.rect(
        box_rect,
        CornerRadius::same(3),
        if *value { color::accent() } else { color::field() },
        Stroke::new(1.0, if *value { color::accent() } else { color::control_border() }),
        egui::StrokeKind::Inside,
    );
    if *value {
        icon::tick(painter, box_rect.center(), color::text_strong());
    }
    let galley = painter.layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(12.5),
        color::text_control(),
    );
    painter.galley(
        Pos2::new(box_rect.right() + 10.0, row.center().y - galley.size().y / 2.0),
        galley,
        color::text_control(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, name)
    });
    if response.clicked() {
        *value = !*value;
        return true;
    }
    false
}

/// `Tools > Terminal`.
fn terminal_page(ui: &mut egui::Ui, area: Rect, settings: &mut Settings) -> Drawn {
    let mut changed = false;
    let mut pen = breadcrumb(ui, area, Page::Terminal);
    pen = section(ui, area, pen, "Font");
    let size_row = row_at(area, pen);
    label(ui, area, size_row, "Size:");
    if let Some(chosen) = controls::dropdown(
        ui,
        Rect::from_min_size(Pos2::new(area.left() + 130.0, size_row.top()), Vec2::new(96.0, 28.0)),
        &format!("{:.0}", settings.terminal_font_size),
        "Terminal font size",
        None,
        |ui| {
            let mut chosen = None;
            for option in TERMINAL_FONT_SIZES {
                let selected = (settings.terminal_font_size - option).abs() < 0.01;
                if ui.selectable_label(selected, format!("{option:.0}")).clicked() {
                    chosen = Some(*option);
                }
            }
            chosen
        },
    ) {
        settings.terminal_font_size = chosen;
        changed = true;
    }
    pen += 34.0;
    pen = note(
        ui,
        area,
        pen,
        "The size of one cell in the terminal grid. Changing it tells the running program the new size.",
    );

    pen = section(ui, area, pen + 12.0, "Shell");
    let shell_row = row_at(area, pen);
    label(ui, area, shell_row, "Program:");
    let before = settings.terminal_shell.clone();
    modal::field(
        ui,
        Rect::from_min_size(
            Pos2::new(area.left() + 130.0, shell_row.top()),
            Vec2::new(240.0, 28.0),
        ),
        "Terminal shell",
        &mut settings.terminal_shell,
    );
    // Compared rather than taken from the field's own `changed`, because a field reports a change on
    // every letter and this is written to disk: what matters is that the setting is not what it was.
    if settings.terminal_shell != before {
        changed = true;
    }
    pen += 34.0;
    pen = note(
        ui,
        area,
        pen,
        "The program each tab runs, started in the folder the explorer is showing.",
    );
    // A note is one line and is not wrapped, so what an empty field means is a note of its own rather
    // than a longer sentence that would run off the end of the page.
    pen = note(ui, area, pen + 8.0, &format!("Leave it empty for {}.", default_shell_name()));

    pen = section(ui, area, pen + 12.0, "Where a tab reopens");
    let integration_row = row_at(area, pen);
    changed |= checkbox(
        ui,
        integration_row,
        "Ask PowerShell where it is",
        &mut settings.shell_integration,
    );
    pen += 32.0;
    pen = note(
        ui,
        area,
        pen,
        "A tab reopens in the folder its shell was in, read off the shell's own process. That answers for cmd.exe, bash and zsh and cannot answer for PowerShell: Set-Location moves PowerShell's location and never the process's current directory.",
    );
    pen = note(
        ui,
        area,
        pen + 4.0,
        "On, Unluminous adds one line to the prompt, after your own profile has set it up, so the shell says where it is. Off, nothing about your shell is changed. A shell that already reports its folder is followed either way.",
    );
    drawn(area, pen, changed)
}

/// What an empty shell setting means on this machine, in words, so the note under the field says the
/// name a person would type rather than `$SHELL`.
fn default_shell_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "PowerShell"
    } else {
        "the shell named in $SHELL"
    }
}

/// `Appearance & Behavior  >  Appearance` across the top of the page, and the line under it.
pub(crate) fn breadcrumb(ui: &mut egui::Ui, area: Rect, page: Page) -> f32 {
    let painter = ui.painter_at(area);
    let y = area.top() + 26.0;
    if page.group().is_empty() {
        let title = painter.layout_no_wrap(
            page.title().to_owned(),
            egui::FontId::proportional(13.5),
            color::text_strong(),
        );
        painter.galley(
            Pos2::new(area.left() + 24.0, y - title.size().y / 2.0),
            title,
            color::text_strong(),
        );
        return y + 22.0;
    }
    let group = painter.layout_no_wrap(
        page.group().to_owned(),
        egui::FontId::proportional(13.5),
        color::text_dim(),
    );
    let mut pen = area.left() + 24.0;
    painter.galley(Pos2::new(pen, y - group.size().y / 2.0), group.clone(), color::text_dim());
    pen += group.size().x + 8.0;
    let arrow = painter.layout_no_wrap(
        "\u{203A}".to_owned(),
        egui::FontId::proportional(13.5),
        color::text_faint(),
    );
    painter.galley(Pos2::new(pen, y - arrow.size().y / 2.0), arrow.clone(), color::text_faint());
    pen += arrow.size().x + 8.0;
    let title = painter.layout_no_wrap(
        page.title().to_owned(),
        egui::FontId::proportional(13.5),
        color::text_strong(),
    );
    painter.galley(Pos2::new(pen, y - title.size().y / 2.0), title, color::text_strong());
    y + 22.0
}

/// A heading inside a page, with a rule running to the right edge, as the reference editor draws one.
pub(crate) fn section(ui: &mut egui::Ui, area: Rect, top: f32, name: &str) -> f32 {
    let painter = ui.painter_at(area);
    let galley = painter.layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(12.5),
        color::text_strong(),
    );
    let y = top + 12.0;
    painter.galley(
        Pos2::new(area.left() + 24.0, y - galley.size().y / 2.0),
        galley.clone(),
        color::text_strong(),
    );
    let from = area.left() + 24.0 + galley.size().x + 12.0;
    painter.line_segment(
        [Pos2::new(from, y), Pos2::new(area.right() - 24.0, y)],
        Stroke::new(1.0, color::divider()),
    );
    y + 20.0
}

pub(crate) fn row_at(area: Rect, top: f32) -> Rect {
    Rect::from_min_size(Pos2::new(area.left() + 24.0, top), Vec2::new(area.width() - 48.0, 28.0))
}

pub(crate) fn label(ui: &mut egui::Ui, area: Rect, row: Rect, text: &str) {
    let painter = ui.painter_at(area);
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(12.5),
        color::text_control(),
    );
    painter.galley(
        Pos2::new(row.left(), row.center().y - galley.size().y / 2.0),
        galley,
        color::text_control(),
    );
}

/// A line of explanation under a control, in the faintest colour.
pub(crate) fn note(ui: &mut egui::Ui, area: Rect, top: f32, text: &str) -> f32 {
    let painter = ui.painter_at(area);
    let galley = painter.layout(
        text.to_owned(),
        egui::FontId::proportional(11.5),
        color::text_faint(),
        area.width() - 48.0,
    );
    painter.galley(Pos2::new(area.left() + 24.0, top), galley.clone(), color::text_faint());
    top + galley.size().y + 8.0
}

/// A button with a word in it, which the footer uses.
pub(crate) fn wide_button(ui: &mut egui::Ui, area: Rect, name: &str) -> bool {
    let response = ui.interact(area, ui.id().with(("settings-button", name)), Sense::click());
    let fill = if response.hovered() { color::accent() } else { color::control() };
    let painter = ui.painter();
    painter.rect(
        area,
        CornerRadius::same(size::CONTROL_CORNER),
        fill,
        Stroke::new(1.0, color::control_border()),
        egui::StrokeKind::Inside,
    );
    let galley = painter.layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(12.5),
        color::text_strong(),
    );
    painter.galley(area.center() - galley.size() / 2.0, galley, color::text_strong());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
    response.clicked()
}

fn line(ui: &egui::Ui, from: Pos2, to: Pos2) {
    ui.painter().line_segment([from, to], Stroke::new(1.0, color::divider()));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rectangle the modal gives `contents`, at the size the dialog asks for.
    fn dialog() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(WIDTH, HEIGHT))
    }

    /// The body a page is drawn into: the dialog less its header and its footer.
    fn body_height() -> f32 {
        HEIGHT - HEADER - FOOTER
    }

    /// Draw the whole dialog on one page and report how tall that page came out, along with
    /// everything that was painted.
    ///
    /// It draws twice, because the bar is built from the height the page came out at last frame: the
    /// first pass is what measures the page and the second is what a person is looking at.
    fn draw(page: Page, scroll: f32) -> (f32, Vec<egui::epaint::ClippedShape>) {
        let area = dialog();
        let plugins = crate::services::plugins::Plugins::load(None).0;
        let mut settings = Settings::default();
        let mut state = SettingsWindow { open: true, page, ..SettingsWindow::default() };
        // So that the MCP page does not read the person's own agent configuration to find out what
        // its buttons should say. A test must not read the settings of whoever is running it.
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
        };
        let egui = egui::Context::default();
        let mut shapes = Vec::new();
        for pass in 0..2 {
            if pass == 1 {
                state.scroll = scroll;
            }
            let output = egui.run_ui(egui::RawInput::default(), |ui| {
                contents(ui, area, &mut state, &mut settings, context, &mut |_, _, _| {});
            });
            shapes = output.shapes.clone();
            output.drop_without_applying_deltas();
        }
        (state.page_height, shapes)
    }

    /// **How tall each page really is**, printed rather than written down, so that the next row added
    /// to one can be measured rather than guessed at.
    ///
    /// `cargo test -p unluminous-app --lib how_tall_every_page_is -- --nocapture` prints the table.
    /// The numbers move with the interface font, so what is asserted is the one thing that is a rule:
    /// the Plugins page sizes itself to the room it is given and every other page reports a real
    /// height for what it drew.
    #[test]
    fn how_tall_every_page_is() {
        println!("body is {} points", body_height());
        for page in Page::ALL {
            let (height, _) = draw(page, 0.0);
            let over = height - body_height();
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
        // The Plugins page is two columns that each run to the bottom of the area and scroll on their
        // own, so it is exactly the room there is and the dialog never puts a third bar over them.
        let (plugins, _) = draw(Page::Plugins, 0.0);
        assert_eq!(plugins, body_height());
    }

    /// **A page that fits cannot be scrolled**, which is what leaves every page that already fitted
    /// drawn exactly where it was.
    #[test]
    fn a_page_that_fits_cannot_be_scrolled() {
        assert_eq!(settle_the_scroll(200.0, 400.0, 582.0), 0.0);
        assert_eq!(settle_the_scroll(200.0, 582.0, 582.0), 0.0);
        assert_eq!(lifted(dialog(), 0.0), dialog());
        assert_eq!(scrollbar::Bar::new(dialog(), 0.0, 400.0, 582.0), None);
    }

    /// A page taller than the body scrolls exactly as far as the part that cannot be seen, and no
    /// further in either direction.
    #[test]
    fn a_page_that_does_not_fit_scrolls_to_its_end_and_no_further() {
        assert_eq!(settle_the_scroll(-40.0, 700.0, 582.0), 0.0);
        assert_eq!(settle_the_scroll(5000.0, 700.0, 582.0), 118.0);
        let area = Rect::from_min_size(Pos2::new(0.0, 100.0), Vec2::new(600.0, 582.0));
        assert_eq!(lifted(area, 118.0).top(), -18.0);
        assert!(scrollbar::Bar::new(area, 0.0, 700.0, 582.0).is_some());
    }

    /// **Opening a different page starts at its top.** A page left half way down and come back to
    /// would put somebody somewhere they did not choose, several pages later.
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
        };
        let egui = egui::Context::default();
        let mut once = |state: &mut SettingsWindow| {
            let output = egui.run_ui(egui::RawInput::default(), |ui| {
                contents(ui, area, state, &mut settings, context, &mut |_, _, _| {});
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

    /// **Nothing the dialog draws lands outside the dialog.** This is the fault the scrolling page
    /// area was built for: before it, the Editor page ran past the body and its last tick box was
    /// painted over the window below the footer, where no picture of the dialog could show it.
    ///
    /// Checked on the Editor page at the top and at the end of its scroll, because the two put
    /// different parts of it against the two edges.
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
