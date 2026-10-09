A modal in Unluminous's own window, the shape every dialog shares.

Unluminous's own window component, not part of rux itself. Ported from `components::modal::show`, `modal::footer`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `title` | string | at the left of the 46 point header |
| `buttons` | string[] | the footer's buttons; the last does the thing and Enter presses it |
| `note` | string | the quiet sentence at the left of the footer |
| `width, height` | number | the size it asks for |
| `inline` | bool | draw over the parent box rather than the whole page |
| `children` | node | the body |
| `className`, `style`, others | | forwarded to the root |

## Rules
- Filled `--ul-explorer` over the scrim, a one point `--ul-control-border` stroke, corner radius `ul-card` (10).
- The header is 46 points in `--ul-title-bar`; the footer 52 with a `--ul-divider` along its top.
- A modal is dragged by its header and resized from any edge. A double click on the header puts it back.
