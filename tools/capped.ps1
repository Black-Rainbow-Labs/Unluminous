<#
.SYNOPSIS
  Runs one command on 80% of the machine's logical processors, with CC and CXX out of its environment.

.DESCRIPTION
  `task-2240`. `tools/release.ps1` and `tools/test.ps1` confine themselves to a share of the
  processors. A cargo command typed by hand does not, and a cold `cargo test` or `cargo build
  --release` takes every processor the machine has. Put this in front of it:

      pwsh tools/capped.ps1 cargo test -p unluminous-app --test '*' --no-fail-fast
      pwsh tools/capped.ps1 cargo build --release -p unluminous-app

  `Enter-ProcessorShare` in `tools/processor-share.ps1` sets this process's affinity mask, and the
  command and everything it starts inherit it. CC and CXX are removed for the reason
  `tools/release.ps1` gives at `Invoke-WithoutCc`: a user `CC` naming `cl.exe` with no `INCLUDE`
  beside it stops `libsqlite3-sys` from compiling.

  UNLUMINOUS_CPU_PERCENT changes the share; 100 runs the command on every processor. The exit code is
  the command's. There is no param block on purpose: one would make this an advanced script, and
  PowerShell would then take an argument such as `-Verbose` meant for the command as one of its own.
#>
$ErrorActionPreference = 'Stop'
if ($args.Count -eq 0) { throw 'usage: pwsh tools/capped.ps1 <program> [arguments...]' }
. (Join-Path $PSScriptRoot 'processor-share.ps1')
Enter-ProcessorShare
Remove-Item env:CC -ErrorAction SilentlyContinue
Remove-Item env:CXX -ErrorAction SilentlyContinue
$program = $args[0]
$rest = @($args | Select-Object -Skip 1)
& $program @rest
exit $LASTEXITCODE
