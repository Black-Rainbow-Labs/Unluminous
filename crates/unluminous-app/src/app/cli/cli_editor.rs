//! `editor` -- the text itself: reading and writing it, the caret and the selection, undo, the
//! three view modes, the line commands and the comment toggles, and the syntactic tier of
//! definitions, references, rename and completion. Twenty-six commands, the largest single area in
//! the catalogue, because a text editor is what Unluminous is.
//!
//! `cli_offset` stays in `cli.rs` rather than here even though most of its callers are editor
//! commands, because `debug.rs`'s `cli_debug_hover` asks it the same question -- see the note beside
//! it there.

use super::*;

/// How many rows `editor complete` prints when the caller does not say.
///
/// `task-1804` §7.4 measured what having no default cost: `--stem rel` on this project returned
/// 1,274 rows and 430 KB, and `--stem a` returned **1.28 MB** -- roughly 320,000 tokens, more than
/// any model's context, out of one keystroke's worth of stem. `task-1704` set the rule for the whole
/// surface -- *answer in a payload proportionate to the question* -- and this was the one command
/// left out of it.
///
/// Fifty, because the rows are already ordered best first by the same scoring the popup uses, and a
/// fiftieth-best completion is not an answer anybody was going to read. The reply says how many
/// there were as well as how many were shown, so a caller can tell a complete answer from a cut one
/// without counting.
const COMPLETIONS_SHOWN: usize = 50;

impl UnluminousApp {
    /// The refusal every editor command answers with while a rendered page is the tab that is showing.
    ///
    /// A browser tab holds an empty document behind its native view, so without this an agent asking
    /// for the text of a web page is handed nothing and told it succeeded, and typing into one would
    /// mark a document nobody can see as modified.
    fn not_a_document(&self, request: &Request) -> Option<Outcome> {
        self.files.active().is_browser().then(|| {
            no(request, code::NOT_APPLICABLE, "This tab renders a web page rather than text. Use `browser status`, or open the file itself with `tab open`.")
        })
    }

    pub(crate) fn cli_editor(
        &mut self,
        request: &Request,
        verb: &str,
        ctx: &egui::Context,
    ) -> Outcome {
        // The file is read again first when something else has changed it, so a caller that wrote it
        // and asked the window about it is answered with what is there rather than with what the tab
        // last read. An edit is included on purpose: one made on top of stale text and then saved
        // would write the stale text back over somebody else's change.
        self.reread_if_the_file_changed();
        match verb {
            "status" => ok(request, self.editor_sentence(), self.editor_value()),
            "text" => self.cli_editor_text(request),
            "set-text" => self.cli_editor_set_text(request),
            "insert" => self.cli_editor_insert(request),
            "caret" => self.cli_editor_caret(request),
            "select" => self.cli_editor_select(request),
            "indent" => self.cli_editor_indent(request),
            "dedent" => self.cli_editor_dedent(request),
            "comment" => self.cli_editor_comment(request),
            "lines" => self.cli_editor_lines(request),
            "bracket" => self.cli_editor_bracket(request),
            "trim" => self.cli_editor_trim(request),
            "undo" => self.cli_editor_history(request, true),
            "redo" => self.cli_editor_history(request, false),
            "view" => self.cli_editor_view(request, ctx),
            "scroll" => self.cli_editor_scroll(request),
            "preview" => self.cli_editor_preview(request, ctx),
            "preview-select" => self.cli_editor_preview_select(request, ctx),
            "definition" => self.cli_editor_definition(request),
            "references" => self.cli_editor_references(request),
            "rename" => self.cli_editor_rename(request),
            "complete" => self.cli_editor_complete(request),
            "find" => self.cli_editor_find(request),
            "replace" => self.cli_editor_replace(request),
            "navigate-back" => self.cli_navigate(request, true),
            "navigate-forward" => self.cli_navigate(request, false),
            _ => unknown(request),
        }
    }

    /// `editor find` -- the Find bar, driven from the command line.
    ///
    /// **It opens the same bar a person opens**, which is `run_cli`'s rule rather than a
    /// convenience: an agent asking where a word is and a person pressing `Ctrl+F` reach the same
    /// state, so a screenshot taken after this shows the bar with the tally on it and the match
    /// selected. A second implementation that only counted would have been a second answer to the
    /// same question, and the two would have come to disagree.
    fn cli_editor_find(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        if request.switch("close") {
            if self.find.is_none() {
                return no(request, code::NOT_APPLICABLE, "The Find bar is not open.");
            }
            self.close_the_find();
            return ok(request, "Put the Find bar away.", json!({ "open": false }));
        }
        let asked = request.text("text").map(|text| text.to_owned());
        if self.find.is_none() {
            let selected = self.document().selected_text();
            let mut find = crate::services::find::Find::opened_with(Some(selected.as_str()), false);
            find.start_from(self.document().selection().start());
            self.find = Some(find);
        }
        let find = self.find.as_mut().expect("it is there now");
        if let Some(text) = asked {
            find.needle = text;
        }
        // A flag that is not given leaves the toggle as it was, so `--next` on its own does not
        // quietly turn `--match-case` off again.
        if request.switch("match-case") {
            find.match_case = true;
        }
        if request.switch("whole-word") {
            find.whole_word = true;
        }
        if find.needle.is_empty() {
            return no(request, code::USAGE, "Say what to look for.");
        }
        let text = self.document().text().to_string();
        let revision = self.document().text_revision();
        let find = self.find.as_mut().expect("it is there");
        find.refresh(&text, revision);
        if request.switch("next") {
            find.next();
        } else if request.switch("previous") {
            find.previous();
        }
        // The current match is **selected in the window**, so what the reply says and what the
        // window shows are the same thing -- `open_the_match`'s rule.
        if let Some(range) = self.find.as_ref().and_then(|find| find.current()) {
            self.select_the_match(range);
        }
        let find = self.find.as_ref().expect("it is there");
        let limit = match request.whole("limit") {
            Some(0) => find.count(),
            Some(asked) => asked,
            None => COMPLETIONS_SHOWN,
        };
        let needle = find.needle.clone();
        let (count, index) = (find.count(), find.index());
        let current = find.current();
        let shown: Vec<std::ops::Range<usize>> = find.all().iter().take(limit).cloned().collect();
        let rows: Vec<String> = shown
            .iter()
            .map(|range| {
                let at = status_bar::position_of(self.document().text(), range.start);
                format!("{:>5}:{:<4} {}", at.line, at.column, self.line_of_the_match(range.start))
            })
            .collect();
        let value: Vec<Value> = shown
            .iter()
            .map(|range| {
                let at = status_bar::position_of(self.document().text(), range.start);
                json!({
                    "line": at.line,
                    "column": at.column,
                    "start": range.start,
                    "end": range.end,
                    "current": Some(range.clone()) == current,
                })
            })
            .collect();
        lines(
            request,
            match count {
                0 => format!("Nothing matches '{needle}' in this file"),
                1 => format!("1 match for '{needle}'"),
                many => format!("{many} matches for '{needle}', on {}", index.unwrap_or(1)),
            },
            rows,
            json!({
                "needle": needle,
                "total": count,
                "shown": shown.len(),
                "current": index,
                "matchCase": self.find.as_ref().is_some_and(|find| find.match_case),
                "wholeWord": self.find.as_ref().is_some_and(|find| find.whole_word),
                "matches": value,
            }),
        )
    }

    /// The line a match is on, cut short so one minified megabyte is not the reply.
    fn line_of_the_match(&self, offset: usize) -> String {
        let text = self.document().text();
        let line = text.byte_to_line(offset);
        let start = text.line_to_byte(line);
        let end = match line + 1 < text.len_lines() {
            true => text.line_to_byte(line + 1),
            false => text.len_bytes(),
        };
        let whole = text.byte_slice(start..end);
        let trimmed = whole.trim_end_matches('\n');
        match trimmed.char_indices().nth(200) {
            Some((at, _)) => format!("{}...", &trimmed[..at]),
            None => trimmed.to_owned(),
        }
    }

    /// `editor replace` -- replace text in the file that is showing.
    ///
    /// **Without `--apply` it changes nothing**, which is `editor rename`'s shape and is here for
    /// the same reason: a replacement across a file is a change somebody wants to see the size of
    /// first. With it, every match goes in one `ReplaceMany`, which is one undo step.
    fn cli_editor_replace(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        let Some(text) = request.text("text").map(|text| text.to_owned()) else {
            return no(request, code::USAGE, "Say what to look for.");
        };
        if text.is_empty() {
            return no(request, code::USAGE, "Say what to look for.");
        }
        let Some(with) = request.text("with").map(|with| with.to_owned()) else {
            return no(request, code::USAGE, "Say what to put in its place.");
        };
        let mut find = crate::services::find::Find::opened_with(None, true);
        find.needle = text.clone();
        find.replacement = with.clone();
        find.match_case = request.switch("match-case");
        find.whole_word = request.switch("whole-word");
        let document_text = self.document().text().to_string();
        find.refresh(&document_text, self.document().text_revision());
        find.start_from(self.document().selection().start());
        let all = request.switch("all");
        let count = if all { find.count() } else { find.current().map_or(0, |_| 1) };
        if find.count() == 0 {
            return no(request, code::NOT_FOUND, format!("Nothing matches '{text}' in this file."));
        }
        if !request.switch("apply") {
            return ok(
                request,
                format!(
                    "{} of {} would be replaced with '{with}'. Nothing was changed; add --apply.",
                    count,
                    find.count()
                ),
                json!({ "total": find.count(), "wouldChange": count, "applied": false }),
            );
        }
        // The bar is left open on the search that was applied, so the window shows what happened and
        // a person can carry on from it -- the same state a person would be in having pressed the
        // buttons themselves.
        self.find = Some(find);
        if all {
            self.replace_every_match();
        } else {
            self.replace_the_current_match();
        }
        let remaining = {
            let text = self.document().text().to_string();
            let revision = self.document().text_revision();
            let find = self.find.as_mut().expect("it is there");
            find.refresh(&text, revision);
            find.count()
        };
        ok(
            request,
            format!(
                "Replaced {count} {} with '{with}'. One undo puts them all back.",
                if count == 1 { "match" } else { "matches" }
            ),
            json!({ "changed": count, "remaining": remaining, "applied": true }),
        )
    }

    /// `unluminous-cli editor navigate-back` and its mirror, through the same stack the menu walks.
    fn cli_navigate(&mut self, request: &Request, back: bool) -> Outcome {
        self.message = None;
        self.navigate(back);
        match self.message.clone() {
            // `navigate` says so in the status bar when there is nowhere to go, and a command that
            // did nothing should say so rather than report success.
            Some(problem) if problem.starts_with("There is nowhere") => {
                no(request, code::NOT_APPLICABLE, problem)
            }
            _ => ok(
                request,
                format!("{} \u{00B7} {}", self.files.active().name(), self.caret_position().line),
                json!({
                    "path": self.files.active().path().map(|path| path.to_string_lossy()),
                    "offset": self.caret_offset(),
                    "back": self.back.len(),
                    "forward": self.forward.len(),
                }),
            ),
        }
    }

    /// `unluminous-cli editor scroll` — how far through the file the view is, and moving it.
    ///
    /// The page it measures against is the one the window laid out on the last frame it drew, which
    /// is the same page the wheel and the scrollbar move. In side by side the other half follows,
    /// through the same `follow_the_other_half` a wheel goes through — the frame's own rule cannot
    /// notice this one, because a command is applied before the frame draws anything and so there is
    /// nothing for its before-and-after comparison to see.
    fn cli_editor_scroll(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        if self.files.active().is_picture() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "This tab holds a picture, which is panned rather than scrolled.",
            );
        }
        let preview = request.switch("preview");
        let room = (self.editor_area.height() - size::EDITOR_PADDING_Y * 2.0).max(0.0);
        let page = if preview { self.preview_layout() } else { self.layout() };
        let height = page.height;
        let overflow = (height - room).max(0.0);
        let wanted = if request.switch("top") {
            Some(0.0)
        } else if request.switch("bottom") {
            Some(overflow)
        } else if let Some(line) = request.whole("line") {
            // Counting from one, as the status bar and `editor caret` do.
            let paragraph = line.max(1) - 1;
            Some(page.paragraph_band(paragraph).map(|(top, _)| top).unwrap_or(overflow))
        } else {
            request.number("to").map(|points| points as f32)
        };
        if let Some(to) = wanted {
            let to = to.clamp(0.0, overflow);
            let file = self.files.active_mut();
            if preview {
                file.preview_scroll = to;
            } else {
                file.scroll = to;
            }
            if self.view_mode() == ViewMode::SideBySide {
                self.follow_the_other_half(!preview, room);
            }
        }
        let file = self.files.active();
        ok(
            request,
            format!(
                "{} \u{00B7} source {:.0} of {:.0} \u{00B7} preview {:.0} of {:.0}",
                file.name(),
                file.scroll,
                file.cached.layout.height,
                file.preview_scroll,
                file.cached.preview_layout.height,
            ),
            json!({
                "tab": self.files.active_index(),
                "name": file.name(),
                "view": room,
                "source": { "scroll": file.scroll, "height": file.cached.layout.height },
                "preview": {
                    "scroll": file.preview_scroll,
                    "height": file.cached.preview_layout.height,
                },
                "viewMode": view_mode_name(self.view_mode()),
            }),
        )
    }

    fn editor_sentence(&self) -> String {
        let at = self.caret_position();
        format!(
            "{} \u{00B7} {} lines \u{00B7} line {} column {}{}",
            self.files.active().name(),
            self.document().text().len_lines(),
            at.line,
            at.column,
            if self.document().is_modified() { " \u{00B7} unsaved" } else { "" }
        )
    }

    pub(crate) fn editor_value(&self) -> Value {
        let file = self.files.active();
        let at = self.caret_position();
        let selection = self.document().selection();
        json!({
            "tab": self.files.active_index(),
            "name": file.name(),
            "path": file.path().map(|path| path.to_string_lossy()),
            "picture": file.is_picture(),
            "browser": file.is_browser(),
            "modified": self.document().is_modified(),
            "lines": self.document().text().len_lines(),
            "characters": self.document().text().len_chars(),
            "caret": { "line": at.line, "column": at.column, "offset": selection.head },
            "selection": {
                "empty": selection.is_empty(),
                "start": selection.start(),
                "end": selection.end(),
                "text": self.document().selected_text(),
            },
            "viewMode": view_mode_name(self.view_mode()),
            "canUndo": self.document().can_undo(),
            "canRedo": self.document().can_redo(),
            "previewApplies": file_kind::preview_applies(file.path()),
            "kind": file_kind::kind_name(file.path()),
            // What the file was on disk and what saving it will write. The same two facts the status
            // bar draws, so an agent can read what a person can see -- which is the rule the whole
            // product is built on. `task-1804` §7.1 and §7.6.
            //
            // `lineEnding` is what would be **written**, so it already has `editor.line_ending`
            // applied; `fileLineEnding` is what was **read**. They differ only when the setting is
            // not `keep`, and a caller that wants to know whether saving would rewrite the file
            // compares them.
            "lineEnding": self
                .settings
                .line_endings
                .applied_to(self.document().line_ending())
                .name(),
            "fileLineEnding": self.document().line_ending().name(),
            "encoding": self.document().encoding().name(),
            // False for a file read in an encoding Unluminous does not write. Saving it refuses with
            // the encoding named, rather than re-encoding somebody's file.
            "writable": self.document().writable(),
        })
    }

    fn cli_editor_text(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        if self.files.active().is_picture() {
            return no(request, code::NOT_APPLICABLE, "This tab holds a picture rather than text.");
        }
        let whole = self.document().text().to_string();
        let from = request.whole("from-line").unwrap_or(1).max(1);
        let to = request.whole("to-line");
        let text = if from == 1 && to.is_none() {
            whole
        } else {
            let all: Vec<&str> = whole.split_inclusive('\n').collect();
            let last = to.unwrap_or(all.len()).min(all.len());
            if from > last {
                String::new()
            } else {
                all[from - 1..last].concat()
            }
        };
        ok(request, String::new(), json!({ "text": text, "fromLine": from, "toLine": to }))
    }

    fn cli_editor_set_text(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        if self.files.active().is_picture() {
            return no(request, code::NOT_APPLICABLE, "This tab holds a picture rather than text.");
        }
        let text = match request.text("from-file") {
            Some(named) => {
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
            None => unescape(&request.text("text").unwrap_or_default()),
        };
        // Selecting everything and typing over it, which is one edit and therefore one undo.
        self.document_mut().apply(unluminous_core::Command::SelectAll);
        self.document_mut().apply(unluminous_core::Command::Insert(text.clone()));
        self.forget_layout();
        ok(
            request,
            format!("Replaced the text with {} characters", text.chars().count()),
            json!({ "characters": text.chars().count(), "lines": self.document().text().len_lines() }),
        )
    }

    fn cli_editor_insert(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        if self.files.active().is_picture() {
            return no(request, code::NOT_APPLICABLE, "This tab holds a picture rather than text.");
        }
        let Some(text) = request.text("text") else {
            return no(request, code::USAGE, "Say what to type.");
        };
        let text = unescape(&text);
        self.document_mut().apply(unluminous_core::Command::Insert(text.clone()));
        self.forget_layout();
        self.reveal_caret = true;
        let at = self.caret_position();
        ok(
            request,
            format!("Typed {} characters", text.chars().count()),
            json!({ "caret": { "line": at.line, "column": at.column } }),
        )
    }

    fn cli_editor_caret(&mut self, request: &Request) -> Outcome {
        let line = request.whole("line");
        let column = request.whole("column");
        if line.is_none() && column.is_none() {
            let at = self.caret_position();
            return ok(
                request,
                format!("Line {} column {}", at.line, at.column),
                json!({ "line": at.line, "column": at.column, "offset": self.document().selection().head }),
            );
        }
        let here = self.caret_position();
        let offset =
            offset_at(self.document().text(), line.unwrap_or(here.line), column.unwrap_or(1));
        self.document_mut().apply(unluminous_core::Command::PlaceCaret { offset, extend: false });
        self.reveal_caret = true;
        let at = self.caret_position();
        ok(
            request,
            format!("The caret is at line {} column {}", at.line, at.column),
            json!({ "line": at.line, "column": at.column, "offset": offset }),
        )
    }

    fn cli_editor_select(&mut self, request: &Request) -> Outcome {
        if request.switch("all") {
            self.document_mut().apply(unluminous_core::Command::SelectAll);
        } else if request.switch("none") {
            let head = self.document().selection().head;
            self.document_mut()
                .apply(unluminous_core::Command::PlaceCaret { offset: head, extend: false });
        } else {
            let Some(from_line) = request.whole("from-line") else {
                return no(
                    request,
                    code::USAGE,
                    "Say --all, --none, or at least --from-line and --to-line.",
                );
            };
            let text = self.document().text();
            let from = offset_at(text, from_line, request.whole("from-column").unwrap_or(1));
            let to_line = request.whole("to-line").unwrap_or(from_line);
            let to = offset_at(text, to_line, request.whole("to-column").unwrap_or(usize::MAX));
            self.document_mut()
                .apply(unluminous_core::Command::PlaceCaret { offset: from, extend: false });
            self.document_mut()
                .apply(unluminous_core::Command::PlaceCaret { offset: to, extend: true });
        }
        self.reveal_caret = true;
        let selection = self.document().selection();
        let chosen = self.document().selected_text();
        ok(
            request,
            format!("{} characters selected", chosen.chars().count()),
            json!({
                "start": selection.start(),
                "end": selection.end(),
                "characters": chosen.chars().count(),
                "text": chosen,
            }),
        )
    }

    /// `unluminous-cli editor indent` — the agent's half of what `Tab` and `Space` do over a selection,
    /// through the same command the keys do, so an indent done by an agent and the same thing done
    /// by hand are the same thing.
    fn cli_editor_indent(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        if self.files.active().is_picture() {
            return no(request, code::NOT_APPLICABLE, "This tab holds a picture rather than text.");
        }
        let unit = if request.switch("space") {
            unluminous_core::IndentUnit::Space
        } else {
            unluminous_core::IndentUnit::Tab
        };
        let selection = self.document().selection();
        let text = self.document().text();
        let lines = if selection.is_empty() {
            1
        } else {
            text.byte_to_line(selection.end() - 1) - text.byte_to_line(selection.start()) + 1
        };
        self.document_mut().apply(unluminous_core::Command::Indent { unit });
        self.forget_layout();
        self.reveal_caret = true;
        let selection = self.document().selection();
        let what = if unit == unluminous_core::IndentUnit::Space { "a space" } else { "a tab" };
        ok(
            request,
            format!("Indented {lines} line{} with {what}", if lines == 1 { "" } else { "s" }),
            json!({
                "lines": lines,
                "unit": if unit == unluminous_core::IndentUnit::Space { "space" } else { "tab" },
                "start": selection.start(),
                "end": selection.end(),
            }),
        )
    }

    /// `unluminous-cli editor dedent` — the agent's half of what `Shift+Tab` and `Shift+Space` do over a
    /// selection, through the same command the keys do, so a dedent done by an agent and the same
    /// thing done by hand are the same thing.
    fn cli_editor_dedent(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        if self.files.active().is_picture() {
            return no(request, code::NOT_APPLICABLE, "This tab holds a picture rather than text.");
        }
        let unit = if request.switch("space") {
            unluminous_core::IndentUnit::Space
        } else {
            unluminous_core::IndentUnit::Tab
        };
        let selection = self.document().selection();
        let text = self.document().text();
        let lines = if selection.is_empty() {
            1
        } else {
            text.byte_to_line(selection.end() - 1) - text.byte_to_line(selection.start()) + 1
        };
        let what = if unit == unluminous_core::IndentUnit::Space { "a space" } else { "a tab" };
        let changed = self.document_mut().apply(unluminous_core::Command::Dedent { unit });
        if !changed {
            return ok(
                request,
                format!("Nothing to remove: no touched line starts with {what}"),
                json!({
                    "lines": 0,
                    "unit": if unit == unluminous_core::IndentUnit::Space { "space" } else { "tab" },
                }),
            );
        }
        self.forget_layout();
        self.reveal_caret = true;
        let selection = self.document().selection();
        ok(
            request,
            format!("Removed {what} from the start of the touched lines that had one"),
            json!({
                "lines": lines,
                "unit": if unit == unluminous_core::IndentUnit::Space { "space" } else { "tab" },
                "start": selection.start(),
                "end": selection.end(),
            }),
        )
    }

    /// `unluminous-cli editor comment` -- the agent's half of `Cmd/Ctrl+/` and `Cmd/Ctrl+Shift+/`.
    ///
    /// Through `toggle_line_comment` and `toggle_block_comment`, the same two functions the menu
    /// entries call, so the marker an agent gets and the marker a person gets come from the same
    /// plugin. `--toggle` is the default because a command given neither flag means the commoner of
    /// the two, and naming it explicitly is what `task-1804`'s rule about a dropped key asks for.
    fn cli_editor_comment(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        if request.switch("block") && request.switch("toggle") {
            return no(
                request,
                code::USAGE,
                "Say --toggle or --block, not both: one comments each line and the other wraps the whole selection once.",
            );
        }
        let block = request.switch("block");
        let commented = match block {
            true => self.toggle_block_comment(),
            false => self.toggle_line_comment(),
        };
        let lines = match commented {
            Ok(lines) => lines,
            Err(problem) => return no(request, code::NOT_APPLICABLE, problem),
        };
        let marker = match block {
            true => self.block_comment_markers().map(|(open, close)| format!("{open} {close}")),
            false => self.line_comment_marker(),
        }
        .unwrap_or_default();
        ok(
            request,
            format!("Toggled the comment on {lines} line{}", if lines == 1 { "" } else { "s" }),
            json!({
                "lines": lines,
                "kind": if block { "block" } else { "line" },
                "marker": marker,
            }),
        )
    }

    /// `unluminous-cli editor lines <what>` -- duplicate, move, join and sort, through the four
    /// `Command` variants the menu entries use.
    ///
    /// One verb with a word after it rather than four verbs, because they are one family and an
    /// agent looking for "the line commands" finds them in one place -- which is `task-1695`'s rule
    /// about naming a command the way an agent guesses.
    fn cli_editor_lines(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        if !self.line_edits_apply_here() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "This tab holds no text to edit by the line.",
            );
        }
        let Some(what) = request.text("what") else {
            return no(request, code::USAGE, "Say duplicate, move, join or sort.");
        };
        match what.trim().to_lowercase().as_str() {
            "duplicate" => {
                let lines = self.duplicate_lines();
                ok(
                    request,
                    format!("Duplicated {lines} line{}", if lines == 1 { "" } else { "s" }),
                    json!({ "lines": lines }),
                )
            }
            "move" => {
                // A direction and a count, which is what `Command::MoveLines` takes and what the two
                // chords send. One down when it is left out, because a move with no direction named
                // is not a thing anybody means.
                let by = request.number("by").unwrap_or(1.0) as i32;
                if by == 0 {
                    return no(request, code::USAGE, "--by 0 would move nothing. Use -1 or 1.");
                }
                let moved = self.move_lines(by);
                let where_to = if by < 0 { "up" } else { "down" };
                ok(
                    request,
                    match moved {
                        true => format!("Moved {} line{where_to}", by.abs()),
                        false => format!(
                            "Nothing moved: those lines are already as far {where_to} as they go"
                        ),
                    },
                    json!({ "moved": moved, "by": by }),
                )
            }
            "join" => {
                let joined = self.join_lines();
                ok(
                    request,
                    match joined {
                        true => "Joined the lines".to_owned(),
                        false => "There is no line below this one to join it to".to_owned(),
                    },
                    json!({ "joined": joined }),
                )
            }
            "sort" => match self.sort_lines() {
                Ok(lines) => {
                    ok(request, format!("Sorted {lines} lines"), json!({ "lines": lines }))
                }
                Err(problem) => no(request, code::NOT_APPLICABLE, problem),
            },
            other => no(
                request,
                code::USAGE,
                format!(
                    "There is no line command called {other}. It is duplicate, move, join or sort."
                ),
            ),
        }
    }

    /// `unluminous-cli editor bracket` -- both ends of the pair, and optionally a jump to the other.
    ///
    /// **Both offsets**, which is what `task-1922` §5.5 asks for: an agent that was handed only the
    /// answer would have to work out where it asked from. The positions are line and column beside
    /// the bytes, because that is what every other editor command prints and what a person reads.
    fn cli_editor_bracket(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        let offset = match self.cli_offset(request) {
            Ok(offset) => offset,
            Err(problem) => return no(request, code::USAGE, problem),
        };
        let Some((here, other)) = self.bracket_pair_at(offset) else {
            return no(
                request,
                code::NOT_FOUND,
                "There is no bracket there with a partner in this file. A bracket inside a comment or a string is not matched, and a pair more than 64 KB apart is not searched for.",
            );
        };
        if request.switch("go") {
            self.document_mut()
                .apply(unluminous_core::Command::PlaceCaret { offset: other, extend: false });
            self.reveal_caret = true;
        }
        let text = self.document().text();
        let at = status_bar::position_of(text, here);
        let partner = status_bar::position_of(text, other);
        ok(
            request,
            format!(
                "The bracket at line {} column {} answers the one at line {} column {}",
                at.line, at.column, partner.line, partner.column
            ),
            json!({
                "from": here,
                "to": other,
                "fromLine": at.line,
                "fromColumn": at.column,
                "toLine": partner.line,
                "toColumn": partner.column,
                "moved": request.switch("go"),
            }),
        )
    }

    /// `unluminous-cli editor trim` -- the `editor.trim` setting asked for once.
    fn cli_editor_trim(&mut self, request: &Request) -> Outcome {
        if let Some(refusal) = self.not_a_document(request) {
            return refusal;
        }
        match self.trim_trailing_whitespace() {
            Ok(trimmed) => ok(
                request,
                match trimmed {
                    true => "Trimmed the trailing whitespace".to_owned(),
                    false => "No line ends in whitespace".to_owned(),
                },
                json!({ "trimmed": trimmed }),
            ),
            Err(problem) => no(request, code::NOT_APPLICABLE, problem),
        }
    }

    fn cli_editor_history(&mut self, request: &Request, undo: bool) -> Outcome {
        let possible = if undo { self.document().can_undo() } else { self.document().can_redo() };
        if !possible {
            return no(
                request,
                code::NOT_APPLICABLE,
                if undo { "There is nothing to undo." } else { "There is nothing to redo." },
            );
        }
        let command =
            if undo { unluminous_core::Command::Undo } else { unluminous_core::Command::Redo };
        self.document_mut().apply(command);
        self.forget_layout();
        done(request, if undo { "Undone" } else { "Redone" })
    }

    fn cli_editor_view(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        let Some(name) = request.text("mode") else {
            return no(request, code::USAGE, "Say raw, side or preview.");
        };
        let mode = match name.trim() {
            "raw" | "source" => ViewMode::Raw,
            "side" | "side-by-side" => ViewMode::SideBySide,
            "preview" => ViewMode::Preview,
            other => {
                return no(
                    request,
                    code::USAGE,
                    format!("{other} is not a view mode. Say raw, side or preview."),
                )
            }
        };
        if mode != ViewMode::Raw && !file_kind::preview_applies(self.document().path()) {
            return no(
                request,
                code::NOT_APPLICABLE,
                format!(
                    "{} has no preview, so only the raw view applies to it.",
                    self.files.active().name()
                ),
            );
        }
        self.run_action(Action::SetViewMode(mode), ctx);
        ok(
            request,
            format!("Showing the {} view", view_mode_name(mode)),
            json!({ "viewMode": view_mode_name(mode) }),
        )
    }

    /// What `editor preview` answers for a file that is a diagram all the way through.
    ///
    /// The scene's own numbers rather than a picture: how many things were drawn, how large it came
    /// out, and every piece of text in it — which is enough for a script, or an agent, to tell that
    /// the right diagram was drawn without being able to look at it.
    fn cli_editor_diagram(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        let source = self.document().text().to_string();
        let base = self.diagram_style();
        let theme = crate::services::mermaid_scene::theme();
        if !self.mermaid_is_enabled() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "The Mermaid plugin is switched off, so this file is not drawn as a diagram."
                    .to_owned(),
            );
        }
        let kind = unluminous_core::mermaid::kind(&source).map(|kind| kind.name().to_owned());
        let metrics =
            crate::services::mermaid_scene::EguiMetrics::new(ctx, self.bold_family.clone());
        match self.mermaid_scenes.scene(&source, &base, &metrics, &theme) {
            Ok(scene) => {
                let texts: Vec<String> = scene.texts().into_iter().map(str::to_owned).collect();
                ok(
                    request,
                    String::new(),
                    json!({
                        "diagram": kind,
                        "width": scene.size.width,
                        "height": scene.size.height,
                        "items": scene.items.len(),
                        "text": texts,
                    }),
                )
            }
            Err(problem) => no(
                request,
                code::NOT_APPLICABLE,
                format!("{} could not be drawn. {}", self.files.active().name(), problem.message()),
            ),
        }
    }
    /// `unluminous-cli editor definition [name]` — where a name is defined.
    ///
    /// With no name it goes through the same function the menu entry uses, including the pivot to
    /// references when the caret is on the definition. A named request needs no occurrence in the
    /// active file and still uses the same ranked candidates and navigation history.
    fn cli_editor_definition(&mut self, request: &Request) -> Outcome {
        let (name, offset, named) = match self.cli_definition_target(request) {
            Ok(target) => target,
            Err(problem) => return *problem,
        };
        let path = self.files.active().path().map(Path::to_path_buf);
        let candidates = self.candidates_for(&name, path.as_deref(), offset);
        let rows: Vec<Value> = candidates
            .iter()
            .map(|candidate| {
                json!({
                    "path": candidate.path.to_string_lossy(),
                    "offset": candidate.name_range.start,
                    "end": candidate.name_range.end,
                    "kind": candidate.kind.name(),
                    "confidence": match candidate.confidence {
                        unluminous_core::symbols::Confidence::Sure => "sure",
                        unluminous_core::symbols::Confidence::Likely => "likely",
                    },
                    "open": candidate.open,
                })
            })
            .collect();
        if request.switch("open") {
            match named {
                true => self.go_to_named_definition(&name, candidates),
                false => self.go_to_definition(offset),
            }
            let sentence = self.message.clone().unwrap_or_else(|| format!("Went to '{name}'"));
            return ok(request, sentence, json!({ "name": name, "candidates": rows }));
        }
        let sentence = match rows.len() {
            0 => format!("No definition found for '{name}'"),
            1 => format!("'{name}' is defined once"),
            many => format!("'{name}' has {many} candidate definitions"),
        };
        ok(request, sentence, json!({ "name": name, "candidates": rows }))
    }

    /// Resolve the exact name a definition request asks about, keeping the caret as the default.
    ///
    /// The error is boxed for the same reason `cli_find_tab`'s is: `Outcome` carries the whole of
    /// `Waiting`, which clippy flags as too large to return unboxed.
    fn cli_definition_target(
        &mut self,
        request: &Request,
    ) -> Result<(String, usize, bool), Box<Outcome>> {
        if let Some(name) = request.text("name").filter(|name| !name.trim().is_empty()) {
            return Ok((name.trim().to_owned(), self.caret_offset(), true));
        }
        if !self.definitions_apply_here() {
            return Err(Box::new(no(
                request,
                code::NOT_APPLICABLE,
                "This file's language has not said what a definition looks like, so there is none to go to.",
            )));
        }
        let offset = self
            .cli_offset(request)
            .map_err(|problem| Box::new(no(request, code::USAGE, problem)))?;
        let name = self.symbol_at(offset).ok_or_else(|| {
            Box::new(no(request, code::NOT_APPLICABLE, "There is no symbol at that position."))
        })?;
        Ok((name, offset, false))
    }

    /// `unluminous-cli editor complete` — the names the word being typed could become.
    ///
    /// It goes through the same two functions the popup does: `completion_offer` works out what a
    /// row replaces and what the rows are, and `--choose` opens the state exactly as `Ctrl+Space`
    /// would and then accepts it
    /// exactly as `Enter` would. So a thing done from the command line and the same thing done by
    /// hand really are the same thing, and the list a script reads is the list a person is looking
    /// at rather than a second answer worked out beside it.
    ///
    /// Listing changes nothing at all — not even which row is chosen — because a script asking what
    /// is on offer must not move a popup somebody is steering.
    fn cli_editor_complete(&mut self, request: &Request) -> Outcome {
        if !self.completion_applies_here() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "No plugin claims this file, so Unluminous has no words to offer.",
            );
        }
        let hypothetical = request.text("stem").map(|stem| stem.trim().to_owned());
        if request.has("stem") && hypothetical.as_deref().is_none_or(str::is_empty) {
            return no(request, code::USAGE, "A hypothetical stem cannot be empty.");
        }
        if hypothetical.is_some() && request.has("choose") {
            return no(
                request,
                code::USAGE,
                "--stem is a read-only question and cannot be combined with --choose.",
            );
        }
        let offset = match self.cli_offset(request) {
            Ok(offset) => offset,
            Err(problem) => return no(request, code::USAGE, problem),
        };
        if let Some(name) = request.text("choose") {
            return self.cli_editor_complete_choose(request, offset, name.trim());
        }
        let offer = match hypothetical.as_deref() {
            Some(stem) => self.hypothetical_completion_offer(offset, stem),
            None => self.completion_offer(offset),
        };
        let (stem, word, rows) = (offer.range, offer.typed, offer.rows);
        // An empty stem with rows behind it is `task-1680`'s one new shape: inside a module
        // specifier there is a real answer to a question with nothing typed in it.
        if word.is_empty() && rows.is_empty() {
            return no(request, code::NOT_APPLICABLE, "There is nothing to complete here.");
        }
        // **[`COMPLETIONS_SHOWN`], when nothing is asked for.** `--limit 0` means all of them, so
        // nothing that could be asked for before has been taken away; it just has to be asked for.
        let limit = match request.whole("limit") {
            Some(0) => rows.len(),
            Some(asked) => asked,
            None => COMPLETIONS_SHOWN,
        };
        let shown: Vec<&unluminous_core::completion::Row> = rows.iter().take(limit).collect();
        let lines_of_it: Vec<String> = shown
            .iter()
            .map(|row| {
                format!(
                    "{:<32}{:<10}{:<10}{}",
                    row.name,
                    row.kind.map_or("", |kind| kind.name()),
                    row.source.name(),
                    row.detail
                )
            })
            .collect();
        let value: Vec<Value> = shown
            .iter()
            .map(|row| {
                json!({
                    "name": row.name,
                    "kind": row.kind.map(|kind| kind.name()),
                    "source": row.source.name(),
                    "detail": row.detail,
                    "matched": row.matched,
                })
            })
            .collect();
        lines(
            request,
            match rows.len() {
                0 => format!("Nothing completes '{word}'"),
                1 => format!("1 completion for '{word}'"),
                many if many == shown.len() => format!("{many} completions for '{word}'"),
                many => format!(
                    "{many} completions for '{word}', {} shown - ask for more with --limit",
                    shown.len()
                ),
            },
            lines_of_it,
            json!({
                "stem": word,
                "offset": stem.start,
                "end": stem.end,
                "total": rows.len(),
                "shown": shown.len(),
                "rows": value,
            }),
        )
    }

    /// `--choose`: apply one of the offered rows, as pressing `Enter` on it would.
    ///
    /// The caret is moved to the point the question was asked about first, so that `--offset` and
    /// `--line` mean the same thing here as they do when the rows are being listed, and so that the
    /// edit lands where the caret is — which is the one place an accept can land.
    fn cli_editor_complete_choose(
        &mut self,
        request: &Request,
        offset: usize,
        name: &str,
    ) -> Outcome {
        if name.is_empty() {
            return no(request, code::USAGE, "Say which completion to take.");
        }
        self.document_mut().apply(unluminous_core::Command::PlaceCaret { offset, extend: false });
        self.complete_word();
        let Some(state) = self.completion.as_ref() else {
            return no(
                request,
                code::NOT_APPLICABLE,
                self.message
                    .clone()
                    .unwrap_or_else(|| "There is nothing to complete here.".to_owned()),
            );
        };
        let stem = self.document().text().byte_slice(state.stem.clone());
        if !self.choose_the_completion(name) {
            let offered: Vec<String> = self
                .completion
                .as_ref()
                .map(|state| state.rows.iter().take(8).map(|row| row.name.clone()).collect())
                .unwrap_or_default();
            self.close_the_completion();
            return no(
                request,
                code::NOT_FOUND,
                format!(
                    "'{name}' is not one of the completions for '{stem}'. These are: {}.",
                    offered.join(", ")
                ),
            );
        }
        if !self.accept_the_completion(false) {
            return no(request, code::FAILED, format!("'{name}' could not be applied."));
        }
        ok(
            request,
            format!("Completed '{stem}' to '{name}'"),
            json!({ "stem": stem, "name": name, "caret": self.caret_offset() }),
        )
    }

    /// `unluminous-cli editor references` — every place a name is used.
    ///
    /// The modal is opened and waited for rather than a second search being run beside it: the
    /// modal *is* the search, so what a script reads is exactly what a person would be looking at.
    fn cli_editor_references(&mut self, request: &Request) -> Outcome {
        let name = match request.text("name").as_deref() {
            Some(name) if !name.trim().is_empty() => name.trim().to_owned(),
            _ => {
                if !self.symbols_apply_here() {
                    return no(
                        request,
                        code::NOT_APPLICABLE,
                        "No plugin claims this file, so Unluminous cannot tell one of its words from another.",
                    );
                }
                let offset = self.caret_offset();
                match self.symbol_at(offset) {
                    Some(name) => name,
                    None => {
                        return no(
                            request,
                            code::NOT_APPLICABLE,
                            "There is no symbol at the caret, so name what to look for.",
                        )
                    }
                }
            }
        };
        self.tree.reload();
        let waker = self.thread_waker();
        self.references = Some(References::open(references::Purpose::References, &name, waker));
        Outcome::Hold(Waiting::References {
            until: Instant::now() + self.cli_timeout(request),
            code_only: request.switch("code-only"),
            rename: None,
        })
    }

    /// `unluminous-cli editor rename` — the change set, and applying it.
    ///
    /// The scope and the roles are the modal's own default-tick rules as flags, which is what makes
    /// twenty renames across a project scriptable the way `highlight apply` already is. Without
    /// `--apply` nothing is edited and the change set is printed, because a rename is exactly the
    /// sort of thing a script should be able to look at before it leaps.
    fn cli_editor_rename(&mut self, request: &Request) -> Outcome {
        let Some(to) = request.text("new-name") else {
            return no(request, code::USAGE, "Say what to call it.");
        };
        if !self.symbols_apply_here() {
            return no(
                request,
                code::NOT_APPLICABLE,
                "No plugin claims this file, so Unluminous cannot tell one of its words from another.",
            );
        }
        let offset = self.caret_offset();
        let from = match request.text("name") {
            Some(name) if !name.trim().is_empty() => name.trim().to_owned(),
            _ => match self.symbol_at(offset) {
                Some(name) => name,
                None => {
                    return no(
                        request,
                        code::NOT_APPLICABLE,
                        "There is no symbol at the caret, so name what to rename.",
                    )
                }
            },
        };
        let grammar = self.grammar_for(self.files.active().path()).cloned().unwrap_or_default();
        if let Err(reason) = unluminous_core::symbols::check_name(to.trim(), &grammar) {
            return no(request, code::USAGE, reason);
        }
        let scope = match request.text("scope").as_deref() {
            None => None,
            Some("file") => Some(true),
            Some("project") => Some(false),
            Some(other) => {
                return no(
                    request,
                    code::USAGE,
                    format!("`{other}` is not a scope. It is `file` or `project`."),
                )
            }
        };
        let include: Vec<String> = request
            .text("include")
            .unwrap_or_default()
            .split(',')
            .map(|part| part.trim().to_lowercase())
            .filter(|part| !part.is_empty())
            .collect();
        for named in &include {
            if named != "comments" && named != "strings" {
                return no(
                    request,
                    code::USAGE,
                    format!(
                        "`{named}` is not something to include. It is `comments` or `strings`."
                    ),
                );
            }
        }
        // The same resolution the modal does, so the default scope is the same one a person sees.
        let path = self.files.active().path().map(Path::to_path_buf);
        let candidates = self.candidates_for(&from, path.as_deref(), offset);
        self.rename_kind = candidates.first().map(|candidate| candidate.kind);
        self.rename_here = path;
        self.rename_ticked_up_to = 0;
        self.tree.reload();
        let waker = self.thread_waker();
        self.references = Some(References::open(references::Purpose::Rename, &from, waker));
        if let Some(modal) = self.references.as_mut() {
            modal.new_name = to.trim().to_owned();
        }
        Outcome::Hold(Waiting::References {
            until: Instant::now() + self.cli_timeout(request),
            code_only: false,
            rename: Some(CliRename {
                to: to.trim().to_owned(),
                scope,
                include,
                apply: request.switch("apply"),
            }),
        })
    }

    /// How long a symbol command waits for its search.
    fn cli_timeout(&self, request: &Request) -> Duration {
        request
            .whole("timeout")
            .map(|milliseconds| Duration::from_millis(milliseconds as u64))
            .unwrap_or(DEFAULT_WAIT)
    }

    /// The word at an offset in the tab that is showing.
    fn symbol_at(&mut self, offset: usize) -> Option<String> {
        let index = self.files.active_index();
        let word = self.tab_symbols(index).read.identifier_at(offset)?;
        Some(self.files.at(index).document.text().byte_slice(word))
    }

    /// What `editor references` and `editor rename` answer once the search has finished.
    pub(crate) fn references_reply(&mut self, request: &Request, waiting: &Waiting) -> Reply {
        let Waiting::References { code_only, rename, .. } = waiting else {
            return Reply::failed(&request.command, code::NOT_APPLICABLE, "Nothing was waiting.");
        };
        let Some(modal) = self.references.as_ref() else {
            return Reply::failed(
                &request.command,
                code::NOT_APPLICABLE,
                "The modal was shut before the search finished.",
            );
        };
        let name = modal.name.clone();
        let capped = modal.is_capped();
        let rows: Vec<Value> = modal
            .hits()
            .iter()
            .filter(|hit| !*code_only || hit.role == Role::Code)
            .map(|hit| {
                json!({
                    "path": hit.path.to_string_lossy(),
                    "line": hit.line,
                    "column": hit.range.start + 1,
                    "offset": hit.offset.start,
                    "role": hit.role.name(),
                    "text": hit.text,
                })
            })
            .collect();
        let Some(rename) = rename else {
            let sentence = match rows.len() {
                0 => format!("Nothing in this project uses '{name}'"),
                1 => format!("'{name}' is used once"),
                many => format!("'{name}' is used in {many} places"),
            };
            self.references = None;
            return Reply::done(
                &request.command,
                sentence,
                json!({ "name": name, "references": rows, "capped": capped }),
            );
        };
        self.tick_for_the_command_line(rename);
        let Some(modal) = self.references.as_ref() else {
            return Reply::failed(&request.command, code::NOT_APPLICABLE, "The modal was shut.");
        };
        let change = crate::app::symbols::RenameChange {
            from: name.clone(),
            to: rename.to.clone(),
            by_file: modal.change(),
        };
        let listed: Vec<Value> = change
            .by_file
            .iter()
            .map(|(path, ranges)| json!({ "path": path.to_string_lossy(), "places": ranges.len() }))
            .collect();
        if !rename.apply {
            self.references = None;
            return Reply::done(
                &request.command,
                format!(
                    "{} places in {} files would be renamed from '{name}' to '{}'",
                    change.count(),
                    change.by_file.len(),
                    rename.to
                ),
                json!({ "from": name, "to": rename.to, "files": listed, "references": rows }),
            );
        }
        let report = self.apply_rename(&change);
        self.references = None;
        let sentence = report.sentence(&rename.to);
        self.message = Some(sentence.clone());
        Reply::done(
            &request.command,
            sentence,
            json!({
                "from": name,
                "to": rename.to,
                "changed": report.changed,
                "files": listed,
                // The two halves said apart, because they are undone in different ways: a file that
                // was **written** is on the disk now, and a tab that was **edited** still has
                // unsaved changes that closing it without saving would throw away. `task-1794`.
                "wrote": report.files.len(),
                "written": report.files.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>(),
                "openTabs": report.open.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>(),
                "skipped": report
                    .skipped
                    .iter()
                    .map(|(path, reason)| json!({ "path": path.to_string_lossy(), "reason": reason }))
                    .collect::<Vec<_>>(),
            }),
        )
    }

    /// Tick the rows a command line rename asks for: the same default rules, with the flags on top.
    fn tick_for_the_command_line(&mut self, rename: &CliRename) {
        let kind = self.rename_kind;
        let here = self.rename_here.clone();
        let Some(modal) = self.references.as_mut() else {
            return;
        };
        let ticks: Vec<bool> = modal
            .hits()
            .iter()
            .map(|hit| {
                let same_file = here.as_deref() == Some(hit.path.as_path());
                let by_role = match hit.role {
                    Role::Code => true,
                    Role::Comment => rename.include.iter().any(|part| part == "comments"),
                    Role::String => rename.include.iter().any(|part| part == "strings"),
                };
                if !by_role {
                    return false;
                }
                match rename.scope {
                    Some(true) => same_file,
                    Some(false) => true,
                    None => {
                        crate::app::symbols::ticked_by_default(hit.role, kind, same_file)
                            || (hit.role != Role::Code && by_role)
                    }
                }
            })
            .collect();
        modal.set_ticks(ticks);
    }

    fn cli_editor_preview(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        if !file_kind::preview_applies(self.document().path()) {
            return no(
                request,
                code::NOT_APPLICABLE,
                format!("{} has no preview.", self.files.active().name()),
            );
        }
        // A Mermaid file's preview is a picture, not text, so what is read back is the diagram: what
        // kind it is and how large it came out, or the reason it could not be drawn. Reading a
        // picture out as words is what a caller is really asking for here.
        if file_kind::is_mermaid(self.document().path()) {
            return self.cli_editor_diagram(request, ctx);
        }
        // The preview is normally built when it is about to be drawn. Asked for from the command
        // line it may never have been drawn, so it is built here at the width the editing area has,
        // or at a sensible width if the window has not laid one out yet.
        let width = if self.editor_area.width() > 1.0 {
            self.editor_area.width()
        } else {
            ctx.content_rect().width().max(400.0)
        };
        self.refresh_preview(ctx, width);
        // What the parser found is the source of each picture; what the window read is whether it
        // could be drawn and how large. They are matched up by the paragraph both of them name.
        let sources = self
            .files
            .active()
            .cached
            .preview
            .as_ref()
            .map(|preview| preview.images.clone())
            .unwrap_or_default();
        let pictures: Vec<Value> = self
            .preview_pictures()
            .iter()
            .map(|placed| {
                let source = sources
                    .iter()
                    .find(|image| image.paragraph == placed.paragraph)
                    .map(|image| image.source.clone());
                json!({
                    "paragraph": placed.paragraph,
                    "source": source,
                    "alt": placed.alt,
                    "width": placed.size.x,
                    "height": placed.size.y,
                    "drawn": placed.texture.is_some(),
                })
            })
            .collect();
        // The diagrams are reported the same way the pictures are: what the parser found beside what
        // the window made of it, matched up by the paragraph both of them name.
        let diagrams: Vec<Value> = self
            .preview_diagrams()
            .iter()
            .map(|placed| {
                json!({
                    "paragraph": placed.paragraph,
                    "diagram": unluminous_core::mermaid::kind(&placed.source).map(|kind| kind.name()),
                    "width": placed.size.x,
                    "height": placed.size.y,
                    "drawn": placed.laid.is_ok(),
                    "problem": placed.laid.as_ref().err().map(|problem| problem.message()),
                })
            })
            .collect();
        // Where the code blocks, the tables and the front matter are, and where the inline code is.
        // The window paints a panel behind the first and a chip behind the second; reported here so
        // that what a person can see is what a script can read.
        let (panels, code_spans) = match self.files.active().cached.preview.as_ref() {
            Some(preview) => (
                preview
                    .panels
                    .iter()
                    .map(|panel| {
                        json!({
                            "from": panel.paragraphs.start,
                            "to": panel.paragraphs.end,
                            "kind": match panel.kind {
                                unluminous_core::PanelKind::Code => "code",
                                unluminous_core::PanelKind::Table => "table",
                                unluminous_core::PanelKind::FrontMatter => "front-matter",
                            },
                        })
                    })
                    .collect::<Vec<Value>>(),
                preview
                    .code_spans
                    .iter()
                    .map(|span| json!({ "from": span.start, "to": span.end }))
                    .collect::<Vec<Value>>(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        // The links and where each one goes. Reported for the same reason the code chips are: what a
        // person can see is what a script can read — and this is the only way an agent can find out what
        // a document links to without reading the source and parsing it a second time.
        let links = match self.files.active().cached.preview.as_ref() {
            Some(preview) => preview
                .links
                .iter()
                .map(|link| {
                    json!({ "from": link.bytes.start, "to": link.bytes.end, "target": link.target })
                })
                .collect::<Vec<Value>>(),
            None => Vec::new(),
        };
        ok(
            request,
            String::new(),
            json!({
                "text": self.preview_text(),
                "pictures": pictures,
                "diagrams": diagrams,
                "panels": panels,
                "code": code_spans,
                "links": links,
            }),
        )
    }

    /// `unluminous-cli editor preview-select` — reading and copying what the preview has selected.
    ///
    /// The preview goes through the same three functions the pointer goes through, so a selection
    /// made from the command line and one made with the mouse are the same thing. The preview is
    /// built first when it has never been drawn, exactly as `editor preview` builds it, because an
    /// offset into a page that does not exist yet has no meaning.
    fn cli_editor_preview_select(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        if !file_kind::preview_applies(self.document().path())
            || file_kind::is_mermaid(self.document().path())
        {
            return no(
                request,
                code::NOT_APPLICABLE,
                format!("{} has no Markdown preview to select in.", self.files.active().name()),
            );
        }
        let width = if self.editor_area.width() > 1.0 {
            self.editor_area.width()
        } else {
            ctx.content_rect().width().max(400.0)
        };
        self.refresh_preview(ctx, width);
        let length = self
            .files
            .active()
            .cached
            .preview
            .as_ref()
            .map(|preview| preview.text.len_bytes())
            .unwrap_or(0);

        if request.switch("all") {
            self.select_the_whole_preview();
        } else if request.switch("none") {
            self.files.active_mut().preview_selection = unluminous_core::Selection::caret(0);
        } else if let Some(from) = request.number("from") {
            let from = (from as usize).min(length);
            let to = request.number("to").map(|to| (to as usize).min(length)).unwrap_or(length);
            self.files.active_mut().preview_selection = unluminous_core::Selection::new(from, to);
            self.reading_preview = true;
        }
        let selection = self.files.active().preview_selection;
        let text = self.preview_selected_text().unwrap_or_default();
        if request.switch("copy") {
            if text.is_empty() {
                return no(request, code::NOT_APPLICABLE, "Nothing is selected in the preview.");
            }
            ctx.copy_text(text.clone());
        }
        let summary = if text.is_empty() {
            "Nothing is selected in the preview.".to_owned()
        } else {
            format!("{} bytes selected in the preview.", text.len())
        };
        ok(
            request,
            summary,
            json!({
                "from": selection.start(),
                "to": selection.end(),
                "length": length,
                "text": text,
                "copied": request.switch("copy"),
            }),
        )
    }
}
