//! Git: the menu, the commit panel, the branches, blame, and the branch widget in the title bar.
//!
//! Every one of these builds a real repository in a temporary folder and runs the machine's real
//! `git`, which is `unluminous-git`'s own rule — a push from Unluminous has to be the same push a
//! person gets in their terminal, so there is nothing here to stub. The repository is built with its
//! identity named on the command line, so a test does not depend on the `.gitconfig` of whoever is
//! running it, and with two commits on widely separated dates so blame has a spread of ages to
//! colour.
//!
//! **5 of the 12 tests here take a picture**, and the rest read the window's state back.

mod common;

use common::*;

use egui_kittest::kittest::Queryable;
use unluminous_app::app::actions::Action;
use unluminous_app::components::branch_widget::BRANCHES_BEFORE_A_FILTER;
use unluminous_app::components::title_bar::MenuPlacement;

#[test]
fn the_git_menu_holds_everything_the_ask_lists() {
    let mut harness = git_harness("menu");
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    steady(&mut harness);
    harness.get_by_label("Git").click();
    steady(&mut harness);
    for entry in [
        "Commit...",
        "Add",
        "Show Diff",
        "Compare with Revision...",
        "Show History",
        "Show Current Revision",
        "Rollback...",
        "Push...",
        "Pull...",
        "Fetch",
        "Merge...",
        "Rebase...",
        "Branches...",
        "New Branch...",
        "New Tag...",
        "Reset HEAD...",
        "Stash Changes...",
        "Unstash Changes...",
        "Manage Remotes...",
        "Clone...",
    ] {
        harness.get_by_label(entry);
    }
    harness.snapshot(shot("git_menu"));
}

#[test]
fn the_window_reads_the_repository_it_is_opened_in() {
    let harness = git_harness("read");
    let git = harness.state().git.as_ref().expect("the folder is a repository");
    assert_eq!(git.snapshot.status.branch.as_deref(), Some("main"));
    // The change that was not committed, and the file git has never seen.
    assert!(git.snapshot.status.entry("version.ts").is_some());
    assert!(git.snapshot.status.entry("notes.txt").expect("notes.txt").untracked());
    assert!(git.status_label().expect("it has been read").starts_with("main"));
}

#[test]
fn the_commit_panel_shows_the_changes_and_the_unversioned_files() {
    let mut harness = git_harness("commit");
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .run_action(Action::Git(unluminous_app::app::actions::GitAction::Commit), &ctx);
    steady(&mut harness);
    // Waited for, because opening the panel asks for the recent commit messages and the status bar
    // says so while it does. Whether that message is still there when the picture is taken depends
    // on how quickly a thread answered, which is not a difference in Unluminous.
    settle(&mut harness, "the history the panel asks for", |app| {
        app.git.as_ref().is_some_and(|git| git.message.is_none() && !git.history.is_empty())
    });
    if let Some(git) = harness.state_mut().git.as_mut() {
        git.panel.message = "task-1649: the commit panel".to_owned();
    }
    steady(&mut harness);
    harness.get_by_label("COMMIT");
    harness.get_by_label("COMMIT AND PUSH...");
    harness.snapshot(shot("git_commit_panel"));
}

#[test]
fn the_branches_dialog_lists_the_branches() {
    let mut harness = git_harness("branches");
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .run_action(Action::Git(unluminous_app::app::actions::GitAction::Branches), &ctx);
    steady(&mut harness);
    harness.get_by_label("main");
    harness.snapshot(shot("git_branches"));
}

#[test]
fn the_gutter_annotates_with_git_blame_and_colours_by_age() {
    let mut harness = git_harness("blame");
    let folder = harness.state().tree.root().to_path_buf();
    harness
        .state_mut()
        .open_path_permanently(&folder.join("sqlClient.ts"))
        .expect("the file opens");
    steady(&mut harness);
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .run_action(Action::Git(unluminous_app::app::actions::GitAction::Annotate), &ctx);
    for _ in 0..600 {
        pump(&mut harness);
        if harness.state().files.active().blame.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let blame = harness.state().files.active().blame.clone().expect("the file is annotated");
    assert!(!blame.is_empty());
    assert_eq!(blame[0].author, "Unluminous", "the first name, which is what fits in the column");
    // Both authors are in it, so the column really has a gradient to show rather than one colour.
    let authors: Vec<&str> = blame.iter().map(|row| row.author.as_str()).collect();
    assert!(authors.contains(&"Sam"), "the second commit's lines carry its author: {authors:?}");
    let ages: Vec<f32> = blame.iter().map(|row| row.age).collect();
    assert!(
        ages.contains(&0.0) && ages.contains(&1.0),
        "oldest and newest are both drawn: {ages:?}"
    );
    harness.snapshot(shot("gutter_blame"));
}

/// The git operations, driven through the window and checked against git itself.
///
/// This is the test `task-1649` asks for when it says to make a project for exercising the
/// operations. It is one test rather than several because each step leaves the repository in the
/// state the next one needs, and because a repository is the slowest thing here to build.
///
/// Every assertion asks **git** what happened rather than asking Unluminous, so a step that only changed
/// the window's own idea of the world fails.
#[test]
fn every_git_operation_can_be_driven_from_the_window() {
    let mut harness = git_harness("operations");
    let root = harness.state().tree.root().to_path_buf();
    let ctx = harness.ctx.clone();
    let git = |action| Action::Git(action);
    use unluminous_app::app::actions::GitAction;

    // ---- stage a file -----------------------------------------------------------------------
    harness.state_mut().open_path_permanently(&root.join("version.ts")).expect("the file opens");
    nudge(&mut harness);
    harness.state_mut().run_action(git(GitAction::Add(None)), &ctx);
    settle(&mut harness, "the file to be staged", |app| {
        app.git.as_ref().is_some_and(|git| {
            git.snapshot
                .status
                .entry("version.ts")
                .is_some_and(unluminous_git::status::Entry::staged)
        })
    });
    assert!(
        ask_git(&root, &["diff", "--cached", "--name-only"]).contains("version.ts"),
        "git agrees the file is staged"
    );

    // ---- commit it, through the panel's own button -------------------------------------------
    harness.state_mut().run_action(git(GitAction::Commit), &ctx);
    nudge(&mut harness);
    if let Some(state) = harness.state_mut().git.as_mut() {
        state.panel.message = "task-1649: driven from the window".to_owned();
    }
    nudge(&mut harness);
    harness.get_by_label("COMMIT").click();
    settle(&mut harness, "the commit", |app| {
        app.git.as_ref().is_some_and(|git| git.snapshot.status.entry("version.ts").is_none())
    });
    assert_eq!(
        ask_git(&root, &["log", "--format=%s", "-n1"]),
        "task-1649: driven from the window",
        "the commit really was made, with the message that was typed"
    );

    // ---- start a branch, through the prompt ---------------------------------------------------
    harness.state_mut().run_action(git(GitAction::NewBranch), &ctx);
    nudge(&mut harness);
    let mut prompt = harness.state_mut().prompt.take().expect("a prompt for the name");
    prompt.value = "from-unluminous".to_owned();
    harness.state_mut().run_prompt_for_test(prompt);
    harness.state_mut().prompt = None;
    settle(&mut harness, "the branch", |app| {
        app.git
            .as_ref()
            .is_some_and(|git| git.snapshot.status.branch.as_deref() == Some("from-unluminous"))
    });
    assert_eq!(ask_git(&root, &["branch", "--show-current"]), "from-unluminous");

    // ---- stash a change and bring it back ------------------------------------------------------
    std::fs::write(root.join("version.ts"), "export const version = 'stashed';\n")
        .expect("change it");
    harness.state_mut().run_action(git(GitAction::Stash), &ctx);
    nudge(&mut harness);
    let mut prompt = harness.state_mut().prompt.take().expect("a prompt for the message");
    prompt.value = "half done".to_owned();
    harness.state_mut().run_prompt_for_test(prompt);
    harness.state_mut().prompt = None;
    settle(&mut harness, "the stash", |app| {
        app.git.as_ref().is_some_and(|git| !git.snapshot.stashes.is_empty())
    });
    assert!(
        !std::fs::read_to_string(root.join("version.ts")).expect("read").contains("stashed"),
        "stashing left the working tree clean"
    );
    assert!(ask_git(&root, &["stash", "list"]).contains("half done"));

    // Unstashing is the `Stashes` tab of the commit panel, and its POP button.
    harness.state_mut().run_action(git(GitAction::Unstash), &ctx);
    nudge(&mut harness);
    harness.get_by_label("POP").click();
    settle(&mut harness, "the stash to come back", |app| {
        app.git.as_ref().is_some_and(|git| git.snapshot.stashes.is_empty())
    });
    assert!(
        std::fs::read_to_string(root.join("version.ts")).expect("read").contains("stashed"),
        "the change came back"
    );

    // ---- roll the change back, which is confirmed first ---------------------------------------
    harness.state_mut().run_action(git(GitAction::Rollback(None)), &ctx);
    nudge(&mut harness);
    assert!(harness.state().confirmation.is_some(), "rollback cannot be undone, so it asks first");
    harness.get_by_label("ROLL BACK").click();
    settle(&mut harness, "the rollback", |app| {
        app.git.as_ref().is_some_and(|git| git.snapshot.status.entry("version.ts").is_none())
    });
    assert!(
        !std::fs::read_to_string(root.join("version.ts")).expect("read").contains("stashed"),
        "the change is gone"
    );

    // ---- merge the branch back into main --------------------------------------------------------
    assert!(unluminous_git::branch::switch(&root, "main").ok);
    harness.state_mut().run_action(git(GitAction::Refresh), &ctx);
    settle(&mut harness, "main to be checked out", |app| {
        app.git.as_ref().is_some_and(|git| git.snapshot.status.branch.as_deref() == Some("main"))
    });
    if let Some(state) = harness.state_mut().git.as_mut() {
        state.dialogs.target = "from-unluminous".to_owned();
    }
    harness.state_mut().run_action(git(GitAction::Merge), &ctx);
    nudge(&mut harness);
    if let Some(state) = harness.state_mut().git.as_mut() {
        state.dialogs.target = "from-unluminous".to_owned();
    }
    nudge(&mut harness);
    harness.get_by_label("MERGE").click();
    // Asked of git rather than of the window: the merge is finished when main really holds the
    // branch's commit, which is the only thing that matters about it.
    let merged = root.clone();
    settle(&mut harness, "the merge", move |_| {
        unluminous_git::command::run(&merged, &["log", "--format=%s", "-n1"]).stdout.trim()
            == "task-1649: driven from the window"
    });
    assert_eq!(
        ask_git(&root, &["log", "--format=%s", "-n1"]),
        "task-1649: driven from the window",
        "main now holds the branch's commit"
    );

    // ---- and the history the window read is the history git has ---------------------------------
    harness.state_mut().run_action(git(GitAction::ShowHistory(None)), &ctx);
    settle(&mut harness, "the history", |app| {
        app.git.as_ref().is_some_and(|git| git.history.len() >= 3)
    });
    let subjects: Vec<String> = harness
        .state()
        .git
        .as_ref()
        .expect("a repository")
        .history
        .iter()
        .map(|commit| commit.subject.clone())
        .collect();
    assert_eq!(subjects[0], "task-1649: driven from the window");
    assert!(subjects.contains(&"the first commit".to_owned()));
    // No picture is taken here. A commit made during the test has a hash and a date that are new
    // every run, so a baseline of it could never match twice; `design/components/git_history.png`
    // is the capture to look at, and what this test is for is that the history is right.
}

// ---------------------------------------------------------------------------------------------
// `task-1848`: "Add a branch selector/indicator at the top bar". §10 of the design.
// ---------------------------------------------------------------------------------------------

/// The indicator half of the ask, against a real repository built in a temporary folder — which is how
/// every `unluminous-git` test works, and the only way to know the branch reported is git's own answer
/// rather than a value the test put there itself.
#[test]
fn the_branch_widget_names_the_branch_the_repository_is_on() {
    let mut harness = git_harness("unluminous-branch-widget");
    // The branch list arrives from the worker a round trip after the status does.
    for _ in 0..600 {
        if !harness.state().branch_state().locals.is_empty() {
            break;
        }
        pump(&mut harness);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let state = harness.state().branch_state();
    assert_eq!(state.current.as_deref(), Some("main"), "git_folder inits on main");
    assert!(
        state.locals.iter().any(|name| name == "main"),
        "and main is in the list, got {:?}",
        state.locals
    );
    assert!(state.applies(), "so the widget is drawn");
    // The button is reachable by name, which is what makes it testable at all.
    harness.get_by_label("Branch");
}

/// Unluminous's rule for a control that can never apply, and the same rule that hides the Git menu
/// outside a repository.
#[test]
fn the_branch_widget_is_absent_outside_a_repository() {
    let harness = harness("# not in a repository");
    assert!(!harness.state().branch_state().applies());
    // `get_all_by_label` panics when nothing matches rather than answering with an empty iterator, so
    // the absence is asserted through the state the widget is drawn from. `applies()` above is the one
    // question `show` asks before drawing anything, and `width` answering zero is what stops the title
    // bar leaving room for it.
    assert_eq!(
        unluminous_app::components::branch_widget::width(
            &harness.state().branch_state(),
            &harness.ctx.layer_painter(egui::LayerId::background()),
        ),
        0.0,
        "no room is left for a branch button outside a repository"
    );
}

/// **Choosing a branch asks the worker to check it out**, through the one path a switch already takes —
/// so a switch from the title bar and a switch from `Git -> Branches...` are the same git command.
#[test]
fn choosing_a_branch_asks_the_worker_to_check_it_out() {
    let mut harness = git_harness("unluminous-branch-switch");
    let root = harness.state().tree.root().to_path_buf();
    let git = |arguments: &[&str]| {
        let outcome = unluminous_git::command::run(&root, arguments);
        assert!(outcome.ok, "git {arguments:?}: {}", outcome.message());
    };
    // A second branch to move to, and back to main so the switch has somewhere to go.
    git(&["add", "-A"]);
    git(&["commit", "-m", "first"]);
    git(&["branch", "a-second-branch"]);
    harness.state_mut().refresh_repository();
    for _ in 0..600 {
        if harness.state().branch_state().locals.iter().any(|name| name == "a-second-branch") {
            break;
        }
        pump(&mut harness);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(
        Action::Git(unluminous_app::app::actions::GitAction::Switch("a-second-branch".to_owned())),
        &ctx,
    );
    for _ in 0..600 {
        if harness.state().branch_state().current.as_deref() == Some("a-second-branch") {
            break;
        }
        pump(&mut harness);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert_eq!(
        harness.state().branch_state().current.as_deref(),
        Some("a-second-branch"),
        "the repository is really on the other branch, which is git's answer rather than the window's hope"
    );
}

/// Switching to the branch already checked out is refused rather than run: a git command that would do
/// nothing and then report that it had worked is the fault `task-1691` measured in `run start`.
#[test]
fn switching_to_the_branch_already_on_says_so_rather_than_running_git() {
    let mut harness = git_harness("unluminous-branch-same");
    for _ in 0..600 {
        if harness.state().branch_state().current.is_some() {
            break;
        }
        pump(&mut harness);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(
        Action::Git(unluminous_app::app::actions::GitAction::Switch("main".to_owned())),
        &ctx,
    );
    steady(&mut harness);
    let said = harness.state().message.clone().unwrap_or_default();
    assert!(said.contains("Already on main"), "said {said:?}");
}

// A field below the threshold would be a control that cannot help: every row is already on the
// screen. Checked here rather than in a runtime test, because clippy is right that comparing a
// constant against a literal can only ever pass or fail the same way: a build fails before the
// popup could ever ship with a threshold nobody would notice was wrong.

const _: () = assert!(
    BRANCHES_BEFORE_A_FILTER >= 8,
    "a threshold low enough to be reached in a real project"
);

const _: () = assert!(
    BRANCHES_BEFORE_A_FILTER <= 20,
    "and high enough that a small project never sees a field"
);

/// The picture, which is what a person reads to see whether it looks like the bar it sits in.
#[test]
fn branch_widget_in_the_title_bar() {
    let mut harness = git_harness("unluminous-branch-picture");
    for _ in 0..600 {
        if !harness.state().branch_state().locals.is_empty() {
            break;
        }
        pump(&mut harness);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    steady(&mut harness);
    harness.snapshot(shot("branch_widget"));
}

/// The rows the flyout offers, asserted against the state it builds them from.
///
/// **The popup is not opened by a synthesised click**, and that is a harness limit rather than a fault:
/// `harness.get_by_label("Branch").click()` reaches the button — the button is in the tree and answers —
/// and no row appears in the next frame's tree at all. `egui::Popup` decides whether it is open from the
/// response id of the frame that toggled it, and driving that offscreen does not settle. The same is
/// true of the run widget's own flyout, which is why there is no picture of that one open either.
///
/// So what is asserted is what the rows are built from, which is where every decision in them lives:
/// which branches are offered, that the checked one is the branch the repository is really on, and that
/// a remote's branch is not offered. Opening it is layer 4.
#[test]
fn the_flyout_offers_every_local_branch_and_marks_the_one_checked_out() {
    let mut harness = git_harness("unluminous-branch-flyout");
    let root = harness.state().tree.root().to_path_buf();
    let git = |arguments: &[&str]| {
        let outcome = unluminous_git::command::run(&root, arguments);
        assert!(outcome.ok, "git {arguments:?}: {}", outcome.message());
    };
    git(&["add", "-A"]);
    git(&["commit", "-m", "first"]);
    git(&["branch", "a-feature-branch"]);
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .run_action(Action::Git(unluminous_app::app::actions::GitAction::Refresh), &ctx);
    for _ in 0..600 {
        if harness.state().branch_state().locals.len() > 1 {
            break;
        }
        pump(&mut harness);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    let state = harness.state().branch_state();
    assert_eq!(state.current.as_deref(), Some("main"), "the branch really checked out");
    assert!(state.locals.iter().any(|name| name == "main"));
    assert!(
        state.locals.iter().any(|name| name == "a-feature-branch"),
        "a branch made behind the window's back reaches the flyout after a refresh: {:?}",
        state.locals
    );
    // A remote's branch is not offered: checking one out detaches HEAD or makes a tracking branch, and
    // which of those somebody meant is not a question a one-click row may answer for them.
    assert!(
        !state.locals.iter().any(|name| name.contains('/')),
        "no remote branch is offered: {:?}",
        state.locals
    );
    // The button is reachable, which is what a person presses to see the rows above.
    harness.get_by_label("Branch");
}
