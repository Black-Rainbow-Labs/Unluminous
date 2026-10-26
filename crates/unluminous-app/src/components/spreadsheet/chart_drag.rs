//! Moving and sizing a chart with the pointer (`task-2360`, the `chartwire` lane). Dragging the body moves
//! the chart and dragging a corner sizes it. Nothing is written while the pointer is down: the outline of
//! where the chart would land is drawn, and on release the drag becomes the words of `chart-move`, so a
//! pointer and an agent move a chart the same way and the whole drag is one undo step.

use egui::{CursorIcon, Pos2, Rect, Vec2};
use unluminous_sheet::objects::Anchor;

use super::grid::Frame;
use super::objects_c::sheet_frame;
use crate::services::spreadsheet::Book;

/// How far from a corner a press still takes it, in points.
const REACH: f32 = 9.0;
/// The least a chart can be sized to, in points.
const LEAST: f32 = 40.0;
/// A drag shorter than this is a click.
const CLICK: f32 = 2.0;

/// What a press on a chart takes hold of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Grab {
    /// The body, which moves the chart.
    Body,
    /// A corner, which sizes it: the corner's horizontal and vertical side (-1 left or top, 1 right or bottom).
    Corner(i8, i8),
}

/// A drag in progress, kept in the context's memory between frames.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub grab: Grab,
    pub start: Pos2,
    pub rect: Rect,
}

/// What a press at a point takes hold of on a chart drawn in a rectangle.
pub fn grab_at(rect: Rect, p: Pos2) -> Grab {
    for (sx, sy) in [(1i8, 1i8), (-1, 1), (1, -1), (-1, -1)] {
        let corner = Pos2::new(
            if sx > 0 { rect.right() } else { rect.left() },
            if sy > 0 { rect.bottom() } else { rect.top() },
        );
        if corner.distance(p) <= REACH {
            return Grab::Corner(sx, sy);
        }
    }
    Grab::Body
}

/// The pointer shape for something grabbed or about to be.
pub fn cursor(grab: Grab) -> CursorIcon {
    match grab {
        Grab::Body => CursorIcon::Grab,
        Grab::Corner(sx, sy) if sx == sy => CursorIcon::ResizeNwSe,
        Grab::Corner(..) => CursorIcon::ResizeNeSw,
    }
}

/// Where the chart lands if the pointer is let go at `to`.
pub fn landing(drag: &Drag, to: Pos2) -> Rect {
    let delta = to - drag.start;
    let r = drag.rect;
    match drag.grab {
        Grab::Body => r.translate(delta),
        Grab::Corner(sx, sy) => {
            let mut min = r.min;
            let mut max = r.max;
            if sx > 0 {
                max.x += delta.x
            } else {
                min.x += delta.x
            }
            if sy > 0 {
                max.y += delta.y
            } else {
                min.y += delta.y
            }
            let size = Vec2::new((max.x - min.x).max(LEAST), (max.y - min.y).max(LEAST));
            let left = if sx > 0 { min.x } else { max.x - size.x };
            let top = if sy > 0 { min.y } else { max.y - size.y };
            Rect::from_min_size(Pos2::new(left, top), size)
        }
    }
}

/// The words a released drag stands for, or none when the pointer hardly moved.
pub fn release_words(
    book: &Book,
    frame: &Frame,
    index: usize,
    drag: &Drag,
    to: Pos2,
) -> Vec<String> {
    if (to - drag.start).length() < CLICK {
        return Vec::new();
    }
    let rect = landing(drag, to);
    let [left, top, width, height] = sheet_frame(book, frame, &Anchor::default(), rect);
    let mut words = vec!["chart-move".to_owned(), (index + 1).to_string()];
    words.extend([left, top, width, height].iter().map(|v| format!("{v:.2}")));
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(300.0, 200.0))
    }

    #[test]
    fn a_press_near_a_corner_takes_the_corner_and_elsewhere_the_body() {
        assert_eq!(grab_at(chart(), Pos2::new(398.0, 297.0)), Grab::Corner(1, 1));
        assert_eq!(grab_at(chart(), Pos2::new(102.0, 101.0)), Grab::Corner(-1, -1));
        assert_eq!(grab_at(chart(), Pos2::new(250.0, 200.0)), Grab::Body);
    }

    #[test]
    fn moving_carries_the_chart_and_a_corner_sizes_it_from_the_far_corner() {
        let body = Drag { grab: Grab::Body, start: Pos2::new(200.0, 200.0), rect: chart() };
        assert_eq!(landing(&body, Pos2::new(230.0, 190.0)).min, Pos2::new(130.0, 90.0));
        let corner =
            Drag { grab: Grab::Corner(1, 1), start: Pos2::new(400.0, 300.0), rect: chart() };
        let sized = landing(&corner, Pos2::new(450.0, 320.0));
        assert_eq!((sized.min, sized.max), (Pos2::new(100.0, 100.0), Pos2::new(450.0, 320.0)));
        let top_left =
            Drag { grab: Grab::Corner(-1, -1), start: Pos2::new(100.0, 100.0), rect: chart() };
        let sized = landing(&top_left, Pos2::new(90.0, 80.0));
        assert_eq!((sized.min, sized.max), (Pos2::new(90.0, 80.0), Pos2::new(400.0, 300.0)));
    }

    #[test]
    fn a_chart_cannot_be_sized_below_the_least() {
        let corner =
            Drag { grab: Grab::Corner(1, 1), start: Pos2::new(400.0, 300.0), rect: chart() };
        assert_eq!(landing(&corner, Pos2::new(0.0, 0.0)).size(), Vec2::splat(LEAST));
        let left =
            Drag { grab: Grab::Corner(-1, -1), start: Pos2::new(100.0, 100.0), rect: chart() };
        assert_eq!(landing(&left, Pos2::new(900.0, 900.0)).max, Pos2::new(400.0, 300.0));
    }
}
