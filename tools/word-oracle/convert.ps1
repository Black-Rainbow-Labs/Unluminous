# Convert a document with Microsoft Word, for the files Unluminous does not read or write itself
# (task-2359, TDD 5.6): .doc, .rtf, .odt, .wps and .pdf open as a .docx, and a .docx saves as .rtf or .odt.
#
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File convert.ps1 -Source <file> -Target <file> -Format docx|rtf|odt
#
# Word opens the source read only, with conversions not confirmed and alerts off, so a PDF is reflowed into
# a document without asking, and saves the target in the format asked for. The script prints `converted`
# and exits 0, or prints why it could not and exits 1. The Word it started is always quit, and stopped by
# its process id if it has not gone three seconds later: a Word started through COM can stay running after
# Quit, and nothing else would ever end it.
param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$Target,
    [ValidateSet('docx', 'rtf', 'odt')][string]$Format = 'docx'
)
$ErrorActionPreference = 'Stop'
$codes = @{ docx = 12; rtf = 6; odt = 23 }
# Word asks before it reflows a PDF, and DisplayAlerts does not stop that question, so the setting that
# does is turned on for this run and put back as it was afterwards.
$options = 'HKCU:\Software\Microsoft\Office\16.0\Word\Options'
$isPdf = $Source -match '\.pdf$'
$pdfWarning = $null
if ($isPdf -and (Test-Path $options)) {
    $pdfWarning = (Get-ItemProperty -Path $options -Name DisableConvertPdfWarning -ErrorAction SilentlyContinue).DisableConvertPdfWarning
    Set-ItemProperty -Path $options -Name DisableConvertPdfWarning -Value 1 -Type DWord
}
$word = $null
$wordPid = $null
$said = 'Word could not convert it'
$code = 1
try {
    $before = @(Get-Process WINWORD -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
    $word = New-Object -ComObject Word.Application
    $wordPid = @(Get-Process WINWORD -ErrorAction SilentlyContinue | ForEach-Object { $_.Id }) | Where-Object { $before -notcontains $_ } | Select-Object -First 1
    $word.Visible = $false
    $word.DisplayAlerts = 0
    $document = $word.Documents.Open($Source, $false, $true, $false)
    # The process that owns the document's window is this Word, whatever else is running.
    try {
        Add-Type -Namespace UnluminousConvert -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hwnd, out uint pid);'
        [uint32]$owner = 0
        [void][UnluminousConvert.Win]::GetWindowThreadProcessId([System.IntPtr][long]$document.Windows.Item(1).Hwnd, [ref]$owner)
        if ($owner -gt 0) { $wordPid = [int]$owner }
    } catch { }
    $document.SaveAs2($Target, $codes[$Format])
    $document.Close(0)
    $said = 'converted'
    $code = 0
} catch {
    if (-not $word) {
        $said = 'Microsoft Word is not installed, so this file cannot be converted'
    } else {
        $said = "Word could not convert it: $($_.Exception.Message)"
    }
} finally {
    if ($word) {
        try { $word.Quit(0) } catch { }
        try { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word) } catch { }
    }
    if ($wordPid) {
        $process = Get-Process -Id $wordPid -ErrorAction SilentlyContinue
        if ($process -and -not $process.WaitForExit(3000)) { Stop-Process -Id $wordPid -Force -ErrorAction SilentlyContinue }
    }
    if ($isPdf -and (Test-Path $options)) {
        if ($null -eq $pdfWarning) {
            Remove-ItemProperty -Path $options -Name DisableConvertPdfWarning -ErrorAction SilentlyContinue
        } else {
            Set-ItemProperty -Path $options -Name DisableConvertPdfWarning -Value $pdfWarning -Type DWord
        }
    }
}
Write-Output $said
exit $code
