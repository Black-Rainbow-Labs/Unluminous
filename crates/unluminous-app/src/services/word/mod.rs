//! The Word plugin's provider: every open document, its layout and its view.
//!
//! `task-2341`, and `tasks/task-2341-word-tdd.md` is the design. The document itself is
//! `unluminous_word`, which has no window in it; this is what holds one per tab, lays it out with the
//! window's fonts when it changed, and answers the commands the panel, the keys and `plugins run word`
//! all send. **A document is read when its tab is first drawn**, and dropped when its tab closes, so a
//! window with no Word document open holds nothing.

pub mod actions;
pub mod commands;
pub mod fonts;
pub mod spelling;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use unluminous_word::layout::{self, Pages};
use unluminous_word::position::{Pos, Selection, StoryId};
use unluminous_word::session::Session;

use crate::services::plugin_ui::{Answer, Context, Look, Request, UiProvider};

/// The key a document's tab has: `word:` and the path it was opened from, or `word:untitled-<n>`.
pub fn tab_key(path: &Path) -> String {
    format!("word:{}", path.to_string_lossy().replace('\\', "/"))
}

/// What a person set on the Settings page.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// The name written on comments and tracked changes.
    pub author: String,
    /// The zoom a document opens at, 1.0 for 100%.
    pub zoom: f32,
    /// Show formatting marks: paragraph marks, spaces and hidden text.
    pub marks: bool,
    /// Underline misspelled and repeated words as they are typed.
    pub spelling: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config { author: whoami(), zoom: 1.0, marks: false, spelling: true }
    }
}

/// The machine's user name, which Word also uses until a person types theirs.
fn whoami() -> String {
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default()
}

impl Config {
    const FILE: &'static str = "word.conf";

    fn read(folder: &Path) -> Config {
        let Ok(text) = std::fs::read_to_string(folder.join(Self::FILE)) else {
            return Config::default();
        };
        let mut config = Config::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            match key.trim() {
                "author" => config.author = value.to_owned(),
                "zoom" => config.zoom = value.parse::<f32>().unwrap_or(1.0).clamp(0.25, 5.0),
                "marks" => config.marks = value == "true",
                "spelling" => config.spelling = value != "false",
                _ => {}
            }
        }
        config
    }

    pub fn write(&self, folder: &Path) -> Result<(), String> {
        std::fs::create_dir_all(folder)
            .map_err(|e| format!("{} could not be made: {e}", folder.display()))?;
        let text = format!(
            "# The Word plugin's settings. The author is the name on comments and tracked changes.\nauthor = {}\nzoom = {}\nmarks = {}\nspelling = {}\n",
            self.author, self.zoom, self.marks, self.spelling
        );
        crate::services::store::write_atomically(&folder.join(Self::FILE), text.as_bytes())
            .map_err(|e| format!("{} could not be written: {e}", folder.join(Self::FILE).display()))
    }
}

/// A level of the drill down panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    Root,
    Text,
    Paragraph,
    Styles,
    Lists,
    Insert,
    Page,
    References,
    Review,
    Document,
}

impl Level {
    pub const ALL: &'static [Level] = &[
        Level::Text,
        Level::Paragraph,
        Level::Styles,
        Level::Lists,
        Level::Insert,
        Level::Page,
        Level::References,
        Level::Review,
        Level::Document,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Level::Root => "Format",
            Level::Text => "Text",
            Level::Paragraph => "Paragraph",
            Level::Styles => "Styles",
            Level::Lists => "Lists",
            Level::Insert => "Insert",
            Level::Page => "Page",
            Level::References => "References",
            Level::Review => "Review",
            Level::Document => "Document",
        }
    }

    pub fn parse(name: &str) -> Option<Level> {
        std::iter::once(Level::Root)
            .chain(Level::ALL.iter().copied())
            .find(|level| level.name().eq_ignore_ascii_case(name))
    }

    /// One line under the level's name on the root level.
    pub fn summary(self) -> &'static str {
        match self {
            Level::Root => "",
            Level::Text => "Font, size, colour, highlight, effects, case",
            Level::Paragraph => "Alignment, indents, spacing, line spacing, borders",
            Level::Styles => "Headings, quotes, titles, your own styles",
            Level::Lists => "Bullets, numbering, levels",
            Level::Insert => "Table, picture, break, link, symbol, field",
            Level::Page => "Size, orientation, margins, columns, header and footer",
            Level::References => "Table of contents, footnotes, captions",
            Level::Review => "Track changes, comments, word count",
            Level::Document => "Properties, view, export",
        }
    }
}

/// How one document is being looked at.
#[derive(Clone)]
pub struct View {
    pub zoom: f32,
    /// How far down the pages are scrolled, in screen points.
    pub scroll: f32,
    pub marks: bool,
    /// Underline misspelled and repeated words.
    pub spelling: bool,
    /// The card a right click on an underlined word opens, or F7: the word, and what it could be.
    pub spell_menu: Option<SpellMenu>,
    pub navigation: bool,
    /// The panel's levels, deepest last; empty when the panel is closed.
    pub panel: Vec<Level>,
    /// The `/` menu, open at a position with what has been typed after the slash.
    pub slash: Option<(Pos, String, usize)>,
    /// `Ctrl+/`: the command search and its query.
    pub palette: Option<(String, usize)>,
    /// The find bar: what is looked for, what replaces it, match case, whole words.
    pub find: Option<Find>,
    /// The x the caret keeps while it moves up and down.
    pub goal_x: Option<f32>,
    /// Scroll the caret into view on the next frame.
    pub reveal: bool,
    /// A press on the pages is selecting.
    pub dragging: bool,
    /// The last press, for double and triple clicks: when, where, how many.
    pub clicks: (f64, egui::Pos2, u32),
    /// What the panel's fields hold while they are typed in.
    pub fields: std::collections::HashMap<&'static str, String>,
    /// Copy Formatting: the run properties to paste.
    pub copied_format: Option<unluminous_word::xml::El>,
    /// The picture textures, by package part.
    pub pictures: std::collections::HashMap<String, Option<egui::TextureHandle>>,
}

/// The card for one underlined word: where it is, and the spellings the dictionary suggests.
#[derive(Debug, Clone)]
pub struct SpellMenu {
    pub word: unluminous_word::spell::Misspelling,
    pub suggestions: Vec<String>,
}

/// The find bar.
#[derive(Debug, Clone, Default)]
pub struct Find {
    pub needle: String,
    pub with: String,
    pub case: bool,
    pub word: bool,
    pub replace: bool,
    pub current: usize,
}

impl View {
    fn new(config: &Config) -> View {
        View {
            zoom: config.zoom,
            scroll: 0.0,
            marks: config.marks,
            spelling: config.spelling,
            spell_menu: None,
            navigation: false,
            panel: Vec::new(),
            slash: None,
            palette: None,
            find: None,
            goal_x: None,
            reveal: true,
            dragging: false,
            clicks: (0.0, egui::Pos2::ZERO, 0),
            fields: Default::default(),
            copied_format: None,
            pictures: Default::default(),
        }
    }
}

/// One open document.
pub struct Doc {
    pub key: String,
    pub path: Option<PathBuf>,
    pub session: Session,
    pub pages: Pages,
    /// The session revision `pages` was laid out at.
    pub laid: u64,
    pub view: View,
    /// The revision when it was last saved.
    pub saved: u64,
    /// What the pages were last laid out with: `window` fonts, or `fixed` widths before the tab was first drawn.
    pub formatted_text: Option<String>,
    /// How many frames have drawn this document's tab.
    pub drawn: u64,
    /// On the last frame: whether the tab held the keyboard, and whether a field of its own had it instead.
    pub keyboard: (bool, bool),
}

impl std::fmt::Debug for Doc {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Doc")
            .field("key", &self.key)
            .field("path", &self.path)
            .field("pages", &self.pages.pages.len())
            .finish()
    }
}

impl Doc {
    /// Lay the document out again if it changed since it was last laid out.
    pub fn lay_out(&mut self, renderer: &crate::services::text_renderer::TextRenderer) {
        if self.laid == self.session.revision && !self.pages.pages.is_empty() {
            return;
        }
        let fonts = fonts::WordFonts::new(renderer);
        self.pages = layout::lay_out(
            &self.session.document,
            &fonts,
            layout::Options { show_marks: self.view.marks },
        );
        self.laid = self.session.revision;
        self.formatted_text = Some("window".into());
    }

    /// Lay out with fixed metrics when the window has not drawn the tab yet, so an agent still gets pages.
    pub fn ensure_pages(&mut self) {
        if self.pages.pages.is_empty() || self.laid != self.session.revision {
            self.pages = layout::lay_out(
                &self.session.document,
                &unluminous_core::FixedMetrics { cluster_width: 5.5, ascent: 9.0, descent: 3.0 },
                layout::Options::default(),
            );
            // Not marked laid, so the window lays it out again with its real fonts.
            self.formatted_text = Some("fixed".into());
        }
    }

    pub fn modified(&self) -> bool {
        self.session.modified && self.session.revision != self.saved
    }

    /// The name shown on the tab.
    pub fn label(&self) -> String {
        match &self.path {
            Some(path) => path
                .file_name()
                .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned()),
            None => self.key.trim_start_matches("word:").to_owned(),
        }
    }

    /// Write the document to its file, or to `to`.
    pub fn save(&mut self, to: Option<&Path>) -> Result<PathBuf, String> {
        let path = to
            .map(Path::to_path_buf)
            .or_else(|| self.path.clone())
            .ok_or("this document has never been saved, so it needs a name: use Save As")?;
        let author = self.session.author.clone();
        unluminous_word::new::set_property(
            &mut self.session.document,
            "cp:lastModifiedBy",
            &author,
        );
        if unluminous_word::new::property(&self.session.document, "dc:creator").is_none() {
            unluminous_word::new::set_property(&mut self.session.document, "dc:creator", &author);
        }
        unluminous_word::new::set_property(
            &mut self.session.document,
            "dcterms:modified",
            &unluminous_word::session::now_iso(),
        );
        let bytes = unluminous_word::write::save(&self.session.document)?;
        crate::services::store::write_atomically(&path, &bytes)
            .map_err(|e| format!("{} could not be written: {e}", path.display()))?;
        self.path = Some(path.clone());
        self.saved = self.session.revision;
        self.session.modified = false;
        Ok(path)
    }
}

/// The provider.
#[derive(Debug, Default)]
pub struct Word {
    open: bool,
    pub project: Option<PathBuf>,
    pub folder: Option<PathBuf>,
    pub config: Config,
    pub docs: Vec<Doc>,
    /// The document commands act on: the one last drawn or chosen with `use`.
    pub current: Option<String>,
    /// Whether Office's font folders have been added to the renderer.
    fonts_added: bool,
    pub outbox: Vec<Request>,
    untitled: u32,
    /// The Settings page's author field while it is being typed in.
    pub author_field: Option<String>,
}

impl Word {
    pub fn new() -> Self {
        Word::default()
    }

    /// The document with this key, read from disk the first time it is asked for.
    pub fn doc(&mut self, key: &str) -> Result<&mut Doc, String> {
        if let Some(index) = self.docs.iter().position(|doc| doc.key == key) {
            return Ok(&mut self.docs[index]);
        }
        let path = PathBuf::from(
            key.strip_prefix("word:").ok_or_else(|| format!("{key} is not a Word tab"))?,
        );
        self.load(key, &path)?;
        Ok(self.docs.last_mut().expect("just loaded"))
    }

    /// Read a document from disk into a new `Doc`.
    pub fn load(&mut self, key: &str, path: &Path) -> Result<(), String> {
        if self.docs.iter().any(|doc| doc.key == key) {
            return Ok(());
        }
        let bytes = std::fs::read(path)
            .map_err(|e| format!("{} could not be read: {e}", path.display()))?;
        let mut document =
            unluminous_word::read::open(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        // A template opens as a new document made from it, which is what double clicking one does.
        let template = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("dotx"));
        if template {
            unluminous_word::new::set_property(&mut document, "dc:creator", "");
        }
        let mut session = Session::new(document);
        session.author = self.config.author.clone();
        self.docs.push(Doc {
            key: key.to_owned(),
            path: (!template).then(|| path.to_path_buf()),
            session,
            pages: Pages::default(),
            laid: 0,
            view: View::new(&self.config),
            saved: 1,
            formatted_text: None,
            drawn: 0,
            keyboard: (false, false),
        });
        self.current = Some(key.to_owned());
        Ok(())
    }

    /// A new, empty document, untitled until it is saved, or at `path` when one is given.
    pub fn new_document(&mut self, path: Option<&Path>) -> Result<String, String> {
        let mut session = Session::new(unluminous_word::new::blank());
        session.author = self.config.author.clone();
        unluminous_word::new::set_property(
            &mut session.document,
            "dc:creator",
            &self.config.author,
        );
        unluminous_word::new::set_property(
            &mut session.document,
            "dcterms:created",
            &unluminous_word::session::now_iso(),
        );
        let key = match path {
            Some(path) => tab_key(path),
            None => {
                self.untitled += 1;
                format!("word:Untitled {}", self.untitled)
            }
        };
        let mut doc = Doc {
            key: key.clone(),
            path: path.map(Path::to_path_buf),
            session,
            pages: Pages::default(),
            laid: 0,
            view: View::new(&self.config),
            saved: 0,
            formatted_text: None,
            drawn: 0,
            keyboard: (false, false),
        };
        if path.is_some() {
            doc.save(None)?;
        }
        self.docs.retain(|d| d.key != key);
        self.docs.push(doc);
        self.current = Some(key.clone());
        Ok(key)
    }

    /// The document commands act on.
    pub fn current_doc(&mut self) -> Result<&mut Doc, String> {
        let key =
            self.current.clone().or_else(|| self.docs.last().map(|d| d.key.clone())).ok_or(
                "no Word document is open: open a .docx file, or run `plugins run word new`",
            )?;
        self.doc(&key)
    }

    /// Add Office's font folders to the renderer, once.
    pub fn add_office_fonts(&mut self, renderer: &crate::services::text_renderer::TextRenderer) {
        if self.fonts_added {
            return;
        }
        self.fonts_added = true;
        for folder in fonts::office_font_folders() {
            renderer.add_font_folder(&folder);
        }
    }

    /// Where a document's export goes: beside it, with the new extension.
    pub fn export_path(doc: &Doc, extension: &str) -> Option<PathBuf> {
        doc.path.as_ref().map(|path| path.with_extension(extension))
    }
}

impl UiProvider for Word {
    fn id(&self) -> &'static str {
        "word"
    }

    fn open(&mut self, context: &Context) -> Result<(), String> {
        self.project = context.project.clone();
        self.folder = context.folder.clone();
        spelling::use_folder(self.folder.as_deref());
        if let Some(folder) = &self.folder {
            self.config = Config::read(folder);
        }
        self.open = true;
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.open
    }

    /// A Word tab is drawn by `tab_keyed`; with no key there is nothing to draw.
    fn pane(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        let _ = look;
        ui.label("Open a .docx file to see it here.");
        Vec::new()
    }

    fn tab_keyed(&mut self, key: &str, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        self.add_office_fonts(look.renderer);
        self.current = Some(key.to_owned());
        let config = self.config.clone();
        match self.doc(key) {
            Ok(doc) => crate::components::word::show(doc, &config, ui, look),
            Err(problem) => {
                ui.painter().text(
                    ui.max_rect().min + egui::vec2(24.0, 24.0),
                    egui::Align2::LEFT_TOP,
                    problem,
                    egui::FontId::proportional(look.font_size),
                    look.palette.text_dim,
                );
                Vec::new()
            }
        }
    }

    fn modified(&self, key: &str) -> bool {
        self.docs.iter().any(|doc| doc.key == key && doc.modified())
    }

    fn save(&mut self, key: &str) -> Result<String, String> {
        // A document with no file yet asks where to go.
        if self.doc(key)?.path.is_none() {
            let start = self.project.clone();
            let chosen = rfd::FileDialog::new()
                .set_title("Save Word Document")
                .add_filter("Word document", &["docx"])
                .set_directory(start.unwrap_or_default())
                .save_file();
            let Some(chosen) = chosen else { return Err("Not saved".to_owned()) };
            let chosen =
                if chosen.extension().is_none() { chosen.with_extension("docx") } else { chosen };
            return self.save_as(key, &chosen);
        }
        let saved = self.doc(key)?.save(None)?;
        Ok(format!("Saved {}", saved.display()))
    }

    fn save_as(&mut self, key: &str, to: &Path) -> Result<String, String> {
        let doc = self.doc(key)?;
        let saved = doc.save(Some(to))?;
        let label = doc.label();
        self.outbox.push(Request::RenameTab { key: key.to_owned(), label });
        Ok(format!("Saved {}", saved.display()))
    }

    fn closed(&mut self, key: &str) {
        self.docs.retain(|doc| doc.key != key);
        if self.current.as_deref() == Some(key) {
            self.current = self.docs.last().map(|d| d.key.clone());
        }
    }

    fn claims_key(&self, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        crate::components::word::keys::claims(key, &modifiers)
    }

    fn settings(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::word::settings_page::show(self, ui, look)
    }

    fn command(&mut self, command: &str, arguments: &[String]) -> Result<Answer, String> {
        commands::run(self, command, arguments)
    }

    fn commands(&self) -> Vec<(&'static str, &'static str)> {
        unluminous_cli::plugins::WORD.to_vec()
    }

    fn view(&self) -> serde_json::Value {
        commands::view(self)
    }

    fn asking(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.outbox)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn close(&mut self) {
        self.docs.clear();
        self.current = None;
        self.open = false;
    }
}

/// The selection as data, for `view` and `selection`.
pub fn selection_value(selection: &Selection) -> serde_json::Value {
    let pos = |p: &Pos| serde_json::json!({ "story": story_name(&p.story), "path": p.path, "offset": p.offset });
    serde_json::json!({ "anchor": pos(&selection.anchor), "head": pos(&selection.head), "empty": selection.is_empty() })
}

/// A story's name as `read` and `selection` print it.
pub fn story_name(story: &StoryId) -> String {
    match story {
        StoryId::Body => "body".into(),
        StoryId::Header(id) => format!("header:{id}"),
        StoryId::Footnote(id) => format!("footnote:{id}"),
        StoryId::Endnote(id) => format!("endnote:{id}"),
        StoryId::Comment(id) => format!("comment:{id}"),
    }
}
