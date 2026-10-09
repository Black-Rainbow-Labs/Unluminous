# Fader

A fader: a groove sunk into the plate with eleven ticks under it, the travelled part lit in a colour, and a raised cap with two engraved lines.

Source: `components/fader.rs` (`Fader`). No reference CSS rule.

## When to use

A value somebody sets along a range: a temperature, a token budget, a volume. For a value somebody only reads, use Meter.

## Props

| Prop | Rust | Default | Meaning |
|---|---|---|---|
| `value` / `defaultValue` | `Fader::new(id, value, ..)` | 0 | controlled or internal |
| `min`, `max` | `min`, `max` | 0, 1 | range; `max <= min` becomes `min + 1` |
| `step` | `.step()` | 0 | snap size; 0 is continuous |
| `colour` | `.colour()` | `--accent-blue` | the lit run, the glow and the grip while held |
| `label` | `.label()` | `Fader` | accessible name |
| `onChange(next)` | `FaderOutcome.changed` | none | value moved |

Also exported: `snapFader(value, min, max, step)` and `litRun(fraction, left, right)`.

## Numbers

Height 30: a 12x16 cap, 3 of gap, 5 of the tallest tick, 6 to spare. The value runs between the cap's centre at each end, 6px in from each side. Groove 4px deep, `--inst-screen`, `--e-pressed-sm`, running 2px beyond those centres. Lit run 2px thick with a glow rect expanded 1px at 22%. Eleven ticks 1px wide starting 3px below the cap, 5px tall at 0, 5 and 10 (`--ink-300`) and 3px tall otherwise (`--ink-200`). Cap radius 4, `--e-raised-sm`, 1px `--ink-200` hairline, lit edge, grip lines at 1.25px either side of the middle, 3.5px in; `--ink-300`, or the colour while held. Hover and held fill `--surface-3`. Focus: 1px `--focus-ring` ring 2px outside the cap.

## States

Rest, hover, dragging (`grabbing`), keyboard focus. At the smallest value nothing is lit. Arrow keys move one `step` (or 1% of the range when continuous).

## Do / do not

- Do give it a `label`.
- Do not draw a fader for a read only value.
