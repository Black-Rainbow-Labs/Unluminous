//! The text tools in the title bar: the `F` button, the panel of text options behind it, and the three
//! view mode buttons beside it.
//!
//! Every control sends a `unluminous_core::Command`. None of them changes the document directly, so the tools
//! and the keyboard go down the same path and cannot disagree about what bold means.
//!
//! Each control is given an accessible name, because the screenshot tests find controls by name rather
//! than by position, so moving a button does not break a test.
//!
//! The design draws the buttons rather than labelling them: the B is bold, the I is italic, the U is
//! underlined and the S is struck through, so each one looks like what it does, and the four alignment
//! buttons are small pictures of a paragraph. The colours are circles with a ring round the chosen one.
//!
//! ## Where they are, and why they moved
//!
//! They used to sit in a strip of their own, forty four points tall, between the title bar and the tabs.
//! `task-1658` moved them into the right hand end of the title bar, next to the window buttons, and the
//! strip went with them. The reason is what the strip did to everything below it: it was drawn for a
//! `.md` file and not for a `.rs` one, so switching tabs moved the tabs, the explorer and the whole
//! editing area up and down by forty four points. A control that appears is fine; a window that jumps is
//! not. In the title bar the room is already there whether the tools are in it or not.
//!
//! ## What is shown, and when
//!
//! Two questions are asked of the open file, and neither is answered anywhere but in
//! `services::file_kind`:
//!
//! - [`applies`] — are there any tools at all. Prose only: Markdown, a text file, a document that has
//!   not been saved yet. A source file and a picture get none.
//! - [`file_kind::preview_applies`] — are the three view mode buttons drawn. Markdown, and a document
//!   that has not been saved anywhere yet. A `.txt` file has nothing to preview.
//!
//! ## Where the formatting went
//!
//! Behind one 28 point button carrying a drawn `F`, named `Text options`, which opens a flyout holding
//! the four character formats, the five colours, the four alignments and the three line spacings. Nine
//! controls that are set rarely no longer take the width of the window permanently, and each keeps the
//! name it had, so a test asks for `Bold` exactly as it did before — after opening the panel.
//!
//! Three things that were here before that are still gone. The font family and the font size are in
//! `Edit -> Settings -> Appearance -> Font` and on the keyboard at command or control with plus and
//! minus, the background opacity is in `Edit -> Settings -> Appearance -> Background`, and undo and
//! redo are on the keyboard alone.

use std::path::Path;

use egui::{CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use unluminous_core::{Align, Color, Command, Document, StyleChange};

use crate::components::controls;
use crate::components::controls::WithHint as _;
use crate::services::file_kind;
use crate::theme::{color, icon, size};

/// The colours the text options panel offers, with the name shown when hovering over each one.
pub const COLORS: &[(&str, Color)] = &[
    ("White", Color::WHITE),
    ("Red", Color::RED),
    ("Green", Color::GREEN),
    ("Blue", Color::BLUE),
    ("Amber", Color::YELLOW),
];

/// The line spacings the spacing control offers, with the name shown for each.
pub const SPACINGS: &[(&str, f32)] = &[("Single", 1.0), ("One and a half", 1.5), ("Double", 2.0)];

/// How big a round button in the title bar is: the text options chip and the run widget's chips, 32 points
/// across in the Editing code design (`task-2241`).
pub const BUTTON: f32 = 32.0;
/// One view mode pill, and the carved track the three sit in: `width: 34px; height: 28px` inside a well
/// with three points round them and two between.
const MODE: Vec2 = Vec2::new(34.0, 28.0);
const TRACK_PAD: f32 = 3.0;
const MODE_GAP: f32 = 2.0;
/// Between the text options chip and the three view modes, so they read as two groups rather than four
/// buttons in a row.
const GROUP_GAP: f32 = 10.0;

/// The least the panel behind the `F` button is wide. It grows past this when its words need more, see
/// [`panel_layout`].
const PANEL: f32 = 316.0;
/// The least room the labels down the left of the panel take.
const PANEL_LABEL: f32 = 78.0;
/// What the panel's frame takes from its width, both sides together.
const PANEL_MARGIN: f32 = 12.0;
/// The four labels down the left of the panel.
const PANEL_LABELS: [&str; 4] = ["Format", "Color", "Alignment", "Line spacing"];
/// From the middle of one row of the panel to the middle of the next.
const PANEL_ROW: f32 = 34.0;

/// What the tools produced this frame.
#[derive(Debug, Default)]
pub struct ToolsOutcome {
    pub commands: Vec<Command>,
    /// Set when one of the three view mode buttons was pressed.
    pub view_mode: Option<crate::app::ViewMode>,
}

/// Whether any of the tools are drawn at all for this file.
///
/// **Either** question, not just the first. The `F` button and everything behind it is about how
/// prose is shown, so it is drawn for prose and not for code; but a `.mmd` file is not prose and
/// still has a preview, so it still wants the three view mode buttons. `task-1660` is what made
/// the two come apart: before it, a file that had a preview always had formatting too.
///
/// The title bar asks this before it lays itself out, because the answer decides how much of its
/// right hand end is spoken for.
pub fn applies(path: Option<&Path>) -> bool {
    file_kind::formatting_applies(path) || file_kind::preview_applies(path)
}

/// How much room the tools need for this file, which is nothing at all for most files.
///
/// The title bar leaves this much clear at its right hand end, in front of the window buttons, so
/// nothing it draws ever runs underneath a tool.
pub fn width(path: Option<&Path>) -> f32 {
    width_shown(path, Shown::BOTH)
}

/// Which of the two tools are pinned to the top bar. Both are, until a person unpins one. `task-2404`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub text_options: bool,
    pub view_modes: bool,
}

impl Shown {
    pub const BOTH: Shown = Shown { text_options: true, view_modes: true };
}

/// How much room the tools pinned to the top bar need for this file.
pub fn width_shown(path: Option<&Path>, shown: Shown) -> f32 {
    let modes_width = track_width();
    let formatting = shown.text_options && file_kind::formatting_applies(path);
    let preview = shown.view_modes && file_kind::preview_applies(path);
    match (formatting, preview) {
        (true, true) => BUTTON + GROUP_GAP + modes_width,
        (true, false) => BUTTON,
        // A diagram: the view modes on their own, with no `F` in front of them.
        (false, true) => modes_width,
        (false, false) => 0.0,
    }
}

/// How wide the carved track holding the three view modes is.
pub fn track_width() -> f32 {
    let modes = crate::app::ViewMode::ALL.len() as f32;
    modes * MODE.x + (modes - 1.0) * MODE_GAP + 2.0 * TRACK_PAD
}

/// Draw the tools into `area`, which is the rectangle the title bar left clear for them.
///
/// Nothing is painted behind them: the title bar is already there, and a second fill over it would be a
/// panel inside a panel. `area` is exactly [`width`] wide, so the `F` starts at its left edge and the
/// view modes finish at its right.
pub fn show(
    ui: &mut egui::Ui,
    area: Rect,
    document: &Document,
    bold_family: &egui::FontFamily,
    view_mode: crate::app::ViewMode,
    shown: Shown,
) -> ToolsOutcome {
    let mut outcome = ToolsOutcome::default();
    let middle = area.center().y;
    let path = document.path();

    // The `F` is drawn only for prose. A control that can never apply to this file is absent, which
    // is the rule the window already keeps.
    let mut pen = area.left();
    if shown.text_options && file_kind::formatting_applies(path) {
        let button =
            Rect::from_min_size(Pos2::new(pen, middle - BUTTON / 2.0), Vec2::splat(BUTTON));
        let paint =
            |ui: &mut egui::Ui, open: bool, hovered: bool| options_chip(ui, button, open, hovered);
        if let Some(commands) =
            controls::flyout_drawn(ui, button, "Text options", paint, panel_layout(ui).1, |panel| {
                text_options(panel, document, bold_family)
            })
        {
            outcome.commands.extend(commands);
        }
        pen = button.right() + GROUP_GAP;
    }

    // The three view modes sit beside the `F`, which is what `task-1658` asks for: one group of text
    // tools rather than one at each end of a bar.
    if shown.view_modes && file_kind::preview_applies(path) {
        let picked = view_modes(ui, Pos2::new(pen, middle), path, view_mode);
        if picked.is_some() {
            outcome.view_mode = picked;
        }
    }

    outcome
}

/// Where the text options chip and the view modes track are inside `area`, the rectangle [`show`] is given,
/// for whichever of them is drawn. The window lays a handle over each so it can be carried to the rail. `task-2404`.
pub fn parts(area: Rect, path: Option<&Path>, shown: Shown) -> (Option<Rect>, Option<Rect>) {
    let middle = area.center().y;
    let mut pen = area.left();
    let chip = match shown.text_options && file_kind::formatting_applies(path) {
        true => {
            let chip =
                Rect::from_min_size(Pos2::new(pen, middle - BUTTON / 2.0), Vec2::splat(BUTTON));
            pen = chip.right() + GROUP_GAP;
            Some(chip)
        }
        false => None,
    };
    let modes = (shown.view_modes && file_kind::preview_applies(path)).then(|| {
        let height = MODE.y + 2.0 * TRACK_PAD;
        Rect::from_min_size(Pos2::new(pen, middle - height / 2.0), Vec2::new(track_width(), height))
    });
    (chip, modes)
}

/// Which of the tools can apply to a file: the Text Options panel, and the view modes. The rail asks this to
/// dim a tool's button, and its flyout to say why it is empty. `task-2404`.
pub fn which_apply(path: Option<&Path>) -> (bool, bool) {
    (file_kind::formatting_applies(path), file_kind::preview_applies(path))
}

/// The carved track of the three view modes, its left end at `start`'s x and its middle at `start`'s y, and
/// the mode chosen in it this frame. The top bar draws it, and so does the View Modes flyout on the rail.
///
/// **One pill switch rather than three loose buttons**, `task-2219` and then `task-2241`. The three are one
/// choice, so they sit in one carved track, and the chosen one is a chip raised out of it with its mark in the
/// strong ink.
/// @param ui - where it is drawn
/// @param start - the track's left end, at the height of its middle
/// @param path - the file showing, which decides what the modes are called
/// @param view_mode - the mode the tab is in
pub fn view_modes(
    ui: &mut egui::Ui,
    start: Pos2,
    path: Option<&Path>,
    view_mode: crate::app::ViewMode,
) -> Option<crate::app::ViewMode> {
    let kind = file_kind::preview_kind(path);
    let middle = start.y;
    let track = Rect::from_min_size(
        Pos2::new(start.x, middle - (MODE.y + 2.0 * TRACK_PAD) / 2.0),
        Vec2::new(track_width(), MODE.y + 2.0 * TRACK_PAD),
    );
    crate::components::chrome_kit::layer(ui, "view-modes", track, 8.0, |rux| {
        let chat = rux.theme().chat;
        rux.chrome.surface(track, track.height() / 2.0, chat.well, chat.carved_sm());
        let mut picked = None;
        let mut left = track.left() + TRACK_PAD;
        for mode in crate::app::ViewMode::ALL {
            let button = Rect::from_min_size(Pos2::new(left, middle - MODE.y / 2.0), MODE);
            if view_mode_button(rux, button, mode, view_mode == mode, kind) {
                picked = Some(mode);
            }
            left += MODE.x + MODE_GAP;
        }
        picked
    })
}

/// The text options button: a round chip raised out of the bar with the design's letters mark, carved in
/// while its panel is open.
fn options_chip(ui: &mut egui::Ui, button: Rect, open: bool, hovered: bool) {
    crate::components::chrome_kit::layer(ui, "text-options", button, 10.0, |rux| {
        let chat = rux.theme().chat;
        let ink = rux.theme().ink;
        match open {
            true => rux.chrome.surface(button, BUTTON / 2.0, chat.well, chat.carved_sm()),
            false => rux::components::chip_disc(rux, button),
        }
        let tint = match (open, hovered) {
            (true, _) => ink.i900,
            (false, true) => chat.sky,
            (false, false) => ink.i500,
        };
        rux.mark(rux::Mark::new(rux::Icon::Letters, 17.0), button.center(), tint);
    });
}

/// The panel behind the `F` button: four rows, each named down the left.
///
/// A rule separates what applies to the selected text from what applies to the paragraph it is in,
/// because those are two different things and pressing one when you meant the other is the mistake
/// the rule is there to stop.
pub fn text_options(
    ui: &mut egui::Ui,
    document: &Document,
    bold_family: &egui::FontFamily,
) -> Vec<Command> {
    let mut commands = Vec::new();
    let style = document.active_style();
    let paragraph = document.active_paragraph_style();

    // The panel is painted at absolute positions inside one reserved rectangle, which is how
    // everything else in Unluminous is drawn. The rule between the two halves adds its own gap.
    let rule_gap = 11.0;
    let (label_column, panel) = panel_layout(ui);
    let (area, _) = ui.allocate_exact_size(
        Vec2::new(panel - PANEL_MARGIN, PANEL_ROW * 4.0 + rule_gap),
        Sense::hover(),
    );
    let left = area.left() + label_column;
    let mut middle = area.top() + PANEL_ROW / 2.0;

    // Bold, italic, underline and strikethrough. Each glyph carries the formatting it applies.
    controls::row_label(ui.painter(), Pos2::new(area.left(), middle), "Format");
    let formats: [(&str, &str, bool, Command); 4] = [
        ("B", "Bold", style.bold, Command::ToggleBold),
        ("I", "Italic", style.italic, Command::ToggleItalic),
        ("U", "Underline", style.underline, Command::ToggleUnderline),
        ("S", "Strikethrough", style.strikethrough, Command::ToggleStrikethrough),
    ];
    let mut pen = left;
    for (letter, name, active, command) in formats {
        let button = Rect::from_min_size(Pos2::new(pen, middle - 14.0), Vec2::splat(28.0));
        if format_button(ui, button, letter, name, active, bold_family) {
            commands.push(command);
        }
        pen += 32.0;
    }
    middle += PANEL_ROW;

    // The colours, as circles with a ring round the chosen one.
    controls::row_label(ui.painter(), Pos2::new(area.left(), middle), "Color");
    let mut pen = left + 4.0;
    for (name, swatch) in COLORS {
        let centre = Pos2::new(pen + 8.0, middle);
        let hit = Rect::from_center_size(centre, Vec2::splat(24.0));
        let response = ui
            .interact(hit, ui.id().with(("colour", name)), Sense::click())
            .with_hint(format!("Color: {name}"));
        let chosen = style.color == *swatch;
        let painter = ui.painter();
        // Through `theme::ink`, so the first swatch shows the ink it really puts on the page: white on a
        // dark theme and the theme's text colour on a light one.
        let fill = crate::theme::ink(*swatch);
        painter.circle_filled(centre, if chosen { 7.5 } else { 8.0 }, fill);
        if chosen {
            painter.circle_stroke(centre, 10.0, Stroke::new(1.6, color::text_strong()));
        } else if response.hovered() {
            painter.circle_stroke(centre, 10.0, Stroke::new(1.2, color::text_dim()));
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), chosen, *name)
        });
        if response.clicked() {
            commands.push(Command::ApplyStyle(StyleChange::color(*swatch)));
        }
        pen += 26.0;
    }
    middle += PANEL_ROW / 2.0 + rule_gap / 2.0;

    ui.painter().line_segment(
        [Pos2::new(area.left(), middle.round()), Pos2::new(area.right(), middle.round())],
        Stroke::new(1.0, color::divider()),
    );
    middle += PANEL_ROW / 2.0 + rule_gap / 2.0;

    // The four alignments, drawn as small pictures of a paragraph.
    controls::row_label(ui.painter(), Pos2::new(area.left(), middle), "Alignment");
    let mut pen = left;
    for align in Align::ALL {
        let button = Rect::from_min_size(Pos2::new(pen, middle - 14.0), Vec2::splat(28.0));
        if alignment_button(ui, button, align, paragraph.align == align) {
            commands.push(Command::SetAlign(align));
        }
        pen += 32.0;
    }
    middle += PANEL_ROW;

    // The line spacing, as three buttons rather than a dropdown: a dropdown inside a flyout would
    // shut the flyout, because egui keeps one popup open at a time.
    controls::row_label(ui.painter(), Pos2::new(area.left(), middle), "Line spacing");
    let mut pen = left;
    for (name, spacing) in SPACINGS {
        let width = spacing_width(ui, name);
        let button = Rect::from_min_size(Pos2::new(pen, middle - 13.0), Vec2::new(width, 26.0));
        let chosen = (paragraph.line_spacing - spacing).abs() < 0.01;
        if controls::choice_button(ui, button, name, chosen) {
            commands.push(Command::SetLineSpacing(*spacing));
        }
        pen += width + 6.0;
    }

    commands
}

/// How wide the label column is and how wide the whole panel is, measured in the interface's face.
///
/// `task-2287`: both were fixed numbers, worked out for a proportional face. In a monospaced interface face
/// `Line spacing` ran into its buttons and `Double` ran past the panel's right edge. So the label column is
/// the widest label and a gap, and the panel is as wide as the line spacing row needs, never less than it was.
pub fn panel_layout(ui: &egui::Ui) -> (f32, f32) {
    let painter = ui.painter();
    let label = PANEL_LABELS
        .iter()
        .map(|name| {
            let font = egui::FontId::proportional(11.5);
            painter.layout_no_wrap((*name).to_owned(), font, color::text_dim()).size().x
        })
        .fold(0.0_f32, f32::max);
    let label_column = (label + 12.0).max(PANEL_LABEL).ceil();
    let spacings: f32 = SPACINGS.iter().map(|(name, _)| spacing_width(ui, name) + 6.0).sum();
    let panel = (label_column + spacings - 6.0 + PANEL_MARGIN).max(PANEL).ceil();
    (label_column, panel)
}

/// How wide the button for one line spacing has to be to hold its name.
fn spacing_width(ui: &egui::Ui, name: &str) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(12.5),
        color::text_control(),
    );
    (galley.size().x + 16.0).round()
}

/// One of the four character formatting buttons. The letter is drawn with the formatting it applies, so
/// the button looks like what it does.
fn format_button(
    ui: &mut egui::Ui,
    area: Rect,
    letter: &str,
    name: &str,
    active: bool,
    bold_family: &egui::FontFamily,
) -> bool {
    let response =
        ui.interact(area, ui.id().with(("format", name)), Sense::click()).with_hint(name);
    if active {
        ui.painter().rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::accent());
    } else if response.hovered() {
        ui.painter().rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::control());
    }
    let tint = if active { color::on_accent() } else { color::text_control() };
    // The letter is drawn with the formatting it applies, so the button looks like what it does. Bold uses
    // the real bold face installed in `theme::install_fonts`, because egui's built in font has none.
    let font_id = if name == "Bold" {
        egui::FontId::new(13.5, bold_family.clone())
    } else {
        egui::FontId::proportional(13.5)
    };
    let mut job = egui::text::LayoutJob::default();
    job.append(
        letter,
        0.0,
        egui::TextFormat {
            font_id,
            color: tint,
            italics: name == "Italic",
            underline: if name == "Underline" { Stroke::new(1.0, tint) } else { Stroke::NONE },
            strikethrough: if name == "Strikethrough" {
                Stroke::new(1.0, tint)
            } else {
                Stroke::NONE
            },
            ..Default::default()
        },
    );
    let painter = ui.painter();
    let galley = painter.layout_job(job);
    painter.galley(area.center() - galley.size() / 2.0, galley, tint);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), active, name)
    });
    response.clicked()
}

/// One of the three view mode buttons, drawn as a small picture of what that mode shows.
fn view_mode_button(
    rux: &mut rux::Rux<'_>,
    area: Rect,
    mode: crate::app::ViewMode,
    active: bool,
    kind: file_kind::PreviewKind,
) -> bool {
    let name = mode.label_for(kind);
    let response = rux
        .ui
        .interact(area, rux.ui.id().with(("view-mode", name)), Sense::click())
        .with_hint(mode.description_for(kind));
    let chat = rux.theme().chat;
    let ink = rux.theme().ink;
    if active {
        rux.chrome.surface(
            area,
            area.height() / 2.0,
            rux::components::chip(&chat, area),
            chat.raised_chip(),
        );
    } else if response.hovered() {
        rux.wash(area, area.height() / 2.0, rux.theme().interaction.hover);
    }
    let tint = match (active, response.hovered()) {
        (true, _) => ink.i900,
        (false, true) => chat.sky,
        (false, false) => ink.i400,
    };
    let mark = match mode {
        crate::app::ViewMode::Raw => rux::Icon::Lines,
        crate::app::ViewMode::SideBySide => rux::Icon::Halves,
        crate::app::ViewMode::Preview => rux::Icon::Eye,
    };
    rux.mark(rux::Mark::new(mark, 16.0), area.center(), tint);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, name));
    response.clicked()
}

/// One of the four alignment buttons, drawn as a small picture of a paragraph placed that way.
fn alignment_button(ui: &mut egui::Ui, area: Rect, align: Align, active: bool) -> bool {
    let name = align.label();
    let response = ui
        .interact(area, ui.id().with(("align", name)), Sense::click())
        .with_hint(format!("Align {}", name.to_lowercase()));
    let painter = ui.painter();
    if active {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::accent());
    } else if response.hovered() {
        painter.rect_filled(area, CornerRadius::same(size::CONTROL_CORNER), color::control());
    }
    let tint = if active { color::on_accent() } else { color::text_control() };
    icon::alignment(painter, area.shrink2(Vec2::new(7.0, 9.0)), align, tint);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), active, name)
    });
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_file_with_no_tools_takes_no_room_in_the_title_bar() {
        for code in ["main.rs", "Cargo.toml", "photo.png"] {
            assert_eq!(width(Some(Path::new(code))), 0.0, "{code} has no text tools");
        }
    }

    #[test]
    fn markdown_gets_the_f_and_the_three_view_modes_and_a_text_file_gets_only_the_f() {
        let markdown = width(Some(Path::new("notes.md")));
        let plain = width(Some(Path::new("notes.txt")));
        assert_eq!(plain, BUTTON, "a text file has nothing to preview, so it is the F alone");
        assert_eq!(markdown, BUTTON + GROUP_GAP + 3.0 * MODE.x + 2.0 * MODE_GAP + 2.0 * TRACK_PAD);
        assert!(markdown > plain);
        // A document that has never been saved is treated as the beginning of a Markdown file, which
        // is what `file_kind::preview_applies` already decides for the View menu.
        assert_eq!(width(None), markdown);
    }
}
