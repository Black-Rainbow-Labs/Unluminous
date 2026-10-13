//! The bar along the top of the window.
//!
//! Unluminous draws its own, because the window has no operating system title bar: the rounded corners and the
//! translucent background need the decorations turned off.
//!
//! It holds four things, and where each of them goes depends on the platform, because the menus and the
//! window buttons swap sides.
//!
//! On macOS the menus belong in the bar along the top of the screen, so this bar holds no menus and the
//! three window buttons sit at the left, where macOS puts them. Everywhere else the menus are drawn here,
//! starting with `Unluminous` at the very left, and the window buttons move to the right hand end, where
//! Windows puts them. Either way `Unluminous` is the first thing in the top bar, which is what
//! `tasks/improvements.md` asks for.
//!
//! **The project's name comes after the menus**, which is what `task-1658` asks for — after `Git`, the
//! last of them, and on macOS after the window buttons and the Unluminous mark, where there are no menus here
//! to follow (`task-2258`).
//!
//! **The open file's name is not here at all.** It used to be centred, with its folder after it and an
//! amber dot for unsaved changes, and all three are now somewhere better: the file's name is on its tab,
//! the dot is on the tab as well, and the folder was the project, which is what the bar now says once
//! rather than repeating on every file.
//!
//! **The run widget sits at the right hand end**, in front of the window buttons, and the **text
//! tools** sit in front of *it* — `task-1693`. That order is the other way round from the one
//! `task-1683` shipped, and the reason is that only one of the two changes width: the tools are
//! nothing at all for a `.rs` file and five buttons for a `.md` one, so measuring the run widget
//! back from them slid the play button along the bar every time the tab changed. Whichever of the
//! two moves has to be the one whose width moves. The bar draws neither — `components::text_tools`
//! and
//! `components::run_widget` do — but it decides where both go, through [`tools_rect`] and
//! [`run_rect`], so that nothing it draws itself and nothing that is dragged to move the window
//! ever runs underneath one. **Neither ever changes the bar's height**, which is the whole reason
//! the tools were moved here in the first place.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use crate::app::actions::{Action, Menu};
use crate::components::controls::WithHint as _;
use crate::components::menu_bar;
use crate::theme::crisp::CrispPainter;
use crate::theme::{color, size};

/// How wide each of the three window buttons is on Windows: the whole height of the bar and 46 points
/// across, which is what Windows itself draws and what the Editing code design draws (`task-2241`).
const WINDOWS_BUTTON: f32 = 46.0;
/// The traffic lights on macOS: twelve points across, eight apart, after six points of room.
const LIGHT: f32 = 12.0;
const LIGHT_GAP: f32 = 8.0;
/// Between the window buttons and whatever is beside them.
const BUTTON_MARGIN: f32 = 10.0;
/// The square the mark is centred in on macOS, the size of the Windows menu's round pill.
const MARK_PILL: f32 = 30.0;

/// Where this window's menus are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPlacement {
    /// Drawn in the title bar, which is what Windows does.
    InWindow,
    /// In the bar along the top of the screen, which is what macOS does.
    Native,
}

impl MenuPlacement {
    /// What this platform does. macOS has a menu bar of its own; nothing else does.
    pub fn for_this_platform() -> Self {
        if cfg!(target_os = "macos") {
            MenuPlacement::Native
        } else {
            MenuPlacement::InWindow
        }
    }
}

/// What the user asked the window to do.
#[derive(Debug, PartialEq)]
pub struct TitleBarOutcome {
    pub close: bool,
    pub minimise: bool,
    /// Fill the desktop the window is already on. A double click on the bar, and the maximise button
    /// everywhere except macOS.
    pub toggle_maximise: bool,
    /// Go full screen, which on macOS means a space of its own. The green light there, because that is
    /// what the green light does on that platform; a double click still only maximises.
    pub toggle_fullscreen: bool,
    /// Something chosen from a menu in the bar.
    pub action: Option<Action>,
    /// Where the branch widget goes: after the project's chip, which is the only control here whose
    /// position depends on how long another control's text turned out to be.
    ///
    /// The bar reports it rather than drawing it, for the reason it reports `tools_rect` and `run_rect`
    /// rather than drawing those: a control added to the `Ui` before the bar's own drag area sits
    /// underneath it and is never pressed, so the window draws all three over the top afterwards.
    ///
    /// `Rect::NOTHING` when there is no branch widget.
    pub branch_rect: Rect,
    /// True on the frame the bar asked the operating system to move the window, so the window can
    /// check a moment later that `winit` was not left stuck. See `services::windows_resize`.
    pub dragged: bool,
}

impl Default for TitleBarOutcome {
    fn default() -> Self {
        Self {
            close: false,
            minimise: false,
            toggle_maximise: false,
            toggle_fullscreen: false,
            action: None,
            branch_rect: Rect::NOTHING,
            dragged: false,
        }
    }
}

/// The three window buttons, left to right, with the rectangle each one takes.
///
/// On Windows each is a strip the whole height of the bar at its right hand end, in the order Windows puts
/// them; on macOS they are the three lights at its left, in the order macOS puts them.
fn window_buttons(area: Rect, placement: MenuPlacement) -> [(Rect, &'static str); 3] {
    match placement {
        MenuPlacement::InWindow => {
            let strip = |index: f32| {
                Rect::from_min_max(
                    Pos2::new(area.right() - WINDOWS_BUTTON * (3.0 - index), area.top()),
                    Pos2::new(area.right() - WINDOWS_BUTTON * (2.0 - index), area.bottom()),
                )
            };
            [(strip(0.0), "Minimise"), (strip(1.0), "Maximise"), (strip(2.0), "Close")]
        }
        MenuPlacement::Native => {
            let light = |index: f32| {
                Rect::from_center_size(
                    Pos2::new(
                        area.left() + 18.0 + LIGHT / 2.0 + index * (LIGHT + LIGHT_GAP),
                        area.center().y,
                    ),
                    Vec2::splat(LIGHT + 4.0),
                )
            };
            [(light(0.0), "Close"), (light(1.0), "Minimise"), (light(2.0), "Maximise")]
        }
    }
}

/// The rectangle the text tools are drawn into, `width` points wide.
///
/// They finish in front of the run widget, which is at the right hand end of the bar. A width of
/// nothing gives an empty rectangle there, which is what a file with no tools gets.
pub fn tools_rect(area: Rect, placement: MenuPlacement, width: f32, run_width: f32) -> Rect {
    // In front of the run widget, which takes the right hand end. `task-1693`: the tools are as wide
    // as the open file needs — nothing at all for a `.rs` file, five buttons for a `.md` one — so
    // measuring the run widget back from *them* slid it along the bar every time the tab changed.
    // Whichever of the two moves has to be the one whose width moves, and that is the tools.
    let run = run_rect(area, placement, run_width);
    let right = if run_width > 0.0 { run.left() - 6.0 } else { run.right() };
    Rect::from_min_max(Pos2::new(right - width, area.top()), Pos2::new(right, area.bottom()))
}

/// The rectangle the run widget is drawn into, `run_width` points wide.
///
/// The right hand end of the bar, clear of the window buttons, so the bar reads project, tools, run,
/// buttons. The play and the bug are then in the same place whatever file is open — `task-1693`.
pub fn run_rect(area: Rect, placement: MenuPlacement, run_width: f32) -> Rect {
    let right = match placement {
        MenuPlacement::InWindow => area.right() - WINDOWS_BUTTON * 3.0 - BUTTON_MARGIN,
        MenuPlacement::Native => area.right() - BUTTON_MARGIN - 8.0,
    };
    Rect::from_min_max(Pos2::new(right - run_width, area.top()), Pos2::new(right, area.bottom()))
}

/// Draw the title bar into `area`.
///
/// `project` is the name of the folder that is open; `tools_width` and `run_width` are how much
/// room to leave clear at the right for the text tools and the run widget, which the window draws
/// over the top afterwards.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    area: Rect,
    project: Option<&str>,
    opacity: f32,
    placement: MenuPlacement,
    menus: &[Menu],
    tools_width: f32,
    run_width: f32,
    branch_width: f32,
    logo_motion: crate::components::logo::Motion,
) -> TitleBarOutcome {
    let mut outcome = TitleBarOutcome::default();
    let painter = ui.painter_at(area);
    // The top two corners are rounded to match the window; the bottom two are square because the tabs
    // sit directly underneath.
    painter.rect_filled(
        area,
        CornerRadius { nw: size::WINDOW_CORNER, ne: size::WINDOW_CORNER, sw: 0, se: 0 },
        crate::theme::faded(color::title_bar(), opacity),
    );
    // The groove under the bar: a dark line and a lit one under it.
    crate::components::chrome_kit::groove(
        &painter,
        Pos2::new(area.left(), area.bottom() - 2.0),
        Pos2::new(area.right(), area.bottom() - 2.0),
    );

    let buttons = window_buttons(area, placement);
    match placement {
        MenuPlacement::InWindow => windows_buttons(ui, &buttons, &mut outcome),
        MenuPlacement::Native => traffic_lights(ui, &buttons, &mut outcome),
    }

    // The menus, when they are drawn here, then the project's chip after them.
    let (mut name_from, drag_to) = match placement {
        MenuPlacement::InWindow => {
            let (action, right) = menu_bar::show(ui, area, menus, logo_motion);
            outcome.action = action;
            (right + 10.0, buttons[0].0.left() - BUTTON_MARGIN)
        }
        MenuPlacement::Native => {
            let right = mac_mark(ui, area, buttons[2].0.right() + 10.0, logo_motion);
            (right + 8.0, area.right())
        }
    };

    // Whichever of the run widget and the tools is furthest left is where the chip has to stop.
    let tools = tools_rect(area, placement, tools_width, run_width);
    let run = run_rect(area, placement, run_width);
    let claimed = if tools_width > 0.0 {
        Some(tools.left())
    } else if run_width > 0.0 {
        Some(run.left())
    } else {
        None
    };
    let name_to = match claimed {
        Some(left) => left - 12.0,
        None => drag_to,
    };
    if let Some(project) = project {
        if let Some(chip) = project_chip(ui, area, project, name_from, name_to) {
            name_from = chip.right();
        }
    }

    // The branch widget, after the project's chip.
    outcome.branch_rect = Rect::from_min_max(
        Pos2::new(name_from + 8.0, area.top()),
        Pos2::new((name_from + 8.0 + branch_width).min(name_to), area.bottom()),
    );
    if branch_width > 0.0 && outcome.branch_rect.width() > 0.0 {
        // The drag area stops before it, so pressing the branch button never begins a drag of the
        // window behind it.
        name_from = outcome.branch_rect.right();
    }

    // Dragging anywhere else on the bar moves the window, which is what a title bar is for. It stops
    // short of whichever control reaches furthest left, so pressing the text options or the play button
    // never begins a drag of the window behind it.
    let drag_to = drag_to.min(claimed.map(|left| left - 4.0).unwrap_or(drag_to));
    let drag_from = name_from + 8.0;
    if drag_to > drag_from {
        let drag_area =
            Rect::from_min_max(Pos2::new(drag_from, area.top()), Pos2::new(drag_to, area.bottom()));
        // Clicks and drags, and no keyboard focus, for the reason given on the window buttons: a double
        // click here resizes the window, and `Enter` on a focused widget counts as a click.
        let drag = ui.interact(drag_area, ui.id().with("title-drag"), Sense::CLICK | Sense::DRAG);
        // Named, because every control in Unluminous has a plain name. `StartDrag` hands the drag to the
        // operating system's own modal loop, so no synthetic event can drive it; a test asks that the
        // control is there and is the size it should be.
        drag.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Move window")
        });
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            outcome.dragged = true;
        }
        if drag.double_clicked() {
            outcome.toggle_maximise = true;
        }
        // **A modal takes every press under it, the title bar's included**, so with Settings open the
        // window could not be moved at all (`task-2247`). The press is read from the frame's input
        // instead, and only where the dialog itself is not: a modal near the top of a small window can
        // reach up over the bar, and a press on the dialog is the dialog's.
        if !drag.drag_started() && pressed_past_a_modal(ui.ctx(), drag_area) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            outcome.dragged = true;
        }
    }

    outcome
}

/// The Unluminous mark at the left of the bar on macOS, just after the traffic lights (`task-2258`).
///
/// On Windows the mark is the application's own menu, in `menu_bar`. macOS keeps that menu in the bar along the
/// top of the screen, so here the mark is the mark alone: it turns while the pointer is over it and does nothing
/// when pressed. It is the same size as the Windows one, and it answers where it ended.
fn mac_mark(
    ui: &mut egui::Ui,
    area: Rect,
    left: f32,
    motion: crate::components::logo::Motion,
) -> f32 {
    let rect = Rect::from_min_size(
        Pos2::new(left, area.center().y - MARK_PILL / 2.0),
        Vec2::splat(MARK_PILL),
    );
    let response = ui.interact(rect, ui.id().with("unluminous-mark"), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Unluminous"));
    crate::components::logo::paint(ui, rect, menu_bar::MARK, motion, response.hovered());
    rect.right()
}

/// Whether the primary button went down inside `area` on this frame while a modal is open, somewhere the
/// modal's own dialog is not.
fn pressed_past_a_modal(ctx: &egui::Context, area: Rect) -> bool {
    let Some(modal) = crate::components::modal::the_open_one(ctx) else {
        return false;
    };
    let Some(at) = ctx.input(|input| {
        input.pointer.primary_pressed().then(|| input.pointer.press_origin()).flatten()
    }) else {
        return false;
    };
    // The dialog's own rectangle, not the layer under the pointer: egui's `Modal` covers the whole window
    // with its backdrop in the modal's own layer, so asking which layer is under the pointer answered "the
    // modal" everywhere and the window could not be moved at all (`task-2255`).
    let over_the_dialog =
        crate::components::modal::drawn_by_id(ctx, modal).is_some_and(|dialog| dialog.contains(at));
    area.contains(at) && !over_the_dialog
}

/// The project's name in a carved pill with a folder before it, answering the rectangle it took.
///
/// `height: 30px; padding: 0 14px 0 11px; gap: 7px`, the folder at 14 points in the quiet ink and the name
/// in 12 point words. Cut short with an ellipsis rather than run underneath the controls on the right.
fn project_chip(ui: &mut egui::Ui, area: Rect, project: &str, from: f32, to: f32) -> Option<Rect> {
    let room = to - from;
    if room < 60.0 {
        return None;
    }
    let font = egui::FontId::proportional(12.0);
    let chat = crate::components::chrome_kit::chat();
    let ink = crate::theme::rux_theme().ink;
    let (shown, _) =
        crate::components::controls::elide(project, room - 11.0 - 14.0 - 7.0 - 14.0, |words| {
            ui.painter().layout_no_wrap(words.to_owned(), font.clone(), ink.i700).size().x
        });
    let galley = ui.painter().crisp_layout_no_wrap(shown, font.clone(), ink.i700);
    let width = 11.0 + 14.0 + 7.0 + galley.size().x + 14.0;
    let chip = Rect::from_min_size(Pos2::new(from, area.center().y - 15.0), Vec2::new(width, 30.0));
    crate::components::chrome_kit::layer(ui, "project-chip", chip, 0.0, |rux| {
        rux.chrome.surface(chip, 15.0, chat.well, chat.carved_sm());
        rux.mark(
            rux::Mark::new(rux::Icon::Project, 14.0).stroke(1.8),
            Pos2::new(chip.left() + 11.0 + 7.0, chip.center().y),
            ink.i400,
        );
    });
    let top = crate::theme::crisp::top_centring_capitals(ui.painter(), &font, chip.center().y);
    ui.painter().crisp_galley(Pos2::new(chip.left() + 11.0 + 14.0 + 7.0, top), galley, ink.i700);
    Some(chip)
}

/// The three window buttons Windows draws: flat strips the height of the bar with a thin mark in each, a wash
/// under the pointer and the close button's red under the pointer on the last.
fn windows_buttons(
    ui: &mut egui::Ui,
    buttons: &[(Rect, &'static str); 3],
    outcome: &mut TitleBarOutcome,
) {
    let ink = crate::theme::rux_theme().ink;
    for (rect, label) in buttons {
        // `Sense::CLICK` and not `Sense::click()`: the same clicks, but the button cannot take keyboard
        // focus. A focused button is pressed by `Space` and by `Enter`, and these three close, minimise
        // and resize the window. See `app::hold_the_keyboard`.
        let response = ui
            .interact(*rect, ui.id().with(("window-button", *label)), Sense::CLICK)
            .with_hint(*label);
        let painter = ui.painter();
        let close = *label == "Close";
        let tint = match (response.hovered(), close) {
            (true, true) => {
                // The window's own top right corner is round, so the red is too.
                painter.rect_filled(
                    *rect,
                    CornerRadius { ne: size::WINDOW_CORNER, ..CornerRadius::ZERO },
                    color::close(),
                );
                color::on_accent()
            }
            (true, false) => {
                painter.rect_filled(*rect, 0.0, color::hover_wash().gamma_multiply(0.08));
                ink.i900
            }
            _ => ink.i500,
        };
        let centre = rect.center();
        let stroke = Stroke::new(1.0, tint);
        match *label {
            "Minimise" => {
                painter.line_segment(
                    [centre + Vec2::new(-4.5, 0.0), centre + Vec2::new(4.5, 0.0)],
                    stroke,
                );
            }
            "Maximise" => {
                painter.rect_stroke(
                    Rect::from_center_size(centre, Vec2::splat(9.0)),
                    1.5,
                    stroke,
                    egui::StrokeKind::Middle,
                );
            }
            _ => {
                painter.line_segment(
                    [centre + Vec2::new(-4.2, -4.2), centre + Vec2::new(4.2, 4.2)],
                    stroke,
                );
                painter.line_segment(
                    [centre + Vec2::new(4.2, -4.2), centre + Vec2::new(-4.2, 4.2)],
                    stroke,
                );
            }
        }
        // The accessible name is the plain word, so a test can ask for the Close button by name.
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), *label)
        });
        if response.clicked() {
            match *label {
                "Close" => outcome.close = true,
                "Minimise" => outcome.minimise = true,
                _ => outcome.toggle_maximise = true,
            }
        }
    }
}

/// The three lights macOS draws at the left of a window: red, amber and green, always in colour, with the
/// mark each one stands for appearing under the pointer.
fn traffic_lights(
    ui: &mut egui::Ui,
    buttons: &[(Rect, &'static str); 3],
    outcome: &mut TitleBarOutcome,
) {
    let group = buttons[0].0.union(buttons[2].0).expand(4.0);
    let lit = ui.rect_contains_pointer(group);
    for (rect, label) in buttons {
        let response = ui
            .interact(*rect, ui.id().with(("window-button", *label)), Sense::CLICK)
            .with_hint(*label);
        let fill = match *label {
            "Close" => color::close(),
            "Minimise" => color::minimise(),
            _ => color::maximise(),
        };
        let painter = ui.painter();
        let centre = rect.center();
        painter.circle_filled(centre, LIGHT / 2.0, fill);
        // any ground: the hairline round a light and the mark in it are drawn on the light itself
        painter.circle_stroke(centre, LIGHT / 2.0, Stroke::new(0.5, Color32::from_black_alpha(64)));
        if lit {
            // any ground: the mark is drawn on the light, which is the same colour in every theme
            let stroke = Stroke::new(1.1, Color32::from_black_alpha(150));
            match *label {
                "Close" => {
                    painter.line_segment(
                        [centre + Vec2::new(-2.6, -2.6), centre + Vec2::new(2.6, 2.6)],
                        stroke,
                    );
                    painter.line_segment(
                        [centre + Vec2::new(2.6, -2.6), centre + Vec2::new(-2.6, 2.6)],
                        stroke,
                    );
                }
                "Minimise" => {
                    painter.line_segment(
                        [centre + Vec2::new(-3.0, 0.0), centre + Vec2::new(3.0, 0.0)],
                        stroke,
                    );
                }
                _ => {
                    painter.line_segment(
                        [centre + Vec2::new(-3.0, 0.0), centre + Vec2::new(3.0, 0.0)],
                        stroke,
                    );
                    painter.line_segment(
                        [centre + Vec2::new(0.0, -3.0), centre + Vec2::new(0.0, 3.0)],
                        stroke,
                    );
                }
            }
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), *label)
        });
        if response.clicked() {
            match *label {
                "Close" => outcome.close = true,
                "Minimise" => outcome.minimise = true,
                // The green light takes the window full screen on macOS, where full screen is a space of
                // its own.
                _ => outcome.toggle_fullscreen = true,
            }
        }
    }
}

/// The line under a bar and above the status bar, and the line between the explorer and the editor.
pub fn divider(painter: &egui::Painter, from: Pos2, to: Pos2) {
    painter.line_segment([from, to], Stroke::new(1.0, color::divider()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1180.0, size::TITLE_BAR))
    }

    #[test]
    fn the_tools_stop_before_the_run_widget_and_the_run_widget_before_the_buttons() {
        let area = bar();
        let tools = tools_rect(area, MenuPlacement::InWindow, 124.0, 120.0);
        let run = run_rect(area, MenuPlacement::InWindow, 120.0);
        let nearest_button = window_buttons(area, MenuPlacement::InWindow)[0].0.left();
        assert!(tools.right() < run.left(), "the tools finish before the run widget starts");
        assert!(
            run.right() < nearest_button,
            "the run widget finishes at {} and the first button starts at {nearest_button}",
            run.right()
        );
        assert_eq!(tools.width(), 124.0);
        assert_eq!(run.width(), 120.0);
    }

    #[test]
    fn the_windows_buttons_are_three_strips_at_the_right_hand_end_the_height_of_the_bar() {
        let area = bar();
        let buttons = window_buttons(area, MenuPlacement::InWindow);
        assert_eq!(buttons.map(|(_, name)| name), ["Minimise", "Maximise", "Close"]);
        assert_eq!(buttons[2].0.right(), area.right());
        for (rect, _) in buttons {
            assert_eq!(rect.width(), WINDOWS_BUTTON);
            assert_eq!(rect.height(), area.height());
        }
    }

    #[test]
    fn the_run_widget_goes_against_the_right_edge_when_the_buttons_are_at_the_left() {
        let area = bar();
        let run = run_rect(area, MenuPlacement::Native, 120.0);
        assert!(run.right() > area.right() - 30.0, "near the right hand edge");
        let lights = window_buttons(area, MenuPlacement::Native);
        assert_eq!(lights.map(|(_, name)| name), ["Close", "Minimise", "Maximise"]);
        assert!(run.left() > lights[2].0.right(), "clear of the lights");
    }

    #[test]
    fn a_file_with_no_tools_leaves_an_empty_rectangle_rather_than_a_gap() {
        let tools = tools_rect(bar(), MenuPlacement::InWindow, 0.0, 120.0);
        assert_eq!(tools.width(), 0.0);
    }

    /// `task-1693`: the run widget is where it is whatever the open file is. The tools are the half that
    /// changes width, so they are the half that moves.
    #[test]
    fn the_run_widget_does_not_move_when_the_open_file_changes() {
        let area = bar();
        let with_tools = run_rect(area, MenuPlacement::InWindow, 120.0);
        let longer = run_rect(area, MenuPlacement::InWindow, 200.0);
        assert_eq!(longer.right(), with_tools.right());
    }

    #[test]
    fn with_no_run_widget_the_tools_take_the_room_it_would_have_had() {
        let area = bar();
        let tools = tools_rect(area, MenuPlacement::InWindow, 124.0, 0.0);
        assert_eq!(tools.right(), run_rect(area, MenuPlacement::InWindow, 0.0).right());
    }
}
