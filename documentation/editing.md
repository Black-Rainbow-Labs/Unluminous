# Editing

What Unluminous does with text, whatever the text is. [Writing code in it](writing-code.md) is the
half that needs a language behind it.

## The ordinary things

Select with the mouse or with shift and an arrow key. Cut, copy and paste. Move the caret by
character, by word, to the start or end of a line, and to the start or end of the document. Undo and
redo.

**Undo restores a state rather than applying an inverse.** A snapshot holds the text, both kinds of
formatting, the selection and the marks, and undoing swaps one in. An inverse operation for every
command would be a second implementation of the editor, and the first one that is subtly wrong leaves
a document nobody can explain.

The saved point is a **history revision** rather than a one-way dirty flag: every persisted state has
an identity that rides its snapshot, a successful save records the current one, and `modified` is
whether the two differ. So undoing back to the saved state clears the dirty marker, and a new branch
can never reuse the identity of a saved state stranded in discarded redo history. Saving also closes a
run of typing, or the next letter would merge across the saved point and leave no exact snapshot to
return to.

## Formatting prose

Character formatting — bold, italic, underline, strikethrough and colour — and paragraph formatting —
left, centre, right and justified alignment, and single, one and a half or double line spacing — are
behind the `F` button at the right of the title bar. `Cmd/Ctrl` with `B`, `I` or `U` are on the
keyboard, `Cmd/Ctrl+Shift+X` is strikethrough, and `Cmd/Ctrl` with `L`, `E`, `R` or `J` are the four
alignments.

They are drawn for prose and **absent** for a source file or a picture. Unluminous saves plain text and
carries no formatting to disk, so for a `.rs` file every one of those controls is a decoration that
lasts until the file is reopened.

## Files

Any file holding text opens, whether Unluminous knows the type or not.

| | |
|---|---|
| `.md` | Markdown: the preview shows it rendered |
| `.mmd`, `.mermaid` | a Mermaid diagram: the preview **draws** it |
| a picture | `.png`, `.jpg`, `.gif`, `.bmp`, `.ico`, `.webp`, `.tiff`, in a tab that shows it |
| anything else | plain text, coloured by the plugin that claims the extension if one does |

A file that is neither text nor a picture is listed in the explorer, dimmed, and says why it cannot be
opened when the pointer rests on it. So is a file larger than 16 MB.

A picture is scaled to fit the editing area to begin with, zoomed with the keyboard, the wheel with
the modifier held, or a pinch, dragged about with the mouse, and put back to filling the area with a
double click.

### A file is written back the way it was read

What a file **was** is kept beside its text: a line ending — `LF`, `CRLF`, or a lone `CR`, which
nothing has written since 2001 and which is kept rather than converted — and an encoding.

The reason is a measurement. A three line file with Windows line breaks, opened, one character typed,
saved:

```
before:  l i n e   o n e \r \n l i n e   t w o \r \n
after:   X l i n e   o n e \n   l i n e   t w o \n
```

Every line ending rewritten by a one character edit. On a machine whose `core.autocrlf` is set, a git
checkout is enough to put every file in that state, so **any edit to any file produced a whole file
diff**. The normalisation itself is right — offsets and line counts need one meaning, and a breakpoint
sent to a debugger from a file read raw landed about fifty bytes early — and the fault was only that
the write did not undo it.

Three rules follow:

- **Whichever ending there are most of wins**, and a file with none gets the platform's. A file is
  very often mixed, and counting is the reading that changes the fewest lines.
- **A file that is not UTF-8 opens read-only** rather than being refused. A UTF-16 byte order mark and
  Latin-1 — which cannot fail, because every byte is a code point there — are read, named in the
  status bar, and never written back. Re-encoding somebody's file into a scheme this version has not
  been asked to get right is worse than saying no.
- **`editor.line_ending` is `keep`** and should stay that way. Any other default rewrites somebody's
  file the first time they type in it.

### A file that moves takes the code that names it with it

Drag a row onto a folder, or use `Rename...`, and the file moves and every import, `use` line and
`mod` declaration in the project that named it is rewritten. A rename **is** a move to a new name, so
both go through one function.

One rule decides every case: *work out what the written text will mean after the move; if that is not
what it means now, rewrite it, and if it is, leave it exactly as it is.* That is what makes moving a
whole folder cheap — every specifier inside it still points where it did — and what keeps a
`super::sibling` in a moved Rust module untouched while rewriting one in a file that stayed behind.

Rust's `mod` declarations are the part that actually breaks the build, because a file is not a module
because of where it sits but because some other file says `mod name;`. The declaration is taken out of
the old parent module file with the attributes and doc comments that belong to it, and put into the
new one in alphabetical order with the same visibility — and when the destination folder has no module
file at all, that is a **note** rather than a guess, because making one is a decision about the shape
of somebody's crate.

There is no preview modal, and that is not the same decision rename-a-symbol made: a name is ambiguous
and a path is not. A specifier resolves to one file or to no file, and one that resolves to no file is
left alone, so there is nothing to disambiguate. And the move is its own inverse — drag it back and
every specifier comes back exactly as it was. `unluminous-cli explorer move --dry-run` prints the whole
change set and touches nothing.

## Finding and replacing

`Ctrl/Cmd+F` is a **bar** rather than a modal, and that is the one design decision worth writing down:
`Go to File` and `Find in Files` are modals because they are about the project and their answer is a
list you read; find in the current file is about the text you are looking at, and a modal over it
would cover the thing being searched.

It has a match count, next and previous, a case toggle and a whole-word toggle. `Ctrl/Cmd+H` opens
Replace and Replace All beside it, and **Replace All is one undo step** whether it changes one match
or four hundred.

`Ctrl/Cmd+Shift+F` searches the project, on a thread, and can replace across everything it found —
through the modal that already lists every file it would write, because a replacement across a project
is a change somebody should see the size of first. An open tab is edited as a document and left with
unsaved changes rather than written behind you; a closed file is read, checked that its matches are
still the ones that were listed, and written once, with every byte outside the replaced ranges
untouched.

`Ctrl/Cmd+Shift+O` is `Go to File`, which narrows the project's files as a name is typed — a
**subsequence**, so `mdrs` finds `markdown.rs`, with a match in the name outranking one in the folders
above it. `Ctrl/Cmd+Shift+A` is `Find Action`, the same idea over every menu entry, ranked by the same
scorer.

**What a build wrote is not searched.** `target`, `node_modules` and `__pycache__` are left out of the
walk that the filter box, `Go to File` and `Find in Files` all read. Three names only, and each is a
folder nobody writes a source file into — `build`, `dist` and `out` are deliberately not among them,
because a search that silently missed a real file would be worse than one offering a few too many.
Measured on this repository, it took the list from 2,022 files to 618 and the whole search from 60 ms
to 20.

## Highlighting a passage

Select some words, right click, and choose one of four colours or open the colour wheel. The colour is
behind those words in this file, next time it is opened, and until it is cleared, and it moves with
the text as the file is edited.

**Marking is not an edit**: it pushes nothing onto the undo history and does not mark the file as
having unsaved changes, which is the rule the editor's font already follows. The ranges live inside
the document, so the two functions that know a range of bytes moved shift them in the same two lines
that already shift everything else.

One rule decides every awkward case: *a file that is open is owned by its document, and every other
file is owned by the store.* The project's own `.unluminous/highlights.txt` holds the rest, one file
for the whole project rather than one per source file, because six hundred source files would be six
hundred files to open when a project opens.

`unluminous-cli highlight apply` takes a JSON array, so twenty passages across twenty files are one
request and none of the files has to be opened.

## Folding

A block that spans lines can be collapsed from the arrow beside its line number, and **the line
numbers stay correct**. That last one decides the design: every offset in Unluminous is a byte offset
into the real file — the caret, the selection, the marks, the syntax spans, the search hits, the
definitions index — so laying out a second document holding only the visible lines would need every
one of them translated at every seam. What is laid out is the real document with some of its
paragraphs left out, so the numbers are unchanged by construction rather than by arithmetic.

What is foldable is derived from the file: brackets that span lines, block comments, runs of line
comments, indentation, and Markdown headings. Which of them are collapsed is state, kept in the
document as byte offsets so it survives an edit, and folding is **not an edit** — no undo step, no
unsaved-changes flag.

`Ctrl/Cmd+.` toggles, `Ctrl/Cmd+Shift+.` collapses all, `Ctrl/Cmd+Shift+,` expands all, and
`Ctrl/Cmd+Alt+.` is `Collapse All But Highlighted`: collapse everything, then keep open every region
that holds a marked passage and every region holding one of those, since a marked line inside a method
inside a class is only visible if both are open. With nothing marked it falls back to the selection,
and with neither it says so rather than collapsing the whole file.

The `…` badge after a collapsed head line is **painted over the text rather than put into it**, which
is the one place the rule that everything on the screen is real text is deliberately not followed:
three characters in the layout that are not in the file would have to be hidden from the caret, the
selection, the clipboard and every byte offset that crosses them. So the badge does not select, and
**copying across a fold copies the hidden text**, which is what the reference editor does and what a person means.

## The Markdown preview

Not a second renderer. `markdown::render` reads the source and produces the same three things a
document holds — text, character spans, one paragraph setting a line — plus a fourth saying which line
of the source each line of the preview came from. The ordinary layout and the ordinary painter draw
it, so nothing in the window knows how to render Markdown, selecting text in it was a small feature,
and scrolling either half of the side by side view can cross to the other **through the text** rather
than through the height.

That last one matters more than it sounds. A heading is one line of source and half again as tall on
the page, a fence's backticks are two lines of source and nothing at all, and a picture is one line of
source and four hundred points of page. Measured on a plain sixty-section document with none of that
in it, the two pages already differ by thirteen per cent.

Two phases, which is what every conforming implementation uses. **Blocks** builds a tree, recursively:
a quote's lines have one `>` taken off them and are parsed again, a list item's lines have its indent
taken off them and are parsed again. **Inline** is the delimiter stack: a run of `*`, `_` or `~` is
measured, asked whether it may open and whether it may close under the flanking rules, and matched
against the runs still open behind it — which is why `2 * 3 * 4` is left alone.

**A table is set in the code font and drawn in a box.** Every cell is padded with spaces to its
column's width, so the columns line up by construction rather than by measurement, the arithmetic is
integers over characters, and the whole table is ordinary text — so it selects, copies and hit-tests
with no new code, and what lands on the clipboard is a table a person can paste anywhere. The rules
are **drawn** rather than lettered, because a glyph cannot tile: set as letters, a table's rules came
out dotted and its columns came out as rows of ticks.

**Code is coloured by the plugin that reads the language.** A fence's word is matched against a
plugin's id, its name and every extension it claims, so ```` ```rs ```` and ```` ```rust ```` are one
question and Unluminous holds no table of aliases.

**A picture is drawn when it is the whole of a line and it is a file on this machine.** One inside a
line of prose stays its alt text, because it would need inline layout the engine has not got, and one
with a scheme in front of it is refused, because Unluminous makes no network requests.

## Diagrams

A `.mmd` file gets the same three view modes a Markdown file has, and a ` ```mermaid ` block inside a
Markdown document is drawn in its preview. **Twenty** of Mermaid's thirty diagram types are drawn —
flowchart, sequence, class, state, entity relationship, requirement, pie, gantt, user journey, git
graph, mindmap, timeline, quadrant, xy chart, sankey, block, packet, kanban, radar and treemap — and
the other **ten** are named rather than mis-drawn, which is a distinction with a test of its own.

None of it runs `mermaid.js`. Three ways of doing that were weighed: `mermaid-cli` needs Node and a
headless Chromium; embedding a JavaScript engine means implementing enough DOM and SVG to answer
`getBBox()` truthfully; and a web view puts a second compositor inside a window whose transparency
took three separate fixes to get right. What a diagram needs on top of what Unluminous already has is
arithmetic, so `unluminous_core::mermaid` does the arithmetic. The cost is stated rather than hidden:
the pictures are **not** pixel identical to `mermaid.js`, and the bar they are held to is correct and
readable.

The seam is a **scene**: five kinds of item — rectangles, circles, polygons, lines and text at
absolute positions — and nothing else. An arrowhead is a filled polygon of three points, a pie slice
is a flattened arc, a crow's foot is three lines, all built where they can be tested with no window.
So the component that draws a diagram has no diagram knowledge in it at all, and a twenty-first type
needs no change there.

**Every sweep count is a constant and nothing is random**, so the same source always gives the same
picture, which is what makes a screenshot test of a diagram possible. A subgraph is laid out on its
own and placed as one box, recursively, so its contents cannot overlap anything outside it.

A diagram's own `style`, `classDef` and `click` directives are **read and ignored**: a document does
not get to choose the window's colours, and nothing in a diagram is going to run. **Nothing is
fetched**, ever.

`cargo run --example mermaid_check` lays out every file in `sample-diagrams/` and says what came of
each, which is the quickest way to see that a layout change has broken nothing.
