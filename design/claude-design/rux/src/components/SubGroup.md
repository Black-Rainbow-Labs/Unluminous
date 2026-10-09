A collapsible group: a mono heading with a coloured mark and a disclosure, under a hairline.

Source: Rust `SubGroup` (`components/panel.rs`), reference rule `.ic-sub` (and `.ic-sub__body { gap: 12px }`).

## When to use
Sections of a settings or prompt panel: Qualities, Scene, Lighting, Person. The whole heading is the collapse control.

## Props
| Prop | Default | Meaning |
|---|---|---|
| `title` | | Heading text, set uppercase. |
| `icon` | `layers` | Icon name, 13px, tinted by `accent`. |
| `accent` | `blue` | `blue`, `violet`, `amber`, `rose`, `mint` or `coral`, mapped to `--accent-*`. The reference uses violet for Qualities, blue for Scene, amber for Lighting, rose for Person. |
| `open`, `defaultOpen`, `onToggle(open)` | `defaultOpen` true | Controlled or internal state. |
| `first` | false | No hairline and no 14px above. |

## Look
`border-top: 1px solid var(--surface-sunken)` over `padding-top: 14px`. The heading is 20px tall, gap 8px: mark 13px, title mono 10.5px 500 uppercase 0.12em `--ink-700`, chevron 11px stroke 2.2 `--ink-400`, turned -90deg when collapsed. The body sits 12px under the heading, children separated by 12px.

## Consumer provides
The body content. A scrolling parent if the group is tall.

## Do / do not
Do mark the first group in a panel with `first`. Do not hide a required field inside a collapsed group.
