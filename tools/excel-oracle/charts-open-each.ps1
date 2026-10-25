# Open each workbook of a folder in its own Excel and say whether Excel opened it, repaired it, or failed,
# with the chart type Excel reports. This finds which file of a set Excel does not accept.
#
#   pwsh tools/excel-oracle/charts-open-each.ps1 -Folder C:\path\types
param([Parameter(Mandatory = $true)][string]$Folder)
$ErrorActionPreference = 'Continue'
foreach ($file in Get-ChildItem -LiteralPath $Folder -Filter *.xlsx | Sort-Object Name) {
    $before = @(Get-Process EXCEL -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
    $excel = $null
    $state = 'ok'
    $types = ''
    try {
        $excel = New-Object -ComObject Excel.Application
        $excel.DisplayAlerts = $false
        $excel.Visible = $false
        $mine = @(Get-Process EXCEL | Where-Object { $before -notcontains $_.Id } | ForEach-Object { $_.Id })
        try {
            $book = $excel.Workbooks.Open($file.FullName, 0, $true)
            if ($book.Windows.Item(1).Caption -match 'Repaired') { $state = 'repaired' }
            $types = (@($book.Worksheets.Item(1).ChartObjects() | ForEach-Object { $_.Chart.ChartType }) -join ',')
            $book.Close($false)
        } catch {
            $state = 'FAILED ' + $_.Exception.Message
        }
        try { $excel.Quit() } catch { }
        foreach ($id in $mine) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
    } catch {
        $state = 'FAILED to start ' + $_.Exception.Message
    }
    '{0}: {1} {2}' -f $file.Name, $state, $types
}
