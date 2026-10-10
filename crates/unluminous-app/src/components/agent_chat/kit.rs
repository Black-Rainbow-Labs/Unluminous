//! Drawing the pane's own controls with `rux`, which is how the Agent-Chat design is carried into Rust.
//!
//! `task-2235`. The design (Claude Design canvas "Unluminous", page Agent-Chat) is built from `rux`'s
//! chat parts: round buttons, raised bubbles, carved wells, status wells and cards, all in the values of
//! `rux::theme::Chat`. Each piece the pane draws opens a small `rux` layer of its own over the rectangle
//! it fills, with room round it for its shadow, and the words that go on it are drawn afterwards with
//! `egui`, so they sit on top of the decoration.
//!
//! One `RuxState` serves the whole pane, kept in `PaneState::chrome_rux` and given back once a frame
//! by [`end_frame`].

use egui::Rect;

use crate::services::agent_chat::PaneState;
use crate::services::plugin_ui::Look;

/// How far a card's raised shadow reaches past it: `-7 -7 16, 8 8 18`.
pub const CARD_REACH: f32 = 32.0;
/// How far a bubble's or a round button's small raised shadow reaches: `-4 -4 9, 4 4 10`.
pub const SMALL_REACH: f32 = 18.0;

/// The pane's `rux` state, made the first time it is asked for and kept in step with the theme and
/// the pane's zoom.
pub fn state<'a>(
    kept: &'a mut Option<rux::RuxState>,
    look: &Look<'_>,
    still: bool,
) -> &'a rux::RuxState {
    let state = kept.get_or_insert_with(|| rux::RuxState::deterministic(crate::theme::rux_theme()));
    state.set_zoom(look.scale());
    state.set_still(still);
    crate::theme::in_step(state);
    state
}

/// Draw with `rux` into `rect`, in a layer that reaches `reach` points past it for the shadows.
///
/// `reach` is in unscaled points and is multiplied by the pane's zoom here.
pub fn layer<R>(
    pane: &mut PaneState,
    look: &Look<'_>,
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    rect: Rect,
    reach: f32,
    add: impl FnOnce(&mut rux::Rux<'_>) -> R,
) -> R {
    let still = pane.still;
    let state = state(&mut pane.chrome_rux, look, still);
    let id = ui.id().with(("agent-chat-rux", salt));
    rux::layer(ui, state, id, rect.expand(reach * look.scale()), add)
}

/// Give back the canvases of pieces that were not drawn this frame.
pub fn end_frame(pane: &PaneState) {
    if let Some(state) = &pane.chrome_rux {
        state.end_frame();
    }
}

/// The pane's chat values, in the active theme.
pub fn chat() -> rux::theme::Chat {
    crate::theme::rux_theme().chat
}
