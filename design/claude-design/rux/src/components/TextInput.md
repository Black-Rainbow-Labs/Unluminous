A one-line field pressed into the surface.

Source: Rust `TextInput` (`components/field.rs`), reference rules `.ic-form-input` and `.search-field`.

## When to use
Names, titles, paths, numbers, search. For several lines use `TextArea`.

## Props
| Prop | Default | Meaning |
|---|---|---|
| `value`, `onChange(text)` | internal | Controlled text; falls back to internal state. |
| `defaultValue` | `''` | Initial text when uncontrolled. |
| `hint` | `''` | Placeholder, `--ink-400`, same font as the text. |
| `search` | false | `.search-field`: pill radius, padding 10px 18px, magnifier. |
| `icon` | | Any icon name, 13px in `--ink-500`, gap 10px. |
| `mono` | false | JetBrains Mono for a number or a path. |
| `onSubmit(text)` | | Enter pressed. |
| `disabled` | false | opacity 0.4. |
| `label` | `Text` | Accessible name. |
| `inputProps` | | Extra props for the inner `input`. |

## Look
Padding 12px 16px, radius `--r-md`, 13px sans, `--ink-900`, caret and selection in `--accent-blue` (selection at 25%). Resting `--e-pressed-sm`; focused `--e-pressed`, the reference's whole focus treatment (the field sinks further). Minimum width 240px.

## Consumer provides
Width and a visible label nearby, or `label`.

## Do / do not
Do use `search` only for filtering. Do not add a border or a focus ring; the shadow is the focus state.
