# Opens a .docx read only in Word and writes what Word reports about it as JSON.
# Usage: pwsh -NoProfile -File tools/word-oracle/inspect.ps1 -Path <file.docx> -Out <out.json>
param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Out)

. "$PSScriptRoot/WordCom.ps1"
$MIXED = 9999999
$script:ThemeCache = @{}
$script:Doc = $null

# Runs a script block and returns $Default when it throws.
function Try-Get {
    param([scriptblock]$Block, $Default = $null)
    try { $v = & $Block; if ($null -eq $v) { return $Default }; return $v } catch { return $Default }
}

# Always returns an object[] so a single element or an empty list still serialises as a JSON array.
function As-Array {
    param($x)
    if ($null -eq $x) { return , (New-Object object[] 0) }
    # @() on a generic list throws in this PowerShell build, so lists are copied with ToArray().
    if ($x -is [System.Collections.Generic.List[object]]) { return , $x.ToArray() }
    if ($x -is [object[]]) { return , $x }
    $one = New-Object object[] 1
    $one[0] = $x
    return , $one
}

# Rounds a length in points to two decimals.
function Round2 { param($v) try { [math]::Round([double]$v, 2) } catch { $v } }

# Converts a Word colour (BGR integer) into "#RRGGBB" or "auto".
function Format-Color {
    param($c)
    if ($c -eq -16777216) { return 'auto' }
    if ($c -eq $MIXED) { return 'mixed' }
    $c = $c -band 0xFFFFFF
    return ('#{0:X2}{1:X2}{2:X2}' -f ($c -band 255), (($c -shr 8) -band 255), (($c -shr 16) -band 255))
}

# True when every font attribute is the same across the whole range (no "mixed" markers).
function Test-UniformFont {
    param($rng)
    $f = $rng.Font
    foreach ($v in @($f.Bold, $f.Italic, $f.Underline, $f.Size, $f.Color, $f.StrikeThrough, $f.DoubleStrikeThrough, $f.Superscript, $f.Subscript, $f.SmallCaps, $f.AllCaps, $f.Hidden, $rng.HighlightColorIndex)) {
        if ($v -eq $MIXED) { return $false }
    }
    return ($f.Name -ne '')
}

# Font colour as a BGR integer; theme colours are resolved through TextColor.RGB.
function Get-RealColor {
    param($f)
    $c = $f.Color
    if ($c -eq -16777216 -or $c -eq $MIXED) { return $c }
    try {
        $tc = $f.TextColor
        if ($tc.Type -eq 2) { return (Resolve-ThemeColor $tc.ObjectThemeColor $tc.TintAndShade) }
        if ($tc.RGB -ge 0) { return $tc.RGB }
    } catch { }
    return $c
}

# Resolves a Word theme colour index (WdThemeColorIndex) plus tint to a BGR integer using the document's theme.
function Resolve-ThemeColor {
    param([int]$themeIndex, [double]$tint)
    $key = "$themeIndex/$tint"
    if ($script:ThemeCache.ContainsKey($key)) { return $script:ThemeCache[$key] }
    $map = @{ 12 = 2; 13 = 1; 14 = 4; 15 = 3 }
    $mso = $(if ($map.ContainsKey($themeIndex)) { $map[$themeIndex] } else { $themeIndex + 1 })
    $bgr = [int]$script:Doc.DocumentTheme.ThemeColorScheme.Colors($mso).RGB
    $ch = @(($bgr -band 255), (($bgr -shr 8) -band 255), (($bgr -shr 16) -band 255))
    $ch = $ch | ForEach-Object { if ($tint -lt 0) { [int][math]::Round($_ * (1 + $tint)) } else { [int][math]::Round($_ + (255 - $_) * $tint) } }
    $result = $ch[0] + 256 * $ch[1] + 65536 * $ch[2]
    $script:ThemeCache[$key] = $result
    return $result
}

# Reads the character formatting of a range that is known to be uniform.
function Read-FontSnapshot {
    param($rng)
    $f = $rng.Font
    [ordered]@{
        font = $f.Name; size = (Round2 $f.Size); bold = ($f.Bold -ne 0); italic = ($f.Italic -ne 0)
        underline = [int]$f.Underline; color = (Format-Color (Get-RealColor $f)); highlight = [int]$rng.HighlightColorIndex
        strike = ($f.StrikeThrough -ne 0); doubleStrike = ($f.DoubleStrikeThrough -ne 0)
        superscript = ($f.Superscript -ne 0); subscript = ($f.Subscript -ne 0)
        smallCaps = ($f.SmallCaps -ne 0); allCaps = ($f.AllCaps -ne 0); hidden = ($f.Hidden -ne 0)
    }
}

# Formatting snapshot of a range plus its text.
function Read-Piece {
    param($rng)
    $snap = Read-FontSnapshot $rng
    $snap['text'] = $rng.Text
    return $snap
}

# True when two snapshots have the same formatting values (text is ignored).
function Test-SameFormat {
    param($a, $b)
    foreach ($k in $a.Keys) { if ($k -ne 'text' -and $a[$k] -ne $b[$k]) { return $false } }
    return $true
}

# Splits a range into runs of identical formatting by halving until each piece is uniform.
function Get-RunPieces {
    param($doc, [int]$start, [int]$end)
    $rng = $doc.Range($start, $end)
    if (($end - $start) -le 1) { return , @(Read-Piece $rng) }
    if (Test-UniformFont $rng) {
        # Word ignores leading and trailing spaces when it reports mixed formatting, so check the edge characters too.
        $snap = Read-Piece $rng
        $first = Read-FontSnapshot $doc.Range($start, $start + 1)
        $last = Read-FontSnapshot $doc.Range($end - 1, $end)
        if ((Test-SameFormat $snap $first) -and (Test-SameFormat $snap $last)) { return , @($snap) }
    }
    $mid = [int](($start + $end) / 2)
    $left = Get-RunPieces $doc $start $mid
    $right = Get-RunPieces $doc $mid $end
    return , @($left + $right)
}

# Merges neighbouring pieces that have identical formatting into single runs.
function Merge-Runs {
    param($pieces)
    $runs = New-Object System.Collections.Generic.List[object]
    foreach ($p in $pieces) {
        if ($runs.Count -gt 0) {
            $last = $runs[$runs.Count - 1]
            $same = $true
            foreach ($k in $p.Keys) { if ($k -ne 'text' -and $last[$k] -ne $p[$k]) { $same = $false; break } }
            if ($same) { $last['text'] += $p['text']; continue }
        }
        $runs.Add($p)
    }
    return , $runs
}

# Builds the runs list for one paragraph (null for very long paragraphs).
function Get-ParagraphRuns {
    param($doc, $para)
    $r = $para.Range
    $end = $r.End - 1
    if (($end - $r.Start) -gt 400) { return $null }
    if ($end -le $r.Start) { return , @() }
    $pieces = Get-RunPieces $doc $r.Start $end
    return , (Merge-Runs $pieces)
}

# Runs of a paragraph as an array, or null when they could not be read or the paragraph is long.
function Get-RunsSafe {
    param($doc, $para)
    try {
        $r = Get-ParagraphRuns $doc $para
        if ($null -eq $r) { return $null }
        return As-Array $r
    } catch { return $null }
}

# The lines Word broke a paragraph into: each line's text and the top of its first character, read with
# the predefined \Line bookmark at the start of each line. Null when the document is too long to ask.
function Get-Lines {
    param($doc, $para)
    if (-not $script:AskLines) { return $null }
    $list = New-Object System.Collections.Generic.List[object]
    try {
        $r = $para.Range
        $sel = $doc.ActiveWindow.Selection
        $pos = $r.Start
        $guard = 0
        while ($pos -lt $r.End -and $guard -lt 400) {
            $guard++
            $sel.SetRange($pos, $pos)
            $line = $doc.Bookmarks.Item('\Line').Range
            if ($line.End -le $pos) { break }
            $end = [math]::Min($line.End, $r.End)
            $text = ([string]$doc.Range($pos, $end).Text) -replace "\r$", '' -replace "\u0007", ''
            $list.Add([ordered]@{ text = $text; top = (Round2 ($doc.Range($pos, $pos).Information(6))); left = (Round2 ($doc.Range($pos, $pos).Information(5))); page = [int]$doc.Range($pos, $pos).Information(3) })
            $pos = $end
        }
    } catch { return $null }
    return , $list.ToArray()
}

# Reads one paragraph into the report shape.
function Read-Paragraph {
    param($doc, $para, [int]$index)
    $r = $para.Range
    $f = $para.Format
    $listType = Try-Get { $r.ListFormat.ListType } 0
    $text = ([string]$r.Text) -replace "\r$", '' -replace "\u0007", ''
    [ordered]@{
        index = $index
        text = $text
        style = (Try-Get { $para.Style.NameLocal } '')
        listString = (Try-Get { [string]$r.ListFormat.ListString } '')
        listLevel = $(if ($listType -ne 0) { Try-Get { [int]$r.ListFormat.ListLevelNumber } 0 } else { 0 })
        alignment = (Try-Get { [int]$f.Alignment } 0)
        leftIndent = (Round2 (Try-Get { $f.LeftIndent } 0)); rightIndent = (Round2 (Try-Get { $f.RightIndent } 0))
        firstLineIndent = (Round2 (Try-Get { $f.FirstLineIndent } 0))
        spaceBefore = (Round2 (Try-Get { $f.SpaceBefore } 0)); spaceAfter = (Round2 (Try-Get { $f.SpaceAfter } 0))
        lineSpacing = (Round2 (Try-Get { $f.LineSpacing } 0)); lineSpacingRule = (Try-Get { [int]$f.LineSpacingRule } 0)
        keepWithNext = (Try-Get { $f.KeepWithNext -ne 0 } $false); keepTogether = (Try-Get { $f.KeepTogether -ne 0 } $false)
        pageBreakBefore = (Try-Get { $f.PageBreakBefore -ne 0 } $false); outlineLevel = (Try-Get { [int]$f.OutlineLevel } 10)
        page = (Try-Get { [int]$doc.Range($r.Start, $r.Start).Information(3) } 0)
        inTable = (Try-Get { [bool]$r.Information(12) } $false)
        lineCount = (Try-Get { [int]$r.ComputeStatistics(1) } 0)
        top = (Round2 (Try-Get { $doc.Range($r.Start, $r.Start).Information(6) } 0))
        left = (Round2 (Try-Get { $doc.Range($r.Start, $r.Start).Information(5) } 0))
        lastTop = (Round2 (Try-Get { $e = [math]::Max($r.Start, $r.End - 1); $doc.Range($e, $e).Information(6) } 0))
        runs = (Get-RunsSafe $doc $para)
        lines = (Get-Lines $doc $para)
    }
}

# Reads every paragraph of the document body.
function Read-Paragraphs {
    param($doc)
    $list = New-Object System.Collections.Generic.List[object]
    $i = 0
    foreach ($p in $doc.Paragraphs) { $i++; $list.Add((Read-Paragraph $doc $p $i)) }
    return , $list
}

# Reads one table (and, recursively, tables nested inside it).
function Read-Table {
    param($tbl, $list)
    $cells = New-Object System.Collections.Generic.List[object]
    foreach ($c in $tbl.Range.Cells) {
        $t = ([string](Try-Get { $c.Range.Text } '')) -replace "\r?\u0007$", '' -replace "\r$", ''
        $cells.Add([ordered]@{ row = (Try-Get { $c.RowIndex } 0); column = (Try-Get { $c.ColumnIndex } 0); text = $t; width = (Round2 (Try-Get { $c.Width } 0)) })
    }
    $list.Add([ordered]@{
        rows = (Try-Get { $tbl.Rows.Count } -1); columns = (Try-Get { $tbl.Columns.Count } -1)
        nested = (Try-Get { [int]$tbl.NestingLevel } 1); cells = $cells
    })
    foreach ($n in (Try-Get { $tbl.Tables } @())) { Read-Table $n $list }
}

# Reads header or footer text for the three kinds (1 primary, 2 first page, 3 even pages).
function Read-HeaderFooter {
    param($collection)
    $names = @{ 1 = 'primary'; 2 = 'first'; 3 = 'even' }
    $o = [ordered]@{}
    foreach ($k in 1, 2, 3) {
        $o[$names[$k]] = Try-Get { $h = $collection.Item($k); if ($h.Exists) { ([string]$h.Range.Text) -replace "\r$", '' } else { '' } } ''
        $o[$names[$k] + 'Pictures'] = Try-Get { $h = $collection.Item($k); if ($h.Exists) { [int]$h.Range.InlineShapes.Count } else { 0 } } 0
        $o[$names[$k] + 'Tables'] = Try-Get { $h = $collection.Item($k); if ($h.Exists) { [int]$h.Range.Tables.Count } else { 0 } } 0
    }
    return $o
}

# Reads page setup and headers/footers for each section.
function Read-Sections {
    param($doc)
    $list = New-Object System.Collections.Generic.List[object]
    foreach ($s in $doc.Sections) {
        $ps = $s.PageSetup
        $list.Add([ordered]@{
            pageWidth = (Round2 (Try-Get { $ps.PageWidth } 0)); pageHeight = (Round2 (Try-Get { $ps.PageHeight } 0))
            orientation = (Try-Get { [int]$ps.Orientation } 0)
            topMargin = (Round2 (Try-Get { $ps.TopMargin } 0)); bottomMargin = (Round2 (Try-Get { $ps.BottomMargin } 0))
            leftMargin = (Round2 (Try-Get { $ps.LeftMargin } 0)); rightMargin = (Round2 (Try-Get { $ps.RightMargin } 0))
            headerDistance = (Round2 (Try-Get { $ps.HeaderDistance } 0)); footerDistance = (Round2 (Try-Get { $ps.FooterDistance } 0))
            columns = (Try-Get { [int]$ps.TextColumns.Count } 1)
            differentFirstPage = (Try-Get { $ps.DifferentFirstPageHeaderFooter -ne 0 } $false)
            oddAndEvenPages = (Try-Get { $ps.OddAndEvenPagesHeaderFooter -ne 0 } $false)
            sectionStart = (Try-Get { [int]$ps.SectionStart } 0)
            headers = (Read-HeaderFooter $s.Headers); footers = (Read-HeaderFooter $s.Footers)
        })
    }
    return , $list
}

# Maps every item of a COM collection through a scriptblock, skipping items that throw.
function Map-Collection {
    param($collection, [scriptblock]$Block)
    $list = New-Object System.Collections.Generic.List[object]
    try { foreach ($item in $collection) { try { $list.Add((& $Block $item)) } catch { } } } catch { }
    return , $list
}

# Reads a built in document property by name through late binding.
function Read-BuiltIn {
    param($doc, [string]$name)
    try {
        $bt = [System.__ComObject]
        $props = $doc.BuiltInDocumentProperties
        $p = $bt.InvokeMember('Item', 'GetProperty', $null, $props, @($name))
        $v = $bt.InvokeMember('Value', 'GetProperty', $null, $p, $null)
        return [string]$v
    } catch { return '' }
}

# Collects every table in the document, including nested ones.
function Read-Tables {
    param($doc)
    $list = New-Object System.Collections.Generic.List[object]
    foreach ($t in $doc.Tables) { Read-Table $t $list }
    return , $list
}

# Builds the whole report for an open document.
function Get-Report {
    param($doc, [string]$fileName)
    [ordered]@{
        file = $fileName; opened = $true; error = $null
        pages = (Try-Get { $doc.ComputeStatistics(2) } 0); words = (Try-Get { $doc.ComputeStatistics(0) } 0)
        characters = (Try-Get { $doc.ComputeStatistics(3) } 0); charactersWithSpaces = (Try-Get { $doc.ComputeStatistics(5) } 0)
        paragraphCount = (Try-Get { $doc.ComputeStatistics(4) } 0); lines = (Try-Get { $doc.ComputeStatistics(1) } 0)
        paragraphs = (Read-Paragraphs $doc)
        tables = (As-Array (Read-Tables $doc))
        sections = (Read-Sections $doc)
        footnotes = (Map-Collection $doc.Footnotes { param($n) ([string]$n.Range.Text) })
        endnotes = (Map-Collection $doc.Endnotes { param($n) ([string]$n.Range.Text) })
        comments = (Map-Collection $doc.Comments { param($c) [ordered]@{ author = [string]$c.Author; text = [string]$c.Range.Text; scope = (Try-Get { [string]$c.Scope.Text } ''); done = (Try-Get { [bool]$c.Done } $false) } })
        revisions = (Map-Collection $doc.Revisions { param($r) [ordered]@{ type = [int]$r.Type; author = [string]$r.Author; text = (Try-Get { [string]$r.Range.Text } '') } })
        inlineShapes = (Map-Collection $doc.InlineShapes { param($s) [ordered]@{ width = (Round2 $s.Width); height = (Round2 $s.Height); type = [int]$s.Type } })
        shapes = (Map-Collection $doc.Shapes { param($s) [ordered]@{ type = [int]$s.Type; width = (Round2 $s.Width); height = (Round2 $s.Height); left = (Round2 $s.Left); top = (Round2 $s.Top); wrap = (Try-Get { [int]$s.WrapFormat.Type } -1); side = (Try-Get { [int]$s.WrapFormat.Side } -1); text = (Try-Get { if ($s.TextFrame.HasText) { [string]$s.TextFrame.TextRange.Text } else { '' } } ''); page = (Try-Get { [int]$s.Anchor.Information(3) } 0); autoShape = (Try-Get { [int]$s.AutoShapeType } 0) } })
        contentControls = (Map-Collection $doc.ContentControls { param($c) [ordered]@{ type = [int]$c.Type; title = [string]$c.Title; tag = [string]$c.Tag; text = (Try-Get { [string]$c.Range.Text } ''); checked = (Try-Get { [bool]$c.Checked } $false) } })
        oMaths = (Map-Collection $doc.OMaths { param($m) [ordered]@{ text = (Try-Get { [string]$m.Range.Text } ''); type = (Try-Get { [int]$m.Type } 0) } })
        settings = [ordered]@{ autoHyphenation = (Try-Get { [bool]$doc.AutoHyphenation } $false); hyphenationZone = (Round2 (Try-Get { $doc.HyphenationZone } 0)); consecutiveHyphensLimit = (Try-Get { [int]$doc.ConsecutiveHyphensLimit } 0); compatibilityMode = (Try-Get { [int]$doc.CompatibilityMode } 0) }
        footnoteOptions = [ordered]@{ numberStyle = (Try-Get { [int]$doc.Footnotes.NumberStyle } 0); numberingRule = (Try-Get { [int]$doc.Footnotes.NumberingRule } 0); startingNumber = (Try-Get { [int]$doc.Footnotes.StartingNumber } 0); location = (Try-Get { [int]$doc.Footnotes.Location } 0) }
        endnoteOptions = [ordered]@{ numberStyle = (Try-Get { [int]$doc.Endnotes.NumberStyle } 0); numberingRule = (Try-Get { [int]$doc.Endnotes.NumberingRule } 0); startingNumber = (Try-Get { [int]$doc.Endnotes.StartingNumber } 0); location = (Try-Get { [int]$doc.Endnotes.Location } 0) }
        footnotePages = (Map-Collection $doc.Footnotes { param($n) [ordered]@{ reference = (Try-Get { [int]$n.Reference.Information(3) } 0); first = (Try-Get { [int]$doc.Range($n.Range.Start, $n.Range.Start).Information(3) } 0); last = (Try-Get { $e = [math]::Max($n.Range.Start, $n.Range.End - 1); [int]$doc.Range($e, $e).Information(3) } 0); mark = (Try-Get { [string]$n.Reference.Text } '') } })
        fields = (Map-Collection $doc.Fields { param($f) [ordered]@{ type = [int]$f.Type; code = (Try-Get { [string]$f.Code.Text } ''); result = (Try-Get { [string]$f.Result.Text } '') } })
        tablesOfContents = (Try-Get { [int]$doc.TablesOfContents.Count } 0)
        bookmarks = (Map-Collection $doc.Bookmarks { param($b) [string]$b.Name })
        hyperlinks = (Map-Collection $doc.Hyperlinks { param($h) [ordered]@{ address = [string]$h.Address; subAddress = [string]$h.SubAddress; text = [string]$h.TextToDisplay } })
        properties = [ordered]@{ title = (Read-BuiltIn $doc 'Title'); subject = (Read-BuiltIn $doc 'Subject'); author = (Read-BuiltIn $doc 'Author'); keywords = (Read-BuiltIn $doc 'Keywords') }
    }
}

# Writes the report as UTF-8 JSON without a byte order mark.
function Write-Report {
    param($report, [string]$outPath)
    $json = $report | ConvertTo-Json -Depth 20
    [IO.File]::WriteAllText($outPath, $json, (New-Object Text.UTF8Encoding($false)))
}

$fullPath = (Resolve-Path -LiteralPath $Path).Path
# Asking for every line is slow, so it is done for documents of a few hundred paragraphs or fewer.
$script:AskLines = $true
$outPath = [IO.Path]::GetFullPath($Out)
$handle = $null
try {
    $handle = Start-WordApp
    $word = $handle.App
    try {
        $doc = $word.Documents.Open($fullPath, $false, $true, $false)
    } catch {
        $report = [ordered]@{ file = [IO.Path]::GetFileName($fullPath); opened = $false; error = $_.Exception.Message }
        Write-Report $report $outPath
        return
    }
    $script:Doc = $doc
    try { if ($doc.Paragraphs.Count -gt 400) { $script:AskLines = $false } } catch { }
    # Positions are only right in Print Layout view.
    try { $doc.ActiveWindow.View.Type = 3 } catch { try { $doc.Windows.Item(1).View.Type = 3 } catch { } }
    try { Write-Report (Get-Report $doc ([IO.Path]::GetFileName($fullPath))) $outPath }
    finally { try { $doc.Close(0) } catch { } }
} finally {
    Stop-WordApp $handle
}
