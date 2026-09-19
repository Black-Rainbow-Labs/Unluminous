// What the palette's contrast ratios really are, against WCAG 2.2.
//
// `task-1804` §3.4: *"there is no mention of contrast, colour-blindness or WCAG in the 458-line style
// guide."* This is the missing measurement rather than an opinion about it — the palette is a closed
// list in `crates/unluminous-app/src/theme/mod.rs`, so the ratios can be computed from the source of
// truth rather than sampled off a screenshot, and they can be computed again the day a colour moves.
//
// ## What the numbers mean
//
// WCAG 2.2 asks for **4.5:1** for ordinary text, **3:1** for text at 18pt or 14pt bold, and **3:1**
// for the boundary of a control somebody has to find. A ratio is between two *rendered* colours, so
// every pair below names the ground it was measured against: `text_dim` on `editor` and `text_dim`
// on `status_bar` are different numbers and the palette has to pass both.
//
// **It is measured at full opacity**, and that is a limit worth stating: Unluminous's background is
// translucent, so what is really behind the text is the desktop at `1 - opacity`. A ratio that
// passes here can fail on a pale wallpaper at 40 per cent. Text is painted at full alpha and the
// grounds are not, which is why the honest measurement is of the palette and the honest statement is
// that a person choosing a low opacity is choosing lower contrast.
//
// ## What `--check` is for
//
// `task-1984` T3 put this in both release scripts' gate, and a gate is only worth having if it is
// green — so what it answers is **has a pair got worse than it was**, not *does the whole palette
// meet WCAG 2.2*. Five pairs are under the bar today, `design/accessibility.md` says which and says
// plainly that screen-reader support does not ship in 1.0, and moving those five colours is a change
// to the product's look that belongs in its own ticket rather than in a review's clean-up.
//
// So each of the five is listed in [`ACCEPTED`] with the ratio it has now, and `--check` fails when
// a pair that is not on that list drops under its bar, when an accepted pair drops **below the
// number written down**, or when an accepted pair has been fixed and the row is still there. The
// last of those is what stops the list rotting, and is the shape
// `exactly_one_command_is_held_back` already has.
//
//   node tools/contrast.mjs            the table
//   node tools/contrast.mjs --check    exit 1 if a pair is worse than it was

import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const repo = join(dirname(fileURLToPath(import.meta.url)), '..')
const source = readFileSync(join(repo, 'crates/unluminous-app/src/theme/mod.rs'), 'utf8')

/** Every `name = Color32::from_rgb(r, g, b);` in the palette macro's own list. */
function palette() {
  const found = new Map()
  const pattern = /^\s{4}([a-z_]+) = Color32::from_rgb\(0x([0-9A-Fa-f]{2}), 0x([0-9A-Fa-f]{2}), 0x([0-9A-Fa-f]{2})\);$/gm
  for (const [, name, r, g, b] of source.matchAll(pattern)) {
    found.set(name, [parseInt(r, 16), parseInt(g, 16), parseInt(b, 16)])
  }
  return found
}

/** WCAG relative luminance. */
function luminance([r, g, b]) {
  const channel = value => {
    const v = value / 255
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/** The contrast ratio between two colours, 1:1 to 21:1. */
function ratio(a, b) {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (high + 0.05) / (low + 0.05)
}

// Every pair the window really draws: what is written, and what it is written on. Taken from the
// components rather than from every combination, because a ratio between two colours that never
// meet is a number about nothing.
const pairs = [
  ['text', 'editor', 'body text in the editing area', 4.5],
  ['text_strong', 'editor', 'the file name, and a value in a dialog', 4.5],
  ['text_control', 'menu', 'a menu entry', 4.5],
  ['text_control', 'control', 'the word on a button', 4.5],
  ['text_control', 'title_bar', 'the project name in the title bar', 4.5],
  ['text_control', 'explorer', 'a file name in the explorer', 4.5],
  ['text_control', 'status_bar', 'the file name in the status bar', 4.5],
  ['text_dim', 'editor', 'the line numbers, and a shortcut beside a menu entry', 4.5],
  ['text_dim', 'status_bar', 'the kind, the line ending and the caret position', 4.5],
  ['text_dim', 'explorer', 'a folder that is not open', 4.5],
  ['text_dim', 'menu', 'a menu entry that cannot be used just now', 3],
  ['text_faint', 'editor', 'a placeholder, and the match count on the Find bar', 4.5],
  ['text_faint', 'explorer_footer', 'the file count under the explorer', 4.5],
  ['text_strong', 'accent', 'the word on the button that does the thing', 4.5],
  ['text_strong', 'selected_row', 'the file that is open, in the explorer', 4.5],
  ['accent', 'editor', 'the caret, and the underline under a linked word', 3],
  ['control_border', 'editor', 'the edge of a control, which has to be findable', 3],
  ['divider', 'editor', 'the line between two panels', 3],
  ['unsaved', 'status_bar', 'the dot that says there are unsaved changes', 3],
  ['git_added', 'editor', 'an added line in a diff', 3],
  ['git_modified', 'editor', 'a changed line in a diff', 3],
  ['git_untracked', 'explorer', 'a file git has never seen', 3],
  ['file_markdown', 'explorer', 'the mark beside a Markdown file', 3],
  ['file_text', 'explorer', 'the mark beside a plain file', 3],
  ['icon', 'title_bar', 'a drawn icon in the rail', 3],
  ['close', 'title_bar', 'the close button', 3],
  ['maximise', 'title_bar', 'the maximise button', 3],
  ['agent', 'editor', "an agent's name on a ticket", 3],
]

const colours = palette()
const rows = pairs.map(([front, back, what, needs]) => {
  const a = colours.get(front)
  const b = colours.get(back)
  if (!a || !b) return { front, back, what, needs, got: 0, missing: true }
  return { front, back, what, needs, got: ratio(a, b), missing: false }
})

const failing = rows.filter(row => row.missing || row.got < row.needs)

// The five pairs that are under WCAG 2.2 today, each with the ratio it has and why it is where it is.
// `design/accessibility.md` is the page that records them for a reader; this is the same list in the
// form a gate can read. See the note at the top of this file for what `--check` does with it.
const ACCEPTED = [
  ['text_faint', 'editor', 4.17, 'a placeholder and a match count, which are deliberately quiet'],
  ['text_faint', 'explorer_footer', 4.11, 'the same colour on the explorer’s own footer'],
  ['text_strong', 'accent', 2.77, 'the word on the primary button; the accent is the brand colour'],
  ['control_border', 'editor', 1.56, 'the edge of a control, drawn as a hairline by design'],
  ['divider', 'editor', 1.25, 'the line between two panels, drawn as a hairline by design'],
]

function acceptedFor(row) {
  return ACCEPTED.find(([front, back]) => front === row.front && back === row.back)
}

if (process.argv.includes('--check')) {
  const worse = []
  for (const row of failing) {
    const accepted = acceptedFor(row)
    if (!accepted) {
      worse.push(
        `${row.front} on ${row.back}: ${row.got.toFixed(2)}:1, needs ${row.needs}:1 — ${row.what}`,
      )
      continue
    }
    // A rounding step below what was written down, so a colour that moved by a hair does not fail.
    if (row.got < accepted[2] - 0.01) {
      worse.push(
        `${row.front} on ${row.back}: ${row.got.toFixed(2)}:1, and it was ${accepted[2]}:1 — ${row.what}`,
      )
    }
  }
  // A row that has been fixed and left on the list: take it off, or the list stops meaning anything.
  for (const [front, back] of ACCEPTED) {
    if (!failing.some(row => row.front === front && row.back === back)) {
      worse.push(
        `${front} on ${back} meets WCAG 2.2 now. Take it out of ACCEPTED in tools/contrast.mjs.`,
      )
    }
  }
  if (worse.length > 0) {
    for (const line of worse) console.error(line)
    console.error('\nSee design/accessibility.md, which records the pairs that are accepted and why.')
    process.exit(1)
  }
  console.log(
    `${rows.length - failing.length} of ${rows.length} pairs meet WCAG 2.2, and the other ` +
      `${failing.length} are no worse than design/accessibility.md records.`,
  )
} else {
  console.log('| what is drawn | ratio | needs | |')
  console.log('|---|---|---|---|')
  for (const row of rows) {
    const mark = row.missing ? 'no such colour' : row.got >= row.needs ? 'passes' : '**FAILS**'
    console.log(`| ${row.what} (\`${row.front}\` on \`${row.back}\`) | ${row.got.toFixed(2)}:1 | ${row.needs}:1 | ${mark} |`)
  }
  console.log(`\n${rows.length - failing.length} of ${rows.length} pairs meet WCAG 2.2.`)
}
