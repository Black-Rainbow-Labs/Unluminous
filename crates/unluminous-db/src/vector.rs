//! Reading a vector out of the bytes a row holds.
//!
//! An embedding is the reason the Inillucent engine exists, and in a grid it arrives as a blob:
//! `inillucent-search` stores one as **little-endian `f32`, four bytes each**, and hands it back from a
//! search table's `vector` column and from its `%_content` shadow table's `v`. Drawn as bytes that is
//! `3072 bytes: 3f 00 00 00…`, which tells somebody the row has *something*, which they knew.
//!
//! So this module is the reading, and it is here rather than in the window for the reason
//! `unluminous_core::mermaid` puts the arithmetic below the drawing: it is a pure function of some
//! bytes, it is where the awkward cases are, and every one of its tests runs with no window.
//!
//! **Nothing here guesses, and this module is not what stops it.** [`Vector::decode`] is deliberately
//! permissive: it rejects a length that is not a whole number of floats, a length below two, and any
//! component that is not finite, and that is all it can honestly do. A PNG's first eight bytes decode
//! to two perfectly finite floats — measured, in this file's own tests — so a decoder is the wrong
//! place to look for safety.
//!
//! What decides that a column *is* a vector is the **schema**: a search table declares its width in
//! its own `%_config`, `catalog::Table::vector_columns` carries that, and the grid draws a vector only
//! where the schema said there is one. Everywhere else a blob stays a blob until a person opens it and
//! asks, which makes the reading their question rather than the grid's claim.

/// A vector read out of a cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    pub values: Vec<f32>,
}

/// The smallest number of dimensions worth calling a vector.
///
/// Two rather than one, because four bytes that happen to divide by four are every four-byte blob
/// there is, and a one-dimension "embedding" is a number somebody stored as bytes.
pub const SMALLEST: usize = 2;

/// How many components the summary shows before it stops.
///
/// Three, because the summary's job is to say *this is a vector, here is roughly what is in it* in the
/// width of a grid cell, and the inspector is where the rest lives.
pub const SHOWN: usize = 3;

impl Vector {
    /// Read a vector out of a cell's bytes, or answer `None`.
    ///
    /// Three conditions, and each rules out a real thing that is not a vector: a length that is not a
    /// whole number of floats, a length below [`SMALLEST`], and any component that is not finite. That
    /// last one is what rejects most files — a NaN is a bit pattern almost every compressed format
    /// produces within its first few words, and no embedder emits one.
    ///
    /// @param bytes - the cell's bytes
    pub fn decode(bytes: &[u8]) -> Option<Vector> {
        if bytes.len() % 4 != 0 || bytes.len() / 4 < SMALLEST {
            return None;
        }
        let mut values = Vec::with_capacity(bytes.len() / 4);
        for word in bytes.chunks_exact(4) {
            let value = f32::from_le_bytes([word[0], word[1], word[2], word[3]]);
            if !value.is_finite() {
                return None;
            }
            values.push(value);
        }
        Some(Vector { values })
    }

    /// The bytes this vector would be written back as, which is `decode`'s inverse.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.values.len() * 4);
        for value in &self.values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    pub fn dimensions(&self) -> usize {
        self.values.len()
    }

    /// The Euclidean length.
    ///
    /// Worth showing beside every vector because the engine's distance is **cosine**, which is defined
    /// on direction alone: a corpus whose vectors are normalised has every norm at 1.000, so one that
    /// is not is the single quickest way to see that something was stored unnormalised.
    pub fn norm(&self) -> f32 {
        self.values.iter().map(|value| value * value).sum::<f32>().sqrt()
    }

    pub fn smallest(&self) -> f32 {
        self.values.iter().copied().fold(f32::INFINITY, f32::min)
    }

    pub fn largest(&self) -> f32 {
        self.values.iter().copied().fold(f32::NEG_INFINITY, f32::max)
    }

    pub fn mean(&self) -> f32 {
        match self.values.is_empty() {
            true => 0.0,
            false => self.values.iter().sum::<f32>() / self.values.len() as f32,
        }
    }

    /// What the grid draws in the cell.
    ///
    /// The three things a person can actually use at that width: how many dimensions, what the first
    /// few components look like, and the norm. The ellipsis is only there when something was left out,
    /// so a four-dimension vector reads as the whole of itself rather than as a truncation.
    pub fn summary(&self) -> String {
        let head: Vec<String> =
            self.values.iter().take(SHOWN).map(|value| format!("{value:.4}")).collect();
        let more = match self.values.len() > SHOWN {
            true => ", …",
            false => "",
        };
        format!("{}d · [{}{more}] · |v| {:.3}", self.values.len(), head.join(", "), self.norm())
    }

    /// The whole vector as JSON, which is what Copy puts on the clipboard.
    ///
    /// A JSON array rather than the engine's bytes, because what somebody does next with a copied
    /// embedding is paste it into something that reads numbers.
    pub fn as_json(&self) -> String {
        let numbers: Vec<String> = self.values.iter().map(|value| format!("{value}")).collect();
        format!("[{}]", numbers.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes_of(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|value| value.to_le_bytes()).collect()
    }

    #[test]
    fn a_vector_survives_the_round_trip_exactly() {
        // Byte for byte, because a viewer that showed a value the row does not hold would be worse
        // than one that showed nothing.
        let values = vec![0.5, -0.5, 1.0e-8, 12_345.75, 0.0];
        let bytes = bytes_of(&values);
        let read = Vector::decode(&bytes).expect("a vector");
        assert_eq!(read.values, values);
        assert_eq!(read.encode(), bytes);
    }

    #[test]
    fn what_is_not_a_vector_answers_none_rather_than_a_wrong_one() {
        // A length that is not a whole number of floats.
        assert!(Vector::decode(&[0, 1, 2]).is_none());
        assert!(Vector::decode(&[0, 1, 2, 3, 4]).is_none());
        // Too short to be worth the claim: one float is a number somebody stored as bytes.
        assert!(Vector::decode(&[0, 0, 0, 0]).is_none());
        assert!(Vector::decode(&[]).is_none());
        // A NaN or an infinity, which is what rejects most files that happen to be 4-aligned. No
        // embedder emits one, and a compressed format produces one within its first few words.
        assert!(Vector::decode(&bytes_of(&[1.0, f32::NAN])).is_none());
        assert!(Vector::decode(&bytes_of(&[1.0, f32::INFINITY])).is_none());
    }

    #[test]
    fn the_decoder_is_permissive_and_the_schema_is_what_actually_guards_this() {
        // Worth being exact about, because it is tempting to believe this function is cleverer than
        // it is. A PNG's first eight bytes are 4-aligned and decode to two perfectly finite floats:
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let read = Vector::decode(&png).expect("a PNG header really does decode");
        assert_eq!(read.dimensions(), 2);
        assert!(read.values.iter().all(|value| value.is_finite()));

        // So nothing is protected by this function, and nothing pretends to be. What decides that a
        // column holds a vector is the **schema** - a search index declares its width in its own
        // `%_config`, and `catalog::Table::is_a_vector_column` is the gate. A blob anywhere else is
        // decoded only when a person asks for it in the inspector, and then it is their question
        // rather than the grid's claim.
        let mut table = crate::catalog::Table::default();
        table.columns = vec![crate::value::Column::new("portrait", "BLOB")];
        assert!(!table.is_a_vector_column("portrait"));
    }

    #[test]
    fn the_summary_says_the_three_things_that_fit_in_a_cell() {
        let vector = Vector::decode(&bytes_of(&[0.5, -0.5, 0.5, 0.5])).expect("a vector");
        // Four dimensions and three shown, so one was left out and the ellipsis says so.
        assert_eq!(vector.summary(), "4d · [0.5000, -0.5000, 0.5000, …] · |v| 1.000");

        // A vector no longer than the summary shows is not written as though something were left out.
        let short = Vector::decode(&bytes_of(&[3.0, 4.0])).expect("a vector");
        assert_eq!(short.summary(), "2d · [3.0000, 4.0000] · |v| 5.000");
        assert!(!short.summary().contains('…'));
    }

    #[test]
    fn the_norm_is_what_says_a_corpus_was_stored_unnormalised() {
        // The engine's distance is cosine, so every vector in a normalised corpus reads 1.000 and one
        // that does not is visible at a glance. That is the whole reason it is in the summary.
        let unit = Vector::decode(&bytes_of(&[0.5, -0.5, 0.5, 0.5])).expect("a vector");
        assert!((unit.norm() - 1.0).abs() < 1.0e-6);
        let raw = Vector::decode(&bytes_of(&[3.0, 4.0])).expect("a vector");
        assert!((raw.norm() - 5.0).abs() < 1.0e-6);
    }

    #[test]
    fn the_inspectors_four_figures_are_the_vectors_own() {
        let vector = Vector::decode(&bytes_of(&[1.0, -3.0, 2.0, 4.0])).expect("a vector");
        assert_eq!(vector.dimensions(), 4);
        assert_eq!(vector.smallest(), -3.0);
        assert_eq!(vector.largest(), 4.0);
        assert_eq!(vector.mean(), 1.0);
    }

    #[test]
    fn copying_gives_numbers_rather_than_bytes() {
        let vector = Vector::decode(&bytes_of(&[0.5, -0.25])).expect("a vector");
        assert_eq!(vector.as_json(), "[0.5, -0.25]");
    }
}
