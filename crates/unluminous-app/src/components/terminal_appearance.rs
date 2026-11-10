//! The terminal appearance picker: Termoil's themes on the left, its shaders in the middle and its cursor
//! effects on the right, in one modal, the way Termoil's own theme switcher lays them out (`task-2601`).
//!
//! Each column is a [`CommandPalette`] of its own, so it filters and ranks the way every other list in the window
//! does. One field filters the column that has the keys. Left and Right (or Tab) move between the columns, Up
//! and Down move through the one that has the keys, and every terminal is drawn with all three rows that are
//! chosen as they move. Enter or `Keep` keeps all three; Escape or the cross puts the terminals back the way they
//! were. The window owns the previews and the settings; this draws the modal and says what happened.

use egui::{Pos2, Rect, Vec2};

use crate::app::actions::Action;
use crate::components::command_palette::{rows, Command, CommandPalette, Words};
use crate::components::{controls, modal};
use crate::theme::color;

/// The modal's id, which is also what `modal open` and `modal state` call its rectangle by.
pub const ID: &str = "unluminous-terminal-appearance";

/// How large the modal is before anything has been dragged: wide enough for three lists side by side.
const WIDTH: f32 = 980.0;
const HEIGHT: f32 = 560.0;

/// One of the three lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Theme,
    Shader,
    Cursor,
}

impl Column {
    /// The three, left to right.
    pub const ALL: [Column; 3] = [Column::Theme, Column::Shader, Column::Cursor];

    /// Where it is, counted from the left.
    pub fn index(self) -> usize {
        match self {
            Column::Theme => 0,
            Column::Shader => 1,
            Column::Cursor => 2,
        }
    }

    /// What its heading says.
    pub fn title(self) -> &'static str {
        match self {
            Column::Theme => "Theme",
            Column::Shader => "Shader",
            Column::Cursor => "Cursor",
        }
    }

    /// The name `modal state` and `--column` use.
    pub fn name(self) -> &'static str {
        match self {
            Column::Theme => "theme",
            Column::Shader => "shader",
            Column::Cursor => "cursor",
        }
    }

    /// The column with this name, ignoring case.
    pub fn from_name(name: &str) -> Option<Column> {
        Column::ALL.into_iter().find(|column| column.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The share of the modal's width it takes. The themes have the longest names.
    fn share(self) -> f32 {
        match self {
            Column::Theme => 0.4,
            Column::Shader | Column::Cursor => 0.3,
        }
    }
}

/// What the three values were when the picker opened, so Escape can put them back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Kept {
    pub theme: String,
    pub shader: String,
    pub cursor: String,
}

/// The picker's state: the three lists, which one has the keys, and what to put back.
#[derive(Debug)]
pub struct AppearancePicker {
    pub column: Column,
    pub lists: [CommandPalette; 3],
    pub kept: Kept,
}

/// The three values the chosen rows name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chosen {
    pub theme: Option<String>,
    pub shader: Option<String>,
    pub cursor: Option<String>,
}

impl AppearancePicker {
    /// Open it over the three lists, with `column` holding the keys and each list on the row in use.
    ///
    /// @param lists - the theme, shader and cursor effect rows, each with the one in use marked `checked`
    /// @param column - the list that has the keys first
    /// @param kept - what the three settings are now
    pub fn open(lists: [Vec<Command>; 3], column: Column, kept: Kept) -> Self {
        let [themes, shaders, cursors] = lists;
        let mut lists = [
            CommandPalette::open_with(themes, Words::THEMES, usize::MAX),
            CommandPalette::open_with(shaders, Words::SHADERS, usize::MAX),
            CommandPalette::open_with(cursors, Words::CURSOR_EFFECTS, usize::MAX),
        ];
        for list in &mut lists {
            if let Some(at) = list.results().iter().position(|found| found.command.checked) {
                list.chosen = at;
            }
        }
        Self { column, lists, kept }
    }

    /// The list that has the keys.
    pub fn active(&mut self) -> &mut CommandPalette {
        &mut self.lists[self.column.index()]
    }

    /// Give the keys to the next column to the right, or the left, stopping at either end.
    pub fn move_column(&mut self, right: bool) {
        let at = self.column.index();
        let next = match right {
            true => (at + 1).min(2),
            false => at.saturating_sub(1),
        };
        self.column = Column::ALL[next];
    }

    /// What the chosen row of each list names. A list whose filter matches nothing names nothing.
    pub fn chosen(&self) -> Chosen {
        let named = |list: &CommandPalette| {
            list.chosen_command().and_then(|command| match &command.action {
                Action::SetTerminalTheme(name)
                | Action::SetTerminalShader(name)
                | Action::SetTerminalCursorEffect(name) => Some(name.clone()),
                _ => None,
            })
        };
        Chosen {
            theme: named(&self.lists[0]),
            shader: named(&self.lists[1]),
            cursor: named(&self.lists[2]),
        }
    }
}

/// What drawing the picker came to.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Keep the three chosen rows and shut the modal.
    pub keep: bool,
    /// Shut the modal and put the terminals back.
    pub close: bool,
}

/// Draw the modal. The window owns whether there is one at all.
///
/// @param ctx - the window's context
/// @param picker - the picker's state
pub fn show(ctx: &egui::Context, picker: &mut AppearancePicker) -> Outcome {
    let mut outcome = Outcome::default();
    for list in &mut picker.lists {
        list.refresh();
    }
    let (_, closed) = modal::show(ctx, ID, WIDTH, HEIGHT, |ui, area| {
        if modal::header(ui, area, "Terminal Appearance") {
            outcome.close = true;
        }
        let enter = take_the_keys(ui, picker);
        let body = modal::body(area);
        let field = Rect::from_min_size(body.min, Vec2::new(body.width(), 30.0));
        let words = picker.active().words;
        let entry =
            controls::search_field(ui, field, words.field, words.hint, &mut picker.active().query);
        if !entry.has_focus() {
            entry.request_focus();
        }
        let columns = Rect::from_min_max(Pos2::new(body.left(), field.bottom() + 12.0), body.max);
        let taken = draw_the_columns(ui, columns, picker);
        if enter || taken {
            outcome.keep = true;
        }
        hint(ui, area, picker);
        if modal::footer(ui, area, &[("Keep", true)]) == Some(0) {
            outcome.keep = true;
        }
    });
    if closed {
        outcome.close = true;
    }
    if outcome.keep {
        outcome.close = true;
    }
    outcome
}

/// Take the arrow keys, Tab and Enter out of the frame before the field is drawn, which is the command palette's
/// own rule: a field leaves the events it read in the list for everyone else. Answers whether Enter was pressed.
fn take_the_keys(ui: &mut egui::Ui, picker: &mut AppearancePicker) -> bool {
    let (down, up, left, right, tab, back, enter) = ui.input_mut(|input| {
        let none = egui::Modifiers::NONE;
        (
            input.consume_key(none, egui::Key::ArrowDown),
            input.consume_key(none, egui::Key::ArrowUp),
            input.consume_key(none, egui::Key::ArrowLeft),
            input.consume_key(none, egui::Key::ArrowRight),
            input.consume_key(none, egui::Key::Tab),
            input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab),
            input.consume_key(none, egui::Key::Enter),
        )
    });
    if down {
        picker.active().move_choice(1);
    }
    if up {
        picker.active().move_choice(-1);
    }
    if right || tab {
        picker.move_column(true);
    }
    if left || back {
        picker.move_column(false);
    }
    enter
}

/// The three lists side by side, each under its heading. A press in a list gives it the keys. Answers whether a
/// row was double clicked, which keeps.
fn draw_the_columns(ui: &mut egui::Ui, area: Rect, picker: &mut AppearancePicker) -> bool {
    let gap = 14.0;
    let room = area.width() - gap * 2.0;
    let mut left = area.left();
    let mut taken = false;
    for column in Column::ALL {
        let width = room * column.share();
        let rect =
            Rect::from_min_size(Pos2::new(left, area.top()), Vec2::new(width, area.height()));
        left += width + gap;
        let active = picker.column == column;
        heading(ui, rect, column, active, &picker.lists[column.index()]);
        let list = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + 26.0), rect.max);
        let pressed = ui.input(|input| {
            input.pointer.any_pressed()
                && input.pointer.interact_pos().is_some_and(|at| list.contains(at))
        });
        if pressed {
            picker.column = column;
        }
        let salt = format!("terminal-appearance-{}", column.name());
        taken |= rows(ui, list, &mut picker.lists[column.index()], &salt).is_some();
    }
    taken
}

/// A column's heading: its title, how many rows match, and a line under it while it has the keys.
fn heading(ui: &egui::Ui, rect: Rect, column: Column, active: bool, list: &CommandPalette) {
    let painter = ui.painter_at(rect);
    let band = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 22.0));
    let ink = if active { color::text_strong() } else { color::text_faint() };
    let title =
        painter.layout_no_wrap(column.title().to_owned(), egui::FontId::proportional(12.5), ink);
    let width = title.size().x;
    painter.galley(
        Pos2::new(band.left() + 4.0, band.center().y - title.size().y / 2.0),
        title,
        ink,
    );
    let count = match list.query.is_empty() {
        true => format!("{}", list.results().len()),
        false => format!("{} for \u{201C}{}\u{201D}", list.results().len(), list.query),
    };
    modal::label(&painter, band, band.left() + width + 14.0, &count, color::text_faint(), 11.0);
    if active {
        let line = Rect::from_min_max(
            Pos2::new(band.left(), band.bottom() - 2.0),
            Pos2::new(band.right(), band.bottom()),
        );
        painter.rect_filled(line, 1.0, color::accent());
    }
}

/// The footer's words: what the chosen shader or cursor effect looks like while its list has the keys, and how to
/// use the picker otherwise, which is Termoil's hint.
fn hint(ui: &egui::Ui, area: Rect, picker: &AppearancePicker) {
    let chosen = picker.lists[picker.column.index()].chosen_command();
    let words = match (picker.column, chosen) {
        (Column::Shader | Column::Cursor, Some(command)) if !command.menu.is_empty() => {
            format!("{}: {}", command.label, command.menu)
        }
        _ => "Left and Right switch lists, Up and Down preview, Enter keeps all, Escape restores"
            .to_owned(),
    };
    modal::label(
        &ui.painter_at(area),
        Rect::from_min_size(
            Pos2::new(area.left() + 20.0, area.bottom() - modal::FOOTER),
            Vec2::new(area.width() - 160.0, modal::FOOTER),
        ),
        area.left() + 20.0,
        &words,
        color::text_faint(),
        11.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, action: Action, checked: bool) -> Command {
        Command {
            name: action.name(),
            label: name.to_owned(),
            menu: String::new(),
            shortcut: String::new(),
            enabled: true,
            checked,
            action,
        }
    }

    fn picker() -> AppearancePicker {
        let themes = vec![
            row("Follow", Action::SetTerminalTheme(String::new()), false),
            row("Dracula", Action::SetTerminalTheme("Dracula".to_owned()), true),
        ];
        let shaders = vec![
            row("No Shader", Action::SetTerminalShader(String::new()), true),
            row("crt", Action::SetTerminalShader("crt".to_owned()), false),
        ];
        let cursors = vec![
            row("No Cursor Effect", Action::SetTerminalCursorEffect(String::new()), false),
            row("sonar", Action::SetTerminalCursorEffect("sonar".to_owned()), true),
        ];
        AppearancePicker::open([themes, shaders, cursors], Column::Shader, Kept::default())
    }

    #[test]
    fn each_list_opens_on_the_row_in_use_and_the_arrows_move_the_one_with_the_keys() {
        let mut picker = picker();
        let chosen = picker.chosen();
        assert_eq!(chosen.theme.as_deref(), Some("Dracula"));
        assert_eq!(chosen.shader.as_deref(), Some(""));
        assert_eq!(chosen.cursor.as_deref(), Some("sonar"));
        picker.active().move_choice(1);
        assert_eq!(picker.chosen().shader.as_deref(), Some("crt"), "the shader list had the keys");
        assert_eq!(
            picker.chosen().theme.as_deref(),
            Some("Dracula"),
            "and the themes did not move"
        );
        picker.move_column(true);
        assert_eq!(picker.column, Column::Cursor);
        picker.move_column(true);
        assert_eq!(picker.column, Column::Cursor, "nothing to the right of the cursor effects");
        picker.move_column(false);
        picker.move_column(false);
        picker.move_column(false);
        assert_eq!(picker.column, Column::Theme);
    }

    #[test]
    fn a_column_is_named_the_way_the_command_line_writes_it() {
        for column in Column::ALL {
            assert_eq!(Column::from_name(column.name()), Some(column));
        }
        assert_eq!(Column::from_name("Cursor"), Some(Column::Cursor));
        assert_eq!(Column::from_name("colour"), None);
    }
}
