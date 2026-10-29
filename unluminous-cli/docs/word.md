# The Page plugin's commands (`plugins run word`)

These commands are run as `unluminous-cli plugins run word <verb> [arguments]`, and through the `unluminous_word` MCP tool. They act on the current document, which is the one last drawn or the one chosen with `use`. Paragraphs are numbered from 1 in document order, table cells included, as `read` lists them. Every command answers a sentence and, where there is data, the same answer as JSON.

### new

Makes a new document and opens it in a tab. With a path, the document is named for that path. `--template <file>` starts it from a template file. With no template it starts from Word's own Normal.dotm when this machine has one. It answers the tab key of the new document.

```
unluminous-cli plugins run word new notes.docx --template letter.dotx
```

### open

Opens a file in a tab. A relative path is read against the project folder. A .doc, .rtf, .odt, .wps or .pdf is converted by Word when Word is installed. With `--as-document`, a .txt, .md or .html file opens as a Word document. It answers the tab key.

```
unluminous-cli plugins run word open --as-document README.md
```

### docs

Lists the open documents. Each entry has its key, path, whether it was converted from another format, whether it is encrypted, whether it has unsaved changes, its page count and whether it is the current one. A tab still waiting for Word or for a password is listed with what it waits for.

```
unluminous-cli plugins run word docs
```

### use

Makes another open document the current one. The argument is a tab key, a key without its `word:` prefix, or the end of a document's path. It is refused when no open document matches.

```
unluminous-cli plugins run word use report.docx
```

### save

Saves the current document. With a path, it is Save As and the tab is renamed. A .rtf or .odt is written by Word. A .txt, .html, .md or .pdf is written as a copy, and the document keeps its own file. It answers the path that was written.

```
unluminous-cli plugins run word save C:/work/report-final.docx
```

### open-as-document

Opens a file as a document. With no argument it shows a file dialog. With a path it behaves as `open --as-document` on that path.

```
unluminous-cli plugins run word open-as-document notes.txt
```

### password

Opens an encrypted document that is waiting in its tab. The password is every word after the verb. It applies to the current document if that one is waiting, and otherwise to the first one waiting. It is refused when no document is waiting.

```
unluminous-cli plugins run word password correct horse battery
```

### unlock

Makes a document editable when it opened read only because it has a password to modify. The password is every word after the verb. A wrong password is refused.

```
unluminous-cli plugins run word unlock secret
```

### protect

With no argument it answers what protects the document: whether it is encrypted, whether it has a password to modify, whether it is read only and which editing restriction applies. `password <p>` encrypts the document when it is saved, and `password none` removes that. `modify <p>` sets the password to modify, and `modify none` removes it. `restrict readOnly|comments|trackedChanges|forms <p>` restricts editing with a password. `stop <p>` ends the restriction.

```
unluminous-cli plugins run word protect restrict comments secret
```

### recover

Handles the AutoRecover copy that is offered when a document opens after a crash. `recover restore` loads the recovered copy, and the document must then be saved to keep it. `recover discard` keeps the file and drops the copy. With no argument it says whether a copy is offered.

```
unluminous-cli plugins run word recover restore
```

### autosave

Turns AutoSave on or off. When on, a document that has a file is saved five seconds after its last change. With no argument it answers the current state. It is off unless switched on.

```
unluminous-cli plugins run word autosave on
```

### autorecover

Turns AutoRecover on or off. When on, a copy of a changed document is kept every ten minutes and offered after a crash. With no argument it answers the current state. It is on unless switched off.

```
unluminous-cli plugins run word autorecover off
```

### close

Closes the tab of the current document. The tab saves the document as it closes.

```
unluminous-cli plugins run word close
```

### read

Lists paragraphs with their style, list, cell and text. `read` alone lists all of them. `read 5` lists paragraph 5. `read 5 9` lists paragraphs 5 to 9. `--runs` adds the text runs of each paragraph. `--story body|header <n>|footer <n>|footnote <id>|endnote <id>|comment <id>|textbox <n>` reads a story other than the body, with its own numbering from 1.

```
unluminous-cli plugins run word read 1 20 --runs
```

### locate

Answers where a place is drawn in the window, in the window's points, and the page it is on. The arguments are a paragraph number and an optional character offset. `--story <kind> [n]` names a story other than the body. It is refused when the place is not on a page yet. Use the result as the position of a press.

```
unluminous-cli plugins run word locate 12 5
```

### stories

Lists every story that can be read: the body, each header and footer, each footnote and endnote, each comment and each text box. Each entry has the name and number to give to `--story`.

```
unluminous-cli plugins run word stories
```

### text

Answers the whole document as plain text, with the number of characters in the sentence.

```
unluminous-cli plugins run word text
```

### outline

With no argument it lists the headings with their level, text and page. With an argument it changes the outline at the selection: `promote`, `demote`, `move-up`, `move-down`, `expand`, `collapse`, `heading-1` or `body`. `level <n>` sets how many heading levels the outline view shows, from 1 to 10.

```
unluminous-cli plugins run word outline demote
```

### stats

Answers the page count and the counts of words, characters, characters with spaces, paragraphs and lines.

```
unluminous-cli plugins run word stats
```

### spelling

With no argument it lists every misspelled or repeated word with its story, paragraph, offset and up to five suggestions. `spelling add <word>` adds a word to the dictionary. `spelling ignore <word>` stops underlining it for the session. `spelling next` selects the next underlined word after the caret.

```
unluminous-cli plugins run word spelling add Unluminous
```

### pages

Lists each page with its number, size, the paragraph it starts in and the first 60 characters it starts with.

```
unluminous-cli plugins run word pages
```

### select

Sets the selection. `select 4` selects paragraph 4. `select 4 10` puts the caret at offset 10 of paragraph 4. `select 4 10 6 3` selects from offset 10 of paragraph 4 to offset 3 of paragraph 6. `select all` selects everything. `select text <words>` selects the first match. `select --story <kind> [n] <paragraph> [offset] [to-offset]` selects in another story. It answers the selected text.

```
unluminous-cli plugins run word select text quarterly results
```

### selection

Answers the selection and its text. It does not change the selection.

```
unluminous-cli plugins run word selection
```

### type

Types the text at the selection, replacing a selected range. The text is every word after the verb. The two characters `\n` start a new paragraph and `\t` types a tab.

```
unluminous-cli plugins run word type Dear Sam,\n\nThank you.
```

### insert

Inserts something at the caret. The kinds are `paragraph`, `page-break`, `line-break`, `column-break`, `section-break [continuous|even|odd]`, `table [rows] [columns]` (3 by 3 by default), `picture <path> [width]`, `textbox [width height]`, `shape <preset> [width height]`, `footnote <text>`, `endnote <text>`, `toc [levels]`, `caption <label> <text>`, `field <code>`, `link <address> [text]`, `bookmark <name>`, `date`, `page-number` and `symbol <character>`. Widths and heights are in points.

```
unluminous-cli plugins run word insert table 4 3
```

### delete

Deletes the selection, or one character before the caret. `forward` deletes after the caret instead. `word` deletes a word instead of a character.

```
unluminous-cli plugins run word delete forward word
```

### find

Finds text and answers each match with its paragraph, offset and length, and selects the first. Options are `--case`, `--word` and `--wildcards`. `--all-stories` searches headers, notes and the other stories. `--format=` limits matches to text with the formatting named, a comma list of `bold`, `italic`, `underline`, `highlight`, `not-bold` and the others like it, `color:RRGGBB`, `style:<id>` and `align:<alignment>`. The codes `^p`, `^t`, `^#`, `^$` and `^?` work as they do in Word.

```
unluminous-cli plugins run word find invoice --case --format=bold
```

### replace

Replaces text. The words before `--` are what to find and the words after are the replacement. Without `--all` only the first match is replaced. Options are `--case`, `--word`, `--wildcards` (with `\1` groups in the replacement), `--all-stories`, `--format=` to limit the matches and `--with-format=` to format the replacement. It answers the number replaced.

```
unluminous-cli plugins run word replace colour -- color --all
```

### goto

Moves the caret to a place and scrolls to it. The first argument is `page`, `section`, `line`, `bookmark`, `footnote`, `endnote`, `comment`, `heading`, `table` or `picture`, and the rest is the number or name. It answers the path and offset reached.

```
unluminous-cli plugins run word goto heading Introduction
```

### format

Formats the selection. `bold`, `italic`, `strike`, `double-strike`, `small-caps`, `caps` and `hidden` toggle, or take `on` or `off`. `underline [style|off]` sets an underline style. `superscript` and `subscript` take `on` or `off`. `font <name>`, `size <pt>`, `color <RRGGBB|auto>`, `highlight <name|none>` and `spacing <pt>` set a value. `clear` removes character formatting.

```
unluminous-cli plugins run word format size 14
```

### paragraph

`paragraph info` answers how the paragraph at the caret looks: style, alignment, indents, spacing, line spacing, keep settings, outline level and tab stops. The setting forms are `align left|center|right|justify`, `indent-left <pt>`, `indent-right <pt>`, `first-line <pt>`, `hanging <pt>`, `before <pt>`, `after <pt>`, `line <multiple>` (such as 1.5), `line exact <pt>`, `line at-least <pt>`, `keep-next`, `keep-lines`, `page-break-before` and `widow-control` (each taking `on` or `off`, on by default), and `shading <RRGGBB>` (no valid color removes shading).

```
unluminous-cli plugins run word paragraph line 1.5
```

### style

Applies a style by name or id to the selection, for example `style Heading 1`. `style list [paragraph|character|table]` lists styles. `style modify <style> <property> <value>` changes a style. `style new <name>` makes a style from the selection. `style update` changes the selection's style to match the selection. `style delete <style>` deletes a style. `style revert <style>` restores a built in style to Word's own definition.

```
unluminous-cli plugins run word style modify Heading1 color 1F3864
```

### list

Changes list formatting at the selection. `bullet` and `number` toggle a list. `deeper` and `shallower` change the level. `restart [n]` restarts numbering at n (1 by default). `continue` continues the previous list. `value <n>` sets the number of the paragraph. `symbol <character> [font]` sets the bullet. `format <numFmt> [before] [after]` sets the number format, such as `upperRoman`. `indents <pt> [hanging]` sets the indents. `define <format:text>...` defines a multilevel list, one word per level, such as `decimal:%1.`.

```
unluminous-cli plugins run word list define decimal:%1. lowerLetter:%1.%2
```

### picture

Reads or changes the picture or shape the selection is on. `info` is the default and answers kind, size, wrap, position, alt text and crop. `select <paragraph> <offset>` selects a picture. `size <w> [h]` sets the size in points. `wrap <kind>` sets text wrapping. `position <relative> <x> <y>` sets where it sits. `crop <left> <top> <right> <bottom>` crops by percent. `alt <text>` sets alt text. `reset` restores the original size and crop. `order forward|backward|front|back` changes the stacking order. The verb `object` does the same thing.

```
unluminous-cli plugins run word picture size 200
```

### print

Prints the document. The optional first argument is a page range such as `1-3,5`. Options are `--copies <n>`, `--odd`, `--even` and `--per-sheet 2|4`. With `--to <file>` the sheets are written as a PDF to that file and nothing is printed.

```
unluminous-cli plugins run word print 1-3 --to C:/work/draft.pdf
```

### thesaurus

Lists synonyms grouped by part of speech. The argument is the word. With no argument it uses the selected text.

```
unluminous-cli plugins run word thesaurus quick
```

### table

Edits the table at the caret. Operations that take arguments: `width <pt>` for the caret's column, `width <column> <pt>` for a named column (the next column gives or takes the difference), `cell <row> <col>` to move the caret to a cell, `select cell|row|column|table`, `merge`, `split-table`, `convert to-text|to-table tab|comma|paragraph`, `sort <col> [text|number|date] [--descending] [--header]`, `distribute rows|columns`, `properties <name> <values>`, `style <id> [htflrc]`, `shade <RRGGBB>` and `borders all|none|outside|inside|top|bottom|left|right [pt] [RRGGBB]`. Any other word, such as `row-below`, `column-right`, `delete-row`, `merge-right`, `merge-down` or `split`, is run as a table operation.

```
unluminous-cli plugins run word table sort 2 number --descending
```

### section

Changes the section at the caret. `size <name>` sets the page size, such as `a4`. `orientation landscape` sets landscape and any other value sets portrait. `margins <preset>` takes `normal`, `narrow`, `moderate` or `wide`, and `margins <top> <bottom> <left> <right>` takes points. `columns <n>` sets the column count. `header <text>`, `footer <text>`, and the forms with `-first` or `-even` set header and footer text. `page-numbers` sets the footer to "Page {PAGE} of {NUMPAGES}".

```
unluminous-cli plugins run word section margins narrow
```

### comment

Works with comments. `add <text>` comments on the selection and answers the new id. `reply <id> <text>`, `edit <id> <text>`, `resolve <id>`, `reopen <id>` and `delete <id>` act on a comment by id. `delete all` removes every comment. `next` and `previous` move to a comment. `list` is the default and lists comments with their ids.

```
unluminous-cli plugins run word comment add Check this figure
```

### track

Controls Track Changes. `on` and `off` switch tracking, and `off` is refused while tracking is locked. `list` answers the tracked changes with paragraph, kind, author and text, and is the default. `accept [all]` and `reject [all]` resolve the change at the selection, or all of them. `view simple|all|none|original` sets the markup shown. `next` and `previous` move between changes. `lock <password>` locks tracking on and `unlock <password>` unlocks it.

```
unluminous-cli plugins run word track accept all
```

### language

Sets the proofing language of the selection from a language tag such as `en-US` or `fr-FR`.

```
unluminous-cli plugins run word language fr-FR
```

### accessibility

Runs the accessibility checker and lists each issue with its kind, message and position. The checks include pictures with no alt text, skipped heading levels, tables with no header row and low contrast text.

```
unluminous-cli plugins run word accessibility
```

### inspect

The Document Inspector. With no argument it counts comments, tracked changes, hidden runs and personal names, and lists the personal properties. `inspect remove comments|revisions|hidden|personal` removes that kind and answers how many were removed.

```
unluminous-cli plugins run word inspect remove personal
```

### fields

Works with fields. `fields` or `fields update` updates every field and table of contents. `unlink` replaces fields in the selection with their results. `lock` and `unlock` lock or unlock fields in the selection. `list` lists each field with its paragraph, code and result.

```
unluminous-cli plugins run word fields update
```

### toc

Works with tables of contents. `insert [levels]` adds one at the caret and uses 3 levels by default. Options are `--figures <label>` for a table of figures, `--no-pages`, `--no-links` and `--leader dot|hyphen|underscore|none`. `update` refreshes every table. `remove` removes the table of contents and is refused if there is none.

```
unluminous-cli plugins run word toc insert 2 --no-links
```

### autocorrect

With no argument it lists the autocorrect rules and whether each is on. `autocorrect <rule> on|off` switches one rule for every open document and saves the choice in the settings. An unknown rule is refused with the list of rules.

```
unluminous-cli plugins run word autocorrect
```

### caption

Adds a numbered caption. The first argument is the label, which is `Figure` by default, and the words after it are the caption text. The caption goes below the paragraph at the caret, or above it with `--above`.

```
unluminous-cli plugins run word caption Table Quarterly sales --above
```

### xref

Inserts a cross reference at the caret to a paragraph. The first argument is the paragraph number and it is required. The second chooses what is inserted and is `text` by default. The choices are `text`, `page`, `number` and `above-below`. The verb `cross-reference` does the same thing.

```
unluminous-cli plugins run word xref 7 page
```

### paste

Pastes formatted text, or changes the last paste. `--as source|merge|text` changes how the last paste was applied. `--html <html>` pastes the HTML that follows, and `--rtf <rtf>` pastes the RTF that follows.

```
unluminous-cli plugins run word paste --as text
```

### link

Works with the hyperlink at the caret. `show` answers its address and tooltip and is the default. `open` follows the link. `edit <address> [text]` changes the address and optionally the text. `tip <text>` changes the tooltip. `remove` removes the link. All of them are refused when the caret is not in a link.

```
unluminous-cli plugins run word link edit https://example.com Example
```

### undo

Undoes the last change. It answers `Nothing to undo` when there is nothing.

```
unluminous-cli plugins run word undo
```

### redo

Redoes the last undone change. It answers `Nothing to redo` when there is nothing.

```
unluminous-cli plugins run word redo
```

### export

Writes a copy of the document as `pdf`, `html`, `md` or `txt`. A path may follow, and without one the copy goes beside the document, which must then have been saved. A PDF is tagged, has the headings as its outline and carries the document information. `--no-tags` and `--no-outline` leave those out, and `--pdfa` writes PDF/A-2b.

```
unluminous-cli plugins run word export pdf C:/work/report.pdf --pdfa
```

### properties

With no argument it answers the Summary properties: title, subject, author, keywords, comments, category, status, manager and company. `properties <name> <value>` sets one. `properties statistics` answers dates, revision, editing minutes and the page, word, character, line and paragraph counts. `properties custom` lists custom properties. `properties custom <name> <kind> <value>` sets one, where the kind is `text`, `number`, `date` or `yes-or-no`. `properties custom remove <name>` removes one.

```
unluminous-cli plugins run word properties custom Project text Atlas
```

### view

Changes how the document is shown, and answers the current view settings. The forms are `zoom <percent>`, `marks [on|off]`, `navigation [on|off]`, `thumbnails`, `ruler [on|off]`, `gridlines [on|off]`, `field-shading always|selected|never`, `multiple [on|off]`, `read`, `outline`, `print`, `fit-width`, `fit-page`, `new-window` and `paper light|theme`. A setting given with no `on` or `off` is toggled.

```
unluminous-cli plugins run word view zoom 125
```

### panel

Opens the format panel at a level: `text`, `paragraph`, `page`, `review` or the root. `panel close` closes it, and so does `panel` with no argument. It answers the levels now open.

```
unluminous-cli plugins run word panel paragraph
```

### action

Runs a command from the Ctrl+/ list by its id. `action list` and `action` with no argument list every id with its label and level. An unknown id is refused.

```
unluminous-cli plugins run word action list
```

### load

Reads a file into a tab key. The window sends this when it opens a tab, and an agent rarely needs it. The first argument is the tab key and the rest is the path.

```
unluminous-cli plugins run word load word:C:/work/report.docx C:/work/report.docx
```
