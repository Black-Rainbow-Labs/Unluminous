//! Where the pointer really is while a shader bends the picture (`task-2622`).
//!
//! `crt` and `retro` bend the frame like the glass of a tube: the pixel at `uv` shows the frame at
//! [`curve`]`(uv)`. Input is not bent by the GPU, so a click used to land on whatever was under the pointer in the
//! unbent frame, several points away from what the person saw under it near the window's edge.
//!
//! Every pass that draws a bending shader records a [`Region`] while the frame is drawn ([`note_a_grid`], [`note_the_window`]). Before the next
//! pass, `UnluminousApp::raw_input_hook` hands the window's events to [`bend_the_pointer`], which moves every
//! pointer position inside a region to the place in the frame that is drawn under it. A grid's own region is asked
//! before the window's, because a grid drawn with its own look is not bent by the window's pass.
//!
//! The arithmetic is the WGSL's in `terminal_shaders/crt.wgsl` and `retro.wgsl`, with the curve scaled by the
//! shader's strength, so the two cannot disagree as long as [`curve_of`] says what those files say.

use std::cell::RefCell;

use crate::services::terminal_shader::Builtin;

/// How much a shader bends the picture at full strength: the `CURVE` constant in its file. `None` for a shader that
/// does not bend it.
///
/// @param shader - the shader
pub fn curve_of(shader: &Builtin) -> Option<f32> {
    match shader.name {
        "crt" => Some(0.0225),
        "retro" => Some(0.03),
        _ => None,
    }
}

/// Where the frame is read for the pixel at `uv`, both from 0 to 1 across the shaded rectangle.
///
/// @param uv - the pixel's place in the rectangle
/// @param amount - the curve times the strength
pub fn curve(uv: egui::Vec2, amount: f32) -> egui::Vec2 {
    let c = uv * 2.0 - egui::Vec2::splat(1.0);
    let bent = c * (1.0 + amount * c.dot(c));
    bent * 0.5 + egui::Vec2::splat(0.5)
}

/// The pixel that shows the frame at `read`: the inverse of [`curve`], found with Newton's method. Four steps
/// are more than enough, because the curve is a few percent away from the identity.
///
/// @param read - a place in the frame, from 0 to 1 across the shaded rectangle
/// @param amount - the curve times the strength
pub fn uncurve(read: egui::Vec2, amount: f32) -> egui::Vec2 {
    // In the centered coordinates the curve is c * (1 + k|c|^2), which keeps the direction of c, so only the
    // length has to be solved: r * (1 + k r^2) = target.
    let target = read * 2.0 - egui::Vec2::splat(1.0);
    let length = target.length();
    if length <= f32::EPSILON || amount == 0.0 {
        return read;
    }
    let mut r = length;
    for _ in 0..4 {
        let value = r * (1.0 + amount * r * r) - length;
        let slope = 1.0 + 3.0 * amount * r * r;
        r -= value / slope;
    }
    (target * (r / length)) * 0.5 + egui::Vec2::splat(0.5)
}

/// One rectangle a bending shader was drawn over in the last frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// The shaded rectangle, in window points.
    pub rect: egui::Rect,
    /// The curve times the strength.
    pub amount: f32,
    /// The parts of the rectangle the pass was kept out of, in window points: for the window's pass, the grids drawn
    /// in a look of their own.
    pub holes: Vec<egui::Rect>,
    /// True for the pass over the whole window, which is asked after every grid.
    pub whole: bool,
}

impl Region {
    /// Where the frame is read under `point`, when the point is inside this region and outside its holes.
    ///
    /// @param point - a pointer position, in window points
    pub fn bend(&self, point: egui::Pos2) -> Option<egui::Pos2> {
        if !self.rect.contains(point) || self.holes.iter().any(|hole| hole.contains(point)) {
            return None;
        }
        let size = self.rect.size();
        if size.x <= 0.0 || size.y <= 0.0 {
            return None;
        }
        let uv = (point - self.rect.min) / size;
        let read = curve(uv, self.amount);
        Some(self.rect.min + read * size)
    }
}

thread_local! {
    /// The regions recorded while the frame being drawn now is drawn.
    static DRAWING: RefCell<Vec<Region>> = const { RefCell::new(Vec::new()) };
    /// Every grid drawn in a look of its own this frame, bending or not, which the window's pass leaves alone.
    static GRIDS: RefCell<Vec<egui::Rect>> = const { RefCell::new(Vec::new()) };
    /// The regions of the last frame drawn, which the next frame's input is bent by.
    static DRAWN: RefCell<Vec<Region>> = const { RefCell::new(Vec::new()) };
}

/// How much a shader at a strength bends the picture, or `None` when it does not bend it at all.
///
/// @param shader - the shader, when there is one
/// @param strength - how much of it is drawn, 0 to 1
fn amount_of(shader: Option<&Builtin>, strength: f32) -> Option<f32> {
    shader.and_then(curve_of).map(|curve| curve * strength).filter(|amount| *amount > 0.0)
}

/// Record that a grid is drawn in a look of its own this frame, so the window's pass leaves it alone, and, when
/// its shader bends the picture, that the pointer is bent over it.
///
/// @param rect - the grid, in window points
/// @param shader - the grid's shader, when it has one
/// @param strength - how much of it is drawn, 0 to 1
/// @param holes - what is drawn over the grid, in window points
pub fn note_a_grid(
    rect: egui::Rect,
    shader: Option<&Builtin>,
    strength: f32,
    holes: Vec<egui::Rect>,
) {
    GRIDS.with(|grids| grids.borrow_mut().push(rect));
    let Some(amount) = amount_of(shader, strength) else { return };
    DRAWING.with(|drawing| drawing.borrow_mut().push(Region { rect, amount, holes, whole: false }));
}

/// Record that the window's shader is drawn over the whole window this frame. Called after every grid, so the
/// grids drawn in a look of their own are its holes.
///
/// @param rect - the window, in points
/// @param shader - the window's shader, when it has one
/// @param strength - how much of it is drawn, 0 to 1
pub fn note_the_window(rect: egui::Rect, shader: Option<&Builtin>, strength: f32) {
    let Some(amount) = amount_of(shader, strength) else { return };
    let holes = GRIDS.with(|grids| grids.borrow().clone());
    DRAWING.with(|drawing| drawing.borrow_mut().push(Region { rect, amount, holes, whole: true }));
}

/// Where a point in the frame is drawn on the screen once a shader over `rect` has bent it: the point itself for a
/// shader that does not bend. Used for a caret, so a cursor effect is drawn around the caret that is seen.
///
/// @param rect - the shaded rectangle, in window points
/// @param shader - its shader, when it has one
/// @param strength - how much of it is drawn, 0 to 1
/// @param point - the point in the frame, in window points
pub fn on_the_screen(
    rect: egui::Rect,
    shader: Option<&Builtin>,
    strength: f32,
    point: egui::Pos2,
) -> egui::Pos2 {
    match amount_of(shader, strength) {
        Some(amount) => unbend(&Region { rect, amount, holes: Vec::new(), whole: false }, point),
        None => point,
    }
}

/// The regions the next frame's input is bent by: what the frame just drawn recorded. Called once before each pass,
/// which also starts the lists for the pass about to be drawn.
pub fn start_the_frame() -> Vec<Region> {
    GRIDS.with(|grids| grids.borrow_mut().clear());
    let drawn = DRAWING.with(|drawing| std::mem::take(&mut *drawing.borrow_mut()));
    DRAWN.with(|last| *last.borrow_mut() = drawn.clone());
    drawn
}

/// The regions the input of the frame now being drawn was bent by.
pub fn regions_in_effect() -> Vec<Region> {
    DRAWN.with(|last| last.borrow().clone())
}

/// Where `point` is in the bent picture: the first region it is in, grids before the window.
///
/// @param regions - the last frame's regions
/// @param point - a pointer position, in window points
pub fn bend(regions: &[Region], point: egui::Pos2) -> egui::Pos2 {
    let grids = regions.iter().filter(|region| !region.whole);
    let windows = regions.iter().filter(|region| region.whole);
    grids.chain(windows).find_map(|region| region.bend(point)).unwrap_or(point)
}

/// Where a caret at `point` in the frame is drawn on the screen once the shader has bent it: the inverse of
/// [`bend`] for the region the caret is in.
///
/// @param region - the region the caret is drawn in
/// @param point - the caret, in window points
pub fn unbend(region: &Region, point: egui::Pos2) -> egui::Pos2 {
    let size = region.rect.size();
    if size.x <= 0.0 || size.y <= 0.0 {
        return point;
    }
    let read = (point - region.rect.min) / size;
    region.rect.min + uncurve(read, region.amount) * size
}

/// Move every pointer position in `events` to the place in the frame that is drawn under it.
///
/// @param regions - the last frame's regions
/// @param events - the events the window is about to be given
pub fn bend_the_pointer(regions: &[Region], events: &mut [egui::Event]) {
    if regions.is_empty() {
        return;
    }
    for event in events {
        match event {
            egui::Event::PointerMoved(pos) => *pos = bend(regions, *pos),
            egui::Event::PointerButton { pos, .. } => *pos = bend(regions, *pos),
            egui::Event::Touch { pos, .. } => *pos = bend(regions, *pos),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::terminal_shader::named;

    fn close(a: egui::Vec2, b: egui::Vec2) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn only_the_crt_and_retro_bend_the_picture() {
        let bending: Vec<&str> = crate::services::terminal_shader::BUILTINS
            .iter()
            .filter(|builtin| curve_of(builtin).is_some())
            .map(|builtin| builtin.name)
            .collect();
        assert_eq!(bending, vec!["crt", "retro"]);
        let warping: Vec<&str> = crate::services::terminal_shader::BUILTINS
            .iter()
            .filter(|builtin| builtin.source.contains("fn termoil_warp("))
            .map(|builtin| builtin.name)
            .collect();
        assert_eq!(
            warping, bending,
            "a shader that reads somewhere else has a curve here, and only one does"
        );
        // Each file says the same curve, and reads its strength.
        for name in ["crt", "retro"] {
            let source = named(name).expect("shipped").source;
            let expected =
                format!("const CURVE: f32 = {};", curve_of(named(name).unwrap()).unwrap());
            assert!(source.contains(&expected), "{name} does not say {expected}");
            assert!(source.contains("fn termoil_warp("), "{name} has no termoil_warp");
            assert!(source.contains("termoil.strength"), "{name} does not read its strength");
        }
    }

    #[test]
    fn the_curve_leaves_the_center_and_moves_a_corner_outward() {
        let middle = egui::vec2(0.5, 0.5);
        assert!(close(curve(middle, 0.03), middle));
        let near = egui::vec2(0.9, 0.95);
        assert!(close(curve(near, 0.0), near), "nothing at strength 0");
        let bent = curve(near, 0.03);
        assert!(bent.x > near.x && bent.y > near.y, "{bent:?}");
    }

    #[test]
    fn the_inverse_undoes_the_curve() {
        for &(x, y) in &[(0.1, 0.2), (0.9, 0.95), (0.5, 0.0), (0.0, 0.0), (0.73, 0.31)] {
            let uv = egui::vec2(x, y);
            for amount in [0.0, 0.0225, 0.03 * 0.7] {
                let back = uncurve(curve(uv, amount), amount);
                assert!(close(back, uv), "{uv:?} at {amount}: {back:?}");
            }
        }
    }

    #[test]
    fn a_pointer_inside_a_region_is_moved_and_one_outside_is_not() {
        let window = Region {
            rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 800.0)),
            amount: 0.0225,
            holes: vec![egui::Rect::from_min_size(
                egui::pos2(600.0, 500.0),
                egui::vec2(400.0, 300.0),
            )],
            whole: true,
        };
        let grid = Region {
            rect: egui::Rect::from_min_size(egui::pos2(600.0, 500.0), egui::vec2(400.0, 300.0)),
            amount: 0.03,
            holes: Vec::new(),
            whole: false,
        };
        let regions = vec![window.clone(), grid.clone()];
        let mut events = vec![
            egui::Event::PointerMoved(egui::pos2(500.0, 400.0)),
            egui::Event::PointerMoved(egui::pos2(20.0, 30.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(990.0, 790.0),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerMoved(egui::pos2(2000.0, 30.0)),
        ];
        bend_the_pointer(&regions, &mut events);
        let at = |index: usize| match &events[index] {
            egui::Event::PointerMoved(pos) => *pos,
            egui::Event::PointerButton { pos, .. } => *pos,
            _ => unreachable!(),
        };
        assert_eq!(at(0), egui::pos2(500.0, 400.0), "the center does not move");
        assert!(
            at(1).x < 20.0 && at(1).y < 30.0,
            "near the top left it reads further out: {:?}",
            at(1)
        );
        // In the grid, the grid's own curve and nothing of the window's.
        assert_eq!(at(2), grid.bend(egui::pos2(990.0, 790.0)).unwrap());
        assert_eq!(at(3), egui::pos2(2000.0, 30.0), "outside every region");
        let caret = egui::pos2(100.0, 50.0);
        let drawn = unbend(&window, caret);
        assert!((window.bend(drawn).unwrap() - caret).length() < 0.01);
    }

    #[test]
    fn a_shader_that_does_not_bend_records_nothing() {
        let _ = start_the_frame();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(10.0, 10.0));
        note_the_window(rect, named("amber"), 1.0);
        note_the_window(rect, named("crt"), 0.0);
        assert!(start_the_frame().is_empty());
        // A grid in a look of its own that does not bend is still a hole in the window's region.
        let grid = egui::Rect::from_min_size(egui::pos2(5.0, 5.0), egui::vec2(5.0, 5.0));
        note_a_grid(grid, named("amber"), 1.0, Vec::new());
        note_the_window(rect, named("crt"), 0.5);
        let regions = start_the_frame();
        assert_eq!(regions.len(), 1);
        assert!((regions[0].amount - 0.0225 * 0.5).abs() < 1e-6);
        assert_eq!(regions[0].holes, vec![grid]);
        assert_eq!(regions_in_effect(), regions);
        assert!(start_the_frame().is_empty(), "the next frame starts with nothing");
    }
}
