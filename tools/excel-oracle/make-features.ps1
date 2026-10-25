# Build one small workbook per feature family with Excel's object model (task-2357 file probe, task-2360 fixtures).
#   pwsh tools/excel-oracle/make-features.ps1 [-Folder <folder>] [-Only name1,name2]
# The default folder is crates/unluminous-sheet/tests/fixtures/features. Each build runs in its own try/catch. A feature
# the object model refuses is written to build-log.txt beside the folder and skipped. 01b-views-pagebreakpreview.xlsx is
# 01-views.xlsx with the view of sheet V3 changed to page break preview by editing its XML, because a hidden Excel
# cannot switch a window to that view.
param([string]$Folder = (Join-Path $PSScriptRoot '../../crates/unluminous-sheet/tests/fixtures/features'), [string[]]$Only = @())
$ErrorActionPreference = 'Stop'
$Only = @($Only | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
New-Item -ItemType Directory -Force -Path $Folder | Out-Null
$Folder = (Resolve-Path -LiteralPath $Folder).Path
$logPath = Join-Path $env:TEMP 'make-features-log.txt'
$m = [Type]::Missing
$sig = '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);'
Add-Type -MemberDefinition $sig -Name Win -Namespace Probe -ErrorAction SilentlyContinue

$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$excelPid = 0
[void][Probe.Win]::GetWindowThreadProcessId([IntPtr]$excel.Hwnd, [ref]$excelPid)
"excel pid $excelPid started $(Get-Date -Format s)" | Set-Content -LiteralPath $logPath

function Log([string]$text) { $text | Add-Content -LiteralPath $logPath; Write-Host $text }

# One call site for every cell write, so the COM binder keeps no type from an earlier value.
function Put($ws, [string]$addr, $value) { $ws.Range($addr).Formula = [string]$value }
function Rows($ws, [string]$topLeft, $rows) {
    $r0 = $ws.Range($topLeft).Row; $c0 = $ws.Range($topLeft).Column
    for ($i = 0; $i -lt $rows.Count; $i++) { for ($j = 0; $j -lt $rows[$i].Count; $j++) { $ws.Cells.Item($r0 + $i, $c0 + $j).Formula = [string]$rows[$i][$j] } }
}
$people = @(@('Item', 'Region', 'Units', 'Price'), @('Apples', 'North', 120, 0.5), @('Pears', 'South', 80, 0.75), @('Plums', 'North', 45, 1.2), @('Figs', 'East', 30, 2.5), @('Kiwis', 'South', 200, 0.4), @('Limes', 'East', 150, 0.3), @('Dates', 'West', 15, 3.1), @('Mango', 'South', 60, 1.9))

function Build([string]$name, [scriptblock]$body, [string]$ext = 'xlsx') {
    if ($Only.Count -gt 0 -and $Only -notcontains $name) { return }
    $path = Join-Path $Folder "$name.$ext"
    $book = $null
    try {
        $book = $excel.Workbooks.Add()
        try { $excel.ActivePrinter = 'Microsoft Print to PDF on Ne00:' } catch { Log "  no printer set: $($_.Exception.Message)" }
        $excel.Calculation = -4105
        $excel.Iteration = $false
        & $body $book
        if ($ext -eq 'xlsm') { $book.SaveAs($path, 52) } else { $book.SaveAs($path, 51) }
        Log "OK   $name.$ext"
    } catch {
        Log "FAIL $name.$ext : $($_.Exception.Message)"
    } finally {
        if ($book) { try { $book.Close($false) } catch { } }
    }
}
# Run one step of a build; a refusal is logged and the build goes on.
function Try-Step([string]$label, [scriptblock]$step) {
    try { & $step } catch { Log "  refused: $label : $($_.Exception.Message)" }
}

# ---- 01 sheet views ----
Build '01-views' {
    param($book)
    while ($book.Worksheets.Count -lt 6) { [void]$book.Worksheets.Add($m, $book.Worksheets.Item($book.Worksheets.Count)) }
    for ($i = 1; $i -le 6; $i++) { $ws = $book.Worksheets.Item($i); $ws.Name = "V$i"; Rows $ws 'A1' $people; Rows $ws 'A12' $people }
    $s = $book.Worksheets.Item(1); [void]$s.Activate()
    $w = $excel.ActiveWindow
    $w.FreezePanes = $false; $w.SplitRow = 3; $w.SplitColumn = 2
    $w.ScrollRow = 5; $w.ScrollColumn = 2
    $s.Range('C8:D9').Select(); $s.Range('C8').Activate()
    Try-Step 'zoom 150' { $w.Zoom = 150 }
    $s = $book.Worksheets.Item(2); [void]$s.Activate(); $w = $excel.ActiveWindow
    $w.DisplayHeadings = $false
    Try-Step 'rtl' { $w.DisplayRightToLeft = $true }
    $w.DisplayGridlines = $false
    $s = $book.Worksheets.Item(3); [void]$s.Activate(); $w = $excel.ActiveWindow
    # Window.View = 2 (page break preview) hangs a hidden Excel (RPC failure); patched into the XML afterwards.
    $s = $book.Worksheets.Item(5); $s.Tab.Color = 255
    $s = $book.Worksheets.Item(6); $s.Tab.ThemeColor = 5; $s.Tab.TintAndShade = 0.4
    $s = $book.Worksheets.Item(4); $s.Visible = 2   # xlSheetVeryHidden
    $book.Worksheets.Item(2).Visible = 0            # hidden
    $book.Worksheets.Item(1).Activate()
}

# ---- 02 protection ----
Build '02-protection' {
    param($book)
    [void]$book.Worksheets.Add($m, $book.Worksheets.Item(1))
    $s = $book.Worksheets.Item(1); $s.Name = 'Locked'
    Rows $s 'A1' $people
    $s.Range('A2:B3').Locked = $false
    $s.Range('C2').Formula = '=A2&B2'; $s.Range('C2').FormulaHidden = $true
    # Protect(Password, DrawingObjects, Contents, Scenarios, UserInterfaceOnly, FormatCells, FormatColumns, FormatRows, InsertColumns, InsertRows, InsertHyperlinks, DeleteColumns, DeleteRows, Sort, Filter, PivotTables)
    $s.Protect('pw123', $true, $true, $true, $false, $true, $true, $false, $true, $false, $true, $false, $true, $true, $true, $false)
    $s2 = $book.Worksheets.Item(2); $s2.Name = 'Open'; Rows $s2 'A1' $people
    $s2.Protect($m, $true, $true, $false)
    $book.Protect('wbpw', $true, $false)
}

# ---- 03 page setup ----
Build '03-pagesetup' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'Print'
    $r = @(); $r += , @('Item', 'Region', 'Units', 'Price'); for ($i = 1; $i -le 60; $i++) { $r += , @("Row$i", 'N', $i, $i * 0.1) }
    Rows $s 'A1' $r
    $ps = $s.PageSetup
    $ps.Orientation = 2
    $ps.PaperSize = 5
    $ps.LeftMargin = $excel.InchesToPoints(0.5); $ps.RightMargin = $excel.InchesToPoints(0.6)
    $ps.TopMargin = $excel.InchesToPoints(1.1); $ps.BottomMargin = $excel.InchesToPoints(1.2)
    $ps.HeaderMargin = $excel.InchesToPoints(0.4); $ps.FooterMargin = $excel.InchesToPoints(0.45)
    $ps.Zoom = $false; $ps.FitToPagesWide = 1; $ps.FitToPagesTall = $false
    $ps.PrintArea = '$A$1:$D$50'
    $ps.PrintTitleRows = '$1:$2'
    $ps.LeftHeader = 'Page &P of &N'; $ps.CenterHeader = '&A'; $ps.RightHeader = '&D'
    $ps.LeftFooter = '&F'; $ps.CenterFooter = 'Footer &P/&N'; $ps.RightFooter = '&D'
    $ps.PrintGridlines = $true
    $ps.CenterHorizontally = $true; $ps.CenterVertically = $true
    Try-Step 'page breaks' { [void]$s.HPageBreaks.Add($s.Range('A20')); [void]$s.HPageBreaks.Add($s.Range('A35')) }
    Try-Step 'vertical page break' { [void]$s.VPageBreaks.Add($s.Range('C1')) }
    $s2 = $book.Worksheets.Add($m, $s); $s2.Name = 'Print2'; Rows $s2 'A1' $people
    $p2 = $s2.PageSetup; $p2.Zoom = 85; $p2.Orientation = 1; $p2.PaperSize = 9
    $p2.FirstPageNumber = 3; $p2.Order = 2; $p2.BlackAndWhite = $true; $p2.PrintHeadings = $true
    $p2.PrintTitleColumns = '$A:$A'
}

# ---- 04 outline ----
Build '04-outline' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'Outline'
    Rows $s 'A1' @(@('Item', 'Q1', 'Q2', 'Q3', 'Q4'), @('a', 1, 2, 3, 4), @('b', 5, 6, 7, 8), @('c', 9, 10, 11, 12), @('Sub1', '', '', '', ''), @('d', 1, 2, 3, 4), @('e', 5, 6, 7, 8), @('f', 9, 10, 11, 12), @('Sub2', '', '', '', ''), @('Total', '', '', '', ''))
    foreach ($col in 'B', 'C', 'D', 'E') {
        $s.Range("${col}5").Formula = "=SUBTOTAL(9,${col}2:${col}4)"
        $s.Range("${col}9").Formula = "=SUBTOTAL(9,${col}6:${col}8)"
        $s.Range("${col}10").Formula = "=SUBTOTAL(9,${col}2:${col}9)"
    }
    $s.Outline.SummaryRow = 1; $s.Outline.SummaryColumn = 2
    [void]$s.Range('A2:A4').EntireRow.Group()
    [void]$s.Range('A6:A8').EntireRow.Group()
    [void]$s.Range('B1:C1').EntireColumn.Group()
    [void]$s.Range('A3:A4').EntireRow.Group()   # nested level 3
    $s.Rows.Item(9).ShowDetail = $false     # one collapsed group (rows 6:8)
    $s2 = $book.Worksheets.Add($m, $s); $s2.Name = 'AboveRight'; Rows $s2 'A1' $people
    $s2.Outline.SummaryRow = 0; $s2.Outline.SummaryColumn = 1
    [void]$s2.Range('A3:A5').EntireRow.Group()
}

# ---- 05 conditional formats ----
Build '05-condfmt' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'CF'
    for ($i = 1; $i -le 10; $i++) { foreach ($c in 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'J', 'K', 'L', 'M', 'N') { Put $s "$c$i" (($i * 7) % 11 - 4) } }
    for ($i = 1; $i -le 10; $i++) { Put $s "H$i" "=TODAY()+($i-5)*4"; $s.Range("H$i").NumberFormat = 'yyyy-mm-dd' }
    $t = 'apple', 'banana', 'apricot', 'cherry', 'avocado', 'fig', 'grape', 'apple pie', 'lime', 'melon'
    for ($i = 1; $i -le 10; $i++) { Put $s "I$i" $t[$i - 1] }
    $setIdx = @{ A = 1; B = 4; C = 9; D = 14; E = 17 }
    foreach ($c in 'A', 'B', 'C', 'D', 'E') { Try-Step "iconset $c" { $fc = $s.Range("${c}1:${c}10").FormatConditions.AddIconSetCondition(); $fc.IconSet = $book.IconSets.Item($setIdx[$c]) } }
    Try-Step 'iconset reverse/showvalue' { $fc = $s.Range('E1:E10').FormatConditions.Item(1); $fc.ReverseOrder = $true; $fc.ShowIconOnly = $true }
    Try-Step 'databar negative fixed axis' {
        $db = $s.Range('F1:F10').FormatConditions.AddDatabar()
        $db.BarFillType = 0; $db.AxisPosition = 1; $db.Direction = -5002
        [void]$db.MinPoint.Modify(0, -10); [void]$db.MaxPoint.Modify(0, 10)
        $db.BarColor.Color = 0xC07000; $db.NegativeBarFormat.ColorType = 1
        $db.BarBorder.Type = 1
    }
    Try-Step '3 color scale' { $cs = $s.Range('G1:G10').FormatConditions.AddColorScale(3); $cs.ColorScaleCriteria.Item(2).Type = 5; $cs.ColorScaleCriteria.Item(2).Value = 50 }
    Try-Step 'time period yesterday' { $fc = $s.Range('H1:H10').FormatConditions.Add(11, $m, $m, $m, $m, $m, 1); $fc.Interior.Color = 0x99FFFF }
    Try-Step 'time period last 7 days' { $fc = $s.Range('H1:H10').FormatConditions.Add(11, $m, $m, $m, $m, $m, 2); $fc.Interior.Color = 0xCCFFCC }
    Try-Step 'time period this month' { $fc = $s.Range('H1:H10').FormatConditions.Add(11, $m, $m, $m, $m, $m, 9); $fc.Font.Bold = $true }
    Try-Step 'begins with' { $fc = $s.Range('I1:I10').FormatConditions.Add(9, $m, $m, $m, 'ap', 2); $fc.Interior.Color = 0xFFCC99 }
    Try-Step 'above std dev' { $fc = $s.Range('J1:J10').FormatConditions.AddAboveAverage(); $fc.AboveBelow = 4; $fc.NumStdDev = 1; $fc.Interior.Color = 0x9999FF }
    Try-Step 'top 10 percent' { $fc = $s.Range('K1:K10').FormatConditions.AddTop10(); $fc.TopBottom = 1; $fc.Rank = 10; $fc.Percent = $true; $fc.Interior.Color = 0x99CCFF }
    Try-Step 'stop if true' {
        $fc = $s.Range('L1:L10').FormatConditions.Add(1, 5, '=5'); $fc.Interior.Color = 0x00FF00; $fc.StopIfTrue = $true
        $fc2 = $s.Range('L1:L10').FormatConditions.Add(1, 5, '=2'); $fc2.Interior.Color = 0xFF0000; $fc2.StopIfTrue = $false
    }
    Try-Step 'formula rule' { $fc = $s.Range('M1:M10').FormatConditions.Add(2, $m, '=MOD(ROW(),2)=0'); $fc.Interior.Color = 0xEEEEEE; $fc.Font.Italic = $true }
    Try-Step 'overlapping rules' {
        $r = $s.Range('N1:N10')
        $a = $r.FormatConditions.Add(1, 5, '=3'); $a.Interior.Color = 0x00FFFF
        $b = $r.FormatConditions.AddDatabar()
        $c = $r.FormatConditions.AddColorScale(2)
        $d = $r.FormatConditions.AddUniqueValues(); $d.DupeUnique = 0; $d.Font.Color = 255
    }
    Try-Step 'between/blanks/errors' {
        $fc = $s.Range('P1:P10').FormatConditions.Add(1, 1, '=1', '=5'); $fc.Interior.Color = 0xFFFF00
        $fc = $s.Range('P1:P10').FormatConditions.Add(10); $fc.Interior.Color = 0x888888
        $fc = $s.Range('P1:P10').FormatConditions.Add(16); $fc.Font.Color = 255
    }
}

# ---- 06 data validation ----
Build '06-validation' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'DV'
    $l = $book.Worksheets.Add($m, $s); $l.Name = 'Lists'
    Rows $l 'A1' @(@('Red'), @('Green'), @('Blue'), @('Black'))
    $s.Activate()
    $v = $s.Range('A1').Validation; $v.Delete(); $v.Add(3, 1, 1, 'Yes,No,Maybe')
    $v.InputTitle = 'Pick'; $v.InputMessage = 'Choose one'; $v.ErrorTitle = 'Bad'; $v.ErrorMessage = 'Not allowed'; $v.ShowInput = $true; $v.ShowError = $true
    $v = $s.Range('A2').Validation; $v.Delete(); $v.Add(1, 2, 1, '1', '10'); $v.ErrorTitle = 'Warn'; $v.ErrorMessage = 'Between 1 and 10 please'
    $v = $s.Range('A3').Validation; $v.Delete(); $v.Add(2, 3, 5, '0.5'); $v.ErrorTitle = 'Info'; $v.ErrorMessage = 'Greater than 0.5'
    $v = $s.Range('A4').Validation; $v.Delete(); $v.Add(3, 1, 1, '=Lists!$A$1:$A$4')
    $v = $s.Range('A5').Validation; $v.Delete(); $v.Add(7, 1, 1, '=AND(A5>0,A5<100)')
    $v = $s.Range('A6').Validation; $v.Delete(); $v.Add(1, 1, 1, '1', '5'); $v.IgnoreBlank = $false
    $v = $s.Range('A7').Validation; $v.Delete(); $v.Add(3, 1, 1, 'x,y,z'); $v.InCellDropdown = $false
    $v = $s.Range('A8').Validation; $v.Delete(); $v.Add(4, 1, 1, '1/1/2026', '12/31/2026')
    $v = $s.Range('A9').Validation; $v.Delete(); $v.Add(5, 1, 1, '08:00', '17:00')
    $v = $s.Range('A10').Validation; $v.Delete(); $v.Add(6, 1, 8, '5'); $v.InputMessage = 'max 5 chars'
    $v = $s.Range('A11:A13').Validation; $v.Delete(); $v.Add(1, 1, 3, '0')
}

# ---- 07 table ----
Build '07-table' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'T'
    Rows $s 'A1' $people
    Put $s 'E1' 'Total'
    $lo = $s.ListObjects.Add(1, $s.Range('A1:E9'), $m, 1); $lo.Name = 'Fruit'
    $s.Range('E2').Formula = '=[@Units]*[@Price]'
    $lo.ShowTotals = $true
    $lo.ListColumns.Item(1).TotalsCalculation = 3
    $lo.ListColumns.Item(3).TotalsCalculation = 1
    $lo.ListColumns.Item(4).TotalsCalculation = 2
    $lo.ListColumns.Item(5).TotalsCalculation = 5
    $lo.ShowTableStyleRowStripes = $true; $lo.ShowTableStyleColumnStripes = $true; $lo.ShowTableStyleFirstColumn = $true
    Try-Step 'custom table style' {
        $ts = $book.TableStyles.Add('ProbeStyle'); $ts.ShowAsAvailableTableStyle = $true
        $ts.TableStyleElements.Item(1).Interior.Color = 0x7030A0
        $ts.TableStyleElements.Item(1).Font.Color = 0xFFFFFF
        $ts.TableStyleElements.Item(0).Borders.Item(8).LineStyle = 1
        $lo.TableStyle = 'ProbeStyle'
    }
    Put $s 'H1' '=SUM(Fruit[Units])'
    Try-Step 'slicer' {
        $sc = $book.SlicerCaches.Add2($lo, 'Region')
        [void]$sc.Slicers.Add($s, $m, 'RegionSlicer', 'Region', 20, 400, 144, 120)
    }
}

# ---- 08 sort and filter ----
Build '08-filter' {
    param($book)
    $d = @(@('Name', 'Score', 'Date', 'Flag'), @('Ann', 90, '=DATE(2026,1,5)', 'a'), @('Bob', 75, '=DATE(2025,6,1)', 'b'), @('Al', 60, '=DATE(2026,10,2)', 'c'), @('Cy', 88, '=DATE(2026,3,3)', 'd'), @('Abe', 45, '=DATE(2024,3,3)', 'e'), @('Dee', 99, '=DATE(2026,8,8)', 'f'), @('Eve', 70, '=DATE(2026,10,20)', 'g'), @('Fay', 66, '=DATE(2023,2,2)', 'h'), @('Gus', 81, '=DATE(2026,12,1)', 'i'), @('Hal', 55, '=DATE(2022,2,2)', 'j'), @('Ian', 92, '=DATE(2026,5,5)', 'k'), @('Jo', 33, '=DATE(2026,7,7)', 'l'))
    $names = 'TopTen', 'ByColor', 'DateDynamic', 'TextCustom', 'SortState'
    for ($i = 0; $i -lt 5; $i++) {
        if ($i -eq 0) { $s = $book.Worksheets.Item(1) } else { $s = $book.Worksheets.Add($m, $book.Worksheets.Item($book.Worksheets.Count)) }
        $s.Name = $names[$i]; Rows $s 'A1' $d; $s.Range('C2:C13').NumberFormat = 'yyyy-mm-dd'
    }
    $s = $book.Worksheets.Item('TopTen'); [void]$s.Range('A1:D13').AutoFilter(2, 3, 3)       # top 3 items
    $s = $book.Worksheets.Item('ByColor'); $s.Range('A3').Interior.Color = 255; $s.Range('A5').Interior.Color = 255; $s.Range('A9').Interior.Color = 255
    [void]$s.Range('A1:D13').AutoFilter(1, 255, 8)                                          # xlFilterCellColor
    $s = $book.Worksheets.Item('DateDynamic'); [void]$s.Range('A1:D13').AutoFilter(3, 13, 11)  # xlFilterThisYear, xlFilterDynamic
    $s = $book.Worksheets.Item('TextCustom'); [void]$s.Range('A1:D13').AutoFilter(1, 'A*', 2, 'B*')   # xlOr
    $s = $book.Worksheets.Item('SortState'); [void]$s.Range('A1:D13').AutoFilter(2, '>50')
    $so = $s.AutoFilter.Sort
    [void]$so.SortFields.Clear()
    [void]$so.SortFields.Add($s.Range('B2:B13'), 0, 2)
    [void]$so.SortFields.Add($s.Range('A2:A13'), 0, 1)
    $so.Header = 1; $so.Apply()
}

# ---- 09 comments ----
Build '09-comments' {
    param($book)
    $excel.UserName = 'Probe Author'
    $s = $book.Worksheets.Item(1); $s.Name = 'C'
    Rows $s 'A1' $people
    [void]$s.Range('A2').AddComment('First note text')
    [void]$s.Range('B3').AddComment('Second note')
    $s.Range('B3').Comment.Visible = $true
    Try-Step 'threaded comment' {
        $tc = $s.Range('C4').AddCommentThreaded('Thread starter')
        [void]$tc.AddReply('Reply one')
        [void]$tc.AddReply('Reply two')
    }
    Try-Step 'second threaded comment' { [void]$s.Range('D5').AddCommentThreaded('Another thread') }
}

# ---- 10 text and alignment ----
Build '10-text' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'Text'
    Put $s 'A1' 'Bold plain red sup2 end'
    $c = $s.Range('A1'); $c.Characters(1, 4).Font.Bold = $true; $c.Characters(11, 3).Font.Color = 255; $c.Characters(16, 4).Font.Superscript = $true; $c.Characters(21, 3).Font.Italic = $true
    $s.Range('A2').Value2 = "line one`nline two"; $s.Range('A2').WrapText = $true
    Put $s 'A3' 'vertical'; $s.Range('A3').Orientation = -4166
    Put $s 'A4' 'rot45'; $s.Range('A4').Orientation = 45
    Put $s 'A5' 'indent3'; $s.Range('A5').HorizontalAlignment = -4131; $s.Range('A5').IndentLevel = 3
    Put $s 'A6' 'shrink to fit this long text'; $s.Range('A6').ShrinkToFit = $true; $s.Columns.Item(1).ColumnWidth = 12
    Put $s 'A7' 'fill'; $s.Range('A7').HorizontalAlignment = 5
    Put $s 'A8' 'justify this long text wraps around'; $s.Range('A8').HorizontalAlignment = -4130; $s.Range('A8').WrapText = $true
    Put $s 'A9' 'distributed'; $s.Range('A9').HorizontalAlignment = -4117
    Put $s 'C9' 'across selection'; $s.Range('C9:E9').HorizontalAlignment = 7
    Put $s 'A10' 'vjustify text here'; $s.Range('A10').VerticalAlignment = -4130; $s.Range('A10').WrapText = $true
    Put $s 'A11' 'vdistributed'; $s.Range('A11').VerticalAlignment = -4117
    Put $s 'A12' 'vtop'; $s.Range('A12').VerticalAlignment = -4160; $s.Rows.Item(12).RowHeight = 40
    Put $s 'A13' 'rtl order'; Try-Step 'reading order' { $s.Range('A13').ReadingOrder = -5003 }
    Put $s 'A14' 'rot-45'; $s.Range('A14').Orientation = -45
}

# ---- 11 styles ----
Build '11-styles' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'Styles'
    $i = 1
    foreach ($n in 'Good', 'Bad', 'Heading 1', 'Currency', 'Neutral', 'Title', 'Percent', 'Accent1') { Put $s "A$i" "style $n"; Try-Step "style $n" { $s.Range("A$i").Style = $n }; $i++ }
    Put $s 'B1' 'gray125'; $s.Range('B1').Interior.Pattern = 18; $s.Range('B1').Interior.PatternColor = 0x0000FF; $s.Range('B1').Interior.Color = 0x00FFFF
    Put $s 'B2' 'lightUp'; $s.Range('B2').Interior.Pattern = -4162
    Put $s 'B3' 'gradient'
    Try-Step 'gradient' { $r = $s.Range('B3'); $r.Interior.Pattern = 4000; $g = $r.Interior.Gradient; $g.ColorStops.Item(1).Color = 255; $g.ColorStops.Item(2).Color = 0xFF0000; $g.ColorStops.Add(0.5).Color = 0x00FF00 }
    $combos = @(@(1, 2), @(1, -4138), @(-4115, 2), @(-4118, 2), @(1, 4), @(-4119, 4), @(1, 1), @(-4115, -4138), @(4, 2), @(4, -4138), @(5, 2), @(5, -4138), @(13, -4138))
    for ($k = 0; $k -lt 13; $k++) { $c = $s.Range("D$($k + 1)"); Put $s "D$($k + 1)" "border $k"; $b = $c.Borders.Item(9); $b.LineStyle = $combos[$k][0]; $b.Weight = $combos[$k][1]; $b.Color = 0x0000FF; $b2 = $c.Borders.Item(7); $b2.LineStyle = $combos[$k][0]; $b2.Weight = $combos[$k][1] }
    Put $s 'F1' 'diag down'; $s.Range('F1').Borders.Item(5).LineStyle = 1
    Put $s 'F2' 'diag up'; $s.Range('F2').Borders.Item(6).LineStyle = 1
    Put $s 'F3' 'diag both'; $s.Range('F3').Borders.Item(5).LineStyle = 1; $s.Range('F3').Borders.Item(6).LineStyle = -4115; $s.Range('F3').Borders.Item(5).Color = 255
    Put $s 'H1' 'theme tint'; $s.Range('H1').Interior.ThemeColor = 5; $s.Range('H1').Interior.TintAndShade = 0.4; $s.Range('H1').Font.ThemeColor = 2
    Put $s 'H2' 'theme dark'; $s.Range('H2').Interior.ThemeColor = 7; $s.Range('H2').Interior.TintAndShade = -0.25
    Put $s 'J1' '-1234.5'; $s.Range('J1').NumberFormat = '[Green]#,##0.00;[Red]-#,##0.00;[Blue]0;"text:"@'
    Put $s 'J2' '0'; $s.Range('J2').NumberFormat = '[Green]#,##0.00;[Red]-#,##0.00;[Blue]0;"text:"@'
    Put $s 'J3' '1234.5'; $s.Range('J3').NumberFormat = '_($* #,##0.00_);_($* (#,##0.00);_($* "-"??_);_(@_)'
    Put $s 'J4' '0.3333333'; $s.Range('J4').NumberFormat = '# ??/??'
    Put $s 'J5' '1.5'; $s.Range('J5').NumberFormat = '[h]:mm:ss'
    Put $s 'J6' '2.5'; $s.Range('J6').NumberFormat = '[mm]:ss'
    Put $s 'J7' '1234567'; $s.Range('J7').NumberFormat = '0.0,,"M"'
    Put $s 'J8' '46318.5'; $s.Range('J8').NumberFormat = '[$-409]dddd, mmmm d, yyyy h:mm AM/PM'
    Put $s 'J9' '5'; $s.Range('J9').NumberFormat = '[Red][<=3]0.0;[Blue][>3]0.00'
    Put $s 'K1' 'font'; $c = $s.Range('K1'); $c.Font.Name = 'Courier New'; $c.Font.Size = 14; $c.Font.Underline = 4; $c.Font.Strikethrough = $true; $c.Font.Bold = $true
}

# ---- 12 hyperlinks ----
Build '12-hyperlinks' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'L'
    [void]$book.Worksheets.Add($m, $s); $book.Worksheets.Item(2).Name = 'Other'
    [void]$s.Hyperlinks.Add($s.Range('A1'), 'https://example.com/page?q=1', $m, 'External tip', 'External')
    [void]$s.Hyperlinks.Add($s.Range('A2'), '', "'Other'!B2", 'Internal tip', 'Go to Other')
    [void]$s.Hyperlinks.Add($s.Range('A3'), 'mailto:jason@example.com?subject=Hi', $m, 'Mail tip', 'Mail')
    [void]$s.Hyperlinks.Add($s.Range('A4'), 'C:\temp\file.txt', $m, 'File tip', 'File link')
    Put $s 'A5' '=HYPERLINK("https://example.org","fn link")'
}

# ---- 13 names ----
Build '13-names' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'N'
    $o = $book.Worksheets.Add($m, $s); $o.Name = 'Other'
    Rows $s 'A1' @(@(3), @(1), @(2))
    [void]$s.Names.Add('LocalName', '=N!$A$1:$A$3')
    [void]$o.Names.Add('LocalName', '=Other!$A$1')
    $h = $book.Names.Add('HiddenName', '=N!$B$1'); $h.Visible = $false
    [void]$book.Names.Add('FiveConst', '=5')
    [void]$book.Names.Add('DynRange', '=OFFSET(N!$A$1,0,0,3,1)')
    [void]$book.Names.Add('Global1', '=N!$A$1:$A$3')
    $s.PageSetup.PrintArea = '$A$1:$B$5'
    Put $s 'D1' '=SORT(Global1)'
    $s.Range('D1').Formula2 = '=SORT(LocalName)'
    Put $s 'F1' '=FiveConst*2'
    Put $s 'F2' '=SUM(DynRange)'
    Put $s 'F3' '=SUM(Global1)'
    $book.Names.Add('Comment1', '=N!$A$1') | Out-Null; $book.Names.Item('Comment1').Comment = 'a comment on a name'
    Put $o 'C1' '=LocalName'
}

# ---- 14 charts ----
Build '14-charts' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'Charts'
    Rows $s 'A1' @(@('Cat', 'High', 'Low', 'Close'), @('Mon', 12, 8, 10), @('Tue', 14, 9, 12), @('Wed', 13, 7, 9), @('Thu', 16, 10, 15), @('Fri', 15, 11, 12))
    $types = [ordered]@{ ColClustered = 51; ColStacked = 52; BarClustered = 57; LineMarkers = 65; Pie = 5; Doughnut = -4120; Area = 1; Scatter = -4169; Bubble = 15; StockHLC = 88; Radar = -4151; Surface = 83; Treemap = 117; Sunburst = 120; Histogram = 118; Waterfall = 119; Funnel = 123; BoxWhisker = 121 }
    $n = 0
    foreach ($k in $types.Keys) {
        $left = 400 + ($n % 4) * 330; $top = 5 + [math]::Floor($n / 4) * 240; $n++
        Try-Step "chart $k" {
            $co = $s.Shapes.AddChart2(-1, $types[$k], $left, $top, 320, 230); $co.Name = "chart_$k"; $ch = $co.Chart
            if ($k -in 'StockHLC') { $ch.SetSourceData($s.Range('A1:D6')) }
            elseif ($k -eq 'Bubble') { $ch.SetSourceData($s.Range('B1:D6')) }
            else { $ch.SetSourceData($s.Range('A1:C6')) }
            $ch.HasTitle = $true; $ch.ChartTitle.Text = "T $k"
            if ($k -notin 'Pie', 'Doughnut', 'Treemap', 'Sunburst', 'Histogram', 'Waterfall', 'Funnel', 'BoxWhisker', 'Radar', 'Surface') {
                Try-Step "axis titles $k" { $ch.Axes(1).HasTitle = $true; $ch.Axes(1).AxisTitle.Text = 'X axis'; $ch.Axes(2).HasTitle = $true; $ch.Axes(2).AxisTitle.Text = 'Y axis' }
            }
            Try-Step "labels $k" { $ch.SeriesCollection(1).HasDataLabels = $true }
            $ch.HasLegend = $true
        }
    }
    Try-Step 'trendline' { $c = $s.ChartObjects('chart_LineMarkers').Chart; [void]$c.SeriesCollection(1).Trendlines().Add(-4132); $c.SeriesCollection(1).Trendlines(1).DisplayEquation = $true }
    Try-Step 'error bars' { $c = $s.ChartObjects('chart_ColClustered').Chart; [void]$c.SeriesCollection(1).ErrorBar(1, 1, 1, 2) }
    Try-Step 'combo secondary axis' {
        $co = $s.Shapes.AddChart2(-1, 51, 400, 1000, 320, 230); $co.Name = 'chart_Combo'; $ch = $co.Chart
        $ch.SetSourceData($s.Range('A1:C6')); $ser = $ch.SeriesCollection(2); $ser.ChartType = 65; $ser.AxisGroup = 2
        $ch.HasTitle = $true; $ch.ChartTitle.Text = 'Combo'; $ch.HasLegend = $true
        $ch.Axes(2, 2).HasTitle = $true; $ch.Axes(2, 2).AxisTitle.Text = 'Secondary'
    }
    Try-Step 'chart sheet' { $cs = $book.Charts.Add(); $cs.SetSourceData($s.Range('A1:C6')); $cs.ChartType = 51; $cs.Name = 'ChartSheet1' }
}

# ---- 15 pictures and shapes ----
Build '15-shapes' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'Shapes'
    $png = Join-Path $Folder 'probe-picture.png'
    Add-Type -AssemblyName System.Drawing
    $bmp = New-Object System.Drawing.Bitmap 80, 60
    $g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::Orange); $g.FillEllipse([System.Drawing.Brushes]::Navy, 10, 10, 50, 40); $g.Dispose()
    $bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
    $p = $s.Shapes.AddPicture($png, 0, -1, 20, 20, 80, 60); $p.Name = 'Pic1'
    $sh = $s.Shapes.AddShape(1, 150, 20, 100, 50); $sh.Name = 'Rect1'; $sh.TextFrame2.TextRange.Text = 'rect'
    $tb = $s.Shapes.AddTextbox(1, 280, 20, 120, 40); $tb.Name = 'Text1'; $tb.TextFrame2.TextRange.Text = 'text box'
    Try-Step 'form button' { $bt = $s.Shapes.AddFormControl(0, 20, 120, 100, 30); $bt.Name = 'Button1'; $bt.TextFrame.Characters().Text = 'Press' }
    Try-Step 'form checkbox' { $cb = $s.Shapes.AddFormControl(1, 150, 120, 100, 30); $cb.Name = 'Check1' }
    Try-Step 'arrow line' { $ln = $s.Shapes.AddLine(20, 200, 200, 240); $ln.Name = 'Line1' }
}

# ---- 16 sparklines ----
Build '16-sparklines' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'Spark'
    Rows $s 'A1' @(@('Row', 'Q1', 'Q2', 'Q3', 'Q4'), @('a', 1, 5, 2, 8), @('b', 4, -3, 6, 1), @('c', -2, 3, -1, 4), @('d', 7, 7, 2, 9))
    $g1 = $s.Range('F2:F5').SparklineGroups.Add(1, 'Spark!B2:E5'); $g1.SeriesColor.Color = 255; $g1.Points.Markers.Visible = $true
    $g2 = $s.Range('G2:G5').SparklineGroups.Add(2, 'Spark!B2:E5'); $g2.Points.Negative.Visible = $true
    $g3 = $s.Range('H2:H5').SparklineGroups.Add(3, 'Spark!B2:E5')
    Try-Step 'axis settings' { $g1.Axes.Vertical.MinScaleType = 2; $g1.Axes.Vertical.CustomMinScaleValue = -5 }
}

# ---- 17 pivot ----
Build '17-pivot' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'Data'
    Rows $s 'A1' $people
    $p = $book.Worksheets.Add($m, $s); $p.Name = 'Pivot'
    $pc = $book.PivotCaches().Create(1, $s.Range('A1:D9'))
    $pt = $pc.CreatePivotTable($p.Range('A3'), 'PT1')
    $pt.PivotFields('Region').Orientation = 1
    $f = $pt.PivotFields('Units'); $f.Orientation = 4; $f.Function = -4157
    Try-Step 'pivot chart' { $co = $p.Shapes.AddChart2(-1, 51, 300, 20, 320, 230); $co.Chart.SetSourceData($pt.TableRange1); $co.Name = 'PivotChart1' }
}

# ---- 18 formulas ----
$extPath = Join-Path $Folder '18-external-target.xlsx'
if ($Only.Count -eq 0 -or $Only -contains '18-formulas') {
    $eb = $excel.Workbooks.Add(); $eb.Worksheets.Item(1).Name = 'Sheet1'; $eb.Worksheets.Item(1).Range('A1').Value2 = 42; $eb.SaveAs($extPath, 51); $eb.Close($false)
}
Build '18-formulas' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'Sheet1'
    [void]$book.Worksheets.Add($m, $s); [void]$book.Worksheets.Add($m, $book.Worksheets.Item(2))
    $book.Worksheets.Item(2).Name = 'Sheet2'; $book.Worksheets.Item(3).Name = 'Sheet3'
    Rows $s 'A1' @(@(1, 2), @(3, 4), @(5, 6), @(7, 8))
    foreach ($k in 2, 3) { Put $book.Worksheets.Item($k) 'A1' ($k * 10) }
    Put $s 'C1' '=A1*2'; $s.Range('C1:C4').FillDown() | Out-Null           # shared formula
    $s.Range('E1:E3').FormulaArray = '=A1:A3*B1:B3'                         # CSE array
    $s.Range('G1').Formula2 = '=SEQUENCE(3,2)'                              # spill
    Put $s 'J1' '=1/0'; Put $s 'J2' '=NA()'; Put $s 'J3' '=FOOBAR(1)'; Put $s 'J4' '=SUM(A1:A2 C1:C2)'; Put $s 'J5' '=SQRT(-1)'
    Put $s 'J6' '=#REF!'; Put $s 'J7' '="a"+1'
    $s.Range('J8').Formula2 = '=SEQUENCE(3)'; Put $s 'J9' 'blocker'                         # #SPILL!
    Try-Step 'calc error' { $s.Range('J10').Formula2 = '=LAMBDA(x,x)' }
    $s.Range('L2').FormulaR1C1 = '=R[-1]C[-10]+RC[-11]*2'
    $s.Range('L3').FormulaR1C1 = '=SUM(R1C1:R4C2)'
    Put $s 'L4' '=SUM(Sheet1:Sheet3!A1)'
    $lo = $s.ListObjects.Add(1, $s.Range('N1:O4'), $m, 0); $lo.Name = 'Tbl'
    Put $s 'N1' 'Alpha'; Put $s 'O1' 'Beta'; Rows $s 'N2' @(@(1, 2), @(3, 4), @(5, 6))
    Put $s 'Q1' '=SUM(Tbl[Alpha])'; Put $s 'Q2' '=Tbl[[#This Row],[Beta]]'
    Try-Step 'external link' { $s.Range('Q3').Formula = "='$($Folder -replace '/', '\')\[18-external-target.xlsx]Sheet1'!`$A`$1" }
    Try-Step 'regextest future' { $s.Range('Q4').Formula2 = '=REGEXTEST("abc","b")' }
    # IMAGE (fetches a URL) and PY (cloud runtime) hung the hidden Excel for over 5 minutes in the first run (one of the two; not told apart), so they are not built.
    Try-Step 'implicit intersection (legacy)' { $s.Range('S1').Formula = '=A1:A4' }
    Try-Step 'implicit intersection (@)' { $s.Range('S2').Formula2 = '=@A1:A4' }
    Try-Step 'let lambda' { $s.Range('S3').Formula2 = '=LET(x,A1:A4,SUM(x))' }
    Try-Step 'xlookup' { $s.Range('S4').Formula2 = '=XLOOKUP(3,A1:A4,B1:B4)' }
}

# ---- 19 xlsm ----
Build '19-macro' {
    param($book)
    $book.Worksheets.Item(1).Range('A1').Value2 = 'macro book'
    try { $mod = $book.VBProject.VBComponents.Add(1); $mod.CodeModule.AddFromString("Sub Hello()`r`n  MsgBox ""hi""`r`nEnd Sub") }
    catch { Log "  refused: VBProject not reachable ($($_.Exception.Message)); saving an .xlsm with no VBA project" }
} 'xlsm'

# ---- 20 properties and calc ----
Build '20-calcprops' {
    param($book)
    while ($book.Worksheets.Count -lt 3) { [void]$book.Worksheets.Add($m, $book.Worksheets.Item($book.Worksheets.Count)) }
    $s = $book.Worksheets.Item(1); $s.Name = 'P1'
    Rows $s 'A1' @(@(1.2345, '=A1*3'), @(2.5555, '=A2*3'))
    $s.Range('A1:B2').NumberFormat = '0.00'
    Put $s 'D1' '=D2+1'; Put $s 'D2' '=D1*0.5'            # a cycle for the iteration setting
    Try-Step 'chart sheet' { $cs = $book.Charts.Add(); $cs.SetSourceData($s.Range('A1:A2')); $cs.Name = 'CS' }
    $book.Date1904 = $true
    $bt = [System.__ComObject]
    Try-Step 'builtin props' {
        $props = $book.BuiltinDocumentProperties
        foreach ($kv in @(@('Title', 'Probe title'), @('Author', 'Probe author'), @('Subject', 'Probe subject'), @('Company', 'Probe company'), @('Keywords', 'k1,k2'))) {
            $p = $bt.InvokeMember('Item', 'GetProperty', $null, $props, @($kv[0]))
            $bt.InvokeMember('Value', 'SetProperty', $null, $p, @($kv[1]))
        }
    }
    Try-Step 'custom props' {
        $cp = $book.CustomDocumentProperties
        [void]$bt.InvokeMember('Add', 'InvokeMethod', $null, $cp, @('ProbeText', $false, 4, 'hello'))
        [void]$bt.InvokeMember('Add', 'InvokeMethod', $null, $cp, @('ProbeNumber', $false, 1, 42))
        [void]$bt.InvokeMember('Add', 'InvokeMethod', $null, $cp, @('ProbeBool', $false, 2, $true))
    }
    $excel.Iteration = $true; $excel.MaxIterations = 50; $excel.MaxChange = 0.01
    Try-Step 'precision as displayed' { $book.PrecisionAsDisplayed = $true }
    $excel.Calculation = -4135    # manual
    $excel.CalculateBeforeSave = $false
}

# ---- 21 data type ----
Build '21-datatype' {
    # ConvertToLinkedDataType hung the hidden Excel for over 6 minutes (needs the cloud service); only built when asked for by name.
    if ($Only -notcontains '21-datatype') { throw 'skipped: hangs' }
    param($book)
    $s = $book.Worksheets.Item(1)
    Put $s 'A1' 'MSFT'
    $s.Range('A1').ConvertToLinkedDataType(268435456, 'en-US')
    Start-Sleep 3
    Put $s 'B1' '=A1.Price'
}

# ---- second sheet of workbook-level: a plain workbook for CSV ----
Build 'csv-source' {
    param($book)
    $s = $book.Worksheets.Item(1); $s.Name = 'csvprobe'
    $s.Range('A1').NumberFormat = '@'; $s.Range('A2').NumberFormat = '@'
    Put $s 'A1' 'say "hi", there'
    $s.Range('B1').Value2 = "two`nlines"
    Put $s 'C1' 'plain'
    Put $s 'A2' '007'
    Put $s 'B2' '=DATE(2026,10,23)'; $s.Range('B2').NumberFormat = 'yyyy-mm-dd'
    Put $s 'C2' '0.256'; $s.Range('C2').NumberFormat = '0.0%'
    Put $s 'A3' '=1+2'
    Put $s 'C3' 'after gap'
    Put $s 'A4' '123456789012345678901'
    Put $s 'B4' '1234567.891'; $s.Range('B4').NumberFormat = '#,##0.00'
    Put $s 'C4' 'x,y'
    Put $s 'A5' 'TRUE'; Put $s 'B5' '=1/3'; Put $s 'C5' 'tail'
    $s.Range('A6').Value2 = '  padded  '
    Put $s 'B6' '45.5'; $s.Range('B6').NumberFormat = '$#,##0.00'
    Put $s 'C6' '1E+30'
}


# ---- 01b: page break preview, written into the XML because Window.View = 2 stalls a hidden Excel ----
if (($Only.Count -eq 0 -or $Only -contains '01-views') -and (Test-Path -LiteralPath (Join-Path $Folder '01-views.xlsx'))) {
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $source = Join-Path $Folder '01-views.xlsx'
    $target = Join-Path $Folder '01b-views-pagebreakpreview.xlsx'
    Copy-Item -LiteralPath $source -Destination $target -Force
    $zip = [IO.Compression.ZipFile]::Open($target, 'Update')
    try {
        $entry = $zip.GetEntry('xl/worksheets/sheet3.xml')
        $reader = New-Object IO.StreamReader($entry.Open(), [Text.Encoding]::UTF8)
        $xml = $reader.ReadToEnd(); $reader.Dispose()
        $views = '<sheetViews><sheetView view="pageBreakPreview" zoomScale="80" zoomScaleNormal="100" zoomScaleSheetLayoutView="80" workbookViewId="0"/></sheetViews>'
        $changed = [regex]::Replace($xml, '<sheetViews>.*?</sheetViews>', $views)
        if ($changed -eq $xml) { throw 'sheet3.xml sheetViews was not found' }
        $entry.Delete()
        $fresh = $zip.CreateEntry('xl/worksheets/sheet3.xml')
        $writer = New-Object IO.StreamWriter($fresh.Open(), (New-Object Text.UTF8Encoding($false)))
        $writer.Write($changed); $writer.Dispose()
        Log 'OK   01b-views-pagebreakpreview.xlsx'
    } finally { $zip.Dispose() }
}
try { $excel.Quit() } catch { }
[void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
Start-Sleep 2
if ($excelPid -gt 0) { $left = Get-Process -Id $excelPid -ErrorAction SilentlyContinue; if ($left) { Stop-Process -Id $excelPid -Force; Log "killed leftover pid $excelPid" } }
Log "done $(Get-Date -Format s)"

