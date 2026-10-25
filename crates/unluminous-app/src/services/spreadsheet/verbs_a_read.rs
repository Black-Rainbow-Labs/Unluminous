//! The read back verbs for this lane's areas: `validation`, `comments`, `protection`, `styles`, `views`
//! and `calc`. (`rules` and `outline` are read by their own verbs.) Each answers JSON, so an agent can check
//! what a change did.

use serde_json::json;
use unluminous_sheet::coord::Range;

use super::verbs_a::Out;
use super::Spreadsheet;
use crate::services::plugin_ui::Answer;

/// Run a read back verb.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "validation" => validation(sheet, args),
        "comments" => sheet
            .current_book()
            .map(|b| super::verbs_a_review::comments_answer(b, args.first().map(String::as_str))),
        "protection" => sheet
            .current_book()
            .map(|b| Answer::said("Protection").with(super::verbs_a_whatif::protection_json(b))),
        "styles" => sheet.current_book().map(|b| super::verbs_a_whatif::styles_answer(b)),
        "views" => views(sheet),
        "calc" => calc(sheet),
        _ => return None,
    })
}

/// Every data validation rule, or those that cover a range.
fn validation(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let within = args.first().and_then(|w| Range::parse(&w.replace('$', "")));
    let s = &book.session.book.sheets[book.view.sheet];
    let list: Vec<_> = s
        .validations
        .iter()
        .filter(|v| within.is_none_or(|w| v.ranges.iter().any(|r| r.intersects(&w))))
        .map(|v| json!({
            "ranges": v.ranges.iter().map(Range::a1).collect::<Vec<_>>(), "kind": v.kind, "operator": v.operator,
            "formula1": v.formula1, "formula2": v.formula2, "allowBlank": v.allow_blank, "dropdown": v.show_dropdown,
            "inputTitle": v.prompt_title, "inputMessage": v.prompt,
            "errorStyle": v.error_style, "errorTitle": v.error_title, "errorMessage": v.error,
        }))
        .collect();
    Ok(Answer::said(format!("{} validations", list.len())).with(json!(list)))
}

/// Panes, zoom and selection of the sheet showing.
fn views(sheet: &mut Spreadsheet) -> Out {
    let book = sheet.current_book()?;
    let s = &book.session.book.sheets[book.view.sheet];
    let v = &s.view;
    let selection: Vec<String> = std::iter::once(book.view.range())
        .chain(book.view.extra.iter().copied())
        .map(|r| r.a1())
        .collect();
    Ok(Answer::said(v.kind.name()).with(json!({
        "view": v.kind.name(), "zoom": (book.view.zoom * 100.0).round(), "sheetZoom": s.zoom,
        "frozen": s.frozen.map(|(r, c)| json!({ "rows": r, "columns": c })),
        "split": v.split.as_ref().map(|p| json!({ "x": p.x, "y": p.y })),
        "gridlines": s.show_grid, "headings": v.show_headings, "formulas": v.show_formulas,
        "zeros": v.show_zeros, "rightToLeft": v.right_to_left,
        "active": book.view.cursor.a1(), "selection": selection, "sheet": s.name,
    })))
}

/// Calculation mode and iteration settings.
fn calc(sheet: &mut Spreadsheet) -> Out {
    let book = sheet.current_book()?;
    let c = &book.session.book.calc;
    Ok(Answer::said(if c.automatic { "automatic" } else { "manual" }).with(json!({
        "mode": if c.automatic { "automatic" } else { "manual" },
        "iterate": c.iterate, "iterations": c.iterate_count, "delta": c.iterate_delta, "fullPrecision": c.full_precision,
    })))
}
