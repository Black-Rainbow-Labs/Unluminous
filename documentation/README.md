# Unluminous documentation

The documentation in this repository. **[unluminous.com](https://unluminous.com)** is what Unluminous
looks like, what it does, and where the installer is; start there if you are deciding whether to use
it rather than looking something up.

Every page here is written to be read on its own. Nothing is left implicit because an earlier page
said it.

**The order below is a reading order**, for somebody who has just opened the repository: what it is,
then how to run it, then what is in the window, then what it does with text and with code, then the
three panes it carries, then how it is built.

## Start here

| | page | what it answers |
|---|---|---|
| 1 | [What it is](what-it-is.md) | what Unluminous is, who it is for, and the case for it against the editor you already use |
| 2 | [What it looks like](overview.md) | thirty-seven captures of the running window, over a real desktop |
| 3 | [Getting started](getting-started.md) | install it, run it, the switches, windows and projects, where the settings live |
| 4 | [The window](the-window.md) | every part of it named, the panels and how they move, the nine menus, every Settings page |

## Using it

| | page | what it answers |
|---|---|---|
| 5 | [Editing](editing.md) | text, formatting, files, encodings, find and replace, highlights, folding, Markdown and diagrams |
| 6 | [Writing code in it](writing-code.md) | line numbers, tabs, completion, definitions and references and rename, git, running, the debugger |
| 7 | [The terminal](the-terminal.md) | the tile, the tabs, which shell, what the emulator handles, and how a tab comes back |
| 8 | [The Base of Infinite Space](the-canvas.md) | the canvas: six kinds of node, what a connection grants, and how it is driven |
| 9 | [The agent panes](agent-panes.md) | Agent-Chat and Agent-Tasks: what runs, what a key is, and what a model may call |
| | [The Database plugin in pictures](database.md) | the tree, a grid, a console, a pending change, and adding a data source |

## Driving it

| | page | what it answers |
|---|---|---|
| 10 | [For AI agents](for-ai-agents.md) | the contract, the three mechanisms that enforce it, what an agent is given and what it costs, and what a study found an agent actually does |
| 11 | [The command line](command-line.md) | the 214 commands, the channel underneath them, and the MCP server on top |
| | [`unluminous-cli/docs/commands.md`](../unluminous-cli/docs/commands.md) | the reference, written to be handed to an AI agent whole |
| | [`unluminous-cli/docs/protocol.md`](../unluminous-cli/docs/protocol.md) | the socket underneath it, for a client in another language |
| | [`unluminous-cli/docs/mcp.md`](../unluminous-cli/docs/mcp.md) | installing the MCP server into an agent, the two tool shapes, and what a local port does and does not defend against |

## Working on it

| | page | what it answers |
|---|---|---|
| 12 | [Architecture](architecture.md) | eight crates, inside the editor, inside the window, what one frame does, the seams, the threads, where state lives, and what it all costs |
| 13 | [How plugins work](plugins.md) | why a plugin is data, the manifest key by key, where they come from, what the tokeniser does with a grammar, and writing one |
| 14 | [Tests](testing.md) | the four layers, the three rules the rendered tests keep, and what the release scripts check |
| | [Not included](not-included.md) | everything deliberately absent, with the reason for each |
| | [Taking the pictures](taking-the-pictures.md) | how the gallery is made, in one command, without taking the keyboard |
| | [`../CONTRIBUTING.md`](../CONTRIBUTING.md) | why a feature is three things, how to run each test layer, which crate a change goes in, and the house style |
| | [`../CLAUDE.md`](../CLAUDE.md) | the conventions the code already follows, for whoever changes it next |
| | [`../AGENTS.md`](../AGENTS.md) | the same front door for an agent that is not Claude Code |

## Elsewhere in the repository

| | |
|---|---|
| [`design/style-guide.md`](../design/style-guide.md) | how a control in Unluminous is built: the closed palette, the row heights, the one shape a modal has, and the plain name every control carries. **Read it before drawing anything new.** |
| [`design/accessibility.md`](../design/accessibility.md) | what the accessibility tree does and does not do, every contrast ratio measured against WCAG 2.2, and the plain answer about 1.0 |
| [`design/icons.md`](../design/icons.md) | how the `material` icon set was designed, which published mark each one was drawn from, and the one the reference sheet got wrong |
| [`installer/README.md`](../installer/README.md) | how to build an installer, on either platform |
| [`CHANGELOG.md`](../CHANGELOG.md) | what changed in each release, written from the ticket-prefixed commits so it cannot fall behind |
| [`tools/agent-study/README.md`](../tools/agent-study/README.md) | the harness that watches an agent drive a real window, and the one number it reports |
| [`unluminous-cli/agent-assessment/`](../unluminous-cli/agent-assessment/) | how well a local model does with the command reference, measured against a live window |
| `tasks/` | the design documents: what was chosen, what was rejected and why, one per feature |

`design/` holds **intent** — the image a component is compared against, changed only when the design
changes. `crates/unluminous-app/tests/snapshots` holds **accepted output** — a change that alters the
rendering fails against it, and nothing is accepted without somebody opening the image.
