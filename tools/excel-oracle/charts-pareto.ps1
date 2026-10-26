# Make a Pareto chart in Excel with AddChart2 from a block of data, save it, and print the chart type Excel
# reports. The saved file is what crates/unluminous-sheet writes for a Pareto chart is compared with.
#
#   pwsh tools/excel-oracle/charts-pareto.ps1 -Out C:/path/pareto.xlsx
param([Parameter(Mandatory = $true)][string]$Out)
$ErrorActionPreference = 'Stop'
$Out = [IO.Path]::GetFullPath($Out)
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $book = $excel.Workbooks.Add()
    $ws = $book.Worksheets.Item(1)
    $ws.Name = 'Sheet1'
    $ws.Range('A1').Value2 = 'Group'; $ws.Range('B1').Value2 = 'Value'
    $names = 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'
    $values = 5, 9, 3, 7, 4, 8, 6, 2
    for ($i = 0; $i -lt 8; $i++) { $ws.Cells.Item($i + 2, 1).Value2 = $names[$i]; $ws.Cells.Item($i + 2, 2).Value2 = [double]$values[$i] }
    $shape = $ws.Shapes.AddChart2(-1, 122, 150, 10, 360, 220)
    $shape.Chart.SetSourceData($ws.Range('A1:B9'))
    "type=" + $shape.Chart.ChartType
    $book.SaveAs($Out, 51)
    $book.Close($false)
} finally {
    $excel.Quit()
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
}
