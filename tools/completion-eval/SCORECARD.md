# Scorecard: Unluminous completion against the reference editor

`task-2232` built what `tasks/task-2231-autocomplete-intellisense-tdd.md` designs: a structural tier read
by Atrius, a semantic tier driven by rust-analyzer and tsserver, and one weigher chain that orders the
rows of all three. This page is the held out gate, the record of every run looked at on the way, and
what the result means. Every number is read from a run folder under `D:/unluminous-completion-eval/runs`,
named beside it, and the scored summaries are in `tools/completion-eval/runs/`.

The reference editor is IntelliJ IDEA 2025.3.4 Ultimate with its Rust and JavaScript plugins, run
headless by `intellij/` with a fresh configuration for each run.

## The outcome

| Goal | Bar | Result | Verdict |
|---|---|---|---|
| G1 quality | On the held out positions, R@1 and R@5 within 3 points of the reference editor on every class, and MRR within 0.03, for Rust and TypeScript | Rust R@1 56.9 against 66.6, MRR 0.663 against 0.745. TypeScript R@1 40.1 against 58.1, MRR 0.513 against 0.663. One class meets the bar, Rust `type`. | **not met** |
| G2 structural completion with no server | members, signatures, auto import, locality and history for every Atrius grammar, measured on Python and Go | Python R@1 18.2 → 35.9. Go 0.0 → 37.9, because Go had no plugin before. Python `member` is worse (21.1 → 15.7). | **met, with one class worse** |
| G3 nothing slower at the keyboard | the synchronous work per keystroke under 5 ms on `app/realm.rs`, and no frame waits on a server | worst stem 2.66 ms; `self.` 0.12 ms; the read at a new text revision 2.97 ms, reported apart as the TDD asks | **met** |
| G4 time to a list with the right answer | p50 under 100 ms and p95 under 300 ms with a warm server | Rust p50 42 ms, p95 176 ms. TypeScript p50 40 ms, p95 123 ms. | **met** |
| G5 a missing or dead server degrades visibly | the footer says what the server is doing; no command silently does nothing | footer states, `status --section servers`, structural rows with the server stopped; tested in `tests/completion_semantic.rs` | **met** |
| G6 agents get the same answers | `editor complete` returns the popup's rows; `editor signature` exists; the MCP tools come from the catalogue | same `completion_offer` path; `editor complete --wait/--after`, `editor signature`, `status --section servers`; catalogue and `commands.md` tests pass | **met** |

The positions were frozen before the first run and have not changed. The bar has not been moved. G1
is not met, and this page gives its numbers.

## G1: quality on the held out positions

Positions file `positions.json`, sha256 `6aa511e384632b60fdb626df1f4177c4aa544793c615ea14e1d3526850b4e65d`,
seed 2231, held half only. 11,917 Rust queries and 9,165 TypeScript queries, answered by every run.

Runs:

- `gate-unluminous-on`: the gate binaries, servers on, `--wait 1000 --first`, run once.
- `baseline-intellij-ml-on`: the reference editor in its default setup, with ML ranking on.
- `intellij-ml-off-held`: the reference editor with ML ranking off.
- `gate-server`: rust-analyzer and tsserver alone, through `unluminous-lsp`'s `ask` example.
- `gate-unluminous-off`: Unluminous with `editor.servers = off`.

### Rust

| Class | Unluminous R@1 / R@5 / MRR | Reference, ML on | Reference, ML off | rust-analyzer alone | Unluminous, no server |
|---|---|---|---|---|---|
| member | 60.9 / 83.4 / 0.708 | 66.1 / 89.8 / 0.761 | 52.6 / 77.0 / 0.635 | 52.3 / 76.1 / 0.627 | 28.8 / 43.1 / 0.350 |
| path | 71.9 / 88.1 / 0.792 | 77.0 / 92.3 / 0.838 | 62.4 / 83.6 / 0.721 | 65.7 / 82.0 / 0.730 | 30.2 / 39.3 / 0.341 |
| local | 66.2 / 87.3 / 0.761 | 79.3 / 94.6 / 0.864 | 58.4 / 81.9 / 0.687 | 68.1 / 80.8 / 0.744 | 63.1 / 94.4 / 0.770 |
| global | 52.8 / 78.5 / 0.647 | 65.6 / 88.4 / 0.753 | 34.3 / 62.2 / 0.471 | 46.0 / 70.2 / 0.569 | 35.3 / 63.4 / 0.471 |
| type | 69.7 / 81.8 / 0.753 | 71.3 / 83.3 / 0.773 | 44.6 / 61.6 / 0.529 | 67.2 / 77.0 / 0.718 | 25.7 / 47.3 / 0.353 |
| import | 53.8 / 78.3 / 0.645 | 58.2 / 80.4 / 0.678 | 49.9 / 71.6 / 0.592 | 49.4 / 71.9 / 0.590 | 19.4 / 29.2 / 0.239 |
| needs-import | 25.0 / 50.0 / 0.356 | 51.2 / 65.5 / 0.592 | 22.2 / 48.3 / 0.337 | 9.4 / 25.4 / 0.164 | 33.9 / 58.9 / 0.443 |
| keyword | 54.3 / 74.6 / 0.639 | 63.3 / 74.4 / 0.694 | 60.0 / 68.9 / 0.653 | 37.7 / 55.3 / 0.453 | 41.2 / 58.0 / 0.493 |
| **all** | **56.9 / 77.7 / 0.663** | **66.6 / 83.6 / 0.745** | 47.9 / 69.3 / 0.577 | 49.6 / 67.4 / 0.576 | 34.7 / 54.4 / 0.433 |

### TypeScript

| Class | Unluminous R@1 / R@5 / MRR | Reference, ML on | Reference, ML off | tsserver alone | Unluminous, no server |
|---|---|---|---|---|---|
| member | 53.4 / 70.9 / 0.610 | 60.1 / 78.6 / 0.682 | 42.3 / 65.3 / 0.532 | 39.9 / 55.5 / 0.473 | 21.7 / 28.6 / 0.249 |
| local | 56.0 / 88.6 / 0.708 | 69.0 / 86.6 / 0.776 | 60.9 / 87.5 / 0.724 | 49.2 / 69.6 / 0.590 | 48.8 / 80.1 / 0.627 |
| global | 35.9 / 62.3 / 0.471 | 43.0 / 69.4 / 0.548 | 37.1 / 62.3 / 0.484 | 38.7 / 65.0 / 0.501 | 16.1 / 45.7 / 0.283 |
| type | 35.6 / 63.1 / 0.492 | 68.2 / 85.9 / 0.764 | 38.5 / 61.8 / 0.504 | 16.9 / 42.4 / 0.273 | 22.1 / 42.9 / 0.314 |
| import | 44.5 / 68.4 / 0.552 | 59.4 / 74.0 / 0.663 | 42.7 / 59.4 / 0.511 | 28.3 / 43.9 / 0.362 | 20.8 / 34.2 / 0.262 |
| needs-import | 12.1 / 32.1 / 0.218 | 29.2 / 43.4 / 0.359 | 15.1 / 28.2 / 0.209 | 10.0 / 14.4 / 0.133 | 29.0 / 45.4 / 0.369 |
| keyword | 37.5 / 58.4 / 0.475 | 71.2 / 86.9 / 0.780 | 51.7 / 72.1 / 0.613 | 25.8 / 38.1 / 0.328 | 37.8 / 57.4 / 0.466 |
| **all** | **40.1 / 64.4 / 0.513** | **58.1 / 75.9 / 0.663** | 42.1 / 63.6 / 0.521 | 30.5 / 47.8 / 0.387 | 28.7 / 48.6 / 0.375 |

### What these numbers say

- **Against the default reference editor**, Unluminous is about 10 R@1 points behind on Rust and 18 on
  TypeScript. Only Rust `type` is within the bar.
- **Against the reference editor with ML ranking off**, Unluminous is 9 points ahead on Rust and level
  on TypeScript (40.1 against 42.1 R@1, 64.4 against 63.6 R@5). Most of the remaining gap is the
  reference editor's trained ranking model.
- **Against the servers on their own**, Unluminous is 7 points ahead on Rust and 10 on TypeScript.
  The merge and the weigher chain add to what the servers know, and do not just pass it through.
- **The largest single gap is names that need an import.** Rust is at 25.0 against 51.2. With the
  server on, its auto import rows from dependencies still crowd out the project's own names. With the
  server off, Unluminous does better on this class (33.9), because it offers only the project's names.
- **TypeScript `type` and `keyword`** are 33 points behind. After a `:`, the reference editor puts the
  primitive types and the file's own interfaces first. tsserver sends hundreds of global DOM types in
  the same sort group, and the chain does not yet tell them apart.

## G2: structural completion with no server

`gate-unluminous-off` against `gate-baseline-0.69.1-controls`, the 0.69.1 release on the same held
control positions. 3,191 Python queries and 1,996 Go queries.

| Class | Python, this build | Python, 0.69.1 | Go, this build | Go, 0.69.1 |
|---|---|---|---|---|
| member | 15.7 / 22.5 / 0.189 | 21.1 / 41.1 / 0.305 | 20.7 / 29.9 / 0.249 | 0.0 |
| local | 58.0 / 86.2 / 0.713 | 30.4 / 53.3 / 0.401 | 52.0 / 68.1 / 0.592 | 0.0 |
| global | 42.9 / 69.6 / 0.541 | 8.3 / 32.3 / 0.199 | 24.3 / 39.4 / 0.355 | 0.0 |
| type | 27.0 / 46.0 / 0.361 | 27.0 / 46.4 / 0.361 | (no positions) | |
| import | 9.3 / 19.8 / 0.141 | 4.5 / 12.2 / 0.083 | (4 positions) | |
| needs-import | 36.0 / 65.1 / 0.490 | 8.8 / 29.4 / 0.198 | (no positions) | |
| keyword | 59.1 / 71.1 / 0.644 | 30.4 / 51.7 / 0.395 | 55.0 / 66.8 / 0.593 | 0.0 |
| **all** | **35.9 / 54.8 / 0.444** | 18.2 / 37.6 / 0.273 | **37.9 / 51.1 / 0.447** | 0.0 |

Go scored nothing in 0.69.1 because no plugin claimed `.go`. This build reads Atrius's own manifest for
any extension no plugin claims (`services::plugins::store::settle`), so every Atrius grammar gets
completion. Python `member` is the one class that got worse. After a `.` with a type the structure
cannot work out, 0.69.1 offered every word of the file. This build offers only the members it can find
by name. A Python server would answer this class. The TDD scopes servers to Rust and TypeScript, so this
build has none for Python.

## G3: the keystroke

`cargo run --release -p unluminous-app --example completion_cost` with
`COMPLETION_COST_FILE=crates/unluminous-app/src/app/realm.rs` (202 KB), on a quiet machine:

| Measure | ms |
|---|---|
| worst stem (`d`, 1,314 gathered, 1,125 offered) | 2.657 |
| members after `self.` (22 offered) | 0.122 |
| inside `use unluminous_core::` | 2.299 |
| the read at a new text revision (symbols and words) | 2.968 |
| the structural read | 4.881 |

The structural read is not on the keystroke. A keystroke that moves the line count by two lines or fewer
uses the tab's last structure, and a frame with no input reads it again
(`app::gather::LINES_A_KEYSTROKE_MAY_MOVE`, `refresh_the_tab_structure_when_idle`). No frame waits on a
server: replies are read once a frame from a channel, and the popup is merged when they arrive.

## G4: time to a list with the right answer

`node tools/completion-eval/latency.mjs --run gate-unluminous-on --split held`. The popup draws the
structural rows at once and merges the server's rows when they arrive. A query's time is the first
list's when the expected name is in its top ten, and the first list's plus the wait for the server when
it only arrives with the server's rows.

| Language | queries with a right list | right in the first list | right after the server | never in a top ten | p50 ms | p95 ms |
|---|---|---|---|---|---|---|
| Rust | 10,086 | 8,884 | 1,202 | 1,831 | 42 | 176 |
| TypeScript | 6,520 | 5,803 | 717 | 2,645 | 40 | 123 |

The servers themselves are slower. rust-analyzer alone has a p95 of 598 ms, and tsserver alone 1,511 ms
with a maximum of 5 s. Most of their slow answers come when nothing is typed, or on the largest corpus.

## G5 and G6

- **G5** is covered by `tests/completion_semantic.rs`:
  - `a_server_that_cannot_start_says_so_and_the_structure_still_answers` checks the footer reads
    `Rust · rust-analyzer stopped`, with a picture, while the structure's rows still appear.
  - `with_servers_off_no_server_is_started` checks `editor.servers = off`.
  - A window that is killed takes its servers with it, through a Windows job object. That was checked by
    killing a window and asking Windows for its rust-analyzer.
- **G6** is covered by `the_command_line_answers_the_servers_rows_and_signature_and_its_state` and
  `every_catalogue_command_is_driven_both_ways` in `tests/command_line.rs`. The documentation and MCP
  tests in `unluminous-cli` pass.

## How the harness works

- **The positions** come from eight corpora pinned in `corpora.json`: `unluminous`, `atrius-index` and
  `ripgrep` for Rust, `ai-service-ui`, `inillucent-ts` and `zod` for TypeScript, and `requests` (Python)
  and `cobra` (Go) as controls. `gen-positions.mjs` drew 13,203 positions across eight classes, split
  half and half by a hash of each id, and froze them.
- **A query** is a position with the expected identifier cut to 0, 1, 2 or 3 letters.
- **The engines** are described in `README.md`. Labels are cut to the name a person would type before
  scoring.
- **Each Unluminous run** uses a fresh copy of each corpus and its own `APPDATA`. With servers on, it
  waits until the server has stayed ready for 15 seconds before asking anything.

## The record of every run looked at

The tune half only, until the gate. In order:

| Run | What it was | Rust R@1 / MRR | TypeScript R@1 / MRR |
|---|---|---|---|
| `baseline-unluminous-0.69.1` | 0.69.1, full tune split. **Not valid**: the engine sent text with backslash escapes that `editor set-text` decoded, which shifted every later offset in many files. Kept as a record. | 9.5 / 0.139 | 15.8 / 0.233 |
| `baseline-unluminous-0.69.1-b` | 0.69.1 again with the engine fixed. **The baseline.** | 18.2 / 0.269 | 18.7 / 0.277 |
| `baseline-intellij-ml-on` | the reference editor, ML on, every position (its held half is the gate's reference) | 66.6 / 0.744 (tune) | 57.3 / 0.657 (tune) |
| `smoke-new-off`, `smoke-new-on`, `smoke2-new-off`, `smoke2-new-on` | 50 positions of `atrius-index`, servers off and on, before and after the first ranking changes | | |
| `tune-a-on` | 30 tune positions from each of six corpora, servers on. `ripgrep` had no server: rust-analyzer was reported absent on a folder whose default toolchain lacks it, since fixed. | 28.9 / 0.376 | 35.7 / 0.477 |
| `tune-a-server` | rust-analyzer and tsserver alone, same positions | 46.6 / 0.580 | 29.2 / 0.419 |
| `tune-b-on` | the server's order leads; a capital typed first; keywords as near as locals at a statement; the exact row offered | 43.3 / 0.552 | 40.4 / 0.532 |
| `tune-c-on` | the half typed word not offered from the file's words; Enter on the typed word stays a new line; dependency imports farther | 47.8 / 0.604 | 39.8 / 0.537 |
| `tune-d-on` | how often the file writes each word ranks ties; keywords fit a type place | 52.0 / 0.627 | 46.8 / 0.604 |
| `tune-e-on`, `tune-f-on` | a kindless row equal to the stem dropped; dependency names before the server's order. These two ran at the same time, so many waits timed out and their totals are not comparable. Classes not affected by timeouts moved: Rust `path` 70.5 → 84.1, `needs-import` 24.3 → 29.7. | 51.7 / 0.622, 51.1 / 0.621 | 46.5 / 0.601, 46.5 / 0.592 |

Runs `dev1` to `dev10`, `smoke1` and `smoke2` are the reference editor's engine being developed, on a
separate positions file under `intellij/dev`, not the frozen one. `intellij-ml-off-held` was run on the
held half and was not scored until the gate.

The gate binaries are the build `tune-f-on` used, copied to `D:/unluminous-completion-eval/bin/gate`,
from commit `39cbf9a4` on `task-2232`.

## Where the design differs from the TDD

- **The token dump is an example** (`unluminous-cli/examples/completion_tokens.rs`), not a hidden command.
- **The Unluminous engine types the prefix for real** with `editor set-text` and takes it away with
  `editor undo`. The TDD named `editor complete --stem`, but a hypothetical stem is not in the document,
  so no server can be asked about it.
- **Atrius's symbol and import tables are keyed by path**, not by file id, so compacting the index does
  not change them.
- **The weigher chain differs from the one in TDD §6.3**, in four places, each moved while tuning:
  - the first letter's case comes right after the match class
  - names from dependencies come after all others
  - the server's order comes before the place and locality
  - how often the file writes a name ranks ties
- **The row equal to the stem is offered**, as the reference editor offers it. `task-1678` dropped it.
  Enter on it is still a new line, and a list holding only the typed word does not open.
- **A server call snippet `name(${1:arg})$0` inserts `name()`** with the caret inside. The parameter
  names are shown by the signature line.
- **One letter stems draw at most 1,000 names from the project index.**
- **A grammar from Atrius's own manifests** is used for any extension no plugin claims, which is how Go
  gets completion.

## What would close the gap

These are measured gaps, not things this ticket did:

- **Rust needs-import.** Rank rust-analyzer's auto import rows by whether their crate is a workspace
  member, which `Cargo.toml` says, instead of by whether the path starts with `crate::`. Most of the
  project's own names arrive through another workspace crate's name and are treated as dependencies
  today.
- **TypeScript types.** Give the file's own interfaces and the primitive types a place ahead of global
  library types after a `:`. tsserver puts them all in one sort group.
- **A ranking model.** The reference editor without ML is level with or behind this build. The gap to
  its default is mostly its trained ranking model. The selection statistics this build keeps are its
  first input.
