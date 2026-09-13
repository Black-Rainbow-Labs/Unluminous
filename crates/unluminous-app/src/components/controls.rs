//! The small controls more than one part of the window needs: a dropdown, a menu row, an icon button
//! and a divider.
//!
//! They live here rather than in the toolbar because the toolbar is no longer the only thing that needs
//! them: the Settings window has dropdowns, and the menu bar has menu rows. One copy means the dropdown
//! in Settings and the dropdown in the toolbar cannot drift apart.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use crate::app::actions::{Action, Entry};
use crate::theme::{color, icon, size};

/// The rectangle the `TextEdit` inside one of Unluminous's fields is given.
///
/// Every field in Unluminous draws its own frame — `FIELD` with a one point stroke, the corner radius the
/// style guide gives — and puts an `egui::TextEdit` inside it with `Frame::NONE`, because egui's own
/// frame is not the one the design shows. egui then lays that box out at the **top** of the
/// rectangle it is given, and with no frame there is no margin to push it down, so a rectangle the
/// height of the whole field left the words sitting against its top edge: `Filter files` was about
/// three points high in a 24 point box, on a different line from the magnifier beside it.
///
/// So a field hands its box one text row, centred in the field. One function rather than a number
/// repeated in five components, because a fifth field added later would otherwise be the fifth
/// chance to get it wrong.
///
/// `left` is how far in from the field's left edge the text starts — 26 points where there is a
/// magnifier in front of it, 8 where there is not.
pub fn field_text_rect(ui: &egui::Ui, field: Rect, left: f32) -> Rect {
    field_text_rect_at(field, left, ui.text_style_height(&egui::TextStyle::Body))
}

/// The same, for a field that sets its text in a size of its own rather than in the interface's.
///
/// **A field centres the row it is going to draw, and the size that row will be set in is therefore an
/// argument rather than an assumption.** [`field_text_rect`] measures the strip with
/// `TextStyle::Body`, which is `appearance.ui.font.size` — 12.5 by default and 24 on the machine
/// `task-1914` was reported from. A caller that then draws at a fixed size gets a strip measured for
/// one size holding text set in another: at 24 the browser's address bar was handed a 28 point strip,
/// drew its 12 point words at the **top** of it, and so put them about six points above the middle of
/// a 22 point field and over its own border.
///
/// `row` is the height the text will really occupy, which is `FontId::proportional(n)`'s own row —
/// `Ui::fonts(|f| f.row_height(&font))` — so the box and the letters are measured the same way.
pub fn field_text_rect_at(field: Rect, left: f32, row: f32) -> Rect {
    let width = (field.width() - left - 8.0).max(1.0);
    Rect::from_min_size(
        Pos2::new(field.left() + left, (field.center().y - row / 2.0).round()),
        Vec2::new(width, row),
    )
}

/// The same rectangle, having first given the **whole** of the field to the box that will go in it.
///
/// That one text row is what stops the words sitting against the top edge, and it was also the only
/// part of the control a pointer could hit. A 24 point field has nine points of dead height and
/// eight points of dead width down its left hand side, and a click in any of it took the keyboard
/// nowhere at all — so the pane behind went on holding the keys, and `Ctrl+V` in the New Data Source
/// dialog put the path into the file behind the window. Measured in `task-1795`:
///
/// ```text
/// text rect = [[491.0 244.0] - [803.0 259.0]]   // 15 points tall inside a 24 point field
/// after a click in the padding, text_edit_focused = false
/// document = "helloPASTED"
/// ```
///
/// The same miss is why `Ctrl/Cmd+Enter` in a SQL console did nothing: that chord is read only while
/// the box has the keyboard.
///
/// The claim is made **before** the box is added, which is what keeps it from taking anything away:
/// egui gives a pointer to the last widget that wanted it, so a press inside the box's own strip is
/// still the box's, and this catches only the padding round it. `id` is the id the caller then gives
/// its `TextEdit`, so this hands the keyboard to that box and not to whichever one egui's auto
/// counter happened to name.
pub fn field_takes_the_whole_rectangle(
    ui: &egui::Ui,
    field: Rect,
    left: f32,
    id: egui::Id,
) -> Rect {
    claim_the_field(ui, field, id);
    field_text_rect(ui, field, left)
}

/// The same claim, for a field whose text is set in a size of its own. See [`field_text_rect_at`].
pub fn field_takes_the_whole_rectangle_at(
    ui: &egui::Ui,
    field: Rect,
    left: f32,
    id: egui::Id,
    font: &egui::FontId,
) -> Rect {
    claim_the_field(ui, field, id);
    let row = ui.ctx().fonts_mut(|fonts| fonts.row_height(font));
    field_text_rect_at(field, left, row)
}

/// The claim on its own, for a box that is laid out over the whole of its field rather than in a
/// strip: the commit message, a ticket's description, a console's SQL.
///
/// Those have the same fault in a milder form — the margin between the drawn frame and the box is
/// dead — and the same answer. See [`field_takes_the_whole_rectangle`] for what it is for.
pub fn claim_the_field(ui: &egui::Ui, field: Rect, id: egui::Id) -> egui::Response {
    let response = ui.interact(field, id.with("field-ground"), Sense::click());
    if response.clicked() {
        ui.ctx().data_mut(|data| data.insert_temp(wants_the_keyboard(), id));
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    response
}

/// Where a field records the box it wants the keyboard given to, for the **next** frame.
///
/// It has to be the next frame, and that is not a stylistic choice. egui surrenders a widget's focus
/// when a press lands anywhere that widget is not hovered:
///
/// ```ignore
/// let pointer_clicked_elsewhere = should_surrender_focus && !res.hovered();
/// if pointer_clicked_elsewhere && memory.has_focus(id) { memory.surrender_focus(id); }
/// ```
///
/// A press in a field's padding is not on the box inside it, so a focus handed over at the moment of
/// the press was taken straight back a few lines later, when the box itself was created in the same
/// frame. Measured: the claim reported `clicked=true` and `text_edit_focused` was still false
/// afterwards. `app::hold_the_keyboard` is where this is acted on, because it runs before anything
/// is drawn and is already the one place the window arranges who holds the keys.
pub fn wants_the_keyboard() -> egui::Id {
    egui::Id::new("unluminous-field-wants-the-keyboard")
}

/// A box to search in: the field, a magnifier in front of it, and the words to show while it is
/// empty.
///
/// The explorer's filter, `Go to File` and `Find in Files` are all this shape, and the field's own
/// frame is `FIELD` with a one point stroke and the style guide's corner radius wherever it appears.
/// `name` is what the box is called, which is what a test asks for and what assistive technology
/// reads out — egui names a text box after whatever has been typed into it, so every field in Unluminous
/// says its own name.
pub fn search_field(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    hint: &str,
    value: &mut String,
) -> egui::Response {
    search_field_over(ui, area, name, hint, value, true)
}

/// The same field with its own ground drawn or left alone, for a field on a decoration canvas.
///
/// See [`choice_button_over`]: the well the picture shows is drawn behind the whole pane, and a flat
/// rectangle drawn here would fill it in.
pub fn search_field_over(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    hint: &str,
    value: &mut String,
    ground: bool,
) -> egui::Response {
    let painter = ui.painter().clone();
    if ground {
        painter.rect(
            area,
            CornerRadius::same(size::CONTROL_CORNER),
            color::field(),
            Stroke::new(1.0, color::control_border()),
            egui::StrokeKind::Inside,
        );
    }
    icon::magnifier(&painter, Pos2::new(area.left() + 15.0, area.center().y), color::text_faint());
    let id = ui.id().with(("search-field", name));
    let text_rect = field_takes_the_whole_rectangle(ui, area, 28.0, id);
    let mut field = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
    let response = field.add(
        egui::TextEdit::singleline(value)
            .id(id)
            .hint_text(egui::RichText::new(hint).color(color::text_faint()))
            .frame(egui::Frame::NONE)
            .desired_width(text_rect.width())
            .text_color(color::text_control()),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, name));
    response
}

/// Text with some of its characters picked out in the accent colour, which is how a search result
/// shows what it matched.
///
/// `marks` are character positions, not byte positions, because that is what a matcher counting
/// letters produces. One `LayoutJob` rather than one galley per letter, so the text is still laid
/// out as text: painting each character at a position worked out by adding up widths loses the
/// kerning between them, which is visible at any size worth reading.
pub fn marked_text(
    painter: &egui::Painter,
    text: &str,
    marks: &[usize],
    tint: Color32,
    font: egui::FontId,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    let plain = egui::TextFormat { font_id: font.clone(), color: tint, ..Default::default() };
    let marked = egui::TextFormat { font_id: font, color: color::accent(), ..Default::default() };
    for (index, character) in text.chars().enumerate() {
        let format = if marks.contains(&index) { marked.clone() } else { plain.clone() };
        job.append(&character.to_string(), 0.0, format);
    }
    painter.layout_job(job)
}

/// A button showing the current value, which opens a list when clicked.
///
/// `contents` draws the list and returns what was chosen, so the caller decides what a choice is: the
/// toolbar returns a `unluminous_core::Command` and the Settings window returns a font size.
pub fn dropdown<T>(
    ui: &mut egui::Ui,
    area: Rect,
    value: &str,
    name: &str,
    draw: Option<fn(&egui::Painter, Pos2, Color32)>,
    contents: impl FnOnce(&mut egui::Ui) -> Option<T>,
) -> Option<T> {
    dropdown_over(ui, area, value, name, draw, true, contents)
}

/// The same dropdown with its own ground drawn or left alone, for one on a decoration canvas.
///
/// See [`choice_button_over`] and [`search_field_over`], which exist for the same reason and are the two
/// controls the board already needed it for: the well a value sits in on the Agent-Tasks ticket is drawn
/// **behind** the whole modal by `services::vello_canvas`, and a flat rectangle painted here would fill it
/// in. Everything else about the control — the words, the chevron, the popup, the name — is one function.
#[allow(clippy::too_many_arguments)]
pub fn dropdown_over<T>(
    ui: &mut egui::Ui,
    area: Rect,
    value: &str,
    name: &str,
    draw: Option<fn(&egui::Painter, Pos2, Color32)>,
    ground: bool,
    contents: impl FnOnce(&mut egui::Ui) -> Option<T>,
) -> Option<T> {
    let id = ui.id().with(("dropdown", name));
    let response = ui.interact(area, id, Sense::click()).on_hover_text(name);
    let painter = ui.painter();
    if ground {
        painter.rect(
            area,
            CornerRadius::same(size::CONTROL_CORNER),
            color::control(),
            Stroke::new(1.0, color::control_border()),
            egui::StrokeKind::Inside,
        );
    }
    let mut text_left = area.left() + 10.0;
    if let Some(draw) = draw {
        draw(painter, Pos2::new(text_left + 4.0, area.center().y), color::text_dim());
        text_left += 16.0;
    }
    let galley = painter.layout_no_wrap(
        value.to_owned(),
        egui::FontId::proportional(12.5),
        color::text_control(),
    );
    painter.galley(
        Pos2::new(text_left, area.center().y - galley.size().y / 2.0),
        galley,
        color::text_control(),
    );
    icon::chevron_down(painter, Pos2::new(area.right() - 11.0, area.center().y), color::text_dim());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), name)
    });

    // `Popup::from_toggle_button_response` opens and closes on clicks of this button and holds the state
    // itself, under `Popup::default_response_id(&response)` — the button's own id with `"popup"` joined
    // on, not the button's id alone. Closing with the bare id closed a popup nothing was tracked under,
    // so the real one stayed open: a value chosen from `Model` stayed on screen, floating over whatever
    // was drawn underneath it, and ate the next field's click as "outside" instead of opening it.
    let popup_id = egui::Popup::default_response_id(&response);
    let chosen = egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            egui::Frame::popup(ui.style())
                .fill(color::control())
                .stroke(Stroke::new(1.0, color::control_border())),
        )
        .width(area.width().max(120.0))
        .show(contents)
        .and_then(|inner| inner.inner);
    if chosen.is_some() {
        egui::Popup::close_id(ui.ctx(), popup_id);
    }
    chosen
}

/// A small icon button that opens a panel under itself and stays open until it is clicked away from.
///
/// A sibling of [`dropdown`] rather than a setting on it, because the two are different things. A
/// dropdown is a value picker: it shows the value it holds, and choosing one closes it. A flyout is
/// a panel of controls that is used several times in a row — bold, then a colour, then an alignment
/// — so it closes only when the pointer goes elsewhere, and `contents` draws whatever it likes into
/// the rectangle it is given rather than returning one choice.
///
/// `contents` is handed the `Ui` inside the panel and returns whatever the caller wants out of it.
/// The return is `None` while the panel is shut.
///
/// One thing a flyout must not hold is another popup. egui keeps at most one popup open at a time,
/// so a dropdown inside this panel would shut the panel the moment it opened.
pub fn flyout<T>(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    draw: fn(&egui::Painter, Pos2, Color32),
    width: f32,
    contents: impl FnOnce(&mut egui::Ui) -> T,
) -> Option<T> {
    let response =
        ui.interact(area, ui.id().with(("flyout", name)), Sense::click()).on_hover_text(name);
    // What the panel will be by the time it is drawn: the click this frame is what toggles it, and
    // the button has to be tinted for the state it is going into rather than the one it is leaving.
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response))
        != response.clicked();
    let painter = ui.painter();
    if open {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::accent());
    } else if response.hovered() {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::control());
    }
    let tint = if open { color::text_strong() } else { color::text_control() };
    draw(painter, area.center(), tint);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), open, name)
    });
    egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            egui::Frame::popup(ui.style())
                .fill(color::menu())
                .stroke(Stroke::new(1.0, color::control_border())),
        )
        .width(width)
        .show(contents)
        .map(|inner| inner.inner)
}

/// A flyout whose button carries a word and a chevron rather than an icon.
///
/// The run widget's name is the only one — see [`flyout`] for what a flyout is and why it is not a
/// [`dropdown`]. It is here rather than in the widget for the reason everything else in this file
/// is: a second control that almost agreed with `flyout` about how a popup opens and closes would
/// be a second chance to get it wrong.
///
/// `label` is what is drawn and `name` is what the control is called, because the label is a
/// configuration's name and changes, and a control whose accessible name changed with its value
/// could not be asked for by a test.
pub fn labelled_flyout<T>(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    label: &str,
    width: f32,
    contents: impl FnOnce(&mut egui::Ui) -> T,
) -> Option<T> {
    labelled_flyout_with_icon(ui, area, name, label, None, width, contents)
}

/// The same, with a drawn mark in front of the word.
///
/// **`flyout` is the wrong helper for a button that carries a word**, and that is what this exists to
/// stop being rediscovered: `flyout` draws its icon at `area.center()`, which is right for the square
/// icon buttons it was written for and puts the mark on top of the word on a button wide enough to hold
/// one. Measured — `components::branch_widget` drew `main` with the branch icon over the middle of it.
///
/// So the mark goes at a fixed offset from the left and the word starts clear of it, and a caller passing
/// `None` gets exactly what `labelled_flyout` always drew.
pub fn labelled_flyout_with_icon<T>(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    label: &str,
    mark: Option<fn(&egui::Painter, Pos2, Color32)>,
    width: f32,
    contents: impl FnOnce(&mut egui::Ui) -> T,
) -> Option<T> {
    let response = ui
        .interact(area, ui.id().with(("labelled-flyout", name)), Sense::click())
        .on_hover_text(name);
    // What the panel will be by the time it is drawn: the click this frame is what toggles it.
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response))
        != response.clicked();
    let painter = ui.painter();
    if open {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::control());
    } else if response.hovered() {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::control());
    }
    let tint = if open { color::text_strong() } else { color::text_control() };
    // The word starts after the mark when there is one, and where it always did when there is not.
    let words_from = match mark {
        Some(draw) => {
            draw(painter, Pos2::new(area.left() + 11.0, area.center().y), tint);
            area.left() + 22.0
        }
        None => area.left() + 9.0,
    };
    let galley = painter.layout_no_wrap(label.to_owned(), egui::FontId::proportional(12.5), tint);
    painter.galley(Pos2::new(words_from, area.center().y - galley.size().y / 2.0), galley, tint);
    icon::chevron_down(painter, Pos2::new(area.right() - 10.0, area.center().y), color::text_dim());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), open, name)
    });
    egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            egui::Frame::popup(ui.style())
                .fill(color::menu())
                .stroke(Stroke::new(1.0, color::control_border())),
        )
        .width(width)
        .show(contents)
        .map(|inner| inner.inner)
}

/// A button carrying a word rather than a picture, filled when what it stands for is switched on.
///
/// The three line spacings are the only ones. They were a dropdown when they sat in the toolbar and
/// cannot stay one inside a flyout, because egui keeps one popup open at a time and opening the list
/// would shut the panel it was in. Three buttons in a row are better in a panel anyway: which
/// spacing is on can be seen without opening anything, which is how the alignments beside them
/// already work.
pub fn choice_button(ui: &mut egui::Ui, area: Rect, label: &str, active: bool) -> bool {
    choice_button_named(ui, area, label, label, active)
}

/// A choice button that announces a name other than the word drawn on it.
///
/// For a button whose word alone does not say what pressing it does, and for one whose word already appears
/// somewhere else on the same screen. The agent chooser under the New lane is both: it draws `claude`, which is
/// also the word on every card assigned to that agent, so a test or a screen reader asking for `claude` finds
/// several things and cannot tell which is the chooser.
pub fn choice_button_named(
    ui: &mut egui::Ui,
    area: Rect,
    label: &str,
    announced: &str,
    active: bool,
) -> bool {
    choice_button_over(ui, area, label, announced, active, true)
}

/// The same button with its own ground drawn or left alone.
///
/// **`ground: false` is for a button whose surface something else has already drawn**, which is what a
/// plugin's decoration canvas does: the gradient, the shadows and the pressed edge are painted into one
/// texture behind the whole pane, and a flat rectangle drawn here would cover them. What is left is the word
/// and the click, which is all this ever really was. See `services::vello_canvas`.
pub fn choice_button_over(
    ui: &mut egui::Ui,
    area: Rect,
    label: &str,
    announced: &str,
    active: bool,
    ground: bool,
) -> bool {
    let response = ui.interact(area, ui.id().with(("choice", announced)), Sense::click());
    let painter = ui.painter();
    if !ground {
        // A hover still has to answer, or a button on a canvas would be the one control in Unluminous that never
        // says it was reached. A wash rather than a fill, so the gradient under it still shows.
        if response.hovered() {
            painter.rect_filled(
                area,
                CornerRadius::same(size::CONTROL_CORNER),
                Color32::from_white_alpha(14),
            );
        }
    } else if active {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::accent());
    } else if response.hovered() {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::control());
    } else {
        painter.rect(
            area,
            CornerRadius::same(size::CONTROL_CORNER),
            Color32::TRANSPARENT,
            Stroke::new(1.0, color::control_border()),
            egui::StrokeKind::Inside,
        );
    }
    let tint = if active { color::text_strong() } else { color::text_control() };
    let galley = painter.layout_no_wrap(label.to_owned(), egui::FontId::proportional(12.5), tint);
    painter.galley(area.center() - galley.size() / 2.0, galley, tint);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), active, announced)
    });
    response.clicked()
}

/// A heading beside a row of controls in a flyout, naming what the row is for.
pub fn row_label(painter: &egui::Painter, at: Pos2, name: &str) {
    let galley = painter.layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(11.5),
        color::text_dim(),
    );
    painter.galley(Pos2::new(at.x, at.y - galley.size().y / 2.0), galley, color::text_dim());
}

/// The rows of one menu, whether it hangs from the bar or from a right click.
///
/// A menu inside a menu is drawn as a heading with its entries indented under it rather than as a
/// second list that opens sideways. Recent Projects and the explorer's `Git` submenu are the only
/// ones, both hold a short list, and a heading with rows under it needs no hovering to reach. The
/// macOS menu bar does have a real submenu there, because that is what the platform draws.
///
/// This lives here rather than in `menu_bar` because there are three menus in Unluminous now — the bar
/// inside the window, the explorer's context menu and the gutter's — and one renderer is what stops
/// them growing three row heights.
pub fn menu_rows(ui: &mut egui::Ui, entries: &[Entry], indent: f32) -> Option<Action> {
    // A menu taller than the window scrolls rather than running off the bottom of it. The Git menu
    // has twenty-two entries and does not fit in a small window; before this, its last few could
    // not be reached at all.
    let room = (ui.ctx().content_rect().height() - 120.0).max(180.0);
    // egui puts `item_spacing.y` between every row, so a count of row heights alone comes out short
    // by a third and the menu is decided to fit when it does not.
    let gap = ui.spacing().item_spacing.y;
    let height = tall(entries, gap);
    if height > room {
        return egui::ScrollArea::vertical()
            .max_height(room)
            // Without this the box comes out about two thirds of what it was allowed, because a
            // scroll area inside a popup measures itself against the popup's own idea of how much
            // room there is rather than against the number it was given.
            .min_scrolled_height(room)
            .id_salt("unluminous-menu-scroll")
            .show(ui, |ui| rows(ui, entries, indent))
            .inner;
    }
    rows(ui, entries, indent)
}

/// The rows themselves, once it has been decided whether they scroll.
fn rows(ui: &mut egui::Ui, entries: &[Entry], indent: f32) -> Option<Action> {
    let mut chosen = None;
    for entry in entries {
        match entry {
            Entry::Separator => {
                ui.separator();
            }
            Entry::Item { name, action, shortcut, enabled, checked, .. } => {
                let keys = shortcut.map(|shortcut| shortcut.label()).unwrap_or_default();
                if menu_row(ui, name, &keys, *enabled, *checked, indent) {
                    chosen = Some(action.clone());
                }
            }
            Entry::Submenu { name, entries } => {
                menu_heading(ui, name, indent);
                if let Some(action) = rows(ui, entries, indent + 14.0) {
                    chosen = Some(action);
                }
            }
        }
    }
    chosen
}

/// How tall a run of entries is drawn, in points, counting the gap egui puts between rows.
///
/// **It recurses, because a submenu can hold a submenu.** `task-1848` put every plugin's entries under one
/// `Plugins` menu, so the rows under that heading are themselves headings with rows beneath them — and a
/// measurement that counted a submenu's entries as plain rows undercounted by a heading and a gap for each
/// one nested. Undercounting here is not a cosmetic fault: it is what decides whether the menu is given a
/// `ScrollArea`, so a menu measured as fitting when it does not is a menu whose last entries cannot be
/// reached at all.
fn tall(entries: &[Entry], gap: f32) -> f32 {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Separator => 8.0 + gap,
            Entry::Item { .. } => 24.0 + gap,
            Entry::Submenu { entries, .. } => 22.0 + gap + tall(entries, gap),
        })
        .sum()
}

/// One row of a menu: a tick when it is switched on, its name, and its keyboard shortcut on the right.
///
/// A row that cannot be used just now is drawn dimmed and takes no clicks, which is how a menu says that
/// there is nothing to undo. The accessible name is the plain wording, with no tick and no padding in it,
/// so a test can ask for `Open Folder` by name however the row happens to be decorated.
pub fn menu_row(
    ui: &mut egui::Ui,
    name: &str,
    shortcut: &str,
    enabled: bool,
    checked: bool,
    indent: f32,
) -> bool {
    let height = 24.0;
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), sense);
    if response.hovered() && enabled {
        ui.painter().rect_filled(rect, CornerRadius::same(4), color::selected_row());
    }
    let painter = ui.painter();
    let tint =
        if enabled { color::text_control() } else { color::text_faint().gamma_multiply(0.6) };
    let left = rect.left() + 8.0 + indent;
    if checked {
        // Drawn, not the character at U+2713. No font in the stack Unluminous hands egui has a shape for
        // it, so it came out as the empty box a missing glyph renders as — visible against
        // `Raw Markdown` on the View menu in any capture of it, and the exact fault the style guide
        // already records for the shift symbol. `icon::tick` is the same tick every tick box in
        // Unluminous draws.
        icon::tick(painter, Pos2::new(left + 6.0, rect.center().y), color::accent());
    }
    let label = painter.layout_no_wrap(name.to_owned(), egui::FontId::proportional(12.5), tint);
    painter.galley(Pos2::new(left + 18.0, rect.center().y - label.size().y / 2.0), label, tint);
    if !shortcut.is_empty() {
        let keys = painter.layout_no_wrap(
            shortcut.to_owned(),
            egui::FontId::proportional(11.5),
            color::text_faint(),
        );
        painter.galley(
            Pos2::new(rect.right() - 8.0 - keys.size().x, rect.center().y - keys.size().y / 2.0),
            keys,
            color::text_faint(),
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    response.clicked()
}

/// A heading inside a menu, which is what a menu inside a menu is drawn as inside the window.
pub fn menu_heading(ui: &mut egui::Ui, name: &str, indent: f32) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    // **A heading has a name too**, which `task-1848` is what made necessary: with every plugin's entries
    // moved under one `Plugins` menu, the plugin's own name is a heading rather than a menu in the bar —
    // so without this there is no way to ask whether `Agent-Tasks` is on the screen at all. It is a label
    // rather than a button, because a heading takes no clicks; `CLAUDE.md`'s rule is that a control with
    // no name cannot be tested, and a heading is what a submenu's title is drawn as here.
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name));
    let painter = ui.painter();
    let label = painter.layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(11.0),
        color::text_dim(),
    );
    painter.galley(
        Pos2::new(rect.left() + 8.0 + indent, rect.center().y - label.size().y / 2.0),
        label,
        color::text_dim(),
    );
}

/// A small square button holding a drawn icon.
pub fn icon_button(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    draw: fn(&egui::Painter, Pos2, Color32),
) -> bool {
    let response =
        ui.interact(area, ui.id().with(("icon-button", name)), Sense::click()).on_hover_text(name);
    if response.hovered() {
        ui.painter().rect_filled(area, CornerRadius::same(4), color::control());
    }
    draw(ui.painter(), area.center(), color::text_dim());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
    response.clicked()
}

/// A word in a bar that opens a menu when clicked, which is what `Unluminous`, `File`, `Edit` and `View` are.
pub fn bar_button(ui: &mut egui::Ui, area: Rect, name: &str, strong: bool) -> egui::Response {
    let response = ui.interact(area, ui.id().with(("bar-button", name)), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(area, CornerRadius::same(4), color::control());
    }
    let tint = if strong { color::text_strong() } else { color::text_control() };
    let painter = ui.painter();
    let label = painter.layout_no_wrap(name.to_owned(), egui::FontId::proportional(12.5), tint);
    painter.galley(
        Pos2::new(area.center().x - label.size().x / 2.0, area.center().y - label.size().y / 2.0),
        label,
        tint,
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field centres the row it is going to draw, and the size that row will be set in is an argument.
    ///
    /// `task-1914`: *"the web browser node url text is not vertically centered in the bar, so its up too
    /// high and partially clipped."* The numbers here are that report. The address bar's field is 22
    /// points tall and its words are set at 12, which is a row of about 16 — but the strip was measured
    /// with `TextStyle::Body`, which is `appearance.ui.font.size` and was **24** on the machine it was
    /// reported from, a row of 28. So the field was handed a strip taller than itself, `egui` laid the 12
    /// point text out at the **top** of it, and the address sat above centre with its top cut off by the
    /// field's own border.
    #[test]
    fn a_fields_text_row_is_measured_at_the_size_the_text_will_be_set_in() {
        let field = Rect::from_min_max(Pos2::new(100.0, 200.0), Pos2::new(400.0, 222.0));

        // The interface's own row at 24 point text, which is what the old reckoning used.
        let interface = field_text_rect_at(field, 9.0, 28.0);
        assert!(
            interface.top() < field.top() && interface.bottom() > field.bottom(),
            "a 28 point strip cannot fit in a 22 point field: {interface:?} in {field:?}",
        );

        // The size the address is really set in.
        let real = field_text_rect_at(field, 9.0, 16.0);
        assert!(field.contains_rect(real), "the strip is inside the field: {real:?} in {field:?}");
        assert!(
            (real.center().y - field.center().y).abs() <= 0.5,
            "and centred on it: {} against {}",
            real.center().y,
            field.center().y,
        );
        assert_eq!(real.left(), field.left() + 9.0, "with the caller's own inset in front of it");
    }
}
