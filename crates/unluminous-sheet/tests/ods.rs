//! `.ods` files: what Excel writes reads as the workbook it saved, what Unluminous writes reads back, and
//! Excel opens it.
//!
//! `fixtures/ods/` holds each feature workbook as Excel saved it (`.ods`) beside the `.xlsx` it was saved
//! from, made by `tools/excel-oracle/save-ods.ps1`. The tests read both and compare the cells. Excel does
//! not write everything an `.xlsx` holds into an `.ods` (a pattern fill, a dash dot border, a table, a
//! hidden name, a sheet that is very hidden), so the comparison allows exactly the differences Excel itself
//! makes, and each is named in `lossy`.
//!
//! The last tests start Excel through `tools/excel-oracle/inspect.ps1` when it is installed, and pass with a
//! printed line when it is not; `UNLUMINOUS_REQUIRE_EXCEL=1` makes a missing Excel a failure.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::{LinkTarget, Visibility, Workbook};
use unluminous_sheet::numfmt::Format;
use unluminous_sheet::styles::{BorderEdit, Color, Edge, FontEdit, Sides, StyleChange};
use unluminous_sheet::{Sheet, Value};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ods")
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(name)).unwrap()
}

/// The names of the feature workbooks Excel saved both ways.
fn names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixtures())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".ods").map(str::to_owned))
        .collect();
    names.sort();
    names
}

fn xlsx_of(name: &str) -> Workbook {
    for ext in ["xlsx", "xlsm"] {
        if let Ok(bytes) = std::fs::read(fixtures().join(format!("{name}.{ext}"))) {
            return unluminous_sheet::xlsx::read(&bytes).unwrap();
        }
    }
    panic!("no workbook for {name}");
}

fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0),
        (Value::Empty, Value::Text(t)) | (Value::Text(t), Value::Empty) => t.is_empty(),
        _ => a == b,
    }
}

/// A cell's look as text. With `lossy`, the look is cut down to what an `.ods` file can hold, which is how
/// Excel writes one: pattern fills are lost, a line drawn with dashes and dots is drawn plain, and
/// alignments an `.ods` has no word for are the default.
fn look(book: &Workbook, xf: u32, lossy: bool) -> String {
    let s = &book.styles;
    let (mut font, mut fill, mut border, mut format) = (s.font_of(xf), s.fill_of(xf), s.border_of(xf), s.xf(xf));
    let near = |c: &Color| {
        let hex = c.to_hex(&s.theme);
        match u32::from_str_radix(hex.trim_start_matches('#'), 16) {
            Ok(rgb) => format!("#{:06X}", rgb & 0x00FC_FCFC),
            Err(_) => hex,
        }
    };
    if lossy {
        if fill.pattern != "solid" {
            fill = unluminous_sheet::styles::Fill::none();
        }
        if matches!(font.underline, unluminous_sheet::styles::Underline::SingleAccounting) {
            font.underline = unluminous_sheet::styles::Underline::Single;
        }
        if matches!(font.underline, unluminous_sheet::styles::Underline::DoubleAccounting) {
            font.underline = unluminous_sheet::styles::Underline::Double;
        }
        if matches!(format.align.horizontal.as_deref(), Some("fill" | "distributed" | "centerContinuous")) {
            format.align.horizontal = Some("left".to_owned());
        }
        if matches!(format.align.vertical.as_deref(), Some("justify" | "distributed")) {
            format.align.vertical = None;
        }
        for edge in [&mut border.left, &mut border.right, &mut border.top, &mut border.bottom, &mut border.diagonal] {
            if matches!(
                edge.style.as_str(),
                "hair" | "dashDot" | "mediumDashDot" | "dashDotDot" | "mediumDashDotDot" | "slantDashDot"
            ) {
                edge.style = "thin".to_owned();
            }
        }
    }
    let edge = |e: &Edge| if e.is_none() { "none".to_owned() } else { format!("{}/{}", e.style, near(&e.color)) };
    format!(
        "font {} {} b{} i{} u{:?} s{} {} | fill {:?} | border {} {} {} {} diag {} {}{} | align {:?} {:?} wrap{} indent{} rot{} shrink{} | lock {:?} hide {:?}",
        font.name,
        font.size,
        font.bold,
        font.italic,
        font.underline,
        font.strike,
        near(&font.color),
        fill.paint().map(|c| near(&c)),
        edge(&border.left),
        edge(&border.right),
        edge(&border.top),
        edge(&border.bottom),
        edge(&border.diagonal),
        border.diagonal_up,
        border.diagonal_down,
        format.align.horizontal,
        format.align.vertical,
        format.align.wrap,
        format.align.indent,
        format.align.rotation,
        format.align.shrink,
        format.locked,
        format.hidden,
    )
}

/// A formula as text without what two readings of one formula legitimately differ by: an implicit
/// intersection mark, which a file written by Excel adds to a formula written before dynamic arrays.
fn formula_text(cell: Option<&unluminous_sheet::Cell>) -> Option<String> {
    cell.and_then(|c| c.formula.as_ref()).map(|f| f.display().replace('@', ""))
}

/// How the cells of two readings of one workbook differ, as text. `a` is the reading of an `.ods` file and
/// `b` is the reading of the workbook it was saved from when `lossy`, or another reading of an `.ods` file.
fn differences(a: &Workbook, b: &Workbook, lossy: bool) -> Vec<String> {
    let mut out = Vec::new();
    let sheets_a: Vec<_> = a.sheets.iter().map(|s| s.name.clone()).collect();
    let sheets_b: Vec<_> = b.sheets.iter().map(|s| s.name.clone()).collect();
    if sheets_a != sheets_b {
        return vec![format!("sheets {sheets_a:?} against {sheets_b:?}")];
    }
    for (index, (sa, sb)) in a.sheets.iter().zip(&b.sheets).enumerate() {
        let mut keys: BTreeSet<(u32, u32)> = sa.cells.keys().chain(sb.cells.keys()).copied().collect();
        keys.extend(sa.spills.values.keys().chain(sb.spills.values.keys()));
        for (r, c) in keys {
            let at = CellRef::new(r, c);
            let place = format!("{}!{}", sa.name, at.a1());
            let (va, vb) = (sa.value_at(at), sb.value_at(at));
            if !same_value(&va, &vb) {
                out.push(format!("{place} value {va:?} against {vb:?}"));
            }
            let (fa, fb) = (formula_text(sa.cell(at)), formula_text(sb.cell(at)));
            // A structured reference reads as the cells it names in an `.ods` file, which has no tables.
            let structured = fb.as_ref().is_some_and(|f| f.contains('['));
            if fa != fb && !(lossy && structured) {
                out.push(format!("{place} formula {fa:?} against {fb:?}"));
            }
            let (xa, xb) = (sa.cell(at).map_or(0, |x| x.style), sb.cell(at).map_or(0, |x| x.style));
            let (ca, cb) = (a.styles.format_code(xa), b.styles.format_code(xb));
            let shown = |code: &str, book: &Workbook, v: &Value| Format::parse(code).render(v, book.date1904, 255).text;
            let (ta, tb) = (shown(&ca, a, &va), shown(&cb, b, &vb));
            if ta != tb {
                out.push(format!("{place} shows {ta:?} ({ca}) against {tb:?} ({cb})"));
            }
            let (la, lb) = (look(a, xa, false), look(b, xb, lossy));
            if la != lb {
                out.push(format!("{place} look {la}\n      against {lb}"));
            }
        }
        out.extend(feature_differences(a, b, index, lossy));
    }
    out
}

/// A tab colour as red, green and blue with the low bits dropped.
fn tint(book: &Workbook, color: Option<Color>) -> String {
    color.map_or("none".to_owned(), |c| {
        let hex = c.to_hex(&book.styles.theme);
        let rgb = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
        format!("#{:06X}", rgb & 0x00FC_FCFC)
    })
}

/// A defined name as text, leaving out the names an `.ods` file has no place for.
fn name_list(book: &Workbook, lossy: bool) -> Vec<String> {
    let mut names: Vec<String> = book
        .names
        .iter()
        .filter(|n| !lossy || (!n.hidden && !n.name.contains("_FilterDatabase") && !n.name.contains("Print_Titles") && !n.name.starts_with("Slicer_")))
        .map(|d| format!("{:?}:{}={}", d.scope, d.name.trim_start_matches("_xlnm."), d.formula))
        .collect();
    names.sort();
    names
}

/// Merges, sizes, visibility, notes, links, validations, conditional formats and names.
fn feature_differences(a: &Workbook, b: &Workbook, index: usize, lossy: bool) -> Vec<String> {
    let (sa, sb) = (&a.sheets[index], &b.sheets[index]);
    let mut out = Vec::new();
    let name = &sa.name;
    let mut note = |what: &str, x: String, y: String| {
        if x != y {
            out.push(format!("{name} {what}: {x} against {y}"));
        }
    };
    let merges = |s: &Sheet| {
        let mut m = s.merges.clone();
        m.sort_by_key(|r| (r.first.row, r.first.col));
        format!("{m:?}")
    };
    note("merges", merges(sa), merges(sb));
    let state = |s: &Sheet| if lossy && s.state == Visibility::VeryHidden { Visibility::Hidden } else { s.state };
    note("hidden", format!("{:?}", sa.state), format!("{:?}", state(sb)));
    note("tab color", tint(a, sa.tab_color), tint(b, sb.tab_color));
    let hidden_rows = |s: &Sheet| s.rows.iter().filter(|(_, i)| i.hidden).map(|(r, _)| *r).collect::<Vec<_>>();
    note("hidden rows", format!("{:?}", hidden_rows(sa)), format!("{:?}", hidden_rows(sb)));
    let hidden_cols = |s: &Sheet| (0..64).filter(|c| s.is_column_hidden(*c)).collect::<Vec<_>>();
    note("hidden columns", format!("{:?}", hidden_cols(sa)), format!("{:?}", hidden_cols(sb)));
    let widths = |s: &Sheet| {
        (0..32)
            .filter_map(|c| s.column(c).and_then(|i| i.width.filter(|_| i.custom_width)).map(|w| (c, w.round())))
            .collect::<Vec<_>>()
    };
    note("custom widths", format!("{:?}", widths(sa)), format!("{:?}", widths(sb)));
    let heights = |s: &Sheet| {
        s.rows.iter().filter_map(|(r, i)| i.height.map(|h| (*r, (h * 10.0).round() / 10.0))).collect::<Vec<_>>()
    };
    note("row heights", format!("{:?}", heights(sa)), format!("{:?}", heights(sb)));
    let notes = |s: &Sheet| {
        s.notes
            .iter()
            .map(|(k, n)| {
                let text = n.text.replace("\r\n", "\n");
                // The text Excel puts in front of a threaded comment for older versions is not in an `.ods`.
                let text = match text.find("Comment:") {
                    Some(at) if text.starts_with("[Threaded comment]") => text[at..].to_owned(),
                    _ => text,
                };
                (*k, text)
            })
            .collect::<Vec<_>>()
    };
    note("notes", format!("{:?}", notes(sa)), format!("{:?}", notes(sb)));
    let links = |s: &Sheet| {
        s.links
            .iter()
            .take(3)
            .map(|l| (l.range.a1(), format!("{:?}", l.target).replace('\'', "")))
            .collect::<Vec<_>>()
    };
    note("links", format!("{:?}", links(sa)), format!("{:?}", links(sb)));
    let validations = |s: &Sheet, other: &Sheet| {
        let known: BTreeSet<String> = other.validations.iter().flat_map(|v| v.ranges.iter().map(|r| r.a1())).collect();
        let mut v: Vec<String> = s
            .validations
            .iter()
            .flat_map(|v| {
                v.ranges.iter().map(move |r| {
                    let operator = v.operator.clone().or_else(|| (!matches!(v.kind.as_str(), "list" | "custom")).then(|| "between".to_owned()));
                    (r.a1(), format!("{} {} {:?} {:?} {:?}", r.a1(), v.kind, operator, v.formula1, v.formula2))
                })
            })
            .filter(|(range, _)| !lossy || known.contains(range))
            .map(|(_, text)| text)
            .collect();
        v.sort();
        v
    };
    note("validations", format!("{:#?}", validations(sa, sb)), format!("{:#?}", validations(sb, sa)));
    let rules = |s: &Sheet| s.conditional.iter().map(|c| c.rules.len()).sum::<usize>();
    if lossy {
        // An `.ods` file written by Excel holds the rules that are a comparison or a formula.
        let fine = !(rules(sa) > rules(sb) || (rules(sb) > 0) != (rules(sa) > 0));
        let (x, y) = if fine {
            ("fine".to_owned(), "fine".to_owned())
        } else {
            (rules(sa).to_string(), rules(sb).to_string())
        };
        note("conditional formats", x, y);
    } else {
        note("conditional formats", rules(sa).to_string(), rules(sb).to_string());
    }
    note("frozen", format!("{:?}", sa.frozen), format!("{:?}", sb.frozen));
    if index == 0 {
        note("names", format!("{:#?}", name_list(a, lossy)), format!("{:#?}", name_list(b, lossy)));
    }
    out
}

#[test]
fn a_workbook_excel_saved_as_ods_reads_as_the_workbook_it_saved() {
    let mut wrong = Vec::new();
    assert!(names().len() >= 15, "the fixtures are missing");
    for name in names() {
        let ods = unluminous_sheet::ods::read(&fixture(&format!("{name}.ods"))).unwrap();
        let diff = differences(&ods, &xlsx_of(&name), true);
        wrong.extend(diff.into_iter().map(|d| format!("{name}: {d}")));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn what_excel_wrote_comes_back_the_same_after_unluminous_writes_it_as_ods() {
    let mut wrong = Vec::new();
    for name in names() {
        let first = unluminous_sheet::ods::read(&fixture(&format!("{name}.ods"))).unwrap();
        let second = unluminous_sheet::ods::read(&unluminous_sheet::ods::write(&first).unwrap()).unwrap();
        wrong.extend(differences(&first, &second, false).into_iter().map(|d| format!("{name}: {d}")));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn an_ods_file_starts_with_its_mimetype_stored_and_lists_its_parts() {
    let bytes = unluminous_sheet::ods::write(&Workbook::new()).unwrap();
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    // The first entry is stored, so its text is at a fixed place near the start of the file.
    let text = String::from_utf8_lossy(&bytes[..200]).into_owned();
    assert!(text.contains("mimetypeapplication/vnd.oasis.opendocument.spreadsheet"), "{text}");
    let entries = unluminous_sheet::zip::read(&bytes).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["mimetype", "META-INF/manifest.xml", "content.xml", "styles.xml", "settings.xml", "meta.xml"]);
}

#[test]
fn a_file_that_is_not_a_spreadsheet_is_refused_with_a_sentence() {
    let entries = vec![unluminous_sheet::zip::Entry {
        name: "mimetype".to_owned(),
        data: b"application/vnd.oasis.opendocument.text".to_vec(),
    }];
    let bytes = unluminous_sheet::zip::write(&entries).unwrap();
    let problem = unluminous_sheet::ods::read(&bytes).unwrap_err();
    assert!(problem.contains("not a spreadsheet"), "{problem}");
    let no_content = unluminous_sheet::zip::write(&[unluminous_sheet::zip::Entry { name: "x".into(), data: vec![] }]).unwrap();
    assert!(unluminous_sheet::ods::read(&no_content).unwrap_err().contains("content.xml"));
    assert!(unluminous_sheet::ods::read(b"not a zip at all, just text").is_err());
}

/// A workbook with one of everything an `.ods` file holds.
fn built() -> Session {
    let mut s = Session::new(Workbook::new(), 46318.5);
    let at = |a: &str| CellRef::parse(a).unwrap();
    let rows = [["Item", "Qty", "Price", "Total"], ["Tea", "3", "2.5", "=B2*C2"], ["Coffee", "2", "4", "=B3*C3"], ["Cake", "5", "3.25", "=B4*C4"]];
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            s.set_input(0, CellRef::new(r as u32, c as u32), v).unwrap();
        }
    }
    s.set_input(0, at("D5"), "=SUM(D2:D4)").unwrap();
    s.set_input(0, at("F1"), "=SORT(A2:A4)").unwrap();
    s.set_input(0, at("K1"), "=SEQUENCE(2,2)").unwrap();
    s.set_input(0, at("H1"), "10/23/2026").unwrap();
    s.set_input(0, at("H2"), "=TEXT(H1,\"dddd\")").unwrap();
    s.set_input(0, at("H3"), "12.5%").unwrap();
    s.set_input(0, at("H4"), "=IF(B2>2,\"many\",\"few\")&\" \"&A2").unwrap();
    s.set_input(0, at("I1"), "TRUE").unwrap();
    s.set_input(0, at("I2"), "=1/0").unwrap();
    s.set_input(0, at("I3"), "=NOT(I1)").unwrap();
    s.set_style(0, &[Range::parse("A1:D1").unwrap()], &StyleChange::Font(FontEdit { bold: Some(true), color: Some(Color::rgb(0x1F, 0x4E, 0x79)), ..Default::default() })).unwrap();
    s.set_style(0, &[Range::parse("A1:D1").unwrap()], &StyleChange::Fill(Some(Color::rgb(0xDD, 0xEB, 0xF7)))).unwrap();
    s.set_style(0, &[Range::parse("C2:D5").unwrap()], &StyleChange::NumberFormat("$#,##0.00".to_owned())).unwrap();
    s.set_style(0, &[Range::parse("H3").unwrap()], &StyleChange::NumberFormat("0.0%".to_owned())).unwrap();
    s.set_style(0, &[Range::parse("A5:D5").unwrap()], &StyleChange::Border(BorderEdit { sides: Sides::Top, edge: Edge::line("double", Color::Auto) })).unwrap();
    s.set_input(0, at("A7"), "Merged").unwrap();
    s.merge(0, Range::parse("A7:C7").unwrap(), true).unwrap();
    s.freeze(0, 1, 1).unwrap();
    s.set_column_width(0, 0, 0, Some(16.0)).unwrap();
    s.set_row_height(0, 6, 6, Some(24.0)).unwrap();
    s.set_note(0, at("A2"), Some("Green tea"), "Jason").unwrap();
    s.set_link(0, Range::parse("A9").unwrap(), Some(LinkTarget::External("https://example.com/".to_owned())), Some("Example".to_owned())).unwrap();
    s.set_link(0, Range::parse("A10").unwrap(), Some(LinkTarget::Location("Notes!A1".to_owned())), Some("To notes".to_owned())).unwrap();
    s.add_validation(0, validation("B12", "list", None, "\"Low,Medium,High\"", None)).unwrap();
    s.add_validation(0, validation("B13:B15", "whole", Some("between"), "1", Some("10"))).unwrap();
    let dxf = {
        s.book.styles.dxfs.push(unluminous_sheet::styles::Dxf {
            fill: Some(Color::rgb(0xC6, 0xEF, 0xCE)),
            font_color: Some(Color::rgb(0, 0x61, 0)),
            bold: Some(true),
            ..Default::default()
        });
        (s.book.styles.dxfs.len() - 1) as u32
    };
    s.add_conditional(0, vec![Range::parse("D2:D4").unwrap()], rule("cellIs", Some("greaterThan"), &["10"], dxf)).unwrap();
    s.add_conditional(0, vec![Range::parse("B2:B4").unwrap()], rule("expression", None, &["MOD(B2,2)=1"], dxf)).unwrap();
    s.set_name("Taxrate", "0.2", None).unwrap();
    s.set_name("Quantities", "Sheet1!$B$2:$B$4", None).unwrap();
    let notes = s.add_sheet(Some("Notes"), None).unwrap();
    s.set_input(notes, at("A1"), "Total from orders").unwrap();
    s.set_input(notes, at("B1"), "=Sheet1!D5*Taxrate").unwrap();
    s.set_input(notes, at("A2"), "Line one\nline two  spaced").unwrap();
    s.set_tab_color(notes, Some(Color::rgb(0xFF, 0xC0, 0))).unwrap();
    let hidden = s.add_sheet(Some("Hidden one"), None).unwrap();
    s.set_sheet_visibility(hidden, Visibility::Hidden).unwrap();
    s.set_input(hidden, at("A1"), "=SUM(Quantities)").unwrap();
    s.book.active_sheet = 0;
    s
}

fn validation(range: &str, kind: &str, operator: Option<&str>, first: &str, second: Option<&str>) -> unluminous_sheet::model::Validation {
    unluminous_sheet::model::Validation {
        ranges: vec![Range::parse(range).unwrap()],
        kind: kind.to_owned(),
        operator: operator.map(str::to_owned),
        formula1: Some(first.to_owned()),
        formula2: second.map(str::to_owned),
        allow_blank: true,
        show_dropdown: true,
        prompt_title: Some("Pick".to_owned()),
        prompt: Some("Choose one".to_owned()),
        error_title: Some("Bad".to_owned()),
        error: Some("Not allowed".to_owned()),
        error_style: Some("warning".to_owned()),
        attrs: Vec::new(),
    }
}

fn rule(kind: &str, operator: Option<&str>, formulas: &[&str], dxf: u32) -> unluminous_sheet::model::CfRule {
    unluminous_sheet::model::CfRule {
        kind: kind.to_owned(),
        dxf: Some(dxf),
        priority: 1,
        stop: false,
        operator: operator.map(str::to_owned),
        text: None,
        rank: None,
        percent: false,
        bottom: false,
        above_average: None,
        formulas: formulas.iter().map(|f| (*f).to_owned()).collect(),
        color_scale: None,
        data_bar: None,
        attrs: Vec::new(),
        raw_children: String::new(),
    }
}

#[test]
#[ignore = "writes the built workbook for a person to open: set UNLUMINOUS_ODS_OUT to a file name"]
fn write_the_built_workbook_for_a_person() {
    let out = PathBuf::from(std::env::var("UNLUMINOUS_ODS_OUT").unwrap());
    std::fs::write(out, unluminous_sheet::ods::write(&built().book).unwrap()).unwrap();
}

#[test]
fn a_workbook_built_here_keeps_its_features_through_an_ods_file() {
    let session = built();
    let bytes = unluminous_sheet::ods::write(&session.book).unwrap();
    let back = unluminous_sheet::ods::read(&bytes).unwrap();
    let diff = differences(&session.book, &back, false);
    assert!(diff.is_empty(), "{}", diff.join("\n"));
    let s = &back.sheets[0];
    assert_eq!(s.frozen, Some((1, 1)));
    assert_eq!(s.merges, vec![Range::parse("A7:C7").unwrap()]);
    assert!((s.column(0).and_then(|c| c.width).unwrap() - 16.0).abs() < 0.01);
    assert_eq!(s.rows.get(&6).and_then(|r| r.height), Some(24.0));
    assert_eq!(s.notes.get(&(1, 0)).map(|n| (n.author.as_str(), n.text.as_str())), Some(("Jason", "Green tea")));
    assert_eq!(s.links.len(), 2);
    assert_eq!(s.validations.len(), 2);
    assert_eq!(s.validations[0].formula1.as_deref(), Some("\"Low,Medium,High\""));
    assert_eq!(s.validations[1].operator.as_deref(), Some("between"));
    assert_eq!(s.validations[1].error_style.as_deref(), Some("warning"));
    assert_eq!(s.conditional.len(), 2);
    assert_eq!(s.conditional[0].rules[0].operator.as_deref(), Some("greaterThan"));
    assert_eq!(s.conditional[1].rules[0].formulas, vec!["MOD(B2,2)=1"]);
    assert_eq!(s.spills.range_of((0, 5)), Some(Range::parse("F1:F3").unwrap()), "the sorted names are a spill");
    assert_eq!(back.sheets[1].name, "Notes");
    assert_eq!(back.sheets[1].value_at(CellRef::parse("A2").unwrap()), Value::text("Line one\nline two  spaced"));
    assert_eq!(back.sheets[2].state, Visibility::Hidden);
    assert!(back.names.iter().any(|n| n.name == "Taxrate" && n.formula == "0.2"));
    assert!(back.names.iter().any(|n| n.name == "Quantities" && n.formula == "Sheet1!$B$2:$B$4"));
    assert!(back.sheets[1].tab_color.is_some());
}

/// A file as LibreOffice writes one: it has `settings.xml`, conditional formats in `calcext`, a print
/// range, a filter, and number styles with a locale.
fn libreoffice_style_file() -> Vec<u8> {
    let content = r##"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:number="urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0" xmlns:calcext="urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" office:version="1.3">
<office:font-face-decls><style:font-face style:name="Liberation Sans" svg:font-family="'Liberation Sans'"/></office:font-face-decls>
<office:automatic-styles>
<number:percentage-style style:name="N11"><number:number number:decimal-places="1" number:min-decimal-places="1" number:min-integer-digits="1"/><number:text>%</number:text></number:percentage-style>
<number:currency-style style:name="N105P0" style:volatile="true"><number:number number:decimal-places="2" number:min-integer-digits="1" number:grouping="true"/><number:text> </number:text><number:currency-symbol number:language="de" number:country="DE">&#8364;</number:currency-symbol></number:currency-style>
<number:currency-style style:name="N105"><style:text-properties fo:color="#ff0000"/><number:text>-</number:text><number:number number:decimal-places="2" number:min-integer-digits="1" number:grouping="true"/><number:text> </number:text><number:currency-symbol number:language="de" number:country="DE">&#8364;</number:currency-symbol><style:map style:condition="value()&gt;=0" style:apply-style-name="N105P0"/></number:currency-style>
<number:date-style style:name="N37"><number:day number:style="long"/><number:text>.</number:text><number:month number:style="long"/><number:text>.</number:text><number:year number:style="long"/></number:date-style>
<number:number-style style:name="N60"><number:scientific-number number:decimal-places="2" number:min-integer-digits="1" number:min-exponent-digits="2"/></number:number-style>
<style:style style:name="co1" style:family="table-column"><style:table-column-properties style:column-width="2.5cm"/></style:style>
<style:style style:name="co2" style:family="table-column"><style:table-column-properties style:column-width="5.081cm"/></style:style>
<style:style style:name="ro1" style:family="table-row"><style:table-row-properties style:row-height="0.452cm"/></style:style>
<style:style style:name="ro2" style:family="table-row"><style:table-row-properties style:row-height="1.2cm"/></style:style>
<style:style style:name="ta1" style:family="table"><style:table-properties table:display="true"/></style:style>
<style:style style:name="ta2" style:family="table"><style:table-properties table:display="false"/></style:style>
<style:style style:name="ce1" style:family="table-cell" style:parent-style-name="Default" style:data-style-name="N11"/>
<style:style style:name="ce2" style:family="table-cell" style:parent-style-name="Default" style:data-style-name="N105"/>
<style:style style:name="ce3" style:family="table-cell" style:parent-style-name="Default" style:data-style-name="N37"/>
<style:style style:name="ce4" style:family="table-cell" style:parent-style-name="Default" style:data-style-name="N60"/>
<style:style style:name="ce5" style:family="table-cell" style:parent-style-name="Default"><style:table-cell-properties fo:background-color="#ffff00" fo:border="0.74pt solid #000000" fo:wrap-option="wrap" style:rotation-angle="90"/><style:paragraph-properties fo:text-align="center"/><style:text-properties fo:font-weight="bold" fo:font-style="italic" style:text-underline-style="solid" style:text-underline-type="double" fo:color="#0000ff" fo:font-size="14pt" style:font-name="Liberation Sans"/></style:style>
<style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="bold" fo:color="#ff0000"/></style:style>
<style:style style:name="cfgood" style:family="table-cell"><style:table-cell-properties fo:background-color="#c6efce"/><style:text-properties fo:color="#006100"/></style:style>
</office:automatic-styles>
<office:body><office:spreadsheet>
<table:calculation-settings><table:null-date table:date-value="1899-12-30"/></table:calculation-settings>
<table:content-validations>
 <table:content-validation table:name="val1" table:condition="of:cell-content-is-in-list([$Lists.$A$1:.$A$3])" table:allow-empty-cell="false" table:display-list="unsorted" table:base-cell-address="Data.B2"><table:error-message table:display="true" table:message-type="stop" table:title="Oops"><text:p>Pick from the list</text:p></table:error-message></table:content-validation>
 <table:content-validation table:name="val2" table:condition="of:cell-content-is-decimal-number() and cell-content-is-between(0;1)" table:base-cell-address="Data.C2"/>
</table:content-validations>
<table:table table:name="Data" table:style-name="ta1" table:print-ranges="Data.A1:Data.D20">
<calcext:conditional-formats>
 <calcext:conditional-format calcext:target-range-address="Data.C2:Data.C5"><calcext:condition calcext:apply-style-name="cfgood" calcext:value="&gt;5" calcext:base-cell-address="Data.C2"/></calcext:conditional-format>
 <calcext:conditional-format calcext:target-range-address="Data.D2:Data.D5"><calcext:color-scale><calcext:color-scale-entry calcext:type="minimum" calcext:color="#ff0000"/><calcext:color-scale-entry calcext:type="percentile" calcext:value="50" calcext:color="#ffff00"/><calcext:color-scale-entry calcext:type="maximum" calcext:color="#00ff00"/></calcext:color-scale></calcext:conditional-format>
 <calcext:conditional-format calcext:target-range-address="Data.E2:Data.E5"><calcext:data-bar calcext:positive-color="#638ec6"><calcext:formatting-entry calcext:type="minimum"/><calcext:formatting-entry calcext:type="maximum"/></calcext:data-bar></calcext:conditional-format>
 <calcext:conditional-format calcext:target-range-address="Data.F2:Data.F5"><calcext:condition calcext:apply-style-name="cfgood" calcext:value="duplicate" calcext:base-cell-address="Data.F2"/></calcext:conditional-format>
</calcext:conditional-formats>
<table:table-column table:style-name="co1" table:number-columns-repeated="2" table:default-cell-style-name="Default"/>
<table:table-column table:style-name="co2" table:visibility="collapse"/>
<table:table-column table:style-name="co1" table:number-columns-repeated="4"/>
<table:table-row table:style-name="ro2">
 <table:table-cell office:value-type="string"><text:p>Name</text:p></table:table-cell>
 <table:table-cell office:value-type="string" table:content-validation-name="val1"><text:p>Kind</text:p></table:table-cell>
 <table:table-cell office:value-type="percentage" office:value="0.255" table:style-name="ce1" table:content-validation-name="val2"><text:p>25.5%</text:p></table:table-cell>
 <table:table-cell office:value-type="float" office:value="-1234.5" table:style-name="ce2"><text:p>-1.234,50 &#8364;</text:p></table:table-cell>
 <table:table-cell office:value-type="date" office:date-value="2026-10-24" table:style-name="ce3"><text:p>24.10.2026</text:p></table:table-cell>
 <table:table-cell office:value-type="float" office:value="12345.678" table:style-name="ce4"><text:p>1.23E+04</text:p></table:table-cell>
</table:table-row>
<table:table-row table:style-name="ro1" table:number-rows-repeated="3">
 <table:table-cell office:value-type="float" office:value="7"><text:p>7</text:p></table:table-cell>
 <table:table-cell table:number-columns-repeated="2"/>
 <table:table-cell office:value-type="string" table:style-name="ce5"><text:p>Hello<text:s text:c="2"/><text:span text:style-name="T1">red bold</text:span><text:tab/>end<text:line-break/>next</text:p><text:p>second paragraph</text:p></table:table-cell>
</table:table-row>
<table:table-row table:style-name="ro1">
 <table:table-cell table:number-columns-spanned="2" table:number-rows-spanned="2" office:value-type="string"><text:p>Merged</text:p><office:annotation><dc:creator>Sam</dc:creator><text:p>A note</text:p></office:annotation></table:table-cell>
 <table:covered-table-cell/>
 <table:table-cell office:value-type="boolean" office:boolean-value="true" table:formula="of:=AND(1;1)"><text:p>TRUE</text:p></table:table-cell>
 <table:table-cell office:value-type="time" office:time-value="PT36H30M00S"><text:p>36:30:00</text:p></table:table-cell>
 <table:table-cell office:value-type="string" office:string-value="from formula" table:formula="of:=&quot;from &quot;&amp;&quot;formula&quot;"><text:p>from formula</text:p></table:table-cell>
 <table:table-cell office:value-type="float" office:value="30" table:formula="msoxl:=SUM(A1,A2)"><text:p>30</text:p></table:table-cell>
</table:table-row>
<table:table-row table:style-name="ro1">
 <table:covered-table-cell table:number-columns-repeated="2"/>
 <table:table-cell office:value-type="float" office:value="1" table:formula="of:=SUM([.A2:.A4])" table:number-matrix-columns-spanned="1" table:number-matrix-rows-spanned="2"/>
 <table:table-cell office:value-type="string"><text:p><text:a xlink:href="#'Hidden sheet'.B2">jump</text:a></text:p></table:table-cell>
</table:table-row>
<table:named-expressions><table:named-expression table:name="Rate" table:expression="of:=0.2" table:base-cell-address="Data.$A$1"/></table:named-expressions>
</table:table>
<table:table table:name="Hidden sheet" table:style-name="ta2"><table:table-column table:style-name="co1"/><table:table-row><table:table-cell office:value-type="float" office:value="1"><text:p>1</text:p></table:table-cell></table:table-row></table:table>
<table:table table:name="Lists" table:style-name="ta1"><table:table-column table:style-name="co1"/><table:table-row><table:table-cell office:value-type="string"><text:p>a</text:p></table:table-cell></table:table-row></table:table>
<table:named-expressions><table:named-range table:name="Everything" table:cell-range-address="$Data.$A$1:.$D$5" table:base-cell-address="$Data.$A$1"/></table:named-expressions>
<table:database-ranges><table:database-range table:name="__Anonymous_Sheet_DB__0" table:target-range-address="Data.A1:Data.D5" table:display-filter-buttons="true"/></table:database-ranges>
</office:spreadsheet></office:body></office:document-content>"##;
    let settings = r##"<?xml version="1.0" encoding="UTF-8"?>
<office:document-settings xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:config="urn:oasis:names:tc:opendocument:xmlns:config:1.0" office:version="1.3"><office:settings><config:config-item-set config:name="ooo:view-settings"><config:config-item-map-indexed config:name="Views"><config:config-item-map-entry>
<config:config-item config:name="ViewId" config:type="string">view1</config:config-item>
<config:config-item config:name="ActiveTable" config:type="string">Lists</config:config-item>
<config:config-item-map-named config:name="Tables">
 <config:config-item-map-entry config:name="Data"><config:config-item config:name="HorizontalSplitMode" config:type="short">2</config:config-item><config:config-item config:name="VerticalSplitMode" config:type="short">2</config:config-item><config:config-item config:name="HorizontalSplitPosition" config:type="int">2</config:config-item><config:config-item config:name="VerticalSplitPosition" config:type="int">1</config:config-item><config:config-item config:name="ZoomValue" config:type="int">125</config:config-item><config:config-item config:name="ShowGrid" config:type="boolean">false</config:config-item></config:config-item-map-entry>
</config:config-item-map-named></config:config-item-map-entry></config:config-item-map-indexed></config:config-item-set></office:settings></office:document-settings>"##;
    let entries = vec![
        unluminous_sheet::zip::Entry { name: "mimetype".into(), data: b"application/vnd.oasis.opendocument.spreadsheet".to_vec() },
        unluminous_sheet::zip::Entry { name: "content.xml".into(), data: content.as_bytes().to_vec() },
        unluminous_sheet::zip::Entry { name: "settings.xml".into(), data: settings.as_bytes().to_vec() },
    ];
    unluminous_sheet::zip::write(&entries).unwrap()
}

#[test]
fn a_file_in_the_form_libreoffice_writes_reads_with_its_settings_and_conditional_formats() {
    let book = unluminous_sheet::ods::read(&libreoffice_style_file()).unwrap();
    let names: Vec<&str> = book.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Data", "Hidden sheet", "Lists"]);
    assert_eq!(book.active_sheet, 2);
    assert_eq!(book.sheets[1].state, Visibility::Hidden);
    let s = &book.sheets[0];
    let at = |a: &str| CellRef::parse(a).unwrap();
    // settings.xml: frozen panes, zoom and gridlines.
    assert_eq!(s.frozen, Some((1, 2)));
    assert_eq!(s.zoom, 125);
    assert!(!s.show_grid);
    // Number styles and typed values.
    let shown = |a: &str| {
        let cell = s.cell(at(a)).unwrap();
        Format::parse(&book.styles.format_code(cell.style)).render(&cell.value, false, 11).text
    };
    assert_eq!(shown("C1"), "25.5%");
    assert_eq!(shown("D1"), "-1,234.50 \u{20AC}", "{}", book.styles.format_code(s.cell(at("D1")).unwrap().style));
    assert_eq!(shown("E1"), "24.10.2026");
    assert_eq!(shown("F1"), "1.23E+04");
    assert_eq!(s.value_at(at("E1")), Value::Number(46319.0));
    assert!(matches!(s.value_at(at("D5")), Value::Number(n) if (n - 36.5 / 24.0).abs() < 1e-12), "a time of 36:30 is a day and a half");
    // Repeated rows and columns, and the widths and heights of their styles.
    for row in 1..=3 {
        assert_eq!(s.value_at(CellRef::new(row, 0)), Value::Number(7.0));
        assert!(s.cell(CellRef::new(row, 3)).is_some(), "row {row} repeated");
    }
    assert!(s.is_column_hidden(2));
    assert!((s.column(2).and_then(|c| c.width).unwrap() - 24.0).abs() < 0.01);
    assert!((s.default_col_width.unwrap() - 11.81).abs() < 0.01);
    assert!((s.rows[&0].height.unwrap() - 34.0).abs() < 0.1);
    // Text with spaces, a span, a tab and a line break, and the look of its style.
    let text = s.cell(at("D2")).unwrap();
    assert_eq!(text.value, Value::text("Hello  red bold\tend\nnext\nsecond paragraph"));
    assert!(text.rich.as_deref().is_some_and(|r| r.contains("<b/>") && r.contains("FFFF0000")), "{:?}", text.rich);
    let xf = book.styles.xf(text.style);
    let font = book.styles.font_of(text.style);
    assert!(font.bold && font.italic);
    assert_eq!(font.underline, unluminous_sheet::styles::Underline::Double);
    assert_eq!((font.name.as_str(), font.size), ("Liberation Sans", 14.0));
    assert_eq!(book.styles.fill_of(text.style).paint(), Some(Color::rgb(255, 255, 0)));
    assert_eq!(xf.align.horizontal.as_deref(), Some("center"));
    assert!(xf.align.wrap);
    assert_eq!(xf.align.rotation, 90);
    assert_eq!(book.styles.border_of(text.style).left.style, "thin");
    // Merges, notes, formulas, errors and links.
    assert_eq!(s.merges, vec![Range::parse("A5:B6").unwrap()]);
    let note = s.notes.get(&(4, 0)).unwrap();
    assert_eq!((note.author.as_str(), note.text.as_str()), ("Sam", "A note"));
    assert_eq!(s.cell(at("C5")).unwrap().formula.as_ref().unwrap().display(), "=AND(1,1)");
    assert_eq!(s.value_at(at("C5")), Value::Bool(true));
    assert_eq!(s.value_at(at("E5")), Value::text("from formula"));
    assert_eq!(s.cell(at("F5")).unwrap().formula.as_ref().unwrap().display(), "=SUM(A1,A2)");
    assert_eq!(s.cell(at("C6")).unwrap().formula.as_ref().unwrap().array, Some(Range::parse("C6:C7").unwrap()));
    assert_eq!(s.links.len(), 1);
    assert_eq!(s.links[0].target, LinkTarget::Location("'Hidden sheet'!B2".to_owned()));
    // Validations.
    assert_eq!(s.validations.len(), 2);
    let list = &s.validations[0];
    assert_eq!((list.kind.as_str(), list.formula1.as_deref()), ("list", Some("Lists!$A$1:$A$3")));
    assert!(!list.allow_blank);
    assert_eq!((list.error_title.as_deref(), list.error.as_deref()), (Some("Oops"), Some("Pick from the list")));
    let number = &s.validations[1];
    assert_eq!((number.kind.as_str(), number.operator.as_deref()), ("decimal", Some("between")));
    assert_eq!((number.formula1.as_deref(), number.formula2.as_deref()), (Some("0"), Some("1")));
    // Conditional formats from the `calcext` form.
    let kinds: Vec<&str> = s.conditional.iter().map(|c| c.rules[0].kind.as_str()).collect();
    assert_eq!(kinds, ["cellIs", "colorScale", "dataBar", "duplicateValues"]);
    assert_eq!(s.conditional[0].rules[0].operator.as_deref(), Some("greaterThan"));
    assert_eq!(s.conditional[0].rules[0].formulas, vec!["5"]);
    assert_eq!(s.conditional[1].rules[0].color_scale.as_ref().unwrap().len(), 3);
    let dxf = s.conditional[0].rules[0].dxf.unwrap() as usize;
    assert_eq!(book.styles.dxfs[dxf].fill, Some(Color::rgb(0xC6, 0xEF, 0xCE)));
    // Names, the print range and the filter.
    let name = |n: &str, scope: Option<usize>| book.names.iter().find(|d| d.name == n && d.scope == scope).map(|d| d.formula.clone());
    assert_eq!(name("Rate", Some(0)).as_deref(), Some("0.2"));
    assert_eq!(name("Everything", None).as_deref(), Some("Data!$A$1:$D$5"));
    assert_eq!(name("_xlnm.Print_Area", Some(0)).as_deref(), Some("Data!$A$1:$D$20"));
    assert_eq!(s.filter.as_ref().map(|f| f.range), Some(Range::parse("A1:D5").unwrap()));
}

// ---------------------------------------------------------------------------------------------------
// Excel.

fn excel_is_installed() -> bool {
    cfg!(windows)
        && std::process::Command::new("reg")
            .args(["query", "HKCR\\Excel.Application"])
            .output()
            .is_ok_and(|o| o.status.success())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf()
}

/// Ask Excel what it sees in a file.
fn inspect(path: &Path) -> serde_json::Value {
    let script = repo_root().join("tools/excel-oracle/inspect.ps1");
    let output = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(&script)
        .arg("-Path")
        .arg(path)
        .arg("-Recalculate")
        .output()
        .expect("pwsh runs");
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim().trim_start_matches('\u{FEFF}');
    serde_json::from_str(text).unwrap_or_else(|e| {
        panic!("Excel's answer did not read: {e}\n{text}\n{}", String::from_utf8_lossy(&output.stderr))
    })
}

/// Skip the test, with a line saying so, when Excel is not here.
fn excel_here() -> bool {
    if excel_is_installed() {
        return true;
    }
    assert!(std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err(), "UNLUMINOUS_REQUIRE_EXCEL is set and Excel is not installed");
    println!("Excel is not installed here, so what it makes of Unluminous's files was not checked");
    false
}

/// What differs between the cells Excel reports and the workbook that was written.
fn excel_disagrees(excel: &serde_json::Value, book: &Workbook) -> Vec<String> {
    let mut wrong = Vec::new();
    for (index, sheet) in excel["sheets"].as_array().unwrap().iter().enumerate() {
        for cell in sheet["cells"].as_array().unwrap() {
            let at = CellRef::parse(cell["cell"].as_str().unwrap()).unwrap();
            let ours = book.sheets[index].value_at(at);
            let theirs = cell["value"].as_str().unwrap();
            let same = match (cell["kind"].as_str().unwrap(), &ours) {
                ("number", Value::Number(n)) => (theirs.parse::<f64>().unwrap() - n).abs() <= 1e-9 * n.abs().max(1.0),
                ("text", Value::Text(t)) => &**t == theirs,
                ("bool", Value::Bool(b)) => theirs.eq_ignore_ascii_case(&b.to_string()),
                ("error", Value::Error(e)) => e.text() == theirs,
                ("empty", Value::Empty) => true,
                _ => false,
            };
            if !same {
                wrong.push(format!("{}!{at}: Excel {} {theirs}, Unluminous {ours:?}", sheet["name"], cell["kind"]));
            }
            let code = book.styles.format_code(book.sheets[index].cell(at).map_or(0, |c| c.style));
            let shown = Format::parse(&code).render(&ours, book.date1904, 255).text;
            let text = cell["text"].as_str().unwrap();
            // A column too narrow for its number shows hash marks, and a General number shows what fits.
            if text != shown && !text.starts_with('#') && !matches!(ours, Value::Number(_)) && !shown.trim().is_empty() {
                wrong.push(format!("{}!{at}: Excel shows {text:?}, Unluminous {shown:?}", sheet["name"]));
            }
        }
    }
    wrong
}

#[test]
fn excel_opens_what_unluminous_writes_as_ods_with_the_same_values_and_no_repair() {
    if !excel_here() {
        return;
    }
    let folder = std::env::temp_dir().join(format!("unluminous-ods-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let session = built();
    let mut cases: Vec<(String, Workbook)> = vec![("built".to_owned(), session.book.clone())];
    let wanted: Vec<String> = if std::env::var("UNLUMINOUS_ODS_EXCEL_ALL").is_ok() {
        names()
    } else {
        ["csv-source", "10-text", "11-styles"].iter().map(|n| (*n).to_owned()).collect()
    };
    for name in wanted {
        cases.push((name.clone(), unluminous_sheet::ods::read(&fixture(&format!("{name}.ods"))).unwrap()));
    }
    for (name, book) in cases {
        let path = folder.join(format!("{name}.ods"));
        std::fs::write(&path, unluminous_sheet::ods::write(&book).unwrap()).unwrap();
        let excel = inspect(&path);
        assert_eq!(excel["repaired"], false, "{name}: Excel repaired the file: {}", excel["problem"]);
        let wrong = excel_disagrees(&excel, &book);
        assert!(wrong.is_empty(), "{name}:\n{}", wrong.join("\n"));
        if name == "built" {
            let data = &excel["sheets"][0]["features"];
            assert_eq!(data["merged"], serde_json::json!(["A7:C7"]));
            assert_eq!(data["notes"], serde_json::json!(["A2|Green tea"]));
            assert!(data["conditional"].as_i64().unwrap() >= 1, "{}", data["conditional"]);
            assert!(data["validations"].as_array().unwrap().iter().any(|v| v == "B12|3|Low,Medium,High"), "{}", data["validations"]);
            assert_eq!(data["links"].as_array().unwrap().len(), 2);
            assert_eq!(excel["sheets"][2]["features"]["visible"], 0, "the hidden sheet stays hidden");
        }
        let _ = std::fs::remove_file(&path);
    }
    let _ = std::fs::remove_dir(&folder);
}
