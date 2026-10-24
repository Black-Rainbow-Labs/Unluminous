# Have Excel read a text file as fixed width columns with the column breaks it guesses itself, the way its text
# import wizard does, and write what it reads: one cell a line, `address<TAB>kind<TAB>value<TAB>format`.
#
#   pwsh tools/excel-oracle/open-fixed-width.ps1 -Path file.prn -Out file.fixed.tsv
param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Out
)
$ErrorActionPreference = 'Stop'
$full = (Resolve-Path -LiteralPath $Path).Path
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $book = $excel.Workbooks.Add()
    $ws = $book.Worksheets.Item(1)
    # A query table with the fixed width type and no widths asks Excel to find the columns itself.
    $query = $ws.QueryTables.Add("TEXT;$full", $ws.Range('A1'))
    $query.TextFilePlatform = 65001
    $query.TextFileParseType = 2   # xlFixedWidth
    $query.TextFileColumnDataTypes = ,1 * 40
    $query.AdjustColumnWidth = $false
    $query.Refresh($false) | Out-Null
    $lines = @()
    foreach ($c in $ws.UsedRange.Cells) {
        $v = $c.Value2
        if ($null -eq $v) { continue }
        $kind = if ($v -is [double]) { 'number' } elseif ($v -is [bool]) { 'bool' } elseif ($v -is [int]) { 'error' } else { 'text' }
        $shown = if ($kind -eq 'number') { $v.ToString('R', [Globalization.CultureInfo]::InvariantCulture) } else { "$v" }
        $lines += ("{0}`t{1}`t{2}`t{3}" -f $c.Address($false, $false), $kind, ($shown -replace "`n", '\n'), $c.NumberFormat)
    }
    Set-Content -LiteralPath $Out -Value $lines -Encoding UTF8
    $book.Close($false)
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
