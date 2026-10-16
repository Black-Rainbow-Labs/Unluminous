<#

.SYNOPSIS
    Measures what a release window holds at rest, in three projects, and writes one JSON row a case.

.DESCRIPTION
    `task-2279`'s memory hill climb. Each case starts a window with `--background` on a fresh settings folder
    and a warm index cache, waits for it to settle, and records its working set, its private bytes, its
    private working set (Task Manager's Memory column), what `tools/memory-map.ps1` says the memory is made
    of, and, in a build made with `diagnostic-allocations`, the Rust heap's live bytes from the frame trace.

    | Case | What is open |
    |---|---|
    | empty | a folder with nothing in it |
    | file | a folder holding one 231 KB Rust file, which is open |
    | project | a copy of this repository, with that same file open |

    A case is opened twice and measured the second time, so the index is loaded rather than built, which is
    what a person reopening a project sees.

.EXAMPLE
    pwsh tools/perf-memory.ps1 -Bin _agent_output/task-2279-perf/bins/final -Label final -Out _agent_output/task-2279-perf/memory
#>
param(
    [Parameter(Mandatory)] [string] $Bin,
    [Parameter(Mandatory)] [string] $Label,
    [Parameter(Mandatory)] [string] $Out,
    [string] $Corpus = '_agent_output/task-2279-perf/bench/runs/c1-r3/corpus',
    [int] $Round = 1,
    [int] $Settle = 40,
    [string] $Settings = '',
    [int] $Width = 1600,
    [int] $Height = 1000,
    [string[]] $Cases = @('empty', 'file', 'project')
)
$ErrorActionPreference = 'Stop'
$bin = (Resolve-Path $Bin).Path
$cli = Join-Path $bin 'unluminous-cli.exe'
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$work = Join-Path $Out "work-$Label-r$Round"
if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force -Confirm:$false }
New-Item -ItemType Directory -Force (Join-Path $work 'appdata\Unluminous'), (Join-Path $work 'empty'), (Join-Path $work 'file\src'), (Join-Path $work 'small\src') | Out-Null
if ($Settings) { Copy-Item -LiteralPath $Settings -Destination (Join-Path $work 'appdata\Unluminous\settings.conf') }
$source = Join-Path (Resolve-Path $Corpus).Path 'crates\unluminous-app\src\app\mod.rs'
Copy-Item -LiteralPath $source -Destination (Join-Path $work 'file\src\mod.rs')
Set-Content -LiteralPath (Join-Path $work 'small\src\main.rs') -Value ((Get-Content -LiteralPath $source -TotalCount 60) -join "`n")
$folders = @{
    empty = @{ Folder = (Join-Path $work 'empty'); File = $null }
    small = @{ Folder = (Join-Path $work 'small'); File = 'src/main.rs' }
    file = @{ Folder = (Join-Path $work 'file'); File = 'src/mod.rs' }
    project = @{ Folder = (Resolve-Path $Corpus).Path; File = 'crates/unluminous-app/src/app/mod.rs' }
    # The project after an agent has asked one plain English question through the window, measured once
    # the questions have stopped for longer than the index keeps their search index (`task-2279`).
    asked = @{ Folder = (Resolve-Path $Corpus).Path; File = 'crates/unluminous-app/src/app/mod.rs'; Ask = 'where is the layout of a paragraph worked out again after an edit' }
}
$env:APPDATA = Join-Path $work 'appdata'
$env:ATRIUS_INDEX_CACHE = Join-Path $Out 'index-cache'
$env:UNLUMINOUS_TELEMETRY = 'off'

foreach ($case in $Cases) {
    $spec = $folders[$case]
    foreach ($pass in 1, 2) {
        $trace = Join-Path $work "$case-$pass-trace.txt"
        $env:UNLUMINOUS_FRAME_TRACE = $trace
        $p = Start-Process -FilePath (Join-Path $bin 'unluminous.exe') -ArgumentList "`"$($spec.Folder)`"", '--background' -PassThru
        for ($i = 0; $i -lt 300; $i++) { & $cli status --instance $p.Id --timeout 500 *> $null; if ($LASTEXITCODE -eq 0) { break }; Start-Sleep -Milliseconds 200 }
        & $cli window size --width $Width --height $Height --instance $p.Id *> $null
        if ($spec.File) { & $cli tab open $spec.File --permanent --instance $p.Id *> $null }
        if ($pass -eq 1) {
            # The first open builds the index; it is only worth reopening once the passages are written too,
            # or the second open builds them again and is measured building (`task-2279`).
            for ($i = 0; $i -lt 240; $i++) {
                $ready = (& $cli search status --root $spec.Folder --json --instance $p.Id 2>$null | ConvertFrom-Json).result.passagesReady
                if ($ready) { break }
                Start-Sleep -Milliseconds 500
            }
            Start-Sleep -Seconds 3
        } else {
            if ($spec.Ask) { & $cli search find $spec.Ask --root $spec.Folder --instance $p.Id *> $null }
            Start-Sleep -Seconds $Settle
        }
        if ($pass -eq 2) {
            $map = pwsh -NoProfile -File (Join-Path $PSScriptRoot 'memory-map.ps1') -ProcessId $p.Id -Json -Top 8 | ConvertFrom-Json
            $heap = (Get-Content -LiteralPath $trace -ErrorAction SilentlyContinue | Select-String 'heap-live-mb ([0-9.]+)' | Select-Object -Last 1)
            $heapMb = if ($heap) { [double]$heap.Matches[0].Groups[1].Value } else { $null }
            $row = [ordered]@{
                prompt_id = $case; variant = $Label; rep = $Round; model = 'n/a'; tags = @('memory', $case)
                ws_mb = $map.working_set_mb; priv_mb = $map.private_bytes_mb; private_ws_mb = $map.private_working_set_mb
                heap_live_mb = $heapMb; kinds = $map.kinds; modules = $map.modules
            }
            ($row | ConvertTo-Json -Compress -Depth 6) | Add-Content -LiteralPath (Join-Path $Out 'results.jsonl')
            '{0} r{1} {2}: private ws {3} MB, ws {4} MB, private {5} MB, heap {6} MB, private committed {7} MB resident {8} MB, image resident {9} MB' -f $Label, $Round, $case, $map.private_working_set_mb, $map.working_set_mb, $map.private_bytes_mb, $heapMb, $map.kinds.private.committed_mb, $map.kinds.private.resident_mb, $map.kinds.image.resident_mb
        }
        & $cli quit --instance $p.Id *> $null
        if (-not $p.WaitForExit(20000)) { Stop-Process -Id $p.Id -Force -Confirm:$false }
        Start-Sleep -Seconds 2
    }
}
