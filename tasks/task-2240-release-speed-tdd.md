# Build, test and release time, and a share of the machine

Technical design, task-2240. Measured on 2026-10-10 on the development machine: Intel Core Ultra 9
285, 24 logical processors (8 performance cores and 16 efficiency cores), 128 GiB, rustc and cargo
1.95.0. Every number has its command, its log and its processor samples under
`_agent_output/task-2240-release-speed/` in the main checkout: `measure.ps1` runs one command and
samples the whole machine once a second, `measurements.tsv` has one row per measurement, and each
measurement has a `.log` and a `.cpu.csv`. No other ticket was in progress on the board while these
were taken.

## 1. Introduction

The ticket: a release seems to take hours and holds the processor at 100%. Builds, tests and the
release must use at most 80% of the processor, and the release needs a strategy that tests only what
a change needs and does independent work at the same time. Inillucent had the same problem
(task-2114 and task-2205), and this design reuses what those tickets measured.

The work found five things.

1. **Nothing limited the processor.** Clippy, the test build and the three release builds each took
   all 24 processors for most of their length: 25% to 58% of the one second samples were over 80%.
2. **One crate's tests are most of the release's test time, and that crate rarely changes.** The
   Jupyter kernel tests (`unluminous-jupyter`, `tests/kernel.rs`) took 119 s of a 168 s workspace test
   run, and 182 s of a 202 s run with nothing to compile. They start real Python and Rust kernels and
   the machine sat at a mean of 32% to 37% while they ran. `unluminous-jupyter` changed in 4 of the
   last 20 releases. Nine of those 20 releases changed no crate but `unluminous-app`, and one changed
   no crate at all, yet every release ran clippy and the tests for all ten.
3. **The three release builds ran one after another.** After a one line change to `unluminous-app`
   the Windows build and the two macOS architectures took 383 s in a row. Run at the same time in one
   target directory they took 507 s, because cargo holds a lock on a target directory for the whole
   build. Run at the same time in a target directory each, on 80% of the processors, they took 210 s.
4. **The window suite ran twice for every ticket.** Each window test binary writes a receipt into the
   `_agent_output/window-suite/` of the checkout it was built in. A ticket runs the suite in its own
   worktree, the release reads the receipts of the main checkout, so after a merge the main checkout
   had none and the whole suite was built and run again there. A first build and run of the suite
   took 832 s. And every release made every receipt stale, because the version bump changes
   `Cargo.toml`, a file a window is built from.
5. **Nobody could say where a release's time went.** Nothing recorded it. The 0.73.0 release can only
   be reconstructed from file times: its last ticket commit is 16:09:03, the Windows installer was
   written at 16:14:12, the macOS zip at 16:21:00 and the release commit at 16:21:09, before the
   GitHub releases and the two sites.

## 2. Goals

| # | goal | before | after |
|---|---|---|---|
| G1 | Every build and test the release starts uses at most 80% of the logical processors | no limit; 25% to 58% of samples over 80% | 19 of 24 processors, inherited by every child, §5.1 |
| G2 | A cargo command typed by hand can use the same limit | no way | `pwsh tools/capped.ps1 <command>` |
| G3 | The release gate runs clippy and tests only for crates whose inputs changed | all ten crates on every release | §5.2 |
| G4 | The Windows build and both macOS architectures build at the same time | 383 s in a row (322 s on the share) | 210 s at once, on the share, §5.3 |
| G5 | A window suite run in a ticket's worktree counts for the release | it did not; the suite ran again in the main checkout | §5.4 |
| G6 | A release says where its time went | nothing recorded | `_agent_output/release-timings/<version>.json`, §5.5 |

## 3. What was measured before any change

All in a worktree of `main` at `1802fb66` (0.73.0), in an empty target directory, with no processor
limit.

### 3.1 The release gate

| step | command | seconds | mean machine | samples over 80% |
|---|---|---:|---:|---:|
| B1 | `cargo fmt --all -- --check` | 5.7 | 16% | 0% |
| B2 | `cargo clippy --workspace --all-targets -- -D warnings`, cold | 56.7 | 46% | 25% |
| B3 | `cargo test --workspace --exclude unluminous-app`, cold | 168.4 | 32% | 10% |
| B3w | the same again, nothing to compile | 201.7 | 37% | 2% |
| B4 | `cargo test -p unluminous-app --lib --bins` (87 s of it is the build) | 104.0 | 51% | 20% |

The run time of each test binary in B3w:

| binary | seconds |
|---|---:|
| `unluminous-jupyter` `tests/kernel.rs` | 182.0 |
| `unluminous-dap` lib | 5.0 |
| `unluminous-terminal` lib | 3.0 |
| every other binary | under 3 each |

The kernel tests are 28 tests that each start a Python or Rust kernel. They do not compute: the machine
was at a mean of 37% while they ran. Measured once more while a release build ran beside them, each
kernel test took up to 272 s, so they are also the binary most slowed by other load.

### 3.2 The release builds

| step | command | seconds | mean machine | samples over 80% |
|---|---|---:|---:|---:|
| B6 | Windows `cargo build --release`, both binaries, cold | 203.3 | 75% | 46% |
| C1 | macOS arm64 `cargo zigbuild --release`, cold, same target directory | 223.4 | 62% | 34% |
| C2 | macOS x86-64, cold, same target directory | 187.2 | 72% | 46% |
| I1 | Windows, after a one line change to `unluminous-app` | 109.4 | 63% | 21% |
| I2 | macOS arm64, after the same change | 161.5 | 46% | 6% |
| I3 | macOS x86-64, after the same change | 112.3 | 55% | 16% |
| P1 | I1, I2 and I3 at once, one target directory | 507.4 | 79% | 58% |

After an ordinary change each build recompiles one crate, `unluminous-app`, and links. One crate keeps
about half the machine busy: 16 codegen units and thin LTO run in parallel only for part of the time.
That is the room §5.3 uses.

P1 is slower than the three in a row. Every cross build writes its build scripts and procedural
macros into the same `release` folder the Windows build uses, and cargo holds its lock on a target
directory for the whole build: both macOS logs say `Blocking waiting for file lock on build
directory` four times.

### 3.3 The window suite

| step | command | seconds | mean machine | samples over 80% |
|---|---|---:|---:|---:|
| W1 | `cargo test -p unluminous-app --test '*' --no-fail-fast`, first build of the suite | 832.2 | 67% | 37% |
| W2 | the same after a one line change to `unluminous-app`, no limit (60 s of build) | 650.0 | 54% | 28% |
| W3 | the same after another one line change, on the 80% share (33 s of build) | 655.8 | 62% | 47% |

The suite is fifteen binaries run one at a time, because each holds a lock for the whole machine
(`tests/common/turn.rs`). The longest in W1 were `plugins_board_and_chat` 139 s, `debugging` 89 s,
`notebooks` 73 s, `command_line` 58 s and `canvas_realm` 39 s. The 80% share cost the suite nothing
(W3 against W2).

**The suite fails on unchanged code.** Every one of W1, W2, W3 and W5 had one to three failures, on
code identical to `main` apart from this ticket's receipt change, and the failing tests were
different each time: the find bar's count, a terminal tab's output, the update check's local server
refusing a connection, two snapshot comparisons, a menu and the MCP settings page. W4 passed
completely. A failing binary deletes its receipt, so each of these costs a ticket a rerun of that
binary at least.

## 4. Research

- **Inillucent task-2205** measured on this machine that a Windows job object with a hard processor
  rate cap does not hold: `pwsh` is the Microsoft Store package, Windows runs it in a job with
  `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`, and every program it starts leaves every job. A processor
  affinity mask is copied into every child at creation and is not a job, so cargo, rustc and the
  linker all keep it. Below normal priority starved builds behind other tickets' builds. This design
  uses the affinity mask, and §5.1 confirms the inheritance again here: a `powershell` started by
  `node` started by `tools/capped.ps1` reported mask `0xC1FFFF`.
- **Inillucent task-2114** split its suite by cadence and made the release rely on evidence a
  scheduled run had already produced for the same commit. Unluminous has no continuous integration
  (task-1928 removed it) and its nightly runs only the six agent tests, so there is no other run to
  rely on. The equivalent here is a receipt per crate keyed by the content the crate reads (§5.2):
  the release relies on a pass that already happened, wherever it happened.
- **Selecting tests by change.** Cargo has no built in way to test only changed crates. The common
  tools ([cargo-test-changed](https://hn.svelte.dev/item/43547861),
  [cargo-delta](https://gitblind.noratr.app/tekian/cargo-delta),
  [cargo-workspaces `changed`](https://crates-io.ganymede-static.progval.net/crates/cargo-workspaces/0.2.5),
  [determinator](https://docs.rs/determinator/)) diff against a git reference and take the crates
  that depend on what changed. A git reference is the wrong key here: the same code is tested in a
  worktree on one commit and released from the main checkout on another, and every release changes
  the version. A hash of the content each crate reads answers the real question, which is whether
  these exact inputs have passed before, and needs no new dependency.
- **Concurrent cargo commands.** Cargo takes a lock per build directory, and concurrent builds that
  share one run one at a time ([a measured case](https://git.azcomputerguru.com/azcomputerguru/claudetools/commit/9c9efdd34bf8a286c17026298d744a69e90bd61f),
  [cargo's layout module](https://doc.rust-lang.org/nightly/nightly-rustc/cargo/core/compiler/layout/index.html)).
  P1 measured the same here. The fix is a target directory per concurrent build.
- **cargo-nextest** runs each test in its own process and reports 1.4 to 3.4 times faster suites on
  Linux, and about 7% to 13% on Windows CI
  ([nextest benchmarks](https://nexte.st/book/benchmarks), [one Windows measurement](https://forge.lthn.ai/core/core-agent-ide/commit/16309d6b68546c392fbec3d30fb65e404bd8e3fa)).
  Here the test time outside the window suite is one binary waiting on kernels, so the gain is
  small, and the window suite holds a machine wide lock per binary on purpose. Not adopted.
- **cargo-zigbuild** has a `universal2-apple-darwin` target that builds both architectures in one
  command ([cargo-zigbuild](https://pypi.org/project/cargo-zigbuild)). It builds the two one after
  the other inside that command, so it would not give the overlap §5.3 measures, and it would replace
  the slice check `New-UniversalBinary` already does. Not adopted.

## 5. Design

### 5.1 A share of the processors

`tools/processor-share.ps1` defines `Enter-ProcessorShare`, ported from Inillucent's
`packaging/stage-layout.ps1`:

- the share is `-Percent`, else `UNLUMINOUS_CPU_PERCENT`, else 80, and 100 removes the limit;
- it keeps that share of the logical processors, rounded down, fastest efficiency class first, then
  lowest index, among the processors the process already has. At 80% here that is 19 processors,
  mask `0xC1FFFF`: all 8 performance cores and 11 efficiency cores;
- it sets the process affinity mask, which every child inherits;
- it sets `CARGO_BUILD_JOBS` and `RUST_TEST_THREADS` to 19 unless the caller set them, because cargo
  and libtest read the machine's processor count and would start 24 jobs to share 19 processors;
- it sets `UNLUMINOUS_CPU_SHARE_ENTERED`, so a script started by another keeps the mask it
  inherited.

Called by `tools/release.ps1` (before anything is started, and not under `-WhatIf`),
`tools/nightly.ps1`, and `tools/capped.ps1`, which runs any command under it with `CC` and `CXX`
removed:

```
pwsh tools/capped.ps1 cargo test -p unluminous-app --test '*' --no-fail-fast
```

The window suite command the release and `tools/window-suite.mjs` print now starts with
`pwsh tools/capped.ps1`.

The five processors outside the mask carry nothing from the release at any instant, which is what
keeps the desktop responsive. A rate cap would let the release use all 24 for most of each scheduling
interval.

### 5.2 Clippy and tests only for the crates whose inputs changed

`tools/test.mjs` replaces the three cargo commands in the release gate. `cargo fmt --check`,
`changelog.mjs --check` and `contrast.mjs --check` still run every time; together they take about
6 s.

**A unit** is clippy or the tests of one workspace crate, run with exactly the command the gate ran
before: `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>`, and for
`unluminous-app` `cargo test -p unluminous-app --lib --bins`.

**A unit's hash** is a SHA-256 over:

1. the unit's kind, command and environment (`UNLUMINOUS_REQUIRE_KERNEL=1` for the tests);
2. `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.cargo/config.toml`, `rustfmt.toml` and
   `kittest.toml`, with the workspace version replaced by `x` in the first two
   (`tools/versionless.mjs`), so a release's version bump does not change any hash while a dependency
   upgrade does;
3. the path and git blob id of every file the crate reads. For a crate other than `unluminous-app`
   that is every tracked file under its own folder and under the folder of every workspace crate it
   depends on, directly or not. For `unluminous-app` it is every tracked file in the repository
   except `.claude/`, `tasks/`, `design/`, `releases/`, `tools/` and the top level documents
   (`CHANGELOG.md`, `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
   `SECURITY.md`, the licences, `.gitignore`, `.gitattributes`).

The rule for `unluminous-app` is written as what it does not read because its tests read widely:
`naming.rs` walks `crates`, `unluminous-cli`, `documentation` and `README.md`, the window tests read
`sample-diagrams` and `sample`, and `build.rs` reads `installer/icon`. A folder added later is read
until somebody adds it to the list, which is the safe direction for that mistake.

Blob ids come from `git ls-files -s`, and a file changed on disk but not committed is hashed with
`git hash-object`, which applies the same line ending conversion a commit does. So a worktree, whose
files have carriage returns, and the main checkout, whose files do not, give the same hash for the
same commit.

**The workspace graph is shallow**, which is why this pays:

| crate | workspace crates it depends on |
|---|---|
| `unluminous-core`, `-chat`, `-db`, `-dap`, `-git`, `-terminal`, `-jupyter`, `unluminous-cli` | none |
| `unluminous-lsp` | `unluminous-core` |
| `unluminous-app` | all nine others |

A change to `unluminous-app` alone runs clippy and the lib and bins tests for `unluminous-app` and
nothing else. A change to `unluminous-core` runs `unluminous-core`, `unluminous-lsp` and
`unluminous-app`. A change to documentation only runs `unluminous-app`, because its naming test reads
documentation. A change only to `tasks/` or `tools/` runs nothing.

**Receipts.** When a cargo command passes, every unit it ran is appended to
`_agent_output/test-receipts/<kind>-<crate>.jsonl` in the **main checkout**, found from any worktree
through `git rev-parse --git-common-dir`. Each line holds the hash, the commit, the time and the
checkout. The file keeps the last 50. A unit whose hash is in its file is skipped and the plan says
when it passed. A failing command writes nothing.

So a ticket that runs `pwsh tools/capped.ps1 node tools/test.mjs` in its worktree after committing
has already done the release's clippy and tests, and the release in the main checkout finds them
passed and runs nothing.

**Three lanes at once.** The selected units are grouped into at most four cargo commands, and the
commands run in three lanes at the same time:

| lane | command | target directory |
|---|---|---|
| clippy | `cargo clippy -p <every selected crate> --all-targets -- -D warnings` | `target/clippy` |
| waiting | `cargo test -p unluminous-jupyter --no-fail-fast` | `target/waiting` |
| test | `cargo test -p <other selected crates> --no-fail-fast`, then `cargo test -p unluminous-app --lib --bins` | `target` |

Each lane has its own target directory because cargo holds its lock on a target directory for the
whole of `cargo test`, the test run included (§3.2, P1). Clippy writes metadata and a test build
writes code, so the clippy lane shares nothing with the test lane anyway. The Jupyter kernel tests
spend their 2 to 3 minutes waiting, and the other two lanes use those processors.

`node tools/test.mjs --dry-run` prints the plan, `--all` runs every unit, and `tools/release.ps1
-FullTests` passes `--all`. `-SkipTests` is unchanged.

**What this gives up.** `cargo test -p a -p b` unifies features across the selected crates only,
where `--workspace` unified them across all of them, so a crate is now tested with the features its
own selection needs. That matches how each crate is used by `unluminous-app` more closely, not less.
And a receipt trusts that a pass on some content still holds on the same content: a change outside
the hash, such as a Python upgrade that breaks the kernel tests, is not seen until something in that
crate changes or `-FullTests` is passed.

### 5.3 The three release builds at once

The release starts `installer/macos/build-on-windows.ps1 -Notarize` as a background process before
the Windows build, with its output in `_agent_output/release-logs/macos-<version>.log`, and waits for
it after the Windows installer is built, installed and copied. The last 40 lines of its log are
printed and a failure stops the release before anything is tagged, exactly as before. If the Windows
build fails first, the macOS process and everything it started are stopped.

`build-on-windows.ps1` starts both architectures at once, each in its own target directory,
`<target>/mac-aarch64-apple-darwin` and `<target>/mac-x86_64-apple-darwin`. The link argument the
signature needs, `-headerpad_max_install_names`, moves from a `--config` argument to
`CARGO_TARGET_<TRIPLE>_RUSTFLAGS`, which is the environment form of the same setting and needs no
quoting through a process start.

`installer/windows/build.ps1` builds `unluminous` and `unluminous-cli` in one cargo command instead of
two, so cargo schedules the command line's crates into the gaps.

| measurement | seconds | mean machine | samples over 80% |
|---|---:|---:|---:|
| I1 + I2 + I3, one after another, no limit | 383.2 | 46% to 63% | 6% to 21% |
| S3, the same three one after another on the 80% share | 322.3 | 44% to 61% | 8% to 28% |
| P1, all three at once, one target directory, no limit | 507.4 | 79% | 58% |
| P2, all three at once, a directory each, no limit | 239.9 | 65% | 45% |
| P3, all three at once, a directory each, on the 80% share | 210.1 | 80% | 85% |

P3 against S3 is 53% faster under the same limit. Its machine mean of 80% is the release's 19
processors at full use plus the desktop on the other 5.

The first release after this change builds the two new macOS directories cold: C3 and C4 measured
335.7 s and 255.2 s. Every release after that is the warm case. The old `target/aarch64-apple-darwin`
and `target/x86_64-apple-darwin` folders are no longer written; deleting them is left to a person.

`tools/prune-target.ps1` now looks three levels below `target` for profile folders, so it prunes
`target/clippy/debug`, `target/waiting/debug` and `target/mac-<triple>/<triple>/release`. A case for
the deeper folder is in `tools/prune-target.test.ps1`.

### 5.4 Window suite receipts shared between checkouts, and a version bump is not a change

`crates/unluminous-app/tests/common/receipt.rs` writes each receipt into the main checkout's
`_agent_output/window-suite/`, found through `git rev-parse --git-common-dir`, and
`tools/window-suite.mjs --check` reads them from there. The check is unchanged otherwise: a receipt
counts only when its commit is one HEAD is built on and nothing a window is built from has changed
since. A receipt from another ticket's unmerged branch is therefore reported stale, never trusted.

`tools/window-suite.mjs` no longer counts `Cargo.toml` or `Cargo.lock` as changed when the only
difference since the receipt's commit is the workspace version. No accepted picture draws the version:
if one did, every release would have needed new pictures accepted.

Together these mean a ticket runs the window suite once, in its worktree, after committing, and the
release after the merge accepts it.

### 5.5 Where a release's time went

`Write-Step` in `tools/release.ps1` records when each step starts. At the end, and also when a
release stops after `-SkipPublish` or with destinations missing, it prints each step's seconds and
writes `_agent_output/release-timings/<version>.json` with the steps, the total, the processor share
and `CARGO_BUILD_JOBS`.

### 5.6 Line tables only in a test build

`Cargo.toml` sets `[profile.test] debug = "line-tables-only"`. A verbose build with only
`CARGO_PROFILE_TEST_DEBUG` set showed the dependencies of a test build getting
`-C debuginfo=line-tables-only` as well, so the setting reaches everything a test binary links.

| measurement | full debug information | line tables only |
|---|---:|---:|
| rebuild of the window suite after a one line change, 80% share | 32.6 s (W3) | 15.3 s (W5) |
| `window_and_chrome` test executable | 125.0 MB | 111.0 MB |
| `window_and_chrome` PDB | 699.1 MB | 291.7 MB |
| a whole build and run of the suite after a one line change, 80% share | 655.8 s (W3) | 704.0 s (W5) |
| W4, a whole first build and run of the suite, 80% share | | 569.5 s, every test passed |

The whole run in W5 was longer than in W3 although its build was 17 s shorter: the suite's run time
moves by more than that between runs (W2 and W3, identical apart from the share, ran 590 s and
623 s), so the rebuild and the disk are what this setting is measured by.

A panic still names its file and line. A debugger attached to a test binary loses local variables.
`[profile.dev]` is unchanged, so a dev build of the application debugs as before; a dev build and a
test build no longer share dependency artifacts.

## 6. Considered and not done

- **A job object rate cap.** Does not hold with the Store `pwsh` (§4).
- **Below normal priority.** Starved builds behind other tickets' builds in task-2205.
- **The release builds in one target directory at once.** P1: 507 s, slower than in a row.
- **Running the gate and the release builds at the same time.** The builds need the new version and
  the version is only written after the gate passes, so a failing gate would have to undo a version
  bump and stop half finished builds. The gain is bounded by the gate's own length, which §5.2 makes
  zero for most releases.
- **Publishing to GitHub while Apple notarises, and attaching the macOS zip afterwards.** A release
  then goes out without macOS whenever notarisation fails after the tag is pushed. The tag is the step
  that cannot be quietly taken back, so macOS stays finished before it.
- **cargo-nextest and `universal2-apple-darwin`.** §4.
- **`debug = "line-tables-only"` for the dev profile.** Inillucent task-2114 did this. Here it would
  also take the local variables out of a debugger attached to a dev build of the application, so it
  is set for the test profile only (§5.6).

## 7. Measured after implementation

AFTER_RESULTS

## 8. Recommendations not built here

- **Fix the window tests that fail on unchanged code.** §3.3 lists them. A failing binary deletes
  its receipt, so each flaky failure costs a ticket a rerun, and an agent that does not know they
  are flaky spends time looking for a cause in its own change. `.claude/repo-plan.md` now names them.

- **Make the Jupyter kernel tests faster.** They are the longest binary outside the window suite and
  the one most slowed by other load. Sharing one started kernel between the tests that only execute
  cells, rather than starting one per test, would cut most of their time; it changes what each test
  isolates, so it is a decision for the notebook tests' owner.
- **Profile the slowest window test binaries.** `plugins_board_and_chat` (139 s), `debugging`
  (89 s) and `notebooks` (73 s) are 40% of the suite's run time in W1.
- **Delete `target/aarch64-apple-darwin` and `target/x86_64-apple-darwin` in the main checkout**
  once a release has built into the new folders.

## 9. Jargon

| term | meaning here |
|---|---|
| affinity mask | the set of logical processors a process may run on, one bit each |
| share | the fraction of the logical processors the release's mask keeps |
| unit | clippy, or the tests, of one workspace crate |
| receipt | a line recording that a unit passed on content with a given hash |
| lane | a cargo command sequence with its own target directory, run beside the others |
| warm, cold | a target directory that already holds the previous build, or an empty one |
