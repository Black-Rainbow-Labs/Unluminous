//! The strip of tabs along the top of the editing area, one for each open file.
//!
//! It sits over the editing area rather than over the whole window, because the explorer is to the
//! left of it and a tab belongs to the editor. That is where the reference editor puts it and where
//! `tasks/unluminous-ide-tdd.md` says it goes.
//!
//! The tab that is showing is a filled row with a two point accent line along its bottom edge, which
//! is the underline `task-1649` asks for. A file with unsaved changes shows the amber dot Unluminous
//! already uses for that, in place of the close cross until the pointer is over the tab. A transient
//! tab — the one a single click reuses — has its name in italic, which is how a person can tell that
//! clicking another file will take it away.
//!
//! When the tabs do not fit, the strip is shifted so the one that is showing is on screen, rather
//! than the tabs being squeezed to unreadable stubs. There is no scroll area, because everything in
//! Unluminous is painted at an absolute position and one container that is not would behave differently
//! from the rest of the window.
//!
//! ## One strip a pane
//!
//! Since `task-1664` the editing area can be split into panes, and each pane has a strip of its own
//! holding its own tabs. Two things follow, and both are parameters rather than something the strip
//! works out.
//!
//! Every control's id carries the **pane number**, because egui identifies a widget by its id and two
//! strips whose second tab shared an id would hand one click to both of them.
//!
//! And the strip is told whether its pane has the **keyboard**. The accent line under the tab that is
//! showing is drawn in the quiet colour when it does not, which is how a person sees at a glance
//! which of four panes their typing is going to.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use crate::components::controls;
use crate::components::controls::WithHint as _;
use crate::theme::crisp::CrispPainter;
use crate::theme::{color, icon};

/// How tall the strip is. The same as the terminal tile's header, so the two horizontal strips in
/// the window are the same height.
pub const HEIGHT: f32 = 32.0;
/// The accent line under the tab that is showing.
const UNDERLINE: f32 = 2.0;
/// Space either side of a tab's contents.
const PADDING: f32 = 10.0;
/// The gap between two tabs.
const GAP: f32 = 2.0;

/// One tab, as the strip draws it. The strip is given these rather than the open files themselves,
/// because a component draws and does not reach into the window's state.
#[derive(Clone)]
pub struct TabView {
    pub name: String,
    /// There are unsaved changes.
    pub modified: bool,
    /// This is the tab a single click in the explorer reuses.
    pub transient: bool,
    /// The small square in front of the name, coloured by what kind of file it is. Drawn only when
    /// the file has no plugin to give it a picture.
    pub marker: Color32,
    /// The picture the file's plugin puts in front of it.
    pub icon: Option<egui::TextureHandle>,
}

/// What the user did in the strip.
#[derive(Debug, Default, PartialEq)]
pub struct TabsOutcome {
    /// A tab was clicked, so it should be shown.
    pub show: Option<usize>,
    /// A tab's close cross was clicked, or it was clicked with the middle button.
    pub close: Option<usize>,
    /// A tab was double clicked, which makes a transient tab permanent.
    pub keep: Option<usize>,
    /// A tab was right clicked: which one, and where the pointer was.
    pub menu: Option<(usize, Pos2)>,
    /// A tab is being dragged: which one, and where the pointer is now.
    ///
    /// Reported every frame the drag is held, and again with [`Self::dropped`] set on the frame it
    /// is let go. Where it lands is not the strip's business: the pointer may be over a strip
    /// belonging to another pane, which this one has never heard of, so the window works it out once
    /// every pane has said where its own tabs are. See [`Strip`].
    pub dragging: Option<(usize, Pos2)>,
    /// The drag ended on this frame.
    pub dropped: bool,
    /// The empty part of the strip, past the last tab, was double clicked.
    ///
    /// `task-1771` asks that two presses at the top of a pane fill the window with it. Every other pane in
    /// Unluminous has a header to press; the editing area has this strip, and the part of it no tab wanted is
    /// exactly what a panel's own drag handle is left with. See `components::dock::handle`.
    pub twice_on_the_empty_part: bool,
    /// The empty part of the strip is being dragged, which carries the whole pane: where the pointer is.
    ///
    /// `task-2280`: *"I should be able to click in the empty space to the right of a tab and drag and place
    /// it."* Reported every frame the drag is held and again with [`Self::pane_dropped`] set on the frame it is
    /// let go; where it lands is the window's to decide, for the reason [`Self::dragging`] gives.
    pub pane_dragging: Option<Pos2>,
    /// The pane drag ended on this frame.
    pub pane_dropped: bool,
    /// The empty part of the strip was right clicked: where the pointer was. `task-2270`.
    pub menu_on_the_empty_part: Option<Pos2>,
    /// Where this strip drew itself and each of its tabs, so the window can say which strip a
    /// pointer is over and where between two tabs it fell.
    pub strip: Strip,
}

/// Where a strip and its tabs ended up on the screen.
///
/// A component draws and does not decide, so this is what the strip reports rather than a conclusion
/// about it. It is the only way a tab can be dragged from one pane into another: each pane draws its
/// own strip and knows nothing of the others, and a drag started in one of them carries a pointer
/// that ends up over a different one.
#[derive(Debug, Clone, PartialEq)]
pub struct Strip {
    /// The whole strip.
    pub area: Rect,
    /// One rectangle a tab, in the order they are drawn — including the ones scrolled off the end,
    /// so dropping a tab past the visible ones still lands somewhere sensible.
    pub tabs: Vec<Rect>,
}

impl Default for Strip {
    /// A strip that has not been drawn: nowhere, holding nothing. `Rect` has no `Default` of its
    /// own, deliberately, because there is no obvious empty rectangle; `NOTHING` is the one egui
    /// offers for a rectangle that has not been decided yet.
    fn default() -> Self {
        Self { area: Rect::NOTHING, tabs: Vec::new() }
    }
}

impl Strip {
    /// Where in this strip a tab dropped at `x` belongs: how many tabs it goes after.
    ///
    /// A tab goes after every tab whose middle the pointer has passed, which is what makes a
    /// rearrangement follow the pointer rather than jump when it crosses an edge.
    pub fn position_at(&self, x: f32) -> usize {
        self.tabs.iter().filter(|rect| rect.center().x < x).count()
    }
}

/// The widest a tab's name is drawn, in points. `task-2277`: a web page's title can be a whole sentence,
/// and a tab as wide as the sentence pushed every other tab off the strip. About thirty characters of the
/// interface font, which is roughly what a browser shows of a page title before it cuts one short.
pub const NAME_MOST: f32 = 180.0;

/// The name as it is drawn on the tab: whole when it fits in [`NAME_MOST`], and otherwise cut at the last
/// character that leaves room for an ellipsis after it.
///
/// The tab's own name, the one its tooltip shows and a test or the command line finds it by, is always the
/// whole name. Only the drawing is cut.
pub fn name_on_the_tab(painter: &egui::Painter, name: &str, font: &egui::FontId) -> String {
    let width =
        |text: String| painter.layout_no_wrap(text, font.clone(), color::text_control()).size().x;
    if width(name.to_owned()) <= NAME_MOST {
        return name.to_owned();
    }
    let cut = |characters: usize| {
        let end = name.char_indices().nth(characters).map_or(name.len(), |(at, _)| at);
        format!("{}\u{2026}", name[..end].trim_end())
    };
    // The longest start of the name that fits with the ellipsis after it, found by halving.
    let (mut fits, mut too_long) = (0usize, name.chars().count());
    while fits < too_long {
        let middle = (fits + too_long).div_ceil(2);
        match width(cut(middle)) <= NAME_MOST {
            true => fits = middle,
            false => too_long = middle - 1,
        }
    }
    cut(fits)
}

/// How wide one tab is. Measured rather than guessed, because a name can be any length, and never wider
/// than [`NAME_MOST`] allows for the name.
fn tab_width(ui: &egui::Ui, tab: &TabView) -> f32 {
    let font = egui::FontId::proportional(12.0);
    let shown = name_on_the_tab(ui.painter(), &tab.name, &font);
    let galley = ui.painter().crisp_layout_no_wrap(shown, font, color::text_control());
    // The marker, the gap after it, the name, the gap before the cross, and the cross.
    PADDING + 8.0 + 8.0 + galley.size().x + 8.0 + 14.0 + PADDING
}

/// Draw the strip into `area`.
///
/// `active` is which of `tabs` is showing, counting within this strip. `pane` is which pane the strip
/// belongs to, which keeps the ids of two strips apart, and `focused` says whether that pane has the
/// keyboard.
pub fn show(
    ui: &mut egui::Ui,
    area: Rect,
    tabs: &[TabView],
    active: usize,
    pane: usize,
    focused: bool,
    opacity: f32,
) -> TabsOutcome {
    let mut outcome = TabsOutcome::default();
    let painter = ui.painter_at(area);
    painter.rect_filled(
        area,
        CornerRadius::ZERO,
        crate::theme::pane_ground(color::toolbar(), opacity),
    );
    // **Under glass the tabs are a row inside the panel's glass** (`task-2281`): clear of its rounded edges at
    // the sides, and in the bottom `HEIGHT` of a strip that the editing area makes taller by a header's drop.
    // A canvas node's strip is `HEIGHT` tall, so it gets the side inset and no drop.
    let row = match crate::theme::is_glass() {
        true => {
            let side = crate::theme::glass::PANE_GAP + 4.0;
            Rect::from_min_max(
                Pos2::new(area.left() + side, (area.bottom() - HEIGHT).max(area.top())),
                Pos2::new(area.right() - side, area.bottom()),
            )
        }
        false => area,
    };
    painter.line_segment(
        [Pos2::new(row.left(), row.bottom()), Pos2::new(row.right(), row.bottom())],
        Stroke::new(1.0, color::divider()),
    );

    let widths: Vec<f32> = tabs.iter().map(|tab| tab_width(ui, tab)).collect();
    let offset = scroll_the_strip(ui, area, &widths, active, row.width(), pane);

    // **Over the whole strip, and added first, so it is left with exactly the part no tab wanted.** egui
    // gives a pointer to the *last* widget that asked for it, so a widget added before the tabs is one the
    // tabs take back wherever they are — which is the order `components::dock::handle` documents and the
    // reason a panel's own header can be both a handle and a row of buttons. Two presses here fill the
    // window with the editing area: `task-1771` asks for that at the top of every pane, and this strip is
    // what the editing area has instead of a header.
    //
    // **And it is the handle the whole pane is carried by** (`task-2280`), which is what a panel's header is.
    // `Sense::CLICK | Sense::DRAG` rather than `click_and_drag()`, for the reason `components::dock::handle`
    // gives: a widget this large must not be able to take the keyboard focus.
    let empty =
        ui.interact(area, ui.id().with(("tab-strip-empty", pane)), Sense::CLICK | Sense::DRAG);
    empty.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Tab strip empty part")
    });
    outcome.twice_on_the_empty_part = empty.double_clicked();
    if empty.secondary_clicked() {
        outcome.menu_on_the_empty_part = empty.interact_pointer_pos();
    }
    if empty.dragged() || empty.drag_stopped() {
        outcome.pane_dragging = empty.interact_pointer_pos();
        outcome.pane_dropped = empty.drag_stopped();
    }

    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(row));
    inner.set_clip_rect(ui.painter().clip_rect().intersect(row));

    outcome.strip.area = area;
    let mut pen = row.left() - offset;
    for (index, tab) in tabs.iter().enumerate() {
        let rect =
            Rect::from_min_size(Pos2::new(pen, row.top()), Vec2::new(widths[index], row.height()));
        pen += widths[index] + GAP;
        outcome.strip.tabs.push(rect);
        if rect.right() < row.left() || rect.left() > row.right() {
            continue;
        }
        draw_tab(
            &mut inner,
            rect,
            tab,
            Where { pane, index, focused },
            index == active,
            &mut outcome,
        );
    }
    outcome
}

/// A tab under glass (`task-2281`): the open tab is a rounded piece of glass with the card's tint and its
/// accent line inside it, and any other tab is clear until the pointer is on it. Jason found the square dark
/// block of the other themes stood out against the glass and ran into the panel's rounded corner.
///
/// @param painter - the strip's painter
/// @param rect - the tab
/// @param active - whether it is the tab that is showing
/// @param focused - whether its pane has the keyboard, which makes the accent line full strength
/// @param pointed - whether the pointer is on it or it is being carried
fn glass_tab(painter: &egui::Painter, rect: Rect, active: bool, focused: bool, pointed: bool) {
    let chip = Rect::from_min_max(
        Pos2::new(rect.left(), rect.top() + 3.0),
        Pos2::new(rect.right(), rect.bottom() - 4.0),
    );
    const RADIUS: f32 = 9.0;
    if active {
        crate::theme::glass::paint_glass(painter, chip, RADIUS);
        painter.rect_filled(
            chip,
            CornerRadius::same(RADIUS as u8),
            crate::theme::glass::popup_fill(),
        );
        let line = if focused { color::accent() } else { color::accent().gamma_multiply(0.35) };
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(chip.left() + RADIUS, chip.bottom() - UNDERLINE - 2.0),
                Vec2::new((chip.width() - RADIUS * 2.0).max(0.0), UNDERLINE),
            ),
            CornerRadius::same(1),
            line,
        );
    } else if pointed {
        painter.rect_filled(chip, CornerRadius::same(RADIUS as u8), controls::hover_fill());
    }
}

/// How far along the strip is scrolled this frame, which is remembered from the last frame.
///
/// `task-2270`: *"I should be able to scroll tabs horizontally, without a scrollbar. e.g. scroll wheel when
/// hovered over a tab should move left to right, same with swiping on trackpad."* The strip used to be shifted
/// by [`shift`] alone, which is worked out from the tab that is showing, so a tab off the end could be reached
/// only by showing it some other way. Now the offset is kept in egui's memory under the strip's pane, and:
///
/// - **The wheel or a swipe over the strip moves it**, whichever way the larger part of the movement went, so
///   a mouse wheel turned downwards moves along to the right and a sideways swipe moves the way the fingers
///   went. What is used is taken out of the frame, so the editing area under the strip does not scroll too.
/// - **A tab that becomes the one showing is brought into view**, the least distance that shows all of it,
///   which is what [`shift`] did on every frame. Asked only when the tab showing or the number of tabs
///   changed, so a scrolled strip is not pulled back on the next frame.
///
/// There is no scrollbar, and nothing is drawn for the scroll at all.
fn scroll_the_strip(
    ui: &egui::Ui,
    area: Rect,
    widths: &[f32],
    active: usize,
    available: f32,
    pane: usize,
) -> f32 {
    let id = ui.id().with(("tab-strip-scroll", pane));
    let total: f32 = widths.iter().map(|width| width + GAP).sum();
    let most = (total - GAP - available).max(0.0);
    let remembered: Option<(f32, usize, usize)> = ui.data(|data| data.get_temp(id));
    let mut offset = match remembered {
        Some((offset, was_active, was_count))
            if was_active == active && was_count == widths.len() =>
        {
            offset
        }
        Some((offset, ..)) => keep_in_view(widths, active, available, offset),
        None => shift(widths, active, available),
    };
    let over = crate::components::controls::pointer_in(ui).is_some_and(|at| area.contains(at))
        && ui.ctx().pointer_hover_pos().is_some();
    if over && most > 0.0 {
        let delta = ui.input(|input| input.smooth_scroll_delta);
        let along = match delta.x.abs() > delta.y.abs() {
            true => delta.x,
            false => delta.y,
        };
        if along != 0.0 {
            offset -= along;
            ui.ctx().input_mut(|input| input.smooth_scroll_delta = Vec2::ZERO);
        }
    }
    let offset = offset.clamp(0.0, most);
    ui.data_mut(|data| data.insert_temp(id, (offset, active, widths.len())));
    offset
}

/// The least change to `offset` that puts the whole of the tab at `active` inside `available` points.
fn keep_in_view(widths: &[f32], active: usize, available: f32, offset: f32) -> f32 {
    let Some(width) = widths.get(active) else { return offset };
    let left: f32 = widths.iter().take(active).map(|width| width + GAP).sum();
    let right = left + width;
    if left < offset {
        left
    } else if right > offset + available {
        (right - available).min(left)
    } else {
        offset
    }
}

/// How far the strip is shifted left so that the tab that is showing is on screen.
///
/// Split out so it can be tested without a window: the arithmetic is what decides whether a tab a
/// long way along the strip can be reached at all.
fn shift(widths: &[f32], active: usize, available: f32) -> f32 {
    let total: f32 = widths.iter().map(|width| width + GAP).sum();
    if total <= available || active >= widths.len() {
        return 0.0;
    }
    let left: f32 = widths.iter().take(active).map(|width| width + GAP).sum();
    let right = left + widths[active];
    // Far enough left to bring the active tab's right edge into view, and never past its left edge.
    let needed = (right - available).max(0.0);
    needed.min(left).max(0.0)
}

/// Where one tab is: which strip it is in, where in that strip, and whether the strip has the
/// keyboard. Three values that travel together, so they are one argument rather than three.
#[derive(Clone, Copy)]
struct Where {
    pane: usize,
    index: usize,
    focused: bool,
}

/// One tab: the marker, the name, and either the unsaved dot or the close cross.
fn draw_tab(
    ui: &mut egui::Ui,
    rect: Rect,
    tab: &TabView,
    at: Where,
    active: bool,
    outcome: &mut TabsOutcome,
) {
    let index = at.index;
    let name = format!("Tab: {}", tab.name);
    // A tab senses a drag as well as a click, which is how it is rearranged. egui only calls a press
    // a drag once the pointer has moved far enough, so a click is still a click.
    let response = ui
        .interact(rect, ui.id().with(("file-tab", at.pane, index)), Sense::click_and_drag())
        .with_hint(&name);
    if response.dragged() || response.drag_stopped() {
        if let Some(pointer) = response.interact_pointer_pos() {
            outcome.dragging = Some((index, pointer));
            outcome.dropped = response.drag_stopped();
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    let painter = ui.painter();
    if crate::theme::is_glass() {
        glass_tab(painter, rect, active, at.focused, response.hovered() || response.dragged());
    } else if active {
        controls::pill(painter, rect, 0);
        // The accent line, quiet in a pane that has not got the keyboard, so which pane is being
        // typed into can be seen at a glance.
        let line = if at.focused { color::accent() } else { color::accent().gamma_multiply(0.35) };
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left(), rect.bottom() - UNDERLINE),
                Vec2::new(rect.width(), UNDERLINE),
            ),
            CornerRadius::ZERO,
            line,
        );
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::ZERO, color::control());
    }
    // A tab being carried is outlined, so it is clear which one is in the air. The insertion mark
    // that says where it would land is drawn by the window, because only the window knows which
    // strip the pointer has ended up over.
    if response.dragged() && !crate::theme::is_glass() {
        painter.rect_filled(rect, CornerRadius::ZERO, color::control());
        painter.rect_stroke(
            rect.shrink(1.0),
            CornerRadius::same(3),
            Stroke::new(1.0, color::accent()),
            egui::StrokeKind::Inside,
        );
    }

    match &tab.icon {
        Some(icon) => crate::services::icons::draw(
            painter,
            Pos2::new(rect.left() + PADDING + 5.0, rect.center().y),
            icon,
        ),
        None => {
            painter.rect_filled(
                Rect::from_center_size(
                    Pos2::new(rect.left() + PADDING + 4.0, rect.center().y),
                    Vec2::splat(8.0),
                ),
                CornerRadius::same(2),
                tab.marker,
            );
        }
    }

    let tint = if active { color::text_strong() } else { color::text_control() };
    // A transient tab is drawn faintly rather than in italic: egui has no italic face for the
    // family Unluminous installs, and a fake slant is worse than a change of weight.
    let tint = if tab.transient { tint.gamma_multiply(0.75) } else { tint };
    // By its capitals, so the name lines up with the icon in front of it — `crisp::top_centring_capitals`.
    let font = egui::FontId::proportional(12.0);
    let top = crate::theme::crisp::top_centring_capitals(painter, &font, rect.center().y);
    let shown = name_on_the_tab(painter, &tab.name, &font);
    let galley = painter.crisp_layout_no_wrap(shown, font, tint);
    painter.crisp_galley(Pos2::new(rect.left() + PADDING + 16.0, top), galley, tint);

    // The cross, or the amber dot when there are unsaved changes and the pointer is elsewhere.
    let shut = Rect::from_center_size(
        Pos2::new(rect.right() - PADDING - 5.0, rect.center().y),
        Vec2::splat(16.0),
    );
    if tab.modified && !response.hovered() {
        painter.circle_filled(shut.center(), 3.5, color::unsaved());
    } else {
        // **`Close tab <name>` rather than `Close <name>`** (`task-1984` S8): an editor tab and a File
        // Editor node on the same file are on the screen together, and two controls with one name is
        // what the style guide forbids and what the screenshot tests find controls by -- a test
        // closing a file by name closed whichever egui walked first.
        let shut_name = format!("Close tab {}", tab.name);
        let shut_response = ui
            .interact(shut, ui.id().with(("file-tab-close", at.pane, index)), Sense::click())
            .with_hint(&shut_name);
        icon::cross(ui.painter(), shut.center(), color::text_dim());
        shut_response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &shut_name));
        if shut_response.clicked() {
            outcome.close = Some(index);
        }
    }

    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, &name));
    if response.double_clicked() {
        outcome.keep = Some(index);
    } else if response.clicked() {
        outcome.show = Some(index);
    }
    if response.middle_clicked() {
        outcome.close = Some(index);
    }
    // A right click opens the tab's own menu. The window shows the tab first, so that every entry in
    // the menu can be about "the tab that is showing" and so be an ordinary action with no argument.
    if response.secondary_clicked() {
        if let Some(at) = response.interact_pointer_pos().or_else(|| response.hover_pos()) {
            outcome.menu = Some((index, at));
        }
    }
}

/// Draw the mark that says where a dragged tab would land: a two point accent line down the gap
/// between two tabs.
///
/// Drawn by the window after every pane, rather than by the strip, for the reason
/// `components::splitter` records about dividers — and because the strip a tab is dropped on is
/// often not the strip it was picked up from, so no one strip can decide where the mark goes.
pub fn insertion_mark(painter: &egui::Painter, strip: &Strip, position: usize) {
    let x = match (strip.tabs.get(position.wrapping_sub(1)), strip.tabs.get(position)) {
        // Between two tabs: in the gap.
        (Some(before), Some(_)) => before.right() + GAP / 2.0,
        // After the last tab.
        (Some(before), None) => before.right() + GAP / 2.0,
        // Before the first, or an empty strip.
        (None, Some(after)) => after.left() - GAP / 2.0,
        (None, None) => strip.area.left() + 2.0,
    };
    let x = x.clamp(strip.area.left() + 1.0, strip.area.right() - 2.0);
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(x - 1.0, strip.area.top() + 3.0),
            Vec2::new(2.0, strip.area.height() - 6.0),
        ),
        CornerRadius::same(1),
        color::accent(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-2277`: a page title as long as a sentence is cut to [`NAME_MOST`] with an ellipsis after it,
    /// and a short name is drawn whole.
    #[test]
    fn a_long_name_is_cut_short_and_a_short_one_is_left_alone() {
        let context = egui::Context::default();
        let font = egui::FontId::proportional(12.0);
        let long = "Rust Programming Language Documentation and Every Chapter of the Book, Online";
        let mut shown = (String::new(), String::new(), 0.0);
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter();
            let cut = name_on_the_tab(painter, long, &font);
            let width = painter.layout_no_wrap(cut.clone(), font.clone(), Color32::WHITE).size().x;
            shown = (cut, name_on_the_tab(painter, "main.rs", &font), width);
        });
        output.textures_delta.clear();
        let (cut, short, width) = shown;
        assert!(cut.ends_with('\u{2026}'), "{cut}");
        assert!(long.starts_with(cut.trim_end_matches('\u{2026}')), "{cut}");
        assert!(width <= NAME_MOST, "{width}");
        assert!(cut.chars().count() > 10, "cut far too short: {cut}");
        assert_eq!(short, "main.rs");
    }

    /// Three 100 point tabs starting at zero, which is what the strip lays out.
    fn strip_of_three() -> Strip {
        Strip {
            area: Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(400.0, HEIGHT)),
            tabs: (0..3)
                .map(|index| {
                    Rect::from_min_size(
                        Pos2::new(index as f32 * 102.0, 0.0),
                        Vec2::new(100.0, HEIGHT),
                    )
                })
                .collect(),
        }
    }

    /// **A tab goes after every tab whose middle the pointer has passed.** That is what makes a
    /// rearrangement follow the pointer instead of jumping when it crosses an edge.
    #[test]
    fn a_drop_lands_after_every_tab_it_has_passed_the_middle_of() {
        let strip = strip_of_three();
        assert_eq!(strip.position_at(0.0), 0, "before the first tab");
        assert_eq!(strip.position_at(49.0), 0, "the left half of the first tab");
        assert_eq!(strip.position_at(51.0), 1, "the right half of the first tab");
        assert_eq!(strip.position_at(153.0), 2, "the right half of the second");
        assert_eq!(strip.position_at(399.0), 3, "past the end of them all");
    }

    /// A strip with nothing in it takes a tab at position zero, which is what dropping into an empty
    /// pane has to mean.
    #[test]
    fn an_empty_strip_takes_a_tab_at_the_start() {
        let empty = Strip { area: strip_of_three().area, tabs: Vec::new() };
        assert_eq!(empty.position_at(200.0), 0);
    }

    #[test]
    fn tabs_that_fit_are_not_shifted() {
        assert_eq!(shift(&[100.0, 100.0], 1, 400.0), 0.0);
    }

    #[test]
    fn a_tab_off_the_right_hand_end_is_brought_into_view() {
        // Three 100 point tabs in 250 points. The third runs from 204 to 304, so the strip has to
        // move 54 points left for its right edge to be reachable.
        let widths = [100.0, 100.0, 100.0];
        let offset = shift(&widths, 2, 250.0);
        assert!((offset - 54.0).abs() < 0.01, "expected 54, got {offset}");
    }

    #[test]
    fn the_first_tab_is_never_shifted_off_its_own_left_edge() {
        let widths = [100.0, 100.0, 100.0];
        assert_eq!(shift(&widths, 0, 50.0), 0.0, "shifting past a tab's left edge hides it");
    }

    #[test]
    fn an_index_past_the_end_shifts_nothing() {
        assert_eq!(shift(&[100.0, 100.0, 100.0], 9, 50.0), 0.0);
    }

    /// `task-2270`: a strip scrolled by the wheel stays where it was put while the tab showing is in view, and
    /// moves the least distance that shows a tab once it is chosen.
    #[test]
    fn a_tab_that_is_chosen_is_brought_into_view_the_least_distance() {
        let widths = [100.0, 100.0, 100.0, 100.0, 100.0];
        // The second tab is in view at an offset of 50, so nothing moves.
        assert_eq!(keep_in_view(&widths, 1, 250.0, 50.0), 50.0);
        // The first tab is partly off the left, so the strip moves back to its left edge.
        assert_eq!(keep_in_view(&widths, 0, 250.0, 50.0), 0.0);
        // The fourth tab ends at 406, past 50 + 250, so the strip moves just far enough to show its right edge.
        assert_eq!(keep_in_view(&widths, 3, 250.0, 50.0), 156.0);
        // A tab wider than the strip is shown from its left edge.
        assert_eq!(keep_in_view(&[600.0], 0, 250.0, 0.0), 0.0);
    }
}
