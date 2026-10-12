# task-2249: the code index is optional, and costs half as much when it is on

## 1. Introduction

The ticket:

> We use atrius-code index model to get really good auto complete options, however, it comes with a cost
> of using a lot more memory. We want that aspect to be optional, as some people don't need auto complete.
> If I turn off the feature, I expect my memory to immediately go down significantly. Before these
> changes, we were only using about 150MB of memory. When it is loaded/turn on, we need to find a way to
> reduce the amount of memory used significantly. Use hill climbing to get the amount needed reduced by
> 50%. It also seems like even small projects now eat up 1GB of memory. We should explore a policy of
> loading the needed data into memory when typing begins on a code file, and offloading it if not used
> for 5 minutes. How much time would that take? Have the policy options be in the settings, and clearly
> describe the cost. Loading data should have a status message at the bottom bar.

This document measures where the memory goes, hill climbs the largest part of it, times the loading and
unloading the ticket's policy needs, and designs the settings, the status bar message and the code changes
in Unluminous, Atrius and Inillucent. Every number here was measured on this machine on 2026-10-11, with
the installed Unluminous 0.76.1 or with the two benches in `_agent_output/task-2249-index-memory/` (in the main checkout). Nothing
in the product was changed by this ticket. Section 10 says what to build and in what order.

## 2. The finding that shapes the design

**The 1 GB is the embedding model, and completion never uses it.**

The window opens Atrius's index for the project on its second frame
(`UnluminousApp::keep_the_symbol_index_fresh`, called from `app/frame.rs`) and becomes the index host. What
that brings into the window process, and what reads each part:

| part | what reads it | memory |
|---|---|---|
| exact index: every file's bytes lz4 compressed, and the trigram posting lists | an agent's `search find`, `files`, `fragment` | about 100 MB of the large project's 300 MB (Atrius's own `memoryBytes`) |
| symbol table: every definition's name, container, signature and type, and four lookup maps | **completion**, go to definition in a closed file, auto import, signature help, `search def`, `refs`, `outline` | most of the rest of the 300 MB |
| store: the Inillucent index file, its page cache and the passage table | loading the index quickly, and plain English `search find "<question>"` | about 110 MB on the large project (section 3.3) |
| **embedding model**: `nomic-embed-text-v1.5`, 137M parameters, ONNX Runtime on the processor | plain English `search find` only, which ranks passages by meaning beside their words | **1,028 MB loaded, 1,470 to 1,920 MB while it embeds** |

`app/completion.rs`, `app/gather.rs`, `app/symbols.rs`, `app/auto_import.rs` and `app/signature.rs` reach
the index through `ProjectSymbols::read`, which reads the symbol table. None of them reaches the model. So
the ticket's "atrius code index model" is two costs with two owners:

1. **The model, about 1 GB.** It serves one feature, plain English search, which a person types rarely and
   an agent reaches through `unluminous_search`. Turning it off loses nothing in completion.
2. **The index without the model, 10 MB on a small project and 300 to 470 MB on a large one.** Completion's
   project names, go to definition in closed files, auto import, signature help and an agent's `search`
   commands are made from it.

A third cost is in the ticket's "my memory" when it is read off Task Manager, which groups a program's
child processes under it: **the language servers** from `task-2231` §5.3. rust-analyzer and tsserver start
the first time a Rust or TypeScript file is showing and are never stopped. Section 3.4 measures them.

**Why the model is loaded at all when nobody searches.** Atrius's store thread calls
`passages::can_embed`, which runs `SELECT embed('search_query: ready')` to learn whether a model is
installed. That loads the model the moment a project opens. Then every save that changes a passage sends
the chunk to `Embedder::step`, which loads it again. `search::MODEL_IDLE` (`task-2247`) drops it 60 seconds
after its last use, so an untouched window recovers, but **a person who saves a file at least once a minute
holds the model the whole time.**

## 3. Measurements

Window measurements start the installed `unluminous.exe <folder> --background` with a fresh `APPDATA` and
sample the process and its children once a second (`measure.mjs`). The raw rows are the `*.jsonl` files
beside it.

### 3.1 A project with one file in it

| state | working set |
|---|---|
| first 60 seconds after opening | **1,212 MB** |
| after `MODEL_IDLE` drops the model | 206 MB |
| the same, started with `ATRIUS_EMBED=off` | **179 MB** |
| a file saved once, 30 seconds after the drop | 1,215 MB again, for 60 more seconds |
| a file saved every 10 seconds | 1,217 MB for the whole run |

So the ticket's 150 MB is today's window with the model off: 179 MB, of which the index for one file is a
few megabytes. The other 27 MB between 206 and 179 is what ONNX Runtime leaves behind after a drop (its
library and allocator pools).

### 3.2 Why even small projects eat 1 GB

Three things add up on a small project, and only the first is in the window process:

| | working set |
|---|---|
| the window with the model loaded (open, or any save in the last minute) | 1,215 MB |
| rust-analyzer on a four line Cargo project, started when `main.rs` showed | 412 MB, plus 15 MB for its proc macro server and 8 MB for its console host |
| total Task Manager shows under Unluminous | **about 1,650 MB** |

rust-analyzer holds 412 MB on four lines because it loads the standard library's sources and their analysis
whatever the project's size. That is outside Unluminous's control except by not running it.

The same run found one oddity worth a separate look: in a folder with no `.gitignore`, rust-analyzer's
`target` folder was indexed (the file count went from 5 to 31), and the passage table never became ready
(`passagesReady` stayed false for three minutes). That is an Atrius question and is not part of this design.

### 3.3 A large project: this repository, 1,934 files, and ai-service, 6,797 files

| | this repository | ai-service |
|---|---|---|
| window, `ATRIUS_EMBED=off` | 526 MB | |
| window, model on, at open | 1,501 MB | |
| window, model on, peak while it embeds 9,839 passages | 2,021 MB | |
| window, model on, held until the model is dropped | 1,812 MB | |
| window, model on, after the drop | **801 MB** | |
| index alone, loaded from its file (`index-memory-bench disk`) | 294 to 326 MB, **0.36 to 0.39 s** | 421 to 478 MB, **0.68 to 1.05 s** |
| index alone, built from the files with no store (`memory`) | 188 MB, **0.23 s** | 316 MB, 0.79 s warm and 3.7 s cold |
| index alone, after it is dropped | 26 to 29 MB | 17 to 20 MB |
| index alone, first build of the store (passage table written) | | peaks at 1,656 MB, and the memory is not given back until the build ends |

Three things follow:

- **Dropping the index gives its memory back.** Each of three open and drop cycles came back to under 30 MB,
  so the ticket's unload is possible, once the window can drop it (section 6.4).
- **The store costs about 110 MB of the large project's 300** (294 against 188). It exists for the passage
  table and for a fast load. With the model off and no plain English search, completion would be served by
  the in memory index alone, which on this repository is faster to build than the store is to load.
- **After the model is dropped the window keeps 275 MB more than one that never loaded it** (801 against 526).
  The bench returns to 40 MB after a drop in every arm, so this is not ONNX Runtime's session. The likely
  holders are the vectors and the vector index Inillucent built in its page cache while 9,839 vectors were
  written, and allocator pages the heap did not return. Section 10 makes finding it a measured task.

### 3.4 The language servers

rust-analyzer reached 390 MB 30 seconds after the file showed and 412 MB at 45 seconds, and stayed there.
tsserver was not measured here. Both are
started by `keep_the_servers_running` and stopped only when the window closes. `editor.servers = off`
already exists and stops them being started.

## 4. The hill climb: the model at half the memory or less

### 4.1 The bench

`embed-memory-bench` (Rust, `ort` 2.0.0-rc.13 against the same `onnxruntime.dll` 1.22 Inillucent installed)
loads a model file with a given set of ONNX Runtime options, embeds 160 chunks of this repository cut the way
Atrius cuts passages (up to 2,000 characters, `make-chunks.cjs`), with the `search_document: ` prefix, mean
pooling and normalisation Inillucent uses, and prints the working set loaded, while embedding, at its peak and
after the session is dropped. One arm is one process. Quality is measured against today's vectors: the mean
and the lowest cosine between an arm's vector and today's for the same chunk, and **recall@10**, the share of
each chunk's ten nearest neighbours today that are still among its ten nearest in the arm. `arm.cjs` runs an
arm and `hillclimb.jsonl` and `round2.log`, `round3.log` hold every result.

### 4.2 Every arm

| arm | loaded MB | while embedding MB | peak MB | load ms | ms a chunk | mean cosine | recall@10 |
|---|---|---|---|---|---|---|---|
| **today**: fp32, ONNX Runtime defaults, 4 chunks a run | 1,028 | 1,470 | 1,470 to 1,920 | 709 | 371 | 1.000 | 1.000 |
| CPU memory arena off | 563 | 573 | 1,162 | 753 to 973 | 360 to 411 | 1.000 | 1.000 |
| memory pattern off | 1,029 | 1,314 | 1,314 | 740 | 375 | 1.000 | 1.000 |
| prepacking off | 563 | 1,531 | 1,531 | 821 | 387 | 1.000 | 1.000 |
| arena, pattern and prepacking off | 563 | 573 | 1,162 | 782 | 383 | 1.000 | 1.000 |
| arena on, arena shrinkage after every run | 1,027 | 1,430 | 1,862 | 706 | 358 | 1.000 | 1.000 |
| arena off, graph optimization basic | 563 | 573 | 1,137 | 726 | 416 | 1.000 | 1.000 |
| arena off, prepacking off, basic | 562 | 572 | 1,136 | 470 | 419 | 1.000 | 1.000 |
| arena off, sequences cut at 512 tokens | 563 | 572 | 870 | 714 | 285 | 0.995 | 0.874 |
| arena off, 2 chunks a run | 562 | 571 | 866 | 812 | 470 | 1.000 | 1.000 |
| **arena off, 1 chunk a run** | **563** | **569** | **716** | 778 | 413 | **1.000** | **1.000** |
| fp16 export, arena on | 324 | 1,858 | 1,858 | 783 | 501 | 1.000 | 0.999 |
| fp16 export, arena off | 308 | 320 | 1,229 | 830 | 568 | 1.000 | 0.999 |
| fp16 export, arena off, 2 a run | 308 | 316 | 929 | 438 | 770 | 1.000 | 0.999 |
| **fp16 export, arena off, 1 a run** | **308** | **315** | **778** | 421 to 438 | 856 to 870 | **1.000** | **0.999** |
| int8 export | 289 | 1,151 | 1,151 | 310 | 243 | 0.941 | 0.773 |
| int8 export, arena off | 174 | 185 | 787 | 608 | 287 | 0.941 | 0.773 |
| uint8 export, arena off | 178 | 185 | 787 | 403 | 298 | 0.949 | 0.803 |
| q4 export | 263 | 1,126 | 1,126 | 773 | 360 | 0.800 | 0.727 |
| bnb4 export | 202 | 1,063 | 1,063 | 617 | 376 | 0.841 | 0.709 |

Three load, embed and drop cycles of the winning fp32 arm came back to 39, 41 and 45 MB, so a policy that
loads and drops the model repeatedly does not leak. Four arms failed once with no output and passed on a
second run; the bench does not say why, and none of the numbers above comes from a failed run.

### 4.3 What the climb found

- **ONNX Runtime's CPU memory arena is most of it.** The arena keeps every buffer it has handed out for the
  life of the session, so about 465 MB of buffers used while the session is built stay in the process (1,028
  against 563 MB loaded), and so does the largest batch's attention memory (1,470 against 573 while embedding). Off, the loaded model is 563 MB, which is the 522 MB of weights and
  little else. The vectors are byte for byte the same, and embedding is between 3% faster and 11% slower
  across runs.
- **Prepacking duplicates the weights only while the arena is on.** With the arena off it changes nothing,
  which is the opposite of what ONNX Runtime's PR 22256 suggests for large language models.
- **The peak while embedding is the attention memory of a batch**, which grows with the square of the longest
  passage in it. One passage a run takes the peak from 1,162 to 716 MB with the arena off. Atrius already has
  the setting for it: `ATRIUS_EMBED_BATCH`, default 4, decides how many passages `Embedder::step` hands over at
  once. One a run costs 10 to 15% of embedding speed, and embedding is a background job that only runs after
  a save.
- **Arena shrinkage does not help**: it returns memory between runs and the peak is inside a run.
- **The int8, uint8, q4 and bnb4 exports cost too much quality.** recall@10 of 0.71 to 0.80 means a fifth to
  a quarter of the passages a search would rank near a question change. Every one of them also needs every
  vector in the shared cache embedded again.
- **The fp16 export keeps the quality** (recall@10 0.999, mean cosine 1.000 to four places) at 308 MB loaded,
  but embeds 2.1 times slower on the processor, because ONNX Runtime's CPU kernels convert to fp32 as they go.

### 4.4 The result against the ticket's 50%

| | today | arena off, 1 a run (fp32) | reduction | fp16, arena off, 1 a run | reduction |
|---|---|---|---|---|---|
| model loaded and idle | 1,028 MB | 563 MB | 45% | 308 MB | 70% |
| model while embedding | 1,470 MB | 569 MB | 61% | 315 MB | 79% |
| model peak | 1,470 to 1,920 MB | 716 MB | 51 to 63% | 778 MB | 47 to 59% |
| vectors | | identical | | recall@10 0.999 | |
| embedding speed | 371 ms a chunk | 413 ms | 11% slower | 863 ms | 133% slower |

**Recommendation: arena off and one passage a run, on the fp32 model.** It meets 50% while embedding and at
the peak, comes to 45% when idle, changes no vector, and needs no new file. The idle figure matters least,
because under the policies in section 6 the model is not held while idle. fp16 is the next step if 563 MB is
still too much, and section 8 says why it is not the first.

What that is in the window, worked out from the measurements rather than measured (the change is in
Inillucent and was not built for this ticket): one file open, model loaded, about 179 + 563 = **740 MB**
against 1,212 today, peaking near 900 MB while it embeds instead of 1,500.

## 5. Loading when typing begins, unloading after five minutes

### 5.1 How long each load takes

| what is loaded | small project | this repository | ai-service | measured with |
|---|---|---|---|---|
| index from its file | 3 to 11 ms | 352 to 386 ms | 670 to 1,050 ms | `index-memory-bench disk` |
| index built from the files, no store | 10 ms | 222 to 256 ms | 783 ms warm, 3,701 ms cold | `index-memory-bench memory` |
| embedding model, fp32 | 470 to 970 ms | same | same | `embed-memory-bench` |
| embedding model, fp16 | 421 to 438 ms | same | same | `embed-memory-bench` |
| rust-analyzer, to its full memory | about 30 s | longer, it analyses the whole workspace | | window sampler |

So the answer to "how much time would that take" is: **under half a second for the index on a project the
size of this one, about a second on ai-service, and under a second for the model.** The index load runs on
Atrius's own threads, so no frame waits; the first keystroke after an unload is answered from the open tabs
and the language's keywords, and the project's names join the list within that half second.

A language server is the expensive one to bring back: rust-analyzer needs tens of seconds before its answers
are good. That is why section 6 gives the servers a longer default idle time than the index.

### 5.2 What unloading gives back

| unloaded | small project | this repository | ai-service |
|---|---|---|---|
| index (exact, symbols and store) | about 10 MB | about 280 MB | about 450 MB |
| model | about 1,000 MB today, 520 MB after section 4 | same | same |
| rust-analyzer | 435 MB | more | |

### 5.3 What has to change before anything can be unloaded

The bench drops an `Index` and gets the memory back. The window cannot do that today, for three reasons in
Atrius's host:

1. `atrius_index::host::in_process` keeps every `Host` it starts in a static map (`hosts()`) for the life of
   the process. `ProjectSymbols` dropping its `Arc<Host>` drops nothing.
2. `Host::listen` runs on a thread that holds an `Arc<Host>` and blocks in `accept` for ever.
3. The store thread holds the exact index and the symbol table until its job queue closes, and a passage
   rebuild in progress runs to its end first (the 1.6 GB ai-service build above kept its memory until it
   finished).

Section 6.4 adds the call that fixes all three.

## 6. Design

### 6.1 Three settings, one card, on `Settings -> Editor`

A card named **Code index and memory** beside **Suggestions**, with each row saying what it costs in the
numbers above. Each is a setting in `settings.rs`'s `coded` block, so `settings get/set` and the command line
reach it with no extra code.

| setting | values | default | what the row says |
|---|---|---|---|
| `index.semantic` | `off`, `when-used`, `always` | **`off`** | "Plain English code search. Ranks the results of a question such as *where are breakpoints sent to the debugger* by meaning as well as by its words. Uses an embedding model: about 560 MB while it is loaded, and about a second to load. `When used` loads it for a question or to embed the passages of a saved file, and lets it go a minute later. Off, questions are ranked by their words alone, and completion is not affected." |
| `index.mode` | `always`, `while-typing`, `off` | **`while-typing`** | "The project's definitions, which completion, Go to Definition, auto import and signature help read. About 10 MB on a small project and 300 to 450 MB on a large one. `While typing` loads them when you type in a code file and lets them go after the idle time below; loading takes under a second, and completion offers the open files' names until it finishes. Off, completion offers only the open files' names and the language's keywords." |
| `index.idle_minutes` | 1 to 60 | **5** | "Minutes without typing in a code file before the definitions, and the language servers, are let go." |

`editor.servers` stays as it is and gains the idle behaviour: with `index.mode = while-typing`, a language
server with no keystroke in its language for `index.idle_minutes` is shut down (LSP `shutdown` then `exit`) and
started again on the next keystroke in that language. The row under it says: "rust-analyzer holds about
400 MB even on a small project and needs about 30 seconds after it starts before its answers are complete."

Why these defaults:

- **`index.semantic = off`** is the one setting that takes a fresh window from 1,212 MB to 179 MB, which is
  the ticket's first expectation. An agent's plain English `search find` still answers, from the words; the
  search evaluation in `tools/search-eval/` measured that the agent's own follow up greps do most of the
  finding either way (`tools/search-eval/SCORECARD.md`).
- **`index.mode = while-typing`** with 5 minutes is the ticket's own proposal, and section 5 shows its cost is
  under a second.

### 6.2 What happens when a setting changes

"If I turn off the feature, I expect my memory to immediately go down." So every change acts on the frame it
is made:

- `index.semantic` to `off`: Unluminous tells Atrius to stop embedding and drop the model now (6.4). The
  working set falls by the model's size within the frame; the status bar says so (6.3).
- `index.mode` to `off`: the window calls `ProjectSymbols::close`, which releases the host (6.4). The working
  set falls by the index's size.
- Turning either on loads it in the background, with the status message.

### 6.3 The status bar message

The status bar already has one slot for a passing message (`Status::message`, which stays until something
replaces it). Loading is something that ends on its own, so it gets its own field, **`Status::activity`**, drawn
before the message in the dim text colour, which a load sets when it starts and clears when it ends:

- `Loading the code index...` while the index loads, then `Code index ready (0.4 s, 312 MB)` for three seconds.
- `Loading the search model...`, then `Search model ready (0.8 s)`.
- `Code index let go after 5 minutes idle (freed 290 MB)`, so the person can see the policy working.
- `Starting rust-analyzer...` already exists in the footer from `task-2231` §6.9 and is left as it is.

The numbers come from `Index::status` (`load_ms`, `memoryBytes`) and from the working set read once before and
once after, through the same `GetProcessMemoryInfo` call the benches use. `status --section index` reports the
same fields as data, so an agent and a test can read them.

### 6.4 The code

**Unluminous** (`crates/unluminous-app`):

- `services::project_symbols::ProjectSymbols` gains `close(self)`, which calls `atrius_index::host::release`.
- `app/symbols.rs`: `keep_the_symbol_index_fresh` stops opening the index unconditionally. It opens it when
  `index.mode = always`, or when `while-typing` and a keystroke lands in a file a language plugin claims
  (the check `completion` already makes), and records `last_typed_at`. Once a second (off the existing
  heartbeat) it closes the index when `while-typing` and `last_typed_at` is older than `index.idle_minutes`.
  A `search` command from an agent counts as use and opens it too, so an agent is never refused.
- With `index.mode = while-typing`, the window opens the index **in memory** (`Index::open_in_memory`) when
  `index.semantic = off`, because that is 0.23 s and 188 MB on this repository against 0.37 s and 300 MB from
  the store, and it writes nothing. It opens it as the host, with the store, when `index.semantic` is on,
  because only the store has a passage table.
- `app/servers.rs`: each `Running` server records its last keystroke; the same once a second check shuts one
  down after the idle time and removes it from `servers.running`, so the next keystroke starts it.
- `unluminous-cli/src/search.rs` `configure()`: sets `ATRIUS_EMBED_BATCH=1` unless it is already set
  (section 4.3), and `ATRIUS_EMBED=off` when the window's `index.semantic` is `off`.
- `components/status_bar.rs`: `Status::activity`.
- `settings.rs`, `components/settings_dialog.rs`, `app/cli/cli_settings.rs`: the three settings and the card.

**Atrius** (`C:/jason/dev/atrius-index`, released as 0.2.2 and taken here by raising the version):

- `host::release(root)`: removes the root from `hosts()`, sets `stopping`, wakes the listener by connecting to
  its own port so `listen` returns, removes `host.conf`, and drops the lock. Dropping the last `Arc<Index>`
  then closes the store's job channel.
- The store thread checks a `stop` flag between passage batches, so a release during a long passage rebuild
  ends within a batch rather than at the end of the build.
- `Index::set_embedding(on: bool)`: off sends the store thread a job that drops the `Embedder` and calls
  Inillucent's `unload` on the managed embedder, so the model goes now rather than at the end of its idle timer.
- **`Embedder::open` stops running the model to find out whether there is one.** `passages::can_embed` runs
  `embed()`, which loads 1 GB to answer a yes or no. Asking whether the model's files are installed answers the
  same question, and the first real embedding finds out whether it runs.

**Inillucent** (`crates/inillucent-core/src/embed_onnx.rs`, `open_session`):

- On the processor, register the CPU execution provider with `with_arena_allocator(false)`. This is the
  change that halves the model. It changes no vector (recall@10 1.000 in every arm), and it is on the CPU
  device only, so the CUDA path keeps its own arena with the `SameAsRequested` strategy it already has.
- `OnnxOptions` gains `cpu_arena: bool` defaulting to false, and `INILLUCENT_EMBED_ARENA=on` turns it back on,
  for a bulk embedding run that wants every bit of speed.

### 6.5 Agents

Everything a person can do here an agent can do through the same code:

- `settings set index.semantic off` and the other two are ordinary settings commands.
- `status --section index` answers `mode`, `loaded`, `loadMs`, `memoryMB`, `semantic`, `modelLoaded`,
  `idleMinutes`, `lastUsedSecondsAgo` and `servers` (each with `running` and `lastUsedSecondsAgo`).
- `search load` and `search unload` load and let go of the index by hand, the way a person would by switching
  the setting, and are rows in `catalogue.rs`, so the MCP tools gain them with no extra code.

## 7. Testing

- **The memory, measured on the installed build**, with `measure.mjs` and the two benches, before release, for:
  a fresh window on a one file project with each `index.semantic` value; the same after switching `index.semantic`
  from `when-used` to `off` with the model loaded (the working set must drop by more than 400 MB within two
  seconds); `index.mode = while-typing` with `index.idle_minutes = 1` on this repository (the index must load
  within a second of a keystroke, and the working set must fall back within 70 seconds of the last one).
- **Unit tests in Atrius**: `release` leaves no thread holding the index (a `Weak` to it fails to upgrade after
  release), a release during a passage rebuild returns within one batch, and `Embedder::open` with a model
  installed does not load it (`Residency` load counter stays at zero).
- **A unit test in Inillucent**: a CPU session reports the arena off, and the vectors of the five fixed texts
  in `embed_onnx` tests are unchanged.
- **Window tests in `crates/unluminous-app/tests/`**: the settings card has the three rows with their names;
  `status --section index` reports `loaded: false` after `search unload` and `true` after a keystroke in a
  `.rs` file; the status bar shows `Loading the code index...` while a load is in progress (a test can hold one
  with an in memory index of a folder).
- `tools/search-eval/` run once with `index.semantic = off` and once with it on, so the cost of the default is a
  measured number in `SCORECARD.md` rather than an estimate.

## 8. Alternatives considered

- **The int8 and 4 bit exports.** Smallest memory (174 to 263 MB), but recall@10 of 0.71 to 0.80, and every
  cached vector would be embedded again. Rejected on quality.
- **The fp16 export first.** 308 MB with no quality loss, but 2.1 times slower to embed on the processor, a
  new 274 MB file for `inillucent setup-embeddings` to fetch and verify, and a model name to change in
  Atrius's cache. Kept as the next step if the arena change is not enough.
- **A smaller model.** `potion-code-16M` (a static 32 MB model) scores 39 on CoIR against 59 for a model of
  nomic's size; all-MiniLM and bge-small have no published code retrieval numbers. Every vector would be
  embedded again and the passage table's dimensions would change. Not worth it while the arena change gives
  half the memory for nothing.
- **Arena shrinkage, the memory pattern and prepacking.** Measured in section 4.2; none helps once the arena is
  off.
- **Keeping the index and dropping only the model.** That is what `MODEL_IDLE` does today. It does not meet the
  ticket's "even small projects", because any save brings the model back, and it leaves the 300 to 450 MB index
  of a large project in memory all day.
- **A separate host process for the index**, so the window's own number stays small. Task Manager would still
  add it to Unluminous, and every completion would cross a socket. Rejected.

## 9. Sources

- ONNX Runtime session option keys: `include/onnxruntime/core/session/onnxruntime_session_options_config_keys.h`
  in microsoft/onnxruntime, and the run option `memory.enable_memory_arena_shrinkage`.
- ONNX Runtime PR 22256 (prepacked weights doubling heap memory on Phi 3 mini) and issue 11627 (arena growth).
- `nomic-ai/nomic-embed-text-v1.5` on Hugging Face: the `onnx/` folder's exports and the Matryoshka table.
- `nomic-ai/CodeRankEmbed` and `minishlab/potion-code-16M-v2` model cards, for the CoIR numbers in section 8.
- Inillucent: `crates/inillucent-core/src/residency.rs`, `embed_onnx.rs`; `crates/inillucent-search/src/embed.rs`.
- Atrius 0.2.1: `crates/atrius-index/src/index.rs` (store thread, `Embedder`), `host.rs`, `passages.rs`.
- Unluminous: `services/project_symbols.rs`, `app/symbols.rs`, `app/frame.rs`, `app/servers.rs`,
  `unluminous-cli/src/search.rs`, `components/status_bar.rs`, `components/settings_dialog.rs`.
- Measurements and the two benches: `_agent_output/task-2249-index-memory/` (in the main checkout).

## 10. Order of work

1. **Inillucent: CPU arena off** (6.4). One builder call and a test. Releases with Inillucent; Atrius and
   Unluminous take it by raising the version. Halves the model wherever it is used.
2. **Unluminous: `index.semantic`, default off**, through `ATRIUS_EMBED=off`, plus `ATRIUS_EMBED_BATCH=1`. With
   step 1 this alone takes a fresh window from 1,212 MB to 179 MB, and a window with the setting on to about
   740 MB with a peak near 900.
3. **Atrius: `host::release`, the stop flag, `set_embedding`, and no model run in `Embedder::open`** (6.4).
4. **Unluminous: `index.mode`, `index.idle_minutes`, the language server idle shutdown, `Status::activity`, and
   `status --section index`.**
5. **Find the 275 MB the large project keeps after the model is dropped** (3.3), with the window's frame trace
   and a heap snapshot, before deciding whether it needs a change.
