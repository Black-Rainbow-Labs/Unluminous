//! Drawing the board with `rux`, which is how the Agent-Tasks design is carried into Rust.
//!
//! `task-2241`. The design (Claude Design canvas "Unluminous", page Agent-Tasks, artboard "Agent-Tasks board ·
//! Dark") is built from `rux`'s board parts in `rux::components::board`: the view switch, the sprint ring, the
//! search well, a lane and a task card, all in the values of `rux::theme::Chat`. The chat pane is drawn the same
//! way (`components::agent_chat::kit`).
//!
//! One `RuxState` serves the board, kept in `AgentTasks::board_rux`. It is taken off the board for the length
//! of a frame's drawing and put back afterwards, because the lanes read the board while they draw.

use egui::Rect;

use crate::services::agent_tasks::AgentTasks;
use crate::services::plugin_ui::Look;

/// How far a chip's small raised shadow reaches past it: `-2 -2 5, 3 3 6`, and the chosen view's.
pub const CHIP_REACH: f32 = 12.0;

/// The board's `rux` state for this frame, made the first time it is asked for and kept in step with the
/// theme and the pane's zoom.
pub fn take(board: &mut AgentTasks, look: &Look<'_>) -> rux::RuxState {
    let state = board
        .board_rux
        .take()
        .unwrap_or_else(|| crate::theme::rux_state(crate::theme::rux_theme()));
    state.set_zoom(look.scale());
    // Nothing on the board moves on its own, and a picture of it has to be the same on every run.
    state.set_still(true);
    crate::theme::in_step(&state);
    state
}

/// Put the state back on the board and give back the canvases of pieces not drawn this frame.
pub fn give_back(board: &mut AgentTasks, state: rux::RuxState) {
    state.end_frame();
    board.board_rux = Some(state);
}

/// Draw with `rux` into `rect`, in a layer that reaches `reach` points past it for the shadows.
///
/// `reach` is in the pane's own points, already multiplied by its zoom.
pub fn layer<R>(
    state: &rux::RuxState,
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    rect: Rect,
    reach: f32,
    add: impl FnOnce(&mut rux::Rux<'_>) -> R,
) -> R {
    let id = ui.id().with(("agent-tasks-rux", salt));
    rux::layer(ui, state, id, rect.expand(reach), add)
}
