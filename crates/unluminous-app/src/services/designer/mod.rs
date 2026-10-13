//! The Designer: a design is an HTML file, its notes are pinned to elements of it, and the agent in the
//! Agent-Chat pane is the one that changes it.
//!
//! `tasks/task-2254-designer-plugin-prd.md` is what it does and `-tdd.md` is how it is built. This is the
//! provider, the code behind `ui.provider = designer`: what a design is, what is known about the open ones,
//! and every change a person or an agent can make to a design's notes. It has no window in it.
//!
//! ## What it cannot do, and who does
//!
//! A window has one native web view and only the window owns it, so the page is drawn by the window
//! (`app::designer`) and this provider is told what the page reported ([`Designer::page_reported`]) and asked
//! what to tell the page ([`Designer::take_calls`]). A press in the window and a command from
//! `unluminous-cli plugins run designer ...` end in the same function here, which is the rule the product is
//! built on.
//!
//! ## Sending to the agent
//!
//! The provider runs no agent. Asking is a history copy of the design, then two commands handed to the window
//! as [`Request::RunCommand`]: show the Agent-Chat pane, and `send` the words. They go down `run_cli`, the one
//! place a command becomes a change, so the request is exactly what an agent typing
//! `unluminous-cli plugins run agent-chat send ...` would cause.

pub mod config;
pub mod history;
pub mod library;
pub mod notes;
pub mod page;
pub mod prompt;

mod actions;
mod commands;
mod tabs;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

pub use config::{Configuration, Device, Starter};
use notes::NotesFile;
use page::{Element, Mode, PinSpec};

use crate::services::plugin_ui::{Answer, Context, Look, Request, UiProvider};
use crate::services::store::Values;

/// How often the list of designs, their counts and the settings file are looked at again.
const LOOK_AGAIN: std::time::Duration = std::time::Duration::from_secs(2);

/// The prefix of the ids this provider gives its commands, so an answer can be told from another plugin's.
pub const SEND_ID: &str = "designer-send:";

/// A design in the list the pane draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignSummary {
    /// The design, relative to the project.
    pub path: PathBuf,
    /// The design as the pane names it: its path inside the designs folder.
    pub name: String,
    pub open: usize,
    pub resolved: usize,
}

/// Which notes the sidebar lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    Open,
    Resolved,
}

impl Filter {
    pub fn name(self) -> &'static str {
        match self {
            Filter::Open => "open",
            Filter::Resolved => "resolved",
        }
    }
}

/// A note being written: the element it is about, where in it the click was, and the words so far.
#[derive(Debug, Clone, PartialEq)]
pub struct Composer {
    pub element: Element,
    pub fx: f32,
    pub fy: f32,
    pub text: String,
}

/// Something the page is to be told, or made to do.
#[derive(Debug, Clone, PartialEq)]
pub enum PageCall {
    SetMode(Mode),
    SetPins(Vec<PinSpec>),
    Select(String),
    Focus(u32),
    Clear,
    Parent,
    Child,
}

/// Something only the window can do for a design.
#[derive(Debug, Clone, PartialEq)]
pub enum Want {
    /// Open the file as text, which is what `Open Source` is.
    OpenSource(PathBuf),
    /// Load the page again.
    Reload(PathBuf),
}

/// The dialogs the provider can have open.
#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    /// Asking what a new design is called.
    NewDesign { name: String },
    /// The copies of a design, to restore one.
    History { design: PathBuf },
}

/// What is known about one open design.
#[derive(Debug)]
pub struct DesignView {
    pub notes: NotesFile,
    pub device: Device,
    pub mode: Mode,
    pub filter: Filter,
    /// The element chosen in the page.
    pub selection: Option<Element>,
    /// The note open in the sidebar.
    pub active: Option<u32>,
    /// A note being written.
    pub composing: Option<Composer>,
    /// The reply box of the active note.
    pub reply: String,
    /// The prompt box under the page.
    pub prompt: String,
    /// The notes whose element the page could not find.
    pub detached: BTreeSet<u32>,
    /// Moves whenever the pins have to be sent to the page again.
    pub page_revision: u64,
    /// What the page last said about each note's element, for the markup a request carries.
    pub details: BTreeMap<u32, Element>,
    /// The last thing that happened, drawn at the right of the toolbar.
    pub status: String,
    pub(super) calls: Vec<PageCall>,
    pub(super) pins_dirty: bool,
    pub(super) notes_stamp: Option<SystemTime>,
}

impl DesignView {
    fn new(notes: NotesFile, device: Device) -> Self {
        Self {
            notes,
            device,
            mode: Mode::Browse,
            filter: Filter::Open,
            selection: None,
            active: None,
            composing: None,
            reply: String::new(),
            prompt: String::new(),
            detached: BTreeSet::new(),
            page_revision: 0,
            details: BTreeMap::new(),
            status: String::new(),
            calls: Vec::new(),
            pins_dirty: true,
            notes_stamp: None,
        }
    }

    /// The notes the sidebar lists, in the order they were made.
    pub fn listed(&self) -> Vec<&notes::Note> {
        let wanted = match self.filter {
            Filter::Open => notes::NoteState::Open,
            Filter::Resolved => notes::NoteState::Resolved,
        };
        self.notes.notes.iter().filter(|note| note.state == wanted).collect()
    }
}

/// The Designer plugin.
#[derive(Debug)]
pub struct Designer {
    open: bool,
    project: Option<PathBuf>,
    folder: Option<PathBuf>,
    pub config: Configuration,
    pub(super) designs: Vec<DesignSummary>,
    pub(super) views: BTreeMap<PathBuf, DesignView>,
    showing: Option<PathBuf>,
    /// The Designer pane's own tabs, `None` being a blank tab, and which one is showing. See [`tabs`].
    pub(super) tabs: Vec<Option<PathBuf>>,
    pub(super) active_tab: usize,
    /// The project whose remembered tabs have been read, so they are read once a project.
    pub(super) tabs_read_for: Option<PathBuf>,
    pub modal: Option<Modal>,
    /// The last thing that went wrong while reading the settings, drawn on the Settings page.
    pub problem: Option<String>,
    outbox: Vec<Request>,
    wants: Vec<Want>,
    config_changed: bool,
    config_stamp: Option<SystemTime>,
    looked_at: Instant,
}

impl Default for Designer {
    fn default() -> Self {
        Self::new()
    }
}

impl Designer {
    pub fn new() -> Self {
        Self {
            open: false,
            project: None,
            folder: None,
            config: Configuration::default(),
            designs: Vec::new(),
            views: BTreeMap::new(),
            showing: None,
            tabs: vec![None],
            active_tab: 0,
            tabs_read_for: None,
            modal: None,
            problem: None,
            outbox: Vec::new(),
            wants: Vec::new(),
            config_changed: false,
            config_stamp: None,
            looked_at: Instant::now(),
        }
    }

    // ------------------------------------------------------------------------------- where things are

    /// The project the window has open.
    pub fn project(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    /// The designs in the folder, with their note counts.
    pub fn designs(&self) -> &[DesignSummary] {
        &self.designs
    }

    /// The design whose tab is showing in the Designer pane.
    pub fn showing_design(&self) -> Option<&Path> {
        self.showing.as_deref()
    }

    /// Whether a file is one of the project's designs.
    ///
    /// @param path - a file, absolute or relative to the project
    pub fn is_a_design(&self, path: &Path) -> bool {
        match self.project.as_deref() {
            Some(project) => library::is_a_design(project, &self.config.folder, path),
            None => false,
        }
    }

    /// A path in the project as the provider keys it: relative, and tidy.
    ///
    /// @param path - a file, absolute or relative to the project
    pub fn key_of(&self, path: &Path) -> Option<PathBuf> {
        library::relative_to_project(self.project.as_deref()?, path)
    }

    /// A design's file on disk.
    ///
    /// @param design - the design, relative to the project
    pub fn absolute(&self, design: &Path) -> Option<PathBuf> {
        Some(self.project.as_deref()?.join(design))
    }

    /// The design as the pane and the command line name it.
    ///
    /// @param design - the design, relative to the project
    pub fn name_of(&self, design: &Path) -> String {
        library::name_of(&self.config.folder, design)
    }

    /// The open design's view.
    ///
    /// @param design - the design, relative to the project
    pub fn design_view(&self, design: &Path) -> Option<&DesignView> {
        self.views.get(design)
    }

    pub fn design_view_mut(&mut self, design: &Path) -> Option<&mut DesignView> {
        self.views.get_mut(design)
    }

    /// The view of a design, read from disk if nobody has looked at it yet.
    ///
    /// @param design - the design, relative to the project
    pub fn loaded(&mut self, design: &Path) -> Result<&mut DesignView, String> {
        if !self.views.contains_key(design) {
            let project = self.project.clone().ok_or_else(|| "No project is open.".to_owned())?;
            let name = self.name_of(design);
            let comments = project.join(library::comments_path(design));
            let notes = NotesFile::read(&comments, &name)?;
            let device = self.remembered_device(design);
            let mut view = DesignView::new(notes, device);
            view.notes_stamp = modified(&comments);
            self.views.insert(design.to_path_buf(), view);
        }
        self.views.get_mut(design).ok_or_else(|| "The design could not be opened.".to_owned())
    }

    /// Say a design's tab is showing, and read its notes if that has not been done.
    ///
    /// @param design - the design, relative to the project
    pub fn show(&mut self, design: &Path) {
        if self.showing.as_deref() != Some(design) {
            self.showing = Some(design.to_path_buf());
        }
        if let Err(why) = self.loaded(design) {
            self.problem = Some(why);
        }
    }

    /// Say no design is showing, which is what the blank tab is.
    pub fn show_nothing(&mut self) {
        self.showing = None;
    }

    /// Say a design's tab has gone, and let go of what was known about it.
    ///
    /// @param design - the design, relative to the project
    pub fn forget(&mut self, design: &Path) {
        self.views.remove(design);
        if self.showing.as_deref() == Some(design) {
            self.showing = None;
        }
    }

    // ----------------------------------------------------------------------------- the list of designs

    /// List the designs again and count each one's notes.
    pub fn refresh_designs(&mut self) {
        let Some(project) = self.project.clone() else {
            self.designs.clear();
            return;
        };
        let folder = self.config.folder.clone();
        self.designs = library::list(&project, &folder)
            .into_iter()
            .map(|path| {
                let (open, resolved) = match self.views.get(&path) {
                    Some(view) => view.notes.counts(),
                    None => counts_on_disk(&project, &path),
                };
                DesignSummary { name: library::name_of(&folder, &path), path, open, resolved }
            })
            .collect();
        self.looked_at = Instant::now();
    }

    /// How many notes are open across every design, which is the pane's badge.
    pub fn open_notes(&self) -> usize {
        self.designs.iter().map(|design| design.open).sum()
    }

    // ------------------------------------------------------------------------------------ settings

    /// Write the settings and list the designs again, because the folder may have changed.
    pub fn write_the_configuration(&mut self) -> Result<(), String> {
        self.config_changed = true;
        self.refresh_designs();
        match self.folder.clone() {
            Some(folder) => {
                let written = self.config.write(&folder);
                self.config_stamp = modified(&folder.join(Configuration::FILE));
                written
            }
            None => Ok(()),
        }
    }

    /// Whether the settings changed since the window last asked, which is when it must tell the explorer.
    pub fn take_config_changed(&mut self) -> bool {
        std::mem::take(&mut self.config_changed)
    }

    /// What the window must do for a design that only it can.
    pub fn take_wants(&mut self) -> Vec<Want> {
        std::mem::take(&mut self.wants)
    }

    pub(super) fn device_file(&self) -> Option<PathBuf> {
        Some(self.project.as_deref()?.join(".unluminous").join("designs.conf"))
    }

    /// The device a person last chose for a design, or the one the settings say.
    fn remembered_device(&self, design: &Path) -> Device {
        let key = format!("device.{}", config::slashed(design));
        self.device_file()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .and_then(|text| Values::parse(&text).text(&key).and_then(Device::from_name))
            .unwrap_or(self.config.device)
    }

    /// Write down the device chosen for a design, in the project's own `.unluminous` folder.
    pub(super) fn remember_device(&self, design: &Path, device: Device) {
        let Some(file) = self.device_file() else { return };
        let mut values = Values::parse(&std::fs::read_to_string(&file).unwrap_or_default());
        values.set(&format!("device.{}", config::slashed(design)), device.name());
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = crate::services::store::write_atomically(
            &file,
            values
                .to_text_headed("# What one person chose in the Designer, in this project.")
                .as_bytes(),
        );
    }

    // ------------------------------------------------------------------------- what the page reported

    /// What the page needs to be told, in the order it needs it.
    ///
    /// @param design - the design, relative to the project
    pub fn take_calls(&mut self, design: &Path) -> Vec<PageCall> {
        let Some(view) = self.views.get_mut(design) else { return Vec::new() };
        let mut calls = std::mem::take(&mut view.calls);
        if view.pins_dirty {
            view.pins_dirty = false;
            calls.push(PageCall::SetPins(pins_of(view)));
        }
        calls
    }

    /// The pins to draw: every open note, the active one marked.
    ///
    /// @param design - the design, relative to the project
    pub fn pins(&self, design: &Path) -> Vec<PinSpec> {
        self.views.get(design).map(pins_of).unwrap_or_default()
    }
}

/// The pins of a design's open notes.
fn pins_of(view: &DesignView) -> Vec<PinSpec> {
    view.notes
        .notes
        .iter()
        .filter(|note| note.state == notes::NoteState::Open)
        .map(|note| PinSpec {
            id: note.id,
            selector: note.anchor.selector.clone(),
            fx: note.anchor.fx,
            fy: note.anchor.fy,
            page: (note.anchor.page_x, note.anchor.page_y),
            active: view.active == Some(note.id),
            detached: view.detached.contains(&note.id),
        })
        .collect()
}

/// A file's modified time, when it has one.
fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

/// How many notes of each kind a design has, read from its notes file without keeping it.
fn counts_on_disk(project: &Path, design: &Path) -> (usize, usize) {
    let comments = project.join(library::comments_path(design));
    NotesFile::read(&comments, "").map(|file| file.counts()).unwrap_or((0, 0))
}

impl UiProvider for Designer {
    fn id(&self) -> &'static str {
        "designer"
    }

    fn open(&mut self, context: &Context) -> Result<(), String> {
        self.project = context.project.clone();
        self.folder = context.folder.clone();
        if let Some(folder) = &self.folder {
            let (config, refused) = Configuration::read(folder);
            self.config = config;
            self.config_stamp = modified(&folder.join(Configuration::FILE));
            if !refused.is_empty() {
                self.problem = Some(refused.join(" "));
            }
        }
        self.open = true;
        self.refresh_designs();
        self.read_tabs();
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.open
    }

    /// The wells, the raised cards and the gradients `egui` cannot draw.
    fn draws_chrome(&self) -> bool {
        true
    }

    fn pane(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::designer::pane::show(self, ui, look)
    }

    fn settings(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::designer::settings_page::show(self, ui, look)
    }

    fn modal(&mut self, ctx: &egui::Context, look: &Look<'_>) -> (Vec<Request>, bool) {
        crate::components::designer::modal::show(self, ctx, look)
    }

    fn command(&mut self, command: &str, arguments: &[String]) -> Result<Answer, String> {
        commands::run(self, command, arguments)
    }

    fn commands(&self) -> Vec<(&'static str, &'static str)> {
        commands::LIST.to_vec()
    }

    fn view(&self) -> serde_json::Value {
        actions::view_value(self)
    }

    fn badge(&self) -> Option<String> {
        match self.open_notes() {
            0 => None,
            many => Some(many.to_string()),
        }
    }

    fn catch_up(&mut self) -> bool {
        self.look_again_when_due();
        false
    }

    fn asking(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.outbox)
    }

    fn answered(&mut self, id: &str, answer: Result<serde_json::Value, String>) {
        self.the_agent_answered(id, answer);
    }

    fn showing(&mut self, project: Option<&Path>, _file: Option<&Path>) {
        if let Some(project) = project {
            if self.project.as_deref() != Some(project) {
                self.project = Some(project.to_path_buf());
                self.views.clear();
                self.refresh_designs();
                self.read_tabs();
            }
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn close(&mut self) {
        self.views.clear();
        self.showing = None;
        self.tabs_read_for = None;
        self.open = false;
    }
}
