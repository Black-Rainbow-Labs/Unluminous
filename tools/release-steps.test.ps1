<#
.SYNOPSIS
  Verifies the steps in release-steps.ps1 that let a release be started again, with nothing published.

.DESCRIPTION
  task-2312. A release that stopped used to start again from the beginning. These are the pieces that stop
  that: a retry that gives up after its tries, the record of finished steps, reusing an installer only when
  its file is unchanged, a GitHub release that is finished rather than refused when it already exists, and
  telling a held file apart from a compile error in a macOS log. `gh` is a stand-in script that records what
  it was asked, so this needs no network and no credential, and runs in a few seconds.

.EXAMPLE
  pwsh -File tools/release-steps.test.ps1
#>
[CmdletBinding()]
param(
  [string] $FixtureRoot = (Join-Path ([System.IO.Path]::GetTempPath()) 'release-steps-test')
)

$ErrorActionPreference = 'Stop'
$script:failures = 0

<#
.SYNOPSIS Reports one assertion and records a failure so the run can exit non-zero.
.PARAMETER Name - what was being checked
.PARAMETER Condition - whether it held
.PARAMETER Detail - extra context printed on failure
#>
function Assert-That([string] $Name, [bool] $Condition, [string] $Detail = '') {
  if ($Condition) {
    Write-Host ('  PASS  {0}' -f $Name)
  } else {
    Write-Host ('  FAIL  {0}  {1}' -f $Name, $Detail) -ForegroundColor Red
    $script:failures++
  }
}

if (Test-Path -LiteralPath $FixtureRoot) { Remove-Item -LiteralPath $FixtureRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path $FixtureRoot | Out-Null

# What release.ps1 sets before it loads the steps.
$Repo = $FixtureRoot
$next = '9.9.9'
. (Join-Path $PSScriptRoot 'release-steps.ps1')
$RetryWaitSeconds = 0

Write-Host 'Invoke-WithRetry'
$script:calls = 0
Invoke-WithRetry 'a step that works the second time' 3 { $script:calls++; if ($script:calls -lt 2) { $global:LASTEXITCODE = 1 } else { $global:LASTEXITCODE = 0 } }
Assert-That 'a step that fails once is run again and then succeeds' ($script:calls -eq 2) "ran $script:calls times"
$script:calls = 0
$threw = $false
try { Invoke-WithRetry 'a step that always throws' 2 { $script:calls++; throw 'no network' } } catch { $threw = $_.Exception.Message -match 'failed 2 times' }
Assert-That 'a step that always fails throws after its tries' ($threw -and $script:calls -eq 2) "threw=$threw calls=$script:calls"

Write-Host 'The record of finished steps'
$script:Journal = @{ version = $next; source = 'abc'; lock = 'def'; steps = @(); builds = @{} }
Save-Journal
Add-JournalStep 'tagged'
Add-JournalStep 'tagged'
$read = Read-Journal $next
Assert-That 'a step is recorded once' (@($read.steps).Count -eq 1 -and $read.steps[0] -eq 'tagged') ($read.steps -join ',')
$script:Journal = $read
Assert-That 'a recorded step is found again after reading the file' (Test-JournalStep 'tagged')
Assert-That 'a step never recorded is not found' (-not (Test-JournalStep 'private'))
Assert-That 'no record answers null' ($null -eq (Read-Journal '0.0.1'))

Write-Host 'Reusing a build'
$installer = Join-Path $FixtureRoot 'UnluminousSetup-9.9.9-x64.exe'
Set-Content -LiteralPath $installer -Value 'the installer' -Encoding ascii
$script:Journal.builds['windows'] = @{ path = $installer; sha256 = (Get-Sha256 $installer) }
Save-Journal
$script:Journal = Read-Journal $next
Assert-That 'an unchanged installer is reused' ((Get-ReusedBuild 'windows') -eq $installer)
Set-Content -LiteralPath $installer -Value 'a different installer' -Encoding ascii
Assert-That 'a changed installer is built again' ($null -eq (Get-ReusedBuild 'windows'))
Assert-That 'a platform never built is built' ($null -eq (Get-ReusedBuild 'macos'))

Write-Host 'Publish-GitHubRelease'
# A stand-in for gh: `release view` answers from a file of asset names (absent means no release), and every
# call is appended to a log.
$state = Join-Path $FixtureRoot 'gh-assets.txt'
$log = Join-Path $FixtureRoot 'gh-calls.txt'
$fake = Join-Path $FixtureRoot 'gh.ps1'
Set-Content -LiteralPath $fake -Encoding utf8 -Value @"
Add-Content -LiteralPath '$log' -Value (`$args -join ' ')
if (`$args[0] -eq 'release' -and `$args[1] -eq 'view') {
  if (-not (Test-Path -LiteralPath '$state')) { exit 1 }
  Get-Content -LiteralPath '$state'
  exit 0
}
exit 0
"@
$gh = $fake
$assets = @((Join-Path $FixtureRoot 'UnluminousSetup-9.9.9-x64.exe'), (Join-Path $FixtureRoot 'Unluminous-9.9.9-macos.zip'))

Publish-GitHubRelease 'owner/repo' $assets 'notes'
Assert-That 'a release that is not there is created' ((Get-Content -LiteralPath $log -Raw) -match 'release create v9\.9\.9')
Assert-That 'a release is created as a prerelease, a nightly until it is promoted' ((Get-Content -LiteralPath $log -Raw) -match 'release create v9\.9\.9 .*--prerelease')

Remove-Item -LiteralPath $log
Set-Content -LiteralPath $state -Value 'UnluminousSetup-9.9.9-x64.exe'
Publish-GitHubRelease 'owner/repo' $assets 'notes'
$calls = Get-Content -LiteralPath $log -Raw
Assert-That 'a release missing an asset is given that asset' ($calls -match 'release upload v9\.9\.9 .*Unluminous-9\.9\.9-macos\.zip' -and $calls -notmatch 'release create') $calls
Assert-That 'the asset it already has is not uploaded again' ($calls -notmatch 'upload v9\.9\.9 .*UnluminousSetup') $calls

Remove-Item -LiteralPath $log
Set-Content -LiteralPath $state -Value @('UnluminousSetup-9.9.9-x64.exe', 'Unluminous-9.9.9-macos.zip')
Publish-GitHubRelease 'owner/repo' $assets 'notes'
$calls = Get-Content -LiteralPath $log -Raw
Assert-That 'a complete release is left alone' ($calls -notmatch 'upload|create') $calls
Assert-That 'and the step reports success' ($LASTEXITCODE -eq 0)

Write-Host 'Test-TransientMacosFailure'
$macLog = Join-Path $FixtureRoot 'macos.log'
Set-Content -LiteralPath $macLog -Value @('==> aarch64-apple-darwin', 'Error: The process cannot access the file because it is being used by another process. (os error 32)')
Assert-That 'a held file is transient' (Test-TransientMacosFailure $macLog)
Set-Content -LiteralPath $macLog -Value @('error[E0425]: cannot find value `x` in this scope', 'error: could not compile `unluminous-app`')
Assert-That 'a compile error is not' (-not (Test-TransientMacosFailure $macLog))
Set-Content -LiteralPath $macLog -Value 'ok'
Set-Content -LiteralPath "$macLog.err" -Value 'Error: error sending request for url (https://appstoreconnect.apple.com/notary/v2/submissions)'
Assert-That 'a request to the notary service that did not arrive is transient, read from the error log' (Test-TransientMacosFailure $macLog)

Remove-Item -LiteralPath $FixtureRoot -Recurse -Force

if ($script:failures -gt 0) {
  Write-Host "$script:failures check(s) failed." -ForegroundColor Red
  exit 1
}
Write-Host 'Every check passed.' -ForegroundColor Green
