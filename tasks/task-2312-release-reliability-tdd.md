# task-2312: releases fail early, retry what is transient, and carry on after a failure

Jason, on task-2308: *"we cant have releases take this long ... we need to ensure that things dont take 20
minutes before we find out they break. we also need smarter retries. do we need to start from scratch when we
run into an issue?"*

task-2240 and task-2295 both worked on releases. This ticket measured what still goes wrong. Then it changed
`tools/release.ps1` so that:

- a release refuses in seconds when it is going to be refused at all;
- it runs again what failed for a reason that goes away;
- it never starts from the beginning after a failure.

## 1. What went wrong, measured

The data comes from every release transcript between 2026-09-19 and 2026-10-20, versions 0.53.0 to 1.3.0.
There were 123 starts that did not finish, across 51 tickets. Each one is a row in
`_agent_output/task-2312-release/runs-table.md`. Those starts ran 649 minutes before they stopped. The time
from each failure to the next start adds about 606 minutes more.

| Cause | Starts | Minutes before failing | Could it be known before building | Goes away on a second run |
|---|---|---|---|---|
| `changelog.mjs --check` | 36 | 39 | yes | removed by task-2295 |
| Site refused, "releases has no installer" | 17 | 174 | yes | fixed by task-2268 |
| `cargo fmt --check` | 15 | 3, then a window suite rerun each | yes | moved into `test.mjs` by task-2295 |
| macOS build, `os error 32` | 13 | 96, from 4.5 to 13.8 each | no | yes, 13 of 13 |
| Site's own check of the edge after deploying | 7 | 74 | no | yes, every recorded rerun |
| Window suite receipt refused | 6 | 24 | yes, in seconds | no, the suite has to run |
| Tests, clippy | 8 | 32 | yes | no |
| Public source push refused | 2 | 26 | yes | no |

Since task-2295 shipped, seven tickets have started a release. Only one of them finished at the first try.
Four of the six that failed hit the macOS file lock. The other two failed on a `CC` variable and on a site
build error.

Three things made each failure cost so much:

1. **The cheap checks ran late.** The window suite receipt was checked after the test gate. The public push
   was found to be impossible after both GitHub releases existed.
2. **The receipt check was not cheap.** `tools/window-suite.mjs --check` took **4 min 54 s** on this machine.
   That is most of the "Running the suite" step, which has taken 148 to 289 s in every release since 10-18.
   The check judged every receipt in the folder (505 files, about 30 for each of the 16 binaries). Each old
   receipt meant a `git diff` of hundreds of `.rs` files. Then every one of those files went through two
   `git show` calls and `rustfmt` to ask whether only its formatting had moved.
3. **Every failure started again from the beginning.** That meant the suite, the Windows build (about 6
   minutes after a version bump), the macOS build and notarisation. A failure after the tag was finished by
   hand, as on 1.3.0.

The macOS build's failure was also seen late. Its exit code was read only after the Windows installer had been
built and installed, even though its error is the first line of its log.

## 2. What changed

### 2.1 The receipt check answers in half a second

`tools/window-suite.mjs` tries each binary's receipts newest first and stops at the first one that answers for
HEAD. When none answers, the newest receipt's reason is the one reported, as before. The formatting question
is asked file by file only while a receipt's diff holds 40 `.rs` files or fewer (`FORMATTING_PASS_FILES`). The
case it exists for is a `cargo fmt` commit after the suite ran, which touches a handful of files. Past 40
files, the first file whose change is real decides. The answers are the same as before.

Measured: **4 min 54 s before, 0.52 s after**, with the same 16 binaries passing.

### 2.2 Everything that can refuse a release is asked before anything is built

The new step "Checking what can refuse this release before anything is built" runs after the working tree
check. It asks, in this order:

1. `tools/release-steps.test.ps1`, which checks the pieces that let a release be started again. It takes half
   a second.
2. `git fetch origin`. A tag `v<next>` that already exists here or on origin is refused.
3. **`origin/main` is merged into the branch** when the branch does not already contain it. A merge that does
   not go cleanly is aborted, and the release is refused with the instruction. The suite then runs on the
   merge, and the merge is what is released. This is what 1.3.0's refused public push needed: task-2310
   reached `main` while that release ran.
4. The GitHub CLI, the token, the public repository and the two site scripts. These checks were already
   before the build. They are now before the suite too.
5. The macOS preflight.
6. The window suite receipt.
7. **`publish-open-source.mjs --ref HEAD --check-push`**, started in the background. It rewrites the history
   as a publish would, which takes about 40 seconds. Then it asks whether the public `main` is in the history
   being published, which is what makes the push a fast forward. The release waits for its answer after the
   test gate and before the version is set.

### 2.3 Transient failures are run again

- **The macOS build retries each architecture on its own.** `installer/macos/build-on-windows.ps1` writes each
  architecture's cargo-zigbuild output to `_agent_output/release-logs/zigbuild-<triple>.log`. It starts the
  second architecture only after the first has printed a line from cargo, waiting at most 30 seconds. This
  makes the second start wait until cargo-zigbuild has written its shared files under
  `%LOCALAPPDATA%\cargo-zigbuild`, which is what the two starts collided on. An architecture that fails with
  `os error 32` or "being used by another process" is started again, up to three more times. cargo keeps
  what the first try built.
- **The release watches both builds together.** Every two seconds it checks the macOS build and the Windows
  build:
  - A macOS failure that `Test-TransientMacosFailure` recognises is restarted at once, up to twice. It
    recognises a held file and a network error on the way to the notary service.
  - Any other macOS failure is printed straight away. The Windows build carries on so that the next start
    can reuse it.
  - A Windows failure stops the macOS build and the release.
- **Every step that reaches the network is retried.** These are `git fetch`, `cargo update`, both pushes,
  both GitHub releases and the public source. Each is tried three times, waiting 10 s and then 20 s
  (`Invoke-WithRetry`). Each site is published twice before the release stops. All of these steps can run
  twice without harm:
  - A push of the same commit changes nothing.
  - `Publish-GitHubRelease` attaches the assets a release lacks, and leaves a complete release alone.
  - The public publish pushes no tag the public repository already has. It leaves the public `main` alone when
    `main` already contains the history.

### 2.4 A release carries on from where it stopped

`_agent_output/release-state/<version>.json` records each release. It holds:

- the commit it was built from;
- the SHA-256 of `Cargo.lock` once the newest Inillucent was taken;
- each installer it built, with that file's SHA-256;
- the name of each step after the tag that finished: `committed`, `tagged`, `pushed`, `private`,
  `public-source`, `public`, `site:<name>`, and `done` when every destination checked out.

**A failure before the commit.** The trap still puts `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md` back. The
installers stay in `releases\`. Starting again computes the same version and the same commit:

- the tests are skipped by `test.mjs` from their receipts;
- the receipt check takes half a second;
- an installer is reused when the record names it and its file still has the same hash.

So a macOS failure costs a macOS build on the next start, and nothing else.

**A failure after the tag.** HEAD is then the release's own commit, tagged with the version `Cargo.toml`
holds. When the record of that version lacks `done`, starting the release again carries on with that version.
It does not cut a new one. Each publishing step the record names is skipped, and the rest run. `-Resume` does
the same for a tagged release that has no record, and every step is then asked of its destination. The trap
says which of the two happens.

## 3. How it was tested

- `tools/release-steps.test.ps1` (19 checks, 0.5 s). It covers:
  - a retry that succeeds on its second try;
  - a retry that gives up after its tries;
  - the record of steps written and read back;
  - an installer reused only while its hash is unchanged;
  - `Publish-GitHubRelease` against a stand-in `gh`, which creates a missing release, attaches only the
    missing asset, and leaves a complete release alone;
  - `Test-TransientMacosFailure` on a held file, a compile error and a notary network error.
- `publish-open-source.mjs --check-push` against `HEAD` passes, and against `v1.2.2` it refuses with exit
  code 2. `--push --ref v1.3.0` now pushes nothing, because the public repository already has it.
- A **real resume of 1.3.0**, in a temporary worktree at the tag, with a record saying the push and the sites
  were done. It found both GitHub releases complete, pushed nothing to the public repository, checked every
  destination as `ok`, and recorded `done`, in 66 s. `-WhatIf` and `-WhatIf -Resume` say the right thing in
  three cases: at the tag with no record, at the tag with a record, and off a tag.
- `build-on-windows.ps1 -CliOnly` built both architectures through the new loop.
- This ticket's own release is the test of the whole path: the merge, the background check, both builds
  watched together, and the record.

## 4. Left out

- **`tools/release.sh`** is unchanged apart from the faster receipt check, which it shares. No release since
  09-19 was run with it.
- **The site repository's build** is not run before the release. A syntax error there (task-2301) is found at
  the site step. After the tag, the release then carries on from that step once the site is fixed, so the
  cost is the site build alone.
- **The Jupyter kernel tests under load** (two starts) are a question about the machine's Python, not about
  the release.
