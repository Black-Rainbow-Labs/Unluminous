//! Real fonts behind the editor's measurements, and rasterised glyphs for painting.
//!
//! `unluminous-core` measures text through the [`unluminous_core::FontMetrics`] trait and never asks how a glyph
//! is drawn. This module is the one implementation of that trait that uses real font files. It finds
//! installed families with `fontdb`, reads and rasterises them with `ab_glyph`, and keeps the resulting
//! pixels in one texture that the editor paints from.
//!
//! Keeping every glyph in one texture and drawing each one as a textured rectangle is the approach
//! glyphon uses (<https://github.com/grovesNL/glyphon>, commit 49dc8f7b). Drawing one rectangle per
//! glyph out of one texture means the whole visible document is a single mesh.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use ab_glyph::{Font as _, FontVec, Glyph, PxScale, ScaleFont as _};
use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use unluminous_core::{CharStyle, FontMetrics, LineMetrics};

/// Families Unluminous offers in the toolbar. Only the ones the operating system actually has are shown, so
/// the list is right on macOS and on Windows without asking which one we are on.
const CANDIDATE_FAMILIES: &[&str] = &[
    "Helvetica",
    "Arial",
    "Times New Roman",
    "Georgia",
    "Verdana",
    "Courier New",
    "Courier",
    "Menlo",
    "Consolas",
    "Segoe UI",
];

/// Families to look in for a character the chosen family has no shape for.
///
/// A terminal needs this more than a document does. `claude` and `codex` draw with box drawing characters,
/// arrows, ticks and spinners, and a text face such as Helvetica or a monospaced one such as Menlo does not
/// have all of them. Without somewhere else to look, a missing character comes out as the empty box a font
/// puts at glyph zero, which is what every terminal that has no fallback shows and what no terminal should.
///
/// The list is tried in order and the ones this system does not have are skipped, so it can hold the usual
/// families of both platforms.
const FALLBACK_FAMILIES: &[&str] = &[
    // macOS.
    "Menlo",
    "Apple Symbols",
    "Arial Unicode MS",
    // The media symbols a program uses for pause and play, which the text faces do not carry. macOS itself
    // draws these from Apple Color Emoji, which is a colour bitmap font and has no outline to rasterise, so
    // the maths face is used instead and they come out in the text colour.
    "STIX Two Math",
    "STIXGeneral",
    "Hiragino Sans",
    "PingFang SC",
    "Zapf Dingbats",
    // Windows.
    "Segoe UI Symbol",
    "Segoe UI Emoji",
    "Cambria Math",
    "MS Gothic",
    // Anywhere.
    "DejaVu Sans",
    "Noto Sans Symbols 2",
];

/// Extra space between one line and the next, as a fraction of the point size.
///
/// A font's own line height sets the lines as close together as the shapes allow, which is tiring to read
/// at length, so every editor adds some. The design's lines sit about half the point size further
/// apart than Helvetica's own metrics ask for, and this is that extra. It is asked for here rather than in
/// `unluminous-core` so that the layout arithmetic and its tests stay exact and platform independent.
const READING_LEADING: f32 = 0.45;

/// The atlas is one texture of this many pixels on each side. A page of text at ordinary sizes needs a
/// few hundred distinct glyphs, so this holds far more than one screen.
const ATLAS_SIDE: usize = 1024;

/// Which face of which family, which is what a font file gives us.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FaceKey {
    family: String,
    bold: bool,
    italic: bool,
}

impl FaceKey {
    fn of(style: &CharStyle) -> Self {
        Self { family: style.family.to_string(), bold: style.bold, italic: style.italic }
    }

    /// Whether this is the key a style would build, without building it.
    ///
    /// `of` allocates, because the key owns the family name. Measuring a letter and drawing a letter
    /// each asked for a key, so laying out a file allocated and threw away a `String` per grapheme —
    /// 167 ns a call, times a hundred and sixteen thousand. The memo in [`TextRenderer`] compares
    /// with this instead.
    fn is(&self, style: &CharStyle) -> bool {
        self.bold == style.bold && self.italic == style.italic && self.family == *style.family
    }
}

/// The size of one cell of the terminal grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellMetrics {
    /// How wide one cell is, which is one character's advance in a monospaced family.
    pub width: f32,
    /// How tall one cell is, from the top of one line to the top of the next.
    pub height: f32,
    /// How far below the top of the cell the baseline sits.
    pub ascent: f32,
}

/// One rasterised glyph in the atlas.
#[derive(Debug, Clone, Copy)]
pub struct AtlasGlyph {
    /// Where the pixels are in the texture, as a fraction of the texture size.
    pub uv: egui::Rect,
    /// How large to draw it, in points.
    pub size: egui::Vec2,
    /// Where its top left corner goes relative to the pen position on the baseline.
    pub offset: egui::Vec2,
}

/// How many pixels one point is worth where the text being drawn will be composited, and where those
/// pixels are.
///
/// **A node on the Realm and a zoomed modal are drawn through a layer transform.** `epaint` applies that
/// transform to the **finished shape**: `epaint::shapes::text_shape::transform` scales the vertices of an
/// already-rasterised mesh and leaves `pixels_per_point` alone, so a glyph rasterised at 12 points and
/// composited at 200% is a bitmap magnified two-to-one. `task-1907` reports it as *"the text in the nodes
/// looks pixelated when I zoom in"*.
///
/// The atlas has no such limit — it is keyed on the size it was asked for — so what it is asked for is the
/// size the glyph is **seen** at, and the quad the glyph is drawn into is divided by the same number. The
/// layout is untouched, which is what keeps `task-1904`'s promise that a zoom costs a matrix and not a
/// relayout.
///
/// ## Exactly the size it is seen at, once the camera is still (`task-2216`)
///
/// *"Our panels like agent tasks, agent chat, realm, etc show pixelated font at certain zoom levels."* Until
/// `task-2216` the raster size was always rounded **up** to a quarter step, so a camera at 1.1 rasterised at
/// 1.25 and drew every glyph at 0.88 of its bitmap. A glyph resampled by 0.88 is not a glyph drawn one
/// texel to one pixel, and through this atlas's nearest filter the dropped columns made the letters uneven;
/// through `egui`'s linear one they were soft. The quarter steps were there to keep a pinch from putting a
/// new size into the atlas on every frame, which is right **while the camera moves** and wrong once it has
/// stopped. So there are two answers now, and [`Crispness::through`] picks between them:
///
/// - **settled**: the raster size is the zoom itself, so a glyph is drawn exactly as large as it was
///   rasterised. One new set of sizes per place the camera comes to rest.
/// - **moving**: the quarter step above the zoom, as before, so a glide asks the atlas for a handful of sizes.
///
/// ## And on a whole pixel of the window, not of the layer
///
/// A glyph drawn one texel to one pixel is only sharp when its corner lands on a pixel boundary. Rounding
/// in the layer's own points is not that: the layer is translated by the camera's position, which is any
/// number at all, so [`Crispness::snap`] rounds in the window's own pixels and maps the answer back.
///
/// ## And the display's own pixels
///
/// The scale includes `pixels_per_point`, which this renderer never asked for before `task-2216`: on a
/// display at 2.0 a 16 point glyph was rasterised with 16 pixels and drawn into 32. `TextRenderer` is told
/// the display's density once a frame by [`TextRenderer::follow_the_display`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crispness {
    /// How many pixels one layout point is rasterised with.
    scale: f32,
    /// How many window points one layout point becomes, which is the layer transform's scaling.
    zoom: f32,
    /// Where the layer's origin is in the window, which is the layer transform's translation.
    origin: egui::Vec2,
    /// How many pixels one window point is.
    pixels_per_point: f32,
}

/// Where a renderer's drawing is going: the layer transform, and whether that transform is still moving.
///
/// What [`TextRenderer::composite_through`] answers with and [`TextRenderer::restore_compositing`] takes
/// back, so a node cannot leave the canvas's transform on for the pane drawn after it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Compositing {
    transform: egui::emath::TSTransform,
    settled: bool,
}

impl Compositing {
    /// Drawing that is not going through any transform, which is everything outside a node and a modal.
    pub const DIRECT: Self = Self { transform: egui::emath::TSTransform::IDENTITY, settled: true };
}

/// The steps the scale is snapped to while the camera moves.
///
/// **The atlas is one texture that is cleared and started again when it fills**, so a distinct raster size on
/// every frame of a pinch would clear it repeatedly and cost far more than the blur it was fixing. Quarter
/// steps keep the whole camera range from 0.25 to 2.5 to **ten** sizes rather than an unbounded number, and a
/// glide settles onto one of them until it stops.
const RASTER_STEPS: f32 = 4.0;

/// The smallest scale a glyph is rasterised at, which is the camera's own smallest zoom.
///
/// See `services::realm::node::MIN_ZOOM`. Kept as a number here rather than imported, because what it
/// bounds is the atlas rather than the camera: a scale below this would ask for a glyph of a couple of
/// pixels, and the two would have to be changed together anyway.
const SMALLEST_RASTER: f32 = 0.25;

/// The largest, which is the camera's own largest zoom.
///
/// Here for the same reason as [`SMALLEST_RASTER`]: it is what bounds the ladder, and a caller that
/// asked for a bigger scale than the camera can reach would otherwise put an unbounded number of sizes
/// into the atlas. A modal zooms to 3, so it reaches past this and is drawn magnified above 2.5.
const MAX_RASTER: f32 = 3.0;

impl Crispness {
    /// Text composited at the size it is laid out at, on a display of one pixel a point.
    pub const EXACT: Self =
        Self { scale: 1.0, zoom: 1.0, origin: egui::Vec2::ZERO, pixels_per_point: 1.0 };

    /// Text composited through a layer transform of `scale` **while it is moving**: the quarter step above.
    pub fn at(scale: f32) -> Self {
        Self { scale: Self::ladder(scale, false), zoom: scale, ..Self::EXACT }
    }

    /// Text composited through a layer transform of `scale` that has stopped: exactly that size.
    pub fn exactly(scale: f32) -> Self {
        Self { scale: Self::ladder(scale, true), zoom: scale, ..Self::EXACT }
    }

    /// Text composited the way `compositing` says, on a display of `pixels_per_point`.
    ///
    /// Exactly the zoom when the transform has settled and the quarter step above it while it moves. See
    /// the type's comment for why the two answers differ.
    pub fn through(compositing: Compositing, pixels_per_point: f32) -> Self {
        let zoom = compositing.transform.scaling;
        Self {
            scale: Self::ladder(zoom, compositing.settled) * pixels_per_point,
            zoom,
            origin: compositing.transform.translation,
            pixels_per_point,
        }
    }

    /// How many times larger to lay text out than its own size, for text composited at `zoom`.
    ///
    /// The one place the ladder is decided, so `theme::crisp`, which does the same job for the text `egui`
    /// lays out, asks the atlas for the same sizes this renderer does. It leaves out `pixels_per_point`,
    /// because `egui` multiplies by that itself.
    pub fn ladder(zoom: f32, settled: bool) -> f32 {
        // **Rounded up, never down, while moving.** The canvas zooms in steps of 1.1, so the very first step
        // a person takes is 1.1 — which rounding to the nearest quarter sends back to 1.0, magnifying the
        // glyphs exactly as before `task-1907`. Rounding up means a glyph is rasterised at **at least** the size
        // it is composited at, so the transform only ever scales it down. The Codex Sol review of `task-1907`
        // found that. And it goes below 1.0, which `task-1907` did not: a canvas zoomed out to 0.61 asks for
        // 0.75, still more pixels than the glyph is composited into. `task-1945`.
        let wanted = match settled {
            true => zoom,
            false => (zoom * RASTER_STEPS).ceil() / RASTER_STEPS,
        };
        wanted.clamp(SMALLEST_RASTER, MAX_RASTER)
    }

    /// Whether this is the ordinary case, where a glyph is rasterised at its own size in pixels.
    pub fn is_exact(self) -> bool {
        self.scale == 1.0
    }

    /// The size to rasterise a glyph at, for text laid out at `points`.
    pub fn raster_size(self, points: f32) -> f32 {
        points * self.scale
    }

    /// Put a rasterised glyph back into the rectangle the layout gave it.
    ///
    /// The glyph was rasterised `scale` times too large on purpose, so its size and its offset are divided by
    /// `scale` and it lands exactly where a glyph rasterised at the layout's own size would have.
    pub fn drawn(self, glyph: AtlasGlyph) -> AtlasGlyph {
        if self.is_exact() {
            return glyph;
        }
        AtlasGlyph {
            uv: glyph.uv,
            size: glyph.size / self.scale,
            offset: glyph.offset / self.scale,
        }
    }

    /// Snap a position to a whole pixel **of the window**.
    ///
    /// Both painters round a glyph's position, because *"a glyph is drawn at exactly the size it was
    /// rasterised at, so landing it on a fraction of a pixel would resample it and soften every letter"*. A
    /// layer's point is not a window's pixel: the layer is scaled by the zoom and moved by the camera's
    /// position, and both are any number at all. So `at` is carried into the window's pixels, rounded there,
    /// and carried back.
    pub fn snap(self, at: egui::Pos2) -> egui::Pos2 {
        let into_pixels = |value: f32, origin: f32| {
            let pixel = ((origin + value * self.zoom) * self.pixels_per_point).round();
            (pixel / self.pixels_per_point - origin) / self.zoom
        };
        egui::Pos2::new(into_pixels(at.x, self.origin.x), into_pixels(at.y, self.origin.y))
    }
}

/// A glyph at one size, which is what the atlas is keyed by. The size is held in sixty-fourths of a pixel so
/// that the key can be hashed.
///
/// **Sixty-fourths rather than the quarters it was before `task-2216`**, because a canvas that has stopped
/// moving rasterises at exactly its zoom, and two sizes a quarter apart sharing one entry would hand one of
/// them a glyph a few percent the wrong size, which is a glyph that is resampled rather than drawn one texel
/// to one pixel.
///
/// The face is a small number rather than a family name and two flags, because this key is built and
/// hashed once for every character on the screen every frame, and hashing a `String` there meant
/// allocating one first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    face: FaceId,
    character: char,
    sixty_fourths: u32,
}

/// Which face, as a number. Handed out in the order the faces are first asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FaceId(u32);

/// The last face resolved: which style it answers to, its small id, and the loaded face itself, if
/// the system has one.
type FaceMemo = Option<(FaceKey, FaceId, Option<Arc<FontVec>>)>;

/// How wide each of the 128 ASCII characters is, for one face at one size. `NaN` is not measured yet.
type AsciiAdvances = [f32; 128];

/// The ASCII advances measured so far, one table for each face and size that has been laid out.
///
/// **`task-2218`.** Laying a 200 KB file out cost about 20 ms, and nearly all of it was
/// [`FontMetrics::advance`] being asked one character at a time: resolve the face, look the character up
/// in the font's `cmap` twice (once to see whether a fallback is needed, once for the advance), and scale
/// the advance. The answer for a character never changes for a face at a size, and source code is almost
/// all ASCII, so the answer is kept. A zoom walks a dozen sizes, so the tables are few; `ADVANCE_TABLES`
/// bounds them anyway.
///
/// A table is measured whole when it is made, for the printable characters and the tab, so layout can be
/// handed it once for a run ([`FontMetrics::ascii_advances`]). The control characters stay `NaN` and are
/// measured one at a time when asked, because measuring one can mean searching the fallback families,
/// which reads fonts from the disk for characters a file almost never holds.
#[derive(Default)]
struct AdvanceTables {
    /// The face and the size bits of each table, in the order they were made.
    keys: Vec<(FaceId, u32)>,
    tables: Vec<Arc<AsciiAdvances>>,
    /// Which table answered last, because a layout walks a run of one style at a time.
    last: usize,
}

/// How many face and size pairs keep an ASCII table before they are all forgotten and measured again.
const ADVANCE_TABLES: usize = 64;

impl AdvanceTables {
    /// The table for a face at a size, if one has been made.
    fn find(&mut self, face: FaceId, size: f32) -> Option<Arc<AsciiAdvances>> {
        let key = (face, size.to_bits());
        if self.keys.get(self.last) != Some(&key) {
            self.last = self.keys.iter().position(|known| *known == key)?;
        }
        Some(Arc::clone(&self.tables[self.last]))
    }

    /// Keep a table that has just been measured.
    fn keep(&mut self, face: FaceId, size: f32, table: Arc<AsciiAdvances>) {
        if self.keys.len() >= ADVANCE_TABLES {
            self.keys.clear();
            self.tables.clear();
        }
        self.keys.push((face, size.to_bits()));
        self.tables.push(table);
        self.last = self.keys.len() - 1;
    }
}

struct Atlas {
    image: ColorImage,
    texture: Option<TextureHandle>,
    entries: HashMap<GlyphKey, Option<AtlasGlyph>>,
    /// Where the next glyph goes.
    pen_x: usize,
    pen_y: usize,
    row_height: usize,
    /// True when the whole texture needs uploading again: it has never been, or the atlas was cleared.
    changed: bool,
    /// The part of the image written since the texture was last uploaded, as left, top, right and bottom
    /// in pixels, when only a part was.
    ///
    /// **Uploaded on its own rather than with the rest of the atlas** (`task-2218`). Every new glyph used
    /// to clone the whole four megabyte image and send all of it to the graphics card, and a zoom on the
    /// canvas adds glyphs at a new size on nearly every frame of the glide.
    dirty: Option<[usize; 4]>,
    /// Bumped whenever the atlas is cleared, so a caller holding positions from it can tell they are
    /// no longer valid.
    generation: u64,
}

impl Atlas {
    fn new() -> Self {
        Self {
            image: ColorImage::filled([ATLAS_SIDE, ATLAS_SIDE], Color32::TRANSPARENT),
            texture: None,
            entries: HashMap::new(),
            pen_x: 0,
            pen_y: 0,
            row_height: 0,
            changed: true,
            dirty: None,
            generation: 0,
        }
    }

    /// Note that a rectangle of the image has been written, for the next upload to send.
    fn mark(&mut self, x: usize, y: usize, width: usize, height: usize) {
        if self.changed {
            return;
        }
        let (right, bottom) = ((x + width).min(ATLAS_SIDE), (y + height).min(ATLAS_SIDE));
        self.dirty = Some(match self.dirty {
            Some([left, top, was_right, was_bottom]) => {
                [left.min(x), top.min(y), was_right.max(right), was_bottom.max(bottom)]
            }
            None => [x, y, right, bottom],
        });
    }

    /// Reserve a rectangle of the atlas, moving to a new row when the current one is full.
    ///
    /// Returns `None` when the atlas is full. The caller then clears it and tries again, which loses
    /// the cache for one frame and is far better than failing to draw.
    fn reserve(&mut self, width: usize, height: usize) -> Option<(usize, usize)> {
        if width > ATLAS_SIDE || height > ATLAS_SIDE {
            return None;
        }
        if self.pen_x + width > ATLAS_SIDE {
            self.pen_x = 0;
            self.pen_y += self.row_height + 1;
            self.row_height = 0;
        }
        if self.pen_y + height > ATLAS_SIDE {
            return None;
        }
        let at = (self.pen_x, self.pen_y);
        self.pen_x += width + 1;
        self.row_height = self.row_height.max(height);
        Some(at)
    }

    fn clear(&mut self) {
        self.image = ColorImage::filled([ATLAS_SIDE, ATLAS_SIDE], Color32::TRANSPARENT);
        self.entries.clear();
        self.pen_x = 0;
        self.pen_y = 0;
        self.row_height = 0;
        self.changed = true;
        self.dirty = None;
        self.generation += 1;
    }
}

/// Fonts, measurements and rasterised glyphs.
pub struct TextRenderer {
    database: fontdb::Database,
    /// Families that are installed, in the order the toolbar shows them.
    families: Vec<String>,
    /// Faces already read from disk. An entry holding `None` is a face this system does not have, kept
    /// so that we do not search the database for it again on every frame.
    faces: RefCell<HashMap<FaceKey, Option<Arc<FontVec>>>>,
    /// Which family a character was found in when the chosen one had no shape for it, so the search runs
    /// once for each character rather than on every measurement.
    fallbacks: RefCell<HashMap<(char, bool, bool), Option<String>>>,
    /// A number for each face that has been asked for, so that the atlas can be keyed on something
    /// small. The face itself is `faces`; this is only the naming.
    ids: RefCell<HashMap<FaceKey, FaceId>>,
    /// The last face resolved, and the key it answers to.
    ///
    /// Layout and painting both walk run by run, and every character of a run has the same style, so
    /// one entry answers nearly every question. It is compared with [`FaceKey::is`], which looks at
    /// the family name rather than copying it.
    memo: RefCell<FaceMemo>,
    /// What each ASCII character measures, kept per face and size. See [`AdvanceTables`].
    advances: RefCell<AdvanceTables>,
    /// The last line metrics asked for, and the face and size they belong to. A layout asks once for
    /// every run of every line, and nearly every run in a file is in the same face at the same size.
    last_metrics: std::cell::Cell<Option<(FaceId, u32, LineMetrics)>>,
    atlas: RefCell<Atlas>,
    /// How many pixels a point is worth where whatever is being drawn right now will be composited.
    ///
    /// **Ambient rather than an argument, and that is a decision worth reading before changing it.**
    /// `paint_text` has five callers and the terminal grid has four, and threading a number through all nine
    /// to be 1.0 in eight of them is nine chances to pass the wrong one. It is also genuinely a property of
    /// *where the drawing is going* rather than of what is being drawn, which is what makes a node's contents
    /// different from the same file drawn in a pane.
    ///
    /// Set with [`TextRenderer::composite_through`] and put back with [`TextRenderer::restore_compositing`] — so a
    /// node cannot leave the canvas's zoom on for the pane drawn after it, which would be the one way this
    /// could go silently wrong. See [`Crispness`].
    compositing: RefCell<Compositing>,
    /// How many pixels one window point is on the display this window is on, from
    /// [`TextRenderer::follow_the_display`]. One until the window says otherwise, which is what every test
    /// draws at.
    pixels_per_point: std::cell::Cell<f32>,
}

impl TextRenderer {
    pub fn new() -> Self {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        let installed: Vec<String> = CANDIDATE_FAMILIES
            .iter()
            .filter(|family| {
                database
                    .query(&fontdb::Query {
                        families: &[fontdb::Family::Name(family)],
                        ..Default::default()
                    })
                    .is_some()
            })
            .map(|family| (*family).to_owned())
            .collect();
        Self {
            database,
            families: installed,
            faces: RefCell::new(HashMap::new()),
            fallbacks: RefCell::new(HashMap::new()),
            ids: RefCell::new(HashMap::new()),
            memo: RefCell::new(None),
            advances: RefCell::new(AdvanceTables::default()),
            last_metrics: std::cell::Cell::new(None),
            atlas: RefCell::new(Atlas::new()),
            compositing: RefCell::new(Compositing::DIRECT),
            pixels_per_point: std::cell::Cell::new(1.0),
        }
    }

    /// The families the toolbar offers.
    pub fn families(&self) -> &[String] {
        &self.families
    }

    /// A monospaced family this system has, for setting code in the Markdown preview. `None` when it has
    /// none of them, in which case code is set in the ordinary family.
    /// The order is this list's own rather than the order the families are offered in, because for a
    /// terminal and for code the choice matters: Menlo and Consolas are designed for it and have far wider
    /// coverage of the box drawing characters and arrows a program draws its own screen with than Courier
    /// does.
    pub fn monospaced_family(&self) -> Option<String> {
        const MONOSPACED: &[&str] = &["Menlo", "Consolas", "Courier New", "Courier"];
        MONOSPACED
            .iter()
            .find(|wanted| self.families.iter().any(|family| family == *wanted))
            .map(|family| (*family).to_owned())
    }

    /// The family to start a new document in: the first candidate this system has.
    pub fn default_family(&self) -> String {
        self.families.first().cloned().unwrap_or_else(|| "Helvetica".to_owned())
    }

    /// Read a face from disk, or report that this system does not have it.
    ///
    /// Bold and italic pick a real face of the family rather than slanting or thickening the regular
    /// one. Helvetica on macOS ships regular, bold, oblique and bold oblique in one collection file, so
    /// the real faces are there to be used.
    fn face(&self, key: &FaceKey) -> Option<Arc<FontVec>> {
        if let Some(found) = self.faces.borrow().get(key) {
            return found.clone();
        }
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(&key.family)],
            weight: if key.bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
            style: if key.italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
            stretch: fontdb::Stretch::Normal,
        };
        let loaded = self
            .database
            .query(&query)
            .and_then(|id| {
                self.database.with_face_data(id, |data, index| {
                    FontVec::try_from_vec_and_index(data.to_vec(), index).ok()
                })
            })
            .flatten()
            .map(Arc::new);
        self.faces.borrow_mut().insert(key.clone(), loaded.clone());
        loaded
    }

    /// The face to use for a style, falling back through the italic and bold variants to the regular
    /// one, and then to any family this system has. A missing font must not stop text appearing.
    fn face_for(&self, style: &CharStyle) -> Option<Arc<FontVec>> {
        self.resolve(style).1
    }

    /// The face for a style and the number it answers to, remembering the last one asked for.
    fn resolve(&self, style: &CharStyle) -> (FaceId, Option<Arc<FontVec>>) {
        if let Some((key, id, face)) = self.memo.borrow().as_ref() {
            if key.is(style) {
                return (*id, face.clone());
            }
        }
        let key = FaceKey::of(style);
        let face = self.search(&key);
        let next = self.ids.borrow().len() as u32;
        let id = *self.ids.borrow_mut().entry(key.clone()).or_insert(FaceId(next));
        *self.memo.borrow_mut() = Some((key, id, face.clone()));
        (id, face)
    }

    /// Find the face for a style, falling back as [`Self::face_for`] describes. Called once per style
    /// rather than once per letter, because [`Self::resolve`] remembers the answer.
    fn search(&self, key: &FaceKey) -> Option<Arc<FontVec>> {
        let key = key.clone();
        if let Some(face) = self.face(&key) {
            return Some(face);
        }
        for fallback in [
            FaceKey { italic: false, ..key.clone() },
            FaceKey { bold: false, italic: false, ..key.clone() },
        ] {
            if let Some(face) = self.face(&fallback) {
                return Some(face);
            }
        }
        self.families.first().and_then(|family| {
            self.face(&FaceKey { family: family.clone(), bold: false, italic: false })
        })
    }

    /// The face to draw one character with: the style's own family when it has a shape for the character,
    /// and the first family in [`FALLBACK_FAMILIES`] that does when it has not.
    ///
    /// Glyph zero is the empty box a font uses for a character it has no shape for, so asking for a
    /// character and being given glyph zero is how a missing character is found.
    fn face_for_character(&self, character: char, style: &CharStyle) -> Option<Arc<FontVec>> {
        let chosen = self.face_for(style)?;
        if chosen.glyph_id(character).0 != 0 || character.is_whitespace() {
            return Some(chosen);
        }
        let key = (character, style.bold, style.italic);
        if let Some(found) = self.fallbacks.borrow().get(&key) {
            return match found {
                Some(family) => self
                    .face(&FaceKey {
                        family: family.clone(),
                        bold: style.bold,
                        italic: style.italic,
                    })
                    .or(Some(chosen)),
                None => Some(chosen),
            };
        }
        let mut answer = None;
        for family in FALLBACK_FAMILIES.iter().map(|family| (*family).to_owned()) {
            let candidate =
                FaceKey { family: family.clone(), bold: style.bold, italic: style.italic };
            if let Some(face) = self.face(&candidate) {
                if face.glyph_id(character).0 != 0 {
                    answer = Some((family, face));
                    break;
                }
            }
        }
        self.fallbacks.borrow_mut().insert(key, answer.as_ref().map(|(family, _)| family.clone()));
        Some(answer.map(|(_, face)| face).unwrap_or(chosen))
    }

    /// Rasterise for drawing that goes through `transform` from here on, and answer what it was before.
    ///
    /// What a node on the Realm sets around its contents with the camera, and the ticket modal with its own
    /// zoom. `settled` is whether the transform has stopped moving: see [`Crispness`] for why a glide is
    /// rasterised on a ladder and a still camera exactly. Everything else draws at [`Compositing::DIRECT`].
    /// **Always paired with [`Self::restore_compositing`]**, which is what stops a node's zoom reaching the
    /// pane drawn after it.
    pub fn composite_through(
        &self,
        transform: egui::emath::TSTransform,
        settled: bool,
    ) -> Compositing {
        self.compositing.replace(Compositing { transform, settled })
    }

    /// Put the compositing back to what [`Self::composite_through`] answered with.
    pub fn restore_compositing(&self, was: Compositing) {
        *self.compositing.borrow_mut() = was;
    }

    /// Rasterise for a display of `pixels_per_point` from here on. Called once a frame by the window.
    pub fn follow_the_display(&self, pixels_per_point: f32) {
        if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
            self.pixels_per_point.set(pixels_per_point);
        }
    }

    /// How many pixels a point is worth for whatever is being drawn now, and where those pixels are.
    pub fn crispness(&self) -> Crispness {
        Crispness::through(*self.compositing.borrow(), self.pixels_per_point.get())
    }

    /// Find or rasterise one glyph, at the size it will be **seen** at rather than laid out at.
    ///
    /// **One function rather than a multiplication at each painter**, because the raster size and the drawn
    /// size are two halves of one fact and asking them separately is how they come apart: a glyph rasterised
    /// larger and drawn at its rasterised size is simply bigger text. See [`Crispness`] for why the canvas
    /// needs this at all. `Crispness::EXACT` is every caller outside a node, and takes the same path
    /// [`Self::glyph`] always did.
    pub fn glyph(&self, character: char, style: &CharStyle) -> Option<AtlasGlyph> {
        let crispness = self.crispness();
        if crispness.is_exact() {
            return self.glyph_exactly(character, style);
        }
        let bigger = CharStyle { size: crispness.raster_size(style.size), ..style.clone() };
        self.glyph_exactly(character, &bigger).map(|glyph| crispness.drawn(glyph))
    }

    /// Find or rasterise one glyph at exactly the size the style names.
    fn glyph_exactly(&self, character: char, style: &CharStyle) -> Option<AtlasGlyph> {
        let key = GlyphKey {
            face: self.resolve(style).0,
            character,
            sixty_fourths: (style.size * 64.0).round() as u32,
        };
        if let Some(found) = self.atlas.borrow().entries.get(&key) {
            return *found;
        }
        let entry = self.rasterise(character, style);
        if entry.is_none_full() {
            // The atlas ran out of room. Start it again rather than stop drawing.
            self.atlas.borrow_mut().clear();
            let retry = self.rasterise(character, style);
            self.atlas.borrow_mut().entries.insert(key, retry.glyph());
            return retry.glyph();
        }
        self.atlas.borrow_mut().entries.insert(key, entry.glyph());
        entry.glyph()
    }

    fn rasterise(&self, character: char, style: &CharStyle) -> Rasterised {
        let Some(face) = self.face_for_character(character, style) else {
            return Rasterised::NoFont;
        };
        let scaled = face.as_scaled(PxScale::from(style.size));
        let glyph: Glyph = face
            .glyph_id(character)
            .with_scale_and_position(PxScale::from(style.size), ab_glyph::point(0.0, 0.0));
        let Some(outlined) = face.outline_glyph(glyph) else {
            // A space has no outline. It still advances the pen, which the metrics report separately.
            let _ = scaled;
            return Rasterised::Blank;
        };
        let bounds = outlined.px_bounds();
        let width = bounds.width().ceil() as usize;
        let height = bounds.height().ceil() as usize;
        if width == 0 || height == 0 {
            return Rasterised::Blank;
        }
        let mut atlas = self.atlas.borrow_mut();
        let Some((x, y)) = atlas.reserve(width, height) else {
            return Rasterised::AtlasFull;
        };
        // The rasteriser reports coverage from 0 to 1 for each pixel. The atlas holds white pixels
        // whose alpha is that coverage, so that painting can tint one texture any colour.
        outlined.draw(|dx, dy, coverage| {
            let px = x + dx as usize;
            let py = y + dy as usize;
            if px < ATLAS_SIDE && py < ATLAS_SIDE {
                let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
                atlas.image[(px, py)] = Color32::from_white_alpha(alpha);
            }
        });
        atlas.mark(x, y, width, height);
        let side = ATLAS_SIDE as f32;
        Rasterised::Glyph(AtlasGlyph {
            uv: egui::Rect::from_min_max(
                egui::pos2(x as f32 / side, y as f32 / side),
                egui::pos2((x + width) as f32 / side, (y + height) as f32 / side),
            ),
            size: egui::vec2(width as f32, height as f32),
            offset: egui::vec2(bounds.min.x, bounds.min.y),
        })
    }

    /// The bytes of a face, for handing to egui so that the interface itself is set in a real font rather
    /// than egui's built in one. egui needs the file contents; it does its own parsing.
    pub fn face_bytes(&self, family: &str, bold: bool) -> Option<Vec<u8>> {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
            style: fontdb::Style::Normal,
            stretch: fontdb::Stretch::Normal,
        };
        let id = self.database.query(&query)?;
        self.database.with_face_data(id, |data, _index| data.to_vec())
    }

    /// The size of one cell of the terminal grid, at `size` points.
    ///
    /// The terminal is a grid, so every cell is the same size and it is worked out once: the width is what
    /// one character advances in the monospaced family, and the height is what the font itself asks for
    /// between one line and the next. The reading leading that `line_metrics` adds for prose is left out
    /// here, because a terminal's lines belong close together and because a program drawing a box expects
    /// the lines to meet.
    ///
    /// Both are rounded to whole points, so that a column lands on a whole pixel and the grid stays sharp.
    pub fn cell_metrics(&self, size: f32) -> CellMetrics {
        let style = self.terminal_style(size, false, false);
        let Some(face) = self.face_for(&style) else {
            return CellMetrics { width: size * 0.6, height: size * 1.25, ascent: size };
        };
        let scaled = face.as_scaled(PxScale::from(size));
        // `M` is the widest ordinary letter, and in a monospaced family every letter is that wide.
        let width = scaled.h_advance(face.glyph_id('M')).round().max(1.0);
        let ascent = scaled.ascent();
        let height = (ascent - scaled.descent() + scaled.line_gap()).round().max(1.0);
        CellMetrics { width, height, ascent }
    }

    /// The formatting one terminal cell is drawn with, which is the monospaced family at the terminal's
    /// own size.
    pub fn terminal_style(&self, size: f32, bold: bool, italic: bool) -> CharStyle {
        CharStyle {
            family: self
                .monospaced_family()
                .unwrap_or_else(|| self.default_family())
                .as_str()
                .into(),
            size,
            bold,
            italic,
            ..CharStyle::default()
        }
    }

    /// The atlas as an image, which a test uses to look at the pixels of one glyph.
    pub fn atlas_image(&self) -> egui::ColorImage {
        self.atlas.borrow().image.clone()
    }

    /// How many times the atlas has been cleared. A caller that collected glyph positions and then sees
    /// this change knows those positions point at pixels that have been overwritten.
    pub fn generation(&self) -> u64 {
        self.atlas.borrow().generation
    }

    /// The texture to paint glyphs from, uploaded again only when new glyphs were added.
    ///
    /// Every glyph the caller intends to draw must be asked for through [`Self::glyph`] before this is
    /// called. Uploading first and rasterising afterwards would draw this frame from a texture that
    /// does not yet hold the new glyphs, and the letters would be missing.
    ///
    /// **Linear, which it was not before `task-2216`.** A glyph drawn one texel to one pixel on a whole pixel
    /// samples exactly the texel it names with either filter, so at rest the two are the same picture. The
    /// difference is every case that is not at rest — a glide, which rasterises a quarter step larger than it
    /// draws — and there a nearest filter drops whole columns of a letter and keeps others, which is what
    /// made the letters look pixelated rather than merely soft.
    pub fn texture(&self, ctx: &egui::Context) -> egui::TextureId {
        let mut atlas = self.atlas.borrow_mut();
        if atlas.texture.is_none() {
            let image = atlas.image.clone();
            atlas.texture =
                Some(ctx.load_texture("unluminous-glyphs", image, TextureOptions::LINEAR));
            atlas.changed = false;
            atlas.dirty = None;
        } else if atlas.changed {
            let image = atlas.image.clone();
            atlas.texture.as_mut().expect("just checked").set(image, TextureOptions::LINEAR);
            atlas.changed = false;
            atlas.dirty = None;
        } else if let Some([left, top, right, bottom]) = atlas.dirty.take() {
            let part = atlas.image.region_by_pixels([left, top], [right - left, bottom - top]);
            atlas.texture.as_mut().expect("just checked").set_partial(
                [left, top],
                part,
                TextureOptions::LINEAR,
            );
        }
        atlas.texture.as_ref().expect("set above").id()
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// What came back from trying to rasterise a glyph. `AtlasFull` is separated from the other failures
/// because it is the one worth retrying after clearing the atlas.
enum Rasterised {
    Glyph(AtlasGlyph),
    /// No outline to draw, such as a space.
    Blank,
    /// This system has no font at all.
    NoFont,
    AtlasFull,
}

impl Rasterised {
    fn glyph(&self) -> Option<AtlasGlyph> {
        match self {
            Self::Glyph(glyph) => Some(*glyph),
            _ => None,
        }
    }

    fn is_none_full(&self) -> bool {
        matches!(self, Self::AtlasFull)
    }
}

impl FontMetrics for TextRenderer {
    fn advance(&self, cluster: &str, style: &CharStyle) -> f32 {
        // One ASCII character is answered out of the table for its face and size. See `AdvanceTables`.
        if let [byte] = cluster.as_bytes() {
            if byte.is_ascii() {
                let known = self.ascii_table(style)[usize::from(*byte)];
                if !known.is_nan() {
                    return known;
                }
            }
        }
        self.measure_advance(cluster, style)
    }

    fn ascii_advances(&self, style: &CharStyle) -> Option<Arc<[f32; 128]>> {
        Some(self.ascii_table(style))
    }

    fn line_metrics(&self, style: &CharStyle) -> LineMetrics {
        let id = self.resolve_id(style);
        if let Some((face, size, metrics)) = self.last_metrics.get() {
            if face == id && size == style.size.to_bits() {
                return metrics;
            }
        }
        let metrics = self.measure_line_metrics(style);
        self.last_metrics.set(Some((id, style.size.to_bits(), metrics)));
        metrics
    }
}

impl TextRenderer {
    /// What [`FontMetrics::line_metrics`] answers, asking the font.
    fn measure_line_metrics(&self, style: &CharStyle) -> LineMetrics {
        let Some(face) = self.face_for(style) else {
            return LineMetrics { ascent: style.size, descent: style.size * 0.25, line_gap: 0.0 };
        };
        let scaled = face.as_scaled(PxScale::from(style.size));
        LineMetrics {
            ascent: scaled.ascent(),
            // ab_glyph reports the descent as a negative number, below the baseline.
            descent: -scaled.descent(),
            line_gap: scaled.line_gap() + style.size * READING_LEADING,
        }
    }

    /// The ASCII advances of `style`'s face at its size, measured the first time they are asked for.
    fn ascii_table(&self, style: &CharStyle) -> Arc<AsciiAdvances> {
        let face = self.resolve_id(style);
        if let Some(table) = self.advances.borrow_mut().find(face, style.size) {
            return table;
        }
        let mut table = [f32::NAN; 128];
        // The printable characters and the tab, which is byte nine.
        const TAB: u8 = 9;
        let mut letter = [0u8; 4];
        for byte in (0x20u8..0x7F).chain([TAB]) {
            table[usize::from(byte)] =
                self.measure_advance(char::from(byte).encode_utf8(&mut letter), style);
        }
        let table = Arc::new(table);
        self.advances.borrow_mut().keep(face, style.size, Arc::clone(&table));
        table
    }

    /// The number of the face a style resolves to, without cloning the face itself.
    ///
    /// [`Self::resolve`] hands back an `Arc` of the face, which is a reference count up and down for every
    /// character measured; the ASCII table only needs the number.
    fn resolve_id(&self, style: &CharStyle) -> FaceId {
        if let Some((key, id, _)) = self.memo.borrow().as_ref() {
            if key.is(style) {
                return *id;
            }
        }
        self.resolve(style).0
    }

    /// Measure how far `cluster` advances the pen, asking the font. [`FontMetrics::advance`] keeps the
    /// answers for ASCII.
    fn measure_advance(&self, cluster: &str, style: &CharStyle) -> f32 {
        // A cluster of several code points, such as a letter and a combining accent, takes the width of
        // its base character: the accent is drawn over the letter rather than after it.
        let Some(base) = cluster.chars().next() else {
            return 0.0;
        };
        // The face the character is actually drawn with, which may be a fallback, so that a character the
        // chosen family has no shape for is measured as wide as it is drawn.
        let Some(face) = self.face_for_character(base, style) else {
            // With no font at all, fall back to a fixed width so that layout still works and the
            // caret still moves. Text will not appear, which is a visible failure rather than a hang.
            return style.size * 0.5 * cluster.chars().count().max(1) as f32;
        };
        let scaled = face.as_scaled(PxScale::from(style.size));
        if base == '\t' {
            return scaled.h_advance(face.glyph_id(' ')) * 4.0;
        }
        scaled.h_advance(face.glyph_id(base))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_system_has_at_least_one_of_the_offered_families() {
        let renderer = TextRenderer::new();
        assert!(
            !renderer.families().is_empty(),
            "none of {CANDIDATE_FAMILIES:?} is installed, so no text could be drawn"
        );
    }

    /// `task-2218`: a glyph added after the atlas has been uploaded sends only the rectangle it was drawn
    /// into, and the upload that follows sends nothing more.
    #[test]
    fn a_new_glyph_uploads_only_the_part_of_the_atlas_it_was_drawn_into() {
        let ctx = egui::Context::default();
        let renderer = TextRenderer::new();
        let style = CharStyle {
            family: renderer.default_family().as_str().into(),
            size: 40.0,
            ..CharStyle::default()
        };
        let _ = renderer.glyph('A', &style);
        let _ = renderer.texture(&ctx);
        assert!(!renderer.atlas.borrow().changed && renderer.atlas.borrow().dirty.is_none());

        let glyph = renderer.glyph('W', &style).expect("a letter has a glyph");
        let dirty = renderer.atlas.borrow().dirty.expect("the new glyph's rectangle is pending");
        let [left, top, right, bottom] = dirty;
        assert!(
            right - left < ATLAS_SIDE && bottom - top < ATLAS_SIDE,
            "only a part, not the atlas"
        );
        assert_eq!((right - left) as f32, glyph.size.x);
        let _ = renderer.texture(&ctx);
        assert!(renderer.atlas.borrow().dirty.is_none(), "and it has been sent");
    }

    /// `task-2218`: an ASCII advance answered out of the table is exactly what the font answers, at every
    /// size and in every style it was measured in, and a size change is measured afresh.
    #[test]
    fn a_remembered_advance_is_exactly_what_the_font_measures() {
        let renderer = TextRenderer::new();
        let family: std::sync::Arc<str> = renderer.default_family().as_str().into();
        for size in [11.0, 16.0, 17.5, 24.0] {
            for (bold, italic) in [(false, false), (true, false), (false, true)] {
                let style = CharStyle {
                    family: family.clone(),
                    size,
                    bold,
                    italic,
                    ..CharStyle::default()
                };
                for byte in 0u8..128 {
                    let letter = (byte as char).to_string();
                    let measured = renderer.measure_advance(&letter, &style);
                    assert_eq!(renderer.advance(&letter, &style), measured, "{byte} at {size}");
                    assert_eq!(
                        renderer.advance(&letter, &style),
                        measured,
                        "asked twice, {byte} at {size}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_wider_letter_advances_further_than_a_narrow_one() {
        let renderer = TextRenderer::new();
        let style =
            CharStyle { family: renderer.default_family().as_str().into(), ..CharStyle::default() };
        let narrow = renderer.advance("i", &style);
        let wide = renderer.advance("W", &style);
        assert!(wide > narrow, "W ({wide}) should be wider than i ({narrow})");
    }

    #[test]
    fn a_bigger_font_size_advances_further_and_stands_taller() {
        let renderer = TextRenderer::new();
        let small = CharStyle {
            family: renderer.default_family().as_str().into(),
            size: 12.0,
            ..CharStyle::default()
        };
        let large = CharStyle { size: 36.0, ..small.clone() };
        assert!(renderer.advance("m", &large) > renderer.advance("m", &small));
        assert!(renderer.line_metrics(&large).height() > renderer.line_metrics(&small).height());
    }

    #[test]
    fn bold_text_is_at_least_as_wide_as_regular_text() {
        let renderer = TextRenderer::new();
        let regular =
            CharStyle { family: renderer.default_family().as_str().into(), ..CharStyle::default() };
        let bold = CharStyle { bold: true, ..regular.clone() };
        assert!(renderer.advance("mmmm", &bold) >= renderer.advance("mmmm", &regular));
    }

    #[test]
    fn an_unknown_family_still_measures_and_still_draws() {
        let renderer = TextRenderer::new();
        let style = CharStyle { family: "No Such Font At All".into(), ..CharStyle::default() };
        assert!(renderer.advance("a", &style) > 0.0, "a missing family must not stop layout");
        assert!(renderer.line_metrics(&style).height() > 0.0);
        assert!(renderer.glyph('a', &style).is_some(), "it should fall back to a family we have");
    }

    /// The renderer remembers the last face it resolved, because layout and painting both walk run by
    /// run and every character of a run shares a style. A memo that answered with the previous
    /// style's face would draw a whole run in the wrong font, so this asks the same two styles over
    /// and over in turn.
    #[test]
    fn alternating_between_two_styles_still_gives_each_its_own_glyph() {
        let renderer = TextRenderer::new();
        let regular = CharStyle { size: 20.0, ..CharStyle::default() };
        let bold = CharStyle { size: 20.0, bold: true, ..CharStyle::default() };
        let large = CharStyle { size: 40.0, ..CharStyle::default() };

        let first_regular = renderer.glyph('R', &regular).expect("a shape for R");
        let first_bold = renderer.glyph('R', &bold).expect("a shape for R in bold");
        let first_large = renderer.glyph('R', &large).expect("a shape for R at 40 points");
        for _ in 0..10 {
            let again_regular = renderer.glyph('R', &regular).expect("a shape for R");
            let again_bold = renderer.glyph('R', &bold).expect("a shape for R in bold");
            let again_large = renderer.glyph('R', &large).expect("a shape for R at 40 points");
            assert_eq!(again_regular.uv, first_regular.uv, "the same style gives the same glyph");
            assert_eq!(again_bold.uv, first_bold.uv);
            assert_eq!(again_large.uv, first_large.uv);
        }
        assert_ne!(first_regular.uv, first_bold.uv, "bold is a different entry in the atlas");
        assert_ne!(first_regular.uv, first_large.uv, "and so is another size");
        assert!(first_large.size.y > first_regular.size.y, "and it is drawn larger");
    }

    /// The same for measuring, which is what layout asks once for every grapheme cluster.
    #[test]
    fn alternating_between_two_styles_still_measures_each_on_its_own() {
        let renderer = TextRenderer::new();
        let small = CharStyle { size: 12.0, ..CharStyle::default() };
        let large = CharStyle { size: 48.0, ..CharStyle::default() };
        let small_first = renderer.advance("m", &small);
        let large_first = renderer.advance("m", &large);
        assert!(large_first > small_first);
        for _ in 0..10 {
            assert_eq!(renderer.advance("m", &small), small_first);
            assert_eq!(renderer.advance("m", &large), large_first);
        }
    }

    #[test]
    fn glyphs_are_rasterised_once_and_then_reused() {
        let renderer = TextRenderer::new();
        let style =
            CharStyle { family: renderer.default_family().as_str().into(), ..CharStyle::default() };
        let first = renderer.glyph('A', &style).expect("A should rasterise");
        let second = renderer.glyph('A', &style).expect("A should still be there");
        assert_eq!(renderer.atlas.borrow().entries.len(), 1, "asked twice, stored once");
        assert_eq!(first.uv, second.uv, "the same glyph comes back from the same place");
        assert!(first.size.x > 0.0 && first.size.y > 0.0);
    }

    #[test]
    fn the_same_letter_at_two_sizes_is_two_entries() {
        let renderer = TextRenderer::new();
        let small = CharStyle {
            family: renderer.default_family().as_str().into(),
            size: 12.0,
            ..CharStyle::default()
        };
        let large = CharStyle { size: 40.0, ..small.clone() };
        let small_glyph = renderer.glyph('B', &small).expect("rasterise at 12");
        let large_glyph = renderer.glyph('B', &large).expect("rasterise at 40");
        assert_eq!(renderer.atlas.borrow().entries.len(), 2);
        assert!(large_glyph.size.y > small_glyph.size.y, "40 point should be taller than 12 point");
    }

    #[test]
    fn a_terminal_cell_is_wider_and_taller_at_a_bigger_size() {
        let renderer = TextRenderer::new();
        let small = renderer.cell_metrics(11.0);
        let large = renderer.cell_metrics(20.0);
        assert!(small.width > 0.0 && small.height > 0.0);
        assert!(large.width > small.width, "a bigger font makes a wider cell");
        assert!(large.height > small.height);
        assert_eq!(large.width, large.width.round(), "a cell is a whole number of points wide");
        assert_eq!(large.height, large.height.round());
        assert!(large.ascent < large.height, "the baseline sits inside the cell");
    }

    #[test]
    fn every_letter_in_the_terminal_family_is_the_same_width() {
        // A terminal is a grid, so this has to hold for the grid to line up. It is checked rather than
        // assumed, because the family is whichever monospaced one the system has.
        let renderer = TextRenderer::new();
        let style = renderer.terminal_style(14.0, false, false);
        let width = renderer.advance("M", &style);
        for letter in ["i", "W", "0", ".", "@"] {
            let other = renderer.advance(letter, &style);
            assert!(
                (other - width).abs() < 0.01,
                "{letter} is {other} wide and M is {width}, so the grid would not line up"
            );
        }
    }

    #[test]
    fn a_character_the_family_has_no_shape_for_is_found_in_another_family() {
        // The characters `claude` and `codex` draw with. A monospaced text face has some of them and not
        // others, and the ones it does not have would otherwise come out as an empty box.
        let renderer = TextRenderer::new();
        let style = renderer.terminal_style(14.0, false, false);
        for character in ['\u{25b6}', '\u{2713}', '\u{2502}', '\u{250c}', '\u{2588}'] {
            let face = renderer
                .face_for_character(character, &style)
                .expect("there should be a face to draw with");
            assert_ne!(
                face.glyph_id(character).0,
                0,
                "{character:?} came out as the empty box a font uses for a character it does not have"
            );
            assert!(
                renderer.glyph(character, &style).is_some(),
                "{character:?} should have pixels to draw"
            );
        }
    }

    #[test]
    fn a_character_no_family_has_is_still_measured_and_still_drawn() {
        // A character in a private use area, which nothing has a shape for. It falls back to the chosen
        // family's own empty box, which is what a terminal shows, rather than to nothing at all.
        let renderer = TextRenderer::new();
        let style = renderer.terminal_style(14.0, false, false);
        assert!(renderer.advance("\u{f8ff}", &style) > 0.0);
        assert!(renderer.face_for_character('\u{101234}', &style).is_some());
    }

    #[test]
    fn a_space_has_width_but_nothing_to_draw() {
        let renderer = TextRenderer::new();
        let style =
            CharStyle { family: renderer.default_family().as_str().into(), ..CharStyle::default() };
        assert!(renderer.advance(" ", &style) > 0.0, "a space advances the pen");
        assert!(renderer.glyph(' ', &style).is_none(), "a space has no pixels");
    }

    #[test]
    fn the_atlas_never_overlaps_two_glyphs() {
        let renderer = TextRenderer::new();
        let style =
            CharStyle { family: renderer.default_family().as_str().into(), ..CharStyle::default() };
        for character in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars() {
            renderer.glyph(character, &style);
        }
        let atlas = renderer.atlas.borrow();
        let placed: Vec<AtlasGlyph> = atlas.entries.values().flatten().copied().collect();
        assert!(placed.len() > 50, "most of those characters should have pixels");
        for (index, one) in placed.iter().enumerate() {
            for other in &placed[index + 1..] {
                assert!(
                    !one.uv.intersects(other.uv),
                    "two glyphs were given overlapping room in the atlas: {:?} and {:?}",
                    one.uv,
                    other.uv
                );
            }
        }
    }
}

#[cfg(test)]
mod crispness_tests {
    use super::*;

    /// The atlas is asked for the size a glyph is seen at, and the quad is the size it was laid out at.
    ///
    /// `task-1907`: a node is drawn into a layer carrying the camera and `epaint` scales the finished mesh, so
    /// a glyph rasterised at the layout's size is a magnified bitmap. What the atlas is asked for is the seen
    /// size; what is drawn is the laid-out rectangle.
    #[test]
    fn the_atlas_is_asked_for_the_size_a_glyph_is_seen_at() {
        let doubled = Crispness::at(2.0);
        assert_eq!(doubled.raster_size(12.0), 24.0, "rasterised at the size it is composited at");
        let glyph = AtlasGlyph {
            uv: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            size: egui::vec2(20.0, 30.0),
            offset: egui::vec2(2.0, -24.0),
        };
        let drawn = doubled.drawn(glyph);
        assert_eq!(drawn.size, egui::vec2(10.0, 15.0), "and drawn into the layout's own rectangle");
        assert_eq!(
            drawn.offset,
            egui::vec2(1.0, -12.0),
            "with its offset divided by the same number"
        );
        assert_eq!(drawn.uv, glyph.uv, "the pixels it points at do not move");
    }

    /// Everything outside the canvas is untouched, which is what makes this safe to add.
    #[test]
    fn text_composited_at_its_own_size_is_left_exactly_alone() {
        let exact = Crispness::EXACT;
        assert!(exact.is_exact());
        assert_eq!(exact.raster_size(16.0), 16.0);
        assert_eq!(exact.snap(egui::pos2(10.4, 20.6)), egui::pos2(10.0, 21.0), "whole points");
        // A camera at 1.0 is the same thing, so opening the canvas changes no glyph.
        assert!(Crispness::at(1.0).is_exact());
    }

    /// A pinch asks the atlas for a handful of sizes rather than one per frame.
    ///
    /// The atlas is one texture that is cleared and restarted when it fills, so an unquantised scale would
    /// clear it repeatedly and cost more than the blur it was fixing.
    #[test]
    fn a_raster_size_is_quantised_so_a_pinch_asks_for_a_handful_of_sizes() {
        let mut sizes = std::collections::BTreeSet::new();
        let mut zoom = 0.25_f32;
        while zoom <= 2.5 {
            sizes.insert((Crispness::at(zoom).raster_size(16.0) * 100.0).round() as i64);
            zoom += 0.01;
        }
        assert!(
            sizes.len() <= 10,
            "the whole camera range asks for a handful of sizes, not one per notch: {sizes:?}"
        );
        assert!(sizes.len() > 1, "and more than one, or nothing was gained");
    }

    /// A glyph is never rasterised **smaller** than the size it is composited at.
    ///
    /// **The quantising must round up, and rounding to the nearest is what fails.** The canvas zooms in steps of
    /// 1.1, so a person's very first zoom is 1.1 — which rounding to the nearest quarter sends back to 1.0,
    /// leaving the glyph rasterised at its layout size and magnified exactly as before the change. The Codex Sol
    /// review of `task-1907` found that, and this is the property that cannot be true of the wrong arithmetic:
    /// magnifying a bitmap is the fault, and scaling one down is ordinary resampling.
    #[test]
    fn a_glyph_is_never_rasterised_smaller_than_it_is_composited() {
        // Every step of the canvas's own 1.1 ladder, from the bottom of the range to the top.
        // The canvas's own bounds, written out rather than imported: this module has no business depending on
        // the canvas, and `Camera` clamps to these — see `services::realm::node`.
        let mut zoom = 0.25_f32;
        while zoom <= 2.5 {
            let scale = Crispness::at(zoom).raster_size(16.0) / 16.0;
            assert!(
                scale >= zoom.min(2.5) - 0.001,
                "at a camera of {zoom} the glyph was rasterised at {scale} of its layout size"
            );
            zoom *= 1.1;
        }
        // And the first step in particular, which is the one the review named.
        assert!(Crispness::at(1.1).raster_size(16.0) >= 16.0 * 1.1);
    }

    /// A canvas zoomed **out** rasterises its glyphs small, which is what `task-1907` stopped short of.
    ///
    /// That ticket ended `Crispness::at` in `max(1.0)`, so a zoomed-out canvas rasterised at the layout's own
    /// size and let the transform shrink it — a 16 point glyph rasterised at 16 and composited into 9.8
    /// points, resampled down through a nearest filter. `task-1945` was reported from a camera at 0.61 and
    /// the Agent Tasks cards there were unreadable.
    ///
    /// Rounding up still holds below one, which is what keeps the rule above true: 0.61 asks for 0.75, so the
    /// glyph still has more pixels than it is composited into and the transform is still only shrinking.
    #[test]
    fn a_canvas_zoomed_out_rasterises_its_glyphs_at_the_size_they_are_seen_at() {
        assert!(!Crispness::at(0.61).is_exact(), "0.61 is not the layout's own size");
        assert_eq!(
            Crispness::at(0.61).raster_size(16.0),
            12.0,
            "16 x 0.75, the quarter step above 0.61"
        );
        assert_eq!(Crispness::at(0.5).raster_size(16.0), 8.0);
        assert_eq!(Crispness::at(0.25).raster_size(16.0), 4.0);
        // Never below the camera's own floor, however small a scale a caller asks for.
        assert_eq!(Crispness::at(0.01).raster_size(16.0), 16.0 * SMALLEST_RASTER);
        // And a glyph rasterised small is put back at its layout size, so nothing about the layout moves.
        let small = Crispness::at(0.5);
        let glyph = AtlasGlyph {
            uv: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(0.1, 0.1)),
            size: egui::vec2(4.0, 8.0),
            offset: egui::vec2(1.0, -6.0),
        };
        let drawn = small.drawn(glyph);
        assert_eq!(drawn.size, egui::vec2(8.0, 16.0), "divided back by the scale it was asked for");
        assert_eq!(drawn.offset, egui::vec2(2.0, -12.0));
    }

    /// A glyph is snapped to a whole pixel rather than to a whole point.
    ///
    /// Rounding to points inside a node fights the scaling and spaces the letters unevenly, which is worse
    /// than the softening the rounding exists to prevent — and it is invisible in a small picture, so it is
    /// asserted as arithmetic.
    #[test]
    fn a_glyph_is_snapped_to_whole_pixels_rather_than_whole_points() {
        let doubled = Crispness::at(2.0);
        // 10.4 points is 20.8 pixels, which rounds to 21 pixels and so to 10.5 points.
        assert_eq!(doubled.snap(egui::pos2(10.4, 20.6)), egui::pos2(10.5, 20.5));
        // And a position already on a pixel boundary does not move.
        assert_eq!(doubled.snap(egui::pos2(10.5, 20.0)), egui::pos2(10.5, 20.0));
    }

    /// A camera that has stopped rasterises at exactly its zoom, so a glyph is drawn one texel to one pixel.
    ///
    /// `task-2216`: at 1.1 the quarter step above was 1.25, and every glyph on a still canvas was drawn at
    /// 0.88 of its bitmap. A glide keeps the quarter steps, which is what keeps the atlas from filling.
    #[test]
    fn a_settled_camera_rasterises_at_exactly_its_zoom() {
        for zoom in [0.61_f32, 0.9, 1.1, 1.21, 1.331, 1.77, 2.3] {
            let still = Compositing {
                transform: egui::emath::TSTransform::new(egui::vec2(3.3, 4.4), zoom),
                settled: true,
            };
            let crispness = Crispness::through(still, 1.0);
            assert!(
                (crispness.raster_size(16.0) - 16.0 * zoom).abs() < 0.001,
                "a still camera at {zoom} rasterised at {}",
                crispness.raster_size(16.0) / 16.0
            );
            let gliding = Compositing { settled: false, ..still };
            assert_eq!(
                Crispness::through(gliding, 1.0).raster_size(16.0),
                Crispness::at(zoom).raster_size(16.0),
                "and a gliding one keeps the ladder"
            );
        }
    }

    /// A glyph is snapped to a whole pixel of the **window**, which a layer moved by a fraction of a pixel
    /// is not the same as.
    ///
    /// The canvas's camera is at any position at all, so its layer's origin is any fraction of a pixel, and a
    /// glyph rounded in the layer's own points lands that fraction off a pixel boundary in the window.
    #[test]
    fn a_glyph_lands_on_a_whole_pixel_of_the_window() {
        let transform = egui::emath::TSTransform::new(egui::vec2(13.37, 7.21), 1.1);
        let crispness = Crispness::through(Compositing { transform, settled: true }, 1.0);
        for at in [egui::pos2(10.4, 20.6), egui::pos2(0.0, 0.0), egui::pos2(123.456, 78.9)] {
            let on_screen = transform * crispness.snap(at);
            assert!(
                (on_screen.x - on_screen.x.round()).abs() < 0.001
                    && (on_screen.y - on_screen.y.round()).abs() < 0.001,
                "{at:?} landed at {on_screen:?}"
            );
            assert!(
                (on_screen - transform * at).length() <= 0.71,
                "and it moved less than a pixel"
            );
        }
    }

    /// A display of two pixels a point rasterises two pixels a point, and snaps to its own pixels.
    ///
    /// Before `task-2216` this renderer never asked the display, so a 16 point glyph on a Retina screen was
    /// rasterised with 16 pixels and drawn into 32.
    #[test]
    fn a_dense_display_is_rasterised_at_its_own_density() {
        let retina = Crispness::through(Compositing::DIRECT, 2.0);
        assert_eq!(retina.raster_size(16.0), 32.0);
        assert!(!retina.is_exact());
        assert_eq!(retina.snap(egui::pos2(10.3, 20.8)), egui::pos2(10.5, 21.0), "half points");
        let renderer = TextRenderer::new();
        renderer.follow_the_display(2.0);
        assert_eq!(renderer.crispness(), retina);
        renderer.follow_the_display(1.0);
        assert!(renderer.crispness().is_exact(), "and back on an ordinary display");
    }
}
