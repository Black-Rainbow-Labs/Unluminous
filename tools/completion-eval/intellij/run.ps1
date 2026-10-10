# Runs IntelliJ IDEA basic completion, headless, at every query of positions.json and writes
# D:/unluminous-completion-eval/runs/<Run>/results.jsonl and run.json (the engine contract in ../README.md).
#
#   pwsh tools/completion-eval/intellij/run.ps1 -Run first -Ml on -Split held
#
# Each corpus gets its own IDE process with a fresh config and system folder, so the completion
# statistics start empty and the person's own IntelliJ is never touched.
param(
    [Parameter(Mandatory)][string]$Run,
    [ValidateSet('on', 'off')][string]$Ml = 'on',
    [ValidateSet('tune', 'held', 'all')][string]$Split = 'all',
    [string]$Corpus = '',
    [string]$Positions = '',
    [int]$Limit = 0,
    [int]$TimeoutMinutes = 240,
    [switch]$Rebuild,
    [string]$Ide = 'C:/Program Files/JetBrains/IntelliJ IDEA 2025.1',
    [string]$Base = 'D:/unluminous-completion-eval'
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
if (-not $Positions) { $Positions = (Resolve-Path "$here/../positions.json").Path }
$Positions = (Resolve-Path $Positions).Path -replace '\\', '/'
$runDir = "$Base/runs/$Run"
$userConfig = "$env:APPDATA/JetBrains/IntelliJIdea2025.3"

<#
.SYNOPSIS Builds the plugin when its jar is missing or a rebuild was asked for.
#>
function Ensure-PluginBuilt {
    $jar = "$Base/build/plugin/completion-eval/lib/completion-eval.jar"
    if ($Rebuild -or -not (Test-Path $jar)) { & "$here/build.ps1" -Ide $Ide -Out "$Base/build" }
}

<#
.SYNOPSIS Copies a pristine corpus into the run folder, recreating top level directory junctions instead of copying through them.
.PARAMETER Name corpus name
#>
function Copy-Corpus([string]$Name) {
    $src = "$Base/corpora/$Name"
    $dst = "$runDir/corpora/$Name"
    if (Test-Path $dst) { return $dst }
    if (-not (Test-Path $src)) { throw "pristine corpus missing: $src" }
    New-Item -ItemType Directory -Force $dst | Out-Null
    robocopy $src $dst /E /XJ /XD target /R:0 /W:0 /NFL /NDL /NJH /NJS /NP | Out-Null
    foreach ($child in Get-ChildItem $src -Force -Directory) {
        if ($child.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            New-Item -ItemType Junction -Path "$dst/$($child.Name)" -Target $child.Target | Out-Null
        }
    }
    return $dst
}

<#
.SYNOPSIS Writes the fresh config, system and plugins folders plus the properties and vmoptions files for one run.
.PARAMETER Tag folder name for this IDE process
#>
function New-IdeEnvironment([string]$Tag) {
    $root = "$runDir/ide-$Tag"
    foreach ($d in 'idea-config', 'idea-system', 'idea-plugins', 'idea-log') { New-Item -ItemType Directory -Force "$root/$d" | Out-Null }
    robocopy "$Base/build/plugin/completion-eval" "$root/idea-plugins/completion-eval" /E /NFL /NDL /NJH /NJS /NP | Out-Null
    robocopy "$userConfig/plugins/intellij-rust" "$root/idea-plugins/intellij-rust" /E /NFL /NDL /NJH /NJS /NP | Out-Null
    Copy-Item "$userConfig/idea.key" "$root/idea-config/idea.key" -Force -ErrorAction SilentlyContinue
    if ($Ml -eq 'off') { Set-Content "$root/idea-config/disabled_plugins.txt" 'org.jetbrains.completion.full.line' }
    Write-IdeProperties $root
    return $root
}

<#
.SYNOPSIS Writes idea.properties and the vmoptions file that IDEA_PROPERTIES and IDEA_VM_OPTIONS point at.
.PARAMETER Root the IDE process folder
#>
function Write-IdeProperties([string]$Root) {
    $props = @(
        "idea.config.path=$Root/idea-config", "idea.system.path=$Root/idea-system",
        "idea.plugins.path=$Root/idea-plugins", "idea.log.path=$Root/idea-log",
        'idea.trust.all.projects=true', 'idea.suppress.statistics.report=true', 'idea.initially.ask.config=never',
        'jb.privacy.policy.text=<!--999.999-->', 'jb.consents.confirmation.enabled=false', 'ide.show.tips.on.startup.default.value=false',
        'idea.fatal.error.notification=disabled', 'ide.no.platform.update=true', 'idea.is.internal=false'
    )
    Set-Content "$Root/idea.properties" $props
    $vm = Get-Content "$Ide/bin/idea64.exe.vmoptions" | Where-Object { $_ -notmatch '^-Xmx|^-Xms' }
    $vm += '-Xms512m', '-Xmx6g', '-Djava.awt.headless=true'
    if ($env:COMPLETION_EVAL_TRACE) { $vm += '-DcompletionEval.trace=true' }
    Set-Content "$Root/idea64.exe.vmoptions" $vm
}

<#
.SYNOPSIS Lists the corpus names and languages of positions.json that this run covers.
#>
function Get-CorpusList {
    $doc = Get-Content $Positions -Raw | ConvertFrom-Json
    $names = $doc.positions | Where-Object { $Split -eq 'all' -or $_.split -eq $Split } | ForEach-Object corpus | Sort-Object -Unique
    if ($Corpus) { $names = $names | Where-Object { $_ -eq $Corpus } }
    return $names | ForEach-Object { [pscustomobject]@{ name = $_; language = $doc.corpora.$_.language } } | Where-Object { $_.language -in "rust", "typescript" }
}

<#
.SYNOPSIS Starts one IDE process on a job file, waits for it, and makes sure nothing it started is left running.
.PARAMETER Root the IDE process folder
.PARAMETER JobFile job description path
#>
function Start-IdeProcess([string]$Root, [string]$JobFile) {
    $env:IDEA_PROPERTIES = "$Root/idea.properties"
    $env:IDEA_VM_OPTIONS = "$Root/idea64.exe.vmoptions"
    $launcher = Start-Process -FilePath "$Ide/bin/idea64.exe" -ArgumentList 'completionEval', "`"$JobFile`"" -PassThru -WindowStyle Hidden
    $deadline = (Get-Date).AddMinutes($TimeoutMinutes)
    $marker = ($Root -replace '/', '\\')
    do {
        Start-Sleep -Seconds 5
        $alive = @(Get-OwnedProcesses $launcher.Id $Root)
    } while ($alive.Count -gt 0 -and (Get-Date) -lt $deadline)
    foreach ($p in Get-OwnedProcesses $launcher.Id $Root) {
        Write-Warning "stopping leftover process $($p.ProcessId)"
        Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
    }
}

<#
.SYNOPSIS Returns the live processes this script started: the launcher and any process whose command line names this run's IDE folder.
.PARAMETER LauncherId pid returned by Start-Process
.PARAMETER Root the IDE process folder
#>
function Get-OwnedProcesses([int]$LauncherId, [string]$Root) {
    $needle = ($Root -replace '\\', '/')
    Get-CimInstance Win32_Process | Where-Object {
        $_.ProcessId -eq $LauncherId -or ($_.CommandLine -and ($_.CommandLine -replace '\\', '/').Contains($needle))
    } | Where-Object { $_.ProcessId -ne $PID }
}

<#
.SYNOPSIS Runs every selected corpus and writes run.json.
#>
function Invoke-Run {
    New-Item -ItemType Directory -Force $runDir | Out-Null
    Ensure-PluginBuilt
    $started = (Get-Date).ToString('o')
    $imports = @()
    foreach ($c in Get-CorpusList) {
        $copy = Copy-Corpus $c.name
        $root = New-IdeEnvironment $c.name
        $job = [ordered]@{ outDir = $runDir; corpus = $c.name; corpusDir = $copy; language = $c.language; ml = ($Ml -eq 'on'); split = $Split; limit = $Limit; positions = $Positions }
        $jobFile = "$runDir/job-$($c.name).json"
        $job | ConvertTo-Json | Set-Content $jobFile
        Write-Host "== $($c.name) ($($c.language)) =="
        Start-IdeProcess $root $jobFile
        $report = "$runDir/import-$($c.name).json"
        if (Test-Path $report) { $imports += (Get-Content $report -Raw | ConvertFrom-Json) } else { Write-Warning "no report for $($c.name); see $runDir/intellij-$($c.name).log" }
    }
    $info = [ordered]@{
        engine = 'intellij'; ml = $Ml; split = $Split; limit = $Limit
        ide = (Get-Content "$Ide/product-info.json" -Raw | ConvertFrom-Json).buildNumber
        positionsSha256 = (Get-FileHash $Positions -Algorithm SHA256).Hash.ToLower()
        started = $started; ended = (Get-Date).ToString('o'); corpora = $imports
    }
    $info | ConvertTo-Json -Depth 6 | Set-Content "$runDir/run.json"
    Write-Host "results: $runDir/results.jsonl"
}

Invoke-Run

exit 0
