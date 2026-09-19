# Security

## Reporting a vulnerability

**Use GitHub's private vulnerability reporting** on this repository: the *Security* tab, then *Report a
vulnerability*. That opens a private thread only the maintainers can read, which is what you want for
anything that should not be in a public issue until there is a fix.

If that is unavailable to you, open a public issue saying only that you have something to report and
asking for a private channel. **Do not put the details in a public issue.**

**What to expect.** A first reply within **five working days**, saying whether the report is understood
and reproducible. After that, an assessment within **fifteen working days** — what the impact is, whether
a fix is coming, and when. If a report goes quiet past those windows, it is an oversight rather than a
decision; say so on the thread.

A fix ships in the next release, and the advisory is published when it does. You are credited by whatever
name you ask for, or not at all if you prefer.

## What this program is, for the purposes of a threat model

Unluminous is a desktop editor. It holds no account, sends nothing anywhere unless you ask it to, and its
whole attack surface is things you point it at: files, a repository, a database, a web page, a debug
adapter, and an AI agent you connected. Two of those are doors that can be opened from outside the
process, and both are shut by default.

## What is in scope

- **The control channel.** A running Unluminous listens on `127.0.0.1` on a port the operating system
  chose, and every request carries a per-run token written into an instance file under your own settings
  folder. Nothing is ever bound to anything but the loopback interface, and there is a test for it. A way
  to reach that channel from another machine, a way to make it accept a request with no token or a
  token from another run, or a way for a page in a browser to drive it, is in scope. The channel can be
  closed entirely with `unluminous --control off`.
- **The MCP HTTP endpoint**, which is **off unless you turn it on**. When it is on, a request whose
  `Origin` is not loopback is refused, and so is one whose `Sec-Fetch-Site` says it came from another
  site — a page cannot set either header, and a browser attaches `Origin` to every cross-origin POST.
  Both have tests. A way past either is in scope.
- **Escaping a browser tab's project origin.** A local page is served from an `unluminous://` origin
  confined to one canonical root. A path that reaches outside that root, a write method that is answered,
  or any way for a page to reach the host process — there is deliberately no JavaScript host bridge —
  is in scope. Those refusals have tests that need no browser runtime.
- **A key reaching somewhere it should not.** Unluminous never writes an API key down: a chat provider that
  sends to an address names an *environment variable*, read at the moment a request is sent and never
  held, and a database password goes to the platform credential store with only its name recorded. A key
  in a settings file, a log, a transcript, an error message or a screenshot is a vulnerability. So is a
  redirect being followed on a request that carries one, which is why redirects are refused outright.
- **A crafted file that makes the editor read out of bounds, allocate without bound or loop forever.**
  Opening a file is the commonest thing this program does, and the parsers behind it — the Markdown
  reader, the syntax tokeniser, the Mermaid reader, the picture decoders, the terminal's escape sequence
  reader — all take bytes you did not write.
- **A crafted response from something Unluminous talks to**: a debug adapter's protocol frames, a
  PostgreSQL server's wire messages, a model's server-sent events, or a shell's escape sequences.
- **SQL built by Unluminous rather than typed by you.** Every value a grid edit sends is a bound
  parameter; a value that reaches a statement as text is in scope.

## What is not

- **What an agent you connected does.** Connecting Claude Code or Codex gives it the editor on purpose,
  and `chat.shell` gives a model the commands that run a program on purpose. Both are off until you turn
  them on and both say what they are. An agent doing something you did not want is a permission you
  granted, not a flaw.
- **What a plugin you installed contains.** A plugin is data and nothing in one is executed — but a
  manifest can name a colour scheme and a set of keywords, and one you installed can make files look
  however it likes.
- **A program you asked the editor to run.** Run configurations, the terminal and the debugger start what
  you named, with your own privileges.
- **A database you pointed it at.** The engine's own limits are the limits; a query you wrote that takes
  an hour takes an hour.
- **A missing feature,** and anything that needs write access to your own files. A caller who can write
  your settings can write anything into them, and no editor defends against that.

## What the repository already does about this

Named so a reporter knows what has been looked at rather than having to find out:

- **Nothing is fetched that was not asked for.** No telemetry, no crash upload, no model list, no
  debugger download, and a Markdown preview shows an image's alt text rather than fetching it. The update
  check is off until you turn it on.
- **The TLS a request uses is the machine's own** — schannel on Windows, Security.framework on macOS — so
  the certificates Unluminous trusts are the certificates you trust.
- **A server's own words are quoted verbatim in a refusal, with the key redacted out of them first**, so
  a gateway that echoes the request back cannot put a secret in a transcript.
- **A stream that never frames an event is stopped at a bound** rather than buffered until the allocator
  gives up.
- **`git` is the real `git` program**, with your credential helper, your ssh agent and your signing
  configuration. Unluminous never reimplements an authentication path.
