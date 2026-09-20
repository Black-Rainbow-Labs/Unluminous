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
    7. Publishes both sites. unluminous.com gets the new installer, the page that prints its size and
       hash, and the manifest at /releases/latest.json that the update check asks before it asks
       anything else; blackrainbowlabs.com then reads that manifest for the version it prints.

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
    [switch] $SkipMacos
)

$ErrorActionPreference = 'Stop'

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
$SiteRepo = $env:UNLUMINOUS_SITE_REPO
if (-not $SiteRepo) { $SiteRepo = 'C:/jason/dev/unluminous-site' }
$ParentSiteRepo = $env:BLACK_RAINBOW_LABS_REPO
if (-not $ParentSiteRepo) { $ParentSiteRepo = 'C:/jason/dev/blackrainbowlabs' }
$SitePublishers = @(
    @{ Name = 'unluminous.com'; Script = (Join-Path $SiteRepo 'scripts/publish.ps1'); Versioned = $true },
    @{ Name = 'blackrainbowlabs.com'; Script = (Join-Path $ParentSiteRepo 'scripts/publish.ps1'); Versioned = $false }
)

function Write-Step([string] $Message) {
    Write-Host ''
    Write-Host "==> $Message" -ForegroundColor Cyan
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

<#
.SYNOPSIS
  The GitHub CLI, installing it with winget the first time.
.DESCRIPTION
  The same choice installer\windows\build.ps1 makes about Inno Setup: the one thing this needs that a
  machine able to build Unluminous does not already have is installed here rather than described in a
  document. It is looked for on the PATH first, then where the MSI puts it, because a shell opened
  before the install will not have the new PATH.
#>
function Get-GitHubCli {
    $onPath = Get-Command 'gh' -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    $candidates = @(
        (Join-Path $env:ProgramFiles 'GitHub CLI\gh.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'GitHub CLI\gh.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\GitHub CLI\gh.exe')
    )
    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) { return $candidate }
    }

    Write-Step 'Installing the GitHub CLI, which is not on this machine'
    $winget = Get-Command 'winget' -ErrorAction SilentlyContinue
    if (-not $winget) {
        throw 'gh is not installed and winget is not available to install it. Install it from https://cli.github.com and run this again.'
    }
    & winget install --id GitHub.cli --exact --silent `
        --accept-source-agreements --accept-package-agreements --disable-interactivity
    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) { return $candidate }
    }
    throw 'The GitHub CLI was installed but gh.exe could not be found afterwards.'
}

<#
.SYNOPSIS
  A GitHub token, from the credential helper git is already using.
.DESCRIPTION
  Pushing to this repository already works, which means a credential for github.com is already
  stored, and `git credential fill` is the supported way to ask for it. Using it means there is no
  second credential to set up and nothing new written to disk. GH_TOKEN wins if it is already set,
  which is how a machine with its own token keeps using it.

  Nothing here is printed or returned into the transcript beyond a yes or no.
#>
function Get-GitHubToken {
    if ($env:GH_TOKEN) { return $env:GH_TOKEN }
    if ($env:GITHUB_TOKEN) { return $env:GITHUB_TOKEN }
    # The request goes in through a file rather than a pipe. Windows PowerShell 5.1 does not deliver a
    # piped string to a native program's standard input in a form `git credential` accepts — it
    # answers `refusing to work with credential missing protocol field` — and a redirection from a
    # file does. The file holds the protocol and the host and no secret; the answer, which does hold
    # one, is only ever in memory.
    $ask = Join-Path ([System.IO.Path]::GetTempPath()) ("unluminous-credential-" + [guid]::NewGuid().ToString('N') + ".txt")
    try {
        Set-Content -Path $ask -Value "protocol=https`nhost=github.com`n" -NoNewline -Encoding ascii
        $answer = & cmd /c "git credential fill < `"$ask`"" 2>$null
    } finally {
        Remove-Item -Path $ask -Force -ErrorAction SilentlyContinue
    }
    $line = $answer | Where-Object { $_ -like 'password=*' } | Select-Object -First 1
    if (-not $line) {
        throw 'No GitHub credential is stored for github.com. Run `gh auth login` (or set GH_TOKEN) and run this again.'
    }
    return $line.Substring('password='.Length)
}

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

# ---------------------------------------------------------------------------------------------

Set-Location $Repo

Restore-Keyboard

$current = Get-CurrentVersion
$next = if ($Version) { $Version } else { Get-NextVersion $current $Part }
if ($next -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') { throw "$next is not a version of the form x.y.z." }
if (-not (Test-Higher $next $current)) {
    throw "$next is not higher than the version already released, $current."
}

$branch = (& git rev-parse --abbrev-ref HEAD).Trim()
Write-Host "Unluminous $current -> $next  on $branch"

if ($WhatIf) {
    Write-Host ''
    Write-Host 'What would happen:' -ForegroundColor Yellow
    Write-Host "  0. cargo fmt --check, cargo clippy -D warnings, changelog --check, contrast --check,"
    Write-Host "     the window suite's receipt, then cargo test --workspace --exclude unluminous-app"
    Write-Host "     and -p unluminous-app --lib --bins"
    Write-Host "  1. Cargo.toml version -> $next"
    Write-Host "  2. installer\windows\build.ps1$(if (-not $SkipInstall) { ' -Install' })"
    Write-Host "  3. releases\UnluminousSetup-$next-x64.exe"
    if (-not $SkipMacos) { Write-Host "  3b. installer\macos\build-on-windows.ps1 -Notarize, then releases\Unluminous-$next-macos.zip, if its preflight passes" }
    Write-Host "  4. commit `"Unluminous $next`", tag v$next, push $branch"
    if (-not $SkipPublish) {
        Write-Host "  5. gh release create v$next with the installer attached, on jasonmcaffee/unluminous"
        Write-Host "  6. node tools\publish-open-source.mjs --push, then the same release on $PublicRepository"
        if (-not $SkipSite) {
            $lead = '  7.'
            foreach ($site in $SitePublishers) {
                Write-Host "$lead pwsh $($site.Script)$(if ($site.Versioned) { " -Version $next" })"
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

Write-Step 'Running the suite'
# `task-1922`: a release was tagged, installed and published before anything had run the tests. It is
# the whole gate now rather than a second opinion -- `task-1928` took the continuous integration out,
# so this is the only thing that runs the suite before a tag, and a release cannot be made from a
# workspace whose tests do not pass.
#
# The screenshot suite is deliberately not here. It needs a graphics card and it needs a person to
# open any image that changed, which is the one rule a script must not be allowed to satisfy on its
# own -- and with no continuous integration there is nowhere else it runs either. So a release says
# plainly that it did not run it, rather than leaving that unsaid.
#
# `cargo fmt --check` and `cargo clippy` are here for the same reason the tests are. `task-1928`
# moved the suite into this script when it removed the continuous integration and left those two
# behind, so between then and `task-1984` seven files drifted out of format and one clippy error
# arrived, with nothing to say so.
if (-not $SkipTests) {
    Invoke-WithoutCc {
        Invoke-Checked 'cargo fmt --all -- --check' {
            & cargo fmt --manifest-path $Manifest --all -- --check
        }
        Invoke-Checked 'cargo clippy --workspace --all-targets -- -D warnings' {
            & cargo clippy --manifest-path $Manifest --workspace --all-targets -- -D warnings
        }
        Invoke-Checked 'node tools\changelog.mjs --check' {
            & node (Join-Path $Repo 'tools\changelog.mjs') --check
        }
        Invoke-Checked 'node tools\contrast.mjs --check' {
            & node (Join-Path $Repo 'tools\contrast.mjs') --check
        }
        Invoke-Checked 'cargo test --workspace --exclude unluminous-app' {
            & cargo test --manifest-path $Manifest --workspace --exclude unluminous-app
        }
        Invoke-Checked 'cargo test -p unluminous-app --lib --bins' {
            & cargo test --manifest-path $Manifest -p unluminous-app --lib --bins
        }
    }
    Write-Host 'The window through wgpu is not run here: it needs a graphics card and a person to look'
    Write-Host "at any image that changed. Run it by hand: cargo test -p unluminous-app --test '*' --no-fail-fast"
    # What is checked instead is that somebody did. `task-1984` T9: the window suite is deliberately
    # manual and nothing recorded when it last passed, so a release could be made from a commit it
    # had never seen.
    Invoke-Checked 'the window suite has passed at this commit' {
        & node (Join-Path $Repo 'tools\window-suite.mjs') --check
    }
} else {
    Write-Host 'Skipped by -SkipTests.' -ForegroundColor Yellow
    Write-Host 'Nothing has checked this build: not the suite, not cargo fmt, not clippy, and not' -ForegroundColor Yellow
    Write-Host 'whether the window suite has ever been run against it.' -ForegroundColor Yellow
}

# Everything GitHub needs is checked here, before anything is changed, so that a missing credential
# cannot leave a pushed tag with no release behind it.
$gh = $null
$token = $null
if (-not $SkipPublish) {
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

# ---------------------------------------------------------------------------------------------
# What this release will and will not reach, decided before anything is written.
#
# `task-1995`. A release publishes to six destinations - two GitHub repositories, two sites and two
# platforms - and until now it found out one at a time, after the tag had been pushed. A tag is the
# one step that cannot be taken back quietly, so the answers are gathered here, while the tree is
# still untouched.
# ---------------------------------------------------------------------------------------------
Write-Step 'What this release will reach'
$macosBuild = Join-Path $Repo 'installer\macos\build-on-windows.ps1'
$macosReason = $null
if ($SkipMacos) {
    $macosReason = '-SkipMacos was passed'
} elseif (-not (Test-Path $macosBuild)) {
    $macosReason = "$macosBuild is missing"
} else {
    $answer = & pwsh -NoProfile -File $macosBuild -Preflight 2>&1
    if ($LASTEXITCODE -ne 0) { $macosReason = ($answer | Out-String).Trim() }
}
$doMacos = -not $macosReason

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

Write-Step "Setting the version to $next"
Set-Version $next

Write-Step 'Building the installer, and installing it'
$build = Join-Path $Repo 'installer\windows\build.ps1'
$arguments = @('-File', $build)
if (-not $SkipInstall) { $arguments += '-Install' }
& powershell @arguments
if ($LASTEXITCODE -ne 0) { throw 'installer\windows\build.ps1 failed.' }

$setup = Join-Path $Repo "installer\dist\UnluminousSetup-$next-x64.exe"
if (-not (Test-Path $setup)) { throw "The installer was not written to $setup." }
New-Item -ItemType Directory -Force -Path $ReleasesDir | Out-Null
$kept = Join-Path $ReleasesDir "UnluminousSetup-$next-x64.exe"
Copy-Item -Path $setup -Destination $kept -Force
Write-Host "Kept $kept"

# The macOS half, when it is asked for. It is a separate script rather than a branch in here for the
# same reason installer\windows\build.ps1 is: the two platforms share the version and nothing else.
# The bundle is signed and notarised by that script, so what comes back is ready to attach.
$macosZip = $null
if ($doMacos) {
    Write-Step 'Building, signing and notarising the macOS bundle'
    & pwsh -NoProfile -File $macosBuild -Version $next -Notarize
    if ($LASTEXITCODE -ne 0) { throw 'installer\macos\build-on-windows.ps1 failed.' }
    $macosZip = Join-Path $ReleasesDir "Unluminous-$next-macos.zip"
    if (-not (Test-Path $macosZip)) { throw "The macOS archive was not written to $macosZip." }
    Write-Host "Kept $macosZip"
} else {
    Write-Host ''
    Write-Host "macOS is not in this release: $macosReason" -ForegroundColor Yellow
}

# **Written from the history rather than kept by hand**, so it cannot fall behind. `task-1804` §6:
# 201 commits and 34 minor versions with no record of what changed that a person could read. It runs
# before the commit so the changelog for this release is in the release's own commit -- the entries
# for the work are already in the history, and the version this makes is the boundary they sit under.
#
# **And it is told which version it is cutting** (`task-1984`). Without that, the tag does not exist
# yet at this point, so the script cannot see the version it is about to make and writes everything it
# holds under `## Unreleased` -- and the tag made three lines below then leaves the file stale from
# that instant. 0.50.0 and 0.51.0 were both cut that way, and `--check` reported each of them one
# release late, because what it compares is the released history and a version only joins that when it
# is tagged.
# **The suite and the installer leave the previous build's artifacts behind, so the release takes
# them back (`task-2011`).** Cargo never removes anything from a target directory: every build writes
# a fresh hash-suffixed copy of each artifact and leaves its predecessor exactly where it is, for
# ever. Measured on this checkout, target had reached 153.7 GB of which 137.8 GB was copies cargo
# could no longer reach, and the shape of it is this script's own suite -- fourteen screenshot-test
# binaries at about 220 MB of executable and debug symbols apiece, written afresh every run.
#
# A release is the right moment to reclaim them. It is the largest single producer of them, and what
# it has just built is the newest generation, which is the one kept. Nothing cargo can still reach is
# removed, so the next build is not slowed by this: measured, a no-op build straight afterwards still
# finishes in half a second having compiled nothing.
#
# A failure here is reported and does not stop the release. The release is about what reaches the
# person's desktop, and disk housekeeping is not a reason to abandon a tag that is already built.
Write-Step 'Reclaiming superseded build output'
& pwsh -NoProfile -File (Join-Path $Repo 'tools\prune-target.ps1') -Quiet
if ($LASTEXITCODE -ne 0) {
    Write-Host '  the prune reported a problem -- the release is unaffected' -ForegroundColor Yellow
}

Write-Step 'Writing CHANGELOG.md'
& node (Join-Path $Repo 'tools\changelog.mjs') --release $next
if ($LASTEXITCODE -ne 0) { throw 'tools/changelog.mjs failed.' }

Write-Step "Committing and tagging v$next"
Invoke-Checked 'git add' { & git add -- Cargo.toml Cargo.lock CHANGELOG.md }
Invoke-Checked 'git commit' { & git commit -m "Unluminous $next" | Out-Null }
Invoke-Checked 'git tag' { & git tag -a "v$next" -m "Unluminous $next" }
Invoke-Checked 'git push' { & git push origin $branch }
Invoke-Checked 'git push --tags' { & git push origin "v$next" }

if ($SkipPublish) {
    Write-Host ''
    Write-Host "Tagged and pushed v$next. Not published, because -SkipPublish was given." -ForegroundColor Green
    return
}

Write-Step "Publishing the GitHub release"
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
& $gh release create "v$next" @assets --repo jasonmcaffee/unluminous --title "Unluminous $next" --notes $body
if ($LASTEXITCODE -ne 0) {
    throw "The tag v$next was pushed but the release was not created. Run: gh release create v$next `"$kept`" --title `"Unluminous $next`""
}

$url = (& $gh release view "v$next" --repo jasonmcaffee/unluminous --json url --jq .url).Trim()

# **The public repository is the one anybody else can see, so it gets the same release.**
# `tools/publish-open-source.mjs` is a pure function of the history — the same commits give the same
# hashes every time — so this is an ordinary push that appends the new commits and the new tag, and
# the release is then created against that tag.
Write-Step "Publishing the source and the release on $PublicRepository"
& node (Join-Path $Repo 'tools\publish-open-source.mjs') --push
if ($LASTEXITCODE -ne 0) {
    throw "The release v$next exists on the private repository but the public source was not pushed. Run: node tools\publish-open-source.mjs --push"
}
& $gh release create "v$next" @assets --repo $PublicRepository --title "Unluminous $next" --notes $body
if ($LASTEXITCODE -ne 0) {
    throw "The public source was pushed but its release was not created. Run: gh release create v$next `"$kept`" --repo $PublicRepository --title `"Unluminous $next`""
}
$publicUrl = (& $gh release view "v$next" --repo $PublicRepository --json url --jq .url).Trim()

# **And the sites last**, because they are the one step that can fail on somebody else's toolchain
# and both releases above are published by the time they run. It is still a hard failure: a site a
# release did not reach answers `update check` with the version before this one, which is worse than
# not answering at all, and the message says exactly what to run again.
if (-not $SkipSite) {
    foreach ($site in $SitePublishers) {
        Write-Step "Publishing $($site.Name)"
        $arguments = @('-NoProfile', '-File', $site.Script)
        # Only the product site is told which version: the parent page reads it out of the manifest
        # the product site has just published, which is the whole reason they go in this order.
        if ($site.Versioned) { $arguments += @('-Version', $next) }
        & pwsh @arguments
        if ($LASTEXITCODE -ne 0) {
            throw "Unluminous $next is released but $($site.Name) still says the version before it. Run: pwsh $($site.Script)$(if ($site.Versioned) { " -Version $next" })"
        }
    }
} else {
    Write-Host ''
    Write-Host 'The sites were not published, so update check still answers with the version before this one.' -ForegroundColor Yellow
    foreach ($site in $SitePublishers) {
        Write-Host "Run: pwsh $($site.Script)$(if ($site.Versioned) { " -Version $next" })" -ForegroundColor Yellow
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
    Test-Destination 'unluminous.com' {
        $manifest = (Invoke-WebRequest -Uri 'https://unluminous.com/releases/latest.json' -UseBasicParsing -TimeoutSec 30).Content | ConvertFrom-Json
        if ($manifest.version -ne $next) { return "the manifest says $($manifest.version)" }

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
    Test-Destination 'unluminous.com, page' {
        $page = (Invoke-WebRequest -Uri 'https://unluminous.com/' -UseBasicParsing -TimeoutSec 30).Content
        if ($page -notlike "*UnluminousSetup-$next-x64.exe*") { return "the page does not link the $next Windows installer" }
        if ($macosZip -and $page -notlike "*Unluminous-$next-macos.zip*") { return "the page does not link the $next macOS archive" }
        $null
    }
}
Write-Host ("  {0,-24} {1}" -f 'macOS in this release', $(if ($macosZip) { 'yes, signed and notarised' } else { "no - $macosReason" }))

Write-Host ''
Write-Host "Unluminous $next is released: $url" -ForegroundColor Green
Write-Host "Public source and release:   $publicUrl" -ForegroundColor Green
if (-not $SkipSite) {
    Write-Host 'Download and update check:   https://unluminous.com/#install' -ForegroundColor Green
}
if (-not $SkipInstall) {
    Write-Host "Installed at $(Join-Path $env:LOCALAPPDATA 'Programs\Unluminous\unluminous.exe')" -ForegroundColor Green
}

# **The exit code, decided after the whole report is printed (task-1995).** A destination that was
# not there printed NOT THERE in red and the script carried on to say the release was done and exit
# 0, so a release that never reached the site was indistinguishable from one that did - to a person
# skimming, and to anything that runs this and reads its exit code. Every check still runs and every
# line is still printed; only the ending changes.
if ($script:MissedDestinations.Count -gt 0) {
    Write-Host ''
    Write-Host "Unluminous $next did not reach $($script:MissedDestinations.Count) destination(s):" -ForegroundColor Red
    $script:MissedDestinations | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    Write-Host 'The tag and the releases are published; fix these and run the step again.' -ForegroundColor Red
    exit 1
}
