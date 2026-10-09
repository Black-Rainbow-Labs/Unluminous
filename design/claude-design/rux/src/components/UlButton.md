A button with a word on it, as Unluminous draws one in its own window.

Unluminous's own window component, not part of rux itself. Ported from `modal::button`, `controls::choice_button`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `label` | string |  |
| `variant` | `default` `primary` `choice` `state` | primary is a modal's last button; choice is one of a set; state says a thing is already so |
| `on` | bool | for a choice button |
| `disabled` | bool |  |
| `className`, `style`, others | | forwarded to the root |

## Rules
- `--ul-control` with a one point `--ul-control-border` stroke, radius 6, the word in `--ul-text-strong` at 12.5.
- A modal's last button is the one that does the thing: filled `--ul-accent`, its word in `--ul-on-accent`.
- A choice button that is on is accent filled; a state that is not a button is the same drawing with no click (`In use`).
