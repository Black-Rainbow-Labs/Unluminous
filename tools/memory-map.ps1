<#
.SYNOPSIS
    Says what a running Unluminous window's memory is made of: its private memory, the program and DLL
    images it has loaded, and the files it has mapped, each as committed bytes and as bytes in the
    working set, plus the private working set Task Manager's Memory column shows.

.DESCRIPTION
    `task-2279`. Working set and private bytes say how much; this says where. It walks the process's
    address space with VirtualQueryEx and asks QueryWorkingSetEx which of the committed pages are resident.
    Images are listed by module, largest first, so a DLL that brought a big table with it shows by name.

.PARAMETER ProcessId
    The window's process id.

.PARAMETER Top
    How many modules to list.

.EXAMPLE
    pwsh tools/memory-map.ps1 -ProcessId 1234
    pwsh tools/memory-map.ps1 -ProcessId 1234 -Json
#>
param([Parameter(Mandatory)] [int] $ProcessId, [int] $Top = 12, [switch] $Json)

$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class MemoryMap {
    [StructLayout(LayoutKind.Sequential)]
    public struct MBI { public IntPtr BaseAddress; public IntPtr AllocationBase; public uint AllocationProtect; public uint __pad1; public IntPtr RegionSize; public uint State; public uint Protect; public uint Type; public uint __pad2; }
    [StructLayout(LayoutKind.Sequential)]
    public struct WSEX { public IntPtr VirtualAddress; public IntPtr Attributes; }
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    [DllImport("kernel32.dll")] static extern IntPtr VirtualQueryEx(IntPtr h, IntPtr addr, out MBI info, IntPtr len);
    [DllImport("psapi.dll")] static extern bool QueryWorkingSetEx(IntPtr h, [In, Out] WSEX[] pv, int cb);
    [DllImport("psapi.dll", CharSet = CharSet.Unicode)] static extern uint GetMappedFileNameW(IntPtr h, IntPtr addr, StringBuilder name, uint size);

    public class Totals { public long Committed; public long Resident; }
    public static Dictionary<string, Totals> Kinds = new Dictionary<string, Totals>();
    public static Dictionary<string, Totals> Modules = new Dictionary<string, Totals>();
    public static Dictionary<string, Totals> Allocations = new Dictionary<string, Totals>();

    static Totals Get(Dictionary<string, Totals> d, string k) { Totals t; if (!d.TryGetValue(k, out t)) { t = new Totals(); d[k] = t; } return t; }

    static long ResidentBytes(IntPtr h, long start, long size) {
        const int page = 4096; long pages = size / page; long resident = 0;
        for (long done = 0; done < pages; done += 8192) {
            int n = (int)Math.Min(8192, pages - done);
            var q = new WSEX[n];
            for (int i = 0; i < n; i++) q[i].VirtualAddress = new IntPtr(start + (done + i) * page);
            if (!QueryWorkingSetEx(h, q, n * Marshal.SizeOf(typeof(WSEX)))) continue;
            for (int i = 0; i < n; i++) if (((long)q[i].Attributes & 1) == 1) resident += page;
        }
        return resident;
    }

    public static void Walk(int pid) {
        IntPtr h = OpenProcess(0x0410, false, pid);
        if (h == IntPtr.Zero) throw new Exception("cannot open the process");
        try {
            long addr = 0; MBI m;
            while (VirtualQueryEx(h, new IntPtr(addr), out m, new IntPtr(Marshal.SizeOf(typeof(MBI)))) != IntPtr.Zero) {
                long size = (long)m.RegionSize;
                if (m.State == 0x1000) {
                    string kind = m.Type == 0x1000000 ? "image" : m.Type == 0x40000 ? "mapped" : "private";
                    long resident = ResidentBytes(h, (long)m.BaseAddress, size);
                    var k = Get(Kinds, kind); k.Committed += size; k.Resident += resident;
                    if (kind == "private") {
                        // Private memory by the reservation it belongs to, which is how a heap segment, a
                        // thread's stack and a driver's own block are told apart.
                        var t = Get(Allocations, "0x" + ((long)m.AllocationBase).ToString("x"));
                        t.Committed += size; t.Resident += resident;
                    } else {
                        var sb = new StringBuilder(1024);
                        GetMappedFileNameW(h, m.BaseAddress, sb, 1024);
                        string name = sb.ToString(); int slash = name.LastIndexOf('\\'); if (slash >= 0) name = name.Substring(slash + 1);
                        if (name.Length == 0) name = "(unnamed " + kind + ")";
                        var t = Get(Modules, kind + ": " + name); t.Committed += size; t.Resident += resident;
                    }
                }
                long next = (long)m.BaseAddress + size;
                if (next <= addr) break; addr = next;
            }
        } finally { CloseHandle(h); }
    }
}
'@

[MemoryMap]::Walk($ProcessId)
$p = Get-Process -Id $ProcessId
$privateWs = (Get-CimInstance Win32_PerfRawData_PerfProc_Process -Filter "IDProcess=$ProcessId").WorkingSetPrivate
$mb = { param($b) [math]::Round($b / 1MB, 1) }
$kinds = [ordered]@{}
foreach ($k in 'private', 'image', 'mapped') {
    $t = [MemoryMap]::Kinds[$k]
    $kinds[$k] = [ordered]@{ committed_mb = (& $mb $t.Committed); resident_mb = (& $mb $t.Resident) }
}
$modules = [MemoryMap]::Modules.GetEnumerator() | Sort-Object { $_.Value.Resident } -Descending | Select-Object -First $Top |
    ForEach-Object { [ordered]@{ name = $_.Key; committed_mb = (& $mb $_.Value.Committed); resident_mb = (& $mb $_.Value.Resident) } }
# Private reservations by how much of them is resident, bucketed by committed size so two runs compare.
$buckets = [ordered]@{}
foreach ($a in [MemoryMap]::Allocations.Values) {
    $size = $a.Committed
    $bucket = if ($size -lt 1MB) { 'under 1 MB' } elseif ($size -lt 4MB) { '1 to 4 MB' } elseif ($size -lt 16MB) { '4 to 16 MB' } elseif ($size -lt 64MB) { '16 to 64 MB' } else { '64 MB or more' }
    if (-not $buckets.Contains($bucket)) { $buckets[$bucket] = [ordered]@{ count = 0; committed_mb = 0.0; resident_mb = 0.0 } }
    $buckets[$bucket].count += 1
    $buckets[$bucket].committed_mb += (& $mb $size)
    $buckets[$bucket].resident_mb += (& $mb $a.Resident)
}
$largest = [MemoryMap]::Allocations.GetEnumerator() | Sort-Object { $_.Value.Resident } -Descending | Select-Object -First $Top |
    ForEach-Object { [ordered]@{ base = $_.Key; committed_mb = (& $mb $_.Value.Committed); resident_mb = (& $mb $_.Value.Resident) } }
$report = [ordered]@{
    pid = $ProcessId
    working_set_mb = (& $mb $p.WorkingSet64)
    private_bytes_mb = (& $mb $p.PrivateMemorySize64)
    private_working_set_mb = (& $mb $privateWs)
    kinds = $kinds
    modules = @($modules)
    private_buckets = $buckets
    private_largest = @($largest)
}
if ($Json) { $report | ConvertTo-Json -Depth 5 -Compress; return }
'working set {0} MB, private bytes {1} MB, private working set (Task Manager) {2} MB' -f $report.working_set_mb, $report.private_bytes_mb, $report.private_working_set_mb
foreach ($k in $kinds.Keys) { '  {0,-8} committed {1,8} MB  resident {2,8} MB' -f $k, $kinds[$k].committed_mb, $kinds[$k].resident_mb }
'  largest images and mapped files by resident bytes:'
foreach ($m in $modules) { '    {0,8} MB resident {1,8} MB committed  {2}' -f $m.resident_mb, $m.committed_mb, $m.name }
'  private memory by reservation size:'
foreach ($b in $buckets.Keys) { '    {0,-14} {1,5} reservations  committed {2,8:N1} MB  resident {3,8:N1} MB' -f $b, $buckets[$b].count, $buckets[$b].committed_mb, $buckets[$b].resident_mb }
'  largest private reservations by resident bytes:'
foreach ($l in $largest) { '    {0,8} MB resident {1,8} MB committed  {2}' -f $l.resident_mb, $l.committed_mb, $l.base }
