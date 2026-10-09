A round chip with a person's initials on a violet gradient, with an optional status dot.

Ported from Rust `Avatar` (`components/avatar.rs`). Rule: `.avatar`, `.avatar__status`.

## When to use
The account button in a header, or a person in a list. Pass the whole name as `name`; initials are not a name.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `initials` | string | required | `Avatar::new` |
| `status` | CSS colour | none | `.status()`, for example `var(--accent-mint)` |
| `diameter` | px number | 40 | `.diameter()` |
| `name` | string | `Account` | `.name()` |

## Measurements
40px circle (`--grad-avatar` 160deg, `--e-raised-sm`). Initials 13px / 600 white, scaled by `diameter / 40`. Status dot 10px, 1px from the bottom right corner, with a 2px ring of `--surface-1`; both scale with the diameter.

## Do / do not
Do pass `name`. Do not use more than two letters. It is a button; wire `onClick` or leave it inert.
