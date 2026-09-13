//! The branch button in the title bar: a branch icon, the branch the repository is on, and a flyout
//! that lists the local branches and switches to one.
//!
//! `task-1848`: "Add a branch selector/indicator at the top bar, similar to the reference editor's".
//! `tasks/task-1848-reported-issues-tdd.md` §10 is the design, and three things in it are decisions
//! rather than arrangement.
//!
//! **It goes to the left, beside the project's name, and not at the right hand end.** `task-1693`
//! recorded the ordering rule for that end of the bar: whichever of two controls changes width has to
//! be the one measured back from the other, or the fixed one slides about. A branch name is as wide
//! as somebody's branch naming habit, so putting it beside the run widget would move the play button
//! every time a branch was switched. Beside the project name it is measured **from** the left, where
//! nothing after it has a fixed position to lose — and it is also where the reference editor has it.
//!
//! **It is absent outside a repository**, which is Unluminous's rule for a control that cannot apply and
//! is the same rule that dims the whole Git menu there.
//!
//! **The list is the local branches, and the switch is the one path a switch already takes.** Every
//! row becomes an [`Action`], so a switch from here and a switch from `Git -> Branches...` are the
//! same `unluminous_git::Worker` request — this component changes nothing, which is the rule every
//! component in Unluminous follows.
//!
//! What is deliberately not here, each with its reason:
//!
//! - **The remote branches and the other repositories** in the reference picture are that editor's
//!   multi-repository project model, which Unluminous does not have: one window is one project. The
//!   full list, remotes included, is `Git -> Branches...`, and the last row of the flyout opens it.
//! - **`Update Project`, `Commit` and `Push`** are on the Git menu with their own chords already, and
//!   a second place to press them would be a second thing to keep in step.
//! - **An ahead-and-behind count** needs a fetch to be truthful, and Unluminous does not fetch without
//!   being asked. The status bar says what the last status knew, which is where that belongs.
//! - **A search field** is for a list of hundreds. The flyout filters when there are more branches
//!   than [`BRANCHES_BEFORE_A_FILTER`] and draws no field below that.

use egui::{Pos2, Rect, Vec2};

use crate::app::actions::{Action, GitAction};
use crate::components::controls;
use crate::theme::{color, icon};

/// How wide the flyout is. Wider than the run widget's, because a branch name is longer than a run
/// configuration's name more often than not.
const PANEL: f32 = 260.0;

/// The tallest the name is allowed to be drawn, in points. A branch called after a ticket and its
/// whole summary is a real thing, and it must not push the drag area off the bar.
const LONGEST_NAME: f32 = 160.0;

/// The height of the button, which is the run widget's so the two read as furniture of one bar.
const BUTTON: f32 = 22.0;

/// Above this many branches the flyout draws a filter field. Below it, a field would be a control
/// that cannot help: every row is already on the screen.
pub const BRANCHES_BEFORE_A_FILTER: usize = 12;

/// How many rows the flyout draws at most, however many branches there are. Beyond this the list is
/// what `Git -> Branches...` is for, and the flyout says so on its last row.
const MOST_ROWS: usize = 15;

/// What the widget needs to know, worked out by the window from the git snapshot.
///
/// **A value rather than a borrow of the repository**, so the whole of this file is testable with no
/// window and no git: `the_branch_widget_names_the_branch_the_repository_is_on` builds one by hand.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BranchState {
    /// The branch that is checked out, or `None` outside a repository — which is what makes the whole
    /// widget absent.
    pub current: Option<String>,
    /// The **local** branches, in the order git listed them. A remote's branch is not offered here:
    /// checking one out detaches HEAD or makes a tracking branch, and which of those somebody meant
    /// is a question a one-click row must not answer for them.
    pub locals: Vec<String>,
}

impl BranchState {
    /// Whether there is anything to draw. Outside a repository there is not.
    pub fn applies(&self) -> bool {
        self.current.is_some()
    }
}

/// How wide the widget is, so the title bar can leave room for it. Zero when it does not apply.
pub fn width(state: &BranchState, painter: &egui::Painter) -> f32 {
    let Some(current) = state.current.as_deref() else { return 0.0 };
    // The room `controls::labelled_flyout_with_icon` really needs: the mark and the gap before the word
    // is where it starts the word from, then the word itself, then the chevron it draws at the right.
    // The name is measured rather than guessed from its character count, because a proportional font
    // makes `illlll` and `WWWWWW` very different widths and the name is somebody else's choice.
    //
    // It is measured at the size that function sets the word in. It was measured at 12.0 while the word
    // was drawn at 12.5, which is a button about four points too narrow for its own text.
    WORDS_FROM + name_width(current, painter) + AFTER_THE_WORD
}

/// How far in from the left of the button the word starts, which is
/// `controls::labelled_flyout_with_icon`'s own offset for a button that carries a mark.
const WORDS_FROM: f32 = 22.0;

/// The room after the word: the chevron, which that function draws ten points in from the right, and a
/// gap so the word does not touch it.
const AFTER_THE_WORD: f32 = 20.0;

/// The size the word is set in, which is that function's own.
const NAME_SIZE: f32 = 12.5;

/// How wide the name is drawn, which is its own width up to [`LONGEST_NAME`].
fn name_width(name: &str, painter: &egui::Painter) -> f32 {
    let galley = painter.layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(NAME_SIZE),
        color::text(),
    );
    galley.size().x.min(LONGEST_NAME)
}

/// Draw the widget into `area` and say what was chosen.
pub fn show(ui: &mut egui::Ui, area: Rect, state: &BranchState) -> Option<Action> {
    let current = state.current.as_deref()?;
    let button = Rect::from_min_size(
        Pos2::new(area.left(), area.center().y - BUTTON / 2.0),
        Vec2::new(area.width(), BUTTON),
    );

    // **`controls::labelled_flyout_with_icon`, not `controls::flyout`.** A flyout draws its icon at the
    // middle of the button, which is right for a square icon button and wrong here: this button is as
    // wide as a branch name, so the branch mark landed on top of the word. Drawing the name separately
    // afterwards is what this used to do and is the same fault from the other side, because the mark was
    // still in the middle.
    controls::labelled_flyout_with_icon(
        ui,
        button,
        // The accessible name says what it does rather than what it shows, because what it shows is a
        // branch name and a test asking for `Branch: main` would be a test of somebody's repository.
        "Branch",
        current,
        Some(icon::branch),
        PANEL,
        |panel| rows(panel, state),
    )
    .flatten()
}

/// The rows of the flyout: the branches, then the way to the full list.
fn rows(ui: &mut egui::Ui, state: &BranchState) -> Option<Action> {
    let mut chosen = None;
    let current = state.current.as_deref().unwrap_or_default();

    // The filter, only when the list is long enough for one to help. Its text lives in egui's memory
    // under the flyout's id: the window has no decision to make about it and nothing is written to
    // disk, which is where `components::modal` keeps a dragged modal's position for the same reason.
    let mut wanted = String::new();
    if state.locals.len() > BRANCHES_BEFORE_A_FILTER {
        let id = ui.id().with("branch filter");
        wanted = ui.data_mut(|data| data.get_temp::<String>(id).unwrap_or_default());
        let field = ui.allocate_space(Vec2::new(ui.available_width(), 24.0)).1;
        let mut editing = wanted.clone();
        // `field_text_rect` is what stops this being the sixth field in Unluminous to put its words
        // against its own top edge: egui lays a text box out at the top of the rectangle it is given and
        // `Frame::NONE` leaves no margin to push it down.
        ui.put(
            controls::field_text_rect(ui, field, 6.0),
            egui::TextEdit::singleline(&mut editing)
                .frame(egui::Frame::NONE)
                .hint_text("Filter branches"),
        );
        if editing != wanted {
            wanted = editing.clone();
            ui.data_mut(|data| data.insert_temp(id, editing));
        }
    }

    let matching: Vec<&String> = state
        .locals
        .iter()
        .filter(|name| wanted.is_empty() || name.to_lowercase().contains(&wanted.to_lowercase()))
        .take(MOST_ROWS)
        .collect();

    for name in &matching {
        // The one that is checked out is ticked rather than left out, so the list always says where
        // you are — which is the whole of the "indicator" half of the ask.
        let on_it = *name == current;
        if controls::menu_row(ui, name, "", !on_it, on_it, 0.0) && !on_it {
            chosen = Some(Action::Git(GitAction::Switch((*name).clone())));
        }
    }
    if matching.is_empty() {
        controls::menu_row(ui, "No branch of that name", "", false, false, 0.0);
    }

    ui.add_space(4.0);
    if controls::menu_row(ui, "New Branch...", "", true, false, 0.0) {
        chosen = Some(Action::Git(GitAction::NewBranch));
    }
    // The remotes, the deletes and the merges are all here, which is why the flyout does not need
    // them: one place holds the whole of branching and this is the quick way to the commonest part.
    if controls::menu_row(ui, "All Branches...", "", true, false, 0.0) {
        chosen = Some(Action::Git(GitAction::Branches));
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(branch: &str, locals: &[&str]) -> BranchState {
        BranchState {
            current: Some(branch.to_owned()),
            locals: locals.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    /// The indicator half of the ask.
    #[test]
    fn the_branch_widget_names_the_branch_the_repository_is_on() {
        let state = on("main", &["main", "task-1848"]);
        assert!(state.applies());
        assert_eq!(state.current.as_deref(), Some("main"));
    }

    /// Unluminous's rule for a control that can never apply — the same rule that hides the Git menu.
    #[test]
    fn the_branch_widget_is_absent_outside_a_repository() {
        let state = BranchState::default();
        assert!(!state.applies());
    }

    /// A field below the threshold would be a control that cannot help: every row is already showing.
    #[test]
    fn the_flyout_filters_only_when_there_are_more_branches_than_it_can_show() {
        let few: Vec<String> = (0..BRANCHES_BEFORE_A_FILTER).map(|n| format!("b{n}")).collect();
        assert!(few.len() <= BRANCHES_BEFORE_A_FILTER, "no field for this many");
        let many: Vec<String> =
            (0..BRANCHES_BEFORE_A_FILTER + 1).map(|n| format!("b{n}")).collect();
        assert!(many.len() > BRANCHES_BEFORE_A_FILTER, "a field for this many");
    }
}
