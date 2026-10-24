# Open every workbook in a folder in Excel, one Excel for all of them, and say for each whether Excel had to
# repair it, with the repair log's own words when it did.
#
#   pwsh tools/excel-oracle/check-folder.ps1 -Folder <folder>
param([Parameter(Mandatory = $true)][string]$Folder)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$excel.AutomationSecurity = 3
try {
    foreach ($file in Get-ChildItem -LiteralPath $Folder -Filter *.xlsx | Sort-Object Name) {
        $started = Get-Date
        $repaired = $false
        try {
            $book = $excel.Workbooks.Open($file.FullName, 0, $true)
        } catch {
            $repaired = $true
            $book = $excel.Workbooks.Open($file.FullName, 0, $true, 5, '', '', $true, 2, '', $false, $false, 0, $false, $false, 1)
        }
        $book.Close($false)
        $log = ''
        if ($repaired) {
            $found = Get-ChildItem $env:TEMP -Filter 'error*.xml' | Where-Object { $_.LastWriteTime -ge $started.AddSeconds(-1) } | Sort-Object LastWriteTime | Select-Object -Last 1
            if ($found) {
                $xml = [xml](Get-Content -LiteralPath $found.FullName -Raw)
                $log = ($xml.recoveryLog.repairedRecords.repairedRecord + $xml.recoveryLog.removedRecords.removedRecord + $xml.recoveryLog.removedParts.removedPart) -join ' / '
            }
        }
        [pscustomobject]@{ file = $file.Name; repaired = $repaired; log = $log } | ConvertTo-Json -Compress
    }
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
