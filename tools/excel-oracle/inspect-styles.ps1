# Open a workbook in Excel and print, as JSON, the name of the style of every cell in the used range of the
# first sheet and, for every style, which groups of formatting it includes.
#
#   pwsh tools/excel-oracle/inspect-styles.ps1 -Path book.xlsx
param([Parameter(Mandatory = $true)][string]$Path)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $book = $excel.Workbooks.Open((Resolve-Path -LiteralPath $Path).Path, 0, $true)
    $sheet = $book.Worksheets.Item(1)
    $cells = @()
    foreach ($cell in $sheet.UsedRange.Cells) {
        $cells += [pscustomobject]@{
            cell = $cell.Address($false, $false)
            style = $cell.Style.Name
            bold = [bool]$cell.Font.Bold
            fontName = $cell.Font.Name
            fontColor = [int]$cell.Font.Color
            fill = [int]$cell.Interior.Color
            format = $cell.NumberFormat
        }
    }
    $styles = @()
    foreach ($s in $book.Styles) {
        $styles += [pscustomobject]@{
            name = $s.Name
            builtIn = [bool]$s.BuiltIn
            number = [bool]$s.IncludeNumber
            font = [bool]$s.IncludeFont
            alignment = [bool]$s.IncludeAlignment
            border = [bool]$s.IncludeBorder
            patterns = [bool]$s.IncludePatterns
            protection = [bool]$s.IncludeProtection
            numberFormat = $s.NumberFormat
        }
    }
    $theme = @()
    foreach ($i in 1..12) { $theme += '{0:X6}' -f $book.Theme.ThemeColorScheme.Colors($i).RGB }
    [pscustomobject]@{
        cells = $cells
        styles = $styles
        themeColors = $theme
        majorFont = $book.Theme.ThemeFontScheme.MajorFont.Item(1).Name
        minorFont = $book.Theme.ThemeFontScheme.MinorFont.Item(1).Name
    } | ConvertTo-Json -Depth 5 -Compress
    $book.Close($false)
} finally {
    $excel.Quit()
}
