# Ask Excel for the automatic scale of a chart's value axis for each data set in a JSON lines file: the minimum,
# maximum, major and minor units it chooses, at one or more chart heights. The answers are what
# crates/unluminous-sheet/src/plot/scale.rs is checked against.
#
#   pwsh tools/excel-oracle/charts-scale.ps1 -Cases scale-cases.jsonl -Heights 216 -ChartType 51 > scale-excel.jsonl
param(
    [Parameter(Mandatory = $true)][string]$Cases,
    [double[]]$Heights = @(216),
    [double]$Width = 360,
    [int]$ChartType = 51,
    [switch]$XAxis
)
$ErrorActionPreference = 'Stop'
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $book = $excel.Workbooks.Add()
    $ws = $book.Worksheets.Item(1)
    foreach ($line in Get-Content -LiteralPath $Cases) {
        if (-not $line.Trim()) { continue }
        $case = $line | ConvertFrom-Json
        $ws.Cells.Clear() | Out-Null
        $values = @($case.values)
        for ($i = 0; $i -lt $values.Count; $i++) {
            $ws.Cells.Item($i + 1, 1).Value2 = [double]($i + 1)
            $ws.Cells.Item($i + 1, 2).Value2 = [double]$values[$i]
        }
        foreach ($height in $Heights) {
            $shape = $ws.Shapes.AddChart2(-1, $ChartType, 10, 10, $Width, $height)
            $chart = $shape.Chart
            while ($chart.SeriesCollection().Count -gt 0) { $chart.SeriesCollection(1).Delete() | Out-Null }
            $s = $chart.SeriesCollection().NewSeries()
            $s.Values = $ws.Range($ws.Cells.Item(1, 2), $ws.Cells.Item($values.Count, 2))
            if ($ChartType -eq -4169) { $s.XValues = $ws.Range($ws.Cells.Item(1, 1), $ws.Cells.Item($values.Count, 1)) }
            $axis = $chart.Axes($(if ($XAxis) { 1 } else { 2 }))
            if ($case.kind -eq 'log') {
                try { $axis.ScaleType = -4133 } catch { }
            }
            $o = [ordered]@{ id = $case.id; kind = $case.kind; height = $height }
            try {
                $o.min = $axis.MinimumScale; $o.max = $axis.MaximumScale
                $o.major = $axis.MajorUnit; $o.minor = $axis.MinorUnit
                $o.plotInsideHeight = $chart.PlotArea.InsideHeight
                $o.plotInsideWidth = $chart.PlotArea.InsideWidth
            } catch { $o.error = $_.Exception.Message }
            $o | ConvertTo-Json -Compress
            $shape.Delete() | Out-Null
        }
    }
    $book.Close($false)
} finally {
    $excel.Quit()
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
}
