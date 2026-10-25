# Open workbooks in Excel and report, for each, whether it had to repair them and what it sees of their drawing
# objects and sparklines: one JSON object a line.
#
#   pwsh tools/excel-oracle/objects-inspect.ps1 -Paths a.xlsx,b.xlsx
#
# Positions are in points as Excel gives them. Every property is read through a guard, so one that does not apply
# to a kind of object comes back as "n/a" and does not stop the rest.
param([Parameter(Mandatory = $true)][string[]]$Paths)
$ErrorActionPreference = 'Continue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$Paths = @($Paths | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$sig = '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);'
Add-Type -MemberDefinition $sig -Name Win -Namespace ObjProbe -ErrorAction SilentlyContinue
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$excel.AutomationSecurity = 3
$excelPid = 0
[void][ObjProbe.Win]::GetWindowThreadProcessId([IntPtr]$excel.Hwnd, [ref]$excelPid)

function S([scriptblock]$b) { try { $v = & $b; if ($null -eq $v) { return '' }; return $v } catch { return 'n/a' } }
function A($range) { try { $range.Address($false, $false) } catch { '' } }

function Describe-Shape($sh) {
    $o = [ordered]@{
        name = S { $sh.Name }; id = S { $sh.ID }; type = S { [int]$sh.Type }; auto = S { [int]$sh.AutoShapeType }
        left = S { [math]::Round($sh.Left, 2) }; top = S { [math]::Round($sh.Top, 2) }
        width = S { [math]::Round($sh.Width, 2) }; height = S { [math]::Round($sh.Height, 2) }
        rotation = S { [math]::Round($sh.Rotation, 2) }; alt = S { $sh.AlternativeText }; z = S { $sh.ZOrderPosition }
        lockAspect = S { $sh.LockAspectRatio }; placement = S { [int]$sh.Placement }; hflip = S { $sh.HorizontalFlip }; vflip = S { $sh.VerticalFlip }
        fillVisible = S { $sh.Fill.Visible }; fillRgb = S { $sh.Fill.ForeColor.RGB }; fillTransparency = S { [math]::Round($sh.Fill.Transparency, 3) }
        lineVisible = S { $sh.Line.Visible }; lineWeight = S { $sh.Line.Weight }; lineRgb = S { $sh.Line.ForeColor.RGB }
        lineDash = S { [int]$sh.Line.DashStyle }; endArrow = S { [int]$sh.Line.EndArrowheadStyle }; beginArrow = S { [int]$sh.Line.BeginArrowheadStyle }
        text = S { $sh.TextFrame2.TextRange.Text }
        fontSize = S { $sh.TextFrame2.TextRange.Font.Size }
        bold = S { $sh.TextFrame2.TextRange.Font.Bold }
        italic = S { $sh.TextFrame2.TextRange.Font.Italic }
        fontRgb = S { $sh.TextFrame2.TextRange.Font.Fill.ForeColor.RGB }
        halign = S { [int]$sh.TextFrame2.TextRange.ParagraphFormat.Alignment }
        valign = S { [int]$sh.TextFrame2.VerticalAnchor }
        topLeft = S { A $sh.TopLeftCell }; bottomRight = S { A $sh.BottomRightCell }
        cropLeft = S { [math]::Round($sh.PictureFormat.CropLeft, 2) }; cropTop = S { [math]::Round($sh.PictureFormat.CropTop, 2) }
        cropRight = S { [math]::Round($sh.PictureFormat.CropRight, 2) }; cropBottom = S { [math]::Round($sh.PictureFormat.CropBottom, 2) }
        items = S { if ([int]$sh.Type -eq 6) { ($sh.GroupItems | ForEach-Object { $_.Name }) -join ',' } }
    }
    return $o
}

function Describe-Group($g) {
    return [ordered]@{
        type = S { [int]$g.Type }; source = S { $g.SourceData }; location = S { A $g.Location }; count = S { $g.Count }
        seriesColor = S { $g.SeriesColor.Color }; negativeColor = S { $g.Points.Negative.Color.Color }
        markersColor = S { $g.Points.Markers.Color.Color }; highColor = S { $g.Points.Highpoint.Color.Color }
        lowColor = S { $g.Points.Lowpoint.Color.Color }; firstColor = S { $g.Points.Firstpoint.Color.Color }; lastColor = S { $g.Points.Lastpoint.Color.Color }
        markers = S { $g.Points.Markers.Visible }; high = S { $g.Points.Highpoint.Visible }; low = S { $g.Points.Lowpoint.Visible }
        first = S { $g.Points.Firstpoint.Visible }; last = S { $g.Points.Lastpoint.Visible }; negative = S { $g.Points.Negative.Visible }
        weight = S { $g.LineWeight }; displayBlanksAs = S { [int]$g.DisplayBlanksAs }; displayHidden = S { $g.DisplayHidden }
        horizontalVisible = S { $g.Axes.Horizontal.Axis.Visible }; dateAxis = S { $g.Axes.Horizontal.IsDateAxis }
        rightToLeft = S { $g.Axes.Horizontal.PlotRightToLeft }
        minType = S { [int]$g.Axes.Vertical.MinScaleType }; maxType = S { [int]$g.Axes.Vertical.MaxScaleType }
        minCustom = S { $g.Axes.Vertical.CustomMinScaleValue }; maxCustom = S { $g.Axes.Vertical.CustomMaxScaleValue }
        locations = S { ($g | ForEach-Object { A $_.Location }) -join ',' }
        each = S { @(for ($i = 1; $i -le $g.Count; $i++) { $s = $g.Item($i); "$(A $s.Location)=$($s.SourceData)" }) -join ';' }
    }
}

foreach ($path in $Paths) {
    $full = (Resolve-Path -LiteralPath $path).Path
    $started = Get-Date
    $repaired = $false; $problem = ''; $book = $null
    try { $book = $excel.Workbooks.Open($full, 0, $true) } catch {
        $repaired = $true; $problem = $_.Exception.Message
        try { $book = $excel.Workbooks.Open($full, 0, $true, 5, '', '', $true, 2, '', $false, $false, 0, $false, $false, 1) } catch { $problem += ' | second open failed' }
    }
    $result = [ordered]@{ file = (Split-Path $full -Leaf); repaired = $repaired; problem = $problem; sheets = @() }
    if ($book) {
        foreach ($ws in $book.Worksheets) {
            $shapes = @(); foreach ($sh in $ws.Shapes) { $shapes += , (Describe-Shape $sh) }
            $groups = @(); $n = S { $ws.Cells.SparklineGroups.Count }
            for ($i = 1; $i -le [int]$n; $i++) { $groups += , (Describe-Group $ws.Cells.SparklineGroups.Item($i)) }
            $result.sheets += , ([ordered]@{ name = $ws.Name; shapes = $shapes; sparklines = $groups })
        }
        try { $book.Close($false) } catch { }
    }
    $result | ConvertTo-Json -Depth 8 -Compress
}
try { $excel.Quit() } catch { }
[void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
Start-Sleep 1
if ($excelPid -gt 0) { $left = Get-Process -Id $excelPid -ErrorAction SilentlyContinue; if ($left) { Stop-Process -Id $excelPid -Force } }
