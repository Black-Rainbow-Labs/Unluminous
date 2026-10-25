# Evaluate formulas in Excel and print what Excel answers, one JSON object a line.
#
# Reads formulas from a file, one a line, without the leading `=`. Each is entered into A1 of a fresh sheet
# with Formula2, so it is a dynamic array formula the way a formula typed into Excel 365 is, and the answer
# is the value of A1 at full precision and the text Excel shows. Cells B1:H10 can be filled first with
# `-Setup <file>`, a file of `A1=value` lines, so a formula can refer to data.
#
#   pwsh tools/excel-oracle/evaluate.ps1 -Formulas formulas.txt [-Setup data.txt]
param(
    [Parameter(Mandatory = $true)][string]$Formulas,
    [string]$Setup
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $book = $excel.Workbooks.Add()
    $sheet = $book.Worksheets.Item(1)
    if ($Setup) {
        foreach ($line in Get-Content -LiteralPath $Setup -Encoding UTF8) {
            if (-not $line) { continue }
            $at = $line.IndexOf('=')
            $cell = $line.Substring(0, $at)
            $value = $line.Substring($at + 1)
            $sheet.Range($cell).Formula2 = $value
        }
    }
    # Wide enough that an error such as #CONNECT! is shown as itself rather than as ####.
    $sheet.Range('Z1').EntireColumn.ColumnWidth = 60
    foreach ($formula in Get-Content -LiteralPath $Formulas -Encoding UTF8) {
        if (-not $formula -or $formula.StartsWith("#!")) { continue }
        $target = $sheet.Range('Z100')
        try {
            $target.Formula2 = '=' + $formula
            $book.Application.Calculate()
            $value = $target.Value2
            $text = $target.Text
            $kind = if ($null -eq $value) { 'empty' } elseif ($value -is [double]) { 'number' } elseif ($value -is [bool]) { 'bool' } elseif ($value -is [int]) { 'error' } else { 'text' }
            $shown = if ($kind -eq 'number') { $value.ToString('R', [Globalization.CultureInfo]::InvariantCulture) } elseif ($kind -eq 'error') { $text } else { "$value" }
            $spill = $target.HasSpill
            $size = if ($spill) { "$($target.SpillingToRange.Rows.Count)x$($target.SpillingToRange.Columns.Count)" } else { '' }
            [pscustomobject]@{ formula = $formula; kind = $kind; value = $shown; text = $text; spill = $size } | ConvertTo-Json -Compress
        } catch {
            [pscustomobject]@{ formula = $formula; kind = 'refused'; value = $_.Exception.Message; text = ''; spill = '' } | ConvertTo-Json -Compress
        }
        $target.Clear() | Out-Null
    }
    $book.Close($false)
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
