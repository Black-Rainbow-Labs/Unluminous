# Every Mermaid diagram type, C4 included (`task-2339`)

The ticket: *"We have several variations of mermaid diagrams we don't render. We also don't support C4
diagrams. Research the code, research online, create a tdd and fully implement supporting all mermaid as
well as c4. Use screenshots to visually confirm things look good before doing the release. We need our
visual verification to be extensive, and ensure things are readable, text is contained properly in
boxes, things don't cover up areas and make them unreadable, etc."*

`tasks/unluminous-mermaid-plugin-tdd.md` is the design this extends. Everything it says still holds: the
diagrams are laid out in Rust in `unluminous_core::mermaid`, the seam is a `Scene` of five kinds of item,
nothing in a document is fetched or run, and a document does not choose the window's colours.

## 1. What was measured before anything changed

Mermaid's own documentation is the corpus. Every page under `packages/mermaid/src/docs/syntax/` on the
`develop` branch was downloaded into `_agent_output/task-2339-mermaid/docs/` (35 pages, 548 ` ```mermaid `
blocks), and `cargo run --example mermaid_audit` was run over them. The result:

| Finding | Count |
| --- | --- |
| Blocks refused | 154 of 548 |
| Refused because the type is named but not drawn | 71 |
| Refused because the keyword was not recognised at all | 76 |
| Refused although the type is drawn | 7 |
| A panic | 1 |
| Text left holding markup (`&nbsp;`) | 2 |
| Pairs of text drawn over each other (fixed width stub) | 74 |

**The panic** was `source::take_word`, which split a line at the byte length of a keyword. The ER example
`"This ❤ Unicode"` puts the end of `❤` past that byte, and `split_at` panicked. A preview of that
document closed the window. It is fixed in this ticket with `str::get`.

**Thirteen types are not drawn.** Ten were named in the old list. Three are newer than that list and were
not recognised at all, so their keyword was refused as "not a Mermaid diagram":

| Type | Keyword(s) | Since |
| --- | --- | --- |
| C4 | `C4Context`, `C4Container`, `C4Component`, `C4Dynamic`, `C4Deployment` | 9.x |
| Architecture | `architecture-beta` | 11.1 |
| Venn | `venn-beta` | 11.12 |
| Ishikawa | `ishikawa-beta`, `ishikawa`, `fishbone` | 11.12 |
| Wardley | `wardley-beta` | 11.14 |
| Tree view | `treeView-beta` | 11.14 |
| Event modelling | `eventmodeling` | 11.15 |
| Cynefin | `cynefin-beta` | 11.16 |
| Swimlanes | `swimlane-beta` | 11.16 |
| Railroad | `railroad-beta`, `railroad-ebnf-beta`, `railroad-abnf-beta`, `railroad-peg-beta` | 11.16 |
| ZenUML | `zenuml` | 10 |
| Agentflow | `agentflow-beta` | 12.0 |
| Use case | `usecase-beta` | 12.0 |

**Seven blocks of types already drawn were refused**, and two drew raw markup:

| Block | What was not read |
| --- | --- |
| flowchart `examples.md #4` | A shape whose label holds Cyrillic text and a `;` after it |
| gantt `#3` | A task line with no metadata after the colon |
| treemap `#2`, `#3` | `:::class` after a leaf's value |
| xychart `#5`, `#6` | A text label after a data point |
| block `#0`, `#22` | Block arrows, `id<["label"]>(down)` |

The refusals are only the faults a parser notices. A feature that is read and ignored is drawn wrongly and
nothing reports it, which is why §6 is the half of the testing that matters.

## 2. Where each new type goes

One module a type in `crates/unluminous-core/src/mermaid/`, each with `pub fn render(source: &Source,
options: &Options) -> Result<Scene, Problem>`, each added to `Kind`, `kind_of`, `render` and `SAMPLES`,
and each with a sample file in `sample-diagrams/`.

Three of the thirteen are another type's grammar with a different keyword, and they are written as a
translation into that type rather than a second renderer:

- **Swimlanes** is a flowchart whose top level subgraphs are lanes. It is read by the flowchart's reader
  and laid out by a lane layout (§3.9), because the layered layout draws a subgraph as a frame sized to its
  contents and a lane has to run the whole length of the diagram.
- **Agentflow** is a flowchart with `flow … end` for `subgraph … end`, a `global` block, `connector`
  nodes, metadata lines, and six shape aliases. It is rewritten into flowchart text and drawn by the
  flowchart.
- **ZenUML** is a sequence diagram in a method call syntax. It is rewritten into sequence diagram text,
  with activations for nested calls, and drawn by the sequence diagram.

The other ten have a picture of their own and a module of their own.

## 3. Each type

### 3.1 C4 (`c4.rs`)

**Read.** Every statement is a call: `Name(arg, arg, …)` with optional `$name="value"` arguments and an
optional `{` that opens a boundary, closed by `}`. Arguments are split outside quotes. The elements are
`Person`, `Person_Ext`, `System`, `SystemDb`, `SystemQueue`, `System_Ext`, `SystemDb_Ext`,
`SystemQueue_Ext`, `Container`, `ContainerDb`, `ContainerQueue` and their `_Ext` forms, the same six for
`Component`, `Deployment_Node`, `Node`, `Node_L`, `Node_R`. The boundaries are `Boundary`,
`Enterprise_Boundary`, `System_Boundary`, `Container_Boundary`, and any `Deployment_Node` or `Node` that
opens a `{`. The relations are `Rel`, `BiRel`, `Rel_U`/`Rel_Up`, `Rel_D`/`Rel_Down`, `Rel_L`/`Rel_Left`,
`Rel_R`/`Rel_Right`, `Rel_Back` and `RelIndex`. `UpdateElementStyle`, `UpdateRelStyle`,
`UpdateLayoutConfig`, `AddElementTag`, `AddRelTag` and `Lay_*` are read and ignored: they choose colours or
hand placement, which §13 of the old design keeps out of a document's hands.

**Lay out.** Mermaid's own C4 layout puts shapes in rows of four in statement order and draws relations
as straight lines across whatever is between, so its pictures routinely have lines through boxes and labels
on top of each other. That is the opposite of what the ticket asks for. Unluminous uses the layered layout
instead: elements are nodes, boundaries are groups (nested to any depth), relations are labelled edges,
top to bottom. `Rel_U` and `Rel_L` reverse the edge for the layering so the target lands above; `Rel_Back`
draws the arrow from the second element to the first.

**Draw.** C4's own notation, which is the point of the diagram type:

- A box holds three parts, centred: the name in bold; a line in brackets saying the kind and the
  technology, `[Container: Java, Spring MVC]`; and the description, wrapped, in a smaller face.
- A person has a head: a circle sitting on the top edge of a rounded box.
- `*Db` is a cylinder and `*Queue` is a cylinder on its side.
- Elements are filled by level, the way C4 is drawn everywhere: person and system in the strongest
  colour, container lighter, component lighter still, and anything `_Ext` grey. The colours come from the
  theme's accent and its dim colour, so they follow a light or a dark theme.
- A boundary is a dashed frame with its name in bold and its type in brackets under it.
- A relation is a line with an arrow, a label and the technology in brackets on a second line. `BiRel` has
  an arrow at both ends. In `C4Dynamic`, and wherever `RelIndex` is used, the label is numbered in the
  order written.

### 3.2 Architecture (`architecture.rs`)

**Read.** `group id(icon)[Title] (in parent)?`, `service id(icon)[Title] (in parent)?`, `junction id (in
parent)?`, edges `a{group}?:S <?--? >? S:b{group}?`, and `align row|column ids…`. `title` and `accTitle`
as usual.

**Lay out.** Mermaid places services by the sides their edges leave from: `a:R -- L:b` puts `b` to the
right of `a`. That is a grid, and Unluminous builds it as one: a breadth first walk from each unplaced
service puts each neighbour one cell in the direction its edge says, and a cell already taken pushes the
newcomer on in the same direction. `align row` and `align column` give their members one row or one column.
Each group's cells are then made contiguous by laying the grid out group by group: the services of one
group are kept in their own rectangle of cells, and groups are placed beside each other in the order they
were declared. A group's frame is the bounds of its members plus a margin and a title.

**Draw.** A service is an icon tile with its title under it. The five built in icons are drawn as
shapes: `cloud`, `database` (a cylinder), `disk` (a flat cylinder), `internet` (a globe) and `server` (a
rack of three units). Any other icon, which Mermaid would fetch from an icon pack, is a plain rounded tile
with the icon's name in it, because nothing is fetched. A junction is a small dot. Edges leave and arrive by
the side they name, run as orthogonal lines with one or two bends, and carry an arrowhead where `<` or `>`
asks. A group is a dashed frame with its icon and title in the top left.

### 3.3 Venn (`venn.rs`)

**Read.** `set Id["Label"]:size`, `union A,B["Label"]:size`, indented `text Id["Label"]` lines that
belong to the set or union above them, `title`, and `style` (ignored).

**Lay out.** One set is a circle. Two are two circles overlapping by a third of their diameter. Three are
the classic arrangement: centres on an equilateral triangle. Four or more are placed with their centres on
a ring so that each overlaps its two neighbours. A size scales a circle's radius by its square root,
clamped so that a tiny set is still large enough to hold its name.

**Draw.** Each set is a translucent circle in a series colour, so the overlaps read as darker regions. A
set's name sits in the part of its circle that no other circle covers, found by sampling points in the
circle and taking the one furthest from every other circle. A union's label sits at the point of the
overlap furthest from the other circles' edges. Text nodes are listed under their set's or union's label.

### 3.4 Ishikawa (`ishikawa.rs`)

**Read.** The first statement is the problem. Each line after it is a cause, nested by indentation.

**Draw.** The fish faces right. The problem is a box at the right end of a horizontal spine. Each top
level cause is a bone leaving the spine on a slant, alternating above and below, with the category's name
in a box at the far end. Each cause under it is a short horizontal line off that bone with its words on
it, and a cause under that is a shorter line off the cause. Bones are spaced along the spine by the room
their causes need, so words never cross a neighbouring bone.

### 3.5 Wardley (`wardley.rs`)

**Read.** `title`, `size [w, h]`, `evolution A -> B -> C -> D` with optional `@width` and `/` dual labels,
`anchor Name [vis, evo]`, `component Name [vis, evo] (label [x, y])? (decorator)*`, links `A -> B`, `A -->
B`, `A -.-> B`, `A +> B`, `A +< B`, `A +<> B`, `A +'text'> B`, with `; annotation`, `evolve Name evo`,
`pipeline Name { component "x" [evo] … }`, `note "text" [vis, evo]`, `annotations [x, y]`, `annotation
n,[x, y] "text"`, `accelerator` and `deaccelerator`. Names may be quoted or contain spaces and hyphens.

**Draw.** A plot area whose left edge is "Visible" at the top and "Invisible" at the bottom and whose
bottom edge is divided into the four evolution stages by dashed lines. A component is a small circle with
its name beside it, an anchor is a bold name with no circle. Links are thin lines; flows are thicker with an
arrow. `evolve` is a red dashed arrow from the component to a ghost circle at its new evolution. Inertia is
a thick bar on the right of the circle. `(build)`, `(buy)`, `(outsource)` and `(market)` are a triangle, a
diamond, a square and a ring drawn behind the circle. A pipeline is a frame round its components under the
parent. Notes are text, annotations are numbered circles with a key box, accelerators are wide arrows.

### 3.6 Cynefin (`cynefin.rs`)

**Read.** `title`, the five domain keywords each opening a block of quoted items, and `a --> b : "label"`
transitions.

**Draw.** Four quadrants, always in the same places (Complex top left, Complicated top right, Chaotic
bottom left, Clear bottom right), with the boundaries between them drawn as gentle waves whose shape is
fixed, so the same source gives the same picture. Each domain's name is bold at its outer corner with its
decision model under it (`Probe → Sense → Respond`). Items are rounded badges stacked in the domain. The
Clear to Chaotic boundary has the "cliff": a thick line along the lower part of the bottom boundary.
Confusion is an ellipse in the centre with up to three items and `+N more`. Transitions are curved arrows
between domain centres with their labels at the middle.

### 3.7 Tree view (`treeview.rs`)

**Read.** Lines nested by indentation, or drawn with box characters (`├──`, `└──`, `│`, and the heavy
forms), whose depth is the column of the branch character. A label may be quoted. A trailing `/` makes a
folder. `:::class` and `icon(…)` are read and ignored. `## description` is kept.

**Draw.** One row a line. Each row is indented by its depth, with an elbow connector from its parent's
vertical line. Folders are bold. A description is drawn after the name in the dim colour, in italic.

### 3.8 Event modelling (`eventmodeling.rs`)

**Read.** `tf`/`timeframe` and `rf`/`resetframe` lines: a number, a type (`ui`, `pcr`/`processor`,
`cmd`/`command`, `rmo`/`readmodel`, `evt`/`event`), an entity name with an optional namespace before a dot,
then inline data in `{…}` or a `[[data block]]`, then `->> n` relations. `data Id { … }` blocks over
several lines.

**Draw.** A grid. Columns are time frames in the order written. Rows are swimlanes: UI/Automation,
Command/Read Model and Events, plus one for each namespace and type pair, in the order they first appear.
Each frame is a sticky note in its column and its lane, coloured by type the way the method colours them
(UI white-ish, command blue, read model green, event orange, processor purple), with the entity's name and
its data under it in a small face. Each frame is joined to the one before it unless it is a reset frame,
and to every frame its `->>` names.

### 3.9 Swimlanes (`swimlane.rs`)

**Read.** The flowchart's reader, given the source with `swimlane-beta` in place of `flowchart`.

**Lay out.** Each node gets a rank by the longest path through the graph, with the flowchart's cycle
breaking. In a left to right diagram the lanes are horizontal bands stacked in the order the subgraphs were
written, and a node's x comes from its rank and its y from its lane; nodes in one lane and one rank are put
side by side. Top to bottom swaps the two. Each lane is as tall as its tallest stack. Edges are routed as
orthogonal lines that leave by the side facing their target.

**Draw.** Each lane is a band with its title in a header strip at its start, alternating between two
washes so the lanes can be told apart. Nodes are the flowchart's shapes. Labels sit on a panel at the
middle of their edge.

### 3.10 Railroad (`railroad.rs`)

**Read.** Four notations into one expression tree (`Terminal`, `NonTerminal`, `Special`, `Sequence`,
`Choice`, `Optional`, `ZeroOrMore`, `OneOrMore`, `Except`, and PEG's `And` and `Not`): EBNF in W3C and ISO
styles (`=` or `::=`, `|`, `?` `*` `+`, `[ ]`, `{ }`, `( )`, `,` sequences, `(* *)` and `/* */` comments,
`? special ?`, `-` exception); ABNF (`/`, `*A`, `1*A`, `n*mA`, `nA`, `[ ]`, `%x41` and ranges, `;`
comments); PEG (`<-`, `/`, `& !`, `.`, `#` comments); and the IR's constructor calls. Rules end with `;`.

**Draw.** One railroad a rule, stacked, each with the rule's name above it: a start mark, the track, and
an end mark. A terminal is a rounded box, a non terminal a square box, a special sequence a box with a
dashed border. A sequence is laid left to right. A choice stacks its alternatives with curves splitting off
the main line and joining back. An optional is a choice with a bare track. A repetition has a loop running
back underneath, with a separator on the loop when there is one. A rule wider than a limit wraps its top
level sequence onto further lines.

### 3.11 ZenUML (`zenuml.rs`)

Rewritten into a sequence diagram:

| ZenUML | Sequence diagram |
| --- | --- |
| `A`, `@Actor A`, `@Database A`, `A as Label` | `participant`, `actor`, a database participant, an alias |
| `A->B: text` | `A->>B: text` |
| `A.method(args)` with a caller | `caller->>A: method(args)`, and a dashed return if it is assigned |
| `A.method() { … }` | the call, `activate A`, the body with `A` as the caller, `deactivate A` |
| `x = A.m()` and `return x` | a dashed reply to the caller |
| `new A(args)` | `create participant A` and a `«create»` message |
| `while`, `for`, `forEach`, `loop` | `loop … end` |
| `if … else if … else` | `alt … else … end` |
| `opt`, `par` | `opt … end`, `par … and … end` |
| `try … catch … finally` | `critical try … option catch … option finally … end` |
| `// comment` | a note over the participant of the next message |

A call at the top level has no caller, and ZenUML draws it from the left edge. Unluminous gives it an
actor named `Client` at the left, which is what ZenUML's own pictures show as a stick figure.

### 3.12 Agentflow (`agentflow.rs`)

Rewritten into flowchart text: `flow id["Label"] … end` becomes `subgraph id["Label"] … end`; `global …
end` keeps its nodes at the top level by declaring them before anything else; `connector id["Label"]`
becomes a node of the `stadium` shape; a line that is only `id@{ … }` for a container or a connector is
dropped, and for a node keeps only `shape` and `label`; a metadata block over several lines is joined
first. The six shape aliases go to the flowchart: `task` round, `tool` subroutine, `input` lean right,
`decision` diamond, `refdoc` the lined document shape, `action` hexagon. `@{ view: collapsed }` on a flow
draws it as one node holding the flow's name, with edges into it.

### 3.13 Use case (`usecase.rs`)

**Read.** `direction`, `actor Id`, `actor Id("Label")@{ … } <<Stereotype>>`, use cases as `Id("Label")`
(ellipse), `Id[Label]` (rectangle), a bare `Id` or a quoted `"Label"`, `systemBoundary id["Title"] … end`,
the seven associations (`-->`, `<--`, `--`, `--o`, `o--`, `--x`, `x--`) with labels and extra dashes,
`..> : include`, `..> : extend`, `--|>`, edge ids (`id@-->`), `note for X "text"`, `json Id@{ … }` over
several lines, and `classDef`, `class`, `style` and edge metadata (ignored).

**Lay out.** The layered layout: actors, use cases, notes and JSON tables are nodes, boundaries are
groups, relationships are edges, in the given direction (left to right by default).

**Draw.** An actor is a stick figure with its name under it; a hollow actor is drawn in outline only; a
business actor or use case has the slash. A use case is an ellipse or a rectangle. A stereotype is a
`«name»` line above the label. Include and extend are dashed lines with an open arrow and `«include»` or
`«extend»` on them, generalisation a hollow triangle. A note is a folded corner box joined by a dotted
line. A JSON table is a two column table of paths and values.

## 4. The variations in types already drawn

Each refusal in §1 is fixed in the type's own reader, and each of the following, found by reading the
documentation pages against the readers, is added:

- **Flowchart.** All of Mermaid's 46 named shapes for `@{ shape: … }`, each drawn and each with its own
  outline (document, lined document, stacked documents, notched card, lined rectangle, stacked
  rectangles, bolt, delay, horizontal cylinder, lined cylinder, display, divided rectangle, triangle,
  flipped triangle, window pane, filled circle, small circle, framed circle, crossed circle, sloped
  rectangle, hourglass, braces left and right and both, text, notched pentagon, fork bar, tagged
  rectangle and document, bow tie, paper tape, odd, and the existing fourteen). `@{ icon: … }` and
  `@{ img: … }` draw a labelled tile, because nothing is fetched. Edge ids (`e1@-->`) are read and
  dropped. Markdown strings drop their backticks and keep `**bold**` as words.
- **Block.** Block arrows, `id<["label"]>(direction)`, are drawn as a fat arrow in the named direction.
- **Gantt, treemap, xychart.** The refused forms in §1.

Anything else found while looking at the gallery in §6 is fixed in the same ticket and listed in its
closing comment.

### 4.1 What looking at the gallery found

Every one of these was a picture in the gallery that a person could not read, and each is fixed with a
test in its module:

| Type | What the picture showed | What changed |
|---|---|---|
| State | A composite state drawn twice, as a frame and as a separate box the arrows went to | The frame is the state; arrows to its name end at the frame; composites nest |
| State | `Still:::notMoving` read as a label and a second state; `%% comment` drawn as part of a name | Classes and trailing comments are taken off |
| Sequence | `participant API@{ "type": "boundary" }` made a second column named with its metadata | The block is read for a figure and an alias, six UML figures are drawn |
| Sequence | `Alice->>()John` made participants named `()John` | Central connections are read and drawn as small circles |
| Sequence | `rect rgb(…)` drawn as a frame with the colour written in its tab | A faint band of that colour, no frame, no words |
| Sequence | `#infin;` written out | Twenty seven named entities are read |
| Flowchart | A node first written outside a subgraph and mentioned inside it stayed outside | It moves into the subgraph, as Mermaid's own example shows |
| Class | Generics written with tildes, lollipop interfaces and notes refused or drawn as text | All three are read and drawn |
| ER | Word relationships (`only one to zero or more`) and subgraphs refused | Both are read |
| Gantt | A chart of times of day and a chart of plain numbers drawn as a heap of empty bars | `dateFormat` decides how a date is read; `vert` draws a line down the chart |
| Block | Each block one cell wide, so long words ran out of their boxes | Each column is as wide as the widest block in it |
| Git graph | A merge line ran through the commit id under the merge | Commit ids sit on a panel drawn after the lines |
| Kanban | `[Create Documentation]` kept its brackets | Bare brackets hold the words |
| Packet | One bit flags wrote their names over each other | Words shrink to fit a narrow field |
| Pie | A 3% share written across two slices, then over the title | Written outside on a leader, with room kept above and below the circle |
| Quadrant | Point names written over the quadrant titles | The titles go to the top of their quadrants when there are points |
| Requirement | `**bold**` and `__name__` written out, `:::class` in a name | Markdown marks and classes are taken off |
| Timeline | A two line section title ran out of its band | The band is as tall as its title |
| XY chart | The axis title written over the category labels | It has a line of its own |
| Wardley | A note crossed by a dependency line | Notes sit on a panel |
| ZenUML | `**Markdown**` in a comment written out | Markdown marks are taken off |

## 5. Shared pieces

`parts.rs` gains what more than one new type wants: a cylinder on its side, a stick figure, a person's
head, a folded note corner, a dashed frame with a title, and orthogonal routing between two rectangles by
named sides. Nothing in it knows what a C4 diagram is.

## 6. Testing

Three layers, and the third is the one the ticket asks for.

1. **Unit tests in each module**, with `FixedMetrics`, as every renderer already has: the reader reads
   every form, a malformed line is refused with its line number, and the scene keeps `check::properties`
   and, where the type has boxes that must not overlap, `check::no_two_rectangles_overlap` or
   `boxes_nest_or_miss`.
2. **The whole corpus.** `mermaid_audit` over the downloaded documentation must report no refusals and no
   markup left in text, and the overlap and escaping counts must be no worse than before. A test in
   `unluminous-core` walks every new sample and every type's `SAMPLES` entry, cut off at every byte, and
   asserts nothing panics.
3. **Looking.** The gallery test (`mermaid_gallery_of_every_block_in_a_folder`) photographs every block in
   the documentation through the real window, and every picture is opened and read: words inside their
   boxes, nothing covering anything, lines arriving where they should, nothing clipped. The screenshot
   suite gains one accepted picture a new type, from `sample-diagrams/`. Faults found by looking are fixed
   and the gallery taken again until a pass finds none.

## 7. Not done

- Icons from icon packs (`logos:aws-lambda`, `fa:user`) are not fetched. The icon's name is drawn instead.
- `style`, `classDef`, `UpdateElementStyle` and the other colour statements stay ignored, as before.
- Mermaid's C4 placement is not copied; §3.1 says why.
- Architecture's force directed layout is not copied; the grid in §3.2 is deterministic, which the
  screenshot tests need.
