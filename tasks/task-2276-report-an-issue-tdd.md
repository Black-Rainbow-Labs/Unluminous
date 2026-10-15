# task-2276: reporting an issue from inside Unluminous, and the rest of the modals in the redesign

## What was asked

> We want users of Unluminous to be able to easily report issues/bugs/feature requests to our github
> through the app. Research online how opencode does this, and follow a similar approach. At the bottom
> right, we want a flag icon that brings up a modal where user can type their issue and submit it to our
> github page. The flag should look similar to `IoFlagOutline` from react-icons/io5. The modal should be
> styled like our other modals like settings, task, etc. Make sure the user can supply screenshots, logs,
> etc, and the modal is well designed. We want as little friction as possible for the user, as we value
> bug reports and issues. Additionally we need to finish restyling our other modals to look like our
> redesign. Eg git modals are the old style we want to enhance.

Two pieces of work:

1. A flag at the bottom right of the window, a `Report an Issue` modal, and the path from that modal to
   an issue on `github.com/Black-Rainbow-Labs/Unluminous`, which is the public repository (issues are
   switched on there; `jasonmcaffee/unluminous` is private, and nobody else can see it).
2. Every modal still drawn with the old furniture, redrawn in the language the Settings window, the
   ticket modal and the New Data Source dialog use.

## 1. How opencode does it

Read from `anomalyco/opencode` (the repository `sst/opencode` now redirects to), branch `dev`:

- `.github/ISSUE_TEMPLATE/bug-report.yml` and `feature-request.yml` are GitHub issue forms. The bug form
  has a required `description`, and optional `plugins`, `opencode-version`, `reproduce`,
  `screenshot-or-link`, `os` and `terminal` fields. `config.yml` sets `blank_issues_enabled: false`.
- The desktop app's Help menu (`packages/app/src/shell/commands/desktop-menu.ts`) has `Report Bug` and
  `Share Feedback` entries whose `href` is `https://github.com/anomalyco/opencode/issues/new?template=…`.
  It also has `Export Logs`, a separate command, because a link cannot carry a file.
- The TUI's crash screen (`packages/tui/src/component/error-component.tsx`, `buildIssueURL`) builds the
  same address and fills the form's fields from the query string: `title`, `opencode-version`, `os`,
  `terminal`, `reproduce`, and the stack trace. It measures the stack against the **encoded** length
  of the whole address and cuts it short with a marker, so the address stays under the length GitHub
  accepts.

So opencode never sends anything itself. It opens the person's browser on a new issue with as much as
possible already written, and the person presses GitHub's own Submit while signed in as themselves. A
picture or a log file cannot travel in an address, so opencode asks for them to be dragged into the form
(`Paste link or drag and drop screenshot here`).

### Why Unluminous follows it rather than posting the issue itself

Posting directly would need a GitHub token. A token in the binary would be a credential anybody can
extract, and anybody holding it could write issues, labels and comments as Black Rainbow Labs. A server
in front of GitHub holding the token would be a service to run, a secret to keep, and something to rate
limit against abuse, for a feature whose whole value is reports from people who are already on GitHub.
The issue should also be from the person who filed it, so they are told when it is answered. Unluminous
holds no keys anywhere else (`CLAUDE.md`, the chat pane section), and this keeps that true.

### Where Unluminous goes further than opencode

What costs a person effort in opencode's flow is the part after the browser opens: finding the
screenshot, finding the log, dragging each one in. Unluminous does that part:

- **The screenshot is already taken.** Pressing the flag photographs the window before the modal opens,
  and the picture is attached, with a thumbnail, and can be removed with one press. More pictures can be
  added from a file, by pasting one, by dropping files on the modal, or by `Take screenshot` (which
  photographs the window with the modal hidden for that frame).
- **The files are put on the system clipboard as files** (`CF_HDROP` on Windows, file addresses on the
  macOS pasteboard, the same calls `services::system_files` makes for the folder pane). Chromium and
  Firefox both upload files pasted into a GitHub text area, so attaching every file is one `Ctrl`+`V`
  in the issue body. The folder holding them is also opened beside the browser, for a person who would
  rather drag.
- **Logs are written into the issue as text** when they fit, inside a `<details>` block, so most reports
  need nothing pasted at all. A log that does not fit is attached as a file and the body says so.
- **The draft is kept.** Closing the modal by accident keeps everything typed and attached until it is
  sent or thrown away with `Discard`.

No issue template is added to the repository. An address with `title` and `body` opens GitHub's blank
issue form already filled, and a template would make GitHub show its template chooser instead unless
the address names one, which is exactly the fault Codex CLI shipped (its `/feedback` named a template
that had been renamed, and every report arrived empty). Labels in the address are applied only when the
person can triage, so the kind of report is also written into the title as a prefix.

## 2. What the person sees

### The flag

`theme::icon::flag`, drawn from `IoFlagOutline`'s geometry (Ionicons 5 `flag-outline`, a 512 unit
viewBox): a pole from the bottom left up to the top, and a flag whose top and bottom edges are each one
wave, stroked rather than filled. It sits at the right hand end of the status bar, 12 points across in
a 22 point round hit area, after the font. The status bar's right hand end moves left to make room for
it. It is named `Report an issue`, has the tooltip `Report an issue or request a feature`, and turns the
accent colour under the pointer.

The same thing is `Unluminous -> Report an Issue...` (and so `unluminous-cli action run "Report an
Issue..."`), and `Action::ReportIssue`.

### The modal

`components::report_dialog`, 720 by 640 points, `modal::show_unframed`, drawn with `settings_kit` and
`components::realm::dialog`'s heading, footer and surface, so it is the same object as the New Data
Source dialog and the realm manager:

```
 ┌──────────────────────────────────────────────────────────────┐
 │  Report an issue                                         (×) │
 │  Goes to github.com/Black-Rainbow-Labs/Unluminous. Your      │
 │  browser opens with everything below filled in.              │
 │ ╭────────────────────────────────────────────────────────╮   │
 │ │ Kind          ( Bug | Feature request | Question )      │   │
 │ │ Title         ( One line that says what went wrong   )  │   │
 │ │ Description   ╭───────────────────────────────────╮     │   │
 │ │               │ What happened, and what you        │     │   │
 │ │               │ expected instead.                  │     │   │
 │ │               ╰───────────────────────────────────╯     │   │
 │ ╰────────────────────────────────────────────────────────╯   │
 │ ╭────────────────────────────────────────────────────────╮   │
 │ │ Attachments                       [Take screenshot] [+] │   │
 │ │ ╭──────╮ ╭──────╮ ╭──────────────╮                      │   │
 │ │ │ img  │ │ img  │ │ crash.log 2KB│   Drop files or paste │   │
 │ │ ╰──────╯ ╰──────╯ ╰──────────────╯                      │   │
 │ │ Include logs              [on]  crash log, last 200 l.  │   │
 │ │ Include version and system [on] Unluminous 0.86 · Win 11 │   │
 │ ╰────────────────────────────────────────────────────────╯   │
 │                                   Discard   Cancel  (Open on GitHub) │
 └──────────────────────────────────────────────────────────────┘
```

- **Kind** is a segmented track: `Bug`, `Feature request`, `Question`. The title prefix and the label
  follow it (`[Bug]`, `bug`; `[Feature]`, `enhancement`; `[Question]`, `question`).
- **Title** is a carved well and is the one required field. It takes the keyboard when the modal opens.
- **Description** is a carved text area whose hint changes with the kind. A bug's hint asks for what
  happened, what was expected and the steps.
- **Attachments** is a carved well of chips. A picture is a 64 point thumbnail with its name under it; a
  text file is a chip with its name and size. Every chip has a round `×` that removes it. `Take
  screenshot` photographs the window again, `Add files` opens the platform's file picker (several at
  once), a file dropped anywhere on the window while the modal is open is attached, and `Ctrl`/`Cmd`+`V`
  with a picture on the clipboard and the keyboard not in a text box attaches it.
- **Include logs** attaches the end of `crash.log` (the last 200 lines of `crash.log` and
  `crash.log.old` together, newest last) and the status bar's last message. Off when there is no crash
  log and nothing to say; the switch then says `No logs yet`.
- **Include version and system** writes the version, the build date, the operating system and its
  version, the processor architecture, the interface theme and the language plugins that are on into
  the issue as a table. The line beside the switch says exactly what goes, so nothing is sent unseen.
- **The footer** is `Discard` (a ghost button, thrown away with no question because nothing has been
  sent), `Cancel` (closes and keeps the draft) and the primary `Open on GitHub`, which is dimmed until
  there is a title. `Ctrl`/`Cmd`+`Enter` presses it, because Enter is a new line in the description.

### After `Open on GitHub`

The same modal shows the second step instead of the form:

```
 │  Almost there                                            (×) │
 │  ① Your browser has the issue open, already filled in.        │
 │  ② Click in the description and press Ctrl+V to attach the    │
 │     3 files. They are on the clipboard.                       │
 │  ③ Press Submit new issue.                                    │
 │                                                                │
 │  [Copy files again] [Show files] [Open the issue again]        │
 │                                                  (Done)        │
```

When there is nothing to attach, step ② is left out. `Done` throws the draft away.

## 3. How it is built

### `services::issue_report` (no window, no drawing)

- `Kind { Bug, Feature, Question }` with `prefix`, `label`, `word`.
- `Attachment { path, name, bytes, picture: bool }`.
- `Report { kind, title, description, attachments, include_logs, include_system }`.
- `System::read()` answers the version, the build date, the operating system (`os_version()`, from the
  registry's `CurrentVersion` key on Windows, `NSProcessInfo.operatingSystemVersionString` on macOS,
  `/etc/os-release` on Linux), the architecture, the theme and the plugins.
- `logs(settings_folder, limit)` reads the end of the crash log.
- `body(report, system, logs, attached_names)` writes the issue's Markdown.
- `address(report, body)` is the issue address. It is built with an encoder of its own (RFC 3986
  unreserved characters pass, everything else is `%XX` of its UTF-8 bytes) and is kept under
  `ADDRESS_LIMIT` (8,000 bytes), GitHub's practical limit, by cutting the logs first and then the
  description, each with a line saying it was cut and that the whole of it is attached.
- `prepare(report, folder)` writes every attachment, and a log that did not fit, into
  `<temp>/unluminous-report-<time>/`, and answers the files.

### The flow in the window (`app/report.rs`)

- `UnluminousApp::report: Option<ReportState>` holds the draft while the modal is open and while it is
  closed. `ReportState { report, step: Form | Sent { address, files }, thumbnails, pending_shot }`.
- `Action::ReportIssue` opens it, and asks for a screenshot with a `ReportShot` marker in the
  `UserData`, the way the backdrop asks with its own marker. The frame that sees the picture writes it
  into the report's folder as `window.png` and attaches it. The backdrop's own photograph is not
  reused: it is blurred, and taken after the modal starts opening.
- `send` calls `prepare`, opens the address with `links::open_in_the_default_browser`, puts the files
  on the system clipboard with `system_files::put_files_on_the_clipboard` (only in the released binary,
  the rule `use_the_system_clipboard` keeps), and moves to `Sent`. A window a test built opens no
  browser and touches no clipboard; it records what it would have opened, which the command line
  reports.

### The agent's half (`unluminous-cli report`)

Rows in `unluminous-cli/src/catalogue.rs`, arms in `app/cli.rs`, sections in `docs/commands.md`:

| Command | What it does |
|---|---|
| `report open [--kind bug\|feature\|question] [--title <t>] [--description <d>]` | Opens the modal, filling what is named. |
| `report status` | The draft as data: kind, title, description, attachments, the two switches, the step, and the issue address it would open. |
| `report set [--kind] [--title] [--description] [--logs on\|off] [--system on\|off]` | Changes the draft. |
| `report attach <path>` | Attaches a file. |
| `report detach <name>` | Removes an attachment. |
| `report screenshot` | Photographs the window and attaches it; answers when it is attached. |
| `report send [--no-browser]` | Prepares the files and opens the issue; answers the address and the files. `--no-browser` only prepares and answers. |
| `report close [--discard]` | Closes the modal, keeping the draft unless told. |

### Telemetry

`run_action` already counts `Action::ReportIssue`, and `note_command` counts the `report` commands. No
title, description or file name is sent, which is the rule `services::telemetry` keeps.

## 4. The rest of the modals

The modals that still use `modal::show` with the old furniture are the commit panel, the eight git
dialogs, the text prompt and the confirmation, About, Run Configurations, the debugger's three dialogs,
New Project, Background, Go to File, Find in Files, the references and rename modals, the command
palette, the database plugin's Preview, DDL, Vector and New Table dialogs, and the Agent-Tasks
supervisor. Restyling them one at a time would be twenty rewrites. **They are restyled where they are
built from**, in `components::modal`, so every one of them changes together and a dialog written later
looks right without anybody remembering:

| Part | Old | New |
|---|---|---|
| `show` (framed) | `EXPLORER` fill, one point border, radius 10 | the `rux` surface on `chat.ground`, radius 28, `modal::shadow()` |
| `header` | 46 point `TITLE_BAR` band with a divider under it, 13 point title, a drawn cross | no band; the title in the design's semibold 16, and the round 30 point close chip |
| `footer` | divider above, flat buttons | no divider; the action's pill for the last button, chips for the others |
| `button` | flat rectangle, accent fill for the primary | the action's pill for the primary, a raised chip otherwise, dimmed ink when disabled |
| `field` | flat `FIELD` rectangle with a border | a carved well, round ends |
| `check` | flat square | a carved 16 point square, the action's gradient with a tick when on |
| `row` | a flat pill | raised out of the list when chosen, a wash under the pointer |
| `section` | 12.5 point heading and a divider | small spaced capitals in the quiet ink and a groove |
| `monospaced` | `EDITOR` rectangle with a border | a carved well, radius 16 |
| `well` (new) | each dialog painted its list ground itself | one carved well every list sits in |
| `tabs` (new) | the commit panel drew its own | a segmented track, the chosen tab raised |

Button words in the git dialogs and the commit panel move from capitals (`PUSH`, `CANCEL`) to the case a
person reads (`Push`, `Cancel`), which is what every redesigned dialog uses; the tests that press them
by name change with them. Every control keeps its name otherwise, so nothing else that finds a control
by name changes. The geometry constants `HEADER`, `FOOTER` and `body` are kept, so no dialog's layout
moves.

Dialogs that draw their own lists inside a `ScrollArea` open a `rux` layer only for the row that is
chosen or under the pointer, so a list of a thousand files costs at most two small canvases.

## 5. Tests

- `services::issue_report` unit tests: the encoder against RFC 3986 examples; the body for each kind,
  with and without logs and system; the address staying under the limit when the logs and the
  description are enormous, and saying so in the body; the kind's prefix and label; attachments that
  do not exist being refused with their path.
- Window tests (in `tests/window_and_chrome.rs`): the flag is in the status bar and opens the modal;
  the screenshot is attached on opening; `Open on GitHub` is dimmed with no title and Enter does not
  send; `Ctrl`+`Enter` sends and the second step names the files; `Cancel` keeps the draft and the flag
  brings it back; `Discard` empties it; `report` commands from the command line do the same things.
  Pictures: `report_issue_form`, `report_issue_sent`.
- The restyle changes every accepted picture with a modal in it. Each is opened and looked at before it
  is accepted.
- `the_flag_icon` joins the icon sheet in `tests/icons.rs`.
