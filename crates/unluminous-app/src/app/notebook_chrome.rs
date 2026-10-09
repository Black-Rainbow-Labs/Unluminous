//! What a notebook tab draws round its cells: the toolbar across the top, the kernel it runs on, the
//! line offering to install `ipykernel` when the Python has none, and the variables down the right.
//!
//! The reference editor's notebook toolbar, in the reference editor's order: add a code cell, move the cell up and down, run
//! the cell and select the one below, run all, interrupt, restart, clear all outputs — then the cell
//! type, and at the right the kernel and what it is doing (the exploration notes of
//! `task-2220`, which photographed it).

use std::path::{Path, PathBuf};

use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};
use unluminous_jupyter::kernel::{KernelState, Python, Variable};
use unluminous_jupyter::nbformat::CellKind;

use crate::app::actions::Action;
use crate::app::notebook::{KernelSlot, Run};
use crate::app::notebook_actions::NotebookAction;
use crate::app::UnluminousApp;
use crate::components::controls;
use crate::components::controls::WithHint;
use crate::theme::{color, icon};

/// How tall the toolbar is, and the line under it offering to install `ipykernel`.
const TOOLBAR: f32 = 34.0;
const BANNER: f32 = 30.0;

/// What the kernel picker chose.
#[derive(Debug, Clone, PartialEq)]
enum KernelChoice {
    Python(PathBuf),
    Spec(String),
    Install(PathBuf),
    LookAgain,
}

impl UnluminousApp {
    /// A notebook tab: its toolbar, its cells, and its variables when they are showing. Answers whether
    /// the cells took the keyboard, as `show_editor` does.
    pub(crate) fn show_a_notebook(&mut self, ui: &mut egui::Ui, area: Rect, focused: bool) -> bool {
        let index = self.files.active_index();
        let toolbar = Rect::from_min_size(area.min, Vec2::new(area.width(), TOOLBAR));
        self.show_the_notebook_toolbar(ui, toolbar, index);
        let mut top = toolbar.bottom();
        if let Some((reason, missing)) = self.kernel_problem(index) {
            let banner =
                Rect::from_min_size(Pos2::new(area.left(), top), Vec2::new(area.width(), BANNER));
            self.show_the_kernel_problem(ui, banner, index, &reason, missing.as_deref());
            top = banner.bottom();
        }
        let showing =
            self.files.at(index).notebook.as_deref().is_some_and(|tab| tab.variables_showing);
        let side = if showing { (area.width() * 0.3).clamp(220.0, 440.0) } else { 0.0 };
        let cells = Rect::from_min_max(
            Pos2::new(area.left(), top),
            Pos2::new(area.right() - side, area.bottom()),
        );
        let took = self.show_editor(ui, cells, focused);
        if showing {
            let panel = Rect::from_min_max(Pos2::new(cells.right(), top), area.max);
            self.show_the_variables(ui, panel, index);
        }
        took
    }

    /// Why the notebook at `index` has no kernel, and which package is missing, when that is so.
    fn kernel_problem(&self, index: usize) -> Option<(String, Option<String>)> {
        match &self.files.at(index).notebook.as_deref()?.kernel {
            KernelSlot::Failed { reason, missing } => Some((reason.clone(), missing.clone())),
            _ => None,
        }
    }

    /// The toolbar across the top of a notebook.
    fn show_the_notebook_toolbar(&mut self, ui: &mut egui::Ui, area: Rect, index: usize) {
        ui.painter().rect_filled(area, CornerRadius::ZERO, color::toolbar());
        ui.painter().hline(area.x_range(), area.bottom() - 0.5, Stroke::new(1.0, color::divider()));
        let x = self.show_the_toolbar_buttons(ui, area) + 8.0;
        let kind_area = Rect::from_min_size(
            Pos2::new(x, area.top() + 5.0),
            Vec2::new(112.0, area.height() - 10.0),
        );
        self.show_the_cell_type(ui, kind_area, index);
        let outline_area = Rect::from_min_size(
            Pos2::new(kind_area.right() + 6.0, kind_area.top()),
            Vec2::new(104.0, kind_area.height()),
        );
        self.show_the_outline(ui, outline_area, index);
        let kind_area = kind_area.union(outline_area);
        let variables = Rect::from_center_size(
            Pos2::new(area.right() - 20.0, area.center().y),
            Vec2::splat(24.0),
        );
        if controls::icon_button(ui, variables, "Variables", icon::table) {
            self.notebook_wanted = Some(Action::Notebook(NotebookAction::ToggleVariables));
        }
        let kernel_area = Rect::from_min_max(
            Pos2::new((area.right() - 300.0).max(kind_area.right() + 8.0), area.top() + 5.0),
            Pos2::new(variables.left() - 6.0, area.bottom() - 5.0),
        );
        self.show_the_kernel_picker(ui, kernel_area, index);
        let between = Rect::from_min_max(
            Pos2::new(kind_area.right() + 12.0, area.top()),
            Pos2::new(kernel_area.left() - 8.0, area.bottom()),
        );
        self.show_what_is_running(ui, between, index);
    }

    /// The row of icon buttons at the left of the toolbar. Pressing one asks for its action. Answers
    /// where the row ends.
    fn show_the_toolbar_buttons(&mut self, ui: &mut egui::Ui, area: Rect) -> f32 {
        let mut x = area.left() + 8.0;
        for (what, name, draw) in toolbar_buttons() {
            let place =
                Rect::from_center_size(Pos2::new(x + 12.0, area.center().y), Vec2::splat(24.0));
            if controls::icon_button(ui, place, name, draw) {
                self.notebook_wanted = Some(Action::Notebook(what));
                self.focus = crate::app::Focus::Editor;
            }
            x += 28.0;
        }
        x
    }

    /// The chosen cell's kind, which choosing another converts it to.
    fn show_the_cell_type(&mut self, ui: &mut egui::Ui, area: Rect, index: usize) {
        let chosen = self.chosen_cells(index);
        let kind = self
            .files
            .at(index)
            .notebook
            .as_deref()
            .and_then(|tab| tab.spans.get(chosen.start))
            .map(|span| span.kind)
            .unwrap_or(CellKind::Code);
        let label = |kind: CellKind| match kind {
            CellKind::Code => "Code",
            CellKind::Markdown => "Markdown",
            CellKind::Raw => "Raw",
        };
        let picked = controls::dropdown(ui, area, label(kind), "Cell type", None, |ui| {
            let mut picked = None;
            for option in [CellKind::Code, CellKind::Markdown, CellKind::Raw] {
                if controls::menu_row(ui, label(option), "", true, option == kind, 0.0) {
                    picked = Some(option);
                }
            }
            picked
        });
        if let Some(option) = picked {
            self.notebook_wanted = Some(Action::Notebook(NotebookAction::Convert(option)));
        }
    }

    /// The notebook's outline: every Markdown heading, set in by its level, and every code cell by its
    /// first line. Choosing one goes to it. The reference editor's Structure view of a notebook.
    fn show_the_outline(&mut self, ui: &mut egui::Ui, area: Rect, index: usize) {
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        if std::mem::take(&mut tab.outline_wanted) {
            // `Notebook Outline` opens the dropdown's own popup, which `controls::dropdown` keeps
            // under its button's id with "popup" joined on.
            let button = ui.id().with(("dropdown", OUTLINE_NAME));
            egui::Popup::open_id(ui.ctx(), button.with("popup"));
        }
        let rows = outline_rows(&tab.model.cells);
        let picked = controls::dropdown(ui, area, "Outline", OUTLINE_NAME, None, |ui| {
            let mut picked = None;
            for (cell, depth, words) in &rows {
                let indent = 12.0 * *depth as f32;
                if controls::menu_row(ui, words, "", true, false, indent) {
                    picked = Some(*cell);
                }
            }
            picked
        });
        if let Some(cell) = picked {
            self.choose_a_cell(index, cell, false);
            self.focus = crate::app::Focus::Editor;
        }
    }

    /// What the notebook is doing, in the middle of the toolbar: the cell that is running, or a link to
    /// the cell that ran last, which is the reference editor's `Go to Cell 7`.
    fn show_what_is_running(&mut self, ui: &mut egui::Ui, area: Rect, index: usize) {
        let Some(tab) = self.files.at(index).notebook.as_deref() else { return };
        let running = tab.running.as_ref().and_then(|running| tab.index_of(&running.cell));
        let last = tab
            .runs
            .iter()
            .filter_map(|(id, run)| match run {
                Run::Done { at, .. } => tab.index_of(id).map(|cell| (cell, *at)),
                _ => None,
            })
            .max_by_key(|(_, at)| *at)
            .map(|(cell, _)| cell);
        let (words, cell) = match (running, last) {
            (Some(cell), _) => (format!("Cell {} is running", cell + 1), Some(cell)),
            (None, Some(cell)) => (format!("Go to Cell {}", cell + 1), Some(cell)),
            _ => return,
        };
        let font = egui::FontId::proportional(12.0);
        let width = ui.fonts_mut(|fonts| {
            fonts.layout_no_wrap(words.clone(), font.clone(), color::text_dim()).size().x
        });
        // Left out rather than drawn over its neighbours when the toolbar is too narrow for it.
        if width + 12.0 > area.width() {
            return;
        }
        let place = Rect::from_center_size(
            Pos2::new(area.right() - width / 2.0 - 4.0, area.center().y),
            Vec2::new(width + 8.0, 22.0),
        );
        let response = ui
            .interact(place, ui.id().with("notebook-go-to-cell"), egui::Sense::click())
            .with_hint(&words);
        let tint = if response.hovered() { color::accent() } else { color::text_dim() };
        ui.painter().text(place.center(), egui::Align2::CENTER_CENTER, &words, font, tint);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &words));
        if response.clicked() {
            if let Some(cell) = cell {
                self.choose_a_cell(index, cell, false);
            }
        }
    }

    /// The kernel the notebook runs on, and the list of Pythons and kernels to choose another from.
    fn show_the_kernel_picker(&mut self, ui: &mut egui::Ui, area: Rect, index: usize) {
        if area.width() < 60.0 {
            return;
        }
        self.look_for_pythons();
        let words = self.kernel_words(index);
        let pythons: Vec<Python> = self.pythons.found().to_vec();
        let python = self.notebook_python(index);
        if let Some(path) = &python {
            self.ask_for_the_kernelspecs(path);
        }
        let specs = python
            .as_ref()
            .and_then(|path| self.kernelspecs.get(path).cloned().flatten().and_then(Result::ok))
            .unwrap_or_default();
        let choice = controls::dropdown(ui, area, &words, "Kernel", None, |ui| {
            kernel_rows(ui, &pythons, python.as_deref(), &specs)
        });
        if let Some(choice) = choice {
            self.take_a_kernel_choice(index, choice);
        }
    }

    /// What the kernel picker says: the Python, the kernel and what it is doing.
    fn kernel_words(&self, index: usize) -> String {
        let Some(tab) = self.files.at(index).notebook.as_deref() else { return String::new() };
        let python =
            tab.python.as_deref().map(python_name).unwrap_or_else(|| "No kernel yet".to_owned());
        let state = match &tab.kernel {
            KernelSlot::NotStarted => "starts on the first run".to_owned(),
            KernelSlot::Failed { .. } => "could not start".to_owned(),
            KernelSlot::Live(kernel) => match kernel.state() {
                KernelState::Starting => "starting".to_owned(),
                KernelState::Idle => "idle".to_owned(),
                KernelState::Busy => "busy".to_owned(),
                KernelState::Restarting => "restarting".to_owned(),
                KernelState::Stopped => "shut down".to_owned(),
                KernelState::Dead(_) => "died".to_owned(),
            },
        };
        format!("{python} \u{00B7} {state}")
    }

    /// Ask, on a thread, which kernels the Python at `python` has, once.
    pub(crate) fn ask_for_the_kernelspecs(&mut self, python: &Path) {
        if self.kernelspecs.contains_key(python) {
            return;
        }
        self.kernelspecs.insert(python.to_path_buf(), None);
        let found = std::sync::Arc::new(std::sync::Mutex::new(None));
        let (path, wake) = (python.to_path_buf(), self.thread_waker());
        let into = found.clone();
        std::thread::spawn(move || {
            let specs = unluminous_jupyter::kernel::list_kernelspecs(&path);
            *into.lock().expect("not poisoned") = Some((path, specs));
            wake();
        });
        self.kernelspec_answers.push(found);
    }

    /// Take the kernelspec lists the threads have finished.
    pub(crate) fn take_the_kernelspecs(&mut self) {
        let mut waiting = Vec::new();
        for answer in self.kernelspec_answers.drain(..) {
            let taken = answer.lock().expect("not poisoned").take();
            match taken {
                Some((path, specs)) => {
                    self.kernelspecs.insert(path, Some(specs));
                }
                None => waiting.push(answer),
            }
        }
        self.kernelspec_answers = waiting;
    }

    /// Act on what the kernel picker chose.
    fn take_a_kernel_choice(&mut self, index: usize, choice: KernelChoice) {
        match choice {
            KernelChoice::Python(path) => self.choose_a_python(index, &path),
            KernelChoice::Spec(name) => self.choose_a_kernelspec(index, &name),
            KernelChoice::Install(path) => self.install_ipykernel(index, &path),
            KernelChoice::LookAgain => {
                self.pythons = crate::app::notebook_kernel::Pythons::NotLooked;
                self.kernelspecs.clear();
                self.look_for_pythons();
            }
        }
    }

    /// Run the notebook at `index` on the Python at `path` from now on, restarting its kernel there.
    pub(crate) fn choose_a_python(&mut self, index: usize, path: &Path) {
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        tab.python = Some(path.to_path_buf());
        let had_one = matches!(tab.kernel, KernelSlot::Live(_) | KernelSlot::Failed { .. });
        if had_one {
            self.shut_down_the_kernel(index);
            self.start_the_kernel(index, path);
        }
    }

    /// Run the notebook at `index` on the kernelspec `name` from now on, and write it into the
    /// notebook's metadata so the file says what it was run with, as Jupyter does.
    pub(crate) fn choose_a_kernelspec(&mut self, index: usize, name: &str) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        tab.kernel_name = Some(name.to_owned());
        if let Some(metadata) = tab.model.metadata.as_object_mut() {
            let spec = metadata.entry("kernelspec").or_insert_with(|| serde_json::json!({}));
            if let Some(spec) = spec.as_object_mut() {
                spec.insert("name".to_owned(), serde_json::Value::String(name.to_owned()));
            }
        }
        file.document.note_a_change_outside_the_text();
        let python = tab.python.clone();
        if let (Some(python), true) = (python, matches!(tab.kernel, KernelSlot::Live(_))) {
            self.shut_down_the_kernel(index);
            self.start_the_kernel(index, &python);
        }
    }

    /// Install `ipykernel` into the Python at `path`, in the run tile so the install can be watched,
    /// and look at the Pythons again once it has had time to finish.
    pub(crate) fn install_ipykernel(&mut self, index: usize, path: &Path) {
        let command = unluminous_jupyter::kernel::install_command(path)
            .into_iter()
            .map(|part| if part.contains(' ') { format!("\"{part}\"") } else { part })
            .collect::<Vec<_>>()
            .join(" ");
        let was_selected = self.run_selected.clone();
        let configuration = crate::services::run_configurations::Configuration::new(
            "Install ipykernel".to_owned(),
            &command,
        );
        match self.start_a_run(configuration) {
            Ok(()) => self.message = Some(format!("Installing ipykernel: {command}")),
            Err(problem) => self.message = Some(problem),
        }
        self.run_selected = was_selected;
        self.pythons = crate::app::notebook_kernel::Pythons::NotLooked;
        if let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() {
            tab.kernel = KernelSlot::NotStarted;
            tab.python = Some(path.to_path_buf());
        }
    }

    /// The line under the toolbar saying why there is no kernel, with the way out.
    fn show_the_kernel_problem(
        &mut self,
        ui: &mut egui::Ui,
        area: Rect,
        index: usize,
        reason: &str,
        missing: Option<&str>,
    ) {
        ui.painter().rect_filled(area, CornerRadius::ZERO, color::failure().gamma_multiply(0.19));
        let python = self.notebook_python(index);
        let mut right = area.right() - 8.0;
        if let (Some(python), Some("ipykernel" | "jupyter_client")) = (python.as_ref(), missing) {
            let button = Rect::from_min_max(
                Pos2::new(right - 130.0, area.top() + 4.0),
                Pos2::new(right, area.bottom() - 4.0),
            );
            if controls::choice_button(ui, button, "Install ipykernel", false) {
                self.install_ipykernel(index, python);
            }
            right = button.left() - 8.0;
        }
        let font = egui::FontId::proportional(12.0);
        let words = controls::truncate_chars(reason, 160, 150);
        let clip = Rect::from_min_max(area.min, Pos2::new(right, area.max.y));
        ui.painter().with_clip_rect(clip).text(
            Pos2::new(area.left() + 12.0, area.center().y),
            egui::Align2::LEFT_CENTER,
            words,
            font,
            color::text_strong(),
        );
    }

    /// The variables the kernel holds, down the right of the notebook: name, type, value and shape,
    /// DataFrames first, as the reference editor sorts them.
    fn show_the_variables(&mut self, ui: &mut egui::Ui, area: Rect, index: usize) {
        ui.painter().rect_filled(area, CornerRadius::ZERO, color::explorer());
        ui.painter().vline(area.left(), area.y_range(), Stroke::new(1.0, color::divider()));
        let heading = Rect::from_min_size(area.min, Vec2::new(area.width(), 30.0));
        ui.painter().text(
            Pos2::new(heading.left() + 12.0, heading.center().y),
            egui::Align2::LEFT_CENTER,
            "Variables",
            egui::FontId::proportional(12.5),
            color::text_strong(),
        );
        let refresh = Rect::from_center_size(
            Pos2::new(heading.right() - 18.0, heading.center().y),
            Vec2::splat(22.0),
        );
        if controls::icon_button(ui, refresh, "Refresh Variables", icon::rerun) {
            self.ask_for_the_variables(index);
        }
        let Some(tab) = self.files.at(index).notebook.as_deref() else { return };
        let mut rows = tab.variables.clone();
        rows.sort_by_key(|row| (row.type_name != "DataFrame", row.name.to_lowercase()));
        let painter = ui.painter().with_clip_rect(area);
        let mut y = heading.bottom() + 4.0;
        if rows.is_empty() {
            let live = matches!(tab.kernel, KernelSlot::Live(_));
            paint_no_variables(&painter, area, y, live);
        }
        for row in rows {
            paint_a_variable(&painter, area, y, &row);
            y += 36.0;
            if y > area.bottom() {
                break;
            }
        }
    }
}

/// The words in the variables panel when the kernel holds none: that there are none yet, or that a
/// cell has to run first when `live` is false because there is no kernel.
fn paint_no_variables(painter: &egui::Painter, area: Rect, y: f32, live: bool) {
    let words = match live {
        true => "No variables yet.",
        false => "Run a cell to see its variables.",
    };
    painter.text(
        Pos2::new(area.left() + 12.0, y + 10.0),
        egui::Align2::LEFT_CENTER,
        words,
        egui::FontId::proportional(12.0),
        color::text_dim(),
    );
}

/// One variable in the panel, at height `y`: its name, type and shape on one line and its value under it.
fn paint_a_variable(painter: &egui::Painter, area: Rect, y: f32, row: &Variable) {
    let font = egui::FontId::monospace(11.5);
    let shape =
        row.shape.clone().or(row.size.map(|size| format!("len {size}"))).unwrap_or_default();
    let head = format!("{} : {} {}", row.name, row.type_name, shape);
    painter.text(
        Pos2::new(area.left() + 12.0, y + 9.0),
        egui::Align2::LEFT_CENTER,
        head,
        font.clone(),
        color::text(),
    );
    let value = controls::truncate_chars(&row.value.replace('\n', " "), 60, 57);
    painter.text(
        Pos2::new(area.left() + 24.0, y + 25.0),
        egui::Align2::LEFT_CENTER,
        value,
        font,
        color::text_dim(),
    );
}

/// The toolbar's icon buttons in order: what each does, its name, and the icon drawn on it.
fn toolbar_buttons() -> [(NotebookAction, &'static str, crate::components::notebook_view::Draw); 8]
{
    [
        (NotebookAction::AddBelow(CellKind::Code), "Code Cell Below", icon::plus),
        (NotebookAction::MoveUp, "Move Cell Up", icon::chevron_up),
        (NotebookAction::MoveDown, "Move Cell Down", icon::chevron_down),
        (NotebookAction::RunCellSelectBelow, "Run Cell and Select Below (Shift+Enter)", icon::run),
        (NotebookAction::RunAll, "Run All (Ctrl+Alt+Shift+Enter)", run_all),
        (NotebookAction::Interrupt, "Interrupt Kernel", icon::stop),
        (NotebookAction::Restart, "Restart Kernel", icon::rerun),
        (NotebookAction::ClearAllOutputs, "Clear All Outputs", icon::clear),
    ]
}

/// The rows of the kernel picker: each Python found, each kernel the chosen one has, and the ways out.
fn kernel_rows(
    ui: &mut egui::Ui,
    pythons: &[Python],
    chosen: Option<&Path>,
    specs: &[unluminous_jupyter::kernel::KernelSpec],
) -> Option<KernelChoice> {
    let mut picked = None;
    controls::menu_heading(ui, "Python", 0.0);
    if pythons.is_empty() {
        controls::menu_row(ui, "Looking for Pythons on this machine...", "", false, false, 0.0);
    }
    for python in pythons {
        let mut name = format!("{} ({})", python_name(&python.path), python.version);
        if !python.has_ipykernel {
            name.push_str(" - no ipykernel");
        }
        if controls::menu_row(ui, &name, "", true, chosen == Some(python.path.as_path()), 0.0) {
            picked = Some(KernelChoice::Python(python.path.clone()));
        }
    }
    if !specs.is_empty() {
        controls::menu_heading(ui, "Kernel", 0.0);
        for spec in specs {
            if controls::menu_row(ui, &spec.display_name, "", true, false, 0.0) {
                picked = Some(KernelChoice::Spec(spec.name.clone()));
            }
        }
    }
    controls::menu_heading(ui, "", 0.0);
    if let Some(chosen) = chosen {
        if controls::menu_row(ui, "Install ipykernel into this Python", "", true, false, 0.0) {
            picked = Some(KernelChoice::Install(chosen.to_path_buf()));
        }
    }
    if controls::menu_row(ui, "Look for Pythons again", "", true, false, 0.0) {
        picked = Some(KernelChoice::LookAgain);
    }
    picked
}

/// What the outline dropdown is called, which is also how its popup is found to open it.
const OUTLINE_NAME: &str = "Notebook outline";

/// The outline's rows: the cell, how deep to set it in, and its words. A Markdown cell gives one row
/// for each heading in it; a code cell gives one row, its first line that is not blank.
pub fn outline_rows(cells: &[unluminous_jupyter::nbformat::Cell]) -> Vec<(usize, usize, String)> {
    let mut rows = Vec::new();
    let mut depth = 0;
    for (at, cell) in cells.iter().enumerate() {
        match cell.kind {
            CellKind::Markdown => {
                for line in cell.source.lines() {
                    let level = line.chars().take_while(|ch| *ch == '#').count();
                    if (1..=6).contains(&level) && line[level..].starts_with(' ') {
                        depth = level;
                        rows.push((at, level - 1, line[level..].trim().to_owned()));
                    }
                }
            }
            CellKind::Code => {
                let first =
                    cell.source.lines().find(|line| !line.trim().is_empty()).unwrap_or("(empty)");
                let words = controls::truncate_chars(first.trim(), 48, 45);
                rows.push((at, depth, format!("[{}] {words}", at + 1)));
            }
            CellKind::Raw => {}
        }
    }
    rows
}

/// What a Python is called on the toolbar: its environment's folder name, or its own file name.
pub fn python_name(path: &Path) -> String {
    let folder = path.parent().and_then(|parent| {
        let up = match parent.file_name()?.to_string_lossy().as_ref() {
            "Scripts" | "bin" => parent.parent()?,
            _ => parent,
        };
        up.file_name().map(|name| name.to_string_lossy().to_string())
    });
    folder.unwrap_or_else(|| path.display().to_string())
}

/// Two triangles side by side: run every cell.
fn run_all(painter: &egui::Painter, centre: Pos2, colour: Color32) {
    icon::run_scaled(painter, Pos2::new(centre.x - 3.5, centre.y), colour, 0.75);
    icon::run_scaled(painter, Pos2::new(centre.x + 3.5, centre.y), colour, 0.75);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_outline_lists_each_heading_by_its_level_and_each_code_cell_under_it() {
        use unluminous_jupyter::nbformat::Cell;
        let cells = vec![
            Cell::new(
                CellKind::Markdown,
                "a",
                "# Load
some words
## Clean",
            ),
            Cell::new(
                CellKind::Code,
                "b",
                "
import pandas as pd",
            ),
            Cell::new(CellKind::Raw, "c", "raw"),
        ];
        let rows = outline_rows(&cells);
        assert_eq!(
            rows,
            vec![
                (0, 0, "Load".to_owned()),
                (0, 1, "Clean".to_owned()),
                (1, 2, "[2] import pandas as pd".to_owned()),
            ]
        );
    }

    #[test]
    fn a_python_is_named_after_its_environment() {
        assert_eq!(python_name(Path::new("C:/work/project/.venv/Scripts/python.exe")), ".venv");
        assert_eq!(python_name(Path::new("/home/a/env/bin/python3")), "env");
        assert_eq!(python_name(Path::new("C:/Python313/python.exe")), "Python313");
    }
}
