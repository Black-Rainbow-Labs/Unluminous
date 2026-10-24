# Word plugin parity gap matrix

Unluminous Word plugin at commit 5b719826 (release 1.14.2) against the Microsoft Word 365 feature inventory. Inputs: `implemented-inventory.md` (called A) and `word-feature-inventory.md` (called B), both in this folder. Nothing was run. Every status comes from reading code. Where A said "not determined" or A and B disagreed, the code under `C:/jason/dev/unluminous` was read and the result is in Part 5.

## Reading key

Path shorthand in the Evidence column (same as A):

- `W/` = `C:/jason/dev/unluminous/crates/unluminous-word/src/`, `WT/` = `.../crates/unluminous-word/tests/`
- `S/` = `.../crates/unluminous-app/src/services/word/`, `C/` = `.../crates/unluminous-app/src/components/word/`
- `PRD n` / `TDD n` = line n of `tasks/task-2341-word-prd.md` / `tasks/task-2341-word-tdd.md`
- `A§n` = section n of A (`A§11#5` is row 5 of A's section 11). `B§n` = section n of B.

Status words:

- DONE: works end to end (model, UI, agent verb where one applies).
- PARTIAL: exists, with the limitation named in the Note.
- PRESERVED: not modelled or not drawn, the XML round trips untouched, so Word still sees it.
- MISSING: nothing in the code.
- NOT-DETERMINED: A could not tell and reading did not settle it.
- OUT-OF-SCOPE: needs a Microsoft or other online service (co authoring, OneDrive, Copilot, Dictate, online pictures, Researcher, online translation, Read Aloud, 3D models, ink). The reason is in the Note.

A row ending in `(N rows)` stands for N rows of B that share a tier and a status. Only niche rows are grouped this way, except one group of 13 common rows in section 13. Every other core and common row has one line. `MANIFEST` = `C:/jason/dev/unluminous/crates/unluminous-app/plugins/word/plugin.conf`, `ORACLE/` = `C:/jason/dev/unluminous/tools/word-oracle/`.

Sections 19 (collaboration) and 21 (shortcuts) follow the brief: 19 is skipped (every row is service bound), 21 is the table of differences at the end of Part 1, after section 22.

---

# Part 1. Feature matrix

### 1. File

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| New blank document | core | DONE | S/commands.rs:69; W/new.rs:10-20 | Plugin menu "New Word Document" and verb `new`; always Letter and 12 pt, no File menu entry (A§2). |
| New from template | core | PARTIAL | S/mod.rs:377-387 | A `.dotx` opens as an untitled document; there is no template gallery. |
| Normal.dotm | common | MISSING | W/new.rs:10-20 | New documents come from an embedded `blank.docx`; there is no user template. |
| Templates .dotx | common | PARTIAL | S/mod.rs:377-387; W/package.rs:208-215 | Opens; Save As writes `.docx` only and the main part keeps the template content type (A§2). |
| Open .docx | core | DONE | W/read.rs:14-56; S/mod.rs | Opens through `_rels/.rels`; 17 fixtures. |
| Open .doc | common | MISSING | W/zip.rs:84-91 | Refused with a message that names the format. |
| Open .rtf | core | MISSING | MANIFEST:13 | Not read. |
| Open .odt | common | MISSING | MANIFEST:13 | Not read. |
| Open .txt | core | MISSING | A§2 | Opens in the ordinary text editor, not in the Word plugin. |
| Open .htm, .html, .mht | common | MISSING | A§2 | Opens in the ordinary text editor, not in the Word plugin. |
| Open PDF (PDF Reflow) | common | MISSING | A§2 | No reader. |
| Protected View | common | MISSING | A§2 | Files from the internet open editable. |
| Recent documents and pinned files | core | NOT-DETERMINED | A§2 | Application level; A did not read the editor's file history. |
| Save | core | DONE | S/mod.rs:304-340 | Writes `lastModifiedBy`, `modified`, and `creator` when empty. |
| Save As | core | DONE | C:/jason/dev/unluminous/crates/unluminous-app/src/app/menus.rs:516-545 | Dialog offers `.docx` only; verb `save <path>`. |
| Save as .docx, .docm, .dotx, .dotm, .doc, .rtf, .odt, .txt, .htm, .xml | core | PARTIAL | menus.rs:516-545; S/commands.rs:928-960 | `.docx` only, plus exports to pdf, html, md, txt. |
| Save as Plain Text options | common | PARTIAL | W/export.rs:221 | Text export has no encoding or line ending choices. |
| Export to PDF | core | PARTIAL | W/pdf.rs:1-60 | No outline, no tags, no link annotations, no page range; font collection faces become Helvetica. |
| Tagged PDF | common | MISSING | W/pdf.rs:1-60 | No structure tags. |
| Change File Type | common | MISSING | A§2 | Exports only. |
| Print | core | MISSING | A§4b | PDF export is the only output. |
| Print ranges | core | MISSING | A§4b | No print. |
| Print only odd or even pages | common | MISSING | A§4b | No print. |
| Scale to paper size and pages per sheet | common | MISSING | A§4b | No print. |
| Print preview | core | MISSING | A§4b | The page view is the only view. |
| Mirror margins | common | PRESERVED | W/layout/section.rs:51-144 | `mirrorMargins` in settings and gutter are never read for placement (A§3b). |
| AutoSave | common | MISSING | PRD 88 | PRD lists it as not done. |
| AutoRecover | common | NOT-DETERMINED | A§2 | Application level; not read. |
| Document Recovery pane | common | NOT-DETERMINED | A§2 | Application level; not read. |
| Info panel | common | PARTIAL | C/panel.rs:912-947 | Document level shows properties only; no protect, inspect or versions. |
| Properties: Summary | common | PARTIAL | C/panel.rs:912-947; S/commands.rs:353-376 | Title, subject, author, keywords, and description by verb; no manager, company, category, comments. |
| Properties: Statistics and Contents | common | PARTIAL | W/session.rs:644-671; C/panel.rs:821-911 | Live counts shown; created, revision and editing time not shown. |
| Inspect Document | common | MISSING | A§2 | No inspector. |
| Check Accessibility | common | MISSING | A§2 | No checker. |
| Encrypt with Password | common | MISSING | W/zip.rs:73-75,84-91 | Password protected files are refused at open. |
| Password to modify | common | MISSING | A§2 | `writeProtection` is not read or written by any command. |
| Restrict Editing | common | PRESERVED | A§2 | `documentProtection` in `settings.xml` survives and is not enforced. |
| Share | common | OUT-OF-SCOPE | B§19 | Needs a cloud share service. |
| Close and Exit | core | DONE | S/mod.rs:304-340 | Saves on close; verb `close`. |
| Options dialog | core | PARTIAL | C/settings_page.rs | Settings page has author, zoom, marks default, spelling; no Language, Save or Trust sections. |
| Customize ribbon and Quick Access Toolbar | common | MISSING | PRD 28-38 | No ribbon by design. |
| Default file format and location | common | MISSING | C/settings_page.rs | No such setting. |
| Templates .dotm; Open .docm (2 rows) | niche | PARTIAL | W/write.rs:16-39 | `.docm` opens and its VBA parts survive a save; macros never run. |
| Attach template; Properties: Custom; Remove personal information on save; Mark as Final; Add a Digital Signature; Restrict Access (IRM); Compatibility Mode; Embed fonts in file (8 rows) | niche | PRESERVED | W/write.rs:16-39 | Parts and settings round trip; none is acted on (embedded fonts are not de obfuscated or used for layout). |
| Open .dot; Open .xml; Open .wps; Open .wpd; Open and Repair; Export to XPS; Save as Strict Open XML Document; Print what; Print options; Booklet (Book fold); Always create backup copy; Manage Document; Convert to current mode; Check Compatibility; Trust Center; Update links on open (16 rows) | niche | MISSING | A§2 | No code. |
| Save as Web Page, Filtered | niche | PARTIAL | W/export.rs:114 | HTML export exists, body only, pictures inline as base64. |
| Version History | niche | OUT-OF-SCOPE | B§19 | Server side versions of cloud files. |

### 2. Edit and selection

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Cut, Copy, Paste | core | DONE | C/keys.rs:33-78 | Plain text on the system clipboard. |
| Paste: Keep Source Formatting | core | PARTIAL | C/keys.rs:57-78 | Works only when the clipboard text equals what a Word tab last copied in this process. |
| Paste: Merge Formatting | core | MISSING | C/keys.rs:40-48 | No such paste mode. |
| Paste: Keep Text Only | core | PARTIAL | C/keys.rs:40-48 | Any other paste is typed as text with the caret's formatting; no explicit command or paste options button. |
| Paste: Use Destination Styles | common | MISSING | C/keys.rs:40-48 | No style mapping. |
| Paste: Picture, Link and Keep Source, Link and Merge | common | MISSING | A§4b | No picture or link paste. |
| Paste Special dialog | common | MISSING | A§4b | No HTML or RTF on the clipboard in or out. |
| Drag and drop text | core | MISSING | C/page_view.rs:648-661 | A drag only extends the selection. |
| Smart cut and paste | common | MISSING | C/keys.rs:33-48 | No space adjustment. |
| Use Ctrl+Click to follow hyperlink | common | MISSING | A§3b | No click handler on links. |
| Overtype mode | common | MISSING | C/keys.rs | No Insert key handling. |
| Undo, Redo, Repeat | core | PARTIAL | W/session.rs:73-75,158-177 | 500 steps, Ctrl+Z and Ctrl+Y; no F4 repeat and no undo list. |
| Selection by mouse | core | PARTIAL | C/page_view.rs:628-697 | Double click word, triple click paragraph, Shift click; no Ctrl+click sentence, no left margin click selection. |
| Selection by keyboard | core | DONE | C/keys.rs:210-233,286-300 | Shift and Ctrl+Shift arrows, Home, End, PageUp, PageDown, Ctrl+A. |
| Column (block) selection | common | MISSING | PRD 208 | Not done. |
| Multiple non contiguous ranges | common | MISSING | W/position.rs | Selection is one range. |
| Select Text with Similar Formatting | common | MISSING | A§4b | No command. |
| Find (Navigation pane) | core | PARTIAL | C/overlays.rs:409-525 | Find bar with count and Next, Previous; no results list in a pane. |
| Navigation pane tabs | core | PARTIAL | C/navigation.rs:1-46 | Headings only; no Pages or Results tab. |
| Advanced Find | core | PARTIAL | W/session.rs:576-640 | Match case and whole words only. |
| Find options | common | PARTIAL | W/session.rs:576-640 | Two of nine options. |
| Wildcards | common | MISSING | W/session.rs:576-640 | No regex or wildcard. |
| Find Format | common | MISSING | W/session.rs:576-640 | Text only. |
| Find Special | common | MISSING | W/session.rs:576-640 | No `^p`, `^t` and the rest. |
| Replace and Replace All | core | DONE | W/session.rs:615; test `find_and_replace_all_is_one_step` | One undo step for Replace All. |
| Replace with formatting | common | MISSING | W/session.rs:615 | Text only. |
| Search direction | common | PARTIAL | C/overlays.rs:409-525 | Next and Previous; no Up, Down, All choice. |
| Go To | common | MISSING | A§11#5 | Ctrl+G and go to page not found. |
| Copy and paste formatting | common | DONE | C/keys.rs:182-183; S/actions.rs | Ctrl+Shift+C and Ctrl+Shift+V (Word binds Ctrl+Shift+V to paste text only, B§21). |
| Format Painter | core | PARTIAL | S/actions.rs `copy-formatting` | Panel buttons and chords; no toolbar brush with single and double click modes. |
| Status bar | common | PARTIAL | C/format_bar.rs:279-326 | Page, words, zoom as text, tracking, spelling count; no language, view buttons or slider. |
| Mini toolbar | common | DONE | C/overlays.rs:307-407 | Bold, italic, underline, highlight (yellow only), link, comment. |
| Set Default Paste; Office Clipboard; Smart paragraph selection; Click and Type; Extend mode F8; Browse Object; Spike (7 rows) | niche | MISSING | A§4b | No code. |

### 3. Fonts and character formatting

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Font name | core | PARTIAL | C/panel.rs:137-256 | Combo of installed families; no preview, recent list or theme fonts (+Headings, +Body). |
| Font size | core | DONE | C/panel.rs:137-256 | Drag from 1 to 1638 pt. |
| Grow Font, Shrink Font | core | DONE | S/actions.rs:91-106 | Steps through Word's size list; Ctrl+Shift+. and Ctrl+Shift+,. |
| Increase and decrease 1 pt | common | PARTIAL | C/keys.rs:177-178 | Ctrl+] and Ctrl+[ run the size list step, not a 1 pt step. |
| Font style | core | DONE | C/format_bar.rs:175-192 | Bold and italic. |
| Font color | core | PARTIAL | C/panel.rs:137-256 | 10 swatches and verb `color RRGGBB`; no theme palette, custom colour picker or gradient. |
| Underline style | core | PARTIAL | W/props.rs:187-239; C/panel.rs:137-256 | 8 names in the UI; heavy and dashed variants are folded when read. |
| Underline color | common | PRESERVED | W/props.rs:187-239 | `w:u` colour attribute stays in the run properties; not drawn. |
| Strikethrough | core | DONE | C/panel.rs:137-256 | Panel, action, verb. |
| Double strikethrough | common | DONE | S/actions.rs | Action and verb. |
| Superscript | core | DONE | C/keys.rs:180 | Ctrl+Shift+=. |
| Subscript | core | DONE | C/keys.rs:179 | Ctrl+=. |
| Small caps | common | DONE | W/layout/paragraph.rs:100-103 | Drawn at 0.8 of the size. |
| All caps | common | DONE | S/actions.rs | Action and verb. |
| Hidden | common | DONE | C/page_view.rs:507 | Shown when formatting marks are on. |
| Character spacing: Scale | common | PARTIAL | W/props.rs:557-565 | Layout honours `w:w`; no control. |
| Character spacing: Spacing | common | DONE | C/panel.rs:137-256 | Drag control. |
| Character spacing: Position | common | PARTIAL | W/props.rs:557-565 | Layout honours `w:position`; no control. |
| Kerning for fonts | common | PARTIAL | W/kern.rs; W/layout/paragraph.rs:378-390 | Pair kerning applied when `w:kern` asks; no control. |
| Text Highlight Color | core | PARTIAL | C/panel.rs:224 | 12 of the 15 colours Word offers in the UI (no dark gray, light gray or black); the verb takes any of the 16 names. |
| Character shading | common | PARTIAL | W/props.rs:187-239 | Drawn; no control. |
| Character border | common | PRESERVED | A§3b | `w:bdr` kept, not drawn. |
| Change Case | core | DONE | W/ops.rs:1297; C/panel.rs | Five cases in the panel; no Shift+F3. |
| Clear All Formatting | core | DONE | W/ops.rs:1323-1339 | Ctrl+Space and panel. |
| Clear formatting shortcuts | common | PARTIAL | C/keys.rs:175,22 | Ctrl+Space bound; Ctrl+Q is claimed by the window (quit), so no paragraph reset chord. |
| Font dialog (Ctrl+D) | core | PARTIAL | C/panel.rs:137-256 | Panel Text level covers most of it; no dialog, no Set As Default. |
| Language per run | common | PRESERVED | W/spell.rs:116 | `w:lang` and `w:noProof` are honoured by spelling; nothing sets them. |
| East Asian font | common | PRESERVED | A§3b | `rFonts` eastAsia attribute kept. |
| Complex script font | common | PRESERVED | A§3b | `w:cs`, `szCs` kept, not applied. |
| Insert Symbol dialog | core | PARTIAL | C/panel.rs:658-668 | 24 fixed buttons; verb types any text; no picker. |
| Special Characters tab | common | PARTIAL | C/panel.rs:658-668 | Dashes, ellipsis, copyright and the like; no nonbreaking space, hyphen or em space. |
| Insert Emoji | common | NOT-DETERMINED | C/keys.rs:31-32 | Text from the OS emoji panel arrives as a text event; colour glyph drawing not read. |
| Formatting marks | core | PARTIAL | C/page_view.rs:507; C/keys.rs:181 | Paragraph marks, spaces, hidden text; no tab arrows, optional hyphens or object anchors. |
| Emboss, Engrave; Outline; Shadow; Equalize character height; OpenType Ligatures, Number spacing, Number forms, Stylistic sets, Contextual Alternates; Text Effects; Text Fill and Text Outline; Emphasis mark; Phonetic Guide; Enclose Characters; Combine Characters (15 rows) | niche | PRESERVED | A§3b | Run properties are kept in the tree and not drawn; `w:ruby` text is not shown at all (see Part 2). |
| Font substitution; Unicode entry (Alt+X); Alt codes; Assign shortcut to symbol; Text boundaries; Reveal Formatting (6 rows) | niche | MISSING | A§6d | No code. |
| Cloud fonts | niche | DONE | S/fonts.rs; S/tests.rs `office_fonts_are_found_once_they_are_added` | Office cloud font folders are added when a document opens. |

### 4. Paragraph

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Alignment: Left, Centered, Right, Justified | core | DONE | C/format_bar.rs:216-229 | Buttons, Ctrl+E, L, R, J. |
| Outline level | common | PARTIAL | W/props.rs:343-496 | Read for the TOC and navigation pane; nothing sets it. |
| Indentation: Left, Right | core | DONE | C/panel.rs:279-445 | Drag controls; Ctrl+M. |
| Special indent: First line, Hanging | core | DONE | C/panel.rs:279-445 | Negative first line hangs. |
| Spacing Before and After | core | DONE | C/panel.rs:312-318 | Points; auto spacing honoured in layout, no UI for lines or Auto. |
| Don't add space between paragraphs of the same style | common | PARTIAL | W/layout/mod.rs:671-676 | Applied in layout; no control. |
| Line spacing: Single, 1.5 lines, Double | core | DONE | C/keys.rs:162-164 | Ctrl+1, 5, 2. |
| Line spacing: At least | common | DONE | C/panel.rs:348 | Rule combo. |
| Line spacing: Exactly | common | DONE | C/panel.rs:348 | Rule combo. |
| Line spacing: Multiple | core | DONE | C/panel.rs:348 | Any multiple. |
| Widow/Orphan control | core | DONE | W/layout/mod.rs:684-712 | Two lines at foot and head. |
| Keep with next | core | PARTIAL | W/layout/paragraph.rs:199; W/layout/mod.rs:475 | UI toggle and model exist; the page flow never reads `keep_next`, so pagination ignores it. |
| Keep lines together | core | DONE | W/layout/mod.rs:689-696 | Moves the whole paragraph when it fits on a fresh column. |
| Page break before | core | DONE | W/layout/mod.rs:668-670 | |
| Don't hyphenate | common | PRESERVED | A§3b | `suppressAutoHyphens` kept; there is no hyphenation. |
| Text direction: LTR and RTL paragraph | common | PRESERVED | W/props.rs:371,449 | `bidi` is parsed and never read by layout. |
| Tabs dialog: stop position | core | PARTIAL | C/panel.rs:410-445 | Add at a position; default tab stop not editable. |
| Tab alignment | common | PARTIAL | W/layout/paragraph.rs:688-701 | Left, centre, right, decimal; bar parsed and skipped. |
| Tab leaders | common | PARTIAL | W/props.rs:458-483 | Drawn (dot, hyphen, underscore, middle dot); UI cannot choose a leader. |
| Clear and Clear All tabs | common | PARTIAL | C/panel.rs:410-445 | Clear all only. |
| Ruler tab selector | core | MISSING | A§6c | No ruler. |
| Borders and Shading: Borders | common | PARTIAL | C/panel.rs:360-409 | Box, Line below, No border; single and double only; no colour or width. |
| Borders and Shading: Shading | common | PARTIAL | C/panel.rs:360-409 | One fixed shade F2F2F2; the verb takes any colour. |
| Horizontal line | common | DONE | C/keys.rs:246-254; S/actions.rs | Bottom border; also `---` then Enter. |
| Borders button gallery | common | PARTIAL | C/panel.rs:360-409 | 3 of about 13 entries. |
| Merged borders | common | NOT-DETERMINED | W/props.rs:343-496 | `between` is parsed; whether adjacent paragraphs share one box was not read in layout. |
| Drop cap | common | PRESERVED | A§3b | `framePr` kept, not drawn (PRD 158). |
| Show/Hide paragraph marks | core | DONE | C/keys.rs:181 | Ctrl+Shift+8. |
| Sort | common | MISSING | PRD 123 | Not found. |
| Bullets, Numbering, Multilevel List buttons | core | PARTIAL | C/format_bar.rs:200-214 | Bullet and numbered buttons; no multilevel gallery. |
| Increase and Decrease Indent buttons | core | DONE | S/actions.rs; C/keys.rs:168-169 | Actions and Ctrl+M, Ctrl+Shift+M. |
| Line and Paragraph Spacing menu | core | DONE | S/actions.rs | Actions for 1.0, 1.15, 1.5, 2.0 and add or remove space before and after. |
| Paragraph mark formatting | common | PRESERVED | W/order.rs | Paragraph mark `rPr` is kept and sorted; no control. |
| Alignment: Distributed; Thai Distributed, Justify High, Medium, Low (2 rows) | niche | PARTIAL | W/props.rs:343-496 | Variants are drawn as justified. |
| Mirror indents; Automatically adjust right indent; Snap to grid; Suppress line numbers; Textbox tight wrap; East Asian typography options; Vertical text direction; Frame; Document grid (9 rows) | niche | PRESERVED | A§3b | Kept in the tree, not acted on. |

### 5. Styles and themes

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Paragraph style | core | DONE | W/session.rs:427; C/format_bar.rs:133 | Dropdown, panel, verb `style`. |
| Character style | core | DONE | C/panel.rs:446-548 | Character style buttons in the Styles level. |
| Linked style | common | PRESERVED | W/styles.rs; W/builtin.rs:48 | `w:link` is kept in `styles.xml` and never followed. |
| Table style | common | PARTIAL | W/layout/table.rs:244-270,556-590; W/styles.rs:250,283 | Own `tblPr`, `pPr`, `rPr` and cell shading from `tblStylePr`; no `basedOn` chain, no conditional run formatting. |
| Styles pane | core | PARTIAL | C/panel.rs:446-548 | List of every paragraph style; no previews, no Ctrl+Alt+Shift+S. |
| Quick Styles gallery | core | PARTIAL | C/format_bar.rs:133-166 | Dropdown with favourites first; no gallery with previews. |
| Modify Style | core | MISSING | PRD 132; A§11#18 | Styles can be applied and created, not modified. |
| Modify Style Format menu | common | MISSING | A§11#18 | No style editor. |
| Create a Style from selection | core | DONE | C/panel.rs:512 | Name field and Create. |
| Style based on | core | DONE | W/styles.rs:190-200 | Chain walked for paragraph and character styles. |
| Style for following paragraph | core | DONE | W/edit.rs:219-227 | Enter at the end of a heading takes `w:next`. |
| Update to match selection | common | MISSING | A§11#18 | Not found. |
| Delete style, Revert to default | common | MISSING | C/panel.rs:446-548 | Not found. |
| Built in styles | core | PARTIAL | W/builtin.rs:23; A§9c `builtin-styles` | Added on first use from an embedded `builtin-styles.docx`; the set is the one A lists, which has no Body Text, Index, Balloon Text or Plain Text. |
| Style Sets (Design tab) | common | MISSING | A§1b | Not found. |
| Themes (Design tab) | common | PRESERVED | W/styles.rs:21-110 | Theme is read for fonts and colours; cannot be changed. |
| Theme Colors | common | PARTIAL | W/styles.rs:21-110 | Slots resolved with shade and tint; no Customize Colors. |
| Theme Fonts | common | PARTIAL | W/styles.rs:21-110 | Theme fonts resolved; cannot be chosen or customised. |
| Document Defaults | core | DONE | W/styles.rs:245-312 | `docDefaults` start the resolution. |
| Set As Default | common | MISSING | A§1b | Not found. |
| Normal style reset | common | PARTIAL | W/ops.rs:1323-1339; C/keys.rs:176 | Ctrl+Shift+N and Clear all formatting reset to Normal; no Reset to Normal Paragraph Style command. |
| Table style options | common | PARTIAL | W/layout/table.rs:80 | `tblLook` is read; no UI. |
| Conditional table formatting | common | PARTIAL | W/layout/table.rs:556-590 | Only cell shading from first row, last row and banded rows; no column regions, no bold from the style. |
| Style property resolution | core | PARTIAL | W/styles.rs:245-312 | Defaults, paragraph chain, character chain, direct, XOR toggles; table style layer incomplete. |
| List style | niche | PARTIAL | W/numbering.rs:16,69 | `numStyleLink` is resolved; no list style editor. |
| Latent styles; Theme Effects; Limit allowed styles (3 rows) | niche | PRESERVED | A§3b | Kept in the XML. |
| Apply Styles; Style Inspector; Reveal Formatting; Automatically update; Manage Styles; Organizer; Paragraph Spacing gallery; Style separator; Style area pane; Keyboard shortcut per style (10 rows) | niche | MISSING | A§4b | No code. |

### 6. Lists

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Bullets | core | DONE | W/ops.rs:47; C/format_bar.rs:200-214 | Toggle, action, verb. |
| Define New Bullet: Symbol | common | MISSING | W/builtin.rs:104-215 | Existing symbol font bullets draw; none can be defined. |
| Bullet font, size, color, alignment | common | PRESERVED | W/numbering.rs:364 | Label run properties are drawn; no control. |
| Numbering | core | DONE | W/ops.rs:47; test `a_bullet_list_toggles_on_and_off_and_survives_a_save` | |
| Number formats | core | PARTIAL | W/numbering.rs:225-240 | 11 formats; every other format prints as decimal. |
| Define New Number Format | common | MISSING | W/numbering.rs | No definition UI. |
| Multilevel List | core | PARTIAL | W/ops.rs:116; W/builtin.rs:104-215 | Levels via Tab and Shift+Tab; no multilevel gallery. |
| Define New Multilevel List | common | MISSING | W/builtin.rs:104-215 | Not found. |
| Link Level to Style | common | PARTIAL | W/numbering.rs `level_for_style` | Honoured on read; cannot be created. |
| Restart at 1 | core | DONE | W/ops.rs:148 | Restart here with a start value. |
| Continue numbering | core | MISSING | A§1b | Not found. |
| Set Numbering Value | common | PARTIAL | W/ops.rs:148 | Start at N exists; no advance value. |
| Adjust List Indents | common | MISSING | A§4b | `suff` is honoured; no UI. |
| Change List Level | core | DONE | W/ops.rs:116; C/keys.rs:259-267 | Tab and Shift+Tab. |
| Automatic bullets and numbering as you type | core | PARTIAL | C/keys.rs:120-144 | `- `, `* `, `1. `, `1) ` on the space; Word's trigger is Enter; only "1" starts a number list. |
| Number restart rules | common | DONE | W/numbering.rs; test `levels_count_and_restart_the_way_word_numbers_an_outline` | `lvlRestart` honoured. |
| List Number and List Bullet styles | common | MISSING | A§9c `builtin-styles` | Only List Paragraph is in the built in set. |
| Lists inside table cells | core | DONE | W/layout/table.rs | Cell paragraphs use the same list code; no test names a list in a cell. |
| numbering.xml model | core | PARTIAL | W/numbering.rs:13-240 | `lvlPicBulletId` and picture bullets not handled. |
| Legal style numbering | niche | DONE | W/numbering.rs | `isLgl` honoured. |
| LISTNUM field; AUTONUM, AUTONUMLGL, AUTONUMOUT (2 rows) | niche | PRESERVED | W/layout/paragraph.rs:236-300 | Stored result shown. |
| Define New Bullet: Picture | niche | PRESERVED | W/order.rs:232; A§3b | `lvlPicBulletId` and `numPicBullet` are kept and not handled. |
| Define New List Style | niche | MISSING | A§1b | Not found. |

### 7. Tables

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Insert Table grid | core | DONE | W/ops.rs:221; C/panel.rs:571-708 | Rows x columns fields and a 3 x 3 `/` item; no hover grid. |
| Insert Table dialog | core | PARTIAL | C/panel.rs:571-708 | Rows and columns only; no fixed width or AutoFit choice. |
| Draw Table | common | MISSING | PRD 150 | Not done. |
| Convert Text to Table | common | MISSING | A§4b | Not found. |
| Convert Table to Text | common | MISSING | A§4b | Not found. |
| Insert rows above and below | core | DONE | W/ops.rs:247 | Panel and verb. |
| Insert columns left and right | core | DONE | W/ops.rs:247 | Panel and verb. |
| Delete cells, rows, columns, table | core | PARTIAL | W/ops.rs:247 | Rows, columns and table; no cell delete with shift. |
| Merge Cells | core | PARTIAL | W/ops.rs:348,365 | Merge right and merge down; no multi cell selection to merge a block. |
| Split Cells | core | PARTIAL | W/ops.rs:381 | Splits a merged cell back; no rows and columns prompt. |
| Split Table | common | MISSING | A§4b | Not found. |
| Table Properties: Table tab | common | PARTIAL | W/layout/table.rs:286-292,472-496 | Alignment, indent, percent width are read and drawn; no dialog. |
| Table Options | common | PARTIAL | W/layout/table.rs:47-48 | Default cell margins read; no UI; `tblCellSpacing` unread. |
| Table Properties: Row tab | common | PARTIAL | W/layout/table.rs:353-359; W/ops.rs:393 | Height and repeat header row; `cantSplit` unread and rows never split. |
| Table Properties: Column tab | common | PARTIAL | W/ops.rs:413 | Column width by verb `table width` only. |
| Table Properties: Cell tab | common | PARTIAL | W/layout/table.rs:286-292,317-326 | Vertical alignment and margins drawn; `tcW` ignored; no UI. |
| Table Properties: Alt Text | common | PRESERVED | A§3b | `tblCaption` and `tblDescription` stay in `tblPr`. |
| Repeat Header Rows | core | DONE | W/layout/table.rs:465; W/layout/mod.rs:717-735; W/ops.rs:393 | Panel button and layout. |
| AutoFit Contents | core | MISSING | W/layout/table.rs:472-496 | `tblLayout` not read; widths come from `tblGrid`. |
| AutoFit Window | core | PARTIAL | W/layout/table.rs:472-496 | A `pct` table width rescales the grid; no command. |
| Fixed Column Width | core | PARTIAL | W/layout/table.rs:472-496 | Grid widths are used as fixed; no command. |
| Table layout algorithm | core | PARTIAL | W/layout/table.rs:472-496 | `tblGrid` plus percent width; `tcW` and `dxa` `tblW` ignored when a grid exists. |
| Distribute Rows Evenly, Distribute Columns Evenly | core | MISSING | A§4b | Not found. |
| Cell Size group | core | PARTIAL | W/ops.rs:413 | Width by verb; row height read from file only. |
| Cell alignment grid | common | PARTIAL | W/layout/table.rs:286-292 | Vertical alignment read and drawn, horizontal through paragraph alignment; no grid control. |
| Text Direction in cells | common | PRESERVED | W/order.rs:177 | `textDirection` kept, not drawn. |
| Cell Margins | common | PARTIAL | W/layout/table.rs:47-48,317-326 | Read and drawn; no control; spacing between cells not read. |
| Table borders | core | PARTIAL | W/layout/table.rs:413-459 | Single and double only; no diagonals; no border controls. |
| Table shading | core | PARTIAL | W/ops.rs:441 | Shade cell with 5 colours; no row or table shading. |
| Table Styles gallery | common | MISSING | A§3b | New tables use Table Grid; no gallery. |
| Clear table style | common | MISSING | A§3b | Not found. |
| Sort (table) | common | MISSING | PRD 123 | Not found. |
| Formula | common | PRESERVED | W/layout/paragraph.rs:236-300 | `=` fields show their stored result. |
| Update table formulas | common | MISSING | W/ops.rs:1212-1250 | F9 updates only TOC and date fields. |
| Nested tables | common | DONE | W/layout/mod.rs:880-885; fixture `tables` | Laid out and drawn. |
| Table move handle | common | MISSING | A§6c | Not found. |
| Table resize handle | common | MISSING | A§6c | Not found. |
| Resize by dragging borders | core | MISSING | A§4b | Widths only by verb. |
| Select cell, row, column, table | core | MISSING | A§4b | No multi cell or whole table selection. |
| Tab in last cell | core | DONE | C/keys.rs:259-267 | Appends a row. |
| View Gridlines | core | MISSING | A§3b | Borderless tables draw no guide lines. |
| Floating table | common | PRESERVED | A§3b | `tblpPr` kept, not read. |
| Table caption | common | PARTIAL | S/actions.rs `caption` | Caption action is generic; stored SEQ number not recomputed. |
| Table Design and Layout tabs | core | PARTIAL | C/panel.rs:571-650 | "This table" buttons appear when the caret is in a table. |
| Cell merging model | core | DONE | W/layout/table.rs:315-326,363-377 | `gridSpan` and `vMerge` read, drawn, written. |
| Table grid | core | DONE | W/model.rs:206-214; W/edit.rs:632 | `tblGrid` read and made. |
| Table look | common | PARTIAL | W/layout/table.rs:80 | Read; no UI. |
| Table Positioning; Diagonal cell borders (2 rows) | niche | PRESERVED | A§3b | Kept, not read. |
| Excel Spreadsheet; Quick Tables; Insert Cells; Bookmarks in table formulas; New and Modify Table Style; Move rows with Alt+Shift+Up/Down (6 rows) | niche | MISSING | A§4b | No code. |

### 8. Illustrations

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Pictures: This Device | core | PARTIAL | S/actions.rs:113-120; C/page_view.rs:418-437 | Dialog offers png, jpg, gif, bmp; webp, tiff, ico decode; EMF, WMF and SVG draw as a grey box. |
| Screenshot, Screen Clipping | common | MISSING | A§4b | No picture paste either. |
| Picture Format tab | core | MISSING | A§4b | A picture cannot be selected. |
| Corrections | common | MISSING | A§4b | Not found. |
| Color | common | MISSING | A§4b | Not found. |
| Compress Pictures | common | MISSING | A§4b | Not found. |
| Change Picture, Reset Picture | common | MISSING | A§4b | Not found. |
| Picture Styles | common | MISSING | A§4b | Not found. |
| Picture Border | common | MISSING | A§4b | Not found. |
| Crop | common | PRESERVED | A§1b | `srcRect` is kept and not applied, so a cropped picture draws uncropped. |
| Format Picture pane | common | MISSING | A§4b | Not found. |
| Lock aspect ratio | core | MISSING | S/commands.rs:901 | Width is chosen at insert only; no resize afterwards. |
| Rotate and flip | common | PRESERVED | A§3b | `a:xfrm` kept, not applied. |
| Shapes gallery | common | PRESERVED | W/model.rs:294-303 | Existing shapes draw as a grey box; none can be made. |
| Shape Format tab | common | MISSING | A§3b | Not found. |
| Text in shapes | common | PRESERVED | W/model.rs:294-303 | Text inside `wps:txbx` is kept, never read or drawn. |
| Chart | common | PRESERVED | A§3b | Chart part and frame kept; frame draws as a grey box. |
| Chart Design and Format | common | MISSING | A§3b | Not found. |
| Text Box: Draw | core | MISSING | PRD 158 | Cannot be made; existing text boxes draw as a grey box without their text. |
| Text box margins and autofit | common | PRESERVED | A§3b | `bodyPr` kept. |
| Wrap Text: In Line with Text | core | DONE | W/layout/paragraph.rs:588-653 | Inline pictures laid out as characters. |
| Wrap Text: Square | core | PARTIAL | W/layout/paragraph.rs:620-653; W/layout/mod.rs:798-826 | The picture is placed; no line avoids it (`Float.wrap` never read). |
| Wrap Text: Tight | common | PARTIAL | W/layout/mod.rs:798-826 | Placed, text not wrapped. |
| Wrap Text: Top and Bottom | core | PARTIAL | W/layout/mod.rs:798-826 | Placed, text runs under the picture. |
| Wrap Text: Behind Text | core | DONE | W/layout/mod.rs:186-202 | Pictures are drawn before text. |
| Wrap Text: In Front of Text | core | PARTIAL | W/layout/mod.rs:186-202 | Drawn before text like every float, so it sits behind. |
| Wrap text sides | common | PRESERVED | A§3b | `wrapText` kept. |
| Distance from text | common | PRESERVED | A§3b | `distT` and the rest kept. |
| Position presets | common | MISSING | A§4b | Not found. |
| Layout dialog: Position | common | PARTIAL | W/layout/mod.rs:798-826 | Relative to page, margin, column, paragraph, line, character; no UI. |
| Layout dialog: Text Wrapping | common | PRESERVED | W/layout/paragraph.rs:634-643 | Wrap kind parsed and stored, not used. |
| Layout dialog: Size | common | PARTIAL | W/layout/paragraph.rs:615-618 | Absolute `wp:extent` drawn; relative size and rotation not applied; no UI. |
| Anchor, Lock Anchor | common | PARTIAL | W/layout/paragraph.rs:541-551 | The float is an atom in its anchor paragraph; no anchor symbol or lock. |
| Align | common | MISSING | A§4b | Not found. |
| Group, Ungroup, Regroup | common | PRESERVED | A§3b | `wpg` kept; draws as a grey box. |
| Z order | core | PRESERVED | W/layout/mod.rs:186-202 | `relativeHeight` kept and ignored. |
| Rotate | common | PRESERVED | A§3b | Kept, not applied. |
| Alt Text | core | PRESERVED | W/media.rs:94 | `wp:docPr` description is kept; nothing sets it. |
| Insert Caption | core | PARTIAL | S/actions.rs `caption` | Inserts a caption paragraph; no label choice or position; SEQ is not recomputed. |
| Selection Pane | common | MISSING | A§4b | Not found. |
| Hyperlink on picture | common | PRESERVED | A§3b | `hlinkClick` kept. |
| SVG with PNG fallback | common | DONE | W/layout/paragraph.rs:588-601 | `a:blip r:embed` is the PNG fallback and is drawn. |
| Stock Images; Online Pictures; Online Video; 3D Models; Draw tab (5 rows) | niche | OUT-OF-SCOPE | B§8 | Stock and online pictures and online video need Microsoft services, 3D models and ink are excluded by the brief. |
| Insert and Link; Remove Background; Artistic Effects; Transparency; Picture Effects; Picture Layout (SmartArt); New Drawing Canvas; Edit Points; Change Shape and Merge Shapes; Connectors; Icons; SmartArt; SmartArt Design tab; WordArt; Text Box: Built in; Linked text boxes; Wrap Text: Through; Edit Wrap Points; Linked pictures; Embedded chart workbook (20 rows) | niche | PRESERVED | A§3b | Parts and XML kept; drawn as a grey box where there is a frame. |

### 9. Page layout and sections

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Margin presets | core | PARTIAL | W/ops.rs:31-36; S/actions.rs | Normal, Narrow, Wide in the UI, Moderate by verb; no Mirrored, Office 2003 or Last Custom. |
| Custom margins | core | DONE | C/panel.rs:709-804 | Four drag boxes. |
| Gutter and gutter position | common | PARTIAL | W/layout/section.rs:51-144 | Gutter is used when read; no control. |
| Mirror margins | common | PRESERVED | A§3b | `mirrorMargins` unread. |
| Orientation | core | DONE | W/ops.rs:542 | Panel and verb. |
| Size | core | PARTIAL | W/ops.rs:19-28,507 | 8 named sizes; no custom width and height, no envelopes. |
| Columns presets | core | PARTIAL | W/ops.rs:586; C/panel.rs | One, Two, Three equal columns; no Left or Right presets. |
| Columns custom | common | PARTIAL | W/layout/section.rs:125-144 | Explicit widths read and drawn; no width UI; separator line not drawn. |
| Balanced columns | common | MISSING | W/layout/mod.rs:572-621 | No balancing at a continuous break. |
| Page Break (Ctrl+Enter) | core | DONE | C/keys.rs:194-197 | |
| Column Break (Ctrl+Shift+Enter) | common | DONE | C/keys.rs:198-201 | |
| Section Break: Next Page | core | DONE | W/ops.rs:600; test `a_section_break_starts_a_new_page` | |
| Section Break: Continuous | core | DONE | W/layout/mod.rs:572-606 | Continuous only when page size is equal. |
| Hyphenation | common | PRESERVED | W/layout/paragraph.rs:490-501 | No hyphenation; `autoHyphenation` stays in settings. |
| Hyphenation Options | common | PRESERVED | A§3b | Settings kept. |
| Optional hyphen | common | PARTIAL | W/layout/paragraph.rs:492 | Soft hyphen is a zero width break point; Ctrl+- does not insert one. |
| Nonbreaking hyphen and space | common | PARTIAL | W/layout/paragraph.rs:389-396,480 | Both behave as non breaking when present; no Ctrl+Shift+- or Ctrl+Shift+Space. |
| Indent and Spacing group | core | DONE | C/panel.rs:279-445 | |
| Watermark | common | PRESERVED | A§11#35 | A VML or WordArt header shape draws as a grey box, not as a watermark. |
| Page Color | common | PARTIAL | C/panel.rs:775-803; W/layout/mod.rs:547-552 | 5 swatches, solid only. |
| Page Borders | common | PRESERVED | W/layout/section.rs | `pgBorders` kept, not drawn. |
| Vertical alignment on page | common | PRESERVED | W/layout/section.rs:41-45 | `vAlign` is read and never used. |
| Different First Page | core | PARTIAL | W/layout/section.rs; S/commands.rs:765-812 | `titlePg` honoured; set only by verb `section header-first`; no toggle. |
| Different Odd and Even | core | PARTIAL | W/layout/mod.rs:371-379 | `evenAndOddHeaders` honoured; set by verb; no toggle. |
| Page numbering per section | core | PARTIAL | W/layout/section.rs | `pgNumType start` honoured; `fmt` unused; no UI. |
| Link to previous | core | PARTIAL | W/layout/mod.rs:917-977 | Inheritance honoured on read; no link or unlink control. |
| Per section settings | core | PARTIAL | W/layout/section.rs:51-144 | Size, orientation, margins, columns, headers, footers; not line numbers, vertical alignment, borders, notes placement, text direction, grid. |
| Page Setup dialog | core | PARTIAL | C/panel.rs:709-804 | Page level; no Paper, Layout or Document Grid tab, no Set As Default. |
| Footnotes and endnotes per section | common | PRESERVED | A§3b | `footnotePr` unread. |
| Gridlines and ruler | core | MISSING | A§6c | No ruler, no gridlines. |
| Section Break: Even Page; Odd Page (2 rows) | niche | PARTIAL | S/commands.rs:498-503 | By verb only. |
| Multiple pages 2 per sheet; Book fold; Paper source; Text Wrapping Break; Line Numbers; Print Background Colors and Images; Section text direction; Suppress endnotes (8 rows) | niche | PRESERVED | A§3b | Kept in `sectPr` or settings, not acted on. |

### 10. Headers, footers and page numbers

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Insert Header, Insert Footer | core | PARTIAL | W/ops.rs:638-701 | Text boxes in the Page level make a header; no gallery. |
| Edit Header, Edit Footer | core | DONE | C/page_view.rs:61-77; test `a_double_press_on_the_margin_edits_the_header` | Double press on the margin. |
| Remove Header, Remove Footer | core | PARTIAL | W/ops.rs:638-701 | Setting empty text empties the story; the part and reference stay. |
| Header & Footer tab | core | PARTIAL | C/page_view.rs:151-178 | A dashed marker and label only. |
| Different First Page | core | PARTIAL | W/ops.rs:677-681 | By verb only. |
| Different Odd & Even Pages | common | PARTIAL | W/ops.rs:682-690 | By verb only. |
| Link to Previous | core | PARTIAL | W/layout/mod.rs:917-977 | Inherited on read; setting text on a section with no own header makes a new part (W/ops.rs:658-693). |
| Go to Header, Go to Footer, Previous, Next | common | MISSING | C/page_view.rs:61-77 | Only a double press. |
| Header from Top, Footer from Bottom | common | PARTIAL | W/layout/section.rs:51-144 | Distances honoured; no control. |
| Page Number gallery | core | PARTIAL | S/actions.rs:108-111 | One action: "Page {PAGE} of {NUMPAGES}" in the footer. |
| Format Page Numbers | common | PARTIAL | W/layout/fields.rs:187-205; W/layout/section.rs:41-45 | Field `\*` switches honoured; `pgNumType fmt` unused; no UI. |
| Remove Page Numbers | common | MISSING | A§4b | Not found. |
| Page X of Y | core | DONE | S/actions.rs:108-111; test `a_footer_with_page_numbers_is_made_and_numbers_each_page` | |
| Date & Time | common | PARTIAL | S/actions.rs `date`; W/ops.rs:1718-1760 | DATE field with a fixed picture; no format list. |
| Document Info | common | MISSING | A§3b | AUTHOR, TITLE, FILENAME are not evaluated. |
| Quick Parts in header | common | MISSING | A§4b | Not found. |
| Pictures in header | core | PARTIAL | W/layout/mod.rs:917-977 | Header stories lay out through the same code and draw pictures; inserting while editing a header was not read. |
| Close Header and Footer | core | DONE | C/keys.rs:269-282 | Esc. |
| Watermark in header | common | PRESERVED | A§11#35 | Draws as a grey box. |
| Header and footer types | core | DONE | W/layout/mod.rs:917-977 | Default, first, even with inheritance. |
| Tables and text boxes in headers | common | PARTIAL | W/read.rs:80 | Tables lay out; text boxes do not draw; setting header text from the panel replaces the story with one paragraph (W/ops.rs:696-700). |
| Show Document Text | niche | MISSING | A§6c | Not found. |
| Insert Alignment Tab | niche | PRESERVED | W/layout/paragraph.rs:516-527 | `w:ptab` has no handler in layout and takes zero width. |
| SECTIONPAGES and SECTION fields | niche | PARTIAL | W/layout/fields.rs:187-205 | SECTIONPAGES evaluated, SECTION not. |

### 11. References

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Table of Contents: Automatic Table 1, 2 | core | PARTIAL | W/ops.rs:1121-1180 | Builds "Contents" plus TOC1..n paragraphs with dot leaders and `\h` links; not wrapped in the `w:sdt docPartObj` Word uses; no gallery. |
| Custom Table of Contents dialog | common | PARTIAL | S/actions.rs `table-of-contents` | Levels 1 to 3 only; no tab leader, page number or style options. |
| Update Table | core | PARTIAL | W/ops.rs:1212-1292 | `find_toc` and `toc_entries` skip any paragraph whose path is longer than 1, so a TOC inside a content control (every TOC Word wrote) is not found and headings inside tables are skipped; no "page numbers only" mode. |
| Remove Table of Contents | common | MISSING | PRD 181 | Not found. |
| Add Text | common | MISSING | A§4b | Not found. |
| Table of Figures | common | MISSING | A§4b | Not found. |
| Insert Footnote | core | PARTIAL | S/actions.rs:779-786; C/page_view.rs:61-77 | The action inserts an empty note and a press cannot land in a note, so text can only be supplied by verb `insert footnote <text>`. |
| Insert Endnote | core | PARTIAL | S/actions.rs:789-797 | Same limit as footnotes. |
| Footnote and Endnote dialog | common | PRESERVED | W/layout/mod.rs:343-368 | `footnotePr` and `endnotePr` unread: footnotes arabic, endnotes lower roman, always continuous. |
| Next Footnote, Show Notes | common | MISSING | A§4b | Not found. |
| Note numbering formats | common | PARTIAL | W/layout/mod.rs:343-368 | Fixed arabic and lower roman; `customMarkFollows` references are not counted. |
| Insert Citation | common | MISSING | A§4b | Not found. |
| Manage Sources | common | PRESERVED | W/package.rs | The bibliography `customXml` part is kept byte for byte. |
| Bibliography styles | common | MISSING | A§4b | Not found. |
| Bibliography, References, Works Cited | common | PRESERVED | A§3b | A bibliography `w:sdt` and CITATION fields show stored results. |
| Insert Caption | common | PARTIAL | S/actions.rs `caption` | No label list, no chapter number, no AutoCaption. |
| Cross-reference | core | MISSING | PRD 156; A§1b | Insert not found; existing REF fields show their stored result. |
| Bookmark | core | PARTIAL | W/ops.rs:1036,1064; C/panel.rs:805-819 | Insert, list, click to go; no delete, no hidden bookmarks, no sort. |
| Hyperlink (Ctrl+K) | core | PARTIAL | W/ops.rs:993-1033; C/keys.rs:184-189 | URL or `#bookmark`; no screen tip, e mail, new document, Remove Hyperlink; cannot be followed. |
| Hyperlink styles | core | PARTIAL | W/layout/paragraph.rs:296-300 | Hyperlink style/colour applied; FollowedHyperlink never used. |
| TOC field | core | PARTIAL | W/ops.rs:1262-1267 | Only `\o` levels read; `\u`, `\z`, `\t`, `\b` and the rest ignored on update. |
| Manual Table; Convert notes; Continuation notice; Insert Placeholder; Update Index (5 rows) | niche | MISSING | A§4b | Not found. |
| TC field; Table of Authorities; Source types; Index: Mark Entry; XE field; Index: Insert Index; Master Document (7 rows) | niche | PRESERVED | A§3b | Fields and `w:subDoc` stay in the XML. |
| Note separators | niche | PARTIAL | W/layout/mod.rs:1010-1020; W/layout/paragraph.rs:469 | A rule is drawn; continuation separator never used because notes do not continue. |
| Smart Lookup; Researcher (2 rows) | niche | OUT-OF-SCOPE | B§11 | Web lookup and an online research service. |

### 12. Fields

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| PAGE | core | PARTIAL | W/layout/fields.rs:187-205; W/layout/mod.rs:393-394 | Evaluated in headers and footers only; in the body it keeps its stored value. |
| NUMPAGES | core | PARTIAL | W/layout/fields.rs:187-205 | Same: headers and footers only. |
| DATE | core | DONE | W/ops.rs:1718-1760 | Rewritten on update fields. |
| TIME | core | DONE | W/ops.rs:1718-1760 | Rewritten on update fields. |
| CREATEDATE | common | DONE | W/ops.rs:1718-1760 | |
| SAVEDATE | common | DONE | W/ops.rs:1718-1760 | |
| AUTHOR | common | PRESERVED | A§3b | Stored result shown, never updated. |
| TITLE, SUBJECT, KEYWORDS, COMMENTS | common | PRESERVED | A§3b | Stored result shown. |
| FILENAME | common | PRESERVED | A§3b | Stored result shown. |
| DOCPROPERTY | common | PRESERVED | A§3b | Stored result shown. |
| TOC | core | PARTIAL | W/ops.rs:1212-1292 | See section 11, Update Table. |
| REF | core | PRESERVED | A§3b | Stored result shown, never updated. |
| PAGEREF | core | PRESERVED | A§3b | Stored result shown; TOC entries made by Unluminous hold page numbers as text. |
| NOTEREF | common | PRESERVED | A§3b | Stored result. |
| STYLEREF | common | PRESERVED | A§3b | Stored result; a running header using it shows the value saved by Word. |
| SEQ | common | PRESERVED | A§3b | Stored result; captions are not renumbered. |
| HYPERLINK | core | PRESERVED | A§3b | Stored result shown; the `w:hyperlink` element is modelled separately. |
| IF | common | PRESERVED | A§3b | Stored result. |
| = (Formula) | common | PRESERVED | A§3b | Stored result. |
| MERGEFIELD | common | PRESERVED | A§3b | Stored result. |
| SYMBOL | common | PRESERVED | A§3b | Stored result. |
| CITATION | common | PRESERVED | A§3b | Stored result. |
| BIBLIOGRAPHY | common | PRESERVED | A§3b | Stored result. |
| ADDRESSBLOCK, GREETINGLINE | common | PRESERVED | A§3b | Stored result. |
| Switch `\* MERGEFORMAT` | common | PRESERVED | A§3b | Kept in the code text. |
| Switches `\* Upper`, `Lower`, `FirstCap`, `Caps` | common | PRESERVED | W/layout/fields.rs:187-205 | Not applied. |
| Switches `\* Arabic`, `Roman`, `roman`, `ALPHABETIC`, `alphabetic`, `Ordinal`, `Cardtext`, `Ordtext`, `Hex`, `Dollartext` | common | PARTIAL | W/layout/fields.rs:187-205 | Roman, alphabetic, Ordinal, CardText on PAGE fields; no Ordtext, Hex or Dollartext. |
| Switch `\@ "dddd, MMMM d, yyyy"` | core | PARTIAL | W/ops.rs:1718-1760 | yyyy, yy, MMMM, MMM, MM, dd, d, HH, mm; no dddd, ddd, M, h, hh, s, ss, AM/PM. |
| Switch `\# "#,##0.00"` | common | PRESERVED | A§3b | Not applied. |
| Update fields (F9) | core | PARTIAL | C/keys.rs:285; W/ops.rs:1212-1250 | Whole document only; TOC, DATE, TIME, CREATEDATE, SAVEDATE. |
| Toggle field codes | core | MISSING | C/keys.rs:154-241 | Shift+F9 and Alt+F9 not bound; codes are always hidden. |
| Unlink | common | MISSING | A§4b | Not found. |
| Field shading | common | MISSING | W/layout/paragraph.rs:236-300 | Not drawn. |
| Cached results | core | DONE | W/layout/paragraph.rs:236-300 | Code hidden, stored result drawn. |
| Complex field storage | core | DONE | W/read.rs:338; W/model.rs:159-161 | `fldChar` and `instrText` round trip. |
| Simple field storage | core | DONE | W/read.rs:203 | `w:fldSimple` is a wrapper. |
| SECTIONPAGES | niche | PARTIAL | W/layout/fields.rs:187-205 | Evaluated in headers and footers. |
| Nested fields | niche | DONE | W/layout/paragraph.rs:236-300 | Stack of open fields. |
| Lock and unlock; Next field; Insert empty field (3 rows) | niche | MISSING | A§6d | Not found. |
| SECTION; NUMWORDS, NUMCHARS; PRINTDATE; EDITTIME; LASTSAVEDBY; FILESIZE; TEMPLATE; DOCVARIABLE; INFO; USERNAME, USERINITIALS, USERADDRESS; TC; TA, TOA; INDEX; XE; RD; AUTONUM family; LISTNUM; INCLUDETEXT; INCLUDEPICTURE; INCLUDE; LINK; EMBED; DDE, DDEAUTO; COMPARE; SKIPIF; MERGEREC, MERGESEQ; NEXT, NEXTIF; DATABASE; ASK; FILLIN; SET; QUOTE; BARCODE; EQ; ADVANCE; GOTOBUTTON, MACROBUTTON; PRIVATE; FORMTEXT family; `\* CHARFORMAT`; `\* LOCK`; Dirty fields; Update on open (42 rows) | niche | PRESERVED | A§3b | Stored result shown or the element kept; `w:dirty` and `updateFields` are not acted on. |

### 13. Mailings

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Insert Merge Field | core | PARTIAL | S/commands.rs:522-525 | Verb `insert field MERGEFIELD name` writes the field with an empty result; no data source. |
| Preview Results | core | MISSING | PRD 244-248 | Mail merge is out of scope. |
| Finish & Merge: Edit Individual Documents | core | MISSING | PRD 244-248 | Out of scope. |
| Finish & Merge: Print Documents | core | MISSING | PRD 244-248 | Out of scope, and there is no print. |
| Envelopes; Labels; Start Mail Merge types; Step by Step Mail Merge Wizard; Select Recipients: Type a New List; Select Recipients: Use an Existing List; Edit Recipient List; Filter and Sort recipients; Highlight Merge Fields; Address Block; Greeting Line; Update Labels; Merge to new document by section (13 rows) | common | MISSING | PRD 244-248 | Mail merge is listed as out of scope. |
| Mail merge number formats | common | PRESERVED | A§3b | `\#` and `\@` on MERGEFIELD stay in the code text. |
| Choose from Outlook Contacts; Match Fields; Rules; Check for Errors; Send Email Messages; Directory merge (6 rows) | niche | MISSING | PRD 244-248 | Out of scope. |
| Data source connection | niche | PRESERVED | W/package.rs | `mailMerge` in settings and the data source relationship are kept. |

### 14. Review

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Spelling & Grammar (Editor) | core | PARTIAL | W/spell.rs; S/spelling.rs | Spelling only (English, SCOWL), F7 jumps to the next error; no grammar, no pane. |
| Editor pane | common | OUT-OF-SCOPE | B§14 | Editor refinements and score come from a Microsoft service. |
| Red squiggle spelling, blue double underline grammar | core | PARTIAL | C/spelling.rs | Red wavy underline only. |
| Right click spelling menu | core | PARTIAL | C/spelling.rs:68-90 | Up to 6 suggestions, Ignore All, Add to Dictionary, Delete Repeated Word; no AutoCorrect or Language. |
| Thesaurus | common | MISSING | MANIFEST:13 | Not done. |
| Word Count | core | PARTIAL | W/session.rs:644-671 | Body only: footnotes and text boxes not counted, no option, no selection count in the status bar. |
| Read Aloud | common | OUT-OF-SCOPE | B§14 | Excluded by the brief (speech service). |
| Check Accessibility | common | MISSING | A§2 | Not found. |
| Language: Set Proofing Language | core | PRESERVED | W/spell.rs:116 | `w:lang`, `w:noProof` honoured by spelling; nothing sets them; English only. |
| Language: Language Preferences | common | MISSING | W/spell.rs:50 | One dictionary. |
| New Comment | core | DONE | W/ops.rs:770-852; C/overlays.rs:307-407 | Bubble, Review level, verb; Ctrl+Alt+M is bound to indent (Part 1 section 21). |
| Reply, Resolve, Reopen | common | DONE | W/ops.rs:920-966; W/ops.rs:1561-1660 | `commentsExtended` updated. |
| Delete Comment | core | PARTIAL | W/ops.rs:966 | Delete one; no delete all. |
| Previous and Next comment | core | MISSING | A§6c | A press on a card selects it; no commands. |
| Show Comments | common | PARTIAL | C/page_view.rs:180-265 | Contextual cards in a margin; no Simple mode or hide. |
| Comments pane | common | PARTIAL | C/panel.rs:821-911 | Review level lists cards; no reviewer filter. |
| Track Changes On/Off | core | DONE | W/session.rs:564-567; C/panel.rs:821-911 | Writes `trackRevisions`; Ctrl+Shift+E is unbound. |
| Display for Review | core | MISSING | A§3b | Always All Markup. |
| Show Markup | common | MISSING | A§3b | Not found. |
| Balloons | common | MISSING | A§3b | Revisions are inline; comments are in the margin. |
| Reviewing Pane | common | MISSING | A§3b | Not found. |
| Accept and Reject | core | DONE | W/edit.rs:805-890; W/session.rs:531-575 | One or all, existing `rPrChange` and `pPrChange` included. |
| Next and Previous change | core | MISSING | A§6c | Not found. |
| Track formatting | common | PARTIAL | W/edit.rs:849,872 | Existing `rPrChange` resolves; new formatting under tracking is not recorded. |
| Change User Name | common | DONE | C/settings_page.rs | Author setting; used for comments and revisions. |
| Document Inspector | common | MISSING | A§2 | Not found. |
| Review tab shortcuts | core | MISSING | C/keys.rs:154-241 | Ctrl+Alt+M and Ctrl+Shift+E not bound. |
| Track moves | niche | PARTIAL | W/edit.rs:96,706,846 | `moveFrom` and `moveTo` display like delete and insert and are accepted or rejected with them. |
| Restrict Editing: Formatting restrictions, Editing restrictions, Exceptions, Start Enforcing Protection (4 rows) | niche | PRESERVED | A§2 | `documentProtection` and `permStart` kept, not enforced. |
| Language: Set as default; Lock Tracking; Track Changes Options; Compare; Combine; Show Source Documents (6 rows) | niche | MISSING | PRD 192 | Compare and combine are out of scope. |
| Translate; @mentions; Hide Ink; Smart Lookup and Researcher; Block Authors (5 rows) | niche | OUT-OF-SCOPE | B§14 | Online translation, notifications to people, ink, web research, shared file author blocking. |

### 15. View

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Read Mode | common | MISSING | PRD 202 | Print Layout only. |
| Print Layout | core | DONE | C/page_view.rs | The only view. |
| Outline view | common | MISSING | PRD 202 | Not done. |
| Ruler | core | MISSING | A§6c | Not found. |
| Gridlines | common | MISSING | A§6c | Not found. |
| Navigation Pane | core | PARTIAL | C/navigation.rs:1-46 | Headings only, no drag to reorder, no Ctrl+Shift+O. |
| Zoom dialog | core | PARTIAL | C/page_view.rs:561-586 | Ctrl+wheel, pinch, 25% to 500%; no Page width, Whole page, Many pages. |
| One Page, Multiple Pages, Page Width | core | MISSING | A§11#4 | Fit Width and Fit Page not found. |
| Zoom slider | core | MISSING | C/format_bar.rs:281-299 | Zoom shown as text. |
| New Window | common | MISSING | A§6c | Not found. |
| Switch Windows | common | DONE | C/mod.rs | Editor tabs. |
| Show: Ruler, Gridlines, Navigation Pane | core | PARTIAL | S/actions.rs `navigation` | Navigation pane only. |
| Dark mode | common | PARTIAL | A§6c | Window theme follows the app; the page is always white. |
| Page thumbnails | core | MISSING | C/navigation.rs | Navigation pane has no Pages tab. |
| Web Layout; Outline tab: Master Document; Draft view; Focus; Page Movement; Arrange All; Split; View Side by Side; Macros; Properties pane (10 rows) | niche | MISSING | PRD 202 | Not found. |
| Immersive Reader; Learning Tools (2 rows) | niche | OUT-OF-SCOPE | B§15 | Online reading service features (read aloud, translate). |

### 16. Insert, other

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Cover Page | common | MISSING | A§4b | Not found. |
| Blank Page | common | MISSING | A§4b | Not found. |
| Page Break | core | DONE | C/keys.rs:194-197 | |
| Quick Parts: AutoText | common | MISSING | A§4b | Not found. |
| Quick Parts: Document Property | common | MISSING | A§4b | Not found. |
| Quick Parts: Field | common | PARTIAL | W/ops.rs:1083; S/commands.rs:522-525 | Verb `insert field <code>` with an empty result; no dialog. |
| Date & Time | common | PARTIAL | S/actions.rs `date` | See section 10. |
| Equation | common | MISSING | W/layout/paragraph.rs:538-553 | Cannot be made; existing equations take zero width and the writer wraps them in `w:r` (Part 2). |
| Equation tools | common | MISSING | PRD 160 | Equation editing is out of scope. |
| Symbol | core | PARTIAL | C/panel.rs:658-668 | See section 3. |
| Emoji | common | NOT-DETERMINED | C/keys.rs:31-32 | See section 3. |
| Horizontal Line | common | DONE | S/actions.rs | |
| Link (Hyperlink) | core | PARTIAL | W/ops.rs:993-1033 | See section 11. |
| Bookmark, Cross-reference | core | PARTIAL | W/ops.rs:1036 | Bookmark yes, cross reference no. |
| Comment | core | DONE | W/ops.rs:770 | |
| Header, Footer, Page Number | core | PARTIAL | W/ops.rs:638 | See section 10. |
| Text Box, WordArt, Drop Cap | common | PRESERVED | A§3b | Not made; existing ones kept and not drawn. |
| Table, Pictures, Shapes, Icons, 3D Models, SmartArt, Chart, Screenshot | core | PARTIAL | W/ops.rs:221; W/media.rs:28 | Table and picture can be inserted; the rest cannot. |
| Building Blocks Organizer; Building block galleries; Object (3 rows) | niche | PRESERVED | W/package.rs | Glossary part and `w:object` kept. |
| Signature Line; Text from File; LaTeX input; UnicodeMath input; Math AutoCorrect; Dictionary of recent Insert items (6 rows) | niche | MISSING | A§4b | Not found. |
| Ink Equation; Online Video; Add-ins: Get Add-ins, My Add-ins; Wikipedia and Bing images add-ins (4 rows) | niche | OUT-OF-SCOPE | B§16 | Ink, online video and the Office add in store. |

### 17. Forms and controls

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Rich Text Content Control | common | PRESERVED | W/read.rs:138-149; W/write.rs:165-190 | Block content laid out and editable as text; no control behaviour, no boundary, cannot be made. |
| Plain Text Content Control | common | PRESERVED | W/read.rs:286-293 | Inline `w:sdt` is a wrapper; same limits. |
| Check Box Content Control | common | PRESERVED | W/read.rs:286-293 | Shows the stored glyph; cannot be toggled. |
| Combo Box Content Control | common | PRESERVED | W/read.rs:286-293 | Shows the stored text. |
| Drop-Down List Content Control | common | PRESERVED | W/read.rs:286-293 | Shows the stored text. |
| Date Picker Content Control | common | PRESERVED | W/read.rs:286-293 | Shows the stored text. |
| Content control properties | common | MISSING | A§3b | Title, tag, lock not shown or enforced. |
| Edit Placeholder Text | common | PRESERVED | W/write.rs:165-190 | `placeholder` and `showingPlcHdr` kept. |
| Picture Content Control; Building Block Gallery Content Control; Repeating Section Content Control; Group; Legacy Tools: Text, Check Box, Drop-Down form fields, Form field protection, Insert Frame and Reset; ActiveX Controls; Control properties window; XML Mapping; .docm storage (13 rows) | niche | PRESERVED | W/write.rs:16-39 | Kept in the XML or as parts. |
| Developer tab; Design Mode; Map Pane; Document Template, Add-ins; Macros dialog; Record Macro; Visual Basic Editor; Macro Security; Auto macros; Document events; Word object model (11 rows) | niche | MISSING | PRD 210 | Macros never run (MANIFEST:13). |

### 18. Proofing and automation

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| AutoCorrect: Replace text as you type | core | MISSING | A§8 | No replacement list. |
| AutoCorrect: Correct TWo INitial CApitals | common | MISSING | A§8 | Not found. |
| AutoCorrect: Capitalize first letter of sentences | core | MISSING | A§8 | Not found. |
| AutoCorrect: Capitalize first letter of table cells | common | MISSING | A§8 | Not found. |
| AutoCorrect: Capitalize names of days | common | MISSING | A§8 | Not found. |
| AutoCorrect: Correct accidental usage of cAPS LOCK key | common | MISSING | A§8 | Not found. |
| Automatically use suggestions from the spelling checker | common | MISSING | A§8 | Not found. |
| AutoFormat As You Type: Straight quotes with smart quotes | core | MISSING | A§8 | Typed quotes stay straight. |
| AutoFormat As You Type: Ordinals (1st) with superscript | common | MISSING | A§8 | Not found. |
| AutoFormat As You Type: Fractions (1/2) with fraction character | common | MISSING | A§8 | Not found. |
| AutoFormat As You Type: Hyphens (--) with dash | common | MISSING | A§8 | Not found. |
| AutoFormat As You Type: Bold and italic with real formatting | common | MISSING | A§8 | Not found. |
| AutoFormat As You Type: Internet and network paths with hyperlinks | core | MISSING | A§8 | A typed URL stays text. |
| AutoFormat As You Type: Automatic bulleted lists | core | PARTIAL | C/keys.rs:128-137 | On the space after `- ` or `* `. |
| AutoFormat As You Type: Automatic numbered lists | core | PARTIAL | C/keys.rs:128-137 | Only `1. ` and `1) `. |
| AutoFormat As You Type: Border lines | common | PARTIAL | C/keys.rs:246-254 | `---` then Enter only. |
| AutoText | common | MISSING | A§4b | Not found. |
| Custom dictionaries | common | PARTIAL | S/spelling.rs; A§8 | `words.dic` and Office `CUSTOM.DIC` read; Add to Dictionary on the card; no list editor. |
| Grammar settings | common | MISSING | MANIFEST:13 | No grammar. |
| Proofing options | common | PARTIAL | W/spell.rs:90-186 | Ignores capitals, words with digits, addresses, flags repeated words; none are options. |
| Check spelling as you type, Mark grammar errors as you type | core | PARTIAL | C/panel.rs:821-911 | Spelling checkbox; no grammar. |
| Recheck Document | common | MISSING | W/spell.rs:45-75 | Ignore All cannot be reset. |
| AutoCorrect Exceptions; Math AutoCorrect; AutoFormat tables; Built in Heading styles; Format beginning of list item; Set left and first indent with tabs and backspaces; Define styles based on your formatting; AutoFormat (batch); Exclusion dictionary; Readability statistics; Autocomplete suggestions (11 rows) | niche | MISSING | A§8 | Not found. |
| Dictate; Transcribe; Editor (AI); Copilot in Word; Sensitivity labels; Smart Lookup (6 rows) | niche | OUT-OF-SCOPE | B§18 | Speech, AI and Microsoft Purview or web services. |

### 20. Accessibility and navigation

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Screen reader support | core | NOT-DETERMINED | A§6 | Whether the egui window provides an accessibility tree for the page was not read. |
| Accessibility Checker | common | MISSING | A§2 | Not found. |
| Alt text on pictures, shapes, charts, tables, SmartArt | core | PRESERVED | W/media.rs:94 | Kept; nothing edits it. |
| Heading structure | core | DONE | S/actions.rs; C/navigation.rs | Real Heading styles and a headings pane. |
| Table header rows | core | DONE | W/ops.rs:393 | `tblHeader` toggle. |
| Hyperlink text | core | PARTIAL | W/ops.rs:993-1033 | Link text is editable; no screen tip. |
| Color contrast | common | MISSING | A§2 | Not found. |
| Language tagging | common | PRESERVED | W/spell.rs:116 | `w:lang` kept. |
| Keyboard navigation | core | PARTIAL | C/keys.rs:146-300 | Text chords; no F6, KeyTips or Shift+F10. |
| Access keys | common | MISSING | A§6c | No ribbon. |
| High contrast | common | NOT-DETERMINED | A§6c | Window theme handling not read. |
| Read Aloud | common | OUT-OF-SCOPE | B§20 | Excluded by the brief. |
| Tagged PDF export | common | MISSING | W/pdf.rs:1-60 | No tags. |
| Navigation pane for headings | core | DONE | C/navigation.rs:1-46 | |
| Zoom and magnifier | core | DONE | C/page_view.rs:561-586 | To 500%. |
| Learning Tools; Text spacing; Accessible templates; Accessibility status bar indicator (4 rows) | niche | MISSING | A§6c | Not found. |
| Immersive Reader; Alt text auto generation (2 rows) | niche | OUT-OF-SCOPE | B§20 | Online services. |

### 22. Layout fidelity

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Line breaking | core | PARTIAL | WT/pagination.rs:126-160 | Matches Word for Calibri, Times New Roman, Arial; the Aptos paragraph is skipped and Aptos runs about 3 percent off, and Aptos is the font of every new document. |
| Break opportunities | core | PARTIAL | W/layout/paragraph.rs:389-396 | Spaces, hyphens, en and em dashes and slash before alphanumerics, ideographs, zero width space; no kinsoku rules. |
| Nonbreaking | core | DONE | W/layout/paragraph.rs:389,480 | U+00A0 is not a break point; `w:noBreakHyphen` is not. |
| Justification | core | DONE | W/layout/paragraph.rs:799+ | Last line and forced break lines not stretched. |
| Hyphenation | common | MISSING | W/layout/paragraph.rs:490-501 | None. |
| Tabs | core | DONE | W/layout/paragraph.rs:688-701 | Custom, default, hanging implicit stop, leaders; bar skipped. |
| Line height | core | DONE | W/props.rs:272-281 | Auto, atLeast, exact. |
| Spacing collapse | core | DONE | W/layout/mod.rs:677-682 | Larger of after and before, marked "measured against Word 16" in the source; B§22 says the sum (Part 5). |
| Widow and orphan control | core | DONE | W/layout/mod.rs:684-712 | |
| Keep with next, keep lines together | core | PARTIAL | W/layout/mod.rs:689-696 | Keep lines works; keep with next is never read, so no chain moves. |
| Page break before | core | DONE | W/layout/mod.rs:668-670 | |
| Floating object wrapping | core | PARTIAL | W/layout/mod.rs:798-826 | Position yes; no text wrap, no `allowOverlap` or `layoutInCell`; one draw order. |
| Anchored object position limits | common | MISSING | W/layout/mod.rs:798-826 | No clamping. |
| Table row breaking | core | PARTIAL | W/layout/mod.rs:5-6,717-737 | Header rows repeat; rows never split and a row taller than a page overflows. |
| Table width resolution | core | PARTIAL | W/layout/table.rs:472-496 | Grid and percent width; no autofit, `tcW` ignored. |
| Cell margins and spacing | common | PARTIAL | W/layout/table.rs:47-48 | `tblCellSpacing` unread. |
| Footnote placement | core | PARTIAL | W/layout/mod.rs:624-634,980-1038 | At the foot of the reference page; a note that does not fit does not continue. |
| Endnotes | common | PARTIAL | W/layout/mod.rs:739-770 | After the last body content only; no end of section placement. |
| Balanced columns | common | MISSING | W/layout/mod.rs:572-621 | No balancing. |
| Column breaks and widths | core | PARTIAL | W/layout/section.rs:125-144 | Separator line not drawn. |
| Section breaks | core | DONE | W/layout/mod.rs:572-606 | Next page, continuous, even, odd, next column. |
| Kerning | common | DONE | W/kern.rs | GPOS pair kerning when `w:kern` asks. |
| Character spacing | common | DONE | W/props.rs:557-565 | Spacing, scale, position; `fitText` not applied. |
| RTL and bidi | common | PRESERVED | W/props.rs:371,449 | Parsed, never used; no shaping (MANIFEST:13). |
| Drop caps | common | PRESERVED | A§3b | `framePr` kept. |
| Hyphenation settings | common | PRESERVED | A§3b | Kept. |
| Field result layout | core | PARTIAL | W/layout/mod.rs:393-394 | Page fields only in headers and footers; TOC page numbers come from layout when updated. |
| Header and footer fit | core | MISSING | W/layout/mod.rs:538-570 | Body top is the top margin; a taller header overlaps the body (TDD 194-195). |
| Section vertical alignment | common | PRESERVED | W/layout/section.rs:41-45 | Read, unused. |
| Text frames and text boxes | common | PRESERVED | W/model.rs:294-303 | Draws as a grey box without its text. |
| Font metrics and substitution | core | PARTIAL | S/fonts.rs | Installed fonts and Office cloud fonts; no `fontTable` substitution and embedded fonts unused. |
| Justification of character spacing; Compatibility settings; Compatibility mode by version; OpenType features; East Asian typography; Vertical text in table cells; Line numbering; Printer metrics; Pagination of floating tables (9 rows) | niche | PRESERVED | W/layout/mod.rs:371-379 | `w:compat` and the rest are kept and not read, so one set of layout rules applies to every file. |
| Empty paragraph and page break semantics | niche | NOT-DETERMINED | W/read.rs:337 | `splitPgBreakAndParaMark` is not read; effect not tested. |

### 21. Keyboard shortcuts

Only the shortcuts of B§21 that Unluminous lacks or binds differently, core and common. B gives no tier for shortcuts; the tier here is by how often a person presses the chord. Unluminous's bindings are in `C/keys.rs:154-241`; the window keeps Ctrl+S, W, Q, O, N, Tab, P and Comma (`C/keys.rs:22`). Chords Unluminous binds the same way as Word (Ctrl+B, I, U, E, L, R, J, 1, 2, 5, M, Shift+M, Z, Y, A, H, K, Space, Shift+N, Shift+8, Shift+C, Enter, Shift+Enter, Shift+Ctrl+Enter, Ctrl+arrows, Home, End, Tab and Shift+Tab in tables) are not listed.

| Word chord | Word action | Tier | Unluminous binding |
|---|---|---|---|
| Ctrl+Shift+V | Paste text only | core | Paste formatting (`C/keys.rs:183`); a foreign paste is already plain text |
| Ctrl+P | Print | core | Left to the window; there is no print |
| Ctrl+G | Go To | common | Not bound |
| Ctrl+Alt+M | Insert comment | core | Indent more (`(Key::M, false)` matches whatever Alt does, `C/keys.rs:168`) |
| Ctrl+Shift+E | Track changes on or off | core | Not bound (action `track-changes` has no chord) |
| Ctrl+Alt+1, 2, 3 | Heading 1, 2, 3 | core | Ctrl+Alt+1 and Ctrl+Alt+2 run single and double line spacing (`C/keys.rs:162-163` match first); headings need Ctrl+Shift+Alt+1 to 3 (`C/keys.rs:165-167`); Ctrl+Alt+3 does nothing |
| Ctrl+Alt+F | Insert footnote | core | Find (`(Key::F, false)` matches regardless of Alt) |
| Ctrl+Alt+D | Insert endnote | common | Not bound |
| Ctrl+Shift+L | Bulleted list | core | Not bound |
| Ctrl+Shift+A | All caps | common | Not bound |
| Ctrl+Shift+K | Small caps | common | Not bound |
| Ctrl+Shift+H | Hidden text | common | Not bound |
| Ctrl+Shift+D | Double underline | common | Not bound |
| Ctrl+Shift+W | Underline words only | common | Not bound |
| Shift+F3 | Toggle case | core | Not bound (case is in the panel) |
| Ctrl+D or Ctrl+Shift+F | Font dialog | core | Not bound; no dialog |
| Ctrl+] and Ctrl+[ | Grow and shrink by 1 pt | common | Bound to the size list step, not 1 pt (`C/keys.rs:177-178`, `S/actions.rs:91-106`) |
| Ctrl+Q | Remove paragraph formatting | common | The window's quit chord (`C/keys.rs:22`) |
| Ctrl+T and Ctrl+Shift+T | Hanging indent, remove hanging indent | common | Not bound |
| Ctrl+Shift+Space | Nonbreaking space | common | Not bound |
| Ctrl+- | Optional hyphen | common | Not bound |
| Ctrl+Shift+- | Nonbreaking hyphen | common | Not bound |
| Ctrl++ , Ctrl+- , Ctrl+0 | Zoom in, zoom out, 100% | common | Ctrl+Plus is superscript (`C/keys.rs:180`); Ctrl+- and Ctrl+0 not bound; zoom is Ctrl+wheel or the panel |
| F4 | Repeat last action | common | Not bound |
| Ctrl+Page Up and Ctrl+Page Down | Previous and next page | common | Not bound |
| Ctrl+F | Navigation pane | common | Opens the find bar (`C/keys.rs:173`) |
| Ctrl+Shift+S and Ctrl+Alt+Shift+S | Apply Styles, Styles pane | common | Not bound |
| Alt+Shift+D, Alt+Shift+T, Alt+Shift+P | Insert DATE, TIME, PAGE field | common | Not bound |
| Shift+F9 and Alt+F9 | Toggle field code | common | Not bound |
| Ctrl+Shift+F9 | Unlink field | common | Not bound |
| F9 | Update selected fields | core | Updates every TOC and date field in the document (`C/keys.rs:285`) |
| Alt+Home, Alt+End, Alt+Page Up, Alt+Page Down | First and last cell of row and column | common | Not bound |
| Shift+arrows across cells | Extend selection to adjacent cells | common | Selection is a single range; no cell selection |
| Shift+F10 and F6 | Context menu, move among panes | common | No context menu (`C/page_view.rs:619-627`) |
| Insert | Overtype | common | Not bound |

---

# Part 2. Interoperability (B§23)

Status words: MODELLED (read into the model and edited or drawn), PRESERVED (read as an XML tree or as part bytes and written back unchanged), DROPPED (the writer loses it, wraps it where it is not valid, or the reader damages it), NOT-DETERMINED.

The Fixture column names the files in `WT/fixtures/` (A§9c) that contain the construct. All 17 fixtures are saved unchanged and compared part by part and element by element in `every_fixture_saved_unchanged_keeps_every_part_and_element` (`WT/roundtrip.rs:94`), so any construct that Word 16 wrote into one of them is covered for an unchanged round trip. Every fixture was written by Word 16 through COM (`ORACLE/make-fixtures.ps1`); none was written by Google Docs, LibreOffice, Pages or python-docx. "none" means no fixture was built to contain it.

Read this part first for damage. Rows with DROPPED are the ones that can make Word report a damaged file or make content disappear.

## Findings that matter most

| Finding | Status | Evidence | Effect |
|---|---|---|---|
| A paragraph level `m:oMath`, `m:oMathPara`, `w:subDoc` or any other element the reader does not know at paragraph level is stored as a run's `Content::Object` and written back inside a new `<w:r>` | DROPPED | W/read.rs:269-278 (the `else` branch); W/write.rs:324-348 (`write_runs` always opens `w:r`) | Math is a paragraph level element in the schema, not a child of `w:r`. Word is likely to report unreadable content or discard the equation. Derived from code, not executed. No fixture contains an equation. |
| Setting a header or footer text from the Page level replaces the whole story with one paragraph | DROPPED | W/ops.rs:695-698 | A header holding a table, a picture, a text box or several paragraphs loses them. |
| `w:ruby` is kept inside its run but has no layout branch | PRESERVED, text not shown | W/read.rs:339; W/layout/paragraph.rs:516-527 | The file keeps the phonetic guide and its base text; the page shows neither (zero width). |
| `w:ptab` has no layout branch | PRESERVED, alignment lost | W/layout/paragraph.rs:516-527 | Word's three column headers and footers use it; the columns collapse. |
| Opening a `.dotx` or `.docm` and using Save As `.docx` keeps the old main part content type | NOT-DETERMINED | W/package.rs:208-215 is the only writer of content types and nothing rewrites the main part's; menus.rs:516-545 appends `.docx` | Word's reaction to a `.docx` name with a template or macro enabled content type was not tested. |
| A Strict OOXML file would be read with its own namespaces and written with the Transitional ones | NOT-DETERMINED | W/xml.rs:21-38 (canonical prefixes) and W/write.rs:118-136 (`root_start` skips a declared prefix that is canonical) | The root is rewritten to the Transitional URIs while relationship types keep their Strict URIs. Not run. |
| Tracked paragraph marks written by Enter under tracking all use `w:id="0"` | NOT-DETERMINED | W/edit.rs:255-263 | B§23.4 says duplicate ids can make Word report damage; Word's actual tolerance was not tested. |
| Deleting across paragraphs removes whole middle paragraphs including their bookmark and comment marks | NOT-DETERMINED | W/edit.rs:392-405 | Can leave a `bookmarkEnd`, `commentRangeEnd` or `commentReference` whose start or comment is gone. |

## 23.1 Document body and inline content

| Construct | Status | Evidence | Fixture | Note |
|---|---|---|---|---|
| `w:document` / `w:body` | MODELLED | W/read.rs:14-56 | all | Root attributes and namespace declarations kept (`Document.root`). A body with no `sectPr` gets an empty one written (W/write.rs:58-60). |
| `w:background` | MODELLED | W/read.rs:35; W/write.rs:53 | none | Kept whole; only `w:color` drawn. |
| `w:p` and its attributes | MODELLED | W/read.rs:234-244 | all | `w14:paraId` and `rsid*` kept; the new half of a split paragraph loses `paraId` and `textId` on purpose (W/edit.rs:265-270). |
| `w:pPr` children | MODELLED | W/props.rs:343-496 | paragraphs, styles | Typed subset; `framePr`, `suppressLineNumbers`, `textDirection`, `cnfStyle` and the rest stay in the tree. |
| `w:r` | MODELLED | W/read.rs:306-342 | formatting | A `w:r` with only an `rPr` and no content is not kept (harmless). Runs from one `w:r` are written as one again. |
| `w:rPr` children | MODELLED | W/props.rs:187-239,518-569 | formatting | Typed subset; `outline`, `shadow`, `emboss`, `bdr`, `w14:` effects, `eastAsianLayout` kept, not drawn. |
| `w:t` with `xml:space` | MODELLED | W/write.rs:352-366 | formatting | The attribute is re derived: written when the text starts or ends with white space or has two spaces in a row. |
| `w:delText` | MODELLED | W/read.rs:322; W/write.rs:337-339 | revisions | Chosen from the run being inside `w:del` or `w:moveFrom`. |
| `w:instrText` | MODELLED | W/read.rs:335 | fields | |
| `w:delInstrText` | MODELLED | W/read.rs:335; W/write.rs:340-342 | none | |
| `w:tab` | MODELLED | W/read.rs:336 | paragraphs | |
| `w:br` (page, column, textWrapping, `w:clear`) | MODELLED | W/read.rs:337; W/model.rs:253-257 | sections | Type read; the `clear` attribute is kept and not acted on. |
| `w:cr` | MODELLED | W/read.rs:337 | none | Drawn as a line break, written back as `w:cr`. |
| `w:sym` | MODELLED | W/model.rs:266-273 | none | The `F000` range maps down. |
| `w:noBreakHyphen` | MODELLED | W/model.rs:264; W/layout/paragraph.rs:480 | none | |
| `w:softHyphen` | MODELLED | W/model.rs:265; W/layout/paragraph.rs:492 | none | |
| `w:ptab` | PRESERVED | W/layout/paragraph.rs:516-527 | none | B says model. Zero width in layout. |
| `w:lastRenderedPageBreak` | DROPPED | W/read.rs:321 | any Word file | B allows this: Word regenerates it. Harmless. |
| `w:pgNum` | PRESERVED | W/layout/paragraph.rs:516-527 | none | Shows nothing; modern Word writes a PAGE field instead. |
| `w:annotationRef` | MODELLED | W/ops.rs:790; W/layout/paragraph.rs:492 | comments | |
| `w:footnoteRef`, `w:endnoteRef` | MODELLED | W/layout/paragraph.rs:566-575 | notes | Number drawn from the reference. |
| `w:separator`, `w:continuationSeparator` | MODELLED | W/layout/paragraph.rs:469; W/ops.rs:1480-1513 | notes | Separator drawn as a rule; the continuation separator is never used. |
| `w:dayShort` and the legacy date components | PRESERVED | W/read.rs:339 | none | |
| `w:fldSimple` | MODELLED | W/read.rs:203 | fields | Wrapper; stored result drawn. |
| `w:fldChar` with `w:fldData`, `w:ffData`, `w:numberingChange` | MODELLED | W/read.rs:338 | fields | Child data stays inside the kept element. |
| Field nesting | MODELLED | W/layout/paragraph.rs:236-300 | fields | |
| `w:hyperlink` | MODELLED | W/model.rs:331; W/ops.rs:993-1033 | fields, kitchen-sink | `tooltip`, `tgtFrame`, `history`, `docLocation` kept, not used. |
| `w:bookmarkStart` / `w:bookmarkEnd` | MODELLED | W/read.rs:212-231; W/ops.rs:1036 | fields | `colFirst` and `colLast` kept. A bookmark between table rows is kept in `Table.extra`. |
| `w:commentRangeStart` / `End` / `commentReference` | MODELLED | W/read.rs:215-216; W/ops.rs:770-852 | comments | |
| `w:footnoteReference` / `w:endnoteReference` | MODELLED | W/ops.rs:706-760; W/layout/mod.rs:343-368 | notes | `customMarkFollows` references are not counted. |
| `w:ins` / `w:del` (run and paragraph mark) | MODELLED | W/edit.rs:16-40,805-890 | revisions | |
| `w:ins` / `w:del` at row and cell level (`trPr`, `cellIns`) | PRESERVED | W/read.rs:176-186 | none | Not read; shown as ordinary rows. |
| `w:moveFrom` / `w:moveTo` and the range markers | MODELLED | W/read.rs:197-231; W/edit.rs:96 | none | Shown as delete and insert; ranges are kept marks. |
| `w:rPrChange`, `w:pPrChange` | MODELLED | W/edit.rs:849-889 | revisions | Accept and reject restore the old properties. |
| `w:sectPrChange`, `w:tblPrChange`, `w:tblPrExChange`, `w:trPrChange`, `w:tcPrChange`, `w:tblGridChange`, `w:numberingChange` | PRESERVED | W/edit.rs:805-890 | none | Not resolved by Accept all or Reject all; they stay in the file. |
| `w:cellIns`, `w:cellDel`, `w:cellMerge` | PRESERVED | W/read.rs:176-186 | none | |
| `w:customXmlInsRangeStart` and related | PRESERVED | W/read.rs:223-230 | none | Kept as marks. |
| `w:sdt` block level | MODELLED | W/read.rs:138-149; W/write.rs:170-185 | fields (TOC) | Content laid out and editable as ordinary blocks; control properties kept. A `w:sdt` with no `sdtContent` is kept whole. |
| `w:sdt` inline | MODELLED | W/read.rs:286-293 | none | A wrapper on the runs; an empty one is kept as a mark. |
| `w:sdt` at row or cell level | PRESERVED | W/read.rs:175-186 | none | Held in `Row.extra` and `Table.extra`; not laid out, so those rows or cells do not draw. |
| `sdtPr` children (alias, tag, lock, placeholder, dataBinding, checkbox, date, dropDownList, repeatingSection and the rest) | PRESERVED | W/write.rs:170-185 | fields (docPartObj only) | No control behaviour. |
| `w:tbl`, `w:tblGrid`, `w:tr`, `w:tc` | MODELLED | W/read.rs:160-194; W/write.rs:191-240 | tables | A cell with no final paragraph gets `<w:p/>` (W/write.rs:225-229). |
| `w:tblPr` children | MODELLED | W/layout/table.rs | tables | Read: `tblStyle`, `tblW` (percent), `jc`, `tblInd`, `tblBorders`, `tblCellMar`, `tblLook`, `shd`. Kept and unread: `tblpPr`, `tblOverlap`, `bidiVisual`, `tblCellSpacing`, `tblLayout`, `tblCaption`, `tblDescription`. |
| `w:trPr` children | MODELLED | W/layout/table.rs:353-359,465 | tables | Read: `trHeight`, `tblHeader`, `gridBefore`. Kept and unread: `gridAfter`, `wBefore`, `wAfter`, `cantSplit`, `tblCellSpacing`, `hidden`, `jc`. |
| `w:tcPr` children | MODELLED | W/layout/table.rs:286-326,413-459 | tables | Read: `gridSpan`, `vMerge`, `tcBorders`, `shd`, `vAlign`, `tcMar`. Kept and unread: `tcW`, `hMerge`, `noWrap`, `textDirection`, `tcFitText`, `hideMark`, diagonal borders. |
| `tcW` and `tblW` units | PRESERVED | W/layout/table.rs:472-496 | tables | Percent `tblW` rescales the grid; `dxa` and `tcW` are ignored when a grid exists. |
| `w:tblLook` | MODELLED | W/layout/table.rs:80 | tables | |
| `vMerge` restart and continue | MODELLED | W/layout/table.rs:363-377 | tables | |
| `w:sectPr` children | MODELLED | W/layout/section.rs:51-144 | sections, headers | Used: `pgSz`, `pgMar`, `cols`, `type`, `titlePg`, header and footer references, `pgNumType start`. Kept and unread: `footnotePr`, `endnotePr`, `pgBorders`, `lnNumType`, `paperSrc`, `formProt`, `vAlign`, `noEndnote`, `textDirection`, `bidi`, `rtlGutter`, `docGrid`, `printerSettings`, `cols sep`. |
| Section break placement | MODELLED | W/layout/mod.rs:413-425; W/edit.rs:249-250,347-349 | sections | Moves with the paragraph on split and join. |
| `pBdr`, `tcBorders`, `tblBorders` | MODELLED | W/layout/table.rs:413-459 | paragraphs, tables | Single and double drawn; `shadow`, `frame`, art borders kept, not drawn. |
| `w:shd` | MODELLED | W/props.rs | paragraphs, tables | Fill only; pattern ignored. |
| `w:framePr` | PRESERVED | W/order.rs:17 | none | B says model. |
| `w:permStart` / `w:permEnd` | PRESERVED | W/read.rs:217-218 | none | Marks. |
| `w:proofErr` | DROPPED | W/read.rs:269-270 | any Word file | B allows this: Word regenerates it. |
| `w:smartTag`, `w:customXml` | PRESERVED | W/read.rs:204-205,297-300 | none | Wrapper kept with its `Pr` child. |
| `w:ruby`, `w:rubyPr`, `w:rt`, `w:rubyBase` | PRESERVED | W/read.rs:339; W/layout/paragraph.rs:516-527 | none | B says model. Text is not drawn. |
| `w:fitText`, `w:eastAsianLayout`, `w:em` | PRESERVED | W/props.rs:518-569 | none | |
| `w:bdo` / `w:dir` | PRESERVED | W/read.rs:206-207 | none | Wrapper kept, no direction effect. |
| `w:subDoc` | DROPPED | W/read.rs:269-278; W/write.rs:324-348 | none | Paragraph level element written inside a new `w:r`; see the finding above. |
| `w:altChunk` | PRESERVED | W/read.rs:154 | none | Block level `Kept`; the relationship and the chunk part are in `Package`. |
| `w:object`, `w:control` | PRESERVED | W/read.rs:339 | none | One zero width object. |
| `w:pict` | PRESERVED | W/layout/paragraph.rs:588-612 | none | Drawn as an inline image when it has `v:imagedata`, sized from `style`. |
| `w:txbxContent` | PRESERVED | W/model.rs:294-303 | none | B says model. The text sits in a kept drawing and is never read or drawn. |
| `w:glossaryDocument` | PRESERVED | W/package.rs | none | Separate part, byte for byte. |
| `m:oMath`, `m:oMathPara` | DROPPED | W/read.rs:269-278; W/write.rs:324-348 | none | Wrapped in `w:r` on write (see the finding above); also zero width on screen. |

## 23.2 Drawing, math and alternate content

| Construct | Status | Evidence | Fixture | Note |
|---|---|---|---|---|
| `w:drawing` | MODELLED | W/layout/paragraph.rs:588-653 | pictures, kitchen-sink | |
| `wp:inline` | MODELLED | W/layout/paragraph.rs:619-621; W/media.rs:94 | pictures | `docPr` ids are unique (W/media.rs:74-90). |
| `wp:anchor` | MODELLED | W/layout/paragraph.rs:622-651 | pictures | Position and size only. `wrapSquare`, `wrapTight`, `wrapThrough`, `wrapTopAndBottom`, `behindDoc`, `allowOverlap`, `relativeHeight`, `distT` and the rest are kept and not used. |
| `a:graphic` / `a:graphicData` | PRESERVED | W/layout/paragraph.rs | pictures | Only a `pic:pic` with a blip is drawn. |
| `pic:pic` with `a:blip`, `a:stretch` | MODELLED | W/layout/paragraph.rs:589-599 | pictures | |
| `a:srcRect` crop | PRESERVED | A§1b | none | Not applied. |
| `a:blip` extensions (`svgBlip`, `a14:imgProps`, effects) | PRESERVED | W/xml.rs | none | |
| `wps:wsp`, `wps:txbx`, `wps:linkedTxbx`, `wps:bodyPr` | PRESERVED | W/model.rs:294-303 | none | Draws as a grey outlined box without the text. |
| `wpg:wgp` | PRESERVED | W/model.rs:294-303 | none | Grey box. |
| `wpc:wpc` | PRESERVED | W/model.rs:294-303 | none | |
| `mc:AlternateContent` / `Choice` / `Fallback` | PRESERVED | W/read.rs:339; W/model.rs:297-300 | none known | Both branches written back. Layout takes the first `w:drawing` or `w:pict` in either branch, Choice first. |
| `mc:Ignorable` | PRESERVED | W/xml.rs:339-353; W/write.rs:118-143 | all | Root attributes kept; every non canonical namespace declared on the root is declared again. |
| VML shapes (`v:shape`, `v:textbox`, `o:lock`, `w10:wrap` and the rest) | PRESERVED | W/layout/paragraph.rs:655-670 | none | Position of an anchored VML shape ignored. |
| Watermark (VML WordArt in a header) | PRESERVED | A§11#35 | none | Draws as a grey box on every page. |
| Charts (`c:chart`, chart parts, embedded workbook) | PRESERVED | W/package.rs | none | Grey box. |
| SmartArt (`dgm:relIds`, data, layout, colors, drawing parts) | PRESERVED | W/model.rs:294-303 | none | Grey box; the cached drawing is not used. |
| 3D model, ink | PRESERVED | W/package.rs | none | |
| `a:ext`, `a:extLst` | PRESERVED | W/xml.rs | all | |
| `docPr hlinkClick` | PRESERVED | W/media.rs:94 | none | |

## 23.3 Package parts

| Part | Status | Evidence | Fixture | Note |
|---|---|---|---|---|
| `[Content_Types].xml` | MODELLED | W/package.rs:136-183,288-307 | all | Rewritten only when an override or default is added. |
| `_rels/.rels` | MODELLED | W/package.rs:161-181 | all | |
| `word/document.xml` | MODELLED | W/write.rs:49-65 | all | Always written from the model. |
| `word/_rels/document.xml.rels` | MODELLED | W/package.rs:86-100 | all | New ids are `rIdU<n>`. Strict relationship types match by their last segment. |
| `word/styles.xml` | MODELLED | W/styles.rs; W/write.rs:19-26 | styles, builtin-styles | Rewritten only when dirty. `latentStyles` and `w:link` kept. |
| `word/stylesWithEffects.xml` | PRESERVED | W/package.rs | none | |
| `word/numbering.xml` | MODELLED | W/numbering.rs | lists | `lvlPicBulletId` and `numPicBullet` kept, not drawn. |
| `word/settings.xml` | MODELLED | W/session.rs:97-100,567; W/layout/mod.rs:371-379 | all | Read: `defaultTabStop`, `evenAndOddHeaders`, `trackRevisions` (also written). `compat`, `documentProtection`, `rsids`, `updateFields`, `mailMerge`, `autoHyphenation` kept. |
| `word/fontTable.xml` | PRESERVED | W/package.rs | all | B says model; not used for substitution. |
| `word/fonts/*.odttf` | PRESERVED | W/package.rs | none | Not de obfuscated, not used for layout. |
| `word/theme/theme1.xml` | MODELLED | W/styles.rs:21-110 | all | |
| `word/webSettings.xml` | PRESERVED | W/package.rs | all | |
| `word/footnotes.xml`, `word/endnotes.xml` | MODELLED | W/read.rs:100-113; W/ops.rs:1453-1520 | notes | |
| `word/comments.xml` | MODELLED | W/read.rs:100-113; W/ops.rs:770-990 | comments | |
| `word/commentsExtended.xml` | MODELLED | W/ops.rs:1561-1660 | comments | Done flag and parent maintained; the override is added when the part is made. |
| `word/commentsIds.xml` | PRESERVED | W/ops.rs:1645 (only `commentsExtended` is touched) | comments | Not updated when comments are added or deleted, so entries for deleted comments stay and new comments have none. |
| `word/commentsExtensible.xml` | PRESERVED | same | comments | Same. |
| `word/people.xml` | PRESERVED | W/package.rs | comments | New authors are not added. |
| `word/header*.xml`, `word/footer*.xml` | MODELLED | W/read.rs:76-88; W/write.rs:27-32 | headers | |
| `word/glossary/document.xml` and its parts | PRESERVED | W/package.rs | none | |
| `customXml/item*.xml`, `itemProps*.xml` | PRESERVED | W/package.rs | none | |
| Bibliography part | PRESERVED | W/package.rs | none | B says model. |
| `docProps/core.xml` | MODELLED | W/read.rs:89-96; S/commands.rs:353-376 | all | Save writes `lastModifiedBy`, `modified`, `creator` when empty. |
| `docProps/app.xml` | PRESERVED | W/package.rs | all | Page and word counts go stale; Word recalculates them. |
| `docProps/custom.xml` | PRESERVED | W/package.rs | none | B says model. |
| `docProps/thumbnail.jpeg` | PRESERVED | W/package.rs | none | Goes stale after an edit. |
| `word/vbaProject.bin`, `vbaData.xml`, `vbaProjectSignature.bin` | PRESERVED | W/write.rs:16-39 | none | `.docm` has no fixture. |
| `word/media/*` | MODELLED | W/media.rs:28 | pictures | PNG, JPEG, GIF, BMP, WebP, TIFF, ICO decode; EMF, WMF, SVG kept and drawn as a box. |
| `word/embeddings/*`, `word/charts/*`, `word/diagrams/*`, `word/ink/*`, `word/printerSettings/*`, `word/activeX/*` | PRESERVED | W/package.rs | none | |
| `_xmlsignatures/` | PRESERVED | W/package.rs | none | Kept; any edit invalidates the signature, as B says. |
| Encrypted package (CFB with `EncryptedPackage`) | DROPPED (open refused) | W/zip.rs:84-91 | none (unit test `an_old_word_file_is_named_rather_than_called_damaged`) | The file is not opened, so nothing is lost; B lists this as PRESERVED. |
| Relationship `TargetMode` | MODELLED | W/package.rs:174-178 | fields | |
| Relationship types | MODELLED | W/package.rs:62-67 | all | Matched by the last path segment. |
| Strict versus Transitional | NOT-DETERMINED | W/xml.rs:307-316; W/write.rs:118-136 | none | See the finding above. |
| Flat OPC | DROPPED (open refused) | W/zip.rs | none | Not a zip, so refused. |
| Zip handling | MODELLED | W/zip.rs; test `a_stub_in_front_of_the_archive_still_reads` | all | zip64 and encrypted entries refused (W/zip.rs:53,75). Part names compare exactly, not case insensitively (W/package.rs:186). |

## 23.4 Round trip rules

| Rule | Status | Evidence | Fixture | Note |
|---|---|---|---|---|
| Keep every unknown element, attribute and part | PRESERVED, with the exceptions above | W/read.rs:154,339; W/write.rs:16-39 | all | Exceptions are the DROPPED rows in 23.1. |
| Keep both branches of `mc:AlternateContent` | PRESERVED | W/read.rs:339 | none known | |
| Keep relationship ids stable | MODELLED | W/package.rs:86-100 | all | New ids cannot equal Word's. |
| Keep `w14:paraId` consistent with `commentsExtended` | MODELLED | W/ops.rs:1561-1660; W/edit.rs:265-270 | comments | A joined paragraph whose id a comment used is not re mapped. |
| Keep styles referenced by id | MODELLED | W/styles.rs | styles | |
| Do not collapse runs with different rsid | NOT-DETERMINED | W/model.rs:440-461 | none | `merge_runs` compares properties and wraps, not `rsid` attributes, after an edit. Harmless to Word. |
| Regenerate cached field results only on update | MODELLED | W/ops.rs:1212-1250 | fields | |
| Preserve `proofErr` and `lastRenderedPageBreak` or drop them deliberately | MODELLED | W/read.rs:269-270,321 | any Word file | Dropped deliberately. |
| Keep compat settings | PRESERVED | W/layout/mod.rs:371-379 | all | Kept; layout does not read them. |
| Keep theme part and `clrSchemeMapping` | PRESERVED | W/package.rs | all | |
| Update or leave `docProps/app.xml` | PRESERVED | W/package.rs | all | Left. |
| Unique ids across tracked changes | NOT-DETERMINED | W/edit.rs:27,255-263 | revisions | Run changes use `next_revision_id`; a split paragraph mark under tracking uses `0`. |
| Element order in `pPr`, `rPr`, `tblPr`, `sectPr` | MODELLED | W/order.rs; test `children_added_at_the_end_are_put_in_the_schemas_place` | all | Sorted on write. |
| Table cells end with a paragraph; body ends with `sectPr` | MODELLED | W/write.rs:58-60,225-229 | tables | |

---

# Part 3. Counts

Counts are rows of B (a grouped row counts as its N rows). Categories 19 and 21 are not counted: 19 is skipped by the brief and 21 has no tiers in B. ND = NOT-DETERMINED, OOS = OUT-OF-SCOPE. Category 23 is counted in Part 2.

| Category | core DONE | core PARTIAL | core PRESERVED | core MISSING | core ND | core OOS | common DONE | common PARTIAL | common PRESERVED | common MISSING | common ND | common OOS |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1. File | 5 | 4 | 0 | 5 | 1 | 0 | 0 | 5 | 2 | 17 | 2 | 1 |
| 2. Edit and selection | 3 | 8 | 0 | 2 | 0 | 0 | 2 | 3 | 0 | 14 | 0 | 0 |
| 3. Fonts and character formatting | 8 | 7 | 0 | 0 | 0 | 0 | 5 | 7 | 5 | 0 | 1 | 0 |
| 4. Paragraph | 12 | 3 | 0 | 1 | 0 | 0 | 3 | 8 | 4 | 1 | 1 | 0 |
| 5. Styles and themes | 6 | 4 | 0 | 1 | 0 | 0 | 0 | 6 | 2 | 5 | 0 | 0 |
| 6. Lists | 5 | 4 | 0 | 1 | 0 | 0 | 1 | 2 | 1 | 5 | 0 | 0 |
| 7. Tables | 7 | 11 | 0 | 5 | 0 | 0 | 1 | 9 | 4 | 10 | 0 | 0 |
| 8. Illustrations | 2 | 5 | 2 | 3 | 0 | 0 | 1 | 4 | 12 | 13 | 0 | 0 |
| 9. Page layout and sections | 6 | 9 | 0 | 1 | 0 | 0 | 1 | 5 | 7 | 1 | 0 | 0 |
| 10. Headers, footers and page numbers | 4 | 7 | 0 | 0 | 0 | 0 | 0 | 5 | 1 | 4 | 0 | 0 |
| 11. References | 0 | 8 | 0 | 1 | 0 | 0 | 0 | 3 | 3 | 6 | 0 | 0 |
| 12. Fields | 5 | 5 | 3 | 1 | 0 | 0 | 2 | 1 | 17 | 2 | 0 | 0 |
| 13. Mailings | 0 | 1 | 0 | 3 | 0 | 0 | 0 | 0 | 1 | 13 | 0 | 0 |
| 14. Review | 3 | 5 | 1 | 4 | 0 | 0 | 2 | 3 | 0 | 7 | 0 | 2 |
| 15. View | 1 | 3 | 0 | 4 | 0 | 0 | 1 | 1 | 0 | 4 | 0 | 0 |
| 16. Insert, other | 2 | 5 | 0 | 0 | 0 | 0 | 1 | 2 | 1 | 6 | 1 | 0 |
| 17. Forms and controls | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 7 | 1 | 0 | 0 |
| 18. Proofing and automation | 0 | 3 | 0 | 4 | 0 | 0 | 0 | 3 | 0 | 12 | 0 | 0 |
| 20. Accessibility and navigation | 4 | 2 | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 4 | 1 | 1 |
| 22. Layout fidelity | 8 | 10 | 0 | 1 | 0 | 0 | 2 | 2 | 5 | 3 | 0 | 0 |
| Total | 81 | 104 | 7 | 37 | 2 | 0 | 22 | 69 | 73 | 128 | 6 | 4 |

| Tier | Rows | DONE | PARTIAL | PRESERVED | MISSING | ND | OOS |
|---|---|---|---|---|---|---|---|
| core | 231 | 81 (35%) | 104 (45%) | 7 (3%) | 37 (16%) | 2 | 0 |
| common | 302 | 22 (7%) | 69 (23%) | 73 (24%) | 128 (42%) | 6 | 4 |
| niche | 300 | 3 | 12 | 148 | 109 | 1 | 27 |
| all | 833 | 106 | 185 | 228 | 274 | 9 | 31 |

Reading the totals: 141 of the 231 core rows (61%) are PARTIAL or MISSING. Of the 73 common rows that are PRESERVED, 17 are fields that show a stored result and never update, and 12 are picture, shape and chart properties that are kept and not applied. The 148 niche rows that are PRESERVED are kept in the file and not acted on.

Category 23 (Part 2) has 6 DROPPED rows: `m:oMath` with `m:oMathPara`, and `w:subDoc`, which the writer puts inside a `w:r` where the schema does not allow them; `lastRenderedPageBreak` and `proofErr`, which B allows to be dropped; and the encrypted package and flat OPC, which are refused at open so nothing is lost. It has 3 NOT-DETERMINED rows (Strict namespaces, run merging against `rsid`, tracked change ids) and the findings table at the top of Part 2 adds two more NOT-DETERMINED items that have no row in 23.x: the content type left on a `.dotx` or `.docm` saved as `.docx`, and delete across paragraphs. Separately from category 23, setting header text from the Page level deletes whatever else the header held (D2 in Part 4).

---

# Part 4. Ranked gap list

Buckets: (a) document model and round trip, (b) layout fidelity, (c) editing operations, (d) UI controls and chords, (e) agent verbs, (f) tests and oracle. Items in one line share a cause. A feature name in a line is the name used in B; the section number is in the Cat column. Order within each block is by how often a business or school document reaches the feature, most often first. Where a layout gap also has no test, (f) is added.

Agent verbs (e): the verb list (A§7) mirrors the UI one to one, so a gap in an operation is a gap in the verbs too unless it says otherwise. Two verbs have no UI: `insert field <code>` and `section header-first` and the other first and even header forms. Neither UI nor verb can edit a footnote's text after insertion, resize or crop a picture, modify a style, or update REF, SEQ or HYPERLINK fields (A§7).

## 4.0 Round trip damage and data loss found in Part 2

| Rank | Cat | Item | Status | Why it matters | Bucket |
|---|---|---|---|---|---|
| D1 | 23 | Equations (`m:oMath`, `m:oMathPara`) and `w:subDoc` written inside a new `w:r` | DROPPED | A saved file with an equation can be reported as unreadable by Word; the equation is also invisible on screen | (a)(f) |
| D2 | 10 | Setting header or footer text replaces the story with one paragraph (W/ops.rs:695-698) | DROPPED | Opening a letterhead and setting its text deletes the logo and table in it | (c)(a) |
| D3 | 23 | `.dotx` or `.docm` saved with Save As `.docx` keeps its content type | NOT-DETERMINED | A person saving a template as a document may produce a file Word refuses | (a)(f) |
| D4 | 23 | Delete across paragraphs removes middle paragraphs with their bookmark and comment marks | NOT-DETERMINED | Can leave a comment reference or bookmark end with nothing to match | (a)(c) |
| D5 | 23 | Enter under Track Changes writes `w:id="0"` on every paragraph mark | NOT-DETERMINED | Duplicate revision ids; Word's tolerance untested | (a) |
| D6 | 23 | Strict OOXML files are rewritten with Transitional root namespaces | NOT-DETERMINED | A file saved by Word as Strict may not open in Word after a save | (a)(f) |
| D7 | 23 | `w:ptab` and `w:ruby` draw as zero width | PRESERVED | Three column footers collapse; Japanese phonetic text is invisible | (b) |
| D8 | 23 | `commentsIds.xml` and `commentsExtensible.xml` are not updated | PRESERVED | Stale entries for deleted comments; Word is expected to tolerate it | (a) |

## 4.1 Core tier

| Rank | Cat | Feature | Status | Why it matters | Bucket |
|---|---|---|---|---|---|
| 1 | 22 | Table row breaking | PARTIAL | Every long table paginates differently from Word because rows never split, and a row taller than a page overflows | (b)(f) |
| 2 | 8, 22 | Floating object wrapping; Wrap Text: Square; Wrap Text: Top and Bottom; Wrap Text: In Front of Text | PARTIAL | Text runs under pictures that Word wraps around; hits letterheads, flyers, reports with figures | (b)(f) |
| 3 | 4, 22 | Keep with next (Cat 4); Keep with next, keep lines together (Cat 22) | PARTIAL | A heading is left alone at the foot of a page; every Word heading style sets it | (b)(f) |
| 4 | 22 | Header and footer fit | MISSING | A header taller than the top margin overlaps the first body line | (b) |
| 5 | 22 | Font metrics and substitution; Line breaking; Break opportunities | PARTIAL | Aptos, the font of every new document, wraps about 3 percent differently, and its test case is skipped | (b)(f) |
| 6 | 7, 22 | Table width resolution; Table layout algorithm; AutoFit Contents (MISSING); AutoFit Window; Fixed Column Width | PARTIAL | Word tables with `tcW` or autofit columns get other widths, so wrapping and page counts differ | (b) |
| 7 | 11 | Insert Footnote; Insert Endnote | PARTIAL | The action makes an empty note and a pointer cannot enter a note, so a person cannot write a footnote in the window | (c)(d) |
| 8 | 22 | Footnote placement | PARTIAL | A note that does not fit is not continued, which moves later text | (b) |
| 9 | 11, 12 | Table of Contents: Automatic Table 1, 2; Update Table; TOC field; TOC | PARTIAL | A TOC that Word wrote is inside a content control and is not found on update; one made here is not a Word TOC | (c)(a)(f) |
| 10 | 12, 22 | PAGE; NUMPAGES; Field result layout | PARTIAL | A page number in the body keeps the value Word stored | (b) |
| 11 | 2 | Paste: Keep Source Formatting; Paste: Merge Formatting; Paste: Keep Text Only | PARTIAL | Text copied from Word, a web page or a PDF arrives without its formatting; only a copy within Unluminous keeps it | (c) |
| 12 | 1 | Print; Print ranges; Print preview | MISSING | Documents are printed; the only output is a PDF export | (c)(d) |
| 13 | 11, 16, 20 | Hyperlink (Ctrl+K); Link (Hyperlink); Hyperlink styles; Hyperlink text | PARTIAL | A link cannot be followed, edited with a screen tip or removed | (c)(d) |
| 14 | 2 | Selection by mouse; Drag and drop text | PARTIAL | No left margin selection, no sentence selection, no moving text by dragging | (c) |
| 15 | 18 | AutoFormat As You Type: Straight quotes with smart quotes; AutoFormat As You Type: Internet and network paths with hyperlinks; AutoCorrect: Replace text as you type; AutoCorrect: Capitalize first letter of sentences | MISSING | Typed text keeps straight quotes, bare URLs and no capitals; every Word user expects the corrections | (c) |
| 16 | 6, 18 | Automatic bullets and numbering as you type; AutoFormat As You Type: Automatic bulleted lists; AutoFormat As You Type: Automatic numbered lists; Bullets, Numbering, Multilevel List buttons | PARTIAL | Word's triggers are Enter, not Space, and only `1.` starts a number list | (c)(d) |
| 17 | 6 | Number formats; Multilevel List; Continue numbering (MISSING); numbering.xml model | PARTIAL | Legal and outline numbering from Word print as decimal; a list cannot be joined to the one above | (a)(c) |
| 18 | 8 | Pictures: This Device; Lock aspect ratio (MISSING); Picture Format tab (MISSING) | PARTIAL | A picture cannot be selected, resized or cropped after insert; EMF, WMF, SVG do not draw | (c)(d)(a) |
| 19 | 7 | Select cell, row, column, table (MISSING); Merge Cells (PARTIAL); Split Cells (PARTIAL); Delete cells, rows, columns, table (PARTIAL) | MISSING | No multi cell selection, so merge and delete work one cell at a time | (c)(d) |
| 20 | 7 | Resize by dragging borders (MISSING); Cell Size group; Distribute Rows Evenly, Distribute Columns Evenly (MISSING) | PARTIAL | Column widths can only be set by the `table width` verb | (c)(d)(e) |
| 21 | 7 | Table borders; Table shading; View Gridlines (MISSING) | PARTIAL | No border or row shading controls; a borderless layout table has no guide lines | (c)(d) |
| 22 | 7 | Insert Table dialog; Table Design and Layout tabs | PARTIAL | Rows and columns only; contextual controls are a panel list | (d) |
| 23 | 5 | Modify Style | MISSING | Styles cannot be changed, so a template's look cannot be adjusted | (c)(d) |
| 24 | 5 | Styles pane; Quick Styles gallery; Built in styles | PARTIAL | No previews; the built in set lacks Body Text, Index and others | (d)(a) |
| 25 | 5 | Style property resolution | PARTIAL | A table style's own fonts and bold header row are not applied | (b)(a) |
| 26 | 9, 10 | Different First Page (Cat 9 and 10); Different Odd and Even (Cat 9); Link to previous; Link to Previous (Cat 10); Page numbering per section; Per section settings; Page Setup dialog | PARTIAL | First page and even page headers, link to previous and page number start are set by verb or not at all | (d)(e) |
| 27 | 10, 16 | Insert Header, Insert Footer; Remove Header, Remove Footer; Header & Footer tab; Page Number gallery; Pictures in header; Header, Footer, Page Number (Cat 16) | PARTIAL | One text box per header; no gallery, no removal, no clear way to put a logo in | (c)(d) |
| 28 | 9 | Margin presets; Size; Columns presets | PARTIAL | No Mirrored or custom sizes; no left and right column presets | (d) |
| 29 | 22 | Column breaks and widths | PARTIAL | Column separator line not drawn | (b) |
| 30 | 14 | Display for Review (MISSING); Next and Previous change (MISSING); Review tab shortcuts (MISSING) | MISSING | A reviewer cannot hide markup or step through changes; Ctrl+Alt+M is bound to indent | (c)(d) |
| 31 | 14 | Delete Comment; Previous and Next comment (MISSING) | PARTIAL | No delete all, no stepping through comments | (c)(d) |
| 32 | 14 | Spelling & Grammar (Editor); Red squiggle spelling, blue double underline grammar; Right click spelling menu; Check spelling as you type, Mark grammar errors as you type; Word Count | PARTIAL | No grammar; the word count leaves out notes and text boxes | (c) |
| 33 | 11 | Cross-reference (MISSING) | MISSING | References to a figure, table or heading cannot be inserted | (c)(e) |
| 34 | 11, 16, 8 | Bookmark; Bookmark, Cross-reference; Insert Caption | PARTIAL | No bookmark delete; captions are not renumbered | (c) |
| 35 | 12 | Update fields (F9); Switch `\@ "dddd, MMMM d, yyyy"` | PARTIAL | Dates with weekday names or 12 hour times cannot be updated; REF, SEQ, PAGEREF never update | (c)(e) |
| 36 | 12 | Toggle field codes | MISSING | A field's code cannot be seen | (d) |
| 37 | 1 | Open .rtf (MISSING); Open .txt (MISSING) | MISSING | Common interchange files do not open in the Word plugin | (a) |
| 38 | 1 | Save as .docx, .docm, .dotx, .dotm, .doc, .rtf, .odt, .txt, .htm, .xml; Export to PDF; New from template; Options dialog | PARTIAL | `.docx` only; PDF has no outline or tags | (a)(d) |
| 39 | 3 | Font name; Font color; Underline style; Text Highlight Color; Font dialog (Ctrl+D); Insert Symbol dialog; Formatting marks | PARTIAL | No theme fonts or colours, 12 of 16 highlights, 24 symbols, no tab arrows | (d) |
| 40 | 4 | Tabs dialog: stop position; Ruler tab selector (MISSING) | PARTIAL | No leader choice, no ruler to place tabs or indents | (d) |
| 41 | 4, 9, 15 | Ruler tab selector; Gridlines and ruler; Ruler; Show: Ruler, Gridlines, Navigation Pane | MISSING | No ruler | (d) |
| 42 | 15 | Navigation Pane; Zoom dialog; One Page, Multiple Pages, Page Width (MISSING); Zoom slider (MISSING); Page thumbnails (MISSING) | PARTIAL | No fit to width or page, no thumbnails | (d) |
| 43 | 2 | Undo, Redo, Repeat; Find (Navigation pane); Navigation pane tabs; Advanced Find; Format Painter | PARTIAL | No F4 repeat, no results list, two find options, no painter button | (c)(d) |
| 44 | 8, 16 | Text Box: Draw; Table, Pictures, Shapes, Icons, 3D Models, SmartArt, Chart, Screenshot (Cat 16: only table and picture can be inserted) | MISSING | Cannot make a text box; existing ones draw as a grey box without their text | (a)(c)(f) |
| 45 | 13 | Insert Merge Field; Preview Results; Finish & Merge: Edit Individual Documents; Finish & Merge: Print Documents | MISSING | Mail merge is out of scope (PRD 244-248) | (c) |
| 46 | 20 | Keyboard navigation | PARTIAL | No F6, KeyTips or context menu key | (d) |

## 4.2 Common tier

| Rank | Cat | Feature | Status | Why it matters | Bucket |
|---|---|---|---|---|---|
| 1 | 16 | Equation; Equation tools | MISSING | Maths and science documents; existing equations are invisible and are the D1 damage | (a)(c)(f) |
| 2 | 7, 5 | Table Properties: Table tab; Table Options; Table Properties: Row tab; Table Properties: Column tab; Table Properties: Cell tab; Cell alignment grid; Cell Margins; Table look; Table style; Table style options; Conditional table formatting | PARTIAL | Read from Word files and drawn in part; cannot be edited, and banded and first column formatting is not applied | (b)(d) |
| 3 | 7, 5 | Table Styles gallery; Clear table style; Table caption (PARTIAL) | MISSING | New tables are plain Table Grid | (d) |
| 4 | 7 | Table move handle; Table resize handle; Split Table; Draw Table; Convert Text to Table; Convert Table to Text; Sort (table); Update table formulas | MISSING | Splitting a table, converting text to a table and sorting rows are routine in reports and lists | (c)(d) |
| 5 | 8, 22 | Wrap Text: Tight; Layout dialog: Position; Layout dialog: Size; Anchor, Lock Anchor; Position presets (MISSING); Align (MISSING); Selection Pane (MISSING); Anchored object position limits (MISSING) | PARTIAL | A placed picture cannot be moved, sized or wrapped from the window | (c)(d)(b) |
| 6 | 1 | Open .doc; Open .odt; Open PDF (PDF Reflow); Open .htm, .html, .mht; Protected View | MISSING | Attachments arrive as .doc, .odt, .pdf and .html and cannot be opened for editing | (a) |
| 7 | 1 | Normal.dotm; Templates .dotx | PARTIAL | No user template; a template opens as a document | (a)(c) |
| 8 | 9, 22 | Columns custom; Balanced columns (MISSING); Cell margins and spacing (Cat 22) | PARTIAL | Newsletter columns do not balance; unequal widths cannot be set | (b)(d) |
| 9 | 22, 9 | Hyphenation (MISSING); Optional hyphen (PARTIAL); Nonbreaking hyphen and space (PARTIAL) | MISSING | Justified text with long words breaks differently | (b)(c) |
| 10 | 9 | Page Color; Gutter and gutter position | PARTIAL | Solid colours from 5 swatches; no mirrored binding | (d) |
| 11 | 11, 22 | Endnotes; Note numbering formats; Next Footnote, Show Notes | PARTIAL | Notes have fixed formats and cannot be stepped through | (b)(d) |
| 12 | 11 | Custom Table of Contents dialog (PARTIAL); Remove Table of Contents (MISSING); Add Text (MISSING); Table of Figures (MISSING) | PARTIAL | TOC levels and leaders cannot be set; a TOC cannot be removed | (c)(d) |
| 13 | 11 | Insert Caption (PARTIAL); Insert Citation (MISSING); Bibliography styles (MISSING) | PARTIAL | A paper cannot add a citation or choose a bibliography style | (c) |
| 14 | 10 | Different Odd & Even Pages; Go to Header, Go to Footer, Previous, Next (MISSING); Header from Top, Footer from Bottom; Format Page Numbers; Remove Page Numbers (MISSING); Document Info (MISSING); Quick Parts in header (MISSING); Date & Time; Tables and text boxes in headers | PARTIAL | Page number style, odd and even headers and document info in a header cannot be set from the window | (c)(d)(e) |
| 15 | 12 | Switches `\* Arabic`, `Roman`, `roman`, `ALPHABETIC`, `alphabetic`, `Ordinal`, `Cardtext`, `Ordtext`, `Hex`, `Dollartext`; Unlink (MISSING); Field shading (MISSING); Quick Parts: Field | PARTIAL | Roman and alphabetic numbering works on PAGE fields only; a field cannot be frozen or shaded | (c)(d) |
| 16 | 16 | Cover Page; Blank Page; Quick Parts: AutoText; Quick Parts: Document Property | MISSING | Cover pages and saved snippets from Word's Insert tab are not available | (c) |
| 17 | 14 | Show Comments; Comments pane; Show Markup (MISSING); Balloons (MISSING); Reviewing Pane (MISSING); Track formatting | PARTIAL | A reviewer cannot filter markup; formatting changes under tracking are not recorded | (c)(d) |
| 18 | 14 | Thesaurus; Check Accessibility; Language: Language Preferences; Document Inspector | MISSING | No synonym lookup, accessibility check, language switch or inspection before sharing | (c) |
| 19 | 2 | Find options; Wildcards (MISSING); Find Format (MISSING); Find Special (MISSING); Replace with formatting (MISSING); Search direction; Go To (MISSING) | PARTIAL | Cleaning up a document by pattern or by style is impossible | (c)(d) |
| 20 | 2 | Paste: Use Destination Styles; Paste: Picture, Link and Keep Source, Link and Merge; Paste Special dialog; Smart cut and paste; Use Ctrl+Click to follow hyperlink; Overtype mode; Column (block) selection; Multiple non contiguous ranges; Select Text with Similar Formatting; Status bar (PARTIAL) | MISSING | Pasting a picture, selecting a column of text and following a link are everyday editing moves | (c)(d) |
| 21 | 8 | Screenshot, Screen Clipping; Corrections; Color; Compress Pictures; Change Picture, Reset Picture; Picture Styles; Picture Border; Format Picture pane; Shape Format tab; Chart Design and Format | MISSING | Brightness, borders, styles and compression of a picture or chart cannot be changed | (c)(d) |
| 22 | 3 | Increase and decrease 1 pt; Character spacing: Scale; Character spacing: Position; Kerning for fonts; Character shading; Clear formatting shortcuts; Special Characters tab | PARTIAL | Honoured when a Word file sets them; the window offers no control | (d) |
| 23 | 4 | Outline level; Don't add space between paragraphs of the same style; Tab alignment; Tab leaders; Clear and Clear All tabs; Borders and Shading: Borders; Borders and Shading: Shading; Borders button gallery; Sort (MISSING) | PARTIAL | Honoured when a Word file sets them; the window offers no control | (d)(c) |
| 24 | 5 | Update to match selection; Delete style, Revert to default; Modify Style Format menu; Style Sets (Design tab); Set As Default; Normal style reset (PARTIAL); Theme Colors (PARTIAL); Theme Fonts (PARTIAL) | MISSING | A style cannot be redefined from a selection, deleted or reset, and themes cannot be customised | (c)(d) |
| 25 | 6 | Define New Bullet: Symbol; Define New Number Format; Define New Multilevel List; Link Level to Style (PARTIAL); Set Numbering Value (PARTIAL); Adjust List Indents; List Number and List Bullet styles | MISSING | A custom bullet, number format or outline numbering cannot be defined | (c)(d) |
| 26 | 18 | AutoCorrect: Correct TWo INitial CApitals; AutoCorrect: Capitalize first letter of table cells; AutoCorrect: Capitalize names of days; AutoCorrect: Correct accidental usage of cAPS LOCK key; Automatically use suggestions from the spelling checker; AutoFormat As You Type: Ordinals (1st) with superscript; AutoFormat As You Type: Fractions (1/2) with fraction character; AutoFormat As You Type: Hyphens (--) with dash; AutoFormat As You Type: Bold and italic with real formatting; AutoFormat As You Type: Border lines (PARTIAL, only `---`); AutoText | MISSING | Typos, double capitals, ordinals, fractions and double hyphens are not corrected as typed | (c) |
| 27 | 18 | Custom dictionaries; Grammar settings (MISSING); Proofing options; Recheck Document (MISSING) | PARTIAL | Words can be added but not managed, and the ignore rules cannot be changed | (c)(d) |
| 28 | 1 | Tagged PDF; Change File Type; Print only odd or even pages; Scale to paper size and pages per sheet | MISSING | A tagged PDF and a two sided print run need these | (a)(c) |
| 29 | 1 | AutoSave; Info panel; Properties: Summary; Properties: Statistics and Contents; Inspect Document; Check Accessibility; Encrypt with Password; Password to modify; Customize ribbon and Quick Access Toolbar; Default file format and location; Save as Plain Text options | MISSING | Document properties, a password and inspection before sharing are not available | (c)(d) |
| 30 | 15 | Read Mode; Outline view; Gridlines; New Window; Dark mode | MISSING | Read Mode and Outline view are how long documents are reviewed and reorganised | (d) |
| 31 | 17 | Content control properties | MISSING | Forms from Word cannot be inspected | (c)(d) |
| 32 | 13 | Envelopes; Labels; Start Mail Merge types; Step by Step Mail Merge Wizard; Select Recipients; Edit Recipient List; Filter and Sort recipients; Highlight Merge Fields; Address Block; Greeting Line; Update Labels; Merge to new document by section | MISSING | Form letters and labels cannot be made (PRD 244-248 puts mail merge out of scope) | (c) |
| 33 | 20 | Accessibility Checker; Color contrast; Access keys; Tagged PDF export | MISSING | Schools and public bodies require checks for alt text, contrast and reading order | (c)(d) |

---

# Part 5. Disagreements and corrections

Source files are under `C:/jason/dev/unluminous/crates/` (paths as in the key). "Resolved" means the code was read and the answer is stated; nothing was executed.

## 5.1 A and B disagree, or A and the code disagree

| # | Topic | A or B says | The code says | Evidence | Outcome |
|---|---|---|---|---|---|
| 1 | Space between paragraphs | B§22: the larger of after and before only under the HTML auto spacing compatibility option, otherwise the sum | Always the larger of the two, with a comment "measured against Word 16, a 6 line paragraph and the heading under it are 8 points apart" | W/layout/mod.rs:677-682 | Unresolved. The source records a measurement and B's tables were written from documentation and knowledge, not tested against Word (B "Method"). The oracle does not compare space between paragraphs on a page; a test with two 8 point spacings and a third unequal pair would settle it. |
| 2 | Encrypted package | B§23.3 treats it as PRESERVED | Refused at open with a message | W/zip.rs:84-91 | The plugin does not edit protected files; this is a decision, listed in Part 2 as DROPPED (open refused). |
| 3 | Constructs B marks "model" | B§23: `w:ptab`, `w:ruby`, `w:txbxContent`, `w:framePr`, `m:oMath`, fontTable, bibliography part, `docProps/custom.xml` | PRESERVED only (kept as XML, not interpreted) | W/read.rs:339; W/layout/paragraph.rs:516-527 | Listed row by row in Part 2. |
| 4 | Unknown paragraph level elements | A§3b and the comment at W/read.rs:272 say `m:oMath`, `w:object` and "anything newer" are "written back whole" | Written back inside a new `<w:r>`. `w:object` is a child of `w:r` and is unaffected; math and `w:subDoc` are paragraph level and are not | W/read.rs:269-278; W/write.rs:324-348 | A's statement is wrong for math and sub documents. This is the largest finding of the comparison (Part 2, D1). |
| 5 | Heading chords | A§6d: Word's Ctrl+Alt+1 to 3 are bound as Ctrl+Shift+Alt+1 to 3 | In addition, Ctrl+Alt+1 and Ctrl+Alt+2 run single and double line spacing, because `(Key::Num1, false)` and `(Key::Num2, false)` match first and ignore Alt; Ctrl+Alt+M runs indent and Ctrl+Alt+F runs find for the same reason | C/keys.rs:162-169,173 | Correction to A§6d. |
| 6 | Highlight colours | A§1b: "12 of 16"; B§3: 15 colours and No Color | The panel shows the first 12 of the 16 names, which leaves out dark gray, light gray, black and white. Word offers 15 colours, so three that Word offers are missing | W/props.rs:109-126; C/panel.rs:224-225 | Both agree once counted; Part 1 says "12 of the 15". |
| 7 | Ctrl+] and Ctrl+[ | B§21: change the size by 1 pt | Run the same step through the size list as Ctrl+Shift+> | C/keys.rs:177-178; S/actions.rs:91-106 | Differently bound; listed in section 21. |
| 8 | Ctrl+Shift+V | B§21: paste text only (its own note says it depends on the paste option) | Paste formatting | C/keys.rs:183 | Differently bound; listed in section 21. |
| 9 | Brief versus A§9c on fixtures | The brief lists no fixtures for "tables with vMerge/gridSpan/repeat header", "modern comments replies/resolved" and "floating anchors with wrap" | `tables` has merged cells and a repeating header; `comments` has a reply and a resolved comment; `pictures` has a floating PNG with square wrapping | A§9c lines for `tables`, `comments`, `pictures` | The fixtures exist. What is missing is any assertion on how they lay out: `pictures` and `comments` are not in the pagination test's list, and `tables` is checked only for the page of each top level paragraph (Part 6). |

## 5.2 A's "not determined" items resolved by reading code

| # | A said | Found | Evidence |
|---|---|---|---|
| 1 | Whether the Markdown habit test checks that one Ctrl+Z restores the characters (A§1b) | It does not. The test types `# Title` and checks the style and the text. The habit selects the marker, deletes it and then runs the heading action, which are separate steps in the history | S/tests.rs:202-211; C/keys.rs:139-143 |
| 2 | Linked styles (`w:link`) (A§1b, A§11#36) | Not followed. `w:link` is read nowhere except to copy it when a built in style is added | W/builtin.rs:48; W/styles.rs |
| 3 | `numStyleLink` and `styleLink` (A§3b) | `numStyleLink` is resolved when numbering is built. `styleLink` has no handler | W/numbering.rs:16,69 |
| 4 | Whether `w:trackRevisions` is written on save (A§3b) | It is written when tracking is switched on or off, into `settings.xml` | W/session.rs:564-567 |
| 5 | SmartArt (A§3b: "not determined for real SmartArt") | Always a grey box: a SmartArt frame is a `w:drawing` with no `a:blip`, and its cached drawing part is never read | W/model.rs:294-303; W/layout/paragraph.rs:588-653 |
| 6 | Comment on text in a header or note (A§3b) | Nothing in `add_comment` restricts the story; it inserts range marks and a reference in whichever story holds the selection. Word does not allow comments in headers and footers, so this can write a file Word complains about. Whether the UI prevents it was not read | W/ops.rs:806-848 |
| 7 | `w:compat` and its effect on old files (A§3b) | Not read anywhere in the Word crate, so one set of layout rules applies to every file regardless of `compatibilityMode` | W/layout/mod.rs:371-379 (the only settings read) |
| 8 | Word's reaction to a `.dotx` or `.docm` saved as `.docx` (A§2, A§11#31) | Still not determined. Code reading adds that `Package::set_override` is called only for parts Unluminous creates, never for the main part | W/package.rs:208-215; W/ops.rs:1438,1478,1543,1645; W/builtin.rs:82 |
| 9 | Whether a Word made TOC is updated (A§3b: "derived, not executed") | Confirmed from code: `find_toc` and `toc_entries` both skip a paragraph whose path is longer than 1, and a TOC Word wrote is inside a block content control, so its paragraphs have path length 2. Execution is untested | W/ops.rs:1189-1192,1253-1257 |
| 10 | Footnote text entry (A§4b) | Confirmed: the `footnote` action calls `insert_note(false, "")` and a press cannot land in a note story, so the window cannot be used to write footnote text; the verb takes the text as an argument | S/actions.rs:779-786; C/page_view.rs:58-77; S/commands.rs:515-516 |
| 11 | Keep with next ignored (A§5: "derived from the code") | Confirmed by search: `keep_next` appears at exactly two lines in the layout code, a declaration and an assignment | W/layout/mod.rs:475; W/layout/paragraph.rs:199 |
| 12 | Float wrap never read (A§3b) | Confirmed by search: no `.wrap` field access anywhere in the layout code | W/layout/ (no match) |

## 5.3 New facts not in A

- `w:ptab` and `w:ruby` have no layout branch. The default branch for a run object that is not a picture is a zero width glyph (W/layout/paragraph.rs:516-527). Word's three column header and footer designs use `w:ptab`, and ruby base text is the visible text of a phonetic guide.
- A break opportunity exists after a space, U+3000, a zero width space, an ideograph, and after `-`, en dash, em dash and `/` when the next character is alphanumeric. U+00A0 is not a space for this purpose, so it does not break (W/layout/paragraph.rs:389-396).
- Tracked paragraph marks under Enter use the id `0` every time (W/edit.rs:255-263).
- `commentsIds.xml`, `commentsExtensible.xml` and `people.xml` are never touched by comment commands; only `commentsExtended.xml` is (W/ops.rs:1561-1660).
- Deleting a range inside one paragraph collapses marks inside the range to its start; deleting across paragraphs in one container drains the middle paragraphs with their marks (W/edit.rs:318-326,392-405).

---

# Part 6. What the current tests would not catch

## 6.1 The instruments, as they stand

| Instrument | What it checks | What it cannot see | Evidence |
|---|---|---|---|
| Oracle comparison, `every_paragraphs_text_and_style_are_what_word_reports` and `every_paragraphs_indents_spacing_and_alignment_are_what_word_reports` | Per paragraph: count (table row end marks included), case folded text, style name, alignment, left, right and first line indent, space before and after (skipped when automatic), `keepNext`, `pageBreakBefore` | Run formatting, list labels, tables, sections, headers, footers, notes, comments and revisions. They are in the `.word.json` reports and no test reads them | WT/oracle.rs:130-246; A§9e |
| Pagination, `page_counts_and_breaks_are_close_to_words` | Equal page count and at least 80 percent of top level paragraphs starting on Word's page, on 11 fixtures | A drift of one paragraph in five passes. Paragraphs inside tables and cells, line positions, footnote placement and anything on a page other than its first paragraph are not compared. It returns without checking when Aptos is not installed. `blank`, `builtin-styles`, `comments`, `pictures`, `revisions` are not in the list | WT/pagination.rs:85-116; A§9e |
| Line breaking, `lines_break_where_word_breaks_them_in_installed_fonts` | Word's line breaks for Calibri, Times New Roman, Arial | Aptos, which is skipped and is the font of every new document; every other font; justified text, tabs, kerning, small caps, mixed sizes | WT/pagination.rs:126-160 |
| Round trip, `every_fixture_saved_unchanged_keeps_every_part_and_element` | Each of the 17 fixtures saved unchanged keeps every part and every element | Anything Word 16 did not write; a document after an edit | WT/roundtrip.rs:94-118 |
| `word_live` (two tests) | The first builds a document with the Session API and has Word open it and report text, styles, list strings, 1 table, 1 footnote, 2 comments, a link, a bookmark, a picture, a TOC, 2 revisions, 2 sections, header and footer text. The second types one phrase into each fixture, saves, and compares paragraph, table, footnote, comment, picture and section counts | Both pass without checking anything when Word is not installed, unless `UNLUMINOUS_REQUIRE_WORD=1`. Neither compares layout, formatting or any text beyond the typed phrase after the edit | WT/word_live.rs:15-30,119-311 |
| In file tests of the layout code | None in `W/layout/paragraph.rs`, `table.rs`, `mod.rs`; none in `W/styles.rs`, `W/read.rs`, `W/write.rs` | Tabs, justification, spacing, tables, notes, fields, style toggles at unit level | A§9a, A§1c #11 |
| Window tests | 9 in `tests/word.rs` and 15 in `S/tests.rs` | No test drives a table, a footnote, a comment card, a hyperlink, a paste from outside, a drag selection or a picture | A§9b |

## 6.2 Gaps in Parts 2 and 4 that no test or fixture exercises

"Fixture" means one of the 17 in `WT/fixtures/` (A§9c). All 17 were written by Word 16 through COM.

| Behaviour | Where in this document | Nearest existing coverage | Why it is not caught | What would catch it |
|---|---|---|---|---|
| Equation saved and reopened (D1) | Part 2, Part 4 D1 | None | No fixture has `m:oMath`; the round trip test only sees what is in the 17 | A Word written document with inline and display equations, saved unchanged, saved after an edit, opened by Word through `word_live` |
| Header text set on a header that holds a table or picture (D2) | Part 4 D2 | `a_footer_with_page_numbers_is_made_and_numbers_each_page` | It starts from a document with no footer | A header fixture with a table and a logo, set text, check both survive |
| `.dotx` and `.docm` open, edit and Save As `.docx` (D3) | Part 4 D3 | `an_old_word_file_is_named_rather_than_called_damaged` (OLE signature only) | No `.dotx` or `.docm` fixture | Both fixtures through open, save and `word_live` |
| Delete across paragraphs holding bookmarks and comments (D4) | Part 4 D4 | `deleting_across_paragraphs_joins_them` | That test has no marks | A fixture with a bookmark and a comment spanning three paragraphs, delete the range, have Word open the result |
| Several Enter presses under Track Changes (D5) | Part 4 D5 | `a_tracked_insertion_is_wrapped_and_deleting_it_removes_it` | Only typing and deleting | Press Enter five times with tracking on, check ids and have Word open it |
| Strict OOXML and files from other producers (D6) | Part 4 D6 | `a_foreign_prefix_for_the_main_namespace_is_read_as_w` (unit) | No Strict or foreign file | Strict, LibreOffice, Google Docs and python-docx outputs in the round trip test |
| `w:ptab`, `w:ruby` | Part 4 D7 | None | No fixture; the oracle does not compare footer layout | A three column footer and a ruby paragraph, compared with Word's line text |
| Table rows splitting across pages; `cantSplit` | Part 4 core 1 | `tables`, `long` (a 5 row table) | Pagination compares top level paragraphs only, and 80 percent passes; no row is taller than a page | A table with rows of mixed height that Word splits, with Word's page for the first line of each row |
| Floating picture wrap | Part 4 core 2 | `pictures` has the floating PNG | `pictures` is not in the pagination list and nothing reads line positions | Add `pictures` to the list and compare each line's start offset against a Word line report |
| Keep with next chains | Part 4 core 3 | `paragraphs` has keep with next | The property is compared as a value (`keepNext`), not its effect; a miss moves a few paragraphs, inside the 80 percent allowance | A fixture where a heading and a paragraph would split, exact page per paragraph |
| Header taller than the margin | Part 4 core 4 | `headers` | Its headers are one line | A fixture with a tall header and the first body line's position |
| Aptos line breaking | Part 4 core 5 | `line-breaks` leaves the Aptos paragraph out | Known to differ by about 3 percent | Put the Aptos paragraph back with the corrected metric |
| Table widths with `tcW`, autofit, `dxa` `tblW` | Part 4 core 6 | `tables` (fixed widths) | No such fixture | Tables saved by Word with each width type, compared column by column |
| Footnote that spans a page | Part 4 core 8 | `notes` (4 footnotes, 2 endnotes) | Nothing says any note spans a page; `W/layout/mod.rs` has no test | A long note near a page foot |
| Footnote text typed in the window | Part 4 core 7 | None | No window test enters a note | A window test that inserts a footnote, types into it, saves, and reads it back |
| Update fields on a Word made TOC | Part 4 core 9 | `fields` has a Word made TOC | No test runs `update fields` on it (A§3b) | Run F9 on `fields`, compare the TOC with Word's |
| PAGE and NUMPAGES in the body | Part 4 core 10 | `page_fields_follow_their_format_switch` (headers) | Body is not evaluated | A body page field fixture |
| Paste of formatted content from another application | Part 4 core 11 | `a_formatted_copy_pastes_with_its_formatting` | In process only | Paste HTML and RTF from the clipboard |
| Hyperlink follow; link edit and remove | Part 4 core 13 | `a_hyperlink_wraps_the_selection_and_keeps_its_address` | Insertion only | Window test for click behaviour |
| Mouse selection modes and drag and drop | Part 4 core 14 | `the_arrows_and_shift_select_and_backspace_deletes_the_selection` | Keyboard only | Window test with pointer events |
| AutoCorrect and AutoFormat | Part 4 core 15, 16 | `the_markdown_habits_make_headings_and_lists` | One heading case, no undo check | Tests for each rule once the rule exists |
| Number formats beyond decimal, roman, letters, ordinal and text; picture bullets | Part 4 core 17 | `every_format_word_offers`; `lists` | Fixture has no other format | A list fixture using Hebrew, Chinese counting and picture bullets |
| Picture resize, crop, EMF, WMF, SVG | Part 4 core 18 | `a_picture_becomes_a_part_a_relationship_and_an_inline_drawing`; `pictures` | Insert only; no `srcRect`, no metafile | Fixture with a cropped picture and an EMF |
| Text boxes, shapes, charts, SmartArt, watermark | Part 4 core 44, Part 2 | None | No fixture | One document with each, compared for placeholder boxes and for round trip |
| Content controls other than Word's TOC | Part 2, Part 4 common 31 | `fields` (TOC `w:sdt` only) | No other control | Fixture with each control type, edit text inside, round trip |
| Table style conditional formatting and `basedOn` | Part 4 core 25, common 2 | `tables`, `builtin-styles` (Table Grid) | No banded or first column style | A table with a Word table style, compared for cell fills and bold |
| Style toggles and the resolution order | Part 4 core 25 | `styles` fixture (samples) | No test of toggles (A§1c) | Unit tests of the XOR rule with a table style in the chain |
| Tracked changes with moves, row and cell revisions, `sectPrChange`, `tblPrChange` | Part 2 | `revisions` (insert, delete, one `rPrChange`) | No fixture with the others | Fixture with each revision kind, accept all, reject all |
| New formatting under Track Changes | Part 4 common 17 | `revisions` | Edits are not made while tracking | Format under tracking, have Word list the revisions |
| Modern comments: delete all, in headers, ids files | Part 4 core 31, D8 | `comments` (reply, resolved), `a_comment_is_added_replied_to_resolved_and_deleted` | `commentsIds.xml` not compared; no comment in a header | Round trip after delete with the extra id parts present |
| Fields other than TOC, DATE, SEQ, links: REF, PAGEREF, STYLEREF, IF, `=`, MERGEFIELD, DOCPROPERTY | Part 4 core 35 | `fields` | Stored results are preserved but nothing changes them | Fixture with each, F9 compared with Word's update |
| Hyphenation, optional and nonbreaking characters, balanced columns | Part 4 common 9, 8 | None | Nothing in the fixtures | Fixtures with each, compared against Word's lines |
| Embedded fonts | Part 2 | None | No `.odttf` fixture | A document with an embedded font, round trip and layout with the font absent |
| Password protected and encrypted files | Part 2 | `an_old_word_file_is_named_rather_than_called_damaged` checks the OLE signature | An Office 2007 agile encrypted file is also OLE; no such sample | A real encrypted `.docx`, check the message |
| Settings that change layout: `w:compat`, `compatibilityMode` | Part 5.2 #7 | None | Never read | A mode 11 and a mode 14 document with different table cell margin handling |
| Right to left text | Part 1 section 22 | None | `bidi` parsed and unused | An Arabic or Hebrew paragraph fixture |

## 6.3 What the follow up should add first

1. Compare, per paragraph, the Word `.word.json` fields that are already collected and unused: run formatting, list strings, table cells, headers, footers, notes, comments, revisions. The data is on disk; only the assertions are missing.
2. Replace the 80 percent allowance with an exact page match per paragraph on the fixtures where Word and Unluminous currently agree, and keep a separate looser test for the ones that do not, so a regression in a matching fixture fails.
3. Make `word_live` fail instead of pass when Word is absent in the release gate (`UNLUMINOUS_REQUIRE_WORD=1`).
4. Add fixtures for the DROPPED and NOT-DETERMINED rows of Part 2 first: an equation, a header with a table, `.dotx`, `.docm`, a document with bookmarks and comments across paragraphs, and a Strict file. Each takes one fixture and one `word_live` assertion.
5. Add layout fixtures for the first six core items of Part 4 before changing the layout code, so each change is measured.
