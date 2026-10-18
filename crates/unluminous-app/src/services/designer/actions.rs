//! Every change a person or an agent can make to a design, as one function each.
//!
//! A button in the window and a verb of `unluminous-cli plugins run designer` both end here, so a thing done
//! by hand and the same thing done by an agent are the same thing. Each function that changes a design's
//! notes writes the notes file before it returns, so what is on the screen, what is in the file and what the
//! agent reads back cannot disagree.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{json, Value};

use super::notes::{Anchor, Message, NoteState, NotesFile};
use super::page::{Element, Mode, Report};
use super::{
    history, library, modified, prompt, Composer, Configuration, DesignView, Designer, Device,
    Filter, PageCall, Want, LOOK_AGAIN, SEND_ID,
};
use crate::components::toast::Kind;
use crate::services::plugin_ui::Request;

/// What the notes thread says when the note has been handed to the agent.
const ASKED: &str = "Sent to the agent";

/// How far an element has to move before its note remembers the new place, in page pixels.
const MOVED_ENOUGH: f32 = 2.0;

impl Designer {
    // ------------------------------------------------------------------------------ the page's state

    /// Choose what a click in the page does.
    ///
    /// **Going back to Browse puts the chosen element away**, and the note being written on it with it:
    /// Browse is for using the page, and an outline left on an element from Comment read as something still
    /// chosen. `task-2294`: *"When I go to comment tab, then back to Browse, no components should be
    /// selected."*
    ///
    /// @param design - the design, relative to the project
    /// @param mode - the mode
    pub fn set_mode(&mut self, design: &Path, mode: Mode) -> Result<(), String> {
        let view = self.loaded(design)?;
        view.mode = mode;
        view.calls.push(PageCall::SetMode(mode));
        if mode == Mode::Browse {
            view.selection = None;
            view.composing = None;
            view.calls.push(PageCall::Clear);
        }
        Ok(())
    }

    /// Choose the width the page is laid out at, and remember it for this project.
    ///
    /// @param design - the design, relative to the project
    /// @param device - the device
    pub fn set_device(&mut self, design: &Path, device: Device) -> Result<(), String> {
        let view = self.loaded(design)?;
        view.device = device;
        view.pins_dirty = true;
        self.remember_device(design, device);
        Ok(())
    }

    /// Choose which notes the sidebar lists.
    ///
    /// @param design - the design, relative to the project
    /// @param filter - open or resolved
    pub fn set_filter(&mut self, design: &Path, filter: Filter) -> Result<(), String> {
        self.loaded(design)?.filter = filter;
        Ok(())
    }

    /// Ask the page to choose an element by selector, as a click would.
    ///
    /// @param design - the design, relative to the project
    /// @param selector - what finds the element
    pub fn select_element(&mut self, design: &Path, selector: &str) -> Result<(), String> {
        let selector = selector.trim();
        if selector.is_empty() {
            return Err("Say which element, as a CSS selector.".to_owned());
        }
        self.loaded(design)?.calls.push(PageCall::Select(selector.to_owned()));
        Ok(())
    }

    /// Move the choice to the element's parent or its first child.
    ///
    /// @param design - the design, relative to the project
    /// @param up - true for the parent
    pub fn walk_the_selection(&mut self, design: &Path, up: bool) -> Result<(), String> {
        let call = if up { PageCall::Parent } else { PageCall::Child };
        self.loaded(design)?.calls.push(call);
        Ok(())
    }

    /// Put the choice away.
    ///
    /// @param design - the design, relative to the project
    pub fn clear_the_selection(&mut self, design: &Path) -> Result<(), String> {
        let view = self.loaded(design)?;
        view.selection = None;
        view.composing = None;
        view.calls.push(PageCall::Clear);
        Ok(())
    }

    /// Open a note in the sidebar and take the page to its element.
    ///
    /// @param design - the design, relative to the project
    /// @param id - the note
    pub fn open_note(&mut self, design: &Path, id: u32) -> Result<(), String> {
        let view = self.loaded(design)?;
        let note = view.notes.get(id).ok_or_else(|| super::notes::no_such_note(id))?;
        view.filter = match note.state {
            NoteState::Open => Filter::Open,
            NoteState::Resolved => Filter::Resolved,
        };
        view.active = Some(id);
        view.reply.clear();
        view.calls.push(PageCall::Focus(id));
        view.pins_dirty = true;
        Ok(())
    }

    /// Take what the page reported and act on it.
    ///
    /// @param design - the design, relative to the project
    /// @param report - what the script said
    pub fn page_reported(&mut self, design: &Path, report: Report) {
        let result = match report {
            Report::Selected(element) => self.the_page_selected(design, element),
            Report::Comment { element, fx, fy } => {
                self.loaded(design).map(|view| begin_a_note(view, element, fx, fy))
            }
            Report::Pin(id) => self.open_note(design, id),
            Report::Cleared => self.loaded(design).map(|view| {
                view.selection = None;
                view.composing = None;
            }),
            Report::Anchors { found, missing } => {
                self.the_page_placed_the_pins(design, found, missing)
            }
            Report::Loaded { .. } => self.the_page_loaded(design),
        };
        if let Err(why) = result {
            self.problem = Some(why);
        }
    }

    /// The page chose an element, so it is the selection, and what is known about any note on it is brought up to date.
    fn the_page_selected(&mut self, design: &Path, element: Element) -> Result<(), String> {
        let view = self.loaded(design)?;
        for (id, known) in view.details.iter_mut() {
            let same =
                view.notes.get(*id).is_some_and(|note| note.anchor.selector == element.selector);
            if same {
                *known = element.clone();
            }
        }
        view.selection = Some(element);
        Ok(())
    }

    /// A new document has the script, so it is told the mode, the pins and the choice again.
    fn the_page_loaded(&mut self, design: &Path) -> Result<(), String> {
        let view = self.loaded(design)?;
        view.page_revision += 1;
        view.calls.push(PageCall::SetMode(view.mode));
        if let Some(chosen) = view.selection.as_ref() {
            view.calls.push(PageCall::Select(chosen.selector.clone()));
        }
        view.pins_dirty = true;
        Ok(())
    }

    /// Mark the notes whose element is gone, and remember where the others were last seen.
    fn the_page_placed_the_pins(
        &mut self,
        design: &Path,
        found: Vec<super::page::Found>,
        missing: Vec<u32>,
    ) -> Result<(), String> {
        let view = self.loaded(design)?;
        let before = view.detached.clone();
        view.detached = missing.into_iter().filter(|id| view.notes.get(*id).is_some()).collect();
        let device = view.device.name().to_owned();
        let mut moved = false;
        for place in found {
            if let Some(element) = place.element {
                view.details.insert(place.id, element);
            }
            let Some(note) = view.notes.get_mut(place.id) else { continue };
            let far = (note.anchor.page_x - place.x).abs() > MOVED_ENOUGH
                || (note.anchor.page_y - place.y).abs() > MOVED_ENOUGH;
            if far || note.anchor.device != device {
                note.anchor.page_x = place.x;
                note.anchor.page_y = place.y;
                note.anchor.device = device.clone();
                moved = true;
            }
        }
        let changed = view.detached != before;
        if moved {
            self.save_notes(design)?;
        } else if changed {
            self.loaded(design)?.pins_dirty = true;
        }
        Ok(())
    }

    // --------------------------------------------------------------------------------------- notes

    /// Write the notes file, and count the design's notes again.
    ///
    /// @param design - the design, relative to the project
    pub fn save_notes(&mut self, design: &Path) -> Result<(), String> {
        let project = self.project.clone().ok_or_else(|| "No project is open.".to_owned())?;
        let comments = project.join(library::comments_path(design));
        let view =
            self.views.get_mut(design).ok_or_else(|| "That design is not open.".to_owned())?;
        view.pins_dirty = true;
        view.page_revision += 1;
        let written = view.notes.write(&comments);
        view.notes_stamp = modified(&comments);
        let (open, resolved) = view.notes.counts();
        if let Some(summary) = self.designs.iter_mut().find(|summary| summary.path == design) {
            summary.open = open;
            summary.resolved = resolved;
        }
        written.map(|_| ())
    }

    /// Make a note on an element found by selector.
    ///
    /// @param design - the design, relative to the project
    /// @param selector - what finds the element
    /// @param text - the note
    /// @param author - who is writing it
    pub fn comment(
        &mut self,
        design: &Path,
        selector: &str,
        text: &str,
        author: &str,
    ) -> Result<u32, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Say what the note is.".to_owned());
        }
        let selector = selector.trim();
        if selector.is_empty() {
            return Err("Say which element, as a CSS selector.".to_owned());
        }
        let view = self.loaded(design)?;
        let known = known_element(view, selector);
        let mut anchor = anchor_for(known.as_ref(), selector, 1.0, 0.0);
        anchor.device = view.device.name().to_owned();
        let id = view.notes.add(anchor, author, text);
        if let Some(element) = known {
            view.details.insert(id, element);
        }
        self.save_notes(design)?;
        Ok(id)
    }

    /// Start a note on the chosen element, pinned to its top right.
    ///
    /// @param design - the design, relative to the project
    pub fn start_a_note_on_the_selection(&mut self, design: &Path) -> Result<(), String> {
        let view = self.loaded(design)?;
        let element = view.selection.clone().ok_or("Choose an element first.")?;
        begin_a_note(view, element, 1.0, 0.0);
        Ok(())
    }

    /// Put away the note being written.
    ///
    /// @param design - the design, relative to the project
    pub fn cancel_the_note(&mut self, design: &Path) -> Result<(), String> {
        self.loaded(design)?.composing = None;
        Ok(())
    }

    /// Send what is in the reply box as a reply from the person, and empty the box.
    ///
    /// @param design - the design, relative to the project
    /// @param id - the note
    pub fn reply_from_the_box(&mut self, design: &Path, id: u32) -> Result<(), String> {
        let text = self.loaded(design)?.reply.clone();
        self.reply(design, id, &text, "you")?;
        self.loaded(design)?.reply.clear();
        Ok(())
    }

    /// Pin a note to the chosen element.
    ///
    /// @param design - the design, relative to the project
    /// @param id - the note
    pub fn move_note_to_the_selection(&mut self, design: &Path, id: u32) -> Result<(), String> {
        let selector = self
            .loaded(design)?
            .selection
            .as_ref()
            .map(|element| element.selector.clone())
            .ok_or("Choose an element first.")?;
        self.move_note(design, id, &selector)
    }

    /// Turn the note being written into a note, and ask the agent about it when `ask` is true.
    ///
    /// @param design - the design, relative to the project
    /// @param ask - whether to send it to the agent at once
    pub fn add_the_note(&mut self, design: &Path, ask: bool) -> Result<u32, String> {
        let view = self.loaded(design)?;
        let Some(composer) = view.composing.take() else {
            return Err("There is no note being written.".to_owned());
        };
        let text = composer.text.trim().to_owned();
        if text.is_empty() {
            view.composing = Some(composer);
            return Err("Say what the note is.".to_owned());
        }
        let mut anchor = anchor_for(
            Some(&composer.element),
            &composer.element.selector,
            composer.fx,
            composer.fy,
        );
        anchor.device = view.device.name().to_owned();
        let id = view.notes.add(anchor, "you", &text);
        view.details.insert(id, composer.element);
        view.active = Some(id);
        view.filter = Filter::Open;
        self.save_notes(design)?;
        if ask {
            self.ask_note(design, id)?;
        }
        Ok(id)
    }

    /// Add to a note's thread.
    ///
    /// @param design - the design, relative to the project
    /// @param id - the note
    /// @param text - what to say
    /// @param author - who is saying it
    pub fn reply(
        &mut self,
        design: &Path,
        id: u32,
        text: &str,
        author: &str,
    ) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Say what the reply is.".to_owned());
        }
        self.loaded(design)?.notes.reply(id, author, text)?;
        self.save_notes(design)
    }

    /// Mark a note dealt with, or open it again.
    ///
    /// @param design - the design, relative to the project
    /// @param id - the note
    /// @param state - what it becomes
    pub fn set_state(&mut self, design: &Path, id: u32, state: NoteState) -> Result<(), String> {
        let view = self.loaded(design)?;
        view.notes.set_state(id, state)?;
        if state == NoteState::Resolved && view.active == Some(id) {
            view.active = None;
        }
        self.save_notes(design)
    }

    /// Take a note away.
    ///
    /// @param design - the design, relative to the project
    /// @param id - the note
    pub fn delete_note(&mut self, design: &Path, id: u32) -> Result<(), String> {
        let view = self.loaded(design)?;
        view.notes.remove(id)?;
        view.details.remove(&id);
        view.detached.remove(&id);
        if view.active == Some(id) {
            view.active = None;
        }
        self.save_notes(design)
    }

    /// Pin a note to another element, which is what a detached note needs.
    ///
    /// @param design - the design, relative to the project
    /// @param id - the note
    /// @param selector - the element it moves to
    pub fn move_note(&mut self, design: &Path, id: u32, selector: &str) -> Result<(), String> {
        let selector = selector.trim();
        if selector.is_empty() {
            return Err("Say which element, as a CSS selector.".to_owned());
        }
        let view = self.loaded(design)?;
        let known = known_element(view, selector);
        let device = view.device.name().to_owned();
        let note = view.notes.get_mut(id).ok_or_else(|| super::notes::no_such_note(id))?;
        let mut anchor = anchor_for(known.as_ref(), selector, note.anchor.fx, note.anchor.fy);
        anchor.device = device;
        anchor.extra = note.anchor.extra.clone();
        note.anchor = anchor;
        view.detached.remove(&id);
        match known {
            Some(element) => view.details.insert(id, element),
            None => view.details.remove(&id),
        };
        self.save_notes(design)
    }

    // ------------------------------------------------------------------------------- the agent

    /// Hand one note to the agent.
    ///
    /// @param design - the design, relative to the project
    /// @param id - the note
    pub fn ask_note(&mut self, design: &Path, id: u32) -> Result<(), String> {
        let send_html = self.config.send_html;
        let said = history::said(design);
        let view = self.loaded(design)?;
        let note = view.notes.get(id).ok_or_else(|| super::notes::no_such_note(id))?;
        let element = known_element(view, &note.anchor.selector);
        let words = prompt::for_a_note(&said, note, element.as_ref(), send_html);
        let about = note.first_words().to_owned();
        self.send_to_the_agent(design, words, &about)?;
        let view = self.loaded(design)?;
        if let Some(note) = view.notes.get_mut(id) {
            note.messages.push(Message::now("designer", ASKED));
        }
        self.save_notes(design)
    }

    /// Hand every open note to the agent in one message.
    ///
    /// @param design - the design, relative to the project
    pub fn ask_every_open_note(&mut self, design: &Path) -> Result<usize, String> {
        let send_html = self.config.send_html;
        let said = history::said(design);
        let view = self.loaded(design)?;
        let open: Vec<&super::notes::Note> =
            view.notes.notes.iter().filter(|note| note.state == NoteState::Open).collect();
        if open.is_empty() {
            return Err("There are no open notes to send.".to_owned());
        }
        let elements: Vec<Option<Element>> =
            open.iter().map(|note| known_element(view, &note.anchor.selector)).collect();
        let pairs: Vec<(&super::notes::Note, Option<&Element>)> =
            open.iter().copied().zip(elements.iter().map(Option::as_ref)).collect();
        let words = prompt::for_every_open_note(&said, &pairs, send_html);
        let ids: Vec<u32> = open.iter().map(|note| note.id).collect();
        let about = format!("{} open notes", ids.len());
        self.send_to_the_agent(design, words, &about)?;
        let view = self.loaded(design)?;
        for id in &ids {
            if let Some(note) = view.notes.get_mut(*id) {
                note.messages.push(Message::now("designer", ASKED));
            }
        }
        self.save_notes(design)?;
        Ok(ids.len())
    }

    /// Hand the agent a request that is not a note: about the chosen element, or the whole design.
    ///
    /// @param design - the design, relative to the project
    /// @param words - what is asked
    pub fn ask_about_the_design(&mut self, design: &Path, words: &str) -> Result<(), String> {
        let words = words.trim();
        if words.is_empty() {
            return Err("Say what to change.".to_owned());
        }
        let send_html = self.config.send_html;
        let said = history::said(design);
        let view = self.loaded(design)?;
        let request = prompt::free(&said, view.selection.as_ref(), words, send_html);
        view.prompt.clear();
        self.send_to_the_agent(design, request, words)
    }

    /// Copy the design, then queue the two commands that put the words in the Agent-Chat pane.
    fn send_to_the_agent(
        &mut self,
        design: &Path,
        words: String,
        about: &str,
    ) -> Result<(), String> {
        let project = self.project.clone().ok_or_else(|| "No project is open.".to_owned())?;
        let folder = self.config.folder.clone();
        let kept = self.config.history_kept;
        history::keep(&project, &folder, design, about, kept)?;
        let said = history::said(design);
        self.outbox.push(run_agent_chat(format!("designer-open:{said}"), "open-pane", None));
        self.outbox.push(run_agent_chat(format!("{SEND_ID}{said}"), "send", Some(words)));
        self.loaded(design)?.status = "Sending to Agent-Chat".to_owned();
        Ok(())
    }

    /// What Agent-Chat said to a command this provider asked for.
    pub(super) fn the_agent_answered(&mut self, id: &str, answer: Result<Value, String>) {
        let Some(design) = id.strip_prefix(SEND_ID) else { return };
        let design = PathBuf::from(design);
        let said = match &answer {
            Ok(value) if value["queued"] == json!(true) => {
                "Queued behind the answer arriving".to_owned()
            }
            Ok(_) => "Sent to Agent-Chat".to_owned(),
            Err(why) => why.clone(),
        };
        if let Some(view) = self.views.get_mut(&design) {
            view.status = said.clone();
        }
        if answer.is_err() {
            self.outbox.push(Request::Notice { text: said, kind: Kind::Problem });
        }
    }

    // ------------------------------------------------------------------------------------ history

    /// The copies of a design, newest first.
    ///
    /// @param design - the design, relative to the project
    pub fn copies(&self, design: &Path) -> Vec<history::Backup> {
        match self.project.as_deref() {
            Some(project) => history::list(project, &self.config.folder, design),
            None => Vec::new(),
        }
    }

    /// Put a copy back as the design.
    ///
    /// @param design - the design, relative to the project
    /// @param wanted - the copy, by name or by place in the list
    pub fn restore(&mut self, design: &Path, wanted: &str) -> Result<history::Backup, String> {
        let project = self.project.clone().ok_or_else(|| "No project is open.".to_owned())?;
        let chosen = history::restore(
            &project,
            &self.config.folder,
            design,
            wanted,
            self.config.history_kept,
        )?;
        self.wants.push(Want::Reload(design.to_path_buf()));
        self.loaded(design)?.status = format!("Restored {}", chosen.when());
        Ok(chosen)
    }

    /// Ask the window to open the design's file as text.
    ///
    /// @param design - the design, relative to the project
    pub fn want_the_source(&mut self, design: &Path) {
        self.wants.push(Want::OpenSource(design.to_path_buf()));
    }

    /// Ask the window to load the page again.
    ///
    /// @param design - the design, relative to the project
    pub fn want_a_reload(&mut self, design: &Path) {
        self.wants.push(Want::Reload(design.to_path_buf()));
    }

    // ------------------------------------------------------------------------------------ designs

    /// Make a design from the starter page and open it in the Designer pane.
    ///
    /// @param name - what it is called
    pub fn make_a_design(&mut self, name: &str) -> Result<PathBuf, String> {
        let project = self.project.clone().ok_or_else(|| "No project is open.".to_owned())?;
        let made = library::make(&project, &self.config.folder, name, self.config.starter)?;
        self.refresh_designs();
        self.open_a_design(&made)?;
        Ok(made)
    }

    /// Open a design in a tab of the Designer pane, and ask the window to put the pane on the screen.
    ///
    /// `task-2260`: a design opens in the pane's own tabs rather than in the editing area beside every other
    /// open file. See [`super::tabs`].
    ///
    /// @param design - the design, relative to the project
    pub fn open_a_design(&mut self, design: &Path) -> Result<(), String> {
        let disk = self.absolute(design).ok_or_else(|| "No project is open.".to_owned())?;
        if !disk.is_file() {
            return Err(format!("{} is not there", self.name_of(design)));
        }
        self.loaded(design)?;
        self.open_in_a_tab(design);
        self.outbox.push(Request::ShowPane(true));
        Ok(())
    }

    // ------------------------------------------------------------------------------------ on a clock

    /// Look at the disk again when it is time: the settings, the notes files and the list of designs.
    pub(super) fn look_again_when_due(&mut self) {
        if self.looked_at.elapsed() < LOOK_AGAIN {
            return;
        }
        self.looked_at = Instant::now();
        self.reread_the_settings();
        self.reread_the_notes();
        self.refresh_designs();
    }

    /// Look at the disk now rather than at the next two second mark, for a test that cannot wait for one.
    #[cfg(test)]
    pub(super) fn force_a_look_for_tests(&mut self) {
        self.looked_at = Instant::now().checked_sub(LOOK_AGAIN * 2).unwrap_or_else(Instant::now);
        self.look_again_when_due();
    }

    /// Read the settings again when the file changed, which is what `settings set plugins.designer.*` does.
    fn reread_the_settings(&mut self) {
        let Some(folder) = self.folder.clone() else { return };
        let stamp = modified(&folder.join(Configuration::FILE));
        if stamp == self.config_stamp {
            return;
        }
        self.config_stamp = stamp;
        let (config, refused) = Configuration::read(&folder);
        self.problem = (!refused.is_empty()).then(|| refused.join(" "));
        self.config = config;
        self.config_changed = true;
    }

    /// Read a design's notes again when something else wrote the file, such as an agent with its own tools.
    fn reread_the_notes(&mut self) {
        let Some(project) = self.project.clone() else { return };
        let designs: Vec<PathBuf> = self.views.keys().cloned().collect();
        for design in designs {
            let comments = project.join(library::comments_path(&design));
            let stamp = modified(&comments);
            let name = self.name_of(&design);
            let Some(view) = self.views.get_mut(&design) else { continue };
            if stamp == view.notes_stamp {
                continue;
            }
            view.notes_stamp = stamp;
            if let Ok(fresh) = NotesFile::read(&comments, &name) {
                if fresh != view.notes {
                    view.notes = fresh;
                    view.pins_dirty = true;
                    view.page_revision += 1;
                }
            }
        }
    }
}

/// The request that runs one of Agent-Chat's own commands.
fn run_agent_chat(id: String, command: &str, words: Option<String>) -> Request {
    let mut arguments = serde_json::Map::new();
    arguments.insert("id".to_owned(), json!("agent-chat"));
    arguments.insert("command".to_owned(), json!(command));
    if let Some(words) = words {
        arguments.insert("arguments".to_owned(), json!(words));
    }
    Request::RunCommand { id, command: "plugins.run".to_owned(), arguments }
}

/// Start writing a note on an element.
fn begin_a_note(view: &mut DesignView, element: Element, fx: f32, fy: f32) {
    view.selection = Some(element.clone());
    view.composing = Some(Composer { element, fx, fy, text: String::new() });
}

/// What the page has said about the element a selector finds, when it has said.
fn known_element(view: &DesignView, selector: &str) -> Option<Element> {
    view.selection
        .iter()
        .chain(view.details.values())
        .find(|element| element.selector == selector)
        .cloned()
}

/// A note's anchor on an element, or on a selector alone when nothing is known about it.
fn anchor_for(element: Option<&Element>, selector: &str, fx: f32, fy: f32) -> Anchor {
    let Some(element) = element else {
        return Anchor { fx, fy, ..Anchor::on(selector) };
    };
    Anchor {
        selector: element.selector.clone(),
        tag: element.tag.clone(),
        excerpt: element.excerpt.clone(),
        fx,
        fy,
        page_x: element.rect.x + fx * element.rect.w,
        page_y: element.rect.y + fy * element.rect.h,
        device: String::new(),
        extra: serde_json::Map::new(),
    }
}

// ------------------------------------------------------------------------------------ the view as data

/// The whole provider as data, which is what `plugins view designer` answers and a test asserts on.
pub(super) fn view_value(designer: &Designer) -> Value {
    let designs: Vec<Value> = designer
        .designs
        .iter()
        .map(|design| {
            json!({
                "name": design.name,
                "path": config_path(&design.path),
                "open": design.open,
                "resolved": design.resolved,
            })
        })
        .collect();
    let open: Vec<Value> =
        designer.views.iter().map(|(path, view)| one_design(designer, path, view)).collect();
    json!({
        "project": designer.project.as_ref().map(|path| path.display().to_string()),
        "folder": config_path(&designer.config.folder),
        "device": designer.config.device.name(),
        "starter": designer.config.starter.name(),
        "sendHtml": designer.config.send_html,
        "historyKept": designer.config.history_kept,
        "problem": designer.problem,
        "designs": designs,
        "showing": designer.showing.as_ref().map(|path| designer.name_of(path)),
        "tabs": designer
            .tabs
            .iter()
            .map(|tab| tab.as_ref().map(|path| designer.name_of(path)))
            .collect::<Vec<Option<String>>>(),
        "activeTab": designer.active_tab(),
        "open": open,
    })
}

fn config_path(path: &Path) -> String {
    super::config::slashed(path)
}

/// One open design.
fn one_design(designer: &Designer, path: &Path, view: &DesignView) -> Value {
    json!({
        "name": designer.name_of(path),
        "path": config_path(path),
        "device": view.device.name(),
        "mode": view.mode.name(),
        "filter": view.filter.name(),
        "selection": view.selection.as_ref().map(element_value),
        "active": view.active,
        "composing": view.composing.as_ref().map(|composer| composer.element.selector.clone()),
        "detached": view.detached.iter().collect::<Vec<&u32>>(),
        "status": view.status,
        "notes": view.notes.notes.iter().map(|note| note_value(note, view)).collect::<Vec<Value>>(),
    })
}

/// An element as data.
pub(super) fn element_value(element: &Element) -> Value {
    json!({
        "selector": element.selector,
        "tag": element.tag,
        "excerpt": element.excerpt,
        "html": element.html,
        "styles": element.styles.iter().map(|(name, value)| json!({ "name": name, "value": value })).collect::<Vec<Value>>(),
        "rect": { "x": element.rect.x, "y": element.rect.y, "w": element.rect.w, "h": element.rect.h },
    })
}

/// A note as data.
pub(super) fn note_value(note: &super::notes::Note, view: &DesignView) -> Value {
    json!({
        "id": note.id,
        "state": note.state.name(),
        "selector": note.anchor.selector,
        "tag": note.anchor.tag,
        "excerpt": note.anchor.excerpt,
        "detached": view.detached.contains(&note.id),
        "created": note.created,
        "resolved": note.resolved,
        "messages": note.messages.iter().map(|message| json!({
            "author": message.author,
            "body": message.body,
            "at": message.at,
        })).collect::<Vec<Value>>(),
    })
}
