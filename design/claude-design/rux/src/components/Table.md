# Table

A table on a recessed screen: silkscreen headers, hairline rows, figures in tabular mono and right aligned, long words wrapped to three lines.

Source: `components/table.rs` (`Table`, `Column`, `Align`, `is_figure`). No reference CSS rule.

## When to use

Rows of records inside an instrument module: crates with build times and sizes, releases with dates. The order is the caller's; a header press only reports `onSort`.

## Props

| Prop | Rust | Default | Meaning |
|---|---|---|---|
| `columns` | `Column { label, align }` | `[]` | `[{ label, align: 'left' \| 'right' \| 'centre' }]` |
| `rows` | `rows: &[Vec<String>]` | `[]` | `string[][]`, one array of cells per row |
| `sorted` | `.sorted(Some((col, desc)))` | none | `[columnIndex, descending]`: its header is brighter, shows a triangle and a blue LED |
| `onSort(columnIndex)` | `TableOutcome.sort` | none | a header was pressed |

Also exported: `isFigure(cell)` (starts with a digit after any of `$ £ € + - −`, and at least half the non space characters are digits) and `columnWidths(columns, rows, width)`.

## Numbers

Screen radius 9. Header 28px, silk type (sans 9.5, 600, 0.12em, uppercase) in `--ink-400`, `--ink-700` when sorted or hovered. Rows at least 27px, inset 3px; 1px `--inst-rule` line across the top from 6px in; `--hover` wash with radius 4 under the pointer. Cells have 6px either side; words sans 12 `--ink-500` wrapped to at most three lines with an ellipsis; figures mono 11.5 `--ink-700`, one line, right aligned unless the column is centred; the first column is `--ink-900`. 4px of screen below the last row.

Column widths: each column wants its widest cell or header plus 12; if all fit, the first column takes what is left; otherwise columns under an even share keep their width and the rest share the remainder in proportion, with a 40px floor.

## Do / do not

- Do right align a column of figures and give it a short header.
- Do not sort inside the component; sort `rows` and pass `sorted`.
