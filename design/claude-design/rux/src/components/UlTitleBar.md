The bar along the top of the window: menus, project, text tools, run widget and window buttons.

Unluminous's own window component, not part of rux itself. Ported from `components::title_bar`, `components::text_tools`, `components::run_widget`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `project` | string | the project folder, bold after the menus |
| `textTools` | bool | the F button and the three view modes; drawn only for a file they mean something for |
| `viewMode` | `raw` `side-by-side` `preview` | the chosen view mode |
| `debug, running` | bool | the debug button, and the stop button while a run is going |
| `platform` | `windows` `macos` | Windows draws the menus and three round buttons; macOS draws the lights |
| `openMenu` | string | the menu whose word is drawn as open |
| `className`, `style`, others | | forwarded to the root |

## Rules
- 38 points tall (`ul-size-title-bar`), filled `--ul-title-bar`. Its height never changes, so a control that is absent leaves room rather than moving the window.
- A control that can never apply to the open file is absent. A control that could be used in a moment is dimmed.
- The view modes are a segmented control: a pressed track and the chosen segment raised out of it, its mark in `--ul-accent`.
- The run widget sits at the right hand end and the text tools in front of it, so the play button never moves when the tab changes.
