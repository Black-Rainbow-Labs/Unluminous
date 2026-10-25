# Save a small workbook with a password to open, as .xls, .xlsx and .xlsb, for the tests that check an
# encrypted workbook is refused.
#
#   pwsh tools/excel-oracle/make-password-fixtures.ps1 -Out crates/unluminous-sheet/tests/fixtures/legacy [-Password secret]
param(
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Password = 'secret',
    [int]$Seconds = 120
)
$ErrorActionPreference = 'Stop'
$Out = [IO.Path]::GetFullPath($Out)
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
Add-Type -Namespace Native -Name Window -MemberDefinition '[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(System.IntPtr hWnd, out int pid);'
$excelPid = 0
[Native.Window]::GetWindowThreadProcessId([IntPtr]$excel.Hwnd, [ref]$excelPid) | Out-Null
$watchdog = Start-Job -ScriptBlock { param($p, $s) Start-Sleep $s; Stop-Process -Id $p -Force -ErrorAction SilentlyContinue } -ArgumentList $excelPid, $Seconds
try {
    foreach ($pair in @(@('xls', 56), @('xlsx', 51), @('xlsb', 50))) {
        $book = $excel.Workbooks.Add()
        $book.Worksheets.Item(1).Range('A1').Value2 = 'secret contents'
        $target = Join-Path $Out ("password." + $pair[0])
        if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target }
        $book.SaveAs($target, $pair[1], $Password)
        $book.Close($false)
        Write-Output "saved $target"
    }
} finally {
    Stop-Job $watchdog -ErrorAction SilentlyContinue
    Remove-Job $watchdog -Force -ErrorAction SilentlyContinue
    try { $excel.Quit() } catch {}
    Stop-Process -Id $excelPid -Force -ErrorAction SilentlyContinue
}
