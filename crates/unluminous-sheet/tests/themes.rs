//! Themes against Excel: applying a theme from the gallery gives a workbook the colours and fonts Excel
//! gives it, and the cells that name a theme colour or the theme's fonts then draw as Excel draws them.
//!
//! The fixtures were made by `tools/excel-oracle/style-fixtures.ps1`, which applies each theme to the same
//! workbook with `Workbook.ApplyTheme` and saves it.

mod describe;

use describe::*;
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::Workbook;
use unluminous_sheet::theme::{read_colors, read_fonts, BUILT_IN};

const LANE: &str = "styles";
const NOW: f64 = 46319.5;

fn session() -> Session {
    Session::new(read_fixture(LANE, "theme-input.xlsx"), NOW)
}

fn theme_part(book: &Workbook) -> String {
    let (_, bytes) = book
        .kept
        .parts
        .iter()
        .find(|(p, _)| p.starts_with("xl/theme/theme"))
        .expect("the workbook has a theme part");
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// What the cells draw: every colour a cell names, resolved against the workbook's theme, and every font name.
fn assert_same_as_excel(s: &Session, fixture: &str) {
    let excel = read_fixture(LANE, fixture);
    let ours = describe(&s.book);
    let theirs = describe(&excel);
    assert_eq!(ours.len(), theirs.len(), "{fixture}\n{}", differences(&ours, &theirs));
    for line in &ours {
        assert!(theirs.contains(line), "{fixture}\n{}", differences(&ours, &theirs));
    }
    assert_eq!(s.book.styles.theme, excel.styles.theme, "{fixture}: the twelve colours");
    assert_eq!(s.book.styles.theme_font, excel.styles.theme_font, "{fixture}: body font");
    assert_eq!(
        s.book.styles.theme_major_font, excel.styles.theme_major_font,
        "{fixture}: heading font"
    );
    let (ours, theirs) = (theme_part(&s.book), theme_part(&excel));
    let colors =
        |xml: &str| read_colors(xml).map(|c| c.slots.iter().map(|s| s.rgb()).collect::<Vec<_>>());
    assert_eq!(colors(&ours), colors(&theirs), "{fixture}: colour scheme of the theme part");
    let fonts = |xml: &str| read_fonts(xml).map(|f| (f.major, f.minor));
    assert_eq!(fonts(&ours), fonts(&theirs), "{fixture}: font scheme of the theme part");
}

#[test]
fn applying_a_theme_gives_the_colours_and_fonts_excel_gives() {
    let cases = [
        ("Facet", "theme-Facet.xlsx"),
        ("Office 2013 - 2022 Theme", "theme-Office_2013_-_2022_Theme.xlsx"),
        ("Office Theme", "theme-Office_Theme.xlsx"),
        ("Retrospect", "theme-Retrospect.xlsx"),
        ("Slice", "theme-Slice.xlsx"),
        ("Wisp", "theme-Wisp.xlsx"),
        ("Gallery", "theme-Gallery.xlsx"),
        ("Frame", "theme-Frame-new.xlsx"),
        ("Dividend", "theme-Dividend-new.xlsx"),
        ("Modulo", "theme-Modulo.xlsx"),
    ];
    assert!(cases.len() >= 6);
    for (name, fixture) in cases {
        let mut s = session();
        s.apply_theme(name).unwrap();
        assert_same_as_excel(&s, fixture);
    }
}

#[test]
fn a_theme_changes_what_a_theme_colour_reference_draws() {
    let mut s = session();
    let before = describe(&s.book);
    s.apply_theme("Facet").unwrap();
    let after = describe(&s.book);
    assert_ne!(before, after);
    // Accent 1 of Facet, as the cell styled Accent1 now draws it.
    assert!(
        after.iter().any(|l| l.contains("style=[Accent1]") && l.contains("fill=[#90C226]")),
        "{after:#?}"
    );
    s.undo();
    assert_eq!(describe(&s.book), before);
}

#[test]
fn a_colour_scheme_or_a_font_scheme_changes_only_its_half() {
    let mut s = session();
    s.apply_theme_colors("Blue Warm").unwrap();
    assert_same_as_excel(&s, "theme-colors-Blue_Warm.xlsx");
    let mut s = session();
    s.apply_theme_fonts("Calibri-Cambria").unwrap();
    assert_same_as_excel(&s, "theme-fonts-Calibri-Cambria.xlsx");
    assert!(session().apply_theme_colors("No such scheme").is_err());
}

#[test]
fn a_theme_changed_by_hand_is_what_excel_makes_of_it() {
    let mut s = session();
    // Accent 1 red and Accent 5 green, the heading font Georgia and the body font Courier New.
    s.set_theme_colors(&[(4, 0xFF0000), (8, 0x00FF00)], None).unwrap();
    s.set_theme_fonts(Some("Georgia"), Some("Courier New")).unwrap();
    assert_same_as_excel(&s, "theme-custom.xlsx");
    assert!(s.set_theme_colors(&[(12, 0)], None).is_err());
    assert!(s.set_theme_fonts(None, None).is_err());
}

#[test]
fn a_theme_survives_a_file_and_a_new_workbook_can_take_one() {
    let mut s = Session::new(Workbook::new(), NOW);
    s.set_input(0, unluminous_sheet::CellRef::parse("A1").unwrap(), "x").unwrap();
    s.apply_style(0, &[unluminous_sheet::Range::parse("A1").unwrap()], "Accent1").unwrap();
    s.apply_theme("Slice").unwrap();
    let back =
        unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    assert_eq!(describe(&back), describe(&s.book));
    assert_eq!(back.styles.theme, s.book.styles.theme);
    assert_eq!(back.styles.theme_font.as_deref(), Some("Century Gothic"));
    let colors = s.theme_colors().unwrap();
    assert_eq!(colors.slots.len(), 12);
    assert!(BUILT_IN.len() >= 6);
}

// ---------------------------------------------------------------------------------------------------
// Excel opens what the engine writes.

#[test]
fn excel_reads_the_theme_unluminous_writes() {
    if !have_excel("what Excel makes of a theme written here") {
        return;
    }
    let mut s = session();
    s.apply_theme("Facet").unwrap();
    s.set_theme_colors(&[(5, 0x123456)], None).unwrap();
    let dir = scratch("themes");
    let path = dir.join("built.xlsx");
    std::fs::write(&path, unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    let text = run_script("inspect-styles.ps1", &["-Path", path.to_str().unwrap()]);
    let excel: serde_json::Value =
        serde_json::from_str(text.trim().trim_start_matches('\u{FEFF}')).unwrap();
    let theme: Vec<String> = excel["themeColors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    // Excel reports each colour as a BGR integer in hexadecimal.
    let bgr = |rgb: u32| format!("{:06X}", (rgb & 0xFF) << 16 | (rgb & 0xFF00) | (rgb >> 16));
    let ours: Vec<String> = s.book.styles.theme.iter().map(|c| bgr(*c)).collect();
    assert_eq!(theme, ours);
    assert_eq!(excel["minorFont"], "Trebuchet MS");
    assert_eq!(excel["majorFont"], "Trebuchet MS");
}
