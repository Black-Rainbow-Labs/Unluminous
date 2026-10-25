# Like tools/excel-oracle/check-folder.ps1, but a workbook Excel cannot open at all is reported and the run goes on.
#   pwsh check-all.ps1 -Folder <folder> [-Filter *.xlsx]
param([Parameter(Mandatory = $true)][string]$Folder, [string]$Filter = '*.xlsx')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$sig = '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);'
Add-Type -MemberDefinition $sig -Name Win -Namespace Probe -ErrorAction SilentlyContinue
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false; $excel.Visible = $false; $excel.AutomationSecurity = 3
$seed = $excel.Workbooks.Add(); try { $excel.ActivePrinter = 'Microsoft Print to PDF on Ne00:' } catch { }  # the default printer is offline and stalls Excel
$excelPid = 0; [void][Probe.Win]::GetWindowThreadProcessId([IntPtr]$excel.Hwnd, [ref]$excelPid)
try {
    foreach ($file in Get-ChildItem -LiteralPath $Folder -Filter $Filter | Sort-Object Name) {
        $started = Get-Date
        $state = 'clean'; $book = $null; $msg = ''
        try { $book = $excel.Workbooks.Open($file.FullName, 0, $true) } catch {
            $msg = $_.Exception.Message
            try { $book = $excel.Workbooks.Open($file.FullName, 0, $true, 5, '', '', $true, 2, '', $false, $false, 0, $false, $false, 1); $state = 'repaired' } catch { $state = 'unopenable'; $msg += ' | ' + $_.Exception.Message }
        }
        if ($book) { try { $excel.ActivePrinter = 'Microsoft Print to PDF on Ne00:' } catch { }; $book.Close($false) }
        $log = ''
        if ($state -ne 'clean') {
            $found = Get-ChildItem $env:TEMP -Filter 'error*.xml' -ErrorAction SilentlyContinue | Where-Object { $_.LastWriteTime -ge $started.AddSeconds(-1) } | Sort-Object LastWriteTime | Select-Object -Last 1
            if ($found) {
                $xml = [xml](Get-Content -LiteralPath $found.FullName -Raw)
                $log = ($xml.recoveryLog.repairedRecords.repairedRecord + $xml.recoveryLog.removedRecords.removedRecord + $xml.recoveryLog.removedParts.removedPart + $xml.recoveryLog.summary) -join ' / '
            }
        }
        [pscustomobject]@{ file = $file.Name; state = $state; message = $msg; log = $log } | ConvertTo-Json -Compress
    }
} finally {
    try { $excel.Quit() } catch { }
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
    Start-Sleep 2
    if ($excelPid -gt 0 -and (Get-Process -Id $excelPid -ErrorAction SilentlyContinue)) { Stop-Process -Id $excelPid -Force }
}

