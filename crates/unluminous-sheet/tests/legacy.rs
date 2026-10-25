//! Reading `.xls` and `.xlsb` files.
//!
//! The fixtures in `fixtures/legacy/` were saved by Excel (`tools/excel-oracle/save-legacy.ps1`) from the
//! feature workbooks of the round 2 audit, which are in `fixtures/legacy/xlsx/`. Next to each is the dump
//! `tools/excel-oracle/inspect.ps1` made of the same file, so these tests run without Excel.

use std::path::{Path, PathBuf};

use unluminous_sheet::coord::CellRef;
use unluminous_sheet::formula::{print::print, Formula};
use unluminous_sheet::model::{LinkTarget, Sheet};
use unluminous_sheet::numfmt::Format;
use unluminous_sheet::{FileKind, Value, Workbook};

/// The workbooks Excel saved in both binary formats.
const BOOKS: &[&str] = &[
    "01-views",
    "02-protection",
    "03-pagesetup",
    "04-outline",
    "05-condfmt",
    "06-validation",
    "07-table",
    "08-filter",
    "09-comments",
    "10-text",
    "11-styles",
    "12-hyperlinks",
    "13-names",
    "15-shapes",
    "16-sparklines",
    "17-pivot",
    "18-external-target",
    "18-formulas",
    "19-macro",
    "csv-source",
    "features",
];

fn legacy_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy")
}

fn open(name: &str) -> Result<Workbook, String> {
    let path = legacy_dir().join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let kind = FileKind::of(&path).unwrap();
    unluminous_sheet::open(&bytes, kind, name, 2026)
}

/// The workbook Excel saved the binary file from, read by the `.xlsx` reader.
fn original(book: &str) -> Workbook {
    let dir = legacy_dir().join("xlsx");
    let path = ["xlsx", "xlsm"]
        .iter()
        .map(|ext| dir.join(format!("{book}.{ext}")))
        .find(|p| p.exists())
        .unwrap_or_else(|| panic!("no original for {book}"));
    unluminous_sheet::xlsx::read(&std::fs::read(path).unwrap()).unwrap()
}

/// Excel's dump of a fixture.
fn excel_dump(name: &str) -> serde_json::Value {
    let path = legacy_dir().join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap()
}

/// A formula as text with the implicit intersection marks Excel adds left out and the engine's own
/// parser and printer applied, so two spellings of one formula compare equal.
fn normal_formula(text: &str) -> String {
    let mut plain = String::new();
    let mut in_text = false;
    // The braces round a legacy array formula are how it is shown and not part of the formula.
    let text = text.strip_prefix('{').and_then(|t| t.strip_suffix('}')).unwrap_or(text);
    for c in text.trim_start_matches('=').chars() {
        if c == '"' {
            in_text = !in_text;
        }
        if c == '@' && !in_text {
            continue;
        }
        plain.push(c);
    }
    match Formula::parse_kept(&plain) {
        Ok(f) => print(&f.expr, false),
        Err(e) => format!("unparsed {plain}: {e}"),
    }
}

/// What the engine shows for a cell, by its value and format, in a column of a given width. A column's
/// width in the file includes the padding Excel adds, which is not a character.
fn shown(book: &Workbook, sheet: &Sheet, at: CellRef, value: &Value, style: u32) -> String {
    let code = book.styles.format_code(style);
    let width = sheet.column(at.col).and_then(|c| c.width).unwrap_or(9.140625);
    let chars = ((width - 0.7109375).floor() as usize).clamp(1, 11);
    Format::parse(&code).render(value, book.date1904, chars).text
}

/// An external workbook reference as Excel writes it (`'C:\folder\[book.xlsx]Sheet1'!A1`) in the form the
/// file holds it: the number of the external link and the sheet (`[1]Sheet1!A1`).
fn external_as_numbered(formula: &str) -> String {
    let Some(open) = formula.find("'") else { return formula.to_owned() };
    let Some(close) = formula[open + 1..].find("'!") else { return formula.to_owned() };
    let inside = &formula[open + 1..open + 1 + close];
    let (Some(bracket), Some(end)) = (inside.find('['), inside.find(']')) else {
        return formula.to_owned();
    };
    let _ = bracket;
    format!("{}[1]{}!{}", &formula[..open], &inside[end + 1..], &formula[open + close + 3..])
}

/// Compare every cell of a workbook with what Excel reports for the same file. Returns the differences.
fn compare_with_excel(name: &str, book: &Workbook, dump: &serde_json::Value) -> Vec<String> {
    let mut problems = Vec::new();
    let sheets = dump["sheets"].as_array().expect("sheets");
    if sheets.len() != book.sheets.len() {
        problems.push(format!("{name}: {} sheets, Excel has {}", book.sheets.len(), sheets.len()));
    }
    for (sheet, theirs) in book.sheets.iter().zip(sheets) {
        let sheet_name = theirs["name"].as_str().unwrap_or("");
        if sheet.name != sheet_name {
            problems.push(format!("{name}: sheet {:?} is {:?} in Excel", sheet.name, sheet_name));
        }
        problems.extend(compare_sheet(book, sheet, theirs, name));
    }
    problems
}

/// Whether an icon set that shows no number covers the cell, so Excel reports no text for it.
fn icon_only(sheet: &Sheet, at: CellRef) -> bool {
    sheet.conditional.iter().any(|format| {
        format.ranges.iter().any(|r| r.contains(at))
            && format.rules.iter().any(|rule| rule.raw_children.contains("showValue=\"0\""))
    })
}

fn in_array(sheet: &Sheet, at: CellRef) -> bool {
    sheet.array_formulas.iter().any(|r| r.contains(at))
}

/// The differences in one sheet's cells: value, formula, number format and the text shown.
fn compare_sheet(
    book: &Workbook,
    sheet: &Sheet,
    theirs: &serde_json::Value,
    name: &str,
) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for cell in theirs["cells"].as_array().into_iter().flatten() {
        let address = cell["cell"].as_str().unwrap_or("");
        let at = CellRef::parse(address).expect("address");
        seen.insert((at.row, at.col));
        let ours = sheet.cell(at);
        let value = sheet.value_at(at);
        let style = ours.map_or(0, |c| c.style);
        let label = format!("{name} {}!{address}", sheet.name);
        // A legacy array formula whose result needs room it does not have is #VALUE! in the file, and
        // Excel works it out again when it opens the file.
        let recalculated = name.starts_with("18-formulas") && (address == "J8" || address == "J10");
        if let Some(p) = compare_value(&value, cell).filter(|_| !recalculated) {
            problems.push(format!("{label}: value {p}"));
        }
        let want = cell["formula"].as_str().unwrap_or("");
        let have = ours.and_then(|c| c.formula.as_ref()).map(|f| f.display());
        // Excel does not report the formula of a cell on a protected sheet that hides its formulas.
        let formula_hidden = book.styles.xf(style).hidden == Some(true);
        match (want.is_empty() || formula_hidden, have) {
            (true, None) => {}
            (false, Some(h)) => {
                if normal_formula(&external_as_numbered(want)) != normal_formula(&h) {
                    problems.push(format!("{label}: formula {h} but Excel has {want}"));
                }
            }
            (true, Some(_)) if in_array(sheet, at) => {}
            (true, Some(_)) if formula_hidden => {}
            (true, Some(h)) => problems.push(format!("{label}: formula {h} but Excel has none")),
            (false, None) => {
                if !sheet.spills.owner.contains_key(&(at.row, at.col)) && !in_array(sheet, at) {
                    problems.push(format!("{label}: no formula but Excel has {want}"));
                }
            }
        }
        // The file escapes a hyphen or a comma in a format, quotes a currency sign, and Excel reports them
        // plain. A format with a fill (`*`) is as wide as the column, which the text reflects.
        let plain = |code: &str| code.replace(['\\', '"'], "");
        let code = plain(cell["format"].as_str().unwrap_or("General"));
        let ours_code = plain(&book.styles.format_code(style));
        if code != ours_code {
            problems.push(format!("{label}: number format {ours_code:?} but Excel has {code:?}"));
        }
        let text = cell["text"].as_str().unwrap_or("");
        if !text.starts_with("##") && !icon_only(sheet, at) && !code.contains('*') && !recalculated
        {
            let ours_text = shown(book, sheet, at, &value, style);
            if ours_text != text {
                problems.push(format!("{label}: shows {ours_text:?} but Excel shows {text:?}"));
            }
        }
    }
    for (&(r, c), cell) in &sheet.cells {
        let filled = !cell.value.is_empty() || cell.formula.is_some();
        if filled && !seen.contains(&(r, c)) {
            problems.push(format!(
                "{name} {}!{}: {:?} is not in Excel's dump",
                sheet.name,
                CellRef::new(r, c),
                cell.value
            ));
        }
    }
    problems
}

/// Why a value differs from Excel's report, or `None` when it does not.
fn compare_value(value: &Value, theirs: &serde_json::Value) -> Option<String> {
    let kind = theirs["kind"].as_str().unwrap_or("");
    let text = theirs["value"].as_str().unwrap_or("");
    let same = match (kind, value) {
        ("number", Value::Number(n)) => text.parse::<f64>().is_ok_and(|t| t == *n),
        ("text", Value::Text(t)) => &**t == text,
        ("bool", Value::Bool(b)) => (text == "True") == *b,
        ("error", Value::Error(e)) => e.text() == text,
        ("empty", Value::Empty) => true,
        ("text", Value::Empty) => text.is_empty(),
        _ => false,
    };
    (!same).then(|| format!("{value:?} but Excel has {kind} {text:?}"))
}

fn problems_for(extension: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for book in BOOKS {
        let name = format!("{book}.{extension}");
        if !legacy_dir().join(format!("{name}.json")).exists() {
            continue;
        }
        match open(&name) {
            Ok(opened) => problems.extend(compare_with_excel(&name, &opened, &excel_dump(&name))),
            Err(e) => problems.push(format!("{name}: {e}")),
        }
    }
    problems
}

#[test]
fn xls_cells_match_what_excel_reports() {
    let problems = problems_for("xls");
    assert!(problems.is_empty(), "{} differences:\n{}", problems.len(), problems.join("\n"));
}

/// A readable dump of a workbook, for looking at what a reader made.
fn dump(book: &Workbook) -> String {
    let mut out = String::new();
    for (i, sheet) in book.sheets.iter().enumerate() {
        out.push_str(&format!("sheet {i} {:?} {:?}\n", sheet.name, sheet.state));
        for (&(r, c), cell) in &sheet.cells {
            let formula = cell.formula.as_ref().map(|f| f.display()).unwrap_or_default();
            out.push_str(&format!(
                "  {} {:?} {} [{}]\n",
                CellRef::new(r, c),
                cell.value,
                formula,
                book.styles.format_code(cell.style)
            ));
        }
    }
    out
}

#[test]
#[ignore]
fn dump_a_fixture() {
    let name = std::env::var("FIXTURE").unwrap_or_else(|_| "features.xls".to_owned());
    match open(&name) {
        Ok(book) => println!("{}", dump(&book)),
        Err(e) => println!("ERROR {e}"),
    }
}

/// The records of a fixture's workbook stream, optionally only some ids, with their bytes.
#[test]
#[ignore]
fn dump_records_of_a_fixture() {
    let name = std::env::var("FIXTURE").unwrap_or_else(|_| "features.xls".to_owned());
    let only: Vec<u16> = std::env::var("RECORDS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| u16::from_str_radix(s.trim(), 16).ok())
        .collect();
    let bytes = std::fs::read(legacy_dir().join(&name)).unwrap();
    let compound = unluminous_sheet::cfb::Compound::parse(&bytes).unwrap();
    let mut stream = compound.stream("Workbook").unwrap().unwrap();
    let filepass =
        unluminous_sheet::xls::records::split(&stream).into_iter().find(|r| r.id == 0x2F);
    if let Some(filepass) = filepass {
        unluminous_sheet::xls::crypto::decrypt_with_default_password(&mut stream, filepass.offset)
            .unwrap();
    }
    for record in unluminous_sheet::xls::records::split(&stream) {
        if !only.is_empty() && !only.contains(&record.id) {
            continue;
        }
        let hex: Vec<String> = record
            .data
            .iter()
            .take(if std::env::var("FULL").is_ok() { usize::MAX } else { 160 })
            .map(|b| format!("{b:02x}"))
            .collect();
        println!(
            "{:04x} len {} starts {:?}: {}",
            record.id,
            record.data.len(),
            record.starts,
            hex.join(" ")
        );
    }
}

/// The differences between a binary file's workbook and the `.xlsx` Excel saved it from, in what both can
/// hold: sheets, merges, column widths, hidden rows and columns, notes and defined names.
fn compare_with_original(name: &str, book: &Workbook, source: &Workbook) -> Vec<String> {
    let mut problems = Vec::new();
    if book.sheets.len() != source.sheets.len() {
        return vec![format!(
            "{name}: {} sheets, the original has {}",
            book.sheets.len(),
            source.sheets.len()
        )];
    }
    for (ours, theirs) in book.sheets.iter().zip(&source.sheets) {
        let label = format!("{name} {}", ours.name);
        if ours.name != theirs.name || ours.state != theirs.state {
            problems.push(format!(
                "{label}: sheet is {:?} {:?}, original {:?} {:?}",
                ours.name, ours.state, theirs.name, theirs.state
            ));
        }
        let mut a = ours.merges.clone();
        let mut b = theirs.merges.clone();
        a.sort_by_key(|r| (r.first.row, r.first.col));
        b.sort_by_key(|r| (r.first.row, r.first.col));
        if a != b {
            problems.push(format!("{label}: merges {a:?}, original {b:?}"));
        }
        problems.extend(compare_columns(&label, ours, theirs));
        let hidden = |s: &Sheet| {
            s.rows.iter().filter(|(_, r)| r.hidden).map(|(&i, _)| i).collect::<Vec<_>>()
        };
        if hidden(ours) != hidden(theirs) {
            problems.push(format!(
                "{label}: hidden rows {:?}, original {:?}",
                hidden(ours),
                hidden(theirs)
            ));
        }
        problems.extend(compare_view_and_features(&label, (book, ours), (source, theirs)));
        // Excel keeps a threaded comment in an .xlsx file and drops it from the binary file.
        let notes = |s: &Sheet| {
            s.notes
                .iter()
                .filter(|(_, n)| !n.text.starts_with("[Threaded comment]"))
                .map(|(&k, n)| (k, n.text.clone()))
                .collect::<Vec<_>>()
        };
        if notes(ours) != notes(theirs) {
            problems.push(format!(
                "{label}: notes {:?}, original {:?}",
                notes(ours),
                notes(theirs)
            ));
        }
    }
    problems.extend(compare_names(name, book, source));
    problems
}

/// The view, the validations, the links, the filter, the protection and the outline of a sheet.
fn compare_view_and_features(
    label: &str,
    (book, ours): (&Workbook, &Sheet),
    (source, theirs): (&Workbook, &Sheet),
) -> Vec<String> {
    let mut problems = Vec::new();
    let mut check = |what: &str, a: String, b: String| {
        if a != b {
            problems.push(format!("{label}: {what} is {a}, original {b}"));
        }
    };
    check("frozen panes", format!("{:?}", ours.frozen), format!("{:?}", theirs.frozen));
    check("gridlines", ours.show_grid.to_string(), theirs.show_grid.to_string());
    check("tab colour", format!("{:?}", ours.tab_color), format!("{:?}", theirs.tab_color));
    let rows = |s: &Sheet| {
        s.rows
            .iter()
            .map(|(&i, r)| (i, r.outline, r.collapsed))
            .filter(|r| r.1 > 0 || r.2)
            .collect::<Vec<_>>()
    };
    check("outline rows", format!("{:?}", rows(ours)), format!("{:?}", rows(theirs)));
    let columns = |s: &Sheet| {
        (0..256)
            .map(|c| (c, s.column(c).map_or((0, false), |i| (i.outline, i.collapsed))))
            .filter(|(_, o)| o.0 > 0 || o.1)
            .collect::<Vec<_>>()
    };
    check("outline columns", format!("{:?}", columns(ours)), format!("{:?}", columns(theirs)));
    // The unique identifier Excel gives a validation in an .xlsx file has no place in a binary file.
    let validations = |s: &Sheet| {
        s.validations
            .iter()
            .map(|v| {
                let mut v = v.clone();
                v.attrs.retain(|(name, _)| name != "xr:uid");
                format!("{v:?}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    check("validations", validations(ours), validations(theirs));
    // A binary file keeps the text shown with every link, and a file link as the path from the folder
    // the file is in, which is not where the .xlsx file was.
    let links = |s: &Sheet| {
        s.links
            .iter()
            .map(|l| {
                let target = match &l.target {
                    LinkTarget::External(t) => {
                        LinkTarget::External(t.trim_start_matches("../").to_owned())
                    }
                    other => other.clone(),
                };
                format!("{:?} {:?} {:?}", l.range, target, l.tooltip)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    check("links", links(ours), links(theirs));
    // The older format has no colour filter and no dynamic date filter, so Excel keeps none of them.
    let filter = |s: &Sheet, old_format: bool| {
        let unsupported = old_format && matches!(s.name.as_str(), "ByColor" | "DateDynamic");
        s.filter.as_ref().map(|f| {
            (f.range, f.columns.keys().copied().filter(|_| !unsupported).collect::<Vec<_>>())
        })
    };
    let old_format = label.contains(".xls ");
    check(
        "filter",
        format!("{:?}", filter(ours, old_format)),
        format!("{:?}", filter(theirs, old_format)),
    );
    let protection = |s: &Sheet| {
        s.raw
            .elements
            .iter()
            .filter(|(n, _)| n == "sheetProtection")
            .map(|(_, x)| x.clone())
            .collect::<Vec<_>>()
    };
    // The password of a sheet protected from Excel 2013 on is a hash this format has no place for.
    let plain = |list: Vec<String>| list.into_iter().map(|x| strip_hash(&x)).collect::<Vec<_>>();
    check(
        "protection",
        format!("{:?}", plain(protection(ours))),
        format!("{:?}", plain(protection(theirs))),
    );
    problems.extend(compare_conditional_formats(label, (book, ours), (source, theirs)));
    problems
}

/// The conditional formats: which cells, and for the rules both formats can say the same way, the rule.
/// A rule the older format can only say as a formula (a date period, a text match, a top ten) is a
/// formula here, so only its range and priority are compared.
fn compare_conditional_formats(
    label: &str,
    (book, ours): (&Workbook, &Sheet),
    (source, theirs): (&Workbook, &Sheet),
) -> Vec<String> {
    let mut problems = Vec::new();
    let rules = |s: &Sheet| {
        s.conditional
            .iter()
            .flat_map(|f| f.rules.iter().map(move |r| (f.ranges.clone(), r.clone())))
            .collect::<Vec<_>>()
    };
    let (mut a, mut b) = (rules(ours), rules(theirs));
    a.sort_by_key(|r| r.1.priority);
    b.sort_by_key(|r| r.1.priority);
    if a.len() != b.len() {
        problems.push(format!(
            "{label}: {} conditional format rules {:?}, original {} {:?}",
            a.len(),
            a.iter().map(|r| (r.1.priority, r.1.kind.clone())).collect::<Vec<_>>(),
            b.len(),
            b.iter().map(|r| (r.1.priority, r.1.kind.clone())).collect::<Vec<_>>()
        ));
        return problems;
    }
    for ((ours_ranges, ours_rule), (their_ranges, their_rule)) in a.iter().zip(&b) {
        let at = format!("{label}: rule {} of {their_ranges:?}", their_rule.priority);
        if ours_ranges != their_ranges {
            problems.push(format!("{at}: ranges {ours_ranges:?}"));
        }
        if ours_rule.priority != their_rule.priority {
            problems.push(format!("{at}: priority {}", ours_rule.priority));
        }
        let comparable = matches!(
            their_rule.kind.as_str(),
            "cellIs" | "expression" | "colorScale" | "dataBar" | "iconSet"
        );
        if !comparable {
            continue;
        }
        let describe = |r: &unluminous_sheet::model::CfRule| {
            let formulas: Vec<String> = r.formulas.iter().map(|f| normal_formula(f)).collect();
            // A data bar keeps the settings of Excel 2010 in an extension this version does not read.
            let kept = if r.kind == "iconSet" { r.raw_children.as_str() } else { "" };
            format!(
                "{} {:?} {formulas:?} {:?} {:?} {kept}",
                r.kind, r.operator, r.color_scale, r.data_bar
            )
        };
        if describe(ours_rule) != describe(their_rule) {
            problems.push(format!(
                "{at}: {} but the original has {}",
                describe(ours_rule),
                describe(their_rule)
            ));
        }
        // The older format holds a colour as the nearest of 56, so its colours are not compared.
        let dxf = |book: &Workbook, r: &unluminous_sheet::model::CfRule| {
            let theme = &book.styles.theme;
            let rgb = |c: Option<unluminous_sheet::styles::Color>| {
                c.and_then(|c| c.resolve(theme)).filter(|_| !label.contains(".xls "))
            };
            r.dxf.and_then(|i| book.styles.dxfs.get(i as usize)).map(|d| {
                format!(
                    "font {:?} bold {:?} italic {:?} fill {:?} format {:?}",
                    rgb(d.font_color),
                    d.bold,
                    d.italic,
                    rgb(d.fill),
                    d.num_fmt
                )
            })
        };
        if dxf(book, ours_rule) != dxf(source, their_rule) {
            problems.push(format!(
                "{at}: looks {:?}, original {:?}",
                dxf(book, ours_rule),
                dxf(source, their_rule)
            ));
        }
        // The older format keeps no stop flag with a rule it writes in the older kind.
        if ours_rule.stop != their_rule.stop && !label.contains(".xls ") {
            problems.push(format!("{at}: stop if true {}", ours_rule.stop));
        }
    }
    problems
}

/// A protection element without the attributes of its password hash.
fn strip_hash(xml: &str) -> String {
    xml.split(' ')
        .filter(|part| {
            !["algorithmName", "hashValue", "saltValue", "spinCount", "password"]
                .iter()
                .any(|k| part.starts_with(k))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Column widths and hidden columns, one column at a time, since the two files cut runs of columns
/// differently.
fn compare_columns(label: &str, ours: &Sheet, theirs: &Sheet) -> Vec<String> {
    let mut problems = Vec::new();
    for col in 0..256 {
        let a = ours.column(col);
        let b = theirs.column(col);
        let width = |c: Option<&unluminous_sheet::model::ColumnInfo>| c.and_then(|c| c.width);
        let hidden = |c: Option<&unluminous_sheet::model::ColumnInfo>| c.is_some_and(|c| c.hidden);
        let widths_agree = match (width(a), width(b)) {
            (Some(x), Some(y)) => (x - y).abs() < 0.02,
            (None, None) => true,
            _ => false,
        };
        if !widths_agree || hidden(a) != hidden(b) {
            problems.push(format!(
                "{label}: column {} is {:?} hidden {}, original {:?} hidden {}",
                unluminous_sheet::coord::column_name(col),
                width(a),
                hidden(a),
                width(b),
                hidden(b)
            ));
        }
    }
    problems
}

fn compare_names(name: &str, book: &Workbook, source: &Workbook) -> Vec<String> {
    let describe = |b: &Workbook| {
        // A slicer's name is lost when Excel saves a binary file.
        let mut list: Vec<String> = b
            .names
            .iter()
            .filter(|n| !n.name.starts_with("Slicer_"))
            .map(|n| {
                format!("{} {:?} {} {}", n.name, n.scope, n.hidden, normal_formula(&n.formula))
            })
            .collect();
        list.sort();
        list
    };
    let (a, b) = (describe(book), describe(source));
    if a == b {
        Vec::new()
    } else {
        vec![format!("{name}: names {a:?}, original {b:?}")]
    }
}

fn feature_problems_for(extension: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for book in BOOKS {
        let name = format!("{book}.{extension}");
        match open(&name) {
            Ok(opened) => problems.extend(compare_with_original(&name, &opened, &original(book))),
            Err(e) => problems.push(format!("{name}: {e}")),
        }
    }
    problems
}

#[test]
fn xls_features_match_the_xlsx_they_were_saved_from() {
    let problems = feature_problems_for("xls");
    assert!(problems.is_empty(), "{} differences:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn xlsb_cells_match_what_excel_reports() {
    let problems = problems_for("xlsb");
    assert!(problems.is_empty(), "{} differences:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn xlsb_features_match_the_xlsx_they_were_saved_from() {
    let problems = feature_problems_for("xlsb");
    assert!(problems.is_empty(), "{} differences:\n{}", problems.len(), problems.join("\n"));
}

/// Every formula of the oracle corpus, saved by Excel in a binary file, reads back as Excel reads it. This
/// is what checks the function numbers of the binary formats, and the tokens the corpus uses.
fn function_problems_for(extension: &str) -> Vec<String> {
    let name = format!("functions.{extension}");
    let book = open(&name).unwrap();
    let list = std::fs::read_to_string(legacy_dir().join("functions.formulas.txt")).unwrap();
    let sheet = &book.sheets[0];
    let mut problems = Vec::new();
    let mut checked = 0;
    for line in list.lines() {
        let Some((row, want)) = line.split_once('\t') else { continue };
        let row: u32 = row.parse().unwrap();
        checked += 1;
        let at = CellRef::new(row - 1, 9);
        let have = sheet.cell(at).and_then(|c| c.formula.as_ref()).map(|f| f.display());
        match have {
            Some(h) if normal_formula(&h) == normal_formula(want) => {}
            Some(h) => problems.push(format!("{name} J{row}: {h} but Excel has {want}")),
            None => problems.push(format!("{name} J{row}: no formula but Excel has {want}")),
        }
    }
    assert!(checked > 500, "only {checked} formulas were checked");
    problems
}

#[test]
fn xls_formulas_of_every_function_read_as_excel_reads_them() {
    let problems = function_problems_for("xls");
    assert!(problems.is_empty(), "{} differences:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn xlsb_formulas_of_every_function_read_as_excel_reads_them() {
    let problems = function_problems_for("xlsb");
    assert!(problems.is_empty(), "{} differences:\n{}", problems.len(), problems.join("\n"));
}
