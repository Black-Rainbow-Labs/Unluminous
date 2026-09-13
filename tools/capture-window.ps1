<#
.SYNOPSIS
  Photograph a window including any native child view inside it, without bringing it to the front.

.DESCRIPTION
  **`unluminous-cli window screenshot` is the one to reach for**, and this is the exception it cannot
  cover. That command reads back the surface Unluminous painted, which needs no focus and works on a
  window that is covered or on another desktop — but a **browser node's page is a native child window**
  that the operating system composites *on top* of that surface, so no picture Unluminous takes has ever
  held one. `task-1904` measured that and `space browser <node> shot` says so.

  `PrintWindow` with `PW_RENDERFULLCONTENT` asks the window itself to draw into a bitmap. It is
  documented to work on a window that is minimised or overlapped, and the flag exists for content that is
  composited rather than painted. Nothing here activates anything.

  What it really produced against a WebView2 child is recorded in
  `tasks/task-1914-testing-without-stealing-focus-tdd.md` §6. Read that before trusting a picture from
  here: an engine that renders through DirectComposition can answer this with a blank rectangle, and a
  blank rectangle looks like a page that failed to load.

.PARAMETER ProcessId
  The process whose main window to photograph. `unluminous-cli instances` prints these.

.PARAMETER Out
  Where to write the PNG.

.EXAMPLE
  pwsh tools/capture-window.ps1 -ProcessId 15852 -Out _agent_output/page.png
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][int]$ProcessId,
    [Parameter(Mandatory)][string]$Out
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

Add-Type @'
using System;
using System.Drawing;
using System.Runtime.InteropServices;

public static class WindowShot {
    [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }

    /// PW_RENDERFULLCONTENT: render content that is composited rather than painted. Windows 8.1 and up.
    const uint RenderFullContent = 0x00000002;

    public static Bitmap Of(IntPtr hwnd) {
        RECT rect;
        if (!GetWindowRect(hwnd, out rect)) throw new Exception("the window has no rectangle");
        int width = rect.Right - rect.Left, height = rect.Bottom - rect.Top;
        if (width <= 0 || height <= 0) throw new Exception("the window has no size");
        var picture = new Bitmap(width, height, System.Drawing.Imaging.PixelFormat.Format32bppArgb);
        using (var canvas = Graphics.FromImage(picture)) {
            IntPtr hdc = canvas.GetHdc();
            try { if (!PrintWindow(hwnd, hdc, RenderFullContent)) throw new Exception("PrintWindow refused"); }
            finally { canvas.ReleaseHdc(hdc); }
        }
        return picture;
    }
}
'@ -ReferencedAssemblies @(
    'System.Drawing.Common',
    'System.Drawing.Primitives',
    # PowerShell 7's `System.Drawing.Common` splits `Graphics` across two private assemblies, and
    # `Add-Type` does not pull them in on its own. Both are needed for `Graphics.FromImage`.
    'System.Private.Windows.GdiPlus',
    'System.Private.Windows.Core'
)

$process = Get-Process -Id $ProcessId -ErrorAction Stop
if ($process.MainWindowHandle -eq 0) { Write-Error "Process $ProcessId has no main window." }
$picture = [WindowShot]::Of($process.MainWindowHandle)
try {
    $full = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine((Get-Location).Path, $Out))
    $picture.Save($full, [System.Drawing.Imaging.ImageFormat]::Png)
    Write-Output "Wrote $full ($($picture.Width)x$($picture.Height))"
} finally {
    $picture.Dispose()
}
