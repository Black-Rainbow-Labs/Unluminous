# Write Excel's inspect dump next to each .xls and .xlsb fixture, as <file>.json.
#
# The dump is what tools/excel-oracle/inspect.ps1 reports for the file. crates/unluminous-sheet/tests/legacy.rs
# compares the readers' cells with it, so the tests need no Excel. A file that makes Excel stop answering is
# ended after -Seconds and gets no dump.
#
#   pwsh tools/excel-oracle/inspect-legacy.ps1 -Folder crates/unluminous-sheet/tests/fixtures/legacy [-Only 05-condfmt.xls]
param(
    [Parameter(Mandatory = $true)][string]$Folder,
    [string[]]$Only,
    [int]$Seconds = 120
)
$ErrorActionPreference = 'Stop'
$inspect = Join-Path $PSScriptRoot 'inspect.ps1'
$files = Get-ChildItem -LiteralPath $Folder -File | Where-Object { $_.Extension -in '.xls', '.xlsb' }
if ($Only) { $files = $files | Where-Object { $Only -contains $_.Name } }
foreach ($file in $files) {
    $target = $file.FullName + '.json'
    $json = & pwsh -NoProfile -File $inspect -Path $file.FullName -Seconds $Seconds 2>$null
    if ($LASTEXITCODE -eq 0 -and $json) {
        [IO.File]::WriteAllText($target, ($json -join "`n"), (New-Object Text.UTF8Encoding $false))
        Write-Output "dumped $($file.Name)"
    } else {
        if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target }
        Write-Output "no dump for $($file.Name)"
    }
}
