<#
.SYNOPSIS
  Start an Unluminous window and drive it **without ever taking the keyboard focus**, so somebody can
  carry on working on their own machine while it is being tested.

.DESCRIPTION
  The Windows sibling of `tools/drive-a-window.sh`, and it exists because the Windows half of that
  script's promise was missing. `task-1848` wrote the rule for macOS and answered the *launching* half
  of it with `open -g`. On Windows there was no answer, so every script that started a window activated
  one — and **activating a window that is on another virtual desktop switches the desktop with it**,
  which is `task-1914`'s report:

      we can't have the window take focus while testing ... right now im switched to a different
      desktop, but get switched to another desktop with unluminous open.

  Three things make the whole of it possible, and each is a thing rather than a habit:

    * `unluminous --background` opens the window without making it the foreground window. `winit` turns
      that into `SW_SHOWNOACTIVATE`, so the window appears where it was started and neither the focus
      nor the desktop moves.
    * `unluminous-cli input` clicks, types and drags by feeding the window the same events a mouse and
      a keyboard produce, down the control channel. Synthetic operating system input goes to whatever
      window is in *front*, which is what forced every earlier script to activate one; this does not.
    * `unluminous-cli window screenshot` photographs the window whether or not it is in front, whether
      or not it is covered, and whether or not the desktop being looked at is the one it is on. It never
      needed the focus, and it never did.

  `tasks/task-1914-testing-without-stealing-focus-tdd.md` is the design and what was measured.

.PARAMETER Folder
  The project to open.

.PARAMETER Shot
  A path to photograph the window into once it has opened. The window is left running.

.PARAMETER Pid
  Print the process id of a window this script started and exit.

.PARAMETER Stop
  Close the windows this script started and exit.

.EXAMPLE
  pwsh tools/drive-a-window.ps1 C:\jason\dev\unluminous
  pwsh tools/drive-a-window.ps1 C:\jason\dev\unluminous -Shot _agent_output/after.png
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)][string]$Folder,
    [string]$Shot,
    [switch]$WindowId,
    [switch]$Stop
)

$ErrorActionPreference = 'Stop'

$installed = Join-Path $env:LOCALAPPDATA 'Programs\Unluminous'
$binary = if ($env:UNLUMINOUS_APP) { Join-Path $env:UNLUMINOUS_APP 'unluminous.exe' } else { Join-Path $installed 'unluminous.exe' }
$cli = if ($env:UNLUMINOUS_APP) { Join-Path $env:UNLUMINOUS_APP 'unluminous-cli.exe' } else { Join-Path $installed 'unluminous-cli.exe' }
$marker = Join-Path $env:LOCALAPPDATA 'unluminous-driven-windows.txt'

if (-not (Test-Path $cli)) {
    Write-Error "No Unluminous at $installed. Build and install it first, or set UNLUMINOUS_APP."
}

# The ids this script started, filtered to the ones still running. A dead id in the file is ordinary:
# a window is closed by `quit`, by its own close button, or by the machine restarting.
function Get-DrivenWindows {
    if (-not (Test-Path $marker)) { return @() }
    Get-Content $marker | Where-Object { $_ } | ForEach-Object {
        $found = Get-Process -Id ([int]$_) -ErrorAction SilentlyContinue
        if ($found -and $found.ProcessName -eq 'unluminous') { [int]$_ }
    }
}

if ($WindowId) {
    Get-DrivenWindows | Select-Object -First 1
    return
}

if ($Stop) {
    foreach ($id in Get-DrivenWindows) {
        # Asked rather than killed, so the window writes down what it was holding — which is what
        # `on_exit` is for. A window that does not answer is left alone rather than forced.
        & $cli --instance $id quit | Out-Null
        Write-Output "closed $id"
    }
    Set-Content -Path $marker -Value ''
    return
}

if (-not $Folder) {
    Get-Help $PSCommandPath -Detailed
    return
}
if (-not (Test-Path -Path $Folder -PathType Container)) {
    Write-Error "$Folder is not a folder."
}
$Folder = (Resolve-Path $Folder).Path

# **`--background` is the whole point.** Without it the new window becomes the foreground window, and on
# Windows that drags the virtual desktop with it.
$started = Start-Process -FilePath $binary -ArgumentList '--background', $Folder -PassThru

# Waited for by **asking** rather than by sleeping a fixed time: a cold start is about a second on this
# machine and much longer on a busy one, and the control channel answering is the honest signal that the
# window is ready to be driven.
$ready = $false
for ($i = 0; $i -lt 60; $i++) {
    Start-Sleep -Milliseconds 500
    $answer = & $cli --instance $started.Id status 2>$null
    if ($LASTEXITCODE -eq 0 -and $answer) { $ready = $true; break }
}
if (-not $ready) {
    Write-Error "The window did not open, or its control channel never answered."
}

Add-Content -Path $marker -Value $started.Id
Write-Output $started.Id

if ($Shot) {
    # One frame is asked for and waited out by the command itself; nothing here touches the window.
    & $cli --instance $started.Id window screenshot $Shot | Write-Verbose
}
