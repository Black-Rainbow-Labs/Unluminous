# Completion evaluation

The harness that decides whether Unluminous's completion matches IntelliJ IDEA's
(`tasks/task-2231-autocomplete-intellisense-tdd.md` §8.1). It is the sibling of `tools/search-eval/`
and keeps the same discipline: the positions are frozen before the first run, half of them are held
out, every number is read from a run folder, and `SCORECARD.md` says what passed and what did not.

Everything a run writes goes under `D:/unluminous-completion-eval/`, never into the repository.

## The steps

```sh
node tools/completion-eval/prepare-corpora.mjs          # pinned copies of the six corpora
cargo build --release -p unluminous-cli --example completion_tokens
node tools/completion-eval/gen-positions.mjs             # writes positions.json (frozen; see below)
node tools/completion-eval/engine-unluminous.mjs --run <name> --binary <unluminous.exe> [--servers off|automatic] [--split tune|held|all]
pwsh tools/completion-eval/intellij/run.ps1 -Run <name> [-Ml on|off] [-Split tune|held|all]
cargo build --release -p unluminous-lsp --example ask
node tools/completion-eval/engine-server.mjs --run <name> --ask <ask.exe> [--split ...]   # rust-analyzer and tsserver alone
node tools/completion-eval/score.mjs --run <name> [--run <name> ...]
```

## How the engines ask

- **Unluminous** opens each corpus in a background window and, for each query, sets the tab's text
  with `editor set-text`, asks `editor complete --offset <caret> --limit 50` (with `--wait <ms>` when
  servers are on), and puts the text back with `editor undo`. The TDD named `--stem` for this; a
  hypothetical stem cannot ask a language server about text that is not in the document, so the
  engine types the prefix for real and takes it away again. With `--servers automatic` it waits for
  `status --section servers` to say the server is ready before the first query of each corpus.
- **IntelliJ** runs the `intellij/` plugin headless, with a fresh configuration and system folder
  for each run, as `intellij/README.md` describes.
- **The servers alone** run `unluminous-lsp`'s `ask` example, which starts rust-analyzer or tsserver
  once per corpus, waits for it to be ready, and answers each query with the server's rows in its
  own order, filtered on the prefix the way a client filters them.

Every engine cuts a label to the name a person types (`vec![…]` is `vec`, `draw(…)` is `draw`)
before writing it.

## Folders

| Folder | What is in it |
|---|---|
| `D:/unluminous-completion-eval/corpora/<corpus>` | The pristine pinned copy of each corpus. Never opened by an engine. |
| `D:/unluminous-completion-eval/runs/<run>/corpora/<corpus>` | The copy one engine run opens, so a project's own state files (`.unluminous`, `.idea`) never reach the pristine copy. |
| `D:/unluminous-completion-eval/runs/<run>/results.jsonl` | One line a query. |
| `D:/unluminous-completion-eval/runs/<run>/run.json` | The engine, its settings, the positions file hash, start and end times. |
| `tools/completion-eval/positions.json` | The frozen positions, in the repository. |
| `tools/completion-eval/runs/<run>.json` | The scored summary of each run, in the repository. |

## The contract every engine keeps

### `positions.json`

```json
{
  "version": 1,
  "seed": 2231,
  "corpora": {
    "unluminous": { "language": "rust", "commit": "6e5a567…" }
  },
  "positions": [
    {
      "id": "unluminous:00042",
      "corpus": "unluminous",
      "language": "rust",
      "path": "crates/unluminous-core/src/layout.rs",
      "start": 18342,
      "end": 18348,
      "expected": "anchor",
      "class": "member",
      "split": "held",
      "remove": [{ "start": 120, "end": 151 }]
    }
  ]
}
```

- `path` is relative to the corpus root, with forward slashes.
- **Offsets are bytes of the file's UTF-8 text after every `\r\n` has been turned into `\n`.** Every
  editor here holds a document with `\n` line breaks, so these are the offsets a document has.
  IntelliJ counts in UTF-16 code units and has to convert.
- `start..end` is the identifier. `expected` is its text.
- `remove` is a list of further byte ranges to delete, which is empty except for the `needs-import`
  class, where it is the import statement that brought the name into scope. A range in `remove`
  never overlaps `start..end`.
- `split` is `tune` or `held`. Only `held` decides a goal, and only once.

### A query

A query is a position and a prefix length `p` of `0`, `1`, `2` or `3` characters. An engine:

1. builds the text: the original (with `\n` line breaks), with every `remove` range deleted and
   `start..end` replaced by the first `p` **characters** of `expected`;
2. puts the caret straight after the prefix, at `start + bytes(prefix) - (bytes removed before start)`;
3. asks for basic completion there, as a person pressing `Ctrl+Space` would, and records the labels
   in the order offered;
4. puts the file back exactly as it was before the next query, and never saves it.

A prefix length longer than the identifier is skipped.

### `results.jsonl`

One JSON object a line:

```json
{"id":"unluminous:00042","prefix":2,"labels":["anchor","anchor_at_y"],"ms":41.7}
{"id":"unluminous:00043","prefix":0,"labels":[],"ms":3.1,"error":"…"}
```

- `labels` are the primary names, in offered order, at most 50. An engine normalises its own labels:
  rust-analyzer's `fn draw(…)` and tsserver's label details are cut to the name a person would type.
- `ms` is the time from asking to having the list.
- `error` is present only when the engine could not ask; such a query counts as a miss.

## Position classes

| Class | Example | How `gen-positions.mjs` recognises it |
|---|---|---|
| `member` | `self.layout.│` | the identifier follows `.` or `?.` |
| `path` | `Layout::│` | the identifier follows `::` |
| `local` | a name bound in the enclosing function | a `let`, `const`, parameter or pattern binding of that name earlier in the same enclosing definition |
| `global` | a project function or type at statement level | a name some file of the corpus defines, used where none of the classes above apply |
| `type` | after `:`, `->`, `impl`, `extends` | the significant token before it is `:` (not `::`), `->`, `impl`, `dyn`, `for` inside an `impl` head, `extends`, `implements` |
| `import` | inside `use` / `import { }` | the identifier is inside an import statement |
| `needs-import` | a project name not imported in this file | a name defined in another file and imported by name in this one; the import statement is deleted with `remove` |
| `keyword` | `fn`, `return`, `const` | one of the grammar's keywords |

Excluded: identifiers in comments and strings, single letter identifiers, and names that occur only
once in the whole corpus.
