//! Paper sizes by the number `pageSetup/@paperSize` uses (`xlPaperSize` in Excel's object model).

/// One paper size, portrait, in millimetres (so a metric size is exact) or inches (so is a US one).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paper {
    pub code: u32,
    pub name: &'static str,
    /// Width in points, portrait.
    pub width: f64,
    /// Height in points, portrait.
    pub height: f64,
}

const fn inches(code: u32, name: &'static str, width: f64, height: f64) -> Paper {
    Paper { code, name, width: width * 72.0, height: height * 72.0 }
}

const fn mm(code: u32, name: &'static str, width: f64, height: f64) -> Paper {
    Paper { code, name, width: width * 72.0 / 25.4, height: height * 72.0 / 25.4 }
}

/// Excel's list of paper sizes. A size laid out wide (Ledger, the fanfolds) is listed as the sheet is, and
/// the orientation turns it.
pub const PAPERS: &[Paper] = &[
    inches(1, "Letter", 8.5, 11.0),
    inches(2, "Letter Small", 8.5, 11.0),
    inches(3, "Tabloid", 11.0, 17.0),
    inches(4, "Ledger", 17.0, 11.0),
    inches(5, "Legal", 8.5, 14.0),
    inches(6, "Statement", 5.5, 8.5),
    inches(7, "Executive", 7.25, 10.5),
    mm(8, "A3", 297.0, 420.0),
    mm(9, "A4", 210.0, 297.0),
    mm(10, "A4 Small", 210.0, 297.0),
    mm(11, "A5", 148.0, 210.0),
    mm(12, "B4 (JIS)", 257.0, 364.0),
    mm(13, "B5 (JIS)", 182.0, 257.0),
    inches(14, "Folio", 8.5, 13.0),
    mm(15, "Quarto", 215.0, 275.0),
    inches(16, "10x14", 10.0, 14.0),
    inches(17, "11x17", 11.0, 17.0),
    inches(18, "Note", 8.5, 11.0),
    inches(19, "Envelope #9", 3.875, 8.875),
    inches(20, "Envelope #10", 4.125, 9.5),
    inches(21, "Envelope #11", 4.5, 10.375),
    inches(22, "Envelope #12", 4.75, 11.0),
    inches(23, "Envelope #14", 5.0, 11.5),
    inches(24, "C size sheet", 17.0, 22.0),
    inches(25, "D size sheet", 22.0, 34.0),
    inches(26, "E size sheet", 34.0, 44.0),
    mm(27, "Envelope DL", 110.0, 220.0),
    mm(28, "Envelope C5", 162.0, 229.0),
    mm(29, "Envelope C3", 324.0, 458.0),
    mm(30, "Envelope C4", 229.0, 324.0),
    mm(31, "Envelope C6", 114.0, 162.0),
    mm(32, "Envelope C65", 114.0, 229.0),
    mm(33, "B4 (ISO)", 250.0, 353.0),
    mm(34, "B5 (ISO)", 176.0, 250.0),
    mm(35, "B6 (ISO)", 125.0, 176.0),
    mm(36, "Envelope Italy", 110.0, 230.0),
    inches(37, "Envelope Monarch", 3.875, 7.5),
    inches(38, "Envelope #6 3/4", 3.625, 6.5),
    inches(39, "US Standard Fanfold", 14.875, 11.0),
    inches(40, "German Standard Fanfold", 8.5, 12.0),
    inches(41, "German Legal Fanfold", 8.5, 13.0),
    mm(42, "B4 (ISO)", 250.0, 353.0),
    mm(43, "Japanese Postcard", 100.0, 148.0),
    inches(44, "9x11", 9.0, 11.0),
    inches(45, "10x11", 10.0, 11.0),
    inches(46, "15x11", 15.0, 11.0),
    mm(47, "Envelope Invite", 220.0, 220.0),
    inches(50, "Letter Extra", 9.5, 12.0),
    inches(51, "Legal Extra", 9.5, 15.0),
    inches(52, "Tabloid Extra", 11.69, 18.0),
    mm(53, "A4 Extra", 235.0, 322.0),
    inches(54, "Letter Transverse", 8.5, 11.0),
    mm(55, "A4 Transverse", 210.0, 297.0),
    inches(56, "Letter Extra Transverse", 9.5, 12.0),
    mm(57, "Super A", 227.0, 356.0),
    mm(58, "Super B", 305.0, 487.0),
    inches(59, "Letter Plus", 8.5, 12.69),
    mm(60, "A4 Plus", 210.0, 330.0),
    mm(61, "A5 Transverse", 148.0, 210.0),
    mm(62, "B5 (JIS) Transverse", 182.0, 257.0),
    mm(63, "A3 Extra", 322.0, 445.0),
    mm(64, "A5 Extra", 174.0, 235.0),
    mm(65, "B5 (ISO) Extra", 201.0, 276.0),
    mm(66, "A2", 420.0, 594.0),
    mm(67, "A3 Transverse", 297.0, 420.0),
    mm(68, "A3 Extra Transverse", 322.0, 445.0),
    mm(69, "Japanese Double Postcard", 200.0, 148.0),
    mm(70, "A6", 105.0, 148.0),
    mm(71, "Japanese Envelope Kaku #2", 240.0, 332.0),
    mm(72, "Japanese Envelope Kaku #3", 216.0, 277.0),
    mm(73, "Japanese Envelope Chou #3", 120.0, 235.0),
    mm(74, "Japanese Envelope Chou #4", 90.0, 205.0),
    mm(75, "Letter Rotated", 279.4, 215.9),
    mm(76, "A3 Rotated", 420.0, 297.0),
    mm(77, "A4 Rotated", 297.0, 210.0),
    mm(78, "A5 Rotated", 210.0, 148.0),
    mm(79, "B4 (JIS) Rotated", 364.0, 257.0),
    mm(80, "B5 (JIS) Rotated", 257.0, 182.0),
    mm(81, "Japanese Postcard Rotated", 148.0, 100.0),
    mm(82, "Double Japanese Postcard Rotated", 148.0, 200.0),
    mm(83, "A6 Rotated", 148.0, 105.0),
    mm(88, "B6 (JIS)", 128.0, 182.0),
    mm(89, "B6 (JIS) Rotated", 182.0, 128.0),
    inches(90, "12x11", 12.0, 11.0),
];

/// The paper a code names, or `None` for a code Excel does not list.
pub fn paper(code: u32) -> Option<Paper> {
    PAPERS.iter().copied().find(|p| p.code == code)
}

/// The size of a sheet in points, portrait, for a code; an unknown code is Letter.
pub fn size_of(code: u32) -> (f64, f64) {
    let p = paper(code).unwrap_or(PAPERS[0]);
    (p.width, p.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_common_sizes_are_what_their_names_say() {
        assert_eq!(size_of(1), (612.0, 792.0));
        assert_eq!(size_of(5), (612.0, 1008.0));
        let (w, h) = size_of(9);
        assert!((w - 595.28).abs() < 0.01 && (h - 841.89).abs() < 0.01);
    }

    #[test]
    fn an_unknown_code_is_letter() {
        assert_eq!(size_of(9999), (612.0, 792.0));
    }

    #[test]
    fn codes_are_listed_once() {
        let mut codes: Vec<u32> = PAPERS.iter().map(|p| p.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), PAPERS.len());
    }
}
