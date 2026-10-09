A menu: the bar's menus and every right click menu in the window are this one drawing.

Unluminous's own window component, not part of rux itself. Ported from `controls::menu_rows`, `app::actions::menus`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `items` | `{ label, shortcut, ticked, disabled, chosen, submenu }` or `{ separator }` or `{ heading }` | the rows |
| `width` | number | 340, wide enough that a long name and its shortcut do not meet |
| `className`, `style`, others | | forwarded to the root |

## Rules
- A menu row is 24 points (`ul-size-menu-row`): a tick when it is on, the name 18 points in, the shortcut right aligned in `--ul-text-faint`.
- A row that cannot be used is `--ul-text-faint` at 60% and takes no clicks.
- A submenu is drawn inline as a heading with its entries, not as a flyout.
- A flyout must not hold a dropdown or another flyout.
