Settings screen 7 of 9: Plugins › Agent-Chat
Code: components/agent_chat/settings_page.rs. Keys are lines in plugins/agent-chat/settings.conf. The page scrolls.

Each setting: Label | control | options or range | default | key

**Endpoints**
Help: "What answers. `claude-cli` and `codex-cli` run the agent already installed on this machine: they need no key, they bring their own tools, and they read the open project's own instructions. `openai`, `anthropic` and `responses` send to a URL instead — for those a key is never stored by Unluminous, so name the environment variable it is in."
One card per endpoint (N = row number); the card in use has an accent border:
- (name) | text field | | | provider.N.name
- Use | button; "In use" pill on the card in use | | | chosen
- Remove | button; absent on the only card
- Speaks | choice buttons, 74 wide | claude-cli, codex-cli, openai, anthropic, responses (uninstalled programs hidden) | | provider.N.wire
- Program | text field, hint "claude" | only on claude-cli / codex-cli cards | | provider.N.command
- URL | text field, hint "https://…" | only on address cards | | provider.N.url
- Model | text field, hint "the agent's own" (program) or "the model's name" (address) | | empty | provider.N.model
- Key from | text field (env variable name), hint ANTHROPIC_API_KEY / OPENAI_API_KEY | only on address cards | | provider.N.key-env
- Most tokens | text field | 64 to 200,000; only on address cards | 4096 | provider.N.max-tokens
- (status) | dot and a line. Green: "Ready · found on this machine, and it holds its own account" / "Ready · key set from $<VAR>" / "Ready · no key needed". Red: why it cannot be reached
Under the cards: "Add endpoint" button (adds an openai card).
Shipped cards: claude (claude-cli), codex (codex-cli), local (openai, http://127.0.0.1:8080/v1/chat/completions, model local).

**Answering** (depends on the card in use)
If it runs a program. Help: "The chosen row is a program on this machine, so it answers with its own tools, its own sandbox and its own account. It runs in the project this window has open and reads that project's own instructions."
- May | three choice buttons, 84 wide | read, edit, full | full | permission
Line under it, by value. read: "Look and answer, changing nothing. `codex` runs in a read-only sandbox; `claude` refuses any tool of its own that would change something." edit: "Change files in the project this window has open. `codex` is sandboxed to it; `claude` accepts its own edits without asking." full: "Anything at all: edit any file and run any command, with no sandbox and nothing to ask. This is what ships, because the agent on the other end is the one already signed in on this machine and trusted with this checkout."
If it sends to an address:
- Let the model use Unluminous's own commands | tick box | | on | tools
Help: "On unless you turn it off. The model is offered Unluminous's whole command catalogue as tools, so it can open a file, read the git status or run a search — through exactly the code a menu entry runs. The commands that run a program of the model's choosing are the switch below, and that one is off. The same switch is the first button in the composer."
- …including the ones that run a program | tick box | | off | shell
Help: "A second switch, because typing into a terminal, starting a run configuration and installing a debug adapter hand this machine's shell to whatever is on the far end of that URL."
- Tool rounds | text field, hint 100 | 1 to 500 | 100 | tool-limit
Help: "How many times in one turn the model may ask for tools before the pane stops asking again."

**Conversation**
- System | one line text field, hint "nothing" | | empty | system
Help: "Added after Unluminous's own line, which says which file is showing. The file's text is never sent by Unluminous."
- History | text field, hint 20 | 1 to 500 | 20 | history
Help: "How many conversations are kept."

**Not built yet**
- provider.N.key-entry (keychain entry for a key) is stored but has no field (services/agent_chat/mod.rs).
