# Make the fixtures tests/protection.rs compares the engine's protection with, each done by Excel.
#
#   pwsh tools/excel-oracle/protect-fixtures.ps1 [-Out crates/unluminous-sheet/tests/fixtures/protection]
#
# `protect-excel.xlsx`: four sheets protected as the tests say, with passwords `secret` (sheet 1), none
# (sheet 2, every action allowed), `sheetpw` and edit ranges `Open range` and `Locked range` with the
# password `rangepw` (sheet 3), and a workbook structure locked with `bpw`.
# `edit-ranges.tsv`: what Excel does to a cell inside an edit range, with and without the range's password.
# `structure.tsv`: what Excel does to the sheets of a workbook whose structure is locked.
param([string]$Out = (Join-Path $PSScriptRoot '..\..\crates\unluminous-sheet\tests\fixtures\protection'))
. "$PSScriptRoot\package-lib.ps1"
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$x = Start-Xl
$tab = [string][char]9
function Try-It([string]$Label, [scriptblock]$Do) {
    try { & $Do | Out-Null; return "$Label${tab}ok" } catch { return "$Label${tab}" + ($_.Exception.Message -replace "`r|`n", ' ') }
}
try {
    $b = $x.Workbooks.Add()
    while ($b.Worksheets.Count -lt 4) { $b.Worksheets.Add([Type]::Missing, $b.Worksheets.Item($b.Worksheets.Count)) | Out-Null }
    $s1 = $b.Worksheets.Item(1); $s2 = $b.Worksheets.Item(2); $s3 = $b.Worksheets.Item(3)
    $s1.Range('A1').Value2 = 1
    $s1.Range('B1').Locked = $false
    $s1.Protect('secret')
    $s2.Protect('', $true, $true, $true, $false, $true, $true, $true, $true, $true, $true, $true, $true, $true, $true, $true)
    $s3.Range('A1:B2').Value2 = 5
    $null = $s3.Protection.AllowEditRanges.Add('Open range', $s3.Range('A1:A2'))
    $null = $s3.Protection.AllowEditRanges.Add('Locked range', $s3.Range('B1:B2'), 'rangepw')
    $s3.Protect('sheetpw')
    $lines = @()
    $lines += Try-It 'edit a cell in the range with no password' { $s3.Range('A1').Value2 = 9 }
    $lines += Try-It 'edit a cell in the range with a password' { $s3.Range('B1').Value2 = 9 }
    $lines += Try-It 'edit a locked cell outside the ranges' { $s3.Range('C1').Value2 = 9 }
    $lines += Try-It 'give the range the wrong password' { $s3.Protection.AllowEditRanges.Item(2).Unprotect('nope') }
    $lines += Try-It 'give the range its password' { $s3.Protection.AllowEditRanges.Item(2).Unprotect('rangepw') }
    $lines += Try-It 'edit the cell in the range once its password was given' { $s3.Range('B1').Value2 = 9 }
    $lines | Set-Content -Path (Join-Path $Out 'edit-ranges.tsv')
    # Put the ranges back as they were before saving the fixture.
    $s3.Range('A1').Value2 = 5; $s3.Range('B1').Value2 = 5
    $s3.Unprotect('sheetpw'); $s3.Protect('sheetpw')
    $b.Protect('bpw', $true, $false)
    $b.SaveAs((Join-Path $Out 'protect-excel.xlsx'), 51)
    $lines = @()
    $s = $b.Worksheets.Item(4)
    $lines += Try-It 'add a sheet' { $b.Worksheets.Add() }
    $lines += Try-It 'rename a sheet' { $s.Name = 'Renamed' }
    $lines += Try-It 'delete a sheet' { $s.Delete() }
    $lines += Try-It 'move a sheet' { $s.Move($b.Worksheets.Item(1)) }
    $lines += Try-It 'copy a sheet' { $s.Copy($b.Worksheets.Item(1)) }
    $lines += Try-It 'hide a sheet' { $s.Visible = 0 }
    $lines += Try-It 'change a tab colour' { $s.Tab.Color = 255 }
    $lines += Try-It 'edit a cell' { $s.Range('A1').Value2 = 3 }
    $lines += Try-It 'unprotect with the wrong password' { $b.Unprotect('x') }
    $lines += Try-It 'unprotect with the password' { $b.Unprotect('bpw') }
    $lines += Try-It 'add a sheet once unprotected' { $b.Worksheets.Add() }
    $lines | Set-Content -Path (Join-Path $Out 'structure.tsv')
    $b.Close($false)
} finally { $x.Quit() }
