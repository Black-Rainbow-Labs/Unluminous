The bar along the very bottom: the file, its state and the caret on the left, the font on the right.

Unluminous's own window component, not part of rux itself. Ported from `components::status_bar`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `items` | `(string | { text, unsaved })[]` | the left hand items |
| `message` | string | a sentence after them |
| `right` | string | the right hand item, the editor font |
| `className`, `style`, others | | forwarded to the root |

## Rules
- 32 points tall (`ul-size-status-bar`), filled `--ul-status-bar`, items at 11.5 in `--ul-text-dim` with a `--ul-divider` between them.
- A long message is cut short rather than drawn over the right hand item.
