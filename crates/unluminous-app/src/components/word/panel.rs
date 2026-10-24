//! The drill down panel: one column at the right of the pages with a back arrow, a breadcrumb, and the
//! controls of one level. The root level lists the others, each with a line saying what is in it, so a
//! setting is two presses from the page and never more than three.

use egui::{Align2, FontId, Rect, RichText, Stroke, Vec2};
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

/// Draw the panel.
pub fn show(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    rect: Rect,
    requests: &mut Vec<Request>,
) {
    ui.painter().rect_filled(rect, 0.0, look.ground(look.palette.panel));
    ui.painter().line_segment(
        [rect.left_top(), rect.left_bottom()],
        Stroke::new(1.0, look.palette.divider),
    );
    let level = *doc.view.panel.last().unwrap_or(&Level::Root);
    let mut inner =
        ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(14.0, 10.0))));
    inner.set_clip_rect(rect);
    // The head: back, the breadcrumb, close.
    inner.horizontal(|ui| {
        if doc.view.panel.len() > 1 && ui.button("‹").on_hover_text("Back").clicked() {
            doc.view.panel.pop();
        }
        let crumbs: Vec<&str> = doc.view.panel.iter().map(|l| l.name()).collect();
        ui.label(
            RichText::new(crumbs.join("  ›  "))
                .color(look.palette.text_strong)
                .size(look.font_size * 1.05),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("✕").on_hover_text("Close the panel").clicked() {
                doc.view.panel.clear();
            }
        });
    });
    inner.add_space(6.0);
    egui::ScrollArea::vertical()
        .id_salt(("word-panel", level as u8))
        .auto_shrink([false, false])
        .show(&mut inner, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            match level {
                Level::Root => root(ui, look, doc),
                Level::Text => text(ui, look, doc, requests),
                Level::Paragraph => paragraph(ui, look, doc, requests),
                Level::Styles => styles(ui, look, doc, requests),
                Level::Lists => lists(ui, doc, requests),
                Level::Insert => insert(ui, doc, requests),
                Level::Page => page(ui, doc, requests),
                Level::References => references(ui, doc, requests),
                Level::Review => review(ui, look, doc, requests),
                Level::Document => document(ui, doc, requests),
            }
        });
}

/// One row of the root level: the level's name over what it holds, the whole row a button.
fn root(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc) {
    for level in Level::ALL {
        let height = 44.0 * look.scale();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), egui::Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, 6.0, look.palette.selected_row);
        }
        ui.painter().text(
            rect.left_top() + Vec2::new(10.0, 8.0),
            Align2::LEFT_TOP,
            level.name(),
            FontId::proportional(look.font_size),
            look.palette.text_strong,
        );
        ui.painter().text(
            rect.left_bottom() + Vec2::new(10.0, -8.0),
            Align2::LEFT_BOTTOM,
            level.summary(),
            FontId::proportional(look.font_size * 0.82),
            look.palette.text_dim,
        );
        ui.painter().text(
            rect.right_center() - Vec2::new(10.0, 0.0),
            Align2::RIGHT_CENTER,
            "›",
            FontId::proportional(look.font_size * 1.3),
            look.palette.text_dim,
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, level.name())
        });
        if response.clicked() {
            doc.view.panel.push(*level);
        }
    }
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).strong());
}

/// A row of buttons that each run an action from the shared list.
fn actions_row(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>, ids: &[&str]) {
    ui.horizontal_wrapped(|ui| {
        for id in ids {
            let Some(action) = crate::services::word::actions::find(id) else { continue };
            if ui.button(action.label).clicked() {
                super::run_action(doc, id, requests);
            }
        }
    });
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
            if ui.button(if id == "grow-font" { "A+" } else { "A−" }).clicked() {
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
    ui.horizontal_wrapped(|ui| {
        for (name, hex) in COLOURS {
            let colour = Rgb::parse(hex)
                .map_or(egui::Color32::from_gray(30), |c| egui::Color32::from_rgb(c.0, c.1, c.2));
            let button =
                egui::Button::new(RichText::new("■").color(colour).size(18.0)).frame(false);
            if ui.add(button).on_hover_text(*name).clicked() {
                let hex = hex.to_string();
                doc.session.format(move |rpr| props::set_val(rpr, "w:color", Some(&hex)));
            }
        }
    });
    heading(ui, "Highlight");
    ui.horizontal_wrapped(|ui| {
        if ui.button("None").clicked() {
            doc.session.format(|rpr| props::set_val(rpr, "w:highlight", None));
        }
        for (name, rgb) in props::HIGHLIGHTS.iter().take(12) {
            let colour = egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2);
            if ui
                .add(egui::Button::new(RichText::new("■").color(colour).size(18.0)).frame(false))
                .on_hover_text(*name)
                .clicked()
            {
                let name = name.to_string();
                doc.session.format(move |rpr| props::set_val(rpr, "w:highlight", Some(&name)));
            }
        }
    });
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
        if ui.checkbox(value, label).changed() {
            let on = *value;
            doc.session.format_paragraphs(move |ppr| props::set_on_off(ppr, element, Some(on)));
        }
    }
    heading(ui, "Borders and shading");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Box").clicked() {
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
        if ui.button("Line below").clicked() {
            super::run_action(doc, "horizontal-line", requests);
        }
        if ui.button("No border").clicked() {
            doc.session.format_paragraphs(|ppr| ppr.remove_children("w:pBdr"));
        }
        if ui.button("Shade").clicked() {
            doc.session.format_paragraphs(|ppr| {
                ppr.put(
                    unluminous_word::xml::El::new("w:shd")
                        .with("w:val", "clear")
                        .with("w:color", "auto")
                        .with("w:fill", "F2F2F2"),
                )
            });
        }
        if ui.button("No shading").clicked() {
            doc.session.format_paragraphs(|ppr| ppr.remove_children("w:shd"));
        }
    });
    heading(ui, "Tab stops");
    let field = doc.view.fields.entry("tab").or_default();
    let mut add = None;
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(field).hint_text("points, e.g. 216").desired_width(90.0));
        for (label, kind) in
            [("Left", "left"), ("Center", "center"), ("Right", "right"), ("Decimal", "decimal")]
        {
            if ui.button(label).clicked() {
                add = field.trim().parse::<f32>().ok().map(|p| (p, kind));
            }
        }
    });
    if let Some((points, kind)) = add {
        let pos = twips(points);
        doc.session.format_paragraphs(move |ppr| {
            let tabs = ppr.child_or_insert("w:tabs");
            tabs.children.push(unluminous_word::xml::Node::El(
                unluminous_word::xml::El::new("w:tab")
                    .with("w:val", kind)
                    .with("w:pos", pos.clone()),
            ));
        });
    }
    for stop in &p.tabs {
        ui.label(format!("{:?} at {} pt", stop.kind, stop.position));
    }
    if !p.tabs.is_empty() && ui.button("Clear tab stops").clicked() {
        doc.session.format_paragraphs(|ppr| ppr.remove_children("w:tabs"));
    }
}

fn styles(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, requests: &mut Vec<Request>) {
    let current = doc.session.paragraph_at_caret().style;
    heading(ui, "Paragraph styles");
    let listed = unluminous_word::styles::Resolver::new(&doc.session.document).listed("paragraph");
    for (id, name) in listed {
        if ui.selectable_label(current == id, RichText::new(&name).size(look.font_size)).clicked() {
            if let Err(problem) = doc.session.apply_style(&id) {
                requests.push(Request::Message(problem));
            }
        }
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
            if ui.button(&name).clicked() {
                let _ = doc.session.apply_style(&id);
            }
        }
    });
    heading(ui, "New style from the selection");
    let field = doc.view.fields.entry("style").or_default();
    let mut make = None;
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(field).hint_text("Name").desired_width(160.0));
        if ui.button("Create").clicked() && !field.trim().is_empty() {
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
        if ui.button("Restart here").clicked() {
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
        make = ui.button("Insert").clicked();
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
                if ui.button(label).clicked() {
                    if let Err(problem) = doc.session.table_op(op) {
                        requests.push(Request::Message(problem));
                    }
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Shade cell");
            for hex in ["D9E2F3", "FBE4D5", "E2EFD9", "FFF2CC", "EDEDED"] {
                let rgb = Rgb::parse(hex).expect("a fixed colour");
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("■")
                                .color(egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2))
                                .size(18.0),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    let _ = doc.session.shade_cell(Some(rgb));
                }
            }
            if ui.button("None").clicked() {
                let _ = doc.session.shade_cell(None);
            }
        });
    }
    heading(ui, "Picture");
    actions_row(ui, doc, requests, &["picture"]);
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
                .hint_text("https://… or #bookmark")
                .desired_width(190.0),
        );
        if (ui.button("Link").clicked()
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
    heading(ui, "Bookmark");
    let mark = doc.view.fields.entry("bookmark").or_default();
    let mut add_mark = None;
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(mark).hint_text("Name").desired_width(150.0));
        if ui.button("Add").clicked() && !mark.trim().is_empty() {
            add_mark = Some(mark.trim().replace(' ', "_"));
        }
    });
    if let Some(name) = add_mark {
        doc.session.insert_bookmark(&name);
        doc.view.fields.remove("bookmark");
    }
    heading(ui, "Symbol");
    ui.horizontal_wrapped(|ui| {
        for symbol in [
            "©", "®", "™", "§", "¶", "†", "‡", "•", "…", "—", "–", "€", "£", "¥", "°", "±", "×",
            "÷", "≠", "≤", "≥", "→", "←", "✓",
        ] {
            if ui.button(symbol).clicked() {
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
            if ui.button(name.to_uppercase()).clicked() {
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
        for (label, value) in ["Top", "Bottom", "Left", "Right"].iter().zip(margins.iter_mut()) {
            ui.label(*label);
            changed |= ui
                .add(egui::DragValue::new(value).range(0.0..=400.0).speed(0.5).suffix(" pt"))
                .changed();
        }
    });
    if changed {
        doc.session.set_margins(margins);
    }
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
                    .hint_text("Text, with {PAGE} and {NUMPAGES}")
                    .desired_width(170.0),
            );
            if ui.button("Set").clicked() {
                set = Some(field.clone());
            }
        });
        if let Some(text) = set {
            doc.session.set_header_text(footer, "default", &text);
        }
    }
    actions_row(ui, doc, requests, &["page-numbers"]);
    heading(ui, "Page colour");
    ui.horizontal_wrapped(|ui| {
        for hex in ["FFFFFF", "FFF8E7", "F2F2F2", "EAF1FB", "EEF7EE"] {
            let rgb = Rgb::parse(hex).expect("a fixed colour");
            if ui
                .add(
                    egui::Button::new(
                        RichText::new("■")
                            .color(egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2))
                            .size(18.0),
                    )
                    .frame(false),
                )
                .clicked()
            {
                let white = hex == "FFFFFF";
                let hex = hex.to_owned();
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
    });
}

fn references(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    heading(ui, "Table of contents");
    actions_row(ui, doc, requests, &["table-of-contents", "update-fields"]);
    heading(ui, "Notes");
    actions_row(ui, doc, requests, &["footnote", "endnote"]);
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

fn review(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, requests: &mut Vec<Request>) {
    heading(ui, "Track changes");
    let mut tracking = doc.session.tracking;
    if ui.checkbox(&mut tracking, "Track changes").changed() {
        doc.session.set_tracking(tracking);
    }
    actions_row(ui, doc, requests, &["accept-change", "reject-change", "accept-all", "reject-all"]);
    heading(ui, "Comments");
    let field = doc.view.fields.entry("comment").or_default();
    let mut add = None;
    ui.add(
        egui::TextEdit::multiline(field)
            .hint_text("A comment on the selection")
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
    if ui.button("Comment").clicked() && !field.trim().is_empty() {
        add = Some(field.trim().to_owned());
    }
    if let Some(text) = add {
        doc.session.add_comment(&text);
        doc.view.fields.remove("comment");
    }
    for comment in doc.session.comments() {
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
                .size(look.font_size * 0.85),
            );
            if let Some(on) = comment["on"].as_str().filter(|s| !s.is_empty()) {
                ui.label(RichText::new(format!("“{on}”")).italics().color(look.palette.text_dim));
            }
            ui.label(comment["text"].as_str().unwrap_or(""));
            ui.horizontal(|ui| {
                let resolved = comment["resolved"].as_bool().unwrap_or(false);
                if ui.button(if resolved { "Reopen" } else { "Resolve" }).clicked() {
                    let _ = doc.session.resolve_comment(id, !resolved);
                }
                if ui.button("Delete").clicked() {
                    let _ = doc.session.delete_comment(id);
                }
            });
        });
    }
    heading(ui, "Word count");
    let stats = doc.session.stats(Some(&doc.pages));
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

fn document(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
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
    heading(ui, "View");
    actions_row(
        ui,
        doc,
        requests,
        &["formatting-marks", "navigation", "zoom-out", "zoom-100", "zoom-in"],
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
