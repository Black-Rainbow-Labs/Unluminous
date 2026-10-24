//! The one thin bar over the pages: the paragraph style, bold, italic, underline, the two lists, the
//! alignment, and the two buttons that open the panel, `Aa` for formatting and `+` for inserting. On the
//! right, where the caret is, how long the document is, and the zoom.

use crate::components::controls::WithHint;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use unluminous_word::props::Align;

use crate::services::plugin_ui::{Look, Request};
use crate::services::word::{Doc, Level};

/// A square button drawn by `paint`, lit when `on`. Answers whether it was clicked.
fn button(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    on: bool,
    name: &str,
    paint: impl FnOnce(&egui::Painter, Rect, Color32),
) -> bool {
    let response = ui.interact(rect, ui.id().with(("word-bar", name)), Sense::click());
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, 5.0, look.palette.selected_row);
    } else if response.hovered() {
        painter.rect_filled(rect, 5.0, look.palette.control);
    }
    let ink = if on { look.palette.text_strong } else { look.palette.text_control };
    paint(painter, rect, ink);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    response.with_hint(name).clicked()
}

/// How a letter on a button is set.
#[derive(Clone, Copy, PartialEq)]
enum Set {
    Plain,
    Bold,
    Italic,
    Underlined,
}

/// One or two letters in the middle of a button.
fn letter(painter: &egui::Painter, rect: Rect, ink: Color32, text: &str, size: f32, set: Set) {
    let mut job = egui::text::LayoutJob::default();
    let format = egui::TextFormat {
        font_id: FontId::proportional(size),
        color: ink,
        italics: set == Set::Italic,
        underline: if set == Set::Underlined { Stroke::new(1.2, ink) } else { Stroke::NONE },
        ..Default::default()
    };
    job.append(text, 0.0, format);
    let galley = painter.layout_job(job);
    let at = rect.center() - galley.size() / 2.0;
    // egui's interface face has no bold, so bold is the letter drawn twice, half a point apart.
    if set == Set::Bold {
        painter.galley(at + Vec2::new(0.6, 0.0), galley.clone(), ink);
    }
    painter.galley(at, galley, ink);
}

/// Four lines laid out the way `align` lays out text.
fn alignment_icon(painter: &egui::Painter, rect: Rect, ink: Color32, align: Align) {
    let width = rect.width() * 0.56;
    let left = rect.center().x - width / 2.0;
    for (row, fraction) in [1.0, 0.65, 1.0, 0.5].iter().enumerate() {
        let w = if align == Align::Justify { width } else { width * fraction };
        let x = match align {
            Align::Left | Align::Justify => left,
            Align::Center => rect.center().x - w / 2.0,
            Align::Right => left + width - w,
        };
        let y = rect.top() + rect.height() * (0.3 + 0.13 * row as f32);
        painter.line_segment([Pos2::new(x, y), Pos2::new(x + w, y)], Stroke::new(1.4, ink));
    }
}

/// Three dots and lines, or three numbers and lines.
fn list_icon(painter: &egui::Painter, rect: Rect, ink: Color32, numbered: bool) {
    for row in 0..3 {
        let y = rect.top() + rect.height() * (0.32 + 0.18 * row as f32);
        let x = rect.left() + rect.width() * 0.26;
        if numbered {
            painter.text(
                Pos2::new(x, y),
                Align2::CENTER_CENTER,
                (row + 1).to_string(),
                FontId::proportional(rect.height() * 0.26),
                ink,
            );
        } else {
            painter.circle_filled(Pos2::new(x, y), 1.6, ink);
        }
        painter.line_segment(
            [Pos2::new(x + 5.0, y), Pos2::new(rect.right() - rect.width() * 0.2, y)],
            Stroke::new(1.3, ink),
        );
    }
}

/// Draw the bar.
pub fn show(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    rect: Rect,
    requests: &mut Vec<Request>,
) {
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, look.ground(look.palette.panel));
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, look.palette.divider),
    );
    let scale = look.scale();
    let size = Vec2::splat(28.0 * scale);
    let y = rect.center().y - size.y / 2.0;
    let mut x = rect.left() + 10.0 * scale;
    let mut next = |width: f32| {
        let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, size.y));
        x += width + 4.0 * scale;
        r
    };
    // The paragraph style, as a dropdown.
    let paragraph = doc.session.paragraph_at_caret();
    let style_name =
        unluminous_word::styles::Resolver::new(&doc.session.document).name_of(&paragraph.style);
    let style_rect = next(150.0 * scale);
    let styles = unluminous_word::styles::Resolver::new(&doc.session.document).listed("paragraph");
    let mut chosen_style: Option<String> = None;
    let mut style_ui = ui.new_child(egui::UiBuilder::new().max_rect(style_rect));
    egui::ComboBox::from_id_salt(("word-style", &doc.key))
        .selected_text(&style_name)
        .width(style_rect.width())
        .height(420.0 * scale)
        .show_ui(&mut style_ui, |ui| {
            ui.set_min_width(200.0 * scale);
            let favourites = [
                ("Normal", "Normal"),
                ("Title", "Title"),
                ("Subtitle", "Subtitle"),
                ("Heading1", "Heading 1"),
                ("Heading2", "Heading 2"),
                ("Heading3", "Heading 3"),
                ("Heading4", "Heading 4"),
                ("Quote", "Quote"),
                ("IntenseQuote", "Intense Quote"),
                ("ListParagraph", "List Paragraph"),
                ("NoSpacing", "No Spacing"),
            ];
            for (id, name) in favourites {
                if ui.selectable_label(paragraph.style == id, name).clicked() {
                    chosen_style = Some(id.to_owned());
                }
            }
            let others: Vec<&(String, String)> =
                styles.iter().filter(|(id, _)| !favourites.iter().any(|(f, _)| f == id)).collect();
            if !others.is_empty() {
                ui.separator();
                for (id, name) in others {
                    if ui.selectable_label(paragraph.style == *id, name).clicked() {
                        chosen_style = Some(id.clone());
                    }
                }
            }
        });
    if let Some(id) = chosen_style {
        if let Err(problem) = doc.session.apply_style(&id) {
            requests.push(Request::Message(problem));
        }
        requests.push(Request::TakeTheKeyboard(true));
    }
    let run = doc.session.look_at_caret();
    let toggles = [
        ("bold", "Bold (Ctrl+B)", run.bold, "B", Set::Bold),
        ("italic", "Italic (Ctrl+I)", run.italic, "I", Set::Italic),
        (
            "underline",
            "Underline (Ctrl+U)",
            run.underline != unluminous_word::props::Underline::None,
            "U",
            Set::Underlined,
        ),
    ];
    for (id, name, on, text, set) in toggles {
        let r = next(size.x);
        if button(ui, look, r, on, name, |p, r, ink| letter(p, r, ink, text, 15.0 * scale, set)) {
            super::run_action(doc, id, requests);
            requests.push(Request::TakeTheKeyboard(true));
        }
    }
    let resolver = unluminous_word::styles::Resolver::new(&doc.session.document);
    let list = paragraph
        .numbering
        .and_then(|(num, _)| unluminous_word::builtin::kind_of(&doc.session.document, num));
    drop(resolver);
    let gap = next(4.0 * scale);
    let _ = gap;
    for (id, name, numbered) in
        [("bullets", "Bulleted list", false), ("numbering", "Numbered list", true)]
    {
        let on = list
            == Some(if numbered {
                unluminous_word::builtin::ListKind::Number
            } else {
                unluminous_word::builtin::ListKind::Bullet
            });
        let r = next(size.x);
        if button(ui, look, r, on, name, |p, r, ink| list_icon(p, r, ink, numbered)) {
            super::run_action(doc, id, requests);
            requests.push(Request::TakeTheKeyboard(true));
        }
    }
    let _ = next(4.0 * scale);
    for (id, name, align) in [
        ("align-left", "Align left (Ctrl+L)", Align::Left),
        ("align-center", "Center (Ctrl+E)", Align::Center),
        ("align-right", "Align right (Ctrl+R)", Align::Right),
        ("justify", "Justify (Ctrl+J)", Align::Justify),
    ] {
        let r = next(size.x);
        if button(ui, look, r, paragraph.align == align, name, |p, r, ink| {
            alignment_icon(p, r, ink, align)
        }) {
            super::run_action(doc, id, requests);
            requests.push(Request::TakeTheKeyboard(true));
        }
    }
    let _ = next(8.0 * scale);
    // The two ways into the panel.
    let panel_open = |doc: &Doc, level: Level| doc.view.panel.last() == Some(&level);
    let format_rect = next(size.x + 6.0 * scale);
    let format_on = !doc.view.panel.is_empty() && !panel_open(doc, Level::Insert);
    if button(
        ui,
        look,
        format_rect,
        format_on,
        "Format: text, paragraph, styles, page and more",
        |p, r, ink| letter(p, r, ink, "Aa", 14.0 * scale, Set::Plain),
    ) {
        doc.view.panel = if format_on { Vec::new() } else { vec![Level::Root] };
    }
    let insert_rect = next(size.x);
    let insert_on = panel_open(doc, Level::Insert);
    if button(
        ui,
        look,
        insert_rect,
        insert_on,
        "Insert: table, picture, break, link, footnote and more",
        |p, r, ink| letter(p, r, ink, "+", 18.0 * scale, Set::Plain),
    ) {
        doc.view.panel = if insert_on { Vec::new() } else { vec![Level::Root, Level::Insert] };
    }
    let search_rect = next(size.x);
    if button(
        ui,
        look,
        search_rect,
        doc.view.palette.is_some(),
        "Find a command by name (Ctrl+/)",
        |p, r, ink| {
            p.circle_stroke(
                r.center() - Vec2::new(2.0, 2.0),
                r.height() * 0.2,
                Stroke::new(1.4, ink),
            );
            p.line_segment(
                [r.center() + Vec2::new(3.0, 3.0), r.center() + Vec2::new(7.0, 7.0)],
                Stroke::new(1.6, ink),
            );
        },
    ) {
        doc.view.palette = if doc.view.palette.is_some() { None } else { Some((String::new(), 0)) };
    }
    // The right side: tracking, where the caret is, how long it is, the zoom.
    let stats = doc.session.stats(Some(&doc.pages));
    let page = doc.pages.find(&doc.session.selection.head).map_or(1, |(index, _)| index + 1);
    let mut status = format!(
        "Page {page} of {}   {} words   {:.0}%",
        stats.pages.max(1),
        stats.words,
        doc.view.zoom * 100.0
    );
    if doc.session.tracking {
        status = format!("Tracking changes   {status}");
    }
    let errors = if doc.view.spelling {
        crate::services::word::spelling::misspellings(&doc.session.document).len()
    } else {
        0
    };
    match errors {
        0 => {}
        1 => status = format!("1 spelling error   {status}"),
        n => status = format!("{n} spelling errors   {status}"),
    }
    let right = Pos2::new(rect.right() - 12.0 * scale, rect.center().y);
    if x < right.x - 260.0 * scale {
        let drawn = ui.painter().text(
            right,
            Align2::RIGHT_CENTER,
            status,
            FontId::proportional(look.font_size * 0.92),
            look.palette.text_dim,
        );
        // A click on the status line when it counts spelling errors goes to the next one, as Word's
        // proofing icon does.
        if errors > 0 {
            let response =
                ui.interact(drawn, ui.id().with("word-status-spelling"), egui::Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    "Next spelling error (F7)",
                )
            });
            if response.with_hint("Next spelling error (F7)").clicked() {
                super::run_action(doc, "spelling", requests);
                requests.push(Request::TakeTheKeyboard(true));
            }
        }
    }
}
