# The agent panes

Three panes a plugin contributes: **Agent-Chat**, a chat beside your work; **Agent-Tasks**, a board
whose tickets are worked by an agent in a terminal Unluminous owns; and **Database**, which has a page
of its own in [the Database plugin in pictures](database.md).

Each is an ordinary `ui` plugin — a folder with a manifest and an icon — so switching one off in
`Settings -> Plugins` withdraws its pane, its menu and its Settings page on the next frame.

## Agent-Chat

A chat pane that **runs the `claude` or `codex` command line already installed on this machine**. That
is the better half of the feature rather than a cheaper one, and the four reasons are worth keeping:

- **Unluminous holds no key at all.** Nothing to put in a settings file, nothing to read out of an
  environment variable, nothing to redact out of an error message. `claude` and `codex` hold their own
  credentials, which is their business and not this window's.
- **The agent brings its own tools**, its own sandbox and its own permission model — so the question
  of what a model may do to this machine is answered by a program the person already trusts with it.
  Unluminous runs **none** of an agent's tool calls; what it does with one is *show* it. The other
  side of that is real: an agent's own MCP servers start in the folder it was started in, and one
  configured with a relative path writes there. On the machine these pictures
  were taken on, `claude` is configured with `inillucent-mcp --db app.rdb --root .`, so a turn in
  this pane leaves an `app.rdb` beside the project's own files. That is the person's own tool doing
  what they configured it to do, in the folder they pointed the window at.
- **It reads the project.** Started in the folder the window has open, `claude` finds that project's
  `CLAUDE.md` and `codex` its `AGENTS.md`, so the answer is about the code in front of you without a
  word of it being uploaded by Unluminous.
- **The conversation is the agent's.** A second question is `--resume <session>` carrying one turn's
  words, not the whole transcript sent again, so the context the agent built is the context it keeps.

A row can be pointed at an address instead. Five wire shapes are read, and all five are read into the
**same** values, so the component that draws a message has never heard of any of them:

| | |
|---|---|
| `claude-cli` | Claude Code's `--output-format stream-json`, which nests the Anthropic wire verbatim inside an envelope — so the decoder that reads that API reads this one level down, and thinking, tool calls and token deltas needed no new code |
| `codex-cli` | a thread of **items** that are started, updated and completed, where a shell command the agent ran is an item beside the words it said. Its items are not deltas, so only the part not yet shown is passed on |
| `messages` | Anthropic's `/v1/messages`: named events, indexed content blocks, a system prompt that is a field rather than a message |
| `chat` | OpenAI's `/v1/chat/completions`, which is what llama.cpp, LM Studio, Ollama and every gateway speak |
| `responses` | the shape the Codex models are served on: a list of items rather than of messages |

A configuration naming a shape this version has not got is **refused with the list**, which is the
rule every other named-thing key in a manifest keeps.

### What an agent may do

`chat.permission` is `read`, `edit` or `full`, in one vocabulary rather than each agent's own. It is a
setting rather than a prompt because an agent run with `--print` cannot stop and ask.

**The two are not the same strength and the page says so.** Codex takes an operating system sandbox,
so at `read` its process cannot write whatever it decides. Claude Code takes a permission *mode*, so
at `read` it refuses any tool that would change something — its own policy rather than the machine's,
with its hooks, plugins and MCP servers still starting. One value, two guarantees, and pretending
otherwise would be the pane promising something it does not enforce.

### The tools a model may call

For an **address**, the tools are Unluminous's own commands, generated from the same catalogue the
command line reads — never a second list. A call goes back through the one place a command turns into
a change, so a tool call and a person pressing the same menu entry are the same thing. For a
**program** there are none, because the agent has its own.

It is **off** unless somebody says so, and `chat.tool_limit` bounds a turn at eight rounds because a
model that decides to list every file should stop being funded by a pane nobody is watching. A command
that *waits* is refused with a sentence: a tool call that never returned would leave the conversation
stopped with nothing on the screen to say why.

**A command that runs a program of the model's choosing is behind a second switch.** The catalogue
includes `terminal send`, `run add`, `run start`, `run rerun`, `debug install` and `launch`, which
between them will run any command line at all on this machine — and the other end of this connection
is a **server**, which is not the same trust as an agent a person started in their own terminal.
`chat.shell` is that switch, and both are off unless somebody says so.

### Where a key lives

**A key is never written by Unluminous**, and for the two rows that ship there is no key. A row that
*sends to an address* names an **environment variable**, read at the moment a request is sent and
never held; a row that *runs a program* names nothing at all. What is written down is the name of the
place the key is. The Settings page says `set` or `not set` and never the value, it does not draw a
key field for a program at all, and a refusal names the variable — or the program — rather than
quoting a header.

### Three refusals in the transport

- **A redirect is not followed.** The HTTP client strips `authorization` when it follows one and has
  never heard of Anthropic's `x-api-key`, so a redirect would carry the key somewhere nobody
  configured — and an endpoint that answers with one is not the endpoint that was typed in.
- **A server's own words are quoted verbatim in a refusal**, which is the rule git already keeps, so
  the **key is redacted out of them first**: a gateway that echoes the request back would otherwise
  put the secret in the conversation and then in the transcript on disk.
- **A stream that never frames an event is stopped** rather than buffered until the allocator gives
  up, which ends the process.

### What is drawn

A message body is the ordinary Markdown render plus the ordinary layout — the same thing the editor's
own preview is made of — so headings, lists, quotes, tables and code all work and none of it is a
second renderer. A fenced block is coloured by the plugin that claims its language.

**A model's own reasoning goes back up exactly as it came down.** Anthropic signs a thinking block and
verifies the signature on the next turn, so a continuation whose blocks were rebuilt out of the words
on the screen is refused — and a redacted block is encrypted and cannot be rebuilt at all. So the
blocks are kept as JSON beside the words they produced and replayed ahead of the message they belong
to. What is *drawn* is the text; what is *sent* is the block.

**A bubble is as wide as its words while a tool block, a failure and the thinking are as wide as the
row**: they are reports rather than speech, and sized to their own message a tool called from a two
word answer came out two words wide with its own caret clipped off the end of it.

**There is no context meter**, and that is the absent-control rule rather than an omission. A URL and
a model name say nothing about a context length, so a bar there would be a fraction of a number nobody
measured. What is drawn is what the server really reported: the tokens in and the tokens out.

**A picture can be attached** — pasted, dropped, or chosen with the button. Pasting is seen on the key
going **up**, and it could not have been seen any other way: the interface library recognises the
paste chord, reads the clipboard's **text**, pushes a paste event only if that text is not empty, and
then returns, swallowing the key press. With a picture on the clipboard the text read fails outright
and the frame carries no event at all. The release comes back through the ordinary path with the
modifier still held, and that is the one report of the chord that reaches Unluminous.

Two things follow from asking a keystroke late. **Nothing on the clipboard is not a fault** — the
release arrives after an ordinary text paste too, and "there is no picture on the clipboard" under the
composer every time somebody pasted a sentence would be a message about nothing. And **a drop with no
pointer is this pane's**: Windows carries a file over a window through OLE and sends no cursor
movement at all, so gating the drop on the pointer threw the picture away silently.

### Nothing is fetched that was not asked for

One turn goes out when somebody presses send — a program they named in a Settings page, or an address
they typed into one. There is no discovery, no model list, no telemetry and nothing at startup, and a
Markdown image inside an answer still shows its alt text rather than being fetched.

### From the command line

```sh
unluminous-cli plugins run agent-chat new
unluminous-cli plugins run agent-chat send "why is this test failing?"
unluminous-cli plugins run agent-chat state
unluminous-cli plugins run agent-chat last
unluminous-cli plugins view agent-chat --json
```

`send` **does not wait**, because a provider's command runs inside a frame and a command that blocked
would stop the window drawing for the length of a model's answer. `state` says when it has finished,
which is the shape `run start` and `run output` already have.

On the canvas, `space chat <node> <verb>` forwards to the same function, so the verbs are not a second
list and the two cannot answer differently. What it adds is *whose* conversation.

## Agent-Tasks

A board whose tickets are worked by an agent in a terminal Unluminous owns. Four lanes, cards with
todos and comments, a terminal for each ticket, and a session that is resumed by the agent's own
conversation id rather than by a process that outlives the editor.

The board is one SQLite file, compiled into Unluminous, so there is nothing to install, no server to
be running and no port.

Three listings beside the board — Backlog, Completed and Epics — and one rule decides their shape: **a
card on the board is a hundred points tall and carries a play button and three counts, because a lane
holds a dozen of them and each is a thing to act on; a listing holds hundreds and what somebody is
doing there is finding one, so a row is one line.** That difference is the difference between a board
and a list.

**A ticket is the modal and nothing else.** The tab used to draw the lanes on the left and the open
ticket on the right, which was reported as a side panel opening up — and the thing that opened it was
an agent *reading* the ticket. `task <key>` and `open <key>` are two commands now: one answers with a
ticket and changes nothing about the window, the other is what puts one in front of somebody.

**The modal's heights add up**, and that is the whole of its difficulty. Four sections are laid out one
under another with no scroll, so a budget that overflows does not clip — it draws the last section off
the bottom edge and the one before it over its own buttons. Every section says what it **wants** and
what it can be **cut to**, and the shortfall comes off them in order.

A sprint or an epic is named on the command line **by its name**, because that is what is on the
screen: the parser takes the longest run of arguments that names one, so `sprint-rename August 2nd
Half September` needs no id.

```sh
unluminous-cli plugins run agent-tasks board
unluminous-cli plugins run agent-tasks new-task Read the passages back
unluminous-cli plugins run agent-tasks move-task task-3 in_progress
unluminous-cli plugins run agent-tasks start task-3
unluminous-cli plugins run agent-tasks terminal task-3 40
```

## The pane a plugin draws has depth

The board is drawn in **dark neumorphism**: every surface is lit from the top left, so a raised thing
carries a pale shadow above and left of it and a dark one below and right, and a recessed thing
carries the same pair inside it.

Four of those are things the ordinary painter cannot draw at all, and they are the four that make the
picture look like the picture: an **inset shadow**, a **diagonal gradient**, a **glow round a circle**,
and **clipping to rounded corners**. So a second renderer draws them, using the CPU rasteriser the
text engine already depends on to rasterise the glyphs in its own font atlas — the renderer was
already in the binary, and this uses the copy that is there rather than adding a second one.

**A plugin asks for it in three places and any one of them says no**: `ui.chrome = vello` in the
manifest, the provider's own answer, and the `plugins.chrome` setting. A provider that draws only with
the ordinary painter costs no pixmap, no rasterisation and no texture.

**The elevation never introduces a hue.** The pale half is the surface *lifted* and the dark half is
black at an alpha, so the palette stays closed while the pane gains a whole dimension. And the pale
half is barely there, which was measured rather than judged: lifting by eighteen read as a bright grey
rim round every surface.

**Five things about the cost, and every one was measured.**
`cargo run --release -p unluminous-app --example vello_cost` measures them again.

- **Nothing is rasterised on a frame where nothing changed** — a comparison of the kept shape list,
  the canvas rectangle and the scale, 0.001 ms and no allocation. It is the list and not a hash of it,
  because a collision would serve stale pixels.
- **A hover deliberately does not change the decoration.** A card keeps the same elevation under the
  pointer and the pointer's answer is a wash painted on top, because moving the pointer across a board
  is the commonest thing anybody does on one.
- **A raised surface's shadows are cut to the band around it**, because the surface is opaque and is
  painted over its own shadows: unclipped, one lane cost 3.3 ms of Gaussian that was then covered up.
  43 ms to 20 for a whole board.
- **The canvas is the decoration's own bounding box, not the pane's**, because rasterising has a floor
  of about two nanoseconds a pixel whether anything is drawn there or not.
- **The rasteriser's multithreading is the big lever and it cannot be pulled.** It took the board from
  20.1 ms to 5.5 — and every screenshot test then panicked inside it, from the text engine's own
  glyphs: cargo features are additive, so enabling it changes the default settings for the text engine
  too, and its glyph rasteriser never flushes.

**And it misses the budget it opened with**, which is said plainly and then revised rather than
softened. The budget was a third of a frame at sixty a second, about 5 ms. A four-lane board of
twenty-four cards over 1400 by 900 points costs **20.7 ms** on a changed frame, so a drag on a full
board runs at something like 40 a second. The revised requirement is four lines: a still frame must be
free, a changed frame may cost more than a frame while something is actually moving, it must be
possible to say no, and the route back to the original number must be written down.
`Settings -> Appearance` has a tick box that turns the whole thing off, at no cost at all, which is
the honest bottom of it: the depth is worth the milliseconds or it is not, and the person decides.
