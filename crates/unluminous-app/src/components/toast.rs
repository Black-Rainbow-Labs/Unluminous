//! Dismissible notices, drawn over the bottom right of the window.
//!
//! **Why the window owns these and no plugin does.** Every provider reports a miss through
//! `plugin_ui::Request::Message`, which is the status bar — and the status bar is the right place for a
//! running commentary and the wrong place for the one thing somebody is waiting on. `task-1848` reports
//! it exactly: a chat message that could not be sent left "nothing happens, no error", because the
//! refusal was a sentence in the smallest text at the far bottom edge of the window, replaced by
//! whatever was reported next.
//!
//! Three reasons a toast is the window's rather than each plugin's:
//!
//! - Every plugin has the same problem, so a toast inside one would be a second answer to a question the
//!   window already owns — the rule `components::modal` and `components::controls` are both built on.
//! - A notice has to outlive the pane. A refusal about a send is worth reading after the pane has been
//!   put away, and a plugin draws nothing once it is not showing.
//! - It is drawn after the pane loop, which is where `components::completion` and the value tooltip
//!   already are and for the same reason: egui gives a pointer to the last widget that asked for it, so
//!   anything that must sit above every pane is added last.
//!
//! **A problem stays and a confirmation goes.** A `Kind::Problem` is dismissed by a person and by
//! nothing else, because a failure nobody read is the fault this exists to fix; a `Kind::Done` fades
//! after [`LIFE`]. That asymmetry is the whole design, and it is what stops a toast becoming another
//! thing to ignore.
//!
//! **An offer is the third kind** (`task-2063`), a notice with buttons on it: *"Unluminous 0.55.0 is
//! available"* with `Install & Restart` and `Don't Ask Again`. It stays until it is answered or
//! dismissed, for a problem's reason: it is waiting on a person. Its edge is the accent, because it is
//! neither a failure nor a confirmation but a question. A button is reported to the window as an
//! [`Act`], and the window does it; this file only draws the card and says which button was pressed.
//!
//! **No new colour.** The palette is closed. A problem's edge is `color::close`, the red the window's own
//! close button is drawn in, and a confirmation's is `color::git_added`, the green a new file is counted
//! in — both already sampled from the design.

use egui::{Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use crate::theme::{color, icon};

/// How long a confirmation stays before it fades.
///
/// Long enough to read a sentence twice. A problem has no life at all: see the module comment.
pub const LIFE: std::time::Duration = std::time::Duration::from_secs(6);

/// How many are drawn at once, oldest first to go.
///
/// **Two**, and it was four. Measured against a real failing endpoint, four cards of three lines each
/// reached from the status bar up over the composer — so the control that would fix what was wrong was
/// behind the complaint about it. A repeat is counted rather than stacked now ([`Toasts::say`]), so two is
/// two genuinely different things to read, which is as much as anybody reads at once.
pub const LIMIT: usize = 2;

/// How wide one is, and how far in from the window's edge the stack sits.
const WIDTH: f32 = 320.0;
const MARGIN: f32 = 12.0;
/// The coloured edge down the left of a card.
const EDGE: f32 = 3.0;
const PAD: f32 = 10.0;
/// The cross, and the room kept clear for it so a long sentence does not run under it.
const CROSS: f32 = 16.0;

/// What kind of notice this is, which decides its colour and whether it fades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Something did not happen. Stays until it is dismissed.
    Problem,
    /// Something did. Fades after [`LIFE`].
    Done,
    /// A question with buttons on it. Stays until it is answered or dismissed.
    Offer,
}

/// What a button on an offer asks the window to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    /// Download this version, check it, install it and start it again. `task-2063`.
    InstallUpdate(String),
    /// Stop offering this version. A later one is offered again.
    SkipUpdate(String),
    /// Open a page in the person's own browser, for a platform with nothing to install.
    OpenPage(String),
}

/// What a person did to the stack this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pressed {
    /// The cross on the notice at this index.
    Dismissed(usize),
    /// A button on the offer at this index.
    Acted(usize, Act),
}

impl Kind {
    /// The name a command line and a test use.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Problem => "problem",
            Kind::Done => "done",
            Kind::Offer => "offer",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "problem" | "error" | "failed" => Some(Kind::Problem),
            "done" | "ok" | "success" => Some(Kind::Done),
            "offer" => Some(Kind::Offer),
            _ => None,
        }
    }

    fn edge(self) -> Color32 {
        match self {
            Kind::Problem => color::close(),
            Kind::Done => color::git_added(),
            Kind::Offer => color::accent(),
        }
    }

    /// Whether a notice of this kind goes away on its own.
    fn fades(self) -> bool {
        matches!(self, Kind::Done)
    }
}

/// One notice.
#[derive(Debug, Clone)]
pub struct Notice {
    pub text: String,
    pub kind: Kind,
    /// How many times this same sentence has been raised, drawn as `× 3` when it is more than one.
    pub count: u32,
    /// When it was raised, which is what [`LIFE`] is measured from. `None` in a test that is asserting on
    /// the list rather than on time.
    pub at: Option<std::time::Instant>,
    /// The buttons along the bottom of an offer, each a label and what it asks for. Empty on the
    /// other two kinds.
    pub actions: Vec<(String, Act)>,
}

/// The notices the window is holding, newest last.
///
/// Pure: nothing here draws, and every rule about what is kept can be asserted with no window. `show` is
/// the half that needs one.
#[derive(Debug, Clone, Default)]
pub struct Toasts {
    notices: Vec<Notice>,
}

impl Toasts {
    /// Raise one. The oldest goes when there are more than [`LIMIT`].
    ///
    /// **The same sentence twice is one notice, counted.** Measured with a real failing endpoint: eight
    /// sends against a server answering `HTTP 500` each raised a card, four were drawn, and they stacked up
    /// over the composer — so the control that would let somebody change what was wrong was behind the
    /// complaint about it. A repeat is the commonest shape a failure takes, because whatever caused it is
    /// still true, and four copies of one sentence carry no more than one does.
    pub fn say(&mut self, text: impl Into<String>, kind: Kind) {
        let text = text.into();
        // An empty notice is nothing to read, and a provider that returns an empty failure string
        // should not put a blank card on the screen.
        if text.trim().is_empty() {
            return;
        }
        // The newest of the same kind and words: its count goes up and its clock starts again, so a repeat
        // of a confirmation stays as long as a fresh one would.
        if let Some(same) =
            self.notices.iter_mut().rev().find(|notice| notice.kind == kind && notice.text == text)
        {
            same.count += 1;
            same.at = Some(std::time::Instant::now());
            return;
        }
        self.notices.push(Notice {
            text,
            kind,
            count: 1,
            at: Some(std::time::Instant::now()),
            actions: Vec::new(),
        });
        self.keep_to_the_limit();
    }

    /// Raise an offer: a sentence with buttons. The same sentence already showing is not raised twice.
    pub fn offer(&mut self, text: impl Into<String>, actions: Vec<(String, Act)>) {
        let text = text.into();
        if self.notices.iter().any(|notice| notice.kind == Kind::Offer && notice.text == text) {
            return;
        }
        self.notices.push(Notice {
            text,
            kind: Kind::Offer,
            count: 1,
            at: Some(std::time::Instant::now()),
            actions,
        });
        self.keep_to_the_limit();
    }

    /// Take away every offer, which is what answering one does to the others about the same thing.
    pub fn withdraw_the_offers(&mut self) {
        self.notices.retain(|notice| notice.kind != Kind::Offer);
    }

    /// The oldest go when there are more than [`LIMIT`], but never an offer while anything else can go
    /// instead: a question pushed off the screen by a later confirmation would never be answered.
    fn keep_to_the_limit(&mut self) {
        while self.notices.len() > LIMIT {
            let oldest =
                self.notices.iter().position(|notice| notice.kind != Kind::Offer).unwrap_or(0);
            self.notices.remove(oldest);
        }
    }

    /// Take away the ones that have had their time.
    ///
    /// Called once a frame. A problem is never dropped here — only [`Self::dismiss`] and
    /// [`Self::dismiss_the_newest`] take one away — which is what makes a failure something a person has
    /// to acknowledge rather than something they may have missed.
    pub fn forget_the_stale_ones(&mut self) {
        let now = std::time::Instant::now();
        self.notices.retain(|notice| match (notice.kind.fades(), notice.at) {
            (true, Some(at)) => now.duration_since(at) < LIFE,
            // A notice with no instant is a test's, and time does not pass for it.
            _ => true,
        });
    }

    /// Take away the one at `index`, which is what its cross does.
    pub fn dismiss(&mut self, index: usize) {
        if index < self.notices.len() {
            self.notices.remove(index);
        }
    }

    /// Take away the newest, which is what `Escape` does.
    ///
    /// The newest rather than all of them: `Escape` closes one thing at a time everywhere else in
    /// Unluminous, and somebody who has three failures should read three.
    pub fn dismiss_the_newest(&mut self) -> bool {
        self.notices.pop().is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.notices.is_empty()
    }

    pub fn len(&self) -> usize {
        self.notices.len()
    }

    /// What is showing, oldest first, which is the order they are drawn up the window.
    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }
}

/// Draw the stack into the bottom right of `area`, and answer which cross was pressed.
///
/// Added to the `Ui` **after every pane**, for the reason the module comment gives. The stack grows
/// upwards from just above the status bar, so a new notice appears nearest the place a person is looking
/// and the older ones move away.
pub fn show(ui: &mut egui::Ui, area: Rect, toasts: &Toasts, look_font: f32) -> Option<Pressed> {
    if toasts.is_empty() {
        return None;
    }
    let mut dismissed = None;
    // Bottom upwards, so `notices` in order oldest first ends up with the newest at the bottom of the
    // stack, nearest the status bar.
    let mut bottom = area.bottom() - MARGIN;
    for (index, notice) in toasts.notices().iter().enumerate().rev() {
        let painter = ui.painter().clone();
        // Measured before it is placed, because a two line sentence makes a taller card and the one
        // above it has to move up by that much. `layout` is the same wrapping the painter will do.
        // `× 3` after the sentence when it has happened more than once — see [`Toasts::say`].
        let said = match notice.count {
            0 | 1 => notice.text.clone(),
            many => format!("{}  × {many}", notice.text),
        };
        // An offer is as wide as its row of buttons needs, and never narrower than any other notice:
        // at a large font the two buttons are wider than the card, and they ran out of both sides of
        // it (`task-2063`).
        let font = FontId::proportional(look_font - 1.0);
        let row = button_row(&painter, notice, &font);
        let width = WIDTH.max(row.width + PAD * 2.0 + EDGE).min(area.width() - MARGIN * 2.0);
        let wrapped = painter.layout(
            said,
            font.clone(),
            color::text(),
            width - PAD * 2.0 - EDGE - CROSS - 6.0,
        );
        // An offer has a row of buttons under its sentence.
        let buttons = match notice.actions.is_empty() {
            true => 0.0,
            false => row.height + PAD,
        };
        let height = (wrapped.size().y + PAD * 2.0 + buttons).max(36.0);
        let card = Rect::from_min_size(
            Pos2::new(area.right() - MARGIN - width, bottom - height),
            Vec2::new(width, height),
        );
        // Off the top of the window rather than drawn over the tabs: with the limit at four this only
        // happens in a window shorter than about two hundred points.
        if card.top() < area.top() {
            break;
        }

        painter.rect(
            card,
            CornerRadius::same(crate::theme::size::CONTROL_CORNER),
            color::field(),
            Stroke::new(1.0, color::control_border()),
            egui::StrokeKind::Inside,
        );
        // The coloured edge, inside the card's own corner radius so it does not square off the corners.
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(card.left() + 1.0, card.top() + 1.0),
                Vec2::new(EDGE, card.height() - 2.0),
            ),
            CornerRadius::same(1),
            notice.kind.edge(),
        );
        painter.galley(
            Pos2::new(card.left() + EDGE + PAD, card.top() + PAD),
            wrapped,
            color::text(),
        );

        // The cross, named so a test can press it and so the name says which notice it belongs to.
        let cross_at = Rect::from_center_size(
            Pos2::new(card.right() - PAD - CROSS / 2.0, card.top() + PAD + CROSS / 2.0),
            Vec2::splat(CROSS),
        );
        let name = format!("Dismiss notice {}", index + 1);
        let response = ui.interact(cross_at, ui.id().with(("toast", index)), Sense::click());
        let tint = match response.hovered() {
            true => color::text_strong(),
            false => color::text_dim(),
        };
        icon::cross(&painter, cross_at.center(), tint);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name.clone())
        });
        if response.clicked() {
            dismissed = Some(Pressed::Dismissed(index));
        }
        if let Some(act) = offer_buttons(ui, card, notice, index, look_font) {
            dismissed = Some(Pressed::Acted(index, act));
        }

        bottom = card.top() - 8.0;
    }
    dismissed
}

/// How much wider and taller a button on an offer is than its words.
const BUTTON_PAD: Vec2 = Vec2::new(20.0, 10.0);
/// The gap between two buttons.
const BUTTON_GAP: f32 = 6.0;

/// How much room an offer's row of buttons takes at `font`.
struct Row {
    width: f32,
    height: f32,
}

fn button_row(painter: &egui::Painter, notice: &Notice, font: &FontId) -> Row {
    let sizes: Vec<Vec2> = notice
        .actions
        .iter()
        .map(|(label, _)| painter.layout_no_wrap(label.clone(), font.clone(), color::text()).size())
        .collect();
    let width = sizes.iter().map(|size| size.x + BUTTON_PAD.x).sum::<f32>()
        + BUTTON_GAP * sizes.len().saturating_sub(1) as f32;
    let height = sizes.iter().map(|size| size.y).fold(0.0, f32::max) + BUTTON_PAD.y;
    Row { width, height }
}

/// Draw an offer's buttons along the bottom of its card, right aligned, with the last one filled in the
/// accent, which is the rule `components::modal::footer` keeps for the button that does the thing.
/// Answers the act of the one pressed.
fn offer_buttons(
    ui: &mut egui::Ui,
    card: Rect,
    notice: &Notice,
    index: usize,
    look_font: f32,
) -> Option<Act> {
    let font = FontId::proportional(look_font - 1.0);
    let row = button_row(ui.painter(), notice, &font);
    let mut right = card.right() - PAD;
    let top = card.bottom() - PAD - row.height;
    let mut pressed = None;
    let last = notice.actions.len().saturating_sub(1);
    for (position, (label, act)) in notice.actions.iter().enumerate().rev() {
        let primary = position == last;
        let ink = match primary {
            true => color::text_strong(),
            false => color::text(),
        };
        let words = ui.painter().layout_no_wrap(label.clone(), font.clone(), ink);
        let width = words.size().x + BUTTON_PAD.x;
        let at = Rect::from_min_size(Pos2::new(right - width, top), Vec2::new(width, row.height));
        let id = ui.id().with(("toast-button", index, position));
        let response = ui.interact(at, id, Sense::click());
        let fill = match (primary, response.hovered()) {
            (true, _) => color::accent(),
            (false, true) => color::selected_row(),
            (false, false) => color::control(),
        };
        ui.painter().rect_filled(at, CornerRadius::same(crate::theme::size::CONTROL_CORNER), fill);
        ui.painter().galley(at.center() - words.size() / 2.0, words, ink);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone())
        });
        if response.clicked() {
            pressed = Some(act.clone());
        }
        right = at.left() - BUTTON_GAP;
    }
    pressed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A notice with no instant, so a test asserting on the list is not asserting on the clock.
    fn timeless(text: &str, kind: Kind) -> Notice {
        Notice { text: text.to_owned(), kind, count: 1, at: None, actions: Vec::new() }
    }

    #[test]
    fn a_problem_stays_until_it_is_dismissed_and_a_done_one_does_not() {
        let mut toasts = Toasts::default();
        toasts.notices.push(Notice {
            text: "could not send".to_owned(),
            kind: Kind::Problem,
            count: 1,
            actions: Vec::new(),
            // Long past its life, which a `Done` notice would be dropped for.
            at: Some(std::time::Instant::now() - LIFE - std::time::Duration::from_secs(1)),
        });
        toasts.notices.push(Notice {
            text: "saved".to_owned(),
            kind: Kind::Done,
            count: 1,
            at: Some(std::time::Instant::now() - LIFE - std::time::Duration::from_secs(1)),
            actions: Vec::new(),
        });
        toasts.forget_the_stale_ones();
        assert_eq!(toasts.len(), 1, "the confirmation faded and the problem did not");
        assert_eq!(toasts.notices()[0].kind, Kind::Problem);
        assert!(toasts.dismiss_the_newest(), "and a person can take it away");
        assert!(toasts.is_empty());
    }

    #[test]
    fn a_fresh_confirmation_is_not_forgotten() {
        let mut toasts = Toasts::default();
        toasts.say("saved", Kind::Done);
        toasts.forget_the_stale_ones();
        assert_eq!(toasts.len(), 1, "it has not had its time yet");
    }

    #[test]
    fn the_same_sentence_twice_is_one_notice_with_a_count() {
        let mut toasts = Toasts::default();
        for _ in 0..8 {
            toasts.say("HTTP 500: server_error", Kind::Problem);
        }
        assert_eq!(toasts.len(), 1, "eight identical failures are one card, not eight");
        assert_eq!(toasts.notices()[0].count, 8, "and it says how many");

        // A different sentence is a different notice, and the same words of a different kind are too — but
        // only `LIMIT` are kept, so the third pushes the first out. That is the cap doing its job.
        toasts.say("HTTP 500: server_error", Kind::Done);
        assert_eq!(toasts.len(), 2, "the same words of a different kind are a second notice");
        toasts.say("something else went wrong", Kind::Problem);
        assert_eq!(toasts.len(), LIMIT, "never more than the limit");
        assert_eq!(toasts.notices().last().expect("the newest").text, "something else went wrong");
    }

    #[test]
    fn a_repeated_confirmation_gets_its_time_back() {
        let mut toasts = Toasts::default();
        toasts.notices.push(Notice {
            text: "saved".to_owned(),
            kind: Kind::Done,
            count: 1,
            at: Some(std::time::Instant::now() - LIFE - std::time::Duration::from_secs(1)),
            actions: Vec::new(),
        });
        // Saying it again is a fresh event: it should not vanish the instant it is raised.
        toasts.say("saved", Kind::Done);
        toasts.forget_the_stale_ones();
        assert_eq!(toasts.len(), 1, "the repeat restarted its clock");
        assert_eq!(toasts.notices()[0].count, 2);
    }

    #[test]
    fn the_oldest_goes_when_there_are_more_than_the_limit() {
        let mut toasts = Toasts::default();
        for index in 0..LIMIT + 2 {
            toasts.say(format!("notice {index}"), Kind::Problem);
        }
        assert_eq!(toasts.len(), LIMIT);
        assert_eq!(
            toasts.notices()[0].text,
            "notice 2",
            "the two oldest went, and the newest is still there"
        );
        assert_eq!(toasts.notices().last().expect("one").text, format!("notice {}", LIMIT + 1));
    }

    #[test]
    fn an_empty_notice_is_not_raised() {
        let mut toasts = Toasts::default();
        toasts.say("   ", Kind::Problem);
        toasts.say("", Kind::Done);
        assert!(
            toasts.is_empty(),
            "a provider with an empty failure puts no blank card on the screen"
        );
    }

    #[test]
    fn dismissing_by_index_takes_the_right_one() {
        let mut toasts = Toasts::default();
        toasts.notices.push(timeless("first", Kind::Problem));
        toasts.notices.push(timeless("second", Kind::Problem));
        toasts.notices.push(timeless("third", Kind::Problem));
        toasts.dismiss(1);
        let left: Vec<&str> = toasts.notices().iter().map(|notice| notice.text.as_str()).collect();
        assert_eq!(left, ["first", "third"]);
        // Out of range is not a panic: a cross pressed on the frame a notice faded would otherwise take
        // the window down.
        toasts.dismiss(99);
        assert_eq!(toasts.len(), 2);
    }

    /// An offer stays until it is answered, is not raised twice, and is never the one the limit takes.
    /// `task-2063`.
    #[test]
    fn an_offer_waits_for_an_answer_and_is_kept_over_the_others() {
        let mut toasts = Toasts::default();
        let buttons = vec![
            ("Don't Ask Again".to_owned(), Act::SkipUpdate("9.9.9".to_owned())),
            ("Install & Restart".to_owned(), Act::InstallUpdate("9.9.9".to_owned())),
        ];
        toasts.offer("Unluminous 9.9.9 is available.", buttons.clone());
        toasts.offer("Unluminous 9.9.9 is available.", buttons);
        assert_eq!(toasts.len(), 1, "the same offer twice is one");
        toasts.notices[0].at =
            Some(std::time::Instant::now() - LIFE - std::time::Duration::from_secs(1));
        toasts.forget_the_stale_ones();
        assert_eq!(toasts.len(), 1, "an offer does not fade");
        for index in 0..LIMIT + 1 {
            toasts.say(format!("saved {index}"), Kind::Done);
        }
        assert_eq!(toasts.len(), LIMIT);
        assert!(
            toasts.notices().iter().any(|notice| notice.kind == Kind::Offer),
            "a later confirmation does not push the question off the screen"
        );
        toasts.withdraw_the_offers();
        assert!(toasts.notices().iter().all(|notice| notice.kind != Kind::Offer));
    }

    #[test]
    fn the_two_kinds_are_named_and_read_back() {
        for kind in [Kind::Problem, Kind::Done, Kind::Offer] {
            assert_eq!(Kind::parse(kind.name()), Some(kind));
        }
        // The spellings a command line caller might use.
        assert_eq!(Kind::parse("error"), Some(Kind::Problem));
        assert_eq!(Kind::parse("ok"), Some(Kind::Done));
        assert_eq!(Kind::parse("whatever"), None);
    }

    #[test]
    fn the_two_kinds_are_told_apart_by_colour_and_by_whether_they_fade() {
        assert_ne!(Kind::Problem.edge(), Kind::Done.edge());
        assert!(!Kind::Problem.fades(), "a failure nobody read is the fault this exists to fix");
        assert!(Kind::Done.fades());
    }
}
