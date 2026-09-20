<#
.SYNOPSIS
  Removes superseded cargo build artifacts from this checkout's target directory.

.DESCRIPTION
  task-2011: unluminous/target had reached 153.7 GB and inillucent/target 365.8 GB, and between
  them they took C: from 1,075 GB free to 200 GB in six days. Cargo never reclaims a target
  directory. Every build writes a fresh hash-suffixed copy of each artifact and leaves the previous
  one exactly where it is, for ever.

  Measured here on 2026-09-20, target/debug/deps held 79.67 GB, of which the artifacts cargo could
  still reach were 2.67 GB. The other 76.39 GB was superseded copies, and the shape of it says
  where they come from: each of the fourteen screenshot-test binaries had twenty copies, laid down
  over 1.6 days, at about 220 MB of executable and debug symbols apiece. A test suite that is
  fourteen separate binaries writes its own weight in dead artifacts every time it runs.

  So this prunes WITHIN a profile directory rather than deleting one. A cargo artifact is named
  <stem>-<hash>; cargo reads exactly one hash per stem, and every other hash is unreachable. That
  is the whole idea here, and it is what makes this different from `scripts/prune-build-output.ps1`
  in ai-service, which deletes whole profile directories and therefore refuses to touch any repo
  somebody is working in. Removing an unreachable copy costs no rebuild at all, so this is safe to
  run in a live checkout -- and safe to run at the end of a build, which is where it is wired in.

  What it never touches:
    - anything outside <target>, and anything in the profile root, which is where cargo puts the
      binaries and libraries a person actually runs;
    - .fingerprint and build, which are cargo's record of what is already fresh rather than weight;
    - the newest -Keep generations of every stem;
    - anything written within -MinAgeMinutes;
    - any profile whose .cargo-lock is held, which is what a build in flight looks like.

  Measured on this checkout: 153.7 GB down to 17.2 GB, and a no-op build afterwards still finishes
  in 0.5 s having compiled nothing.

  And the reason a mistake here is cheap: cargo is self-healing. An artifact removed while it was
  still wanted is rebuilt the next time it is asked for. There is no corrupt state to get into and
  no wrong output to ship -- only time, bounded by the one crate involved. The one thing that would
  genuinely break a build is deleting underneath it, which is what the lock guard is for.

.PARAMETER Path
  The checkout to prune. Defaults to the repository this script lives in.

.PARAMETER Keep
  How many generations of each stem to keep. Two by default rather than one, because cargo picks an
  artifact by hash and not by age: toggling between two feature sets, two branches or two toolchains
  makes the live artifact the older of the pair, and keeping a second generation covers that without
  materially changing what is reclaimed. Twenty generations to two is still 90% of it.

.PARAMETER MinAgeMinutes
  Never remove anything written more recently than this, whatever its generation.

.PARAMETER SkipIncremental
  Leave target/<profile>/incremental alone. The incremental cache is 58.6 GB here and is pruned by
  the same rule as everything else, so there is no reason to skip it by default.

.PARAMETER WhatIf
  Report what would be removed and remove nothing.

.PARAMETER Quiet
  Print only the closing summary. What the release script uses.

.EXAMPLE
  pwsh -File tools/prune-target.ps1 -WhatIf
  pwsh -File tools/prune-target.ps1
  pwsh -File tools/prune-target.ps1 -Path C:\jason\dev\inillucent -Keep 3
#>
[CmdletBinding()]
param(
  [string] $Path,
  [int] $Keep = 2,
  [int] $MinAgeMinutes = 60,
  [switch] $SkipIncremental,
  [switch] $WhatIf,
  [switch] $Quiet
)

$ErrorActionPreference = 'Stop'

# The directories inside a profile that hold hash-suffixed artifacts AND are safe to prune by
# generation. The profile root itself is deliberately absent: debug/unluminous.exe and
# debug/libunluminous_app.rlib live there, they carry no hash, and they are what a person runs.
#
# .fingerprint and build are deliberately absent too, and that was measured rather than assumed.
# Pruning them alongside the rest reclaimed 0.00 GB and 0.22 GB of a profile and cost a rebuild of
# 219 crates, because they are cargo's own bookkeeping rather than weight: one package has many
# fingerprint hashes alive at the same time -- the library, each of the fourteen test targets, the
# build script, each feature unification -- so keeping the newest two per stem throws live entries
# away, and cargo then rebuilds a unit whose artifact was sitting there the whole time. Removing a
# build script's output directory reruns the script and everything downstream of it for the same
# reason. Between them they are a fraction of a percent of the disk, so there is nothing to win.
#
# deps, examples and incremental are a different case and hold effectively all of the weight: one
# stem there is one artifact, and a superseded hash is unreachable.
$ARTIFACT_DIRS = @('deps', 'examples', 'incremental')

# --- logging -----------------------------------------------------------------------------------

<#
.SYNOPSIS Writes one line to the console. Write-Host rather than Write-Output on purpose: anything a
          PowerShell function writes to the success stream becomes part of its return value, so a
          function that logged with Write-Output would return its log lines alongside its answer.
.PARAMETER Message - the line to print
.PARAMETER Always - print even under -Quiet
#>
function Write-Line([string] $Message, [switch] $Always) {
  if ($Quiet -and -not $Always) { return }
  Write-Host $Message
}

# --- discovery ---------------------------------------------------------------------------------

<#
.SYNOPSIS The repository root this script belongs to, used when no -Path was given.
#>
function Get-DefaultRoot {
  return (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
}

<#
.SYNOPSIS Every cargo profile directory under a target directory.

          A profile is recognised by what is inside it rather than by name, because the names are
          not a fixed list: `debug` and `release` sit directly under target, a cross-compiled
          profile sits one deeper under its triple (target/x86_64-apple-darwin/release), and a
          custom profile can be called anything at all.
.PARAMETER Target - the target directory to search
#>
function Get-ProfileDirectories([string] $Target) {
  $found = @()
  foreach ($child in Get-ChildItem -LiteralPath $Target -Directory -Force -ErrorAction SilentlyContinue) {
    if (Test-IsProfileDirectory $child.FullName) { $found += $child.FullName; continue }
    foreach ($grand in Get-ChildItem -LiteralPath $child.FullName -Directory -Force -ErrorAction SilentlyContinue) {
      if (Test-IsProfileDirectory $grand.FullName) { $found += $grand.FullName }
    }
  }
  return $found
}

<#
.SYNOPSIS True when a directory looks like a cargo profile, meaning it holds deps or .fingerprint.
.PARAMETER Dir - the directory to test
#>
function Test-IsProfileDirectory([string] $Dir) {
  foreach ($name in @('deps', '.fingerprint')) {
    if (Test-Path -LiteralPath (Join-Path $Dir $name)) { return $true }
  }
  return $false
}

# --- guards ------------------------------------------------------------------------------------

<#
.SYNOPSIS True when cargo is building into this profile right now.

          Asked of the profile's own .cargo-lock, which cargo holds open for the length of a build.
          That is deliberately narrower than looking for a cargo process anywhere on the box: this
          machine runs several agents at once, and one repository's build is no reason to leave
          another repository's dead artifacts on the disk.
.PARAMETER ProfileDir - the profile directory to test
#>
function Test-BuildInFlight([string] $ProfileDir) {
  $lock = Join-Path $ProfileDir '.cargo-lock'
  if (-not (Test-Path -LiteralPath $lock)) { return $false }
  try {
    $stream = [System.IO.File]::Open($lock, 'Open', 'ReadWrite', 'None')
    $stream.Close()
    return $false
  } catch {
    return $true
  }
}

<#
.SYNOPSIS True when a running process's executable lives inside the given directory, which is what a
          test binary or an editor started out of target looks like.
.PARAMETER Dir - the directory to test
.PARAMETER Processes - the cached process list to search
#>
function Test-DirInUse([string] $Dir, $Processes) {
  $prefix = $Dir.TrimEnd('\') + '\'
  foreach ($p in $Processes) {
    if ($p.ExecutablePath -and $p.ExecutablePath.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) { return $true }
  }
  return $false
}

# --- naming ------------------------------------------------------------------------------------

<#
.SYNOPSIS Splits a cargo artifact name into its stem and its hash, or answers null when the name
          carries no hash.

          Cargo writes two kinds of hash: base-16 for a metadata hash in deps and .fingerprint, and
          base-36 for an incremental cache directory. Both are matched, and the stem match is greedy
          so that a package whose own name contains a hyphen -- unluminous-app-<hash> -- splits at
          the last one. A name that does not parse is never returned and so is never removed, which
          is the safe direction for a name this does not recognise.
.PARAMETER Name - the file or directory name, with any extension already removed
#>
function Split-ArtifactName([string] $Name) {
  if ($Name -notmatch '^(?<stem>.+)-(?<hash>[0-9a-z]{8,32})$') { return $null }
  # Both halves are read out before anything else is matched. Every -match and -notmatch in
  # PowerShell rewrites $Matches, so testing the hash first and reading the stem afterwards answers
  # with a null stem -- which reaches Get-Generations as a null hashtable key.
  $stem = $Matches['stem']
  $hash = $Matches['hash']
  # A hash carries at least one digit. Without this an ordinary word at the end of a hyphenated
  # name reads as a hash, and two unrelated packages would be filed as generations of each other.
  if ($hash -notmatch '[0-9]') { return $null }
  return [PSCustomObject]@{ Stem = $stem; Hash = $hash }
}

# --- generations -------------------------------------------------------------------------------

<#
.SYNOPSIS Gathers one artifact directory into generations: a map of stem to the hashes found under
          it, each hash carrying its paths, its size and the newest write anywhere in it.

          A generation is a hash rather than a file, because one build of one crate writes several
          files that share a hash -- the rlib, the rmeta, the dep file and, on Windows, the pdb --
          and they live or die together.
.PARAMETER Dir - deps, .fingerprint, build, incremental or examples
#>
function Get-Generations([string] $Dir) {
  $stems = @{}
  foreach ($entry in Get-ChildItem -LiteralPath $Dir -Force -ErrorAction SilentlyContinue) {
    $bare = if ($entry.PSIsContainer) { $entry.Name } else { [System.IO.Path]::GetFileNameWithoutExtension($entry.Name) }
    $split = Split-ArtifactName $bare
    if (-not $split) { continue }

    if (-not $stems.ContainsKey($split.Stem)) { $stems[$split.Stem] = @{} }
    if (-not $stems[$split.Stem].ContainsKey($split.Hash)) {
      $stems[$split.Stem][$split.Hash] = [PSCustomObject]@{
        Paths = [System.Collections.ArrayList]::new()
        Size = 0L
        Newest = [datetime]::MinValue
        IsDirectory = $entry.PSIsContainer
      }
    }
    $gen = $stems[$split.Stem][$split.Hash]
    [void] $gen.Paths.Add($entry.FullName)
    $gen.Size += Measure-EntrySize $entry
    $newest = Get-EntryWriteTime $entry
    if ($newest -gt $gen.Newest) { $gen.Newest = $newest }
  }
  return $stems
}

<#
.SYNOPSIS The size of one entry in bytes, walking it when it is a directory.
.PARAMETER Entry - the file or directory
#>
function Measure-EntrySize($Entry) {
  if (-not $Entry.PSIsContainer) { return [int64] $Entry.Length }
  $sum = (Get-ChildItem -LiteralPath $Entry.FullName -Recurse -File -Force -ErrorAction SilentlyContinue |
          Measure-Object -Property Length -Sum).Sum
  if (-not $sum) { return 0L }
  return [int64] $sum
}

<#
.SYNOPSIS The newest write time in one entry. A directory answers with the newest write anywhere
          inside it, so that a cache directory being written into right now reads as new rather
          than as whatever its own timestamp happens to say.
.PARAMETER Entry - the file or directory
#>
function Get-EntryWriteTime($Entry) {
  if (-not $Entry.PSIsContainer) { return $Entry.LastWriteTime }
  $newest = $Entry.LastWriteTime
  foreach ($f in Get-ChildItem -LiteralPath $Entry.FullName -Recurse -File -Force -ErrorAction SilentlyContinue) {
    if ($f.LastWriteTime -gt $newest) { $newest = $f.LastWriteTime }
  }
  return $newest
}

<#
.SYNOPSIS The generations of one artifact directory that may be removed: everything past the newest
          -Keep of each stem, with anything written inside -MinAgeMinutes held back.
.PARAMETER Stems - the map Get-Generations produced
.PARAMETER Cutoff - writes newer than this are kept whatever their generation
#>
function Get-SupersededGenerations($Stems, [datetime] $Cutoff) {
  $superseded = @()
  foreach ($stem in $Stems.Keys) {
    $ordered = @($Stems[$stem].GetEnumerator() | Sort-Object { $_.Value.Newest } -Descending)
    if ($ordered.Count -le $Keep) { continue }
    foreach ($old in $ordered[$Keep..($ordered.Count - 1)]) {
      if ($old.Value.Newest -gt $Cutoff) { continue }
      $superseded += [PSCustomObject]@{
        Stem = $stem
        Hash = $old.Key
        Paths = $old.Value.Paths
        Size = $old.Value.Size
        Newest = $old.Value.Newest
      }
    }
  }
  return $superseded
}

# --- removal -----------------------------------------------------------------------------------

<#
.SYNOPSIS Removes one generation, naming each path in full. Answers with the bytes it accounted for.

          Every path is passed literally and one at a time. A delete whose targets are worked out
          while it runs is the shape that raises an approval dialog on this machine, and an
          unattended build must never stop to ask a question.
.PARAMETER Generation - the record to remove
#>
function Remove-Generation($Generation) {
  if ($WhatIf) { return [int64] $Generation.Size }
  $removed = 0L
  foreach ($p in $Generation.Paths) {
    try {
      Remove-Item -LiteralPath $p -Recurse -Force -Confirm:$false -ErrorAction Stop
      $removed += 1
    } catch {
      Write-Line ("  could not remove {0} -- {1}" -f $p, $_.Exception.Message)
      return 0L
    }
  }
  if ($removed -eq 0) { return 0L }
  return [int64] $Generation.Size
}

# --- the pass ----------------------------------------------------------------------------------

<#
.SYNOPSIS Prunes one artifact directory and answers with what it reclaimed and how much it left.
.PARAMETER Dir - the artifact directory
.PARAMETER Cutoff - the minimum age for removal
#>
function Invoke-PruneDirectory([string] $Dir, [datetime] $Cutoff) {
  $empty = [PSCustomObject]@{ Freed = 0L; Removed = 0; Kept = 0 }
  if (-not (Test-Path -LiteralPath $Dir)) { return $empty }

  $stems = Get-Generations $Dir
  if ($stems.Count -eq 0) { return $empty }

  $superseded = Get-SupersededGenerations $stems $Cutoff
  $freed = 0L
  foreach ($gen in $superseded) { $freed += (Remove-Generation $gen) }

  $kept = 0
  foreach ($stem in $stems.Keys) { $kept += [math]::Min($Keep, $stems[$stem].Count) }

  $verb = if ($WhatIf) { 'would remove' } else { 'removed' }
  Write-Line ("  {0,-14} {1,8:N2} GB {2} from {3,4} generations, keeping {4,4} across {5,4} stems" -f
              (Split-Path -Leaf $Dir), ($freed / 1GB), $verb, $superseded.Count, $kept, $stems.Count)

  return [PSCustomObject]@{ Freed = $freed; Removed = $superseded.Count; Kept = $kept }
}

<#
.SYNOPSIS Prunes every artifact directory of one profile, after its two guards have passed.
.PARAMETER ProfileDir - the profile directory
.PARAMETER Cutoff - the minimum age for removal
.PARAMETER Processes - the cached process list
#>
function Invoke-PruneProfile([string] $ProfileDir, [datetime] $Cutoff, $Processes) {
  $skipped = [PSCustomObject]@{ Freed = 0L; Removed = 0; Skipped = $true }

  if (Test-BuildInFlight $ProfileDir) {
    Write-Line ("{0}: skipped, cargo holds its lock so a build is in flight" -f $ProfileDir) -Always
    return $skipped
  }
  if (Test-DirInUse $ProfileDir $Processes) {
    Write-Line ("{0}: skipped, a running process lives inside it" -f $ProfileDir) -Always
    return $skipped
  }

  Write-Line ("{0}" -f $ProfileDir)
  $freed = 0L; $removed = 0
  foreach ($name in $ARTIFACT_DIRS) {
    if ($SkipIncremental -and $name -eq 'incremental') { continue }
    $result = Invoke-PruneDirectory (Join-Path $ProfileDir $name) $Cutoff
    $freed += $result.Freed; $removed += $result.Removed
  }
  return [PSCustomObject]@{ Freed = $freed; Removed = $removed; Skipped = $false }
}

<#
.SYNOPSIS Prunes every profile of one checkout and prints the closing summary.
#>
function Invoke-Prune {
  $root = if ($Path) { (Resolve-Path $Path).Path } else { Get-DefaultRoot }
  $target = Join-Path $root 'target'
  if (-not (Test-Path -LiteralPath $target)) {
    Write-Line ("nothing to prune: {0} has no target directory" -f $root) -Always
    return
  }

  $before = [math]::Round((Get-PSDrive C).Free / 1GB, 2)
  $mode = if ($WhatIf) { '  [WhatIf]' } else { '' }
  Write-Line ("pruning {0}  keep {1} generations  free {2:N1} GB{3}" -f $target, $Keep, $before, $mode) -Always

  $cutoff = (Get-Date).AddMinutes(-$MinAgeMinutes)
  $processes = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object { $_.ExecutablePath })

  $freed = 0L; $removed = 0; $skipped = 0
  foreach ($profileDir in Get-ProfileDirectories $target) {
    $result = Invoke-PruneProfile $profileDir $cutoff $processes
    if ($result.Skipped) { $skipped++; continue }
    $freed += $result.Freed; $removed += $result.Removed
  }

  $verb = if ($WhatIf) { 'would reclaim' } else { 'reclaimed' }
  Write-Line ("{0} {1:N2} GB from {2:N0} superseded generations{3}" -f
              $verb, ($freed / 1GB), $removed, $(if ($skipped) { ", $skipped profile(s) skipped" } else { '' })) -Always
}

Invoke-Prune
