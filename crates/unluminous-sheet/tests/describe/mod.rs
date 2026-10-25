//! Helpers the What If, protection, style and theme tests share: describe a workbook as lines of text so
//! two workbooks (one made by Excel, one by the engine) can be compared, and ask Excel about a file.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use unluminous_sheet::coord::CellRef;
use unluminous_sheet::model::Workbook;
use unluminous_sheet::styles::{Color, Edge};
use unluminous_sheet::Value;

pub fn fixture_dir(lane: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(lane)
}

pub fn read_fixture(lane: &str, name: &str) -> Workbook {
    let bytes = std::fs::read(fixture_dir(lane).join(name))
        .unwrap_or_else(|e| panic!("fixture {lane}/{name}: {e}"));
    unluminous_sheet::xlsx::read(&bytes).unwrap_or_else(|e| panic!("fixture {lane}/{name}: {e}"))
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf()
}

/// A number written to 12 significant digits, so two computations of one value compare equal.
pub fn rounded(n: f64) -> String {
    if n == 0.0 {
        return "0".to_owned();
    }
    let digits = 11 - n.abs().log10().floor() as i32;
    let scale = 10f64.powi(digits);
    format!("{}", (n * scale).round() / scale)
}

pub fn value_text(v: &Value) -> String {
    match v {
        Value::Number(n) => rounded(*n),
        other => other.to_text(),
    }
}

fn edge(e: &Edge, theme: &[u32]) -> String {
    if e.is_none() {
        "-".to_owned()
    } else {
        format!("{}:{}", e.style, e.color.to_hex(theme))
    }
}

fn hex(c: &Color, theme: &[u32]) -> String {
    c.to_hex(theme)
}

/// One line for a cell: its value, its formula and everything its format says.
pub fn describe_cell(book: &Workbook, sheet: usize, at: CellRef) -> Option<String> {
    let s = &book.sheets[sheet];
    let cell = s.cell(at)?;
    let styles = &book.styles;
    let theme = &styles.theme;
    let xf = styles.xf(cell.style);
    let font = styles.font_of(cell.style);
    let fill = styles.fill_of(cell.style);
    let border = styles.border_of(cell.style);
    let formula = cell.formula.as_ref().map(|f| f.display()).unwrap_or_default();
    if cell.value.is_empty() && formula.is_empty() && cell.style == 0 {
        return None;
    }
    let paint = fill.paint().map(|c| hex(&c, theme)).unwrap_or_else(|| "none".to_owned());
    let look = format!(
        "fmt=[{}] style=[{}] font=[{}/{}/{}{}{}{}/{}] fill=[{}] border=[l {} r {} t {} b {}] align=[{:?} {:?} wrap={} indent={}] locked={:?} hidden={:?}",
        styles.format_code(cell.style),
        styles.style_name_of(cell.style),
        font.name,
        font.size,
        if font.bold { "b" } else { "" },
        if font.italic { "i" } else { "" },
        if font.underline != unluminous_sheet::styles::Underline::None { "u" } else { "" },
        if font.strike { "s" } else { "" },
        hex(&font.color, theme),
        paint,
        edge(&border.left, theme),
        edge(&border.right, theme),
        edge(&border.top, theme),
        edge(&border.bottom, theme),
        xf.align.horizontal,
        xf.align.vertical,
        xf.align.wrap,
        xf.align.indent,
        xf.locked,
        xf.hidden,
    );
    // An empty cell that looks like Normal is a cell with a format of its own that changes nothing, which
    // is not worth a line.
    let plain = styles.xf(0);
    let normal_look = format!(
        "fmt=[General] style=[Normal] font=[{}/{}//{}] fill=[none] border=[l - r - t - b -] align=[None None wrap=false indent=0] locked={:?} hidden={:?}",
        styles.font_of(0).name,
        styles.font_of(0).size,
        hex(&styles.font_of(0).color, theme),
        plain.locked,
        plain.hidden
    );
    if cell.value.is_empty() && formula.is_empty() && look == normal_look {
        return None;
    }
    Some(format!("{}!{} [{}] f=[{}] {look}", s.name, at, value_text(&cell.value), formula))
}

/// Every cell of every sheet, one line each, in sheet then row then column order.
pub fn describe(book: &Workbook) -> Vec<String> {
    let mut out = Vec::new();
    for (si, s) in book.sheets.iter().enumerate() {
        out.push(format!(
            "sheet {} state={:?} grid={} tab={:?}",
            s.name,
            s.state,
            s.show_grid,
            s.tab_color.map(|c| c.to_hex(&book.styles.theme))
        ));
        for &(r, c) in s.cells.keys() {
            if let Some(line) = describe_cell(book, si, CellRef::new(r, c)) {
                out.push(line);
            }
        }
        for (&(r, c), v) in &s.spills.values {
            out.push(format!("{}!{} spill {}", s.name, CellRef::new(r, c), value_text(v)));
        }
    }
    out
}

/// The first lines that differ, for a failure message.
pub fn differences(ours: &[String], theirs: &[String]) -> String {
    let mut out = String::new();
    for line in ours.iter().filter(|l| !theirs.contains(l)).take(8) {
        out.push_str(&format!("  ours only:   {line}\n"));
    }
    for line in theirs.iter().filter(|l| !ours.contains(l)).take(8) {
        out.push_str(&format!("  Excel only:  {line}\n"));
    }
    out
}

pub fn excel_is_installed() -> bool {
    cfg!(windows)
        && std::process::Command::new("reg")
            .args(["query", "HKCR\\Excel.Application"])
            .output()
            .is_ok_and(|o| o.status.success())
}

/// Whether a test that needs Excel can run. Without Excel it passes with a printed line, and
/// `UNLUMINOUS_REQUIRE_EXCEL=1` makes a missing Excel a failure.
pub fn have_excel(what: &str) -> bool {
    if excel_is_installed() {
        return true;
    }
    assert!(
        std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err(),
        "UNLUMINOUS_REQUIRE_EXCEL is set and Excel is not installed"
    );
    println!("Excel is not installed here, so {what} was not checked");
    false
}

/// Run a PowerShell script from `tools/excel-oracle` and answer its output.
pub fn run_script(script: &str, args: &[&str]) -> String {
    let output = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(repo_root().join("tools/excel-oracle").join(script))
        .args(args)
        .output()
        .expect("pwsh runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "{script} failed:\n{text}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    text
}

/// A folder for files a test makes, empty and its own.
pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("unluminous-sheet-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
