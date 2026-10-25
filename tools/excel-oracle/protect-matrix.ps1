# Try 43 operations on a protected sheet with each of the allow flags of Protect Sheet on its own, in Excel, and
# write one line per attempt: the operation, the flag allowed, and `ok` or Excel's message.
#
#   pwsh tools/excel-oracle/protect-matrix.ps1 -Out crates/unluminous-sheet/tests/fixtures/protection/protect-matrix.tsv
#
# tests/protection.rs holds the engine to the same table. The sheet it builds: A1:A3 hold 1 and are
# locked, B1:E8 hold 2 and are unlocked, row 5 is unlocked all the way across and row 6 is locked all the
# way across.
param([Parameter(Mandatory = $true)][string]$Out)
. "$PSScriptRoot\package-lib.ps1"
$x = Start-Xl
$flags = 'none','formatCells','formatColumns','formatRows','insertColumns','insertRows','insertHyperlinks','deleteColumns','deleteRows','sort','autoFilter','pivotTables','selectLocked','objectsOff','scenariosOff'
# Protect args: pw, DrawingObjects, Contents, Scenarios, UIonly, FmtCells, FmtCols, FmtRows, InsCols, InsRows, InsHyper, DelCols, DelRows, Sort, Filter, Pivot
function Protect-With($s, [string]$flag) {
  $a = @{ obj = $true; sc = $true; fc = $false; fcol = $false; fr = $false; ic = $false; ir = $false; ih = $false; dc = $false; dr = $false; so = $false; af = $false; pv = $false }
  switch ($flag) {
    'formatCells' { $a.fc = $true } 'formatColumns' { $a.fcol = $true } 'formatRows' { $a.fr = $true }
    'insertColumns' { $a.ic = $true } 'insertRows' { $a.ir = $true } 'insertHyperlinks' { $a.ih = $true }
    'deleteColumns' { $a.dc = $true } 'deleteRows' { $a.dr = $true } 'sort' { $a.so = $true } 'autoFilter' { $a.af = $true } 'pivotTables' { $a.pv = $true }
    'objectsOff' { $a.obj = $false } 'scenariosOff' { $a.sc = $false }
  }
  $s.Protect('', $a.obj, $true, $a.sc, $false, $a.fc, $a.fcol, $a.fr, $a.ic, $a.ir, $a.ih, $a.dc, $a.dr, $a.so, $a.af, $a.pv)
}
$ops = [ordered]@{
  'set locked A1'         = { param($s) $s.Range('A1').Value2 = 9 }
  'set unlocked B1'       = { param($s) $s.Range('B1').Value2 = 9 }
  'clear unlocked B1'     = { param($s) $s.Range('B1').ClearContents() | Out-Null }
  'clear locked A1'       = { param($s) $s.Range('A1').ClearContents() | Out-Null }
  'bold unlocked B1'      = { param($s) $s.Range('B1').Font.Bold = $true }
  'bold locked A1'        = { param($s) $s.Range('A1').Font.Bold = $true }
  'numfmt unlocked B1'    = { param($s) $s.Range('B1').NumberFormat = '0.00' }
  'fill unlocked B1'      = { param($s) $s.Range('B1').Interior.Color = 255 }
  'merge unlocked B1:C1'  = { param($s) $s.Range('B1:C1').Merge() }
  'colwidth B'            = { param($s) $s.Columns.Item(2).ColumnWidth = 20 }
  'col hide B'            = { param($s) $s.Columns.Item(2).Hidden = $true }
  'rowheight 1'           = { param($s) $s.Rows.Item(1).RowHeight = 30 }
  'row hide 1'            = { param($s) $s.Rows.Item(1).Hidden = $true }
  'insert row 2'          = { param($s) $s.Rows.Item(2).Insert() | Out-Null }
  'insert col B'          = { param($s) $s.Columns.Item(2).Insert() | Out-Null }
  'delete row 5 unlocked' = { param($s) $s.Rows.Item(5).Delete() | Out-Null }
  'delete row 6 locked'   = { param($s) $s.Rows.Item(6).Delete() | Out-Null }
  'delete col E unlocked' = { param($s) $s.Columns.Item(5).Delete() | Out-Null }
  'delete col A locked'   = { param($s) $s.Columns.Item(1).Delete() | Out-Null }
  'sort unlocked B1:C3'   = { param($s) $s.Range('B1:C3').Sort($s.Range('B1'), 1, [Type]::Missing, [Type]::Missing, 1, [Type]::Missing, 1, 2) | Out-Null }
  'sort locked A1:A3'     = { param($s) $s.Range('A1:A3').Sort($s.Range('A1'), 1, [Type]::Missing, [Type]::Missing, 1, [Type]::Missing, 1, 2) | Out-Null }
  'autofilter B1:C3'      = { param($s) $s.Range('B1:C3').AutoFilter() | Out-Null }
  'hyperlink unlocked B1' = { param($s) $s.Hyperlinks.Add($s.Range('B1'), 'http://example.com') | Out-Null }
  'hyperlink locked A1'   = { param($s) $s.Hyperlinks.Add($s.Range('A1'), 'http://example.com') | Out-Null }
  'note unlocked B1'      = { param($s) $s.Range('B1').AddComment('x') | Out-Null }
  'validation B1'         = { param($s) $s.Range('B1').Validation.Add(3, 1, 1, 'a,b') | Out-Null }
  'conditional B1'        = { param($s) $s.Range('B1').FormatConditions.Add(1, 5, '=0') | Out-Null }
  'name define'           = { param($s) $s.Parent.Names.Add('Nm', '=Sheet1!$A$1') | Out-Null }
  'freeze'                = { param($s) $s.Activate(); $s.Application.ActiveWindow.FreezePanes = $true }
  'fill down unlocked'    = { param($s) $s.Range('B1:B3').FillDown() | Out-Null }
  'text to columns B'     = { param($s) $s.Range('B1:B3').TextToColumns($s.Range('B1')) | Out-Null }
  'remove dups B1:C3'     = { param($s) $s.Range('B1:C3').RemoveDuplicates(1) | Out-Null }
  'list object'           = { param($s) $s.ListObjects.Add(1, $s.Range('B1:C3'), $null, 1) | Out-Null }
  'chart'                 = { param($s) $s.ChartObjects().Add(10, 10, 100, 100) | Out-Null }
  'shape'                 = { param($s) $s.Shapes.AddShape(1, 10, 10, 50, 50) | Out-Null }
  'tab color'             = { param($s) $s.Tab.Color = 255 }
  'rename sheet'          = { param($s) $s.Name = 'Renamed' }
  'delete sheet'          = { param($s) $s.Delete() }
  'unhide sheet'          = { param($s) $s.Visible = 0 }
  'scenario add'          = { param($s) $s.Scenarios().Add('S1', $s.Range('B1'), 5) | Out-Null }
  'paste unlocked B1'     = { param($s) $s.Range('B2').Copy() | Out-Null; $s.Range('B1').PasteSpecial(-4163) | Out-Null }
  'group rows'            = { param($s) $s.Rows('1:2').Group() | Out-Null }
  'cut paste unlocked'    = { param($s) $s.Range('B2').Cut($s.Range('C1')) | Out-Null }
  'row insert, hidden row 7' = { param($s) $s.Rows.Item(7).Hidden = $false }
}
$lines = @()
try {
  $b = $x.Workbooks.Add()
  $b.Worksheets.Add() | Out-Null
  foreach ($opName in $ops.Keys) {
    foreach ($flag in $flags) {
      $s = $b.Worksheets.Add()
      $s.Name = 'T'
      $s.Range('A1:A3').Value2 = 1
      $s.Range('B1:E8').Value2 = 2
      $s.Range('B1:E8').Locked = $false
      $s.Rows.Item(5).Locked = $false
      $s.Range('A6:A6').Locked = $true
      $s.Rows.Item(6).Locked = $true
      $s.Range('E1:E8').Locked = $false
      $s.Range('B2').Value2 = 2
      Protect-With $s $flag
      $r = 'ok'
      try { & $ops[$opName] $s } catch { $r = $_.Exception.Message -replace "`r|`n", ' ' }
      $lines += "$opName`t$flag`t$r"
      $x.DisplayAlerts = $false
      try { $s.Unprotect('') } catch {}
      try { $s.Delete() } catch { try { $s.Visible = -1; $s.Delete() } catch {} }
    }
  }
  $b.Close($false)
} finally { $x.Quit(); $lines | Set-Content -Path $Out }
