# Microsoft Word for Windows feature inventory (Microsoft 365 desktop, Word 2024 level)

Purpose: an inventory of the features a word processor claiming full major feature parity with Word must have, and the WordprocessingML constructs that matter for round trip fidelity. A later agent audits an implementation against it.

## How to read this document

- Tier `core`: every serious word processor has it (LibreOffice Writer, Google Docs, OnlyOffice, Pages).
- Tier `common`: most desktop word processors have it.
- Tier `niche`: Word specific or rarely used.
- The Source column uses short keys. The key table below maps each key to a URL. Each category header also names its main sources.
- Method: the keyboard shortcut table (section 21) was taken from the Microsoft Support page fetched during this task. The other tables were written from knowledge of Word's ribbon, dialogs and the ECMA-376 schema, and the cited pages are the pages that document those areas. The individual URLs below the top level of support.microsoft.com were not each opened. A later agent should confirm any single row against the cited page before treating it as a hard requirement.
- Feature names are Word's own names so they can be matched against Word's UI.

## Source keys

| Key | URL |
|---|---|
| MS-WORD | https://support.microsoft.com/en-us/word |
| MS-KEYS | https://support.microsoft.com/en-us/office/keyboard-shortcuts-in-word-95ef89dd-7142-4b50-afb2-f762f663ceb2 |
| MS-FIELDS | https://support.microsoft.com/en-us/office/list-of-field-codes-in-word-1ad6d91a-55a7-4a8d-b535-cf7888635c6c |
| MS-ACCESS | https://support.microsoft.com/en-us/office/make-your-word-documents-accessible-to-people-with-disabilities-d9bf3683-87ac-47ea-b91a-78dcacb3c66d |
| MS-FORMATS | https://support.microsoft.com/en-us/office/file-formats-that-are-supported-in-word |
| MS-LEARN-WML | https://learn.microsoft.com/en-us/office/open-xml/word/overview |
| MS-LEARN-SDK | https://learn.microsoft.com/en-us/office/open-xml/word/working-with-wordprocessingml-documents |
| ECMA-376 | https://ecma-international.org/publications-and-standards/standards/ecma-376/ |
| MS-OI29500 | https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/ |
| MS-DOCX | https://learn.microsoft.com/en-us/openspecs/office_standards/ms-docx/ |
| MS-OE376 | https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/ |
| LO-WRITER | https://help.libreoffice.org/latest/en-US/text/swriter/main0000.html |
| LO-FEATURES | https://www.libreoffice.org/discover/writer/ |
| LO-WIKI | https://wiki.documentfoundation.org/Feature_Comparison:_LibreOffice_-_Microsoft_Office |
| OO-DOCS | https://www.onlyoffice.com/document-editor.aspx |
| GDOCS | https://support.google.com/docs |

The LibreOffice wiki (`wiki.documentfoundation.org`) returned a bot protection block when fetched. LO-WIKI is cited as a pointer only and was not read.

---

## 1. File

Main sources: MS-WORD, MS-FORMATS.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| New blank document | Creates an empty document from Normal.dotm | core | MS-WORD |
| New from template | Opens the Backstage New page with built in and online templates | core | MS-WORD |
| Normal.dotm | Global default template holding the Normal style, default fonts, AutoText and macros | common | MS-WORD |
| Templates .dotx | Macro free template format | common | MS-FORMATS |
| Templates .dotm | Macro enabled template format | niche | MS-FORMATS |
| Attach template | Links a document to a template and loads its styles and building blocks | niche | MS-WORD |
| Open .docx | Word 2007 and later Open XML document | core | MS-FORMATS |
| Open .docm | Macro enabled document | niche | MS-FORMATS |
| Open .doc | Word 97-2003 binary document | common | MS-FORMATS |
| Open .dot | Word 97-2003 template | niche | MS-FORMATS |
| Open .rtf | Rich Text Format | core | MS-FORMATS |
| Open .odt | OpenDocument Text | common | MS-FORMATS |
| Open .txt | Plain text with encoding choice (ANSI, UTF-8, UTF-16, others) | core | MS-FORMATS |
| Open .htm, .html, .mht | Web page and single file web page | common | MS-FORMATS |
| Open .xml | Word 2003 XML and Word XML Document (flat OPC) | niche | MS-FORMATS |
| Open .wps | Microsoft Works document | niche | MS-FORMATS |
| Open .wpd | WordPerfect document | niche | MS-FORMATS |
| Open PDF (PDF Reflow) | Converts a PDF into an editable Word document, layout approximate | common | MS-WORD |
| Open and Repair | Opens a damaged file with recovery | niche | MS-WORD |
| Protected View | Opens files from the internet or email read only until editing is enabled | common | MS-WORD |
| Recent documents and pinned files | List of recent files with pinning | core | MS-WORD |
| Save | Saves in the current format | core | MS-WORD |
| Save As | Saves under a new name, format or location | core | MS-WORD |
| Save as .docx, .docm, .dotx, .dotm, .doc, .rtf, .odt, .txt, .htm, .xml | Output formats | core | MS-FORMATS |
| Save as Strict Open XML Document | Saves the ISO 29500 Strict variant | niche | MS-FORMATS |
| Save as Plain Text options | Encoding, line endings, insert line breaks, allow character substitution | common | MS-FORMATS |
| Save as Web Page, Filtered | Saves HTML without Office specific markup | niche | MS-FORMATS |
| Export to PDF | Writes PDF with Options: page range, publish what (document or markup), create bookmarks from headings or bookmarks, document properties, document structure tags for accessibility, ISO 19005-1 (PDF/A) compliant, optimize for standard or minimum size, encrypt with password, bitmap text when fonts cannot be embedded | core | MS-WORD |
| Export to XPS | XPS output with the same option set | niche | MS-WORD |
| Tagged PDF | Structure tags carry headings, lists, tables and alt text for screen readers | common | MS-ACCESS |
| Change File Type | Backstage shortcut to convert format | common | MS-WORD |
| Print | Backstage print with printer, copies, collated, one sided or manual duplex, orientation, paper size, margins, pages per sheet | core | MS-WORD |
| Print ranges | Print all, selection, current page, custom range such as 1,3,5-12 and section syntax p1s2 | core | MS-WORD |
| Print what | Document, document properties, markup, list of markup, styles, AutoText entries, key assignments | niche | MS-WORD |
| Print only odd or even pages | Used for manual duplex | common | MS-WORD |
| Scale to paper size and pages per sheet | Shrinks multiple pages on a sheet or fits another paper size | common | MS-WORD |
| Print preview | Preview in Backstage with zoom and page navigation | core | MS-WORD |
| Print options | Print to file, background colors and images, drawings, update fields before printing, update links before printing, reverse print order | niche | MS-WORD |
| Booklet (Book fold) | Page Setup Multiple pages: Book fold, with sheets per booklet, prints 2 pages per sheet in booklet order | niche | MS-WORD |
| Mirror margins | Inside and outside margins for facing pages | common | MS-WORD |
| AutoSave | Continuous save to OneDrive or SharePoint when on | common | MS-WORD |
| AutoRecover | Timed recovery copy every N minutes (default 10), keep last autosaved version if closed without saving | common | MS-WORD |
| Document Recovery pane | Offers recovered files after a crash | common | MS-WORD |
| Always create backup copy | Keeps a .wbk previous copy on save | niche | MS-WORD |
| Version History | Lists earlier saved versions of cloud stored files, with open and compare | niche | MS-WORD |
| Manage Document | Recovers local unsaved versions | niche | MS-WORD |
| Info panel | Shows protect, inspect, versions, browser view options, properties, related dates, people and documents | common | MS-WORD |
| Properties: Summary | Title, subject, author, manager, company, category, keywords, comments, hyperlink base | common | MS-WORD |
| Properties: Statistics and Contents | Created, modified, last printed, revision number, total editing time, pages, words, characters, lines, paragraphs | common | MS-WORD |
| Properties: Custom | Named custom properties with type (text, date, number, yes or no) and link to content | niche | MS-WORD |
| Inspect Document | Finds comments, revisions, document properties, personal information, headers and footers, hidden text, custom XML, invisible content, embedded documents | common | MS-WORD |
| Remove personal information on save | Strips author and last modified by on save | niche | MS-WORD |
| Check Accessibility | Starts the accessibility checker (section 20) | common | MS-ACCESS |
| Check Compatibility | Lists features not supported by earlier Word versions | niche | MS-WORD |
| Encrypt with Password | Password required to open (AES 128 or 256 in current Word) | common | MS-WORD |
| Password to modify | General Options: password to modify, read only recommended | common | MS-WORD |
| Mark as Final | Flags the document as final and read only | niche | MS-WORD |
| Restrict Editing | Formatting restrictions, editing restrictions, start enforcement with password | common | MS-WORD |
| Restrict Access (IRM) | Information Rights Management permissions | niche | MS-WORD |
| Add a Digital Signature | Invisible signature stored in _xmlsignatures | niche | MS-WORD |
| Compatibility Mode | Opening a file with an older compat mode shows "[Compatibility Mode]" and disables newer features | niche | MS-OE376 |
| Convert to current mode | Updates a document to the latest compatibility mode | niche | MS-OE376 |
| Share | Share link or send a copy (section 19) | common | MS-WORD |
| Close and Exit | Closes the document or application, prompts to save | core | MS-WORD |
| Options dialog | General, Display, Proofing, Save, Language, Accessibility, Advanced, Customize Ribbon, Quick Access Toolbar, Add ins, Trust Center | core | MS-WORD |
| Customize ribbon and Quick Access Toolbar | User defined tabs, groups and commands | common | MS-WORD |
| Trust Center | Macro settings, Protected View, trusted locations, trusted publishers, add in security, ActiveX, Message Bar | niche | MS-WORD |
| Embed fonts in file | Saves TrueType fonts inside the document (odttf) with options to embed only characters in use and not embed common system fonts | niche | MS-WORD |
| Default file format and location | Save options for new documents | common | MS-WORD |
| Update links on open | Prompt for linked files | niche | MS-WORD |

---

## 2. Edit and selection

Main sources: MS-WORD, MS-KEYS.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Cut, Copy, Paste | Basic clipboard | core | MS-KEYS |
| Paste: Keep Source Formatting | Pastes with the source styles and direct formatting | core | MS-WORD |
| Paste: Merge Formatting | Matches destination text, keeps bold and italic | core | MS-WORD |
| Paste: Keep Text Only | Pastes plain text | core | MS-WORD |
| Paste: Use Destination Styles | Maps pasted styles to destination style definitions | common | MS-WORD |
| Paste: Picture, Link and Keep Source, Link and Merge | Alternative paste forms | common | MS-WORD |
| Paste Special dialog | Choices include Microsoft Word Document Object, Formatted Text (RTF), Unformatted Text, Picture (PNG, JPEG, GIF, Enhanced Metafile), HTML Format, Unformatted Unicode Text, Paste link | common | MS-WORD |
| Set Default Paste | Options for pasting within one document, between documents, between documents with style conflicts, from other programs, Show Paste Options button | niche | MS-WORD |
| Office Clipboard | Pane that holds up to 24 items, paste all, clear all | niche | MS-WORD |
| Drag and drop text | Moves text; Ctrl drag copies | core | MS-WORD |
| Smart cut and paste | Adjusts spaces and punctuation around cut and pasted words | common | MS-WORD |
| Smart paragraph selection | Includes the paragraph mark when selecting a whole paragraph | niche | MS-WORD |
| Use Ctrl+Click to follow hyperlink | Option controlling link activation | common | MS-WORD |
| Click and Type | Double click in an empty area to place text with automatic alignment and paragraph marks in Print Layout | niche | MS-WORD |
| Overtype mode | Insert key or option toggles overtype, status bar indicator | common | MS-WORD |
| Undo, Redo, Repeat | Multi level undo and redo with a list, F4 repeats the last action | core | MS-KEYS |
| Selection by mouse | Double click selects a word; triple click selects a paragraph; Ctrl+click selects a sentence; left margin click selects a line; left margin double click selects a paragraph; left margin triple click or Ctrl+click selects the document | core | MS-WORD |
| Selection by keyboard | Shift+Arrow, Ctrl+Shift+Arrow, Ctrl+A and others (section 21) | core | MS-KEYS |
| Extend mode F8 | F8 starts extend; repeated F8 selects word, sentence, paragraph, section, document; Shift+F8 reduces | niche | MS-KEYS |
| Column (block) selection | Alt+drag or Ctrl+Shift+F8 | common | MS-KEYS |
| Multiple non contiguous ranges | Ctrl+drag adds a range | common | MS-WORD |
| Select Text with Similar Formatting | Select menu command | common | MS-WORD |
| Find (Navigation pane) | Ctrl+F opens the pane with a results list and highlights | core | MS-WORD |
| Navigation pane tabs | Headings, Pages, Results | core | MS-WORD |
| Advanced Find | Find and Replace dialog, Find tab, More options | core | MS-WORD |
| Find options | Match case, Find whole words only, Use wildcards, Sounds like (English), Find all word forms (English), Match prefix, Match suffix, Ignore punctuation characters, Ignore white space characters | common | MS-WORD |
| Wildcards | `?` `*` `[abc]` `[a-z]` `[!abc]` `<` word start, `>` word end, `{n}` `{n,m}` `@` (one or more), `\` escape, `( )` grouping with `\1` in Replace | common | MS-WORD |
| Find Format | Find by font, paragraph, tabs, language, frame, style, highlight | common | MS-WORD |
| Find Special | `^p` paragraph mark, `^t` tab, `^?` any character, `^#` any digit, `^$` any letter, `^^` caret, `^n` column break, `^+` em dash, `^=` en dash, `^e` endnote mark, `^d` field, `^f` footnote mark, `^g` graphic, `^l` manual line break, `^m` manual page break, `^~` nonbreaking hyphen, `^s` nonbreaking space, `^-` optional hyphen, `^b` section break, `^w` white space, `^c` clipboard contents, `^&` find what text | common | MS-WORD |
| Replace and Replace All | Substitutes text and optionally formatting or special items | core | MS-WORD |
| Replace with formatting | Applies font, paragraph or style to matches | common | MS-WORD |
| Search direction | Up, Down, All | common | MS-WORD |
| Go To | Ctrl+G: Page, Section, Line, Bookmark, Comment, Footnote, Endnote, Field, Table, Graphic, Equation, Heading, Object; relative +n and -n | common | MS-KEYS |
| Browse Object | Browse by chosen object type | niche | MS-WORD |
| Spike | Collects cut items (Ctrl+F3), pastes with Ctrl+Shift+F3 | niche | MS-WORD |
| Copy and paste formatting | Ctrl+Shift+C and Ctrl+Shift+V | common | MS-KEYS |
| Format Painter | Single click for one use, double click for repeated use | core | MS-WORD |
| Status bar | Page, word count, language, track changes state, view buttons, zoom | common | MS-WORD |
| Mini toolbar | Floating toolbar on selection | common | MS-WORD |

---

## 3. Fonts and character formatting

Main sources: MS-WORD, MS-KEYS.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Font name | Chooses a face with preview list, recent fonts, theme fonts (+Headings, +Body) | core | MS-WORD |
| Font size | Points from 1 to 1638, half point steps, size list | core | MS-WORD |
| Grow Font, Shrink Font | Steps through the size list (Ctrl+Shift+> and <) | core | MS-KEYS |
| Increase and decrease 1 pt | Ctrl+] and Ctrl+[ | common | MS-KEYS |
| Font style | Regular, Italic, Bold, Bold Italic | core | MS-WORD |
| Font color | Theme colors, standard colors, More Colors (Standard, Custom RGB or HSL), Gradient | core | MS-WORD |
| Underline style | Single, Words only, Double, Thick, Dotted, Dashed, Dot-dash, Dot-dot-dash, Wave, and thick or heavy variants | core | MS-WORD |
| Underline color | Independent color for the underline | common | MS-WORD |
| Strikethrough | Single line through text | core | MS-WORD |
| Double strikethrough | Two lines through text | common | MS-WORD |
| Superscript | Raises and shrinks text (Ctrl+Shift+=) | core | MS-KEYS |
| Subscript | Lowers and shrinks text (Ctrl+=) | core | MS-KEYS |
| Small caps | Lowercase shown as small capitals | common | MS-WORD |
| All caps | Shows letters as capitals without changing stored case | common | MS-WORD |
| Hidden | Hides text unless formatting marks are on or the option to print hidden text is set | common | MS-WORD |
| Emboss, Engrave | Relief effects | niche | MS-WORD |
| Outline (font effect) | Outlines glyphs | niche | MS-WORD |
| Shadow (font effect) | Basic shadow | niche | MS-WORD |
| Equalize character height | Makes lowercase the same height as uppercase | niche | MS-WORD |
| Character spacing: Scale | Horizontal scaling percent | common | MS-WORD |
| Character spacing: Spacing | Expanded or Condensed by points | common | MS-WORD |
| Character spacing: Position | Raised or Lowered by points | common | MS-WORD |
| Kerning for fonts | Pair kerning at or above a chosen point size | common | MS-WORD |
| OpenType: Ligatures | None, Standard Only, Standard and Contextual, Historical and Discretionary, All | niche | MS-WORD |
| OpenType: Number spacing | Default, Proportional, Tabular | niche | MS-WORD |
| OpenType: Number forms | Default, Lining, Old-style | niche | MS-WORD |
| OpenType: Stylistic sets | Default, 1 to 20 | niche | MS-WORD |
| OpenType: Use Contextual Alternates | Toggles contextual alternates | niche | MS-WORD |
| Text Effects | Outline, Shadow, Reflection, Glow (preset and custom: color, size, transparency, blur) | niche | MS-WORD |
| Text Fill and Text Outline | Solid, gradient, picture or texture, pattern; outline color, weight, dash | niche | MS-WORD |
| Text Highlight Color | 15 colors and No Color; Stop Highlighting | core | MS-WORD |
| Character shading | Background color of a run via Borders and Shading | common | MS-WORD |
| Character border | Box around a run | common | MS-WORD |
| Change Case | Sentence case, lowercase, UPPERCASE, Capitalize Each Word, tOGGLE cASE (Shift+F3 cycles) | core | MS-KEYS |
| Clear All Formatting | Resets to Normal paragraph and removes direct run formatting | core | MS-WORD |
| Clear formatting shortcuts | Ctrl+Space (characters), Ctrl+Q (paragraph) | common | MS-KEYS |
| Font dialog (Ctrl+D) | Font and Advanced tabs; Set As Default for this document only or all documents based on Normal.dotm | core | MS-KEYS |
| Language per run | Set language and "Do not check spelling or grammar" | common | MS-WORD |
| Font substitution | Substitutes fonts missing on the machine; Font Substitution dialog on open | niche | MS-WORD |
| East Asian font | Separate font for East Asian text | common | MS-WORD |
| Complex script font | Separate font, size, bold and italic for complex script runs | common | MS-WORD |
| Emphasis mark | East Asian dot or circle above or below | niche | MS-WORD |
| Phonetic Guide (ruby) | Furigana above text with font, size, alignment | niche | MS-WORD |
| Enclose Characters | Circles, squares, triangles, diamond around characters | niche | MS-WORD |
| Combine Characters, Two Lines in One, Horizontal in Vertical | East Asian layout | niche | MS-WORD |
| Insert Symbol dialog | Symbols tab (font, subset, recently used, Unicode and ASCII code, shortcut key, AutoCorrect) | core | MS-WORD |
| Special Characters tab | Em dash, en dash, nonbreaking hyphen, optional hyphen, em space, en space, nonbreaking space, copyright, registered, trademark, section, paragraph, ellipsis, quotes | common | MS-WORD |
| Unicode entry (Alt+X) | Type hex then Alt+X converts to the character and back | niche | MS-WORD |
| Alt codes | Alt plus numeric keypad decimal code entry | niche | MS-WORD |
| Assign shortcut to symbol | Customize Keyboard binding | niche | MS-WORD |
| Insert Emoji | Windows emoji panel (Win+.) | common | MS-WORD |
| Text boundaries | Shows dotted margins and column boundaries in Advanced options | niche | MS-WORD |
| Formatting marks | Show/Hide (Ctrl+Shift+8) for paragraph marks, tabs, spaces, hidden text, optional hyphens, object anchors | core | MS-KEYS |
| Reveal Formatting | Shift+F1 pane listing font, paragraph and section formatting and comparing two selections | niche | MS-KEYS |
| Cloud fonts | Fonts downloaded on demand in Microsoft 365 | niche | MS-WORD |

---

## 4. Paragraph

Main sources: MS-WORD, MS-KEYS.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Alignment: Left, Centered, Right, Justified | Basic alignment | core | MS-KEYS |
| Alignment: Distributed | Spreads words and, for East Asian text, characters across the line | niche | MS-WORD |
| Alignment: Thai Distributed, Justify High, Medium, Low | Language specific justification | niche | MS-OI29500 |
| Outline level | Body Text or Level 1 to 9 for navigation and TOC | common | MS-WORD |
| Indentation: Left, Right | Paragraph indents in cm, in, pt | core | MS-WORD |
| Special indent: First line, Hanging | Indentation of the first line or of the others | core | MS-WORD |
| Mirror indents | Inside and Outside replace Left and Right | niche | MS-WORD |
| Automatically adjust right indent when document grid is defined | Grid option | niche | MS-WORD |
| Spacing Before and After | Points or lines, Auto | core | MS-WORD |
| Don't add space between paragraphs of the same style | Contextual spacing | common | MS-WORD |
| Line spacing: Single, 1.5 lines, Double | Multiples of the font line height | core | MS-KEYS |
| Line spacing: At least | Minimum height in points | common | MS-WORD |
| Line spacing: Exactly | Fixed height in points, clips tall glyphs | common | MS-WORD |
| Line spacing: Multiple | Any multiplier such as 1.15 | core | MS-WORD |
| Snap to grid when document grid is defined | Aligns lines to the page grid | niche | MS-WORD |
| Widow/Orphan control | Prevents a single line at the top or bottom of a page | core | MS-WORD |
| Keep with next | Keeps the paragraph on the same page as the next | core | MS-WORD |
| Keep lines together | Prevents the paragraph splitting across pages | core | MS-WORD |
| Page break before | Starts the paragraph on a new page | core | MS-WORD |
| Suppress line numbers | Removes line numbers for the paragraph | niche | MS-WORD |
| Don't hyphenate | Excludes the paragraph from automatic hyphenation | common | MS-WORD |
| Textbox tight wrap | None, All lines, First and last lines, First line only, Last line only | niche | MS-WORD |
| East Asian typography options | Allow Latin text to wrap in the middle of a word, hanging punctuation, overflow punctuation, compress punctuation, kinsoku line breaking, auto space between East Asian and Latin text and numbers | niche | MS-WORD |
| Text direction: LTR and RTL paragraph | Right to Left paragraph direction buttons | common | MS-WORD |
| Vertical text direction | Text Direction for text boxes and table cells | niche | MS-WORD |
| Tabs dialog: stop position | Default tab stops and custom positions | core | MS-WORD |
| Tab alignment | Left, Center, Right, Decimal, Bar | common | MS-WORD |
| Tab leaders | None, dots, dashes, line, middle dot | common | MS-WORD |
| Clear and Clear All tabs | Removes tab stops | common | MS-WORD |
| Ruler tab selector | Click the ruler to add stops; first line indent and hanging indent markers | core | MS-WORD |
| Borders and Shading: Borders | Box, All, Grid, Custom, Shadow, 3-D; style, color, width (1/4 pt to 6 pt), Art borders, apply to paragraph or text, Options distance from text | common | MS-WORD |
| Borders and Shading: Shading | Fill, pattern style and color, apply to paragraph or text | common | MS-WORD |
| Horizontal line | Line from the borders menu | common | MS-WORD |
| Borders button gallery | Bottom, Top, Left, Right, No border, All, Outside, Inside, Inside horizontal, Inside vertical, Diagonal Down, Diagonal Up, View Gridlines | common | MS-WORD |
| Merged borders | Adjacent paragraphs with identical borders share one box | common | MS-OI29500 |
| Drop cap | None, Dropped, In margin; font, lines to drop, distance from text | common | MS-WORD |
| Show/Hide paragraph marks | Pilcrow display | core | MS-KEYS |
| Sort | Sort paragraphs or table rows by text, number or date, up to 3 keys, header row, field separators, case sensitive, language | common | MS-WORD |
| Bullets, Numbering, Multilevel List buttons | See section 6 | core | MS-WORD |
| Increase and Decrease Indent buttons | Step the indent by the default tab stop | core | MS-WORD |
| Line and Paragraph Spacing menu | Preset spacing, Add or Remove Space Before and After | core | MS-WORD |
| Frame | Positioned paragraph container with text wrapping, horizontal and vertical position, size | niche | MS-WORD |
| Paragraph mark formatting | The paragraph mark carries its own run properties | common | MS-LEARN-WML |
| Document grid | Layout > Page Setup > Document Grid: no grid, text only, lines and characters, snap to grid, characters per line, lines per page | niche | MS-WORD |

---

## 5. Styles and themes

Main sources: MS-WORD, MS-LEARN-WML.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Paragraph style | Named set of paragraph and run properties | core | MS-WORD |
| Character style | Named run properties applied to a selection | core | MS-WORD |
| Linked style | Paragraph and character pair that applies as either depending on the selection | common | MS-WORD |
| Table style | Named table formatting with conditional regions | common | MS-WORD |
| List style | Named multilevel list definition | niche | MS-WORD |
| Styles pane (Ctrl+Alt+Shift+S) | Floating pane with style list, show preview, disable linked styles, options | core | MS-KEYS |
| Apply Styles (Ctrl+Shift+S) | Small box to type or choose a style name | niche | MS-KEYS |
| Quick Styles gallery | Ribbon gallery of styles flagged to show in the gallery | core | MS-WORD |
| Style Inspector | Shows paragraph and text level style and direct formatting, with Reset to Normal Paragraph Style, Clear Paragraph Formatting, New style from selection | niche | MS-WORD |
| Reveal Formatting | See section 3 | niche | MS-KEYS |
| Modify Style | Name, style type, style based on, style for following paragraph, formatting, add to gallery, automatically update, only in this document, new documents based on this template | core | MS-WORD |
| Modify Style Format menu | Font, Paragraph, Tabs, Border, Language, Frame, Numbering, Shortcut key, Text Effects | common | MS-WORD |
| Create a Style from selection | New style from formatted text | core | MS-WORD |
| Style based on | Inheritance chain between styles | core | MS-LEARN-WML |
| Style for following paragraph | Next style after Enter | core | MS-LEARN-WML |
| Automatically update | Style redefines when direct formatting changes | niche | MS-WORD |
| Update to match selection | Redefines the style from the selection | common | MS-WORD |
| Delete style, Revert to default | Removes a custom style or resets a built in one | common | MS-WORD |
| Manage Styles dialog | Edit, Recommend, Restrict, Set Defaults; sort order (Alphabetical, As Recommended, By Font, By Type) | niche | MS-WORD |
| Organizer | Copies styles, macros, toolbars between documents and templates | niche | MS-WORD |
| Latent styles | Built in styles that appear on demand, with locked, priority, hidden state | niche | MS-OI29500 |
| Built in styles | Normal, Heading 1 to 9, Title, Subtitle, Body Text, List Paragraph, Quote, Intense Quote, Strong, Emphasis, Caption, Header, Footer, Footnote Text, Endnote Text, Hyperlink, TOC 1 to 9, Index 1 to 9, Table of Figures, Comment Text, Balloon Text, No Spacing, Plain Text | core | MS-OI29500 |
| Style Sets (Design tab) | Switches the whole style definition set; Set as Default | common | MS-WORD |
| Themes (Design tab) | Package of theme colors, theme fonts and theme effects | common | MS-WORD |
| Theme Colors | Twelve slots: Dark1, Light1, Dark2, Light2, Accent1 to 6, Hyperlink, Followed Hyperlink; Customize Colors | common | MS-WORD |
| Theme Fonts | Heading and body font pair; Customize Fonts | common | MS-WORD |
| Theme Effects | Shape effect scheme | niche | MS-WORD |
| Paragraph Spacing gallery | Document wide spacing presets: Compact, Tight, Open, Relaxed, Double | niche | MS-WORD |
| Document Defaults | docDefaults in styles.xml: default run and paragraph properties | core | MS-LEARN-WML |
| Set As Default | Saves the current font or paragraph settings to Normal.dotm | common | MS-WORD |
| Normal style reset | Reset to the template Normal; Reset to Normal Paragraph Style | common | MS-WORD |
| Style separator | Hidden paragraph mark that joins two styled runs in one paragraph (Ctrl+Alt+Enter) | niche | MS-WORD |
| Style area pane | Left side listing of paragraph style names in Draft and Outline | niche | MS-WORD |
| Keyboard shortcut per style | Customize Keyboard assignment | niche | MS-WORD |
| Limit allowed styles | Formatting restrictions through Restrict Editing | niche | MS-WORD |
| Table style options | Header Row, Total Row, Banded Rows, First Column, Last Column, Banded Columns | common | MS-WORD |
| Conditional table formatting | Style regions: whole table, first and last row, first and last column, banded rows and columns, corner cells | common | MS-LEARN-WML |
| Style property resolution | Document defaults, then table style, then paragraph style, then character style, then direct formatting; toggle properties (b, i, caps, smallCaps, strike, dstrike, vanish, shadow, outline, emboss, imprint) combine by XOR | core | MS-OI29500 |

---

## 6. Lists

Main sources: MS-WORD, MS-LEARN-WML.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Bullets | Applies a bullet list from the bullet library | core | MS-WORD |
| Define New Bullet: Symbol | Picks any font and glyph | common | MS-WORD |
| Define New Bullet: Picture | Image bullet from file or online | niche | MS-WORD |
| Bullet font, size, color, alignment | Bullet formatting | common | MS-WORD |
| Numbering | Numbered list with library | core | MS-WORD |
| Number formats | 1,2,3; 1st,2nd; One,Two; First,Second; a,b,c; A,B,C; i,ii,iii; I,II,III; 01,02; plus Arabic, Hebrew, Chinese counting, Japanese, Korean, Thai and others | core | MS-OI29500 |
| Define New Number Format | Number style, format string with level placeholders such as "%1.%2", font, alignment, start at | common | MS-WORD |
| Multilevel List | Up to 9 levels, each with its own numbering style and indent | core | MS-WORD |
| Define New Multilevel List | Link level to style, number style for this level, include level number from, start at, restart list after, legal style numbering, follow number with (Tab, Space, Nothing), number position, text indent at, add tab stop at, apply changes to (whole list, this point forward, current paragraph), set for all levels | common | MS-WORD |
| Define New List Style | Creates a named list style | niche | MS-WORD |
| Link Level to Style | Binds a heading style to a list level, giving numbered headings | common | MS-WORD |
| Restart at 1 | Starts new numbering at the selected paragraph | core | MS-WORD |
| Continue numbering | Joins the previous list | core | MS-WORD |
| Set Numbering Value | Start at value, advance value (skip numbers) | common | MS-WORD |
| Legal style numbering | Forces arabic numerals at all levels | niche | MS-WORD |
| Adjust List Indents | Number position, text indent, follow number with | common | MS-WORD |
| Change List Level | Demote and promote with Tab and Shift+Tab | core | MS-WORD |
| Automatic bullets and numbering as you type | Starting a line with "1." or "*" and pressing Enter begins a list; Enter on an empty item ends it | core | MS-WORD |
| Number restart rules | A level restarts after higher level numbers unless lvlRestart says otherwise | common | MS-OI29500 |
| List Number and List Bullet styles | Built in list styles | common | MS-WORD |
| Lists inside table cells | Numbering in cells | core | MS-WORD |
| LISTNUM field | Numbering inside a paragraph (Ctrl+Alt+L) | niche | MS-KEYS |
| AUTONUM, AUTONUMLGL, AUTONUMOUT fields | Legacy paragraph numbering fields | niche | MS-FIELDS |
| numbering.xml model | abstractNum, num, lvl, lvlOverride, multiLevelType (singleLevel, multilevel, hybridMultilevel), nsid, tmpl, isLgl, suff, lvlText, lvlJc, pPr, rPr, lvlPicBulletId | core | MS-LEARN-WML |

---

## 7. Tables

Main sources: MS-WORD, MS-LEARN-WML.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Insert Table grid | Pick rows and columns by hovering (up to 10 by 8) | core | MS-WORD |
| Insert Table dialog | Rows, columns, fixed column width, AutoFit to contents, AutoFit to window, remember dimensions | core | MS-WORD |
| Draw Table | Pen to draw cell borders, Eraser | common | MS-WORD |
| Excel Spreadsheet | Embedded OLE worksheet | niche | MS-WORD |
| Quick Tables | Gallery of formatted tables (calendars, tabular lists, matrix) | niche | MS-WORD |
| Convert Text to Table | Separate by paragraphs, commas, tabs, other; fixed width and autofit options | common | MS-WORD |
| Convert Table to Text | Separate by paragraph marks, tabs, commas, other; convert nested tables | common | MS-WORD |
| Insert rows above and below | Adds rows | core | MS-WORD |
| Insert columns left and right | Adds columns | core | MS-WORD |
| Insert Cells | Shift cells right or down, insert entire row or column | niche | MS-WORD |
| Delete cells, rows, columns, table | Deletion with shift options | core | MS-WORD |
| Merge Cells | Joins selected cells | core | MS-WORD |
| Split Cells | Splits into N columns and rows, merge cells before split | core | MS-WORD |
| Split Table | Breaks a table into two at the selected row | common | MS-WORD |
| Table Properties: Table tab | Preferred width (points or percent), alignment (Left, Center, Right), indent from left, text wrapping (None, Around), positioning, borders and shading, options | common | MS-WORD |
| Table Positioning | Horizontal and vertical position, distance from surrounding text, move with text, allow overlap | niche | MS-WORD |
| Table Options | Default cell margins, allow spacing between cells, automatically resize to fit contents | common | MS-WORD |
| Table Properties: Row tab | Specify height (At least, Exactly), allow row to break across pages, repeat as header row at the top of each page | common | MS-WORD |
| Table Properties: Column tab | Preferred width (points or percent), previous and next column | common | MS-WORD |
| Table Properties: Cell tab | Preferred width, vertical alignment (Top, Center, Bottom), Cell Options (margins, wrap text, fit text, same as the whole table) | common | MS-WORD |
| Table Properties: Alt Text | Title and description | common | MS-ACCESS |
| Repeat Header Rows | Header row appears on each page | core | MS-WORD |
| AutoFit Contents | Columns size to text | core | MS-WORD |
| AutoFit Window | Table fills the page width | core | MS-WORD |
| Fixed Column Width | Disables automatic resizing | core | MS-WORD |
| Table layout algorithm | tblLayout fixed or autofit, with tcW and tblGrid interplay | core | MS-OI29500 |
| Distribute Rows Evenly, Distribute Columns Evenly | Equal sizing | core | MS-WORD |
| Cell Size group | Height, width, AutoFit, Distribute | core | MS-WORD |
| Cell alignment grid | Nine alignments (Align Top Left through Align Bottom Right) | common | MS-WORD |
| Text Direction in cells | Horizontal, rotate all text 90, rotate all text 270 | common | MS-WORD |
| Cell Margins | Top, bottom, left, right, spacing between cells | common | MS-WORD |
| Table borders | Border styles gallery, line style, line weight, pen color, Border Painter, Borders menu | core | MS-WORD |
| Table shading | Cell, row, table fill | core | MS-WORD |
| Table Styles gallery | Prebuilt styles (Plain, Grid Table 1 to 7, List Table 1 to 7, each in accent colors) | common | MS-WORD |
| New and Modify Table Style | Formatting for 10 regions | niche | MS-WORD |
| Clear table style | Reverts to Table Normal | common | MS-WORD |
| Sort (table) | Up to three columns, ascending or descending, header row | common | MS-WORD |
| Formula | `=SUM(ABOVE)`, `=SUM(LEFT)`, AVERAGE, COUNT, MIN, MAX, PRODUCT, IF, AND, OR, NOT, ABS, INT, MOD, ROUND, SIGN, TRUE, FALSE, DEFINED; A1 style cell references; ABOVE, BELOW, LEFT, RIGHT; number format | common | MS-FIELDS |
| Bookmarks in table formulas | Refer to values through bookmark names | niche | MS-FIELDS |
| Update table formulas | F9 updates `=` fields | common | MS-KEYS |
| Nested tables | A table inside a cell | common | MS-WORD |
| Table move handle | Four headed arrow at top left to drag the table | common | MS-WORD |
| Table resize handle | Small square at bottom right | common | MS-WORD |
| Resize by dragging borders | Drag column and row borders; Alt shows measurements | core | MS-WORD |
| Select cell, row, column, table | Click margin or column top, Layout > Select | core | MS-WORD |
| Tab in last cell | Appends a row | core | MS-WORD |
| Move rows with Alt+Shift+Up/Down | Reorders rows | niche | MS-KEYS |
| View Gridlines | Shows dotted borders for tables without borders | core | MS-WORD |
| Floating table | Text wraps around the table | common | MS-WORD |
| Table caption | Insert Caption for tables | common | MS-WORD |
| Table Design and Layout tabs | Contextual tabs | core | MS-WORD |
| Diagonal cell borders | Diagonal Down and Up | niche | MS-WORD |
| Cell merging model | gridSpan horizontally, vMerge vertically | core | MS-LEARN-WML |
| Table grid | tblGrid gridCol widths in twips | core | MS-LEARN-WML |
| Table look | Flags for conditional formatting | common | MS-LEARN-WML |

---

## 8. Illustrations

Main sources: MS-WORD, MS-LEARN-WML.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Pictures: This Device | Inserts PNG, JPEG, GIF, BMP, TIFF, EMF, WMF, SVG, WebP | core | MS-WORD |
| Pictures: Stock Images | Microsoft licensed images, icons, cutouts, illustrations, stickers (subscription) | niche | MS-WORD |
| Pictures: Online Pictures | Bing search with Creative Commons filter | niche | MS-WORD |
| Insert and Link | Picture linked to a file instead of embedded | niche | MS-WORD |
| Screenshot, Screen Clipping | Captures a window or a clipped area | common | MS-WORD |
| Picture Format tab | Contextual tab | core | MS-WORD |
| Remove Background | Marks areas to keep or remove | niche | MS-WORD |
| Corrections | Sharpen or soften, brightness and contrast | common | MS-WORD |
| Color | Saturation, tone, recolor presets, Set Transparent Color | common | MS-WORD |
| Artistic Effects | Pencil sketch, blur, mosaic bubbles and others | niche | MS-WORD |
| Transparency | Picture transparency presets | niche | MS-WORD |
| Compress Pictures | Reduce resolution (330, 220, 150 ppi), delete cropped areas, apply only to this picture | common | MS-WORD |
| Change Picture, Reset Picture | Replace the source keeping size and format; discard formatting | common | MS-WORD |
| Picture Styles | Preset borders, shadows, frames, reflections | common | MS-WORD |
| Picture Border | Color, weight, dashes | common | MS-WORD |
| Picture Effects | Shadow, Reflection, Glow, Soft Edges, Bevel, 3-D Rotation | niche | MS-WORD |
| Picture Layout | Convert pictures to SmartArt | niche | MS-WORD |
| Crop | Drag crop handles; Crop to Shape; Aspect Ratio presets; Fill; Fit | common | MS-WORD |
| Format Picture pane | Fill and Line, Effects, Layout and Properties, Picture sections | common | MS-WORD |
| Lock aspect ratio | Preserves proportions when sizing | core | MS-WORD |
| Rotate and flip | Rotate Right 90, Left 90, Flip Vertical, Flip Horizontal, More Rotation Options | common | MS-WORD |
| Shapes gallery | Lines, rectangles, basic shapes, block arrows, equation shapes, flowchart, stars and banners, callouts, action buttons | common | MS-WORD |
| New Drawing Canvas | Container that groups shapes and fits to content | niche | MS-WORD |
| Shape Format tab | Shape Styles, Shape Fill (solid, gradient, picture, texture), Shape Outline, Shape Effects, WordArt Styles, Text Direction, Align Text, Create Link | common | MS-WORD |
| Edit Points | Edit freeform vertices | niche | MS-WORD |
| Change Shape, Merge Shapes | Union, Combine, Fragment, Intersect, Subtract | niche | MS-WORD |
| Connectors | Lines that stay connected to shapes | niche | MS-WORD |
| Text in shapes | Text with margins, autofit, wrap | common | MS-WORD |
| Icons | Library of SVG icons | niche | MS-WORD |
| 3D Models | glTF, GLB, FBX, OBJ, 3MF, PLY, STL models with rotation | niche | MS-WORD |
| SmartArt | Graphics for list, process, cycle, hierarchy, relationship, matrix, pyramid, picture; Text pane | niche | MS-WORD |
| SmartArt Design tab | Add Shape, Promote, Demote, Right to Left, Layouts, Change Colors, SmartArt Styles, Reset | niche | MS-WORD |
| Chart | Column, line, pie, bar, area, XY scatter, map, stock, surface, radar, treemap, sunburst, histogram, box and whisker, waterfall, funnel, combo; data edited in Excel | common | MS-WORD |
| Chart Design and Format | Chart Elements, Quick Layout, Change Colors, Chart Styles, Switch Row/Column, Select Data, Change Chart Type | common | MS-WORD |
| WordArt | Text with preset effects | niche | MS-WORD |
| Text Box: Draw | Draws a text box | core | MS-WORD |
| Text Box: Built in | Gallery (Simple, Austin Quote, Sidebar, Banded and others) | niche | MS-WORD |
| Linked text boxes | Create Link and Break Link flow text across boxes | niche | MS-WORD |
| Text box margins and autofit | Resize shape to fit text, do not autofit, wrap text in shape | common | MS-WORD |
| Wrap Text: In Line with Text | Object treated as a character | core | MS-WORD |
| Wrap Text: Square | Wraps in a rectangle around the shape | core | MS-WORD |
| Wrap Text: Tight | Wraps to the shape contour | common | MS-WORD |
| Wrap Text: Through | Wraps through open areas | niche | MS-WORD |
| Wrap Text: Top and Bottom | Text above and below only | core | MS-WORD |
| Wrap Text: Behind Text | Object under text | core | MS-WORD |
| Wrap Text: In Front of Text | Object over text | core | MS-WORD |
| Edit Wrap Points | Adjusts the contour used by Tight and Through | niche | MS-WORD |
| Wrap text sides | Both sides, Left only, Right only, Largest only | common | MS-WORD |
| Distance from text | Top, bottom, left, right | common | MS-WORD |
| Position presets | With Text Wrapping presets in nine page positions | common | MS-WORD |
| Layout dialog: Position | Horizontal: alignment, book layout, absolute position, relative position relative to margin, page, column, character, left margin, right margin, inside margin, outside margin. Vertical: same with line, paragraph, top margin, bottom margin. Move object with text, lock anchor, allow overlap, layout in table cell | common | MS-WORD |
| Layout dialog: Text Wrapping | Wrapping style, wrap text sides, distance from text | common | MS-WORD |
| Layout dialog: Size | Height, width absolute and relative, rotation, scale, lock aspect ratio, relative to original picture size | common | MS-WORD |
| Anchor, Lock Anchor | Object anchored to a paragraph; anchor icon visible with formatting marks | common | MS-WORD |
| Align | Left, Center, Right, Top, Middle, Bottom, Distribute Horizontally and Vertically, Align to Page or Margin, Use Alignment Guides, Grid Settings | common | MS-WORD |
| Group, Ungroup, Regroup | Combines shapes | common | MS-WORD |
| Z order | Bring Forward, Send Backward, Bring to Front, Send to Back, Bring in Front of Text, Send Behind Text | core | MS-WORD |
| Rotate | Arbitrary angle | common | MS-WORD |
| Alt Text | Description and decorative marking | core | MS-ACCESS |
| Insert Caption | Label and number above or below a figure | core | MS-WORD |
| Selection Pane | Names, hides and reorders objects | common | MS-WORD |
| Hyperlink on picture | Ctrl+K on a picture or shape | common | MS-WORD |
| Linked pictures | INCLUDEPICTURE field and external relationship | niche | MS-FIELDS |
| Draw tab (inking) | Pens, highlighters, ink to shape, ink to math, ruler | niche | MS-WORD |
| Online Video | Embeds a web video as a picture with a link | niche | MS-WORD |
| SVG with PNG fallback | Native SVG with PNG fallback stored together | common | MS-LEARN-WML |
| Embedded chart workbook | Chart part with externalData relationship | niche | MS-LEARN-WML |

---

## 9. Page layout and sections

Main sources: MS-WORD, MS-LEARN-WML.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Margin presets | Normal, Narrow, Moderate, Wide, Mirrored, Office 2003 Default, Last Custom | core | MS-WORD |
| Custom margins | Top, Bottom, Left, Right | core | MS-WORD |
| Gutter and gutter position | Extra binding margin on the left or top | common | MS-WORD |
| Mirror margins | Inside and Outside | common | MS-WORD |
| Multiple pages: 2 pages per sheet | Page Setup option | niche | MS-WORD |
| Multiple pages: Book fold | Sheets per booklet, gutter | niche | MS-WORD |
| Orientation | Portrait, Landscape | core | MS-WORD |
| Size | Letter, Legal, A4, A3, B5, envelope sizes, custom width and height | core | MS-WORD |
| Paper source | First page and other pages tray | niche | MS-WORD |
| Columns presets | One, Two, Three, Left, Right | core | MS-WORD |
| Columns custom | Number, width and spacing per column, equal width, apply to (whole document, this section, this point forward), line between | common | MS-WORD |
| Balanced columns | A continuous section break at the end balances column lengths | common | MS-WORD |
| Page Break (Ctrl+Enter) | Hard page break | core | MS-WORD |
| Column Break (Ctrl+Shift+Enter) | Moves text to the next column | common | MS-WORD |
| Text Wrapping Break | Ends the line and clears wrap around objects | niche | MS-WORD |
| Section Break: Next Page | Starts the section on a new page | core | MS-WORD |
| Section Break: Continuous | Starts on the same page | core | MS-WORD |
| Section Break: Even Page | Starts on the next even page | niche | MS-WORD |
| Section Break: Odd Page | Starts on the next odd page | niche | MS-WORD |
| Line Numbers | None, Continuous, Restart Each Page, Restart Each Section, Suppress for Current Paragraph; options start at, from text, count by | niche | MS-WORD |
| Hyphenation | None, Automatic, Manual | common | MS-WORD |
| Hyphenation Options | Hyphenate words in CAPS, hyphenation zone, limit consecutive hyphens | common | MS-WORD |
| Optional hyphen | Ctrl+hyphen | common | MS-WORD |
| Nonbreaking hyphen and space | Ctrl+Shift+hyphen and Ctrl+Shift+Space | common | MS-WORD |
| Indent and Spacing group | Layout tab paragraph indent and spacing boxes | core | MS-WORD |
| Watermark | Text or picture behind text; built in (CONFIDENTIAL, DRAFT, DO NOT COPY); custom text with font, size, color, layout (diagonal, horizontal), semitransparent; Remove Watermark | common | MS-WORD |
| Page Color | Solid, Fill Effects (gradient, texture, pattern, picture), No Color | common | MS-WORD |
| Print Background Colors and Images | Option | niche | MS-WORD |
| Page Borders | Box, Shadow, 3-D, Custom; style, color, width, Art; apply to whole document, this section, first page only, all except first; measure from edge of page or text | common | MS-WORD |
| Vertical alignment on page | Top, Center, Justified, Bottom | common | MS-WORD |
| Different First Page | Section setting (titlePg) | core | MS-WORD |
| Different Odd and Even | Document wide setting (evenAndOddHeaders in settings) | core | MS-WORD |
| Page numbering per section | Start at, continue from previous | core | MS-WORD |
| Link to previous | Each header or footer reference inherits the previous section | core | MS-WORD |
| Per section settings | Page size, orientation, margins, columns, headers, footers, line numbers, vertical alignment, borders, notes placement, text direction, document grid | core | MS-LEARN-WML |
| Section text direction | Horizontal, Vertical, Stacked | niche | MS-WORD |
| Page Setup dialog | Margins, Paper, Layout, Document Grid tabs; Set As Default | core | MS-WORD |
| Footnotes and endnotes per section | Placement and numbering restart | common | MS-WORD |
| Suppress endnotes | Section option | niche | MS-OI29500 |
| Gridlines and ruler | Display options on View | core | MS-WORD |

---

## 10. Headers, footers and page numbers

Main sources: MS-WORD, MS-LEARN-WML.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Insert Header, Insert Footer | Gallery of built in headers and footers (Blank, Three Columns, Austin, Banded and others) | core | MS-WORD |
| Edit Header, Edit Footer | Double click the area to edit | core | MS-WORD |
| Remove Header, Remove Footer | Deletes content | core | MS-WORD |
| Header & Footer tab | Contextual tab | core | MS-WORD |
| Different First Page | Separate first page header and footer | core | MS-WORD |
| Different Odd & Even Pages | Separate headers for odd and even pages | common | MS-WORD |
| Show Document Text | Toggles body visibility while editing a header | niche | MS-WORD |
| Link to Previous | Inherits the previous section's header and footer | core | MS-WORD |
| Go to Header, Go to Footer, Previous, Next | Navigation | common | MS-WORD |
| Header from Top, Footer from Bottom | Distance from the page edge | common | MS-WORD |
| Insert Alignment Tab | Left, Center, Right alignment relative to margin or indent with leader (ptab) | niche | MS-WORD |
| Page Number gallery | Top of Page, Bottom of Page, Page Margins, Current Position | core | MS-WORD |
| Format Page Numbers | Number format (1,2,3; a,b,c; A,B,C; i,ii,iii; I,II,III and others), include chapter number (heading level and separator), continue from previous section, start at | common | MS-WORD |
| Remove Page Numbers | Removes page number fields in all headers and footers | common | MS-WORD |
| Page X of Y | NUMPAGES with PAGE | core | MS-FIELDS |
| Date & Time | Insert with format list; update automatically | common | MS-WORD |
| Document Info | Author, File Name, File Path, Document Title | common | MS-WORD |
| Quick Parts in header | Fields and building blocks | common | MS-WORD |
| Pictures in header | Logos | core | MS-WORD |
| Close Header and Footer | Esc or button | core | MS-WORD |
| Watermark in header | Drawn in the header story | common | MS-WORD |
| Header and footer types | default, first, even per section | core | MS-LEARN-WML |
| Tables and text boxes in headers | Layout in headers | common | MS-WORD |
| SECTIONPAGES and SECTION fields | Section info | niche | MS-FIELDS |

---

## 11. References

Main sources: MS-WORD, MS-FIELDS.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Table of Contents: Automatic Table 1, 2 | Builds from Heading styles with a title and content control | core | MS-WORD |
| Table of Contents: Manual Table | Placeholder entries | niche | MS-WORD |
| Custom Table of Contents dialog | Show page numbers, right align page numbers, tab leader, show levels, use hyperlinks instead of page numbers, Options (styles with levels, outline levels, table entry fields), Modify (TOC styles) | common | MS-WORD |
| Update Table | Update page numbers only or the entire table | core | MS-WORD |
| Remove Table of Contents | Deletes | common | MS-WORD |
| Add Text | Marks a paragraph for a TOC level | common | MS-WORD |
| TC field (Mark entry) | `TC "text" \l level \f identifier \n` | niche | MS-FIELDS |
| Table of Figures | From caption labels, other style, TC entries; include label and number; show page numbers | common | MS-WORD |
| Table of Authorities | Mark Citation (TA fields), categories (Cases, Statutes, Other Authorities, Rules, Treatises, Regulations, Constitutional Provisions), short and long citation, passim | niche | MS-WORD |
| Insert Footnote | Ctrl+Alt+F; reference mark in text and note text at the bottom of the page | core | MS-WORD |
| Insert Endnote | Ctrl+Alt+D; notes at the end of the document or section | core | MS-WORD |
| Footnote and Endnote dialog | Location (bottom of page, below text; end of section, end of document), columns, number format, custom mark, symbol, start at, numbering (continuous, restart each section, restart each page) | common | MS-WORD |
| Convert notes | Footnotes to endnotes, endnotes to footnotes, swap | niche | MS-WORD |
| Next Footnote, Show Notes | Navigation | common | MS-WORD |
| Note separators | Separator, continuation separator, continuation notice | niche | MS-WORD |
| Continuation notice | Text such as "continued" when a note spills to the next page | niche | MS-WORD |
| Note numbering formats | Arabic, roman, letters, symbols (* † ‡ §) | common | MS-WORD |
| Insert Citation | Adds a source from the list with page numbers, or a placeholder | common | MS-WORD |
| Manage Sources | Master list and current list; Copy, Delete, Edit, New, Browse | common | MS-WORD |
| Source types | Book, Book Section, Journal Article, Article in a Periodical, Conference Proceedings, Report, Web site, Document from Web site, Electronic Source, Art, Sound Recording, Performance, Film, Interview, Patent, Case, Miscellaneous | niche | MS-WORD |
| Bibliography styles | APA, MLA, Chicago, Harvard, IEEE, ISO 690, SIST02, GB7714, GOST, Turabian | common | MS-WORD |
| Bibliography, References, Works Cited | Gallery of bibliography blocks | common | MS-WORD |
| Insert Placeholder | Citation placeholder for an unknown source | niche | MS-WORD |
| Insert Caption | Label (Figure, Table, Equation, or custom through New Label), position, exclude label, numbering format, include chapter number, AutoCaption | common | MS-WORD |
| Cross-reference | Reference type (Numbered item, Heading, Bookmark, Footnote, Endnote, Equation, Figure, Table, custom labels); insert reference to (heading text, page number, heading number, paragraph number, above or below, entire caption, only label and number, only caption text); insert as hyperlink | core | MS-WORD |
| Index: Mark Entry | Main entry, subentry, cross-reference, current page, page range (bookmark), bold, italic; Mark All | niche | MS-WORD |
| XE field | Index entry field `XE "main:sub" \b \i \r \t` | niche | MS-FIELDS |
| Index: Insert Index | Print Preview, type (Indented, Run-in), columns, language, right align page numbers, tab leader, formats (From template, Classic, Fancy, Modern, Bulleted, Formal, Simple), AutoMark with a concordance file | niche | MS-WORD |
| Update Index | Rebuilds | niche | MS-WORD |
| Bookmark | Name, hidden bookmarks option, sort by name or location, Go To, Delete | core | MS-WORD |
| Hyperlink (Ctrl+K) | Existing File or Web Page, Place in This Document (headings, bookmarks), Create New Document, E-mail Address (subject), ScreenTip, Target Frame, Remove Hyperlink | core | MS-WORD |
| Hyperlink styles | Hyperlink and FollowedHyperlink character styles | core | MS-WORD |
| Smart Lookup | Web definitions and results in a pane | niche | MS-WORD |
| Researcher | Searches sources and inserts cited text | niche | MS-WORD |
| TOC field | `TOC \o "1-3" \h \z \u` | core | MS-FIELDS |
| Master Document | Subdocuments linked through Outline view (section 15) | niche | MS-WORD |

---

## 12. Fields

Main sources: MS-FIELDS (Insert > Quick Parts > Field), MS-LEARN-WML.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| PAGE | Current page number | core | MS-FIELDS |
| NUMPAGES | Total pages | core | MS-FIELDS |
| SECTION | Current section number | niche | MS-FIELDS |
| SECTIONPAGES | Pages in the section | niche | MS-FIELDS |
| NUMWORDS, NUMCHARS | Statistics | niche | MS-FIELDS |
| DATE | Current date with `\@` picture | core | MS-FIELDS |
| TIME | Current time | core | MS-FIELDS |
| CREATEDATE | Date the document was created | common | MS-FIELDS |
| SAVEDATE | Date last saved | common | MS-FIELDS |
| PRINTDATE | Date last printed | niche | MS-FIELDS |
| EDITTIME | Total editing minutes | niche | MS-FIELDS |
| AUTHOR | Author property | common | MS-FIELDS |
| LASTSAVEDBY | Last author | niche | MS-FIELDS |
| TITLE, SUBJECT, KEYWORDS, COMMENTS | Summary properties | common | MS-FIELDS |
| FILENAME | Name, with `\p` for path | common | MS-FIELDS |
| FILESIZE | Size | niche | MS-FIELDS |
| TEMPLATE | Template name | niche | MS-FIELDS |
| DOCPROPERTY | Built in or custom property | common | MS-FIELDS |
| DOCVARIABLE | Document variable | niche | MS-FIELDS |
| INFO | Legacy property | niche | MS-FIELDS |
| USERNAME, USERINITIALS, USERADDRESS | User info | niche | MS-FIELDS |
| TOC | Table of contents; switches `\o \t \u \h \z \n \p \b \f \l \w \x \d \a \c \s` | core | MS-FIELDS |
| TC | Table entry | niche | MS-FIELDS |
| TA, TOA | Table of authorities | niche | MS-FIELDS |
| INDEX | Index with switches | niche | MS-FIELDS |
| XE | Index entry | niche | MS-FIELDS |
| RD | Includes entries from another document in a TOC or index | niche | MS-FIELDS |
| REF | Bookmark text; switches `\h \p \n \r \w \f \d \t` | core | MS-FIELDS |
| PAGEREF | Page number of a bookmark | core | MS-FIELDS |
| NOTEREF | Footnote or endnote number | common | MS-FIELDS |
| STYLEREF | Text of the nearest paragraph of a style, for running headers | common | MS-FIELDS |
| SEQ | Sequence number by identifier with `\c \n \r \h \s` | common | MS-FIELDS |
| AUTONUM, AUTONUMLGL, AUTONUMOUT | Automatic paragraph numbers | niche | MS-FIELDS |
| LISTNUM | List number inside a paragraph | niche | MS-FIELDS |
| HYPERLINK | Link with `\l \m \n \o \t` | core | MS-FIELDS |
| INCLUDETEXT | Inserts text from another file or bookmark | niche | MS-FIELDS |
| INCLUDEPICTURE | Inserts a picture from a file | niche | MS-FIELDS |
| INCLUDE | Legacy include | niche | MS-FIELDS |
| LINK | Linked OLE data | niche | MS-FIELDS |
| EMBED | Embedded OLE object | niche | MS-FIELDS |
| DDE, DDEAUTO | Dynamic data exchange | niche | MS-FIELDS |
| IF | Conditional text with comparison operators `= <> < > <= >=` | common | MS-FIELDS |
| COMPARE | Compares two expressions and returns 0 or 1 | niche | MS-FIELDS |
| SKIPIF | Merge skip | niche | MS-FIELDS |
| = (Formula) | Arithmetic with functions and `\#` formats | common | MS-FIELDS |
| MERGEFIELD | Merge data field with `\b \f \m \v` | common | MS-FIELDS |
| MERGEREC, MERGESEQ | Record number | niche | MS-FIELDS |
| NEXT, NEXTIF | Next record | niche | MS-FIELDS |
| DATABASE | Inserts a data table from a database | niche | MS-FIELDS |
| ASK | Prompts and stores the answer in a bookmark | niche | MS-FIELDS |
| FILLIN | Prompts and inserts the response | niche | MS-FIELDS |
| SET | Assigns a bookmark value | niche | MS-FIELDS |
| QUOTE | Inserts literal text | niche | MS-FIELDS |
| SYMBOL | Inserts a character with `\f \s \u \h \j` | common | MS-FIELDS |
| CITATION | Citation from sources | common | MS-FIELDS |
| BIBLIOGRAPHY | Bibliography | common | MS-FIELDS |
| BARCODE | POSTNET or FIM barcode | niche | MS-FIELDS |
| EQ | Equation field: array, bracket, displace, fraction, integral, list, overstrike, radical, subscript or superscript, box | niche | MS-FIELDS |
| ADVANCE | Moves text position | niche | MS-FIELDS |
| ADDRESSBLOCK, GREETINGLINE | Mail merge blocks | common | MS-FIELDS |
| GOTOBUTTON, MACROBUTTON | Click targets | niche | MS-FIELDS |
| PRIVATE | Preserves data from file conversion | niche | MS-FIELDS |
| FORMTEXT, FORMCHECKBOX, FORMDROPDOWN | Legacy form fields | niche | MS-FIELDS |
| Switch `\* MERGEFORMAT` | Preserves formatting of the previous result | common | MS-FIELDS |
| Switch `\* CHARFORMAT` | Formats the result like the first letter of the field type | niche | MS-FIELDS |
| Switches `\* Upper`, `Lower`, `FirstCap`, `Caps` | Case | common | MS-FIELDS |
| Switches `\* Arabic`, `Roman`, `roman`, `ALPHABETIC`, `alphabetic`, `Ordinal`, `Cardtext`, `Ordtext`, `Hex`, `Dollartext` | Number formats | common | MS-FIELDS |
| Switch `\@ "dddd, MMMM d, yyyy"` | Date and time picture: d, dd, ddd, dddd, M, MM, MMM, MMMM, yy, yyyy, h, hh, H, HH, m, mm, s, ss, AM/PM, am/pm | core | MS-FIELDS |
| Switch `\# "#,##0.00"` | Numeric picture: 0, #, x, decimal point, thousands, sign, `;` sections, quoted literal text | common | MS-FIELDS |
| Switch `\* LOCK`, `\!` | Lock the result; formatting switch | niche | MS-FIELDS |
| Update fields (F9) | Updates the selected fields; Ctrl+A then F9 updates all | core | MS-KEYS |
| Toggle field codes | Shift+F9 for one field, Alt+F9 for all | core | MS-KEYS |
| Lock and unlock | Ctrl+F11 and Ctrl+Shift+F11 | niche | MS-KEYS |
| Unlink | Ctrl+Shift+F9 converts the result to static text | common | MS-KEYS |
| Next field | F11 | niche | MS-KEYS |
| Insert empty field | Ctrl+F9 | niche | MS-KEYS |
| Field shading | Never, Always, When selected (Advanced options) | common | MS-WORD |
| Nested fields | Fields inside field arguments | niche | MS-FIELDS |
| Cached results | Result text stored with the field | core | MS-LEARN-WML |
| Dirty fields | w:dirty updates the field on open | niche | MS-LEARN-WML |
| Complex field storage | fldChar begin, separate, end with instrText | core | ECMA-376 |
| Simple field storage | w:fldSimple with w:instr | core | ECMA-376 |
| Update on open | updateFields in settings prompts to update | niche | MS-OE376 |

---

## 13. Mailings

Main sources: MS-WORD, MS-FIELDS.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Envelopes | Delivery and return address, envelope size, feed, add to document, electronic postage | common | MS-WORD |
| Labels | Label vendor and product number (Avery and others), single label or full page, options for tray and custom label sizes | common | MS-WORD |
| Start Mail Merge types | Letters, E-mail Messages, Envelopes, Labels, Directory, Normal Word Document | common | MS-WORD |
| Step by Step Mail Merge Wizard | Task pane that walks through the merge | common | MS-WORD |
| Select Recipients: Type a New List | Creates an address list in a small database | common | MS-WORD |
| Select Recipients: Use an Existing List | Excel workbook, Access database, text or CSV file, ODBC, Outlook contacts | common | MS-WORD |
| Select Recipients: Choose from Outlook Contacts | Contacts folder as source | niche | MS-WORD |
| Edit Recipient List | Sort, filter, find duplicates, find recipient, validate addresses, deselect records | common | MS-WORD |
| Filter and Sort recipients | Query options with AND and OR | common | MS-WORD |
| Highlight Merge Fields | Shades merge fields | common | MS-WORD |
| Address Block | Inserts a formatted address with name formats and country rules | common | MS-WORD |
| Greeting Line | Salutation with format and invalid recipient fallback | common | MS-WORD |
| Insert Merge Field | Inserts MERGEFIELD for a data column | core | MS-FIELDS |
| Match Fields | Maps address block fields to columns | niche | MS-WORD |
| Rules | Ask, Fill-in, If...Then...Else, Merge Record #, Merge Sequence #, Next Record, Next Record If, Set Bookmark, Skip Record If | niche | MS-WORD |
| Update Labels | Copies the first label layout to all labels | common | MS-WORD |
| Preview Results | Shows merged data with record navigation and Find Recipient | core | MS-WORD |
| Check for Errors | Simulates the merge and reports problems | niche | MS-WORD |
| Finish & Merge: Edit Individual Documents | Creates a new document with all merged letters | core | MS-WORD |
| Finish & Merge: Print Documents | Prints all, current, or a record range | core | MS-WORD |
| Finish & Merge: Send Email Messages | Uses Outlook; To field, subject, HTML, attachment or plain text | niche | MS-WORD |
| Directory merge | Catalog output with all records in one document | niche | MS-WORD |
| Merge to new document by section | Next record section break between records | common | MS-WORD |
| Data source connection | Data source file path, header source, ODSO elements in settings.xml (mailMerge) | niche | MS-OE376 |
| Mail merge number formats | `\# ` and `\@` switches on MERGEFIELD to format numbers and dates | common | MS-FIELDS |

---

## 14. Review

Main sources: MS-WORD, MS-KEYS.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Spelling & Grammar (Editor) | Checks spelling, grammar and refinements; F7 opens the Editor pane | core | MS-WORD |
| Editor pane | Lists corrections, refinements (clarity, conciseness, formality, inclusive language), similarity check, Editor Score | common | MS-WORD |
| Red squiggle spelling, blue double underline grammar | Check as you type indicators | core | MS-WORD |
| Right click spelling menu | Suggestions, Ignore All, Add to Dictionary, AutoCorrect, Language | core | MS-WORD |
| Thesaurus | Shift+F7 synonyms pane | common | MS-WORD |
| Word Count | Pages, words, characters (no spaces and with spaces), paragraphs, lines; include textboxes, footnotes and endnotes; selection count in the status bar | core | MS-WORD |
| Read Aloud | Speaks text with reading speed and voice | common | MS-WORD |
| Check Accessibility | Accessibility checker pane | common | MS-ACCESS |
| Translate | Translates a selection or the document | niche | MS-WORD |
| Language: Set Proofing Language | Per selection language and "do not check spelling" | core | MS-WORD |
| Language: Language Preferences | Editing and display languages | common | MS-WORD |
| Language: Set as default | Applies language to Normal.dotm | niche | MS-WORD |
| New Comment | Ctrl+Alt+M adds a comment anchored to a range | core | MS-KEYS |
| Reply, Resolve, Reopen | Threaded comments in modern comments | common | MS-WORD |
| Delete Comment | Delete one, all shown, all in document | core | MS-WORD |
| Previous and Next comment | Navigation | core | MS-WORD |
| @mentions | Mentions a person and sends them a notification | niche | MS-WORD |
| Show Comments | Contextual (balloon style) or Simple, and the comments pane | common | MS-WORD |
| Comments pane | Lists all comments with filter by reviewer | common | MS-WORD |
| Track Changes On/Off | Ctrl+Shift+E; For Everyone or Just Me | core | MS-KEYS |
| Lock Tracking | Password so tracking cannot be turned off | niche | MS-WORD |
| Display for Review | Simple Markup, All Markup, No Markup, Original | core | MS-WORD |
| Show Markup | Comments, Ink, Insertions and Deletions, Formatting, Highlight Updates, Specific People | common | MS-WORD |
| Balloons | Show revisions in balloons, show all revisions inline, show only comments and formatting in balloons | common | MS-WORD |
| Reviewing Pane | Vertical or horizontal list of revisions and comments with a summary count | common | MS-WORD |
| Accept and Reject | Accept or Reject this change, all shown, all in document | core | MS-WORD |
| Next and Previous change | Navigation | core | MS-WORD |
| Track moves | Moves recorded as moveFrom and moveTo instead of delete and insert | niche | MS-WORD |
| Track formatting | Formatting changes recorded with the old properties | common | MS-WORD |
| Track Changes Options | Insertions color and mark, deletions, changed lines, comments color, moves, table cell highlighting, formatting, balloons width and margin, paper orientation for printing | niche | MS-WORD |
| Change User Name | Author name and initials used for revisions | common | MS-WORD |
| Compare | Compares two versions and shows differences as tracked changes; options for comparison settings and level (character or word) and show changes in | niche | MS-WORD |
| Combine | Merges revisions from multiple authors into one document | niche | MS-WORD |
| Show Source Documents | Hide, show original, show revised, show both | niche | MS-WORD |
| Block Authors | Prevents edits in the region by a chosen author in shared files | niche | MS-WORD |
| Restrict Editing: Formatting restrictions | Limit formatting to a selection of styles | niche | MS-WORD |
| Restrict Editing: Editing restrictions | Tracked changes, Comments, Filling in forms, No changes (Read only) | niche | MS-WORD |
| Restrict Editing: Exceptions | Select regions editable by Everyone or named users | niche | MS-WORD |
| Start Enforcing Protection | Password or user authentication | niche | MS-WORD |
| Hide Ink | Hides ink annotations | niche | MS-WORD |
| Smart Lookup, Researcher | See section 11 | niche | MS-WORD |
| Document Inspector | See section 1 | common | MS-WORD |
| Review tab shortcuts | Alt then R opens the tab; Ctrl+Alt+M comment; Ctrl+Shift+E track changes | core | MS-KEYS |

---

## 15. View

Main sources: MS-WORD, MS-KEYS.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Read Mode | Full screen reading with columns, page navigation, tools (Find, Search with Bing, View options) | common | MS-WORD |
| Print Layout | Page view with margins, headers, footers, floating objects | core | MS-WORD |
| Web Layout | Shows the document as a web page without pagination | niche | MS-WORD |
| Outline view | Hierarchical view with promote, demote, move up and down, expand and collapse, show level, show text formatting, show first line only | common | MS-WORD |
| Outline tab: Master Document | Show Document, Create, Insert, Unlink, Merge, Split, Lock; subdocument files | niche | MS-WORD |
| Draft view | Text only view with no page margins and a style area | niche | MS-WORD |
| Focus | Hides ribbon and all other chrome | niche | MS-WORD |
| Immersive Reader | Column width, page color, text spacing, syllables, read aloud, grammar tools, line focus, picture dictionary, translate | niche | MS-WORD |
| Page Movement: Vertical, Side to Side | Scroll direction | niche | MS-WORD |
| Ruler | Horizontal and vertical ruler | core | MS-WORD |
| Gridlines | Alignment grid | common | MS-WORD |
| Navigation Pane | Headings, Pages (thumbnails), Results; drag headings to reorder | core | MS-WORD |
| Zoom dialog | Percent, Page width, Text width, Whole page, Many pages, Zoom to 100% | core | MS-WORD |
| One Page, Multiple Pages, Page Width | Zoom group commands | core | MS-WORD |
| Zoom slider | Status bar slider | core | MS-WORD |
| New Window | A second window on the same document | common | MS-WORD |
| Arrange All | Tiles open windows | niche | MS-WORD |
| Split | Splits the window into two panes (Ctrl+Alt+S) | niche | MS-KEYS |
| View Side by Side | Two documents with Synchronous Scrolling and Reset Window Position | niche | MS-WORD |
| Switch Windows | Choose among open documents | common | MS-WORD |
| Macros | View Macros, Record Macro, Pause Recording | niche | MS-WORD |
| Properties pane | Document panel for properties | niche | MS-WORD |
| Show: Ruler, Gridlines, Navigation Pane | View tab checkboxes | core | MS-WORD |
| Dark mode | Black Office theme and Switch Modes for page color | common | MS-WORD |
| Learning Tools | Immersive Reader entry from View | niche | MS-WORD |
| Page thumbnails | Pages tab in Navigation | core | MS-WORD |

---

## 16. Insert, other

Main sources: MS-WORD.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Cover Page | Gallery of cover pages with content controls | common | MS-WORD |
| Blank Page | Inserts a page break pair | common | MS-WORD |
| Page Break | Ctrl+Enter | core | MS-WORD |
| Quick Parts: AutoText | Saves selected text as a reusable entry | common | MS-WORD |
| Quick Parts: Document Property | Inserts a property as content control | common | MS-WORD |
| Quick Parts: Field | Insert Field dialog | common | MS-WORD |
| Building Blocks Organizer | Preview, edit properties (name, gallery, category, description, save in, options), insert, delete | niche | MS-WORD |
| Building block galleries | Quick Parts, AutoText, Cover Pages, Headers, Footers, Page Numbers, Text Boxes, Watermarks, Equations, Tables, Bibliography, Table of Contents, Custom | niche | MS-WORD |
| Signature Line | Microsoft Office Signature Line with signer, title, email, instructions | niche | MS-WORD |
| Date & Time | Insert dialog with formats and Update automatically | common | MS-WORD |
| Object | OLE object: Create New, Create from File (Link to file, Display as icon) | niche | MS-WORD |
| Text from File | Inserts text from another document | niche | MS-WORD |
| Equation | Inserts equation; built in equations gallery (Area of Circle, Binomial Theorem, Expansion of a Sum, Fourier Series, Pythagorean Theorem, Quadratic Formula, Taylor Expansion, Trig Identities) | common | MS-WORD |
| Equation tools | Symbols, structures (fraction, script, radical, integral, large operator, bracket, function, accent, limit and log, operator, matrix), Professional, Linear, Normal Text, conversion | common | MS-WORD |
| Ink Equation | Handwriting to equation | niche | MS-WORD |
| LaTeX input | Alt+= then LaTeX syntax converted to math | niche | MS-WORD |
| UnicodeMath input | Linear format input in the equation box | niche | MS-WORD |
| Math AutoCorrect | `\alpha` and similar replaced by symbols inside equations | niche | MS-WORD |
| Symbol | See section 3 | core | MS-WORD |
| Emoji | Emoji picker | common | MS-WORD |
| Horizontal Line | Border based rule | common | MS-WORD |
| Online Video | Embedded video | niche | MS-WORD |
| Link (Hyperlink) | Ctrl+K | core | MS-WORD |
| Bookmark, Cross-reference | See section 11 | core | MS-WORD |
| Comment | Insert comment | core | MS-WORD |
| Header, Footer, Page Number | See section 10 | core | MS-WORD |
| Text Box, WordArt, Drop Cap | See sections 4 and 8 | common | MS-WORD |
| Add-ins: Get Add-ins, My Add-ins | Office Add-ins store and task pane add-ins | niche | MS-WORD |
| Wikipedia, Bing images add-ins | Examples of web add-ins | niche | MS-WORD |
| Table, Pictures, Shapes, Icons, 3D Models, SmartArt, Chart, Screenshot | See sections 7 and 8 | core | MS-WORD |
| Dictionary of recent Insert items | Quick insert shortcuts | niche | MS-WORD |

---

## 17. Forms and controls

Main sources: MS-WORD, MS-LEARN-WML.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Developer tab | Hidden by default, enabled in Customize Ribbon | niche | MS-WORD |
| Rich Text Content Control | Formatted text region with placeholder | common | MS-WORD |
| Plain Text Content Control | Unformatted text, optional multiline | common | MS-WORD |
| Picture Content Control | Picture placeholder | niche | MS-WORD |
| Building Block Gallery Content Control | Picks from a gallery | niche | MS-WORD |
| Check Box Content Control | Checked and unchecked symbols (w14:checkbox) | common | MS-WORD |
| Combo Box Content Control | Choice list that also allows typed text | common | MS-WORD |
| Drop-Down List Content Control | Choice list only | common | MS-WORD |
| Date Picker Content Control | Date format, language, calendar type | common | MS-WORD |
| Repeating Section Content Control | Repeats a block of controls (w15:repeatingSection) | niche | MS-WORD |
| Content control properties | Title, Tag, Show as (Bounding box, Start/End tags, None), Color, Use a style to format text, Remove when contents are edited, Content control cannot be deleted, Contents cannot be edited | common | MS-WORD |
| Edit Placeholder Text | Placeholder text shown when empty | common | MS-WORD |
| Group | Locks a set of controls so they cannot be edited as one unit | niche | MS-WORD |
| Design Mode | Shows and edits control boundaries and placeholder text | niche | MS-WORD |
| Legacy Tools: Text Form Field | Text, number, date, current date, current time, calculation; maximum length, default, format | niche | MS-WORD |
| Legacy Tools: Check Box Form Field | Checkbox with size and default state | niche | MS-WORD |
| Legacy Tools: Drop-Down Form Field | Drop down items | niche | MS-WORD |
| Form field protection | Forms work only when the section is protected for Filling in forms | niche | MS-WORD |
| Legacy Tools: Insert Frame, Form Field Shading, Reset Form Fields | Misc legacy controls | niche | MS-WORD |
| ActiveX Controls | Command button, text box, option button, list box, combo box, image, scroll bar, spin button, toggle button, label, check box | niche | MS-WORD |
| Control properties window | ActiveX properties in Design Mode | niche | MS-WORD |
| XML Mapping | Binds a content control to a custom XML part via w:dataBinding and XPath | niche | MS-LEARN-WML |
| Map Pane (XML Mapping Pane) | Shows custom XML parts and maps nodes to controls | niche | MS-WORD |
| Document Template, Add-ins | Attach template, manage COM add-ins and Word add-ins | niche | MS-WORD |
| Macros: Macros dialog | Run, Step Into, Edit, Create, Delete, Organizer | niche | MS-WORD |
| Record Macro | Name, assign to button or keyboard, store in all documents (Normal.dotm) or this document | niche | MS-WORD |
| Visual Basic Editor | Alt+F11 with modules, forms and class modules | niche | MS-WORD |
| Macro Security | Disable all, disable with notification, disable except digitally signed, enable all; Trust access to the VBA project object model | niche | MS-WORD |
| Auto macros | AutoExec, AutoNew, AutoOpen, AutoClose, AutoExit | niche | MS-WORD |
| Document events | Document_Open, Document_New, Document_Close | niche | MS-WORD |
| Word object model | Application, Documents, Range, Selection, Paragraphs, Tables, Fields, Revisions, ContentControls | niche | MS-WORD |
| .docm storage | vbaProject.bin and vbaData.xml | niche | MS-DOCX |

---

## 18. Proofing and automation

Main sources: MS-WORD.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| AutoCorrect: Replace text as you type | Replacement list of typos and symbols (e.g. `(c)` to ©) | core | MS-WORD |
| AutoCorrect: Correct TWo INitial CApitals | Fixes initial double capitals | common | MS-WORD |
| AutoCorrect: Capitalize first letter of sentences | Sentence start capitalisation | core | MS-WORD |
| AutoCorrect: Capitalize first letter of table cells | Cell capitalisation | common | MS-WORD |
| AutoCorrect: Capitalize names of days | Day names capitalised | common | MS-WORD |
| AutoCorrect: Correct accidental usage of cAPS LOCK key | Toggle repair | common | MS-WORD |
| AutoCorrect Exceptions | First Letter exceptions, Initial Caps exceptions, Other corrections | niche | MS-WORD |
| Automatically use suggestions from the spelling checker | Auto replace with suggestion | common | MS-WORD |
| Math AutoCorrect | Replaces `\` names with symbols inside equations or everywhere | niche | MS-WORD |
| AutoFormat As You Type: Straight quotes with smart quotes | Smart quotes | core | MS-WORD |
| AutoFormat As You Type: Ordinals (1st) with superscript | Superscript ordinals | common | MS-WORD |
| AutoFormat As You Type: Fractions (1/2) with fraction character | ½ replacement | common | MS-WORD |
| AutoFormat As You Type: Hyphens (--) with dash | Em or en dash | common | MS-WORD |
| AutoFormat As You Type: Bold and italic with real formatting | `*bold*` and `_italic_` | common | MS-WORD |
| AutoFormat As You Type: Internet and network paths with hyperlinks | URL conversion | core | MS-WORD |
| AutoFormat As You Type: Automatic bulleted lists | List starts | core | MS-WORD |
| AutoFormat As You Type: Automatic numbered lists | List starts | core | MS-WORD |
| AutoFormat As You Type: Border lines | Three dashes or underscores then Enter makes a border | common | MS-WORD |
| AutoFormat As You Type: Tables | `+---+---+` then Enter makes a table | niche | MS-WORD |
| AutoFormat As You Type: Built in Heading styles | Short line without punctuation becomes a heading | niche | MS-WORD |
| AutoFormat As You Type: Format beginning of list item like the one before it | List consistency | niche | MS-WORD |
| AutoFormat As You Type: Set left and first indent with tabs and backspaces | Indent shortcuts | niche | MS-WORD |
| AutoFormat As You Type: Define styles based on your formatting | Style creation | niche | MS-WORD |
| AutoFormat (batch) | Ctrl+Alt+K applies AutoFormat to the document | niche | MS-KEYS |
| AutoText | Reusable entries inserted by typing the name and F3 | common | MS-WORD |
| Custom dictionaries | Add, edit and manage word lists per language (CUSTOM.DIC), Add or Edit Word List, RTL | common | MS-WORD |
| Exclusion dictionary | List of words always flagged (per language file) | niche | MS-WORD |
| Grammar settings | Writing Style (Grammar, Grammar & Refinements), settings for each rule, Check grammar with spelling, Show readability statistics | common | MS-WORD |
| Readability statistics | Counts, averages (sentences per paragraph, words per sentence, characters per word), Flesch Reading Ease, Flesch-Kincaid Grade Level, Passive Sentences | niche | MS-WORD |
| Proofing options | Ignore words in UPPERCASE, words that contain numbers, Internet and file addresses, flag repeated words, enforce accented uppercase in French, suggest from main dictionary only | common | MS-WORD |
| Check spelling as you type, Mark grammar errors as you type | Background proofing | core | MS-WORD |
| Recheck Document | Resets ignored words | common | MS-WORD |
| Autocomplete suggestions (Text Predictions) | Gray inline suggestions accepted with Tab | niche | MS-WORD |
| Dictate | Speech to text with voice commands and punctuation | niche | MS-WORD |
| Transcribe | Records and transcribes audio | niche | MS-WORD |
| Editor (AI) | Clarity, conciseness, formality, vocabulary suggestions | niche | MS-WORD |
| Copilot in Word | Generative drafting, rewriting, summarising, chat; Microsoft 365 Copilot licence required. Word specific and cloud bound. | niche | MS-WORD |
| Sensitivity labels | Microsoft Purview labels applied to the document | niche | MS-WORD |
| Smart Lookup | Web lookup | niche | MS-WORD |

---

## 19. Collaboration and sharing

Main sources: MS-WORD. These features depend on OneDrive, SharePoint, Exchange or Microsoft account services. A desktop clone without Microsoft cloud services cannot implement them as Word does, so they are out of scope unless the clone supplies its own service. A clone can still offer file based equivalents (send as attachment, comments, tracked changes, compare).

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Real time co authoring | Several people edit one cloud stored file at the same time with presence cursors | common | MS-WORD |
| Share link | Link with view or edit permission, expiry, password, block download | common | MS-WORD |
| Share with people | Invite by email with permissions | common | MS-WORD |
| Comments @mention notifications | Email and in app notification for a mentioned person | niche | MS-WORD |
| OneDrive and SharePoint storage | Cloud storage with AutoSave | common | MS-WORD |
| Version History | Server side versions with restore | niche | MS-WORD |
| Email as attachment | Send a copy as .docx or PDF through Outlook | common | MS-WORD |
| Present Online | Broadcast to a browser audience | niche | MS-WORD |
| Transform to a web page (Sway) | Retired service | niche | MS-WORD |
| Check for conflicting changes | Merge dialog when two authors edit offline | niche | MS-WORD |
| Catch up and Show changes | Highlights what others changed since the last visit | niche | MS-WORD |
| Word for the web | Browser editor with a subset of features | niche | MS-WORD |
| Loop components | Embedded live components | niche | MS-WORD |
| Teams integration | Share to a chat or channel | niche | MS-WORD |

---

## 20. Accessibility and navigation

Main sources: MS-ACCESS, MS-WORD.

| Feature | What it does | Tier | Source |
|---|---|---|---|
| Screen reader support | Exposes document structure and UI to Narrator, JAWS and NVDA through UI Automation and MSAA | core | MS-ACCESS |
| Accessibility Checker | Lists Errors, Warnings and Tips such as missing alt text, no header row, merged cells, blank table cells, poor heading order, hard to read contrast, ambiguous hyperlink text, floating objects, repeated blank characters | common | MS-ACCESS |
| Alt text on pictures, shapes, charts, tables, SmartArt | Description; "Mark as decorative" | core | MS-ACCESS |
| Heading structure | Use real Heading styles in order | core | MS-ACCESS |
| Table header rows | Mark header row for tables | core | MS-ACCESS |
| Hyperlink text | Meaningful link text and screen tips | core | MS-ACCESS |
| Color contrast | Warn on low contrast text | common | MS-ACCESS |
| Language tagging | Correct proofing language for text | common | MS-ACCESS |
| Keyboard navigation | Every command reachable by keyboard: Alt key tips (KeyTips), Tab and arrow movement in the ribbon, F6 to move between panes, Ctrl+F6 between windows, Shift+F10 context menu | core | MS-KEYS |
| Access keys | Alt then letters, for example Alt, H, FG for Grow Font | common | MS-KEYS |
| High contrast | Respects Windows High Contrast themes | common | MS-ACCESS |
| Learning Tools | Column width, page color, text spacing, syllables, line focus, read aloud | niche | MS-WORD |
| Text spacing | Space between characters, words, lines | niche | MS-WORD |
| Read Aloud | Speech with word highlight | common | MS-WORD |
| Immersive Reader | See section 15 | niche | MS-WORD |
| Tagged PDF export | Structure tags | common | MS-ACCESS |
| Accessible templates | Built in templates marked accessible | niche | MS-ACCESS |
| Navigation pane for headings | Jump by heading | core | MS-WORD |
| Zoom and magnifier | Zoom to 500%, Windows Magnifier compatibility | core | MS-WORD |
| Alt text auto generation | Suggests descriptions using Microsoft services | niche | MS-ACCESS |
| Accessibility status bar indicator | Shows "Accessibility: Good to go" or the issue count | niche | MS-WORD |

---

## 21. Keyboard shortcuts (Word for Windows)

Source: MS-KEYS, https://support.microsoft.com/en-us/office/keyboard-shortcuts-in-word-95ef89dd-7142-4b50-afb2-f762f663ceb2 (Windows section, fetched during this task). Rows marked "added" are shortcuts widely documented for Word that the fetched summary did not list. They come from knowledge, not from that fetch.

### Frequently used

| Keys | Action |
|---|---|
| Ctrl+O | Open a document |
| Ctrl+N | New document |
| Ctrl+S | Save |
| Ctrl+W | Close |
| Ctrl+X | Cut |
| Ctrl+C | Copy |
| Ctrl+V | Paste |
| Ctrl+Shift+V | Paste text only |
| Ctrl+A | Select all |
| Ctrl+B | Bold |
| Ctrl+I | Italic |
| Ctrl+U | Underline |
| Ctrl+[ | Decrease font size by 1 pt |
| Ctrl+] | Increase font size by 1 pt |
| Ctrl+E | Center text |
| Ctrl+L | Align left |
| Ctrl+R | Align right |
| Esc | Cancel a command |
| Ctrl+Z | Undo |
| Ctrl+Y | Redo |
| Ctrl++ | Zoom in |
| Ctrl+- | Zoom out |
| Ctrl+0 | Return to 100% zoom |
| Ctrl+Scroll | Zoom with the mouse |
| Ctrl+Alt+S | Split the document window |
| Alt+Shift+C or Ctrl+Alt+S | Remove the window split |
| F4 (added) | Repeat last action |
| Ctrl+P (added) | Print |
| Ctrl+H (added) | Replace |
| Ctrl+K (added) | Insert hyperlink |
| Ctrl+F6 (added) | Next document window |
| F7 (added) | Spelling and grammar |
| F12 (added) | Save As |
| Alt+F4 (added) | Close Word |

### Navigate the document

| Keys | Action |
|---|---|
| Ctrl+Left | Move one word left |
| Ctrl+Right | Move one word right |
| Ctrl+Up | Move up one paragraph |
| Ctrl+Down | Move down one paragraph |
| End | End of line |
| Home | Beginning of line |
| Ctrl+Alt+Page Up | Top of screen |
| Ctrl+Alt+Page Down | Bottom of screen |
| Page Up | Scroll up one screen |
| Page Down | Scroll down one screen |
| Ctrl+Page Down | Top of next page |
| Ctrl+Page Up | Top of previous page |
| Ctrl+End | End of document |
| Ctrl+Home | Beginning of document |
| Shift+F5 | Previous revision location (or last revision before closing, if pressed right after opening) |
| Ctrl+Alt+5, then Tab repeatedly | Cycle through floating shapes |
| Esc | Exit floating shape navigation |
| Ctrl+F | Open the Navigation pane |
| Ctrl+G | Open Go To |
| Ctrl+Alt+Z | Cycle through the four previous changes |

### Select text and graphics

| Keys | Action |
|---|---|
| Shift+Arrow keys | Select text |
| Ctrl+Shift+Left | Select word to the left |
| Ctrl+Shift+Right | Select word to the right |
| Shift+Home | Select to beginning of line |
| Shift+End | Select to end of line |
| Ctrl+Shift+Up | Select to beginning of paragraph |
| Ctrl+Shift+Down | Select to end of paragraph |
| Shift+Page Up | Select to top of screen |
| Shift+Page Down | Select to bottom of screen |
| Ctrl+Shift+Home | Select to beginning of document |
| Ctrl+Shift+End | Select to end of document |
| Ctrl+Alt+Shift+Page Down | Select to bottom of window |
| Ctrl+A | Select all |
| F8 | Start extend selection mode |
| F8 then Left or Right | Select nearest character left or right |
| F8 repeated | Expand to word, sentence, paragraph, section, then document |
| Shift+F8 | Reduce the selection |
| Ctrl+Shift+F8, then arrow keys | Select a vertical block |
| Esc | Stop extending the selection |

### Format characters

| Keys | Action |
|---|---|
| Ctrl+D or Ctrl+Shift+F | Open the Font dialog |
| Ctrl+Shift+> | Increase font size |
| Ctrl+Shift+< | Decrease font size |
| Ctrl+] | Increase font size by 1 pt |
| Ctrl+[ | Decrease font size by 1 pt |
| Shift+F3 | Toggle case (upper, lower, title) |
| Ctrl+Shift+A | All caps |
| Ctrl+Shift+H | Hidden text |
| Ctrl+B | Bold |
| Ctrl+Shift+L | Bulleted list |
| Ctrl+U | Underline |
| Ctrl+Shift+W | Underline words only |
| Ctrl+Shift+D | Double underline |
| Ctrl+I | Italic |
| Ctrl+Shift+K | Small caps |
| Ctrl+Shift+- | Subscript (the Microsoft page lists this key as shown) |
| Ctrl+Shift++ | Superscript (the Microsoft page lists this key as shown) |
| Ctrl+= (added) | Subscript in the classic keymap |
| Ctrl+Shift+= (added) | Superscript in the classic keymap |
| Ctrl+Space | Remove manual character formatting |
| Ctrl+Shift+Q | Change selection to Symbol font |
| Ctrl+Shift+8 | Show or hide nonprinting characters |
| Shift+F1 | Reveal Formatting pane |
| Ctrl+Shift+C (added) | Copy formatting |
| Ctrl+Shift+V (added) | Paste formatting. The Microsoft page lists Ctrl+Shift+V as Paste text only in the Frequently used section. Behaviour depends on the configured paste option. |

### Paragraphs

| Keys | Action |
|---|---|
| Ctrl+E | Center |
| Ctrl+J | Justify |
| Ctrl+L | Align left |
| Ctrl+R | Align right |
| Ctrl+M | Indent |
| Ctrl+Shift+M | Remove indent |
| Ctrl+T | Hanging indent |
| Ctrl+Shift+T | Remove hanging indent |
| Ctrl+Q | Remove paragraph formatting |
| Ctrl+1 | Single spacing |
| Ctrl+2 | Double spacing |
| Ctrl+5 | 1.5 line spacing |
| Ctrl+Alt+K | Enable AutoFormat |
| Ctrl+Enter (added) | Page break |
| Shift+Enter (added) | Line break |
| Ctrl+Shift+Enter (added) | Column break |
| Ctrl+- (added) | Optional hyphen |
| Ctrl+Shift+- (added) | Nonbreaking hyphen |
| Ctrl+Shift+Space (added) | Nonbreaking space |

### Styles

| Keys | Action |
|---|---|
| Ctrl+Shift+N | Normal style |
| Ctrl+Alt+1 | Heading 1 |
| Ctrl+Alt+2 | Heading 2 |
| Ctrl+Alt+3 | Heading 3 |
| Ctrl+Shift+S | Apply Styles pane |
| Ctrl+Alt+Shift+S | Styles pane |

### Tables

| Keys | Action |
|---|---|
| Tab | Next cell |
| Shift+Tab | Previous cell |
| Alt+Home | First cell in row |
| Alt+End | Last cell in row |
| Alt+Page Up | First cell in column |
| Alt+Page Down | Last cell in column |
| Up | Previous row |
| Down | Next row |
| Alt+Shift+Up | Move row up |
| Alt+Shift+Down | Move row down |
| Tab (selecting) | Select next cell's content |
| Shift+Tab (selecting) | Select previous cell's content |
| Shift+Arrow keys | Extend selection to adjacent cells |
| Shift+Page Up or Page Down from top or bottom cell | Select column |
| Shift+Alt+End or Home from first or last cell of row | Select row |
| Alt+5 on numeric keypad (Num Lock off) | Select whole table |
| Enter | New paragraph in cell |
| Ctrl+Tab | Insert a tab character in a cell |

### Fields

| Keys | Action |
|---|---|
| Alt+Shift+D | Insert DATE field |
| Ctrl+Alt+L | Insert LISTNUM field |
| Alt+Shift+P | Insert PAGE field |
| Alt+Shift+T | Insert TIME field |
| Ctrl+F9 | Insert empty field |
| Ctrl+Shift+F7 | Update linked information in source document |
| F9 | Update selected fields |
| Ctrl+Shift+F9 | Unlink field |
| Shift+F9 | Toggle selected field code and result |
| Alt+F9 | Toggle all field codes and results |
| Alt+Shift+F9 | Run GOTOBUTTON or MACROBUTTON from field result |
| F11 | Next field |
| Shift+F11 | Previous field |
| Ctrl+F11 | Lock field |
| Ctrl+Shift+F11 | Unlock field |

### Outline (Outline view only)

| Keys | Action |
|---|---|
| Alt+Shift+Left | Promote paragraph |
| Alt+Shift+Right | Demote paragraph |
| Ctrl+Shift+N | Demote to body text |
| Alt+Shift+Up | Move selected paragraphs up |
| Alt+Shift+Down | Move selected paragraphs down |
| Alt+Shift+Plus | Expand or collapse text under heading |
| Alt+Shift+A | Expand or collapse all |
| Numpad / | Show or hide character formatting |
| Alt+Shift+L | Toggle first line of body text or all body text |
| Alt+Shift+1 | Show all Heading 1 headings |
| Alt+Shift+(level number) | Show headings of that level |
| Ctrl+Tab | Insert a tab character |

### Review

| Keys | Action |
|---|---|
| Alt, R | Open Review tab |
| Alt, R, P, 1, L | Show list of comments |
| Ctrl+Alt+M | Insert comment |
| Ctrl+Shift+E | Turn change tracking on or off |
| Alt+Shift+C | Close Reviewing Pane |

### Other (added from knowledge, not from the fetched page)

| Keys | Action |
|---|---|
| Ctrl+F3 | Cut to Spike |
| Ctrl+Shift+F3 | Insert Spike contents |
| F3 | Insert AutoText entry after typing its name |
| Alt+Ctrl+F | Insert footnote |
| Alt+Ctrl+D | Insert endnote |
| Alt+X | Convert Unicode hex to character |
| Alt+Ctrl+Enter | Insert style separator |
| Alt+F11 | Visual Basic Editor |
| Alt+F8 | Macros dialog |
| Alt+Ctrl+I | Print preview toggle in older versions |
| F6 | Move among panes and ribbon |
| Shift+F10 | Context menu |
| F10 or Alt | Show KeyTips |
| Alt+= | Insert equation |

---

## 22. Layout fidelity

Main sources: MS-OI29500 (Word's documented deviations from the standard), MS-OE376 (compatibility settings), ECMA-376 Part 1 clause 17. Word does not publish its line breaking or table layout algorithms. The items below list the behaviours an implementation must reproduce to place the same text on the same pages. Exact figures need testing against Word output.

| Area | What Word does | Tier | Source |
|---|---|---|---|
| Line breaking | Greedy line fill: places words until the next word does not fit, then breaks at a space, hyphen or other break opportunity. Trailing spaces hang past the right margin and are not counted for fit. | core | ECMA-376 |
| Break opportunities | Spaces, hyphens, optional hyphens, em and en dashes, slashes in some cases, East Asian characters between most pairs, subject to kinsoku rules | core | MS-OI29500 |
| Nonbreaking | Nonbreaking space and nonbreaking hyphen prevent breaks | core | ECMA-376 |
| Justification | Pads spaces on the line. Last line of a paragraph and lines ended by a manual break are not justified unless the compat option "Don't expand character spaces on a line ending with SHIFT-RETURN" is off (see compatibility). | core | MS-OE376 |
| Justification of character spacing | Distributed alignment adjusts inter character gaps | niche | MS-OI29500 |
| Hyphenation | Automatic hyphenation uses language dictionaries, the hyphenation zone (words that begin before the zone are candidates), limit on consecutive hyphens, caps option | common | MS-WORD |
| Tabs | Tab advances to the next custom stop, then default tab stops; decimal and bar stops; leaders; hanging indent acts as an implicit tab stop | core | ECMA-376 |
| Line height | Computed from the tallest font in the line using font ascent, descent and line gap; "Exactly" clips; Multiple scales the font line height; compat options alter how font leading is added | core | MS-OE376 |
| Spacing collapse | Space between paragraphs is the larger of after and before only when the HTML auto spacing compat option applies; otherwise the sum. Contextual spacing removes it between same style paragraphs. | core | MS-OE376 |
| Widow and orphan control | Two lines minimum at the top and bottom of a page when on | core | ECMA-376 |
| Keep with next, keep lines together | Chain of paragraphs moves to the next page together; breaks when the chain exceeds a page | core | ECMA-376 |
| Page break before | Forces a page start | core | ECMA-376 |
| Floating object wrapping | Square, tight, through, top and bottom use the wrap polygon; distances from text; wrapText sides; anchors move with the paragraph; behindDoc; allowOverlap; layoutInCell; objects positioned relative to margin, page, column, paragraph, line or character | core | MS-OI29500 |
| Anchored object position limits | Object kept inside page when anchored in a table cell unless layout in cell is off | common | MS-OI29500 |
| Table row breaking | Rows split across pages unless cantSplit is set; header rows repeat; a row taller than a page is cut | core | ECMA-376 |
| Table width resolution | tblW, tcW, tblGrid, tblLayout and content determine column widths; autofit uses minimum and maximum content widths; percentage widths of fiftieths of a percent | core | MS-OI29500 |
| Cell margins and spacing | tblCellMar defaults and tcMar override; tblCellSpacing between cells | common | ECMA-376 |
| Footnote placement | Notes appear at the bottom of the page containing the reference, above the bottom margin, separated by the separator. If the note does not fit, it continues on the next page with the continuation separator and optional continuation notice. | core | ECMA-376 |
| Endnotes | Collected at end of section or document | common | ECMA-376 |
| Balanced columns | A continuous section break after columnar text balances column heights | common | ECMA-376 |
| Column breaks and widths | Equal or custom widths with spacing; separator line | core | ECMA-376 |
| Section breaks | Next page, continuous, even, odd; page size and margin change at break; headers per section | core | ECMA-376 |
| Compatibility settings | w:compat with compatSetting names compatibilityMode (11, 12, 14, 15), overrideTableStyleFontSizeAndJustification, enableOpenTypeFeatures, doNotFlipMirrorIndents, differentiateMultirowTableHeaders, useWord2013TrackBottomHyphenation and others; dozens of boolean w:compat children such as useFELayout, splitPgBreakAndParaMark, doNotUseHTMLParagraphAutoSpacing, balanceSingleByteDoubleByteWidth, suppressSpBfAfterPgBrk, wpJustification, noTabHangInd, noLeading, spaceForUL, noColumnBalance, ulTrailSpace, doNotExpandShiftReturn, usePrinterMetrics, layoutRawTableWidth, layoutTableRowsApart, useWord97LineBreakRules, dontAllowFieldEndSelect | niche | MS-OE376 |
| Compatibility mode by version | Mode 11 is Word 2003, 12 is 2007, 14 is 2010, 15 is 2013 and later. Layout differs between modes (table cell margin handling, line spacing, shape wrapping, text box behaviour). | niche | MS-OE376 |
| Kerning | Pair kerning on from a minimum point size when kern is set | common | ECMA-376 |
| Character spacing | spacing, w (scale), position, fitText | common | ECMA-376 |
| OpenType features | Applied when enableOpenTypeFeatures compat is on or w14 properties are set (ligatures, numForm, numSpacing, stylisticSets, cntxtAlts) | niche | MS-OI29500 |
| East Asian typography | Character grid (docGrid type lines, linesAndChars, snapToChars), kinsoku start and end sets, punctuation compression, hanging punctuation, auto space between Latin and East Asian, vertical text (tbRl), ruby, emphasis marks, eastAsianLayout (combine, vert) | niche | MS-OI29500 |
| RTL and bidi | Paragraph bidi, run rtl, complex script fonts and sizes, mirrored indents, mirrored tab stops, Arabic and Hebrew numerals, section rtlGutter, table bidiVisual | common | MS-OI29500 |
| Vertical text in table cells | textDirection btLr, tbRl on tcPr and sectPr | niche | ECMA-376 |
| Drop caps | Frame based (framePr dropCap with lines) with text wrapped around | common | ECMA-376 |
| Line numbering | lnNumType with countBy, start, distance, restart (newPage, newSection, continuous); suppressLineNumbers per paragraph | niche | ECMA-376 |
| Hyphenation settings | autoHyphenation, consecutiveHyphenLimit, hyphenationZone, doNotHyphenateCaps in settings | common | ECMA-376 |
| Field result layout | Page number fields reflect layout; TOC page numbers need layout engine | core | MS-FIELDS |
| Header and footer fit | Header grows body top margin when taller than the margin | core | ECMA-376 |
| Empty paragraph and page break semantics | Page break followed by paragraph mark; splitPgBreakAndParaMark compat | niche | MS-OE376 |
| Section vertical alignment | vAlign top, center, both, bottom | common | ECMA-376 |
| Text frames and text boxes | Text box content flows within the shape, internal margins, autofit shape to text, linked boxes flow across | common | MS-OI29500 |
| Font metrics and substitution | Layout depends on the actual font; missing fonts are substituted using fontTable panose, family, pitch and altName | core | ECMA-376 |
| Printer metrics | usePrinterMetrics compat uses printer font metrics | niche | MS-OE376 |
| Pagination of floating tables | tblpPr positions tables relative to page, margin or text | niche | ECMA-376 |

---

## 23. WordprocessingML interoperability list

Main sources: ECMA-376 (Parts 1 and 4), ISO/IEC 29500, MS-OI29500, MS-DOCX, MS-LEARN-WML. Namespace prefixes: w (main), r (relationships), wp (DrawingML wordprocessing drawing), a (DrawingML main), pic (picture), wps (word processing shape), wpg (group), wpc (canvas), mc (markup compatibility), m (math), v and o (VML), w10, w14, w15, w16, w16cid, w16cex (Microsoft extensions).

Treatment column values: model means the editor must understand and edit it; preserve means keep the original XML bytes for it when the document is saved unchanged around it. Unknown elements and attributes inside mc:AlternateContent, extension lists (w:extLst) and ignorable namespaces (mc:Ignorable) must always be preserved.

### 23.1 Document body and inline content

| Construct | What it does | Treatment | Source |
|---|---|---|---|
| w:document / w:body | Root and body; last child is the final w:sectPr | model | ECMA-376 |
| w:background | Page color (w:color, themeColor, tint) shown when displayBackgroundShape is set | model | ECMA-376 |
| w:p | Paragraph; attributes w:rsidR, w:rsidRPr, w:rsidRDefault, w:rsidP, w:rsidDel, w14:paraId, w14:textId | model | ECMA-376 |
| w:pPr | Paragraph properties: pStyle, keepNext, keepLines, pageBreakBefore, framePr, widowControl, numPr, suppressLineNumbers, pBdr, shd, tabs, suppressAutoHyphens, kinsoku, wordWrap, overflowPunct, topLinePunct, autoSpaceDE, autoSpaceDN, bidi, adjustRightInd, snapToGrid, spacing, ind, contextualSpacing, mirrorIndents, suppressOverlap, jc, textDirection, textAlignment, textboxTightWrap, outlineLvl, divId, cnfStyle, rPr, sectPr, pPrChange | model | ECMA-376 |
| w:r | Run; attributes rsid*; contains w:rPr then content | model | ECMA-376 |
| w:rPr | Run properties: rStyle, rFonts (ascii, hAnsi, eastAsia, cs, hint, and theme variants), b, bCs, i, iCs, caps, smallCaps, strike, dstrike, outline, shadow, emboss, imprint, noProof, snapToGrid, vanish, webHidden, color, spacing, w, kern, position, sz, szCs, highlight, u, effect, bdr, shd, fitText, vertAlign, rtl, cs, em, lang, eastAsianLayout, specVanish, oMath, plus w14 text effects (glow, shadow, reflection, textOutline, textFill, ligatures, numForm, numSpacing, stylisticSets, cntxtAlts), rPrChange | model | ECMA-376 |
| w:t with xml:space="preserve" | Text; leading and trailing spaces are lost without xml:space="preserve" | model | ECMA-376 |
| w:delText | Deleted text inside w:del | model | ECMA-376 |
| w:instrText | Field instruction text | model | ECMA-376 |
| w:delInstrText | Deleted field instruction | model | ECMA-376 |
| w:tab | Tab character in a run (distinct from w:tabs stops in pPr) | model | ECMA-376 |
| w:br | Break with w:type page, column, textWrapping and w:clear left, right, all | model | ECMA-376 |
| w:cr | Carriage return (line break) | model | ECMA-376 |
| w:sym | Symbol with w:font and w:char hex code | model | ECMA-376 |
| w:noBreakHyphen | Nonbreaking hyphen | model | ECMA-376 |
| w:softHyphen | Optional hyphen | model | ECMA-376 |
| w:ptab | Absolute position tab with alignment, relativeTo, leader | model | ECMA-376 |
| w:lastRenderedPageBreak | Cached page break position from the last render; ignorable; may be dropped on edit | preserve | ECMA-376 |
| w:pgNum | Current page number in header or footer run | model | ECMA-376 |
| w:annotationRef | Comment reference mark inside comment text | model | ECMA-376 |
| w:footnoteRef, w:endnoteRef | Note number mark inside note text | model | ECMA-376 |
| w:separator, w:continuationSeparator | Note separators inside notes parts | model | ECMA-376 |
| w:dayShort, w:monthShort, w:yearShort, w:dayLong, w:monthLong, w:yearLong | Legacy date components | preserve | ECMA-376 |
| w:fldSimple | Simple field with w:instr and w:fldLock, w:dirty | model | ECMA-376 |
| w:fldChar | Complex field marker w:fldCharType begin, separate, end; optional w:fldData, w:ffData (legacy form field data), w:numberingChange | model | ECMA-376 |
| Field nesting | Nested begin and end pairs inside instruction or result | model | ECMA-376 |
| w:hyperlink | Link with r:id (external), w:anchor (bookmark), w:tooltip, w:tgtFrame, w:history, w:docLocation | model | ECMA-376 |
| w:bookmarkStart / w:bookmarkEnd | Range markers with w:id and w:name; colFirst and colLast for table columns; may cross paragraph and table boundaries; _GoBack and _Toc, _Ref prefixed names are common | model | ECMA-376 |
| w:commentRangeStart / w:commentRangeEnd / w:commentReference | Comment anchor range and the reference run | model | ECMA-376 |
| w:footnoteReference / w:endnoteReference | Reference marks with w:id and optional w:customMarkFollows | model | ECMA-376 |
| w:ins / w:del | Tracked insertion and deletion with w:id, w:author, w:date; at run, paragraph mark (inside rPr), row (trPr) and cell level | model | ECMA-376 |
| w:moveFrom / w:moveTo with moveFromRangeStart/End, moveToRangeStart/End | Tracked move | model | ECMA-376 |
| w:rPrChange / w:pPrChange / w:sectPrChange / w:tblPrChange / w:tblPrExChange / w:trPrChange / w:tcPrChange / w:tblGridChange / w:numberingChange | Tracked formatting changes holding the previous properties | model | ECMA-376 |
| w:cellIns / w:cellDel / w:cellMerge | Tracked cell changes | preserve | ECMA-376 |
| w:customXmlInsRangeStart and related | Tracked custom XML range changes | preserve | ECMA-376 |
| w:sdt / w:sdtPr / w:sdtContent / w:sdtEndPr | Structured document tag at block, run, row and cell level; sdtPr children: alias, tag, id, lock, placeholder (docPart), showingPlcHdr, dataBinding, temporary, text, richText, picture, date, dropDownList, comboBox, checkbox (w14), docPartObj, docPartList, citation, bibliography, group, equation, repeatingSection (w15), repeatingSectionItem (w15), color (w15), appearance (w15) | model | ECMA-376 |
| w:tbl / w:tblPr / w:tblGrid / w:tr / w:trPr / w:tc / w:tcPr | Table structure | model | ECMA-376 |
| w:tblPr children | tblStyle, tblpPr, tblOverlap, bidiVisual, tblStyleRowBandSize, tblStyleColBandSize, tblW, jc, tblCellSpacing, tblInd, tblBorders, shd, tblLayout, tblCellMar, tblLook, tblCaption, tblDescription | model | ECMA-376 |
| w:trPr children | cnfStyle, divId, gridBefore, gridAfter, wBefore, wAfter, cantSplit, trHeight, tblHeader, tblCellSpacing, jc, hidden, ins, del | model | ECMA-376 |
| w:tcPr children | cnfStyle, tcW, gridSpan, hMerge, vMerge, tcBorders (including tl2br, tr2bl), shd, noWrap, tcMar, textDirection, tcFitText, vAlign, hideMark | model | ECMA-376 |
| tcW and tblW units | dxa twentieths of a point, pct fiftieths of a percent, auto, nil | model | ECMA-376 |
| w:tblLook | Attributes firstRow, lastRow, firstColumn, lastColumn, noHBand, noVBand and the val hex mask | model | ECMA-376 |
| vMerge | restart starts a merged region; omitted value continues it | model | ECMA-376 |
| w:sectPr | Section properties: headerReference and footerReference (type default, first, even), footnotePr, endnotePr, type, pgSz (w, h, orient, code), pgMar (top, right, bottom, left, header, footer, gutter), paperSrc, pgBorders, lnNumType, pgNumType (fmt, start, chapStyle, chapSep), cols (num, space, equalWidth, sep, child col w and space), formProt, vAlign, noEndnote, titlePg, textDirection, bidi, rtlGutter, docGrid (type, linePitch, charSpace), printerSettings, sectPrChange | model | ECMA-376 |
| Section break placement | Last paragraph of a section holds its sectPr in pPr; final section sectPr is a child of w:body | model | ECMA-376 |
| w:pBdr, w:tcBorders, w:tblBorders borders | Attributes val, sz (eighths of a point), space, color, themeColor, themeTint, themeShade, shadow, frame | model | ECMA-376 |
| w:shd | Shading with val pattern, color, fill, themeFill, themeFillTint, themeFillShade | model | ECMA-376 |
| w:framePr | Frame properties (dropCap, lines, w, h, hSpace, vSpace, wrap, hAnchor, vAnchor, x, xAlign, y, yAlign, hRule) | model | ECMA-376 |
| w:permStart / w:permEnd | Editing permission ranges for protected documents (edGrp, ed) | preserve | ECMA-376 |
| w:proofErr | Spelling and grammar error markers; safe to drop | preserve | ECMA-376 |
| w:smartTag | Smart tag wrapper with element, uri, attributes | preserve | ECMA-376 |
| w:customXml | Custom XML wrapper with element, uri, attributes and w:customXmlPr | preserve | ECMA-376 |
| w:ruby / w:rubyPr / w:rt / w:rubyBase | Phonetic guide | model | ECMA-376 |
| w:fitText, w:eastAsianLayout, w:em | East Asian run properties | model | ECMA-376 |
| w:bdo / w:dir | Bidirectional override containers | preserve | ECMA-376 |
| w:subDoc | Subdocument reference in master documents | preserve | ECMA-376 |
| w:altChunk | Imported external content (HTML, RTF, docx, text) to be merged on open | preserve | ECMA-376 |
| w:object / w:control | Embedded OLE object and ActiveX control reference | preserve | ECMA-376 |
| w:pict | Legacy VML container | preserve | ECMA-376 |
| w:txbxContent | Text box content inside VML or DrawingML shapes; holds block level content | model | ECMA-376 |
| w:glossaryDocument | Building blocks part root | preserve | ECMA-376 |

### 23.2 Drawing, math and alternate content

| Construct | What it does | Treatment | Source |
|---|---|---|---|
| w:drawing | Container for DrawingML | model | ECMA-376 |
| wp:inline | Inline drawing with extent, effectExtent, docPr (id, name, descr, title, hidden), cNvGraphicFramePr, a:graphic | model | ECMA-376 |
| wp:anchor | Floating drawing: simplePos, positionH, positionV (relativeFrom, align, posOffset, pctPosHOffset), extent, effectExtent, wrapNone, wrapSquare, wrapTight, wrapThrough, wrapTopAndBottom (with wrapPolygon, wrapText), docPr, attributes behindDoc, locked, layoutInCell, allowOverlap, relativeHeight, distT, distB, distL, distR, simplePos, wp14:sizeRelH and sizeRelV | model | ECMA-376 |
| a:graphic / a:graphicData | Holds picture, chart, diagram or shape; uri identifies the type | model | ECMA-376 |
| pic:pic | Picture: nvPicPr, blipFill (a:blip r:embed or r:link, a:srcRect crop, a:stretch), spPr (xfrm, prstGeom, ln, effects) | model | ECMA-376 |
| a:blip extensions | a14:imgProps, svg:svgBlip (SVG with PNG fallback), alphaModFix, lum, duotone, artistic effects | preserve | MS-DOCX |
| wps:wsp | Word shape: cNvSpPr, spPr, style, txbx (wps:txbx with w:txbxContent), bodyPr (insets, anchor, wrap, autofit) | model | MS-DOCX |
| wps:txbx and wps:linkedTxbx | Text box content and linked text box chain | model | MS-DOCX |
| wpg:wgp | Group of shapes | model | MS-DOCX |
| wpc:wpc | Drawing canvas | preserve | MS-DOCX |
| mc:AlternateContent / mc:Choice / mc:Fallback | Alternate representations. Choice has a Requires namespace prefix (for example wps, wpg, w14). Fallback usually holds w:pict with VML. Word writes both and reads Choice when it understands the namespace. | model | ECMA-376 |
| mc:Ignorable | Lists namespace prefixes a consumer may ignore | preserve | ECMA-376 |
| VML: v:shape, v:shapetype, v:rect, v:roundrect, v:oval, v:line, v:group, v:imagedata, v:textbox (with w:txbxContent), v:fill, v:stroke, v:shadow, v:path, v:formulas, v:handles, o:lock, o:OLEObject, w10:wrap, w10:anchorlock | Legacy shapes, WordArt, horizontal lines (o:hr), watermarks in headers (shape type _x0000_t136), OLE previews | preserve | ECMA-376 |
| Watermark | VML WordArt shape in a header with id PowerPlusWaterMarkObject | preserve | MS-DOCX |
| m:oMath / m:oMathPara | Office Math inline and display; children m:r, m:f, m:rad, m:sSup, m:sSub, m:sSubSup, m:nary, m:d, m:func, m:acc, m:bar, m:box, m:borderBox, m:eqArr, m:limLow, m:limUpp, m:m (matrix), m:groupChr, m:phant, m:sPre, m:lim, with m:oMathPr and m:rPr | model | ECMA-376 |
| a:graphicData uri charts | c:chart r:id points to a chart part | model | ECMA-376 |
| Charts: c:chartSpace | Chart definition with plotArea, series, axes, legend, style, externalData r:id pointing to the embedded workbook (xlsx) | preserve | ECMA-376 |
| Chart parts | chartN.xml, plus style (chartStyle), colors (chartColorStyle), embedded workbook in /word/embeddings | preserve | ECMA-376 |
| SmartArt: dgm:relIds | r:dm (data), r:lo (layout), r:qs (quick style), r:cs (colors) | preserve | ECMA-376 |
| SmartArt parts | diagrams/data1.xml, layout1.xml, quickStyle1.xml, colors1.xml, and the cached drawing1.xml (dsp:drawing) used for rendering | preserve | MS-DOCX |
| 3D model: am3d:model3d | Model with fallback picture | preserve | MS-DOCX |
| Ink: w14 contentPart or ink parts | Ink strokes stored as inkML | preserve | MS-DOCX |
| a:ext, a:extLst | DrawingML extension lists (decorative flag a16:decorative, svg, cNvPr hlinkClick, creationId) | preserve | MS-DOCX |
| docPr hlinkClick | Hyperlink on a picture or shape | model | ECMA-376 |

### 23.3 Package parts

| Part | What it holds | Treatment | Source |
|---|---|---|---|
| [Content_Types].xml | Default (by extension) and Override (by part name) content types | model | ECMA-376 |
| _rels/.rels | Package relationships to document, core properties, extended properties, custom properties, thumbnail, signatures | model | ECMA-376 |
| word/document.xml | Main story | model | ECMA-376 |
| word/_rels/document.xml.rels | Relationships to styles, numbering, settings, fontTable, theme, webSettings, footnotes, endnotes, comments, headers, footers, images, hyperlinks, charts, diagrams, OLE, custom XML, glossary, people, commentsExtended | model | ECMA-376 |
| word/styles.xml | docDefaults (rPrDefault, pPrDefault), latentStyles (lsdException), style elements (type paragraph, character, table, numbering; styleId, default, customStyle; name, aliases, basedOn, next, link, autoRedefine, hidden, uiPriority, semiHidden, unhideWhenUsed, qFormat, locked, personal, personalCompose, personalReply, rsid, pPr, rPr, tblPr, trPr, tcPr, tblStylePr by type) | model | ECMA-376 |
| word/stylesWithEffects.xml | Word 2010 copy of styles with effects, legacy | preserve | MS-DOCX |
| word/numbering.xml | abstractNum (multiLevelType, lvl: start, numFmt, lvlRestart, pStyle, isLgl, suff, lvlText, lvlPicBulletId, legacy, lvlJc, pPr, rPr), numPicBullet, num (abstractNumId, lvlOverride with startOverride and lvl) | model | ECMA-376 |
| word/settings.xml | zoom, removePersonalInformation, view, trackRevisions, doNotTrackMoves, doNotTrackFormatting, documentProtection (edit, formatting, enforcement, cryptProviderType, hash, salt, spinCount, algorithmName), defaultTabStop, autoHyphenation, hyphenationZone, consecutiveHyphenLimit, evenAndOddHeaders, bookFoldPrinting, characterSpacingControl, footnotePr, endnotePr, compat (compatSetting name/uri/val), docVars, rsids (rsidRoot, rsid), mathPr, themeFontLang, clrSchemeMapping, shapeDefaults, decimalSymbol, listSeparator, attachedTemplate, updateFields, mailMerge, w14:docId, w15:chartTrackingRefBased, w15:presenceInfo | model | ECMA-376 |
| settings rsids | Revision save ids recorded for every edit session; harmless to keep or regenerate | preserve | ECMA-376 |
| word/fontTable.xml | font elements: name, altName, panose1, charset, family, pitch, sig, embedRegular, embedBold, embedItalic, embedBoldItalic with r:id and w:fontKey (GUID) | model | ECMA-376 |
| word/fonts/fontN.odttf | Obfuscated embedded font. The first 32 bytes are XORed with the GUID from w:fontKey (bytes in reverse order of the GUID hex string). Content type application/vnd.openxmlformats-officedocument.obfuscatedFont. | preserve | ECMA-376 |
| word/theme/theme1.xml | a:theme with clrScheme, fontScheme (majorFont, minorFont with script fonts), fmtScheme | model | ECMA-376 |
| word/webSettings.xml | Web view settings (optimizeForBrowser, allowPNG, pixelsPerInch) | preserve | ECMA-376 |
| word/footnotes.xml, word/endnotes.xml | Notes with w:id; ids -1/0 or 0/1 are separator and continuationSeparator (type attribute) | model | ECMA-376 |
| word/comments.xml | w:comment with id, author, date, initials; content paragraphs | model | ECMA-376 |
| word/commentsExtended.xml | w15:commentEx with paraId (the last paragraph's w14:paraId), paraIdParent (reply parent), done (resolved) | model | MS-DOCX |
| word/commentsIds.xml | w16cid:commentId paraId and durableId | preserve | MS-DOCX |
| word/commentsExtensible.xml | w16cex:commentExtensible durableId, dateUtc, intelligentPlaceholder | preserve | MS-DOCX |
| word/people.xml | w15:person with author, presenceInfo providerId and userId | preserve | MS-DOCX |
| word/header1.xml, footer1.xml | Header and footer stories referenced from sectPr by r:id | model | ECMA-376 |
| word/glossary/document.xml | Glossary document (building blocks) with its own styles, settings, fontTable, webSettings and docParts (docPartPr: name, category, types, behaviors, description, guid) | preserve | ECMA-376 |
| customXml/itemN.xml and itemProps N.xml | Custom XML data parts with ds:datastoreItem itemID and schemaRefs; bound by content controls through dataBinding storeItemID and XPath | preserve | ECMA-376 |
| Bibliography part | customXml part with b:Sources (namespace http://schemas.openxmlformats.org/officeDocument/2006/bibliography) | model | ECMA-376 |
| docProps/core.xml | dc:title, dc:subject, dc:creator, cp:keywords, dc:description, cp:lastModifiedBy, cp:revision, dcterms:created, dcterms:modified, cp:category, cp:contentStatus | model | ECMA-376 |
| docProps/app.xml | Application, AppVersion, Template, TotalTime, Pages, Words, Characters, CharactersWithSpaces, Lines, Paragraphs, Company, DocSecurity, ScaleCrop, HeadingPairs, TitlesOfParts, SharedDoc, HyperlinksChanged | preserve | ECMA-376 |
| docProps/custom.xml | Custom properties (op:property fmtid, pid, name, vt:lpwstr, vt:i4, vt:bool, vt:filetime) | model | ECMA-376 |
| docProps/thumbnail.jpeg | Preview image | preserve | ECMA-376 |
| word/vbaProject.bin and word/vbaData.xml | VBA project (compound file) and its data for .docm; also word/vbaProjectSignature.bin | preserve | MS-DOCX |
| word/media/* | Images (png, jpeg, gif, emf, wmf, svg, tiff, bmp) referenced by r:embed | model | ECMA-376 |
| word/embeddings/* | Embedded OLE objects (oleObjectN.bin), packages and chart workbooks (Microsoft_Excel_Sheet.xlsx) | preserve | ECMA-376 |
| word/charts/chartN.xml and word/charts/_rels | Chart parts with relationship to the embedded workbook and to style and colors parts | preserve | ECMA-376 |
| word/diagrams/* | SmartArt data, layout, quickStyle, colors, drawing | preserve | ECMA-376 |
| word/ink/* | Ink content | preserve | MS-DOCX |
| word/printerSettings/* | Printer settings binary | preserve | ECMA-376 |
| word/activeX/* | ActiveX control XML and binary | preserve | ECMA-376 |
| _xmlsignatures/ | Digital signatures (origin.sigs, sigN.xml, XML-DSig) covering package parts; any edit to a signed part invalidates the signature | preserve | ECMA-376 |
| Encrypted package | Password protected files are a CFB container (EncryptionInfo and EncryptedPackage streams, agile encryption with AES) holding the docx, per MS-OFFCRYPTO | preserve | MS-DOCX |
| Relationship TargetMode | Internal versus External (hyperlinks, linked pictures, attached templates, mail merge data sources) | model | ECMA-376 |
| Relationship types | Standard URIs under http://schemas.openxmlformats.org/officeDocument/2006/relationships plus Microsoft extension types for commentsExtended, commentsIds, people, webSettings | model | ECMA-376 |
| Strict versus Transitional | Strict uses namespaces under http://purl.oclc.org/ooxml/ and drops VML and legacy elements. Word writes Transitional by default and can read Strict. | niche | ECMA-376 |
| Flat OPC | Single XML file with pkg:package and pkg:part elements | niche | MS-DOCX |
| Zip handling | Parts are deflated; names are case insensitive per OPC; mimetype ordering is not required; Word tolerates different part names | model | ECMA-376 |

### 23.4 Round trip rules

| Rule | Reason | Source |
|---|---|---|
| Keep every unknown element, attribute and part | Word writes extensions with ignorable namespaces; dropping them loses features the reader cannot see | ECMA-376 |
| Keep both branches of mc:AlternateContent | The Choice is the modern form and the Fallback is read by older consumers | ECMA-376 |
| Keep relationship ids stable or rewrite every reference | r:id values are referenced from document.xml, headers, footers and notes | ECMA-376 |
| Keep w14:paraId consistent with commentsExtended | Reply threading and resolved state depend on paraId equality | MS-DOCX |
| Keep styles referenced by id, not name | styleId is the key; w:name is the UI name | ECMA-376 |
| Do not collapse runs with different rsid attributes if byte stability matters | rsids are noise for rendering but present in diffs | ECMA-376 |
| Regenerate cached field results only on update | TOC, PAGEREF and similar show their last results until updated | MS-FIELDS |
| Preserve w:proofErr and w:lastRenderedPageBreak or drop them deliberately | Safe to drop; Word regenerates them | ECMA-376 |
| Keep compat settings | Dropping w:compat switches Word to a different layout mode and changes pagination | MS-OE376 |
| Keep theme part and clrSchemeMapping | Styles and shapes reference theme colors and fonts by name | ECMA-376 |
| Update docProps/app.xml statistics or leave them | Word recalculates them on open | ECMA-376 |
| Tracked changes: keep w:id unique across ins, del, move, format changes, comments and bookmarks as Word expects uniqueness inside each kind | Duplicate ids can make Word report a damaged file | ECMA-376 |
| Element order inside pPr, rPr, tblPr, sectPr follows the schema sequence | Word rejects some out of order children as corrupt | ECMA-376 |
| Table cells must end with a paragraph; a body ends with sectPr after the last block | Word reports corruption otherwise | ECMA-376 |

---

## 24. What LibreOffice Writer still lacks versus Word

Sources: LO-WRITER (help), LO-FEATURES, LO-WIKI. The LibreOffice wiki feature comparison was blocked to the fetch tool, and a web search returned mostly 2013 to 2020 era forum posts and conference talks (a 2013 LibreOffice conference compatibility talk at https://conference.libreoffice.org/talks/2013/content/sessions/063/files/LibreOffice%202013%20-%20MS%20Compatibility%20v3.9%20%5Bpublic%5D.pdf, a Nouws interoperability talk at https://conference.libreoffice.org/assets/Conference/Brno/nouws-libreoffice-interoperability-strong-weak-points.pdf, and bug reports). Those are old. The statements below mix what those sources report with general knowledge of Writer, and each should be re-tested against a current LibreOffice release (the help site showed version 26.8 when fetched). Treat the list as a calibration of what parity means in practice, not as a verified bug list.

| Area | Gap relative to Word | Confidence |
|---|---|---|
| Layout equivalence | Line breaks and page breaks of imported .docx files often differ from Word because of differences in font metrics handling, table width resolution, compat mode emulation and floating object wrapping. Word compatibility options are partly emulated through Writer compatibility settings. | High (widely reported) |
| Modern comments | Threaded replies and resolved state from commentsExtended are supported in newer releases, but the full modern comment model (mentions, durable ids, reactions) is not. | Medium |
| Co-authoring | No real time co-authoring with presence. Collaboration needs LibreOffice Online or Collabora services. | High |
| Content controls | Writer gained content control support for rich text, plain text, checkbox, dropdown, combo box and date in the 7.x series, but repeating section, building block gallery, data binding to custom XML and Word's group and lock semantics are incomplete. | Medium |
| SmartArt | Imported with fallback drawing or converted to shapes; Writer does not edit SmartArt layouts. The 2013 talk listed SmartArt flattened to simple shapes. | High |
| Charts | Charts import as Writer chart objects with differences in styles; round trip of Word chart styles and extensions is lossy. | Medium |
| Themes | Word theme colors and fonts are mapped to direct formatting on import in older versions; theme support exists for Impress and Calc more than Writer. The 2013 talk listed themes transformed to direct formatting. | Medium |
| OpenType typography | Ligatures, stylistic sets and number forms are available through font feature syntax in the font name box, not through Word's Font dialog controls. | Medium |
| Equations | Math objects use a separate formula editor (Math) with StarMath syntax, not Word's inline OMML editor with UnicodeMath and LaTeX input; OMML converts with loss at the edges. | High |
| Fields | A large part of Word's field set (for example nested IF and COMPARE logic, STYLEREF variants, ASK and FILLIN prompts, EQ, BARCODE, ADVANCE) is missing or imported as text. Table formulas cover fewer functions. | Medium |
| Mail merge | Writer's merge uses its own database layer and wizard; Word rules fields (ASK, FILLIN, SKIPIF, NEXTIF) are not equivalent. | Medium |
| Track changes | Supports revisions, but move tracking, formatting change detail and compare and combine semantics differ; one reviewer reported full revision support while forum reports describe round trip problems. | Medium |
| Highlight color | Character highlighting and shading are handled differently, and a bug report states highlighting is converted to background color on export. | Medium |
| Master documents | Present but less complete than Word's subdocument workflow. | Medium |
| Building blocks | AutoText exists, but Word's building block galleries and glossary parts do not round trip fully. | Medium |
| Macros | VBA is interpreted for a subset of the object model; Word's object model is much larger. | High |
| ActiveX and legacy form fields | Limited import. | Medium |
| Digital signatures | OOXML signatures can be verified and added in newer releases; Word's signature line object is not equivalent. | Low |
| IRM and sensitivity labels | Not supported. | High |
| AI features | No equivalent of Copilot, Dictate or Editor AI refinements built in; the grammar check uses LanguageTool style extensions or the built in Hunspell spelling only. | High |
| Accessibility checker | Present in recent versions with fewer checks than Word's. | Medium |
| Immersive Reader, Read Aloud | Not present as built in features. | Medium |
| East Asian and RTL layout | Supported, but document grid, ruby layout and some kinsoku details differ from Word. | Low |
| Docx fidelity at save | Files edited and saved in Writer can lose Word specific constructs (extension elements, some content controls, SmartArt, ink, 3D models) because Writer does not keep unknown XML. Forum reports describe documents that Word refuses to open after a Writer save; these are mostly old and bug dependent. | Medium |

Practical reading: Writer reaches the core, common and most niche layout and formatting features, and its main shortfalls against Word are exact page layout equivalence on .docx round trips, the breadth of fields, modern collaboration, equation editing, and preservation of unknown OOXML content. A clone that claims full major feature parity with Word should be judged first on layout match for typical business documents, then on the completeness of tables, styles, lists, fields, tracked changes and comments, then on the round trip rules in section 23.4.
