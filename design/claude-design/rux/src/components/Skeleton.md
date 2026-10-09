A sunken placeholder block with a soft highlight sweeping across it.

Ported from Rust `Skeleton` (`components/loader.rs`).

## When to use
Standing in for text, a card or a picture while content loads. Give it the size of what will arrive.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `radius` | CSS length | `--r-md` (12px) | `.radius()` |
| `className`, `style` | | | size it here (width defaults to 100%, height to 48px) |

## Measurements
`--surface-sunken`, `--e-pressed-sm`. The highlight is 35% of the width, `--surface-2` fading 0%, 55%, 0%, running left to right from outside the box to outside the box, one pass every 1.25s, linear. It is hidden under reduced motion.

## Do / do not
Do match the shape of the real content. Do not use it for more than a few seconds; show an error or an empty state after that.
