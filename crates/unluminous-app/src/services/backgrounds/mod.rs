//! The pictures the window can be drawn on, and the folder Unluminous keeps them in.
//!
//! `task-2004` asks for a Background Selector beside `appearance.background.opacity`: a grid whose
//! first option is the current behaviour — the desktop showing through — and whose other options are
//! pictures chosen from disk, *"copied over to the app's appropriate content folder so it's
//! retained"*, each with a way to remove it, applied at once.
//!
//! **The folder is the whole of the state.** What the grid shows is what is in
//! `<the person's settings folder>/backgrounds/`, and `appearance.background.image` is a file name in
//! it. There is no list written down beside the files, so a picture somebody drops in by hand appears
//! and one they delete by hand goes, and nothing can get out of step with the disk.
//!
//! **A picture is copied in rather than pointed at.** A setting holding a path into somebody's Pictures
//! folder stops working the day they tidy it up, and it makes one person's settings file depend on
//! another person's disk — which is the same reason `services::file_marks` writes paths relative to the
//! project. The copy is named after the file it came from with a number added when that name is taken.
//!
//! **Removing one deletes the copy.** That is a file this application made, in a folder this
//! application owns, which is the one kind of deletion it does.
//!
//! **Nothing is fetched.** A picture is read from the disk somebody pointed at, or out of the binary
//! itself, and from nowhere else — which is the rule the Markdown preview already keeps.
//!
//! **Five pictures ship inside the binary** and are written into the folder the first time it is made,
//! so a fresh Unluminous has something to choose from rather than an empty grid and a file dialog.
//! [`bundled`] is that list and the reasoning; once written they are ordinary files, so removing one
//! removes it for good.

pub mod bundled;
pub mod thumbnails;

use std::path::{Path, PathBuf};

/// The backgrounds folder inside a settings folder, made when something is first put in it.
///
/// Beside `settings.conf` and `recent.txt`, because a background is a choice about a person's window
/// rather than about a project — the line `task-1697` drew for where the panels are, and the same one
/// `appearance.theme` keeps.
///
/// **There is no function here that answers with the person's own folder, and that is deliberate.**
/// Every question this module answers takes the folder it is about, and the window asks with the folder
/// of the `Store` it was given — `UnluminousApp::backgrounds_folder`. `task-2105` is why: `folder()` used
/// to answer with the person's own settings folder and `path_of` was built on it, so a window a test had
/// pointed at a temporary store wrote the bundled pictures into that store and then *drew* whatever
/// picture of the same name was in the settings folder of whoever was running the tests. The suite
/// passed or failed depending on whether that machine had ever started a build that wrote
/// `forest-1.jpg` there. With no such function, a store's window cannot reach any folder but its
/// store's.
pub fn folder_in(settings: &Path) -> PathBuf {
    settings.join("backgrounds")
}

/// The extensions a picture may have, which is what `services::picture` can decode.
const PICTURES: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];

/// Whether this file is one of them, by its extension.
pub fn is_a_picture(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_lowercase())
        .is_some_and(|extension| PICTURES.contains(&extension.as_str()))
}

/// The pictures in the folder, by name, in the order a person reads them.
///
/// Sorted by name rather than by when they were added: a grid whose cells move about between openings
/// is a grid nobody can point at twice. A folder that is not there is no pictures, which is what a
/// fresh Unluminous has and is not a fault.
pub fn list_in(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_a_picture(path))
        .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

/// Copy a picture into the folder and answer with the name it was given.
///
/// The name is the file's own, with ` (2)`, ` (3)` and so on added while that name is taken — which is
/// what every file manager does and is the one shape nobody has to be told about. A file that is
/// already *in* the folder is left where it is and answered with its own name, so choosing one from the
/// grid's own folder does not make a second copy of it.
pub fn add_into(folder: &Path, source: &Path) -> Result<String, String> {
    if !source.is_file() {
        return Err(format!("There is no file at {}", source.display()));
    }
    if !is_a_picture(source) {
        return Err(format!(
            "{} is not a picture Unluminous can read. It reads {}.",
            source.display(),
            PICTURES.join(", ")
        ));
    }
    if source.parent() == Some(folder) {
        let name = source.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let _ = thumbnails::make(folder, &name);
        return Ok(name);
    }
    std::fs::create_dir_all(folder)
        .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
    let name = free_name(folder, source);
    std::fs::copy(source, folder.join(&name))
        .map_err(|problem| format!("{} could not be copied: {problem}", source.display()))?;
    // Its thumbnail now, so the grid has it the next time it opens (`task-2293`). A picture that cannot be
    // made small is still added; the grid draws its cell empty, as it does for one it cannot read.
    let _ = thumbnails::make(folder, &name);
    Ok(name)
}

/// The name a copy of `source` gets in `folder`, counting past whatever is already there.
fn free_name(folder: &Path, source: &Path) -> String {
    let stem = source.file_stem().map(|stem| stem.to_string_lossy().into_owned());
    let stem = stem.filter(|stem| !stem.is_empty()).unwrap_or_else(|| "background".to_owned());
    let extension = source
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let first = format!("{stem}{extension}");
    if !folder.join(&first).exists() {
        return first;
    }
    for number in 2..1000 {
        let tried = format!("{stem} ({number}){extension}");
        if !folder.join(&tried).exists() {
            return tried;
        }
    }
    first
}

/// Delete one of the copies.
///
/// **A name rather than a path**, and it is joined onto the folder here, so nothing outside the folder
/// can be reached by asking for one: a name with a separator in it, or one that climbs, is refused
/// before anything is opened. That is the same rule `services::browser`'s local root keeps about a page
/// asking for a file.
pub fn remove_from(folder: &Path, name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(format!("{name} is not the name of a background."));
    }
    let path = folder.join(name);
    if !path.is_file() {
        return Err(format!("There is no background called {name}."));
    }
    std::fs::remove_file(&path)
        .map_err(|problem| format!("{} could not be removed: {problem}", path.display()))?;
    thumbnails::forget(folder, name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join("unluminous-backgrounds").join(name);
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("make the folder");
        folder
    }

    /// A one pixel PNG, so the tests copy something a decoder would accept.
    fn a_picture(at: &Path) {
        let bytes: [u8; 67] = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(at, bytes).expect("write the picture");
    }

    #[test]
    fn a_picture_is_copied_in_and_the_second_of_a_name_is_numbered() {
        let holding = temporary("copy-holding");
        let from = temporary("copy-from");
        a_picture(&from.join("wallpaper.png"));
        let first = add_into(&holding, &from.join("wallpaper.png")).expect("copied");
        assert_eq!(first, "wallpaper.png");
        let second = add_into(&holding, &from.join("wallpaper.png")).expect("copied again");
        assert_eq!(second, "wallpaper (2).png", "the second of a name counts up");
        assert_eq!(list_in(&holding), vec!["wallpaper (2).png", "wallpaper.png"]);
    }

    #[test]
    fn a_picture_already_in_the_folder_is_not_copied_again() {
        let holding = temporary("copy-inside");
        a_picture(&holding.join("already.png"));
        let name = add_into(&holding, &holding.join("already.png")).expect("answered");
        assert_eq!(name, "already.png");
        assert_eq!(list_in(&holding).len(), 1, "and there is still only one of it");
    }

    #[test]
    fn a_file_that_is_not_a_picture_and_one_that_is_not_there_are_both_refused() {
        let holding = temporary("copy-refusals");
        let from = temporary("copy-refusals-from");
        std::fs::write(from.join("notes.txt"), "hello").expect("write");
        assert!(add_into(&holding, &from.join("notes.txt")).is_err());
        assert!(add_into(&holding, &from.join("nothing-here.png")).is_err());
        assert!(list_in(&holding).is_empty());
    }

    #[test]
    fn only_a_name_can_be_removed_and_only_one_that_is_there() {
        let holding = temporary("remove");
        a_picture(&holding.join("one.png"));
        assert!(remove_from(&holding, "two.png").is_err(), "one that is not there");
        assert!(remove_from(&holding, "../one.png").is_err(), "a name that climbs out");
        assert!(remove_from(&holding, "a/one.png").is_err(), "and one with a separator in it");
        assert!(remove_from(&holding, "one.png").is_ok());
        assert!(list_in(&holding).is_empty());
    }

    /// **Nothing in this module can name the person's own settings folder.** Every function takes the
    /// folder it is about, so a window pointed at a temporary store cannot read or write anybody's real
    /// pictures through here. `task-2105` is what it cost when one function could: the suite drew
    /// whatever wallpaper the machine running it had. The name is split so this test does not find
    /// itself.
    #[test]
    fn no_function_here_reaches_the_persons_settings_folder() {
        let source = [include_str!("mod.rs"), include_str!("bundled.rs")];
        let forbidden = ["folder_for_", "this_person"].concat();
        for text in source {
            assert!(
                !text.contains(&forbidden),
                "a background is being read from the person's folder"
            );
        }
    }

    #[test]
    fn a_folder_that_is_not_there_holds_no_pictures_and_that_is_not_a_fault() {
        let missing = std::env::temp_dir().join("unluminous-backgrounds").join("never-made");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(list_in(&missing).is_empty());
    }

    #[test]
    fn only_the_extensions_a_decoder_reads_are_offered() {
        assert!(is_a_picture(Path::new("a.PNG")), "the extension is read whatever its case");
        assert!(is_a_picture(Path::new("a.jpeg")));
        assert!(!is_a_picture(Path::new("a.svg")), "which nothing here decodes");
        assert!(!is_a_picture(Path::new("a")));
    }
}

/// The picture the window is drawn on, decoded once and kept.
///
/// **Keyed on the name and the file's own modified time**, so a picture replaced on disk is read again
/// and one nobody has touched costs a `metadata` call — and that call is made at most every
/// [`Wallpaper::CHECK`] rather than once a frame, which is `task-1805`'s rule about a number on the
/// screen not being a reason to ask the disk.
///
/// **It is put on the graphics card in strips, one a frame** (`task-2279`). The graphics driver keeps the
/// staging memory of a large upload for the rest of the process: one 8 MB picture sent in one piece left
/// 8 MB of private memory behind, measured on DX12, and the same picture sent as strips of about a
/// megabyte, each in a frame of its own, left about one. Strips sent in the same frame are one submission
/// and are kept together, which is why it is one a frame. The picture showing stays until the new one is
/// complete.
#[derive(Default)]
pub struct Wallpaper {
    name: String,
    /// The file the texture was decoded from, so a window given a different store reads again.
    path: PathBuf,
    stamp: Option<std::time::SystemTime>,
    picture: Option<Picture>,
    /// A picture still being uploaded, the row its next strip starts at, and the strips already sent.
    arriving: Option<(egui::ColorImage, usize, Vec<Strip>)>,
    looked: Option<std::time::Instant>,
}

/// Where the count of background picture changes is kept in a window's `egui` memory.
fn changes_slot() -> egui::Id {
    egui::Id::new("unluminous-background-changes")
}

/// How many times the picture the window is drawn on has changed: arrived, been replaced or been taken
/// away. The modal backdrop is a photograph of the window, and takes a new one when this moves.
///
/// @param ctx - the window's context
pub fn changes(ctx: &egui::Context) -> u64 {
    ctx.data(|data| data.get_temp(changes_slot())).unwrap_or(0)
}

/// Count one change of the picture the window is drawn on.
///
/// @param ctx - the window's context
fn note_a_change(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        let count: &mut u64 = data.get_temp_mut_or_default(changes_slot());
        *count += 1;
    });
}

/// A picture on the graphics card, as strips one above another.
pub struct Picture {
    /// Its size in pixels.
    size: egui::Vec2,
    strips: Vec<Strip>,
    /// The same picture shrunk and blurred, which a modal is drawn over under glass (`task-2281`).
    frosted: egui::TextureHandle,
}

/// One strip of a [`Picture`]: the rows it is drawn for, and the rows its texture holds, which are one
/// more above and below so a linear filter at a seam reads the neighbouring row rather than repeating
/// its own.
struct Strip {
    texture: egui::TextureHandle,
    drawn: std::ops::Range<usize>,
    held: std::ops::Range<usize>,
}

/// How many bytes one strip holds at most. Uploads of 2 MB or less left nothing behind in the
/// measurement, so a megabyte has room to spare.
const STRIP_BYTES: usize = 1 << 20;

impl Picture {
    /// Its size in pixels.
    pub fn size_vec2(&self) -> egui::Vec2 {
        self.size
    }

    /// How many strips it was uploaded as.
    pub fn strips(&self) -> usize {
        self.strips.len()
    }

    /// The frosted copy of the picture: shrunk an eighth and blurred, one small texture. See
    /// `backdrop_blur::frosted`.
    pub fn frosted(&self) -> egui::TextureId {
        self.frosted.id()
    }

    /// The strips as the bands glass bends (`task-2281`): each strip's texture, the rows it is drawn for
    /// and the rows it holds, as fractions of the whole picture's height.
    pub fn bands(&self) -> Vec<rux::render::Band> {
        let rows = self.size.y.max(1.0);
        let share = |row: usize| row as f32 / rows;
        self.strips
            .iter()
            .map(|strip| rux::render::Band {
                texture: strip.texture.id(),
                drawn: (share(strip.drawn.start), share(strip.drawn.end)),
                held: (share(strip.held.start), share(strip.held.end)),
            })
            .collect()
    }

    /// Paint the part `uv` of the picture over `rect`, with its corners rounded by `corners`.
    ///
    /// The rounded rectangle is tessellated once, as egui would tessellate it with the picture as its
    /// brush, and its triangles are then cut at each seam between strips, so the corners and their
    /// feathered edge come out exactly as they did when the picture was one texture.
    ///
    /// @param painter - what to paint with
    /// @param rect - where the picture goes on the screen
    /// @param corners - how round the corners are
    /// @param uv - which part of the picture, from 0 to 1 across the whole of it
    pub fn paint(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        corners: egui::CornerRadius,
        uv: egui::Rect,
    ) {
        let shape = egui::epaint::RectShape::filled(rect, corners, egui::Color32::WHITE)
            .with_texture(egui::TextureId::default(), uv);
        let mut whole = egui::Mesh::default();
        let mut tessellator = egui::epaint::Tessellator::new(
            painter.ctx().pixels_per_point(),
            egui::epaint::TessellationOptions::default(),
            [1, 1],
            Vec::new(),
        );
        tessellator.tessellate_rect(&shape, &mut whole);
        let rows = self.size.y.max(1.0);
        for strip in &self.strips {
            let (top, bottom) = (strip.drawn.start as f32 / rows, strip.drawn.end as f32 / rows);
            if bottom <= uv.min.y || top >= uv.max.y {
                continue;
            }
            let mut piece = cut_between(&whole, top, bottom);
            let (from, span) =
                (strip.held.start as f32, (strip.held.end - strip.held.start) as f32);
            for vertex in &mut piece.vertices {
                vertex.uv.y = (vertex.uv.y * rows - from) / span;
            }
            piece.texture_id = strip.texture.id();
            painter.add(egui::Shape::mesh(piece));
        }
    }
}

/// The triangles of `mesh` cut to the band of texture rows between `top` and `bottom`, as fractions of
/// the whole picture.
///
/// Each triangle is clipped against the two lines and what is left is a polygon of up to five corners,
/// fanned back into triangles. Position, texture coordinate and colour all change in a straight line
/// across a triangle, so the point where an edge meets a line is found by the same fraction for all
/// three.
///
/// @param mesh - the whole rounded rectangle
/// @param top - the band's top, as a fraction of the picture's height
/// @param bottom - the band's bottom
fn cut_between(mesh: &egui::Mesh, top: f32, bottom: f32) -> egui::Mesh {
    let mut out = egui::Mesh::default();
    for triangle in mesh.indices.chunks_exact(3) {
        let corners: Vec<egui::epaint::Vertex> =
            triangle.iter().map(|&i| mesh.vertices[i as usize]).collect();
        let kept = clip(&clip(&corners, top, true), bottom, false);
        if kept.len() < 3 {
            continue;
        }
        let first = out.vertices.len() as u32;
        out.vertices.extend(kept.iter().copied());
        for next in 1..kept.len() as u32 - 1 {
            out.indices.extend([first, first + next, first + next + 1]);
        }
    }
    out
}

/// One step of Sutherland and Hodgman's clipping: the polygon on one side of the line where the texture
/// coordinate's `y` is `at`.
///
/// @param polygon - the corners, in order
/// @param at - where the line is
/// @param below - true to keep what is below the line (larger `y`), false to keep what is above it
fn clip(polygon: &[egui::epaint::Vertex], at: f32, below: bool) -> Vec<egui::epaint::Vertex> {
    let inside = |v: &egui::epaint::Vertex| if below { v.uv.y >= at } else { v.uv.y <= at };
    let mut out = Vec::with_capacity(polygon.len() + 2);
    for (i, current) in polygon.iter().enumerate() {
        let previous = &polygon[(i + polygon.len() - 1) % polygon.len()];
        if inside(current) != inside(previous) {
            let t = (at - previous.uv.y) / (current.uv.y - previous.uv.y);
            out.push(egui::epaint::Vertex {
                pos: previous.pos + (current.pos - previous.pos) * t,
                uv: previous.uv + (current.uv - previous.uv) * t,
                color: previous.color.lerp_to_gamma(current.color, t),
            });
        }
        if inside(current) {
            out.push(*current);
        }
    }
    out
}

impl Wallpaper {
    /// How often the file behind the picture is asked whether it has changed.
    const CHECK: std::time::Duration = std::time::Duration::from_millis(1500);

    /// The picture for `name` in `folder`, decoding it when the name or the file has changed.
    ///
    /// An empty name is no picture, which is the setting saying the desktop shows through. A name that
    /// is not there, or a file that will not decode, is also no picture — a window drawn on nothing at
    /// all would be worse than the desktop, and it is what a picture somebody deleted by hand should do.
    /// **No folder is no picture too**: a window nobody has given a store to has no settings folder, and
    /// reaching for the person's own one instead is the fault `task-2105` removed.
    ///
    /// A picture taller than one strip is uploaded a strip a call, and the window is asked to draw again
    /// until the last one is there.
    ///
    /// @param ctx - the window's context, which the strips are uploaded through
    /// @param folder - the backgrounds folder, if the window has one
    /// @param name - the picture the setting names
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        folder: Option<&Path>,
        name: &str,
    ) -> Option<&Picture> {
        let Some(folder) = folder.filter(|_| !name.trim().is_empty()) else {
            if self.picture.is_some() {
                note_a_change(ctx);
            }
            self.forget();
            return None;
        };
        self.look_again(ctx, folder, name);
        self.send_a_strip(ctx);
        self.picture.as_ref()
    }

    /// Decode the picture again when its name, its folder or its file has changed.
    ///
    /// @param ctx - the window's context, for the largest texture the card takes
    /// @param folder - the backgrounds folder
    /// @param name - the picture the setting names
    fn look_again(&mut self, ctx: &egui::Context, folder: &Path, name: &str) {
        let due = self.looked.is_none_or(|at| at.elapsed() >= Self::CHECK);
        let path = folder.join(name);
        if name == self.name && self.path == path && !due {
            return;
        }
        let stamp = std::fs::metadata(&path).ok().and_then(|found| found.modified().ok());
        self.looked = Some(std::time::Instant::now());
        if name == self.name && stamp == self.stamp && self.path == path {
            return;
        }
        self.name = name.to_owned();
        self.path = path.clone();
        self.stamp = stamp;
        let limit = ctx.input(|input| input.max_texture_side);
        match crate::services::picture::decode(&path) {
            Ok(image) => {
                let image = shrink_to_cover(image, screen_pixels(ctx));
                self.arriving =
                    Some((crate::services::picture::shrink_to_fit(image, limit), 0, Vec::new()));
            }
            Err(_) => {
                if self.picture.take().is_some() {
                    note_a_change(ctx);
                }
                self.arriving = None;
            }
        }
    }

    /// Upload the next strip of a picture that is arriving, and show the picture once the last one is
    /// there.
    ///
    /// @param ctx - the window's context
    fn send_a_strip(&mut self, ctx: &egui::Context) {
        let Some((image, next, strips)) = self.arriving.as_mut() else { return };
        let [width, height] = image.size;
        let rows = (STRIP_BYTES / (width.max(1) * 4)).max(1);
        let drawn = *next..(*next + rows).min(height);
        let held = drawn.start.saturating_sub(1)..(drawn.end + 1).min(height);
        let pixels = image.pixels[held.start * width..held.end * width].to_vec();
        let piece = egui::ColorImage::new([width, held.end - held.start], pixels);
        // Linear, because a wallpaper is almost always drawn at a size other than its own and nearest
        // neighbour would make every edge in it ragged.
        let texture = ctx.load_texture(
            format!("unluminous-background-{}-{}", self.name, drawn.start),
            piece,
            egui::TextureOptions::LINEAR,
        );
        *next = drawn.end;
        strips.push(Strip { texture, drawn, held });
        if *next < height {
            ctx.request_repaint();
            return;
        }
        if let Some((image, _, strips)) = self.arriving.take() {
            let size = egui::vec2(image.size[0] as f32, image.size[1] as f32);
            // A small texture, uploaded in one piece: well under the two megabytes the driver lets go of.
            let frosted = ctx.load_texture(
                format!("unluminous-background-{}-frosted", self.name),
                crate::services::backdrop_blur::frosted(&image),
                egui::TextureOptions::LINEAR,
            );
            self.picture = Some(Picture { size, strips, frosted });
            note_a_change(ctx);
        }
    }

    /// Drop what is held, which is what choosing the desktop means.
    fn forget(&mut self) {
        self.name.clear();
        self.path = PathBuf::new();
        self.stamp = None;
        self.picture = None;
        self.arriving = None;
        self.looked = None;
    }
}

/// How many pixels the screen the window is on has, across and down, or the window's own size where the
/// screen is not known (a test's window has no monitor).
///
/// @param ctx - the window's context
fn screen_pixels(ctx: &egui::Context) -> egui::Vec2 {
    let (monitor, window, pixels_per_point) = ctx.input(|input| {
        (input.viewport().monitor_size, input.content_rect().size(), input.pixels_per_point)
    });
    monitor.filter(|size| size.x > 0.0 && size.y > 0.0).unwrap_or(window) * pixels_per_point
}

/// `image` made no larger than it needs to be to cover a screen of `screen` pixels (`task-2291`).
///
/// The bundled pictures are 3840 by 2160, so a 4K screen gets every pixel of them. On a smaller screen
/// the picture is shrunk to the size it will be drawn at, once, when it is chosen: a 4K picture is a 33 MB
/// texture, and on a 1920 by 1080 screen three quarters of it would never be seen. A picture is only
/// shrunk when that saves something worth the time, a fifth of its size or more.
///
/// @param image - the decoded picture
/// @param screen - the screen's size in pixels
pub fn shrink_to_cover(image: egui::ColorImage, screen: egui::Vec2) -> egui::ColorImage {
    let [width, height] = image.size;
    if width == 0 || height == 0 || screen.x < 1.0 || screen.y < 1.0 {
        return image;
    }
    let scale = (screen.x / width as f32).max(screen.y / height as f32);
    if scale > 0.8 {
        return image;
    }
    let longest = (width.max(height) as f32 * scale).ceil() as usize;
    crate::services::picture::shrink_to_fit(image, longest)
}

/// The part of a picture that fills `area` without distorting it, as texture coordinates.
///
/// **Scaled to cover rather than to fit**, which is what a desktop does with a wallpaper: a picture
/// that fitted would leave bands of nothing down two sides of the window, and the window has no colour
/// to put there — it is drawn on a transparent ground. What is cut is taken evenly off both sides, so
/// the middle of the picture is the middle of the window.
pub fn cover(picture: egui::Vec2, area: egui::Vec2) -> egui::Rect {
    if picture.x <= 0.0 || picture.y <= 0.0 || area.x <= 0.0 || area.y <= 0.0 {
        return egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    }
    let wanted = area.x / area.y;
    let has = picture.x / picture.y;
    let (across, down) = match has > wanted {
        // Wider than the window, so the sides are cut.
        true => (wanted / has, 1.0),
        false => (1.0, has / wanted),
    };
    egui::Rect::from_min_size(
        egui::pos2((1.0 - across) / 2.0, (1.0 - down) / 2.0),
        egui::vec2(across, down),
    )
}

#[cfg(test)]
mod covering {
    use super::*;

    /// The area of a mesh's triangles, which is how much of the screen it paints.
    fn area(mesh: &egui::Mesh) -> f32 {
        mesh.indices
            .chunks_exact(3)
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|i| mesh.vertices[t[i] as usize].pos);
                ((b - a).x * (c - a).y - (b - a).y * (c - a).x).abs() / 2.0
            })
            .sum()
    }

    /// **The strips together paint exactly what the one rectangle painted** (`task-2279`): the rounded
    /// rectangle is cut at every seam, and the pieces add up to the whole with nothing lost or painted
    /// twice, and every piece stays inside its own band of the picture.
    #[test]
    fn the_pieces_cut_at_the_seams_add_up_to_the_whole_rounded_rectangle() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(400.0, 300.0));
        let shape = egui::epaint::RectShape::filled(
            rect,
            egui::CornerRadius::same(14),
            egui::Color32::WHITE,
        )
        .with_texture(
            egui::TextureId::default(),
            egui::Rect::from_min_max(egui::pos2(0.1, 0.05), egui::pos2(0.9, 0.95)),
        );
        let mut whole = egui::Mesh::default();
        egui::epaint::Tessellator::new(1.0, Default::default(), [1, 1], Vec::new())
            .tessellate_rect(&shape, &mut whole);
        let seams = [0.0, 0.03, 0.2, 0.5, 0.51, 0.97, 1.0];
        let mut total = 0.0;
        for band in seams.windows(2) {
            let piece = cut_between(&whole, band[0], band[1]);
            assert!(piece
                .vertices
                .iter()
                .all(|v| v.uv.y >= band[0] - 1e-5 && v.uv.y <= band[1] + 1e-5));
            total += area(&piece);
        }
        assert!(
            (total - area(&whole)).abs() < area(&whole) * 1e-5,
            "pieces {total} against the whole {}",
            area(&whole)
        );
    }

    #[test]
    fn a_picture_wider_than_the_window_is_cut_down_its_sides_and_evenly() {
        let taken = cover(egui::vec2(4000.0, 1000.0), egui::vec2(1000.0, 1000.0));
        assert!((taken.width() - 0.25).abs() < 0.001, "a quarter of its width fills a square");
        assert_eq!(taken.height(), 1.0, "and all of its height");
        assert!((taken.center().x - 0.5).abs() < 0.001, "the middle of the picture is the middle");
    }

    #[test]
    fn a_picture_taller_than_the_window_is_cut_top_and_bottom() {
        let taken = cover(egui::vec2(1000.0, 4000.0), egui::vec2(1000.0, 1000.0));
        assert_eq!(taken.width(), 1.0);
        assert!((taken.height() - 0.25).abs() < 0.001);
        assert!((taken.center().y - 0.5).abs() < 0.001);
    }

    #[test]
    fn a_picture_the_shape_of_the_window_is_not_cut_at_all() {
        let taken = cover(egui::vec2(1600.0, 900.0), egui::vec2(800.0, 450.0));
        assert!((taken.width() - 1.0).abs() < 0.001);
        assert!((taken.height() - 1.0).abs() < 0.001);
    }

    #[test]
    fn a_4k_picture_is_shrunk_to_cover_a_smaller_screen_and_kept_whole_on_a_4k_one() {
        let picture = egui::ColorImage::filled([3840, 2160], egui::Color32::WHITE);
        let small = shrink_to_cover(picture.clone(), egui::vec2(1920.0, 1080.0));
        assert_eq!(small.size, [1920, 1080]);
        // A tall window on a narrow screen still covers it down its whole height.
        let tall = shrink_to_cover(picture.clone(), egui::vec2(1000.0, 1600.0));
        assert!(tall.size[1] >= 1600 && tall.size[0] >= 1000, "{:?}", tall.size);
        let whole = shrink_to_cover(picture.clone(), egui::vec2(3840.0, 2160.0));
        assert_eq!(whole.size, [3840, 2160]);
        let unknown = shrink_to_cover(picture, egui::Vec2::ZERO);
        assert_eq!(unknown.size, [3840, 2160]);
    }

    #[test]
    fn a_picture_with_no_size_asks_for_the_whole_of_itself_rather_than_dividing_by_nothing() {
        let taken = cover(egui::vec2(0.0, 0.0), egui::vec2(800.0, 600.0));
        assert_eq!(taken, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)));
    }
}
