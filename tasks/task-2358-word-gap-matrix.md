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
| New blank document | core | DONE | S/mod.rs:1069-1100; S/files.rs:93-130; MANIFEST:30 | The Word menu has New Word Document and the verb `new` makes one. It starts from the shipped blank with the locale's page size, or from Word's Normal.dotm when this machine has one. |
| New from template | core | PARTIAL | S/mod.rs:1075-1100; S/commands.rs:264 | A .dotx or .dotm named with `new --template` starts a new untitled document that keeps no macros and none of the template's title or keywords. The window has no template gallery. |
| Normal.dotm | common | DONE | S/files.rs:93-107; S/mod.rs:1080-1082 | New documents start from `%APPDATA%\Microsoft\Templates\Normal.dotm` when it exists. Only the released binary reads it, so tests start from the shipped blank. |
| Templates .dotx | common | DONE | S/mod.rs:840-851,881-892; W/write.rs:68-102 | Opens as an untitled document. Save As .dotx, .dotm and .docm write the matching main part content type through `prepare_for`. |
| Open .docx | core | DONE | W/read.rs:14-56; S/mod.rs:1032-1070 | Opens through `_rels/.rels`. Every fixture round trips part by part. |
| Open .doc | common | PARTIAL | W/convert.rs:18-35; S/mod.rs:1038-1044; S/files.rs:52 | Word converts it to a .docx on a thread, so Word must be installed. Without Word the tab says so. Native readers are a non goal of the TDD. |
| Open .rtf | core | PARTIAL | W/convert.rs:18-35; S/mod.rs:1038-1044 | Converted by Word, so Word must be installed. A native reader is a non goal of the TDD. |
| Open .odt | common | PARTIAL | W/convert.rs:18-35; S/mod.rs:1038-1044 | Converted by Word, so Word must be installed. A native reader is a non goal of the TDD. |
| Open .txt | core | PARTIAL | S/mod.rs:1046-1052; W/import.rs:9-30; S/commands.rs:284-305 | Open as Document (Word menu, verb `open --as-document`) makes a document from the text, one paragraph a line. A plain open still goes to the text editor. |
| Open .htm, .html, .mht | common | PARTIAL | S/mod.rs:1046-1052; W/import.rs:29-110 | Open as Document reads .html and .htm through a small HTML reader. .mht is not read. |
| Open PDF (PDF Reflow) | common | PARTIAL | W/convert.rs:18-35; S/mod.rs:1038-1044 | Word reflows the PDF into a .docx, so Word must be installed. The verb `open` takes the file. The extension is not in the plugin's claimed list. |
| Protected View | common | MISSING | S/mod.rs:1032-1070 | Files from the internet open editable. |
| Recent documents and pinned files | core | MISSING | S/mod.rs:1032-1070 | The window keeps a list of recent projects, not of Word documents. The Word plugin keeps none. |
| Save | core | DONE | S/mod.rs:808-856,1262 | Writes `lastModifiedBy`, `modified` and `creator` when empty. A password, if one was set, encrypts the written file. |
| Save As | core | DONE | S/mod.rs:862-892,1294; S/commands.rs:364 | The dialog offers 10 kinds (SAVE_KINDS). Verb `save <path>`. |
| Save as .docx, .docm, .dotx, .dotm, .doc, .rtf, .odt, .txt, .htm, .xml | core | PARTIAL | S/mod.rs:858-892; W/convert.rs:23-27 | Written: .docx, .dotx, .docm, .dotm, and .rtf and .odt through Word. .txt, .html, .md and .pdf are copies. .doc and .xml are not written. |
| Save as Plain Text options | common | PARTIAL | W/export.rs:13-30; S/mod.rs:867 | The text copy has no encoding or line ending choice. |
| Export to PDF | core | PARTIAL | W/pdf.rs:62-170,859; S/commands.rs:2131-2200 | Tagged, with the headings as an outline, document information and PDF/A-2b (`--pdfa`). Links are not clickable. A font from a font collection is replaced by Helvetica. Page ranges go through Print. |
| Tagged PDF | common | DONE | W/pdf.rs:50-135,859 | Headings, paragraphs, list items, cells and figures with alt text. `--no-tags` leaves them out. |
| Change File Type | common | MISSING | S/mod.rs:858-892 | Save As covers the formats; there is no Change File Type command. |
| Print | core | DONE | S/print.rs:161-201; C/panel.rs:1295-1366; C/keys.rs:99 | Writes the pages as a PDF and hands it to the system print dialog (ShellExecute print on Windows, the viewer elsewhere). Ctrl+P and the verb `print`. |
| Print ranges | core | DONE | S/print.rs:52-76; C/panel.rs:1296-1310 | Pages such as `1-3,5`, in the panel and the verb. |
| Print only odd or even pages | common | DONE | S/print.rs:76-104; C/panel.rs:1311-1317 | All, odd or even pages. |
| Scale to paper size and pages per sheet | common | PARTIAL | S/print.rs:105-138; C/panel.rs:1325-1336 | Two or four pages per sheet are imposed on the PDF. There is no scale to paper size. |
| Print preview | core | DONE | C/panel.rs:1337-1350 | The Preview button writes the same PDF and opens it in a tab. It shows the printed sheets, not an in window preview. |
| Mirror margins | common | DONE | W/page_setup.rs:193; W/layout/mod.rs:2113-2175; C/panel_setup.rs:62 | Mirrored margins and the gutter are laid out and set by a tick box in the Page setup level. |
| AutoSave | common | DONE | S/files.rs:142-144,270; S/mod.rs:1346; S/commands.rs:388-406 | Off until switched on with the verb `autosave on`. When on, a document with a file is saved 5 seconds after its last change. |
| AutoRecover | common | DONE | S/files.rs:147-245; S/commands.rs:388-406; S/tests.rs:523 | On by default. A copy of a changed document is kept every 10 minutes and offered when the file is opened again. |
| Document Recovery pane | common | PARTIAL | C/banner.rs:20-98; S/files.rs:212-245; S/commands.rs:1074 | A banner offers the one recovered copy of the document being opened (restore or discard). There is no pane listing every unsaved file. |
| Info panel | common | PARTIAL | C/panel.rs:1901-1925; S/commands.rs:1715-1822 | The Document level edits 4 properties. The verbs `properties` and `protect` cover the rest; the window has no single Info page. |
| Properties: Summary | common | PARTIAL | C/panel.rs:1901-1925; S/commands.rs:1715-1780 | The panel edits title, subject, author and keywords. The verb also sets comments, category, status, manager and company. |
| Properties: Statistics and Contents | common | PARTIAL | W/session.rs:721; C/panel.rs:1877-1895; S/commands.rs:1780-1822 | Live counts are in the Review level. Created, modified, revision and editing time are answered by `properties statistics` and are not shown in the window. |
| Inspect Document | common | DONE | W/review.rs:536-620; C/panel.rs:1860-1875; S/commands.rs:961 | Counts comments, tracked changes, hidden text and personal names, and removes each kind. |
| Check Accessibility | common | DONE | W/review.rs:442-535; C/panel.rs:1849-1858; S/commands.rs:953 | Pictures without alt text, skipped heading levels, tables without a header row and low contrast text. |
| Encrypt with Password | common | PARTIAL | W/crypto.rs:123-298; S/mod.rs:836-840; S/commands.rs:377-387,1823 | Agile encryption opens and saves. The password is set with `protect password <p>`; the window only asks for a password when an encrypted file is opened. |
| Password to modify | common | PARTIAL | W/review.rs:262-314; C/banner.rs:60-98; S/commands.rs:1069 | Set by `protect modify <p>`. A file that has one opens read only with a banner that takes the password. The window cannot set one. |
| Restrict Editing | common | PARTIAL | W/review.rs:210-262; S/commands.rs:1823 | Enforced: read only, comments, tracked changes and forms, with a password to stop it. Set by `protect restrict`; the window has no control for it. |
| Share | common | OUT-OF-SCOPE | B§19 | Needs a cloud share service. |
| Close and Exit | core | DONE | S/mod.rs:1302; S/commands.rs:359 | The tab saves as it closes; verb `close`. |
| Options dialog | core | PARTIAL | C/settings_page.rs:9-77; C/panel.rs:1609-1618 | The Settings page has author, zoom, formatting marks and spelling; the Proofing level has the autocorrect rules. No Language, Save or Trust sections. |
| Customize ribbon and Quick Access Toolbar | common | MISSING | MANIFEST:20 | No ribbon, by the PRD's decision (a non goal of the TDD). Chords can be moved with the keymap. |
| Default file format and location | common | MISSING | C/settings_page.rs:9-77 | No such setting. |
| Templates .dotm; Open .docm (2 rows) | niche | PARTIAL | W/write.rs:16-39 | `.docm` opens and its VBA parts survive a save; macros never run. |
| Attach template; Properties: Custom; Remove personal information on save; Mark as Final; Add a Digital Signature; Restrict Access (IRM); Compatibility Mode; Embed fonts in file (8 rows) | niche | PRESERVED | W/write.rs:16-39 | Parts and settings round trip; none is acted on (embedded fonts are not de obfuscated or used for layout). |
| Open .dot; Open .xml; Open .wps; Open .wpd; Open and Repair; Export to XPS; Save as Strict Open XML Document; Print what; Print options; Booklet (Book fold); Always create backup copy; Manage Document; Convert to current mode; Check Compatibility; Trust Center; Update links on open (16 rows) | niche | MISSING | A§2 | No code. |
| Save as Web Page, Filtered | niche | PARTIAL | W/export.rs:114 | HTML export exists, body only, pictures inline as base64. |
| Version History | niche | OUT-OF-SCOPE | B§19 | Server side versions of cloud files. |

### 2. Edit and selection

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Cut, Copy, Paste | core | DONE | C/keys.rs:182-232; W/clipboard.rs:45-110,349-396,669-905; S/mod.rs:445-604 | Copy puts text, HTML and RTF on the clipboard. Paste reads HTML and RTF from Word, a browser or another program, and keeps the formatting. |
| Paste: Keep Source Formatting | core | DONE | W/clipboard.rs:20-30,930; C/overlays.rs:395-460 | The default paste. The Paste Options card under the paste offers it again. |
| Paste: Merge Formatting | core | DONE | W/clipboard.rs:20-30,930-945; C/overlays.rs:395-460; S/commands.rs:986 | Takes the destination paragraph's properties and keeps the run formatting. Verb `paste --as merge`. |
| Paste: Keep Text Only | core | DONE | W/clipboard.rs:20-30,945-955; C/keys.rs:193-209; C/overlays.rs:395-460 | Choose it on the Paste Options card, or paste with Shift held. Verb `paste --as text`. |
| Paste: Use Destination Styles | common | MISSING | W/clipboard.rs:419; C/overlays.rs:395-460 | No style mapping. Merge Formatting is the nearest choice. |
| Paste: Picture, Link and Keep Source, Link and Merge | common | PARTIAL | S/mod.rs:506-523; C/keys.rs:176-181 | A picture on the clipboard is pasted as a picture. Linked paste is not offered. |
| Paste Special dialog | common | PARTIAL | C/keys.rs:93,518-521; S/mod.rs:463-471 | Ctrl+Alt+V pastes the clipboard and shows the Paste Options card. There is no list of clipboard formats. |
| Drag and drop text | core | DONE | C/page_view.rs:845-888,945-1000 | A press inside the selection carries it. Dropping moves the text, or copies it with Ctrl held. |
| Smart cut and paste | common | MISSING | C/keys.rs:193-209 | No space adjustment. |
| Use Ctrl+Click to follow hyperlink | common | DONE | C/page_view.rs:817-841,969-990; W/references.rs:658 | Ctrl+click follows the link and the pointer shows its address and screen tip. |
| Overtype mode | common | MISSING | C/keys.rs:314-481 | The Insert key is not handled. |
| Undo, Redo, Repeat | core | DONE | W/session.rs:87,244-262; C/keys.rs:55,446-451,483-490 | 500 undo steps. Ctrl+Z, Ctrl+Y and F4 (redo, or repeat the last command). There is no list of steps to undo. |
| Selection by mouse | core | PARTIAL | C/page_view.rs:870-940,990-1019 | Double click a word, triple click a paragraph, Shift click to extend. F8 steps on to the sentence. No click in the left margin to select a line. |
| Selection by keyboard | core | DONE | C/keys.rs:314-481 | Shift and Ctrl+Shift with the arrows, Home, End, PageUp, PageDown, Ctrl+A and F8. |
| Column (block) selection | common | MISSING | C/page_view.rs:870-940 | The selection is one range. |
| Multiple non contiguous ranges | common | MISSING | W/position.rs:62-90 | The selection is one range. |
| Select Text with Similar Formatting | common | MISSING | C/context_menu.rs:18-136 | No command. |
| Find (Navigation pane) | core | DONE | C/overlays.rs:563-709; C/navigation.rs:183; W/session.rs:678 | Ctrl+F opens the find bar with a count and Next and Previous. The Results tab of the Navigation pane lists the matches. |
| Navigation pane tabs | core | DONE | C/navigation.rs:10-60,64-112,183 | Headings, Pages (thumbnails) and Results. |
| Advanced Find | core | PARTIAL | C/overlays.rs:563-640; W/search.rs:61-116; S/commands.rs:634-700 | The bar has match case, whole words, wildcards and all stories. Format and the `^` codes work by typing or by verb. No sounds like, all word forms, prefix or suffix. |
| Find options | common | PARTIAL | C/overlays.rs:563-640; W/find.rs:16-100 | Match case, whole words, wildcards and all stories; no sounds like, all word forms, prefix, suffix or ignore punctuation. |
| Wildcards | common | DONE | W/find.rs:99-360,463-647; C/overlays.rs:600 | `?`, `*`, `[a-z]`, `[!x]`, `{n,m}`, `@`, `<`, `>` and `(group)` with `\1` in the replacement. Always case sensitive, as Word's are. |
| Find Format | common | PARTIAL | W/search.rs:20-60,390-460; S/commands.rs:97-152 | `--format=bold,italic,color:RRGGBB,style:<id>` limits a find by verb. The find bar has no Format button. |
| Find Special | common | PARTIAL | W/find.rs:81-99,463-530 | `^p`, `^t`, `^#`, `^$`, `^?`, `^w`, `^&` and the rest work when typed. There is no Special drop down. |
| Replace and Replace All | core | DONE | W/session.rs:692; W/search.rs:116-184; W/session.rs:821 | Replace All is one undo step. |
| Replace with formatting | common | PARTIAL | W/search.rs:116-184; S/commands.rs:696-716 | `replace --with-format=` formats the replacement by verb. The bar has no Format button. |
| Search direction | common | PARTIAL | C/overlays.rs:595-625 | Next and Previous; there is no Up, Down, All choice. |
| Go To | common | DONE | W/search.rs:184-296; C/overlays.rs:123; C/keys.rs:337 | Ctrl+G opens a box that takes a page, section, line, bookmark, footnote, endnote, comment, heading, table or picture. Verb `goto`. |
| Copy and paste formatting | common | DONE | C/keys.rs:69-70; S/actions.rs:339,361 | Ctrl+Shift+C and Ctrl+Shift+V. |
| Format Painter | core | PARTIAL | S/actions.rs:339,361; C/panel.rs:365-375 | Panel buttons and chords; no brush button with single and double click modes. |
| Status bar | common | DONE | C/views.rs:303-380 | Page, words (selected of total), language, Track Changes, the three views and a zoom slider with a percentage. |
| Mini toolbar | common | DONE | C/overlays.rs:462-562 | Bold, italic, underline, highlight (yellow, or off), link and comment over a selection. |
| Set Default Paste; Office Clipboard; Smart paragraph selection; Click and Type; Extend mode F8; Browse Object; Spike (7 rows) | niche | MISSING | A§4b | No code. |

### 3. Fonts and character formatting

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Font name | core | PARTIAL | C/panel.rs:153-175; C/format_bar.rs:103; S/commands.rs:1404-1495 | A combo of installed families, also set by `format font`. No preview, no recent list and no theme fonts (+Headings, +Body). |
| Font size | core | DONE | C/panel.rs:176-195; S/commands.rs:1404-1495 | Drag from 1 to 1638 pt, and the verb `format size`. |
| Grow Font, Shrink Font | core | DONE | S/actions.rs:265,272; C/keys.rs:62-63 | Steps through Word's size list. Ctrl+Shift+. and Ctrl+Shift+,. |
| Increase and decrease 1 pt | common | DONE | S/actions.rs:1736,1743; C/keys.rs:64-65 | Ctrl+] and Ctrl+[ change the size by one point. |
| Font style | core | DONE | C/format_bar.rs:175-192; C/panel.rs:190-197 | Bold and italic: buttons, Ctrl+B, Ctrl+I, the verb. |
| Font color | core | DONE | C/panel.rs:219-272; S/commands.rs:1404-1495 | 10 swatches, a colour picker, Automatic, and 8 theme colours that keep their `themeColor` name. No shade grid and no gradient. |
| Underline style | core | PARTIAL | W/props.rs:134-178; C/panel.rs:198-211; S/actions.rs:1693,1700 | The panel offers 8 styles. The heavy and long dash variants are folded into thick and dash when read. |
| Underline color | common | PRESERVED | W/props.rs:134-178 | The colour attribute of `w:u` stays in the run properties; it is not drawn. |
| Strikethrough | core | DONE | S/actions.rs:216; C/panel.rs:190-197 | Panel, action, verb. |
| Double strikethrough | common | DONE | S/actions.rs:223; C/panel.rs:190-197 | Action and verb. |
| Superscript | core | DONE | S/actions.rs:230; C/keys.rs:67 | Ctrl+Shift+=. |
| Subscript | core | DONE | S/actions.rs:237; C/keys.rs:66 | Ctrl+=. |
| Small caps | common | DONE | S/actions.rs:244; W/layout/paragraph.rs:439-617; C/keys.rs:35 | Drawn at 0.8 of the size. Ctrl+Shift+K. |
| All caps | common | DONE | S/actions.rs:251; C/keys.rs:36 | Action, verb and Ctrl+Shift+A. |
| Hidden | common | DONE | S/actions.rs:258; C/page_view.rs:631-648; C/keys.rs:37 | Shown while formatting marks are on. Ctrl+Shift+H. |
| Character spacing: Scale | common | DONE | C/panel.rs:304-314; W/props.rs:557-565 | A drag control in percent. |
| Character spacing: Spacing | common | DONE | C/panel.rs:291-303 | Drag control in points. |
| Character spacing: Position | common | DONE | C/panel.rs:315-332; W/props.rs:557-565 | The Raise control, in points. |
| Kerning for fonts | common | DONE | C/panel.rs:333-337; W/kern.rs:30-138 | A tick box. Pair kerning from the font's GPOS table is applied when `w:kern` asks. |
| Text Highlight Color | core | DONE | C/panel.rs:274-290; W/props.rs:109-127 | All 16 colours Word has, and None. |
| Character shading | common | PARTIAL | W/props.rs:506-523; W/layout/paragraph.rs:1522-1566 | Drawn; no control. |
| Character border | common | PRESERVED | W/layout/paragraph.rs:1566-1713 | `w:bdr` is kept and not drawn. |
| Change Case | core | DONE | W/ops.rs:1400-1425; C/panel.rs:365-371; S/actions.rs:289-329,1717 | Five cases in the panel, and Shift+F3 steps through them. |
| Clear All Formatting | core | DONE | W/ops.rs:1426-1440; S/actions.rs:279 | Ctrl+Space and the panel. |
| Clear formatting shortcuts | common | DONE | C/keys.rs:52,59-60; W/shortcuts.rs:89 | Ctrl+Space clears character formatting, Ctrl+Q resets the paragraph, Ctrl+Shift+N applies Normal. |
| Font dialog (Ctrl+D) | core | PARTIAL | C/keys.rs:71-73; S/actions.rs:1456 | Ctrl+D, Ctrl+Shift+F and Ctrl+Shift+P open the Text level of the panel. There is no separate dialog and no Set As Default. |
| Language per run | common | DONE | W/review.rs:377-440; C/panel.rs:1837-1848; S/commands.rs:948 | The Review level and the verb `language` set `w:lang` on the selection. Spelling skips a run marked as another language, and checks English only. |
| East Asian font | common | PRESERVED | C/panel.rs:164-172 | `eastAsia` is set to the chosen font name and kept; it is not drawn separately. |
| Complex script font | common | PRESERVED | C/panel.rs:164-172 | `w:cs` is set to the chosen font name and kept; it is not applied. |
| Insert Symbol dialog | core | DONE | C/panel_setup.rs:386-483 | Blocks of Unicode, the recently used characters and the special characters. |
| Special Characters tab | common | DONE | C/panel_setup.rs:416-440; S/actions.rs:1775-1885 | Dashes, ellipsis, copyright, registered, trademark, section and paragraph signs, nonbreaking space and hyphen, optional hyphen. |
| Insert Emoji | common | NOT-DETERMINED | C/keys.rs:184-185 | Text from the operating system's emoji panel arrives as a text event and is typed. Whether a colour glyph draws was not tested. |
| Formatting marks | core | PARTIAL | C/page_view.rs:631-648; W/layout/paragraph.rs:526; C/keys.rs:68 | Paragraph marks and hidden text; no tab arrows, space dots, optional hyphens or object anchors. |
| Emboss, Engrave; Outline; Shadow; Equalize character height; OpenType Ligatures, Number spacing, Number forms, Stylistic sets, Contextual Alternates; Text Effects; Text Fill and Text Outline; Emphasis mark; Phonetic Guide; Enclose Characters; Combine Characters (15 rows) | niche | PRESERVED | A§3b | Run properties are kept in the tree and not drawn; `w:ruby` text is not shown at all (see Part 2). |
| Font substitution; Unicode entry (Alt+X); Alt codes; Assign shortcut to symbol; Text boundaries; Reveal Formatting (6 rows) | niche | MISSING | A§6d | No code. |
| Cloud fonts | niche | DONE | S/fonts.rs; S/tests.rs `office_fonts_are_found_once_they_are_added` | Office cloud font folders are added when a document opens. |

### 4. Paragraph

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Alignment: Left, Centered, Right, Justified | core | DONE | C/format_bar.rs:216-229; S/actions.rs:373-394; C/keys.rs:40-43 | Buttons, Ctrl+E, L, R, J. |
| Outline level | common | DONE | C/panel.rs:481-502; W/props.rs:343-496; C/outline.rs:78-210 | A combo in the Paragraph level sets it, and the Outline view and the outline verb promote and demote. |
| Indentation: Left, Right | core | DONE | C/panel.rs:402-415; C/views.rs:78-300; W/ruler.rs:29 | Drag controls, the ruler and Ctrl+M. |
| Special indent: First line, Hanging | core | DONE | C/panel.rs:416-425; W/shortcuts.rs:64 | Negative first line hangs. Ctrl+T makes a hanging indent. |
| Spacing Before and After | core | DONE | C/panel.rs:426-438; S/actions.rs:443-464; S/commands.rs:1495-1612 | Points, and add or remove space. Auto spacing is honoured in layout and has no control. |
| Don't add space between paragraphs of the same style | common | DONE | C/panel.rs:470-480; W/layout/tests/spacing.rs:43-52 | A tick box; layout drops the space between two paragraphs of one style. |
| Line spacing: Single, 1.5 lines, Double | core | DONE | C/keys.rs:44-46; S/actions.rs:415-436 | Ctrl+1, 5, 2, and the actions. |
| Line spacing: At least | common | DONE | C/panel.rs:439-469 | Rule combo. |
| Line spacing: Exactly | common | DONE | C/panel.rs:439-469 | Rule combo. |
| Line spacing: Multiple | core | DONE | C/panel.rs:439-469 | Any multiple. |
| Widow/Orphan control | core | DONE | C/panel.rs:503-518; W/layout/mod.rs:1737-1830 | Two lines at the foot and the head of a page; a tick box. |
| Keep with next | core | DONE | W/layout/mod.rs:1737-1772; W/layout/tests/keep_next.rs; C/panel.rs:503-518 | The page flow moves a chain of kept paragraphs together with the paragraph after it. |
| Keep lines together | core | DONE | W/layout/mod.rs:1737-1830; C/panel.rs:503-518 | Moves the whole paragraph when it fits on a fresh column. |
| Page break before | core | DONE | W/layout/mod.rs:1716-1772; C/panel.rs:503-518 | A tick box in the Paragraph level; the paragraph starts a page. |
| Don't hyphenate | common | PARTIAL | W/props.rs:381,462; W/layout/tests/hyphenation.rs:84 | `suppressAutoHyphens` is honoured by hyphenation. The Paragraph level has no tick box for it. |
| Text direction: LTR and RTL paragraph | common | PRESERVED | W/props.rs:380,461 | `bidi` is parsed and never used by layout. Right to left is not drawn. |
| Tabs dialog: stop position | core | DONE | C/panel.rs:637-694; C/views.rs:78-300; W/ruler.rs:55-129 | Add a stop at a position with its alignment and leader, drag it on the ruler, clear them. The default tab stop is not editable. |
| Tab alignment | common | PARTIAL | W/props.rs:479; W/layout/paragraph.rs:994-1010 | Left, centre, right and decimal are laid out. A bar tab is stored and drawn as nothing. |
| Tab leaders | common | DONE | C/panel.rs:664-668; W/props.rs:458-483 | Dot, hyphen and underscore can be chosen and are drawn. |
| Clear and Clear All tabs | common | DONE | C/panel.rs:690-693; W/ruler.rs:70-101 | Clear all in the panel; drag one off the ruler to clear it. |
| Ruler tab selector | core | DONE | C/views.rs:44-110 | The selector at the ruler's left cycles left, centre, right, decimal and bar. |
| Borders and Shading: Borders | common | PARTIAL | C/panel.rs:519-612; W/layout/paragraph.rs:1717-1775 | Box, line below, no border, and a border on any side with a style, width and colour. Only single and double are drawn; the other styles draw as single. |
| Borders and Shading: Shading | common | DONE | C/panel.rs:613-626; W/props.rs:512 | A colour picker, a fixed shade and none. Pattern shading is not drawn. |
| Horizontal line | common | DONE | C/keys.rs:392-410; S/actions.rs:760 | Bottom border; also `---` then Enter. |
| Borders button gallery | common | PARTIAL | C/panel.rs:519-612 | Box, line below, none and the five sides; no inside or diagonal entries. |
| Merged borders | common | PARTIAL | W/layout/paragraph.rs:1717-1775 | Each paragraph draws its own box; two paragraphs with the same borders are not joined into one. |
| Drop cap | common | PARTIAL | W/layout/mod.rs:953-1038; W/styles.rs:223-245 | A `framePr` paragraph is laid out as a text frame with text going round it, which is how a drop cap is stored. None can be made. |
| Show/Hide paragraph marks | core | DONE | C/keys.rs:68; S/actions.rs:1128 | Ctrl+Shift+8. |
| Sort | common | DONE | W/shortcuts.rs:238-294; C/panel.rs:627-636; S/actions.rs:2169 | A to Z or Z to A over the selected paragraphs. |
| Bullets, Numbering, Multilevel List buttons | core | PARTIAL | C/format_bar.rs:200-214; C/panel.rs:812-832; W/lists.rs:134 | Bullet and numbered buttons and the Lists level. `list define` makes a multilevel list; the window has no multilevel gallery. |
| Increase and Decrease Indent buttons | core | DONE | S/actions.rs:401,408; C/keys.rs:48-49 | Actions, Ctrl+M and Ctrl+Shift+M. |
| Line and Paragraph Spacing menu | core | DONE | S/actions.rs:415-464 | Actions for 1.0, 1.15, 1.5, 2.0 and add or remove space before and after. |
| Paragraph mark formatting | common | PRESERVED | W/order.rs:240-257 | The paragraph mark's run properties are kept and sorted; no control. |
| Alignment: Distributed; Thai Distributed, Justify High, Medium, Low (2 rows) | niche | PARTIAL | W/props.rs:343-496 | Variants are drawn as justified. |
| Mirror indents; Automatically adjust right indent; Snap to grid; Suppress line numbers; Textbox tight wrap; East Asian typography options; Vertical text direction; Frame; Document grid (9 rows) | niche | PRESERVED | A§3b | Kept in the tree, not acted on. |

### 5. Styles and themes

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Paragraph style | core | DONE | W/session.rs:525; C/format_bar.rs:103-166; S/commands.rs:720 | Dropdown, panel, verb `style`. |
| Character style | core | DONE | C/panel.rs:737-746; W/restyle.rs:318-340 | Character style buttons in the Styles level. A linked style applies to the selected words only. |
| Linked style | common | DONE | W/restyle.rs:318-340,418; W/builtin.rs:28-82 | The `w:link` pair is followed: a heading style applied to part of a paragraph uses its character style. |
| Table style | common | PARTIAL | W/styles.rs:206-226,454-468; W/layout/table.rs:165-190,630-729; W/tables.rs:607 | The style's table borders, cell margins, cell shading, paragraph and run properties and its conditional formats apply. The style's own `basedOn` chain is not followed and the borders in a conditional format are not applied. The panel offers two styles; the verb `table style <id>` takes any built in one. |
| Styles pane | core | PARTIAL | C/panel.rs:695-773; C/keys.rs:74 | Ctrl+Alt+Shift+S opens a list of every paragraph style with Modify, the built in styles and the character styles. No previews. |
| Quick Styles gallery | core | PARTIAL | C/format_bar.rs:133-166 | A dropdown with favourites first; no gallery with previews. |
| Modify Style | core | DONE | W/restyle.rs:19-123; C/panel_setup.rs:238-380; S/commands.rs:720 | The Modify level changes font, size, bold, italic, underline, colour, alignment, spacing, indents and line spacing, and every paragraph in the style follows. Verb `style modify`. |
| Modify Style Format menu | common | PARTIAL | C/panel_setup.rs:238-380 | Font and paragraph basics only; no tabs, borders, numbering or language entries. |
| Create a Style from selection | core | DONE | C/panel.rs:747-773,774; W/restyle.rs:124-187 | Name field and Create; verb `style new`. |
| Style based on | core | DONE | W/styles.rs:189-206; C/panel_setup.rs:345-360 | Chain walked for paragraph and character styles, and set in the Modify level. |
| Style for following paragraph | core | DONE | W/edit.rs:197-268; C/panel_setup.rs:345-360 | Enter at the end of a heading takes `w:next`; set in the Modify level. |
| Update to match selection | common | DONE | W/restyle.rs:188-242; C/panel_setup.rs:364-370 | Button and verb `style update`. |
| Delete style, Revert to default | common | DONE | W/restyle.rs:243-317; C/panel_setup.rs:370-385 | Delete, and Revert restores a built in style to Word's own definition. |
| Built in styles | core | DONE | W/builtin.rs:23-82; WT/fixtures/builtin-styles.docx | 109 styles made on first use from an embedded template Word wrote: headings, Title, Quote, TOC, notes, Header and Footer, List Bullet and List Number, Body Text, Plain Text, and 24 table styles. Not every latent style of Word's gallery. |
| Style Sets (Design tab) | common | MISSING | C/panel.rs:695-773 | No style sets. |
| Themes (Design tab) | common | PRESERVED | W/styles.rs:21-110 | The theme is read for fonts and colours and cannot be changed. |
| Theme Colors | common | PARTIAL | W/styles.rs:21-110; C/panel.rs:247-272 | Eight theme colours can be applied to text; no Customize Colors. |
| Theme Fonts | common | PARTIAL | W/styles.rs:75-83 | Theme fonts resolve; they cannot be chosen or customised. |
| Document Defaults | core | DONE | W/styles.rs:245-312 | `docDefaults` start the resolution. |
| Set As Default | common | MISSING | C/panel_setup.rs:238-380 | No such command. |
| Normal style reset | common | DONE | W/ops.rs:1426-1440; W/shortcuts.rs:89; C/keys.rs:52,60 | Ctrl+Shift+N applies Normal and Ctrl+Q resets the paragraph's formatting. |
| Table style options | common | PARTIAL | W/tables.rs:607-636; W/layout/table.rs:630-690; S/commands.rs:1617-1715 | The six options (header row, total row, first and last column, banded rows and columns) are set by `table style <id> [htflrc]` and honoured in layout. The panel has no tick boxes. |
| Conditional table formatting | common | PARTIAL | W/layout/table.rs:630-729; W/styles.rs:206-226,436-450 | First and last row and column, banded rows and columns and the four corner cells are worked out from the table look and the band sizes. Shading, paragraph and run properties apply; borders in a conditional format do not. |
| Style property resolution | core | PARTIAL | W/styles.rs:189-226,245-340 | Defaults, paragraph chain, character chain, direct formatting, toggles by XOR and the table style with its conditional formats. The table style's own `basedOn` chain is not walked. |
| List style | niche | PARTIAL | W/numbering.rs:16,69 | `numStyleLink` is resolved; no list style editor. |
| Latent styles; Theme Effects; Limit allowed styles (3 rows) | niche | PRESERVED | A§3b | Kept in the XML. |
| Apply Styles; Style Inspector; Reveal Formatting; Automatically update; Manage Styles; Organizer; Paragraph Spacing gallery; Style separator; Style area pane; Keyboard shortcut per style (10 rows) | niche | MISSING | A§4b | No code. |

### 6. Lists

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Bullets | core | DONE | W/ops.rs:47; C/format_bar.rs:200-214; S/actions.rs:564; C/keys.rs:61 | Toggle, action and verb. Ctrl+Shift+L applies the List Bullet style. |
| Define New Bullet: Symbol | common | PARTIAL | W/lists.rs:80-100; S/commands.rs:780-840 | `list symbol <character> [font]` defines the bullet. The window has no dialog for it. |
| Bullet font, size, color, alignment | common | PRESERVED | W/numbering.rs:117-148; W/lists.rs:80-100 | The label's run properties are drawn. Only the symbol and its font can be set, by verb. |
| Numbering | core | DONE | W/ops.rs:47; C/format_bar.rs:200-214; S/actions.rs:574; W/ops.rs:2010 | Toggle, action and verb. |
| Number formats | core | PARTIAL | W/numbering.rs:228-260,408; W/lists.rs:101 | 17 formats, including Roman, letters, ordinal and text numbers and enclosed numbers. Others print as decimal. |
| Define New Number Format | common | PARTIAL | W/lists.rs:101-120; S/commands.rs:780-840 | `list format <numFmt> [before] [after]` defines it. The window has no dialog for it. |
| Multilevel List | core | PARTIAL | W/ops.rs:116; W/lists.rs:134-169; C/panel.rs:812-832 | Levels change with Tab and Shift+Tab and in the Lists level; `list define` makes one. No multilevel gallery. |
| Define New Multilevel List | common | PARTIAL | W/lists.rs:134-169,310; S/commands.rs:780-840 | `list define <format:text>...` defines the levels. The window has no dialog for it. |
| Link Level to Style | common | PARTIAL | W/numbering.rs:100-106 | Honoured on read; cannot be created. |
| Restart at 1 | core | DONE | W/ops.rs:148; C/panel.rs:821-829; S/actions.rs:604 | Restart here with a start value. |
| Continue numbering | core | PARTIAL | W/lists.rs:37-71,334; S/commands.rs:780-840 | `list continue` joins the list before. The Lists level has no button for it. |
| Set Numbering Value | common | DONE | W/lists.rs:72-79; C/panel.rs:821-829; S/commands.rs:794 | Start at in the Lists level and `list value <n>` set the number of the paragraph. |
| Adjust List Indents | common | PARTIAL | W/lists.rs:121-133; S/commands.rs:780-840 | `list indents <pt> [hanging]` sets them; `suff` is honoured. No dialog. |
| Change List Level | core | DONE | W/ops.rs:116; W/lists.rs:169-205; C/keys.rs:419-421; S/actions.rs:584,594 | Tab and Shift+Tab, and the actions. |
| Automatic bullets and numbering as you type | core | PARTIAL | C/keys.rs:287-313 | `- `, `* `, `1. ` and `1) ` on the space; Word's trigger is Enter; only "1" starts a numbered list. |
| Number restart rules | common | DONE | W/numbering.rs:138-225; W/numbering.rs:444 | `lvlRestart` honoured. |
| List Number and List Bullet styles | common | DONE | W/builtin.rs:28-82,281; WT/fixtures/builtin-styles.docx | The built in template carries List Bullet, List Bullet 2 and 3, List Number, List Number 2 and 3 with their numbering. |
| Lists inside table cells | core | DONE | W/layout/table.rs:190-260 | Cell paragraphs use the same list code. |
| numbering.xml model | core | DONE | W/numbering.rs:13-228; W/lists.rs:17-33,231-283; W/layout/drawing.rs:196-224 | Abstract numbers, overrides, level text, start, restart, suffix, `isLgl`, symbol bullets and picture bullets (`lvlPicBulletId`) are read, drawn and written. |
| Legal style numbering | niche | DONE | W/numbering.rs | `isLgl` honoured. |
| LISTNUM field; AUTONUM, AUTONUMLGL, AUTONUMOUT (2 rows) | niche | PRESERVED | W/layout/paragraph.rs:236-300 | Stored result shown. |
| Define New Bullet: Picture | niche | PRESERVED | W/order.rs:232; A§3b | `lvlPicBulletId` and `numPicBullet` are kept and not handled. |
| Define New List Style | niche | MISSING | A§1b | Not found. |

### 7. Tables

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Insert Table grid | core | DONE | W/ops.rs:221; W/edit.rs:706-760; C/panel.rs:833-857; S/actions.rs:615 | Rows by columns fields in the Insert level and a 3 by 3 `/` item; no hover grid. |
| Insert Table dialog | core | PARTIAL | C/panel.rs:833-857; S/commands.rs:1336-1403 | Rows and columns only; no fixed width or AutoFit choice. The verb `insert table [rows] [columns]` is the same. |
| Draw Table | common | MISSING | C/page_view.rs:766-960 | Not done. |
| Convert Text to Table | common | DONE | W/tables.rs:235-300; S/actions.rs:663; S/commands.rs:1617-1715 | Splits the selected paragraphs at tabs, commas or paragraph marks. Verb `table convert to-table`. |
| Convert Table to Text | common | DONE | W/tables.rs:301-367; S/actions.rs:656 | Separated by tabs, commas or paragraph marks. Verb `table convert to-text`. |
| Insert rows above and below | core | DONE | W/ops.rs:247; S/actions.rs:1510,1517; C/panel.rs:856-869 | Panel, right click menu, action and verb. |
| Insert columns left and right | core | DONE | W/ops.rs:247; S/actions.rs:1524,1531; C/panel.rs:856-869 | Panel, right click menu, action and verb. |
| Delete cells, rows, columns, table | core | PARTIAL | W/ops.rs:247; S/actions.rs:1538-1552 | Rows, columns and the table; no delete of cells with a shift. |
| Merge Cells | core | DONE | W/tables.rs:106-199; S/actions.rs:625; C/context_menu.rs:84 | Merges a selected block of cells, found from the selection across cells. Merge right and down also work from one cell. |
| Split Cells | core | PARTIAL | W/ops.rs:381; S/actions.rs:1559 | Splits a merged cell back; no rows and columns prompt. |
| Split Table | common | DONE | W/tables.rs:200-234; S/actions.rs:632 | Splits at the caret's row and leaves a paragraph between the tables. |
| Table Properties: Table tab | common | DONE | W/tables.rs:481-606; C/panel.rs:1524-1560; W/layout/table.rs:453-496 | Alignment, indent, preferred width (points, percent or auto) and cell margins by verb; alignment and width in the Table level. |
| Table Options | common | PARTIAL | W/tables.rs:481-606; W/layout/table.rs:182-190 | Default cell margins are set by `table properties margins`; spacing between cells (`tblCellSpacing`) is not read. |
| Table Properties: Row tab | common | DONE | W/tables.rs:481-606; C/panel.rs:1552-1575; W/layout/table.rs:57-140,373 | Height with a rule, repeat as header row, and allow row to break across pages. |
| Table Properties: Column tab | common | DONE | W/ops.rs:413; W/tables.rs:35-105; C/objects.rs:61-135; S/commands.rs:1621-1634 | Drag a column's border or use `table width <column> <pt>`; the next column gives or takes the difference. |
| Table Properties: Cell tab | common | PARTIAL | W/tables.rs:560-606; C/panel.rs:1575-1583 | Vertical alignment in the Table level; cell margins are drawn and have no control. |
| Table Properties: Alt Text | common | PRESERVED | W/order.rs:142-143 | `tblCaption` and `tblDescription` stay in `tblPr`; nothing sets them. |
| Repeat Header Rows | core | DONE | W/layout/table.rs:465; W/layout/mod.rs:1879-1887; W/ops.rs:393; C/panel.rs:1565 | Panel button, table property and layout. |
| AutoFit Contents | core | MISSING | W/layout/table.rs:471-550 | A table with no usable grid is sized from its contents, but there is no command and `tblLayout` is not read. |
| AutoFit Window | core | PARTIAL | W/layout/table.rs:453-496; W/tables.rs:481-606 | `table properties width 100%` sets a percent width that rescales the grid; no command by that name. |
| Fixed Column Width | core | PARTIAL | W/layout/table.rs:471-550; W/ops.rs:413 | Grid widths are used as fixed, and are set by dragging or `table width`; no command by that name. |
| Table layout algorithm | core | DONE | W/layout/table.rs:453-575; W/layout/tests/table_widths.rs; WT/fixtures/table-widths.docx | The grid, `tcW` for a table with no grid, then content; `tblW` in points, percent or auto. Matches Word on the six cases of table-widths. |
| Distribute Rows Evenly, Distribute Columns Evenly | core | DONE | W/tables.rs:425-480; S/actions.rs:639,649 | Rows to the tallest row's height and columns to equal width. Right click menu, action and verb. |
| Cell Size group | core | DONE | W/tables.rs:481-606; W/ops.rs:413; C/panel.rs:1552-1558 | Row height with a rule and column width. |
| Cell alignment grid | common | PARTIAL | C/panel.rs:1575-1583; W/layout/table.rs:286-295 | Vertical alignment is set in the Table level; horizontal alignment is the paragraph's. No 3 by 3 grid. |
| Text Direction in cells | common | PRESERVED | W/tables.rs:560-590; W/order.rs:177 | `table properties direction` writes `textDirection`; vertical text is not drawn. |
| Cell Margins | common | PARTIAL | W/layout/table.rs:608-629; W/tables.rs:481-606 | Table margins are set by verb and cell margins are drawn; spacing between cells is not read. |
| Table borders | core | PARTIAL | W/tables.rs:636-686; W/layout/table.rs:400-459; C/panel.rs:1594-1605 | All, outside, inside, none or one side with a width and colour. Only single and double are drawn; no diagonals. |
| Table shading | core | PARTIAL | W/ops.rs:441-466; C/panel.rs:896-912 | Shades the caret's cell with 5 swatches or any colour by verb; no row or table shading. Table styles shade rows and columns. |
| Table Styles gallery | common | PARTIAL | W/tables.rs:607-636; C/panel.rs:1584-1593 | Two buttons in the Table level; the verb takes any of the 24 built in table styles. No gallery. |
| Clear table style | common | DONE | W/tables.rs:607-636; C/panel.rs:1586 | The Plain button applies Table Normal. |
| Sort (table) | common | DONE | W/tables.rs:368-424; S/actions.rs:670 | By column, as text, number or date, with or without a header row. Verb `table sort`. |
| Formula | common | DONE | W/fields.rs:715-1000; W/ops.rs:1361; C/panel.rs:1040-1066 | `= SUM(ABOVE)` and other formulas insert with `insert field` and evaluate, with `\#` pictures and table ranges. |
| Update table formulas | common | DONE | W/references.rs:342-410; C/keys.rs:454 | F9 updates the formulas with the other fields. |
| Nested tables | common | DONE | W/layout/mod.rs:880-885; WT/fixtures/tables.docx | Laid out and drawn. |
| Table move handle | common | MISSING | C/page_view.rs:766-960 | Not found. |
| Table resize handle | common | MISSING | C/page_view.rs:766-960 | Not found. |
| Resize by dragging borders | core | PARTIAL | C/objects.rs:61-135; C/page_view.rs:842-860 | A column's border is dragged. A row's height is not. |
| Select cell, row, column, table | core | DONE | W/tables.rs:106-141; S/actions.rs:682,689,696; C/page_view.rs:870-940 | Drag across cells, or the select row, column and table actions. Verb `table select`. |
| Tab in last cell | core | DONE | C/keys.rs:419-423,767 | Appends a row. |
| View Gridlines | core | MISSING | C/page_view.rs:698-725 | The Gridlines view draws the page's drawing grid. A table with no borders has no guide lines. |
| Floating table | common | PRESERVED | W/layout/table.rs:182-190 | `tblpPr` is kept and not read. |
| Table caption | common | DONE | W/references.rs:454-486; S/actions.rs:953; S/commands.rs:913 | Caption with the label Table above or below, a SEQ field that renumbers on update. |
| Table Design and Layout tabs | core | PARTIAL | C/panel.rs:856-869,1524-1608 | The Insert level's This table buttons and the Table level; no tabs of Word's shape. |
| Cell merging model | core | DONE | W/layout/table.rs:447-455,591-607 | `gridSpan` and `vMerge` read, drawn, written and merged by block. |
| Table grid | core | DONE | W/model.rs:243-265; W/edit.rs:706-760; W/layout/table.rs:471 | `tblGrid` read, made and kept in step by column edits. |
| Table look | common | PARTIAL | W/layout/table.rs:630-690; W/tables.rs:607-636 | `tblLook` is read and set by `table style <id> [htflrc]`; the panel has no tick boxes. |
| Table Positioning; Diagonal cell borders (2 rows) | niche | PRESERVED | A§3b | Kept, not read. |
| Excel Spreadsheet; Quick Tables; Insert Cells; Bookmarks in table formulas; New and Modify Table Style; Move rows with Alt+Shift+Up/Down (6 rows) | niche | MISSING | A§4b | No code. |

### 8. Illustrations

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Pictures: This Device | core | DONE | S/actions.rs:178-185; S/commands.rs:2047-2075; W/media.rs:28-68; C/page_view.rs:491-512 | The dialog lists png, jpg, gif and bmp; the verb takes any format the window decodes. EMF, WMF and SVG draw, and a picture can be pasted from the clipboard. |
| Screenshot, Screen Clipping | common | MISSING | S/mod.rs:506-523 | No screen capture. A picture already on the clipboard can be pasted. |
| Picture Format tab | core | PARTIAL | C/panel.rs:1367-1523; C/objects.rs:170-384; W/pictures.rs:64-260 | A picture is a selectable object with handles. The Picture level sets size, ratio, wrap, position, order, crop and alt text. No corrections, colour, styles or borders. |
| Corrections | common | MISSING | C/panel.rs:1367-1523 | Not found. |
| Color | common | MISSING | C/panel.rs:1367-1523 | Not found. |
| Compress Pictures | common | MISSING | C/panel.rs:1367-1523 | Not found. |
| Change Picture, Reset Picture | common | PARTIAL | W/pictures.rs:204-221; S/actions.rs:1429 | Reset Picture restores the original size and crop. Change Picture is not offered. |
| Picture Styles | common | MISSING | C/panel.rs:1367-1523 | Not found. |
| Picture Border | common | MISSING | C/panel.rs:1367-1523 | Not found. |
| Crop | common | DONE | W/pictures.rs:147-187; W/layout/drawing.rs:225-233; C/panel.rs:1478-1493; S/actions.rs:1418 | `a:srcRect` is applied to the drawing and set in the Picture level by percent. |
| Format Picture pane | common | PARTIAL | C/panel.rs:1367-1523 | The Picture level has size, wrap, position, order, crop and alt text; no effects, fill or line. |
| Lock aspect ratio | core | DONE | C/panel.rs:1387-1400; C/objects.rs:287-336; W/pictures.rs:90 | A tick box, and a corner handle keeps the ratio. |
| Rotate and flip | common | PARTIAL | W/layout/mod.rs:259-300; W/layout/drawing.rs:297-340 | A flip is drawn. Rotation is kept and not applied, and neither has a control. |
| Shapes gallery | common | PARTIAL | W/pictures.rs:330-346; W/layout/drawing.rs:21-35; S/actions.rs:1264-1334; C/panel.rs:923-942 | Ten shapes can be made and are drawn. Other presets draw their cached picture or a labelled box. |
| Shape Format tab | common | PARTIAL | C/panel.rs:1367-1523; W/pictures.rs:64-260 | Size, wrap, position, order and alt text. No fill, outline or effects. |
| Text in shapes | common | DONE | W/layout/drawing.rs:386-536; C/objects.rs:266-286; W/read.rs:71-123 | Text in `wps:txbx` is read, laid out inside the shape, and edited by clicking into it. |
| Chart | common | PRESERVED | W/layout/drawing.rs:156-176 | The chart part is kept. It draws the picture Word cached beside it, or a labelled box. Drawing charts is a non goal of the TDD. |
| Chart Design and Format | common | MISSING | W/layout/drawing.rs:156-176 | Authoring charts is a non goal of the TDD. |
| Text Box: Draw | core | DONE | W/pictures.rs:309-329; S/actions.rs:1257; S/commands.rs:1336-1403 | A text box is inserted at the caret with `/`, the Insert level or `insert textbox [width height]`, and its text is edited in place. |
| Text box margins and autofit | common | PARTIAL | W/layout/drawing.rs:386-536; W/pictures.rs:770-786 | The insets in `bodyPr` are read and the text is laid out inside them. Autofit and the margins have no control. |
| Wrap Text: In Line with Text | core | DONE | W/layout/paragraph.rs:588-653; S/actions.rs:1341 | Inline pictures are laid out as characters. |
| Wrap Text: Square | core | DONE | W/layout/mod.rs:1432-1560; W/layout/tests/wrapping.rs; WT/fixtures/wrapping.docx | Lines avoid the picture and its distances and the wrap side. Matches Word on the wrapping fixture. |
| Wrap Text: Tight | common | PARTIAL | W/layout/mod.rs:259-270,1432-1470; W/layout/drawing.rs:84-110 | Text goes around the bounds of the wrap polygon, not around its outline. |
| Wrap Text: Top and Bottom | core | DONE | W/layout/mod.rs:1432-1560 | Text runs only above and below the picture. |
| Wrap Text: Behind Text | core | DONE | W/layout/mod.rs:1439-1443; S/actions.rs:1376 | Drawn before the text. |
| Wrap Text: In Front of Text | core | DONE | W/layout/mod.rs:1445,1662-1670; S/actions.rs:1383 | Drawn after the text. |
| Wrap text sides | common | DONE | W/layout/mod.rs:265-280,1421-1478 | `wrapText` (both sides, left, right, largest) decides which side text may use. |
| Distance from text | common | DONE | W/layout/mod.rs:1452-1470; W/layout/drawing.rs:84-110 | `distT`, `distB`, `distL` and `distR` keep text away; they have no control. |
| Position presets | common | MISSING | W/pictures.rs:130-147 | Not found. |
| Layout dialog: Position | common | PARTIAL | W/pictures.rs:130-146,646-700; C/panel.rs:1431-1468 | Relative to page, margin, column, paragraph, line or character, with an offset, set in the Picture level. No alignment choice. |
| Layout dialog: Text Wrapping | common | PARTIAL | W/pictures.rs:104-129,618-645; C/panel.rs:1414-1430 | The wrap kind is set; the side and the distances are read and have no control. |
| Layout dialog: Size | common | PARTIAL | W/pictures.rs:90-103; C/panel.rs:1387-1400 | Width and height in points. Relative size and rotation are not applied. |
| Anchor, Lock Anchor | common | PARTIAL | C/objects.rs:156-170; W/pictures.rs:267-308 | The object is anchored to its paragraph and can be dragged to another; no anchor symbol and no lock. |
| Align | common | MISSING | W/pictures.rs:130-146 | Not found. |
| Group, Ungroup, Regroup | common | PRESERVED | W/layout/drawing.rs:156-176 | `wpg` is kept; a group draws its cached picture or a labelled box. |
| Z order | core | PARTIAL | W/pictures.rs:222-254; C/context_menu.rs:36-60; S/actions.rs:1390-1411 | Bring forward, send backward, to front and to back change `relativeHeight` and `behindDoc`. Behind and in front text are respected; the order among pictures at one level is the order in the document. |
| Rotate | common | PRESERVED | W/layout/drawing.rs:297-340 | `rot` is kept and not applied. |
| Alt Text | core | DONE | W/pictures.rs:188-203; C/panel.rs:1494-1523; S/actions.rs:1446; C/context_menu.rs:45 | Set in the Picture level, the right click menu and `picture alt <text>`; Mark as decorative. |
| Insert Caption | core | PARTIAL | W/references.rs:454-486; S/actions.rs:953-962; S/commands.rs:913-921 | The action inserts a Figure caption below the paragraph; the verb takes any label and `--above`. SEQ numbers are worked out when fields update. No chapter numbers and no label list. |
| Selection Pane | common | MISSING | C/objects.rs:170-215 | Not found. |
| Hyperlink on picture | common | PRESERVED | W/media.rs:144-208 | `hlinkClick` is kept. |
| SVG with PNG fallback | common | DONE | W/layout/paragraph.rs:588-601; C/page_view.rs:491-512 | `a:blip r:embed` is the PNG fallback and is drawn. A bare SVG is drawn too. |
| Stock Images; Online Pictures; Online Video; 3D Models; Draw tab (5 rows) | niche | OUT-OF-SCOPE | B§8 | Stock and online pictures and online video need Microsoft services, 3D models and ink are excluded by the brief. |
| Insert and Link; Remove Background; Artistic Effects; Transparency; Picture Effects; Picture Layout (SmartArt); New Drawing Canvas; Edit Points; Change Shape and Merge Shapes; Connectors; Icons; SmartArt; SmartArt Design tab; WordArt; Text Box: Built in; Linked text boxes; Wrap Text: Through; Edit Wrap Points; Linked pictures; Embedded chart workbook (20 rows) | niche | PRESERVED | A§3b | Parts and XML kept; drawn as a grey box where there is a frame. |

### 9. Page layout and sections

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Margin presets | core | PARTIAL | W/ops.rs:571; S/actions.rs:843-863; S/commands.rs:1900-1942 | Normal, Narrow and Wide in the Page level, Moderate by verb, and mirrored margins by a tick box. No Office 2003 or Last Custom. |
| Custom margins | core | DONE | W/page_setup.rs:179; C/panel_setup.rs:27-65; C/panel.rs:1124-1134 | Top, bottom, inside and outside margins, gutter, and header and footer distances. |
| Gutter and gutter position | common | PARTIAL | W/page_setup.rs:179-192; W/layout/section.rs:61-141; C/panel_setup.rs:44-50 | The gutter is set in the Page setup level and used by layout. Its position (top or side) has no control. |
| Mirror margins | common | DONE | W/page_setup.rs:193; W/layout/mod.rs:2113-2175; C/panel_setup.rs:62 | Inside and outside margins swap on even pages. |
| Orientation | core | DONE | W/ops.rs:542; S/actions.rs:823,833 | Panel and verb. |
| Size | core | DONE | W/ops.rs:507; W/page_setup.rs:200; C/panel_setup.rs:67-80 | 8 named sizes and a custom width and height. No envelope sizes. |
| Columns presets | core | PARTIAL | W/ops.rs:586; S/actions.rs:873-893 | One, two or three equal columns. No Left or Right presets. |
| Columns custom | common | PARTIAL | W/layout/section.rs:161-186; W/layout/mod.rs:1342-1370 | Explicit widths and the line between columns are read and drawn; there is no control for widths or the line. |
| Balanced columns | common | DONE | W/layout/mod.rs:1301-1341; W/layout/tests/column_balance.rs; WT/fixtures/columns-balance.docx | Columns are balanced before a continuous section break. |
| Page Break (Ctrl+Enter) | core | DONE | C/keys.rs:373-381; S/actions.rs:710 | Ctrl+Enter and the action. |
| Column Break (Ctrl+Shift+Enter) | common | DONE | C/keys.rs:378-381; S/actions.rs:720 | Ctrl+Shift+Enter and the action. |
| Section Break: Next Page | core | DONE | W/ops.rs:600; S/actions.rs:740; W/ops.rs:2145 | The action, the Insert level and the verb `insert section-break`. |
| Section Break: Continuous | core | DONE | W/layout/mod.rs:836-868; S/actions.rs:750 | A continuous break keeps the page when the page sizes are equal. |
| Hyphenation | common | DONE | W/layout/paragraph.rs:928-1070; W/page_setup.rs:259; C/panel_setup.rs:126-136; W/layout/tests/hyphenation.rs | Automatic hyphenation with English patterns, set by a tick box in the Page setup level. |
| Hyphenation Options | common | PARTIAL | W/page_setup.rs:259; W/page_setup.rs:60 | Automatic on or off. The hyphenation zone and the limit on consecutive hyphens are kept and not used. |
| Optional hyphen | common | DONE | W/shortcuts.rs:428; S/actions.rs:1875; C/keys.rs:92 | Ctrl+- inserts a soft hyphen, a break point that prints only at a line end. |
| Nonbreaking hyphen and space | common | DONE | W/shortcuts.rs:428; S/actions.rs:1815,1865; C/keys.rs:90-91 | Ctrl+Shift+- and Ctrl+Shift+Space. |
| Indent and Spacing group | core | DONE | C/panel.rs:402-445; C/views.rs:78-300 | The Paragraph level and the ruler. |
| Watermark | common | PRESERVED | W/layout/drawing.rs:241-297 | A VML or WordArt header shape draws its cached picture or a labelled box, not a watermark. |
| Page Color | common | PARTIAL | C/panel.rs:1161-1190; W/layout/mod.rs:547-552 | 5 swatches, solid only. |
| Page Borders | common | DONE | W/page_setup.rs:266-290; W/layout/mod.rs:2265-2296; C/panel_setup.rs:138-165 | None, thin, box or thick box with a colour, drawn on every page. No art borders. |
| Vertical alignment on page | common | DONE | W/page_setup.rs:216-238; W/layout/mod.rs:2190-2218; C/panel_setup.rs:76-86 | Top, center, justified and bottom. |
| Different First Page | core | DONE | W/page_setup.rs:291; C/panel_setup.rs:169-180 | A tick box in the Page setup level. |
| Different Odd and Even | core | DONE | W/page_setup.rs:299; C/panel_setup.rs:181-190 | A tick box in the Page setup level. |
| Page numbering per section | core | DONE | W/page_setup.rs:307-328; C/panel_setup.rs:199-236 | Start at and the number format (Roman, letters, decimal) for each section. |
| Link to previous | core | DONE | W/page_setup.rs:329-384; C/panel_setup.rs:191-198 | Link and unlink buttons for the header and the footer of a section. |
| Per section settings | core | PARTIAL | W/layout/section.rs:61-141; W/page_setup.rs:147-424 | Size, orientation, margins, columns, headers, footers, line numbers, vertical alignment, borders, note options and page numbering. Not text direction or the document grid. |
| Page Setup dialog | core | PARTIAL | C/panel_setup.rs:27-236; S/actions.rs:2149 | The Page setup level has margins, paper, layout and the rest. No Document Grid and no Set As Default. |
| Footnotes and endnotes per section | common | DONE | W/page_setup.rs:385-424; W/layout/notes.rs:16-140; C/panel_setup.rs:485-563 | Number format, start, restart (each page, each section, continuous) and placement, for each section. |
| Gridlines and ruler | core | DONE | C/views.rs:78-300; C/page_view.rs:698-725; S/actions.rs:1937,2009 | The ruler shows indents and tab stops that can be dragged; Gridlines draws the page grid. |
| Section Break: Even Page; Odd Page (2 rows) | niche | PARTIAL | S/commands.rs:498-503 | By verb only. |
| Multiple pages 2 per sheet; Book fold; Paper source; Text Wrapping Break; Line Numbers; Print Background Colors and Images; Section text direction; Suppress endnotes (8 rows) | niche | PRESERVED | A§3b | Kept in `sectPr` or settings, not acted on. |

### 10. Headers, footers and page numbers

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Insert Header, Insert Footer | core | PARTIAL | W/ops.rs:638-701; C/panel.rs:1137-1160 | The Page level's Header and Footer fields make the part and put text in it, with {PAGE} and {NUMPAGES}. No gallery of designs. |
| Edit Header, Edit Footer | core | DONE | C/page_view.rs:61-77; S/tests.rs:299 | A double press on the margin edits the header or footer in place. |
| Remove Header, Remove Footer | core | DONE | W/ops.rs:899-920; S/actions.rs:1628; C/context_menu.rs:81 | Empties the header or footer and keeps its paragraph's formatting, as Word's Remove Header does. The part and reference stay. |
| Header & Footer tab | core | PARTIAL | C/page_view.rs:195-225; C/panel_setup.rs:169-236 | A dashed marker and label on the page, and the headers and footers section of the Page setup level. |
| Different First Page | core | DONE | W/page_setup.rs:291; C/panel_setup.rs:169-180 | A tick box in the Page setup level. |
| Different Odd & Even Pages | common | DONE | W/page_setup.rs:299; C/panel_setup.rs:181-190 | A tick box in the Page setup level. |
| Link to Previous | core | DONE | W/page_setup.rs:329-384; W/layout/mod.rs:917-977; C/panel_setup.rs:191-198 | Link and unlink buttons; inheritance is honoured in layout. |
| Go to Header, Go to Footer, Previous, Next | common | MISSING | C/page_view.rs:61-77 | Only a double press on the margin. |
| Header from Top, Footer from Bottom | common | DONE | C/panel_setup.rs:44-50; W/layout/section.rs:61-141 | Both distances are set in the Page setup level. |
| Page Number gallery | core | PARTIAL | S/actions.rs:792,903; W/page_setup.rs:307-328 | One action puts a page number field at the caret, and another makes "Page {PAGE} of {NUMPAGES}" in the footer. No gallery of positions. |
| Format Page Numbers | common | DONE | W/layout/fields.rs:15-53; W/page_setup.rs:307-328; C/panel_setup.rs:199-236 | Start at and the number format for each section; the `\*` switches of a PAGE field are applied. |
| Remove Page Numbers | common | PARTIAL | W/ops.rs:899-920; S/actions.rs:1628 | Remove Header or Footer empties the footer holding them. There is no command that removes only the numbers. |
| Page X of Y | core | DONE | S/actions.rs:903; W/layout/fields.rs:21-45; W/ops.rs:2121 | The Page numbers action makes the footer; the fields count the pages as laid out. |
| Date & Time | common | DONE | S/actions.rs:780; C/panel.rs:945-966; W/fields.rs:596-705 | The Insert level offers date and time formats and writes a DATE field that updates. |
| Document Info | common | DONE | C/panel.rs:967-984; W/fields.rs:121-240 | Title, Author, Subject, Keywords, File name, Pages and Words fields are inserted and worked out on update. |
| Quick Parts in header | common | PARTIAL | C/panel.rs:967-1066 | Document property and field boxes insert at the caret in a header. There are no building blocks. |
| Pictures in header | core | PARTIAL | W/layout/mod.rs:2011-2034; W/media.rs:69-87 | A header's pictures lay out and draw. Inserting a picture while the caret is in a header adds the relationship to the main part, not the header's, so Word is expected to lose it (read from the code, not run). |
| Close Header and Footer | core | DONE | C/keys.rs:429-441; S/actions.rs:1635 | Esc, and the action. |
| Watermark in header | common | PRESERVED | W/layout/drawing.rs:241-297 | Draws its cached picture or a labelled box, not a watermark. |
| Header and footer types | core | DONE | W/layout/mod.rs:917-977; W/layout/section.rs:61-141 | Default, first and even with inheritance. |
| Tables and text boxes in headers | common | DONE | W/ops.rs:1486-1515; W/layout/mod.rs:2011-2034; W/layout/drawing.rs:386-536 | Tables and text boxes in a header lay out and draw. Setting the text from the panel replaces only the first paragraph of text and keeps the rest of the story. |
| Show Document Text | niche | MISSING | A§6c | Not found. |
| Insert Alignment Tab | niche | PRESERVED | W/layout/paragraph.rs:516-527 | `w:ptab` has no handler in layout and takes zero width. |
| SECTIONPAGES and SECTION fields | niche | PARTIAL | W/layout/fields.rs:187-205 | SECTIONPAGES evaluated, SECTION not. |

### 11. References

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Table of Contents: Automatic Table 1, 2 | core | DONE | W/references.rs:231-262,1053-1190; C/panel.rs:1209-1217; S/actions.rs:911 | Inserts a Contents heading and TOC1 to TOC9 entries with leaders, page numbers and `\h` links, inside the `w:sdt docPartObj` control Word uses. No gallery of designs. |
| Custom Table of Contents dialog | common | DONE | C/panel.rs:1619-1672; W/references.rs:23-86; S/commands.rs:187-226 | The Contents level sets levels, page numbers, right alignment, tab leader and hyperlinks, then inserts or replaces the table. Verb `toc insert [levels] --no-pages --no-links --leader`. |
| Update Table | core | PARTIAL | W/references.rs:97-165,275-298; S/actions.rs:979 | Finds a table inside a content control as well as in the body, and rebuilds its entries and page numbers. Headings inside a table are left out of the list. No page numbers only mode. |
| Remove Table of Contents | common | DONE | W/references.rs:299-341; S/actions.rs:990; S/commands.rs:214 | Removes the table and its control. Verb `toc remove`. |
| Add Text | common | MISSING | C/panel.rs:1209-1275 | No command to mark a paragraph for a table of contents level. |
| Table of Figures | common | DONE | W/references.rs:185-230,231-262; S/actions.rs:964; S/commands.rs:187-226 | Lists the captions with a label (`toc insert --figures Figure`) and updates with the other tables. |
| Insert Footnote | core | DONE | W/ops.rs:706-776; S/actions.rs:922; C/keys.rs:79 | Ctrl+Alt+F inserts the note and puts the caret in it, ready to type. Verb `insert footnote <text>`. |
| Insert Endnote | core | DONE | W/ops.rs:706-776; S/actions.rs:932; C/keys.rs:80 | Ctrl+Alt+D, with the same behaviour. |
| Footnote and Endnote dialog | common | DONE | W/layout/notes.rs:16-145; W/page_setup.rs:385-424; C/panel_setup.rs:485-563 | The Note options level sets each kind's number format, start and restart rule, and where notes go. Convert between footnote and endnote. |
| Next Footnote, Show Notes | common | DONE | S/actions.rs:98-120,2117,2124,1608; C/context_menu.rs:84 | Next and previous footnote, and Go to Note from the right click menu. |
| Note numbering formats | common | DONE | W/layout/notes.rs:66-140; W/numbering.rs:228-262 | Arabic, Roman, letters and symbols, restarting by page or section. A `customMarkFollows` reference is not counted. |
| Insert Citation | common | MISSING | C/panel.rs:1209-1275 | Citations and bibliography are a non goal of the TDD. |
| Manage Sources | common | PRESERVED | W/package.rs:136-185 | The bibliography `customXml` part is kept byte for byte. |
| Bibliography styles | common | MISSING | C/panel.rs:1209-1275 | A non goal of the TDD. |
| Bibliography, References, Works Cited | common | PRESERVED | W/fields.rs:1-20 | A bibliography `w:sdt` and CITATION fields show their stored results. |
| Insert Caption | common | PARTIAL | W/references.rs:454-486; S/actions.rs:953; S/commands.rs:913 | The action inserts a Figure caption below; the verb takes any label and `--above`. SEQ numbers are worked out when fields update. No chapter number and no AutoCaption. |
| Cross-reference | core | DONE | W/references.rs:487-559; W/shortcuts.rs:295-326; C/panel.rs:1228-1260; S/commands.rs:922 | Choose a heading, caption or bookmark and insert its text, its page number or above/below. It writes a hidden `_Ref` bookmark and a REF, PAGEREF or NOTEREF field. |
| Bookmark | core | PARTIAL | W/ops.rs:1314-1360; C/panel.rs:1235-1256,1263-1274; C/keys.rs:78 | Insert, list and click to go (Ctrl+Shift+F5). No delete, hidden bookmarks or sort. |
| Hyperlink (Ctrl+K) | core | PARTIAL | W/ops.rs:1271; W/references.rs:560-690; C/panel.rs:1015-1040; C/keys.rs:337-345 | URL or `#bookmark` from the selection; edit, screen tip, remove and open by action or verb, and Ctrl+click follows. No dialog and no e mail or new document choice. |
| Hyperlink styles | core | PARTIAL | W/layout/paragraph.rs:296-300 | The Hyperlink style and colour apply; FollowedHyperlink is never used. |
| TOC field | core | PARTIAL | W/references.rs:50-86 | `\o`, `\h`, `\n`, `\c` and the heading levels are read on update; `\u`, `\t`, `\b` and the rest are ignored. |
| Manual Table; Convert notes; Continuation notice; Insert Placeholder; Update Index (5 rows) | niche | MISSING | A§4b | Not found. |
| TC field; Table of Authorities; Source types; Index: Mark Entry; XE field; Index: Insert Index; Master Document (7 rows) | niche | PRESERVED | A§3b | Fields and `w:subDoc` stay in the XML. |
| Note separators | niche | PARTIAL | W/layout/mod.rs:1010-1020; W/layout/paragraph.rs:469 | A rule is drawn; continuation separator never used because notes do not continue. |
| Smart Lookup; Researcher (2 rows) | niche | OUT-OF-SCOPE | B§11 | Web lookup and an online research service. |

### 12. Fields

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| PAGE | core | DONE | W/fields.rs:130; W/layout/fields.rs:21-45; W/layout/mod.rs:721-748; W/layout/tests/page_field.rs | Worked out in headers, footers and the body, from the page each one lands on. |
| NUMPAGES | core | DONE | W/fields.rs:131; W/layout/mod.rs:721-748 | Worked out in headers, footers and the body. |
| DATE | core | DONE | W/fields.rs:596-705; W/references.rs:342-410 | Rewritten on update fields, with every `\@` picture Word offers. |
| TIME | core | DONE | W/fields.rs:596-705; W/references.rs:342-410 | Rewritten on update fields. |
| CREATEDATE | common | DONE | W/fields.rs:159-162 | Read from the document's created date. |
| SAVEDATE | common | DONE | W/fields.rs:163-166 | Read from the document's modified date. |
| AUTHOR | common | DONE | W/fields.rs:135 | Read from the document properties and worked out on update. |
| TITLE, SUBJECT, KEYWORDS, COMMENTS | common | DONE | W/fields.rs:136-139 | Read from the document properties and worked out on update. |
| FILENAME | common | DONE | W/fields.rs:142-145 | The file's name, or its path with `\p`, when the document has a file. |
| DOCPROPERTY | common | DONE | W/fields.rs:141,244-272; W/properties.rs:18-160 | Built in and custom properties. |
| TOC | core | PARTIAL | W/references.rs:275-298; W/references.rs:50-86 | Updated through `update_tocs`, including a table inside a content control. See Update Table in section 11 for what the update reads. |
| REF | core | DONE | W/fields.rs:181-193 | The bookmark's text, its paragraph number with `\n`, or above/below with `\p`. |
| PAGEREF | core | DONE | W/fields.rs:194-200 | The page the bookmark lands on, or "on page N" with `\p`. |
| NOTEREF | common | DONE | W/fields.rs:201-206,387 | The number of the note the bookmark covers. |
| STYLEREF | common | DONE | W/fields.rs:207-210,415 | The text of the nearest paragraph in the style, for running headers. |
| SEQ | common | DONE | W/fields.rs:167-180; W/references.rs:454-486 | Counts in order, with `\r` and `\c`. |
| HYPERLINK | core | PRESERVED | W/fields.rs:9-11; W/read.rs:203 | The stored result is shown; the `w:hyperlink` element is modelled separately. |
| IF | common | DONE | W/fields.rs:219-228,555 | Compares and gives the true or false text. |
| = (Formula) | common | DONE | W/fields.rs:211-218,715-1060,1116 | Arithmetic, comparisons, `SUM`, `AVERAGE` and the other functions, table ranges such as `ABOVE`, and `\#` pictures. |
| MERGEFIELD | common | PRESERVED | W/fields.rs:20-30 | The stored result is shown. Mail merge is a non goal of the TDD. |
| SYMBOL | common | PRESERVED | W/fields.rs:20-30 | The stored result is shown. |
| CITATION | common | PRESERVED | W/fields.rs:20-30 | The stored result is shown. Citations are a non goal of the TDD. |
| BIBLIOGRAPHY | common | PRESERVED | W/fields.rs:20-30 | The stored result is shown. Bibliographies are a non goal of the TDD. |
| ADDRESSBLOCK, GREETINGLINE | common | PRESERVED | W/fields.rs:20-30 | The stored result is shown. Mail merge is a non goal of the TDD. |
| Switch `\* MERGEFORMAT` | common | PRESERVED | W/fields.rs:432-436 | Kept in the code. It says how a result is formatted, so nothing is applied. |
| Switches `\* Upper`, `Lower`, `FirstCap`, `Caps` | common | DONE | W/fields.rs:432-470; W/fields.rs:1100 | Applied to the result. |
| Switches `\* Arabic`, `Roman`, `roman`, `ALPHABETIC`, `alphabetic`, `Ordinal`, `Cardtext`, `Ordtext`, `Hex`, `Dollartext` | common | DONE | W/fields.rs:432-490; W/numbering.rs:228-262,335-378 | All ten forms, applied to a numeric result. |
| Switch `\@ "dddd, MMMM d, yyyy"` | core | DONE | W/fields.rs:596-722 | Year, month, day, weekday, hours (12 and 24), minutes, seconds, AM/PM and quoted text. |
| Switch `\# "#,##0.00"` | common | DONE | W/fields.rs:500-554 | `0`, `#`, grouping, decimals, percent, literal text and a section for negative numbers. |
| Update fields (F9) | core | DONE | W/references.rs:342-410,723-860; C/keys.rs:331-334,454 | F9 updates the fields in the selection, and Update Fields in the panel or `fields update` does the whole document: page fields, dates, references, sequences, properties, formulas and tables of contents. |
| Toggle field codes | core | DONE | S/actions.rs:1117; C/keys.rs:452; W/layout/paragraph.rs:617-683 | Shift+F9 and Alt+F9 show the codes in place of the results. |
| Unlink | common | DONE | W/references.rs:409-425,949-1013; S/actions.rs:1003; C/keys.rs:25 | Ctrl+Shift+F9 replaces the selected fields with their results. Verb `fields unlink`. |
| Field shading | common | DONE | C/page_view.rs:514-562; S/actions.rs:2019-2049; S/commands.rs:1110 | Always, when selected or never, as Word's option is. |
| Cached results | core | DONE | W/layout/paragraph.rs:236-300,617 | The code is hidden and the stored result is drawn until the field is updated. |
| Complex field storage | core | DONE | W/read.rs:338; W/model.rs:159-161 | `fldChar` and `instrText` round trip. |
| Simple field storage | core | DONE | W/read.rs:203 | `w:fldSimple` is a wrapper. |
| SECTIONPAGES | niche | PARTIAL | W/layout/fields.rs:187-205 | Evaluated in headers and footers. |
| Nested fields | niche | DONE | W/layout/paragraph.rs:236-300 | Stack of open fields. |
| Lock and unlock; Next field; Insert empty field (3 rows) | niche | MISSING | A§6d | Not found. |
| SECTION; NUMWORDS, NUMCHARS; PRINTDATE; EDITTIME; LASTSAVEDBY; FILESIZE; TEMPLATE; DOCVARIABLE; INFO; USERNAME, USERINITIALS, USERADDRESS; TC; TA, TOA; INDEX; XE; RD; AUTONUM family; LISTNUM; INCLUDETEXT; INCLUDEPICTURE; INCLUDE; LINK; EMBED; DDE, DDEAUTO; COMPARE; SKIPIF; MERGEREC, MERGESEQ; NEXT, NEXTIF; DATABASE; ASK; FILLIN; SET; QUOTE; BARCODE; EQ; ADVANCE; GOTOBUTTON, MACROBUTTON; PRIVATE; FORMTEXT family; `\* CHARFORMAT`; `\* LOCK`; Dirty fields; Update on open (42 rows) | niche | PRESERVED | A§3b | Stored result shown or the element kept; `w:dirty` and `updateFields` are not acted on. |

### 13. Mailings

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Insert Merge Field | core | PARTIAL | W/ops.rs:1361; S/commands.rs:1336-1403; W/fields.rs:20-30 | `insert field MERGEFIELD name` writes the field with an empty result. There is no data source. Mail merge is a non goal of the TDD. |
| Preview Results | core | MISSING | W/fields.rs:20-30 | Mail merge is a non goal of the TDD. |
| Finish & Merge: Edit Individual Documents | core | MISSING | W/fields.rs:20-30 | Mail merge is a non goal of the TDD. |
| Finish & Merge: Print Documents | core | MISSING | S/print.rs:161-201 | Mail merge is a non goal of the TDD. Print prints the document as it is. |
| Envelopes; Labels; Start Mail Merge types; Step by Step Mail Merge Wizard; Select Recipients: Type a New List; Select Recipients: Use an Existing List; Edit Recipient List; Filter and Sort recipients; Highlight Merge Fields; Address Block; Greeting Line; Update Labels; Merge to new document by section (13 rows) | common | MISSING | W/fields.rs:20-30 | Mail merge, envelopes and labels are a non goal of the TDD. |
| Mail merge number formats | common | PRESERVED | W/fields.rs:432-436 | `\#` and `\@` on MERGEFIELD stay in the code text. |
| Choose from Outlook Contacts; Match Fields; Rules; Check for Errors; Send Email Messages; Directory merge (6 rows) | niche | MISSING | PRD 244-248 | Out of scope. |
| Data source connection | niche | PRESERVED | W/package.rs | `mailMerge` in settings and the data source relationship are kept. |

### 14. Review

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Spelling & Grammar (Editor) | core | PARTIAL | W/spell.rs:43-220; S/spelling.rs:104-268; C/panel.rs:1675-1692; C/keys.rs:445 | Spelling only (English, SCOWL). F7 jumps to the next error. Grammar is a non goal of the TDD. |
| Editor pane | common | OUT-OF-SCOPE | B§14 | Editor refinements and score come from a Microsoft service. |
| Red squiggle spelling, blue double underline grammar | core | PARTIAL | C/spelling.rs:13-67; S/spelling.rs:104-150 | Red wavy underline for spelling and repeated words. No grammar, so no blue underline. |
| Right click spelling menu | core | PARTIAL | C/spelling.rs:68-254; S/spelling.rs:151-268 | Up to 6 suggestions, Ignore All, Add to Dictionary, Delete Repeated Word. No AutoCorrect or Language entries. |
| Thesaurus | common | DONE | W/thesaurus.rs:20-210; C/context_menu.rs:112-130; S/commands.rs:935 | Offline WordNet 3.1: synonyms by part of speech, offered as Synonyms of the word in the right click menu, and the verb `thesaurus`. |
| Word Count | core | DONE | W/review.rs:349-376; W/session.rs:721; C/panel.rs:1877-1895; C/views.rs:340-352 | Pages, words, characters with and without spaces, paragraphs and lines, with a tick box to include text boxes, footnotes and endnotes. The status bar shows the selected words. |
| Read Aloud | common | OUT-OF-SCOPE | B§14 | Needs a speech service. |
| Check Accessibility | common | DONE | W/review.rs:442-535; C/panel.rs:1849-1858; S/commands.rs:953 | Alt text, heading order, table header rows and contrast. |
| Language: Set Proofing Language | core | PARTIAL | W/review.rs:377-440; C/panel.rs:1837-1848; W/spell.rs:117 | Sets `w:lang` on the selection. Spelling checks English only and skips a run in another language. |
| Language: Language Preferences | common | MISSING | W/spell.rs:50 | One dictionary. |
| New Comment | core | DONE | W/ops.rs:923-1018; C/overlays.rs:462-562; C/panel.rs:1699-1712; C/keys.rs:75 | Bubble, Review level, verb, and Ctrl+Alt+M. |
| Reply, Resolve, Reopen | common | DONE | W/ops.rs:1084-1113,1219-1235,1745-1830 | `commentsExtended` is updated. |
| Delete Comment | core | DONE | W/ops.rs:1236-1270,1150-1183; C/panel.rs:1730 | Delete one, or Delete all. |
| Previous and Next comment | core | DONE | W/ops.rs:1184-1218; C/panel.rs:1722-1730; S/commands.rs:1943 | Buttons and `comment next` and `comment previous`. |
| Show Comments | common | PARTIAL | C/page_view.rs:226-310; C/panel.rs:1804 | Cards in a margin beside the page, with a Show comments tick box. No Simple mode. |
| Comments pane | common | PARTIAL | C/panel.rs:1741-1790 | The Review level lists each comment with edit, reply, resolve and delete; no reviewer filter. |
| Track Changes On/Off | core | DONE | W/session.rs:662; C/panel.rs:1693-1698; C/keys.rs:76 | Writes `trackRevisions`. Ctrl+Shift+E. Off is refused while tracking is locked. |
| Display for Review | core | DONE | W/review.rs:16-50; W/layout/paragraph.rs:100-110,498,1417; C/panel.rs:1789-1800 | Simple Markup, All Markup, No Markup and Original change what is drawn. Verb `track view`. |
| Show Markup | common | PARTIAL | C/panel.rs:1789-1810 | Comments can be hidden and the four review views chosen. There is no list of markup kinds to show. |
| Balloons | common | MISSING | W/layout/paragraph.rs:100-110 | Revisions are shown inline; comments are in the margin. |
| Reviewing Pane | common | PARTIAL | W/review.rs:128-180; S/commands.rs:844-910 | `track list` lists the changes and Next and Previous change step through them. The window has no pane that lists them. |
| Accept and Reject | core | DONE | W/edit.rs:805-975; W/session.rs:629; S/actions.rs:1043-1073 | One or all, `rPrChange` and `pPrChange` included. |
| Next and Previous change | core | DONE | W/review.rs:128-180; C/panel.rs:1806-1814 | Buttons and `track next` and `track previous`. |
| Track formatting | common | DONE | W/review.rs:73-127; W/session.rs:452; W/review.rs:763 | Formatting under tracking is recorded as `rPrChange` and `pPrChange`, and rejecting puts it back. |
| Change User Name | common | DONE | C/settings_page.rs:9-40 | The author setting is written on comments and revisions. |
| Document Inspector | common | DONE | W/review.rs:536-620; C/panel.rs:1860-1875 | See section 1. |
| Review tab shortcuts | core | DONE | C/keys.rs:75-76,77 | Ctrl+Alt+M comments and Ctrl+Shift+E tracks changes. |
| Track moves | niche | PARTIAL | W/edit.rs:96,706,846 | `moveFrom` and `moveTo` display like delete and insert and are accepted or rejected with them. |
| Restrict Editing: Formatting restrictions, Editing restrictions, Exceptions, Start Enforcing Protection (4 rows) | niche | PRESERVED | A§2 | `documentProtection` and `permStart` kept, not enforced. |
| Language: Set as default; Lock Tracking; Track Changes Options; Compare; Combine; Show Source Documents (6 rows) | niche | MISSING | PRD 192 | Compare and combine are out of scope. |
| Translate; @mentions; Hide Ink; Smart Lookup and Researcher; Block Authors (5 rows) | niche | OUT-OF-SCOPE | B§14 | Online translation, notifications to people, ink, web research, shared file author blocking. |

### 15. View

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Read Mode | common | DONE | W/layout/mod.rs:818-835; S/actions.rs:1947; C/views.rs:303-380 | Pages laid out for reading, set from the status bar, the Document level or `view read`. |
| Print Layout | core | DONE | C/page_view.rs:110-195; S/actions.rs:1958 | The default view. |
| Outline view | common | DONE | C/outline.rs:25-242; S/actions.rs:1969,2069-2107; S/commands.rs:553-580 | Headings by level with promote, demote, move up and down, expand and collapse, and a level selector. Verb `outline`. |
| Ruler | core | DONE | C/views.rs:78-300; W/ruler.rs:29-160; S/actions.rs:1937 | Indents and tab stops that can be dragged, and a tab selector. |
| Gridlines | common | DONE | C/page_view.rs:698-725; S/actions.rs:2009 | Draws the drawing grid across the text column. |
| Navigation Pane | core | PARTIAL | C/navigation.rs:10-60; C/keys.rs:77; S/actions.rs:1139 | Headings, Pages and Results tabs, and Ctrl+Shift+O. Headings cannot be dragged to reorder; the Outline view moves them. |
| Zoom dialog | core | DONE | C/panel.rs:1942-1975; S/actions.rs:2059; C/page_view.rs:740-766 | A percent field and presets in the Document level, the zoom slider, Ctrl+wheel, 10% to 500%, and Page width, One page and Multiple pages. |
| One Page, Multiple Pages, Page Width | core | DONE | S/actions.rs:1979,1989,1999; S/commands.rs:2215-2260 | Fit width, fit page and multiple pages. |
| Zoom slider | core | DONE | C/views.rs:340-352 | A logarithmic slider and the percentage in the status bar. |
| New Window | common | DONE | S/actions.rs:2049; S/mod.rs:166-212 | Opens a second view of the same document. |
| Switch Windows | common | DONE | C/mod.rs:36-135 | Editor tabs. |
| Show: Ruler, Gridlines, Navigation Pane | core | DONE | S/actions.rs:1139,1937,2009; C/panel.rs:1926-1935 | Tick boxes and actions for all three. |
| Dark mode | common | DONE | C/page_view.rs:667-696; S/commands.rs:1091-1137 | `view paper theme` draws the page in the theme's ground; `paper light` draws it white as Word does. |
| Page thumbnails | core | DONE | C/navigation.rs:64-182; S/commands.rs:1091-1137 | The Pages tab shows a small picture of each page, and a press goes to it. |
| Web Layout; Outline tab: Master Document; Draft view; Focus; Page Movement; Arrange All; Split; View Side by Side; Macros; Properties pane (10 rows) | niche | MISSING | PRD 202 | Not found. |
| Immersive Reader; Learning Tools (2 rows) | niche | OUT-OF-SCOPE | B§15 | Online reading service features (read aloud, translate). |

### 16. Insert, other

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Cover Page | common | DONE | W/shortcuts.rs:326-372; S/actions.rs:2142 | Inserts at the top of the document a Title, a Subtitle, an author line and the date, then a page break. One design, with placeholder words to type over. |
| Blank Page | common | DONE | S/actions.rs:2131; C/panel.rs:943-944 | The action inserts two page breaks. |
| Page Break | core | DONE | C/keys.rs:373-377; S/actions.rs:710 | Ctrl+Enter and the action. |
| Quick Parts: AutoText | common | MISSING | C/panel.rs:833-1100 | No building blocks. |
| Quick Parts: Document Property | common | DONE | C/panel.rs:967-984; W/fields.rs:239-272 | Title, Author, Subject, Keywords, File name, Pages and Words fields. |
| Quick Parts: Field | common | DONE | C/panel.rs:1040-1066; W/ops.rs:1361; C/keys.rs:331-336 | A field box in the Insert level (Ctrl+F9 opens it) and `insert field <code>`; the result is worked out on insert. |
| Date & Time | common | DONE | S/actions.rs:780; C/panel.rs:945-966 | See section 10. |
| Equation | common | PARTIAL | W/math.rs:1-80; W/layout/paragraph.rs:401-440,589; WT/fixtures/equations.docx | An existing equation is drawn as its linear text in Cambria Math and written back as a paragraph level element, which the schema allows. None can be made. Editing and layout are left for a later round (TDD section 10). |
| Equation tools | common | MISSING | W/math.rs:1-10 | Editing equations is left for a later round (TDD section 10). |
| Symbol | core | DONE | C/panel_setup.rs:386-483; S/actions.rs:2159 | See section 3. |
| Emoji | common | NOT-DETERMINED | C/keys.rs:184-185 | See section 3. |
| Horizontal Line | common | DONE | S/actions.rs:760; C/panel.rs:521-540 | Also `---` then Enter. |
| Link (Hyperlink) | core | PARTIAL | W/ops.rs:1271; W/references.rs:560-690 | See section 11. |
| Bookmark, Cross-reference | core | PARTIAL | W/ops.rs:1314; W/references.rs:487-559 | Both insert. The bookmark list has no delete. |
| Comment | core | DONE | W/ops.rs:923; C/keys.rs:75 | Ctrl+Alt+M. |
| Header, Footer, Page Number | core | PARTIAL | W/ops.rs:638; W/page_setup.rs:307 | See section 10. |
| Text Box, WordArt, Drop Cap | common | PARTIAL | W/pictures.rs:309-329; W/layout/drawing.rs:386-536; W/layout/mod.rs:953-1038 | A text box can be made and edited. A drop cap is drawn as a text frame and a WordArt shape as its cached picture; neither can be made. |
| Table, Pictures, Shapes, Icons, 3D Models, SmartArt, Chart, Screenshot | core | PARTIAL | W/ops.rs:221; W/media.rs:28; W/pictures.rs:330-346 | Table, picture, ten shapes and text boxes can be inserted. Icons, 3D models, SmartArt, charts and screenshots cannot; authoring SmartArt and charts is a non goal of the TDD. |
| Building Blocks Organizer; Building block galleries; Object (3 rows) | niche | PRESERVED | W/package.rs | Glossary part and `w:object` kept. |
| Signature Line; Text from File; LaTeX input; UnicodeMath input; Math AutoCorrect; Dictionary of recent Insert items (6 rows) | niche | MISSING | A§4b | Not found. |
| Ink Equation; Online Video; Add-ins: Get Add-ins, My Add-ins; Wikipedia and Bing images add-ins (4 rows) | niche | OUT-OF-SCOPE | B§16 | Ink, online video and the Office add in store. |

### 17. Forms and controls

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Rich Text Content Control | common | PRESERVED | W/read.rs:202-210,351-356; W/write.rs:242-244; W/session.rs:184-193 | Block content is laid out and editable as text, and a Restrict Editing to forms lets a person type in it. No control behaviour or boundary, and none can be made. |
| Plain Text Content Control | common | PRESERVED | W/read.rs:202-210; W/write.rs:368-370 | An inline `w:sdt` is a wrapper on its runs; same limits. |
| Check Box Content Control | common | PRESERVED | W/read.rs:202-210 | Shows the stored glyph; cannot be toggled. |
| Combo Box Content Control | common | PRESERVED | W/read.rs:202-210 | Shows the stored text. |
| Drop-Down List Content Control | common | PRESERVED | W/read.rs:202-210 | Shows the stored text. |
| Date Picker Content Control | common | PRESERVED | W/read.rs:202-210 | Shows the stored text. |
| Content control properties | common | MISSING | W/model.rs:219,288 | Title, tag and lock are kept in `sdtPr` and not shown or enforced. |
| Edit Placeholder Text | common | PRESERVED | W/write.rs:242-244,368-370 | `placeholder` and `showingPlcHdr` are kept. |
| Picture Content Control; Building Block Gallery Content Control; Repeating Section Content Control; Group; Legacy Tools: Text, Check Box, Drop-Down form fields, Form field protection, Insert Frame and Reset; ActiveX Controls; Control properties window; XML Mapping; .docm storage (13 rows) | niche | PRESERVED | W/write.rs:16-39 | Kept in the XML or as parts. |
| Developer tab; Design Mode; Map Pane; Document Template, Add-ins; Macros dialog; Record Macro; Visual Basic Editor; Macro Security; Auto macros; Document events; Word object model (11 rows) | niche | MISSING | PRD 210 | Macros never run (MANIFEST:13). |

### 18. Proofing and automation

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| AutoCorrect: Replace text as you type | core | DONE | W/autocorrect.rs:57-66,95-200; C/panel.rs:1609-1618 | Word's English list of symbols and common misspellings, matched on the whole word. |
| AutoCorrect: Correct TWo INitial CApitals | common | DONE | W/autocorrect.rs:57-66,200-335 | A tick box in the Proofing level and `autocorrect two-initials`. |
| AutoCorrect: Capitalize first letter of sentences | core | DONE | W/autocorrect.rs:200-335 | A tick box in the Proofing level. |
| AutoCorrect: Capitalize first letter of table cells | common | DONE | W/autocorrect.rs:57-66,200-335 | A tick box in the Proofing level. |
| AutoCorrect: Capitalize names of days | common | DONE | W/autocorrect.rs:57-66,200-335 | A tick box in the Proofing level. |
| AutoCorrect: Correct accidental usage of cAPS LOCK key | common | MISSING | W/autocorrect.rs:57-66 | Not one of the rules. |
| Automatically use suggestions from the spelling checker | common | MISSING | W/autocorrect.rs:57-66 | Not one of the rules. |
| AutoFormat As You Type: Straight quotes with smart quotes | core | DONE | W/autocorrect.rs:57-66,200-335 | Single and double quotes follow the text before them. Ctrl+Z straight after gives back what was typed. |
| AutoFormat As You Type: Ordinals (1st) with superscript | common | DONE | W/autocorrect.rs:341-358 | 1st, 2nd, 3rd and 4th get a superscript suffix. |
| AutoFormat As You Type: Fractions (1/2) with fraction character | common | DONE | W/autocorrect.rs:57-66,200-335 | 1/2, 1/4 and 3/4 become the fraction characters. |
| AutoFormat As You Type: Hyphens (--) with dash | common | DONE | W/autocorrect.rs:359-405 | `--` and `---` become an en or em dash. |
| AutoFormat As You Type: Bold and italic with real formatting | common | MISSING | W/autocorrect.rs:57-66 | `*bold*` and `_italic_` are not converted. |
| AutoFormat As You Type: Internet and network paths with hyperlinks | core | DONE | W/autocorrect.rs:406-417 | A typed address becomes a hyperlink when the word ends. |
| AutoFormat As You Type: Automatic bulleted lists | core | PARTIAL | C/keys.rs:287-313 | On the space after `- ` or `* `; Word's trigger is Enter. |
| AutoFormat As You Type: Automatic numbered lists | core | PARTIAL | C/keys.rs:287-313 | Only `1. ` and `1) ` on the space. |
| AutoFormat As You Type: Border lines | common | DONE | W/autocorrect.rs:418-440,521-556; C/keys.rs:392-410 | `---`, `___`, `===`, `***`, `~~~` and `###` then Enter make a border line. |
| AutoText | common | MISSING | C/panel.rs:833-1100 | No building blocks. |
| Custom dictionaries | common | PARTIAL | S/spelling.rs:43-125; W/spell.rs:61-70 | `words.dic` and Office `CUSTOM.DIC` are read; Add to Dictionary on the card. No list editor. |
| Grammar settings | common | MISSING | W/spell.rs:1-60 | No grammar; a non goal of the TDD. |
| Proofing options | common | PARTIAL | W/spell.rs:90-186; C/panel.rs:1609-1618 | Ignoring capitals, words with digits and addresses, and flagging repeated words, are fixed; the autocorrect rules are tick boxes. |
| Check spelling as you type, Mark grammar errors as you type | core | PARTIAL | C/panel.rs:1675-1682; S/actions.rs:1101 | A spelling tick box; no grammar. |
| Recheck Document | common | MISSING | W/spell.rs:61-75 | Ignore All cannot be reset. |
| AutoCorrect Exceptions; Math AutoCorrect; AutoFormat tables; Built in Heading styles; Format beginning of list item; Set left and first indent with tabs and backspaces; Define styles based on your formatting; AutoFormat (batch); Exclusion dictionary; Readability statistics; Autocomplete suggestions (11 rows) | niche | MISSING | A§8 | Not found. |
| Dictate; Transcribe; Editor (AI); Copilot in Word; Sensitivity labels; Smart Lookup (6 rows) | niche | OUT-OF-SCOPE | B§18 | Speech, AI and Microsoft Purview or web services. |

### 20. Accessibility and navigation

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Screen reader support | core | NOT-DETERMINED | C/page_view.rs:296 | The window's controls carry names for the accessibility tree. Whether the page's text is exposed to a screen reader was not read. |
| Accessibility Checker | common | DONE | W/review.rs:442-535; C/panel.rs:1849-1858 | Alt text, heading order, table header rows and contrast. |
| Alt text on pictures, shapes, charts, tables, SmartArt | core | PARTIAL | W/pictures.rs:188-203; C/panel.rs:1494-1523 | Alt text on a picture, shape or text box is set in the Picture level, the right click menu and `picture alt`, and a picture can be marked decorative. Tables, charts and SmartArt keep theirs and nothing edits it. |
| Heading structure | core | DONE | S/actions.rs:514-535; C/navigation.rs:10-60; C/keys.rs:22-24 | Real Heading styles, Ctrl+Alt+1 to 3, and a headings pane. |
| Table header rows | core | DONE | W/ops.rs:393; C/panel.rs:1565 | `tblHeader` toggle, and the checker reports a table without one. |
| Hyperlink text | core | PARTIAL | W/references.rs:560-690; S/commands.rs:227-256 | Link text is editable and a screen tip is set by `link tip`; the window has no screen tip field. |
| Color contrast | common | DONE | W/review.rs:706-726; W/review.rs:442-535 | The checker reports text colours below the contrast ratio. |
| Language tagging | common | DONE | W/review.rs:377-440; C/panel.rs:1837-1848 | `w:lang` is set on the selection. |
| Keyboard navigation | core | PARTIAL | C/keys.rs:314-481 | Text chords, Shift+F10 for the context menu and Ctrl+/ for any command. No F6 between panes and no KeyTips. |
| Access keys | common | MISSING | MANIFEST:20 | No ribbon, so no access keys. |
| High contrast | common | NOT-DETERMINED | C/page_view.rs:667-696 | The window theme handling was not read. |
| Read Aloud | common | OUT-OF-SCOPE | B§20 | Needs a speech service. |
| Tagged PDF export | common | DONE | W/pdf.rs:50-135,859 | See section 1. |
| Navigation pane for headings | core | DONE | C/navigation.rs:10-60 | Headings, indented by level, each one a link to its place. |
| Zoom and magnifier | core | DONE | C/page_view.rs:740-766; C/views.rs:340-352 | Ctrl+wheel and the slider, to 500%. |
| Learning Tools; Text spacing; Accessible templates; Accessibility status bar indicator (4 rows) | niche | MISSING | A§6c | Not found. |
| Immersive Reader; Alt text auto generation (2 rows) | niche | OUT-OF-SCOPE | B§20 | Online services. |

### 22. Layout fidelity

| Feature | Tier | Status | Evidence | Note |
|---|---|---|---|---|
| Line breaking | core | DONE | WT/pagination.rs:191-225; S/tests.rs:51-89; W/layout/paragraph.rs:1070-1237 | Every face's lines match Word's to the word, Aptos included, in the test with the machine's own fonts. The window's own copy of the test skips Aptos. Two programs can measure a font a fraction of a point apart. |
| Break opportunities | core | PARTIAL | W/layout/paragraph.rs:389-396,737; W/layout/tests/hyphenation.rs | Spaces, hyphens, dashes, slash before alphanumerics, ideographs, zero width space and hyphenation points. No kinsoku rules. |
| Nonbreaking | core | DONE | W/layout/paragraph.rs:389,480 | U+00A0 and `w:noBreakHyphen` are not break points. |
| Justification | core | DONE | W/layout/paragraph.rs:1237-1522 | Spaces shrink as well as grow; the last line and forced break lines are not stretched. |
| Hyphenation | common | DONE | W/layout/paragraph.rs:928-1070; W/layout/tests/hyphenation.rs | Pattern based, when the document or section turns it on. |
| Tabs | core | DONE | W/layout/paragraph.rs:994-1026; W/layout/tests/ptab.rs | Custom, default, hanging implicit stop, leaders and absolute position tabs. A bar tab is skipped. |
| Line height | core | DONE | W/props.rs:272-281; W/layout/paragraph.rs:307-321 | Auto, atLeast and exact; the spacing below the last line may hang into the bottom margin. |
| Spacing collapse | core | DONE | W/layout/mod.rs:1686-1700; W/layout/tests/spacing.rs; WT/fixtures/spacing-sum.docx | The larger of the space after and before, or their sum for a file whose compatibility settings say so. |
| Widow and orphan control | core | DONE | W/layout/mod.rs:1700-1716 | Two lines at the foot and the head of a page. |
| Keep with next, keep lines together | core | DONE | W/layout/mod.rs:1737-1830; W/layout/tests/keep_next.rs | Keep with next moves a chain of paragraphs together; keep lines moves the whole paragraph. |
| Page break before | core | DONE | W/layout/mod.rs:1772-1780 | The paragraph starts a page. |
| Floating object wrapping | core | PARTIAL | W/layout/mod.rs:1432-1560; W/layout/tests/wrapping.rs | Square, tight (by the polygon's bounds), top and bottom, behind and in front, with sides and distances. `allowOverlap` and `layoutInCell` are not used. |
| Anchored object position limits | common | MISSING | W/layout/mod.rs:1981-2010 | An object is not kept inside the page or the margins. |
| Table row breaking | core | DONE | W/layout/table.rs:57-140,373; W/layout/mod.rs:1879-1887; W/layout/tests/row_splitting.rs | Rows split between lines unless `cantSplit`; header rows repeat; widow lines apply inside a split row. |
| Table width resolution | core | DONE | W/layout/table.rs:453-575; W/layout/tests/table_widths.rs | The grid, `tcW` and content, with `tblW` in points, percent or auto. |
| Cell margins and spacing | common | PARTIAL | W/layout/table.rs:182-190,608-629 | Table and cell margins; `tblCellSpacing` is not read. |
| Footnote placement | core | DONE | W/layout/mod.rs:1575-1640,2296-2352; W/layout/tests/footnote_continuation.rs | At the foot of the reference page, continued on the next page under a continuation separator when it does not fit. |
| Endnotes | common | DONE | W/layout/mod.rs:688,1888-1920; W/layout/notes.rs:16-65 | At the end of the document or the end of each section. |
| Balanced columns | common | DONE | W/layout/mod.rs:1301-1341; W/layout/tests/column_balance.rs | Columns balance before a continuous section break. |
| Column breaks and widths | core | DONE | W/layout/section.rs:161-186; W/layout/mod.rs:1342-1370 | Explicit widths, column breaks and the line between columns are laid out and drawn. |
| Section breaks | core | DONE | W/layout/mod.rs:836-868,1242-1300 | Next page, continuous, even, odd, next column. |
| Kerning | common | DONE | W/kern.rs:30-138; W/layout/paragraph.rs:378-390 | GPOS pair kerning when `w:kern` asks. |
| Character spacing | common | DONE | W/props.rs:557-565; W/layout/paragraph.rs:1522-1566 | Spacing, scale and position; `fitText` is not applied. |
| RTL and bidi | common | PRESERVED | W/props.rs:380,461 | Parsed, never used; no shaping (MANIFEST:20). |
| Drop caps | common | PARTIAL | W/layout/mod.rs:953-1038 | A `framePr` paragraph is laid out as a text frame with text going round it. |
| Hyphenation settings | common | DONE | W/page_setup.rs:259; W/layout/paragraph.rs:928-962 | `autoHyphenation` and `suppressAutoHyphens` are used. |
| Field result layout | core | DONE | W/layout/mod.rs:721-748; W/layout/fields.rs:21-45 | Page fields are worked out in the body, headers and footers; a table of contents takes its page numbers from layout when updated. |
| Header and footer fit | core | DONE | W/layout/mod.rs:2011-2034; W/layout/tests/header_push.rs | A header taller than the margin pushes the body down, and a tall footer pulls its foot up. |
| Section vertical alignment | common | DONE | W/layout/mod.rs:2190-2218; W/layout/section.rs:109-110 | Top, center, justified and bottom. |
| Text frames and text boxes | common | DONE | W/layout/drawing.rs:386-536; W/layout/mod.rs:953-1038 | Text boxes draw with their text laid out inside, linked boxes continue, and frames lay out as floats. |
| Font metrics and substitution | core | PARTIAL | S/fonts.rs:26-123 | Installed fonts and Office cloud fonts are used. `fontTable` substitution is not and embedded fonts are not used. |
| Justification of character spacing; Compatibility settings; Compatibility mode by version; OpenType features; East Asian typography; Vertical text in table cells; Line numbering; Printer metrics; Pagination of floating tables (9 rows) | niche | PRESERVED | W/layout/mod.rs:371-379 | `w:compat` and the rest are kept and not read, so one set of layout rules applies to every file. |
| Empty paragraph and page break semantics | niche | NOT-DETERMINED | W/read.rs:337 | `splitPgBreakAndParaMark` is not read; effect not tested. |

### 21. Keyboard shortcuts

Only the shortcuts of B§21 that Unluminous lacks or binds differently, core and common. B gives no tier for shortcuts; the tier here is by how often a person presses the chord. Unluminous's bindings are the `CHORDS` table in `C/keys.rs:21-100`, which a person can move with the keymap; the window keeps Ctrl+S, W, O, N, Tab and Comma (`C/keys.rs:151-153`). After task-2359 most of the chords below are bound, and their rows say so with the word Implemented. Chords Unluminous binds the same way as Word (Ctrl+B, I, U, E, L, R, J, 1, 2, 5, M, Shift+M, Z, Y, A, H, K, Space, Shift+N, Shift+8, Shift+C, Enter, Shift+Enter, Shift+Ctrl+Enter, Ctrl+arrows, Home, End, Tab and Shift+Tab in tables) are not listed.

| Word chord | Word action | Tier | Unluminous binding |
|---|---|---|---|
| Ctrl+Shift+V | Paste text only | core | Paste formatting (`C/keys.rs:70`). Holding Shift on a paste event pastes the text alone (`C/keys.rs:198`). Still differs from Word |
| Ctrl+P | Print | core | Implemented: Print (`C/keys.rs:99`) |
| Ctrl+G | Go To | common | Implemented: opens the Go To box (`C/keys.rs:337`) |
| Ctrl+Alt+M | Insert comment | core | Implemented: Comment (`C/keys.rs:75`) |
| Ctrl+Shift+E | Track changes on or off | core | Implemented: Track Changes (`C/keys.rs:76`) |
| Ctrl+Alt+1, 2, 3 | Heading 1, 2, 3 | core | Implemented: Heading 1, 2 and 3 (`C/keys.rs:22-24`); the chord table is read before the line spacing arms |
| Ctrl+Alt+F | Insert footnote | core | Implemented: Footnote (`C/keys.rs:79`) |
| Ctrl+Alt+D | Insert endnote | common | Implemented: Endnote (`C/keys.rs:80`) |
| Ctrl+Shift+L | Bulleted list | core | Implemented: applies the List Bullet style (`C/keys.rs:61`) |
| Ctrl+Shift+A | All caps | common | Implemented: All caps (`C/keys.rs:36`) |
| Ctrl+Shift+K | Small caps | common | Implemented: Small caps (`C/keys.rs:35`) |
| Ctrl+Shift+H | Hidden text | common | Implemented: Hidden text (`C/keys.rs:37`) |
| Ctrl+Shift+D | Double underline | common | Implemented: Double underline (`C/keys.rs:33`) |
| Ctrl+Shift+W | Underline words only | common | Implemented: Underline words only (`C/keys.rs:34`) |
| Shift+F3 | Toggle case | core | Implemented: steps through the cases (`C/keys.rs:39`) |
| Ctrl+D or Ctrl+Shift+F | Font dialog | core | Implemented: opens the Text level of the panel, not a dialog (`C/keys.rs:71-72`) |
| Ctrl+] and Ctrl+[ | Grow and shrink by 1 pt | common | Implemented: grow and shrink by 1 pt (`C/keys.rs:64-65`) |
| Ctrl+Q | Remove paragraph formatting | common | Implemented: Reset paragraph formatting (`C/keys.rs:52`), claimed before the window's quit chord while a Word tab has the keyboard |
| Ctrl+T and Ctrl+Shift+T | Hanging indent, remove hanging indent | common | Implemented: hanging indent and remove hanging indent (`C/keys.rs:50-51`) |
| Ctrl+Shift+Space | Nonbreaking space | common | Implemented: nonbreaking space (`C/keys.rs:90`) |
| Ctrl+- | Optional hyphen | common | Implemented: optional hyphen (`C/keys.rs:92`) |
| Ctrl+Shift+- | Nonbreaking hyphen | common | Implemented: nonbreaking hyphen (`C/keys.rs:91`) |
| Ctrl++ , Ctrl+- , Ctrl+0 | Zoom in, zoom out, 100% | common | Not bound as zoom. Ctrl+- is the optional hyphen and Ctrl+0 adds or removes space before, as in Word; zoom is Ctrl+wheel, the slider or the panel |
| F4 | Repeat last action | common | Implemented: repeats the last command (`C/keys.rs:446-450`) |
| Ctrl+Page Up and Ctrl+Page Down | Previous and next page | common | Not bound |
| Ctrl+F | Navigation pane | common | Opens the find bar (`C/keys.rs:57`); the Navigation pane has a Results tab but Ctrl+F does not open it |
| Ctrl+Shift+S and Ctrl+Alt+Shift+S | Apply Styles, Styles pane | common | Ctrl+Alt+Shift+S implemented: opens the Styles level (`C/keys.rs:74`). Ctrl+Shift+S not bound |
| Alt+Shift+D, Alt+Shift+T, Alt+Shift+P | Insert DATE, TIME, PAGE field | common | Not bound |
| Shift+F9 and Alt+F9 | Toggle field code | common | Implemented: show field codes (`C/keys.rs:452`) |
| Ctrl+Shift+F9 | Unlink field | common | Implemented: unlink fields (`C/keys.rs:25`) |
| F9 | Update selected fields | core | Implemented: updates the fields in the selection (`C/keys.rs:454`); Update Fields in the panel does the whole document |
| Alt+Home, Alt+End, Alt+Page Up, Alt+Page Down | First and last cell of row and column | common | Not bound |
| Shift+arrows across cells | Extend selection to adjacent cells | common | Implemented: a selection that spans cells selects the block of cells (`W/tables.rs:106-141`) |
| Shift+F10 and F6 | Context menu, move among panes | common | Shift+F10 implemented: opens the right click menu at the caret (`C/keys.rs:455`). F6 not bound |
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

The counts below are after task-2359 (round 2). Before it, the core tier was 231 rows: 81 DONE, 104 PARTIAL, 7 PRESERVED, 37 MISSING and 2 NOT-DETERMINED; the common tier was 302 rows: 22 DONE, 69 PARTIAL, 73 PRESERVED, 128 MISSING, 6 NOT-DETERMINED and 4 OUT-OF-SCOPE. The niche tier was not touched.

Counts are rows of B (a grouped row counts as its N rows). Categories 19 and 21 are not counted: 19 is skipped by the brief and 21 has no tiers in B. ND = NOT-DETERMINED, OOS = OUT-OF-SCOPE. Category 23 is counted in Part 2.

| Category | core DONE | core PARTIAL | core PRESERVED | core MISSING | core ND | core OOS | common DONE | common PARTIAL | common PRESERVED | common MISSING | common ND | common OOS |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1. File | 8 | 6 | 0 | 1 | 0 | 0 | 9 | 13 | 0 | 4 | 0 | 1 |
| 2. Edit and selection | 10 | 3 | 0 | 0 | 0 | 0 | 6 | 7 | 0 | 6 | 0 | 0 |
| 3. Fonts and character formatting | 11 | 4 | 0 | 0 | 0 | 0 | 12 | 1 | 4 | 0 | 1 | 0 |
| 4. Paragraph | 15 | 1 | 0 | 0 | 0 | 0 | 9 | 6 | 2 | 0 | 0 | 0 |
| 5. Styles and themes | 8 | 3 | 0 | 0 | 0 | 0 | 4 | 6 | 1 | 2 | 0 | 0 |
| 6. Lists | 6 | 4 | 0 | 0 | 0 | 0 | 3 | 5 | 1 | 0 | 0 | 0 |
| 7. Tables | 12 | 9 | 0 | 2 | 0 | 0 | 12 | 6 | 3 | 3 | 0 | 0 |
| 8. Illustrations | 9 | 3 | 0 | 0 | 0 | 0 | 5 | 11 | 4 | 10 | 0 | 0 |
| 9. Page layout and sections | 12 | 4 | 0 | 0 | 0 | 0 | 9 | 4 | 1 | 0 | 0 | 0 |
| 10. Headers, footers and page numbers | 7 | 4 | 0 | 0 | 0 | 0 | 6 | 2 | 1 | 1 | 0 | 0 |
| 11. References | 4 | 5 | 0 | 0 | 0 | 0 | 6 | 1 | 2 | 3 | 0 | 0 |
| 12. Fields | 12 | 1 | 1 | 0 | 0 | 0 | 16 | 0 | 6 | 0 | 0 | 0 |
| 13. Mailings | 0 | 1 | 0 | 3 | 0 | 0 | 0 | 0 | 1 | 13 | 0 | 0 |
| 14. Review | 9 | 4 | 0 | 0 | 0 | 0 | 6 | 4 | 0 | 2 | 0 | 2 |
| 15. View | 7 | 1 | 0 | 0 | 0 | 0 | 6 | 0 | 0 | 0 | 0 | 0 |
| 16. Insert, other | 3 | 4 | 0 | 0 | 0 | 0 | 6 | 2 | 0 | 2 | 1 | 0 |
| 17. Forms and controls | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 7 | 1 | 0 | 0 |
| 18. Proofing and automation | 4 | 3 | 0 | 0 | 0 | 0 | 7 | 2 | 0 | 6 | 0 | 0 |
| 20. Accessibility and navigation | 4 | 3 | 0 | 0 | 1 | 0 | 4 | 0 | 0 | 1 | 1 | 1 |
| 22. Layout fidelity | 16 | 3 | 0 | 0 | 0 | 0 | 8 | 2 | 1 | 1 | 0 | 0 |
| Total | 157 | 66 | 1 | 6 | 1 | 0 | 134 | 72 | 34 | 55 | 3 | 4 |

| Tier | Rows | DONE | PARTIAL | PRESERVED | MISSING | ND | OOS |
|---|---|---|---|---|---|---|---|
| core | 231 | 157 (68%) | 66 (29%) | 1 (0%) | 6 (3%) | 1 | 0 |
| common | 302 | 134 (44%) | 72 (24%) | 34 (11%) | 55 (18%) | 3 | 4 |
| niche | 300 | 3 | 12 | 148 | 109 | 1 | 27 |
| all | 833 | 294 | 150 | 183 | 170 | 5 | 31 |

Reading the totals: 72 of the 231 core rows (31%) are PARTIAL or MISSING, down from 141. 34 common rows are PRESERVED, down from 73. They are kept in the file and not acted on: fields of the mail merge and citation features that are non goals of the TDD, content controls, and picture, shape and chart properties. The 148 niche rows that are PRESERVED are kept in the file and not acted on.

Category 23 (Part 2) was not re-read for this update. Its DROPPED rows for equations and `w:subDoc` and the header text replacement (D2 in Part 4) describe the code at commit 5b719826; round 2 changed the code that handled them (an equation is now written back as a paragraph level element and a header keeps the rest of its story), so Part 2 and Part 4 are out of date on those points.

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
