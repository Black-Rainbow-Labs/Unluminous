//! The shortcuts on the rail and the top bar: the gallery of every one of them, the chips the top bar draws for
//! the ones pinned to it, and the flyout a tool's button on the rail opens. `task-2404`.
//!
//! *"If I right click the empty region on the left or top bar, I should see a popup menu that nicely displays a
//! grid of cards with icons with names for each."* The gallery is that popup. Each card is a shortcut, carved in
//! while it is pinned to the bar the gallery was opened for and raised out of it while it is not, which is how
//! the rail already says a pane is showing. A click pins or unpins at once, and the gallery stays open so several
//! can be changed in one visit.
//!
//! **Both popups here are `egui::Area`s rather than `egui::Popup`s.** egui keeps one popup open at a time, and the
//! Run Configurations flyout holds the configuration dropdown, which is a popup: a flyout that was one would shut
//! the moment its own dropdown opened. An area holding a popup is fine.
//!
//! Nothing here changes a setting. The gallery answers which card was pressed and the window pins it, through
//! `UnluminousApp::pin_the_shortcut`, which is where the right click menus and `window pin` go too.

use egui::{Color32, Pos2, Rect, Sense, Vec2};

use crate::components::activity_bar::{paint_mark, IconDrawer};
use crate::components::controls::WithHint as _;
use crate::services::shortcuts::{Bar, Kind};
use crate::theme::color;

/// One card: 100 by 76 points with a 14 point radius, four across with 8 between them. Four rather than three so
/// a window with every plugin switched on still holds the whole gallery above its status bar.
const CARD: Vec2 = Vec2::new(100.0, 76.0);
const CARD_RADIUS: f32 = 14.0;
const CARD_GAP: f32 = 8.0;
const COLUMNS: usize = 4;
/// How big a card's mark is drawn.
const CARD_MARK: f32 = 22.0;
/// How far the gallery's contents sit in from the frame on each side, beyond the frame's own margin.
const INSET: f32 = 6.0;

/// A chip on the top bar for a pinned shortcut: the text options chip's size, with 6 points between two.
pub const CHIP: f32 = crate::components::text_tools::BUTTON;
const CHIP_GAP: f32 = 6.0;

/// One card the gallery draws.
#[derive(Clone)]
pub struct GalleryCard {
    pub name: String,
    pub mark: IconDrawer,
    pub kind: Kind,
    /// Whether it is pinned to the bar the gallery is for.
    pub pinned: bool,
}

/// What the gallery reported this frame.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GalleryOutcome {
    /// The card that was pressed, by name.
    pub toggled: Option<String>,
    /// `Reset to Default` was pressed.
    pub reset: bool,
    /// A press outside it, or Escape: the gallery should close.
    pub close: bool,
}

/// Where a popup here may be: the window above its status bar, so a tall gallery slides up rather than over it.
fn room_above_the_status_bar(ctx: &egui::Context) -> Rect {
    let screen = ctx.content_rect();
    Rect::from_min_max(
        screen.min,
        Pos2::new(screen.right(), screen.bottom() - crate::theme::size::STATUS_BAR - 4.0),
    )
}

/// How wide the gallery's grid is.
fn grid_width() -> f32 {
    COLUMNS as f32 * CARD.x + (COLUMNS as f32 - 1.0) * CARD_GAP
}

/// Draw the gallery for `bar` with its corner near `at`, and report what was pressed.
///
/// On the rail it opens to the right of the pointer and on the top bar under it, and egui keeps it on the screen.
/// @param ctx - the window
/// @param at - where the right click was
/// @param bar - the bar it is for, which decides its heading and what pinned means
/// @param cards - every shortcut, panels first and then tools
pub fn show(ctx: &egui::Context, at: Pos2, bar: Bar, cards: &[GalleryCard]) -> GalleryOutcome {
    let mut outcome = GalleryOutcome::default();
    let place = match bar {
        Bar::Rail => at + Vec2::new(10.0, -18.0),
        Bar::Top => at + Vec2::new(-grid_width() / 2.0, 10.0),
    };
    let area = egui::Area::new(egui::Id::new("shortcut-gallery"))
        .order(egui::Order::Foreground)
        .fixed_pos(place)
        .constrain_to(room_above_the_status_bar(ctx))
        .show(ctx, |ui| {
            crate::components::controls::menu_frame(ui)
                .show(ui, |ui| contents(ui, bar, cards, &mut outcome))
        });
    let inside = area.response.rect;
    let pressed_outside = ctx.input(|input| {
        input.pointer.primary_pressed()
            && input.pointer.interact_pos().is_some_and(|pos| !inside.contains(pos))
    });
    if pressed_outside || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        outcome.close = true;
    }
    outcome
}

/// What is inside the gallery's frame: the heading, the two sections of cards, and the reset button.
fn contents(ui: &mut egui::Ui, bar: Bar, cards: &[GalleryCard], outcome: &mut GalleryOutcome) {
    let panels: Vec<&GalleryCard> = cards.iter().filter(|card| card.kind == Kind::Panel).collect();
    let tools: Vec<&GalleryCard> = cards.iter().filter(|card| card.kind != Kind::Panel).collect();
    let rows = |count: usize| count.div_ceil(COLUMNS) as f32;
    let section = |count: usize| match count {
        0 => 0.0,
        _ => 26.0 + rows(count) * CARD.y + (rows(count) - 1.0) * CARD_GAP + 10.0,
    };
    let height = 52.0 + section(panels.len()) + section(tools.len()) + 40.0;
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(grid_width() + 2.0 * INSET, height), Sense::hover());
    let inner = rect.shrink2(Vec2::new(INSET, 0.0));
    heading(ui, inner, bar);
    let mut top = inner.top() + 52.0;
    let label_and_cards = |ui: &mut egui::Ui,
                           top: f32,
                           label: &str,
                           group: &[&GalleryCard],
                           outcome: &mut GalleryOutcome| {
        if group.is_empty() {
            return top;
        }
        section_label(ui.painter(), Pos2::new(inner.left() + 2.0, top + 9.0), label);
        let grid = Rect::from_min_size(
            Pos2::new(inner.left(), top + 26.0),
            Vec2::new(grid_width(), section(group.len()) - 36.0),
        );
        if let Some(name) = grid_of_cards(ui, grid, bar, group) {
            outcome.toggled = Some(name);
        }
        top + section(group.len())
    };
    top = label_and_cards(ui, top, "PANELS", &panels, outcome);
    top = label_and_cards(ui, top, "TOOLS", &tools, outcome);
    // The footer: a groove, then `Reset to Default` at the right.
    let groove_y = top + 2.0;
    crate::components::chrome_kit::groove(
        ui.painter(),
        Pos2::new(inner.left(), groove_y),
        Pos2::new(inner.right(), groove_y),
    );
    let button = Rect::from_min_size(
        Pos2::new(inner.right() - 124.0, groove_y + 8.0),
        Vec2::new(124.0, 26.0),
    );
    if crate::components::controls::bar_button(ui, button, "Reset to Default", false).clicked() {
        outcome.reset = true;
    }
}

/// The gallery's heading, and the line under it that says how it is used.
fn heading(ui: &egui::Ui, inner: Rect, bar: Bar) {
    let painter = ui.painter();
    let title = format!("{} Shortcuts", bar.label());
    let galley =
        painter.layout_no_wrap(title, egui::FontId::proportional(15.0), color::text_strong());
    painter.galley(Pos2::new(inner.left() + 2.0, inner.top() + 6.0), galley, color::text_strong());
    let hint = match bar {
        Bar::Rail => "Click a card to pin it to the side bar or unpin it.",
        Bar::Top => "Click a card to pin it to the top bar or unpin it.",
    };
    let galley = painter.layout_no_wrap(
        hint.to_owned(),
        egui::FontId::proportional(11.5),
        color::text_dim(),
    );
    painter.galley(Pos2::new(inner.left() + 2.0, inner.top() + 28.0), galley, color::text_dim());
}

/// A section's name, small and in the quiet ink, over its cards.
fn section_label(painter: &egui::Painter, at: Pos2, label: &str) {
    let galley = painter.layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(10.5),
        color::text_faint(),
    );
    painter.galley(Pos2::new(at.x, at.y - galley.size().y / 2.0), galley, color::text_faint());
}

/// The cards of one section, three across, and the name of the one pressed.
fn grid_of_cards(
    ui: &mut egui::Ui,
    grid: Rect,
    bar: Bar,
    group: &[&GalleryCard],
) -> Option<String> {
    crate::components::chrome_kit::layer(
        ui,
        ("shortcut-cards", bar.name(), grid.top() as i32),
        grid,
        12.0,
        |rux| {
            let mut pressed = None;
            for (index, card) in group.iter().enumerate() {
                let column = (index % COLUMNS) as f32;
                let row = (index / COLUMNS) as f32;
                let rect = Rect::from_min_size(
                    grid.min + Vec2::new(column * (CARD.x + CARD_GAP), row * (CARD.y + CARD_GAP)),
                    CARD,
                );
                if one_card(rux, rect, card) {
                    pressed = Some(card.name.clone());
                }
            }
            pressed
        },
    )
}

/// One card: carved in while pinned with a tick at its corner, raised while not, its mark over its name.
fn one_card(rux: &mut rux::Rux<'_>, rect: Rect, card: &GalleryCard) -> bool {
    let name = format!("Shortcut: {}", card.name);
    let hint = match card.pinned {
        true => format!("{}: pinned, click to unpin", card.name),
        false => format!("{}: click to pin", card.name),
    };
    let response = rux
        .ui
        .interact(rect, rux.ui.id().with(("shortcut-card", &card.name)), Sense::click())
        .with_hint(hint);
    let chat = rux.theme().chat;
    let ink = rux.theme().ink;
    match card.pinned {
        true => rux.chrome.surface(rect, CARD_RADIUS, chat.well, chat.carved_sm()),
        false => rux.chrome.surface(
            rect,
            CARD_RADIUS,
            rux::components::chip(&chat, rect),
            chat.raised_chip(),
        ),
    }
    if response.hovered() && !card.pinned {
        rux.wash(rect, CARD_RADIUS, rux.theme().interaction.hover);
    }
    let tint = match (response.hovered(), card.pinned) {
        (true, _) => chat.sky,
        (false, true) => ink.i900,
        (false, false) => ink.i400,
    };
    paint_mark(rux, card.mark, Pos2::new(rect.center().x, rect.top() + 26.0), CARD_MARK, tint);
    words(rux.ui.painter(), rect, &card.name, if card.pinned { ink.i900 } else { ink.i500 });
    if card.pinned {
        tick(rux, Pos2::new(rect.right() - 13.0, rect.top() + 13.0));
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, card.pinned, &name)
    });
    response.clicked()
}

/// A card's name under its mark: centred, wrapped to two lines, and cut short after that.
fn words(painter: &egui::Painter, rect: Rect, name: &str, tint: Color32) {
    let font = egui::FontId::proportional(12.0);
    let mut job = egui::text::LayoutJob::simple(name.to_owned(), font, tint, rect.width() - 16.0);
    job.wrap.max_rows = 2;
    job.halign = egui::Align::Center;
    let galley = painter.layout_job(job);
    painter.galley(Pos2::new(rect.center().x, rect.top() + 44.0), galley, tint);
}

/// The small accent disc with a tick that says a card is pinned.
fn tick(rux: &mut rux::Rux<'_>, centre: Pos2) {
    rux.ui.painter().circle_filled(centre, 7.5, color::accent());
    rux.mark(rux::Mark::new(rux::Icon::Check, 10.0), centre, Color32::WHITE);
}

/// One chip on the top bar for a pinned shortcut.
#[derive(Clone)]
pub struct Chip {
    pub name: String,
    pub mark: IconDrawer,
    /// Whether what it opens is showing, which carves it in.
    pub on: bool,
}

/// What the top bar's chips reported.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ChipsOutcome {
    /// The chip pressed, by its place in the list.
    pub pressed: Option<usize>,
    /// A chip right clicked: where, and which.
    pub menu: Option<(Pos2, usize)>,
}

/// How wide `count` chips are side by side.
pub fn chips_width(count: usize) -> f32 {
    match count {
        0 => 0.0,
        _ => count as f32 * CHIP + (count as f32 - 1.0) * CHIP_GAP,
    }
}

/// Draw the chips into `area`, left to right, and report what was done to them.
///
/// A chip is the text options chip's shape: raised out of the bar, carved in while its pane is showing. It is
/// named `<name> in the top bar`, because the rail's button for the same shortcut may be drawn at the same time
/// and no two controls in the window share a name.
pub fn chips(ui: &mut egui::Ui, area: Rect, chips: &[Chip]) -> ChipsOutcome {
    let mut outcome = ChipsOutcome::default();
    let middle = area.center().y;
    for (index, chip) in chips.iter().enumerate() {
        let left = area.left() + index as f32 * (CHIP + CHIP_GAP);
        let rect = Rect::from_min_size(Pos2::new(left, middle - CHIP / 2.0), Vec2::splat(CHIP));
        let name = format!("{} in the top bar", chip.name);
        crate::components::chrome_kit::layer(ui, ("pinned-chip", &chip.name), rect, 10.0, |rux| {
            let response = rux
                .ui
                .interact(rect, rux.ui.id().with(("pinned-chip", &chip.name)), Sense::click())
                .with_hint(&chip.name);
            let chat = rux.theme().chat;
            let ink = rux.theme().ink;
            match chip.on {
                true => rux.chrome.surface(rect, CHIP / 2.0, chat.well, chat.carved_sm()),
                false => rux::components::chip_disc(rux, rect),
            }
            let tint = match (response.hovered(), chip.on) {
                (true, _) => chat.sky,
                (false, true) => ink.i900,
                (false, false) => ink.i500,
            };
            paint_mark(rux, chip.mark, rect.center(), 17.0, tint);
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Button, true, chip.on, &name)
            });
            if response.clicked() {
                outcome.pressed = Some(index);
            }
            if response.secondary_clicked() {
                let at = response.interact_pointer_pos().unwrap_or(rect.center());
                outcome.menu = Some((at, index));
            }
        });
    }
    outcome
}

/// Where a tool's flyout on the rail is, and whether it was opened by a press rather than by the pointer resting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flyout {
    pub tool: crate::services::shortcuts::Tool,
    /// The button it was opened from.
    pub anchor: Rect,
    /// Opened by a press: it stays open until a press outside it or Escape.
    pub held: bool,
    /// When the pointer left the button and the flyout, for one that was not pressed open.
    pub left_at: Option<f64>,
}

/// How long a flyout that was opened by the pointer stays once the pointer has left it, which is what lets the
/// pointer travel from the button to the flyout's lower rows on a slant.
pub const GRACE: f64 = 0.25;

/// The least a flyout is wide, so a heading always fits.
const FLYOUT_LEAST: f32 = 200.0;

/// Draw a tool's flyout beside its button on the rail: a raised card with the tool's mark and name as its heading,
/// and `add` drawing the tool's own controls under it. Answers what `add` returned and the card's rectangle.
/// @param ctx - the window
/// @param flyout - which tool, and the button it came from
/// @param mark - the tool's mark
/// @param width - how wide the tool's controls are, measured in the card's own `Ui`, which the card is sized to
/// @param add - the tool's controls
pub fn tool_flyout<R>(
    ctx: &egui::Context,
    flyout: &Flyout,
    mark: IconDrawer,
    width: impl FnOnce(&egui::Ui) -> f32,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, Rect) {
    let place = Pos2::new(flyout.anchor.right() + 12.0, flyout.anchor.top() - 10.0);
    let shown = egui::Area::new(egui::Id::new(("rail-flyout", flyout.tool.name())))
        .order(egui::Order::Foreground)
        .fixed_pos(place)
        .constrain_to(room_above_the_status_bar(ctx))
        .show(ctx, |ui| {
            crate::components::controls::menu_frame(ui)
                .show(ui, |ui| {
                    // An area offers the whole screen's width, so the card is given the tool's own.
                    let width = width(ui);
                    ui.set_width(width.max(FLYOUT_LEAST));
                    crate::components::controls::set_in_the_list_size(ui);
                    flyout_heading(ui, flyout.tool.name(), mark);
                    add(ui)
                })
                .inner
        });
    (shown.inner, shown.response.rect)
}

/// The flyout's heading: the tool's mark and its name in the strong ink.
fn flyout_heading(ui: &mut egui::Ui, title: &str, mark: IconDrawer) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
    let centre = Pos2::new(rect.left() + 14.0, rect.center().y);
    crate::components::chrome_kit::layer(
        ui,
        ("flyout-heading", title),
        Rect::from_center_size(centre, Vec2::splat(20.0)),
        2.0,
        |rux| {
            let tint = rux.theme().chat.sky;
            paint_mark(rux, mark, centre, 16.0, tint);
        },
    );
    let painter = ui.painter();
    let font = egui::FontId::proportional(13.0);
    let top = crate::theme::crisp::top_centring_capitals(painter, &font, rect.center().y);
    let galley = painter.layout_no_wrap(title.to_owned(), font, color::text_strong());
    painter.galley(Pos2::new(rect.left() + 30.0, top), galley, color::text_strong());
    ui.add_space(4.0);
}

/// One quiet line in a flyout whose tool cannot apply to the tab that is showing.
pub fn cannot_apply(ui: &mut egui::Ui, words: &str) {
    ui.add_space(2.0);
    ui.label(egui::RichText::new(words).size(11.5).color(color::text_dim()));
    ui.add_space(4.0);
}

/// Whether the pointer is over the button, the flyout, or the corridor between them, which keeps a flyout open.
pub fn pointer_is_with_it(ctx: &egui::Context, anchor: Rect, card: Rect) -> bool {
    let corridor = Rect::from_min_max(
        Pos2::new(anchor.right() - 2.0, anchor.top().min(card.top())),
        Pos2::new(card.left() + 2.0, anchor.bottom().max(card.bottom())),
    );
    ctx.input(|input| input.pointer.latest_pos()).is_some_and(|pos| {
        anchor.expand(2.0).contains(pos) || card.expand(4.0).contains(pos) || corridor.contains(pos)
    })
}
