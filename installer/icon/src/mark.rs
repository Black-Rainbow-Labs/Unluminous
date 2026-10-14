//! The Unluminous mark, from the Claude Design canvas "unluminous — App Icon", page "Arches U" (`task-2258`).
//!
//! Six nested channels make a `U`. Each channel is a thick stroke with a thin line of the background drawn
//! along its middle, so it reads as an outlined track rather than a solid band. The arms end at staggered
//! heights in round caps. Every number below is the design's own: the six radii, the twelve arm heights, the
//! two stroke widths and the four colours of its light and dark boards.
//!
//! Everything is in the design's own 1024 point square.

use tiny_skia::{BlendMode, Color, LineCap, Paint, PathBuilder, Pixmap, Shader, Stroke, Transform};

/// The centre every channel's bowl is drawn about.
const CX: f32 = 512.0;
const CY: f32 = 521.0;

/// Each channel's radius, and how high its left and its right arm reach, innermost first.
const CHANNELS: [(f32, f32, f32); 6] = [
    (123.5, 260.0, 260.0),
    (181.5, 166.0, 166.0),
    (239.5, 324.0, 324.0),
    (297.5, 86.0, 86.0),
    (355.5, 134.0, 134.0),
    (413.5, 229.0, 291.0),
];

/// The width of a channel, and of the line of background along its middle.
const CHANNEL: f32 = 87.0;
const GROOVE: f32 = 29.0;

/// The square from 40 to 984 both ways holds every channel, caps included, with a little air round it.
pub const BOUNDS: (f32, f32) = (40.0, 984.0);

/// One of the design's two boards.
#[derive(Clone, Copy)]
pub enum Board {
    /// `#111111` channels on `#F4F4F2`.
    Light,
    /// `#EDEDEA` channels on `#121214`.
    Dark,
}

impl Board {
    /// The colour the channels are drawn in.
    pub fn ink(self) -> [u8; 3] {
        match self {
            Board::Light => [0x11, 0x11, 0x11],
            Board::Dark => [0xED, 0xED, 0xEA],
        }
    }

    /// The colour behind them.
    pub fn ground(self) -> [u8; 3] {
        match self {
            Board::Light => [0xF4, 0xF4, 0xF2],
            Board::Dark => [0x12, 0x12, 0x14],
        }
    }
}

fn colour(rgb: [u8; 3]) -> Color {
    Color::from_rgba8(rgb[0], rgb[1], rgb[2], 255)
}

/// One channel's centre line: up its left arm's top, down to the bowl, round under it, and up the right arm.
fn channel(radius: f32, left_top: f32, right_top: f32) -> tiny_skia::Path {
    let mut path = PathBuilder::new();
    path.move_to(CX - radius, left_top);
    path.line_to(CX - radius, CY);
    // From the left (half a turn) to the right (no turn), the short way, which is under the centre.
    let pieces = 4;
    let step = -std::f32::consts::PI / pieces as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    for piece in 0..pieces {
        let a = std::f32::consts::PI + step * piece as f32;
        let b = a + step;
        let (sa, ca, sb, cb) = (a.sin(), a.cos(), b.sin(), b.cos());
        path.cubic_to(
            CX + radius * (ca - k * sa),
            CY + radius * (sa + k * ca),
            CX + radius * (cb + k * sb),
            CY + radius * (sb - k * cb),
            CX + radius * cb,
            CY + radius * sb,
        );
    }
    path.line_to(CX + radius, right_top);
    path.finish().expect("a channel")
}

fn stroke_all(pixmap: &mut Pixmap, paint: &Paint, width: f32, t: Transform) {
    let stroke = Stroke { width, line_cap: LineCap::Round, ..Stroke::default() };
    for (radius, left, right) in CHANNELS {
        pixmap.stroke_path(&channel(radius, left, right), paint, &stroke, t, None);
    }
}

/// Draw the six channels in `board`'s ink through `t`, which maps the design's square onto the pixmap.
///
/// With `ground` the line along each channel's middle is the board's background colour, as the design draws
/// it; without, it is cut out of the picture, so the mark can sit on whatever is behind it.
pub fn draw(pixmap: &mut Pixmap, board: Board, ground: bool, t: Transform) {
    let ink = Paint { shader: Shader::SolidColor(colour(board.ink())), anti_alias: true, ..Paint::default() };
    stroke_all(pixmap, &ink, CHANNEL, t);
    let groove = match ground {
        true => Paint {
            shader: Shader::SolidColor(colour(board.ground())),
            anti_alias: true,
            ..Paint::default()
        },
        false => Paint {
            shader: Shader::SolidColor(Color::BLACK),
            anti_alias: true,
            blend_mode: BlendMode::DestinationOut,
            ..Paint::default()
        },
    };
    stroke_all(pixmap, &groove, GROOVE, t);
}

/// The transform that puts the design's square, scaled by `scale` about its centre, into `side` pixels.
pub fn about_the_centre(side: f32, scale: f32) -> Transform {
    let to_pixels = side / 1024.0;
    Transform::from_translate(512.0, 512.0)
        .pre_scale(scale, scale)
        .pre_translate(-512.0, -512.0)
        .post_scale(to_pixels, to_pixels)
}
