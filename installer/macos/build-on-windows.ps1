<#
.SYNOPSIS
    Builds, signs and notarises Unluminous.app on the Windows machine.

.DESCRIPTION
    `installer/macos/build.sh` does this on a Mac and still does. This is the
    same bundle without one (task-1995). Every Apple program it used has a
    replacement that runs here:

        lipo        -> rcodesign macho-universal-create
        codesign    -> rcodesign sign, which signs a bundle recursively and
                       writes its _CodeSignature/CodeResources
        notarytool  -> rcodesign notary-submit
        stapler     -> rcodesign staple
        iconutil    -> not needed; installer/icon/unluminous.icns is committed
        plutil      -> .NET's XML reader, which is what the lint was for

    THE ONE THING THIS MACHINE CANNOT SUPPLY BY ITSELF: THE macOS SDK

    Unluminous is a windowed application. The link line asks for `-lobjc` and for
    ApplicationServices, AppKit, Carbon, CoreGraphics, CoreVideo, Foundation,
    CoreFoundation, Metal, QuartzCore, Security and WebKit. zig ships a stub for
    `libSystem` and for nothing else, so the link stops at the first of them:

        error: unable to find dynamic system library 'objc' using strategy
        'paths_first'

    measured here on 2026-09-19. Those stubs come from Apple's SDK, which is in
    Xcode and in the Command Line Tools. **Apple's licence for it says Apple-
    branded hardware**, so whether a copy may sit on this machine is a decision
    for the person who accepted that licence, not one a script should make
    quietly. This script therefore never downloads an SDK: it looks for one, and
    says exactly what is missing when there is none.

    Point it at one with -Sdk, with $env:SDKROOT, or by putting it at
    tools/cross/sdk/MacOSX.sdk. On a Mac it is at
    /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk, or inside Xcode at
    Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk; the
    whole directory is copied, and about 1.5 GB of it is frameworks.

    `unluminous-cli` needs none of this. It links no framework and cross compiles
    here with no SDK at all, which -CliOnly does.

    WHAT IS DELIVERED, AND WHY IT IS A .zip RATHER THAN A .dmg

    A `.dmg` holds an HFS+ filesystem, and writing one needs `hdiutil` or a
    reimplementation of that filesystem; rcodesign signs a disk image but does
    not create one. A zipped bundle is the other container Apple's notary
    accepts, and the ticket is stapled to the **application** inside it rather
    than to the zip, so what a person ends up running carries its own ticket and
    opens with no network. `installer/macos/build.sh` still writes the .dmg on a
    Mac; this writes releases/Unluminous-<version>-macos.zip.

.PARAMETER Version
    The version the bundle claims. Defaults to the workspace version.

.PARAMETER Sdk
    The macOS SDK directory.

.PARAMETER SkipBuild
    Assemble, sign and package what is already built.

.PARAMETER CliOnly
    Build only `unluminous-cli`, which needs no SDK. For checking that the macOS
    cross toolchain still works on a machine with no SDK on it.

.PARAMETER SelfSigned
    Sign with a certificate generated on the spot. Proves every step except the
    two that are about Apple's opinion of the certificate.

.PARAMETER Notarize
    Submit the zip to Apple, wait, and staple the ticket to the bundle.

.EXAMPLE
    pwsh installer\macos\build-on-windows.ps1 -CliOnly
    pwsh installer\macos\build-on-windows.ps1 -Sdk J:\mac-sdk\MacOSX.sdk -Notarize
#>
[CmdletBinding()]
param(
    [string] $Version,
    [string] $Sdk,
    [switch] $SkipBuild,
    [switch] $CliOnly,
    [switch] $SelfSigned,
    [switch] $Notarize
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$crossBin = Join-Path $repo 'tools/cross/bin'
$rcodesign = Join-Path $crossBin 'rcodesign.exe'
$cargoZigbuild = Join-Path $crossBin 'cargo-zigbuild.exe'
$zigDir = Join-Path $crossBin 'zig'
$dist = Join-Path $repo 'installer/dist'
$app = Join-Path $dist 'Unluminous.app'
$releases = Join-Path $repo 'releases'

# The two Mach-O files inside Contents/MacOS. `unluminous-cli` is signed first and
# the bundle is signed round it: codesign, and rcodesign after it, treat a second
# binary in Contents/MacOS as a nested entity, and an unsigned one inside a
# signed bundle is what makes a verification fail on a bundle that looked fine.
$programs = @(
    @{ Package = 'unluminous-app'; Bin = 'unluminous' },
    @{ Package = 'unluminous-cli'; Bin = 'unluminous-cli' }
)

$appleTargets = @('aarch64-apple-darwin', 'x86_64-apple-darwin')

function Invoke-Rcodesign {
    <#
    .SYNOPSIS
        Runs rcodesign and throws if it failed, so no step is skipped silently.

    .PARAMETER Arguments
        The whole command line.

    .PARAMETER Quiet
        Discard its output.
    #>
    param([string[]] $Arguments, [switch] $Quiet)
    if ($Quiet) { & $rcodesign @Arguments *> $null } else { & $rcodesign @Arguments }
    if ($LASTEXITCODE -ne 0) { throw "rcodesign $($Arguments[0]) failed with $LASTEXITCODE" }
}

function Get-CargoTargetDir {
    <#
    .SYNOPSIS
        Where cargo puts its output, honouring CARGO_TARGET_DIR.

    .DESCRIPTION
        Two Apple architectures of a windowed application is tens of gigabytes
        of intermediate objects. Reading the variable rather than assuming
        <repo>/target is what lets that sit on another drive.
    #>
    if ($env:CARGO_TARGET_DIR) { return $env:CARGO_TARGET_DIR }
    return (Join-Path $repo 'target')
}

function Get-WorkspaceVersion {
    <#
    .SYNOPSIS
        Reads the version out of Cargo.toml's [workspace.package], which is the
        one place it is written down.
    #>
    $manifest = Get-Content -Path (Join-Path $repo 'Cargo.toml') -Raw
    if ($manifest -match '(?ms)\[workspace\.package\].*?version\s*=\s*"([^"]+)"') { return $Matches[1] }
    throw 'Cargo.toml does not declare [workspace.package] version'
}

function Resolve-Sdk {
    <#
    .SYNOPSIS
        Finds the macOS SDK, or explains precisely what is missing.

    .DESCRIPTION
        The message is long on purpose. A release that stops here stops because
        of a licence decision and a 1.5 GB copy, and "SDK not found" would send
        the reader to a search engine for both.
    #>
    foreach ($candidate in @($Sdk, $env:SDKROOT, (Join-Path $repo 'tools/cross/sdk/MacOSX.sdk'))) {
        if ($candidate -and (Test-Path -LiteralPath $candidate)) {
            if (-not (Test-Path -LiteralPath (Join-Path $candidate 'System/Library/Frameworks/AppKit.framework'))) {
                throw "$candidate does not hold System/Library/Frameworks/AppKit.framework, so it is not a macOS SDK"
            }
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }
    throw @"
No macOS SDK. Unluminous links AppKit, Metal, QuartzCore, WebKit and eight more
frameworks, and zig ships a stub for libSystem and nothing else, so the link
stops at `-lobjc` before it reaches any of them.

Copy one from a Mac:
  /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk
  or <Xcode>/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk

and put it at tools/cross/sdk/MacOSX.sdk, or name it with -Sdk or `$env:SDKROOT.

Apple's licence for that SDK says Apple-branded hardware, so whether a copy
belongs on this machine is a decision for whoever accepted it. Nothing here
downloads one.

-CliOnly builds unluminous-cli, which links no framework and needs no SDK.
"@
}

function Build-AppleTarget {
    <#
    .SYNOPSIS
        Cross compiles one Apple architecture with zig as the linker.

    .DESCRIPTION
        -headerpad_max_install_names reserves room for the code signature load
        command. A Mach-O linked by zig for x86-64 has neither the command nor
        space to add one, and rcodesign then refuses it.

    .PARAMETER Target
        The triple.

    .PARAMETER SdkRoot
        The macOS SDK, or an empty string when only the CLI is being built.
    #>
    param([string] $Target, [string] $SdkRoot)

    if (-not (Test-Path -LiteralPath $cargoZigbuild)) {
        throw 'cargo-zigbuild is missing. Run: pwsh tools/cross/fetch-toolchain.ps1'
    }
    $packages = if ($CliOnly) {
        @('-p', 'unluminous-cli', '--bin', 'unluminous-cli')
    } else {
        @('-p', 'unluminous-app', '--bin', 'unluminous', '-p', 'unluminous-cli', '--bin', 'unluminous-cli')
    }

    $previousPath = $env:PATH
    $previousSdk = $env:SDKROOT
    $env:PATH = "$zigDir;$env:PATH"
    if ($SdkRoot) { $env:SDKROOT = $SdkRoot }
    try {
        & $cargoZigbuild zigbuild --manifest-path (Join-Path $repo 'Cargo.toml') `
            --release --locked --target $Target `
            --config "target.$Target.rustflags=[`"-C`",`"link-arg=-Wl,-headerpad_max_install_names`"]" `
            @packages
        if ($LASTEXITCODE -ne 0) { throw "cargo-zigbuild failed for $Target with $LASTEXITCODE" }
    } finally {
        $env:PATH = $previousPath
        $env:SDKROOT = $previousSdk
    }
}

function New-UniversalBinary {
    <#
    .SYNOPSIS
        Joins the two architectures of one program and checks both slices are
        in the result.

    .DESCRIPTION
        A universal binary with one slice runs on the machine that made it and
        on half the Macs that download it, so the check is what makes this
        different from "the command exited 0".

    .PARAMETER Name
        The program's file name.

    .PARAMETER Into
        The directory to write it to.
    #>
    param([string] $Name, [string] $Into)

    $targetDir = Get-CargoTargetDir
    $slices = $appleTargets | ForEach-Object { Join-Path $targetDir "$_/release/$Name" }
    foreach ($slice in $slices) {
        if (-not (Test-Path -LiteralPath $slice)) { throw "the build did not produce $slice" }
    }
    $output = Join-Path $Into $Name
    Invoke-Rcodesign -Quiet -Arguments (@('macho-universal-create', '--output', $output) + $slices)

    $seen = @()
    foreach ($index in 0, 1) {
        $header = & $rcodesign extract macho-header --universal-index $index $output 2>&1
        if ($LASTEXITCODE -ne 0) { throw "$output has no slice at index $index" }
        $seen += ($header | Select-String -Pattern 'cputype: (\d+)').Matches.Groups[1].Value
    }
    # 16777228 is arm64 and 16777223 is x86-64, as Mach-O numbers them.
    foreach ($wanted in '16777228', '16777223') {
        if ($seen -notcontains $wanted) { throw "$output is missing the cputype $wanted slice" }
    }
    return $output
}

function New-AppBundle {
    <#
    .SYNOPSIS
        Assembles Unluminous.app: the two programs, the icon, the manifest and
        PkgInfo.

    .DESCRIPTION
        The same layout build.sh writes, because a bundle built here and one
        built on a Mac have to be the same application. The icon is the
        committed installer/icon/unluminous.icns rather than one drawn by
        iconutil, which build.sh already falls back to on a machine without it.

    .PARAMETER Version
        The version written into Info.plist.
    #>
    param([string] $Version)

    if (Test-Path -LiteralPath $app) { Remove-Item -LiteralPath $app -Recurse -Force -Confirm:$false }
    $macos = Join-Path $app 'Contents/MacOS'
    $resources = Join-Path $app 'Contents/Resources'
    New-Item -ItemType Directory -Force -Path $macos, $resources | Out-Null

    foreach ($program in $programs) {
        New-UniversalBinary -Name $program.Bin -Into $macos | Out-Null
    }

    Copy-Item -LiteralPath (Join-Path $repo 'installer/icon/unluminous.icns') `
        -Destination (Join-Path $resources 'Unluminous.icns') -Force

    $plist = (Get-Content -Path (Join-Path $PSScriptRoot 'Info.plist') -Raw).Replace('__VERSION__', $Version)
    $plistPath = Join-Path $app 'Contents/Info.plist'
    [System.IO.File]::WriteAllText($plistPath, $plist)
    # What plutil -lint did on the Mac: refuse a manifest that is not
    # well-formed XML, here rather than when the application will not start.
    $document = New-Object System.Xml.XmlDocument
    $document.XmlResolver = $null
    $document.Load($plistPath)

    [System.IO.File]::WriteAllText((Join-Path $app 'Contents/PkgInfo'), 'APPL????')
    return $app
}

function New-SigningSession {
    <#
    .SYNOPSIS
        Works out how to sign, and returns the rcodesign arguments plus whatever
        has to be deleted afterwards.

    .DESCRIPTION
        There is no keychain on Windows, so CODESIGN_IDENTITY - a keychain
        identity name - cannot be used here. What takes its place is a .p12,
        which is what a Mac's Keychain Access exports an identity as, named by
        CODESIGN_P12 in the same installer/macos/notarize.env the rest of the
        credentials already live in.

        CODESIGN_P12_PASSWORD_FILE is preferred and holds the password sealed
        with DPAPI, which encrypts under this Windows account, so the file is
        worthless on another machine. CODESIGN_P12_PASSWORD is the plain
        alternative for a one-off run.
    #>
    $scratch = if (Test-Path -LiteralPath 'R:\') { 'R:\unluminous-release' } else { Join-Path $env:TEMP 'unluminous-release' }
    New-Item -ItemType Directory -Force -Path $scratch | Out-Null
    $stamp = [guid]::NewGuid().ToString('N')

    if ($SelfSigned) {
        $pem = Join-Path $scratch "selfsigned-$stamp.pem"
        Invoke-Rcodesign -Quiet -Arguments @(
            'generate-self-signed-certificate', '--algorithm', 'rsa',
            '--profile', 'developer-id-application', '--team-id', 'SELFSIGNED',
            '--person-name', 'Unluminous self-signed build', '--validity-days', '30',
            '--pem-unified-file', $pem)
        return [pscustomobject]@{ Arguments = @('--pem-file', $pem); Scratch = @($pem); Kind = 'self-signed' }
    }

    if (-not $env:CODESIGN_P12) {
        throw 'CODESIGN_P12 is not set. It names a .p12 holding the Developer ID Application identity, exported from a Mac''s Keychain Access. installer/macos/notarize.env is where it belongs. Or pass -SelfSigned to prove the pipeline.'
    }
    if (-not (Test-Path -LiteralPath $env:CODESIGN_P12)) { throw "CODESIGN_P12 points at $($env:CODESIGN_P12), which is not there" }

    $passwordFile = Join-Path $scratch "p12-$stamp.txt"
    if ($env:CODESIGN_P12_PASSWORD_FILE) {
        $secure = Get-Content -Path $env:CODESIGN_P12_PASSWORD_FILE -Raw | ConvertTo-SecureString
        Set-Content -Path $passwordFile -Value ([System.Net.NetworkCredential]::new('', $secure).Password) -NoNewline
    } elseif ($null -ne $env:CODESIGN_P12_PASSWORD) {
        Set-Content -Path $passwordFile -Value $env:CODESIGN_P12_PASSWORD -NoNewline
    } else {
        throw 'CODESIGN_P12 is set but neither CODESIGN_P12_PASSWORD_FILE nor CODESIGN_P12_PASSWORD is.'
    }
    return [pscustomobject]@{
        Arguments = @('--p12-file', $env:CODESIGN_P12, '--p12-password-file', $passwordFile)
        Scratch   = @($passwordFile)
        Kind      = 'Developer ID'
    }
}

function Remove-SigningSession {
    <#
    .SYNOPSIS
        Deletes whatever New-SigningSession unsealed or generated.

    .PARAMETER Session
        The object it returned; a null session is accepted.
    #>
    param([object] $Session)
    if ($null -eq $Session) { return }
    foreach ($path in $Session.Scratch) {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force -Confirm:$false }
    }
}

function Invoke-BundleSigning {
    <#
    .SYNOPSIS
        Signs the nested binary, then the bundle, and reads both back.

    .DESCRIPTION
        The hardened runtime is set at every level including a self-signed one.
        Notarising requires it, and an application that breaks under it breaks
        whether the signature is real or not, so it is the self-signed build
        that finds out rather than the first real one.

    .PARAMETER Session
        The signing identity's rcodesign arguments.
    #>
    param([object] $Session)

    Invoke-Rcodesign -Quiet -Arguments (@('sign') + $Session.Arguments + @(
            '--binary-identifier', 'unluminous-cli', '--code-signature-flags', 'runtime',
            (Join-Path $app 'Contents/MacOS/unluminous-cli')))
    Invoke-Rcodesign -Quiet -Arguments (@('sign') + $Session.Arguments + @(
            '--code-signature-flags', 'runtime', $app))

    $info = & $rcodesign print-signature-info $app 2>&1
    if ($LASTEXITCODE -ne 0) { throw 'the signed bundle has no readable signature' }
    if (-not (Test-Path -LiteralPath (Join-Path $app 'Contents/_CodeSignature/CodeResources'))) {
        throw 'the bundle carries no _CodeSignature/CodeResources, so nothing sealed its resources'
    }
    # Two programs, two architectures each, and every one of the four needs the
    # hardened runtime: notarisation refuses a submission without it, and it
    # names the file rather than the flag when it does.
    $runtime = ($info | Select-String -Pattern 'CodeSignatureFlags\(RUNTIME\)').Count
    if ($runtime -lt 4) { throw "only $runtime of the four signatures carry the hardened runtime flag" }

    # `rcodesign verify` reads a Mach-O and refuses a bundle path, so each
    # binary is verified rather than the directory holding them.
    foreach ($program in $programs) {
        Invoke-Rcodesign -Quiet -Arguments @('verify', (Join-Path $app "Contents/MacOS/$($program.Bin)"))
    }
}

function Set-ZipUnixModes {
    <#
    .SYNOPSIS
        Writes a Unix mode onto every entry of a zip, so the bundle still has
        its executable bit when somebody unpacks it on a Mac.

    .DESCRIPTION
        **A zip written on Windows carries no Unix permissions at all.** .NET
        records the host system as MS-DOS and leaves the external attributes
        zero, and `unzip` on macOS reads the mode only when the host byte says
        Unix - so `Contents/MacOS/unluminous` arrives without its executable bit
        and the application does not start. `ditto -c -k` is what avoids that on
        a Mac; this is what avoids it here.

        The patch is over the central directory, whose layout is fixed: each
        record starts `PK`, the high byte of `version made by` at +4 is the
        host system, and the external attributes at +38 hold the mode in their
        top sixteen bits.

    .PARAMETER Path
        The zip to patch.

    .PARAMETER ExecutablePrefix
        Entries under this path get 0755; everything else gets 0644.
    #>
    param([string] $Path, [string] $ExecutablePrefix)

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $signature = [byte[]] @(0x50, 0x4B, 0x01, 0x02)
    for ($at = 0; $at -lt $bytes.Length - 46; $at++) {
        if ($bytes[$at] -ne $signature[0] -or $bytes[$at + 1] -ne $signature[1] `
                -or $bytes[$at + 2] -ne $signature[2] -or $bytes[$at + 3] -ne $signature[3]) {
            continue
        }
        $nameLength = [BitConverter]::ToUInt16($bytes, $at + 28)
        $name = [System.Text.Encoding]::UTF8.GetString($bytes, $at + 46, $nameLength)
        # 33261 is 0100755 and 33188 is 0100644, written in decimal because
        # PowerShell has no octal literal.
        $mode = if ($name.StartsWith($ExecutablePrefix)) { 33261 } else { 33188 }

        # 3 is Unix in the host-system byte; without it the mode below is ignored.
        $bytes[$at + 5] = 3
        # Multiplied rather than shifted: PowerShell's -shl works on a signed
        # 32-bit integer, and 0100755 shifted sixteen places overflows it.
        [Array]::Copy([BitConverter]::GetBytes([uint32] ([int64] $mode * 65536)), 0, $bytes, $at + 38, 4)
    }
    [System.IO.File]::WriteAllBytes($Path, $bytes)
}

function New-BundleZip {
    <#
    .SYNOPSIS
        Zips the bundle into releases/, and returns the path.

    .DESCRIPTION
        Written with .NET's zip writer rather than Compress-Archive because the
        bundle holds a nested directory whose paths matter, and because the
        notary reads the entries by name.

    .PARAMETER Version
        The version in the file's name.
    #>
    param([string] $Version)
    New-Item -ItemType Directory -Force -Path $releases | Out-Null
    $zip = Join-Path $releases "Unluminous-$Version-macos.zip"
    if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force -Confirm:$false }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    # The bundle itself, with its own name as the top entry. The working area it
    # sits in holds every installer the Windows build ever kept, so zipping the
    # parent directory puts 80 unrelated executables in the archive - which is
    # what the first version of this did.
    [System.IO.Compression.ZipFile]::CreateFromDirectory(
        $app, $zip, [System.IO.Compression.CompressionLevel]::Optimal, $true)
    Set-ZipUnixModes -Path $zip -ExecutablePrefix 'Unluminous.app/Contents/MacOS/'
    return $zip
}

function Get-NotaryKeyFile {
    <#
    .SYNOPSIS
        The App Store Connect key file rcodesign notarises with, built from the
        three values installer/macos/notarize.env already defines.

    .DESCRIPTION
        NOTARY_KEY, NOTARY_KEY_ID and NOTARY_ISSUER are what build.sh reads on
        the Mac. rcodesign wants the same three folded into one JSON file, so it
        is written to the RAM disk for the length of the submission rather than
        kept: it carries the private key.
    #>
    foreach ($needed in 'NOTARY_KEY', 'NOTARY_KEY_ID', 'NOTARY_ISSUER') {
        if (-not (Get-Item "env:$needed" -ErrorAction SilentlyContinue)) {
            throw "$needed is not set. installer/macos/notarize.env.example says where each of the three comes from."
        }
    }
    if (-not (Test-Path -LiteralPath $env:NOTARY_KEY)) { throw "NOTARY_KEY points at $($env:NOTARY_KEY), which is not there" }

    $scratch = if (Test-Path -LiteralPath 'R:\') { 'R:\unluminous-release' } else { Join-Path $env:TEMP 'unluminous-release' }
    New-Item -ItemType Directory -Force -Path $scratch | Out-Null
    $file = Join-Path $scratch ('notary-' + [guid]::NewGuid().ToString('N') + '.json')
    Invoke-Rcodesign -Quiet -Arguments @(
        'encode-app-store-connect-api-key', '-o', $file,
        $env:NOTARY_ISSUER, $env:NOTARY_KEY_ID, $env:NOTARY_KEY)
    return $file
}

# ---------------------------------------------------------------------------
# The build itself.
# ---------------------------------------------------------------------------

if (-not (Test-Path -LiteralPath $rcodesign)) { throw 'rcodesign is missing. Run: pwsh tools/cross/fetch-toolchain.ps1' }
if (-not $Version) { $Version = Get-WorkspaceVersion }

# installer/macos/notarize.env holds the identity and the notary credentials on
# a Mac, and the same file is read here. Anything already in the environment
# wins, so a one-off run can still override it.
$envFile = Join-Path $PSScriptRoot 'notarize.env'
if (Test-Path -LiteralPath $envFile) {
    foreach ($line in Get-Content -Path $envFile) {
        if ($line -match '^\s*([A-Z_][A-Z0-9_]*)\s*=\s*(.*?)\s*$') {
            $name = $Matches[1]
            if (Get-Item "env:$name" -ErrorAction SilentlyContinue) { continue }
            Set-Item "env:$name" ($Matches[2].Trim('"').Trim("'"))
        }
    }
}

# Only a build needs the SDK. Assembling, signing and packaging what is already
# built does not, and asking for it there would make -SkipBuild useless on the
# machine the earlier build ran on.
$sdkRoot = if ($CliOnly -or $SkipBuild) { '' } else { Resolve-Sdk }
Write-Host "Unluminous $Version - macOS, built on Windows"
if ($sdkRoot) { Write-Host "  SDK: $sdkRoot" }

if (-not $SkipBuild) {
    foreach ($target in $appleTargets) {
        Write-Host "==> $target"
        Build-AppleTarget -Target $target -SdkRoot $sdkRoot
    }
}

if ($CliOnly) {
    Write-Host ''
    Write-Host 'unluminous-cli built for both Apple architectures. The bundle needs the SDK; see -Sdk.'
    return
}

Write-Host '==> Unluminous.app'
New-AppBundle -Version $Version | Out-Null
Write-Host "  $app"

$session = $null
try {
    $session = New-SigningSession
    Write-Host "==> signing ($($session.Kind))"
    Invoke-BundleSigning -Session $session
} finally {
    Remove-SigningSession -Session $session
}

Write-Host '==> zip'
$zip = New-BundleZip -Version $Version
Write-Host "  $zip"

if ($Notarize) {
    if ($SelfSigned) { throw 'a self-signed certificate cannot be notarised' }
    $keyFile = Get-NotaryKeyFile
    try {
        Write-Host '==> notarising; Apple usually answers in two to fifteen minutes'
        Invoke-Rcodesign -Arguments @('notary-submit', '--api-key-file', $keyFile, '--wait', $zip)
        # The ticket is stapled to the application rather than to the zip,
        # because what a person ends up running is the application they dragged
        # out of it, and a stapled ticket is checked with no network.
        Invoke-Rcodesign -Arguments @('staple', $app)
        Write-Host '==> re-zipping, so the delivered archive holds the stapled bundle'
        $zip = New-BundleZip -Version $Version
    } finally {
        if (Test-Path -LiteralPath $keyFile) { Remove-Item -LiteralPath $keyFile -Force -Confirm:$false }
    }
}

Write-Host ''
Write-Host 'checked here:'
Write-Host '  both architectures in both programs'
Write-Host '  the nested unluminous-cli signed, then the bundle signed round it'
Write-Host '  _CodeSignature/CodeResources written, hardened runtime on all four signatures'
if ($Notarize) {
    Write-Host '  Apple notarised the archive, and the bundle carries a stapled ticket'
} else {
    Write-Host '  NOT notarised, so Gatekeeper will warn about it'
}
Write-Host ''
Write-Host 'not checked here, because a Mach-O only runs on macOS:'
Write-Host '  that the application starts, that spctl accepts it, and that it behaves under the'
Write-Host '  hardened runtime. installer/macos/build.sh runs spctl on a Mac.'
if ($SelfSigned) {
    Write-Host ''
    Write-Warning 'signed with a throwaway certificate. Gatekeeper rejects these; this proves the pipeline, not a release.'
}
