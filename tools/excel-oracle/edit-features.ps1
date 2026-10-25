# Perform the edits of edits.txt in Excel through its object model and save each result, then dump what Excel sees in
# every result (feature properties and recalculated cells) into <OutDir>. An edit Excel refuses is listed in
# <OutDir>/refusals.txt as name|Excel's message, and nothing is saved for it. The saved workbooks go to <WorkDir>.
# A run with -Only keeps the refusals of the edits it did not run.
#
#   pwsh tools/excel-oracle/edit-features.ps1 [-Only name1,name2]
param(
    [string]$Folder = (Join-Path $PSScriptRoot '../../crates/unluminous-sheet/tests/fixtures/features'),
    [string]$OutDir = (Join-Path $PSScriptRoot '../../crates/unluminous-sheet/tests/fixtures/features/expected-edits'),
    [string]$WorkDir = (Join-Path $PSScriptRoot '../../_agent_output/task-2360-fidelity/excel-edits'),
    [string[]]$Only = @()
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$Only = @($Only | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
New-Item -ItemType Directory -Force -Path $OutDir, $WorkDir | Out-Null
$Folder = (Resolve-Path -LiteralPath $Folder).Path; $OutDir = (Resolve-Path -LiteralPath $OutDir).Path; $WorkDir = (Resolve-Path -LiteralPath $WorkDir).Path
$m = [Type]::Missing
$cases = @(Get-Content -LiteralPath (Join-Path $Folder 'edits.txt') | Where-Object { $_ -and -not $_.StartsWith('#') } | ForEach-Object { , ($_ -split '\|') })
$sig = '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);'
Add-Type -MemberDefinition $sig -Name Win -Namespace Probe -ErrorAction SilentlyContinue
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false; $excel.Visible = $false; $excel.AutomationSecurity = 3
$excelPid = 0; [void][Probe.Win]::GetWindowThreadProcessId([IntPtr]$excel.Hwnd, [ref]$excelPid)
$refusals = New-Object System.Collections.Generic.List[string]

# Insert two whole rows at the top, one whole column at the left, delete one whole row, rename the sheet.
function Edit-Combo($book, [int]$index, [int]$row) {
    $ws = $book.Sheets.Item($index + 1)
    [void]$ws.Range('1:2').EntireRow.Insert()
    [void]$ws.Range('A:A').EntireColumn.Insert()
    [void]$ws.Range("${row}:${row}").EntireRow.Delete()
    $ws.Name = 'Renamed'
}

# Move the sheet at one position to another position, both counted from 0 over every sheet.
function Edit-Move($book, [int]$from, [int]$to) {
    $sheet = $book.Sheets.Item($from + 1)
    $target = $book.Sheets.Item($to + 1)
    if ($from -lt $to) { $sheet.Move($m, $target) } else { $sheet.Move($target) }
}

try {
    foreach ($case in $cases) {
        $name = $case[0]; $source = $case[1]; $op = $case[2]
        if ($Only.Count -gt 0 -and $Only -notcontains $name) { continue }
        $book = $null
        try {
            $book = $excel.Workbooks.Open((Join-Path $Folder $source), 0, $false)
            try { $excel.ActivePrinter = 'Microsoft Print to PDF on Ne00:' } catch { }
            switch ($op) {
                'combo' { Edit-Combo $book ([int]$case[3]) ([int]$case[4]) }
                'movesheet' { Edit-Move $book ([int]$case[3]) ([int]$case[4]) }
                'delsheet' { $book.Sheets.Item([int]$case[3] + 1).Delete() }
                default { throw "unknown operation $op" }
            }
            $extension = [IO.Path]::GetExtension($source)
            $book.SaveAs((Join-Path $WorkDir "$name$extension"), $(if ($extension -eq '.xlsm') { 52 } else { 51 }))
            Write-Host "edited $name"
        } catch {
            $refusals.Add("$name|" + ($_.Exception.Message.Trim() -replace '\s+', ' '))
            Write-Host "refused $name : $($_.Exception.Message)"
        } finally {
            if ($book) { try { $book.Close($false) } catch { } }
        }
    }
} finally {
    try { $excel.Quit() } catch { }
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
    Start-Sleep 2
    if ($excelPid -gt 0 -and (Get-Process -Id $excelPid -ErrorAction SilentlyContinue)) { Stop-Process -Id $excelPid -Force }
}
# A run over some edits keeps the refusals of the others and replaces the refusals of the ones it ran.
$refusalFile = Join-Path $OutDir 'refusals.txt'
$kept = @()
if ($Only.Count -gt 0 -and (Test-Path -LiteralPath $refusalFile)) { $kept = @(Get-Content -LiteralPath $refusalFile | Where-Object { $_ -and ($Only -notcontains ($_ -split '\|')[0]) }) }
[IO.File]::WriteAllText($refusalFile, ((($kept + $refusals) | Sort-Object) -join "`n") + "`n", (New-Object Text.UTF8Encoding($false)))
$wanted = @($cases | ForEach-Object { $_[0] } | Where-Object { $Only.Count -eq 0 -or $Only -contains $_ })
$names = @(Get-ChildItem -LiteralPath $WorkDir | Where-Object { $wanted -contains $_.BaseName } | ForEach-Object { $_.Name })
& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'dump-expected.ps1') -Folder $WorkDir -OutDir $OutDir -Names ($names -join ',')
