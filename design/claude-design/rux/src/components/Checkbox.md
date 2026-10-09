# Checkbox

A checkbox: a 20px recessed well with a mint tick, down when ticked, with its label beside it; the whole row is the target.

Source: `components/key.rs` (`Checkbox`). At this commit a checkbox is a well, not a raised key (task-2219): a raised cap the colour of the plate was nearly invisible on a light plate.

## When to use

A checklist item or a settings tick in an instrument module.

## Props

| Prop | Rust | Default | Meaning |
|---|---|---|---|
| `checked` / `defaultChecked` | `Checkbox::new(on, label)` | false | ticked; controlled or internal |
| `label` | label | none | words beside the well |
| `colour` | `.colour()` | `--accent-mint` | the tick |
| `settles` | `.settles()` | true | ticking quiets the words to `--ink-400`; pass false for a form toggle |
| `onChange(next)` | outcome | none | pressed |

## Numbers

Well 20x20, radius 5, ground `--inst-screen`, `--e-pressed-sm`; hover (when not ticked) the ground at 80%. Tick: polyline at (0.29,0.52), (0.44,0.67), (0.72,0.36) of the well, stroke 1.9, fades in over 0.16s, drops 1px while pressed. Label sans 12.5, line height 1.45, 10px from the well, `--ink-700` (ticked and settling `--ink-400`). The well sits on the first line of the label. Wrapped labels grow the row.

## Do / do not

- Do use `settles={false}` for a setting that is on but not finished with.
- Do not use it as a switch; use Switch for that.
