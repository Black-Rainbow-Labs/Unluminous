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
/// A turn that fails every round would otherwise fill the window with the same sentence. Four is what
/// fits above the status bar at the default font size without reaching the middle of the window.
pub const LIMIT: usize = 4;

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
}

impl Kind {
    /// The name a command line and a test use.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Problem => "problem",
            Kind::Done => "done",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "problem" | "error" | "failed" => Some(Kind::Problem),
            "done" | "ok" | "success" => Some(Kind::Done),
            _ => None,
        }
    }

    fn edge(self) -> Color32 {
        match self {
            Kind::Problem => color::close(),
            Kind::Done => color::git_added(),
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
    /// When it was raised, which is what [`LIFE`] is measured from. `None` in a test that is asserting on
    /// the list rather than on time.
    pub at: Option<std::time::Instant>,
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
    pub fn say(&mut self, text: impl Into<String>, kind: Kind) {
        let text = text.into();
        // An empty notice is nothing to read, and a provider that returns an empty failure string
        // should not put a blank card on the screen.
        if text.trim().is_empty() {
            return;
        }
        self.notices.push(Notice { text, kind, at: Some(std::time::Instant::now()) });
        while self.notices.len() > LIMIT {
            self.notices.remove(0);
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
pub fn show(ui: &mut egui::Ui, area: Rect, toasts: &Toasts, look_font: f32) -> Option<usize> {
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
        let wrapped = painter.layout(
            notice.text.clone(),
            FontId::proportional(look_font - 1.0),
            color::text(),
            WIDTH - PAD * 2.0 - EDGE - CROSS - 6.0,
        );
        let height = (wrapped.size().y + PAD * 2.0).max(36.0);
        let card = Rect::from_min_size(
            Pos2::new(area.right() - MARGIN - WIDTH, bottom - height),
            Vec2::new(WIDTH, height),
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
            dismissed = Some(index);
        }

        bottom = card.top() - 8.0;
    }
    dismissed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A notice with no instant, so a test asserting on the list is not asserting on the clock.
    fn timeless(text: &str, kind: Kind) -> Notice {
        Notice { text: text.to_owned(), kind, at: None }
    }

    #[test]
    fn a_problem_stays_until_it_is_dismissed_and_a_done_one_does_not() {
        let mut toasts = Toasts::default();
        toasts.notices.push(Notice {
            text: "could not send".to_owned(),
            kind: Kind::Problem,
            // Long past its life, which a `Done` notice would be dropped for.
            at: Some(std::time::Instant::now() - LIFE - std::time::Duration::from_secs(1)),
        });
        toasts.notices.push(Notice {
            text: "saved".to_owned(),
            kind: Kind::Done,
            at: Some(std::time::Instant::now() - LIFE - std::time::Duration::from_secs(1)),
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
    fn the_oldest_goes_when_there_are_more_than_the_limit() {
        let mut toasts = Toasts::default();
        for index in 0..LIMIT + 2 {
            toasts.say(format!("notice {index}"), Kind::Problem);
        }
        assert_eq!(toasts.len(), LIMIT);
        assert_eq!(
            toasts.notices()[0].text, "notice 2",
            "the two oldest went, and the newest is still there"
        );
        assert_eq!(toasts.notices().last().expect("one").text, format!("notice {}", LIMIT + 1));
    }

    #[test]
    fn an_empty_notice_is_not_raised() {
        let mut toasts = Toasts::default();
        toasts.say("   ", Kind::Problem);
        toasts.say("", Kind::Done);
        assert!(toasts.is_empty(), "a provider with an empty failure puts no blank card on the screen");
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

    #[test]
    fn the_two_kinds_are_named_and_read_back() {
        for kind in [Kind::Problem, Kind::Done] {
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
