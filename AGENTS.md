# AGENTS.md — working on Unluminous, for an AI agent

**[`CLAUDE.md`](CLAUDE.md) is the guide, whichever agent you are.** It is not about one vendor: it is
what each crate is for and what must never be in it, the seams between them, and the reason beside
every rule that was measured rather than chosen. It is long because the reasons are in it. Read the
section covering the part you are changing, and nothing else, and you will not have to undo anything.

[`CONTRIBUTING.md`](CONTRIBUTING.md) is the short version: why a feature is three things, how to run
each of the four test layers, which crate a change goes in, and the house style.
[`documentation/README.md`](documentation/README.md) is what Unluminous *does*, in a reading order —
[Architecture](documentation/architecture.md), [How plugins work](documentation/plugins.md) and
[Tests](documentation/testing.md) are the three pages worth having open while changing it.

Two things to know before you start.

**Everything a person can do in this window, you can do too, through the same command.** That is the
product rather than a feature of it, so a change that gives a person a new control and gives you
nothing is not finished. `unluminous-cli` is the command line, `unluminous-cli/docs/commands.md` is
the reference — written to be handed to an agent whole — and `unluminous-cli mcp serve` offers every
one of those commands as a tool, generated from the same catalogue the command line parses against.
`Settings -> Tools -> MCP` writes Unluminous into Claude Code's or Codex's own configuration.

**Four things are enforced by a test rather than by a reviewer**, and each fails a build on somebody
else's machine if you skip it:

| you added | what fails if you do not |
|---|---|
| a menu entry with no name | `app/action_names.rs` |
| a command with no catalogue row | `unluminous-cli` cannot parse it and the window cannot dispatch it |
| a command not offered as a tool | `every_command_is_offered_as_a_tool_in_both_shapes` |
| a command with no documentation section, or a stale usage line | `unluminous-cli/src/documentation.rs` |

If you are **using** Unluminous rather than changing it — driving the window from an agent — read
`unluminous-cli/docs/commands.md` and `unluminous-cli/docs/mcp.md` and stop. Neither needs anything
in this file.
