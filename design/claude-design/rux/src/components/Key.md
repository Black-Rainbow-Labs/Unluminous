# Key

A key cap: a raised plate with a lit top edge that sinks one point when pressed, with an optional LED or tinted words to say which key matters.

Source: `components/key.rs` (`Key`), built on `instrument.rs`. No reference CSS rule.

## When to use

Buttons in an instrument module, a row of choices or a strip of tabs. A key that matters more than its neighbours is not painted a colour: it lights an LED, or its words take a colour.

## Props

| Prop | Rust | Default | Meaning |
|---|---|---|---|
| `label` / children | `Key::new(label)` | none | the words |
| `led` | `.led(colour, lit)` | none | `{ colour, lit }`, an LED at the left of the label |
| `down` | `.down(bool)` | false | held down |
| `chosen` | `.chosen(colour, on)` | none | `{ colour, on }`: down, words in `colour`, no LED. The chosen key in a row of choices |
| `tinted` | `.tinted(colour)` | none | words in this colour |
| `disabled` | `.enabled(false)` | false | ink `--ink-300`, no hover, default cursor (the Rust does not fade a key) |
| `compact` | `.compact()` | false | 24px tall, 11px type, radius 7, padding 10, LED radius 2.5 |
| `onClick` | outcome | none | pressed |

## Numbers

Height 30 (24 compact). Padding 13 (10). Radius 9 (7). Label sans 12, 500, -0.005em (11 compact). LED radius 3 (2.5), and it takes 14px with the gap to the words. Ground `--surface-2` (`--inst-plate`); hover `--surface-3`; press: `translateY(1px)`, `--e-pressed-sm`, lit edge fades out, over 0.09s. Ink: disabled `--ink-300`, tint or chosen colour, lit or down `--ink-900`, otherwise `--ink-700`.

## States

Rest, hover, pressed (`:active`), `is-down` (held), lit (LED on, or down), tinted, disabled, keyboard focus.

## Do / do not

- Do show the chosen key of a row with `chosen`; one mark, not an LED and a fill.
- Do not fill a key with an accent colour.
- Do not put a lamp on a key that is chosen when `chosen` already holds it down.

The consumer provides the labels and the state of which key is chosen.
