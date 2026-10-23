//! Pair kerning from a font's GPOS table, the way Word kerns.
//!
//! Word kerns a run when its `w:kern` says so, and it takes the pairs from the font's OpenType `kern`
//! feature. The old `kern` table that most font libraries read holds only some of them: Aptos, Word's
//! default face, has 11,103 pairs there and many more through class pairs in GPOS, and measuring with the
//! old table alone made lines up to two points wider than Word's, so a paragraph wrapped a line early.
//!
//! [`PairKerning::read`] reads every pair adjustment lookup the `kern` feature names, once a face, into
//! tables that answer a pair in constant time.

use std::collections::HashMap;

use ttf_parser::gpos::{PairAdjustment, PositioningSubtable};
use ttf_parser::GlyphId;

enum Subtable {
    /// Format 1: listed pairs.
    Pairs(HashMap<(u16, u16), i16>),
    /// Format 2: a value for each pair of classes, for the glyphs in the coverage.
    Classes { covered: Vec<bool>, first: Vec<u16>, second: Vec<u16>, columns: usize, values: Vec<i16> },
}

/// A face's kerning pairs, in font units.
pub struct PairKerning {
    subtables: Vec<Subtable>,
    pub units_per_em: f32,
}

impl PairKerning {
    /// Read the `kern` feature's pair lookups. `None` when the face has none, which leaves the old table
    /// (or nothing) to answer.
    pub fn read(bytes: &[u8], index: u32) -> Option<PairKerning> {
        let face = ttf_parser::Face::parse(bytes, index).ok()?;
        let glyphs = face.number_of_glyphs();
        let gpos = face.tables().gpos?;
        let mut lookups: Vec<u16> = gpos.features.into_iter().filter(|f| &f.tag.to_bytes() == b"kern").flat_map(|f| f.lookup_indices.into_iter()).collect();
        lookups.sort_unstable();
        lookups.dedup();
        let mut subtables = Vec::new();
        for index in lookups {
            let Some(lookup) = gpos.lookups.get(index) else { continue };
            for n in 0..lookup.subtables.len() {
                let Some(PositioningSubtable::Pair(pair)) = lookup.subtables.get::<PositioningSubtable>(n) else { continue };
                match pair {
                    PairAdjustment::Format1 { coverage, sets } => {
                        let mut pairs = HashMap::new();
                        for glyph in 0..glyphs {
                            let Some(set_index) = coverage.get(GlyphId(glyph)) else { continue };
                            let Some(set) = sets.get(set_index) else { continue };
                            for second in 0..glyphs {
                                if let Some((value, _)) = set.get(GlyphId(second)) {
                                    if value.x_advance != 0 {
                                        pairs.entry((glyph, second)).or_insert(value.x_advance);
                                    }
                                }
                            }
                        }
                        subtables.push(Subtable::Pairs(pairs));
                    }
                    PairAdjustment::Format2 { coverage, classes, matrix } => {
                        let covered: Vec<bool> = (0..glyphs).map(|g| coverage.contains(GlyphId(g))).collect();
                        let first: Vec<u16> = (0..glyphs).map(|g| classes.0.get(GlyphId(g))).collect();
                        let second: Vec<u16> = (0..glyphs).map(|g| classes.1.get(GlyphId(g))).collect();
                        let rows = first.iter().copied().max().unwrap_or(0) as usize + 1;
                        let columns = second.iter().copied().max().unwrap_or(0) as usize + 1;
                        let mut values = vec![0i16; rows * columns];
                        for row in 0..rows {
                            for column in 0..columns {
                                if let Some((value, _)) = matrix.get((row as u16, column as u16)) {
                                    values[row * columns + column] = value.x_advance;
                                }
                            }
                        }
                        subtables.push(Subtable::Classes { covered, first, second, columns, values });
                    }
                }
            }
        }
        if subtables.is_empty() {
            return None;
        }
        Some(PairKerning { subtables, units_per_em: f32::from(face.units_per_em()) })
    }

    /// The adjustment for `left` then `right`, in font units. The first subtable that covers the first
    /// glyph answers, which is how a shaper applies a pair lookup.
    pub fn get(&self, left: u16, right: u16) -> i16 {
        for subtable in &self.subtables {
            match subtable {
                Subtable::Pairs(pairs) => {
                    if let Some(value) = pairs.get(&(left, right)) {
                        return *value;
                    }
                }
                Subtable::Classes { covered, first, second, columns, values } => {
                    if covered.get(left as usize).copied().unwrap_or(false) {
                        let row = first.get(left as usize).copied().unwrap_or(0) as usize;
                        let column = second.get(right as usize).copied().unwrap_or(0) as usize;
                        let value = values.get(row * columns + column).copied().unwrap_or(0);
                        if value != 0 {
                            return value;
                        }
                    }
                }
            }
        }
        0
    }

    /// The adjustment in points at `size`.
    pub fn points(&self, left: u16, right: u16, size: f32) -> f32 {
        f32::from(self.get(left, right)) / self.units_per_em * size
    }
}
