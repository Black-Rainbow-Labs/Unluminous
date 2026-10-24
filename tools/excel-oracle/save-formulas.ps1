# Enter formulas into Excel and save the workbook, to see how Excel writes each one into a file.
#
# Reads formulas from a file, one a line, without the leading `=`. The first goes in A1, the next in A2, and so
# on, entered with Formula2 as a person types them. Cells B1:H10 can be filled first with `-Setup <file>`, the same
# file of `A1=value` lines evaluate.ps1 reads (its cells are placed on the same sheet, so formulas can refer to them).
# The workbook is saved to `-Out`, and the sheet's `<f>` elements can then be read from the zip.
#
#   pwsh tools/excel-oracle/save-formulas.ps1 -Formulas formulas.txt -Out book.xlsx [-Setup data.txt]
param(
    [Parameter(Mandatory = $true)][string]$Formulas,
    [Parameter(Mandatory = $true)][string]$Out,
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
            $sheet.Range($line.Substring(0, $at)).Formula2 = $line.Substring($at + 1)
        }
    }
    $row = 0
    foreach ($formula in Get-Content -LiteralPath $Formulas -Encoding UTF8) {
        if (-not $formula) { continue }
        $row++
        try {
            $sheet.Range("A$row").Formula2 = '=' + $formula
        } catch {
            [pscustomobject]@{ formula = $formula; refused = $_.Exception.Message } | ConvertTo-Json -Compress
        }
    }
    $book.Application.Calculate()
    $full = [IO.Path]::GetFullPath($Out)
    if (Test-Path -LiteralPath $full) { Remove-Item -LiteralPath $full }
    $book.SaveAs($full, 51)
    $book.Close($false)
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
