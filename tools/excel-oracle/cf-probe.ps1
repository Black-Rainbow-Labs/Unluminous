# Make a workbook in Excel holding conditional formats of every kind with their options, and save it, so the
# XML Excel writes for each can be read. Used to learn the x14 extension shapes.
#
#   pwsh tools/excel-oracle/cf-probe.ps1 -Out probe.xlsx
param([Parameter(Mandatory = $true)][string]$Out)
$ErrorActionPreference = 'Stop'
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $book = $excel.Workbooks.Add()
    $sheet = $book.Worksheets.Item(1)
    $other = $book.Worksheets.Add([Type]::Missing, $sheet)
    $other.Name = 'Other'
    $other.Range('A1').Value2 = 5
    $sheet.Activate()
    for ($c = 1; $c -le 30; $c++) { for ($r = 1; $r -le 10; $r++) { $sheet.Cells.Item($r, $c).Value2 = ($r * 3 + $c * 7) % 11 - 3 } }
    # Data bar with every option.
    $bar = $sheet.Range('A1:A10').FormatConditions.AddDatabar()
    $bar.BarFillType = 0
    $bar.BarBorder.Type = 1
    $bar.BarBorder.Color.Color = 255
    $bar.AxisPosition = 1
    $bar.AxisColor.Color = 65280
    $bar.Direction = -5004
    $bar.NegativeBarFormat.ColorType = 0
    $bar.NegativeBarFormat.Color.Color = 16711680
    $bar.NegativeBarFormat.BorderColorType = 0
    $bar.NegativeBarFormat.BorderColor.Color = 65535
    $bar.BarColor.Color = 8421504
    $bar.MinPoint.Modify(0, 0) | Out-Null
    $bar.MaxPoint.Modify(2, 50) | Out-Null
    $bar.ShowValue = $false
    # Plain data bar, defaults.
    $sheet.Range('B1:B10').FormatConditions.AddDatabar() | Out-Null
    # Icon sets, every built in set.
    for ($i = 1; $i -le 20; $i++) {
        $col = 2 + $i
        $rule = $sheet.Range($sheet.Cells.Item(1, $col), $sheet.Cells.Item(10, $col)).FormatConditions.AddIconSetCondition()
        $rule.IconSet = $book.IconSets.Item($i)
    }
    # Icon set with reverse, show icon only, custom icons and criteria.
    $rule = $sheet.Range('X1:X10').FormatConditions.AddIconSetCondition()
    $rule.IconSet = $book.IconSets.Item(4)
    $rule.ReverseOrder = $true
    $rule.ShowIconOnly = $true
    $rule.IconCriteria.Item(2).Type = 3
    $rule.IconCriteria.Item(2).Value = 2
    $rule.IconCriteria.Item(2).Operator = 5
    $rule.IconCriteria.Item(3).Type = 0
    $rule.IconCriteria.Item(3).Value = 4
    $rule.IconCriteria.Item(1).Icon = 12
    # Rule with a formula on another sheet.
    $x = $sheet.Range('Y1:Y10').FormatConditions.Add(1, 5, '=Other!$A$1')
    $x.Interior.Color = 255
    $y = $sheet.Range('Z1:Z10').FormatConditions.Add(2, [Type]::Missing, '=Z1>Other!$A$1')
    $y.Interior.Color = 65280
    $book.SaveAs($Out, 51)
    $book.Close($false)
} finally {
    $excel.Quit()
    [Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
