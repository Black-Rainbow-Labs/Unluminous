# Meter

A row of lit segments on a recessed screen: a value somebody reads, like a level meter on a mixing desk.

Source: `components/fader.rs` (`Meter`). No reference CSS rule.

## When to use

A token usage level, a context fill, a progress that is a fraction. For a value somebody sets, use Fader.

## Props

| Prop | Rust | Default | Meaning |
|---|---|---|---|
| `fraction` | `Meter::new(f)` | 0 | 0 to 1, clamped |
| `colour` | `.colour()` | `--accent-blue` | lit segments |
| `top` | `.top()` | none | colour of lit segments in the last tenth, so a meter goes amber near the top |
| `animate` | `.id()` | false | rises from zero over 0.63s (RISE x 1.5, ease out) the first time it is drawn |
| `label` | none | `Meter` | accessible name |

## Numbers

Screen 21px tall, radius 7, no graticule. Segments sit inside 7px at the sides and 6px above and below (9px tall), 2px apart, radius 1.5, as many as fit at 8px each (`floor((inner + 2) / 8)`, at least 4). Lit count is `round(fraction x rise x count)`. Off segments are `--inst-led-off`. A lit segment has a 1.5px glow at 14%. Segments from 90% of the count onward use `top`.

## Do / do not

- Do pass `top` when the end of the scale is a limit (tokens, context).
- Do not use it for a value somebody should be able to set.
