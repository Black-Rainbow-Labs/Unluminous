# task-2231: completion equal to IntelliJ, for Rust and TypeScript first

## 1. Introduction

Unluminous completes words. It offers the definitions and distinct words of the open tabs, the names
the project index knows, and the keywords of the language, ranked by a subsequence scorer, in under
5 ms on the UI thread. That is the design of task-1677, and it does what it was built to do. It does
not know what `self.layout.` can be followed by, which `use` path a name needs, what a function's
parameters are, or whether the name under the caret is a type or a local. IntelliJ knows all of
those, and that knowledge is most of what people mean when they say IntelliJ's completion is good.

This document designs completion that is measured against IntelliJ IDEA on the same cursor
positions and is expected to match it for Rust and TypeScript, while every other language Unluminous
colours gets better completion for free. It records the one decision that makes this possible, which
reverses a decision this repository made in task-1675, and it defines the evaluation that decides
whether the work succeeded.

The reading behind it: the current completion code (`crates/unluminous-core/src/completion.rs`,
`crates/unluminous-app/src/app/completion.rs`), the Atrius index (`C:/jason/dev/atrius-index`), the
JetBrains platform sources and blog posts on completion, the rust-analyzer `ide-completion` crate,
the TypeScript `tsserver` protocol, the LSP 3.17 specification, and the JetBrains Evaluation Plugin
(`plugins/evaluation-plugin` in the `intellij-community` release tags). Section 9 lists the sources.

## 2. Goals and non goals

### Goals

| # | Goal | Bar |
|---|---|---|
| G1 | **Quality equal to IntelliJ on Rust and TypeScript.** | On the held out positions of section 8, Recall@1 and Recall@5 within 3 points of IntelliJ IDEA 2025.3 on every position class, and MRR within 0.03, for both languages. |
| G2 | **Every language Unluminous colours gets structural completion.** | Members by container, signatures in the row, auto import of project names, and a ranking with locality and selection history, for all 24 Atrius grammars, with no language server installed. Measured on Python and Go as the control languages. |
| G3 | **Nothing gets slower at the keyboard.** | The synchronous work per keystroke stays under 5 ms on `app/realm.rs` (`examples/completion_cost.rs`), and no frame waits on a language server. |
| G4 | **The popup opens as fast as IntelliJ's.** | Time from keystroke to a list with the right answer: p50 under 100 ms and p95 under 300 ms with a warm language server, measured by the harness. IntelliJ's own target is 150 ms. |
| G5 | **A missing or dead language server degrades to G2, visibly.** | The footer says what the server is doing or why it is absent. No command silently does nothing. |
| G6 | **Agents get the same answers.** | `unluminous-cli editor complete` returns the same rows the popup shows, with the same ranks, plus the new `editor signature` command. The MCP tools come from the catalogue as they do today. |

### Non goals

- A type checker or name resolver of our own. IntelliJ's Rust engine is years of work by a team;
  this document does not propose rebuilding it.
- Full line or AI completion. The ranker here is deterministic weighers plus selection statistics.
  A learned ranker is section 7's follow up once the harness exists to train and judge it.
- Hover, go to definition, rename or diagnostics from the language server. The client is built so
  they can follow, but this ticket wires completion and signature help only.
- Snippet placeholders with tab stops. Section 6.7 inserts call parentheses and puts the caret
  inside them. Placeholders are a later ticket.
- Languages beyond the Atrius grammars, and language servers beyond rust-analyzer and tsserver.

## 3. Problem statement

What exists, from the code as read on 2026-10-09:

- **Four sources, one scorer, no structure.** `completion_candidates` gathers the active tab's
  definitions and distinct words, other tabs' definitions, up to 2,000 names from
  `services::symbol_index`, and the grammar's keywords. `completion::rank` scores them with a
  subsequence match (prefix +30, boundary +12, consecutive +6, same case +2, unmatched −1). A
  definition is "a definer keyword followed by a word" and carries a name, a kind out of five, a
  confidence and an exported flag. Nothing records what a function takes or returns, what type a
  method belongs to, what a struct's fields are, or what a file imports.
- **Nothing after a dot.** Outside notebooks, typing `self.` or `foo::` offers nothing, because the
  automatic popup needs a two letter stem and the pool has no notion of members. In a notebook the
  kernel is asked (task-2229), which is the only semantic completion in the editor today.
- **The Atrius index is not used by the window.** The window keeps its own in memory
  `symbol_index` (name, kind, path) and uses `atrius-index` only to register grammars. Atrius holds
  the same five kind definitions in memory plus a signature line, and nothing about containers,
  parameters, imports or references in its database.
- **No auto import.** task-1680 completes a name inside an import statement, and deliberately
  stops short of adding the import a name needs. Section 12 of that document lists what it would
  take: knowing what is already imported, where the import block ends, and which exporter was meant.
- **No ranking signals beyond the match.** No locality, no selection history, no expected type, no
  kind preference. `foo` from a file in another crate ranks beside the local `foo`.
- **No signature help, no documentation, no insertion beyond the name.** Accepting `draw` inserts
  `draw`; IntelliJ inserts `draw()` with the caret inside and shows the parameters.
- **No measurement of quality.** `examples/completion_cost.rs` measures time. Nothing measures
  whether the right row was offered, and nothing compares to another editor.

What IntelliJ does that produces the experience, from the sources in section 9:

- A copy of the file with a dummy identifier at the caret is parsed, so the tree around the caret
  is always valid, and contributors answer against resolved names and inferred types.
- The lookup is filtered by a camel hump matcher and ordered by a chain of weighers: match
  quality, kind and context preference (locals first, expected type, same type members), proximity
  (same file, same directory, same module), selection statistics (`StatisticsManager`, what the
  person chose for this prefix in this context), then an ML sorter that reorders the top five.
- Items carry an insert handler: parentheses, the import the name needs, a follow up popup.
- The popup opens 100 ms after the first contributor returns, and the rest fills in.
- Quality is measured by the Evaluation Plugin: delete a token, invoke completion, record the
  rank of the real token, compute Recall@k, MRR, mean rank and latency per position class.

The gap is semantic. Match quality and ranking can be improved on the existing tier and will be,
but Recall@1 on `receiver.` positions is zero until something knows the receiver's type.

## 4. The decision: a structural tier for every language, a semantic tier from language servers

task-1675 chose a syntactic tier and rejected a language server client, for four reasons that are
still true: a server is a separate program per language, it is found on `PATH` or not at all, it
answers when it pleases, and a refactor that silently does nothing when the server is missing is
the worst outcome. task-1680, 1681 and 1686 repeated the decision. This document reverses it for
completion and signature help in Rust and TypeScript, and only there, because:

1. **The goal needs name resolution and type inference, and there are two sources of each.**
   JetBrains built their own for Rust. For TypeScript they did not: WebStorm runs `tsserver` as a
   separate process and merges its answers with its own contributors
   (https://www.jetbrains.com/help/webstorm/typescript-support.html). The alternative to a server is
   writing a Rust type checker, which is a non goal.
2. **Each of the four reasons has an answer this repository already uses.** `unluminous-dap`
   drives a separate program (a debug adapter) over a protocol on a worker thread with a scripted
   counterpart in the tests, because debugging had no syntactic tier either. The language server
   client is the same shape. "Found on `PATH`" is answered by looking where the toolchain puts the
   server (`rustup`'s `rust-analyzer` component next to `cargo`; the project's own
   `node_modules/typescript/lib/tsserver.js`, which is what WebStorm uses) and by saying in the
   footer what was found. "Answers when it pleases" is answered by the structural tier answering
   first, every time, in under 5 ms, with the server's rows merged in when they arrive. "Silently
   does nothing" is answered by G5: the state is drawn, and the structural rows are always there.
3. **Everything else stays data.** The structural tier is driven by manifest keys, as today, so a
   plugin for a language with no server still gets members, signatures, auto import and the
   ranking model. The server tier is two adapters behind one trait, named by a manifest key that
   names a built in adapter kind, checked and refused with a message if unknown, as `plugin.kind`
   and `debug.adapter` are.

What does **not** change: no tree-sitter, no async runtime, no debounce timer, no per language
list inside the binary, no network fetch, no new graphics device, and the window never blocks a
frame on a server. The editor with no server installed is strictly better than today's.

## 5. Architecture

```mermaid
flowchart LR
    subgraph UI thread, one frame
        K[keystroke] --> Q[Question at caret<br/>stem, receiver, statement kind]
        Q --> S1[Structural tier<br/>Atrius symbols, open tabs, grammar words]
        Q --> C[Server cache<br/>last answer for this position]
        S1 --> M[Merge and rank<br/>weigher chain + stats]
        C --> M
        M --> P[Popup / editor complete]
    end
    Q -. ask, if a server is up .-> W
    subgraph Worker threads
        W[unluminous-lsp worker] --> RA[rust-analyzer<br/>LSP over stdio]
        W --> TS[tsserver<br/>tsserver protocol over stdio]
        A[Atrius store thread] --> DB[(inillucent index.rdb<br/>file, blob, trigram,<br/>symbol, import, passage)]
    end
    RA -. rows .-> C
    TS -. rows .-> C
    C -. waker .-> K
    A -. symbols in memory .-> S1
```

Three tiers, one pipeline:

| Tier | What it knows | Where | Latency | Present when |
|---|---|---|---|---|
| Lexical (today's) | words and definitions of open tabs, grammar words | `unluminous-core` | synchronous, under 5 ms | always |
| Structural (new) | every definition in the project with kind, container, signature, parameters, return text, fields, visibility, doc line; imports and exports per file | Atrius symbol table, persisted in `index.rdb`, in memory in the window | synchronous, under 5 ms | always, for every Atrius grammar |
| Semantic (new) | resolved members of the receiver's type, expected type, the import a name needs, signature help, documentation | rust-analyzer, tsserver, via `unluminous-lsp` | asynchronous, 20 to 300 ms warm | when the plugin names an adapter and the server is found |

A keystroke always produces a list from the first two tiers in the same frame. If a server is up, the
same question goes to it, and when its rows arrive the list is re ranked and redrawn. The popup never
waits. The person sees the structural list at once and the semantic list a moment later, which is
exactly IntelliJ's "show after the first contributor, fill in the rest" behaviour.

### 5.1 One question, three answers

`Question` is built once per keystroke in `app/completion.rs` from the caret, and is what every tier
and the CLI read:

```rust
pub struct Question {
    pub path: PathBuf,
    pub offset: usize,                 // byte offset in the document
    pub stem: Range<usize>,            // completion::stem_at
    pub word: Range<usize>,            // completion::word_at (Tab replaces this)
    pub receiver: Option<Receiver>,    // expressions::at, extended: `a.b.` -> ["a","b"], separator
    pub place: Place,                  // Statement | Expression | Type | Pattern | Import(Context) | Argument | Markup(...)
    pub trigger: Trigger,              // Typed(char) | Invoked | Continued
    pub revision: u64,                 // Document::text_revision
}
```

`Place` is read backwards from the caret by `unluminous_core::place::at`, a sibling of
`imports::context_at` and `expressions::at`, using the same tokens. It is a heuristic and says so
(`Place::Unknown` is allowed); the semantic tier does not need it, the structural tier uses it to
pick which kinds to offer (types after `:` and `->` and `impl`, nothing in a string, modules in a
`use`), and the ranker uses it as a feature.

### 5.2 The structural tier: Atrius learns the shape of a definition

Today's `outline::Definition` in Atrius is `name, kind (5 values), line, end, start, signature line,
likely, exported, depth`. The structural tier extends it, in `atrius-index` (released there, version
raised here, as the repository rule says):

```rust
pub struct Symbol {
    pub name: String,
    pub kind: Kind,            // Function, Method, Type, Struct, Enum, Variant, Trait, Interface, Class,
                               // Field, Constant, Variable, Module, Parameter, TypeAlias, Macro
    pub container: Option<String>,   // the type or module whose block holds it: "Layout" for `impl Layout { fn x }`
    pub signature: String,     // the head line, as today
    pub parameters: Vec<Parameter>,  // name and type text, from the head line
    pub returns: Option<String>,     // return type text, from the head line
    pub type_text: Option<String>,   // a field's or variable's annotated type, or constructor name
    pub visibility: Visibility,      // Public, Crate, Private; from export_keyword and language.visibility
    pub doc: Option<String>,   // first line of the preamble comment
    pub line: u32, pub end: u32, pub start: u32, pub range: Range<usize>,
    pub likely: bool, pub depth: u32,
}
pub struct Import { pub path: Vec<String>, pub alias: Option<String>, pub glob: bool, pub range: Range<usize> }
```

How it is read, still from `syntax::scan` tokens and `folding::regions`, with no new parser:

- **Container.** A definition whose name range lies inside a `Kind::Block` region whose head line
  starts with a word in `language.containers` (`impl`, `trait`, `struct`, `enum`, `mod` for Rust;
  `class`, `interface`, `enum`, `namespace`, `type … = {` for TypeScript) takes that head's type name
  as container. `impl Trait for Type` takes `Type`. This is the `depth` calculation that exists,
  recording the parent instead of counting it.
- **Fields.** Inside a `struct`, `class`, `interface` or object type block, a line of the shape
  `name: Type` (with `language.annotation = :`) is a `Field` with `type_text`. Enum bodies give
  `Variant`.
- **Parameters and return.** From the head line between the first `(` and its `)`, split on commas
  outside brackets, each `name: Type` or `name` is a `Parameter`; the text after
  `language.returns` (`->` for Rust, `:` after the `)` for TypeScript) up to `{` or `=` or `;` is
  `returns`. The existing truncation at 200 characters stays.
- **Variables.** `let x: T`, `const x: T`, `let x = T::new(…)`, `let x = new T(…)`, `let x = T {` give
  `type_text` from the annotation or the constructor. Everything else is `None`. This is the only
  type inference the structural tier does, and section 6.4 says how it is used.
- **Imports.** `imports::use_statements_in` and `specifiers_in` already read Rust `use` and quoted
  imports for the move refactor; they are stored per file. Exports are the `exported` flag plus
  `export { a, b }` lists, as today.

New manifest keys, all optional, all off unless a language names them, checked and refused if they
name something unknown: `language.containers`, `language.annotation`, `language.returns`,
`language.visibility` (`pub=public, pub(crate)=crate` for Rust; `private, protected` for
TypeScript), `language.members` (the separators that reach a member: `., ::` for Rust; `., ?.`
for TypeScript), `language.server` (section 5.3).

**Persistence.** A `symbol` table and an `import` table join `file`, `blob` and `trigram` in
`index.rdb`, written by the same store thread in the same per batch transaction, keyed by `file.id`
and rebuilt per file when the gate finds the file changed. `SCHEMA_VERSION` goes to `3`. The in
memory `SymbolTable` gains `by_container: HashMap<String, Vec<usize>>` and
`by_prefix: Vec<(lowercase name, index)>` sorted, so "members of `Layout`" and "names starting
with `la`" are a binary search, not a walk. Loading from the table on open replaces rebuilding
from the blobs, which is what makes the first keystroke after opening a project answer from the
whole project.

```sql
CREATE TABLE IF NOT EXISTS symbol (id INTEGER PRIMARY KEY, file_id INTEGER, name TEXT, kind TEXT,
    container TEXT, signature TEXT, parameters TEXT, returns TEXT, type_text TEXT, visibility TEXT,
    doc TEXT, line INTEGER, end_line INTEGER, start_line INTEGER, byte_start INTEGER, byte_end INTEGER,
    likely INTEGER, depth INTEGER);
CREATE INDEX symbol_file ON symbol(file_id); CREATE INDEX symbol_name ON symbol(name);
CREATE INDEX symbol_container ON symbol(container);
CREATE TABLE IF NOT EXISTS import (id INTEGER PRIMARY KEY, file_id INTEGER, path TEXT, alias TEXT,
    glob INTEGER, byte_start INTEGER, byte_end INTEGER);
CREATE INDEX import_file ON import(file_id);
```

**The window uses Atrius and drops `services::symbol_index`.** The window already depends on
`atrius-index` and the repository rule already says the window may be the checkout's one host. It
opens the index in process (`Index::open`, `Hosting::InProcess`, as `mcp/driver.rs` does), reads
symbols through `Index::with_symbols`, and stops building its own 200,000 definition table on a
second thread. The ownership rule stands: an open tab's symbols come from its live `TabSymbols`,
and Atrius's copy of an open file is never offered beside it (`by_file` is skipped for open paths,
as `symbol_index` skips them today). `MOST_FROM_THE_INDEX` stays as the cap on how many prefix
matches one stem draws; with `by_prefix` the cap is reached by a range scan, not by walking every
name, which is what task-1984 measured as the cost.

`outline`, `def`, `fragment` and the `code-search` MCP tool get the richer rows for free, which is
a second reason to put the structure in Atrius rather than in the window.

### 5.3 The semantic tier: `unluminous-lsp`

A new crate in the shape of `unluminous-dap`: no UI dependency, a worker thread per server, a
`Scripted` counterpart in the tests that answers from a fixed script, and nothing that blocks a
frame.

```rust
pub trait Semantic: Send {
    fn open(&mut self, path: &Path, text: &str, language: &str);
    fn change(&mut self, path: &Path, revision: u64, edits: &[Edit]);   // incremental, from Document's edit log
    fn close(&mut self, path: &Path);
    fn complete(&mut self, q: &Question, text: &str) -> Ticket;         // answers arrive as Reply::Completions
    fn resolve(&mut self, item: &ItemHandle) -> Ticket;                 // docs, detail, import edits
    fn signature(&mut self, path: &Path, offset: usize) -> Ticket;
    fn state(&self) -> ServerState;                                     // Starting | Indexing(progress) | Ready | Failed(reason) | Absent(reason)
}
pub enum Reply { Completions { ticket, revision, incomplete: bool, items: Vec<Item> },
                 Resolved { ticket, item: Item }, Signature { ticket, help: Option<SignatureHelp> },
                 State(ServerState) }
pub struct Item {
    pub label: String, pub filter: String, pub sort: String, pub kind: Kind, pub detail: Option<String>,
    pub doc: Option<String>, pub deprecated: bool, pub preselect: bool,
    pub edit: Edit,                       // the server's own range and text, bytes, this revision
    pub extra_edits: Vec<Edit>,           // auto import; may be empty until resolved
    pub snippet: bool, pub needs_resolve: bool, pub handle: ItemHandle,
    pub relevance: Relevance,             // parsed from rust-analyzer sortText / tsserver sortText groups, section 6.3
}
```

Two adapters, chosen by the plugin manifest key `language.server = lsp | tsserver`, with
`language.server_command` naming the program and `language.server_args` its arguments. Unknown
adapter names are refused with a message at plugin load, as `debug.adapter` is.

- **`lsp`** (rust-analyzer): JSON-RPC over stdio with `Content-Length` framing, written by hand on
  top of `serde_json`, as `unluminous-dap` does for DAP. `initialize` with
  `positionEncodings: ["utf-8"]` (rust-analyzer accepts it; byte offsets then convert to line and
  byte column with no UTF-16 arithmetic), `textDocument.completion.completionItem.snippetSupport`,
  `resolveSupport.properties: ["documentation","detail","additionalTextEdits"]`,
  `insertReplaceSupport`, `labelDetailsSupport`, `completionList.itemDefaults`. Then
  `didOpen`/`didChange` (incremental), `textDocument/completion` with `context.triggerKind` and
  `triggerCharacter`, `completionItem/resolve`, `textDocument/signatureHelp`, and
  `$/progress` for the indexing state. rust-analyzer always answers `isIncomplete: true`, so every
  keystroke re asks (with `triggerKind: 3`) while the last answer is filtered locally in the
  meantime. Settings sent in `initializationOptions`: `completion.callable.snippets =
  "add_parentheses"`, `completion.autoimport.enable = true`, `completion.postfix.enable = true`,
  `completion.limit = 200`. The server is looked for at `language.server_command`, then next to
  `cargo` (`rustup`'s `rust-analyzer` component), then on `PATH`. If none is found the state is
  `Absent("rustup component add rust-analyzer")` and the footer shows it.
- **`tsserver`**: the tsserver protocol over stdio (one JSON object per line, `seq`/`request_seq`),
  which is what WebStorm speaks. Commands: `configure` (`preferences` with
  `includeCompletionsForModuleExports`, `includeCompletionsWithInsertText`,
  `includeCompletionsWithSnippetText`, `includeCompletionsForImportStatements`,
  `allowIncompleteCompletions`, `useLabelDetailsInCompletionEntries`), `open`, `change`,
  `completionInfo` (with `triggerCharacter` and `triggerKind`), `completionEntryDetails` (the
  resolve; its `codeActions` are the auto import edits), `signatureHelp`, `projectInfo`/
  `projectLoadingFinish` events for state. Positions are 1 based line and UTF-16 offset, converted
  at the adapter boundary. The program is `node` with `<project>/node_modules/typescript/lib/tsserver.js`,
  found by walking up from the file to the nearest `node_modules/typescript`, which is what
  WebStorm does by default; a global `typescript` is the fallback; none gives
  `Absent("no typescript in node_modules")`. Talking to tsserver directly rather than through
  `typescript-language-server` keeps `isNewIdentifierLocation`, `isMemberCompletion`, `data` and the
  `sortText` groups, which section 6.3 uses, and needs nothing installed beyond the project's own
  dependency.

**Process rules**, from the DAP and terminal experience recorded in CLAUDE.md: one server per
language per project root, started on the second frame after a file of that language is opened
(never before the first frame), stopped on `on_exit` with a join, restarted at most three times an
hour on crash, and never restarted while a request is in flight. A server that has not answered
`initialize` in 30 s is `Failed`. Nothing is read from the network.

**Threading.** The worker owns the child process and the two pipes. The UI thread sends requests
over a channel and never waits; replies land in `Mailbox<Reply>` and the worker calls
`services::wake::the_run_loop`, the same waker the terminal and git use, so a frame is requested
only when there is something to draw. A `Ticket` is a generation number; a reply for a stale
revision or an older ticket is dropped on arrival, which replaces cancellation (rust-analyzer's
`$/cancelRequest` is also sent, cheaply, so the server stops work it does not need to do).

### 5.4 Merge and rank

The three tiers produce `Candidate`s with the same shape, extended from today's:

```rust
pub struct Candidate {
    pub name: String, pub filter: String, pub source: Source,   // + Source::Server, Source::Member, Source::Import
    pub kind: Option<Kind>, pub detail: String,
    pub insert: Insert,            // Name | Edit(Edit) | Call { name, has_params }
    pub extra_edits: Vec<Edit>,    // the import to add
    pub features: Features,        // section 6.3: what the weighers read
}
```

Merging is by `(name, kind, container)`: a server row and a structural row for the same method
become one row carrying the server's `insert`, `detail` and `relevance` and the structural row's
`doc` until resolve replaces it. A server row with no structural twin is kept. A structural row
whose name the server list contains under a different kind is dropped (the server knows better).
Rows from a server answer for an older `revision` are kept only until the next answer arrives and
are filtered by the current stem locally, which is what every LSP client does while `isIncomplete`
answers are outstanding.

Ranking is section 6.3. The row equal to the stem is still never offered, Enter still replaces the
stem, Tab still replaces the word, the five keys are still the only keys consumed, and the popup is
still an `egui::Area` drawn after the pane loop.

## 6. Detailed design

### 6.1 Triggers

| Event | Today | New |
|---|---|---|
| Two letters of a word typed | automatic popup | unchanged |
| `.`, `::`, `?.` typed (per `language.members`) outside a comment or string | nothing | popup opens with an empty stem: structural members of the receiver (6.4) at once, server rows when they arrive. This is `Trigger::Typed('.')` and the server gets `triggerCharacter`. |
| `(` or `,` typed, or caret moved inside parentheses | nothing | signature help asked of the server; nothing drawn until it answers (6.8). The structural tier draws the head line's parameter text for a function it knows when no server is up. |
| One letter typed after a trigger character | nothing | the empty stem list is filtered to the letter; no re ask unless `incomplete` |
| `Ctrl+Space`, `Edit -> Complete Word` | manual popup from 1 letter, anywhere | unchanged; a second `Ctrl+Space` while open widens the list to names needing an import (IntelliJ's second invocation; rust-analyzer flyimport rows are already in the first answer and are ranked low by 6.3, so the second press only lifts the import penalty) |
| Caret moves without an edit | popup closes | unchanged |
| `editor.suggestions = manual` | no automatic popup | unchanged, and `language.members` triggers are automatic popups so they obey it too |

The `Document` edit log already exists for undo; `change()` sends each `Command`'s byte ranges as
incremental edits, so the server never receives the whole file after `open`.

### 6.2 Matching

`completion::could_match` and the alignment scorer stay, and two things are added on top, because
IntelliJ's matcher behaves differently in ways people's fingers know:

- **Humps.** A stem whose letters each start a word of the candidate (`NPE` for
  `NullPointerException`, `lsm` for `layout_scroll_margin`, `scm` for `selectionRectsIn`) is a
  `Match::Humps`, ranked above a subsequence match of the same prefix quality. The existing
  `BOUNDARY` bonus rewards this partially; the new matcher names it as a class so the weigher can
  prefer it outright, the way Zed's first tier requires the first stem letter to start a word.
- **Match classes**, in order: `Exact`, `Prefix`, `Humps`, `WordStart` (the stem is a prefix of a
  later word: `scroll` in `layout_scroll`), `Subsequence`. The class is the first weigher; the
  alignment score orders within a class. Case: `FIRST_LETTER` like IntelliJ, so `La` prefers
  `Layout` over `layout` but still offers both.
- **Filter text.** A server row is matched on `filter`, never on `label` (rust-analyzer labels
  carry signatures, tsserver labels can be `["a-b"]`).

Tests pin orderings, never scores, as today.

### 6.3 Ranking: a chain of weighers

The flat score becomes a sort key built from weighers, compared in order, and the rules below are
the design, written so the tests can pin them one at a time:

| # | Weigher | Reads | Rule |
|---|---|---|---|
| 1 | match class | 6.2 | Exact > Prefix > Humps > WordStart > Subsequence |
| 2 | expected type | server `relevance` (rust-analyzer `type_match`, tsserver `isRecommended`) | type matched first |
| 3 | place fit | `Question.place`, kind | after `:`/`->`/`impl`, types before values; in a `use`, modules and types; in a pattern, variants; in a statement, locals and functions before types |
| 4 | locality | source, file, container | receiver's own members > locals and parameters of the enclosing function (from `first_written` and parameters) > this file > open tabs > same folder > same crate or package > project > needs import > grammar words |
| 5 | selection statistics | `services::stats` | a row chosen before for this stem prefix in this `Place` and language ranks first among equals; decays by count, persisted per person in `services::store`, as IntelliJ's `StatisticsManager` |
| 6 | server order | `sort`, parsed | rust-analyzer: the inverted score in `sortText` as a number; tsserver: the group (`10` locals … `16` auto imports, `z` deprecated) as a number |
| 7 | deprecated | tags, `kindModifiers` | last within its class |
| 8 | alignment score, length, bytes | today's scorer | ties, deterministic |

A server's `preselect` row with a `Prefix` or better match wins outright. Weigher 5 lives in a new
`services::stats` with a `name -> (stem prefix, place, language, count, last chosen)` map, written
on `WINDOW_SETTLE` like every other per person setting, never per keystroke. The chain is a pure
function in `unluminous_core::completion::order(question, rows, stats) -> Vec<Row>` so the CLI and
the popup cannot disagree and the harness can call it with no window.

### 6.4 Members of a receiver, with and without a server

For `receiver.` the structural tier answers in the same frame, from `by_container`:

1. **Known type.** The receiver's type is read from, in order: the enclosing `impl`/`class` block
   when the receiver is `self`/`this`; a parameter or `let` with `type_text` in the enclosing
   function (from `first_written` extended to return the symbol, not only the range); a field with
   `type_text` when the receiver is `self.field`; a constructor call on the same line
   (`Foo::new()`, `new Foo()`, `Foo {`). Generics are stripped (`Vec<Layout>` reads as `Vec`, and
   `Option<T>` reads as `Option`), and references (`&`, `&mut`, `Box<T>`) are looked through. Then
   `by_container[type]` plus the members of every `impl Trait for Type` block are the rows, with
   `Source::Member`.
2. **Unknown type.** The rows are every member of every container whose name matches the stem, each
   detailed with its container (`draw · Layout`, `draw · Painter`). This is what WebStorm does for
   untyped JavaScript, and what IntelliJ does on a second invocation. With an empty stem after the
   dot and an unknown type the structural list is empty rather than the whole project; the server
   fills it.
3. **A server answer replaces 1 and 2** for the rows it covers (5.4). The person sees the structural
   guess for 20 to 100 ms and then the resolved list, and in the common case they are the same rows
   in the same order, so nothing visibly moves.

The accuracy of step 1 is measured by the harness as its own class (`member, structural only`),
so the follow up list in section 7 can be chosen by numbers.

### 6.5 Auto import

A row with `Source::Import` is a name the project exports from a file this file does not import,
and a server row with `extra_edits` (rust-analyzer `import_to_add` after resolve; tsserver
`codeActions` from `completionEntryDetails`) is the same thing with the server's edit. Accepting it:

1. Resolves the row if `needs_resolve` (a round trip, awaited by the accept, not by a frame: the
   name is inserted at once, and the import edit is applied as a second `Command` when it arrives,
   still inside the same typing run so undo removes both).
2. Without a server, the structural tier writes the import itself: Rust gets a `use crate::…::Name;`
   placed by `imports::use_statements_in` (after the last `use` at the same nesting, alphabetical
   among them, merged into an existing `use crate::module::{…}` when one exists); TypeScript gets
   `import { Name } from './relative';` with the specifier from `services::imports::write_specifier`,
   merged into an existing import from the same specifier. Both are the move refactor's rewriting
   code, used in the other direction.
3. Ambiguity (two files export `Name`) shows both rows, detailed by file, never a guess. This is the
   repository rule and it is what task-1680 §12 asked for.

The row is detailed with the import it would add (`Layout · use crate::layout`), ranked by weigher 4
below everything already in scope, and the whole insertion is one undo step as `ReplaceMany`.

### 6.6 What a row shows

`components/completion.rs` gains: the kind glyph for the new kinds (Method, Field, Variant,
Interface, Class, Parameter, Macro, Snippet, Keyword), the signature as the row detail
(`fn draw(&self, ui: &mut Ui) -> Response` cut to the column), a `deprecated` strike through, and a
documentation panel to the right of the list that shows the selected row's `doc` and fills from
`resolve` when a server is up. The panel is asked for when the selection rests on a row for one
`HEARTBEAT` (no new timer; the heartbeat is already requested every frame while the popup is
open) and is drawn when the reply arrives. Width grows from 360 to 480 points for the list; the
panel is another 360. Everything is painted at absolute positions in `theme::size`, with
`widget_info` names `Completion <name>` and `Completion documentation`.

### 6.7 Insertion

`Insert::Name` is today's behaviour. `Insert::Edit` applies the server's range and text (an
`InsertReplaceEdit` chooses `insert` on Enter and `replace` on Tab, which is what the two keys
already mean here). `Insert::Call` inserts `name()` with the caret between the parentheses when the
function has parameters and `name()` with the caret after when it has none (rust-analyzer
`add_parentheses`; tsserver `insertText` with `includeCompletionsWithInsertText`), and then asks for
signature help. A snippet `insertText` (`$0`, `$1`, `${1:x}`) is reduced to its plain text with the
caret at `$0` or at the first `$1`; placeholders are a non goal. Every insertion is one
`Command::ReplaceMany`, so one undo step, and the server receives it as one `change`.

### 6.8 Signature help

A new `app/signature.rs` and `components/signature.rs`: a one line `egui::Area` above the caret line
showing the active signature with the active parameter in bold, from `textDocument/signatureHelp`
or tsserver `signatureHelp`, re asked on `(`, `,` and `)` and on caret movement inside the call, closed
when the caret leaves the parentheses or on Escape. It takes no keys. Without a server, the
structural tier shows the head line of the one function the index knows by that name, with no
active parameter, so the information is there in every language.

### 6.9 State, footer and settings

- `ServerState` is drawn in the footer beside the language (`rust-analyzer: indexing 42%`,
  `rust-analyzer: ready`, `tsserver: not found in node_modules`, `rust-analyzer: crashed 3 times,
  stopped`) and is in `unluminous-cli status --section servers`. The question "what is the server
  doing" is asked of the worker, never of a port.
- Settings: `editor.servers = automatic | off` (default `automatic`; `off` is the switch for
  someone who wants the editor to run nothing). `editor.suggestions` is unchanged.
- Nothing is written to the project folder by the server tier. rust-analyzer writes its own cache
  in `target/`, as it does for every editor.

### 6.10 Commands and tools

Catalogue rows and `run_cli` arms, with a section each in `unluminous-cli/docs/commands.md` and an
agent study scenario:

| Command | Change |
|---|---|
| `editor complete` | rows gain `kind`, `container`, `signature`, `source`, `needsImport`; `--wait <ms>` (default 300) waits for the server's answer when one is up, so an agent gets the resolved list; `--stem` remains read only and now also takes `--after "."` to ask a hypothetical member question |
| `editor signature` | new: the active signature and parameter at the caret, read only |
| `status --section servers` | new section |
| `search def`, `outline`, `fragment` | rows gain container, parameters, returns, visibility, doc |

The `code-search` tool answers improve with no change to the tool, and the MCP tools are generated
from the catalogue as always.

## 7. Follow ups this design enables, not in scope

- Hover, go to definition, references and diagnostics from the server, through the same client,
  each a ticket that adds a `Semantic` method and a command.
- A learned reorder of the top five (IntelliJ's `MLSorter`) trained on the harness's feature
  logs, once the deterministic chain is measured.
- Snippet placeholders with tab stops.
- Python through `pyright`/`jedi-language-server` as a third `lsp` adapter use, which is one
  manifest line once the client exists; Go through `gopls` likewise.
- Local type inference past constructors in the structural tier, if the harness's
  `member, structural only` class shows it is worth it.

## 8. Testing and evaluation

### 8.1 The harness: `tools/completion-eval/`

The sibling of `tools/search-eval/`, with the same discipline: positions frozen before the first
run, a held out split, every number read from a run folder, and a `SCORECARD.md` that says what
passed and what did not with the numbers.

**Positions.** `gen-positions.mjs` reads a corpus through `unluminous-cli search outline` and the
scanner (`unluminous-cli` gains a hidden `search tokens <path>` for this, which prints identifier
tokens with their byte ranges and `Place`) and writes `positions.json`: one record per chosen
identifier with `path`, `byte offset of the identifier start`, `expected` (the identifier text),
`class`, and `prefix` lengths `0, 1, 2, 3`. Classes:

| Class | Example | Why it is its own class |
|---|---|---|
| `member` | `self.layout.|` | needs the receiver's type; where IntelliJ wins today |
| `path` | `std::collections::|`, `Layout::|` | module and associated item resolution |
| `local` | a name bound in the enclosing function | should be rank 1 almost always |
| `global` | a project function or type at statement level | structural tier's home ground |
| `type` | after `:`, `->`, `impl`, `extends` | place fit |
| `import` | inside `use` / `import { }` | task-1680's ground, re measured |
| `needs-import` | a project name not imported in this file | auto import |
| `keyword` | `fn`, `return`, `const` | should never be worse than today |

Sampling: 2,000 positions per language per corpus, stratified by class, seeded. Positions inside
comments and strings, single letter identifiers, and identifiers that are the only occurrence of
their name in the corpus are excluded, as TreeRanker excludes dunders and sub five item lists.

**Corpora.** Rust: `unluminous` itself and `atrius-index`. TypeScript: `ai-service/ui`
(Next.js, React) and `inillucent`'s TypeScript driver. One more of each from outside Jason's own
code, cloned at a pinned commit, so the ranker is not tuned to one style: `ripgrep` for Rust,
`zod` for TypeScript. Each corpus is copied to a scratch folder per run (a project remembers its
open tabs; measure on a copy).

**Engines.** Each takes a position, produces the file with the identifier cut to its prefix, and
returns the ordered labels and the time to the list:

- *Unluminous*: a background window (`unluminous --background`, `tools/drive-a-window.ps1`), driven by
  `unluminous-cli editor complete --stem <prefix> --wait 1000` at the position, so the read only
  hypothetical path is exercised and the document is never changed between positions. Run twice,
  once with `editor.servers = off` (structural only) and once `automatic` (with servers), because
  G2 and G1 are different bars.
- *IntelliJ IDEA 2025.3.4 Ultimate* (installed on this machine with the Rust and JavaScript plugins,
  `C:/Program Files/JetBrains/IntelliJ IDEA 2025.1`, product build 253.32098.37): a small plugin
  with an `ApplicationStarter`, the pattern of `CompletionEvaluationStarter`, that opens the
  project, waits for smart mode, and for each position opens the file, sets the caret, invokes
  `CodeCompletionHandlerBase(CompletionType.BASIC)`, reads the active lookup's items and writes
  them to JSON. Run headless (`-Djava.awt.headless=true`) with a fresh config and system path per
  run so `StatisticsManager` is empty, with ML ranking and full line completion off for the
  baseline and on for a second run (both are reported; the bar in G1 is the default configuration,
  which is ML on). The JetBrains Evaluation Plugin's `token-completion` feature is the reference
  for the method and its metrics, and is used directly if its Rust and TypeScript support turns out
  to be reachable from IDEA's bundled plugins; the custom starter is the fallback and is the plan.
- *rust-analyzer alone* and *tsserver alone*, driven by the same `unluminous-lsp` crate's
  `examples/ask.rs`, sorted by `sortText`, filtered on `filterText`. These separate "what the
  server knows" from "what the merge and ranking did with it", which is the table that finds
  ranking bugs.

**Metrics**, per engine, language, class and prefix length: Recall@1, Recall@5, Recall@10, MRR,
miss rate (expected label absent from the list), list length, p50 and p95 latency. Labels are
normalised to the primary name before comparison (rust-analyzer appends signatures; tsserver adds
`labelDetails`). The per position table of "IntelliJ ranked it higher than we did" is written out
in full, because that is where the fixes come from.

**Gate.** G1 is read on the held out half only, once, from a run whose positions file hash matches
the frozen one, as `search-eval` does. The scorecard records every run that was looked at while
tuning so the held out number is not a tuned number.

### 8.2 Automated tests

Favouring tests that drive the real pipeline, per the repository's four layers:

**Layer 1, `unluminous-core` and `atrius-index` (no window).**
- `structure::read` on fixture files for Rust and TypeScript: container, fields, parameters,
  returns, variables' `type_text`, visibility, doc line, for `impl`, `impl Trait for`, nested
  `mod`, `class`/`interface`/`enum`/`namespace`, arrow function constants, and a file that is only
  half typed. Every assertion is on the symbol list, not on a snapshot.
- `place::at` on a table of caret positions.
- `completion::order`: one test per weigher row in 6.3, each pinning an ordering and nothing
  else; a determinism test; a test that a stale server answer filtered by a longer stem never
  shows a row that cannot match.
- Match classes: `Humps`, `WordStart`, `FIRST_LETTER` case rule, filter text over label.
- Atrius: the `symbol` and `import` tables round trip; a changed file replaces its rows in one
  transaction; `by_container` and `by_prefix` agree with a linear scan on a random symbol table.

**Layer 2, `unluminous-lsp` (no window, scripted server).**
- A `Scripted` server that answers from a script, the `unluminous-dap` pattern, exercising:
  framing, `initialize` capabilities, incremental `didChange` ranges in UTF-8 and the tsserver
  UTF-16 conversion on a line with emoji, `isIncomplete` re asking, `itemDefaults` applied to
  items, `resolve` filling `additionalTextEdits`, a reply for a stale ticket dropped, a reply for
  an older revision kept only until the next, a crash restarted at most three times, a server
  that never answers `initialize` marked `Failed` at 30 s (clock injected), and `state()` after
  each.
- The real servers, behind `UNLUMINOUS_REQUIRE_SERVERS=1` (the `UNLUMINOUS_REQUIRE_KERNEL`
  pattern: skipped when absent, failed when required): rust-analyzer on a two file crate answers
  `self.|` with the struct's methods, and tsserver on a two file project answers `obj.|` with the
  interface's properties and `completionEntryDetails` returns an import `codeAction`.

**Layer 3, `crates/unluminous-app/tests/` with `egui_kittest`, using `builder()` from the test
file, never a device of its own.** New binary `completion_semantic.rs` with the scripted server
plugged into the window:
- Typing `self.` opens the popup with structural members in the same frame; the scripted answer
  arrives; the list re ranks; the row count and first row are asserted, and a screenshot
  `completion_members.png` is taken once.
- Accepting a `Call` row inserts `name()` with the caret inside and opens signature help
  (`signature_help.png`).
- Accepting a `needs-import` row adds the `use` line and the name in one undo step; `Ctrl+Z` once
  removes both.
- Without a server, accepting a `needs-import` row writes the import the structural tier
  composes, in Rust and in TypeScript, merged into an existing block.
- The documentation panel fills after one heartbeat of rest (`completion_documentation.png`).
- A split view has one popup, in the pane with the keyboard, unchanged.
- The footer shows each `ServerState` (`a_server_that_is_absent_says_so_in_the_footer`).
- `a_frame_in_which_nothing_moved_recomputes_nothing` still passes, and a server reply for a
  stale revision does not reopen a closed popup.
- `editor complete --wait` returns the merged rows with the same order the popup shows;
  `editor complete --stem x --after "."` changes nothing in the document; `editor signature`
  answers.
- `tests/command_line.rs`: the payload cap of 50 and `total` hold with the new fields.

**Layer 4, the real application**, in the release checklist: open `unluminous` itself, type
`self.` in `app/completion.rs`, see the methods; open `ai-service/ui`, type `props.` in a component,
see the fields; `unluminous-cli status --section servers` on both.

**Budgets, asserted by `examples/completion_cost.rs`** (extended, run in release): the synchronous
path on `app/realm.rs` stays under 5 ms for a two letter stem and under 2 ms for `self.`; the
structural read per text revision is reported separately, as today. `examples/frame_cost.rs` is run
with the popup open to show the documentation panel adds no per frame allocation. The harness's
latency columns cover G4.

### 8.3 Verification rules that carry over

No visual judgement from dev screenshots: the kittest baselines are the suite, and the person
opens the PNGs before `UPDATE_SNAPSHOTS=1`. Every artefact of a run goes under
`_agent_output/task-<N>-completion/` or `D:/unluminous-completion-eval/runs`. The window is never
activated to drive it. No key is left held.

## 9. Alternatives considered

| Option | For | Against | Verdict |
|---|---|---|---|
| **A. Improve the syntactic tier only** (humps, locality, stats, signatures) | fits every recorded rule; no process | cannot answer `member` and `path` classes; G1 unreachable; the harness would show a fixed gap | the structural tier is kept and built, but it is not enough alone |
| **B. Our own resolver and type inference over Atrius** (IntelliJ's own route for Rust) | no external process; one index | a Rust type checker with traits, generics and macros is years of work; TypeScript's is `tsc`; nothing of it is data | rejected as a non goal |
| **C. Structural tier + language servers for Rust and TypeScript** (this design) | reaches G1; the structural tier keeps every other language and the no server case; DAP precedent for the process model; WebStorm does the same for TypeScript | reverses task-1675 for two languages; two protocols; a server to find and to watch | **chosen** |
| **D. `typescript-language-server` instead of tsserver directly** | one protocol for both | an extra global install; loses `sortText` groups, `isNewIdentifierLocation` and `data`; WebStorm talks to tsserver | rejected |
| **E. tree-sitter for structure** | real syntax trees; many grammars | code where plugins are data; a C dependency per grammar; the recorded rule | rejected; the scan plus regions give containers, fields and head lines without it |
| **F. `tower-lsp`/`async-lsp` client crates** | less code | both assume an async runtime; `tokio` is refused; the client side of LSP is a few hundred lines over `serde_json` | rejected; hand written, as DAP is |
| **G. A learned ranker now** | IntelliJ ships one | no training data until the harness exists; the deterministic chain is what IntelliJ ranks with before ML reorders the top five | deferred to section 7 |

## 10. Risks

| Risk | Answer |
|---|---|
| rust-analyzer on a large workspace takes a minute to index | the structural tier answers from the first keystroke; the footer shows progress; G4 is measured warm and the cold time is reported beside it |
| A server crashes repeatedly | three restarts an hour, then `Failed`, drawn in the footer; the structural tier remains |
| Two documents, one server: edits arrive out of order | one worker per server serialises them; revision numbers are checked on every reply |
| UTF-16 arithmetic for tsserver on lines with emoji | one conversion at the adapter boundary with a test on such a line; rust-analyzer is asked for UTF-8 so it has none |
| The merge shows a structural row and then the server replaces it with a different first row, which feels like a jump | the harness's `member, structural only` class measures how often the guess is wrong; a wrong guess is the follow up list's priority; `preselect` from the server only moves the highlight when the person has not moved it |
| Atrius schema change invalidates every index on disk | `SCHEMA_VERSION` 3 rebuilds, which takes 0.6 s on vite and 1.5 s on django by the recorded numbers; the trigram and passage tables are untouched |
| The window becoming the Atrius host changes who answers the MCP tool | the repository rule already allows the window to be the host, and `mcp/driver.rs` already runs in process; `search status` reports `answeredBy` |
| Keystroke cost grows with the richer rows | the 5 ms budget is asserted by `completion_cost.rs` in release; `by_prefix` replaces the name walk that was the recorded cost |

## 11. Environment facts for the implementer

- IntelliJ IDEA Ultimate 2025.3.4 (build 253.32098.37) is installed at
  `C:/Program Files/JetBrains/IntelliJ IDEA 2025.1` (the folder name lags the product), with the
  `intellij-rust`, `javascript-plugin`, `completionMlRanking` and `fullLine` plugins in
  `%APPDATA%/JetBrains/IntelliJIdea2025.3/plugins`. The IntelliJ engine of the harness runs on this
  machine.
- `rustc 1.95.0` with `rust-src` is installed; the `rust-analyzer` rustup component is not
  (`~/.cargo/bin/rust-analyzer.exe` is a proxy with nothing behind it). `rustup component add
  rust-analyzer` installs it; the adapter's "next to cargo" lookup then finds it.
- TypeScript 5.9.3 exists in `C:/jason/dev/service-manager/node_modules/typescript`, and
  `ai-service/ui` has its own. No global `typescript-language-server`, and none is needed.
- The current completion code and tests: `crates/unluminous-core/src/completion.rs` (922 lines),
  `crates/unluminous-app/src/app/completion.rs` (2,176 lines, about 1,000 of tests),
  `crates/unluminous-app/src/components/completion.rs` (218), `services/symbol_index.rs` (483, to be
  removed in favour of Atrius), `crates/unluminous-app/tests/navigation.rs` lines 878 to 1300, and
  `examples/completion_cost.rs`. Atrius: `crates/atrius-index/src/{outline,symbols,store,passages,
  verbs,host,protocol,freshness}.rs` at version 0.1.2.
- The recorded budgets: 5 ms per keystroke, 0.65 ms idle frame, nothing before the first frame.

## 12. Sources

- JetBrains SDK, code completion: https://plugins.jetbrains.com/docs/intellij/code-completion.html
- `CompletionContributor`, `CodeCompletionHandlerBase`, `CompletionUtilCore`, `CodeInsightSettings`,
  `LangExtensions.xml`, `MLSorter.kt` in https://github.com/JetBrains/intellij-community
- Code completion episodes 1 and 4: https://blog.jetbrains.com/blog/2021/05/28/code-completion-episode-1-scenarios-and-requirements/ ,
  https://blog.jetbrains.com/blog/2021/08/20/code-completion-episode-4-model-training/
- Full line code completion paper (metrics, RoCC, acceptance): https://arxiv.org/html/2405.08704v3
- Evaluation Plugin (`plugins/evaluation-plugin`, tag `idea/252.23892.409`), README and `core/src/com/intellij/cce/metric`
- TreeRanker, IntelliJ against VS Code on identical positions: https://arxiv.org/html/2508.02455
- How Rust IDEs understand code (RustRover's own engine): https://blog.jetbrains.com/rust/2026/05/29/how-rust-ides-understand-code/
- IntelliJ Rust weighers: https://github.com/intellij-rust/intellij-rust (`RsCompletionWeighers.kt`)
- rust-analyzer `ide-completion` (`item.rs`, `CompletionRelevance::score`), `lsp/to_proto.rs`, configuration: https://github.com/rust-lang/rust-analyzer
- TypeScript `src/server/protocol.ts`, `src/services/completions.ts` at v5.9.2 (sortText groups, preferences)
- WebStorm TypeScript support: https://www.jetbrains.com/help/webstorm/typescript-support.html
- LSP 3.17: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
- VS Code `suggest`, Zed `code_context_menus.rs`, Helix `completion.rs` and `nucleo`, for the client side patterns
- This repository: `tasks/task-1675-code-editing-tdd.md` §2, `task-1677-autocomplete-tdd.md`,
  `task-1680-import-completion-tdd.md`, `task-1702-saved-state-and-hypothetical-completion-tdd.md`,
  `task-2138-unluminous-code-index-tdd.md`, `task-2229-notebooks-rust-and-completion-tdd.md`,
  `tools/search-eval/SCORECARD.md`
