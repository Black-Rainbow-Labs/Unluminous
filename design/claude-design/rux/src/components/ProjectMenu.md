The project picker for the top left of a page: a gradient icon chip, a kicker over the project name, and a menu with a heading, a list and a "New project" action.

Source: Rust `ProjectMenu` (`components/project_menu.rs`), reference rules `.proj`, `.proj__trigger`, `.proj__menu`. It is not a `Select` with a bigger trigger; the trigger and menu differ too much.

## When to use
Switching the current project or workspace. For a plain list of values use `Select`.

## Props
| Prop | Default | Meaning |
|---|---|---|
| `projects` | `[]` | Array of names. |
| `selected` | 0 | Index of the current project. |
| `kicker` | `Project` | Mono word over the name. |
| `heading` | `Recent projects` | The `MenuHeading` in the menu. |
| `createLabel` | `New project` | Footer action label. |
| `onChoose(index)` | | A project row pressed; the menu closes. |
| `onCreate()` | | Footer action pressed; the menu closes. |
| `open`, `onToggle(open)` | internal | Controlled open state. |

## Look
Trigger: min-width 240px, padding 8px 14px 8px 8px, gap 12px, radius `--r-lg`, `--e-raised-sm`; open `--e-pressed-sm` with the chevron turned. Chip: 36px, radius `--r-md`, `--grad-primary-diagonal-*` at 160deg, inset `1px 1px 2px` white at 0.2, folder mark 16px in `--on-accent`. Kicker mono 9px uppercase 0.14em `--ink-400`; name 13px 600 -0.01em `--ink-900`, max 160px, ellipsis. Menu: 10px below, min-width 280px, padding 8px, radius `--r-lg`, `--e-raised-lg`; heading 28px; rows 36px with a dot; footer has a 1px `--hairline` rule, 4px margin and padding, and a 36px action in `--accent-blue` (the one ghost button drawn in the accent).

## Consumer provides
Position on the page. Closes on outside press and `Escape`.

## Do / do not
Do keep names short. Do not add more actions to the footer; it holds one.
