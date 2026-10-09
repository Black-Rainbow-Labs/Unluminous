A pill button with a word on it, in seven variants and four sizes.

Ported from Rust `Button` (`components/button.rs`). Rules: `.ic-create`, `.render-btn`, `.ic-mini-btn`, `.add-scene`, `.ic-fbtn`, `.ic-fbtn--danger`, `.ic-fbtn--primary`, `.ic-clear`, `.ic-add-person`.

## When to use
Any action with a label. Use `primary` once per screen (create), `mint` for save, `secondary` for everything else, `danger` for destructive actions (coral ink, never a red fill), `dashed` for an invitation such as Add Person, `ghost` for a quiet inline action. For a mark with no words use `IconButton`.

## Props
| Prop | Type | Default | Rust |
|---|---|---|---|
| `label` (or children) | string | | `Button::new` |
| `variant` | `primary` `secondary` `raised` `mint` `danger` `dashed` `ghost` | `secondary` | `.variant()`, `.primary()` |
| `size` | `mini` `sm` `md` `lg` | `sm` | `.size()` |
| `icon` | Icon name | | `.icon()` |
| `trailing` | Icon name | | `.trailing()` |
| `disabled` | bool | false | `.enabled(false)` |
| `stretch` | bool | false | `.stretch()` (`flex: 1`) |
| `onClick`, `className`, `style`, others | | | forwarded to the root |

## Sizes
| size | padding | gap | type | mark | min height |
|---|---|---|---|---|---|
| `mini` | 9px 12px | 6px | 12 / 500 | 12 | 34 |
| `sm` | 9px 14px | 6px | 12 / 500 | 12 | 34 |
| `md` | 10px 20px | 10px | 13 / 500 | 13 | 40 |
| `lg` | 14px 20px | 8px | 14 / 600 | 15 | 48 |

## Variants and states
- `secondary`: `--surface-1`, `--ink-700`, `--e-raised-sm`; hover ink `--accent-blue`; press `--e-pressed-sm`.
- `raised`: as secondary with `--ink-900` and `--e-raised`.
- `primary`: `--grad-primary` 180deg, `--on-accent`; `--e-primary` at `lg`, `--e-primary-sm` otherwise; press `--e-primary-pressed`.
- `mint`: `--grad-mint` with a mint glow at 35%.
- `danger`: secondary with `--accent-coral` ink.
- `dashed`: 1px dashed `--ink-300`, `--r-md`, no shadow; hover ink blue plus `--accent-wash`.
- `ghost`: no surface; hover `--hover`, press `--pressed`, ink `--ink-900`.
- Press: `translateY(0.5px)`. Disabled: opacity 0.4.

## Consumer provides
The click handler and the label.

## Do / do not
Do keep one `primary` per screen. Do not set a colour on a button; pick a variant. Do not use `danger` as a filled button.
