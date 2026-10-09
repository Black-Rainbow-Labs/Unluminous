The file list: the project name, the filter box, the tree of 28 point rows and the counts.

Unluminous's own window component, not part of rux itself. Ported from `components::explorer`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `project` | string | drawn in spaced capitals at 10.5 |
| `rows` | `{ name, depth, folder, open, chosen, cursor, faint, git, mark, badge, unsaved }[]` | the tree, flattened |
| `filter` | string | the words in the filter box |
| `footer` | string | the counts along the bottom |
| `keyboard` | bool | whether the explorer has the keyboard; rings the cursor's row in the accent |
| `className`, `style`, others | | forwarded to the root |

## Rules
- A row is 28 points (`ul-size-row`) and one level of nesting is 18 (`ul-size-indent`).
- The chosen row is one pill: the row inset by 8 and 1, radius 5, filled `--ul-selected-row`, its name in `--ul-text-strong`. Hovering draws the same pill in `--ul-control`.
- A row with the keyboard on it adds a one point `--ul-accent` ring inside the pill, and only while the explorer has the keyboard.
- Git colours the name: `--ul-git-added`, `--ul-git-modified`, `--ul-git-untracked`. A file nothing can open is `--ul-text-faint`.
- An open folder's mark is `--ul-folder-open`, the one loud move of the material icon set.
