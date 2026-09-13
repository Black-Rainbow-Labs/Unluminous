//! `highlight` and `fold` -- the two things a document can be marked up with that are not an
//! edit: a coloured passage and a collapsed block. Neither bumps a document's `text_revision` or
//! its undo history, which is the rule `unluminous_core::highlights` and `unluminous_core::folding`
//! both keep.

use super::*;

impl UnluminousApp {
    /// `fold` — the blocks collapsed in the tab that is showing.
    ///
    /// What can be collapsed is worked out from the file, so `fold list` is the command to run
    /// first: nothing else here can be aimed anywhere until a caller knows which lines head a block.
    /// Lines count from 1 everywhere, as they do throughout `editor`, and they are the file's own
    /// numbers rather than the numbers of what happens to be showing — folding hides lines and
    /// renumbers nothing.
    pub(crate) fn cli_fold(&mut self, request: &Request, verb: &str) -> Outcome {
        if !crate::services::file_kind::folding_applies(self.document().path()) {
            return no(request, code::NOT_APPLICABLE, "There is nothing to fold in this tab.");
        }
        // A line the caller gave, turned into the paragraph number everything inside Unluminous counts in.
        let asked =
            request.number("line").filter(|line| *line >= 1.0).map(|line| line as usize - 1);
        match verb {
            "list" => {
                let regions = self.fold_report();
                let rows: Vec<String> = regions
                    .iter()
                    .map(|region| {
                        format!(
                            "{}{:>6}  to {:>6}  {:>5} lines  {}",
                            if region["collapsed"] == json!(true) { "*" } else { " " },
                            region["line"],
                            region["lastLine"],
                            region["hiddenLines"],
                            region["kind"].as_str().unwrap_or_default(),
                        )
                    })
                    .collect();
                let collapsed = regions.iter().filter(|it| it["collapsed"] == json!(true)).count();
                lines(
                    request,
                    format!(
                        "{} block{} that can be collapsed, {collapsed} collapsed",
                        regions.len(),
                        if regions.len() == 1 { "" } else { "s" }
                    ),
                    rows,
                    json!({ "regions": regions }),
                )
            }
            "toggle" => {
                let line = match asked {
                    Some(line) => line,
                    None => {
                        if !self.toggle_fold_at_caret() {
                            return no(request, code::NOT_APPLICABLE, self.take_the_message());
                        }
                        return self.fold_answer(request);
                    }
                };
                if !self.toggle_fold_at_line(line) {
                    return no(request, code::NOT_APPLICABLE, self.take_the_message());
                }
                self.fold_answer(request)
            }
            "collapse" | "expand" => {
                let collapse = verb == "collapse";
                let recursive = request.switch("recursive");
                if request.switch("all") {
                    if recursive {
                        return no(
                            request,
                            code::USAGE,
                            "--recursive needs --line; --all already covers the whole file.",
                        );
                    }
                    let changed =
                        if collapse { self.collapse_all_folds() } else { self.expand_all_folds() };
                    if !changed {
                        return no(
                            request,
                            code::NOT_APPLICABLE,
                            format!(
                                "There is nothing to {verb}{}.",
                                if collapse { "" } else { ": nothing is collapsed" }
                            ),
                        );
                    }
                    return self.fold_answer(request);
                }
                let Some(line) = asked else {
                    return no(
                        request,
                        code::USAGE,
                        format!("Say which block with --line, or --all to {verb} every one."),
                    );
                };
                if recursive {
                    // The line must head a block, which is what --line means for every fold command.
                    // Checked here rather than in the function so that "no block at this line" is a
                    // refusal and "the subtree is already in that state" is a no-op, as it is for
                    // the plain command: `fold collapse --line 7 --recursive` twice does not expand.
                    let index = self.files.active_index();
                    if !self.fold_marks(index).iter().any(|(head, _)| *head == line) {
                        return no(
                            request,
                            code::NOT_APPLICABLE,
                            format!("Nothing at line {} heads a block. Run `fold list`.", line + 1),
                        );
                    }
                    if collapse {
                        self.collapse_recursively_at_line(line);
                    } else {
                        self.expand_recursively_at_line(line);
                    }
                    return self.fold_answer(request);
                }
                // Asked for what it already is, this is a no-op rather than the opposite of what was
                // wanted: `fold collapse --line 40` twice must not expand line 40.
                let index = self.files.active_index();
                let already = self
                    .fold_marks(index)
                    .iter()
                    .find(|(head, _)| *head == line)
                    .map(|(_, shut)| *shut);
                match already {
                    None => no(
                        request,
                        code::NOT_APPLICABLE,
                        format!("Nothing at line {} heads a block. Run `fold list`.", line + 1),
                    ),
                    Some(shut) if shut == collapse => self.fold_answer(request),
                    Some(_) => {
                        self.toggle_fold_at_line(line);
                        self.fold_answer(request)
                    }
                }
            }
            "others" => {
                if !self.collapse_all_but_kept(request.switch("selection")) {
                    return no(request, code::NOT_APPLICABLE, self.take_the_message());
                }
                self.fold_answer(request)
            }
            _ => no(
                request,
                code::UNKNOWN_COMMAND,
                format!("There is no command called {}.", request.command),
            ),
        }
    }

    /// What every fold command that changed something answers with: how many blocks there are and
    /// how many are now collapsed.
    ///
    /// The sentence is the answer and the two numbers are the data a caller asked for by changing
    /// something. The region list is what `fold list` is for, and it comes along only when the
    /// caller asks for it with `--regions` — `task-1704` measured the study's agent paying for
    /// nine regions, four calls in a row, to read one sentence.
    fn fold_answer(&mut self, request: &Request) -> Outcome {
        let regions = self.fold_report();
        let collapsed = regions.iter().filter(|it| it["collapsed"] == json!(true)).count();
        let result = if request.switch("regions") {
            json!({ "collapsed": collapsed, "total": regions.len(), "regions": regions })
        } else {
            json!({ "collapsed": collapsed, "total": regions.len() })
        };
        ok(request, format!("{collapsed} of {} blocks collapsed", regions.len()), result)
    }

    /// The status bar message a fold command left behind, which is why it refused.
    fn take_the_message(&mut self) -> String {
        self.message.take().unwrap_or_else(|| "There is nothing to fold here.".to_owned())
    }

    /// `unluminous-cli highlight ...` — the passages marked in the project's files.
    ///
    /// Every one of these works on a file whether it is open or not, which is the point of the bulk
    /// commands: an agent that has worked out twenty places worth marking marks them in one call and
    /// none of the files has to be opened first. [`UnluminousApp::change_highlights`] is where the choice
    /// between the open document and the store is made, so nothing here has to think about it.
    pub(crate) fn cli_highlight(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "list" => self.cli_highlight_list(request),
            "add" => self.cli_highlight_add(request),
            "clear" => self.cli_highlight_clear(request),
            "apply" => self.cli_highlight_apply(request),
            _ => unknown(request),
        }
    }

    fn cli_highlight_list(&mut self, request: &Request) -> Outcome {
        let paths: Vec<PathBuf> = if request.switch("all") {
            let mut paths: Vec<PathBuf> =
                self.marks.files().iter().map(|(path, _)| (*path).clone()).collect();
            for path in self.files.paths() {
                if !self.highlights_of(&path).is_empty() && !paths.contains(&path) {
                    paths.push(path);
                }
            }
            paths.sort();
            paths
        } else {
            match self.highlight_target(request) {
                Ok(path) => vec![path],
                Err(refusal) => return no(request, code::USAGE, refusal),
            }
        };
        let mut rows: Vec<String> = Vec::new();
        let mut listed: Vec<Value> = Vec::new();
        for path in paths {
            let marks = self.highlights_of(&path);
            if marks.is_empty() {
                continue;
            }
            let text = self.highlight_text(&path).ok();
            for mark in marks.iter() {
                let (from, to) = match &text {
                    Some(text) => (
                        status_bar::position_of(text, mark.range.start),
                        status_bar::position_of(text, mark.range.end),
                    ),
                    None => (
                        status_bar::Position { line: 0, column: 0 },
                        status_bar::Position { line: 0, column: 0 },
                    ),
                };
                let words = text
                    .as_ref()
                    .map(|text| {
                        text.byte_slice(mark.range.start..mark.range.end.min(text.len_bytes()))
                    })
                    .unwrap_or_default();
                rows.push(format!(
                    "{:<10} {:>4}:{:<3} {:>4}:{:<3} {}",
                    mark.color.to_hex(),
                    from.line,
                    from.column,
                    to.line,
                    to.column,
                    self.tree_relative(&path)
                ));
                listed.push(json!({
                    "path": path.to_string_lossy(),
                    "start": mark.range.start,
                    "end": mark.range.end,
                    "fromLine": from.line,
                    "fromColumn": from.column,
                    "toLine": to.line,
                    "toColumn": to.column,
                    "color": mark.color.to_hex(),
                    "text": words,
                }));
            }
        }
        lines(
            request,
            format!("{} highlight{}", listed.len(), if listed.len() == 1 { "" } else { "s" }),
            rows,
            json!({ "highlights": listed }),
        )
    }

    fn cli_highlight_add(&mut self, request: &Request) -> Outcome {
        let path = match self.highlight_target(request) {
            Ok(path) => path,
            Err(refusal) => return no(request, code::USAGE, refusal),
        };
        let color = match request.text("color") {
            Some(name) => match HighlightColor::parse(&name) {
                Some(color) => color,
                None => {
                    return no(
                        request,
                        code::USAGE,
                        format!(
                            "{name} is not a colour. Say one of {}, or #rrggbb or #rrggbbaa.",
                            HighlightColor::names()
                        ),
                    )
                }
            },
            None => HighlightColor::Yellow.rgba(),
        };
        let text = match self.highlight_text(&path) {
            Ok(text) => text,
            Err(refusal) => return no(request, code::NOT_FOUND, refusal),
        };
        let ranges = match self.highlight_ranges(request, &text) {
            Ok(ranges) => ranges,
            Err(refusal) => return no(request, code::USAGE, refusal),
        };
        if ranges.is_empty() {
            return no(
                request,
                code::NOT_FOUND,
                "Nothing in that file matched, so nothing was marked.",
            );
        }
        let marked = ranges.len();
        self.change_highlights(&path, |marks| {
            for range in &ranges {
                marks.add(range.clone(), color);
            }
        });
        self.forget_layout();
        ok(
            request,
            format!(
                "Marked {marked} passage{} in {}",
                if marked == 1 { "" } else { "s" },
                self.tree_relative(&path)
            ),
            json!({
                "path": path.to_string_lossy(),
                "marked": marked,
                "color": color.to_hex(),
                "highlights": self.highlights_of(&path).len(),
            }),
        )
    }

    fn cli_highlight_clear(&mut self, request: &Request) -> Outcome {
        if request.switch("all") {
            // Counted before anything is taken away, and each file counted once. A file that is open
            // is in the store as well — the window pushes it there every frame — so adding the two
            // totals together would report twice as many as there were.
            let open: Vec<PathBuf> = self.files.paths();
            let mut cleared: usize = (0..self.files.len())
                .map(|index| self.files.at(index).document.highlights().len())
                .sum();
            cleared += self
                .marks
                .files()
                .iter()
                .filter(|(path, _)| !open.contains(path))
                .map(|(_, marks)| marks.len())
                .sum::<usize>();
            self.marks.clear_all();
            for index in 0..self.files.len() {
                self.files.at_mut(index).document.clear_highlights();
            }
            self.forget_layout();
            return ok(
                request,
                format!("Cleared {cleared} highlight{}", if cleared == 1 { "" } else { "s" }),
                json!({ "cleared": cleared }),
            );
        }
        let path = match self.highlight_target(request) {
            Ok(path) => path,
            Err(refusal) => return no(request, code::USAGE, refusal),
        };
        let before = self.highlights_of(&path).len();
        match request.whole("from-line") {
            None => {
                self.change_highlights(&path, |marks| {
                    marks.clear_all();
                });
            }
            Some(from_line) => {
                let text = match self.highlight_text(&path) {
                    Ok(text) => text,
                    Err(refusal) => return no(request, code::NOT_FOUND, refusal),
                };
                let from = offset_at(&text, from_line, 1);
                let to =
                    offset_at(&text, request.whole("to-line").unwrap_or(from_line), usize::MAX);
                self.change_highlights(&path, |marks| {
                    marks.clear(from..to);
                });
            }
        }
        self.forget_layout();
        let after = self.highlights_of(&path).len();
        ok(
            request,
            format!(
                "{} highlight{} left in {}",
                after,
                if after == 1 { "" } else { "s" },
                self.tree_relative(&path)
            ),
            json!({
                "path": path.to_string_lossy(),
                "cleared": before.saturating_sub(after),
                "highlights": after,
            }),
        )
    }

    fn cli_highlight_apply(&mut self, request: &Request) -> Outcome {
        let text = match (request.text("from-file"), request.text("json-text")) {
            (Some(named), _) => {
                let path = self.cli_path(&named);
                match std::fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(problem) => {
                        return no(
                            request,
                            code::NOT_FOUND,
                            format!("Could not read {}: {problem}", path.display()),
                        )
                    }
                }
            }
            (None, Some(inline)) => inline,
            (None, None) => {
                return no(request, code::USAGE, "Say --from-file <path> or --json-text <json>.")
            }
        };
        let wanted: Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(problem) => {
                return no(request, code::USAGE, format!("That is not JSON: {problem}"))
            }
        };
        let Some(list) = wanted.as_array() else {
            return no(
                request,
                code::USAGE,
                "The JSON should be an array of {path, fromLine, toLine, color} objects.",
            );
        };
        if request.switch("replace") {
            self.marks.clear_all();
            for index in 0..self.files.len() {
                self.files.at_mut(index).document.clear_highlights();
            }
        }
        let mut marked = 0;
        let mut touched: Vec<String> = Vec::new();
        let mut refused: Vec<String> = Vec::new();
        for (at, entry) in list.iter().enumerate() {
            match self.apply_one_highlight(entry) {
                Ok(path) => {
                    marked += 1;
                    let shown = self.tree_relative(&path);
                    if !touched.contains(&shown) {
                        touched.push(shown);
                    }
                }
                Err(refusal) => refused.push(format!("{}: {refusal}", at + 1)),
            }
        }
        self.forget_layout();
        let message = format!(
            "Marked {marked} passage{} across {} file{}{}",
            if marked == 1 { "" } else { "s" },
            touched.len(),
            if touched.len() == 1 { "" } else { "s" },
            if refused.is_empty() {
                String::new()
            } else {
                format!(", and refused {}", refused.len())
            }
        );
        lines(
            request,
            message,
            refused.clone(),
            json!({ "marked": marked, "files": touched, "refused": refused }),
        )
    }

    /// One entry of a bulk request. The error is what is reported against that entry's number, so a
    /// list with one bad row in it still applies the rest — which is what an agent wants of a batch.
    fn apply_one_highlight(&mut self, entry: &Value) -> Result<PathBuf, String> {
        let Some(named) = entry.get("path").and_then(Value::as_str) else {
            return Err("no path".to_owned());
        };
        let path = self.cli_path(named);
        let color = match entry.get("color").and_then(Value::as_str) {
            Some(name) => {
                HighlightColor::parse(name).ok_or_else(|| format!("{name} is not a colour"))?
            }
            None => HighlightColor::Yellow.rgba(),
        };
        let text = self.highlight_text(&path)?;
        let whole = |name: &str| entry.get(name).and_then(Value::as_u64).map(|n| n as usize);
        let from_line = whole("fromLine").ok_or_else(|| "no fromLine".to_owned())?;
        let from = offset_at(&text, from_line, whole("fromColumn").unwrap_or(1));
        let to_line = whole("toLine").unwrap_or(from_line);
        let to = offset_at(&text, to_line, whole("toColumn").unwrap_or(usize::MAX));
        if from >= to {
            return Err("the passage ends before it starts".to_owned());
        }
        self.change_highlights(&path, |marks| marks.add(from..to, color));
        Ok(path)
    }

    /// The file a highlight command is about: the one named, or the tab that is showing.
    fn highlight_target(&self, request: &Request) -> Result<PathBuf, String> {
        if let Some(path) = self.cli_path_argument(request, "path") {
            return Ok(path);
        }
        self.document()
            .path()
            .map(Path::to_path_buf)
            .ok_or_else(|| "This tab has never been saved, so name the file to mark.".to_owned())
    }

    /// The text of a file, whether it is open or not.
    ///
    /// An open file's document rather than what is on the disk, because what has been typed and not
    /// yet saved is what the person is looking at and is what the offsets have to be against.
    fn highlight_text(&self, path: &Path) -> Result<unluminous_core::Rope, String> {
        if let Some(index) = self.files.index_of(path) {
            return Ok(self.files.at(index).document.text().clone());
        }
        std::fs::read_to_string(path)
            .map(|text| unluminous_core::Rope::from_str(&text.replace("\r\n", "\n")))
            .map_err(|problem| format!("Could not read {}: {problem}", path.display()))
    }

    /// The ranges one `highlight add` asks for: a passage, or every occurrence of some words.
    fn highlight_ranges(
        &self,
        request: &Request,
        text: &unluminous_core::Rope,
    ) -> Result<Vec<std::ops::Range<usize>>, String> {
        if let Some(needle) = request.text("text") {
            let needle = unescape(&needle);
            if needle.is_empty() {
                return Err("Say what words to mark.".to_owned());
            }
            let whole = text.to_string();
            let mut out = Vec::new();
            let mut at = 0;
            while let Some(found) = whole[at..].find(&needle) {
                let start = at + found;
                out.push(start..start + needle.len());
                at = start + needle.len();
            }
            return Ok(out);
        }
        let Some(from_line) = request.whole("from-line") else {
            return Err(Self::missing_highlight_start(request));
        };
        let from = offset_at(text, from_line, request.whole("from-column").unwrap_or(1));
        let to_line = request.whole("to-line").unwrap_or(from_line);
        let to = offset_at(text, to_line, request.whole("to-column").unwrap_or(usize::MAX));
        if from >= to {
            return Err("The passage ends before it starts.".to_owned());
        }
        // Collected from one item rather than written as `vec![from..to]`, which clippy reads as
        // somebody who meant a list of every number between the two and warns about every build.
        Ok(std::iter::once(from..to).collect())
    }

    /// Explains the missing range start in terms of the range value the caller already supplied.
    ///
    /// Naming the absent field matters when `to-line` was supplied: repeating both valid options
    /// leaves an agent no reason to change its next request and can produce an identical-call loop.
    ///
    /// @param request - highlight request whose supplied range arguments should be described
    fn missing_highlight_start(request: &Request) -> String {
        if request.has("to-line") {
            "highlight add got --to-line but not --from-line; add --from-line N.".to_owned()
        } else {
            "highlight add needs --from-line and --to-line, or --text.".to_owned()
        }
    }

    /// A path as it is worth showing: relative to the project when it is inside it.
    fn tree_relative(&self, path: &Path) -> String {
        crate::services::project_state::relative(self.tree.root(), path).display().to_string()
    }
}
