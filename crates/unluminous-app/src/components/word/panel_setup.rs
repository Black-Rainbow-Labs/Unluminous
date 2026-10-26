//! Third levels of the panel (task-2359, TDD 5.5): Page setup under Page, Modify Style under Styles,
//! Symbol under Insert and Note options under References.

use egui::RichText;
use unluminous_word::layout::section::Geometry;

use super::kit;
use super::panel::{actions_row, heading};
use crate::services::plugin_ui::{Look, Request};
use crate::services::word::Doc;

/// A number of points in a small field, answered when it changes.
fn points(ui: &mut egui::Ui, label: &str, value: f32) -> Option<f32> {
    let mut v = value;
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed = ui
            .add(egui::DragValue::new(&mut v).range(0.0..=1584.0).speed(0.5).suffix(" pt"))
            .changed();
    });
    changed.then_some(v)
}

/// Page setup: every margin, the gutter, the header and footer distances, mirrored margins, the paper,
/// vertical alignment, line numbers, hyphenation, the page border, the header and footer options and
/// page numbering.
pub fn page_setup(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    let _ = requests;
    let sect = doc.session.section_at_caret().unwrap_or_default();
    let geometry = Geometry::of(&sect);
    heading(ui, "Margins");
    let mut margins = [geometry.top, geometry.bottom, geometry.left, geometry.right];
    let (mut gutter, mut header, mut footer) = (geometry.gutter, geometry.header, geometry.footer);
    let mut changed = false;
    for (label, value) in
        ["Top", "Bottom", "Left (inside)", "Right (outside)"].iter().zip(margins.iter_mut())
    {
        if let Some(v) = points(ui, label, *value) {
            *value = v;
            changed = true;
        }
    }
    for (label, value) in [
        ("Gutter", &mut gutter),
        ("Header from edge", &mut header),
        ("Footer from edge", &mut footer),
    ] {
        if let Some(v) = points(ui, label, *value) {
            *value = v;
            changed = true;
        }
    }
    if changed {
        doc.session.set_custom_margins(margins, gutter, header, footer);
    }
    let mirrored = doc
        .session
        .document
        .settings
        .as_ref()
        .and_then(|s| unluminous_word::props::on_off(&s.root, "w:mirrorMargins"))
        .unwrap_or(false);
    let mut mirror = mirrored;
    if kit::check(ui, &mut mirror, "Mirror margins (inside and outside, for binding)").changed() {
        doc.session.set_mirror_margins(mirror);
    }
    heading(ui, "Paper");
    let (mut width, mut height) = (geometry.width, geometry.height);
    let w = points(ui, "Width", width);
    let h = points(ui, "Height", height);
    if w.is_some() || h.is_some() {
        width = w.unwrap_or(width);
        height = h.unwrap_or(height);
        doc.session.set_paper(width, height);
    }
    heading(ui, "Vertical alignment");
    ui.horizontal(|ui| {
        for (label, value) in
            [("Top", "top"), ("Center", "center"), ("Justified", "both"), ("Bottom", "bottom")]
        {
            if kit::toggle(ui, geometry.vertical_align == value, label).clicked() {
                let _ = doc.session.set_vertical_alignment(value);
            }
        }
    });
    heading(ui, "Line numbers");
    let numbers = geometry.line_numbers.clone();
    ui.horizontal_wrapped(|ui| {
        if kit::toggle(ui, numbers.is_none(), "None").clicked() {
            doc.session.set_line_numbers(None);
        }
        for (label, restart) in [
            ("Restart each page", "newPage"),
            ("Restart each section", "newSection"),
            ("Continuous", "continuous"),
        ] {
            let chosen = numbers.as_ref().is_some_and(|n| n.2 == restart);
            if kit::toggle(ui, chosen, label).clicked() {
                let (count_by, start, distance) =
                    numbers.as_ref().map_or((1, 1, 18.0), |n| (n.0, n.1, n.3));
                doc.session.set_line_numbers(Some((count_by, start, restart, distance)));
            }
        }
    });
    if let Some((count_by, start, restart, distance)) = numbers {
        let mut by = count_by as f32;
        let mut from = start as f32;
        let mut away = distance;
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Count by");
            changed |=
                ui.add(egui::DragValue::new(&mut by).range(1.0..=100.0).speed(0.2)).changed();
            ui.label("Start at");
            changed |=
                ui.add(egui::DragValue::new(&mut from).range(1.0..=32767.0).speed(0.2)).changed();
        });
        if let Some(v) = points(ui, "From text", away) {
            away = v;
            changed = true;
        }
        if changed {
            doc.session.set_line_numbers(Some((by as u32, from as u32, &restart, away)));
        }
    }
    heading(ui, "Hyphenation");
    let auto = doc
        .session
        .document
        .settings
        .as_ref()
        .and_then(|s| unluminous_word::props::on_off(&s.root, "w:autoHyphenation"))
        .unwrap_or(false);
    let mut hyphenate = auto;
    if kit::check(ui, &mut hyphenate, "Hyphenate automatically").changed() {
        doc.session.set_auto_hyphenation(hyphenate);
    }
    heading(ui, "Page border");
    ui.horizontal_wrapped(|ui| {
        if kit::toggle(ui, geometry.page_border.is_none(), "None").clicked() {
            doc.session.set_page_border(None);
        }
        for (label, width) in [("Thin box", 0.5), ("Box", 1.5), ("Thick box", 3.0)] {
            let chosen = geometry.page_border.is_some_and(|b| (b.0 - width).abs() < 0.2);
            if kit::toggle(ui, chosen, label).clicked() {
                let colour = doc
                    .view
                    .fields
                    .get("border-colour")
                    .cloned()
                    .unwrap_or_else(|| "000000".into());
                doc.session.set_page_border(Some((width, &colour)));
            }
        }
    });
    let mut colour = geometry
        .page_border
        .map_or(egui::Color32::BLACK, |b| egui::Color32::from_rgb(b.1 .0, b.1 .1, b.1 .2)); // any ground: a border colour, not a surface
    ui.horizontal(|ui| {
        ui.label("Border colour");
        if ui.color_edit_button_srgba(&mut colour).changed() {
            let hex = format!("{:02X}{:02X}{:02X}", colour.r(), colour.g(), colour.b());
            doc.view.fields.insert("border-colour", hex.clone());
            if let Some(border) = geometry.page_border {
                doc.session.set_page_border(Some((border.0, &hex)));
            }
        }
    });
    heading(ui, "Headers and footers");
    let mut first = geometry.title_page;
    if kit::check(ui, &mut first, "Different first page").changed() {
        doc.session.set_different_first_page(first);
    }
    let odd_even = doc
        .session
        .document
        .settings
        .as_ref()
        .and_then(|s| unluminous_word::props::on_off(&s.root, "w:evenAndOddHeaders"))
        .unwrap_or(false);
    let mut even = odd_even;
    if kit::check(ui, &mut even, "Different odd and even pages").changed() {
        doc.session.set_different_odd_even(even);
    }
    ui.horizontal_wrapped(|ui| {
        for (label, footer, link) in [
            ("Link header to previous", false, true),
            ("Unlink header", false, false),
            ("Link footer to previous", true, true),
            ("Unlink footer", true, false),
        ] {
            if kit::button(ui, label).clicked() {
                if let Err(problem) = doc.session.link_to_previous(footer, "default", link) {
                    requests.push(Request::Message(problem));
                }
            }
        }
    });
    heading(ui, "Page numbers");
    let format = geometry.page_number_format.clone();
    let mut chosen = if format.is_empty() { "decimal".to_owned() } else { format };
    egui::ComboBox::from_id_salt(("word-page-number-format", &doc.key))
        .selected_text(&chosen)
        .show_ui(ui, |ui| {
            crate::components::controls::set_in_the_list_size(ui);
            for (value, label) in [
                ("decimal", "1, 2, 3"),
                ("lowerRoman", "i, ii, iii"),
                ("upperRoman", "I, II, III"),
                ("lowerLetter", "a, b, c"),
                ("upperLetter", "A, B, C"),
            ] {
                ui.selectable_value(&mut chosen, value.to_owned(), label);
            }
        });
    let mut restart = geometry.page_number_start.is_some();
    let mut start = geometry.page_number_start.unwrap_or(1) as f32;
    let mut numbering_changed = chosen
        != if geometry.page_number_format.is_empty() {
            "decimal"
        } else {
            geometry.page_number_format.as_str()
        };
    ui.horizontal(|ui| {
        numbering_changed |= kit::check(ui, &mut restart, "Start at").changed();
        numbering_changed |= ui
            .add_enabled(restart, egui::DragValue::new(&mut start).range(0.0..=32767.0).speed(0.2))
            .changed();
    });
    if numbering_changed {
        doc.session.set_page_numbering(&chosen, restart.then_some(start as u32));
    }
}

/// Modify Style: the font, size, emphasis, colour, alignment, spacing, line spacing, indents, the style
/// it is based on and the one after it, for the style `doc.view.modify_style` names; and Update to
/// Match, Delete and Revert.
pub fn modify_style(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    requests: &mut Vec<Request>,
) {
    let id =
        doc.view.modify_style.clone().unwrap_or_else(|| doc.session.paragraph_at_caret().style);
    let resolver = unluminous_word::styles::Resolver::new(&doc.session.document);
    let name = resolver.name_of(&id);
    let paragraph_look = resolver.paragraph(
        &unluminous_word::xml::El::new("w:pPr")
            .with_child(unluminous_word::xml::El::new("w:pStyle").with("w:val", id.clone())),
        None,
    );
    let run_look = resolver.run(
        &unluminous_word::xml::El::new("w:pPr")
            .with_child(unluminous_word::xml::El::new("w:pStyle").with("w:val", id.clone())),
        &unluminous_word::xml::El::new("w:rPr"),
        None,
    );
    let styles = resolver.listed("paragraph");
    drop(resolver);
    ui.label(RichText::new(&name).size(15.0 * kit::zoom(ui.ctx())).color(kit::ink().i900));
    let mut set = |doc: &mut Doc, property: &str, value: String| {
        if let Err(problem) = doc.session.modify_style(&id, property, &value) {
            requests.push(Request::Message(problem));
        }
    };
    heading(ui, "Font");
    let mut family = run_look.font.to_string();
    egui::ComboBox::from_id_salt(("word-modify-font", &doc.key))
        .selected_text(&family)
        .width(ui.available_width() - 40.0)
        .show_ui(ui, |ui| {
            crate::components::controls::set_in_the_list_size(ui);
            for name in look.renderer.families() {
                ui.selectable_value(&mut family, name.clone(), name);
            }
        });
    if family != run_look.font.as_ref() {
        set(doc, "font", family);
    }
    ui.horizontal(|ui| {
        let mut size = run_look.size;
        ui.label("Size");
        if ui
            .add(egui::DragValue::new(&mut size).range(1.0..=1638.0).speed(0.5).suffix(" pt"))
            .changed()
        {
            set(doc, "size", format!("{size}"));
        }
    });
    ui.horizontal(|ui| {
        for (label, property, on) in
            [("Bold", "bold", run_look.bold), ("Italic", "italic", run_look.italic)]
        {
            if kit::toggle(ui, on, label).clicked() {
                set(doc, property, if on { "off" } else { "on" }.into());
            }
        }
        let underlined = run_look.underline != unluminous_word::props::Underline::None;
        if kit::toggle(ui, underlined, "Underline").clicked() {
            set(doc, "underline", if underlined { "off" } else { "on" }.into());
        }
    });
    let mut colour = run_look
        .color
        .map_or(crate::theme::color::word_ink(), |c| egui::Color32::from_rgb(c.0, c.1, c.2));
    ui.horizontal(|ui| {
        ui.label("Colour");
        if ui.color_edit_button_srgba(&mut colour).changed() {
            set(doc, "colour", format!("{:02X}{:02X}{:02X}", colour.r(), colour.g(), colour.b()));
        }
        if kit::button(ui, "Automatic").clicked() {
            set(doc, "colour", "auto".into());
        }
    });
    heading(ui, "Paragraph");
    ui.horizontal(|ui| {
        for (label, value) in
            [("Left", "left"), ("Center", "center"), ("Right", "right"), ("Justify", "justify")]
        {
            if kit::button(ui, label).clicked() {
                set(doc, "align", value.into());
            }
        }
    });
    for (label, property, value) in [
        ("Space before", "before", paragraph_look.space_before),
        ("Space after", "after", paragraph_look.space_after),
        ("Left indent", "indent-left", paragraph_look.indent_left),
        ("First line", "first-line", paragraph_look.indent_first.max(0.0)),
        ("Hanging", "hanging", (-paragraph_look.indent_first).max(0.0)),
    ] {
        if let Some(v) = points(ui, label, value) {
            set(doc, property, format!("{v}"));
        }
    }
    ui.horizontal(|ui| {
        ui.label("Line spacing");
        for (label, value) in [("1.0", "1"), ("1.15", "1.15"), ("1.5", "1.5"), ("2.0", "2")] {
            if kit::button(ui, label).clicked() {
                set(doc, "line", value.into());
            }
        }
    });
    heading(ui, "Based on and next");
    for (label, property) in [("Based on", "based-on"), ("Style for the next paragraph", "next")] {
        ui.horizontal(|ui| {
            ui.label(label);
            let mut chosen = String::new();
            egui::ComboBox::from_id_salt(("word-modify", property, &doc.key))
                .selected_text("Choose")
                .show_ui(ui, |ui| {
                    crate::components::controls::set_in_the_list_size(ui);
                    for (sid, sname) in &styles {
                        ui.selectable_value(&mut chosen, sid.clone(), sname);
                    }
                });
            if !chosen.is_empty() {
                set(doc, property, chosen);
            }
        });
    }
    heading(ui, "This style");
    ui.horizontal_wrapped(|ui| {
        if kit::button(ui, "Update to match the selection").clicked() {
            match doc.session.update_style_to_match() {
                Ok(id) => requests.push(Request::Message(format!("{id} updated to match"))),
                Err(problem) => requests.push(Request::Message(problem)),
            }
        }
        if kit::button(ui, "Revert to Word's own").clicked() {
            if let Err(problem) = doc.session.revert_style(&id) {
                requests.push(Request::Message(problem));
            }
        }
        if kit::button(ui, "Delete").clicked() {
            match doc.session.delete_style(&id) {
                Ok(()) => {
                    doc.view.modify_style = None;
                    doc.view.panel.pop();
                }
                Err(problem) => requests.push(Request::Message(problem)),
            }
        }
    });
}

/// The blocks of characters the Symbol level offers.
const BLOCKS: [(&str, u32, u32); 12] = [
    ("Latin-1 Supplement", 0x00A1, 0x00FF),
    ("Latin Extended-A", 0x0100, 0x017F),
    ("Greek", 0x0391, 0x03C9),
    ("Cyrillic", 0x0410, 0x044F),
    ("General Punctuation", 0x2010, 0x2044),
    ("Currency Symbols", 0x20A0, 0x20BF),
    ("Letterlike Symbols", 0x2100, 0x214F),
    ("Arrows", 0x2190, 0x21FF),
    ("Mathematical Operators", 0x2200, 0x22FF),
    ("Box Drawing", 0x2500, 0x257F),
    ("Geometric Shapes", 0x25A0, 0x25FF),
    ("Miscellaneous Symbols", 0x2600, 0x26FF),
];

/// Symbol: the characters of a block, the ones used lately, and Word's special characters.
pub fn symbols(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, requests: &mut Vec<Request>) {
    let _ = look;
    let font = doc.session.look_at_caret().font.to_string();
    ui.label(RichText::new(format!("In {font}")).weak());
    if !doc.view.recent_symbols.is_empty() {
        heading(ui, "Recently used");
        let recent = doc.view.recent_symbols.clone();
        ui.horizontal_wrapped(|ui| {
            for symbol in recent {
                if kit::button(ui, symbol.to_string()).clicked() {
                    insert_symbol(doc, symbol);
                }
            }
        });
    }
    heading(ui, "Special characters");
    actions_row(
        ui,
        doc,
        requests,
        &[
            "em-dash",
            "en-dash",
            "nonbreaking-space",
            "nonbreaking-hyphen",
            "optional-hyphen",
            "copyright",
            "registered",
            "trademark",
            "section-sign",
            "paragraph-sign",
            "ellipsis",
        ],
    );
    heading(ui, "Symbols");
    let block = doc
        .view
        .fields
        .get("symbol-block")
        .and_then(|b| b.parse::<usize>().ok())
        .unwrap_or(0)
        .min(BLOCKS.len() - 1);
    let mut chosen = block;
    egui::ComboBox::from_id_salt(("word-symbol-block", &doc.key))
        .selected_text(BLOCKS[block].0)
        .show_ui(ui, |ui| {
            crate::components::controls::set_in_the_list_size(ui);
            for (index, (name, _, _)) in BLOCKS.iter().enumerate() {
                ui.selectable_value(&mut chosen, index, *name);
            }
        });
    doc.view.fields.insert("symbol-block", chosen.to_string());
    let (_, from, to) = BLOCKS[chosen];
    let mut pressed = None;
    ui.horizontal_wrapped(|ui| {
        for code in from..=to {
            let Some(character) = char::from_u32(code) else { continue };
            let button = egui::Button::new(
                RichText::new(character.to_string()).size(15.0 * kit::zoom(ui.ctx())),
            )
            .min_size(egui::vec2(26.0, 26.0));
            let response = ui.add(button);
            if response.clicked() {
                pressed = Some(character);
            }
        }
    });
    if let Some(character) = pressed {
        insert_symbol(doc, character);
    }
}

fn insert_symbol(doc: &mut Doc, character: char) {
    doc.session.type_text(&character.to_string());
    doc.view.recent_symbols.retain(|c| *c != character);
    doc.view.recent_symbols.insert(0, character);
    doc.view.recent_symbols.truncate(16);
}

/// Footnote and endnote options: the number format, where numbering starts and restarts, and where the
/// notes go.
pub fn note_options(ui: &mut egui::Ui, doc: &mut Doc, requests: &mut Vec<Request>) {
    for (endnote, title) in [(false, "Footnotes"), (true, "Endnotes")] {
        heading(ui, title);
        let key = if endnote { "endnote" } else { "footnote" };
        let field = |doc: &Doc, name: &str, fallback: &str| {
            doc.view.fields.get(name).cloned().unwrap_or_else(|| fallback.to_owned())
        };
        let mut format = field(
            doc,
            if endnote { "endnote-format" } else { "footnote-format" },
            if endnote { "lowerRoman" } else { "decimal" },
        );
        let mut restart =
            field(doc, if endnote { "endnote-restart" } else { "footnote-restart" }, "continuous");
        let mut place = field(
            doc,
            if endnote { "endnote-place" } else { "footnote-place" },
            if endnote { "docEnd" } else { "pageBottom" },
        );
        egui::ComboBox::from_id_salt(("word-note-format", key, &doc.key))
            .selected_text(&format)
            .show_ui(ui, |ui| {
                crate::components::controls::set_in_the_list_size(ui);
                for (value, label) in [
                    ("decimal", "1, 2, 3"),
                    ("lowerRoman", "i, ii, iii"),
                    ("upperRoman", "I, II, III"),
                    ("lowerLetter", "a, b, c"),
                    ("upperLetter", "A, B, C"),
                    ("chicago", "*, †, ‡"),
                ] {
                    ui.selectable_value(&mut format, value.to_owned(), label);
                }
            });
        let restarts: &[(&str, &str)] = if endnote {
            &[("continuous", "Continuous"), ("eachSect", "Restart each section")]
        } else {
            &[
                ("continuous", "Continuous"),
                ("eachSect", "Restart each section"),
                ("eachPage", "Restart each page"),
            ]
        };
        ui.horizontal_wrapped(|ui| {
            for (value, label) in restarts {
                if kit::toggle(ui, restart == *value, *label).clicked() {
                    restart = (*value).to_owned();
                }
            }
        });
        let places: &[(&str, &str)] = if endnote {
            &[("docEnd", "End of document"), ("sectEnd", "End of section")]
        } else {
            &[("pageBottom", "Bottom of page"), ("beneathText", "Below text")]
        };
        ui.horizontal_wrapped(|ui| {
            for (value, label) in places {
                if kit::toggle(ui, place == *value, *label).clicked() {
                    place = (*value).to_owned();
                }
            }
        });
        doc.view
            .fields
            .insert(if endnote { "endnote-format" } else { "footnote-format" }, format.clone());
        doc.view
            .fields
            .insert(if endnote { "endnote-restart" } else { "footnote-restart" }, restart.clone());
        doc.view
            .fields
            .insert(if endnote { "endnote-place" } else { "footnote-place" }, place.clone());
        if kit::button(ui, format!("Apply to {}", title.to_lowercase())).clicked() {
            doc.session.set_note_options(endnote, &format, 1, &restart, &place);
            requests.push(Request::Message(format!("{title} options set")));
        }
    }
    heading(ui, "Go to");
    actions_row(ui, doc, requests, &["next-footnote", "previous-footnote"]);
}
