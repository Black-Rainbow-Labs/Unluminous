# task-2295: a release that does not have to be run twice

Jason, after task-2294's release had to be started three times: *"We keep having build issues and have to rerun
for various issues. Look at our previous cards, create a TDD, and resolve the build issues."*

## What was looked at

- The comments on the last 70 Unluminous tickets on the Tasks board.
- Every release log kept under `_agent_output/` in the main checkout and in the retired worktrees.
- `git log` for commits whose only purpose was to get a release through: `rustfmt on the ticket's own code`,
  `the changelog`, clippy fixes.
- task-2294's own release, which failed twice before it went out.

## What made a release run again

Ordered by how often. The line numbers are `tools/release.ps1` before this ticket.

| # | What failed | Where | How often | What it cost |
|---|---|---|---|---|
| 1 | `cargo fmt --all -- --check` | the gate, line 448 | about 8 times (2003, 2063, 2105, 2269, 2276 to 2293) | a format commit, then the whole window suite again, about 25 minutes |
| 2 | `node tools\changelog.mjs --check` | the gate, line 451 | at least 6 times (1994, 2105, 2281, 2293, 2294) | a changelog commit and a second start |
| 3 | the window suite receipt check | the gate, line 476 | at least 6 times | the window suite again |
| 4 | clippy or a unit test | `tools/test.mjs`, line 469 | about 6 times | a fix, then the window suite again |
| 5 | the installer exits with 5 | `installer\windows\build.ps1`, line 358 | task-2294, and every release made while an agent is connected | a third start, after stopping processes by hand |
| 6 | the release left `Cargo.toml` and `Cargo.lock` changed after failing | any step after line 545 | task-2294 | the next start refused the checkout as dirty |
| 7 | macOS left out of a release from a worktree | the preflight, line 527 | task-2207 (0.64.4 went out Windows only), task-2294 by hand | a second release, or copying files by hand first |
| 8 | the main branch left behind a release cut from a ticket's branch | line 651 pushes only the branch | task-2200, task-2294 | a merge and a push by hand |
| 9 | a site publish failing after the GitHub releases were made | line 726 | 2 times (2261, 2274) | the printed command, run by hand |

### 1. Formatting is found after the window suite, and fixing it makes the window suite stale

The gate checks formatting, and the gate runs at release time. By then the ticket has run the window suite.
`cargo fmt` rewrites `.rs` files, and `tools/window-suite.mjs` counts any changed `.rs` file as something a
window is built from, so the receipt no longer answers for HEAD and the suite runs again. Rustfmt changes only
whitespace and line breaks, so nothing a window draws can have moved. The check cannot tell that.

### 2. The changelog is checked and then rewritten

The gate refuses a `CHANGELOG.md` that does not match the git history. Every commit a ticket makes adds a line
to that history, so the file is out of date after every ticket unless the agent remembers to run
`node tools/changelog.mjs` and commit the result. Five lines below the gate, at "Writing CHANGELOG.md", the
release rewrites the whole file from the history and commits it with the version. So the check refuses a
release over a file the release is about to write anyway.

### 3 and 4. A real failure, found late

These are real faults: a stale receipt, a clippy lint, a failing test. The cost comes from the order. When
clippy fails at release time, after the window suite, the fix changes code and the suite runs again.
`tools/test.mjs` already runs clippy and the tests for the crates that changed, and the project's CLAUDE.md says
to run it after committing and before the window suite. Formatting was not in it, which is cause 1.

The receipts themselves were fixed by earlier tickets: 2240 moved them to the main checkout, 2285 gave each
commit its own file, and 2240 stopped a version bump from making them stale.

### 5. An agent's `mcp serve` holds `unluminous-cli.exe`, and setup aborts

Every coding agent connected to Unluminous runs `unluminous-cli mcp serve` from the install folder, and the
code index runs `unluminous-cli search serve`. Neither has a window. Inno Setup's Restart Manager finds them,
cannot close them, and a setup run with `/SUPPRESSMSGBOXES` answers its "unable to close all applications"
question with Abort. Reproduced on this machine with one `mcp serve` and the 0.99.1 installer:

```
RestartManager found an application using one of our files: unluminous-cli.exe
Defaulting to Abort for suppressed message box (Abort/Retry/Ignore):
  Setup was unable to automatically close all applications.
exit 5
```

`installer\windows\build.ps1` closes a running `unluminous.exe` before it installs, and nothing closes
`unluminous-cli.exe`. Any release made while any agent session is open hits this. The in app update
(`unluminous-cli --apply-update`) runs the same setup with the same switches, so an update fails the same way
for anybody with an agent connected.

### 6. A failed release leaves the version changed

The release takes the newest `inillucent-driver` into `Cargo.lock` before the gate, sets the version in
`Cargo.toml` before the build, and rewrites `CHANGELOG.md` before the commit. A failure anywhere between the
first of those and the commit leaves those files changed. The next start refuses the checkout as dirty, and
the person has to work out which files to put back.

### 7. A worktree has no macOS signing files

`installer/macos/notarize.env` and `tools/cross/bin` are ignored by git, so a new worktree has neither. The
preflight then fails and the release continues without macOS, printing one yellow line. CLAUDE.md now tells an
agent to copy the files first, which works when the agent reads that paragraph.

### 8. A release from a ticket's branch pushes that branch

`git push origin $branch` pushes `task-NNNN`. The version commit and the tag reach GitHub, but `main` on GitHub
and in the main checkout stay one commit behind until somebody fast forwards them by hand.

### 9. Site publishing

Two failures in 70 tickets, each with a different cause, both fixed in the site repository, and the release
already prints the one command that finishes the job. This ticket does not change it.

## The design

### Formatting: checked first, and a formatting commit no longer makes the window suite stale

- `tools/test.mjs` runs `cargo fmt --all -- --check` before anything else, and stops with the command that
  fixes it. That is the step an agent runs after committing and before the window suite, so unformatted code
  is found in about a second, before 25 minutes are spent.
- `tools/window-suite.mjs` treats a `.rs` file as unchanged when rustfmt applied to the file at the receipt's
  commit gives exactly the file at HEAD. It runs the workspace's own `rustfmt.toml` with the workspace edition.
  That is a precise test: a file that changed in any other way, even by one character, is still counted. So
  when the release's format check does refuse, `cargo fmt --all`, a commit and a second start are enough. The
  window suite does not run again.
- The release keeps its own format check, as the last line of defence.

### The changelog check is taken out of the gate

The release writes `CHANGELOG.md` from the history at "Writing CHANGELOG.md" and commits it with the version.
That step is the guarantee. A ticket no longer needs a "the changelog" commit.

### The installer moves a running `unluminous-cli.exe` aside

`PrepareToInstall` in `installer/windows/unluminous.iss` runs before the Restart Manager looks at the files. It
deletes `unluminous-cli.exe`, and when that fails because something is running it, renames it to
`unluminous-cli.exe.<time>.old`. Windows allows a running program's file to be renamed, though not replaced or
deleted. The new file is then written in its place, the agents' servers keep running the old copy until their
agent restarts them, and the Restart Manager finds nothing to close. Renamed copies that are no longer running
are deleted the next time setup runs, and uninstalling deletes any that are left.

`unluminous.exe` is left to the Restart Manager as before, because a window can be asked to close and should
be, so the person is not left on the old build.

This fixes the release, the in app update and a manual install with one change, in the one place they share.

### A failed release puts back what it changed

From the moment the release first changes a file (`cargo update`) to the moment it commits, a `trap` restores
`Cargo.toml`, `Cargo.lock` and `CHANGELOG.md` with `git checkout --` and stops the macOS build it started, then
lets the error through. Only those three files are touched, because they are the only files the release
writes. The trap does nothing for a failure before the first write, so a person's own uncommitted change to
`Cargo.toml` is never thrown away. It does nothing after the commit either, because by then the files are
the release's own commit.

### A release from a worktree fetches its own macOS files, and missing macOS stops it

Before the preflight, when `installer/macos/notarize.env` is missing and the main checkout has one, the release
copies it. When `tools/cross/bin` has no `zig.exe`, it runs `tools/cross/fetch-toolchain.ps1`. The main checkout
is found the way the receipts find it: the parent of `git rev-parse --git-common-dir`.

If the preflight still fails, the release stops before it changes anything and says why, unless `-SkipMacos`
was given. Jason, on task-2207: *"release requires macos."*

### A release from another branch brings main with it

After the version commit and the tag are pushed, a release cut on a branch other than `main` also:

1. pushes `HEAD` to `origin/main`. Git refuses this unless it is a fast forward, so a `main` that has moved on
   is never overwritten.
2. fast forwards the main checkout's `main` when that checkout is on `main`. `git merge --ff-only` leaves
   uncommitted files there alone unless the merge would change them, in which case it refuses.

A refusal at either step is printed with the command to run, and the release carries on, because the tag is
already published.

## What this does not change

- Clippy and test failures are real faults. The fix is to find them earlier, which `tools/test.mjs` does, and
  now with formatting in it.
- Timing tests that fail under load and pass alone (task-2232, task-2258) need their own ticket per test.
- Site publishing (cause 9).

## How each fix is verified

| Fix | Check |
|---|---|
| installer | build the setup, hold `unluminous-cli.exe` with a running `mcp serve`, install silently with the release's switches: exit 0, the old copy renamed, the new one in place. Run setup again once nothing holds the old copy: it is deleted. |
| trap | a release started with a test that fails after `cargo update` and `Set-Version`: the checkout is clean afterwards. |
| changelog | the gate no longer runs `changelog.mjs --check`. The release still writes and commits the file. |
| formatting | `tools/test.mjs` on an unformatted file stops in seconds with the command. `tools/window-suite.mjs --check` accepts receipts across a commit that only ran `cargo fmt`, and refuses one that also changed a character. |
| worktree macOS | remove `notarize.env` and `tools/cross/bin` from a worktree, run `release.ps1 -WhatIf` and the preflight step: both come back and the preflight says ready. |
| main | this ticket's own release, cut from `task-2295`: `origin/main` and the main checkout end at the version commit with nothing done by hand. |
