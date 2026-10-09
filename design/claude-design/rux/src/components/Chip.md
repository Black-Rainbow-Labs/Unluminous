A small pill that states something rather than does something.

Ported from Rust `Chip` (`components/chip.rs`). Rules: `.meta-chip`, `.volume-chip`, `.scene__badge`, `.ic-elapsed`, `.ic-saveto`, `.ic-preset-chip`.

## When to use
Metadata readings (a size, a volume), badges, an elapsed time, or a removable preset. A chip is not pressable; the only control in it is the dismiss.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `label` (or children) | string | | `Chip::new` |
| `value` | string | | `.value()`, a second darker 600 run |
| `dot` | CSS colour | | `.dot()`, 7px disc with a 6px glow |
| `icon` | Icon name | | `.icon()`, 12px |
| `dismissible` | bool | false | `.dismissible()` |
| `onDismiss` | () => void | | the dismiss outcome |
| `variant` | `sunken` `raised` `accent` | `sunken` | `.variant()` |
| `mono` | bool | false | `.mono()`, mono 11 / 500, tracking 0.02em |
| `accent` | `mint` `violet` `primary` `danger` | `mint` | `.accent()` gradient of an accent chip |

## Variants and states
- `sunken`: `--e-pressed-sm`, `--ink-700`; padding 7px 14px; type 11 / 500; gap 8px.
- `raised`: `--e-raised-sm`.
- `accent`: gradient 160deg, `--on-accent`, glow of the gradient end at 30%, padding 6px 8px 6px 12px.
- Dismiss: 18px disc of white at 25% (40% on hover), an `x` mark of 9px at stroke 2.4.

## Consumer provides
The text, the colour for `dot` (a token), and what dismiss does.

## Do / do not
Do use a chip for a reading. Do not make a whole chip a button; use `Button`.
