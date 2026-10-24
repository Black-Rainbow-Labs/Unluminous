# Build the workbook Go To Special and the precedent and dependent traces are tested against, and ask Excel
# what each option selects. `special.xlsx` is the workbook; `special-expected.json` has one row for each
# option and selection with the cells Excel selects, or the sentence it gives when it finds none.
#
#   pwsh tools/excel-oracle/make-review-special.ps1 [-Folder crates/unluminous-sheet/tests/fixtures/review]
param([string]$Folder = 'crates/unluminous-sheet/tests/fixtures/review')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
New-Item -ItemType Directory -Force -Path $Folder | Out-Null
$folder = (Resolve-Path -LiteralPath $Folder).Path
$path = Join-Path $folder 'special.xlsx'
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
function Put($sheet, $address, $content) { $sheet.Range($address).Formula = $content }
function Cells-Of($range) {
    $out = @()
    foreach ($area in $range.Areas) { foreach ($c in $area.Cells) { $out += $c.Address($false, $false) } }
    return $out
}
function Try-Cells($script) {
    try { $r = & $script; if ($null -eq $r) { return @{ error = 'none' } }; return @{ cells = @(Cells-Of $r) } }
    catch { return @{ error = $_.Exception.Message } }
}
try {
    $book = $excel.Workbooks.Add()
    while ($book.Worksheets.Count -lt 2) { $book.Worksheets.Add([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count)) | Out-Null }
    $s = $book.Worksheets.Item(1)
    $s.Name = 'Sheet1'
    $t = $book.Worksheets.Item(2)
    $t.Name = 'Sheet2'
    foreach ($pair in @(
            @('A1', 'Item'), @('B1', 'Qty'), @('C1', 'Price'), @('D1', 'Total'),
            @('A2', 'Pens'), @('B2', '4'), @('C2', '1.5'), @('D2', '=B2*C2'),
            @('A3', 'Ink'), @('B3', '10'), @('C3', '=1/0'), @('D3', '=B3*C3'),
            @('A4', ''), @('B4', 'TRUE'), @('C4', '=B4'), @('D4', '=IF(B4,"yes","no")'),
            @('F1', 'north'), @('G1', '7'), @('F3', 'south'), @('G3', '=G1+1'),
            @('A6', 'x'), @('B6', '1'), @('C6', '1'), @('D6', '1'),
            @('A7', 'y'), @('B7', '1'), @('C7', '2'), @('D7', '1'),
            @('P1', '=Q1+Q2'), @('Q1', '=R1*2'), @('R1', '5'), @('Q2', '=Sheet2!A1'), @('S1', '=P1+1'), @('T1', '=S1+Q1'),
            @('J10', '1'), @('K10', '2'), @('J11', '3'), @('K11', '4'))) {
        if ($pair[1] -ne '') { Put $s $pair[0] $pair[1] }
    }
    $s.Range('A4').Value2 = ''
    $s.Range('E10:F11').FormulaArray = '=J10:K11*2'
    $s.Range('H1').Formula2 = '=SEQUENCE(3)'
    Put $s 'M5' 'merged'
    $s.Range('M5:N5').Merge()
    $s.Rows.Item(8).Hidden = $true
    $s.Columns.Item(9).Hidden = $true
    Put $s 'I2' 'hidden col'
    Put $t 'A1' '=Sheet1!R1+1'
    Put $t 'B1' '=A1*2'
    Put $s 'U1' '=Sheet2!B1'
    $s.Range('A1').AddComment('a note') | Out-Null
    $s.Range('D3').AddCommentThreaded('a thread') | Out-Null
    $s.Range('B2:B3').FormatConditions.Add(1, 5, '=5') | Out-Null
    $s.Range('D2').FormatConditions.Add(1, 5, '=5') | Out-Null
    $s.Range('C4').FormatConditions.Add(1, 6, '=2') | Out-Null
    $s.Range('A2:A3').Validation.Add(3, 1, 1, 'a,b,c')
    $s.Range('B4').Validation.Add(3, 1, 1, 'a,b,c')
    $s.Range('C1').Validation.Add(1, 1, 1, '1', '9')
    $s.Range('G7').Interior.Color = 0x00FFFF
    $chart = $s.ChartObjects().Add(400, 10, 200, 100)
    $chart.Chart.SetSourceData($s.Range('J10:K11'))
    $s.Shapes.AddShape(1, 400, 150, 50, 50) | Out-Null
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
    $book.SaveAs($path, 51)

    $rows = @()
    $selections = @(@('A1', 'A1'), @('A1:D4', 'A1'), @('B2:D4', 'B2'), @('A1:H12', 'A1'), @('A1:H12', 'C3'), @('A6:D7', 'A6'), @('B3', 'B3'), @('D3', 'D3'), @('P1', 'P1'), @('R1', 'R1'), @('Q1:Q2', 'Q1'), @('E10', 'E10'), @('H2', 'H2'), @('F1', 'F1'), @('M5', 'M5'), @('A1:A30', 'A1'), @('I1:I3', 'I1'))
    $types = @(
        @('notes', -4144, $null), @('constants', 2, 23), @('constants-numbers', 2, 1), @('constants-text', 2, 2), @('constants-logical', 2, 4), @('constants-errors', 2, 16),
        @('formulas', -4123, 23), @('formulas-numbers', -4123, 1), @('formulas-text', -4123, 2), @('formulas-logical', -4123, 4), @('formulas-errors', -4123, 16),
        @('blanks', 4, $null), @('visible', 12, $null), @('last', 11, $null),
        @('cf-all', -4172, $null), @('cf-same', -4173, $null), @('validation-all', -4174, $null), @('validation-same', -4175, $null)
    )
    foreach ($sel in $selections) {
        foreach ($ty in $types) {
            $range = $s.Range($sel[0])
            $s.Range($sel[1]).Select() | Out-Null
            $range.Select() | Out-Null
            $s.Range($sel[1]).Activate() | Out-Null
            $r = Try-Cells { if ($null -eq $ty[2]) { $range.SpecialCells($ty[1]) } else { $range.SpecialCells($ty[1], $ty[2]) } }
            $rows += [pscustomobject]@{ kind = $ty[0]; selection = $sel[0]; active = $sel[1]; cells = $r.cells; error = $r.error }
        }
        $active = $s.Range($sel[1])
        $range = $s.Range($sel[0])
        $r = Try-Cells { $active.CurrentRegion }
        $rows += [pscustomobject]@{ kind = 'current-region'; selection = $sel[0]; active = $sel[1]; cells = $r.cells; error = $r.error }
        $r = Try-Cells { $active.CurrentArray }
        $rows += [pscustomobject]@{ kind = 'current-array'; selection = $sel[0]; active = $sel[1]; cells = $r.cells; error = $r.error }
        if ($range.Cells.Count -gt 1) {
            $r = Try-Cells { $range.RowDifferences($active) }
            $rows += [pscustomobject]@{ kind = 'row-differences'; selection = $sel[0]; active = $sel[1]; cells = $r.cells; error = $r.error }
            $r = Try-Cells { $range.ColumnDifferences($active) }
            $rows += [pscustomobject]@{ kind = 'column-differences'; selection = $sel[0]; active = $sel[1]; cells = $r.cells; error = $r.error }
        }
        foreach ($tr in @(@('precedents-direct', 'DirectPrecedents'), @('precedents-all', 'Precedents'), @('dependents-direct', 'DirectDependents'), @('dependents-all', 'Dependents'))) {
            $r = Try-Cells { $range.($tr[1]) }
            $rows += [pscustomobject]@{ kind = $tr[0]; selection = $sel[0]; active = $sel[1]; cells = $r.cells; error = $r.error }
        }
    }
    $objects = @(); foreach ($shape in $s.Shapes) { $objects += $shape.TopLeftCell.Address($false, $false) + '|' + $shape.BottomRightCell.Address($false, $false) }
    $json = [pscustomobject]@{ rows = $rows; objects = $objects; usedRange = $s.UsedRange.Address($false, $false) } | ConvertTo-Json -Depth 6 -Compress
    Set-Content -LiteralPath (Join-Path $folder 'special-expected.json') -Value $json -Encoding utf8
    $book.Close($false)
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
