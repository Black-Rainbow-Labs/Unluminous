A row of mutually exclusive choices in a pressed track, the chosen one raised.

Ported from Rust `Segmented` (`components/toggle.rs`). Rules: `.view-toggle`, `.ic-modal-tabs`, `.ic-modal-tab.is-active`.

## When to use
Switching a view or a modal tab where exactly one option is on. Use `compact` for modal tabs.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `items` | `[{ label, icon?, accent? }]` | required | `items` (label, optional icon) |
| `value` | index | internal state | `active` |
| `defaultValue` | index | 0 | |
| `onChange` | (index) => void | | `SegmentedOutcome::chosen` |
| `compact` | bool | false | `.compact()` |

`accent` on an item is a CSS colour (for example `var(--accent-blue)`) used as the chosen ink, as `.ic-modal-tab.is-active[data-kind]` does; the Rust `.accents()` list becomes one value per item.

## Measurements
| | regular | compact |
|---|---|---|
| track padding | 5px | 4px |
| item padding | 7px 14px | 6px 10px |
| gap between items | 2px | 2px |
| gap icon to label | 6px | 5px |
| type | 12 / 500 | 11 / 500 |
| mark | 13 | 11 |

## States
Track: `--surface-1` with `--e-pressed-sm`, pill. Item idle `--ink-500`, hover `--ink-900`, chosen `--surface-1` with `--e-raised-sm` and `--ink-900` (or its accent).

## Do / do not
Do give every item a label. Do not use it for more than about five options.
