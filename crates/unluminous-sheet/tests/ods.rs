//! `.ods` files: what Excel writes reads as the workbook it saved, what Unluminous writes reads back, and
//! Excel opens it.
//!
//! `fixtures/ods/` holds each feature workbook as Excel saved it (`.ods`) beside the `.xlsx` it was saved
//! from, made by `tools/excel-oracle/save-ods.ps1`. The tests read both and compare the cells.

use std::path::{Path, PathBuf};

use unluminous_sheet::coord::CellRef;
use unluminous_sheet::numfmt::Format;
use unluminous_sheet::{Value, Workbook};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ods").join(name)).unwrap()
}

/// The names of the feature workbooks Excel saved both ways.
fn names() -> Vec<String> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ods");
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".ods").map(str::to_owned))
        .collect();
    names.sort();
    names
}

fn xlsx_of(name: &str) -> Workbook {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ods");
    for ext in ["xlsx", "xlsm"] {
        if let Ok(bytes) = std::fs::read(path.join(format!("{name}.{ext}"))) {
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

/// How each cell differs between two readings of the same workbook, as text.
fn differences(a: &Workbook, b: &Workbook) -> Vec<String> {
    let mut out = Vec::new();
    let sheets_a: Vec<_> = a.sheets.iter().map(|s| s.name.clone()).collect();
    let sheets_b: Vec<_> = b.sheets.iter().map(|s| s.name.clone()).collect();
    if sheets_a != sheets_b {
        out.push(format!("sheets {sheets_a:?} against {sheets_b:?}"));
        return out;
    }
    for (index, (sa, sb)) in a.sheets.iter().zip(&b.sheets).enumerate() {
        let mut keys: Vec<(u32, u32)> = sa.cells.keys().chain(sb.cells.keys()).copied().collect();
        keys.extend(sa.spills.values.keys().chain(sb.spills.values.keys()));
        keys.sort();
        keys.dedup();
        for (r, c) in keys {
            let at = CellRef::new(r, c);
            let (va, vb) = (sa.value_at(at), sb.value_at(at));
            let place = format!("{}!{}", sa.name, at.a1());
            if !same_value(&va, &vb) {
                out.push(format!("{place} value {va:?} against {vb:?}"));
            }
            let fa = sa.cell(at).and_then(|x| x.formula.as_ref()).map(|f| f.display());
            let fb = sb.cell(at).and_then(|x| x.formula.as_ref()).map(|f| f.display());
            if fa != fb {
                out.push(format!("{place} formula {fa:?} against {fb:?}"));
            }
            let (ca, cb) = (
                a.styles.format_code(sa.cell(at).map_or(0, |x| x.style)),
                b.styles.format_code(sb.cell(at).map_or(0, |x| x.style)),
            );
            let shown = |code: &str, book: &Workbook, v: &Value| Format::parse(code).render(v, book.date1904, 255).text;
            let (ta, tb) = (shown(&ca, a, &va), shown(&cb, b, &vb));
            if ta != tb {
                out.push(format!("{place} shows {ta:?} ({ca}) against {tb:?} ({cb})"));
            }
        }
        out.extend(feature_differences(a, b, index));
    }
    out
}

/// The look of a cell as plain text: font, fill, borders, alignment and protection, resolved to colours.
fn look(book: &Workbook, xf: u32) -> String {
    let s = &book.styles;
    let (font, fill, border, format) = (s.font_of(xf), s.fill_of(xf), s.border_of(xf), s.xf(xf));
    let hex = |c: &unluminous_sheet::styles::Color| c.to_hex(&s.theme);
    let edge = |e: &unluminous_sheet::styles::Edge| {
        if e.is_none() { "none".to_owned() } else { format!("{}/{}", e.style, hex(&e.color)) }
    };
    format!(
        "font {} {} b{} i{} u{:?} s{} {} | fill {:?} | border {} {} {} {} diag {}{}{} | align {:?} {:?} wrap{} indent{} rot{} shrink{} | lock {:?} hide {:?}",
        font.name,
        font.size,
        font.bold,
        font.italic,
        font.underline,
        font.strike,
        hex(&font.color),
        fill.paint().map(|c| hex(&c)),
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

/// Merges, sizes, visibility, notes, links, validations, conditional formats and names.
fn feature_differences(a: &Workbook, b: &Workbook, index: usize) -> Vec<String> {
    let (sa, sb) = (&a.sheets[index], &b.sheets[index]);
    let mut out = Vec::new();
    let name = &sa.name;
    let mut note = |what: &str, x: String, y: String| {
        if x != y {
            out.push(format!("{name} {what}: {x} against {y}"));
        }
    };
    let mut merges_a = sa.merges.clone();
    let mut merges_b = sb.merges.clone();
    merges_a.sort_by_key(|r| (r.first.row, r.first.col));
    merges_b.sort_by_key(|r| (r.first.row, r.first.col));
    note("merges", format!("{merges_a:?}"), format!("{merges_b:?}"));
    note("hidden", format!("{:?}", sa.state), format!("{:?}", sb.state));
    note("tab color", format!("{:?}", sa.tab_color.map(|c| c.to_hex(&a.styles.theme))), format!("{:?}", sb.tab_color.map(|c| c.to_hex(&b.styles.theme))));
    let hidden_rows = |s: &unluminous_sheet::Sheet| s.rows.iter().filter(|(_, i)| i.hidden).map(|(r, _)| *r).collect::<Vec<_>>();
    note("hidden rows", format!("{:?}", hidden_rows(sa)), format!("{:?}", hidden_rows(sb)));
    let hidden_cols = |s: &unluminous_sheet::Sheet| (0..64).filter(|c| s.is_column_hidden(*c)).collect::<Vec<_>>();
    note("hidden columns", format!("{:?}", hidden_cols(sa)), format!("{:?}", hidden_cols(sb)));
    let widths = |s: &unluminous_sheet::Sheet| {
        (0..32).filter_map(|c| s.column(c).and_then(|i| i.width.filter(|_| i.custom_width)).map(|w| (c, (w * 100.0).round() / 100.0))).collect::<Vec<_>>()
    };
    note("custom widths", format!("{:?}", widths(sa)), format!("{:?}", widths(sb)));
    let heights = |s: &unluminous_sheet::Sheet| s.rows.iter().filter_map(|(r, i)| i.height.map(|h| (*r, (h * 10.0).round() / 10.0))).collect::<Vec<_>>();
    note("row heights", format!("{:?}", heights(sa)), format!("{:?}", heights(sb)));
    let notes = |s: &unluminous_sheet::Sheet| s.notes.iter().map(|(k, n)| (*k, n.text.replace("\r\n", "\n"))).collect::<Vec<_>>();
    note("notes", format!("{:?}", notes(sa)), format!("{:?}", notes(sb)));
    let links = |s: &unluminous_sheet::Sheet| s.links.iter().map(|l| (l.range.a1(), format!("{:?}", l.target))).collect::<Vec<_>>();
    note("links", format!("{:?}", links(sa)), format!("{:?}", links(sb)));
    let validations = |s: &unluminous_sheet::Sheet| {
        let mut v: Vec<String> = s.validations.iter().flat_map(|v| v.ranges.iter().map(move |r| format!("{} {} {:?} {:?} {:?}", r.a1(), v.kind, v.operator, v.formula1, v.formula2))).collect();
        v.sort();
        v
    };
    note("validations", format!("{:#?}", validations(sa)), format!("{:#?}", validations(sb)));
    note(
        "conditional formats",
        format!("{:?}", sa.conditional.iter().map(|c| c.rules.len()).sum::<usize>()),
        format!("{:?}", sb.conditional.iter().map(|c| c.rules.len()).sum::<usize>()),
    );
    note("frozen", format!("{:?}", sa.frozen), format!("{:?}", sb.frozen));
    let names = |w: &Workbook| {
        let mut n: Vec<String> = w.names.iter().map(|d| format!("{:?}:{}={}", d.scope, d.name.trim_start_matches("_xlnm."), d.formula)).collect();
        n.sort();
        n
    };
    if index == 0 {
        note("names", format!("{:#?}", names(a)), format!("{:#?}", names(b)));
    }
    for (r, c) in sa.cells.keys().chain(sb.cells.keys()).copied().collect::<std::collections::BTreeSet<_>>() {
        let (xa, xb) = (sa.cell(CellRef::new(r, c)).map_or(0, |x| x.style), sb.cell(CellRef::new(r, c)).map_or(0, |x| x.style));
        let (la, lb) = (look(a, xa), look(b, xb));
        if la != lb {
            out.push(format!("{name}!{} look: {la}\n      against {lb}", CellRef::new(r, c).a1()));
        }
    }
    out
}

#[test]
fn dump_differences() {
    for name in names() {
        let ods = unluminous_sheet::ods::read(&fixture(&format!("{name}.ods"))).unwrap();
        let xlsx = xlsx_of(&name);
        let diff = differences(&ods, &xlsx);
        println!("== {name}: {} differences", diff.len());
        for line in diff.iter().take(40) {
            println!("   {line}");
        }
    }
    let _: PathBuf = PathBuf::new();
}

#[test]
#[ignore = "writes files for a person to open"]
fn write_every_fixture_for_excel() {
    let out = PathBuf::from(std::env::var("UNLUMINOUS_ODS_OUT").unwrap());
    std::fs::create_dir_all(&out).unwrap();
    for name in names() {
        let first = unluminous_sheet::ods::read(&fixture(&format!("{name}.ods"))).unwrap();
        std::fs::write(out.join(format!("{name}.ods")), unluminous_sheet::ods::write(&first).unwrap()).unwrap();
    }
}

#[test]
fn dump_round_trip_differences() {
    for name in names() {
        let first = unluminous_sheet::ods::read(&fixture(&format!("{name}.ods"))).unwrap();
        let bytes = match unluminous_sheet::ods::write(&first) {
            Ok(b) => b,
            Err(e) => panic!("{name}: {e}"),
        };
        let second = unluminous_sheet::ods::read(&bytes).unwrap();
        let diff = differences(&first, &second);
        println!("== {name}: {} differences ({} bytes)", diff.len(), bytes.len());
        for line in diff.iter().take(30) {
            println!("   {line}");
        }
    }
}
