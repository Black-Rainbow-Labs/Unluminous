//! The Word plugin's provider: every open document, its layout and its view.
//!
//! `task-2341`, and `tasks/task-2341-word-tdd.md` is the design. The document itself is
//! `unluminous_word`, which has no window in it; this is what holds one per tab, lays it out with the
//! window's fonts when it changed, and answers the commands the panel, the keys and `plugins run word`
//! all send. **A document is read when its tab is first drawn**, and dropped when its tab closes, so a
//! window with no Word document open holds nothing.

pub mod actions;
pub mod commands;
pub mod files;
pub mod fonts;
pub mod print;
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
    /// Autocorrect and AutoFormat as you type.
    pub autocorrect: unluminous_word::autocorrect::Rules,
    /// Chords a person moved: `chord.<action> = <chord>` lines, `none` taking a chord away.
    pub chords: Vec<(String, String)>,
    /// `paper = theme` draws the page in the theme's colours; `light`, the default, as white paper.
    pub dark_paper: bool,
    /// AutoSave: save a document that has a file a few seconds after its last change. Off by default.
    pub autosave: bool,
    /// Field shading a document opens with: `never`, `selected` or `always`.
    pub field_shading: &'static str,
    /// AutoRecover: write a copy of a changed document every ten minutes, offered after a crash. On by
    /// default.
    pub autorecover: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            author: whoami(),
            zoom: 1.0,
            marks: false,
            spelling: true,
            autocorrect: Default::default(),
            chords: Vec::new(),
            dark_paper: false,
            autosave: false,
            autorecover: true,
            field_shading: "selected",
        }
    }
}

/// The fields of one story at one revision: where each starts and ends.
pub type FieldRanges = (u64, StoryId, Vec<(Pos, Pos)>);

/// A field shading setting by name: `never`, `selected` (or `when-selected`) or `always`.
pub fn shading_kind(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "never" | "off" => Some("never"),
        "selected" | "when-selected" | "when selected" => Some("selected"),
        "always" | "on" => Some("always"),
        _ => None,
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
                "paper" => config.dark_paper = value == "theme",
                "autosave" => config.autosave = value == "true",
                "field_shading" => config.field_shading = shading_kind(value).unwrap_or("selected"),
                "autorecover" => config.autorecover = value != "false",
                // The rules switched off, by name.
                "autocorrect.off" => {
                    for name in value.split(',').map(str::trim) {
                        if let Some(slot) = config.autocorrect.slot(name) {
                            *slot = false;
                        }
                    }
                }
                key if key.trim().starts_with("chord.") => {
                    let action = key.trim().trim_start_matches("chord.").to_owned();
                    config.chords.push((action, value.to_owned()));
                }
                _ => {}
            }
        }
        config
    }

    pub fn write(&self, folder: &Path) -> Result<(), String> {
        std::fs::create_dir_all(folder)
            .map_err(|e| format!("{} could not be made: {e}", folder.display()))?;
        let off: Vec<&str> = unluminous_word::autocorrect::RULES
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| self.autocorrect.get(name) == Some(false))
            .collect();
        let text = format!(
            "# The Word plugin's settings. The author is the name on comments and tracked changes.\nauthor = {}\nzoom = {}\nmarks = {}\nspelling = {}\nautocorrect.off = {}\npaper = {}\nautosave = {}\nautorecover = {}\nfield_shading = {}\n",
            self.author,
            self.zoom,
            self.marks,
            self.spelling,
            off.join(", "),
            if self.dark_paper { "theme" } else { "light" },
            self.autosave,
            self.autorecover,
            self.field_shading
        );
        let mut text = text;
        for (action, chord) in &self.chords {
            text.push_str(&format!("chord.{action} = {chord}\n"));
        }
        crate::services::store::write_atomically(&folder.join(Self::FILE), text.as_bytes())
            .map_err(|e| format!("{} could not be written: {e}", folder.join(Self::FILE).display()))
    }
}

/// How the document is shown: as pages, as screens of text to read, or as its outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Print,
    Read,
    Outline,
}

/// A tab's key and, for a second window onto a document (`word:<path>#2`), the window's number.
pub fn split_window_key(key: &str) -> (&str, Option<u32>) {
    match key.rsplit_once('#') {
        Some((base, n)) if n.parse::<u32>().is_ok() => (base, n.parse().ok()),
        _ => (key, None),
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
    /// Third levels, each opened from the level `parent` names (task-2359, TDD 5.5).
    Proofing,
    Contents,
    TableProperties,
    Picture,
    Print,
    PageSetup,
    ModifyStyle,
    Symbol,
    NoteOptions,
}

impl Level {
    /// The third levels, which the root level does not list.
    pub const DEEPER: &'static [Level] = &[
        Level::Proofing,
        Level::Contents,
        Level::TableProperties,
        Level::Picture,
        Level::Print,
        Level::PageSetup,
        Level::ModifyStyle,
        Level::Symbol,
        Level::NoteOptions,
    ];

    /// The level a third level is opened from.
    pub fn parent(self) -> Option<Level> {
        match self {
            Level::Proofing => Some(Level::Review),
            Level::Contents => Some(Level::References),
            Level::TableProperties | Level::Picture => Some(Level::Insert),
            Level::Print => Some(Level::Document),
            Level::PageSetup => Some(Level::Page),
            Level::ModifyStyle => Some(Level::Styles),
            Level::Symbol => Some(Level::Insert),
            Level::NoteOptions => Some(Level::References),
            _ => None,
        }
    }

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
            Level::Proofing => "Proofing",
            Level::Contents => "Table of contents",
            Level::TableProperties => "Table properties",
            Level::Picture => "Picture",
            Level::Print => "Print",
            Level::PageSetup => "Page setup",
            Level::ModifyStyle => "Modify style",
            Level::Symbol => "Symbol",
            Level::NoteOptions => "Note options",
        }
    }

    pub fn parse(name: &str) -> Option<Level> {
        std::iter::once(Level::Root)
            .chain(Level::ALL.iter().copied())
            .chain(Level::DEEPER.iter().copied())
            .find(|level| {
                level.name().eq_ignore_ascii_case(name)
                    || level.name().replace(' ', "-").eq_ignore_ascii_case(name)
            })
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
            Level::Proofing => "Autocorrect and AutoFormat as you type",
            Level::Contents => "Levels, page numbers, leader, links",
            Level::TableProperties => "Alignment, width, rows, cells, style, borders",
            Level::Picture => "Size, wrap, position, crop, alt text, order",
            Level::Print => "Pages, copies, odd or even, pages per sheet, preview",
            Level::PageSetup => "Custom margins, paper, line numbers, borders, page numbers",
            Level::ModifyStyle => "Font, paragraph, based on, next style",
            Level::Symbol => "Every character of a block, recent ones, special characters",
            Level::NoteOptions => "Number format, restart, where notes go",
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
    /// Field codes drawn instead of their results (Alt+F9).
    pub codes: bool,
    /// Display for Review: how tracked changes are drawn.
    pub markup: unluminous_word::review::Markup,
    /// Show Markup: the comment cards beside the pages.
    pub show_comments: bool,
    /// The word count includes footnotes, endnotes and text boxes.
    pub count_notes: bool,
    /// The text of a paste waiting for the window to say what else the clipboard holds.
    pub paste_waiting: Option<(String, unluminous_word::clipboard::PasteAs)>,
    /// The last paste, for Paste Options: what was pasted, its text, where it started and where the
    /// caret was left, and how it was pasted.
    pub pasted: Option<Pasted>,
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
    /// Which tab the navigation pane shows: 0 headings, 1 pages, 2 the results of the find bar.
    pub navigation_tab: u8,
    /// The last formatting or insertion command, which F4 repeats.
    pub last_action: Option<&'static str>,
    /// The Go To box (Ctrl+G), with what has been typed in it: `page 3`, `heading 2`, `bookmark Intro`.
    pub goto: Option<String>,
    /// The x the caret keeps while it moves up and down.
    pub goal_x: Option<f32>,
    /// Scroll the caret into view on the next frame.
    pub reveal: bool,
    /// A press on the pages is selecting.
    pub dragging: bool,
    /// A press inside the selection is carrying it: on release it moves to where the pointer is, or is
    /// copied there with Ctrl held, as Word's drag and drop of text does (task-2359, M20).
    pub carrying: bool,
    /// The last places an edit was made, oldest first, for Go Back (Shift+F5).
    pub edits: Vec<Pos>,
    /// F8's extend mode: 0 off, then how many times F8 was pressed.
    pub extend: u8,
    /// How tall the pages' area was when it was last drawn, in screen points.
    pub viewport_height: f32,
    /// Print Layout, Read Mode or Outline view.
    pub mode: Mode,
    /// The ruler over the pages.
    pub ruler: bool,
    /// The kind of tab stop a press on the ruler adds.
    pub ruler_tab: unluminous_word::props::TabKind,
    /// A drag on the ruler, while it lasts.
    pub ruler_drag: Option<crate::components::word::views::RulerDrag>,
    /// Pages side by side, as many as fit across.
    pub multiple: bool,
    /// The drawing grid over the text.
    pub gridlines: bool,
    /// Field shading, Word's grey behind a field: `never`, `selected` (Word's default: while the caret or
    /// the selection is in it) or `always`.
    pub field_shading: &'static str,
    /// The fields of the story the shading was last worked out for, at a revision.
    pub field_ranges: Option<FieldRanges>,
    /// The page drawn in the theme's colours rather than as white paper.
    pub dark: bool,
    /// Outline view: the deepest level shown, 10 for body text too.
    pub outline_level: u8,
    /// Outline view: the headings folded shut, by path.
    pub outline_collapsed: std::collections::HashSet<Vec<u32>>,
    /// Outline view: body text as its first line only.
    pub first_line_only: bool,
    /// Read Mode: the screen's size in points, which the text is laid out at.
    pub read_size: Option<(f32, f32)>,
    /// How wide the pages' area was when it was last drawn, in screen points, for Fit Width.
    pub pane_width: f32,
    /// New Window was asked for; the tab asks the window for the second tab.
    pub new_window: bool,
    /// A table column's edge being dragged: the page, where the edge was and is in page points, and
    /// which table and grid column it ends.
    pub column_drag: Option<crate::components::word::objects::ColumnDrag>,
    /// Where the pages were drawn on the last frame, in the window's points.
    pub pages_rect: egui::Rect,
    /// The window's interface zoom on the last frame, which turns egui's points into the window's own.
    pub window_zoom: f32,
    /// Where the ruler drew its first line, hanging and right indent marks, and its band's top and
    /// bottom, on the last frame.
    pub ruler_marks: Option<[f32; 5]>,
    /// The style the Modify Style level is changing.
    pub modify_style: Option<String>,
    /// The symbols inserted lately, newest first.
    pub recent_symbols: Vec<char>,
    /// The right click menu, while it is open.
    pub context_menu: Option<ContextMenu>,
    /// Shift+F10 asked for the menu at the caret; the page view opens it, because it knows where that is.
    pub menu_at_caret: bool,
    /// A picture, shape or text box being moved, resized or cropped by its handles.
    pub object_drag: Option<crate::components::word::objects::ObjectDrag>,
    /// The selected picture's handles crop it (the Crop action) rather than resize it.
    pub cropping: bool,
    /// The last press, for double and triple clicks: when, where, how many.
    pub clicks: (f64, egui::Pos2, u32),
    /// What the panel's fields hold while they are typed in.
    pub fields: std::collections::HashMap<&'static str, String>,
    /// Copy Formatting: the run properties to paste.
    pub copied_format: Option<unluminous_word::xml::El>,
    /// The picture textures, by package part.
    pub pictures: std::collections::HashMap<String, Option<egui::TextureHandle>>,
}

/// The right click menu: where it opened, what it offers, and what was under the pointer.
#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub at: egui::Pos2,
    pub entries: Vec<MenuEntry>,
    /// The word under the pointer, which a synonym replaces.
    pub word: String,
    pub pos: Pos,
}

/// One row of the right click menu: its words and the action id it runs. A row of `-` is a line, and a
/// row with no id is a heading.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuEntry {
    pub label: String,
    pub id: String,
}

/// The last paste, which Paste Options can paste again another way.
#[derive(Debug, Clone)]
pub struct Pasted {
    pub paragraphs: Vec<unluminous_word::model::Paragraph>,
    pub text: String,
    pub at: Pos,
    pub end: Pos,
    pub how: unluminous_word::clipboard::PasteAs,
}

/// Remember a paste that was just made from `paragraphs`, starting at `at`, for Paste Options.
pub fn remember_the_paste(
    doc: &mut Doc,
    paragraphs: Vec<unluminous_word::model::Paragraph>,
    text: String,
    at: Pos,
) {
    let end = doc.session.selection.head.clone();
    doc.view.pasted = Some(Pasted {
        paragraphs,
        text,
        at,
        end,
        how: unluminous_word::clipboard::PasteAs::Source,
    });
}

/// A paste of something another program copied: ask the window for the clipboard's HTML and RTF, and
/// paste when it answers ([`Word::answered`]).
pub fn ask_for_the_clipboard(doc: &mut Doc, text: String, requests: &mut Vec<Request>) {
    doc.view.paste_waiting = Some((text, unluminous_word::clipboard::PasteAs::Source));
    requests.push(Request::ClipboardRich { id: format!("word-paste:{}", doc.key) });
}

/// Insert the text of a file at the caret, as Insert's Text from File does: a Word document's paragraphs
/// with their formatting, a Markdown file converted, an HTML file read as the clipboard reads it, and
/// anything else as plain text.
pub fn insert_text_from_file(doc: &mut Doc, path: &Path) -> Result<String, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("{} could not be read: {e}", path.display()))?;
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let paragraphs: Vec<unluminous_word::model::Paragraph> = match extension.as_str() {
        "docx" | "dotx" | "docm" | "dotm" => {
            let other = unluminous_word::read::open(&bytes)?;
            for (_, paragraph) in unluminous_word::position::paragraphs(&other.body) {
                if let Some(id) = paragraph.props.val_of("w:pStyle") {
                    let _ = unluminous_word::builtin::ensure_style(&mut doc.session.document, id);
                }
            }
            unluminous_word::position::paragraphs(&other.body)
                .into_iter()
                .map(|(_, p)| p.clone())
                .collect()
        }
        "md" | "markdown" => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            unluminous_word::import::from_markdown(&mut doc.session.document, &text)
        }
        "html" | "htm" => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            unluminous_word::import::from_html(&mut doc.session.document, &text)
        }
        _ => unluminous_word::import::from_text(&String::from_utf8_lossy(&bytes)),
    };
    let count = paragraphs.len();
    doc.session.paste_fragment(paragraphs);
    doc.view.reveal = true;
    Ok(format!("Inserted {count} paragraphs from {}", path.display()))
}

/// Paste a picture from the clipboard as an inline PNG, its own size at 96 pixels an inch and no wider
/// than the text column.
pub fn paste_a_picture(doc: &mut Doc, png: &[u8]) {
    let Some((w, h)) = unluminous_word::media::picture_size(png) else { return };
    let (mut width, mut height) = (w as f32 * 0.75, h as f32 * 0.75);
    let column = doc.pages.pages.first().map_or(468.0, |p| p.body.w);
    if width > column {
        height *= column / width;
        width = column;
    }
    let name = "Pasted picture";
    if unluminous_word::media::insert_picture(&mut doc.session, png, "png", width, height, name)
        .is_ok()
    {
        doc.view.reveal = true;
    }
}

/// Paste what the clipboard answered: its HTML, or its RTF, or the text, as `how` asks.
pub fn paste_from(
    doc: &mut Doc,
    html: Option<&str>,
    rtf: Option<&str>,
    text: &str,
    how: unluminous_word::clipboard::PasteAs,
) {
    use unluminous_word::clipboard;
    let paragraphs = match (html, rtf) {
        (Some(html), _) if !html.trim().is_empty() => {
            clipboard::from_html(&mut doc.session.document, html)
        }
        (_, Some(rtf)) if rtf.starts_with("{\\rtf") => clipboard::from_rtf(rtf),
        _ => text
            .split('\n')
            .map(|line| {
                let mut p = unluminous_word::model::Paragraph::new();
                if !line.is_empty() {
                    p.runs.push(unluminous_word::model::Run::text(
                        line,
                        unluminous_word::model::no_props(),
                    ));
                }
                p
            })
            .collect(),
    };
    let at = doc.session.selection.ordered().0.clone();
    if how == clipboard::PasteAs::Text || paragraphs.is_empty() {
        doc.session.type_text(text);
    } else {
        doc.session.paste_fragment(clipboard::apply_paste_as(paragraphs.clone(), how));
    }
    let end = doc.session.selection.head.clone();
    doc.view.pasted = Some(Pasted { paragraphs, text: text.to_owned(), at, end, how });
    doc.view.reveal = true;
}

/// Paste Options: undo the last paste and make it again `how`.
pub fn paste_again(doc: &mut Doc, how: unluminous_word::clipboard::PasteAs) -> Result<(), String> {
    use unluminous_word::clipboard;
    let pasted = doc.view.pasted.clone().ok_or("there is no paste to change")?;
    if pasted.how == how {
        return Ok(());
    }
    doc.session.undo();
    doc.session.select(pasted.at.clone(), pasted.at.clone());
    if how == clipboard::PasteAs::Text {
        doc.session.type_text(&pasted.text);
    } else {
        doc.session.paste_fragment(clipboard::apply_paste_as(pasted.paragraphs.clone(), how));
    }
    let end = doc.session.selection.head.clone();
    doc.view.pasted = Some(Pasted { end, how, ..pasted });
    Ok(())
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
    /// Word's wildcard language (`?`, `*`, `[a-z]`, `(...)` and `\1`) instead of plain text.
    pub wildcards: bool,
    /// Search the headers, footers, notes, comments and text boxes too.
    pub every_story: bool,
}

impl Find {
    /// The search the bar's fields and toggles describe.
    pub fn search(&self) -> unluminous_word::search::Search {
        unluminous_word::search::Search {
            query: unluminous_word::find::Query {
                text: self.needle.clone(),
                match_case: self.case,
                whole_word: self.word,
                wildcards: self.wildcards,
            },
            format: Default::default(),
            every_story: self.every_story,
        }
    }
}

impl View {
    fn new(config: &Config) -> View {
        View {
            zoom: config.zoom,
            scroll: 0.0,
            marks: config.marks,
            codes: false,
            markup: unluminous_word::review::Markup::All,
            show_comments: true,
            count_notes: false,
            paste_waiting: None,
            pasted: None,
            spelling: config.spelling,
            spell_menu: None,
            navigation: false,
            panel: Vec::new(),
            slash: None,
            palette: None,
            find: None,
            goal_x: None,
            navigation_tab: 0,
            last_action: None,
            goto: None,
            reveal: true,
            dragging: false,
            carrying: false,
            object_drag: None,
            context_menu: None,
            modify_style: None,
            pages_rect: egui::Rect::NOTHING,
            window_zoom: 1.0,
            ruler_marks: None,
            column_drag: None,
            recent_symbols: Vec::new(),
            mode: Mode::Print,
            ruler: false,
            ruler_tab: unluminous_word::props::TabKind::Left,
            ruler_drag: None,
            multiple: false,
            gridlines: false,
            field_shading: config.field_shading,
            field_ranges: None,
            dark: false,
            outline_level: 10,
            outline_collapsed: Default::default(),
            first_line_only: false,
            read_size: None,
            pane_width: 800.0,
            new_window: false,
            edits: Vec::new(),
            extend: 0,
            viewport_height: 600.0,
            menu_at_caret: false,
            cropping: false,
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
    /// The other windows onto this document (New Window): each tab's key and how it looks at the pages.
    pub windows: Vec<(String, View)>,
    /// The Read Mode screen the pages were last laid out on, in whole points.
    pub laid_reading: Option<(i32, i32)>,
    /// The paragraphs the last layout laid out, which a keystroke's layout takes again.
    pub layout_cache: std::cell::RefCell<layout::LayoutCache>,
    /// The file this document was opened from when it is not a `.docx` (a `.doc`, `.rtf`, `.txt` or `.md`
    /// opened as a document): Save then asks for a `.docx` name. `path` is `None` for such a document.
    pub converted_from: Option<PathBuf>,
    /// The password an encrypted document was opened with, or Encrypt with Password set: saving encrypts
    /// with it.
    pub password: Option<String>,
    /// When it last changed and was last copied, for AutoSave and AutoRecover.
    pub timing: files::Timing,
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
    /// A document just opened or made, with nothing laid out yet.
    fn new(
        key: String,
        path: Option<PathBuf>,
        session: Session,
        config: &Config,
        saved: u64,
    ) -> Doc {
        Doc {
            key,
            path,
            session,
            pages: Pages::default(),
            laid: 0,
            windows: Vec::new(),
            laid_reading: None,
            layout_cache: Default::default(),
            view: View::new(config),
            saved,
            formatted_text: None,
            drawn: 0,
            keyboard: (false, false),
            converted_from: None,
            password: None,
            timing: Default::default(),
        }
    }

    /// Lay the document out again if it changed since it was last laid out.
    pub fn lay_out(&mut self, renderer: &crate::services::text_renderer::TextRenderer) {
        let reading = (self.view.mode == Mode::Read).then_some(self.view.read_size).flatten();
        let reading_key = reading.map(|(w, h)| (w.round() as i32, h.round() as i32));
        if self.laid == self.session.revision
            && !self.pages.pages.is_empty()
            && reading_key == self.laid_reading
        {
            return;
        }
        self.laid_reading = reading_key;
        let fonts = fonts::WordFonts::new(renderer);
        self.pages = layout::lay_out_cached(
            &self.session.document,
            &fonts,
            layout::Options {
                show_marks: self.view.marks,
                show_codes: self.view.codes,
                markup: self.view.markup,
                reading,
            },
            &self.layout_cache,
            1,
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
        match self.path.as_ref().or(self.converted_from.as_ref()) {
            Some(path) => files::name_of(path),
            None => self.key.trim_start_matches("word:").to_owned(),
        }
    }

    /// Write the document to its file, or to `to`. A `.docx`, `.dotx`, `.docm` or `.dotm` becomes the
    /// document's file; `.rtf` and `.odt` (through Word) and `.txt`, `.html`, `.md` and `.pdf` are copies,
    /// and the document keeps the file it had.
    pub fn save(&mut self, to: Option<&Path>) -> Result<PathBuf, String> {
        let path = to.map(Path::to_path_buf).or_else(|| self.path.clone()).ok_or_else(|| {
            match &self.converted_from {
                Some(from) => format!(
                    "{} was opened from another format, so it needs a .docx name: use Save As",
                    files::name_of(from)
                ),
                None => {
                    "this document has never been saved, so it needs a name: use Save As".to_owned()
                }
            }
        })?;
        let kind = path.extension().and_then(|e| e.to_str()).unwrap_or("docx").to_ascii_lowercase();
        if !matches!(kind.as_str(), "docx" | "dotx" | "docm" | "dotm") {
            return self.save_a_copy(&path, &kind);
        }
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
        // A document, a template or either with macros, by the extension it is saved with.
        let extension =
            path.extension().and_then(|e| e.to_str()).unwrap_or("docx").to_ascii_lowercase();
        unluminous_word::write::prepare_for(&mut self.session.document, &extension);
        let mut bytes = unluminous_word::write::save(&self.session.document)?;
        if let Some(password) = self.password.as_deref().filter(|p| !p.is_empty()) {
            bytes = unluminous_word::crypto::encrypt(&bytes, password)?;
        }
        crate::services::store::write_atomically(&path, &bytes)
            .map_err(|e| format!("{} could not be written: {e}", path.display()))?;
        self.path = Some(path.clone());
        self.converted_from = None;
        self.session.file = Some(path.clone());
        self.saved = self.session.revision;
        self.session.modified = false;
        Ok(path)
    }

    /// Save As a format that is not Word's own: `.rtf` and `.odt` through Word, `.txt`, `.html`, `.md` and
    /// `.pdf` by Unluminous. The document keeps its own file and stays modified if it was.
    fn save_a_copy(&mut self, path: &Path, kind: &str) -> Result<PathBuf, String> {
        if unluminous_word::convert::saved_through_word(kind) {
            let mut document = self.session.document.clone();
            unluminous_word::write::prepare_for(&mut document, "docx");
            let bytes = unluminous_word::write::save(&document)?;
            files::save_through_word(&bytes, path, kind)?;
            return Ok(path.to_path_buf());
        }
        match kind {
            "txt" | "html" | "htm" | "md" | "pdf" => {
                let kind = if kind == "htm" { "html" } else { kind };
                commands::export_with(self, kind, Some(path), commands::PdfHow { tagged: true, outline: true, pdfa: false })?;
                Ok(path.to_path_buf())
            }
            other => Err(format!(".{other} is not a format a Word document is saved as: use .docx, .dotx, .docm, .dotm, .rtf, .odt, .pdf, .html, .md or .txt")),
        }
    }
}

/// The kinds of file a Word document is saved as, for the Save As dialog, each with its extensions.
pub const SAVE_KINDS: &[(&str, &[&str])] = &[
    ("Word document", &["docx"]),
    ("Word template", &["dotx"]),
    ("Word macro-enabled document", &["docm"]),
    ("Word macro-enabled template", &["dotm"]),
    ("PDF", &["pdf"]),
    ("Rich Text Format (needs Word)", &["rtf"]),
    ("OpenDocument Text (needs Word)", &["odt"]),
    ("Web page", &["html"]),
    ("Markdown", &["md"]),
    ("Plain text", &["txt"]),
];

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
    /// Tabs whose document is not ready: a conversion, a password, a failure.
    pub waiting: Vec<(String, files::Waiting)>,
    /// The password field of a tab waiting for one.
    pub password_field: String,
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
        if !self.waiting.iter().any(|(k, _)| k == key) {
            let path = PathBuf::from(
                key.strip_prefix("word:").ok_or_else(|| format!("{key} is not a Word tab"))?,
            );
            self.load(key, &path)?;
        }
        self.finish_waiting(key);
        if let Some((_, waiting)) = self.waiting.iter().find(|(k, _)| k == key) {
            return Err(waiting.sentence());
        }
        self.docs.iter_mut().find(|doc| doc.key == key).ok_or_else(|| format!("{key} is not open"))
    }

    /// A conversion that has finished becomes its document.
    fn finish_waiting(&mut self, key: &str) {
        let Some(index) = self.waiting.iter().position(|(k, _)| k == key) else { return };
        let files::Waiting::Converting { path, answer } = &self.waiting[index].1 else { return };
        let path = path.clone();
        let result = match answer.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("Word stopped before it finished".into())
            }
        };
        self.waiting.remove(index);
        let opened = result.and_then(|bytes| unluminous_word::read::open(&bytes));
        match opened {
            Ok(document) => {
                self.push_doc(key, document, None, Some(path));
                self.outbox.push(Request::Message(format!(
                    "Word converted {}. Save asks for a .docx name.",
                    files::name_of(
                        self.docs
                            .last()
                            .and_then(|d| d.converted_from.as_deref())
                            .unwrap_or(Path::new(""))
                    )
                )));
            }
            Err(problem) => {
                self.waiting.push((key.to_owned(), files::Waiting::Failed { path, problem }))
            }
        }
    }

    /// Give an encrypted document waiting under `key` its password.
    pub fn give_password(&mut self, key: &str, password: &str) -> Result<(), String> {
        let index = self
            .waiting
            .iter()
            .position(|(k, w)| k == key && matches!(w, files::Waiting::Password { .. }))
            .ok_or("no document is waiting for a password")?;
        let files::Waiting::Password { path, bytes, problem } = &mut self.waiting[index].1 else {
            unreachable!()
        };
        let opened = unluminous_word::crypto::decrypt(bytes, password)
            .and_then(|b| unluminous_word::read::open(&b));
        match opened {
            Ok(document) => {
                let path = path.clone();
                self.waiting.remove(index);
                self.push_doc(key, document, Some(path), None);
                if let Some(doc) = self.docs.last_mut() {
                    doc.password = Some(password.to_owned());
                }
                self.password_field.clear();
                Ok(())
            }
            Err(_) => {
                *problem = Some("that password is not right".into());
                Err("that password is not right".into())
            }
        }
    }

    /// Add a document read or converted under `key`: `path` is the file it saves to, `converted_from` the
    /// file it came from when that is not a Word document.
    fn push_doc(
        &mut self,
        key: &str,
        mut document: unluminous_word::model::Document,
        path: Option<PathBuf>,
        converted_from: Option<PathBuf>,
    ) {
        // A template opens as a new document made from it, which is what double clicking one does.
        let template = path
            .as_ref()
            .and_then(|p| p.extension())
            .is_some_and(|e| e.eq_ignore_ascii_case("dotx") || e.eq_ignore_ascii_case("dotm"));
        if template {
            unluminous_word::new::set_property(&mut document, "dc:creator", "");
        }
        let path = if template { None } else { path };
        let mut session = Session::new(document);
        session.author = self.config.author.clone();
        session.file = path.clone();
        session.autocorrect_rules = self.config.autocorrect.clone();
        let mut doc = Doc::new(key.to_owned(), path, session, &self.config, 1);
        doc.converted_from = converted_from;
        if let Some(folder) = &self.folder {
            doc.timing.offer = files::recovery_for(folder, key, doc.path.as_deref())
                .map(|(copy, at)| (copy, files::when(at)));
        }
        self.docs.push(doc);
        self.current = Some(key.to_owned());
    }

    /// Read a document from disk into a new `Doc`.
    pub fn load(&mut self, key: &str, path: &Path) -> Result<(), String> {
        if self.docs.iter().any(|doc| doc.key == key) {
            return Ok(());
        }
        self.waiting.retain(|(k, _)| k != key);
        let extension =
            path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        // A .doc, .rtf, .odt, .wps or .pdf is converted by Word, on a thread.
        if unluminous_word::convert::opened_through_word(&extension) {
            self.waiting.push((key.to_owned(), files::start_converting(path)));
            self.current = Some(key.to_owned());
            return Ok(());
        }
        let bytes = std::fs::read(path)
            .map_err(|e| format!("{} could not be read: {e}", path.display()))?;
        // Text, Markdown and HTML open as a document made from them.
        if matches!(extension.as_str(), "txt" | "md" | "markdown" | "html" | "htm") {
            let text = String::from_utf8_lossy(&bytes);
            let document = unluminous_word::import::document_from(&extension, &text);
            self.push_doc(key, document, None, Some(path.to_path_buf()));
            return Ok(());
        }
        if unluminous_word::crypto::is_encrypted(&bytes) {
            self.waiting.push((
                key.to_owned(),
                files::Waiting::Password { path: path.to_path_buf(), bytes, problem: None },
            ));
            self.current = Some(key.to_owned());
            return Ok(());
        }
        let document =
            unluminous_word::read::open(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        self.push_doc(key, document, Some(path.to_path_buf()), None);
        Ok(())
    }

    /// A new, empty document, untitled until it is saved, or at `path` when one is given.
    pub fn new_document(&mut self, path: Option<&Path>) -> Result<String, String> {
        self.new_from(path, None)
    }

    /// A new document made from `template` (a `.dotx` or `.dotm`), or from Word's `Normal.dotm` when
    /// there is one and no template is named, or the shipped blank with the locale's page size.
    pub fn new_from(
        &mut self,
        path: Option<&Path>,
        template: Option<&Path>,
    ) -> Result<String, String> {
        let normal = files::office_template();
        let from = template.map(Path::to_path_buf).or(normal);
        let document = match &from {
            Some(template) => {
                let bytes = std::fs::read(template)
                    .map_err(|e| format!("{} could not be read: {e}", template.display()))?;
                let mut document = unluminous_word::read::open(&bytes)
                    .map_err(|e| format!("{}: {e}", template.display()))?;
                // The new document is a document, not the template, and keeps no macros.
                unluminous_word::write::prepare_for(&mut document, "docx");
                for name in
                    ["dc:title", "dc:subject", "cp:keywords", "dc:description", "cp:lastModifiedBy"]
                {
                    if unluminous_word::new::property(&document, name).is_some() {
                        unluminous_word::new::set_property(&mut document, name, "");
                    }
                }
                document
            }
            None => unluminous_word::new::blank(),
        };
        let mut session = Session::new(document);
        if from.is_none() {
            if let Some(locale) = files::machine_locale() {
                let _ = session.set_page_size(files::locale_page_size(&locale));
                session.modified = false;
            }
        }
        session.author = self.config.author.clone();
        session.autocorrect_rules = self.config.autocorrect.clone();
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
        let mut doc = Doc::new(key.clone(), path.map(Path::to_path_buf), session, &self.config, 0);
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
            crate::components::word::keys::set_chords(&self.config.chords);
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
        // A second window onto a document is its own tab with its own view of the same pages.
        let (base, window) = split_window_key(key);
        match self.doc(base) {
            Ok(doc) => {
                let requests = match window {
                    Some(_) => {
                        let index = match doc.windows.iter().position(|(k, _)| k == key) {
                            Some(index) => index,
                            None => {
                                doc.windows.push((key.to_owned(), doc.view.clone()));
                                doc.windows.len() - 1
                            }
                        };
                        std::mem::swap(&mut doc.view, &mut doc.windows[index].1);
                        let requests = crate::components::word::show(doc, &config, ui, look);
                        std::mem::swap(&mut doc.view, &mut doc.windows[index].1);
                        requests
                    }
                    None => crate::components::word::show(doc, &config, ui, look),
                };
                // A rule switched in the Proofing level is every document's, and is written down.
                let rules = doc.session.autocorrect_rules.clone();
                if rules != self.config.autocorrect {
                    self.config.autocorrect = rules.clone();
                    for doc in &mut self.docs {
                        doc.session.autocorrect_rules = rules.clone();
                    }
                    if let Some(folder) = &self.folder {
                        let _ = self.config.write(folder);
                    }
                }
                requests
            }
            Err(problem) => self.draw_waiting(base, &problem, ui, look),
        }
    }

    /// The window's answer to a paste's [`Request::ClipboardRich`]: paste the HTML, the RTF or the text
    /// into the document that asked.
    fn answered(&mut self, id: &str, answer: Result<serde_json::Value, String>) {
        if let Some(key) = id.strip_prefix("word-paste-picture:") {
            let answer = answer.unwrap_or_default();
            let has_text = ["text", "html"]
                .iter()
                .any(|k| answer[*k].as_str().is_some_and(|t| !t.trim().is_empty()));
            let bytes =
                answer["picture"]["data"].as_str().and_then(unluminous_chat::base64::decode);
            if let (false, Some(bytes), Ok(doc)) = (has_text, bytes, self.doc(key)) {
                paste_a_picture(doc, &bytes);
            }
            return;
        }
        let Some(key) = id.strip_prefix("word-paste:") else { return };
        let Ok(doc) = self.doc(key) else { return };
        let Some((text, how)) = doc.view.paste_waiting.take() else { return };
        let answer = answer.unwrap_or_default();
        let html = answer["html"].as_str().map(str::to_owned);
        let rtf = answer["rtf"].as_str().map(str::to_owned);
        let text = answer["text"].as_str().map(str::to_owned).unwrap_or(text);
        paste_from(doc, html.as_deref(), rtf.as_deref(), &text, how);
    }

    fn modified(&self, key: &str) -> bool {
        let (key, _) = split_window_key(key);
        self.docs.iter().any(|doc| doc.key == key && doc.modified())
    }

    fn save(&mut self, key: &str) -> Result<String, String> {
        let (key, _) = split_window_key(key);
        // A document with no file yet asks where to go; one converted from another format offers its own
        // name as a .docx.
        if self.doc(key)?.path.is_none() {
            let converted = self.doc(key)?.converted_from.clone();
            let start = converted
                .as_ref()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .or_else(|| self.project.clone());
            let mut dialog = rfd::FileDialog::new()
                .set_title("Save Word Document")
                .set_directory(start.unwrap_or_default());
            if let Some(name) = converted.as_ref().and_then(|p| p.file_stem()) {
                dialog = dialog.set_file_name(format!("{}.docx", name.to_string_lossy()));
            }
            for (name, extensions) in SAVE_KINDS {
                dialog = dialog.add_filter(*name, extensions);
            }
            let chosen = dialog.save_file();
            let Some(chosen) = chosen else { return Err("Not saved".to_owned()) };
            let chosen =
                if chosen.extension().is_none() { chosen.with_extension("docx") } else { chosen };
            return self.save_as(key, &chosen);
        }
        let saved = self.doc(key)?.save(None)?;
        if let Some(folder) = &self.folder {
            files::forget_recovery(folder, key);
        }
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
        // Closing a second window leaves the document open in the first.
        if let (base, Some(_)) = split_window_key(key) {
            if let Some(doc) = self.docs.iter_mut().find(|d| d.key == base) {
                doc.windows.retain(|(k, _)| k != key);
            }
            return;
        }
        self.docs.retain(|doc| doc.key != key);
        self.waiting.retain(|(k, _)| k != key);
        // A document closed on purpose leaves no copy to recover.
        if let Some(folder) = &self.folder {
            files::forget_recovery(folder, key);
        }
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

    /// AutoSave, AutoRecover and conversions finishing, once a frame.
    fn catch_up(&mut self) -> bool {
        self.keep_the_files(std::time::Instant::now())
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
        StoryId::TextBox(id) => format!("textbox:{id}"),
    }
}
