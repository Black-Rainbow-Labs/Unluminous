# task-2218 — what using Unluminous costs, case by case

> *"Unluminous needs performance optimizations to increase speed, reduce cpu, reduce memory. We need to be
> extremely fast and extremely memory efficient. Use the hillclimb skill … to measure interactions, zoom,
> etc and iterate until drastic performance gains are made. Research online on performance optimizations
> that could be useful, write a tdd, and fully implement."*

`task-1666` measured one frame of dragging a selection, `task-1805` an idle window and `task-1813` what a
window holds in memory. This ticket measures what **using** the window costs: typing, scrolling, zooming,
switching tabs and moving a canvas, each one driven through the real release build. Every number here was
measured on the machine it was written on, before and after, by a script that is kept in the repository.

## 1. The instrument

Every earlier cost tool in this repository measures one component with no window behind it, and every one
of them reports that its own part is cheap. So this ticket started by building a harness that drives a
window the way a person does.

`tools/perf-bench.mjs` starts a release build with `--background` on a fresh copy of a fixed corpus,
with its own `APPDATA` and its own git repository. It drives the window through `unluminous-cli input`,
which feeds the window the same egui events a keyboard and mouse produce, and for each case it records:

- the **process's own processor time**, read by one long lived PowerShell from `Get-Process`. The cost of
  the `unluminous-cli` processes sending the commands is therefore not in it;
- the working set, private bytes, handles and threads after the case;
- every frame the frame trace wrote during the case, with its phases.

The cases, each repeated enough times to measure:

| Case | What is done |
|---|---|
| `idle` | nothing for eight seconds |
| `open-<file>` | `tab open` a file |
| `type-<file>` | click in the text and type a 46 character line |
| `scroll-<file>` | the wheel, fifteen notches of five down and fifteen back up |
| `zoom-<file>` | `Ctrl`+wheel, six notches in and six out |
| `tab-switch` | twenty switches between two open files |
| `canvas-zoom` | `Ctrl`+wheel over a Realm canvas holding four nodes, eight in and eight out |
| `canvas-pan` | two drags across the empty canvas, thirty steps each |
| `canvas-idle` | the canvas left alone for six seconds |
| `startup`, `memory-end` | time until the control channel answers, and what the session ends holding |

The files are copies of four files from this repository, frozen in the corpus so that every build lays out
the same bytes: a 206 KB Rust file (`train.rs`), a 123 KB one (`test.rs`), and two Markdown files.

**Train and test.** The hill climb guide this follows asks for a split, so that the cases whose profiles
are read while choosing changes are not the cases whose numbers are reported. Here the cases are workloads
rather than prompts, so the split is by file and by canvas: the `-train` cases were the ones profiled, and
the `-test` cases use a different file or a different canvas and were never opened while choosing.

**Frame phases.** The frame trace's `rest` phase turned out to hold the largest cost in the window, so it
was split into `status`, `modals`, `notices`, `action`, `remember` and `marks`. The editing area gained
`editor-readings`, `editor-layout`, `input-edit`, `input-layout`, `input-completion`, `editor-scroll`,
`editor-gutter` and `editor-paint`; the canvas gained `realm-files`, `realm-ground`, `realm-nodes`,
`realm-wires` and `realm-chrome`. And `egui-cpu` records what eframe says the previous frame cost on the
window's thread, egui's own layout and tessellation included, so the trace can say how much of a frame's
processor time is outside `UnluminousApp::ui`. Each of these is one relaxed atomic load when the trace is
off.

## 2. What it found

### 2.1 The project state was written to disk on every frame where the caret or the scroll moved

Typing a line into the 206 KB file, a frame took 12 to 14 ms, and **13 ms of it was `remember`**:
`UnluminousApp::remember_the_project`. Scrolling, 17 of 18 ms. What a project remembers includes each tab's
scroll and caret (`task-1693`), and it was written the frame it changed — three files, each written
through a temporary that is flushed to the disk and renamed, and each read again by the virus scanner.
The caret moves on every key and the scroll on every frame of a wheel.

`task-2009` had already found the same fault in the window's position and made that one field wait until
the window had stood still for `WINDOW_SETTLE`. The fix widens the same rule:
`ProjectState::differs_only_in_what_a_gesture_moves` is true when nothing changed but the window, the
scrolls, the carets and which tab and pane are showing, and those wait until they have stood still for a
third of a second. A tab opened, a folder opened out or a pane split is still written the frame it happens,
and `on_exit` writes whatever is outstanding.

Two smaller writes had the same shape. Every notch of `Ctrl`+wheel changes `appearance.font.size`, and the
settings file was written after each one, because a wheel leaves the pointer up. It now waits until no zoom
step has been taken for `WINDOW_SETTLE`. And panning or zooming the canvas wrote the realm's sidecar on every
frame, because the camera is in it: 5.8 ms of an 11 ms frame. It is written once the pointer is up and the
zoom has finished gliding.

And a file that already holds its text is no longer written again, so switching tabs writes one file where
it wrote four.

### 2.2 Every tab switch threw the tab's layout away

`UnluminousApp::show_tab` ended in `forget_layout()`, and so did `Ctrl`+`Tab` and `unluminous-cli tab next`.
The comment above it explained why: the layout was one cache on the window, each document counts its own
revisions from one, and two tabs at the same revision would otherwise share a layout. That was true until
`task-1664` moved the layout, the preview and the colouring onto each tab, keyed on the tab's own revisions,
and the call stayed. So every switch coloured the file again and laid all of it out again: **15 to 28 ms a
switch on the 206 KB file**, and the Markdown preview went back to its top. A switch now costs nothing.

`showing_a_tab_that_was_already_laid_out_does_not_lay_it_out_again` had been passing all along, because it
calls `files.show` directly rather than the path a click takes.

### 2.3 Completion scanned the file a second time on every letter

While the completion popup is open, each letter typed asks `tab_symbols` for this tab's definitions and
words at the new revision, and that ran `syntax::scan` over the whole file: 2.0 ms on the 206 KB file. The
colouring had already scanned it, incrementally, and keeps every token including the plain words in
`IncrementalTokens`. `FileSymbols::read_tokens` reads the symbols off that list, which is 0.2 ms, and
`OpenFile::syntax_tokens_revision` says when the list describes the text as it is. `distinct_words` dropped
repeats before sorting rather than after, which took it from 0.86 ms to 0.57.

### 2.4 The canvas decoration was rasterised again on every frame the camera moved

`realm-chrome` was 5 ms of every frame of a pan or a zoom: `vello_cpu` drawing the ground and every node's
shadows again because the camera had moved them. The decoration is Gaussians and gradients.
`vello_canvas::DRAFT_SCALE` rasterises it at half the resolution in each direction while the camera is
moving, which is a quarter of the pixels, and the frame the camera stops on is rasterised at the full
resolution again, because the scale is part of what a canvas compares to decide whether to rasterise.

### 2.5 The graphics device set memory aside in 64 MB blocks

`task-1805` measured the graphics driver at 55% of the window's memory and concluded nothing inside
Unluminous could get it back. One thing can, and it is a setting. wgpu suballocates out of large blocks and
eframe asks for `MemoryHints::Performance`, which on DX12 takes device memory in blocks of at least 128 MB
and host memory, which is system RAM and counts against the process, in blocks of at least 64 MB.
`services::graphics_memory::frugal` asks for `MemoryHints::MemoryUsage`, 8 MB and 4 MB blocks. Everything
else eframe asks for is kept, because the descriptor eframe would have built is built first.

### 2.6 Layout

Laying the 206 KB file out took 19.9 ms, which is what every zoom step and every change of width pays.

- `flatten_into_clusters` ran `grapheme_indices` over every character. Every ASCII character is a cluster
  of its own except a carriage return followed by a line feed, so ASCII runs are split by
  `layout::ascii_clusters`, and `ascii_clusters_are_the_grapheme_clusters` holds the two to the same answer.
- `TextRenderer::advance` resolved the face, cloned an `Arc`, and looked the character up in the font's
  `cmap` twice for every character. ASCII advances are kept in a table for each face and size
  (`AdvanceTables`), and `a_remembered_advance_is_exactly_what_the_font_measures` checks every ASCII
  character at four sizes and three styles. `line_metrics` keeps its last answer.
- `place_one_line` compared each cluster's whole `CharStyle` with the run before it. A cluster from the
  same run as the one before it is now in the same run without the comparison.
- A paragraph's clusters and the run each came from were one list of pairs, so each line copied its
  clusters out one at a time. They are two lists now, and a line's clusters are one contiguous copy;
  `PlacedCluster` is `Copy`.
- `FontMetrics::ascii_advances` hands layout the whole ASCII table for a run's face and size in one call,
  rather than one call through a trait object for each character. A measurer that does not offer one is
  asked per character exactly as before, and an in process comparison of the two on the 206 KB file put the
  table 20 to 25% ahead.

With all of these, the 206 KB file lays out in about 10 ms with the machine's fonts, from 19.9, and in about
6.5 ms in `unluminous-core` with `FixedMetrics`, from 11. `cargo run --release -p unluminous-core --example
layout_cost -- <file>` is the second number.

### 2.7 The glyph atlas was uploaded whole for every new glyph

`TextRenderer::texture` cloned the whole 1024 by 1024 atlas, 4 MB, and sent all of it to the graphics card
on any frame that added a glyph, which a zoom of the canvas does on nearly every frame of the glide. It now
sends only the rectangle the new glyphs were drawn into, through `TextureHandle::set_partial`.

## 3. Research

A sweep of what has been written about egui, wgpu and editors in Rust. Most of it confirmed decisions this
repository had already made, so only what changed something or was ruled out is listed.

| Technique | Finding | Here |
|---|---|---|
| `MemoryHints::MemoryUsage` | the one lever aimed at the driver's share; wgpu's own docs say backends may ignore it | adopted, §2.5 |
| `desired_maximum_frame_latency` 1 | lowest latency, one queued frame | already eframe's default (`SurfaceConfig::LOW_LATENCY`) |
| `repaint_causes` | finds what is requesting frames | the frame trace already answers this |
| Segment heap manifest | VLC measured 266.1 MB to 264.4 MB | not worth the manifest change |
| mimalloc or snmalloc | no Windows working set measurement found | `task-1813` measured and rejected an allocator swap |
| `EmptyWorkingSet` when idle | moves pages to the standby list, costs page faults on restore | rejected: it changes the number Task Manager shows and nothing else |
| `codegen-units = 1`, fat LTO | about 2% faster, 5% smaller (Android toolchain) | `task-1805` measured nothing a person can feel |
| PGO, BOLT | 10 to 15% on rustc; BOLT is not practical on Windows | left for later, needs a training workload |
| `vello_cpu` `multithreading` | 20.1 to 5.5 ms here | still blocked: epaint's glyph rasteriser never calls `flush()` |
| Damage regions in egui | none exist | the levers are fewer repaints and fewer shapes |
| Zed, Lapce | Zed's memory drop on Windows came from moving to DirectX 11 | not open to eframe |

## 4. Results

Measured with every other agent on the machine asked to pause, the two builds launched in turn: 0.66.2,
then the final build, three times each, each launch on a fresh corpus. Each number is the median of the
three. Processor time is the process's own across the whole case; the median frame is `UnluminousApp::ui`
end to end. Processor time is counted in Windows clock ticks of 15.6 ms, so the cases that cost a few ticks
(`idle`, `open`) are left out.

| Case | Processor time, 0.66.2 | Now | Less | Median frame, 0.66.2 | Now | Faster |
|---|---:|---:|---:|---:|---:|---:|
| `type-train` | 422 ms | 250 ms | 41% | 8.6 ms | 1.9 ms | 355% |
| `type-test` | 344 ms | 234 ms | 32% | 8.6 ms | 1.6 ms | 428% |
| `type-train-md` | 406 ms | 219 ms | 46% | 9.9 ms | 1.5 ms | 547% |
| `type-test-md` | 234 ms | 141 ms | 40% | 8.1 ms | 0.6 ms | 1253% |
| `scroll-train` | 625 ms | 219 ms | 65% | 8.9 ms | 0.6 ms | 1510% |
| `scroll-test` | 656 ms | 297 ms | 55% | 8.8 ms | 0.7 ms | 1220% |
| `scroll-train-md` | 500 ms | 266 ms | 47% | 9.3 ms | 0.8 ms | 1096% |
| `scroll-test-md` | 500 ms | 375 ms | 25% | 11.0 ms | 0.7 ms | 1571% |
| `zoom-train` | 391 ms | 281 ms | 28% | | | |
| `zoom-test` | 359 ms | 219 ms | 39% | | | |
| `zoom-train-md` | 625 ms | 375 ms | 40% | | | |
| `zoom-test-md` | 219 ms | 203 ms | 7% | | | |
| `tab-switch` | 406 ms | 156 ms | 62% | 26.7 ms | 0.8 ms | 3399% |
| `canvas-zoom-train` | 578 ms | 406 ms | 30% | 3.9 ms | 2.0 ms | 93% |
| `canvas-zoom-test` | 500 ms | 516 ms | the same | 3.7 ms | 1.9 ms | 91% |
| `canvas-pan-train` | 438 ms | 203 ms | 54% | 8.5 ms | 1.7 ms | 395% |
| `canvas-pan-test` | 438 ms | 234 ms | 46% | 7.3 ms | 1.7 ms | 342% |

A zoom's median frame is not in the table because most frames of a zoom gesture are the glide between two
sizes, which costs little either way; the processor time is the measure there. A canvas zoom's processor
time moved less than its frame time for the same reason in reverse: the glide lasts a fixed time, so cheaper
frames are drawn more often within it.

| The whole window | 0.66.2 | Now | Less |
|---|---:|---:|---:|
| Processor time for the whole session | 11.03 s | 7.27 s | 34% |
| Working set after starting | 214.6 MB | 177.1 MB | 17% |
| Private bytes after starting | 414.3 MB | 273.1 MB | 34% |
| Working set at the end of the session | 267.7 MB | 219.3 MB | 18% |
| Private bytes at the end of the session | 468.1 MB | 347.4 MB | 26% |
| Until the control channel answers | 733 ms | 652 ms | 11% |

**Where each part of it came from.** An earlier quiet window measured every round's build, interleaved,
four launches of 0.66.2 and of the round 5 build and three of each of the others. The machine was busier
then, so its absolute numbers are higher; the ratios are what it is for.

| Round | Change | Whole session processor time | Tab switch | Working set after starting |
|---|---|---:|---:|---:|
| 0 | 0.66.2 | 13.98 s | 578 ms | 214.6 MB |
| 1 | the project state waits for a gesture to stop (§2.1) | 0.80x | 0.78x | 214.5 MB |
| 2 | the tab keeps its layout, the settings wait for a zoom, symbols from the tokens (§2.1 to §2.3) | 0.81x | 0.43x | 214.2 MB |
| 3 | the canvas decoration at half resolution while it moves (§2.4) | 0.80x | 0.35x | 214.4 MB |
| 4 | `MemoryHints::MemoryUsage` (§2.5) | 0.78x | 0.41x | 177.3 MB |
| 5 | layout, the advance tables and the partial atlas upload (§2.6, §2.7) | 0.69x | 0.30x | 177.5 MB |

The last round of layout work, the two cluster lists and the ASCII table handed to layout once a run, was
measured in process with `frame_cost` and `layout_cost`, and its effect is in the final table: the 206 KB
file lays out in about 10 ms with the machine's fonts, from 19.9 ms, and in about 6.5 ms in `unluminous-core`
with a measurer that does no work, from 11.

The record, in the layout the hill climb guide asks for, is `_agent_output/task-2218-perf/flow-hc/`:
`_state.json`, `baseline/` and `v1/` to `v5/` with a `results.jsonl` of one row per case per launch,
`trajectory/scores.tsv` and `report.html`. `_agent_output/task-2218-perf/quiet/` and `final/` hold the raw
rows, and `headline.mjs` recomputes this section's tables from them.

## 5. What was not done, and why

In the order they would be done next.

1. **A zoom step still lays the whole file out**: about 10 ms for a 200 KB file now, and the largest
   single cost left in any case here. Laying out only what is on the screen during a gesture would change
   what `Layout` promises (a height, and a position for every line) and every caller that relies on it, so
   it is a design of its own.
2. **Completion still rebuilds this tab's symbol lists on each letter while the popup is open.** Reading
   them is 0.2 ms now; the definitions map and the distinct words are most of the 0.8 ms a typing frame
   still spends there. Updating them from the edit, as the colouring is updated, is the next step.
3. **The fold regions are read again on each letter** (`editor-readings`, about 0.5 ms a typing frame),
   for the same reason.
4. **egui's own tessellation and the present** cost 0.6 to 1.8 ms on every frame on top of
   `UnluminousApp::ui`, more on the canvas, which has more shapes. Nothing inside Unluminous moves that
   except drawing fewer shapes.
5. **The heartbeat.** An idle window still draws twice a second, for the reason `app::HEARTBEAT` records,
   and each idle frame costs about 1 to 2 ms of processor time.
6. **Startup** is nearly all window and device creation inside the graphics driver.
