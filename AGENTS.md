# AGENTS.md — working on Unluminous, for an AI agent

> ## Releasing: one command
>
> ```powershell
> pwsh tools/release.ps1              # patch
> pwsh tools/release.ps1 -Part minor
> pwsh tools/release.ps1 -WhatIf      # the plan, and nothing written
> ```
>
> It runs the suite, bumps the version, builds and installs the Windows installer, **builds, signs
> and notarises the macOS bundle**, writes the changelog, commits, tags, pushes, publishes the
> GitHub release on both repositories and both sites, and then asks each destination what it
> actually serves.
>
> **macOS is in by default and is not a flag.** It was behind `-Macos` for one afternoon, and a
> switch that defaults off is a switch the next release forgets - 0.53.1 went out Windows-only that
> same day and put Windows 0.53.1 next to macOS 0.53.0 on the site. It is included whenever its
> preflight passes (`installer/macos/build-on-windows.ps1 -Preflight`), and the release says at the
> start why it is not when it does not. `-SkipMacos` forces it off.
>
> **Do not run `installer/windows/build.ps1` or `installer/macos/build-on-windows.ps1` by hand for a
> release.** They are what it calls.


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
