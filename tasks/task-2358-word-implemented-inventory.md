# Word plugin: what is implemented today

Repository `C:/jason/dev/unluminous`, commit 5b719826, release 1.14.2. Read only inventory made by reading the source. No cargo command was run, so statements about behaviour come from reading code and the committed fixtures, not from running anything. Where something could not be determined the row says "not determined".

Path shorthand used in every table:

- `PRD` = `C:/jason/dev/unluminous/tasks/task-2341-word-prd.md`, `TDD` = `C:/jason/dev/unluminous/tasks/task-2341-word-tdd.md`
- `W/` = `C:/jason/dev/unluminous/crates/unluminous-word/src/`
- `WT/` = `C:/jason/dev/unluminous/crates/unluminous-word/tests/`
- `S/` = `C:/jason/dev/unluminous/crates/unluminous-app/src/services/word/`
- `C/` = `C:/jason/dev/unluminous/crates/unluminous-app/src/components/word/`
- `CLI` = `C:/jason/dev/unluminous/unluminous-cli/src/plugins.rs`
- `ORACLE/` = `C:/jason/dev/unluminous/tools/word-oracle/`
- `MANIFEST` = `C:/jason/dev/unluminous/crates/unluminous-app/plugins/word/plugin.conf`

Size of the code read: `W/` is 12,886 lines of Rust (22 modules plus `layout/`), `S/` about 3,500, `C/` about 3,500.

One structural fact that changes how several rows read: the TDD §3 describes a typed model (`ParaProps`, `RunProps`, `Section`, `Story`, `Run.props: RunProps`, `RunProps::link`/`change`). The code does not have those types. Properties are held as the XML tree (`El`) they were read as, and typed values are computed on demand (`W/props.rs` `ParaLook`, `RunLook`). Hyperlinks, tracked changes, fields and content controls are `Wrap` elements on a run (`W/model.rs:169`). Every Rust type named below is the real one in `W/model.rs`.

---

## 1. PRD and TDD promises

### 1a. PRD scope and non-goals, verbatim

PRD lines 244 to 248, "Out of scope, said once":

> Grammar, collaboration and co-authoring, cloud storage, `.doc` reading, encryption,
> macros running, drawing shapes, SmartArt, charts, equation editing, mail merge, compare, and Read Mode.
> Each is said in the plugin's `plugin.limitations` so the Plugins page tells a person before they find out.

The Files table (PRD 79 to 94) also lists as "No": `.doc`/`.rtf`/`.odt` (PRD 85), AutoSave and version history (PRD 88), Print (PRD 92), password protection, encryption, IRM (PRD 93), enforced editing restriction (PRD 94). Other "No" or "kept, not drawn" rows: text effects (PRD 107), picture/shape/SmartArt/chart creation (PRD 153), drawing a table with the pointer (PRD 150), text box/WordArt/drop cap creation (PRD 158), equation editing (PRD 160), line numbers and hyphenation drawn (PRD 172), compare and combine (PRD 192), grammar/thesaurus/translate/read aloud (PRD 195), Read Mode/Web Layout/Outline/Draft (PRD 202), block selection (PRD 208), macros (PRD 210), RTL shaping (PRD 124).

Goals, PRD 28 to 38: six goals (Word files open faithfully; Word opens Unluminous files with no repair; unknown parts survive a save; the everyday features; no ribbon; the whole feature is a plugin that can be disabled).

### 1b. PRD sections and status in code

Status key: DONE (found in code and covered by a test or an obvious code path), PARTIAL, NOT FOUND, DIFFERENT (built another way).

| PRD ref | Promise | Status | Evidence |
|---|---|---|---|
| PRD 47-50, format bar | Style dropdown, bold, italic, underline, list toggle, alignment, `Aa`, `+`, page and word count, zoom | PARTIAL | `C/format_bar.rs:103-327`: style dropdown (133), B/I/U (175-192), bullet and number (200-214), four alignments (216-229), `Aa` (233-244), `+` (245-256), a Ctrl+/ search button (257-277), status text with page, words and zoom percent (281-299). Zoom is shown as text only. |
| PRD 51-55, drill-down panel | Levels with back arrow and breadcrumb; nine first level entries; "some of those open a third level (`Page -> Margins -> Custom`)" | PARTIAL | `C/panel.rs:27-79`; nine levels at `S/mod.rs:96-135`. Back arrow and breadcrumb exist (44-59). No level opens a child level; the panel is the root plus one level. Margins are four drag boxes on the Page level (`C/panel.rs:709-804`). |
| PRD 56-58, selection bubble | Bold, italic, underline, highlight, link, comment | DONE | `C/overlays.rs:307-407`. Highlight toggles yellow only (385-392). Link opens the Insert level at the link field (394-397). |
| PRD 59-61, commands by name | `/` at the start of an empty paragraph; `Ctrl+/` anywhere | DONE | `C/keys.rs:106-110` (slash), `190-193` (Ctrl+/), `C/overlays.rs:17-83`; list is `S/actions.rs:127` `ACTIONS` (100 rows), `search` at `S/actions.rs:1046`. |
| PRD 63-66, Markdown habits | `# `, `## `, `- `, `* `, `1. `, `> `, `---`; "Each is one undo step" | PARTIAL | `C/keys.rs:120-144` (`#` to `####`, `-`, `*`, `1.`, `1)`, `>`), `246-254` (`---` + Enter). The code selects the marker, backspaces it, then runs an action (139-143), which is two undo entries; whether the test `the_markdown_habits_make_headings_and_lists` (`S/tests.rs`) checks that one Ctrl+Z restores the characters: not determined. |
| PRD 68-69, right click menu | Cut, copy, paste, paragraph style, table commands | NOT FOUND | The only right click handler is `C/page_view.rs:619-627` calling `C/spelling.rs:68`, which opens the spelling card or moves the caret. |
| PRD 71-72, navigation pane | From the `Document` level or `Ctrl+Shift+O` | PARTIAL | `C/navigation.rs:1-46`, toggled by action `navigation` and verb `view navigation`. `Ctrl+Shift+O` is not bound (`C/keys.rs:154-190`). The pane skips headings inside tables or content controls (`W/ops.rs:1190`). |
| PRD 79-94, Files | See section 2 | PARTIAL | Section 2. |
| PRD 98-110, Text | Family, size, grow/shrink; bold, italic, 7 underline kinds; strike, double strike; sub/superscript and position; small/all caps, hidden; colour, highlight (16), shading; spacing and scale; effects kept; change case; clear; format painter | PARTIAL | Panel Text level `C/panel.rs:137-256`: family combo, size drag, A+/A-, underline combo (8 names), effects row, 10 colour swatches, 12 of 16 highlights (`take(12)` at 224), character spacing drag, case, copy/paste/clear formatting. No control for character scale (`w:w`), raised or lowered position, run shading. "Named in the panel as missing" (PRD 100): no code (`rg -i missing` in `C/`, `S/` finds nothing relevant). Position and scale are read by layout (`W/props.rs:557-565`) and have no control. |
| PRD 112-124, Paragraph | Alignment; indents; spacing and line spacing with Word's arithmetic; keep with next, keep lines, page break before, widow control; tab stops with leaders; borders and shading; formatting marks; sort; RTL | PARTIAL | `C/panel.rs:279-445`. Tab stop UI adds left, center, right, decimal at a position; no leader, no bar (410-445). Borders: a "Box" preset, "Line below", "No border", one fixed shade F2F2F2 (360-409). Sort paragraphs: NOT FOUND. RTL: `ParaLook.bidi` is parsed (`W/props.rs:371,449`) and never read by `W/layout/`. Keep with next: parsed, shown in the panel, and then ignored by the page flow (section 5). Contextual spacing is applied (`W/layout/mod.rs:671-676`). Formatting marks: `Ctrl+Shift+8` (`C/keys.rs:181`), `W/layout/mod.rs:271`. |
| PRD 126-134, Styles | Apply; built in styles added on first use; based on; modify; create from selection; theme read | PARTIAL | Apply `W/session.rs:427`; built in added on first use `W/builtin.rs:23`; create from selection `C/panel.rs:512`; theme `W/styles.rs:21-110`. Modify an existing style: NOT FOUND. Linked styles (`w:link`): not determined, no handling found in `W/styles.rs`. |
| PRD 136-143, Lists | Bullets, numbering, multilevel; formats; picture bullets as default bullet; restart, continue, set value; Tab and Shift+Tab | PARTIAL | `W/ops.rs:47,116,148`; formats `W/numbering.rs:225-240` (decimal, decimalZero, upper and lower roman, upper and lower letter, ordinal, cardinalText, ordinalText, decimalEnclosedParen, none, bullet; every other format prints as decimal). No `lvlPicBulletId` handling. Only "restart at N" exists; continue and set value as separate commands: NOT FOUND. |
| PRD 145-162, Insert | Breaks; table; picture from file and clipboard; resize and crop; shapes kept; hyperlink; bookmark; cross reference; header, footer, page number; text box drawn as its text; date and fields; equation as linear text; symbol; horizontal line | PARTIAL | Breaks DONE. Table DONE (`W/ops.rs:221`). Picture from file DONE (`S/actions.rs:107-114`, `S/commands.rs:509`); from clipboard NOT FOUND; resize after insert NOT FOUND (width chosen only at insert, `S/commands.rs:901`); crop not applied (`srcRect` absent from `W/`). Hyperlink DONE (`W/ops.rs:993`). Bookmark DONE (`W/ops.rs:1036`). Cross reference insert: NOT FOUND (stored result only). Header and footer with first and even DONE (`W/ops.rs:638`). Text box text: NOT DONE (section 3). Equation as linear text: NOT DONE; `m:oMath` lays out as a zero width empty glyph (`W/layout/paragraph.rs:538-553`). Symbol: 24 fixed buttons (`C/panel.rs:658-668`), no picker for any character. Horizontal line DONE (bottom border). |
| PRD 164-175, Layout | Margins incl. moderate and mirrored; orientation; sizes; columns; sections; line numbers and hyphenation kept; page colour; watermark; page borders kept | PARTIAL | Sizes: 8 (`W/ops.rs:19-28`: letter, legal, a4, a5, a3, b5, executive, tabloid). Margins: actions normal, narrow, wide and four drag boxes; `moderate` exists in `W/ops.rs:31-36` and the verb only; mirrored: NOT FOUND (`mirrorMargins` unread). Columns: buttons 1 to 3, any count by verb (`W/ops.rs:586`), equal width only. Page colour: 5 swatches (`C/panel.rs:775-803`), drawn (`W/layout/mod.rs:547-552`). Watermark: NOT drawn (a VML or WordArt header shape becomes a placeholder box). |
| PRD 177-183, References | TOC insert, update, remove; footnotes and endnotes; captions | PARTIAL | Insert `W/ops.rs:1121`, update `W/ops.rs:1212`, remove NOT FOUND. Notes DONE (`W/ops.rs:706`, layout `W/layout/mod.rs:980,739`). Caption action `caption` (`S/actions.rs`). |
| PRD 185-195, Review | Comments new, reply, edit, delete, resolve, show; track changes with accept and reject; compare no; word count; spelling | PARTIAL | Comments: add, reply, resolve, reopen, delete, list (`W/ops.rs:770,920,949,966,855`); edit text NOT FOUND. Track changes insert and delete DONE (`W/edit.rs:16-40`, `805-890`); formatting changes made while tracking are not recorded as `w:rPrChange` (the only `rPrChange` code resolves existing ones, `W/edit.rs:849,872`). Word count DONE (`W/session.rs:644`). Spelling DONE (section 8). |
| PRD 197-210, View and editing | Print Layout; zoom with Ctrl+wheel, `Fit Width`, `Fit Page`; navigation pane; find, replace, go to page; undo, redo; clipboard; selection modes; shortcuts | PARTIAL | Print Layout only. Ctrl+wheel zoom DONE (`C/page_view.rs:561-586`). `Fit Width` and `Fit Page`: NOT FOUND. Go to page, `Ctrl+G`: NOT FOUND. Find and replace DONE with match case and whole words (`W/session.rs:576,615`, `C/overlays.rs:409-525`). Selection: double click word, triple click paragraph (`C/page_view.rs:668-697`), Shift extend; Ctrl+click sentence NOT FOUND. Shortcuts: section 6d. |
| PRD 212-217, agent half | Verbs listed in TDD §9 | PARTIAL | Section 7. |
| PRD 219-227, switching off | Off means docx opens as "cannot be shown"; nothing runs at startup; closing the last tab drops everything | DONE | Test `switched_off_a_docx_is_a_file_that_cannot_be_shown` (`crates/unluminous-app/tests/word.rs`); `S/mod.rs:540-546`. |
| PRD 229-242, interoperability tests | Five test levels | PARTIAL | Section 9. Level 1 compares only paragraph text, style, alignment, indents, spacing, keepNext and pageBreakBefore (`WT/oracle.rs:130-246`). |
| PRD 247-248 | Each out of scope item is in `plugin.limitations` | PARTIAL | `MANIFEST:13`. It says text boxes, equations, shapes etc. "are drawn as their stored picture or text when the file has one"; the layout does not draw text box text or equations (section 3). |

### 1c. TDD sections and status

| TDD ref | Promise | Status | Evidence |
|---|---|---|---|
| §1 (5-15) | Two halves: `crates/unluminous-word` and the `word` plugin | DONE | `W/lib.rs:14-35`; `S/mod.rs`, `C/mod.rs`. |
| §2 (17-29) | Own engine; `flate2`, `crc32fast`, `roxmltree`; picture pixel size read from the header "by the crate" | DONE except last clause | `crates/unluminous-word/Cargo.toml`. Picture pixel size is read in the app by `services::picture::dimensions_of` (`S/commands.rs:901+`), not by this crate. |
| §3 (31-87) Model | Typed `ParaProps`, `RunProps`, `Section`; `Story`; `Pos`/`StoryId`/`Path` | DIFFERENT | Real model `W/model.rs:26-243` (XML tree properties). `StoryId` is `Body, Header(id), Footnote(id), Endnote(id), Comment(id)` (`W/position.rs:14`, names `S/mod.rs:592`). The behaviour promised (flat text with one character per non text run, marks, wraps) is DONE (`W/model.rs:343-352`, `W/read.rs:212-231,247-280`). |
| §4.1 (91-96) | Zip read and write, no zip64 | DONE | `W/zip.rs` (zip64 refused at 53; encrypted entry refused at 75). |
| §4.2 (98-108) | Main part via `_rels/.rels`; `mc:AlternateContent` kept as one fragment | DONE | `W/read.rs:14-56`. |
| §4.3 (110-118) | Unmodelled parts byte for byte; modelled parts as read unless dirty | DONE | `W/write.rs:16-39`; test `every_fixture_saved_unchanged_keeps_every_part_and_element` (`WT/roundtrip.rs:94`). |
| §4.4 (120-141) | Schema order tables, `rIdU<n>` ids | DONE | `W/order.rs`; `W/package.rs:86`; test `a_new_relationship_id_cannot_be_one_word_makes`. |
| §5 (143-161) | Style resolution order, toggles, built in styles, list labels | DONE except table style detail | `W/styles.rs:245-312`, `W/numbering.rs:146-220`. Table style `basedOn` and conditional run formatting are not applied (`W/styles.rs:250,283`; `tblStylePr` read only for cell shading, `W/layout/table.rs:556-590`). |
| §6 (163-205) Layout | Lines, tabs, justification, spacing, keep and widow rules, tables (rows split, header repeat, gridSpan, vMerge, borders), pictures with floats avoided, headers and footers (body pushed down), footnotes (continue on next page), incremental layout | PARTIAL | Section 5. Differences: rows never split (`W/layout/mod.rs:5-6,717-737`); floats do not displace text (`Float.wrap` is never read); body is not pushed down by a tall header; footnotes do not continue; layout is not incremental (`S/mod.rs` `Doc::lay_out` re-lays the whole document whenever `revision` changes). |
| §7 (207-229) Editing | The listed operations; track changes; undo as path copies; clipboard fragment plus HTML | PARTIAL | Undo copies the whole story, or the whole document for `Scope::Whole` (`W/session.rs:124-148`), 500 steps (73). Clipboard writes plain text only (`C/keys.rs:62-71`); no HTML. |
| §8.1 (233-239) | Manifest | DONE | `MANIFEST:19-28`. |
| §8.2 (241-253) | Contract additions | DONE | `C:/jason/dev/unluminous/CLAUDE.md` from line 7541; `S/mod.rs:540-546`. |
| §8.3-8.4 (255-277) | Provider; `page_view`, `format_bar`, `panel`, `bubble`, `slash`, `navigation` files | DIFFERENT | Bubble and slash menu are in `C/overlays.rs`; there is no `bubble.rs` or `slash.rs`. Pictures as textures made once per picture DONE (`C/page_view.rs:418-437`). |
| §8.5 (279-284) | Keys | PARTIAL | Section 6d. |
| §8.6 (286-290) | Fonts | DONE | `S/fonts.rs`, `crates/unluminous-app/src/services/text_renderer.rs:1275`. |
| §9 (292-306) | Verbs | PARTIAL | Section 7. |
| §10 (308-327) | Oracle scripts and tests | DONE | `ORACLE/`, `WT/`. |
| §11 (329-340) | Test list: zip, fixtures, order, style toggles, list labels, every edit and its undo, track changes, line breaking with `FixedMetrics`, pagination, tables, footnotes, fields, TOC, word count, exports | PARTIAL | Section 9. No in file tests in `W/layout/paragraph.rs`, `W/layout/table.rs`, `W/layout/mod.rs`, `W/styles.rs`; no test of style toggles. |
| §12 (342-352) | Nine steps | DONE | All nine have code. |
| §13 (354-391) Spell check | `spell.rs`, `spellbook`, SCOWL en_US, cache, card, F7, agent verb, custom dictionaries | DONE | Section 8. |

---

## 2. File formats

| Question | Answer | Evidence |
|---|---|---|
| Extensions that open | `.docx`, `.dotx`, `.docm`. The reader accepts any zip whose main part root is `w:document` (`W/read.rs:14-30`). | `MANIFEST:19`. |
| `.dotx` | Opens as a new document: the path is cleared and the creator emptied. The main part's content type is not rewritten on save (no code mentions `template.main` or `macroEnabled`; `rg "main\+xml|macroEnabled"` in `W/` is empty), so a template Saved As `.docx` keeps the template content type. Whether Word accepts that file: not determined. | `S/mod.rs:377-387`; `W/package.rs:273-312`. |
| `.docm` | Opens. Every part including `word/vbaProject.bin`, its relationship and content type is held in `Package` and written back byte for byte, so macros survive a save in place. Macros never run. No test opens a `.docm`; there is no `.docm` fixture. Save As offers a `.docx` filter only and appends `.docx` when no extension is typed (`C:/jason/dev/unluminous/crates/unluminous-app/src/app/menus.rs:516-545`), while the content type stays macro enabled. | `W/write.rs:16-39`, `W/package.rs`. |
| Other formats open | `.doc`, `.rtf`, `.odt`: not read. A `.doc`, or a password protected `.docx` (both start with the OLE signature `D0 CF 11 E0 A1 B1 1A E1`), fails with "this is a Word 97-2003 document or a password protected one, which is not a zip; open it in Word and save it as .docx". `.txt`, `.md`, `.html` open in the ordinary editor, not this plugin. | `W/zip.rs:84-91`; test `an_old_word_file_is_named_rather_than_called_damaged`. |
| Save | `.docx` only: Save, Save As (path argument to the verb `save`), save on close. Save writes `cp:lastModifiedBy`, `dcterms:modified`, and `dc:creator` when empty. | `S/mod.rs:304-340`; `menus.rs:516-545`. |
| Exports | PDF, HTML, Markdown, plain text. | `S/commands.rs:928-960`, `W/export.rs:13,114,221`, `W/pdf.rs:39`. |
| PDF specifics | Draws the laid out pages. Each TrueType face is embedded whole as a CID font with a `ToUnicode` map; a face in a font collection is replaced by Helvetica; JPEG bytes embedded as is, other pictures re-encoded as RGB; the grey placeholder boxes are drawn. No link annotations, outlines or tags found in `W/pdf.rs`. | `W/pdf.rs:1-60`, one test. |
| HTML, Markdown, text | Body only (the walkers go over `document.body`); no headers, footers, footnotes or comments. HTML inlines pictures as base64 data. | `W/export.rs:13-120,221-383`, `350-362`. |
| Imports | None. Paste of text from anywhere is plain text; a paste of exactly what a Word tab copied keeps its formatting. | `C/keys.rs:40-48,73-78`. |
| Password protected or encrypted | Not read (above). An encrypted entry inside a zip is refused. | `W/zip.rs:73-75,84-91`. |
| `w:documentProtection`, `w:writeProtection` | Not enforced and not mentioned in code (`rg` in `W/`, `S/`, `C/` finds nothing). `settings.xml` is kept, so the element survives. Permission ranges (`w:permStart`/`End`) are kept as marks. | `W/read.rs:212-231`. |
| Templates | New documents come from `W/templates/blank.docx`, a document Word 16 wrote: Letter (`w:pgSz` 12240 x 15840), document default size 12 pt (`w:sz 24`), theme minor font, `en-US`. The PRD said Letter or A4 by locale and Calibri 11; the code always gives Letter and 12 pt. | `W/new.rs:10-20`; `blank.docx` inspected. |
| Where "new" is reachable | The plugin's "Word" menu entry "New Word Document" (`MANIFEST:23`) and `plugins run word new [path]` (`S/commands.rs:69`). The PRD also promised File menu and explorer right click; no code found (`rg -i "new word"` outside the plugin returns nothing). | |
| Document properties | title, subject, author, keywords in the Document level; description by verb only | `C/panel.rs:912-947`, `S/commands.rs:353-376`. |

---

## 3. Document model

### 3a. Containers and blocks

| Element | Treatment | Rust type | Evidence |
|---|---|---|---|
| `w:body` children `w:p`, `w:tbl`, `w:sdt` | Modelled | `Block::{Paragraph, Table, Sdt, Kept}` | `W/model.rs:101-108`, `W/read.rs:126-158` |
| Any other block child (`w:bookmarkStart` at block level, `w:customXml`, `w:altChunk`...) | Kept, written back, not laid out | `Block::Kept(El)` | `W/read.rs:154`, `W/layout/mod.rs:465` |
| Body `w:sectPr` | Held as XML | `Document.section: El` | `W/model.rs:30` |
| Paragraph `w:sectPr` (a section end) | In the paragraph's `w:pPr` | `Paragraph.props: El` | `W/layout/mod.rs:413-425` |
| `w:background` | Kept; only `w:color` read for the page colour | `Document.background: Option<El>` | `W/model.rs:34`, `W/layout/mod.rs:547-552` |
| Root namespaces, `mc:Ignorable` | Kept | `Document.root: El` | `W/model.rs:32`, `W/write.rs:118` |
| Other parts (theme, fontTable, webSettings, customXml, glossary, VBA, embeddings, charts, diagrams, printerSettings) | Kept byte for byte | `Package.parts` | `W/package.rs`, `W/write.rs:28-37` |
| styles, numbering, settings, core | Parsed to a tree, rewritten only when `dirty` | `XmlPart` | `W/model.rs:56-61` |
| Headers, footers | Blocks | `HeaderPart`, `Story` | `W/model.rs:64-97` |
| Footnotes, endnotes, comments | A list of stories | `NotesPart` | `W/model.rs:75-82` |
| `commentsExtended` | Read and written by comment code (done flag and reply parent) | raw part | `W/ops.rs:1561-1600` |

### 3b. The elements asked about

| Element or feature | Answer | Evidence |
|---|---|---|
| `w:tbl` | Modelled: `Table { props: El, grid: El, rows, extra }`, `Row { attrs, exceptions (tblPrEx), props, cells, extra }`, `Cell { attrs, props, blocks }`. | `W/model.rs:196-236` |
| Nested tables | Supported (a cell holds `Vec<Block>` and layout recurses). Fixture `tables.docx` has one. Only page counts and page starts of top level paragraphs are compared against Word. | `W/layout/mod.rs:880-885`; `ORACLE/make-fixtures.ps1:289-329` |
| Merged cells | `w:gridSpan` and `w:vMerge` restart and continue are laid out and drawn as one cell; operations `merge-right`, `merge-down`, `split` exist. | `W/layout/table.rs:315-326,363-377,389-392`; `W/ops.rs:348,365,381` |
| `w:gridBefore` | Read. `w:gridAfter`, `w:wBefore`, `w:wAfter`: not read. | `W/layout/table.rs:498` |
| Table styles | Only the named style's own `w:tblPr` (borders, cell margins), `w:pPr`, `w:rPr`. `w:tblStylePr` (firstRow, lastRow, band1Horz, band2Horz) is read only to take a cell shading colour. No `basedOn` chain for table styles, no first or last column, no vertical bands, no conditional run properties (a header row bold only through the style draws regular). | `W/layout/table.rs:244-270,556-590`; `W/styles.rs:250,283` |
| Repeat header row | Yes: leading `w:tblHeader` rows repeat at the top of each page the table continues on. Panel button "Header row" (`header-row`). | `W/layout/table.rs:465`; `W/layout/mod.rs:717-735`; `W/ops.rs:393` |
| Autofit, `w:tblLayout` | Not read. Widths come from `w:tblGrid`; only `w:tblW type=pct` rescales; `dxa` `tblW` and cell `w:tcW` are ignored when a grid exists; with no grid, equal columns. | `W/layout/table.rs:472-496` |
| Row height | `w:trHeight` atLeast and exact | `W/layout/table.rs:353-359` |
| Rows across pages | A row is one indivisible piece. A row taller than a page is placed at the column top and overflows. | `W/layout/mod.rs:5-6`, `717-737` |
| Table borders | Cell `w:tcBorders`, table `w:tblBorders` (top, left, bottom, right, insideH, insideV; single and double only), fills from `w:shd`; diagonal borders not drawn. | `W/layout/table.rs:413-459` |
| Cell vertical alignment, table alignment, `w:tblInd` | top, centre, bottom; `w:jc` centre and right; indent. | `W/layout/table.rs:286-292,406-410` |
| Floating tables (`w:tblpPr`) | Not read. | not found |
| `w:drawing` inline pictures | Drawn. Size from `wp:extent`; bytes from the `a:blip r:embed` or `r:link` relationship. | `W/layout/paragraph.rs:588-653` |
| Image formats decoded | Decoder is the `image` crate with features png, jpeg, gif, bmp, ico, webp, tiff (`C:/jason/dev/unluminous/Cargo.toml:153-161`), called by `C/page_view.rs:418-437` through `services::picture::decode_bytes`. EMF, WMF and SVG do not decode: a grey outlined box is drawn (`C/page_view.rs:331-338`). The Insert dialog offers png, jpg, jpeg, gif, bmp. | |
| `wp:anchor` floating pictures | Size, horizontal and vertical position (relative to page, margin, column, paragraph, line, character, left or top margin; offset or align) are read and the picture is drawn there (`W/layout/mod.rs:798-826`). Wrapping is not applied: `Float.wrap` is stored (`square`, `tight`, `through`, `topandbottom`, `none`, `behind`) and never read (`rg "\.wrap\b" W/layout` is empty), so no line avoids the rectangle. Draw order is the same for every float (pictures before text, so `behindDoc` and in front look the same). | `W/layout/paragraph.rs:620-653`; `W/layout/mod.rs:186-202,799-826` |
| Text boxes `w:txbxContent`, `v:textbox` | Not found anywhere in `W/` (`rg "txbx|v:textbox"` is empty). A text box inside `mc:AlternateContent` is reached through `Content::picture()` (`W/model.rs:294-303`), which takes the first `w:drawing` or `w:pict` in any branch, Choice first. That has no `a:blip`, so it becomes an inline or floating "picture" with an empty part and is drawn as a grey outlined box (`W/layout/paragraph.rs:1079`, `C/page_view.rs:298-337`). The text inside is not drawn. This contradicts PRD 158 and `MANIFEST:13`. | |
| Shapes `a:sp`, `wps:wsp` | Same as text boxes: kept, drawn as a grey box. | |
| Charts | Kept (chart part, relationship, content type are in `Package`); the frame is a `w:drawing` with an extent and draws as a grey box. | |
| SmartArt | Same. Its fallback picture is used only if Choice holds no `w:drawing` or `w:pict`, because `picture()` takes the first match. Not determined for real SmartArt (no fixture). | `W/model.rs:294-303` |
| VML `w:pict` | `v:imagedata r:id` is the image; size from `style="width:..;height:.."` (default 72 x 72 pt); drawn inline. Anchored VML position ignored. | `W/layout/paragraph.rs:588-612,655-670` |
| `w:object` (OLE) | Kept as one object character, zero width. | `W/read.rs:272`; `W/layout/paragraph.rs:538-553` |
| `w:fldSimple` | A `Wrap`; stored result runs are drawn; `PAGE`, `NUMPAGES`, `SECTIONPAGES` replaced in headers and footers. | `W/read.rs:203`; `W/layout/paragraph.rs:302-315` |
| `w:fldChar`, `w:instrText` | Runs `Content::FieldChar`, `Content::Instr`. Layout hides the code and shows the stored result; nested fields tracked on a stack. | `W/model.rs:159-161`; `W/layout/paragraph.rs:236-300` |
| Field codes evaluated | At layout time, in headers and footers only: `PAGE`, `NUMPAGES`, `SECTIONPAGES` with `\* roman`, `ROMAN`, `alphabetic`, `ALPHABETIC`, `Ordinal`, `CardText` (`W/layout/fields.rs:187-205`). At `update fields` (`F9`, verb `fields`): `TOC` rebuilt, and `DATE`, `TIME`, `CREATEDATE`, `SAVEDATE` rewritten with `\@` pictures made of yyyy, yy, MMMM, MMM, MM, dd, d, HH, mm (`W/ops.rs:1212-1250,1718-1760`). Not evaluated (stored result shown): `REF`, `PAGEREF`, `SEQ`, `HYPERLINK`, `AUTHOR`, `TITLE`, `FILENAME`, `NUMWORDS`, `STYLEREF`, `IF`, `=`, mail merge. `PAGE` in the body keeps its stored value because `ctx.page` is `None` while the body is laid out (`W/layout/mod.rs:393-394`). PRD 159 promised AUTHOR, TITLE, FILENAME, REF, HYPERLINK: not done. | |
| Insert field | `insert_field(code, result)` writes a complex field with that code and stored result; the verb passes an empty result. | `W/ops.rs:1083`; `S/commands.rs:522-525` |
| TOC | Insert builds a "Contents" title (`TOCHeading`) and TOC1..n paragraphs with right tab dot leaders and `\h` hyperlinks to `_Toc` bookmarks it adds to the headings. It is not wrapped in the `w:sdt docPartObj` Word uses. Update (`find_toc`, `W/ops.rs:1252`) and the heading list (`toc_entries`, `W/ops.rs:1182`) both skip every paragraph whose path length is not 1, i.e. every paragraph inside a content control or a table. A TOC Word wrote is a block `w:sdt` (docPartGallery), whose paragraphs have path length 2, so `update fields` does not find it and leaves it alone. Derived from the code; not executed. Fixture `fields.docx` has a Word made TOC and no test updates it. | `W/ops.rs:1121-1295` |
| `w:sdt` content controls | Block level: `Block::Sdt`, content laid out, properties kept. Inline: a `Wrap`. Not editable as controls (no dropdown, date picker, checkbox). Written back. | `W/model.rs:240`; `W/read.rs:138-149,286-293`; `W/write.rs:165-190,301-318` |
| `w:hyperlink` | `Wrap`; external via a relationship, internal via `w:anchor`. Drawn in the theme hyperlink colour when the run has no `rStyle` and no colour; underline from the `Hyperlink` style. Following a link (click or Ctrl+click): NOT FOUND. | `W/model.rs:331`; `W/layout/paragraph.rs:296-300`; `W/ops.rs:993-1033` |
| Bookmarks | `Mark` (`w:bookmarkStart`, `End`); list in the References level; insert; click to go. | `W/read.rs:212-231`; `W/ops.rs:1036,1064`; `C/panel.rs:805-819` |
| `w:footnote`, `w:endnote` | Stories. Footnotes numbered arabic, endnotes lower roman, by first reference; `w:customMarkFollows` references not counted. Drawn at the foot of the page and after the last body content. `w:footnotePr`/`w:endnotePr` (restart rules, number format) not read. | `W/layout/mod.rs:343-368,739-796,980-1038`; `W/layout/paragraph.rs:418-460` |
| `w:comment` | Read from `comments.xml`; drawn as cards in a margin beside the pages with a line to the anchor; add, reply, resolve, reopen, delete, list; done flag and reply parent in `commentsExtended.xml`. Editing the text: no. Comment on text in a header or note: not determined. | `W/ops.rs:770-990`; `C/page_view.rs:180-265` |
| Tracked changes | `w:ins`, `w:del`, `w:moveFrom`, `w:moveTo` are `Wrap`s. Shown: inserted and deleted text in red `C00000` (`REVISION`, `W/layout/paragraph.rs:24`), inserted underlined, deleted struck. Tracking on: typing wraps in `w:ins`, deleting marks `w:del`, a deleted paragraph mark sets `rPr/del`. Accept or reject one or all, including existing `w:rPrChange` and `w:pPrChange` (`W/edit.rs:805-890`). New formatting under tracking is not recorded as `rPrChange`. Row and cell revisions (`w:cellIns`, `trPr/w:ins`) are not read. The `w:trackRevisions` setting is read at open (`W/session.rs:97-100`); writing it back on save: not found. | `W/edit.rs`; `W/session.rs:531-575` |
| `m:oMath`, `m:oMathPara` | Kept as one object character. Not drawn (zero width, no linear text). | `W/read.rs:271-278`; `W/layout/paragraph.rs:538-553` |
| `w:sym` | Drawn as the mapped character (the `F000` range maps down); symbol font bullets via `symbol_to_unicode`. | `W/model.rs:266-273`; `W/layout/paragraph.rs:503-515`; `W/numbering.rs:364` |
| Tab stops | Left, centre, right, decimal with dot, hyphen, underscore, middle dot leaders; bar parsed and skipped; default tab stop from settings. | `W/props.rs:458-483`; `W/layout/paragraph.rs:688-701` |
| `w:br` | page, column, and everything else as a line break; `w:cr` as a line break; `w:lastRenderedPageBreak` dropped on read. | `W/read.rs:321,337`; `W/layout/paragraph.rs:325-337` |
| Section properties | Used: `w:pgSz` size and orientation, `w:pgMar` margins, header and footer distance, gutter, `w:cols` (equal, or explicit when `equalWidth=0`, up to 45), `w:type` (nextPage, continuous, evenPage, oddPage, nextColumn), `w:titlePg`, header and footer references default, first, even with inheritance from the previous section, `pgNumType w:start`. Read and then unused by drawing: `pgNumType w:fmt` (`page_number_format`) and `w:vAlign` (`vertical_center`) (`W/layout/section.rs:41-45`; nothing else reads them). Not read: `w:pgBorders`, `w:lnNumType`, `w:textDirection`, `w:docGrid`, `w:rtlGutter`, `w:mirrorMargins`, `w:paperSrc`, column separator (`w:cols w:sep`), `w:footnotePr`. Page colour yes; `w:displayBackgroundShape` is written when a colour is chosen and never read. | `W/layout/section.rs:51-144`; `W/layout/mod.rs:572-606,917-977` |
| Paragraph properties | Typed through `ParaLook`: alignment (left, centre, right, both and its variants to justify), indents left, right, start, end, first line, hanging; spacing before, after, line with auto, atLeast, exact, plus `beforeAutospacing` and `afterAutospacing` (14 pt); keepNext, keepLines, pageBreakBefore, widowControl, contextualSpacing, outlineLvl, tabs, borders (top, left, bottom, right, between; single and double), shading fill, bidi (parsed only), numbering. Kept in the tree and not acted on: `framePr`, `suppressLineNumbers`, `suppressAutoHyphens`, `textAlignment`, `textDirection`, `snapToGrid`, `mirrorIndents`, `wordWrap`, `divId`, `cnfStyle`. | `W/props.rs:343-496` |
| Run properties | Typed through `RunLook`: font (ascii, hAnsi, theme fonts), size, bold, italic, underline (9 kinds; heavy and dashed variants folded), strike, dstrike, caps, smallCaps, vanish, colour (theme colour with shade and tint), highlight (16), shading fill, vertAlign, position, spacing, scale (`w:w`), kern. Kept and not drawn: `w:outline`, `w:shadow`, `w:emboss`, `w:imprint`, `w:effect`, `w:bdr`, `w:fitText`, `w:em`, `w:rtl`, `w:cs`, `w:lang` (used only by spell check), `w14:` ligatures and OpenType features, `w:eastAsianLayout`, `w:specVanish`. | `W/props.rs:187-239,518-569`; `W/layout/paragraph.rs:81-103` |
| Styles | Paragraph, character, table, numbering types read by id; `basedOn` chain walked for paragraph and character (`W/styles.rs:190-200`); document defaults; toggles combine by XOR between the paragraph chain and the character chain (`W/styles.rs:296-306`); theme fonts and colours (`Theme::of`). `w:link` not followed; table style `basedOn` not followed. | `W/styles.rs` |
| Numbering | `abstractNum`, `num`, `lvlOverride`/`startOverride`, `lvlRestart`, `isLgl`, `suff` (tab, space, nothing), `lvlJc`, label run properties, `pStyle` linked levels (`level_for_style`), symbol font bullets. Not handled: `w:lvlPicBulletId`/`w:numPicBullet`; `w:numStyleLink`/`styleLink` (not determined); number formats beyond the list in section 1b (they print as decimal). Word's default bullet and numbered definitions for new lists: `W/builtin.rs:104-215`. | `W/numbering.rs:13-240`; `W/builtin.rs` |
| `w:ruby` | Not handled (no hit). | not found |
| `w:smartTag`, `w:customXml`, `w:dir`, `w:bdo` | Unwrapped; the wrapper is kept on each run's `wraps` and written back; no direction effect. | `W/read.rs:197-209,283-304` |
| Settings compat | `settings.xml` kept. Read: `defaultTabStop`, `evenAndOddHeaders`, `trackRevisions`. `w:compat` (compatibilityMode and the spacing rules it switches) is not read, so one set of layout rules applies to all documents; effect on old mode files: not determined. | `W/layout/mod.rs:371-379,923-928`; `W/session.rs:97-100` |
| `w:cols` | Count, `w:space`, `w:equalWidth`, explicit `w:col`; separator line not drawn; a continuous section break starts its columns below existing content; no column balancing. | `W/layout/section.rs:125-144`; `W/layout/mod.rs:572-606` |

---

## 4. Editing operations

### 4a. Functions in `unluminous_word`

`W/edit.rs` (document functions, no selection state): `props_at` (52), `insert_in_paragraph` (107), `insert_text` (182), `split_paragraph` (212), `delete_range` (360), `delete_backward` (416), `delete_forward` (460), `previous_boundary`, `next_boundary`, `previous_word`, `next_word` (489-516), `format_runs_in_paragraph` (529), `format_range` (562), `covered` (577), `format_paragraphs` (597), `insert_block` (608), `new_table` (632), `copy_fragment` (687), `paste_fragment` (721), `resolve_revisions` (805).

`W/session.rs` `Session`: `new` (78), `change` (124), `can_undo`, `can_redo`, `undo`, `redo` (158-177), `type_text` (211), `enter` (277), `backspace` (299), `delete` (314), `insert_break` (329), `look_at_caret` (336), `paragraph_at_caret` (354), `format` (363), `toggle` (401), `format_paragraphs` (419), `apply_style` (427), `select_all` (454), `select` (466), `selected_text` (478), `copy` (494), `paste_fragment` (502), `resolve_revisions` (531), `set_tracking` (564), `find` (576), `replace_all` (615), `stats` (644).

`W/ops.rs` `impl Session`: `toggle_list` (47), `change_list_level` (116), `restart_list` (148), `insert_table` (221), `cell_at_caret` (240), `table_op` (247: row-above, row-below, column-left, column-right, delete-row, delete-column, delete-table, merge-right, merge-down, split, header-row), `set_column_width` (413), `shade_cell` (441), `section_at_caret` (470), `change_section` (491), `set_page_size` (507), `set_orientation` (542), `set_margins` (571), `set_columns` (586), `insert_section_break` (600), `set_header_text` (638), `insert_note` (706), `add_comment` (770), `comments` (855), `reply_to_comment` (920), `resolve_comment` (949), `delete_comment` (966), `insert_hyperlink` (993), `insert_bookmark` (1036), `bookmarks` (1064), `insert_field` (1083), `insert_toc` (1121), `toc_entries` (1182), `update_fields` (1212), `change_case` (1297), `clear_formatting` (1323).

`W/media.rs`: `insert_picture` (28), `drawing` (94).

### 4b. What the UI exposes for each

| Operation | UI | Notes and evidence |
|---|---|---|
| Typing | Yes | `C/keys.rs:31-32,87-116` (`Event::Text`, IME commit). |
| Selection | Click, drag (scrolls at the edge), Shift+click, double click word (takes trailing spaces), triple click paragraph, Shift+arrows, Ctrl+A. No Ctrl+click sentence, no block selection, no multi cell table selection. | `C/page_view.rs:588-697`; `C/keys.rs:172`. |
| Cut, copy, paste | Cut and copy put plain text on the clipboard and remember the copied paragraphs in a process wide static; a paste whose text equals the copied text pastes the paragraphs with formatting (also into another document); any other paste is typed plain text. No HTML or RTF to or from the clipboard, no "paste as plain text" command, no picture paste. | `C/keys.rs:33-78`. |
| Undo, redo | Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z, actions, verbs. Typing joins steps until a space follows a word, the caret moves or 1 s passes. 500 steps. Each step stores a clone of the whole story (or the whole document for structural changes). | `W/session.rs:73-75,124-148`. |
| Find, replace | Find bar: Next, Previous, match case, whole word, Replace, All, "n found". No regex, no wildcards, no formatting search. Searches the story the caret is in (body unless the caret is in a header, note or comment). Matching is on the paragraph's flat text. | `W/session.rs:576-640`; `C/overlays.rs:409-525`. |
| Format painter | Panel Text level and Ctrl+Shift+C, Ctrl+Shift+V. | `S/actions.rs` ids `copy-formatting`, `paste-formatting`; `View.copied_format` (`S/mod.rs`). |
| Clear formatting | Ctrl+Space and panel: removes direct run formatting and resets the paragraph to Normal. | `W/ops.rs:1323-1339`. |
| Change case | Upper, lower, sentence, title, toggle. | `W/ops.rs:1297`. |
| Insert table | Panel rows x columns; `/` menu (3 x 3). Style `Table Grid` when the document has it, else explicit borders. | `W/ops.rs:221`; `W/edit.rs:632`. |
| Table row, column, delete, merge, split | Panel "This table" buttons when the caret is in a table; Tab in the last cell adds a row. Merge right and merge down only. Shade cell (5 colours). Column width only by verb `table width`. | `C/panel.rs:571-650`; `C/keys.rs:259-267,511-542`. |
| Insert picture | From a file via dialog (png, jpg, gif, bmp). No clipboard. No resize or crop afterwards; no picture selection. | `S/actions.rs:107-114`; `W/media.rs`. |
| Page, column, section break | Ctrl+Enter, Ctrl+Shift+Enter, panel, `/`. Next page and continuous in the UI; even and odd by verb. | `C/keys.rs:194-201`; `S/commands.rs:498-503`. |
| Hyperlink | Ctrl+K opens the Insert level at the Link field; bubble button; field takes `https://...` or `#bookmark`. | `C/keys.rs:184-189`; `C/panel.rs:571-708`. |
| Bookmark | Insert level field (spaces become underscores). | `C/panel.rs:571-708`. |
| Footnote, endnote | Actions `footnote`, `endnote`; verb with text. After insertion the note text cannot be edited by pointer: a press may land only in the body or in a header or footer being edited (`C/page_view.rs:61-77`). | |
| Comment | Bubble, Review level; verb. | `W/ops.rs:770`. |
| TOC | Action `table-of-contents` (levels 1 to 3), `update-fields`. | `W/ops.rs:1121,1212`. |
| Field | `date` action; `page-number` action (a `PAGE` field with stored "1"); any code only by verb `insert field <code>`. | `S/commands.rs:522`. |
| Symbol | 24 buttons; verb types any text. | `C/panel.rs:658-668`. |
| Equation | None. | |
| Headers and footers | (1) Panel Page level "Header" and "Footer" text boxes replace the section's default header or footer with a single paragraph of text (`{PAGE}` and `{NUMPAGES}` become fields) (`W/ops.rs:638-704`, the story is replaced at lines 696-700), so a header holding a table, picture or several paragraphs loses them when set this way. (2) A double press in the top or bottom margin edits the header or footer story in place (`C/page_view.rs:61-77`; test `a_double_press_on_the_margin_edits_the_header`). First page and even variants: verbs `section header-first`, `header-even` and the footer forms; no UI toggle for "different first page". | |
| Styles | Apply from the dropdown or panel; create from the selection; no modify, no delete, no gallery with previews. | `C/panel.rs:446-548`. |
| Lists | Toggle bullet or number; demote and promote (Tab, Shift+Tab); restart at N. | `W/ops.rs:47-220`. |
| Word count | Panel Review table (pages, words, characters without and with spaces, paragraphs, lines); status line words and page; verb `stats`. | `C/panel.rs:821-911`; `W/session.rs:644`. |
| Go to page | None. | |
| Zoom | Ctrl+wheel, pinch, actions `zoom-in`, `zoom-out`, `zoom-100`, verb `view zoom N`; clamp 25% to 500%; default from settings. No fit width or page. | `C/page_view.rs:561-586`. |
| Print | None (PDF export only). | |
| Sort, drop cap, text box, shapes, equation, compare, mail merge | None. | |

---

## 5. Layout engine

Entry point `W/layout/mod.rs:382` `lay_out(&Document, &dyn FontMetrics, Options) -> Pages`. Drawn items are `Text`, `Fill`, `Rule` (solid, double, dotted, dashed, wave), `Picture`, `Placeholder` (`W/layout/mod.rs:73-99`).

| Topic | What the code does | Fidelity notes |
|---|---|---|
| Lines | Grapheme atoms; break opportunities after spaces, after hyphens and around ideographs; greedy; a word wider than the line is broken at a cluster. `W/layout/paragraph.rs:703-797` | Lines equal Word's, word for word, for Calibri, Times New Roman and Arial in `lines_break_where_word_breaks_them_in_installed_fonts` (`WT/pagination.rs:126-160`). The Aptos paragraph is skipped (line 148): Word lays Aptos out about 3 percent narrower than the file Office downloads (`WT/pagination.rs:121-124`; `C:/jason/dev/unluminous/CLAUDE.md` near line 7563). The template of every new document uses the theme minor font, so default documents are affected. |
| Kerning | GPOS pair kerning when `w:kern` asks for it at that size. | `W/kern.rs`; `W/layout/paragraph.rs:378-390`. |
| Justification | Extra space spread over the line's spaces; not on the last line or a line ended by a typed break. | `W/layout/paragraph.rs` place_line (799+). |
| Tabs | Custom left, centre, right, decimal with leaders; hanging indent as an implicit stop; default stops. Bar tabs skipped. | `W/layout/paragraph.rs:688-701`. |
| Line height | Font ascent + descent + gap; auto multiple, atLeast, exact. | `W/props.rs:272-281`. |
| Paragraph spacing | The larger of space after and space before, not the sum (measured against Word 16); dropped at the top of a page except page 1; contextual spacing for the same style. | `W/layout/mod.rs:667-715`. |
| Keep with next | `LaidParagraph.keep_next` is set (`W/layout/paragraph.rs:199`) and declared (`W/layout/mod.rs:475`) and read nowhere else (`rg keep_next W/layout` finds only those two lines). The page flow never uses it, so keep with next has no effect on pagination. Derived from the code; not executed. | `W/layout/mod.rs:667-715` |
| Keep lines together | The paragraph moves whole to the next column when it fits there. | `W/layout/mod.rs:689-696`. |
| Widow and orphan | Two lines at the foot and head of a column. | `W/layout/mod.rs:684-712`. |
| Page break before | Yes. | `W/layout/mod.rs:668-670`. |
| Columns | Equal or explicit; column break; no balancing at a continuous break. | `W/layout/mod.rs:572-621`. |
| Sections | next page, continuous (only when the page size is equal), even, odd, next column; page size per section. | `W/layout/mod.rs:572-606`. |
| Headers and footers | default, first, even per section with inheritance; `PAGE`, `NUMPAGES`, `SECTIONPAGES` per page. The body top is the top margin: a header taller than its space is not pushed into the body (`W/layout/mod.rs:538-570` uses `geometry.top`), against TDD 194-195. | `W/layout/mod.rs:917-977`. |
| Footnotes | Whole note at the foot of the page of its reference line; measured once at the first section's text width; a note that does not fit does not continue on the next page. | `W/layout/mod.rs:624-634,781-796,980-1038`. |
| Endnotes | After the last body content. | `W/layout/mod.rs:739-770`. |
| Tables | Section 3b. Rows are indivisible. | |
| Floating objects | Positioned, text not wrapped (section 3b). | |
| Hyphenation | None; a soft hyphen is a zero width break opportunity. | `W/layout/paragraph.rs:490-501`. |
| Line numbering, drop caps, frames | None. | |
| RTL, bidi, vertical text | None. | |
| Page number format | Fields honour their `\*` switch; `pgNumType fmt` is ignored. | `W/layout/fields.rs:187-205`. |
| Small caps, sub and superscript | Small caps at 0.8 of the size; raise 0.33 and lower 0.14 of the size. | `W/layout/paragraph.rs:100-103,1026-1035`. |
| Incremental layout | None. The whole document is laid out again whenever `Session.revision` changes. Cost on a large document: not determined. | `S/mod.rs` `Doc::lay_out`. |
| Pagination against Word | Test compares page count and the page each top level paragraph starts on for 11 fixtures; it passes when counts are equal and at least 80 percent of paragraphs start on Word's page, and it returns early without checking anything when Aptos is not installed. | `WT/pagination.rs:85-116`. |

---

## 6. UI

### 6a. Format bar (`C/format_bar.rs`)

Left to right: paragraph style dropdown (favourites Normal, Title, Subtitle, Heading 1 to 4, Quote, Intense Quote, List Paragraph, No Spacing, then every other style, 139-166); Bold, Italic, Underline (175-192); Bulleted list, Numbered list (200-214); Align left, Center, Right, Justify (216-229); `Aa` Format, which opens the root level (233-244); `+` Insert, which opens the root and then Insert (245-256); a magnifier "Find a command by name (Ctrl+/)" (257-277). Right side status text: optional "Tracking changes", optional "n spelling error(s)" (a click goes to the next one), "Page n of m", "N words", zoom percent (279-326). The status text is not drawn when less than about 260 points of room remain (301).

### 6b. Drill down panel (`C/panel.rs`)

Width 320 points (`C/mod.rs:22`). Back arrow, breadcrumb, close. The root (`C/panel.rs:82-118`) lists the nine levels with a summary line each (`S/mod.rs:127-135`). Depth is the root plus one level. Leaves per level:

- **Text** (137-256): Font combo of every installed family; Size drag (1 to 1638 pt), A- and A+; Bold, Italic, Underline, Strikethrough, Double strikethrough; underline kind combo (none, single, double, thick, dotted, dash, wave, words); Superscript, Subscript, Small caps, All caps, Hidden text; Colour swatches (Automatic, Black, Dark blue, Blue, Red, Orange, Gold, Green, Purple, Grey); Highlight None plus the first 12 of 16 colours (excludes darkGray, lightGray, black, white); Character spacing drag; Case (Sentence case, lowercase, UPPERCASE, Capitalize Each Word, tOGGLE cASE); Copy formatting, Paste formatting, Clear all formatting.
- **Paragraph** (279-445): Align left, Center, Right, Justify; Left, Right and First line (negative hangs) drags; Before and After drags; Line spacing 1.0, 1.15, 1.5, 2.0 and a rule combo (Multiple, At least, Exactly) with an amount; checkboxes Keep with next, Keep lines together, Page break before, Widow and orphan control; Box, Line below, No border, Shade (fixed F2F2F2), No shading; Tab stops: position field, Left, Center, Right, Decimal buttons, a list of the stops, Clear tab stops.
- **Styles** (446-548): a selectable list of every paragraph style; Built in buttons (Normal text, Title, Subtitle, Heading 1 to 4, Quote, Intense quote, No spacing); Character styles buttons; "New style from the selection" with a name field and Create.
- **Lists** (550-570): Bulleted list, Numbered list; Promote, Demote; "Start at" field with "Restart here"; a help line.
- **Insert** (571-708): Table rows x columns with Insert; inside a table: Row above, Row below, Column left, Column right, Delete row, Delete column, Merge right, Merge down, Split, Header row, Delete table, Shade cell with 5 swatches and None; Picture...; Page break, Column break, Line break, Section break (next page), Section break (continuous); Link field; Bookmark field; Symbol (24 buttons: © ® ™ § ¶ † ‡ • … — – € £ ¥ ° ± × ÷ ≠ ≤ ≥ → ← ✓); Today's date, Page number, Horizontal line.
- **Page** (709-804): a button for each of the 8 sizes and the current size in inches; Portrait, Landscape; Normal, Narrow, Wide margins and four margin drags (Top, Bottom, Left, Right); One, Two, Three columns; Header text and Set, Footer text and Set (default only); "Page numbers in the footer"; Page colour (5 swatches FFFFFF, FFF8E7, F2F2F2, EAF1FB, EEF7EE).
- **References** (805-820): Table of contents, Update fields; Footnote, Endnote; Caption; the bookmarks as links.
- **Review** (821-911): "Check spelling as you type" checkbox, error count, Next error (F7); Track changes checkbox, Accept change, Reject change, Accept all, Reject all; Comment text box with a Comment button and one card per comment (author, date, quoted text, Resolve or Reopen, Delete); the word count grid.
- **Document** (912-948): Title, Subject, Author, Keywords fields; Show formatting marks, Navigation pane, Zoom out, Zoom 100%, Zoom in; Find, Replace; Export as PDF, HTML, Markdown, plain text.

### 6c. Other UI parts

| Part | Status | Evidence |
|---|---|---|
| Right click menu | The spelling card for the word under the pointer; otherwise it moves the caret. No general menu. | `C/page_view.rs:619-627`; `C/spelling.rs:68-90` |
| Bubble | Bold, Italic, Underline, Highlight (yellow toggle), Link, Comment; shown above the first selected line. | `C/overlays.rs:307-407` |
| `/` command search | Opens when `/` is typed in an empty paragraph; filters the actions where `slash` is true; Up, Down, Enter, Tab, Esc. | `C/keys.rs:106-110`; `C/overlays.rs:17-83,263-292` |
| `Ctrl+/` search | All 100 actions; shows each action's level on the right; "No command has that name" when empty. | `C/overlays.rs:145-262` |
| "`...` menu" | Not found. There is no overflow menu. | |
| Status line | Inside the format bar (6a). Action results show as window messages (`Request::Message`). | `C/mod.rs:67-79` |
| Comments margin | Cards at the height of the anchor text with a line to it; the pages shift left to make room when the document has comments; resolved cards grey; a press on a card puts the caret at the comment and opens Review. | `C/page_view.rs:18-37,180-265` |
| Ruler | Not found. | |
| Navigation pane | Yes, 230 pt wide, headings of levels 1 to 9, click to go; no `Ctrl+Shift+O`. | `C/navigation.rs` |
| Views | Print Layout only. | |
| Zoom | Ctrl+wheel, pinch, buttons; 25% to 500%. | `C/page_view.rs:561-586` |
| Themes | The page is white in every window theme (`word_paper`, `C:/jason/dev/unluminous/crates/unluminous-app/src/theme/mod.rs:250-252`); the ground and panels follow the window theme; a page colour set in the document is drawn. No dark page mode. | |
| Find bar | Top right of the pages. | `C/overlays.rs:409-525` |
| Settings page | Author, zoom a document opens at, formatting marks default, check spelling. | `C/settings_page.rs` |
| Plugin menu "Word" | New Word Document; Export as PDF, HTML, Markdown; Track Changes, Accept All Changes, Reject All Changes; Spelling; Update Fields; Word Count. | `MANIFEST:22-23` |
| Header and footer edit mode | A dashed marker and label while editing. | `C/page_view.rs:151-178` |
| Formatting marks | Paragraph marks, spaces, hidden text. | `C/page_view.rs:507` |

### 6d. Keyboard chords handled (`C/keys.rs`)

`claims` (17-24): while a Word tab holds the keyboard, every Ctrl or Cmd chord except S, W, Q, O, N, Tab, P and Comma stays in the tab (Ctrl+Shift+N is claimed); without Ctrl only Tab, F7, F9 and Escape.

With Ctrl: B bold; I italic; U underline; E centre; L left; R right; J justify; 1 single spacing; 2 double; 5 1.5 lines; M indent more; Shift+M indent less; Z undo; Shift+Z and Y redo; A select all; F find; H replace; Space clear formatting; Shift+N Normal; Shift+. and ] larger font; Shift+, and [ smaller font; = subscript; Shift+= and + superscript; Shift+8 formatting marks; Shift+C copy formatting; Shift+V paste formatting; K link; / command search; Enter page break; Shift+Enter column break; Backspace and Delete by word; Home and End to the ends of the story; Left and Right by word; Up and Down by paragraph; Ctrl+Shift+Alt+1, 2, 3 Heading 1, 2, 3 (the code needs Shift together with Alt, `C/keys.rs:165-167`; Word's chord is Ctrl+Alt+1 to 3 and PRD 209 wrote "Alt+1 to 3"). Ctrl+C, X, V arrive as egui Copy, Cut and Paste events.

Without Ctrl: Shift+Enter line break; Enter (with the `---` habit); Backspace; Delete; Tab (list level, next table cell, or a tab); Esc (closes menus, find bar, panel, leaves a header); F7 spelling; F9 update fields; arrows, Home, End, PageUp, PageDown, each with Shift to extend.

PRD 209 chords with no binding: Ctrl+T, Ctrl+Shift+T, Ctrl+G, Ctrl+Alt+M, Ctrl+Shift+E. `Ctrl+Shift+O` from PRD 71 is not bound either.

### 6e. `ACTIONS` (`S/actions.rs:127`), 100 rows: id | label | panel level | in `/` menu

Text (none in the `/` menu): bold | Bold; italic | Italic; underline | Underline; strikethrough | Strikethrough; double-strikethrough | Double strikethrough; superscript | Superscript; subscript | Subscript; small-caps | Small caps; all-caps | All caps; hidden | Hidden text; grow-font | Larger text; shrink-font | Smaller text; clear-formatting | Clear all formatting; uppercase | UPPERCASE; lowercase | lowercase; sentence-case | Sentence case; title-case | Capitalize Each Word; toggle-case | tOGGLE cASE; copy-formatting | Copy formatting; paste-formatting | Paste formatting.

Paragraph (none in the `/` menu): align-left | Align left; align-center | Center; align-right | Align right; justify | Justify; indent-more | Increase indent; indent-less | Decrease indent; line-single | Line spacing 1.0; line-115 | Line spacing 1.15; line-15 | Line spacing 1.5; line-double | Line spacing 2.0; space-before | Add space before paragraph; no-space-before | Remove space before paragraph; space-after | Add space after paragraph; no-space-after | Remove space after paragraph; keep-with-next | Keep with next; keep-lines | Keep lines together; page-break-before | Page break before.

Styles (in the `/` menu except no-spacing): normal | Normal text; title | Title; subtitle | Subtitle; heading-1 | Heading 1; heading-2 | Heading 2; heading-3 | Heading 3; heading-4 | Heading 4; quote | Quote; intense-quote | Intense quote; no-spacing | No spacing.

Lists: bullets | Bulleted list (`/`); numbering | Numbered list (`/`); list-deeper | Demote list item; list-shallower | Promote list item; restart-numbering | Restart numbering at 1.

Insert: table | Table (`/`); picture | Picture... (`/`); page-break | Page break (`/`); column-break | Column break; line-break | Line break; section-next-page | Section break (next page) (`/`); section-continuous | Section break (continuous); horizontal-line | Horizontal line (`/`); date | Today's date (`/`); page-number | Page number.

Page (none in the `/` menu): letter | Letter paper; a4 | A4 paper; portrait | Portrait; landscape | Landscape; margins-normal | Normal margins; margins-narrow | Narrow margins; margins-wide | Wide margins; one-column | One column; two-columns | Two columns; three-columns | Three columns; page-numbers | Page numbers in the footer.

References: table-of-contents | Table of contents (`/`); footnote | Footnote (`/`); endnote | Endnote; update-fields | Update fields; caption | Caption (`/`).

Review: track-changes | Track changes; accept-change | Accept change; reject-change | Reject change; accept-all | Accept all changes; reject-all | Reject all changes; comment | Comment (`/`); spelling | Spelling (F7); check-spelling | Check spelling as you type.

Document (none in the `/` menu): formatting-marks | Show formatting marks; navigation | Navigation pane; zoom-in | Zoom in; zoom-out | Zoom out; zoom-100 | Zoom 100%; find | Find; replace | Replace; select-all | Select all; undo | Undo; redo | Redo; export-pdf | Export as PDF; export-html | Export as HTML; export-md | Export as Markdown; export-txt | Export as plain text.

The `/` menu therefore holds 21 rows: normal, title, subtitle, heading-1, heading-2, heading-3, heading-4, quote, intense-quote, bullets, numbering, table, picture, page-break, section-next-page, horizontal-line, date, table-of-contents, footnote, caption, comment.

No action exists for: hyperlink, bookmark, symbol, font colour or highlight (panel only), font family or size (panel only), tab stops (panel only), custom margins, header or footer first or even variants, table operations (panel and keys only), sort, go to page, fit width, equation.

---

## 7. Agent half: `plugins run word` (`CLI:239-275` `WORD`, handled in `S/commands.rs`)

Commands act on the current document, or the one chosen with `use` (`S/commands.rs:97`). A paragraph is numbered from 1 in document order, table cells included (`S/commands.rs:51`).

| Verb | Arguments | What it does | Evidence |
|---|---|---|---|
| `new` | `[path.docx]` | New blank document (untitled or at the path), opens a tab | `S/commands.rs:69` |
| `open` | `<path>` | Open a `.docx` in a tab | `:76` |
| `docs` | | List open documents | `:89` |
| `use` | `<key or path>` | Choose the document commands act on | `:97` |
| `save` | `[path]` | Save, or Save As with a path | `:117` |
| `close` | | Save then close the tab | `:112` |
| `read` | `[from] [to] [--runs]` | Paragraphs with style, list label, table cell, text, optionally runs | `:139`; `W/export.rs:384` |
| `text` | | The document as plain text (body only) | `:155` |
| `outline` | | Headings with level and page | `:159` |
| `stats` | | Pages, words, characters, paragraphs, lines | `:172` |
| `spelling` | none, `add <word>`, `ignore <word>`, `next` | Misspelled and repeated words with suggestions | `:169`; `S/spelling.rs` |
| `pages` | | Each page and what it starts with | `:180` |
| `select` | `<paragraph> [offset] [to] [to-offset]`, `all`, `text <words>` | Set the selection (body paragraphs only) | `:201`, `459-488` |
| `selection` | | The selection and its text | `:202` |
| `type` | `<text>` (`\n` splits a paragraph) | Type at the selection | `:206` |
| `insert` | `paragraph`, `page-break`, `line-break`, `column-break`, `section-break [continuous\|even\|odd]`, `table <rows> <cols>`, `picture <path> [width]`, `footnote <text>`, `endnote <text>`, `toc`, `field <code>`, `link <url> [text]` (alias `hyperlink`), `bookmark <name>`, `date`, `page-number`, `symbol <text>` | Insert at the selection | `:490-537` |
| `delete` | `[forward] [word]` | Delete the selection, a character or a word | `:212` |
| `find` | `<text> [--case] [--word]` | Matches as paragraph and offset | `:221` |
| `replace` | `<find> -- <with> [--all] [--case] [--word]` | Replace one or all | `:238` |
| `format` | `bold`, `italic`, `strike`, `double-strike`, `small-caps`, `caps`, `hidden` with `on` or `off`; `underline [kind\|off]`; `superscript`, `subscript`; `font <name>`; `size <pt>`; `color <RRGGBB>`; `highlight <name>`; `spacing`; `clear` | Run properties on the selection | `:540-630` |
| `paragraph` | `align <left\|center\|right\|justify>`; `indent-left`, `indent-right`, `first-line`, `hanging <pt>`; `before`, `after <pt>`; `line <multiple>`; `keep-next`, `keep-lines`, `page-break-before`, `widow-control` on or off; `shading` | Paragraph properties | `:631-730` |
| `style` | `<name>`, or `list [paragraph\|character\|table]` | Apply a style, or list them | `:262-281` |
| `list` | `bullet`, `number`, `deeper`, `shallower`, `restart [n]` | List control | `:282-302` |
| `table` | `row-above`, `row-below`, `column-left`, `column-right`, `delete-row`, `delete-column`, `delete-table`, `merge-right`, `merge-down`, `split`, `header-row`, `width <pt>`, `shade <RRGGBB>`, `cell <row> <col>` | Table operations at the caret cell | `:735-757` |
| `section` | `size <name>`, `orientation <landscape\|portrait>`, `margins <preset or top bottom left right>`, `columns <n>`, `header`, `footer`, `header-first`, `footer-first`, `header-even`, `footer-even` `<text>`, `page-numbers` | Section settings and header text | `:765-812` |
| `comment` | `add <text>`, `reply <id> <text>`, `resolve <id>`, `reopen <id>`, `delete <id>`, `list` | Comments | `:814-852` |
| `track` | `on`, `off`, `list`, `accept [all]`, `reject [all]` | Track Changes | `:306-337` |
| `fields` | | Update the TOC and the DATE, TIME, CREATEDATE, SAVEDATE fields | `:338` |
| `undo`, `redo` | | | `:344-345` |
| `export` | `pdf\|html\|md\|txt [path]` | Export | `:346` |
| `properties` | `[name] [value]` (title, subject, author, keywords, description) | Read or set | `:353` |
| `view` | `zoom <percent>`, `marks on\|off`, `navigation on\|off` | View state | `:377` |
| `panel` | `text`, `paragraph`, `styles`, `lists`, `insert`, `page`, `references`, `review`, `document`, `close` | Open the panel at a level | `:393` |
| `action` | `<id>` or `list` | Run any of the 100 actions by id | `:418` |
| `load` | `<key> <path>` | Used by the window only | `:63` |

Docs: `C:/jason/dev/unluminous/unluminous-cli/docs/commands.md`, `mcp.md` and `protocol.md` contain no Word specific text (`rg -i "docx|plugins run word|unluminous_word"` finds nothing); their `plugins` section is generic. The only prose for the verbs is the `note` string at `CLI:50-53` and the one line per verb in `WORD`. The design notes are in `C:/jason/dev/unluminous/CLAUDE.md` from line 7541.

What the UI can do that no verb can: select inside a header, footer, footnote or comment story (verb `select` takes body paragraph numbers, `S/commands.rs:459`); pointer selection, drag and Ctrl+wheel; the find bar's Next and Previous (the `find` verb lists matches). What a verb can do that the UI cannot: `insert field <any code>`; `section margins` with a preset `moderate`; `section header-first`, `header-even`, `footer-first`, `footer-even`; even and odd section breaks; `table width`; `paragraph shading` with any colour; `properties description`; `format spacing` in points. Neither can: edit a comment's text, edit a footnote's text after insertion, resize or crop a picture, modify an existing style, update `REF`, `SEQ` or `HYPERLINK` fields, read headers, footers, footnotes or endnotes as text (`read` and `text` are body only).

---

## 8. Spell check and review

| Feature | Status | Evidence |
|---|---|---|
| Spelling | Yes, as typed: red wavy underline, right click card with up to 6 suggestions, Ignore All, Add to Dictionary, Delete Repeated Word; F7 next error; status line count | `W/spell.rs`; `S/spelling.rs`; `C/spelling.rs`; tests `WT/spelling.rs`, `S/tests.rs` |
| Languages | English only: SCOWL en_US through `spellbook`. Runs marked `w:noProof` or with a `w:lang` that is not English are skipped (`W/spell.rs:116`). No other dictionary, no per document language choice. | `W/spell.rs:50,116`; `crates/unluminous-word/Cargo.toml` |
| Rules | Words in capitals, words with digits, and web, email and file addresses are ignored; a repeated word is underlined; field codes, deleted text and objects become spaces. | `W/spell.rs:90-186` |
| Custom dictionary | `words.dic` in the plugin's settings folder; Office's `%APPDATA%\Microsoft\UProof\CUSTOM.DIC` is read, never written. | TDD 380-383; `S/spelling.rs` |
| Stories checked | body, headers and footers, footnotes, endnotes, comments | `W/spell.rs:186-220` |
| Grammar | None. | `MANIFEST:13` |
| Thesaurus, translate | None. | `MANIFEST:13` |
| Read aloud | None (no code). | |
| Autocorrect | None (no smart quotes or replacement list). Only the Markdown habits (`C/keys.rs:120-144`). | |
| Language setting (proofing language) | None. | |
| Word count | Panel, status line, verb `stats`; test `counting_follows_words_rules` (`W/session.rs`). Counts body paragraphs only: notes, headers and text boxes are not counted. | `W/session.rs:644-671` |
| Track changes, comments | Section 3b. | |

---

## 9. Tests and oracle

### 9a. Counts (`#[test]` attributes)

| File | Tests | Ignored |
|---|---|---|
| `WT/oracle.rs` | 2 | 0 |
| `WT/pagination.rs` | 2 | 0 |
| `WT/probe.rs` | 4 | 4 |
| `WT/roundtrip.rs` | 2 | 0 |
| `WT/spelling.rs` | 5 | 0 |
| `WT/word_live.rs` | 2 | 0 |
| `crates/unluminous-app/tests/word.rs` | 9 | 0 |
| `S/tests.rs` | 15 | 0 |
| `S/actions.rs` (in file) | 2 | 0 |
| In file tests in `W/`: edit 11, ops 11, session 5, export 3, numbering 4, builtin 2, new 2, order 3, package 3, pdf 1, props 4, xml 5, zip 4, media 1, model 3, layout/fields 1, layout/section 2 | 65 | 0 |
| Total | 106 | 4 |

`W/layout/paragraph.rs`, `W/layout/table.rs`, `W/layout/mod.rs`, `W/styles.rs`, `W/read.rs`, `W/write.rs`, `W/spell.rs`, `W/kern.rs` and all of `C/` have no in file tests.

### 9b. Test names

- `WT/oracle.rs`: every_paragraphs_text_and_style_are_what_word_reports; every_paragraphs_indents_spacing_and_alignment_are_what_word_reports
- `WT/pagination.rs`: page_counts_and_breaks_are_close_to_words; lines_break_where_word_breaks_them_in_installed_fonts
- `WT/probe.rs` (all four `#[ignore]`, at lines 10, 81, 98, 130; run with `-- --ignored`): probe; kerning_probe; lines_probe; fonts_probe
- `WT/roundtrip.rs`: every_fixture_saved_unchanged_keeps_every_part_and_element; a_saved_fixture_reads_back_to_the_same_text
- `WT/spelling.rs`: unknown_and_repeated_words_are_underlined_and_the_rest_are_not; a_repeated_word_across_punctuation_is_not_a_repeat; suggestions_come_best_first_and_an_added_word_is_no_longer_underlined; a_run_marked_no_proof_or_in_another_language_is_not_checked; field_codes_and_deleted_text_are_not_checked
- `WT/word_live.rs`: a_document_using_every_feature_opens_in_word_as_written; every_fixture_saved_by_unluminous_opens_in_word_unrepaired
- `crates/unluminous-app/tests/word.rs`: a_document_word_made_opens_as_its_pages; typing_formats_with_words_chords_and_undoes; the_arrows_and_shift_select_and_backspace_deletes_the_selection; the_bubble_over_a_selection_formats_it; f7_opens_a_misspelled_words_card_and_a_suggestion_replaces_it; the_format_panel_drills_down_and_its_controls_change_the_document; the_slash_menu_and_the_command_search_insert_by_name; a_document_is_saved_with_ctrl_s_and_on_closing_and_reads_back; switched_off_a_docx_is_a_file_that_cannot_be_shown
- `S/tests.rs`: the_window_measures_text_at_words_sizes; the_window_breaks_lines_where_word_does; an_agent_reads_a_document_as_paragraphs; an_agent_types_formats_and_undoes; an_agent_builds_lists_tables_comments_and_page_settings_and_saves; every_action_runs_on_a_document; a_refusal_names_the_verbs_there_are; the_markdown_habits_make_headings_and_lists; office_fonts_are_found_once_they_are_added; the_long_fixture_has_words_page_count_with_the_windows_fonts; a_formatted_copy_pastes_with_its_formatting; a_double_press_on_the_margin_edits_the_header; an_agent_lists_spelling_errors_and_a_suggestion_replaces_one; a_suggestion_keeps_the_formatting_of_the_word_it_replaces; a_right_click_on_a_misspelled_word_opens_its_card
- `S/actions.rs`: every_id_is_unique_and_kebab_case; a_search_finds_by_any_word_and_puts_the_best_match_first
- In `W/` files. edit: typing_inserts_and_enter_splits; typing_inside_a_run_takes_that_runs_formatting; a_tab_in_typed_text_is_a_tab_run; deleting_across_paragraphs_joins_them; backspace_at_a_paragraph_start_joins_it_to_the_one_before; ctrl_backspace_takes_the_word; formatting_a_range_splits_runs_and_shares_the_new_properties; a_tracked_deletion_keeps_the_text_marked_and_rejecting_brings_it_back; a_tracked_insertion_is_wrapped_and_deleting_it_removes_it; a_copied_range_pastes_with_its_formatting_and_its_paragraphs; a_table_goes_after_the_paragraph_and_leaves_a_paragraph_after_it. ops: a_bullet_list_toggles_on_and_off_and_survives_a_save; tab_in_a_list_makes_the_item_deeper; a_table_is_inserted_and_rows_and_columns_are_added_and_merged; a_footnote_is_numbered_and_its_part_is_made; a_comment_is_added_replied_to_resolved_and_deleted; a_hyperlink_wraps_the_selection_and_keeps_its_address; page_settings_change_the_section_and_round_trip; a_footer_with_page_numbers_is_made_and_numbers_each_page; a_section_break_starts_a_new_page; a_table_of_contents_lists_the_headings_with_their_pages; case_changes_follow_words_rules. session: typing_is_one_undo_step_per_word; bold_with_nothing_selected_applies_to_what_is_typed_next; find_and_replace_all_is_one_step; counting_follows_words_rules; the_date_is_written_the_way_word_writes_it. numbering: every_format_word_offers; levels_count_and_restart_the_way_word_numbers_an_outline; a_start_override_restarts_the_shared_list_once; a_symbol_font_bullet_is_drawn_as_a_bullet. export: markdown_has_headings_lists_bold_and_escapes; html_and_text_are_whole_documents; base64_matches_the_standard. builtin: a_style_the_document_lacks_comes_with_the_styles_it_needs; a_new_list_numbers_its_paragraphs. new: a_blank_document_has_one_empty_paragraph_and_words_styles; a_property_set_is_written_and_read_back. order: children_added_at_the_end_are_put_in_the_schemas_place; an_extension_stays_after_the_element_it_followed; the_paragraph_marks_run_properties_are_sorted_inside_the_paragraph_properties. package: a_rels_part_names_the_part_it_belongs_to_and_back; targets_resolve_against_the_source_folder; a_new_relationship_id_cannot_be_one_word_makes. pdf: a_document_becomes_a_pdf_with_a_page_for_each_page. props: numbers_read_the_way_word_and_other_programs_write_them; indents_and_spacing_read_in_points; a_theme_shade_darkens_and_a_tint_lightens; on_off_reads_every_spelling_of_off. xml: a_foreign_prefix_for_the_main_namespace_is_read_as_w; an_unknown_namespace_is_declared_again_where_it_was_declared; spaces_in_text_are_kept_and_spaces_between_elements_are_not; attribute_values_are_escaped_so_they_read_back_the_same; utf16_and_a_byte_order_mark_are_read. zip: what_is_written_reads_back_in_the_same_order; a_stub_in_front_of_the_archive_still_reads; an_old_word_file_is_named_rather_than_called_damaged; a_cut_off_file_is_refused_rather_than_panicking. media: a_picture_becomes_a_part_a_relationship_and_an_inline_drawing. model: flat_text_gives_each_object_one_character; splitting_puts_a_boundary_at_the_offset_and_merging_takes_it_away; visible_text_leaves_field_codes_and_deletions_out. layout/fields: page_fields_follow_their_format_switch. layout/section: a_section_with_nothing_said_is_letter_with_inch_margins; two_equal_columns_share_the_width_with_the_gap.

### 9c. Oracle fixtures

`WT/fixtures/` holds 17 `.docx` files, not 15, each with a `.word.json` report beside it. Sixteen are built by `ORACLE/make-fixtures.ps1` (fixture bodies at the lines shown). `line-breaks.docx` and `line-breaks.word-lines.txt` come from a one-off script outside the repository (`WT/pagination.rs:118-120` names `_agent_output`'s `fonts-doc.ps1`).

| Fixture | What it covers | Made at |
|---|---|---|
| `blank` | An empty `Documents.Add` document; source of `W/templates/blank.docx`. | `make-fixtures.ps1:503` |
| `builtin-styles` | One paragraph per built in paragraph style (Normal, Title, Subtitle, Heading 1 to 9, Quote, Intense Quote, No Spacing, List Paragraph, Caption, TOC Heading, TOC 1 to 3, Footnote...), character styles Strong, Emphasis, Intense Emphasis, Subtle Emphasis, Hyperlink, Footnote Reference, and a Table Grid table; source of `W/templates/builtin-styles.docx`. | `:487` |
| `comments` | Four paragraphs, three comments, one reply, one resolved (`Done`). | `:401` |
| `fields` | TOC of 3 levels, headings 1 to 3, a bookmark, a DATE field with `\@`, an external hyperlink, an internal hyperlink, a SEQ caption. | `:428` |
| `formatting` | 38 character formats: bold, italic, 7 underline kinds, strike, double strike, super and subscript, small caps, all caps, hidden, 3 colours, 5 highlights, 5 fonts, 5 sizes, expanded and condensed spacing, one mixed run. | `:169` |
| `headers` | Different first page, odd and even headers and footers, "Page X of Y" fields, 30 paragraphs. | `:372` |
| `kitchen-sink` | A report with header and footer, title, subtitle, bold and italic runs, a footnote, bullets, a table with shaded header row, an inline picture, a numbered list, a hyperlink. | `:507` |
| `line-breaks` | One paragraph set in Calibri, Times New Roman, Arial and Aptos, with Word's own line breaks in `line-breaks.word-lines.txt`. | outside the repo |
| `lists` | Three level bullets, numbered, outline `1`, `1.1`, `1.1.1`, lower letter, roman, restart, continuation after an interruption, start at 5. | `:263` |
| `long` | A document of 12 or more pages: Title, chapters, Heading 1 and 2, a bullet list, a numbered list, a 5 row table with a header row. | `:456` |
| `notes` | Four footnotes, two endnotes. | `:388` |
| `paragraphs` | Alignments, indents, hanging indent, spacing, 8 line spacing rules, keep with next, keep together, tab stops with leaders, border and shading, page break before. | `:220` |
| `pictures` | Inline PNG, resized PNG, JPEG, a floating PNG with square wrapping. | `:331` |
| `revisions` | One tracked insertion, one tracked deletion, one tracked bold change (`rPrChange`). | `:412` |
| `sections` | Portrait Letter; landscape A4 with narrow margins; two columns with a column break. | `:351` |
| `styles` | A custom paragraph style and a custom character style; Title, Subtitle, Headings, Quote, Intense Quote, No Spacing samples. | `:249` |
| `tables` | Repeating header row, merged across and down, shading and fixed widths, a nested table, borders off, cell alignment and row height. | `:289` |

No fixture covers: text boxes, shapes, charts, SmartArt, equations, content controls (other than the TOC), RTL text, line numbering, page borders, drop caps, `.docm`, `.dotx`, document protection, a VML watermark, `w:customXml`, comments in headers, columns with separators, table styles with conditional formatting, footnote properties, or `compat` modes other than Word 16's.

### 9d. What `word_live` asserts

Both tests need Word installed. Without it each prints "Word is not installed, so this test checked nothing" and passes (`WT/word_live.rs:15-30`); `UNLUMINOUS_REQUIRE_WORD=1` turns that into a failure.

1. `a_document_using_every_feature_opens_in_word_as_written` (`:119-254`) builds a document with the Session API (Title and Heading 1 styles; bold, italic, colour, highlight, double underline; a justified, indented paragraph; a bullet list and a numbered list; a 3 x 3 table with a merge; a picture; a footnote; a comment with a reply, resolved; a hyperlink; a bookmark; a header and a footer with page fields; a next page section break to landscape; a TOC; a tracked insertion and deletion; a title property), writes it, has Word open it through `inspect.ps1`, and asserts: `opened == true`; the text of seven paragraphs; the styles of the title and the heading; list strings `1.` and `2.` and at least 5 list items; 1 table; 1 footnote with its text; 2 comments, the first by "Unluminous Test", one `done`; one hyperlink address; bookmark "Top"; 1 inline shape; 1 table of contents; at least 2 revisions; 2 sections, the second landscape; the header text; a footer text starting "Page "; the title property.
2. `every_fixture_saved_by_unluminous_opens_in_word_unrepaired` (`:256-311`): for each of the 17 fixtures, types "Edited by Unluminous. " at the start, saves, has Word open it, and asserts it opened; the counts of paragraphs, tables, footnotes, comments, inline pictures and sections equal Word's committed report of the original; and the first non empty paragraph starts with the typed text. It does not compare text, styles, formatting or layout of the rest.

### 9e. Other test facts

- `WT/oracle.rs` checks paragraph count (including table row end marks), text (case folded), style name, alignment, left and right and first line indents, space before and after (skipped for automatic spacing), keepWithNext and pageBreakBefore. Run formatting, list strings, tables, sections, headers, footnotes, comments and revisions are in the `.word.json` files and are compared by no test (`rg 'listString|"runs"|"tables"|"footnotes"|"comments"|"revisions"' WT/oracle.rs` finds nothing).
- `WT/roundtrip.rs:94-118` saves every fixture unchanged and compares every part's bytes and `document.xml` as canonical XML.
- `WT/pagination.rs:92-104` lists 11 fixtures: long, headers, kitchen-sink, paragraphs, styles, lists, notes, sections, tables, formatting, fields. Not in the page comparison: blank, builtin-styles, comments, pictures, revisions (line-breaks has its own test).
- `S/tests.rs` `the_long_fixture_has_words_page_count_with_the_windows_fonts` runs the window's fonts against `long.docx`.
- Window tests take pictures named `word_kitchen_sink`, `word_typed`, `word_panel_root`, `word_panel_page`, `word_panel_text`, `word_slash`, `word_command_search`, `word_spelling` (`C:/jason/dev/unluminous/CLAUDE.md` near line 7585).

---

## 10. Known gaps found in the code (requested keyword search)

Search run: `rg -n -i "TODO|FIXME|not yet|unsupported|not supported|ignored|kept as|later|deferred"` over `crates/unluminous-word/src`, `crates/unluminous-word/tests/*.rs`, `crates/unluminous-app/src/services/word`, `crates/unluminous-app/src/components/word` and `crates/unluminous-app/tests/word.rs`.

No `TODO`, `FIXME`, `not yet`, `unsupported`, `not supported` or `deferred` appears in these paths. Every hit:

| File:line | Hit | Meaning |
|---|---|---|
| `WT/probe.rs:2` | "`-- --ignored --nocapture`" | Run instruction for the four ignored probe tests. |
| `C/keys.rs:142-143` | `ignored` | A discarded request vector in the Markdown habit; not a gap. |
| `W/zip.rs:6` | "so one copy can later serve both" | The zip reader is duplicated with the spreadsheet crate and not yet shared. |
| `W/ops.rs:603,618` | `later` | A variable name (the section after). |
| `W/spell.rs:45,57,70,75` | `ignored` | The Ignore All set. |
| `W/props.rs:3`, `W/model.rs:4` | "kept as the El they were read as" | Design note. |
| `W/position.rs:74` | "the later one" | Selection end. |
| `W/package.rs:260` | "not yet used" | The part name generator. |

Gaps the code states in other words:

| File:line | Statement |
|---|---|
| `W/layout/mod.rs:5-6` | "each table row becomes one piece that is never split" |
| `W/layout/mod.rs:94` | `Placeholder`: "Something Unluminous does not draw, shown as a box with a word in it." |
| `W/layout/section.rs:41-45` | `page_number_format` and `vertical_center` are stored and never read afterwards. |
| `W/layout/mod.rs:630` | Footnote heights "Measured before the body was laid out, at the width of the text". |
| `W/pdf.rs:6-7` | A face in a font collection "is replaced by Helvetica". |
| `W/zip.rs:53,75` | zip64 archives and encrypted entries are refused. |
| `MANIFEST:13` | The product's own limitation list: `.doc`, encrypted, RTF and ODT not read; English only spelling; no grammar, thesaurus, translation; objects kept but not editable; RTL not shaped; page breaks may differ from Word's by a fraction of a point; macros never run. |

---

## 11. Promised in the PRD or TDD but not found in code

Cross check of section 1 against sections 2 to 9. "Not found" means a search of `W/`, `S/`, `C/` and `CLI` found no code.

| # | Promise (ref) | Finding |
|---|---|---|
| 1 | Right click menu with cut, copy, paste, style, table commands (PRD 68-69) | Not found. Right click opens only the spelling card (`C/page_view.rs:619`). |
| 2 | `Ctrl+Shift+O`, `Ctrl+G`, `Ctrl+T`, `Ctrl+Shift+T`, `Ctrl+Alt+M`, `Ctrl+Shift+E` (PRD 71, 209) | Not bound (`C/keys.rs:154-241`). The heading chords need an extra Shift. |
| 3 | A third panel level such as `Page -> Margins -> Custom` (PRD 53) | Not found; the panel is two deep. |
| 4 | `Fit Width`, `Fit Page` (PRD 203) | Not found. |
| 5 | Go to page (PRD 205) | Not found. |
| 6 | Sentence selection with Ctrl+click (PRD 208) | Not found. |
| 7 | Equation "drawn as its linear text" (PRD 160) | Not drawn; a zero width object. |
| 8 | Text box "drawn as the text box's text" (PRD 158); shape fallback picture (PRD 153) | Text box text never drawn; a shape is a grey box; a fallback is used only if the first `w:drawing` or `w:pict` found has a blip. |
| 9 | Floating pictures "with square and top and bottom wrapping" (PRD 151, TDD 191-193) | Placed, but lines do not avoid them. |
| 10 | A table row taller than a page may split (TDD 188-189) | Rows never split. |
| 11 | A tall header pushes the body down (TDD 194-195) | Not implemented. |
| 12 | A footnote that does not fit continues on the next page (TDD 197-198) | Not implemented. |
| 13 | Incremental layout, "one page or two" per keystroke (TDD 199-201) | Not implemented; full relayout per revision. |
| 14 | Keep with next (PRD 120, TDD 183) | Parsed and in the panel; ignored by the page flow. |
| 15 | F9 updates AUTHOR, TITLE, FILENAME, REF, HYPERLINK, TOC (PRD 159) | Only TOC, DATE, TIME, CREATEDATE, SAVEDATE. |
| 16 | Update of a TOC that Word wrote | Likely not working: `find_toc` and `toc_entries` skip paragraphs inside content controls and tables (`W/ops.rs:1190,1255`). Derived from code, not executed. |
| 17 | Remove a TOC (PRD 181) | Not found. |
| 18 | Modify a style (PRD 132) | Not found. |
| 19 | Picture from the clipboard; resize; crop (PRD 151-152) | Not found; `srcRect` is not applied. |
| 20 | HTML on the clipboard on copy; paste as plain text (PRD 207, TDD 226-229) | Plain text only. |
| 21 | Edit a comment (PRD 189) | Not found. |
| 22 | Formatting changes recorded as revisions (PRD 190) | Existing `rPrChange` resolves on accept or reject; none is recorded when typing under tracking. |
| 23 | Sort paragraphs (PRD 123) | Not found. |
| 24 | Mirrored margins (PRD 169) | Not found. |
| 25 | A missing font named in the panel (PRD 100) | Not found. |
| 26 | Character scale and raised or lowered position controls (PRD 103, 106) | Layout honours both; no control. |
| 27 | Highlight with Word's sixteen colours (PRD 105) | The panel shows 12. |
| 28 | Tab stop leaders and bar tab (PRD 121) | UI adds four kinds with no leader; bar tab parsed and not drawn. |
| 29 | `File -> New Word Document` and the explorer right click (PRD 86) | Only the Word plugin menu and the verb `new`. |
| 30 | Letter or A4 by locale, Calibri 11 (PRD 86) | Letter and 12 pt always. |
| 31 | A `.dotx` or `.docm` saved as `.docx` is a plain document (implied by PRD 84) | The main part's content type is not rewritten. Word's reaction: not determined. |
| 32 | Oracle comparison of runs' formatting, table cells, headers and footers, footnotes, comments, tracked changes (PRD 233-236, TDD 324) | `WT/oracle.rs` compares only the fields listed in 9e. |
| 33 | Layout unit tests with `FixedMetrics` for lines, tabs, justification, spacing, tables, footnotes, fields (TDD 334-337) | None in `W/layout/`. |
| 34 | Each Markdown habit is one undo step (PRD 65-66) | Two undo entries by the code; the test's content not checked. |
| 35 | Watermark drawn when it has a picture (PRD 173) | A WordArt or VML watermark in a header is a grey box; not drawn as a watermark. |
| 36 | Linked styles (PRD 133) | Not determined; no `w:link` handling. |
| 37 | Setting a header from the panel keeps what is in it | It replaces the header story with one paragraph (`W/ops.rs:696-700`). |
| 38 | Footnote and endnote text editable after insertion | Not reachable by pointer (`C/page_view.rs:61-77`). |
| 39 | Following a hyperlink | No click or Ctrl+click handler. |
| 40 | Number formats beyond decimal, roman, letters, ordinal, text (PRD 141 "and others") and picture bullets | Print as decimal / not handled. |

---

## Summary: the biggest gaps

1. Floating pictures are positioned but text does not wrap around them, so a document with a square or tight wrap picture lays its text under the picture.
2. Text boxes, shapes, charts, SmartArt and WordArt watermarks draw as a grey outlined box (or nothing). The plugin description and PRD say their text or fallback picture is drawn; text box text is never read.
3. Equations (`m:oMath`) take zero width and are invisible; the PRD promised linear text.
4. Table rows never split across pages, and table styles apply only borders, margins, cell shading and the style's own paragraph and run properties: no `basedOn` chain, no conditional bold for first row or column.
5. `keepNext` is parsed and shown in the panel but the page flow never uses it, so keep with next does not affect pagination.
6. A tall header does not push the body down, footnotes do not continue across pages, and layout is not incremental (the whole document is laid out per change).
7. The pagination test accepts up to one paragraph in five starting on a different page than Word's, and it checks nothing when Aptos is not installed.
8. Aptos, the default font of a new document, wraps about 3 percent differently from Word, and the line break test leaves it out.
9. The oracle tests compare only paragraph text, style, alignment, indents, spacing, keepNext and pageBreakBefore. Run formatting, list labels, tables, headers, footers, notes, comments and revisions are in the JSON reports and compared by no test.
10. No fixture covers text boxes, shapes, equations, content controls, RTL, `.docm`, `.dotx`, protected documents, or updating a Word made TOC.
11. TOC update and the headings list ignore anything inside a content control or table, which includes Word's own TOC (derived from the code, not executed). Removing a TOC is not possible.
12. Field updating covers the TOC and date fields only; AUTHOR, TITLE, FILENAME, REF, SEQ, PAGEREF, HYPERLINK, and PAGE in the body keep stored results.
13. There is no right click menu (cut, copy, paste, style, table), no third panel level, no `Ctrl+G`, `Ctrl+T`, `Ctrl+Shift+T`, `Ctrl+Alt+M`, `Ctrl+Shift+E`, `Ctrl+Shift+O`, no Fit Width or Fit Page, no ruler and no `...` menu.
14. The clipboard is plain text plus an in process formatted fragment: no HTML or RTF in or out, no picture paste, no paste as plain text command.
15. Pictures cannot be resized or cropped after insert (`srcRect` ignored), EMF, WMF and SVG do not draw, and footnote text cannot be edited by pointer after insertion.
16. Styles can be applied and created from a selection but not modified; linked styles are not followed; number formats beyond decimal, roman, letters, ordinal and text print as decimal; picture bullets are not handled.
17. Section properties pgBorders, lnNumType, vAlign, pgNumType format, text direction, mirror margins and column separators are kept or read but not drawn; RTL, hyphenation, line numbers, drop caps and frames are not drawn.
18. Setting a header or footer from the panel replaces the whole story with one paragraph, discarding tables or pictures that were in it.
19. Formatting changed under Track Changes is not recorded as a revision, and the tracking flag is not written back to `settings.xml`.
20. A `.dotx` or `.docm` saved as `.docx` keeps its original main part content type, no test opens either type, and new documents are always Letter at 12 pt rather than by locale at Calibri 11.
