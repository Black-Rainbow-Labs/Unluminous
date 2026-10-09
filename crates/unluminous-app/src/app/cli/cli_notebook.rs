//! `notebook` -- Jupyter notebooks: what a notebook holds, running its cells in a kernel, reading what
//! they output, changing cells as a whole, the kernel, and exporting. `task-2220`.
//!
//! **Every command here goes down the function a person's key or button goes down.** Running a cell is
//! `run_notebook_cells`, a cell operation is `run_a_notebook_action` with the cell chosen first, the
//! kernel picker's choices are `choose_a_python` and `choose_a_kernelspec` — so a cell an agent ran and
//! a cell a person ran are the same thing, which is `run_cli`'s rule.

use std::path::{Path, PathBuf};

use unluminous_jupyter::nbformat::{Cell, CellKind};
use unluminous_jupyter::outputs::{self, Shown};

use super::*;
use crate::app::notebook::{KernelSlot, Mode, Run};
use crate::app::notebook_actions::NotebookAction;

/// How long `notebook kernel pythons` and `notebook kernel kernels` wait for Python to answer.
const PYTHONS_WAIT: std::time::Duration = std::time::Duration::from_secs(90);

impl UnluminousApp {
    pub(crate) fn cli_notebook(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "new" => return self.cli_notebook_new(request),
            "convert" => return self.cli_notebook_convert(request),
            _ => {}
        }
        if let Some(path) = request.text("path") {
            let path = self.tree.root().join(path);
            if let Err(problem) = self.open_path_permanently(&path) {
                return no(request, code::FAILED, problem);
            }
        }
        if self.files.active().notebook.is_none() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "The tab that is showing is not a notebook. Open one with `tab open <file>.ipynb`, or make one with `notebook new`.",
            );
        }
        let index = self.files.active_index();
        self.refresh_the_notebook(index);
        match verb {
            "status" => self.cli_notebook_status(request, index),
            "cell" => self.cli_notebook_cell(request, index),
            "run" => self.cli_notebook_run(request, index),
            "add" => self.cli_notebook_add(request, index),
            "source" => self.cli_notebook_source(request, index),
            "edit" => self.cli_notebook_edit(request, index),
            "select" => self.cli_notebook_select(request, index),
            "kernel" => self.cli_notebook_kernel(request, index),
            "variables" => self.cli_notebook_variables(request, index),
            "input" => self.cli_notebook_input(request, index),
            "export" => self.cli_notebook_export(request),
            other => no(request, code::UNKNOWN_COMMAND, format!("notebook has no {other}.")),
        }
    }

    /// The cell a command names: `--id`, then the number in `field`, then the chosen cell.
    fn cli_cell(&self, request: &Request, index: usize, field: &str) -> Result<usize, String> {
        let tab = self.files.at(index).notebook.as_deref().ok_or("not a notebook")?;
        if let Some(id) = request.text("id") {
            return tab
                .index_of(&id)
                .ok_or_else(|| format!("No cell has the id {id}. `notebook status` lists them."));
        }
        match request.whole(field) {
            Some(0) => Err("Cells count from 1.".to_owned()),
            Some(number) if number <= tab.len() => Ok(number - 1),
            Some(number) => {
                Err(format!("There is no cell {number}; the notebook has {}.", tab.len()))
            }
            None => Ok(self.chosen_cells(index).start),
        }
    }

    fn cli_notebook_status(&mut self, request: &Request, index: usize) -> Outcome {
        let chosen = self.chosen_cells(index);
        let kernel = self.cli_kernel_json(index);
        let file = self.files.at(index);
        let Some(tab) = file.notebook.as_deref() else {
            return no(request, code::FAILED, "not a notebook");
        };
        let mut rows = Vec::new();
        let mut cells = Vec::new();
        for (at, cell) in tab.model.cells.iter().enumerate() {
            let state = run_state(tab.runs.get(&cell.id), cell);
            let finished = match tab.runs.get(&cell.id) {
                Some(Run::Done { took, clock, .. }) => {
                    Some((took.as_millis() as u64, clock.to_string()))
                }
                _ => None,
            };
            let first =
                cell.source.lines().next().unwrap_or("").chars().take(70).collect::<String>();
            let summary = output_summary(cell);
            let count = cell
                .execution_count
                .map(|count| format!("[{count}]"))
                .unwrap_or_else(|| "[ ]".to_owned());
            let mark = if chosen.contains(&at) { "*" } else { " " };
            rows.push(format!(
                "{mark}{:>3} {:<8} {:<8} {count:<6} {state:<9} {first}{}",
                at + 1,
                cell.id,
                cell.kind.name(),
                if summary.is_empty() { String::new() } else { format!("  -> {summary}") }
            ));
            cells.push(json!({
                "number": at + 1,
                "id": cell.id,
                "kind": cell.kind.name(),
                "executionCount": cell.execution_count,
                "state": state,
                "firstLine": first,
                "outputs": summary,
                "tookMs": finished.as_ref().map(|(took, _)| *took),
                "finishedAt": finished.as_ref().map(|(_, clock)| clock.clone()),
                "collapsed": tab.collapsed.contains(&cell.id),
                "outputCollapsed": tab.outputs_collapsed.contains(&cell.id),
            }));
        }
        let waiting = tab.waiting.as_ref().map(|waiting| json!({ "cell": tab.index_of(&waiting.cell).map(|at| at + 1), "prompt": waiting.prompt }));
        let mode = match tab.mode {
            Mode::Edit => "edit",
            Mode::Command { .. } => "command",
        };
        let path = file.path().map(|path| path.to_string_lossy().to_string());
        let message = format!(
            "{} cells \u{00B7} kernel: {}",
            tab.len(),
            kernel["state"].as_str().unwrap_or("")
        );
        lines(
            request,
            message,
            rows,
            json!({
                "path": path,
                "kernel": kernel,
                "mode": mode,
                "chosen": [chosen.start + 1, chosen.end],
                "waitingForInput": waiting,
                "unsaved": file.document.is_modified(),
                "lineNumbers": tab.line_numbers,
                "variablesShowing": tab.variables_showing,
                "cells": cells,
            }),
        )
    }

    /// What `status` and `kernel status` say about the kernel.
    fn cli_kernel_json(&self, index: usize) -> Value {
        let Some(tab) = self.files.at(index).notebook.as_deref() else { return Value::Null };
        let (state, problem) = match &tab.kernel {
            KernelSlot::NotStarted => ("not started", None),
            KernelSlot::Failed { reason, .. } => ("failed", Some(reason.clone())),
            KernelSlot::Live(kernel) => (
                match kernel.state() {
                    unluminous_jupyter::kernel::KernelState::Starting => "starting",
                    unluminous_jupyter::kernel::KernelState::Idle => "idle",
                    unluminous_jupyter::kernel::KernelState::Busy => "busy",
                    unluminous_jupyter::kernel::KernelState::Restarting => "restarting",
                    unluminous_jupyter::kernel::KernelState::Stopped => "shut down",
                    unluminous_jupyter::kernel::KernelState::Dead(_) => "dead",
                },
                match kernel.state() {
                    // What the bridge wrote to its error output is what says why, so it is part of
                    // the answer rather than something to go looking for.
                    unluminous_jupyter::kernel::KernelState::Dead(why) => Some(
                        format!(
                            "{why}
{}",
                            kernel.stderr_tail()
                        )
                        .trim()
                        .to_owned(),
                    ),
                    _ => None,
                },
            ),
        };
        let language = match &tab.kernel {
            KernelSlot::Live(kernel) => {
                kernel.language().map(|info| format!("{} {}", info.name, info.version))
            }
            _ => None,
        };
        json!({
            "state": state,
            "problem": problem,
            "python": tab.python.as_ref().map(|path| path.to_string_lossy().to_string()),
            "kernelspec": tab.kernel_name.clone().or_else(|| tab.model.metadata.get("kernelspec").and_then(|spec| spec.get("name")).and_then(Value::as_str).map(str::to_owned)),
            "language": language,
            "queued": tab.queue.len(),
            "running": tab.running.as_ref().and_then(|running| tab.index_of(&running.cell)).map(|at| at + 1),
        })
    }

    fn cli_notebook_cell(&mut self, request: &Request, index: usize) -> Outcome {
        let cell = match self.cli_cell(request, index, "cell") {
            Ok(cell) => cell,
            Err(problem) => return no(request, code::USAGE, problem),
        };
        let pictures =
            request.text("pictures").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let Some(tab) = self.files.at(index).notebook.as_deref() else {
            return no(request, code::FAILED, "not a notebook");
        };
        let found = tab.model.cells[cell].clone();
        let state = run_state(tab.runs.get(&found.id), &found);
        let (text, outputs) = outputs_as_text(&found, &pictures, cell + 1);
        let mut rows: Vec<String> = found.source.lines().map(str::to_owned).collect();
        if !text.is_empty() {
            rows.push("--- output ---".to_owned());
            rows.extend(text.lines().map(str::to_owned));
        }
        let message = format!("Cell {} ({}, {state})", cell + 1, found.kind.name());
        lines(
            request,
            message,
            rows,
            json!({
                "number": cell + 1,
                "id": found.id,
                "kind": found.kind.name(),
                "executionCount": found.execution_count,
                "state": state,
                "source": found.source,
                "outputs": outputs,
                "outputText": text,
            }),
        )
    }

    fn cli_notebook_run(&mut self, request: &Request, index: usize) -> Outcome {
        let count = self.files.at(index).notebook.as_deref().map(|tab| tab.len()).unwrap_or(0);
        let cells = if request.switch("all") {
            0..count
        } else {
            let first = match self.cli_cell(request, index, "cell") {
                Ok(cell) => cell,
                Err(problem) => return no(request, code::USAGE, problem),
            };
            match (request.switch("above"), request.switch("below"), request.whole("to")) {
                (true, _, _) => 0..first,
                (_, true, _) => first..count,
                (_, _, Some(to)) if to > first && to <= count => first..to,
                (_, _, Some(to)) => {
                    return no(
                        request,
                        code::USAGE,
                        format!("--to {to} is not a cell at or after cell {}.", first + 1),
                    )
                }
                _ => first..first + 1,
            }
        };
        let ids: Vec<String> = {
            let Some(tab) = self.files.at(index).notebook.as_deref() else {
                return no(request, code::FAILED, "not a notebook");
            };
            cells
                .clone()
                .filter_map(|at| tab.model.cells.get(at))
                .filter(|cell| cell.kind == CellKind::Code)
                .map(|cell| cell.id.clone())
                .collect()
        };
        self.run_notebook_cells(index, cells.clone());
        if !request.switch("wait") {
            return ok(
                request,
                format!("Running {} code cell(s).", ids.len()),
                json!({ "cells": ids }),
            );
        }
        let wait = request
            .whole("timeout")
            .map(|ms| ms as u64)
            .unwrap_or(unluminous_cli::catalogue::NOTEBOOK_WAIT_MS);
        let path = self.files.at(index).path().map(Path::to_path_buf);
        Outcome::Hold(Waiting::NotebookRun {
            path,
            cells: ids,
            until: Instant::now() + Duration::from_millis(wait),
        })
    }

    /// The answer to a held `notebook run --wait`, once every cell it ran has finished or been
    /// skipped. `None` while any is still queued or running.
    pub(crate) fn notebook_run_answer(
        &mut self,
        request: &Request,
        path: Option<&Path>,
        cells: &[String],
    ) -> Option<Reply> {
        let index = match path {
            Some(path) => self.files.index_of(path)?,
            None => self.files.active_index(),
        };
        let tab = self.files.at(index).notebook.as_deref()?;
        if let KernelSlot::Failed { reason, .. } = &tab.kernel {
            return Some(Reply::failed(
                &request.command,
                code::FAILED,
                format!("The kernel could not start: {reason}"),
            ));
        }
        let busy = cells
            .iter()
            .any(|id| matches!(tab.runs.get(id), Some(Run::Queued | Run::Running { .. }) | None));
        if busy {
            return None;
        }
        let pictures = std::env::temp_dir();
        let mut failed = 0;
        let mut rows = Vec::new();
        let mut answered = Vec::new();
        for id in cells {
            let Some(at) = tab.index_of(id) else { continue };
            let cell = &tab.model.cells[at];
            let state = run_state(tab.runs.get(id), cell);
            if state == "error" {
                failed += 1;
            }
            let (text, outputs) = outputs_as_text(cell, &pictures, at + 1);
            rows.push(format!(
                "--- cell {} [{}] {state} ---",
                at + 1,
                cell.execution_count.map(|count| count.to_string()).unwrap_or_default()
            ));
            rows.extend(text.lines().map(str::to_owned));
            answered.push(json!({ "number": at + 1, "id": id, "state": state, "executionCount": cell.execution_count, "outputs": outputs, "outputText": text }));
        }
        let message = match failed {
            0 => format!("Ran {} cell(s).", cells.len()),
            _ => format!("Ran {} cell(s); one raised, and the run stopped there.", cells.len()),
        };
        let mut result = json!({ "cells": answered, "lines": rows });
        result["failed"] = json!(failed > 0);
        Some(Reply::done(&request.command, message, result))
    }

    fn cli_notebook_add(&mut self, request: &Request, index: usize) -> Outcome {
        let kind = match request.text("kind").as_deref().map(CellKind::from_name) {
            None => CellKind::Code,
            Some(Some(kind)) => kind,
            Some(None) => return no(request, code::USAGE, "--kind is code, markdown or raw."),
        };
        let count = self.files.at(index).notebook.as_deref().map(|tab| tab.len()).unwrap_or(0);
        let at = match request.whole("at") {
            Some(0) => return no(request, code::USAGE, "Positions count from 1."),
            Some(at) if at <= count + 1 => at - 1,
            Some(at) => {
                return no(
                    request,
                    code::USAGE,
                    format!("--at {at} is past the end; the notebook has {count} cells."),
                )
            }
            None => self.chosen_cells(index).end.min(count),
        };
        let source = request.text("source").unwrap_or_default().replace("\\n", "\n");
        let id = self.add_a_cell_holding(index, at, kind, &source);
        ok(
            request,
            format!("Added cell {} ({}).", at + 1, kind.name()),
            json!({ "number": at + 1, "id": id }),
        )
    }

    fn cli_notebook_source(&mut self, request: &Request, index: usize) -> Outcome {
        let cell = match self.cli_cell(request, index, "cell") {
            Ok(cell) => cell,
            Err(problem) => return no(request, code::USAGE, problem),
        };
        let source = request.text("source").unwrap_or_default().replace("\\n", "\n");
        let source = unluminous_jupyter::text::escape_source(&source).into_owned();
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else {
            return no(request, code::FAILED, "not a notebook");
        };
        let range = tab.spans[cell].body_bytes.clone();
        let caret = range.start + source.len();
        crate::app::notebook::apply_edits(&mut file.document, vec![(range, source)], Some(caret));
        self.refresh_the_notebook(index);
        ok(
            request,
            format!("Cell {} now holds the new source.", cell + 1),
            json!({ "number": cell + 1 }),
        )
    }

    fn cli_notebook_select(&mut self, request: &Request, index: usize) -> Outcome {
        let cell = match self.cli_cell(request, index, "cell") {
            Ok(cell) => cell,
            Err(problem) => return no(request, code::USAGE, problem),
        };
        let count = self.files.at(index).notebook.as_deref().map(|tab| tab.len()).unwrap_or(0);
        let last = request.whole("to").map(|to| to.clamp(cell + 1, count) - 1).unwrap_or(cell);
        self.choose_a_cell(index, cell, request.switch("edit"));
        if last != cell {
            if let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() {
                tab.mode = Mode::Command { anchor: cell, head: last };
            }
        }
        ok(
            request,
            format!("Chose cell {} through {}.", cell + 1, last + 1),
            json!({ "chosen": [cell + 1, last + 1] }),
        )
    }

    fn cli_notebook_edit(&mut self, request: &Request, index: usize) -> Outcome {
        let operation = request.text("operation").unwrap_or_default();
        if operation == "clear-all" {
            return self.cli_notebook_action(request, NotebookAction::ClearAllOutputs);
        }
        let cell = match self.cli_cell(request, index, "cell") {
            Ok(cell) => cell,
            Err(problem) => return no(request, code::USAGE, problem),
        };
        self.choose_a_cell(index, cell, false);
        let what = match operation.as_str() {
            "delete" => NotebookAction::Delete,
            "copy" => NotebookAction::Copy,
            "cut" => NotebookAction::Cut,
            "paste" if request.switch("above") => NotebookAction::PasteAbove,
            "paste" => NotebookAction::PasteBelow,
            "clear" => NotebookAction::ClearOutput,
            "kind" => match request.text("kind").as_deref().and_then(CellKind::from_name) {
                Some(kind) => NotebookAction::Convert(kind),
                None => {
                    return no(request, code::USAGE, "kind needs --kind code, markdown or raw.")
                }
            },
            "move" => return self.cli_move_a_cell(request, index, cell),
            "merge" => return self.cli_merge_cells(request, index, cell),
            "split" => return self.cli_split_a_cell(request, index, cell),
            other => {
                return no(
                    request,
                    code::USAGE,
                    format!("{other} is not an edit. See `notebook edit --help`."),
                )
            }
        };
        self.cli_notebook_action(request, what)
    }

    /// Run one notebook action and answer with what it said.
    fn cli_notebook_action(&mut self, request: &Request, what: NotebookAction) -> Outcome {
        match self.run_a_notebook_action(what) {
            Ok(said) => ok(request, said, json!({ "action": what.name() })),
            Err(problem) => no(request, code::NOT_APPLICABLE, problem),
        }
    }

    /// Move a cell to a position, with the edit `Move Cell Up` and dragging a cell make.
    fn cli_move_a_cell(&mut self, request: &Request, index: usize, cell: usize) -> Outcome {
        let count = self.files.at(index).notebook.as_deref().map(|tab| tab.len()).unwrap_or(0);
        let Some(to) = request.whole("to").filter(|to| *to >= 1 && *to <= count) else {
            return no(
                request,
                code::USAGE,
                format!("move needs --to, a position from 1 to {count}."),
            );
        };
        // `to` is where the cell ends up; the gap it goes into is counted before it is taken out.
        let gap = if to - 1 > cell { to } else { to - 1 };
        self.move_cells_into(index, cell..cell + 1, gap);
        ok(request, format!("Cell {} is now cell {to}.", cell + 1), json!({ "number": to }))
    }

    /// Merge the cell and every cell through `--to` into one.
    fn cli_merge_cells(&mut self, request: &Request, index: usize, cell: usize) -> Outcome {
        let count = self.files.at(index).notebook.as_deref().map(|tab| tab.len()).unwrap_or(0);
        let last = request.whole("to").unwrap_or(cell + 2).clamp(cell + 1, count) - 1;
        if let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() {
            tab.mode = Mode::Command { anchor: cell, head: last };
        }
        self.cli_notebook_action(request, NotebookAction::MergeSelected)
    }

    /// Split a cell before line `--to` of it.
    fn cli_split_a_cell(&mut self, request: &Request, index: usize, cell: usize) -> Outcome {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else {
            return no(request, code::FAILED, "not a notebook");
        };
        let span = &tab.spans[cell];
        let Some(line) = request.whole("to").filter(|line| *line >= 2 && *line <= span.body.len())
        else {
            return no(
                request,
                code::USAGE,
                format!("split needs --to, a line of the cell from 2 to {}.", span.body.len()),
            );
        };
        let offset =
            file.document.text().line_to_byte(span.body.start + line - 1).saturating_sub(1);
        file.document.apply(unluminous_core::Command::PlaceCaret { offset, extend: false });
        tab.mode = Mode::Edit;
        self.cli_notebook_action(request, NotebookAction::Split)
    }

    fn cli_notebook_kernel(&mut self, request: &Request, index: usize) -> Outcome {
        let operation = request.text("operation").unwrap_or_else(|| "status".to_owned());
        match operation.as_str() {
            "status" => {
                let kernel = self.cli_kernel_json(index);
                ok(request, format!("Kernel: {}", kernel["state"].as_str().unwrap_or("")), kernel)
            }
            "start" => {
                self.start_a_kernel_if_needed(index);
                ok(request, "Starting the kernel.", self.cli_kernel_json(index))
            }
            "interrupt" => self.cli_notebook_action(request, NotebookAction::Interrupt),
            "restart" => self.cli_notebook_action(request, NotebookAction::Restart),
            "shut-down" => self.cli_notebook_action(request, NotebookAction::ShutDown),
            "pythons" => self.cli_notebook_pythons(request),
            "kernels" => self.cli_notebook_kernels(request, index),
            "choose" => self.cli_notebook_choose(request, index),
            "install" => {
                let python = request.text("python").map(PathBuf::from).or_else(|| {
                    self.files.at(index).notebook.as_deref().and_then(|tab| tab.python.clone())
                });
                let Some(python) = python else {
                    return no(
                        request,
                        code::USAGE,
                        "Say which Python with --python, or choose one first.",
                    );
                };
                self.install_ipykernel(index, &python);
                ok(
                    request,
                    self.message.clone().unwrap_or_default(),
                    json!({ "python": python.to_string_lossy() }),
                )
            }
            other => no(request, code::USAGE, format!("{other} is not something the kernel does.")),
        }
    }

    /// Every Python on this machine, and whether each has ipykernel. Looked for now if nobody has.
    /// The Pythons on this machine. The search runs on a thread, so the answer waits for it.
    fn cli_notebook_pythons(&mut self, request: &Request) -> Outcome {
        self.look_for_pythons();
        match self.notebook_pythons_answer(request) {
            Some(reply) => Outcome::Reply(reply),
            None => Outcome::Hold(Waiting::NotebookPythons { until: Instant::now() + PYTHONS_WAIT }),
        }
    }

    /// The answer to `notebook kernel pythons`, once the search has finished.
    pub(crate) fn notebook_pythons_answer(&mut self, request: &Request) -> Option<Reply> {
        self.look_for_pythons();
        if !matches!(self.pythons, crate::app::notebook_kernel::Pythons::Found(_)) {
            return None;
        }
        let found = self.pythons.found();
        let rows: Vec<String> = found
            .iter()
            .map(|python| {
                format!(
                    "{} {} ({}){}",
                    python.path.display(),
                    python.version,
                    python.found_by,
                    if python.has_ipykernel { "" } else { " - no ipykernel" }
                )
            })
            .collect();
        let data: Vec<Value> = found.iter().map(|python| json!({ "path": python.path.to_string_lossy(), "version": python.version, "ipykernel": python.has_ipykernel, "foundBy": python.found_by })).collect();
        Some(lines_reply(request, format!("{} Python(s)", found.len()), rows, json!({ "pythons": data })))
    }

    /// The kernels a Python has.
    fn cli_notebook_kernels(&mut self, request: &Request, index: usize) -> Outcome {
        let python = request.text("python").map(PathBuf::from).or_else(|| {
            self.files.at(index).notebook.as_deref().and_then(|tab| tab.python.clone())
        });
        let Some(python) = python else {
            return no(
                request,
                code::USAGE,
                "Say which Python with --python, or run a cell first.",
            );
        };
        // Asked afresh, so a kernel installed since the last listing is in the answer.
        self.kernelspecs.remove(&python);
        self.ask_for_the_kernelspecs(&python);
        Outcome::Hold(Waiting::NotebookKernels { python, until: Instant::now() + PYTHONS_WAIT })
    }

    /// The answer to `notebook kernel kernels`, once the Python has listed them.
    pub(crate) fn notebook_kernels_answer(&mut self, request: &Request, python: &Path) -> Option<Reply> {
        self.take_the_kernelspecs();
        let listed = self.kernelspecs.get(python)?.as_ref()?;
        Some(match listed {
            Ok(specs) => {
                let rows: Vec<String> = specs
                    .iter()
                    .map(|spec| format!("{}  {} ({})", spec.name, spec.display_name, spec.language))
                    .collect();
                let data: Vec<Value> = specs.iter().map(|spec| json!({ "name": spec.name, "displayName": spec.display_name, "language": spec.language })).collect();
                lines_reply(request, format!("{} kernel(s)", rows.len()), rows, json!({ "kernels": data }))
            }
            Err(problem) => Reply::failed(&request.command, code::FAILED, problem.clone()),
        })
    }

    /// Choose the Python, the kernelspec, or both.
    fn cli_notebook_choose(&mut self, request: &Request, index: usize) -> Outcome {
        let python = request.text("python").map(PathBuf::from);
        let name = request.text("name");
        if python.is_none() && name.is_none() {
            return no(request, code::USAGE, "choose needs --python, --name or both.");
        }
        if let Some(python) = &python {
            if !python.exists() {
                return no(request, code::FAILED, format!("{} does not exist.", python.display()));
            }
            self.choose_a_python(index, python);
        }
        if let Some(name) = &name {
            self.choose_a_kernelspec(index, name);
        }
        ok(request, "Chosen. The next run uses it.", self.cli_kernel_json(index))
    }

    fn cli_notebook_variables(&mut self, request: &Request, index: usize) -> Outcome {
        let live = self
            .files
            .at(index)
            .notebook
            .as_deref()
            .is_some_and(|tab| matches!(tab.kernel, KernelSlot::Live(_)));
        if !live {
            return no(
                request,
                code::NOT_APPLICABLE,
                "No kernel is running, so there are no variables. Run a cell first.",
            );
        }
        self.ask_for_the_variables(index);
        let path = self.files.at(index).path().map(Path::to_path_buf);
        Outcome::Hold(Waiting::NotebookVariables { path, until: Instant::now() + DEFAULT_WAIT })
    }

    /// The answer to a held `notebook variables`, once the kernel has said.
    pub(crate) fn notebook_variables_answer(
        &mut self,
        request: &Request,
        path: Option<&Path>,
    ) -> Option<Reply> {
        let index = match path {
            Some(path) => self.files.index_of(path)?,
            None => self.files.active_index(),
        };
        let tab = self.files.at(index).notebook.as_deref()?;
        if tab.variables_request.is_some() {
            return None;
        }
        let rows: Vec<String> = tab
            .variables
            .iter()
            .map(|row| {
                format!(
                    "{} : {} {} = {}",
                    row.name,
                    row.type_name,
                    row.shape.clone().unwrap_or_default(),
                    row.value
                )
            })
            .collect();
        let data: Vec<Value> = tab.variables.iter().map(|row| json!({ "name": row.name, "type": row.type_name, "value": row.value, "shape": row.shape, "size": row.size })).collect();
        let mut result = json!({ "variables": data });
        result["lines"] = json!(rows);
        Some(Reply::done(&request.command, format!("{} variable(s)", tab.variables.len()), result))
    }

    fn cli_notebook_input(&mut self, request: &Request, index: usize) -> Outcome {
        let waiting =
            self.files.at(index).notebook.as_deref().is_some_and(|tab| tab.waiting.is_some());
        if !waiting {
            return no(request, code::NOT_APPLICABLE, "No cell is waiting for input.");
        }
        let value = request.text("value").unwrap_or_default();
        self.answer_the_kernel(index, &value);
        ok(request, "Sent.", Value::Null)
    }

    fn cli_notebook_export(&mut self, request: &Request) -> Outcome {
        let format = request.text("format").unwrap_or_default();
        let to = request.text("to").map(|to| self.tree.root().join(to));
        match self.export_the_notebook(&format, to.as_deref()) {
            Ok(written) => ok(
                request,
                format!("Wrote {}", written.display()),
                json!({ "path": written.to_string_lossy() }),
            ),
            Err(problem) => no(request, code::FAILED, problem),
        }
    }

    fn cli_notebook_new(&mut self, request: &Request) -> Outcome {
        let path = request.text("path").map(PathBuf::from);
        match self.make_a_new_notebook(path.as_deref()) {
            Ok(made) => ok(
                request,
                format!("Made {}", made.display()),
                json!({ "path": made.to_string_lossy() }),
            ),
            Err(problem) => no(request, code::FAILED, problem),
        }
    }

    fn cli_notebook_convert(&mut self, request: &Request) -> Outcome {
        let Some(path) = request.text("path").map(|path| self.tree.root().join(path)) else {
            return no(request, code::USAGE, "convert needs the file to convert.");
        };
        let to_notebook = !crate::app::notebook_files::is_notebook(&path);
        match self.convert(&path, to_notebook) {
            Ok(written) => ok(
                request,
                format!("Wrote {}", written.display()),
                json!({ "path": written.to_string_lossy() }),
            ),
            Err(problem) => no(request, code::FAILED, problem),
        }
    }
}

/// How a cell's last run went, in one word: `ok`, `error`, `queued`, `running`, `skipped`, or
/// `not run`. A cell never run in this window but with an execution count from the file is `ok`.
fn run_state(run: Option<&Run>, cell: &Cell) -> &'static str {
    match run {
        Some(Run::Queued) => "queued",
        Some(Run::Running { .. }) => "running",
        Some(Run::Done { ok: true, .. }) => "ok",
        Some(Run::Done { ok: false, .. }) => "error",
        Some(Run::Skipped) => "skipped",
        None if cell.outputs.iter().any(|output| output.output_type() == "error") => "error",
        None if cell.execution_count.is_some() => "ok",
        None => "not run",
    }
}

/// A one line summary of a cell's outputs: how many and of what kind.
fn output_summary(cell: &Cell) -> String {
    if cell.outputs.is_empty() {
        return String::new();
    }
    let kinds: Vec<&str> = cell
        .outputs
        .iter()
        .map(|output| match outputs::shown(output) {
            Shown::Stream { stderr: true, .. } => "stderr",
            Shown::Stream { .. } => "stdout",
            Shown::Error { .. } => "error",
            Shown::Table(_) => "table",
            Shown::Png(_) | Shown::Jpeg(_) | Shown::Svg(_) => "picture",
            Shown::Html(_) => "html",
            Shown::Markdown(_) => "markdown",
            _ => "text",
        })
        .collect();
    format!("{} output(s): {}", kinds.len(), kinds.join(", "))
}

/// A cell's outputs as text a terminal or an agent can read, and as data. Pictures are written to
/// `pictures` and named by the cell's number, so a later run of the same cell writes over its own.
fn outputs_as_text(cell: &Cell, pictures: &Path, number: usize) -> (String, Vec<Value>) {
    let mut text = Vec::new();
    let mut data = Vec::new();
    for (at, output) in cell.outputs.iter().enumerate() {
        let name = format!("notebook-cell-{number}-output-{}", at + 1);
        let (words, value) = output_as_text(outputs::shown(output), pictures, &name);
        text.push(words.trim_end().to_owned());
        data.push(value);
    }
    (text.join("\n"), data)
}

/// One output as the words `notebook cell` prints and the data it answers with. A picture is written
/// to `pictures` under `name`, with the extension its format has.
fn output_as_text(shown: Shown, pictures: &Path, name: &str) -> (String, Value) {
    match shown {
        Shown::Stream { stderr, text } => (
            outputs::strip_ansi(&outputs::collapse_carriage_returns(&text)),
            json!({ "type": if stderr { "stderr" } else { "stdout" }, "text": text }),
        ),
        Shown::Error { ename, evalue, traceback } => {
            let lines: Vec<String> = traceback
                .iter()
                .map(|line| line.iter().map(|span| span.text.as_str()).collect())
                .collect();
            // IPython's traceback already ends with the error's own line; a kernel that sends
            // none gets the line said once.
            let words = match lines.is_empty() {
                true => format!("{ename}: {evalue}"),
                false => lines.join("\n"),
            };
            (words, json!({ "type": "error", "ename": ename, "evalue": evalue, "traceback": lines }))
        }
        Shown::Table(table) => {
            let rows: Vec<String> =
                table.header.iter().chain(&table.rows).map(|row| row.join(" | ")).collect();
            (rows.join("\n"), json!({ "type": "table", "header": table.header, "rows": table.rows }))
        }
        Shown::Png(bytes) => picture_as_text(&pictures.join(format!("{name}.png")), &bytes),
        Shown::Jpeg(bytes) => picture_as_text(&pictures.join(format!("{name}.jpg")), &bytes),
        Shown::Svg(svg) => ("[an SVG picture]".to_owned(), json!({ "type": "svg", "svg": svg })),
        Shown::Text(words) => {
            let words = outputs::strip_ansi(&words);
            (words.clone(), json!({ "type": "text", "text": words }))
        }
        Shown::Html(words) | Shown::Markdown(words) | Shown::Latex(words) | Shown::Json(words) => {
            (words.clone(), json!({ "type": "text", "text": words }))
        }
    }
}

/// Write a picture output to `place`, and say where it went.
fn picture_as_text(place: &Path, bytes: &[u8]) -> (String, Value) {
    match crate::services::store::write_atomically(place, bytes) {
        Ok(()) => (
            format!("[picture written to {}]", place.display()),
            json!({ "type": "picture", "path": place.to_string_lossy() }),
        ),
        Err(problem) => {
            (format!("[a picture that could not be written: {problem}]"), json!({ "type": "picture" }))
        }
    }
}
