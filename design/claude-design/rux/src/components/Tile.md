A 16:9 box where a picture or a result goes.

Ported from Rust `Tile` (`components/tile.rs`). Rules: `.ic-tile` (well) and `.scene__image` (dropzone).

## When to use
`well` holds a result as it appears; a `Spinner` fits in it. `dropzone` is the empty slot where a picture is chosen or dropped.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `kind` | `well` `dropzone` | `well` | `.kind()` |
| `image` | url | | `.image()` |
| `title`, `subtitle` | string | | `.empty_text()` |
| `label` | string | `Tile` | `.label()` |
| `children` | node | | drawn in the returned inner rectangle (`rect.shrink(12.0)`) |
| `onClick` | handler | | the dropzone click |

## Measurements and states
- `aspect-ratio: 16 / 9`, radius `--r-lg` (18px), clipped.
- `well`: `--surface-1`, `--e-pressed`.
- `dropzone`: `--grad-image-well` 160deg, `--e-image-well`, pointer cursor. Dark in both themes on purpose; everything over it is white at an alpha.
- Empty dropzone: a 56px dashed box (1.5px, white 15%, fill white 4%, `--r-md`) holding `imagePlus` 22px at white 50%; 4px below it plus an 8px gap, a title (14 / 500, -0.01em, white 70%), then 8px and a subtitle (12px, white 35%).
- With `image`, the picture covers the box with `object-fit: cover`.

## Do / do not
Do give a dropzone a title. Do not put controls over the whole tile; leave the 12px margin.
