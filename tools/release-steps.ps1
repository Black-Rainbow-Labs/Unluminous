<#
.SYNOPSIS
  The steps of `tools/release.ps1` that let a release be started again: retries, the record of finished steps,
  reusing a build made from the same source, and a GitHub release that can be published twice.

.DESCRIPTION
  `task-2312`. Dot-sourced by `tools/release.ps1`, and by `tools/release-steps.test.ps1`, which runs them against
  a temporary folder and a stand-in for `gh`. They read `$Repo`, `$next`, `$gh`, `$macosBuild` and `$macosLog`
  from the script that loads them. `tools/promote.ps1` loads them for `Invoke-WithRetry` and the two GitHub
  helpers, which call `Write-Step` from the script that loads them.
#>

<#
.SYNOPSIS
  Runs a command again when it fails, waiting longer each time, and throws once it has failed every time.
.DESCRIPTION
  `task-2312`. The steps that reach the network (a push, a GitHub release, a site) failed a release outright on
  their first error, and a release that stopped after its tag had to be finished by hand. Every step this wraps
  can be run twice without harm.
.PARAMETER What
  What the command does, for the messages.
.PARAMETER Tries
  How many times to run it.
.PARAMETER Body
  The command. It reports failure through `$LASTEXITCODE` or by throwing.
#>
# Seconds to wait before the second try; the third waits twice as long. A test sets it to 0.
$RetryWaitSeconds = 10

function Invoke-WithRetry([string] $What, [int] $Tries, [scriptblock] $Body) {
    for ($try = 1; $try -le $Tries; $try++) {
        $failure = $null
        try {
            $global:LASTEXITCODE = 0
            & $Body
            if ($LASTEXITCODE -ne 0) { $failure = "exit code $LASTEXITCODE" }
        } catch {
            $failure = $_.Exception.Message
        }
        if (-not $failure) { return }
        if ($try -lt $Tries) {
            $wait = $RetryWaitSeconds * $try
            Write-Host "  $What failed ($failure). Trying again in $wait s, try $($try + 1) of $Tries." -ForegroundColor Yellow
            Start-Sleep -Seconds $wait
        } else {
            throw "$What failed $Tries times, the last with $failure."
        }
    }
}

<#
.SYNOPSIS
  Where the record of a release's finished steps is kept.
.PARAMETER Version
  The version the release is cutting.
#>
function Get-JournalPath([string] $Version) {
    return (Join-Path $Repo "_agent_output\release-state\$Version.json")
}

<#
.SYNOPSIS
  Reads the record of a release's finished steps, or answers $null when there is none.
.PARAMETER Version
  The version the release is cutting.
#>
function Read-Journal([string] $Version) {
    $path = Get-JournalPath $Version
    if (-not (Test-Path -LiteralPath $path)) { return $null }
    try { return (Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -AsHashtable) } catch { return $null }
}

<#
.SYNOPSIS
  Writes the record of this release's finished steps.
.DESCRIPTION
  `task-2312`. A release that stopped used to start again from the beginning: the suite, both builds and
  everything after them. The record says which source the builds were made from and which publishing steps are
  done, so the next start reuses an installer built from the same source and carries on after the last step
  that finished.
#>
function Save-Journal {
    $path = Get-JournalPath $script:Journal.version
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
    $script:Journal | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $path -Encoding utf8
}

<#
.SYNOPSIS
  Records that a step of this release is done.
.PARAMETER Step
  The step's name.
#>
function Add-JournalStep([string] $Step) {
    if ($script:Journal.steps -notcontains $Step) { $script:Journal.steps = @($script:Journal.steps) + $Step }
    Save-Journal
}

<#
.SYNOPSIS
  Answers whether a step of this release was done by an earlier start.
.PARAMETER Step
  The step's name.
#>
function Test-JournalStep([string] $Step) {
    return (@($script:Journal.steps) -contains $Step)
}

<#
.SYNOPSIS
  Answers a file's SHA-256, or $null when it does not exist.
.PARAMETER Path
  The file.
#>
function Get-Sha256([string] $Path) {
    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) { return $null }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
}

<#
.SYNOPSIS
  Answers a build an earlier start made from the same source, or $null when it has to be built again.
.PARAMETER Platform
  `windows` or `macos`.
#>
function Get-ReusedBuild([string] $Platform) {
    $built = $script:Journal.builds[$Platform]
    if (-not $built) { return $null }
    if ((Get-Sha256 $built.path) -ne $built.sha256) { return $null }
    return $built.path
}

<#
.SYNOPSIS
  Creates a GitHub release, or finishes one an earlier start began.
.DESCRIPTION
  `task-2312`. `gh release create` refuses a tag that already has a release, so a start that carried on after a
  failed upload stopped on the release it had made itself. A release that is there is given the assets it lacks.
.PARAMETER Repository
  The `owner/name` of the repository.
.PARAMETER Assets
  The files to attach.
.PARAMETER Body
  The release notes.
#>
function Publish-GitHubRelease([string] $Repository, [string[]] $Assets, [string] $Body) {
    $have = & $gh release view "v$next" --repo $Repository --json assets --jq '.assets[].name' 2>$null
    if ($LASTEXITCODE -eq 0) {
        $missing = @($Assets | Where-Object { @($have) -notcontains (Split-Path -Leaf $_) })
        if ($missing.Count -eq 0) {
            Write-Host "  v$next is already released on $Repository with every asset."
            $global:LASTEXITCODE = 0
            return
        }
        Write-Host "  v$next is already released on $Repository; attaching $($missing.Count) missing asset(s)."
        & $gh release upload "v$next" @missing --repo $Repository --clobber
        return
    }
    # A prerelease, because every release is a nightly until `tools/promote.ps1` makes it stable, and GitHub's
    # `releases/latest`, which an Unluminous asks when unluminous.com does not answer, never answers with one
    # (task-2328).
    & $gh release create "v$next" @Assets --repo $Repository --title "Unluminous $next" --notes $Body --prerelease
}

<#
.SYNOPSIS
  Answers whether a failed macOS build failed on something that may go away when it is run again.
.DESCRIPTION
  A file another process held, or the network on the way to Apple's notary service. A compile error is not one
  of these, and is reported at once.
.PARAMETER Log
  The build's log.
#>
function Test-TransientMacosFailure([string] $Log) {
    $text = (@(Get-Content -LiteralPath $Log -ErrorAction SilentlyContinue) + @(Get-Content -LiteralPath "$Log.err" -ErrorAction SilentlyContinue)) -join "`n"
    return ($text -match 'os error 32|being used by another process|timed out|connection (was )?(reset|closed|refused)|Could not resolve|error sending request|50[234] ')
}

<#
.SYNOPSIS
  Starts the macOS build in the background, its output going to the release's log.
#>
function Start-MacosBuild {
    $process = Invoke-WithoutCc {
        Start-Process -FilePath 'pwsh' -NoNewWindow -PassThru `
            -ArgumentList @('-NoProfile', '-File', "`"$macosBuild`"", '-Version', $next, '-Notarize') `
            -RedirectStandardOutput $macosLog -RedirectStandardError "$macosLog.err"
    }
    # Read now, so the exit code is still there to read after the process has gone.
    $null = $process.Handle
    $script:MacosChild = $process
    return $process
}

<#
.SYNOPSIS
  Prints the end of the macOS build's log, without cargo's lines about each crate.
#>
function Write-MacosLogTail {
    $lines = @(Get-Content -LiteralPath $macosLog -ErrorAction SilentlyContinue) + @(Get-Content -LiteralPath "$macosLog.err" -ErrorAction SilentlyContinue)
    $lines | Where-Object { $_ -notmatch '^\s+(Compiling|Checking|Fresh|Blocking)\b' } | Select-Object -Last 40 | ForEach-Object { Write-Host "  $_" }
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
