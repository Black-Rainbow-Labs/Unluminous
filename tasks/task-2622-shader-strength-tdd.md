# task-2622: shader strength, pointer mapping under a curve, and rounded corners

The ticket: *"The unluminous shaders are too strong. We need to be able to dial up/down the impact of the effects.
Add a percentage slider, and have it be about 70% by default. Also, for effects like crt, where there is curvature,
we need to fix the mouse clicks/drags, so they account for where things are when that effect is in place. Also, when
the shader is applied, the rounded corners on the window are lost."*

## 1. What is there now

`task-2597` and `task-2612` draw Termoil's shaders in the vendored egui-wgpu's `AfterTheFrameHook`
(`services::terminal_shader::Passes`). Every rectangle a shader covers is a `Job`: a grid's (`terminal.shader`)
or the whole window's (`appearance.shader`). The hook copies that part of the frame into a texture and draws
`termoil_fragment` over it, which is `termoil_shade(uv, color)` from the shader file. The uniform block is
Termoil's: `size`, `cursor`, `time` and three padding floats.

Three faults follow from that:

- A shader is drawn at full strength, always. There is no way to have less of it.
- `crt` and `retro` bend the picture: the pixel at `uv` shows the frame at `curve(uv)`. Input is not bent, so a
  click lands on whatever is under the pointer in the unbent frame, which is not what the person sees there.
  Near the edges of the window that is several points off.
- The pass writes every pixel of its rectangle with `blend: None`. `crt` returns opaque black for every pixel whose
  bent position falls outside the frame, and most shaders return an opaque alpha, so the window's transparent
  rounded corners are painted over and the window becomes a square.

## 2. Strength

Two settings, both percentages from 0 to 100, both 70 by default:

| Setting | What it scales | Where it is set |
|---|---|---|
| `appearance.shader_strength` | the window's shader and cursor effect | `Settings -> Theme -> Shader -> Strength` |
| `terminal.shader_strength` | a terminal's own shader and cursor effect | `Settings -> Terminal -> Shader -> Strength` |

A terminal covered by the window's shader is drawn at the window's strength, because it is the window's shader.

The strength goes to the GPU in the first padding float of the uniform block, which the wrapper names
`strength` (0 to 1). The wrapper mixes the shader's answer with the frame it would have read:

```wgsl
let shaded = termoil_shade(in.uv, color);
let plain = termoil_frame_at(termoil_warp(in.uv));
var out = mix(plain, shaded, termoil.strength);
```

Mixing with `color` (the frame at `uv`) would be wrong for a shader that bends the picture: at 70% it would show
the bent picture over the straight one, two images 30% apart. So a shader that bends defines `termoil_warp(uv)`,
the position it reads, and scales its own curve by `termoil.strength`. The plain frame is read at that position,
so a bent picture mixed at 70% is one picture, bent by 70% of the curve, with 70% of the shader's color work.
`Look::source` adds `fn termoil_warp(uv) -> vec2<f32> { return uv; }` to a shader that does not define one.

`crt.wgsl` and `retro.wgsl` gain `termoil_warp` and read `CURVE * termoil.strength`. They are the only two of the
22 with a curve that does not move: `vhs`, `underwater` and `glitch` move the picture over time by a pixel or two,
which is not worth mapping input for. These two files are no longer byte copies of Termoil's; the comment at the
top of each says so. A shader written for Termoil still runs here unchanged, at the strength chosen.

## 3. Rounded corners

The window pass is told the window's corner radius in pixels, `size::WINDOW_CORNER` times the pixels per point, in
the second padding float (`radius`). The wrapper works out how far inside the window's rounded rectangle the pixel
is, with the usual rounded box distance and one pixel of feather, and keeps the frame's own pixel outside it:

```wgsl
out = mix(color, out, inside);
```

`color` there is the frame at `uv`, which is transparent outside the rounded corner, so the corners stay as
transparent as egui drew them. A grid's pass is given a radius of 0 and is unchanged.

## 4. Pointer events under a curve

`services::shader_warp` holds the curve on the processor, the same arithmetic as the WGSL:

```text
c = 2 * uv - 1
curve(uv) = (c * (1 + K * strength * |c|^2)) / 2 + 1/2
```

with `K` 0.0225 for `crt` and 0.03 for `retro`. Every pass that draws a bending shader records a `Region`: the
rectangle in points, `K * strength`, and for the window pass the grids it leaves to their own look. They are
recorded while the frame is drawn and read in `raw_input_hook` before the next pass, which is where every pointer
event the window receives passes before egui sees it. `PointerMoved`, `PointerButton` and `Touch` positions inside
a region are replaced with `curve` of their position in that region, so egui is told the pointer is over the thing
that is drawn under it. A grid region is asked before the window region, because a grid with its own look is not
bent by the window's pass.

The command line's `input` events go through the same hook, which is right: a position measured off
`window screenshot` is measured off the bent picture.

A cursor effect is drawn around the caret in screen space, so under a curve it was drawn where the caret would be
without the curve. The caret is moved through the inverse of the curve before it is sent, found with four Newton
steps, so the glow sits on the caret that is drawn.

## 5. Tests

- `shader_warp`: the curve is the identity at strength 0, the center does not move, a point near a corner moves
  outward, the inverse undoes the curve, and an event inside a region is moved while one outside is not.
- `terminal_shader`: every shader and cursor effect still parses and validates with the new wrapper, and exactly
  `crt` and `retro` define `termoil_warp`.
- `settings`: the two new keys are in the named settings list, read back, and default to 70.
- The settings list test in `tests/command_line.rs` gets the two help strings.

What cannot be tested without a graphics card is the picture itself. It is checked by hand in the installed build:
the CRT at 70% and 100%, the rounded corners, and a click near the window's edge under the CRT landing on the row
that is drawn there.
