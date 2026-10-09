Unluminous's own drawn marks: the rail, the explorer, the title bar, the debugger and the symbol kinds.

These are not rux icons. Unluminous draws each mark with egui painter calls in `crates/unluminous-app/src/theme/icon.rs`, and `cargo run -p unluminous-app --example export_icons_svg` writes those exact shapes out as SVG. The data here is that export, so a mark shown here is the mark the window draws.

## Props
| Prop | Type | Default | Notes |
|---|---|---|---|
| `name` | a mark name | | `folder` `file` `branch` `realm` `chat` `board` `database` `terminal` `run` `bug` `debug-run` `stop` `rerun` `magnifier` `plus` `cross` `tick` `disclosure-open` `disclosure-closed` `folder-mark-open` `folder-mark-closed` `file-mark-code` `file-mark-prose` `view-raw` `view-side-by-side` `view-preview` `font` `symbol-function` … (73 in all) |
| `set` | `material` `classic` | `material` | `theme::IconSet`. Ten marks differ between the sets; the rest are one drawing. |
| `size` | number | 24 | the 24 point cell the mark sits in. The rail uses 24, a menu row 20. |

## Colour
A mark takes `color`. The roles: `--ul-icon` at rest, `--ul-icon-active` when its pane is open, `--ul-icon-disabled` when it cannot be used, `--ul-accent` when it is on in a bar, `--ul-folder` and `--ul-folder-open` for a folder's mark, `--ul-git-added` for a play button that runs something.

## Rules (from design/style-guide.md)
- An icon is drawn inside about a 10 point square, at a 1.3 to 1.6 point stroke. In a bar it sits on the pixel grid.
- A mark is judged at two sizes: on the eight times sheet, and at the fourteen pixels a person sees.
- The colour wheel is the one mark drawn in colours of its own.
- A redesigned mark goes back into `theme::icon` as painter calls, then this export is run again.
