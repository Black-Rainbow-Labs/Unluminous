# task-2341: Word documents in Unluminous (TDD)

`tasks/task-2341-word-prd.md` is what is built. This is how.

## 1. Shape

Two halves, the split every feature of this size here already has:

- **`crates/unluminous-word`**, with no user interface dependency: the document model, the zip container,
  reading and writing WordprocessingML, styles and numbering, editing operations and undo, fields, the
  table of contents, pagination and line layout, word count, and the exports (text, HTML, Markdown, PDF).
  Its tests run with no window. Text is measured through `unluminous_core::FontMetrics`, so the tests use
  `FixedMetrics` and give exact numbers, and the window passes its real fonts.
- **The `word` plugin** in `unluminous-app`: `plugins/word/plugin.conf`, the provider
  `services::word::Word`, and the drawing in `components::word`.

## 2. Why the engine is written here

`docx-rs` is the most used Rust crate for `.docx`. It reads into its own model and writes that model out,
so anything it does not model is gone after a save; the same is true of `rdocx` and `rwml` for a fresh
write. `ooxmlsdk` keeps more but is generated from the schema, very large, and still lists editing unknown
elements as future work. Interoperability is half of the request and a save that loses somebody's chart
is the fault nobody forgives, so the reader here keeps every part and every element it does not model and
writes them back where the schema puts them (§4.3). `task-2338` made the same decision for workbooks for
the same reason.

Dependencies: `flate2` and `crc32fast` for the zip container and `roxmltree` for reading XML, all three
already in the lock file. XML is written by hand with one escaping function. `image` (already a
dependency of the app) is not needed by the crate: a picture's pixel size is read from its header.

## 3. The model

```rust
pub struct Document {
    pub body: Vec<Block>,
    pub final_section: Section,      // the body's last sectPr
    pub styles: Styles,              // docDefaults, every style, latent styles kept as XML
    pub numbering: Numbering,        // abstractNum and num, kept as XML where not modelled
    pub headers: BTreeMap<String, Story>,   // by relationship id, headers and footers alike
    pub notes: Notes,                // footnotes and endnotes, by id, separators kept
    pub comments: Vec<Comment>,
    pub settings: Settings,          // track changes on, even and odd headers, default tab stop; rest kept
    pub properties: Properties,      // core.xml: title, subject, creator, keywords, dates
    pub media: BTreeMap<String, Media>,     // relationship id -> bytes and content type
    pub package: Package,            // every part, its content type and relationships, as read
    pub root: RootTag,               // the document element's namespace declarations and mc:Ignorable
}
pub enum Block { Paragraph(Paragraph), Table(Table), Sdt(Sdt), Kept(Xml) }
pub struct Paragraph { pub props: ParaProps, pub runs: Vec<Run>, pub marks: Vec<Mark> }
pub struct Run { pub props: RunProps, pub content: Content }
pub enum Content { Text(String), Tab, Break(BreakKind), Picture(Picture), NoteRef(NoteKind, i64),
                   FieldChar(FieldChar), FieldCode(String), Symbol(char), Kept(Xml) }
pub struct Table { pub props: TableProps, pub grid: Vec<i32>, pub rows: Vec<Row> }
pub struct Row { pub props: RowProps, pub cells: Vec<Cell> }
pub struct Cell { pub props: CellProps, pub blocks: Vec<Block> }
pub struct Sdt { pub props: Xml, pub end_props: Option<Xml>, pub blocks: Vec<Block> }
```

- **A run holds one thing.** Text is one run; a tab, a break, a picture, a note reference and a field
  character are each a run of their own. A paragraph's *flat text* is its runs' text concatenated, where
  every run that is not text counts as one character (`\t` for a tab, `\n` for a line break, U+FFFC for
  anything else). Every position in a paragraph is a byte offset into that flat text, which is what the
  caret, the selection, find and the agent all use.
- **What wraps runs in the file is a property of the run in the model.** A hyperlink becomes
  `RunProps::link`, a `w:ins` or `w:del` becomes `RunProps::change`, a `w:smartTag` or a `w:customXml`
  wrapper is unwrapped and its runs read as runs. The writer wraps consecutive runs that share a link or
  a change again. `w:proofErr` and `w:lastRenderedPageBreak` are dropped, which Word allows.
- **What sits between runs and is not content is a mark**: bookmark starts and ends, comment range starts
  and ends, permission starts and ends, each at an offset in the flat text, written back in place.
- **Properties are typed where Unluminous acts on them and kept where it does not.** `RunProps`,
  `ParaProps`, `TableProps`, `RowProps`, `CellProps` and `Section` each have typed `Option` fields and
  `kept: Vec<Xml>`, the child elements nobody modelled, by name. The writer emits known and kept children
  together in the schema's sequence order (§4.4), which is what stops Word calling a file corrupt.
- **A complex field** (`fldChar begin`, `instrText`, `fldChar separate`, result runs, `fldChar end`) is
  kept as those runs. The layout shows the result runs and hides the code, the way Word does with field
  codes off, and `update fields` replaces the result runs.

Positions:

```rust
pub struct Pos { pub story: StoryId, pub path: Path, pub offset: usize }
pub enum StoryId { Body, Header(String), Note(NoteKind, i64), Comment(i64) }
pub struct Path(SmallVec<[u32; 4]>);   // block, then row, cell, block for each table level, block for an sdt
```

A selection is an anchor and a head, both `Pos` in one story. Paragraph order is document order, so two
positions compare by path then offset.

## 4. The file

### 4.1 Zip

`zip.rs` reads the central directory, then each entry by its local header, stored or deflated, and writes
deflated entries with a CRC and a central directory, no zip64. Entries are written in the order they were
read, new parts after. Its interface is `task-2338`'s (`read(bytes) -> Vec<Entry>`, `write(&[Entry])`), so
a later change can move one copy into a shared crate.

### 4.2 Reading

`read::open` follows `_rels/.rels` to the main document part (whatever it is called), the document's
relationships to the styles, numbering, settings, footnotes, endnotes, comments, headers, footers, theme
and media, and `docProps/core.xml` for the properties. Each modelled part is parsed with `roxmltree`; an
element Unluminous does not model is kept as the exact text of the source between its start and end
(`Node::range`), so its prefixes stay as written. Because the writer writes the document element with the
original namespace declarations, every kept fragment is still valid where it is written back.

`mc:AlternateContent` is kept as one fragment. When its fallback holds a `w:drawing` or `w:pict` with a
picture, that picture is what the layout draws.

### 4.3 What is kept

- Every part Unluminous does not regenerate is written back byte for byte under the same name, with its
  content type and relationships: the theme, font table, web settings, custom XML, glossary, VBA project,
  embedded objects, charts.
- A part Unluminous models (styles, numbering, settings, footnotes, endnotes, comments, headers, footers,
  core properties) is written back **as it was read** unless the model of it changed. Each has a `dirty`
  flag; only the document part itself is always regenerated.
- Inside a regenerated part, every element and attribute not modelled is kept, as above.

### 4.4 Writing

`write::save` regenerates `word/document.xml`, any dirty modelled part, `[Content_Types].xml` and the
relationships of any part whose relationships changed (a new picture, a new header, a new comments part).
New relationship ids are `rIdU<n>`, which cannot collide with Word's `rId<n>`.

Child element order follows the schema sequences, from one table per parent in `order.rs`:

- `w:pPr`: pStyle, keepNext, keepLines, pageBreakBefore, framePr, widowControl, numPr, suppressLineNumbers,
  pBdr, shd, tabs, suppressAutoHyphens, kinsoku, wordWrap, overflowPunct, topLinePunct, autoSpaceDE,
  autoSpaceDN, bidi, adjustRightInd, snapToGrid, spacing, ind, contextualSpacing, mirrorIndents,
  suppressOverlap, jc, textDirection, textAlignment, textboxTightWrap, outlineLvl, divId, cnfStyle, rPr,
  sectPr, pPrChange.
- `w:rPr`: rStyle, rFonts, b, bCs, i, iCs, caps, smallCaps, strike, dstrike, outline, shadow, emboss,
  imprint, noProof, snapToGrid, vanish, webHidden, color, spacing, w, kern, position, sz, szCs, highlight,
  u, effect, bdr, shd, fitText, vertAlign, rtl, cs, em, lang, eastAsianLayout, specVanish, oMath, rPrChange.
- `w:sectPr`: headerReference*, footerReference*, footnotePr, endnotePr, type, pgSz, pgMar, paperSrc,
  pgBorders, lnNumType, pgNumType, cols, formProt, vAlign, noEndnote, titlePg, textDirection, bidi,
  rtlGutter, docGrid, printerSettings, sectPrChange.
- `w:tblPr`, `w:trPr`, `w:tcPr` likewise.

An element name not in the table is written after every named one, in the order it was read.

## 5. Styles and numbering

`Styles::resolve_paragraph(&ParaProps) -> ResolvedPara` and `resolve_run(&ParaProps, &RunProps) ->
ResolvedRun` apply, in Word's order: document defaults, the table style (for a paragraph in a table), the
paragraph style chain by `basedOn`, the numbering level's paragraph properties, the paragraph's own
properties; and for a run, the paragraph style's run properties, the character style chain, then the
run's own. Toggle properties (bold, italic, caps, small caps, strike, double strike, vanish) follow the
spec's rule: a toggle set in a style flips the inherited value rather than setting it. A resolved value
is plain (`bold: bool`, `size: f32` in points, `color: Rgb`), and resolution is cached per
`(paragraph style, character style, props hash)`.

`builtin.rs` has Word's own definitions of the built in styles, written as Word 16 writes them for a new
blank document, so applying `Heading 2` to a document that has never used it adds exactly what Word would.

`Numbering::label(num_id, level, counters) -> Label { text, font, tab }` formats one list label from the
level's `lvlText` (`%1.%2.`), its format (decimal, lowerLetter, upperLetter, lowerRoman, upperRoman,
decimalZero, ordinal, cardinalText, ordinalText, bullet, none) and the counters. Counters are kept per
abstract list while the layout walks the document in order, with `lvlRestart`, `startOverride` and
`lvlOverride` applied. New lists use Word's own default bullet and numbered definitions.

## 6. Layout

`layout::Engine::lay_out(&Document, &dyn FontMetrics, &Fonts) -> Pages`.

- **Units.** Points (1/72 inch) everywhere in the layout. The file's twips (1/20 point), half points and
  EMUs (1/12700 point) are converted on reading.
- **Lines.** A paragraph's runs are split into pieces of one resolved style; each piece is measured cluster
  by cluster through `FontMetrics`, with caps and small caps applied, superscript and subscript at Word's
  size (two thirds) and offset, and character spacing added. Lines break at Unicode line break opportunities
  (spaces, after a hyphen, before and after CJK ideographs) with the greedy algorithm Word uses. A word wider
  than the line is broken at a cluster. A tab advances to the next tab stop of the resolved stops, or the
  next multiple of the default tab stop; right, centre and decimal tabs measure the text that follows.
  Justified lines spread the space over the line's spaces, except the paragraph's last line.
- **Line height.** Word's arithmetic: a line's height at single spacing is the tallest font's ascent plus
  descent plus line gap on that line; `auto` multiplies it by `line/240`, `atLeast` takes the larger of the
  two, `exact` takes the value and clips. Spacing before and after are added, with contextual spacing and
  the rule that spacing after one paragraph and before the next are both kept (Word does not collapse
  them, unlike CSS).
- **Pages.** Each section's page size, margins, header and footer distances, columns and break type make
  page boxes. Paragraphs flow into the current column; a line that does not fit moves to the next column or
  page. Keep with next, keep lines together, page break before and widow and orphan control (two lines) are
  applied by backing up to the last legal break. A page break, column break or section break ends the
  column, page or section.
- **Tables.** Column widths from `tblGrid` (or from cell widths when the grid is missing), scaled to `tblW`
  when it is a percentage. Each cell is laid out as a small story with the cell margins; a row's height is
  its tallest cell, or `trHeight` when that is larger, or exactly it when it is exact. A row does not split
  across pages unless it is taller than a page; header rows repeat at the top of each page. `gridSpan` and
  `vMerge` merge cells. Borders resolve table, conditional (first row, banded rows) and cell borders.
- **Pictures.** An inline picture is a cluster as wide and tall as its extent. A floating picture is placed
  at its anchor's position relative to the page, margin, column or paragraph, and lines in its paragraph
  and the ones after it avoid its rectangle when wrapping is square or top and bottom.
- **Headers, footers, footnotes.** Each page's header and footer are the section's first, even or default
  reference, laid out between the header distance and the top margin (pushing the body down if taller).
  Fields in them (PAGE, NUMPAGES, SECTIONPAGES) are evaluated per page. Footnotes referenced on a page are
  laid out at its foot under a separator, reducing the body's height; a note that does not fit continues
  on the next page.
- **Incremental layout.** Pages are rebuilt from the page holding the first changed paragraph, and laying
  out stops early when a page's first paragraph and offset match the previous layout's from that point on.
  The cost of a keystroke in a 300 page document is one page or two.

The result is a list of pages, each a list of `Placed` items (glyph runs with a resolved style, rules,
fills, pictures, list labels) in page coordinates, plus a map from `Pos` to caret rectangles and back,
which is what hit testing and the caret use.

## 7. Editing

`edit.rs` holds every change as a function of a `Document` and a selection, returning the new selection:
insert text, delete backward and forward (by character, word), delete the selection, split a paragraph
(Enter), insert a line break, page break, column break and section break, set run properties over a
range, set paragraph properties over the paragraphs a range touches, apply a style, toggle a list and
change its level, insert and delete table rows and columns, merge and split cells, insert a table, insert
a picture, insert a footnote, a comment, a bookmark, a hyperlink, a field, a table of contents, change case,
clear formatting, paste a fragment.

**Track changes** is a mode of the same functions: with it on, inserted text is a run with
`change: Insert`, deleted text is not removed but marked `change: Delete`, and a deleted paragraph mark
sets the paragraph's `rPr/del`. Accept and reject walk the runs.

**Undo** is a stack of entries, each a copy of the paragraphs and blocks an operation touched (by path),
or of the whole body for a structural change, plus the selection before and after. Typing into one
paragraph is merged into one entry until a space follows a word, the caret moves, or a second passes.
Kept parts are `Arc`, so a whole body copy shares them.

**The clipboard.** Copy writes plain text, and HTML for other programs, and remembers the copied blocks
as a fragment beside the text; a paste whose text is the text Unluminous copied pastes the fragment with
its formatting, styles it needs and pictures. Otherwise the clipboard's text is pasted as runs in the
style at the caret.

## 8. The plugin in the window

### 8.1 Manifest and opening

`plugins/word/plugin.conf`: `plugin.kind = ui`, `ui.provider = word`, `ui.extensions = .docx, .dotx,
.docm`, a menu, a settings page, no pane. `opening.rs` sends a file the plugin claims to
`open_a_word_document`, which opens a plugin tab keyed `word:<path relative to the project>` and tells the
provider to load it. Switched off, `opens_elsewhere` answers nothing and the file opens as "cannot be
shown" naming the plugin.

### 8.2 Three additions to the plugin contract

The same three `task-2338` adds for workbooks, with the same names, each with a default so nothing that
shipped changes:

- **`UiProvider::tab_keyed(key, ui, look)`**, defaulting to `tab`, because a provider that draws one tab a
  file has to know which file the tab is.
- **`UiProvider::modified(key) -> bool`** and **`save(key) -> Result<(), String>`**, defaulting to false and
  a refusal, so a plugin tab has the unsaved dot, is saved by `Ctrl+S` and `File -> Save`, and is asked
  about before it closes.

And one more, **`UiProvider::tab_closed(key)`**, so the provider drops a document's model, layout and undo
when its tab goes.

### 8.3 The provider

`services::word::Word` holds `docs: Vec<Doc>`, each with its path, `Document`, undo stack, layout, view
(selection, scroll, zoom, the panel's level, the bubble) and the clipboard fragment. `open` reads nothing;
a document is read when its tab is first drawn or when a command names it.

### 8.4 Drawing

`components::word`:

- `page_view.rs` draws the pages on the editing area's ground, only those on the screen, each a white
  sheet with a shadow, then its placed items: fills, rules, list labels, glyphs through the window's
  `TextRenderer` (the atlas the editor draws with), pictures as textures made once per picture, the
  selection behind the text and the caret over it. Comments in a margin to the right of the page.
- `format_bar.rs`: the style dropdown and the toggles, `Aa`, `+`, counts and zoom.
- `panel.rs`: the drill-down panel, a stack of levels drawn from one table of `Level`s, so the panel, the
  `Ctrl+/` search and the agent's `panel` verb read one list.
- `bubble.rs`: the selection bubble.
- `slash.rs`: the `/` menu.
- `navigation.rs`: the headings pane.

All of it is drawn with egui and the window's `controls`, `modal` and `rux` furniture, sized by the tab's
own zoom.

### 8.5 Keys

The PRD's list. Arrow keys move by character and line (the line above or below at the same x, read from
the layout), Ctrl with them by word and paragraph, Home and End to the line's ends, Ctrl+Home and Ctrl+End
to the document's, PageUp and PageDown by a screen, Shift to extend. Text arrives as `egui::Event::Text`
and an IME's `Ime::Commit`.

### 8.6 Fonts

`FontMetrics` is the window's `TextRenderer`, which already resolves any installed family through `fontdb`.
`TextRenderer::font_line_metrics` answers a face's own ascent, descent and gap without the reading leading
the editor adds, because Word's single line height is exactly the font's.

## 9. The agent's half

`unluminous-cli/src/plugins.rs` gains `WORD`, so `plugins run word` and the MCP tool are generated from
one list:

`open`, `new`, `docs`, `save`, `save-as`, `close`, `export` (pdf, html, md, txt), `read` (paragraphs with
style, list, table cell and text, optionally runs), `outline`, `stats`, `select`, `selection`, `type`,
`insert` (text, paragraph, break, table, picture, footnote, toc, field, hyperlink, bookmark), `delete`,
`replace`, `find`, `format` (run properties over a range), `paragraph` (paragraph properties), `style`
(apply, define, list), `list`, `table` (insert, row, column, merge, split, delete, cell text, width),
`section` (page size, orientation, margins, columns, header and footer text), `comment` (add, reply,
resolve, delete, list), `track` (on, off, list, accept, reject), `fields` (update), `undo`, `redo`, `pages`
(how many and where each starts), `view` (zoom, marks, navigation), `panel` (open a level).

Each verb calls the same `edit` functions the interface calls, through the provider's `command`.

## 10. The Word oracle

`tools/word-oracle/` holds PowerShell scripts that drive Word through COM, with `DisplayAlerts` off and
the document opened read only:

- `inspect.ps1 <file> <out.json>` writes what Word reads: whether it opened cleanly (a damaged file is an
  exception with alerts off), the page count, and for each paragraph its text, style name, list string,
  alignment, indents and spacing in points, the page it starts on, and its runs' font, size, bold, italic,
  underline, colour and highlight; each table's cells' text; each section's page setup; headers and
  footers; footnotes, comments and revisions.
- `make-fixtures.ps1` builds `crates/unluminous-word/tests/fixtures/*.docx` with Word, one file per area of
  the PRD (formatting, paragraphs, styles, lists, tables, pictures, sections, headers and footers, notes,
  comments, tracked changes, fields and a table of contents, a long document), and runs `inspect.ps1` on
  each into the `.word.json` beside it. Both are committed.
- `roundtrip.ps1 <dir>` inspects every `.docx` in a folder, for the files Unluminous writes.

`tests/oracle.rs` reads each fixture with Unluminous and compares with its `.word.json`. The tests that
start Word (`tests/word_live.rs`) write documents with Unluminous, inspect them with Word and compare; they
print a line and pass where Word is not installed, and `UNLUMINOUS_REQUIRE_WORD=1` makes a missing Word a
failure, the rule `UNLUMINOUS_REQUIRE_KERNEL` set for notebooks.

## 11. Tests

- `unluminous-word`: zip round trip; reading every fixture; writing and reading back; a fixture opened and
  saved unchanged keeps every part byte for byte apart from `document.xml`, and `document.xml` keeps every
  element (compared as canonical XML); element order in every written properties element against the order
  tables; style resolution including toggles; list labels for every format; every edit and its undo;
  track changes and accept and reject; line breaking, tabs, justification and line spacing with
  `FixedMetrics`; pagination with keep rules and widows; tables; footnotes; fields; the table of contents;
  word count against Word's; exports.
- `unluminous-app`: provider command tests driving `Word::command`; window tests in `tests/word.rs` that
  open a fixture, type, format with the bar and the panel, use the `/` menu and the bubble, save, and take
  pictures of the page view, the panel at its levels, the bubble and the slash menu.

## 12. Order of work

1. Crate, zip, units, the model, reading, writing with keeping, order tables; round trip tests.
2. Styles, numbering, builtin styles.
3. The oracle scripts and fixtures; the oracle reading tests.
4. Layout: lines, paragraphs, pages, tables, pictures, headers and footers, footnotes.
5. Editing, undo, track changes, fields, TOC, clipboard fragments, find.
6. The plugin: contract additions, manifest, opening, provider, page view, caret and keys.
7. The format bar, panel, bubble, slash menu, navigation.
8. Exports including PDF.
9. The agent's half, documentation, window tests, release.
