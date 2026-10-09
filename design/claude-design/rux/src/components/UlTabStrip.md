The strip of file tabs above an editing pane.

Unluminous's own window component, not part of rux itself. Ported from `components::file_tabs`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `tabs` | `{ name, active, unsaved, transient, badge }[]` | the pane's tabs, left to right |
| `onPick, onClose` | (index) => void | a tab was shown or closed |
| `className`, `style`, others | | forwarded to the root |

## Rules
- 32 points tall (`ul-size-tab-strip`), filled `--ul-toolbar`. The open tab takes the editor's colour and a 2 point `--ul-accent` underline.
- An unsaved tab shows the `--ul-unsaved` dot where its close cross would be.
- A tab opened with one click is transient, in italics, until it is kept.
