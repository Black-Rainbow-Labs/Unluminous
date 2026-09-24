# Working this repository beside another agent

What the next agent needs to know before it starts, and which nothing in the code says. Add to it
when you learn something; it is checked in, so it travels with the commit that taught you.

## A worktree checks markdown out with carriage returns, and one test reads a markdown file

`.gitattributes` says `* text=auto` and this machine has `core.autocrlf=true`, so a **fresh** git
worktree gets CRLF in every text file while `C:\jason\dev\unluminous` itself still has LF — it was
checked out before that combination. One test compares a string against the bytes of a file rather
than against a line-normalised reading of it:

```
unluminous-cli documentation::every_area_heading_appears_once
  "## Commands with no area\n" appears 0 times in unluminous-cli/docs/commands.md
```

It fails in **every** worktree and passes in the main checkout, and it has nothing to do with
whatever you changed. Two things follow:

- Do not go looking for a cause in your own work, and do not "fix" `commands.md` — rewriting its
  line endings in a worktree would commit 3,794 changed lines.
- **Run `pwsh tools/release.ps1` from `C:\jason\dev\unluminous`, after merging**, not from the
  worktree. The release runs `cargo test --workspace --exclude unluminous-app`, which includes that
  test, so a release cut from a worktree is refused by a failure that is not real.

## The window suite writes a receipt, and the receipt names a commit

`tools/window-suite.mjs --check` is what both release scripts ask before they publish, and it
refuses while a binary's receipt names a commit whose drawing code has moved since. So the order is
**commit first, then run the suite**:

```sh
git -C <worktree> commit -m "task-N: ..."
cargo test -p unluminous-app --test '*' --no-fail-fast    # from the worktree; needs a graphics card
node tools/window-suite.mjs --check
```

Running the suite before the commit leaves a receipt at the previous commit, and the release then
tells you every file that has changed since it.

## Window test binaries from every checkout take turns

The window tests' fixtures are folders under `%TEMP%` with fixed names, because the name
`unluminous-screenshot-folder` is drawn in hundreds of accepted pictures, and each binary clears
them before writing. Two worktrees running the suite at once used to rewrite each other's folders
mid-test: `task-2100` measured the picture showing `Reloaded ...readme.md` in the status bar and the
explorer row moved. `tests/common/turn.rs` makes each binary hold `%TEMP%\unluminous-window-tests.lock`
for its lifetime, so a suite running beside another ticket's is slower rather than wrong. A binary
that is waiting prints `waiting for another window test binary to finish: process N running ...`.
That is not a hang. It is somebody else's suite, and it names the process.

## Two builds of this repository cannot share a target directory

Each worktree gets its own under `D:/agent-worktrees/cargo-target/...`, written by a
`.cargo/config.toml` the backend puts inside the worktree. Leave that file alone and do not commit
it. Your first build is cold, which is the price of not waiting for the other ticket.

## `CC` is set in an agent terminal, and it breaks the C build

`libsqlite3-sys` compiles `sqlite3.c`. With `CC` pointing straight at `cl.exe`, `cc-rs` uses it and
skips setting `INCLUDE`, so the build stops at `fatal error C1034: stdarg.h: no include path set`.
Run cargo with it unset:

```sh
env -u CC -u CXX cargo build --release --manifest-path <worktree>/Cargo.toml -p unluminous-app
```

## Driving a real window without touching the person's own Unluminous

`tools/drive-a-window.ps1` starts one without taking the focus, and `UNLUMINOUS_APP` points it at
`target/release` so nothing has to be installed. Point `APPDATA` at a scratch folder as well: the
settings, the plugins' own folders and every conversation the Agent-Chat plugin has ever kept live
under `%APPDATA%\Unluminous`, so a fixture written there is a fixture written into Jason's real
history.

**And check which endpoint the chat is actually pointed at before sending anything.** The `local`
row that ships is `http://127.0.0.1:8080/v1/chat/completions`, which is the machine's own llama
server — a shared resource. A plugin settings file needs `providers = N` at the top of it or every
`provider.N.*` row in it is ignored and the three shipped rows are used instead, silently.
