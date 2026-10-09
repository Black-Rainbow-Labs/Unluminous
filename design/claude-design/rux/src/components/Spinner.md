A pressed disc with a quarter turn arc that runs blue into violet and turns once every 1.4 seconds.

Ported from Rust `Spinner` (`components/loader.rs`). Rules: `.ic-spinner`, `.ic-spinner__ring`, `@keyframes ic-spin`.

## When to use
Waiting on a result with no known length, usually inside a `Tile`. Use `bare` inside a button or a row where a disc would be too heavy. For a known fraction use `Progress`.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `diameter` | px number | 56 | `.diameter()` |
| `bare` | bool | false | `.bare()` (no pressed disc) |
| `label` | string | `Loading` | accessible name |

## Exports
`Spinner`, and `elapsedText(seconds)` for the `.ic-elapsed` chip: `7s`, then `1m 35s`.

## Measurements
Disc 56px, `--surface-1`, `--e-pressed`. Ring inset 8px (scales with diameter). The ring is a 44 unit viewBox with r 18 and stroke 3. Track: `--ink-900` at 8%. Arc: `M22 4 a18 18 0 0 1 18 18` (twelve to three o'clock), stroke gradient `--grad-multistop` left to right, round caps. The motion is `linear` and reduces to a slow turn when the user asks for reduced motion.

## Do / do not
Do pair the spinner with `elapsedText` in a `Chip` for long jobs. Do not stack several; the Rust draws it as a texture to stay cheap.
