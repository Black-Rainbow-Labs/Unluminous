# task-2556: an editable supervisor skill, and the supervisor's own model

## What was asked

> In agent tasks, when I click the supervisor and the modal pops up, I want a tab to show an editable supervisor
> skill file so I can modify just like in file preview pane, and saving it immediately gets applied as the skill
> the supervisor uses. [...] The goal is to be able to have users customize how the supervisor processes the
> board. Eg they may want at most 3 tasks worked on at a time, or to only do one at a time. Or perhaps they want
> the supervisor to verify the work. [...] We also want the user to be able to configure which model is used for
> the supervisor (perhaps in settings or a gear icon in our supervisor modal).

## What is there today

- `Chat with supervisor` opens `components::agent_tasks::supervisor_modal`, drawn with the old
  `components::modal` furniture: a header, the supervisor's terminal, a message box with `Send`, and a footer with
  `Start supervisor` / `Stop supervisor` and `Close`. It does not look like the ticket modal next to it.
- `AgentTasks::start_the_supervisor` launches the board's default endpoint (`Settings -> Agent-Tasks -> Endpoint`)
  with that endpoint's model. The supervisor has no model of its own.
- The supervisor is told how to work the board by one line, `agent::supervisor_handoff`, which hard codes "one
  at a time". Nothing a person writes can change that.
- `crates/unluminous-app/skills/unluminous-supervisor/SKILL.md` is the copy of the ai-service `/process-tasks`
  skill made for this board (`task-2396`). The Skills Manager ships it and installs it into agents' skill
  folders, but the supervisor the board starts is never told to read it.

## Design

### The skill file

- **The board's supervisor skill is one file**: `<plugin folder>/supervisor/SKILL.md`, beside `settings.conf`
  in `plugins/agent-tasks/`. A window with no settings folder (a test's) keeps it under
  `%TEMP%/unluminous-agent-tasks-<pid>/supervisor/SKILL.md`, so a test never writes into the person's settings.
- **The baseline is the shipped `unluminous-supervisor` skill**, compiled in with `include_str!` as
  `supervisor::DEFAULT_SKILL`. The file does not exist until somebody saves or the supervisor starts; until then
  the editor shows the baseline. `Reset to default` writes the baseline back.
- The shipped `SKILL.md` is not changed. Its hash is in `skills/catalogue.json` and on unluminous.com, so editing
  it would need the site's skills sync and a new snapshot. A person's changes live in the board's own file.
- **The supervisor is told to follow the file.** `agent::supervisor_handoff(agent, path)` keeps the board's
  commands, drops the hard coded "one at a time", and says: read the skill at `<path>` before anything else,
  follow it, and where it differs from this message the file wins. `start_the_supervisor` writes the baseline to
  the file first when it is missing, so the path in the handoff always exists.
- **Saving applies at once.** `save_the_supervisor_skill` writes the file atomically and, when the supervisor is
  running, types `agent::skill_changed(path)` into its terminal: the skill was changed, read it again now and follow
  it from here on. When the supervisor is not running, the next start reads the saved file.
- **A file changed outside the modal is read again.** The modal opens the file in a tab with `Open in a tab`. While
  the Skill tab shows a draft with no unsaved changes, the file's modified time is checked once a second and a
  changed file replaces the draft.

### The supervisor's model

Three new settings in `settings.conf`, all optional, all on `Configuration`:

| Key | Field | Empty means |
|---|---|---|
| `supervisor-endpoint` | `supervisor_endpoint` | the board's default endpoint, which is what starts today |
| `supervisor-model` | `supervisor_model` | the endpoint's own model |
| `supervisor-effort` | `supervisor_effort` | the agent's default effort |

`AgentTasks::supervisor_agent()` resolves the endpoint by name with `assignee_named`, ignores `human`, and falls
back to `default_assignee()`. `start_the_supervisor` passes the model and effort into `agent::Plan`, the same way a
ticket's own model and effort are passed. A change takes effect on the next start; the modal says so and offers
`Restart supervisor` while one is running.

They are set in two places, which write the same three fields:

- **The gear in the supervisor modal's header** opens the modal's Settings view: Endpoint, Model and Effort as the
  ticket modal's `Select::chip` fields, a sentence saying when a change takes effect, and `Restart supervisor`.
- **`Settings -> Agent-Tasks`** gets a fourth card, `The supervisor`, with the same three selects.

### The modal

The modal is redrawn in the Agent-Tasks design with `rux`, the way `ticket_modal` is: `modal::show_unframed`, a
thirty point surface, the ticket modal's header row and section language.

```
+--------------------------------------------------------------------------------------+
| (Supervisor * running)  Chat with supervisor        [ Terminal | Skill ]  (gear) (x) |
|                                                                                      |
|  Terminal view:                                                                      |
|  +--------------------------------------------------------------------------------+  |
|  | the supervisor's terminal                                                      |  |
|  +--------------------------------------------------------------------------------+  |
|  [ Message to the supervisor ............................................ ] (Send)   |
|                                                         (Stop supervisor)            |
|                                                                                      |
|  Skill view:                                                                         |
|  SKILL.md  plugins/agent-tasks/supervisor/SKILL.md  * Unsaved   [Edit|Split|Preview] |
|  +-------------------------------------+  +---------------------------------------+  |
|  | the markdown source, code face      |  | the rendered preview                  |  |
|  +-------------------------------------+  +---------------------------------------+  |
|  (Open in a tab)  (Reset to default)                       (Revert)  (Save & apply)  |
|                                                                                      |
|  Settings view (the gear): Endpoint, Model, Effort, a note, (Restart supervisor)     |
+--------------------------------------------------------------------------------------+
```

- The header chip says `Supervisor` with a dot, green while it runs. The tabs are the ticket modal's carved track
  (`Terminal tab`, `Skill tab`). The gear is a `ChipButton` drawn on while the Settings view shows; a tab press
  leaves Settings.
- The Skill view is the file preview pane's arrangement: source and rendered Markdown side by side in `Split`,
  either alone in `Edit` and `Preview`. The source is an `egui` text box in the code face inside a carved well;
  the preview is `components::markdown_text`, the renderer the description and every `.md` preview use.
- `Save & apply` is the primary pill, usable only with unsaved changes, and `Ctrl`/`Cmd`+`S` in the source does
  the same. `Revert` puts the draft back to the saved file. `Reset to default` puts the baseline in the draft and
  saves it. A line under the buttons says what the last save did.
- Closing the modal keeps an unsaved draft in memory, so reopening shows it with `Unsaved` beside the name.

### The agent's half

New words after `plugins run agent-tasks supervisor`, all in `commands::supervisor`:

| Command | Does |
|---|---|
| `supervisor skill` | the path, the saved text, whether it is the baseline, and whether there is an unsaved draft |
| `supervisor skill-set <text>` | saves `<text>` as the skill and applies it, exactly as `Save & apply` |
| `supervisor skill-reset` | writes the baseline and applies it, exactly as `Reset to default` |
| `supervisor view terminal\|skill\|settings` | which view the modal shows |
| `supervisor layout edit\|split\|preview` | how the Skill view is laid out |
| `supervisor endpoint [name\|default]` | reads or sets `supervisor-endpoint` |
| `supervisor model [name\|default]` | reads or sets `supervisor-model` |
| `supervisor effort [level\|default]` | reads or sets `supervisor-effort` |
| `supervisor restart` | stops and starts it, so a new endpoint or model takes effect |

`supervisor` with nothing after it also answers `endpoint`, `model`, `effort` and `skill` (the path).
`unluminous-cli/src/plugins.rs` describes the new words, so the generated `unluminous_agent_tasks` tool offers them.

## Code

| File | Change |
|---|---|
| `services/agent_tasks/supervisor.rs` (new) | `DEFAULT_SKILL`, `SupervisorView`, `SkillLayout`, `SkillEditor`, and every supervisor method moved out of `mod.rs`: start, stop, send, restart, the skill's path, read, save, reset, reload, and the three settings |
| `services/agent_tasks/configuration.rs` | the three fields, read and written |
| `services/agent_tasks/agent.rs` | `supervisor_handoff(agent, path)` and `skill_changed(path)` |
| `services/agent_tasks/commands.rs` | the new `supervisor` words |
| `components/agent_tasks/supervisor_modal.rs` | rewritten in `rux`: header, tabs, gear, the three views |
| `components/agent_tasks/settings_page.rs` | the `The supervisor` card |
| `unluminous-cli/src/plugins.rs` | the `supervisor` description |
| `documentation/agent-panes.md`, `CLAUDE.md` | what changed and the rules it left |

## Tests

- Unit tests in `supervisor.rs`: the baseline before any save; save writes the file and a second board reads it;
  reset writes the baseline; the handoff names the file; `skill_changed` names the file; a saved setting round
  trips through `settings.conf`; `supervisor_agent` falls back to the default endpoint and ignores `human`.
- Window tests in `tests/plugins_board_and_chat.rs`: the modal has `Terminal tab`, `Skill tab` and the gear; the
  Skill tab shows the source box, `Save & apply` and `Reset to default`; typing into the source and pressing
  `Save & apply` writes the file that `supervisor skill` reads back; the gear shows Endpoint, Model and Effort and
  `supervisor model <name>` is read back from the board's settings. The existing
  `chat_with_supervisor_opens_a_modal_with_its_terminal_a_message_box_and_send` keeps passing.
- In a real window: start the supervisor, change the skill to "at most 3 tickets at a time", save, and read the
  supervisor's terminal for the line telling it to read the file again and for it reading the file.

## QA pass

Jason's comment after 1.27.0:

> I want the edit skill in the modal to be an actual file editor, with line numbers etc, just like in file editor
> pane. If I click open in tab, it should close the modal and ensure the file editor pane is open with the file.
> The icon for supervisor should be changed to look like FaPersonMilitaryPointing. Skill tab should be renamed
> Supervisor Skill. Settings for the supervisor modal shouldn't have an edit the skill button/section. Supervisor
> icon should get moved to the very left of the pane, to the left of the tab nav for Board, epic, etc.

What changed:

- **The editor is the window's.** A plugin cannot draw the editing area: the documents, the gutter, coloring, folding
  and undo belong to `UnluminousApp`. So the plugin contract gained `UiProvider::take_the_embedded_file`, which names a
  file, the modal's layer and a rectangle, and `embedded_file_shown`, which the window answers with whether the file
  has unsaved changes. `app::modal_editor` draws the tab there with `show_a_document_in`. The tab lives in a new home,
  `files::Home::Modal`, beside `Pane` and `Node`. The egui text box, the hand drawn preview and the draft the modal
  used to keep are gone.
- **Save, Revert and Reset** are `Request::SaveFile`, `Request::ReloadFile`, and a write followed by `ReloadFile`.
  `Ctrl`/`Cmd`+`S` in the editor is the window's own `Save`, because the focus stays on `Home::Modal` while the editor
  has the keyboard. `controls::claim_chords`, which the text box needed, was removed.
- **Applying a save** is one watch of the file's modified time, run from the board's `catch_up` once a second, so a save
  from the modal, from a tab, from `supervisor skill-set` or from another program all reach a running supervisor.
- **`Open in a tab`** closes the modal and moves the tab into the pane that last had the keyboard, with the editing area
  shown. Opening the file from the explorer while it lives in the modal does the same.
- **The supervisor button** is the first control in the board's header, before the view switch, in every view. Its mark
  is Font Awesome 6 `person-military-pointing`, drawn by `theme::svg_icon::paint_filled`.
- **The tab is `Supervisor Skill`**, and the gear's card no longer mentions or links to the skill.
- New agent verbs: `supervisor skill-save`, `skill-revert` and `skill-open`, the three buttons.
- Tests: `the_supervisor_skill_tab_is_the_windows_editor_on_the_skill_file` types into the editor, saves with
  `Ctrl`/`Cmd`+`S`, reverts and opens the file in a tab; `the_supervisor_button_is_at_the_left_of_the_boards_header`;
  and unit tests for the watch and the two requests.
