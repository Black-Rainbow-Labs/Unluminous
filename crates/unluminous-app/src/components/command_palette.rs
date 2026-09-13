//! `Find Action`: type part of a menu entry's name, and run it. `task-1922` WP4.
//!
//! The reference editor calls it `Find Action` and every other editor calls it the command palette;
//! it is the same thing, and what it answers is the question `unluminous-cli action list` already
//! answers — *what can this window be asked to do* — put in front of a person instead of a script.
//!
//! **It is built the way [`crate::components::go_to_file`] is**, and that is the whole design: one
//! box, a list that narrows as you type, arrow keys and Enter. The ranking is
//! `services::file_search::score`, **the same scorer**, not a second one written for names — it is
//! already a case-insensitive subsequence with a bonus for letters that start a word, which is what
//! puts `Toggle Line Numbers` at the top of `tln`.
//!
//! **A dimmed entry is shown dimmed and refused.** `Redo` with nothing to redo is a real thing a
//! person is looking for, and a palette that hid it would answer "there is no such command" about a
//! command that plainly exists. What it does instead is say why it cannot be used just now.

use egui::{Pos2, Rect, Vec2};

use crate::app::actions::Action;
use crate::components::{controls, modal};
use crate::theme::color;

/// How large the modal is before anything has been dragged. Narrower than `Go to File`, because a
/// row here is a menu entry's wording and its chord rather than a name and a folder path.
const WIDTH: f32 = 620.0;
const HEIGHT: f32 = 460.0;

/// The most rows that are listed, which is `go_to_file`'s cap and is for the same reason.
const LIMIT: usize = 200;

/// One menu entry the palette can offer.
///
/// Flat rather than a borrowed `Entry`, because the menus are rebuilt from `MenuState` every time
/// they are asked for and the palette outlives the frame that opened it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The name `action run` takes, which is what is run and what the command line prints.
    pub name: String,
    /// The wording on the menu row, which is what a person is looking for.
    pub label: String,
    /// The menu it is on, shown after the wording so two rows called `Refresh` can be told apart.
    pub menu: String,
    /// The chord, spelled the way the menu spells it. Empty when it has none.
    pub shortcut: String,
    /// False when it cannot be used just now. The row is drawn dimmed and running it is refused.
    pub enabled: bool,
    /// True when the entry is switched on, such as the view mode that is showing.
    ///
    /// The palette draws nothing with it. It is here because this is the **one** walk of the menus
    /// there is — `action list` reads the same rows — and a second walk that carried one more field
    /// would be a second answer to what the menus hold.
    pub checked: bool,
    pub action: Action,
}

/// One row of the list: a command and how well it matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub command: Command,
    /// Which characters of the label matched, so the row can pick them out.
    pub hits: Vec<usize>,
}

/// What has been typed into the palette and which row is chosen.
#[derive(Debug, Default)]
pub struct CommandPalette {
    pub query: String,
    /// Which row is chosen, as a position in [`Self::results`].
    pub chosen: usize,
    commands: Vec<Command>,
    results: Vec<Match>,
    /// The query the results were worked out for, so they are worked out again only when it changes.
    searched: Option<String>,
    /// Set when the keyboard moved the choice, so the list scrolls to keep it in view.
    follow: bool,
}

impl CommandPalette {
    /// Open it over the entries the menus hold right now.
    pub fn open(commands: Vec<Command>) -> Self {
        let mut palette = Self { commands, ..Self::default() };
        palette.refresh();
        palette
    }

    /// Work the matches out again if the query has changed since the last time.
    ///
    /// Called by the window before the modal is drawn, which is `GoToFile::refresh`'s arrangement.
    pub fn refresh(&mut self) {
        if self.searched.as_deref() == Some(self.query.as_str()) {
            return;
        }
        self.results = rank(&self.commands, &self.query, LIMIT);
        self.searched = Some(self.query.clone());
        self.chosen = 0;
        self.follow = true;
    }

    /// The rows being offered, for a test and for `action find`.
    pub fn results(&self) -> &[Match] {
        &self.results
    }

    /// The command the chosen row holds, if there is one.
    pub fn chosen_command(&self) -> Option<&Command> {
        self.results.get(self.chosen).map(|found| &found.command)
    }

    fn move_choice(&mut self, by: i32) {
        if self.results.is_empty() {
            return;
        }
        let last = self.results.len() as i32 - 1;
        self.chosen = (self.chosen as i32 + by).clamp(0, last) as usize;
        self.follow = true;
    }
}

/// The commands matching `query`, best first and no more than `limit` of them.
///
/// **`services::file_search::score` and nothing else**, which is what `task-1922` §5.5 asks for: the
/// scorer already ranks a subsequence with bonuses for adjacency and for letters that start a word,
/// and a second implementation written for menu names would be a second set of ties to break.
///
/// The **label** is what is scored first, because that is what is on the screen and what a person is
/// looking for. A label that does not match is tried again as `<menu> <label>`, so `git commit`
/// finds `Commit...` on the Git menu, and then as the entry's own **name**, so `tln` finds
/// `toggle-line-numbers` — which is what somebody who has read `action list` types, and is the
/// spelling `action find` shares this ranking with. Both of those score lower than a match in the
/// wording, for the same reason `file_search` ranks a folder match below a name match, and both
/// carry no hits, because the characters they matched are not all in the label.
///
/// An empty query offers everything, in the order the menus hold it — which is
/// `completion::rank_all`'s rule: a palette that showed nothing until a letter was typed would hide
/// the answer to *what can this window do*.
pub fn rank(commands: &[Command], query: &str, limit: usize) -> Vec<Match> {
    let needle: Vec<char> = query.trim().to_lowercase().chars().collect();
    if needle.is_empty() {
        return commands
            .iter()
            .take(limit)
            .map(|command| Match { command: command.clone(), hits: Vec::new() })
            .collect();
    }
    let mut found: Vec<(i32, Match)> = Vec::new();
    for command in commands {
        let scored = match crate::services::file_search::score(&command.label, &needle) {
            Some((score, hits)) => Some((score + LABEL_BONUS, hits)),
            None => {
                let whole = format!("{} {}", command.menu, command.label);
                crate::services::file_search::score(&whole, &needle)
                    .or_else(|| crate::services::file_search::score(&command.name, &needle))
                    .map(|(score, _)| (score, Vec::new()))
            }
        };
        if let Some((score, hits)) = scored {
            found.push((score, Match { command: command.clone(), hits }));
        }
    }
    // Best first, and a tie broken by the shorter wording, which is nearly always the one meant.
    // `file_search::find`'s own comparator, over a label rather than a file name.
    found.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.command.label.len().cmp(&b.1.command.label.len()))
            .then_with(|| a.1.command.name.cmp(&b.1.command.name))
    });
    found.truncate(limit);
    found.into_iter().map(|(_, row)| row).collect()
}

/// What a match in the wording is worth over one that needed the menu's name as well.
///
/// `file_search::NAME_BONUS`'s counterpart, and the same number, because it is the same decision:
/// what is typed is nearly always the thing itself rather than where it lives.
const LABEL_BONUS: i32 = 400;

/// What the palette asked for this frame.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PaletteOutcome {
    /// Run this command and shut the modal.
    pub run: Option<Command>,
    /// The chosen row cannot be used just now; say so and leave the modal open.
    pub refused: Option<Command>,
    /// Shut the modal without running anything.
    pub close: bool,
}

/// Draw the modal. The window owns whether there is one at all.
pub fn show(ctx: &egui::Context, state: &mut CommandPalette) -> PaletteOutcome {
    let mut outcome = PaletteOutcome::default();
    let (_, closed) = modal::show(ctx, "unluminous-command-palette", WIDTH, HEIGHT, |ui, area| {
        if modal::header(ui, area, "Find Action") {
            outcome.close = true;
        }
        // The arrow keys and Enter are taken out of the frame's events before the field is drawn,
        // which is `go_to_file`'s own note: egui leaves the events a text box consumed in the list
        // for everyone else to read, and the list moving is not the same as a caret moving.
        let (down, up, enter) = ui.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            )
        });
        if down {
            state.move_choice(1);
        }
        if up {
            state.move_choice(-1);
        }

        let body = modal::body(area);
        let field = Rect::from_min_size(body.min, Vec2::new(body.width(), 30.0));
        let entry = controls::search_field(
            ui,
            field,
            "Find action",
            "Type part of a command",
            &mut state.query,
        );
        if !entry.has_focus() {
            entry.request_focus();
        }

        let list = Rect::from_min_max(Pos2::new(body.left(), field.bottom() + 10.0), body.max);
        let taken = rows(ui, list, state);
        let chosen = taken.or(enter.then_some(state.chosen));
        if let Some(index) = chosen {
            if let Some(row) = state.results.get(index) {
                match row.command.enabled {
                    true => outcome.run = Some(row.command.clone()),
                    false => outcome.refused = Some(row.command.clone()),
                }
            }
        }

        let count = state.results.len();
        let summary = match count {
            0 => "No command matches".to_owned(),
            1 => "1 command".to_owned(),
            many => format!("{many} commands"),
        };
        modal::label(
            &ui.painter_at(area),
            Rect::from_min_size(
                Pos2::new(area.left() + 20.0, area.bottom() - modal::FOOTER),
                Vec2::new(240.0, modal::FOOTER),
            ),
            area.left() + 20.0,
            &summary,
            color::text_faint(),
            11.0,
        );
        if modal::footer(ui, area, &[("RUN", state.chosen_command().is_some())]) == Some(0) {
            if let Some(command) = state.chosen_command().cloned() {
                match command.enabled {
                    true => outcome.run = Some(command),
                    false => outcome.refused = Some(command),
                }
            }
        }
    });
    if closed {
        outcome.close = true;
    }
    if outcome.run.is_some() {
        outcome.close = true;
    }
    outcome
}

/// The list of commands. Answers with the row that was double clicked.
fn rows(ui: &mut egui::Ui, area: Rect, state: &mut CommandPalette) -> Option<usize> {
    let mut taken = None;
    let mut chose = None;
    let mut follow_to = None;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    // Intersected rather than assigned, which is `components::explorer`'s rule: a clip written from
    // a component's own rectangle throws away whatever the caller had cut it to.
    child.set_clip_rect(child.clip_rect().intersect(area));
    egui::ScrollArea::vertical().id_salt("command-palette-rows").show(&mut child, |ui| {
        if state.results.is_empty() {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("  No command matches").size(11.5).color(color::text_faint()),
            );
            return;
        }
        for (index, found) in state.results.iter().enumerate() {
            let chosen = index == state.chosen;
            let command = found.command.clone();
            let hits = found.hits.clone();
            // The row's name carries the menu as well as the wording, because `Refresh` is on two
            // menus and two controls with one name cannot be told apart — by a person hearing them
            // read out, or by a test asking for one. `go_to_file`'s rule about a folder.
            let label = format!("Run {} on {}", command.label, command.menu);
            let response = modal::row(ui, index, &label, chosen, |painter, row| {
                let tint = match (command.enabled, chosen) {
                    (false, _) => color::text_faint(),
                    (true, true) => color::text_strong(),
                    (true, false) => color::text_control(),
                };
                let galley = controls::marked_text(
                    painter,
                    &command.label,
                    &hits,
                    tint,
                    egui::FontId::proportional(12.5),
                );
                let width = galley.size().x;
                painter.galley(
                    Pos2::new(row.left() + 20.0, row.center().y - galley.size().y / 2.0),
                    galley,
                    tint,
                );
                modal::label(
                    painter,
                    row,
                    row.left() + 20.0 + width + 14.0,
                    &command.menu,
                    color::text_faint(),
                    11.0,
                );
                if !command.shortcut.is_empty() {
                    let chord = painter.layout_no_wrap(
                        command.shortcut.clone(),
                        egui::FontId::proportional(11.0),
                        color::text_faint(),
                    );
                    painter.galley(
                        Pos2::new(
                            row.right() - 16.0 - chord.size().x,
                            row.center().y - chord.size().y / 2.0,
                        ),
                        chord,
                        color::text_faint(),
                    );
                }
            });
            if response.clicked() {
                chose = Some(index);
            }
            if response.double_clicked() {
                taken = Some(index);
            }
            if chosen && state.follow {
                follow_to = Some(response.rect);
            }
        }
    });
    if let Some(index) = chose {
        state.chosen = index;
    }
    if let Some(rect) = follow_to {
        child.scroll_to_rect(rect, None);
        state.follow = false;
    }
    taken
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(label: &str, menu: &str, enabled: bool) -> Command {
        Command {
            name: label.to_lowercase().replace(' ', "-"),
            label: label.to_owned(),
            menu: menu.to_owned(),
            shortcut: String::new(),
            enabled,
            checked: false,
            action: Action::About,
        }
    }

    fn labels(rows: &[Match]) -> Vec<String> {
        rows.iter().map(|row| row.command.label.clone()).collect()
    }

    #[test]
    fn an_empty_query_offers_every_command_in_the_order_the_menus_hold_it() {
        let commands =
            vec![command("Save", "File", true), command("Toggle Line Numbers", "View", true)];
        assert_eq!(labels(&rank(&commands, "", 50)), ["Save", "Toggle Line Numbers"]);
    }

    #[test]
    fn the_letters_are_a_subsequence_rather_than_a_substring() {
        let commands =
            vec![command("Save", "File", true), command("Toggle Line Numbers", "View", true)];
        assert_eq!(labels(&rank(&commands, "tln", 50)), ["Toggle Line Numbers"]);
    }

    #[test]
    fn the_menu_is_matched_when_the_wording_alone_does_not() {
        let commands = vec![command("Commit...", "Git", true), command("Save", "File", true)];
        assert_eq!(labels(&rank(&commands, "git commit", 50)), ["Commit..."]);
    }

    #[test]
    fn the_entrys_own_name_is_matched_when_neither_the_wording_nor_the_menu_does() {
        // `Show Line Numbers` holds no `t` at all, so somebody who read `action list` and typed the
        // name they saw there would otherwise be told there is no such command.
        let mut numbers = command("Show Line Numbers", "View", true);
        numbers.name = "toggle-line-numbers".to_owned();
        let commands = vec![numbers, command("Save", "File", true)];
        assert_eq!(labels(&rank(&commands, "tln", 50)), ["Show Line Numbers"]);
    }

    #[test]
    fn a_match_in_the_wording_outranks_one_that_needed_the_menu() {
        // `Find` is the wording of one row and the menu of the other, so without `LABEL_BONUS` the
        // two would be separated by the scorer's own tie breaks rather than by which one a person
        // typed the name of.
        let commands =
            vec![command("Find in Files...", "Find", true), command("Find...", "Find", true)];
        assert_eq!(labels(&rank(&commands, "find", 50))[0], "Find...");
    }

    #[test]
    fn a_command_that_cannot_be_used_is_still_offered() {
        // The palette is where somebody looks for a command they cannot see; leaving out the ones
        // that are dimmed would answer "there is no such command" about one that plainly exists.
        let commands = vec![command("Redo", "Edit", false)];
        let rows = rank(&commands, "redo", 50);
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].command.enabled);
    }

    #[test]
    fn the_arrow_keys_walk_the_list_and_stop_at_its_ends() {
        let mut palette = CommandPalette::open(vec![
            command("A", "Edit", true),
            command("B", "Edit", true),
            command("C", "Edit", true),
        ]);
        palette.move_choice(-1);
        assert_eq!(palette.chosen, 0, "there is nothing above the first row");
        for _ in 0..5 {
            palette.move_choice(1);
        }
        assert_eq!(palette.chosen, 2, "and nothing below the last");
    }

    #[test]
    fn a_new_query_starts_at_the_top_of_the_list() {
        let mut palette = CommandPalette::open(vec![
            command("Save", "File", true),
            command("Save As", "File", true),
        ]);
        palette.move_choice(1);
        palette.query = "save as".to_owned();
        palette.refresh();
        assert_eq!(palette.chosen, 0);
        assert_eq!(
            palette.chosen_command().map(|found| found.label.clone()),
            Some("Save As".to_owned())
        );
    }
}
