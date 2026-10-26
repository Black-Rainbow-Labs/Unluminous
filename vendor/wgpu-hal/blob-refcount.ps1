# Asks whether d3dcompiler_47.dll's ID3DBlob counts its references atomically.
# Several threads AddRef and Release one blob in a tight loop; every thread's pairs balance, so a
# count kept atomically is back at 1 at the end. Run against a blob made by D3DCreateBlob and against
# one made by D3DCompile, which is the kind wgpu-hal caches.
$source = @'
using System;
using System.Runtime.InteropServices;
using System.Threading;

public static class BlobProbe {
    [DllImport("d3dcompiler_47.dll")]
    static extern int D3DCreateBlob(UIntPtr size, out IntPtr blob);

    [DllImport("d3dcompiler_47.dll", CharSet = CharSet.Ansi)]
    static extern int D3DCompile(byte[] src, UIntPtr len, string name, IntPtr defines, IntPtr include,
        string entry, string target, uint flags1, uint flags2, out IntPtr code, out IntPtr errors);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    delegate uint RefFn(IntPtr self);

    public static IntPtr Created() { IntPtr b; Marshal.ThrowExceptionForHR(D3DCreateBlob((UIntPtr)64, out b)); return b; }

    public static IntPtr Compiled() {
        var text = System.Text.Encoding.ASCII.GetBytes("float4 main(float4 p : POSITION) : SV_Position { return p; }");
        IntPtr code, errors;
        Marshal.ThrowExceptionForHR(D3DCompile(text, (UIntPtr)text.Length, null, IntPtr.Zero, IntPtr.Zero,
            "main", "vs_5_1", 0, 0, out code, out errors));
        return code;
    }

    public static uint Hammer(IntPtr blob, int threads, int pairs) {
        IntPtr vtable = Marshal.ReadIntPtr(blob);
        var addRef = Marshal.GetDelegateForFunctionPointer<RefFn>(Marshal.ReadIntPtr(vtable, IntPtr.Size));
        var release = Marshal.GetDelegateForFunctionPointer<RefFn>(Marshal.ReadIntPtr(vtable, 2 * IntPtr.Size));
        // Hold extra references so a lost update shows as a wrong count rather than a freed blob.
        for (int i = 0; i < 1000; i++) addRef(blob);
        var go = new ManualResetEventSlim(false);
        var all = new Thread[threads];
        for (int t = 0; t < threads; t++) {
            all[t] = new Thread(() => { go.Wait(); for (int i = 0; i < pairs; i++) { addRef(blob); release(blob); } });
            all[t].Start();
        }
        go.Set();
        foreach (var t in all) t.Join();
        // Expected: 1 + 1000. AddRef then Release reads the count back without changing it.
        addRef(blob);
        return release(blob);
    }
}
'@
Add-Type -TypeDefinition $source
foreach ($kind in 'Created', 'Compiled') {
    foreach ($run in 1..3) {
        $blob = [BlobProbe]::$kind()
        $count = [BlobProbe]::Hammer($blob, 16, 200000)
        "$kind blob, run ${run}: count after balanced AddRef/Release from 16 threads = $count (atomic would be 1001)"
    }
}
