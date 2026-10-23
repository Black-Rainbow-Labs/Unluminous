//! Real fonts for the tests that compare pages with Word's, so a line is as wide as Word measures it.
//!
//! The machine's own fonts through `fontdb`, plus Office's cloud fonts (`Aptos` lives there on a machine
//! with Microsoft 365), measured at their em size: a 12 point font is 12 points from em edge to em edge,
//! which is what Word means by the size.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use unluminous_core::{CharStyle, FontMetrics, LineMetrics};

pub struct RealFonts {
    database: fontdb::Database,
    faces: RefCell<HashMap<(String, bool, bool), Option<FontArc>>>,
}

impl RealFonts {
    pub fn new() -> RealFonts {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let cloud = std::path::Path::new(&local).join("Microsoft/FontCache/4/CloudFonts");
            if cloud.is_dir() {
                database.load_fonts_dir(cloud);
            }
        }
        RealFonts { database, faces: RefCell::new(HashMap::new()) }
    }

    fn face(&self, style: &CharStyle) -> Option<FontArc> {
        let key = (style.family.to_string(), style.bold, style.italic);
        if let Some(found) = self.faces.borrow().get(&key) {
            return found.clone();
        }
        let families = [fontdb::Family::Name(&style.family), fontdb::Family::Name("Calibri"), fontdb::Family::SansSerif];
        let query = fontdb::Query {
            families: &families,
            weight: if style.bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
            style: if style.italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
            stretch: fontdb::Stretch::Normal,
        };
        let face = self.database.query(&query).and_then(|id| {
            self.database
                .with_face_data(id, |data, index| ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index).ok().map(FontArc::new))
                .flatten()
        });
        self.faces.borrow_mut().insert(key, face.clone());
        face
    }

    fn scale(font: &FontArc, size: f32) -> PxScale {
        let units = font.units_per_em().unwrap_or(2048.0);
        PxScale::from(size * font.height_unscaled() / units)
    }
}

impl FontMetrics for RealFonts {
    fn advance(&self, cluster: &str, style: &CharStyle) -> f32 {
        let Some(font) = self.face(style) else { return style.size * 0.5 * cluster.chars().count() as f32 };
        let scaled = font.as_scaled(Self::scale(&font, style.size));
        cluster.chars().next().map_or(0.0, |c| scaled.h_advance(font.glyph_id(c)))
    }

    fn line_metrics(&self, style: &CharStyle) -> LineMetrics {
        let Some(font) = self.face(style) else {
            return LineMetrics { ascent: style.size * 0.9, descent: style.size * 0.3, line_gap: 0.0 };
        };
        let scaled = font.as_scaled(Self::scale(&font, style.size));
        LineMetrics { ascent: scaled.ascent(), descent: -scaled.descent(), line_gap: scaled.line_gap() }
    }

    fn ascii_advances(&self, _style: &CharStyle) -> Option<Arc<[f32; 128]>> {
        None
    }
}
