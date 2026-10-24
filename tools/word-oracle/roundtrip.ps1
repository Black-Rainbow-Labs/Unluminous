# Runs inspect.ps1 on every .docx in a folder and writes <name>.word.json beside each file.
# Usage: pwsh -NoProfile -File tools/word-oracle/roundtrip.ps1 -Folder <dir>
param([Parameter(Mandatory)][string]$Folder)

$dir = (Resolve-Path -LiteralPath $Folder).Path
foreach ($file in Get-ChildItem -LiteralPath $dir -Filter *.docx) {
    $out = Join-Path $dir ($file.BaseName + '.word.json')
    Write-Host "inspecting $($file.Name)"
    $proc = Start-Process pwsh -ArgumentList '-NoProfile', '-File', "$PSScriptRoot/inspect.ps1", '-Path', $file.FullName, '-Out', $out -PassThru -NoNewWindow
    if (-not $proc.WaitForExit(300000)) { Stop-Process -Id $proc.Id -Force; Write-Host "  timed out" }
}
