# Write what Excel sees in every workbook of a folder, so tests can compare against it without Excel:
#   <name>.feat.txt   the feature properties from inspect-features.ps1 (one Excel for all the files)
#   <name>.cells.json  every cell after a full recalculation, from inspect.ps1 -Recalculate (one Excel a file)
#
#   pwsh tools/excel-oracle/dump-expected.ps1 -Folder <workbooks> -OutDir <dumps> [-Names a.xlsx,b.xlsx]
param(
    [Parameter(Mandatory = $true)][string]$Folder,
    [Parameter(Mandatory = $true)][string]$OutDir,
    [string[]]$Names = @()
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$Names = @($Names | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path -LiteralPath $OutDir).Path
$files = @(Get-ChildItem -LiteralPath $Folder | Where-Object { $_.Extension -in '.xlsx', '.xlsm' } | Where-Object { $Names.Count -eq 0 -or $Names -contains $_.Name } | Sort-Object Name)
$here = $PSScriptRoot
& pwsh -NoProfile -File (Join-Path $here 'inspect-features.ps1') -Paths ($files.FullName -join ',') -OutDir $OutDir | Out-Null
foreach ($file in $files) {
    $json = & pwsh -NoProfile -File (Join-Path $here 'inspect.ps1') -Path $file.FullName -Recalculate
    $text = ($json -join "`n").Trim().TrimStart([char]0xFEFF)
    [IO.File]::WriteAllText((Join-Path $OutDir ($file.BaseName + '.cells.json')), $text, (New-Object Text.UTF8Encoding($false)))
    Write-Host "cells $($file.Name)"
}