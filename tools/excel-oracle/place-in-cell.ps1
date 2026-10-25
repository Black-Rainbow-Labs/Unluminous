# Make the workbook Excel writes when a picture is placed in a cell (Picture > Place in Cell), so the rich value
# parts can be studied. Usage: pwsh tools/excel-oracle/place-in-cell.ps1 -Picture <png> -Out <xlsx> [-Cell B2] [-AltText text]
param([Parameter(Mandatory = $true)][string]$Picture, [Parameter(Mandatory = $true)][string]$Out, [string]$Cell = 'A1', [string]$AltText = '')
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false; $excel.Visible = $true
try {
  $wb = $excel.Workbooks.Add(); $ws = $wb.Worksheets.Item(1)
  $ws.Range($Cell).Select()
  $pic = $ws.Shapes.AddPicture($Picture, 0, -1, 10, 10, 80, 60)
  if ($AltText) { $pic.AlternativeText = $AltText }
  $pic.Select()
  $excel.CommandBars.ExecuteMso('PicturePlaceInCell')
  Start-Sleep -Milliseconds 800
  "shapes=$($ws.Shapes.Count) value=[$($ws.Range($Cell).Value2)] text=[$($ws.Range($Cell).Text)]"
  $wb.SaveAs($Out, 51)
  $wb.Close($false)
} finally { $excel.Quit(); [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel) }
