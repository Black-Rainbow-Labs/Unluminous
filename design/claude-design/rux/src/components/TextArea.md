A well of running text, pressed into the surface.

Source: Rust `TextArea` (`components/field.rs`), reference rules `.ic-prompt-wrap`, `.ic-prompt`, `.ic-prompt-shuffle`, `.ic-prompt-count`, `.ic-form-textarea`.

## When to use
Prompts, descriptions, notes. For one line use `TextInput`.

## Props
| Prop | Default | Meaning |
|---|---|---|
| `value`, `onChange(text)` | internal | Controlled text; falls back to internal state. |
| `hint` | `''` | Placeholder in `--ink-400`. |
| `count` | false | Mono 10px character count, bottom 8px right 12px, `--ink-400`. |
| `corner` | | Icon name for a 28px round button at top 8px right 8px (`.ic-prompt-shuffle`). |
| `onCorner` | | Corner button pressed. |
| `cornerLabel` | `Regenerate` | Its accessible name. |
| `disabled` | false | opacity 0.4. |
| `label` | `Text` | Accessible name. |

## Look
Minimum height 280px, radius `--r-md`, `--e-pressed`, text 13px/1.6 (`.rux-t-prose`). Padding 14px 16px; with a corner button or a count it is 14px 40px 28px 14px, leaving room for both, which are painted over the well. The corner button is raised (`--e-raised-sm`) and presses to `--e-pressed-sm`.

## Consumer provides
Width, and the height if 280px is wrong. The counter counts characters, not bytes.

## Do / do not
Do give the corner button a meaningful `cornerLabel`. Do not add a second shadow on focus; the well does not change when focused.
