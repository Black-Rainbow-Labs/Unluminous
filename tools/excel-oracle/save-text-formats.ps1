# Have Excel save workbooks in the text formats, so the CSV, text, PRN, XML and HTML writers and readers are
# tested against what Excel writes.
#
# For every workbook in -Source (and an edge case workbook this script builds, `edge.xlsx`), Excel saves:
#   <name>.utf8.csv   CSV UTF-8 (FileFormat 62)
#   <name>.ansi.csv   CSV, the code page (6)
#   <name>.unicode.txt  Unicode Text (42): UTF-16, tab separated
#   <name>.tab.txt    Text (Tab delimited), the code page (-4158)
#   <name>.prn        Formatted Text, space delimited (36)
#   <name>.xml        XML Spreadsheet 2003 (46)
#   <name>.html       Web Page (44)
# Then it opens each .prn and .txt Excel wrote and writes what Excel reads back, cell by cell, to
# <name>.prn.reads.tsv, which is the oracle for the fixed width reader.
#
#   pwsh tools/excel-oracle/save-text-formats.ps1 -Source <folder with .xlsx> [-Out crates/unluminous-sheet/tests/fixtures/text]
param(
    [Parameter(Mandatory = $true)][string]$Source,
    [string]$Out = 'crates/unluminous-sheet/tests/fixtures/text',
    [switch]$SkipEdge
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$out = (Resolve-Path -LiteralPath $Out).Path
$source = (Resolve-Path -LiteralPath $Source).Path
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$excel.AutomationSecurity = 3

# A workbook with one of every kind of value, in columns of different widths, for the writers to be held to.
function New-EdgeWorkbook([string]$Path) {
    $book = $excel.Workbooks.Add()
    $ws = $book.Worksheets.Item(1)
    $ws.Name = 'Edge'
    $headers = 'Text', 'Number', 'Date', 'Percent', 'Currency', 'Accounting', 'Time', 'Bool', 'Error', 'Formula', 'Wide', 'Narrow'
    for ($i = 0; $i -lt $headers.Count; $i++) { $ws.Cells.Item(1, $i + 1).Value2 = $headers[$i] }
    $ws.Range('A2').Value2 = 'plain'
    $ws.Range('B2').Value2 = 42
    $ws.Range('C2').Value2 = 46318; $ws.Range('C2').NumberFormat = 'yyyy-mm-dd'
    $ws.Range('D2').Value2 = 0.256; $ws.Range('D2').NumberFormat = '0.0%'
    $ws.Range('E2').Value2 = 1234.5; $ws.Range('E2').NumberFormat = '$#,##0.00'
    $ws.Range('F2').Value2 = 1234.5; $ws.Range('F2').NumberFormat = '_($* #,##0.00_);_($* (#,##0.00);_($* "-"??_);_(@_)'
    $ws.Range('G2').Value2 = 0.5; $ws.Range('G2').NumberFormat = 'h:mm AM/PM'
    $ws.Range('H2').Value2 = $true
    $ws.Range('I2').Formula = '=1/0'
    $ws.Range('J2').Formula = '=A2&"x"'
    $ws.Range('K2').Formula = '=1/3'
    $ws.Range('L2').Value2 = 0.5
    $ws.Range('A3').NumberFormat = '@'; $ws.Range('A3').Value2 = '007'
    $ws.Range('B3').Value2 = -17.25
    $ws.Range('C3').Value2 = 46027; $ws.Range('C3').NumberFormat = 'm/d/yyyy'
    $ws.Range('D3').Value2 = -0.5; $ws.Range('D3').NumberFormat = '0%'
    $ws.Range('E3').Value2 = -5; $ws.Range('E3').NumberFormat = '$#,##0.00;[Red]($#,##0.00)'
    $ws.Range('F3').Value2 = -5; $ws.Range('F3').NumberFormat = '_($* #,##0.00_);_($* (#,##0.00);_($* "-"??_);_(@_)'
    $ws.Range('G3').Value2 = 0.999; $ws.Range('G3').NumberFormat = 'h:mm:ss'
    $ws.Range('H3').Value2 = $false
    $ws.Range('I3').Formula = '=NA()'
    $ws.Range('J3').Formula = '=1/3'
    $ws.Range('K3').Formula = '=PI()*100000'
    $ws.Range('L3').Value2 = 12345
    $ws.Range('A4').Value2 = 'a,b'
    $ws.Range('B4').Value2 = 1.23456789012345E+20
    $ws.Range('C4').Value2 = 1; $ws.Range('C4').NumberFormat = 'd-mmm-yy'
    $ws.Range('D4').Value2 = 1E-10; $ws.Range('D4').NumberFormat = '0.00%'
    $ws.Range('E4').Value2 = 12345678.9; $ws.Range('E4').NumberFormat = '#,##0.0'
    $ws.Range('F4').Value2 = 0; $ws.Range('F4').NumberFormat = '_($* #,##0.00_);_($* (#,##0.00);_($* "-"??_);_(@_)'
    $ws.Range('G4').Value2 = 1.5; $ws.Range('G4').NumberFormat = '[h]:mm'
    $ws.Range('K4').Value2 = 1234567.891
    $ws.Range('L4').Value2 = 46318; $ws.Range('L4').NumberFormat = 'yyyy-mm-dd'
    $ws.Range('A5').Value2 = 'say "hi"'
    $ws.Range('B5').Formula = '=0.1+0.2'
    $ws.Range('C5').Value2 = 60; $ws.Range('C5').NumberFormat = 'm/d/yyyy'
    $ws.Range('K5').Value2 = 0.000012345
    $ws.Range('L5').Value2 = 0.123456789
    $ws.Range('A6').Value2 = "line1`nline2"
    $ws.Range('B6').Value2 = 123456789012
    $ws.Range('K6').Value2 = 123456789.123456789
    $ws.Range('L6').Value2 = -1234.5678
    $ws.Range('A7').Value2 = '  pad  '
    $ws.Range('B7').Value2 = 1E+15
    $ws.Range('K7').Value2 = 1E-5
    $ws.Range('A8').Value2 = "'=1+1"
    $ws.Range('B8').Value2 = 0.00001234
    $ws.Range('K8').Value2 = 100
    $ws.Range('A9').Value2 = [string]::Concat([char]0xE9, [char]0x20AC, [char]0x65E5, [char]0x672C, [char]0x8A9E)
    $ws.Range('B9').Value2 = 99999999999
    $ws.Range('A10').Value2 = "tab`there"
    $ws.Range('B10').Value2 = 0.5
    $ws.Range('A12').Value2 = 'after a blank row'
    $ws.Columns.Item(11).ColumnWidth = 30
    $ws.Columns.Item(12).ColumnWidth = 4
    $book.SaveAs($Path, 51)
    $book.Close($false)
}

# Open a text file the way Excel would and write what it reads, one cell a line.
function Save-Reads([string]$Path, [string]$Target) {
    $book = $excel.Workbooks.Open($Path, 0, $true)
    $ws = $book.Worksheets.Item(1)
    $lines = @()
    foreach ($c in $ws.UsedRange.Cells) {
        $v = $c.Value2
        if ($null -eq $v) { continue }
        $kind = if ($v -is [double]) { 'number' } elseif ($v -is [bool]) { 'bool' } elseif ($v -is [int]) { 'error' } else { 'text' }
        $shown = if ($kind -eq 'number') { $v.ToString('R', [Globalization.CultureInfo]::InvariantCulture) } else { "$v" }
        $lines += ('{0}`t{1}`t{2}`t{3}' -f $c.Address($false, $false), $kind, ($shown -replace "`n", '\n'), $c.NumberFormat)
    }
    Set-Content -LiteralPath $Target -Value $lines -Encoding UTF8
    $book.Close($false)
}

try {
    if (-not $SkipEdge -and -not (Test-Path (Join-Path $out 'edge.xlsx'))) { New-EdgeWorkbook (Join-Path $out 'edge.xlsx') }
    $files = @(Get-ChildItem -LiteralPath $source -Filter *.xlsx)
    if (Test-Path (Join-Path $out 'edge.xlsx')) { $files += Get-Item (Join-Path $out 'edge.xlsx') }
    $formats = @(
        @{ Suffix = 'utf8.csv'; Code = 62 }, @{ Suffix = 'ansi.csv'; Code = 6 }, @{ Suffix = 'unicode.txt'; Code = 42 },
        @{ Suffix = 'tab.txt'; Code = -4158 }, @{ Suffix = 'prn'; Code = 36 }, @{ Suffix = 'xml'; Code = 46 }, @{ Suffix = 'html'; Code = 44 }
    )
    foreach ($file in $files) {
        $name = [IO.Path]::GetFileNameWithoutExtension($file.Name)
        foreach ($f in $formats) {
            $book = $excel.Workbooks.Open($file.FullName, 0, $true)
            $book.Worksheets.Item(1).Activate() | Out-Null
            $target = Join-Path $out "$name.$($f.Suffix)"
            $book.SaveAs($target, $f.Code)
            $book.Close($false)
            Write-Host "saved $name.$($f.Suffix)"
        }
        Save-Reads (Join-Path $out "$name.prn") (Join-Path $out "$name.prn.reads.tsv")
    }
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
