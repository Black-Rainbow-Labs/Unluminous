A switch with a pressed track and a raised knob that moves to the chosen end.

Ported from Rust `Switch` (`components/toggle.rs`). Rule: the style guide's `.toggle__knob` with `--grad-knob`. It is not on the Image Creator page.

## When to use
An immediate on or off setting. For a choice among several, use `Segmented`.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `on` | bool | internal state | `Switch::new(on)` |
| `defaultOn` | bool | false | |
| `onChange` | (on) => void | | the click outcome |
| `label` | string | `Toggle` | `.label()` |
| `disabled` | bool | false | `.enabled(false)` |

## Measurements
Track 44 by 24, radius 12, `--e-pressed-sm`. Knob 18px (track radius minus 3) inset 3px, `--grad-knob` 145deg, `--e-raised-sm`.

## States
Off: track `--surface-1`, knob on the left. On: track `--grad-primary` 180deg, knob on the right. Disabled: opacity 0.4. The Rust draws no hover change and does not animate; neither does this.

## Do / do not
Do give it a `label`. Do not put the label inside; place text beside it.
