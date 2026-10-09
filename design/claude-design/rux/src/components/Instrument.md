# Instrument

The instrument language: raised plates, recessed screens, LEDs, silkscreen labels and readings that every instrument control (Key, Fader, Meter, Chart, Table, Timeline) is built from.

Source: `components/instrument.rs` (`Instrument`, `plate`, `screen`, `led`, `lit_edge`, `SILK`, `READING`, `timing`). There is no reference CSS rule; the numbers are the Rust's.

## When to use

Use it to compose a module that reads as machined hardware: a `Plate` holds controls, a `Screen` holds data, an `Led` is where colour lives. A control is never filled with an accent colour; it lights an LED or tints its words.

## Pieces

| Component | Rust | What it is |
|---|---|---|
| `Plate` | `plate` + `lit_edge` | `--inst-plate` (`--surface-2`) with `--e-raised-sm` and a 1px lit edge along the top, brightest 35% along, gone 0.8 radius from each corner |
| `Screen` | `screen` | `--inst-screen` with `--e-pressed-sm`; `graticule` adds one dot every 6px inset by half the radius |
| `Led` | `led` | a lamp that fades from `--inst-led-off` to its colour, six bloom rings, a 0.6px dark rim, a glint |
| `Silk` | `SILK` | sans 9.5, 600, 0.12em tracking, uppercase |
| `Readout` | `READING` | mono 21, 500, -0.02em tracking |

Also exported: `TIMING` (press 0.09s, light 0.16s, rise 0.42s, stagger 0.03s, roll 0.6s, breath 2.4s, breathing for 12s), `SERIES` and `seriesColor(i)` (blue, mint, amber, coral, violet), `alpha(colour, a)`, `brighten`, `easeOut`, `easeBack`, `useClock`, `progress`, `textWidth`.

## Props

| Prop | Component | Default | Meaning |
|---|---|---|---|
| `radius` | Plate | 12 | corner radius in px (also positions the lit edge) |
| `padding` | Plate | 12 | padding in px |
| `radius` | Screen | 9 | corner radius in px |
| `graticule` | Screen | false | dot graticule |
| `colour` | Led | `var(--accent-blue)` | lit colour, any CSS colour |
| `brightness` | Led | 1 | 0 to 1; 0 is the off lamp |
| `radius` | Led | 3 | lamp radius in px |

All forward `className`, `style` and unknown props to the root.

## Tokens

Instrument colours are derived from the theme, not new theme values. `Instrument.css` defines them: `--inst-screen`, `--inst-dots`, `--inst-edge`, `--inst-plate`, `--inst-led-off`, `--inst-rule`, `--inst-rule-strong` (the rule at double alpha, used for a chart's hover line), `--inst-quiet`. Dark values are the default; `[data-theme='light']` overrides them.

## Do / do not

- Do light an LED or tint words to say "this one". Do not fill a control with an accent.
- Do put data on a `Screen`. Do not put a `Screen` on a `Screen`.
- Do not change the elevation on hover; Key only changes fill.
