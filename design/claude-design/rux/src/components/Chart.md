# Chart

A chart on a recessed screen: bars made of lit segments, a glowing trace, an area under one, or a segmented ring, with the value under the pointer read out in the screen's corner.

Source: `components/chart.rs` (`Chart`, `ChartKind`, `Series`, `nice_ticks`). Drawn as inline SVG over a graticule `Screen`. No reference CSS rule.

## When to use

Numbers over categories in an instrument module: build times per crate, tokens per turn, a share of a total. Bars compare, lines and areas show a trend, a donut shows parts of a whole (the first series only).

## Props

| Prop | Rust | Default | Meaning |
|---|---|---|---|
| `kind` | `ChartKind` | `bar` | `bar`, `line`, `area`, `donut` |
| `labels` | `labels` | `[]` | category names; for a donut, the slice names |
| `series` | `Series { name, values, colour }` | `[]` | `[{ name, values: number[], colour? }]`; colour defaults to blue, mint, amber, coral, violet in order |
| `stacked` | `.stacked()` | false | stack series (bars and traces) |
| `format(v)` | `format` | `String` | writes the axis numbers and the reading |
| `animate` | `appearing` | true | bars rise (0.42s, 0.03s stagger, ease back), traces draw (0.67s), the ring sweeps (0.76s) |
| `onHover({series,index}\|null)` | `ChartOutcome.hovered` | none | the point under the pointer |

`niceTicks(low, high, count)` is exported: round gridline values, each step 1, 2, 2.5 or 5 times a power of ten, always covering the data.

## Numbers

- Plot kinds: the screen is 160px tall, radius 9, with a graticule. Plot area is 22px in from top and bottom, 12px from the right, and `widest tick + 14` from the left. Gridlines are dotted (2 on, 4 off) in `--inst-rule` at round numbers, axis numbers mono 9.5 `--ink-300` 8px left of the plot, category labels mono 9.5 `--ink-400` 6px under it (every 2nd, 3rd, 4th, 5th... when they do not fit).
- Bars: a group is 72% of its column; lanes 4px apart; each 3 to 16px wide; segments 5px tall, 2px apart, radius 1, from the baseline outward and the last cut to the value; a 2px glow behind at 10%; the hovered one brightened 25%.
- Traces: three strokes of 6px (8%), 3px (18%), 1.6px (full); points of radius 2.4 (3.5 hovered) on a ring of 1.2px in the screen colour; area fills from 30% at the line to 2% at the base; a dotted vertical line (2 on, 3 off, `--inst-rule-strong`) at the hovered index.
- Legend (more than one series, not a donut): 10px under the screen, 16px rows, LED radius 3, sans 11 `--ink-500`.
- Donut: a round screen of side `min(height, width/2)`, ring from 66% of the outer radius to `side/2 - 9`, 1.6 degree gap each side of a slice, starting at the top; a 2px glow at 10%; a hovered slice grows 3px. The middle shows the percentage (mono 16, 500) and the name in silk letters for the hovered slice or the largest. The legend sits 18px right of the dial, 18px rows. Height is `clamp(110, 0.6 x width, 140)`.

## Do / do not

- Do give `format` for units (`s`, `k`, `%`).
- Do keep a donut to one series of non negative values.
- Do not use more than five series; the colours repeat.
- Do not put it in a container narrower than about 160px.
