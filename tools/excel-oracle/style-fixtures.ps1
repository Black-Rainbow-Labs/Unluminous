# Make the fixtures that tests/styles.rs and tests/themes.rs compare the engine's named cell styles and
# themes with: each operation done by Excel itself, with the workbook saved before and after.
#
#   pwsh tools/excel-oracle/style-fixtures.ps1 [-Out crates/unluminous-sheet/tests/fixtures/styles]
param([string]$Out = (Join-Path $PSScriptRoot '..\..\crates\unluminous-sheet\tests\fixtures\styles'))
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$themes = 'C:\Program Files\Microsoft Office\root\Document Themes 16'
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
function Save($book, $name) { $book.SaveAs((Join-Path $Out $name), 51) }

try {
    # ------------------------------------------------------------------------------------------------
    # Every built in style, applied to a cell each.
    $names = 'Normal', 'Good', 'Bad', 'Neutral', 'Input', 'Output', 'Calculation', 'Check Cell', 'Linked Cell', 'Note', 'Warning Text', 'Explanatory Text', 'Heading 1', 'Heading 2', 'Heading 3', 'Heading 4', 'Title', 'Total', '20% - Accent1', '40% - Accent1', '60% - Accent1', 'Accent1', '20% - Accent2', '40% - Accent2', '60% - Accent2', 'Accent2', '20% - Accent3', '40% - Accent3', '60% - Accent3', 'Accent3', '20% - Accent4', '40% - Accent4', '60% - Accent4', 'Accent4', '20% - Accent5', '40% - Accent5', '60% - Accent5', 'Accent5', '20% - Accent6', '40% - Accent6', '60% - Accent6', 'Accent6', 'Comma', 'Comma [0]', 'Currency', 'Currency [0]', 'Percent'
    $book = $excel.Workbooks.Add()
    $s = $book.Worksheets.Item(1)
    $i = 1
    foreach ($n in $names) { $s.Cells.Item($i, 1).Value2 = $n; $s.Cells.Item($i, 2).Value2 = 1234.5; $i++ }
    Save $book 'styles-all-input.xlsx'
    $i = 1
    foreach ($n in $names) { $s.Cells.Item($i, 1).Style = $n; $s.Cells.Item($i, 2).Style = $n; $i++ }
    Save $book 'styles-all.xlsx'
    # A cell with direct formatting over a style, and a style applied over direct formatting.
    $book.Close($false)

    $book = $excel.Workbooks.Add(); $s = $book.Worksheets.Item(1)
    $s.Range('A1').Value2 = 'bold'; $s.Range('A1').Font.Bold = $true; $s.Range('A1').Font.Color = 255
    $s.Range('A2').Value2 = 'italic'; $s.Range('A2').Font.Italic = $true; $s.Range('A2').Interior.Color = 65535
    $s.Range('A3').Value2 = 12.5; $s.Range('A3').NumberFormat = '0.0%'; $s.Range('A3').HorizontalAlignment = -4108
    Save $book 'styles-direct-input.xlsx'
    foreach ($a in 'A1', 'A2', 'A3') { $s.Range($a).Style = 'Good' }
    Save $book 'styles-direct-good.xlsx'
    $s.Range('A1:A3').Style = 'Normal'
    Save $book 'styles-direct-normal.xlsx'
    $book.Close($false)

    # ------------------------------------------------------------------------------------------------
    # A style made from a cell, applied, changed and deleted.
    $book = $excel.Workbooks.Add(); $s = $book.Worksheets.Item(1)
    $s.Range('A1').Value2 = 'one'; $s.Range('A2').Value2 = 'two'; $s.Range('A3').Value2 = 'three'; $s.Range('A4').Value2 = 'four'
    $s.Range('A1').Font.Bold = $true; $s.Range('A1').Interior.Color = 0x00FFFF; $s.Range('A1').NumberFormat = '0.000'
    Save $book 'styles-custom-input.xlsx'
    $null = $book.Styles.Add('My Style', $s.Range('A1'))
    $s.Range('A2').Style = 'My Style'
    $s.Range('A3').Style = 'My Style'
    $s.Range('A3').Font.Italic = $true
    $s.Range('A4').Style = 'Good'
    Save $book 'styles-custom-applied.xlsx'
    $st = $book.Styles.Item('My Style')
    $st.Font.Color = 0xFF0000; $st.Font.Bold = $false; $st.NumberFormat = '0.0'; $st.Interior.Color = 0x00FF00
    Save $book 'styles-custom-modified.xlsx'
    $book.Styles.Item('My Style').Delete()
    Save $book 'styles-custom-deleted.xlsx'
    $book.Styles.Item('Good').Delete()
    Save $book 'styles-builtin-deleted.xlsx'
    $book.Close($false)

    # A built in style changed, and styles made with the groups they include switched off.
    $book = $excel.Workbooks.Add(); $s = $book.Worksheets.Item(1)
    foreach ($r in 1..3) { $s.Cells.Item($r, 1).Value2 = [double]$r; $s.Cells.Item($r, 1).Style = 'Good' }
    $s.Range('A3').Font.Italic = $true
    $s.Range('B1').Value2 = 'plain'; $s.Range('B2').Value2 = 0.5; $s.Range('B2').NumberFormat = '0.00'
    Save $book 'styles-modify-input.xlsx'
    $g = $book.Styles.Item('Good'); $g.Font.Color = 255; $g.Font.Bold = $true
    Save $book 'styles-modify-builtin.xlsx'
    $book.Close($false)

    # ------------------------------------------------------------------------------------------------
    # Merge styles from another workbook.
    $src = $excel.Workbooks.Add(); $ss = $src.Worksheets.Item(1)
    $a = $src.Styles.Add('Src1'); $a.Font.Bold = $true; $a.Font.Color = 0xFF0000; $a.Interior.Color = 0x00FFFF
    $b = $src.Styles.Add('Src2'); $b.NumberFormat = '0.00%'; $b.Font.Italic = $true
    $src.Styles.Item('Good').Font.Color = 255
    $ss.Range('A1').Style = 'Src1'
    Save $src 'styles-merge-src.xlsx'
    $dst = $excel.Workbooks.Add(); $ds = $dst.Worksheets.Item(1)
    $d1 = $dst.Styles.Add('Src1'); $d1.Font.Italic = $true; $d1.Font.Color = 0x00FF00
    $ds.Range('A1').Value2 = 'x'; $ds.Range('A1').Style = 'Src1'
    $ds.Range('A2').Value2 = 'y'; $ds.Range('A2').Style = 'Good'
    Save $dst 'styles-merge-dst-input.xlsx'
    $dst.Styles.Merge($src)
    Save $dst 'styles-merge-dst.xlsx'
    $dst.Close($false); $src.Close($false)

    # ------------------------------------------------------------------------------------------------
    # Themes.
    function New-ThemeInput {
        $book = $excel.Workbooks.Add(); $s = $book.Worksheets.Item(1)
        $i = 1
        foreach ($n in 'Good', 'Bad', 'Heading 1', 'Heading 2', 'Title', 'Accent1', 'Accent2', '20% - Accent3', '60% - Accent6', 'Hyperlink-like') {
            $s.Cells.Item($i, 1).Value2 = $n
            if ($n -ne 'Hyperlink-like') { $s.Cells.Item($i, 1).Style = $n }
            $i++
        }
        $s.Range('B1').Value2 = 'red'; $s.Range('B1').Font.Color = 255
        $s.Range('B2').Value2 = 'theme font colour'; $s.Range('B2').Font.ThemeColor = 6; $s.Range('B2').Font.TintAndShade = 0.4
        $s.Range('B3').Value2 = 'theme fill'; $s.Range('B3').Interior.ThemeColor = 7; $s.Range('B3').Interior.TintAndShade = -0.25
        $s.Range('B4').Value2 = 'accent fill 5'; $s.Range('B4').Interior.ThemeColor = 9
        $s.Range('B5').Value2 = 'dark 2 text'; $s.Range('B5').Font.ThemeColor = 3
        $s.Range('B6').Borders.Item(9).Color = 255
        $s.Range('B6').Borders.Item(9).LineStyle = 1
        $s.Range('B7').Value2 = 'border theme'; $s.Range('B7').Borders.Item(8).ThemeColor = 5; $s.Range('B7').Borders.Item(8).LineStyle = 1
        return $book
    }
    $book = New-ThemeInput
    Save $book 'theme-input.xlsx'
    $book.Close($false)
    foreach ($t in 'Facet', 'Office 2013 - 2022 Theme', 'Office Theme', 'Retrospect', 'Slice', 'Wisp', 'Gallery', 'Frame-new', 'Dividend-new', 'Modulo') {
        $book = New-ThemeInput
        $book.ApplyTheme((Join-Path $themes ($t + '.thmx')))
        Save $book ('theme-' + ($t -replace '[ ]', '_') + '.xlsx')
        $book.Close($false)
    }
    # Colours only, from the Colors gallery, fonts only from the Fonts gallery, and a theme changed by hand.
    $book = New-ThemeInput
    $book.Theme.ThemeColorScheme.Load((Join-Path $themes 'Theme Colors\Blue Warm.xml'))
    Save $book 'theme-colors-Blue_Warm.xlsx'
    $book.Close($false)
    $book = New-ThemeInput
    $book.Theme.ThemeFontScheme.Load((Join-Path $themes 'Theme Fonts\Calibri-Cambria.xml'))
    Save $book 'theme-fonts-Calibri-Cambria.xlsx'
    $book.Close($false)
    $book = New-ThemeInput
    $book.Theme.ThemeColorScheme.Colors(5).RGB = 0x0000FF
    $book.Theme.ThemeColorScheme.Colors(9).RGB = 0x00FF00
    $book.Theme.ThemeFontScheme.MinorFont.Item(1).Name = 'Courier New'
    $book.Theme.ThemeFontScheme.MajorFont.Item(1).Name = 'Georgia'
    Save $book 'theme-custom.xlsx'
    $book.Close($false)
} finally {
    $excel.Quit()
}
