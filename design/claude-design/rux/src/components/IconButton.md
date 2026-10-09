A round button with a mark in it and nothing else, in five diameters.

Ported from Rust `IconButton` (`components/icon_button.rs`). Rules: `.toolbtn`, `.iconbtn`, `.dock__btn`, `.ic-modal__close`, `.ic-prompt-shuffle`, `.ic-dice`, `.ic-chev`, `.toolbtn__dot`, `.dock__badge`, `.scene__menu-btn`.

## When to use
A toolbar, dock, modal close or collapse handle where the mark says enough. Always pass `label`, since icon names are not words.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `icon` | Icon name | required | `IconButton::new` |
| `label` | string | required | accessible name |
| `size` | `tiny` 24, `sm` 28, `md` 32, `lg` 40, `xl` 48 | `lg` | `.size()` |
| `variant` | `raised` `sunken` `primary` `ghost` | `raised` | `.variant()` |
| `dot` | CSS colour | | `.dot()` status dot, top right |
| `badge` | number | | `.badge()` count, top right |
| `tint` | CSS colour | | `.tint()` overrides the ink |
| `turn` | radians | 0 | `.turn()` |
| `disabled` | bool | false | `.enabled(false)` |

## Marks per size
tiny 12, sm 13, md 14, lg 17, xl 19.

## Variants and states
- `raised`: `--surface-1`, `--e-raised-sm`, ink `--ink-700`, hover ink `--accent-blue`, press `--e-pressed-sm`.
- `sunken`: `--e-pressed-sm`, ink `--ink-400`; reads as a switch that is off.
- `primary`: coral diagonal gradient with a coral glow, `--on-accent` ink.
- `ghost`: no surface until hover (`--hover`) or press (`--pressed`).
- Press: `translateY(0.5px)`. Disabled: opacity 0.4.

## Consumer provides
The handler, the label, and the colour string for `dot` or `tint` (use a token, for example `var(--success)`).

## Do / do not
Do keep the circle square. Do not add text. Use `Button` for a word.
