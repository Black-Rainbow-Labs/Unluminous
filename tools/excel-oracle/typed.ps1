# What Excel does with text typed into a cell: Range.Formula = text, then Value2, NumberFormat, Text.
param([Parameter(Mandatory = $true)][string]$Inputs)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$items = Get-Content -LiteralPath $Inputs -Raw -Encoding UTF8 | ConvertFrom-Json
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $book = $excel.Workbooks.Add()
    $sheet = $book.Worksheets.Item(1)
    foreach ($item in $items) {
        $cell = $sheet.Range('A1')
        $cell.Clear() | Out-Null
        $cell.NumberFormat = 'General'
        try {
            $cell.Formula = [string]$item
            $value = $cell.Value2
            $kind = if ($null -eq $value) { 'empty' } elseif ($value -is [double]) { 'number' } elseif ($value -is [bool]) { 'bool' } elseif ($value -is [int]) { 'error' } else { 'text' }
            $text = $cell.Text
            $shown = if ($kind -eq 'number') { $value.ToString('R', [Globalization.CultureInfo]::InvariantCulture) } elseif ($kind -eq 'error') { $text } else { "$value" }
            [pscustomobject]@{ input = $item; kind = $kind; value = $shown; numberformat = $cell.NumberFormat; text = $text; hasformula = [bool]$cell.HasFormula; prefix = $cell.PrefixCharacter } | ConvertTo-Json -Compress
        } catch {
            [pscustomobject]@{ input = $item; kind = 'refused'; value = $_.Exception.Message; numberformat = ''; text = ''; hasformula = $false; prefix = '' } | ConvertTo-Json -Compress
        }
    }
    $book.Close($false)
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
