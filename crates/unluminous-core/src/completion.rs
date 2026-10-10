//! Offering a name while it is still being typed.
//!
//! [`crate::symbols`] answers "where is this name defined and used" once the name exists; this
//! module is the other half of the same question, asked one letter earlier: *given what has been
//! typed so far, which names could it become*. Like the module beside it, it is pure — it reads a
//! `&str` and a [`Grammar`], it draws nothing, it touches no disk, and its tests run with no
//! window.
//!
//! ## What it decides, and what it does not
//!
//! It decides three things and nothing else: what the **stem** under the caret is, which of a pile
//! of [`Candidate`]s that stem **matches**, and what **order** the matches are offered in. Where
//! the candidates came from is the window's business — `app/completion.rs` gathers them from the
//! open tabs, the project's index and the grammar — and drawing them is the popup's.
//!
//! ## The match, and why it is a subsequence
//!
//! **A candidate matches when the stem is a case-insensitive subsequence of it.** `lyt` finds
//! `layout`, `psttx` finds `paint_text`, and middle matching comes free, so `draw` finds `redraw`.
//! That is the reference editor's documented behaviour and Sublime Text's, and it is the shape
//! [`crate::symbols`]' sibling `services::file_search` already ranks file names by.
//!
//! ## The score, and why the tests pin orderings rather than numbers
//!
//! The rubric is Sublime Text's, restated for identifiers: a large bonus when the candidate starts
//! with the stem, a bonus per matched letter sitting on a word boundary, a bonus per consecutive
//! matched letter, a small bonus per letter whose case agrees exactly, and a penalty per unmatched
//! letter so the shorter of two otherwise-equal names wins.
//!
//! The alignment behind a score is the **best** one rather than the first, found by dynamic
//! programming over the two strings, because `pt` has two readings of `paint_text` and only one of
//! them is the one a person meant. But a score is meaningless outside a comparison, so every test
//! here pins an **order**: a test asserting `-13` would be a test of the constants rather than of
//! anything anybody can see.
//!
//! ## And why the order is total
//!
//! Ties are broken by source, then by the shorter name, then by the name's own bytes, so the same
//! text and the same stem give the same list in the same order every time. That is not tidiness:
//! the popup's screenshot tests and the command line's output both rest on it.
//!
//! ## The order is a chain of weighers (`task-2231` §6.3)
//!
//! A row's place is decided by [`order`]: its match class first ([`MatchClass`]: the name is the stem,
//! starts with it, is reached by its humps, by a later word, or only as a subsequence), then what a
//! language server says about it, how well its kind fits the place, how near its answer is
//! ([`Locality`]), how often it was chosen before, and only then the alignment score. The candidates
//! carry what each source knows about them in [`Info`], so the structural index, a server and a kernel
//! each fill in what they know and the order reads it all the same way.

use std::ops::Range;

use crate::place::Place;
use crate::symbols::SymbolKind;
use crate::syntax::Grammar;

/// A large bonus for a candidate that **starts with** the stem, which is by far the commonest
/// intent: somebody typing `dra` nearly always wants `draw` rather than `redraw`.
const PREFIX: i32 = 30;
/// A matched letter sitting at the start of the name or of a part of it — after `_` or `-`, or at a
/// lower-to-upper camel step. Worth the most of the per-letter bonuses, which is what makes `pt`
/// prefer `paint_text` over `pointer`.
const BOUNDARY: i32 = 12;
/// A matched letter directly after another matched letter.
const CONSECUTIVE: i32 = 6;
/// A matched letter whose case agrees with what was typed. Small, because a bonus that decided
/// anything on its own would make completion case-sensitive by the back door.
const SAME_CASE: i32 = 2;
/// Per letter of the candidate the stem did not match, so the shorter of two otherwise-equal names
/// is offered first.
const UNMATCHED: i32 = 1;

/// A score no alignment can reach, standing for "these two do not line up at all".
const IMPOSSIBLE: i32 = i32::MIN / 4;

/// Where a candidate came from.
///
/// It is carried all the way to the row, because a row that can say `draw_frame · layout.rs` is
/// answering "why is this being offered" and a bare word cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// A definition in the file being typed in.
    ThisFile,
    /// A word of the file being typed in — a local, a parameter, a field name, a CSS property.
    /// Everything the definers cannot see, and the only source a language with no definers has.
    Word,
    /// A definition in another tab that is open, read from its live text.
    OpenTab,
    /// A definition the project's index holds, which is every file that is **not** open. The
    /// ownership rule of `task-1675` §3.3: a file that is open is owned by its `Document`.
    Index,
    /// One of the language's own words: a keyword, a builtin or a type from the manifest.
    Language,
    /// A file or a module, offered inside an import. `task-1680`'s one new source: the rows a
    /// specifier or a module path could become, which are not names inside a file but files.
    Module,
    /// What a running Jupyter kernel answered for a notebook cell: the names that really exist in
    /// it now, which is how `df.` offers a DataFrame's columns. `task-2220`.
    Kernel,
    /// A member of the value before a `.` or `::`, read from the structural index: the fields and
    /// methods of the receiver's type. `task-2231` §6.4.
    Member,
    /// A name the project exports from a file this one does not import, which accepting adds the
    /// import for. `task-2231` §6.5.
    Import,
    /// What a language server answered: rust-analyzer or tsserver. `task-2231` §5.3.
    Server,
}

/// How far from the caret, in lines, a place the name is written counts as near it: about a screen of
/// a function. [`Info::uses_near`].
pub const NEAR_LINES: u32 = 30;

impl Source {
    /// This source's bit in [`Info::offered_by`].
    pub fn bit(self) -> u16 {
        1 << (self as u16)
    }

    /// Which source wins the row when two of them offer the same spelling.
    ///
    /// **A server beats a definition beats a keyword beats a plain word**, because a server's row
    /// carries the edit that inserts it correctly and resolved knowledge of what it is, and a plain
    /// word has nothing at all. A name already in scope beats the same name offered with an import.
    fn describes_itself(self) -> u8 {
        match self {
            Source::Server => 0,
            Source::Module => 1,
            Source::ThisFile => 2,
            Source::Member => 3,
            Source::Kernel => 4,
            Source::OpenTab => 5,
            Source::Index => 6,
            Source::Import => 7,
            Source::Language => 8,
            Source::Word => 9,
        }
    }

    /// The word the command line prints and a test compares against.
    pub fn name(self) -> &'static str {
        match self {
            Source::ThisFile => "this file",
            Source::Word => "word",
            Source::OpenTab => "open tab",
            Source::Index => "project",
            Source::Language => "language",
            Source::Module => "module",
            Source::Kernel => "kernel",
            Source::Member => "member",
            Source::Import => "needs import",
            Source::Server => "server",
        }
    }

    /// How near a source's answer is, when nothing better is known about the row. The gatherer says
    /// more where it knows more: a parameter of the enclosing function is nearer than this file.
    pub fn locality(self) -> Locality {
        match self {
            Source::Member | Source::Module => Locality::Receiver,
            Source::ThisFile | Source::Word | Source::Kernel => Locality::ThisFile,
            Source::OpenTab => Locality::OpenTab,
            Source::Index | Source::Server => Locality::Project,
            Source::Import => Locality::NeedsImport,
            Source::Language => Locality::Language,
        }
    }
}

/// What a row names, finer than the five kinds a definer keyword gives. The structural index
/// (`atrius_index::structure::Kind`) and the language servers both answer in these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Function,
    Method,
    Type,
    Struct,
    Enum,
    Variant,
    Trait,
    Interface,
    Class,
    Field,
    Constant,
    Variable,
    Module,
    Parameter,
    TypeAlias,
    Macro,
    Keyword,
    Snippet,
}

impl Kind {
    /// Every kind.
    pub const ALL: [Kind; 18] = [
        Kind::Function,
        Kind::Method,
        Kind::Type,
        Kind::Struct,
        Kind::Enum,
        Kind::Variant,
        Kind::Trait,
        Kind::Interface,
        Kind::Class,
        Kind::Field,
        Kind::Constant,
        Kind::Variable,
        Kind::Module,
        Kind::Parameter,
        Kind::TypeAlias,
        Kind::Macro,
        Kind::Keyword,
        Kind::Snippet,
    ];

    /// The word the command line prints. The structural index uses the same words, `alias` for a
    /// type alias included, so its rows are read with [`Kind::parse`].
    pub fn name(self) -> &'static str {
        match self {
            Kind::Function => "function",
            Kind::Method => "method",
            Kind::Type => "type",
            Kind::Struct => "struct",
            Kind::Enum => "enum",
            Kind::Variant => "variant",
            Kind::Trait => "trait",
            Kind::Interface => "interface",
            Kind::Class => "class",
            Kind::Field => "field",
            Kind::Constant => "constant",
            Kind::Variable => "variable",
            Kind::Module => "module",
            Kind::Parameter => "parameter",
            Kind::TypeAlias => "alias",
            Kind::Macro => "macro",
            Kind::Keyword => "keyword",
            Kind::Snippet => "snippet",
        }
    }

    /// The kind a word names.
    pub fn parse(word: &str) -> Option<Kind> {
        Kind::ALL.iter().copied().find(|kind| kind.name() == word)
    }

    /// True for a kind a value is made of or a member looked up on.
    pub fn is_type(self) -> bool {
        matches!(
            self,
            Kind::Type
                | Kind::Struct
                | Kind::Enum
                | Kind::Trait
                | Kind::Interface
                | Kind::Class
                | Kind::TypeAlias
        )
    }

    /// True for a kind that is called with brackets.
    pub fn is_callable(self) -> bool {
        matches!(self, Kind::Function | Kind::Method | Kind::Macro)
    }

    /// True for a kind that stands for a value where an expression is written.
    pub fn is_value(self) -> bool {
        matches!(
            self,
            Kind::Variable
                | Kind::Parameter
                | Kind::Constant
                | Kind::Field
                | Kind::Variant
                | Kind::Function
                | Kind::Method
                | Kind::Macro
        )
    }
}

impl From<SymbolKind> for Kind {
    fn from(kind: SymbolKind) -> Self {
        match kind {
            SymbolKind::Function => Kind::Function,
            SymbolKind::Type => Kind::Type,
            SymbolKind::Constant => Kind::Constant,
            SymbolKind::Variable => Kind::Variable,
            SymbolKind::Module => Kind::Module,
        }
    }
}

/// How a stem lines up with a name, best first. `task-2231` §6.2.
///
/// The first weigher of [`order`]: the reference editor's matcher prefers a name the stem starts, then
/// one whose words the stem's letters each start (`lsm` for `layout_scroll_margin`), then one with a
/// later word the stem starts (`scroll` in `layout_scroll`), and only then any other subsequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchClass {
    /// The name is the stem, in another case.
    Exact,
    /// The name starts with the stem.
    Prefix,
    /// Each matched letter starts a word of the name or follows one that does, from the first word.
    Humps,
    /// The stem starts a later word of the name.
    WordStart,
    /// Any other subsequence.
    Subsequence,
}

impl MatchClass {
    /// The word the command line prints.
    pub fn name(self) -> &'static str {
        match self {
            MatchClass::Exact => "exact",
            MatchClass::Prefix => "prefix",
            MatchClass::Humps => "humps",
            MatchClass::WordStart => "word start",
            MatchClass::Subsequence => "subsequence",
        }
    }
}

/// How near a row's answer is to the caret, nearest first: the fourth weigher. `task-2231` §6.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Locality {
    /// A member of the receiver's own type.
    Receiver,
    /// A local or a parameter of the enclosing function.
    Local,
    /// Defined or written in this file.
    ThisFile,
    /// Defined in an open tab.
    OpenTab,
    /// Defined in a file in the same folder.
    SameFolder,
    /// Defined in the same crate or package.
    Package,
    /// Defined somewhere in the project.
    #[default]
    Project,
    /// Defined in the project and not imported here: accepting it adds the import.
    NeedsImport,
    /// Defined in a dependency and not imported here: a language server's auto import from a crate
    /// or a package the project uses, farther than any name of the project's own.
    Dependency,
    /// One of the language's own words.
    Language,
}

/// One change to the text, in the bytes of the document at the revision it was worked out for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

/// What accepting a row puts in the document. `task-2231` §6.7.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Insert {
    /// The name, over the stem (`Enter`) or the word (`Tab`). What every row did before.
    #[default]
    Name,
    /// A server's own text and range. `insert` is the range `Enter` replaces and `replace` the one
    /// `Tab` replaces; the caret goes to `caret` bytes into the text when it is set, after it otherwise.
    Text { insert: Range<usize>, replace: Range<usize>, text: String, caret: Option<usize> },
    /// A call: `name()`, with the caret between the brackets when it takes parameters and after
    /// them when it takes none.
    Call { has_parameters: bool },
}

/// Everything a row carries beyond its name, its source, its kind and its detail.
///
/// Kept in one place so a candidate and the row it becomes cannot disagree about it, and so the
/// structural tier, the server and the kernel can each fill in what they know and leave the rest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Info {
    /// What the stem is matched against when it is not the name: a server's `filterText`.
    pub filter: Option<String>,
    /// The definition's head line, or a server's detail: `fn draw(&self, ui: &mut Ui) -> Response`.
    pub signature: Option<String>,
    /// The type or module it belongs to.
    pub container: Option<String>,
    /// The first line of its documentation, or the whole of it once a server has resolved the row.
    pub doc: Option<String>,
    pub insert: Insert,
    /// Changes elsewhere in the file that accepting makes: the import a name needs.
    pub extra_edits: Vec<Edit>,
    /// The import accepting adds, as it would be written, for the row's detail: `use crate::layout`.
    pub needs_import: Option<String>,
    pub locality: Locality,
    /// True when a server said the row's type is the type expected here.
    pub expected_type: bool,
    /// How many times the name is already written in the file being edited, keywords included. A name
    /// used nearby is the likelier of two that otherwise tie: `push` over `pop` after `flags.` in a file
    /// that pushes onto `flags` already, and `const` over `case` in a file full of `const`.
    pub uses_here: u32,
    /// How many lines above the caret the nearest place the name is written is, `u32::MAX` when it is
    /// not written above it. A name written a line ago is the likelier of two that otherwise tie, which
    /// is the strongest single thing a ranking learned from people's choices reads (`task-2237`).
    pub lines_above: u32,
    /// How many lines below the caret the nearest place the name is written is, `u32::MAX` when none.
    pub lines_below: u32,
    /// How many times the name is written within [`NEAR_LINES`] lines of the caret.
    pub uses_near: u32,
    /// How many places the name is written in this file come straight after the token that comes
    /// straight before the word here: `let` before `mut`, `.` before a method this file calls on a
    /// value, `->` before a type. One file's bigrams, read off the places the name is written.
    pub same_before: u32,
    /// How many of those places are followed by the token that follows the word here: `(` after a
    /// function this file calls, `::` after a module.
    pub same_after: u32,
    /// How many of the name's words (`draw_frame` is `draw` and `frame`, `drawFrame` too) are written
    /// on the lines around the caret, in thousandths of the name's words.
    pub words_nearby: u32,
    /// The learned score the order gave this row, in millionths, larger better. `None` where the chain
    /// alone ordered the rows. What `editor complete --explain` prints. `task-2237`.
    pub learned: Option<i64>,
    /// Where the chain of weighers put this row, from 0, which the learned score reads. Set by
    /// [`order`]. `task-2237`.
    pub chain_rank: u32,
    /// Every source that offered this spelling, one bit a [`Source`] (`1 << source as u16`). A name the
    /// server, this file and the project all offer is a surer answer than one only a server offers.
    pub offered_by: u16,
    /// A server's own order, smaller first, read from its sort text. `None` for every other source.
    pub server_order: Option<u64>,
    pub deprecated: bool,
    /// True when a server asked for this row to be chosen first.
    pub preselect: bool,
    /// True when the server's answer has to be resolved before its documentation and its import are
    /// known.
    pub needs_resolve: bool,
    /// A server's own handle for resolving this row.
    pub handle: Option<String>,
}

/// One thing that could be offered, before anything has been matched against it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// What would be inserted. The whole of the row's identity: the pool is deduplicated by this,
    /// because two entries that would type the same bytes are one offer.
    pub name: String,
    pub source: Source,
    /// What the definition names, where the candidate is one. Nothing for a word.
    pub kind: Option<Kind>,
    /// The quiet suffix a row shows — the defining file's name, or `keyword`. Empty where the
    /// candidate needs no explanation, which is what this file's own words need.
    pub detail: String,
    pub info: Info,
}

impl Candidate {
    /// A candidate with nothing to say about itself but its name, which is what a word is.
    pub fn new(name: impl Into<String>, source: Source) -> Self {
        let info = Info { locality: source.locality(), ..Info::default() };
        Self { name: name.into(), source, kind: None, detail: String::new(), info }
    }

    /// The same, carrying what it is and where it came from.
    pub fn described(
        name: impl Into<String>,
        source: Source,
        kind: Option<Kind>,
        detail: impl Into<String>,
    ) -> Self {
        let info = Info { locality: source.locality(), ..Info::default() };
        Self { name: name.into(), source, kind, detail: detail.into(), info }
    }

    /// The same candidate, nearer or further than its source says.
    pub fn at(mut self, locality: Locality) -> Self {
        self.info.locality = locality;
        self
    }
}

/// One offered row: a candidate that matched, with its score and which of its letters matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub source: Source,
    pub kind: Option<Kind>,
    pub detail: String,
    /// Which **characters** of the name the stem landed on, in order.
    ///
    /// Characters rather than bytes, because that is what picking letters out of a drawn string
    /// counts in — `components::controls::marked_text` walks the name a character at a time — and
    /// because a byte position would be a different number in `déjà` depending on the accents
    /// before it.
    pub matched: Vec<usize>,
    pub score: i32,
    pub class: MatchClass,
    pub info: Info,
}

/// What [`order`] needs to know about the place the rows are offered at, beyond the stem.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Question {
    pub stem: String,
    pub place: Place,
    /// The language of the file, the plugin's id: `rust`, `typescript`. A question with a language is
    /// ordered by the learned score ([`crate::completion_model`]); one with none, an import list or a
    /// kernel's rows, by the chain alone. `task-2237`.
    pub language: String,
    /// What kind of token comes straight before the word, as [`token_class`] numbers it: `let`, `.`,
    /// `::`, `(`. What the model reads beside the place, because `let` and `return` ask for different
    /// names though both start an expression.
    pub before: u8,
    /// What kind of token comes straight after the word on its line: `(` after a function, `:` after a
    /// field, `=` after a variable being set. 0 at the end of the line.
    pub after: u8,
}

/// The words a token class names beyond "a word", each its own class from [`WORD_CLASSES_START`].
const CLASSED_WORDS: [&str; 34] = [
    "let",
    "mut",
    "fn",
    "const",
    "return",
    "use",
    "import",
    "from",
    "pub",
    "impl",
    "struct",
    "new",
    "if",
    "match",
    "for",
    "in",
    "as",
    "await",
    "type",
    "extends",
    "export",
    "async",
    "function",
    "class",
    "interface",
    "enum",
    "crate",
    "self",
    "super",
    "Self",
    "this",
    "typeof",
    "keyof",
    "else",
];

/// Where the word classes start in [`token_class`]'s numbering.
const WORD_CLASSES_START: u8 = 20;

/// A small number standing for a token, the same in the window and in the trainer: 0 for nothing, 1
/// for any other word, the marks from 2, and the words a language asks for one kind of name after from
/// [`WORD_CLASSES_START`]. `task-2237`.
///
/// @param token - the token's bytes
pub fn token_class(token: &[u8]) -> u8 {
    let Ok(token) = std::str::from_utf8(token) else { return 1 };
    if token.is_empty() {
        return 0;
    }
    if let Some(at) = CLASSED_WORDS.iter().position(|word| *word == token) {
        return WORD_CLASSES_START + at as u8;
    }
    let first = token.as_bytes()[0];
    if first.is_ascii_alphanumeric() || first == b'_' || first == b'$' || first >= 0x80 {
        return 1;
    }
    match token {
        "." | "?." => 2,
        "::" => 3,
        "(" => 4,
        "," => 5,
        "=" => 6,
        ":" => 7,
        "{" => 8,
        "}" => 9,
        ";" => 10,
        "<" => 11,
        "->" | "=>" => 12,
        "&" | "&&" => 13,
        "[" => 14,
        ")" => 15,
        "!" => 16,
        "|" | "||" => 17,
        _ => 18,
    }
}

impl Question {
    /// A question about a stem with nothing known about the place.
    pub fn stem(stem: &str) -> Self {
        Self { stem: stem.to_owned(), place: Place::Unknown, ..Self::default() }
    }
}

/// The identifier characters immediately left of the caret: what has been typed of the word so far.
///
/// Empty when there are none, which is what the automatic trigger reads as "there is no word being
/// typed here" and what the manual one reports as an honest miss.
///
/// The characters are the grammar's own, which is the whole reason this asks rather than assumes: a
/// hyphen bounds a word in Rust and is inside one in CSS, so `--brand-hue` is one stem there and
/// three words here. And the **first** character has to be one a word may start with, so the caret
/// after `42` has no stem at all rather than a stem of `42` that matches nothing.
pub fn stem_at(text: &str, offset: usize, grammar: &Grammar) -> Range<usize> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return 0..0;
    }
    let mut start = offset;
    for (index, character) in text[..offset].char_indices().rev() {
        if !grammar.is_word_character(character, false) {
            break;
        }
        start = index;
    }
    while start < offset {
        let character = text[start..].chars().next().expect("start is inside the text");
        if grammar.is_word_character(character, true) {
            break;
        }
        start += character.len_utf8();
    }
    start..offset
}

/// The whole identifier the caret is inside: the stem, and whatever is still to the right of it.
///
/// What `Tab` replaces. `dra│wing` completed to `draw_frame` should not leave `wing` dangling
/// behind the caret, which is the reference editor's own reason for having two acceptance keys.
pub fn word_at(text: &str, offset: usize, grammar: &Grammar) -> Range<usize> {
    let stem = stem_at(text, offset, grammar);
    if offset > text.len() || !text.is_char_boundary(offset) {
        return stem;
    }
    let mut end = offset;
    for (index, character) in text[offset..].char_indices() {
        if !grammar.is_word_character(character, false) {
            break;
        }
        end = offset + index + character.len_utf8();
    }
    stem.start..end
}

/// Whether a stem could match a name at all, which is the cheap half of the match.
///
/// The one part of scoring a caller needs **before** it decides a candidate is worth building. The
/// window gathers from four thousand names in the project's index a keystroke, and turning each of
/// them into a [`Candidate`] — a string copy, a path's file name, a hash probe for its definition —
/// to have nearly all of them thrown out again is the difference between a keystroke that costs
/// nothing and one that allocates. Answering from two `&str`s costs one walk of each.
pub fn could_match(stem: &str, name: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let mut wanted = stem.chars().map(lower).peekable();
    for letter in name.chars().map(lower) {
        if wanted.peek() == Some(&letter) {
            wanted.next();
        }
    }
    wanted.peek().is_none()
}

/// Which rows a stem offers, best first.
///
/// The row **equal to the stem** is offered, first, as the reference editor offers it: a name typed in
/// full is still a question worth answering, and the evaluation counts it. What made `task-1678` drop
/// it was `Enter`, and the window keeps that safe instead: a list with nothing longer than the typed
/// word does not open, and `Enter` on the typed word is the new line it means.
///
/// An empty stem offers nothing: with nothing typed there is nothing being completed. [`rank_all`]
/// is the form for the places the language itself says what comes next.
pub fn rank(stem: &str, candidates: Vec<Candidate>) -> Vec<Row> {
    if stem.is_empty() {
        return Vec::new();
    }
    order(&Question::stem(stem), candidates, &|_| 0)
}

/// The same, except that an **empty** stem offers everything rather than nothing.
///
/// `task-1680`. [`rank`]'s guard is right for a word being typed and wrong for an import, where
/// `from '│'` and `use │` are positions at which the language itself says what comes next, and wrong
/// straight after a `.`, where the members of the value are the answer (`task-2231` §6.1).
pub fn rank_all(stem: &str, candidates: Vec<Candidate>) -> Vec<Row> {
    order(&Question::stem(stem), candidates, &|_| 0)
}

/// The rows a question offers, in the order of the weigher chain. `task-2231` §6.3.
///
/// A pure function of the question, the candidates and what was chosen before, so the popup, the
/// command line and the evaluation harness all get the same list. The weighers, compared in order:
///
/// 1. a server's preselected row, when the stem is its prefix, comes first outright;
/// 2. the match class ([`MatchClass`]);
/// 3. a capital typed first asks for a name that starts with one: `Co` is a type or a constant;
/// 4. a row a server said has the expected type;
/// 5. a name from a dependency the file does not import yet after every other, then a server's own
///    order, and every row a server offered before every row it did not;
/// 6. how well the row's kind fits the place ([`place_fit`]);
/// 7. locality ([`Locality`]);
/// 8. how often the name is already written in this file ([`Info::uses_here`]);
/// 9. how often the row was chosen before here (`chosen_before`, the window's selection statistics);
/// 10. a deprecated row last;
/// 11. the alignment score, the shorter name, the source, the bytes.
///
/// Three of these were moved while tuning against the evaluation's tune positions
/// (`tools/completion-eval/SCORECARD.md`). The case and the file's own words came up from the end of
/// the chain. The server's order came up from after the statistics: rust-analyzer's order alone
/// ranked the answer first more often than the chain did with it last, and tsserver's order is
/// groups (locals, then globals, then auto imports) inside which the rest of the chain still
/// decides.
///
/// Every part of the key is an integer or the name's bytes, so the order is total and the same on
/// every machine, which the popup's pictures and the command line's output rest on.
///
/// @param question - the stem and the place
/// @param candidates - every candidate the sources gathered
/// @param chosen_before - how many times a row of this name was chosen before for this question
pub fn order(
    question: &Question,
    candidates: Vec<Candidate>,
    chosen_before: &dyn Fn(&str) -> u32,
) -> Vec<Row> {
    let rows = matched(&question.stem, candidates);
    let first_upper = question.stem.chars().next().map(char::is_uppercase);
    let mut keyed: Vec<(OrderKey, usize)> = rows
        .iter()
        .enumerate()
        .map(|(at, row)| (key_of(question, row, first_upper, chosen_before(&row.name)), at))
        .collect();
    keyed.sort_by(|left, right| {
        left.0.cmp(&right.0).then(rows[left.1].name.as_bytes().cmp(rows[right.1].name.as_bytes()))
    });
    let mut taken: Vec<Option<Row>> = rows.into_iter().map(Some).collect();
    let mut chained: Vec<Row> = keyed.into_iter().filter_map(|(_, at)| taken[at].take()).collect();
    for (rank, row) in chained.iter_mut().enumerate() {
        row.info.chain_rank = rank as u32;
    }
    if question.language.is_empty() || crate::completion_model::ROOTS.is_empty() {
        return chained;
    }
    learned_order(question, chained)
}

/// The rows in the learned order: a server's preselected row when the stem is its prefix, then the
/// match class, then the model's score, then the name's bytes. `task-2237`.
///
/// The match class stays ahead of the score, because a person who has typed `lay` and sees `Layout`
/// below `replay` reads the list as broken whatever a model thinks. The chain's own order is one of
/// the features ([`features`]), so what the chain knew is not thrown away.
///
/// @param question - the stem, the place and the language
/// @param chained - the rows in the chain's order
fn learned_order(question: &Question, chained: Vec<Row>) -> Vec<Row> {
    let near = nearest_rows(&chained);
    // A row the model does not score keeps the chain's order, after every row it does score in its
    // class, which is what an unscored row's `f64::NEG_INFINITY` and the chain rank as the last tie
    // break give.
    let mut scored: Vec<(f64, usize, Row)> = chained
        .into_iter()
        .enumerate()
        .map(|(rank, mut row)| {
            let score = match rank < SCORED_BY_THE_CHAIN || near[rank] {
                true => crate::completion_model::score(&features(question, &row, rank)),
                false => f64::NEG_INFINITY,
            };
            row.info.learned = score.is_finite().then(|| (score * 1e6).round() as i64);
            (score, rank, row)
        })
        .collect();
    let preselected = |row: &Row| row.info.preselect && row.class <= MatchClass::Prefix;
    // A name equal to a short stem is in the prefix group rather than ahead of it: with `fi` typed, a
    // project function called `fi` is rarely the name being typed, and the model sees that nothing is
    // left to type and weighs it against everything else. From [`EXACT_FIRST_FROM`] letters a name
    // equal to the stem is the word typed in full and comes first, which keeps `Enter` a new line.
    let short = question.stem.chars().count() < EXACT_FIRST_FROM;
    let group = |row: &Row| match short {
        true => row.class.max(MatchClass::Prefix),
        false => row.class,
    };
    scored.sort_by(|(left_score, left_rank, left), (right_score, right_rank, right)| {
        (!preselected(left))
            .cmp(&!preselected(right))
            .then(group(left).cmp(&group(right)))
            .then(right_score.total_cmp(left_score))
            .then(left_rank.cmp(right_rank))
    });
    scored.into_iter().map(|(_, _, row)| row).collect()
}

/// From how many letters typed a name equal to the stem comes first in the learned order. `task-2237`.
pub const EXACT_FIRST_FROM: usize = 4;

/// How many of the chain's first rows the model scores. A TypeScript server offers fifteen thousand
/// names with one letter typed, nearly all of them auto imports, and the model costs a few
/// microseconds a row, so it scores the top of the chain and the names written nearest the caret, as
/// IntelliJ's model reorders only the top of its list. `task-2237`.
pub const SCORED_BY_THE_CHAIN: usize = 100;

/// How many of the rows written nearest the caret the model scores beside the chain's first rows.
pub const SCORED_BY_NEARNESS: usize = 100;

/// Which rows, by chain rank, are among the [`SCORED_BY_NEARNESS`] written nearest the caret: by the
/// nearer of the lines above and below, then by chain rank. A row written nowhere in the file is
/// never one of them.
///
/// @param chained - the rows in the chain's order
fn nearest_rows(chained: &[Row]) -> Vec<bool> {
    let mut near: Vec<(u32, usize)> = chained
        .iter()
        .enumerate()
        .map(|(rank, row)| (row.info.lines_above.min(row.info.lines_below), rank))
        .filter(|(lines, _)| *lines != u32::MAX)
        .collect();
    near.sort_unstable();
    let mut is_near = vec![false; chained.len()];
    for (_, rank) in near.into_iter().take(SCORED_BY_NEARNESS) {
        is_near[rank] = true;
    }
    is_near
}

/// A distance in lines with no place to measure it to, as the model reads it.
const FAR: f64 = 100_000.0;

/// What the model reads about a row, in the order of [`crate::completion_model::NAMES`]. The trainer,
/// `tools/completion-eval/rank-model/features.py`, works the same numbers out of
/// `editor complete --explain`, and `tests/completion_model.rs` checks the two agree.
///
/// @param question - the stem, the place and the language
/// @param row - the row
/// @param chain_rank - where the chain of weighers put the row, from 0
pub fn features(
    question: &Question,
    row: &Row,
    chain_rank: usize,
) -> [f64; crate::completion_model::FEATURES] {
    let info = &row.info;
    let stem_length = question.stem.chars().count();
    let case_agrees = match (question.stem.chars().next(), row.name.chars().next()) {
        (Some(typed), Some(first)) if typed.is_uppercase() && first.is_alphabetic() => {
            first.is_uppercase()
        }
        _ => true,
    };
    let kind =
        row.kind.and_then(|k| Kind::ALL.iter().position(|a| *a == k)).unwrap_or(Kind::ALL.len());
    let server_score = match info.server_order {
        Some(order) if order <= 0xFFFF_FFFF => match order > 0xFFFF {
            true => (0xFFFF_FFFF_i64 - order as i64 - 0x7FFF_FFFF) as f64,
            false => -(order as f64),
        },
        _ => -1000.0,
    };
    let language = match question.language.as_str() {
        "rust" => 0.0,
        "typescript" | "javascript" => 1.0,
        _ => 2.0,
    };
    let lines = |n: u32| if n == u32::MAX { FAR } else { f64::from(n) };
    let length = row.name.chars().count();
    let flag = |b: bool| if b { 1.0 } else { 0.0 };
    let bit = |source: Source| flag(info.offered_by & source.bit() != 0);
    [
        stem_length as f64,
        flag(case_agrees),
        flag(info.expected_type),
        flag(info.preselect),
        flag(info.deprecated),
        info.locality as u8 as f64,
        f64::from(place_fit(question.place, row.kind)),
        question.place as u8 as f64,
        kind as f64,
        row.source as u8 as f64,
        f64::from(info.offered_by.count_ones()),
        server_score,
        language,
        f64::from(info.uses_here),
        lines(info.lines_above),
        lines(info.lines_below),
        f64::from(info.uses_near),
        f64::from(info.same_before),
        f64::from(info.same_after),
        f64::from(info.words_nearby),
        f64::from(row.score),
        length as f64,
        length as f64 - stem_length as f64,
        chain_rank as f64,
        bit(Source::ThisFile),
        bit(Source::Word),
        bit(Source::OpenTab),
        bit(Source::Index),
        bit(Source::Language),
        bit(Source::Module),
        bit(Source::Member),
        bit(Source::Import),
        bit(Source::Server),
        f64::from(question.before),
        f64::from(question.after),
    ]
}

/// The parts of a row's place in the order, compared left to right. See [`order`]. Two tuples, because
/// a tuple compares as a whole only up to twelve parts.
type OrderKey = (
    (u8, MatchClass, u8, u8, u8, u64, u8, Locality, std::cmp::Reverse<u32>),
    (std::cmp::Reverse<u32>, u8, i32, usize, u8),
);

/// A row's place in the order.
///
/// @param question - the stem and the place
/// @param row - the row
/// @param first_upper - whether the stem's first letter is a capital, when there is one
/// @param chosen - how many times the row was chosen before here
fn key_of(question: &Question, row: &Row, first_upper: Option<bool>, chosen: u32) -> OrderKey {
    let preselected = row.info.preselect && row.class <= MatchClass::Prefix;
    // Only a capital is a signal. Lowercase is how most people type whatever they mean, `lay` for
    // `Layout` as often as for `layout`, so a lowercase first letter leaves the place to decide.
    let case_agrees = match (first_upper, row.name.chars().next()) {
        (Some(true), Some(first)) if first.is_alphabetic() => first.is_uppercase(),
        _ => true,
    };
    (
        (
            u8::from(!preselected),
            row.class,
            u8::from(!case_agrees),
            u8::from(!row.info.expected_type),
            u8::from(row.info.locality == Locality::Dependency),
            row.info.server_order.unwrap_or(u64::MAX),
            place_fit(question.place, row.kind),
            locality_here(question.place, row),
            std::cmp::Reverse(row.info.uses_here),
        ),
        (
            std::cmp::Reverse(chosen),
            u8::from(row.info.deprecated),
            -row.score,
            row.name.chars().count(),
            row.source.tie(),
        ),
    )
}

/// A row's locality as the order reads it. Where a statement or an expression starts, the language's
/// own keywords are as near as a local: `let`, `return` and `await` are written there more often than
/// any one project name, and left at [`Locality::Language`] a one letter stem buried them under the
/// project's names.
///
/// @param place - where the caret is
/// @param row - the row
fn locality_here(place: Place, row: &Row) -> Locality {
    let opens_a_statement = matches!(place, Place::Statement | Place::Expression | Place::Unknown);
    match (row.kind, opens_a_statement) {
        (Some(Kind::Keyword), true) => row.info.locality.min(Locality::Local),
        _ => row.info.locality,
    }
}

/// How well a kind fits a place, smaller better: 0 for what the place asks for, 1 for what it allows,
/// 2 for what it does not want. A row with no kind, a plain word, is always 1. `task-2231` §6.3.
///
/// @param place - where the caret is
/// @param kind - what the row names
pub fn place_fit(place: Place, kind: Option<Kind>) -> u8 {
    let Some(kind) = kind else { return 1 };
    match place {
        // A keyword where a type goes is a primitive or a type operator: `string`, `keyof`, `typeof`,
        // Rust's `dyn` and `impl`. tsserver and rust-analyzer both send the primitives as keywords.
        Place::Type => match kind {
            k if k.is_type() || k == Kind::Keyword => 0,
            Kind::Module => 1,
            _ => 2,
        },
        Place::Import => match kind {
            Kind::Module => 0,
            k if k.is_type() || k.is_callable() || k == Kind::Constant => 0,
            _ => 1,
        },
        Place::Member => match kind {
            Kind::Field | Kind::Method | Kind::Variant | Kind::Constant => 0,
            Kind::Keyword => 2,
            _ => 1,
        },
        Place::Pattern => match kind {
            Kind::Variant | Kind::Struct | Kind::Constant => 0,
            _ => 1,
        },
        Place::Statement => match kind {
            Kind::Variable | Kind::Parameter | Kind::Function | Kind::Keyword | Kind::Macro => 0,
            Kind::Method | Kind::Field => 2,
            _ => 1,
        },
        Place::Expression | Place::Argument => match kind {
            Kind::Method | Kind::Field => 2,
            k if k.is_value() => 0,
            _ => 1,
        },
        Place::Unknown => 1,
    }
}

impl Source {
    /// Where this source comes when everything else about two rows is equal: the nearest answer
    /// first, and the language's own words last, because a keyword is the one candidate a person can
    /// always type out from memory. A module wins inside an import, where `use a::b` with `b` both a
    /// module and a function far more often means the module.
    fn tie(self) -> u8 {
        match self {
            Source::Module => 0,
            Source::Member => 1,
            Source::Server => 2,
            Source::ThisFile => 3,
            Source::Kernel => 4,
            Source::Word => 5,
            Source::OpenTab => 6,
            Source::Index => 7,
            Source::Import => 8,
            Source::Language => 9,
        }
    }
}

/// Every candidate that matches the stem, one row a spelling, scored and classed, in the order they
/// were gathered. An empty stem matches everything, unscored.
///
/// One row per spelling is chosen as the pool is walked, by a table rather than by searching the
/// rows already kept: a stem of one letter on this repository's largest file offers well over two
/// thousand rows. The source that describes itself best keeps the row, and what it does not know
/// (a structural row's documentation under a server's row) is taken from the one it replaces.
///
/// @param stem - what has been typed
/// @param candidates - the pool
fn matched(stem: &str, candidates: Vec<Candidate>) -> Vec<Row> {
    let needle: Vec<char> = stem.chars().collect();
    let lowered: Vec<char> = needle.iter().flat_map(|c| c.to_lowercase()).collect();
    // A stem whose case folding changes its length is compared unfolded, because a subsequence of
    // characters is only meaningful while one character stays one character.
    let folded = (lowered.len() == needle.len()).then_some(lowered);
    let folded = folded.as_deref().unwrap_or(&needle);
    let mut scratch = Scratch::default();
    let mut seen: std::collections::HashMap<String, usize> =
        std::collections::HashMap::with_capacity(candidates.len());
    let mut rows: Vec<Row> = Vec::with_capacity(candidates.len());
    let mut kernel_order = 0u64;
    for mut candidate in candidates {
        // A short word that differs from the stem only in case completes nothing: `Eg` for `eg`, `S`
        // for `s`. From four letters a word of the other case is a real answer, `Layout` for
        // `layout`, and the Exact class. The row exactly equal to the stem is offered, as the
        // reference editor offers it; the window keeps `Enter` a new line on it.
        let short_case_variant = stem.chars().count() < 4
            && candidate.name != stem
            && candidate.name.eq_ignore_ascii_case(stem);
        // A row equal to the stem with no kind is the half typed word itself, which tsserver also sends
        // among the file's plain identifiers; a definition or a keyword of that name has a kind.
        let the_typed_word = !stem.is_empty() && candidate.name == stem && candidate.kind.is_none();
        if candidate.name.is_empty() || short_case_variant || the_typed_word {
            continue;
        }
        // A kernel's rows keep the kernel's own order when nothing else decides, as a server's do:
        // with nothing typed after `v.`, rust-analyzer and Jedi put the value's own members first.
        candidate.info.offered_by |= candidate.source.bit();
        if candidate.source == Source::Kernel && candidate.info.server_order.is_none() {
            candidate.info.server_order = Some(kernel_order);
            kernel_order += 1;
        }
        if let Some(at) = seen.get(&candidate.name) {
            merge_into(&mut rows[*at], candidate);
            continue;
        }
        let Some((class, score, matched)) =
            class_and_score(folded, &needle, &candidate, &mut scratch)
        else {
            continue;
        };
        seen.insert(candidate.name.clone(), rows.len());
        rows.push(Row {
            name: candidate.name,
            source: candidate.source,
            kind: candidate.kind,
            detail: candidate.detail,
            matched,
            score,
            class,
            info: candidate.info,
        });
    }
    rows
}

/// A second candidate of a spelling already offered: it takes the row when it describes itself better,
/// and either way what the row does not know is filled in from it.
///
/// @param row - the row already offered
/// @param candidate - the candidate of the same spelling
fn merge_into(row: &mut Row, candidate: Candidate) {
    let better = candidate.source.describes_itself() < row.source.describes_itself();
    let (mut keep, other) = match better {
        true => {
            let old = std::mem::replace(&mut row.info, candidate.info);
            row.source = candidate.source;
            row.kind = candidate.kind.or(row.kind);
            row.detail = candidate.detail;
            (std::mem::take(&mut row.info), old)
        }
        false => (std::mem::take(&mut row.info), candidate.info),
    };
    keep.doc = keep.doc.or(other.doc);
    keep.signature = keep.signature.or(other.signature);
    keep.container = keep.container.or(other.container);
    keep.locality = keep.locality.min(other.locality);
    keep.expected_type |= other.expected_type;
    keep.uses_here = keep.uses_here.max(other.uses_here);
    keep.lines_above = keep.lines_above.min(other.lines_above);
    keep.lines_below = keep.lines_below.min(other.lines_below);
    keep.uses_near = keep.uses_near.max(other.uses_near);
    keep.offered_by |= other.offered_by;
    keep.same_before = keep.same_before.max(other.same_before);
    keep.same_after = keep.same_after.max(other.same_after);
    keep.words_nearby = keep.words_nearby.max(other.words_nearby);
    keep.server_order = match (keep.server_order, other.server_order) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    row.info = keep;
}

/// The class, score and matched letters of one candidate, or nothing when the stem does not match.
/// Matched against a server's filter text when it has one, never against its label.
///
/// @param folded - the stem lowercased
/// @param typed - the stem as typed
/// @param candidate - the candidate
/// @param scratch - reused working space
fn class_and_score(
    folded: &[char],
    typed: &[char],
    candidate: &Candidate,
    scratch: &mut Scratch,
) -> Option<(MatchClass, i32, Vec<usize>)> {
    if typed.is_empty() {
        return Some((MatchClass::Prefix, 0, Vec::new()));
    }
    let against = candidate.info.filter.as_deref().unwrap_or(&candidate.name);
    let found = score(folded, typed, against, scratch)?;
    let class = match_class(folded, &scratch.letters, &scratch.lowered);
    // The letters to pick out are the name's, which is what is drawn; a filter text that is not the
    // name marks nothing rather than marking the wrong letters.
    let matched = match against == candidate.name {
        true => found.matched,
        false => {
            score(folded, typed, &candidate.name, scratch).map(|s| s.matched).unwrap_or_default()
        }
    };
    Some((class, found.score, matched))
}

/// The class of a match, from the stem and the name's letters (as typed and lowercased).
///
/// @param stem - the stem lowercased
/// @param letters - the name's letters
/// @param lowered - the name's letters lowercased
fn match_class(stem: &[char], letters: &[char], lowered: &[char]) -> MatchClass {
    if lowered == stem {
        return MatchClass::Exact;
    }
    if lowered.starts_with(stem) {
        return MatchClass::Prefix;
    }
    let starts: Vec<bool> = (0..letters.len()).map(|at| is_boundary(letters, at)).collect();
    if humps(stem, lowered, &starts, 0, 0, false) {
        return MatchClass::Humps;
    }
    let later_word = (1..lowered.len()).any(|at| starts[at] && lowered[at..].starts_with(stem));
    match later_word {
        true => MatchClass::WordStart,
        false => MatchClass::Subsequence,
    }
}

/// Whether the rest of a stem lines up with the rest of a name with every letter starting a word or
/// following a matched letter, the first at the name's first letter. `NPE` and `NuPoEx` both reach
/// `NullPointerException`; `lsm` reaches `layout_scroll_margin`.
///
/// Identifiers are short and the recursion stops at the first reading that works, so this is a few
/// steps for the names a stem can match at all.
///
/// @param stem - the stem lowercased
/// @param name - the name lowercased
/// @param starts - which of the name's letters start a word
/// @param i - how much of the stem is matched
/// @param j - where in the name to carry on
/// @param running - whether the letter before `j` was matched
fn humps(stem: &[char], name: &[char], starts: &[bool], i: usize, j: usize, running: bool) -> bool {
    if i == stem.len() {
        return true;
    }
    if j >= name.len() {
        return false;
    }
    if i == 0 && j == 0 {
        return name[0] == stem[0] && humps(stem, name, starts, 1, 1, true);
    }
    if running && name[j] == stem[i] && humps(stem, name, starts, i + 1, j + 1, true) {
        return true;
    }
    // Skip to the next word start that holds the next letter.
    (j..name.len())
        .filter(|&at| starts[at] && name[at] == stem[i])
        .any(|at| humps(stem, name, starts, i + 1, at + 1, true))
}

/// Working space reused across a whole pool, so scoring a project's worth of names allocates once.
///
/// The candidate's characters, their folded copies and the alignment table are all per-candidate
/// scratch, and building three vectors for each of a few thousand names was the largest single cost
/// of a keystroke when this was measured on Unluminous's own biggest file. Kept and cleared instead: the
/// answer is identical and nothing is allocated after the first candidate.
#[derive(Default)]
struct Scratch {
    letters: Vec<char>,
    lowered: Vec<char>,
    table: Vec<i32>,
}

/// What one candidate scored, and where the stem landed in it.
struct Scored {
    score: i32,
    matched: Vec<usize>,
}

/// Score one candidate against one stem, or nothing when the stem is not a subsequence of it.
///
/// `folded` is the stem's letters lowercased and `typed` is them as they were typed; both are
/// needed, because the match is case-insensitive and one of the bonuses is not.
fn score(folded: &[char], typed: &[char], name: &str, scratch: &mut Scratch) -> Option<Scored> {
    scratch.letters.clear();
    scratch.letters.extend(name.chars());
    scratch.lowered.clear();
    for at in 0..scratch.letters.len() {
        let folded_letter = lower(scratch.letters[at]);
        scratch.lowered.push(folded_letter);
    }
    if !is_subsequence(folded, &scratch.lowered) {
        return None;
    }
    let alignment = align(folded, typed, scratch)?;
    let mut score = alignment.score;
    // Starting with the stem is the commonest intent by far, and it is a property of the whole
    // candidate rather than of any one letter, so it is added once here.
    if scratch.lowered.len() >= folded.len() && scratch.lowered[..folded.len()] == *folded {
        score += PREFIX;
    }
    score -= UNMATCHED * (scratch.letters.len().saturating_sub(folded.len())) as i32;
    Some(Scored { score, matched: alignment.matched })
}

/// Whether `needle` appears in `haystack` in order, both already folded. The cheap reject: nearly
/// every candidate in a project fails here, and it costs one walk of two short strings.
fn is_subsequence(needle: &[char], haystack: &[char]) -> bool {
    let mut at = 0;
    for letter in haystack {
        if at < needle.len() && needle[at] == *letter {
            at += 1;
        }
    }
    at == needle.len()
}

/// The best alignment of a stem inside a name, and what it scored.
struct Alignment {
    score: i32,
    matched: Vec<usize>,
}

/// Find the **best** alignment rather than the first one.
///
/// `pt` lines up with `paint_text` two ways — the `t` of `paint` or the `t` of `text` — and only
/// the second sits on a word boundary, which is the one a person meant. Sublime finds this by
/// bounded recursion; the same answer comes out of filling a small table, which is what this does,
/// and a table cannot run out of recursion budget half way through a long name and silently return
/// the worse reading.
///
/// The table is `stem × name × whether the letter before was matched`, because the consecutive
/// bonus is the one thing a cell's value depends on outside itself. Identifiers are short: a three
/// letter stem in a twelve letter name is seventy-two cells.
fn align(folded: &[char], typed: &[char], scratch: &mut Scratch) -> Option<Alignment> {
    let letters = &scratch.letters;
    let lowered = &scratch.lowered;
    let stem = folded.len();
    let name = letters.len();
    // `best[(i * (name + 1) + j) * 2 + run]` is the best total for stem[i..] inside name[j..].
    let width = (name + 1) * 2;
    let best = &mut scratch.table;
    best.clear();
    best.resize((stem + 1) * width, IMPOSSIBLE);
    let cell = |i: usize, j: usize, run: bool| (i * width) + j * 2 + usize::from(run);
    for j in 0..=name {
        best[cell(stem, j, false)] = 0;
        best[cell(stem, j, true)] = 0;
    }
    for i in (0..stem).rev() {
        for j in (0..name).rev() {
            for run in [false, true] {
                // Step over this letter of the name. The run of consecutive matches ends here.
                let mut value = best[cell(i, j + 1, false)];
                if folded[i] == lowered[j] {
                    let mut bonus = 0;
                    if is_boundary(letters, j) {
                        bonus += BOUNDARY;
                    }
                    if run {
                        bonus += CONSECUTIVE;
                    }
                    if typed[i] == letters[j] {
                        bonus += SAME_CASE;
                    }
                    let rest = best[cell(i + 1, j + 1, true)];
                    if rest > IMPOSSIBLE {
                        value = value.max(bonus + rest);
                    }
                }
                best[cell(i, j, run)] = value;
            }
        }
    }
    let total = best[cell(0, 0, false)];
    if total <= IMPOSSIBLE {
        return None;
    }
    // Walk the table back out to say which letters the best reading used. Taking the match whenever
    // it is as good as stepping over settles a tie towards the earlier letter, which is what makes
    // the picked-out letters the same on every run.
    let mut matched = Vec::with_capacity(stem);
    let (mut i, mut j, mut run) = (0, 0, false);
    while i < stem && j < name {
        let step = best[cell(i, j + 1, false)];
        let mut took = false;
        if folded[i] == lowered[j] {
            let mut bonus = 0;
            if is_boundary(letters, j) {
                bonus += BOUNDARY;
            }
            if run {
                bonus += CONSECUTIVE;
            }
            if typed[i] == letters[j] {
                bonus += SAME_CASE;
            }
            let rest = best[cell(i + 1, j + 1, true)];
            took = rest > IMPOSSIBLE && bonus + rest >= step;
        }
        if took {
            matched.push(j);
            i += 1;
            j += 1;
            run = true;
        } else {
            j += 1;
            run = false;
        }
    }
    Some(Alignment { score: total, matched })
}

/// Whether the letter at `at` starts the name or a part of it.
///
/// The start, anything after a character that is not a letter or a digit — `_`, `-`, `@`, `$`, the
/// three separators the languages Unluminous reads actually use — and a lower-to-upper camel step. Not
/// asked of the grammar, deliberately: a hyphen is *inside* a CSS word, which is exactly why it is
/// a boundary within one.
fn is_boundary(letters: &[char], at: usize) -> bool {
    if at == 0 {
        return true;
    }
    let before = letters[at - 1];
    if !before.is_alphanumeric() {
        return true;
    }
    before.is_lowercase() && letters[at].is_uppercase()
}

/// One character folded for comparison. `char::to_lowercase` gives an iterator because a few
/// characters fold to more than one, and a subsequence match needs one character to stay one
/// character, so the first is taken and the rest — which no identifier in practice reaches — are
/// left alone.
fn lower(character: char) -> char {
    character.to_lowercase().next().unwrap_or(character)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_scores_a_row_exactly_as_the_trainer_does() {
        // Written by `tools/completion-eval/rank-model/export.py`: the trainer's score, then the row's
        // features, a row a line. `task-2237`.
        let checks = include_str!("completion_model_checks.txt");
        let mut read = 0;
        for line in checks.lines().filter(|l| !l.trim().is_empty()) {
            let numbers: Vec<f64> = line.split(' ').map(|v| v.parse().unwrap()).collect();
            let x: [f64; crate::completion_model::FEATURES] = numbers[1..].try_into().unwrap();
            let score = crate::completion_model::score(&x);
            assert!((score - numbers[0]).abs() < 1e-9, "{score} against {} for {line}", numbers[0]);
            read += 1;
        }
        assert!(read > 0 || crate::completion_model::ROOTS.is_empty());
    }

    #[test]
    fn with_nothing_typed_a_kernels_rows_keep_the_kernels_order() {
        let pool = vec![
            Candidate::new("into_raw_parts", Source::Kernel),
            Candidate::new("len", Source::Kernel),
            Candidate::new("eq", Source::Kernel),
        ];
        let names: Vec<String> = rank_all("", pool).into_iter().map(|row| row.name).collect();
        assert_eq!(names, vec!["into_raw_parts", "len", "eq"]);
    }

    /// Rust as the bundled plugin describes it, cut down to what these tests need.
    fn rust() -> Grammar {
        let words = |list: &str| list.split(' ').map(str::to_owned).collect::<Vec<String>>();
        Grammar {
            language: "Rust".to_owned(),
            keywords: words("fn let mut const struct impl pub use match return"),
            builtins: words("String Vec Option Some None usize"),
            line_comment: Some("//".to_owned()),
            strings: vec!['"'],
            escapes: true,
            operators: "+-*/%=<>!&|^?:;,.#".chars().collect(),
            numbers: true,
            ..Grammar::default()
        }
    }

    #[test]
    fn an_empty_stem_offers_nothing_to_rank_and_everything_to_rank_all() {
        // `task-1680` §5.2. The guard is right for a word being typed and wrong inside an import,
        // where `from '|'` is a position at which the language itself says what comes next.
        let pool = || {
            vec![
                Candidate::described("./layout", Source::Module, Some(Kind::Module), "src"),
                Candidate::described("./caret", Source::Module, Some(Kind::Module), "src"),
                Candidate::new("draw", Source::Word),
            ]
        };
        assert!(rank("", pool()).is_empty(), "nothing is being completed");
        let all: Vec<String> = rank_all("", pool()).into_iter().map(|row| row.name).collect();
        assert_eq!(
            all,
            vec!["./caret".to_owned(), "./layout".to_owned(), "draw".to_owned()],
            "by source, then by the shorter name: a module comes before a word"
        );
    }

    #[test]
    fn a_module_wins_a_tie_with_a_name_spelt_the_same() {
        // The one place `Source::Module`'s order is read: `use a::b` with `b` both a module and a
        // function far more often means the module.
        let rows = rank(
            "part",
            vec![
                Candidate::described("parts", Source::Index, Some(Kind::Function), "a.rs"),
                Candidate::described("parts", Source::Module, Some(Kind::Module), "a/"),
            ],
        );
        assert_eq!(rows.len(), 1, "two entries that would type the same bytes are one offer");
        assert_eq!(rows[0].source, Source::Module);
        assert_eq!(rows[0].detail, "a/");
    }

    /// CSS, where a hyphen is a letter.
    fn css() -> Grammar {
        Grammar {
            language: "CSS".to_owned(),
            keywords: vec!["@media".to_owned()],
            builtins: vec!["background-color".to_owned()],
            operators: "{}();:,".chars().collect(),
            numbers: true,
            word_characters: vec!['-', '@'],
            ..Grammar::default()
        }
    }

    /// The names a stem offers, in order, out of a list of plain words.
    fn offered(stem: &str, names: &[&str]) -> Vec<String> {
        let pool = names.iter().map(|name| Candidate::new(*name, Source::Word)).collect();
        rank(stem, pool).into_iter().map(|row| row.name).collect()
    }

    #[test]
    fn a_prefix_wins_and_the_shorter_of_two_prefixes_wins_before_a_middle_match() {
        // Scenario 1.
        assert_eq!(
            offered("dra", &["draw_frame", "redraw", "draw"]),
            ["draw", "draw_frame", "redraw"]
        );
    }

    #[test]
    fn a_letter_on_a_word_boundary_is_worth_more_than_one_in_the_middle() {
        // Scenario 2: `pt` prefers `paint_text`, which needs the **best** alignment rather than the
        // first — the `t` of `paint` lines up too, and it is not on a boundary.
        assert_eq!(offered("pt", &["pointer", "paint_text"]), ["paint_text", "pointer"]);
        // The same at a camel step, which is the other kind of boundary.
        assert_eq!(offered("pt", &["pointer", "paintText"]), ["paintText", "pointer"]);
    }

    #[test]
    fn the_match_is_case_insensitive_and_the_case_bonus_never_excludes() {
        // Scenario 3.
        assert_eq!(offered("LYT", &["layout"]), ["layout"]);
        assert_eq!(offered("lyt", &["layout"]), ["layout"]);
        // Typed as it is spelled, the same name still wins against one spelled differently.
        assert_eq!(offered("lay", &["Layout", "layout"]), ["layout", "Layout"]);
    }

    #[test]
    fn the_row_equal_to_the_stem_is_offered_first() {
        // Scenario 4, changed by `task-2231`: the typed word is offered, as the reference editor offers
        // it, and the window keeps `Enter` a new line on it.
        let defined = |names: &[&str]| -> Vec<String> {
            let pool = names
                .iter()
                .map(|name| Candidate::described(*name, Source::ThisFile, Some(Kind::Function), ""))
                .collect();
            rank("draw", pool).into_iter().map(|row| row.name).collect()
        };
        assert_eq!(defined(&["draw", "draw_frame", "redraw"]), ["draw", "draw_frame", "redraw"]);
        assert_eq!(defined(&["draw"]), ["draw"]);
    }

    #[test]
    fn the_half_typed_word_with_no_kind_is_not_offered() {
        let pool = vec![
            Candidate::new("st", Source::Server),
            Candidate::described("stamp", Source::Server, Some(Kind::Function), ""),
        ];
        assert_eq!(ordered(&Question::stem("st"), pool), ["stamp"]);
    }

    #[test]
    fn a_short_word_that_differs_only_in_case_is_not_offered() {
        assert_eq!(offered("eg", &["Eg", "egui"]), ["egui"]);
        assert_eq!(offered("layout", &["Layout", "layout_scroll"]), ["Layout", "layout_scroll"]);
    }

    #[test]
    fn a_stem_that_matches_nothing_offers_nothing_rather_than_everything() {
        // Scenario 5.
        assert!(offered("zzz", &["draw", "layout", "paint_text"]).is_empty());
        assert!(offered("", &["draw"]).is_empty(), "and an empty stem asks nothing at all");
    }

    #[test]
    fn two_sources_offering_one_spelling_are_one_row_labelled_from_the_better_one() {
        // Scenario 6. A definition beats a keyword beats a plain word.
        let pool = vec![
            Candidate::new("let", Source::Word),
            Candidate::described("let", Source::Language, None, "keyword"),
        ];
        let rows = rank("le", pool);
        assert_eq!(rows.len(), 1, "one row, because both would type the same bytes: {rows:?}");
        assert_eq!(rows[0].source, Source::Language);
        assert_eq!(rows[0].detail, "keyword");

        let pool = vec![
            Candidate::described("draw", Source::Language, None, "keyword"),
            Candidate::described("draw", Source::ThisFile, Some(Kind::Function), "layout.rs"),
            Candidate::new("draw", Source::Word),
        ];
        let rows = rank("dr", pool);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, Source::ThisFile, "a definition has the most to say");
        assert_eq!(rows[0].kind, Some(Kind::Function));
    }

    #[test]
    fn equally_scored_rows_are_offered_nearest_source_first() {
        // Scenario 7. One name each, so the scores are as close as they can be made and only the
        // source decides.
        let pool = vec![
            Candidate::described("drawc", Source::Language, None, "keyword"),
            Candidate::described("drawb", Source::Index, Some(Kind::Function), "far.rs"),
            Candidate::new("drawd", Source::Word),
            Candidate::described("drawa", Source::ThisFile, Some(Kind::Function), "here.rs"),
        ];
        let order: Vec<Source> = rank("draw", pool).into_iter().map(|row| row.source).collect();
        assert_eq!(
            order,
            [Source::ThisFile, Source::Word, Source::Index, Source::Language],
            "this file's definitions, then its words, then the project's, then the language's"
        );
    }

    #[test]
    fn letters_wider_than_one_byte_match_and_land_on_character_positions() {
        // Scenario 8. The positions are what picks letters out of a drawn name, and a byte position
        // would be a different number after an accent.
        let rows = rank("dj", vec![Candidate::new("d\u{00E9}j\u{00E0}", Source::Word)]);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].matched, vec![0, 2], "the d and the j, counted in characters");
        // And a script with no case at all is matched the same way.
        let rows = rank("\u{6771}", vec![Candidate::new("\u{6771}\u{4EAC}", Source::Word)]);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].matched, vec![0]);
    }

    #[test]
    fn a_css_custom_property_is_one_word_and_its_hyphens_are_boundaries() {
        // Scenario 9.
        let text = "--brand-hue: 280;\n.card { color: var(--br";
        let stem = stem_at(text, text.len(), &css());
        assert_eq!(&text[stem.clone()], "--br", "the hyphens are word characters here");
        let pool = vec![
            Candidate::new("--brand-hue", Source::Word),
            Candidate::new("border-radius", Source::Word),
        ];
        let names: Vec<String> = rank(&text[stem], pool).into_iter().map(|row| row.name).collect();
        assert_eq!(names, ["--brand-hue"], "`border-radius` has no `--` in it at all");
    }

    #[test]
    fn scoring_the_same_stem_against_the_same_pool_twice_gives_the_same_list() {
        // Scenario 10, and the determinism property.
        let pool = || {
            vec![
                Candidate::new("draw", Source::Word),
                Candidate::new("draw_frame", Source::Word),
                Candidate::described("draw_all", Source::Index, Some(Kind::Function), "a.rs"),
                Candidate::described("redraw", Source::OpenTab, Some(Kind::Function), "b.rs"),
                Candidate::described("drop", Source::Language, None, "keyword"),
            ]
        };
        assert_eq!(rank("dr", pool()), rank("dr", pool()));
        assert_eq!(rank("dra", pool()), rank("dra", pool()));
    }

    #[test]
    fn the_stem_is_what_has_been_typed_of_the_word_and_nothing_to_the_right_of_it() {
        let text = "let value = dra";
        assert_eq!(&text[stem_at(text, text.len(), &rust())], "dra");
        // Mid-word: the stem is the left half and the word is the whole of it.
        let text = "drawing";
        assert_eq!(&text[stem_at(text, 3, &rust())], "dra");
        assert_eq!(&text[word_at(text, 3, &rust())], "drawing");
        // With nothing to the right, the two are the same thing.
        assert_eq!(stem_at(text, 7, &rust()), word_at(text, 7, &rust()));
    }

    #[test]
    fn a_point_that_is_not_on_a_word_has_no_stem_at_all() {
        // Which is what `Ctrl+Space` reports as an honest miss rather than opening an empty list.
        let text = "let value = 42;";
        assert!(stem_at(text, 0, &rust()).is_empty(), "the very start of the file");
        assert!(stem_at(text, 4, &rust()).is_empty(), "just after a space");
        assert!(stem_at(text, 14, &rust()).is_empty(), "just after a number");
        assert!(stem_at(text, text.len(), &rust()).is_empty(), "just after a semicolon");
        assert!(stem_at("", 0, &rust()).is_empty());
        // A digit cannot start a word, so `x2` is a stem and the `2` of `42` is not.
        assert_eq!(&"let x2"[stem_at("let x2", 6, &rust())], "x2");
    }

    #[test]
    fn an_offset_that_is_not_a_character_boundary_answers_nothing_rather_than_panicking() {
        let text = "d\u{00E9}j\u{00E0}";
        assert!(stem_at(text, 2, &rust()).is_empty(), "inside the é");
        assert!(stem_at(text, 99, &rust()).is_empty(), "past the end");
        assert!(word_at(text, 99, &rust()).is_empty());
    }

    #[test]
    fn the_cheap_reject_agrees_with_the_scorer_about_what_matches() {
        // The window uses it to decide which of four thousand names are worth building a candidate
        // out of, so a name it lets through that the scorer then drops is waste, and one it drops
        // that the scorer would have offered is a missing row.
        let names = [
            "draw",
            "draw_frame",
            "redraw",
            "layout",
            "paint_text",
            "--brand-hue",
            "d\u{00E9}j\u{00E0}",
            "x",
        ];
        for stem in ["d", "dr", "dra", "lyt", "pt", "--br", "zz", "drawn", "\u{00E9}"] {
            for name in names {
                let offered = !rank(stem, vec![Candidate::new(name, Source::Word)]).is_empty();
                let cheap = could_match(stem, name) && name != stem;
                assert_eq!(offered, cheap, "{stem} against {name}");
            }
        }
        assert!(!could_match("", "draw"), "an empty stem asks nothing");
    }

    #[test]
    fn a_row_says_which_of_its_letters_were_matched() {
        let rows = rank("ptx", vec![Candidate::new("paint_text", Source::Word)]);
        assert_eq!(rows.len(), 1, "{rows:?}");
        let name: Vec<char> = "paint_text".chars().collect();
        let picked: String = rows[0].matched.iter().map(|at| name[*at]).collect();
        assert_eq!(picked, "ptx", "the picked out letters spell what was typed");
        assert_eq!(rows[0].matched, vec![0, 6, 8], "on the boundaries, which is the best reading");
    }

    /// **Truthfulness**: every row really is a case-insensitive subsequence match, its matched
    /// positions are inside the name, in order, and spell the stem back.
    #[test]
    fn every_row_offered_is_one_the_stem_really_matches() {
        for stem in ["d", "dr", "dra", "LYT", "pt", "--br", "\u{00E9}"] {
            for row in rank(stem, a_pool()) {
                let letters: Vec<char> = row.name.chars().collect();
                assert_eq!(row.matched.len(), stem.chars().count(), "{stem} against {}", row.name);
                let mut last = None;
                for at in &row.matched {
                    assert!(*at < letters.len(), "{at} is outside {}", row.name);
                    assert!(last.is_none_or(|before| before < *at), "in order");
                    last = Some(*at);
                }
                let picked: String = row.matched.iter().map(|at| lower(letters[*at])).collect();
                let wanted: String = stem.chars().map(lower).collect();
                assert_eq!(picked, wanted, "{stem} against {}", row.name);
            }
        }
    }

    /// **Determinism**: same stem, same pool, same list — including the picked out letters.
    #[test]
    fn the_same_question_always_gets_the_same_answer() {
        for stem in ["d", "dr", "dra", "aw", "e", "n"] {
            assert_eq!(rank(stem, a_pool()), rank(stem, a_pool()), "{stem}");
        }
    }

    /// **Isolation**: nothing here reads or writes anything. There is no filesystem call to test
    /// for, so what is exercised is that every entry point answers from a `&str`, a `Grammar` and a
    /// list of names, with no path in sight.
    #[test]
    fn every_answer_comes_from_the_text_the_grammar_and_the_pool_alone() {
        let text = "fn draw() { let dra";
        let grammar = rust();
        let stem = stem_at(text, text.len(), &grammar);
        assert_eq!(&text[stem.clone()], "dra");
        assert_eq!(word_at(text, text.len(), &grammar), stem);
        assert!(!rank(&text[stem], a_pool()).is_empty());
    }

    /// Every shape of candidate worth holding the properties against.
    fn a_pool() -> Vec<Candidate> {
        vec![
            Candidate::new("draw", Source::Word),
            Candidate::new("draw_frame", Source::Word),
            Candidate::new("redraw", Source::Word),
            Candidate::new("d", Source::Word),
            Candidate::new("d\u{00E9}j\u{00E0}", Source::Word),
            Candidate::new("--brand-hue", Source::Word),
            Candidate::new("paintText", Source::Word),
            Candidate::described("layout", Source::ThisFile, Some(Kind::Type), "layout.rs"),
            Candidate::described("new", Source::Index, Some(Kind::Function), "caret.rs"),
            Candidate::described("let", Source::Language, None, "keyword"),
            Candidate::new(String::new(), Source::Word),
        ]
    }

    /// The names a question offers, in order.
    fn ordered(question: &Question, pool: Vec<Candidate>) -> Vec<String> {
        order(question, pool, &|_| 0).into_iter().map(|row| row.name).collect()
    }

    #[test]
    fn the_match_classes_are_told_apart() {
        let class = |stem: &str, name: &str| {
            rank(stem, vec![Candidate::new(name, Source::Word)]).first().map(|row| row.class)
        };
        assert_eq!(class("layout", "Layout"), Some(MatchClass::Exact));
        assert_eq!(class("lay", "layout_scroll"), Some(MatchClass::Prefix));
        assert_eq!(class("lsm", "layout_scroll_margin"), Some(MatchClass::Humps));
        assert_eq!(class("NPE", "NullPointerException"), Some(MatchClass::Humps));
        assert_eq!(class("NuPoEx", "NullPointerException"), Some(MatchClass::Humps));
        assert_eq!(class("scroll", "layout_scroll"), Some(MatchClass::WordStart));
        assert_eq!(class("rects", "selectionRectsIn"), Some(MatchClass::WordStart));
        assert_eq!(class("lyt", "layout"), Some(MatchClass::Subsequence));
    }

    #[test]
    fn weigher_one_the_match_class_comes_before_the_alignment_score() {
        // `scroll` starts a later word of the second and is only a subsequence of the third.
        let pool = vec![
            Candidate::new("sXcXrXoXlXl", Source::ThisFile),
            Candidate::new("layout_scroll", Source::Index),
            Candidate::new("scroller", Source::Index),
        ];
        assert_eq!(
            ordered(&Question::stem("scroll"), pool),
            ["scroller", "layout_scroll", "sXcXrXoXlXl"]
        );
        // A hump match beats a subsequence even when the subsequence is shorter.
        let pool = vec![
            Candidate::new("lasm", Source::Word),
            Candidate::new("layout_scroll_margin", Source::Word),
        ];
        assert_eq!(ordered(&Question::stem("lsm"), pool)[0], "layout_scroll_margin");
    }

    #[test]
    fn weigher_two_a_row_of_the_expected_type_comes_first_among_equals() {
        let mut typed = Candidate::described("draw_line", Source::Server, Some(Kind::Function), "");
        typed.info.expected_type = true;
        let pool =
            vec![Candidate::described("draw", Source::ThisFile, Some(Kind::Function), ""), typed];
        assert_eq!(ordered(&Question::stem("dr"), pool), ["draw_line", "draw"]);
    }

    #[test]
    fn a_capital_typed_first_asks_for_a_name_that_starts_with_one() {
        let pool = || {
            vec![
                Candidate::described("command", Source::ThisFile, Some(Kind::Function), ""),
                Candidate::described("Command", Source::ThisFile, Some(Kind::Struct), ""),
            ]
        };
        let at = |stem: &str| Question {
            stem: stem.to_owned(),
            place: Place::Statement,
            ..Default::default()
        };
        assert_eq!(ordered(&at("Co"), pool()), ["Command", "command"]);
        // Lowercase leaves it to the place, where a statement wants the function.
        assert_eq!(ordered(&at("co"), pool()), ["command", "Command"]);
    }

    #[test]
    fn a_name_from_a_dependency_comes_after_the_projects_own_whatever_the_server_order() {
        let mut dependency =
            Candidate::described("TextBuffer", Source::Server, Some(Kind::Struct), "");
        dependency.info.server_order = Some(1);
        dependency.info.locality = Locality::Dependency;
        let mut own = Candidate::described("TextRenderer", Source::Server, Some(Kind::Struct), "");
        own.info.server_order = Some(2);
        own.info.locality = Locality::NeedsImport;
        assert_eq!(
            ordered(&Question::stem("Text"), vec![dependency, own]),
            ["TextRenderer", "TextBuffer"]
        );
    }

    #[test]
    fn a_keyword_where_a_statement_starts_is_as_near_as_a_local() {
        let pool = vec![
            Candidate::described("letters", Source::Index, Some(Kind::Function), ""),
            Candidate::described("let", Source::Language, Some(Kind::Keyword), ""),
        ];
        let question =
            Question { stem: "le".to_owned(), place: Place::Statement, ..Default::default() };
        assert_eq!(ordered(&question, pool), ["let", "letters"]);
    }

    #[test]
    fn a_server_row_comes_before_a_row_no_server_offered() {
        let mut served =
            Candidate::described("draw_line", Source::Server, Some(Kind::Function), "");
        served.info.server_order = Some(5);
        let pool =
            vec![Candidate::described("draw", Source::ThisFile, Some(Kind::Function), ""), served];
        assert_eq!(ordered(&Question::stem("dr"), pool), ["draw_line", "draw"]);
    }

    #[test]
    fn a_name_already_written_in_this_file_wins_a_tie() {
        let mut push = Candidate::described("push", Source::Server, Some(Kind::Method), "");
        push.info.uses_here = 3;
        let pop = Candidate::described("pop", Source::Server, Some(Kind::Method), "");
        let question =
            Question { stem: "p".to_owned(), place: Place::Member, ..Default::default() };
        assert_eq!(ordered(&question, vec![pop, push]), ["push", "pop"]);
    }

    #[test]
    fn weigher_three_the_place_decides_between_a_type_and_a_value() {
        let pool = || {
            vec![
                Candidate::described("layout", Source::ThisFile, Some(Kind::Variable), ""),
                Candidate::described("Layout", Source::Index, Some(Kind::Struct), ""),
            ]
        };
        let at = |place| Question { stem: "lay".to_owned(), place, ..Default::default() };
        assert_eq!(ordered(&at(Place::Type), pool()), ["Layout", "layout"]);
        assert_eq!(ordered(&at(Place::Expression), pool()), ["layout", "Layout"]);
    }

    #[test]
    fn weigher_four_a_local_beats_this_file_beats_the_project_beats_an_import() {
        let pool = vec![
            Candidate::described("drawc", Source::Import, Some(Kind::Function), ""),
            Candidate::described("drawb", Source::Index, Some(Kind::Function), ""),
            Candidate::described("drawd", Source::ThisFile, Some(Kind::Function), ""),
            Candidate::described("drawa", Source::Word, Some(Kind::Variable), "")
                .at(Locality::Local),
        ];
        assert_eq!(ordered(&Question::stem("draw"), pool), ["drawa", "drawd", "drawb", "drawc"]);
    }

    #[test]
    fn weigher_five_a_row_chosen_before_comes_first_among_equals() {
        let pool =
            || vec![Candidate::new("drawb", Source::Index), Candidate::new("drawa", Source::Index)];
        let names: Vec<String> =
            order(&Question::stem("draw"), pool(), &|name| u32::from(name == "drawb"))
                .into_iter()
                .map(|row| row.name)
                .collect();
        assert_eq!(names, ["drawb", "drawa"]);
        assert_eq!(ordered(&Question::stem("draw"), pool()), ["drawa", "drawb"]);
    }

    #[test]
    fn weigher_six_a_servers_own_order_decides_among_its_equals() {
        let row = |name: &str, at: u64| {
            let mut c = Candidate::described(name, Source::Server, Some(Kind::Method), "");
            c.info.server_order = Some(at);
            c
        };
        let pool = vec![row("aaa_b", 2), row("aaa_c", 0), row("aaa_a", 1)];
        assert_eq!(ordered(&Question::stem("aaa"), pool), ["aaa_c", "aaa_a", "aaa_b"]);
    }

    #[test]
    fn weigher_seven_a_deprecated_row_comes_last_in_its_class() {
        let mut old = Candidate::new("drawa", Source::Server);
        old.info.deprecated = true;
        let pool = vec![old, Candidate::new("drawb", Source::Server)];
        assert_eq!(ordered(&Question::stem("draw"), pool), ["drawb", "drawa"]);
    }

    #[test]
    fn weigher_eight_the_first_letters_case_then_the_score_then_the_length() {
        let pool =
            vec![Candidate::new("layout", Source::Word), Candidate::new("Layout", Source::Word)];
        assert_eq!(ordered(&Question::stem("La"), pool.clone()), ["Layout", "layout"]);
        assert_eq!(ordered(&Question::stem("la"), pool), ["layout", "Layout"]);
    }

    #[test]
    fn a_preselected_server_row_wins_outright_when_the_stem_is_its_prefix() {
        let mut chosen = Candidate::new("draw_everything", Source::Server);
        chosen.info.preselect = true;
        let pool = vec![Candidate::new("draw", Source::ThisFile).at(Locality::Local), chosen];
        assert_eq!(ordered(&Question::stem("dr"), pool)[0], "draw_everything");
    }

    #[test]
    fn a_server_row_is_matched_on_its_filter_text_never_its_label() {
        let mut row = Candidate::new("draw(…)", Source::Server);
        row.info.filter = Some("draw".to_owned());
        assert_eq!(ordered(&Question::stem("dra"), vec![row]), ["draw(…)"]);
        let mut row = Candidate::new("fn draw(&self)", Source::Server);
        row.info.filter = Some("draw".to_owned());
        assert!(ordered(&Question::stem("fn"), vec![row]).is_empty(), "the label does not match");
    }

    #[test]
    fn a_server_row_takes_the_name_and_keeps_what_the_structural_row_knew() {
        let mut structural =
            Candidate::described("draw", Source::Member, Some(Kind::Method), "Layout");
        structural.info.doc = Some("Draws the layout.".to_owned());
        let mut server =
            Candidate::described("draw", Source::Server, Some(Kind::Method), "fn(&self)");
        server.info.insert = Insert::Call { has_parameters: false };
        let rows = order(&Question::stem("dr"), vec![structural, server], &|_| 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, Source::Server);
        assert_eq!(rows[0].info.insert, Insert::Call { has_parameters: false });
        assert_eq!(rows[0].info.doc.as_deref(), Some("Draws the layout."));
    }

    #[test]
    fn a_stale_server_answer_filtered_by_a_longer_stem_never_shows_a_row_that_cannot_match() {
        let answer: Vec<Candidate> = ["draw", "drain", "dry_run", "redraw"]
            .iter()
            .map(|n| Candidate::new(*n, Source::Server))
            .collect();
        for row in order(&Question::stem("drw"), answer, &|_| 0) {
            assert!(could_match("drw", &row.name), "{} cannot match drw", row.name);
        }
    }

    #[test]
    fn the_order_is_the_same_every_time() {
        let pool = || {
            let mut server =
                Candidate::described("draw_line", Source::Server, Some(Kind::Method), "");
            server.info.server_order = Some(3);
            vec![
                server,
                Candidate::described("draw", Source::ThisFile, Some(Kind::Function), ""),
                Candidate::described("Drawer", Source::Index, Some(Kind::Struct), ""),
                Candidate::described("drawn", Source::Import, Some(Kind::Constant), ""),
            ]
        };
        let q = Question { stem: "dra".to_owned(), place: Place::Expression, ..Default::default() };
        assert_eq!(order(&q, pool(), &|_| 0), order(&q, pool(), &|_| 0));
    }
}
