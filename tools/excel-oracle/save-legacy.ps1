# Save each workbook in a folder as .xls (Excel 97-2003, FileFormat 56) and .xlsb (Excel binary, 50).
#
# These are the fixtures of crates/unluminous-sheet/tests/legacy.rs. The workbooks come from the file
# probes of task-2357 (one per feature) plus tests/fixtures/features.xlsx. Excel saves into -Out; a
# workbook Excel cannot save in a format is reported and skipped, because some features (threaded
# comments, sparklines, tables with slicers) have no place in the older formats.
#
# A file that makes Excel stop answering is given -Seconds and then Excel's own process is stopped by its
# process id, and the next file gets a new Excel.
#
#   pwsh tools/excel-oracle/save-legacy.ps1 -In <folder or file...> -Out <folder> [-Seconds 90]
param(
    [Parameter(Mandatory = $true)][string[]]$In,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$Seconds = 90
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path -LiteralPath $Out).Path
$files = @()
foreach ($item in $In) {
    if (Test-Path -LiteralPath $item -PathType Container) {
        $files += Get-ChildItem -LiteralPath $item -File | Where-Object { $_.Extension -in '.xlsx', '.xlsm' }
    } else {
        $files += Get-Item -LiteralPath $item
    }
}

Add-Type -Namespace Native -Name Window -MemberDefinition '[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(System.IntPtr hWnd, out int pid);'

# Excel's process id, from its window handle.
function Get-ExcelProcessId($excel) {
    $owner = 0
    [Native.Window]::GetWindowThreadProcessId([IntPtr]$excel.Hwnd, [ref]$owner) | Out-Null
    return $owner
}

# One Excel, one file, one format, with a watchdog that stops that Excel if it hangs.
function Save-One($file, $target, $format) {
    $excel = New-Object -ComObject Excel.Application
    $excel.DisplayAlerts = $false
    $excel.Visible = $false
    $excel.AutomationSecurity = 3
    $excelPid = Get-ExcelProcessId $excel
    $watchdog = Start-Job -ScriptBlock { param($p, $s) Start-Sleep $s; Stop-Process -Id $p -Force -ErrorAction SilentlyContinue } -ArgumentList $excelPid, $Seconds
    try {
        $book = $excel.Workbooks.Open($file.FullName, 0, $true)
        $book.SaveAs($target, $format)
        $book.Close($false)
        return "saved $target"
    } catch {
        return "skipped $target : $($_.Exception.Message)"
    } finally {
        Stop-Job $watchdog -ErrorAction SilentlyContinue
        Remove-Job $watchdog -Force -ErrorAction SilentlyContinue
        try { $excel.Quit() } catch {}
        Stop-Process -Id $excelPid -Force -ErrorAction SilentlyContinue
    }
}

foreach ($file in $files) {
    $base = [IO.Path]::GetFileNameWithoutExtension($file.Name)
    foreach ($pair in @(@('.xls', 56), @('.xlsb', 50))) {
        $target = Join-Path $Out ($base + $pair[0])
        if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target }
        Write-Output (Save-One $file $target $pair[1])
    }
}
