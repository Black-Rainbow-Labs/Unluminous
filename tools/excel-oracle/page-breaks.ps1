# Record where Excel puts the page breaks, for the paginator's tests.
#
# Builds workbooks with Excel, one sheet for each page setup variant (paper sizes, both orientations, margins,
# scale, fit to pages, print titles, manual breaks, hidden rows and columns, print areas, wide and tall data),
# saves each with Excel, and writes next to it the breaks Excel computed for every sheet: `HPageBreaks`,
# `VPageBreaks` (the cell at which each new page starts, and whether the break is manual or automatic) and
# `PageSetup.Pages.Count`. The workbooks are `pages-<group>.xlsx` and the answers `pages-<group>.json`. A group is
# a default font, because the font decides how wide a column is. `pages-fonts.json` records how wide Excel
# makes a column in each font, which is the table the paginator uses for the width of a character.
#
# Excel computes automatic breaks from the printer driver's printable area, so the answers depend on the
# active printer. This script makes Microsoft Print to PDF the active printer for its own Excel, and records
# the printer's name in every JSON file. It needs a workbook open before the printer can be changed.
#
#   pwsh tools/excel-oracle/page-breaks.ps1 [-Folder crates/unluminous-sheet/tests/fixtures/pages] [-Only calibri]
param(
    [string]$Folder = 'crates/unluminous-sheet/tests/fixtures/pages',
    [string]$Only = ''
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
New-Item -ItemType Directory -Force -Path $Folder | Out-Null
$folder = (Resolve-Path -LiteralPath $Folder).Path
$inv = [Globalization.CultureInfo]::InvariantCulture

# ---- the variants -------------------------------------------------------------------------------------

function Fill($ws, [int]$rows, [int]$cols) {
    $ws.Range($ws.Cells.Item(1, 1), $ws.Cells.Item($rows, $cols)).Value2 = 1.5
}
function Fit($ps, $w, $h) {
    $ps.Zoom = $false
    if ($w -eq 0) { $ps.FitToPagesWide = $false } else { $ps.FitToPagesWide = [int]$w }
    if ($h -eq 0) { $ps.FitToPagesTall = $false } else { $ps.FitToPagesTall = [int]$h }
}
function Margins($ps, $l, $r, $t, $b, $hd, $ft) {
    $ps.LeftMargin = [double]($l * 72); $ps.RightMargin = [double]($r * 72); $ps.TopMargin = [double]($t * 72); $ps.BottomMargin = [double]($b * 72)
    $ps.HeaderMargin = [double]($hd * 72); $ps.FooterMargin = [double]($ft * 72)
}
function Variant([string]$name, [int]$rows, [int]$cols, [scriptblock]$setup, [hashtable]$with = @{}) {
    [pscustomobject]@{ Name = $name; Rows = $rows; Cols = $cols; Setup = $setup; With = $with }
}

# The full list, for the workbook whose default font is Calibri 11.
function Full-Variants {
    $v = @()
    $papers = [ordered]@{ letter = 1; legal = 5; a4 = 9; a3 = 8; tabloid = 3; a5 = 11; executive = 7; b4 = 12 }
    foreach ($p in $papers.Keys) {
        foreach ($o in 1, 2) {
            $v += Variant "paper-$p-$(if ($o -eq 1) { 'p' } else { 'l' })" 130 40 { param($ws, $a) $ws.PageSetup.PaperSize = $a.code; $ws.PageSetup.Orientation = $a.orient } @{ code = $papers[$p]; orient = $o }
        }
    }
    $v += Variant 'margins-narrow' 130 40 { param($ws) Margins $ws.PageSetup 0.25 0.25 0.25 0.25 0.1 0.1 }
    $v += Variant 'margins-wide' 130 40 { param($ws) Margins $ws.PageSetup 1.5 1.5 1.25 1.25 0.5 0.5 }
    $v += Variant 'margins-asym' 130 40 { param($ws) Margins $ws.PageSetup 0.2 1.1 1.7 0.35 0.3 0.15 }
    $v += Variant 'margins-zero' 130 40 { param($ws) Margins $ws.PageSetup 0 0 0 0 0 0 }
    foreach ($z in 10, 25, 50, 75, 100, 125, 150, 200, 400) {
        $v += Variant "scale-$z" 130 40 { param($ws, $a) $ws.PageSetup.Zoom = $a.zoom } @{ zoom = $z }
    }
    foreach ($f in @(@(1, 1), @(1, 2), @(2, 1), @(2, 3), @(3, 2), @(1, 0), @(0, 1), @(2, 0), @(0, 2), @(4, 4), @(3, 3))) {
        $v += Variant "fit-$($f[0])x$($f[1])" 130 40 { param($ws, $a) Fit $ws.PageSetup $a.w $a.h } @{ w = $f[0]; h = $f[1] }
    }
    $v += Variant 'fit-1x1-landscape' 130 40 { param($ws) $ws.PageSetup.Orientation = 2; Fit $ws.PageSetup 1 1 }
    $v += Variant 'fit-1x0-a4' 130 40 { param($ws) $ws.PageSetup.PaperSize = 9; Fit $ws.PageSetup 1 0 }
    $v += Variant 'fit-small-data' 8 5 { param($ws) Fit $ws.PageSetup 2 2 }
    $v += Variant 'titles-rows' 130 40 { param($ws) $ws.PageSetup.PrintTitleRows = '$1:$2' }
    $v += Variant 'titles-rows-tall' 130 40 { param($ws) $ws.PageSetup.PrintTitleRows = '$1:$12' }
    $v += Variant 'titles-rows-mid' 130 40 { param($ws) $ws.PageSetup.PrintTitleRows = '$5:$7' }
    $v += Variant 'titles-cols' 130 40 { param($ws) $ws.PageSetup.PrintTitleColumns = '$A:$B' }
    $v += Variant 'titles-cols-wide' 130 40 { param($ws) $ws.Columns.Item(1).ColumnWidth = 25; $ws.PageSetup.PrintTitleColumns = '$A:$A' }
    $v += Variant 'titles-both' 130 40 { param($ws) $ws.PageSetup.PrintTitleRows = '$1:$2'; $ws.PageSetup.PrintTitleColumns = '$A:$B' }
    $v += Variant 'titles-both-landscape' 130 40 { param($ws) $ws.PageSetup.Orientation = 2; $ws.PageSetup.PrintTitleRows = '$1:$3'; $ws.PageSetup.PrintTitleColumns = '$A:$C' }
    $v += Variant 'titles-fit-2x3' 130 40 { param($ws) $ws.PageSetup.PrintTitleRows = '$1:$2'; Fit $ws.PageSetup 2 3 }
    $v += Variant 'break-row-30' 130 40 { param($ws) [void]$ws.HPageBreaks.Add($ws.Range('A31')) }
    $v += Variant 'break-col-10' 130 40 { param($ws) [void]$ws.VPageBreaks.Add($ws.Range('K1')) }
    $v += Variant 'break-both' 130 40 { param($ws) [void]$ws.HPageBreaks.Add($ws.Range('A21')); [void]$ws.VPageBreaks.Add($ws.Range('G1')) }
    $v += Variant 'break-many' 130 40 { param($ws) foreach ($r in 11, 25, 60, 61, 90) { [void]$ws.HPageBreaks.Add($ws.Range("A$r")) }; foreach ($c in 4, 9) { [void]$ws.VPageBreaks.Add($ws.Cells.Item(1, $c)) } }
    $v += Variant 'break-near-auto' 130 40 { param($ws) [void]$ws.HPageBreaks.Add($ws.Range('A50')); [void]$ws.HPageBreaks.Add($ws.Range('A53')) }
    $v += Variant 'break-after-auto' 130 40 { param($ws) [void]$ws.HPageBreaks.Add($ws.Range('A100')) }
    $v += Variant 'break-with-titles' 130 40 { param($ws) $ws.PageSetup.PrintTitleRows = '$1:$2'; [void]$ws.HPageBreaks.Add($ws.Range('A20')) }
    $v += Variant 'break-fit' 130 40 { param($ws) [void]$ws.HPageBreaks.Add($ws.Range('A20')); Fit $ws.PageSetup 2 3 }
    $v += Variant 'break-scale' 130 40 { param($ws) $ws.PageSetup.Zoom = 60; [void]$ws.HPageBreaks.Add($ws.Range('A45')) }
    $v += Variant 'hidden-rows' 130 40 { param($ws) $ws.Rows.Item('10:50').Hidden = $true }
    $v += Variant 'hidden-cols' 130 40 { param($ws) $ws.Columns.Item('C:J').Hidden = $true }
    $v += Variant 'hidden-both' 130 40 { param($ws) $ws.Rows.Item('5:60').Hidden = $true; $ws.Columns.Item('B:M').Hidden = $true }
    $v += Variant 'hidden-first' 130 40 { param($ws) $ws.Rows.Item('1:3').Hidden = $true; $ws.Columns.Item('A:A').Hidden = $true }
    $v += Variant 'area-single' 130 40 { param($ws) $ws.PageSetup.PrintArea = '$A$1:$L$50' }
    $v += Variant 'area-offset' 130 40 { param($ws) $ws.PageSetup.PrintArea = '$C$10:$R$90' }
    $v += Variant 'area-multiple' 130 40 { param($ws) $ws.PageSetup.PrintArea = '$A$1:$F$20,$H$1:$L$30' }
    $v += Variant 'area-with-titles' 130 40 { param($ws) $ws.PageSetup.PrintArea = '$A$1:$Z$120'; $ws.PageSetup.PrintTitleRows = '$1:$1'; $ws.PageSetup.PrintTitleColumns = '$A:$A' }
    $v += Variant 'area-beyond-data' 20 10 { param($ws) $ws.PageSetup.PrintArea = '$A$1:$AD$150' }
    $v += Variant 'size-widths' 130 40 { param($ws) foreach ($c in 1..40) { $ws.Columns.Item($c).ColumnWidth = 4 + ($c * 7 % 23) } }
    $v += Variant 'size-heights' 130 40 { param($ws) foreach ($r in 1..130) { $ws.Rows.Item($r).RowHeight = 8 + ($r * 5 % 37) } }
    $v += Variant 'size-wide-cols' 100 24 { param($ws) $ws.Columns.Item('A:X').ColumnWidth = 30 }
    $v += Variant 'size-tall-rows' 60 20 { param($ws) $ws.Rows.Item('1:60').RowHeight = 40 }
    foreach ($h in 10, 12.5, 13, 14, 16, 17, 19, 20, 25, 40) {
        $v += Variant "rowheight-$h" 130 12 { param($ws, $a) $ws.Rows.Item('1:130').RowHeight = [double]$a.h } @{ h = $h }
    }
    $v += Variant 'size-tiny' 130 40 { param($ws) $ws.Columns.Item('A:AN').ColumnWidth = 1.5; $ws.Rows.Item('1:130').RowHeight = 6 }
    $v += Variant 'headings' 130 40 { param($ws) $ws.PageSetup.PrintHeadings = $true }
    $v += Variant 'gridlines' 130 40 { param($ws) $ws.PageSetup.PrintGridlines = $true }
    $v += Variant 'headings-gridlines' 130 40 { param($ws) $ws.PageSetup.PrintHeadings = $true; $ws.PageSetup.PrintGridlines = $true }
    $v += Variant 'centered' 130 40 { param($ws) $ws.PageSetup.CenterHorizontally = $true; $ws.PageSetup.CenterVertically = $true }
    $v += Variant 'header-footer' 130 40 { param($ws) $ws.PageSetup.CenterHeader = '&A'; $ws.PageSetup.CenterFooter = 'Page &P of &N'; $ws.PageSetup.LeftHeader = '&"Arial,Bold"&16Title' }
    $v += Variant 'tall-data' 520 1 { param($ws) }
    $v += Variant 'wide-data' 3 220 { param($ws) }
    $v += Variant 'small-data' 10 10 { param($ws) }
    $v += Variant 'one-cell' 1 1 { param($ws) }
    $v += Variant 'order-over' 130 40 { param($ws) $ws.PageSetup.Order = 2 }
    $v += Variant 'first-page-number' 130 40 { param($ws) $ws.PageSetup.FirstPageNumber = 5 }
    return $v
}

# The shorter list run in every other default font.
function Short-Variants {
    $v = @()
    $v += Variant 'letter-p' 130 40 { param($ws) }
    $v += Variant 'a4-l' 130 40 { param($ws) $ws.PageSetup.PaperSize = 9; $ws.PageSetup.Orientation = 2 }
    $v += Variant 'fit-1x1' 130 40 { param($ws) Fit $ws.PageSetup 1 1 }
    $v += Variant 'fit-2x3' 130 40 { param($ws) Fit $ws.PageSetup 2 3 }
    $v += Variant 'scale-70' 130 40 { param($ws) $ws.PageSetup.Zoom = 70 }
    $v += Variant 'titles-both' 130 40 { param($ws) $ws.PageSetup.PrintTitleRows = '$1:$2'; $ws.PageSetup.PrintTitleColumns = '$A:$B' }
    $v += Variant 'widths' 130 40 { param($ws) foreach ($c in 1..40) { $ws.Columns.Item($c).ColumnWidth = 4 + ($c * 7 % 23) } }
    return $v
}

$groups = @(
    @{ Name = 'calibri'; Font = 'Calibri'; Size = 11; Variants = { Full-Variants } },
    @{ Name = 'arial'; Font = 'Arial'; Size = 10; Variants = { Short-Variants } },
    @{ Name = 'times'; Font = 'Times New Roman'; Size = 12; Variants = { Short-Variants } },
    @{ Name = 'aptos-narrow'; Font = 'Aptos Narrow'; Size = 11; Variants = { Short-Variants } },
    @{ Name = 'consolas'; Font = 'Consolas'; Size = 14; Variants = { Short-Variants } }
)

# ---- running them -------------------------------------------------------------------------------------

# The breaks of a collection, by index. `Item($i)` fails with "Invalid index" for a few breaks (for the break after
# the data, it seems), so the ones that can be read are returned with the count Excel gave.
function Break-List($breaks) {
    $list = @()
    $n = [int]$breaks.Count
    for ($i = 1; $i -le $n; $i++) {
        try {
            $b = $breaks.Item($i)
            $loc = $b.Location
            $list += [pscustomobject]@{ row = [int]$loc.Row; col = [int]$loc.Column; manual = ($b.Type -eq -4135) }
        } catch { break }
    }
    return [pscustomobject]@{ count = $n; list = $list }
}

# Excel computes page breaks on its own schedule, and a read straight after a change can return fewer breaks than
# there are pages. Read again until the breaks and the page count agree, up to six times.
function Read-Breaks($ws) {
    $ws.Activate()
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        $pages = [int]$ws.PageSetup.Pages.Count
        $h = Break-List $ws.HPageBreaks
        $v = Break-List $ws.VPageBreaks
        if (@($h.list).Count -eq $h.count -and @($v.list).Count -eq $v.count) { break }
        Start-Sleep -Milliseconds 700
    }
    return [pscustomobject]@{ pages = $pages; hbreaks = @($h.list); vbreaks = @($v.list); hcount = $h.count; vcount = $v.count; attempts = $attempt }
}

# How many columns of one width fit on the first page, found from the first vertical break.
function Columns-Per-Page($ws, [double]$chars) {
    $ws.Columns.Item('A:LZ').ColumnWidth = [double]$chars
    $pixels = [double]$ws.Columns.Item(1).Width / 0.75
    $perPage = 0
    if ([int]$ws.VPageBreaks.Count -ge 1) { $perPage = [int]$ws.VPageBreaks.Item(1).Location.Column - 1 }
    return [pscustomobject]@{ pixels = $pixels; per_page = $perPage }
}

$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $first = $excel.Workbooks.Add()
    $excel.ActivePrinter = 'Microsoft Print to PDF on Ne00:'
    $printer = $excel.ActivePrinter
    $first.Close($false)
    Write-Host "printer: $printer"

    foreach ($g in $groups) {
        if ($Only -and $g.Name -ne $Only) { continue }
        Write-Host "group $($g.Name)"
        $book = $excel.Workbooks.Add()
        $normal = $book.Styles.Item('Normal')
        $normal.Font.Name = $g.Font
        $normal.Font.Size = [double]$g.Size
        while ($book.Worksheets.Count -gt 1) { $book.Worksheets.Item($book.Worksheets.Count).Delete() }
        $variants = & $g.Variants
        $answers = @()
        $index = 0
        foreach ($variant in $variants) {
            $index++
            if ($index -gt 1) { $ws = $book.Worksheets.Add([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count)) } else { $ws = $book.Worksheets.Item(1) }
            $ws.Name = $variant.Name
            Fill $ws $variant.Rows $variant.Cols
            $failure = ''
            try {
                $excel.PrintCommunication = $false
                & $variant.Setup $ws $variant.With
            } catch { $failure = "$_" } finally { $excel.PrintCommunication = $true }
            $ps = $ws.PageSetup
            $answer = [ordered]@{ sheet = $variant.Name; failure = $failure }
            try {
                $read = Read-Breaks $ws
                $answer.pages = $read.pages
                $answer.hbreaks = $read.hbreaks
                $answer.vbreaks = $read.vbreaks
                $answer.hcount = $read.hcount
                $answer.vcount = $read.vcount
                $answer.attempts = $read.attempts
            } catch { $answer.failure = "$failure | read: $_" }
            $answer.paper = [int]$ps.PaperSize
            $answer.orientation = [int]$ps.Orientation
            $answer.zoom = "$($ps.Zoom)"
            $answers += [pscustomobject]$answer
            Write-Host ("  {0,-24} pages={1} h={2} v={3} tries={4} {5}" -f $variant.Name, $answer.pages, @($answer.hbreaks).Count, @($answer.vbreaks).Count, $answer.attempts, $failure)
        }
        $path = Join-Path $folder "pages-$($g.Name).xlsx"
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
        $book.SaveAs($path, 51)
        $book.Close($false)
        $json = [pscustomobject]@{
            printer = $printer; font = $g.Font; size = $g.Size; workbook = "pages-$($g.Name).xlsx"; sheets = $answers
        }
        $json | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $folder "pages-$($g.Name).json") -Encoding UTF8
    }

    if (-not $Only -or $Only -eq 'fonts') {
        Write-Host 'fonts'
        $book = $excel.Workbooks.Add()
        $ws = $book.Worksheets.Item(1)
        $ws.Range('A1:LZ1').Value2 = [double]1.5
        # A second sheet measures the height of a default row: forty rows left at the default height, then
        # rows of 0.75 points, each of which is six printer pixels. The first break says how many of those fit
        # after the forty, which pins the default row's height to within a sixth of a pixel.
        $rowsSheet = $book.Worksheets.Add([Type]::Missing, $ws)
        $rowsSheet.Range('A1:A1500').Value2 = [double]1.5
        $rowsSheet.Rows.Item('41:1500').RowHeight = [double]0.75
        $plain = $book.Worksheets.Add([Type]::Missing, $rowsSheet)
        $fonts = @()
        $names = 'Calibri', 'Arial', 'Times New Roman', 'Aptos Narrow', 'Aptos', 'Segoe UI', 'Verdana', 'Tahoma', 'Consolas', 'Courier New', 'Cambria', 'Georgia', 'Trebuchet MS', 'Comic Sans MS'
        foreach ($name in $names) {
            foreach ($size in 9, 10, 11, 12, 14) {
                try {
                    $normal = $book.Styles.Item('Normal')
                    $normal.Font.Name = $name
                    $normal.Font.Size = [double]$size
                    $ws.Columns.Item(1).ColumnWidth = [double]10
                    $p10 = [double]$ws.Columns.Item(1).Width / 0.75
                    $ws.Columns.Item(1).ColumnWidth = [double]1
                    $p1 = [double]$ws.Columns.Item(1).Width / 0.75
                    $perPage = [ordered]@{}
                    foreach ($chars in 1, 3, 8.43, 12, 20, 40) { $perPage["$chars"] = Columns-Per-Page $ws $chars }
                    $rowBreak = 0
                    # Excel sometimes answers with a break from the font before; read until two reads in a row agree.
                    $previous = -1
                    for ($try = 0; $try -lt 8; $try++) {
                        $rowBreak = 0
                        if ([int]$rowsSheet.HPageBreaks.Count -ge 1) { $rowBreak = [int]$rowsSheet.HPageBreaks.Item(1).Location.Row }
                        if ($rowBreak -eq $previous) { break }
                        $previous = $rowBreak
                        Start-Sleep -Milliseconds 400
                    }
                    $fonts += [pscustomobject]@{ font = $name; size = $size; pixels_of_10_chars = $p10; pixels_of_1_char = $p1; default_column_pixels = [double]$plain.Columns.Item(3).Width / 0.75; row_height = [double]$rowsSheet.Rows.Item(1).Height; default_rows_then_tiny_rows_first_break = $rowBreak; columns_per_page = $perPage }
                    Write-Host ("  {0} {1}: p10={2} p1={3} rows-break={4} per page {5}" -f $name, $size, $p10, $p1, $rowBreak, (($perPage.Values | ForEach-Object { $_.per_page }) -join ','))
                } catch { Write-Host "  $name $size failed: $_" }
            }
        }
        $book.Close($false)
        [pscustomobject]@{ printer = $printer; fonts = $fonts } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $folder 'pages-fonts.json') -Encoding UTF8
    }
} finally {
    $excel.PrintCommunication = $true
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
