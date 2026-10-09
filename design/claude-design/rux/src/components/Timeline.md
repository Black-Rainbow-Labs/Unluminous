# Timeline

A timeline down a groove: an LED for each item, lit for done, breathing for the one in progress, dark for what is to come.

Source: `components/table.rs` (`Timeline`, `Item`, `Stage`). No reference CSS rule.

## When to use

Steps of a release, a task's stages, a build pipeline. Not for a list of equal items; use a list.

## Props

| Prop | Rust | Default | Meaning |
|---|---|---|---|
| `items` | `Item { time, title, text, stage }` | `[]` | `[{ time?, title, text?, stage }]` |
| `stage` | `Stage` | `todo` | `done` (mint lamp), `active` (blue lamp that breathes), `todo` (dark lamp with a 1px `--ink-200` rim) |
| `animate` | `appearing` | true | each lamp comes on in turn (0.03s x 3 apart, 0.32s); the active lamp breathes for 12s (period 2.4s, 75% to 100%) then holds |

## Numbers

Items 12px apart. Time column mono 10.5 `--ink-400`, as wide as the widest time plus 12, absent when no item has a time. Rail 26px; lamp radius 4, centred 8px in and on the first line of the title (line height 17.5px). Groove 3px wide, radius 1.5, `--inst-screen` with `--e-pressed-sm`, from the first lamp's centre to the last. Title sans 12.5, 500, line height 1.4, `--ink-900` (`--ink-500` when to do). Text under it sans 12, line height 1.45, `--ink-500`.

## Do / do not

- Do give at most one item `active`.
- Do not use it for more than about a dozen items.
