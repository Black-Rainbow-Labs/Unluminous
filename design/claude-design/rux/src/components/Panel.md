A card that stands off the page, or a flat panel.

Source: Rust `Panel` and `PanelKind` (`components/panel.rs`), reference rules `.scene` (card) and `.ic-panel` (flat).

## When to use
The main container for a group of controls. Nest a `Well` inside for input areas and a `SubGroup` for collapsible sections.

## Props
| Prop | Default | Meaning |
|---|---|---|
| `kind` | `raised` | `raised` is `--e-raised`; `raisedSmall` is `--e-raised-sm`, one step quieter; `flat` has no shadow. |
| `radius` | `--r-xl` | A number (px) or CSS length. |
| `pad` | 22px | A number (px) or CSS padding value. |
| `fill` | `--surface-1` | CSS colour or `var(--surface-2)`. |

## Look
Background `--surface-1`, radius `--r-xl` (24px), padding 22px. The Rust returns the rectangle inside the padding; here `children` are rendered there. The Rust notes that `.ic-panel` has a hairline down the edge facing the page; the Rust code does not draw it, and neither does this port: add a `Divider` or a border at the use site if wanted.

## Consumer provides
Size and the content. Panels do not change elevation on hover.

## Do / do not
Do nest `Well` inside `Panel`, not `Panel` inside `Panel`. Do not use `fill` for a hue; surfaces come from the `--surface-*` tokens.
