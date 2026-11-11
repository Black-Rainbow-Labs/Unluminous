//! Termoil's shaders and cursor effects over the whole window (`task-2612`).
//!
//! The terminals' pass (`terminal_shader`) already runs Termoil's shaders in the vendored egui-wgpu's
//! `AfterTheFrameHook`, one rectangle a grid. The window's shader is one more rectangle there: the whole window,
//! recorded once a frame by [`shade_the_window`] at the end of `UnluminousApp::ui`. Its `uv` runs from one corner
//! of the window to the other, so a curved tube or a vignette belongs to the window rather than to each pane.
//!
//! Three settings decide it. `appearance.shader` is the shader, `appearance.cursor_effect` is a cursor effect
//! drawn around the caret that has the keyboard, and `appearance.terminals_keep_their_shader`, off by default,
//! leaves a terminal that has a look of its own to that look. While it is off, `appearance.shader` is drawn over the
//! terminals and their own shader and cursor effect are not drawn ([`covers_the_terminals`]).
//!
//! A shader that moves asks for a frame every 33 ms only while the window has the focus, the rule Ghostty's
//! `custom-shader-animation` keeps by default, so a window in the background costs what it cost before.

use std::cell::{Cell, RefCell};

use crate::services::terminal_cursor_effect::{self, Effect};
use crate::services::terminal_shader::{self, Builtin, Look};

/// How often a window shader that moves asks for a frame while the window has the focus: 30 frames a second.
const MOVING_FRAME: std::time::Duration = std::time::Duration::from_millis(33);

thread_local! {
    /// The window's shader, from `appearance.shader`. Empty is none.
    static SHADER: RefCell<String> = const { RefCell::new(String::new()) };
    /// The window's cursor effect, from `appearance.cursor_effect`. Empty is none.
    static EFFECT: RefCell<String> = const { RefCell::new(String::new()) };
    /// `appearance.terminals_keep_their_shader`.
    static TERMINALS_KEEP: Cell<bool> = const { Cell::new(false) };
    /// The caret that has the keyboard, in window points, and the pass it was recorded in.
    static CARET: Cell<Option<(egui::Pos2, u64)>> = const { Cell::new(None) };
    /// What [`shade_the_window`] last recorded, for a test to read.
    static LAST_SHADED: RefCell<Option<Shaded>> = const { RefCell::new(None) };
}

/// What the window pass was last asked to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct Shaded {
    /// The window's rectangle, in points.
    pub rect: egui::Rect,
    /// The shader's name, or empty for none.
    pub shader: String,
    /// The cursor effect's name, or empty for none.
    pub cursor_effect: String,
    /// Where the cursor effect was centered, in points.
    pub caret: Option<egui::Pos2>,
}

/// Choose what the window is drawn with, by name. An empty name, `none`, or a name that is not one turns that part
/// off.
///
/// @param shader - a Termoil shader's name
/// @param effect - a Termoil cursor effect's name
/// @param terminals_keep - whether a terminal with a look of its own keeps it
pub fn choose(shader: &str, effect: &str, terminals_keep: bool) {
    let shader = terminal_shader::named(shader).map(|found| found.name).unwrap_or_default();
    let effect = terminal_cursor_effect::named(effect).map(|found| found.name).unwrap_or_default();
    SHADER.with(|chosen| *chosen.borrow_mut() = shader.to_owned());
    EFFECT.with(|chosen| *chosen.borrow_mut() = effect.to_owned());
    TERMINALS_KEEP.with(|keep| keep.set(terminals_keep));
}

/// The shader the window is drawn with, when there is one.
pub fn shader_in_use() -> Option<&'static Builtin> {
    SHADER.with(|chosen| terminal_shader::named(&chosen.borrow()))
}

/// The cursor effect drawn around the window's caret, when there is one.
pub fn effect_in_use() -> Option<&'static Effect> {
    EFFECT.with(|chosen| terminal_cursor_effect::named(&chosen.borrow()))
}

/// True while the window's shader is drawn over the terminals in place of their own look: a window shader is chosen
/// and `appearance.terminals_keep_their_shader` is off. A window cursor effect alone never hides a terminal's look.
pub fn covers_the_terminals() -> bool {
    shader_in_use().is_some() && !TERMINALS_KEEP.with(Cell::get)
}

/// Record where the caret that has the keyboard is this frame, so the window's cursor effect is drawn around it.
/// The editor calls it where it paints its caret, and a terminal grid with the keyboard where it knows its cursor.
///
/// @param ctx - the window's context
/// @param center - the caret's center, in window points
pub fn note_the_caret(ctx: &egui::Context, center: egui::Pos2) {
    CARET.with(|caret| caret.set(Some((center, ctx.cumulative_pass_nr()))));
}

/// The caret with the keyboard this frame: the one noted this pass, or else the caret of the egui text field that
/// has the keyboard, which egui gives the platform for its input method.
///
/// @param ctx - the window's context
fn the_caret(ctx: &egui::Context) -> Option<egui::Pos2> {
    let pass = ctx.cumulative_pass_nr();
    let noted = CARET.with(Cell::get).filter(|(_, at)| *at == pass).map(|(center, _)| center);
    noted.or_else(|| ctx.output(|output| output.ime.map(|ime| ime.cursor_rect.center())))
}

/// Draw the window's shader and cursor effect over the whole window, once everything else has been drawn. Nothing
/// while neither is chosen. One that moves asks for the next frame while the window has the focus.
///
/// @param ctx - the window's context
pub fn shade_the_window(ctx: &egui::Context) {
    let (shader, effect) = (shader_in_use(), effect_in_use());
    if shader.is_none() && effect.is_none() {
        return;
    }
    let look = Look { shader, effect };
    let rect = ctx.content_rect();
    let caret = effect.and_then(|_| the_caret(ctx));
    let painter = ctx.layer_painter(egui::LayerId::background());
    terminal_shader::register(&painter, rect, look, caret, Vec::new(), true);
    LAST_SHADED.with(|last| {
        *last.borrow_mut() = Some(Shaded {
            rect,
            shader: shader.map(|found| found.name.to_owned()).unwrap_or_default(),
            cursor_effect: effect.map(|found| found.name.to_owned()).unwrap_or_default(),
            caret,
        })
    });
    let focused = ctx.input(|input| input.viewport().focused) != Some(false);
    if look.moves() && focused {
        ctx.request_repaint_after(MOVING_FRAME);
    }
}

/// What the window pass was last asked to draw on this thread, and forget it, so a test can ask about the next frame.
pub fn take_the_last_shaded() -> Option<Shaded> {
    LAST_SHADED.with(|last| last.borrow_mut().take())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table in `tasks/task-2612-window-shaders-tdd.md`: a window shader covers the terminals unless they keep
    /// their own, and a window cursor effect alone never does.
    #[test]
    fn a_window_shader_covers_the_terminals_unless_they_keep_their_own() {
        choose("", "", false);
        assert!(!covers_the_terminals(), "nothing chosen");
        choose("crt", "", false);
        assert_eq!(shader_in_use().map(|found| found.name), Some("crt"));
        assert!(covers_the_terminals());
        choose("crt", "", true);
        assert!(!covers_the_terminals(), "the terminals keep their own");
        choose("", "sonar", false);
        assert!(!covers_the_terminals(), "a cursor effect alone");
        assert_eq!(effect_in_use().map(|found| found.name), Some("sonar"));
        choose("not-a-shader", "none", false);
        assert!(shader_in_use().is_none() && effect_in_use().is_none());
    }
}
