# task-2612: Termoil's shaders and cursor effects over the whole window

*"We want unluminous to have shaders, similar to how termoil does, along with customizations for cursor effects like
termoil. Add the shaders to the theme section. The goal is to apply the shader to the entire window, and keep amazing
performance, rendering times, low cpu, etc. Add an option, which is off by default, for the terminal/termoil theme to
take precedence, and not have the main shader override it."*

## What exists

`task-2597` and `task-2601` draw Termoil's 22 shaders and 20 cursor effects over each terminal grid:

- The vendored egui-wgpu runs an `AfterTheFrame` hook after egui has drawn a frame into the scene texture, and before
  the frame is copied to the window or read for a screenshot.
- Each grid adds an empty paint callback (`terminal_shader::shade`). Its `paint` records the grid's rectangle in
  pixels as a `Job`. The hook copies that part of the frame into a texture and draws the shader back over it.
- `terminal.shader` and `terminal.cursor_effect` choose the look. `Look` joins a shader and a cursor effect with
  Termoil's own `compose`, and one pipeline is compiled for each pair.

## Research

- **Termoil** (`docs/shaders.md`) runs one shader over its whole window, and a shader that reads `termoil.time`
  redraws the window at the display rate. A shader that does not read the time costs nothing while the window is idle.
- **Ghostty** has `custom-shader-animation`, `true` by default: the focused window runs its animation loop, an
  unfocused one draws only when its contents change, and `always` keeps animating in the background. Its users report
  that a moving shader in the background is a cost they notice.
- **Windows Terminal** runs its `experimental.pixelShaderPath` over the whole terminal surface in one pass, the same
  model as Termoil.

So the window shader is one pass over the whole window, and it moves only while the window has the focus.

## Settings

| Key | Default | Meaning |
|---|---|---|
| `appearance.shader` | empty | A Termoil shader drawn over the whole window. Empty is none. |
| `appearance.cursor_effect` | empty | A Termoil cursor effect drawn around the caret that has the keyboard: the editor's caret, a text field's caret, or a terminal's cursor. Empty is none. |
| `appearance.terminals_keep_their_shader` | `false` | When on, a terminal that has its own `terminal.shader` or `terminal.cursor_effect` is drawn with that look, and `appearance.shader` is kept off the terminal's rectangle. |

All three are on `Settings -> Theme`, in a card called Shader below the theme list.

## What is drawn

| `appearance.shader` | `appearance.terminals_keep_their_shader` | Terminal grid | Rest of the window |
|---|---|---|---|
| none | either | `terminal.shader` and `terminal.cursor_effect`, as before | nothing, or only `appearance.cursor_effect` |
| set | off (default) | `appearance.shader`; the terminal's own look is not drawn | `appearance.shader` |
| set | on | the terminal's own look when it has one; `appearance.shader` when it has none | `appearance.shader` |

`appearance.cursor_effect` alone does not hide a terminal's own look. The terminal's own look is hidden only by
`appearance.shader`, because that is the case the ticket describes.

## How it is drawn

- **One pass for the window.** `services::window_shader::shade_the_window` runs at the end of `UnluminousApp::ui`
  and adds one empty paint callback the size of the window, which records a `Job` marked `whole`. The window's `uv`
  runs from one corner of the window to the other, so a curved tube or a vignette is the window's, not each pane's.
- **The terminals are cut out of the window pass.** In the hook, the `whole` job's holes are the pieces of every grid
  job that the grid's own pass draws (`uncovered(grid, grid.holes)`). A menu or a modal over a terminal is a hole in
  the grid's pass, so the window pass draws over it.
- **Every copy before every draw.** The hook copies every job's part of the frame first and draws second, so no pass
  reads pixels another pass already shaded.
- **The cursor is the caret with the keyboard**, in window pixels. The editor records its caret where it paints it, a
  focused terminal grid records its cursor, and otherwise egui's `PlatformOutput::ime` gives a text field's caret.
- **A modal's blurred backdrop is made from the frame without the shader.** The GPU blur reads the previous frame,
  which already has the window shader in it, and the window pass would then shade the backdrop a second time. The
  hook keeps its unshaded copy of the whole window, and the vendored egui-wgpu hands that copy to callbacks as the
  `SceneTexture` when there is one (`AfterTheFrame::unshaded`).

## Performance

- A shader that does not move costs no processor time: it is one copy and one full screen fragment pass on the
  graphics card, on frames that were being drawn anyway.
- A shader or cursor effect that moves asks for a frame every 33 ms (30 frames a second) **only while the window has
  the focus**, the rule Ghostty uses. An unfocused window does not animate and costs what it cost before.
- The pipeline for each shader and effect pair is compiled once and kept, as for the terminals.
- Measured with `UNLUMINOUS_FRAME_TRACE` and the process's processor time, idle, with and without a moving shader.

## Agents

- `theme shader [name] [--terminals keep|cover]` reads or sets `appearance.shader`, and with `--terminals`,
  `appearance.terminals_keep_their_shader`.
- `theme cursor-effect [name]` reads or sets `appearance.cursor_effect`.
- `theme shaders` and `theme cursor-effects` list the names, the same lists `terminal shaders` and
  `terminal cursor-effects` give.
- `settings set appearance.shader crt` reaches the same setting by name.

## Tests

- Unit: the window job's holes are the grids' uncovered pieces; the window look is in use for each row of the table
  above; `terminal_shader::shade` registers nothing while `appearance.shader` covers the terminals.
- Command line: `theme shader`, `theme cursor-effect` and `--terminals` set and read back the three settings, and an
  unknown name is refused.
- Settings round trip carries the three keys.
- Screenshots of a real window: no shader, a window shader over a terminal that has its own shader with the toggle
  off, and the same with the toggle on.
