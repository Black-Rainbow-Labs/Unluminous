The 37 rux marks, drawn as stroked SVG in the current text colour.

Ported from `crates/rux/src/icon/paths.rs`, which keeps the `SbIcon` path data from AI Studio verbatim. `tools/extract-rux-icons.mjs` reads that file, so a mark here is the mark the Rust draws.

## When to use
Inside a control (`Button` `icon`, `IconButton`, `Select`, `ListItem`), or on its own beside a label. An icon takes the colour of the words around it, so set `color` on the parent rather than tinting the icon.

## Props
| Prop | Type | Default | Notes |
|---|---|---|---|
| `name` | one of the 37 names | | `search` `plus` `x` `check` `chevDown` `chevLeft` `chevRight` `arrowRight` `home` `docs` `spark` `dots` `film` `storyboard` `layers` `folder` `settings` `bell` `user` `trash` `download` `heart` `star` `chat` `cube` `image` `play` `pause` `music` `imagePlus` `edit` `video` `upload` `grip` `expand` `copy` `wand` |
| `size` | number | 16 | the side of the square in px |
| `stroke` | number | 1.8 | stroke width in viewBox units (24) |
| `aria-label` | string | | give one when the icon is the only thing saying what a control does |

## Rules
- The viewBox is 24, the stroke 1.8 with round caps and joins. `dots` and `grip` are filled circles that keep the stroke; `play` and `pause` are the only solid marks with no stroke.
- Sizes the library uses: 12 on small controls, 13 on medium, 15 on the large primary button, 14 to 16 in a nav item.
- Ink: `--ink-500` at rest beside a label, `--ink-900` when chosen, `--accent-blue` when hovered or on, `--on-accent` on a gradient fill.
- Do not draw a new mark by hand. Add it to `paths.rs` in rux and re-run the extractor so the Rust and the web stay one set.

Unluminous draws its own window marks (the rail, the explorer, the title bar) with egui; those are in the Unluminous Icons asset groups, exported from `theme::icon` by `tools/export-unluminous-icons`.
