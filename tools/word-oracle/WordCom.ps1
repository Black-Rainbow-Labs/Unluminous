# Shared helpers: start a private Word instance and shut down only that instance.
#
# **The fonts Office downloads are loaded into the session while Word runs (task-2359).** Office keeps its
# cloud fonts, Aptos among them, in %LOCALAPPDATA%\Microsoft\FontCache\4\CloudFonts, and a Word started
# through COM does not see them: its FontNames list has Calibri and no Aptos, and it lays Aptos text out
# with Calibri's widths. Every Aptos page break and line break measured before this was Calibri's, which
# is the "three percent narrower" round 1 recorded. Start-WordApp adds those files with AddFontResource
# and Stop-WordApp removes them again, so the oracle lays Aptos out with the font a person's own Word uses.

Add-Type -Namespace WordOracle -Name Fonts -MemberDefinition @'
[DllImport("gdi32.dll", CharSet = CharSet.Unicode)] public static extern int AddFontResourceW(string file);
[DllImport("gdi32.dll", CharSet = CharSet.Unicode)] public static extern bool RemoveFontResourceW(string file);
[DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, IntPtr w, IntPtr l, uint flags, uint timeout, out IntPtr result);
'@ -ErrorAction SilentlyContinue

# The font files Office downloaded for this person.
function Get-CloudFonts {
    $folder = Join-Path $env:LOCALAPPDATA 'Microsoft\FontCache\4\CloudFonts'
    if (-not (Test-Path $folder)) { return @() }
    return @(Get-ChildItem -Path $folder -Recurse -File | Where-Object { $_.Extension -in '.ttf', '.otf', '.ttc' } | ForEach-Object { $_.FullName })
}

# Tells every window the fonts changed, so a Word started afterwards enumerates them.
function Send-FontChange {
    $result = [IntPtr]::Zero
    [void][WordOracle.Fonts]::SendMessageTimeout([IntPtr]0xffff, 0x001D, [IntPtr]::Zero, [IntPtr]::Zero, 2, 1000, [ref]$result)
}

# Starts Word hidden. Returns @{ App; Pid; Watchdog; Fonts }. Pid is the one new WINWORD process (or $null).
# A watchdog process force-stops that one pid after $TimeoutSeconds in case a modal dialog hangs a COM call.
function Start-WordApp {
    param([int]$TimeoutSeconds = 900)
    $fonts = @(Get-CloudFonts | Where-Object { [WordOracle.Fonts]::AddFontResourceW($_) -gt 0 })
    if ($fonts.Count -gt 0) { Send-FontChange }
    $before = @(Get-Process WINWORD -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
    $app = New-Object -ComObject Word.Application
    $after = @(Get-Process WINWORD -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
    $newPid = $after | Where-Object { $before -notcontains $_ } | Select-Object -First 1
    $app.Visible = $false
    $app.DisplayAlerts = 0
    try { $app.Options.SaveNormalPrompt = $false } catch { }
    $watchdog = $null
    if ($newPid) {
        $cmd = "Start-Sleep -Seconds $TimeoutSeconds; Stop-Process -Id $newPid -Force -ErrorAction SilentlyContinue"
        $watchdog = Start-Process pwsh -ArgumentList '-NoProfile', '-Command', $cmd -PassThru -WindowStyle Hidden
    }
    return @{ App = $app; Pid = $newPid; Watchdog = $watchdog; Fonts = $fonts }
}

# Quits the Word instance started by Start-WordApp. Only that instance's pid is ever force stopped. The
# fonts it added are removed again.
function Stop-WordApp {
    param($Handle)
    if (-not $Handle) { return }
    try { $Handle.App.Quit(0) } catch { }
    try { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($Handle.App) } catch { }
    if ($Handle.Pid) {
        $p = Get-Process -Id $Handle.Pid -ErrorAction SilentlyContinue
        if ($p -and -not $p.WaitForExit(10000)) { Stop-Process -Id $Handle.Pid -Force -ErrorAction SilentlyContinue }
    }
    if ($Handle.Watchdog) { Stop-Process -Id $Handle.Watchdog.Id -Force -ErrorAction SilentlyContinue }
    foreach ($font in @($Handle.Fonts)) { [void][WordOracle.Fonts]::RemoveFontResourceW($font) }
    if (@($Handle.Fonts).Count -gt 0) { Send-FontChange }
}
