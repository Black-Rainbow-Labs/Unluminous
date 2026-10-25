# Put every formula of an oracle corpus in a workbook, save it as .xls and .xlsb, and write the formulas
# Excel reads back from the .xls.
#
# crates/unluminous-sheet/tests/legacy.rs reads the .xls and .xlsb and compares each formula with Excel's
# own reading of the .xls, which checks every function number in the binary formats' function table. The
# formulas go in column J, twelve rows apart so an array result does not run into the next formula, with
# the setup data of the corpus in A1:H10.
#
#   pwsh tools/excel-oracle/make-function-fixture.ps1 -Formulas tests/oracle/formulas.txt -Setup tests/oracle/setup.txt -Out tests/fixtures/legacy/functions [-Seconds 300]
#
# Writes <Out>.xls, <Out>.xlsb and <Out>.formulas.txt (a row number, a tab, and the formula).
param(
    [Parameter(Mandatory = $true)][string[]]$Formulas,
    [Parameter(Mandatory = $true)][string]$Setup,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$Seconds = 300
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$Out = [IO.Path]::GetFullPath($Out)
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
Add-Type -Namespace Native -Name Window -MemberDefinition '[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(System.IntPtr hWnd, out int pid);'
$excelPid = 0
[Native.Window]::GetWindowThreadProcessId([IntPtr]$excel.Hwnd, [ref]$excelPid) | Out-Null
$watchdog = Start-Job -ScriptBlock { param($p, $s) Start-Sleep $s; Stop-Process -Id $p -Force -ErrorAction SilentlyContinue } -ArgumentList $excelPid, $Seconds
try {
    $book = $excel.Workbooks.Add()
    $sheet = $book.Worksheets.Item(1)
    foreach ($line in Get-Content -LiteralPath $Setup -Encoding UTF8) {
        if (-not $line) { continue }
        $at = $line.IndexOf('=')
        $sheet.Range($line.Substring(0, $at)).Formula2 = $line.Substring($at + 1)
    }
    $row = 1
    foreach ($file in $Formulas) {
        foreach ($formula in Get-Content -LiteralPath $file -Encoding UTF8) {
            if (-not $formula -or $formula.StartsWith('#')) { continue }
            try { $sheet.Range("J$row").Formula2 = '=' + $formula } catch { }
            $row += 12
        }
    }
    $excel.Calculate()
    $book.SaveAs("$Out.xls", 56)
    $book.Close($false)
    $excel.Workbooks.Open("$Out.xls") | Out-Null
    $opened = $excel.ActiveWorkbook
    $lines = @()
    for ($r = 1; $r -lt $row; $r += 12) {
        $cell = $opened.Worksheets.Item(1).Range("J$r")
        if ($cell.HasFormula) { $lines += "$r`t$($cell.Formula2)" }
    }
    [IO.File]::WriteAllLines("$Out.formulas.txt", $lines, (New-Object Text.UTF8Encoding $false))
    $opened.SaveAs("$Out.xlsb", 50)
    $opened.Close($false)
} finally {
    Stop-Job $watchdog -ErrorAction SilentlyContinue
    Remove-Job $watchdog -Force -ErrorAction SilentlyContinue
    try { $excel.Quit() } catch {}
    Stop-Process -Id $excelPid -Force -ErrorAction SilentlyContinue
}
