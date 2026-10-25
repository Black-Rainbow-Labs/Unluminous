//! Conditional formats in the file: the rules Excel keeps in its `x14` extension are read into the same
//! rule type as the others, written back where Excel puts them, and Excel opens what is written.
//!
//! `fixtures/conditional/cf-probe.xlsx` was made by `tools/excel-oracle/cf-probe.ps1`: a data bar with every
//! option, a plain one, all twenty icon sets, an icon set with custom icons, and two rules that name another
//! sheet. `05-condfmt.xlsx` and `16-sparklines.xlsx` are from the file audit.

use std::path::{Path, PathBuf};

use unluminous_sheet::coord::Range;
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::{CfRule, Cfvo, DataBarExt};
use unluminous_sheet::styles::Color;
use unluminous_sheet::Workbook;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conditional").join(name)
}

fn open(name: &str) -> Workbook {
    unluminous_sheet::xlsx::read(&std::fs::read(fixture(name)).unwrap()).unwrap()
}

/// The rule of a sheet that covers exactly `range`.
fn rule_at<'a>(book: &'a Workbook, sheet: usize, range: &str) -> &'a CfRule {
    let want = Range::parse(range).unwrap();
    book.sheets[sheet]
        .conditional
        .iter()
        .find(|cf| cf.ranges == vec![want])
        .unwrap_or_else(|| panic!("no rule over {range}"))
        .rules
        .first()
        .unwrap()
}

fn rgb_of(book: &Workbook, c: &Color) -> (u8, u8, u8) {
    c.resolve(&book.styles.theme).unwrap()
}

#[test]
fn the_extension_rules_excel_wrote_are_read_into_the_rule_type() {
    let book = open("cf-probe.xlsx");
    let bar = rule_at(&book, 0, "A1:A10");
    let options = bar.data_bar_options.as_ref().unwrap();
    assert!(options.has_extension && options.border && !options.gradient && !options.show_value);
    assert_eq!(options.direction, "rightToLeft");
    assert_eq!(options.axis_position, "middle");
    assert!(!options.negative_same_fill && !options.negative_same_border);
    assert_eq!(rgb_of(&book, options.border_color.as_ref().unwrap()), (255, 0, 0));
    assert_eq!(rgb_of(&book, options.negative_fill.as_ref().unwrap()), (0, 0, 255));
    assert_eq!(rgb_of(&book, options.negative_border.as_ref().unwrap()), (255, 255, 0));
    assert_eq!(rgb_of(&book, options.axis_color.as_ref().unwrap()), (0, 255, 0));
    assert_eq!((options.min_length, options.max_length), (0, 100));
    assert_eq!(bar.data_bar.as_ref().unwrap().0.kind, "num");
    assert!(bar.x14.as_ref().is_some_and(|x| !x.only && x.id.starts_with('{')));

    let plain = rule_at(&book, 0, "B1:B10").data_bar_options.as_ref().unwrap();
    assert_eq!(plain.axis_position, "none");
    assert!(plain.negative_same_fill && plain.gradient && !plain.border);

    let arrows = rule_at(&book, 0, "C1:C10");
    let set = arrows.icon_set.as_ref().unwrap();
    assert_eq!(set.name, "3Arrows");
    assert_eq!(set.thresholds.len(), 3);
    assert_eq!(set.thresholds[1].0.value.as_deref(), Some("33"));
    assert!(!set.reverse && set.show_value && arrows.x14.is_none());
    assert_eq!(rule_at(&book, 0, "F1:F10").icon_set.as_ref().unwrap().name, "3TrafficLights1");

    for (range, name, count) in
        [("T1:T10", "3Stars", 3), ("U1:U10", "3Triangles", 3), ("V1:V10", "5Boxes", 5)]
    {
        let rule = rule_at(&book, 0, range);
        let set = rule.icon_set.as_ref().unwrap();
        assert_eq!((set.name.as_str(), set.thresholds.len()), (name, count));
        assert!(rule.x14.as_ref().unwrap().only, "{name} is only in the extension");
        assert!(set.thresholds[1].0.value.is_some());
    }
    let custom = rule_at(&book, 0, "X1:X10").icon_set.as_ref().unwrap();
    assert!(!custom.show_value && custom.custom.len() == 3);
    assert!(!custom.thresholds[1].1, "the threshold is > rather than >=");

    let other_sheet = rule_at(&book, 0, "Y1:Y10");
    assert_eq!(other_sheet.formulas, vec!["Other!$A$1".to_owned()]);
    assert_eq!(other_sheet.operator.as_deref(), Some("greaterThan"));
    assert!(other_sheet.x14.as_ref().unwrap().only);
    let dxf = &book.styles.dxfs[other_sheet.dxf.unwrap() as usize];
    assert_eq!(rgb_of(&book, dxf.fill.as_ref().unwrap()), (255, 0, 0));
    assert_eq!(rule_at(&book, 0, "Z1:Z10").formulas, vec!["Z1>Other!$A$1".to_owned()]);
}

type Fingerprint = Vec<(Vec<Range>, CfRule, Option<(u8, u8, u8)>)>;

/// A sheet's rules with the format index replaced by the format's fill, so two reads can be compared.
fn fingerprint(book: &Workbook, sheet: usize) -> Fingerprint {
    book.sheets[sheet]
        .conditional
        .iter()
        .flat_map(|cf| {
            cf.rules.iter().map(|r| {
                let fill = r
                    .dxf
                    .and_then(|i| book.styles.dxfs.get(i as usize))
                    .and_then(|d| d.fill.as_ref().map(|c| rgb_of(book, c)));
                (cf.ranges.clone(), CfRule { dxf: None, ..r.clone() }, fill)
            })
        })
        .collect()
}

#[test]
fn the_rules_come_back_the_same_after_the_file_is_written() {
    for name in ["cf-probe.xlsx", "05-condfmt.xlsx", "cf-evaluate.xlsx"] {
        let book = open(name);
        let back =
            unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(&book).unwrap()).unwrap();
        for sheet in 0..book.sheets.len() {
            assert_eq!(
                fingerprint(&book, sheet),
                fingerprint(&back, sheet),
                "{name} sheet {sheet}"
            );
        }
        // Written again, the same rules come out: nothing drifts a pass at a time.
        let again =
            unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(&back).unwrap()).unwrap();
        for sheet in 0..book.sheets.len() {
            assert_eq!(fingerprint(&back, sheet), fingerprint(&again, sheet), "{name} second pass");
        }
    }
}

fn sheet_xml(bytes: &[u8], part: &str) -> String {
    let entries = unluminous_sheet::zip::read(bytes).unwrap();
    let entry = entries.iter().find(|e| e.name == part).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

#[test]
fn the_extension_is_written_where_excel_puts_it() {
    let book = open("cf-probe.xlsx");
    let sheet =
        sheet_xml(&unluminous_sheet::xlsx::write(&book).unwrap(), "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<x14:conditionalFormattings>"));
    assert!(sheet.contains("<x14:dataBar minLength=\"0\" maxLength=\"100\" border=\"1\""));
    assert!(sheet.contains("<x14:iconSet iconSet=\"3Stars\">"));
    assert!(sheet.contains("<x14:cfIcon iconSet=\"3TrafficLights1\" iconId=\"2\"/>"));
    assert!(sheet.contains("<xm:f>Other!$A$1</xm:f>"));
    assert_eq!(sheet.matches("<x14:id>").count(), 2, "each data bar names its twin");
    // The main part keeps the data bars and the 2007 icon sets, and none of the extension only rules.
    assert!(sheet.contains("<iconSet iconSet=\"3Arrows\">"));
    assert!(!sheet.contains("iconSet=\"3Stars\"><cfvo"));
    assert!(!sheet.contains("<formula>Other!"));
}

#[test]
fn extensions_that_are_not_conditional_formats_stay_beside_the_ones_made_here() {
    let mut s = Session::new(open("16-sparklines.xlsx"), 46319.5);
    let rule = CfRule {
        data_bar: Some((
            Cfvo { kind: "min".to_owned(), value: None },
            Cfvo { kind: "max".to_owned(), value: None },
            Color::rgb(0x63, 0x8E, 0xC6),
        )),
        data_bar_options: Some(DataBarExt::default()),
        ..CfRule::new("dataBar")
    };
    s.add_conditional(0, vec![Range::parse("A1:A3").unwrap()], rule).unwrap();
    let sheet =
        sheet_xml(&unluminous_sheet::xlsx::write(&s.book).unwrap(), "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("x14:sparklineGroups"), "the sparklines were kept");
    assert!(sheet.contains("x14:conditionalFormattings"), "the new data bar has its twin");
    assert_eq!(sheet.matches("<extLst>").count(), 2, "one in the rule and one for the sheet");
    // The file reads back with the data bar and its options.
    let back =
        unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    let again = rule_at(&back, 0, "A1:A3");
    assert_eq!(again.data_bar_options, Some(DataBarExt::default()));
}

fn excel_is_installed() -> bool {
    cfg!(windows)
        && std::process::Command::new("reg")
            .args(["query", "HKCR\\Excel.Application"])
            .output()
            .is_ok_and(|o| o.status.success())
}

/// Ask Excel what it sees in a file.
fn inspect(path: &Path) -> serde_json::Value {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/excel-oracle/inspect.ps1");
    let output = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(&script)
        .arg("-Path")
        .arg(path)
        .output()
        .expect("pwsh runs");
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim().trim_start_matches('\u{FEFF}');
    serde_json::from_str(text).unwrap_or_else(|e| {
        panic!(
            "Excel's answer did not read: {e}\n{text}\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn excel_opens_the_rules_written_here_without_repairing_the_file() {
    if !excel_is_installed() {
        assert!(
            std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err(),
            "UNLUMINOUS_REQUIRE_EXCEL is set and Excel is not installed"
        );
        println!("Excel is not installed here, so what it makes of the rules was not checked");
        return;
    }
    let folder = std::env::temp_dir().join(format!("unluminous-cf-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    for name in ["cf-probe.xlsx", "05-condfmt.xlsx"] {
        let book = open(name);
        let path = folder.join(name);
        std::fs::write(&path, unluminous_sheet::xlsx::write(&book).unwrap()).unwrap();
        let excel = inspect(&path);
        assert_eq!(
            excel["repaired"], false,
            "{name}: Excel repaired the file: {}",
            excel["problem"]
        );
        let ours: usize = book.sheets[0].conditional.iter().map(|c| c.rules.len()).sum();
        // Excel's count leaves out rules of kinds it does not list for the used range in the other file.
        if name != "cf-probe.xlsx" {
            continue;
        }
        assert_eq!(
            excel["sheets"][0]["features"]["conditional"].as_u64().unwrap() as usize,
            ours,
            "{name}: Excel counts the rules of the first sheet"
        );
        let _ = std::fs::remove_file(&path);
    }
    let _ = std::fs::remove_dir(&folder);
}
