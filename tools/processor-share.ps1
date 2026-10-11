<#
.SYNOPSIS
  Confines the calling script, and every process it starts from then on, to a share of the machine's
  logical processors. Dot-source it and call Enter-ProcessorShare.

.DESCRIPTION
  `task-2240`. The machine that releases Unluminous is the machine a person works at, and the machine
  other tickets render, train and build on. Before this, a release held all 24 logical processors at
  100% through its clippy, its test build and its release builds, and nothing in the scripts limited
  it.

  The limit is a processor affinity mask, taken from Inillucent's `task-2205`, which measured the
  alternatives on this same machine:

    - **A job object with a hard processor rate cap does not hold here.** `pwsh` on this machine is
      the Microsoft Store package. Windows runs a packaged program inside a job with
      JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, so every program it starts, cargo and rustc included,
      leaves every job in the chain. A five target build under an 80% rate cap still held the machine
      at a median of 96%.
    - **An affinity mask is not a job.** Windows copies it into every child at creation, so cmd, a
      second pwsh, cargo, rustc, link.exe and every test binary carry it. The processors outside the
      mask carry nothing from the release at any instant, which is what keeps a key press responsive.
    - **Below normal priority starved the build** behind other tickets' builds at normal priority, and
      the test binaries have their own time limits, so a starved run fails for no reason in the code.

  CARGO_BUILD_JOBS is set to the number of processors kept, unless the caller already set it, so cargo
  does not start more compilers than there are processors for them.

  The share is -Percent, else UNLUMINOUS_CPU_PERCENT, else 80. 100 changes nothing. A script started
  from one that already entered a share sees UNLUMINOUS_CPU_SHARE_ENTERED, keeps the mask it inherited,
  and does nothing more.
#>

if (-not ('UnluminousProcessorShare' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class UnluminousProcessorShare {
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool GetSystemCpuSetInformation(IntPtr information, uint length, out uint returned, IntPtr process, uint flags);
    // Every logical processor in group 0 with its efficiency class, as pairs of index and class.
    // Each SYSTEM_CPU_SET_INFORMATION entry starts with its own Size; Type is at 4, Group at 12,
    // LogicalProcessorIndex at 14 and EfficiencyClass at 18.
    public static List<int[]> Processors() {
        List<int[]> found = new List<int[]>();
        uint length;
        GetSystemCpuSetInformation(IntPtr.Zero, 0, out length, IntPtr.Zero, 0);
        if (length == 0) { return found; }
        IntPtr buffer = Marshal.AllocHGlobal((int)length);
        try {
            if (!GetSystemCpuSetInformation(buffer, length, out length, IntPtr.Zero, 0)) { return found; }
            int at = 0;
            while (at + 19 <= (int)length) {
                int size = Marshal.ReadInt32(buffer, at);
                if (size <= 0) { break; }
                if (Marshal.ReadInt32(buffer, at + 4) == 0 && Marshal.ReadInt16(buffer, at + 12) == 0) {
                    found.Add(new int[] { Marshal.ReadByte(buffer, at + 14), Marshal.ReadByte(buffer, at + 18) });
                }
                at += size;
            }
        } finally { Marshal.FreeHGlobal(buffer); }
        return found;
    }
}
'@
}

<#
.SYNOPSIS
  The share asked for: -Percent when it is not 0, else UNLUMINOUS_CPU_PERCENT, else 80.
.PARAMETER Percent
  The share passed by the caller, or 0 for the default.
#>
function Get-ProcessorSharePercent([int] $Percent) {
    if ($Percent -eq 0) {
        $Percent = if ($env:UNLUMINOUS_CPU_PERCENT) { [int]($env:UNLUMINOUS_CPU_PERCENT.Trim().TrimEnd('%')) } else { 80 }
    }
    if ($Percent -lt 1 -or $Percent -gt 100) { throw "the processor share must be from 1 to 100, not $Percent" }
    return $Percent
}

<#
.SYNOPSIS
  The affinity mask for a share: that many logical processors, fastest efficiency class first, then
  lowest index, chosen only among the processors in Allowed.
.PARAMETER Processors
  Pairs of logical processor index and efficiency class, from UnluminousProcessorShare.
.PARAMETER Allowed
  The mask the process already has. A share never adds a processor the caller was not given.
.PARAMETER Percent
  The share, from 1 to 100.
#>
function Get-ProcessorShareMask([object[]] $Processors, [int64] $Allowed, [int] $Percent) {
    $count = [math]::Max(1, [math]::Floor($Processors.Count * $Percent / 100))
    $kept = @($Processors | Where-Object { $Allowed -band ([int64]1 -shl $_[0]) } |
            Sort-Object -Property @{ Expression = { $_[1] }; Descending = $true }, @{ Expression = { $_[0] } } |
            Select-Object -First $count)
    $mask = [int64]0
    foreach ($processor in $kept) { $mask = $mask -bor ([int64]1 -shl $processor[0]) }
    return [pscustomobject]@{ Mask = $mask; Count = $kept.Count; Total = $Processors.Count }
}

<#
.SYNOPSIS
  Confines this process, and every process it starts from now on, to a share of the logical
  processors, and sets CARGO_BUILD_JOBS to match.
.DESCRIPTION
  The mask lasts as long as the process, so call it from a script run with `pwsh -File`, never from an
  interactive shell you mean to keep using with every processor.
.PARAMETER Percent
  The share, from 1 to 100. 0 means UNLUMINOUS_CPU_PERCENT, or 80.
#>
function Enter-ProcessorShare {
    param([int] $Percent = 0)
    if ($env:UNLUMINOUS_CPU_SHARE_ENTERED) { return }
    $Percent = Get-ProcessorSharePercent $Percent
    $env:UNLUMINOUS_CPU_SHARE_ENTERED = '1'
    if ($Percent -ge 100) {
        Write-Host '  processor share: every processor, uncapped (UNLUMINOUS_CPU_PERCENT=100)'
        return
    }
    if (-not $IsWindows) {
        $count = [math]::Max(1, [math]::Floor([Environment]::ProcessorCount * $Percent / 100))
        if (-not $env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = [string]$count }
        Write-Host "  processor share: CARGO_BUILD_JOBS=$($env:CARGO_BUILD_JOBS); no affinity mask is set on this platform"
        return
    }
    $me = Get-Process -Id $PID
    $allowed = [int64]$me.ProcessorAffinity
    $share = Get-ProcessorShareMask -Processors @([UnluminousProcessorShare]::Processors()) -Allowed $allowed -Percent $Percent
    if ($share.Mask -ne 0 -and $share.Mask -ne $allowed) { $me.ProcessorAffinity = [IntPtr]$share.Mask }
    if (-not $env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = [string]$share.Count }
    # Test binaries run one test per processor by default, and the libtest harness reads the machine's
    # count rather than the mask, so it is told the share as well.
    if (-not $env:RUST_TEST_THREADS) { $env:RUST_TEST_THREADS = [string]$share.Count }
    Write-Host ('  processor share: {0}% of {1} processors ({2} processors, mask 0x{3:X}), CARGO_BUILD_JOBS={4}; UNLUMINOUS_CPU_PERCENT=100 removes it' -f $Percent, $share.Total, $share.Count, [int64]$me.ProcessorAffinity, $env:CARGO_BUILD_JOBS)
}
