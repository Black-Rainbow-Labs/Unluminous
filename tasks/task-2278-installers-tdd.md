# task-2278: an install that finishes setting Unluminous up

## 1. What the ticket asks

> The mac and windows installers only partially setup Unluminous. [...] We want to install the cli,
> wire up when available (codex, opencode, claude code, api approach anthropic key, openai key) as part
> of the installation process and/or the agent instructions. Agent instructions for wiring up may be
> less brittle. We also need to test running the install as a fresh install/environment, without
> impacting our main installation, and verify that things get wired up correctly and function after
> install.

Four things, then:

1. The command line tool is on the path after an install, on both platforms.
2. Every coding agent on the machine that can talk to an MCP server is told about Unluminous's server:
   Claude Code, Codex and OpenCode.
3. Unluminous's own AI endpoints list includes the Anthropic and OpenAI APIs when their keys are set.
4. A test that installs into a fresh environment, checks all of the above against the real agents, and
   leaves the installation on this machine exactly as it was.

## 2. What an install does today

### Windows

`installer/windows/unluminous.iss` installs `unluminous.exe` and `unluminous-cli.exe` into
`%LOCALAPPDATA%\Programs\Unluminous` with no elevation. Five tasks: a desktop icon, the PATH entry, the
two right click menus and the Open With registration. None of them is marked `unchecked`, so all five
are on by default, and a silent install with no `/TASKS` takes all five. Nothing connects an agent.

### macOS

The archive holds `Unluminous.app`. The agent instructions tell the agent to move it to
`/Applications` and to make a link in `/usr/local/bin`. That folder does not exist on a new Apple
Silicon Mac and is owned by root where it does exist, so the `ln -sf` fails without `sudo` on most
Macs. Nothing connects an agent.

### The agent instructions on unluminous.com

`sites/unluminous/src/data/page.ts` in black-rainbow-labs-sites. Three faults:

- **`/TASKS="addtopath"` turns the other four tasks off.** Inno's `/TASKS` is the complete list of
  tasks to select, so an install by an agent lost the desktop
  icon, both right click menus and the Open With registration. `/MERGETASKS` adds to the defaults, and
  leaving the switch out entirely takes the defaults, which is what is wanted.
- The macOS `ln -sf` into `/usr/local/bin` needs administrator rights or a folder that is not there.
- Step 5 knows two agents and asks the agent to pick one. OpenCode is not mentioned, and an agent that
  is not Claude Code or Codex has no instruction at all.

### What already exists to build on

- `unluminous-cli mcp install claude|codex|both` (`unluminous-cli/src/mcp/install.rs`). It uses the
  agent's own command when it is on the path (`claude mcp add-json`, `codex mcp add`) and edits the file
  when it is not, taking a copy first.
- `Settings -> Tools -> MCP` has an Install button for each of the two.
- `Settings -> AI Configuration` (`services::ai_endpoints`) ships four endpoints: `claude`, `codex`,
  `opencode` (each runs the agent installed on the machine and needs no key) and `local` (llama.cpp on
  127.0.0.1:8080). A row that runs a program that is not installed is hidden. There is no row for the
  Anthropic or OpenAI API, although the wire shapes for both exist (`anthropic`, `responses`).

## 3. What other tools do

Measured or read during this ticket:

| Tool | How it reaches agents |
|---|---|
| `add-mcp` (Neon) | Detects installed agents by their configuration folders, writes each file directly, about 25 agents. `-y` for no questions. |
| VS Code | `code --add-mcp <json>` writes the user profile's `mcp.json`. |
| Cursor | A `cursor://` link the person clicks. |
| Claude Code | `claude mcp add-json <name> <json> --scope user`, non interactive. Writes `~/.claude.json`. |
| Codex | `codex mcp add <name> -- <command> <args>`, non interactive. Writes `~/.codex/config.toml`; honours `CODEX_HOME`. |
| OpenCode 1.18 | `opencode mcp add` exists and is interactive (it asks for local or remote, the name and the command). The configuration is `~/.config/opencode/opencode.json` on every platform, `$XDG_CONFIG_HOME/opencode` when that is set, and an `mcp.<name>` entry of `{ "type": "local", "command": [program, args...], "enabled": true }`. |

The pattern every one of them follows, and which `mcp install` already follows for two agents:

1. Detect the agent by its program on the path or by its configuration folder.
2. Use the agent's own command where it is non interactive; edit the file where it is not.
3. Take a copy of a file before it is edited, and change one key.
4. Running it twice changes nothing.
5. Never replace an entry somebody wrote by hand.
6. Say what changed, and that the agent has to be restarted to see it.

## 4. The design

### 4.1 One command does the setting up: `unluminous-cli setup`

The installer, the window's first start on macOS, the agent instructions and a person in a terminal
all run the same command. Two reasons it is a command rather than code in the installer:

- **The agent instructions become one line.** "Run `unluminous-cli setup` and read what it says" does
  not break when an agent is added, a file moves or a flag changes, because the knowledge is in the
  program and the program ships with the install. That is what the ticket means by less brittle.
- **It can be tested.** Inno Setup's Pascal and a shell script in a zip cannot be unit tested; Rust can,
  against a home folder of the test's own.

What it does, in order, each step reported as one line and as a JSON row with `--json`:

1. **The command line tool is on the path.**
   - Windows: the user's `PATH` in `HKCU\Environment` holds the install folder. The installer's
     `addtopath` task already writes it, so `setup` only reports it, and adds it when it is missing and
     `setup` was run from an installed copy (`--path` forces it, `--no-path` skips it). Written through
     `reg.exe` so the value keeps its `REG_EXPAND_SZ` type and its unexpanded `%VARIABLES%`, and
     followed by a `WM_SETTINGCHANGE` broadcast through the same tool so new terminals see it.
   - macOS and Linux: a symbolic link to this `unluminous-cli` in the first folder of this list that
     exists and can be written: `/usr/local/bin`, `/opt/homebrew/bin`, `~/.local/bin` (made when it is
     the one chosen). When the chosen folder is not on the login shell's `PATH`, one marked line is
     added to `~/.zprofile` (or `~/.bash_profile` when the login shell is bash):
     `export PATH="$HOME/.local/bin:$PATH" # added by unluminous-cli setup`. A line with that marker is
     never added twice. A link that already points somewhere else is left alone and reported, unless it
     points at a file that is no longer there.
2. **Which agents are on this machine.** For each of Claude Code, Codex and OpenCode: the program on
   the `PATH` (with `PATHEXT` on Windows, so `codex.cmd` from npm is found), then the folders their own
   installers use (`~/.local/bin/claude`, `%APPDATA%\npm`), then whether its configuration folder exists
   (`~/.claude`, `~/.codex` or `$CODEX_HOME`, `~/.config/opencode`). An agent with neither is skipped
   and named as not found.
3. **Each agent found is connected to Unluminous's MCP server** over stdio, with the absolute path of
   this `unluminous-cli`, so the agent needs nothing on its `PATH` to start it.
   - An entry called `unluminous` that already points at this program: nothing to do.
   - An entry called `unluminous` that points at another program that exists (a build in a checkout, a
     second install): left alone and reported, because somebody chose it. `--force` replaces it.
   - An entry that points at a program that is no longer there: replaced, because it cannot work.
   - No entry: written, through the agent's own command where there is a non interactive one, by
     editing the file otherwise. OpenCode is always the file.
4. **The AI endpoints.** Which keys are set, by name only: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`. The
   endpoint rows themselves are the window's (§4.4); `setup` reports what the window will offer.
5. **It works.** The MCP server this install would start is started, sent `initialize` and
   `tools/list` over stdio exactly as an agent would, and the number of tools it answered is reported.
   A server that does not answer is a failure and the exit code says so.

Flags:

| Flag | Meaning |
|---|---|
| `--check` | Report what is and is not set up, and change nothing. |
| `--remove` | Take out every `unluminous` entry that points at this program, and the link `setup` made. Entries pointing elsewhere are left. The uninstaller runs this. |
| `--force` | Replace an entry or a link that points at another program. |
| `--agents claude,codex,opencode` | Only these agents. |
| `--no-path` | Leave the path alone. |
| `--json` | One JSON object with a row per step. |

Exit code 0 when every step that was attempted worked, 1 when one failed. An agent that is not
installed is not a failure.

`setup` changes an agent's configuration, so it is held back from the MCP tools for the reason
`mcp install` is (`mcp::tools::offered`): a tool that edits the file that decides which tools exist
takes effect on a later run of a program the model is not in. The held back list becomes three, and the
test that counts it says so.

### 4.2 OpenCode joins `mcp install`

`mcp::install::Client` gains `OpenCode`. Its file is `$XDG_CONFIG_HOME/opencode/opencode.json`, or
`~/.config/opencode/opencode.json`; an `opencode.jsonc` there is used when it exists and the `.json` does
not, and `OPENCODE_CONFIG` naming a file wins over both. The edit is the JSON edit `mcp install` already
does for Claude Code, with the key `mcp` and the entry

```json
{ "type": "local", "command": ["C:\\...\\unluminous-cli.exe", "mcp", "serve"], "enabled": true }
```

A `.jsonc` file with comments in it does not parse as JSON and is refused with a sentence naming the
entry to paste, which is the rule `write_json_server` already keeps for a broken `.claude.json`.

`mcp install` takes `opencode` and `all` (`both` keeps meaning Claude Code and Codex, so a script
written against it does not change). `mcp config opencode` prints the entry. `Settings -> Tools -> MCP`
gets a third Install button and a third tab in its example.

### 4.3 Where the installers run it

**Windows.** A sixth task, `connectagents`, on by default: *"Connect the coding agents on this machine
(Claude Code, Codex, OpenCode) to Unluminous"*. A `[Run]` line runs
`unluminous-cli.exe setup --installer` hidden, waiting for it, as the user who started setup (not
elevated, because the files it edits are that user's). `--installer` writes the report to
`%APPDATA%\Unluminous\setup.log` as well, since a hidden program's output goes nowhere. `[UninstallRun]`
runs `setup --remove`, so uninstalling does not leave agents pointing at a program that is gone.

An upgrade from a version without the task: Inno remembers the tasks of the last install and selects a
task it has not seen by its default, so an upgrade runs `setup` once. That is safe because of §4.1 step
3: an entry somebody pointed at a checkout is left alone.

**macOS.** There is no installer to run anything, so the window does it: the first time Unluminous
starts on a machine (the settings file does not exist yet, the same test that picks the default
background), it runs the same `setup` on a thread with the login shell's environment, and shows a toast
saying which agents were connected. Only the released binary does this; a test's window never does.
Linux takes the same path. Windows does not, because its installer already did.

### 4.4 The API endpoints

`services::ai_endpoints::load` writes the file the first time it runs. At that moment it now also adds:

| Name | Wire | URL | Model | Key |
|---|---|---|---|---|
| `anthropic` | `anthropic` | `https://api.anthropic.com/v1/messages` | `claude-sonnet-5-5` | `ANTHROPIC_API_KEY` |
| `openai` | `responses` | `https://api.openai.com/v1/responses` | `gpt-5` | `OPENAI_API_KEY` |

only when that variable is set in the environment the window reads keys from (the login shell's on
macOS). The key is never written; the row names the variable, which is the rule the file already keeps.
A list that already exists is not changed, because a person may have removed a row on purpose.
A key set after the first start is added with `Add endpoint` on `Settings -> AI Configuration`, as any
other endpoint is.

### 4.5 The agent instructions

```
Download and install Unluminous on this machine. It is a code editor built for AI agents: anything I can
click in it, you can do through its command line tool or its MCP server.

1. Read https://unluminous.com/releases/latest.json. It has the address, size and SHA-256 of the Windows
   installer and of the macOS archive.
2. Download the one for my operating system and check that its SHA-256 matches.
3. On Windows, run the installer with /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER. It installs into
   %LOCALAPPDATA%\Programs\Unluminous, needs no administrator rights, and finishes by running
   unluminous-cli setup.
   On macOS, unzip the archive, move Unluminous.app to /Applications, and run
   /Applications/Unluminous.app/Contents/MacOS/unluminous-cli setup
4. Run unluminous-cli setup --check --json (on Windows the full path is
   %LOCALAPPDATA%\Programs\Unluminous\unluminous-cli.exe if this shell has not picked up the new PATH).
   It says where the command line tool is, which coding agents it connected, and whether the MCP server
   answered.
5. If you are a coding agent it did not list, connect yourself with the entry unluminous-cli mcp config
   prints.
6. Tell me the version you installed, where it is, which agents were connected, and that each agent has
   to be restarted to see the new tools.
```

### 4.6 Testing a fresh install without touching this machine's install

There is no Windows Sandbox on this machine (the feature is not enabled and enabling it needs an
administrator and a restart), Docker is not running, and there is no administrator to make a second
user. So the fresh environment is made inside this user, and the parts of this user's state the
installer would touch are saved first and put back after.

What the Windows installer writes outside its own folder, read off `unluminous.iss`:

- `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{64635361-...}_is1`, the record of the
  installed copy. Same key for a second copy, because the AppId is what makes an upgrade an upgrade.
- `HKCU\Environment\Path`.
- `HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths\unluminous.exe`.
- Under `HKCU\Software\Classes`: `*\shell\Unluminous`, `Directory\shell\Unluminous`,
  `Directory\Background\shell\Unluminous`, `Unluminous.Document`, `Applications\unluminous.exe`, and
  the `Unluminous.Document` value in ten `OpenWithProgids` keys.
- `Unluminous.lnk` in the Start menu's Programs folder and on the Desktop.

`tools/test-fresh-install.ps1`:

1. Exports every key above with `reg export`, copies both shortcuts, and records the main install's
   files with their hashes. All of it goes to `_agent_output/fresh-install/<time>/before/`.
2. Deletes the uninstall record, so the installer sees a machine with no Unluminous on it and takes its
   defaults rather than the tasks the last install chose. (Restored in step 8 whatever happens.)
3. Makes a fresh profile under the scratch folder: `USERPROFILE`, `HOME`, `APPDATA`, `LOCALAPPDATA`,
   `XDG_CONFIG_HOME` and `CODEX_HOME` all point inside it. `claude`, `codex` and `opencode` are put on
   its `PATH` from where they are installed, so the real agents are what gets connected, and each
   writes its configuration inside the fresh profile.
4. Runs the installer silently with `/DIR=<fresh profile>\Programs\Unluminous /LOG`, from that
   environment.
5. Checks: both programs are in the folder; `PATH` names the folder; `setup.log` exists; each agent's
   own command lists the server and says it connected (`claude mcp list`, `codex mcp list`,
   `opencode mcp list`), run in the fresh profile; and the fresh install's MCP server answers
   `tools/list`.
6. Starts the fresh `unluminous.exe` with `--background` on a scratch project (its own `APPDATA`, so it
   reads no settings of this machine) and has Claude Code, with this machine's own login and
   `--strict-mcp-config` naming only the fresh install's server, call one tool and say what the window
   answered. That is an agent driving a freshly installed Unluminous through the wiring the installer
   made.
7. Uninstalls the fresh copy silently and checks that the agent entries are gone and the folder is gone.
8. Imports every saved key, puts the shortcuts back, deletes the uninstall record only if the saved
   one did not exist, and compares a fresh export of every key with the saved one byte for byte, and
   the main install's files with their saved hashes. A difference fails the test.

Steps 1, 2 and 8 run in a `try`/`finally`, so a failure in the middle still puts this machine back. The
script refuses to start while an Unluminous installer is running, and it does not touch a running
Unluminous window: the main install's files are not in the folder being installed into.

macOS: there is no Mac on this machine. `setup`'s macOS half is covered by unit tests that run on every
platform where they can (choosing the folder, the profile line, the link decisions), and it compiles for
`aarch64-apple-darwin` through the cross toolchain the release already uses. It has not been run on a Mac
by this ticket, and the release notes say so.

## 5. Tests

- `unluminous-cli/src/setup.rs`: unit tests against a home folder of the test's own and a `PATH` of fake
  agent programs: detection, each of the four entry cases in §4.1 step 3 for all three agents, `--check`
  writes nothing, `--remove` takes out only our entries, running twice changes nothing, the macOS folder
  choice and profile line, the Windows `PATH` decision.
- `mcp/install.rs`: OpenCode's file location rules, the entry, keeping every other key, refusing a file
  that is not JSON.
- `ai_endpoints`: the two rows are added on first load only when their variables are set, and never to a
  list that exists.
- The catalogue, documentation and MCP tool tests that every command is held to.
- `tools/test-fresh-install.ps1`, run against the installer this ticket builds.

## 6. Left out

- **Gemini CLI, Cursor and VS Code.** Each reads an `mcpServers` file `setup` could write the same way.
  They were not asked for and none of them is installed here to test against. Adding one is a row in
  `Client` and a file location.
- **Writing a key anywhere.** The endpoints name variables, as they always have.
- **A Linux installer.** There is none; `setup` works there through the macOS path.
