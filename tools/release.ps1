<#
.SYNOPSIS
  Releases Unluminous: bump the version, build, install it on this machine, tag, push, and publish a
  GitHub release with the installer on it.

.DESCRIPTION
  `task-1667` asks that finishing a task means releasing it, and asks for that in a form that is
  actually followed. Four commands in the right order is not that form; one is. So everything a
  person would otherwise have to remember is in here, in the order it has to happen, stopping at the
  first thing that goes wrong.

  What it does:

    1. Refuses to run on a dirty checkout. A release built from one is a release nobody can rebuild.
    2. Bumps `version` under `[workspace.package]` in Cargo.toml, which is the one place the version
       is written down. It reaches unluminous.exe's version block, the installer's file name, the Add or
       Remove Programs entry and Info.plist from there.
    3. Runs installer\windows\build.ps1 -Install: builds unluminous.exe and unluminous-cli.exe, refuses to
       package an executable with no version block, compiles the Inno Setup installer, closes a
       running Unluminous politely and installs it with every optional task on. The rebuild is what moves
       the build date the About box shows.
    4. Copies the installer into releases\.
    5. Commits Cargo.toml and Cargo.lock on their own as `Unluminous <version>`, tags `v<version>`, and
       pushes the branch and the tag.
    6. Creates the GitHub release with the installer attached, on BOTH repositories: the private one
       where releases are cut, and the public Black-Rainbow-Labs one, whose history it publishes
       first with tools/publish-open-source.mjs.
    7. Publishes both sites. unluminous.com gets the new installer and puts the version at the top of
       the nightly channel, /releases/nightly.json; blackrainbowlabs.com reads /releases/latest.json for
       the version it prints.

  **Every release is a nightly (task-2328).** The GitHub releases are created as prereleases and the
  site adds the version to the nightly channel only, so the page, /releases/latest.json and GitHub's
  latest release stay on the stable version. A window following stable, which is every window unless
  somebody chose nightly, is not offered this release until `pwsh tools/promote.ps1 -Version <version>`
  promotes it, with the same installer and nothing built again.
  `tasks/task-2328-nightly-and-stable-releases-tdd.md` is the design.

  Steps 6 and 7 are both there because of `task-1993`, which found that every address the update
  check knew named the private repository and therefore answered 404 to everybody but Jason. The
  check asks unluminous.com now and falls back to the public repository, so a release that reached
  neither of them is a release nobody is told about -- and a site left behind by a release is worse
  than one that never answered, because it answers with the version before this one. The same
  argument is why blackrainbowlabs.com is published here rather than by hand: its version was set by
  a script nobody could run, and the page said v0.37.1 through fifteen releases.

  The task's own code is expected to be committed already: the version bump is a commit of its own so
  that the history stays greppable by ticket.

.PARAMETER Part
  Which part of the version goes up: patch (the default), minor or major. Patch for a fix, minor for
  a feature.

.PARAMETER Version
  Release exactly this version instead of bumping. Must be higher than the one in Cargo.toml.

.PARAMETER Notes
  The body of the GitHub release. Defaults to the subject of the commit the release is cut from.

.PARAMETER SkipInstall
  Build the installer but do not install it on this machine. The About box will then still show the
  old build, which is the thing this script exists to keep true, so use it only when releasing from a
  machine that is not the one Unluminous is used on.

.PARAMETER SkipPublish
  Do everything up to and including the tag, and stop before touching GitHub or the site.

.PARAMETER SkipSite
  Publish the releases but leave unluminous.com and blackrainbowlabs.com alone. The sites then say
  the version before this one, and so does `update check` for everybody, so use this only when they
  are being published by hand straight afterwards.

.PARAMETER SkipMacos
  Leave macOS out of this release.

  It is **in** by default. It was behind a `-Macos` switch for one afternoon, and a switch that
  defaults off is a switch the next release forgets - which is how a product ends up shipping one
  platform for fifteen versions. What replaces the switch is a preflight:
  `installer\macos\build-on-windows.ps1 -Preflight` says whether the SDK, the Developer ID identity
  and the notary credential are all present, and the release includes macOS when they are and says
  at the start why it does not when they are not.

.PARAMETER WhatIf
  Say what would happen and change nothing.

.PARAMETER Resume
  Carry on publishing the release whose tag is on HEAD. A release keeps a record of its finished steps in
  `_agent_output
elease-state\<version>.json` (`task-2312`), and starting it again carries on from the step that
  failed without being asked: a failure before the commit reuses the installers it built from the same source,
  and a failure after the tag skips every step that finished. `-Resume` is for a tagged release with no record.

.EXAMPLE
  pwsh tools\release.ps1
  pwsh tools\release.ps1 -Part minor -Notes "task-1667: the About box and a one-command release"
#>
[CmdletBinding()]
param(
    [ValidateSet('patch', 'minor', 'major')]
    [string] $Part = 'patch',
    [string] $Version,
    [string] $Notes,
    [switch] $SkipInstall,
    [switch] $SkipPublish,
    [switch] $SkipSite,
    [switch] $WhatIf,
    # Skip the suite. For a release whose tests were just run by hand; the gate exists because
    # `task-1922` found every release so far had been made with nothing checking the build at all.
    [switch] $SkipTests,
    # Run clippy and the tests for every crate, rather than only for the crates whose inputs have
    # changed since they last passed (`task-2240`). For a release after a toolchain or machine change
    # that the inputs `tools/test.mjs` hashes would not see.
    [switch] $FullTests,
    [switch] $SkipMacos,
    # Carry on a release whose commit is tagged but whose publishing stopped, when there is no record of it.
    # With a record, starting the release again carries on by itself.
    [switch] $Resume
)

$ErrorActionPreference = 'Stop'

# **On 80% of the processors, and everything this starts as well (`task-2240`).** A release used to
# hold all 24 logical processors at 100% through clippy, the test build and three release builds, on
# the machine a person is working at and other tickets render and build on. The affinity mask is set
# here, before anything is started, so cargo, rustc, the linker, every test binary, Inno Setup and the
# macOS build inherit it. `UNLUMINOUS_CPU_PERCENT=100` removes it. See `tools/processor-share.ps1`.
. (Join-Path $PSScriptRoot 'processor-share.ps1')
if (-not $WhatIf) { Enter-ProcessorShare }

$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Repo = (Resolve-Path (Join-Path $Here '..')).Path
$Manifest = Join-Path $Repo 'Cargo.toml'
$ReleasesDir = Join-Path $Repo 'releases'
# The public repository the source was opened under on `task-1989`, and what `update check` falls
# back to when unluminous.com does not answer. Never the private one: it is 404 to everybody else.
$PublicRepository = 'Black-Rainbow-Labs/Unluminous'
# The two sites that say which version Unluminous is, each carrying its own publish script: the
# product page, which is where the installer and the manifest `update check` reads actually are, and
# the parent company page, which prints the version beside the product. A machine without a checkout
# releases everything else and says so.
#
# They are published in this order because the second reads the first: blackrainbowlabs.com takes the
# version out of unluminous.com's manifest rather than out of a repository, which is what `task-1993`
# gave it in place of the private repository that answered 404 and left it saying v0.37.1 for fifteen
# releases.
#
# Both sites live in the black-rainbow-labs-sites repository since task-2128, and each publish script
# now deploys to Cloudflare as well as the origin on this machine.
$SiteRepo = $env:UNLUMINOUS_SITE_REPO
if (-not $SiteRepo) { $SiteRepo = 'C:/jason/dev/black-rainbow-labs-sites/sites/unluminous' }
$ParentSiteRepo = $env:BLACK_RAINBOW_LABS_REPO
if (-not $ParentSiteRepo) { $ParentSiteRepo = 'C:/jason/dev/black-rainbow-labs-sites/sites/blackrainbowlabs' }
$SitePublishers = @(
    @{ Name = 'unluminous.com'; Script = (Join-Path $SiteRepo 'scripts/publish.ps1'); Versioned = $true },
    @{ Name = 'blackrainbowlabs.com'; Script = (Join-Path $ParentSiteRepo 'scripts/publish.ps1'); Versioned = $false }
)

# Each step's name and when it started, so the release can say where its time went (`task-2240`).
$script:Steps = [System.Collections.Generic.List[object]]::new()
$script:ReleaseStarted = Get-Date

<#
.SYNOPSIS
  Prints a step's heading and starts timing it. The previous step's time ends where this one starts.
.PARAMETER Message
  The step's heading.
#>
function Write-Step([string] $Message) {
    $script:Steps.Add([pscustomobject]@{ Step = $Message; Started = Get-Date })
    Write-Host ''
    Write-Host "==> $Message" -ForegroundColor Cyan
}

<#
.SYNOPSIS
  Prints how long each step took and writes the same to `_agent_output/release-timings/`.
.DESCRIPTION
  `task-2240` was asked why a release took so long, and nothing had recorded where a release's time
  went: the only evidence was the times on the files it left behind. Every release now leaves one JSON
  file naming each step, its seconds and the processor share it ran on.
.PARAMETER Version
  The version being released, which names the file.
#>
function Write-StepTimes([string] $Version) {
    $finished = Get-Date
    $rows = @()
    for ($index = 0; $index -lt $script:Steps.Count; $index++) {
        $until = if ($index + 1 -lt $script:Steps.Count) { $script:Steps[$index + 1].Started } else { $finished }
        $rows += [pscustomobject]@{ step = $script:Steps[$index].Step; seconds = [math]::Round(($until - $script:Steps[$index].Started).TotalSeconds, 1) }
    }
    $total = [math]::Round(($finished - $script:ReleaseStarted).TotalSeconds, 1)
    Write-Host ''
    Write-Host "Where the $total seconds went:" -ForegroundColor Cyan
    foreach ($row in $rows) { Write-Host ("  {0,7:N1} s  {1}" -f $row.seconds, $row.step) }
    $folder = Join-Path $Repo '_agent_output\release-timings'
    New-Item -ItemType Directory -Force -Path $folder | Out-Null
    [pscustomobject]@{
        version = $Version
        started = $script:ReleaseStarted.ToString('o')
        seconds = $total
        processorShare = $(if ($env:UNLUMINOUS_CPU_PERCENT) { $env:UNLUMINOUS_CPU_PERCENT } else { '80' })
        cargoBuildJobs = $env:CARGO_BUILD_JOBS
        steps = $rows
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $folder "$Version.json") -Encoding utf8
}

function Invoke-Checked([string] $What, [scriptblock] $Body) {
    & $Body
    if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE." }
}

<#
.SYNOPSIS
  Run a block with `CC` out of the environment, and put it back afterwards.
.DESCRIPTION
  `task-1984` T4. This machine has a user environment variable `CC` naming `cl.exe` by its full path
  with no `INCLUDE` or `LIB` beside it. `cc-rs` takes a `CC` it is given as the whole answer and skips
  its own Visual Studio lookup, so the compiler it invokes cannot find a single system header and
  `libsqlite3-sys` fails to build -- from any ordinary shell, which is every shell this script is
  started from. The only gate therefore failed at step 0 here and every measurement in that review
  had to be taken with `env -u CC`.

  So cargo is run with `CC` unset. The variable itself is the person's and is left alone; what
  changes is the environment this script hands to a build.
#>
function Invoke-WithoutCc([scriptblock] $Body) {
    $had = Test-Path env:CC
    $was = if ($had) { $env:CC } else { $null }
    if ($had) { Remove-Item env:CC }
    try { & $Body } finally { if ($had) { $env:CC = $was } }
}

<#
.SYNOPSIS
  The version in Cargo.toml, which is the one place a version is written down.
#>
function Get-CurrentVersion {
    $line = Select-String -Path $Manifest -Pattern '^\s*version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"' |
        Select-Object -First 1
    if (-not $line) { throw "No version = 'x.y.z' in $Manifest." }
    return $line.Matches[0].Groups[1].Value
}

<#
.SYNOPSIS
  The next version, given which part is going up.
.DESCRIPTION
  The ordinary semantic-version rules: a minor bump zeroes the patch and a major bump zeroes both, so
  0.1.4 -Part minor is 0.2.0 rather than 0.2.4.
#>
function Get-NextVersion([string] $Current, [string] $Which) {
    $parts = $Current.Split('.') | ForEach-Object { [int] $_ }
    switch ($Which) {
        'major' { return "$($parts[0] + 1).0.0" }
        'minor' { return "$($parts[0]).$($parts[1] + 1).0" }
        default { return "$($parts[0]).$($parts[1]).$($parts[2] + 1)" }
    }
}

<#
.SYNOPSIS
  True when `a` is a higher version than `b`, compared part by part rather than as text.
#>
function Test-Higher([string] $A, [string] $B) {
    $left = $A.Split('.') | ForEach-Object { [int] $_ }
    $right = $B.Split('.') | ForEach-Object { [int] $_ }
    for ($index = 0; $index -lt 3; $index++) {
        if ($left[$index] -ne $right[$index]) { return $left[$index] -gt $right[$index] }
    }
    return $false
}

<#
.SYNOPSIS
  Write the new version into the `[workspace.package]` table of Cargo.toml.
.DESCRIPTION
  Only the first `version = "x.y.z"` in the file is touched. The workspace table is at the top and
  every crate inherits from it with `version.workspace = true`, so there is exactly one line to
  change and changing more than one would be a mistake rather than a thoroughness.
#>
function Set-Version([string] $New) {
    $text = Get-Content -Raw -Path $Manifest
    $pattern = '(?m)^(\s*version\s*=\s*")([0-9]+\.[0-9]+\.[0-9]+)(")'
    $replaced = [regex]::new($pattern).Replace($text, "`${1}$New`${3}", 1)
    if ($replaced -eq $text) { throw "Could not write the version into $Manifest." }
    # -NoNewline because the file already ends with one, and Set-Content would add a second.
    Set-Content -Path $Manifest -Value $replaced -NoNewline -Encoding utf8
    # Cargo.lock names every workspace member's version, so it has to move with the manifest. A
    # metadata read is the cheapest thing that rewrites it, and it fails loudly if the edit was wrong.
    Invoke-Checked 'cargo metadata' { & cargo metadata --no-deps --format-version 1 --manifest-path $Manifest | Out-Null }
}

# `Get-GitHubCli` and `Get-GitHubToken` are in `release-steps.ps1`, which `tools/promote.ps1` loads too.

<#
.SYNOPSIS
  Puts the keyboard back before anything else happens.

.DESCRIPTION
  `task-1762` reported a machine on which pressing D minimised the window in front, because a script
  that drives the real window had pressed the Windows key and stopped before releasing it. Windows
  believes a synthesised key is held until its key-up arrives, and the physical keyboard cannot clear
  it, because the physical key was never down.

  Releasing it is one line, and the reason it is here is that this is the line an Unluminous task ends on.
  Anything that drove the real window has finished by now, so nothing legitimate is holding a
  modifier, and a person about to type into their own machine again should not have to know any of
  the above. It only reports under -WhatIf, which changes nothing by contract.
#>
function Restore-Keyboard {
    $unstick = Join-Path $PSScriptRoot 'unstick-keyboard.ps1'
    if (-not (Test-Path $unstick)) { return }
    if ($WhatIf) {
        & pwsh -NoProfile -File $unstick -Check | ForEach-Object { Write-Host "  $_" }
        return
    }
    $held = & pwsh -NoProfile -File $unstick -Check
    if ($LASTEXITCODE -eq 0) { return }
    Write-Step 'Putting the keyboard back'
    Write-Host "  $held"
    & pwsh -NoProfile -File $unstick | ForEach-Object { Write-Host "  $_" }
}

<#
.SYNOPSIS
  The main checkout: the parent of git's common directory. In a ticket's worktree that is the folder the
  worktree was made from; in the main checkout it is the checkout itself.
#>
function Get-MainCheckout {
    $common = (& git -C $Repo rev-parse --path-format=absolute --git-common-dir).Trim()
    return (Split-Path -Parent $common)
}

<#
.SYNOPSIS
  Puts the two files the macOS build needs into this checkout when they are missing.

.DESCRIPTION
  `task-2295`. `installer/macos/notarize.env` and `tools/cross/bin` are ignored by git, so a ticket's
  worktree starts without them and the preflight fails. The project's CLAUDE.md told an agent to copy one
  and fetch the other first, which worked when the agent read that paragraph. The release does it now: the
  signing file is copied from the main checkout, and the toolchain is fetched.
#>
function Get-TheMacosFiles {
    $settings = Join-Path $Repo 'installer\macos\notarize.env'
    if (-not (Test-Path $settings)) {
        $kept = Join-Path (Get-MainCheckout) 'installer\macos\notarize.env'
        if ((Test-Path $kept) -and ((Resolve-Path $kept).Path -ne $settings)) {
            Copy-Item -LiteralPath $kept -Destination $settings
            Write-Host "Copied installer\macos\notarize.env from $(Get-MainCheckout)"
        }
    }
    if (-not (Test-Path (Join-Path $Repo 'tools\cross\bin\cargo-zigbuild.exe'))) {
        Write-Host 'Fetching the macOS cross toolchain into tools\cross\bin'
        & pwsh -NoProfile -File (Join-Path $Repo 'tools\cross\fetch-toolchain.ps1') | Out-Null
        if ($LASTEXITCODE -ne 0) { Write-Host '  tools\cross\fetch-toolchain.ps1 failed; the preflight says what is missing.' -ForegroundColor Yellow }
    }
}

<#
.SYNOPSIS
  Brings main up to a release cut on another branch.

.DESCRIPTION
  `task-2295`. A ticket releases from its worktree's branch, and `git push origin $branch` pushed that
  branch alone: the version commit and the tag reached GitHub while `main`, on GitHub and in the main
  checkout, stayed behind until somebody moved it by hand (task-2200, task-2294). Both moves are fast
  forwards only. Git refuses to push one that is not, and `merge --ff-only` refuses to touch a file the
  person has changed in the main checkout. A refusal is reported with the command to run, and the release
  goes on, because the tag is already published.
.PARAMETER Branch
  The branch the release was cut on.
#>
function Update-Main([string] $Branch) {
    if ($Branch -eq 'main') { return }
    Write-Step 'Bringing main up to this release'
    & git -C $Repo push origin HEAD:main
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  origin/main could not be moved to this release. Run: git push origin $($Branch):main" -ForegroundColor Yellow
    }
    $main = Get-MainCheckout
    if ((Resolve-Path $main).Path -eq (Resolve-Path $Repo).Path) { return }
    $there = (& git -C $main rev-parse --abbrev-ref HEAD).Trim()
    if ($there -ne 'main') {
        Write-Host "  The main checkout is on $there, so its main was left alone." -ForegroundColor Yellow
        return
    }
    & git -C $main merge --ff-only $Branch
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  The main checkout could not be moved to this release. Run: git -C $main merge --ff-only $Branch" -ForegroundColor Yellow
    }
}

# The steps that make a release safe to start again (`task-2312`), in a file of their own so
# `tools/release-steps.test.ps1` can run them with nothing published.
. (Join-Path $Here 'release-steps.ps1')

# ---------------------------------------------------------------------------------------------

Set-Location $Repo

Restore-Keyboard

$current = Get-CurrentVersion
$branch = (& git rev-parse --abbrev-ref HEAD).Trim()

# **A release that stopped after its tag carries on from the step that failed (`task-2312`).** HEAD is then the
# release's own commit, tagged with the version Cargo.toml already holds, and the record of the release says
# which publishing steps are done. Starting from the beginning there would cut a second version.
$taggedHere = @(& git tag --points-at HEAD) -contains "v$current"
$earlier = if ($taggedHere) { Read-Journal $current } else { $null }
$carryingOn = $taggedHere -and ($Resume -or ($earlier -and (@($earlier.steps) -notcontains 'done')))
if ($Resume -and -not $taggedHere) {
    throw "HEAD is not a release commit tagged v$current, so there is nothing to carry on. A release that stopped before its commit reuses what it built when it is started again, with no -Resume."
}

if ($carryingOn) {
    $next = $current
    Write-Host "Unluminous $next on ${branch}: carrying on the release that stopped after its tag"
} else {
    $next = if ($Version) { $Version } else { Get-NextVersion $current $Part }
    if ($next -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') { throw "$next is not a version of the form x.y.z." }
    if (-not (Test-Higher $next $current)) {
        throw "$next is not higher than the version already released, $current."
    }
    Write-Host "Unluminous $current -> $next  on $branch"
}

if ($WhatIf) {
    Write-Host ''
    Write-Host 'What would happen:' -ForegroundColor Yellow
    if ($carryingOn) {
        Write-Host "  carry on v$next from the first publishing step not yet done: $(if ($earlier) { "done so far: $(@($earlier.steps) -join ', ')" } else { 'no record, so every step is asked of its destination' })"
        return
    }
    Write-Host '  0. in seconds, before anything is built: fetch origin, refuse a tag that exists, merge origin/main,'
    Write-Host '     the GitHub credential, the macOS preflight and the window suite receipt; the public push check'
    Write-Host '     runs in the background while the suite does'
    Write-Host "     then, on $(Get-ProcessorSharePercent 0)% of the processors: contrast --check and node tools\test.mjs$(if ($FullTests) { ' --all' })"
    $dryRun = @((Join-Path $Repo 'tools\test.mjs'), '--dry-run')
    if ($FullTests) { $dryRun += '--all' }
    & node @dryRun | ForEach-Object { Write-Host "     $_" }
    Write-Host "  1. Cargo.toml version -> $next"
    Write-Host "  2. installer\windows\build.ps1$(if (-not $SkipInstall) { ' -Install' }), unless an earlier start built it from this source"
    Write-Host "  3. releases\UnluminousSetup-$next-x64.exe"
    if (-not $SkipMacos) { Write-Host "  3b. at the same time as 2: installer\macos\build-on-windows.ps1 -Notarize, then releases\Unluminous-$next-macos.zip" }
    Write-Host "  4. commit `"Unluminous $next`", tag v$next, push $branch"
    if (-not $SkipPublish) {
        Write-Host "  5. the GitHub release with the installers attached, on jasonmcaffee/unluminous"
        Write-Host "  6. node tools\publish-open-source.mjs --push --ref v$next, then the same release on $PublicRepository"
        if (-not $SkipSite) {
            $lead = '  7.'
            foreach ($site in $SitePublishers) {
                Write-Host "$lead pwsh $($site.Script)$(if ($site.Versioned) { " -Version $next -Channel nightly" })"
                $lead = '    '
            }
        }
    }
    return
}

Write-Step 'Checking the working tree'
$dirty = & git status --porcelain
if ($dirty) {
    Write-Host ($dirty -join "`n")
    throw 'The working tree is not clean. Commit the task''s own work first: a release built from a dirty checkout is one nobody can rebuild.'
}

# **A release that fails puts back what it changed (`task-2295`).** From here to the commit the release writes
# three files: `Cargo.lock` (the newest Inillucent), `Cargo.toml` (the version) and `CHANGELOG.md`. A failure
# in between used to leave them changed, and the next start refused the checkout as dirty, which is how
# task-2294's release needed a third start. The trap puts those three back with `git checkout --` and stops
# the macOS build this release started. It does nothing before `$script:Writing` is set, so a person's own
# change to `Cargo.toml` is never thrown away, and nothing after the commit, when the files are the release's.
#
# **What was built is kept (`task-2312`).** The installers stay in `releases\` and the record of the release
# names them with their hashes, so the next start reuses them when the source has not changed. After the tag
# the trap says that starting again carries on from the step that failed.
$script:Writing = $false
$script:Tagged = $false
$script:MacosChild = $null
trap {
    if ($script:Writing) {
        if ($script:MacosChild -and -not $script:MacosChild.HasExited) {
            & taskkill /PID $script:MacosChild.Id /T /F | Out-Null
        }
        & git -C $Repo checkout -- Cargo.toml Cargo.lock CHANGELOG.md
        Write-Host ''
        Write-Host 'The release stopped, and Cargo.toml, Cargo.lock and CHANGELOG.md were put back as they were.' -ForegroundColor Yellow
        Write-Host 'Starting it again reuses any installer it finished building from this source.' -ForegroundColor Yellow
        $script:Writing = $false
    } elseif ($script:Tagged) {
        Write-Host ''
        Write-Host "v$next is tagged and the steps that finished are recorded in $(Get-JournalPath $next)." -ForegroundColor Yellow
        Write-Host 'Run pwsh tools/release.ps1 again: it carries on from the step that failed.' -ForegroundColor Yellow
    }
    break
}

# Everything GitHub needs is checked here, before anything is changed, so that a missing credential
# cannot leave a pushed tag with no release behind it.
$gh = $null
$token = $null
if (-not $SkipPublish) {
    Write-Step 'Checking GitHub'
    $gh = Get-GitHubCli
    $token = Get-GitHubToken
    $env:GH_TOKEN = $token
    Invoke-Checked 'gh auth status' { & $gh auth status 2>&1 | Out-Null }
    Write-Host "GitHub CLI: $gh (authenticated)"
    # The public repository is checked here for the same reason the token is: a credential that
    # cannot reach it would otherwise be found out after the tag had been pushed.
    Invoke-Checked "gh can reach $PublicRepository" {
        & $gh repo view $PublicRepository --json name 2>&1 | Out-Null
    }
    Write-Host "Public repository: $PublicRepository (reachable)"
    if (-not $SkipSite) {
        foreach ($site in $SitePublishers) {
            if (-not (Test-Path $site.Script)) {
                throw "No publish script at $($site.Script) for $($site.Name). Set UNLUMINOUS_SITE_REPO or BLACK_RAINBOW_LABS_REPO, or pass -SkipSite and publish the sites by hand."
            }
        }
        Write-Host "Sites: $(($SitePublishers | ForEach-Object { $_.Name }) -join ', ')"
    }
}

$macosReason = $null
$kept = Join-Path $ReleasesDir "UnluminousSetup-$next-x64.exe"
$macosZip = $null

if ($carryingOn) {
    $script:Journal = if ($earlier) { $earlier } else { @{ version = $next; steps = @('tagged'); builds = @{} } }
    $script:Tagged = $true
    if (-not (Test-Path -LiteralPath $kept)) { throw "The Windows installer $kept is not there, so v$next cannot be published from here." }
    $zip = Join-Path $ReleasesDir "Unluminous-$next-macos.zip"
    if (Test-Path -LiteralPath $zip) { $macosZip = $zip } else { $macosReason = "$zip is not there" }
} else {
    # -----------------------------------------------------------------------------------------
    # Everything that can refuse a release in seconds, before anything slow (`task-2312`).
    #
    # The window suite's receipt, the public push and the macOS preflight were all asked after the suite had
    # run, and the public push only after both GitHub releases were made. Each was found minutes into a release
    # that then had to start again. They are asked here, in the order of how quickly they answer.
    # -----------------------------------------------------------------------------------------
    Write-Step 'Checking what can refuse this release before anything is built'
    # The steps that let this release be started again are checked first, in half a second, because a release
    # that stops after its tag depends on them to carry on.
    $stepsTest = & pwsh -NoProfile -File (Join-Path $Here 'release-steps.test.ps1') 2>&1
    if ($LASTEXITCODE -ne 0) {
        $stepsTest | ForEach-Object { Write-Host "  $_" }
        throw 'toolselease-steps.test.ps1 failed.'
    }
    Invoke-WithRetry 'git fetch origin' 3 { & git fetch origin --tags --quiet }
    if (& git rev-parse -q --verify "refs/tags/v$next") { throw "The tag v$next already exists here. Pass -Version for a different version, or delete the tag if it was never published." }
    if (& git ls-remote --tags origin "refs/tags/v$next") { throw "The tag v$next already exists on origin." }

    # **This branch takes what main has gained since it was cut.** The public source is a rewrite of the history
    # being released, so a release whose history lacks a commit `main` already published is refused by the public
    # push, which is how 1.3.0's push was refused after task-2310 reached `main` while it ran. Merged here, the
    # merge is what the suite runs and what is released, and `main` moves to it by a fast forward afterwards.
    & git merge-base --is-ancestor origin/main HEAD
    if ($LASTEXITCODE -ne 0) {
        $behind = (& git rev-list --count HEAD..origin/main).Trim()
        Write-Host "origin/main has $behind commit(s) this branch has not; merging them"
        & git merge --no-edit origin/main
        if ($LASTEXITCODE -ne 0) {
            & git merge --abort
            throw 'origin/main does not merge into this branch cleanly. Merge it by hand, run the window suite if it asks, and release again.'
        }
    }

    $macosBuild = Join-Path $Repo 'installer\macos\build-on-windows.ps1'
    if ($SkipMacos) {
        $macosReason = '-SkipMacos was passed'
    } elseif (-not (Test-Path $macosBuild)) {
        $macosReason = "$macosBuild is missing"
    } else {
        Get-TheMacosFiles
        $answer = & pwsh -NoProfile -File $macosBuild -Preflight 2>&1
        if ($LASTEXITCODE -ne 0) { $macosReason = ($answer | Out-String).Trim() }
    }
    # **A release without macOS is not a release (`task-2295`).** A failed preflight used to print one yellow line
    # and go on, and 0.64.4 went out Windows only that way (`task-2207`). It stops here instead, before anything is
    # built, unless `-SkipMacos` says to leave macOS out.
    if ($macosReason -and -not $SkipMacos) {
        throw "macOS cannot be built, so nothing was released: $macosReason`nFix that, or pass -SkipMacos to release Windows only."
    }
    $doMacos = -not $macosReason

    # `task-1984` T9: the window suite is deliberately manual and nothing recorded when it last passed, so a
    # release could be made from a commit it had never seen. It is checked first rather than after the suite,
    # because it answers in under a second and a refusal means running the window suite, which is the longest
    # thing a release can be sent back for.
    if (-not $SkipTests) {
        Invoke-Checked 'the window suite has passed at this commit' {
            & node (Join-Path $Repo 'tools\window-suite.mjs') --check
        }
    }

    # The public push is asked while the suite runs, because rewriting the history takes about 40 seconds.
    $publicCheck = $null
    $publicLog = Join-Path $Repo '_agent_output\release-logs\public-check.log'
    if (-not $SkipPublish) {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $publicLog) | Out-Null
        $publicCheck = Start-Process -FilePath 'node' -NoNewWindow -PassThru `
            -ArgumentList @("`"$(Join-Path $Repo 'tools\publish-open-source.mjs')`"", '--ref', 'HEAD', '--check-push') `
            -RedirectStandardOutput $publicLog -RedirectStandardError "$publicLog.err"
        $null = $publicCheck.Handle
    }

    $plan = [ordered]@{
        'Windows installer'    = 'yes'
        'macOS bundle'         = $(if ($doMacos) { 'yes' } else { "no - $macosReason" })
        'GitHub (private)'     = $(if ($SkipPublish) { 'no - -SkipPublish' } else { 'yes' })
        'GitHub (public)'      = $(if ($SkipPublish) { 'no - -SkipPublish' } else { 'yes' })
        'unluminous.com'       = $(if ($SkipPublish -or $SkipSite) { 'no - skipped' } else { 'yes' })
        'blackrainbowlabs.com' = $(if ($SkipPublish -or $SkipSite) { 'no - skipped' } else { 'yes' })
    }
    foreach ($destination in $plan.Keys) {
        Write-Host ("  {0,-22} {1}" -f $destination, $plan[$destination])
    }

    Write-Step 'Taking the newest Inillucent'
    $script:Writing = $true
    # `task-2293`: *"Make sure our build always pulls in the latest version of inillucent."* The manifest asks for
    # `inillucent-driver` 2.x and Cargo.lock held whichever 2.x was newest the day it was last written, which had
    # fallen six releases behind. The newest compatible release is taken here, before the suite, so what is tested
    # is what ships; Cargo.lock goes into the version commit with the manifest. A new major version is a change to
    # the manifest, made by hand.
    Invoke-WithRetry 'cargo update' 3 { & cargo update --manifest-path $Manifest -p inillucent-driver }

    Write-Step 'Running the suite'
    # `task-1922`: a release was tagged, installed and published before anything had run the tests. It is
    # the whole gate now rather than a second opinion -- `task-1928` took the continuous integration out,
    # so this is the only thing that runs the suite before a tag, and a release cannot be made from a
    # workspace whose tests do not pass.
    #
    # The screenshot suite is deliberately not here. It needs a graphics card and it needs a person to
    # open any image that changed, which is the one rule a script must not be allowed to satisfy on its
    # own. Its receipt was checked above.
    if (-not $SkipTests) {
        Invoke-WithoutCc {
            # No `changelog.mjs --check` (`task-2295`). Every ticket's commits put the file out of date, so it
            # refused a release at least six times over a file that "Writing CHANGELOG.md" below rewrites from
            # the history and commits with the version anyway.
            Invoke-Checked 'node tools\contrast.mjs --check' {
                & node (Join-Path $Repo 'tools\contrast.mjs') --check
            }
            # **Formatting, clippy and the tests, for the crates whose inputs have changed since they last passed
            # (`task-2240`, `task-2295`).** `tools/test.mjs` checks formatting first, hashes what each crate's
            # build and tests can read, skips a crate that has passed on exactly that content before (in this
            # checkout or in a ticket's worktree), and runs clippy, the Jupyter kernel tests and the rest in three
            # lanes at once. The notebook kernel tests run with UNLUMINOUS_REQUIRE_KERNEL=1, so a missing Python
            # fails them (task-2220). `-FullTests` runs every crate.
            $testArguments = @((Join-Path $Repo 'tools\test.mjs'))
            if ($FullTests) { $testArguments += '--all' }
            Invoke-Checked 'node tools\test.mjs' { & node @testArguments }
            # A test that failed and then passed on its own does not stop a release (`task-2364`), so the
            # release says which tests have done that lately, where the person releasing reads it.
            & node (Join-Path $Repo 'tools\flakes.mjs')
        }
    } else {
        Write-Host 'Skipped by -SkipTests.' -ForegroundColor Yellow
        Write-Host 'Nothing has checked this build: not the suite, not cargo fmt, not clippy, and not' -ForegroundColor Yellow
        Write-Host 'whether the window suite has ever been run against it.' -ForegroundColor Yellow
    }

    if ($publicCheck) {
        $publicCheck.WaitForExit()
        if ($publicCheck.ExitCode -ne 0) {
            Get-Content -LiteralPath $publicLog, "$publicLog.err" -ErrorAction SilentlyContinue | Select-Object -Last 15 | ForEach-Object { Write-Host "  $_" }
            throw "The public source could not be published from this history. The whole log: $publicLog"
        }
        Write-Host "The push to $PublicRepository would be a fast forward."
    }

    # **The record of this release (`task-2312`).** What an earlier start built is reused when it was built from
    # this same source: the same commit and the same Cargo.lock once the newest Inillucent was taken.
    $source = (& git rev-parse HEAD).Trim()
    $lock = Get-Sha256 (Join-Path $Repo 'Cargo.lock')
    $script:Journal = Read-Journal $next
    if (-not $script:Journal -or $script:Journal.source -ne $source -or $script:Journal.lock -ne $lock -or ($script:Journal.steps -contains 'committed')) {
        $script:Journal = @{ version = $next; source = $source; lock = $lock; steps = @(); builds = @{} }
    }
    if (-not $script:Journal.builds) { $script:Journal.builds = @{} }
    Save-Journal

    Write-Step "Setting the version to $next"
    Set-Version $next

    $reusedWindows = Get-ReusedBuild 'windows'
    $reusedMacos = if ($doMacos) { Get-ReusedBuild 'macos' } else { $null }

    # **The macOS half starts first and builds beside the Windows half (`task-2240`).** The two platforms
    # share the version and nothing else, and each build after an ordinary change is mostly the one
    # `unluminous-app` crate, which keeps about half of the processors busy on its own. One after another,
    # the Windows build and the two Apple architectures took 383 s after a one line change; all three at
    # once, each in its own target directory and on 80% of the processors, took 210 s. Notarisation then
    # waits on Apple while the Windows installer is compiled and installed. Its output goes to a log in
    # `_agent_output/release-logs/`.
    $macosProcess = $null
    $macosLog = $null
    if ($doMacos -and -not $reusedMacos) {
        $logs = Join-Path $Repo '_agent_output\release-logs'
        New-Item -ItemType Directory -Force -Path $logs | Out-Null
        $macosLog = Join-Path $logs "macos-$next.log"
        Write-Host "Started the macOS build beside the Windows one. Its log: $macosLog"
        $macosProcess = Start-MacosBuild
    } elseif ($reusedMacos) {
        Write-Host "The macOS archive an earlier start built from this source is reused: $reusedMacos"
    }

    Write-Step 'Building the installer, and installing it'
    $windowsProcess = $null
    if ($reusedWindows) {
        Write-Host "The Windows installer an earlier start built from this source is reused: $reusedWindows"
    } else {
        $build = Join-Path $Repo 'installer\windows\build.ps1'
        $arguments = @('-NoProfile', '-File', "`"$build`"")
        if (-not $SkipInstall) { $arguments += '-Install' }
        $windowsProcess = Start-Process -FilePath 'powershell' -NoNewWindow -PassThru -ArgumentList $arguments
        $null = $windowsProcess.Handle
    }

    # **Both builds are watched together, so a failure is said when it happens (`task-2312`).** The macOS build's
    # exit code used to be read only once the Windows installer was built and installed, so its file lock, which
    # is its first line, surfaced 4.5 to 13.8 minutes in. A macOS failure that came from a held file or the
    # network is started again at once, and cargo has already built everything the first try finished. Any other
    # macOS failure is printed straight away, and the Windows build carries on so the next start can reuse it.
    $macosTries = 1
    $macosFailure = $null
    $windowsFailure = $null
    while ($true) {
        if ($macosProcess -and $macosProcess.HasExited -and $macosProcess.ExitCode -ne 0 -and -not $macosFailure) {
            if ($macosTries -lt 3 -and (Test-TransientMacosFailure $macosLog)) {
                Write-Host ''
                Write-Host "The macOS build failed on something that may not happen twice; starting it again (try $($macosTries + 1))." -ForegroundColor Yellow
                Write-MacosLogTail
                Copy-Item -LiteralPath $macosLog -Destination "$macosLog.try$macosTries" -Force -ErrorAction SilentlyContinue
                $macosTries += 1
                $macosProcess = Start-MacosBuild
            } else {
                $macosFailure = "installer\macos\build-on-windows.ps1 failed with exit code $($macosProcess.ExitCode). The whole log: $macosLog"
                Write-Host ''
                Write-Host 'The macOS build failed:' -ForegroundColor Red
                Write-MacosLogTail
                if ($windowsProcess -and -not $windowsProcess.HasExited) {
                    Write-Host 'The Windows build carries on, so that the next start reuses it and builds only macOS.' -ForegroundColor Yellow
                }
            }
        }
        if ($windowsProcess -and $windowsProcess.HasExited -and $windowsProcess.ExitCode -ne 0 -and -not $windowsFailure) {
            $windowsFailure = 'installer\windows\build.ps1 failed.'
            # The macOS build started above is this release's own child, and a release that stops here
            # publishes nothing, so it is stopped with everything it started rather than left building.
            if ($macosProcess -and -not $macosProcess.HasExited) { & taskkill /PID $macosProcess.Id /T /F | Out-Null }
            throw $windowsFailure
        }
        $windowsDone = (-not $windowsProcess) -or $windowsProcess.HasExited
        $macosDone = (-not $macosProcess) -or $macosProcess.HasExited
        if ($windowsDone -and $macosDone) { break }
        Start-Sleep -Seconds 2
    }

    if ($reusedWindows) {
        $kept = $reusedWindows
    } else {
        $setup = Join-Path $Repo "installer\dist\UnluminousSetup-$next-x64.exe"
        if (-not (Test-Path $setup)) { throw "The installer was not written to $setup." }
        New-Item -ItemType Directory -Force -Path $ReleasesDir | Out-Null
        Copy-Item -Path $setup -Destination $kept -Force
        $script:Journal.builds['windows'] = @{ path = $kept; sha256 = (Get-Sha256 $kept) }
        Save-Journal
        Write-Host "Kept $kept"
    }

    if ($macosFailure) { throw $macosFailure }
    if ($doMacos) {
        if ($reusedMacos) {
            $macosZip = $reusedMacos
        } else {
            Write-Step 'The macOS bundle, built, signed and notarised'
            Write-MacosLogTail
            $macosZip = Join-Path $ReleasesDir "Unluminous-$next-macos.zip"
            if (-not (Test-Path $macosZip)) { throw "The macOS archive was not written to $macosZip." }
            $script:Journal.builds['macos'] = @{ path = $macosZip; sha256 = (Get-Sha256 $macosZip) }
            Save-Journal
            Write-Host "Kept $macosZip"
        }
    } else {
        Write-Host ''
        Write-Host "macOS is not in this release: $macosReason" -ForegroundColor Yellow
    }

    # **The suite and the installer leave the previous build's artifacts behind, so the release takes
    # them back (`task-2011`).** Cargo never removes anything from a target directory: every build writes
    # a fresh hash-suffixed copy of each artifact and leaves its predecessor exactly where it is, for
    # ever. Measured on this checkout, target had reached 153.7 GB of which 137.8 GB was copies cargo
    # could no longer reach. Nothing cargo can still reach is removed, so the next build is not slowed.
    #
    # A failure here is reported and does not stop the release. The release is about what reaches the
    # person's desktop, and disk housekeeping is not a reason to abandon a tag that is already built.
    Write-Step 'Reclaiming superseded build output'
    & pwsh -NoProfile -File (Join-Path $Repo 'tools\prune-target.ps1') -Quiet
    if ($LASTEXITCODE -ne 0) {
        Write-Host '  the prune reported a problem -- the release is unaffected' -ForegroundColor Yellow
    }

    # **Written from the history rather than kept by hand**, so it cannot fall behind (`task-1804` §6), and
    # told which version it is cutting (`task-1984`), because the tag does not exist yet at this point.
    Write-Step 'Writing CHANGELOG.md'
    & node (Join-Path $Repo 'tools\changelog.mjs') --release $next
    if ($LASTEXITCODE -ne 0) { throw 'tools/changelog.mjs failed.' }

    Write-Step "Committing and tagging v$next"
    Invoke-Checked 'git add' { & git add -- Cargo.toml Cargo.lock CHANGELOG.md }
    Invoke-Checked 'git commit' { & git commit -m "Unluminous $next" | Out-Null }
    $script:Writing = $false
    Invoke-Checked 'git tag' { & git tag -a "v$next" -m "Unluminous $next" }
    $script:Tagged = $true
    Add-JournalStep 'committed'
    Add-JournalStep 'tagged'
}

if (-not (Test-JournalStep 'pushed')) {
    Invoke-WithRetry 'git push' 3 { & git push origin $branch }
    Invoke-WithRetry 'git push of the tag' 3 { & git push origin "v$next" }
    Add-JournalStep 'pushed'
    Update-Main $branch
}

if ($SkipPublish) {
    Write-Host ''
    Write-Host "Tagged and pushed v$next. Not published, because -SkipPublish was given." -ForegroundColor Green
    Write-StepTimes $next
    return
}

if (-not $Notes) { $Notes = (& git log -1 --pretty=%s "v$next^").Trim() }
$body = @"
$Notes

Windows: download **UnluminousSetup-$next-x64.exe** below and run it. It installs into
%LOCALAPPDATA%\Programs\Unluminous with no elevation prompt, and puts ``unluminous`` and ``unluminous-cli`` on the PATH.

``Unluminous -> About Unluminous`` in the window says which build this is.
"@
if ($macosZip) {
    $body += @"

macOS: download **Unluminous-$next-macos.zip**, unpack it, and drag Unluminous.app into Applications. It is
signed with a Developer ID and notarised, so it opens with no warning. ``unluminous`` and ``unluminous-cli`` are
inside ``Unluminous.app/Contents/MacOS``.
"@
}
$assets = @($kept)
if ($macosZip) { $assets += $macosZip }

if (-not (Test-JournalStep 'private')) {
    Write-Step "Publishing the GitHub release"
    Invoke-WithRetry "The GitHub release v$next on jasonmcaffee/unluminous" 3 { Publish-GitHubRelease 'jasonmcaffee/unluminous' $assets $body }
    Add-JournalStep 'private'
}
$url = (& $gh release view "v$next" --repo jasonmcaffee/unluminous --json url --jq .url).Trim()

# **The public repository is the one anybody else can see, so it gets the same release.**
# `tools/publish-open-source.mjs` is a pure function of the history — the same commits give the same
# hashes every time — so this is an ordinary push that appends the new commits and the new tag, and
# the release is then created against that tag. Running it twice is harmless: a tag the public
# repository already has is left alone, and a push of the same commit changes nothing.
#
# **It publishes the tag, not `origin/main`.** A release cut from a ticket worktree pushes the ticket's
# branch, so `origin/main` can still be the version before; 0.84.5 was published to the public
# repository with 0.84.4's source for that reason. The tag is this release whatever the branch.
if (-not (Test-JournalStep 'public-source')) {
    Write-Step "Publishing the source on $PublicRepository"
    Invoke-WithRetry "The public source of v$next" 3 { & node (Join-Path $Repo 'tools\publish-open-source.mjs') --push --ref "v$next" }
    Add-JournalStep 'public-source'
}
if (-not (Test-JournalStep 'public')) {
    Write-Step "Publishing the release on $PublicRepository"
    Invoke-WithRetry "The GitHub release v$next on $PublicRepository" 3 { Publish-GitHubRelease $PublicRepository $assets $body }
    Add-JournalStep 'public'
}
$publicUrl = (& $gh release view "v$next" --repo $PublicRepository --json url --jq .url).Trim()

# **And the sites last**, because they are the one step that can fail on somebody else's toolchain
# and both releases above are published by the time they run. A site's own check of what the edge serves
# failed seven times on a network error that a second publish did not see, so each site is tried twice
# before the release stops (`task-2312`). A site a release did not reach answers `update check` with the
# version before this one, which is worse than not answering at all.
if (-not $SkipSite) {
    # The product site takes its installers out of `releases/`, and it reads the main checkout's unless
    # told otherwise. A release cut from a ticket worktree writes them into the worktree's own
    # `releases/`, so 0.84.5 was published to GitHub and refused by the site with "has no installer".
    # The folder this release wrote is the one to read, and the changelog it wrote likewise.
    if (-not $env:UNLUMINOUS_RELEASES_DIR) { $env:UNLUMINOUS_RELEASES_DIR = Join-Path $Repo 'releases' }
    if (-not $env:UNLUMINOUS_CHANGELOG) { $env:UNLUMINOUS_CHANGELOG = Join-Path $Repo 'CHANGELOG.md' }
    foreach ($site in $SitePublishers) {
        if (Test-JournalStep "site:$($site.Name)") { continue }
        Write-Step "Publishing $($site.Name)"
        $arguments = @('-NoProfile', '-File', $site.Script)
        # Only the product site is told which version: the parent page reads it out of the manifest
        # the product site has just published, which is the whole reason they go in this order. It is
        # told the nightly channel, so the page and latest.json stay on the stable version (task-2328).
        if ($site.Versioned) { $arguments += @('-Version', $next, '-Channel', 'nightly') }
        Invoke-WithRetry "Publishing $($site.Name)" 2 { & pwsh @arguments }
        Add-JournalStep "site:$($site.Name)"
    }
} else {
    Write-Host ''
    Write-Host 'The sites were not published, so update check still answers with the version before this one.' -ForegroundColor Yellow
    foreach ($site in $SitePublishers) {
        Write-Host "Run: pwsh $($site.Script)$(if ($site.Versioned) { " -Version $next -Channel nightly" })" -ForegroundColor Yellow
    }
}

# ---------------------------------------------------------------------------------------------
# What reached where, asked of the destinations rather than assumed from exit codes.
#
# `task-1995`. Six destinations scrolled past in the output above and nobody could say afterwards
# which of them actually changed - which is how 0.53.0 went out with no macOS archive on either
# GitHub release and nobody noticed until somebody looked. The site is asked what it serves; the two
# GitHub releases are asked for their assets.
# ---------------------------------------------------------------------------------------------
function Test-Destination {
    <#
    .SYNOPSIS
        Runs one check and turns it into a line for the report.

    .PARAMETER Name
        What is being checked.

    .PARAMETER Check
        A scriptblock returning $null when the destination is right, or the reason it is not.
    #>
    param([string] $Name, [scriptblock] $Check)
    $why = try { & $Check } catch { $_.Exception.Message }
    $state = if ($why) { 'NOT THERE' } else { 'ok' }
    $colour = if ($why) { 'Red' } else { 'Green' }
    Write-Host ("  {0,-24} {1,-10} {2}" -f $Name, $state, $why) -ForegroundColor $colour
    # **Recorded, because printing it in red was not enough (task-1995).** Every check ran, the ones
    # that failed said NOT THERE, and the script then printed "Unluminous <version> is released" and
    # exited 0 - so a release that never reached the site looked exactly like one that did. The
    # whole report is still printed first; the exit code is decided after it.
    if ($why) { $script:MissedDestinations += "$Name - $why" }
}

$script:MissedDestinations = @()

Write-Step "What Unluminous $next reached"
Test-Destination 'GitHub (private)' {
    $assets = & $gh release view "v$next" --repo jasonmcaffee/unluminous --json assets --jq '.assets[].name' 2>$null
    if (-not $assets) { return 'no assets' }
    # **`-notmatch` against an array filters it**, it does not answer a question: `$assets` holds both
    # asset names, so `$assets -notmatch 'macos'` is the Windows installer's name — a non-empty array,
    # which is true — and this said the macOS archive was missing on every release that had one.
    # Measured on 0.54.0, where both archives were attached to both repositories and this reported
    # `NOT THERE`. `-not ($assets -match 'macos')` is the question: filter for the ones that match, and
    # ask whether anything did.
    if ($macosZip -and -not ($assets -match 'macos')) { return 'the macOS archive is not attached' }
    $null
}
Test-Destination 'GitHub (public)' {
    $assets = & $gh release view "v$next" --repo $PublicRepository --json assets --jq '.assets[].name' 2>$null
    if (-not $assets) { return 'no assets' }
    $null
}
if (-not $SkipSite) {
    # **Every platform this release ships, checked the same way (task-1995).** The manifest's version
    # and a HEAD on the macOS zip left the Windows installer unchecked entirely - it could have
    # failed to copy and this would have said the site was fine. And a 200 says a file is there, not
    # that it is this release's: the manifest states each artifact's size, so the size the server
    # actually serves is what decides.
    # The nightly channel, which is where a release goes (task-2328). The page and latest.json are the
    # stable version's, and `tools/promote.ps1` checks those when it promotes one.
    Test-Destination 'unluminous.com, nightly' {
        $manifest = (Invoke-WebRequest -Uri 'https://unluminous.com/releases/nightly.json' -UseBasicParsing -TimeoutSec 30).Content | ConvertFrom-Json
        if ($manifest.version -ne $next) { return "the nightly manifest says $($manifest.version)" }

        $platforms = @(@{ Platform = 'Windows'; Url = $manifest.installer; Bytes = $manifest.installerBytes })
        if ($macosZip) {
            if (-not $manifest.macos) { return 'this release has a macOS archive and the manifest does not name it' }
            $platforms += @{ Platform = 'macOS'; Url = $manifest.macos; Bytes = $manifest.macosBytes }
        }

        foreach ($one in $platforms) {
            if (-not $one.Url) { return "the manifest has no $($one.Platform) download" }
            if ($one.Url -notlike "*$next*") { return "the $($one.Platform) download is $($one.Url), which is not $next" }
            $head = Invoke-WebRequest -Uri $one.Url -Method Head -UseBasicParsing -TimeoutSec 30
            if ([int] $head.StatusCode -ne 200) { return "$($one.Platform) answered $($head.StatusCode)" }
            $served = [int64] $head.Headers['Content-Length'][0]
            if ($served -ne [int64] $one.Bytes) {
                return "$($one.Platform) is served as $served bytes and the manifest says $($one.Bytes)"
            }
        }
        $null
    }
}
Write-Host ("  {0,-24} {1}" -f 'macOS in this release', $(if ($macosZip) { 'yes, signed and notarised' } else { "no - $macosReason" }))

Write-Host ''
Write-Host "Unluminous $next is released: $url" -ForegroundColor Green
Write-Host "Public source and release:   $publicUrl" -ForegroundColor Green
if (-not $SkipSite) {
    Write-Host 'Nightly channel:             https://unluminous.com/releases/nightly.json' -ForegroundColor Green
    Write-Host "Promote it to stable:        pwsh tools/promote.ps1 -Version $next" -ForegroundColor Green
}
if (-not $SkipInstall) {
    Write-Host "Installed at $(Join-Path $env:LOCALAPPDATA 'Programs\Unluminous\unluminous.exe')" -ForegroundColor Green
}

# **The exit code, decided after the whole report is printed (task-1995).** A destination that was
# not there printed NOT THERE in red and the script carried on to say the release was done and exit
# 0, so a release that never reached the site was indistinguishable from one that did - to a person
# skimming, and to anything that runs this and reads its exit code. Every check still runs and every
# line is still printed; only the ending changes.
Write-StepTimes $next

if ($script:MissedDestinations.Count -eq 0) { Add-JournalStep 'done' }

if ($script:MissedDestinations.Count -gt 0) {
    Write-Host ''
    Write-Host "Unluminous $next did not reach $($script:MissedDestinations.Count) destination(s):" -ForegroundColor Red
    $script:MissedDestinations | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    Write-Host 'The tag and the releases are published; fix these and run pwsh tools/release.ps1 again, which carries on.' -ForegroundColor Red
    exit 1
}
