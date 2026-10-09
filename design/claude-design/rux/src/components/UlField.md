A text field, with a magnifier when it searches.

Unluminous's own window component, not part of rux itself. Ported from `controls::search_field`, `modal::field`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `search` | bool | draw the magnifier 13 points in |
| `value, defaultValue, onChange` |  | the words in it |
| `placeholder` | string | the words before anything is typed, in `--ul-text-faint` |
| `className`, `style`, others | | forwarded to the root |

## Rules
- Filled `--ul-field`, a one point `--ul-divider` stroke, corner radius `ul-control-corner` (6).
- The words are a fraction of the field's own height, so at 24 points they are the 12.5 the rows are set in.
