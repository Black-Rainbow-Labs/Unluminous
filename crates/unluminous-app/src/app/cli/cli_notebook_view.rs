//! `notebook view`, `notebook output` and `notebook outline` -- what is shown of a notebook, and the
//! things a person does to an output with the pointer. `task-2220`, from the review in `task-2227`:
//! sorting a table, opening a traceback, scrolling a tall output and opening HTML in a browser tab
//! were things a person could do and an agent could not.
//!
//! Each command goes down the same code as the click or the menu entry it stands for.

use unluminous_jupyter::outputs::{self, Shown};

use super::*;
use crate::app::notebook_actions::NotebookAction;

impl UnluminousApp {
    /// `notebook view`: collapse a cell, its outputs or its section, render or edit a Markdown cell,
    /// and switch line numbers and the Variables panel.
    pub(crate) fn cli_notebook_view(&mut self, request: &Request, index: usize) -> Outcome {
        let operation = request.text("operation").unwrap_or_default();
        let what = match operation.as_str() {
            "collapse" => NotebookAction::CollapseCell,
            "collapse-output" => NotebookAction::CollapseOutput,
            "collapse-section" => NotebookAction::CollapseSection,
            "render" => NotebookAction::RenderMarkdown,
            "line-numbers" => NotebookAction::ToggleLineNumbers,
            "variables" => NotebookAction::ToggleVariables,
            other => {
                return no(
                    request,
                    code::USAGE,
                    format!("{other} is not something to show or hide. See `notebook view --help`."),
                )
            }
        };
        if let Err(problem) = self.choose_the_named_cell(request, index) {
            return no(request, code::USAGE, problem);
        }
        match self.run_a_notebook_action(what) {
            Ok(said) => ok(request, said, json!({ "action": what.name() })),
            Err(problem) => no(request, code::NOT_APPLICABLE, problem),
        }
    }

    /// Choose the cell `--cell` or `--id` names, when either is given.
    fn choose_the_named_cell(&mut self, request: &Request, index: usize) -> Result<(), String> {
        if request.whole("cell").is_none() && request.text("id").is_none() {
            return Ok(());
        }
        let cell = self.cli_cell(request, index, "cell")?;
        self.choose_a_cell(index, cell, false);
        Ok(())
    }

    /// `notebook output`: sort a table output by a column, open or close a traceback, scroll a tall
    /// output, or open an HTML or SVG output in a browser tab.
    pub(crate) fn cli_notebook_output(&mut self, request: &Request, index: usize) -> Outcome {
        let cell = match self.cli_cell(request, index, "cell") {
            Ok(cell) => cell,
            Err(problem) => return no(request, code::USAGE, problem),
        };
        let operation = request.text("operation").unwrap_or_default();
        let done = match operation.as_str() {
            "sort" => self.sort_an_output(request, index, cell),
            "traceback" => self.toggle_a_traceback(index, cell),
            "scroll" => self.scroll_an_output(request, index, cell),
            "open" => self.open_an_output(request, index, cell),
            other => Err(format!("{other} is not something to do to an output. See `notebook output --help`.")),
        };
        match done {
            Ok(said) => ok(request, said, json!({ "cell": cell + 1 })),
            Err(problem) => no(request, code::NOT_APPLICABLE, problem),
        }
    }

    /// Sort the table output of cell `cell` by `--column`, counting from 1, or put it back in its
    /// own order when no column is given.
    fn sort_an_output(&mut self, request: &Request, index: usize, cell: usize) -> Result<String, String> {
        let tab = self.files.at_mut(index).notebook.as_deref_mut().ok_or("not a notebook")?;
        let found = tab.model.cells.get(cell).ok_or("There is no such cell.")?;
        let columns = found
            .outputs
            .iter()
            .find_map(|output| match outputs::shown(output) {
                Shown::Table(table) => Some(table.header.len()),
                _ => None,
            })
            .ok_or(format!("Cell {} has no table output to sort.", cell + 1))?;
        let id = found.id.clone();
        let Some(column) = request.whole("column") else {
            tab.sorts.remove(&id);
            tab.bands_revision += 1;
            return Ok(format!("The table in cell {} is in its own order again.", cell + 1));
        };
        if column == 0 || column > columns {
            return Err(format!("The table has {columns} columns, counting from 1."));
        }
        let descending = request.switch("descending");
        tab.sorts.insert(id, (column - 1, descending));
        tab.bands_revision += 1;
        let order = if descending { "largest first" } else { "smallest first" };
        Ok(format!("The table in cell {} is sorted by column {column}, {order}.", cell + 1))
    }

    /// Open the traceback of cell `cell`'s error, or close it again.
    fn toggle_a_traceback(&mut self, index: usize, cell: usize) -> Result<String, String> {
        let tab = self.files.at_mut(index).notebook.as_deref_mut().ok_or("not a notebook")?;
        let found = tab.model.cells.get(cell).ok_or("There is no such cell.")?;
        if !found.outputs.iter().any(|output| output.output_type() == "error") {
            return Err(format!("Cell {} has no error, so there is no traceback.", cell + 1));
        }
        let id = found.id.clone();
        let opened = !tab.tracebacks_open.remove(&id);
        if opened {
            tab.tracebacks_open.insert(id);
        }
        tab.bands_revision += 1;
        Ok(match opened {
            true => format!("The traceback of cell {} is open.", cell + 1),
            false => format!("The traceback of cell {} is closed.", cell + 1),
        })
    }

    /// Scroll cell `cell`'s outputs so `--line`, counting from 1, is at their top.
    fn scroll_an_output(&mut self, request: &Request, index: usize, cell: usize) -> Result<String, String> {
        let line = request.whole("line").filter(|line| *line >= 1).ok_or("scroll needs --line, counting from 1.")?;
        let size = self.files.at(index).sized_at.unwrap_or(self.settings.font_size);
        let tab = self.files.at_mut(index).notebook.as_deref_mut().ok_or("not a notebook")?;
        let id = tab.id_of(cell).ok_or("There is no such cell.")?;
        let drawn = tab.drawn.get(&id).ok_or(format!("Cell {} has no outputs drawn yet.", cell + 1))?;
        let metrics = crate::components::notebook_view::Metrics { size, scroll: true };
        let shown = crate::components::notebook_view::shown_height(drawn, metrics);
        let furthest = (drawn.height - shown).max(0.0);
        let wanted = (line - 1) as f32 * metrics.output_font().size * 1.35;
        tab.output_scroll.insert(id, wanted.min(furthest));
        Ok(match furthest > 0.0 {
            true => format!("The outputs of cell {} are scrolled to line {line}.", cell + 1),
            false => format!("The outputs of cell {} all fit, so there is nothing to scroll.", cell + 1),
        })
    }

    /// Open output `--output` of cell `cell`, counting from 1, in a browser tab. Only an HTML or SVG
    /// output has a page to open.
    fn open_an_output(&mut self, request: &Request, index: usize, cell: usize) -> Result<String, String> {
        let at = request.whole("output").unwrap_or(1);
        let tab = self.files.at(index).notebook.as_deref().ok_or("not a notebook")?;
        let found = tab.model.cells.get(cell).ok_or("There is no such cell.")?;
        let output = found
            .outputs
            .get(at.saturating_sub(1))
            .ok_or(format!("Cell {} has {} outputs.", cell + 1, found.outputs.len()))?;
        let page = output
            .mime_text("text/html")
            .or_else(|| output.mime_text("image/svg+xml"))
            .ok_or(format!("Output {at} of cell {} is not HTML or SVG.", cell + 1))?;
        self.open_an_output_in_a_browser_tab(&page);
        Ok(format!("Opened output {at} of cell {} in a browser tab.", cell + 1))
    }

    /// `notebook outline`: the headings and code cells, set in by level. `--go` goes to one of them,
    /// counting the lines from 1.
    pub(crate) fn cli_notebook_outline(&mut self, request: &Request, index: usize) -> Outcome {
        let rows = match self.files.at(index).notebook.as_deref() {
            Some(tab) => crate::app::notebook_chrome::outline_rows(&tab.model.cells),
            None => return no(request, code::FAILED, "not a notebook"),
        };
        let lines: Vec<String> = rows
            .iter()
            .map(|(cell, depth, words)| format!("{}cell {}: {words}", "  ".repeat(*depth), cell + 1))
            .collect();
        let data: Vec<Value> = rows
            .iter()
            .map(|(cell, depth, words)| json!({ "cell": cell + 1, "depth": depth, "text": words }))
            .collect();
        let Some(go) = request.whole("go") else {
            return super::lines(request, format!("{} entries", rows.len()), lines, json!({ "outline": data }));
        };
        let Some((cell, _, words)) = go.checked_sub(1).and_then(|at| rows.get(at)) else {
            return no(request, code::USAGE, format!("The outline has {} entries, counting from 1.", rows.len()));
        };
        self.choose_a_cell(index, *cell, false);
        ok(request, format!("Went to cell {}: {words}.", cell + 1), json!({ "cell": cell + 1 }))
    }
}
