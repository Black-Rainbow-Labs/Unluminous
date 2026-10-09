A round toggle beside a control that turns randomising on or off.

Ported from Rust `DiceToggle` (`components/toggle.rs`). Rule: `.ic-dice`.

## When to use
Next to a field whose value can be randomised, such as a prompt or a seed. The state reads without a label.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `on` | bool | internal state | `DiceToggle::new(on)` |
| `defaultOn` | bool | false | |
| `onChange` | (on) => void | | the click outcome |
| `icon` | Icon name | `cube` | `.icon()` |
| `label` | string | `Randomize` | `.label()` |
| `disabled` | bool | false | `.enabled(false)` |

## States
- Off: 28px circle, `--surface-1`, `--e-pressed-sm`, ink `--ink-400`, hover ink `--accent-violet`.
- On: `--grad-violet` 160deg, `--on-accent` ink; shadow is `-2px -2px 4px --shadow-light`, `2px 2px 6px` violet at 40%, and an inset `1px 1px 1px` white at 30%.
- Mark is 13px. Disabled: opacity 0.4.

## Consumer provides
The state and what it means.

## Do / do not
Do pair it with a label elsewhere. Do not use it as a plain action button; use `IconButton`.
