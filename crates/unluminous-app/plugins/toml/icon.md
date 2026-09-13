# Making this plugin's icon again

`icon.png` (32 by 32) and `icon-128.png` are generated on this machine rather than drawn by hand,
which is how the other bundled plugins' icons were made — `plugins/css/icon.md` and
`plugins/mermaid/icon.md` record the same recipe. Written down here so the icon can be made again
without guessing.

TOML's own community mark is a cat sitting on a curled page, which belongs to the project rather than
to Unluminous. The mark drawn here instead is a bracket holding an equals sign — TOML's two most visible
pieces of punctuation, a section header's `[` and a key's `=` — so it stays a description of the
syntax rather than a borrowed mascot.

## 1. Render it

Through the AI service's `POST /image-creation/generateImageToProjectFile`, which renders with Krea 2
and writes a verified PNG straight into this repository. It needs the local tooling token, which
every agent terminal has.

```bash
curl -s -X POST http://localhost:8091/image-creation/generateImageToProjectFile \
  -H 'Content-Type: application/json' -H "x-skip-token: $CLAUDE_SKIP_TOKEN" \
  -d '{
    "prompt": "A flat vector app icon: one bold square bracket shape open on the right, enclosing a small thick horizontal equals sign made of two parallel bars, centred, over a single large rounded square colour swatch behind it. Two solid colours only plus the background. Bold even shapes, geometric, perfectly centred, generous margin, no outlines around the shapes. Bright cyan bracket and bars, magenta swatch, on a completely flat solid dark navy background. Minimal, crisp, high contrast, icon design, no text, no letters, no words, no gradients, no shadows, no perspective, no white outline.",
    "negativePrompt": "text, letters, words, numbers, watermark, signature, photorealistic, 3d render, gradient, drop shadow, noise, texture, busy background, clutter, person, face, hand, cat, animal, white outline, stroke, border",
    "width": 1024, "height": 1024,
    "projectId": "unluminous",
    "relativePath": "_agent_output/task-1922-language-plugins/toml-icon-source.png",
    "transparentBackground": true,
    "timeoutMs": 600000
  }'
```

`transparentBackground` matters: the renderer only emits opaque pixels, so without it the icon is a
navy square rather than a mark, and an explorer row would show a rectangle of the wrong colour behind
every `.toml` file. And the negative prompt says `cat`: asking for a TOML icon otherwise tends towards
the project's own mascot, which is exactly the mark this one is not meant to be.

## 2. Turn it into the two icons

```bash
cargo run --example plugin_icon -- _agent_output/task-1922-language-plugins/toml-icon-source.png crates/unluminous-app/plugins/toml
```

`examples/plugin_icon.rs` keys the flat background out by flood filling from the four corners, crops
to the mark with an even margin, squares it, and scales it to 128 and to 32 with a smooth filter.

## 3. Look at it

```bash
cargo run --example scale -- crates/unluminous-app/plugins/toml/icon.png _agent_output/task-1922-language-plugins/toml-icon-large.png 8
```

The 32 by 32 one is the one to check, because it is the size a tab and an explorer row use and it is
where a mark with too much detail in it turns to mush.
