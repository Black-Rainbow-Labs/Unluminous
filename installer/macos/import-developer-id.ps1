<#
.SYNOPSIS
    Turns the Developer ID identity inillucent holds into the .p12 this
    repository's macOS build signs with, on Windows, with no Mac involved.

.DESCRIPTION
    installer/macos/build-on-windows.ps1 signs with a .p12, because Windows has
    no keychain for CODESIGN_IDENTITY to name, and its error message says to
    export one from a Mac's Keychain Access. On this machine there is no Mac to
    export from. What there is instead is the identity
    C:/jason/dev/inillucent/packaging/macos/new-apple-csr.ps1 obtained: a
    private key sealed with DPAPI, and the certificate Apple issued against it.

    The two halves are the same identity in a different container, and openssl
    converts between them. This script does that conversion once and leaves the
    result where build-on-windows.ps1 will find it.

        %LOCALAPPDATA%\unluminous\apple\developer-id-application.p12
        %LOCALAPPDATA%\unluminous\apple\developer-id-application.p12.pw

    WHY THE .p12 DOES NOT GO IN THE REPOSITORY

    .gitignore here names installer/macos/notarize.env and installer/macos/*.p8,
    and neither pattern matches a .p12. A Developer ID private key committed to
    a public repository is the one mistake in this whole sequence that cannot be
    taken back, so the file is written outside the tree and notarize.env names
    its path.

    WHY openssl IS TOLD WHICH ALGORITHMS TO USE

    openssl 3 writes a PKCS#12 with AES-256-CBC and PBKDF2 by default, and
    rcodesign cannot read that. It fails with `incorrect password given when
    decrypting PFX data`, which names the password and not the cipher, so the
    obvious next move is to retype a password that was never wrong.
    -certpbe, -keypbe and -macalg ask for the older algorithms rcodesign reads.
    Measured here on 2026-09-19: the default form fails, this form signs.

    THE PLAIN TEXT WINDOW

    The unsealed key is a file for the seconds openssl reads it, on the RAM disk
    at R:\, which is memory. It is deleted in a `finally`. The .p12 password is
    random, is never shown, and is sealed with DPAPI beside the .p12, so it is
    worthless on another machine or to another Windows account.

.PARAMETER AppleDir
    The directory holding the sealed key and Apple's certificate. Defaults to
    inillucent's, which is where new-apple-csr.ps1 put them.

.PARAMETER OutputDir
    Where the .p12 and its sealed password are written.

.PARAMETER Force
    Overwrite an existing .p12 instead of refusing.

.EXAMPLE
    pwsh installer\macos\import-developer-id.ps1
#>
[CmdletBinding()]
param(
    [string] $AppleDir,
    [string] $OutputDir,
    [switch] $Force
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

function Get-OpensslPath {
    <#
    .SYNOPSIS
        Finds openssl, or says where to get one.

    .DESCRIPTION
        Git for Windows ships it, so a machine that can clone this repository
        almost always has it already. Naming the two places it lives is quicker
        than a search, and quicker than installing a second copy.
    #>
    $onPath = Get-Command 'openssl.exe' -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    foreach ($candidate in @(
            'C:\Program Files\Git\mingw64\bin\openssl.exe',
            'C:\Program Files\Git\usr\bin\openssl.exe')) {
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }
    throw 'openssl is not on PATH and is not in the usual Git for Windows locations. Git for Windows ships one at mingw64\bin\openssl.exe.'
}

function Get-ScratchDir {
    <#
    .SYNOPSIS
        Where the unsealed private key is written for the seconds openssl reads
        it.

    .DESCRIPTION
        R:\ is a RAM disk on this machine, so nothing about the key survives a
        reboot and nothing lands on a disk that could be read afterwards. A
        machine without one falls back to the temp directory, and says so rather
        than degrading the guarantee silently.
    #>
    $scratch = if (Test-Path -LiteralPath 'R:\') {
        'R:\unluminous-release'
    } else {
        Write-Warning 'no RAM disk at R:\; the unsealed private key will be written to the temp directory instead'
        Join-Path $env:TEMP 'unluminous-release'
    }
    New-Item -ItemType Directory -Force -Path $scratch | Out-Null
    return $scratch
}

function New-P12Password {
    <#
    .SYNOPSIS
        A random password for the .p12.

    .DESCRIPTION
        Nobody types this one. The .p12 is read by a script that is handed the
        password from the sealed file beside it, so a password a person could
        remember would be weaker for no gain.
    #>
    $bytes = [byte[]]::new(24)
    [System.Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
    return [Convert]::ToBase64String($bytes)
}

function Convert-ToP12 {
    <#
    .SYNOPSIS
        Writes the .p12 from the unsealed private key and Apple's certificate.

    .PARAMETER Openssl
        The openssl executable.

    .PARAMETER Unified
        The unsealed file, holding the private key.

    .PARAMETER Certificate
        Apple's certificate, in the DER form the download arrives as.

    .PARAMETER Scratch
        The RAM disk directory the intermediates are written to.

    .PARAMETER Output
        Where the .p12 goes.

    .PARAMETER Password
        The password to encrypt it under.
    #>
    param([string] $Openssl, [string] $Unified, [string] $Certificate, [string] $Scratch, [string] $Output, [string] $Password)

    # The sealed blob is the unified PEM rcodesign wrote: a private key and the
    # throwaway self-signed certificate the signing request was made from. Only
    # the key half is wanted, and `openssl pkey` takes exactly that, so the
    # discarded certificate cannot end up in the .p12 beside Apple's.
    $key = Join-Path $Scratch ('key-' + [guid]::NewGuid().ToString('N') + '.pem')
    $cert = Join-Path $Scratch ('cert-' + [guid]::NewGuid().ToString('N') + '.pem')
    $passwordFile = Join-Path $Scratch ('pw-' + [guid]::NewGuid().ToString('N') + '.txt')
    try {
        & $Openssl pkey -in $Unified -out $key
        if ($LASTEXITCODE -ne 0) { throw "openssl could not read the private key out of the sealed identity ($LASTEXITCODE)" }

        & $Openssl x509 -inform DER -in $Certificate -out $cert
        if ($LASTEXITCODE -ne 0) { throw "openssl could not read $Certificate as a DER certificate ($LASTEXITCODE)" }

        Set-Content -Path $passwordFile -Value $Password -NoNewline
        & $Openssl pkcs12 -export -inkey $key -in $cert -name 'Developer ID Application' `
            -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1 `
            -out $Output -passout "file:$passwordFile"
        if ($LASTEXITCODE -ne 0) { throw "openssl could not write the .p12 ($LASTEXITCODE)" }
    } finally {
        foreach ($path in @($key, $cert, $passwordFile)) {
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force -Confirm:$false }
        }
    }
}

function Assert-P12IsReadable {
    <#
    .SYNOPSIS
        Fails unless rcodesign can open the .p12 and it holds a Developer ID
        Application certificate.

    .DESCRIPTION
        Reading it back is the check, not openssl's exit code: openssl writes a
        PKCS#12 rcodesign cannot parse without complaining about it, and the
        first thing that would notice is a release stopping part way through.
        The profile is checked here too, because the Installer certificate is a
        file of the same shape and signing a bundle with it fails much later,
        with a message from Apple that does not say which certificate was wrong.

    .PARAMETER Rcodesign
        The rcodesign executable.

    .PARAMETER Path
        The .p12.

    .PARAMETER Password
        Its password.

    .PARAMETER Scratch
        Where the password file is written for the seconds rcodesign reads it.
    #>
    param([string] $Rcodesign, [string] $Path, [string] $Password, [string] $Scratch)

    $passwordFile = Join-Path $Scratch ('check-' + [guid]::NewGuid().ToString('N') + '.txt')
    try {
        Set-Content -Path $passwordFile -Value $Password -NoNewline
        $report = & $Rcodesign analyze-certificate --p12-file $Path --p12-password-file $passwordFile 2>&1
        if ($LASTEXITCODE -ne 0) { throw "rcodesign could not read the .p12 that was just written:`n$report" }
        # rcodesign writes many lines, so $report is an array. `-notmatch` on an
        # array filters it rather than answering yes or no, and a filtered array
        # is truthy whenever any line failed to match - which is every time.
        # Joining first is what makes this a question about the whole report.
        if (($report -join "`n") -notmatch 'Guessed Certificate Profile:\s*DeveloperIdApplication') {
            throw "the .p12 does not hold a Developer ID Application certificate. rcodesign read it as:`n$($report -join "`n")"
        }
        return $report
    } finally {
        if (Test-Path -LiteralPath $passwordFile) { Remove-Item -LiteralPath $passwordFile -Force -Confirm:$false }
    }
}

# ---------------------------------------------------------------------------
# The conversion itself.
# ---------------------------------------------------------------------------

if (-not $AppleDir) {
    $AppleDir = if ($env:INILLUCENT_APPLE_DIR) { $env:INILLUCENT_APPLE_DIR } else { Join-Path $env:LOCALAPPDATA 'inillucent\apple' }
}
if (-not $OutputDir) { $OutputDir = Join-Path $env:LOCALAPPDATA 'unluminous\apple' }

$sealedKey = Join-Path $AppleDir 'developer-id-application.key.sealed'
$certificate = Join-Path $AppleDir 'developer-id-application.cer'
if (-not (Test-Path -LiteralPath $sealedKey)) {
    throw "no sealed Developer ID Application key at $sealedKey. Run, in the inillucent checkout: pwsh packaging/macos/new-apple-csr.ps1 -Kind application"
}
if (-not (Test-Path -LiteralPath $certificate)) {
    throw "no certificate at $certificate. It is the .cer downloaded from developer.apple.com, installed with: pwsh packaging/macos/new-apple-csr.ps1 -Kind application -Certificate <the .cer>"
}

$p12 = Join-Path $OutputDir 'developer-id-application.p12'
$sealedPassword = "$p12.pw"
if ((Test-Path -LiteralPath $p12) -and -not $Force) {
    throw "$p12 already exists. Add -Force to replace it."
}

$openssl = Get-OpensslPath
$rcodesign = Join-Path $repo 'tools/cross/bin/rcodesign.exe'
if (-not (Test-Path -LiteralPath $rcodesign)) {
    throw 'rcodesign is missing. Run: pwsh tools/cross/fetch-toolchain.ps1'
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$scratch = Get-ScratchDir
$unified = Join-Path $scratch ('unsealed-' + [guid]::NewGuid().ToString('N') + '.pem')
$password = New-P12Password
try {
    $secure = Get-Content -Path $sealedKey -Raw | ConvertTo-SecureString
    Set-Content -Path $unified -Value ([System.Net.NetworkCredential]::new('', $secure).Password) -NoNewline
    Convert-ToP12 -Openssl $openssl -Unified $unified -Certificate $certificate -Scratch $scratch -Output $p12 -Password $password
} finally {
    if (Test-Path -LiteralPath $unified) { Remove-Item -LiteralPath $unified -Force -Confirm:$false }
}

# A .p12 that failed the check is removed rather than left where it is. It has
# no sealed password beside it, so nothing can use it, and leaving it would make
# the next run stop at "already exists" and name the file that was wrong.
try {
    $report = Assert-P12IsReadable -Rcodesign $rcodesign -Path $p12 -Password $password -Scratch $scratch
} catch {
    if (Test-Path -LiteralPath $p12) { Remove-Item -LiteralPath $p12 -Force -Confirm:$false }
    throw
}

$secure = ConvertTo-SecureString -String $password -AsPlainText -Force
ConvertFrom-SecureString -SecureString $secure | Set-Content -Path $sealedPassword -NoNewline

Write-Host ''
Write-Host "wrote $p12"
Write-Host "     $sealedPassword   (the password, sealed with DPAPI to this Windows account)"
Write-Host ''
$report | Select-String -Pattern 'Subject CN|Team ID|Signed by Apple|Guessed Certificate Profile' | ForEach-Object { "  $_" }
Write-Host ''
Write-Host 'Put these two lines in installer\macos\notarize.env, which git ignores:'
Write-Host ''
Write-Host "  CODESIGN_P12=$p12"
Write-Host "  CODESIGN_P12_PASSWORD_FILE=$sealedPassword"
Write-Host ''
Write-Host 'It also needs NOTARY_KEY, NOTARY_KEY_ID and NOTARY_ISSUER; installer\macos\notarize.env.example says where each comes from.'
