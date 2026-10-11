Settings screen 5 of 9: Tools › MCP
Drawn by crates/unluminous-app/src/components/mcp_page.rs. Keys are lines in the settings file.

Each setting: Label | control | options or range | default | key

**Install**
Help above the buttons: "Writes Unluminous into the agent's own configuration. The agent then launches Unluminous's server itself over a pipe, so nothing listens on a port."
- "Install for Claude Code" / "Remove from Claude Code" | button, 196 wide; the label depends on whether Claude Code already has Unluminous | writes the agent's own config, not a setting
- "Install for Codex" / "Remove from Codex" | button, 196 wide | the same for Codex
Under the buttons, before any press: "Restart the agent afterwards. In Claude Code, `/mcp` then lists Unluminous's tools." After a press: what the button did.

**Server**
- Also serve over HTTP on this machine | tick box | | off | mcp.enabled
- Port: | text field, 84 wide; only a whole number is taken | 1024 to 65535 | 7345 | mcp.port
- Tools: | dropdown, on the same row as Port | One tool an area, One tool a command | One tool an area | mcp.tools (grouped, every)
- Areas: | text field, 300 wide | comma separated area names | empty (every area) | mcp.areas
Three notes under them:
1. "Which parts of the catalogue this offers, such as editor,git. Empty offers everything."
2. "<N> tools. One an area names every command Unluminous has for about a third of the context one a command costs; one a command lets an agent permit a single tool by name." (N is counted live.)
3. The server's state, one of: "Not listening. Agents that launch Unluminous's server themselves still work." / "Listening at <url>" / "Port <n> is already in use (<reason>). If that is another Unluminous, its server drives this window too." / "Unluminous was started with --control off, so there is no command channel for an MCP server to drive."

**Configuration** (nothing here is stored)
Help: "For an agent with no button above. Paste it into that client's own configuration file."
- "Claude Code" / "Codex" | two choice buttons, 118 wide; the chosen one is filled with the accent | default Claude Code | picks which block shows
- Copy | button, 60 wide, at the right of the same row | copies the block
- (block) | read only monospace block, 560 wide | JSON for Claude Code, TOML for Codex
