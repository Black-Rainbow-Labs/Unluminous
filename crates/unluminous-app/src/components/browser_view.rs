//! The small Unluminous-owned toolbar above a native browser child view.
//!
//! ## It is drawn whether or not there is a page, and that is the whole of `task-1905`
//!
//! *"I need a url address bar and back/forward and reload buttons at the top."* A browser node on the
//! canvas drew none of them, because the window returned before reaching this function whenever the node
//! had no tab yet — and the only ways to give a node a tab were `space browser <node> go --url …` and
//! `space add browser --url …`. So the node in the report is not a node whose toolbar is missing; it is a
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

use egui::{Align2, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use crate::services::browser::{BrowserCommand, BrowserPlacement, BrowserTab};
use crate::theme::{color, size};

const TOOLBAR_HEIGHT: f32 = 38.0;
const BUTTON_SIZE: f32 = 26.0;

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
    glyph: &'a str,
    enabled: bool,
    command: BrowserCommand,
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
    ui.painter().rect_filled(strip, CornerRadius::ZERO, color::toolbar());
    ui.painter().line_segment([Pos2::new(strip.left(), strip.bottom()), strip.right_bottom()], Stroke::new(1.0, color::divider()));
    let mut outcome = Outcome::default();
    let mut left = strip.left() + 6.0;
    draw_button(ui, &mut left, strip, Button { name: "Back", glyph: "‹", enabled: toolbar.can_go_back(), command: BrowserCommand::Back }, &mut outcome);
    draw_button(ui, &mut left, strip, Button { name: "Forward", glyph: "›", enabled: toolbar.can_go_forward(), command: BrowserCommand::Forward }, &mut outcome);
    draw_button(ui, &mut left, strip, Button { name: "Reload", glyph: "↻", enabled: toolbar.tab.is_some(), command: BrowserCommand::Reload }, &mut outcome);
    address_field(ui, &mut left, strip, &mut toolbar, &mut outcome);

    // **No page and nothing to place.** A native child view is placed where a tab is drawn, and a node
    // with no tab has none to place — answering with a placement for a tab that does not exist would ask
    // the host to point its one view at nothing.
    let Some(tab) = toolbar.tab else {
        ui.painter().rect_filled(browser, CornerRadius::ZERO, color::editor());
        ui.painter().text(browser.center(), Align2::CENTER_CENTER, "Type an address above to open a page.", FontId::proportional(12.0), color::text_faint());
        return (outcome, None);
    };
    let page = ui.interact(browser, ui.id().with(("browser-page", tab.id)), Sense::click());
    outcome.took_focus |= page.clicked();
    // A window has one native view, so a second rendered tab beside this one in a split pane has
    // nothing to draw here. It says so rather than showing an empty rectangle.
    if !showing {
        ui.painter().rect_filled(browser, CornerRadius::ZERO, color::editor());
        ui.painter().text(browser.center(), Align2::CENTER_CENTER, "This page is showing in the other pane.", FontId::proportional(13.0), color::text_faint());
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
    ui: &mut egui::Ui,
    left: &mut f32,
    strip: Rect,
    toolbar: &mut Toolbar<'_>,
    outcome: &mut Outcome,
) {
    let field = Rect::from_min_max(Pos2::new(*left + 6.0, strip.top() + 6.0), Pos2::new(strip.right() - 8.0, strip.bottom() - 6.0));
    ui.painter().rect(field, CornerRadius::same(size::CONTROL_CORNER), color::field(), Stroke::new(1.0, color::control_border()), egui::StrokeKind::Inside);
    // The whole rectangle claims the press and hands the keyboard over on the next frame, which is
    // `controls::field_takes_the_whole_rectangle`'s own rule and the fault `task-1795` fixed in the
    // nineteen fields that came before this one.
    //
    // **Measured at the size the words are really set in**, which is this address bar's own 12 points and
    // not the interface's row height. `task-1914` reported the difference: with
    // `appearance.ui.font.size` at 24 the strip was 28 points tall inside a 22 point field, egui laid the
    // 12 point text out at the top of it, and the address sat above centre with its top clipped by the
    // field's own border. See `controls::field_text_rect_at`.
    let text_rect = crate::components::controls::field_takes_the_whole_rectangle_at(ui, field, 9.0, toolbar.id, &ADDRESS_FONT);
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
    let response = inner.add(
        egui::TextEdit::singleline(toolbar.typed)
            .id(toolbar.id)
            .hint_text(egui::RichText::new("Type an address").color(color::text_faint()).size(ADDRESS_TEXT))
            .font(ADDRESS_FONT)
            .frame(egui::Frame::NONE)
            .desired_width(text_rect.width())
            .text_color(color::text_control()),
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
    }
    // **What the tab really says**, which is what every address bar puts back.
    if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        *toolbar.editing = false;
    }
    // The field says where the page is, so it follows a redirect, a link and `space browser … go` without
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
        ui.painter().with_clip_rect(field).text(Pos2::new(field.right() - 6.0, field.center().y), Align2::RIGHT_CENTER, "Loading", FontId::proportional(10.5), color::text_faint());
    }
    *left = field.right();
}

/// Draw one compact navigation button and record a click when it is available.
fn draw_button(ui: &mut egui::Ui, left: &mut f32, toolbar: Rect, button: Button<'_>, outcome: &mut Outcome) {
    let area = Rect::from_min_size(Pos2::new(*left, toolbar.top() + 6.0), Vec2::splat(BUTTON_SIZE));
    let response = ui.interact(area, ui.id().with(("browser", button.name)), Sense::click());
    let fill = if response.hovered() && button.enabled { color::control() } else { egui::Color32::TRANSPARENT };
    ui.painter().rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), fill);
    let text = if button.enabled { color::text_control() } else { color::text_faint() };
    ui.painter().text(area.center(), Align2::CENTER_CENTER, button.glyph, FontId::proportional(19.0), text);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, button.enabled, button.name));
    // **Pressing a toolbar button is using this node**, so it takes the focus as clicking the page does.
    // Without that, a button or the address field on a node that does not own the one native view left the
    // node unselected — and `BrowserHost` refuses to drive a tab it is not pointed at, so Enter answered
    // "that rendered tab is not the one showing". The Codex Sol review of `task-1905` found it.
    if response.clicked() {
        outcome.took_focus = true;
    }
    if button.enabled && response.clicked() { outcome.command = Some(button.command); }
    *left = area.right() + 2.0;
}
