# How plugins work

## A plugin is data, and nothing in one is executed

A plugin is a folder holding a manifest and an icon, read by the same value store the settings file
uses. Loading one is reading a file. **Nothing in one is executed**, so installing one is copying a
folder.

The two alternatives were weighed and both are the right answer to a question this is not asking yet.
A **dynamic library** would let a plugin run arbitrary Rust, and it also means an unstable interface
across a library boundary — a Rust structure passed over one is undefined behaviour unless both sides
were built by the same compiler with the same flags — so every plugin would have to be rebuilt for
every release of Unluminous, and a plugin that crashes would take the editor with it. **WebAssembly**
answers both of those and costs a runtime plus a host interface that has to be designed, versioned and
documented before the first plugin can be written. For "colour these keywords", both are a great deal
of risk bought for nothing.

So the seam is named, and widened in the open. `plugin.kind` is read and checked, and a manifest naming
a kind this version does not run is **refused with a message** rather than half-loaded. It has three
values:

| | |
|---|---|
| `language` | describes a file type: the extensions it claims, the words worth colouring, what a comment and a string look like, and a colour for each kind of token |
| `ui` | contributes a pane, a tab, a menu and a Settings page that are **drawn by code which shipped in the binary** |
| `theme` | says what every colour in Unluminous's own palette means |

Each of the two additions came with a check and a test rather than quietly, which is what the field
is for. It is still the line a later version widens again, the day a plugin wants to *do* something:
run a formatter, talk to a language server, add a tool window.

## What ships

Sixteen plugins are bundled inside the binary, so an Unluminous that has just been installed colours a
`.rs` file the first time it opens one, and so that the marketplace has something in it with no
network involved.

| kind | |
|---|---|
| `language` (twelve) | JavaScript, TypeScript, Rust, Python, CSS, HTML, JSON, YAML, TOML, SQL, shell, Mermaid |
| `ui` (three) | Agent-Chat, Agent-Tasks, Database |
| `theme` (one) | Themes Bundle 1, carrying five |

## Where they come from, and in what order

The bundled ones are read first, then every folder under `<settings folder>/plugins`. **A plugin on
disk shadows a bundled one with the same id**, so a bundled plugin can be corrected by hand without
rebuilding Unluminous. Then the disabled list in the settings file switches off the ones that were
switched off last time.

A plugin that will not parse is **skipped and its reason kept** rather than thrown away, and
Unluminous starts with one plugin fewer. That is the rule the settings file already keeps: starting
with a default is better than refusing to start because a file has a stray line in it.

`Plugins::for_path` is the whole of "which plugin claims this file" — the first one that is switched
on and lists the extension. Nothing else asks the question.

`Settings -> Plugins` is the marketplace and the list of what is installed. Switching one off takes
effect at once: the files it claims lose their colours and their icon on the next frame. `Install`
writes a bundled plugin's folder out to `<settings folder>/plugins/<id>/` and then **reads it back
from disk**, which is what proves the loader works on real files and not only on what was baked into
the binary.

## The manifest

`plugin.conf`, in the same `name = value` format the settings file uses, read by the same value store.
No new dependency, and a plugin can be read and corrected in a text editor, which is fitting in a text
editor. A list is comma separated, a flag is `true` or `false`, and a colour is `#RRGGBB`.

### Every kind

| Key | What it is |
|---|---|
| `plugin.id` | the name of its folder, and how it is switched off. Required. |
| `plugin.kind` | `language`, `ui` or `theme`. Anything else is refused with the list. |
| `plugin.name`, `.version`, `.vendor`, `.description` | what `Settings -> Plugins` shows |
| `plugin.limitations` | what it does not do. Every bundled plugin has one, and it answers why a regular expression is coloured as division before anybody has to ask. |

### A language

| Key | What it is |
|---|---|
| `language.extensions` | the extensions it claims, with or without the dot. Empty is refused, because nothing would ever use it. |
| `language.line_comment` | what starts a comment that runs to the end of the line |
| `language.block_comment` | the opener and the terminator, as two values |
| `language.strings` | the quote characters that open a string. `", '` unless it says otherwise. |
| `language.escapes` | whether a backslash escapes the next character inside a string. On unless it says otherwise. |
| `language.numbers` | whether a run of digits is a number. On unless it says otherwise. |
| `language.operators` | the characters that are operators |
| `language.keywords` | the words the language reserves |
| `language.builtins` | the names the language provides |
| `language.types` | a third list of words, tried after the other two |
| `language.word_characters` | characters that are part of a word wherever they appear, such as the hyphen in CSS |
| `language.hex_colors` | whether `#` and hexadecimal digits are a number |
| `language.markup` | whether the file is markup — text with tags in it rather than tags with text in them |
| `language.raw_text` | the elements of a markup language whose contents are not markup, as `element` or `element=language` |
| `language.definers` | `keyword=kind` pairs saying what defines a name — `fn=function, struct=type, let=variable` |
| `language.brace_definitions` | the one heuristic: a name followed by a brace defines something, for the method a class never puts a keyword in front of |
| `language.export_keyword` | what makes a definition importable — `export`, `pub` |
| `language.imports`, `.import_keywords`, `.import_extensions`, `.import_index`, `.import_omit_extension`, `.path_separator`, `.source_roots`, `.path_roots` | how an import is written in this language, for the completion inside one |
| `language.renders` | the built-in renderer this language's files are drawn with |
| `run.file`, `run.project` | how one file of this language runs, and which built-in project detector applies |
| `debug.adapter` | which built-in debugger this language's files use |
| `theme.name` | what the colour scheme is called |
| `theme.keyword`, `.builtin`, `.function`, `.type`, `.string`, `.number`, `.comment`, `.operator`, `.text` | one colour a token. A token with no colour is left as ordinary text. This is a language plugin's own scheme, and it is what colours its files until a **theme** that names all nine is chosen. |

### A pane

| Key | What it is |
|---|---|
| `ui.provider` | which code that shipped inside Unluminous draws it, checked against the providers this version has |
| `ui.chrome` | which renderer draws the depth behind it. Refused on a plugin that is not a `ui` plugin, because a renderer with no pane to draw is a line that would do nothing silently. |
| `pane.id`, `.label`, `.icon` | what it is called and what its rail button looks like |
| `pane.side`, `.order` | which edge it starts on and where in that edge |
| `pane.width`, `.height` | how large it starts, one for a column and one for a strip |
| `pane.group` | which half of the rail its button is in |
| `pane.tile` | whether it may share a strip. Defaults to `group == bottom`. |
| `pane.applies` | when it is offered at all |
| `tab.id`, `.label`, `.icon` | a tab in the editing area with no file behind it |
| `menu.name`, `.entries`, `.submenu.<id>` | a menu of its own in the bar, as `verb=Label` pairs with `-` for a separator |
| `settings.page`, `.icon` | a page of its own in the Settings window |

**`pane.group` and `pane.tile` are two questions**, and they were only the same question by
coincidence: the rail's bottom group holds the things with a character grid in them, and a thing with
a character grid in it is exactly a thing that must not be given half a strip. The board is neither,
so moving its button alone would have left it and the terminal each drawn half the width of the
window.

### A theme

One plugin carries several.

| Key | What it is |
|---|---|
| `themes` | the ids it carries, in the order Settings lists them. A group nothing lists is refused, and so is an empty line. |
| `theme.<id>.name` | what a person reads in the list. Required. |
| `theme.<id>.dark` | true unless it says otherwise. **False is refused**: this version draws dark themes only. |
| `theme.<id>.icons` | which drawn icon set the rail and the explorer use — `material` or `classic` |
| `theme.<id>.ui.<role>` | one colour a role, by the names in the palette. A role that is not named keeps Unluminous Dark's, and a role Unluminous has not got is refused with the list. |
| `theme.<id>.syntax.<token>` | the nine token colours, which then colour every language at once. **All nine or none**: eight would leave one line of code drawn in two schemes. |

## Four keys name something built into the binary

`language.renders`, `run.project`, `debug.adapter` and `ui.provider` each name a thing that shipped
inside Unluminous rather than describing a language. Each is **checked**, and a manifest naming one
this version does not have is refused with a message rather than loading as a language whose files
quietly never draw, are never noticed, or offer a Debug button that never works. The most a
third-party manifest can do is name something that shipped in the binary, visibly.

`ui.chrome` is the fifth of that shape, and `theme.<id>.icons` the sixth.

## What the tokeniser does with a grammar

One linear pass, no regular expressions, no dependency, and the order of the rules is the whole
design: a line comment, then a block comment, then a string, then a number, then a word in one of the
three lists, then a word directly followed by `(` as a function or one starting with a capital letter
as a type, then text. Comments and strings win over everything, because a keyword inside a string is
not a keyword.

Those last two are a **heuristic and are meant to be one**. `Promise.all(` colours `all` as a function
and `Promise` as a type without Unluminous understanding a single thing about JavaScript. Real
understanding is a language server, and that is not what this is.

**Nothing in the editor crate knows what a colour scheme is.** A token says what a stretch of text
*is*, and the window turns it into a colour: it runs the tokeniser, maps each token through the
plugin's theme, and applies the whole result in one pass rather than one pass per token — 561 ms to
1.4 ms on a coloured 170 kilobyte file. It is keyed on the document's **text** revision, so moving the
caret does not re-tokenise the file, and a file over two megabytes is left as plain text with a line
in the status bar saying so.

**A colour scheme colours the tokens and not the editing area.** Dracula's own background is not used
and Unluminous's stays: the window letting the desktop show through is the whole character of the
product, and a scheme that repainted the editing area opaque would trade that away to be a shade
nearer a screenshot.

**And the colour scheme moved off the language plugins.** Rust, JavaScript, TypeScript, CSS and HTML
each carried their own copy of Dracula — five copies, and a sixth language would have arrived with a
sixth. A theme's nine are used if it names them and the plugin's own otherwise, and **Unluminous Dark
names none**, so an Unluminous nobody has chosen a theme in colours every file exactly as it did
before.

### Three keys that arrived with CSS

`word_characters`, `types` and `hex_colors` are **off unless a manifest names them**, so no plugin
written before they existed changes by a pixel. All three arrived with the CSS plugin, which none of
the tokeniser's rules could read: **a hyphen is a letter in CSS**, and a pass that split a word there
could not name a single property; `#ff0000` is a colour and the number rule wants a digit first, so
half the colours in a stylesheet were coloured and half were not; and a stylesheet has **three** kinds
of word worth telling apart — the at-rule, the property and the value — where a grammar had two lists.

Which word goes in which list is the whole of a plugin's design, and one rule decides the awkward
cases: **a word that is both a property and a value is coloured whichever way it is written more
often.** In the CSS plugin `inset`, `left` and `content` are properties; `flex`, `grid` and `all` are
values, because `display: flex` and `transition: all` are far commoner than the shorthand properties
of the same name.

### Two keys that changed the rules rather than a list

`language.markup` and `language.raw_text` are the first two that change what the tokeniser *does*, and
HTML is what made them necessary. HTML cannot be done the CSS way, because most of an HTML file is
prose and **seventy-six of its element names are ordinary English words** — `body`, `table`, `form`,
`main`, `code`, `time` — so a word-list plugin would colour a paragraph of English like a stylesheet,
and an apostrophe read as a quote would make every contraction yellow to the end of its line.

With `markup` on the tokeniser runs five states — text, tag name, attribute, value and raw text — and
a `<` opens a tag **only** when a letter, `/`, `!` or `?` follows it, which is the HTML Standard's own
tag-open state and the reason `5 < 3` in prose stays prose. Outside a tag everything is text: no
strings, no numbers, no operators.

The first word of a tag is a **keyword** if the language names it and a **type** if it does not,
because its position is certain and only whether the language defines it is unknown; an attribute is a
**builtin** if the language names it and plain text if it does not, which is the CSS rule applied
where it belongs.

`language.raw_text = script=javascript, style=css, textarea, title` is the second key, and the two
categories are **derived from one value rather than written down twice**: an entry that names a
language is a raw text element and one that names none is an escapable one, so `&amp;` inside a
`<title>` is coloured and `&amp;` inside a `<script>` is not, exactly as a browser reads them.

**The body of a raw text element is another language.** The editor crate says where it is and what it
names rather than colouring it, and the window runs the ordinary scan over that stretch with the
plugin that claims its language — the mirror of what a fence in a Markdown document already does. So a
`<style>` block is coloured exactly as a `.css` file is, and switching the CSS plugin off withdraws
the colouring inside `<style>` in the same frame. One level deep: the embedded scan's own list is
discarded.

## A language that has a picture

Mermaid did not widen the seam. It **is** a language — keywords, comments, strings, an extension — so
it is an ordinary `language` plugin, and colouring `.mmd` source is worth having on its own.

It carries one extra key, `language.renders = mermaid`, naming a renderer that is **built into
Unluminous**. Nothing is loaded from the plugin and nothing is executed: the manifest says "files of
this language have a picture, and this is which picture", and the code that draws it shipped with the
binary.

What it buys is that switching the plugin off actually withdraws the feature. The window asks before
it draws a diagram anywhere, so `.mmd` files stop being drawn and mermaid blocks inside Markdown go
back to being code — in the same frame, not at the next restart.

## The icons

A plugin's icon is `icon.png` beside the manifest, decoded once and drawn in front of every file the
plugin claims. A bundled plugin's icon is **generated rather than drawn**, and each one records how:
`crates/unluminous-app/plugins/<id>/icon.md` holds the prompt, the endpoint and the two commands, so
it can be made again without guessing.

`cargo run --example plugin_icon -- <source> <plugin folder>` keys the flat background out by flood
filling from the four corners, crops to the mark with an even margin, squares it, and scales it to 128
and to 32. Flood filling from the corners rather than matching a colour everywhere is deliberate: a
node in the mark may be near the background's own colour, and matching everywhere would eat it.

The window's own icons are drawn rather than lettered, and there are two sets: `classic` is what
shipped, and `material` — filled, rounder, a chevron disclosure and a folder mark in the explorer — is
the default. `design/icons.md` records how the material set was designed and which mark on the
reference sheet was rejected.

**A mark is judged at both sizes or it is not judged.** There is one test sheet per icon set at eight
pixels a point, accepted like any other picture, and it had to learn that being right at eight times
life size is not enough: the first debug mark was a play triangle and a beetle of equal weight, which
is fine at eight times and was a smudge with no half of it legible at the fourteen pixels a person
sees. Add a mark, add it to that sheet, and photograph it off the real title bar as well.

## Writing one

Copy a bundled plugin's folder out of `crates/unluminous-app/plugins`, or press `Install` and edit
what it wrote. Change `plugin.id` and `language.extensions`, put the language's words in the three
lists, and start Unluminous again. There is nothing to compile and nothing to register.
