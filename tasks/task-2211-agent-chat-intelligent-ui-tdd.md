# task-2211: Agent-Chat answers that are interfaces (TDD)

`tasks/task-2211-agent-chat-intelligent-ui-prd.md` is what is wanted and why. This document is how.

## 1. The shape of it

```
the agent writes            unluminous-chat::rich               rux                      unluminous-app
-----------------           ----------------------             ---------------           -------------------------
markdown                    segments(text)                     Callout, Stat,            message.rs: a row is
```ui                  -->    Markdown | Block{source,done} --> Chart, Table,      -->    words, blocks, words...
{ "type": "chart", ...}     repair(partial json)               Timeline, Checkbox,       blocks.rs: one function a
```                         Component::read(value)             Slider, Tabs, Badge,      component, measured then
more markdown               expr::eval (calculator)            + zoom on Button, Chip,   drawn, reporting Acts
                            catalogue (docs + examples)        Progress, Segmented
```

Three crates, each with the job its crate already has:

- **`unluminous-chat::rich`** reads. It has no user interface dependency, so every rule about what an
  agent may write, what a half written block means, and what an expression evaluates to is a unit test
  with no window. It also holds the catalogue: the one list of components, their fields and an example
  each, from which the agent's guide, `components`, `validate` and the gallery are all made. Writing
  any of those out by hand would be a second list.
- **`rux`** draws the base controls. The ticket names it, and the controls belong there: a chart, a
  callout, a stat tile, a checkbox and a slider are controls any Black Rainbow Labs program will want,
  and none of them knows what a chat is.
- **`unluminous-app`** composes. `components/agent_chat/blocks.rs` turns a `Component` into rux
  controls inside a card, measures it, draws it and reports what was pressed as an `Act`.
  `services/agent_chat` keeps the state a component needs between frames and carries out the acts.

## 2. How an agent writes a component

A fenced block whose language is `ui`, holding one JSON object with a `type`:

````markdown
The build got slower in three places.

```ui
{"type": "chart", "kind": "bar", "title": "Build time by crate",
 "unit": "s", "labels": ["core", "app", "cli"],
 "series": [{"name": "before", "values": [41, 212, 9]},
            {"name": "after",  "values": [38, 131, 9]}]}
```

The app crate is most of it.
````

Why a fence holding JSON:

- **A model writes JSON correctly.** It is the format tool arguments are written in, so every model
  this pane can talk to has been trained hard on it.
- **The answer stays readable anywhere else.** In a terminal, a transcript, a copy pasted into a
  ticket, a fence is a code block. Nothing is lost, which a custom tag syntax could not promise.
- **The fence marks where a component starts before its JSON is complete**, which is what makes
  progressive drawing possible: the moment ```` ```ui ```` arrives, Unluminous knows the next bytes are a
  component.

`ui` is the language. ```` ```json ui ```` and ```` ```unluminous ```` were weighed and refused: a
second spelling is a second thing for the guide to explain.

A fence of any other language is markdown, exactly as today, so ```` ```json ```` stays a code block.

## 3. The progressive compiler

`rich::segments(text) -> Vec<Segment>` walks the text line by line and answers
`Segment::Markdown(String)` and `Segment::Block { source, finished }`. A ```` ```ui ```` line opens a
block, a line that is exactly ```` ``` ```` closes it, and a block still open when the text ends is
`finished: false`. The rules the markdown parser uses for fences (up to three spaces of indent, a
longer closing fence) are kept, so a block the preview would read as code is a block here.

`rich::repair(source) -> String` closes a JSON document that was cut off:

- an unterminated string is closed (an escape cut in half is dropped first);
- a key with no value, a trailing `:` and a trailing `,` are dropped;
- a number cut after its `-`, `.` or `e` is cut back to the last digit;
- a `t`, `tr`, `f`, `nul` prefix of a literal is completed;
- every open `[` and `{` is closed in order.

Then `serde_json` reads it. It is checked against the text cut at every byte boundary of every
example in the catalogue: each prefix either reads as JSON or is empty, and the components read from
the prefixes only ever grow (a table never loses a row it had a byte ago). That property is the test
`a_component_only_grows_as_its_text_arrives`.

`Component::read(&Value, finished) -> Result<Component, Problem>`:

- While the block is still arriving, a missing field takes its empty value and the component draws
  with what it has. A `type` that has not arrived yet draws a quiet placeholder of one row.
- Once finished, a missing required field or a value of the wrong kind is a `Problem` naming the path
  (`series[1].values[3]: expected a number`). A key the component does not have is **ignored when
  drawing and reported by `validate`**, so a model adding a field we do not support costs nothing in
  the pane and is still told about it.

Each read is cached in `PaneState` by the block's key (message id and block index) and the source's
length and hash, so a finished conversation is parsed once, and a streaming block once a frame.

## 4. The components

Every field name is `snake_case`, a list is always a list (never "a string or a list"), and every
component takes an optional `title`. Defaults are written in the catalogue next to each field.

| `type` | Fields | Drawn as |
|---|---|---|
| `card` | `title`, `subtitle`, `text` (markdown), `badge`, `children` | a raised card |
| `columns` | `columns: [component]` | side by side; one column under 260 points per column |
| `tabs` | `tabs: [{label, children}]` | rux `Segmented` over the chosen tab's children |
| `stack` | `children` | one under another, no card |
| `callout` | `tone` (info, success, warning, danger, tip), `title`, `text` | rux `Callout` |
| `steps` | `items: [{title, text, state}]`, state is done, active or todo | rux `Timeline`, numbered |
| `timeline` | `items: [{time, title, text, state}]` | rux `Timeline`, with the time beside each |
| `keyvalue` | `items: [{key, value}]` | two columns, keys quiet |
| `badges` | `items: [{label, tone}]` | rux `Chip`s, wrapped |
| `stats` | `items: [{label, value, delta, trend, note}]`, trend up, down or flat | rux `Stat` tiles in a grid |
| `progress` | `items: [{label, value, max}]` | rux `Progress` bars with numbers |
| `chart` | `kind` (bar, line, area, donut), `labels`, `series: [{name, values}]`, `unit`, `stacked` | rux `Chart` |
| `table` | `columns: [{label, align}]` or `[string]`, `rows: [[cell]]` | rux `Table`, sortable by a header press |
| `files` | `items: [{path, line, note}]` | rows that open the file at the line |
| `diff` | `path`, `text` (unified diff) | added and removed lines coloured, the file openable |
| `diagram` | `source` (Mermaid) | the editor's own Mermaid layout, fitted to the width |
| `actions` | `items: [{label, send \| open \| copy \| fill, primary}]` | rux `Button`s |
| `choices` | `question`, `options: [string]` | one button an option; pressing sends the option |
| `checklist` | `items: [{label, done}]` | rux `Checkbox`es; ticks are kept in the pane |
| `form` | `fields: [{name, label, kind, options, value, min, max, step}]`, `submit` | rux fields; submit sends the values as a message |
| `calculator` | `inputs: [{name, label, value, min, max, step, unit}]`, `outputs: [{label, expr, unit, format}]`, `chart` | rux `Slider`s and `Stat`s, recomputed as an input moves |

`calculator.chart` is `{kind, x: {name, from, to, step}, series: [{name, expr}]}`: each series is an
expression evaluated at every `x`, with the inputs in scope. That is how "a calculator to explore how
savings could grow" is written:

```json
{"type": "calculator", "title": "Savings",
 "inputs": [{"name": "monthly", "label": "Each month", "value": 300, "min": 0, "max": 2000, "step": 50, "unit": "$"},
            {"name": "rate", "label": "Return", "value": 5, "min": 0, "max": 12, "step": 0.5, "unit": "%"},
            {"name": "years", "label": "Years", "value": 20, "min": 1, "max": 40}],
 "outputs": [{"label": "After the last year", "expr": "fv(rate/100/12, years*12, monthly)", "format": "money"}],
 "chart": {"kind": "area", "x": {"name": "y", "from": 0, "to": "years"},
           "series": [{"name": "balance", "expr": "fv(rate/100/12, y*12, monthly)"}]}}
```

### 4.1 Expressions

`rich::expr` is a Pratt parser over numbers, names, `+ - * / % ^`, comparisons, `&& || !`, `a ? b : c`
and function calls. Functions: `abs min max round floor ceil sqrt pow exp ln log10 clamp if`, and two
for money because money is what people ask about: `fv(rate, periods, payment)` and
`pmt(rate, periods, principal)`. Names are the calculator's inputs and, inside a chart, its `x` name.
Nothing else is in scope: there is no way to reach a file, a variable of the process or a function
the list does not name, and evaluation is bounded at 10,000 steps so an answer cannot hang the frame.
A divide by zero or an unknown name is shown as `—` beside the output with the reason on hover.

### 4.2 Actions

An action is one of four, and only these:

| key | What a press does |
|---|---|
| `send` | sends the text as the person's next message, as if typed |
| `fill` | puts the text in the composer and leaves it there to be edited |
| `open` | opens `path` (relative to the project) at `line` in the editing area |
| `copy` | copies the text |

A component **cannot run a command**, change a file, or fetch anything. `open` refuses a path outside
the project and says so in a notice. That is the safety argument for the whole library: the worst a
component an agent wrote can do is put words in the composer.

A `form` submit is `send` with the values written as a short list under the form's title, so the
agent receives them as ordinary text and needs no new protocol.

## 5. rux

### 5.0 The design language: an instrument

Jason chose this direction on 2026-10-08 over a dot matrix language and a flat typographic one, after
rejecting the first baseline as too common: coloured side bars on cards and generic blue filled
buttons and checkboxes. The canvas at https://claude.ai/artifact/2jXSbhPcCGnt8iHmE2z5YR shows it.

An answer's components read as machined modules on a studio desk, after Teenage Engineering and Braun.
The rules:

- **Two surfaces only.** A **plate** is raised, with a one point lit edge along its top (the chamfer of
  machined metal catching light) and the theme's soft shadow pair. A **screen** is recessed into a plate,
  darker than anything else (`#15181D`), with a faint dot graticule every six points; data lives on
  screens: chart, stat readouts, table body, diff, meters, fields.
- **Colour is light, never paint.** No fill of accent colour on a control, no coloured stripe on a card.
  Colour appears as an **LED** (a six point lit dot with a soft bloom), as glowing data (segments,
  lines, numbers on a screen), and as nothing else. A primary button is a key cap like any other with
  its LED lit. A tone (warning, success) is the colour of the callout's LED.
- **Controls are hardware.** A button is a key cap: raised, and on press it sinks one point and its
  shadow turns inward over 90 ms. A checkbox is a small key cap with an LED in it: down and lit when
  ticked. A slider is a fader: a groove, tick marks under it, a raised rectangular cap with an
  engraved centre line, the travelled part of the groove lit. A choice row and tabs are a row of keys
  where the chosen one is down with its LED lit.
- **Encoder colours for data.** Series take blue, mint, amber, coral, violet in order, the OP-1's
  encoder colours, already in the theme's accents. A comparison's older series is a quiet grey.
- **Silkscreen for labels.** Small spaced capitals in a quiet ink for axis and field labels and stat
  names, printed on the plate. Never as a kicker above a heading.
- **Numbers are instruments.** Tabular figures in the mono face on screens.
- **Motion is physical, and authored once per kind of change.** A press sinks (90 ms). An LED fades on
  with its bloom (160 ms, ease out). Bars and segments rise from the baseline with a small overshoot
  when a chart first appears or a value changes (320 ms, 30 ms stagger). A number rolls to its new
  value (240 ms, ease out). An active step's LED breathes slowly (2.4 s). The welcome page lights its
  status LEDs in sequence once. Nothing loops except the breathing LED of a step in progress.

### 5.1 A zoom for the whole library

The chat pane zooms by a multiplier (`Look::scale`), not through a layer transform, so a rux control
drawn into it has to be told. `Select::zoom` already does this for one control. The rest of the
library gains the same through **`RuxState::set_zoom`**, read by `Rux::zoom()` and a helper
`Rux::z(points) -> f32`. It is on the state rather than on each builder because the chat draws a dozen
controls per component and a builder argument on each would be a dozen chances to forget it. The
default is 1, so every existing caller and every `tests/reference.rs` measurement is unchanged.

Every new component reads it. Of the existing ones, the four the chat uses read it too: `Button`,
`Chip`, `Progress` and `Segmented`. `Select::zoom` stays, multiplied by the state's zoom.

### 5.2 New components

All of them follow the library's rules: builder, `measure`/`height_at`, `show(rux, rect)`, theme
tokens only, no state of their own, and nothing that changes every frame recorded into the chrome.

| Component | Notes |
|---|---|
| `Led` | the lit dot every other control uses: off, or on in a colour with a bloom, with a brightness from 0 to 1 so it can fade. |
| `Screen` | the recessed data surface with its graticule. A drawing helper as much as a component. |
| `Plate` | the raised surface with its lit top edge. |
| `Key` | a key cap button: label, optional LED, down while pressed or while chosen. |
| `Checkbox` | an 18 point key cap with an LED in it; down and lit when ticked. |
| `Fader` | a groove, ticks, a raised cap; drag, click on the groove, arrow keys. Reports the new value. |
| `Meter` | a row of segments, lit up to a fraction, in a colour. |
| `Chart` | bar (grouped or stacked, drawn as segments), line (a glowing trace with a dot at each point), area and donut (a segmented ring). Gridlines at round numbers (`nice_ticks`), axis numbers in the mono face, a legend of LEDs, the value under the pointer. Bars are egui shapes over the screen, so hovering does not rasterise the layer again. |
| `Table` | on a screen: silkscreen header, hairline rows, numbers right aligned in tabular figures, cells elided to their column. Pressing a header reports it; sorting is the caller's. |
| `Timeline` | a groove with an LED per item: lit mint for done, breathing blue for active, dark for todo. |

Each gains a drawing path in `tests/draw.rs` (both themes, desktop and phone) and a story in the
storybook. Their numbers are composed from the existing tokens (radii, elevations, ink and accents),
and each says so where it is set, because there is no reference stylesheet for them.

## 6. Unluminous

### 6.1 A row with components in it

`message::pieces` currently gives a message one `Piece::Words`. It becomes a run of
`Piece::Words { segment }` and `Piece::Block { segment }` in the order the segments came, so text
before, between and after components is drawn where it was written. Words stay in the bubble they
are drawn in today. A block is drawn at the width of a report (`BLOCK_SHARE`), on the left, raised,
because it is a thing to look at and use rather than speech. Only the first bubble of a message keeps
the squared corner.

The selection and the copy button work per words segment. `Copy Message` still copies the whole
source, components included, as JSON, which is what pasting it into a file or another chat wants.

### 6.2 Measuring and drawing

`blocks::height(component, state, look, ui, width)` and `blocks::show(...) -> Vec<Act>` are the pair,
called the way `message::shape` and `message::show` are. Every block is drawn inside one
`rux::layer` whose `RuxState` lives in `PaneState::rux` and has its zoom set from `look.scale()` each
frame, with `end_frame()` called once after the conversation is drawn. The layer's chrome is clipped
to the conversation's rectangle, as the bubbles' are, so a card scrolled half out of view does not
draw its shadow over the header.

Text inside a component that the agent may format (`text` fields) goes through the same
`markdown_text` cache as the bubbles, keyed `block-<message>-<index>-<field>`. Labels and numbers are
rux text.

### 6.3 What a component remembers

`PaneState::blocks: HashMap<String, BlockState>` keyed `<message>-<index>`: the chosen tab, the
table's sort, the checklist's ticks, the form's values, the calculator's inputs, which items an
`actions` block has had pressed. It lives as long as the window, and `forget_what_was_showing`
clears it on a conversation switch for the reason it already clears selections: ids are per
conversation.

### 6.4 The acts

`Act` gains `Send(String)`, `Fill(String)`, `OpenFile(String, Option<u32>)`, `Block(String, BlockAct)`
where `BlockAct` is `Tab(usize)`, `Sort(usize)`, `Tick(usize)`, `Set(String, f64 | String)`,
`Submit`. `Send` goes through `AgentChat::send` after putting the text in the draft, so it queues
behind an answer that is still arriving exactly as typing does. `OpenFile` becomes a new
`Request::OpenFile { path, line }`, which the window carries out through `open_path_permanently` and
the caret move `Go to Line` uses.

### 6.5 What the agent is told

`rich::catalogue::guide()` writes a compact reference: one line per component with its fields, the
four actions, five rules of judgement, and two examples. About 1,400 tokens. It goes:

- into the first question of a conversation with a command line agent, after the line that says it is
  answering in a pane (`what_the_agent_should_know`);
- into the system prompt for a model at an address (`system_prompt`).

The rules of judgement are the ones the OpenAI page describes the model learning, written as
instructions:

1. Text is the default. Use a component when it makes the answer faster to read or lets the person
   act on it.
2. One component that answers the question beats several that decorate it.
3. Numbers that are compared want a chart or a table; a sequence wants steps or a timeline; a
   decision wants choices; next steps want actions.
4. Every value in a component must be real. Never invent data to fill a chart.
5. Before answering with a component you have not used before in this conversation, check it with
   `unluminous-cli plugins run agent-chat validate '<json>'` if you can run commands.

### 6.6 Commands

On the plugin's own `command`, so `plugins run agent-chat <verb>` and the MCP tools reach them with
no new catalogue rows:

| Verb | What it does |
|---|---|
| `components [name]` | the reference, or one component with its fields and example |
| `validate <json>` | `valid` or every problem with its path |
| `gallery` | opens a new conversation holding one answer per component |
| `press <message> <block> <label>` | presses an action, a choice or a submit |
| `tick <message> <block> <item>` | toggles a checklist item |
| `set <message> <block> <name> <value>` | sets a form field or calculator input |
| `tab <message> <block> <index>` | chooses a tab |

`plugins view agent-chat` gains `blocks` for each message: kind, title, whether it read, its
problem if not, and its state (ticks, values, computed outputs). An agent can therefore read what a
calculator shows without a screenshot.

## 7. The welcome page

The four starter chips go. `components/agent_chat/welcome.rs` draws, in the instrument language:

1. **The agent.** A plate with the editor's mark and a lamp in its corner (mint when the agent can
   answer, amber when it cannot), then the headline "Claude Code, on unluminous" and one line on what
   answers can hold. When the chosen agent is not installed, the headline says so and the line under it
   is the reason.
2. **What it can see.** A screen of four rows, each a lamp, a silkscreen label, a value and a quiet
   detail: the agent and its model, the access it has, the project and its folder, the file showing.
   The lamps light in turn as the pane opens, the way an instrument's do when it is switched on.
3. **Earlier.** Up to three recent conversations as rows that rise like a key cap under the pointer, with
   their age (`2h`, `3d`). Pressing one opens it. These are the person's own conversations, so they are
   not suggestions.
4. **Keys.** One quiet line over the composer.

Under 300 points of width the detail column goes and the key line shortens. As the pane gets shorter
the sections are given up from the bottom: first the earlier conversations, then the status screen, so
a strip along the bottom still reads as a composed page.

## 8. Testing

- `unluminous-chat`: segments (fences, nested backticks, unclosed), repair at every byte of every
  catalogue example, every component reading its own example, every problem path, expressions
  (precedence, functions, bounds, errors), the guide mentioning every component.
- `rux`: `tests/draw.rs` for each new component in both themes on desktop and phone; zoom 1 leaves
  every reference measurement unchanged; `nice_ticks`; slider value from a drag position.
- `unluminous-app` unit tests: block heights at five font sizes, act application, the command verbs,
  the view's `blocks`.
- Screenshot tests in `plugins_board_and_chat.rs`: the gallery at 420 and 760 points wide; zoomed to
  0.8 and 1.5; docked as a bottom strip; a streaming chart and table cut at three points; a block
  that does not read; the welcome page at three widths and with an agent that is not installed; a
  calculator after `set`; a checklist after `tick`; and a chat node on the canvas showing the gallery.
- In the installed window: a real Claude Code turn asked for something whose best answer is a
  component, photographed, plus the gallery photographed at several zooms and arrangements.

## 9. Left out, with the reason

- Maps and fetched pictures: Unluminous fetches nothing.
- A component that runs a command: see §4.2.
- Persisting ticks and values across a restart: §6.3.
- Interleaving thinking with the answer: that is how a model is trained, not something a client
  draws. The pane already shows thinking and words as they arrive.
- A rux theme that follows an Unluminous theme: rux has two themes and the chat already draws its
  model selector in `dark-neumorphic`. Following `plugin.kind = theme` is a rux change of its own.
