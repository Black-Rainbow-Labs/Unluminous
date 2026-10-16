//! Marks drawn from published SVG path data, for the buttons whose mark is a named icon from another set.
//!
//! The design's own line marks are `rux::Icon`, a closed list that this crate cannot add to. A mark that
//! `task-2280` takes from Lucide or from the VS Code codicons is therefore kept here as the icon's own
//! `d` strings, parsed once per frame and stroked with the painter, at the size and in the tint the
//! neighbouring `rux` marks use.
//!
//! Supported path commands are `M L H V C A Z` and their lowercase forms, which is every command the three
//! icons below use.

use egui::{Color32, Pos2, Shape, Stroke};

/// One child of an icon's `<svg>`.
#[derive(Debug, Clone, Copy)]
pub enum Part {
    /// `<path d="..."/>`
    Path(&'static str),
    /// `<rect x y width height rx/>`
    Rect { x: f32, y: f32, w: f32, h: f32, rx: f32 },
}

/// An icon: its view box side, the stroke width in view box units, and its parts, all stroked.
#[derive(Debug)]
pub struct SvgMark {
    pub view: f32,
    pub stroke: f32,
    pub parts: &'static [Part],
}

/// Lucide `paintbrush-vertical` (MIT), 24 unit view box.
pub static PAINTBRUSH_VERTICAL: SvgMark = SvgMark {
    view: 24.0,
    stroke: 1.8,
    parts: &[
        Part::Path("M10 2v2"),
        Part::Path("M14 2v4"),
        Part::Path("M17 2a1 1 0 0 1 1 1v9H6V3a1 1 0 0 1 1-1z"),
        Part::Path("M6 12a1 1 0 0 0-1 1v1a2 2 0 0 0 2 2h2a1 1 0 0 1 1 1v2.9a2 2 0 1 0 4 0V17a1 1 0 0 1 1-1h2a2 2 0 0 0 2-2v-1a1 1 0 0 0-1-1"),
    ],
};

/// Lucide `list-todo` (MIT), 24 unit view box.
pub static LIST_TODO: SvgMark = SvgMark {
    view: 24.0,
    stroke: 1.8,
    parts: &[
        Part::Path("M13 5h8"),
        Part::Path("M13 12h8"),
        Part::Path("M13 19h8"),
        Part::Path("m3 17 2 2 4-4"),
        Part::Rect { x: 3.0, y: 4.0, w: 6.0, h: 6.0, rx: 1.0 },
    ],
};

/// VS Code codicon `debug-continue` (CC BY 4.0), 16 unit view box.
///
/// The codicon is a filled outline: a bar 1.5 wide at the left and a play triangle drawn as a ring 1.5
/// wide. A ring of even width is the same picture as a stroke of that width along its middle, so the bar
/// is the line `x = 2.75` from 2.75 to 13.25 (the codicon's bar is x 2 to 3.5, y 2 to 14 with round ends)
/// and the triangle is the closed line through the middle of its ring: the left edge at x 6.75 (ring
/// from 6 to 7.5), and the apex where the two slanted edges, each 1.5 inside the codicon's outer edge,
/// meet at y 8.
pub static DEBUG_CONTINUE: SvgMark = SvgMark {
    view: 16.0,
    stroke: 1.5,
    parts: &[Part::Path("M2.75 2.75V13.25"), Part::Path("M6.75 2.52 14.48 8 6.75 13.48z")],
};

/// Draw `mark` centred on `centre`, `size` points across, in `color`.
pub fn paint(painter: &egui::Painter, centre: Pos2, size: f32, mark: &SvgMark, color: Color32) {
    let scale = size / mark.view;
    let to_screen = |p: [f32; 2]| {
        Pos2::new(
            centre.x + (p[0] - mark.view / 2.0) * scale,
            centre.y + (p[1] - mark.view / 2.0) * scale,
        )
    };
    let width = mark.stroke * scale;
    for part in mark.parts {
        let d = match part {
            Part::Path(d) => (*d).to_owned(),
            Part::Rect { x, y, w, h, rx } => rect_path(*x, *y, *w, *h, *rx),
        };
        for line in flatten(&d) {
            paint_line(painter, &line, &to_screen, width, color);
        }
    }
}

/// One stroked subpath: the line itself and a round dot on every node, which gives round caps and joins.
fn paint_line(painter: &egui::Painter, line: &Line, to_screen: &impl Fn([f32; 2]) -> Pos2, width: f32, color: Color32) {
    let mut points: Vec<Pos2> = line.points.iter().map(|p| to_screen(*p)).collect();
    if line.closed && points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    let stroke = Stroke::new(width, color);
    painter.add(if line.closed { Shape::closed_line(points, stroke) } else { Shape::line(points, stroke) });
    for node in &line.nodes {
        painter.circle_filled(to_screen(*node), width / 2.0, color);
    }
}

/// The path data of a rectangle with corner radius `rx`.
fn rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> String {
    format!(
        "M{} {}H{}A{r} {r} 0 0 1 {} {}V{}A{r} {r} 0 0 1 {} {}H{}A{r} {r} 0 0 1 {} {}V{}A{r} {r} 0 0 1 {} {}z",
        x + r, y, x + w - r, x + w, y + r, y + h - r, x + w - r, y + h, x + r, x, y + h - r, y + r, x + r, y
    )
}

/// A flattened subpath, in view box units.
struct Line {
    points: Vec<[f32; 2]>,
    /// The end of every command, where a round dot goes.
    nodes: Vec<[f32; 2]>,
    closed: bool,
}

/// Split SVG path data into numbers and command letters.
fn tokens(d: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut number = String::new();
    let flush = |number: &mut String, out: &mut Vec<String>| {
        if !number.is_empty() {
            out.push(std::mem::take(number));
        }
    };
    for c in d.chars() {
        match c {
            'a'..='z' | 'A'..='Z' => {
                flush(&mut number, &mut out);
                out.push(c.to_string());
            }
            '-' => {
                flush(&mut number, &mut out);
                number.push(c);
            }
            '.' => {
                if number.contains('.') {
                    flush(&mut number, &mut out);
                }
                number.push(c);
            }
            c if c.is_ascii_digit() => number.push(c),
            _ => flush(&mut number, &mut out),
        }
    }
    flush(&mut number, &mut out);
    out
}

/// Walk path data and return its subpaths as polylines, curves cut into short segments.
fn flatten(d: &str) -> Vec<Line> {
    let tokens = tokens(d);
    let mut lines: Vec<Line> = Vec::new();
    let (mut at, mut start) = ([0.0f32; 2], [0.0f32; 2]);
    let (mut index, mut command) = (0usize, 'M');
    let number = |index: &mut usize| -> f32 {
        let value = tokens.get(*index).and_then(|t| t.parse().ok()).unwrap_or(0.0);
        *index += 1;
        value
    };
    while index < tokens.len() {
        if tokens[index].chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            command = tokens[index].chars().next().unwrap_or('M');
            index += 1;
            if command == 'z' || command == 'Z' {
                if let Some(line) = lines.last_mut() {
                    line.closed = true;
                }
                at = start;
                continue;
            }
        }
        let relative = command.is_ascii_lowercase();
        let base = if relative { at } else { [0.0, 0.0] };
        let mut next = at;
        match command.to_ascii_uppercase() {
            'M' | 'L' => {
                next = [base[0] + number(&mut index), base[1] + number(&mut index)];
                if command.to_ascii_uppercase() == 'M' {
                    start = next;
                    lines.push(Line { points: vec![next], nodes: vec![next], closed: false });
                    command = if relative { 'l' } else { 'L' };
                } else {
                    push_to(&mut lines, &[next], next);
                }
            }
            'H' => {
                next = [base[0] + number(&mut index), at[1]];
                push_to(&mut lines, &[next], next);
            }
            'V' => {
                next = [at[0], base[1] + number(&mut index)];
                push_to(&mut lines, &[next], next);
            }
            'C' => {
                let c1 = [base[0] + number(&mut index), base[1] + number(&mut index)];
                let c2 = [base[0] + number(&mut index), base[1] + number(&mut index)];
                next = [base[0] + number(&mut index), base[1] + number(&mut index)];
                push_to(&mut lines, &cubic(at, c1, c2, next), next);
            }
            'A' => {
                let (rx, ry, turn) = (number(&mut index), number(&mut index), number(&mut index));
                let (large, sweep) = (number(&mut index) != 0.0, number(&mut index) != 0.0);
                next = [base[0] + number(&mut index), base[1] + number(&mut index)];
                push_to(&mut lines, &arc(at, rx, ry, turn, large, sweep, next), next);
            }
            _ => index += 1,
        }
        at = next;
    }
    lines
}

/// Add `points` to the subpath being drawn, with `node` as the end of the command.
fn push_to(lines: &mut [Line], points: &[[f32; 2]], node: [f32; 2]) {
    if let Some(line) = lines.last_mut() {
        line.points.extend_from_slice(points);
        line.nodes.push(node);
    }
}

/// A cubic Bezier cut into twelve segments, not including its first point.
fn cubic(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2]) -> Vec<[f32; 2]> {
    (1..=12)
        .map(|i| {
            let t = i as f32 / 12.0;
            let u = 1.0 - t;
            let at = |k: usize| {
                u * u * u * p0[k] + 3.0 * u * u * t * p1[k] + 3.0 * u * t * t * p2[k] + t * t * t * p3[k]
            };
            [at(0), at(1)]
        })
        .collect()
}

/// An SVG elliptical arc (the specification's endpoint to centre conversion, F.6.5) cut into segments,
/// not including its first point.
fn arc(from: [f32; 2], rx: f32, ry: f32, degrees: f32, large: bool, sweep: bool, to: [f32; 2]) -> Vec<[f32; 2]> {
    if from == to || rx == 0.0 || ry == 0.0 {
        return vec![to];
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    let phi = degrees.to_radians();
    let (cos, sin) = (phi.cos(), phi.sin());
    let (dx, dy) = ((from[0] - to[0]) / 2.0, (from[1] - to[1]) / 2.0);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let scale_up = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if scale_up > 1.0 {
        rx *= scale_up.sqrt();
        ry *= scale_up.sqrt();
    }
    let numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let factor = sign * (numerator / denominator).max(0.0).sqrt();
    let (cx1, cy1) = (factor * rx * y1 / ry, -factor * ry * x1 / rx);
    let centre = [
        cos * cx1 - sin * cy1 + (from[0] + to[0]) / 2.0,
        sin * cx1 + cos * cy1 + (from[1] + to[1]) / 2.0,
    ];
    let angle = |ux: f32, uy: f32| uy.atan2(ux);
    let start = angle((x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut sweep_by = angle((-x1 - cx1) / rx, (-y1 - cy1) / ry) - start;
    if sweep && sweep_by < 0.0 {
        sweep_by += std::f32::consts::TAU;
    } else if !sweep && sweep_by > 0.0 {
        sweep_by -= std::f32::consts::TAU;
    }
    (1..=12)
        .map(|i| {
            let theta = start + sweep_by * i as f32 / 12.0;
            let (ex, ey) = (rx * theta.cos(), ry * theta.sin());
            [cos * ex - sin * ey + centre[0], sin * ex + cos * ey + centre[1]]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_arc_ends_where_the_path_says_and_stays_on_its_circle() {
        let points = arc([17.0, 2.0], 1.0, 1.0, 0.0, false, true, [18.0, 3.0]);
        let last = points.last().copied().unwrap_or_default();
        assert!((last[0] - 18.0).abs() < 1e-3 && (last[1] - 3.0).abs() < 1e-3, "{last:?}");
        for p in points {
            let d = ((p[0] - 17.0).powi(2) + (p[1] - 3.0).powi(2)).sqrt();
            assert!((d - 1.0).abs() < 1e-3, "{p:?} is {d} from the centre");
        }
    }

    #[test]
    fn every_icon_parses_into_lines_inside_its_view_box() {
        for mark in [&PAINTBRUSH_VERTICAL, &LIST_TODO, &DEBUG_CONTINUE] {
            for part in mark.parts {
                let d = match part {
                    Part::Path(d) => (*d).to_owned(),
                    Part::Rect { x, y, w, h, rx } => rect_path(*x, *y, *w, *h, *rx),
                };
                let lines = flatten(&d);
                assert!(!lines.is_empty(), "{d}");
                for p in lines.iter().flat_map(|l| &l.points) {
                    assert!(p[0] >= 0.0 && p[0] <= mark.view && p[1] >= 0.0 && p[1] <= mark.view, "{d}: {p:?}");
                }
            }
        }
    }
}
