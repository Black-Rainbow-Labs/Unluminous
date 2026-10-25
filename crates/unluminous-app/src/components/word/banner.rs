//! The strip under the format bar that says something about the file itself (task-2359, TDD 5.6): a
//! document opened read only because it has a password to modify, and an AutoRecover copy newer than the
//! file. Each offers what Word's own message bar does.

use egui::{Rect, Vec2};

use crate::components::controls::TextBoxes;
use crate::services::plugin_ui::{Look, Request};
use crate::services::word::Doc;

/// How tall the strip is, in points before the pane's zoom.
pub const HEIGHT: f32 = 34.0;

/// Whether the strip has something to say.
pub fn wanted(doc: &Doc) -> bool {
    doc.session.read_only || doc.timing.offer.is_some()
}

/// Draw the strip in `rect`.
pub fn show(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, rect: Rect, requests: &mut Vec<Request>) {
    ui.painter().rect_filled(rect, 0.0, look.palette.board_well);
    let inner = rect.shrink2(Vec2::new(12.0, 4.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        if let Some((_, when)) = doc.timing.offer.clone() {
            ui.label(egui::RichText::new(format!("AutoRecover kept a newer copy of this document from {when}.")).color(look.palette.text));
            let restore = ui.button("Restore");
            restore.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Restore the recovered copy"));
            if restore.clicked() {
                match doc.restore_recovery() {
                    Ok(()) => requests.push(Request::Message("Restored the recovered copy; save it to keep it".into())),
                    Err(problem) => requests.push(Request::Message(problem)),
                }
            }
            let discard = ui.button("Discard");
            discard.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Discard the recovered copy"));
            if discard.clicked() {
                doc.discard_recovery();
            }
            return;
        }
        ui.label(egui::RichText::new("Read only: this document has a password to modify.").color(look.palette.text));
        let password = doc.view.fields.entry("unlock-password").or_default();
        let field = ui.text_box(
            egui::TextEdit::singleline(password).password(true).id(egui::Id::new(("word-unlock", doc.key.as_str()))).desired_width(160.0),
        );
        field.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Password to modify"));
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let unlock = ui.button("Unlock");
        unlock.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Unlock for editing"));
        if unlock.clicked() || entered {
            let password = doc.view.fields.remove("unlock-password").unwrap_or_default();
            match doc.session.unlock_for_editing(&password) {
                Ok(()) => requests.push(Request::Message("The document can be changed".into())),
                Err(problem) => requests.push(Request::Message(problem)),
            }
        }
    });
}
