<#
.SYNOPSIS
  Verifies prune-target.ps1 against a throwaway fixture: that it removes superseded cargo
  artifacts, and that everything cargo still needs survives.

.DESCRIPTION
  task-2011. The whole value of this script is that it can be run at the end of a build without
  costing the next build anything, so the things it must NOT touch are tested at least as carefully
  as the things it must. The fixture is a synthetic target directory, so none of this needs cargo,
  a toolchain or a graphics card, and it runs in about a second.

  The fixture lives under the scratch directory this script creates and removes.

.EXAMPLE
  pwsh -File tools/prune-target.test.ps1
#>
[CmdletBinding()]
param(
  [string] $FixtureRoot = (Join-Path ([System.IO.Path]::GetTempPath()) 'prune-target-test')
)

$ErrorActionPreference = 'Stop'
$script:failures = 0
$SCRIPT_UNDER_TEST = Join-Path $PSScriptRoot 'prune-target.ps1'

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

<#
.SYNOPSIS Writes one generation of one artifact: every file that shares a hash, aged together.
.PARAMETER Dir - the artifact directory to write into
.PARAMETER Stem - the artifact stem, as cargo writes it
.PARAMETER Hash - the metadata hash
.PARAMETER AgeHours - how long ago this generation was written
.PARAMETER Extensions - the files cargo would have produced for it
.PARAMETER SizeKB - how large to make each file
#>
function New-Generation([string] $Dir, [string] $Stem, [string] $Hash, [double] $AgeHours,
                        [string[]] $Extensions = @('.rlib', '.pdb'), [int] $SizeKB = 4) {
  New-Item -ItemType Directory -Path $Dir -Force | Out-Null
  foreach ($ext in $Extensions) {
    $path = Join-Path $Dir ('{0}-{1}{2}' -f $Stem, $Hash, $ext)
    Set-Content -LiteralPath $path -Value ('x' * ($SizeKB * 1024)) -Encoding ascii
    (Get-Item -LiteralPath $path).LastWriteTime = (Get-Date).AddHours(-$AgeHours)
  }
}

<#
.SYNOPSIS Writes one directory-shaped generation, which is what .fingerprint, build and incremental
          hold.
.PARAMETER Dir - the artifact directory to write into
.PARAMETER Name - the full directory name, hash included
.PARAMETER AgeHours - how long ago it was written
#>
function New-GenerationDir([string] $Dir, [string] $Name, [double] $AgeHours) {
  $path = Join-Path $Dir $Name
  New-Item -ItemType Directory -Path $path -Force | Out-Null
  $inner = Join-Path $path 'payload.bin'
  Set-Content -LiteralPath $inner -Value ('x' * 4096) -Encoding ascii
  (Get-Item -LiteralPath $inner).LastWriteTime = (Get-Date).AddHours(-$AgeHours)
  (Get-Item -LiteralPath $path).LastWriteTime = (Get-Date).AddHours(-$AgeHours)
}

<#
.SYNOPSIS Builds a synthetic checkout whose target directory holds every case the pruner has to
          decide about, and answers with the paths the assertions name.
#>
function New-Fixture {
  if (Test-Path -LiteralPath $FixtureRoot) { Remove-Item -LiteralPath $FixtureRoot -Recurse -Force }
  $root = Join-Path $FixtureRoot 'repo'
  $profileDir = Join-Path (Join-Path $root 'target') 'debug'
  $deps = Join-Path $profileDir 'deps'
  $examples = Join-Path $profileDir 'examples'

  # Four generations of one stem, oldest to newest. With -Keep 2 the two oldest must go.
  New-Generation $deps 'libthing' 'aaaa1111aaaa1111' 96
  New-Generation $deps 'libthing' 'bbbb2222bbbb2222' 72
  New-Generation $deps 'libthing' 'cccc3333cccc3333' 48
  New-Generation $deps 'libthing' 'dddd4444dddd4444' 24

  # A superseded generation written just now, which -MinAgeMinutes must hold back.
  New-Generation $deps 'libfresh' '1111aaaa1111aaaa' 100
  New-Generation $deps 'libfresh' '2222bbbb2222bbbb' 100
  New-Generation $deps 'libfresh' '3333cccc3333cccc' 0

  # Names the pruner must not parse as an artifact at all.
  Set-Content -LiteralPath (Join-Path $deps 'libplain.rlib') -Value 'x' -Encoding ascii
  Set-Content -LiteralPath (Join-Path $deps 'serde-derive.rlib') -Value 'x' -Encoding ascii

  New-Generation $examples 'frame_cost' 'eeee5555eeee5555' 96 @('.exe')
  New-Generation $examples 'frame_cost' '7777dddd7777dddd' 48 @('.exe')
  New-Generation $examples 'frame_cost' 'ffff6666ffff6666' 24 @('.exe')

  # Cargo's own bookkeeping, which is never pruned however old it looks.
  New-GenerationDir (Join-Path $profileDir '.fingerprint') 'thing-aaaa1111aaaa1111' 500
  New-GenerationDir (Join-Path $profileDir '.fingerprint') 'thing-bbbb2222bbbb2222' 400
  New-GenerationDir (Join-Path $profileDir '.fingerprint') 'thing-cccc3333cccc3333' 300
  New-GenerationDir (Join-Path $profileDir 'build') 'thing-aaaa1111aaaa1111' 500
  New-GenerationDir (Join-Path $profileDir 'build') 'thing-bbbb2222bbbb2222' 400
  New-GenerationDir (Join-Path $profileDir 'build') 'thing-cccc3333cccc3333' 300

  # Incremental caches, which are pruned by the same rule as deps. Cargo names these base-36.
  $incremental = Join-Path $profileDir 'incremental'
  New-GenerationDir $incremental 'agent_board-08xdx3lmitle8' 96
  New-GenerationDir $incremental 'agent_board-0a509f5wlzgmh' 72
  New-GenerationDir $incremental 'agent_board-0az156j1vpeay' 48

  # The profile root, where the binaries a person runs live.
  Set-Content -LiteralPath (Join-Path $profileDir 'unluminous.exe') -Value 'x' -Encoding ascii
  Set-Content -LiteralPath (Join-Path $profileDir 'libunluminous_app.rlib') -Value 'x' -Encoding ascii
  Set-Content -LiteralPath (Join-Path $profileDir '.cargo-lock') -Value '' -Encoding ascii

  # A cross-compiled profile, one level deeper under its triple.
  $cross = Join-Path (Join-Path (Join-Path $root 'target') 'x86_64-apple-darwin') 'release'
  New-Generation (Join-Path $cross 'deps') 'libcross' 'aaaa1111aaaa1111' 96
  New-Generation (Join-Path $cross 'deps') 'libcross' 'bbbb2222bbbb2222' 72
  New-Generation (Join-Path $cross 'deps') 'libcross' 'cccc3333cccc3333' 48

  return [PSCustomObject]@{
    Root = $root
    Profile = $profileDir
    Deps = $deps
    Examples = $examples
    Incremental = $incremental
    Fingerprint = Join-Path $profileDir '.fingerprint'
    Build = Join-Path $profileDir 'build'
    CrossDeps = Join-Path $cross 'deps'
    Lock = Join-Path $profileDir '.cargo-lock'
  }
}

<#
.SYNOPSIS Runs the script under test against the fixture and answers with everything it printed.
.PARAMETER Root - the fixture checkout
.PARAMETER Extra - further arguments to pass
#>
function Invoke-Pruner([string] $Root, [string[]] $Extra = @()) {
  $args = @('-NoProfile', '-File', $SCRIPT_UNDER_TEST, '-Path', $Root) + $Extra
  return (& pwsh @args 2>&1 | Out-String)
}

<#
.SYNOPSIS True when a generation's files are all gone.
.PARAMETER Dir - the artifact directory
.PARAMETER Stem - the artifact stem
.PARAMETER Hash - the generation's hash
#>
function Test-GenerationGone([string] $Dir, [string] $Stem, [string] $Hash) {
  return @(Get-ChildItem -LiteralPath $Dir -Filter ('{0}-{1}.*' -f $Stem, $Hash) -ErrorAction SilentlyContinue).Count -eq 0
}

# --- a dry run changes nothing -----------------------------------------------------------------

Write-Host 'a dry run reports and removes nothing'
$f = New-Fixture
$log = Invoke-Pruner $f.Root @('-WhatIf')
Assert-That 'the oldest generation survives a dry run' (-not (Test-GenerationGone $f.Deps 'libthing' 'aaaa1111aaaa1111'))
Assert-That 'a dry run says what it would reclaim' ($log -match 'would reclaim') $log

# --- the generation rule -----------------------------------------------------------------------

Write-Host 'superseded generations go and the newest -Keep stay'
$log = Invoke-Pruner $f.Root
Assert-That 'the oldest generation is removed' (Test-GenerationGone $f.Deps 'libthing' 'aaaa1111aaaa1111')
Assert-That 'the second oldest generation is removed' (Test-GenerationGone $f.Deps 'libthing' 'bbbb2222bbbb2222')
Assert-That 'the newest generation is kept' (-not (Test-GenerationGone $f.Deps 'libthing' 'dddd4444dddd4444'))
Assert-That 'the second newest generation is kept' (-not (Test-GenerationGone $f.Deps 'libthing' 'cccc3333cccc3333'))
Assert-That 'every file sharing a removed hash goes with it' `
  (-not (Test-Path -LiteralPath (Join-Path $f.Deps 'libthing-aaaa1111aaaa1111.pdb')))

# --- what must never be touched ------------------------------------------------------------------

Write-Host 'cargo bookkeeping and the profile root are never touched'
Assert-That 'an ancient .fingerprint generation survives' `
  (Test-Path -LiteralPath (Join-Path $f.Fingerprint 'thing-aaaa1111aaaa1111'))
Assert-That 'every .fingerprint generation survives' `
  (@(Get-ChildItem -LiteralPath $f.Fingerprint).Count -eq 3)
Assert-That 'every build script directory survives' `
  (@(Get-ChildItem -LiteralPath $f.Build).Count -eq 3)
Assert-That 'the binary in the profile root survives' `
  (Test-Path -LiteralPath (Join-Path $f.Profile 'unluminous.exe'))
Assert-That 'the library in the profile root survives' `
  (Test-Path -LiteralPath (Join-Path $f.Profile 'libunluminous_app.rlib'))

Write-Host 'a name carrying no hash is never an artifact'
Assert-That 'a plain name survives' (Test-Path -LiteralPath (Join-Path $f.Deps 'libplain.rlib'))
Assert-That 'a hyphenated word is not read as a hash' (Test-Path -LiteralPath (Join-Path $f.Deps 'serde-derive.rlib'))

Write-Host 'a superseded generation written just now is held back'
Assert-That 'the recent generation survives' (-not (Test-GenerationGone $f.Deps 'libfresh' '3333cccc3333cccc'))

# --- the other artifact directories ---------------------------------------------------------------

Write-Host 'examples and incremental follow the same rule'
Assert-That 'a superseded example is removed' (Test-GenerationGone $f.Examples 'frame_cost' 'eeee5555eeee5555')
Assert-That 'the newest example is kept' (-not (Test-GenerationGone $f.Examples 'frame_cost' 'ffff6666ffff6666'))
Assert-That 'the oldest base-36 incremental cache is removed' `
  (-not (Test-Path -LiteralPath (Join-Path $f.Incremental 'agent_board-08xdx3lmitle8')))
Assert-That 'the newest incremental cache is kept' `
  (Test-Path -LiteralPath (Join-Path $f.Incremental 'agent_board-0az156j1vpeay'))

Write-Host 'a cross-compiled profile under its triple is found'
Assert-That 'the superseded cross artifact is removed' (Test-GenerationGone $f.CrossDeps 'libcross' 'aaaa1111aaaa1111')
Assert-That 'the newest cross artifact is kept' (-not (Test-GenerationGone $f.CrossDeps 'libcross' 'cccc3333cccc3333'))

# --- the build-in-flight guard ---------------------------------------------------------------------

Write-Host 'a profile whose lock is held is skipped whole'
$f = New-Fixture
$held = [System.IO.File]::Open($f.Lock, 'Open', 'ReadWrite', 'None')
try {
  $log = Invoke-Pruner $f.Root
} finally {
  $held.Close()
}
Assert-That 'nothing was removed while the lock was held' (-not (Test-GenerationGone $f.Deps 'libthing' 'aaaa1111aaaa1111'))
Assert-That 'the skip was reported' ($log -match 'build is in flight') $log
Assert-That 'a profile with no lock held is still pruned' (Test-GenerationGone $f.CrossDeps 'libcross' 'aaaa1111aaaa1111')

# --- -Keep is honoured -------------------------------------------------------------------------

Write-Host '-Keep 1 keeps exactly one generation'
$f = New-Fixture
[void] (Invoke-Pruner $f.Root @('-Keep', '1'))
Assert-That 'the second newest is removed at -Keep 1' (Test-GenerationGone $f.Deps 'libthing' 'cccc3333cccc3333')
Assert-That 'the newest is still kept at -Keep 1' (-not (Test-GenerationGone $f.Deps 'libthing' 'dddd4444dddd4444'))

Write-Host '-SkipIncremental leaves the incremental cache alone'
$f = New-Fixture
[void] (Invoke-Pruner $f.Root @('-SkipIncremental'))
Assert-That 'the oldest incremental cache survives' `
  (Test-Path -LiteralPath (Join-Path $f.Incremental 'agent_board-08xdx3lmitle8'))

# --- the name reader ---------------------------------------------------------------------------

Write-Host 'Split-ArtifactName reads a stem and a hash'
. $SCRIPT_UNDER_TEST -Path $FixtureRoot | Out-Null
$split = Split-ArtifactName 'libunluminous_app-1a2b3c4d5e6f7890'
Assert-That 'the stem is read' ($split -and $split.Stem -eq 'libunluminous_app') ('got ' + ($split | Out-String))
Assert-That 'the hash is read' ($split -and $split.Hash -eq '1a2b3c4d5e6f7890') ('got ' + ($split | Out-String))
$hyphenated = Split-ArtifactName 'unluminous-app-1a2b3c4d5e6f7890'
Assert-That 'a package whose name has a hyphen splits at the last one' `
  ($hyphenated -and $hyphenated.Stem -eq 'unluminous-app') ('got ' + ($hyphenated | Out-String))
Assert-That 'a name with no hash reads as no artifact' ($null -eq (Split-ArtifactName 'libplain'))
Assert-That 'a hash with no digit reads as no artifact' ($null -eq (Split-ArtifactName 'serde-derive'))
$base36 = Split-ArtifactName 'agent_board-08xdx3lmitle8'
Assert-That 'a base-36 incremental hash is read' `
  ($base36 -and $base36.Hash -eq '08xdx3lmitle8') ('got ' + ($base36 | Out-String))

# --- cleanup -----------------------------------------------------------------------------------

if (Test-Path -LiteralPath $FixtureRoot) { Remove-Item -LiteralPath $FixtureRoot -Recurse -Force }

if ($script:failures -eq 0) {
  Write-Host ''
  Write-Host 'all assertions passed' -ForegroundColor Green
  exit 0
}
Write-Host ''
Write-Host ('{0} assertion(s) failed' -f $script:failures) -ForegroundColor Red
exit 1
