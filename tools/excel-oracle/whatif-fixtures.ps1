# Make the fixtures that tests/whatif.rs compares the engine's What If analysis with: Goal Seek, data tables
# and the Scenario Manager, each done by Excel itself.
#
#   pwsh tools/excel-oracle/whatif-fixtures.ps1 [-Out crates/unluminous-sheet/tests/fixtures/whatif]
#
# Every workbook is saved in pairs: the workbook before an operation (`*-input.xlsx`) and the workbook after
# Excel did it. The test reads the input, does the same operation with the engine and compares the result
# with what Excel saved. `goalseek.tsv` has one line a Goal Seek: the formula, the start, the goal, the
# MaxChange, whether Excel found a solution, the value it left and the formula's value there.
param([string]$Out = (Join-Path $PSScriptRoot '..\..\crates\unluminous-sheet\tests\fixtures\whatif'))
$ErrorActionPreference = 'Stop'
$inv = [Globalization.CultureInfo]::InvariantCulture
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$excel.UserName = 'Fixture Author'
$missing = [Type]::Missing

function Save($book, $name) { $book.SaveAs((Join-Path $Out $name), 51) }

try {
    # ------------------------------------------------------------------------------------------------
    # Data tables: one variable down a column, along a row, two variables, and two formulas.
    $book = $excel.Workbooks.Add()
    $s = $book.Worksheets.Item(1)
    $s.Range('B1').Value2 = 2; $s.Range('B2').Value2 = 3; $s.Range('B3').Formula = '=B1*B2+B1^2'
    $s.Range('E1').Formula = '=B3'
    foreach ($i in 0..3) { $s.Cells.Item(2 + $i, 4).Value2 = $i + 1 }
    $s.Range('G2').Formula = '=B3'
    foreach ($i in 0..3) { $s.Cells.Item(1, 8 + $i).Value2 = 10 * ($i + 1) }
    $s.Range('M1').Formula = '=B3'
    foreach ($i in 0..2) { $s.Cells.Item(1, 14 + $i).Value2 = $i + 1; $s.Cells.Item(2 + $i, 13).Value2 = 5 + $i }
    $s.Range('S1').Formula = '=B3'; $s.Range('T1').Formula = '=B1+B2'
    $s.Range('R2').Value2 = 1; $s.Range('R3').Value2 = 2
    Save $book 'datatable-input.xlsx'
    $s.Range('D1:E5').Table($missing, $s.Range('B1')) | Out-Null
    $s.Range('G1:K2').Table($s.Range('B2'), $missing) | Out-Null
    $s.Range('M1:P4').Table($s.Range('B2'), $s.Range('B1')) | Out-Null
    $s.Range('R1:T3').Table($missing, $s.Range('B1')) | Out-Null
    Save $book 'datatable.xlsx'
    $s.Range('B1').Value2 = 3; $s.Range('B2').Value2 = 5
    $s.Range('D3').Value2 = 7
    $s.Range('N1').Value2 = 4
    Save $book 'datatable-changed.xlsx'
    $book.Close($false)

    # ------------------------------------------------------------------------------------------------
    # Goal Seek: twenty cases that find a solution and some that do not.
    $cases = @(
        @('=A1^2', 1, 2, 0.001), @('=A1^2', 10, 2, 0.001), @('=A1^2', -1, 2, 0.001), @('=A1^2', 0.5, 2, 0.001),
        @('=A1^2', 1, 2, 1e-10), @('=3*A1+2', 0, 11, 0.001), @('=3*A1+2', 5, 11, 0.001), @('=A1^3-A1', 1, 5, 0.001),
        @('=A1^3-A1', 1, 5, 1e-10), @('=EXP(A1)', 0, 10, 0.001), @('=EXP(A1)', 1, 10, 1e-10), @('=LN(A1)', 1, 2, 0.001),
        @('=SIN(A1)', 1, 0.5, 0.001), @('=SIN(A1)', 1, 0.5, 1e-10), @('=1/A1', 1, 0.25, 0.001), @('=SQRT(A1)', 4, 3, 0.001),
        @('=(A1-3)^2', 0, 0, 1e-8), @('=A1^2', 0, 2, 0.001), @('=A1^2', 0.001, 2, 0.001), @('=A1^2', 100, 2, 0.001),
        @('=A1^2', 1, 0.5, 0.001), @('=A1^2', 1, 4, 0.001), @('=ABS(A1-3)', 0, 0, 0.001), @('=A1^20', 0.1, 1, 0.001),
        @('=LN(A1)', 1, -5, 0.001), @('=A1', 5, 5.0005, 0.001), @('=A1^2+1', 0, 0, 0.001), @('=A1*0+1', 3, 2, 0.001),
        @('=A1^2', 1, 2, 0)
    )
    $book = $excel.Workbooks.Add()
    $s = $book.Worksheets.Item(1)
    $lines = @()
    foreach ($c in $cases) {
        $s.Range('A1').Value2 = [double]$c[1]
        $s.Range('B1').Formula = $c[0]
        $excel.MaxChange = [double]$c[3]
        $ok = $s.Range('B1').GoalSeek([double]$c[2], $s.Range('A1'))
        $fields = @($c[0], ([double]$c[1]).ToString('R', $inv), ([double]$c[2]).ToString('R', $inv), ([double]$c[3]).ToString('R', $inv), $ok, $s.Range('A1').Value2.ToString('R', $inv), $s.Range('B1').Value2.ToString('R', $inv))
        $lines += ($fields -join [string][char]9)
    }
    $excel.MaxChange = 0.001
    $lines | Set-Content -Path (Join-Path $Out 'goalseek.tsv')
    $book.Close($false)

    # ------------------------------------------------------------------------------------------------
    # Scenarios.
    function New-Model($book) {
        $s = $book.Worksheets.Item(1)
        $s.Name = 'Model'
        $s.Range('A1').Value2 = 'Price'; $s.Range('B1').Value2 = 10
        $s.Range('A2').Value2 = 'Units'; $s.Range('B2').Value2 = 100
        $s.Range('A3').Value2 = 'Revenue'; $s.Range('B3').Formula = '=B1*B2'
        $s.Range('A4').Value2 = 'Note'; $s.Range('B4').Value2 = 'text'
        $s.Range('A5').Value2 = 'Margin'; $s.Range('B5').Formula = '=(B3-400)/B3'
        $s.Range('B3').NumberFormat = '#,##0'
        $s.Range('B5').NumberFormat = '0.00%'
        return $s
    }
    $book = $excel.Workbooks.Add(); $s = New-Model $book
    Save $book 'scenarios-input.xlsx'
    $null = $s.Scenarios().Add('Base', $s.Range('B1:B2'), @(10, 100))
    $null = $s.Scenarios().Add('Best', $s.Range('B1:B2'), @(12, 150), 'Optimistic case')
    $null = $s.Scenarios().Add('Worst', $s.Range('B1:B2,B4'), @(8, 60, 'abc'))
    Save $book 'scenarios-added.xlsx'
    $s.Scenarios('Best').Show() | Out-Null
    Save $book 'scenarios-shown.xlsx'
    $s.Scenarios('Base').Show() | Out-Null
    $s.Scenarios('Best').ChangeScenario($s.Range('B1:B2,B5'), @(13, 160, 0.5))
    Save $book 'scenarios-edited.xlsx'
    $s.Scenarios('Worst').Delete()
    Save $book 'scenarios-deleted.xlsx'
    $book.Close($false)

    $book = $excel.Workbooks.Add(); $s = New-Model $book
    $null = $s.Scenarios().Add('Base', $s.Range('B1:B2'), @(10, 100))
    $null = $s.Scenarios().Add('Best', $s.Range('B1:B2'), @(12, 150), 'Optimistic case')
    $null = $s.Scenarios().Add('Worst', $s.Range('B1:B2,B4'), @(8, 60, 'abc'))
    Save $book 'scenarios-summary-input.xlsx'
    $s.Scenarios().CreateSummary(1, $s.Range('B3,B5')) | Out-Null
    Save $book 'scenarios-summary.xlsx'
    $s.Scenarios().CreateSummary(1, $s.Range('B3')) | Out-Null
    Save $book 'scenarios-summary-twice.xlsx'
    $book.Close($false)

    $book = $excel.Workbooks.Add(); $s = New-Model $book
    $s.Range('B1').Name = 'Price'
    $s.Range('B3').Name = 'Revenue'
    $s.Range('B1').NumberFormat = '$#,##0.00'
    $null = $s.Scenarios().Add('Low', $s.Range('B1:B2'), @(8, 60), 'Cautious, with a long enough comment to wrap onto several lines')
    $null = $s.Scenarios().Add('High', $s.Range('B1:B2'), @(12, 150))
    Save $book 'scenarios-names-input.xlsx'
    $s.Scenarios().CreateSummary(1, $s.Range('B3,B5')) | Out-Null
    Save $book 'scenarios-names-summary.xlsx'
    $book.Close($false)
} finally {
    $excel.Quit()
}
