A surface pressed into the page, for content that is held rather than presented.

Source: Rust `Well` (`components/panel.rs`), reference rules `.timeline`, `.ic-prompt-wrap`, `.ic-tile`.

## When to use
A recessed area inside a `Panel`: a timeline, a tile, an image well, a log. `TextArea` and `TextInput` draw their own wells; use `Well` for everything else.

## Props
| Prop | Default | Meaning |
|---|---|---|
| `shallow` | false | `--e-pressed-sm` instead of `--e-pressed`. |
| `radius` | `--r-md` | Number (px) or CSS length. |
| `pad` | 0 | Number (px) or CSS padding. |
| `fill` | `--surface-1` | CSS colour or var(). |

## Look
A sunken surface: `--surface-1` background with an inset elevation. No padding by default, so the caller decides.

## Consumer provides
Size and padding.

## Do / do not
Do use a well for content that can be pressed or scrolled inside. Do not put a raised card directly on a well edge; leave padding.
