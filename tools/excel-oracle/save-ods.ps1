# Have Excel save each workbook in a folder as an OpenDocument spreadsheet (.ods), the way Excel's own
# Save As does, so the .ods reader is tested against files Excel writes. The source workbooks are copied
# beside the .ods files so a test can compare the two without Excel.
#
# Each workbook is saved by a PowerShell process and an Excel of its own, and a file that takes longer than
# -Seconds is abandoned by stopping that one Excel by its process id, so one file Excel cannot save does not
# stop the rest.
#
#   pwsh tools/excel-oracle/save-ods.ps1 -From <folder of .xlsx files> [-To crates/unluminous-sheet/tests/fixtures/ods]
param(
    [string]$From,
    [string]$To = 'crates/unluminous-sheet/tests/fixtures/ods',
    [int]$Seconds = 90,
    [string]$One,
    [string]$Out
)
$ErrorActionPreference = 'Stop'

# Save one workbook as .ods with an Excel of its own.
function Save-One([string]$Path, [string]$Target) {
    $excel = New-Object -ComObject Excel.Application
    $excel.DisplayAlerts = $false
    $excel.Visible = $false
    $excel.AutomationSecurity = 3  # never run macros
    try {
        $book = $excel.Workbooks.Open($Path, 0, $true)
        $book.SaveAs($Target, 60)  # xlOpenDocumentSpreadsheet
        $book.Close($false)
    } finally {
        $excel.Quit()
        [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
    }
}

if ($One) {
    Save-One $One $Out
    return
}

New-Item -ItemType Directory -Force -Path $To | Out-Null
$source = (Resolve-Path -LiteralPath $From).Path
$dest = (Resolve-Path -LiteralPath $To).Path
foreach ($file in Get-ChildItem -LiteralPath $source | Where-Object { $_.Extension -in '.xlsx', '.xlsm' }) {
    $name = [IO.Path]::GetFileNameWithoutExtension($file.Name)
    $before = @(Get-Process excel -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
    $child = Start-Process pwsh -PassThru -WindowStyle Hidden -ArgumentList @('-NoProfile', '-File', $PSCommandPath, '-One', $file.FullName, '-Out', (Join-Path $dest "$name.ods"))
    if (-not $child.WaitForExit($Seconds * 1000)) {
        Stop-Process -Id $child.Id -Force
        foreach ($p in Get-Process excel -ErrorAction SilentlyContinue) { if ($before -notcontains $p.Id) { Stop-Process -Id $p.Id -Force } }
        Write-Host "abandoned $name"
        continue
    }
    if (-not (Test-Path -LiteralPath (Join-Path $dest "$name.ods"))) {
        Write-Host "Excel could not save $name"
        continue
    }
    Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $dest $file.Name) -Force
    Write-Host "saved $name.ods"
}
