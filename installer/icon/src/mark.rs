//! The Unluminous mark, from the Claude Design canvas "unluminous — App Icon" (`task-2258`).
//!
//! A blue `U` with eight coloured pages turning round its bowl. Each page is pinned on the outer edge at
//! the lower right of the bowl and high on the left arm, and bulges further out the further through its
//! turn it is, so the pages run from blue through purple, red and orange to yellow as they go. The design
//! is an SVG drawn by a small script, and every number below is that script's: the `U`'s outline, the
//! curve each page follows, the ten colour stops, the two highlights along a page's edge and the four
//! shadows under it. Nothing here was measured off a picture.
//!
//! The canvas animates the pages with one number, the **phase**, which runs from 0 to 1 once every 0.55
//! seconds. At phase 1 every page has moved to where the next one started, so the loop has no seam.
//! The still version of the design is phase 0.5, and so is the application icon.
//!
//! Everything is in the design's own 1024 point square.

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Mask, Paint, Path, PathBuilder, Pixmap,
    PixmapPaint, Point, Shader, SpreadMode, Stroke, Transform,
};

/// The phase the still version of the design is drawn at.
pub const STILL: f32 = 0.5;

/// How many pages turn round the bowl.
const PAGES: usize = 8;

/// The bowl: its centre, the radius of its outside and of the counter inside it.
const CX: f32 = 512.0;
const CY: f32 = 532.0;
const R: f32 = 380.0;
const RI: f32 = 110.0;
/// The outside of the left arm, and where the inside of it is.
const LX: f32 = 132.0;
const INNER_LEFT: f32 = 402.0;
/// Where the pages are pinned at the right, as an angle round the bowl, and how high up the left arm.
const PIN: f32 = 40.0 * std::f32::consts::PI / 180.0;
const YTOP: f32 = 230.0;
/// How many straight pieces a page's edge is drawn with.
const STEPS: usize = 56;

/// The colour a page has at each point of its turn, from just open to fully turned.
const STOPS: [(f32, u32); 10] = [
    (0.0, 0x4FA8FA),
    (0.07, 0x2A78E8),
    (0.15, 0x1D3CB0),
    (0.24, 0x2E1C7E),
    (0.32, 0x6A0A2E),
    (0.41, 0xB5102C),
    (0.5, 0xE0461A),
    (0.6, 0xF28A18),
    (0.7, 0xF6B01C),
    (1.0, 0xF8CC24),
];

/// The `U`'s own blue.
const FRONT: u32 = 0x1594EE;

/// A colour as three channels from 0 to 255, kept as floats because the design mixes them.
type Rgb = [f32; 3];

fn rgb(hex: u32) -> Rgb {
    [((hex >> 16) & 255) as f32, ((hex >> 8) & 255) as f32, (hex & 255) as f32]
}

/// The design's `shade`: towards white by `amount`, or towards black when `amount` is negative. Each
/// channel is rounded the way the design writes it back out as a hex colour.
fn shade(colour: Rgb, amount: f32) -> Rgb {
    let target = if amount < 0.0 { 0.0 } else { 255.0 };
    let by = amount.abs();
    colour.map(|channel| (channel + (target - channel) * by).round())
}

/// The design's `colorAt`: the colour `s` of the way through a page's turn.
fn colour_at(s: f32) -> Rgb {
    for pair in STOPS.windows(2) {
        let (from, a) = pair[0];
        let (to, b) = pair[1];
        if s <= to {
            let t = (s - from) / (to - from);
            let (a, b) = (rgb(a), rgb(b));
            return [0, 1, 2].map(|i| (a[i] + (b[i] - a[i]) * t).round());
        }
    }
    rgb(STOPS[STOPS.len() - 1].1)
}

fn paint_colour(colour: Rgb, alpha: f32) -> Color {
    Color::from_rgba(colour[0] / 255.0, colour[1] / 255.0, colour[2] / 255.0, alpha)
        .unwrap_or(Color::BLACK)
}

/// Append a circular arc to `path` from `from` radians to `to`, about `centre`, in quarter turns or less.
/// SVG measures angles with y pointing down, and so does this.
fn arc(path: &mut PathBuilder, centre: (f32, f32), radius: f32, from: f32, to: f32) {
    let sweep = to - from;
    let pieces = (sweep.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / pieces as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    for piece in 0..pieces {
        let a = from + step * piece as f32;
        let b = a + step;
        let (sa, ca, sb, cb) = (a.sin(), a.cos(), b.sin(), b.cos());
        path.cubic_to(
            centre.0 + radius * (ca - k * sa),
            centre.1 + radius * (sa + k * ca),
            centre.0 + radius * (cb + k * sb),
            centre.1 + radius * (sb - k * cb),
            centre.0 + radius * cb,
            centre.1 + radius * sb,
        );
    }
}

/// The outline of the `U`, which is also what every page is clipped to.
fn letter() -> Path {
    use std::f32::consts::{FRAC_PI_2, PI};
    let mut path = PathBuilder::new();
    path.move_to(212.0, 112.0);
    path.line_to(372.0, 112.0);
    arc(&mut path, (372.0, 142.0), 30.0, -FRAC_PI_2, 0.0);
    path.line_to(402.0, 532.0);
    // The counter, under from the left arm's inside to the right arm's.
    arc(&mut path, (CX, CY), RI, PI, 0.0);
    path.line_to(622.0, 142.0);
    arc(&mut path, (652.0, 142.0), 30.0, PI, PI + FRAC_PI_2);
    path.line_to(812.0, 112.0);
    arc(&mut path, (812.0, 192.0), 80.0, -FRAC_PI_2, 0.0);
    path.line_to(892.0, 532.0);
    // The outside of the bowl, from the right arm round to the left.
    arc(&mut path, (CX, CY), R, 0.0, PI);
    path.line_to(132.0, 192.0);
    arc(&mut path, (212.0, 192.0), 80.0, PI, PI + FRAC_PI_2);
    path.close();
    path.finish().expect("the U is a closed path")
}

/// The design's `at`: a point `t` of the way along a page's spine, `w` of the way across the letter's
/// stroke from inside to outside.
fn along(t: f32, w: f32) -> (f32, f32) {
    let middle = (RI + R) / 2.0;
    let round_the_bowl = (std::f32::consts::PI - PIN) * middle;
    let up_the_arm = CY - YTOP;
    let length = t * (round_the_bowl + up_the_arm);
    if length < round_the_bowl {
        let angle = PIN + length / middle;
        let radius = RI + w * (R - RI);
        (CX + radius * angle.cos(), CY + radius * angle.sin())
    } else {
        (INNER_LEFT - w * (INNER_LEFT - LX), CY - (length - round_the_bowl))
    }
}

/// One page `s` of the way through its turn: its free edge, and the region between that edge and the
/// outside of the letter.
struct Page {
    edge: Path,
    region: Path,
}

fn page(s: f32) -> Page {
    let reach = 0.2 + (1.45 - 0.2) * s;
    let point = |i: usize| {
        let t = i as f32 / STEPS as f32;
        // `max(0.0)`: in single precision `sin(PI)` comes out a hair below nothing, and a negative number
        // to the power 0.8 is not a number.
        let bump = (std::f32::consts::PI * t).sin().max(0.0).powf(0.8);
        along(t, 1.0 - (1.0 - reach) * bump)
    };
    // The design writes each point with one decimal place, and so does this.
    let tenth = |v: f32| (v * 10.0).round() / 10.0;

    let mut edge = PathBuilder::new();
    let mut region = PathBuilder::new();
    for i in 0..=STEPS {
        let (x, y) = point(i);
        let (x, y) = (tenth(x), tenth(y));
        if i == 0 {
            edge.move_to(x, y);
            region.move_to(x, y);
        } else {
            edge.line_to(x, y);
            region.line_to(x, y);
        }
    }
    region.line_to(LX, CY);
    arc(&mut region, (CX, CY), R, std::f32::consts::PI, PIN);
    region.close();
    Page {
        edge: edge.finish().expect("a page's edge"),
        region: region.finish().expect("a page's region"),
    }
}

/// A diagonal gradient across `from` to `to`, in the design's own square.
fn gradient(from: (f32, f32), to: (f32, f32), stops: Vec<(f32, Rgb)>) -> Shader<'static> {
    LinearGradient::new(
        Point::from_xy(from.0, from.1),
        Point::from_xy(to.0, to.1),
        stops.into_iter().map(|(at, colour)| GradientStop::new(at, paint_colour(colour, 1.0))).collect(),
        SpreadMode::Pad,
        Transform::identity(),
    )
    .expect("a gradient with two stops")
}

fn fill(pixmap: &mut Pixmap, path: &Path, shader: Shader, t: Transform, mask: &Mask) {
    let paint = Paint { shader, anti_alias: true, ..Paint::default() };
    pixmap.fill_path(path, &paint, FillRule::Winding, t, Some(mask));
}

/// A stroke as SVG draws one by default: butt ends and mitred joins.
fn stroke(pixmap: &mut Pixmap, path: &Path, colour: Rgb, alpha: f32, width: f32, t: Transform, mask: &Mask) {
    let paint = Paint {
        shader: Shader::SolidColor(paint_colour(colour, alpha)),
        anti_alias: true,
        ..Paint::default()
    };
    let stroke = Stroke { width, ..Stroke::default() };
    pixmap.stroke_path(path, &paint, &stroke, t, Some(mask));
}

/// A mask that is `path` within `within`.
fn clip(within: &Mask, path: &Path, t: Transform) -> Mask {
    let mut mask = within.clone();
    mask.intersect_path(path, FillRule::Winding, true, t);
    mask
}

/// Draw the letter and its pages at `phase` into `pixmap`, through `t`, which maps the design's square
/// onto the pixmap.
pub fn draw(pixmap: &mut Pixmap, phase: f32, t: Transform) {
    let letter = letter();
    let mut inside = Mask::new(pixmap.width(), pixmap.height()).expect("a mask the size of the picture");
    inside.fill_path(&letter, FillRule::Winding, true, t);

    // The letter itself, lit from the upper left.
    let front = rgb(FRONT);
    fill(
        pixmap,
        &letter,
        gradient(
            (200.0, 100.0),
            (760.0, 900.0),
            vec![(0.0, shade(front, 0.16)), (0.6, front), (1.0, shade(front, -0.18))],
        ),
        t,
        &inside,
    );

    // The page that lies flat against the letter, before any of them has lifted.
    let flat = page(0.0);
    let lip = shade(front, 0.5);
    stroke(pixmap, &flat.edge, lip, 0.3, 30.0, t, &inside);
    stroke(pixmap, &flat.edge, lip, 0.4, 10.0, t, &inside);
    let below = colour_at(phase / PAGES as f32 * 0.5);
    fill(
        pixmap,
        &flat.region,
        gradient((300.0, 300.0), (900.0, 900.0), vec![(0.0, shade(below, 0.1)), (1.0, shade(below, -0.12))]),
        t,
        &inside,
    );
    let under_flat = clip(&inside, &flat.region, t);
    for (width, alpha) in [(120.0, 0.1), (60.0, 0.12), (22.0, 0.16)] {
        stroke(pixmap, &flat.edge, [0.0; 3], alpha, width, t, &under_flat);
    }

    for k in 0..PAGES {
        let s = (k as f32 + phase) / PAGES as f32;
        let next = ((k as f32 + 1.0 + phase) / PAGES as f32).min(1.0);
        let turning = page(s);
        let colour = colour_at((s + next) / 2.0);
        let lip = shade(colour_at((s - 0.02).max(0.0)), 0.3);

        stroke(pixmap, &turning.edge, lip, 0.12, 22.0, t, &inside);
        stroke(pixmap, &turning.edge, lip, 0.3, 6.0, t, &inside);
        fill(
            pixmap,
            &turning.region,
            gradient((300.0, 300.0), (900.0, 900.0), vec![(0.0, shade(colour, 0.06)), (1.0, shade(colour, -0.1))]),
            t,
            &inside,
        );

        // The shadow the page casts on the one under it, fading as it turns. The four strokes are drawn
        // as one group and the group is faded, as the design's `opacity` on the group does, so where the
        // strokes overlap they do not fade separately.
        let fade = 1.0 - 0.75 * (s / 0.7).min(1.0);
        let mut group = Pixmap::new(pixmap.width(), pixmap.height()).expect("a layer");
        let under = clip(&inside, &turning.region, t);
        for (width, alpha) in [(150.0, 0.09), (90.0, 0.1), (46.0, 0.12), (16.0, 0.14)] {
            stroke(&mut group, &turning.edge, [0.0; 3], alpha, width, t, &under);
        }
        pixmap.draw_pixmap(
            0,
            0,
            group.as_ref(),
            &PixmapPaint { opacity: fade, ..PixmapPaint::default() },
            Transform::identity(),
            None,
        );
    }
}

/// The transform that puts the design's square, scaled by `scale` about its centre, into `side` pixels.
pub fn about_the_centre(side: f32, scale: f32) -> Transform {
    let to_pixels = side / 1024.0;
    Transform::from_translate(512.0, 512.0)
        .pre_scale(scale, scale)
        .pre_translate(-512.0, -512.0)
        .post_scale(to_pixels, to_pixels)
}
