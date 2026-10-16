# A drag of the window by its title bar, as Windows' move loop delivers it, without the pointer or the focus.
#
#   pwsh drag.ps1 <process id> [steps] [pause ms]
#
# Sends WM_ENTERSIZEMOVE, moves the window `steps` times by eight pixels with SetWindowPos (no activation),
# pausing between steps the way a mouse reports at about 125 times a second, and sends WM_EXITSIZEMOVE.
# SetWindowPos sends WM_WINDOWPOSCHANGED to the window's own thread and waits for it to be handled, so how
# long each step takes is how long the window kept the drag waiting. Prints one JSON line:
# {"steps":N,"median_ms":..,"p95_ms":..,"max_ms":..,"total_ms":..}
param([int]$ProcessId, [int]$Steps = 60, [int]$PauseMs = 8)

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class Drag {
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
'@

$hwnd = (Get-Process -Id $ProcessId).MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) { throw "process $ProcessId has no main window" }
$rect = New-Object Drag+RECT
[void][Drag]::GetWindowRect($hwnd, [ref]$rect)
$WM_ENTERSIZEMOVE = 0x0231; $WM_EXITSIZEMOVE = 0x0232
# SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE
$flags = 0x0001 -bor 0x0004 -bor 0x0010

[void][Drag]::SendMessageW($hwnd, $WM_ENTERSIZEMOVE, [IntPtr]::Zero, [IntPtr]::Zero)
$took = New-Object System.Collections.Generic.List[double]
$clock = [System.Diagnostics.Stopwatch]::StartNew()
for ($step = 1; $step -le $Steps; $step++) {
    $offset = if ($step -le $Steps / 2) { $step * 8 } else { ($Steps - $step) * 8 }
    $before = $clock.Elapsed.TotalMilliseconds
    [void][Drag]::SetWindowPos($hwnd, [IntPtr]::Zero, $rect.Left + $offset, $rect.Top, 0, 0, $flags)
    $took.Add($clock.Elapsed.TotalMilliseconds - $before)
    Start-Sleep -Milliseconds $PauseMs
}
[void][Drag]::SendMessageW($hwnd, $WM_EXITSIZEMOVE, [IntPtr]::Zero, [IntPtr]::Zero)
$total = $clock.Elapsed.TotalMilliseconds
$sorted = $took | Sort-Object
$median = $sorted[[int][Math]::Floor($sorted.Count / 2)]
$p95 = $sorted[[Math]::Min($sorted.Count - 1, [int][Math]::Floor($sorted.Count * 0.95))]
$max = $sorted[$sorted.Count - 1]
'{{"steps":{0},"median_ms":{1:F3},"p95_ms":{2:F3},"max_ms":{3:F3},"total_ms":{4:F1}}}' -f $Steps, $median, $p95, $max, $total
