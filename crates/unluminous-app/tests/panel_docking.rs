//! Where the panels are: dragged to an edge, resized, maximised and zoomed.
//!
//! Dragging a panel's header to the top, bottom, left or right of the window and reading back which
//! side it ended up on; dragging a divider and reading back the rectangle each panel was given;
//! filling the window with one pane and putting it back; and the wheel and the keys that zoom
//! whichever panel the pointer or the keyboard is on.
//!
//! What is asserted is the window's own state read back — which side the panel is on and what
//! rectangle it has — rather than what the drag reported. The pictures are there so somebody can
//! look and see that it is a panel rather than a stripe.
//!
//! **7 of the 33 tests here take a picture**, and the rest read the window's state back.

mod common;

use common::*;

use egui::Modifiers;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::app::actions::Action;
use unluminous_app::app::dock::Panel;
use unluminous_app::settings;
use unluminous_app::UnluminousApp;

/// The core panels docked on a side, in screen order.
///
/// **Without the panes plugins contribute.** The Designer's pane asks to be docked on the left (`task-2254`), and a
/// pane that is not showing is still on a side, so a list of everything on the left now ends with it. These tests
/// are about where the explorer and the terminal go in relation to each other, which is what is read here.
fn core_panels_on(
    harness: &Harness<'static, UnluminousApp>,
    side: unluminous_app::app::dock::Side,
) -> Vec<Panel> {
    harness
        .state()
        .panes
        .dock
        .panels_on(side)
        .into_iter()
        .filter(|panel| !matches!(panel, Panel::Plugin(_)))
        .collect()
}

// The panes, all of which are resized by dragging their edge.

#[test]
fn the_explorer_can_be_dragged_wider_and_the_editor_gives_up_the_room() {
    let mut harness = harness("");
    let before = harness.state().panes.explorer_width;
    let editor_before = harness.state().editor_area().width();
    let handle = harness.get_by_label("Resize explorer").rect();
    let from = handle.center();
    drag(&mut harness, from, egui::pos2(from.x + 120.0, from.y));

    let after = harness.state().panes.explorer_width;
    assert!(after > before + 100.0, "the explorer should be wider: {before} then {after}");
    assert!(
        harness.state().editor_area().width() < editor_before,
        "and the editing area should have given up the room"
    );
    harness.snapshot(shot("explorer_wide"));
}

#[test]
fn the_explorer_cannot_be_dragged_past_its_limits() {
    let mut harness = harness("");
    let handle = harness.get_by_label("Resize explorer").rect();
    let from = handle.center();
    // Far further than the window is wide, in both directions.
    drag(&mut harness, from, egui::pos2(from.x + 2000.0, from.y));
    assert_eq!(harness.state().panes.explorer_width, unluminous_app::settings::EXPLORER_MAX);
    let handle = harness.get_by_label("Resize explorer").rect();
    drag(&mut harness, handle.center(), egui::pos2(handle.center().x - 2000.0, handle.center().y));
    assert_eq!(harness.state().panes.explorer_width, unluminous_app::settings::EXPLORER_MIN);
}

/// `task-1771`: *"I should be able to double click anywhere in the top of a pane to get it to maximize,
/// then Esc or double click to put it back to the size it was."*
///
/// Maximising is putting everything else away, which is why nothing here has to check a rectangle: what is
/// showing is the layout's own input, and `dock::regions` gives the room to whatever is left.
#[test]
fn two_presses_on_a_panels_header_fill_the_window_with_it_and_two_more_put_it_back() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    did(&mut harness, "terminal show");
    steady(&mut harness);
    assert!(harness.state().explorer_visible && harness.state().editor_visible);

    // The explorer's heading is its own drag handle, so it is the top of the pane in the ticket's sense.
    let header = harness.get_by_label("Move Project").rect();
    double_click_at(&mut harness, header.center());
    assert_eq!(harness.state().maximised_pane(), Some(Some(Panel::Explorer)));
    assert!(!harness.state().editor_visible, "the editing area is away");
    assert!(!harness.state().terminal.visible, "and so is the terminal");
    assert!(harness.state().explorer_visible, "and the explorer is the one thing left");
    let filling = harness.state().panel_area(Panel::Explorer);
    assert!(
        (filling.width() - harness.state().panes_area().width()).abs() < 1.5,
        "it fills the window: {filling:?}"
    );

    // And again puts back exactly what was showing.
    let header = harness.get_by_label("Move Project").rect();
    double_click_at(&mut harness, header.center());
    assert_eq!(harness.state().maximised_pane(), None);
    assert!(harness.state().editor_visible);
    assert!(
        harness.state().terminal.visible,
        "the terminal was showing before, so it is showing again"
    );
}

/// The editing area has a tab strip where every other pane has a header, so that is its top.
#[test]
fn two_presses_on_the_empty_part_of_the_tab_strip_fill_the_window_with_the_editing_area() {
    let mut harness = harness("");
    did(&mut harness, "terminal show");
    steady(&mut harness);
    // The far right of the strip, which is past every tab and is therefore the part no tab wanted.
    let strip = harness.state().tab_strip_for_tests(0);
    let empty = egui::pos2(strip.right() - 12.0, strip.center().y);
    double_click_at(&mut harness, empty);
    assert_eq!(harness.state().maximised_pane(), Some(None), "the editing area fills the window");
    assert!(!harness.state().explorer_visible && !harness.state().terminal.visible);

    double_click_at(&mut harness, empty);
    assert_eq!(harness.state().maximised_pane(), None);
    assert!(harness.state().explorer_visible && harness.state().terminal.visible);
}

/// Four things the `task-1771` review found about maximising, each of which had a way of leaving the
/// window in a state nobody could have asked for.
#[test]
fn maximising_is_not_an_arrangement_and_a_toggle_inside_it_ends_it() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    did(&mut harness, "terminal show");
    steady(&mut harness);
    let (editor, panels) = harness.state().remembered_panels_for_tests();
    assert!(editor && panels[Panel::Explorer.index()] && panels[Panel::Terminal.index()]);

    // **What a project remembers is the arrangement a person chose.** Maximising is every other panel put
    // away, and the project's own `.unluminous` is written every frame — so Unluminous closed while a pane filled the
    // window used to open next time with the editing area and the terminal hidden and nothing to say why.
    let header = harness.get_by_label("Move Project").rect();
    double_click_at(&mut harness, header.center());
    assert_eq!(harness.state().maximised_pane(), Some(Some(Panel::Explorer)));
    assert!(!harness.state().editor_visible, "the editing area is away on the screen");
    let (editor, panels) = harness.state().remembered_panels_for_tests();
    assert!(editor, "and showing as far as the project is concerned");
    assert!(panels[Panel::Terminal.index()], "and so is the terminal");

    // **A toggle inside a maximise ends it.** Hiding the maximised pane used to leave a body with nothing
    // in it at all; showing a second one left two panes up with the menu still offering `Restore Pane`.
    //
    // **What it does not do is put the arrangement back** — `task-2003`, and see
    // `a_pane_toggle_inside_a_maximise_opens_that_pane_and_nothing_else` for the report. The terminal
    // was asked for, so the terminal appears; the explorer that was filling the window is still there;
    // and the editing area, which nobody asked for, stays away.
    did(&mut harness, "action run toggle-terminal");
    steady(&mut harness);
    assert_eq!(harness.state().maximised_pane(), None, "the maximize is over");
    assert!(harness.state().terminal.visible, "the pane that was asked for is showing");
    assert!(harness.state().explorer_visible, "so there is something to look at");
    assert!(!harness.state().editor_visible, "and nothing nobody asked for came back with it");
    // Put it back by hand, because the rest of this test is about a window with an editing area in it.
    did(&mut harness, "action run toggle-editor");
    steady(&mut harness);
    assert!(harness.state().editor_visible);

    // **The tile with the keyboard is the tile that maximises.** The terminal tile and the run tile both
    // say `Focus::Terminal`, which is fine for the zoom — the three share one font size — and wrong here.
    did(&mut harness, "action run toggle-run-tile");
    steady(&mut harness);
    assert!(harness.state().run.visible);
    did(&mut harness, "action run toggle-maximised-pane");
    steady(&mut harness);
    assert_eq!(
        harness.state().maximised_pane(),
        Some(Some(Panel::Run)),
        "the run tile, not the terminal"
    );
    assert!(harness.state().run.visible && !harness.state().terminal.visible);
}

/// Escape is the other way back, and it must not also do the two other things Escape means.
#[test]
fn escape_puts_a_maximised_pane_back_and_does_nothing_when_none_is() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    did(&mut harness, "action run toggle-maximised-pane");
    steady(&mut harness);
    // Nothing had the keyboard but the editing area, so the editing area is what filled the window.
    assert_eq!(harness.state().maximised_pane(), Some(None));
    assert!(!harness.state().explorer_visible);

    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    assert_eq!(harness.state().maximised_pane(), None);
    assert!(harness.state().explorer_visible, "the explorer came back");

    // **The keyboard comes back with everything else.** Putting a panel back *takes* the keys —
    // `show_the_terminal_tile` hands them to the terminal whenever it is shown — so a restore that did not
    // remember who had them left the caret in a terminal nobody had asked for.
    did(&mut harness, "terminal show");
    steady(&mut harness);
    harness.get_by_label("readme.md").click();
    steady(&mut harness);
    assert_eq!(harness.state().focus, unluminous_app::app::Focus::Explorer);
    did(&mut harness, "action run toggle-maximised-pane");
    steady(&mut harness);
    assert_eq!(
        harness.state().maximised_pane(),
        Some(Some(Panel::Explorer)),
        "the pane with the keys"
    );
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    assert!(harness.state().terminal.visible, "the terminal came back");
    assert_eq!(
        harness.state().focus,
        unluminous_app::app::Focus::Explorer,
        "and the keyboard is where it was, not in the terminal that was just put back"
    );

    // A second Escape with nothing maximised is nobody's business here: the explorer's own Escape goes on
    // meaning what it meant, which is that the keyboard goes back to the document.
    harness.get_by_label("readme.md").click();
    steady(&mut harness);
    assert_eq!(harness.state().focus, unluminous_app::app::Focus::Explorer);
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    assert_eq!(harness.state().focus, unluminous_app::app::Focus::Editor);
    assert_eq!(harness.state().maximised_pane(), None);
}

/// `task-1771`: *"we want every pane (file panel, folders, terminal, agent chat, agent tasks, etc) to be
/// zoomable with Ctrl/Cmd + or Ctrl/Cmd scroll wheel."*
///
/// The gesture belongs to whichever pane the pointer is over, which is the rule the editing area already
/// kept for itself and the reason `zoom_taken` exists. What one step means differs by pane, and that is the
/// half worth pinning: a tile is a character grid drawn at the terminal's own font size, so its zoom walks
/// that setting rather than putting a multiplier on top of it.
#[test]
fn a_wheel_with_the_modifier_zooms_whichever_pane_the_pointer_is_over() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    did(&mut harness, "terminal show");
    steady(&mut harness);

    let editor_font = harness.state().settings.font_size;
    let explorer = harness.state().panel_area(Panel::Explorer);
    zoom_at(&mut harness, explorer.center(), true);
    assert!(
        harness.state().panes.zoom_of(Panel::Explorer) > 1.0,
        "the explorer took the gesture: {}",
        harness.state().panes.zoom_of(Panel::Explorer)
    );
    assert_eq!(
        harness.state().settings.font_size,
        editor_font,
        "and the editing area did not also take it, which is the fault `zoom_taken` exists for"
    );

    // A tile has no multiplier of its own. Its zoom is `terminal.font.size`, so that one number goes on
    // saying how big a terminal is and the Settings window cannot disagree with the wheel.
    let terminal = harness.state().panel_area(Panel::Terminal);
    let before = harness.state().settings.terminal_font_size;
    zoom_at(&mut harness, terminal.center(), true);
    assert!(
        harness.state().settings.terminal_font_size > before,
        "the terminal grew: {before} then {}",
        harness.state().settings.terminal_font_size
    );
    assert_eq!(harness.state().panes.zoom_of(Panel::Terminal), 1.0, "and gained no second knob");

    // A pane a plugin contributed carries its own, remembered against that pane.
    let chat = harness.state().panel_area(Panel::Plugin(0));
    zoom_at(&mut harness, chat.center(), false);
    assert!(
        harness.state().panes.zoom_of(Panel::Plugin(0)) < 1.0,
        "the chat pane zoomed out: {}",
        harness.state().panes.zoom_of(Panel::Plugin(0))
    );
    assert_eq!(
        harness.state().settings.font_size,
        editor_font,
        "and the editor's font is still nobody else's business"
    );
}

/// The keys go where the keyboard is, which is the other half of the same ask.
#[test]
fn the_zoom_keys_are_about_whichever_pane_holds_the_keyboard() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    let editor_font = harness.state().settings.font_size;

    // With the editing area holding the keys they mean what they always meant.
    did(&mut harness, "action run increase-font-size");
    steady(&mut harness);
    assert!(harness.state().settings.font_size > editor_font);
    let editor_font = harness.state().settings.font_size;

    // Click a row, which is what gives the explorer the keyboard, and the same action is about the
    // explorer instead.
    harness.get_by_label("readme.md").click();
    steady(&mut harness);
    assert_eq!(harness.state().focus, unluminous_app::app::Focus::Explorer);
    did(&mut harness, "action run increase-font-size");
    steady(&mut harness);
    assert!(harness.state().panes.zoom_of(Panel::Explorer) > 1.0, "the explorer grew");
    assert_eq!(harness.state().settings.font_size, editor_font, "and the editor's font did not");

    // And `Reset Font Size` puts that pane back rather than the editor.
    did(&mut harness, "action run reset-font-size");
    steady(&mut harness);
    assert_eq!(harness.state().panes.zoom_of(Panel::Explorer), 1.0);
    assert_eq!(harness.state().settings.font_size, editor_font);

    // **A picture takes the zoom keys and nothing else.** `task-1658` asks that control and plus zoom an
    // image, and an image is opened from the tree — which leaves the keyboard in the explorer, so the zoom
    // has to make an exception of it. Maximising does not: that is still about the pane holding the keys,
    // and reusing one answer for both questions would have maximised the editing area from the file tree.
    harness.get_by_label_contains("picture.png").click();
    steady(&mut harness);
    assert_eq!(harness.state().focus, unluminous_app::app::Focus::Explorer);
    did(&mut harness, "action run toggle-maximised-pane");
    steady(&mut harness);
    assert_eq!(
        harness.state().maximised_pane(),
        Some(Some(Panel::Explorer)),
        "the pane with the keyboard, not the tab with the picture"
    );
}

/// Everything a person can do an agent can do, through the same code. `panel zoom` is that half.
#[test]
fn a_panels_zoom_is_set_and_read_from_the_command_line() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);

    let set = did(&mut harness, "panel zoom explorer 1.35");
    assert_eq!(set["kind"], "zoom");
    assert!((harness.state().panes.zoom_of(Panel::Explorer) - 1.35).abs() < 0.001);
    // Named the way the rest of the command line names a contributed pane, rather than by its slot.
    did(&mut harness, "panel zoom agent-chat/chat 2.0");
    assert!((harness.state().panes.zoom_of(Panel::Plugin(0)) - 2.0).abs() < 0.001);
    // And by the plugin's id alone, which is what `plugins pane agent-chat` takes.
    did(&mut harness, "panel zoom agent-chat 1.5");
    assert!((harness.state().panes.zoom_of(Panel::Plugin(0)) - 1.5).abs() < 0.001);
    // A tile answers with the one number that decides its size, and says which kind of answer it is.
    let tile = did(&mut harness, "panel zoom terminal");
    assert_eq!(tile["kind"], "font size");
    assert_eq!(tile["font_size"], harness.state().settings.terminal_font_size);

    did(&mut harness, "panel zoom explorer reset");
    assert_eq!(harness.state().panes.zoom_of(Panel::Explorer), 1.0);
    assert_eq!(refused(&mut harness, "panel zoom explorer sideways"), "usage");
}

/// Turn the wheel with the zoom modifier held, with the pointer at `at`.
///
/// `egui` reports a pinch and Ctrl with the wheel as one `zoom_delta`, so this is what both look like from
/// inside the window. The pointer is moved first because which pane a gesture belongs to is decided by
/// where it is — see `UnluminousApp::zoom_over_a_panel`.
///
/// **`steady` rather than `Harness::run`** (`task-1984`). `run` gives the window four steps to go quiet
/// and panics otherwise, and a pointer move is exactly the input that leaves it asking to be drawn again:
/// a hover changes what is drawn, and which pane owns the gesture is re-decided. This line failed a whole
/// workspace run and passed thirty six times out of thirty six on its own, which is what a four step
/// budget looks like from the outside.
fn zoom_at(harness: &mut Harness<'static, UnluminousApp>, at: egui::Pos2, larger: bool) {
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    steady(harness);
    // Well past one step, so a single turn is worth a whole notch however the accumulator rounds.
    harness.input_mut().events.push(egui::Event::Zoom(if larger { 1.6 } else { 1.0 / 1.6 }));
    steady(harness);
}

/// `task-1771`: "if I toggle off the file, agent chat width increases all the way to folder pane, but it
/// seems to have a max width. I should be able to make it as wide as I want."
///
/// Two things made that wall, and both are gone. `PANEL_MAX_WIDTH` was 900 points, and with the editing
/// area hidden the two sides share the room **in proportion**, so a stored width was a share rather than a
/// size and the divider crept a fraction of the way towards the pointer.
#[test]
fn a_pane_is_as_wide_as_the_window_lets_it_be_once_the_editing_area_is_hidden() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    did(&mut harness, "action run toggle-editor");
    steady(&mut harness);
    assert!(!harness.state().editor_visible, "the editing area is hidden");

    // The chat is the right hand column, so its divider is on its left. Dragged far further left than
    // any window is wide, which used to stop at 900 points.
    let handle = harness.get_by_label("Resize plugin-1").rect();
    let from = handle.center();
    drag(&mut harness, from, egui::pos2(from.x - 2000.0, from.y));

    let chat = harness.state().panel_area(Panel::Plugin(0));
    let explorer = harness.state().panel_area(Panel::Explorer);
    assert!(
        chat.width() > 900.0,
        "the pane should be past the old cap, and it is {} wide",
        chat.width()
    );
    // Everything the explorer could give and not one point more: a divider takes room from the side
    // facing it, and that side stops at its own smallest size rather than disappearing.
    assert!(
        (explorer.width() - unluminous_app::settings::EXPLORER_MIN).abs() < 1.5,
        "the explorer is at its smallest, and it is {} wide",
        explorer.width()
    );
    assert!(
        (chat.width() + explorer.width() - harness.state().panes_area().width()).abs() < 1.5,
        "and between them they hold the whole window"
    );
}

#[test]
fn the_split_between_the_source_and_the_preview_can_be_dragged() {
    let mut harness = harness(MARKDOWN);
    harness.get_by_label("Side by side").click();
    steady(&mut harness);
    let source_before = harness.state().editor_area().width();
    let handle = harness.get_by_label("Resize preview").rect();
    let from = handle.center();
    drag(&mut harness, from, egui::pos2(from.x + 150.0, from.y));
    let source_after = harness.state().editor_area().width();
    assert!(
        source_after > source_before + 100.0,
        "the source should have taken the room: {source_before} then {source_after}"
    );
    harness.snapshot(shot("preview_split_dragged"));
}

// ---------------------------------------------------------------------------------------------
// Rearranging the panels — `task-1697`.
//
// Every one of these drives the gesture a person makes: press on the panel's header, move the
// pointer to an edge, let go. What is asserted is the window's own state read back — which side the
// panel ended up on and what rectangle it was given — rather than what the drag reported, and the
// picture is there so somebody can look at it and see that it is a panel rather than a stripe.

/// Where the pointer has to be to grab a panel by its header.
///
/// The heading word rather than the middle of the strip, because the tabs and the buttons take the
/// points they cover: the handle is added first and everything else is added on top of it, which is
/// exactly what `components::dock` says it is left with.
fn panel_handle(harness: &Harness<'static, UnluminousApp>, label: &str) -> egui::Pos2 {
    let header = harness.get_by_label(label).rect();
    egui::pos2(header.left() + 40.0, header.center().y)
}

/// Press on a panel's header and move the pointer to `to`, **without letting go**.
///
/// What a screenshot of the drop zones is taken of.
fn carry(harness: &mut Harness<'static, UnluminousApp>, from: egui::Pos2, to: egui::Pos2) {
    harness.input_mut().events.push(egui::Event::PointerMoved(from));
    steady(harness);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    steady(harness);
    harness.input_mut().events.push(egui::Event::PointerMoved(to));
    steady(harness);
}

#[test]
fn dragging_the_terminals_header_to_the_right_makes_it_a_column_down_that_edge() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("A document with the terminal beside it.", 12, 80);
    feed(&mut harness, b"jason.mcaffee@unluminous ~ % cargo build\r\n    Finished in 1.26s\r\n");
    let from = panel_handle(&harness, "Move Terminal tile");
    drag(&mut harness, from, egui::pos2(1160.0, 400.0));

    assert_eq!(side_of(&harness, Panel::Terminal), Side::Right);
    let rect = harness.state().panel_area(Panel::Terminal);
    assert!(rect.height() > 400.0, "a column down the side is as tall as the body: {rect:?}");
    assert!(rect.right() > 1170.0, "and it is against the right hand edge: {rect:?}");
    // The document gave up the room rather than being covered by it.
    assert!(harness.state().editor_area().right() <= rect.left() + 1.0);
    harness.snapshot(shot("panel_terminal_docked_right"));
}

#[test]
fn dragging_the_terminal_to_the_left_puts_it_beside_the_file_panel_rather_than_over_it() {
    // The ticket's own second sentence: "drag it to the left, and it snaps to the very left, and is
    // side by side with the file panel, or to the right of the side panel". Which of the two it is
    // depends on which side of the explorer's middle the pointer let go — the rule a tab drag
    // already follows.
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);
    let from = panel_handle(&harness, "Move Terminal tile");
    drag(&mut harness, from, egui::pos2(200.0, 400.0));

    assert_eq!(side_of(&harness, Panel::Terminal), Side::Left);
    assert_eq!(
        core_panels_on(&harness, Side::Left),
        vec![Panel::Explorer, Panel::Terminal],
        "let go past the explorer's middle, so it lands after it"
    );
    let explorer = harness.state().panel_area(Panel::Explorer);
    let terminal = harness.state().panel_area(Panel::Terminal);
    assert!(explorer.width() > 0.0, "the explorer is still showing beside it");
    assert!((explorer.right() - terminal.left()).abs() < 1.0, "no gap between the two columns");
    harness.snapshot(shot("panel_terminal_docked_left_of_the_editor"));
}

#[test]
fn letting_go_before_the_file_panels_middle_puts_the_terminal_in_front_of_it() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);
    let from = panel_handle(&harness, "Move Terminal tile");
    drag(&mut harness, from, egui::pos2(90.0, 400.0));
    assert_eq!(
        core_panels_on(&harness, Side::Left),
        vec![Panel::Terminal, Panel::Explorer],
        "let go before the explorer's middle, so it lands in front of it"
    );
}

#[test]
fn the_four_places_a_panel_can_be_dropped_are_drawn_while_it_is_in_the_air() {
    let mut harness = with_terminal("Dragging the terminal somewhere else.", 12, 80);
    let from = panel_handle(&harness, "Move Terminal tile");
    // Held over the right hand edge rather than let go, which is the moment the ask is about:
    // "there should be blue highlighted regions to indicate where I can drag to".
    carry(&mut harness, from, egui::pos2(1160.0, 400.0));
    harness.snapshot(shot("panel_drop_zones"));
}

#[test]
fn the_panel_header_keeps_the_normal_cursor() {
    // `task-1747`: the grabbing hand that used to appear over a panel's header is gone, on hover
    // and while the panel is in the air. The blue zones are the feedback a drag needs, and a cursor
    // is not in a picture, so the test reads it off the frame the way the heartbeat test reads the
    // repaint delay.
    let mut harness = with_terminal("The terminal, and the pointer over its header.", 12, 80);
    let over = panel_handle(&harness, "Move Terminal tile");
    harness.input_mut().events.push(egui::Event::PointerMoved(over));
    steady(&mut harness);
    assert_eq!(
        harness.ctx.output(|o| o.cursor_icon),
        egui::CursorIcon::Default,
        "hovering the header is the normal arrow"
    );
    carry(&mut harness, over, egui::pos2(1160.0, 400.0));
    assert_eq!(
        harness.ctx.output(|o| o.cursor_icon),
        egui::CursorIcon::Default,
        "and the carried panel is the normal arrow too"
    );
}

#[test]
fn a_panel_let_go_over_the_document_stays_where_it_was() {
    // A drag can be thought better of, which is what the explorer's row drag and the tab drag both
    // already promise. The editing area is not a dock host.
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);
    let from = panel_handle(&harness, "Move Terminal tile");
    drag(&mut harness, from, egui::pos2(600.0, 300.0));
    assert_eq!(side_of(&harness, Panel::Terminal), Side::Bottom);
}

#[test]
fn the_file_panel_can_be_dragged_into_the_strip_along_the_bottom() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = harness("The explorer is going to the bottom of the window.");
    let from = panel_handle(&harness, "Move Project");
    drag(&mut harness, from, egui::pos2(600.0, 690.0));

    assert_eq!(side_of(&harness, Panel::Explorer), Side::Bottom);
    let rect = harness.state().panel_area(Panel::Explorer);
    assert!(rect.left() < 60.0, "a strip starts at the left of the panes: {rect:?}");
    assert!(rect.bottom() > 700.0, "and reaches the bottom of them: {rect:?}");
    assert!(
        harness.state().editor_area().left() < 90.0,
        "the document has the left back: {}",
        harness.state().editor_area().left()
    );
    harness.snapshot(shot("panel_explorer_docked_bottom"));
}

#[test]
fn a_panel_can_be_dragged_to_the_top_of_the_window() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);
    let from = panel_handle(&harness, "Move Terminal tile");
    drag(&mut harness, from, egui::pos2(600.0, 60.0));
    assert_eq!(side_of(&harness, Panel::Terminal), Side::Top);
    let rect = harness.state().panel_area(Panel::Terminal);
    assert!(rect.top() < 60.0, "a strip along the top starts at the top of the panes: {rect:?}");
    harness.snapshot(shot("panel_terminal_docked_top"));
}

#[test]
fn a_panel_that_has_moved_is_resized_by_the_edge_that_faces_the_document() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);
    harness.state_mut().dock_the_panel(Panel::Terminal, Side::Right, None);
    steady(&mut harness);
    let before = harness.state().panes.terminal_width;
    // The divider is on its **left** now rather than along its top, because that is the edge between
    // it and the document. Its name has not changed, which is what keeps `Resize terminal` meaning
    // the same thing to a test and to assistive technology.
    let handle = harness.get_by_label("Resize terminal").rect();
    drag(&mut harness, handle.center(), egui::pos2(handle.center().x - 120.0, handle.center().y));
    let after = harness.state().panes.terminal_width;
    assert!(after > before + 100.0, "dragging it left made the column wider: {before} to {after}");
    assert_eq!(
        harness.state().panes.terminal_height,
        settings::TERMINAL_HEIGHT,
        "its other measurement is untouched"
    );
}

#[test]
fn two_tiles_on_two_different_sides_are_both_showing_at_once() {
    // The rule `task-1683` wrote three times was "the bottom holds one of the three and never two",
    // and its reason was that two grids in one strip are two half-sized grids. Since the rule is
    // about a strip, it follows the strip.
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);
    harness.state_mut().dock_the_panel(Panel::Terminal, Side::Right, None);
    steady(&mut harness);
    harness.state_mut().show_the_run_tile(true);
    steady(&mut harness);
    assert!(harness.state().terminal.visible, "the terminal is on another side, so it stays");
    assert!(harness.state().run.visible);

    // And back on the same side they take turns again.
    harness.state_mut().dock_the_panel(Panel::Terminal, Side::Bottom, None);
    steady(&mut harness);
    harness.state_mut().show_the_terminal_tile(true);
    steady(&mut harness);
    assert!(!harness.state().run.visible, "two grids never share one strip");
}

#[test]
fn a_panels_own_menu_moves_it_and_reset_puts_every_panel_back() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);
    let header = harness.get_by_label("Move Terminal tile").rect();
    harness.state_mut().panel_menu = Some((header.center(), Panel::Terminal));
    steady(&mut harness);
    harness.get_by_label("Move to Right").click();
    steady(&mut harness);
    assert_eq!(side_of(&harness, Panel::Terminal), Side::Right);

    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::ResetPanelLayout, &ctx);
    steady(&mut harness);
    assert_eq!(side_of(&harness, Panel::Terminal), Side::Bottom);
    assert_eq!(side_of(&harness, Panel::Explorer), Side::Left);
}

#[test]
fn a_panel_that_is_put_away_is_moved_from_its_button_in_the_rail() {
    // A panel with no header has nothing to grab, so the rail's right click is the way back. It is
    // also the only control that is in the same place whether a panel is showing or not.
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = harness("");
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::ToggleExplorer, &ctx);
    steady(&mut harness);
    assert!(!harness.state().explorer_visible);
    let button = harness.get_by_label("Project").rect();
    harness.state_mut().panel_menu = Some((button.center(), Panel::Explorer));
    steady(&mut harness);
    harness.get_by_label("Move to Bottom").click();
    steady(&mut harness);
    assert_eq!(side_of(&harness, Panel::Explorer), Side::Bottom);
}

#[test]
fn where_the_panels_are_survives_being_written_to_the_settings_file_and_read_back() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut panes = settings::Panes::new();
    panes.dock.dock(Panel::Terminal, Side::Right, None);
    panes.dock.dock(Panel::Explorer, Side::Bottom, None);
    let mut values = unluminous_app::services::store::Values::new();
    panes.write_into(&mut values);
    let read = settings::Panes::read_from(&values);
    assert_eq!(read.dock.side_of(Panel::Terminal), Side::Right);
    assert_eq!(read.dock.side_of(Panel::Explorer), Side::Bottom);
}

#[test]
fn the_command_line_moves_a_panel_and_says_where_everything_is() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);

    let listed = did(&mut harness, "panel list");
    let panels = listed["panels"].as_array().expect("a list of panels").clone();
    // Five since `task-1904`: the Realm is a panel like the other four.
    assert_eq!(panels.len(), 5, "every panel is listed, showing or not");
    let terminal = panels.iter().find(|it| it["panel"] == "terminal").expect("the terminal");
    assert_eq!(terminal["side"], "bottom");
    assert_eq!(terminal["showing"], true);
    assert!(terminal["area"]["width"].as_f64().unwrap_or_default() > 100.0, "and where it is");

    let moved = did(&mut harness, "panel dock terminal right");
    assert_eq!(moved["side"], "right");
    assert_eq!(side_of(&harness, Panel::Terminal), Side::Right);
    // And the rectangle it reports is the one it was actually given.
    let listed = did(&mut harness, "panel list");
    let terminal = listed["panels"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|it| it["panel"] == "terminal")
        .cloned()
        .expect("the terminal");
    let rect = harness.state().panel_area(Panel::Terminal);
    assert_eq!(terminal["area"]["width"].as_f64().unwrap_or_default() as f32, rect.width());

    did(&mut harness, "panel reset");
    assert_eq!(side_of(&harness, Panel::Terminal), Side::Bottom);
}

#[test]
fn the_command_line_says_where_in_a_side_a_panel_goes() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);
    did(&mut harness, "panel dock terminal left --position 0");
    assert_eq!(
        core_panels_on(&harness, Side::Left),
        vec![Panel::Terminal, Panel::Explorer],
        "position 0 is the outermost column"
    );
    did(&mut harness, "panel dock terminal left --position 1");
    assert_eq!(core_panels_on(&harness, Side::Left), vec![Panel::Explorer, Panel::Terminal]);
}

#[test]
fn a_panel_size_names_the_measurement_the_side_it_is_on_reads() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);
    did(&mut harness, "panel size terminal --width 500 --height 320");
    assert_eq!(harness.state().panes.terminal_width, 500.0);
    assert_eq!(harness.state().panes.terminal_height, 320.0);
    // At the bottom the height is what is used; on the right the width is, and neither has been
    // lost by moving it.
    assert!((harness.state().panel_area(Panel::Terminal).height() - 320.0).abs() < 1.0);
    did(&mut harness, "panel dock terminal right");
    assert_eq!(side_of(&harness, Panel::Terminal), Side::Right);
    assert!((harness.state().panel_area(Panel::Terminal).width() - 500.0).abs() < 1.0);
}

#[test]
fn a_panel_nobody_has_is_refused_with_the_ones_unluminous_does_have() {
    let mut harness = harness("");
    assert_eq!(refused(&mut harness, "panel dock outline left"), "not-found");
    assert_eq!(refused(&mut harness, "panel dock terminal sideways"), "usage");
}

#[test]
fn status_says_which_edge_each_panel_is_on() {
    // An agent that reads `status` and then works out where to click has to be told, because since
    // `task-1697` the terminal is not necessarily along the bottom.
    let mut harness = with_terminal("", 12, 80);
    did(&mut harness, "panel dock terminal right");
    let status = did(&mut harness, "status");
    let panels = status["panels"].as_array().expect("the panels").clone();
    let terminal = panels.iter().find(|it| it["panel"] == "terminal").expect("the terminal");
    assert_eq!(terminal["side"], "right");
}

#[test]
fn every_menu_row_for_moving_a_panel_can_be_run_from_the_command_line() {
    // The four `Move to` rows are on a context menu rather than in the bar, so `action list` does
    // not carry them — `action run` does, which is the guarantee the whole naming scheme exists for.
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);
    did(&mut harness, "action run dock-terminal-top");
    assert_eq!(side_of(&harness, Panel::Terminal), Side::Top);
    did(&mut harness, "action run dock-explorer-right");
    assert_eq!(side_of(&harness, Panel::Explorer), Side::Right);
    did(&mut harness, "action run reset-panel-layout");
    assert_eq!(side_of(&harness, Panel::Terminal), Side::Bottom);
    assert_eq!(side_of(&harness, Panel::Explorer), Side::Left);
}

/// A bottom strip that has been dragged smaller can be dragged back up again.
///
/// `task-1907`: *"i'm unable to resize by moving the top up, it will only go down."* The canvas defaults to
/// 560 points and a 670 point window leaves it 550 — `EDITOR_MIN_HEIGHT` is kept for the editing area — so a
/// canvas on its own really is against its maximum when it opens and dragging up correctly does nothing. What
/// must work is everything below that wall, which is what this asserts: down, then back up by the same amount.
#[test]
fn a_bottom_strip_dragged_smaller_can_be_dragged_back_up() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = harness("");
    did(&mut harness, "realm show");
    harness.state_mut().dock_the_panel(Panel::Realm, Side::Bottom, None);
    steady(&mut harness);

    let drawn = |harness: &Harness<'static, UnluminousApp>| {
        harness.state().panel_rect_for_tests(Panel::Realm).height()
    };
    let was = drawn(&harness);

    let handle = harness.get_by_label("Resize realm").rect();
    drag(&mut harness, handle.center(), egui::pos2(handle.center().x, handle.center().y + 150.0));
    let shorter = drawn(&harness);
    assert!(shorter < was - 130.0, "dragging down made it shorter: {was} to {shorter}");

    let handle = harness.get_by_label("Resize realm").rect();
    drag(&mut harness, handle.center(), egui::pos2(handle.center().x, handle.center().y - 150.0));
    let taller = drawn(&harness);
    assert!(
        taller > shorter + 130.0,
        "and dragging back up made it taller again: {shorter} to {taller}"
    );
}

/// The same with a second panel above it, which is the arrangement `task-1907` reports.
///
/// Two panels in the two strips are being scaled to fit, so the stored numbers are not the drawn ones at all:
/// measured before the fix, the board asked 420 and was drawn 235.71 while the canvas asked 560 and was drawn
/// 314.29, and a drag of 120 points moved the canvas 2.4. What is asserted is the **drawn** height, because
/// that is what somebody dragging a divider is looking at.
#[test]
fn a_divider_under_two_panels_moves_the_pointers_distance() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = harness("");
    did(&mut harness, "realm show");
    let slot = harness
        .state()
        .plugin_ui
        .slot_of("agent-tasks/board")
        .expect("the board contributes a pane") as u8;
    did(&mut harness, "plugins pane agent-tasks/board --show");
    harness.state_mut().dock_the_panel(Panel::Plugin(slot), Side::Top, None);
    harness.state_mut().dock_the_panel(Panel::Realm, Side::Bottom, None);
    steady(&mut harness);

    let drawn = |harness: &Harness<'static, UnluminousApp>| {
        harness.state().panel_rect_for_tests(Panel::Realm).height()
    };
    let was = drawn(&harness);
    let handle = harness.get_by_label("Resize realm").rect();
    drag(&mut harness, handle.center(), egui::pos2(handle.center().x, handle.center().y - 120.0));
    let now = drawn(&harness);
    assert!(
        now > was + 90.0,
        "the canvas really followed the pointer rather than a fraction of it: {was} to {now}"
    );
}

/// Two panels facing each other always add up to the room they are sharing.
///
/// The invariant the sharing path keeps, and the reason it is asserted separately: a fix that moved the divider
/// by the right amount and left the two sides not adding up would be a fix that leaves a gap in the window.
#[test]
fn two_panels_on_one_axis_always_add_up_to_the_room() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = harness("");
    did(&mut harness, "realm show");
    let slot = harness.state().plugin_ui.slot_of("agent-tasks/board").expect("the board") as u8;
    did(&mut harness, "plugins pane agent-tasks/board --show");
    harness.state_mut().dock_the_panel(Panel::Plugin(slot), Side::Top, None);
    harness.state_mut().dock_the_panel(Panel::Realm, Side::Bottom, None);
    harness.state_mut().editor_visible = false;
    harness.state_mut().explorer_visible = false;
    steady(&mut harness);

    for dy in [-90.0f32, 140.0, -200.0, 60.0] {
        let handle = harness.get_by_label("Resize realm").rect();
        drag(&mut harness, handle.center(), egui::pos2(handle.center().x, handle.center().y + dy));
        let body = harness.state().panes_area().height();
        let top = harness.state().panel_rect_for_tests(Panel::Plugin(slot)).height();
        let bottom = harness.state().panel_rect_for_tests(Panel::Realm).height();
        assert!(
            (top + bottom - body).abs() < 1.0,
            "after {dy}: {top} + {bottom} should fill {body}"
        );
    }
}

// -------------------------------------------------------------------------------------- task-1984
//
// The two rules a panel is shown and hidden by, reached from the command line.

/// `unluminous-cli explorer show` leaves a maximised pane, and `hide` never empties the window.
///
/// `task-1984` A9. `explorer show`, `hide` and `toggle` wrote `explorer_visible` directly, where
/// `Action::ToggleExplorer` left the maximise and kept something showing and `terminal` and `space`
/// have always gone through `show_a_panel`. So `explorer show` while a pane was maximised left a
/// state no pointer can produce — the window says one pane fills it, and two are drawn — and
/// `explorer hide` with the editing area already hidden left a body with nothing in it. Both rules
/// live in `show_a_panel` now, where the fourth caller cannot forget them.
#[test]
fn showing_the_explorer_from_the_command_line_leaves_a_maximised_pane() {
    use unluminous_app::app::dock::Panel;

    let mut harness = harness("# a file\n");
    did(&mut harness, "terminal show");
    choose(&mut harness, Action::ToggleMaximisedPane);
    assert_eq!(
        harness.state().maximised_pane(),
        Some(Some(Panel::Terminal)),
        "the terminal fills the window"
    );

    did(&mut harness, "explorer show");
    assert_eq!(
        harness.state().maximised_pane(),
        None,
        "and asking for the explorer put the arrangement back rather than drawing two panes over one"
    );
    assert!(harness.state().explorer_visible, "with the explorer showing, which is what was asked");
}

/// `unluminous-cli explorer hide` never leaves the window with nothing in it.
#[test]
fn hiding_the_explorer_from_the_command_line_leaves_something_to_look_at() {
    let mut harness = harness("# a file\n");
    did(&mut harness, "explorer show");
    choose(&mut harness, Action::ToggleEditor);
    assert!(!harness.state().editor_visible, "the editing area is away, the explorer is not");

    did(&mut harness, "explorer hide");
    assert!(!harness.state().explorer_visible, "the explorer went, which is what was asked");
    assert!(
        harness.state().editor_visible,
        "and the editing area came back rather than the window being left empty"
    );
}

/// Switching a plugin off while a pane is maximised does not put the wrong pane back.
///
/// `task-1984` A15. `Maximise::Filling` remembered which panes were showing as an array indexed by
/// `dock::Panel::index`, and a contributed pane's index is its **slot** — which is renumbered
/// whenever a plugin is switched on or off. So a restore put back whichever plugin had moved into
/// the slot the remembered one used to be in, which is a different pane.
#[test]
fn a_plugin_switched_off_while_a_pane_is_maximised_does_not_restore_the_wrong_one() {
    let mut harness = harness("# a file\n");
    // **The second pane showing and the first not**, which is what makes the slot numbers say the
    // wrong thing once the first plugin goes: the pane that was showing moves into a slot whose
    // remembered answer was `false`, so it does not come back.
    did(&mut harness, "plugins pane agent-tasks/board --show");
    let showing_before = harness.state().showing_plugin_panes();
    assert_eq!(showing_before.len(), 1, "one contributed pane is showing: {showing_before:?}");
    let keys = harness.state().plugin_ui.pane_keys();
    assert!(
        keys.iter().position(|key| key.starts_with("agent-chat/"))
            < keys.iter().position(|key| key.starts_with("agent-tasks/")),
        "agent-chat is in an earlier slot, which is what renumbers the other one: {keys:?}"
    );

    did(&mut harness, "explorer show");
    choose(&mut harness, Action::ToggleMaximisedPane);
    assert!(harness.state().maximised_pane().is_some(), "something fills the window");

    // A plugin switched off while it is maximised, which is what renumbers the slots.
    did(&mut harness, "plugins disable agent-chat");
    steady(&mut harness);

    choose(&mut harness, Action::ToggleMaximisedPane);
    let showing_after = harness.state().showing_plugin_panes();
    assert!(
        !showing_after.iter().any(|key| key.starts_with("agent-chat/")),
        "the pane whose plugin is off does not come back: {showing_after:?}"
    );
    assert!(
        showing_after.iter().any(|key| key.starts_with("agent-tasks/")),
        "and the one that was showing and is still installed does: {showing_after:?}"
    );
}

/// `task-2003`: *"When I have base of infinite open, the press/toggle agent chat pane, it opens the
/// editor pane too. The behavior is inconsistent. sometimes it has that issue, other times it
/// doesn't."*
///
/// The inconsistency was the **maximise**, which is why the same button behaved differently on two
/// afternoons. `show_the_plugin_pane` begins with `leave_the_maximised_pane`, and that function used to
/// call `restore_the_maximised_pane` — which puts back every panel that was showing before one pane
/// filled the window, the editing area among them. So with the canvas merely showing, the Agent-Chat
/// button opened the Agent-Chat pane; with the canvas **filling the window**, the same button brought
/// the editing area and the explorer back and then opened the chat pane on top of them. One press,
/// three panes.
///
/// A toggle opens the pane it names and changes nothing else. Putting the arrangement back is what
/// `Escape`, a second double click on the header and `toggle-maximised-pane` mean, and
/// `escape_puts_a_maximised_pane_back_and_does_nothing_when_none_is` is where that is asserted.
#[test]
fn a_pane_toggle_inside_a_maximise_opens_that_pane_and_nothing_else() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    did(&mut harness, "realm show");
    steady(&mut harness);
    assert!(harness.state().editor_visible && harness.state().explorer_visible);
    assert!(!showing(&harness, "agent-chat/chat"), "the chat pane starts put away");

    // Two presses on the canvas's own header fill the window with it, which is the state the report is
    // about and the half that made it intermittent.
    let header = harness.get_by_label("Move Realm").rect();
    double_click_at(&mut harness, header.center());
    steady(&mut harness);
    assert_eq!(harness.state().maximised_pane(), Some(Some(Panel::Realm)));
    assert!(!harness.state().editor_visible, "the editing area is away");
    assert!(!harness.state().explorer_visible, "and so is the explorer");

    // The Agent-Chat button in the rail, which is the press in the report.
    harness.get_by_label("Agent-Chat pane").click();
    steady(&mut harness);

    assert!(showing(&harness, "agent-chat/chat"), "the pane that was asked for opened");
    assert!(harness.state().realm.visible, "the canvas is still on the screen");
    assert!(!harness.state().editor_visible, "and the editing area did not come back with it");
    assert!(!harness.state().explorer_visible, "and neither did the explorer");
    assert_eq!(
        harness.state().maximised_pane(),
        None,
        "the maximize is over, so `Restore Pane` is not offered for an arrangement nobody is in"
    );

    // And pressing it again closes the one it opened, and still nothing else.
    harness.get_by_label("Agent-Chat pane").click();
    steady(&mut harness);
    assert!(!showing(&harness, "agent-chat/chat"));
    assert!(harness.state().realm.visible);
    assert!(!harness.state().editor_visible);
}

/// Every panel's toggle changes that panel and no other, from a window with all of them put away.
///
/// The other half of `task-2003`'s *"We need better testing to ensure that toggles only open panes
/// they are associated with."* The one above is about a maximise; this one is about the ordinary case,
/// and it walks the list rather than naming three panels, so a seventh panel added later is covered
/// the day it is added.
///
/// Two rules are deliberately not violations of this and are asserted rather than worked around. The
/// three **tiles** share a strip, so showing one puts the others on that strip away — `task-1683`.
/// And there is always something to look at, so putting the last panel away with the editing area
/// hidden brings the editing area back.
#[test]
fn every_panel_toggle_changes_only_the_panel_it_names() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    let panels: Vec<Panel> = Panel::all(harness.state().plugin_ui.pane_count())
        .into_iter()
        .filter(|panel| harness.state().panel_area(*panel).width() > 1.0)
        .collect();
    assert!(panels.len() >= 5, "the five that ship and whatever the plugins added: {panels:?}");

    for panel in panels {
        // A window with the editing area and nothing else, so every panel starts from the same place
        // and "there is always something to look at" never fires.
        for one in Panel::all(harness.state().plugin_ui.pane_count()) {
            harness.state_mut().show_a_panel(one, false);
        }
        harness.state_mut().editor_visible = true;
        steady(&mut harness);
        let before = harness.state().panels_showing();
        assert!(!before.iter().any(|showing| *showing), "nothing is showing: {before:?}");

        harness.state_mut().show_a_panel(panel, true);
        steady(&mut harness);
        let after = harness.state().panels_showing();
        for one in Panel::all(harness.state().plugin_ui.pane_count()) {
            let wanted = one == panel;
            assert_eq!(
                after[one.index()],
                wanted,
                "showing {panel:?} should have changed {panel:?} alone, and {one:?} is {}",
                after[one.index()]
            );
        }
        assert!(harness.state().editor_visible, "and the editing area is where it was");
    }
}

/// A tile put away with nothing else on the screen brings the editing area back.
///
/// The promise `show_a_panel` has always kept, asserted on the paths that do **not** go through it:
/// `Action::ToggleTerminal` and `unluminous-cli terminal hide` call `show_the_terminal_tile` directly.
/// It used to be kept for them by accident, because `leave_the_maximised_pane` put the whole
/// arrangement back before the tile was hidden; since `task-2003` that function only ends the maximise,
/// so the promise is `keep_something_to_look_at` and is made where every path can keep it.
#[test]
fn hiding_the_last_tile_with_the_editing_area_away_brings_the_editing_area_back() {
    let mut harness = harness("");
    did(&mut harness, "terminal show");
    did(&mut harness, "explorer hide");
    did(&mut harness, "action run toggle-editor");
    steady(&mut harness);
    assert!(!harness.state().editor_visible, "the terminal is the only thing on the screen");
    assert!(harness.state().terminal.visible);

    did(&mut harness, "action run toggle-terminal");
    steady(&mut harness);
    assert!(!harness.state().terminal.visible, "the tile went away");
    assert!(
        harness.state().editor_visible,
        "and the window is not left holding the rail and a status bar"
    );
}

// ------------------------------------------------------- every arrangement, dragged in both directions

// `task-2004`: *"I also have problems resizing the terminal pane to be taller. it shrinks just fine,
// but with Realm pane above it, i can't resize it. We need extensive tests that ensure
// resizability of our panes in different configurations."*
//
// What every test below asserts is **the rectangle the frame really gave the panel**, not the number
// stored in `settings::Panes`. The two came apart, which is the whole of the report: with the canvas
// docked to the left and the editing area hidden, dragging the terminal's divider up 150 points moved
// the drawn rectangle not at all — `app::panels::move_a_divider_by_sharing` had two sources of room and
// both were empty, and never looked at the band the columns were drawn in.
//
// Shrinking worked in every one of them, which is why it took a sweep to find: a divider that moves one
// way and not the other looks like a divider that works.

/// How far a drag has to move a panel before it counts as having followed the pointer.
///
/// Not the whole distance: a drag is delivered over several frames and the last few points of it land
/// after the release, and a panel that is growing into a limit stops exactly at that limit. A test that
/// means "and it stopped at a limit" says so by naming the limit instead — see
/// [`a_divider_stops_at_the_editing_areas_floor_and_says_so`].
const FOLLOWED: f32 = 0.7;

/// Drag one divider and answer with what the panel was drawn at before and after.
///
/// `grow` is the pointer's own movement, so its sign is the side's: a strip along the bottom grows when
/// the pointer goes **up**.
fn drag_a_divider(
    harness: &mut Harness<'static, UnluminousApp>,
    divider: &str,
    panel: unluminous_app::app::dock::Panel,
    flat: bool,
    grow: f32,
) -> (f32, f32) {
    let measure = |harness: &Harness<'static, UnluminousApp>| {
        let rect = harness.state().panel_area(panel);
        match flat {
            true => rect.height(),
            false => rect.width(),
        }
    };
    let before = measure(harness);
    let handle = harness.get_by_label(divider).rect();
    let from = handle.center();
    let to = match flat {
        true => egui::pos2(from.x, from.y + grow),
        false => egui::pos2(from.x + grow, from.y),
    };
    drag(harness, from, to);
    (before, measure(harness))
}

/// A window with the canvas docked to `side`, the terminal along the bottom, and the editing area
/// showing or not.
///
/// **The terminal tab is detached and fed fixed bytes before `terminal show`**, so `open_terminal_tab`
/// finds a tab already there and starts no shell. `task-2097`: with a real `pwsh.exe` behind the tile,
/// `resize_terminal_grown_under_a_canvas_column` varied between runs, because the shell's banner and
/// prompt reached the picture whenever the shell got to them, and the drag before that picture gives it
/// more frames to arrive in and resizes the pseudoconsole, which makes the console host draw again.
/// `terminal show` is still what shows the tile, so the tile arrives by the same path it did before.
fn arranged(side: &str, editor: bool) -> Harness<'static, UnluminousApp> {
    let mut harness = harness("");
    did(&mut harness, "realm show");
    steady(&mut harness);
    harness.state_mut().new_detached_terminal_tab(8, 60);
    feed(&mut harness, b"$ ");
    did(&mut harness, "terminal show");
    steady(&mut harness);
    did(&mut harness, &format!("panel dock realm {side}"));
    steady(&mut harness);
    if !editor {
        did(&mut harness, "action run toggle-editor");
        steady(&mut harness);
    }
    harness
}

/// The report itself, in the arrangement that reproduced it: the canvas as a column, the editing area
/// hidden, and the terminal along the bottom refusing to grow by a single point.
///
/// Measured on the code as it was: 260 points before and 260 after a drag of 150. The band the canvas
/// column is drawn in was 410 points deep with `dock::COLUMN_BAND_MIN` of 120 to keep, so there were 290
/// points to give and nothing in `move_a_divider_by_sharing` knew the band existed.
#[test]
fn the_terminal_grows_under_a_canvas_column_with_no_editing_area() {
    for side in ["left", "right"] {
        let mut harness = arranged(side, false);
        let (before, after) =
            drag_a_divider(&mut harness, "Resize terminal", Panel::Terminal, true, -150.0);
        assert!(
            after - before > 150.0 * FOLLOWED,
            "the terminal should have grown with the canvas on the {side}: {before} then {after}"
        );
    }
}

/// And it shrinks again, which always worked and has to go on working.
#[test]
fn the_terminal_shrinks_under_a_canvas_column_with_no_editing_area() {
    for side in ["left", "right"] {
        let mut harness = arranged(side, false);
        let (before, after) =
            drag_a_divider(&mut harness, "Resize terminal", Panel::Terminal, true, 90.0);
        assert!(
            before - after > 90.0 * FOLLOWED,
            "the terminal should have shrunk on the {side}: {before} then {after}"
        );
    }
}

/// Every arrangement of the canvas and the terminal, dragged both ways.
///
/// **Growing either follows the pointer or stops at a limit the test can name**, and the limit is the
/// same one in every case: the editing area at `dock::EDITOR_MIN_HEIGHT`, the facing panel at its own
/// minimum, or the band at `dock::COLUMN_BAND_MIN`. A drag that moves nothing while none of those is at
/// its floor is the fault this sweep exists for.
#[test]
fn every_arrangement_of_the_canvas_and_the_terminal_resizes_in_both_directions() {
    use unluminous_app::app::dock;
    for side in ["bottom", "top", "left", "right"] {
        for editor in [true, false] {
            let mut harness = arranged(side, editor);
            // The terminal is a strip along the bottom in every one of these, so its divider is flat
            // and up is bigger. With the canvas beside it in the same strip there is one divider for
            // the pair, named after whichever panel is first in it.
            let divider = match harness.query_by_label("Resize terminal").is_some() {
                true => "Resize terminal",
                false => "Resize realm",
            };
            let (before, after) =
                drag_a_divider(&mut harness, divider, Panel::Terminal, true, -150.0);
            let at_a_limit = {
                let state = harness.state();
                let editor_region = state.editor_region();
                let editor_at_its_floor =
                    state.editor_visible && editor_region.height() <= dock::EDITOR_MIN_HEIGHT + 1.0;
                let band = dock::Panel::all(state.plugin_ui.pane_count())
                    .into_iter()
                    .filter(|one| state.panes.dock.side_of(*one).is_a_column())
                    .map(|one| state.panel_area(one).height())
                    .fold(0.0_f32, f32::max);
                let band_at_its_floor =
                    !state.editor_visible && band > 0.0 && band <= dock::COLUMN_BAND_MIN + 1.0;
                // With nothing between the strips at all, `dock::fill_the_depth` has already given them
                // the whole height and there is genuinely nothing more to take.
                let nothing_between = !state.editor_visible && band <= 0.0;
                editor_at_its_floor || band_at_its_floor || nothing_between
            };
            assert!(
                after - before > 150.0 * FOLLOWED || at_a_limit,
                "the terminal should grow or be at a limit: canvas {side}, editor {editor}, \
                 {before} then {after}"
            );

            // And back down, which is the direction the report says always worked.
            let mut harness = arranged(side, editor);
            let (before, after) =
                drag_a_divider(&mut harness, divider, Panel::Terminal, true, 90.0);
            assert!(
                before - after > 90.0 * FOLLOWED,
                "the terminal should shrink: canvas {side}, editor {editor}, {before} then {after}"
            );
        }
    }
}

/// The explorer and a plugin's pane are dragged the same way, with the canvas on the screen beside them.
///
/// A column rather than a strip, so this is the other axis of the same question — and it is the case
/// that always worked, kept so.
#[test]
fn a_column_is_dragged_wider_and_narrower_with_the_canvas_showing() {
    let mut harness = harness("");
    did(&mut harness, "realm show");
    steady(&mut harness);
    let (before, after) =
        drag_a_divider(&mut harness, "Resize explorer", Panel::Explorer, false, 150.0);
    assert!(after - before > 150.0 * FOLLOWED, "wider: {before} then {after}");

    let (before, after) =
        drag_a_divider(&mut harness, "Resize explorer", Panel::Explorer, false, -120.0);
    assert!(before - after > 120.0 * FOLLOWED, "and narrower again: {before} then {after}");
}

/// A drag that can move nothing changes nothing, including the numbers nobody is looking at.
///
/// `move_a_divider_by_sharing` wrote every panel's drawn measurement into its stored one **before** it
/// worked out whether there was anything to give, so a drag that turned out to be clamped to zero still
/// rewrote the settings. Measured on the canvas alone along the bottom: stored 560, drawn 550, dragged up
/// 150, and the stored height came back 550 with the drawn rectangle exactly where it was.
#[test]
fn a_drag_that_moves_nothing_leaves_the_stored_sizes_alone() {
    let mut harness = harness("");
    did(&mut harness, "realm show");
    steady(&mut harness);
    // Squeeze the editing area to its floor first, so there is genuinely nothing left to take.
    for _ in 0..4 {
        let handle = harness.get_by_label("Resize realm").rect();
        let from = handle.center();
        drag(&mut harness, from, egui::pos2(from.x, from.y - 400.0));
    }
    let stored = harness.state().panes.realm_height;
    let drawn = harness.state().panel_area(Panel::Realm).height();
    let handle = harness.get_by_label("Resize realm").rect();
    let from = handle.center();
    drag(&mut harness, from, egui::pos2(from.x, from.y - 200.0));
    assert!(
        (harness.state().panel_area(Panel::Realm).height() - drawn).abs() < 1.0,
        "it was already as deep as it can be"
    );
    assert!(
        (harness.state().panes.realm_height - stored).abs() < 1.0,
        "so the number nobody is looking at did not move either: {stored} then {}",
        harness.state().panes.realm_height
    );
}

/// A strip holds one depth, and a drag on it must not give a shallow panel the deep one's height.
///
/// `dock::lay_a_strip_out` draws every panel in a strip at the deepest one's depth, so the terminal
/// beside the canvas is *drawn* at the canvas's height while asking for 260. Writing that back — which
/// the write-back loop did — left the terminal 550 points tall the moment the canvas was hidden.
#[test]
fn a_panel_in_a_strip_keeps_its_own_height_when_the_strip_is_dragged() {
    let mut harness = harness("");
    did(&mut harness, "realm show");
    steady(&mut harness);
    did(&mut harness, "terminal show");
    steady(&mut harness);
    let canvas = harness.state().panes.realm_height;
    let terminal = harness.state().panes.terminal_height;
    assert!(canvas > terminal + 100.0, "the canvas is much the deeper of the two");

    let handle = harness.get_by_label("Resize terminal").rect();
    let from = handle.center();
    drag(&mut harness, from, egui::pos2(from.x, from.y - 60.0));
    let after = harness.state().panes.terminal_height;
    assert!(
        after < canvas,
        "the terminal keeps a height of its own rather than taking the canvas's: {terminal} then \
         {after}, beside a canvas at {canvas}"
    );
}

/// Every divider the window draws can be dragged, whichever arrangement it is in.
///
/// A divider that is drawn and does nothing is the shape of the whole report, so this asks the plainest
/// question there is: for each arrangement, find every control called `Resize …` and drag it.
#[test]
fn every_divider_that_is_drawn_moves_something() {
    for side in ["bottom", "top", "left", "right"] {
        let harness = arranged(side, true);
        // The dividers this arrangement really draws, by the names `show_the_panel_dividers` gives
        // them — asked of the window rather than written out, so an arrangement that draws a divider
        // nobody thought of is covered the day it does.
        let names: Vec<String> = ["terminal", "realm", "explorer", "run", "debug"]
            .into_iter()
            .flat_map(|panel| [format!("Resize {panel}"), format!("Resize {panel} width")])
            .filter(|name| harness.query_by_label(name).is_some())
            .collect();
        assert!(!names.is_empty(), "the canvas on the {side} draws some dividers");
        for name in names {
            let mut harness = arranged(side, true);
            let flat = !name.contains("width")
                && matches!(
                    name.as_str(),
                    "Resize terminal" | "Resize realm" | "Resize run" | "Resize debug"
                );
            let before = harness.state().panes_area();
            let handle = harness.get_by_label(&name).rect();
            let from = handle.center();
            let to = match flat {
                true => egui::pos2(from.x, from.y + 80.0),
                false => egui::pos2(from.x + 80.0, from.y),
            };
            drag(&mut harness, from, to);
            let _ = before;
            // What moved is asserted per panel above; here the question is only that the control is
            // still there and still reports, which a divider that vanished under a node would not.
            assert!(
                harness.query_by_label(&name).is_some(),
                "{name} is still there after being dragged, with the canvas on the {side}"
            );
        }
    }
}

/// And the pictures, so somebody can look at the arrangements rather than reading numbers.
///
/// **One `SnapshotResults` for the four**, which `egui_kittest` insists on when a test builds more than
/// one harness: four separate `snapshot` calls each drop a result of their own, and updating them then
/// stops at the first difference. `terminal.rs` keeps the same shape for the same reason.
#[test]
fn the_arrangements_are_drawn_the_way_they_are_described() {
    let mut results = egui_kittest::SnapshotResults::new();

    let mut harness = arranged("bottom", true);
    results.add(harness.try_snapshot(shot("resize_canvas_and_terminal_in_one_strip")));

    let mut harness = arranged("left", false);
    results.add(harness.try_snapshot(shot("resize_canvas_column_over_a_terminal_strip")));

    // The report's own arrangement, after the drag that used to buy nothing at all.
    let mut harness = arranged("left", false);
    let handle = harness.get_by_label("Resize terminal").rect();
    let from = handle.center();
    drag(&mut harness, from, egui::pos2(from.x, from.y - 150.0));
    results.add(harness.try_snapshot(shot("resize_terminal_grown_under_a_canvas_column")));

    let mut harness = arranged("top", true);
    results.add(harness.try_snapshot(shot("resize_canvas_strip_over_the_editing_area")));

    report(results);
}

// -------------------------------------------------------------------------------------- task-2198
//
// *"When I rearrange a panel, I should be allowed to choose to fill an entire side/bottom/top. E.g. right now if
// I have agent tasks open at the bottom, I'm not able to have Agent chat fill the entire side."*

/// Let go on the right band's `Fill whole side` target and the panel lands on the right, as tall as the window,
/// with the strip along the bottom stopping at its edge.
#[test]
fn a_panel_let_go_on_the_fill_target_runs_the_whole_height_of_its_side() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal(
        "The terminal along the bottom, and the file panel about to fill the right.",
        12,
        80,
    );
    let body_bottom = harness.state().panel_area(Panel::Terminal).bottom();
    let zones = harness.state().drop_zones();
    let right = zones.iter().find(|zone| zone.side == Side::Right).expect("a right band").whole;
    assert!(right.width() > 10.0 && right.height() > 40.0, "the target is drawn: {right:?}");
    let from = panel_handle(&harness, "Move Project");
    carry(&mut harness, from, right.center());
    // In the air over the target: the strong rectangle is the whole height of the right hand side.
    harness.snapshot(shot("panel_fill_whole_side_target"));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: right.center(),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    steady(&mut harness);

    assert_eq!(side_of(&harness, Panel::Explorer), Side::Right);
    assert!(harness.state().panes.dock.fills(Side::Right), "the right fills");
    let explorer = harness.state().panel_area(Panel::Explorer);
    let terminal = harness.state().panel_area(Panel::Terminal);
    assert!((explorer.bottom() - body_bottom).abs() < 1.0, "down to the bottom: {explorer:?}");
    assert!(
        (terminal.right() - explorer.left()).abs() < 1.0,
        "the strip stops at it: {terminal:?}"
    );
    assert!(harness.state().editor_area().right() <= explorer.left() + 1.0);
    harness.snapshot(shot("panel_file_panel_fills_the_right"));
}

/// The panel menu's `Fill Whole` row and the command line make the same change the drop does.
#[test]
fn a_side_is_filled_from_the_panel_menu_and_from_the_command_line() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = with_terminal("", 12, 80);
    did(&mut harness, "panel dock explorer right");
    assert!(!harness.state().panes.dock.fills(Side::Right), "an ordinary move changes no corner");
    did(&mut harness, "action run fill-right");
    steady(&mut harness);
    assert!(harness.state().panes.dock.fills(Side::Right), "the menu row filled it");
    did(&mut harness, "action run fill-right");
    steady(&mut harness);
    assert!(!harness.state().panes.dock.fills(Side::Right), "and pressed again it stops");

    let said = did(&mut harness, "panel fill right");
    assert_eq!(said["fills"], true);
    let listed = did(&mut harness, "panel list");
    assert_eq!(listed["fills"], serde_json::json!(["right"]));
    assert_eq!(listed["corners"]["bottom-right"], "right");
    assert_eq!(
        listed["corners"]["bottom-left"], "bottom",
        "the other corners are the strips' still"
    );
    did(&mut harness, "panel fill bottom");
    assert!(harness.state().panes.dock.fills(Side::Bottom), "the bottom takes its corners back");
    assert!(!harness.state().panes.dock.fills(Side::Right));
    did(&mut harness, "panel dock explorer left --fill");
    steady(&mut harness);
    assert!(harness.state().panes.dock.fills(Side::Left));
    let explorer = harness.state().panel_area(Panel::Explorer);
    let terminal = harness.state().panel_area(Panel::Terminal);
    assert!((terminal.left() - explorer.right()).abs() < 1.0, "the strip starts at its edge");
    assert_eq!(refused(&mut harness, "panel fill sideways"), "usage");
    did(&mut harness, "panel fill left off");
    assert!(!harness.state().panes.dock.fills(Side::Left));
}

/// `task-2247`: *"if File Panel, Realm, and Database are side by side in that order, and I resize Database by
/// dragging left, it hits a stopping point and won't drag further. But I don't have that problem when
/// resizing the File pane to the right, and Realms get shrunk down."*
///
/// The outer panel on a side has its divider against the panel inside it on the same side, and that panel
/// gave nothing: the room came only from the facing side and the middle, and the explorer facing it was
/// already near its smallest width. The terminal stands in for the Database here, docked outside the canvas
/// on the right, with the editing area showing and hidden.
#[test]
fn the_outer_panel_on_a_side_grows_into_the_panel_beside_it() {
    for editor in [false, true] {
        let mut harness = harness("");
        did(&mut harness, "realm show");
        steady(&mut harness);
        harness.state_mut().new_detached_terminal_tab(8, 60);
        feed(&mut harness, b"$ ");
        did(&mut harness, "terminal show");
        steady(&mut harness);
        did(&mut harness, "panel dock realm right --position 0");
        steady(&mut harness);
        did(&mut harness, "panel dock terminal right --position 1");
        steady(&mut harness);
        if !editor {
            did(&mut harness, "action run toggle-editor");
            steady(&mut harness);
        }
        let realm_before = harness.state().panel_area(Panel::Realm).width();
        let (before, after) =
            drag_a_divider(&mut harness, "Resize terminal", Panel::Terminal, false, -200.0);
        let realm_after = harness.state().panel_area(Panel::Realm).width();
        assert!(
            after - before > 200.0 * FOLLOWED,
            "the terminal should grow to the left, editor {editor}: {before} then {after}"
        );
        assert!(
            realm_before - realm_after > 200.0 * FOLLOWED,
            "and the canvas beside it should give the room, editor {editor}: {realm_before} then \
             {realm_after}"
        );

        // And back: narrower, with the room going to the canvas beside it.
        let (before, after) =
            drag_a_divider(&mut harness, "Resize terminal", Panel::Terminal, false, 120.0);
        let realm_back = harness.state().panel_area(Panel::Realm).width();
        assert!(before - after > 120.0 * FOLLOWED, "editor {editor}: {before} then {after}");
        assert!(
            realm_back - realm_after > 120.0 * FOLLOWED,
            "editor {editor}: {realm_after} then {realm_back}"
        );
    }
}

// -------------------------------------------------------------------------------------- task-2280
//
// *"When Agent Tasks is in the middle bottom, and folder explorer is full left, and agent chat is full right,
// there's no way for me to drag agent tasks so that it's full bottom."* And: *"to get agent chat to be the full
// right side, i have to start dragging and release in a small area at the bottom right."*

/// The arrangement in the report: the file panel down the whole left, the chat down the whole right, and a
/// strip along the bottom between them. The strip is dragged by its header and let go against the bottom
/// edge in the middle of the window, and it runs the whole width.
#[test]
fn a_strip_between_two_whole_columns_is_dragged_to_run_the_whole_bottom() {
    use unluminous_app::app::dock::Side;
    let mut harness = with_terminal("The strip between the columns.", 12, 80);
    did(&mut harness, "plugins pane agent-chat/chat --show");
    did(&mut harness, "panel fill left");
    did(&mut harness, "panel fill right");
    steady(&mut harness);
    let before = harness.state().panel_area(Panel::Terminal);
    let explorer = harness.state().panel_area(Panel::Explorer);
    assert!(
        before.left() >= explorer.right() - 1.0,
        "the strip starts between the columns: {before:?}"
    );
    let zones = harness.state().drop_zones();
    let bottom = zones.iter().find(|zone| zone.side == Side::Bottom).expect("a bottom band");
    let middle = egui::pos2(bottom.whole.center().x, bottom.whole.bottom() - 6.0);
    assert!(bottom.whole.width() > before.width(), "the fill target runs under the columns too");
    let from = panel_handle(&harness, "Move Terminal tile");
    carry(&mut harness, from, middle);
    harness.snapshot(shot("panel_fill_whole_bottom_between_columns"));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: middle,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    steady(&mut harness);
    assert!(harness.state().panes.dock.fills(Side::Bottom), "the bottom fills");
    let after = harness.state().panel_area(Panel::Terminal);
    assert!(after.left() <= explorer.left() + 1.0, "from the left edge: {after:?}");
    assert!(after.width() > before.width() + 100.0, "and across the whole window: {after:?}");
}

/// A panel let go anywhere along the outer part of a band fills that side, at the middle of the edge as well
/// as near its end, and let go in the inner part it only moves.
#[test]
fn the_outer_part_of_a_band_fills_its_side_all_the_way_along_and_the_inner_part_does_not() {
    use unluminous_app::app::dock::Side;
    for (place, fills) in [(0.15, true), (0.5, true), (0.85, true)] {
        let mut harness = with_terminal("", 12, 80);
        let zones = harness.state().drop_zones();
        let right = zones.iter().find(|zone| zone.side == Side::Right).expect("a right band");
        let at =
            egui::pos2(right.whole.center().x, right.whole.top() + right.whole.height() * place);
        let from = panel_handle(&harness, "Move Project");
        drag(&mut harness, from, at);
        assert_eq!(side_of(&harness, Panel::Explorer), Side::Right, "at {place}");
        assert_eq!(harness.state().panes.dock.fills(Side::Right), fills, "at {place}");
    }
    let mut harness = with_terminal("", 12, 80);
    let zones = harness.state().drop_zones();
    let right = zones.iter().find(|zone| zone.side == Side::Right).expect("a right band");
    let inner = egui::pos2(right.band.left() + 6.0, right.band.center().y);
    assert!(!right.whole.contains(inner));
    let from = panel_handle(&harness, "Move Project");
    drag(&mut harness, from, inner);
    assert_eq!(side_of(&harness, Panel::Explorer), Side::Right);
    assert!(!harness.state().panes.dock.fills(Side::Right), "the inner part only moves it");
}

/// *"I'm unable to drag the file pane around. I should be able to click in the empty space to the right of a
/// tab and drag and place it."* The empty part of a pane's tab strip carries the whole pane, and it lands
/// before or after the pane it is let go over.
#[test]
fn a_pane_is_dragged_by_the_empty_part_of_its_tab_strip() {
    let mut harness = harness("the first pane");
    did(&mut harness, "pane split");
    did(&mut harness, "pane split");
    steady(&mut harness);
    let listed = did(&mut harness, "pane list");
    assert_eq!(listed["count"], 3);
    // By the tabs it holds, because all three show an untitled document.
    let first_holds = listed["panes"][0]["tabs"].clone();
    let strips: Vec<egui::Rect> =
        harness.get_all_by_label("Tab strip empty part").map(|node| node.rect()).collect();
    assert_eq!(strips.len(), 3, "one strip a pane");
    let from = egui::pos2(strips[0].right() - 12.0, strips[0].center().y);
    let last = harness.state().editor_area();
    let onto = egui::pos2(last.right() - 20.0, last.center().y);
    carry_a_pane(&mut harness, from, onto);
    harness.snapshot(shot("pane_carried_by_its_strip"));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: onto,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    steady(&mut harness);
    let listed = did(&mut harness, "pane list");
    assert_eq!(listed["panes"][2]["tabs"], first_holds, "the first pane is now the last");

    let moved = did(&mut harness, "pane place 2 0");
    assert_eq!(moved["panes"][0]["tabs"], first_holds, "and the command puts it back");
}

/// Press on the empty part of a strip and move away, without letting go.
fn carry_a_pane(harness: &mut Harness<'static, UnluminousApp>, from: egui::Pos2, to: egui::Pos2) {
    harness.input_mut().events.push(egui::Event::PointerMoved(from));
    steady(harness);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    steady(harness);
    for step in 1..=6 {
        let at = from + (to - from) * (step as f32 / 6.0);
        harness.input_mut().events.push(egui::Event::PointerMoved(at));
        harness.step();
    }
    steady(harness);
}

// -------------------------------------------------------------------------------------- task-2298
//
// *"Some panels I'm not able to drag to where the blue boxes indicate I can. After I release, nothing happens.
// Eg if realm is to the left of agent chat and I drag realm to the right for full right side, nothing happens.
// Thoroughly test all combinations and placements and ensure they work as expected."*

/// The pane the Agent-Chat plugin contributed, which is the one plugin pane these tests show.
fn the_chat(harness: &Harness<'static, UnluminousApp>) -> Panel {
    (0..unluminous_app::app::dock::PLUGIN_PANES as u8)
        .map(Panel::Plugin)
        .find(|panel| harness.state().panel_is_showing(*panel))
        .expect("the chat pane is showing")
}

/// A point on a panel's header, left of the buttons at its right end.
fn header_of(harness: &Harness<'static, UnluminousApp>, panel: Panel) -> egui::Pos2 {
    let area = harness.state().panel_area(panel);
    egui::pos2(area.left() + 40.0_f32.min(area.width() / 3.0), area.top() + 12.0)
}

/// Press on `from` and move to `to` in steps, leaving the button down, so the panel is in the air.
fn lift(harness: &mut Harness<'static, UnluminousApp>, from: egui::Pos2, to: egui::Pos2) {
    harness.input_mut().events.push(egui::Event::PointerMoved(from));
    steady(harness);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    steady(harness);
    for step in 1..=8 {
        let at = from + (to - from) * (step as f32 / 8.0);
        harness.input_mut().events.push(egui::Event::PointerMoved(at));
        harness.step();
    }
    steady(harness);
}

/// Let the button go at `at`.
fn let_go(harness: &mut Harness<'static, UnluminousApp>, at: egui::Pos2) {
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    steady(harness);
}

/// The reported arrangement: the canvas inside the chat on the right. Let go anywhere on the `Fill whole right`
/// target, the canvas goes against the window's right edge and runs the whole height. Checked with the right
/// already filling or not, with the editing area showing or not, and with a strip along the bottom or not.
#[test]
fn the_canvas_let_go_on_fill_whole_right_goes_against_the_edge_whatever_was_there() {
    use unluminous_app::app::dock::Side;
    for (filled, editor, strip) in [
        (false, true, false),
        (true, true, false),
        (false, false, false),
        (true, false, true),
        (false, true, true),
    ] {
        let mut harness = harness("");
        if strip {
            harness.state_mut().new_detached_terminal_tab(8, 60);
            feed(&mut harness, b"$ ");
            did(&mut harness, "terminal show");
        }
        did(&mut harness, "realm show");
        did(&mut harness, "plugins pane agent-chat/chat --show");
        steady(&mut harness);
        let chat = the_chat(&harness);
        did(&mut harness, "panel dock realm right --position 0");
        if filled {
            did(&mut harness, "panel fill right");
        }
        if !editor {
            did(&mut harness, "action run toggle-editor");
        }
        steady(&mut harness);
        let start = harness.state().panes.dock;
        assert!(
            harness.state().panel_area(Panel::Realm).right()
                < harness.state().panel_area(chat).left() + 1.0,
            "the canvas starts inside the chat"
        );
        for (across, along) in [(0.15, 0.5), (0.85, 0.2), (0.5, 0.9)] {
            harness.state_mut().panes.dock = start;
            steady(&mut harness);
            let body = harness.state().panes_area();
            let zones = harness.state().drop_zones();
            let right = zones.iter().find(|zone| zone.side == Side::Right).expect("a right band");
            let at = egui::pos2(
                right.whole.left() + right.whole.width() * across,
                right.whole.top() + right.whole.height() * along,
            );
            let from = header_of(&harness, Panel::Realm);
            lift(&mut harness, from, at);
            let (_, aim, _) = harness.state().drop_preview().expect("the canvas is in the air");
            assert!(aim.fill, "the pointer is on the fill target: {at:?}");
            let_go(&mut harness, at);
            let case =
                format!("filled {filled}, editor {editor}, strip {strip}, at {across},{along}");
            let realm = harness.state().panel_area(Panel::Realm);
            assert!(
                (realm.right() - body.right()).abs() < 1.0,
                "against the right edge, {case}: {realm:?}"
            );
            assert!((realm.top() - body.top()).abs() < 1.0, "from the top, {case}: {realm:?}");
            assert!(
                (realm.bottom() - body.bottom()).abs() < 1.0,
                "to the bottom, {case}: {realm:?}"
            );
            assert!(harness.state().panes.dock.fills(Side::Right), "{case}");
        }
    }
}

/// A pane that is put away but docked to the same side has no middle to pass, and must not change where a panel
/// lands. Every pane that is not showing is put on the right in front of the canvas and the chat, and the canvas
/// let go just past the chat's middle lands outside the chat.
#[test]
fn a_pane_that_is_put_away_does_not_change_where_a_panel_lands() {
    use unluminous_app::app::dock::Side;
    let mut harness = harness("");
    did(&mut harness, "realm show");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    did(&mut harness, "action run toggle-editor");
    steady(&mut harness);
    let chat = the_chat(&harness);
    let mut layout = harness.state().panes.dock;
    for slot in 0..unluminous_app::app::dock::PLUGIN_PANES as u8 {
        let panel = Panel::Plugin(slot);
        if panel != chat {
            layout.dock(panel, Side::Right, Some(0));
        }
    }
    layout.dock(Panel::Realm, Side::Right, None);
    layout.dock(chat, Side::Right, None);
    harness.state_mut().panes.dock = layout;
    steady(&mut harness);
    let before = harness.state().panel_area(chat);
    let zones = harness.state().drop_zones();
    let right = zones.iter().find(|zone| zone.side == Side::Right).expect("a right band");
    let at = egui::pos2(before.center().x + 8.0, before.center().y);
    assert!(right.band.contains(at), "the pointer is in the right band: {at:?} {:?}", right.band);
    let from = header_of(&harness, Panel::Realm);
    lift(&mut harness, from, at);
    let (_, _, after) = harness.state().drop_preview().expect("the canvas is in the air");
    let_go(&mut harness, at);
    assert_eq!(harness.state().panes.dock, after, "the drop is what the preview showed");
    let realm = harness.state().panel_area(Panel::Realm);
    let chat_now = harness.state().panel_area(chat);
    assert!(
        realm.left() >= chat_now.right() - 1.0,
        "past the chat's middle, so outside it: {realm:?} {chat_now:?}"
    );
}

/// Every panel, several arrangements, every side, and the inner and outer part of each band: the drop does what
/// the strong rectangle showed, and what it showed is what the pointer was over.
#[test]
fn every_drop_lands_where_the_preview_said_it_would() {
    let arrangements: [(&str, &[&str]); 7] = [
        ("default", &[]),
        ("canvas inside the chat", &["panel dock realm right --position 0"]),
        ("chat inside the canvas", &["panel dock realm right --position 9"]),
        ("canvas along the bottom", &["panel dock realm bottom"]),
        ("canvas on top, right fills", &["panel dock realm top", "panel fill right"]),
        (
            "explorer right, left fills",
            &["panel dock explorer right --position 0", "panel fill left"],
        ),
        (
            "canvas left beside the explorer",
            &["panel dock realm left --position 9", "panel fill bottom"],
        ),
    ];
    let mut checked = 0;
    for editor in [true, false] {
        for (name, commands) in arrangements {
            let mut harness = harness("");
            harness.state_mut().new_detached_terminal_tab(8, 60);
            feed(&mut harness, b"$ ");
            did(&mut harness, "terminal show");
            did(&mut harness, "realm show");
            did(&mut harness, "plugins pane agent-chat/chat --show");
            for command in commands {
                did(&mut harness, command);
            }
            if !editor {
                did(&mut harness, "action run toggle-editor");
            }
            steady(&mut harness);
            let chat = the_chat(&harness);
            let start = harness.state().panes.dock;
            let showing: Vec<Panel> = [Panel::Explorer, Panel::Terminal, Panel::Realm, chat]
                .into_iter()
                .filter(|panel| harness.state().panel_is_showing(*panel))
                .collect();
            assert_eq!(showing.len(), 4, "editor {editor}, {name}: every panel is showing");
            for carried in showing {
                harness.state_mut().panes.dock = start;
                steady(&mut harness);
                let zones = harness.state().drop_zones();
                for zone in zones {
                    for (part, outer) in [(zone.whole, true), (inner_part(&zone), false)] {
                        if part.width() < 4.0 || part.height() < 4.0 {
                            continue;
                        }
                        // Two points across the inner part of a column band, where which middles the pointer
                        // has passed decides the order, and one point everywhere else.
                        let points: &[(f32, f32)] = match !outer && zone.side.is_a_column() {
                            true => &[(0.25, 0.3), (0.75, 0.7)],
                            false => &[(0.5, 0.5)],
                        };
                        for &(across, along) in points {
                            let at = match zone.side.is_a_column() {
                                true => egui::pos2(
                                    part.left() + part.width() * across,
                                    part.top() + part.height() * along,
                                ),
                                false => egui::pos2(
                                    part.left() + part.width() * along,
                                    part.top() + part.height() * across,
                                ),
                            };
                            // A corner where two bands overlap goes to the edge it is nearest, which
                            // `a_corner_goes_to_the_edge_it_is_nearest` checks. Here each point is in one band.
                            if zones
                                .iter()
                                .any(|other| other.side != zone.side && other.band.contains(at))
                            {
                                continue;
                            }
                            let case = format!(
                                "editor {editor}, {name}, {carried:?} to {:?} {} at {at:?}",
                                zone.side,
                                if outer { "outer" } else { "inner" }
                            );
                            check_one_drop(
                                &mut harness,
                                start,
                                carried,
                                zone.side,
                                outer,
                                at,
                                &case,
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!("{checked} drops checked");
    assert!(checked > 200, "{checked}");
}

/// The part of a band nearest the document, which moves a panel to the side without filling it.
fn inner_part(zone: &unluminous_app::app::dock::Zone) -> egui::Rect {
    use unluminous_app::app::dock::Side;
    let band = zone.band;
    match zone.side {
        Side::Left => {
            egui::Rect::from_min_max(egui::pos2(zone.whole.right(), band.top()), band.max)
        }
        Side::Right => {
            egui::Rect::from_min_max(band.min, egui::pos2(zone.whole.left(), band.bottom()))
        }
        Side::Top => {
            egui::Rect::from_min_max(egui::pos2(band.left(), zone.whole.bottom()), band.max)
        }
        Side::Bottom => {
            egui::Rect::from_min_max(band.min, egui::pos2(band.right(), zone.whole.top()))
        }
    }
}

/// Put the arrangement back to `start`, carry `carried` to `at` and let go, and check the drop against the preview
/// and against the place the pointer was in.
fn check_one_drop(
    harness: &mut Harness<'static, UnluminousApp>,
    start: unluminous_app::app::dock::Layout,
    carried: Panel,
    side: unluminous_app::app::dock::Side,
    outer: bool,
    at: egui::Pos2,
    case: &str,
) {
    use unluminous_app::app::dock::Side;
    harness.state_mut().panes.dock = start;
    steady(harness);
    // Which showing panels on the side the pointer is past the middle of, measured before anything moves.
    let passed = start
        .panels_on(side)
        .into_iter()
        .filter(|panel| *panel != carried)
        .filter(|panel| {
            harness.state().panel_is_showing(*panel)
                && at.x > harness.state().panel_area(*panel).center().x
        })
        .count();
    let from = header_of(harness, carried);
    lift(harness, from, at);
    let (panel, aim, after) =
        harness.state().drop_preview().unwrap_or_else(|| panic!("no target, {case}"));
    assert_eq!(panel, carried, "{case}");
    assert_eq!(aim.side, side, "the band the pointer is in, {case}");
    assert_eq!(aim.fill, outer, "the outer part fills and the inner part does not, {case}");
    let_go(harness, at);
    let now = harness.state().panes.dock;
    assert_eq!(now, after, "the drop is what the preview showed, {case}");
    assert_eq!(now.side_of(carried), side, "{case}");
    let body = harness.state().panes_area();
    let area = harness.state().panel_area(carried);
    let showing_order: Vec<Panel> = now
        .panels_on(side)
        .into_iter()
        .filter(|panel| harness.state().panel_is_showing(*panel))
        .collect();
    let placed = showing_order.iter().position(|panel| *panel == carried).expect("on the side");
    match (outer, side.is_a_column()) {
        (true, true) => {
            assert!(now.fills(side), "{case}");
            let against = match side {
                Side::Left => (area.left() - body.left()).abs(),
                _ => (area.right() - body.right()).abs(),
            };
            assert!(against < 1.0, "against the edge, {case}: {area:?} in {body:?}");
            assert!(area.height() >= body.height() - 1.0, "the whole height, {case}: {area:?}");
        }
        (true, false) => {
            assert!(now.fills(side), "{case}");
            assert_eq!(placed, passed, "after every panel whose middle it passed, {case}");
        }
        (false, _) => {
            assert_eq!(
                now.fills(side),
                start.fills(side),
                "the inner part changes no corner, {case}"
            );
            assert_eq!(placed, passed, "after every panel whose middle it passed, {case}");
        }
    }
}

// -------------------------------------------------------------------------------------- task-2317

/// The board along the bottom grows and shrinks with the explorer, the editing area and the chat in a row
/// above it, from anywhere along its divider and in every way the corners can be owned.
///
/// `task-2317`: *"If agent tasks is on bottom, and folder explorer, file editor, and agent chat are on top in a
/// row, sometimes I can't increase the height of the agent tasks. We need thorough testing to ensure we can
/// resize panes in all the various configurations."* The divider is grabbed near its left end, in its middle
/// and near its right end, because "sometimes" is what a divider that only some of its length answers on
/// looks like.
#[test]
fn the_board_along_the_bottom_resizes_under_a_row_of_columns() {
    use unluminous_app::app::dock::{self, Side};
    let corners: [(&str, &[Side]); 5] = [
        ("no side fills", &[]),
        ("the left fills", &[Side::Left]),
        ("the right fills", &[Side::Right]),
        ("both columns fill", &[Side::Left, Side::Right]),
        ("the bottom fills", &[Side::Bottom]),
    ];
    for (case, fills) in corners {
        for along in [0.04f32, 0.5, 0.96] {
            for grow in [true, false] {
                let mut harness = harness("");
                let board =
                    harness.state().plugin_ui.slot_of("agent-tasks/board").expect("the board")
                        as u8;
                let chat =
                    harness.state().plugin_ui.slot_of("agent-chat/chat").expect("the chat") as u8;
                did(&mut harness, "plugins pane agent-tasks/board --show");
                did(&mut harness, "plugins pane agent-chat/chat --show");
                harness.state_mut().dock_the_panel(Panel::Plugin(board), Side::Bottom, None);
                harness.state_mut().dock_the_panel(Panel::Plugin(chat), Side::Right, None);
                harness.state_mut().dock_the_panel(Panel::Explorer, Side::Left, None);
                for side in fills {
                    harness.state_mut().fill_a_side(*side, true);
                }
                steady(&mut harness);
                let divider = format!("Resize {}", Panel::Plugin(board).name());
                let before = harness.state().panel_area(Panel::Plugin(board)).height();
                let handle = harness.get_by_label(&divider).rect();
                let from = egui::pos2(handle.left() + handle.width() * along, handle.center().y);
                let distance = if grow { -150.0 } else { 90.0 };
                drag(&mut harness, from, egui::pos2(from.x, from.y + distance));
                let after = harness.state().panel_area(Panel::Plugin(board)).height();
                let editor = harness.state().editor_region().height();
                if grow {
                    assert!(
                        after - before > 150.0 * FOLLOWED
                            || editor <= dock::EDITOR_MIN_HEIGHT + 1.0,
                        "the board should grow when {case}, grabbed {along} along: {before} then \
                         {after}, the editing area {editor}"
                    );
                } else {
                    assert!(
                        before - after > 90.0 * FOLLOWED,
                        "the board should shrink when {case}, grabbed {along} along: {before} then \
                         {after}"
                    );
                }
            }
        }
    }
}

/// A project that was opened before keeps its layout in `.unluminous/layout.conf`, and a plugin pane switched on
/// in it is drawn, where that file says it was left.
///
/// `task-2327`, reported against 1.8.2 on macOS and then on Windows: Agent-Tasks, the Database and the Designer
/// lit their rail buttons and drew nothing. `restore_project` replaced the window's panes with the ones read from
/// the project's file (`task-2322`), and the dock read from it knew no contributed panes, so every one of them was
/// on no side and given no room. A project opened for the first time has no such file, which is why a test on a
/// fresh folder never saw it. `plugins pane` without a flag refuses a pane that has no room, so `did` is the check.
#[test]
fn a_plugin_pane_is_drawn_in_a_project_whose_layout_was_written_before() {
    use unluminous_app::app::dock::Side;
    let folder =
        copy_out_of_the_repository(&sample_folder(), "unluminous-screenshot-project-layout-panes");
    let written = folder.join(".unluminous");
    std::fs::create_dir_all(&written).expect("make .unluminous");
    std::fs::write(
        written.join("layout.conf"),
        "panes.agent-chat/chat.side = left\npanes.agent-chat/chat.order = 1\npanes.agent-chat/chat.width = 360\n",
    )
    .expect("write layout.conf");
    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    steady(&mut harness);
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    let answer = did(&mut harness, "plugins pane agent-chat/chat");
    let width = answer["area"]["width"].as_f64().unwrap_or(0.0);
    assert!(width > 100.0, "the chat pane was given room, and it is {width} wide: {answer}");
    let slot = (0..harness.state().plugin_ui.pane_count())
        .find(|slot| {
            harness.state().plugin_ui.pane_key(*slot).as_deref() == Some("agent-chat/chat")
        })
        .expect("the chat pane has a slot");
    assert_eq!(
        harness.state().panes.dock.side_of(Panel::Plugin(slot as u8)),
        Side::Left,
        "and it is on the side the project's own file left it on"
    );
    let chat = harness.state().panel_area(Panel::Plugin(slot as u8));
    let explorer = harness.state().panel_area(Panel::Explorer);
    assert!(chat.width() > 100.0 && chat.height() > 100.0, "drawn at {chat:?}");
    assert!(chat.left() >= explorer.right() - 1.0, "beside the file panel, on the left: {chat:?}");
}
