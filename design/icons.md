# The `material` icon set, and where its design came from

`theme::icon` draws two sets of marks. `classic` is what Unluminous shipped with; `material` is the one
`task-1776` added and the one a window comes up in. This records how the second one was designed, so
it can be designed again — the same thing `plugins/mermaid/icon.md` and `plugins/css/icon.md` record
for the file-type icons.

## The marks are drawn, and the pictures are the design

Nothing in `theme::icon` is a bitmap, and `design/style-guide.md` gives the reason rather than a
preference: every mark here is tinted where it is used — `color::icon()` sitting there,
`color::icon_active()` when its pane is open, `color::icon_disabled()` when it cannot be used — and it
is drawn again at whatever the pane is zoomed to. A picture is one colour at one size.

So the image service was used for what it is good for: **the design**. Two sheets were generated,
read for stroke weight, fill, corner radius and silhouette, and the Rust was written against them.
The sheets live in `_agent_output/task-1776-themes/` rather than here, because they are working
material rather than the thing a component is measured against.

## Making the sheets again

Through the AI service's `POST /image-creation/generateImageToProjectFile`, which renders with Krea 2
and writes a verified PNG straight into this repository. It needs the local tooling token, which
every agent terminal has.

```bash
curl -s -X POST http://localhost:8091/image-creation/generateImageToProjectFile \
  -H 'Content-Type: application/json' -H "x-skip-token: $CLAUDE_SKIP_TOKEN" \
  -d '{
    "prompt": "A flat vector icon sheet on a 4 by 2 grid, eight separate developer-tool glyphs evenly spaced with generous margins between them, all in one single solid light grey colour on a completely flat solid dark navy background. The eight glyphs, in order: a filled folder with a raised tab, a rounded rectangle split into two panes, a git branch of two dots joined by a curved line, a terminal window with a chevron prompt, a filled play triangle, a rounded bug with two antennae and legs, a kanban board of three vertical columns, a rounded speech bubble. Material Design style: heavy even shapes, filled rather than outlined, rounded corners and round line caps, geometric, perfectly aligned on the grid, minimal, crisp, very high contrast. No text, no letters, no words, no numbers, no gradients, no shadows, no perspective, no white outlines, no colour variation.",
    "negativePrompt": "text, letters, words, numbers, watermark, signature, photorealistic, 3d render, gradient, drop shadow, noise, texture, busy background, clutter, person, face, hand, multiple colours, colourful, rainbow, white outline, stroke, border, frame, thin hairlines, skeuomorphic",
    "width": 1024, "height": 1024,
    "projectId": "unluminous",
    "relativePath": "_agent_output/task-1776-themes/icons-rail-material.png",
    "transparentBackground": false,
    "timeoutMs": 900000
  }'
```

The second sheet is the same call with the explorer's six — a chevron right, a chevron down, a closed
folder, an open folder, a magnifier and a plus — and `arrow head, triangle` added to the negative
prompt, because without it a "chevron" comes back as an arrow with a head on it.

**One colour, and say so three times.** The first thing to get wrong is asking for a set of icons and
being given a *colourful* set: the sheet is read for shape, and a mark drawn in three colours cannot
be read for a silhouette. `all in one single solid light grey colour`, plus `multiple colours,
colourful, rainbow` in the negative prompt, is what it takes.

**`transparentBackground` is off here**, unlike the plugin icons. Nothing is cut out of these sheets —
they are looked at, not used — and the flat navy ground is what makes the shapes readable.

## What was taken from the sheets, and what was not

| Mark | What the sheet gave |
|---|---|
| `folder` | The silhouette: a filled body with a raised tab on the left and a **step** where the tab meets it. The body's top left corner is square when the folder is shut, or the round leaves a notch that reads as a bite. |
| `editing_area` | **Two slabs side by side**, which is better than the panel-with-a-tab that was drawn first: Unluminous's editing area *is* a row of panes, and a panel with a tab could as easily have meant the explorer. |
| `terminal` | A window with a filled title bar and a chevron prompt in the body. |
| `disclosure` | A chevron of two thick strokes with round caps, taller than it is wide. |
| `bug` | A filled body with stroked legs and antennae, which is the set's rule in one mark: mass is filled and limbs are lines. |
| `board` | Three columns **under a header bar**. Without the bar it is a chart rather than a board. |
| `chat` | A filled bubble with a tail, and no lines of words in it — inside a fill they would have to be painted in the ground, which is the one thing this set does not do. |
| `branch` | **Nothing.** The sheet's git branch came back as an X with four dots, which says nothing about git. The mark drawn instead is the ordinary three-commit fork, at the set's weight and caps. |

That last row is what a design reference is for: the parts of it that are better than what you would
have drawn are copied, and the parts that are not are not.

## `task-1949` redrew ten of them, and the reference was a named icon rather than a sheet

The sheets above are what the set was *designed* from. `task-1949` is the other way round: the ticket
named a particular icon from a particular published set for each mark it wanted changed, so there was
nothing to generate and nothing to read for a silhouette — the shape was already decided and the work
was drawing it in points.

**The path data was fetched and read, not remembered.** A drawing made from a recollection of what
`LuBotMessageSquare` looks like is a drawing of something else with the right name on it. Each
reference was pulled as its own SVG and converted: the viewBox maps onto Unluminous's points as
`p = (v - centre) * scale`, and the numbers in `theme::icon` are that arithmetic.

| Mark | Reference | Where it came from |
|---|---|---|
| `editing_area` | Circum `CiFileOn` | `react-icons@5.5.0/ci/index.mjs` on unpkg — Iconify has no `circum` set |
| `folder` | Circum `CiFolderOn` | the same file |
| `branch` | Ionicons 4 `IoIosGitBranch` | `api.iconify.design/ion:ios-git-branch.svg` |
| `database` | Tabler `TbDatabase` | `tabler/tabler-icons` `icons/outline/database.svg` |
| `chat` | Lucide `LuBotMessageSquare` | `lucide-icons/lucide` `icons/bot-message-square.svg` |
| `board` | Lucide `LuClipboardList` | `icons/clipboard-list.svg` |
| `terminal` | Lucide `LuSquareTerminal` | `icons/square-terminal.svg` |
| `debug_run` | VS Code `VscDebugAlt` | `microsoft/vscode-codicons` `src/icons/debug-alt.svg` |
| `bug` | Line Awesome `LiaBugSolid` | `api.iconify.design/la:bug-solid.svg` |
| `run` | Font Awesome `FaPlay` | `api.iconify.design/fa6-solid:play.svg` |
| `wrench` | Lucide `LuWrench` | `lucide-icons/lucide` `icons/wrench.svg` (`task-2060`) |

Three of them are **not** what the reference draws, and each is a decision rather than a slip:

- **The explorer keeps the filled folder.** `folder` on the rail is Circum's thin outline, and the
  mark in front of every folder row is still `material_folder`. A column of thin outlines down a file
  tree is texture rather than information, and the filled mark is what carries `color::folder`
  against `color::folder_open`, which is the whole of Atom Material Icons' idea.
- **The bot bubble has no ears.** Lucide draws a one point tick a point clear of the bubble on each
  side. At the twelve physical pixels this occupies on an ordinary display that is a speck beside the
  mark rather than a part of it.
- **`debug_run` is a beetle with a play badge, where VS Code's is a large play outline with a bug
  across it.** VS Code's balance works at the sixteen pixels VS Code draws it at. It was drawn that
  way first, photographed in the real title bar, and neither half could be made out — so one shape
  carries the mark and the other says what kind of start it is, and the beetle carries it because it
  is the half that differs from the Run button beside it.
- **The wrench's head is a stroked ring rather than a filled one.** Lucide's `wrench` is one filled
  path: a solid head with a bite out of it and a solid tapering handle. Every other mark in
  `theme/icon.rs` is a stroke at twelve points across, and a filled head beside them reads as a blob
  rather than as a tool. The bite, the diagonal and the proportion of head to handle are the
  reference's; the fill is not. `task-2060` added it for the tool calls in the chat pane.

## Looking at one, properly

`crates/unluminous-app/tests/icons.rs` draws **every** mark on one sheet at eight pixels a point, one
sheet per icon set, and the two sheets are accepted pictures like any other — so redrawing a mark
tomorrow fails that test and the failure is a picture somebody has to open.

```sh
UPDATE_SNAPSHOTS=1 cargo test -p unluminous-app --test icons
```

It exists because until `task-1949` there was no way to see an icon at all. A mark is twelve points
across, so in a picture of the whole window it is a dozen pixels and a change to it is a smudge
moving — which is how a folder, a chat bubble, a clipboard and a beetle could all be redrawn at once
without one of the 483 accepted pictures failing. The sheet caught two things that would otherwise
have shipped: the first beetle read as a spider, and the first `debug_run` was two shapes of equal
weight that came out of the real window as a smudge.

**And the sheet is not the last word.** It is eight times life size, and an icon has to work at one:
every mark `task-1949` changed was also read off a photograph of the real built window at the size a
person sees it. The sheet is for deciding whether a stroke meets a stroke; the window is for deciding
whether anybody can tell what it is.

## Nothing is painted in the background

An icon is drawn over four different grounds — the rail, the rail's own pill when its pane is open, a
menu row and a flyout — so a shape "knocked out" of a fill by painting it in `color::editor()` would
be right in one place and wrong in the other three. Every mark in the set is one colour on whatever
is behind it. Where the design sheet knocked a shape out, the Rust uses a stroke and a fill instead.

## Looking at one inside a real picture

The sheet above is the whole set. To blow a mark up out of a picture of the window instead — which is
how the first branch icon was caught reading as a question mark:

```bash
cargo run --example crop  -- crates/unluminous-app/tests/snapshots/windows/git_commit_panel.png out.png 3 50 32 200
cargo run --example scale -- out.png out-8x.png 8
```
