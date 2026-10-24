# Write the colour schemes and font schemes of Excel's Page Layout, Colors and Fonts galleries as the text
# files `src/themes/color-schemes.txt` and `src/themes/font-schemes.txt` that the engine ships.
#
#   pwsh tools/excel-oracle/export-theme-schemes.ps1
#
# A colour scheme line is `Name|` followed by twelve colours separated by commas, each `RRGGBB` or
# `sys:windowText:RRGGBB` for a system colour. A font scheme line is `Name|major|minor|major panose|minor panose`.
param(
    [string]$Source = 'C:\Program Files\Microsoft Office\root\Document Themes 16',
    [string]$Out = (Join-Path $PSScriptRoot '..\..\crates\unluminous-sheet\src\themes')
)
$ErrorActionPreference = 'Stop'
$ns = @{ a = 'http://schemas.openxmlformats.org/drawingml/2006/main' }
$slots = 'dk1', 'lt1', 'dk2', 'lt2', 'accent1', 'accent2', 'accent3', 'accent4', 'accent5', 'accent6', 'hlink', 'folHlink'
$lines = @()
foreach ($f in Get-ChildItem (Join-Path $Source 'Theme Colors') -Filter '*.xml') {
    [xml]$x = Get-Content -Raw -Encoding UTF8 $f.FullName
    $scheme = $x.DocumentElement
    $parts = foreach ($s in $slots) {
        $node = $scheme.$s.FirstChild
        if ($node.LocalName -eq 'sysClr') { 'sys:{0}:{1}' -f $node.val, $node.lastClr } else { $node.val }
    }
    $lines += ('{0}|{1}' -f $scheme.name, ($parts -join ','))
}
$utf8 = New-Object Text.UTF8Encoding($false)
[IO.File]::WriteAllText((Join-Path $Out 'color-schemes.txt'), (($lines -join "`n") + "`n"), $utf8)
$fonts = @()
foreach ($f in Get-ChildItem (Join-Path $Source 'Theme Fonts') -Filter '*.xml') {
    [xml]$x = Get-Content -Raw -Encoding UTF8 $f.FullName
    $scheme = $x.DocumentElement
    $fonts += ('{0}|{1}|{2}|{3}|{4}' -f $scheme.name, $scheme.majorFont.latin.typeface, $scheme.minorFont.latin.typeface, $scheme.majorFont.latin.panose, $scheme.minorFont.latin.panose)
}
[IO.File]::WriteAllText((Join-Path $Out 'font-schemes.txt'), (($fonts -join "`n") + "`n"), $utf8)
"{0} colour schemes, {1} font schemes" -f $lines.Count, $fonts.Count
