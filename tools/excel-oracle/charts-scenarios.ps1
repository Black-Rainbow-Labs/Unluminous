# Make the workbooks the chart editing tests compare against: for each scenario, Excel builds a chart over the
# same small data sheet and makes the same changes through COM that a Session method makes, and saves it.
# tests/charts.rs makes the same scenarios with Session methods and asks Excel what it sees in both.
#
#   pwsh tools/excel-oracle/charts-scenarios.ps1 -Out crates/unluminous-sheet/tests/fixtures/charts/excel
param([Parameter(Mandatory = $true)][string]$Out)
$ErrorActionPreference = 'Stop'
$Out = [IO.Path]::GetFullPath($Out)
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false

# The scenario names and what each does to a column chart over A1:C6 (a heading row and two series).
$scenarios = [ordered]@{
    'secondary' = { param($c) $c.SeriesCollection(2).ChartType = 65; $c.SeriesCollection(2).AxisGroup = 2 }
    'titles' = {
        param($c)
        $c.HasTitle = $true; $c.ChartTitle.Text = 'Sales'
        $c.Axes(1).HasTitle = $true; $c.Axes(1).AxisTitle.Text = 'Month'
        $c.Axes(2).HasTitle = $true; $c.Axes(2).AxisTitle.Text = 'Units'
        $c.HasLegend = $true; $c.Legend.Position = -4160
        $c.HasDataTable = $true
    }
    'labels' = {
        param($c)
        $s = $c.SeriesCollection(1)
        $s.HasDataLabels = $true
        $s.DataLabels().ShowValue = $true; $s.DataLabels().ShowCategoryName = $true
        $s.DataLabels().Position = 2
    }
    'trend' = {
        param($c)
        $t = $c.SeriesCollection(1).Trendlines().Add(-4132); $t.DisplayEquation = $true; $t.DisplayRSquared = $true
        $t.Forward = 1; $t.Backward = 0.5
        $u = $c.SeriesCollection(2).Trendlines().Add(3); $u.Order = 3
        $v = $c.SeriesCollection(2).Trendlines().Add(6); $v.Period = 3
    }
    'errbars' = {
        param($c)
        $c.SeriesCollection(1).ErrorBar(1, 1, 1, 2) | Out-Null
        $c.SeriesCollection(2).ErrorBar(1, 3, 2, 10) | Out-Null
    }
    'axis' = {
        param($c)
        $a = $c.Axes(2)
        $a.MinimumScale = 2; $a.MaximumScale = 40; $a.MajorUnit = 5; $a.MinorUnit = 1
        $a.HasMinorGridlines = $true
        $a.TickLabels.NumberFormat = '0.0'
        $a.MajorTickMark = 3; $a.TickLabelPosition = 4
        $c.Axes(1).ReversePlotOrder = $true
    }
    'log' = { param($c) $a = $c.Axes(2); $a.ScaleType = -4133; $a.LogBase = 10 }
    'filter' = { param($c) $c.SeriesCollection(2).IsFiltered = $true }
    'layout' = { param($c) $c.ApplyLayout(3) }
    'style' = { param($c) $c.ChartStyle = 10 }
    'switch' = { param($c) $c.PlotBy = 1 }
    'blanks' = { param($c) $c.PlotVisibleOnly = $false; $c.DisplayBlanksAs = 2 }
    'bargroup' = {
        param($c)
        $g = $c.ChartGroups(1); $g.GapWidth = 80; $g.Overlap = 20
        $c.SeriesCollection(1).Format.Fill.ForeColor.RGB = 255
    }
    'pie' = {
        param($c)
        $c.ChartType = 5
        $c.ChartGroups(1).FirstSliceAngle = 90
    }
}

try {
    foreach ($name in $scenarios.Keys) {
        $book = $excel.Workbooks.Add()
        $ws = $book.Worksheets.Item(1)
        $ws.Name = 'Sheet1'
        $ws.Range('A1:C1').Value2 = [object[]]@('Cat', 'One', 'Two')
        for ($r = 2; $r -le 6; $r++) {
            $ws.Cells.Item($r, 1).Value2 = "c$($r - 1)"
            $ws.Cells.Item($r, 2).Value2 = [double](($r - 1) * 3)
            $ws.Cells.Item($r, 3).Value2 = [double](($r - 1) * 2 + 1)
        }
        $shape = $ws.Shapes.AddChart2(-1, 51, 200, 20, 400, 250)
        $shape.Chart.SetSourceData($ws.Range('A1:C6'))
        & $scenarios[$name] $shape.Chart
        $book.SaveAs((Join-Path $Out "excel-$name.xlsx"), 51)
        $book.Close($false)
        Write-Host "made $name"
    }
} finally {
    $excel.Quit()
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
}
