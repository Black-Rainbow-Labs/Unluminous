# Helpers the Excel oracle scripts share: start Excel, and read a part out of an .xlsx without opening it.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
function Start-Xl { $x = New-Object -ComObject Excel.Application; $x.DisplayAlerts = $false; $x.Visible = $false; return $x }
function Read-Part([string]$Path, [string]$Part) {
    $z = [IO.Compression.ZipFile]::OpenRead($Path)
    try { $e = $z.GetEntry($Part); if ($null -eq $e) { return $null }; $r = New-Object IO.StreamReader($e.Open()); $t = $r.ReadToEnd(); $r.Close(); return $t } finally { $z.Dispose() }
}
function List-Parts([string]$Path) { $z = [IO.Compression.ZipFile]::OpenRead($Path); try { return $z.Entries.FullName } finally { $z.Dispose() } }
