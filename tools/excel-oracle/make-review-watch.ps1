# Build a workbook with cells in the Watch Window, and say what Excel lists for them. `watch.xlsx` is the
# workbook and `watch-expected.json` is Excel's Watches list: the sheet, the cell, and what Excel shows.
# With -Read it opens the workbook named by -Path and prints that list instead.
#
#   pwsh tools/excel-oracle/make-review-watch.ps1 [-Folder crates/unluminous-sheet/tests/fixtures/review]
#   pwsh tools/excel-oracle/make-review-watch.ps1 -Read -Path some.xlsx
param([string]$Folder = 'crates/unluminous-sheet/tests/fixtures/review', [switch]$Read, [string]$Path)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
function Watch-List($book) {
    $list = @()
    foreach ($w in $excel.Watches) { $list += $w.Source.Parent.Name + '!' + $w.Source.Address($false, $false) }
    return $list
}
try {
    if ($Read) {
        $book = $excel.Workbooks.Open((Resolve-Path -LiteralPath $Path).Path, 0, $true)
        ConvertTo-Json -Compress -InputObject @(Watch-List $book)
        $book.Close($false)
        return
    }
    New-Item -ItemType Directory -Force -Path $Folder | Out-Null
    $folder = (Resolve-Path -LiteralPath $Folder).Path
    $book = $excel.Workbooks.Add()
    while ($book.Worksheets.Count -lt 2) { $book.Worksheets.Add([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count)) | Out-Null }
    $s = $book.Worksheets.Item(1)
    $t = $book.Worksheets.Item(2)
    $s.Range('A1').Value2 = 3
    $s.Range('B2').Formula = '=A1*2'
    $t.Range('A1').Formula = '=Sheet1!B2+1'
    $book.Names.Add('Doubled', '=Sheet1!$B$2') | Out-Null
    $excel.Watches.Add($s.Range('B2')) | Out-Null
    $excel.Watches.Add($t.Range('A1')) | Out-Null
    $excel.Watches.Add($s.Range('A1')) | Out-Null
    $path = Join-Path $folder 'watch.xlsx'
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
    $book.SaveAs($path, 51)
    $json = ConvertTo-Json -Compress -InputObject @(Watch-List $book)
    Set-Content -LiteralPath (Join-Path $folder 'watch-expected.json') -Value $json -Encoding utf8
    $book.Close($false)
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
