# Open workbooks in Excel and write the feature specific properties, as "key = value" lines, one <name>.feat.txt per workbook.
#   pwsh inspect-features.ps1 -Paths a.xlsx,b.xlsx -OutDir <dir>
# The first line of each output is "REPAIRED = True/False" with the failure text. Compare two outputs with a text diff.
param([Parameter(Mandatory = $true)][string[]]$Paths, [Parameter(Mandatory = $true)][string]$OutDir)
$ErrorActionPreference = 'Continue'
$Paths = @($Paths | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$sig = '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);'
Add-Type -MemberDefinition $sig -Name Win -Namespace Probe -ErrorAction SilentlyContinue
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$excel.AutomationSecurity = 3
$excelPid = 0
[void][Probe.Win]::GetWindowThreadProcessId([IntPtr]$excel.Hwnd, [ref]$excelPid)

$script:lines = New-Object System.Collections.Generic.List[string]
function E([string]$key, $value) { $script:lines.Add("$key = $value") }
# Evaluate a script block, giving 'n/a' when the property does not apply or the call fails.
function S([scriptblock]$b) { try { $v = & $b; if ($null -eq $v) { return '' } ; if ($v -is [array]) { return ($v -join ',') } ; return "$v" } catch { return 'n/a' } }
function A($range) { try { $range.Address($false, $false) } catch { '' } }

function Dump-FormatConditions($ws, [string]$p) {
    $fcs = $null
    try { $fcs = $ws.UsedRange.FormatConditions } catch { return }
    $n = S { $fcs.Count }
    E "$p.CF.count" $n
    # Excel lists the rules of a sheet in an order of its own after an edit and numbers their priorities again, so the
    # rules are written sorted by what they apply to and what they hold, without the priority.
    $rules = @()
    for ($i = 1; $i -le [int]$n; $i++) {
        $mark = $script:lines.Count
        $fc = $fcs.Item($i); $k = "$p.CF[$i]"
        $type = S { $fc.Type }
        E "$k.Type" $type; E "$k.AppliesTo" (S { A $fc.AppliesTo }); E "$k.StopIfTrue" (S { $fc.StopIfTrue })
        $props = switch ($type) {
            '1' { 'Operator', 'Formula1', 'Formula2' }
            '2' { 'Formula1' }
            '5' { 'TopBottom', 'Rank', 'Percent' }
            '8' { 'DupeUnique' }
            '9' { 'Text', 'TextOperator' }
            '11' { 'DateOperator' }
            '12' { 'AboveBelow', 'NumStdDev' }
            '10' { 'Formula1' }
            '16' { 'Formula1' }
            default { @() }
        }
        foreach ($prop in $props) { E "$k.$prop" (S { $fc.$prop }) }
        if ($type -eq '6') {
            E "$k.IconSet.ID" (S { $fc.IconSet.ID }); E "$k.ReverseOrder" (S { $fc.ReverseOrder }); E "$k.ShowIconOnly" (S { $fc.ShowIconOnly }); E "$k.PercentileSet" (S { $fc.PercentileSet })
            E "$k.IconCriteria" (S { ($fc.IconCriteria | ForEach-Object { "$($_.Type):$($_.Value):$($_.Operator)" }) -join ';' })
        }
        if ($type -eq '4') {
            E "$k.DataBar.AxisPosition" (S { $fc.AxisPosition }); E "$k.DataBar.BarFillType" (S { $fc.BarFillType }); E "$k.DataBar.Direction" (S { $fc.Direction })
            E "$k.DataBar.Min" (S { "$($fc.MinPoint.Type):$($fc.MinPoint.Value)" }); E "$k.DataBar.Max" (S { "$($fc.MaxPoint.Type):$($fc.MaxPoint.Value)" })
            E "$k.DataBar.BarColor" (S { $fc.BarColor.Color }); E "$k.DataBar.NegColor" (S { "$($fc.NegativeBarFormat.ColorType)" }); E "$k.DataBar.Border" (S { $fc.BarBorder.Type })
            E "$k.DataBar.AxisColor" (S { $fc.AxisColor.Color }); E "$k.DataBar.ShowValue" (S { $fc.ShowValue })
        }
        if ($type -eq '3') { E "$k.ColorScale" (S { ($fc.ColorScaleCriteria | ForEach-Object { "$($_.Type):$($_.Value):$($_.FormatColor.Color)" }) -join ';' }) }
        if ($type -notin '3', '4', '6') { E "$k.Fill" (S { $fc.Interior.Color }); E "$k.FontBold" (S { $fc.Font.Bold }); E "$k.FontItalic" (S { $fc.Font.Italic }); E "$k.FontColor" (S { $fc.Font.Color }) }
        $chunk = @($script:lines.GetRange($mark, $script:lines.Count - $mark) | ForEach-Object { $_.Substring($k.Length) })
        $script:lines.RemoveRange($mark, $script:lines.Count - $mark)
        $rules += [pscustomobject]@{ key = (($chunk | Sort-Object) -join "`n"); lines = $chunk }
    }
    $order = 0
    foreach ($rule in ($rules | Sort-Object { ($_.lines | Where-Object { $_ -like ".AppliesTo*" }) }, key)) { $order++; foreach ($line in $rule.lines) { $script:lines.Add("$p.CF[$order]$line") } }
}

function Dump-Validation($ws, [string]$p) {
    $cells = $null
    try { $cells = $ws.Cells.SpecialCells(-4174) } catch { E "$p.DV.count" 0; return }
    $n = 0
    foreach ($c in $cells.Cells) {
        $n++; if ($n -gt 60) { break }
        $v = $c.Validation; $k = "$p.DV[$(A $c)]"
        foreach ($prop in 'Type', 'AlertStyle', 'Operator', 'Formula1', 'Formula2', 'InputTitle', 'InputMessage', 'ErrorTitle', 'ErrorMessage', 'IgnoreBlank', 'InCellDropdown', 'ShowInput', 'ShowError') { E "$k.$prop" (S { $v.$prop }) }
    }
    E "$p.DV.count" $n
}

function Dump-Cells($ws, [string]$p) {
    $used = $ws.UsedRange
    E "$p.UsedRange" (A $used)
    $count = 0
    foreach ($c in $used.Cells) {
        $count++; if ($count -gt 400) { break }
        $hasAny = ($null -ne $c.Value2) -or $c.HasFormula -or ($c.Interior.Pattern -ne -4142) -or ($c.Borders.Item(9).LineStyle -ne -4142) -or ($c.Style.Name -ne 'Normal')
        if (-not $hasAny) { continue }
        $k = "$p.Cell[$(A $c)]"
        E "$k.Style" (S { $c.Style.Name }); E "$k.NumFmt" (S { $c.NumberFormat })
        E "$k.Pattern" (S { $c.Interior.Pattern }); E "$k.Color" (S { $c.Interior.Color }); E "$k.Theme" (S { "$($c.Interior.ThemeColor):$([math]::Round($c.Interior.TintAndShade,3))" })
        if ((S { $c.Interior.Pattern }) -eq '4000') { E "$k.Gradient" (S { "$($c.Interior.Gradient.Degree):" + (($c.Interior.Gradient.ColorStops | ForEach-Object { "$($_.Position)=$($_.Color)" }) -join ';') }) }
        E "$k.PatternColor" (S { $c.Interior.PatternColor })
        foreach ($b in 5, 6, 7, 8, 9, 10) { $ls = S { $c.Borders.Item($b).LineStyle }; if ($ls -ne '-4142') { E "$k.Border$b" (S { "$($c.Borders.Item($b).LineStyle):$($c.Borders.Item($b).Weight):$($c.Borders.Item($b).Color)" }) } }
        E "$k.Orient" (S { $c.Orientation }); E "$k.Indent" (S { $c.IndentLevel }); E "$k.Shrink" (S { $c.ShrinkToFit }); E "$k.HAlign" (S { $c.HorizontalAlignment }); E "$k.VAlign" (S { $c.VerticalAlignment }); E "$k.Wrap" (S { $c.WrapText }); E "$k.ReadOrder" (S { $c.ReadingOrder })
        E "$k.Font" (S { "$($c.Font.Name)|$($c.Font.Size)|B$($c.Font.Bold)|I$($c.Font.Italic)|U$($c.Font.Underline)|S$($c.Font.Strikethrough)|C$($c.Font.Color)" })
        E "$k.Locked" (S { "$($c.Locked)|hidden$($c.FormulaHidden)" })
        $bold = S { $c.Font.Bold }; $col = S { $c.Font.Color }; $sup = S { $c.Font.Superscript }
        if (($bold -eq '') -or ($col -eq '') -or ($sup -eq '') -or ($bold -eq 'n/a')) {
            $txt = S { $c.Value2 }
            if ($txt.Length -gt 0 -and $txt.Length -le 60) {
                $runs = @(); $prev = ''
                for ($i = 1; $i -le $txt.Length; $i++) {
                    $f = $c.Characters($i, 1).Font; $sig = "B$($f.Bold)I$($f.Italic)C$($f.Color)Sup$($f.Superscript)"
                    if ($sig -ne $prev) { $runs += "$i`:$sig"; $prev = $sig }
                }
                E "$k.Runs" ($runs -join ' ')
            }
        }
    }
}

function Dump-Sheet($book, $ws, [string]$p) {
    Write-Host "  sheet $p"
    E "$p.Visible" (S { $ws.Visible }); E "$p.TabColor" (S { "$($ws.Tab.Color):$($ws.Tab.ThemeColor):$($ws.Tab.TintAndShade)" })
    E "$p.Protect" (S { "contents=$($ws.ProtectContents) drawing=$($ws.ProtectDrawingObjects) scenarios=$($ws.ProtectScenarios) uionly=$($ws.ProtectionMode) select=$($ws.EnableSelection)" })
    foreach ($f in 'AllowFormattingCells', 'AllowFormattingColumns', 'AllowFormattingRows', 'AllowInsertingColumns', 'AllowInsertingRows', 'AllowInsertingHyperlinks', 'AllowDeletingColumns', 'AllowDeletingRows', 'AllowSorting', 'AllowFiltering', 'AllowUsingPivotTables') { E "$p.Protection.$f" (S { $ws.Protection.$f }) }
    if ($ws.Visible -eq -1) {
        try {
            [void]$ws.Activate(); $w = $excel.ActiveWindow
            foreach ($f in 'FreezePanes', 'Split', 'SplitRow', 'SplitColumn', 'SplitHorizontal', 'SplitVertical', 'ScrollRow', 'ScrollColumn', 'DisplayHeadings', 'DisplayRightToLeft', 'DisplayGridlines', 'View', 'Zoom', 'DisplayZeros', 'DisplayOutline') { E "$p.Window.$f" (S { $w.$f }) }
            E "$p.Window.ActiveCell" (S { A $excel.ActiveCell }); E "$p.Window.Selection" (S { A $excel.Selection })
            E "$p.Window.Panes" (S { $w.Panes.Count })
            for ($i = 1; $i -le [int](S { $w.Panes.Count }); $i++) { E "$p.Window.Pane$i" (S { "$($w.Panes.Item($i).ScrollRow),$($w.Panes.Item($i).ScrollColumn)" }) }
        } catch { E "$p.Window" "activate failed" }
    }
    Write-Host '   pagesetup'
    # Page setup
    $ps = $null; try { $ps = $ws.PageSetup } catch { }
    if ($ps) {
        foreach ($f in 'Orientation', 'PaperSize', 'LeftMargin', 'RightMargin', 'TopMargin', 'BottomMargin', 'HeaderMargin', 'FooterMargin', 'Zoom', 'FitToPagesWide', 'FitToPagesTall', 'PrintArea', 'PrintTitleRows', 'PrintTitleColumns', 'LeftHeader', 'CenterHeader', 'RightHeader', 'LeftFooter', 'CenterFooter', 'RightFooter', 'PrintGridlines', 'PrintHeadings', 'CenterHorizontally', 'CenterVertically', 'FirstPageNumber', 'Order', 'BlackAndWhite', 'Draft', 'PrintComments', 'PrintErrors', 'DifferentFirstPageHeaderFooter', 'OddAndEvenPagesHeaderFooter') {
            $v = S { $ps.$f }; if ($f -like '*Margin') { $v = S { [math]::Round($ps.$f, 2) } }; E "$p.PS.$f" $v
        }
    }
    E "$p.HPageBreaks" (S { $ws.HPageBreaks.Count }); E "$p.VPageBreaks" (S { $ws.VPageBreaks.Count })
    for ($i = 1; $i -le [int](S { $ws.HPageBreaks.Count }); $i++) { E "$p.HBreak$i" (S { "$($ws.HPageBreaks.Item($i).Type):$(A $ws.HPageBreaks.Item($i).Location)" }) }
    for ($i = 1; $i -le [int](S { $ws.VPageBreaks.Count }); $i++) { E "$p.VBreak$i" (S { "$($ws.VPageBreaks.Item($i).Type):$(A $ws.VPageBreaks.Item($i).Location)" }) }
    Write-Host '   outline'
    # Outline
    E "$p.Outline.Summary" (S { "row=$($ws.Outline.SummaryRow) col=$($ws.Outline.SummaryColumn)" })
    $used = $ws.UsedRange; $lastR = [math]::Min($used.Row + $used.Rows.Count - 1, 100); $lastC = [math]::Min($used.Column + $used.Columns.Count - 1, 40)
    for ($r = 1; $r -le $lastR; $r++) { $row = $ws.Rows.Item($r); $lv = $row.OutlineLevel; if ($lv -gt 1 -or $row.Hidden) { E "$p.Row[$r]" "level=$lv hidden=$($row.Hidden) showdetail=$(S { $row.ShowDetail })" } }
    for ($c = 1; $c -le $lastC; $c++) { $col = $ws.Columns.Item($c); $lv = $col.OutlineLevel; if ($lv -gt 1 -or $col.Hidden) { E "$p.Col[$c]" "level=$lv hidden=$($col.Hidden) showdetail=$(S { $col.ShowDetail })" } }
    Write-Host '   cf'
    Dump-FormatConditions $ws $p
    Write-Host '   dv'
    Dump-Validation $ws $p
    Write-Host '   tables'
    # Tables
    foreach ($lo in $ws.ListObjects) {
        $k = "$p.Table[$($lo.Name)]"
        E "$k.Range" (A $lo.Range); E "$k.ShowTotals" $lo.ShowTotals; E "$k.ShowHeaders" $lo.ShowHeaders; E "$k.Style" (S { $lo.TableStyle.Name })
        E "$k.Stripes" (S { "row=$($lo.ShowTableStyleRowStripes) col=$($lo.ShowTableStyleColumnStripes) first=$($lo.ShowTableStyleFirstColumn) last=$($lo.ShowTableStyleLastColumn)" })
        foreach ($lc in $lo.ListColumns) { E "$k.Col[$($lc.Index)]" (S { "$($lc.Name) totals=$($lc.TotalsCalculation)" }) }
        E "$k.DataBodyRange" (S { A $lo.DataBodyRange }); E "$k.AutoFilter" (S { $lo.ShowAutoFilter })
        E "$k.CalcColumnFormula" (S { $lo.ListColumns.Item($lo.ListColumns.Count).DataBodyRange.Cells.Item(1).Formula })
    }
    # AutoFilter and sort
    E "$p.FilterMode" (S { $ws.FilterMode }); E "$p.AutoFilterOn" (S { $ws.AutoFilterMode })
    if ($ws.AutoFilterMode) {
        E "$p.AutoFilter.Range" (S { A $ws.AutoFilter.Range })
        for ($i = 1; $i -le [int](S { $ws.AutoFilter.Filters.Count }); $i++) {
            $f = $ws.AutoFilter.Filters.Item($i)
            if ($f.On) { E "$p.AutoFilter.Filter[$i]" (S { "op=$($f.Operator) c1=$($f.Criteria1) c2=$($f.Criteria2)" }) }
        }
        $sf = S { $ws.AutoFilter.Sort.SortFields.Count }
        E "$p.AutoFilter.Sort.count" $sf
        for ($i = 1; $i -le [int]$sf; $i++) { E "$p.AutoFilter.Sort[$i]" (S { $x = $ws.AutoFilter.Sort.SortFields.Item($i); "key=$(A $x.Key) on=$($x.SortOn) order=$($x.Order)" }) }
        E "$p.AutoFilter.Sort.range" (S { A $ws.AutoFilter.Sort.Rng })
    }
    E "$p.SortFieldsOnSheet" (S { $ws.Sort.SortFields.Count })
    Write-Host '   comments'
    # Comments
    E "$p.Comments" (S { $ws.Comments.Count }); E "$p.CommentsThreaded" (S { $ws.CommentsThreaded.Count })
    foreach ($c in $ws.Comments) { E "$p.Note[$(A $c.Parent)]" (S { "$($c.Author)|$($c.Text())|visible=$($c.Visible)" }) }
    foreach ($c in $ws.CommentsThreaded) { E "$p.Thread[$(A $c.Parent)]" (S { "$($c.Author.Name)|$($c.Text)|replies=$($c.Replies.Count)|" + (($c.Replies | ForEach-Object { $_.Text }) -join '/') }) }
    # Hyperlinks
    $hn = 0; foreach ($h in $ws.Hyperlinks) { $hn++; E "$p.Link[$hn]" (S { "$(A $h.Range)|addr=$($h.Address -replace '^(\.\.[\\/])+', '../*/')|sub=$($h.SubAddress)|tip=$($h.ScreenTip)|text=$($h.TextToDisplay)" }) }
    E "$p.Links" $hn
    Write-Host '   charts'
    # Charts and shapes
    E "$p.ChartObjects" (S { $ws.ChartObjects().Count })
    foreach ($co in $ws.ChartObjects()) {
        $ch = $co.Chart; $k = "$p.Chart[$($co.Name)]"
        E "$k.Type" (S { $ch.ChartType }); E "$k.HasTitle" (S { $ch.HasTitle }); E "$k.Title" (S { if ($ch.HasTitle) { $ch.ChartTitle.Text } }); E "$k.HasLegend" (S { $ch.HasLegend })
        E "$k.Anchor" (S { "$(A $co.TopLeftCell):$(A $co.BottomRightCell)" })
        E "$k.Series" (S { $ch.SeriesCollection().Count })
        for ($i = 1; $i -le [int](S { $ch.SeriesCollection().Count }); $i++) {
            $se = $ch.SeriesCollection($i)
            E "$k.Series[$i]" (S { "$($se.Formula)|type=$($se.ChartType)|axis=$($se.AxisGroup)|labels=$($se.HasDataLabels)|trend=$($se.Trendlines().Count)|errbars=$($se.HasErrorBars)" })
        }
        E "$k.AxisTitles" (S { "x=$($ch.Axes(1).HasTitle):$(if($ch.Axes(1).HasTitle){$ch.Axes(1).AxisTitle.Text}) y=$($ch.Axes(2).HasTitle):$(if($ch.Axes(2).HasTitle){$ch.Axes(2).AxisTitle.Text})" })
        E "$k.PivotChart" (S { $ch.PivotLayout.PivotTable.Name })
    }
    E "$p.Shapes" (S { $ws.Shapes.Count })
    foreach ($sh in $ws.Shapes) { E "$p.Shape[$($sh.Name)]" (S { "type=$($sh.Type) auto=$($sh.AutoShapeType) at=$(A $sh.TopLeftCell) size=$([math]::Round($sh.Width)),$([math]::Round($sh.Height)) text=$(if ($sh.HasTextFrame) { $sh.TextFrame2.TextRange.Text }) form=$($sh.FormControlType)" }) }
    # Sparklines
    $sgc = S { $ws.Cells.SparklineGroups.Count }
    E "$p.SparklineGroups" $sgc
    # Excel lists the groups in an order that changes after an edit, so they are written sorted by where they are.
    $groups = @(); for ($i = 1; $i -le [int]$sgc; $i++) { $g = $ws.Cells.SparklineGroups.Item($i); $groups += (S { "loc=$(A $g.Location) type=$($g.Type) source=$($g.SourceData) color=$($g.SeriesColor.Color) markers=$($g.Points.Markers.Visible) neg=$($g.Points.Negative.Visible) count=$($g.Count)" }) }
    $order = 0; foreach ($g in ($groups | Sort-Object)) { $order++; E "$p.Sparkline[$order]" $g }
    # Pivot tables
    E "$p.PivotTables" (S { $ws.PivotTables().Count })
    foreach ($pt in $ws.PivotTables()) { E "$p.Pivot[$($pt.Name)]" (S { "range=$(A $pt.TableRange1) source=$($pt.SourceData) rows=$(($pt.RowFields | ForEach-Object { $_.Name }) -join ',') data=$(($pt.DataFields | ForEach-Object { $_.Name }) -join ',')" }) }
    Write-Host '   cells'
    Dump-Cells $ws $p
}

foreach ($path in $Paths) {
    $full = (Resolve-Path -LiteralPath $path).Path
    $script:lines.Clear()
    $book = $null; $repaired = $false; $problem = ''
    try {
        $book = $excel.Workbooks.Open($full, 0, $true)
    } catch {
        $repaired = $true; $problem = $_.Exception.Message
        try { $book = $excel.Workbooks.Open($full, 0, $true, 5, '', '', $true, 2, '', $false, $false, 0, $false, $false, 1) } catch { $problem += " | second open failed: $($_.Exception.Message)" }
    }
    if ($book) {
        # Reading PageSetup talks to the active printer; the default (a network printer that is offline) blocks Excel, so the PDF printer is made active for this Excel only.
        try { $excel.ActivePrinter = 'Microsoft Print to PDF on Ne00:' } catch { }
        try { $repaired = $repaired -or ($book.Windows.Item(1).Caption -match 'Repaired') } catch { }
        E 'REPAIRED' "$repaired $problem"
        E 'WB.Calculation' (S { $book.Calculation }); E 'WB.Iteration' (S { $excel.Iteration }); E 'WB.MaxIterations' (S { $excel.MaxIterations }); E 'WB.MaxChange' (S { $excel.MaxChange })
        E 'WB.Date1904' (S { $book.Date1904 }); E 'WB.PrecisionAsDisplayed' (S { $book.PrecisionAsDisplayed }); E 'WB.HasVBProject' (S { $book.HasVBProject })
        E 'WB.ProtectStructure' (S { $book.ProtectStructure }); E 'WB.ProtectWindows' (S { $book.ProtectWindows })
        E 'WB.Sheets' (S { $book.Sheets.Count }); E 'WB.Worksheets' (S { $book.Worksheets.Count }); E 'WB.ChartSheets' (S { $book.Charts.Count })
        E 'WB.SlicerCaches' (S { $book.SlicerCaches.Count })
        foreach ($sc in $book.SlicerCaches) { E "WB.SlicerCache[$($sc.Name)]" (S { "source=$($sc.SourceName) slicers=$($sc.Slicers.Count) " + (($sc.Slicers | ForEach-Object { "$($_.Name)/$($_.Caption)/$(A $_.Shape.TopLeftCell)" }) -join ';') }) }
        E 'WB.PivotCaches' (S { $book.PivotCaches().Count })
        foreach ($f in 'Title', 'Author', 'Subject', 'Company', 'Keywords') { E "WB.Prop.$f" (S { [System.__ComObject].InvokeMember('Value', 'GetProperty', $null, ([System.__ComObject].InvokeMember('Item', 'GetProperty', $null, $book.BuiltinDocumentProperties, @($f))), $null) }) }
        $cp = $book.CustomDocumentProperties
        $cn = S { [System.__ComObject].InvokeMember('Count', 'GetProperty', $null, $cp, $null) }
        E 'WB.CustomProps.count' $cn
        for ($i = 1; $i -le [int]$cn; $i++) { E "WB.CustomProp[$i]" (S { $q = [System.__ComObject].InvokeMember('Item', 'GetProperty', $null, $cp, @($i)); "$([System.__ComObject].InvokeMember('Name', 'GetProperty', $null, $q, $null))=$([System.__ComObject].InvokeMember('Value', 'GetProperty', $null, $q, $null)) type=$([System.__ComObject].InvokeMember('Type', 'GetProperty', $null, $q, $null))" }) }
        foreach ($n in $book.Names) { E "WB.Name[$($n.Name)]" (S { "refers=$($n.RefersTo)|visible=$($n.Visible)|comment=$($n.Comment)" }) }
        foreach ($sh in $book.Sheets) {
            $p = "Sheet[$($sh.Name)]"
            E "$p.Type" (S { $sh.Type })
            if ($sh.Type -eq -4167) { Dump-Sheet $book $sh $p }
            else { E "$p.ChartType" (S { $sh.ChartType }); E "$p.HasTitle" (S { $sh.HasTitle }); E "$p.Series" (S { $sh.SeriesCollection().Count }) }
        }
        try { $book.Close($false) } catch { }
    } else { E 'REPAIRED' "open failed $problem" }
    $out = Join-Path $OutDir ([IO.Path]::GetFileNameWithoutExtension($full) + '.feat.txt')
    $script:lines | Set-Content -LiteralPath $out -Encoding UTF8
    Write-Host "wrote $out ($($script:lines.Count) lines)"
}
try { $excel.Quit() } catch { }
[void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
Start-Sleep 2
if ($excelPid -gt 0) { $left = Get-Process -Id $excelPid -ErrorAction SilentlyContinue; if ($left) { Stop-Process -Id $excelPid -Force; Write-Host "killed leftover $excelPid" } }
