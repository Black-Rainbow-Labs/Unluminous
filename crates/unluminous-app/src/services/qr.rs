//! The QR code the Pair a Phone dialog shows (`task-30`).
//!
//! The `qrcode` crate is used with no image feature. It answers a square of dark and light modules, and
//! `components::pair_dialog` paints the squares itself, so nothing here makes a picture.
//!
//! The error correction level is M, which is the level `tasks/task-29-pairing-tdd.md` section 5.1 sizes the
//! link for: about 300 characters with four addresses is a version 13 code at level M.

use qrcode::{Color, EcLevel, QrCode};

/// A QR code as a square of modules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matrix {
    /// How many modules there are along one side, not counting the quiet zone.
    pub size: usize,
    /// Every module in rows from the top left, true for a dark one. `size * size` long.
    pub dark: Vec<bool>,
}

impl Matrix {
    /// Whether the module in column `x` of row `y` is dark. A position outside the square is light, which
    /// is what the quiet zone around the code is.
    /// @param x - the column, from the left
    /// @param y - the row, from the top
    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        x < self.size && y < self.size && self.dark[y * self.size + x]
    }

    /// The modules as rows of `#` for a dark one and `.` for a light one, one line a row. Used to store a
    /// matrix in a test and to read a failure.
    pub fn to_text(&self) -> String {
        let mut text = String::with_capacity(self.size * (self.size + 1));
        for y in 0..self.size {
            for x in 0..self.size {
                text.push(if self.is_dark(x, y) { '#' } else { '.' });
            }
            text.push('\n');
        }
        text
    }
}

/// The quiet zone a scanner needs around the code, in modules, on every side.
pub const QUIET_ZONE: usize = 4;

/// Encode `text` at error correction level M. `None` when it is too long for any QR code.
/// @param text - the link to put in the code
pub fn matrix(text: &str) -> Option<Matrix> {
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M).ok()?;
    let size = code.width();
    let dark = code.to_colors().into_iter().map(|color| color == Color::Dark).collect();
    Some(Matrix { size, dark })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed link and the matrix it must always make, so a change in how the code is encoded is a failing
    /// test. The link is the shape of a real one with four addresses.
    const LINK: &str = "unluminous://pair?v=1&o=AAAAAAAAAAAAAAAAAAAAAA&s=BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB&f=CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC&n=Test%20Mac&a=192.168.0.48:8196,Test-Mac.local:8196,test-mac.tail1234.ts.net:8196,100.75.142.76:8196";

    #[test]
    fn a_link_with_four_addresses_is_a_code_a_phone_can_read_at_this_size() {
        let matrix = matrix(LINK).expect("the link fits");
        // A version is 17 + 4 * version modules a side. 240 characters at level M is version 11, and the
        // design's "about 300 characters" is version 13, so anything from 10 to 14 is the right size.
        let version = (matrix.size - 17) / 4;
        assert!((10..=14).contains(&version), "{} characters made version {version}", LINK.len());
        assert_eq!(matrix.dark.len(), matrix.size * matrix.size);
    }

    #[test]
    fn the_matrix_for_a_fixed_link_is_the_stored_one() {
        let made = matrix("unluminous://pair?v=1&o=a").expect("a short link fits").to_text();
        // Git checks the file out with CRLF line endings on Windows (task-2879).
        let stored = include_str!("qr_fixed_matrix.txt").replace("\r\n", "\n");
        assert_eq!(made, stored, "the code made for the fixed link changed:\n{made}");
    }

    #[test]
    fn the_three_finder_squares_are_where_a_scanner_looks_for_them() {
        let matrix = matrix("unluminous://pair?v=1&o=a").expect("fits");
        let size = matrix.size;
        // A finder pattern is dark along its outer ring: seven modules across at three of the corners.
        for (left, top) in [(0, 0), (size - 7, 0), (0, size - 7)] {
            for step in 0..7 {
                assert!(matrix.is_dark(left + step, top), "top edge of the square at {left},{top}");
                assert!(matrix.is_dark(left + step, top + 6), "bottom edge");
                assert!(matrix.is_dark(left, top + step), "left edge");
                assert!(matrix.is_dark(left + 6, top + step), "right edge");
            }
        }
    }

    #[test]
    fn a_position_outside_the_square_is_light() {
        let matrix = matrix("x").expect("fits");
        assert!(!matrix.is_dark(matrix.size, 0));
        assert!(!matrix.is_dark(0, matrix.size));
    }

    #[test]
    fn text_too_long_for_any_code_is_refused() {
        assert!(matrix(&"x".repeat(5000)).is_none());
    }
}
