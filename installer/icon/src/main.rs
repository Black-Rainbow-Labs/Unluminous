//! Draws the Unluminous application icon, once, at 1024 points, and writes every size and both container
//! formats the two installers ask for, and the still mark the window draws in its title bar.
//!
//! The mark is the one designed on the Claude Design canvas "unluminous — App Icon" (`task-2258`): a blue
//! `U` with eight coloured pages turning round its bowl. `mark.rs` draws it, from the design's own numbers.
//!
//! **Nothing moves.** Both icons and the title bar's mark are the design's still version, the turn
//! stopped half way. The title bar's mark is `crates/unluminous-app/icons/logo/logo.png`, the letter on
//! nothing, and it is the same picture in a light and a dark theme. An animated version was built on
//! `task-2258` and taken out at Jason's request, and `task-2271` put this blue `U` back after a black and
//! white one had replaced it.
//!
//! The two icons are not the same drawing. **macOS** gets the design exactly: the dark rounded plate is
//! 824 points of the 1024 with a shadow under it, which is the grid every macOS icon since Big Sur is
//! drawn on, so it sits in the Dock at the size of its neighbours. **Windows** draws an icon to the edge of
//! its square, and the same plate there would be a small picture in a large empty square on the taskbar,
//! so its plate is 944 points across with no shadow, and the letter is scaled with it.
//!
//! Everything is drawn **once, large, and downsampled**. A 6 point highlight drawn at 16 points is a
//! smear; the same highlight drawn at 1024 and averaged down is the soft legible mark that every other
//! application's small icon is.
//!
//! Run it with `cargo run --release --manifest-path installer/icon/Cargo.toml`. Its output is
//! committed, because `unluminous.ico` is a build input for `unluminous.exe` itself and `logo.png` is
//! built into the window, so a checkout must build without running this first.

mod mark;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, Shader,
    SpreadMode, Stroke, Transform,
};

/// The side of the drawing everything else is averaged down from.
const MASTER: u32 = 1024;

/// The sizes that go into `unluminous.ico`. Windows asks for these seven; the three smallest are written
/// as a DIB and the rest as PNG, for the reason in `tasks/unluminous-installer-tdd.md`.
const ICO_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

/// The sizes an `.iconset` folder holds, and so the sizes `unluminous.icns` is built from.
const ICONSET_SIZES: [u32; 6] = [16, 32, 64, 128, 256, 512];

fn main() {
    let out = output_folder();

    let mut sizes: Vec<u32> = ICO_SIZES.to_vec();
    sizes.extend(ICONSET_SIZES);
    sizes.push(MASTER);
    sizes.sort_unstable();
    sizes.dedup();

    let mac = icon(MASTER as f32, Plate::Mac);
    let mac: Vec<(u32, Vec<u8>)> =
        sizes.iter().map(|&size| (size, resample(&mac, MASTER, size))).collect();
    write_iconset(&out, &mac);
    write_icns(&out.join("unluminous.icns"), &mac);

    let windows = icon(MASTER as f32, Plate::Windows);
    let windows: Vec<(u32, Vec<u8>)> =
        ICO_SIZES.iter().map(|&size| (size, resample(&windows, MASTER, size))).collect();
    write_ico(&out.join("unluminous.ico"), &windows);

    let logo = out.join("..").join("..").join("crates").join("unluminous-app").join("icons").join("logo");
    fs::create_dir_all(&logo).expect("the logo folder");
    let still = letter_alone(1024, mark::STILL);
    fs::write(logo.join("logo.png"), png(&resample(&still, 1024, 512), 512)).expect("logo.png");

    println!("wrote {} and {}", out.display(), logo.display());
}

/// The folder the files are written to: `installer/icon`, found from this program's own manifest so
/// that it does not matter which directory it was run from.
fn output_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ---------------------------------------------------------------------------------------------
// The drawing
// ---------------------------------------------------------------------------------------------

/// Which platform's plate the icon is drawn on.
#[derive(Clone, Copy)]
enum Plate {
    /// The design's own: 824 points of the 1024, 185 round, with a shadow under it.
    Mac,
    /// 944 points of the 1024, scaled up from the design's, with no shadow.
    Windows,
}

impl Plate {
    /// How far in from the edge of the square the plate starts.
    fn margin(self) -> f32 {
        match self {
            Plate::Mac => 100.0,
            Plate::Windows => 40.0,
        }
    }

    /// How much larger than the design's plate this one is.
    fn scale(self) -> f32 {
        (1024.0 - 2.0 * self.margin()) / 824.0
    }
}

/// The icon at `side` pixels square: the plate, and the letter on it at the still phase.
///
/// On its plate the design draws the letter at 0.82 of its own size, about the centre.
fn icon(side: f32, plate: Plate) -> Pixmap {
    let mut pixmap = Pixmap::new(side as u32, side as u32).expect("a square pixmap");
    let t = Transform::from_scale(side / 1024.0, side / 1024.0);
    if let Plate::Mac = plate {
        shadow(&mut pixmap, plate, t);
    }
    draw_plate(&mut pixmap, plate, t);
    mark::draw(&mut pixmap, mark::STILL, mark::about_the_centre(side, 0.82 * plate.scale()));
    pixmap
}

fn plate_path(plate: Plate) -> tiny_skia::Path {
    let m = plate.margin();
    rounded_rect(m, m, 1024.0 - m, 1024.0 - m, 185.0 * plate.scale())
}

/// The design's plate: `linear-gradient(180deg, #121218, #000000)` with a two point hairline of white at
/// seven per cent inside its edge, so the shape survives a dark wallpaper.
fn draw_plate(pixmap: &mut Pixmap, plate: Plate, t: Transform) {
    let path = plate_path(plate);
    let m = plate.margin();
    let paint = Paint {
        shader: LinearGradient::new(
            Point::from_xy(512.0, m),
            Point::from_xy(512.0, 1024.0 - m),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(0x12, 0x12, 0x18, 255)),
                GradientStop::new(1.0, Color::from_rgba8(0, 0, 0, 255)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .unwrap_or(Shader::SolidColor(Color::BLACK)),
        anti_alias: true,
        ..Paint::default()
    };
    pixmap.fill_path(&path, &paint, FillRule::Winding, t, None);

    let radius = 185.0 * plate.scale() - 1.0;
    let inset = rounded_rect(m + 1.0, m + 1.0, 1024.0 - m - 1.0, 1024.0 - m - 1.0, radius);
    let hairline = Paint {
        shader: Shader::SolidColor(Color::from_rgba8(255, 255, 255, 18)),
        anti_alias: true,
        ..Paint::default()
    };
    let stroke = Stroke { width: 2.0, ..Stroke::default() };
    pixmap.stroke_path(&inset, &hairline, &stroke, t, None);
}

/// The design's `box-shadow: 0 14px 40px rgba(0, 0, 0, 0.45)`: the plate's shape, moved 14 points down,
/// blurred by a Gaussian of 20 points, in black at 45 per cent. Three box blurs make the Gaussian, which
/// is what a browser does too.
fn shadow(pixmap: &mut Pixmap, plate: Plate, t: Transform) {
    let side = pixmap.width() as usize;
    let mut shape = Pixmap::new(side as u32, side as u32).expect("a layer");
    let paint = Paint { shader: Shader::SolidColor(Color::BLACK), anti_alias: true, ..Paint::default() };
    shape.fill_path(&plate_path(plate), &paint, FillRule::Winding, t.pre_translate(0.0, 14.0), None);

    let mut alpha: Vec<f32> = shape.pixels().iter().map(|p| p.alpha() as f32 / 255.0).collect();
    let radius = (20.0 * side as f32 / 1024.0).round() as usize;
    for _ in 0..3 {
        box_blur(&mut alpha, side, radius, true);
        box_blur(&mut alpha, side, radius, false);
    }
    let blurred: Vec<u8> = alpha
        .iter()
        .flat_map(|a| {
            let a = (a * 0.45 * 255.0).round().clamp(0.0, 255.0) as u8;
            [0, 0, 0, a]
        })
        .collect();
    let size = tiny_skia::IntSize::from_wh(side as u32, side as u32).expect("a size");
    let blurred = Pixmap::from_vec(blurred, size).expect("the shadow");
    pixmap.draw_pixmap(
        0,
        0,
        blurred.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::identity(),
        None,
    );
}

/// One pass of a box blur `radius` either side, along the rows or down the columns.
fn box_blur(values: &mut [f32], side: usize, radius: usize, rows: bool) {
    let width = (2 * radius + 1) as f32;
    let mut line = vec![0f32; side];
    for a in 0..side {
        let index = |b: usize| if rows { a * side + b } else { b * side + a };
        let mut sum = 0f32;
        for b in 0..=radius.min(side - 1) {
            sum += values[index(b)];
        }
        for (b, slot) in line.iter_mut().enumerate() {
            *slot = sum / width;
            if b + radius + 1 < side {
                sum += values[index(b + radius + 1)];
            }
            if b >= radius {
                sum -= values[index(b - radius)];
            }
        }
        for (b, value) in line.iter().enumerate() {
            values[index(b)] = *value;
        }
    }
}

/// The letter alone on nothing at `side` pixels square, cropped to the square it fills, at `phase`.
///
/// The letter runs from 132 to 892 across and from 112 to 912 down, so the square from 112 to 912 both
/// ways holds it with twenty points of air either side and none above or below.
fn letter_alone(side: u32, phase: f32) -> Pixmap {
    let mut pixmap = Pixmap::new(side, side).expect("a square pixmap");
    let scale = side as f32 / 800.0;
    mark::draw(&mut pixmap, phase, Transform::from_scale(scale, scale).pre_translate(-112.0, -112.0));
    pixmap
}

/// A rounded rectangle, which `tiny_skia` has no constructor for.
fn rounded_rect(left: f32, top: f32, right: f32, bottom: f32, radius: f32) -> tiny_skia::Path {
    let r = radius.min((right - left) / 2.0).min((bottom - top) / 2.0);
    // The control point that turns a cubic corner into something close to a quarter circle.
    let k = r * 0.5523;
    let mut path = PathBuilder::new();
    path.move_to(left + r, top);
    path.line_to(right - r, top);
    path.cubic_to(right - r + k, top, right, top + r - k, right, top + r);
    path.line_to(right, bottom - r);
    path.cubic_to(right, bottom - r + k, right - r + k, bottom, right - r, bottom);
    path.line_to(left + r, bottom);
    path.cubic_to(left + r - k, bottom, left, bottom - r + k, left, bottom - r);
    path.line_to(left, top + r);
    path.cubic_to(left, top + r - k, left + r - k, top, left + r, top);
    path.close();
    path.finish().expect("a rounded rectangle is a closed path")
}

// ---------------------------------------------------------------------------------------------
// Resampling
// ---------------------------------------------------------------------------------------------

/// Average the master drawing down to `size` points square, and hand back straight (unpremultiplied)
/// RGBA, which is what both PNG and a DIB want.
///
/// The averaging is done on the premultiplied pixels `tiny_skia` produced, because averaging colours
/// that have already been divided by their own alpha smears the colour of the transparent pixels
/// round an edge into the visible ones. Dividing happens once, at the end.
fn resample(master: &Pixmap, from: u32, size: u32) -> Vec<u8> {
    let source = master.data();
    let ratio = from as f32 / size as f32;
    let mut out = vec![0u8; (size * size * 4) as usize];

    for y in 0..size {
        let top = (y as f32 * ratio).floor() as u32;
        let bottom = (((y + 1) as f32 * ratio).ceil() as u32).min(from);
        for x in 0..size {
            let left = (x as f32 * ratio).floor() as u32;
            let right = (((x + 1) as f32 * ratio).ceil() as u32).min(from);

            let mut sums = [0f32; 4];
            let mut count = 0f32;
            for sy in top..bottom {
                for sx in left..right {
                    let at = ((sy * from + sx) * 4) as usize;
                    for channel in 0..4 {
                        sums[channel] += source[at + channel] as f32;
                    }
                    count += 1.0;
                }
            }

            let at = ((y * size + x) * 4) as usize;
            let alpha = sums[3] / count;
            if alpha <= 0.5 {
                continue; // Already zero, and dividing by it would be a division by nothing.
            }
            for channel in 0..3 {
                let straight = (sums[channel] / count) * 255.0 / alpha;
                out[at + channel] = straight.round().clamp(0.0, 255.0) as u8;
            }
            out[at + 3] = alpha.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The three outputs
// ---------------------------------------------------------------------------------------------

/// The straight RGBA of one already-resampled size.
fn at(images: &[(u32, Vec<u8>)], size: u32) -> &[u8] {
    &images.iter().find(|(known, _)| *known == size).expect("every size was resampled").1
}

/// Encode straight RGBA as a PNG in memory.
fn png(pixels: &[u8], size: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, size, size);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Best);
        let mut writer = encoder.write_header().expect("a PNG header");
        writer.write_image_data(pixels).expect("PNG pixels");
    }
    bytes
}

/// Write `Unluminous.iconset`, which is the folder `iconutil` on a Mac turns into an `.icns`, and which is
/// also the plain PNG set anything else can read.
fn write_iconset(out: &Path, images: &[(u32, Vec<u8>)]) {
    let folder = out.join("Unluminous.iconset");
    fs::create_dir_all(&folder).expect("the iconset folder");
    // Apple's names: a size, and the same picture again at twice it for a retina screen.
    for (name, size) in [
        ("icon_16x16.png", 16),
        ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32),
        ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128),
        ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256),
        ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512),
        ("icon_512x512@2x.png", 1024),
    ] {
        fs::write(folder.join(name), png(at(images, size), size)).expect("an iconset picture");
    }
}

/// Write `unluminous.ico`.
///
/// Six bytes of header, sixteen per image, then the images. The three smallest are a DIB — a
/// `BITMAPINFOHEADER` with double height, bottom-up BGRA rows and an all-zero AND mask, because the
/// alpha channel does the masking — and the rest are PNG, which Windows has read since Vista and
/// which is a great deal smaller at 128 and 256.
fn write_ico(path: &Path, images: &[(u32, Vec<u8>)]) {
    let bodies: Vec<(u32, Vec<u8>)> = ICO_SIZES
        .iter()
        .map(|&size| {
            let body =
                if size <= 32 { dib(at(images, size), size) } else { png(at(images, size), size) };
            (size, body)
        })
        .collect();

    let mut file = Vec::new();
    file.extend_from_slice(&0u16.to_le_bytes()); // reserved
    file.extend_from_slice(&1u16.to_le_bytes()); // 1 = an icon rather than a cursor
    file.extend_from_slice(&(bodies.len() as u16).to_le_bytes());

    let mut offset = 6 + 16 * bodies.len() as u32;
    for (size, body) in &bodies {
        // 256 is written as zero, which is the format's way of saying "the big one".
        let side = if *size >= 256 { 0u8 } else { *size as u8 };
        file.push(side);
        file.push(side);
        file.push(0); // no colour palette
        file.push(0); // reserved
        file.extend_from_slice(&1u16.to_le_bytes()); // planes
        file.extend_from_slice(&32u16.to_le_bytes()); // bits a pixel
        file.extend_from_slice(&(body.len() as u32).to_le_bytes());
        file.extend_from_slice(&offset.to_le_bytes());
        offset += body.len() as u32;
    }
    for (_, body) in &bodies {
        file.extend_from_slice(body);
    }

    let mut handle = fs::File::create(path).expect("unluminous.ico");
    handle.write_all(&file).expect("unluminous.ico");
}

/// One image inside an `.ico`, in the device independent bitmap form.
fn dib(pixels: &[u8], size: u32) -> Vec<u8> {
    let mask_row = (((size + 31) / 32) * 4) as usize; // each row padded to four bytes
    let mask_len = mask_row * size as usize;
    let colour_len = (size * size * 4) as usize;

    let mut out = Vec::with_capacity(40 + colour_len + mask_len);
    out.extend_from_slice(&40u32.to_le_bytes()); // header size
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // colour rows and mask rows together
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bits a pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // no compression
    out.extend_from_slice(&((colour_len + mask_len) as u32).to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes()); // pixels a metre across, not stated
    out.extend_from_slice(&0i32.to_le_bytes()); // and down
    out.extend_from_slice(&0u32.to_le_bytes()); // colours used
    out.extend_from_slice(&0u32.to_le_bytes()); // colours that matter

    for y in (0..size).rev() {
        for x in 0..size {
            let at = ((y * size + x) * 4) as usize;
            out.push(pixels[at + 2]); // blue
            out.push(pixels[at + 1]); // green
            out.push(pixels[at]); // red
            out.push(pixels[at + 3]); // alpha
        }
    }
    out.extend(std::iter::repeat(0u8).take(mask_len));
    out
}

/// Write `unluminous.icns`.
///
/// The four bytes `icns`, a big-endian total length, then one chunk per picture: a four byte type, a
/// big-endian length that counts the eight bytes of its own header, and a PNG. macOS has taken PNG in
/// these chunks since 10.8; the run-length encoded `is32`/`s8mk` pair it wanted before that is not
/// written, because nothing left reads it.
fn write_icns(path: &Path, images: &[(u32, Vec<u8>)]) {
    // The type each size goes in. `ic11` to `ic14` are the retina pairs: the same picture, filed
    // under the smaller size it stands in for.
    let chunks: [(&[u8; 4], u32); 10] = [
        (b"ic04", 16),
        (b"ic05", 32),
        (b"ic11", 32),
        (b"ic12", 64),
        (b"ic07", 128),
        (b"ic13", 256),
        (b"ic08", 256),
        (b"ic14", 512),
        (b"ic09", 512),
        (b"ic10", 1024),
    ];

    let mut body = Vec::new();
    for (kind, size) in chunks {
        let picture = png(at(images, size), size);
        body.extend_from_slice(kind);
        body.extend_from_slice(&((picture.len() + 8) as u32).to_be_bytes());
        body.extend_from_slice(&picture);
    }

    let mut file = Vec::with_capacity(body.len() + 8);
    file.extend_from_slice(b"icns");
    file.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
    file.extend_from_slice(&body);

    fs::write(path, file).expect("unluminous.icns");
}
