# task-2271: usage statistics for Unluminous, sent to Google Analytics

## The ask

> add telemetry to Unluminous. add a setting to turn it off in Settings. We want to collect when
> people have unluminous open, how long its open, what panels and features they use most.
>
> We have google tag manager, and have set up inillucent.com with it. Set up a new site in gtm first
> called "Unluminous App". Then add the telemetry.
>
> Do deep analysis of the code to find the most useful things to track, ensure we are mindful of the
> number of requests we emit, it's non-blocking, etc.

And a second message on the same ticket: add a tag for **Unluminous Web** and put tracking on
unluminous.com, especially the different kinds of download. Section 11 covers that half.

## 1. What was set up in Google's tools

inillucent.com loads `gtag.js` for `G-2G0CSYY4WN`, which is the Google tag of the GA4 property
`Inillucent` in the Google Analytics account `Inillucent.com` (`a407371481`). Tag Manager lists it
under **Google tags**, not as a container. "A new site in GTM called Unluminous App" is therefore a
second Google tag of the same kind, and a Google tag is made by making a GA4 property and a data stream:

| What | Value |
|---|---|
| GA4 property | `Unluminous App`, in the account `Inillucent.com` |
| Data stream | web stream `Unluminous App`, `https://unluminous.com`, stream id `16131627954` |
| Measurement id | `G-WHZDTZ5JK1` |
| Google tag in Tag Manager | `Unluminous App`, `G-WHZDTZ5JK1, GT-M398H9N8` |
| Measurement Protocol API secret | nickname `Unluminous desktop app` |

Creating the API secret required accepting Google's *User Data Collection Acknowledgement* on the
property. A test event posted to `/debug/mp/collect` came back with no validation messages.

### Why a web stream and the Measurement Protocol

Unluminous is a native program, not a web page, so `gtag.js` and a Tag Manager web container cannot
run in it. The ways a native program reaches GA4 are:

1. **Firebase**, with an Android or iOS stream. There is no Firebase SDK for a Windows or macOS desktop
   program in Rust.
2. **A hidden web view running `gtag.js`.** A second native web view costs a WebView2 process tree
   (about 200 MB, measured in the browser section of `CLAUDE.md`), and the window may have only one
   native view at a time.
3. **The GA4 Measurement Protocol**: an HTTPS `POST` of a small JSON body to
   `https://www.google-analytics.com/mp/collect?measurement_id=…&api_secret=…`. It needs a web stream's
   measurement id and an API secret, and nothing else.

The third is the only one that fits. It is one `ureq` request on a thread, the same transport
`services::update` already uses.

The API secret is compiled into the binary. A Measurement Protocol secret only lets the holder send
events to this one stream; it cannot read anything. Every desktop program that uses the protocol ships
its secret this way, and anyone could find it in the binary. If it is ever abused, it is rotated in the
GA admin page and the next release carries the new one.

## 2. This reverses a standing rule, and says so

`services::update` records `task-1692`'s line: *"There is no discovery, no model list, no telemetry and
nothing at startup."* `task-2063` already reversed the update check's default on Jason's instruction.
This ticket reverses the telemetry half on the same terms:

- **On by default**, because the ticket asks for a setting to turn it *off*.
- **One switch turns it off completely**: `Settings -> Editor -> Usage statistics`, or
  `settings set telemetry.enabled false`. Off, nothing is counted and nothing is sent, from that frame
  on, including the closing event.
- **What is sent is a closed list of names** (section 4). No path, file name, project name, text,
  search, command argument, URL, address, user name or machine name is ever sent.
- **A window something else is driving sends nothing**: a window started with `--background`, and every
  window a test builds. `UNLUMINOUS_TELEMETRY=off` turns it off for a process, and
  `UNLUMINOUS_TELEMETRY=on` turns it on for a `--background` window.

## 3. Where the code already funnels what a person does

The analysis looked for the few places every interaction passes through, so that counting is one line
in each rather than a line in every feature.

| Funnel | What goes through it | Name it gives |
|---|---|---|
| `UnluminousApp::run_action` (`app/menus.rs`) | Every menu entry, every key chord, the command palette, `action run` | `Action::name()`, a closed list in `app/action_names.rs` (`save`, `find-in-files`, `toggle-terminal`, `git:commit`, `debug:start`, …) |
| `UnluminousApp::run_cli`, called from `pump_control` (`app/cli.rs`) | `unluminous-cli` and every MCP tool call from an outside agent | the catalogue's wire name (`tab.open`, `editor.rename`, `realm.add`) |
| `run_cli_for_a_plugin` | A tool call made by the Agent-Chat pane's own model, a chat node, or a plugin | the same wire name |
| `UnluminousApp::panels_showing` and `showing_plugin_panes` (`app/panels.rs`) | Which panels are on the screen | `explorer`, `terminal`, `run`, `debug`, `realm`, and `<plugin>/<pane>` |
| `OpenFiles::active` | The tab that is showing | `file_kind::kind_name`, a closed list (`Rust`, `Markdown`, `Plain text`…), or `browser`, `picture`, `notebook`, `plugin:<id>` |
| `AgentChat::begin_a_turn` (`services/agent_chat`) | A question sent to an agent, from the composer, the queue or the command line | the endpoint's wire shape (`claude-cli`, `codex-cli`, `openai`…) |
| `AgentTasks::spawn` (`services/agent_tasks`) | A ticket's agent started | the assignee kind (`claude`, `codex`, `opencode`) |

Most features are reached through the first two rows: git, debugging, running, find, folding, the
Realm, notebooks, themes and splitting all have menu entries or commands. What has neither is drawn
inside a plugin's pane, and the two rows at the bottom cover the two that matter most.

## 4. The events

GA4 reserves `session_start`, `first_visit`, `first_open`, `user_engagement` and `screen_view`, so
none of these names collide with them.

| Event | When | Parameters |
|---|---|---|
| `app_open` | in the first request of a window | `launch` (`desktop`, `folder`), `theme` (a bundled theme id or `other`), `first_run` (1 the first time this machine sends anything) |
| `app_heartbeat` | in every request while the window is used | `engagement_time_msec`: active time since the last request |
| `feature` | in a request, one per distinct name used since the last one | `category` (`action`, `command`, `agent_tool`, `agent_chat`, `agent_task`, `file`), `feature` (the name), `uses` (how many times) |
| `panel_time` | in a request, one per panel showing since the last one | `panel`, `seconds` |
| `tab_time` | in a request, one per kind of tab shown since the last one | `kind`, `seconds` |
| `app_close` | in the last request of a window | `open_seconds`, `focused_seconds`, `active_seconds` |
| `app_crash` | in the first request after a start that finds `crash.log` grew | `uses` (always 1) |

Every event carries `session_id` and `engagement_time_msec`, which is what GA4 needs to count a session
and an engaged user. Every request carries `user_properties` (`app_version`, `os`, `arch`) and a
`device` object (`category: desktop`, `operating_system`, `operating_system_version`).

**Open, focused and active are three different numbers.** *Open* is wall time from the first frame to
`on_exit`. *Focused* is time the window held the operating system's focus. *Active* is focused time
with input in the last 60 seconds, which is GA4's own idea of engagement. "How long it is open" is the
first; "how much it is used" is the third.

**`feature` names are cut to the closed lists.** A plugin's action is
`plugin-run:<plugin>:<command>`; a plugin that is not one of `services::plugins::bundled::ALL` is sent
as `plugin-run:other`, because a third party plugin's id is a folder name somebody chose. A wire name
not in the catalogue is never sent, because `run_cli` refuses it before it is counted.

## 5. How many requests

| Moment | Requests |
|---|---|
| The window opens | 1, ten seconds after the first frame, carrying `app_open` and anything left from a window that closed without sending (section 7) |
| While somebody is using it | 1 every 20 minutes, only when something was counted or there was active time |
| A window open and idle | **none**, however long it stays open |
| The window closes | 1, carrying `app_close` and what was counted since the last request |

One request carries at most 25 events, which is GA4's limit. When more distinct things were counted
than fit, the extra rows wait for the next request, and a request never becomes more than 4 at once.
A window has a ceiling of 200 requests for its life, so a fault cannot turn into a flood.

A person who uses Unluminous for an eight hour day in one window sends about 26 requests. Twenty
minutes is under GA4's 30 minute session timeout, so a session that is being used stays one session.

## 6. Nothing waits on the network

- **Counting is a relaxed atomic load when telemetry is off**, and one lock of an uncontended mutex and
  one hash map increment when it is on. `services::telemetry::note` is safe to call from anywhere on the
  window's thread, including inside a plugin, which is why it is a function over a process wide tally
  rather than a field threaded through every caller.
- **The per frame cost is two comparisons.** Panels and the tab are sampled once a second, not once a
  frame; the sample is `panels_showing()`, which builds a small fixed array.
- **Sending is on a thread of its own**, `unluminous-telemetry`, fed by a channel. The window hands it a
  finished JSON body and never waits for an answer. A request has a 10 second budget, and a request
  that fails is written to the spool (section 7) rather than retried in a loop.
- **Closing the window waits at most 600 ms**, and only when there is something to send. The closing
  batch is written to the spool first, so a slow network costs the next start a request rather than
  costing this window its close.

## 7. The spool: what was not sent survives

`<settings folder>/telemetry/` holds:

- `id.txt`: the client id, 32 random hexadecimal characters made once per installation from the
  operating system's random source (`getrandom`). It is not derived from anything about the machine or
  the person. Deleting it makes a new one.
- `spool-<pid>.jsonl`: request bodies a window could not send, one a line, each with its own
  `timestamp_micros`.

A window that starts sends every spool file whose window is no longer running, after claiming it by
renaming it to `sending-<its own pid>-…`, so two windows starting together cannot both send it. A body
older than 72 hours is dropped, because GA4 refuses a `timestamp_micros` older than that. A spool file
is cut to its newest 200 lines.

`crash.log` growth is read here too: `telemetry/crash-size.txt` records its length at the last start, and
a longer `crash.log` adds `app_crash`.

## 8. The setting and the agent's half

- `telemetry.enabled`, a flag, `true` by default, in the settings file and on `settings list|get|set`.
- `Settings -> Editor`, a card called **Usage statistics** under **Updates**, with one switch:
  *"Send anonymous usage statistics"*, and a sentence saying what is and is not sent.
- `unluminous-cli settings telemetry`: whether it is on, why it is off when it is (`setting`,
  `background window`, `environment`, `test`), the client id, how many requests this window has sent,
  how many rows are waiting, and the last failure. `--flush` sends what is waiting now, which is how the
  end to end check in section 10 is driven without waiting twenty minutes.

A verb on the existing `settings` area rather than an area of its own, because an area is a whole MCP
tool in every agent's preamble and this is two read questions about one setting.

## 9. Code

| File | What it holds |
|---|---|
| `services/telemetry/mod.rs` | `note`, the process wide tally, the switch, `Telemetry` (timers, sampling, when to send) |
| `services/telemetry/payload.rs` | building GA4 bodies from a tally; pure, tested with no network |
| `services/telemetry/sender.rs` | the thread, the spool, claiming leftover spool files |
| `app/telemetry.rs` | the window's half: start, the once a second sample, `on_exit`, the setting changing |

Test seams, in the shape of `UNLUMINOUS_RELEASES`:

- `UNLUMINOUS_TELEMETRY` = `off`, `on`, or `debug` (on, plus GA4's `debug_mode` so events show in
  DebugView, plus every body appended to `telemetry/sent.jsonl`).
- `UNLUMINOUS_TELEMETRY_URL` points the sender at a scripted server on loopback.
- `UNLUMINOUS_TELEMETRY_EVERY` (seconds) shortens the 20 minute interval for a test.

## 10. How it is verified

1. Unit tests: the payload builder (limits of 25 events, 40 character names, 100 character values,
   carrying overflow to the next request), the timers (open, focused, active), the spool (claiming,
   72 hour expiry, cutting), and that a third party plugin's name is never sent.
2. A test with a scripted HTTP server on loopback: a window with telemetry on sends `app_open`, a
   `feature` row for an action it ran, and `app_close`; with the setting off it sends nothing.
3. The installed build, started from the desktop, with `UNLUMINOUS_TELEMETRY=debug`: the events show in
   GA4's DebugView and Realtime report for the `Unluminous App` property.

## 11. unluminous.com: the `Unluminous Web` tag

A separate GA4 property, **`Unluminous Web`** (559064667), with the web stream `Unluminous Web` and
measurement id **`G-G75FZL7PF3`**, so the site's page views do not mix into the app's reports. It shows
in Tag Manager under Google tags beside `Unluminous App` and `Inillucent`.

The site is `sites/unluminous` in black-rainbow-labs-sites. It loads `gtag.js` exactly the way
inillucent.com does (`src/client/analytics.ts`, two scripts in `layout.tsx` that run after the page is
interactive, and one delegated click listener in `components/Analytics.tsx`). Every download link carries
`data-ga-*` attributes from `downloadEvent`, and a click on one reports **`download_click`** with:

| Parameter | Values |
|---|---|
| `platform` | `windows`, `macos` |
| `file_kind` | `installer` (`.exe`), `zip`, `dmg`, `msi`, `tarball`, `other`, read from the file name |
| `file_name` | `UnluminousSetup-0.85.0-x64.exe` |
| `version` | the version the link points at |
| `placement` | `hero`, `masthead`, `install-panel`, `install-section` |

`download_click` rather than GA4's own `file_download`, which enhanced measurement also sends for an
`.exe` or a `.zip`, so the two do not add up to twice the real number. The header's `Download` pill, which
scrolls to the install section, reports `download_menu_click`, and a link that leaves the site reports
`outbound_click`. `platform`, `file_kind`, `version` and `placement` are registered as custom dimensions.

The site's "What it won't do" row said *"No telemetry"*; it now says the app sends anonymous usage
statistics and that Settings turns them off.

## 12. Left out

- **Error reporting with messages.** A crash is counted; its text stays in `crash.log`, because a panic
  message can hold a path.
- **Timing of individual features** (how long a build or a search took). Counts and panel time answer
  the ticket; timings are a later question.
- **A consent prompt on first start.** The ticket asks for a switch in Settings, and an on by default
  setting with a clear sentence beside it is what `update.check` already does.
