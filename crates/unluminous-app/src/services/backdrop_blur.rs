//! A blurred copy of the window, drawn under a modal's backdrop (`task-2247`).
//!
//! egui draws every layer straight into the frame and has no way to read back what is under one, so
//! there is nothing a modal could blur as it is drawn. What there is, is `ViewportCommand::Screenshot`:
//! the window is photographed once when a modal opens, with the modal hidden for that one frame, and the
//! picture is shrunk, blurred and uploaded as a texture the backdrop draws. A picture rather than the
//! window means what is behind the dialog stands still while it is open, which is what a dialog that
//! asks for attention wants anyway. It is taken again when the theme changes. It is not taken again when
//! the window changes size, because taking it hides the modal for a frame and a resize is many frames
//! (`task-2255`); the picture is stretched to the new size, and it is blurred past reading anyway.
//!
//! Shrinking first is what makes it cheap: a 2560 by 1440 window is a 640 by 360 picture by the time it
//! is blurred, and the texture's linear filter smooths it back up to the window's size.

/// How many pixels of the window become one pixel of the blurred picture, along each side.
pub const SHRINK: usize = 4;

/// The radius of each of the three box blurs, in pixels of the shrunk picture. Three passes of a box are
/// close to a Gaussian, and two pixels at a quarter of the size is eight on the screen: a slight blur,
/// enough that text behind the dialog no longer reads.
pub const RADIUS: usize = 2;

/// The marker a backdrop's screenshot request carries, so the command line's `window screenshot` does
/// not take it and this does not take the command line's.
#[derive(Debug)]
pub struct Marker;

/// Whether a screenshot event was asked for by the backdrop.
pub fn is_ours(user_data: &egui::UserData) -> bool {
    user_data.data.as_ref().is_some_and(|data| data.downcast_ref::<Marker>().is_some())
}

/// The window's picture, shrunk by [`SHRINK`] and blurred.
///
/// @param image - the screenshot, in pixels, with premultiplied colours
pub fn blurred(image: &egui::ColorImage) -> egui::ColorImage {
    let small = shrink(image, SHRINK);
    let mut pixels: Vec<[f32; 4]> = small
        .pixels
        .iter()
        .map(|p| [p.r() as f32, p.g() as f32, p.b() as f32, p.a() as f32])
        .collect();
    let [width, height] = small.size;
    for _ in 0..3 {
        box_blur(&mut pixels, width, height, RADIUS);
    }
    let out = pixels
        .iter()
        .map(|p| {
            let [r, g, b, a] = p.map(|v| v.round().clamp(0.0, 255.0) as u8);
            egui::Color32::from_rgba_premultiplied(r, g, b, a)
        })
        .collect();
    egui::ColorImage { size: [width, height], source_size: small.source_size, pixels: out }
}

/// Every `factor` by `factor` block of pixels averaged into one.
fn shrink(image: &egui::ColorImage, factor: usize) -> egui::ColorImage {
    let [width, height] = image.size;
    let small = [(width / factor).max(1), (height / factor).max(1)];
    let mut pixels = Vec::with_capacity(small[0] * small[1]);
    for y in 0..small[1] {
        for x in 0..small[0] {
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for dy in 0..factor {
                for dx in 0..factor {
                    let (px, py) = (x * factor + dx, y * factor + dy);
                    if px >= width || py >= height {
                        continue;
                    }
                    let p = image.pixels[py * width + px];
                    for (total, value) in sum.iter_mut().zip([p.r(), p.g(), p.b(), p.a()]) {
                        *total += u32::from(value);
                    }
                    count += 1;
                }
            }
            let [r, g, b, a] = sum.map(|total| (total / count.max(1)) as u8);
            pixels.push(egui::Color32::from_rgba_premultiplied(r, g, b, a));
        }
    }
    egui::ColorImage {
        size: small,
        source_size: egui::Vec2::new(small[0] as f32, small[1] as f32),
        pixels,
    }
}

/// One box blur across and one down, each the mean of the `2 * radius + 1` pixels around a pixel, with
/// the edge pixel repeated past the edge.
fn box_blur(pixels: &mut [[f32; 4]], width: usize, height: usize, radius: usize) {
    let mut line = Vec::new();
    for y in 0..height {
        line.clear();
        line.extend((0..width).map(|x| pixels[y * width + x]));
        for x in 0..width {
            pixels[y * width + x] = mean_around(&line, x, radius);
        }
    }
    for x in 0..width {
        line.clear();
        line.extend((0..height).map(|y| pixels[y * width + x]));
        for y in 0..height {
            pixels[y * width + x] = mean_around(&line, y, radius);
        }
    }
}

fn mean_around(line: &[[f32; 4]], at: usize, radius: usize) -> [f32; 4] {
    let mut sum = [0.0f32; 4];
    let last = line.len() as isize - 1;
    for offset in -(radius as isize)..=radius as isize {
        let p = line[(at as isize + offset).clamp(0, last) as usize];
        for (total, value) in sum.iter_mut().zip(p) {
            *total += value;
        }
    }
    sum.map(|total| total / (2 * radius + 1) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Color32;

    fn picture(
        width: usize,
        height: usize,
        at: impl Fn(usize, usize) -> Color32,
    ) -> egui::ColorImage {
        let pixels =
            (0..height).flat_map(|y| (0..width).map(move |x| (x, y))).map(|(x, y)| at(x, y));
        egui::ColorImage {
            size: [width, height],
            source_size: egui::Vec2::new(width as f32, height as f32),
            pixels: pixels.collect(),
        }
    }

    #[test]
    fn a_flat_picture_comes_back_the_same_colour_at_a_quarter_of_the_size() {
        let grey = Color32::from_rgba_premultiplied(40, 50, 60, 200);
        let out = blurred(&picture(64, 32, |_, _| grey));
        assert_eq!(out.size, [16, 8]);
        assert!(out.pixels.iter().all(|p| *p == grey), "{:?}", out.pixels[0]);
    }

    #[test]
    fn a_hard_edge_becomes_a_gradient() {
        // White on the left half, black on the right: after the blur the columns either side of the edge
        // are neither, and they still get darker from left to right.
        let out =
            blurred(&picture(80, 8, |x, _| if x < 40 { Color32::WHITE } else { Color32::BLACK }));
        let row: Vec<u8> = (0..out.size[0]).map(|x| out.pixels[x].r()).collect();
        assert_eq!(row[0], 255);
        assert_eq!(*row.last().unwrap(), 0);
        assert!(row[9] > 0 && row[9] < 255, "the edge was not softened: {row:?}");
        assert!(row[10] > 0 && row[10] < 255, "the edge was not softened: {row:?}");
        assert!(row.windows(2).all(|pair| pair[0] >= pair[1]), "{row:?}");
    }

    #[test]
    fn only_a_screenshot_the_backdrop_asked_for_is_its_own() {
        assert!(is_ours(&egui::UserData::new(Marker)));
        assert!(!is_ours(&egui::UserData::default()));
    }
}
