Settings screen 8 of 9: Plugins › Agent-Tasks
Code: components/agent_tasks/settings_page.rs. Keys are lines in plugins/agent-tasks/settings.conf; the key goes to the keychain. The page scrolls. Each setting is its name in strong text, the control under it (300 wide), buttons to its right, and a line of help under that. Every text field has a Copy button beside it.

Each setting: Label | control | options or range | default | key

**The board**
- Board file | read only path in monospace, with Copy and Reveal buttons | | the board in the settings folder | database (not editable here)
Help: "One Inillucent file. Copy the path to back it up, `Reveal` to open the folder it is in, or set `database` in this plugin's own settings.conf to move it."
- Default project | text field, hint "the folder this window has open" | | empty | project
Help: "Where an agent is launched when a ticket names no project of its own."
- Lease | text field, hint 45 | whole minutes above 0 | 45 | lease
Help: "How many minutes the board waits to hear from an agent before the watchdog calls its lease expired and starts nudging it."

**What a new ticket gets**
- Default agent | two choice buttons, 84 wide | claude, codex | claude | agent
Help: "Which agent a new ticket is assigned to, and which one takes a ticket assigned to a person."
- Default model | dropdown (the same control as a ticket's Model) | empty = "the agent's own default"; claude: claude-opus-5[1m], claude-opus-5, claude-sonnet-5, claude-fable-5, claude-haiku-4-5-20251001; codex: gpt-5.6-sol, gpt-5.6-terra; a saved value not in the list is shown first | empty | model
Help: "Passed as `--model`. The models the chosen agent has, and whatever this setting already names if it is not one of them."
- Default effort | dropdown | empty = "the agent's own default", low, medium, high, xhigh, max | empty | effort
Help: "Passed as `--effort`. Codex knows nothing above `high`, so `xhigh` and `max` collapse onto it."

**How an agent is launched**
- Claude command | text field, hint "claude --dangerously-skip-permissions" | | empty (Unluminous's own) | claude-command
Help: "The program and the flags in front of it, for a ticket assigned to Claude. The ticket's own model and effort and the session flag are added after it, because those come from the row and the board has to be able to resume the conversation it named. No shell runs this, so nothing expands and a path with spaces in it goes in \"quotes\"."
- Codex command | text field, hint "codex" | | empty | codex-command
Help: "The same for a ticket assigned to Codex. `resume <id>` stays the first thing after the program, because it is a subcommand rather than a flag."

**Where the agent connects**
- Iliad URL | text field, hint "the agent's own endpoint" | | https://iliad-emerging-api.abbvienet.com/api/llm until cleared | base-url
Help: "The gateway the agent talks to, handed to it as ANTHROPIC_BASE_URL and OPENAI_BASE_URL. Empty means whichever endpoint the agent itself is configured for, which is api.anthropic.com for Claude."
- Iliad key | password field; its placeholder says "set, from <source>" or "not set: type it here, or export ANTHROPIC_API_KEY in your shell profile"; buttons Save key (70 wide) and Clear (60 wide) | | not set | keychain entry "iliad", service unluminous-agent-tasks
Help (macOS, Linux): "Typed once and never shown again. It goes to this machine's keychain under `iliad`, not to a file, and Unluminous reads it at the moment an agent is launched and hands it over as ANTHROPIC_API_KEY, OPENAI_API_KEY and ILIAD_API_KEY. Nothing needs typing here when your shell profile already exports ANTHROPIC_API_KEY …"
On Windows the field and buttons are not drawn; instead: "not set, and Unluminous has no keychain on windows: export ANTHROPIC_API_KEY in your shell profile".

**Not built yet**
- The Iliad key on Windows: keychain.rs can use Windows Credential Manager, but the page only draws the field on macOS and Linux (settings_page.rs, has_a_keychain).
- The board file location can only be changed by editing settings.conf.
