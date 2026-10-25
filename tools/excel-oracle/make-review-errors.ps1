# Build a workbook with every kind of cell Excel's Error Checking flags, and ask Excel which rules flag
# which cell. `errors.xlsx` is the workbook (with one error ignored so `ignoredErrors` is in the file) and
# `errors-expected.json` lists, for each cell Excel flags, the rules it flags it for, by Excel's numbers:
# 1 evaluates to an error, 2 text date with a two digit year, 3 number stored as text, 4 inconsistent
# formula, 5 formula omits adjacent cells, 6 unlocked cell with a formula, 7 formula refers to empty
# cells, 9 inconsistent calculated column formula.
#
#   pwsh tools/excel-oracle/make-review-errors.ps1 [-Folder crates/unluminous-sheet/tests/fixtures/review]
param([string]$Folder = 'crates/unluminous-sheet/tests/fixtures/review')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
New-Item -ItemType Directory -Force -Path $Folder | Out-Null
$folder = (Resolve-Path -LiteralPath $Folder).Path
$path = Join-Path $folder 'errors.xlsx'
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
function Put($sheet, $address, $content) { $sheet.Range($address).Formula = $content }
try {
    $opt = $excel.ErrorCheckingOptions
    $opt.BackgroundChecking = $true
    foreach ($name in 'EvaluateToError', 'TextDate', 'NumberAsText', 'InconsistentFormula', 'OmittedCells', 'UnlockedFormulaCells', 'EmptyCellReferences', 'ListDataValidation', 'InconsistentTableFormula') {
        $opt.$name = $true
    }
    $book = $excel.Workbooks.Add()
    $s = $book.Worksheets.Item(1)
    $s.Name = 'Checks'
    # Inconsistent formulas, down columns and along rows.
    foreach ($r in 1..6) { Put $s "A$r" "$r" }
    foreach ($r in 1..6) { Put $s "B$r" "=A$r*2" }
    Put $s 'B3' '=A3*3'
    foreach ($r in 1..6) { Put $s "C$r" "=A$r+1" }
    Put $s 'C1' '=A1+9'
    Put $s 'C6' '=A6+9'
    foreach ($r in 1..6) { Put $s "D$r" "=A$r-1" }
    Put $s 'D2' '=A2+5'
    Put $s 'D3' '=A3+5'
    foreach ($r in 1..6) { Put $s "E$r" "=A$r/1" }
    Put $s 'E5' '=A5/2'
    foreach ($c in 'G', 'H', 'I', 'J', 'K', 'L') { Put $s "${c}1" '=1+1' }
    Put $s 'I1' '=2+2'
    foreach ($c in 'G', 'H', 'I', 'J') { Put $s "${c}2" "=${c}1*2" }
    Put $s 'J2' '=J1*3'
    # A formula between constants.
    Put $s 'G4' '10'
    Put $s 'G5' '=G4+1'
    Put $s 'G6' '12'
    # Numbers stored as text.
    foreach ($pair in @(@('A9', '123'), @('A10', '12.5'), @('A11', '1,234'), @('A12', ' 5 '), @('A13', 'abc'), @('A14', '1e3'), @('A15', '$5'), @('A16', '-7'), @('A17', '50%'), @('A18', '(8)'))) {
        $s.Range($pair[0]).NumberFormat = '@'
        $s.Range($pair[0]).Value2 = $pair[1]
    }
    Put $s 'B9' '=TEXT(5,"0")'
    Put $s 'B10' '=" 7"'
    $s.Range('B11').Value2 = "'42"
    # Text dates with two digit years.
    foreach ($pair in @(@('D9', '1/2/30'), @('D10', '1/2/2030'), @('D11', '1/2/99'), @('D12', '5-Jan-25'), @('D13', '5-Jan-2025'), @('D14', 'Jan 5 25'), @('D15', '12/31/68'), @('D16', '12/31/29'))) {
        $s.Range($pair[0]).NumberFormat = '@'
        $s.Range($pair[0]).Value2 = $pair[1]
    }
    # Formulas that omit adjacent cells.
    foreach ($r in 20..24) { Put $s "A$r" "$($r - 19)" }
    Put $s 'B20' '=SUM(A20:A23)'
    Put $s 'B21' '=SUM(A20:A24)'
    Put $s 'B22' '=SUM(A21:A24)'
    Put $s 'B23' '=AVERAGE(A20:A22)'
    Put $s 'B24' '=A20+A21'
    foreach ($c in 'D', 'E', 'F') { Put $s "${c}20" '3' }
    Put $s 'G20' '=SUM(D20:E20)'
    Put $s 'H25' '=SUM(A20:A24)'
    Put $s 'D22' '=SUM(D20:D21)'
    # An unlocked cell with a formula, and an unlocked constant.
    Put $s 'A28' '=1+2'
    $s.Range('A28').Locked = $false
    Put $s 'A29' '5'
    $s.Range('A29').Locked = $false
    # References to empty cells.
    Put $s 'B28' '=Z1+1'
    Put $s 'B29' '=SUM(Z1:Z3)'
    Put $s 'B30' '=SUM(A1:A3)'
    Put $s 'B31' '=A100*2'
    Put $s 'B32' '=SUM(A1:A8)'
    # Formulas that give errors, and an error typed in.
    Put $s 'A33' '=1/0'
    Put $s 'A34' '=NA()'
    Put $s 'A35' '=A33+1'
    Put $s 'A36' '#REF!'
    Put $s 'A37' '=IFERROR(1/0,0)'
    # A table with a calculated column and one exception.
    Put $s 'K9' 'Item'
    Put $s 'L9' 'Qty'
    Put $s 'M9' 'Double'
    foreach ($r in 10..14) { Put $s "K$r" "i$r"; Put $s "L$r" "$r" }
    $t = $s.ListObjects.Add(1, $s.Range('K9:M14'), [Type]::Missing, 1)
    $t.Name = 'Sales'
    $s.Range('M10').Formula = '=[@Qty]*2'
    $s.Range('M12').Formula = '=[@Qty]*3'
    $s.Range('M14').Value2 = 99
    # One error ignored, so the file has an ignoredErrors element.
    $s.Range('B3').Errors.Item(4).Ignore = $true
    $s.Range('A11').Errors.Item(3).Ignore = $true
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
    $book.SaveAs($path, 51)
    $flags = [ordered]@{}
    foreach ($cell in $s.Range('A1:M40').Cells) {
        $rules = @()
        foreach ($i in 1, 2, 3, 4, 5, 6, 7, 9) { if ($cell.Errors.Item($i).Value) { $rules += $i } }
        if ($rules.Count -gt 0) { $flags[$cell.Address($false, $false)] = $rules }
    }
    $ignored = [ordered]@{}
    foreach ($address in 'B3', 'A11') {
        foreach ($i in 1..9) { if ($s.Range($address).Errors.Item($i).Ignore) { $ignored[$address] = $i } }
    }
    $json = [pscustomobject]@{ flags = $flags; ignored = $ignored } | ConvertTo-Json -Depth 5 -Compress
    Set-Content -LiteralPath (Join-Path $folder 'errors-expected.json') -Value $json -Encoding utf8
    $book.Close($false)
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
