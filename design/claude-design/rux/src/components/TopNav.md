The pill of navigation items along the top of a page; only the chosen item shows its label.

Source: Rust `TopNav` and `NavItem` (`components/top_nav.rs`), reference rules `.topnav` and `.topnav__item` (`.topnav__item span { display: none }`, `.is-active span { display: inline }`).

## When to use
Switching between the top level views of an app (Home, Storyboard, Film). The width changes with which item is active, so place it where it can grow.

## Props
| Prop | Default | Meaning |
|---|---|---|
| `items` | `[]` | Array of `{ id, icon, label }` (the Rust `NavItem`; icon is an `Icon` name). |
| `active` | 0 | Index of the chosen item. |
| `onChange(index)` | | An item pressed. |
| `fit` | `full` | `full`: active item carries its label. `icons`: marks only. `trimmed`: marks only, side padding 10px (an item is then 40px wide, the diameter of `.toolbtn`). The Rust picks the widest fit that gets inside the available width; the web port cannot measure, so choose it at the use site (for example from a media query). |

## Look
Strip: padding 6px, gap 4px, radius `--r-pill`, `--e-raised`. Item: padding 9px 14px, gap 8px, 20px mark, 12px 500 words, `--ink-500`; hover `--ink-900`. Active: padding 9px 18px, `--grad-nav-active-*` at 165deg, `--on-accent` words, `--e-nav-active`.

## Consumer provides
Where it sits and what each item navigates to. Each item has an accessible name from its label even when hidden.

## Do / do not
Do keep labels to one short word. Do not show every label; the one chosen item is meant to stand out. Do not use more than about five items.
