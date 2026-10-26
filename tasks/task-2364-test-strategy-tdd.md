# task-2364: a test suite and a release that pass in one go

## What this is for

task-2359 (Word round 2) finished its code and verified it, and then spent many hours getting
through `tools/test.mjs`, the window suite and `tools/release.ps1`. Most of those hours went to
failures that had nothing to do with the change. Each one meant another 40 minute window suite run
or another 35 minute release attempt.

This document records what was measured, the rules the suites keep from now on, the changes that
were made, and what is left, ranked by the hours each one saves.

## Words used here

| Word | What it means |
|---|---|
| the gate | `node tools/test.mjs`: formatting, clippy and every crate's tests except the window suite. A ticket runs it after committing, and the release runs it again. |
| the window suite | `cargo test -p unluminous-app --test '*'`: 19 binaries that build the whole window and compare screenshots. |
| a receipt | A file that says a unit passed at some inputs, so it is not run again on the same inputs. The gate keeps one per crate (`_agent_output/test-receipts/`), the window suite one per binary and commit (`_agent_output/window-suite/`). Both live in the main checkout. |
| a flake | A test that fails and then passes when it is run again on its own, with nothing changed. |
| a run folder | The temporary folder one run of the gate or the window suite gets for itself, under `%TEMP%\unluminous-tests`. |

## 1. What task-2359 hit, and what each one turned out to be

(Measurements are in section 2.)

1. **19 of 28 Jupyter kernel tests timed out.** Python lists the folder it is started in on its
   first import, and the tests started Python in `%TEMP%`, which held about 166,000 entries. A
   kernel started in an empty folder answered in 17 s; started in `%TEMP%` it gave no answer in
   188 s. task-2359 fixed the kernel tests and `python.rs` (commit 80c69ac7). Nothing removed what
   the suites leave in `%TEMP%`, and every suite that starts a program in `%TEMP%` had the same
   dependence.
2. **The Word live tests hung the release for an hour, twice.** Under the release's parallel load,
   one of several Words driven through COM stopped answering. Its 15 minute watchdog stopped it, and
   `inspect.ps1` then waited for ever. A stopped Word also leaves a Resiliency record that makes the
   next open of that path wait on a dialog nobody can see. task-2359 made the tests use one Word at a
   time with a 10 minute limit (commit d146db21). The test still did not stop the Word when it gave
   up, because COM starts Word and it is not a child of the `pwsh` the test killed.
3. **Receipts went stale for reasons that could not change a test.**
   - The gate ran the tests of every crate that needed running as **one cargo command**. One
     failure anywhere (the hung `word_live`) left every crate in that command unrecorded, so the
     next attempt ran all of them again. The receipts at HEAD show it: every crate's test receipt
     has the same second, `d146db21 2026-10-25T16:09:10`.
   - A change to another crate's integration test (`crates/unluminous-word/tests/word_live.rs`)
     counted as an input of `unluminous-app` and of all 19 window binaries, although nothing a
     window is built from includes it.
   - `inillucent-driver` stayed at 2.8.1 from 1.14.0 to 1.15.1, so the release's
     `cargo update -p inillucent-driver` was not what changed the hashes.
4. **Window tests that failed once and passed alone.** Read one by one:
   - `plugins_board_and_chat a_contributed_tab_opens_in_the_editing_area_beside_the_file_tabs`
     opened its window on a copy of `sample_folder()` and then counted the files in
     `sample_folder()` itself, the folder every other test in the binary opens windows on. It
     failed 8 against 7 whenever another test wrote there between its two counts. That is a bug in
     the test.
   - `completion_semantic the_command_line_answers_the_servers_rows_and_signature_and_its_state`:
     `did()` drove a command with `run_command_line` and insisted on an answer on the frame it was
     asked. `editor signature --wait 3000` holds its answer until the language server replies, so
     the test failed whenever the server was one frame slower than the test.
   - The glass shapes test failed in `copy_out_of_the_repository` with "The system cannot find the
     file specified". It copies `sample_folder()`, and a file listed by `read_dir` was gone by the
     time it was copied.
   - **What was writing into the shared folder is the code index.** Atrius writes a fence file,
     `atrius-index-fence-<n>`, into the root of a project that is not a git repository and removes
     it 250 ms later (`atrius_index::freshness`). Every window a test opens on `sample_folder()`
     starts the index, so the folder gained and lost a file at moments nobody chose. The explorer
     has hidden the fence since task-2247; the tests that count or copy a project did not. The
     read only guard on `sample_folder()` (section 3) named it on its first loaded run.
   - Under load, kittest gave up on pictures with `PollError: The requested Wait timed out before
     the submission was completed`. A render waits ten seconds for the graphics card, and every
     test thread in a binary submits to the eight devices the binary shares.
   - `word.rs a_picture_is_resized_by_its_handle_and_wrapped_from_its_menu` depended on when the
     folder watch next looked (fixed in task-2359).
5. **A fault the old test path hid.** `did()` used to run a held command synchronously in the test
   (`answer_git_status_now`) instead of through frames. With `did()` answering through frames like a
   real request, `a_recording_played_live_fills_the_prompt_as_it_plays` showed the caret left at the
   start of the prompt after live dictation: egui keeps a text box's caret where it was when its text
   is replaced from outside, so the next key a person pressed went in front of the dictated words.
   Fixed: dictated words leave the caret after them (`PaneState::caret_to_the_end`).
6. **A crash the walk found under load.** `every_action_changes_the_window_or_says_why_it_cannot_4`
   failed once on `assertion failed: bytes.end <= self.text.len_bytes()` in `LayoutTextView`. Reading a
   file again (`reread_a_tab`) replaced the tab's document and forgot its layout only when the tab was
   the active one. Every document counts its text revisions from one, so a file showing in the other
   pane of a split, never edited and then made shorter on disk (which is what an agent does), kept the
   layout of its longer text under the same revision and was painted with it. In a debug build that is
   the assertion; in a release build it is the rope's `assert!` (`slice past the end of the text`),
   which ends the process. Fixed: the layout goes with the document whichever tab it is.
   `a_file_read_again_in_the_pane_without_the_keyboard_is_laid_out_again` reproduces it every time
   without the fix.
7. **The release ran the gate under 80% processor load** with Word and the kernels in a lane beside
   clippy and the test build, which is the load under which the Word hang happened.

## 2. Measurements

All on this machine (24 logical processors, the window suite on 19 of them through
`tools/capped.ps1`), on 2026-10-25. "Under load" is `tools/perf-smooth/load.mjs --threads 12`
beside the suite, which is about what a release's build or a second ticket's suite puts on the
machine. The raw logs are in `_agent_output/task-2364-tests/` of this ticket's worktree and the run
records in `_agent_output/test-runs/` of the main checkout.

### `%TEMP%`

| | entries |
|---|---:|
| everything at the top of `%TEMP%` | 169,221 |
| `inillucent-*`, from Inillucent's suites | 105,562 |
| `unluminous-*`, from this repository's suites | 54,925 |
| what one gate run with the new runner left behind | 0 (279 entries at the top of its run folder, removed at the end) |

A lib test run of `unluminous-app` alone left about 100 uniquely named folders (`unluminous-db-*`,
`unluminous-chat-plugin-*`, `unluminous-browser-*`, ...), and there had been about 550 of them.

### The window suite

| run | code | wall time | failures |
|---|---|---:|---|
| alone, `cargo test --test '*'` | before | 24.5 min | none |
| under load, `cargo test --test '*'` | before | 36.4 min | `canvas_realm the_realm_manager` (modal backdrop missing from both pictures) |
| `canvas_realm` 5 times under load, runner | partial | 76 to 152 s a run | 9 in 2 of 5 runs: 7 the code index's fence file, 2 `PollError` render timeouts |
| alone, runner, one binary at a time | after | 14.5 min | 10 pictures that draw the fixture path (accepted again), 1 abort in `notebooks` |
| alone, runner, `--jobs 3`, 19 threads each | after | 11.3 min | the same pictures; three heaviest binaries 3 to 4 times slower than alone |
| alone, runner, `--jobs 2`, 10 threads each | after | 11.7 min | the walk's ordering race, `Untitled.ipynb` written into the shared folder (both fixed) |
| alone, runner, `--jobs 2` | all fixes but the profile | 9.6 min | none |
| under load, runner, `--jobs 2` | all fixes but the profile | 18.5 min | 1 flake retried (`the_description_and_the_comments_fold...`, the relative time) |
| alone, runner, `--jobs 2`, dependencies optimized | final | 4.0 min | none |
| alone, runner, final commit, twice | final | 4.8 and 4.3 min | none |
| under load, runner, final commit, twice | final | 7.3 and 7.0 min | one: `canvas_realm` ended with no output 13 s in, passed run again whole (task-2369) |

So the whole window suite takes about four and a half minutes alone and about seven under load, against
24.5 and 36.4 before, and in four final runs one binary needed running again and no test failed.

The three longest binaries before and after splitting their longest tests, alone, one binary at a
time:

| binary | before | after |
|---|---:|---:|
| `window_and_chrome` | 405 s | 99 s |
| `panel_docking` | 331 s | 78 s |
| `plugins_board_and_chat` | 289 s | 208 s |
| `command_line` | 94 s | 48 s (the walk's drives side by side) |

With dependencies optimized (section 4a), the same binaries alone, two at a time: `window_and_chrome`
104 s, `plugins_board_and_chat` 99 s, `debugging` 41 s, `panel_docking` 28 s, every other one under
25 s.

Eleven tests ran for more than a minute each before; libtest says so with `has been running for over
60 seconds`, and each held its binary open after every other test in it had finished.

### The gate

| run | wall time | what ran | failures |
|---|---:|---|---|
| `tools/test.mjs --all`, cold `clippy` and `waiting` targets | 22.4 min | all 26 units | clippy's `type_complexity` on a new test (fixed), and `unluminous-terminal`'s verbatim path test, which wrapped a longer path across the grid (fixed) |
| `tools/test.mjs` after those two fixes | 78 s | 6 of 26 units | none |

In the `--all` run, `unluminous-word` took 835 s in the late lane, 745 s of it `word_live`: 11 tests,
one Word at a time, about 68 s each. `unluminous-jupyter`'s kernel tests took 54 s, against 387 s
when task-2359 measured them with `%TEMP%` as their folder.

### What task-2359's receipts show

Every crate's test receipt at HEAD has the same second, `d146db21 2026-10-25T16:09:10`: one cargo
command ran them all, and before that, attempts in which one crate failed recorded none.
`inillucent-driver` stayed at 2.8.1 from 1.14.0 to 1.15.1, so the release's
`cargo update -p inillucent-driver` changed no hash.

## 3. Rules every test keeps

These are written into `CLAUDE.md` as well, under "Tests".

1. **Every test writes only inside a folder of its own.** A test that changes a project works on a
   copy (`copy_out_of_the_repository`, `fixture(name)`, `scratch_folder(name)`). `sample_folder()`
   is read only: it checks its own listing every time it is asked for, and panics naming the files
   a test added or removed.
2. **No test depends on what is in `%TEMP%`.** The gate and the window suite runner give each run a
   folder of its own and point `TMP`, `TEMP` and `TMPDIR` at it, so Rust's `std::env::temp_dir()`,
   Python and PowerShell all write there. The folder is removed when the run ends, and a folder a
   killed run left is removed by the next run.
3. **A wait is a deadline in time and a condition, never a count of frames.** `common::wait_until`
   draws frames until a condition holds, with a minute's deadline. A held command (`--wait`,
   `--wait-for`) is driven with `did()`, which now waits for the window to answer it exactly as the
   control channel does.
4. **A test that drives another program stops that program when it gives up.** `word_live` reads
   the Word process id `inspect.ps1` writes and stops it on a timeout.
5. **A test that needs another program skips without it, and fails without it in the gate.**
   `UNLUMINOUS_REQUIRE_KERNEL` and `UNLUMINOUS_REQUIRE_WORD` already do this. A new dependency on
   the machine gets a variable of the same kind.
6. **A test that is ignored says why.** `#[ignore = "<reason>"]`, and a flaky test that has been
   taken out of the gate says `#[ignore = "quarantined: task-N <what fails>"]`.
7. **A test that lists or copies a project leaves out what the explorer leaves out**, the code
   index's fence above all (`common::is_the_code_index_fence`).
8. **No single test holds a binary open.** A binary takes as long as its longest test, so a sweep
   over many cases that each open their own window is split into one test per case, which libtest
   runs side by side.
9. **Every picture goes through `PatientRenderer`**, the renderer `common::builder` gives every
   harness: at most four renders in flight in a binary, and a render that failed (not a picture
   that differs) tried up to three times. kittest returns some failures as an error and panics on
   others (`Failed to poll device: Timeout` is an `expect`), so the panic is caught too, and the
   receipt's panic hook is told it is expected (`receipt::while_a_panic_is_expected`).
10. **A relative time is drawn from the drawing clock.** `clock::now_for_drawing` and
    `clock::seconds_for_drawing` answer the real clock, except on the thread of a window built with
    `draw_deterministically`, where they answer the moment the window was built. The ticket modal said
    `just now` for a ticket a test had just made and `1m ago` when a loaded run took over a minute to
    photograph it. The board's own timestamps still use the real clock.

## 4. The gate (`tools/test.mjs`)

What changed:

- **One `cargo test` per crate.** The selected crates are still compiled together (`--no-run`), and
  then each crate's tests run in a command of their own, so a failure in one crate costs the others
  nothing. A crate's receipt is written when its own command passes.
- **A failed test is run once more, alone, on one thread.** If it passes, it is a flake: it is
  written to `_agent_output/test-runs/flakes.jsonl` and its crate is recorded as passed. A binary
  that ended without a result line (an abort, a hang that was stopped) is run again whole, and the
  line it died on is recorded with the flake.
- **The tests that drive other programs run after the other lanes.** `unluminous-jupyter` (real
  kernels) and `unluminous-word` (Word through COM) are compiled beside clippy and the test build,
  and their tests start once both have finished, so the Word and kernel tests run on a quiet machine.
- **A run folder** for every test command.
- **Inputs.** Another crate's `tests/*.rs`, `benches/*.rs` and `examples/*.rs` are not inputs of a
  crate that depends on it, because they are compiled into nothing that crate builds. Their
  fixtures stay inputs: `unluminous-app`'s tests read `unluminous-word/tests/fixtures` and
  `unluminous-chat/tests/streams`. `unluminous-app`'s own unit runs `--lib --bins`, so its own
  `tests/*.rs` (the window suite) are not that unit's inputs either. Clippy builds `--all-targets`,
  so clippy's own tests stay its inputs.
- **A record of every run** in `_agent_output/test-runs/runs.jsonl`, with each crate's time.

## 4a. Dependencies are optimized in a test build

`[profile.test.package."*"] opt-level = 2` in the workspace `Cargo.toml`. The workspace's own crates
stay at `opt-level = 0`, so a panic still names its file and line and our code is built as it was.

A window test spends nearly all of its time in code that is not ours: egui laying out and
tessellating a frame, `vello_cpu` and `rux` rasterizing the decoration, `image` reading and comparing
pictures. Unoptimized, one frame of the forty-card AI Configuration page took seconds. Measured on
single tests run alone:

| test | before | after |
|---|---:|---:|
| `the_acp_registry_is_cards_that_add_and_remove_an_agent` | 157 s | 17 s |
| `create_task_with_no_title_keeps_the_ticket_and_says_the_title_is_needed` | 56 s | 7 s |

The whole window suite went from 9.6 to 4.0 minutes. Every accepted picture still matched: Rust does
not reorder floating point arithmetic at any optimization level, and `vello_cpu`'s SIMD level is
pinned in a test window. The cost is a one time rebuild of the dependencies, about five minutes on
this machine, and test builds no longer share dependency artifacts with `cargo run`, which they had
already stopped doing for `debug = "line-tables-only"`.

## 5. The window suite

`pwsh tools/capped.ps1 node tools/window-suite.mjs --run` is how the suite is run now. It does three
things the bare `cargo test` left to whoever typed it:

- a run folder, removed at the end;
- one retry of each failed test, alone, with `UNLUMINOUS_RECEIPT_RETRY=1`, so a binary whose only
  failure was a flake keeps its receipt;
- a record of the run, each binary's time and result.

`--only word,git` runs some binaries, `--repeat N` runs the suite N times and prints which tests
failed in which runs, and `--load N` runs `tools/perf-smooth/load.mjs` beside it with N threads,
which is how a release's load is reproduced.

`--jobs N` (two by default) builds the suite once and runs N test executables at a time, each with its own run folder
and `RUST_TEST_THREADS` set to its share of the processors. Giving each of three binaries all 19 test
threads made each take three to four times as long as it took alone.

**Where the window tests' fixtures are.** The fixtures used to be folders with fixed names straight
under `%TEMP%`, and their full paths are drawn in ten accepted pictures (the chat pane's project row,
the run dialog's directory, the new file prompt, the project tabs' list). A run folder with a time
stamp in it would change those pictures on every run. So every fixture is now under
`%TEMP%\unluminous-window\<binary>` (`common::fixtures_root`), worked out from the machine's
temporary folder (`UNLUMINOUS_MACHINE_TEMP` under a runner), so a test run by hand and one run by the
runner draw the same path, and two binaries never share a fixture. The ten pictures were accepted
again after checking that the path was the only difference. The binary's lock
(`tests/common/turn.rs`) is in that folder, and taking it empties the folder once, so fixtures named
with a process id no longer pile up.

Receipts:

- **A filtered run writes no receipt.** `cargo test --test word -- one_test` used to leave a
  receipt saying all of `word.rs` had passed. Only a run of the whole binary writes one, or the
  runner's retry, which follows a whole run.
- **Other crates' test code does not make a receipt stale.** `crates/<crate>/tests/*.rs`,
  `benches/` and `examples/` of every crate but the window suite itself.
- **Test fixtures and `sample-diagrams/` do.** They were not counted at all, so a receipt could
  answer for pictures of a document that had since changed.

## 6. Word and the kernels

- `inspect.ps1` clears Word's Resiliency records for the document's folder before it starts Word
  and after it stops it (`Clear-WordResiliency` in `WordCom.ps1`), and writes Word's process id to
  `<report>.pid`.
- `word_live` stops that Word on its timeout.
- Both run on a quiet machine, after the other lanes (section 4).
- They stay in the gate. They only run when `unluminous-word` or `unluminous-jupyter` changed (the
  gate's receipts), and when either does change, Word opening what Unluminous writes and a real
  kernel running a cell are the checks that matter. The nightly runs them every night whatever
  changed.

## 7. Flakes

- The gate and the window suite runner retry a failed test once, alone. A test that passes then
  does not stop a release, and is written to `flakes.jsonl`.
- `node tools/flakes.mjs` names the tests that did this in the last 14 days, most frequent first.
  The release prints it after the gate.
- **A test named twice in 14 days gets a ticket**, and is fixed, or marked
  `#[ignore = "quarantined: task-N <what fails>"]` until it is. A quarantined test still runs in
  the nightly (`--include-ignored` is not used; the nightly's `--all` and the window run under load
  are what find new ones, and the ticket is what brings a quarantined one back).
- A test that fails twice in a row is a failure, not a flake.

## 8. Cleaning up

- A run removes its own run folder. The next run removes any run folder whose process has gone and
  that is more than six hours old.
- `node tools/clean-test-leftovers.mjs` reports what runs before this left at the top of `%TEMP%`
  and Word records naming the temporary folder, and `--delete` removes them. It never touches the
  folders the shipped program uses there (`unluminous-python`, `unluminous-update-*`,
  `unluminous-report-*`).
- The `inillucent-*` folders are Inillucent's suites', and task-2367 asks that repository to remove them.

## 9. The changes, ranked by the hours they save

Hours are per ticket that changes code, estimated from task-2359 and the runs above. A ticket
typically runs the gate twice and the window suite twice, and the release runs the gate again.

| # | Change | Saves | How |
|---|---|---|---|
| 0 | Dependencies optimized in test builds | about 20 min per window suite run, with the splitting below | 24.5 min to 4.0 min for the whole suite alone, together with rows 4 and 5. Most of a window test's time was unoptimized egui, `vello_cpu`, `rux` and `image`. |
| 1 | Flake retry and record (gate and runner) | 0.5 to 2 h when a flake happens | A test that fails under load is run again on its own instead of sending the person round the whole suite. task-2359 saw five unrelated failures in one afternoon, each a 40 minute rerun. |
| 2 | One `cargo test` per crate in the gate | up to the whole gate per failure | One crate's failure no longer leaves the others unrecorded, so the next attempt runs only the crate that failed. task-2359's release reran every crate after `word_live` hung. |
| 3 | Run folders for every test command | the 19 kernel failures, and every later dependence on `%TEMP%` | Nothing lists 169,000 entries; nothing a run writes is left behind; lib tests with fixed folder names in two worktrees stop colliding. |
| 4 | Split the eleven longest tests, run the walk's drives side by side | about 10 min per window suite run | 24.5 to 14.5 min one binary at a time. |
| 5 | `--jobs 2` | about 3 min per window suite run | 14.5 to 11.7 min. |
| 6 | Receipts that ignore other crates' test code | a 40 min window run each time a test file elsewhere changes | `word_live.rs` alone sent all 19 binaries round again in task-2359. |
| 7 | The read only guard on `sample_folder()`, the fence left out | the plugin tab count, the vanishing file, `Untitled.ipynb` | Found three writers in its first runs, none of them a fault in the product. |
| 8 | `PatientRenderer` | render timeouts under load | Two `PollError` failures in five loaded `canvas_realm` runs, none since. |
| 9 | Word: pid file, stop on timeout, Resiliency records cleared, late lane | the two one hour hangs | A Word that stops answering is stopped at the test's limit and leaves nothing that makes the next open wait. |
| 10 | `did()` answers held commands through frames | `editor signature --wait` and its kind | A command that holds its answer is waited for the way the control channel waits, which also found the dictation caret fault. |
| 11 | The drawing clock | a 160 pixel difference under load | `just now` stays `just now` however long a loaded run takes to photograph it. |
| 12 | The layout forgotten on every reload | a crash in the shipped window | Not test time at all: a product fault the loaded walk found, now with a test that reproduces it. |

## 10. What is left

Filed as tickets:

- **`word_live` takes 745 s** (task-2368). Each test starts a Word, and `inspect.ps1` asks for every line of
  every paragraph. One Word for the whole binary, reused by every inspect, would cut most of it. Done in
  task-2368: 1,259 s before and 211 s after, measured one after the other on the same machine.
- **`notebooks` aborted once** (task-2369) in a `windows-core` COM call ("misaligned pointer dereference", a
  non unwinding panic, so the process ended). Not reproduced in six more runs. The runner now runs a
  binary that ends without a result again whole and records the line it died on, so the next one
  arrives with a backtrace.
- **Inillucent's suites leave about 105,000 folders in `%TEMP%`** (task-2367, in that repository).
- **The code index writes its fence into the root of a project that is not a git repository** (task-2370). The
  explorer hides it and the tests now leave it out, but any other program watching that folder sees
  a file come and go every time the index checks it is fresh. Worth asking Atrius to put it in a
  folder of its own.
- **`canvas_realm the_realm_manager`** lost its modal backdrop once, in the loaded baseline before
  any change. Not seen in the runs since. The most likely cause is the render timeout above, which
  makes kittest drop the screenshot the backdrop is made from.
