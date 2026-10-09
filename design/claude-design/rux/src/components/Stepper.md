A number with a stacked up and down tick beside it.

Ported from Rust `Stepper` (`components/stepper.rs`). Rules: `.ic-count`, `.ic-count__steps`, `.num-input`.

## When to use
A small bounded integer such as a count of images. It is 66px wide and refuses to shrink in a flex row (`flex: none`), because a squeezed number is unreadable.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `value` | number | internal state | `Stepper::new(value, ...)` |
| `defaultValue` | number | 0 | |
| `min`, `max` | number | 0, 100 | `min`, `max` |
| `onChange` | (n) => void | | `StepperOutcome::changed` |
| `label` | string | `Count` | `.label()` |
| `disabled` | bool | false | `.enabled(false)` |

## Measurements
Well 66px wide, `--r-md`, `--e-pressed-sm`, padding 4px 4px 4px 10px, gap 4px. Number: mono 600 12, `--ink-900`, centred. Ticks: 18 by 11, radius 3px, 1px apart, `--e-raised-sm`, chevron 9px stroke 2.4. Tick ink `--ink-500`, hover `--accent-blue`, press `--e-pressed-sm`. A tick at its limit has opacity 0.4.

## Not ported
The Rust also has a wide touch form (a minus, the number, a plus, each a full 44px target). That form depends on the touch target size and is not drawn here.

## Do / do not
Do set `min` and `max`. Do not use it for free numeric entry; use a text input.
