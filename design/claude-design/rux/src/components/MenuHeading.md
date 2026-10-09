The mono caption over a group of rows in a menu.

Source: Rust `menu_heading` helper in `components/list.rs`, reference rule `.proj__menu-head`.

## When to use
At the top of a menu or a list group, above `ListItem` rows (`ProjectMenu` uses it for "Recent projects").

## Props
| Prop | Default | Meaning |
|---|---|---|
| `children` | | The heading words. |

## Look
28px tall, padding 10px 14px 8px, JetBrains Mono 9px, uppercase, letter spacing 0.14em, `--ink-400` (the `.rux-t-field-label` style).

## Consumer provides
The width. Nothing interactive goes inside.

## Do / do not
Do keep it to a few words. Do not use it as a page heading; use `.rux-t-panel-title` or `SubGroup` for sections.
