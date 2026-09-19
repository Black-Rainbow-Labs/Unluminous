<#
.SYNOPSIS
  Builds the project every picture in `documentation/overview.md` is taken of.

.DESCRIPTION
  It lives under the temporary folder rather than inside this repository, for the reason
  `documentation/taking-the-pictures.md` gives: `sample/` sits inside the checkout, so opening it
  where it lies makes the status bar say how many files happened to be uncommitted that day. A copy
  with a small history of its own says `main`, which is what a reader with a fresh checkout sees.

  What it puts in, and why each one is there:

    * Three commits by three authors on three widely separated dates, so the blame column has a real
      spread of ages to colour and the history has something in it.
    * A branch, so the branch dialog is not empty.
    * A change that has not been committed, so the commit panel and the gutter's change bars have
      something to show, and an untracked file, so `Unversioned Files` is not empty.
    * A file in each of several languages, because the colours are a plugin's doing and a picture of
      one language is a picture of one plugin.
    * A Markdown file with a Mermaid block in it, and a `.mmd` file of its own.
    * A picture, for the tab that shows one.
    * A `package.json` with scripts in it, so the run configurations have something to detect.

  `git -C` rather than a change of directory, so this script never moves the shell it was started
  from.

.PARAMETER At
  Where to build it. The temporary folder by default.
#>
[CmdletBinding()]
param([string]$At = (Join-Path $env:TEMP 'unluminous-docs'))

$ErrorActionPreference = 'Stop'

# Windows PowerShell's `Set-Content -Encoding utf8` writes a byte order mark, and a file that starts
# with one opens with a box in front of its first letter, because no font in the stack has a shape
# for U+FEFF. Everything here is written without one.
function Write-Text {
    param([string]$Path, [string]$Text)
    $full = Join-Path $script:Demo $Path
    $folder = Split-Path -Parent $full
    if (-not (Test-Path $folder)) { New-Item -ItemType Directory -Path $folder -Force | Out-Null }
    [System.IO.File]::WriteAllText($full, $Text, (New-Object System.Text.UTF8Encoding $false))
}

$script:Demo = $At
if (Test-Path $script:Demo) { Remove-Item -Recurse -Force $script:Demo }
New-Item -ItemType Directory -Path $script:Demo -Force | Out-Null

git -C $script:Demo init -q --initial-branch=main
git -C $script:Demo config user.name 'Jason'
git -C $script:Demo config user.email 'jason@example.com'
git -C $script:Demo config commit.gpgsign false
# Otherwise `git add` warns on every file that its line endings will be changed, and a warning on
# git's error output stops this script on the first file.
git -C $script:Demo config core.autocrlf false

# Unluminous writes what this project had open into `.unluminous`. A real project ignores it; the
# fixture does the same, so the status bar's count of changed files is the same in every picture.
Write-Text '.git/info/exclude' "# Paths listed here are ignored, and this file is not committed.`n.unluminous/`n"

Write-Text 'readme.md' @'
# Aurora

A small project, here so that the pictures of Unluminous have something in them worth looking at.
Everything below is ordinary Markdown, and the preview beside it is drawn by the same layout engine
that draws the source: `markdown::render` produces the same three things a document holds - a rope
of text, character spans over it, and one setting a paragraph - so nothing in the window knows how
to render Markdown at all.

## What is in it

- `src/layout.rs` breaks text into lines
- `src/theme.rs` holds the palette
- `src/query.ts` reads the database
- `src/probe.py` measures a run
- `src/site.css` and `src/index.html` are the page it serves
- `app.js` is the program the run configurations start

## The parser draws nothing

```rust
let preview = unluminous_core::markdown::render(&source, &base, colours, monospaced);
```

> A fenced block is coloured by the plugin that claims its language, which is the same plugin that
> colours a file of that language.

## How a request is answered

```mermaid
flowchart LR
    person[A person] --> window[The window]
    agent[An agent] --> cli[unluminous-cli]
    cli --> channel[The control channel]
    channel --> window
    window --> action[run_action and run_cli]
    action --> document[The document]
```

Both roads meet at the same function, which is what makes a thing done by hand and the same thing
done by an agent the same thing. See [the documentation](https://unluminous.com) for the rest.

## Searching by meaning

`src/query.ts` asks for the passages nearest a question rather than the passages that hold its
words, which is a different question and very often a better one. The distance is a cosine over an
embedding, and the index is asked for it rather than the table being read:

```sql
SELECT id, title, body FROM passage
 ORDER BY vector_distance_cos(v, embed('search_query: ' || ?1))
 LIMIT ?2;
```

The schema is in `schema.sql`. There is one index on the vector column and none on anything else,
because every other column here is read by the row rather than searched.

## Measuring it

`src/probe.py` runs something two hundred times and reports the median and the ninety-fifth
percentile, because a mean over two hundred samples hides the one that took a second.

| what | median | p95 |
|---|---|---|
| a search | 4.41 ms | 9.80 ms |
| an embedding | 12.0 ms | 14.4 ms |
| loading the weights | 800 ms | 860 ms |

The last row is why the weights are loaded once rather than per call, and why there are three
profiles saying when they are in memory.

## What is left

- Read the passages back and check the recall against an exhaustive scan
- Give the retrieval branch a test of its own
- Write the migration down before running it
'@

Write-Text 'chapters/one.md' "# One`n`nThe first chapter.`n"
Write-Text 'chapters/two.md' "# Two`n`nThe second chapter.`n"
Write-Text 'notes.txt' @'
A plain text file. It is prose, so it keeps the F button in the title bar, and it is not Markdown,
so there is nothing for it to preview and the three view modes are not drawn for it.
'@

Write-Text 'diagram.mmd' @'
flowchart TD
    subgraph nowindow[No user interface in any of these]
        core[unluminous-core]
        term[unluminous-terminal]
        git[unluminous-git]
        dap[unluminous-dap]
        db[unluminous-db]
        chat[unluminous-chat]
    end

    app[unluminous-app: the window]
    cli[unluminous-cli: the client]
    shared[(The command catalogue)]

    core --> app
    term --> app
    git --> app
    dap --> app
    db --> app
    chat --> app
    app --> shared
    cli --> shared
'@

Write-Text 'src/layout.rs' @'
//! Line breaking, alignment and hit testing.

use crate::metrics::FontMetrics;
use crate::style::{Align, CharStyle};

/// One laid out line: where it sits, how tall it is, and the runs of text on it.
pub struct Line {
    pub y: f32,
    pub height: f32,
    pub baseline: f32,
    pub runs: Vec<Run>,
}

/// Break `text` into lines no wider than `width`.
///
/// Breaking happens at grapheme cluster boundaries, because a letter with a combining accent is one
/// character to a reader however many bytes it takes.
pub fn layout(text: &str, width: f32, metrics: &dyn FontMetrics) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut pen = 0.0_f32;
    for word in text.split_whitespace() {
        let advance = metrics.width_of(word);
        if pen + advance > width {
            lines.push(Line { y: pen, height: 18.0, baseline: 14.0, runs: Vec::new() });
            pen = 0.0;
        }
        pen += advance;
    }
    lines
}
'@

Write-Text 'src/theme.rs' @'
//! The palette, read out of the design rather than chosen by eye.

use egui::Color32;

/// Behind the text. The window's alpha is applied to this by the opacity setting.
pub const EDITOR: Color32 = Color32::from_rgb(0x1A, 0x1F, 0x26);
/// Anything switched on: an active button, the caret, the row of the open file.
pub const ACCENT: Color32 = Color32::from_rgb(0x48, 0x9F, 0xF8);

/// Apply the opacity setting to a background colour.
///
/// Every glyph is painted at full alpha whatever this returns, which is what lets the desktop show
/// through the background while the writing stays solid.
pub fn faded(base: Color32, opacity: f32) -> Color32 {
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha)
}
'@

Write-Text 'src/query.ts' @'
import { connect, type Row } from './client';

/** One passage of a document, with the vector that was embedded from it. */
export interface Passage {
  id: number;
  title: string;
  body: string;
}

/** Reads the passages nearest a question, by meaning rather than by word. */
export async function nearest(question: string, k = 5): Promise<Passage[]> {
  const db = connect('aurora.rdb');
  const rows: Row[] = db.query(
    `SELECT id, title, body FROM passage
      ORDER BY vector_distance_cos(v, embed('search_query: ' || ?1)) LIMIT ?2`,
    [question, k],
  );
  return rows.map((row) => ({ id: row.id, title: row.title, body: row.body }));
}
'@

Write-Text 'src/probe.py' @'
"""Measures how long one frame costs, with the real fonts of this machine."""

import statistics
import time


def measure(run, repeats: int = 200) -> dict[str, float]:
    """Run `run` `repeats` times and report the median and the spread."""
    samples = []
    for _ in range(repeats):
        started = time.perf_counter()
        run()
        samples.append((time.perf_counter() - started) * 1000.0)
    return {
        "median_ms": statistics.median(samples),
        "p95_ms": sorted(samples)[int(len(samples) * 0.95)],
    }
'@

Write-Text 'src/site.css' @'
:root {
  --page: #1a1f26;
  --accent: #489ff8;
  --text: #e6e9ef;
}

.editor {
  background: var(--page);
  color: var(--text);
  display: flex;
  transition: all 120ms ease-out;
}

.editor::selection {
  background: rgba(72, 159, 248, 0.35);
}
'@

Write-Text 'src/index.html' @'
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Aurora</title>
    <link rel="stylesheet" href="site.css" />
    <style>
      body { margin: 0; background: #1a1f26; }
    </style>
  </head>
  <body>
    <!-- 5 < 3 is arithmetic here, and stays prose. -->
    <main class="editor">
      <h1>Aurora</h1>
      <p>A page the browser tab renders through the project's own origin.</p>
    </main>
    <script>
      const ready = document.querySelector('main') !== null;
      console.log('ready', ready);
    </script>
  </body>
</html>
'@

Write-Text 'app.js' @'
// The program the run configurations start and the debugger stops inside.

/** The nth number of the sequence, worked out the slow way on purpose. */
function fibonacci(n) {
  if (n < 2) return n;
  return fibonacci(n - 1) + fibonacci(n - 2);
}

/** Every number up to `count`, with how long each one took. */
function measure(count) {
  const values = [];
  for (let index = 0; index < count; index += 1) {
    const started = Date.now();
    const value = fibonacci(index);
    values.push({ index, value, milliseconds: Date.now() - started });
  }
  return values;
}

const measured = measure(12);
const total = measured.reduce((sum, row) => sum + row.value, 0);
console.log(`${measured.length} numbers, ${total} in total`);
'@

Write-Text 'config.toml' @'
[package]
name = "aurora"
version = "0.3.1"
edition = "2021"

[dependencies]
unicode-segmentation = "1.13"
'@

Write-Text 'package.json' @'
{
  "name": "aurora",
  "version": "0.3.1",
  "scripts": {
    "build": "node tools/build.mjs",
    "start": "node tools/serve.mjs",
    "test": "node --test"
  }
}
'@

Write-Text 'schema.sql' @'
CREATE TABLE passage (
    id      INTEGER PRIMARY KEY,
    title   TEXT NOT NULL,
    body    TEXT NOT NULL,
    v       VECTOR(768)
);

CREATE INDEX passage_v ON passage USING inillucent_hnsw (v) WITH (metric = 'cosine');
'@

# The picture the tab that shows one opens. It is a crop of the gallery's own backdrop, which is this
# repository's image, so nothing here needs anybody's permission to be redistributed.
Add-Type -AssemblyName System.Drawing
$backdrop = Join-Path $PSScriptRoot 'backdrop.jpg'
if (Test-Path $backdrop) {
    $source = [System.Drawing.Image]::FromFile($backdrop)
    # Larger than the editing area on purpose, so the picture is scaled to fit and the status bar has
    # a percentage worth reading.
    $crop = New-Object System.Drawing.Bitmap(2400, 1500)
    $g = [System.Drawing.Graphics]::FromImage($crop)
    $g.InterpolationMode = 'HighQualityBicubic'
    # A crop rather than the whole plate: the picture in the tab and the thing behind the window
    # would otherwise be the same image, which reads as a fault rather than as a photograph.
    $from = New-Object System.Drawing.Rectangle(0, [int]($source.Height * 0.30), [int]($source.Width * 0.55), [int]($source.Height * 0.55))
    $to = New-Object System.Drawing.Rectangle(0, 0, 2400, 1500)
    $g.DrawImage($source, $to, $from, [System.Drawing.GraphicsUnit]::Pixel)
    $codec = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq 'image/jpeg' }
    $parameters = New-Object System.Drawing.Imaging.EncoderParameters(1)
    $parameters.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter([System.Drawing.Imaging.Encoder]::Quality, [long]92)
    $g.Dispose()
    New-Item -ItemType Directory -Path (Join-Path $script:Demo 'images') -Force | Out-Null
    $crop.Save((Join-Path $script:Demo 'images/aurora.jpg'), $codec, $parameters)
    $crop.Dispose(); $source.Dispose()
}

git -C $script:Demo add -A
$env:GIT_AUTHOR_DATE = '2026-01-14T09:00:00+00:00'
git -C $script:Demo commit -q -m 'the first commit'
Remove-Item Env:\GIT_AUTHOR_DATE -ErrorAction SilentlyContinue

Write-Text 'src/version.ts' "export const version = '0.3.1';`n"
# Each commit touches `src/query.ts` as well, so its history has three entries by three authors and
# its blame column has three ages to colour. A file every commit leaves alone has nothing to show.
Write-Text 'src/query.ts' ((Get-Content (Join-Path $script:Demo 'src/query.ts') -Raw) + @'

/** How many passages there are, which is what a page count is worked out from. */
export async function total(): Promise<number> {
  const db = connect('aurora.rdb');
  const rows: Row[] = db.query('SELECT count(*) AS n FROM passage');
  return Number(rows[0].n);
}
'@)
git -C $script:Demo add -A
git -C $script:Demo -c user.name='Sam Okafor' -c user.email='sam@example.com' commit -q --date '2026-03-02T11:00:00+00:00' -m 'add a version and a count'

Write-Text 'src/client.ts' @'
/** The one row shape every query answers in. */
export interface Row {
  [column: string]: string | number | null;
}

export function connect(file: string) {
  return { query: (_sql: string, _params: unknown[]): Row[] => [] };
}
'@
Write-Text 'src/query.ts' ((Get-Content (Join-Path $script:Demo 'src/query.ts') -Raw) + @'

/** The passages whose text holds every word of `terms`, exactly as they were typed. */
export async function matching(terms: string[]): Promise<Passage[]> {
  const db = connect('aurora.rdb');
  const rows: Row[] = db.query(
    'SELECT id, title, body FROM passage WHERE passage MATCH ?1',
    [terms.join(' ')],
  );
  return rows.map((row) => ({ id: row.id, title: row.title, body: row.body }));
}
'@)
git -C $script:Demo add -A
git -C $script:Demo -c user.name='Kim Rivera' -c user.email='kim@example.com' commit -q --date '2026-07-21T16:00:00+00:00' -m 'add the client and a keyword search'

git -C $script:Demo switch -q -c aurora/retrieval
Write-Text 'src/retrieval.ts' "export const hybrid = true;`n"
git -C $script:Demo add -A
git -C $script:Demo commit -q -m 'start the retrieval branch'
git -C $script:Demo switch -q main

# Uncommitted, so the gutter has a change bar and the commit panel has a file in it.
Write-Text 'src/version.ts' "export const version = '0.3.1';`n// a line that has not been committed`n"
Write-Text 'scratch.txt' "an untracked file`n"

# The database every picture in `documentation/database.md` is taken of. It is written **beside** the
# project rather than inside it, so it never appears in the explorer and the pictures of the window
# are not changed by a page about the Database plugin.
$library = Join-Path (Split-Path -Parent $script:Demo) 'unluminous-docs-library.db'
node --no-warnings (Join-Path $PSScriptRoot 'library-db.mjs') $library | Write-Output

Write-Output "Built $script:Demo"
