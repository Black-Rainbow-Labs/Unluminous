# Compiles the completion eval plugin with the JetBrains runtime's javac against the IDE's own jars.
# Output: D:/unluminous-completion-eval/build/plugin/completion-eval  (a plugin folder: lib/completion-eval.jar)
param(
    [string]$Ide = 'C:/Program Files/JetBrains/IntelliJ IDEA 2025.1',
    [string]$Out = 'D:/unluminous-completion-eval/build',
    [string]$RustPlugin = "$env:APPDATA/JetBrains/IntelliJIdea2025.3/plugins/intellij-rust"
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$javac = "$Ide/jbr/bin/javac.exe"
$classes = "$Out/classes"
$pluginDir = "$Out/plugin/completion-eval"
New-Item -ItemType Directory -Force $classes, "$pluginDir/lib" | Out-Null
# Only this build's own output is cleared, by literal path.
Remove-Item "$classes/org", "$classes/META-INF" -Recurse -Force -ErrorAction SilentlyContinue

$jars = @()
$jars += Get-ChildItem "$Ide/lib" -Filter *.jar | ForEach-Object FullName
$jars += "$Ide/plugins/completionMlRanking/lib/completionMlRanking.jar"
$jars += Get-ChildItem "$RustPlugin/lib" -Filter *.jar | ForEach-Object FullName
$cp = ($jars -join ';')
$sources = Get-ChildItem "$here/src" -Recurse -Filter *.java | ForEach-Object FullName
& $javac -nowarn --release 17 -encoding UTF-8 -cp $cp -d $classes @sources
if ($LASTEXITCODE -ne 0) { throw "javac failed" }
New-Item -ItemType Directory -Force "$classes/META-INF" | Out-Null
Copy-Item "$here/META-INF/*" "$classes/META-INF" -Force
if (Test-Path "$pluginDir/lib/completion-eval.jar") { Remove-Item "$pluginDir/lib/completion-eval.jar" -Force }
# The JetBrains runtime ships no jar tool, and a jar is a zip; META-INF must be at the archive root.
$zip = "$Out/completion-eval.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path "$classes/*" -DestinationPath $zip
Move-Item $zip "$pluginDir/lib/completion-eval.jar" -Force
Write-Host "built $pluginDir/lib/completion-eval.jar"
