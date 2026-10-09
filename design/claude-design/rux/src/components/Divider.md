A one pixel hairline across a region, optionally fading out at both ends.

Source: Rust `Divider` (`components/panel.rs`), reference rule `.topbar::after` (`linear-gradient(90deg, transparent, rgba(0,0,0,0.06), transparent)`).

## When to use
Between a bar and the page (`faded`), or between blocks inside a panel (plain).

## Props
| Prop | Default | Meaning |
|---|---|---|
| `faded` | false | 90deg gradient from transparent to the colour and back. |
| `color` | `--hairline` | CSS colour or var(). |

## Look
1px tall, full width of its container, `--hairline`.

## Consumer provides
The container width.

## Do / do not
Do use `faded` under a top bar so the rule does not meet the window edges. Do not use it as a column divider; it is horizontal only.
