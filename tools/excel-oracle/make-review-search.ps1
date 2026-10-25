# Build the workbook Find and Replace are tested against, and ask Excel what each search finds.
#
# `search.xlsx` is a grid of text, numbers in several formats, formulas, errors, merged and hidden cells,
# formatted cells, notes and a threaded comment. `search-expected.json` has one row for every combination
# of search text, Look in, Look at, match case and search order, with the cells Range.Find and FindNext
# visit, in the order they visit them; rows for searching by format; and the result of Replace for a list
# of replacements, as the cells that differ afterwards.
#
#   pwsh tools/excel-oracle/make-review-search.ps1 [-Folder crates/unluminous-sheet/tests/fixtures/review]
param([string]$Folder = 'crates/unluminous-sheet/tests/fixtures/review')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
New-Item -ItemType Directory -Force -Path $Folder | Out-Null
$folder = (Resolve-Path -LiteralPath $Folder).Path
$path = Join-Path $folder 'search.xlsx'
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false

function Put($sheet, $address, $content) { $sheet.Range($address).Formula = $content }
function Build-Data($book) {
    $a = $book.Worksheets.Item(1)
    $a.Name = 'Sheet1'
    $b = $book.Worksheets.Item(2)
    $b.Name = 'Sheet2'
    foreach ($pair in @(
            @('A1', 'Apple'), @('B1', 'apple pie'), @('C1', 'Banana'), @('D1', 'APPLE'), @('E1', 'pineapple'),
            @('A2', '1234.5'), @('B2', '0.5'), @('C2', '2026-10-24'), @('D2', '=A2*2'), @('E2', '=UPPER(A1)'),
            @('A3', '=TRUE'), @('B3', '=NA()'), @('D3', 'what?'), @('E3', '~tilde'),
            @('A4', '=""'), @('B4', '=A1&" x"'), @('D4', '100'), @('E4', '1,234'),
            @('B5', '  spaced  '), @('C5', '-5'), @('D5', '1E+20'), @('E5', '0.000001'),
            @('A6', 'Hidden apple'), @('F1', 'apple hidden col'),
            @('G1', 'Merged apple'),
            @('B7', 'Bold apple'), @('C7', 'Red fill apple'), @('D7', 'Italic apple'), @('E7', '5'))) {
        Put $a $pair[0] $pair[1]
    }
    $a.Range('C3').NumberFormat = '@'
    $a.Range('C3').Value2 = 'a*b'
    $a.Range('C4').NumberFormat = '@'
    $a.Range('C4').Value2 = '100'
    $a.Range('A5').Value2 = "multi`nline apple"
    $a.Range('A2').NumberFormat = '#,##0.00'
    $a.Range('B2').NumberFormat = '0%'
    $a.Range('C2').NumberFormat = 'yyyy-mm-dd'
    $a.Range('D2').NumberFormat = '0.0'
    $a.Range('C5').NumberFormat = '0;[Red]-0'
    $a.Range('B7').Font.Bold = $true
    $a.Range('C7').Interior.Color = 0x00FFFF
    $a.Range('D7').Font.Italic = $true
    $a.Range('E7').Font.Bold = $true
    $a.Range('E7').NumberFormat = '0.00'
    $a.Range('G1:H2').Merge()
    $a.Rows.Item(6).Hidden = $true
    $a.Columns.Item(6).Hidden = $true
    $a.Range('A9').AddComment('apple note') | Out-Null
    $a.Range('B9').AddComment('Banana note') | Out-Null
    $t = $a.Range('C9').AddCommentThreaded('apple thread')
    $t.AddReply('banana reply') | Out-Null
    Put $b 'A1' 'apple on sheet 2'
    Put $b 'B2' '=Sheet1!A1'
    Put $b 'C3' 'Banana'
}

function Visit($sheet, $what, $lookIn, $lookAt, $order, $case, $format) {
    # A search by format only looks in the used range: every empty cell of the sheet has a format too.
    # (Assigned inside the branches: PowerShell would unroll a Range returned from an if expression.)
    if ($format) { $cells = $sheet.UsedRange } else { $cells = $sheet.Cells }
    $found = $cells.Find($what, [Type]::Missing, $lookIn, $lookAt, $order, 1, $case, $false, $format)
    $hits = @()
    if ($null -eq $found) { return $hits }
    $first = $found.Address($false, $false)
    while ($true) {
        $address = $found.Address($false, $false)
        if ($hits -contains "$($sheet.Name)!$address") { break }
        $hits += "$($sheet.Name)!$address"
        $found = $cells.FindNext($found)
        if ($null -eq $found) { break }
    }
    return $hits
}

try {
    $book = $excel.Workbooks.Add()
    while ($book.Worksheets.Count -lt 2) { $book.Worksheets.Add([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count)) | Out-Null }
    Add-Content -LiteralPath (Join-Path $env:TEMP 'review-search-progress.txt') -Value 'build'
    Build-Data $book
    Add-Content -LiteralPath (Join-Path $env:TEMP 'review-search-progress.txt') -Value 'built'
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
    $book.SaveAs($path, 51)

    $whats = @('apple', 'app*', '?pple', 'a~*b', '1234', '1,234', '50%', '#N/A', '*', '5')
    $lookIns = @(@('formulas', -4123), @('values', -4163), @('notes', -4144))
    $queries = @()
    $id = 0
    $watch = [Diagnostics.Stopwatch]::StartNew()
    foreach ($what in $whats) {
        foreach ($li in $lookIns) {
            foreach ($at in @(@('part', 2), @('whole', 1))) {
                foreach ($case in $false, $true) {
                    foreach ($order in @(@('rows', 1), @('columns', 2))) {
                        $hits = @()
                        foreach ($sheet in $book.Worksheets) { $hits += Visit $sheet $what $li[1] $at[1] $order[1] $case $false }
                        Add-Content -LiteralPath (Join-Path $env:TEMP 'review-search-progress.txt') -Value "$id $what $($li[0]) $($at[0]) $case $($order[0]) $($watch.Elapsed.TotalSeconds)"
                        $queries += [pscustomobject]@{ id = $id++; what = $what; lookIn = $li[0]; lookAt = $at[0]; matchCase = $case; order = $order[0]; hits = $hits }
                    }
                }
            }
        }
    }

    Add-Content -LiteralPath (Join-Path $env:TEMP 'review-search-progress.txt') -Value 'formats'
    # Searching by format: the format of a template cell, with and without text to find.
    $formats = @()
    $a = $book.Worksheets.Item(1)
    foreach ($tpl in 'B7', 'C7', 'D7', 'E7', 'A2', 'B2', 'C2', 'C5') {
        $cell = $a.Range($tpl)
        foreach ($what in '', 'apple', '5') {
            $excel.FindFormat.Clear()
            $excel.FindFormat.NumberFormat = $cell.NumberFormat
            $excel.FindFormat.Font.Name = $cell.Font.Name
            $excel.FindFormat.Font.Size = $cell.Font.Size
            $excel.FindFormat.Font.Bold = $cell.Font.Bold
            $excel.FindFormat.Font.Italic = $cell.Font.Italic
            $excel.FindFormat.Interior.Pattern = $cell.Interior.Pattern
            if ($cell.Interior.Pattern -ne -4142) { $excel.FindFormat.Interior.Color = $cell.Interior.Color }
            $hits = @()
            foreach ($sheet in $book.Worksheets) { $hits += Visit $sheet $what -4123 2 1 $false $true }
            $formats += [pscustomobject]@{ template = $tpl; what = $what; hits = $hits }
        }
    }
    $excel.FindFormat.Clear()
    $book.Close($false)

    Add-Content -LiteralPath (Join-Path $env:TEMP 'review-search-progress.txt') -Value 'replace'
    # Replace: each on a fresh copy, and the cells that differ afterwards.
    $replacements = @(
        @('apple', 'pear', 'part', $false), @('apple', 'pear', 'whole', $false), @('Apple', 'pear', 'part', $true),
        @('app*', 'X', 'part', $false), @('?pple', 'Y', 'part', $false), @('1234', '99', 'part', $false),
        @('a~*b', 'ab', 'whole', $false), @('A2', 'B2', 'part', $false), @('e', 'E', 'part', $true),
        @('*', 'all', 'whole', $false), @('banana', 'plum', 'part', $false), @('pin*e', 'X', 'part', $false),
        @('5', '6', 'whole', $false), @('0.5', '0.25', 'part', $false), @('TRUE', 'FALSE', 'whole', $false)
    )
    $replaced = @()
    foreach ($r in $replacements) {
        Add-Content -LiteralPath (Join-Path $env:TEMP 'review-search-progress.txt') -Value "replace $($r[0])"
        $copy = $excel.Workbooks.Open($path)
        $before = @{}
        $s1 = $copy.Worksheets.Item(1)
        foreach ($sheet in $copy.Worksheets) {
            foreach ($cell in $sheet.UsedRange.Cells) { $before["$($sheet.Name)!$($cell.Address($false,$false))"] = $cell.Formula }
        }
        $lookAt = if ($r[2] -eq 'whole') { 1 } else { 2 }
        $count = 0
        foreach ($sheet in $copy.Worksheets) { $sheet.Cells.Replace($r[0], $r[1], $lookAt, 1, $r[3]) | Out-Null }
        $changes = [ordered]@{}
        foreach ($sheet in $copy.Worksheets) {
            foreach ($cell in $sheet.UsedRange.Cells) {
                $key = "$($sheet.Name)!$($cell.Address($false,$false))"
                if ($before[$key] -ne $cell.Formula) { $changes[$key] = $cell.Formula }
            }
        }
        $replaced += [pscustomobject]@{ what = $r[0]; with = $r[1]; lookAt = $r[2]; matchCase = $r[3]; changes = $changes }
        $copy.Close($false)
    }
    $json = [pscustomobject]@{ queries = $queries; formats = $formats; replacements = $replaced } | ConvertTo-Json -Depth 6 -Compress
    Set-Content -LiteralPath (Join-Path $folder 'search-expected.json') -Value $json -Encoding utf8
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
