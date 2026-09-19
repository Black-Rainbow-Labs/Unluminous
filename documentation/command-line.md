# The command line, and the channel underneath it

`unluminous-cli` drives a **running** Unluminous.

```sh
unluminous-cli launch .                                   # start an Unluminous here and wait for it
unluminous-cli tab open README.md                         # open a file
unluminous-cli editor view preview                        # look at its Markdown preview
unluminous-cli browser open examples/site/index.html      # render a local web page
unluminous-cli terminal send cargo test                   # run something in the terminal
unluminous-cli terminal read --wait-for "test result"     # wait for it, and read what it said
unluminous-cli window screenshot shot.png                 # a real picture of the window
```

**214 commands across 23 areas**, and `--json` makes every answer machine-readable.
`unluminous-cli/docs/commands.md` is the reference, written to be handed to an AI agent whole;
`unluminous-cli commands --json` is the same thing as data.

It is **not a second way of doing things**. `run_cli` is to the command line what `run_action` is to
the menus, and wherever there is already a way in it uses it — so a thing done from the command line
and the same thing done by hand are the same thing.

## Three rules, all of them tests rather than promises

**A menu entry needs nothing at all.** `unluminous-cli action list` is built by walking the real menus,
so an entry added tomorrow can be run from the command line tomorrow. A test fails the day a menu
entry has no name.

**Anything with no menu entry** is a row in `unluminous-cli/src/catalogue.rs` and an arm in
`app/cli.rs`. The catalogue is one list in a crate both halves depend on, so a command the client
accepts is a command the window knows.

**Documentation is a test.** One fails while a command has no section in
`unluminous-cli/docs/commands.md`, while a section's usage line is out of date, or while a section
describes a command that no longer exists. A second parses every example in the catalogue and checks
it runs the command it is filed under, because the examples are what an agent copies.

`cargo run -p unluminous-cli --example reference` writes the documentation from the catalogue.

## The flags every command takes

| | |
|---|---|
| `--instance <pid\|port\|path>` | which window to talk to when several are running |
| `--json` | print the whole reply as JSON. This is what a program or an agent should always pass |
| `--quiet` | print nothing when it worked. The exit code still says whether it did |
| `--timeout <ms>` | how long to wait for an answer, 15000 by default |
| `--dry-run` | print the command and the arguments that would be sent, and send nothing. Needs no running Unluminous |
| `--no-color` | never colour the output. `NO_COLOR` in the environment does the same |
| `--help` | print help for the command, or for the whole client |

**An explicit `timeout` is used as given unless the command itself waits**, and the catalogue decides
which — because it is already the list the client parses against. It used to be a floor rather than a
value, so a caller could raise the deadline and never lower it.

## The channel

A socket on `127.0.0.1`, a port the operating system chose, one JSON object a line, and a per-run
token in an instance file under the person's own settings folder:

```text
-> {"token":"4f1a...","command":"tab.open","arguments":{"path":"README.md"}}
<- {"ok":true,"command":"tab.open","message":"Opened README.md","result":{"tab":2}}
```

A loopback socket rather than a Unix domain socket or a named pipe, because it is the same `std::net`
on both platforms — and because any language with a socket and a JSON library can drive Unluminous in
three lines, which makes `unluminous-cli` the comfortable way in rather than the only one. Nothing is
ever bound to anything but `127.0.0.1`, and there is a test for that.

**The token is what stops a page in a browser** — which can post to a loopback port and cannot read a
file — from driving somebody's editor. It is not protection against a program already running as them,
and nothing on a desktop is. `unluminous --control off` closes the channel altogether.

**The sentence in `message` is written by the window** rather than by the client, because the window is
the only one that knows what actually happened: which tab the file landed in, what the setting was
before it changed, how many results a search found.

`unluminous-cli/docs/protocol.md` is what a client in another language needs.

## Two refusals that keep it honest

**A value the command has no name for is refused.** The usage lines spell every flag `--permanent`, so
that is what a caller writing a request from the catalogue sends — and an `arguments` object is
written by hand by anything that is not `unluminous-cli`, which the MCP server always is. Those keys
used to be accepted and dropped: `tab open --permanent` left the tab transient so the next file
replaced it, and `run output --tail 40` returned the whole screen. Both replied that they had worked.
The leading dashes come off, and a key no argument and no flag of that command is named by is a
**usage refusal naming what the command does take**.

**A mistyped flag is refused rather than quietly treated as text**, because a command that did the
wrong thing without saying so is worse than one that did nothing.

## The disk is re-checked at the moment of use

Unluminous watches nothing, and a tab is owned by its document — which is right while Unluminous is
the only writer and wrong the moment something else writes. A tab went on showing text no longer in
the file and `editor text` answered with it, and `explorer files` did not list a file plainly on the
disk.

So the `editor` commands read the file again first when it has changed underneath *and* the tab has no
unsaved changes — a modified time and a length rather than a hash — and `explorer files` and
`explorer tree` walk the folder before answering. Unsaved changes are never touched: those are the
person's and losing them has no undo, which is what `tab reload --discard` is for.

## Commands that wait

Four are asked on one frame and answered on a later one: a screenshot, because the picture of a frame
arrives after that frame has been painted; a terminal read waiting for a shell; a search still
running; a git operation still running. Each keeps its request until it is ready, and every one of
them takes a timeout, because a command that could wait for ever is a script that hangs.

**A request the caller gave up on is thrown away rather than applied later.** The caller says how long
it will wait, because a client that gives up says nothing — it stops reading — and without it the
command ran whenever the window next drew, long after the caller had been told it failed. Only a
request **still on the queue** is abandoned: a command the window has taken owns its own wait.

## The window has to be awake to answer

The window is answered at the **top of a frame**, before anything is drawn, which is what makes a
screenshot taken straight after a command show what the command did. An idle window draws twice a
second anyway, so a command can wait up to half a second for the frame that answers it. That is a
wait, not a failure.

**One repaint request is not a wake**, and that was measured rather than guessed: an idle window
answered nothing for sixty-five seconds while sleeping at no processor use, with four connection
threads parked on their deadlines. Two mechanisms lose a single repaint request, both in code
Unluminous does not own — a request made while the window is already repainting sends nothing at all,
and a request whose pass number is more than one behind is discarded as outdated, which for a repaint
means it already happened and for a wake meaning *there is work on a queue* does not.

So **the wake repeats while the request is on the queue and stops the moment the window takes it**. A
command the window is *holding* — `terminal read --wait-for`, a git action — costs no wakes at all for
the minutes it may wait.

**And repeating it is still not enough**, which a later measurement showed: three hundred and
fifty-nine seconds with no frame and four requests queued, which is seven thousand repaint requests
lost rather than one. What recovered it every time was activating the window from outside the process
— so the thing that gets through is an event the operating system puts on the main run loop rather
than a repaint request. Once nothing has been drawn for half a second the wake becomes one of those:
on macOS a block posted to the main dispatch queue, which the main run loop is a consumer of, so the
post wakes the loop *and* the closure runs on the main thread. It is an escalation rather than the
ordinary path, and it is only ever reached by a window that has stopped drawing.

A timeout reply says how long since a frame was drawn and how many requests are queued, so a caller
can tell a busy window from a stopped one without running a profiler on the process.

## The MCP server on top of it

An AI client that speaks the Model Context Protocol is given Unluminous's commands as **tools**, so it
does not have to be handed a document first and does not have to know it may shell out to a program it
has never heard of. [For AI agents](for-ai-agents.md) is the whole of it; three things belong here.

**The tools are generated from the catalogue**, which is the fourth rule of the three above.

**It is a client of the channel rather than a peer of it.** A tool call becomes exactly the request
`unluminous-cli` would have sent, down the same socket with the same token, so `run_cli` stays the one
place a command becomes a change. It also means one server drives every open window, which is why two
Unluminous windows sharing one `mcp.port` is the behaviour rather than a collision.

**The server holds no session, and that is deliberate.** The 2025 revision of the protocol has an
`initialize` handshake and an optional session id; the 2026 one deleted both. A server that never
*requires* `initialize`, issues no session id and echoes back whatever version the client named
answers both with one code path. Do not add a version switch, and do not add a session.

**The HTTP endpoint is off by default and should stay that way.** The stdio server an agent launches
needs no port and lives as long as the conversation; a fixed open port will run `terminal send` for
anything that can reach it. The browser case — the one thing a loopback port really has to defend
against — is closed by refusing a non-loopback `Origin` and a cross-site `Sec-Fetch-Site`, which is
what the specification asks for and what the token does for the older channel.
