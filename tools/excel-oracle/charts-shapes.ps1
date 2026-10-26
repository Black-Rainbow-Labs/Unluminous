# Make a workbook of small blocks of data of different shapes, each with a column chart made by Excel from
# the block with SetSourceData, named by the range it was made from. tests/charts.rs reads the workbook and
# checks that insert_chart lays the same range out into the same series as Excel did.
#
#   pwsh tools/excel-oracle/charts-shapes.ps1 -Out crates/unluminous-sheet/tests/fixtures/charts/shapes.xlsx
param([Parameter(Mandatory = $true)][string]$Out)
$ErrorActionPreference = 'Stop'
$Out = [IO.Path]::GetFullPath($Out)
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false

# Each block: its top left cell and rows of cells; a string is text, a number is a number, a date is d:serial.
$blocks = @(
    @{ at = 'A1'; rows = @(@('Cat','One','Two'), @('a',1,2), @('b',3,4), @('c',5,6), @('d',7,8), @('e',9,10)) },
    @{ at = 'F1'; rows = @(@($null,'One','Two'), @('a',1,2), @('b',3,4), @('c',5,6), @('d',7,8), @('e',9,10)) },
    @{ at = 'K1'; rows = @(@($null,'a','b','c','d','e'), @('x',1,2,3,4,5), @('y',6,7,8,9,10)) },
    @{ at = 'S1'; rows = @(@('Cat','One','Two','Three'), @('a',1,2,3), @('b',4,5,6), @('c',7,8,9)) },
    @{ at = 'A9'; rows = @(@(1,2,3), @(4,5,6), @(7,8,9), @(10,11,12), @(13,14,15)) },
    @{ at = 'F9'; rows = @(@(1,2,3,4,5), @(6,7,8,9,10), @(11,12,13,14,15)) },
    @{ at = 'K9'; rows = @(@('Sales'), @(5), @(7), @(9), @(4)) },
    @{ at = 'M9'; rows = @(@(5), @(7), @(9), @(4)) },
    @{ at = 'A17'; rows = @(@('Date','One','Two'), @('d:45292',1,2), @('d:45293',3,4), @('d:45294',5,6), @('d:45295',7,8)) },
    @{ at = 'F17'; rows = @(@('Year','One','Two'), @(2020,1,2), @(2021,3,4), @(2022,5,6), @(2023,7,8)) },
    @{ at = 'K17'; rows = @(@($null,'One','Two'), @(2020,1,2), @(2021,3,4), @(2022,5,6), @(2023,7,8)) },
    @{ at = 'P17'; rows = @(@('Name','Score'), @('a',1), @('b',3), @('c',5), @('d',7), @('e',9), @('f',11), @('g',13)) },
    @{ at = 'A25'; rows = @(@('Cat','One','Two','Three','Four'), @('a',1,2,3,4), @('b',5,6,7,8)) },
    @{ at = 'H25'; rows = @(@('a','One','Two'), @('b',1,2), @('c',3,4), @('d',5,6)) }
)

try {
    $book = $excel.Workbooks.Add()
    $ws = $book.Worksheets.Item(1)
    $ws.Name = 'Sheet1'
    foreach ($b in $blocks) {
        $origin = $ws.Range($b.at)
        $rowsN = $b.rows.Count
        $colsN = ($b.rows | ForEach-Object { $_.Count } | Measure-Object -Maximum).Maximum
        for ($r = 0; $r -lt $rowsN; $r++) {
            for ($c = 0; $c -lt $b.rows[$r].Count; $c++) {
                $v = $b.rows[$r][$c]
                $cell = $origin.Offset($r, $c)
                if ($null -eq $v) { continue }
                if ($v -is [string] -and $v.StartsWith('d:')) {
                    $cell.Value2 = [double]($v.Substring(2)); $cell.NumberFormat = 'm/d/yyyy'
                } elseif ($v -is [string]) { $cell.Value2 = $v }
                else { $cell.Value2 = [double]$v }
            }
        }
        $range = $ws.Range($origin, $origin.Offset($rowsN - 1, $colsN - 1))
        $shape = $ws.Shapes.AddChart2(-1, 51, 20, 400, 200, 120)
        $shape.Chart.SetSourceData($range)
        $shape.Name = $range.Address($false, $false)
    }
    $book.SaveAs($Out, 51)
    $book.Close($false)
} finally {
    $excel.Quit()
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
}
