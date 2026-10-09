A scrolling container with a thin, quiet scrollbar.

Ported from Rust `Scrollbar` (`components/scrollbar.rs`). Rule: `.ic-panel__scroll::-webkit-scrollbar`.

## When to use
Any panel or list that scrolls vertically. The bar is only present when the content is taller than the area, as in the Rust (`needed()`).

## Props
| Prop | Type | Default | Notes |
|---|---|---|---|
| `children` | node | | the content |
| `className`, `style` | | | set a height or `maxHeight` in `style` |

## Measurements
Thumb 6px wide, radius 3px, minimum height 24px, `--surface-sunken`; under the pointer `--ink-300`. No track is drawn. The thumb sits 6px in from the right edge.

## Not expressible
The Rust computes the thumb as `area / content` clamped to at least 8% and 24px, and draws it itself. CSS leaves the thumb size to the browser, so only the 24px minimum is kept. Firefox ignores the 6px inset and the hover colour (it gets `scrollbar-width: thin` and the thumb colour).

## Do / do not
Do wrap only the scrolling region. Do not nest scrolling containers.
