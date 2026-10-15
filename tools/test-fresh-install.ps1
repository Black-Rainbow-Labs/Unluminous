<#
.SYNOPSIS
  Install Unluminous into a fresh environment on this machine, check that it set itself up, uninstall
  it, and put this machine's own installation back exactly as it was. (`task-2278`)

.DESCRIPTION
  There is no Windows Sandbox here (it needs an administrator to switch on and a restart), Docker is not
  running Windows containers, and a second Windows user needs an administrator to make. So the fresh
  environment is made inside this user:

  - Every home folder an installed Unluminous or an agent reads is pointed inside a scratch profile:
    USERPROFILE, HOME, APPDATA, LOCALAPPDATA, CODEX_HOME and CLAUDE_CONFIG_DIR. The real `claude`, `codex`
    and `opencode` are put on its PATH, so the real agents are what gets connected, and each writes its
    settings inside the scratch profile.
  - The installer still writes some of this user's own state, because Inno Setup reads the registry and
    the shell's folders rather than the environment: the uninstall record, the PATH, App Paths, the right
    click menus, the Open With entries, and the Start menu and Desktop shortcuts. Every one of them is
    saved first and put back afterwards, in a `finally`, and then compared with what was saved. A
    difference fails the test.
  - The uninstall record of this machine's installation is taken away while the test runs, so the
    installer sees a machine with no Unluminous on it and takes its defaults, which is what a fresh
    install does. It is put back with the rest.

  What is checked, in order:
    1. The installer finishes, and both programs are in the folder it was told to use.
    2. The PATH names that folder, and `setup.log` was written into the scratch profile.
    3. `claude mcp list`, `codex mcp list` and `opencode mcp list`, run in the scratch profile, each list
       Unluminous and say it connected.
    4. `unluminous-cli setup --check --json` says every agent is already connected and the server answers.
    5. The freshly installed window starts on a scratch project, and Claude Code, with this machine's own
       login and an MCP configuration naming only the fresh install's server, calls a tool and reports
       what the window answered. (Skipped with -NoAgentCall.)
    6. The uninstaller takes Unluminous out of every agent and removes the folder.
    7. This machine's registry keys, PATH, shortcuts, installed files and the agents' own settings are
       exactly what they were.

  Everything it writes goes under _agent_output/fresh-install/<time>/, including summary.json.

.PARAMETER Installer
  The setup program to test. The newest in installer/dist when it is left out.

.PARAMETER NoAgentCall
  Skip step 5, which asks Claude Code a question and so uses a little of its allowance.
#>
param(
    [string] $Installer,
    [switch] $NoAgentCall
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

if (-not $Installer) {
    $Installer = Get-ChildItem (Join-Path $repo 'installer/dist') -Filter 'UnluminousSetup-*.exe' |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if (-not $Installer -or -not (Test-Path $Installer)) { throw 'No installer to test. Build one with installer/windows/build.ps1.' }
$Installer = (Resolve-Path $Installer).Path

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$out = Join-Path $repo "_agent_output/fresh-install/$stamp"
$before = Join-Path $out 'before'
$after = Join-Path $out 'after'
New-Item -ItemType Directory -Force -Path $before, $after | Out-Null

$results = [System.Collections.Generic.List[object]]::new()
function Check([string] $Name, [bool] $Passed, [string] $Detail = '') {
    $results.Add([pscustomobject]@{ check = $Name; passed = $Passed; detail = $Detail })
    $mark = if ($Passed) { 'PASS' } else { 'FAIL' }
    $colour = if ($Passed) { 'Green' } else { 'Red' }
    Write-Host ("{0}  {1}  {2}" -f $mark, $Name, $Detail) -ForegroundColor $colour
}

if (Get-Process -Name 'UnluminousSetup*', 'unins000', '_iu*' -ErrorAction SilentlyContinue) {
    throw 'An Unluminous installer or uninstaller is running. Let it finish first.'
}

# ------------------------------------------------------------------ what the installer touches
$appId = '{64635361-18F5-4632-B119-16D487214CA1}_is1'
$keys = [ordered]@{
    uninstall  = "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\$appId"
    apppaths   = 'HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths\unluminous.exe'
    filemenu   = 'HKCU\Software\Classes\*\shell\Unluminous'
    foldermenu = 'HKCU\Software\Classes\Directory\shell\Unluminous'
    background = 'HKCU\Software\Classes\Directory\Background\shell\Unluminous'
    progid     = 'HKCU\Software\Classes\Unluminous.Document'
    application = 'HKCU\Software\Classes\Applications\unluminous.exe'
}
$extensions = '.md', '.markdown', '.txt', '.rs', '.js', '.ts', '.json', '.toml', '.yml', '.yaml'
$shortcuts = @(
    (Join-Path ([Environment]::GetFolderPath('Programs')) 'Unluminous.lnk'),
    (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Unluminous.lnk')
)
$mainInstall = Join-Path $env:LOCALAPPDATA 'Programs\Unluminous'
$realHome = $env:USERPROFILE

function Test-Key([string] $Key) {
    & reg.exe query $Key *> $null
    return $LASTEXITCODE -eq 0
}

function Export-Key([string] $Key, [string] $File) {
    if (Test-Key $Key) {
        & reg.exe export $Key $File /y *> $null
        return $true
    }
    return $false
}

function Get-OpenWith {
    $values = [ordered]@{}
    foreach ($extension in $extensions) {
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\Classes\$extension\OpenWithProgids")
        $values[$extension] = if ($key) { @($key.GetValueNames()) -contains 'Unluminous.Document' } else { $false }
        if ($key) { $key.Close() }
    }
    return $values
}

function Get-UserPath {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
    $value = $key.GetValue('Path', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $kind = if ($null -ne $value) { $key.GetValueKind('Path').ToString() } else { $null }
    $key.Close()
    return [pscustomobject]@{ value = $value; kind = $kind }
}

function Get-AgentSettings {
    # Only the parts this test could change: Claude Code's `.claude.json` is rewritten by every running
    # Claude Code, so its `unluminous` entry is compared rather than the whole file.
    $claude = Join-Path $realHome '.claude.json'
    $entry = $null
    if (Test-Path $claude) {
        $json = Get-Content $claude -Raw | ConvertFrom-Json -AsHashtable
        if ($json.ContainsKey('mcpServers') -and $json.mcpServers.ContainsKey('unluminous')) {
            $entry = $json.mcpServers.unluminous | ConvertTo-Json -Compress -Depth 8
        }
    }
    $hash = { param($file) if (Test-Path $file) { (Get-FileHash $file).Hash } else { 'absent' } }
    return [pscustomobject]@{
        claude   = $entry
        codex    = & $hash (Join-Path $realHome '.codex\config.toml')
        opencode = & $hash (Join-Path $realHome '.config\opencode\opencode.json')
    }
}

function Get-InstallHashes {
    if (-not (Test-Path $mainInstall)) { return @{} }
    $hashes = [ordered]@{}
    Get-ChildItem $mainInstall -File | ForEach-Object { $hashes[$_.Name] = (Get-FileHash $_.FullName).Hash }
    return $hashes
}

# ------------------------------------------------------------------------------ 1. save it all
Write-Host "Saving this machine's state to $before"
$existed = [ordered]@{}
foreach ($name in $keys.Keys) {
    $existed[$name] = Export-Key $keys[$name] (Join-Path $before "$name.reg")
}
$openWithBefore = Get-OpenWith
$pathBefore = Get-UserPath
$agentsBefore = Get-AgentSettings
$installBefore = Get-InstallHashes
$shortcutsBefore = @($false, $false)
for ($index = 0; $index -lt $shortcuts.Count; $index++) {
    $exists = Test-Path $shortcuts[$index]
    $shortcutsBefore[$index] = $exists
    if ($exists) { Copy-Item $shortcuts[$index] (Join-Path $before "shortcut-$index.lnk") -Force }
}
@{ existed = $existed; openWith = $openWithBefore; path = $pathBefore; agents = $agentsBefore; install = $installBefore; shortcuts = $shortcutsBefore } |
    ConvertTo-Json -Depth 6 | Set-Content (Join-Path $before 'state.json') -Encoding utf8

# The scratch profile.
$profileRoot = Join-Path $out 'profile'
$roaming = Join-Path $profileRoot 'AppData\Roaming'
$local = Join-Path $profileRoot 'AppData\Local'
$target = Join-Path $profileRoot 'Programs\Unluminous'
$project = Join-Path $profileRoot 'project'
New-Item -ItemType Directory -Force -Path $roaming, $local, (Join-Path $profileRoot '.codex'), (Join-Path $profileRoot '.claude'), $project | Out-Null
Set-Content (Join-Path $project 'README.md') "# A project made by test-fresh-install.ps1`n" -Encoding utf8

$agentFolders = @()
foreach ($name in 'claude', 'codex', 'opencode') {
    $found = Get-Command $name -All -ErrorAction SilentlyContinue | Where-Object { $_.Source -match '\.(exe|cmd)$' } | Select-Object -First 1
    if ($found) { $agentFolders += (Split-Path $found.Source) } else { Write-Warning "$name is not installed here, so it is not tested." }
}
$agentsHere = @('claude', 'codex', 'opencode' | Where-Object { Get-Command $_ -ErrorAction SilentlyContinue })
$freshPath = (@("$env:SystemRoot\system32", $env:SystemRoot, "$env:SystemRoot\System32\WindowsPowerShell\v1.0") + ($agentFolders | Select-Object -Unique)) -join ';'

$fresh = @{
    USERPROFILE       = $profileRoot
    HOME              = $profileRoot
    APPDATA           = $roaming
    LOCALAPPDATA      = $local
    CODEX_HOME        = (Join-Path $profileRoot '.codex')
    CLAUDE_CONFIG_DIR = (Join-Path $profileRoot '.claude')
    XDG_CONFIG_HOME   = $null
    OPENCODE_CONFIG   = $null
    ANTHROPIC_API_KEY = $null
    OPENAI_API_KEY    = $null
    PATH              = $freshPath
}

<#
.SYNOPSIS
  Run a block with the scratch profile's environment, and put this process's own back afterwards.
#>
function Invoke-Fresh([scriptblock] $Block) {
    $saved = @{}
    foreach ($name in $fresh.Keys) {
        $saved[$name] = [Environment]::GetEnvironmentVariable($name)
        Set-Variable-Or-Remove $name $fresh[$name]
    }
    try { & $Block } finally {
        foreach ($name in $saved.Keys) { Set-Variable-Or-Remove $name $saved[$name] }
    }
}

<#
.SYNOPSIS
  Set an environment variable, or remove it when the value is $null. PowerShell hands $null to
  SetEnvironmentVariable as an empty string, which leaves the variable set and empty, and Claude Code
  then reads an empty CLAUDE_CONFIG_DIR as a folder and is not logged in.
#>
function Set-Variable-Or-Remove([string] $Name, $Value) {
    if ($null -eq $Value) { Remove-Item "Env:$Name" -ErrorAction SilentlyContinue }
    else { Set-Item "Env:$Name" $Value }
}

<#
.SYNOPSIS
  Wait for a process and every uninstaller copy it starts to end, up to a deadline.
#>
function Wait-Quiet([int] $Seconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        if (-not (Get-Process -Name 'UnluminousSetup*', 'unins000', '_iu*', 'Un_*' -ErrorAction SilentlyContinue)) { return }
        Start-Sleep -Milliseconds 500
    }
}

$window = $null
try {
    # The uninstall record is what makes a second install an upgrade. Take it away so this one is fresh.
    if ($existed.uninstall) { & reg.exe delete $keys.uninstall /f *> $null }

    # -------------------------------------------------------------------------- 2. install
    $log = Join-Path $out 'install.log'
    Write-Host "Installing $Installer into $target"
    $exit = Invoke-Fresh {
        $run = Start-Process -FilePath $Installer -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', '/CURRENTUSER', "/DIR=`"$target`"", "/LOG=`"$log`"") -Wait -PassThru
        $run.ExitCode
    }
    Wait-Quiet 120
    Check 'installer exit code' ($exit -eq 0) "exit $exit"
    $cli = Join-Path $target 'unluminous-cli.exe'
    Check 'both programs installed' ((Test-Path (Join-Path $target 'unluminous.exe')) -and (Test-Path $cli)) $target
    Check 'PATH names the install folder' ((Get-UserPath).value -split ';' -contains $target)
    $setupLog = Join-Path $roaming 'Unluminous\setup.log'
    Check 'setup.log written in the fresh profile' (Test-Path $setupLog) $setupLog
    if (Test-Path $setupLog) { Copy-Item $setupLog (Join-Path $out 'setup.log') }

    # --------------------------------------------------------------- 3. the agents' own view
    foreach ($agent in $agentsHere) {
        $listed = Invoke-Fresh { & $agent mcp list 2>&1 | Out-String }
        Set-Content (Join-Path $out "$agent-mcp-list.txt") $listed -Encoding utf8
        $plain = $listed -replace "`e\[[0-9;]*m", ''
        $line = ($plain -split "`n" | Where-Object { $_ -match 'unluminous' } | Select-Object -First 1)
        $connected = switch ($agent) {
            'claude' { $line -match 'Connected' }
            'codex' { $line -match 'enabled' -and $line -match [regex]::Escape($cli) }
            'opencode' { $line -match 'connected' }
        }
        Check "$agent lists Unluminous" ([bool] $line) ($line ?? '').Trim()
        Check "$agent says it connected" ([bool] $connected)
    }

    # ----------------------------------------------------------------- 4. setup's own view
    $checked = Invoke-Fresh { & $cli setup --check --json 2>&1 | Out-String }
    Set-Content (Join-Path $out 'setup-check.json') $checked -Encoding utf8
    $report = ($checked | ConvertFrom-Json).result
    foreach ($agent in $agentsHere) {
        $step = $report.steps | Where-Object step -eq $agent
        Check "setup --check: $agent already connected" ($step.state -eq 'already') $step.message
    }
    $server = $report.steps | Where-Object step -eq 'server'
    Check 'setup --check: the server answers' ($server.state -eq 'already') $server.message
    $path = $report.steps | Where-Object step -eq 'path'
    Check 'setup --check: unluminous-cli is on the path' ($path.state -eq 'already') $path.message

    # ------------------------------------------- 5. an agent drives the freshly installed window
    if (-not $NoAgentCall -and ($agentsHere -contains 'claude')) {
        $window = Invoke-Fresh { Start-Process -FilePath (Join-Path $target 'unluminous.exe') -ArgumentList @('--background', "`"$project`"") -PassThru }
        $answered = $false
        for ($attempt = 0; $attempt -lt 60 -and -not $answered; $attempt++) {
            Start-Sleep -Milliseconds 500
            $status = Invoke-Fresh { & $cli status --json 2>&1 | Out-String }
            $answered = $LASTEXITCODE -eq 0
        }
        Check 'the fresh window answers' $answered
        $config = @{ mcpServers = @{ unluminous = @{ type = 'stdio'; command = $cli; args = @('mcp', 'serve'); env = @{ APPDATA = $roaming; LOCALAPPDATA = $local; USERPROFILE = $profileRoot } } } }
        $configFile = Join-Path $out 'agent-mcp.json'
        $config | ConvertTo-Json -Depth 6 | Set-Content $configFile -Encoding utf8
        $question = 'Call the Unluminous MCP tool that reports the status of the running Unluminous window. Reply with only the project folder path it reports, nothing else.'
        # The question goes first: `--allowedTools` and `--mcp-config` each take several values, and
        # one at the end swallows the question as another value.
        $said = & claude -p $question --model haiku --allowedTools 'mcp__unluminous__*' --strict-mcp-config --mcp-config $configFile --output-format json 2>&1 | Out-String
        Set-Content (Join-Path $out 'agent-call.json') $said -Encoding utf8
        $text = try { ($said | ConvertFrom-Json).result } catch { $said }
        $leaf = Split-Path $profileRoot -Leaf
        Check 'Claude Code drove the fresh window through its MCP server' ($text -match [regex]::Escape('project') -and $text -match [regex]::Escape($leaf)) ($text ?? '').Trim()
        Invoke-Fresh { & $cli quit 2>&1 | Out-Null }
        if ($window -and -not $window.WaitForExit(15000)) { Stop-Process -Id $window.Id -Force -ErrorAction SilentlyContinue }
        $window = $null
    }

    # ----------------------------------------------------------------------- 6. uninstall
    $uninstaller = Join-Path $target 'unins000.exe'
    Invoke-Fresh { Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') -Wait | Out-Null }
    Wait-Quiet 120
    $deadline = (Get-Date).AddSeconds(60)
    while ((Test-Path $cli) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
    Check 'uninstall removed the programs' (-not (Test-Path $cli))
    Check 'uninstall took the folder off the PATH' (-not ((Get-UserPath).value -split ';' -contains $target))
    foreach ($agent in $agentsHere) {
        $listed = Invoke-Fresh { & $agent mcp list 2>&1 | Out-String }
        Set-Content (Join-Path $out "$agent-mcp-list-after-uninstall.txt") $listed -Encoding utf8
        Check "uninstall took Unluminous out of $agent" (-not ($listed -match 'unluminous'))
    }
}
finally {
    if ($window) { Stop-Process -Id $window.Id -Force -ErrorAction SilentlyContinue }
    # --------------------------------------------------------------------- 7. put it all back
    Write-Host 'Putting this machine back'
    foreach ($name in $keys.Keys) {
        if (Test-Key $keys[$name]) { & reg.exe delete $keys[$name] /f *> $null }
        if ($existed[$name]) { & reg.exe import (Join-Path $before "$name.reg") *> $null }
    }
    foreach ($extension in $extensions) {
        $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey("Software\Classes\$extension\OpenWithProgids")
        $has = @($key.GetValueNames()) -contains 'Unluminous.Document'
        if ($openWithBefore[$extension] -and -not $has) { $key.SetValue('Unluminous.Document', '', [Microsoft.Win32.RegistryValueKind]::String) }
        if (-not $openWithBefore[$extension] -and $has) { $key.DeleteValue('Unluminous.Document') }
        $key.Close()
    }
    $environment = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    if ($null -eq $pathBefore.value) { $environment.DeleteValue('Path', $false) }
    else { $environment.SetValue('Path', $pathBefore.value, [Microsoft.Win32.RegistryValueKind]$pathBefore.kind) }
    $environment.Close()
    for ($index = 0; $index -lt $shortcuts.Count; $index++) {
        if ($shortcutsBefore[$index]) { Copy-Item (Join-Path $before "shortcut-$index.lnk") $shortcuts[$index] -Force }
        elseif (Test-Path $shortcuts[$index]) { Remove-Item $shortcuts[$index] -Force }
    }
}

# --------------------------------------------------------------------------- 8. compare
foreach ($name in $keys.Keys) {
    $exported = Export-Key $keys[$name] (Join-Path $after "$name.reg")
    $same = if ($existed[$name]) {
        $exported -and ((Get-FileHash (Join-Path $before "$name.reg")).Hash -eq (Get-FileHash (Join-Path $after "$name.reg")).Hash)
    } else { -not $exported }
    Check "restored: $($keys[$name])" $same
}
$openWithAfter = Get-OpenWith
Check 'restored: OpenWithProgids' (($openWithAfter | ConvertTo-Json) -eq ($openWithBefore | ConvertTo-Json))
$pathAfter = Get-UserPath
Check 'restored: PATH, value and type' (($pathAfter.value -ceq $pathBefore.value) -and ($pathAfter.kind -eq $pathBefore.kind))
for ($index = 0; $index -lt $shortcuts.Count; $index++) {
    $same = if ($shortcutsBefore[$index]) {
        (Test-Path $shortcuts[$index]) -and ((Get-FileHash $shortcuts[$index]).Hash -eq (Get-FileHash (Join-Path $before "shortcut-$index.lnk")).Hash)
    } else { -not (Test-Path $shortcuts[$index]) }
    Check "restored: $($shortcuts[$index])" $same
}
$installAfter = Get-InstallHashes
Check 'untouched: the installed Unluminous' (($installAfter | ConvertTo-Json) -eq ($installBefore | ConvertTo-Json)) $mainInstall
$agentsAfter = Get-AgentSettings
Check "untouched: this machine's agent settings" (($agentsAfter | ConvertTo-Json) -eq ($agentsBefore | ConvertTo-Json))

$failed = @($results | Where-Object { -not $_.passed })
@{ installer = $Installer; profile = $profileRoot; passed = $failed.Count -eq 0; checks = $results } |
    ConvertTo-Json -Depth 5 | Set-Content (Join-Path $out 'summary.json') -Encoding utf8
Write-Host ''
Write-Host ("{0} of {1} checks passed. Everything is in {2}" -f ($results.Count - $failed.Count), $results.Count, $out)
if ($failed.Count) { exit 1 }
