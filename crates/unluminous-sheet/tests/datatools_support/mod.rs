//! The engine half of the data tools test: it builds a workbook from a scenario, performs the steps with the
//! engine and compares the result with the dump Excel made of the same steps.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value as Json;
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::{CellDisplay, Session};
use unluminous_sheet::model::*;
use unluminous_sheet::styles::{AlignEdit, BorderEdit, Color, Edge, FontEdit, Sides, StyleChange};
use unluminous_sheet::{Value, Workbook};

mod criteria;
mod filters;
mod operations;

/// `tests/fixtures/datatools`.
pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/datatools")
}

/// The families that have both a scenario file and Excel's answers.
pub fn families() -> Vec<String> {
    let only = std::env::var("DATATOOLS").ok();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(fixture_dir()).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(family) = name.strip_suffix(".excel.json") {
            if only.as_deref().is_none_or(|o| o == family) {
                out.push(family.to_owned());
            }
        }
    }
    out.sort();
    out
}

fn read_json(path: PathBuf) -> Json {
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Each scenario with what Excel did with it.
pub fn cases(family: &str) -> Vec<(Json, Json)> {
    let scenarios = read_json(fixture_dir().join(format!("{family}.json")));
    let expected = read_json(fixture_dir().join(format!("{family}.excel.json")));
    let by_name: BTreeMap<String, Json> = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["name"].as_str().unwrap().to_owned(), e.clone()))
        .collect();
    scenarios
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| by_name.get(s["name"].as_str().unwrap()).map(|e| (s.clone(), e.clone())))
        .collect()
}

/// What typing a JSON value into a cell means.
pub fn typed(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
        Json::Number(n) => {
            let f = n.as_f64().unwrap();
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", f as i64)
            } else if f.abs() >= 1e-4 && f.abs() < 1e15 {
                format!("{f}")
            } else {
                format!("{f:e}").replace('e', "E")
            }
        }
        other => other.to_string(),
    }
}

/// The index of a sheet by name, adding it when there is none.
pub fn sheet_of(s: &mut Session, name: &str) -> usize {
    if let Some(i) = s.book.sheet_index(name) {
        return i;
    }
    s.add_sheet(Some(name), None).unwrap()
}

/// The sheet a step works on.
pub fn step_sheet(s: &mut Session, step: &Json) -> usize {
    sheet_of(s, step["sheet"].as_str().unwrap_or("Sheet1"))
}

pub fn range_of(text: &str) -> Range {
    Range::parse(text).unwrap_or_else(|| panic!("bad range {text}"))
}

fn fill_cells(s: &mut Session, sheet: usize, cells: &Json) {
    let Some(map) = cells.as_object() else { return };
    let mut inputs: Vec<(CellRef, String)> = Vec::new();
    for (addr, v) in map {
        inputs.push((
            CellRef::parse(addr).unwrap(),
            if v.is_null() { String::new() } else { typed(v) },
        ));
    }
    s.set_inputs(sheet, &inputs).unwrap();
}

fn rgb(hex: &str) -> Color {
    Color::Rgb(0xFF00_0000 | u32::from_str_radix(hex, 16).unwrap())
}

/// Apply one `formats` entry of a scenario.
pub fn apply_format(s: &mut Session, f: &Json) -> Result<(), String> {
    let sheet = sheet_of(s, f["sheet"].as_str().unwrap_or("Sheet1"));
    let range = range_of(f["range"].as_str().unwrap());
    let mut changes: Vec<StyleChange> = Vec::new();
    if let Some(b) = f["bold"].as_bool() {
        changes.push(StyleChange::Font(FontEdit { bold: Some(b), ..Default::default() }));
    }
    if let Some(b) = f["italic"].as_bool() {
        changes.push(StyleChange::Font(FontEdit { italic: Some(b), ..Default::default() }));
    }
    if f["underline"].as_bool() == Some(true) {
        changes.push(StyleChange::Font(FontEdit {
            underline: Some(unluminous_sheet::styles::Underline::Single),
            ..Default::default()
        }));
    }
    if let Some(h) = f["fill"].as_str() {
        changes.push(StyleChange::Fill(Some(rgb(h))));
    }
    if let Some(h) = f["font"].as_str() {
        changes.push(StyleChange::Font(FontEdit { color: Some(rgb(h)), ..Default::default() }));
    }
    if let Some(n) = f["size"].as_f64() {
        changes.push(StyleChange::Font(FontEdit { size: Some(n), ..Default::default() }));
    }
    if let Some(n) = f["name"].as_str() {
        changes
            .push(StyleChange::Font(FontEdit { name: Some(n.to_owned()), ..Default::default() }));
    }
    if let Some(n) = f["numfmt"].as_str() {
        changes.push(StyleChange::NumberFormat(n.to_owned()));
    }
    if let Some(h) = f["halign"].as_str() {
        changes.push(StyleChange::Align(AlignEdit {
            horizontal: Some(Some(h.to_owned())),
            ..Default::default()
        }));
    }
    if let Some(w) = f["wrap"].as_bool() {
        changes.push(StyleChange::Align(AlignEdit { wrap: Some(w), ..Default::default() }));
    }
    if f["borderBottom"].as_bool() == Some(true) {
        changes.push(StyleChange::Border(BorderEdit {
            sides: Sides::Bottom,
            edge: Edge::line("thin", Color::Indexed(64)),
        }));
    }
    for c in changes {
        s.set_style(sheet, &[range], &c)?;
    }
    if f["merge"].as_bool() == Some(true) {
        s.merge(sheet, range, false)?;
    }
    Ok(())
}

/// The three traffic lights the scenarios put on a column: a number in the lowest third of the range of the
/// column shows the first icon, the middle third the second and the highest the third.
pub struct TrafficLights {
    pub sheet: usize,
    pub ranges: Vec<Range>,
}

impl CellDisplay for TrafficLights {
    fn icon(&self, book: &Workbook, sheet: usize, at: CellRef) -> Option<(String, u32)> {
        if sheet != self.sheet {
            return None;
        }
        let range = self.ranges.iter().find(|r| r.contains(at))?;
        let numbers: Vec<f64> = range
            .cells()
            .filter_map(|c| match book.sheets[sheet].value_at(c) {
                Value::Number(n) => Some(n),
                _ => None,
            })
            .collect();
        let (lo, hi) =
            numbers.iter().fold((f64::MAX, f64::MIN), |(l, h), n| (l.min(*n), h.max(*n)));
        let Value::Number(n) = book.sheets[sheet].value_at(at) else { return None };
        let at_percent = |p: f64| lo + (hi - lo) * p;
        let id = if n >= at_percent(0.67) {
            2
        } else if n >= at_percent(0.33) {
            1
        } else {
            0
        };
        Some(("3Arrows".to_owned(), id))
    }
}

/// Build the workbook of a scenario, run its steps, and report what differs from Excel.
pub fn check(scenario: &Json, expected: &Json) -> Vec<String> {
    let today = expected["today"].as_f64().unwrap_or(46318.0);
    let mut s = Session::new(Workbook::new(), today + 0.5);
    s.recalculate_all();
    let mut ctx = operations::Context::default();
    if let Some(sheets) = scenario["sheets"].as_object() {
        for name in sheets.keys() {
            sheet_of(&mut s, name);
        }
        for (name, cells) in sheets {
            let i = sheet_of(&mut s, name);
            fill_cells(&mut s, i, cells);
        }
    }
    for f in scenario["formats"].as_array().into_iter().flatten() {
        if f["icons"].is_number() {
            let sheet = sheet_of(&mut s, f["sheet"].as_str().unwrap_or("Sheet1"));
            ctx.lights
                .get_or_insert(TrafficLights { sheet, ranges: Vec::new() })
                .ranges
                .push(range_of(f["range"].as_str().unwrap()));
        } else if let Err(e) = apply_format(&mut s, f) {
            return vec![format!("format failed: {e}")];
        }
    }
    for step in scenario["setup"].as_array().into_iter().flatten() {
        if let Err(e) = operations::run_step(&mut s, step, &mut ctx) {
            return vec![format!("setup step {} failed: {e}", step["kind"])];
        }
    }
    let mut error = String::new();
    let ops = match &scenario["op"] {
        Json::Array(a) => a.clone(),
        other => vec![other.clone()],
    };
    for step in &ops {
        if let Err(e) = operations::run_step(&mut s, step, &mut ctx) {
            error = e;
            break;
        }
    }
    compare(&s, scenario, expected, &error)
}

fn compare(s: &Session, scenario: &Json, expected: &Json, error: &str) -> Vec<String> {
    let mut out = Vec::new();
    let expected_error = expected["error"].as_str().unwrap_or("").split(" @ ").next().unwrap_or("");
    if expected_error.starts_with("HARNESS") {
        return vec![format!("Excel's harness failed: {expected_error}")];
    }
    if expected_error.is_empty() != error.is_empty() {
        out.push(format!("Excel error {expected_error:?}, engine error {error:?}"));
    } else if scenario["checkMessage"].as_bool() == Some(true) && expected_error != error {
        out.push(format!("message: Excel {expected_error:?}, engine {error:?}"));
    }
    let deep = !scenario["dumpFormat"].is_null();
    for sheet in expected["sheets"].as_array().into_iter().flatten() {
        let name = sheet["name"].as_str().unwrap();
        let Some(index) = s.book.sheet_index(name) else {
            out.push(format!("no sheet {name}"));
            continue;
        };
        out.extend(compare_cells(s, index, sheet, deep));
        out.extend(compare_rows(s, index, sheet, scenario));
        out.extend(compare_tables(s, index, sheet));
    }
    out.extend(criteria::compare_xml(s, expected, scenario));
    out
}

fn format_code_alias(code: &str) -> &str {
    match code {
        "mm-dd-yy" => "m/d/yyyy",
        other => other,
    }
}

fn bgr_hex(c: Option<(u8, u8, u8)>) -> String {
    match c {
        Some((r, g, b)) => format!("{:06X}", (b as u32) << 16 | (g as u32) << 8 | r as u32),
        None => String::new(),
    }
}

/// Whether text is the name Excel gives a new workbook, such as `Book21`.
fn is_book_name(text: &str) -> bool {
    text.strip_prefix("Book")
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

fn same_value(kind: &str, shown: &str, v: &Value) -> bool {
    match (kind, v) {
        ("number", Value::Number(n)) => {
            let e: f64 = shown.parse().unwrap_or(f64::NAN);
            (e - n).abs() <= 1e-12 * e.abs().max(n.abs()).max(1.0)
        }
        // Excel writes the name of its temporary workbook beside linked rows, and the number differs per run.
        ("text", Value::Text(t)) if is_book_name(shown) => is_book_name(t),
        ("text", Value::Text(t)) => &**t == shown,
        ("bool", Value::Bool(b)) => shown.eq_ignore_ascii_case(if *b { "true" } else { "false" }),
        ("error", Value::Error(e)) => e.text() == shown,
        ("empty", Value::Empty) => true,
        _ => false,
    }
}

fn compare_cells(s: &Session, index: usize, sheet: &Json, deep: bool) -> Vec<String> {
    let mut out = Vec::new();
    let book = &s.book;
    let ours = &book.sheets[index];
    let name = &ours.name;
    let mut expected_at: BTreeMap<(u32, u32), &Json> = BTreeMap::new();
    for c in sheet["cells"].as_array().into_iter().flatten() {
        let at = CellRef::parse(c["a"].as_str().unwrap()).unwrap();
        expected_at.insert((at.row, at.col), c);
    }
    for (&(r, c), cell) in &expected_at {
        let at = CellRef::new(r, c);
        let mine = ours.cell(at).cloned().unwrap_or_default();
        let kind = cell["k"].as_str().unwrap();
        let shown = cell["v"].as_str().unwrap();
        if !same_value(kind, shown, &mine.value) {
            out.push(format!("{name}!{at}: Excel {kind} {shown:?}, engine {:?}", mine.value));
        }
        // Text that begins with `=` after a quote prefix reads back from Excel as though it were a formula.
        let ef = cell["f"].as_str().unwrap_or("");
        let ef = if kind == "text" && ef.trim_start_matches('=') == shown.trim_start_matches('=') {
            ""
        } else {
            ef
        };
        let mut mf = mine.formula.as_ref().map(|f| f.display()).unwrap_or_default();
        // Inside its own table Excel writes a column as `[Qty]` rather than `Tbl[Qty]`.
        for t in book.tables.iter().filter(|t| t.sheet == index && t.range.contains(at)) {
            mf = mf.replace(&format!("{}[", t.name), "[");
        }
        if !ef.replace(' ', "").eq_ignore_ascii_case(&mf.replace(' ', "")) {
            out.push(format!("{name}!{at}: Excel formula {ef:?}, engine {mf:?}"));
        }
        let nf = cell["nf"].as_str().unwrap_or("General");
        let mine_nf = book.styles.format_code(mine.style);
        if format_code_alias(nf) != format_code_alias(&mine_nf) {
            out.push(format!("{name}!{at}: Excel format {nf:?}, engine {mine_nf:?}"));
        }
        if deep {
            let theme = &book.styles.theme;
            let fill =
                bgr_hex(book.styles.fill_of(mine.style).paint().and_then(|c| c.resolve(theme)));
            if fill != cell["fill"].as_str().unwrap_or("") {
                out.push(format!("{name}!{at}: Excel fill {:?}, engine {fill:?}", cell["fill"]));
            }
            let font =
                bgr_hex(book.styles.font_of(mine.style).color.resolve(theme).or(Some((0, 0, 0))));
            if cell["font"].as_str().is_some_and(|f| !f.is_empty() && f != font) {
                out.push(format!(
                    "{name}!{at}: Excel font colour {:?}, engine {font:?}",
                    cell["font"]
                ));
            }
        }
    }
    for (&(r, c), cell) in &ours.cells {
        if expected_at.contains_key(&(r, c)) {
            continue;
        }
        if !cell.value.is_empty() || cell.formula.is_some() {
            out.push(format!(
                "{name}!{}: engine has {:?}, Excel has nothing",
                CellRef::new(r, c),
                cell.value
            ));
        }
    }
    out
}

fn compare_rows(s: &Session, index: usize, sheet: &Json, scenario: &Json) -> Vec<String> {
    let mut out = Vec::new();
    let ours = &s.book.sheets[index];
    let mut bound = 1;
    for c in sheet["cells"].as_array().into_iter().flatten() {
        bound = bound.max(CellRef::parse(c["a"].as_str().unwrap()).unwrap().row + 1);
    }
    let expected: Vec<u32> = sheet["hiddenRows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_u64().map(|n| n as u32))
        .filter(|r| *r <= bound)
        .collect();
    let mine: Vec<u32> = (1..=bound).filter(|r| ours.is_row_hidden(r - 1)).collect();
    if expected != mine {
        out.push(format!("{}: hidden rows Excel {expected:?}, engine {mine:?}", ours.name));
    }
    let notes_expected: BTreeMap<String, String> = sheet["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|n| (n["a"].as_str().unwrap().to_owned(), n["text"].as_str().unwrap_or("").to_owned()))
        .collect();
    let notes_mine: BTreeMap<String, String> =
        ours.notes.iter().map(|(&(r, c), n)| (CellRef::new(r, c).a1(), n.text.clone())).collect();
    if notes_expected != notes_mine {
        out.push(format!("{}: notes Excel {notes_expected:?}, engine {notes_mine:?}", ours.name));
    }
    if !scenario["dumpOutline"].is_null() {
        let expected_rows: Vec<(u64, u64)> = sheet["outlineRows"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|o| (o["row"].as_f64().unwrap() as u64, o["level"].as_f64().unwrap() as u64))
            .collect();
        let mine_rows: Vec<(u64, u64)> = ours
            .rows
            .iter()
            .filter(|(_, i)| i.outline > 0)
            .map(|(r, i)| (*r as u64 + 1, i.outline as u64 + 1))
            .collect();
        if expected_rows != mine_rows {
            out.push(format!(
                "{}: outline rows Excel {expected_rows:?}, engine {mine_rows:?}",
                ours.name
            ));
        }
    }
    out
}

fn compare_tables(s: &Session, index: usize, sheet: &Json) -> Vec<String> {
    let mut out = Vec::new();
    let mine: Vec<&Table> = s.book.tables.iter().filter(|t| t.sheet == index).collect();
    let theirs = sheet["tables"].as_array().cloned().unwrap_or_default();
    if mine.len() != theirs.len() {
        out.push(format!("{} tables in Excel, {} in the engine", theirs.len(), mine.len()));
        return out;
    }
    for (t, e) in mine.iter().zip(&theirs) {
        if t.name != e["name"].as_str().unwrap() {
            out.push(format!("table name Excel {}, engine {}", e["name"], t.name));
        }
        if t.range.a1() != e["range"].as_str().unwrap().replace('$', "") {
            out.push(format!("table range Excel {}, engine {}", e["range"], t.range.a1()));
        }
        if t.header_row != e["headers"].as_bool().unwrap_or(true) {
            out.push(format!("table headers Excel {}, engine {}", e["headers"], t.header_row));
        }
        if t.totals_row != e["totals"].as_bool().unwrap_or(false) {
            out.push(format!("table totals Excel {}, engine {}", e["totals"], t.totals_row));
        }
        let names: Vec<String> = e["columns"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| c["name"].as_str().unwrap().to_owned())
            .collect();
        if names != t.columns {
            out.push(format!("table columns Excel {names:?}, engine {:?}", t.columns));
        }
    }
    out
}
