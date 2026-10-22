<#
.SYNOPSIS
  Promotes a nightly Unluminous to stable: the same installer, published again on the stable channel.

.DESCRIPTION
  `task-2328`. Every release `tools/release.ps1` cuts is a nightly: its GitHub releases are prereleases and
  unluminous.com lists it in /releases/nightly.json only, so nobody following the stable channel, which is
  everybody unless they chose otherwise, is offered it. Once a nightly has been used and has held up, this
  makes it the stable version. Nothing is built: the installer and the macOS archive the nightly published
  are published again with the sizes and hashes recorded when they were, so the bytes a stable window
  installs are the bytes somebody already ran.

  What it does:

    1. Finds the version: -Version, or the newest on the nightly channel. A version that is not on the
       nightly channel is refused, because promoting means promoting something that was tested.
    2. Publishes unluminous.com with `-Channel stable`: the version goes to the top of /releases/stable.json,
       the page offers it, and /releases/latest.json names it, which is what every Unluminous released before
       the channels existed reads.
    3. Publishes blackrainbowlabs.com, which prints the version /releases/latest.json names.
    4. Marks the GitHub release on both repositories as a full release and the latest one, so GitHub's
       `releases/latest`, which a window asks when unluminous.com does not answer, names it too.
    5. Asks every destination what it serves, and exits 1 when one of them is not right.

  Running it twice is harmless: a version already at the top of stable stays there.

.PARAMETER Version
  The version to promote. The newest on the nightly channel when left out.

.PARAMETER SkipSite
  Leave both sites alone and only change the GitHub releases.

.PARAMETER SkipGitHub
  Leave the GitHub releases alone and only publish the sites.

.PARAMETER WhatIf
  Say what would happen and change nothing.

.EXAMPLE
  pwsh tools/promote.ps1
  pwsh tools/promote.ps1 -Version 1.9.0
#>
[CmdletBinding()]
param(
    [string] $Version,
    [switch] $SkipSite,
    [switch] $SkipGitHub,
    [switch] $WhatIf
)

$ErrorActionPreference = 'Stop'

$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Repo = (Resolve-Path (Join-Path $Here '..')).Path
$PublicRepository = 'Black-Rainbow-Labs/Unluminous'
$PrivateRepository = 'jasonmcaffee/unluminous'
$SiteRepo = $env:UNLUMINOUS_SITE_REPO
if (-not $SiteRepo) { $SiteRepo = 'C:/jason/dev/black-rainbow-labs-sites/sites/unluminous' }
$ParentSiteRepo = $env:BLACK_RAINBOW_LABS_REPO
if (-not $ParentSiteRepo) { $ParentSiteRepo = 'C:/jason/dev/black-rainbow-labs-sites/sites/blackrainbowlabs' }
$Origin = 'https://unluminous.com'

function Write-Step([string] $Message) {
    Write-Host ''
    Write-Host "==> $Message" -ForegroundColor Cyan
}

. (Join-Path $Here 'release-steps.ps1')

<#
.SYNOPSIS
  The versions on the nightly channel, newest first: the site's own record when this machine has it, and
  what unluminous.com serves otherwise.
#>
function Get-NightlyVersions {
    $record = Join-Path $SiteRepo 'releases/channels.json'
    if (Test-Path -LiteralPath $record) {
        return @((Get-Content -Raw -LiteralPath $record | ConvertFrom-Json).nightly | ForEach-Object { $_.version })
    }
    try {
        $served = Invoke-RestMethod -Uri "$Origin/releases/nightly.json" -TimeoutSec 30 -Headers @{ 'cache-control' = 'no-cache' }
    } catch {
        throw "There is no $record on this machine and $Origin/releases/nightly.json did not answer with a manifest ($($_.Exception.Response.StatusCode)). Run this on the machine that releases."
    }
    return @($served.versions | ForEach-Object { $_.version })
}

Write-Step 'Finding the version'
$nightly = @(Get-NightlyVersions)
if (-not $nightly.Count) { throw 'The nightly channel lists no versions, so there is nothing to promote.' }
if (-not $Version) { $Version = $nightly[0] }
$Version = $Version.Trim().TrimStart('v')
if ($nightly -notcontains $Version) {
    throw "Unluminous $Version is not on the nightly channel ($($nightly[0..([Math]::Min(4, $nightly.Count - 1))] -join ', ')...). Only a version that was released as a nightly can be promoted."
}
Write-Host "Promoting Unluminous $Version from nightly to stable."

if ($WhatIf) {
    Write-Host 'Would run, in order:'
    if (-not $SkipSite) {
        Write-Host "  pwsh $SiteRepo/scripts/publish.ps1 -Version $Version -Channel stable"
        Write-Host "  pwsh $ParentSiteRepo/scripts/publish.ps1"
    }
    if (-not $SkipGitHub) {
        foreach ($repository in @($PrivateRepository, $PublicRepository)) {
            Write-Host "  gh release edit v$Version --repo $repository --prerelease=false --latest"
        }
    }
    return
}

if (-not $SkipSite) {
    Write-Step 'Publishing unluminous.com on the stable channel'
    Invoke-WithRetry 'Publishing unluminous.com' 2 {
        & pwsh -NoProfile -File (Join-Path $SiteRepo 'scripts/publish.ps1') -Version $Version -Channel stable
    }
    Write-Step 'Publishing blackrainbowlabs.com'
    Invoke-WithRetry 'Publishing blackrainbowlabs.com' 2 {
        & pwsh -NoProfile -File (Join-Path $ParentSiteRepo 'scripts/publish.ps1')
    }
}

if (-not $SkipGitHub) {
    $gh = Get-GitHubCli
    $env:GH_TOKEN = Get-GitHubToken
    foreach ($repository in @($PrivateRepository, $PublicRepository)) {
        Write-Step "Marking v$Version the latest release on $repository"
        Invoke-WithRetry "The GitHub release v$Version on $repository" 3 {
            & $gh release edit "v$Version" --repo $repository --prerelease=false --latest
        }
    }
}

# What each destination serves, asked rather than assumed, which is `tools/release.ps1`'s report.
Write-Step "What stable $Version reached"
$missed = @()
function Test-Promoted([string] $Name, [scriptblock] $Check) {
    $why = try { & $Check } catch { $_.Exception.Message }
    Write-Host ("  {0,-28} {1,-10} {2}" -f $Name, $(if ($why) { 'NOT THERE' } else { 'ok' }), $why) -ForegroundColor $(if ($why) { 'Red' } else { 'Green' })
    if ($why) { $script:missed += "$Name - $why" }
}
if (-not $SkipSite) {
    foreach ($file in @('stable.json', 'latest.json')) {
        Test-Promoted "unluminous.com/releases/$file" {
            $served = Invoke-RestMethod -Uri "$Origin/releases/$file" -TimeoutSec 30 -Headers @{ 'cache-control' = 'no-cache' }
            if ($served.version -ne $Version) { return "it says $($served.version)" }
            $null
        }
    }
    Test-Promoted 'unluminous.com, page' {
        $page = (Invoke-WebRequest -Uri "$Origin/" -UseBasicParsing -TimeoutSec 30).Content
        if ($page -notlike "*UnluminousSetup-$Version-x64.exe*") { return "the page does not link the $Version Windows installer" }
        $null
    }
}
if (-not $SkipGitHub) {
    foreach ($repository in @($PrivateRepository, $PublicRepository)) {
        Test-Promoted "GitHub latest ($repository)" {
            $latest = (& $gh release view --repo $repository --json tagName --jq .tagName).Trim()
            if ($latest -ne "v$Version") { return "the latest release is $latest" }
            $null
        }
    }
}

if ($missed.Count) {
    Write-Host ''
    Write-Host "Unluminous $Version did not reach $($missed.Count) destination(s). Fix them and run this again, which is harmless." -ForegroundColor Red
    exit 1
}
Write-Host ''
Write-Host "Unluminous $Version is the stable version." -ForegroundColor Green
