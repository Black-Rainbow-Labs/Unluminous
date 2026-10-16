# task-2279 — what an open project costs, and the switches that decide it

*"At rest with the Unluminous project open, Unluminous is using 18% CPU, and 1.8 GB of memory is used.
After a computer restart its near 0% and 512MB. Then I quit and re-open and it's using 1.5GB of
memory."* The ticket also asks that loading the model be off by default, that each switch say what it
costs in memory, that turning a switch off take effect at once, and that opening, closing, resizing and
showing and hiding panels be fast.

## 1. The instrument

`tools/perf-session.mjs` drives a release window through a whole session on a copy of this repository
(2,023 files, the project the report is about). It starts from a fresh settings folder and a fresh index
cache, so nothing a person chose decides the result, and it **sums every counter over the window's
process tree**. rust-analyzer, the cargo and rustc processes it starts, and the shells are children of
the window, and they are what Task Manager groups under Unluminous.

| Case | What is done |
|---|---|
| `cold-open` | start the window, open `crates/unluminous-app/src/app/mod.rs`, watch the tree for 90 s |
| `cold-idle` | the next 30 s, with nothing done |
| `toggle-explorer`, `toggle-terminal` | each panel shown and put away 20 times |
| `drag-explorer`, `drag-terminal` | the divider dragged 200 points and back, 30 steps each way |
| `resize-window` | the window's size set 12 times |
| `tab-switch`, `type`, `scroll` | 20 switches, a 46 character line, 30 wheel notches |
| `cold-close` | from `quit` until the window has gone |
| `warm-open`, `warm-idle`, `warm-close` | the same project opened again with what it remembered |

Builds are measured interleaved (base, candidate, base, candidate), three rounds each, on a quiet
machine. The flow directory is `_agent_output/task-2279-perf/`.

## 2. What it found on 0.88.0

90 s after opening a Rust file:

| | Working set | Private | CPU in 90 s |
|---|---:|---:|---:|
| The whole tree (47 processes) | 8,871 MB | 10,421 MB | 100.5 s |
| rust-analyzer | 3,146 to 3,881 MB | 3,200 to 3,952 MB | 30.2 s |
| 19 rustc processes building proc macros for rust-analyzer | 3,733 MB | 4,845 MB | 25.2 s |
| unluminous.exe | 1,572 MB | 2,159 MB | 41.0 s |

At rest the window used **2.0 to 2.2 processors**, and closing it took **2.7 s**.

### 2.1 rust-analyzer starts by default and keeps running after it is switched off

`editor.servers` defaults to `automatic`, so the first Rust file shown starts rust-analyzer. On this
project it holds 3.9 GB and, while it reads the workspace, starts cargo and a rustc per proc macro
crate. `UnluminousApp::server_for` answers `None` when the setting is `off`, which stops a **new** server
starting and does nothing to the one already running. That is the second question in the ticket: with
both switches off, rust-analyzer was still running.

### 2.2 The embedding model is loaded at every open, even when there is nothing to embed

The window hosts the code index (Atrius), and the index embeds every passage of the project with
`nomic-embed-text` through ONNX Runtime. While the model is loaded it is about 1.1 GB of the window.
Three things make that the steady state rather than an exception:

- **The model is probed by running it.** `Embedder::open` calls `embed('search_query: ready')` to learn
  whether a model is installed, so the model is loaded at every open, even when every vector is already
  made and kept in the shared cache. That is the 1.5 GB after a reopen.
- **A cold project takes many minutes to embed.** 10,497 passages at about ten a second on two threads
  is the two processors at rest: the 18% of a 24 thread machine, with rust-analyzer's own reading.
- **Nothing in Settings mentions it.** The only way to stop it was `ATRIUS_EMBED=off` in the
  environment.

What the vectors buy is plain English questions in `search find`, which the agent tools use. Completion,
go to definition, references and `find` with text or a regex do not use them.

### 2.3 Closing waits for rust-analyzer

`Servers::stop_everything` runs `close_down` for each server on the window's thread and waits up to
`GRACE` (2 s) for the program to leave. rust-analyzer in the middle of reading a workspace does not
answer `shutdown` within that, so the window waits the whole two seconds and then kills it. The process
is in a job object that kills everything in it when the window's process ends, so the wait buys nothing.

### 2.4 Frames

| Case | Median frame | Largest phases |
|---|---:|---|
| `toggle-explorer` | 9.2 ms | egui 10.1 ms, `editor-layout` 4.3 ms, `remember` 3.7 ms |
| `drag-explorer` | 4.4 ms | egui 5.0 ms, `editor-layout` 3.6 ms |
| `type` | 4.4 ms | egui 5.1 ms, `input-completion` 2.5 ms |
| `resize-window` | 1.0 ms | egui 3.5 ms |

- **Every change of the editing area's width lays the whole file out again.** `app/mod.rs` is about
  6,000 lines, and showing the explorer, dragging its divider or resizing the window changes the width.
  Code does not wrap until a line is longer than the pane is wide, and in most source files no line is.
- **Showing or hiding a panel writes the project's state to disk on the window's thread**: three files
  flushed, 3.7 ms, every time.

These figures were taken while the window was also embedding on two threads, so they are measured again
with embedding off before anything is decided about them.

## 3. Research

| Source | Finding | Here |
|---|---|---|
| [Rust Analyzer using 13 GiB](https://users.rust-lang.org/t/rust-analyzer-using-13-gib/133914), [RAM consumption](https://users.rust-lang.org/t/rust-analyzer-ram-consumption/50420) | Several GB is normal for a workspace with hundreds of dependencies, and maintainers call 13 GB typical for 830 | rust-analyzer's memory is its own and cannot be tuned down from outside without losing answers, so the switch is what decides it |
| [What is rust-analyzer doing on startup?](https://users.rust-lang.org/t/what-is-rust-analyzer-doing-on-startup/125706) | `cachePriming` reads the whole workspace at start; turning it off moves the cost later rather than removing it | not adopted: it only delays the same memory |
| [Zed, TypeScript language servers](https://zed.dev/docs/languages/typescript) | Zed lets a person exclude a server per language, and caps vtsls at 8 GB | the same shape as `editor.servers`, with the cost stated |
| [egui discussion 4062](https://github.com/emilk/egui/discussions/4062), [objdiff repaint rework](https://git.axiodl.com/encounter/objdiff/commit/74e89130a8a4719e63d4e767bf5f098d183019c8) | repaint only when state changes; a periodic repaint timer costs CPU at rest | Unluminous already repaints on demand plus a half second heartbeat (`task-1805`): 17 ms of CPU a second at rest with nothing embedding |
| [egui issue 1196](https://goodfirstissue.org/emilk/egui/issues/1196) | egui's own layout and tessellation have room, inside egui | out of scope: the levers here are fewer and cheaper shapes |
| `task-1805`, `task-1813`, `task-2218` | idle frame, layout memory, interaction costs | already done; this ticket does not repeat them |

## 4. The changes

### 4.1 Language servers are off by default and stop the moment they are switched off

- `editor.servers` defaults to `off`. A settings file that names `automatic` keeps it.
- `keep_the_servers_running` stops every running server when the setting is `off`, so turning the
  switch off frees the memory on the next frame.
- A server is stopped **without the window waiting for it**: the worker thread asks it to leave, waits
  its grace, and kills it, while the window carries on. At exit the job object ends it with the window.
- The switch says what it costs: *"rust-analyzer typically holds 1 to 4 GB for a project this size, and
  starts cargo and rustc to build the project's macros when it opens; tsserver holds a few hundred MB.
  Off stops a running server at once."*

### 4.2 The embedding model is off by default, never probed while off, and dropped when switched off

A new setting, `search.embeddings`, `off` by default, under `Settings -> Editor -> Code index`.

- **Off at start**: `ATRIUS_EMBED=off` is set before the index host starts, so `Embedder::open` returns
  before its probe and the model is never loaded. Plain English questions are answered from the
  passages' words.
- **Switched off while running**: the index is told to stop embedding at once and the model is released.
  Atrius reads `ATRIUS_EMBED` once when its store thread starts, so this needs a switch in Atrius that is
  read on every pass of the store loop (`atrius_index::set_embedding`) and a way to release the model.
- **Switched on**: the index starts embedding where it left off, with vectors already in the shared
  cache reused.
- The switch says what it costs: about 1.1 GB while the model is loaded, two processors for several
  minutes the first time a project is opened, and what it buys.

### 4.3 Closing does not wait

The language servers are stopped without waiting (4.1). Each step of `on_exit` writes a frame trace mark,
so the rest of the close time can be read off one run.

### 4.4 Frames, measured again with nothing in the background

- **A width change that cannot change any line's wrapping keeps the layout.** When the widest line of a
  left aligned file fits both the old and the new width, the layout is the same, so it is kept.
- **Writing the project's state goes to a thread.** The state is worked out on the window's thread and
  written on another, in order, so a press costs the comparison and not the flush.

Each is kept only if the benchmark shows it, and the frame phases say where the next cost is.

## 5. What is deliberately left alone

- rust-analyzer's own settings (`cachePriming`, `lru.capacity`, `numThreads`): they trade answers for
  memory inside a program Unluminous does not own, and a person who wants rust-analyzer wants its
  answers.
- The graphics driver's share of the window, which `task-1805` measured at 55% of an idle window.

## 6. The window's own memory

Once rust-analyzer and the embedding model were off, the window process itself was what was left. Jason
asked for it to come back to about 150 MB in Task Manager's Memory column, which is the private working set.
At the start of this part a window on this repository with `app/mod.rs` open held 259 MB there.

### 6.1 The instrument

`tools/perf-memory.ps1` opens a release window with `--background` on a fresh settings folder, once to build
the index and once more to measure, so the index is loaded rather than built. It records the working set,
the private bytes and the private working set, and `tools/memory-map.ps1` splits the memory by kind (private,
image, mapped) and by module. A build made with `--features diagnostic-allocations` also writes the Rust
heap's live bytes into the frame trace, and with `UNLUMINOUS_HEAP_PROFILE=<file>` it writes the live heap by
call site at exit. Builds are measured interleaved, two rounds each.

Two probes outside the window answered questions the window could not:
`_agent_output/task-2279-perf/index-probe` opens the code index alone and reports its heap by call site, and
`_agent_output/task-2279-perf/gpu-probe` creates a DX12 device alone and uploads textures.

### 6.2 What the memory was

| Part | What it was | What was done |
|---|---|---|
| The code index | 140 MB private in the probe: every file's contents held, a 32 MiB page cache, the search index a plain English question builds (115 MB) held until the next file change, and a `String` for every place a path was named | Atrius 0.3.0: contents read from disk, 128 pages at rest, the question's index let go of after 20 s, paths shared. 51 MB in the probe |
| The graphics device | wgpu's default of a million bindings is a 30.6 MB descriptor heap | 16,384 bindings (`graphics_memory::BINDINGS`) |
| The process heap | Windows' default heap keeps freed memory | the segment heap, asked for in the manifest: 13 to 21 MB less private |
| The background picture | one 8 MB upload, and the driver keeps an upload's staging memory | uploaded as 1 MB strips, one a frame: 16 MB less in the project window |
| The glyph atlas | 4 MB of white pixels | one byte a pixel: 3 MB less |
| Fonts | each face's file copied into the heap, 3.4 MB | the files are memory mapped |
| eframe's default icon | 1 MB decoded and kept | an empty icon, which eframe reads as none |

The gpu probe is where the upload rule came from. Uploading a 16 MB texture left 17 MB of private memory
behind. Sent as 16 strips with a submission each, it left 1.5 MB. Uploads of 2 MB or less left nothing, and
strips sent in one submission were kept together.

### 6.3 Result

| Window | Start of this part | Now |
|---|---:|---:|
| This repository, `app/mod.rs` open | 259 MB | 148.5 MB |
| An empty folder | | 86.5 MB |

Measured in the diagnostic build. The release build is measured again in 6.5.

### 6.4 What is left

About 42 MB is the NVIDIA device itself: 21 MB of it is created when the adapter is chosen and then reused
by the device, so there is nothing to give back. In the heap, the index is about 23 MB on this repository
(the trigram posting lists and the symbol table), a restored terminal's scrollback is about 5 MB, and the
fonts egui parses are about 3 MB. Each is data the window uses.
