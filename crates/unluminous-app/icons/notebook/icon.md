# Making the notebook icon again

`icon.png` (32 by 32) and `icon-128.png` are the picture in front of a `.ipynb` file, in the explorer
and on its tab (`task-2220`). They were made the way the bundled plugins' icons are, which
`plugins/python/icon.md` records in full.

The mark is a notebook page with three rings along its top and a play triangle on it: a notebook that
runs. It is not Jupyter's logo.

## 1. Render it

```bash
curl -s -X POST http://localhost:8091/image-creation/generateImageToProjectFile \
  -H 'Content-Type: application/json' -H "x-skip-token: $CLAUDE_SKIP_TOKEN" \
  -d '{
    "prompt": "A flat vector app icon: one bold open notebook page with three spiral rings along its top edge and a solid play triangle in the middle of the page, centred, over a single large rounded square colour swatch behind it. Two solid colours only plus the background. Bold even shapes, geometric, perfectly centred, generous margin, no outlines around the shapes. Bright warm orange page, deep teal swatch, on a completely flat solid dark navy background. Minimal, crisp, high contrast, icon design, no text, no letters, no words, no gradients, no shadows, no perspective, no white outline.",
    "negativePrompt": "text, letters, words, numbers, watermark, signature, photorealistic, 3d render, gradient, drop shadow, noise, texture, busy background, clutter, person, face, hand, lines of writing, pen, pencil, white outline, stroke, border, planets, moons, circles orbiting",
    "width": 1024, "height": 1024,
    "projectId": "unluminous",
    "relativePath": "_agent_output/task-2220-notebooks/icon/notebook-icon-source.png",
    "transparentBackground": true,
    "timeoutMs": 600000
  }'
```

## 2. Turn it into the two icons

```bash
cargo run -p unluminous-app --example plugin_icon -- _agent_output/task-2220-notebooks/icon/notebook-icon-source.png crates/unluminous-app/icons/notebook
```

## 3. Look at it

Open `icon.png` at its own size. It is the size a tab and an explorer row draw it at.
