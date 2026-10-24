# Open a workbook in Excel and print what Excel sees, as JSON.
#
# Answers whether Excel had to repair the file, and for every sheet the cells of its used range: the value
# at full precision, the formula as Excel shows it, the text Excel draws and the number format. With
# -Recalculate, Excel computes every formula first, so the values are Excel's own rather than the values
# saved in the file.
#
#   pwsh tools/excel-oracle/inspect.ps1 -Path book.xlsx [-Recalculate] [-SaveAs copy.xlsx]
param(
    [Parameter(Mandatory = $true)][string]$Path,
    [switch]$Recalculate,
    [string]$SaveAs
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$full = (Resolve-Path -LiteralPath $Path).Path
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$excel.AutomationSecurity = 3  # never run macros
try {
    # A file Excel would offer to repair makes Open fail when alerts are off; opening it again with
    # CorruptLoad set to repair is how the repair is observed.
    $repaired = $false
    $problem = ''
    try {
        $book = $excel.Workbooks.Open($full, 0, $true)
    } catch {
        $repaired = $true
        $problem = $_.Exception.Message
        # Every argument up to CorruptLoad, which is 1, repair.
        $book = $excel.Workbooks.Open($full, 0, $true, 5, '', '', $true, 2, '', $false, $false, 0, $false, $false, 1)
    }
    $caption = $book.Windows.Item(1).Caption
    $repaired = $repaired -or ($caption -match 'Repaired')
    if ($Recalculate) { $excel.CalculateFull() }
    $sheets = @()
    foreach ($sheet in $book.Worksheets) {
        $cells = @()
        $used = $sheet.UsedRange
        foreach ($cell in $used.Cells) {
            $value = $cell.Value2
            if ($null -eq $value -and -not $cell.HasFormula) { continue }
            $kind = if ($null -eq $value) { 'empty' } elseif ($value -is [double]) { 'number' } elseif ($value -is [bool]) { 'bool' } elseif ($value -is [int]) { 'error' } else { 'text' }
            $shown = if ($kind -eq 'number') { $value.ToString('R', [Globalization.CultureInfo]::InvariantCulture) } elseif ($kind -eq 'error') { $cell.Text } else { "$value" }
            $cells += [pscustomobject]@{
                cell = $cell.Address($false, $false)
                kind = $kind
                value = $shown
                formula = if ($cell.HasFormula) { $cell.Formula2 } else { '' }
                text = $cell.Text
                format = $cell.NumberFormat
            }
        }
        $merged = @()
        foreach ($cell in $used.Cells) { if ($cell.MergeCells -and $cell.MergeArea.Cells.Item(1).Address($false, $false) -eq $cell.Address($false, $false)) { $merged += $cell.MergeArea.Address($false, $false) } }
        $sheet.Activate() | Out-Null
        $window = $excel.ActiveWindow
        $links = @(); foreach ($h in $sheet.Hyperlinks) { $links += "$($h.Range.Address($false, $false))|$($h.Address)|$($h.SubAddress)|$($h.TextToDisplay)" }
        $notes = @(); foreach ($c in $sheet.Comments) { $notes += "$($c.Parent.Address($false, $false))|$($c.Text())" }
        $tables = @(); foreach ($t in $sheet.ListObjects) { $tables += "$($t.Name)|$($t.Range.Address($false, $false))" }
        $charts = @(); foreach ($c in $sheet.ChartObjects()) { $charts += "$($c.Chart.ChartType)|$(if ($c.Chart.HasTitle) { $c.Chart.ChartTitle.Text })" }
        $validations = @()
        try { $checked = $sheet.Cells.SpecialCells(-4174) } catch { $checked = $null }
        if ($checked) { foreach ($cell in $checked.Cells) { $validations += "$($cell.Address($false, $false))|$($cell.Validation.Type)|$($cell.Validation.Formula1)" } }
        $features = [pscustomobject]@{
            visible = $sheet.Visible
            tab = $sheet.Tab.Color
            merged = $merged
            frozen = "$($window.FreezePanes)|$($window.SplitRow)|$($window.SplitColumn)"
            gridlines = $window.DisplayGridlines
            conditional = $used.FormatConditions.Count
            links = $links
            notes = $notes
            tables = $tables
            charts = $charts
            validations = $validations
            hiddenRows = @(1..([Math]::Min($used.Rows.Count + $used.Row, 200)) | Where-Object { $sheet.Rows.Item($_).Hidden })
        }
        $sheets += [pscustomobject]@{ name = $sheet.Name; cells = $cells; features = $features }
    }
    $names = @(); foreach ($n in $book.Names) { $names += "$($n.Name)|$($n.RefersTo)" }
    [pscustomobject]@{ repaired = $repaired; problem = $problem; caption = $caption; names = $names; sheets = $sheets } | ConvertTo-Json -Depth 6 -Compress
    if ($SaveAs) { $book.SaveAs($SaveAs, 51) }
    $book.Close($false)
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
