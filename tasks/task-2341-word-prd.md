# task-2341: Word documents in Unluminous (PRD)

## The request

> We want unluminous to support creating and editing Microsoft word documents. Research online, write a
> prd, then a tdd, and fully implement. We want full major feature parity with word, and
> interoperability. We have excel installed locally that should be used as an oracle. We want extensive
> testing. We do not want to use the tool ribbon that word has, and should focus on creating our own,
> amazing UX that is minimal, but allows users to drill down to access features. We want this to be a
> plugin that can be disabled/uninstalled, so no additional resources are used, if someone chooses.

Microsoft Word 16 is installed beside Excel (`C:\Program Files\Microsoft Office\root\Office16\WINWORD.EXE`),
so Word itself is the oracle, driven through COM. `task-2338` is the Excel plugin, worked at the same
time; the two share the plugin tab contract (TDD §8) and the shape of the zip reader.

## Who uses it

- **A person** opens a `.docx` file from the explorer and sees its pages, as Word's Print Layout shows them:
  white sheets of the right size, margins, headers and footers, page numbers, tables, pictures and
  footnotes. They type into it, format it, and save it, and Word opens the result with no complaint.
- **An agent** does the same through `unluminous-cli plugins run word ...` and the MCP tool generated from
  it: reads the document as paragraphs with their styles, inserts and replaces text, applies formatting,
  builds tables and lists, adds comments, accepts tracked changes, and saves. Unluminous's own rule is that
  every control a person has, an agent has too, through the same code.

## Goals

1. A `.docx` written by Word opens in Unluminous and looks like it does in Word: the same text, styles,
   lists, tables, pictures, headers and footers, and close to the same page breaks.
2. A `.docx` written by Unluminous opens in Word with no repair prompt and with the same content.
3. A document opened and saved with no change keeps every part and every element Unluminous does not
   understand. Word content Unluminous cannot draw (SmartArt, charts, equations, content controls,
   VBA) survives a save untouched.
4. The everyday features of Word work: everything in the tables below marked Yes.
5. There is no ribbon. The page is the interface, and every feature is reached by drilling down from
   four small entry points (§ "The interface").
6. The whole feature is a plugin. Switched off, a `.docx` opens as "cannot be shown", no Word code runs,
   no fonts are measured, and no memory is held.

## The interface

Word's ribbon shows about 300 controls at once. Research on minimal writing tools (Google Docs' compact
mode, Notion, iA Writer, Bear, Dropbox Paper) settles on the same four patterns, and Unluminous uses all
four, so that a feature is never more than three presses from the page and the screen at rest shows only
the document:

1. **The format bar**: one thin row above the page with only what is used on most edits: the paragraph
   style (a dropdown showing `Normal`, `Heading 1`...), bold, italic, underline, a list toggle, alignment,
   and two drill-down buttons, `Aa` (format) and `+` (insert). On the right, the page and word count and
   the zoom. Nothing else.
2. **The drill-down panel**: `Aa` and `+` open one panel on the right of the page. It is a stack of
   levels with a back arrow and a breadcrumb, like a phone's settings. The first level is a short list:
   `Text`, `Paragraph`, `Styles`, `Lists`, `Page`, `Insert`, `References`, `Review`, `Document`. Each
   opens the controls for that area, and some of those open a third level (`Page -> Margins -> Custom`).
   Every control in Word's ribbon that Unluminous supports is in this panel somewhere.
3. **The selection bubble**: selecting text shows a small floating bar just above the selection with bold,
   italic, underline, highlight, link and comment. It disappears as soon as the selection does.
4. **Commands by name**: typing `/` at the start of an empty paragraph opens a menu of everything that can
   be inserted (heading, list, table, picture, page break, footnote, table of contents, comment...),
   filtered as you type. `Ctrl+/` anywhere opens the same search over every command in the panel, so
   "line spacing", "landscape" or "accept all" is found by name.

Markdown habits work too, because a person who writes in an editor all day types them without thinking:
`# ` at the start of a paragraph makes Heading 1 (`## ` Heading 2, and so on), `- ` or `* ` starts a
bullet list, `1. ` a numbered list, `> ` a quote, and `---` a horizontal line. Each is one undo step, so
`Ctrl+Z` straight afterwards gives back the characters typed.

The right click menu on the page holds cut, copy, paste, the paragraph's style, and the table commands
when the caret is in a table.

A navigation pane, opened from the panel's `Document` level or `Ctrl+Shift+O`, lists the headings, and a
click on one goes to it.

## What Word does, and what Unluminous will do

Each row is a Word feature, from Word's own Home, Insert, Layout, References, Review and View tabs and its
File menu. The last column is the decision.

### Files

| Word | Unluminous |
|---|---|
| Open `.docx` | Yes. |
| Open `.dotx` (template), `.docm` (with macros) | Yes. A template opens as a new untitled document made from it; a `.docm` keeps its macros byte for byte and never runs them. |
| Open `.doc` (Word 97-2003 binary), `.rtf`, `.odt` | No. A `.doc` opens as a tab that says the binary format is not read, and names `Save As .docx` in Word. RTF and ODT are other formats with their own readers. |
| New blank document | Yes: `File -> New Word Document`, the explorer's right click menu, and `plugins run word new`. Letter or A4 from the machine's locale, Calibri 11, Word's default styles. |
| Save, Save As, unsaved dot, ask before closing | Yes, the same as a text file. |
| AutoSave, version history | No. A file is saved when it is saved; git is Unluminous's history. |
| Document properties (title, author, subject, keywords) | Yes, in `Document -> Properties`. |
| Export as PDF | Yes, written by Unluminous with the fonts embedded. |
| Export as plain text, HTML, Markdown | Yes. |
| Print | No. The operating system prints the PDF. |
| Password protection, encryption, IRM | No. An encrypted file is an OLE compound file rather than a zip, and opens as a tab saying it is protected. |
| Restrict editing | No editing restriction is enforced; the setting is kept in the file. |

### Text (Home: Font)

| Word | Unluminous |
|---|---|
| Font family, size, grow and shrink | Yes, with every family installed on the machine. A family that is not installed is drawn in a fallback and named in the panel as missing. |
| Bold, italic, underline (single, double, thick, dotted, dashed, wavy, words only) | Yes. |
| Strikethrough, double strikethrough | Yes. |
| Subscript, superscript, raised and lowered position | Yes. |
| Small caps, all caps, hidden text | Yes. Hidden text is drawn only while formatting marks are shown. |
| Font colour, text highlight colour, shading | Yes. Highlight uses Word's sixteen colours. |
| Character spacing (expanded, condensed), scale | Yes. |
| Text effects (outline, shadow, glow, reflection) | No. Kept in the file and drawn as plain text. |
| Change case (sentence, lower, upper, capitalise each word, toggle) | Yes. |
| Clear formatting | Yes. |
| Format painter | Yes, as `Copy Formatting` and `Paste Formatting`. |

### Paragraph (Home: Paragraph)

| Word | Unluminous |
|---|---|
| Align left, centre, right, justify | Yes. |
| Left, right, first line and hanging indent, increase and decrease indent | Yes. |
| Spacing before and after, line spacing (single, 1.15, 1.5, double, at least, exactly, multiple) | Yes, with Word's own arithmetic, including "don't add space between paragraphs of the same style". |
| Keep with next, keep lines together, page break before, widow and orphan control | Yes. |
| Tab stops (left, centre, right, decimal, bar) with leaders | Yes. |
| Paragraph borders and shading | Yes. |
| Show formatting marks (¶) | Yes, `Ctrl+Shift+8`. |
| Sort paragraphs | Yes, as text, ascending or descending. |
| Right to left paragraphs | Kept in the file; drawn left to right. Shaping for Arabic and Hebrew is the text engine's limit. |

### Styles

| Word | Unluminous |
|---|---|
| Apply a paragraph or character style | Yes, from the format bar and the panel. |
| Word's built in styles (Normal, Title, Subtitle, Heading 1 to 9, Quote, Intense Quote, List Paragraph, Caption, No Spacing, Hyperlink, Footnote Text, TOC 1 to 9...) | Yes. A built in style a document does not define yet is added the first time it is used, with Word's own definition. |
| Style inheritance (based on), document defaults, linked styles | Yes. |
| Modify a style, create a style from the selection | Yes. |
| Style sets and themes (fonts and colours) | The theme's fonts and colours are read and used. Changing the theme is not offered. |

### Lists

| Word | Unluminous |
|---|---|
| Bullets, numbering, multilevel lists | Yes, read from and written to the numbering part. |
| Number formats (1, a, A, i, I, 01, ordinal and others), custom bullet characters and picture bullets | The first group yes; picture bullets drawn as the default bullet. |
| Restart numbering, continue numbering, set value | Yes. |
| Promote and demote with Tab and Shift+Tab | Yes. |

### Insert

| Word | Unluminous |
|---|---|
| Page break, column break, section break (next page, continuous, even, odd) | Yes. |
| Table (insert, from a grid), draw table | Insert yes; drawing a table with the pointer no. |
| Pictures from a file, from the clipboard | Yes, PNG, JPEG, GIF and BMP. Inline, and floating pictures are drawn where Word anchors them, with square and top and bottom wrapping. |
| Resize and crop a picture | Resize yes; crop is kept and drawn. |
| Shapes, icons, 3D models, SmartArt, charts, screenshots | No. Kept in the file, and a shape's fallback picture is drawn when the file has one. |
| Hyperlink (web address, place in this document) | Yes. |
| Bookmark, cross reference | Bookmarks yes; a cross reference is a field and shows its last result. |
| Comment | Yes (Review). |
| Header, footer, page number | Yes, with different first page and different odd and even pages. |
| Text box, WordArt, drop cap | Kept and drawn as the text box's text where the file stores it; not created. |
| Date and time, fields (PAGE, NUMPAGES, DATE, TIME, AUTHOR, TITLE, FILENAME, TOC, REF, HYPERLINK) | Yes, updated with `F9`. Other fields show their last result. |
| Equation | Kept, drawn as its linear text. |
| Symbol | Yes, any character, typed or from a picker. |
| Horizontal line | Yes, as a bottom border, which is what Word makes. |

### Layout

| Word | Unluminous |
|---|---|
| Margins (normal, narrow, moderate, wide, mirrored, custom) | Yes. |
| Orientation, page size | Yes, every size Word lists. |
| Columns (one, two, three, left, right, custom) | Yes, with column breaks. |
| Sections with their own page settings | Yes. |
| Line numbers, hyphenation | Kept; not drawn. |
| Page colour, watermark, page borders | Page colour yes; a watermark is a header shape and is drawn when it has a picture; page borders kept. |
| Indents and spacing on the Layout tab | The same controls as Paragraph. |
| Arrange (position, wrap text, bring forward) | Position and wrap for pictures, as above. |

### References

| Word | Unluminous |
|---|---|
| Table of contents (insert, update, remove) | Yes, built from the headings, with page numbers from Unluminous's layout and links to the headings. |
| Footnotes and endnotes | Yes, numbered automatically, drawn at the foot of the page or the end of the document. |
| Citations, bibliography, captions, table of figures, index | Captions yes (a `Caption` paragraph with a SEQ field). The others are fields and show their last result. |

### Review

| Word | Unluminous |
|---|---|
| Comments: new, reply, edit, delete, resolve, show | Yes, in a margin beside the page, with the author's name from the settings. |
| Track changes on and off, insertions and deletions shown | Yes. Formatting changes are kept and shown as formatted. |
| Accept and reject one change, all changes | Yes. |
| Compare and combine documents | No. |
| Word count (pages, words, characters, paragraphs, lines) | Yes, as Word counts. |
| Spelling | Yes, added after the first build was tried: misspelled and repeated words are underlined as they are typed, a right click or F7 opens the suggestions with Ignore All and Add to Dictionary, and the status line counts the errors. English only, with SCOWL's en_US Hunspell dictionary. Runs marked no proofing or another language are skipped, as are words in capitals, words with digits and addresses, which are Word's defaults. Words added in Word's own custom dictionary are known. |
| Grammar, thesaurus, translate, read aloud | No: each needs a service Unluminous does not ship. |

### View and editing

| Word | Unluminous |
|---|---|
| Print Layout | Yes, the only view: pages on a dark ground, one under another. |
| Read Mode, Web Layout, Outline, Draft | Outline is the navigation pane; the others no. |
| Zoom, one page, multiple pages, page width | Zoom with `Ctrl` and the wheel, and `Fit Width` and `Fit Page`. |
| Navigation pane (headings) | Yes. |
| Find, replace, go to page | Yes, with match case and whole words. |
| Undo, redo | Yes, many steps; typing is grouped by word. |
| Cut, copy, paste, paste as plain text | Yes. Between Unluminous documents formatting is kept; to other programs plain text and HTML go on the clipboard; pasting from another program pastes its text. |
| Selection: words, sentences (Ctrl+click), paragraphs (triple click), extend with Shift, select all, block selection | All but block selection. |
| Keyboard shortcuts | Word's own: Ctrl+B, I, U, E, L, R, J, 1, 2, 5, M, Shift+M, T, Shift+T, Space, Shift+N, Alt+1 to 3, Ctrl+Enter, Shift+Enter, Ctrl+Shift+Enter, Ctrl+K, Ctrl+F, Ctrl+H, Ctrl+G, Ctrl+Z, Ctrl+Y, Ctrl+A, Ctrl+Shift+> and <, Ctrl+[ and ], Ctrl+= and Ctrl+Shift+=, Ctrl+Shift+8, Ctrl+Shift+C and V, Ctrl+Alt+M, Ctrl+Shift+E, F9. |
| Macros | No. A `.docm` keeps its project. |

## The agent's half

`unluminous-cli plugins run word <verb>` reaches every feature above through the same code the panel
uses. The verbs are listed in the TDD §9; the MCP tool `unluminous_word` is generated from them. An agent
reads a document as paragraphs (text, style, list level, the table it is in) rather than as XML, and
addresses a paragraph by its number and a range by paragraph and character.

## Switching it off

`Settings -> Plugins -> Word` switches the plugin off, and uninstalling a bundled plugin is the same
switch. Off means:

- `.docx`, `.dotx` and `.docm` open as "cannot be shown", with a sentence naming the plugin.
- No Word code runs at startup. A document is read only when its tab is first drawn, and the provider is
  built only when a document is opened.
- Closing the last document tab drops every document, its layout and its undo history.

## Interoperability, and how it is tested

Word is the oracle. Tests drive it through COM (TDD §10):

1. Documents Word wrote, covering every feature row marked Yes, are read by Unluminous and compared with
   what Word reports about them: the text of every paragraph, its style, list label, alignment and
   indents, the runs' formatting, the tables' cells, the headers and footers, footnotes, comments and
   tracked changes.
2. Documents Unluminous wrote are opened by Word, which must not offer a repair, and the same report is
   compared with Unluminous's model.
3. A Word document opened and saved by Unluminous with no change is compared part by part with the
   original: every part present, every element Unluminous does not model still there.
4. Page counts and where each page starts are compared with Word's own pagination for the fixtures.
5. The window tests drive the plugin the way a person does and the way an agent does, and take pictures.

## Out of scope, said once

Grammar, collaboration and co-authoring, cloud storage, `.doc` reading, encryption,
macros running, drawing shapes, SmartArt, charts, equation editing, mail merge, compare, and Read Mode.
Each is said in the plugin's `plugin.limitations` so the Plugins page tells a person before they find out.
