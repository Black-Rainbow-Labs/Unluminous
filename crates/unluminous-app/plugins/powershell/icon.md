# Making this plugin's icon again

`icon.png` (32 by 32) and `icon-128.png` are generated on this machine rather than drawn by hand, the
way every bundled plugin's icon is (`plugins/shell/icon.md` has the longer version of this recipe).
Made for `task-2384`, with the other twenty six icons that ticket added.

The mark is a bold chevron pointing right with a short underscore beside it, white chevron, inside a slanted royal blue parallelogram tile. It is a generic shape rather than the project's own logo, so the icon copies no
trademark, and its colour is what tells it apart from the other icons in an explorer row.

## 1. Render it

Through the AI service's `POST /image-creation/generateImageToProjectFile`, which renders with Krea 2.

```bash
curl -s -X POST http://localhost:8091/image-creation/generateImageToProjectFile \
  -H 'Content-Type: application/json' -H "x-skip-token: $CLAUDE_SKIP_TOKEN" \
  -d '{
    "prompt": "A flat vector app icon: a bold chevron pointing right with a short underscore beside it, white chevron, inside a slanted royal blue parallelogram tile, centred, bold even shapes, geometric, perfectly centred, generous margin, no outlines around the shapes, on a completely flat solid dark navy background. Minimal, crisp, high contrast, icon design, no text, no letters, no words, no gradients, no shadows, no perspective, no white outline.",
    "negativePrompt": "text, letters, words, numbers, watermark, signature, photorealistic, 3d render, gradient, drop shadow, noise, texture, busy background, clutter, person, face, hand, white outline, stroke, border",
    "width": 1024, "height": 1024,
    "projectId": "unluminous",
    "relativePath": "_agent_output/task-2384-common-file-types/icons/powershell-icon-source.png",
    "transparentBackground": true,
    "timeoutMs": 600000
  }'
```

## 2. Turn it into the two icons

```bash
cargo run --example plugin_icon -- _agent_output/task-2384-common-file-types/icons/powershell-icon-source.png crates/unluminous-app/plugins/powershell
```

Check the 32 by 32 one, because that is the size a tab and an explorer row draw.
