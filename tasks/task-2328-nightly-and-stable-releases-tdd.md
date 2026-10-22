# task-2328: nightly and stable releases

## The ask

> We've run into issues with shipping major bugs with our releases. We need a plan/strategy for
> releasing Nightlies and official stable releases. In the check for updates menu item, I want a sub
> menu that shows me stable, and nightly, and then sub menus for each where I see the last 5 versions
> available, and can click to have them install and restart. The top option should be "latest". That
> way, we can test out the nightly before promoting it to stable.

## What happens today

Every finished ticket runs `tools/release.ps1`. It bumps the version, builds the Windows installer and
the macOS archive, tags, publishes a GitHub release on the private and the public repository, and runs
the unluminous.com publish script. That script copies the installer into the site's `public/downloads/`,
rewrites `SITE` in `content.ts`, and the site's build writes `/releases/latest.json` from `SITE`.

Every installed Unluminous asks `/releases/latest.json` once a day (`update.check = daily` is the
default since `task-2063`) and offers whatever it names with `Install & Restart`. So the moment a ticket
is released, every person who has Unluminous is offered it the next day. A release with a bad bug in it
reaches everybody before anybody has used it.

The site already keeps every installer it has ever published in `public/downloads/` (206 files on
2026-10-22, the 1.7 and 1.8 series with both the Windows installer and the macOS archive). Nothing
describes them, so the window cannot offer any of them.

## What other editors do

| Product | Channels | How a build becomes stable |
|---|---|---|
| Zed | Nightly (from `main`), Preview (weekly), Stable | Each Wednesday a preview branch is cut from `main` and a stable branch from the previous preview, so stable is always one week behind preview ([Zed: Introducing the preview channel](https://zed.dev/blog/preview-channel), [Zed release docs](https://zed.dev/docs/development/releases.html), [Zed FAQ](https://zed.dev/faq)). |
| VS Code | Insiders (daily from `main`), Stable | A release branch is cut during the endgame, tested, and the shipped build is built from its tag ([VS Code Insiders](https://code.visualstudio.com/Insiders/), [Evolution of Insiders](https://code.visualstudio.com/blogs/2016/05/23/evolution-of-insiders)). |
| Sparkle (macOS) | Items in one appcast tagged with `sparkle:channel`; the app lists the channels it accepts | An item moves channel by editing the appcast. Sparkle 2 removed automated downgrades ([Sparkle API: SUAppcastItem](https://sparkle-project.org/documentation/api-reference/Classes/SUAppcastItem.html), [SPUUpdaterDelegate](https://www.sparkle-project.org/documentation/api-reference/Protocols/SPUUpdaterDelegate.html), [Sparkle upgrading notes](https://sparkle-project.org/documentation/upgrading/)). |
| GitHub releases | `prerelease: true` marks a release that is not ready; `releases/latest` never answers with one | Edit the release to clear `prerelease`. |

Three ideas carry over.

1. **A channel is a list of published builds, not a separate build.** Zed and VS Code rebuild for each
   channel because their channels come from different branches. Unluminous has one branch and releases
   every ticket, so there is nothing to branch. The safest stable build is the exact installer that
   people already ran as a nightly: the same bytes, the same SHA-256. Rebuilding it for stable would
   ship bytes nobody tested.
2. **Promotion is a separate, deliberate step.** Sparkle moves an item between channels by editing the
   feed; GitHub clears `prerelease`. Neither rebuilds.
3. **Old clients must keep working.** Sparkle's channels need every client to understand the channel
   tag. Unluminous 1.8.3 and every earlier version read `/releases/latest.json` and nothing else, so
   that file has to keep a meaning that is safe for them.

## The design

### Channels

| Channel | What it holds | Who gets it automatically |
|---|---|---|
| `nightly` | Every release `tools/release.ps1` cuts, which is every finished ticket. | A window whose `update.channel` is `nightly`. |
| `stable` | Only versions somebody promoted with `tools/promote.ps1`, after using them as a nightly. | Every other window, including every Unluminous released before this ticket. |

Versions stay plain numbers (`1.9.0`, `1.9.1`). A stable version is a nightly version that was
promoted, so the version in the About box means the same build whichever channel delivered it, and
`update::is_newer` needs no change. A nightly is not named `nightly-2026-10-22`, because a build that
was promoted would then carry two names.

### What unluminous.com publishes

Three manifests, all written by `scripts/write-release-manifest.mjs` in the site's build.

- `/releases/stable.json` and `/releases/nightly.json`. The top level is the newest version on that
  channel, in exactly the shape `latest.json` has (`version`, `url`, `notes`, `installer`,
  `installerBytes`, `installerSha256`, `macos`, `macosBytes`, `macosSha256`), so `update::read` reads it
  unchanged. Beside that, `channel` names the channel and `versions` lists the newest ten entries on it,
  newest first, each in the same shape plus `date`.
- `/releases/latest.json` is unchanged in shape and **means the newest stable version**. That is what
  makes every Unluminous already installed safe: its daily check now only offers promoted builds.

The channel history lives in the site's `src/data/releases.json`, written by `scripts/sync-release.mjs`.
It is not committed, for the same reason `content.ts`'s release fields are not: the publish scripts
write it on the machine that releases. The first time it is missing it is seeded from the installers
already in `public/downloads/`: the five newest versions with both a Windows installer and a macOS
archive go on both channels, measured the way a release measures them. Those builds were all public
releases, so they are the honest stable history up to now.

`sync-release.mjs` takes `--channel nightly|stable`:

- `nightly` copies the installer and the archive into `public/downloads/`, measures the copies, and
  adds the version to the top of the nightly list. It does **not** touch `SITE`, so the download button
  on the page keeps offering the stable version.
- `stable` takes the version from the nightly list when it is there (the same measured bytes), adds it
  to the top of the stable list, and rewrites `SITE` as it always has, so the page and `latest.json`
  move to it.

A version is never in a list twice. Adding one that is already there moves it to the top.

`scripts/publish.ps1` takes `-Channel` (default `stable`, which is what it did before) and checks the
channel's own manifest at the end.

### The release scripts

- `tools/release.ps1` publishes to `nightly`. It passes `-Channel nightly` to the site, creates both
  GitHub releases with `--prerelease`, and checks `/releases/nightly.json` in its report. GitHub's
  `releases/latest` never answers with a prerelease, so the GitHub fallback the window asks keeps
  meaning stable too.
- `tools/promote.ps1 [-Version x.y.z]` is new. With no version it promotes the newest nightly. It
  refuses a version that is not on the nightly channel, then runs the site publish with
  `-Channel stable -Version x.y.z`, clears `prerelease` on both GitHub releases and marks them latest,
  and checks that `stable.json`, `latest.json` and the page all say the version. Nothing is built.
- `tools/release.sh` (macOS) creates its GitHub release as a prerelease too.

The working rule this gives: **a ticket ships a nightly; Jason runs the nightly; when it has held up,
`pwsh tools/promote.ps1` makes it stable.** A bad nightly is fixed by the next nightly, and nobody on
stable ever saw it.

### The window

**Setting.** `update.channel` is `stable` or `nightly`, `stable` by default, on `Settings -> Editor`
beside `update.check`. The daily check, `Check for Updates` and `update check` ask that channel.

**Sources per channel.** `stable` asks `stable.json`, then `latest.json` (so a site not yet rebuilt
still answers), then the public GitHub `releases/latest`. `nightly` asks `nightly.json` only: GitHub's
latest release is the stable one, and offering it as the newest nightly would be wrong.

**The menu.** `Unluminous -> Check for Updates` becomes a submenu:

```
Check for Updates  >  Stable   >  Latest (1.8.3)
                                  ------------
                                  1.8.3   (ticked: the one running)
                                  1.8.2
                                  ...five in all
                                  ------------
                                  Follow This Channel   (ticked when followed)
                      Nightly  >  Latest (1.9.0)
                                  ------------
                                  1.9.0
                                  ...
                                  ------------
                                  Follow This Channel
                      ------------
                      Check Now
```

- `Latest` installs the newest version on that channel and restarts. A version row installs that
  version and restarts, older or newer: the Inno Setup script marks every file `ignoreversion`, so an
  older installer replaces a newer one, and the macOS helper swaps the bundle whatever its version.
- The row for the version that is running is ticked and dimmed, because installing it again does
  nothing useful.
- `Check Now` is the old `Check for Updates` entry, with the same action name, `check-for-updates`.
- `Follow This Channel` is ticked on the channel `update.channel` names, and pressing it on the other channel
  changes the setting, which is the Settings page's `Channel` row from the menu.
- Until the list has been fetched each channel shows one dimmed row, `Loading versions...`, or the
  reason it could not be fetched, plus `Load Versions`. `Latest` works either way: it fetches the
  channel and installs the newest.

**When the list is fetched.** One request per channel, on a thread, never at startup on its own:

- when the in-window `Check for Updates` submenu opens and the list is older than ten minutes;
- after every check (the daily one, `Check Now`, `update check`), which already asks the channel;
- on `Load Versions` and `update versions`.

On macOS the bar along the top of the screen is AppKit's and does not say when a submenu opens, so
there the list arrives with the daily check or with `Load Versions`. A rebuilt list changes `MenuState`,
and the native bar rebuilds when its menus change.

**Installing a chosen version** reuses `update_install::Install` unchanged: it is given the version and
its `Download` (URL, size, SHA-256) from the channel list, so the file is checked before anything runs,
exactly as `Install & Restart` is.

### Command line and MCP

- `update check [--channel stable|nightly]` asks one channel; the setting by default.
- `update versions [--channel ...] [--refresh]` lists both channels' versions (newest first, at most
  five, the one running marked), fetching them first when `--refresh` is given or nothing is loaded. It
  waits for the fetch, like `update check`.
- `update install [version] [--channel ...] [--no-restart]` installs a named version, or the newest on
  the channel when one is given and no version is. With neither it is what it was: the version the last
  check found.
- `update status` reports the channel and the loaded lists.
- `settings set update.channel nightly` reaches the setting by name.
- `action run install-nightly 1.9.0` and `action run install-stable` are the menu rows, named the way
  `run-` actions are named, with the version as the argument.

### What is left out

- **Staged rollouts** (a percentage of stable windows at a time). There is no server that could choose,
  and every window reads one static file.
- **Automatic promotion** after some number of days. Promotion is the point where a person decides a
  build is good; making it a timer would put back the problem this ticket is about.
- **Delta updates.** The installer is 110 MB and is downloaded whole, as it is today.

## Tests

- `services::update`: reading a channel manifest with and without `versions`, a version list cut to
  five, the sources each channel asks in order, a stable check falling through a 404 `stable.json` to
  `latest.json`, and a nightly check that never asks GitHub.
- `app::action_names`: the two install actions read back from their names with the version as the
  argument, and the menu rows exist with and without a loaded list.
- Menu: the `Check for Updates` submenu holds `Stable`, `Nightly` and `Check Now`; each channel lists
  `Latest` and five versions; the running one is ticked and dimmed.
- Command line: `update versions` against a scripted server (`UNLUMINOUS_RELEASES_STABLE`,
  `UNLUMINOUS_RELEASES_NIGHTLY` name the two manifests), and `update install <version>` refusing a
  version that is on no channel.
- Site scripts: `node --test` over the channel list code: seeding, adding to a channel, moving a
  version to the top, cutting to ten, and the three manifests agreeing.
