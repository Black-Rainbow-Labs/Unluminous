The rail of pane buttons down the far left: one button a pane, the open ones lit.

Unluminous's own window component, not part of rux itself. Ported from `components::activity_bar`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `top, bottom` | `{ id, icon, label, on, disabled }[]` | the two groups of buttons |
| `onToggle` | (id) => void | a button was pressed |
| `className`, `style`, others | | forwarded to the root |

## Rules
- 36 points wide (`ul-size-activity-bar`), filled `--ul-explorer-footer`, a 24 point button every 30 points.
- A pane that is open is a state, so it is quiet: the accent at 15% behind the mark, the mark in `--ul-accent`, and a 2.5 by 14 point bar against the left edge. Never a solid accent square.
- The buttons are inset 6 from the left, which is what the window's own resize grip takes.
