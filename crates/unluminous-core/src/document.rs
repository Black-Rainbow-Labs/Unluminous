//! One open document: its text, its formatting, its caret and its undo history.
//!
//! Everything the editor does goes through `Document::apply`, which takes a `Command`. Keeping every
//! change behind one function is what makes undo and the stale layout flag reliable: there is one
//! place where a change is recorded and one place where the revision is bumped.

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::breakpoints::Breakpoints;
use crate::cursor::{self, Selection};
use crate::encoding::{Encoding, LineEnding};
use crate::folding::Folds;
use crate::highlights::{Highlights, Rgba};
use crate::incremental::Dirt;
use crate::layout::{Layout, Touched};
use crate::rope::Rope;
use crate::style::{
    Align, CharStyle, Color, ParagraphStyle, ParagraphStyles, StyleChange, StyleSpans,
};

/// Something the user asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Type text, replacing the selection if there is one. Also used for paste.
    Insert(String),
    /// Backspace: delete the selection, or the cluster before the caret.
    DeleteBackward,
    /// Delete forwards: delete the selection, or the cluster after the caret.
    DeleteForward,
    /// Delete the word before the caret.
    DeleteWordBackward,
    MoveLeft {
        extend: bool,
    },
    MoveRight {
        extend: bool,
    },
    MoveWordLeft {
        extend: bool,
    },
    MoveWordRight {
        extend: bool,
    },
    MoveLineStart {
        extend: bool,
    },
    MoveLineEnd {
        extend: bool,
    },
    MoveDocumentStart {
        extend: bool,
    },
    MoveDocumentEnd {
        extend: bool,
    },
    SelectAll,
    /// Put the caret at a document offset, from a mouse click.
    PlaceCaret {
        offset: usize,
        extend: bool,
    },
    /// Replace several ranges at once, as **one** undo step.
    ///
    /// This is what a rename is applied by. Undo in Unluminous restores a snapshot, so a whole
    /// document's rename being one step is not something this has to arrange — it follows from
    /// pushing one snapshot and then making every edit. The edits are applied back to front, so no
    /// range can shift one still to be made; `symbols::replacements` is what puts them in that
    /// order and drops the overlapping ones, and this trusts nothing it is given: a range outside
    /// the text or across a character is left alone rather than panicking.
    ReplaceMany(Vec<(Range<usize>, String)>),
    /// Indent each line the selection touches, by one tab or one space.
    ///
    /// This is what `Tab` and `Space` do over a selection: the selection is what makes the key an
    /// indent rather than a type. With no selection it indents the line the caret is on, so the
    /// command line can ask it about a document that has neither. The selection follows the text it
    /// covered, so pressing the key again indents the same lines again. `task-1747`.
    Indent {
        unit: IndentUnit,
    },
    /// Remove one indent from each line the selection touches, the reverse of `Indent`.
    ///
    /// This is what `Shift+Tab` and `Shift+Space` do over a selection. A line only loses a
    /// character when it starts with exactly the unit's own character; a line with no indentation,
    /// or one indented with the other unit, is left alone, because there is no tab width to fall
    /// back on and guessing at a mismatched indent would be wrong more often than right.
    Dedent {
        unit: IndentUnit,
    },
    /// Comment or uncomment every line the selection touches, with the language's own marker.
    ///
    /// **The marker is passed in**, because this crate holds no list of languages: the window reads
    /// `language.line_comment` off the plugin that claims the file and hands it down, which is the
    /// seam `folding::Reading` and `markdown::CodeHighlighter` already are.
    ///
    /// It **uncomments when every line the selection touches that has anything on it already begins
    /// with the marker**, and comments otherwise, which is what makes one key both. A wholly blank
    /// line is left out of that reckoning and is left alone, so a selection with a blank line in the
    /// middle of it comes back byte for byte when the key is pressed again. Exactly the marker is
    /// put in and exactly the marker is taken out — a space after it is part of the marker or it is
    /// not there, and that is what makes the toggle its own inverse.
    ///
    /// With no selection it is the line the caret is on, as [`Command::Indent`] is.
    ToggleLineComment {
        marker: String,
    },
    /// Wrap the selection in the language's block comment markers, or take them off again.
    ///
    /// The same decision [`Command::ToggleLineComment`] makes, made once over the whole range rather
    /// than once a line: the range is uncommented when what it holds, ignoring the whitespace at
    /// either end, opens with `open` and closes with `close`. The markers hug the words rather than
    /// the indentation, so a second press gives back the bytes the first one changed.
    ///
    /// With no selection the range is the line the caret is on, which is what a person pressing the
    /// chord on one line means.
    ToggleBlockComment {
        open: String,
        close: String,
    },
    /// Copy the lines the selection touches in below themselves.
    ///
    /// The selection stays on the lines it was on rather than moving to the copy, which is
    /// [`Command::Indent`]'s promise: pressing the key again duplicates the same lines again.
    DuplicateLines,
    /// Move the lines the selection touches up or down, carrying the selection with them.
    ///
    /// `by` is a direction and a count: `-1` and `1` are what the two key chords send. A block
    /// already against the top or the bottom of the file does not move.
    MoveLines {
        by: i32,
    },
    /// Join the lines the selection touches into one, with a single space at each seam.
    ///
    /// The joined line's leading whitespace goes, and so does the trailing whitespace of the line it
    /// is joined to — which is the only way "one space between" can be true of a line that ended in
    /// one. With no selection it joins the caret's line to the line below it, so the chord can be
    /// pressed again to pull up the next one.
    JoinLines,
    /// Sort the lines the selection touches, in byte order, keeping equal lines in the order they
    /// were in.
    ///
    /// Nothing is selected, nothing is sorted: one line is already in order, and a command that
    /// silently sorted the whole file would be a command nobody could undo in their head.
    SortLines,
    /// Take the trailing whitespace off every line in the document.
    ///
    /// **This does not know about saving.** Whether it runs when a file is written is the window's
    /// question, because the setting that answers it and the language it has to be withheld from —
    /// Markdown, where two trailing spaces are a line break — are both the window's knowledge.
    TrimTrailingWhitespace,
    /// Apply character formatting to the selection, or to the next text typed if nothing is selected.
    ApplyStyle(StyleChange),
    /// Turn bold on for the selection, or off if all of it is already bold.
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleStrikethrough,
    SetAlign(Align),
    SetLineSpacing(f32),
    Undo,
    Redo,
}

/// The character an indent is made of: the one the key that asked for it names.
///
/// A tab from `Tab`, a space from `Space`. Unluminous has no tab width and no "insert spaces for tabs"
/// preference, and the character the key says is the one answer that needs no setting — see
/// `tasks/task-1747-selection-indent-tdd.md` section 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentUnit {
    /// A tab, from the `Tab` key.
    Tab,
    /// A space, from the `Space` key.
    Space,
}

impl IndentUnit {
    /// The character it stands for.
    pub fn text(&self) -> &'static str {
        match self {
            IndentUnit::Tab => "\t",
            IndentUnit::Space => " ",
        }
    }
}

/// What kind of change the last command made, so that a run of typing collapses into one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
    None,
    Typing,
    Deleting,
    Other,
}

/// A whole document state, kept so that undo can restore it.
///
/// Undo restores a saved state rather than replaying an inverse operation. Replaying inverses is more
/// frugal with memory, but it has to reconstruct the formatting spans and the paragraph list that the
/// edit destroyed, and getting that wrong corrupts the document silently. Restoring a state cannot be
/// wrong. The cost is bounded by capping the history at `UNDO_LIMIT` states, and Unluminous opens plain
/// text files rather than very large ones.
#[derive(Debug, Clone)]
struct Snapshot {
    text: Rope,
    chars: StyleSpans,
    paragraphs: ParagraphStyles,
    selection: Selection,
    /// The identity of the persisted-content state this snapshot restores. Comparing it with the
    /// saved revision is what makes undoing back to the saved point clear the dirty marker.
    history_revision: u64,
    /// The marked passages, which ride the snapshot for the same reason everything else here does:
    /// undo restores a state, and the marks are part of the state the text was in. Undoing back past
    /// the moment a passage was marked therefore unmarks it, and redoing marks it again.
    highlights: Highlights,
    /// Which blocks are collapsed, for exactly the same reason. `task-1686`.
    folds: Folds,
    /// Where the program is to stop, for exactly the same reason again. `task-1687`.
    breakpoints: Breakpoints,
}

pub const UNDO_LIMIT: usize = 256;

/// How many text revisions [`Document::touched_since`] can answer about.
///
/// `task-1984` C7. A pane lays out on the frame after an edit, so one would nearly always do;
/// what needs more than one is a pane that was not showing, or a window that dropped frames, and
/// a pane further behind than this lays the whole document out once. Sixty four is four seconds
/// of sustained typing.
pub const TOUCHED_REVISIONS: usize = 64;

/// How much text either side of the caret the movement commands are given to look at.
///
/// **`task-1984` C5.** `line_window` used to hand over the whole line, and the grapheme and word
/// walks then read it from its start -- a few dozen bytes in ordinary source and the whole file in a
/// minified `.js`, a one line `.json` or a single log line, where one arrow key was measured at
/// 15.9 ms on a megabyte. Two hundred and fifty six bytes is far more than the longest grapheme
/// cluster Unicode defines and far more than any word, so every answer inside a line is unchanged;
/// what changes is that a line nobody writes by hand no longer costs its whole length per keystroke.
pub const CARET_WINDOW: usize = 256;

/// An open document.
#[derive(Debug, Clone)]
pub struct Document {
    text: Rope,
    chars: StyleSpans,
    paragraphs: ParagraphStyles,
    /// The passages somebody has marked with a colour. Sparse, unlike `chars`, and shifted by the
    /// same place that shifts `chars`: see `Self::splice`.
    highlights: Highlights,
    /// Which blocks are collapsed, held as the byte offset of each collapsed region's head line and
    /// shifted by `Self::splice`, for the same reason. What is *foldable* is derived from the text
    /// by `crate::folding::regions` and is not state; this is the half that is.
    folds: Folds,
    /// Where a debugger is to stop, held as the byte offset of each line's start and shifted by
    /// `Self::splice`, for the same reason: it is the only function in Unluminous that knows a range
    /// of bytes moved. See `crate::breakpoints`.
    breakpoints: Breakpoints,
    selection: Selection,
    /// Formatting chosen while nothing was selected, applied to the next text typed.
    pending: StyleChange,
    /// The horizontal position to aim for when moving up and down. Without this, moving down through a
    /// short line and on to a long one loses the original column.
    desired_x: Option<f32>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: EditKind,
    path: Option<PathBuf>,
    /// What this file's line breaks were on disk, so that writing it back does not change them.
    ///
    /// The text above holds line feeds and nothing else -- see `read_to_normalised_string` for why
    /// there is one meaning of an offset -- and this is what that normalisation is undone with in
    /// `save_as`.
    /// `task-1804` §7.1: without it, typing one character into a file with Windows line breaks
    /// rewrote every line ending in it.
    line_ending: LineEnding,
    /// What this file's characters were on disk. A file that is not UTF-8 is opened read-only rather
    /// than refused, and `save_as` will not write one back. `task-1804` §7.6.
    encoding: Encoding,
    /// Identity of the current persisted-content state. Unlike `text_revision`, this value is
    /// restored by undo rather than advanced, so returning to a saved state can be recognised.
    history_revision: u64,
    /// The history revision last opened from or successfully written to disk.
    saved_history_revision: u64,
    /// A fresh identity for the next edit. Revisions are never reused after a redo branch is
    /// discarded, so an unreachable saved point cannot be mistaken for a later state.
    next_history_revision: u64,
    /// Bumped on every change of any kind, including one that only moved the caret. Anything asking
    /// "did anything at all happen" reads this: whether the window needs painting again, whether the
    /// marked passages need writing out.
    revision: u64,
    /// Bumped only when the text or its formatting changed, so that what was laid out and what was
    /// coloured can be told apart from where the caret is.
    ///
    /// Moving the caret used to bump `revision`, and both the layout cache and the syntax colouring
    /// were keyed on `revision` — so every frame of dragging a selection re-tokenised the file and
    /// laid the whole document out again. See `tasks/task-1666-performance-tdd.md` section 2, and
    /// the test that says a layout which changed means this moved.
    text_revision: u64,
    /// What has changed since the syntax was last set, so the tokeniser can read the part that
    /// moved rather than the whole file after every keystroke.
    ///
    /// `task-1804` §5.2, which is `task-1666` §12's *"the next thing to become the
    /// largest item"* becoming it: at 2 MB a keystroke cost 73.6 ms, and nearly all of that was
    /// reading a file that had changed in one place. Noted only by `insert` and `remove_range`,
    /// which is ordinary typing and deleting; `indent`, `dedent` and `replace_many` call the same
    /// `Self::splice` that shifts the marks, the folds and the breakpoints, but do not note the
    /// dirt, so those three still colour the file as a whole rather than incrementally.
    syntax_dirt: Dirt,
    /// Which paragraphs the edit being applied right now has touched so far.
    ///
    /// `task-1984` C7. Widened by every path that moves a byte or repaints a colour, and folded into
    /// [`Self::touched`] by `text_changed` when the edit finishes. Several splices in one undo step --
    /// a rename, an indent, Replace All -- fold into one answer, which is what [`Touched::and`] is
    /// for and why it counts from the two ends rather than naming a range.
    /// `None` until something says, which is what makes saying nothing mean the whole document.
    touching: Option<Touched>,
    /// What each of the last few text revisions touched, newest last.
    ///
    /// A ring rather than one value, because more than one pane can be showing the same document and
    /// each asks about the revision **it** last laid out, which are different numbers. A pane that
    /// has fallen further behind than this holds is told [`Touched::whole`] and lays the document out
    /// the way every pane did before this existed, so the bound costs correctness nothing.
    touched: std::collections::VecDeque<(u64, Touched)>,
    /// Bumped only when a block is collapsed or expanded.
    ///
    /// The third counter, and it is the second one's argument made once more. Folding changes the
    /// layout and changes nothing else: keyed on `text_revision` it would re-colour the file and
    /// rebuild the Markdown preview for a fold, and keyed on `revision` the layout would be built
    /// again on every frame of a drag. So `refresh_layout` is keyed on the pair and nothing else
    /// reads this. See `tasks/task-1686-folding-tdd.md` section 5.1.
    fold_revision: u64,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

/// The text of a file as a [`Document`] holds it: Windows line breaks turned into line feeds.
///
/// **The one reading of a file, so an offset means the same byte everywhere.** A `Document` has
/// always normalised `\r\n` on the way in, so that "offsets and line counts have one meaning" — but
/// the window also reads files it has *not* opened, to turn a breakpoint's byte offset into the line
/// number the debugger is told about, and those readings were raw.
///
/// `task-1794`: on a file with Windows line breaks the two disagree by **one byte per line before
/// the offset**, so a breakpoint set on line 50 of a file that was open, and then sent to the adapter
/// while that file was shut, named a line about fifty bytes early — a different line, very often one
/// with no code on it, which an adapter declines to bind. Nothing reports that: the program runs to
/// completion and the debug tile stays empty. A `git checkout` on a machine with `core.autocrlf`
/// set — which is this one — is enough to put every file in that state.
///
/// So this is public and every reading of a file's own bytes goes through it.
pub fn read_to_normalised_string(path: &Path) -> std::io::Result<String> {
    Ok(read_file(path)?.text)
}

/// The same reading, keeping what the bytes were as well as what they say.
///
/// `Document::open` is this plus a caret, and everything else that reads a file's own bytes uses the
/// line above, so the two cannot drift: an offset means the same byte in both.
pub fn read_file(path: &Path) -> std::io::Result<crate::encoding::Decoded> {
    Ok(crate::encoding::decode(&std::fs::read(path)?))
}

/// The same rule applied to text that has already been read.
///
/// Every one of the three endings, not only Windows': `task-1804` §7.5 found that a lone carriage
/// return was not a line break anywhere in Unluminous, so a classic-Macintosh file opened as one line
/// however long it was.
pub fn normalise_line_breaks(text: &str) -> String {
    crate::encoding::normalise_line_breaks(text)
}

impl Document {
    pub fn new() -> Self {
        Self {
            text: Rope::new(),
            chars: StyleSpans::new(0, CharStyle::default()),
            paragraphs: ParagraphStyles::new(1),
            highlights: Highlights::new(),
            folds: Folds::new(),
            breakpoints: Breakpoints::new(),
            selection: Selection::caret(0),
            pending: StyleChange::default(),
            desired_x: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: EditKind::None,
            path: None,
            line_ending: LineEnding::platform_default(),
            encoding: Encoding::default(),
            history_revision: 1,
            saved_history_revision: 1,
            next_history_revision: 2,
            revision: 1,
            text_revision: 1,
            touching: None,
            touched: std::collections::VecDeque::new(),
            // A fresh document has nothing coloured, so the first reading is the whole of it.
            syntax_dirt: Dirt::Whole,
            fold_revision: 1,
        }
    }

    /// A document holding `text`, as if it had just been opened from disk.
    ///
    /// **Including its line ending**, because "as if it had just been opened" is what the sentence
    /// above promises and a document built this way is otherwise written back with the platform's
    /// own -- which on Windows turns a body of text with line feeds in it into a file with carriage
    /// returns the first time it is saved. Text with no line break in it at all gets the platform's,
    /// because there is nothing to keep.
    ///
    /// `Document::open` passes text that has already been normalised and then sets the real answer
    /// over the top, so the two do not disagree.
    pub fn from_text(text: &str) -> Self {
        let rope = Rope::from_str(text);
        Self {
            chars: StyleSpans::new(rope.len_bytes(), CharStyle::default()),
            paragraphs: ParagraphStyles::new(rope.len_lines()),
            text: rope,
            line_ending: LineEnding::dominant_in(text),
            ..Self::new()
        }
    }

    /// An empty document that names a file it did not read.
    ///
    /// A picture opens in a tab like any other file, and a tab holds a document, but there is no text
    /// in a picture to hold. So the tab holds this: it carries the path, so the tab is named after the
    /// file and the explorer marks the row as open, and it holds nothing else. It is never modified and
    /// so is never written, and `unluminous_app` refuses to save a tab showing a picture in any case.
    pub fn at_path(path: &Path) -> Self {
        Self { path: Some(path.to_owned()), ..Self::new() }
    }

    /// The largest file this crate will open, which is what a document offset can spell.
    ///
    /// `task-1922` B18. A `PlacedCluster` holds its byte range as two `u32`, with an `expect` on the
    /// conversion, and `Document::open` had no size guard of its own -- so a file past four gigabytes
    /// was read, kept, and then panicked in layout, which is after the window has committed to
    /// showing it. `unluminous_app::services::file_kind` refuses a file over 16 MB before it is ever
    /// opened in a tab, and that is the limit a person meets; this is the one underneath it, for the
    /// callers that do not go through `file_kind` at all.
    pub const LARGEST: u64 = u32::MAX as u64;

    /// Open the file at `path`, keeping what its bytes were so that saving it does not change them.
    ///
    /// A file that is not UTF-8 opens **read-only** rather than being refused: see [`Encoding`] for
    /// which shapes are read that way and why none of them is written back. A file larger than
    /// [`Document::LARGEST`] is refused here rather than panicking later, which is what it did.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        if let Ok(about) = std::fs::metadata(path) {
            if about.len() > Self::LARGEST {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "{} is {} bytes, and Unluminous reads a file of at most {} bytes",
                        path.display(),
                        about.len(),
                        Self::LARGEST
                    ),
                ));
            }
        }
        let read = read_file(path)?;
        let mut document = Self::from_text(&read.text);
        document.path = Some(path.to_owned());
        document.line_ending = read.line_ending;
        document.encoding = read.encoding;
        Ok(document)
    }

    /// What this file's line breaks were on disk, and what saving it will write.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Write this file with `ending` from now on. What a Settings key and `editor line-ending` set.
    ///
    /// It does not touch the text -- the text holds one kind of line break and only one -- so it is
    /// not an edit and does not go through `apply`. It does make the document unsaved, because what
    /// is on disk and what saving would write are no longer the same thing, and a person who chose an
    /// ending and saw nothing to save would reasonably think it had not been taken.
    pub fn set_line_ending(&mut self, ending: LineEnding) {
        if self.line_ending == ending {
            return;
        }
        self.line_ending = ending;
        self.history_revision = self.next_history_revision;
        self.next_history_revision += 1;
        self.revision += 1;
    }

    /// What this file's characters were on disk.
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// Whether this document can be written back at all.
    ///
    /// False for a file opened in an encoding Unluminous reads and does not write. The window asks
    /// this before it offers Save, rather than letting the write fail at the end of the person's
    /// work. See [`Encoding::writable`].
    pub fn writable(&self) -> bool {
        self.encoding.writable()
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| std::io::Error::other("this document has no file to save to"))?;
        self.save_as(&path)
    }

    /// Write the current text to `path` and make this history revision the saved point.
    ///
    /// **As the file was read**: its own line breaks, its own byte order mark, and a refusal rather
    /// than a re-encoding for anything this version only reads. `task-1804` §7.1 measured what the
    /// first of those is worth -- one character typed into a file with Windows line breaks used to
    /// rewrite every line ending in it, which on a checkout with `core.autocrlf` set is every file.
    /// Write `bytes` to `path` through a temporary in the same folder and a rename.
    ///
    /// **`task-1984` A10, and the one place this crate has to answer it itself.** `std::fs::write`
    /// truncates the file and then fills it, so a crash, a power cut or a full disk between those two
    /// leaves a person's source file at zero length. `services::store::write_atomically` is where
    /// this is written down for the files Unluminous remembers itself in, and `unluminous-core`
    /// cannot depend on the app crate -- the dependency points the other way -- so the shape is
    /// repeated here rather than the dependency being turned round for one function.
    ///
    /// The temporary is in the **same folder** as the target, because a rename is only atomic within
    /// one file system. Its name carries the process id, so two Unluminous windows saving the same
    /// file cannot take each other's temporary. `sync_all` before the rename is what makes the bytes
    /// really be on the disk rather than in the operating system's cache when the rename makes them
    /// visible.
    fn write_through_a_temporary(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write as _;

        let folder = path.parent().unwrap_or_else(|| Path::new("."));
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned());
        let name = name.unwrap_or_else(|| "unluminous".to_owned());
        // The process id and a number no other write in this process is using. Two Unluminous
        // windows are two processes, which is what the first half is for; two threads saving the
        // same file name in one process share it, which is what the second half is for.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let ticket = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary = folder.join(format!("{name}.tmp-{}-{ticket}", std::process::id()));

        let written = (|| -> std::io::Result<()> {
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()
        })();
        if let Err(problem) = written {
            let _ = std::fs::remove_file(&temporary);
            return Err(problem);
        }
        if let Err(problem) = std::fs::rename(&temporary, path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(problem);
        }
        Ok(())
    }

    pub fn save_as(&mut self, path: &Path) -> std::io::Result<()> {
        if !self.encoding.writable() {
            return Err(std::io::Error::other(format!(
                "this file was read as {} and Unluminous does not write that encoding",
                self.encoding.name()
            )));
        }
        let normalised = self.text.to_string();
        let text = self.line_ending.apply(&normalised);
        let mut bytes = Vec::with_capacity(self.encoding.prefix().len() + text.len());
        bytes.extend_from_slice(self.encoding.prefix());
        bytes.extend_from_slice(text.as_bytes());
        Self::write_through_a_temporary(path, &bytes)?;
        self.path = Some(path.to_owned());
        self.saved_history_revision = self.history_revision;
        // Typing after a save must begin a new undo group. Otherwise it merges with the typing that
        // preceded the save and there is no snapshot at the exact state that was written to disk.
        self.last_edit = EditKind::Other;
        Ok(())
    }

    pub fn text(&self) -> &Rope {
        &self.text
    }

    pub fn chars(&self) -> &StyleSpans {
        &self.chars
    }

    pub fn paragraphs(&self) -> &ParagraphStyles {
        &self.paragraphs
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Point the document at a different file, without reading or writing anything.
    ///
    /// What a move needs and nothing else does: the bytes are already at the new path, so the tab
    /// that is showing them has to follow. It is **not an edit** — no byte of the text changes — so
    /// `modified`, the undo history and the revisions are all left exactly as they are.
    pub fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    /// True when the current history state is not the one last opened or successfully saved.
    pub fn is_modified(&self) -> bool {
        self.history_revision != self.saved_history_revision
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The revision of the text and its formatting alone.
    ///
    /// Unchanged by moving the caret, by selecting, and by marking a passage with a highlight colour,
    /// because none of those changes where a single letter sits. Laying out and colouring are keyed
    /// on this.
    pub fn text_revision(&self) -> u64 {
        self.text_revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The text that is selected, which is what copy and cut put on the clipboard.
    pub fn selected_text(&self) -> String {
        self.text.byte_slice(self.selection.range())
    }

    /// The formatting that a toolbar should show as active: the formatting of the selection when it is
    /// uniform, and otherwise the formatting at the caret, with anything chosen but not yet typed
    /// applied on top.
    pub fn active_style(&self) -> CharStyle {
        let mut style = self.chars.style_at(self.selection.head).clone();
        if !self.selection.is_empty() {
            let range = self.selection.range();
            style = self.chars.style_at(range.start).clone();
            style.bold = self.chars.all_in(range.clone(), |s| s.bold);
            style.italic = self.chars.all_in(range.clone(), |s| s.italic);
            style.underline = self.chars.all_in(range.clone(), |s| s.underline);
            style.strikethrough = self.chars.all_in(range, |s| s.strikethrough);
        }
        self.pending.clone().apply_over(&mut style);
        style
    }

    /// Change the formatting of the whole document, and of text typed after it, without treating it as an
    /// edit.
    ///
    /// This is what `Settings -> Appearance -> Font` uses. Setting the family and the size there is a
    /// choice about how the document is shown rather than a change to it, so three things follow. It
    /// pushes nothing onto the undo history, because undoing a settings change from the editor would be
    /// surprising and because the setting is written to the settings file instead. It does not mark the
    /// document as having unsaved changes, because a file Unluminous saves is plain text and carries no
    /// formatting, so nothing about the file has changed. It does bump the revision, because the text has
    /// to be laid out again at the new size.
    ///
    /// Only the fields the change names are touched, so a word set in bold or in red stays bold or red
    /// when the family changes underneath it.
    pub fn set_base_style(&mut self, change: StyleChange) {
        if change.is_empty() {
            return;
        }
        let end = self.text.len_bytes();
        if end > 0 {
            self.chars.set(0..end, &change);
        }
        // Also the formatting for text typed next, so that an empty document and a document with text in
        // it behave the same way.
        change.apply_over_change(&mut self.pending);
        self.text_changed();
    }

    /// Colour the document by what its text is: keywords, strings, comments and the rest.
    ///
    /// This is not an edit, and follows the same three rules [`Self::set_base_style`] does and for
    /// the same reasons. It pushes nothing onto the undo history, because undoing a colour scheme
    /// from the editor would be surprising. It does not mark the document as having unsaved
    /// changes, because what Unluminous saves is plain text and carries no formatting, so nothing about
    /// the file has changed. It does bump the revision, because the text has to be drawn again.
    ///
    /// The spans are given as ranges of bytes and a colour each. Everything not covered goes back to
    /// `base`, so switching from one language to another, or turning colouring off, does not leave
    /// the last language's colours behind.
    pub fn set_syntax(&mut self, base: Color, spans: &[(Range<usize>, Color)]) {
        let end = self.text.len_bytes();
        if end == 0 {
            return;
        }
        self.chars.set(0..end, &StyleChange::color(base));
        // Gathered first and applied in one pass. Calling `set` once per token walked the whole span
        // list once per token, which on a 169 kilobyte source file was 561 ms — long enough that a
        // keystroke was a visible stall. See `tasks/task-1666-performance-tdd.md` section 3.
        let mut changes: Vec<(Range<usize>, StyleChange)> = Vec::with_capacity(spans.len());
        for (range, color) in spans {
            // A span from a highlighter that has not caught up with an edit yet would otherwise
            // panic inside the span list, and a colour is never worth a crash.
            let start = range.start.min(end);
            let stop = range.end.min(end);
            if start >= stop
                || !self.text.is_char_boundary(start)
                || !self.text.is_char_boundary(stop)
            {
                continue;
            }
            changes.push((start..stop, StyleChange::color(*color)));
        }
        self.chars.set_many(&changes);
        self.syntax_dirt = Dirt::Clean;
        self.text_changed();
    }

    /// The same, over one stretch of the file rather than the whole of it.
    ///
    /// **The other half of `task-1804` §5.2, and the larger half.** Reading the tokens
    /// incrementally took the tokeniser's share of a keystroke away and left `set_syntax`'s, which at
    /// 2 MB was the bigger of the two: it painted the base colour over two million bytes and then
    /// laid 157,000 spans back over it, every time.
    ///
    /// It does not have to. `insert` and `remove_range` have **already shifted the style spans** by
    /// the edit -- that is what `chars.insert` and `chars.remove` do -- so outside the stretch whose
    /// tokens changed, the colours are already right. Only `changed` needs painting again, and
    /// `spans` are the tokens that fall inside it.
    ///
    /// `base` is applied to `changed` only, for the same reason: everything outside it keeps the
    /// colour it had, which is the colour it should have.
    pub fn set_syntax_in(
        &mut self,
        base: Color,
        spans: &[(Range<usize>, Color)],
        changed: Range<usize>,
    ) {
        let end = self.text.len_bytes();
        if end == 0 {
            self.syntax_dirt = Dirt::Clean;
            return;
        }
        let from = changed.start.min(end);
        let to = changed.end.min(end);
        if from >= to {
            self.syntax_dirt = Dirt::Clean;
            self.text_changed();
            return;
        }
        let mut changes: Vec<(Range<usize>, StyleChange)> = Vec::with_capacity(spans.len());
        for (range, color) in spans {
            let start = range.start.max(from).min(end);
            let stop = range.end.min(to);
            if start >= stop
                || !self.text.is_char_boundary(start)
                || !self.text.is_char_boundary(stop)
            {
                continue;
            }
            changes.push((start..stop, StyleChange::color(*color)));
        }
        // One call rather than a `set` and a `set_many`, and rebuilding only the spans it touches:
        // see `StyleSpans::set_in` for what the difference is worth.
        self.chars.set_in(from..to, &StyleChange::color(base), &changes);
        // And the layout only has to look at the paragraphs whose colours moved, which is the same
        // stretch. `task-1984` C7.
        self.note_paragraphs(self.text.byte_to_line(from), self.text.byte_to_line(to));
        self.syntax_dirt = Dirt::Clean;
        self.text_changed();
    }

    /// What has changed since the syntax was last set.
    pub fn syntax_dirt(&self) -> Dirt {
        self.syntax_dirt
    }

    /// Say the whole file has to be read again, which is what an undo and a fresh text mean.
    pub fn syntax_is_wholly_dirty(&mut self) {
        self.syntax_dirt = Dirt::Whole;
    }

    // --------------------------------------------------------------------- the marked passages

    /// Every passage somebody has marked with a colour.
    pub fn highlights(&self) -> &Highlights {
        &self.highlights
    }

    /// Put a whole set back, which is what opening a file that has been marked before does.
    ///
    /// The ranges are clamped to the text, because they were written against the bytes the file had
    /// when it was closed and something outside Unluminous may have rewritten it since. A mark in the
    /// wrong place is a wrong colour; a range past the end of the rope is a panic in the layout
    /// engine.
    pub fn set_highlights(&mut self, mut highlights: Highlights) {
        highlights.clamp(self.text.len_bytes());
        if highlights == self.highlights {
            return;
        }
        self.highlights = highlights;
        self.revision += 1;
    }

    /// Mark `range` in `color`. True when anything changed.
    ///
    /// **Not an edit**, which is the rule the editor's font already follows: nothing goes onto the
    /// undo history and the file is not marked as having unsaved changes, because what Unluminous saves
    /// is plain text and a mark is not in it. The revision is bumped, because it has to be drawn.
    pub fn highlight(&mut self, range: Range<usize>, color: Rgba) -> bool {
        let end = self.text.len_bytes();
        let start = range.start.min(end);
        let stop = range.end.min(end);
        if start >= stop {
            return false;
        }
        let before = self.highlights.clone();
        self.highlights.add(start..stop, color);
        if before == self.highlights {
            return false;
        }
        self.revision += 1;
        true
    }

    /// Take away the mark covering `offset`, which is what a right click on one offers.
    pub fn clear_highlight_at(&mut self, offset: usize) -> bool {
        let cleared = self.highlights.clear_at(offset);
        if cleared {
            self.revision += 1;
        }
        cleared
    }

    /// Take away the marks `range` touches.
    pub fn clear_highlight(&mut self, range: Range<usize>) -> bool {
        let cleared = self.highlights.clear(range);
        if cleared {
            self.revision += 1;
        }
        cleared
    }

    /// Take away every mark in this file.
    pub fn clear_highlights(&mut self) -> bool {
        let cleared = self.highlights.clear_all();
        if cleared {
            self.revision += 1;
        }
        cleared
    }

    // --------------------------------------------------------------------- the collapsed blocks

    /// Which blocks are collapsed.
    pub fn folds(&self) -> &Folds {
        &self.folds
    }

    /// How many times a block has been collapsed or expanded, which is what the layout cache is
    /// keyed on beside [`Self::text_revision`].
    pub fn fold_revision(&self) -> u64 {
        self.fold_revision
    }

    /// Put a whole set back. True when anything changed.
    ///
    /// Every fold command rebuilds the set from the regions as the text has them now and hands the
    /// answer here, which is what keeps an offset that no longer names any head from being kept for
    /// ever.
    ///
    /// **Not an edit**, which is the rule the marked passages and the editor's font already follow:
    /// nothing goes on the undo history and the file is not marked as having unsaved changes,
    /// because what Unluminous saves is plain text and a fold is not in it.
    pub fn set_folds(&mut self, mut folds: Folds) -> bool {
        folds.clamp(self.text.len_bytes());
        if folds == self.folds {
            return false;
        }
        self.folds = folds;
        self.revision += 1;
        self.fold_revision += 1;
        true
    }

    /// Expand everything. True when anything was collapsed.
    pub fn expand_all_folds(&mut self) -> bool {
        self.set_folds(Folds::new())
    }

    // ------------------------------------------------------------------------------ the breakpoints

    /// Where a debugger is to stop in this file.
    pub fn breakpoints(&self) -> &Breakpoints {
        &self.breakpoints
    }

    /// Put a whole set back, which is what opening a file that has been debugged before does.
    ///
    /// The offsets are clamped to the text for `set_highlights`' reason: they were written against
    /// the bytes the file had when it was closed, and something outside Unluminous may have rewritten it
    /// since. A dot in the wrong place is a dot in the wrong place — and the adapter's `verified`
    /// answer then says so honestly — where an offset past the end of the rope is a panic.
    pub fn set_breakpoints(&mut self, mut breakpoints: Breakpoints) -> bool {
        breakpoints.clamp(self.text.len_bytes());
        if breakpoints == self.breakpoints {
            return false;
        }
        self.breakpoints = breakpoints;
        self.revision += 1;
        true
    }

    /// Put a breakpoint on the line `offset` is in, or take away the one that is there.
    ///
    /// The offset is snapped to the **start of its line**, so clicking anywhere in the gutter row and
    /// putting the caret anywhere in the line mean the same thing — and so a line can never end up
    /// with two.
    ///
    /// **Not an edit**, which is the rule the marked passages, the folds and the editor's font all
    /// follow: nothing goes on the undo history and the file is not marked as having unsaved changes,
    /// because what Unluminous saves is plain text and a breakpoint is not in it. The revision moves,
    /// because the gutter has to be drawn again.
    pub fn toggle_breakpoint(&mut self, offset: usize) -> bool {
        let start = self.line_start_of(offset);
        let now = self.breakpoints.toggle(start);
        self.revision += 1;
        now
    }

    /// Change the breakpoint on the line `offset` is in, which is what the edit modal and
    /// `Disable Breakpoint` both do. True when there was one to change.
    pub fn change_breakpoint(
        &mut self,
        offset: usize,
        change: impl FnOnce(&mut crate::breakpoints::Breakpoint),
    ) -> bool {
        let start = self.line_start_of(offset);
        let Some(breakpoint) = self.breakpoints.at_mut(start) else {
            return false;
        };
        let before = breakpoint.clone();
        change(breakpoint);
        if *breakpoint == before {
            return false;
        }
        self.revision += 1;
        true
    }

    /// The byte offset the line holding `offset` starts at.
    ///
    /// The one conversion, so the gutter, the command line and the adapter's answers all snap the
    /// same way. An offset past the end of the text is the last line's, which is what clamping a set
    /// read from disk needs.
    pub fn line_start_of(&self, offset: usize) -> usize {
        let offset = offset.min(self.text.len_bytes());
        self.text.line_to_byte(self.text.byte_to_line(offset))
    }

    /// Which **one-based** line `offset` is on, which is what the protocol takes and what the gutter
    /// draws. `unluminous-core` counts paragraphs from zero everywhere else, so the conversion is here
    /// rather than at each of the places that need it.
    pub fn line_number_of(&self, offset: usize) -> usize {
        self.text.byte_to_line(offset.min(self.text.len_bytes())) + 1
    }

    /// The byte offset a **one-based** line starts at: the other direction, for an adapter's answer
    /// and for `debug breakpoint add <path> <line>`.
    pub fn offset_of_line_number(&self, line: usize) -> usize {
        self.text.line_to_byte(line.saturating_sub(1))
    }

    /// The paragraph formatting of the paragraph the caret is in.
    pub fn active_paragraph_style(&self) -> ParagraphStyle {
        self.paragraphs.get(self.text.byte_to_line(self.selection.head))
    }

    /// The paragraphs the selection touches.
    fn selected_paragraphs(&self) -> Range<usize> {
        let range = self.selection.range();
        let first = self.text.byte_to_line(range.start);
        let last = self.text.byte_to_line(range.end);
        first..last + 1
    }

    /// A window of the text around `offset`, and the document offset it starts at. Movement by
    /// grapheme cluster and by word works on this window rather than on the whole document.
    ///
    /// **Bounded at [`CARET_WINDOW`] either side** (`task-1984` C5). It used to be the whole line,
    /// copied into a fresh `String` -- which is a few dozen bytes in ordinary source and is the whole
    /// file in a minified `.js`, a one line `.json` or a single log line. Measured there: **15.9 ms
    /// for one arrow key** on a 1 MB file with one line in it, against 0.002 ms on a file with line
    /// breaks. Holding an arrow key dropped every frame.
    ///
    /// The window is cut to the line as well, so nothing steps across a line break that was not there
    /// before, and to character boundaries, so the slice is a `str`. A grapheme cluster longer than
    /// the bound is the one thing this cannot see whole, and a few hundred bytes is far more than the
    /// longest cluster Unicode defines.
    fn line_window(&self, offset: usize) -> (String, usize) {
        let line = self.text.byte_to_line(offset);
        let whole = self.text.line_range(line);
        let from = self.on_a_boundary(offset.saturating_sub(CARET_WINDOW).max(whole.start), false);
        let to = self.on_a_boundary((offset + CARET_WINDOW).min(whole.end), true);
        (self.text.byte_slice(from..to), from)
    }

    /// `offset` moved to the nearest character boundary, outwards from the caret.
    ///
    /// `forwards` says which way to move: a window's end goes up and its start goes down, so the
    /// window can only ever grow to reach one, never shrink past what was asked for.
    fn on_a_boundary(&self, offset: usize, forwards: bool) -> usize {
        let mut offset = offset.min(self.text.len_bytes());
        while offset > 0 && offset < self.text.len_bytes() && !self.text.is_char_boundary(offset) {
            offset = match forwards {
                true => offset + 1,
                false => offset - 1,
            };
        }
        offset
    }

    /// Run a command. Returns true when the document changed in a way that needs repainting.
    pub fn apply(&mut self, command: Command) -> bool {
        let before = self.revision;
        let selection_before = self.selection;
        match command {
            Command::Insert(text) => self.insert(&text),
            Command::DeleteBackward => self.delete_backward(),
            Command::DeleteForward => self.delete_forward(),
            Command::DeleteWordBackward => self.delete_word_backward(),
            Command::MoveLeft { extend } => self.move_horizontally(-1, extend, false),
            Command::MoveRight { extend } => self.move_horizontally(1, extend, false),
            Command::MoveWordLeft { extend } => self.move_horizontally(-1, extend, true),
            Command::MoveWordRight { extend } => self.move_horizontally(1, extend, true),
            // **The rope already has both of these numbers** (`task-1984` C5). Both used to build a
            // window -- a copy of the whole line -- and then throw the text away to keep an offset.
            Command::MoveLineStart { extend } => {
                let line = self.text.byte_to_line(self.selection.head);
                self.selection.move_to(self.text.line_range(line).start, extend);
                self.desired_x = None;
                self.caret_moved(selection_before);
            }
            Command::MoveLineEnd { extend } => {
                let line = self.text.byte_to_line(self.selection.head);
                self.selection.move_to(self.text.line_range(line).end, extend);
                self.desired_x = None;
                self.caret_moved(selection_before);
            }
            Command::MoveDocumentStart { extend } => {
                self.selection.move_to(0, extend);
                self.desired_x = None;
                self.caret_moved(selection_before);
            }
            Command::MoveDocumentEnd { extend } => {
                self.selection.move_to(self.text.len_bytes(), extend);
                self.desired_x = None;
                self.caret_moved(selection_before);
            }
            Command::SelectAll => {
                self.selection = Selection::new(0, self.text.len_bytes());
                self.desired_x = None;
                self.caret_moved(selection_before);
            }
            Command::PlaceCaret { offset, extend } => {
                let offset = self.clamp_to_boundary(offset);
                self.selection.move_to(offset, extend);
                self.desired_x = None;
                self.caret_moved(selection_before);
            }
            Command::ReplaceMany(edits) => self.replace_many(edits),
            Command::Indent { unit } => self.indent(unit),
            Command::Dedent { unit } => self.dedent(unit),
            Command::ToggleLineComment { marker } => self.toggle_line_comment(&marker),
            Command::ToggleBlockComment { open, close } => self.toggle_block_comment(&open, &close),
            Command::DuplicateLines => self.duplicate_lines(),
            Command::MoveLines { by } => self.move_lines(by),
            Command::JoinLines => self.join_lines(),
            Command::SortLines => self.sort_lines(),
            Command::TrimTrailingWhitespace => self.trim_trailing_whitespace(),
            Command::ApplyStyle(change) => self.apply_style(change),
            Command::ToggleBold => {
                let on = !self.active_style().bold;
                self.apply_style(StyleChange::bold(on));
            }
            Command::ToggleItalic => {
                let on = !self.active_style().italic;
                self.apply_style(StyleChange::italic(on));
            }
            Command::ToggleUnderline => {
                let on = !self.active_style().underline;
                self.apply_style(StyleChange::underline(on));
            }
            Command::ToggleStrikethrough => {
                let on = !self.active_style().strikethrough;
                self.apply_style(StyleChange::strikethrough(on));
            }
            Command::SetAlign(align) => {
                let paragraphs = self.selected_paragraphs();
                self.push_undo(EditKind::Other);
                self.paragraphs.set(paragraphs, |p| p.align = align);
                self.mark_changed();
            }
            Command::SetLineSpacing(spacing) => {
                let paragraphs = self.selected_paragraphs();
                let spacing = spacing.clamp(0.5, 4.0);
                self.push_undo(EditKind::Other);
                self.paragraphs.set(paragraphs, |p| p.line_spacing = spacing);
                self.mark_changed();
            }
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
        }
        self.revision != before
    }

    /// Move the caret up or down `delta` lines, using a laid out document, because with word wrap one
    /// paragraph is several lines on screen and the text alone cannot say what is above a position.
    pub fn move_vertically(&mut self, layout: &Layout, delta: i32, extend: bool) -> bool {
        if layout.lines.is_empty() {
            return false;
        }
        let selection_before = self.selection;
        let caret = layout.caret_at(self.selection.head);
        // Remember the column across a run of vertical moves, so that going down through a short line
        // and on to a long one comes back to where it started.
        let target_x = self.desired_x.unwrap_or(caret.x);
        let line = (caret.line as i32 + delta).clamp(0, layout.lines.len() as i32 - 1) as usize;
        if line == caret.line {
            // Already at the top or the bottom: go to the start or the end of the text instead, which
            // is what pressing up on the first line does everywhere.
            let at = if delta < 0 { 0 } else { self.text.len_bytes() };
            self.selection.move_to(at, extend);
            self.caret_moved(selection_before);
            return true;
        }
        let offset = layout.offset_on_line_at_x(line, target_x);
        self.selection.move_to(self.clamp_to_boundary(offset), extend);
        self.desired_x = Some(target_x);
        self.last_edit = EditKind::Other;
        self.revision += 1;
        true
    }

    fn clamp_to_boundary(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len_bytes());
        while offset > 0 && !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    /// Record that the caret may have moved. A key press that moves nothing, such as the right arrow
    /// at the end of the text, must not bump the revision, or the view repaints for nothing.
    fn caret_moved(&mut self, before: Selection) {
        if self.selection != before {
            self.last_edit = EditKind::Other;
            self.revision += 1;
        }
    }

    /// Give a real edit a fresh history identity, invalidate redo, and move the view revisions.
    fn mark_changed(&mut self) {
        self.history_revision = self.next_history_revision;
        self.next_history_revision += 1;
        self.text_changed();
        self.redo.clear();
    }

    /// Record that the text, or the formatting over it, is different from what was last laid out.
    ///
    /// Every change that moves a letter goes through here, and nothing that only moves the caret
    /// does. `a_layout_that_changed_means_the_text_revision_moved` is what keeps that true.
    fn text_changed(&mut self) {
        self.revision += 1;
        self.text_revision += 1;
        // What this edit touched, closed off and filed under the revision it produced. Anything that
        // did not say leaves `touching` at `whole`, which is the answer that reads the document.
        if self.touched.len() == TOUCHED_REVISIONS {
            self.touched.pop_front();
        }
        let touching = self.touching.take().unwrap_or_else(Touched::whole);
        self.touched.push_back((self.text_revision, touching));
    }

    /// Record that paragraphs `from` to `through` inclusive have been changed by the edit in hand.
    ///
    /// `task-1984` C7. Called after the change, so the indices and the paragraph count are the ones
    /// the document has now. Saying nothing is always safe and means the whole document; saying more
    /// than was really touched is safe too, and is what a caller that is unsure should do.
    fn note_paragraphs(&mut self, from: usize, through: usize) {
        let count = self.text.len_lines();
        let one = Touched::between(from, through, count);
        // The **narrower** of what has been said so far and this, because two edits in one undo step
        // have between them left unchanged whatever both of them left unchanged. Starting from
        // `whole` and narrowing would not work: `whole` is `from: 0, tail: 0`, so the narrowing would
        // answer `whole` for ever -- which is why nothing said yet is `None` rather than `whole`.
        self.touching = Some(match self.touching {
            Some(so_far) => so_far.and(one),
            None => one,
        });
    }

    /// Which paragraphs have changed since `revision`, for [`crate::relayout_touching`].
    ///
    /// **`task-1984` C7.** A pane that laid the document out at `revision` and is laying it out again
    /// now needs to know which paragraphs to fingerprint, and reading the whole file to find out was
    /// 17.2 ms of a 21.7 ms keystroke at 2 MB. Every revision in between is folded together, so a
    /// pane that missed a few frames gets one answer covering all of them.
    ///
    /// [`Touched::whole`] when the pane is further behind than the ring holds, or when any edit in
    /// between did not say what it touched -- both of which are the reading this replaced, so being
    /// wrong about it is slow rather than incorrect.
    pub fn touched_since(&self, revision: u64) -> Touched {
        let Some((oldest, _)) = self.touched.front() else { return Touched::whole() };
        if *oldest > revision + 1 {
            return Touched::whole();
        }
        self.touched
            .iter()
            .filter(|(at, _)| *at > revision)
            .fold(Touched { from: usize::MAX, tail: usize::MAX }, |all, (_, one)| all.and(*one))
    }

    /// Save the current state unless this edit belongs to the current run of typing.
    fn push_undo(&mut self, kind: EditKind) {
        // A run of single character typing is one undo step, so that undo removes a word rather than a
        // letter, and **a run of deleting is one too** -- which is what `EditKind::Deleting` was
        // recorded for and never used for. `task-1984` C13: the only comparison against it was
        // `kind == EditKind::Typing`, so it behaved exactly as `Other` and holding Backspace was one
        // whole document snapshot per character and one undo step per character. On a 2 MB file that
        // is 15.5 MB a keystroke.
        //
        // A caret move, a word delete or a formatting change is `Other` and still breaks the run,
        // which is what keeps undo's steps the size of an action rather than of a gesture.
        let grouping = matches!(kind, EditKind::Typing | EditKind::Deleting);
        if grouping && kind == self.last_edit {
            return;
        }
        // `snapshot()` rather than a second copy of its eight fields, which is `task-1922`'s review
        // finding: a ninth thing kept per document would have had two places to be added and one of
        // them would have been forgotten.
        self.undo.push(self.snapshot());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.last_edit = kind;
    }

    /// Capture everything undo restores, including the identity used for saved-point tracking.
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            chars: self.chars.clone(),
            paragraphs: self.paragraphs.clone(),
            selection: self.selection,
            history_revision: self.history_revision,
            highlights: self.highlights.clone(),
            folds: self.folds.clone(),
            breakpoints: self.breakpoints.clone(),
        }
    }

    /// Replace the document state with a snapshot while leaving monotonic counters monotonic.
    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.chars = snapshot.chars;
        self.paragraphs = snapshot.paragraphs;
        self.selection = snapshot.selection;
        self.history_revision = snapshot.history_revision;
        self.highlights = snapshot.highlights;
        // Restoring a state restores which blocks were collapsed, exactly as it restores which
        // passages were marked. `fold_revision` moves because the layout has to be worked out
        // again: the text has changed underneath the folds.
        self.folds = snapshot.folds;
        self.fold_revision += 1;
        // And which lines the program was to stop on, for the third time and the same reason.
        self.breakpoints = snapshot.breakpoints;
        // A restored snapshot brings its **own** style spans with it, and they were the spans of a
        // different text. There is no edit to be incremental about here, so the next colouring reads
        // the whole file. `task-1804` §5.2.
        self.syntax_dirt = crate::incremental::Dirt::Whole;
    }

    /// Move to the preceding snapshot and preserve the current state for redo.
    fn undo(&mut self) {
        let Some(previous) = self.undo.pop() else {
            return;
        };
        let current = self.snapshot();
        self.restore(previous);
        self.redo.push(current);
        self.last_edit = EditKind::Other;
        self.text_changed();
    }

    /// Move to the next snapshot and preserve the current state for undo.
    fn redo(&mut self) {
        let Some(next) = self.redo.pop() else {
            return;
        };
        let current = self.snapshot();
        self.restore(next);
        self.undo.push(current);
        self.last_edit = EditKind::Other;
        self.text_changed();
    }

    /// Replace `range` with `text` in the text, the character formatting, the marked passages, the
    /// folds, the breakpoints and the paragraph list, all by the same edit.
    ///
    /// This is the one place in Unluminous that knows a range of bytes moved. `insert`,
    /// `remove_range`, `replace_many`, `indent` and `dedent` used to shift all six by hand, which
    /// meant a sixth structure -- and there have been five so far -- would have had four call sites
    /// to be threaded through, each an easy one to forget. An empty `range` is a pure insertion, an
    /// empty `text` is a pure removal, and that is what lets one function stand in for both
    /// `insert`'s shape and `remove_range`'s.
    ///
    /// What it does not do: it does not touch the caret or the undo history, because those are the
    /// caller's business, not the edit's; it does not note `syntax_dirt`, because `indent`,
    /// `dedent` and `replace_many` never did and still do not, so a rename or an indent still
    /// colours the file as a whole rather than incrementally; and it does not apply a style to the
    /// inserted bytes, because `chars.insert` only makes room for them -- the caller colours what it
    /// put there, with whatever style it read before the edit.
    fn splice(&mut self, range: Range<usize>, text: &str) {
        let paragraph = self.text.byte_to_line(range.start);
        if !range.is_empty() {
            let last = self.text.byte_to_line(range.end);
            self.text.remove(range.clone());
            self.chars.remove(range.clone());
            self.highlights.remove(range.clone());
            self.folds.remove(range.clone());
            self.breakpoints.remove(range.clone());
            self.paragraphs.join(paragraph, last);
        }
        let mut line_breaks = 0;
        if !text.is_empty() {
            let at = range.start;
            line_breaks = text.bytes().filter(|byte| *byte == b'\n').count();
            self.text.insert(at, text);
            self.chars.insert(at, text.len());
            self.highlights.insert(at, text.len());
            self.folds.insert(at, text.len());
            self.breakpoints.insert(at, text.len());
            self.paragraphs.split(paragraph, line_breaks);
        }
        // The seventh thing this one function tells, after the text, the formatting, the marks, the
        // folds, the breakpoints and the paragraph list: **which paragraphs moved**. A removal
        // collapses everything it spanned into `paragraph` and an insertion turns `paragraph` into
        // that many more, so what the edit left is `paragraph` through `paragraph + line_breaks`.
        // `task-1984` C7.
        self.note_paragraphs(paragraph, paragraph + line_breaks);
    }

    fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let single = text.chars().count() == 1 && text != "\n";
        self.push_undo(if single { EditKind::Typing } else { EditKind::Other });
        let range = self.selection.range();
        // The formatting for the new text is read before the deletion, because deleting the selection
        // can remove the very span the new text should inherit from.
        let mut style = self.chars.style_for_insertion(range.start);
        self.pending.clone().apply_over(&mut style);
        if !range.is_empty() {
            self.remove_range(range.clone());
        }
        let at = range.start;
        // The fourth thing this function tells, beside the marks, the folds and the breakpoints.
        self.syntax_dirt = self.syntax_dirt.note(at, 0, text.len());
        self.splice(at..at, text);
        // `set_in` rather than `set`, because this is one letter and `set` rebuilds the whole span
        // list to apply it -- 234,000 style clones for a keystroke on a 2 MB file. `task-1804` §5.2.
        self.chars.set_in(at..at + text.len(), &style_as_change(&style), &[]);

        self.selection.set_caret(at + text.len());
        self.pending = StyleChange::default();
        self.desired_x = None;
        self.mark_changed();
        self.last_edit = if single { EditKind::Typing } else { EditKind::Other };
    }

    /// Replace several ranges in one undo step, back to front.
    ///
    /// One `push_undo` and then every edit, which is what makes a rename across a file a single
    /// step to undo. Each edit is deliberately the same `Self::splice` that [`Self::insert`] calls —
    /// the text, the character formatting and the marked passages moving together — because a place
    /// that knew the bytes had moved and forgot one of them would be a mark left behind on the wrong
    /// word, and that is a fault that looks like a drawing bug and lives in the model.
    fn replace_many(&mut self, edits: Vec<(Range<usize>, String)>) {
        // Put in order here as well as by the caller, because a `Command` can be built by hand —
        // the command line builds one — and applying these in the wrong order would corrupt the
        // file rather than refuse. Nothing is trusted: a range outside the text or across the
        // middle of a character is dropped, and where two overlap the earlier one wins, which is
        // the same choice `symbols::replacements` makes so the two cannot disagree.
        //
        // An **empty** range carrying text is an insertion at that point, which `task-1680` needs:
        // accepting `./layout` inside `from '│'` replaces nothing and inserts everything, and it has
        // to be the same one-undo-step command every other completion is. An empty range carrying
        // nothing is dropped, because it would push an undo step for an edit that changes no byte.
        let mut edits: Vec<(Range<usize>, String)> = edits
            .into_iter()
            .filter(|(range, replacement)| {
                (range.start < range.end || !replacement.is_empty())
                    && range.start <= range.end
                    && range.end <= self.text.len_bytes()
                    && self.clamp_to_boundary(range.start) == range.start
                    && self.clamp_to_boundary(range.end) == range.end
            })
            .collect();
        edits.sort_by_key(|(range, _)| range.start);
        let mut reached = 0;
        edits.retain(|(range, _)| {
            if range.start < reached {
                return false;
            }
            reached = range.end;
            true
        });
        edits.reverse();
        if edits.is_empty() {
            return;
        }
        self.push_undo(EditKind::Other);

        // **Every style is read in one walk, and every colour is written in one** (`task-1984` C6).
        //
        // This loop used to read `style_for_insertion` and call `StyleSpans::set` once per
        // replacement, and both of those walk the whole span list -- so Replace All over 210 matches
        // in a coloured 117 KB file walked 10,000 spans four hundred and twenty times. Measured,
        // **27.5 ms coloured against 0.24 ms uncoloured**: a colour scheme was deciding what an edit
        // cost, which is the same fault `task-1804` §5.2 took out of one keystroke, left behind on
        // the one path that pays it hundreds of times in a row. `editor rename`, Find in Files
        // Replace All and every accepted completion come through here.
        //
        // The styles are read **before any edit**, forwards, in a single pass over the spans, which
        // is what `style_at` would have been doing repeatedly -- and it is the same answer, because
        // `insert` and `replace_many` both read a style from the text as it was before the deletion.
        let styles = self.chars.styles_at(edits.iter().rev().map(|(range, _)| range.start));

        // Then the text, back to front, so an earlier edit's start is not moved by a later one.
        // Nothing is coloured in here: the colouring is one call once every byte has stopped moving.
        for (range, replacement) in &edits {
            let at = range.start;
            self.remove_range(range.clone());
            if !replacement.is_empty() {
                self.splice(at..at, replacement);
            }
        }

        // Where each replacement ended up. Applying back to front means a replacement is shifted
        // only by the edits **before** it in the text, and those are exactly the ones applied after
        // it, so the shift is the running difference in length over the earlier edits.
        let mut changes: Vec<(Range<usize>, StyleChange)> = Vec::with_capacity(edits.len());
        let mut shift = 0isize;
        let mut caret = None;
        for (turn, ((range, replacement), style)) in edits.iter().rev().zip(styles).enumerate() {
            let at = range.start.saturating_add_signed(shift);
            shift += replacement.len() as isize - (range.end - range.start) as isize;
            if replacement.is_empty() {
                continue;
            }
            changes.push((at..at + replacement.len(), style_as_change(&style)));
            // **Only the earliest edit moves the caret**, and only when it really inserted
            // something. The loop this replaced ran back to front setting the caret every turn, so
            // the last thing to touch it was the earliest edit -- and where an earliest edit with
            // an empty replacement left it is `remove_range`'s answer, which the splice loop above
            // has already given, so there is nothing to say about that case here.
            if turn == 0 {
                caret = Some(at + replacement.len());
            }
        }
        if let (Some(first), Some(last)) = (changes.first(), changes.last()) {
            let stretch = first.0.start..last.0.end;
            self.chars.set_in(stretch, &StyleChange::default(), &changes);
        }
        if let Some(caret) = caret {
            self.selection.set_caret(caret);
        }
        self.desired_x = None;
        self.mark_changed();
        self.last_edit = EditKind::Other;
    }

    /// Indent each line the selection touches, by one tab or one space.
    ///
    /// The lines are indented back to front, so an earlier line's start is not moved by a later
    /// line's edit, and each end of the selection moves past the indents put at or before it: the
    /// same shift the marks and the folds get, applied to the two numbers a selection is. That is
    /// what keeps the highlight over the words it covered, and what makes pressing the key again
    /// indent the same lines again.
    ///
    /// The edit is one `push_undo` and one `mark_changed`, whatever the selection spans: one key
    /// press is one snapshot and one undo step, for the reason `replace_many` states. No line break
    /// is inserted or removed, so the paragraph list is left exactly as it was.
    fn indent(&mut self, unit: IndentUnit) {
        let (first, last) = self.touched_lines();
        let starts: Vec<usize> = (first..=last).map(|line| self.text.line_to_byte(line)).collect();
        let unit = unit.text();
        self.push_undo(EditKind::Other);
        for at in starts.iter().rev() {
            let at = *at;
            // Read before the earlier lines are edited, the way `insert` reads its style, though an
            // indent never deletes the span it reads. The pending formatting is not applied, because
            // an indent is not typing.
            let style = self.chars.style_for_insertion(at);
            self.splice(at..at, unit);
            // `set_in`, the same as `replace_many` above: one indent is one span's worth of colour
            // and this loop runs once per line of the selection.
            self.chars.set_in(at..at + unit.len(), &style_as_change(&style), &[]);
        }
        let shift =
            |offset: usize| starts.iter().filter(|start| **start <= offset).count() * unit.len();
        let selection = self.selection;
        self.selection.anchor += shift(selection.anchor);
        self.selection.head += shift(selection.head);
        self.desired_x = None;
        self.mark_changed();
        self.last_edit = EditKind::Other;
    }

    /// Remove one indent from each line the selection touches, the reverse of `indent`.
    ///
    /// Only a line whose first byte is exactly the unit's character loses one; every other touched
    /// line — flush against the margin, or indented with the other unit — is left untouched, so a
    /// selection spanning a mix of lines dedents only the ones that actually can be. A press that
    /// finds nothing to remove anywhere in the selection does not push an undo step, because there
    /// would be nothing in it to undo.
    ///
    /// The removal mirrors `indent`'s insertion byte for byte: found back to front so an earlier
    /// line's start is not moved by a later line's edit, taken out of the text, the character
    /// formatting and the marked passages together, with the folds and the breakpoints shifted by
    /// the same `Self::splice`. The selection follows the text the same way `indent`'s does: each
    /// end moves back past the removals at or before it, so pressing the key again removes another
    /// level from whatever is still indented.
    fn dedent(&mut self, unit: IndentUnit) {
        let (first, last) = self.touched_lines();
        let unit = unit.text();
        let starts: Vec<usize> = (first..=last)
            .map(|line| self.text.line_to_byte(line))
            .filter(|&at| {
                at + unit.len() <= self.text.len_bytes()
                    && self.text.byte_slice(at..at + unit.len()) == unit
            })
            .collect();
        if starts.is_empty() {
            return;
        }
        self.push_undo(EditKind::Other);
        for at in starts.iter().rev() {
            let at = *at;
            self.splice(at..at + unit.len(), "");
        }
        let shift =
            |offset: usize| starts.iter().filter(|start| **start < offset).count() * unit.len();
        let selection = self.selection;
        self.selection.anchor = selection.anchor.saturating_sub(shift(selection.anchor));
        self.selection.head = selection.head.saturating_sub(shift(selection.head));
        self.desired_x = None;
        self.mark_changed();
        self.last_edit = EditKind::Other;
    }

    // --------------------------------------------------------------------------- the line commands

    // Seven commands that work on whole lines, and one question about brackets. `task-1922` §5.5.
    //
    // Every one of them makes its edits through `Self::splice`, which is the one place in this crate
    // that knows a range of bytes moved: that is what carries the marked passages, the folds and the
    // breakpoints along with the text rather than leaving them behind on the wrong line. Each is one
    // `push_undo` and one `mark_changed` however many lines it touched, which is `replace_many`'s
    // rule — one gesture is one thing to undo — and a command that finds nothing to do pushes no
    // undo step at all, for the reason `replace_many` drops an edit that would change no byte.

    /// The lines the selection touches, counting from zero: the first and the last.
    ///
    /// With nothing selected it is the line the caret is on, so the command line can ask for a line
    /// operation on a document nobody has selected anything in. A selection that ends on a line
    /// break touches the line above it rather than the line below, because it holds no byte of the
    /// line below — which is what the byte before the end is for.
    fn touched_lines(&self) -> (usize, usize) {
        let range = self.selection.range();
        if range.is_empty() {
            let line = self.text.byte_to_line(range.start);
            (line, line)
        } else {
            (self.text.byte_to_line(range.start), self.text.byte_to_line(range.end - 1))
        }
    }

    /// The byte range of whole lines `first` to `last`, including the line break that ends `last`.
    ///
    /// The last line of a file that does not end in a line break has no break to include, which is
    /// the one case every command here has to say something about rather than assume.
    fn block_of(&self, first: usize, last: usize) -> Range<usize> {
        let start = self.text.line_to_byte(first);
        let end = match self.line_ends_in_a_break(last) {
            true => self.text.line_to_byte(last + 1),
            false => self.text.len_bytes(),
        };
        start..end.max(start)
    }

    /// True when line `last` ends in a line break, which is the same question as whether there is a
    /// line under it.
    fn line_ends_in_a_break(&self, last: usize) -> bool {
        last + 1 < self.text.len_lines()
    }

    /// The text of one line, without the line break that ends it.
    fn line_text(&self, line: usize) -> String {
        self.text.byte_slice(self.text.line_range(line))
    }

    /// The character formatting of a byte range, as changes measured from `base`.
    ///
    /// What a command that copies or moves a line uses to give it the formatting it had.
    /// [`Self::splice`] only makes room in the span list — see its comment — so text put in carries
    /// the formatting of the place it landed until somebody says otherwise, which is right for a
    /// typed letter and wrong for a duplicated paragraph.
    fn formatting_of(&self, range: Range<usize>, base: usize) -> Vec<(Range<usize>, StyleChange)> {
        let from = range.start;
        self.chars
            .runs_in(range)
            .into_iter()
            .map(|(run, style)| {
                (base + run.start - from..base + run.end - from, style_as_change(style))
            })
            .collect()
    }

    /// Put `text` in at `at`, with the formatting of the place it lands and `over` laid on top.
    ///
    /// `over` holds absolute ranges inside the new text, which is what [`Self::formatting_of`]
    /// answers with. `set_in` rather than `set`, for the reason [`Self::insert`] gives: `set`
    /// rebuilds the whole span list to apply one change.
    fn splice_in(&mut self, at: usize, text: &str, over: &[(Range<usize>, StyleChange)]) {
        if text.is_empty() {
            return;
        }
        let style = self.chars.style_for_insertion(at);
        self.splice(at..at, text);
        self.chars.set_in(at..at + text.len(), &style_as_change(&style), over);
    }

    /// What every command here does once its edits are made.
    ///
    /// The whole file is read again for its colours rather than the stretch that changed. A line
    /// move, a sort or a comment marker changes what the tokens **are** rather than shifting them,
    /// and `Dirt::Part` describes one contiguous edit where these make several — so this is `undo`'s
    /// answer for `undo`'s reason: there is no edit here to be incremental about.
    fn finish_the_line_edit(&mut self) {
        self.syntax_dirt = Dirt::Whole;
        self.desired_x = None;
        self.mark_changed();
        self.last_edit = EditKind::Other;
    }

    /// Move each end of the selection to where the text it was on went.
    fn move_the_selection_by(&mut self, edits: &[(usize, usize, usize)]) {
        let selection = self.selection;
        self.selection.anchor = self.clamp_to_boundary(moved_by(selection.anchor, edits));
        self.selection.head = self.clamp_to_boundary(moved_by(selection.head, edits));
    }

    /// Put the selection back over a block of lines that has moved from `was` to `now`.
    fn put_the_selection_on_the_block(&mut self, was: usize, now: usize, length: usize) {
        let move_it = |offset: usize| now + offset.saturating_sub(was).min(length);
        let selection = self.selection;
        self.selection.anchor = self.clamp_to_boundary(move_it(selection.anchor));
        self.selection.head = self.clamp_to_boundary(move_it(selection.head));
    }

    /// Comment or uncomment every line the selection touches. [`Command::ToggleLineComment`].
    fn toggle_line_comment(&mut self, marker: &str) {
        if marker.trim().is_empty() || marker.contains('\n') {
            return;
        }
        let (first, last) = self.touched_lines();
        let lines: Vec<(usize, String)> = (first..=last)
            .map(|line| (self.text.line_to_byte(line), self.line_text(line)))
            .collect();
        // A wholly blank line is neither asked about nor commented: a marker with nothing after it
        // is an edit the uncommenting press would then have to undo as well, and that is exactly how
        // a toggle stops being its own inverse.
        let said: Vec<&(usize, String)> =
            lines.iter().filter(|(_, text)| !text.trim().is_empty()).collect();
        if said.is_empty() {
            return;
        }
        let commented = said.iter().all(|(_, text)| text.trim_start().starts_with(marker));
        let edits: Vec<(usize, usize, usize)> = match commented {
            true => said
                .iter()
                .map(|(start, text)| (start + leading_whitespace(text).len(), marker.len(), 0))
                .collect(),
            // Every marker goes in at one column, the least indented line's, so a block that was
            // aligned is still aligned once it is commented out.
            false => {
                let column =
                    said.iter().map(|(_, text)| leading_whitespace(text).len()).min().unwrap_or(0);
                said.iter().map(|(start, _)| (start + column, 0, marker.len())).collect()
            }
        };
        self.push_undo(EditKind::Other);
        // Back to front, so an earlier line's start is not moved by a later line's edit, which is
        // `indent`'s rule and the reason `edits` stays in file order for the selection to read.
        for &(at, removed, _) in edits.iter().rev() {
            match removed {
                0 => self.splice_in(at, marker, &[]),
                _ => self.splice(at..at + removed, ""),
            }
        }
        self.move_the_selection_by(&edits);
        self.finish_the_line_edit();
    }

    /// Wrap the selection in block comment markers, or take them off again.
    /// [`Command::ToggleBlockComment`].
    fn toggle_block_comment(&mut self, open: &str, close: &str) {
        if open.is_empty() || close.is_empty() || open.contains('\n') || close.contains('\n') {
            return;
        }
        let asked_about_a_line = self.selection.is_empty();
        let range = match asked_about_a_line {
            true => self.text.line_range(self.text.byte_to_line(self.selection.head)),
            false => self.selection.range(),
        };
        let text = self.text.byte_slice(range.clone());
        // The markers hug the words rather than the indentation, so that the second press gives back
        // the bytes the first one changed whichever end of a line the selection began at.
        let lead = text.len() - text.trim_start().len();
        let tail = text.len() - text.trim_end().len();
        let inner = range.start + lead..(range.end - tail).max(range.start + lead);
        let body = text.trim();
        let commented = body.len() >= open.len() + close.len()
            && body.starts_with(open)
            && body.ends_with(close);
        self.push_undo(EditKind::Other);
        if commented {
            self.splice(inner.end - close.len()..inner.end, "");
            self.splice(inner.start..inner.start + open.len(), "");
            let end = inner.end - open.len() - close.len();
            self.selection = match asked_about_a_line {
                true => Selection::caret(inner.start),
                false => Selection::new(inner.start, end),
            };
        } else {
            self.splice_in(inner.end, close, &[]);
            self.splice_in(inner.start, open, &[]);
            self.selection = match asked_about_a_line {
                // The caret lands inside the markers, which is where the next word goes.
                true => Selection::caret(inner.start + open.len()),
                false => Selection::new(inner.start, inner.end + open.len() + close.len()),
            };
        }
        self.finish_the_line_edit();
    }

    /// Copy the lines the selection touches in below themselves. [`Command::DuplicateLines`].
    fn duplicate_lines(&mut self) {
        let (first, last) = self.touched_lines();
        let block = self.block_of(first, last);
        if block.is_empty() {
            return;
        }
        let text = self.text.byte_slice(block.clone());
        let at = block.end;
        // A block that is the last line of a file with no line break at the end of it needs one in
        // front of the copy, or the line and its copy arrive as one line.
        let (put, base) = match text.ends_with('\n') {
            true => (text, at),
            false => (format!("\n{text}"), at + 1),
        };
        let formatting = self.formatting_of(block, base);
        self.push_undo(EditKind::Other);
        self.splice_in(at, &put, &formatting);
        // The selection does not move, because everything it covers is in front of the copy. That is
        // `indent`'s promise kept: pressing the key again duplicates the same lines again.
        self.finish_the_line_edit();
    }

    /// Move the lines the selection touches up or down. [`Command::MoveLines`].
    fn move_lines(&mut self, by: i32) {
        if by == 0 {
            return;
        }
        let down = by > 0;
        let (first, last) = self.touched_lines();
        let can_move = match down {
            true => self.line_ends_in_a_break(last),
            false => first > 0,
        };
        if !can_move {
            return;
        }
        self.push_undo(EditKind::Other);
        for _ in 0..by.unsigned_abs() {
            if !self.move_the_lines_one_line(down) {
                break;
            }
        }
        self.finish_the_line_edit();
    }

    /// One line of it, and the whole of the reason this is two splices rather than one.
    ///
    /// **The line that is swapped past is what moves; the block stays where it is in the rope.** A
    /// block lifted out and put back would lose every mark, fold and breakpoint on it, because
    /// [`Self::splice`] is told that a range went and each of those three drops or collapses what
    /// was inside it. Shifting the block by an edit on the other side of it carries all three
    /// instead. The line it swaps with pays that price, which is the right way round: the block is
    /// what somebody selected.
    ///
    /// Going down, the neighbour comes back in **in front of** the block's first byte rather than at
    /// it, by carrying a line break across with it. `Folds::insert` and `Breakpoints::insert`
    /// deliberately leave an offset that is exactly at an insertion point where it is, so putting
    /// the neighbour in at the block's own start would leave a fold or a breakpoint on the block's
    /// first line behind. The one place with nothing in front of it to insert at is byte zero, and
    /// there a fold or a breakpoint on the first line of the file does stay where it is.
    fn move_the_lines_one_line(&mut self, down: bool) -> bool {
        let (first, last) = self.touched_lines();
        let block = self.block_of(first, last);
        let block_ends_in_a_break = self.line_ends_in_a_break(last);
        if down {
            if !block_ends_in_a_break {
                return false;
            }
            let next = self.block_of(last + 1, last + 1);
            let next_ends_in_a_break = self.line_ends_in_a_break(last + 1);
            // The neighbour and one line break: its own where it has one, and otherwise the block's,
            // which the block gives up because it is about to be the last line of the file.
            let (removal, body) = match next_ends_in_a_break {
                true => (next.start..next.end, next.start..next.end - 1),
                false => (next.start - 1..next.end, next.start..next.end),
            };
            let text = self.text.byte_slice(body.clone());
            let (at, put) = match block.start {
                0 => (0, format!("{text}\n")),
                start => (start - 1, format!("\n{text}")),
            };
            let base = at + usize::from(block.start > 0);
            let formatting = self.formatting_of(body, base);
            let length = block.len() - usize::from(!next_ends_in_a_break);
            let put_length = put.len();
            self.splice(removal, "");
            self.splice_in(at, &put, &formatting);
            self.put_the_selection_on_the_block(block.start, block.start + put_length, length);
        } else {
            if first == 0 {
                return false;
            }
            let previous = self.block_of(first - 1, first - 1);
            // The line above the block always ends in a line break, because the block is under it.
            let body = previous.start..previous.end - 1;
            let text = self.text.byte_slice(body.clone());
            let put = match block_ends_in_a_break {
                true => format!("{text}\n"),
                false => format!("\n{text}"),
            };
            let at = block.end - previous.len();
            let base = at + usize::from(!block_ends_in_a_break);
            let formatting = self.formatting_of(body, base);
            self.splice(previous.clone(), "");
            self.splice_in(at, &put, &formatting);
            self.put_the_selection_on_the_block(block.start, previous.start, block.len());
        }
        true
    }

    /// Join the lines the selection touches into one. [`Command::JoinLines`].
    fn join_lines(&mut self) {
        let (first, last) = self.touched_lines();
        // With nothing selected the line below is pulled up, so the chord can be pressed again to
        // pull up the next one.
        let seams = match last > first {
            true => last - first,
            false => 1,
        };
        // Forwards, joining the first line to the one under it over and over, rather than reading
        // every seam first and applying them back to front. Each join is decided against the text as
        // it is by then, which is what puts one space into `one`, a blank line, `two`: the blank line
        // goes with the first join and the second join then has words on both sides of it.
        let mut seam = None;
        for _ in 0..seams {
            if !self.line_ends_in_a_break(first) {
                break;
            }
            let above = self.text.line_range(first);
            let below = self.text.line_range(first + 1);
            let above_text = self.text.byte_slice(above.clone());
            let below_text = self.text.byte_slice(below.clone());
            let from = above.start + above_text.trim_end().len();
            let to = below.start + leading_whitespace(&below_text).len();
            // No space where there is nothing on one side of the seam to put one between.
            let between = match above_text.trim().is_empty() || below_text.trim().is_empty() {
                true => "",
                false => " ",
            };
            if seam.is_none() {
                self.push_undo(EditKind::Other);
                seam = Some(from);
            }
            self.splice(from..to, "");
            self.splice_in(from, between, &[]);
        }
        // The caret lands on the first seam, which is the one place from which the chord pressed
        // again pulls up the next line.
        let Some(seam) = seam else {
            return;
        };
        self.selection = Selection::caret(self.clamp_to_boundary(seam));
        self.finish_the_line_edit();
    }

    /// Sort the lines the selection touches, stably, in byte order. [`Command::SortLines`].
    fn sort_lines(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let (first, last) = self.touched_lines();
        if last <= first {
            return;
        }
        let lines: Vec<(Range<usize>, String)> = (first..=last)
            .map(|line| {
                let range = self.text.line_range(line);
                (range.clone(), self.text.byte_slice(range))
            })
            .collect();
        // Which line ends up where, rather than the sorted text, so that each line's character
        // formatting travels with it. `sort_by` is stable, which is what keeps two equal lines in
        // the order somebody wrote them.
        let mut order: Vec<usize> = (0..lines.len()).collect();
        order.sort_by(|a, b| lines[*a].1.cmp(&lines[*b].1));
        if order.iter().enumerate().all(|(to, from)| lines[*from].1 == lines[to].1) {
            return;
        }
        let formatting: Vec<Vec<(Range<usize>, StyleChange)>> =
            lines.iter().map(|(range, _)| self.formatting_of(range.clone(), 0)).collect();
        self.push_undo(EditKind::Other);
        // Back to front, and each line **put in before its old content is taken out**. A fold and a
        // breakpoint sit at the start of their line and `Folds::remove` drops an offset inside what
        // went, so writing a line the other way round would take the arrow off it. What cannot
        // survive is a marked passage on a line the sort moved: the words it was drawn over are not
        // on that line any more, and no shift of an offset can say where they went.
        for (to, from) in order.iter().enumerate().rev() {
            let now = lines[*from].1.clone();
            let range = lines[to].0.clone();
            if now == lines[to].1 {
                continue;
            }
            let over = formatting_at(&formatting[*from], range.start);
            self.splice_in(range.start, &now, &over);
            self.splice(range.start + now.len()..range.end + now.len(), "");
        }
        // Over the lines it sorted, so that sorting again sorts the same lines.
        let block = self.block_of(first, last);
        self.selection = Selection::new(block.start, block.end);
        self.finish_the_line_edit();
    }

    /// Take the trailing whitespace off every line. [`Command::TrimTrailingWhitespace`].
    fn trim_trailing_whitespace(&mut self) {
        // One reading of the text rather than one slice a line: this runs over a whole file, and a
        // file with no trailing whitespace in it — which is most of them — has to be cheap to ask.
        let text = self.text.to_string();
        let mut edits: Vec<(usize, usize, usize)> = Vec::new();
        let mut at = 0;
        for line in text.split('\n') {
            let keep = line.trim_end().len();
            if keep < line.len() {
                edits.push((at + keep, line.len() - keep, 0));
            }
            at += line.len() + 1;
        }
        if edits.is_empty() {
            return;
        }
        self.push_undo(EditKind::Other);
        for &(from, removed, _) in edits.iter().rev() {
            self.splice(from..from + removed, "");
        }
        self.move_the_selection_by(&edits);
        self.finish_the_line_edit();
    }

    /// The bracket the caret is on and the one that answers it.
    ///
    /// The caret is between two bytes, so both are asked about: the bracket it is in front of first
    /// and then the one behind it, which is what makes the answer the same whichever side of a `)`
    /// somebody put it.
    ///
    /// `read` is every comment and string in the file, so that a `}` inside `// }` or inside `"}"`
    /// is not a bracket. It is handed in rather than read here for the reason
    /// [`crate::folding::regions_from`] takes one: the window has already read the file for its
    /// colours at this text revision, and a second `syntax::scan` was worth 2.5 ms a keystroke on
    /// the largest file in this repository. A caller with nothing read hands in
    /// `&folding::Tokens::default()` and gets an answer that does not know about comments.
    ///
    /// Nesting is counted over the one pair of brackets rather than over all three, which is the
    /// tier this is: half-typed source is the ordinary state of a file being edited, and a stack
    /// over all three would have to decide what a stray closer of another kind means.
    ///
    /// The search is bounded at [`BRACKET_SEARCH_LIMIT`] bytes each way. A pair further apart than
    /// that is a pair nobody is reading both ends of at once, and the alternative is walking a two
    /// megabyte file every time the caret moves.
    pub fn bracket_pair(
        &self,
        offset: usize,
        read: &crate::folding::Tokens,
    ) -> Option<(usize, usize)> {
        let offset = offset.min(self.text.len_bytes());
        let mut candidates = Vec::with_capacity(2);
        if offset < self.text.len_bytes() {
            candidates.push(offset);
        }
        if offset > 0 {
            candidates.push(offset - 1);
        }
        for at in candidates {
            let Some(byte) = self.ascii_at(at) else { continue };
            let Some((open, close, forwards)) = bracket(byte) else { continue };
            if read.covers(at) {
                continue;
            }
            let other = match forwards {
                true => self.match_forwards(at, open, close, read),
                false => self.match_backwards(at, open, close, read),
            };
            if let Some(other) = other {
                return Some((at, other));
            }
        }
        None
    }

    /// Where the bracket at `offset` is closed or opened.
    ///
    /// [`Self::bracket_pair`] is the same answer with the bracket it was asked about beside it,
    /// which is what a painter drawing both ends needs and what `editor bracket` prints.
    pub fn matching_bracket(&self, offset: usize, read: &crate::folding::Tokens) -> Option<usize> {
        self.bracket_pair(offset, read).map(|(_, other)| other)
    }

    /// The indentation a new line started at the caret should begin with.
    ///
    /// The reckoning is [`indentation_for_a_new_line`], which is a pure function over a text and an
    /// offset; this is it asked of the caret's own line rather than of the whole document, so the
    /// window does not have to build the file's text to ask a question about one line.
    pub fn indentation_for_a_new_line(&self) -> String {
        let (window, base) = self.line_window(self.selection.head);
        indentation_for_a_new_line(&window, self.selection.head - base).to_owned()
    }

    /// The byte at `at`, when a whole character starts and ends around it — so a byte in the middle
    /// of a multi-byte character answers nothing rather than answering about half of one.
    fn ascii_at(&self, at: usize) -> Option<u8> {
        let end = at + 1;
        if end > self.text.len_bytes()
            || !self.text.is_char_boundary(at)
            || !self.text.is_char_boundary(end)
        {
            return None;
        }
        self.text.byte_slice(at..end).bytes().next()
    }

    /// The closer that answers the opener at `from`.
    fn match_forwards(
        &self,
        from: usize,
        open: u8,
        close: u8,
        read: &crate::folding::Tokens,
    ) -> Option<usize> {
        let end = self.clamp_to_boundary((from + BRACKET_SEARCH_LIMIT).min(self.text.len_bytes()));
        let window = self.text.byte_slice(from..end);
        let mut depth = 0usize;
        for (index, byte) in window.bytes().enumerate() {
            if byte != open && byte != close {
                continue;
            }
            let at = from + index;
            if read.covers(at) {
                continue;
            }
            if byte == open {
                depth += 1;
                continue;
            }
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at);
            }
        }
        None
    }

    /// The opener that the closer at `from` answers.
    fn match_backwards(
        &self,
        from: usize,
        open: u8,
        close: u8,
        read: &crate::folding::Tokens,
    ) -> Option<usize> {
        let start = self.clamp_to_boundary(from.saturating_sub(BRACKET_SEARCH_LIMIT));
        let window = self.text.byte_slice(start..from + 1);
        let mut depth = 0usize;
        for (index, byte) in window.bytes().enumerate().rev() {
            if byte != open && byte != close {
                continue;
            }
            let at = start + index;
            if read.covers(at) {
                continue;
            }
            if byte == close {
                depth += 1;
                continue;
            }
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at);
            }
        }
        None
    }

    fn remove_range(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        self.syntax_dirt = self.syntax_dirt.note(range.start, range.end - range.start, 0);
        self.splice(range.clone(), "");
        self.selection.set_caret(range.start);
    }

    fn delete_backward(&mut self) {
        self.push_undo(EditKind::Deleting);
        if !self.selection.is_empty() {
            let range = self.selection.range();
            self.remove_range(range);
        } else if self.selection.head > 0 {
            let (window, base) = self.line_window(self.selection.head);
            let from = if self.selection.head == base {
                // At the start of a line, backspace joins this line to the one before it.
                self.selection.head - 1
            } else {
                cursor::prev_grapheme(&window, base, self.selection.head)
            };
            self.remove_range(from..self.selection.head);
        } else {
            return;
        }
        self.desired_x = None;
        self.mark_changed();
        self.last_edit = EditKind::Deleting;
    }

    fn delete_forward(&mut self) {
        self.push_undo(EditKind::Deleting);
        if !self.selection.is_empty() {
            let range = self.selection.range();
            self.remove_range(range);
        } else if self.selection.head < self.text.len_bytes() {
            let (window, base) = self.line_window(self.selection.head);
            let to = if self.selection.head == base + window.len() {
                self.selection.head + 1
            } else {
                cursor::next_grapheme(&window, base, self.selection.head)
            };
            self.remove_range(self.selection.head..to);
        } else {
            return;
        }
        self.desired_x = None;
        self.mark_changed();
        self.last_edit = EditKind::Deleting;
    }

    fn delete_word_backward(&mut self) {
        self.push_undo(EditKind::Other);
        if !self.selection.is_empty() {
            let range = self.selection.range();
            self.remove_range(range);
        } else {
            let (window, base) = self.line_window(self.selection.head);
            let from = cursor::prev_word(&window, base, self.selection.head);
            if from == self.selection.head {
                return;
            }
            self.remove_range(from..self.selection.head);
        }
        self.desired_x = None;
        self.mark_changed();
    }

    fn move_horizontally(&mut self, direction: i32, extend: bool, by_word: bool) {
        let selection_before = self.selection;
        // With a selection and no shift held, an arrow key collapses to the near edge rather than
        // moving past it, which is what a writer expects.
        if !extend && !self.selection.is_empty() {
            let range = self.selection.range();
            let at = if direction < 0 { range.start } else { range.end };
            self.selection.set_caret(at);
            self.desired_x = None;
            self.caret_moved(selection_before);
            return;
        }
        let head = self.selection.head;
        let (window, base) = self.line_window(head);
        let target = match (direction < 0, by_word) {
            (true, false) => {
                if head == base && head > 0 {
                    head - 1 // cross the line break into the line above
                } else {
                    cursor::prev_grapheme(&window, base, head)
                }
            }
            (false, false) => {
                if head == base + window.len() && head < self.text.len_bytes() {
                    head + 1 // cross the line break into the line below
                } else {
                    cursor::next_grapheme(&window, base, head)
                }
            }
            (true, true) => {
                let at = cursor::prev_word(&window, base, head);
                if at == head && head > 0 {
                    head - 1
                } else {
                    at
                }
            }
            (false, true) => {
                let at = cursor::next_word(&window, base, head);
                if at == head && head < self.text.len_bytes() {
                    head + 1
                } else {
                    at
                }
            }
        };
        self.selection.move_to(target.min(self.text.len_bytes()), extend);
        self.desired_x = None;
        self.caret_moved(selection_before);
    }

    fn apply_style(&mut self, change: StyleChange) {
        if change.is_empty() {
            return;
        }
        if self.selection.is_empty() {
            // Nothing is selected, so remember the choice and apply it to the next text typed. This is
            // how pressing bold and then typing works in every word processor.
            change.apply_over_change(&mut self.pending);
            self.revision += 1;
            return;
        }
        self.push_undo(EditKind::Other);
        let range = self.selection.range();
        // `set_in` rather than `set`: pressing bold on one word of a coloured file rebuilt every span
        // in it. One call rather than hundreds, so this is the smallest of the three, and it is the
        // same change for the same reason. `task-1984` C6.
        self.chars.set_in(range.clone(), &change, &[]);
        self.note_paragraphs(
            self.text.byte_to_line(range.start),
            self.text.byte_to_line(range.end),
        );
        self.mark_changed();
    }
}

/// Turn a full style into a change that sets every field, so that inserted text is given exactly that
/// style rather than inheriting from its neighbours.
/// How far a bracket is looked for, in bytes, in each direction.
///
/// A pair further apart than this is a pair nobody is reading both ends of at once, and the question
/// is asked every time the caret moves — so the alternative is walking a two megabyte file to answer
/// it. `folding::regions` reads the whole file, but it reads it once a change rather than once a
/// frame.
pub const BRACKET_SEARCH_LIMIT: usize = 64 * 1024;

/// The indentation a new line started at `at` should begin with: the spaces and tabs the line
/// holding `at` begins with, and no more of them than are in front of `at`.
///
/// **The reckoning only.** Whether a new line gets it is the window's question, because the setting
/// that answers it is the window's — the split [`crate::folding::regions`] and [`crate::Folds`]
/// already are. What the window does with the answer is insert `"\n"` and this as one
/// [`Command::Insert`], so a new line and its indentation are one thing to undo.
///
/// Clamped to `at` so that pressing Enter in the middle of a line's indentation does not double it:
/// the whitespace after the caret goes down to the new line by itself.
pub fn indentation_for_a_new_line(text: &str, at: usize) -> &str {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    let start = text[..at].rfind('\n').map_or(0, |line_break| line_break + 1);
    let end = text[start..at]
        .find(|character: char| character != ' ' && character != '\t')
        .map_or(at, |offset| start + offset);
    &text[start..end]
}

/// The run of spaces and tabs a line begins with.
fn leading_whitespace(line: &str) -> &str {
    let end =
        line.find(|character: char| character != ' ' && character != '\t').unwrap_or(line.len());
    &line[..end]
}

/// What a bracket is: the pair it belongs to, and whether its partner lies after it.
fn bracket(byte: u8) -> Option<(u8, u8, bool)> {
    match byte {
        b'{' => Some((b'{', b'}', true)),
        b'}' => Some((b'{', b'}', false)),
        b'[' => Some((b'[', b']', true)),
        b']' => Some((b'[', b']', false)),
        b'(' => Some((b'(', b')', true)),
        b')' => Some((b'(', b')', false)),
        _ => None,
    }
}

/// The same formatting, measured from a different place.
fn formatting_at(
    runs: &[(Range<usize>, StyleChange)],
    base: usize,
) -> Vec<(Range<usize>, StyleChange)> {
    runs.iter()
        .map(|(range, change)| (base + range.start..base + range.end, change.clone()))
        .collect()
}

/// Where an offset ends up after a list of edits, each of them `(at, removed, inserted)` in the
/// coordinates of the text before any of them was made.
///
/// The edits must not overlap. An offset at or past the end of one moves by the difference; one
/// inside what went lands at the start of it; and one **exactly at** an insertion point moves
/// forward, so a selection that began where a comment marker was put still covers the words it
/// covered. That last rule is `Document::indent`'s, written down once here rather than as a closure
/// in each of the commands that needs it.
fn moved_by(offset: usize, edits: &[(usize, usize, usize)]) -> usize {
    let mut delta: isize = 0;
    for &(at, removed, inserted) in edits {
        if offset >= at + removed {
            delta += inserted as isize - removed as isize;
        } else if offset > at {
            delta -= (offset - at) as isize;
        }
    }
    (offset as isize + delta).max(0) as usize
}

fn style_as_change(style: &CharStyle) -> StyleChange {
    StyleChange {
        family: Some(style.family.clone()),
        size: Some(style.size),
        bold: Some(style.bold),
        italic: Some(style.italic),
        underline: Some(style.underline),
        strikethrough: Some(style.strikethrough),
        color: Some(style.color),
    }
}

impl StyleChange {
    /// Apply this change on top of a style.
    fn apply_over(self, style: &mut CharStyle) {
        if let Some(family) = self.family {
            style.family = family;
        }
        if let Some(size) = self.size {
            style.size = size;
        }
        if let Some(bold) = self.bold {
            style.bold = bold;
        }
        if let Some(italic) = self.italic {
            style.italic = italic;
        }
        if let Some(underline) = self.underline {
            style.underline = underline;
        }
        if let Some(strikethrough) = self.strikethrough {
            style.strikethrough = strikethrough;
        }
        if let Some(color) = self.color {
            style.color = color;
        }
    }

    /// Fold this change into another one, so that pressing bold and then italic before typing keeps
    /// both.
    fn apply_over_change(self, other: &mut StyleChange) {
        if self.family.is_some() {
            other.family = self.family;
        }
        if self.size.is_some() {
            other.size = self.size;
        }
        if self.bold.is_some() {
            other.bold = self.bold;
        }
        if self.italic.is_some() {
            other.italic = self.italic;
        }
        if self.underline.is_some() {
            other.underline = self.underline;
        }
        if self.strikethrough.is_some() {
            other.strikethrough = self.strikethrough;
        }
        if self.color.is_some() {
            other.color = self.color;
        }
    }
}

#[cfg(test)]
mod base_style_tests {
    use super::*;
    use crate::style::Color;

    fn document_with_a_bold_red_word() -> Document {
        let mut document = Document::from_text("plain BOLD plain");
        document.apply(Command::PlaceCaret { offset: 6, extend: false });
        document.apply(Command::PlaceCaret { offset: 10, extend: true });
        document.apply(Command::ToggleBold);
        document.apply(Command::ApplyStyle(StyleChange::color(Color::RED)));
        document.apply(Command::PlaceCaret { offset: 0, extend: false });
        document
    }

    #[test]
    fn the_base_style_changes_the_family_and_size_of_the_whole_document() {
        let mut document = Document::from_text("two lines here\nand the second");
        document.set_base_style(StyleChange { size: Some(28.0), ..StyleChange::family("Courier") });
        for offset in [0, 5, 20, document.text().len_bytes() - 1] {
            let style = document.chars().style_at(offset);
            assert_eq!(&*style.family, "Courier", "offset {offset} should have the new family");
            assert_eq!(style.size, 28.0, "offset {offset} should have the new size");
        }
    }

    #[test]
    fn the_base_style_leaves_formatting_it_does_not_name_alone() {
        let mut document = document_with_a_bold_red_word();
        document.set_base_style(StyleChange::family("Courier"));
        let word = document.chars().style_at(7);
        assert!(word.bold, "the word that was made bold should still be bold");
        assert_eq!(word.color, Color::RED, "and still red");
        assert_eq!(&*word.family, "Courier", "with the new family under it");
        assert!(!document.chars().style_at(0).bold, "the words either side are still not bold");
    }

    #[test]
    fn the_base_style_is_not_an_edit() {
        let mut document = Document::from_text("nothing here has been edited");
        assert!(!document.is_modified());
        let revision = document.revision();
        document.set_base_style(StyleChange::size(30.0));
        assert!(
            !document.is_modified(),
            "a font setting is not a change to the file, which holds no formatting"
        );
        assert!(!document.can_undo(), "and there is nothing to undo");
        assert!(document.revision() > revision, "but the text has to be laid out again");
    }

    #[test]
    fn the_base_style_applies_to_text_typed_afterwards() {
        let mut document = Document::new();
        document.set_base_style(StyleChange::size(30.0));
        document.apply(Command::Insert("typed after the setting".to_owned()));
        assert_eq!(document.chars().style_at(3).size, 30.0);
    }

    #[test]
    fn an_empty_change_does_nothing_at_all() {
        let mut document = Document::from_text("unchanged");
        let revision = document.revision();
        document.set_base_style(StyleChange::default());
        assert_eq!(document.revision(), revision);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout;
    use crate::metrics::FixedMetrics;
    use crate::style::Color;

    /// A rename inside an open document is **one** undo step, however many places it touched.
    ///
    /// It follows from `push_undo` being called once and then every edit being made, rather than
    /// from anything `ReplaceMany` arranges: undo in Unluminous restores a state, so the state it
    /// restores is the one before the first of them.
    #[test]
    fn replacing_many_ranges_is_one_edit_to_undo() {
        let mut document = Document::new();
        document.apply(Command::Insert("let value = value + value;".to_owned()));
        let before = document.text().to_string();
        let edits = unluminous_core_replacements(&before, "value", "total");
        assert_eq!(edits.len(), 3);
        assert!(document.apply(Command::ReplaceMany(edits)));
        assert_eq!(document.text().to_string(), "let total = total + total;");
        assert!(document.is_modified());
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), before, "one step puts all three back");
    }

    /// And the marked passages come back with it, because they ride the snapshot.
    #[test]
    fn undoing_a_rename_brings_back_the_marks_it_moved() {
        let mut document = Document::new();
        document.apply(Command::Insert("let value = 1;\nlet other = value;".to_owned()));
        // Mark `other`, which sits after the first replacement and so is shifted by it.
        let at = document.text().to_string().find("other").expect("other");
        document.set_highlights(Highlights::from_list([crate::highlights::Highlight {
            range: at..at + 5,
            color: Rgba::parse("#ffff0080").expect("a colour"),
        }]));
        let text = document.text().to_string();
        document.apply(Command::ReplaceMany(unluminous_core_replacements(&text, "value", "v")));
        let moved = document.highlights().iter().next().expect("the mark").range.clone();
        assert_eq!(
            &document.text().to_string()[moved.clone()],
            "other",
            "the mark moved with the text: {moved:?}"
        );
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), text);
        assert_eq!(
            document.highlights().iter().next().expect("the mark").range,
            at..at + 5,
            "and undo restored where it was"
        );
    }

    // ------------------------------------------------------------------------------ the breakpoints

    /// The whole reason breakpoints are byte offsets rather than line numbers: a line typed at the
    /// top of the file moves them, in the one place that knows the text moved.
    #[test]
    fn a_breakpoint_stays_on_its_line_while_the_file_is_edited_above_it() {
        let mut document = Document::from_text(
            "one
two
three
",
        );
        let two = document.text().to_string().find("two").expect("two");
        assert!(document.toggle_breakpoint(two + 1), "a caret anywhere in the line means the line");
        assert_eq!(document.line_number_of(document.breakpoints().all()[0].offset), 2);
        document.apply(Command::MoveDocumentStart { extend: false });
        document.apply(Command::Insert(
            "zero
"
            .to_owned(),
        ));
        assert_eq!(
            document.line_number_of(document.breakpoints().all()[0].offset),
            3,
            "the same line of the program, one further down the file"
        );
        let at = document.breakpoints().all()[0].offset;
        assert_eq!(&document.text().to_string()[at..at + 3], "two");
    }

    /// Toggling one is not an edit, which is the rule the marked passages and the folds follow: the
    /// window repaints, the file is not modified, and there is nothing to undo.
    #[test]
    fn putting_a_breakpoint_on_a_line_is_not_an_edit() {
        let mut document = Document::from_text(
            "one
two
",
        );
        let revision = document.revision();
        let text_revision = document.text_revision();
        assert!(document.toggle_breakpoint(4));
        assert!(document.revision() > revision, "the gutter has to be drawn again");
        assert_eq!(document.text_revision(), text_revision, "nothing was laid out again");
        assert!(
            !document.is_modified(),
            "what Unluminous saves is plain text, and a dot is not in it"
        );
        assert!(!document.can_undo());
    }

    /// It does ride the undo snapshot, because undo restores a state.
    #[test]
    fn undoing_an_edit_brings_the_breakpoints_back_where_they_were() {
        let mut document = Document::from_text(
            "one
two
three
",
        );
        document.toggle_breakpoint(document.offset_of_line_number(3));
        let before = document.breakpoints().clone();
        document.apply(Command::MoveDocumentStart { extend: false });
        document.apply(Command::Insert(
            "zero
minus
"
            .to_owned(),
        ));
        assert_ne!(document.breakpoints(), &before, "the edit moved it");
        document.apply(Command::Undo);
        assert_eq!(document.breakpoints(), &before, "and undo restored where it was");
    }

    #[test]
    fn a_second_click_on_a_line_takes_the_breakpoint_away() {
        let mut document = Document::from_text(
            "one
two
",
        );
        assert!(document.toggle_breakpoint(5));
        // A different byte of the same line is the same line, which is what makes the gutter's row
        // and the caret's position one question.
        assert!(!document.toggle_breakpoint(6));
        assert!(document.breakpoints().is_empty());
    }

    #[test]
    fn the_two_line_conversions_are_each_others_inverse() {
        let document = Document::from_text(
            "one
two
three
",
        );
        for line in 1..=3 {
            let at = document.offset_of_line_number(line);
            assert_eq!(document.line_number_of(at), line);
            assert_eq!(
                document.line_start_of(at + 1),
                at,
                "any byte of the line snaps to its start"
            );
        }
    }

    /// A set read from a file that was rewritten outside Unluminous is brought inside the text rather
    /// than left to panic the layout engine.
    #[test]
    fn a_set_put_back_is_clamped_to_the_text_it_lands_in() {
        let mut document = Document::from_text(
            "short
",
        );
        assert!(document.set_breakpoints(crate::breakpoints::Breakpoints::from_list([
            crate::breakpoints::Breakpoint::at(0),
            crate::breakpoints::Breakpoint::at(9000),
        ])));
        assert!(document.breakpoints().check());
        assert!(document
            .breakpoints()
            .all()
            .iter()
            .all(|one| one.offset <= document.text().len_bytes()));
    }

    #[test]
    fn changing_a_breakpoint_that_is_not_there_changes_nothing() {
        let mut document = Document::from_text(
            "one
",
        );
        assert!(!document.change_breakpoint(0, |one| one.enabled = false));
        document.toggle_breakpoint(0);
        assert!(document.change_breakpoint(0, |one| one.enabled = false));
        assert!(!document.breakpoints().at(0).expect("still there").enabled);
        assert!(
            !document.change_breakpoint(0, |one| one.enabled = false),
            "setting it to what it already is changes nothing"
        );
    }

    #[test]
    fn a_replacement_range_outside_the_text_is_left_alone_rather_than_panicking() {
        // A `Command` can be built by hand — the command line builds one — so nothing here trusts
        // what it is given.
        let mut document = Document::new();
        document.apply(Command::Insert("short".to_owned()));
        assert!(!document.apply(Command::ReplaceMany(vec![(40..50, "x".to_owned())])));
        assert!(!document.apply(Command::ReplaceMany(Vec::new())));
        assert_eq!(document.text().to_string(), "short");
        // Overlapping ranges are applied once rather than twice.
        document.apply(Command::ReplaceMany(vec![(0..3, "A".to_owned()), (1..4, "B".to_owned())]));
        assert_eq!(document.text().to_string(), "Art", "the earlier of two overlapping edits wins");
    }

    #[test]
    fn an_empty_range_carrying_text_is_an_insertion_and_one_carrying_nothing_is_dropped() {
        // `task-1680`: accepting `./layout` inside `from '|'` replaces no bytes and inserts all of
        // them, and it has to be the same one-undo-step command every other completion is.
        let mut document = Document::new();
        assert!(document.apply(Command::ReplaceMany(vec![(0..0, "hello".to_owned())])));
        assert_eq!(document.text().to_string(), "hello");
        assert!(document.apply(Command::ReplaceMany(vec![(5..5, " there".to_owned())])));
        assert_eq!(document.text().to_string(), "hello there");
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "hello", "one step, as every other edit is");
        // An empty range with nothing in it would be an undo step for an edit that changes no byte.
        assert!(!document.apply(Command::ReplaceMany(vec![(2..2, String::new())])));
        assert_eq!(document.text().to_string(), "hello");
    }

    /// The ranges of a rename built the way the modal builds them.
    fn unluminous_core_replacements(
        text: &str,
        name: &str,
        to: &str,
    ) -> Vec<(Range<usize>, String)> {
        let grammar = crate::syntax::Grammar {
            word_characters: Vec::new(),
            ..crate::syntax::Grammar::default()
        };
        let ranges: Vec<Range<usize>> = crate::symbols::occurrences(text, name, &grammar)
            .into_iter()
            .map(|found| found.range)
            .collect();
        crate::symbols::replacements(text, &ranges, to)
    }

    fn typed(text: &str) -> Document {
        let mut document = Document::new();
        document.apply(Command::Insert(text.to_owned()));
        document
    }

    fn lay_out(document: &Document, width: f32) -> Layout {
        layout(
            document.text(),
            document.chars(),
            document.paragraphs(),
            &FixedMetrics::default(),
            width,
        )
    }

    /// A document with something of everything in it, so that a command applied to it has something
    /// to do: several paragraphs, a bold word, a coloured word, a marked passage and a selection.
    fn a_document_with_something_of_everything() -> Document {
        let mut document = Document::from_text(
            "the first line
the second line

the fourth line",
        );
        document.apply(Command::PlaceCaret { offset: 4, extend: false });
        document.apply(Command::PlaceCaret { offset: 9, extend: true });
        document.apply(Command::ToggleBold);
        document.set_syntax(Color::WHITE, &[(0..3, Color::BLUE), (15..18, Color::GREEN)]);
        document.highlight(20..26, Rgba::new(0xFF, 0xC0, 0x40, 0x60));
        document.apply(Command::PlaceCaret { offset: 20, extend: false });
        // Ending in the middle of a line rather than at the end of one, so that a command such as
        // `MoveLineEnd` has somewhere to go.
        document.apply(Command::PlaceCaret { offset: 26, extend: true });
        document
    }

    /// Every command, in the order they are declared, so that one added later is added here too.
    fn every_command() -> Vec<Command> {
        vec![
            Command::Insert("x".to_owned()),
            Command::Insert(
                "
"
                .to_owned(),
            ),
            Command::DeleteBackward,
            Command::DeleteForward,
            Command::DeleteWordBackward,
            Command::MoveLeft { extend: false },
            Command::MoveLeft { extend: true },
            Command::MoveRight { extend: false },
            Command::MoveRight { extend: true },
            Command::MoveWordLeft { extend: false },
            Command::MoveWordRight { extend: true },
            Command::MoveLineStart { extend: false },
            Command::MoveLineEnd { extend: true },
            Command::MoveDocumentStart { extend: false },
            Command::MoveDocumentEnd { extend: true },
            Command::PlaceCaret { offset: 7, extend: false },
            Command::PlaceCaret { offset: 12, extend: true },
            Command::ReplaceMany(vec![(3..5, "XY".to_owned())]),
            Command::ReplaceMany(vec![(6..8, "a much longer replacement".to_owned())]),
            Command::ReplaceMany(Vec::new()),
            Command::Indent { unit: IndentUnit::Tab },
            Command::Indent { unit: IndentUnit::Space },
            Command::Dedent { unit: IndentUnit::Tab },
            Command::Dedent { unit: IndentUnit::Space },
            Command::ToggleLineComment { marker: "//".to_owned() },
            Command::ToggleLineComment { marker: "# ".to_owned() },
            Command::ToggleBlockComment { open: "/*".to_owned(), close: "*/".to_owned() },
            Command::DuplicateLines,
            Command::MoveLines { by: -1 },
            Command::MoveLines { by: 1 },
            Command::JoinLines,
            Command::SortLines,
            Command::TrimTrailingWhitespace,
            Command::SelectAll,
            Command::ApplyStyle(StyleChange::size(28.0)),
            Command::ToggleBold,
            Command::ToggleItalic,
            Command::ToggleUnderline,
            Command::ToggleStrikethrough,
            Command::SetAlign(Align::Center),
            Command::SetLineSpacing(2.0),
            Command::Undo,
            Command::Redo,
        ]
    }

    /// The invariant the whole of `task-1666` rests on: **a layout that changed means the text
    /// revision moved.**
    ///
    /// Laying out and colouring are keyed on `text_revision`, so a command that changes what the
    /// text looks like without bumping it would leave the old picture on the screen — a fault that
    /// looks like a drawing bug and lives in the model. This applies every command in turn and fails
    /// if the layout came out different while the revision stood still, so a command added later
    /// that forgets is caught the day it is written.
    #[test]
    fn a_layout_that_changed_means_the_text_revision_moved() {
        for command in every_command() {
            let mut document = a_document_with_something_of_everything();
            let before_layout = lay_out(&document, 200.0);
            let before_revision = document.text_revision();
            document.apply(command.clone());
            let after_layout = lay_out(&document, 200.0);
            if after_layout != before_layout {
                assert_ne!(
                    document.text_revision(),
                    before_revision,
                    "{command:?} changed the layout without bumping the text revision"
                );
            }
        }
    }

    /// The other half of it: a command that only moved the caret must not bump the text revision, or
    /// dragging a selection lays the document out again on every frame, which is what made a large
    /// file crawl.
    #[test]
    fn moving_the_caret_is_not_a_change_to_the_text() {
        let movements = [
            Command::MoveLeft { extend: false },
            Command::MoveRight { extend: true },
            Command::MoveWordRight { extend: false },
            Command::MoveLineEnd { extend: true },
            Command::MoveDocumentStart { extend: false },
            Command::MoveDocumentEnd { extend: true },
            Command::PlaceCaret { offset: 3, extend: false },
            Command::PlaceCaret { offset: 11, extend: true },
            Command::SelectAll,
        ];
        for command in movements {
            let mut document = a_document_with_something_of_everything();
            let before = document.text_revision();
            let moved = document.apply(command.clone());
            assert!(moved, "{command:?} is expected to move the caret in this document");
            assert_eq!(
                document.text_revision(),
                before,
                "{command:?} moved the caret and nothing else, so nothing needs laying out again"
            );
            assert_ne!(document.revision(), before, "the window still has to be painted again");
        }
    }

    /// Marking a passage is a colour painted behind the text. It moves no letter, so nothing needs
    /// laying out again — but the window does have to be painted.
    #[test]
    fn marking_a_passage_is_not_a_change_to_the_text() {
        let mut document = a_document_with_something_of_everything();
        let before = document.text_revision();
        assert!(document.highlight(0..5, Rgba::new(0x40, 0xC0, 0xFF, 0x60)));
        assert_eq!(document.text_revision(), before);
        assert_ne!(document.revision(), before);
        assert!(document.clear_highlight(0..5));
        assert_eq!(document.text_revision(), before);
    }

    #[test]
    fn a_new_document_is_empty_and_unmodified() {
        let document = Document::new();
        assert_eq!(document.text().to_string(), "");
        assert!(!document.is_modified());
        assert!(!document.can_undo());
        assert_eq!(document.selection(), Selection::caret(0));
    }

    #[test]
    fn typing_puts_the_caret_after_what_was_typed() {
        let document = typed("hello");
        assert_eq!(document.text().to_string(), "hello");
        assert_eq!(document.selection(), Selection::caret(5));
        assert!(document.is_modified());
    }

    #[test]
    fn typing_over_a_selection_replaces_it() {
        let mut document = typed("hello world");
        document.apply(Command::PlaceCaret { offset: 0, extend: false });
        document.apply(Command::PlaceCaret { offset: 5, extend: true });
        document.apply(Command::Insert("goodbye".to_owned()));
        assert_eq!(document.text().to_string(), "goodbye world");
        assert_eq!(document.selection(), Selection::caret(7));
    }

    #[test]
    fn backspace_removes_one_cluster_not_one_byte() {
        let mut document = typed("cafe\u{0301}");
        assert_eq!(document.text().len_bytes(), 6);
        document.apply(Command::DeleteBackward);
        assert_eq!(document.text().to_string(), "caf", "the e and its accent went together");
    }

    #[test]
    fn backspace_at_the_start_of_a_line_joins_it_to_the_line_before() {
        let mut document = typed("one\ntwo");
        document.apply(Command::PlaceCaret { offset: 4, extend: false });
        document.apply(Command::DeleteBackward);
        assert_eq!(document.text().to_string(), "onetwo");
        assert_eq!(document.text().len_lines(), 1);
    }

    #[test]
    fn delete_forward_at_the_end_of_a_line_pulls_the_next_line_up() {
        let mut document = typed("one\ntwo");
        document.apply(Command::PlaceCaret { offset: 3, extend: false });
        document.apply(Command::DeleteForward);
        assert_eq!(document.text().to_string(), "onetwo");
    }

    #[test]
    fn arrow_keys_move_the_caret_and_shift_extends_the_selection() {
        let mut document = typed("hello");
        document.apply(Command::MoveLeft { extend: false });
        assert_eq!(document.selection(), Selection::caret(4));
        document.apply(Command::MoveLeft { extend: true });
        assert_eq!(document.selection(), Selection::new(4, 3));
        assert_eq!(document.selected_text(), "l");
        document.apply(Command::MoveRight { extend: true });
        assert!(document.selection().is_empty());
    }

    #[test]
    fn an_arrow_key_with_a_selection_collapses_to_the_near_edge() {
        let mut document = typed("hello world");
        document.apply(Command::PlaceCaret { offset: 2, extend: false });
        document.apply(Command::PlaceCaret { offset: 8, extend: true });
        document.apply(Command::MoveLeft { extend: false });
        assert_eq!(
            document.selection(),
            Selection::caret(2),
            "collapses to the start, does not move past it"
        );

        document.apply(Command::PlaceCaret { offset: 2, extend: false });
        document.apply(Command::PlaceCaret { offset: 8, extend: true });
        document.apply(Command::MoveRight { extend: false });
        assert_eq!(document.selection(), Selection::caret(8));
    }

    #[test]
    fn left_and_right_cross_line_breaks() {
        let mut document = typed("ab\ncd");
        document.apply(Command::PlaceCaret { offset: 2, extend: false });
        document.apply(Command::MoveRight { extend: false });
        assert_eq!(document.selection().head, 3, "moved over the line break");
        document.apply(Command::MoveLeft { extend: false });
        assert_eq!(document.selection().head, 2);
    }

    #[test]
    fn line_start_and_line_end_stay_on_the_line() {
        let mut document = typed("one\ntwo three");
        document.apply(Command::PlaceCaret { offset: 6, extend: false });
        document.apply(Command::MoveLineStart { extend: false });
        assert_eq!(document.selection().head, 4);
        document.apply(Command::MoveLineEnd { extend: false });
        assert_eq!(document.selection().head, 13);
    }

    #[test]
    fn word_movement_jumps_over_words() {
        let mut document = typed("the quick brown");
        document.apply(Command::MoveDocumentStart { extend: false });
        document.apply(Command::MoveWordRight { extend: false });
        assert_eq!(document.selection().head, 3);
        document.apply(Command::MoveWordRight { extend: false });
        assert_eq!(document.selection().head, 9);
        document.apply(Command::MoveWordLeft { extend: false });
        assert_eq!(document.selection().head, 4);
    }

    #[test]
    fn select_all_selects_the_whole_document() {
        let mut document = typed("one\ntwo");
        document.apply(Command::SelectAll);
        assert_eq!(document.selected_text(), "one\ntwo");
    }

    #[test]
    fn moving_down_and_back_up_returns_to_the_same_column() {
        //   long line
        //   ab            <- short
        //   another long line
        let mut document = typed("0123456789\nab\n0123456789");
        document.apply(Command::PlaceCaret { offset: 8, extend: false });
        let placed = lay_out(&document, 1000.0);
        assert_eq!(placed.caret_at(8).x, 80.0);

        document.move_vertically(&placed, 1, false);
        assert_eq!(document.selection().head, 13, "the short line ends before column 8");

        document.move_vertically(&placed, 1, false);
        assert_eq!(
            document.selection().head,
            22,
            "the remembered column brings the caret back to column 8 on the long line"
        );
    }

    #[test]
    fn moving_up_from_the_first_line_goes_to_the_start_of_the_document() {
        let mut document = typed("one\ntwo");
        document.apply(Command::PlaceCaret { offset: 2, extend: false });
        let placed = lay_out(&document, 1000.0);
        document.move_vertically(&placed, -1, false);
        assert_eq!(document.selection().head, 0);
    }

    #[test]
    fn moving_down_from_the_last_line_goes_to_the_end_of_the_document() {
        let mut document = typed("one\ntwo");
        document.apply(Command::PlaceCaret { offset: 5, extend: false });
        let placed = lay_out(&document, 1000.0);
        document.move_vertically(&placed, 1, false);
        assert_eq!(document.selection().head, 7);
    }

    #[test]
    fn moving_down_through_wrapped_lines_stays_in_one_paragraph() {
        let mut document = typed("aa bb cc dd ee ff gg hh");
        document.apply(Command::MoveDocumentStart { extend: false });
        let placed = lay_out(&document, 60.0);
        assert!(placed.lines.len() > 2, "the paragraph must wrap for this test to mean anything");
        document.move_vertically(&placed, 1, false);
        assert!(
            document.selection().head > 0,
            "moved onto the second visual line of one paragraph"
        );
        assert_eq!(document.text().byte_to_line(document.selection().head), 0, "still paragraph 0");
    }

    #[test]
    fn bold_applies_to_the_selection_only() {
        let mut document = typed("plain bold plain");
        document.apply(Command::PlaceCaret { offset: 6, extend: false });
        document.apply(Command::PlaceCaret { offset: 10, extend: true });
        document.apply(Command::ToggleBold);
        assert!(document.chars().style_at(7).bold);
        assert!(!document.chars().style_at(2).bold);
        assert!(!document.chars().style_at(12).bold);
    }

    #[test]
    fn the_bold_button_turns_bold_off_when_the_whole_selection_is_bold() {
        let mut document = typed("word");
        document.apply(Command::SelectAll);
        document.apply(Command::ToggleBold);
        assert!(document.active_style().bold);
        document.apply(Command::ToggleBold);
        assert!(!document.active_style().bold, "pressing it again turns it off");
        assert!(!document.chars().style_at(2).bold);
    }

    #[test]
    fn the_bold_button_turns_bold_on_for_a_partly_bold_selection() {
        let mut document = typed("aaaabbbb");
        document.apply(Command::PlaceCaret { offset: 0, extend: false });
        document.apply(Command::PlaceCaret { offset: 4, extend: true });
        document.apply(Command::ToggleBold);
        document.apply(Command::SelectAll);
        assert!(!document.active_style().bold, "a half bold selection reports as not bold");
        document.apply(Command::ToggleBold);
        assert!(document.chars().style_at(1).bold);
        assert!(document.chars().style_at(6).bold, "the whole selection is now bold");
    }

    #[test]
    fn formatting_chosen_with_nothing_selected_applies_to_the_next_text_typed() {
        let mut document = typed("plain ");
        document.apply(Command::ToggleBold);
        assert!(document.text().to_string() == "plain ", "pressing bold types nothing");
        document.apply(Command::Insert("bold".to_owned()));
        assert!(document.chars().style_at(8).bold);
        assert!(!document.chars().style_at(2).bold);
        document.apply(Command::Insert(" after".to_owned()));
        assert!(document.chars().style_at(13).bold, "the run carries on until it is turned off");
    }

    #[test]
    fn two_formatting_choices_before_typing_both_apply() {
        let mut document = Document::new();
        document.apply(Command::ToggleBold);
        document.apply(Command::ApplyStyle(StyleChange::color(Color::RED)));
        document.apply(Command::Insert("x".to_owned()));
        let style = document.chars().style_at(0);
        assert!(style.bold);
        assert_eq!(style.color, Color::RED);
    }

    #[test]
    fn setting_the_colour_of_a_selection_keeps_its_bold() {
        let mut document = typed("word");
        document.apply(Command::SelectAll);
        document.apply(Command::ToggleBold);
        document.apply(Command::ApplyStyle(StyleChange::color(Color::BLUE)));
        let style = document.chars().style_at(2);
        assert!(style.bold, "the colour change must not clear bold");
        assert_eq!(style.color, Color::BLUE);
    }

    #[test]
    fn text_typed_inside_a_formatted_run_inherits_its_formatting() {
        let mut document = typed("bold");
        document.apply(Command::SelectAll);
        document.apply(Command::ToggleBold);
        document.apply(Command::PlaceCaret { offset: 2, extend: false });
        document.apply(Command::Insert("XX".to_owned()));
        assert_eq!(document.text().to_string(), "boXXld");
        assert!(document.chars().style_at(3).bold);
    }

    #[test]
    fn alignment_applies_to_every_paragraph_the_selection_touches() {
        let mut document = typed("one\ntwo\nthree");
        document.apply(Command::PlaceCaret { offset: 5, extend: false });
        document.apply(Command::SetAlign(Align::Center));
        assert_eq!(document.paragraphs().get(1).align, Align::Center);
        assert_eq!(document.paragraphs().get(0).align, Align::Left, "only the caret's paragraph");

        document.apply(Command::SelectAll);
        document.apply(Command::SetAlign(Align::Right));
        for paragraph in 0..3 {
            assert_eq!(document.paragraphs().get(paragraph).align, Align::Right);
        }
    }

    #[test]
    fn line_spacing_is_kept_within_sensible_bounds() {
        let mut document = typed("text");
        document.apply(Command::SetLineSpacing(2.0));
        assert_eq!(document.active_paragraph_style().line_spacing, 2.0);
        document.apply(Command::SetLineSpacing(99.0));
        assert_eq!(document.active_paragraph_style().line_spacing, 4.0, "clamped");
        document.apply(Command::SetLineSpacing(0.0));
        assert_eq!(document.active_paragraph_style().line_spacing, 0.5, "clamped");
    }

    #[test]
    fn a_new_paragraph_inherits_the_formatting_of_the_one_it_came_from() {
        let mut document = typed("centred");
        document.apply(Command::SetAlign(Align::Center));
        document.apply(Command::MoveDocumentEnd { extend: false });
        document.apply(Command::Insert("\n".to_owned()));
        document.apply(Command::Insert("second".to_owned()));
        assert_eq!(document.text().len_lines(), 2);
        assert_eq!(document.paragraphs().len(), 2);
        assert_eq!(document.paragraphs().get(1).align, Align::Center);
    }

    #[test]
    fn deleting_a_line_break_joins_the_paragraphs_and_keeps_the_first_ones_formatting() {
        let mut document = typed("one\ntwo");
        document.apply(Command::PlaceCaret { offset: 0, extend: false });
        document.apply(Command::SetAlign(Align::Right));
        document.apply(Command::PlaceCaret { offset: 5, extend: false });
        document.apply(Command::SetAlign(Align::Center));
        document.apply(Command::PlaceCaret { offset: 4, extend: false });
        document.apply(Command::DeleteBackward);
        assert_eq!(document.text().to_string(), "onetwo");
        assert_eq!(document.paragraphs().len(), 1);
        assert_eq!(document.paragraphs().get(0).align, Align::Right);
    }

    #[test]
    fn undo_removes_a_whole_run_of_typing_not_one_letter() {
        let mut document = Document::new();
        for letter in "hello".chars() {
            document.apply(Command::Insert(letter.to_string()));
        }
        assert_eq!(document.text().to_string(), "hello");
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "", "one undo removes the whole run");
    }

    #[test]
    fn a_caret_move_breaks_a_run_of_typing_into_two_undo_steps() {
        let mut document = Document::new();
        for letter in "one".chars() {
            document.apply(Command::Insert(letter.to_string()));
        }
        document.apply(Command::MoveDocumentStart { extend: false });
        for letter in "two".chars() {
            document.apply(Command::Insert(letter.to_string()));
        }
        assert_eq!(document.text().to_string(), "twoone");
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "one");
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "");
    }

    #[test]
    fn undo_restores_formatting_as_well_as_text() {
        let mut document = typed("word");
        document.apply(Command::SelectAll);
        document.apply(Command::ToggleBold);
        assert!(document.chars().style_at(2).bold);
        document.apply(Command::Undo);
        assert!(!document.chars().style_at(2).bold, "undo puts the formatting back too");
    }

    #[test]
    fn undo_restores_alignment() {
        let mut document = typed("one\ntwo");
        document.apply(Command::SelectAll);
        document.apply(Command::SetAlign(Align::Center));
        document.apply(Command::Undo);
        assert_eq!(document.paragraphs().get(0).align, Align::Left);
    }

    #[test]
    fn redo_puts_back_what_undo_removed() {
        let mut document = typed("hello");
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "");
        document.apply(Command::Redo);
        assert_eq!(document.text().to_string(), "hello");
    }

    /// Returning to the state loaded from disk must also return to its clean history identity.
    #[test]
    fn undoing_back_to_the_loaded_state_clears_the_modified_mark() {
        let mut document = Document::from_text("on disk");
        document.apply(Command::MoveDocumentEnd { extend: false });
        document.apply(Command::Insert(" changed".to_owned()));
        assert!(document.is_modified());

        document.apply(Command::Undo);

        assert_eq!(document.text().to_string(), "on disk");
        assert!(!document.is_modified(), "the current history revision is the loaded revision");
    }

    #[test]
    fn a_new_edit_clears_the_redo_history() {
        let mut document = typed("hello");
        document.apply(Command::Undo);
        document.apply(Command::Insert("different".to_owned()));
        assert!(!document.can_redo());
        document.apply(Command::Redo);
        assert_eq!(document.text().to_string(), "different", "redo did nothing");
    }

    #[test]
    fn undo_on_a_fresh_document_does_nothing() {
        let mut document = Document::new();
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "");
    }

    #[test]
    fn the_undo_history_is_capped() {
        let mut document = Document::new();
        for i in 0..(UNDO_LIMIT + 50) {
            // A caret move between each insert makes every one its own undo step.
            document.apply(Command::Insert(format!("{i} ")));
            document.apply(Command::MoveDocumentStart { extend: false });
        }
        assert!(document.undo.len() <= UNDO_LIMIT, "history grew to {}", document.undo.len());
    }

    #[test]
    fn the_revision_changes_on_every_edit_so_the_view_knows_to_lay_out_again() {
        let mut document = Document::new();
        let start = document.revision();
        assert!(document.apply(Command::Insert("a".to_owned())));
        assert!(document.revision() > start);
        let after = document.revision();
        assert!(!document.apply(Command::MoveRight { extend: false }), "already at the end");
        assert_eq!(document.revision(), after, "a move that changes nothing does not bump it");
    }

    #[test]
    fn opening_a_file_written_on_windows_normalises_its_line_breaks() {
        let directory = std::env::temp_dir().join("unluminous-core-test-crlf");
        std::fs::create_dir_all(&directory).expect("make the test directory");
        let path = directory.join("windows.txt");
        std::fs::write(&path, "one\r\ntwo\r\n").expect("write the test file");
        let document = Document::open(&path).expect("open it");
        assert_eq!(document.text().to_string(), "one\ntwo\n");
        assert_eq!(document.text().len_lines(), 3);
        std::fs::remove_file(&path).ok();
    }

    /// `task-1804` §7.1. The measurement in the ticket, made a test: a file with Windows line
    /// breaks, one character typed, saved, compared **byte for byte** with what it was.
    ///
    /// It is a round trip rather than an assertion about `save_as`, because the fault was never in
    /// one function -- the reading was right and the writing was right, and what was missing was the
    /// fact that joins them.
    #[test]
    fn a_round_trip_leaves_every_line_ending_exactly_as_it_was() {
        for (name, before, after) in [
            (
                "crlf.txt",
                "line one\r\nline two\r\nline three\r\n",
                "Xline one\r\nline two\r\nline three\r\n",
            ),
            ("lf.txt", "line one\nline two\nline three\n", "Xline one\nline two\nline three\n"),
            // No trailing newline. This one was already correct and is pinned so it stays that way.
            ("bare.txt", "line one\r\nline two", "Xline one\r\nline two"),
            ("classic.txt", "line one\rline two\r", "Xline one\rline two\r"),
        ] {
            let path = round_trip_folder().join(name);
            std::fs::write(&path, before).expect("write the test file");
            let mut document = Document::open(&path).expect("open it");
            document.apply(Command::MoveDocumentStart { extend: false });
            document.apply(Command::Insert("X".to_owned()));
            document.save().expect("save it");
            let written = std::fs::read(&path).expect("read it back");
            assert_eq!(
                String::from_utf8_lossy(&written),
                after,
                "{name} did not come back the way it went in"
            );
        }
    }

    /// A file with two kinds of line ending in it comes back with one, and it is the commoner one.
    ///
    /// **`task-1984` C22.** The round trip above pins four files that each use one ending throughout,
    /// which is the case where the answer is that nothing changes. A **mixed** file is the case where
    /// something has to, and `LineEnding::dominant_in`'s own comment says why: there is no reading
    /// that leaves every line alone, so counting is the one that changes the fewest of them. That is a
    /// decision a reader might disagree with, so it is written down as bytes rather than left to be
    /// discovered by somebody whose file came back different.
    #[test]
    fn a_file_with_two_kinds_of_line_ending_comes_back_with_the_commoner_one() {
        // Three Windows endings and two Unix ones, so Windows wins by counting.
        let path = round_trip_folder().join("mixed.txt");
        let before = "one\r\ntwo\r\nthree\nfour\r\nfive\n";
        std::fs::write(&path, before).expect("write the test file");
        let mut document = Document::open(&path).expect("open it");
        assert_eq!(document.line_ending(), crate::encoding::LineEnding::Crlf, "counted");
        document.apply(Command::MoveDocumentStart { extend: false });
        document.apply(Command::Insert("X".to_owned()));
        document.save().expect("save it");
        let written = std::fs::read(&path).expect("read it back");
        assert_eq!(
            String::from_utf8_lossy(&written),
            "Xone\r\ntwo\r\nthree\r\nfour\r\nfive\r\n",
            "the two Unix endings became Windows ones and the three Windows ones did not move"
        );
    }

    /// A lone carriage return is a line break, which it was not before `task-1804` §7.5.
    #[test]
    fn a_classic_macintosh_file_opens_as_its_lines_rather_than_as_one() {
        let path = round_trip_folder().join("mac-lines.txt");
        std::fs::write(&path, "one\rtwo\r").expect("write the test file");
        let document = Document::open(&path).expect("open it");
        assert_eq!(document.text().len_lines(), 3, "two breaks make three lines");
        assert_eq!(document.line_ending(), crate::encoding::LineEnding::Cr);
    }

    /// `task-1804` §7.6. The file that used to be refused, and that `tab open` then reported
    /// success for. It opens, it says what it is, and it will not be written back.
    #[test]
    fn a_file_that_is_not_utf8_opens_read_only_rather_than_being_refused() {
        let path = round_trip_folder().join("latin1.txt");
        std::fs::write(&path, b"caf\xE9\n").expect("write the test file");
        let mut document = Document::open(&path).expect("it opens");
        assert_eq!(document.encoding(), crate::encoding::Encoding::Latin1);
        assert_eq!(document.text().to_string(), "caf\u{e9}\n");
        assert!(!document.writable());
        let refusal = document.save().expect_err("it must not be written back");
        assert!(refusal.to_string().contains("Latin-1"), "{refusal}");
        assert_eq!(
            std::fs::read(&path).expect("still there"),
            b"caf\xE9\n",
            "the bytes are untouched"
        );
    }

    /// A byte order mark is part of the file and comes back with it.
    #[test]
    fn a_utf8_file_with_a_byte_order_mark_keeps_it() {
        let path = round_trip_folder().join("bom.txt");
        std::fs::write(&path, b"\xEF\xBB\xBFhello\n").expect("write the test file");
        let mut document = Document::open(&path).expect("open it");
        assert_eq!(document.encoding(), crate::encoding::Encoding::Utf8Bom);
        assert_eq!(document.text().to_string(), "hello\n");
        document.save().expect("it is writable");
        assert_eq!(std::fs::read(&path).expect("read it back"), b"\xEF\xBB\xBFhello\n");
    }

    /// Choosing an ending is not an edit and is still something to save.
    #[test]
    fn choosing_a_line_ending_writes_the_file_that_way_and_marks_it_unsaved() {
        let path = round_trip_folder().join("chosen.txt");
        std::fs::write(&path, "one\ntwo\n").expect("write the test file");
        let mut document = Document::open(&path).expect("open it");
        assert!(!document.is_modified());
        document.set_line_ending(crate::encoding::LineEnding::Crlf);
        assert!(document.is_modified(), "there is something to save now");
        assert_eq!(document.text().to_string(), "one\ntwo\n", "the text itself did not change");
        document.save().expect("save it");
        assert_eq!(std::fs::read(&path).expect("read it back"), b"one\r\ntwo\r\n");
    }

    /// One folder for the round-trip tests, made once, so two of them cannot race on it.
    fn round_trip_folder() -> std::path::PathBuf {
        static FOLDER: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        FOLDER
            .get_or_init(|| {
                let folder = std::env::temp_dir().join("unluminous-core-test-round-trip");
                std::fs::create_dir_all(&folder).expect("make the test directory");
                folder
            })
            .clone()
    }

    #[test]
    fn saving_writes_the_text_and_clears_the_modified_mark() {
        let directory = std::env::temp_dir().join("unluminous-core-test-save");
        std::fs::create_dir_all(&directory).expect("make the test directory");
        let path = directory.join("out.md");
        let mut document = typed("# heading\n\nbody");
        assert!(document.is_modified());
        document.save_as(&path).expect("save it");
        assert!(!document.is_modified());
        // A document nobody opened from disk is written with the platform's own line ending, which
        // is what a file this machine creates ought to look like. `task-1804`.
        let expected =
            crate::encoding::LineEnding::platform_default().apply("# heading\n\nbody").into_owned();
        assert_eq!(std::fs::read_to_string(&path).expect("read it back"), expected);

        document.apply(Command::Insert(" after save".to_owned()));
        document.apply(Command::Undo);
        assert!(!document.is_modified(), "saving closes the typing group at an exact undo point");
        document.apply(Command::Redo);
        assert!(document.is_modified(), "redo leaves the saved point");
        document.apply(Command::Undo);
        document.apply(Command::Undo);
        assert!(document.is_modified(), "a state before the save is not clean");
        document.apply(Command::Insert("a new branch".to_owned()));
        assert!(document.is_modified(), "a branch cannot reuse the discarded clean identity");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn every_edit_keeps_the_formatting_and_paragraph_counts_in_step_with_the_text() {
        // The three structures have to agree: the spans must cover exactly the document's bytes and
        // there must be one paragraph entry per line. Anything that breaks that corrupts the document
        // later, so it is checked after every command here.
        let mut document = Document::new();
        let commands = [
            Command::Insert("the quick brown fox\njumps over\n".to_owned()),
            Command::SelectAll,
            Command::ToggleBold,
            Command::PlaceCaret { offset: 4, extend: false },
            Command::PlaceCaret { offset: 9, extend: true },
            Command::ApplyStyle(StyleChange::size(28.0)),
            Command::Insert("SLOW".to_owned()),
            Command::SetAlign(Align::Center),
            Command::Insert("\n\nmore".to_owned()),
            Command::DeleteBackward,
            Command::DeleteWordBackward,
            Command::SelectAll,
            Command::DeleteForward,
            Command::Insert("start again".to_owned()),
            Command::Undo,
            Command::Undo,
            Command::Redo,
        ];
        for (index, command) in commands.into_iter().enumerate() {
            document.apply(command.clone());
            assert_eq!(
                document.chars().total_len(),
                document.text().len_bytes(),
                "the formatting spans no longer cover the text after command {index}: {command:?}"
            );
            assert_eq!(
                document.paragraphs().len(),
                document.text().len_lines(),
                "the paragraph count no longer matches the line count after command {index}: {command:?}"
            );
            assert!(
                document.selection().head <= document.text().len_bytes(),
                "the caret is past the end of the text after command {index}: {command:?}"
            );
            // Laying out must not panic on any of these states.
            let _ = lay_out(&document, 120.0);
        }
    }

    // --------------------------------------------------------------------- the marked passages

    const MARK: Rgba = Rgba::new(0xE8, 0xC0, 0x4A, 0x59);

    #[test]
    fn marking_a_passage_is_not_an_edit() {
        let mut document = Document::from_text("one two three");
        let revision = document.revision();
        assert!(document.highlight(4..7, MARK));
        assert!(
            !document.is_modified(),
            "a mark is not written to the file, so nothing is unsaved"
        );
        assert!(!document.can_undo(), "and nothing goes onto the undo history");
        assert!(document.revision() > revision, "but it does have to be drawn again");
    }

    #[test]
    fn marking_the_same_passage_twice_says_nothing_changed_the_second_time() {
        let mut document = Document::from_text("one two three");
        assert!(document.highlight(4..7, MARK));
        assert!(!document.highlight(4..7, MARK));
    }

    #[test]
    fn a_mark_moves_by_exactly_what_was_typed_above_it() {
        let mut document = Document::from_text("one two three");
        document.highlight(8..13, MARK);
        document.apply(Command::PlaceCaret { offset: 0, extend: false });
        document.apply(Command::Insert("zero ".to_owned()));
        assert_eq!(document.highlights().at(13).map(|mark| mark.range.clone()), Some(13..18));
    }

    #[test]
    fn deleting_the_text_under_a_mark_takes_the_mark_away() {
        let mut document = Document::from_text("one two three");
        document.highlight(4..7, MARK);
        document.apply(Command::PlaceCaret { offset: 3, extend: false });
        document.apply(Command::PlaceCaret { offset: 8, extend: true });
        document.apply(Command::DeleteBackward);
        assert!(document.highlights().is_empty());
    }

    #[test]
    fn undo_puts_a_mark_back_where_the_text_it_was_on_went() {
        let mut document = Document::from_text("one two three");
        document.highlight(4..7, MARK);
        document.apply(Command::PlaceCaret { offset: 3, extend: false });
        document.apply(Command::PlaceCaret { offset: 8, extend: true });
        document.apply(Command::DeleteBackward);
        assert!(document.highlights().is_empty());
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "one two three");
        assert_eq!(document.highlights().at(5).map(|mark| mark.range.clone()), Some(4..7));
    }

    #[test]
    fn a_set_that_is_restored_is_brought_inside_a_file_that_has_since_shrunk() {
        let mut document = Document::from_text("short");
        let mut marks = Highlights::new();
        marks.add(0..3, MARK);
        marks.add(100..200, MARK);
        document.set_highlights(marks);
        assert_eq!(document.highlights().len(), 1, "the mark past the end of the file is dropped");
        assert!(document.highlights().check());
    }

    #[test]
    fn accepting_a_completion_shifts_the_marks_after_it_and_leaves_the_rest_alone() {
        // `task-1677` scenario 34. Accepting a suggestion is one `ReplaceMany` and nothing else, so
        // the marked passages move by exactly what it inserted — the same two lines that already
        // shift them for every other edit, rather than a rule of completion's own.
        let mut document = Document::from_text("let dra = one two three");
        let before = document.text().to_string().find("two").expect("two");
        document.highlight(before..before + 3, MARK);
        let stem = document.text().to_string().find("dra").expect("dra");
        assert!(
            document.apply(Command::ReplaceMany(vec![(stem..stem + 3, "draw_frame".to_owned()),]))
        );
        assert_eq!(document.text().to_string(), "let draw_frame = one two three");
        let grown = "draw_frame".len() - "dra".len();
        let mark = document.highlights().at(before + grown).expect("the mark");
        assert_eq!(
            document.text().byte_slice(mark.range.clone()),
            "two",
            "the mark is still on its own word: {:?}",
            mark.range
        );
        assert_eq!(document.highlights().len(), 1);
        // And the caret lands after the inserted name, which is what makes typing carry on.
        assert_eq!(document.selection().head, stem + "draw_frame".len());
        // One undo step, by construction: undo restores a state.
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "let dra = one two three");
        assert_eq!(
            document.highlights().at(before).map(|mark| mark.range.clone()),
            Some(before..before + 3),
            "and the mark came back with it"
        );
    }

    #[test]
    fn clearing_takes_the_one_under_the_caret_and_leaves_the_others() {
        let mut document = Document::from_text("one two three");
        document.highlight(0..3, MARK);
        document.highlight(8..13, MARK);
        assert!(document.clear_highlight_at(9));
        assert_eq!(document.highlights().len(), 1);
        assert!(document.clear_highlights());
        assert!(document.highlights().is_empty());
        assert!(!document.clear_highlights(), "there is nothing left to clear");
    }
    /// Collapsing a block is not an edit: nothing goes on the undo history and the file is not
    /// marked as having unsaved changes. The same rule the marked passages follow.
    #[test]
    fn collapsing_a_block_is_not_an_edit() {
        let mut document = Document::from_text("one\ntwo\nthree\n");
        let undo = document.can_undo();
        let text = document.text_revision();
        let mut folds = crate::folding::Folds::new();
        folds.add(4);
        assert!(document.set_folds(folds));
        assert!(!document.is_modified(), "a fold is not an unsaved change");
        assert_eq!(document.can_undo(), undo, "and it is not on the undo history");
        assert_eq!(document.text_revision(), text, "the text did not change");
        assert!(document.fold_revision() > 1, "but the layout has to be worked out again");
    }

    /// The collapsed blocks move with the text, in the same splice that already moves the marked
    /// passages.
    #[test]
    fn a_collapsed_block_moves_with_the_text() {
        let mut document = Document::from_text("one\ntwo\nthree\n");
        let mut folds = crate::folding::Folds::new();
        folds.add(4);
        document.set_folds(folds);
        document.apply(Command::PlaceCaret { offset: 0, extend: false });
        document.apply(Command::Insert("zero\n".to_owned()));
        assert_eq!(document.folds().offsets(), &[9], "the head line moved down the file");
        document.apply(Command::PlaceCaret { offset: 0, extend: false });
        document.apply(Command::Insert("!".to_owned()));
        assert_eq!(document.folds().offsets(), &[10]);
    }

    /// Undo restores a state, and which blocks were collapsed is part of the state.
    #[test]
    fn undo_puts_the_collapsed_blocks_back_as_they_were() {
        let mut document = Document::from_text("one\ntwo\nthree\n");
        let mut folds = crate::folding::Folds::new();
        folds.add(4);
        document.set_folds(folds);
        document.apply(Command::PlaceCaret { offset: 0, extend: false });
        document.apply(Command::Insert("zero\n".to_owned()));
        assert_eq!(document.folds().offsets(), &[9], "the head line moved down the file");
        document.apply(Command::Undo);
        assert_eq!(document.folds().offsets(), &[4], "back where it was before the edit");
    }
}

#[cfg(test)]
mod indent_tests {
    use super::*;

    /// Select `from` to `to` in the document, the way the keys would leave it.
    fn selected(document: &mut Document, from: usize, to: usize) {
        document.apply(Command::PlaceCaret { offset: from, extend: false });
        document.apply(Command::PlaceCaret { offset: to, extend: true });
    }

    /// The whole of the ask: a block of lines, `Tab`, and the block is indented rather than gone.
    #[test]
    fn tab_over_a_selection_indents_each_line_it_touches() {
        let mut document = Document::from_text("one\ntwo\nthree\nfour");
        selected(&mut document, 0, 18);
        assert!(document.apply(Command::Indent { unit: IndentUnit::Tab }));
        assert_eq!(
            document.text().to_string(),
            "\tone\n\ttwo\n\tthree\n\tfour",
            "a tab at the start of each of the four lines, and nothing else"
        );
    }

    /// A selection that ends exactly on a line break holds no byte of the line below it, so the
    /// line below is not indented.
    #[test]
    fn a_selection_ending_on_a_line_break_indents_the_line_above_not_the_one_below() {
        let mut document = Document::from_text("one\ntwo\nthree");
        selected(&mut document, 0, 4); // "one\n"
        assert!(document.apply(Command::Indent { unit: IndentUnit::Tab }));
        assert_eq!(document.text().to_string(), "\tone\ntwo\nthree", "only the first line moved");
    }

    /// The command answers for a bare caret too, because the command line asks it about a document
    /// that may have no selection: the caret's line is indented and the caret moves past the
    /// character put in front of it.
    #[test]
    fn a_bare_caret_indents_its_own_line_and_moves_past_the_character() {
        let mut document = Document::from_text("one\ntwo\nthree");
        document.apply(Command::PlaceCaret { offset: 5, extend: false }); // in "two"
        assert!(document.apply(Command::Indent { unit: IndentUnit::Tab }));
        assert_eq!(document.text().to_string(), "one\n\ttwo\nthree");
        let caret = document.selection().head;
        assert_eq!(
            &document.text().to_string()[caret - 1..caret + 2],
            "two",
            "still between the same letters"
        );
    }

    /// Each end of the selection moves past the indents put at or before it, no further: the
    /// highlight stays over the words it covered, and a second press indents the same lines again.
    #[test]
    fn the_selection_follows_the_text_it_covered() {
        let mut document = Document::from_text("one\ntwo\nthree\nfour");
        selected(&mut document, 0, 18);
        document.apply(Command::Indent { unit: IndentUnit::Tab });
        assert_eq!(
            document.selection().range(),
            1..22,
            "the original bytes, shifted by the four indents"
        );
        document.apply(Command::Indent { unit: IndentUnit::Tab });
        assert_eq!(document.text().to_string(), "\t\tone\n\t\ttwo\n\t\tthree\n\t\tfour");
        assert_eq!(document.selection().range(), 2..26, "the same lines, indented again");
    }

    /// A single line selected in the middle of the file indents that line alone.
    #[test]
    fn a_single_line_selection_indents_that_line_alone() {
        let mut document = Document::from_text("one\ntwo\nthree");
        selected(&mut document, 5, 6); // the "w" of "two"
        assert!(document.apply(Command::Indent { unit: IndentUnit::Space }));
        assert_eq!(
            document.text().to_string(),
            "one\n two\nthree",
            "one space in front of the selected line and nowhere else"
        );
    }

    /// One key press is one snapshot, whatever the selection spans: one undo puts every line back,
    /// and the selection comes back with it.
    #[test]
    fn one_press_is_one_undo_step() {
        let mut document = Document::from_text("one\ntwo\nthree\nfour");
        selected(&mut document, 0, 18);
        document.apply(Command::Indent { unit: IndentUnit::Tab });
        document.apply(Command::Undo);
        assert_eq!(
            document.text().to_string(),
            "one\ntwo\nthree\nfour",
            "one step put all four lines back"
        );
        assert_eq!(document.selection().range(), 0..18, "the selection comes back with it");
    }

    /// The two units differ only in the character they put in front of the lines.
    #[test]
    fn the_units_differ_only_in_the_character() {
        let mut tabbed = Document::from_text("one\ntwo");
        selected(&mut tabbed, 0, 8);
        tabbed.apply(Command::Indent { unit: IndentUnit::Tab });
        let mut spaced = Document::from_text("one\ntwo");
        selected(&mut spaced, 0, 8);
        spaced.apply(Command::Indent { unit: IndentUnit::Space });
        assert_eq!(tabbed.text().to_string(), "\tone\n\ttwo");
        assert_eq!(spaced.text().to_string(), " one\n two");
    }

    /// The marked passages move with the text, in the same place the bytes move: a mark over a
    /// whole line lands over the line's new letters, and a mark that ends on a line break does not
    /// grow into the indent put in front of the next line.
    #[test]
    fn the_marks_move_with_the_indent() {
        let mut document = Document::from_text("one\ntwo\nthree");
        document.highlight(4..7, Rgba::new(0xFF, 0xC0, 0x40, 0x60)); // "two"
        selected(&mut document, 0, 13);
        document.apply(Command::Indent { unit: IndentUnit::Tab });
        let text = document.text().to_string();
        let mark = document.highlights().iter().next().expect("the mark").range.clone();
        assert_eq!(&text[mark.clone()], "two", "the mark is over the line's new letters");
    }
}

#[cfg(test)]
mod dedent_tests {
    use super::*;

    /// Select `from` to `to` in the document, the way the keys would leave it.
    fn selected(document: &mut Document, from: usize, to: usize) {
        document.apply(Command::PlaceCaret { offset: from, extend: false });
        document.apply(Command::PlaceCaret { offset: to, extend: true });
    }

    /// The whole of the ask: an indented block, `Shift+Tab`, and every line loses its tab.
    #[test]
    fn shift_tab_over_a_selection_removes_the_indent_from_every_line() {
        let mut document = Document::from_text("\tone\n\ttwo\n\tthree\n\tfour");
        selected(&mut document, 0, 22);
        assert!(document.apply(Command::Dedent { unit: IndentUnit::Tab }));
        assert_eq!(document.text().to_string(), "one\ntwo\nthree\nfour");
    }

    /// A line with nothing at its start to remove is left exactly as it was.
    #[test]
    fn a_line_with_no_indentation_is_left_alone() {
        let mut document = Document::from_text("\tone\ntwo\n\tthree");
        selected(&mut document, 0, 15);
        assert!(document.apply(Command::Dedent { unit: IndentUnit::Tab }));
        assert_eq!(
            document.text().to_string(),
            "one\ntwo\nthree",
            "the middle line had nothing to remove, so it is unchanged"
        );
    }

    /// A tab-dedent does not touch a line indented with a space, and a space-dedent does not touch
    /// one indented with a tab: there is no tab width to fall back on, so a mismatched indent is
    /// left rather than guessed at.
    #[test]
    fn a_line_indented_with_the_other_unit_is_left_alone() {
        let mut document = Document::from_text(" one\n two");
        selected(&mut document, 0, 9);
        assert!(
            !document.apply(Command::Dedent { unit: IndentUnit::Tab }),
            "neither line starts with a tab"
        );
        assert_eq!(document.text().to_string(), " one\n two");
    }

    /// A press that finds nothing to remove anywhere in the selection makes no change at all, and
    /// pushes no undo step for there to be nothing to undo.
    #[test]
    fn a_press_that_finds_nothing_to_remove_does_not_push_an_undo_step() {
        let mut document = Document::from_text("one\ntwo");
        selected(&mut document, 0, 7);
        let undo = document.can_undo();
        assert!(!document.apply(Command::Dedent { unit: IndentUnit::Tab }));
        assert_eq!(document.can_undo(), undo, "nothing was removed, so nothing was pushed");
    }

    /// The command answers for a bare caret too: the caret's line loses its indent and the caret
    /// moves back with the letters that shifted under it.
    #[test]
    fn a_bare_caret_dedents_its_own_line_and_moves_back_with_it() {
        let mut document = Document::from_text("one\n\ttwo\nthree");
        document.apply(Command::PlaceCaret { offset: 6, extend: false }); // between "t" and "wo" in "\ttwo"
        assert!(document.apply(Command::Dedent { unit: IndentUnit::Tab }));
        assert_eq!(document.text().to_string(), "one\ntwo\nthree");
        let caret = document.selection().head;
        assert_eq!(
            &document.text().to_string()[caret - 1..caret + 2],
            "two",
            "still between the same letters"
        );
    }

    /// Each end of the selection moves back past the removals at or before it: the highlight stays
    /// over the words it covered, and a second press removes another level from a doubly indented
    /// block.
    #[test]
    fn the_selection_follows_the_text_it_covered() {
        let mut document = Document::from_text("\t\tone\n\t\ttwo\n\t\tthree\n\t\tfour");
        selected(&mut document, 2, 26);
        document.apply(Command::Dedent { unit: IndentUnit::Tab });
        assert_eq!(
            document.text().to_string(),
            "\tone\n\ttwo\n\tthree\n\tfour",
            "one level removed from each line"
        );
        assert_eq!(
            document.selection().range(),
            1..22,
            "the same bytes, shifted back by the four removals"
        );
        document.apply(Command::Dedent { unit: IndentUnit::Tab });
        assert_eq!(
            document.text().to_string(),
            "one\ntwo\nthree\nfour",
            "the second level is gone too"
        );
        assert_eq!(document.selection().range(), 0..18);
    }

    /// One key press is one snapshot, whatever the selection spans: one undo puts every line's
    /// indent back, and the selection comes back with it.
    #[test]
    fn one_press_is_one_undo_step() {
        let mut document = Document::from_text("\tone\n\ttwo\n\tthree\n\tfour");
        selected(&mut document, 0, 22);
        document.apply(Command::Dedent { unit: IndentUnit::Tab });
        document.apply(Command::Undo);
        assert_eq!(
            document.text().to_string(),
            "\tone\n\ttwo\n\tthree\n\tfour",
            "one step put every line's tab back"
        );
        assert_eq!(document.selection().range(), 0..22, "the selection comes back with it");
    }

    /// The two units differ only in the character they look for and remove.
    #[test]
    fn the_units_differ_only_in_the_character() {
        let mut tabbed = Document::from_text("\tone\n\ttwo");
        selected(&mut tabbed, 0, 9);
        tabbed.apply(Command::Dedent { unit: IndentUnit::Tab });
        let mut spaced = Document::from_text(" one\n two");
        selected(&mut spaced, 0, 9);
        spaced.apply(Command::Dedent { unit: IndentUnit::Space });
        assert_eq!(tabbed.text().to_string(), "one\ntwo");
        assert_eq!(spaced.text().to_string(), "one\ntwo");
    }

    /// Dedent is indent's own inverse: indenting a block and dedenting it again leaves the text and
    /// the selection exactly as they started.
    #[test]
    fn dedent_undoes_what_indent_did() {
        let mut document = Document::from_text("one\ntwo\nthree");
        selected(&mut document, 0, 13);
        document.apply(Command::Indent { unit: IndentUnit::Space });
        document.apply(Command::Dedent { unit: IndentUnit::Space });
        assert_eq!(document.text().to_string(), "one\ntwo\nthree");
        assert_eq!(document.selection().range(), 0..13);
    }
}

/// The seven commands that work on whole lines. `task-1922` §5.5.
///
/// Two of these are worth more than the others and are written first: a toggle is its own inverse,
/// and a marked passage, a collapsed block and a breakpoint follow the text every one of them moves.
/// The second is the property `Document::splice` exists for, and the one a later change is most
/// likely to break.
#[cfg(test)]
mod line_command_tests {
    use super::*;

    fn selected(document: &mut Document, from: usize, to: usize) {
        document.apply(Command::PlaceCaret { offset: from, extend: false });
        document.apply(Command::PlaceCaret { offset: to, extend: true });
    }

    fn caret_at(document: &mut Document, offset: usize) {
        document.apply(Command::PlaceCaret { offset, extend: false });
    }

    fn all_of(document: &mut Document) {
        document.apply(Command::SelectAll);
    }

    fn comment(marker: &str) -> Command {
        Command::ToggleLineComment { marker: marker.to_owned() }
    }

    /// Comment a selection, comment it again, and the bytes are the bytes that were there.
    ///
    /// Three shapes, because each of them is a different way of getting it wrong: a selection that
    /// starts part way through its first line, one with a blank line in the middle of it, and one
    /// where a line was already commented before anybody pressed anything.
    #[test]
    fn a_line_comment_toggled_twice_gives_back_every_byte() {
        let cases = [
            ("    let a = 1;\n    let b = 2;\n", 8usize, 20usize),
            ("    let a = 1;\n\n    let b = 2;\n", 0, 29),
            ("let a = 1;\n// already\nlet b = 2;\n", 0, 32),
        ];
        for (source, from, to) in cases {
            let mut document = Document::from_text(source);
            selected(&mut document, from, to);
            document.apply(comment("// "));
            assert_ne!(document.text().to_string(), source, "{source:?} was not commented at all");
            document.apply(comment("// "));
            assert_eq!(document.text().to_string(), source, "{source:?} did not come back");
            assert_eq!(
                document.selection().range(),
                from..to,
                "{source:?} left the selection somewhere else"
            );
        }
    }

    /// A marked passage, a collapsed block and a breakpoint follow the text each command moves.
    ///
    /// One document a command, because what "where the text moved" means differs: a line that was
    /// copied has not moved at all, and a line that was sorted has moved somewhere no offset can
    /// describe — which is `a_marked_passage_on_a_line_the_sort_moved_does_not_survive_it`, the one
    /// honest exception and the reason it has a test of its own.
    #[test]
    fn a_mark_a_fold_and_a_breakpoint_follow_the_text_through_every_line_command() {
        // Commenting: everything stays on the line it was on, with three bytes in front of it.
        let mut document = marked("alpha\nbravo marked\ncharlie\ndelta\n", 2);
        all_of(&mut document);
        document.apply(comment("// "));
        assert_eq!(marks_of(&document), ("marked".to_owned(), 2, 2));

        // A block comment round the middle line, the same.
        let mut document = marked("alpha\nbravo marked\ncharlie\ndelta\n", 2);
        selected(&mut document, 6, 18);
        document
            .apply(Command::ToggleBlockComment { open: "/*".to_owned(), close: "*/".to_owned() });
        assert_eq!(document.text().to_string(), "alpha\n/*bravo marked*/\ncharlie\ndelta\n");
        assert_eq!(marks_of(&document), ("marked".to_owned(), 2, 2));

        // Duplicating: the copy goes in underneath, so nothing on the original line moves.
        let mut document = marked("alpha\nbravo marked\ncharlie\ndelta\n", 2);
        caret_at(&mut document, 8);
        document.apply(Command::DuplicateLines);
        assert_eq!(marks_of(&document), ("marked".to_owned(), 2, 2));

        // Moving down: all three land on the line the block landed on.
        let mut document = marked("alpha\nbravo marked\ncharlie\ndelta\n", 2);
        caret_at(&mut document, 8);
        document.apply(Command::MoveLines { by: 1 });
        assert_eq!(document.text().to_string(), "alpha\ncharlie\nbravo marked\ndelta\n");
        assert_eq!(marks_of(&document), ("marked".to_owned(), 3, 3));

        // And moving up.
        let mut document = marked("alpha\nbravo marked\ncharlie\ndelta\n", 2);
        caret_at(&mut document, 8);
        document.apply(Command::MoveLines { by: -1 });
        assert_eq!(document.text().to_string(), "bravo marked\nalpha\ncharlie\ndelta\n");
        assert_eq!(marks_of(&document), ("marked".to_owned(), 1, 1));

        // Joining pulls the line below up, so the line itself stays where it is.
        let mut document = marked("alpha\nbravo marked\ncharlie\ndelta\n", 2);
        caret_at(&mut document, 8);
        document.apply(Command::JoinLines);
        assert_eq!(document.text().to_string(), "alpha\nbravo marked charlie\ndelta\n");
        assert_eq!(marks_of(&document), ("marked".to_owned(), 2, 2));

        // Sorting, with the marked line already in the place the sort puts it.
        let mut document = marked("apple marked\ncherry\nbanana\n", 1);
        all_of(&mut document);
        document.apply(Command::SortLines);
        assert_eq!(document.text().to_string(), "apple marked\nbanana\ncherry\n");
        assert_eq!(marks_of(&document), ("marked".to_owned(), 1, 1));

        // And trimming, where what goes is at the far end of a line from all three.
        let mut document = marked("alpha   \nbravo marked   \ncharlie\n", 2);
        document.apply(Command::TrimTrailingWhitespace);
        assert_eq!(document.text().to_string(), "alpha\nbravo marked\ncharlie\n");
        assert_eq!(marks_of(&document), ("marked".to_owned(), 2, 2));
    }

    /// A document with the word `marked` marked, and a collapsed block and a breakpoint on line
    /// `line`, counting from one.
    fn marked(source: &str, line: usize) -> Document {
        let mut document = Document::from_text(source);
        let at = source.find("marked").expect("the word the tests read back");
        document.highlight(at..at + "marked".len(), Rgba::new(0xFF, 0xC0, 0x40, 0x60));
        let mut folds = Folds::new();
        folds.add(document.offset_of_line_number(line));
        document.set_folds(folds);
        document.toggle_breakpoint(document.offset_of_line_number(line));
        document
    }

    /// What the mark now covers, and which line the fold and the breakpoint are on. A zero says the
    /// thing is gone, which is a failure everywhere it is read.
    fn marks_of(document: &Document) -> (String, usize, usize) {
        let text = document.text().to_string();
        let mark = document
            .highlights()
            .iter()
            .next()
            .map(|mark| text[mark.range.clone()].to_owned())
            .unwrap_or_default();
        let fold =
            document.folds().offsets().first().map(|at| document.line_number_of(*at)).unwrap_or(0);
        let breakpoint = document
            .breakpoints()
            .all()
            .first()
            .map(|breakpoint| document.line_number_of(breakpoint.offset))
            .unwrap_or(0);
        (mark, fold, breakpoint)
    }

    /// The one thing a line command cannot carry, written down rather than left to be rediscovered.
    ///
    /// A sort is a permutation, and `Document::splice` describes a range going and a range arriving
    /// rather than a line changing places. The fold and the breakpoint survive because they sit at
    /// the start of their line and a line keeps its number; the mark does not, because the words it
    /// was drawn over are somewhere else in the file. Sorting lines somebody has marked passages on
    /// is not a thing anybody does, and the alternative is a second place that knows bytes moved.
    #[test]
    fn a_marked_passage_on_a_line_the_sort_moved_does_not_survive_it() {
        let mut document = marked("cherry\napple\nbravo marked\n", 3);
        all_of(&mut document);
        document.apply(Command::SortLines);
        assert_eq!(document.text().to_string(), "apple\nbravo marked\ncherry\n");
        let (mark, fold, breakpoint) = marks_of(&document);
        assert_eq!(mark, "", "the mark went with the words, which are not on that line any more");
        assert_eq!(fold, 3, "the fold and the breakpoint keep the line they were put on");
        assert_eq!(breakpoint, 3);
    }

    // ------------------------------------------------------------------------- the comment toggles

    /// Every marker goes in at one column, so a block that was aligned is aligned commented out.
    #[test]
    fn a_line_comment_goes_in_at_the_least_indented_line_of_the_block() {
        let mut document = Document::from_text("    one\n        two\n    three\n");
        all_of(&mut document);
        document.apply(comment("// "));
        assert_eq!(document.text().to_string(), "    // one\n    //     two\n    // three\n");
    }

    /// A blank line is neither commented nor asked about, which is what makes the toggle exact.
    #[test]
    fn a_blank_line_in_a_selection_is_left_alone() {
        let mut document = Document::from_text("one\n\ntwo\n");
        all_of(&mut document);
        document.apply(comment("#"));
        assert_eq!(document.text().to_string(), "#one\n\n#two\n");
        document.apply(comment("#"));
        assert_eq!(document.text().to_string(), "one\n\ntwo\n");
    }

    /// One line still commented anywhere in the selection means the press comments rather than
    /// uncomments, which is what stops a half-commented block being made more half-commented.
    #[test]
    fn one_line_that_is_not_commented_makes_the_press_comment() {
        let mut document = Document::from_text("// one\ntwo\n");
        all_of(&mut document);
        document.apply(comment("// "));
        assert_eq!(document.text().to_string(), "// // one\n// two\n");
    }

    /// With nothing selected it is the line the caret is on, as `Indent` is.
    #[test]
    fn with_nothing_selected_a_line_comment_is_the_caret_s_line() {
        let mut document = Document::from_text("one\ntwo\nthree\n");
        caret_at(&mut document, 5);
        document.apply(comment("// "));
        assert_eq!(document.text().to_string(), "one\n// two\nthree\n");
    }

    /// A selection of nothing but blank lines is nothing to do, so there is no undo step either.
    #[test]
    fn a_selection_of_blank_lines_is_not_an_edit() {
        let mut document = Document::from_text("one\n\n\ntwo\n");
        selected(&mut document, 4, 6);
        assert!(!document.apply(comment("// ")));
        assert!(!document.can_undo());
    }

    /// A block comment wraps the words rather than the indentation, and a second press takes it off.
    #[test]
    fn a_block_comment_hugs_the_words_and_toggles_off_again() {
        let source = "    let a = 1;\n";
        let mut document = Document::from_text(source);
        selected(&mut document, 0, 14);
        document
            .apply(Command::ToggleBlockComment { open: "/*".to_owned(), close: "*/".to_owned() });
        assert_eq!(document.text().to_string(), "    /*let a = 1;*/\n");
        document
            .apply(Command::ToggleBlockComment { open: "/*".to_owned(), close: "*/".to_owned() });
        assert_eq!(document.text().to_string(), source);
    }

    /// With nothing selected the line is what is wrapped, and the caret lands inside the markers.
    #[test]
    fn a_block_comment_with_no_selection_wraps_the_line() {
        let mut document = Document::from_text("one\ntwo\n");
        caret_at(&mut document, 5);
        document.apply(Command::ToggleBlockComment {
            open: "<!--".to_owned(),
            close: "-->".to_owned(),
        });
        assert_eq!(document.text().to_string(), "one\n<!--two-->\n");
        assert_eq!(document.selection(), Selection::caret(8));
    }

    // ---------------------------------------------------------------------------- the line editing

    #[test]
    fn duplicating_puts_the_copy_underneath_and_leaves_the_selection_where_it_was() {
        let mut document = Document::from_text("one\ntwo\nthree\n");
        selected(&mut document, 0, 7);
        document.apply(Command::DuplicateLines);
        assert_eq!(document.text().to_string(), "one\ntwo\none\ntwo\nthree\n");
        assert_eq!(document.selection().range(), 0..7, "a second press duplicates the same lines");
        document.apply(Command::DuplicateLines);
        assert_eq!(document.text().to_string(), "one\ntwo\none\ntwo\none\ntwo\nthree\n");
    }

    /// The last line of a file with no line break at the end of it needs one in front of the copy.
    #[test]
    fn duplicating_the_last_line_of_a_file_with_no_line_break_gives_two_lines() {
        let mut document = Document::from_text("one\ntwo");
        caret_at(&mut document, 5);
        document.apply(Command::DuplicateLines);
        assert_eq!(document.text().to_string(), "one\ntwo\ntwo");
    }

    #[test]
    fn moving_a_line_down_and_back_up_again_gives_back_the_file() {
        let source = "one\ntwo\nthree\n";
        let mut document = Document::from_text(source);
        caret_at(&mut document, 1);
        document.apply(Command::MoveLines { by: 1 });
        assert_eq!(document.text().to_string(), "two\none\nthree\n");
        assert_eq!(document.selection(), Selection::caret(5), "the caret went with the line");
        document.apply(Command::MoveLines { by: -1 });
        assert_eq!(document.text().to_string(), source);
        assert_eq!(document.selection(), Selection::caret(1));
    }

    /// The selection covers the same lines afterwards, which is what makes the chord repeat.
    #[test]
    fn a_selection_of_several_lines_moves_as_one_block() {
        let mut document = Document::from_text("one\ntwo\nthree\nfour\n");
        selected(&mut document, 4, 13);
        document.apply(Command::MoveLines { by: 1 });
        assert_eq!(document.text().to_string(), "one\nfour\ntwo\nthree\n");
        assert_eq!(document.selection().range(), 9..18);
        assert_eq!(&document.text().to_string()[9..18], "two\nthree");
    }

    /// A move of more than one line is that many moves in one undo step.
    #[test]
    fn a_move_of_two_lines_moves_two_lines() {
        let mut document = Document::from_text("one\ntwo\nthree\nfour\n");
        caret_at(&mut document, 0);
        document.apply(Command::MoveLines { by: 2 });
        assert_eq!(document.text().to_string(), "two\nthree\none\nfour\n");
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "one\ntwo\nthree\nfour\n");
    }

    /// A block against the edge of the file does not move, and leaves nothing to undo.
    ///
    /// The bottom edge is the last line there is, which in a file that ends in a line break is the
    /// empty line after it — a line the gutter numbers and the caret can be put on, so a line the
    /// one above it can be swapped with. The file used here has no line break at the end of it, so
    /// `two` really is the last line.
    #[test]
    fn a_block_against_the_edge_of_the_file_does_not_move() {
        let mut document = Document::from_text("one\ntwo");
        caret_at(&mut document, 0);
        assert!(!document.apply(Command::MoveLines { by: -1 }));
        assert!(!document.can_undo());
        caret_at(&mut document, 5);
        assert!(!document.apply(Command::MoveLines { by: 1 }));
        assert!(!document.can_undo());
    }

    /// The last line of a file with no line break at the end of it: the break moves with the lines.
    #[test]
    fn moving_past_the_last_line_of_a_file_with_no_line_break_keeps_the_lines_apart() {
        let mut document = Document::from_text("one\ntwo");
        caret_at(&mut document, 0);
        document.apply(Command::MoveLines { by: 1 });
        assert_eq!(document.text().to_string(), "two\none");
        document.apply(Command::MoveLines { by: -1 });
        assert_eq!(document.text().to_string(), "one\ntwo");
    }

    #[test]
    fn joining_puts_one_space_in_and_takes_the_indentation_out() {
        let mut document = Document::from_text("one   \n     two\nthree\n");
        caret_at(&mut document, 0);
        document.apply(Command::JoinLines);
        assert_eq!(document.text().to_string(), "one two\nthree\n");
        assert_eq!(document.selection(), Selection::caret(3));
        document.apply(Command::JoinLines);
        assert_eq!(document.text().to_string(), "one two three\n");
    }

    /// A blank line between two lines with words on them goes, and one space is left behind.
    #[test]
    fn joining_over_a_blank_line_still_leaves_one_space() {
        let mut document = Document::from_text("one\n\ntwo\n");
        all_of(&mut document);
        document.apply(Command::JoinLines);
        assert_eq!(document.text().to_string(), "one two\n");
    }

    /// With a selection every line it touches becomes one line.
    #[test]
    fn joining_a_selection_makes_one_line_of_all_of_it() {
        let mut document = Document::from_text("one\ntwo\nthree\nfour\n");
        selected(&mut document, 0, 13);
        document.apply(Command::JoinLines);
        assert_eq!(document.text().to_string(), "one two three\nfour\n");
    }

    #[test]
    fn the_last_line_of_a_file_has_nothing_to_join_to() {
        let mut document = Document::from_text("one\ntwo");
        caret_at(&mut document, 5);
        assert!(!document.apply(Command::JoinLines));
        assert!(!document.can_undo());
    }

    #[test]
    fn sorting_puts_the_selection_s_lines_in_byte_order() {
        let mut document = Document::from_text("cherry\nApple\nbanana\n");
        all_of(&mut document);
        document.apply(Command::SortLines);
        assert_eq!(
            document.text().to_string(),
            "Apple\nbanana\ncherry\n",
            "byte order, so a capital letter comes first"
        );
        assert_eq!(document.selection().range(), 0..20, "over the lines it sorted");
    }

    /// Stable: two lines that are the same stay in the order somebody wrote them, which is only
    /// visible through something else the lines carry — here the formatting on one of them.
    #[test]
    fn sorting_keeps_equal_lines_in_the_order_they_were_in() {
        let mut document = Document::from_text("b\na\na\n");
        selected(&mut document, 2, 3);
        document.apply(Command::ToggleBold);
        all_of(&mut document);
        document.apply(Command::SortLines);
        assert_eq!(document.text().to_string(), "a\na\nb\n");
        assert!(document.text().to_string().is_char_boundary(0));
        let bold: Vec<bool> =
            [0usize, 2, 4].iter().map(|at| document.chars().style_at(*at).bold).collect();
        assert_eq!(bold, vec![true, false, false], "the first `a` is still the one that was bold");
    }

    #[test]
    fn sorting_with_nothing_selected_is_not_an_edit() {
        let mut document = Document::from_text("cherry\napple\n");
        caret_at(&mut document, 0);
        assert!(!document.apply(Command::SortLines));
        assert!(!document.can_undo());
    }

    #[test]
    fn sorting_lines_that_are_already_in_order_is_not_an_edit() {
        let mut document = Document::from_text("apple\nbanana\n");
        all_of(&mut document);
        assert!(!document.apply(Command::SortLines));
        assert!(!document.can_undo());
    }

    #[test]
    fn trimming_takes_the_trailing_whitespace_off_every_line() {
        let mut document = Document::from_text("one   \ntwo\t\n\t\nthree ");
        document.apply(Command::TrimTrailingWhitespace);
        assert_eq!(document.text().to_string(), "one\ntwo\n\nthree");
    }

    /// A caret standing in the whitespace that went lands at the end of what is left of its line.
    #[test]
    fn trimming_brings_the_caret_back_to_the_end_of_its_line() {
        let mut document = Document::from_text("one   \ntwo\n");
        caret_at(&mut document, 5);
        document.apply(Command::TrimTrailingWhitespace);
        assert_eq!(document.text().to_string(), "one\ntwo\n");
        assert_eq!(document.selection(), Selection::caret(3));
    }

    #[test]
    fn trimming_a_file_with_nothing_to_trim_is_not_an_edit() {
        let mut document = Document::from_text("one\ntwo\n");
        assert!(!document.apply(Command::TrimTrailingWhitespace));
        assert!(!document.can_undo());
    }

    // ---------------------------------------------------------------------------------- the undo

    /// Every one of them is one undo step, however many lines it touched. That follows from one
    /// `push_undo` and then every edit, which is `replace_many`'s rule.
    #[test]
    fn every_line_command_is_one_undo_step() {
        let commands = [
            comment("// "),
            Command::ToggleBlockComment { open: "/*".to_owned(), close: "*/".to_owned() },
            Command::DuplicateLines,
            Command::MoveLines { by: 1 },
            Command::JoinLines,
            Command::SortLines,
            Command::TrimTrailingWhitespace,
        ];
        for command in commands {
            let mut document = Document::from_text("cherry  \napple\nbanana\ndamson\n");
            all_of(&mut document);
            let before = document.text().to_string();
            assert!(document.apply(command.clone()), "{command:?} changed nothing");
            assert_ne!(document.text().to_string(), before);
            document.apply(Command::Undo);
            assert_eq!(document.text().to_string(), before, "{command:?} is not one step");
            assert!(!document.can_undo(), "{command:?} pushed more than one step");
        }
    }

    /// And every one of them says the file has to be coloured again. A comment marker, a moved line
    /// and a sorted block change what the tokens are rather than shifting them, so a reading that
    /// started at the edit would be reading against the wrong list.
    #[test]
    fn every_line_command_says_the_colours_have_to_be_read_again() {
        let commands = [
            comment("// "),
            Command::ToggleBlockComment { open: "/*".to_owned(), close: "*/".to_owned() },
            Command::DuplicateLines,
            Command::MoveLines { by: 1 },
            Command::JoinLines,
            Command::SortLines,
            Command::TrimTrailingWhitespace,
        ];
        for command in commands {
            let mut document = Document::from_text("cherry  \napple\nbanana\ndamson\n");
            document.set_syntax(Color::WHITE, &[]);
            assert_eq!(document.syntax_dirt(), crate::incremental::Dirt::Clean);
            all_of(&mut document);
            document.apply(command.clone());
            assert_eq!(
                document.syntax_dirt(),
                crate::incremental::Dirt::Whole,
                "{command:?} left the colours as they were"
            );
        }
    }
}

/// Matching one bracket against the one that answers it.
#[cfg(test)]
mod bracket_tests {
    use super::*;
    use crate::folding::Tokens;
    use crate::syntax::Grammar;

    /// Enough of Rust for a tokeniser to find the comments and the strings in the sample below.
    fn rust() -> Grammar {
        Grammar {
            language: "rust".to_owned(),
            keywords: vec!["fn".to_owned(), "let".to_owned()],
            line_comment: Some("//".to_owned()),
            block_comment: Some(("/*".to_owned(), "*/".to_owned())),
            strings: vec!['"'],
            escapes: true,
            operators: vec!['{', '}', '(', ')'],
            numbers: true,
            ..Grammar::default()
        }
    }

    const SOURCE: &str = "fn main() {\n    let a = \"}\";\n    // }\n}\n";

    fn read(source: &str) -> Tokens {
        crate::folding::tokens(source, &rust())
    }

    /// The brace that closes the function is the one at the end, not the one in the string and not
    /// the one in the comment. That is the whole of what handing the tokens in buys.
    #[test]
    fn a_brace_inside_a_string_or_a_comment_is_not_a_bracket() {
        let document = Document::from_text(SOURCE);
        let open = SOURCE.find('{').expect("the opening brace");
        let close = SOURCE.rfind('}').expect("the closing brace");
        assert_eq!(document.matching_bracket(open, &read(SOURCE)), Some(close));
        assert_eq!(document.bracket_pair(open, &read(SOURCE)), Some((open, close)));
    }

    /// And with nothing read, the same question answers with the brace in the string — which is
    /// what a caller that hands in an empty `Tokens` is asking for and should know it is getting.
    #[test]
    fn with_no_tokens_read_a_brace_in_a_string_answers_for_one() {
        let document = Document::from_text(SOURCE);
        let open = SOURCE.find('{').expect("the opening brace");
        let in_a_string = SOURCE.find("\"}\"").expect("the string") + 1;
        assert_eq!(document.matching_bracket(open, &Tokens::default()), Some(in_a_string));
    }

    /// The caret is between two bytes, so a closer is found from the far side of it as well.
    #[test]
    fn the_bracket_behind_the_caret_is_asked_about_too() {
        let document = Document::from_text(SOURCE);
        let open = SOURCE.find('(').expect("the open parenthesis");
        let close = SOURCE.find(')').expect("the close parenthesis");
        assert_eq!(document.matching_bracket(close + 1, &read(SOURCE)), Some(open));
        assert_eq!(document.matching_bracket(close, &read(SOURCE)), Some(open));
    }

    #[test]
    fn nesting_of_the_same_bracket_is_counted() {
        let source = "a[b[c]d]e";
        let document = Document::from_text(source);
        assert_eq!(document.matching_bracket(1, &Tokens::default()), Some(7));
        assert_eq!(document.matching_bracket(3, &Tokens::default()), Some(5));
    }

    #[test]
    fn a_caret_on_nothing_and_a_bracket_that_never_closes_answer_nothing() {
        let document = Document::from_text("fn main( {\n");
        assert_eq!(
            document.matching_bracket(2, &Tokens::default()),
            None,
            "a letter is not a bracket"
        );
        let open = 7;
        assert_eq!(document.matching_bracket(open, &Tokens::default()), None, "nothing closes it");
    }

    /// A byte in the middle of a character is not a bracket, and asking about one is not a panic.
    #[test]
    fn a_byte_inside_a_character_answers_nothing() {
        let document = Document::from_text("éé");
        for offset in 0..=4 {
            assert_eq!(document.matching_bracket(offset, &Tokens::default()), None);
        }
    }
}

/// What a new line's indentation should be. The reckoning only: when it is applied is the window's.
#[cfg(test)]
mod auto_indent_tests {
    use super::*;

    #[test]
    fn a_new_line_starts_with_the_indentation_of_the_line_it_was_started_on() {
        assert_eq!(indentation_for_a_new_line("        let a = 1;", 18), "        ");
        assert_eq!(indentation_for_a_new_line("\t\tlet a = 1;", 12), "\t\t");
        assert_eq!(indentation_for_a_new_line("let a = 1;", 10), "");
    }

    /// The line the offset is on, not the first line of the file.
    #[test]
    fn it_is_the_line_the_offset_is_on() {
        let text = "fn main() {\n    let a = 1;\n";
        assert_eq!(indentation_for_a_new_line(text, text.len() - 1), "    ");
        assert_eq!(indentation_for_a_new_line(text, 11), "");
    }

    /// Clamped to the offset, so that pressing Enter in the middle of a line's indentation does not
    /// double it: the whitespace after the caret goes down to the new line by itself.
    #[test]
    fn it_is_no_longer_than_what_is_in_front_of_the_offset() {
        assert_eq!(indentation_for_a_new_line("        let a = 1;", 3), "   ");
        assert_eq!(indentation_for_a_new_line("        ", 8), "        ");
    }

    /// An offset in the middle of a character answers rather than panicking, which is the rule every
    /// offset taken from outside this crate is read under.
    #[test]
    fn an_offset_inside_a_character_is_not_a_panic() {
        assert_eq!(indentation_for_a_new_line("  é", 3), "  ");
        assert_eq!(indentation_for_a_new_line("é", 1), "");
        assert_eq!(indentation_for_a_new_line("  é", 99), "  ");
    }

    /// The document asks it of the caret's own line, so the window does not have to build the whole
    /// file's text to ask about one line.
    #[test]
    fn the_document_answers_for_the_caret_s_line() {
        let mut document = Document::from_text("fn main() {\n    let a = 1;\n");
        document.apply(Command::PlaceCaret { offset: 25, extend: false });
        assert_eq!(document.indentation_for_a_new_line(), "    ");
        document.apply(Command::PlaceCaret { offset: 5, extend: false });
        assert_eq!(document.indentation_for_a_new_line(), "");
    }
}

#[cfg(test)]
mod grouping {
    use super::*;

    /// A run of Backspaces is one undo step, as a run of typing is.
    ///
    /// `task-1984` C13. `EditKind::Deleting` was recorded and the only comparison against it was
    /// `kind == EditKind::Typing`, so it behaved exactly as `Other`: holding Backspace was one whole
    /// document snapshot per character and one undo step per character. On a 2 MB file that is
    /// 15.5 MB a keystroke, and it is why undoing a deleted word took as many presses as the word
    /// had letters.
    #[test]
    fn a_run_of_backspaces_is_one_undo_step() {
        let mut document = Document::from_text("hello world\n");
        document.apply(Command::PlaceCaret { offset: 11, extend: false });
        for _ in 0..5 {
            document.apply(Command::DeleteBackward);
        }
        assert_eq!(document.text().to_string(), "hello \n");

        document.apply(Command::Undo);
        assert_eq!(
            document.text().to_string(),
            "hello world\n",
            "one press brings back the whole run rather than one letter of it"
        );
    }

    /// A caret move between two deletes breaks the run, which is what keeps an undo step the size of
    /// an action rather than of a session.
    #[test]
    fn a_caret_move_breaks_a_run_of_deleting() {
        let mut document = Document::from_text("one two three\n");
        document.apply(Command::PlaceCaret { offset: 13, extend: false });
        document.apply(Command::DeleteBackward);
        document.apply(Command::DeleteBackward);
        document.apply(Command::PlaceCaret { offset: 3, extend: false });
        document.apply(Command::DeleteBackward);
        assert_eq!(document.text().to_string(), "on two thr\n");

        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "one two thr\n", "the second run came back");
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "one two three\n", "and then the first");
    }

    /// Typing and deleting are two runs, not one.
    #[test]
    fn typing_and_deleting_are_two_runs() {
        let mut document = Document::from_text("");
        for letter in "abc".chars() {
            document.apply(Command::Insert(letter.to_string()));
        }
        document.apply(Command::DeleteBackward);
        document.apply(Command::DeleteBackward);
        assert_eq!(document.text().to_string(), "a");

        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "abc", "the deleting undid as one step");
        document.apply(Command::Undo);
        assert_eq!(document.text().to_string(), "", "and the typing as another");
    }
}

#[cfg(test)]
mod caret_windows {
    use super::*;

    /// A line longer than the window is still moved through one grapheme and one word at a time.
    ///
    /// **`task-1984` C5.** `line_window` handed the whole line to the grapheme and word walks, and a
    /// minified `.js`, a one line `.json` or a single log line is a line a megabyte long — where one
    /// arrow key was measured at 15.9 ms and holding one dropped every frame. The window is bounded
    /// now, and what has to stay true is that every answer inside a line is the answer it was.
    #[test]
    fn a_line_longer_than_the_window_moves_a_grapheme_and_a_word_at_a_time() {
        // A single line far longer than `CARET_WINDOW`, with words in it.
        let words = "alpha beta gamma delta ".repeat(4_000);
        let mut document = Document::from_text(&words);
        // A caret deep inside it, on a word boundary, so what is being tested is the middle of a line
        // rather than either end.
        let caret = words.find("gamma").expect("it is in there") + 20_000;
        let caret = words[..caret].rfind(' ').expect("a space") + 1;
        document.apply(Command::PlaceCaret { offset: caret, extend: false });

        document.apply(Command::MoveRight { extend: false });
        assert_eq!(document.selection().head, caret + 1, "one grapheme");
        document.apply(Command::MoveLeft { extend: false });
        assert_eq!(document.selection().head, caret, "and back");

        document.apply(Command::MoveWordRight { extend: false });
        let after = document.selection().head;
        assert!(after > caret, "the word moved forward, to {after}");
        assert!(after - caret < 32, "by one word rather than to the end of the line");
        document.apply(Command::MoveWordLeft { extend: false });
        assert_eq!(document.selection().head, caret, "and back again");
    }

    /// And the ends of such a line are still its ends.
    #[test]
    fn the_ends_of_a_long_line_are_where_the_rope_says_they_are() {
        let source = format!("{}\nsecond\n", "x".repeat(50_000));
        let mut document = Document::from_text(&source);
        document.apply(Command::PlaceCaret { offset: 25_000, extend: false });
        document.apply(Command::MoveLineStart { extend: false });
        assert_eq!(document.selection().head, 0, "the first line starts at nothing");
        document.apply(Command::PlaceCaret { offset: 25_000, extend: false });
        document.apply(Command::MoveLineEnd { extend: false });
        assert_eq!(document.selection().head, 50_000, "and ends before the line break");
    }

    /// A multi byte character at the window's edge does not split a `str`.
    #[test]
    fn a_window_edge_lands_on_a_character_boundary() {
        // Three byte characters throughout, so nearly every offset is inside one.
        let source = "\u{4f60}".repeat(20_000);
        let mut document = Document::from_text(&source);
        // A caret at a boundary in the middle, and then a step each way.
        let caret = 3 * 10_000;
        document.apply(Command::PlaceCaret { offset: caret, extend: false });
        document.apply(Command::MoveRight { extend: false });
        assert_eq!(document.selection().head, caret + 3, "one whole character");
        document.apply(Command::MoveLeft { extend: false });
        document.apply(Command::MoveLeft { extend: false });
        assert_eq!(document.selection().head, caret - 3, "and one back the other way");
    }
}

#[cfg(test)]
mod replacing_many {
    use super::*;

    /// The reading `replace_many` used to be: one style read and one colour written per turn.
    ///
    /// Kept here so the two can be compared. It is the loop as it stood before `task-1984` C6, with
    /// nothing else changed -- the same filtering and ordering above it, so what differs between this
    /// and [`Document::replace_many`] is only the part C6 moved.
    fn the_old_way(document: &mut Document, edits: Vec<(Range<usize>, String)>) {
        let mut edits: Vec<(Range<usize>, String)> = edits
            .into_iter()
            .filter(|(range, replacement)| {
                (range.start < range.end || !replacement.is_empty())
                    && range.start <= range.end
                    && range.end <= document.text.len_bytes()
                    && document.clamp_to_boundary(range.start) == range.start
                    && document.clamp_to_boundary(range.end) == range.end
            })
            .collect();
        edits.sort_by_key(|(range, _)| range.start);
        let mut reached = 0;
        edits.retain(|(range, _)| {
            if range.start < reached {
                return false;
            }
            reached = range.end;
            true
        });
        edits.reverse();
        if edits.is_empty() {
            return;
        }
        document.push_undo(EditKind::Other);
        for (range, replacement) in edits {
            let at = range.start;
            let style = document.chars.style_for_insertion(at);
            document.remove_range(range);
            if replacement.is_empty() {
                continue;
            }
            document.splice(at..at, &replacement);
            document.chars.set(at..at + replacement.len(), &style_as_change(&style));
            document.selection.set_caret(at + replacement.len());
        }
        document.desired_x = None;
        document.mark_changed();
        document.last_edit = EditKind::Other;
    }

    /// A coloured document with one colour per word, which is where a colour landing on the wrong
    /// word would show.
    fn coloured(text: &str) -> Document {
        let mut document = Document::from_text(text);
        let mut at = 0usize;
        let mut index = 0u8;
        for word in text.split(' ') {
            index += 1;
            document.apply(Command::PlaceCaret { offset: at, extend: false });
            document.apply(Command::PlaceCaret { offset: at + word.len(), extend: true });
            document.apply(Command::ApplyStyle(StyleChange::color(Color::rgb(index * 10, 0, 0))));
            at += word.len() + 1;
        }
        document.apply(Command::PlaceCaret { offset: 0, extend: false });
        document
    }

    /// Every span of a document, as a length and the red channel of its colour.
    fn shape(document: &Document) -> Vec<(usize, u8)> {
        document
            .chars()
            .spans()
            .map(|(range, style)| (range.end - range.start, style.color.r))
            .collect()
    }

    /// Replacing many ranges leaves exactly what the loop it replaced left.
    ///
    /// **`task-1984` C6.** `replace_many` read a style and wrote a colour inside the same loop turn,
    /// and both of those walk the whole span list -- so Replace All over 210 matches in a coloured
    /// 117 KB file walked 10,084 spans four hundred and twenty times: **27.5 ms coloured against
    /// 0.24 ms uncoloured**, a colour scheme deciding what an edit costs. It reads every style in one
    /// pass, applies every edit, and writes every colour in one call now, and the arithmetic that
    /// works out where each replacement ended up is where a colour could land on the wrong word.
    ///
    /// So this is a differential test against the loop itself rather than against an expectation
    /// somebody wrote down: same text, same edits, same everything, compared span for span. The
    /// replacements are longer, shorter and the same length, and there is an empty one, so no two
    /// edits shift the ones after them by the same amount.
    #[test]
    fn replacing_many_ranges_leaves_what_the_loop_it_replaced_left() {
        let cases: Vec<Vec<(Range<usize>, String)>> = vec![
            vec![(0..3, "AAAAAA".to_owned())],
            vec![(0..3, "AAAAAA".to_owned()), (8..11, "C".to_owned())],
            vec![(0..3, "AAAAAA".to_owned()), (8..11, "C".to_owned()), (16..19, "EEE".to_owned())],
            vec![(4..7, String::new()), (12..15, "DDDDDDDD".to_owned())],
            vec![(3..3, "|".to_owned()), (7..7, "||".to_owned())],
            vec![(0..19, "one".to_owned())],
            // Out of order and overlapping, which the filtering above the loop has to settle the
            // same way in both readings.
            vec![(12..15, "d".to_owned()), (0..3, "a".to_owned()), (1..5, "x".to_owned())],
        ];
        for edits in cases {
            let mut mine = coloured("aaa bbb ccc ddd eee");
            let mut reference = coloured("aaa bbb ccc ddd eee");
            assert!(mine.apply(Command::ReplaceMany(edits.clone())));
            the_old_way(&mut reference, edits.clone());
            assert_eq!(
                mine.text().to_string(),
                reference.text().to_string(),
                "the text, for {edits:?}"
            );
            assert_eq!(shape(&mine), shape(&reference), "the formatting, for {edits:?}");
            assert_eq!(
                mine.selection().head,
                reference.selection().head,
                "the caret, for {edits:?}"
            );
        }
    }

    /// And every replacement is still one undo step.
    #[test]
    fn replacing_many_ranges_is_one_undo_step() {
        let mut document = coloured("aaa bbb ccc ddd eee");
        let before = document.text().to_string();
        let edits =
            vec![(0..3, "AAAAAA".to_owned()), (8..11, String::new()), (16..19, "EEE".to_owned())];
        assert!(document.apply(Command::ReplaceMany(edits)));
        assert_ne!(document.text().to_string(), before);
        assert!(document.apply(Command::Undo));
        assert_eq!(document.text().to_string(), before, "one step back");
    }
}

#[cfg(test)]
mod splicing {
    use super::*;
    use crate::breakpoints::Breakpoint;
    use crate::highlights::Highlight;

    /// An offset moved back to a character boundary, which is what `ReplaceMany` requires.
    fn clamped(document: &Document, mut at: usize) -> usize {
        while at > 0 && !document.text().is_char_boundary(at) {
            at -= 1;
        }
        at
    }

    /// Every structure an edit moves, checked against a plain reckoning of how each one moves.
    ///
    /// **`task-1984` C20.** [`Document::splice`] is the one place in Unluminous that knows a range of
    /// bytes moved, and it moves six things by the same edit: the text, the character formatting, the
    /// marked passages, the folds, the breakpoints and the paragraph list. Its own comment says that a
    /// seventh would otherwise have had four call sites to be threaded through. What nothing checked
    /// is where each one ends up.
    ///
    /// **They do not all move the same way, and that is the finding rather than a fault.** A splice
    /// is a removal and then an insertion, and each structure answers both in its own terms:
    ///
    /// | | text removed over it | text inserted at its own first byte |
    /// |---|---|---|
    /// | a marked passage | cut to what is left, and dropped when nothing is | moves after what was typed |
    /// | a fold | **dropped**, because a fold's offset *is* its head line's first byte | stays where it is |
    /// | a breakpoint | moved to the seam, because a breakpoint is a mark *on* a line | stays where it is |
    ///
    /// Every one of those is right about its own thing. A fold whose head line has gone has nothing
    /// left to fold; a breakpoint whose line has gone belongs on the line the text closed up into,
    /// which is what every editor does; and typing in front of a highlighted word must not draw the
    /// highlight over what was typed. The reckoning below is those three rules written out, and the
    /// test is three hundred random edits held against them.
    ///
    /// **The ticket asked for this as `splice_shifts_every_structure_the_same_way`**, and the name is
    /// different because writing it disproved it. A test named after an assumption it goes on to
    /// contradict is the §3.6 fault this same package spent a day taking out of the comments.
    #[test]
    fn splice_moves_every_structure_by_its_own_rule() {
        let mut seed = 0x0198_4c20_u64;
        let mut next = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };

        let source: String =
            (0..30).map(|line| format!("line {line} with some words on it\n")).collect();
        let mut document = Document::from_text(&source);

        let mut marked: Vec<(usize, usize)> =
            (0..6).map(|step| (step * 90 + 5, step * 90 + 17)).collect();
        document.set_highlights(crate::Highlights::from_list(
            marked.iter().map(|(from, to)| Highlight {
                range: *from..*to,
                color: Rgba::new(255, 255, 0, 96),
            }),
        ));
        let mut folded: Vec<usize> = (0..6).map(|step| step * 120 + 3).collect();
        let mut folds = crate::Folds::new();
        for at in &folded {
            folds.add(*at);
        }
        document.set_folds(folds);
        let mut stops: Vec<usize> = (0..6).map(|step| step * 100 + 7).collect();
        document.set_breakpoints(crate::Breakpoints::from_list(
            stops.iter().map(|at| Breakpoint::at(*at)),
        ));

        let words = ["", "x", "one two", "\nand a line\n", "abc"];
        for round in 0..300 {
            let len = document.text().len_bytes();
            if len < 8 {
                break;
            }
            let start = clamped(&document, next() % len);
            let end = clamped(&document, (start + next() % 40).min(len));
            let replacement = words[next() % words.len()].to_owned();
            let range = start..end;
            if range.is_empty() && replacement.is_empty() {
                continue;
            }
            assert!(
                document.apply(Command::ReplaceMany(vec![(range.clone(), replacement.clone())])),
                "round {round}"
            );

            // The removal, in each structure's own terms.
            let gone = range.end - range.start;
            if gone > 0 {
                // An offset before the deletion stays, one after it comes back by what went, and
                // one inside it lands on the seam -- after which a mark of no width is dropped.
                let shift = |offset: usize| match offset {
                    _ if offset <= range.start => offset,
                    _ if offset >= range.end => offset - gone,
                    _ => range.start,
                };
                for (from, to) in marked.iter_mut() {
                    *from = shift(*from);
                    *to = shift(*to);
                }
                marked.retain(|(from, to)| from < to);
                folded.retain(|at| !(range.start..range.end).contains(at));
                for at in folded.iter_mut() {
                    if *at >= range.end {
                        *at -= gone;
                    }
                }
                for at in stops.iter_mut() {
                    if *at >= range.end {
                        *at -= gone;
                    } else if *at > range.start {
                        *at = range.start;
                    }
                }
                stops.dedup();
            }

            // Then the insertion, in each structure's own terms.
            let grew = replacement.len();
            if grew > 0 {
                let at = range.start;
                for (from, to) in marked.iter_mut() {
                    if *from >= at {
                        *from += grew;
                        *to += grew;
                    } else if *to > at {
                        *to += grew;
                    }
                }
                for offset in folded.iter_mut().chain(stops.iter_mut()) {
                    if *offset > at {
                        *offset += grew;
                    }
                }
            }

            let held: Vec<(usize, usize)> = document
                .highlights()
                .iter()
                .map(|mark| (mark.range.start, mark.range.end))
                .collect();
            assert_eq!(held, marked, "the marked passages, round {round}, after {range:?}");

            let shut: Vec<usize> = document.folds().offsets().to_vec();
            assert_eq!(shut, folded, "the folds, round {round}, after {range:?}");

            let stopped: Vec<usize> =
                document.breakpoints().all().iter().map(|one| one.offset).collect();
            assert_eq!(stopped, stops, "the breakpoints, round {round}, after {range:?}");
        }
    }
}
