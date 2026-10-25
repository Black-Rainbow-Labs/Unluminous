# For each chart type and each quick layout number, apply the layout in Excel and print which chart elements
# result: title, legend and its place, data table, data labels and what they show, the axes, their titles and
# gridlines. The output is JSON lines, and is how src/chartlayout.rs was written.
#
#   pwsh tools/excel-oracle/charts-layouts.ps1 > layouts.jsonl
param([int[]]$Types = @(51, 52, 57, 4, 65, 5, 1, -4169, -4151, 76))
$ErrorActionPreference = 'Stop'
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $book = $excel.Workbooks.Add()
    $ws = $book.Worksheets.Item(1)
    $ws.Range('A1:C1').Value2 = [object[]]@('Cat', 'One', 'Two')
    for ($r = 2; $r -le 6; $r++) {
        $ws.Cells.Item($r, 1).Value2 = "c$r"
        $ws.Cells.Item($r, 2).Value2 = [double]($r * 3)
        $ws.Cells.Item($r, 3).Value2 = [double]($r * 2 + 1)
    }
    function Probe($v, $block) { try { & $block } catch { $null } }
    foreach ($type in $Types) {
        for ($layout = 1; $layout -le 14; $layout++) {
            $shape = $ws.Shapes.AddChart2(-1, $type, 200, 20, 400, 250)
            $c = $shape.Chart
            $c.SetSourceData($ws.Range('A1:C6'))
            $ok = $true
            try { $c.ApplyLayout($layout) } catch { $ok = $false }
            if (-not $ok) { $shape.Delete(); continue }
            $o = [ordered]@{ type = $type; layout = $layout }
            $o.title = [bool]$c.HasTitle
            $o.titleOverlay = Probe 0 { if ($c.HasTitle) { [bool]$c.ChartTitle.IncludeInLayout -eq $false } }
            $o.legend = [bool]$c.HasLegend
            $o.legendPos = Probe 0 { if ($c.HasLegend) { $c.Legend.Position } }
            $o.table = [bool]$c.HasDataTable
            $o.labels = Probe 0 { [bool]$c.SeriesCollection(1).HasDataLabels }
            $o.labelPos = Probe 0 { if ($c.SeriesCollection(1).HasDataLabels) { $c.SeriesCollection(1).DataLabels().Position } }
            $o.labelValue = Probe 0 { if ($c.SeriesCollection(1).HasDataLabels) { [bool]$c.SeriesCollection(1).DataLabels().ShowValue } }
            $o.labelCat = Probe 0 { if ($c.SeriesCollection(1).HasDataLabels) { [bool]$c.SeriesCollection(1).DataLabels().ShowCategoryName } }
            $o.labelPct = Probe 0 { if ($c.SeriesCollection(1).HasDataLabels) { [bool]$c.SeriesCollection(1).DataLabels().ShowPercentage } }
            $o.labelSer = Probe 0 { if ($c.SeriesCollection(1).HasDataLabels) { [bool]$c.SeriesCollection(1).DataLabels().ShowSeriesName } }
            foreach ($pair in @(@('cat', 1, 1), @('val', 2, 1))) {
                $name = $pair[0]; $t = $pair[1]
                $o["${name}Axis"] = Probe 0 { [bool]$c.HasAxis($t) }
                $o["${name}Title"] = Probe 0 { [bool]$c.Axes($t).HasTitle }
                $o["${name}Major"] = Probe 0 { [bool]$c.Axes($t).HasMajorGridlines }
                $o["${name}Minor"] = Probe 0 { [bool]$c.Axes($t).HasMinorGridlines }
            }
            $o.trend = Probe 0 { $c.SeriesCollection(1).Trendlines().Count }
            $o.errbars = Probe 0 { [bool]$c.SeriesCollection(1).HasErrorBars }
            $o.upDown = Probe 0 { [bool]$c.ChartGroups(1).HasUpDownBars }
            $o.dropLines = Probe 0 { [bool]$c.ChartGroups(1).HasDropLines }
            $o.style = Probe 0 { $c.ChartStyle }
            $o | ConvertTo-Json -Compress
            $shape.Delete()
        }
    }
    $book.Close($false)
} finally {
    $excel.Quit()
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
}
