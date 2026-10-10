# IntelliJ engine

Runs IntelliJ IDEA Ultimate's basic code completion, headless, at every query of `positions.json` and
writes `results.jsonl` and `run.json` in the format `../README.md` defines.

```powershell
pwsh tools/completion-eval/intellij/build.ps1                                   # optional; run.ps1 builds when the jar is missing
pwsh tools/completion-eval/intellij/run.ps1 -Run <name> [-Ml on|off] [-Split tune|held|all] [-Corpus <name>] [-Positions <path>] [-Limit <n>]
```

## How it works

- `run.ps1` copies each pristine corpus into `D:/unluminous-completion-eval/runs/<run>/corpora/<corpus>`
  (top level directory junctions such as `node_modules` are recreated, not copied through). Only corpora whose
  language is `rust` or `typescript` are run.
- Each corpus gets its own IDE process with a fresh `idea-config`, `idea-system`, `idea-plugins` and `idea-log`
  folder under `runs/<run>/ide-<corpus>/`, selected through `IDEA_PROPERTIES` and `IDEA_VM_OPTIONS`. The completion
  statistics (`StatisticsManager`) therefore start empty and the person's own IntelliJ is not touched. The
  `intellij-rust` plugin folder and the eval plugin are copied into `idea-plugins`; the license file `idea.key` is
  copied from the person's config so the Ultimate only plugins (Rust, JavaScript) load.
- The plugin (`src/`, `META-INF/`) registers `<appStarter id="completionEval">`. `idea64.exe completionEval job.json`
  opens the corpus with `ProjectUtil.openOrImport`, waits for smart mode and, for Rust, for the Cargo import
  (workspace, standard library and rustc info all `UpToDate`), then runs the queries file by file.
- A query edits the open document in a write command (the `remove` ranges are deleted, the identifier becomes
  the prefix), moves the caret, and calls `CodeCompletionHandlerBase.createHandler(BASIC, invokedExplicitly=true,
  autopopup=false, synchronous=false).invokeCompletion`, which is what Ctrl+Space calls. It waits until
  the completion phase leaves `CommittingDocuments` and `BgCalculation` (at most 10 s), reads
  `LookupImpl.getItems()` in display order (`LookupElement.getLookupString()`), hides the lookup, and undoes every
  edit in reverse order, falling back to `setText(original)` if the text differs. Documents are never saved.
- `CodeInsightSettings.AUTOCOMPLETE_ON_CODE_COMPLETION` is set to false so a single match is listed rather than
  inserted.
- Labels are de-duplicated (first occurrence kept) and cut to 50. `rawCount` in each line is the number of items
  before de-duplication.

### Why completion is not run in synchronous mode

`CodeCompletionHandlerBase(..., synchronous=true)` runs the contributors on the event thread. The TypeScript
service contributor waits for tsserver with `runBlockingCancellable`, which IntelliJ forbids on the event
thread, so results from the TypeScript language service were missing for some prefixes. The asynchronous
mode is what a person gets.

### Things that had to be handled

- **Rust toolchain.** A fresh IDE config has no Rust toolchain set, so the Cargo project never syncs (its status stays
  `NeedsUpdate`). `RustReady` sets `RustProjectSettings.toolchainHomeDirectory` to the folder holding `cargo` on PATH
  (`~/.cargo/bin`, the rustup shims), attaches `Cargo.toml` and waits for workspace, standard library and rustc info
  to be `UpToDate`. The standard library comes from the `rust-src` rustup component, which is installed.
  Build script evaluation reports `failed` on the corpora (dependencies are not built); completion works without it.
  The Rust plugin also downloads and indexes the crates.io index into the run's `idea-system` for Cargo.toml completion.
  That is unrelated background work.
- **TypeScript warm up.** tsserver is started by the first completion request and answers nothing until its project has
  loaded (up to about 15 s on the larger corpora). Before the first measured query the engine repeats the longest prefix
  member or path query of a file, discarding the answer, until the expected name is in the list (at most 120 tries), and
  does the same for each later file (at most 3 tries). Without it 49 of 79 queries on ai-service-ui came back empty.
  `warmUpMs` and `warmUpReady` are in `import-<corpus>.json`.
- **License.** `idea.key` is copied from the person's IntelliJ config into the fresh config. Rust and JavaScript need
  the Ultimate license.

## Measured (dev corpora and a 20 position smoke run)

- Project import: Rust 32 to 62 s from opening to a finished Cargo import (ripgrep 35 s, atrius-index 41 s, unluminous
  62 s); TypeScript 7 to 21 s plus 5 to 15 s of tsserver warm up.
- Median query: 30 to 130 ms. The first query of a Rust corpus takes about 3 s. Slowest query seen: 8 s (unluminous);
  none reached the 10 s limit.
- `-Ml on` and `-Ml off` give different orderings and different sets (with ML on the ranked items come first, and the
  50 label cap cuts at a different place).

## ML on and off

- `-Ml on`: the default IntelliJ configuration. `CompletionMLRankingSettings` has ranking enabled for Rust,
  JavaScript and TypeScript by default in a fresh config (logged at the start of each corpus log).
- `-Ml off`: `CompletionMLRankingSettings.setRankingEnabled(false)` (the master switch) and the Full Line Code
  Completion plugin (`org.jetbrains.completion.full.line`) is disabled through `disabled_plugins.txt`.

## Notes

- Project import and indexing times are in `run.json` (`openMs`, `importMs`, `cargo`) and
  `runs/<run>/intellij-<corpus>.log`.
- `dev/make-dev-positions.mjs` builds a small hand written positions file for the two dev corpora
  (`ripgrep-dev`, `ts-dev`).
- Set `COMPLETION_EVAL_TRACE=1` to log the completion phase transitions of every query.
