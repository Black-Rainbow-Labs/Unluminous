//! Sheet protection, edit ranges and workbook structure protection against Excel.
//!
//! `fixtures/protection/protect-matrix.tsv` is what Excel did when 43 operations were tried on a protected
//! sheet with each allow flag of Protect Sheet on its own (`tools/excel-oracle/protect-matrix.ps1`); the engine
//! has to allow and refuse the same ones. The passwords are checked both ways: a workbook Excel protected
//! unprotects here, and a workbook protected here unprotects in Excel.

mod describe;

use describe::*;
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::{Clear, Direction, Session, Shift};
use unluminous_sheet::model::{CfRule, LinkTarget, Validation, Visibility, Workbook};
use unluminous_sheet::protect::{
    legacy_hash, Secret, SheetProtection, COMMAND_REFUSED, LOCKED_CELL, STRUCTURE_LOCKED,
    WRONG_PASSWORD,
};
use unluminous_sheet::styles::{Color, FontEdit, StyleChange};
use unluminous_sheet::Value;

const LANE: &str = "protection";
const NOW: f64 = 46319.5;

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn rg(a: &str) -> Range {
    Range::parse(a).unwrap()
}

fn tsv(name: &str) -> Vec<Vec<String>> {
    std::fs::read_to_string(fixture_dir(LANE).join(name))
        .unwrap()
        .lines()
        .map(|l| l.split('\t').map(str::to_owned).collect())
        .collect()
}

// ---------------------------------------------------------------------------------------------------
// The protected sheet of the matrix.

/// The sheet `protect-matrix.ps1` builds: A1:A3 locked, B1:E8 unlocked, row 5 unlocked and row 6 locked,
/// on a workbook of three sheets.
fn matrix_book() -> Session {
    let mut s = Session::new(Workbook::new(), NOW);
    s.add_sheet(Some("Second"), None).unwrap();
    s.add_sheet(Some("Third"), None).unwrap();
    s.book.active_sheet = 0;
    for r in 1..=3 {
        s.set_value(0, at(&format!("A{r}")), Value::Number(1.0)).unwrap();
    }
    for r in 1..=8 {
        for c in ["B", "C", "D", "E"] {
            s.set_value(0, at(&format!("{c}{r}")), Value::Number(2.0)).unwrap();
        }
    }
    let unlocked = StyleChange::Locked(false);
    s.set_style(0, &[rg("B1:E8")], &unlocked).unwrap();
    s.set_style(0, &[rg("5:5")], &unlocked).unwrap();
    s.set_style(0, &[rg("6:6")], &StyleChange::Locked(true)).unwrap();
    s.set_style(0, &[rg("E1:E8")], &unlocked).unwrap();
    s
}

/// The options Protect Sheet is given for a row of the matrix.
fn options_for(flag: &str) -> SheetProtection {
    let mut p = SheetProtection {
        objects: flag != "objectsOff",
        scenarios: flag != "scenariosOff",
        ..Default::default()
    };
    let allow = &mut p.allow;
    match flag {
        "formatCells" => allow.format_cells = true,
        "formatColumns" => allow.format_columns = true,
        "formatRows" => allow.format_rows = true,
        "insertColumns" => allow.insert_columns = true,
        "insertRows" => allow.insert_rows = true,
        "insertHyperlinks" => allow.insert_hyperlinks = true,
        "deleteColumns" => allow.delete_columns = true,
        "deleteRows" => allow.delete_rows = true,
        "sort" => allow.sort = true,
        "autoFilter" => allow.auto_filter = true,
        "pivotTables" => allow.pivot_tables = true,
        _ => {}
    }
    p
}

fn bold() -> StyleChange {
    StyleChange::Font(FontEdit { bold: Some(true), ..Default::default() })
}

fn validation() -> Validation {
    Validation {
        ranges: vec![rg("B1")],
        kind: "list".to_owned(),
        operator: None,
        formula1: Some("\"a,b\"".to_owned()),
        formula2: None,
        allow_blank: true,
        show_dropdown: true,
        prompt_title: None,
        prompt: None,
        error_title: None,
        error: None,
        error_style: None,
        attrs: Vec::new(),
    }
}

fn rule() -> CfRule {
    CfRule {
        kind: "cellIs".to_owned(),
        dxf: None,
        priority: 1,
        stop: false,
        operator: Some("equal".to_owned()),
        text: None,
        rank: None,
        percent: false,
        bottom: false,
        above_average: None,
        formulas: vec!["0".to_owned()],
        color_scale: None,
        data_bar: None,
        attrs: Vec::new(),
        raw_children: String::new(),
    }
}

/// Do an operation of the matrix with the engine. `None` when the engine has no such operation.
fn run_op(s: &mut Session, op: &str) -> Option<Result<(), String>> {
    Some(match op {
        "set locked A1" => s.set_input(0, at("A1"), "9"),
        "set unlocked B1" => s.set_input(0, at("B1"), "9"),
        "clear unlocked B1" => s.clear(0, &[rg("B1")], Clear::Contents),
        "clear locked A1" => s.clear(0, &[rg("A1")], Clear::Contents),
        "bold unlocked B1" => s.set_style(0, &[rg("B1")], &bold()),
        "bold locked A1" => s.set_style(0, &[rg("A1")], &bold()),
        "numfmt unlocked B1" => {
            s.set_style(0, &[rg("B1")], &StyleChange::NumberFormat("0.00".into()))
        }
        "fill unlocked B1" => {
            s.set_style(0, &[rg("B1")], &StyleChange::Fill(Some(Color::rgb(255, 0, 0))))
        }
        "merge unlocked B1:C1" => s.merge(0, rg("B1:C1"), false),
        "colwidth B" => s.set_column_width(0, 1, 1, Some(20.0)),
        "col hide B" => s.hide_columns(0, 1, 1, true),
        "rowheight 1" => s.set_row_height(0, 0, 0, Some(30.0)),
        "row hide 1" => s.hide_rows(0, 0, 0, true),
        "insert row 2" => s.insert(0, rg("2:2"), Shift::WholeRows),
        "insert col B" => s.insert(0, rg("B:B"), Shift::WholeColumns),
        "delete row 5 unlocked" => s.delete(0, rg("5:5"), Shift::WholeRows),
        "delete row 6 locked" => s.delete(0, rg("6:6"), Shift::WholeRows),
        "delete col E unlocked" => s.delete(0, rg("E:E"), Shift::WholeColumns),
        "delete col A locked" => s.delete(0, rg("A:A"), Shift::WholeColumns),
        "sort unlocked B1:C3" => s.sort(0, rg("B1:C3"), &[(1, true)], false),
        "sort locked A1:A3" => s.sort(0, rg("A1:A3"), &[(0, true)], false),
        "autofilter B1:C3" => s.set_filter(0, Some(rg("B1:C3"))),
        "hyperlink unlocked B1" => {
            s.set_link(0, rg("B1"), Some(LinkTarget::External("http://example.com".into())), None)
        }
        "hyperlink locked A1" => {
            s.set_link(0, rg("A1"), Some(LinkTarget::External("http://example.com".into())), None)
        }
        "note unlocked B1" => s.set_note(0, at("B1"), Some("x"), "a"),
        "validation B1" => s.add_validation(0, validation()),
        "conditional B1" => s.add_conditional(0, vec![rg("B1")], rule()),
        "name define" => s.set_name("Nm", "Sheet1!$A$1", None),
        "freeze" => s.freeze(0, 1, 0),
        "fill down unlocked" => s.fill(0, rg("B1:B3"), Direction::Down),
        "text to columns B" => s.text_to_columns(0, rg("B1:B3"), ","),
        "remove dups B1:C3" => s.remove_duplicates(0, rg("B1:C3"), &[0, 1], true).map(|_| ()),
        "list object" => s.add_table(0, rg("B1:C3"), None).map(|_| ()),
        "tab color" => s.set_tab_color(0, Some(Color::rgb(255, 0, 0))),
        "rename sheet" => s.rename_sheet(0, "Renamed"),
        "delete sheet" => s.delete_sheet(0),
        "unhide sheet" => s.set_sheet_visibility(0, Visibility::Hidden),
        "scenario add" => s.add_scenario(
            0,
            "S1",
            &[rg("B1")],
            Some(&[Value::Number(5.0)]),
            None,
            "x",
            true,
            false,
        ),
        "paste unlocked B1" => {
            let clip = s.copy(0, rg("B2"), false);
            s.paste(&clip, 0, rg("B1"), unluminous_sheet::edit::PasteMode::Values).map(|_| ())
        }
        "cut paste unlocked" => {
            let clip = s.copy(0, rg("B2"), true);
            s.paste(&clip, 0, rg("C1"), unluminous_sheet::edit::PasteMode::All).map(|_| ())
        }
        "row insert, hidden row 7" => s.hide_rows(0, 6, 6, false),
        // Charts and shapes need drawing objects the engine does not make from a command here.
        _ => return None,
    })
}

#[test]
fn a_protected_sheet_allows_and_refuses_what_excel_allows_and_refuses() {
    let rows = tsv("protect-matrix.tsv");
    assert_eq!(rows.len(), 660);
    let mut checked = 0;
    let mut skipped = std::collections::BTreeSet::new();
    let mut wrong = Vec::new();
    for row in &rows {
        let (op, flag, excel) = (row[0].as_str(), row[1].as_str(), row[2].as_str());
        let mut s = matrix_book();
        s.protect_sheet(0, None, &options_for(flag)).unwrap();
        let Some(result) = run_op(&mut s, op) else {
            skipped.insert(op.to_owned());
            continue;
        };
        checked += 1;
        if result.is_ok() != (excel == "ok") {
            wrong.push(format!("{op} with {flag}: Excel {excel:?}, engine {result:?}"));
        }
    }
    assert!(checked >= 560, "{checked} checked, {skipped:?} skipped");
    assert!(wrong.is_empty(), "{} differ from Excel:\n{}", wrong.len(), wrong.join("\n"));
    // What was left out is the drawing objects only.
    assert!(
        skipped.iter().all(|o| ["chart", "shape", "group rows"].contains(&o.as_str())),
        "{skipped:?}"
    );
}

#[test]
fn a_refusal_says_what_excel_says() {
    let mut s = matrix_book();
    s.protect_sheet(0, None, &options_for("none")).unwrap();
    assert_eq!(s.set_input(0, at("A1"), "1").unwrap_err(), LOCKED_CELL);
    assert_eq!(s.set_style(0, &[rg("B1")], &bold()).unwrap_err(), COMMAND_REFUSED);
    assert_eq!(
        s.add_table(0, rg("B1:C3"), None).unwrap_err(),
        "Table features aren't available because the sheet is protected."
    );
    // The messages are the ones in the matrix.
    let rows = tsv("protect-matrix.tsv");
    let message = |op: &str| rows.iter().find(|r| r[0] == op && r[1] == "none").unwrap()[2].clone();
    assert_eq!(message("set locked A1"), LOCKED_CELL);
    assert_eq!(message("autofilter B1:C3"), COMMAND_REFUSED);
    assert_eq!(
        message("list object"),
        "Table features aren't available because the sheet is protected."
    );
    // Nothing changed.
    assert!(s.book.sheets[0].tab_color.is_none());
}

#[test]
fn a_refused_change_leaves_the_workbook_and_the_undo_history_alone() {
    let mut s = matrix_book();
    s.protect_sheet(0, None, &options_for("none")).unwrap();
    let before = describe(&s.book);
    let undo = s.undo_label().map(str::to_owned);
    for op in [
        "set locked A1",
        "bold unlocked B1",
        "insert row 2",
        "merge unlocked B1:C1",
        "delete row 6 locked",
    ] {
        assert!(run_op(&mut s, op).unwrap().is_err(), "{op}");
    }
    assert_eq!(describe(&s.book), before);
    assert_eq!(s.undo_label().map(str::to_owned), undo);
}

#[test]
fn cutting_locked_cells_is_refused_even_when_the_place_they_go_is_free() {
    let mut s = matrix_book();
    s.protect_sheet(0, None, &options_for("none")).unwrap();
    let clip = s.copy(0, rg("A1"), true);
    let err = s.paste(&clip, 0, rg("C1"), unluminous_sheet::edit::PasteMode::All).unwrap_err();
    assert_eq!(err, LOCKED_CELL);
    assert_eq!(s.book.sheets[0].value_at(at("A1")), Value::Number(1.0));
}

#[test]
fn selecting_cells_follows_the_select_flags() {
    let mut s = matrix_book();
    assert!(s.can_select(0, at("A1")));
    let mut p = options_for("none");
    p.allow.select_locked_cells = false;
    s.protect_sheet(0, None, &p).unwrap();
    assert!(!s.can_select(0, at("A1")));
    assert!(s.can_select(0, at("B1")));
    s.unprotect_sheet(0, None).unwrap();
    let mut p = options_for("none");
    p.allow.select_unlocked_cells = false;
    s.protect_sheet(0, None, &p).unwrap();
    assert!(s.can_select(0, at("A1")));
    assert!(!s.can_select(0, at("B1")));
}

// ---------------------------------------------------------------------------------------------------
// Passwords.

#[test]
fn a_workbook_excel_protected_unprotects_with_the_passwords_excel_was_given() {
    let mut s = Session::new(read_fixture(LANE, "protect-excel.xlsx"), NOW);
    let p1 = s.sheet_protection(0).unwrap();
    assert!(p1.sheet && p1.objects && p1.scenarios && !p1.allow.format_cells);
    assert!(p1.secret.as_ref().is_some_and(|x| x.matches("secret") && !x.matches("Secret")));
    let p2 = s.sheet_protection(1).unwrap();
    assert!(p2.secret.is_none() && p2.allow.format_cells && p2.allow.pivot_tables && p2.allow.sort);
    let ranges = s.edit_ranges(2);
    assert_eq!(
        ranges.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["Open range", "Locked range"]
    );
    assert!(ranges[0].secret.is_none());
    assert!(ranges[1].secret.as_ref().unwrap().matches("rangepw"));
    assert!(s.sheet_protection(2).unwrap().secret.unwrap().matches("sheetpw"));
    assert!(s.sheet_protection(3).is_none());
    let book = s.workbook_protection().unwrap();
    assert!(book.lock_structure && !book.lock_windows && book.secret.unwrap().matches("bpw"));
    // Wrong passwords are refused with Excel's sentence, right ones unprotect.
    let excel_wrong =
        tsv("edit-ranges.tsv").iter().find(|r| r[0].contains("wrong password")).unwrap()[1].clone();
    assert_eq!(excel_wrong, WRONG_PASSWORD);
    assert_eq!(s.unprotect_sheet(0, Some("nope")).unwrap_err(), WRONG_PASSWORD);
    assert_eq!(s.unprotect_sheet(0, None).unwrap_err(), WRONG_PASSWORD);
    s.unprotect_sheet(0, Some("secret")).unwrap();
    assert!(s.sheet_protection(0).is_none());
    s.unprotect_sheet(1, None).unwrap();
    assert_eq!(s.unprotect_workbook(Some("x")).unwrap_err(), WRONG_PASSWORD);
    s.unprotect_workbook(Some("bpw")).unwrap();
    assert!(s.workbook_protection().is_none());
    // Unprotecting is one undo step.
    s.undo();
    assert!(s.workbook_protection().is_some());
}

#[test]
fn a_password_is_never_kept_and_is_spelled_the_way_excel_spells_it() {
    let mut s = Session::new(Workbook::new(), NOW);
    s.protect_sheet(0, Some("pässwörd €"), &SheetProtection::default()).unwrap();
    let secret = s.sheet_protection(0).unwrap().secret.unwrap();
    let Secret::Hashed { algorithm, hash, salt, spin } = &secret else { panic!("{secret:?}") };
    assert_eq!((algorithm.as_str(), *spin), ("SHA-512", 100_000));
    assert_eq!(hash.len(), 88);
    assert_eq!(salt.len(), 24);
    let xml = String::from_utf8(
        unluminous_sheet::zip::read(&unluminous_sheet::xlsx::write(&s.book).unwrap())
            .unwrap()
            .into_iter()
            .find(|e| e.name == "xl/worksheets/sheet1.xml")
            .unwrap()
            .data,
    )
    .unwrap();
    assert!(!xml.contains("pässwörd"));
    assert!(xml.contains("algorithmName=\"SHA-512\"") && xml.contains("spinCount=\"100000\""));
    assert!(s.protect_sheet(0, None, &SheetProtection::default()).is_err(), "already protected");
    assert_eq!(s.unprotect_sheet(0, Some("password €")).unwrap_err(), WRONG_PASSWORD);
    s.unprotect_sheet(0, Some("pässwörd €")).unwrap();
    // An empty password is no password.
    s.protect_sheet(0, Some(""), &SheetProtection::default()).unwrap();
    assert!(s.sheet_protection(0).unwrap().secret.is_none());
}

#[test]
fn a_legacy_password_is_read_and_written_back_only_when_the_element_had_one() {
    let mut s = Session::new(Workbook::new(), NOW);
    let legacy =
        SheetProtection { secret: Some(Secret::Legacy(legacy_hash("test"))), ..Default::default() };
    s.book.sheets[0].set_protection(Some(&legacy));
    let back =
        unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    let again = back.sheets[0].protection().unwrap();
    assert_eq!(again.secret, Some(Secret::Legacy(legacy_hash("test"))));
    let mut s = Session::new(back, NOW);
    assert_eq!(s.unprotect_sheet(0, Some("Test")).unwrap_err(), WRONG_PASSWORD);
    s.unprotect_sheet(0, Some("test")).unwrap();
}

// ---------------------------------------------------------------------------------------------------
// Edit ranges.

#[test]
fn an_edit_range_lets_its_cells_be_edited_with_its_password_if_it_has_one() {
    let mut s = Session::new(Workbook::new(), NOW);
    s.add_sheet(None, None).unwrap();
    s.book.active_sheet = 0;
    for a in ["A1", "A2", "B1", "B2"] {
        s.set_value(0, at(a), Value::Number(5.0)).unwrap();
    }
    s.add_edit_range(0, "Open range", &[rg("A1:A2")], None).unwrap();
    s.add_edit_range(0, "Locked range", &[rg("B1:B2")], Some("rangepw")).unwrap();
    assert!(s.add_edit_range(0, "open RANGE", &[rg("C1")], None).is_err(), "titles are unique");
    s.protect_sheet(0, Some("sheetpw"), &SheetProtection::default()).unwrap();
    let excel = tsv("edit-ranges.tsv");
    let line = |what: &str| excel.iter().find(|r| r[0].starts_with(what)).unwrap()[1].clone();
    // The same attempts Excel was given, with the same answers.
    assert_eq!(
        s.set_input(0, at("A1"), "9").is_ok(),
        line("edit a cell in the range with no password") == "ok"
    );
    assert_eq!(
        s.set_input(0, at("B1"), "9").unwrap_err(),
        line("edit a cell in the range with a password")
    );
    assert_eq!(s.set_input(0, at("C1"), "9").unwrap_err(), line("edit a locked cell outside"));
    assert_eq!(
        s.unlock_edit_range(0, "Locked range", "nope").unwrap_err(),
        line("give the range the wrong")
    );
    assert!(s.unlock_edit_range(0, "Locked range", "rangepw").is_ok());
    assert_eq!(
        s.set_input(0, at("B1"), "9").is_ok(),
        line("edit the cell in the range once") == "ok"
    );
    // A range is added or removed only while the sheet is unprotected, and removing one with a
    // password asks for it.
    assert!(s.add_edit_range(0, "More", &[rg("D1")], None).is_err());
    s.unprotect_sheet(0, Some("sheetpw")).unwrap();
    assert_eq!(s.remove_edit_range(0, "Locked range", Some("x")).unwrap_err(), WRONG_PASSWORD);
    s.remove_edit_range(0, "Locked range", Some("rangepw")).unwrap();
    assert_eq!(s.edit_ranges(0).len(), 1);
    // The ranges move with the cells they name, as the kept XML does.
    s.insert(0, rg("1:1"), Shift::WholeRows).unwrap();
    assert_eq!(s.edit_ranges(0)[0].ranges, vec![rg("A2:A3")]);
    let back =
        unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    assert_eq!(back.sheets[0].protected_ranges(), s.edit_ranges(0));
}

// ---------------------------------------------------------------------------------------------------
// Workbook structure.

#[test]
fn a_locked_structure_refuses_what_excel_refuses() {
    let mut s = Session::new(Workbook::new(), NOW);
    s.add_sheet(Some("Two"), None).unwrap();
    s.add_sheet(Some("Three"), None).unwrap();
    s.protect_workbook(Some("bpw"), true, false).unwrap();
    let excel = tsv("structure.tsv");
    let line = |what: &str| excel.iter().find(|r| r[0] == what).unwrap()[1].clone();
    assert_eq!(line("copy a sheet"), STRUCTURE_LOCKED);
    let refused = [
        ("add a sheet", s.add_sheet(None, None).map(|_| ())),
        ("rename a sheet", s.rename_sheet(1, "Renamed")),
        ("delete a sheet", s.delete_sheet(1)),
        ("move a sheet", s.move_sheet(1, 0)),
        ("copy a sheet", s.duplicate_sheet(1).map(|_| ())),
        ("hide a sheet", s.set_sheet_visibility(1, Visibility::Hidden)),
        ("change a tab colour", s.set_tab_color(1, Some(Color::rgb(255, 0, 0)))),
    ];
    for (what, result) in refused {
        assert_ne!(line(what), "ok", "{what}");
        assert_eq!(result.unwrap_err(), STRUCTURE_LOCKED, "{what}");
    }
    assert_eq!(line("edit a cell"), "ok");
    assert!(s.set_input(1, at("A1"), "3").is_ok());
    assert_eq!(s.book.sheets.len(), 3);
    assert_eq!(
        s.unprotect_workbook(Some("x")).unwrap_err(),
        line("unprotect with the wrong password")
    );
    s.unprotect_workbook(Some("bpw")).unwrap();
    assert_eq!(line("add a sheet once unprotected"), "ok");
    assert!(s.add_sheet(None, None).is_ok());
    assert!(s.protect_workbook(None, false, false).is_err(), "something has to be locked");
}

// ---------------------------------------------------------------------------------------------------
// Excel and the engine agree about a file either one protected.

#[test]
fn excel_unprotects_with_the_passwords_unluminous_protected_with() {
    if !have_excel("what Excel makes of the passwords written here") {
        return;
    }
    let mut s = Session::new(Workbook::new(), NOW);
    s.add_sheet(Some("Legacy"), None).unwrap();
    s.add_sheet(Some("Ranges"), None).unwrap();
    s.add_sheet(Some("Open"), None).unwrap();
    s.book.active_sheet = 0;
    s.set_value(2, at("A1"), Value::Number(1.0)).unwrap();
    s.add_edit_range(2, "Editable", &[rg("A1")], Some("rangepw")).unwrap();
    s.add_edit_range(2, "Free", &[rg("B1")], None).unwrap();
    let mut allow = SheetProtection::default();
    allow.allow.format_cells = true;
    allow.allow.sort = true;
    allow.allow.delete_rows = true;
    allow.allow.select_locked_cells = false;
    s.protect_sheet(0, Some("pässwörd €"), &allow).unwrap();
    s.book.sheets[1].set_protection(Some(&SheetProtection {
        secret: Some(Secret::Legacy(legacy_hash("test"))),
        ..Default::default()
    }));
    s.protect_sheet(2, Some("sheetpw"), &SheetProtection::default()).unwrap();
    s.protect_workbook(Some("bpw"), true, false).unwrap();
    let dir = scratch("protection");
    let path = dir.join("built.xlsx");
    std::fs::write(&path, unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    let plan = r#"{"sheets":[{"index":1,"password":"pässwörd €"},{"index":2,"password":"test"},{"index":3,"password":"sheetpw"}],"workbook":"bpw","ranges":[{"sheet":3,"title":"Editable","password":"rangepw"}]}"#;
    use base64::Engine;
    let plan = base64::engine::general_purpose::STANDARD.encode(plan);
    let text =
        run_script("check-protection.ps1", &["-Path", path.to_str().unwrap(), "-Plan", &plan]);
    let excel: serde_json::Value =
        serde_json::from_str(text.trim().trim_start_matches('\u{FEFF}')).unwrap();
    // The flags Excel reads are the flags that were written.
    let first = &excel["sheets"][0];
    assert_eq!(first["contents"], true);
    assert_eq!(first["objects"], true);
    assert_eq!(first["scenarios"], true);
    assert_eq!(first["formatCells"], true);
    assert_eq!(first["sort"], true);
    assert_eq!(first["deleteRows"], true);
    assert_eq!(first["formatColumns"], false);
    assert_eq!(first["insertRows"], false);
    assert_eq!(first["pivotTables"], false);
    let ranges = excel["sheets"][2]["ranges"].as_array().unwrap();
    assert_eq!(
        ranges.iter().map(|r| r["title"].as_str().unwrap()).collect::<Vec<_>>(),
        ["Editable", "Free"]
    );
    assert_eq!(excel["workbook"]["structure"], true);
    // A wrong password is refused and the right one is accepted, for every kind of secret.
    let checks = excel["checks"].as_array().unwrap();
    let result = |what: &str| {
        checks
            .iter()
            .find(|c| c["what"] == what)
            .unwrap_or_else(|| panic!("Excel gave no answer for {what}: {checks:#?}"))["result"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    for what in [
        "sheet 1 right password",
        "sheet 2 right password",
        "sheet 3 right password",
        "range Editable right password",
        "workbook right password",
    ] {
        assert_eq!(result(what), "ok", "{what}");
    }
    for what in [
        "sheet 1 wrong password",
        "sheet 2 wrong password",
        "sheet 3 wrong password",
        "range Editable wrong password",
        "workbook wrong password",
    ] {
        assert_eq!(result(what), WRONG_PASSWORD, "{what}");
    }
    for what in [
        "sheet 1 unprotected",
        "sheet 2 unprotected",
        "sheet 3 unprotected",
        "workbook unprotected",
    ] {
        assert_eq!(result(what), "True", "{what}");
    }
}

#[test]
fn what_excel_protected_reads_back_with_the_flags_excel_reports() {
    if !have_excel("what Excel reports of the protection it wrote") {
        return;
    }
    let path = fixture_dir(LANE).join("protect-excel.xlsx");
    let text = run_script("check-protection.ps1", &["-Path", path.to_str().unwrap()]);
    let excel: serde_json::Value =
        serde_json::from_str(text.trim().trim_start_matches('\u{FEFF}')).unwrap();
    let book = read_fixture(LANE, "protect-excel.xlsx");
    for (i, sheet) in excel["sheets"].as_array().unwrap().iter().enumerate() {
        let ours = book.sheets[i].protection();
        let p = ours.clone().unwrap_or_default();
        assert_eq!(
            sheet["contents"].as_bool().unwrap(),
            ours.as_ref().is_some_and(|p| p.sheet),
            "sheet {i}"
        );
        if ours.is_none() {
            continue;
        }
        for (name, ours) in [
            ("formatCells", p.allow.format_cells),
            ("formatColumns", p.allow.format_columns),
            ("formatRows", p.allow.format_rows),
            ("insertColumns", p.allow.insert_columns),
            ("insertRows", p.allow.insert_rows),
            ("insertHyperlinks", p.allow.insert_hyperlinks),
            ("deleteColumns", p.allow.delete_columns),
            ("deleteRows", p.allow.delete_rows),
            ("sort", p.allow.sort),
            ("autoFilter", p.allow.auto_filter),
            ("pivotTables", p.allow.pivot_tables),
        ] {
            assert_eq!(sheet[name].as_bool().unwrap(), ours, "sheet {i} {name}");
        }
    }
}
