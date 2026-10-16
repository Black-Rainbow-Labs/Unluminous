# task-2281: the Unluminous Glass theme, with refraction

## What was asked

A new theme in which the surfaces are glass that **bends** the picture behind it, the way
`https://real-glass.vercel.app` does, rather than blurring it. Jason designed it in Claude Design
(canvas "Unluminous", page "Agent-Chat 2": `ChatPaneGlass`, `ChatSettingsGlass`, `ChatPartsGlass`), and
the design agent's handoff says how the browser version works: an SVG backdrop filter per element, a
displacement map per element size, three `feDisplacementMap` passes at 1.0, 0.92 and 0.84 of the
strength for a faint colour split, and a translucent tint with lit edges painted over it. The ticket
asks for the same thing in Rust and `rux`, drawn cheaply, checked by looking at it, and carried through
every screen Unluminous has.

## What the design is made of

Read out of `ChatPaneGlass.dc.html`:

| Name in the design | Value | What it is on |
|---|---|---|
| backdrop | a sharp picture of glossy spheres on fine diagonal pinstripes, no blur | the pane |
| `--card` | `linear-gradient(135deg, rgba(255,255,255,.13), rgba(255,255,255,.02) 60%)` over `rgba(10,12,20,.34)` | every card, bubble, round button, chosen pill |
| `--well` | `rgba(6,8,14,.42)` | every carved well: a code block, a track, the prompt box |
| `--out` | `0 16px 36px rgba(0,0,0,.38)`, `inset 0 1px 0 rgba(255,255,255,.38)`, `inset 0 -1px 0 rgba(255,255,255,.10)`, `inset 0 0 0 1px rgba(255,255,255,.14)`, `inset 0 0 22px rgba(255,255,255,.05)` | a card |
| `--out-sm` | `0 6px 16px rgba(0,0,0,.34)`, `inset 0 1px 0 rgba(255,255,255,.42)`, `inset 0 -1px 0 rgba(255,255,255,.10)`, `inset 0 0 0 1px rgba(255,255,255,.16)` | a bubble, a round button |
| `--in` | `inset 0 1px 3px rgba(0,0,0,.35)`, `inset 0 0 0 1px rgba(255,255,255,.10)`, `0 1px 0 rgba(255,255,255,.06)` | the prompt box, the dial's inner well |
| `--in-sm` | `inset 0 1px 2px rgba(0,0,0,.3)`, `inset 0 0 0 1px rgba(255,255,255,.08)` | every other well |
| `--grad` | `linear-gradient(135deg, rgba(61,120,255,.88), rgba(92,182,255,.72))` | the one action |
| `--grad-out` | `0 6px 20px rgba(40,100,255,.35)`, `inset 0 1px 0 rgba(255,255,255,.45)`, `inset 0 0 0 1px rgba(255,255,255,.22)` | the one action |
| text | `--ink-900` with `text-shadow: 0 1px 2px rgba(0,0,0,.7), 0 0 12px rgba(0,0,0,.5)` | words drawn straight on the backdrop |
| modal | the card gradient at `.14`/`.03` over `rgba(12,14,22,.40)`, a 3 px blur before the displacement, `0 24px 60px rgba(0,0,0,.5)` | the Chat settings dialog |

The refraction itself, per element, is two terms added together:

- **the rim**: for `bevel` points inside the edge, the picture is sampled from *outside* the shape,
  along the edge's outward normal, by `t^2.2` where `t = clamp(1 + d/bevel, 0, 1)` and `d` is the signed
  distance to the rounded rectangle (negative inside);
- **the lens**: across the whole interior, the picture is sampled further from the centre the further
  the point is from the centre, `(p - centre)/(half size) × 0.42 × clamp(-d/bevel, 0, 1)`, with the
  vertical term scaled by `min(1, h/w × 2.2)`. This magnifies, so the pinstripes curve into the shape.

The sample point is `p + offset × strength × 0.5`, once for each of red, green and blue with the
strength scaled by 1.0, 0.92 and 0.84. The strengths in the design are `1.7 ×` a base of 16 for a 38
point circle, 12 for 30, about a third of the diameter for a larger circle, 22 to 26 for a bubble, 18
for a small pill, 28 for the prompt box, 36 for a card and 40 for the modal. The bevels are about 0.42
of a circle's diameter, 16 to 20 for a bubble, 14 for a pill, 22 for the prompt box and 28 to 30 for a
card and the modal.

## What Unluminous has to sample

Refraction needs the picture behind the glass. egui draws every layer straight into the frame in one
render pass and cannot read back what it has already drawn (the reason `services::backdrop_blur`
photographs the window). So the glass cannot sample "whatever is under it". It can sample a picture
Unluminous already holds as a texture:

- **the background picture** (`appearance.background.image`), which `services::backgrounds::Wallpaper`
  keeps on the graphics card as strips of about a megabyte each (`task-2279`). That is the design's
  backdrop exactly: the sharp picture the panes sit on.
- **the modal backdrop**, the shrunk and blurred photograph of the window `services::backdrop_blur`
  takes when a modal opens. Glass inside a modal samples this one, which gives the design's frosted
  modal for nothing: the 3 px blur is already in the picture, and what bends under the dialog is the
  window's own content.

With no picture chosen the desktop shows through, and there is nothing to bend. Glass then draws its
tint and its lit edges and no refraction, which is the design's own fallback for a browser without
SVG backdrop filters.

## How the bending is drawn: three ways weighed

**A. A wgpu paint callback with a shader**, as the handoff's §3e suggests. The maths is a few lines of
WGSL and costs nothing on the card. Refused for three reasons, each measured against the code rather
than assumed. The callback's `prepare` is called while `egui_wgpu::Renderer` is write locked, so it
cannot reach egui's textures, which is where the background picture is; the picture would have to be
uploaded a second time into a texture of the callback's own, which is the 8 MB the `task-2279` memory
work took away. The screenshot tests render through `egui_kittest`, and a callback is a second render
path to keep working there. And every callback ends egui's batch, so a pane of forty glass buttons is
eighty draw calls and forty pipeline switches.

**B. Refraction on the processor, into the `vello_cpu` decoration.** The decoration is already
rasterised on the processor whenever a layer changes. Each glass pixel would read three pixels of a
copy of the picture. Refused: it needs a second copy of the picture in memory, and a chat pane is
about a million glass pixels, so every frame of scrolling it would cost several milliseconds more than
it does now.

**C. An egui mesh that draws the picture with bent texture coordinates. Chosen.** The refraction is a
change of *where* the picture is sampled, and a mesh already carries that: every vertex has a texture
coordinate and the card interpolates between them. So each glass shape becomes a small mesh whose
vertices sit on rings parallel to its edge, and whose texture coordinates are where the displaced
sample lands in the background picture. The picture's own strips are the textures. Nothing new is
uploaded, the card does a texture fetch per pixel as it does for any picture, and it draws through
egui's own pipeline, so the window and the screenshot tests draw it the same way.

Why it is accurate, not an approximation in a way anybody can see: along a straight edge both terms
change linearly (the normal is constant and the lens term is linear in `x`), so the mesh needs no
points along a straight edge at all; it needs points round each corner, and rings across the rim,
where `t^2.2` curves. Eight rings across the rim and up to ten segments round a corner give a
piecewise linear `t^2.2` within half a point of the true curve at the design's strengths. The interior
inside the innermost ring is a fan to the centre, and there the lens term is exactly linear, so the fan
is exact. A 424 by 152 card is about 400 vertices.

**The colour split is three passes that add.** egui blends premultiplied: `src + dst × (1 - src.a)`.
A vertex colour with alpha zero therefore adds. The red pass is drawn with colour `(a, 0, 0, a)`,
which covers what was behind by the glass's own opacity `a` and puts back red; the green and blue
passes are drawn with `(0, a, 0, 0)` and `(0, 0, a, 0)` at their own strengths, which add their
channel and cover nothing. The three passes are one mesh, so one glass shape is one draw item per
strip of the picture it touches. A ring half a point outside the edge, whose colour is zero, gives the
edge the same feather egui gives a rounded rectangle, so the glass is antialiased.

**The strips.** The picture is held in strips, and a displaced sample can land in any of them. Each
triangle of the mesh is cut at the strip seams in texture space, exactly as `Picture::paint` already
cuts the wallpaper, and each piece is drawn with its strip's texture. A sample that lands outside the
picture is clamped to its edge, which is what `feDisplacementMap` does too.

## Where it lives

### In `rux`

`rux` is where a surface is decided, so it is where glass is decided. Four additions.

1. **`theme::Glass`**, a material: the lens (0.42), how the bevel and the strength follow from a
   shape's size, the three channel strengths, and the design's tint, edges and shadows as a `Chat`
   worked out for glass. `Theme::glass: Option<Glass>` is `None` in both shipped themes, so nothing
   that exists today changes. `Theme::glazed(&self)` gives the same theme with glass: the card, the
   chip, the well, the bubble, the action and the shadows become the table above. This is the one place
   a theme changes a *recipe* rather than a value, and that is what a material is; the rule that both
   themes share every recipe still holds for the two themes it was written about.
2. **`Decor::Glass { rect, corners, bevel, strength }`**, recorded by `Chrome::surface` before the
   surface's fill when the layer's theme has glass and the surface is raised (it has an outer shadow)
   and its fill is translucent. The rule is in `render`, not in a component, so no component branches
   on the theme. A component that draws a carved well gets no glass; the design's prompt box is a well
   with an outer lit edge, so it gets glass by the same rule.
3. **`render::Backdrop`**: the picture glass bends, as a list of bands (a texture and the rows of the
   picture it holds), where the picture is drawn on the screen, which part of it is drawn, and its
   opacity. An application sets one per window with `render::set_backdrop(ctx, order, backdrop)`, where
   `order` is `Background` for the panes and `Foreground` for modals and popups. It is kept in egui's
   memory, so every `RuxState` in the window sees it with nothing passed through.
4. **`render::layer` splits the decoration at each glass item.** Today a layer is one texture painted
   into one slot. With glass in it, the list is cut into runs: the decoration before the first glass,
   the glass mesh, the decoration after it, and so on, and the slot is filled with all of them in that
   order. That order is what makes a chosen pill in a carved track, a knob on a slider and a button on a
   card come out right: the well is painted over the card's glass and under the pill's. A layer with no
   glass is exactly one texture, as now. The meshes are kept per layer and built again only when the
   shape, the backdrop or the layer's transform changes, so a still frame costs a comparison.

A layer drawn through a transform (a modal that is zoomed, a canvas node) is handled by mapping each
vertex through `ctx.layer_transform_to_global` before working out its texture coordinate, so the glass
samples the picture where it really is on the screen.

### In Unluminous

1. **`Theme::unluminous_glass()`**, key `unluminous/glass`, called Unluminous Glass. Dark. Its palette
   is the dark palette with the grounds turned toward the design's near black (`#0B0D14`), its accent the
   design's sky blue, and `glass: true`. A theme plugin may say `theme.<id>.glass = true` too.
2. **`theme::veil()`**: how much of the ground a pane paints over the picture. `faded` multiplies the
   opacity setting by it. 1.0 in every other theme, so nothing else moves; 0.42 in glass, so the picture
   behind the panes is seen, as in the design, and the opacity slider still decides how much. The editing
   area is painted a second time under glass, so code over a bright sphere stays readable.
3. **`theme::rux_theme`** glazes the retoned `rux` theme when the active theme is glass, so every
   surface drawn through `rux` (the chat, the board, the Settings window, every modal, the title bar,
   the rail, the realm bar, the database pane, the designer) is glass with no change to its own code.
   The glazed ground is translucent too, so a dialog that paints its own surface in it is glass as well.
4. **The backdrop.** `paint_the_background` hands `rux` the picture's strips and where they are drawn,
   every frame. `modal::show` hands over the blurred photograph for whatever floats above the page.
5. **What egui frames itself.** A menu, a dropdown, a submenu, a context menu and a tooltip are egui
   frames rather than `rux` surfaces. Under glass they are filled in the card's tint
   (`theme::glass::popup_fill`), and once the frame is drawn `theme::glass::glaze_the_popups` puts the
   bent picture under every rectangle of that colour on a layer above the page. Found by colour rather
   than by a list of menus, so a popup added later is covered.
6. **Canvas nodes.** A node's card is painted into the node's own layer to hide the nodes behind it.
   Under glass its tint is translucent, so `theme::glass::paint_glass` draws the bent picture under it at
   full strength first, and with no picture an opaque ground does the hiding.
7. **A picture to bend.** The design's backdrop is portrait and was made for one pane. A landscape one
   is generated for the window, `glass-spheres.jpg`, spheres on pinstripes at 1920 by 1088, drawn by
   `tools/glass-spheres.py`, and ships with the other five. Choosing the Glass theme while no picture is
   chosen chooses it, because glass over the desktop has nothing to bend; choosing any picture afterwards
   is the person's.

The design's dark halo under words drawn straight on the picture was not needed: under glass every pane
keeps a smoke over the picture, and the words read on it in every screen photographed.

## What it costs

The budget is that a still frame costs nothing new, and a frame that moves glass costs less than a
millisecond more than the same frame in Unluminous Dark. The vertex count is about 300 a shape, times
three passes. Building them is arithmetic on a few hundred points a shape, and `rux` keeps the shapes
until the surface, the picture or the layer's transform moves.

Measured on the release build with `UNLUMINOUS_FRAME_TRACE`, scrolling the chat's component gallery
eighty notches, two runs of each theme interleaved: egui's own time per frame (the `egui-cpu` phase,
which includes tessellating the meshes) had a median of 2.30 and 2.50 ms under glass against 1.89 and
2.05 ms under Unluminous Dark, so about 0.4 ms more. The rest of the frame was the same.

## Tests

- `rux`: the map's maths against the handoff's Python at sample points (the rim pulls outward, the
  lens pulls from the centre, zero at the centre); a raised translucent surface records `Glass` under a
  glazed theme and not under a shipped one; a layer with glass is cut into runs in order; the mesh
  stays inside the shape plus its feather; a still layer builds no new mesh; the three passes add.
- Unluminous: the glass theme is listed and chosen by `theme set`; its palette is readable (the
  contrast test is extended to it); choosing it with no picture chooses the glass picture; screenshot
  tests of the chat pane, the board, the Settings window and a modal in the glass theme, looked at
  before they are accepted.
- The installed window, driven with `unluminous-cli`, photographed in the glass theme on every main
  screen, and looked at.

## Left out

- **Glass inside glass bends the picture, not the glass under it.** The browser bends the parent's
  already bent picture. Doing that would need the frame read back, which egui cannot do. At the
  design's strengths the difference is a few points of extra bend inside a nested button.
- **Bending the desktop.** No picture of the desktop is available to a window, so glass over the
  desktop is tint only.
- **Glass in the light theme.** The design is dark; a light glass would be a second design.
