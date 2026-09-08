# Measures the retained resources of a release build across the fixed layout-memory corpus.
#
# Two things here are not incidental, and both were measured rather than assumed.
#
# It runs on a **corpus folder of its own**, copied out of the checkout, rather than on the checkout.
# A project Unluminous has been used in remembers what was open in it, and `.unluminous/plugin-tabs.txt`
# on this one names the Database plugin's tab - so the editing area was drawing that plugin, no file
# was ever laid out, and opening the 538 KB reference file moved the working set by 5.9 MB instead of
# the 58 MB `tasks/task-1813-performance-review-tdd.md` recorded. The corpus folder has no saved
# state, so the editor is what the editing area shows and the layout is really built.
#
# And every state is measured against a **settled** clean project rather than against the first
# reading after the window answers. A window that has only just become ready is still growing -
# fonts, plugins and the project walk are all still landing - so a baseline taken there reads low and
# every increase measured from it reads small. `Wait-UntilSettled` is what makes two builds
# comparable.
param(
  [string]$Exe = "$PSScriptRoot\..\target\release\unluminous.exe",
  [string]$Cli = "$PSScriptRoot\..\target\release\unluminous-cli.exe",
  [string]$Source = "$PSScriptRoot\..",
  [string]$Corpus = "$PSScriptRoot\..\_agent_output\layout-memory\corpus",
  [string]$Out = "$PSScriptRoot\..\_agent_output\layout-memory\release-resources.json",
  [int]$Cycles = 2
)

$ErrorActionPreference = 'Stop'

# The fixed corpus. The first entry is the 538 KB reference file the large-file thresholds are about.
$sources = @(
  'crates/unluminous-app/src/app/mod.rs',
  'crates/unluminous-app/src/components/file_tabs.rs',
  'crates/unluminous-app/src/components/explorer.rs',
  'crates/unluminous-app/src/services/plugins.rs',
  'crates/unluminous-core/src/document.rs',
  'crates/unluminous-core/src/layout.rs',
  'crates/unluminous-chat/src/lib.rs',
  'crates/unluminous-dap/src/lib.rs',
  'crates/unluminous-db/src/lib.rs',
  'crates/unluminous-git/src/lib.rs'
)

# Copies the corpus into a project of its own, so no remembered workspace decides what is drawn.
function Build-Corpus {
  if (Test-Path -LiteralPath $Corpus) {
    Remove-Item -LiteralPath $Corpus -Recurse -Force -Confirm:$false
  }
  [void](New-Item -ItemType Directory -Path $Corpus -Force)
  $names = @()
  $index = 0
  # `$relative` rather than `$source`: PowerShell variable names are case-insensitive, so a loop
  # variable called `$source` is the `$Source` parameter and the second path built is a doubled one.
  foreach ($relative in $sources) {
    $from = Join-Path $Source $relative
    # Flattened, numbered and kept in order, because two of the ten are called `lib.rs`.
    $name = '{0:d2}-{1}' -f $index, (($relative -replace '/', '-') -replace '^crates-', '')
    Copy-Item -LiteralPath $from -Destination (Join-Path $Corpus $name) -Force
    $names += $name
    $index++
  }
  return $names
}

$files = Build-Corpus
$rows = [System.Collections.Generic.List[object]]::new()

# Waits until the working set stops moving, so a state is measured settled rather than mid-growth.
function Wait-UntilSettled([int]$ProcessId) {
  $previous = -1
  $steady = 0
  $watch = [Diagnostics.Stopwatch]::StartNew()
  while ($watch.ElapsedMilliseconds -lt 30000) {
    Start-Sleep -Milliseconds 700
    $working = (Get-Process -Id $ProcessId).WorkingSet64 / 1MB
    if ($previous -ge 0 -and [math]::Abs($working - $previous) -lt 1.0) { $steady++ } else { $steady = 0 }
    $previous = $working
    if ($steady -ge 3) { return }
  }
}

# Captures the process counters that describe retained application resources.
function Measure-State([string]$Name, [int]$ProcessId) {
  Wait-UntilSettled $ProcessId
  $process = Get-Process -Id $ProcessId
  $row = [pscustomobject]@{
    state = $Name
    working_mb = [math]::Round($process.WorkingSet64 / 1MB, 1)
    private_mb = [math]::Round($process.PrivateMemorySize64 / 1MB, 1)
    handles = $process.HandleCount
    threads = $process.Threads.Count
  }
  $rows.Add($row)
  return $row
}

# Waits until the new window's control interface can answer rather than guessing a launch delay.
function Wait-UntilReady([int]$ProcessId) {
  $watch = [Diagnostics.Stopwatch]::StartNew()
  do {
    & $Cli status --instance $ProcessId --timeout 500 *> $null
    if ($LASTEXITCODE -eq 0) { return }
    Start-Sleep -Milliseconds 20
  } while ($watch.ElapsedMilliseconds -lt 60000)
  throw 'Unluminous control interface did not become ready within 60 seconds.'
}

function Open-Tab([int]$ProcessId, [string]$Path) {
  & $Cli tab open $Path --permanent --instance $ProcessId --timeout 20000 *> $null
}

function Close-Tabs([int]$ProcessId, [int]$Count) {
  1..$Count | ForEach-Object { & $Cli tab close --instance $ProcessId --timeout 20000 *> $null }
}

# Proves a file really was laid out, rather than opened behind a plugin tab that was drawing instead.
function Assert-LaidOut([int]$ProcessId, [string]$What) {
  $status = & $Cli editor status --instance $ProcessId --timeout 5000
  if ($LASTEXITCODE -ne 0 -or "$status" -notmatch 'lines') {
    throw "$What is not showing a file in the editing area, so nothing was laid out: $status"
  }
}

$process = Start-Process -FilePath $Exe -ArgumentList @($Corpus) -PassThru
try {
  Wait-UntilReady $process.Id
  $clean = Measure-State 'clean-project' $process.Id

  # The large-file thresholds, measured on their own so nothing else is in the increase.
  Open-Tab $process.Id $files[0]
  Assert-LaidOut $process.Id 'the large file'
  $large = Measure-State 'large-file' $process.Id
  Close-Tabs $process.Id 1
  [void](Measure-State 'large-file-closed' $process.Id)

  # The ten-tab threshold, measured from the same settled clean project.
  foreach ($file in $files) { Open-Tab $process.Id $file }
  Assert-LaidOut $process.Id 'the tenth tab'
  $ten = Measure-State 'ten-tabs' $process.Id

  # Switching back to the large tab must reuse its cached layout rather than rebuild it.
  $switch = [Diagnostics.Stopwatch]::StartNew()
  Open-Tab $process.Id $files[0]
  $switchMs = $switch.ElapsedMilliseconds
  [void](Measure-State 'cached-tab-switch' $process.Id)

  # Repeated open and close, to show the closed state plateaus rather than growing for ever.
  1..$Cycles | ForEach-Object {
    $cycle = $_
    Close-Tabs $process.Id $files.Count
    [void](Measure-State "cycle-$cycle-closed" $process.Id)
    foreach ($file in $files) { Open-Tab $process.Id $file }
    [void](Measure-State "cycle-$cycle-open" $process.Id)
  }

  $summary = [pscustomobject]@{
    exe = $Exe
    large_file_working_increase_mb = [math]::Round($large.working_mb - $clean.working_mb, 1)
    large_file_private_increase_mb = [math]::Round($large.private_mb - $clean.private_mb, 1)
    ten_tab_working_increase_mb = [math]::Round($ten.working_mb - $clean.working_mb, 1)
    ten_tab_private_increase_mb = [math]::Round($ten.private_mb - $clean.private_mb, 1)
    cached_tab_switch_ms = $switchMs
  }

  $parent = Split-Path -Parent $Out
  [void](New-Item -ItemType Directory -Path $parent -Force)
  [pscustomobject]@{ summary = $summary; states = $rows } | ConvertTo-Json -Depth 5 |
    Set-Content -LiteralPath $Out -Encoding utf8
  $rows | Format-Table -AutoSize
  $summary | Format-List
} finally {
  Stop-Process -Id $process.Id -Force -Confirm:$false -ErrorAction SilentlyContinue
}
