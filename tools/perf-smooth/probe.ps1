# How long the window keeps Windows waiting, asked from outside the process (`task-2288`).
#
#   pwsh tools/perf-smooth/probe.ps1
#
# A long lived helper for `bench.mjs`. It reads one command a line and answers one JSON line:
#
#   watch <pid> <period ms>   start sending the window WM_NULL every period and timing each answer
#   stop                      stop, and answer the timings: {"count":..,"p50":..,"p95":..,"p99":..,"max":..}
#   drag <pid> <steps> <ms>   a drag of the window as Windows' move loop delivers it (see glass-bench/drag.ps1)
#   quit                      leave
#
# Every message a person's mouse makes the window answer goes through the same queue: the hit test that
# picks the cursor, each step of a drag, each step of a resize. WM_NULL does nothing, so how long one waits
# for its answer is how long the window's thread took to get back to its message queue. This helper runs at
# high priority so that its own timing is not what a busy machine slows down.
#
# Each timing is also kept with the time it was sent, in milliseconds since 1970, so a harness can line
# the timings up with the cases it ran: `stop` answers `samples` as pairs of [sent, waited].

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;
public static class Probe {
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeoutW(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("winmm.dll")] static extern uint timeBeginPeriod(uint period);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }

    static Thread worker;
    static volatile bool running;
    static List<double[]> samples = new List<double[]>();

    static double Now() { return (DateTime.UtcNow - new DateTime(1970, 1, 1, 0, 0, 0, DateTimeKind.Utc)).TotalMilliseconds; }

    public static void Watch(IntPtr hwnd, int period) {
        timeBeginPeriod(1);
        lock (samples) { samples.Clear(); }
        running = true;
        worker = new Thread(() => {
            var clock = Stopwatch.StartNew();
            while (running) {
                double sent = Now();
                double before = clock.Elapsed.TotalMilliseconds;
                IntPtr ignored;
                // SMTO_NORMAL with a two second limit: a window that does not answer in two seconds is hung,
                // and the limit is what is recorded.
                SendMessageTimeoutW(hwnd, 0, IntPtr.Zero, IntPtr.Zero, 0, 2000, out ignored);
                double waited = clock.Elapsed.TotalMilliseconds - before;
                lock (samples) { samples.Add(new double[] { sent, waited }); }
                int rest = period - (int)waited;
                if (rest > 0) Thread.Sleep(rest);
            }
        });
        worker.Priority = ThreadPriority.Highest;
        worker.IsBackground = true;
        worker.Start();
    }

    public static List<double[]> Stop() {
        running = false;
        if (worker != null) worker.Join();
        lock (samples) { return new List<double[]>(samples); }
    }

    public static double[] Drag(IntPtr hwnd, int steps, int pause) {
        timeBeginPeriod(1);
        RECT rect;
        GetWindowRect(hwnd, out rect);
        var took = new double[steps];
        SendMessageW(hwnd, 0x0231, IntPtr.Zero, IntPtr.Zero);
        var clock = Stopwatch.StartNew();
        for (int step = 1; step <= steps; step++) {
            int offset = step <= steps / 2 ? step * 8 : (steps - step) * 8;
            double before = clock.Elapsed.TotalMilliseconds;
            // SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE
            SetWindowPos(hwnd, IntPtr.Zero, rect.Left + offset, rect.Top, 0, 0, 0x0001 | 0x0004 | 0x0010);
            took[step - 1] = clock.Elapsed.TotalMilliseconds - before;
            Thread.Sleep(pause);
        }
        SendMessageW(hwnd, 0x0232, IntPtr.Zero, IntPtr.Zero);
        return took;
    }
}
'@

(Get-Process -Id $PID).PriorityClass = 'High'

<#
.SYNOPSIS
  The window handle of a process, waiting a little for it to have one.
.PARAMETER ProcessId
  The process.
#>
function Get-WindowOf([int] $ProcessId) {
    for ($attempt = 0; $attempt -lt 50; $attempt++) {
        $hwnd = (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue).MainWindowHandle
        if ($hwnd -and $hwnd -ne [IntPtr]::Zero) { return $hwnd }
        Start-Sleep -Milliseconds 100
    }
    throw "process $ProcessId has no main window"
}

<#
.SYNOPSIS
  The middle, the 95th and 99th percentiles and the largest of some timings, as a JSON line.
.PARAMETER Values
  The timings in milliseconds.
.PARAMETER Samples
  Pairs of when each was sent and how long it waited, written out when given.
#>
function Format-Timings([double[]] $Values, $Samples) {
    if ($Values.Count -eq 0) { return '{"count":0}' }
    $sorted = $Values | Sort-Object
    $at = { param($share) $sorted[[Math]::Min($sorted.Count - 1, [int][Math]::Floor($sorted.Count * $share))] }
    $summary = [ordered]@{
        count = $sorted.Count; p50 = & $at 0.5; p95 = & $at 0.95; p99 = & $at 0.99; max = $sorted[$sorted.Count - 1]
        over8 = @($Values | Where-Object { $_ -gt 8 }).Count; over16 = @($Values | Where-Object { $_ -gt 16 }).Count
        over50 = @($Values | Where-Object { $_ -gt 50 }).Count
    }
    if ($Samples) { $summary.samples = @($Samples | ForEach-Object { , @([Math]::Round($_[0], 3), [Math]::Round($_[1], 3)) }) }
    return ($summary | ConvertTo-Json -Compress -Depth 4)
}

while ($true) {
    $line = [Console]::In.ReadLine()
    if ($null -eq $line) { break }
    $words = $line.Trim() -split '\s+'
    try {
        switch ($words[0]) {
            'watch' { [Probe]::Watch((Get-WindowOf ([int]$words[1])), [int]$words[2]); [Console]::Out.WriteLine('{"watching":true}') }
            'stop' {
                $samples = [Probe]::Stop()
                [Console]::Out.WriteLine((Format-Timings @($samples | ForEach-Object { $_[1] }) $samples))
            }
            'drag' {
                $took = [Probe]::Drag((Get-WindowOf ([int]$words[1])), [int]$words[2], [int]$words[3])
                [Console]::Out.WriteLine((Format-Timings $took $null))
            }
            'quit' { [void][Probe]::Stop(); exit 0 }
            default { [Console]::Out.WriteLine('{"error":"unknown command"}') }
        }
    } catch {
        [Console]::Out.WriteLine((@{ error = "$_" } | ConvertTo-Json -Compress))
    }
    [Console]::Out.Flush()
}
