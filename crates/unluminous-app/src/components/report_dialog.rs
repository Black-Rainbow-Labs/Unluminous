//! The Report an Issue modal (`task-2276`).
//!
//! The flag at the bottom right of the window opens it, and so does `Unluminous -> Report an Issue...`. It is
//! drawn in the redesign, with `components::realm::dialog`'s surface, heading and footer and
//! `components::settings_kit`'s cards, so it is the same object as the New Data Source dialog and the
//! Settings window: one surface thirty points round on the pane's ground, raised cards, carved wells.
//!
//! It has two faces. The **form** is the kind, the title, the description, the attachments and the two
//! switches that say what else goes in. Once `Open on GitHub` is pressed it shows **the second step**
//! instead: the issue is open in the browser, and this says what is left to do there, which is one paste
//! and one press.
//!
//! Like every component it decides nothing. It draws the draft it is handed, changes the fields a person
//! types into, and answers what was pressed; `app::report` does the rest.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{Pos2, Rect, Vec2};
use rux::components::ChipButton;
use rux::icon::{Icon, Mark};
use rux::{Rux, Style};

use crate::components::realm::dialog;
use crate::components::settings_kit::{self as kit, Column};
use crate::components::modal;
use crate::services::issue_report::{Attachment, Kind, Report};

/// The modal's id, which is what `modal move`, `modal size` and `modal zoom` name it by.
pub const ID: &str = "unluminous-report";
const WIDTH: f32 = 720.0;
const HEIGHT: f32 = 700.0;

/// A picture's chip: its thumbnail and its name under it.
const PICTURE: Vec2 = Vec2::new(116.0, 92.0);
/// A file's chip.
const FILE: Vec2 = Vec2::new(188.0, 44.0);
const CHIP_GAP: f32 = 12.0;

/// What the second step needs: what was opened, and the files that wait to be pasted.
pub struct Sent<'a> {
    pub address: &'a str,
    /// The issue's title and body, shown as they will arrive so nothing is sent unseen.
    pub title: &'a str,
    pub body: &'a str,
    pub files: &'a [PathBuf],
    /// Whether the files really went onto the clipboard, which a window a test built never does.
    pub on_the_clipboard: bool,
    /// Whether the browser was really asked to open it.
    pub opened: bool,
}

/// What the modal shows besides the draft.
pub struct Showing<'a> {
    /// The line beside `Include version and system`.
    pub system: &'a str,
    /// The line beside `Include logs`, or `None` when there is no log to include.
    pub logs: Option<&'a str>,
    pub thumbnails: &'a HashMap<PathBuf, egui::TextureHandle>,
    /// Something to say under the attachments, such as a file that could not be attached.
    pub problem: Option<&'a str>,
    pub sent: Option<Sent<'a>>,
    /// Put the keyboard in the title, on the frame the modal opens.
    pub focus_title: bool,
}

/// What a person pressed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Closed, keeping the draft.
    pub close: bool,
    /// `Discard`, or `Done` after it was sent: close and throw the draft away.
    pub discard: bool,
    pub send: bool,
    pub screenshot: bool,
    pub add_files: bool,
    pub detach: Option<String>,
    pub copy_again: bool,
    pub show_files: bool,
    pub open_again: bool,
}

/// Draw the modal for `report`. Answers what was pressed.
pub fn show(ctx: &egui::Context, report: &mut Report, showing: &Showing<'_>) -> Outcome {
    let mut outcome = Outcome::default();
    let (_, escaped) = modal::show_unframed(ctx, ID, WIDTH, HEIGHT, |ui, area| {
        dialog::surface(ui, "report", area);
        let inner = area.shrink2(Vec2::new(dialog::PAD_X, dialog::PAD_Y));
        let heading = match showing.sent {
            Some(_) => "Almost there",
            None => "Report an issue",
        };
        let pressed =
            kit::layer(ui, "report-heading", inner, |rux| dialog::heading(rux, inner, heading));
        if pressed {
            outcome.close = true;
        }
        // The heading is named as well as drawn, so a test and assistive technology can say which of the
        // two faces is showing.
        let named = ui.interact(
            Rect::from_min_size(inner.min, Vec2::new(220.0, dialog::CLOSE)),
            ui.id().with("report-heading-name"),
            egui::Sense::hover(),
        );
        named.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, heading));
        let below = inner.top() + dialog::CLOSE + 8.0;
        match &showing.sent {
            Some(sent) => second_step(ui, inner, below, report, sent, &mut outcome),
            None => form(ui, inner, below, report, showing, &mut outcome),
        }
    });
    if escaped {
        outcome.close = true;
    }
    outcome
}

/// The form: the report itself, then the attachments, then the buttons.
fn form(
    ui: &mut egui::Ui,
    inner: Rect,
    top: f32,
    report: &mut Report,
    showing: &Showing<'_>,
    outcome: &mut Outcome,
) {
    let foot = inner.bottom() - dialog::BUTTON - dialog::GAP;
    let lead = format!(
        "It goes to github.com/{}. Your browser opens with everything below already written in, and you press Submit there.",
        crate::services::issue_report::REPOSITORY
    );
    let below_the_lead = kit::layer(ui, "report-lead", inner, |rux| {
        kit::paragraph(rux, Pos2::new(inner.left(), top), inner.width(), &lead)
    }) + 6.0;
    // **The cards scroll and the buttons do not.** A window shorter than the modal asks for squeezes the
    // modal, and without this the cards ran under the footer. The region reaches past the cards to the
    // dialog's own edge so their shadows are not cut, which is `settings_kit::plugin_page`'s arrangement.
    let region = Rect::from_min_max(
        Pos2::new(inner.left() - dialog::PAD_X + 4.0, below_the_lead),
        Pos2::new(inner.right() + dialog::PAD_X - 4.0, foot - 6.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(region));
    child.set_clip_rect(region.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical()
        .id_salt("report-form")
        .max_height(region.height())
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show(&mut child, |ui| {
            let room = ui.available_rect_before_wrap();
            let mut column = Column::new(inner.left(), inner.right(), room.top() + 10.0);
            cards(ui, &mut column, report, showing, outcome);
            let drew = (column.pen - kit::CARD_GAP + 14.0 - room.top()).max(0.0);
            ui.allocate_space(Vec2::new(room.width(), drew));
        });

    let can_send = report.can_be_sent();
    let footer = Rect::from_min_max(Pos2::new(inner.left(), foot), inner.max);
    let pressed = kit::layer(ui, "report-footer", footer, |rux| {
        dialog::footer(rux, inner, &[("Discard", false), ("Cancel", true)], ("Open on GitHub", can_send))
    });
    match pressed {
        Some(0) => outcome.discard = true,
        Some(1) => outcome.close = true,
        Some(_) => outcome.send = true,
        None => {}
    }
    // The command key with Enter, because Enter is a new line in the description.
    if can_send && modal::Confirm::CommandEnter.pressed(ui) {
        outcome.send = true;
    }
}

/// The two cards: the report itself, and what is attached to it.
fn cards(
    ui: &mut egui::Ui,
    column: &mut Column,
    report: &mut Report,
    showing: &Showing<'_>,
    outcome: &mut Outcome,
) {
    kit::card(ui, column, "", None, |card| {
        let chosen = Kind::ALL.iter().position(|kind| *kind == report.kind);
        let words: Vec<&str> = Kind::ALL.iter().map(|kind| kind.word()).collect();
        if let Some(picked) =
            card.row("Kind", None, |row| row.segmented("Kind", &words, chosen, 90.0, false))
        {
            report.kind = Kind::ALL[picked];
        }
        let title = card.row("Title", None, |row| {
            let width = row.right - row.x;
            let rect = Rect::from_min_size(Pos2::new(row.x, row.top), Vec2::new(width, kit::CONTROL));
            kit::text_well(
                row.rux,
                rect,
                "Title",
                &mut report.title,
                "One line that says what it is about",
                false,
                false,
            )
        });
        if showing.focus_title {
            title.request_focus();
        }
        card.text_area("Description", None, &mut report.description, report.kind.hint());
    });

    kit::card(ui, column, "Attachments", Some(ATTACHMENTS_HELP), |card| {
        card.custom(kit::CONTROL, |rux, rect| {
            let mut x = rect.left();
            for (label, act) in [("Take screenshot", 0), ("Add files...", 1)] {
                let width = kit::button_width(rux, label, false);
                let button =
                    Rect::from_min_size(Pos2::new(x, rect.top() + 2.0), Vec2::new(width, kit::BUTTON));
                if kit::button(rux, button, label, kit::Kind::Chip) {
                    match act {
                        0 => outcome.screenshot = true,
                        _ => outcome.add_files = true,
                    }
                }
                x = button.right() + 10.0;
            }
        });
        let height = attachments_height(report, card.width());
        card.custom(height, |rux, rect| {
            attachments(rux, rect, &report.attachments, showing.thumbnails, outcome)
        });
        if let Some(problem) = showing.problem {
            card.help(problem);
        }
        // A switch for a log that does not exist would be a switch that does nothing, so it is a sentence.
        match showing.logs {
            Some(said) => {
                card.switch("Include the log", Some(said), &mut report.include_logs);
            }
            None => card.help("Unluminous has not written a crash log on this machine, so there is no log to include."),
        }
        card.switch(
            "Include the version and system",
            Some(showing.system),
            &mut report.include_system,
        );
    });
}

const ATTACHMENTS_HELP: &str = "The window was photographed when this opened. Drop files anywhere on the window, or press Ctrl+V with a picture copied, to add more.";

/// How tall the well of attachments is for this many, laid out across `width`.
fn attachments_height(report: &Report, width: f32) -> f32 {
    let mut x = 12.0;
    let mut rows = 1;
    let mut row_height: f32 = 0.0;
    let mut height = 0.0;
    for attachment in &report.attachments {
        let size = chip_size(attachment);
        if x + size.x > width - 12.0 && x > 12.0 {
            rows += 1;
            height += row_height + CHIP_GAP;
            row_height = 0.0;
            x = 12.0;
        }
        row_height = row_height.max(size.y);
        x += size.x + CHIP_GAP;
    }
    let _ = rows;
    match report.attachments.is_empty() {
        true => 56.0,
        false => height + row_height + 24.0,
    }
}

fn chip_size(attachment: &Attachment) -> Vec2 {
    match attachment.picture {
        true => PICTURE,
        false => FILE,
    }
}

/// The carved well of attachments: a picture is its thumbnail with its name under it, any other file is a
/// chip with its name and size, and each has a round cross that takes it off.
fn attachments(
    rux: &mut Rux<'_>,
    rect: Rect,
    attached: &[Attachment],
    thumbnails: &HashMap<PathBuf, egui::TextureHandle>,
    outcome: &mut Outcome,
) {
    let theme = rux.theme();
    let chat = theme.chat;
    kit::list_well(rux, rect, 18.0);
    if attached.is_empty() {
        let style = Style::sans(12.0);
        let galley = rux.text(style, "Nothing attached yet.", theme.ink.i400);
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(rect.left() + 18.0, rect.center().y),
            galley,
            style,
            theme.ink.i400,
        );
        return;
    }
    let mut x = rect.left() + 12.0;
    let mut y = rect.top() + 12.0;
    let mut row_height: f32 = 0.0;
    for attachment in attached {
        let size = chip_size(attachment);
        if x + size.x > rect.right() - 12.0 && x > rect.left() + 12.0 {
            x = rect.left() + 12.0;
            y += row_height + CHIP_GAP;
            row_height = 0.0;
        }
        row_height = row_height.max(size.y);
        let chip = Rect::from_min_size(Pos2::new(x, y), size);
        x = chip.right() + CHIP_GAP;
        if chip.bottom() > rect.bottom() {
            continue;
        }
        rux.chrome.surface(chip, 14.0, rux::Fill::gradient(chat.chip, chip), chat.raised_chip());
        match attachment.picture {
            true => picture_chip(rux, chip, attachment, thumbnails.get(&attachment.path)),
            false => file_chip(rux, chip, attachment),
        }
        let cross = Rect::from_center_size(chip.right_top() + Vec2::new(-12.0, 12.0), Vec2::splat(20.0));
        let label = format!("Remove {}", attachment.name);
        if ChipButton::new(Icon::X, 20.0, &label).mark(8.0, Some(1.6)).show(rux, cross).clicked() {
            outcome.detach = Some(attachment.name.clone());
        }
        let response = rux.ui.interact(
            chip,
            rux.ui.id().with(("report-attachment", &attachment.name)),
            egui::Sense::hover(),
        );
        let named = format!("Attachment: {}", attachment.name);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, named.clone()));
    }
}

/// A picture's thumbnail, fitted into the chip above its name.
fn picture_chip(rux: &mut Rux<'_>, chip: Rect, attachment: &Attachment, texture: Option<&egui::TextureHandle>) {
    let theme = rux.theme();
    let frame = Rect::from_min_max(chip.min + Vec2::splat(6.0), Pos2::new(chip.right() - 6.0, chip.bottom() - 24.0));
    match texture {
        Some(texture) => {
            let size = texture.size_vec2();
            let scale = (frame.width() / size.x).min(frame.height() / size.y);
            let shown = Rect::from_center_size(frame.center(), size * scale);
            rux.painter().image(
                texture.id(),
                shown,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE, // any ground: a white tint draws the picture in its own colours
            );
        }
        None => rux.mark(Mark::new(Icon::Image, 18.0), frame.center(), theme.ink.i400),
    }
    let style = Style::sans(11.0);
    let galley = rux.elided(style, &attachment.name, theme.ink.i700, chip.width() - 14.0);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(chip.left() + 8.0, chip.bottom() - 12.0),
        galley,
        style,
        theme.ink.i700,
    );
}

/// A file's mark, its name and its size.
fn file_chip(rux: &mut Rux<'_>, chip: Rect, attachment: &Attachment) {
    let theme = rux.theme();
    rux.mark(Mark::new(Icon::File, 15.0), Pos2::new(chip.left() + 18.0, chip.center().y), theme.ink.i500);
    let style = Style::sans(12.0).medium();
    let galley = rux.elided(style, &attachment.name, theme.ink.i900, chip.width() - 34.0 - 30.0);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(chip.left() + 34.0, chip.center().y - 7.0),
        galley,
        style,
        theme.ink.i900,
    );
    let small = Style::sans(10.5);
    let galley = rux.text(small, &attachment.size(), theme.ink.i400);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(chip.left() + 34.0, chip.center().y + 8.0),
        galley,
        small,
        theme.ink.i400,
    );
}

/// What is left to do once the issue is open in the browser.
fn second_step(
    ui: &mut egui::Ui,
    inner: Rect,
    top: f32,
    report: &Report,
    sent: &Sent<'_>,
    outcome: &mut Outcome,
) {
    let mut column = Column::new(inner.left(), inner.right(), top + 6.0);
    let paste = match cfg!(target_os = "macos") {
        true => "Cmd+V",
        false => "Ctrl+V",
    };
    let files = sent.files.len();
    let mut steps: Vec<String> = Vec::new();
    steps.push(match sent.opened {
        true => format!("Your browser has \u{201C}{}\u{201D} open on GitHub, already written.", report.full_title()),
        false => "The issue is ready to open. Press Open the issue below to open it on GitHub.".to_owned(),
    });
    if files > 0 {
        let what = match files {
            1 => "the file".to_owned(),
            count => format!("the {count} files"),
        };
        steps.push(match sent.on_the_clipboard {
            true => format!("Click in the description there and press {paste} to attach {what}. They are on the clipboard."),
            false => format!("Drag {what} from the folder Show files opens into the description there."),
        });
    }
    steps.push("Press Submit new issue. GitHub tells you when somebody answers.".to_owned());
    kit::card(ui, &mut column, "", None, |card| {
        for (index, step) in steps.iter().enumerate() {
            card.custom(40.0, |rux, rect| numbered(rux, rect, index + 1, step));
        }
    });
    if files > 0 {
        kit::card(ui, &mut column, "Files to attach", None, |card| {
            for file in sent.files {
                let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                let size = std::fs::metadata(file).map(|metadata| metadata.len()).unwrap_or_default();
                card.custom(24.0, |rux, rect| {
                    let theme = rux.theme();
                    rux.mark(Mark::new(Icon::File, 13.0), Pos2::new(rect.left() + 8.0, rect.center().y), theme.ink.i500);
                    let style = Style::sans(12.5);
                    let galley = rux.text(style, &name, theme.ink.i900);
                    let width = galley.size().x;
                    rux::text::draw_left_capitals(rux.painter(), Pos2::new(rect.left() + 24.0, rect.center().y), galley, style, theme.ink.i900);
                    let small = Style::sans(11.5);
                    let galley = rux.text(small, &crate::services::issue_report::size_of(size), theme.ink.i400);
                    rux::text::draw_left_capitals(rux.painter(), Pos2::new(rect.left() + 36.0 + width, rect.center().y), galley, small, theme.ink.i400);
                });
            }
        });
    }
    let row = Rect::from_min_size(Pos2::new(inner.left(), column.pen), Vec2::new(inner.width(), kit::BUTTON));
    kit::layer(ui, "report-sent-buttons", row, |rux| {
        let mut x = row.left();
        let open = match sent.opened {
            true => "Open the issue again",
            false => "Open the issue",
        };
        let mut labels = vec![(open, 2)];
        if files > 0 {
            labels.insert(0, ("Show files", 1));
            labels.insert(0, ("Copy files again", 0));
        }
        for (label, act) in labels {
            let width = kit::button_width(rux, label, false);
            let button = Rect::from_min_size(Pos2::new(x, row.top()), Vec2::new(width, kit::BUTTON));
            if kit::button(rux, button, label, kit::Kind::Chip) {
                match act {
                    0 => outcome.copy_again = true,
                    1 => outcome.show_files = true,
                    _ => outcome.open_again = true,
                }
            }
            x = button.right() + 10.0;
        }
    });
    let _ = sent.address;
    let foot = inner.bottom() - dialog::BUTTON - dialog::GAP;
    // What the issue says, in the face it is written in, under the buttons and above the foot.
    let said = Rect::from_min_max(
        Pos2::new(inner.left(), row.bottom() + 22.0),
        Pos2::new(inner.right(), foot - 12.0),
    );
    if said.height() > 60.0 {
        let text = format!("{}

{}", sent.title, sent.body);
        modal::monospaced(ui, said, "report-issue-text", &text);
    }
    let footer = Rect::from_min_max(Pos2::new(inner.left(), foot), inner.max);
    let pressed = kit::layer(ui, "report-sent-footer", footer, |rux| {
        dialog::footer(rux, inner, &[], ("Done", true))
    });
    if pressed.is_some() || modal::Confirm::Enter.pressed(ui) {
        outcome.discard = true;
    }
}

/// One numbered step: the number in a carved disc and the words beside it.
fn numbered(rux: &mut Rux<'_>, rect: Rect, number: usize, said: &str) {
    let theme = rux.theme();
    let chat = theme.chat;
    let disc = Rect::from_center_size(Pos2::new(rect.left() + 13.0, rect.center().y), Vec2::splat(26.0));
    rux.chrome.surface(disc, 13.0, rux::Fill::gradient(chat.action, disc), chat.raised_chip());
    let style = Style::sans(12.0).semibold();
    let galley = rux.text(style, &number.to_string(), egui::Color32::WHITE); // any ground: on the action's colour
    rux::text::draw_centred(rux.painter(), disc, galley, egui::Color32::WHITE); // any ground: on the action's colour
    let style = Style::sans(13.0).leading(1.4);
    let galley = rux::text::wrapped(rux.painter(), style, said, theme.ink.i900, rect.width() - 40.0);
    let at = Pos2::new(rect.left() + 40.0, rect.center().y - galley.size().y / 2.0);
    rux.painter().galley(at, galley, theme.ink.i900);
}
