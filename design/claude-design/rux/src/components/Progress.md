A sunken track with a gradient fill that grows as something finishes.

Ported from Rust `Progress` (`components/loader.rs`). Reference gradient: `--grad-progress`.

## When to use
Work with a known fraction. With no fraction it sweeps instead, for work of unknown length. Use `Spinner` for a waiting result.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `fraction` | 0 to 1, or omitted | omitted (indeterminate) | `Progress::new`, `Progress::indeterminate()` |
| `height` | px number | 6 | `.height()` |
| `label` | string | `Progress` | accessible name |

## Measurements
Track: pill, `--surface-sunken`, `--e-pressed-sm`, height 6, natural width 200 (it fills its container, minimum 200px). Fill: `--grad-progress` left to right, same radius, width is the fraction. Indeterminate band: 20% of the track wide, sinusoidal travel over 80% of the track, one full back and forth in about 10.5s.

## Do / do not
Do clamp nothing yourself; 0 to 1 is clamped. Do not show 0% as a fill; at zero no fill is visible.
