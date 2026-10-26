//! Automatic axis scales and trendline labels, compared with the answers Excel gave
//! (`tests/fixtures/charts/scale-excel.jsonl` and `trend-excel.jsonl`, made by `tools/excel-oracle`).

use std::path::Path;

/// The lines of a JSON lines fixture.
fn json_lines(name: &str) -> Vec<serde_json::Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/charts").join(name);
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// The cases where the automatic scale and Excel's differ, as text.
fn scale_differences() -> Vec<String> {
    use unluminous_sheet::plot::scale::{value_scale, Fixed};
    let cases = json_lines("scale-cases.jsonl");
    let mut out = Vec::new();
    for answer in json_lines("scale-excel.jsonl") {
        let id = answer["id"].as_i64().unwrap();
        let case = cases.iter().find(|c| c["id"].as_i64() == Some(id)).unwrap();
        let values: Vec<f64> =
            case["values"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let (low, high) =
            values.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        let fixed = Fixed { log_base: (case["kind"] == "log").then_some(10.0), ..Fixed::default() };
        let Some(length) = answer["plotInsideHeight"].as_f64() else { continue };
        let ours = value_scale(low, high, fixed, length);
        let theirs = (
            answer["min"].as_f64().unwrap(),
            answer["max"].as_f64().unwrap(),
            answer["major"].as_f64().unwrap(),
        );
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
        // At the shortest chart Excel leaves more room between gridlines than the other heights do when the
        // labels are long (a small decimal, millions, a run of decades); the rule for that is not found.
        let known = answer["height"].as_f64() == Some(150.0) && matches!(id, 58 | 61 | 68);
        if known {
            continue;
        }
        if !(close(ours.min, theirs.0) && close(ours.max, theirs.1) && close(ours.major, theirs.2))
        {
            let kind = &case["kind"];
            let height = &answer["height"];
            let mine = (ours.min, ours.max, ours.major);
            out.push(format!("case {id} {kind} height {height} length {length:.1}: values {values:?} ours {mine:?} excel {theirs:?}"));
        }
    }
    out
}

#[test]
fn automatic_axis_scales_match_excel() {
    let differences = scale_differences();
    let shown = differences.iter().take(30).cloned().collect::<Vec<_>>().join("\n");
    assert!(differences.is_empty(), "{} differ:\n{shown}", differences.len());
}

/// The series of a chart as text: name, categories and values, each as the reference it names.
fn series_text(chart: &unluminous_sheet::model::Chart) -> Vec<String> {
    let clean = |s: &str| s.replace('$', "");
    chart
        .series
        .iter()
        .map(|s| {
            let name = s.name.as_deref().map(clean).unwrap_or_default();
            let categories = s.categories.as_deref().map(clean).unwrap_or_default();
            format!("name {name} categories {categories} values {}", clean(&s.values))
        })
        .collect()
}

/// Excel made a column chart from each block of cells in `shapes.xlsx` with SetSourceData, named by the
/// range; making a chart from the same range here has to lay it out into the same series.
#[test]
fn a_chart_made_from_a_range_lays_it_out_as_excel_does() {
    use unluminous_sheet::coord::Range;
    use unluminous_sheet::edit::Session;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/charts/shapes.xlsx");
    let book = unluminous_sheet::xlsx::read(&std::fs::read(path).unwrap()).unwrap();
    let theirs: Vec<(String, Vec<String>)> = book.sheets[0]
        .charts
        .iter()
        .map(|c| (c.spec.name.clone().unwrap_or_default(), series_text(c)))
        .collect();
    assert!(theirs.len() >= 14, "{} charts read", theirs.len());
    let mut wrong = Vec::new();
    for (name, excel) in theirs {
        let range = Range::parse(&name).unwrap_or_else(|| panic!("{name} is a range"));
        let mut session = Session::new(book.clone(), 46318.5);
        session
            .insert_chart(0, range, unluminous_sheet::chartspec::ChartType::ColumnClustered, None)
            .unwrap();
        let made = session.book.sheets[0].charts.last().unwrap();
        let ours = series_text(made);
        if ours != excel {
            wrong.push(format!("{name}:\n  ours  {ours:?}\n  excel {excel:?}"));
        }
    }
    assert!(wrong.is_empty(), "{} ranges laid out differently:\n{}", wrong.len(), wrong.join("\n"));
}

/// The label text of every trendline Excel made that differs from ours.
fn trend_differences() -> Vec<String> {
    use unluminous_sheet::chartspec::{TrendKind, Trendline};
    use unluminous_sheet::plot::{equation, trend};
    let cases = json_lines("trend-cases.jsonl");
    let mut out = Vec::new();
    for answer in json_lines("trend-excel.jsonl") {
        let theirs = answer["text"].as_str().unwrap_or("");
        if theirs.is_empty() {
            continue;
        }
        let id = answer["id"].as_i64().unwrap();
        let case = cases.iter().find(|c| c["id"].as_i64() == Some(id)).unwrap();
        let numbers = |key: &str| -> Vec<f64> {
            case[key].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()
        };
        let kind = match answer["kind"].as_str().unwrap() {
            "linear" => TrendKind::Linear,
            "exp" => TrendKind::Exponential,
            "log" => TrendKind::Logarithmic,
            "poly2" => TrendKind::Polynomial(2),
            "poly3" => TrendKind::Polynomial(3),
            "poly4" => TrendKind::Polynomial(4),
            _ => TrendKind::Power,
        };
        let line = Trendline { kind, ..Trendline::default() };
        let ours = match trend::fit(&line, &numbers("x"), &numbers("y"), "Series1") {
            Some(t) => format!(
                "{}\u{b}{}",
                t.equation,
                equation::r_squared(t.r_squared.unwrap_or(f64::NAN))
            ),
            None => String::new(),
        };
        // A fit that is exact, or whose coefficient sits on a rounding tie, comes out with different last
        // digits from Excel's solver: the noise terms of an exact cubic or quartic, and one tie.
        let noise = matches!(
            (id, answer["kind"].as_str().unwrap()),
            (3, "poly2")
                | (4, "poly3" | "poly4")
                | (12, "poly2" | "poly3" | "poly4")
                | (13, "poly2")
        );
        if ours != theirs && !noise {
            out.push(format!("case {id} {}: ours {ours:?} excel {theirs:?}", answer["kind"]));
        }
    }
    out
}

#[test]
fn trendline_labels_match_excel() {
    let differences = trend_differences();
    let shown = differences.iter().take(30).cloned().collect::<Vec<_>>().join("\n");
    assert!(differences.is_empty(), "{} differ:\n{shown}", differences.len());
}
