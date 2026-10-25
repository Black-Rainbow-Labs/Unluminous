# Makes a workbook with a pivot table, a slicer on Region (West switched off) and a timeline on Date, saves it
# and writes its part names. Usage: pwsh -File slicer-make.ps1 -Out <xlsx path>
param([Parameter(Mandatory = $true)][string]$Out, [switch]$AllRegions)
$ErrorActionPreference = 'Stop'
$Out = [IO.Path]::GetFullPath($Out)
$xl = New-Object -ComObject Excel.Application
$xl.Visible = $false
$xl.DisplayAlerts = $false
$missing = [Type]::Missing
try {
    $wb = $xl.Workbooks.Add()
    $ws = $wb.Worksheets.Item(1)
    $rows = @(
        @('Region', 'Item', 'Units', 'Day'),
        @('East', 'Tea', 5, 46027), @('East', 'Cake', 3, 46064), @('West', 'Tea', 7, 46081),
        @('West', 'Cake', 1, 46100), @('North', 'Tea', 2, 46130), @('North', 'Cake', 9, 46160)
    )
    for ($r = 0; $r -lt $rows.Count; $r++) {
        for ($c = 0; $c -lt 4; $c++) {
            $v = $rows[$r][$c]
            $ws.Range(('A', 'B', 'C', 'D')[$c] + ($r + 1)).Formula2 = [string]$v
        }
    }
    $ws.Range('D2:D7').NumberFormat = 'm/d/yyyy'
    $pc = $wb.PivotCaches().Create(1, 'Sheet1!R1C1:R7C4')
    $pt = $pc.CreatePivotTable('Sheet1!R1C7', 'PivotTable1')
    $pt.PivotFields('Region').Orientation = 1
    $df = $pt.AddDataField($pt.PivotFields('Units'), 'Sum of Units', -4157)
    $sc = $wb.SlicerCaches.Add2($pt, 'Region')
    $null = $sc.Slicers.Add($ws, $missing, 'Region', 'Region', 120, 480, 140, 160)
    if (-not $AllRegions) { $sc.SlicerItems.Item('West').Selected = $false }
    $tc = $wb.SlicerCaches.Add2($pt, 'Day', $missing, 2)
    $null = $tc.Slicers.Add($ws, $missing, 'Day', 'Day', 300, 480, 300, 100)
    $tc.TimelineState.SetFilterDateRange('2/1/2026', '3/31/2026')
    $wb.SaveAs($Out, 51)
    $wb.Close($false)
    Write-Output "saved $Out"
} catch {
    Write-Output $_.InvocationInfo.PositionMessage
    throw
} finally {
    $xl.Quit()
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($xl)
}
