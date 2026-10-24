# Shared helpers: start a private Word instance and shut down only that instance.

# Starts Word hidden. Returns @{ App; Pid; Watchdog }. Pid is the one new WINWORD process (or $null).
# A watchdog process force-stops that one pid after $TimeoutSeconds in case a modal dialog hangs a COM call.
function Start-WordApp {
    param([int]$TimeoutSeconds = 900)
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
    return @{ App = $app; Pid = $newPid; Watchdog = $watchdog }
}

# Quits the Word instance started by Start-WordApp. Only that instance's pid is ever force stopped.
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
}
