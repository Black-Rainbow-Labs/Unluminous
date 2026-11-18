# task-2737: Agent Chat's `full` permission when Claude Code refuses `bypassPermissions`

GitHub issue: https://github.com/Black-Rainbow-Labs/Unluminous/issues/4, reported against 1.8.3 on macOS.

## What the issue says

With `chat.permission = full`, a Claude Code turn in the Agent Chat pane had Edit, Write, `git checkout -b`,
running a binary and Unluminous's own MCP tools refused with "Claude requested permissions to use <tool>, but you
haven't granted it yet." No approval prompt was shown. Calls matched by a `permissions.allow` rule ran.

## What was measured (Claude Code 2.1.326, 2026-11-17)

`full` is sent to Claude Code as `--permission-mode bypassPermissions` (`unluminous_chat::agent::mode_of`).

| Folder | Mode asked | Mode on the `init` line | Write |
|---|---|---|---|
| no settings | `bypassPermissions` | `bypassPermissions` | ran |
| `.claude/settings.json` with `permissions.disableBypassPermissionsMode = "disable"` | `bypassPermissions` | `default` | refused, "you haven't granted it yet" |
| same, with `--input-format stream-json --permission-prompt-tool stdio` | `bypassPermissions` | `default` | a `can_use_tool` request |
| same | `acceptEdits` | `acceptEdits` | (not refused by the setting) |

So the cause is that **Claude Code refuses `bypassPermissions` when any settings file disables it, and falls back to
`default` without an error**. A company's `managed-settings.json` (`/Library/Application Support/ClaudeCode/` on macOS,
`C:\Program Files\ClaudeCode\` on Windows) commonly sets it. In `--print` with nobody to ask, `default` refuses every
tool no allow rule names, which is exactly the report. The other two init values measured: `manual` is reported as
`default`; `acceptEdits`, `auto`, `dontAsk` and `plan` are reported as themselves.

## Is it still a fault in 1.31.1

Partly. `task-2392` added `chat.ask` (on by default), which starts Claude Code with `--permission-prompt-tool stdio`,
so the refused calls now reach the pane as approval cards. But:

- `full` asks about every edit and every command, and nothing says why the person's choice of `full` was not used.
- With `chat.ask` off, every tool is still refused with no prompt, as in the report.

## The fix

1. **The decoder knows the mode it asked for.** `agent::Decoder::asked_mode` is set by `agent::run` from `mode_of`.
   On Claude Code's `system/init` line it compares `permissionMode` with it, reading `manual` and `default` as one
   mode, and sends `Reply::ModeRefused { asked, running }` when they differ.
2. **The pane says so once**, as a notice of kind Problem: the mode asked for, the mode Claude Code is running in,
   the setting that does this and where a company puts it, and what Unluminous does next. With `chat.ask` off it
   also says that tools will be refused until `Ask before running tools` is turned on.
3. **Later turns in that folder ask for `acceptEdits`** when the mode would be `bypassPermissions`
   (`AgentChat::bypass_refused`, a set of working folders kept for the life of the window). Edits then run without
   asking, which is the strongest mode the machine allows that still means "full" for files; commands are asked
   about through `chat.ask`. The set is not written to disk, so a policy that is lifted takes effect at the next
   start. A row that names its own mode is left alone, apart from the notice.
4. `Session::reply` ignores the new reply. The conversation is not changed by it.

## Not done

- Reading the managed settings file ourselves: Claude Code's own report on `init` is the answer and covers every
  settings file, `--restricted` and running as root.
- Trying `auto` before `acceptEdits`: `auto` can be disabled by the same kind of policy (`disableAutoMode`) and is a
  classifier the person did not choose.
- The issue's side note that `node` is missing from `PATH` because nvm loads from `~/.zshrc`: `services::login_shell`
  already starts the profile as an interactive login shell since `task-1905`, so it is not this fault.

## Tests

- `agent.rs`: the decoder sends `ModeRefused` for `bypassPermissions` reported as `default`, sends nothing for
  `bypassPermissions` reported as itself, and nothing for `manual` reported as `default`. The lines are the real
  `init` line shape from the measurement.
- `services/agent_chat`: with a scripted Claude Code that reports `default`, the first turn raises the notice and the
  second turn's command line carries `--permission-mode acceptEdits`.
