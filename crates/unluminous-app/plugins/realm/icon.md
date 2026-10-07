# Making this plugin's icon again

`icon.png` (32 by 32) and `icon-128.png` were generated on this machine rather than drawn by hand, which
is how the other bundled plugins' icons were made. `plugins/mermaid/icon.md` and `plugins/json/icon.md`
record the same recipe. Written down here so the icon can be made again without guessing.

The mark is a magenta ring with a cyan hub and four cyan nodes on spokes inside it: the ring is the realm,
a bounded canvas, and the hub and nodes are the things on it, connected. It uses the cyan and magenta the
Mermaid, JSON and Database icons use, flat, on a transparent background.

## 1. How it was rendered

Ideogram 4 through the AI service, `POST /image-creation/createImageIdeogram`, 1024 by 1024, quality mode,
for `task-2199`. Eleven candidates in two rounds, in the AI service's projects 332 and 333. The prompts
were JSON captions with a full frame background box, and the palette was background `#1B1F2A`, cyan
`#22B8E6`, magenta `#D946EF` and pink `#E91E63`. The full captions are the `*.meta.json` files under
`C:\jason\dev\ai-service\_agent_output\task-2199-realm\icon\round1\` and `round2\`.

The winner is round 2's `d2-heavy-b`, prompt id `3c9f33ba-e0dc-4704-bfbc-4fa45393fec5`: round 1's best
candidate (a magenta ring with a hub and four spokes) redrawn with heavier spokes and larger nodes, so it
still reads at 16 pixels.

## 2. How each candidate was graded

Three questions: does it read at 16 pixels, does it match the other plugins' style, and does it say
"a canvas of connected things". Out of ten:

| Candidate | Idea | Grade |
|---|---|---|
| c1 portal ring | cyan ring, four magenta nodes joined by thin lines | 6, the nodes vanish at 16 px |
| c2 portal three | cyan ring, three nodes | 5, reads as a share button |
| c3 rounded square constellation | outline square, four nodes | 6, the lines read as a Z |
| c4 filled square nodes | pink square, three cyan nodes | 6, reads as a warning triangle |
| c5 horizon graph | a horizon line and three nodes | 4 |
| c6 infinite nodes | an infinity loop with nodes | 4 |
| c7 canvas frame | viewfinder corners and three nodes | 6, best meaning, weak at 16 px |
| c8 orbit | magenta ring, hub with four spokes | 7, spokes too thin at 16 px |
| d1 heavy | c8 with heavier spokes | 6, the nodes overlap the ring |
| **d2 heavy b** | c8 with heavier spokes and larger nodes | **8** |
| d3 three | three spokes | 6, reads as a warning triangle |

## 3. Turn a source into the two icons

The background was keyed to transparent, including the inside of the ring, and the picture cropped square.
`realm-icon-1024.png` beside the report is that source, and the two sizes here are made from it with:

```bash
cargo run --example plugin_icon -- C:/jason/dev/ai-service/_agent_output/task-2199-realm/icon/realm-icon-1024.png crates/unluminous-app/plugins/realm
```

## 4. The drawn mark

The rail button and the panel's header are drawn in code rather than from this picture, so they follow the
window's colours: `theme::icon::realm` is the same ring, hub and four nodes in one colour, copied from
`realm-icon.svg` (two colours, 128 units across) beside the report.
