# Open a workbook in Excel and print, as JSON, whether Excel had to repair it and what every chart is: its
# type, title, legend, series formulas, series chart types and axis groups, data labels, trendlines, error
# bars, the axes' bounds and units, and the position. Chart sheets are included.
#
# Several files can be given; Excel is started once, because opening a workbook with many charts is slow.
# The answer is a JSON array with one object a file.
#
#   pwsh tools/excel-oracle/charts-inspect.ps1 -Path book.xlsx,other.xlsx
param([Parameter(Mandatory = $true)][string[]]$Path)
$ErrorActionPreference = 'Stop'
$Path = @($Path | ForEach-Object { $_ -split ',' })
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$excel.AutomationSecurity = 3

function Try-Get($block) { try { & $block } catch { $null } }

function Describe-Axis($chart, $type, $group) {
    $a = Try-Get { $chart.Axes($type, $group) }
    if ($null -eq $a) { return $null }
    $o = [ordered]@{}
    $o.title = Try-Get { if ($a.HasTitle) { $a.AxisTitle.Text } }
    $o.major = Try-Get { [bool]$a.HasMajorGridlines }
    $o.minor = Try-Get { [bool]$a.HasMinorGridlines }
    if ($type -eq 2) {
        $o.min = Try-Get { $a.MinimumScale }; $o.max = Try-Get { $a.MaximumScale }
        $o.minAuto = Try-Get { [bool]$a.MinimumScaleIsAuto }; $o.maxAuto = Try-Get { [bool]$a.MaximumScaleIsAuto }
        $o.majorUnit = Try-Get { $a.MajorUnit }; $o.minorUnit = Try-Get { $a.MinorUnit }
        $o.majorAuto = Try-Get { [bool]$a.MajorUnitIsAuto }
        $o.log = Try-Get { [bool]$a.ScaleType -eq $true }
        $o.logBase = Try-Get { $a.LogBase }
        $o.numberFormat = Try-Get { $a.TickLabels.NumberFormat }
    }
    $o.reverse = Try-Get { [bool]$a.ReversePlotOrder }
    $o.crosses = Try-Get { $a.Crosses }
    $o.majorTick = Try-Get { $a.MajorTickMark }
    $o.minorTick = Try-Get { $a.MinorTickMark }
    $o.labels = Try-Get { $a.TickLabelPosition }
    return $o
}

function Describe-Chart($chart, $name) {
    [Console]::Error.WriteLine("chart $name")
    $o = [ordered]@{ name = $name }
    $o.type = Try-Get { $chart.ChartType }
    $o.title = Try-Get { if ($chart.HasTitle) { $chart.ChartTitle.Text } }
    $o.legend = Try-Get { if ($chart.HasLegend) { $chart.Legend.Position } else { 'none' } }
    $o.table = Try-Get { [bool]$chart.HasDataTable }
    $o.plotVisibleOnly = Try-Get { [bool]$chart.PlotVisibleOnly }
    $o.blanks = Try-Get { $chart.DisplayBlanksAs }
    $o.style = Try-Get { $chart.ChartStyle }
    if ($o.type -lt 117) {
        $o.gapWidth = Try-Get { $chart.ChartGroups(1).GapWidth }
        $o.overlap = Try-Get { $chart.ChartGroups(1).Overlap }
        $o.firstSliceAngle = Try-Get { $chart.ChartGroups(1).FirstSliceAngle }
        $o.groups = Try-Get { $chart.ChartGroups().Count }
    }
    $series = @()
    $n = Try-Get { $chart.SeriesCollection().Count }
    # The Office 2016 chart types (117 and up) have no axes, trendlines or error bars in the object model, and
    # asking for them makes Excel drop the connection.
    $extended = $o.type -ge 117
    for ($i = 1; $i -le $n; $i++) {
        $s = $chart.SeriesCollection($i)
        $e = [ordered]@{}
        if ($extended) {
            $e.name = Try-Get { $s.Name }
            $e.type = Try-Get { $s.ChartType }
            $series += $e
            continue
        }
        $e.formula = Try-Get { $s.Formula }
        $e.type = Try-Get { $s.ChartType }
        $e.axisGroup = Try-Get { $s.AxisGroup }
        $e.labels = Try-Get { [bool]$s.HasDataLabels }
        $e.labelPos = Try-Get { if ($s.HasDataLabels) { $s.DataLabels().Position } }
        $e.labelValue = Try-Get { if ($s.HasDataLabels) { [bool]$s.DataLabels().ShowValue } }
        $e.labelCategory = Try-Get { if ($s.HasDataLabels) { [bool]$s.DataLabels().ShowCategoryName } }
        $e.labelPercent = Try-Get { if ($s.HasDataLabels) { [bool]$s.DataLabels().ShowPercentage } }
        $e.labelSeries = Try-Get { if ($s.HasDataLabels) { [bool]$s.DataLabels().ShowSeriesName } }
        $tl = Try-Get { $s.Trendlines().Count }
        $e.trendlines = @()
        for ($t = 1; $t -le $tl; $t++) {
            $tr = $s.Trendlines($t)
            $e.trendlines += [ordered]@{
                type = Try-Get { $tr.Type }; order = Try-Get { if ($tr.Type -eq 3) { $tr.Order } }
                period = Try-Get { if ($tr.Type -eq 7) { $tr.Period } }
                forward = Try-Get { $tr.Forward }; backward = Try-Get { $tr.Backward }
                eq = Try-Get { [bool]$tr.DisplayEquation }; r2 = Try-Get { [bool]$tr.DisplayRSquared }
                label = Try-Get { if ($tr.DisplayEquation -or $tr.DisplayRSquared) { $tr.DataLabel.Text } }
            }
        }
        $e.errorBars = Try-Get { [bool]$s.HasErrorBars }
        $e.explosion = Try-Get { $s.Explosion }
        $e.smooth = Try-Get { [bool]$s.Smooth }
        $e.filtered = Try-Get { [bool]$s.IsFiltered }
        $e.fill = Try-Get { $s.Format.Fill.ForeColor.RGB }
        $e.errorKind = Try-Get { if ($s.HasErrorBars) { 1 } }
        $series += $e
    }
    $o.series = $series
    if (-not $extended) {
        $o.axes = [ordered]@{
            category = Describe-Axis $chart 1 1
            value = Describe-Axis $chart 2 1
            secondaryValue = Describe-Axis $chart 2 2
        }
    }
    return $o
}

try {
    $results = @()
    foreach ($file in $Path) {
        $full = (Resolve-Path -LiteralPath $file).Path
        $repaired = $false
        $problem = ''
        try {
            $book = $excel.Workbooks.Open($full, 0, $true)
        } catch {
            $repaired = $true
            $problem = $_.Exception.Message
            $book = $excel.Workbooks.Open($full, 0, $true, 5, '', '', $true, 2, '', $false, $false, 0, $false, $false, 1)
        }
        $repaired = $repaired -or ($book.Windows.Item(1).Caption -match 'Repaired')
        $charts = @()
        foreach ($sheet in $book.Sheets) {
            $isChartSheet = $false
            try { $null = $sheet.ChartObjects; } catch { $isChartSheet = $true }
            if ($sheet.Type -eq 3) {
                $d = Describe-Chart $sheet ('sheet:' + $sheet.Name)
                $d.sheet = $sheet.Name
                $charts += $d
            } else {
                foreach ($co in $sheet.ChartObjects()) {
                    $d = Describe-Chart $co.Chart $co.Name
                    $d.sheet = $sheet.Name
                    $d.left = [math]::Round($co.Left, 1); $d.top = [math]::Round($co.Top, 1)
                    $d.width = [math]::Round($co.Width, 1); $d.height = [math]::Round($co.Height, 1)
                    $d.topLeft = $co.TopLeftCell.Address($false, $false)
                    $d.bottomRight = $co.BottomRightCell.Address($false, $false)
                    $d.alt = Try-Get { $co.ShapeRange.AlternativeText }
                    $charts += $d
                }
            }
        }
        [Console]::Error.WriteLine("closing")
        $book.Close($false)
        [Console]::Error.WriteLine("closed")
        $results += [ordered]@{ path = $full; repaired = $repaired; problem = $problem; charts = $charts }
    }
    ConvertTo-Json -InputObject @($results) -Depth 14 -Compress
} finally {
    $excel.Quit()
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
}
