# For each data set of a JSON lines file ({"id", "x", "y"}) and each kind of trendline, add the trendline to a
# scatter chart in Excel with its equation and R squared shown, and print the label text Excel makes. The
# answers are what crates/unluminous-sheet/src/plot/{trend,equation}.rs are checked against.
#
#   pwsh tools/excel-oracle/charts-trend.ps1 -Cases trend-cases.jsonl > trend-excel.jsonl
param([Parameter(Mandatory = $true)][string]$Cases)
$ErrorActionPreference = 'Stop'
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$kinds = @(
    @{ name = 'linear'; type = -4132 },
    @{ name = 'exp'; type = 5 },
    @{ name = 'log'; type = -4133 },
    @{ name = 'poly2'; type = 3; order = 2 },
    @{ name = 'poly3'; type = 3; order = 3 },
    @{ name = 'poly4'; type = 3; order = 4 },
    @{ name = 'power'; type = 4 }
)
try {
    $book = $excel.Workbooks.Add()
    $ws = $book.Worksheets.Item(1)
    foreach ($line in Get-Content -LiteralPath $Cases) {
        if (-not $line.Trim()) { continue }
        $case = $line | ConvertFrom-Json
        $ws.Cells.Clear() | Out-Null
        $xs = @($case.x); $ys = @($case.y)
        for ($i = 0; $i -lt $ys.Count; $i++) {
            $ws.Cells.Item($i + 1, 1).Value2 = [double]$xs[$i]
            $ws.Cells.Item($i + 1, 2).Value2 = [double]$ys[$i]
        }
        $shape = $ws.Shapes.AddChart2(-1, -4169, 10, 10, 400, 260)
        $chart = $shape.Chart
        while ($chart.SeriesCollection().Count -gt 0) { $chart.SeriesCollection(1).Delete() | Out-Null }
        $s = $chart.SeriesCollection().NewSeries()
        $s.XValues = $ws.Range($ws.Cells.Item(1, 1), $ws.Cells.Item($ys.Count, 1))
        $s.Values = $ws.Range($ws.Cells.Item(1, 2), $ws.Cells.Item($ys.Count, 2))
        foreach ($k in $kinds) {
            $o = [ordered]@{ id = $case.id; kind = $k.name }
            try {
                $t = $s.Trendlines().Add($k.type)
                if ($k.order) { $t.Order = $k.order }
                $t.DisplayEquation = $true
                $t.DisplayRSquared = $true
                $chart.Refresh()
                [void]$chart.Export((Join-Path ([IO.Path]::GetTempPath()) 'charts-trend-render.png'))
                $o.text = $t.DataLabel.Text
                if (-not $o.text) { $o.text = $t.DataLabel.Caption }
                if (-not $o.text) { $o.text = [string]$t.DataLabel.Formula }
                $t.Delete() | Out-Null
            } catch { $o.error = $_.Exception.Message }
            $o | ConvertTo-Json -Compress
        }
        $shape.Delete() | Out-Null
    }
    $book.Close($false)
} finally {
    $excel.Quit()
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
}
