//! The Spreadsheet plugin: Excel workbooks and CSV files, each in a tab of its own (`task-2338`).
//!
//! `tasks/task-2338-spreadsheet-prd.md` and `-tdd.md` are the design. Everything that is not drawing is
//! here or in `unluminous-sheet`: which workbooks are open, what each tab is looking at, and the actions a
//! person and an agent take on them. `components::spreadsheet` draws a tab and reports what was done in it
//! as [`actions::Act`] values, and [`commands`] turns an agent's words into the same values, so a change made
//! with the pointer and the same change asked for by an agent go through [`actions::apply`] alike.
//!
//! **A workbook is read when its tab is first drawn or when a command names it**, never when the plugin
//! is switched on. A book is let go of when its tab closes.

pub mod actions;
pub mod axis;
pub mod catalogue;
pub mod catalogue_a;
pub mod catalogue_b;
pub mod catalogue_c;
pub mod catalogue_charts;
pub mod commands;
pub mod conditional;
pub mod config;
pub mod display;
pub mod entry;
pub mod excel_ink;
pub mod formula_text;
pub mod measure;
pub mod names;
pub mod print;
pub mod status;
pub mod verbs;
pub mod verbs_a;
pub mod verbs_a_data;
pub mod verbs_a_outline;
pub mod verbs_a_read;
pub mod verbs_a_review;
pub mod verbs_a_whatif;
pub mod verbs_b;
pub mod verbs_b_clip;
pub mod verbs_b_files;
pub mod verbs_b_page;
pub mod verbs_b_pivot;
pub mod verbs_b_pivot_more;
pub mod verbs_b_slicers;
pub mod verbs_c;
pub mod verbs_c_spark;
pub mod verbs_charts;
pub mod verbs_charts_axis;
pub mod verbs_charts_info;
pub mod verbs_charts_series;
pub mod verbs_d;
pub mod view;
pub mod view_a;
pub mod view_b;
pub mod view_c;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_a;
#[cfg(test)]
mod tests_b;
#[cfg(test)]
mod tests_c;
#[cfg(test)]
mod tests_charts;

use std::path::{Path, PathBuf};

use unluminous_sheet::edit::Session;
use unluminous_sheet::{FileKind, Workbook};

use crate::services::plugin_ui::{Answer, Context, Look, Request, UiProvider};

pub use view::View;

/// What a tab key starts with: `spreadsheet:<path>`.
pub const KEY_PREFIX: &str = "spreadsheet:";

/// One workbook open in a tab.
#[derive(Debug)]
pub struct Book {
    /// The tab's key, `spreadsheet:<path>`.
    pub key: String,
    pub path: PathBuf,
    pub kind: FileKind,
    pub session: Session,
    pub view: View,
    /// Counts every change, so a drawing cache keyed on it is thrown away when the workbook changes.
    pub revision: u64,
    /// When the file was last read or written by this tab, so a change made by another program is noticed.
    pub stamp: Option<std::time::SystemTime>,
    /// The sheet's conditional formats, prepared for the revision they were prepared at.
    pub rules: Option<conditional::Rules>,
    /// The values under each filter button, ticked while kept, while that button's list is open.
    pub filter_values: Vec<(String, bool)>,
    /// The pictures of the workbook, decoded once and kept as textures, by their part's path. A picture
    /// that could not be decoded is kept as `None` so it is not tried again every frame.
    pub pictures: Pictures,
    /// The last change made, which F4 and Ctrl+Y repeat on the selection when there is nothing to redo.
    pub last: Option<actions::Act>,
    /// How a text file was encoded when it was read, so it is saved the same way. `None` for a new file,
    /// which is saved as Excel's default for its kind.
    pub encoding: Option<unluminous_sheet::csv::Encoding>,
}

/// The decoded pictures of a workbook. Written out by hand because a texture handle has no `Debug`.
#[derive(Default)]
pub struct Pictures(pub std::collections::HashMap<String, Option<egui::TextureHandle>>);

impl std::fmt::Debug for Pictures {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{} pictures", self.0.len())
    }
}

impl Book {
    /// Read a workbook from its file.
    pub fn read(key: &str, path: &Path) -> Result<Book, String> {
        let kind = FileKind::of(path)?;
        let bytes = std::fs::read(path)
            .map_err(|e| format!("{} could not be read: {e}", path.display()))?;
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Sheet1");
        let book = unluminous_sheet::open(&bytes, kind, name, this_year())
            .map_err(|e| format!("{} could not be opened: {e}", path.display()))?;
        let mut read = Book::of(key, path, kind, book, modified_time(path));
        if matches!(kind, FileKind::Csv | FileKind::Tsv | FileKind::Txt) {
            read.encoding = Some(unluminous_sheet::csv::encoding_of(&bytes));
        }
        Ok(read)
    }

    /// A book that has not been written anywhere yet, for `new`.
    pub fn new_workbook(key: &str, path: &Path) -> Book {
        let kind = FileKind::of(path).unwrap_or(FileKind::Xlsx);
        Book::of(key, path, kind, Workbook::new(), None)
    }

    fn of(
        key: &str,
        path: &Path,
        kind: FileKind,
        book: Workbook,
        stamp: Option<std::time::SystemTime>,
    ) -> Book {
        let mut session = Session::new(book, now_serial());
        session.this_year = this_year();
        let mut view = View {
            sheet: session.book.active_sheet.min(session.book.sheets.len().saturating_sub(1)),
            ..View::default()
        };
        restore_view(&session.book, &mut view);
        Book {
            key: key.to_owned(),
            path: path.to_path_buf(),
            kind,
            session,
            view,
            revision: 0,
            stamp,
            rules: None,
            filter_values: Vec::new(),
            pictures: Default::default(),
            last: None,
            encoding: None,
        }
    }

    /// Write the workbook to its file.
    pub fn save(&mut self) -> Result<String, String> {
        self.session.book.active_sheet = self.view.sheet;
        keep_view(&mut self.session, &self.view);
        let options = unluminous_sheet::SaveOptions { encoding: self.encoding };
        let bytes = unluminous_sheet::save_with(&self.session.book, self.kind, &options)?;
        if let Some(folder) = self.path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        // Written beside the file and moved over it, so a failed write leaves the workbook as it was.
        crate::services::store::write_a_source_file(&self.path, &bytes)
            .map_err(|e| format!("{} could not be saved: {e}", self.path.display()))?;
        self.session.modified = false;
        self.stamp = modified_time(&self.path);
        let name = self.name();
        Ok(match self.kind {
            FileKind::Csv | FileKind::Tsv if self.session.book.sheets.len() > 1 => {
                format!(
                    "Saved {name}. A CSV file holds one sheet, so only {} was written.",
                    self.session.book.sheets[self.view.sheet].name
                )
            }
            _ => format!("Saved {name}"),
        })
    }

    /// The file's name.
    pub fn name(&self) -> String {
        self.path.file_name().and_then(|n| n.to_str()).unwrap_or("workbook").to_owned()
    }

    /// Note that the workbook changed, so what was drawn from it is drawn again.
    pub fn changed(&mut self) {
        self.revision += 1;
    }
}

/// Put the view where the sheet's saved view says: its active cell, what was chosen and where it was
/// scrolled, as Excel opens a workbook where it was left.
pub fn restore_view(book: &Workbook, view: &mut View) {
    let Some(sheet) = book.sheets.get(view.sheet) else { return };
    let saved = &sheet.view;
    match saved.selections.last() {
        Some(selection) => {
            let mut ranges = selection.ranges.clone();
            let last = ranges.pop().unwrap_or(unluminous_sheet::Range::cell(selection.active));
            view.extra = ranges;
            view.cursor = selection.active;
            view.anchor = if last.first == selection.active { last.last } else { last.first };
        }
        None => view.select_cell(unluminous_sheet::CellRef::new(0, 0)),
    }
    view.held = None;
    let columns = axis::Axis::columns(sheet);
    let rows = axis::Axis::rows(sheet);
    let frozen = sheet.frozen.unwrap_or((0, 0));
    let top_left =
        saved.pane_top_left.or(saved.split.as_ref().and_then(|s| s.top_left)).or(saved.top_left);
    view.scroll = match top_left {
        Some(at) => egui::Vec2::new(
            (columns.start(at.col) - columns.start(frozen.1)).max(0.0),
            (rows.start(at.row) - rows.start(frozen.0)).max(0.0),
        ),
        None => egui::Vec2::ZERO,
    };
    view.split_scroll = match (saved.split.is_some(), saved.top_left) {
        (true, Some(at)) => egui::Vec2::new(columns.start(at.col), rows.start(at.row)),
        _ => egui::Vec2::ZERO,
    };
    view.reveal = false;
}

/// Write what the view shows into the sheet's saved view, so the file opens where it was left.
pub fn keep_view(session: &mut Session, view: &View) {
    let Some(sheet) = session.book.sheets.get(view.sheet) else { return };
    let columns = axis::Axis::columns(sheet);
    let rows = axis::Axis::rows(sheet);
    let frozen = sheet.frozen.unwrap_or((0, 0));
    let top_left = unluminous_sheet::CellRef::new(
        rows.index_at(rows.start(frozen.0) + view.scroll.y),
        columns.index_at(columns.start(frozen.1) + view.scroll.x),
    );
    session.remember_selection(view.sheet, view.cursor, view.ranges(), top_left);
}

/// When a file was last written, to compare with later.
pub fn modified_time(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// NOW() as a serial on this machine's clock.
pub fn now_serial() -> f64 {
    let at = crate::services::clock::date_time();
    let day =
        unluminous_sheet::dates::serial(at.year as i64, at.month as i64, at.day as i64, false)
            .unwrap_or(0.0);
    day + (at.hour as f64 * 3600.0 + at.minute as f64 * 60.0 + at.second as f64) / 86_400.0
}

/// The year a date typed without one is in.
pub fn this_year() -> i64 {
    crate::services::clock::date_time().year as i64
}

/// The path a tab key names, read against the project when it is relative.
pub fn path_of_key(key: &str, project: Option<&Path>) -> Option<PathBuf> {
    let rest = key.strip_prefix(KEY_PREFIX)?;
    let path = PathBuf::from(rest);
    Some(match (path.is_absolute(), project) {
        (false, Some(project)) => project.join(path),
        _ => path,
    })
}

/// The tab key for a file: relative to the project when it is inside it.
pub fn key_of_path(path: &Path, project: Option<&Path>) -> String {
    let shown = match project.and_then(|project| path.strip_prefix(project).ok()) {
        Some(relative) => relative.to_string_lossy().replace('\\', "/"),
        None => path.to_string_lossy().into_owned(),
    };
    format!("{KEY_PREFIX}{shown}")
}

/// The provider.
#[derive(Debug, Default)]
pub struct Spreadsheet {
    open: bool,
    pub(crate) project: Option<PathBuf>,
    folder: Option<PathBuf>,
    pub(crate) books: Vec<Book>,
    /// The key of the book drawn or named last, which a command that names no book acts on.
    pub(crate) current: Option<String>,
    /// Cells this window copied, kept with their formulas and formats so a paste of the same text pastes
    /// them rather than their shown values.
    pub(crate) clip: Option<actions::Copied>,
    pub(crate) config: config::Config,
    pub(crate) has_the_keyboard: bool,
    /// Requests decided outside a draw, such as the tab an agent's `open` asks for.
    asked: Vec<Request>,
    /// A problem reading the settings file, shown on the Settings page.
    pub(crate) problem: Option<String>,
    /// When the settings file was last looked at, and what its time was.
    looked_at: Option<std::time::Instant>,
    settings_stamp: Option<std::time::SystemTime>,
}

impl Spreadsheet {
    pub fn new() -> Self {
        Self::default()
    }

    /// The book with this key, reading it from its file the first time it is asked for.
    pub fn book_for_key(&mut self, key: &str) -> Result<&mut Book, String> {
        if let Some(index) = self.books.iter().position(|book| book.key == key) {
            return Ok(&mut self.books[index]);
        }
        let path = path_of_key(key, self.project.as_deref())
            .ok_or_else(|| format!("`{key}` is not a spreadsheet tab"))?;
        let mut book =
            if path.exists() { Book::read(key, &path)? } else { Book::new_workbook(key, &path) };
        book.view.paper = self.config.paper;
        self.books.push(book);
        Ok(self.books.last_mut().expect("just pushed"))
    }

    /// The book a command acts on: the one named last.
    pub fn current_book(&mut self) -> Result<&mut Book, String> {
        let key = match &self.current {
            Some(key) if self.books.iter().any(|book| &book.key == key) => key.clone(),
            _ => self.books.last().map(|book| book.key.clone()).ok_or_else(|| {
                "No workbook is open. `open <file>` opens one and `new <file>` makes one."
                    .to_owned()
            })?,
        };
        self.book_for_key(&key)
    }

    /// Open a file as the current book, asking the window for its tab.
    pub fn open_path(&mut self, path: &Path, show: bool) -> Result<String, String> {
        let path = match (path.is_absolute(), &self.project) {
            (false, Some(project)) => project.join(path),
            _ => path.to_path_buf(),
        };
        FileKind::of(&path)?;
        let key = key_of_path(&path, self.project.as_deref());
        let book = self.book_for_key(&key)?;
        let name = book.name();
        self.current = Some(key);
        if show {
            self.asked.push(Request::OpenFile(path));
        }
        Ok(name)
    }

    /// Queue a request for the window, decided outside a draw.
    pub fn ask(&mut self, request: Request) {
        self.asked.push(request);
    }

    /// The requests queued outside a draw, taken.
    pub fn asking(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.asked)
    }

    /// Read the settings file again when something else changed it, such as `settings set
    /// plugins.spreadsheet.paper light`. Looked at once a second.
    pub fn follow_the_settings_file(&mut self) {
        let Some(folder) = self.folder.clone() else { return };
        let now = std::time::Instant::now();
        if self
            .looked_at
            .is_some_and(|at| now.duration_since(at) < std::time::Duration::from_secs(1))
        {
            return;
        }
        self.looked_at = Some(now);
        let stamp = modified_time(&folder.join(config::FILE));
        if stamp != self.settings_stamp {
            self.settings_stamp = stamp;
            let (config, problem) = config::Config::read(&folder);
            self.config = config;
            if problem.is_some() {
                self.problem = problem;
            }
        }
    }

    /// Write the settings to the plugin's folder.
    pub fn write_the_settings(&mut self) -> Result<(), String> {
        let Some(folder) = self.folder.clone() else { return Ok(()) };
        self.config.write(&folder)?;
        self.settings_stamp = modified_time(&folder.join(config::FILE));
        Ok(())
    }
}

impl UiProvider for Spreadsheet {
    fn id(&self) -> &'static str {
        "spreadsheet"
    }

    fn open(&mut self, context: &Context) -> Result<(), String> {
        self.project = context.project.clone();
        self.folder = context.folder.clone();
        if let Some(folder) = &self.folder {
            let (config, problem) = config::Config::read(folder);
            self.config = config;
            self.problem = problem;
            self.settings_stamp = modified_time(&folder.join(config::FILE));
        }
        self.open = true;
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.open
    }

    /// No pane: a workbook is drawn in a tab. What draws if the window asks anyway is the list of books.
    fn pane(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::spreadsheet::books_list(self, ui, look)
    }

    fn tab_keyed(&mut self, key: &str, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        self.current = Some(key.to_owned());
        crate::components::spreadsheet::show(self, key, ui, look)
    }

    fn modified(&self, key: &str) -> bool {
        self.books.iter().any(|book| book.key == key && book.session.modified)
    }

    fn tab_status(&self, key: &str) -> Option<crate::services::plugin_ui::TabStatus> {
        self.books.iter().find(|book| book.key == key).map(status::of)
    }

    fn save(&mut self, key: &str) -> Result<String, String> {
        let book = self.book_for_key(key)?;
        book.save()
    }

    fn closed(&mut self, key: &str) {
        self.books.retain(|book| book.key != key);
        if self.current.as_deref() == Some(key) {
            self.current = None;
        }
    }

    fn claims_key(&self, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        view::claims_key(key, modifiers)
    }

    fn settings(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::spreadsheet::settings_page(self, ui, look)
    }

    fn command(&mut self, command: &str, arguments: &[String]) -> Result<Answer, String> {
        commands::run(self, command, arguments)
    }

    fn commands(&self) -> Vec<(&'static str, &'static str)> {
        commands::LIST.to_vec()
    }

    fn view(&self) -> serde_json::Value {
        commands::view_value(self)
    }

    fn keyboard(&mut self, has_it: bool) {
        self.has_the_keyboard = has_it;
    }

    fn asking(&mut self) -> Vec<Request> {
        Spreadsheet::asking(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn close(&mut self) {
        self.books.clear();
        self.open = false;
    }
}
