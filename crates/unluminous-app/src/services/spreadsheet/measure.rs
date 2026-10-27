//! The widths of a workbook's faces as the window drew them, for AutoFit to measure text with.
//!
//! AutoFit runs where a command or a key is handled, which has no fonts. The grid has them while it draws,
//! so each face it draws a cell in is remembered here: the advance of every ASCII character, in pixels at
//! 100% for each point of the font's size. AutoFit then measures a value's shown text the way Excel does,
//! from the face itself, rather than counting its characters (task-2378). A face nothing has drawn yet,
//! which is every face in a test with no window, is measured by the count of characters as before.

use std::cell::RefCell;
use std::collections::HashMap;

/// A face as a cell's font names it: its family, bold and italic.
type FaceKey = (String, bool, bool);

thread_local! {
    static FACES: RefCell<HashMap<FaceKey, [f32; 128]>> = RefCell::new(HashMap::new());
}

/// Whether a face's widths are known.
pub fn knows(family: &str, bold: bool, italic: bool) -> bool {
    FACES.with(|faces| faces.borrow().contains_key(&(family.to_owned(), bold, italic)))
}

/// Remember a face's widths: the advance of each ASCII character, in pixels at 100% for each point.
pub fn remember(family: &str, bold: bool, italic: bool, units: [f32; 128]) {
    FACES.with(|faces| {
        faces.borrow_mut().insert((family.to_owned(), bold, italic), units);
    });
}

/// How wide `text` is in pixels at 100% in a face at `size` points, when the face's widths are known. A
/// character past ASCII is measured as an `n`, which is near the average of a face's lower case letters.
pub fn width(family: &str, bold: bool, italic: bool, size: f64, text: &str) -> Option<f64> {
    FACES.with(|faces| {
        let faces = faces.borrow();
        let units = faces.get(&(family.to_owned(), bold, italic))?;
        let per_point: f64 = text
            .chars()
            .map(|c| f64::from(units[if c.is_ascii() { c as usize } else { usize::from(b'n') }]))
            .sum();
        Some(per_point * size)
    })
}
