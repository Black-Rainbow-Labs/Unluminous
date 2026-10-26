//! The drill down panel: one column at the right of the pages with a back chip, the level's name under
//! its path, a close chip, and the controls of one level. The root level lists the others as raised rows,
//! each with its mark and a line saying what is in it, so a setting is two presses from the page and never
//! more than three.
//!
//! `task-2366` redrew it in the design system: the head's marks are drawn (a close button drawn as the
//! character `✕` was an empty box in a face without it), a group of controls is a section with a small
//! spaced heading, a command is a pill, an on and off setting a switch, a colour a circle, and the egui
//! widgets the deep levels still use (a number to drag, a list, a field) are set in the design's style by
//! `kit::style_the_panel`. Every size is the design's times the tab's zoom.

use super::kit::{self, Mark};
use crate::components::controls::WithHint;
use crate::theme::svg_icon;
use egui::{Pos2, Rect, RichText, Sense, Vec2};
use unluminous_word::props::{self, LineRule, Rgb};

use crate::services::plugin_ui::{Look, Request};
use crate::services::word::{Doc, Level};

/// Word's own font colours, the top row of its colour picker.
const COLOURS: &[(&str, &str)] = &[
    ("Automatic", "auto"),
    ("Black", "000000"),
    ("Dark blue", "1F3864"),
    ("Blue", "2F5597"),
    ("Red", "C00000"),
    ("Orange", "ED7D31"),
    ("Gold", "BF8F00"),
    ("Green", "548235"),
    ("Purple", "7030A0"),
    ("Grey", "7F7F7F"),
];

/// How tall the panel's head is, before the tab's zoom.
const HEAD: f32 = 58.0;

/// Draw the panel.
pub fn show(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    rect: Rect,
    requests: &mut Vec<Request>,
) {
    let z = kit::zoom(ui.ctx());
    ui.painter().rect_filled(rect, 0.0, look.ground(look.palette.panel));
    crate::components::chrome_kit::groove(ui.painter(), rect.left_top(), rect.left_bottom());
    let level = *doc.view.panel.last().unwrap_or(&Level::Root);
    let head = Rect::from_min_size(rect.min, Vec2::new(rect.width(), HEAD * z));
    show_head(ui, doc, head, level);
    let body = Rect::from_min_max(
        Pos2::new(rect.left() + 14.0 * z, head.bottom()),
        Pos2::new(rect.right() - 12.0 * z, rect.bottom()),
    );
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(body));
    inner.set_clip_rect(body.intersect(ui.clip_rect()));
    kit::style_the_panel(&mut inner);
    egui::ScrollArea::vertical()
        .id_salt(("word-panel", level as u8))
        .auto_shrink([false, false])
        .show(&mut inner, |ui| {
            ui.set_width(body.width() - 10.0 * z);
            match level {
                Level::Root => root(ui, doc),
                Level::Text => text(ui, look, doc, requests),
                Level::Paragraph => paragraph(ui, look, doc, requests),
                Level::Styles => styles(ui, look, doc, requests),
                Level::Lists => lists(ui, doc, requests),
                Level::Insert => insert(ui, doc, requests),
                Level::Page => page(ui, doc, requests),
                Level::References => references(ui, doc, requests),
                Level::Review => review(ui, look, doc, requests),
                Level::Document => document(ui, doc, requests),
                Level::Proofing => proofing(ui, doc),
                Level::Contents => contents(ui, doc, requests),
                Level::TableProperties => table_properties(ui, doc, requests),
                Level::Picture => picture(ui, doc, requests),
                Level::Print => print(ui, doc, requests),
                Level::PageSetup => super::panel_setup::page_setup(ui, doc, requests),
                Level::ModifyStyle => super::panel_setup::modify_style(ui, look, doc, requests),
                Level::Symbol => super::panel_setup::symbols(ui, look, doc, requests),
                Level::NoteOptions => super::panel_setup::note_options(ui, doc, requests),
            }
            ui.add_space(16.0 * z);
        });
}

/// The head: a back chip when the level is deeper than the root, the level's name under its path, and a
/// close chip.
fn show_head(ui: &mut egui::Ui, doc: &mut Doc, head: Rect, level: Level) {
    let z = kit::zoom(ui.ctx());
    let ink = kit::ink();
    let chip = 28.0 * z;
    let middle = head.center().y;
    let mut left = head.left() + 14.0 * z;
    if doc.view.panel.len() > 1 {
        let back = Rect::from_min_size(Pos2::new(left, middle - chip / 2.0), Vec2::splat(chip));
        let back_mark = Mark::Rux(rux::Icon::ChevLeft);
        if kit::round_chip(ui, back, "Back", "Back", back_mark, false).clicked() {
            doc.view.panel.pop();
        }
        left = back.right() + 10.0 * z;
    }
    let close = Rect::from_min_size(
        Pos2::new(head.right() - 14.0 * z - chip, middle - chip / 2.0),
        Vec2::splat(chip),
    );
    let cross = Mark::Rux(rux::Icon::Cross);
    if kit::round_chip(ui, close, "Close the panel", "Close the panel", cross, false).clicked() {
        doc.view.panel.clear();
    }
    let room = close.left() - 10.0 * z - left;
    let path: Vec<&str> = doc.view.panel.iter().rev().skip(1).rev().map(|l| l.name()).collect();
    let title_size = 15.0 * z;
    let title = kit::words(ui, level.name(), title_size, ink.i900, room);
    match path.is_empty() {
        true => kit::draw_by_capitals(ui.painter(), left, middle, title, title_size, ink.i900),
        false => {
            let crumb_size = 10.5 * z;
            let crumbs = kit::words(ui, &path.join("  /  "), crumb_size, ink.i400, room);
            kit::draw_by_capitals(
                ui.painter(),
                left,
                middle - 9.0 * z,
                crumbs,
                crumb_size,
                ink.i400,
            );
            kit::draw_by_capitals(
                ui.painter(),
                left,
                middle + 7.0 * z,
                title,
                title_size,
                ink.i900,
            );
        }
    }
    // The interface face may have no semibold, so the title is drawn a second time half a point over.
    let named = level.name().to_owned();
    let response = ui.interact(
        Rect::from_min_max(Pos2::new(left, head.top()), Pos2::new(close.left(), head.bottom())),
        ui.id().with("word-panel-title"),
        Sense::hover(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &named));
}

/// The mark each level of the root list carries.
fn mark_of(level: Level) -> Mark {
    match level {
        Level::Text => Mark::Svg(&svg_icon::TYPE),
        Level::Paragraph => Mark::Svg(&svg_icon::PILCROW),
        Level::Styles => Mark::Svg(&svg_icon::HEADING),
        Level::Lists => Mark::Svg(&svg_icon::LIST),
        Level::Insert => Mark::Rux(rux::Icon::Plus),
        Level::Page => Mark::Rux(rux::Icon::Document),
        Level::References => Mark::Svg(&svg_icon::BOOK_OPEN),
        Level::Review => Mark::Rux(rux::Icon::Chat),
        _ => Mark::Rux(rux::Icon::Settings),
    }
}

/// The root level: one raised row a level, its mark in a carved disc, its name over what it holds, and a
/// chevron. The whole row is a button.
fn root(ui: &mut egui::Ui, doc: &mut Doc) {
    let z = kit::zoom(ui.ctx());
    ui.add_space(2.0 * z);
    for level in Level::ALL {
        if level_row(ui, level.name(), level.summary(), mark_of(*level)) {
            doc.view.panel.push(*level);
        }
        ui.add_space(2.0 * z);
    }
}

/// A raised row that opens a level: a mark in a carved disc, a name, a line under it, and a chevron.
/// Answers whether it was pressed.
fn level_row(ui: &mut egui::Ui, name: &str, summary: &str, mark: Mark) -> bool {
    let z = kit::zoom(ui.ctx());
    let height = if summary.is_empty() { 44.0 } else { 58.0 } * z;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let rect = rect.shrink2(Vec2::new(2.0 * z, 2.0 * z));
    let hovered = response.hovered();
    let held = response.is_pointer_button_down_on();
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let ink = kit::ink();
    let disc = Rect::from_center_size(
        Pos2::new(rect.left() + 12.0 * z + 16.0 * z, rect.center().y),
        Vec2::splat(32.0 * z),
    );
    kit::layer(ui, ("level-row", name), rect, |rux| {
        match held {
            true => kit::well_surface(rux, rect, 16.0 * z),
            false => kit::card_surface(rux, rect, 16.0 * z),
        }
        kit::well_surface(rux, disc, disc.width() / 2.0);
        let chat = rux.theme().chat;
        let tint = if hovered { chat.sky } else { ink.i700 };
        kit::paint_mark(rux, mark, disc.center(), 16.0 * z, tint);
        let chevron = Pos2::new(rect.right() - 16.0 * z, rect.center().y);
        rux.mark(
            rux::Mark::new(rux::Icon::ChevRight, 11.0 * z),
            chevron,
            if hovered { chat.sky } else { ink.i400 },
        );
    });
    let left = disc.right() + 12.0 * z;
    let room = rect.right() - 34.0 * z - left;
    let name_size = 13.0 * z;
    let title = kit::words(ui, name, name_size, ink.i900, room);
    match summary.is_empty() {
        true => {
            kit::draw_by_capitals(ui.painter(), left, rect.center().y, title, name_size, ink.i900)
        }
        false => {
            let small = 11.0 * z;
            let line = kit::words(ui, summary, small, ink.i400, room);
            let middle = rect.center().y;
            kit::draw_by_capitals(ui.painter(), left, middle - 8.0 * z, title, name_size, ink.i900);
            kit::draw_by_capitals(ui.painter(), left, middle + 9.0 * z, line, small, ink.i400);
        }
    }
    let named = name.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &named));
    response.with_hint(if summary.is_empty() { name } else { summary }).clicked()
}

/// A section heading: small spaced capitals in the quiet ink.
pub(super) fn heading(ui: &mut egui::Ui, text: &str) {
    kit::section(ui, text);
}

/// A row of pills that each run an action from the shared list. A pill whose action cannot run here is
/// drawn quiet and takes no press, so nothing in the panel answers with an error (`task-2366`).
pub(super) fn actions_row(
    ui: &mut egui::Ui,
    doc: &mut Doc,
    requests: &mut Vec<Request>,
    ids: &[&str],
) {
    ui.horizontal_wrapped(|ui| {
        for id in ids {
            let Some(action) = crate::services::word::actions::find(id) else { continue };
            let enabled = crate::services::word::actions::applies(id, doc);
            if kit::pill(ui, action.label, false, enabled).clicked() {
                super::run_action(doc, id, requests);
            }
        }
    });
}

/// Colours as swatches, each named. Answers the index of the one pressed.
fn colour_swatches(ui: &mut egui::Ui, colours: &[(&str, egui::Color32)]) -> Option<usize> {
    let named: Vec<(String, egui::Color32)> =
        colours.iter().map(|(name, colour)| ((*name).to_owned(), *colour)).collect();
    kit::swatches(ui, &named, None)
}

/// A colour from six hex digits.
fn hex_colour(hex: &str) -> egui::Color32 {
    Rgb::parse(hex)
        .map_or(crate::theme::color::word_ink(), |c| egui::Color32::from_rgb(c.0, c.1, c.2))
}

fn text(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, requests: &mut Vec<Request>) {
    let run = doc.session.look_at_caret();
    heading(ui, "Font");
    let mut family = run.font.to_string();
    egui::ComboBox::from_id_salt("word-font")
        .selected_text(&family)
        .width(ui.available_width() - 70.0)
        .show_ui(ui, |ui| {
            for name in look.renderer.families() {
                ui.selectable_value(&mut family, name.clone(), name);
            }
        });
    if family != run.font.as_ref() {
        doc.session.format(move |rpr| {
            rpr.put(
                unluminous_word::xml::El::new("w:rFonts")
                    .with("w:ascii", family.clone())
                    .with("w:hAnsi", family.clone())
                    .with("w:cs", family.clone())
                    .with("w:eastAsia", family.clone()),
            );
        });
    }
    ui.horizontal(|ui| {
        let mut size = run.size;
        ui.label("Size");
        if ui
            .add(egui::DragValue::new(&mut size).range(1.0..=1638.0).speed(0.5).suffix(" pt"))
            .changed()
        {
            let half = ((size * 2.0).round() as i64).to_string();
            doc.session.format(move |rpr| props::set_val(rpr, "w:sz", Some(&half)));
        }
        for id in ["shrink-font", "grow-font"] {
            if kit::button(ui, if id == "grow-font" { "Larger" } else { "Smaller" }).clicked() {
                super::run_action(doc, id, requests);
            }
        }
    });
    heading(ui, "Style");
    actions_row(
        ui,
        doc,
        requests,
        &["bold", "italic", "underline", "strikethrough", "double-strikethrough"],
    );
    ui.horizontal(|ui| {
        ui.label("Underline");
        let names = ["none", "single", "double", "thick", "dotted", "dash", "wave", "words"];
        let mut chosen = run.underline.name().to_owned();
        egui::ComboBox::from_id_salt("word-underline").selected_text(&chosen).show_ui(ui, |ui| {
            for name in names {
                ui.selectable_value(&mut chosen, name.to_owned(), name);
            }
        });
        if chosen != run.underline.name() {
            doc.session.format(move |rpr| {
                props::set_val(rpr, "w:u", (chosen != "none").then_some(chosen.as_str()))
            });
        }
    });
    heading(ui, "Effects");
    actions_row(
        ui,
        doc,
        requests,
        &["superscript", "subscript", "small-caps", "all-caps", "hidden"],
    );
    heading(ui, "Colour");
    let colours: Vec<(&str, egui::Color32)> =
        COLOURS.iter().map(|(name, hex)| (*name, hex_colour(hex))).collect();
    if let Some(index) = colour_swatches(ui, &colours) {
        let hex = COLOURS[index].1.to_string();
        doc.session.format(move |rpr| props::set_val(rpr, "w:color", Some(&hex)));
    }
    ui.horizontal(|ui| {
        let mut colour = run
            .color
            .map_or(crate::theme::color::word_ink(), |c| egui::Color32::from_rgb(c.0, c.1, c.2));
        ui.label("More colours");
        if ui.color_edit_button_srgba(&mut colour).changed() {
            let hex = format!("{:02X}{:02X}{:02X}", colour.r(), colour.g(), colour.b());
            doc.session.format(move |rpr| props::set_val(rpr, "w:color", Some(&hex)));
        }
        if kit::button(ui, "Automatic").clicked() {
            doc.session.format(|rpr| props::set_val(rpr, "w:color", Some("auto")));
        }
    });
    kit::label(ui, "Theme colours");
    let themes =
        ["text1", "text2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6"];
    let theme_rgb: Vec<Option<Rgb>> = themes
        .iter()
        .map(|t| unluminous_word::styles::Resolver::new(&doc.session.document).theme.colour(t))
        .collect();
    let theme_colours: Vec<(&str, egui::Color32)> = themes
        .iter()
        .zip(&theme_rgb)
        .map(|(t, rgb)| {
            let colour = rgb.map_or(crate::theme::color::word_ink(), |c| {
                egui::Color32::from_rgb(c.0, c.1, c.2)
            });
            (*t, colour)
        })
        .collect();
    let pressed = colour_swatches(ui, &theme_colours);
    {
        for (index, theme) in themes.iter().enumerate() {
            let rgb = theme_rgb[index];
            if pressed == Some(index) {
                let hex = rgb.map_or_else(
                    || "000000".to_owned(),
                    |c| format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2),
                );
                let theme = theme.to_string();
                doc.session.format(move |rpr| {
                    rpr.put(
                        unluminous_word::xml::El::new("w:color")
                            .with("w:val", hex.clone())
                            .with("w:themeColor", theme.clone()),
                    );
                });
            }
        }
    }
    heading(ui, "Highlight");
    let highlights: Vec<(&str, egui::Color32)> = props::HIGHLIGHTS
        .iter()
        .map(|(name, rgb)| (*name, egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2)))
        .collect();
    if let Some(index) = colour_swatches(ui, &highlights) {
        let name = highlights[index].0.to_string();
        doc.session.format(move |rpr| props::set_val(rpr, "w:highlight", Some(&name)));
    }
    if kit::button(ui, "No highlight").clicked() {
        doc.session.format(|rpr| props::set_val(rpr, "w:highlight", None));
    }
    heading(ui, "Character spacing");
    ui.horizontal(|ui| {
        let mut spacing = run.spacing;
        if ui
            .add(egui::DragValue::new(&mut spacing).range(-20.0..=40.0).speed(0.1).suffix(" pt"))
            .changed()
        {
            let twips = ((spacing * 20.0).round() as i64).to_string();
            doc.session.format(move |rpr| props::set_val(rpr, "w:spacing", Some(&twips)));
        }
    });
    ui.horizontal(|ui| {
        let mut scale = run.scale * 100.0;
        ui.label("Scale");
        if ui
            .add(egui::DragValue::new(&mut scale).range(1.0..=600.0).speed(1.0).suffix("%"))
            .changed()
        {
            let value = (scale.round() as i64).to_string();
            doc.session.format(move |rpr| {
                props::set_val(rpr, "w:w", (value != "100").then_some(value.as_str()))
            });
        }
        let mut position = run.position;
        ui.label("Raise");
        if ui
            .add(
                egui::DragValue::new(&mut position)
                    .range(-1584.0..=1584.0)
                    .speed(0.5)
                    .suffix(" pt"),
            )
            .changed()
        {
            let half = ((position * 2.0).round() as i64).to_string();
            doc.session.format(move |rpr| {
                props::set_val(rpr, "w:position", (half != "0").then_some(half.as_str()))
            });
        }
    });
    ui.horizontal(|ui| {
        let mut kern = run.kern.is_some();
        if kit::check(ui, &mut kern, "Kerning").changed() {
            doc.session.format(move |rpr| props::set_val(rpr, "w:kern", kern.then_some("2")));
        }
        let ligatures = doc.session.look_at_caret_rpr_has("w14:ligatures");
        let mut on = ligatures;
        if kit::check(ui, &mut on, "Ligatures").changed() {
            doc.session.format(move |rpr| {
                if on {
                    rpr.put(
                        unluminous_word::xml::El::new("w14:ligatures")
                            .with("w14:val", "standardContextual"),
                    );
                } else {
                    rpr.remove_children("w14:ligatures");
                }
            });
        }
    });
    heading(ui, "Text effects");
    ui.horizontal_wrapped(|ui| {
        for (label, element, on) in [
            ("Outline", "w:outline", run.outline),
            ("Shadow", "w:shadow", run.shadow),
            ("Emboss", "w:emboss", run.emboss),
            ("Engrave", "w:imprint", run.imprint),
        ] {
            if kit::toggle(ui, on, label).clicked() {
                doc.session
                    .format(move |rpr| props::set_on_off(rpr, element, (!on).then_some(true)));
            }
        }
    });
    heading(ui, "Case");
    actions_row(
        ui,
        doc,
        requests,
        &["sentence-case", "lowercase", "uppercase", "title-case", "toggle-case"],
    );
    heading(ui, "Formatting");
    actions_row(ui, doc, requests, &["copy-formatting", "paste-formatting", "clear-formatting"]);
}

/// A number of points that changes a paragraph property as it is dragged.
fn points_row(
    ui: &mut egui::Ui,
    label: &str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
) -> Option<f32> {
    let mut changed = None;
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut v = value;
            if ui.add(egui::DragValue::new(&mut v).range(range).speed(0.5).suffix(" pt")).changed()
            {
                changed = Some(v);
            }
        });
    });
    changed
}

fn paragraph(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, requests: &mut Vec<Request>) {
    let _ = look;
    let p = doc.session.paragraph_at_caret();
    heading(ui, "Alignment");
    actions_row(ui, doc, requests, &["align-left", "align-center", "align-right", "justify"]);
    heading(ui, "Indentation");
    let twips = |v: f32| ((v * 20.0).round() as i64).to_string();
    if let Some(v) = points_row(ui, "Left", p.indent_left, -720.0..=1440.0) {
        let t = twips(v);
        doc.session.format_paragraphs(move |ppr| {
            ppr.child_or_insert("w:ind").set_attr("w:left", t.clone())
        });
    }
    if let Some(v) = points_row(ui, "Right", p.indent_right, -720.0..=1440.0) {
        let t = twips(v);
        doc.session.format_paragraphs(move |ppr| {
            ppr.child_or_insert("w:ind").set_attr("w:right", t.clone())
        });
    }
    if let Some(v) = points_row(ui, "First line (negative hangs)", p.indent_first, -720.0..=720.0) {
        let t = twips(v.abs());
        let hanging = v < 0.0;
        doc.session.format_paragraphs(move |ppr| {
            let ind = ppr.child_or_insert("w:ind");
            ind.remove_attr("w:firstLine");
            ind.remove_attr("w:hanging");
            ind.set_attr(if hanging { "w:hanging" } else { "w:firstLine" }, t.clone());
        });
    }
    heading(ui, "Spacing");
    if let Some(v) = points_row(ui, "Before", p.space_before, 0.0..=1584.0) {
        let t = twips(v);
        doc.session.format_paragraphs(move |ppr| {
            ppr.child_or_insert("w:spacing").set_attr("w:before", t.clone())
        });
    }
    if let Some(v) = points_row(ui, "After", p.space_after, 0.0..=1584.0) {
        let t = twips(v);
        doc.session.format_paragraphs(move |ppr| {
            ppr.child_or_insert("w:spacing").set_attr("w:after", t.clone())
        });
    }
    heading(ui, "Line spacing");
    actions_row(ui, doc, requests, &["line-single", "line-115", "line-15", "line-double"]);
    let (mut kind, mut amount) = match p.line {
        LineRule::Auto(m) => ("Multiple", m),
        LineRule::AtLeast(v) => ("At least", v),
        LineRule::Exact(v) => ("Exactly", v),
    };
    let before = (kind, amount);
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("word-line-rule").selected_text(kind).show_ui(ui, |ui| {
            for name in ["Multiple", "At least", "Exactly"] {
                ui.selectable_value(&mut kind, name, name);
            }
        });
        let suffix = if kind == "Multiple" { "×" } else { " pt" };
        ui.add(egui::DragValue::new(&mut amount).range(0.06..=132.0).speed(0.05).suffix(suffix));
    });
    if (kind, amount) != before {
        if kind != before.0 {
            amount = if kind == "Multiple" { 1.0 } else { 14.0 };
        }
        let (line, rule) = match kind {
            "Multiple" => (((amount * 240.0).round() as i64).to_string(), "auto"),
            "At least" => (twips(amount), "atLeast"),
            _ => (twips(amount), "exact"),
        };
        doc.session.format_paragraphs(move |ppr| {
            let spacing = ppr.child_or_insert("w:spacing");
            spacing.set_attr("w:line", line.clone());
            spacing.set_attr("w:lineRule", rule);
        });
    }
    let mut contextual = p.contextual;
    if ui
        .checkbox(&mut contextual, "Don't add space between paragraphs of the same style")
        .changed()
    {
        doc.session.format_paragraphs(move |ppr| {
            props::set_on_off(ppr, "w:contextualSpacing", contextual.then_some(true))
        });
    }
    ui.horizontal(|ui| {
        ui.label("Outline level");
        let mut level = p.outline_level.map_or(10u8, |l| l + 1);
        egui::ComboBox::from_id_salt(("word-outline-level-of", &doc.key))
            .selected_text(if level >= 10 {
                "Body text".to_owned()
            } else {
                format!("Level {level}")
            })
            .show_ui(ui, |ui| {
                crate::components::controls::set_in_the_list_size(ui);
                for n in 1..=9u8 {
                    ui.selectable_value(&mut level, n, format!("Level {n}"));
                }
                ui.selectable_value(&mut level, 10, "Body text");
            });
        if level != p.outline_level.map_or(10u8, |l| l + 1) {
            let value = (level.min(10) - 1).to_string();
            doc.session.format_paragraphs(move |ppr| {
                props::set_val(ppr, "w:outlineLvl", Some(value.as_str()))
            });
        }
    });
    heading(ui, "Pagination");
    let mut keep_next = p.keep_next;
    let mut keep_lines = p.keep_lines;
    let mut break_before = p.page_break_before;
    let mut widow = p.widow_control;
    for (label, value, element) in [
        ("Keep with next", &mut keep_next, "w:keepNext"),
        ("Keep lines together", &mut keep_lines, "w:keepLines"),
        ("Page break before", &mut break_before, "w:pageBreakBefore"),
        ("Widow and orphan control", &mut widow, "w:widowControl"),
    ] {
        if kit::check(ui, value, label).changed() {
            let on = *value;
            doc.session.format_paragraphs(move |ppr| props::set_on_off(ppr, element, Some(on)));
        }
    }
    heading(ui, "Borders and shading");
    ui.horizontal_wrapped(|ui| {
        if kit::button(ui, "Box").clicked() {
            doc.session.format_paragraphs(|ppr| {
                let mut borders = unluminous_word::xml::El::new("w:pBdr");
                for side in ["top", "left", "bottom", "right"] {
                    borders.put(
                        unluminous_word::xml::El::new(format!("w:{side}"))
                            .with("w:val", "single")
                            .with("w:sz", "4")
                            .with("w:space", "4")
                            .with("w:color", "auto"),
                    );
                }
                ppr.put(borders);
            });
        }
        if kit::button(ui, "Line below").clicked() {
            super::run_action(doc, "horizontal-line", requests);
        }
        if kit::button(ui, "No border").clicked() {
            doc.session.format_paragraphs(|ppr| ppr.remove_children("w:pBdr"));
        }
        if kit::button(ui, "Shade").clicked() {
            doc.session.format_paragraphs(|ppr| {
                ppr.put(
                    unluminous_word::xml::El::new("w:shd")
                        .with("w:val", "clear")
                        .with("w:color", "auto")
                        .with("w:fill", "F2F2F2"),
                )
            });
        }
        if kit::button(ui, "No shading").clicked() {
            doc.session.format_paragraphs(|ppr| ppr.remove_children("w:shd"));
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Border on");
        for side in ["top", "bottom", "left", "right", "between"] {
            if kit::button(ui, side).clicked() {
                let style =
                    doc.view.fields.get("border-style").cloned().unwrap_or_else(|| "single".into());
                let size =
                    doc.view.fields.get("border-width").cloned().unwrap_or_else(|| "4".into());
                let colour =
                    doc.view.fields.get("border-colour").cloned().unwrap_or_else(|| "auto".into());
                doc.session.format_paragraphs(move |ppr| {
                    let borders = ppr.child_or_insert("w:pBdr");
                    borders.put(
                        unluminous_word::xml::El::new(format!("w:{side}"))
                            .with("w:val", style.clone())
                            .with("w:sz", size.clone())
                            .with("w:space", "4")
                            .with("w:color", colour.clone()),
                    );
                    unluminous_word::order::sort(ppr);
                });
            }
        }
    });
    ui.horizontal(|ui| {
        let mut style =
            doc.view.fields.get("border-style").cloned().unwrap_or_else(|| "single".into());
        egui::ComboBox::from_id_salt(("word-border-style", &doc.key))
            .selected_text(&style)
            .show_ui(ui, |ui| {
                crate::components::controls::set_in_the_list_size(ui);
                for name in ["single", "double", "dotted", "dashed", "thick", "wave"] {
                    ui.selectable_value(&mut style, name.to_owned(), name);
                }
            });
        doc.view.fields.insert("border-style", style);
        let mut width = doc
            .view
            .fields
            .get("border-width")
            .and_then(|w| w.parse::<f32>().ok())
            .map_or(0.5, |w| w / 8.0);
        if ui
            .add(egui::DragValue::new(&mut width).range(0.25..=6.0).speed(0.05).suffix(" pt"))
            .changed()
        {
            doc.view.fields.insert("border-width", ((width * 8.0).round() as i64).to_string());
        }
        let mut colour = egui::Color32::BLACK; // any ground: a border colour, not a surface
        if ui.color_edit_button_srgba(&mut colour).changed() {
            doc.view.fields.insert(
                "border-colour",
                format!("{:02X}{:02X}{:02X}", colour.r(), colour.g(), colour.b()),
            );
        }
        let mut shade =
            p.shading.map_or(egui::Color32::WHITE, |c| egui::Color32::from_rgb(c.0, c.1, c.2)); // any ground: a shading colour
        ui.label("Shading");
        if ui.color_edit_button_srgba(&mut shade).changed() {
            let hex = format!("{:02X}{:02X}{:02X}", shade.r(), shade.g(), shade.b());
            doc.session.format_paragraphs(move |ppr| {
                ppr.put(
                    unluminous_word::xml::El::new("w:shd")
                        .with("w:val", "clear")
                        .with("w:color", "auto")
                        .with("w:fill", hex.clone()),
                );
                unluminous_word::order::sort(ppr);
            });
        }
    });
    heading(ui, "Sort");
    ui.horizontal(|ui| {
        for (label, ascending) in [("A to Z", true), ("Z to A", false)] {
            if kit::button(ui, label).clicked() {
                if let Err(problem) = doc.session.sort_paragraphs(ascending) {
                    requests.push(Request::Message(problem));
                }
            }
        }
    });
    heading(ui, "Tab stops");
    let field = doc.view.fields.entry("tab").or_default();
    let mut add = None;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(field)
                .hint_text(crate::components::controls::placeholder(
                    "points, e.g. 216",
                    &egui::TextStyle::Body.resolve(ui.style()),
                    crate::theme::color::text_faint(),
                ))
                .desired_width(90.0),
        );
        for (label, kind) in [
            ("Left", "left"),
            ("Center", "center"),
            ("Right", "right"),
            ("Decimal", "decimal"),
            ("Bar", "bar"),
        ] {
            if kit::button(ui, label).clicked() {
                add = field.trim().parse::<f32>().ok().map(|p| (p, kind));
            }
        }
    });
    let mut leader = doc.view.fields.get("tab-leader").cloned().unwrap_or_else(|| "none".into());
    ui.horizontal(|ui| {
        ui.label("Leader");
        for (label, value) in
            [("None", "none"), ("....", "dot"), ("----", "hyphen"), ("____", "underscore")]
        {
            if kit::toggle(ui, leader == value, label).clicked() {
                leader = value.to_owned();
            }
        }
    });
    doc.view.fields.insert("tab-leader", leader.clone());
    if let Some((points, kind)) = add {
        let pos = twips(points);
        doc.session.format_paragraphs(move |ppr| {
            let tabs = ppr.child_or_insert("w:tabs");
            let mut tab = unluminous_word::xml::El::new("w:tab")
                .with("w:val", kind)
                .with("w:pos", pos.clone());
            if leader != "none" {
                tab.set_attr("w:leader", leader.clone());
            }
            tabs.children.push(unluminous_word::xml::Node::El(tab));
        });
    }
    for stop in &p.tabs {
        ui.label(format!("{:?} at {} pt", stop.kind, stop.position));
    }
    if !p.tabs.is_empty() && kit::button(ui, "Clear tab stops").clicked() {
        doc.session.format_paragraphs(|ppr| ppr.remove_children("w:tabs"));
    }
}

fn styles(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, requests: &mut Vec<Request>) {
    let _ = look;
    let current = doc.session.paragraph_at_caret().style;
    heading(ui, "Paragraph styles");
    let listed = unluminous_word::styles::Resolver::new(&doc.session.document).listed("paragraph");
    for (id, name) in listed {
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    current == id,
                    RichText::new(&name).size(12.5 * kit::zoom(ui.ctx())),
                )
                .clicked()
            {
                if let Err(problem) = doc.session.apply_style(&id) {
                    requests.push(Request::Message(problem));
                }
            }
            let modify = ui.small_button("Modify");
            modify.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Modify {name}"))
            });
            if modify.clicked() {
                doc.view.modify_style = Some(id.clone());
                doc.view.panel.push(Level::ModifyStyle);
            }
        });
    }
    heading(ui, "Built in");
    actions_row(
        ui,
        doc,
        requests,
        &[
            "normal",
            "title",
            "subtitle",
            "heading-1",
            "heading-2",
            "heading-3",
            "heading-4",
            "quote",
            "intense-quote",
            "no-spacing",
        ],
    );
    heading(ui, "Character styles");
    let characters =
        unluminous_word::styles::Resolver::new(&doc.session.document).listed("character");
    ui.horizontal_wrapped(|ui| {
        for (id, name) in characters {
            if kit::button(ui, &name).clicked() {
                let _ = doc.session.apply_style(&id);
            }
        }
    });
    heading(ui, "New style from the selection");
    let field = doc.view.fields.entry("style").or_default();
    let mut make = None;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(field)
                .hint_text(crate::components::controls::placeholder(
                    "Name",
                    &egui::TextStyle::Body.resolve(ui.style()),
                    crate::theme::color::text_faint(),
                ))
                .desired_width(160.0),
        );
        if kit::button(ui, "Create").clicked() && !field.trim().is_empty() {
            make = Some(field.trim().to_owned());
        }
    });
    if let Some(name) = make {
        match new_style_from_selection(doc, &name) {
            Ok(()) => requests.push(Request::Message(format!("Made the style {name}"))),
            Err(problem) => requests.push(Request::Message(problem)),
        }
        doc.view.fields.remove("style");
    }
}

/// A paragraph style named `name`, based on the caret's style, with the caret's direct formatting, applied.
pub fn new_style_from_selection(doc: &mut Doc, name: &str) -> Result<(), String> {
    let head = doc.session.selection.head.clone();
    let paragraph =
        unluminous_word::position::paragraph(&doc.session.document, &head.story, &head.path)
            .ok_or("the caret is not in a paragraph")?;
    let mut ppr = paragraph.props.clone();
    ppr.remove_children("w:pStyle");
    ppr.remove_children("w:sectPr");
    ppr.remove_children("w:numPr");
    let rpr = unluminous_word::edit::props_at(paragraph, head.offset).0.as_ref().clone();
    let based = doc.session.paragraph_at_caret().style;
    let id: String = name.chars().filter(|c| c.is_alphanumeric()).collect();
    if id.is_empty() {
        return Err("a style needs a name with a letter or a number in it".into());
    }
    let mut style = unluminous_word::xml::El::new("w:style")
        .with("w:type", "paragraph")
        .with("w:customStyle", "1")
        .with("w:styleId", id.clone());
    style.put(unluminous_word::xml::El::new("w:name").with("w:val", name));
    style.put(unluminous_word::xml::El::new("w:basedOn").with("w:val", based));
    style.put(unluminous_word::xml::El::new("w:qFormat"));
    ppr.remove_children("w:rPr");
    style.put(ppr);
    let mut clean = rpr;
    clean.remove_children("w:rStyle");
    style.put(clean);
    doc.session.change(unluminous_word::session::Scope::Whole, move |document, selection, _| {
        if let Some(part) = document.styles.as_mut() {
            part.root.children.retain(|n| !matches!(n, unluminous_word::xml::Node::El(el) if el.attr("w:styleId") == Some(id.as_str())));
            part.root.children.push(unluminous_word::xml::Node::El(style.clone()));
            part.dirty = true;
        }
        selection.clone()
    });
    doc.session.apply_style(&name.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
}

fn lists(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    heading(ui, "Lists");
    actions_row(ui, doc, requests, &["bullets", "numbering"]);
    heading(ui, "Level");
    actions_row(ui, doc, requests, &["list-shallower", "list-deeper"]);
    heading(ui, "Numbering");
    let field = doc.view.fields.entry("start").or_insert_with(|| "1".to_owned());
    let mut restart = None;
    ui.horizontal(|ui| {
        ui.label("Start at");
        ui.add(egui::TextEdit::singleline(field).desired_width(50.0));
        if kit::button(ui, "Restart here").clicked() {
            restart = field.trim().parse::<i64>().ok();
        }
    });
    if let Some(start) = restart {
        doc.session.restart_list(start);
    }
    ui.label(RichText::new("Tab and Shift+Tab change the level of a list item. Typing - or 1. and a space at the start of a paragraph starts a list.").weak());
}

fn insert(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    heading(ui, "Table");
    let rows = doc.view.fields.entry("rows").or_insert_with(|| "3".to_owned()).clone();
    let columns = doc.view.fields.entry("columns").or_insert_with(|| "3".to_owned()).clone();
    let mut make = false;
    ui.horizontal(|ui| {
        let mut r = rows.clone();
        let mut c = columns.clone();
        ui.add(egui::TextEdit::singleline(&mut r).desired_width(36.0));
        ui.label("rows by");
        ui.add(egui::TextEdit::singleline(&mut c).desired_width(36.0));
        ui.label("columns");
        make = kit::button(ui, "Insert").clicked();
        doc.view.fields.insert("rows", r);
        doc.view.fields.insert("columns", c);
    });
    if make {
        let r = doc.view.fields.get("rows").and_then(|v| v.trim().parse().ok()).unwrap_or(3);
        let c = doc.view.fields.get("columns").and_then(|v| v.trim().parse().ok()).unwrap_or(3);
        doc.session.insert_table(r, c);
        doc.view.reveal = true;
    }
    if doc.session.cell_at_caret().is_some() {
        heading(ui, "This table");
        ui.horizontal_wrapped(|ui| {
            for (label, op) in [
                ("Row above", "row-above"),
                ("Row below", "row-below"),
                ("Column left", "column-left"),
                ("Column right", "column-right"),
                ("Delete row", "delete-row"),
                ("Delete column", "delete-column"),
                ("Merge right", "merge-right"),
                ("Merge down", "merge-down"),
                ("Split", "split"),
                ("Header row", "header-row"),
                ("Delete table", "delete-table"),
            ] {
                if kit::button(ui, label).clicked() {
                    if let Err(problem) = doc.session.table_op(op) {
                        requests.push(Request::Message(problem));
                    }
                }
            }
        });
        actions_row(
            ui,
            doc,
            requests,
            &[
                "merge-cells",
                "split-table",
                "distribute-rows",
                "distribute-columns",
                "sort-table",
                "table-to-text",
                "select-row",
                "select-column",
                "select-table",
            ],
        );
        deeper(ui, doc, Level::TableProperties);
        heading(ui, "Shade cell");
        let shades = ["D9E2F3", "FBE4D5", "E2EFD9", "FFF2CC", "EDEDED"];
        let names = ["Blue", "Orange", "Green", "Gold", "Grey"];
        let colours: Vec<(&str, egui::Color32)> =
            names.iter().zip(shades).map(|(name, hex)| (*name, hex_colour(hex))).collect();
        if let Some(index) = colour_swatches(ui, &colours) {
            let _ = doc.session.shade_cell(Rgb::parse(shades[index]));
        }
        if kit::button(ui, "No shading").clicked() {
            let _ = doc.session.shade_cell(None);
        }
    }
    heading(ui, "Picture");
    actions_row(ui, doc, requests, &["picture"]);
    if doc.session.selected_object().is_some() {
        deeper(ui, doc, Level::Picture);
    }
    heading(ui, "Text box and shapes");
    actions_row(
        ui,
        doc,
        requests,
        &[
            "text-box",
            "shape-rect",
            "shape-roundrect",
            "shape-ellipse",
            "shape-line",
            "shape-straightconnector1",
            "shape-rightarrow",
            "shape-leftarrow",
            "shape-uparrow",
            "shape-downarrow",
            "shape-triangle",
            "shape-diamond",
        ],
    );
    heading(ui, "Pages");
    actions_row(ui, doc, requests, &["blank-page", "cover-page"]);
    heading(ui, "Date and time");
    ui.horizontal_wrapped(|ui| {
        for format in [
            "M/d/yyyy",
            "dddd, MMMM d, yyyy",
            "MMMM d, yyyy",
            "d MMMM yyyy",
            "yyyy-MM-dd",
            "h:mm am/pm",
            "M/d/yyyy h:mm am/pm",
        ] {
            if kit::button(ui, format).clicked() {
                let code = format!("DATE \\@ \"{format}\"");
                let shown = unluminous_word::fields::format_date(
                    &unluminous_word::session::now_iso(),
                    Some(format),
                    format.contains('h'),
                );
                doc.session.insert_field(&code, &shown);
            }
        }
    });
    heading(ui, "Document property");
    ui.horizontal_wrapped(|ui| {
        for (label, code) in [
            ("Title", "TITLE"),
            ("Author", "AUTHOR"),
            ("Subject", "SUBJECT"),
            ("Keywords", "KEYWORDS"),
            ("File name", "FILENAME"),
            ("Pages", "NUMPAGES"),
            ("Words", "NUMWORDS"),
        ] {
            if kit::button(ui, label).clicked() {
                doc.session.insert_field(code, "");
                let pages = doc.pages.clone();
                doc.session.update_selected_fields(&pages);
            }
        }
    });
    heading(ui, "Text from file");
    let file = doc.view.fields.entry("text-from-file").or_default();
    let mut from_file = None;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(file)
                .hint_text(crate::components::controls::placeholder(
                    "a .docx, .txt or .md file",
                    &egui::TextStyle::Body.resolve(ui.style()),
                    crate::theme::color::text_faint(),
                ))
                .desired_width(170.0),
        );
        if kit::button(ui, "Insert").clicked() && !file.trim().is_empty() {
            from_file = Some(file.trim().to_owned());
        }
    });
    if let Some(path) = from_file {
        match crate::services::word::insert_text_from_file(doc, std::path::Path::new(&path)) {
            Ok(said) => requests.push(Request::Message(said)),
            Err(problem) => requests.push(Request::Message(problem)),
        }
    }
    heading(ui, "Breaks");
    actions_row(
        ui,
        doc,
        requests,
        &["page-break", "column-break", "line-break", "section-next-page", "section-continuous"],
    );
    heading(ui, "Link");
    let link = doc.view.fields.entry("link").or_default();
    let mut add_link = None;
    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(link)
                .hint_text(crate::components::controls::placeholder(
                    "https://… or #bookmark",
                    &egui::TextStyle::Body.resolve(ui.style()),
                    crate::theme::color::text_faint(),
                ))
                .desired_width(190.0),
        );
        if (kit::button(ui, "Link").clicked()
            || field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            && !link.trim().is_empty()
        {
            add_link = Some(link.trim().to_owned());
        }
    });
    if let Some(target) = add_link {
        doc.session.insert_hyperlink(&target, None);
        doc.view.fields.remove("link");
        requests.push(Request::TakeTheKeyboard(true));
    }
    heading(ui, "Field");
    let code = doc.view.fields.entry("field").or_default();
    let mut add_field = None;
    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(code)
                .hint_text(crate::components::controls::placeholder(
                    "PAGE, DATE, AUTHOR, NUMWORDS, = 2 * 21",
                    &egui::TextStyle::Body.resolve(ui.style()),
                    crate::theme::color::text_faint(),
                ))
                .desired_width(190.0),
        );
        if (kit::button(ui, "Insert").clicked()
            || field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            && !code.trim().is_empty()
        {
            add_field = Some(code.trim().to_owned());
        }
    });
    if let Some(code) = add_field {
        doc.session.insert_field(&code, "");
        let pages = doc.pages.clone();
        doc.session.update_fields(&pages);
        doc.view.fields.remove("field");
        requests.push(Request::TakeTheKeyboard(true));
    }
    heading(ui, "Bookmark");
    let mark = doc.view.fields.entry("bookmark").or_default();
    let mut add_mark = None;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(mark)
                .hint_text(crate::components::controls::placeholder(
                    "Name",
                    &egui::TextStyle::Body.resolve(ui.style()),
                    crate::theme::color::text_faint(),
                ))
                .desired_width(150.0),
        );
        if kit::button(ui, "Add").clicked() && !mark.trim().is_empty() {
            add_mark = Some(mark.trim().replace(' ', "_"));
        }
    });
    if let Some(name) = add_mark {
        doc.session.insert_bookmark(&name);
        doc.view.fields.remove("bookmark");
    }
    deeper(ui, doc, Level::Symbol);
    heading(ui, "Symbol");
    ui.horizontal_wrapped(|ui| {
        for symbol in [
            "©", "®", "™", "§", "¶", "†", "‡", "•", "…", "—", "–", "€", "£", "¥", "°", "±", "×",
            "÷", "≠", "≤", "≥", "→", "←", "✓",
        ] {
            if kit::button(ui, symbol).clicked() {
                doc.session.type_text(symbol);
            }
        }
    });
    heading(ui, "Fields");
    actions_row(ui, doc, requests, &["date", "page-number", "horizontal-line"]);
}

fn page(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    let geometry = unluminous_word::layout::section::Geometry::of(
        &doc.session.section_at_caret().unwrap_or_default(),
    );
    heading(ui, "Size");
    ui.horizontal_wrapped(|ui| {
        for (name, _, _) in unluminous_word::ops::PAGE_SIZES {
            if kit::button(ui, name.to_uppercase()).clicked() {
                let _ = doc.session.set_page_size(name);
            }
        }
    });
    ui.label(format!("{:.1} by {:.1} inches", geometry.width / 72.0, geometry.height / 72.0));
    heading(ui, "Orientation");
    actions_row(ui, doc, requests, &["portrait", "landscape"]);
    heading(ui, "Margins");
    actions_row(ui, doc, requests, &["margins-normal", "margins-narrow", "margins-wide"]);
    let mut margins = [geometry.top, geometry.bottom, geometry.left, geometry.right];
    let mut changed = false;
    egui::Grid::new("word-margins").num_columns(4).show(ui, |ui| {
        let sides = ["Top", "Bottom", "Left", "Right"];
        for (index, (label, value)) in sides.iter().zip(margins.iter_mut()).enumerate() {
            ui.label(*label);
            changed |= ui
                .add(egui::DragValue::new(value).range(0.0..=400.0).speed(0.5).suffix(" pt"))
                .changed();
            // Two sides a row, so the grid fits the panel's width.
            if index % 2 == 1 {
                ui.end_row();
            }
        }
    });
    if changed {
        doc.session.set_margins(margins);
    }
    deeper(ui, doc, Level::PageSetup);
    heading(ui, "Columns");
    actions_row(ui, doc, requests, &["one-column", "two-columns", "three-columns"]);
    heading(ui, "Header and footer");
    for (key, label, footer) in [("header", "Header", false), ("footer", "Footer", true)] {
        let field = doc.view.fields.entry(key).or_default();
        let mut set = None;
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(
                egui::TextEdit::singleline(field)
                    .hint_text(crate::components::controls::placeholder(
                        "Text, with {PAGE} and {NUMPAGES}",
                        &egui::TextStyle::Body.resolve(ui.style()),
                        crate::theme::color::text_faint(),
                    ))
                    .desired_width(170.0),
            );
            if kit::button(ui, "Set").clicked() {
                set = Some(field.clone());
            }
        });
        if let Some(text) = set {
            doc.session.set_header_text(footer, "default", &text);
        }
    }
    actions_row(ui, doc, requests, &["page-numbers"]);
    heading(ui, "Page colour");
    let pages = ["FFFFFF", "FFF8E7", "F2F2F2", "EAF1FB", "EEF7EE"];
    let names = ["White", "Cream", "Grey", "Blue", "Green"];
    let colours: Vec<(&str, egui::Color32)> =
        names.iter().zip(pages).map(|(name, hex)| (*name, hex_colour(hex))).collect();
    let pressed = colour_swatches(ui, &colours);
    {
        for (index, hex) in pages.iter().enumerate() {
            if pressed == Some(index) {
                let white = *hex == "FFFFFF";
                let hex = hex.to_string();
                doc.session.change(
                    unluminous_word::session::Scope::Whole,
                    move |document, selection, _| {
                        document.background = (!white).then(|| {
                            unluminous_word::xml::El::new("w:background")
                                .with("w:color", hex.clone())
                        });
                        if let Some(settings) = document.settings.as_mut() {
                            props::set_on_off(
                                &mut settings.root,
                                "w:displayBackgroundShape",
                                (!white).then_some(true),
                            );
                            settings.dirty = true;
                        }
                        selection.clone()
                    },
                );
            }
        }
    }
}

/// A row that opens a third level.
fn deeper(ui: &mut egui::Ui, doc: &mut Doc, level: Level) {
    let z = kit::zoom(ui.ctx());
    ui.add_space(2.0 * z);
    if level_row(ui, level.name(), "", mark_of(level)) {
        doc.view.panel.push(level);
    }
}

fn references(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    heading(ui, "Table of contents");
    actions_row(
        ui,
        doc,
        requests,
        &["table-of-contents", "update-table", "remove-table-of-contents", "table-of-figures"],
    );
    deeper(ui, doc, Level::Contents);
    heading(ui, "Fields");
    actions_row(
        ui,
        doc,
        requests,
        &["update-fields", "field-codes", "unlink-fields", "lock-fields", "unlock-fields"],
    );
    heading(ui, "Notes");
    actions_row(ui, doc, requests, &["footnote", "endnote", "next-footnote", "previous-footnote"]);
    deeper(ui, doc, Level::NoteOptions);
    heading(ui, "Cross reference");
    let targets = doc.session.cross_reference_targets();
    let mut target: usize =
        doc.view.fields.get("xref-target").and_then(|t| t.parse().ok()).unwrap_or(usize::MAX);
    let shown = targets
        .get(target)
        .map_or_else(|| "Choose a heading, caption or bookmark".to_owned(), |t| t.1.clone());
    egui::ComboBox::from_id_salt(("word-xref", &doc.key))
        .selected_text(shown)
        .width(ui.available_width() - 20.0)
        .show_ui(ui, |ui| {
            crate::components::controls::set_in_the_list_size(ui);
            for (index, (_, label)) in targets.iter().enumerate() {
                ui.selectable_value(&mut target, index, label);
            }
        });
    doc.view.fields.insert("xref-target", target.to_string());
    ui.horizontal_wrapped(|ui| {
        for (label, what) in [
            ("Insert its text", "text"),
            ("Its page number", "page"),
            ("Above or below", "above-below"),
        ] {
            if kit::button(ui, label).clicked() {
                if let Some((path, _)) = targets.get(target) {
                    let pages = doc.pages.clone();
                    if let Err(problem) = doc.session.insert_cross_reference(path, what, &pages) {
                        requests.push(Request::Message(problem));
                    }
                }
            }
        }
    });
    heading(ui, "Captions");
    actions_row(ui, doc, requests, &["caption"]);
    heading(ui, "Bookmarks");
    for (name, pos) in doc.session.bookmarks() {
        if ui.link(&name).clicked() {
            doc.session.select(pos.clone(), pos);
            doc.view.reveal = true;
        }
    }
}

/// Insert -> Table properties: the table's alignment and width, the selected rows' height and whether
/// they repeat or break across pages, the selected cells' vertical alignment, the table style with its
/// options, and borders.
/// The print options read from the Print level's fields.
fn print_options(doc: &mut Doc) -> Result<crate::services::word::print::PrintOptions, String> {
    use crate::services::word::print::{parse_range, Parity, PrintOptions};
    doc.ensure_pages();
    let field = |doc: &Doc, name: &str| doc.view.fields.get(name).cloned().unwrap_or_default();
    let count = doc.pages.pages.len();
    Ok(PrintOptions {
        pages: parse_range(&field(doc, "print-pages"), count)?,
        parity: match field(doc, "print-parity").as_str() {
            "odd" => Parity::Odd,
            "even" => Parity::Even,
            _ => Parity::All,
        },
        copies: field(doc, "print-copies").trim().parse().unwrap_or(1).clamp(1, 99),
        per_sheet: field(doc, "print-per-sheet").trim().parse().unwrap_or(1),
    })
}

/// The Print level (Ctrl+P): which pages, how many copies, odd or even, pages per sheet, then Print,
/// which hands the PDF to the system's print dialog, or Preview, which opens it in a tab.
fn print(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    heading(ui, "Pages");
    ui.horizontal(|ui| {
        ui.label("Pages");
        let pages = doc.view.fields.entry("print-pages").or_default();
        ui.add(
            egui::TextEdit::singleline(pages)
                .hint_text(crate::components::controls::placeholder(
                    "all, or 1-3, 5",
                    &egui::TextStyle::Body.resolve(ui.style()),
                    crate::theme::color::text_faint(),
                ))
                .desired_width(120.0),
        );
    });
    ui.horizontal(|ui| {
        for (label, value) in [("All pages", "all"), ("Odd pages", "odd"), ("Even pages", "even")] {
            let chosen = doc.view.fields.get("print-parity").map_or("all", String::as_str) == value;
            if kit::toggle(ui, chosen, label).clicked() {
                doc.view.fields.insert("print-parity", value.to_owned());
            }
        }
    });
    heading(ui, "Copies and sheets");
    ui.horizontal(|ui| {
        ui.label("Copies");
        let copies = doc.view.fields.entry("print-copies").or_insert_with(|| "1".to_owned());
        ui.add(egui::TextEdit::singleline(copies).desired_width(40.0));
    });
    ui.horizontal(|ui| {
        ui.label("Pages per sheet");
        for value in ["1", "2", "4"] {
            let chosen =
                doc.view.fields.get("print-per-sheet").map_or("1", String::as_str) == value;
            if kit::toggle(ui, chosen, value).clicked() {
                doc.view.fields.insert("print-per-sheet", value.to_owned());
            }
        }
    });
    ui.add_space(6.0);
    let (mut print_now, mut preview) = (false, false);
    ui.horizontal(|ui| {
        print_now = kit::button(ui, "Print").clicked();
        preview = kit::button(ui, "Preview").clicked();
    });
    if print_now || preview {
        let result = print_options(doc).and_then(|options| {
            if preview {
                crate::services::word::print::write_pdf(doc, &options).map(|path| {
                    requests.push(Request::OpenFile(path.clone()));
                    format!("Preview of {}", path.display())
                })
            } else {
                crate::services::word::print::print(doc, &options)
            }
        });
        match result {
            Ok(said) => requests.push(Request::Message(said)),
            Err(problem) => requests.push(Request::Notice {
                text: problem,
                kind: crate::components::toast::Kind::Problem,
            }),
        }
    }
    ui.label(
        RichText::new("Print hands a PDF of the pages to the system's print dialog, where the printer, duplex and paper are chosen.")
            .weak(),
    );
}

/// The selected picture, shape or text box: its size, how text wraps round it, where it sits, its crop,
/// its alt text and its place in the stack (task-2359, TDD 5.3).
fn picture(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    let Some(info) = doc.session.object_info() else {
        ui.label(RichText::new("Select a picture, shape or text box to change it.").weak());
        return;
    };
    let mut result = Ok(());
    heading(ui, &format!("{} size", info.kind));
    let key = format!("{}:{}:{:?}", info.width, info.height, info.at);
    if doc.view.fields.get("picture-key") != Some(&key) {
        doc.view.fields.insert("picture-key", key);
        doc.view.fields.insert("picture-width", format!("{:.1}", info.width));
        doc.view.fields.insert("picture-height", format!("{:.1}", info.height));
        doc.view.fields.insert("picture-alt", info.alt.clone());
        let offset = |o: Option<f32>| o.map_or_else(String::new, |v| format!("{v:.1}"));
        doc.view.fields.insert("picture-x", offset(info.horizontal.1));
        doc.view.fields.insert("picture-y", offset(info.vertical.1));
    }
    let mut locked = doc.view.fields.get("picture-free").is_none_or(|v| v != "1");
    let mut apply = false;
    ui.horizontal(|ui| {
        ui.label("Width");
        let w = doc.view.fields.entry("picture-width").or_default();
        ui.add(egui::TextEdit::singleline(w).desired_width(48.0));
        ui.label("Height");
        let h = doc.view.fields.entry("picture-height").or_default();
        ui.add(egui::TextEdit::singleline(h).desired_width(48.0));
        apply = kit::button(ui, "Set").clicked();
    });
    if kit::check(ui, &mut locked, "Lock the ratio").changed() {
        doc.view.fields.insert("picture-free", if locked { "0" } else { "1" }.to_owned());
    }
    if apply {
        let read =
            |name: &str| doc.view.fields.get(name).and_then(|v| v.trim().parse::<f32>().ok());
        if let (Some(w), Some(h)) = (read("picture-width"), read("picture-height")) {
            let width_moved = (w - info.width).abs() > 0.05;
            let height_moved = (h - info.height).abs() > 0.05;
            result = match (locked, width_moved, height_moved) {
                (_, false, false) => Ok(()),
                (true, true, _) => doc.session.set_object_size(w, None),
                (true, false, true) => {
                    doc.session.set_object_size(info.width * h / info.height.max(0.01), Some(h))
                }
                (false, _, _) => doc.session.set_object_size(w, Some(h)),
            };
        }
    }
    heading(ui, "Wrap text");
    actions_row(
        ui,
        doc,
        requests,
        &[
            "wrap-inline",
            "wrap-square",
            "wrap-tight",
            "wrap-through",
            "wrap-top-and-bottom",
            "wrap-behind",
            "wrap-front",
        ],
    );
    ui.label(RichText::new(format!("Now: {}", info.wrap)).weak());
    if info.wrap != "inline" {
        heading(ui, "Position");
        let mut chosen =
            doc.view.fields.entry("picture-relative").or_insert_with(|| "page".to_owned()).clone();
        egui::ComboBox::from_id_salt("word-picture-relative").selected_text(&chosen).show_ui(
            ui,
            |ui| {
                for name in [
                    "page",
                    "margin",
                    "column",
                    "page/paragraph",
                    "margin/paragraph",
                    "column/paragraph",
                ] {
                    ui.selectable_value(&mut chosen, name.to_owned(), name);
                }
            },
        );
        doc.view.fields.insert("picture-relative", chosen.clone());
        let mut place = false;
        ui.horizontal(|ui| {
            ui.label("Across");
            let x = doc.view.fields.entry("picture-x").or_default();
            ui.add(egui::TextEdit::singleline(x).desired_width(48.0));
            ui.label("Down");
            let y = doc.view.fields.entry("picture-y").or_default();
            ui.add(egui::TextEdit::singleline(y).desired_width(48.0));
            place = kit::button(ui, "Set").clicked();
        });
        ui.label(
            RichText::new(format!("From {} and {}", info.horizontal.0, info.vertical.0)).weak(),
        );
        if place {
            let read = |name: &str| {
                doc.view.fields.get(name).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0)
            };
            result = doc.session.set_object_position(&chosen, read("picture-x"), read("picture-y"));
        }
        heading(ui, "Order");
        actions_row(
            ui,
            doc,
            requests,
            &["bring-forward", "send-backward", "bring-to-front", "send-to-back"],
        );
    }
    if info.kind == "Picture" {
        heading(ui, "Crop");
        actions_row(ui, doc, requests, &["crop", "reset-picture"]);
        if info.crop != (0.0, 0.0, 0.0, 0.0) {
            let (l, t, r, b) = info.crop;
            ui.label(
                RichText::new(format!(
                    "Cut off: left {:.0}%, top {:.0}%, right {:.0}%, bottom {:.0}%",
                    l * 100.0,
                    t * 100.0,
                    r * 100.0,
                    b * 100.0
                ))
                .weak(),
            );
        }
    }
    heading(ui, "Alt text");
    let mut set_alt = false;
    {
        let alt = doc.view.fields.entry("picture-alt").or_default();
        ui.add(
            egui::TextEdit::multiline(alt).desired_rows(2).desired_width(f32::INFINITY).hint_text(
                crate::components::controls::placeholder(
                    "Describe it for a reader who cannot see it",
                    &egui::TextStyle::Body.resolve(ui.style()),
                    crate::theme::color::text_faint(),
                ),
            ),
        );
    }
    ui.horizontal(|ui| {
        set_alt = kit::button(ui, "Set alt text").clicked();
        if kit::button(ui, "Mark as decorative").clicked() {
            doc.view.fields.insert("picture-alt", String::new());
            set_alt = true;
        }
    });
    if set_alt {
        let text = doc.view.fields.get("picture-alt").cloned().unwrap_or_default();
        result = doc.session.set_object_alt_text(&text);
    }
    if let Err(problem) = result {
        requests.push(Request::Message(problem));
    }
}

fn table_properties(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    if doc.session.cell_at_caret().is_none() {
        ui.label(RichText::new("Put the caret in a table to change its properties.").weak());
        return;
    }
    let mut set = |doc: &mut Doc, name: &str, values: &[&str]| {
        let values: Vec<String> = values.iter().map(|v| (*v).to_owned()).collect();
        if let Err(problem) = doc.session.table_property(name, &values) {
            requests.push(Request::Message(problem));
        }
    };
    heading(ui, "Table");
    ui.horizontal(|ui| {
        ui.label("Alignment");
        for (label, value) in [("Left", "left"), ("Center", "center"), ("Right", "right")] {
            if kit::button(ui, label).clicked() {
                set(doc, "align", &[value]);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Width");
        for (label, value) in [("Auto", "auto"), ("50%", "50%"), ("75%", "75%"), ("100%", "100%")] {
            if kit::button(ui, label).clicked() {
                set(doc, "width", &[value]);
            }
        }
    });
    heading(ui, "Rows");
    let height = doc.view.fields.entry("row-height").or_insert_with(|| "0".to_owned());
    let mut apply_height = false;
    ui.horizontal(|ui| {
        ui.label("Height (pt, 0 for auto)");
        ui.add(egui::TextEdit::singleline(height).desired_width(44.0));
        apply_height = kit::button(ui, "Set").clicked();
    });
    if apply_height {
        let value = doc.view.fields.get("row-height").cloned().unwrap_or_default();
        set(doc, "row-height", &[value.as_str()]);
    }
    ui.horizontal(|ui| {
        if kit::button(ui, "Repeat as header row").clicked() {
            set(doc, "repeat-header", &["on"]);
        }
        if kit::button(ui, "Keep row on one page").clicked() {
            set(doc, "allow-break", &["off"]);
        }
        if kit::button(ui, "Allow row to break").clicked() {
            set(doc, "allow-break", &["on"]);
        }
    });
    heading(ui, "Cells");
    ui.horizontal(|ui| {
        ui.label("Vertical alignment");
        for (label, value) in [("Top", "top"), ("Center", "center"), ("Bottom", "bottom")] {
            if kit::button(ui, label).clicked() {
                set(doc, "valign", &[value]);
            }
        }
    });
    heading(ui, "Style");
    ui.horizontal_wrapped(|ui| {
        for (label, id) in [("Table Grid", "TableGrid"), ("Plain", "TableNormal")] {
            if kit::button(ui, label).clicked() {
                if let Err(problem) = doc.session.apply_table_style(id, None) {
                    requests.push(Request::Message(problem));
                }
            }
        }
    });
    heading(ui, "Borders");
    ui.horizontal_wrapped(|ui| {
        for (label, which) in
            [("All", "all"), ("Outside", "outside"), ("Inside", "inside"), ("None", "none")]
        {
            if kit::button(ui, label).clicked() {
                if let Err(problem) = doc.session.set_table_borders(which, 0.5, None) {
                    requests.push(Request::Message(problem));
                }
            }
        }
    });
}

/// Review -> Proofing: a tick box for each autocorrect rule.
fn proofing(ui: &mut egui::Ui, doc: &mut Doc) {
    heading(ui, "AutoCorrect as you type");
    for (name, label) in unluminous_word::autocorrect::RULES {
        let Some(slot) = doc.session.autocorrect_rules.slot(name) else { continue };
        kit::check(ui, slot, *label);
    }
    ui.label(RichText::new("Ctrl+Z straight after a correction gives back what was typed.").weak());
}

/// References -> Table of contents: the custom table of contents.
fn contents(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    use unluminous_word::references::TocOptions;
    let field = |doc: &mut Doc, key: &'static str, default: &str| {
        doc.view.fields.entry(key).or_insert_with(|| default.to_owned()).clone()
    };
    let mut levels: u8 = field(doc, "toc-levels", "3").parse().unwrap_or(3);
    let mut pages = field(doc, "toc-pages", "true") == "true";
    let mut right = field(doc, "toc-right", "true") == "true";
    let mut links = field(doc, "toc-links", "true") == "true";
    let mut leader = field(doc, "toc-leader", "dot");
    heading(ui, "Table of contents");
    ui.horizontal(|ui| {
        ui.label("Show levels");
        ui.add(egui::DragValue::new(&mut levels).range(1..=9));
    });
    kit::check(ui, &mut pages, "Show page numbers");
    ui.add_enabled(pages, egui::Checkbox::new(&mut right, "Right align page numbers"));
    ui.horizontal(|ui| {
        ui.label("Tab leader");
        for (label, value) in
            [("None", "none"), ("Dots", "dot"), ("Dashes", "hyphen"), ("Line", "underscore")]
        {
            ui.radio_value(&mut leader, value.to_owned(), label);
        }
    });
    kit::check(ui, &mut links, "Use hyperlinks instead of page numbers on the web");
    doc.view.fields.insert("toc-levels", levels.to_string());
    doc.view.fields.insert("toc-pages", pages.to_string());
    doc.view.fields.insert("toc-right", right.to_string());
    doc.view.fields.insert("toc-links", links.to_string());
    doc.view.fields.insert("toc-leader", leader.clone());
    let options = TocOptions {
        levels,
        page_numbers: pages,
        right_align: right,
        leader,
        hyperlinks: links,
        caption: None,
    };
    ui.horizontal(|ui| {
        if kit::button(ui, "Insert").clicked() {
            let laid = doc.pages.clone();
            doc.session.insert_toc_with(&options, &laid);
            requests.push(Request::TakeTheKeyboard(true));
        }
        if kit::button(ui, "Replace the table").clicked() {
            let laid = doc.pages.clone();
            if doc.session.remove_toc() {
                doc.session.insert_toc_with(&options, &laid);
            }
        }
    });
}

fn review(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, requests: &mut Vec<Request>) {
    deeper(ui, doc, Level::Proofing);
    heading(ui, "Spelling");
    let mut spelling = doc.view.spelling;
    if kit::check(ui, &mut spelling, "Check spelling as you type").changed() {
        super::run_action(doc, "check-spelling", requests);
    }
    if doc.view.spelling {
        let count = crate::services::word::spelling::misspellings(&doc.session.document).len();
        let said = match count {
            0 => "No spelling errors".to_owned(),
            1 => "1 spelling error".to_owned(),
            n => format!("{n} spelling errors"),
        };
        ui.label(RichText::new(said).color(look.palette.text_dim));
        if count > 0 && kit::button(ui, "Next error (F7)").clicked() {
            super::run_action(doc, "spelling", requests);
            requests.push(Request::TakeTheKeyboard(true));
        }
    }
    heading(ui, "Track changes");
    let mut tracking = doc.session.tracking;
    if kit::check(ui, &mut tracking, "Track changes").changed() {
        doc.session.set_tracking(tracking);
    }
    actions_row(ui, doc, requests, &["accept-change", "reject-change", "accept-all", "reject-all"]);
    heading(ui, "Comments");
    let field = doc.view.fields.entry("comment").or_default();
    let mut add = None;
    ui.add(
        egui::TextEdit::multiline(field)
            .hint_text(crate::components::controls::placeholder(
                "A comment on the selection",
                &egui::TextStyle::Body.resolve(ui.style()),
                crate::theme::color::text_faint(),
            ))
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
    if kit::button(ui, "Comment").clicked() && !field.trim().is_empty() {
        add = Some(field.trim().to_owned());
    }
    if let Some(text) = add {
        doc.session.add_comment(&text);
        doc.view.fields.remove("comment");
    }
    let comments = doc.session.comments();
    if !comments.is_empty() {
        ui.horizontal(|ui| {
            if kit::button(ui, "Previous").clicked() {
                doc.session.go_to_comment(false);
                doc.view.reveal = true;
            }
            if kit::button(ui, "Next").clicked() {
                doc.session.go_to_comment(true);
                doc.view.reveal = true;
            }
            if kit::button(ui, "Delete all").clicked() {
                doc.session.delete_all_comments();
            }
        });
    }
    for comment in comments {
        let id = comment["id"].as_i64().unwrap_or(0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(format!(
                    "{}  {}",
                    comment["author"].as_str().unwrap_or(""),
                    comment["date"].as_str().unwrap_or("").get(..10).unwrap_or("")
                ))
                .color(look.palette.text_dim)
                .size(11.0 * kit::zoom(ui.ctx())),
            );
            if let Some(on) = comment["on"].as_str().filter(|s| !s.is_empty()) {
                ui.label(RichText::new(format!("“{on}”")).italics().color(look.palette.text_dim));
            }
            // The comment being edited is a text box; Save writes it back as one undo step.
            let key = "comment-edit";
            let editing =
                doc.view.fields.get("comment-edit-id").and_then(|v| v.parse::<i64>().ok())
                    == Some(id);
            if editing {
                let field = doc.view.fields.entry(key).or_default();
                ui.add(
                    egui::TextEdit::multiline(field).desired_rows(2).desired_width(f32::INFINITY),
                );
            } else {
                ui.label(comment["text"].as_str().unwrap_or(""));
            }
            ui.horizontal(|ui| {
                if editing {
                    if kit::button(ui, "Save").clicked() {
                        let text = doc.view.fields.remove(key).unwrap_or_default();
                        doc.view.fields.remove("comment-edit-id");
                        let _ = doc.session.edit_comment(id, text.trim());
                    }
                    if kit::button(ui, "Cancel").clicked() {
                        doc.view.fields.remove(key);
                        doc.view.fields.remove("comment-edit-id");
                    }
                } else if kit::button(ui, "Edit").clicked() {
                    doc.view.fields.insert("comment-edit-id", id.to_string());
                    doc.view.fields.insert(key, comment["text"].as_str().unwrap_or("").to_owned());
                }
                let resolved = comment["resolved"].as_bool().unwrap_or(false);
                if kit::button(ui, if resolved { "Reopen" } else { "Resolve" }).clicked() {
                    let _ = doc.session.resolve_comment(id, !resolved);
                }
                if kit::button(ui, "Delete").clicked() {
                    let _ = doc.session.delete_comment(id);
                }
            });
        });
    }
    heading(ui, "Display for review");
    ui.horizontal_wrapped(|ui| {
        use unluminous_word::review::Markup;
        for (label, mode) in [
            ("Simple Markup", Markup::Simple),
            ("All Markup", Markup::All),
            ("No Markup", Markup::None),
            ("Original", Markup::Original),
        ] {
            if kit::toggle(ui, doc.view.markup == mode, label).clicked() {
                doc.view.markup = mode;
                doc.laid = 0;
            }
        }
    });
    kit::check(ui, &mut doc.view.show_comments, "Show comments");
    ui.horizontal(|ui| {
        if kit::button(ui, "Previous change").clicked() {
            doc.session.go_to_change(false);
            doc.view.reveal = true;
        }
        if kit::button(ui, "Next change").clicked() {
            doc.session.go_to_change(true);
            doc.view.reveal = true;
        }
    });
    heading(ui, "Lock tracking");
    let password = doc.view.fields.entry("lock-password").or_default();
    let mut lock = None;
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(password).password(true).desired_width(140.0));
        let locked = doc.session.tracking_locked();
        if kit::button(ui, if locked { "Unlock" } else { "Lock" }).clicked() {
            lock = Some((locked, password.clone()));
        }
    });
    if let Some((locked, password)) = lock {
        doc.view.fields.remove("lock-password");
        if locked {
            if doc.session.protection_password_matches(&password) {
                doc.session.lock_tracking("");
            } else {
                requests.push(Request::Message("That is not the password".into()));
            }
        } else if !password.is_empty() {
            doc.session.lock_tracking(&password);
        }
    }
    heading(ui, "Language");
    let tag = doc.view.fields.entry("language").or_insert_with(|| "en-US".to_owned());
    let mut set_language = None;
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(tag).desired_width(80.0));
        if kit::button(ui, "Set for the selection").clicked() {
            set_language = Some(tag.clone());
        }
    });
    if let Some(tag) = set_language {
        doc.session.set_language(tag.trim());
    }
    heading(ui, "Accessibility");
    let issues = doc.session.accessibility();
    if issues.is_empty() {
        ui.label(RichText::new("No accessibility issues found").color(look.palette.text_dim));
    }
    for issue in issues.into_iter().take(30) {
        if ui.link(&issue.message).clicked() {
            doc.session.select(issue.at.clone(), issue.at);
            doc.view.reveal = true;
        }
    }
    heading(ui, "Document inspector");
    let found = doc.session.inspect();
    for (label, count, kind) in [
        ("Comments", found.comments, "comments"),
        ("Tracked changes", found.revisions, "revisions"),
        ("Hidden text", found.hidden, "hidden"),
        ("Personal names in properties", found.personal.len(), "personal"),
    ] {
        ui.horizontal(|ui| {
            ui.label(format!("{label}: {count}"));
            if count > 0 && kit::button(ui, "Remove all").clicked() {
                if let Err(problem) = doc.session.remove_inspected(kind) {
                    requests.push(Request::Message(problem));
                }
            }
        });
    }
    heading(ui, "Word count");
    kit::check(ui, &mut doc.view.count_notes, "Include text boxes, footnotes and endnotes");
    let mut stats = doc.session.stats(Some(&doc.pages));
    if doc.view.count_notes {
        let (words, characters) = doc.session.word_count_with_notes();
        stats.words = words;
        stats.characters = characters;
    }
    egui::Grid::new("word-stats").num_columns(2).show(ui, |ui| {
        for (label, value) in [
            ("Pages", stats.pages),
            ("Words", stats.words),
            ("Characters (no spaces)", stats.characters),
            ("Characters (with spaces)", stats.characters_with_spaces),
            ("Paragraphs", stats.paragraphs),
            ("Lines", stats.lines),
        ] {
            ui.label(label);
            ui.label(value.to_string());
            ui.end_row();
        }
    });
}

/// Protect Document: Encrypt with Password, Password to Modify and Restrict Editing, each through the
/// `protect` verb an agent runs, so the two cannot differ.
fn protect_document(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    heading(ui, "Protect document");
    let mut asked: Option<Vec<String>> = None;
    let encrypted = doc.password.is_some();
    let password = doc.view.fields.entry("protect-password").or_default();
    ui.horizontal(|ui| {
        ui.label("Password");
        ui.add(egui::TextEdit::singleline(password).password(true).desired_width(150.0));
    });
    let password = password.clone();
    ui.horizontal_wrapped(|ui| {
        let encrypt = if encrypted { "Stop encrypting" } else { "Encrypt with password" };
        if kit::button(ui, encrypt).clicked() {
            let value = if encrypted { "none".to_owned() } else { password.clone() };
            asked = Some(vec!["password".into(), value]);
        }
        if kit::button(ui, "Password to modify").clicked() {
            asked = Some(vec![
                "modify".into(),
                if password.is_empty() { "none".into() } else { password.clone() },
            ]);
        }
    });
    match doc.session.protection.clone() {
        Some(kind) => {
            ui.label(format!("Restrict Editing allows {kind}"));
            if kit::button(ui, "Stop protection").clicked() {
                asked = Some(vec!["stop".into(), password.clone()]);
            }
        }
        None => {
            ui.horizontal_wrapped(|ui| {
                ui.label("Restrict editing to");
                for (label, kind) in [
                    ("No changes", "readOnly"),
                    ("Comments", "comments"),
                    ("Tracked changes", "trackedChanges"),
                    ("Filling in forms", "forms"),
                ] {
                    if kit::button(ui, label).clicked() {
                        asked = Some(vec!["restrict".into(), kind.into(), password.clone()]);
                    }
                }
            });
        }
    }
    if let Some(arguments) = asked {
        if arguments.iter().skip(1).any(String::is_empty) {
            requests.push(Request::Message("Type a password first".into()));
            return;
        }
        doc.view.fields.remove("protect-password");
        match crate::services::word::commands::protect(doc, &arguments) {
            Ok(answer) => requests.push(Request::Message(answer.message)),
            Err(problem) => requests.push(Request::Message(problem)),
        }
    }
}

fn document(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    deeper(ui, doc, Level::Print);
    heading(ui, "Properties");
    for (label, element) in [
        ("Title", "dc:title"),
        ("Subject", "dc:subject"),
        ("Author", "dc:creator"),
        ("Keywords", "cp:keywords"),
    ] {
        let current =
            unluminous_word::new::property(&doc.session.document, element).unwrap_or_default();
        let field = doc.view.fields.entry(element_key(element)).or_insert_with(|| current.clone());
        let mut set = false;
        ui.horizontal(|ui| {
            ui.label(label);
            let response = ui.add(egui::TextEdit::singleline(field).desired_width(190.0));
            set = response.lost_focus();
        });
        if set && *field != current {
            let value = field.clone();
            unluminous_word::new::set_property(&mut doc.session.document, element, &value);
            doc.session.revision += 1;
            doc.session.modified = true;
        }
    }
    protect_document(ui, doc, requests);
    heading(ui, "Views");
    actions_row(ui, doc, requests, &["read-mode", "print-layout", "outline-view", "new-window"]);
    heading(ui, "Show");
    actions_row(
        ui,
        doc,
        requests,
        &["ruler", "gridlines", "formatting-marks", "navigation", "field-codes"],
    );
    heading(ui, "Field shading");
    actions_row(
        ui,
        doc,
        requests,
        &["field-shading-always", "field-shading-selected", "field-shading-never"],
    );
    heading(ui, "Zoom");
    let mut percent = doc
        .view
        .fields
        .get("zoom")
        .cloned()
        .unwrap_or_else(|| format!("{:.0}", doc.view.zoom * 100.0));
    let mut set = false;
    ui.horizontal(|ui| {
        ui.label("Percent");
        set = ui.add(egui::TextEdit::singleline(&mut percent).desired_width(48.0)).lost_focus();
        set |= kit::button(ui, "Set").clicked();
    });
    doc.view.fields.insert("zoom", percent.clone());
    if set {
        if let Ok(value) = percent.trim().trim_end_matches('%').parse::<f32>() {
            doc.view.zoom = (value / 100.0).clamp(0.1, 5.0);
        }
        doc.view.fields.remove("zoom");
    }
    ui.horizontal_wrapped(|ui| {
        for value in [50.0, 75.0, 100.0, 150.0, 200.0] {
            if kit::button(ui, format!("{value}%")).clicked() {
                doc.view.zoom = value / 100.0;
                doc.view.multiple = false;
                doc.view.fields.remove("zoom");
            }
        }
    });
    actions_row(
        ui,
        doc,
        requests,
        &["fit-width", "fit-page", "multiple-pages", "zoom-out", "zoom-in"],
    );
    heading(ui, "Find");
    actions_row(ui, doc, requests, &["find", "replace"]);
    heading(ui, "Export");
    actions_row(ui, doc, requests, &["export-pdf", "export-html", "export-md", "export-txt"]);
}

fn element_key(element: &str) -> &'static str {
    match element {
        "dc:title" => "title",
        "dc:subject" => "subject",
        "dc:creator" => "author",
        _ => "keywords",
    }
}
