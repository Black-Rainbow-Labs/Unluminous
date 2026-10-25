# Perform outline operations in Excel and save the workbook before and after each, so the engine's
# grouping, collapsing, Auto Outline and Subtotal can be compared with what Excel does.
#
# Each scenario starts a fresh workbook holding one of the data layouts below, saves it as `<name>-in.xlsx`,
# runs the scenario's steps and saves `<name>.xlsx`. The steps use Excel's own object model, so what
# lands in the file is Excel's answer. `-Only` runs the named scenarios.
#
#   pwsh tools/excel-oracle/outline.ps1 -Out crates/unluminous-sheet/tests/fixtures/outline [-Only name,name]
param(
    [Parameter(Mandatory = $true)][string]$Out,
    [string[]]$Only
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
if ($Only) { $Only = @($Only | ForEach-Object { $_ -split ',' }) }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path -LiteralPath $Out).Path

# Collapse or expand the group whose summary row or column is `index`. Range.ShowDetail is refused by Excel
# when driven from a script, so this runs the XLM function the outline buttons run.
function Show-Detail($kind, $index, [bool]$expand) {
    $word = if ($expand) { 'TRUE' } else { 'FALSE' }
    $excel.ExecuteExcel4Macro("SHOW.DETAIL($kind,$index,$word)") | Out-Null
}
# A list: header row, then twelve rows of region, product and two numbers, sorted by region.
function Fill-List($sheet) {
    $sheet.Range('A1:D1').Value2 = [object[]]@('Region', 'Product', 'Units', 'Sales')
    $regions = @('East', 'East', 'East', 'North', 'North', 'South', 'South', 'South', 'South', 'West', 'West', 'West')
    for ($i = 0; $i -lt 12; $i++) {
        $r = $i + 2
        $sheet.Cells.Item($r, 1).Value2 = $regions[$i]
        $sheet.Cells.Item($r, 2).Value2 = "P$($i + 1)"
        $sheet.Cells.Item($r, 3).Value2 = [double](($i * 7 + 3) % 10 + 1)
        $sheet.Cells.Item($r, 4).Value2 = [double]((($i * 13 + 5) % 17 + 2) * 10)
    }
}

# Detail rows with sum rows under them and a total, and detail columns with a sum column beside them.
function Fill-Auto($sheet) {
    $sheet.Range('A1:F1').Value2 = [object[]]@('Item', 'Jan', 'Feb', 'Mar', 'Q1', 'Note')
    $rows = @(2, 3, 4, 6, 7, 8)
    $n = 1
    foreach ($r in $rows) {
        $sheet.Cells.Item($r, 1).Value2 = "Item$n"
        for ($c = 2; $c -le 4; $c++) { $sheet.Cells.Item($r, $c).Value2 = $n * $c }
        $n++
    }
    $sheet.Cells.Item(5, 1).Value2 = 'Sub 1'
    $sheet.Cells.Item(9, 1).Value2 = 'Sub 2'
    $sheet.Cells.Item(10, 1).Value2 = 'Total'
    for ($c = 2; $c -le 4; $c++) {
        $l = [char](64 + $c)
        $sheet.Cells.Item(5, $c).Formula = "=SUM($($l)2:$($l)4)"
        $sheet.Cells.Item(9, $c).Formula = "=SUM($($l)6:$($l)8)"
        $sheet.Cells.Item(10, $c).Formula = "=$($l)5+$($l)9"
    }
    foreach ($r in 2, 3, 4, 5, 6, 7, 8, 9, 10) { $sheet.Cells.Item($r, 5).Formula = "=SUM(B$($r):D$($r))" }
}

function Fill-Plain($sheet) {
    for ($r = 1; $r -le 12; $r++) { for ($c = 1; $c -le 6; $c++) { $sheet.Cells.Item($r, $c).Value2 = $r * 10 + $c } }
}

$scenarios = [ordered]@{
    'group-rows' = @{ data = 'plain'; steps = { param($s) $s.Range('3:5').Group() | Out-Null } }
    'group-rows-nested' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; $s.Range('4:4').Group() | Out-Null } }
    'group-rows-adjacent' = @{ data = 'plain'; steps = { param($s) $s.Range('2:4').Group() | Out-Null; $s.Range('5:7').Group() | Out-Null } }
    'group-columns' = @{ data = 'plain'; steps = { param($s) $s.Range('B:C').Group() | Out-Null } }
    'group-columns-nested' = @{ data = 'plain'; steps = { param($s) $s.Range('B:E').Group() | Out-Null; $s.Range('C:D').Group() | Out-Null } }
    'group-seven-levels' = @{ data = 'plain'; steps = { param($s) for ($i = 1; $i -le 7; $i++) { $s.Range("$($i + 1):12").Group() | Out-Null } } }
    'group-eighth-level-refused' = @{ data = 'plain'; steps = { param($s) for ($i = 1; $i -le 7; $i++) { $s.Range("2:12").Group() | Out-Null }; try { $s.Range('2:12').Group() | Out-Null } catch { "refused: $($_.Exception.InnerException.Message) $($_.Exception.InnerException.HResult)" | Out-File -Encoding utf8 (Join-Path $script:Out 'group-eighth-level-refused.refused.txt') } } }
    'ungroup-rows' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; $s.Range('3:4').Ungroup() | Out-Null } }
    'ungroup-all-rows' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; $s.Range('2:8').Ungroup() | Out-Null; $s.Range('2:8').Ungroup() | Out-Null } }
    'ungroup-nothing' = @{ data = 'plain'; steps = { param($s) try { $s.Range('2:3').Ungroup() | Out-Null } catch { "refused: $($_.Exception.InnerException.Message) $($_.Exception.InnerException.HResult)" | Out-File -Encoding utf8 (Join-Path $script:Out 'ungroup-nothing.refused.txt') } } }
    'ungroup-collapsed' = @{ data = 'plain'; steps = { param($s) $s.Range('3:5').Group() | Out-Null; Show-Detail 1 6 $false; $s.Range('3:5').Ungroup() | Out-Null } }
    'ungroup-columns' = @{ data = 'plain'; steps = { param($s) $s.Range('B:E').Group() | Out-Null; $s.Range('C:D').Group() | Out-Null; $s.Range('B:C').Ungroup() | Out-Null } }
    'collapse-rows' = @{ data = 'plain'; steps = { param($s) $s.Range('3:5').Group() | Out-Null; Show-Detail 1 6 $false } }
    'collapse-then-expand-rows' = @{ data = 'plain'; steps = { param($s) $s.Range('3:5').Group() | Out-Null; Show-Detail 1 6 $false; Show-Detail 1 6 $true } }
    'collapse-nested-rows' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; Show-Detail 1 6 $false } }
    'collapse-outer-nested-rows' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; Show-Detail 1 6 $false; Show-Detail 1 9 $false } }
    'expand-outer-keeps-inner-collapsed' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; Show-Detail 1 6 $false; Show-Detail 1 9 $false; Show-Detail 1 9 $true } }
    'collapse-columns' = @{ data = 'plain'; steps = { param($s) $s.Range('B:C').Group() | Out-Null; Show-Detail 2 4 $false } }
    'collapse-columns-then-expand' = @{ data = 'plain'; steps = { param($s) $s.Range('B:C').Group() | Out-Null; Show-Detail 2 4 $false; Show-Detail 2 4 $true } }
    'collapse-adjacent-groups' = @{ data = 'plain'; steps = { param($s) $s.Range('2:4').Group() | Out-Null; $s.Range('5:7').Group() | Out-Null; Show-Detail 1 5 $false } }
    'show-levels-1' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; $s.Range('B:E').Group() | Out-Null; $s.Outline.ShowLevels(1, 1) | Out-Null } }
    'show-levels-2' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; $s.Range('B:E').Group() | Out-Null; $s.Range('C:D').Group() | Out-Null; $s.Outline.ShowLevels(2, 2) | Out-Null } }
    'show-levels-rows-only' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; $s.Range('B:E').Group() | Out-Null; $s.Outline.ShowLevels(1, 0) | Out-Null } }
    'show-levels-all' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; $s.Outline.ShowLevels(1, 0) | Out-Null; $s.Outline.ShowLevels(3, 0) | Out-Null } }
    'show-levels-beyond' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; $s.Outline.ShowLevels(6, 0) | Out-Null } }
    'summary-above' = @{ data = 'plain'; steps = { param($s) $s.Outline.SummaryRow = 0; $s.Range('3:5').Group() | Out-Null; Show-Detail 1 2 $false } }
    'summary-above-collapse-nested' = @{ data = 'plain'; steps = { param($s) $s.Outline.SummaryRow = 0; $s.Range('3:9').Group() | Out-Null; $s.Range('4:6').Group() | Out-Null; Show-Detail 1 3 $false } }
    'summary-left' = @{ data = 'plain'; steps = { param($s) $s.Outline.SummaryColumn = -4131; $s.Range('C:D').Group() | Out-Null; Show-Detail 2 2 $false } }
    'summary-both-default' = @{ data = 'plain'; steps = { param($s) $s.Outline.SummaryRow = 1; $s.Outline.SummaryColumn = -4152; $s.Range('3:5').Group() | Out-Null } }
    'summary-flip-after-collapse' = @{ data = 'plain'; steps = { param($s) $s.Range('3:5').Group() | Out-Null; Show-Detail 1 6 $false; $s.Outline.SummaryRow = 0 } }
    'auto-outline' = @{ data = 'auto'; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-outline-rows-only-range' = @{ data = 'auto'; steps = { param($s) $s.Range('A1:D10').AutoOutline() | Out-Null } }
    'auto-outline-then-clear' = @{ data = 'auto'; steps = { param($s) $s.Cells.AutoOutline() | Out-Null; $s.Cells.ClearOutline() | Out-Null } }
    'clear-outline' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('B:C').Group() | Out-Null; Show-Detail 1 9 $false; $s.Cells.ClearOutline() | Out-Null } }
    'clear-outline-range' = @{ data = 'plain'; steps = { param($s) $s.Range('2:4').Group() | Out-Null; $s.Range('6:8').Group() | Out-Null; $s.Range('2:4').ClearOutline() | Out-Null } }
    'subtotal-sum' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(3, 4), $true, $false, $true) | Out-Null } }
    'subtotal-summary-above' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(4), $true, $false, $false) | Out-Null } }
    # Excel hangs on this machine when Subtotal is asked for page breaks (it waits for a printer), so the page break option has no oracle here.
    'clear-outline-keeps-manual-hidden' = @{ data = 'plain'; steps = { param($s) $s.Range('11:11').EntireRow.Hidden = $true; $s.Range('2:4').Group() | Out-Null; Show-Detail 1 5 $false; $s.Cells.ClearOutline() | Out-Null } }
    'show-levels-manual-hidden' = @{ data = 'plain'; steps = { param($s) $s.Range('11:11').EntireRow.Hidden = $true; $s.Range('2:4').Group() | Out-Null; $s.Outline.ShowLevels(1, 0) | Out-Null; $s.Outline.ShowLevels(2, 0) | Out-Null } }
    'show-levels-manual-hidden-inside' = @{ data = 'plain'; steps = { param($s) $s.Range('2:4').Group() | Out-Null; $s.Range('3:3').EntireRow.Hidden = $true; $s.Outline.ShowLevels(2, 0) | Out-Null } }
    'expand-inner-under-collapsed-outer' = @{ data = 'plain'; steps = { param($s) $s.Range('2:8').Group() | Out-Null; $s.Range('3:5').Group() | Out-Null; Show-Detail 1 6 $false; Show-Detail 1 9 $false; Show-Detail 1 6 $true } }
    'ungroup-partial-level0' = @{ data = 'plain'; steps = { param($s) $s.Range('3:5').Group() | Out-Null; $s.Range('1:6').Ungroup() | Out-Null } }
    'group-with-hidden-rows' = @{ data = 'plain'; steps = { param($s) $s.Range('4:4').EntireRow.Hidden = $true; $s.Range('3:5').Group() | Out-Null } }
    'group-whole-sheet-rows' = @{ data = 'plain'; steps = { param($s) $s.Range('1:1048576').Group() | Out-Null } }
    'group-last-row' = @{ data = 'plain'; steps = { param($s) $s.Range('1048575:1048576').Group() | Out-Null } }
    'group-first-row-summary-above' = @{ data = 'plain'; steps = { param($s) $s.Outline.SummaryRow = 0; $s.Range('1:3').Group() | Out-Null } }
    'collapse-non-summary' = @{ data = 'plain'; steps = { param($s) $s.Range('3:5').Group() | Out-Null; Show-Detail 1 8 $false } }
    'collapse-group-at-edge-columns' = @{ data = 'plain'; steps = { param($s) $s.Range('A:B').Group() | Out-Null; Show-Detail 2 3 $false } }
    'group-single-cells-range' = @{ data = 'plain'; steps = { param($s) try { $s.Range('B3:C5').Group() | Out-Null } catch { 'refused' | Out-File -Encoding utf8 (Join-Path $script:Out 'group-single-cells-range.refused.txt') } } }
    'group-columns-seven' = @{ data = 'plain'; steps = { param($s) for ($i = 1; $i -le 3; $i++) { $s.Range("$([char](65 + $i)):F").Group() | Out-Null } } }
    'auto-above' = @{ data = 'plain'; setup = { param($s) $s.Range('B2').Formula = '=SUM(B3:B5)'; $s.Range('C2').Formula = '=SUM(C3:C5)' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-single-ref' = @{ data = 'plain'; setup = { param($s) $s.Range('B5').Formula = '=B4'; $s.Range('C5').Formula = '=C4' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-gap' = @{ data = 'plain'; setup = { param($s) $s.Range('B6').Formula = '=SUM(B2:B4)'; $s.Range('C6').Formula = '=SUM(C2:C4)' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-partial-columns' = @{ data = 'plain'; setup = { param($s) $s.Range('B5').Formula = '=SUM(B2:B4)'; $s.Range('C5').Formula = '=SUM(C3:C4)' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-subtotal-function' = @{ data = 'plain'; setup = { param($s) $s.Range('B5').Formula = '=SUBTOTAL(9,B2:B4)'; $s.Range('C5').Formula = '=SUBTOTAL(9,C2:C4)' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-two-summary-rows' = @{ data = 'plain'; setup = { param($s) $s.Range('B5').Formula = '=SUM(B2:B4)'; $s.Range('B6').Formula = '=AVERAGE(B2:B4)' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-cell-references' = @{ data = 'plain'; setup = { param($s) $s.Range('B5').Formula = '=B2+B3+B4'; $s.Range('C5').Formula = '=C2+C3+C4' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-expression-around-sum' = @{ data = 'plain'; setup = { param($s) $s.Range('B5').Formula = '=SUM(B2:B4)*2'; $s.Range('C5').Formula = '=SUM(C2:C4)*2' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-columns-left' = @{ data = 'plain'; setup = { param($s) $s.Range('B2:B10').Formula = '=SUM(C2:E2)' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-columns-gap' = @{ data = 'plain'; setup = { param($s) $s.Range('G2:G10').Formula = '=SUM(B2:D2)' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-both-directions' = @{ data = 'plain'; setup = { param($s) $s.Range('B5').Formula = '=SUM(B2:B4)'; $s.Range('C5').Formula = '=SUM(C2:C4)'; $s.Range('D5').Formula = '=SUM(D2:D4)'; $s.Range('E2:E5').Formula = '=SUM(B2:D2)' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-over-existing-outline' = @{ data = 'plain'; setup = { param($s) $s.Range('B5').Formula = '=SUM(B2:B4)'; $s.Range('C5').Formula = '=SUM(C2:C4)'; $s.Range('8:10').Group() | Out-Null }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-three-levels' = @{ data = 'plain'; setup = { param($s) $s.Range('B4').Formula = '=SUM(B2:B3)'; $s.Range('B7').Formula = '=SUM(B5:B6)'; $s.Range('B8').Formula = '=B4+B7'; $s.Range('B11').Formula = '=SUM(B9:B10)'; $s.Range('B12').Formula = '=B8+B11' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-summary-above-setting' = @{ data = 'plain'; setup = { param($s) $s.Outline.SummaryRow = 0; $s.Range('B2').Formula = '=SUM(B3:B5)'; $s.Range('C2').Formula = '=SUM(C3:C5)' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-below-with-above-setting' = @{ data = 'plain'; setup = { param($s) $s.Outline.SummaryRow = 0; $s.Range('B5').Formula = '=SUM(B2:B4)'; $s.Range('C5').Formula = '=SUM(C2:C4)' }; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'auto-no-formulas' = @{ data = 'plain'; steps = { param($s) $s.Cells.AutoOutline() | Out-Null } }
    'subtotal-blank-key' = @{ data = 'list'; setup = { param($s) $s.Range('A4').ClearContents() | Out-Null; $s.Range('A5').ClearContents() | Out-Null }; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-numeric-key' = @{ data = 'list'; setup = { param($s) for ($r = 2; $r -le 13; $r++) { $s.Cells.Item($r, 1).Value2 = [double]([math]::Floor(($r - 2) / 4) + 1) } }; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-case-key' = @{ data = 'list'; setup = { param($s) $s.Range('A3').Value2 = 'EAST'; $s.Range('A4').Value2 = 'east' }; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-text-column' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4112, [object[]]@(2), $true, $false, $true) | Out-Null } }
    'subtotal-key-column-total' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(1, 4), $true, $false, $true) | Out-Null } }
    'subtotal-middle-column-key' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(2, -4157, [object[]]@(3), $true, $false, $true) | Out-Null } }
    'subtotal-after-manual-group' = @{ data = 'list'; setup = { param($s) $s.Range('3:4').Group() | Out-Null }; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-collapsed-then-subtotal' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(4), $true, $false, $true) | Out-Null; $s.Outline.ShowLevels(2, 0) | Out-Null } }
    'subtotal-hidden-rows-preexisting' = @{ data = 'list'; setup = { param($s) $s.Range('3:3').EntireRow.Hidden = $true }; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-one-row-range' = @{ data = 'list'; steps = { param($s) try { $s.Range('A1:D1').Subtotal(1, -4157, [object[]]@(4), $true, $false, $true) | Out-Null } catch { 'refused' | Out-File -Encoding utf8 (Join-Path $script:Out 'subtotal-one-row-range.refused.txt') } } }
    'subtotal-count' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4112, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-average' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4106, [object[]]@(3, 4), $true, $false, $true) | Out-Null } }
    'subtotal-max' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4136, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-min' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4139, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-product' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4149, [object[]]@(3), $true, $false, $true) | Out-Null } }
    'subtotal-count-numbers' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4113, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-stdev' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4155, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-stdevp' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4156, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-var' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4164, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-varp' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4165, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-by-product' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(2, -4157, [object[]]@(4), $true, $false, $true) | Out-Null } }
    'subtotal-replace' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(4), $true, $false, $true) | Out-Null; $s.Range('A1:D20').Subtotal(1, -4112, [object[]]@(3), $true, $false, $true) | Out-Null } }
    'subtotal-no-replace' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(4), $true, $false, $true) | Out-Null; $s.Range('A1:D20').Subtotal(1, -4112, [object[]]@(3), $false, $false, $true) | Out-Null } }
    'subtotal-remove' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(4), $true, $false, $true) | Out-Null; $s.Range('A1:D20').RemoveSubtotal() | Out-Null } }
    'subtotal-nested' = @{ data = 'list'; steps = { param($s) $s.Range('A1:D13').Subtotal(1, -4157, [object[]]@(4), $true, $false, $true) | Out-Null; $s.Range('A1:D20').Subtotal(2, -4112, [object[]]@(3), $false, $false, $true) | Out-Null } }
}

$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    foreach ($name in $scenarios.Keys) {
        if ($Only -and ($Only -notcontains $name)) { continue }
        $spec = $scenarios[$name]
        [IO.File]::Delete((Join-Path $Out "$name.refused.txt"))
        $book = $excel.Workbooks.Add()
        $sheet = $book.Worksheets.Item(1)
        switch ($spec.data) {
            'plain' { Fill-Plain $sheet }
            'auto' { Fill-Auto $sheet }
            'list' { Fill-List $sheet }
        }
        if ($spec.setup) { try { & $spec.setup $sheet } catch { "setup failed: $($_.Exception.Message) at $($_.InvocationInfo.PositionMessage)" | Out-File -Encoding utf8 (Join-Path $Out "$name.setup-failed.txt") } }
        $book.SaveAs((Join-Path $Out "$name-in.xlsx"), 51)
        try {
            & $spec.steps $sheet
        } catch {
            "refused: $($_.Exception.Message) | $($_.Exception.InnerException.Message) | $($_.Exception.InnerException.HResult)" | Out-File -Encoding utf8 (Join-Path $Out "$name.refused.txt")
        }
        $excel.Calculate()
        $book.SaveAs((Join-Path $Out "$name.xlsx"), 51)
        $book.Close($false)
        Write-Output $name
    }
} finally {
    $excel.Quit()
    [Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}






