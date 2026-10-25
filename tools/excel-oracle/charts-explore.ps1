# Build a workbook in Excel holding charts, by running a PowerShell snippet against a small data sheet,
# save it and leave the file for inspection. This is how the XML Excel writes for a chart option is found out.
#
# The snippet sees $excel, $book, $ws (sheet "Data": A1:E7 with Cat, High, Low, Open, Close) and the
# function Add-Chart $type $range, which returns the new chart.
#
#   pwsh tools/excel-oracle/charts-explore.ps1 -Out C:\path\book.xlsx -CodeFile C:\path\snippet.ps1
param(
    [Parameter(Mandatory = $true)][string]$Out,
    [Parameter(Mandatory = $true)][string]$CodeFile
)
$ErrorActionPreference = 'Stop'
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
try {
    $book = $excel.Workbooks.Add()
    $ws = $book.Worksheets.Item(1)
    $ws.Name = 'Data'
    $ws.Range('A1:E1').Value2 = [object[]]@('Cat', 'High', 'Low', 'Open', 'Close')
    $cats = 'Alpha', 'Beta', 'Gamma', 'Delta', 'Epsilon', 'Zeta'
    $high = 12, 18, 7, 25, 14, 30
    $low = 3, 5, 2, 9, 4, 11
    for ($i = 0; $i -lt 6; $i++) {
        $r = $i + 2
        $ws.Cells.Item($r, 1).Value2 = $cats[$i]
        $ws.Cells.Item($r, 2).Value2 = [double]$high[$i]
        $ws.Cells.Item($r, 3).Value2 = [double]$low[$i]
        $ws.Cells.Item($r, 4).Value2 = [double]($high[$i] - 2)
        $ws.Cells.Item($r, 5).Value2 = [double]($low[$i] + 3)
    }
    function Add-Chart($type, $range) {
        $shape = $ws.Shapes.AddChart2(-1, [int]$type, 200, 20, 400, 250)
        $shape.Chart.SetSourceData($ws.Range([string]$range))
        return $shape.Chart
    }
    try {
        . $CodeFile
    } catch {
        Write-Host ("snippet failed at line " + $_.InvocationInfo.ScriptLineNumber + ": " + $_.Exception.Message)
        throw
    }
    $book.SaveAs($Out, 51)
    $book.Close($false)
} finally {
    $excel.Quit()
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
}
