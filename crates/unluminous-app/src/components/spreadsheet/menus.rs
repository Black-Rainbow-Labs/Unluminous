//! The menus of a spreadsheet tab, drawn from `services::spreadsheet::catalogue`: the right click menu,
//! the workbook menu with its search, a sheet tab's menu, the Format panel's levels, the box that asks for a
//! value, the find bar, a filter button's list and a cell's list of allowed values.
//!
//! Every entry answers the `plugins run spreadsheet` words it runs, which the tab then runs through the
//! same function an agent's command reaches.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use unluminous_sheet::styles::Color as SheetColor;

use crate::components::controls::{self, TextBoxes};
use crate::services::plugin_ui::Look;
use crate::services::spreadsheet::catalogue::{self, Item};
use crate::services::spreadsheet::view::Prompt;
use crate::theme::icon;

/// What a press in a menu asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Picked {
    /// Run these words.
    Words(Vec<String>),
    /// Ask for a value first.
    Ask(Prompt),
    /// Go a level down in the Format panel.
    Enter(String),
    /// The menu should close without doing anything.
    Close,
}

fn ask_of(item: &Item) -> Option<Prompt> {
    match item {
        Item::Ask { title, field, words, second, initial, .. } => Some(Prompt {
            verb: words.clone(),
            title: title.clone(),
            label: field.clone(),
            text: initial.clone(),
            second: second.clone().map(|label| (label, String::new())),
            focus: true,
            fields: Vec::new(),
        }),
        Item::Form { title, words, fields, .. } => {
            Some(Prompt::form(words.clone(), title.clone(), fields.clone()))
        }
        _ => None,
    }
}

/// A colour from `#RRGGBB`.
pub fn hex_colour(hex: &str) -> Color32 {
    match SheetColor::parse_hex(hex).and_then(|c| c.resolve(&[])) {
        Some((r, g, b)) => Color32::from_rgb(r, g, b),
        None => Color32::TRANSPARENT,
    }
}

/// A grid of colour swatches, answering the words for the one pressed.
fn swatch_grid(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    words: &[String],
    none: Option<&str>,
    name: &str,
) -> Option<Picked> {
    let mut picked = None;
    let side = 18.0;
    let gap = 4.0;
    let per_row = 10;
    let hexes = catalogue::swatches();
    let rows = hexes.len().div_ceil(per_row);
    let width = per_row as f32 * (side + gap);
    let (area, _) = ui.allocate_exact_size(
        Vec2::new(width.max(ui.available_width()), rows as f32 * (side + gap) + 4.0),
        Sense::hover(),
    );
    for (i, hex) in hexes.iter().enumerate() {
        let (r, c) = (i / per_row, i % per_row);
        let rect = Rect::from_min_size(
            area.min + Vec2::new(c as f32 * (side + gap) + 6.0, r as f32 * (side + gap) + 2.0),
            Vec2::splat(side),
        );
        let response = ui.interact(rect, ui.id().with(("swatch", name, i)), Sense::click());
        ui.painter().rect(
            rect,
            CornerRadius::same(4),
            hex_colour(hex),
            Stroke::new(1.0, look.palette.control_border),
            egui::StrokeKind::Inside,
        );
        if response.hovered() {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                CornerRadius::same(5),
                Stroke::new(2.0, look.palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        let response = controls::WithHint::with_hint(response, *hex);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{name} {hex}"))
        });
        if response.clicked() {
            let mut w = words.to_vec();
            w.push((*hex).to_owned());
            picked = Some(Picked::Words(w));
        }
    }
    if let Some(none) = none {
        let label = if none == "auto" { "Automatic" } else { "None" };
        if controls::menu_row(ui, label, "", true, false, 0.0) {
            let mut w = words.to_vec();
            w.push(none.to_owned());
            picked = Some(Picked::Words(w));
        }
    }
    picked
}

/// A row that opens a level of its own, with a chevron at its right.
fn sub_row(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    label: &str,
    open: bool,
    height: f32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    if response.hovered() || open {
        ui.painter().rect_filled(rect, CornerRadius::same(10), look.palette.control);
    }
    ui.painter().text(
        rect.left_center() + Vec2::new(36.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.5),
        look.palette.text_strong,
    );
    let c = Pos2::new(rect.right() - 14.0, rect.center().y);
    let s = 3.5;
    ui.painter().line_segment(
        [c + Vec2::new(-s / 2.0, -s), c + Vec2::new(s / 2.0, 0.0)],
        Stroke::new(1.5, look.palette.text_dim),
    );
    ui.painter().line_segment(
        [c + Vec2::new(s / 2.0, 0.0), c + Vec2::new(-s / 2.0, s)],
        Stroke::new(1.5, look.palette.text_dim),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    response
}

fn line(ui: &mut egui::Ui, look: &Look<'_>) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 9.0), Sense::hover());
    ui.painter().line_segment(
        [
            Pos2::new(rect.left() + 8.0, rect.center().y),
            Pos2::new(rect.right() - 8.0, rect.center().y),
        ],
        Stroke::new(1.0, look.palette.divider),
    );
}

/// One level of a popup menu, with its submenus opening to the side on hover.
fn popup_level(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    items: &[Item],
    id: egui::Id,
    depth: usize,
    drawn: &mut Vec<Rect>,
) -> Option<Picked> {
    let mut picked = None;
    let open_id = id.with(("open", depth));
    let mut open: Option<usize> = ui.ctx().data(|d| d.get_temp(open_id)).flatten();
    let mut open_rect = None;
    for (i, item) in items.iter().enumerate() {
        let top = ui.cursor().top();
        let hovered_row = |ui: &egui::Ui| {
            let bottom = ui.cursor().top();
            ui.ctx().pointer_hover_pos().is_some_and(|p| {
                p.y >= top && p.y < bottom && ui.max_rect().x_range().contains(p.x)
            })
        };
        match item {
            Item::Run { label, keys, words, checked } => {
                if controls::menu_row(ui, label, keys, true, *checked, 0.0) {
                    picked = Some(Picked::Words(words.clone()));
                }
                if hovered_row(ui) {
                    open = None;
                }
            }
            Item::Ask { label, .. } | Item::Form { label, .. } => {
                if controls::menu_row(ui, label, "", true, false, 0.0) {
                    picked = ask_of(item).map(Picked::Ask);
                }
                if hovered_row(ui) {
                    open = None;
                }
            }
            Item::Sub { label, .. } => {
                let response = sub_row(ui, look, label, open == Some(i), controls::MENU_ROW);
                if response.hovered() || response.clicked() {
                    open = Some(i);
                }
                if open == Some(i) {
                    open_rect = Some(response.rect);
                }
            }
            Item::Colors { label, words, none } => {
                controls::menu_heading(ui, label, 0.0);
                if let Some(p) = swatch_grid(ui, look, words, none.as_deref(), label) {
                    picked = Some(p);
                }
            }
            Item::Line => line(ui, look),
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(open_id, open));
    if let (Some(index), Some(row)) = (open, open_rect) {
        if let Some(Item::Sub { items: inner, label }) = items.get(index) {
            let shown = egui::Area::new(id.with(("flyout", depth, label.as_str())))
                .order(egui::Order::Foreground)
                .fixed_pos(row.right_top() + Vec2::new(4.0, -8.0))
                .show(ui.ctx(), |ui| {
                    controls::menu_frame(ui)
                        .show(ui, |ui| {
                            ui.set_width(250.0);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            popup_level(ui, look, inner, id.with(label.as_str()), depth + 1, drawn)
                        })
                        .inner
                });
            drawn.push(shown.response.rect);
            if shown.inner.is_some() {
                picked = shown.inner;
            }
        }
    }
    picked
}

/// The rows a command search found, each with the path to it. Answers what was pressed.
fn found_rows(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    everything: &[Item],
    search: &str,
) -> Option<Picked> {
    let mut picked = None;
    for (path, item) in catalogue::search(everything, search) {
        match &item {
            Item::Run { words, keys, checked, .. }
                if controls::menu_row(ui, &path, keys, true, *checked, 0.0) =>
            {
                picked = Some(Picked::Words(words.clone()));
            }
            Item::Ask { .. } | Item::Form { .. }
                if controls::menu_row(ui, &path, "", true, false, 0.0) =>
            {
                picked = ask_of(&item).map(Picked::Ask);
            }
            Item::Colors { words, none, label } => {
                controls::menu_heading(ui, &path, 0.0);
                if let Some(p) = swatch_grid(ui, look, words, none.as_deref(), label) {
                    picked = Some(p);
                }
            }
            _ => {}
        }
    }
    picked
}

/// A popup menu of items at a point. Answers what was picked, or `Close` when a press landed outside it.
pub fn popup(
    ctx: &egui::Context,
    look: &Look<'_>,
    id: egui::Id,
    at: Pos2,
    items: &[Item],
    width: f32,
) -> Option<Picked> {
    let mut drawn = Vec::new();
    let screen = ctx.content_rect();
    // Kept on the screen: a menu opened near the bottom opens upwards as far as it has to.
    let wanted = items.len() as f32 * controls::MENU_ROW + 16.0;
    let at = Pos2::new(
        at.x.min(screen.right() - width - 8.0),
        at.y.min((screen.bottom() - wanted - 8.0).max(screen.top() + 8.0)),
    );
    let shown = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        controls::menu_frame(ui)
            .show(ui, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 0.0;
                popup_level(ui, look, items, id, 0, &mut drawn)
            })
            .inner
    });
    drawn.push(shown.response.rect);
    if shown.inner.is_some() {
        return shown.inner;
    }
    let pressed_outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer.interact_pos().is_some_and(|p| !drawn.iter().any(|r| r.contains(p)))
    });
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    (pressed_outside || escape).then_some(Picked::Close)
}

/// The workbook menu: a search box over the whole tree, then the tree's first level.
pub fn book_menu(
    ctx: &egui::Context,
    look: &Look<'_>,
    id: egui::Id,
    at: Pos2,
    items: &[Item],
    search: &mut String,
) -> Option<Picked> {
    let mut drawn = Vec::new();
    let width = 290.0;
    let screen = ctx.content_rect();
    let at = Pos2::new(at.x.min(screen.right() - width - 12.0), at.y);
    let shown = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        controls::menu_frame(ui)
            .show(ui, |ui| {
                ui.set_width(width);
                let (field, _) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::hover());
                let response = controls::search_field(
                    ui,
                    field.shrink2(Vec2::new(2.0, 2.0)),
                    "Search commands",
                    "Search commands",
                    search,
                );
                // The search box has the keyboard while the menu is open, unless another box took it.
                if !response.has_focus() && !ui.ctx().text_edit_focused() {
                    response.request_focus();
                }
                ui.add_space(4.0);
                if search.trim().is_empty() {
                    popup_level(ui, look, items, id, 0, &mut drawn)
                } else {
                    let mut picked = None;
                    if catalogue::search(items, search).is_empty() {
                        ui.label(
                            egui::RichText::new("Nothing matches").color(look.palette.text_dim),
                        );
                    }
                    if let Some(p) = found_rows(ui, look, items, search) {
                        picked = Some(p);
                    }
                    // Enter runs the first match.
                    if picked.is_none() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Some((_, Item::Run { words, .. })) =
                            catalogue::search(items, search).first()
                        {
                            picked = Some(Picked::Words(words.clone()));
                        }
                    }
                    picked
                }
            })
            .inner
    });
    drawn.push(shown.response.rect);
    if shown.inner.is_some() {
        return shown.inner;
    }
    let pressed_outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer.interact_pos().is_some_and(|p| !drawn.iter().any(|r| r.contains(p)))
    });
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    (pressed_outside || escape).then_some(Picked::Close)
}

#[allow(clippy::too_many_arguments)]
/// The Format panel: the level at `path`, with a way back up and the path as its title.
pub fn panel(
    ui: &mut egui::Ui,
    area: Rect,
    look: &Look<'_>,
    root: &[Item],
    path: &[String],
    what: &str,
    details: &[(&str, String)],
    everything: &[Item],
) -> Option<Picked> {
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 0, look.ground(look.palette.panel));
    painter.line_segment(
        [area.left_top(), area.left_bottom()],
        Stroke::new(1.0, look.palette.divider),
    );
    let mut picked = None;
    let header = Rect::from_min_size(area.min, Vec2::new(area.width(), 40.0));
    let mut child =
        ui.new_child(egui::UiBuilder::new().max_rect(area.shrink2(Vec2::new(8.0, 0.0))));
    // The back button, or the close button at the top level.
    let back = Rect::from_center_size(
        Pos2::new(header.left() + 22.0, header.center().y),
        Vec2::splat(26.0),
    );
    if path.is_empty() {
        if controls::icon_button(&mut child, back, "Close the Format panel", icon::cross) {
            picked = Some(Picked::Close);
        }
    } else {
        let response = child.interact(back, child.id().with("panel-back"), Sense::click());
        if response.hovered() {
            child.painter().rect_filled(back, CornerRadius::same(6), look.palette.control);
        }
        let c = back.center();
        child.painter().line_segment(
            [c + Vec2::new(2.0, -4.5), c + Vec2::new(-2.5, 0.0)],
            Stroke::new(1.6, look.palette.text_dim),
        );
        child.painter().line_segment(
            [c + Vec2::new(-2.5, 0.0), c + Vec2::new(2.0, 4.5)],
            Stroke::new(1.6, look.palette.text_dim),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Back"));
        if response.clicked() {
            picked = Some(Picked::Enter("..".to_owned()));
        }
    }
    let title = if path.is_empty() { "Format".to_owned() } else { path.join(" / ") };
    painter.text(
        Pos2::new(header.left() + 44.0, header.center().y - 6.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(13.5),
        look.palette.text_strong,
    );
    painter.text(
        Pos2::new(header.left() + 44.0, header.center().y + 9.0),
        egui::Align2::LEFT_CENTER,
        what,
        egui::FontId::proportional(11.0),
        look.palette.text_dim,
    );
    let level = catalogue::at_path(root, path).unwrap_or(root);
    let mut top = header.bottom() + 4.0;
    // At the first level, a box that finds any command by name.
    let search_id = egui::Id::new("spreadsheet-panel-search");
    let mut search: String = ui.ctx().data(|d| d.get_temp(search_id)).unwrap_or_default();
    if path.is_empty() {
        let field = Rect::from_min_size(
            Pos2::new(area.left() + 10.0, top),
            Vec2::new(area.width() - 20.0, 28.0),
        );
        controls::search_field(ui, field, "Find a command", "Find a command", &mut search);
        top = field.bottom() + 6.0;
    }
    ui.ctx().data_mut(|d| d.insert_temp(search_id, search.clone()));
    let body = Rect::from_min_max(
        Pos2::new(area.left() + 6.0, top),
        Pos2::new(area.right() - 6.0, area.bottom()),
    );
    if path.is_empty() && !search.trim().is_empty() {
        let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
        egui::ScrollArea::vertical().id_salt("spreadsheet-panel-found").show(&mut body_ui, |ui| {
            ui.set_width((body.width() - 4.0).max(0.0));
            ui.spacing_mut().item_spacing.y = 2.0;
            if let Some(p) = found_rows(ui, look, everything, &search) {
                picked = Some(p);
            }
        });
        return picked;
    }
    let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
    egui::ScrollArea::vertical().id_salt(("spreadsheet-panel", path.join("/"))).show(
        &mut body_ui,
        |ui| {
            ui.set_width((body.width() - 4.0).max(0.0));
            ui.spacing_mut().item_spacing.y = 2.0;
            for item in level {
                match item {
                    Item::Run { label, keys, words, checked } => {
                        if controls::menu_row(ui, label, keys, true, *checked, 0.0) {
                            picked = Some(Picked::Words(words.clone()));
                        }
                    }
                    Item::Ask { label, .. } | Item::Form { label, .. } => {
                        if controls::menu_row(ui, label, "", true, false, 0.0) {
                            picked = ask_of(item).map(Picked::Ask);
                        }
                    }
                    Item::Sub { label, .. } => {
                        let response = sub_row(ui, look, label, false, 32.0);
                        // The current value for the chosen cells, beside the level's name.
                        if let Some((_, detail)) =
                            details.iter().find(|(name, _)| *name == label.as_str())
                        {
                            let shown = controls::truncate_chars(detail, 22, 20);
                            ui.painter().text(
                                Pos2::new(response.rect.right() - 26.0, response.rect.center().y),
                                egui::Align2::RIGHT_CENTER,
                                shown,
                                egui::FontId::proportional(11.0),
                                look.palette.text_dim,
                            );
                        }
                        if response.clicked() {
                            picked = Some(Picked::Enter(label.clone()));
                        }
                    }
                    Item::Colors { label, words, none } => {
                        controls::menu_heading(ui, label, 0.0);
                        if let Some(p) = swatch_grid(ui, look, words, none.as_deref(), label) {
                            picked = Some(p);
                        }
                    }
                    Item::Line => line(ui, look),
                }
            }
        },
    );
    picked
}

/// What the box asking for a value answered.
pub enum Answered {
    Ok(Vec<String>),
    Cancel,
}

/// The box asking for a value, over the top of the grid.
pub fn prompt_box(
    ctx: &egui::Context,
    look: &Look<'_>,
    id: egui::Id,
    at: Pos2,
    prompt: &mut Prompt,
) -> Option<Answered> {
    let mut answer = None;
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        controls::menu_frame(ui).show(ui, |ui| {
            ui.set_width(340.0);
            ui.label(
                egui::RichText::new(&prompt.title)
                    .strong()
                    .color(look.palette.text_strong)
                    .size(13.5),
            );
            ui.add_space(6.0);
            let font = egui::FontId::proportional(13.0);
            if !prompt.fields.is_empty() {
                form_fields(ui, look, id, prompt, &font);
            } else {
                ui.label(
                    egui::RichText::new(&prompt.label).color(look.palette.text_dim).size(11.5),
                );
                let first = ui.text_box(
                    egui::TextEdit::singleline(&mut prompt.text)
                        .id(id.with("first"))
                        .font(font.clone())
                        .desired_width(320.0)
                        .hint_text(controls::placeholder(
                            &prompt.label,
                            &font,
                            look.palette.text_faint,
                        )),
                );
                first.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::TextEdit,
                        true,
                        prompt.label.clone(),
                    )
                });
                if prompt.focus {
                    first.request_focus();
                    prompt.focus = false;
                }
                if let Some((label, text)) = prompt.second.as_mut() {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(label.as_str()).color(look.palette.text_dim).size(11.5),
                    );
                    let second = ui.text_box(
                        egui::TextEdit::singleline(text)
                            .id(id.with("second"))
                            .font(font.clone())
                            .desired_width(320.0),
                    );
                    let name = label.clone();
                    second.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, name.clone())
                    });
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ok = ui.button("OK");
                ok.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "OK"));
                let cancel = ui.button("Cancel");
                cancel.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Cancel")
                });
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if ok.clicked() || enter {
                    answer = Some(Answered::Ok(prompt.answer()));
                }
                if cancel.clicked() || escape {
                    answer = Some(Answered::Cancel);
                }
            });
        });
    });
    answer
}

/// The fields of a larger box: a label over each text field, a row of choices, a switch.
fn form_fields(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    id: egui::Id,
    prompt: &mut Prompt,
    font: &egui::FontId,
) {
    use crate::services::spreadsheet::view::FieldKind;
    let focus = std::mem::take(&mut prompt.focus);
    let mut first_text = true;
    for (i, field) in prompt.fields.iter_mut().enumerate() {
        ui.add_space(3.0);
        match &field.kind {
            FieldKind::Text => {
                ui.label(egui::RichText::new(&field.label).color(look.palette.text_dim).size(11.5));
                let response = ui.text_box(
                    egui::TextEdit::singleline(&mut field.value)
                        .id(id.with(("field", i)))
                        .font(font.clone())
                        .desired_width(320.0),
                );
                let name = field.label.clone();
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, name.clone())
                });
                if focus && first_text {
                    response.request_focus();
                }
                first_text = false;
            }
            FieldKind::Choice(options) => {
                ui.label(egui::RichText::new(&field.label).color(look.palette.text_dim).size(11.5));
                let options = options.clone();
                ui.horizontal_wrapped(|ui| {
                    for (label, word) in &options {
                        let on = field.value == *word;
                        let response = ui.selectable_label(on, label.as_str());
                        let name = format!("{}: {label}", field.label);
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::RadioButton,
                                true,
                                on,
                                name.clone(),
                            )
                        });
                        if response.clicked() {
                            field.value = word.clone();
                        }
                    }
                });
            }
            FieldKind::Check => {
                let mut on = field.is_on();
                let response = ui.checkbox(&mut on, field.label.as_str());
                if response.changed() {
                    field.value = if on { "on".to_owned() } else { String::new() };
                }
            }
        }
    }
}

/// What the find bar asked for.
pub enum Found {
    Next(bool),
    Replace,
    ReplaceAll,
    Close,
}

/// The find bar along the top of the grid, with the replace field when Ctrl+H opened it.
pub fn find_bar(
    ui: &mut egui::Ui,
    area: Rect,
    look: &Look<'_>,
    find: &mut crate::services::spreadsheet::view::Find,
    count: usize,
) -> Option<Found> {
    let mut out = None;
    let painter = ui.painter_at(area);
    painter.rect(
        area,
        CornerRadius::same(10),
        look.palette.menu,
        Stroke::new(1.0, look.palette.control_border),
        egui::StrokeKind::Inside,
    );
    let mut row =
        Rect::from_min_size(area.min + Vec2::new(8.0, 6.0), Vec2::new(area.width() - 16.0, 26.0));
    let field = Rect::from_min_size(row.min, Vec2::new(row.width() - 150.0, 26.0));
    let response = controls::search_field(ui, field, "Find in sheet", "Find", &mut find.text);
    if find.focus {
        response.request_focus();
        find.focus = false;
    }
    if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        out = Some(Found::Next(!ui.input(|i| i.modifiers.shift)));
    }
    let label = if find.text.is_empty() {
        String::new()
    } else if count == 0 {
        "No results".to_owned()
    } else {
        format!("{} of {count}", find.at.min(count - 1) + 1)
    };
    painter.text(
        Pos2::new(field.right() + 8.0, row.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(11.5),
        look.palette.text_dim,
    );
    let buttons = [("Previous match", 0), ("Next match", 1), ("Close find", 2)];
    for (name, i) in buttons {
        let rect = Rect::from_center_size(
            Pos2::new(row.right() - 64.0 + i as f32 * 26.0, row.center().y),
            Vec2::splat(22.0),
        );
        let pressed = match i {
            0 => controls::icon_button(ui, rect, name, icon::chevron_up),
            1 => controls::icon_button(ui, rect, name, icon::chevron_down),
            _ => controls::icon_button(ui, rect, name, icon::cross),
        };
        if pressed {
            out = Some(match i {
                0 => Found::Next(false),
                1 => Found::Next(true),
                _ => Found::Close,
            });
        }
    }
    if let Some(replace) = find.replace.as_mut() {
        row = row.translate(Vec2::new(0.0, 32.0));
        let field = Rect::from_min_size(row.min, Vec2::new(row.width() - 150.0, 26.0));
        let font = egui::FontId::proportional(12.5);
        let response = ui.text_box_at(
            field,
            egui::TextEdit::singleline(replace)
                .id(ui.id().with("replace-with"))
                .font(font.clone())
                .hint_text(controls::placeholder("Replace with", &font, look.palette.text_faint)),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Replace with")
        });
        let one =
            Rect::from_min_size(Pos2::new(field.right() + 6.0, row.top()), Vec2::new(64.0, 26.0));
        let all =
            Rect::from_min_size(Pos2::new(one.right() + 6.0, row.top()), Vec2::new(70.0, 26.0));
        if controls::bar_button(ui, one, "Replace", false).clicked() {
            out = Some(Found::Replace);
        }
        if controls::bar_button(ui, all, "Replace all", false).clicked() {
            out = Some(Found::ReplaceAll);
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        out = Some(Found::Close);
    }
    out
}

/// A list of words to pick one of, under a cell: a list rule's choices or a filter's values.
pub fn choices(
    ctx: &egui::Context,
    id: egui::Id,
    at: Pos2,
    width: f32,
    values: &[String],
) -> Option<Option<String>> {
    let mut picked = None;
    let mut drawn = Vec::new();
    let shown = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        controls::menu_frame(ui).show(ui, |ui| {
            ui.set_width(width.max(140.0));
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for value in values {
                    if controls::menu_row(ui, value, "", true, false, 0.0) {
                        picked = Some(Some(value.clone()));
                    }
                }
            });
        });
    });
    drawn.push(shown.response.rect);
    let pressed_outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer.interact_pos().is_some_and(|p| !drawn.iter().any(|r| r.contains(p)))
    });
    if picked.is_none() && (pressed_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape))) {
        return Some(None);
    }
    picked
}

/// The list under a filter button: sorting, and the values to keep, each with a tick box.
pub fn filter_menu(
    ctx: &egui::Context,
    look: &Look<'_>,
    id: egui::Id,
    at: Pos2,
    values: &mut [(String, bool)],
    column: &str,
) -> Option<Picked> {
    let mut picked = None;
    let mut drawn = Vec::new();
    let shown = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        controls::menu_frame(ui).show(ui, |ui| {
            ui.set_width(240.0);
            if controls::menu_row(ui, "Sort A to Z", "", true, false, 0.0) {
                picked = Some(Picked::Words(vec![
                    "sort".to_owned(),
                    column.to_owned(),
                    "asc".to_owned(),
                ]));
            }
            if controls::menu_row(ui, "Sort Z to A", "", true, false, 0.0) {
                picked = Some(Picked::Words(vec![
                    "sort".to_owned(),
                    column.to_owned(),
                    "desc".to_owned(),
                ]));
            }
            if controls::menu_row(ui, "Show every row", "", true, false, 0.0) {
                picked = Some(Picked::Words(vec![
                    "filter".to_owned(),
                    column.to_owned(),
                    "clear".to_owned(),
                ]));
            }
            line(ui, look);
            let all = values.iter().all(|(_, keep)| *keep);
            if controls::menu_row(ui, "(Select all)", "", true, all, 0.0) {
                for (_, keep) in values.iter_mut() {
                    *keep = !all;
                }
            }
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for (value, keep) in values.iter_mut() {
                    let shown = if value.is_empty() { "(blank)".to_owned() } else { value.clone() };
                    if controls::menu_row(ui, &shown, "", true, *keep, 0.0) {
                        *keep = !*keep;
                    }
                }
            });
            ui.add_space(6.0);
            let ok = ui.button("Apply");
            ok.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Apply filter")
            });
            if ok.clicked() {
                let kept: Vec<String> = values
                    .iter()
                    .filter(|(_, k)| *k)
                    .map(|(v, _)| if v.is_empty() { "(blank)".to_owned() } else { v.clone() })
                    .collect();
                picked = Some(if kept.len() == values.len() {
                    Picked::Words(vec!["filter".to_owned(), column.to_owned(), "clear".to_owned()])
                } else {
                    Picked::Words(vec!["filter".to_owned(), column.to_owned(), kept.join(",")])
                });
            }
        });
    });
    drawn.push(shown.response.rect);
    if picked.is_some() {
        return picked;
    }
    let pressed_outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer.interact_pos().is_some_and(|p| !drawn.iter().any(|r| r.contains(p)))
    });
    (pressed_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape))).then_some(Picked::Close)
}
