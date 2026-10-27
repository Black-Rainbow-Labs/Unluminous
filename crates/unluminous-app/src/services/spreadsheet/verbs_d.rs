//! The verbs `task-2382` added: `where`, which says where a cell was drawn so a pointer can be aimed at it,
//! and the Excel conveniences that have a verb of their own. [`super::verbs_c::run`] hands any verb it does
//! not know to [`run`] here.

use serde_json::json;

use super::verbs_a::{cell_or_active, Out};
use super::Spreadsheet;
use crate::services::plugin_ui::Answer;

/// Run a verb this module knows, or answer `None` for one it does not.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    match verb {
        "where" => Some(where_is(sheet, args)),
        _ => None,
    }
}

/// `where [cell]`: the cell's rectangle as the grid drew it on the last frame, in egui's points, and its
/// middle. Multiply by the interface zoom for the window's own points, which `input click` takes.
fn where_is(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let at = cell_or_active(book, args.first().map(String::as_str))?;
    let screen = book.view.on_screen.ok_or("The workbook has not been drawn yet.")?;
    let axes = book.view.axes.as_ref().ok_or("The workbook has not been drawn yet.")?;
    let rect = screen.cell(axes, at);
    let shown = screen.cells.intersects(rect);
    Ok(Answer::said(format!(
        "{} is at {:.0}, {:.0}{}",
        at.a1(),
        rect.center().x,
        rect.center().y,
        if shown { "" } else { ", scrolled out of view" }
    ))
    .with(json!({
        "cell": at.a1(),
        "x": rect.center().x,
        "y": rect.center().y,
        "left": rect.left(),
        "top": rect.top(),
        "width": rect.width(),
        "height": rect.height(),
        "shown": shown,
    })))
}
