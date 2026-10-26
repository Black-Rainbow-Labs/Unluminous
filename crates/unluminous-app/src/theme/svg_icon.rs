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

/// Lucide `key-round`'s older drawing, `key` (MIT), 24 unit view box: a ring and a shaft with two teeth.
///
/// The Settings list's Keymap mark since `task-2293`, whose `rux::Icon::Key` sat a unit and a half above the
/// middle of its view box and so above the middle of its row. This one fills the box evenly, 2 to 21 both ways.
pub static KEY: SvgMark = SvgMark {
    view: 24.0,
    stroke: 1.8,
    parts: &[
        Part::Path("M2 15.5a5.5 5.5 0 1 0 11 0a5.5 5.5 0 1 0 -11 0z"),
        Part::Path("M21 2 11.4 11.6"),
        Part::Path("M15.5 7.5 17.8 9.8a1 1 0 0 0 1.4 0l2.1 -2.1a1 1 0 0 0 0 -1.4L19 4"),
    ],
};

/// Material `record_voice_over` (Apache 2.0), as a line mark in a 24 unit view box: a head and shoulders and
/// two arcs of sound in front of the face. `task-2293`: *"The voice icon should look more like a person
/// talking, not music, like MdRecordVoiceOver."* The Material icon is filled; it is drawn here as strokes along
/// the same shapes so it sits with the line marks round it in the Settings list.
pub static VOICE_OVER: SvgMark = SvgMark {
    view: 24.0,
    stroke: 1.8,
    parts: &[
        Part::Path("M5.5 9a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0 -7 0z"),
        Part::Path("M2 20.5v-0.5c0 -2.7 4.6 -4.2 7 -4.2s7 1.5 7 4.2v0.5"),
        Part::Path("M15.6 6.6a3.4 3.4 0 0 1 0 4.8"),
        Part::Path("M18.6 3.8a7.4 7.4 0 0 1 0 10.4"),
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

/// A Lucide line mark (MIT) in a 24 unit view box with Lucide's own stroke, drawn a little lighter so it
/// sits with the `rux` marks beside it. The Word tab's bar, flyout and panel use these (`task-2366`).
macro_rules! lucide {
    ($(#[$doc:meta])* $name:ident, [$($part:expr),* $(,)?]) => {
        $(#[$doc])*
        pub static $name: SvgMark =
            SvgMark { view: 24.0, stroke: 1.9, parts: &[$(Part::Path($part)),*] };
    };
}

lucide!(
    /// Lucide `bold`.
    BOLD,
    ["M6 12h9a4 4 0 0 1 0 8H7a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h7a4 4 0 0 1 0 8"]
);
lucide!(
    /// Lucide `italic`.
    ITALIC,
    ["M19 4h-9", "M14 20H5", "M15 4 9 20"]
);
lucide!(
    /// Lucide `underline`.
    UNDERLINE,
    ["M6 4v6a6 6 0 0 0 12 0V4", "M4 20h16"]
);
lucide!(
    /// Lucide `strikethrough`.
    STRIKETHROUGH,
    ["M16 4H9a3 3 0 0 0-2.83 4", "M14 12a4 4 0 0 1 0 8H6", "M4 12h16"]
);
lucide!(
    /// Lucide `superscript`, its small two written with `C` where Lucide writes `S`, which this parser
    /// does not read.
    SUPERSCRIPT,
    [
        "m4 19 8-8",
        "m12 19-8-8",
        "M20 12h-4c0-1.5 0.4-2 1.5-2.5C18.6 9 20 8.3 20 7c0-1.1-0.9-2-2-2c-0.8 0-1.5 0.4-1.8 1.1"
    ]
);
lucide!(
    /// Lucide `subscript`, with the two written as in [`SUPERSCRIPT`].
    SUBSCRIPT,
    [
        "m4 5 8 8",
        "m12 5-8 8",
        "M20 19h-4c0-1.5 0.4-2 1.5-2.5C18.6 16 20 15.3 20 14c0-1.1-0.9-2-2-2c-0.8 0-1.5 0.4-1.8 1.1"
    ]
);
lucide!(
    /// Lucide `list`.
    LIST,
    ["M3 12h0.01", "M3 18h0.01", "M3 6h0.01", "M8 12h13", "M8 18h13", "M8 6h13"]
);
lucide!(
    /// Lucide `list-ordered`, its two written with `C` in place of `S`.
    LIST_ORDERED,
    ["M10 12h11", "M10 18h11", "M10 6h11", "M4 10h2", "M4 6h1v4", "M6 18H4c0-1 2-2 2-3C6 14 5 13.5 4 14"]
);
lucide!(
    /// Lucide `list-indent-decrease`.
    OUTDENT,
    ["M21 12H11", "M21 18H11", "M21 6H11", "m7 8-4 4 4 4"]
);
lucide!(
    /// Lucide `list-indent-increase`.
    INDENT,
    ["M21 12H11", "M21 18H11", "M21 6H11", "m3 8 4 4-4 4"]
);
lucide!(
    /// Lucide `text-align-start`.
    ALIGN_LEFT,
    ["M15 12H3", "M17 18H3", "M21 6H3"]
);
lucide!(
    /// Lucide `text-align-center`.
    ALIGN_CENTER,
    ["M17 12H7", "M19 18H5", "M21 6H3"]
);
lucide!(
    /// Lucide `text-align-end`.
    ALIGN_RIGHT,
    ["M21 12H9", "M21 18H7", "M21 6H3"]
);
lucide!(
    /// Lucide `text-align-justify`.
    ALIGN_JUSTIFY,
    ["M3 12h18", "M3 18h18", "M3 6h18"]
);
lucide!(
    /// Lucide `a-large-small`: the Format button, a large A beside a small a.
    LETTER_SIZES,
    ["M21 14h-5", "M16 16v-3.5a2.5 2.5 0 0 1 5 0V16", "M4.5 13h6", "m3 16 4.5-9 4.5 9"]
);
lucide!(
    /// Lucide `type`: the Text level.
    TYPE,
    ["M12 4v16", "M4 7V5a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v2", "M9 20h6"]
);
lucide!(
    /// Lucide `pilcrow`: the Paragraph level.
    PILCROW,
    ["M13 4v16", "M17 4v16", "M19 4H9.5a4.5 4.5 0 0 0 0 9H13"]
);
lucide!(
    /// Lucide `heading`: the Styles level.
    HEADING,
    ["M6 12h12", "M6 20V4", "M18 20V4"]
);
lucide!(
    /// Lucide `link`.
    LINK,
    [
        "M10 13a5 5 0 0 0 7.54 0.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71",
        "M14 11a5 5 0 0 0-7.54-0.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"
    ]
);
lucide!(
    /// Lucide `highlighter`.
    HIGHLIGHTER,
    ["m9 11-6 6v3h9l3-3", "m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4"]
);
lucide!(
    /// Lucide `book-open`: the References level. Lucide repeats each arc command without its letter; it is
    /// written out here.
    BOOK_OPEN,
    [
        "M12 7v14",
        "M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4a4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3a3 3 0 0 0-3-3z"
    ]
);
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
fn paint_line(
    painter: &egui::Painter,
    line: &Line,
    to_screen: &impl Fn([f32; 2]) -> Pos2,
    width: f32,
    color: Color32,
) {
    let mut points: Vec<Pos2> = line.points.iter().map(|p| to_screen(*p)).collect();
    if line.closed && points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    let stroke = Stroke::new(width, color);
    painter.add(if line.closed {
        Shape::closed_line(points, stroke)
    } else {
        Shape::line(points, stroke)
    });
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
                if command.eq_ignore_ascii_case(&'M') {
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
                u * u * u * p0[k]
                    + 3.0 * u * u * t * p1[k]
                    + 3.0 * u * t * t * p2[k]
                    + t * t * t * p3[k]
            };
            [at(0), at(1)]
        })
        .collect()
}

/// An SVG elliptical arc (the specification's endpoint to centre conversion, F.6.5) cut into segments,
/// not including its first point.
fn arc(
    from: [f32; 2],
    rx: f32,
    ry: f32,
    degrees: f32,
    large: bool,
    sweep: bool,
    to: [f32; 2],
) -> Vec<[f32; 2]> {
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
            assert!((d - 1.0).abs() < 1e-3, "{p:?} is {d} from the center");
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
                    assert!(
                        p[0] >= 0.0 && p[0] <= mark.view && p[1] >= 0.0 && p[1] <= mark.view,
                        "{d}: {p:?}"
                    );
                }
            }
        }
    }
}
