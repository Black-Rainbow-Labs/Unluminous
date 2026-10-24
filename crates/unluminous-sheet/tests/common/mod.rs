//! The scenario runner shared by the editing tests.
//!
//! A scenario file (`tests/fixtures/<lane>/<name>.in.json`) is a list of cases, and its answer file
//! (`<name>.out.json`) is what Excel did, written by `tools/excel-oracle/edit-oracle.ps1`. This module builds
//! each case in a [`Session`], performs the same steps with the engine, and compares every cell in the
//! ranges the case dumps.

#![allow(dead_code)]

use serde_json::Value as J;
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::clipboard::{
    AcrossSheets, CustomLists, ExternalPaste, FillKind, PasteKind, PasteOperation, PasteSpecial, SeriesDate, SeriesKind,
    SeriesSpec,
};
use unluminous_sheet::edit::{Clip, Session, Shift};
use unluminous_sheet::model::{CfRule, FilterCriteria, Validation, Workbook};
use unluminous_sheet::styles::{
    AlignEdit, BorderEdit, Color, Dxf, Edge, FontEdit, Sides, StyleChange,
};
use unluminous_sheet::value::Value;

/// The state of a case while its steps run: the workbook and what was last copied.
pub struct World {
    pub session: Session,
    pub clip: Option<Clip>,
    /// Text and HTML put on the clipboard by another program, and the text a copy gave for other programs.
    pub outside_text: Option<String>,
    pub outside_html: Option<String>,
    pub copied_text: Option<String>,
}

/// An Excel `Color` integer (blue, green, red from the top) as a colour.
pub fn color_of(bgr: i64) -> Color {
    Color::rgb((bgr & 255) as u8, ((bgr >> 8) & 255) as u8, ((bgr >> 16) & 255) as u8)
}

/// The sheet index and range an address such as `Sheet2!A1:B2` names.
pub fn address(session: &Session, text: &str) -> (usize, Range) {
    let (sheet, cells) = match text.split_once('!') {
        Some((name, rest)) => (session.book.sheet_index(name).expect("sheet"), rest),
        None => (0, text),
    };
    (sheet, Range::parse(cells).unwrap_or_else(|| panic!("address {text}")))
}

/// Build the workbook of a case: its cells, notes, validation, conditional formats, widths, hidden rows
/// and merged cells.
pub fn build(case: &J) -> World {
    let mut session = Session::new(Workbook::new(), 46318.5);
    for name in case["sheets"].as_array().into_iter().flatten().filter_map(|n| n.as_str()) {
        session.add_sheet(Some(name), None).unwrap();
    }
    if let Some(cells) = case["cells"].as_object() {
        for (at, spec) in cells {
            put_cell(&mut session, at, spec);
        }
    }
    for (at, text) in case["notes"].as_object().into_iter().flatten() {
        let (sheet, range) = address(&session, at);
        session.set_note(sheet, range.first, text.as_str(), "author").unwrap();
    }
    for v in case["validation"].as_array().into_iter().flatten() {
        add_validation(&mut session, v);
    }
    for c in case["cf"].as_array().into_iter().flatten() {
        add_conditional(&mut session, c);
    }
    for (letter, width) in case["widths"].as_object().into_iter().flatten() {
        let col = unluminous_sheet::coord::column_number(letter).unwrap();
        session.set_column_width(0, col, col, Some(width.as_f64().unwrap() + 0.7109375)).unwrap();
    }
    for row in case["hidden_rows"].as_array().into_iter().flatten() {
        let r = row.as_u64().unwrap() as u32 - 1;
        session.hide_rows(0, r, r, true).unwrap();
    }
    for m in case["merge"].as_array().into_iter().flatten() {
        let (sheet, range) = address(&session, m.as_str().unwrap());
        session.merge(sheet, range, false).unwrap();
    }
    World { session, clip: None, outside_text: None, outside_html: None, copied_text: None }
}

fn put_cell(session: &mut Session, at: &str, spec: &J) {
    let (sheet, range) = address(session, at);
    let at = range.first;
    let cell = Range::cell(at);
    let (value, formula) = match spec {
        J::Object(o) => (o.get("v").cloned().unwrap_or(J::Null), o.get("f").and_then(|f| f.as_str())),
        other => (other.clone(), None),
    };
    if let Some(code) = spec["nf"].as_str() {
        session.set_style(sheet, &[cell], &StyleChange::NumberFormat(code.to_owned())).unwrap();
    }
    if let Some(f) = formula {
        session.set_input(sheet, at, f).unwrap();
    } else {
        match value {
            J::Number(n) => session.set_value(sheet, at, Value::Number(n.as_f64().unwrap())).unwrap(),
            // Excel reads a string it is given as it would be typed, unless the cell is a text cell.
            J::String(s) if spec["nf"].as_str() == Some("@") => {
                session.set_value(sheet, at, Value::text(s)).unwrap()
            }
            J::String(s) if s.is_empty() => {}
            J::String(s) => session.set_input(sheet, at, &s).unwrap(),
            J::Bool(b) => session.set_value(sheet, at, Value::Bool(b)).unwrap(),
            _ => {}
        }
    }
    style_cell(session, sheet, cell, spec);
}

fn style_cell(session: &mut Session, sheet: usize, cell: Range, spec: &J) {
    let apply = |session: &mut Session, change: StyleChange| {
        session.set_style(sheet, &[cell], &change).unwrap();
    };
    let font = |edit: FontEdit| StyleChange::Font(edit);
    if spec["bold"].as_bool() == Some(true) {
        apply(session, font(FontEdit { bold: Some(true), ..Default::default() }));
    }
    if spec["italic"].as_bool() == Some(true) {
        apply(session, font(FontEdit { italic: Some(true), ..Default::default() }));
    }
    if let Some(c) = spec["color"].as_i64() {
        apply(session, font(FontEdit { color: Some(color_of(c)), ..Default::default() }));
    }
    if let Some(c) = spec["fill"].as_i64() {
        apply(session, StyleChange::Fill(Some(color_of(c))));
    }
    if let Some(a) = spec["align"].as_i64() {
        let name = match a {
            -4108 => "center",
            -4131 => "left",
            -4152 => "right",
            other => panic!("alignment {other}"),
        };
        let edit = AlignEdit { horizontal: Some(Some(name.to_owned())), ..Default::default() };
        apply(session, StyleChange::Align(edit));
    }
    if spec["border"].as_bool() == Some(true) {
        let edge = Edge::line("thin", Color::Auto);
        apply(session, StyleChange::Border(BorderEdit { sides: Sides::Bottom, edge }));
    }
}

fn add_validation(session: &mut Session, v: &J) {
    let (sheet, range) = address(session, v["at"].as_str().unwrap());
    let kind = match v["type"].as_i64().unwrap() {
        1 => "whole",
        2 => "decimal",
        3 => "list",
        other => panic!("validation type {other}"),
    };
    let validation = Validation {
        ranges: vec![range],
        kind: kind.to_owned(),
        operator: Some("between".to_owned()),
        formula1: v["f1"].as_str().map(str::to_owned),
        formula2: v["f2"].as_str().map(str::to_owned),
        allow_blank: true,
        show_dropdown: true,
        prompt_title: None,
        prompt: None,
        error_title: None,
        error: None,
        error_style: None,
        attrs: Vec::new(),
    };
    session.add_validation(sheet, validation).unwrap();
}

fn add_conditional(session: &mut Session, c: &J) {
    let (sheet, range) = address(session, c["at"].as_str().unwrap());
    let operator = match c["op"].as_i64().unwrap() {
        5 => "greaterThan",
        6 => "lessThan",
        3 => "equal",
        other => panic!("conditional operator {other}"),
    };
    let dxf = Dxf { fill: c["fill"].as_i64().map(color_of), ..Default::default() };
    session.book.styles.dxfs.push(dxf);
    let rule = CfRule {
        kind: "cellIs".to_owned(),
        dxf: Some(session.book.styles.dxfs.len() as u32 - 1),
        priority: 1,
        stop: false,
        operator: Some(operator.to_owned()),
        text: None,
        rank: None,
        percent: false,
        bottom: false,
        above_average: None,
        formulas: vec![c["f1"].as_str().unwrap().to_owned()],
        color_scale: None,
        data_bar: None,
        attrs: Vec::new(),
        raw_children: String::new(),
    };
    session.add_conditional(sheet, vec![range], rule).unwrap();
}

/// Perform the steps of a case. `Err` carries the engine's message when a step is refused, which is what
/// Excel's `error` field is compared with.
pub fn run_steps(world: &mut World, case: &J) -> Result<(), String> {
    for step in case["steps"].as_array().into_iter().flatten() {
        run_step(world, step)?;
    }
    Ok(())
}

fn run_step(world: &mut World, step: &J) -> Result<(), String> {
    let at_text = step["at"].as_str().unwrap_or("A1");
    let (sheet, at) = address(&world.session, at_text);
    let s = &mut world.session;
    match step["do"].as_str().unwrap() {
        "copy" => world.clip = Some(s.copy(sheet, at, false)),
        "cut" => world.clip = Some(s.copy(sheet, at, true)),
        "paste" if world.outside_text.is_some() => {
            let text = world.outside_text.clone().unwrap();
            s.paste_text(sheet, at.first, &text, ExternalPaste { formulas: true })?;
        }
        "paste" => {
            let clip = world.clip.clone().expect("a clip");
            s.paste_special(&clip, sheet, at, &PasteSpecial::default())?;
        }
        "settext" => world.outside_text = step["value"].as_str().map(str::to_owned),
        "sethtml" => world.outside_html = step["value"].as_str().map(str::to_owned),
        "pasteformat" => {
            let page = world.outside_html.clone().expect("html");
            s.paste_html(sheet, at.first, &page, ExternalPaste { formulas: true })?;
        }
        "copytext" => {
            let clip = s.copy(sheet, at, false);
            world.copied_text = Some(clip.to_text(&s.book));
            world.clip = Some(clip);
        }
        "clipboardtext" => {
            let clip = world.clip.as_ref().expect("a clip");
            world.copied_text = Some(clip.to_text(&s.book));
        }
        "flashfill" => {
            s.flash_fill(sheet, at)?;
        }
        "pastespecial" => {
            let clip = world.clip.clone().expect("a clip");
            s.paste_special(&clip, sheet, at, &special_of(step))?;
        }
        "pastelink" => {
            let clip = world.clip.clone().expect("a clip");
            s.paste_link(&clip, sheet, at.first)?;
        }
        "insertcopied" => {
            let clip = world.clip.clone().expect("a clip");
            let shift = if step["shift"].as_i64() == Some(-4161) { Shift::Right } else { Shift::Down };
            s.insert_copied(&clip, sheet, at.first, shift)?;
        }
        "filter" => filter(s, sheet, at, step)?,
        "autofill" => {
            let (_, to) = address(s, step["to"].as_str().unwrap());
            let kind = FillKind::from_excel_code(step["type"].as_i64().unwrap_or(0) as i32).unwrap();
            s.fill_series_with(sheet, at, to, kind, &CustomLists::default())?;
        }
        "dataseries" => s.data_series(sheet, at, &series_of(step))?,
        "fillacross" => {
            let others: Vec<usize> = step["sheets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| s.book.sheet_index(n.as_str().unwrap()).unwrap())
                .filter(|i| *i != sheet)
                .collect();
            let what = match step["type"].as_i64().unwrap() {
                -4104 => AcrossSheets::All,
                2 => AcrossSheets::Contents,
                _ => AcrossSheets::Formats,
            };
            s.fill_across_sheets(sheet, at, &others, what)?;
        }
        other => return Err(format!("the runner has no step {other}")),
    }
    Ok(())
}

/// The options of a `dataseries` step.
pub fn series_of(step: &J) -> SeriesSpec {
    let kind = match step["type"].as_i64().unwrap_or(-4132) {
        2 => SeriesKind::Growth,
        3 => SeriesKind::Date(match step["date"].as_i64().unwrap_or(1) {
            2 => SeriesDate::Weekday,
            3 => SeriesDate::Month,
            4 => SeriesDate::Year,
            _ => SeriesDate::Day,
        }),
        4 => SeriesKind::AutoFill,
        _ => SeriesKind::Linear,
    };
    SeriesSpec {
        by_rows: step["rowcol"].as_i64() == Some(1),
        kind,
        step: step["step"].as_f64().unwrap_or(1.0),
        stop: step["stop"].as_f64(),
        trend: step["trend"].as_bool().unwrap_or(false),
    }
}

/// The options of a `pastespecial` step.
pub fn special_of(step: &J) -> PasteSpecial {
    PasteSpecial {
        kind: step["paste"].as_i64().map_or(PasteKind::All, |c| PasteKind::from_excel_code(c as i32).unwrap()),
        operation: step["op"]
            .as_i64()
            .map_or(PasteOperation::None, |c| PasteOperation::from_excel_code(c as i32).unwrap()),
        skip_blanks: step["skip"].as_bool().unwrap_or(false),
        transpose: step["transpose"].as_bool().unwrap_or(false),
    }
}

fn condition(text: &str) -> (String, String) {
    let table = [
        ("<>", "notEqual"),
        (">=", "greaterThanOrEqual"),
        ("<=", "lessThanOrEqual"),
        (">", "greaterThan"),
        ("<", "lessThan"),
        ("=", "equal"),
    ];
    for (symbol, name) in table {
        if let Some(rest) = text.strip_prefix(symbol) {
            return (name.to_owned(), rest.to_owned());
        }
    }
    ("equal".to_owned(), text.to_owned())
}

fn filter(s: &mut Session, sheet: usize, range: Range, step: &J) -> Result<(), String> {
    s.set_filter(sheet, Some(range))?;
    let mut conditions = vec![condition(step["criteria"].as_str().unwrap())];
    let and = step["operator"].as_i64() != Some(2);
    if let Some(second) = step["criteria2"].as_str() {
        conditions.push(condition(second));
    }
    let col = range.first.col + step["field"].as_u64().unwrap() as u32 - 1;
    s.filter_column(sheet, col, Some(FilterCriteria::Custom { conditions, and }))
}

/// What Excel reports about a cell, and what the engine says about the same cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub kind: String,
    pub value: String,
    pub formula: String,
    pub nf: String,
    pub bold: bool,
    pub italic: bool,
    pub fill: i64,
    pub color: i64,
    pub align: i64,
    pub border: i64,
    pub note: String,
    pub validation: i64,
    pub cf: i64,
    pub merged: bool,
}

impl Seen {
    /// The cell as Excel's dump has it, or the empty cell when the dump lists none.
    pub fn from_excel(x: Option<&J>) -> Seen {
        let blank = Seen {
            kind: "empty".into(),
            value: String::new(),
            formula: String::new(),
            nf: "General".into(),
            bold: false,
            italic: false,
            fill: -1,
            color: 0,
            align: 1,
            border: -4142,
            note: String::new(),
            validation: -1,
            cf: 0,
            merged: false,
        };
        let Some(x) = x else { return blank };
        Seen {
            kind: x["kind"].as_str().unwrap().into(),
            value: x["value"].as_str().unwrap().into(),
            formula: x["formula"].as_str().unwrap().into(),
            nf: x["nf"].as_str().unwrap().into(),
            bold: x["bold"].as_bool().unwrap(),
            italic: x["italic"].as_bool().unwrap(),
            fill: x["fill"].as_i64().unwrap(),
            color: x["color"].as_i64().unwrap(),
            align: x["align"].as_i64().unwrap(),
            border: x["border"].as_i64().unwrap(),
            note: x["note"].as_str().unwrap().into(),
            validation: x["validation"].as_i64().unwrap(),
            cf: x["cf"].as_i64().unwrap(),
            merged: x["merged"].as_bool().unwrap(),
        }
    }

    /// The engine's view of a cell.
    pub fn from_engine(session: &Session, sheet: usize, at: CellRef) -> Seen {
        let book = &session.book;
        let s = &book.sheets[sheet];
        let cell = s.cell(at);
        let value = s.value_at(at);
        let style = cell.map_or(0, |c| c.style);
        let font = book.styles.font_of(style);
        let theme = &book.styles.theme;
        let rgb_int = |color: Color| {
            color.resolve(theme).map_or(0, |(r, g, b)| r as i64 + g as i64 * 256 + b as i64 * 65536)
        };
        let fill = book.styles.fill_of(style).paint().map_or(-1, |c| rgb_int(c));
        let xf = book.styles.xf(style);
        let align = match xf.align.horizontal.as_deref() {
            None => 1,
            Some("center") => -4108,
            Some("left") => -4131,
            Some("right") => -4152,
            Some("fill") => 5,
            Some("justify") => -4130,
            Some(_) => 0,
        };
        let border = if book.styles.border_of(style).bottom.is_none() { -4142 } else { 1 };
        let validation = s
            .validations
            .iter()
            .find(|v| v.ranges.iter().any(|r| r.contains(at)))
            .map_or(-1, |v| match v.kind.as_str() {
                "whole" => 1,
                "decimal" => 2,
                "list" => 3,
                _ => 0,
            });
        let cf = s
            .conditional
            .iter()
            .filter(|c| c.ranges.iter().any(|r| r.contains(at)))
            .map(|c| c.rules.len() as i64)
            .sum();
        let (kind, shown) = match &value {
            Value::Empty => ("empty", String::new()),
            Value::Number(n) => ("number", format!("{n}")),
            Value::Text(t) => ("text", t.to_string()),
            Value::Bool(b) => ("bool", if *b { "True" } else { "False" }.to_owned()),
            Value::Error(e) => ("error", e.text().to_owned()),
        };
        Seen {
            kind: kind.into(),
            value: shown,
            formula: cell.and_then(|c| c.formula.as_ref()).map(|f| f.display()).unwrap_or_default(),
            nf: book.styles.format_code(style),
            bold: font.bold,
            italic: font.italic,
            fill,
            color: rgb_int(font.color),
            align,
            border,
            note: s.notes.get(&(at.row, at.col)).map(|n| n.text.clone()).unwrap_or_default(),
            validation,
            cf,
            merged: s.merge_at(at).is_some(),
        }
    }

    /// Whether Excel's dump would list this cell.
    pub fn is_notable(&self) -> bool {
        self.kind != "empty"
            || !self.formula.is_empty()
            || self.nf != "General"
            || self.bold
            || self.italic
            || self.fill != -1
            || self.color != 0
            || self.align != 1
            || self.border != -4142
            || !self.note.is_empty()
            || self.validation != -1
            || self.cf > 0
            || self.merged
    }
}

fn numbers_agree(a: &str, b: &str) -> bool {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => {
            let scale = x.abs().max(y.abs()).max(1e-300);
            (x - y).abs() <= scale * 1e-12 || (x - y).abs() < 1e-12
        }
        _ => a == b,
    }
}

fn same_formula(a: &str, b: &str) -> bool {
    a.replace(' ', "").eq_ignore_ascii_case(&b.replace(' ', ""))
}

/// The differences between Excel's cell and the engine's, one line each; empty when they agree. With
/// `formats` off only the value and the formula are compared.
pub fn differences(excel: &Seen, ours: &Seen, formats: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |name: &str, same: bool, e: String, o: String| {
        if !same {
            out.push(format!("{name}: excel {e}, engine {o}"));
        }
    };
    check("kind", excel.kind == ours.kind, excel.kind.clone(), ours.kind.clone());
    let same_value = if excel.kind == "number" {
        numbers_agree(&excel.value, &ours.value)
    } else {
        excel.value == ours.value
    };
    check("value", same_value, excel.value.clone(), ours.value.clone());
    check(
        "formula",
        same_formula(&excel.formula, &ours.formula),
        excel.formula.clone(),
        ours.formula.clone(),
    );
    if !formats {
        return out;
    }
    check("number format", excel.nf == ours.nf, excel.nf.clone(), ours.nf.clone());
    check("bold", excel.bold == ours.bold, excel.bold.to_string(), ours.bold.to_string());
    check("italic", excel.italic == ours.italic, excel.italic.to_string(), ours.italic.to_string());
    check("fill", excel.fill == ours.fill, excel.fill.to_string(), ours.fill.to_string());
    check("font colour", excel.color == ours.color, excel.color.to_string(), ours.color.to_string());
    check("alignment", excel.align == ours.align, excel.align.to_string(), ours.align.to_string());
    check("border", excel.border == ours.border, excel.border.to_string(), ours.border.to_string());
    check("note", excel.note == ours.note, excel.note.clone(), ours.note.clone());
    check("validation", excel.validation == ours.validation, excel.validation.to_string(), ours.validation.to_string());
    check("conditional formats", excel.cf == ours.cf, excel.cf.to_string(), ours.cf.to_string());
    check("merged", excel.merged == ours.merged, excel.merged.to_string(), ours.merged.to_string());
    out
}

/// Run every case of a scenario file against Excel's answers and return the differences, labelled by case.
pub fn check_file(folder: &str, stem: &str, skip: &[(&str, &str)]) -> Vec<String> {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(folder);
    let cases: Vec<J> = read(&folder, &format!("{stem}.in.json"));
    let answers: Vec<J> = read(&folder, &format!("{stem}.out.json"));
    let mut wrong = Vec::new();
    for (case, answer) in cases.iter().zip(&answers) {
        let label = case["name"].as_str().unwrap();
        assert_eq!(answer["name"], case["name"], "answers out of step with cases");
        if skip.iter().any(|(n, _)| *n == label) {
            continue;
        }
        let ignored: Vec<String> = skip
            .iter()
            .filter_map(|(n, _)| n.strip_prefix(label).and_then(|rest| rest.strip_prefix(':')))
            .map(str::to_owned)
            .collect();
        for line in check_case(case, answer, &ignored) {
            wrong.push(format!("{label}: {line}"));
        }
    }
    wrong
}

fn read(folder: &std::path::Path, file: &str) -> Vec<J> {
    let text = std::fs::read_to_string(folder.join(file)).unwrap();
    let text = text.trim_start_matches('\u{feff}');
    let value: J = serde_json::from_str(text).unwrap();
    value.as_array().cloned().unwrap_or_default()
}

/// Compare one case with Excel's answer.
pub fn check_case(case: &J, answer: &J, ignored_columns: &[String]) -> Vec<String> {
    let mut world = build(case);
    let result = run_steps(&mut world, case);
    let excel_error = answer["error"].as_str().unwrap_or("");
    let mut out = Vec::new();
    match (&result, excel_error.is_empty()) {
        (Err(_), true) => out.push(format!("engine refused: {}", result.as_ref().unwrap_err())),
        (Ok(()), false) => out.push(format!("engine did not refuse; excel: {excel_error}")),
        // A method Excel's object model refuses says so only with an error code.
        (Err(_), false) if excel_error.starts_with("0x800A03EC") => {}
        (Err(ours), false) if !excel_error.starts_with(ours.lines().next().unwrap_or("")) => {
            out.push(format!("refusal differs: excel {excel_error}, engine {ours}"))
        }
        _ => {}
    }
    let formats = case["formats"].as_bool() != Some(false);
    for range_text in case["dump"].as_array().into_iter().flatten().map(|d| d.as_str().unwrap()) {
        let (sheet, range) = address(&world.session, range_text);
        let sheet_name = world.session.book.sheets[sheet].name.clone();
        for at in range.cells() {
            let column = unluminous_sheet::coord::column_name(at.col);
            if ignored_columns.contains(&column) || ignored_columns.contains(&at.a1()) {
                continue;
            }
            let listed = answer["cells"].as_array().into_iter().flatten().find(|x| {
                x["cell"].as_str() == Some(&at.a1()) && x["sheet"].as_str() == Some(&sheet_name)
            });
            let excel = Seen::from_excel(listed);
            let ours = Seen::from_engine(&world.session, sheet, at);
            for d in differences(&excel, &ours, formats) {
                out.push(format!("{sheet_name}!{}: {d}", at.a1()));
            }
        }
    }
    out.extend(width_differences(case, answer, &world));
    if let Some(expected) = answer["clipboard"].as_str() {
        if world.copied_text.as_deref() != Some(expected) {
            out.push(format!("text for other programs: excel {expected:?}, engine {:?}", world.copied_text));
        }
    }
    out
}

fn width_differences(case: &J, answer: &J, world: &World) -> Vec<String> {
    let mut out = Vec::new();
    if case["formats"].as_bool() == Some(false) || answer["widths"].is_null() {
        return out;
    }
    let s = &world.session.book.sheets[0];
    for (letter, width) in answer["widths"].as_object().into_iter().flatten() {
        let col = unluminous_sheet::coord::column_number(letter).unwrap();
        let excel = width.as_f64().unwrap();
        let ours = s.column(col).and_then(|c| c.width).map(|w| w - 0.7109375);
        let agrees = match ours {
            Some(w) => (w - excel).abs() < 0.01,
            None => (excel - 8.38).abs() < 0.1,
        };
        if !agrees {
            out.push(format!("column {letter} width: excel {excel}, engine {ours:?}"));
        }
    }
    out
}
