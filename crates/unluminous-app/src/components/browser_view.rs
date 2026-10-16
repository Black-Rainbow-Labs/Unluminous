//! The small Unluminous-owned toolbar above a native browser child view.
//!
//! ## It is drawn whether or not there is a page, and that is the whole of `task-1905`
//!
//! *"I need a url address bar and back/forward and reload buttons at the top."* A browser node on the
//! canvas drew none of them, because the window returned before reaching this function whenever the node
//! had no tab yet — and the only ways to give a node a tab were `realm browser <node> go --url …` and
//! `realm add browser --url …`. So the node in the report is not a node whose toolbar is missing; it is a
//! node with no toolbar because it has no page, and no way to get one.
//!
//! So the toolbar is the node's own furniture, in the same way a terminal node's header is: it is drawn
//! from [`Toolbar`], which holds a tab when there is one and nothing when there is not. With no tab the
//! three buttons are **dimmed** rather than absent, because they are controls that will apply the moment
//! a page opens — which is `design/style-guide.md`'s distinction: absent is for a control that can never
//! apply, dimmed is for one that could be used in a moment.
//!
//! ## The address is a field
//!
//! It was a painted rectangle with the address written in it. It is now a real `egui::TextEdit` through
//! `controls::field_takes_the_whole_rectangle`, which is the one path all nineteen text boxes in
//! Unluminous go through since `task-1795`: a press anywhere in the field's own padding claims the
//! keyboard, and it is handed over on the **next** frame through `app::hold_the_keyboard`, because
//! handing it over inside the press does nothing — a `TextEdit` created later in the same frame sees a
//! click that was not on it and surrenders the focus it has just been given.
//!
//! `Enter` sends [`BrowserCommand::Go`]. `Escape` puts back what the tab really says, which is what every
//! address bar does. The field is given an **explicit id**, because an id from egui's auto counter shifts
//! when the number of widgets above it changes — the second latent fault `task-1795` records.
//!
//! What is being typed belongs to the caller, because it has to outlive a frame in which this is not
//! drawn at all: a node scrolled off the canvas stops being drawn, and `egui`'s memory is not where a
//! half-typed address should live.
//!
//! ## Where a person has been, and one element of a page
//!
//! `task-2203` added two things. While the field is being typed into, the addresses visited in every
//! project that match what is typed are listed under it (`services::browser_session::Visits`); `Up` and
//! `Down` choose one and `Enter` or a click goes there. And a browser **node** has a crosshair button at
//! the right hand end, which opens the element picker in the page, and a second button while it shows one
//! element, which shows the whole page again. A tab in the editing area has neither, because pinning is a
//! node's feature and a control that can never apply is absent.
//!
//! ## Drawn in the language of the Agent-Tasks and Agent-Chat panes (`task-2255`)
//!
//! *"The browser tab buttons, refresh, and url bar need to be redesigned to match our new style."* The strip
//! is the pane's own ground with the window's groove under it, `Back`, `Forward` and `Reload` are 32 point
//! round chips raised out of it with `rux`'s line marks, and the address is a carved pill 34 points tall with
//! a mark at its left, which is the board's search well. A button that cannot be used now is drawn flat with
//! its mark in the faintest ink, rather than raised, so the difference is the surface and not only the colour.

use egui::{Align2, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use rux::icon::{Icon, Mark as RuxMark};

use crate::services::browser::{BrowserCommand, BrowserPlacement, BrowserTab};
use crate::theme::crisp::CrispPainter;
use crate::theme::{color, size};
use crate::components::controls::TextBoxes;

const TOOLBAR_HEIGHT: f32 = 46.0;
const BUTTON_SIZE: f32 = 32.0;
/// How tall the address well is.
const WELL_HEIGHT: f32 = 34.0;

/// What the toolbar is drawn from: a tab when there is one, and what is being typed either way.
pub struct Toolbar<'a> {
    /// The tab, when a page has been opened. `None` before one has.
    pub tab: Option<&'a BrowserTab>,
    /// What is in the address field, which the caller owns because it is being typed into.
    pub typed: &'a mut String,
    /// Whether what is in the field is the person's rather than the page's.
    ///
    /// Owned by the caller for the same reason `typed` is: a node scrolled off the canvas is not drawn, and
    /// a half-typed address must not be thrown away because nobody was looking at it.
    pub editing: &'a mut bool,
    /// Which widget id the field takes, so it is stable across frames and unique between two of these.
    pub id: egui::Id,
    /// Whether this is a browser node, which is what offers the element picker. `task-2203`.
    pub pickable: bool,
    /// Whether the node is showing one element of its page rather than the whole of it.
    pub pinned: bool,
    /// Whether the element picker is open in this page now.
    pub picking: bool,
    /// The visited addresses that match what is being typed, best first. Empty while nothing is typed.
    pub suggestions: Vec<Suggestion>,
}

/// One visited address offered under the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub url: String,
    pub title: String,
}

impl Toolbar<'_> {
    /// Whether what is in the field is the person's rather than the page's.
    fn editing(&self) -> bool {
        *self.editing
    }

    fn can_go_back(&self) -> bool {
        self.tab.is_some_and(BrowserTab::can_go_back)
    }

    fn can_go_forward(&self) -> bool {
        self.tab.is_some_and(BrowserTab::can_go_forward)
    }

    fn loading(&self) -> bool {
        self.tab.is_some_and(|tab| tab.loading)
    }

    /// The address the tab is really on, which is what `Escape` puts back and what the field opens with.
    fn address(&self) -> &str {
        self.tab.map(BrowserTab::current_url).unwrap_or_default()
    }
}

/// What the browser toolbar asked the window to do this frame.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub command: Option<BrowserCommand>,
    pub took_focus: bool,
}

/// The content and command of one compact navigation control.
struct Button<'a> {
    name: &'a str,
    mark: Mark,
    enabled: bool,
    command: BrowserCommand,
}

/// What is drawn on a button: one of `rux`'s line marks, or one of the drawn marks in `theme::icon`.
enum Mark {
    Line(Icon),
    Drawn(fn(&egui::Painter, Pos2, egui::Color32)),
}

/// How much of the strip's right hand end the picker's buttons take, which the field stops short of.
fn room_for_the_picker(toolbar: &Toolbar<'_>) -> f32 {
    match (toolbar.pickable, toolbar.pinned) {
        (false, _) => 0.0,
        (true, false) => BUTTON_SIZE + 6.0,
        (true, true) => 2.0 * BUTTON_SIZE + 8.0,
    }
}

/// Draw navigation controls and return the rectangle reserved for the native child view.
pub fn show(
    ui: &mut egui::Ui,
    area: Rect,
    mut toolbar: Toolbar<'_>,
    focused: bool,
    showing: bool,
) -> (Outcome, Option<BrowserPlacement>) {
    let strip = Rect::from_min_size(area.min, Vec2::new(area.width(), TOOLBAR_HEIGHT));
    let browser = Rect::from_min_max(Pos2::new(area.left(), strip.bottom()), area.max);
    let chat = crate::components::chrome_kit::chat();
    ui.painter().rect_filled(strip, CornerRadius::ZERO, chat.ground);
    crate::components::chrome_kit::groove(
        ui.painter(),
        Pos2::new(strip.left(), strip.bottom() - 1.0),
        Pos2::new(strip.right(), strip.bottom() - 1.0),
    );
    let mut outcome = Outcome::default();
    let salt = ("browser-toolbar", toolbar.id);
    crate::components::chrome_kit::layer(ui, salt, strip, 12.0, |rux| {
        let mut left = strip.left() + 8.0;
        for button in [
            Button {
                name: "Back",
                mark: Mark::Line(Icon::ChevLeft),
                enabled: toolbar.can_go_back(),
                command: BrowserCommand::Back,
            },
            Button {
                name: "Forward",
                mark: Mark::Line(Icon::ChevRight),
                enabled: toolbar.can_go_forward(),
                command: BrowserCommand::Forward,
            },
            Button {
                name: "Reload",
                mark: Mark::Line(Icon::Rerun),
                enabled: toolbar.tab.is_some(),
                command: BrowserCommand::Reload,
            },
        ] {
            draw_button(rux, &mut left, strip, button, &mut outcome);
        }
        address_field(rux, &mut left, strip, &mut toolbar, &mut outcome);
        picker_buttons(rux, strip, &toolbar, &mut outcome);
    });

    // **No page and nothing to place.** A native child view is placed where a tab is drawn, and a node
    // with no tab has none to place — answering with a placement for a tab that does not exist would ask
    // the host to point its one view at nothing.
    let Some(tab) = toolbar.tab else {
        ui.painter().rect_filled(browser, CornerRadius::ZERO, color::editor());
        ui.painter().crisp_text(
            browser.center(),
            Align2::CENTER_CENTER,
            "Type an address above to open a page.",
            FontId::proportional(12.0),
            color::text_faint(),
        );
        return (outcome, None);
    };
    let page = ui.interact(browser, ui.id().with(("browser-page", tab.id)), Sense::click());
    outcome.took_focus |= page.clicked();
    // A window has one native view, so a second rendered tab beside this one in a split pane has
    // nothing to draw here. It says so rather than showing an empty rectangle.
    if !showing {
        ui.painter().rect_filled(browser, CornerRadius::ZERO, color::editor());
        // Wrapped to the pane and clipped to it, because a split pane can be narrower than the sentence and the
        // words ran into the pane beside it (`task-2263`).
        let painter = ui.painter_at(browser);
        let words = painter.layout(
            "Click to show this page here. A window shows one live page at a time.".to_owned(),
            FontId::proportional(13.0),
            color::text_faint(),
            (browser.width() - 24.0).max(40.0),
        );
        let at = browser.center() - words.size() / 2.0;
        painter.galley(at, words, color::text_faint());
    }
    // **A page in a pane fills its pane, so nothing of it is cut.** A node's own placement is cropped by
    // `show_a_browser_node`, which is where the canvas is and where the pane's edge is known.
    (outcome, Some(BrowserPlacement::whole(tab.id, browser, focused)))
}

/// How large the address is set, which is the size the field measures its own row at.
///
/// One constant rather than the number written in three places, because the box, the hint and the
/// strip the box is laid out in all have to agree — see `controls::field_text_rect_at`.
const ADDRESS_TEXT: f32 = 12.0;

/// The address's font, built from [`ADDRESS_TEXT`].
const ADDRESS_FONT: FontId = FontId::proportional(ADDRESS_TEXT);

/// The address field: what the tab is on, or what is being typed over it.
///
/// The words `Loading ·` are drawn **beside** the field rather than into it, because a field's text is
/// what is being typed and a caller who typed while a page was loading would have had the word inserted
/// into their own address.
fn address_field(
    rux: &mut rux::Rux<'_>,
    left: &mut f32,
    strip: Rect,
    toolbar: &mut Toolbar<'_>,
    outcome: &mut Outcome,
) {
    let well = Rect::from_min_max(
        Pos2::new(*left + 6.0, strip.center().y - WELL_HEIGHT / 2.0),
        Pos2::new(
            strip.right() - 10.0 - room_for_the_picker(toolbar),
            strip.center().y + WELL_HEIGHT / 2.0,
        ),
    );
    let chat = rux.theme().chat;
    let ink = rux.theme().ink;
    rux.chrome.surface(well, well.height() / 2.0, chat.well, chat.carved_sm());
    rux.mark(
        RuxMark::new(Icon::Search, 13.0),
        Pos2::new(well.left() + 18.0, well.center().y),
        ink.i400,
    );
    // The words start after the mark, and stop short of the round end.
    let field = Rect::from_min_max(
        Pos2::new(well.left() + 30.0, well.top() + 4.0),
        Pos2::new(well.right() - 12.0, well.bottom() - 4.0),
    );
    let ui = &mut *rux.ui;
    // The whole rectangle claims the press and hands the keyboard over on the next frame, which is
    // `controls::field_takes_the_whole_rectangle`'s own rule and the fault `task-1795` fixed in the
    // nineteen fields that came before this one.
    //
    // **Measured at the size the words are really set in**, which is this address bar's own 12 points and
    // not the interface's row height. `task-1914` reported the difference: with
    // `appearance.ui.font.size` at 24 the strip was 28 points tall inside a 22 point field, egui laid the
    // 12 point text out at the top of it, and the address sat above centre with its top clipped by the
    // field's own border. See `controls::field_text_rect_at`.
    let text_rect = crate::components::controls::field_takes_the_whole_rectangle_at(
        ui,
        field,
        0.0,
        toolbar.id,
        "Address field",
        &ADDRESS_FONT,
    );
    // **The list's keys are taken before the field sees them**, because a single line `TextEdit` reads `Up`
    // and `Down` as moving the cursor to the ends and `Enter` as giving up the focus.
    let offered = suggestions_open(ui, toolbar) && !toolbar.suggestions.is_empty();
    let chosen_id = toolbar.id.with("suggestion");
    let mut chosen: Option<usize> = ui.data(|data| data.get_temp(chosen_id)).flatten();
    let mut chose_by_key = None;
    if offered {
        let last = toolbar.suggestions.len() - 1;
        ui.input_mut(|input| {
            if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                chosen = Some(chosen.map_or(0, |at| (at + 1).min(last)));
            }
            if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                chosen = chosen.and_then(|at| at.checked_sub(1));
            }
            if chosen.is_some() && input.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                chose_by_key = chosen;
            }
        });
    } else {
        chosen = None;
    }
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
    let response = inner.text_box(
        egui::TextEdit::singleline(toolbar.typed)
            .id(toolbar.id)
            .hint_text(crate::components::controls::placeholder(
                "Type an address",
                &ADDRESS_FONT,
                color::text_faint(),
            ))
            .font(ADDRESS_FONT)
            .frame(egui::Frame::NONE)
            .desired_width(text_rect.width())
            .text_color(ink.i900),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Address"));
    // Typing in a node's address bar is using that node — see `draw_button` for why the focus has to follow.
    if response.gained_focus() || response.changed() {
        outcome.took_focus = true;
    }
    // **`lost_focus`, not `has_focus`, and that is the whole of reading Enter in a field.** A singleline
    // `TextEdit` handles `return_key` itself: it calls `surrender_focus` and **breaks out of the event
    // loop**, consuming the press. So on the frame Enter arrives the box no longer has the focus and the
    // key is not in the frame's input either — asking `has_focus()` was asking a condition that cannot be
    // true, which is `task-1678`'s trap in a different library and `task-1771`'s in the same one. It is
    // the idiom egui's own `TextEdit` documentation opens with. Measured on the real window: the address
    // was typed, Enter did nothing, and the sentence below then wiped the field.
    let entered = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    if entered && !toolbar.typed.trim().is_empty() {
        outcome.command = Some(BrowserCommand::Go(toolbar.typed.trim().to_owned()));
        // Entered, so the field is the page's again: what it holds is the address being navigated to, and
        // the tab arriving there is what fills it in.
        *toolbar.editing = false;
    }
    // Typing makes the field the person's until they enter it or put it back.
    if response.changed() {
        *toolbar.editing = true;
        chosen = None;
        set_suggestions_open(ui, toolbar.id, true);
    }
    if response.gained_focus() {
        set_suggestions_open(ui, toolbar.id, true);
    }
    // A visited address chosen from the list, by `Enter` on the one chosen with the arrows or by a click.
    let clicked = match offered {
        true => suggestions(ui, well, toolbar, chosen),
        false => None,
    };
    if let Some(at) = chose_by_key.or(clicked) {
        if let Some(suggestion) = toolbar.suggestions.get(at) {
            outcome.command = Some(BrowserCommand::Go(suggestion.url.clone()));
            *toolbar.typed = suggestion.url.clone();
            *toolbar.editing = false;
            outcome.took_focus = true;
        }
        chosen = None;
        set_suggestions_open(ui, toolbar.id, false);
        ui.memory_mut(|memory| memory.surrender_focus(toolbar.id));
    }
    if entered || ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        set_suggestions_open(ui, toolbar.id, false);
    }
    ui.data_mut(|data| data.insert_temp(chosen_id, chosen));
    // **What the tab really says**, which is what every address bar puts back.
    if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        *toolbar.editing = false;
    }
    // The field says where the page is, so it follows a redirect, a link and `realm browser … go` without
    // the caller having to write it back.
    //
    // **Only while nothing has been typed over it**, which is the difference between following the page and
    // throwing somebody's work away. Written as "nobody has the focus", it wiped a half-typed address the
    // moment the focus went anywhere else — pressing Reload, clicking another node, clicking a pane — and
    // Escape is what is *for* putting the address back. The Codex Sol review of `task-1905` found it; the
    // frame Enter was pressed was already excluded for the same reason, and this is the general form of it.
    //
    // `Toolbar::typed` is the person's until they clear it or enter it, and `showing` is what says whether
    // it is theirs: the caller keeps it, so it survives the node not being drawn.
    if !toolbar.editing() {
        *toolbar.typed = toolbar.address().to_owned();
    }
    if toolbar.loading() {
        ui.painter().with_clip_rect(well).crisp_text(
            Pos2::new(well.right() - 14.0, well.center().y),
            Align2::RIGHT_CENTER,
            "Loading",
            FontId::proportional(10.5),
            ink.i400,
        );
    }
    *left = well.right();
}

/// Whether the list of visited addresses is open under this field.
///
/// Open from the moment the field is typed into or given the focus until an address is chosen, `Enter` or
/// `Escape` is pressed, or the pointer is pressed somewhere that is neither the field nor the list. Kept in
/// `egui`'s memory rather than read off the field's focus, because pressing a row in the list takes the focus
/// off the field a frame before the click is seen.
fn suggestions_open(ui: &egui::Ui, toolbar: &Toolbar<'_>) -> bool {
    let open =
        ui.data(|data| data.get_temp::<bool>(toolbar.id.with("suggesting"))).unwrap_or(false);
    let focused = ui.memory(|memory| memory.has_focus(toolbar.id));
    let area = ui.data(|data| data.get_temp::<Rect>(toolbar.id.with("suggestions-area")));
    let field = ui.data(|data| data.get_temp::<Rect>(toolbar.id.with("field-area")));
    let pressed_elsewhere = ui.input(|input| {
        input.pointer.any_pressed()
            && input.pointer.interact_pos().is_some_and(|at| {
                !area.is_some_and(|area| area.contains(at))
                    && !field.is_some_and(|field| field.contains(at))
            })
    });
    if pressed_elsewhere && !focused {
        set_suggestions_open(ui, toolbar.id, false);
        return false;
    }
    open && toolbar.editing()
}

fn set_suggestions_open(ui: &egui::Ui, id: egui::Id, open: bool) {
    ui.data_mut(|data| data.insert_temp(id.with("suggesting"), open));
}

/// Height of one row in the list of visited addresses.
const SUGGESTION_ROW: f32 = 34.0;

/// The list of visited addresses under the field, and the row a click chose.
///
/// **Drawn in the window's own points on a layer of its own**, so it sits over the page and over whatever
/// is below the node. A browser node is drawn into a layer carrying the canvas's camera, so the field's
/// rectangle is mapped through that layer's transform first. A native page is cut around the list, which is
/// what `UnluminousApp::occluding_rects` already does for every popup.
fn suggestions(
    ui: &egui::Ui,
    field: Rect,
    toolbar: &Toolbar<'_>,
    chosen: Option<usize>,
) -> Option<usize> {
    let to_screen = ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap_or_default();
    let field_on_screen = to_screen * field;
    ui.data_mut(|data| data.insert_temp(toolbar.id.with("field-area"), field_on_screen));
    let width = field_on_screen.width().max(260.0);
    let height = SUGGESTION_ROW * toolbar.suggestions.len() as f32 + 8.0;
    let top_left = Pos2::new(field_on_screen.left(), field_on_screen.bottom() + 4.0);
    let mut picked = None;
    let area = egui::Area::new(toolbar.id.with("suggestions"))
        .order(egui::Order::Foreground)
        .fixed_pos(top_left)
        .show(ui.ctx(), |ui| {
            let outer = Rect::from_min_size(top_left, Vec2::new(width, height));
            ui.painter().rect(
                outer,
                CornerRadius::same(size::CONTROL_CORNER),
                color::menu(),
                Stroke::new(1.0, color::control_border()),
                egui::StrokeKind::Inside,
            );
            for (at, suggestion) in toolbar.suggestions.iter().enumerate() {
                let row = Rect::from_min_size(
                    Pos2::new(outer.left() + 4.0, outer.top() + 4.0 + SUGGESTION_ROW * at as f32),
                    Vec2::new(width - 8.0, SUGGESTION_ROW),
                );
                let response =
                    ui.interact(row, toolbar.id.with(("suggestion-row", at)), Sense::click());
                if response.hovered() || chosen == Some(at) {
                    ui.painter().rect_filled(
                        row,
                        CornerRadius::same(size::CONTROL_CORNER),
                        color::control(),
                    );
                }
                // Cut at the sides only, so a long address stops at the list's edge and no line is clipped.
                let painter = ui.painter().with_clip_rect(row.shrink2(Vec2::new(6.0, 0.0)));
                let title = match suggestion.title.trim().is_empty() {
                    true => suggestion.url.as_str(),
                    false => suggestion.title.trim(),
                };
                painter.crisp_text(
                    Pos2::new(row.left() + 8.0, row.top() + 10.0),
                    Align2::LEFT_CENTER,
                    title,
                    FontId::proportional(12.0),
                    color::text_control(),
                );
                painter.crisp_text(
                    Pos2::new(row.left() + 8.0, row.top() + 24.0),
                    Align2::LEFT_CENTER,
                    &suggestion.url,
                    FontId::proportional(10.5),
                    color::text_faint(),
                );
                let name = format!("Visited: {}", suggestion.url);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name)
                });
                if response.clicked() {
                    picked = Some(at);
                }
            }
            ui.allocate_rect(outer, Sense::hover());
        });
    ui.data_mut(|data| data.insert_temp(toolbar.id.with("suggestions-area"), area.response.rect));
    picked
}

/// The picker's buttons at the right hand end of a node's toolbar: the crosshair that opens the element
/// picker, and while one element is shown alone, the button that shows the whole page again.
fn picker_buttons(
    rux: &mut rux::Rux<'_>,
    strip: Rect,
    toolbar: &Toolbar<'_>,
    outcome: &mut Outcome,
) {
    if !toolbar.pickable {
        return;
    }
    let mut left = strip.right() - 6.0 - room_for_the_picker(toolbar);
    let has_a_page = toolbar.tab.is_some();
    let picking = toolbar.picking;
    let area = Rect::from_center_size(
        Pos2::new(left + BUTTON_SIZE / 2.0, strip.center().y),
        Vec2::splat(BUTTON_SIZE),
    );
    draw_button(
        rux,
        &mut left,
        strip,
        Button {
            name: if picking { "Stop Selecting" } else { "Select Element" },
            mark: Mark::Drawn(crate::theme::icon::crosshair),
            enabled: has_a_page,
            command: if picking { BrowserCommand::CancelPick } else { BrowserCommand::Pick },
        },
        outcome,
    );
    // **Lit while the picker is open**: carved in, with the mark in the sky colour.
    if picking {
        let chat = rux.theme().chat;
        rux.chrome.surface(area, area.height() / 2.0, chat.well, chat.carved_sm());
        crate::theme::icon::crosshair(rux.painter(), area.center(), chat.sky);
    }
    if toolbar.pinned {
        draw_button(
            rux,
            &mut left,
            strip,
            Button {
                name: "Show Whole Page",
                mark: Mark::Drawn(crate::theme::icon::whole_page),
                enabled: has_a_page,
                command: BrowserCommand::Unpin,
            },
            outcome,
        );
    }
}

/// Draw one round navigation button and record a click when it is available.
///
/// Raised out of the strip while it can be used, carved in while it is held down, and flat with its mark in
/// the faintest ink while it cannot be used. The mark is the sky colour under the pointer, which is
/// `rux::components::ChipButton`'s own answer.
fn draw_button(
    rux: &mut rux::Rux<'_>,
    left: &mut f32,
    toolbar: Rect,
    button: Button<'_>,
    outcome: &mut Outcome,
) {
    let area = Rect::from_center_size(
        Pos2::new(*left + BUTTON_SIZE / 2.0, toolbar.center().y),
        Vec2::splat(BUTTON_SIZE),
    );
    let response =
        rux.ui.interact(area, rux.ui.id().with(("browser", button.name)), Sense::click());
    let chat = rux.theme().chat;
    let ink = rux.theme().ink;
    let held = response.is_pointer_button_down_on();
    match (button.enabled, held) {
        (true, true) => rux.chrome.surface(area, area.height() / 2.0, chat.well, chat.carved_sm()),
        (true, false) => rux::components::chip_disc(rux, area),
        (false, _) => {}
    }
    if button.enabled && response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let tint = match (button.enabled, response.hovered()) {
        (false, _) => ink.i300,
        (true, true) => chat.sky,
        (true, false) => ink.i500,
    };
    match button.mark {
        Mark::Line(icon) => rux.mark(RuxMark::new(icon, 13.0).stroke(1.8), area.center(), tint),
        Mark::Drawn(draw) => draw(rux.painter(), area.center(), tint),
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, button.enabled, button.name)
    });
    // **Pressing a toolbar button is using this node**, so it takes the focus as clicking the page does.
    // Without that, a button or the address field on a node that does not own the one native view left the
    // node unselected — and `BrowserHost` refuses to drive a tab it is not pointed at, so Enter answered
    // "that rendered tab is not the one showing". The Codex Sol review of `task-1905` found it.
    if response.clicked() {
        outcome.took_focus = true;
    }
    if button.enabled && response.clicked() {
        outcome.command = Some(button.command);
    }
    *left = area.right() + 6.0;
}
