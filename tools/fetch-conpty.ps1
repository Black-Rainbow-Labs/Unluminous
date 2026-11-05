<#
.SYNOPSIS
Puts Microsoft's own pseudoconsole, conpty.dll and OpenConsole.exe, in a folder, from the
Microsoft.Windows.Console.ConPTY NuGet package.

.DESCRIPTION
`task-2514`. The pseudoconsole built into Windows removes the kitty graphics (APC) and sixel (DCS) sequences
before a terminal sees them, so a picture a program sends never reaches Unluminous's terminal. The newer
pseudoconsole Microsoft publishes on NuGet passes them through byte for byte, which is why Termoil ships it, and
`termoil-pty` loads it from the folder the executable is in when it is there (`ConptyChoice::Auto`).

The version and the SHA-256 of each file are in installer/windows/conpty.version. A folder that already holds
both files with those hashes is left alone, so a second build downloads nothing. A file whose hash does not
match is refused rather than shipped. Runs without prompts:

    pwsh -NoProfile -File tools/fetch-conpty.ps1 -Destination <folder>
#>
param(
    [Parameter(Mandatory = $true)][string]$Destination,
    [string]$Arch = 'x64'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$lines = Get-Content -LiteralPath (Join-Path $root 'installer/windows/conpty.version')
$version = $lines[0].Trim()
$wanted = @{}
foreach ($line in $lines | Select-Object -Skip 1) {
    $parts = $line.Trim() -split '\s+'
    if ($parts.Count -eq 2) { $wanted[$parts[0]] = $parts[1].ToLowerInvariant() }
}

function Test-Hashes([string]$folder) {
    foreach ($name in $wanted.Keys) {
        $file = Join-Path $folder $name
        if (-not (Test-Path -LiteralPath $file)) { return $false }
        if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -ne $wanted[$name]) { return $false }
    }
    return $true
}

New-Item -ItemType Directory -Force -Path $Destination | Out-Null
if (Test-Hashes $Destination) {
    Write-Host "conpty.dll and OpenConsole.exe $version are already in $Destination"
    return
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("unluminous-conpty-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
try {
    $package = Join-Path $work 'conpty.zip'
    $url = "https://www.nuget.org/api/v2/package/Microsoft.Windows.Console.ConPTY/$version"
    Write-Host "Downloading $url"
    Invoke-WebRequest -Uri $url -OutFile $package -UseBasicParsing
    $extracted = Join-Path $work 'x'
    Expand-Archive -LiteralPath $package -DestinationPath $extracted -Force
    foreach ($name in $wanted.Keys) {
        $found = Get-ChildItem -Path $extracted -Recurse -Filter $name |
            Where-Object { $_.FullName.Replace('\', '/') -like "*$Arch/*" } |
            Select-Object -First 1
        if (-not $found) { throw "$name for $Arch is not in the package." }
        $hash = (Get-FileHash -LiteralPath $found.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($hash -ne $wanted[$name]) {
            throw "$name in the package hashes to $hash, not the $($wanted[$name]) installer/windows/conpty.version records."
        }
        Copy-Item -LiteralPath $found.FullName -Destination (Join-Path $Destination $name) -Force
    }
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
if (-not (Test-Hashes $Destination)) { throw "The files copied to $Destination do not have the recorded hashes." }
Write-Host "conpty.dll and OpenConsole.exe $version ($Arch) are in $Destination"
