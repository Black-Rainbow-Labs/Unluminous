A row in a list or a menu: a label, an optional leading dot, an optional trailing tick or action.

Source: Rust `ListItem` (`components/list.rs`), reference rules `.proj__item` and `.ic-modal__list-item` (one row, two selected looks).

## When to use
Project lists, prompt lists, any vertical list where one row is chosen. In a menu, put `MenuHeading` above it.

## Props
| Prop | Default | Meaning |
|---|---|---|
| `label` | | Row text, elided with an ellipsis. |
| `selected` | false | The chosen row. |
| `dot` | false | `.proj__item-dot`: 6px disc, `--ink-300`; when selected `--accent-blue` with `0 0 8px` glow. |
| `tick` | false | Reserves a 14px trailing cell and shows a check when selected. |
| `action` | null | `{ icon, label }`: trailing 12px mark in `--accent-mint` at 75% (full on hover), only visible while the row is hovered or selected. Replaces the tick. |
| `onAction` | | Called when the trailing action is pressed (the row's `onClick` is not). |
| `pressedWhenSelected` | false | `.ic-modal__list-item.is-active`: selected row is `--e-pressed-sm` on `--surface-1` with `--ink-900` words, instead of the accent. |
| `onClick` | | Row pressed. |

## States
Padding 10px 14px, gap 10px, radius `--r-md`, 13px sans. Idle `--ink-700`; hover `--hover` wash and `--ink-900`; selected `--accent-blue`, weight 500. Press moves 0.5px.

## Consumer provides
The width and the rows' container. Wrap rows in `role="listbox"` when it is a list.

## Do / do not
Do give an action a `label`. Do not show an action on every row all the time; it appears on hover or selection by design. Do not use both accent and pressed looks in one list.
