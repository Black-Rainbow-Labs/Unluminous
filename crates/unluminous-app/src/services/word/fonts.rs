//! The window's fonts, measured the way Word measures them.
//!
//! `TextRenderer` sizes a face so that its ascent to descent is the size asked for, and adds reading
//! leading to every line. A Word document's size is the em and its single line is the font's own, so
//! [`WordFonts`] asks the renderer for `size * em_ratio` and for the font's own line metrics. Drawing goes
//! through the same ratio (`components::word::page_view`), so what is drawn is what was measured.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use unluminous_core::{CharStyle, FontMetrics, LineMetrics};

use crate::services::text_renderer::TextRenderer;

/// A `FontMetrics` over the window's renderer at Word's sizes.
pub struct WordFonts<'a> {
    pub renderer: &'a TextRenderer,
    ratios: RefCell<HashMap<(Arc<str>, bool, bool), f32>>,
}

impl<'a> WordFonts<'a> {
    pub fn new(renderer: &'a TextRenderer) -> Self {
        WordFonts { renderer, ratios: RefCell::new(HashMap::new()) }
    }

    /// The style to ask the renderer for, for a run Word sets at `style.size` points.
    pub fn drawn(&self, style: &CharStyle) -> CharStyle {
        let key = (Arc::clone(&style.family), style.bold, style.italic);
        let ratio = *self.ratios.borrow_mut().entry(key).or_insert_with(|| self.renderer.em_ratio(style));
        CharStyle { size: style.size * ratio, ..style.clone() }
    }
}

impl FontMetrics for WordFonts<'_> {
    fn advance(&self, cluster: &str, style: &CharStyle) -> f32 {
        self.renderer.advance(cluster, &self.drawn(style))
    }

    fn line_metrics(&self, style: &CharStyle) -> LineMetrics {
        self.renderer.font_line_metrics(&self.drawn(style))
    }

    fn ascii_advances(&self, style: &CharStyle) -> Option<Arc<[f32; 128]>> {
        self.renderer.ascii_advances(&self.drawn(style))
    }

    /// The face's GPOS pairs, which is where Word reads kerning; the old `kern` table when it has none.
    fn kerning(&self, left: char, right: char, style: &CharStyle) -> f32 {
        match pairs_for(self.renderer, style) {
            Some(pairs) => pairs.points(self.renderer.glyph_index(left, style), self.renderer.glyph_index(right, style), style.size),
            None => self.renderer.kerning(left, right, &self.drawn(style)),
        }
    }
}

thread_local! {
    /// Each face's GPOS kerning pairs, read once: reading them walks the whole table.
    static PAIRS: RefCell<HashMap<(Arc<str>, bool, bool), Option<Arc<unluminous_word::kern::PairKerning>>>> = RefCell::new(HashMap::new());
}

fn pairs_for(renderer: &TextRenderer, style: &CharStyle) -> Option<Arc<unluminous_word::kern::PairKerning>> {
    let key = (Arc::clone(&style.family), style.bold, style.italic);
    PAIRS.with(|cache| {
        cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let (bytes, index) = renderer.face_file(style)?;
                unluminous_word::kern::PairKerning::read((*bytes).as_ref(), index).map(Arc::new)
            })
            .clone()
    })
}

/// The folders Office keeps the fonts it downloads in, which is where `Aptos`, Word's default face since
/// 2023, lives on a machine with Microsoft 365. Read once, the first time a document is opened.
pub fn office_font_folders() -> Vec<std::path::PathBuf> {
    let mut folders = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        folders.push(std::path::Path::new(&local).join("Microsoft").join("FontCache").join("4").join("CloudFonts"));
    }
    // The fonts Office installs for its own programs, Agency, Bookman, Century Gothic and the rest, which a
    // document from Word names as if every machine had them.
    if let Some(files) = std::env::var_os("ProgramFiles") {
        folders.push(std::path::Path::new(&files).join("Microsoft Office").join("root").join("vfs").join("Fonts").join("private"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        folders.push(
            std::path::Path::new(&home)
                .join("Library/Group Containers/UBF8T346G9.Office/FontCache/4/CloudFonts"),
        );
    }
    folders.into_iter().filter(|folder| folder.is_dir()).collect()
}
