An editing pane showing code: the gutter, the coloured text and the caret.

Unluminous's own window component, not part of rux itself. Ported from `components::editor_view`, `components::gutter`, `unluminous_core::syntax`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `code` | string | the text |
| `caretLine` | number | the caret's line, from 1; its number is drawn in `--ul-text-strong` |
| `breakpoints` | number[] | lines with a breakpoint, drawn over the number in `--ul-close` |
| `executionLine` | number | the line a paused program is stopped on |
| `fontSize` | number | 14 for code |
| `prose` | bool | set the text in the interface font, as a Markdown file is |
| `className`, `style`, others | | forwarded to the root |

## Rules
- The editing area is `--ul-editor`, the colour the window's opacity setting applies to.
- Code is coloured by the nine `--ul-syntax-*` tokens. A theme colours the tokens and never the editing area's ground.
- The gutter numbers are 11.5 in `--ul-text-faint`. The 12 points between them and the text are where a fold arrow sits.
