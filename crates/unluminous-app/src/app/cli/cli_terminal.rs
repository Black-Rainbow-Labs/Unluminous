//! `terminal` -- the tabs along the bottom that run a real shell, or an agent, over a
//! pseudoterminal.

use super::*;

impl UnluminousApp {
    pub(crate) fn cli_terminal(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            // Through `show_the_terminal_tile`, which is what the menu entry goes through, so
            // showing the terminal puts the run tile away here as well: the bottom of the window
            // holds one tile, and two grids drawn into one rectangle is what leaving this to each
            // caller produced.
            "show" => {
                self.show_the_terminal_tile(true);
                ok(request, "The terminal is showing.", self.terminal_value())
            }
            "hide" => {
                self.show_the_terminal_tile(false);
                ok(request, "The terminal is hidden.", self.terminal_value())
            }
            "toggle" => {
                let visible = !self.terminal.visible;
                let verb = if visible { "show" } else { "hide" };
                self.cli_terminal(request, verb)
            }
            "new" => {
                self.show_the_terminal_tile(true);
                self.new_terminal_tab();
                ok(
                    request,
                    format!("Started terminal tab {}", self.terminal.tabs.active_index()),
                    self.terminal_value(),
                )
            }
            "list" => {
                let names = self.terminal.tabs.names();
                // **And where each one is**, which until `task-1950` nothing could be asked at all: a tab
                // reopens in the folder its shell was in, and whether that folder is the right one was only
                // answerable by closing the window and reading `.unluminous/terminal-tabs.txt`. It is the one
                // thing this feature is about, so it is the one thing that has to be readable. `None` is
                // written as an empty string, which is what a platform that will not say answers with.
                let folders = self.terminal_folders();
                let rows: Vec<String> = names
                    .iter()
                    .enumerate()
                    .map(|(at, name)| {
                        format!(
                            "{}{at:<3} {name:<24} {}",
                            if at == self.terminal.tabs.active_index() { "*" } else { " " },
                            folders.get(at).map(String::as_str).unwrap_or_default()
                        )
                    })
                    .collect();
                let mut value = self.terminal_value();
                value["folders"] = json!(folders);
                lines(request, format!("{} terminal tabs", names.len()), rows, value)
            }
            "select" => self.cli_terminal_select(request),
            "close" => self.cli_terminal_close(request),
            "rename" => self.cli_terminal_rename(request),
            "move" => self.cli_terminal_move(request),
            "send" => self.cli_terminal_send(request),
            "read" => self.cli_terminal_read(request),
            "height" => self.cli_terminal_height(request),
            _ => unknown(request),
        }
    }

    fn cli_terminal_select(&mut self, request: &Request) -> Outcome {
        if self.terminal.tabs.is_empty() {
            return no(request, code::NOT_APPLICABLE, "There is no terminal tab to show.");
        }
        let index = self.cli_terminal_tab(request).expect("a tab, checked above");
        if index >= self.terminal.tabs.count() {
            return no(
                request,
                code::NOT_FOUND,
                format!(
                    "There is no terminal tab {index}; there are {}.",
                    self.terminal.tabs.count()
                ),
            );
        }
        self.terminal.tabs.show(index);
        ok(request, format!("Showing terminal tab {index}"), self.terminal_value())
    }

    fn cli_terminal_close(&mut self, request: &Request) -> Outcome {
        if self.terminal.tabs.is_empty() {
            return no(request, code::NOT_APPLICABLE, "There is no terminal tab to close.");
        }
        let index = self.cli_terminal_tab(request).expect("a tab, checked above");
        if index >= self.terminal.tabs.count() {
            return no(request, code::NOT_FOUND, format!("There is no terminal tab {index}."));
        }
        self.terminal.tabs.close(index);
        if self.terminal.tabs.is_empty() {
            self.terminal.visible = false;
            self.focus = crate::app::Focus::Editor;
        }
        ok(request, format!("Closed terminal tab {index}"), self.terminal_value())
    }

    /// `unluminous-cli terminal rename [--tab <index>] <name>` — what the tab's own `Rename...` does.
    fn cli_terminal_rename(&mut self, request: &Request) -> Outcome {
        let Some(index) = self.cli_terminal_tab(request) else {
            return no(request, code::NOT_APPLICABLE, "There is no terminal tab to rename.");
        };
        if index >= self.terminal.tabs.count() {
            return no(request, code::NOT_FOUND, format!("There is no terminal tab {index}."));
        }
        // An empty name is not a mistake: it is how a tab is put back to being named after the
        // program in it, which the dialog cannot ask for because its button needs a name in the
        // field. `request.text` gives nothing at all for an empty argument, so both spellings —
        // left out, and given as nothing — mean the same thing here.
        let name = request.text("name").unwrap_or_default();
        self.terminal.tabs.rename(index, &name);
        let now = self.terminal.tabs.names().get(index).cloned().unwrap_or_default();
        ok(request, format!("Terminal tab {index} is called {now}"), self.terminal_value())
    }

    /// `unluminous-cli terminal move [--tab <index>] <position>` — what dragging a terminal tab does.
    ///
    /// It goes through `unluminous_terminal::Tabs::move_tab`, which is the same call the drag makes, so a
    /// rearrangement made from a script and one made with the pointer are the same rearrangement —
    /// including what `position` counts, which is the tabs as they are on the screen.
    fn cli_terminal_move(&mut self, request: &Request) -> Outcome {
        let Some(position) = request.whole("position") else {
            return no(request, code::USAGE, "Say where it goes, counting from 0.");
        };
        let Some(index) = self.cli_terminal_tab(request) else {
            return no(request, code::NOT_APPLICABLE, "There is no terminal tab to move.");
        };
        if index >= self.terminal.tabs.count() {
            return no(request, code::NOT_FOUND, format!("There is no terminal tab {index}."));
        }
        self.terminal.tabs.move_tab(index, position);
        ok(
            request,
            format!("Terminal tab {index} is now tab {}", self.terminal.tabs.active_index()),
            self.terminal_value(),
        )
    }

    /// Which terminal tab a command is about: `--tab` when it is given, the positional `index` when
    /// the command has one and it was given, the one showing otherwise.
    ///
    /// The flag is the settled convention for naming a tab and the positional is the thing the old
    /// callers have, so the flag wins when both are given. `None` when there is no terminal tab at
    /// all, which is a different thing to be told than a number that is out of range and is why this
    /// does not answer with the active index blindly.
    fn cli_terminal_tab(&self, request: &Request) -> Option<usize> {
        if self.terminal.tabs.is_empty() {
            return None;
        }
        Some(
            request
                .whole("tab")
                .or_else(|| request.whole("index"))
                .unwrap_or_else(|| self.terminal.tabs.active_index()),
        )
    }

    fn cli_terminal_send(&mut self, request: &Request) -> Outcome {
        let Some(index) = self.cli_terminal_tab(request) else {
            return no(
                request,
                code::NOT_APPLICABLE,
                "There is no terminal running. `terminal show` starts one.",
            );
        };
        if index >= self.terminal.tabs.count() {
            return no(request, code::NOT_FOUND, format!("There is no terminal tab {index}."));
        }
        // The mode comes from the tab the keys are going to, because application cursor and
        // bracketed paste are per-program and the two tabs may be running two different ones.
        let mode = self.terminal.tabs.at(index).map(|session| session.mode()).unwrap_or_default();
        let mut bytes: Vec<u8> = Vec::new();
        let mut said = String::new();
        if let Some(name) = request.text("key") {
            let Some(press) = key_named(&name) else {
                return no(
                    request,
                    code::USAGE,
                    format!("{name} is not a key this understands. `unluminous-cli commands \"terminal send\"` lists them."),
                );
            };
            match unluminous_terminal::keys::encode(press, mode) {
                Some(encoded) => bytes.extend(encoded),
                None => {
                    return no(request, code::USAGE, format!("{name} sends nothing to a shell."))
                }
            }
            said = format!("Sent {name}");
        }
        if let Some(text) = request.text("text") {
            let text = unescape(&text);
            bytes.extend(text.as_bytes());
            if !request.switch("no-enter") {
                bytes.push(b'\r');
            }
            said = format!(
                "Sent `{text}`{}",
                if request.switch("no-enter") { " without pressing Enter" } else { "" }
            );
        }
        if bytes.is_empty() {
            return no(request, code::USAGE, "Say what to send: some text, or --key.");
        }
        // Naming a tab does not show it: the bytes go to the named tab and the tab that is showing
        // is left alone, which is the whole point of targeting.
        self.terminal.tabs.at(index).expect("a tab, checked above").send(bytes.clone());
        self.show_the_terminal_tile(true);
        ok(request, said, json!({ "bytes": bytes.len(), "tab": index }))
    }

    fn cli_terminal_read(&mut self, request: &Request) -> Outcome {
        let Some(tab) = self.cli_terminal_tab(request) else {
            return no(
                request,
                code::NOT_APPLICABLE,
                "There is no terminal running. `terminal show` starts one.",
            );
        };
        if tab >= self.terminal.tabs.count() {
            return no(request, code::NOT_FOUND, format!("There is no terminal tab {tab}."));
        }
        // Take in whatever the shell has written since the last frame, so a read straight after a
        // send is not looking at the screen as it was before the command ran.
        self.terminal.tabs.pump();
        let count = request.whole("lines");
        let Some(text) = self.terminal_text(tab, count) else {
            // The tab may have closed while the request was on the queue, in which case there is
            // no screen to read and the honest answer is the one `close` gives for the same state.
            return no(request, code::NOT_FOUND, format!("There is no terminal tab {tab}."));
        };
        match request.text("wait-for") {
            Some(needle) if !text.contains(&needle) => Outcome::Hold(Waiting::TerminalText {
                tab,
                needle,
                lines: count,
                until: waits_for(request, "timeout", DEFAULT_WAIT),
            }),
            Some(needle) => ok(
                request,
                String::new(),
                json!({ "text": text, "tab": tab, "waitedFor": needle, "found": true }),
            ),
            None => ok(request, String::new(), json!({ "text": text, "tab": tab })),
        }
    }

    fn cli_terminal_height(&mut self, request: &Request) -> Outcome {
        if let Some(points) = request.number("points") {
            self.panes.terminal_height = (points as f32).max(settings::TERMINAL_MIN);
            self.unsaved_settings = true;
        }
        ok(
            request,
            format!("The terminal is {} points tall", self.panes.terminal_height),
            json!({ "height": self.panes.terminal_height }),
        )
    }

    /// What the tab at `tab` has on its screen, with the blank lines under it trimmed away and at
    /// most `count` lines kept. `None` when the tab is not there any more, which is what a tab that
    /// closed while a read was on the queue answers with.
    pub(crate) fn terminal_text(&self, tab: usize, count: Option<usize>) -> Option<String> {
        let session = self.terminal.tabs.at(tab)?;
        let whole = session.snapshot().text();
        let mut rows: Vec<&str> = whole.lines().collect();
        while rows.last().is_some_and(|row| row.trim().is_empty()) {
            rows.pop();
        }
        if let Some(count) = count {
            let from = rows.len().saturating_sub(count);
            rows = rows[from..].to_vec();
        }
        Some(rows.join("\n"))
    }

    /// Where each terminal tab's shell is, in the order the tabs are in.
    ///
    /// **The answer a tab is reopened with**, so asking this is asking what the window would write down if it
    /// closed now. What the shell reported beats what its process says, which is the whole of `task-1950`:
    /// see `unluminous_terminal::Session::folder`. A tab whose platform will not say answers with an empty
    /// string rather than being left out, so the list is one entry a tab and can be indexed by tab number.
    pub(crate) fn terminal_folders(&self) -> Vec<String> {
        self.terminal
            .tabs
            .sessions()
            .iter()
            .map(|session| {
                session.folder().map(|path| path.display().to_string()).unwrap_or_default()
            })
            .collect()
    }

    pub(crate) fn terminal_value(&self) -> Value {
        json!({
            "visible": self.terminal.visible,
            "height": self.panes.terminal_height,
            "tabs": self.terminal.tabs.names(),
            "activeTab": self.terminal.tabs.active_index(),
            "count": self.terminal.tabs.count(),
            "focused": self.focus == crate::app::Focus::Terminal,
        })
    }
}

/// The key `terminal send --key` names, as a key press.
///
/// A short list on purpose: the keys somebody driving a shell actually needs. Anything else is
/// text, which `terminal send` already sends.
fn key_named(name: &str) -> Option<unluminous_terminal::keys::KeyPress> {
    use unluminous_terminal::keys::{Key, KeyPress, Modifiers};
    let name = name.trim().to_lowercase();
    if let Some(letter) = name.strip_prefix("ctrl-") {
        let character = letter.chars().next()?;
        if letter.chars().count() != 1 {
            return None;
        }
        return Some(KeyPress::new(Key::Character(character), Modifiers::control()));
    }
    Some(KeyPress::plain(match name.as_str() {
        "enter" | "return" => Key::Enter,
        "tab" => Key::Tab,
        "escape" | "esc" => Key::Escape,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "page-up" => Key::PageUp,
        "page-down" => Key::PageDown,
        _ => return None,
    }))
}
