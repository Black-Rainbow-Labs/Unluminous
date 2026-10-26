//! The one bar over the pages (`task-2366`).
//!
//! From the left: the paragraph style as a select chip; `Text options`, a round chip opening a flyout with
//! bold, italic, underline, strikethrough, the two scripts, the lists and the alignment; the view as a
//! select chip; and at the right three round chips, `Format` and `Insert`, which open the panel, and
//! `Find a command`, which opens the command search. Where the caret is and how long the document is are in
//! the window's status bar (`services::word::status`).
//!
//! The text options sit behind one chip because they are a dozen controls a person uses a few at a time,
//! and the title bar already keeps the same controls for Markdown behind the same chip (`text_tools`). The
//! chip carries a dot while the text at the caret is bold, italic or underlined, so the state shows with the
//! flyout shut.

use egui::{Pos2, Rect, Sense, Vec2};
use unluminous_word::props::{Align, Underline, VertAlign};

use super::kit::{self, Mark};
use crate::components::controls::{self, WithHint};
use crate::services::plugin_ui::{Look, Request};
use crate::services::word::{Doc, Level, Mode};
use crate::theme::svg_icon;

/// The bar's height before the tab's zoom.
pub const HEIGHT: f32 = 44.0;
/// How wide the paragraph style chip is, and how wide it may shrink to in a narrow tab.
const STYLE: f32 = 168.0;
const STYLE_NARROW: f32 = 112.0;
/// How wide the view chip is, and how wide it is when it keeps only its chevron.
const VIEW: f32 = 128.0;
const VIEW_NARROW: f32 = 40.0;
/// How wide the text options flyout is.
const FLYOUT: f32 = 312.0;

/// The styles a person uses most, first in the list, each with the size and weight its name is set in.
const FAVOURITES: [(&str, &str, f32, bool); 11] = [
    ("Normal", "Normal", 12.5, false),
    ("Title", "Title", 19.0, false),
    ("Subtitle", "Subtitle", 13.5, false),
    ("Heading1", "Heading 1", 16.5, true),
    ("Heading2", "Heading 2", 15.0, true),
    ("Heading3", "Heading 3", 13.5, true),
    ("Heading4", "Heading 4", 12.5, true),
    ("Quote", "Quote", 12.5, false),
    ("IntenseQuote", "Intense Quote", 12.5, true),
    ("ListParagraph", "List Paragraph", 12.5, false),
    ("NoSpacing", "No Spacing", 12.5, false),
];

/// The three views, in the order the view chip lists them.
pub const VIEWS: [(Mode, &str); 3] =
    [(Mode::Print, "Print Layout"), (Mode::Read, "Read Mode"), (Mode::Outline, "Outline")];

/// The name a view is listed by.
pub fn view_name(mode: Mode) -> &'static str {
    VIEWS.iter().find(|(m, _)| *m == mode).map_or("Print Layout", |(_, name)| name)
}

/// Draw the bar.
pub fn show(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    rect: Rect,
    requests: &mut Vec<Request>,
) {
    let z = kit::zoom(ui.ctx());
    ui.painter().rect_filled(rect, 0.0, look.ground(look.palette.panel));
    crate::components::chrome_kit::groove(ui.painter(), rect.left_bottom(), rect.right_bottom());
    let middle = rect.center().y;
    let pad = 10.0 * z;
    let round = Vec2::splat(kit::ROUND * z);
    // The three round chips at the right first, so the chips at the left know how much room is theirs.
    let right_chips = 3.0 * round.x + 2.0 * 8.0 * z;
    let left_end = rect.right() - pad - right_chips - 12.0 * z;
    let room = left_end - rect.left() - pad;
    let roomy = room >= STYLE * z + round.x + VIEW * z + 48.0 * z;
    let style_width = if roomy { STYLE } else { STYLE_NARROW } * z;
    let view_width = if roomy { VIEW } else { VIEW_NARROW } * z;
    let mut x = rect.left() + pad;
    let chip = |x: f32, width: f32| {
        Rect::from_min_size(
            Pos2::new(x, middle - kit::CHIP * z / 2.0),
            Vec2::new(width, kit::CHIP * z),
        )
    };
    style_chip(ui, doc, chip(x, style_width), requests);
    x += style_width + 8.0 * z;
    let options = Rect::from_min_size(Pos2::new(x, middle - round.y / 2.0), round);
    text_options(ui, doc, options, requests);
    x += round.x + 12.0 * z;
    crate::components::chrome_kit::groove(
        ui.painter(),
        Pos2::new(x, middle - 11.0 * z),
        Pos2::new(x, middle + 11.0 * z),
    );
    x += 12.0 * z;
    view_chip(ui, doc, chip(x, view_width));
    // The three round chips.
    let mut x = rect.right() - pad - right_chips;
    let panel_open = |doc: &Doc, level: Level| doc.view.panel.last() == Some(&level);
    let format_on = !doc.view.panel.is_empty() && !panel_open(doc, Level::Insert);
    let at = |x: f32| Rect::from_min_size(Pos2::new(x, middle - round.y / 2.0), round);
    let format = kit::round_chip(
        ui,
        at(x),
        "Format: text, paragraph, styles, page and more",
        "Format: text, paragraph, styles, page and more",
        Mark::Rux(rux::Icon::Palette),
        format_on,
    );
    if format.clicked() {
        doc.view.panel = if format_on { Vec::new() } else { vec![Level::Root] };
    }
    x += round.x + 8.0 * z;
    let insert_on = panel_open(doc, Level::Insert);
    let insert = kit::round_chip(
        ui,
        at(x),
        "Insert: table, picture, break, link, footnote and more",
        "Insert: table, picture, break, link, footnote and more",
        Mark::Rux(rux::Icon::Plus),
        insert_on,
    );
    if insert.clicked() {
        doc.view.panel = if insert_on { Vec::new() } else { vec![Level::Root, Level::Insert] };
    }
    x += round.x + 8.0 * z;
    let search = kit::round_chip(
        ui,
        at(x),
        "Find a command by name (Ctrl+/)",
        "Find a command by name (Ctrl+/)",
        Mark::Rux(rux::Icon::Search),
        doc.view.palette.is_some(),
    );
    if search.clicked() {
        doc.view.palette = if doc.view.palette.is_some() { None } else { Some((String::new(), 0)) };
    }
}

/// The paragraph style chip and its list.
fn style_chip(ui: &mut egui::Ui, doc: &mut Doc, rect: Rect, requests: &mut Vec<Request>) {
    let z = kit::zoom(ui.ctx());
    let paragraph = doc.session.paragraph_at_caret();
    let resolver = unluminous_word::styles::Resolver::new(&doc.session.document);
    let name = resolver.name_of(&paragraph.style);
    let others: Vec<(String, String)> = resolver
        .listed("paragraph")
        .into_iter()
        .filter(|(id, _)| !FAVOURITES.iter().any(|(f, ..)| f == id))
        .collect();
    let current = paragraph.style.clone();
    let chosen = kit::select_chip(ui, rect, "Paragraph style", &name, 240.0 * z, |ui| {
        let mut chosen = None;
        let tall = FAVOURITES.len() as f32 * 34.0 + others.len() as f32 * 31.0;
        let mut rows = |ui: &mut egui::Ui| {
            for (id, label, size, strong) in FAVOURITES {
                if kit::styled_row(ui, label, size * z, strong, current == id) {
                    chosen = Some(id.to_owned());
                }
            }
            if !others.is_empty() {
                separator(ui);
                for (id, label) in &others {
                    if controls::menu_row(ui, label, "", true, current == *id, 0.0) {
                        chosen = Some(id.clone());
                    }
                }
            }
        };
        let most = (ui.ctx().content_rect().height() * 0.6).max(200.0);
        match tall > most {
            true => {
                egui::ScrollArea::vertical().max_height(most).show(ui, |ui| rows(ui));
            }
            false => rows(ui),
        }
        chosen
    });
    if let Some(id) = chosen {
        if let Err(problem) = doc.session.apply_style(&id) {
            requests.push(Request::Message(problem));
        }
        requests.push(Request::TakeTheKeyboard(true));
    }
}

/// The groove between two groups of rows in a menu card.
fn separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 11.0), Sense::hover());
    crate::components::chrome_kit::groove(
        ui.painter(),
        Pos2::new(rect.left() + 10.0, rect.center().y),
        Pos2::new(rect.right() - 10.0, rect.center().y),
    );
}

/// The view chip and its three views.
fn view_chip(ui: &mut egui::Ui, doc: &mut Doc, rect: Rect) {
    let z = kit::zoom(ui.ctx());
    let now = doc.view.mode;
    let chosen = kit::select_chip(ui, rect, "View mode", view_name(now), 180.0 * z, |ui| {
        let mut chosen = None;
        for (mode, name) in VIEWS {
            if controls::menu_row(ui, name, "", true, now == mode, 0.0) {
                chosen = Some(mode);
            }
        }
        chosen
    });
    if let Some(mode) = chosen {
        set_view(doc, mode);
    }
}

/// Change the view, laying the document out again for it.
pub fn set_view(doc: &mut Doc, mode: Mode) {
    if doc.view.mode != mode {
        doc.view.mode = mode;
        doc.laid = u64::MAX;
        doc.view.reveal = true;
    }
}

/// One option in a row of the text options flyout.
struct TextOption {
    id: &'static str,
    name: &'static str,
    mark: &'static svg_icon::SvgMark,
    on: bool,
}

/// The `Text options` chip and its flyout.
fn text_options(ui: &mut egui::Ui, doc: &mut Doc, rect: Rect, requests: &mut Vec<Request>) {
    let z = kit::zoom(ui.ctx());
    let run = doc.session.look_at_caret();
    let paragraph = doc.session.paragraph_at_caret();
    let list = paragraph
        .numbering
        .and_then(|(num, _)| unluminous_word::builtin::kind_of(&doc.session.document, num));
    let any_on = run.bold || run.italic || run.underline != Underline::None;
    let rows: [(&str, Vec<TextOption>); 4] = [
        (
            "Text",
            vec![
                TextOption { id: "bold", name: "Bold", mark: &svg_icon::BOLD, on: run.bold },
                TextOption {
                    id: "italic",
                    name: "Italic",
                    mark: &svg_icon::ITALIC,
                    on: run.italic,
                },
                TextOption {
                    id: "underline",
                    name: "Underline",
                    mark: &svg_icon::UNDERLINE,
                    on: run.underline != Underline::None,
                },
                TextOption {
                    id: "strikethrough",
                    name: "Strikethrough",
                    mark: &svg_icon::STRIKETHROUGH,
                    on: run.strike,
                },
            ],
        ),
        (
            "Script",
            vec![
                TextOption {
                    id: "superscript",
                    name: "Superscript",
                    mark: &svg_icon::SUPERSCRIPT,
                    on: run.vert == VertAlign::Superscript,
                },
                TextOption {
                    id: "subscript",
                    name: "Subscript",
                    mark: &svg_icon::SUBSCRIPT,
                    on: run.vert == VertAlign::Subscript,
                },
            ],
        ),
        (
            "Lists",
            vec![
                TextOption {
                    id: "bullets",
                    name: "Bulleted list",
                    mark: &svg_icon::LIST,
                    on: list == Some(unluminous_word::builtin::ListKind::Bullet),
                },
                TextOption {
                    id: "numbering",
                    name: "Numbered list",
                    mark: &svg_icon::LIST_ORDERED,
                    on: list == Some(unluminous_word::builtin::ListKind::Number),
                },
                TextOption {
                    id: "indent-less",
                    name: "Decrease indent",
                    mark: &svg_icon::OUTDENT,
                    on: false,
                },
                TextOption {
                    id: "indent-more",
                    name: "Increase indent",
                    mark: &svg_icon::INDENT,
                    on: false,
                },
            ],
        ),
        (
            "Align",
            vec![
                TextOption {
                    id: "align-left",
                    name: "Align left",
                    mark: &svg_icon::ALIGN_LEFT,
                    on: paragraph.align == Align::Left,
                },
                TextOption {
                    id: "align-center",
                    name: "Center",
                    mark: &svg_icon::ALIGN_CENTER,
                    on: paragraph.align == Align::Center,
                },
                TextOption {
                    id: "align-right",
                    name: "Align right",
                    mark: &svg_icon::ALIGN_RIGHT,
                    on: paragraph.align == Align::Right,
                },
                TextOption {
                    id: "justify",
                    name: "Justify",
                    mark: &svg_icon::ALIGN_JUSTIFY,
                    on: paragraph.align == Align::Justify,
                },
            ],
        ),
    ];
    let paint = |ui: &mut egui::Ui, open: bool, hovered: bool| {
        kit::layer(ui, "text-options", rect, |rux| {
            let ink = rux.theme().ink;
            let chat = rux.theme().chat;
            match open {
                true => kit::well_surface(rux, rect, rect.width() / 2.0),
                false => rux::components::chip_disc(rux, rect),
            }
            let tint = match (open, hovered) {
                (true, _) => ink.i900,
                (false, true) => chat.sky,
                (false, false) => ink.i500,
            };
            kit::paint_mark(rux, Mark::Svg(&svg_icon::BOLD), rect.center(), 15.0 * z, tint);
            // A dot at the chip's lower right while the text at the caret is bold, italic or underlined.
            if any_on {
                let dot = rect.center() + Vec2::new(9.5, 9.5) * z;
                rux.painter().circle_filled(dot, 2.6 * z, chat.sky);
            }
        });
    };
    let chosen = controls::flyout_drawn(ui, rect, "Text options", paint, FLYOUT * z, |ui| {
        flyout_rows(ui, &rows)
    });
    if let Some(Some(id)) = chosen {
        super::run_action(doc, id, requests);
        requests.push(Request::TakeTheKeyboard(true));
    }
}

/// The flyout's rows: a name at the left and a carved track of options. Answers the option pressed.
fn flyout_rows(ui: &mut egui::Ui, rows: &[(&str, Vec<TextOption>)]) -> Option<&'static str> {
    let z = kit::zoom(ui.ctx());
    let row = 40.0 * z;
    let option = Vec2::new(34.0 * z, 28.0 * z);
    let label_column = 62.0 * z;
    let width = FLYOUT * z - 16.0;
    let (area, _) =
        ui.allocate_exact_size(Vec2::new(width, row * rows.len() as f32), Sense::hover());
    let ink = kit::ink();
    let mut pressed = None;
    for (index, (label, options)) in rows.iter().enumerate() {
        let middle = area.top() + row * (index as f32 + 0.5);
        let size = 11.5 * z;
        let galley = kit::words(ui, label, size, ink.i400, label_column);
        kit::draw_by_capitals(ui.painter(), area.left() + 8.0 * z, middle, galley, size, ink.i400);
        let track = Rect::from_min_size(
            Pos2::new(area.left() + label_column + 8.0 * z, middle - option.y / 2.0 - 3.0 * z),
            Vec2::new(options.len() as f32 * (option.x + 2.0 * z) + 4.0 * z, option.y + 6.0 * z),
        );
        // Every press is read first, then the track is drawn once in a layer of its own.
        let responses: Vec<(Rect, egui::Response)> = options
            .iter()
            .enumerate()
            .map(|(n, o)| {
                let at = Rect::from_min_size(
                    Pos2::new(
                        track.left() + 3.0 * z + n as f32 * (option.x + 2.0 * z),
                        middle - option.y / 2.0,
                    ),
                    option,
                );
                let response = ui
                    .interact(at, ui.id().with(("word-text-option", o.id)), Sense::click())
                    .with_hint(hint_for(o));
                (at, response)
            })
            .collect();
        kit::layer(ui, ("text-options-row", index), track, |rux| {
            kit::well_surface(rux, track, track.height() / 2.0);
            let chat = rux.theme().chat;
            for (o, (at, response)) in options.iter().zip(&responses) {
                if o.on {
                    kit::chip_surface(rux, *at, at.height() / 2.0, false);
                }
                let tint = match (o.on, response.hovered()) {
                    (true, _) => ink.i900,
                    (false, true) => chat.sky,
                    (false, false) => ink.i500,
                };
                kit::paint_mark(rux, Mark::Svg(o.mark), at.center(), 16.0 * z, tint);
            }
        });
        for (o, (_, response)) in options.iter().zip(responses) {
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let (on, name) = (o.on, o.name);
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, name)
            });
            if response.clicked() {
                pressed = Some(o.id);
            }
        }
    }
    pressed
}

/// What the pointer is told about an option: its name and the chord Word gives it.
fn hint_for(option: &TextOption) -> String {
    let chord = match option.id {
        "bold" => " (Ctrl+B)",
        "italic" => " (Ctrl+I)",
        "underline" => " (Ctrl+U)",
        "superscript" => " (Ctrl+Shift+=)",
        "subscript" => " (Ctrl+=)",
        "align-left" => " (Ctrl+L)",
        "align-center" => " (Ctrl+E)",
        "align-right" => " (Ctrl+R)",
        "justify" => " (Ctrl+J)",
        _ => "",
    };
    format!("{}{chord}", option.name)
}
