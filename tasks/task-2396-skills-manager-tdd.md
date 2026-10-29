# Skills Manager: the technical design

`task-2396`. Jason asked for a plugin that shows popular agent skills from `unluminous.com/skills`, lets a person
add and configure their own, installs any of them into every major coding agent's skills folder, for the
project or for everyone on the machine, and removes them again. The agent has a command for everything. The
skills for the Agent-Tasks board (`unluminous-task` and `unluminous-supervisor`) and the `unluminous` skill sit at
the top of the list and can be edited. The research behind the list is in
`_agent_output/task-2396-skills/research/`.

## 1. The shape

Three parts, each with one owner.

1. **The skills themselves live in this repository**, under `crates/unluminous-app/skills/`. Each skill is a
   folder in the Agent Skills format: a `SKILL.md` with YAML frontmatter (`name`, `description`) and any
   scripts, references or assets beside it. `skills/catalogue.json` says what the site and the plugin need to
   know about each one that the files do not: the display name, the author, the licence, the source repository,
   the category, the tags, the GitHub stars, the popularity evidence and the picture. The list is the 75 most used
   skills of the last three months (by skills.sh installs and GitHub stars) and the three Unluminous makes, and
   every summary says what the skill does and why that helps. Every file under `skills/` is compiled into
   the binary by `build.rs`, so a fresh install shows all of them with no network.
2. **The site publishes the same folder.** `sites/unluminous/scripts/sync-skills.mjs` in
   black-rainbow-labs-sites copies `skills/` from the Unluminous checkout into `public/skills/`, works out a
   hash for each skill, and writes `public/skills/catalogue.json`. The `/skills` page is built from that file.
   `https://unluminous.com/skills/catalogue.json` is what a running Unluminous asks for to find newer skills.
3. **The plugin, `skills`**, is a `ui` plugin whose provider is `services::skills`. It draws one pane (the
   Skills Manager), a menu, a Settings page, and answers `plugins run skills <verb>`.

```
 crates/unluminous-app/skills/  ──build.rs──▶ binary (bundled snapshot)
          │                                         │
          └──sync-skills.mjs──▶ unluminous.com/skills/catalogue.json + files ──fetch when the pane shows──┐
                                                                                                         ▼
                       services::skills::Skills (provider)  ◀── plugins run skills … ── agent / CLI / MCP
                         │  library: <plugin folder>/library/<id>/        (the person's working copy)
                         │  harness folders: ~/.claude/skills, .agents/skills, …   (installed copies)
                         ▼
                       components::skills (pane: cards, detail, editor actions)
```

## 2. Where a skill can come from

| Source | Where its files are | Can be edited | Can be deleted |
|---|---|---|---|
| `unluminous` | in the binary, and the site | yes, into the library | no, reset only |
| `catalogue` | in the binary (the snapshot) and the site's newer copy | yes, into the library | no, reset only |
| `mine` | the library only | yes | yes, to the Recycle Bin |
| `found` | a harness folder on this machine that Unluminous did not write | yes, in place | removed, to the Recycle Bin |

**The library is the person's working copy.** `<settings folder>/plugins/skills/library/<id>/` holds the files a
skill is installed from. A bundled or catalogue skill is written there the first time it is installed or edited,
with `.unluminous-skill.json` beside it recording the source and the hash of what was written. The skill is
**edited** when the hash of its files is not the recorded one. `Reset` writes the shipped files back.

**An update is a newer hash from the site.** The catalogue the site serves has a `sha256` per skill. When it
differs from the hash the binary or the library recorded for that skill, the card says `Update`. An unedited
skill takes the update when it is pressed; an edited one offers `Take the update` (the person's edits go to the
Recycle Bin first) and `Keep mine`. Downloaded files are kept in `<plugin folder>/catalogue/` and are never run.

The hash is SHA-256 over every file, sorted by path, as `path NUL bytes NUL`, with line endings turned into
`\n` first so a checkout on Windows and one on a Mac agree. The site script and `services::skills::hash` write
the same bytes; a test checks one against a fixed value the script also prints.

## 3. The harnesses

A harness is a coding agent that reads Agent Skills. Each has a folder for one project and one for the person,
and several read a shared folder. The table is `services::skills::harness::HARNESSES`; the sources are in
`_agent_output/task-2396-skills/research/harnesses.md`.

| Harness | Writes to, project | Writes to, global | Also reads |
|---|---|---|---|
| Claude Code | `.claude/skills` | `~/.claude/skills` (`CLAUDE_CONFIG_DIR`) | nothing else |
| Codex | `.agents/skills` | `~/.agents/skills` | |
| OpenCode | `.opencode/skills` | `~/.config/opencode/skills` | `.claude`, `.agents` |
| Pi | `.pi/skills` | `~/.pi/agent/skills` (`PI_CODING_AGENT_DIR`) | `.agents` |
| Gemini CLI | `.gemini/skills` | `~/.gemini/skills` | `.agents` |
| Cursor | `.cursor/skills` | `~/.cursor/skills` | `.agents`, `.claude` |
| GitHub Copilot | `.github/skills` | `~/.copilot/skills` (`COPILOT_HOME`) | `.claude`, `.agents` |
| Windsurf | `.windsurf/skills` | `~/.codeium/windsurf/skills` | `.agents` (project) |
| Amp | `.agents/skills` | `~/.config/agents/skills` | `.claude`, `~/.agents/skills`, `~/.config/amp/skills` |
| Kiro | `.kiro/skills` | `~/.kiro/skills` | |
| Cline | `.cline/skills` | `~/.cline/skills` | `.clinerules/skills`, `.claude` (project) |
| Roo Code | `.roo/skills` | `~/.roo/skills` | `.agents` |
| Goose | `.agents/skills` | `~/.agents/skills` | `.goose/skills`, `.claude` (project) |
| Factory Droid | `.factory/skills` | `~/.factory/skills` | `.agents` |
| Junie | `.junie/skills` | `~/.junie/skills` | `.agents` |
| Qwen Code | `.qwen/skills` | `~/.qwen/skills` | `.agents` |
| Zed | `.agents/skills` | `~/.agents/skills` | |

Gemini CLI was replaced by Antigravity CLI in June 2026; its folders are kept for the copies people still run.
Windows paths are the same relative paths under `%USERPROFILE%`, which only Cline and Junie document.

Rules:

- **Installing copies the skill's folder to `<harness folder>/<id>/`** and adds `installed-by: unluminous` to
  the frontmatter of the copy's `SKILL.md`, the mark `mcp::skill` already uses for its own file. Agents ignore a
  key they do not know.
- **A folder Unluminous did not write is never replaced or removed by an install.** Install answers that the
  folder is somebody else's and names it; `install --force` replaces it after moving it to the Recycle Bin.
- **Two harnesses that read one folder share one copy.** Installing for Codex puts the skill in `.agents/skills`,
  and the card shows Codex and every other harness that reads that folder as installed.
- **Editing a library skill reaches every copy.** Every two seconds, while the plugin is open, a library skill
  whose files changed is copied again into each place it is installed with the mark. That is what makes "edit the
  Markdown and the agent sees it" true without a second button.
- **Removing** takes the copy out of the harness folder, to the Recycle Bin on Windows.
- `UNLUMINOUS_HOME` is the home folder in a test, which is the seam `mcp::install::home` already reads, so no
  test touches the person's own agents.

## 4. The Unluminous skills

Three skills ship first and are drawn first, under the heading `Unluminous`:

| id | What it is |
|---|---|
| `unluminous` | The skill `mcp install` already writes: which Unluminous tool does which job. Its text stays `unluminous-cli/skills/unluminous/SKILL.md`; the skills folder holds a copy that a test keeps equal. |
| `unluminous-task` | Pick up one ticket on the Unluminous Agent-Tasks board, plan it as todos, work it, comment, and hand it back as `agent_done`, through `unluminous-cli plugins run agent-tasks`. |
| `unluminous-supervisor` | Work the board: QA Failed first, then New, one ticket at a time, each in its own agent, watching and reporting. |

Their names start with `unluminous` so that installing them globally never replaces a person's own `task` skill.

## 5. The site

`/skills` on unluminous.com, in the site's own language (DESIGN.md: the light ground, Inter Tight, the dark
rounded panels). A header with the count and a search box, chips for the categories, then a grid of cards: the
picture, the name, the author, the summary, the tags, and the install line with a copy button. The Unluminous
skills come first in a row of their own. A card opens a dialog with the description, the popularity evidence,
the source and licence, the install command for each harness, and the full `SKILL.md`.

The install line is the one command that works everywhere: `unluminous-cli plugins run skills install <id>` when
Unluminous is running, and beside it the plain copy for people without it, `npx skills add <repo>` where the
upstream project publishes one, and a folder path otherwise.

Pictures are 1200 by 800, made with Krea 2 from the `imagePrompt` the research wrote, and kept in
`public/images/skills/` (ignored by git, copied into the main checkout before a deploy, as every picture is). The
binary holds a 480 by 320 JPEG of each.

## 6. The pane

The Skills Manager pane docks on the right, 760 points wide. From the top:

1. **Header**: the blocks mark, `Skills`, the count, `Check for Updates` (a chip) and `New Skill` (the primary pill).
2. **A search well** and a pill switch: `All`, `Installed`, `Mine`, `Updates`.
3. **Where to install**: a pill switch `This project` and `Everyone` (global), and the harnesses that are on
   (from Settings), each a small chip with its mark.
4. **The grid of cards**, as wide as the pane allows (one to four columns, cards at least 220 points). A card is
   the Agent-Tasks card's surface (`chat.card` gradient, `chat.raised()`, 20 point radius): the picture across
   the top with rounded top corners, the name in 13 point semibold, the author in the quiet ink, two lines of
   summary, the tags as carved chips, and a foot with one small disc per harness lit where it is installed and a
   round button: `+` installs into the chosen scope for every harness that is on, a tick says it is installed
   there, an arrow says there is an update. `Unluminous` skills are under their own heading first, then the
   catalogue by popularity, then `Mine`, then `Found on this machine`. Every skill is a card. A skill with no
   picture (every one of the person's own and every one found on the machine) has the stand in: a dark ground with
   a glow in a colour worked out from its name and the blocks mark over it (`components::skills::paint_placeholder`),
   on its card and as its page's banner.
5. **A card opens the skill's page in the pane**: a back chip, the picture as a banner, the name, author and stars,
   the description, `View / Edit` (the `SKILL.md` in a tab), `Update` and `Reset` when they apply, and two round
   icon buttons, Reveal and the trash (which removes the skill from every agent, or deletes one of the person's own,
   and is dimmed while the skill is installed nowhere). Under them, the agents found on this machine, one
   `Project` or `Global` switch for the list, and one `Install` or `Remove` button an agent. The `SKILL.md` is not
   printed on the page; Jason asked for it to be reached through `View / Edit`.

`New Skill` asks for a name and a one line description, writes `library/<name>/SKILL.md` from a starter, and opens
it in a tab. `Edit SKILL.md` opens the library copy in a tab (writing it into the library first for a bundled
skill), or the found file in place.

## 7. Commands

`plugins run skills <verb>`, listed in `unluminous_cli::plugins::SKILLS`:

| Verb | What it does |
|---|---|
| `open-pane` | Show the Skills Manager. |
| `list [all\|installed\|mine\|updates\|unluminous\|found]` | The skills, with where each is installed. |
| `show <id>` | One skill: its fields, its files, its installs, and whether it is edited or has an update. |
| `text <id>` | The `SKILL.md`. |
| `write <id> <text>` | Replace the `SKILL.md` of a library skill, which reaches every copy. |
| `harnesses` | The harnesses, their folders here, which are on, and which exist on this machine. |
| `install <id> [harness…\|all] [--project\|--global] [--force]` | Install. With no harness, the ones that are on. |
| `remove <id> [harness…\|all] [--project\|--global]` | Remove. |
| `new <name> [description]` | Make a skill and open its `SKILL.md`. |
| `edit <id>` | Open the `SKILL.md` in a tab. |
| `delete <id>` | Delete one of mine, to the Recycle Bin. |
| `reset <id>` | Put the shipped files back. |
| `check` | Ask the site for the newest catalogue now. |
| `update <id\|all> [--discard]` | Take the newer files. |
| `scope project\|global` | The scope the pane installs into. |
| `search <words>` | The skills whose name, summary or tags hold the words. |
| `view <id\|list>` | Show a skill's page in the pane, or the grid. |

## 8. Files

| File | What is in it |
|---|---|
| `crates/unluminous-app/skills/catalogue.json`, `skills/<id>/…`, `skills/images/<id>.jpg` | The skills. |
| `crates/unluminous-app/build.rs` | Writes `$OUT_DIR/skills_bundle.rs`, a table of every file under `skills/`. |
| `crates/unluminous-app/plugins/skills/plugin.conf`, `icon.png`, `icon-128.png`, `icon.md` | The manifest and icon. |
| `services/skills/mod.rs` | `Skills`, the provider. |
| `services/skills/catalogue.rs` | Reading `catalogue.json`, the bundle, the downloaded copy, and the hash. |
| `services/skills/harness.rs` | The harness table and the folders. |
| `services/skills/library.rs` | The library, edits, reset, new skills. |
| `services/skills/install.rs` | Installing, removing, finding what is installed, keeping copies in step. |
| `services/skills/frontmatter.rs` | Reading and writing a `SKILL.md`'s frontmatter. |
| `services/skills/commands.rs` | The verbs. |
| `services/skills/tests.rs` | Unit tests. |
| `components/skills/{mod,pane,card,detail,kit}.rs` | The drawing. |
| `unluminous-cli/src/plugins.rs` | `SKILLS`, and the plugin's entry. |
| `tests/skills.rs` | Window tests and pictures. |

## 9. What it does not do

- It runs nothing in a skill. A script in a skill's folder is copied, never executed.
- It fetches nothing until the pane is shown (`read_when_shown`, the ACP registry's rule), and a test's window
  fetches only from `UNLUMINOUS_SKILLS_CATALOGUE` when that names a file.
- It does not publish a person's own skill to the site.
- A harness not in the table can still be used through `Settings -> Skills -> Other folders`, a list of folders to
  install into.
