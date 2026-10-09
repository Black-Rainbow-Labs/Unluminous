//! Everything a person or an agent can do to a notebook, as one list of actions, and what each does.
//!
//! The `Notebook` menu, the toolbar across a notebook, the buttons on a cell, the keys of both modes and
//! `unluminous-cli action run notebook-...` all produce a [`NotebookAction`], and
//! [`UnluminousApp::run_a_notebook_action`] is the one place one turns into a change — the rule
//! `UnluminousApp::run_action` keeps for the whole window. Every action is about the notebook tab that is
//! showing, and about its chosen cells: the range in command mode, or the cell holding the caret.

use unluminous_core::Command;
use unluminous_jupyter::nbformat::{self, CellKind};
use unluminous_jupyter::text;

use crate::app::actions::{Entry, Menu, MenuState, Shortcut};
use crate::app::notebook::{self, Deleted, Mode};
use crate::app::UnluminousApp;

/// One thing that can be done to a notebook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotebookAction {
    RunCell,
    RunCellSelectBelow,
    RunCellInsertBelow,
    RunAll,
    RunAbove,
    RunCellAndBelow,
    DebugCell,
    Interrupt,
    Restart,
    RestartRunAll,
    ShutDown,
    AddAbove(CellKind),
    AddBelow(CellKind),
    Delete,
    UndoDelete,
    Copy,
    Cut,
    PasteBelow,
    PasteAbove,
    Duplicate,
    MoveUp,
    MoveDown,
    MergeAbove,
    MergeBelow,
    MergeSelected,
    Split,
    Convert(CellKind),
    ClearOutput,
    ClearAllOutputs,
    CollapseCell,
    CollapseOutput,
    ToggleLineNumbers,
    ToggleVariables,
    CommandMode,
    EditMode,
    SelectAbove,
    SelectBelow,
    ExtendAbove,
    ExtendBelow,
    SelectFirst,
    SelectLast,
    RenderMarkdown,
    PreviousSection,
    NextSection,
    CommentCells,
}

/// Every action, in the order the menu and `action list` give them. Used by the names' test.
pub const ALL: &[NotebookAction] = &[
    NotebookAction::RunCell,
    NotebookAction::RunCellSelectBelow,
    NotebookAction::RunCellInsertBelow,
    NotebookAction::RunAll,
    NotebookAction::RunAbove,
    NotebookAction::RunCellAndBelow,
    NotebookAction::DebugCell,
    NotebookAction::Interrupt,
    NotebookAction::Restart,
    NotebookAction::RestartRunAll,
    NotebookAction::ShutDown,
    NotebookAction::AddAbove(CellKind::Code),
    NotebookAction::AddBelow(CellKind::Code),
    NotebookAction::AddAbove(CellKind::Markdown),
    NotebookAction::AddBelow(CellKind::Markdown),
    NotebookAction::AddAbove(CellKind::Raw),
    NotebookAction::AddBelow(CellKind::Raw),
    NotebookAction::Delete,
    NotebookAction::UndoDelete,
    NotebookAction::Copy,
    NotebookAction::Cut,
    NotebookAction::PasteBelow,
    NotebookAction::PasteAbove,
    NotebookAction::Duplicate,
    NotebookAction::MoveUp,
    NotebookAction::MoveDown,
    NotebookAction::MergeAbove,
    NotebookAction::MergeBelow,
    NotebookAction::MergeSelected,
    NotebookAction::Split,
    NotebookAction::Convert(CellKind::Code),
    NotebookAction::Convert(CellKind::Markdown),
    NotebookAction::Convert(CellKind::Raw),
    NotebookAction::ClearOutput,
    NotebookAction::ClearAllOutputs,
    NotebookAction::CollapseCell,
    NotebookAction::CollapseOutput,
    NotebookAction::ToggleLineNumbers,
    NotebookAction::ToggleVariables,
    NotebookAction::CommandMode,
    NotebookAction::EditMode,
    NotebookAction::SelectAbove,
    NotebookAction::SelectBelow,
    NotebookAction::ExtendAbove,
    NotebookAction::ExtendBelow,
    NotebookAction::SelectFirst,
    NotebookAction::SelectLast,
    NotebookAction::RenderMarkdown,
    NotebookAction::PreviousSection,
    NotebookAction::NextSection,
    NotebookAction::CommentCells,
];

impl NotebookAction {
    /// The name the command line calls it, after the `notebook-` that `Action::name` puts in front.
    pub fn name(&self) -> String {
        let kind = |kind: &CellKind| kind.name().to_owned();
        match self {
            NotebookAction::RunCell => "run-cell".into(),
            NotebookAction::RunCellSelectBelow => "run-cell-select-below".into(),
            NotebookAction::RunCellInsertBelow => "run-cell-insert-below".into(),
            NotebookAction::RunAll => "run-all".into(),
            NotebookAction::RunAbove => "run-above".into(),
            NotebookAction::RunCellAndBelow => "run-cell-and-below".into(),
            NotebookAction::DebugCell => "debug-cell".into(),
            NotebookAction::Interrupt => "interrupt".into(),
            NotebookAction::Restart => "restart".into(),
            NotebookAction::RestartRunAll => "restart-run-all".into(),
            NotebookAction::ShutDown => "shut-down".into(),
            NotebookAction::AddAbove(what) => format!("add-{}-above", kind(what)),
            NotebookAction::AddBelow(what) => format!("add-{}-below", kind(what)),
            NotebookAction::Delete => "delete-cell".into(),
            NotebookAction::UndoDelete => "undo-delete-cell".into(),
            NotebookAction::Copy => "copy-cell".into(),
            NotebookAction::Cut => "cut-cell".into(),
            NotebookAction::PasteBelow => "paste-cell-below".into(),
            NotebookAction::PasteAbove => "paste-cell-above".into(),
            NotebookAction::Duplicate => "duplicate-cell".into(),
            NotebookAction::MoveUp => "move-cell-up".into(),
            NotebookAction::MoveDown => "move-cell-down".into(),
            NotebookAction::MergeAbove => "merge-cell-above".into(),
            NotebookAction::MergeBelow => "merge-cell-below".into(),
            NotebookAction::MergeSelected => "merge-selected-cells".into(),
            NotebookAction::Split => "split-cell".into(),
            NotebookAction::Convert(what) => format!("convert-to-{}", kind(what)),
            NotebookAction::ClearOutput => "clear-output".into(),
            NotebookAction::ClearAllOutputs => "clear-all-outputs".into(),
            NotebookAction::CollapseCell => "collapse-cell".into(),
            NotebookAction::CollapseOutput => "collapse-output".into(),
            NotebookAction::ToggleLineNumbers => "toggle-line-numbers".into(),
            NotebookAction::ToggleVariables => "toggle-variables".into(),
            NotebookAction::CommandMode => "command-mode".into(),
            NotebookAction::EditMode => "edit-mode".into(),
            NotebookAction::SelectAbove => "select-cell-above".into(),
            NotebookAction::SelectBelow => "select-cell-below".into(),
            NotebookAction::ExtendAbove => "extend-selection-above".into(),
            NotebookAction::ExtendBelow => "extend-selection-below".into(),
            NotebookAction::SelectFirst => "select-first-cell".into(),
            NotebookAction::SelectLast => "select-last-cell".into(),
            NotebookAction::RenderMarkdown => "render-markdown".into(),
            NotebookAction::PreviousSection => "previous-section".into(),
            NotebookAction::NextSection => "next-section".into(),
            NotebookAction::CommentCells => "comment-cells".into(),
        }
    }

    /// The action of this name.
    pub fn from_name(name: &str) -> Option<NotebookAction> {
        ALL.iter().copied().find(|action| action.name() == name)
    }
}

/// The `Notebook` menu, which is in the bar only while a notebook tab is showing.
///
/// The shortcuts are the reference editor's, and each entry is marked as not watched by the menu bar: a notebook
/// reads its own keys, only while it has the keyboard, because `Shift+Enter` typed into a terminal must
/// stay a new line in the terminal. See `app::notebook_frame`.
pub fn notebook_menu(state: &MenuState) -> Option<Menu> {
    if !state.notebook_showing {
        return None;
    }
    let item = |name: &str, what: NotebookAction| {
        Entry::item(name, crate::app::actions::Action::Notebook(what))
    };
    let keyed = |name: &str, what: NotebookAction, shortcut: Shortcut| {
        Entry::with_shortcut(name, crate::app::actions::Action::Notebook(what), shortcut)
            .not_from_the_keyboard()
    };
    let enter = |command: bool, shift: bool, alt: bool| Shortcut {
        key: egui::Key::Enter,
        command,
        shift,
        alt,
        ctrl: false,
    };
    let running = state.notebook_kernel_running;
    let entries = vec![
        keyed("Run Cell", NotebookAction::RunCell, enter(true, false, false)),
        keyed(
            "Run Cell and Select Below",
            NotebookAction::RunCellSelectBelow,
            enter(false, true, false),
        ),
        keyed(
            "Run Cell and Insert Below",
            NotebookAction::RunCellInsertBelow,
            enter(false, false, true),
        ),
        keyed("Run All", NotebookAction::RunAll, enter(true, true, true)),
        item("Run All Above", NotebookAction::RunAbove),
        item("Run Cell and Below", NotebookAction::RunCellAndBelow),
        keyed(
            "Debug Cell",
            NotebookAction::DebugCell,
            Shortcut { key: egui::Key::Enter, command: false, shift: true, alt: true, ctrl: false },
        ),
        Entry::Separator,
        item("Interrupt Kernel", NotebookAction::Interrupt).enabled(running),
        item("Restart Kernel", NotebookAction::Restart),
        item("Restart Kernel and Run All", NotebookAction::RestartRunAll),
        item("Shut Down Kernel", NotebookAction::ShutDown).enabled(running),
        Entry::Separator,
        keyed(
            "Code Cell Above",
            NotebookAction::AddAbove(CellKind::Code),
            Shortcut { key: egui::Key::A, command: false, shift: true, alt: true, ctrl: false },
        ),
        keyed(
            "Code Cell Below",
            NotebookAction::AddBelow(CellKind::Code),
            Shortcut { key: egui::Key::B, command: false, shift: true, alt: true, ctrl: false },
        ),
        item("Markdown Cell Above", NotebookAction::AddAbove(CellKind::Markdown)),
        item("Markdown Cell Below", NotebookAction::AddBelow(CellKind::Markdown)),
        Entry::Submenu { name: "Cell".to_owned(), entries: cell_entries() },
        Entry::Separator,
        item("Clear Output", NotebookAction::ClearOutput),
        item("Clear All Outputs", NotebookAction::ClearAllOutputs),
        item("Show Line Numbers in Cells", NotebookAction::ToggleLineNumbers)
            .checked(state.notebook_line_numbers),
        item("Variables", NotebookAction::ToggleVariables).checked(state.notebook_variables),
    ];
    Some(Menu { name: "Notebook".to_owned(), entries })
}

/// A cell's own menu, from the three dots on it: the operations on one cell, and its output.
pub fn cell_menu() -> Vec<Entry> {
    let item = |name: &str, what: NotebookAction| {
        Entry::item(name, crate::app::actions::Action::Notebook(what))
    };
    let mut entries = vec![
        item("Run Cell", NotebookAction::RunCell),
        item("Run All Above", NotebookAction::RunAbove),
        item("Run Cell and Below", NotebookAction::RunCellAndBelow),
        item("Clear Output", NotebookAction::ClearOutput),
        Entry::Separator,
    ];
    entries.extend(cell_entries());
    entries
}

/// The `Notebook -> Cell` submenu: every operation on the chosen cells.
fn cell_entries() -> Vec<Entry> {
    let item = |name: &str, what: NotebookAction| {
        Entry::item(name, crate::app::actions::Action::Notebook(what))
    };
    vec![
        item("Delete Cell", NotebookAction::Delete),
        item("Copy Cell", NotebookAction::Copy),
        item("Cut Cell", NotebookAction::Cut),
        item("Paste Cell Below", NotebookAction::PasteBelow),
        item("Paste Cell Above", NotebookAction::PasteAbove),
        item("Duplicate Cell", NotebookAction::Duplicate),
        Entry::Separator,
        item("Move Cell Up", NotebookAction::MoveUp),
        item("Move Cell Down", NotebookAction::MoveDown),
        item("Merge Cell Above", NotebookAction::MergeAbove),
        item("Merge Cell Below", NotebookAction::MergeBelow),
        item("Merge Selected Cells", NotebookAction::MergeSelected),
        Entry::with_shortcut(
            "Split Cell",
            crate::app::actions::Action::Notebook(NotebookAction::Split),
            Shortcut { key: egui::Key::Minus, command: true, shift: true, alt: false, ctrl: false },
        )
        .not_from_the_keyboard(),
        Entry::Separator,
        item("Convert to Code", NotebookAction::Convert(CellKind::Code)),
        item("Convert to Markdown", NotebookAction::Convert(CellKind::Markdown)),
        item("Convert to Raw", NotebookAction::Convert(CellKind::Raw)),
        Entry::Separator,
        item("Collapse Cell", NotebookAction::CollapseCell),
        item("Collapse Output", NotebookAction::CollapseOutput),
        item("Comment Out Cells", NotebookAction::CommentCells),
    ]
}

impl UnluminousApp {
    /// Do one notebook action to the notebook tab that is showing. Answers what happened, for the
    /// command line; a person sees it.
    pub(crate) fn run_a_notebook_action(&mut self, what: NotebookAction) -> Result<String, String> {
        let index = self.files.active_index();
        if self.files.active().notebook.is_none() {
            return Err("The tab that is showing is not a notebook.".to_owned());
        }
        self.refresh_the_notebook(index);
        let said = match what {
            NotebookAction::RunCell
            | NotebookAction::RunCellSelectBelow
            | NotebookAction::RunCellInsertBelow
            | NotebookAction::RunAll
            | NotebookAction::RunAbove
            | NotebookAction::RunCellAndBelow
            | NotebookAction::RenderMarkdown => self.run_from_an_action(index, what),
            NotebookAction::DebugCell => self.debug_the_chosen_cell(index),
            NotebookAction::Interrupt
            | NotebookAction::Restart
            | NotebookAction::RestartRunAll
            | NotebookAction::ShutDown => self.kernel_from_an_action(index, what),
            NotebookAction::CommandMode
            | NotebookAction::EditMode
            | NotebookAction::SelectAbove
            | NotebookAction::SelectBelow
            | NotebookAction::ExtendAbove
            | NotebookAction::ExtendBelow
            | NotebookAction::SelectFirst
            | NotebookAction::SelectLast
            | NotebookAction::PreviousSection
            | NotebookAction::NextSection => self.move_the_choice(index, what),
            NotebookAction::ClearOutput
            | NotebookAction::ClearAllOutputs
            | NotebookAction::CollapseCell
            | NotebookAction::CollapseOutput
            | NotebookAction::ToggleLineNumbers
            | NotebookAction::ToggleVariables => self.change_what_is_shown(index, what),
            _ => self.edit_the_cells(index, what),
        };
        self.refresh_the_notebook(index);
        said
    }

    /// Bring the tab's cells up to date with its text, repairing any marker the text lost an id from.
    pub(crate) fn refresh_the_notebook(&mut self, index: usize) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let text = file.document.text().to_string();
        let repairs = text::repairs(&text);
        if !repairs.is_empty() {
            let caret = file.document.selection().head;
            notebook::apply_edits(&mut file.document, repairs, Some(caret));
        }
        tab.refresh(&file.document);
        let count = tab.len();
        if let Mode::Command { anchor, head } = tab.mode {
            let last = count.saturating_sub(1);
            tab.mode = Mode::Command { anchor: anchor.min(last), head: head.min(last) };
        }
    }

    /// The chosen cells of the tab at `index`.
    pub(crate) fn chosen_cells(&self, index: usize) -> std::ops::Range<usize> {
        let file = self.files.at(index);
        match file.notebook.as_deref() {
            Some(tab) => tab.chosen(file.document.selection().head),
            None => 0..0,
        }
    }

    /// The running half: which cells run, and where the choice goes afterwards.
    fn run_from_an_action(&mut self, index: usize, what: NotebookAction) -> Result<String, String> {
        let chosen = self.chosen_cells(index);
        let count = self.files.at(index).notebook.as_deref().map(|tab| tab.len()).unwrap_or(0);
        let cells = match what {
            NotebookAction::RunAll => 0..count,
            NotebookAction::RunAbove => 0..chosen.start,
            NotebookAction::RunCellAndBelow => chosen.start..count,
            _ => chosen.clone(),
        };
        self.run_notebook_cells(index, cells.clone());
        match what {
            NotebookAction::RunCellSelectBelow => {
                if chosen.end >= count {
                    self.add_a_cell(index, count, CellKind::Code);
                } else {
                    self.choose_a_cell(index, chosen.end, true);
                }
            }
            NotebookAction::RunCellInsertBelow => {
                self.add_a_cell(index, chosen.end, CellKind::Code)
            }
            _ => {}
        }
        Ok(format!("Running {} cell(s).", cells.len()))
    }

    /// Interrupt, restart or shut down.
    fn kernel_from_an_action(
        &mut self,
        index: usize,
        what: NotebookAction,
    ) -> Result<String, String> {
        match what {
            NotebookAction::Interrupt => self.interrupt_the_kernel(index),
            NotebookAction::Restart => self.restart_the_kernel(index, false),
            NotebookAction::RestartRunAll => self.restart_the_kernel(index, true),
            _ => self.shut_down_the_kernel(index),
        }
        Ok(format!("Kernel: {}", what.name()))
    }

    /// Put the caret at the start of cell `cell`, in edit mode, or choose it in command mode.
    pub(crate) fn choose_a_cell(&mut self, index: usize, cell: usize, edit: bool) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let Some(span) = tab.spans.get(cell) else { return };
        let offset = span.body_bytes.start;
        file.document.apply(Command::PlaceCaret { offset, extend: false });
        tab.mode = match edit {
            true => Mode::Edit,
            false => Mode::Command { anchor: cell, head: cell },
        };
        if edit {
            if let Some(id) = tab.id_of(cell) {
                if tab.spans[cell].kind == CellKind::Markdown && tab.editing.insert(id) {
                    tab.bands_revision += 1;
                }
            }
        }
        self.reveal_caret = true;
    }

    /// Moving the choice, and the two modes.
    fn move_the_choice(&mut self, index: usize, what: NotebookAction) -> Result<String, String> {
        let chosen = self.chosen_cells(index);
        let Some(tab) = self.files.at(index).notebook.as_deref() else { return Err(String::new()) };
        let last = tab.len().saturating_sub(1);
        let (anchor, head) = match tab.mode {
            Mode::Command { anchor, head } => (anchor, head),
            Mode::Edit => (chosen.start, chosen.start),
        };
        let sections = section_starts(tab);
        let (anchor, head, edit) = match what {
            NotebookAction::EditMode => (head, head, true),
            NotebookAction::CommandMode => (head, head, false),
            NotebookAction::SelectAbove => (head.saturating_sub(1), head.saturating_sub(1), false),
            NotebookAction::SelectBelow => ((head + 1).min(last), (head + 1).min(last), false),
            NotebookAction::ExtendAbove => (anchor, head.saturating_sub(1), false),
            NotebookAction::ExtendBelow => (anchor, (head + 1).min(last), false),
            NotebookAction::SelectFirst => (0, 0, false),
            NotebookAction::SelectLast => (last, last, false),
            NotebookAction::PreviousSection => {
                let at = sections.iter().rev().find(|start| **start < head).copied().unwrap_or(0);
                (at, at, false)
            }
            _ => {
                let at = sections.iter().find(|start| **start > head).copied().unwrap_or(last);
                (at, at, false)
            }
        };
        self.choose_a_cell(index, head, edit);
        if let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() {
            if !edit {
                tab.mode = Mode::Command { anchor, head };
            }
        }
        Ok(format!("Cell {} of {}.", head + 1, last + 1))
    }

    /// Outputs, collapsing, and the two switches.
    fn change_what_is_shown(
        &mut self,
        index: usize,
        what: NotebookAction,
    ) -> Result<String, String> {
        let chosen = self.chosen_cells(index);
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return Err(String::new()) };
        let ids: Vec<String> = chosen.clone().filter_map(|cell| tab.id_of(cell)).collect();
        match what {
            NotebookAction::ClearOutput | NotebookAction::ClearAllOutputs => {
                let all = what == NotebookAction::ClearAllOutputs;
                let ids: Vec<String> = match all {
                    true => tab.model.cells.iter().map(|cell| cell.id.clone()).collect(),
                    false => ids,
                };
                for id in &ids {
                    if let Some(cell) = tab.index_of(id).and_then(|at| tab.model.cells.get_mut(at))
                    {
                        cell.outputs.clear();
                        cell.execution_count = None;
                    }
                    tab.runs.remove(id);
                    tab.outputs_changed(id);
                }
                file.document.note_a_change_outside_the_text();
            }
            NotebookAction::CollapseCell => toggle_each(&mut tab.collapsed, &ids),
            NotebookAction::CollapseOutput => toggle_each(&mut tab.outputs_collapsed, &ids),
            NotebookAction::ToggleLineNumbers => tab.line_numbers = !tab.line_numbers,
            _ => tab.variables_showing = !tab.variables_showing,
        }
        tab.bands_revision += 1;
        if what == NotebookAction::ToggleVariables {
            self.ask_for_the_variables(index);
        }
        Ok(what.name())
    }

    /// Every action that changes the text: adding, removing, moving, merging, splitting and converting
    /// cells, and the cell clipboard.
    fn edit_the_cells(&mut self, index: usize, what: NotebookAction) -> Result<String, String> {
        let chosen = self.chosen_cells(index);
        if chosen.is_empty()
            && !matches!(
                what,
                NotebookAction::AddAbove(_)
                    | NotebookAction::AddBelow(_)
                    | NotebookAction::PasteBelow
                    | NotebookAction::UndoDelete
            )
        {
            return Err("No cell is chosen.".to_owned());
        }
        match what {
            NotebookAction::AddAbove(kind) => self.add_a_cell(index, chosen.start, kind),
            NotebookAction::AddBelow(kind) => self.add_a_cell(index, chosen.end, kind),
            NotebookAction::Delete => self.delete_cells(index, chosen),
            NotebookAction::UndoDelete => self.bring_back_a_cell(index),
            NotebookAction::Copy => self.copy_cells(index, chosen),
            NotebookAction::Cut => {
                self.copy_cells(index, chosen.clone());
                self.delete_cells(index, chosen);
            }
            NotebookAction::PasteBelow => self.paste_cells(index, chosen.end),
            NotebookAction::PasteAbove => self.paste_cells(index, chosen.start),
            NotebookAction::Duplicate => {
                self.copy_cells(index, chosen.clone());
                self.paste_cells(index, chosen.end);
            }
            NotebookAction::MoveUp => self.move_cells(index, chosen, -1),
            NotebookAction::MoveDown => self.move_cells(index, chosen, 1),
            NotebookAction::MergeAbove => {
                self.merge_cells(index, chosen.start.saturating_sub(1)..chosen.end)
            }
            NotebookAction::MergeBelow => {
                let count =
                    self.files.at(index).notebook.as_deref().map(|tab| tab.len()).unwrap_or(0);
                self.merge_cells(index, chosen.start..(chosen.end + 1).min(count))
            }
            NotebookAction::MergeSelected => self.merge_cells(index, chosen),
            NotebookAction::Split => self.split_the_cell(index),
            NotebookAction::Convert(kind) => self.convert_cells(index, chosen, kind),
            NotebookAction::CommentCells => self.comment_cells(index, chosen),
            _ => {}
        }
        Ok(what.name())
    }

    /// Add an empty cell of `kind` before cell `at`, and put the caret in it.
    pub(crate) fn add_a_cell(&mut self, index: usize, at: usize, kind: CellKind) {
        self.add_a_cell_holding(index, at, kind, "");
    }

    /// Add a cell of `kind` holding `source` before cell `at`, as one undo step, and put the caret at
    /// the end of it. Answers the new cell's id.
    pub(crate) fn add_a_cell_holding(
        &mut self,
        index: usize,
        at: usize,
        kind: CellKind,
        source: &str,
    ) -> String {
        let id = nbformat::new_id();
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return id };
        let length = file.document.text().len_bytes();
        let (edits, caret) = notebook::insert_cell(&tab.spans, length, at, kind, &id, source);
        notebook::apply_edits(&mut file.document, edits, Some(caret));
        if kind == CellKind::Markdown && source.is_empty() {
            tab.editing.insert(id.clone());
        }
        tab.mode = Mode::Edit;
        self.refresh_the_notebook(index);
        self.reveal_caret = true;
        id
    }

    /// Delete cells `cells`, keeping them so `Z` can bring them back. The last cell is never deleted:
    /// it is emptied instead, because a notebook with no cells has nowhere to type.
    fn delete_cells(&mut self, index: usize, cells: std::ops::Range<usize>) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let text = file.document.text().to_string();
        if cells.len() >= tab.len() {
            let edits =
                vec![(0..text.len(), text::marker(CellKind::Code, &nbformat::new_id()) + "\n")];
            notebook::apply_edits(&mut file.document, edits, None);
        } else {
            for cell in cells.clone().rev() {
                tab.deleted
                    .push(Deleted { at: cell, text: notebook::cell_text(&text, &tab.spans[cell]) });
            }
            let bytes = notebook::cells_bytes(&tab.spans, cells.clone(), text.len());
            let caret = bytes.start.min(text.len() - bytes.len());
            notebook::apply_edits(&mut file.document, vec![(bytes, String::new())], Some(caret));
        }
        tab.mode = Mode::Command { anchor: cells.start, head: cells.start };
        self.refresh_the_notebook(index);
        let last = self
            .files
            .at(index)
            .notebook
            .as_deref()
            .map(|tab| tab.len().saturating_sub(1))
            .unwrap_or(0);
        self.choose_a_cell(index, cells.start.min(last), false);
    }

    /// Put back the cell deleted most recently, where it was.
    fn bring_back_a_cell(&mut self, index: usize) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let Some(deleted) = tab.deleted.pop() else { return };
        let length = file.document.text().len_bytes();
        let edit = match tab.spans.get(deleted.at) {
            Some(span) => (
                notebook::span_start(span)..notebook::span_start(span),
                format!("{}\n", deleted.text),
            ),
            None => (length..length, format!("\n{}", deleted.text)),
        };
        notebook::apply_edits(&mut file.document, vec![edit], None);
        self.refresh_the_notebook(index);
        self.choose_a_cell(index, deleted.at, false);
    }

    /// Copy cells `cells` to the notebook's own clipboard, and their text to the system's.
    fn copy_cells(&mut self, index: usize, cells: std::ops::Range<usize>) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let text = file.document.text().to_string();
        tab.clipboard = notebook::cells_copied(&text, &tab.spans, cells.clone());
        let copied = notebook::cells_text(&text, &tab.spans, cells);
        if let Some(context) = &self.context {
            context.copy_text(copied);
        }
    }

    /// Paste the notebook's clipboard as new cells before cell `at`.
    fn paste_cells(&mut self, index: usize, at: usize) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        if tab.clipboard.is_empty() {
            return;
        }
        let pasted = notebook::with_fresh_ids(&tab.clipboard);
        let length = file.document.text().len_bytes();
        let edit = match tab.spans.get(at) {
            Some(span) => {
                (notebook::span_start(span)..notebook::span_start(span), format!("{pasted}\n"))
            }
            None => (length..length, format!("\n{pasted}")),
        };
        notebook::apply_edits(&mut file.document, vec![edit], None);
        self.refresh_the_notebook(index);
        self.choose_a_cell(index, at, false);
    }

    /// Move cells `cells` one place up or down, keeping them chosen.
    fn move_cells(&mut self, index: usize, cells: std::ops::Range<usize>, by: i32) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let text = file.document.text().to_string();
        let Some(edit) = notebook::move_cells(&text, &tab.spans, cells.clone(), by) else { return };
        let edit_mode = tab.mode == Mode::Edit;
        notebook::apply_edits(&mut file.document, vec![edit], None);
        self.refresh_the_notebook(index);
        let start = (cells.start as i64 + i64::from(by)).max(0) as usize;
        self.choose_a_cell(index, start, edit_mode);
        if let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() {
            if !edit_mode {
                tab.mode = Mode::Command { anchor: start, head: start + cells.len() - 1 };
            }
        }
    }

    /// Merge cells `cells` into the first of them.
    fn merge_cells(&mut self, index: usize, cells: std::ops::Range<usize>) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let text = file.document.text().to_string();
        let Some(edit) = notebook::merge_cells(&text, &tab.spans, cells.clone()) else { return };
        notebook::apply_edits(&mut file.document, vec![edit], None);
        self.refresh_the_notebook(index);
        self.choose_a_cell(index, cells.start, false);
    }

    /// Split the caret's cell at the caret: what is after it becomes a new cell of the same kind.
    fn split_the_cell(&mut self, index: usize) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let caret = file.document.selection().head;
        let Some(cell) = tab.cell_at_offset(caret) else { return };
        let span = &tab.spans[cell];
        if caret < span.body_bytes.start {
            return;
        }
        let marker = text::marker(span.kind, &nbformat::new_id());
        let inserted = format!("\n{marker}\n");
        let after = caret + inserted.len();
        notebook::apply_edits(&mut file.document, vec![(caret..caret, inserted)], Some(after));
        tab.mode = Mode::Edit;
        self.refresh_the_notebook(index);
    }

    /// Make cells `cells` cells of `kind`. A code cell made Markdown loses its outputs, as in Jupyter.
    fn convert_cells(&mut self, index: usize, cells: std::ops::Range<usize>, kind: CellKind) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let text = file.document.text().to_string();
        let edits = notebook::change_kind(&text, &tab.spans, cells, kind);
        let caret = file.document.selection().head;
        notebook::apply_edits(&mut file.document, edits, Some(caret));
        self.refresh_the_notebook(index);
    }

    /// Comment out every line of cells `cells` with `#`, or take the comments off again.
    fn comment_cells(&mut self, index: usize, cells: std::ops::Range<usize>) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let (Some(first), Some(last)) = (tab.spans.get(cells.start), tab.spans.get(cells.end - 1))
        else {
            return;
        };
        let (start, end) = (first.body_bytes.start, last.body_bytes.end);
        let caret = file.document.selection();
        file.document.apply(Command::PlaceCaret { offset: start, extend: false });
        file.document.apply(Command::PlaceCaret { offset: end, extend: true });
        file.document.apply(Command::ToggleLineComment { marker: "# ".to_owned() });
        let length = file.document.text().len_bytes();
        file.document.apply(Command::PlaceCaret { offset: caret.head.min(length), extend: false });
    }
}

/// The cells that start a section: every Markdown cell whose source begins with a heading.
pub fn section_starts(tab: &notebook::NotebookTab) -> Vec<usize> {
    tab.model
        .cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| {
            cell.kind == CellKind::Markdown && cell.source.trim_start().starts_with('#')
        })
        .map(|(at, _)| at)
        .collect()
}

/// Turn each id in or out of a set: every one in when any was out, otherwise every one out.
fn toggle_each(set: &mut std::collections::HashSet<String>, ids: &[String]) {
    let any_out = ids.iter().any(|id| !set.contains(id));
    for id in ids {
        match any_out {
            true => set.insert(id.clone()),
            false => set.remove(id),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_notebook_action_has_a_name_that_reads_back_as_itself() {
        let mut seen = std::collections::HashSet::new();
        for action in ALL {
            let name = action.name();
            assert!(seen.insert(name.clone()), "{name} is used twice");
            assert_eq!(NotebookAction::from_name(&name), Some(*action), "{name}");
        }
    }
}
