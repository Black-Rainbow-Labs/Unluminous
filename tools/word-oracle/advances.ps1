# Measures the advance width Word lays each character out with in one font and size: a paragraph of the
# character repeated, and the distance Word reports between the first character and the paragraph mark,
# divided by the count. Writes `{ "family", "size", "advances": { "a": 6.12, ... } }` as JSON.
# Usage: pwsh -NoProfile -File tools/word-oracle/advances.ps1 -Family Aptos -Size 12 -Out <file.json>
param([Parameter(Mandatory)][string]$Family, [double]$Size = 12, [Parameter(Mandatory)][string]$Out, [int]$Repeat = 24)

. "$PSScriptRoot/WordCom.ps1"
$characters = @()
for ($c = 32; $c -le 126; $c++) { $characters += [string][char]$c }
$characters += @([string][char]0x2019, [string][char]0x201C, [string][char]0x201D, [string][char]0x2013, [string][char]0x2014, [string][char]0x2026)
$handle = $null
$result = [ordered]@{ family = $Family; size = $Size; repeat = $Repeat; advances = [ordered]@{} }
try {
    $handle = Start-WordApp -TimeoutSeconds 1200
    $word = $handle.App
    $doc = $word.Documents.Add()
    try { $doc.ActiveWindow.View.Type = 3 } catch { }
    $section = $doc.Sections.Item(1).PageSetup
    $section.LeftMargin = 36; $section.RightMargin = 36
    foreach ($ch in $characters) {
        $doc.Content.Text = ($ch * $Repeat) + 'x'
        $range = $doc.Content
        $range.Font.Name = $Family
        $range.Font.Size = $Size
        $range.Font.Kerning = 0
        $range.ParagraphFormat.Alignment = 0
        $start = $doc.Range(0, 0).Information(5)
        $end = $doc.Range($Repeat, $Repeat).Information(5)
        $line = $doc.Range($Repeat, $Repeat).Information(10)
        if ($line -ne 1) { continue }
        $result.advances[$ch] = [math]::Round(($end - $start) / $Repeat, 4)
    }
    $doc.Close(0)
} finally {
    Stop-WordApp $handle
}
$json = $result | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText([IO.Path]::GetFullPath($Out), $json, (New-Object Text.UTF8Encoding($false)))
