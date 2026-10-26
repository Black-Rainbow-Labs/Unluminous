# Builds the Word fixtures with real Word features, saves them as .docx and runs inspect.ps1 on each.
# Usage: pwsh -NoProfile -File tools/word-oracle/make-fixtures.ps1 [-OutDir <dir>] [-Only name1,name2]
param([string]$OutDir = "$PSScriptRoot/../../crates/unluminous-word/tests/fixtures", [string[]]$Only = @())

. "$PSScriptRoot/WordCom.ps1"
$Only = @($Only | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$ErrorActionPreference = 'Stop'
$MISSING = [Type]::Missing
$script:Started = $false
$script:Notes = New-Object System.Collections.Generic.List[string]
$scratch = [IO.Path]::GetFullPath("$PSScriptRoot/../../_agent_output/task-2341-word")
New-Item -ItemType Directory -Force -Path $scratch | Out-Null
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path

# ---------- text generation (deterministic) ----------

$script:Words = 'the', 'editor', 'opens', 'a', 'document', 'and', 'shows', 'each', 'paragraph', 'with', 'careful', 'spacing', 'between', 'lines', 'while', 'the', 'reader', 'scrolls', 'through', 'pages', 'of', 'plain', 'text', 'that', 'wraps', 'at', 'the', 'margin', 'without', 'surprise'

# Builds sentence number $n from the fixed word list.
function Get-Sentence {
    param([int]$n)
    $len = 8 + (($n * 5) % 7)
    $w = for ($i = 0; $i -lt $len; $i++) { $script:Words[($n * 7 + $i * 3 + [int][math]::Floor($n / 30) * 11) % $script:Words.Count] }
    $s = $w -join ' '
    return $s.Substring(0, 1).ToUpper() + $s.Substring(1) + '.'
}

# Builds a paragraph of $count sentences starting from $seed.
function Get-Paragraph {
    param([int]$seed, [int]$count)
    return ((0..($count - 1) | ForEach-Object { Get-Sentence ($seed * 10 + $_) }) -join ' ')
}

# Word colour value from red, green, blue.
function Rgb { param([int]$r, [int]$g, [int]$b) return $r + 256 * $g + 65536 * $b }

# ---------- document building helpers ----------

# Makes the last paragraph of the document ready for new content (clean style and formatting).
function New-LastPara {
    param($doc, [string]$style = 'Normal')
    if ($script:Started) { $doc.Content.InsertParagraphAfter() }
    $script:Started = $true
    $p = $doc.Paragraphs.Last
    if ($p.Range.ListFormat.ListType -ne 0) { $p.Range.ListFormat.RemoveNumbers() }
    $p.Range.Style = $style
    $p.Range.ParagraphFormat.Reset()
    $p.Range.Font.Reset()
    return $p
}

# Appends a paragraph with the given text and style and returns it.
function Add-Para {
    param($doc, [string]$text, [string]$style = 'Normal')
    $p = New-LastPara $doc $style
    if ($text -ne '') { $p.Range.InsertBefore($text) }
    # Paragraph objects and ranges that include the final paragraph mark grow when text is appended to the document.
    # A range that stops before the mark does not, so return that (as .Range) with its paragraph format (as .Format).
    $r = $doc.Paragraphs.Last.Range
    $textRange = $doc.Range($r.Start, $r.End - 1)
    return [pscustomobject]@{ Range = $textRange; Format = $textRange.ParagraphFormat }
}

# Appends a paragraph "label + formatted segment + trailing text"; $fmt receives the range of the segment.
function Add-FormatPara {
    param($doc, [string]$label, [string]$segment, [scriptblock]$fmt)
    $p = Add-Para $doc ($label + $segment + ' and then plain text again.')
    $start = $p.Range.Start + $label.Length
    & $fmt ($doc.Range($start, $start + $segment.Length))
    return $p
}

# Appends a table in the last paragraph and returns it. The next Add-Para reuses the paragraph after the table.
function Add-Table {
    param($doc, [int]$rows, [int]$cols)
    $p = New-LastPara $doc 'Normal'
    $t = $doc.Tables.Add($p.Range, $rows, $cols)
    $t.Borders.Enable = $true
    $script:Started = $false
    return $t
}

# Collapsed range at the end of a paragraph, before its paragraph mark.
function Get-ParaEnd {
    param($doc, $p)
    $e = $p.Range.End
    if ($p.Range.Text.EndsWith("`r")) { $e-- }
    return $doc.Range($e, $e)
}

# Inserts a section break (2 next page, 3 continuous) at the end of the last paragraph; the next paragraph starts the new section.
function Add-SectionBreak {
    param($doc, [int]$type)
    (Get-ParaEnd $doc $doc.Paragraphs.Last).InsertBreak($type)
    $script:Started = $false
}

# Adds a field at the end of the last paragraph.
function Add-FieldAtEnd {
    param($doc, [int]$type, [string]$text = '')
    $r = Get-ParaEnd $doc $doc.Paragraphs.Last
    return $doc.Fields.Add($r, $type, $text, $false)
}

# Fills "Page X of Y" fields into a header/footer.
function Set-PageXofY {
    param($hf, [string]$prefix)
    $hf.Range.Text = $prefix
    $r = $hf.Range; $r.SetRange($r.End - 1, $r.End - 1); [void]$r.Fields.Add($r, 33)
    $r = $hf.Range; $r.SetRange($r.End - 1, $r.End - 1); $r.InsertAfter(' of ')
    $r = $hf.Range; $r.SetRange($r.End - 1, $r.End - 1); [void]$r.Fields.Add($r, 26)
}

# Records something that could not be produced through COM.
function Add-Note { param([string]$text) $script:Notes.Add($text); Write-Host "NOTE: $text" }

# Creates a list template on the document with the given level definitions (format, style, startAt).
function New-ListTemplate {
    param($doc, [bool]$outline, $levels)
    $tpl = $doc.ListTemplates.Add($outline)
    for ($i = 0; $i -lt $levels.Count; $i++) {
        $lv = $tpl.ListLevels.Item($i + 1)
        $def = $levels[$i]
        $lv.NumberFormat = $def.format
        $lv.NumberStyle = $def.style
        $lv.StartAt = $(if ($def.startAt) { $def.startAt } else { 1 })
        $lv.TrailingCharacter = 0
        $lv.NumberPosition = 18 * $i + $(if ($def.pos) { $def.pos } else { 18 })
        $lv.TextPosition = 18 * $i + 36
        $lv.TabPosition = 18 * $i + 36
        if ($def.font) { $lv.Font.Name = $def.font }
    }
    return $tpl
}

# Appends list items (text, level) and applies the template to them as one list.
function Add-List {
    param($doc, $tpl, $items, [bool]$continue = $false)
    $paras = foreach ($it in $items) { Add-Para $doc $it.text }
    $rng = $doc.Range($paras[0].Range.Start, $paras[-1].Range.End)
    $rng.ListFormat.ApplyListTemplate($tpl, $continue, 0, 2)
    for ($i = 0; $i -lt $paras.Count; $i++) { $paras[$i].Range.ListFormat.ListLevelNumber = $items[$i].level }
}

# Writes the sample PNG/JPEG pictures used by the picture fixtures.
function New-TestImages {
    Add-Type -AssemblyName System.Drawing
    $make = {
        param($path, $fmt, $w, $h, $c1, $c2, $label)
        $bmp = New-Object System.Drawing.Bitmap $w, $h
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush (New-Object System.Drawing.Point 0, 0), (New-Object System.Drawing.Point $w, $h), $c1, $c2
        $g.FillRectangle($brush, 0, 0, $w, $h)
        $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), 3
        $g.DrawRectangle($pen, 4, 4, $w - 8, $h - 8)
        $font = New-Object System.Drawing.Font 'Arial', 16, ([System.Drawing.FontStyle]::Bold)
        $g.DrawString($label, $font, [System.Drawing.Brushes]::White, 14, [int]($h / 2 - 14))
        $g.Dispose(); $bmp.Save($path, $fmt); $bmp.Dispose()
    }
    & $make "$scratch/sample.png" ([System.Drawing.Imaging.ImageFormat]::Png) 200 120 ([System.Drawing.Color]::DarkBlue) ([System.Drawing.Color]::Orange) 'PNG 200x120'
    & $make "$scratch/sample.jpg" ([System.Drawing.Imaging.ImageFormat]::Jpeg) 300 180 ([System.Drawing.Color]::DarkGreen) ([System.Drawing.Color]::Gold) 'JPEG 300x180'
}

# ---------- fixtures ----------

$fixtures = [ordered]@{}

$fixtures['formatting'] = {
    param($word, $doc)
    Add-Para $doc 'Character formatting' 'Heading 1' | Out-Null
    $items = @(
        @{ l = 'Bold: '; t = 'bold words'; f = { param($r) $r.Font.Bold = 1 } },
        @{ l = 'Italic: '; t = 'italic words'; f = { param($r) $r.Font.Italic = 1 } },
        @{ l = 'Bold italic: '; t = 'both together'; f = { param($r) $r.Font.Bold = 1; $r.Font.Italic = 1 } },
        @{ l = 'Underline single: '; t = 'single underline'; f = { param($r) $r.Font.Underline = 1 } },
        @{ l = 'Underline double: '; t = 'double underline'; f = { param($r) $r.Font.Underline = 3 } },
        @{ l = 'Underline thick: '; t = 'thick underline'; f = { param($r) $r.Font.Underline = 6 } },
        @{ l = 'Underline dotted: '; t = 'dotted underline'; f = { param($r) $r.Font.Underline = 4 } },
        @{ l = 'Underline dash: '; t = 'dashed underline'; f = { param($r) $r.Font.Underline = 7 } },
        @{ l = 'Underline wavy: '; t = 'wavy underline'; f = { param($r) $r.Font.Underline = 11 } },
        @{ l = 'Underline words: '; t = 'words only underline'; f = { param($r) $r.Font.Underline = 2 } },
        @{ l = 'Strikethrough: '; t = 'struck text'; f = { param($r) $r.Font.StrikeThrough = 1 } },
        @{ l = 'Double strikethrough: '; t = 'double struck'; f = { param($r) $r.Font.DoubleStrikeThrough = 1 } },
        @{ l = 'Superscript: '; t = 'raised'; f = { param($r) $r.Font.Superscript = 1 } },
        @{ l = 'Subscript: '; t = 'lowered'; f = { param($r) $r.Font.Subscript = 1 } },
        @{ l = 'Small caps: '; t = 'small capitals'; f = { param($r) $r.Font.SmallCaps = 1 } },
        @{ l = 'All caps: '; t = 'all capitals'; f = { param($r) $r.Font.AllCaps = 1 } },
        @{ l = 'Hidden: '; t = 'hidden text'; f = { param($r) $r.Font.Hidden = 1 } },
        @{ l = 'Colour red: '; t = 'red text'; f = { param($r) $r.Font.Color = (Rgb 255 0 0) } },
        @{ l = 'Colour green: '; t = 'green text'; f = { param($r) $r.Font.Color = (Rgb 0 128 0) } },
        @{ l = 'Colour custom: '; t = 'custom colour'; f = { param($r) $r.Font.Color = (Rgb 18 52 86) } },
        @{ l = 'Highlight yellow: '; t = 'yellow'; f = { param($r) $r.HighlightColorIndex = 7 } },
        @{ l = 'Highlight bright green: '; t = 'bright green'; f = { param($r) $r.HighlightColorIndex = 4 } },
        @{ l = 'Highlight turquoise: '; t = 'turquoise'; f = { param($r) $r.HighlightColorIndex = 3 } },
        @{ l = 'Highlight pink: '; t = 'pink'; f = { param($r) $r.HighlightColorIndex = 5 } },
        @{ l = 'Highlight red: '; t = 'red'; f = { param($r) $r.HighlightColorIndex = 6 } },
        @{ l = 'Font Calibri: '; t = 'Calibri text'; f = { param($r) $r.Font.Name = 'Calibri' } },
        @{ l = 'Font Times: '; t = 'Times New Roman text'; f = { param($r) $r.Font.Name = 'Times New Roman' } },
        @{ l = 'Font Arial: '; t = 'Arial text'; f = { param($r) $r.Font.Name = 'Arial' } },
        @{ l = 'Font Courier: '; t = 'Courier New text'; f = { param($r) $r.Font.Name = 'Courier New' } },
        @{ l = 'Font Georgia: '; t = 'Georgia text'; f = { param($r) $r.Font.Name = 'Georgia' } },
        @{ l = 'Size 8: '; t = 'eight point'; f = { param($r) $r.Font.Size = 8 } },
        @{ l = 'Size 11: '; t = 'eleven point'; f = { param($r) $r.Font.Size = 11 } },
        @{ l = 'Size 14: '; t = 'fourteen point'; f = { param($r) $r.Font.Size = 14 } },
        @{ l = 'Size 24: '; t = 'twenty four'; f = { param($r) $r.Font.Size = 24 } },
        @{ l = 'Size 36: '; t = 'thirty six'; f = { param($r) $r.Font.Size = 36 } },
        @{ l = 'Spacing expanded: '; t = 'expanded by 3pt'; f = { param($r) $r.Font.Spacing = 3 } },
        @{ l = 'Spacing condensed: '; t = 'condensed by 1.5pt'; f = { param($r) $r.Font.Spacing = -1.5 } }
    )
    foreach ($it in $items) { Add-FormatPara $doc $it.l $it.t $it.f | Out-Null }
    $p = Add-Para $doc 'Mixed run: plain, bold, italic, bold italic, and underlined in a single paragraph.'
    $s = $p.Range.Start
    $doc.Range($s + 13, $s + 17).Font.Bold = 1
    $doc.Range($s + 19, $s + 25).Font.Italic = 1
    $b = $s + 27; $doc.Range($b, $b + 11).Font.Bold = 1; $doc.Range($b, $b + 11).Font.Italic = 1
    $doc.Range($p.Range.End - 12, $p.Range.End - 2).Font.Underline = 1
}

$fixtures['paragraphs'] = {
    param($word, $doc)
    $sample = { param($n) Get-Paragraph $n 2 }
    $p = Add-Para $doc ('Left aligned. ' + (& $sample 1)); $p.Format.Alignment = 0
    $p = Add-Para $doc ('Centered. ' + (& $sample 2)); $p.Format.Alignment = 1
    $p = Add-Para $doc ('Right aligned. ' + (& $sample 3)); $p.Format.Alignment = 2
    $p = Add-Para $doc ('Justified. ' + (Get-Paragraph 4 4)); $p.Format.Alignment = 3
    $p = Add-Para $doc ('Left indent 36 and right indent 36. ' + (& $sample 5)); $p.Format.LeftIndent = 36; $p.Format.RightIndent = 36
    $p = Add-Para $doc ('First line indent 24. ' + (& $sample 6)); $p.Format.FirstLineIndent = 24
    $p = Add-Para $doc ('Hanging indent 36. ' + (& $sample 7)); $p.Format.LeftIndent = 36; $p.Format.FirstLineIndent = -36
    $p = Add-Para $doc ('Space before 24 and after 12. ' + (& $sample 8)); $p.Format.SpaceBefore = 24; $p.Format.SpaceAfter = 12
    $p = Add-Para $doc ('Space before 0 and after 0. ' + (& $sample 9)); $p.Format.SpaceBefore = 0; $p.Format.SpaceAfter = 0
    $p = Add-Para $doc ('Line spacing single. ' + (Get-Paragraph 10 4)); $p.Format.LineSpacingRule = 0
    $p = Add-Para $doc ('Line spacing 1.5 lines. ' + (Get-Paragraph 11 4)); $p.Format.LineSpacingRule = 1
    $p = Add-Para $doc ('Line spacing double. ' + (Get-Paragraph 12 4)); $p.Format.LineSpacingRule = 2
    $p = Add-Para $doc ('Line spacing at least 18pt. ' + (Get-Paragraph 13 4)); $p.Format.LineSpacingRule = 3; $p.Format.LineSpacing = 18
    $p = Add-Para $doc ('Line spacing exactly 20pt. ' + (Get-Paragraph 14 4)); $p.Format.LineSpacingRule = 4; $p.Format.LineSpacing = 20
    $p = Add-Para $doc ('Line spacing multiple 1.15. ' + (Get-Paragraph 15 4)); $p.Format.LineSpacingRule = 5; $p.Format.LineSpacing = $word.LinesToPoints(1.15)
    $p = Add-Para $doc ('Line spacing multiple 3. ' + (Get-Paragraph 16 4)); $p.Format.LineSpacingRule = 5; $p.Format.LineSpacing = $word.LinesToPoints(3)
    $p = Add-Para $doc ('Keep with next. ' + (& $sample 17)); $p.Format.KeepWithNext = -1
    $p = Add-Para $doc ('Keep together. ' + (Get-Paragraph 18 5)); $p.Format.KeepTogether = -1
    $p = Add-Para $doc ("Tab stops:`tleft 72`tcenter 180`t12.5`tright 430"); $p.Format.TabStops.ClearAll()
    [void]$p.Format.TabStops.Add(72, 0, 0); [void]$p.Format.TabStops.Add(180, 1, 0); [void]$p.Format.TabStops.Add(300, 3, 1); [void]$p.Format.TabStops.Add(430, 2, 0)
    $p = Add-Para $doc ("Name`t.....`t1234.56"); $p.Format.TabStops.ClearAll(); [void]$p.Format.TabStops.Add(200, 3, 1); [void]$p.Format.TabStops.Add(440, 2, 1)
    $p = Add-Para $doc ('Bordered and shaded paragraph. ' + (& $sample 19))
    $p.Format.Borders.Enable = $true; $p.Format.Shading.BackgroundPatternColor = (Rgb 255 230 200)
    $p = Add-Para $doc ('Page break before this paragraph. ' + (& $sample 20)); $p.Format.PageBreakBefore = -1
}

$fixtures['styles'] = {
    param($word, $doc)
    $custom = $doc.Styles.Add('Oracle Paragraph', 1)
    $custom.BaseStyle = 'Normal'; $custom.Font.Bold = 1; $custom.Font.Size = 14; $custom.Font.Color = (Rgb 0 0 255); $custom.ParagraphFormat.SpaceAfter = 18
    $char = $doc.Styles.Add('Oracle Emphasis', 2)
    $char.Font.Italic = 1; $char.Font.Color = (Rgb 192 0 0)
    foreach ($s in 'Title', 'Subtitle', 'Heading 1', 'Heading 2', 'Heading 3', 'Heading 4', 'Normal', 'Quote', 'Intense Quote', 'No Spacing', 'Oracle Paragraph') {
        Add-Para $doc "$s sample text" $s | Out-Null
        Add-Para $doc (Get-Paragraph 3 1) 'Normal' | Out-Null
    }
    $p = Add-Para $doc 'A normal paragraph with a custom character style applied to some words in the middle.'
    $doc.Range($p.Range.Start + 38, $p.Range.Start + 62).Style = 'Oracle Emphasis'
}

$fixtures['lists'] = {
    param($word, $doc)
    $bullet = [string][char]0xF0B7
    $tplBullet = New-ListTemplate $doc $true @(@{ format = $bullet; style = 23; font = 'Symbol' }, @{ format = 'o'; style = 23; font = 'Courier New' }, @{ format = ([string][char]0xF0A7); style = 23; font = 'Wingdings' })
    $tplNum = New-ListTemplate $doc $false @(@{ format = '%1.'; style = 0 })
    $tplOutline = New-ListTemplate $doc $true @(@{ format = '%1'; style = 0 }, @{ format = '%1.%2'; style = 0 }, @{ format = '%1.%2.%3'; style = 0 })
    $tplLetter = New-ListTemplate $doc $false @(@{ format = '%1)'; style = 4 })
    $tplRoman = New-ListTemplate $doc $false @(@{ format = '%1.'; style = 2 })
    $tplFive = New-ListTemplate $doc $false @(@{ format = '%1.'; style = 0; startAt = 5 })
    Add-Para $doc 'Bullets with three levels' 'Heading 2' | Out-Null
    Add-List $doc $tplBullet @(@{ text = 'Bullet level one'; level = 1 }, @{ text = 'Bullet level two'; level = 2 }, @{ text = 'Bullet level three'; level = 3 }, @{ text = 'Back to level two'; level = 2 }, @{ text = 'Back to level one'; level = 1 })
    Add-Para $doc 'Numbered list' 'Heading 2' | Out-Null
    Add-List $doc $tplNum @(@{ text = 'First item'; level = 1 }, @{ text = 'Second item'; level = 1 }, @{ text = 'Third item'; level = 1 })
    Add-Para $doc 'Multilevel outline' 'Heading 2' | Out-Null
    Add-List $doc $tplOutline @(@{ text = 'Outline one'; level = 1 }, @{ text = 'Outline one point one'; level = 2 }, @{ text = 'Outline one point one point one'; level = 3 }, @{ text = 'Outline one point two'; level = 2 }, @{ text = 'Outline two'; level = 1 })
    Add-Para $doc 'Lower letter and roman' 'Heading 2' | Out-Null
    Add-List $doc $tplLetter @(@{ text = 'Letter a'; level = 1 }, @{ text = 'Letter b'; level = 1 }, @{ text = 'Letter c'; level = 1 })
    Add-List $doc $tplRoman @(@{ text = 'Roman one'; level = 1 }, @{ text = 'Roman two'; level = 1 }, @{ text = 'Roman three'; level = 1 })
    Add-Para $doc 'Restarted at 1' 'Heading 2' | Out-Null
    Add-List $doc $tplNum @(@{ text = 'Restarted first'; level = 1 }, @{ text = 'Restarted second'; level = 1 })
    Add-Para $doc 'A normal paragraph between the two parts of one list.' | Out-Null
    Add-List $doc $tplNum @(@{ text = 'Continued after the paragraph'; level = 1 }, @{ text = 'Continued again'; level = 1 }) $true
    Add-Para $doc 'List starting at 5' 'Heading 2' | Out-Null
    Add-List $doc $tplFive @(@{ text = 'Starts at five'; level = 1 }, @{ text = 'Then six'; level = 1 }, @{ text = 'Then seven'; level = 1 })
}

$fixtures['tables'] = {
    param($word, $doc)
    Add-Para $doc 'Table with a repeating header row' 'Heading 2' | Out-Null
    $t = Add-Table $doc 4 3
    for ($r = 1; $r -le 4; $r++) { for ($c = 1; $c -le 3; $c++) { $t.Cell($r, $c).Range.Text = $(if ($r -eq 1) { "Header $c" } else { "R${r}C${c}" }) } }
    $t.Rows.Item(1).HeadingFormat = -1; $t.Rows.Item(1).Range.Font.Bold = 1
    Add-Para $doc 'Merged cells' 'Heading 2' | Out-Null
    $t = Add-Table $doc 4 4
    $t.Cell(1, 1).Merge($t.Cell(1, 2))
    $t.Cell(2, 4).Merge($t.Cell(3, 4))
    $t.Cell(1, 1).Range.Text = 'Merged across two columns'
    $t.Cell(2, 4).Range.Text = 'Merged down two rows'
    $t.Cell(2, 1).Range.Text = 'A2'; $t.Cell(3, 1).Range.Text = 'A3'; $t.Cell(4, 1).Range.Text = 'A4'
    Add-Para $doc 'Shading and varied widths' 'Heading 2' | Out-Null
    $t = Add-Table $doc 3 3
    $t.AllowAutoFit = $false
    $t.Columns.Item(1).Width = 60; $t.Columns.Item(2).Width = 150; $t.Columns.Item(3).Width = 240
    for ($r = 1; $r -le 3; $r++) { for ($c = 1; $c -le 3; $c++) { $t.Cell($r, $c).Range.Text = "S${r}${c}" } }
    $t.Cell(1, 1).Shading.BackgroundPatternColor = (Rgb 255 200 200)
    $t.Cell(2, 2).Shading.BackgroundPatternColor = (Rgb 200 255 200)
    $t.Cell(3, 3).Shading.BackgroundPatternColor = (Rgb 200 200 255)
    Add-Para $doc 'Nested table' 'Heading 2' | Out-Null
    $t = Add-Table $doc 2 2
    $t.Cell(1, 1).Range.Text = 'Outer 1,1'; $t.Cell(1, 2).Range.Text = 'Outer 1,2'; $t.Cell(2, 1).Range.Text = 'Outer 2,1'
    $cr = $t.Cell(2, 2).Range; $cr.Collapse(1)
    $inner = $doc.Tables.Add($cr, 2, 2)
    $inner.Cell(1, 1).Range.Text = 'In 1,1'; $inner.Cell(1, 2).Range.Text = 'In 1,2'; $inner.Cell(2, 1).Range.Text = 'In 2,1'; $inner.Cell(2, 2).Range.Text = 'In 2,2'
    Add-Para $doc 'Table with borders removed' 'Heading 2' | Out-Null
    $t = Add-Table $doc 2 3
    $t.Borders.Enable = $false
    for ($r = 1; $r -le 2; $r++) { for ($c = 1; $c -le 3; $c++) { $t.Cell($r, $c).Range.Text = "NB${r}${c}" } }
    Add-Para $doc 'Cell text alignment' 'Heading 2' | Out-Null
    $t = Add-Table $doc 2 3
    $t.Rows.Item(1).HeightRule = 1; $t.Rows.Item(1).Height = 50
    $t.Cell(1, 1).Range.Text = 'Left top'; $t.Cell(1, 2).Range.Text = 'Center middle'; $t.Cell(1, 3).Range.Text = 'Right bottom'
    $t.Cell(1, 1).Range.ParagraphFormat.Alignment = 0; $t.Cell(1, 1).VerticalAlignment = 0
    $t.Cell(1, 2).Range.ParagraphFormat.Alignment = 1; $t.Cell(1, 2).VerticalAlignment = 1
    $t.Cell(1, 3).Range.ParagraphFormat.Alignment = 2; $t.Cell(1, 3).VerticalAlignment = 3
    $t.Cell(2, 1).Range.Text = 'Justified text in a cell that is long enough to wrap onto several lines of text'; $t.Cell(2, 1).Range.ParagraphFormat.Alignment = 3
    Add-Para $doc 'End of tables.' | Out-Null
}

$fixtures['pictures'] = {
    param($word, $doc)
    New-TestImages
    Add-Para $doc 'Inline PNG, 200 by 120 points as inserted:' | Out-Null
    $p = Add-Para $doc ''; $r = $p.Range; $r.Collapse(1)
    [void]$doc.InlineShapes.AddPicture("$scratch/sample.png", $false, $true, $r)
    Add-Para $doc 'Same PNG resized to 100 by 60 points:' | Out-Null
    $p = Add-Para $doc ''; $r = $p.Range; $r.Collapse(1)
    $s = $doc.InlineShapes.AddPicture("$scratch/sample.png", $false, $true, $r)
    $s.LockAspectRatio = 0; $s.Width = 100; $s.Height = 60
    Add-Para $doc 'JPEG, 300 by 180 points as inserted:' | Out-Null
    $p = Add-Para $doc ''; $r = $p.Range; $r.Collapse(1)
    [void]$doc.InlineShapes.AddPicture("$scratch/sample.jpg", $false, $true, $r)
    $p = Add-Para $doc ('A floating picture with square wrapping sits to the right of this paragraph. ' + (Get-Paragraph 30 6))
    $anchor = $doc.Range($p.Range.Start, $p.Range.Start)
    $shp = $doc.Shapes.AddPicture("$scratch/sample.png", $false, $true, 330, 400, 140, 84, $anchor)
    $shp.WrapFormat.Type = 0
    Add-Para $doc (Get-Paragraph 31 6) | Out-Null
}

$fixtures['sections'] = {
    param($word, $doc)
    $ps = $doc.Sections.Item(1).PageSetup
    $ps.PageWidth = 612; $ps.PageHeight = 792; $ps.TopMargin = 72; $ps.BottomMargin = 72; $ps.LeftMargin = 72; $ps.RightMargin = 72
    Add-Para $doc 'Section one: portrait Letter' 'Heading 1' | Out-Null
    Add-Para $doc (Get-Paragraph 40 4) | Out-Null
    Add-SectionBreak $doc 2
    $ps = $doc.Sections.Item(2).PageSetup
    $ps.PaperSize = 7; $ps.Orientation = 1; $ps.TopMargin = 36; $ps.BottomMargin = 36; $ps.LeftMargin = 36; $ps.RightMargin = 36
    Add-Para $doc 'Section two: landscape A4, narrow margins' 'Heading 1' | Out-Null
    Add-Para $doc (Get-Paragraph 41 5) | Out-Null
    Add-SectionBreak $doc 3
    $s3 = $doc.Sections.Item(3).PageSetup
    $s3.TextColumns.SetCount(2)
    Add-Para $doc 'Section three: two columns' 'Heading 1' | Out-Null
    Add-Para $doc (Get-Paragraph 42 5) | Out-Null
    $p = $doc.Paragraphs.Last; $e = Get-ParaEnd $doc $p; $e.InsertBreak(8)
    Add-Para $doc 'After the column break.' | Out-Null
    Add-Para $doc (Get-Paragraph 43 5) | Out-Null
}

$fixtures['headers'] = {
    param($word, $doc)
    $sec = $doc.Sections.Item(1)
    $sec.PageSetup.DifferentFirstPageHeaderFooter = -1
    $sec.PageSetup.OddAndEvenPagesHeaderFooter = -1
    $sec.Headers.Item(1).Range.Text = 'Primary header (odd pages)'
    $sec.Headers.Item(2).Range.Text = 'First page header'
    $sec.Headers.Item(3).Range.Text = 'Even page header'
    $sec.Footers.Item(2).Range.Text = 'First page footer'
    Set-PageXofY $sec.Footers.Item(1) 'Page '
    Set-PageXofY $sec.Footers.Item(3) 'Even Page '
    $sec.Footers.Item(1).Range.ParagraphFormat.Alignment = 1
    Add-Para $doc 'Headers and footers' 'Heading 1' | Out-Null
    for ($i = 1; $i -le 30; $i++) { Add-Para $doc (Get-Paragraph (50 + $i) 5) | Out-Null }
}

$fixtures['notes'] = {
    param($word, $doc)
    Add-Para $doc 'Footnotes and endnotes' 'Heading 1' | Out-Null
    for ($i = 1; $i -le 4; $i++) {
        $p = Add-Para $doc ("Paragraph with footnote $i. " + (Get-Paragraph (60 + $i) 2))
        [void]$doc.Footnotes.Add((Get-ParaEnd $doc $p), $MISSING, "Footnote text number $i.")
    }
    for ($i = 1; $i -le 2; $i++) {
        $p = Add-Para $doc ("Paragraph with endnote $i. " + (Get-Paragraph (70 + $i) 2))
        [void]$doc.Endnotes.Add((Get-ParaEnd $doc $p), $MISSING, "Endnote text number $i.")
    }
}

$fixtures['comments'] = {
    param($word, $doc)
    $word.UserName = 'Oracle'; $word.UserInitials = 'OR'
    $paras = 1..4 | ForEach-Object { Add-Para $doc ("Commented paragraph $_. " + (Get-Paragraph (80 + $_) 2)) }
    $c1 = $doc.Comments.Add($doc.Range($paras[0].Range.Start, $paras[0].Range.Start + 11), 'First comment from the oracle.')
    $c2 = $doc.Comments.Add($doc.Range($paras[1].Range.Start, $paras[1].Range.Start + 11), 'Second comment, resolved.')
    $c3 = $doc.Comments.Add($doc.Range($paras[2].Range.Start, $paras[2].Range.Start + 11), 'Third comment with a reply.')
    try { [void]$c3.Replies.Add($c3.Scope, 'A reply to the third comment.') } catch { Add-Note "Comment reply failed: $($_.Exception.Message)" }
    try { $c2.Done = $true } catch { Add-Note "Comment Done failed: $($_.Exception.Message)" }
}

$fixtures['revisions'] = {
    param($word, $doc)
    $word.UserName = 'Oracle'; $word.UserInitials = 'OR'
    $a = Add-Para $doc 'This sentence will receive an insertion at the marker: HERE and then it ends.'
    $b = Add-Para $doc 'This sentence has some words that will be deleted from the middle of it.'
    $c = Add-Para $doc 'This sentence will have part of its text made bold by a tracked format change.'
    $doc.TrackRevisions = $true
    $s = $a.Range.Start + $a.Range.Text.IndexOf('HERE') + 4
    $doc.Range($s, $s).InsertAfter(' [inserted words]')
    $s = $b.Range.Start + $b.Range.Text.IndexOf('some words that will be deleted ')
    $doc.Range($s, $s + 32).Delete() | Out-Null
    $s = $c.Range.Start + $c.Range.Text.IndexOf('part of its text')
    $doc.Range($s, $s + 16).Font.Bold = 1
    $doc.TrackRevisions = $false
}

$fixtures['fields'] = {
    param($word, $doc)
    Add-Para $doc 'Contents' 'Normal' | Out-Null
    $placeholder = Add-Para $doc ''
    Add-Para $doc 'First chapter' 'Heading 1' | Out-Null
    Add-Para $doc (Get-Paragraph 90 2) | Out-Null
    Add-Para $doc 'A section' 'Heading 2' | Out-Null
    Add-Para $doc (Get-Paragraph 91 2) | Out-Null
    Add-Para $doc 'A subsection' 'Heading 3' | Out-Null
    $target = Add-Para $doc 'This paragraph is the bookmark target. '
    [void]$doc.Bookmarks.Add('OracleTarget', $target.Range)
    Add-Para $doc 'Second chapter' 'Heading 1' | Out-Null
    Add-Para $doc 'Today is ' | Out-Null
    [void](Add-FieldAtEnd $doc 31 '\@ "yyyy-MM-dd"')
    $p = Add-Para $doc 'Visit '
    $r = Get-ParaEnd $doc $p
    [void]$doc.Hyperlinks.Add($r, 'https://example.com', $MISSING, 'Example site', 'example.com')
    $p = Add-Para $doc 'Jump to the '
    $r = Get-ParaEnd $doc $p
    [void]$doc.Hyperlinks.Add($r, '', 'OracleTarget', 'Go to target', 'bookmark target')
    Add-Para $doc 'Figure ' 'Caption' | Out-Null
    [void](Add-FieldAtEnd $doc -1 'SEQ Figure \* ARABIC')
    $doc.Paragraphs.Last.Range.InsertAfter(': a captioned figure')
    $r = $placeholder.Range; $r.Collapse(1)
    [void]$doc.TablesOfContents.Add($r, $true, 1, 3)
    $doc.TablesOfContents.Item(1).Update()
}

$fixtures['long'] = {
    param($word, $doc)
    $bullet = [string][char]0xF0B7
    $tplBullet = New-ListTemplate $doc $false @(@{ format = $bullet; style = 23; font = 'Symbol' })
    $tplNum = New-ListTemplate $doc $false @(@{ format = '%1.'; style = 0 })
    Add-Para $doc 'A long document for pagination tests' 'Title' | Out-Null
    $chapter = 0
    do {
        $chapter++
        Add-Para $doc "Chapter $chapter" 'Heading 1' | Out-Null
        Add-Para $doc (Get-Paragraph (100 + $chapter * 5) 3) | Out-Null
        Add-Para $doc (Get-Paragraph (101 + $chapter * 5) 7) | Out-Null
        Add-Para $doc "Section $chapter.1" 'Heading 2' | Out-Null
        Add-Para $doc (Get-Paragraph (102 + $chapter * 5) 5) | Out-Null
        Add-Para $doc (Get-Paragraph (103 + $chapter * 5) 2) | Out-Null
        if ($chapter -eq 3 -or $chapter -eq 6) {
            $tpl = $(if ($chapter -eq 3) { $tplBullet } else { $tplNum })
            Add-List $doc $tpl @(@{ text = 'First point of the list'; level = 1 }, @{ text = 'Second point of the list'; level = 1 }, @{ text = 'Third point of the list'; level = 1 })
        }
        if ($chapter -eq 5) {
            $t = Add-Table $doc 5 3
            for ($r = 1; $r -le 5; $r++) { for ($c = 1; $c -le 3; $c++) { $t.Cell($r, $c).Range.Text = "Cell $r.$c" } }
            $t.Rows.Item(1).HeadingFormat = -1
            Add-Para $doc 'Text after the table.' | Out-Null
        }
        Add-Para $doc "Section $chapter.2" 'Heading 2' | Out-Null
        Add-Para $doc (Get-Paragraph (104 + $chapter * 5) 8) | Out-Null
        $pages = $doc.ComputeStatistics(2)
    } while ($pages -lt 12 -and $chapter -lt 40)
}

$fixtures['builtin-styles'] = {
    param($word, $doc)
    # One paragraph per built in paragraph style, so Word writes each definition into styles.xml.
    $names = @('Normal', 'Title', 'Subtitle') + (1..9 | ForEach-Object { "Heading $_" }) + @('Quote', 'Intense Quote', 'No Spacing', 'List Paragraph', 'Caption', 'TOC Heading') + (1..9 | ForEach-Object { "TOC $_" }) + @('Footnote Text', 'Endnote Text', 'Header', 'Footer', 'List Bullet', 'List Bullet 2', 'List Bullet 3', 'List Number', 'List Number 2', 'List Number 3', 'List', 'List Continue', 'Table of Figures', 'HTML Preformatted', 'Block Text', 'Body Text', 'Plain Text', 'Bibliography', 'Index 1', 'Envelope Address', 'Signature', 'Salutation', 'Closing', 'Date')
    foreach ($n in $names) { Add-Para $doc "$n paragraph" $n | Out-Null }
    # Character styles applied to a word in their own paragraphs.
    foreach ($n in 'Strong', 'Emphasis', 'Intense Emphasis', 'Subtle Emphasis', 'Hyperlink', 'Footnote Reference', 'Endnote Reference', 'Placeholder Text', 'Book Title', 'Subtle Reference', 'Intense Reference', 'FollowedHyperlink', 'HTML Code', 'Page Number', 'Line Number') {
        $p = Add-Para $doc "Sample $n text"
        $doc.Range($p.Range.Start + 7, $p.Range.Start + 7 + $n.Length).Style = $n
    }
    Add-Para $doc 'Table Grid style:' | Out-Null
    $t = Add-Table $doc 2 2
    $t.Style = 'Table Grid'
    for ($r = 1; $r -le 2; $r++) { for ($c = 1; $c -le 2; $c++) { $t.Cell($r, $c).Range.Text = "G${r}${c}" } }
    # Word's plain, grid and list table styles, so a table style can be applied from the built in set
    # (task-2359, TDD 5.4). Each in a small table of its own, which is what makes Word write it.
    foreach ($n in 'Table Grid Light', 'Plain Table 1', 'Plain Table 2', 'Plain Table 3', 'Plain Table 4', 'Plain Table 5', 'Grid Table 1 Light', 'Grid Table 2', 'Grid Table 4', 'Grid Table 4 - Accent 1', 'Grid Table 5 Dark', 'List Table 1 Light', 'List Table 2', 'List Table 3', 'List Table 4 - Accent 1') {
        Add-Para $doc "$n style:" | Out-Null
        $t = Add-Table $doc 2 2
        try { $t.Style = $n } catch { Write-Host "   no table style $n" }
        for ($r = 1; $r -le 2; $r++) { for ($c = 1; $c -le 2; $c++) { $t.Cell($r, $c).Range.Text = "$r$c" } }
    }
}

$fixtures['blank'] = {
    param($word, $doc)
}

$fixtures['kitchen-sink'] = {
    param($word, $doc)
    New-TestImages
    $bullet = [string][char]0xF0B7
    $tplBullet = New-ListTemplate $doc $false @(@{ format = $bullet; style = 23; font = 'Symbol' })
    $tplNum = New-ListTemplate $doc $false @(@{ format = '%1.'; style = 0 })
    $sec = $doc.Sections.Item(1)
    $sec.Headers.Item(1).Range.Text = 'Black Rainbow Labs - Quarterly Report'
    $sec.Headers.Item(1).Range.ParagraphFormat.Alignment = 2
    Set-PageXofY $sec.Footers.Item(1) 'Page '
    $sec.Footers.Item(1).Range.ParagraphFormat.Alignment = 1
    Add-Para $doc 'Quarterly Report' 'Title' | Out-Null
    Add-Para $doc 'Prepared for the board of directors' 'Subtitle' | Out-Null
    Add-Para $doc "Jason Example`rBlack Rainbow Labs`r1 Example Street`rSpringfield" | Out-Null
    $p = Add-Para $doc 'October 23, 2026'; $p.Format.Alignment = 2
    Add-Para $doc 'Dear board members,' | Out-Null
    $p = Add-Para $doc ('This report summarises the quarter. It covers revenue, product work and the plan ahead. ' + (Get-Paragraph 120 3))
    $s = $p.Range.Start
    $doc.Range($s + 5, $s + 11).Font.Bold = 1
    $doc.Range($s + 12, $s + 14).Font.Italic = 1
    [void]$doc.Footnotes.Add((Get-ParaEnd $doc $p), $MISSING, 'Figures are unaudited and rounded to the nearest thousand.')
    Add-Para $doc 'Highlights' 'Heading 1' | Out-Null
    Add-List $doc $tplBullet @(@{ text = 'Revenue grew in every region'; level = 1 }, @{ text = 'The editor shipped three releases'; level = 1 }, @{ text = 'Support response time fell by a third'; level = 1 })
    Add-Para $doc 'Results by region' 'Heading 1' | Out-Null
    $t = Add-Table $doc 4 3
    $vals = @('Region', 'Revenue', 'Growth', 'North', '120,000', '12%', 'South', '98,000', '8%', 'West', '143,000', '15%')
    for ($i = 0; $i -lt 12; $i++) { $t.Cell([int][math]::Floor($i / 3) + 1, ($i % 3) + 1).Range.Text = $vals[$i] }
    $t.Rows.Item(1).HeadingFormat = -1; $t.Rows.Item(1).Range.Font.Bold = 1
    $t.Rows.Item(1).Shading.BackgroundPatternColor = (Rgb 220 230 241)
    Add-Para $doc 'Chart' 'Heading 1' | Out-Null
    $p = Add-Para $doc ''; $r = $p.Range; $r.Collapse(1)
    [void]$doc.InlineShapes.AddPicture("$scratch/sample.png", $false, $true, $r)
    $p.Format.Alignment = 1
    Add-Para $doc 'Plan' 'Heading 1' | Out-Null
    Add-List $doc $tplNum @(@{ text = 'Finish the Word plugin'; level = 1 }, @{ text = 'Improve pagination'; level = 1 }, @{ text = 'Release the next version'; level = 1 })
    Add-Para $doc (Get-Paragraph 121 4) | Out-Null
    Add-Para $doc (Get-Paragraph 122 4) | Out-Null
    Add-Para $doc (Get-Paragraph 123 4) | Out-Null
    $p = Add-Para $doc 'More detail is available at '
    [void]$doc.Hyperlinks.Add((Get-ParaEnd $doc $p), 'https://example.com/report', $MISSING, 'Report', 'example.com/report')
    $doc.Paragraphs.Last.Range.InsertAfter('.')
    Add-Para $doc 'Sincerely,' | Out-Null
    Add-Para $doc 'Jason' | Out-Null
}

# ---------- round 2 fixtures (task-2359) ----------
# Each one is made for a rule the round 2 design (tasks/task-2358-word-round2-tdd.md) measures against Word.

# A paragraph's end as a collapsed range, for inserting an object or a field after the text.
function Get-EndOf { param($doc, $p) $e = $p.Range.End; return $doc.Range($e, $e) }

# An equation made from linear text inside a paragraph: `$from` and `$length` say where the text is.
function Add-Equation {
    param($doc, $p, [int]$from, [int]$length)
    $r = $doc.Range($p.Range.Start + $from, $p.Range.Start + $from + $length)
    [void]$doc.OMaths.Add($r)
    $r.OMaths.Item(1).BuildUp()
}

$fixtures['equations'] = {
    param($word, $doc)
    Add-Para $doc 'Equations' 'Heading 1' | Out-Null
    $p = Add-Para $doc 'The area of a circle is A=πr^2 for a circle of radius r.'
    Add-Equation $doc $p 24 6
    Add-Para $doc 'The roots of a quadratic are given by the display equation below.' | Out-Null
    $p = Add-Para $doc ''
    $r = $doc.Range($p.Range.Start, $p.Range.Start)
    $r.Text = 'x=(-b±√(b^2-4ac))/2a'
    [void]$doc.OMaths.Add($r)
    $r.OMaths.Item(1).BuildUp()
    $p = Add-Para $doc 'A fraction inline: 1/2+1/3=5/6, and text after it.'
    Add-Equation $doc $p 19 11
    Add-Para $doc (Get-Paragraph 200 3) | Out-Null
}

$fixtures['letterhead'] = {
    param($word, $doc)
    New-TestImages
    $h = $doc.Sections.Item(1).Headers.Item(1)
    $h.Range.Text = "Black Rainbow Labs`rResearch and development"
    # A range made by $doc.Range is in the body, so the place in the header is the header's own range,
    # collapsed to its start.
    $at = $h.Range.Paragraphs.Item(1).Range
    $at.Collapse(1)
    [void]$h.Range.InlineShapes.AddPicture("$scratch/sample.png", $false, $true, $at)
    $h.Range.InsertParagraphAfter()
    $last = $h.Range.Paragraphs.Last.Range
    $t = $h.Range.Tables.Add($last, 1, 2)
    $t.Cell(1, 1).Range.Text = '1 Example Street, Springfield'
    $t.Cell(1, 2).Range.Text = 'hello@example.com'
    $t.Borders.Enable = $false
    Add-Para $doc 'October 24, 2026' | Out-Null
    Add-Para $doc 'Dear reader,' | Out-Null
    Add-Para $doc (Get-Paragraph 210 4) | Out-Null
    Add-Para $doc (Get-Paragraph 211 4) | Out-Null
    Add-Para $doc 'Yours sincerely,' | Out-Null
}

$fixtures['template'] = {
    param($word, $doc)
    Add-Para $doc 'A template' 'Title' | Out-Null
    Add-Para $doc 'Section heading' 'Heading 1' | Out-Null
    Add-Para $doc (Get-Paragraph 220 3) | Out-Null
    Add-Para $doc 'Second heading' 'Heading 2' | Out-Null
    Add-Para $doc (Get-Paragraph 221 2) | Out-Null
}

$fixtures['macros'] = {
    param($word, $doc)
    Add-Para $doc 'A document with macros' 'Heading 1' | Out-Null
    Add-Para $doc (Get-Paragraph 230 3) | Out-Null
    try {
        $module = $doc.VBProject.VBComponents.Add(1)
        $module.CodeModule.AddFromString("Sub Hello()`r`n    MsgBox ""Hello""`r`nEnd Sub")
    } catch {
        Add-Note "macros: no VBA project could be added ($($_.Exception.Message)); the file is a .docm with no vbaProject.bin"
    }
}

$fixtures['strict'] = {
    param($word, $doc)
    Add-Para $doc 'Strict Open XML' 'Heading 1' | Out-Null
    $p = Add-Para $doc ('Bold and italic words. ' + (Get-Paragraph 240 2))
    $s = $p.Range.Start
    $doc.Range($s, $s + 4).Font.Bold = 1
    $doc.Range($s + 9, $s + 15).Font.Italic = 1
    $t = Add-Table $doc 2 2
    $t.Cell(1, 1).Range.Text = 'a'; $t.Cell(1, 2).Range.Text = 'b'; $t.Cell(2, 1).Range.Text = 'c'; $t.Cell(2, 2).Range.Text = 'd'
    Add-Para $doc (Get-Paragraph 241 2) | Out-Null
}

$fixtures['marks'] = {
    param($word, $doc)
    $ps = foreach ($i in 0..4) { Add-Para $doc ("Paragraph $($i + 1). " + (Get-Paragraph (250 + $i) 2)) }
    $from = $ps[1].Range.Start + 5
    $to = $ps[3].Range.Start + 9
    [void]$doc.Bookmarks.Add('Span', $doc.Range($from, $to))
    [void]$doc.Comments.Add($doc.Range($from + 2, $to + 3), 'A comment over three paragraphs.')
}

$fixtures['wrapping'] = {
    param($word, $doc)
    New-TestImages
    Add-Para $doc 'Wrapping' 'Heading 1' | Out-Null
    $kinds = @(
        @{ n = 'square'; t = 0; side = 0; left = 0 },
        @{ n = 'square right side'; t = 0; side = 2; left = 330 },
        @{ n = 'tight'; t = 1; side = 0; left = 330 },
        @{ n = 'through'; t = 2; side = 0; left = 0 },
        @{ n = 'top and bottom'; t = 4; side = 0; left = 150 },
        @{ n = 'behind text'; t = 5; side = 0; left = 100 },
        @{ n = 'in front of text'; t = 3; side = 0; left = 200 }
    )
    $seed = 260
    foreach ($k in $kinds) {
        $p = Add-Para $doc ("Wrap $($k.n). " + (Get-Paragraph $seed 5))
        $seed++
        # Shapes.AddPicture anchors every picture in the first paragraph whatever range it is given, so
        # the picture is put in line in its own paragraph and turned into a floating shape there.
        $at = $doc.Range($p.Range.Start, $p.Range.Start)
        $inline = $doc.InlineShapes.AddPicture("$scratch/sample.png", $false, $true, $at)
        $inline.Width = 120; $inline.Height = 72
        $s = $inline.ConvertToShape()
        $s.WrapFormat.Type = $k.t
        if ($k.t -le 2) { $s.WrapFormat.Side = $k.side }
        $s.RelativeHorizontalPosition = 0
        $s.RelativeVerticalPosition = 2
        $s.Left = $k.left
        $s.Top = 12
    }
    $p = Add-Para $doc ('A text box. ' + (Get-Paragraph $seed 5))
    $tb = $doc.Shapes.AddTextbox(1, 300, 10, 150, 80, $p.Range)
    $tb.TextFrame.TextRange.Text = 'Text inside a text box, which wraps at the box.'
    $tb.WrapFormat.Type = 0
    $tb.RelativeHorizontalPosition = 0
    $tb.RelativeVerticalPosition = 2
    $tb.Left = 318
    $tb.Top = 10
    Add-Para $doc (Get-Paragraph ($seed + 1) 4) | Out-Null
}

$fixtures['linked-textboxes'] = {
    param($word, $doc)
    $p = Add-Para $doc ('Two linked text boxes. ' + (Get-Paragraph 41 3))
    $one = $doc.Shapes.AddTextbox(1, 72, 120, 160, 70, $p.Range)
    $two = $doc.Shapes.AddTextbox(1, 320, 120, 160, 120, $p.Range)
    $one.TextFrame.Next = $two.TextFrame
    $one.TextFrame.TextRange.Text = (Get-Paragraph 42 6)
    foreach ($tb in @($one, $two)) {
        $tb.WrapFormat.Type = 3
        $tb.RelativeHorizontalPosition = 1
        $tb.RelativeVerticalPosition = 1
    }
    $one.Left = 72; $one.Top = 200
    $two.Left = 320; $two.Top = 200
    Add-Para $doc (Get-Paragraph 43 4) | Out-Null
}

$fixtures['long-table'] = {
    param($word, $doc)
    Add-Para $doc 'A long table' 'Heading 1' | Out-Null
    $t = Add-Table $doc 60 3
    $t.Rows.Item(1).HeadingFormat = -1
    $t.Rows.Item(1).Range.Font.Bold = 1
    $t.Cell(1, 1).Range.Text = 'Number'; $t.Cell(1, 2).Range.Text = 'Name'; $t.Cell(1, 3).Range.Text = 'Notes'
    for ($r = 2; $r -le 60; $r++) {
        $t.Cell($r, 1).Range.Text = "$r"
        $t.Cell($r, 2).Range.Text = "Row $r"
        $t.Cell($r, 3).Range.Text = (Get-Sentence $r)
    }
    # A cell taller than most of a page, which Word splits across pages.
    $t.Cell(12, 3).Range.Text = (Get-Paragraph 270 14)
    # A row that may not split, which moves whole to the next page.
    $t.Cell(30, 3).Range.Text = (Get-Paragraph 271 6)
    $t.Rows.Item(30).AllowBreakAcrossPages = 0
    Add-Para $doc 'After the table.' | Out-Null
}

$fixtures['keeps'] = {
    param($word, $doc)
    # Headings, a kept together paragraph and a chain of kept paragraphs, each placed near a page foot.
    $seed = 280
    foreach ($round in 0..4) {
        for ($i = 0; $i -lt (3 + $round); $i++) { Add-Para $doc (Get-Paragraph $seed (2 + ($i % 3))) | Out-Null; $seed++ }
        Add-Para $doc "Heading in round $round" 'Heading 1' | Out-Null
        Add-Para $doc (Get-Paragraph $seed 3) | Out-Null
        $seed++
    }
    $p = Add-Para $doc ('Kept together. ' + (Get-Paragraph $seed 6)); $p.Format.KeepTogether = -1; $seed++
    foreach ($i in 0..2) { $p = Add-Para $doc ("Chained $i. " + (Get-Paragraph $seed 1)); $p.Format.KeepWithNext = -1; $seed++ }
    Add-Para $doc (Get-Paragraph $seed 3) | Out-Null
    Add-Para $doc 'A heading before a table' 'Heading 2' | Out-Null
    $t = Add-Table $doc 3 2
    for ($i = 1; $i -le 3; $i++) { $t.Cell($i, 1).Range.Text = "a$i"; $t.Cell($i, 2).Range.Text = "b$i" }
    Add-Para $doc (Get-Paragraph ($seed + 1) 2) | Out-Null
}

$fixtures['table-widths'] = {
    param($word, $doc)
    $content = @('Short', 'A somewhat longer cell text', 'Mid length text', 'x', 'Another cell with several words in it', 'End')
    $make = {
        param($label)
        Add-Para $doc $label 'Heading 2' | Out-Null
        $t = Add-Table $doc 2 3
        for ($i = 0; $i -lt 6; $i++) { $t.Cell([int][math]::Floor($i / 3) + 1, ($i % 3) + 1).Range.Text = $content[$i] }
        return $t
    }
    $t = & $make 'Fixed column widths'
    $t.AllowAutoFit = $false
    $t.Columns.Item(1).Width = 80; $t.Columns.Item(2).Width = 200; $t.Columns.Item(3).Width = 120
    $t = & $make 'AutoFit to contents'
    $t.AutoFitBehavior(1)
    $t = & $make 'AutoFit to window'
    $t.AutoFitBehavior(2)
    $t = & $make 'Preferred width 60 percent'
    $t.PreferredWidthType = 2; $t.PreferredWidth = 60
    $t = & $make 'Preferred width 300 points'
    $t.PreferredWidthType = 3; $t.PreferredWidth = 300
    $t = & $make 'Cells of different widths'
    $t.Cell(2, 1).Width = 50
    $t.Cell(2, 2).Width = 250
    Add-Para $doc 'After the tables.' | Out-Null
}

$fixtures['tall-header'] = {
    param($word, $doc)
    $ps = $doc.Sections.Item(1).PageSetup
    $ps.TopMargin = 36
    $ps.HeaderDistance = 18
    $h = $doc.Sections.Item(1).Headers.Item(1)
    $h.Range.Text = "Header line one`rHeader line two`rHeader line three`rHeader line four"
    for ($i = 0; $i -lt 14; $i++) { Add-Para $doc (Get-Paragraph (300 + $i) 3) | Out-Null }
}

$fixtures['long-footnote'] = {
    param($word, $doc)
    for ($i = 0; $i -lt 6; $i++) { Add-Para $doc (Get-Paragraph (310 + $i) 4) | Out-Null }
    $p = Add-Para $doc (Get-Paragraph 317 3)
    [void]$doc.Footnotes.Add((Get-ParaEnd $doc $doc.Paragraphs.Last), $MISSING, (Get-Paragraph 318 14))
    for ($i = 0; $i -lt 6; $i++) { Add-Para $doc (Get-Paragraph (320 + $i) 4) | Out-Null }
}

# Space between paragraphs, Word's default (the larger of after and before).
$fixtures['spacing-collapse'] = {
    param($word, $doc)
    $pairs = @(@(12, 18), @(18, 12), @(6, 24), @(24, 6), @(0, 30), @(30, 0), @(10, 10))
    $seed = 330
    foreach ($pair in $pairs) {
        $p = Add-Para $doc (Get-Paragraph $seed 2); $p.Format.SpaceAfter = $pair[0]; $seed++
        $p = Add-Para $doc (Get-Paragraph $seed 2); $p.Format.SpaceBefore = $pair[1]; $seed++
    }
}

# The same with the HTML paragraph auto spacing compatibility option set.
$fixtures['spacing-sum'] = {
    param($word, $doc)
    & $fixtures['spacing-collapse'] $word $doc
    $doc.Compatibility(35) = $true
}

$fixtures['ptab-footer'] = {
    param($word, $doc)
    $f = $doc.Sections.Item(1).Footers.Item(1)
    $f.Range.Text = 'Left side'
    $r = $f.Range; $r.SetRange($r.End - 1, $r.End - 1); [void]$r.InsertAlignmentTab(1, 0)
    $r = $f.Range; $r.SetRange($r.End - 1, $r.End - 1); $r.InsertAfter('Centre text')
    $r = $f.Range; $r.SetRange($r.End - 1, $r.End - 1); [void]$r.InsertAlignmentTab(2, 0)
    $r = $f.Range; $r.SetRange($r.End - 1, $r.End - 1); $r.InsertAfter('Right side')
    $h = $doc.Sections.Item(1).Headers.Item(1)
    $h.Range.Text = 'Title'
    $r = $h.Range; $r.SetRange($r.End - 1, $r.End - 1); [void]$r.InsertAlignmentTab(2, 0)
    $r = $h.Range; $r.SetRange($r.End - 1, $r.End - 1); $r.InsertAfter('October 2026')
    Add-Para $doc (Get-Paragraph 340 3) | Out-Null
    $p = Add-Para $doc 'In the body: left'
    $r = Get-EndOf $doc $p; [void]$r.InsertAlignmentTab(1, 0)
    $doc.Paragraphs.Last.Range.InsertAfter('centre')
    $r = Get-ParaEnd $doc $doc.Paragraphs.Last; [void]$r.InsertAlignmentTab(2, 0)
    $doc.Paragraphs.Last.Range.InsertAfter('right')
}

$fixtures['notes-options'] = {
    param($word, $doc)
    # Footnotes in lower case roman, numbered again on each page; endnotes in capital letters from C, at
    # the end of each section. Word refuses a start other than 1 for numbering that restarts.
    $doc.Footnotes.NumberStyle = 2
    $doc.Footnotes.NumberingRule = 2
    $doc.Endnotes.NumberStyle = 3
    $doc.Endnotes.StartingNumber = 3
    $doc.Endnotes.Location = 0
    for ($i = 0; $i -lt 10; $i++) {
        $p = Add-Para $doc (Get-Paragraph (350 + $i) 4)
        [void]$doc.Footnotes.Add((Get-ParaEnd $doc $doc.Paragraphs.Last), $MISSING, "Footnote for paragraph $($i + 1).")
        if ($i % 4 -eq 1) { [void]$doc.Endnotes.Add((Get-ParaEnd $doc $doc.Paragraphs.Last), $MISSING, "Endnote for paragraph $($i + 1).") }
        if ($i -eq 5) { Add-SectionBreak $doc 2 }
    }
}

$fixtures['hyphenation'] = {
    param($word, $doc)
    $doc.AutoHyphenation = $true
    $doc.HyphenationZone = 18
    $doc.ConsecutiveHyphensLimit = 2
    $long = 'Internationalisation and telecommunications infrastructure demonstrate extraordinarily characteristic considerations, notwithstanding unquestionably comprehensive documentation requirements.'
    for ($i = 0; $i -lt 4; $i++) {
        $p = Add-Para $doc ($long + ' ' + (Get-Paragraph (360 + $i) 2))
        $p.Format.Alignment = 3
    }
    $p = Add-Para $doc 'A nonbreaking hyphen and a nonbreaking space:'
    $r = Get-EndOf $doc $p; $r.InsertAfter(' well'); $r = Get-ParaEnd $doc $doc.Paragraphs.Last; $r.InsertAfter([string][char]0x2011); $r = Get-ParaEnd $doc $doc.Paragraphs.Last; $r.InsertAfter('known')
    $p = Add-Para $doc ('Two columns. ' + $long + ' ' + (Get-Paragraph 365 3))
    $p.Format.Alignment = 3
}

$fixtures['controls'] = {
    param($word, $doc)
    Add-Para $doc 'Content controls' 'Heading 1' | Out-Null
    $kinds = @(@{ n = 'Rich text'; t = 0 }, @{ n = 'Plain text'; t = 1 }, @{ n = 'Combo box'; t = 3 }, @{ n = 'Drop down'; t = 4 }, @{ n = 'Date'; t = 6 }, @{ n = 'Check box'; t = 8 })
    foreach ($k in $kinds) {
        $p = Add-Para $doc "$($k.n): "
        $r = Get-EndOf $doc $p
        $c = $doc.ContentControls.Add($k.t, $r)
        $c.Title = $k.n
        $c.Tag = ($k.n -replace ' ', '').ToLower()
        if ($k.t -eq 3 -or $k.t -eq 4) { [void]$c.DropdownListEntries.Add('First choice', 'one'); [void]$c.DropdownListEntries.Add('Second choice', 'two') }
        if ($k.t -eq 0 -or $k.t -eq 1) { $c.Range.Text = "Text in the $($k.n.ToLower()) control" }
        if ($k.t -eq 8) { $c.Checked = $true }
    }
    Add-Para $doc (Get-Paragraph 370 2) | Out-Null
}

$fixtures['page-fields'] = {
    param($word, $doc)
    for ($i = 0; $i -lt 18; $i++) {
        if ($i % 6 -eq 0) {
            $p = Add-Para $doc 'This is page '
            [void](Add-FieldAtEnd $doc 33)
            $doc.Paragraphs.Last.Range.InsertAfter(' of ')
            [void](Add-FieldAtEnd $doc 26)
            $doc.Paragraphs.Last.Range.InsertAfter('.')
        }
        Add-Para $doc (Get-Paragraph (380 + $i) 4) | Out-Null
    }
}

$fixtures['columns-balance'] = {
    param($word, $doc)
    Add-Para $doc 'Balanced columns' 'Heading 1' | Out-Null
    Add-Para $doc (Get-Paragraph 400 2) | Out-Null
    Add-SectionBreak $doc 3
    Add-Para $doc (Get-Paragraph 401 3) | Out-Null
    Add-Para $doc (Get-Paragraph 402 2) | Out-Null
    Add-Para $doc (Get-Paragraph 403 3) | Out-Null
    $doc.Sections.Item(2).PageSetup.TextColumns.SetCount(2)
    $doc.Sections.Item(2).PageSetup.TextColumns.LineBetween = -1
    Add-SectionBreak $doc 3
    Add-Para $doc ('After the columns. ' + (Get-Paragraph 404 2)) | Out-Null
    $doc.Sections.Item(3).PageSetup.TextColumns.SetCount(1)
}

# One paragraph a field code and switch, Word's results the expected values for `tests/fields.rs`. Nothing
# here depends on the clock or on where the file is: DATE, TIME, FILESIZE and FILENAME \p are left out.
$fixtures['field-codes'] = {
    param($word, $doc)
    # Office's property objects answer only through InvokeMember.
    $bt = [System.__ComObject]
    foreach ($pair in @(@('Title', 'Field codes'), @('Subject', 'every switch'), @('Keywords', 'oracle fields'))) {
        $property = $bt.InvokeMember('Item', 'GetProperty', $null, $doc.BuiltInDocumentProperties, @($pair[0]))
        [void]$bt.InvokeMember('Value', 'SetProperty', $null, $property, @($pair[1]))
    }
    [void]$bt.InvokeMember('Add', 'InvokeMethod', $null, $doc.CustomDocumentProperties, @('Project', $false, 4, 'Unluminous'))
    Add-Para $doc 'Field codes' 'Heading 1' | Out-Null
    $amount = Add-Para $doc '1234.5'
    $r = $amount.Range; $r.SetRange($r.Start, $r.End - 1); [void]$doc.Bookmarks.Add('Amount', $r)
    $name = Add-Para $doc 'mixed Case words'
    $r = $name.Range; $r.SetRange($r.Start, $r.End - 1); [void]$doc.Bookmarks.Add('Name', $r)
    $codes = @(
        'PAGE', 'PAGE \* ROMAN', 'PAGE \* OrdText', 'NUMPAGES', 'NUMPAGES \* CardText', 'SECTION', 'SECTIONPAGES',
        'AUTHOR', 'AUTHOR \* Upper', 'TITLE', 'TITLE \* Lower', 'SUBJECT \* FirstCap', 'KEYWORDS \* Caps',
        'DOCPROPERTY Project', 'DOCPROPERTY Title', 'FILENAME',
        'SEQ Item', 'SEQ Item', 'SEQ Item \c', 'SEQ Item \* ROMAN', 'SEQ Item \* roman', 'SEQ Item \* ALPHABETIC',
        'SEQ Item \* alphabetic', 'SEQ Item \* Ordinal', 'SEQ Item \* CardText', 'SEQ Item \* OrdText', 'SEQ Item \* Hex',
        'SEQ Item \r 20', 'SEQ Other \* Arabic',
        '= 2+3*4', '= 10/4 \# "0.00"', '= 1234.5 \# "#,##0.00"', '= 7 \# "$#,##0.00"', '= Amount * 2',
        '= 5 > 3', 'IF 1 = 1 "yes" "no"', 'IF Amount > 1000 "big" "small"', 'COMPARE 3 > 2',
        'REF Name', 'REF Name \* Upper', 'REF Name \* FirstCap', 'STYLEREF "Heading 1"',
        'CREATEDATE \@ "yyyy"', '= 1234 \* DollarText', '= 42 \* CardText', '= 3 \* Ordinal'
    )
    foreach ($code in $codes) {
        Add-Para $doc 'Field: ' | Out-Null
        [void](Add-FieldAtEnd $doc -1 $code)
    }
    Add-Para $doc 'Table:' | Out-Null
    $table = Add-Table $doc 4 2
    $table.Cell(1, 1).Range.Text = '10'; $table.Cell(1, 2).Range.Text = '1'
    $table.Cell(2, 1).Range.Text = '20'; $table.Cell(2, 2).Range.Text = '2'
    $table.Cell(3, 1).Range.Text = '30'; $table.Cell(3, 2).Range.Text = '3'
    $cell = $table.Cell(4, 1).Range; $cell.SetRange($cell.Start, $cell.Start); [void]$doc.Fields.Add($cell, -1, '= SUM(ABOVE)', $false)
    $cell = $table.Cell(4, 2).Range; $cell.SetRange($cell.Start, $cell.Start); [void]$doc.Fields.Add($cell, -1, '= AVERAGE(ABOVE) \# "0.0"', $false)
    Add-Para $doc 'After the table.' | Out-Null
    # Saved first and repaginated, so FILENAME, NUMPAGES and SECTIONPAGES are what Word answers for the file.
    $doc.SaveAs2((Join-Path $OutDir 'field-codes.docx'), 16)
    $doc.Repaginate()
    [void]$doc.Fields.Update()
}

# Fixtures saved in another format than .docx: the extension and the WdSaveFormat.
$script:Formats = @{ 'template' = @('dotx', 14); 'macros' = @('docm', 13); 'strict' = @('docx', 24) }

# ---------- runner ----------

# Saves the document, closes it and writes the Word report for it.
function Save-AndInspect {
    param($word, $doc, [string]$name)
    $extension = 'docx'; $format = 16
    if ($script:Formats.ContainsKey($name)) { $extension = $script:Formats[$name][0]; $format = $script:Formats[$name][1] }
    $path = Join-Path $OutDir "$name.$extension"
    $doc.Repaginate()
    $doc.SaveAs2($path, $format)
    $doc.Close(0)
    $json = Join-Path $OutDir "$name.word.json"
    $proc = Start-Process pwsh -ArgumentList '-NoProfile', '-File', "$PSScriptRoot/inspect.ps1", '-Path', $path, '-Out', $json -PassThru -NoNewWindow
    if (-not $proc.WaitForExit(300000)) { Stop-Process -Id $proc.Id -Force; throw "inspect timed out for $name" }
    return $json
}

$clock = [Diagnostics.Stopwatch]::StartNew()
$handle = $null
$oldUser = $null; $oldInitials = $null
try {
    $handle = Start-WordApp -TimeoutSeconds 1800
    $word = $handle.App
    $oldUser = $word.UserName; $oldInitials = $word.UserInitials; $oldLocal = $word.Options.UseLocalUserInfo
    # Without UseLocalUserInfo Word keeps using the signed in account name for comments and revisions.
    $word.Options.UseLocalUserInfo = $true
    $word.UserName = 'Oracle'; $word.UserInitials = 'OR'
    foreach ($name in $fixtures.Keys) {
        if ($Only.Count -gt 0 -and $Only -notcontains $name) { continue }
        Write-Host ("== $name (at {0:N0}s)" -f $clock.Elapsed.TotalSeconds)
        $script:Started = $false
        $doc = $null
        try {
            $doc = $word.Documents.Add()
            & $fixtures[$name] $word $doc
            $json = Save-AndInspect $word $doc $name
            $doc = $null
            $rep = Get-Content -Raw $json | ConvertFrom-Json
            Write-Host ("   opened={0} pages={1} paragraphs={2}" -f $rep.opened, $rep.pages, $rep.paragraphCount)
        } catch {
            Write-Host "   FAILED: $($_.Exception.Message) at $($_.InvocationInfo.ScriptLineNumber)"
            if ($doc) { try { $doc.Close(0) } catch { } }
        }
    }
} finally {
    if ($word -and $oldUser) { try { $word.UserName = $oldUser; $word.UserInitials = $oldInitials; $word.Options.UseLocalUserInfo = $oldLocal } catch { } }
    Stop-WordApp $handle
}
Write-Host ("make-fixtures took {0:N1} seconds" -f $clock.Elapsed.TotalSeconds)
