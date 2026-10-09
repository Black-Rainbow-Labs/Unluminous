The whole Unluminous window, built from the parts below, with every part a prop.

Unluminous's own window component, not part of rux itself. Ported from `UnluminousApp::ui` and `app::dock::regions`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `width, height` | number | 1180 × 740, the size the screenshot tests draw at |
| `titleBar, rail, statusBar` | element | replace one bar and keep the rest |
| `left` | element or null | the explorer by default; null puts it away |
| `main` | element | the tab strip and the editor by default |
| `right, rightWidth` | element, number | a panel docked on the right, such as Agent-Chat |
| `bottom, bottomHeight` | element, number | a strip along the bottom, such as the terminal tile |
| `overlay` | element | drawn over the window: a modal, an open menu |
| `transparent` | bool | the editor at 86% so a picture behind the window shows through |
| `className`, `style`, others | | forwarded to the root |

## Rules
- The window has no operating system frame. Its corner is `ul-window-corner` (12px).
- Strips are taken across the whole width first and the columns come out of what is left, unless a side is set to fill its edge.
- The editing area keeps at least 72 points of height and 160 of width.
