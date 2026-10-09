A dropdown: a trigger that shows the chosen option and a menu of rows that opens under it.

Source: Rust `Select` (`components/select.rs`), reference rules `.ic-select`, `.ic-select__trigger`, `.ic-select__menu`, `.ic-select__item`, `.ic-select--up`, `.ic-select--mono`.

## When to use
Choosing one value from a short list (a model, a dimension, an endpoint). For a project picker with a title and a footer action use `ProjectMenu`.

## Props
| Prop | Default | Meaning |
|---|---|---|
| `options` | `[]` | Array of strings. |
| `value` | `null` | Index of the chosen option. Null shows `placeholder`. |
| `onChange(index)` | | Called with the chosen index; the menu then closes. |
| `placeholder` | `—` | Shown with nothing chosen, in `--ink-400`. |
| `mono` | false | `.ic-select--mono`: JetBrains Mono 11px, 0.01em. |
| `up` | false | `.ic-select--up`: the menu opens above the trigger. |
| `disabled` | false | opacity 0.4. |
| `label` | `Select` | Accessible name. |
| `zoom` | 1 | Multiplier for words, padding, chevron and menu (set as `--z`). |
| `menuElevation` | `default` | `quiet` swaps the menu shadow `--e-raised-lg` for `--e-raised-sm` (Rust `menu_elevation`). |
| `open`, `onToggle(open)` | internal | Controlled open state. |

## States
Trigger: `--e-raised-sm`; hover `--e-raised`; open `--e-pressed-sm` with the chevron turned 180deg. Row hover `--accent-wash`; chosen row `--accent-wash-soft`; both use `--accent-blue` words, and the chosen row is weight 500 with a 12px check. Trigger is 34px high, padding 9px 12px, gap 8px, chevron 13px stroke 2. Menu: 6px below the trigger, padding 6px, max-height 280px, radius `--r-md`; rows padding 8px 12px, radius `--r-sm`.

## Consumer provides
The width (the menu matches the trigger) and room for the menu. Use `up` near the bottom edge: the web port does not flip automatically, the Rust does.

## Do / do not
Do give a `label`. Do use `menuElevation="quiet"` where the select sits close to the page. Do not change elevation on hover beyond the states above.
