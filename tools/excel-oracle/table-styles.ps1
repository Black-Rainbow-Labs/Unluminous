# Every built-in table style, PivotTable style and slicer style, as Excel defines it, written as JSON.
#
#   pwsh tools/excel-oracle/table-styles.ps1 crates/unluminous-sheet/src/table_styles.json
#
# Excel keeps these definitions inside the program, not in a workbook, so the only way to read them is to
# ask it. For each style and each of its parts (the whole table, the header row, a row stripe, the grand
# total row and so on) this records the fill, the font's color, bold and italic, and each border, with a
# color as a theme color and tint wherever Excel defines it that way, so the style follows the workbook's
# theme as it does in Excel. A part a style leaves undefined is left out. Unluminous draws tables and
# PivotTables from the file this writes (task-2513).

param([Parameter(Mandatory)][string]$Out)
$ErrorActionPreference = 'Stop'
trap { Write-Error "$($_.InvocationInfo.ScriptLineNumber): $($_.Exception.Message)"; break }

# The part names, by Excel's XlTableStyleElementType number, as the file names them in a style's dxfs.
$parts = @{
    0 = 'wholeTable'; 1 = 'headerRow'; 2 = 'totalRow'; 3 = 'firstColumn'; 4 = 'lastColumn'
    5 = 'firstRowStripe'; 6 = 'secondRowStripe'; 7 = 'firstColumnStripe'; 8 = 'secondColumnStripe'
    9 = 'firstHeaderCell'; 10 = 'lastHeaderCell'; 11 = 'firstTotalCell'; 12 = 'lastTotalCell'
    13 = 'firstSubtotalColumn'; 14 = 'secondSubtotalColumn'; 15 = 'thirdSubtotalColumn'
    16 = 'firstSubtotalRow'; 17 = 'secondSubtotalRow'; 18 = 'thirdSubtotalRow'; 19 = 'blankRow'
    20 = 'firstColumnSubheading'; 21 = 'secondColumnSubheading'; 22 = 'thirdColumnSubheading'
    23 = 'firstRowSubheading'; 24 = 'secondRowSubheading'; 25 = 'thirdRowSubheading'
    26 = 'pageFieldLabels'; 27 = 'pageFieldValues'
    28 = 'unselectedItemWithData'; 29 = 'unselectedItemWithNoData'; 30 = 'selectedItemWithData'
    31 = 'selectedItemWithNoData'; 32 = 'hoveredUnselectedItemWithData'; 33 = 'hoveredSelectedItemWithData'
    34 = 'hoveredUnselectedItemWithNoData'; 35 = 'hoveredSelectedItemWithNoData'
}
# Excel's ThemeColor through COM is the file's theme index plus one: 1 is lt1 (white in the Office theme)
# and 2 is dk1 (black), which the rgb Excel resolves each one to confirms. Its documented names, Dark1 for
# 1 and Light1 for 2, are the other way round.
$themeIndex = @{}; foreach ($n in 1..12) { $themeIndex[$n] = $n - 1 }
$edges = @{ 7 = 'left'; 8 = 'top'; 9 = 'bottom'; 10 = 'right'; 11 = 'vertical'; 12 = 'horizontal' }
$weights = @{ 1 = 'hair'; 2 = 'thin'; -4138 = 'medium'; 4 = 'thick' }

# A color as {theme, tint} when Excel defines it by the theme, otherwise {rgb}.
# Whether a property Excel answered is set: an undefined one comes back as DBNull, which PowerShell reads as true.
function IsSet($value) { return -not ($null -eq $value -or $value -is [DBNull]) }

# A color as {theme, tint} when Excel defines it by the theme, otherwise {rgb}. Both carry the color Excel
# resolves it to in the default Office theme as rgb, which the tests compare against.
function ColorOf($thing) {
    if (-not (IsSet $thing.Color)) { return $null }
    $bgr = [int]$thing.Color
    $rgb = '{0:X2}{1:X2}{2:X2}' -f ($bgr -band 255), (($bgr -shr 8) -band 255), (($bgr -shr 16) -band 255)
    $theme = $null
    try { if (IsSet $thing.ThemeColor) { $theme = [int]$thing.ThemeColor } } catch { }
    if ($theme -and $themeIndex.ContainsKey($theme)) {
        return [ordered]@{ theme = $themeIndex[$theme]; tint = [math]::Round([double]$thing.TintAndShade, 6); rgb = $rgb }
    }
    return [ordered]@{ rgb = $rgb }
}

function PartOf($element) {
    $part = [ordered]@{}
    $interior = $element.Interior
    if ((IsSet $interior.Pattern) -and $interior.Pattern -ne -4142) { $c = ColorOf $interior; if ($c) { $part.fill = $c } }
    $font = $element.Font
    if ((IsSet $font.Color) -and (IsSet $font.ColorIndex) -and $font.ColorIndex -ne -4105) { $c = ColorOf $font; if ($c) { $part.color = $c } }
    if ((IsSet $font.Bold) -and $font.Bold -eq $true) { $part.bold = $true }
    if ((IsSet $font.Italic) -and $font.Italic -eq $true) { $part.italic = $true }
    $lines = [ordered]@{}
    foreach ($e in 7..12) {
        $b = $element.Borders.Item($e)
        if (-not (IsSet $b.LineStyle) -or $b.LineStyle -eq -4142) { continue }
        $style = if ($b.LineStyle -eq -4119) { 'double' } elseif ((IsSet $b.Weight) -and $weights.ContainsKey([int]$b.Weight)) { $weights[[int]$b.Weight] } else { 'thin' }
        $line = ColorOf $b
        if (-not $line) { $line = [ordered]@{} }
        $line.style = $style
        $lines[$edges[$e]] = $line
    }
    if ($lines.Count) { $part.borders = $lines }
    return $part
}

$app = New-Object -ComObject Excel.Application
$app.DisplayAlerts = $false
try {
    $book = $app.Workbooks.Add()
    $styles = [ordered]@{}
    foreach ($style in $book.TableStyles) {
        if (-not $style.BuiltIn) { continue }
        if (-not ($style.ShowAsAvailableTableStyle -or $style.ShowAsAvailablePivotTableStyle -or $style.ShowAsAvailableSlicerStyle)) { continue }
        $entry = [ordered]@{}
        # A slicer style has the whole slicer, its header and the item parts; the others stop at 27.
        $last = if ($style.ShowAsAvailableSlicerStyle) { 35 } else { 27 }
        foreach ($n in 0..$last) {
            if ($style.ShowAsAvailableSlicerStyle -and $n -gt 1 -and $n -lt 28) { continue }
            $element = $null
            try { $element = $style.TableStyleElements.Item($n) } catch { continue }
            if (-not $element.HasFormat) { continue }
            $part = PartOf $element
            if ($n -in 5, 6, 7, 8 -and $element.StripeSize -gt 1) { $part.size = [int]$element.StripeSize }
            if ($part.Count) { $entry[$parts[$n]] = $part }
        }
        $styles[$style.Name] = $entry
    }
    $book.Close($false)
    $json = $styles | ConvertTo-Json -Depth 8
    [IO.File]::WriteAllText($Out, $json + "`n", [Text.UTF8Encoding]::new($false))
    Write-Output "$($styles.Count) styles written to $Out"
} finally {
    $app.Quit()
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($app)
}
