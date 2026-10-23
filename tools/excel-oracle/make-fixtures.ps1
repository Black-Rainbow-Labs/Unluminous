# Build the workbooks the reader and writer are tested against, with Excel itself.
#
# Each one is made through Excel's own object model and saved by Excel, so it is exactly what a person's
# file looks like: shared formulas, dynamic arrays, a table, conditional formats, validation, notes, a chart
# and the rest. The files are committed under crates/unluminous-sheet/tests/fixtures, so the tests that
# read them need no Excel.
#
#   pwsh tools/excel-oracle/make-fixtures.ps1 [-Folder crates/unluminous-sheet/tests/fixtures]
param([string]$Folder = 'crates/unluminous-sheet/tests/fixtures')
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Folder | Out-Null
$folder = (Resolve-Path -LiteralPath $Folder).Path
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    # ---- features.xlsx: one of everything a person makes ----
    $book = $excel.Workbooks.Add()
    while ($book.Worksheets.Count -lt 3) { $book.Worksheets.Add([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count)) | Out-Null }
    $data = $book.Worksheets.Item(1)
    $data.Name = 'Data'
    $data.Range('A1').Value2 = 'Item'
    $data.Range('B1').Value2 = 'Region'
    $data.Range('C1').Value2 = 'Units'
    $data.Range('D1').Value2 = 'Price'
    $rows = @(
        @('Apples', 'North', 120, 0.5),
        @('Pears', 'South', 80, 0.75),
        @('Plums', 'North', 45, 1.2),
        @('Figs', 'East', 30, 2.5),
        @('Kiwis', 'South', 200, 0.4),
        @('Limes', 'East', 150, 0.3)
    )
    $letters = 'A', 'B', 'C', 'D'
    for ($i = 0; $i -lt $rows.Count; $i++) {
        for ($j = 0; $j -lt 4; $j++) {
            # PowerShell's COM binder keeps the type of the first value assigned at a call site, so
            # text and numbers are assigned from two different statements.
            $value = $rows[$i][$j]
            $cell = $data.Range("$($letters[$j])$($i + 2)")
            if ($value -is [string]) { $cell.Value2 = $value } else { $cell.Formula = [string]::Format([Globalization.CultureInfo]::InvariantCulture, '{0}', $value) }
        }
    }
    # A formula filled down is written as a shared formula.
    $data.Range('E1').Value2 = 'Total'
    $data.Range('E2').Formula = '=C2*D2'
    $data.Range('E2:E7').FillDown() | Out-Null
    $data.Range('E8').Formula = '=SUM(E2:E7)'
    $data.Range('D2:E8').NumberFormat = '$#,##0.00'
    $data.Range('A1:E1').Font.Bold = $true
    $data.Range('A1:E1').Interior.Color = 0xD9E1F2
    $data.Range('A8').Value2 = 'Sum'
    $data.Range('A8:E8').Borders.Item(8).LineStyle = 1      # top edge
    $data.Range('A8:E8').Borders.Item(8).Weight = -4138      # medium
    $data.Columns.Item(1).ColumnWidth = 14
    # A table over the data.
    $table = $data.ListObjects.Add(1, $data.Range('A1:E7'), [Type]::Missing, 1)
    $table.Name = 'Fruit'
    # Dynamic arrays.
    $data.Range('G1').Value2 = 'Regions'
    $data.Range('G2').Formula2 = '=SORT(UNIQUE(B2:B7))'
    $data.Range('H1').Value2 = 'Big orders'
    $data.Range('H2').Formula2 = '=FILTER(A2:A7,C2:C7>100,"none")'
    $data.Range('I1').Value2 = 'Seq'
    $data.Range('I2').Formula2 = '=SEQUENCE(3,2,10,5)'
    $data.Range('K2').Formula2 = '=SUM(I2#)'
    # A legacy array formula.
    $data.Range('L2:L4').FormulaArray = '=C2:C4*2'
    # Structured reference and a name.
    $data.Range('M2').Formula = '=SUM(Fruit[Units])'
    $book.Names.Add('TaxRate', '=0.2') | Out-Null
    $data.Range('M3').Formula = '=E8*TaxRate'
    $data.Range('M4').Formula = '=XLOOKUP("Figs",A2:A7,C2:C7)'
    $data.Range('M5').Formula = '=LET(x,C2:C7,AVERAGE(x))'
    $data.Range('M6').Formula = '=TEXT(DATE(2026,10,23),"dddd")'
    $data.Range('M7').Formula = '46318.75'
    $data.Range('M7').NumberFormat = 'yyyy-mm-dd h:mm'
    $data.Range('M8').Formula = '0.256'
    $data.Range('M8').NumberFormat = '0.0%'
    # Conditional formats: a cell value rule, a colour scale, a data bar and an icon set.
    $cf = $data.Range('C2:C7').FormatConditions.Add(1, 5, '=100')   # greater than 100
    $cf.Interior.Color = 0xCEEFC6
    $cf.Font.Color = 0x006100
    $data.Range('D2:D7').FormatConditions.AddColorScale(3) | Out-Null
    $data.Range('E2:E7').FormatConditions.AddDatabar() | Out-Null
    $data.Range('C2:C7').FormatConditions.AddIconSetCondition() | Out-Null
    # Validation: a list.
    $data.Range('B10').Validation.Add(3, 1, 1, 'North,South,East,West') | Out-Null
    $data.Range('B10').Value2 = 'North'
    $data.Range('A10').Value2 = 'Pick a region'
    # A note and a link.
    $data.Range('A2').AddComment('Picked in October') | Out-Null
    $data.Hyperlinks.Add($data.Range('A12'), 'https://example.com/fruit', '', 'The fruit site', 'Fruit site') | Out-Null
    $data.Hyperlinks.Add($data.Range('A13'), '', "'Summary'!A1", '', 'Go to summary') | Out-Null
    # Merges, frozen panes, a hidden row, a tab colour.
    $data.Range('A15:C15').Merge()
    $data.Range('A15').Value2 = 'Merged title'
    $data.Range('A15').HorizontalAlignment = -4108
    $data.Rows.Item(16).Hidden = $true
    $data.Range('A16').Value2 = 'hidden row'
    $data.Tab.Color = 0x00C0FF
    $data.Activate()
    $excel.ActiveWindow.SplitRow = 1
    $excel.ActiveWindow.SplitColumn = 0
    $excel.ActiveWindow.FreezePanes = $true
    # A chart.
    $chart = $data.Shapes.AddChart2(201, 51, 400, 200, 360, 220).Chart
    $chart.SetSourceData($data.Range('A1:A7,C1:C7'))
    $chart.HasTitle = $true
    $chart.ChartTitle.Text = 'Units by item'

    $summary = $book.Worksheets.Item(2)
    $summary.Name = 'Summary'
    $summary.Range('A1').Value2 = 'Units'
    $summary.Range('B1').Formula = '=SUM(Data!C2:C7)'
    $summary.Range('A2').Value2 = 'North units'
    $summary.Range('B2').Formula = '=SUMIF(Data!B2:B7,"North",Data!C2:C7)'
    $summary.Range('A3').Value2 = 'Average price'
    $summary.Range('B3').Formula = '=AVERAGE(Data!D2:D7)'
    $summary.Range('A4').Value2 = 'Text'
    $summary.Range('B4').Value2 = "Line one`nLine two"
    $summary.Range('B4').WrapText = $true
    $summary.Range('A5').Value2 = 'Logical'
    $summary.Range('B5').Formula = 'TRUE'
    $summary.Range('A6').Value2 = 'Error'
    $summary.Range('B6').Formula = '=1/0'
    $summary.Range('A7').Value2 = 'Italic red'
    $summary.Range('A7').Font.Italic = $true
    $summary.Range('A7').Font.Color = 0x0000FF
    $summary.Range('B7').Value2 = '0012'
    $summary.Range('B7').NumberFormat = '@'
    $summary.Range('A8').Value2 = 'Leading apostrophe'
    $summary.Range('B8').Formula = "'=not a formula"

    $hidden = $book.Worksheets.Item(3)
    $hidden.Name = 'Hidden'
    $hidden.Range('A1').Value2 = 'secret'
    $hidden.Visible = 0

    $data.Activate()
    $book.SaveAs((Join-Path $folder 'features.xlsx'), 51)
    $book.Close($false)

    # ---- blank.xlsx: what Excel saves for a new workbook ----
    $book = $excel.Workbooks.Add()
    $book.SaveAs((Join-Path $folder 'blank.xlsx'), 51)
    $book.Close($false)
    Write-Output "made features.xlsx and blank.xlsx in $folder"
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
