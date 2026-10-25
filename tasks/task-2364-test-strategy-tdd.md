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
6. **The release ran the gate under 80% processor load** with Word and the kernels in a lane beside
   clippy and the test build, which is the load under which the Word hang happened.

## 2. Measurements

(Filled in section 9 as the runs finished.)

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
   that differs) tried up to three times.

## 4. The gate (`tools/test.mjs`)

What changed:

- **One `cargo test` per crate.** The selected crates are still compiled together (`--no-run`), and
  then each crate's tests run in a command of their own, so a failure in one crate costs the others
  nothing. A crate's receipt is written when its own command passes.
- **A failed test is run once more, alone, on one thread.** If it passes, it is a flake: it is
  written to `_agent_output/test-runs/flakes.jsonl` and its crate is recorded as passed. A binary
  that ended without a result line (an abort, a hang that was stopped) is not retried.
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

`--jobs N` builds the suite once and runs N test executables at a time, each with its own run folder
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
- The `inillucent-*` folders are Inillucent's suites'. A ticket is filed for that repository.

## 9. Measurements

(Filled in from the runs.)

## 10. Ranked by hours saved

(Filled in from the runs.)
