# Run data tool scenarios in Excel and write what Excel did, as JSON.
#
# A scenario file is a JSON array. Each scenario names the cells to fill, optional formats, optional setup
# steps and one operation (sort, filter, advanced-filter, remove-duplicates, text-to-columns, consolidate or a
# table operation). The script performs them in a new workbook through COM, saves a copy, and dumps what
# changed: the cells of every sheet (value, formula, number format, colours), the hidden rows, the tables, the
# defined names and the XML Excel wrote for the sheet filters and the tables. The Rust test
# `tests/datatools.rs` performs the same steps with the engine and compares.
#
#   pwsh tools/excel-oracle/data-tools.ps1 -Scenarios scenarios.json -Out expected.json [-Only name] [-Resume]
#
# Each answer is also appended to `<Out>.partial` as it is made; if Excel stops answering the script starts it
# again and carries on, and -Resume continues an interrupted run from the partial file.
param(
    [Parameter(Mandatory = $true)][string]$Scenarios,
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Only,
    [switch]$Resume
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
Add-Type -AssemblyName System.IO.Compression.FileSystem
$missing = [Type]::Missing

function Bgr([string]$hex) {
    $r = [Convert]::ToInt32($hex.Substring(0, 2), 16); $g = [Convert]::ToInt32($hex.Substring(2, 2), 16); $b = [Convert]::ToInt32($hex.Substring(4, 2), 16)
    return $r + 256 * $g + 65536 * $b
}
function Has($o, [string]$name) { return $null -ne $o -and $o.PSObject.Properties.Name -contains $name }
function Opt($o, [string]$name, $default) { if (Has $o $name) { return $o.$name } else { return $default } }
function Yn($text) { switch ($text) { 'yes' { 1 } 'no' { 2 } default { 0 } } }

function Sheet($book, [string]$name) {
    foreach ($s in $book.Worksheets) { if ($s.Name -eq $name) { return $s } }
    $s = $book.Worksheets.Add([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count))
    $s.Name = $name
    return $s
}

function Fill-Cells($book, $sheets) {
    # Every sheet exists before any cell is filled, so a formula may name a sheet that comes later.
    foreach ($p in $sheets.PSObject.Properties) { $null = Sheet $book $p.Name }
    foreach ($p in $sheets.PSObject.Properties) {
        $ws = Sheet $book $p.Name
        foreach ($c in $p.Value.PSObject.Properties) {
            $v = $c.Value
            Put-Cell $ws $c.Name $v
        }
    }
}

# Numbers are written through their text so a very large or very small one is read the way a person types it.
function Put-Cell($ws, [string]$at, $v) {
    if ($null -eq $v) { $null = $ws.Range($at).ClearContents() }
    elseif ($v -is [string]) { $ws.Range($at).Formula = $v }
    elseif ($v -is [bool]) { $ws.Range($at).Value2 = $v }
    else { $ws.Range($at).Formula = ([double]$v).ToString('R', [Globalization.CultureInfo]::InvariantCulture) }
}

function Apply-Formats($book, $formats) {
    foreach ($f in $formats) {
        $ws = Sheet $book (Opt $f 'sheet' 'Sheet1')
        $r = $ws.Range($f.range)
        if (Has $f 'bold') { $r.Font.Bold = $f.bold }
        if (Has $f 'italic') { $r.Font.Italic = $f.italic }
        if (Has $f 'underline') { $r.Font.Underline = $(if ($f.underline) { 2 } else { -4142 }) }
        if (Has $f 'fill') { $r.Interior.Color = (Bgr $f.fill) }
        if (Has $f 'font') { $r.Font.Color = (Bgr $f.font) }
        if (Has $f 'size') { $r.Font.Size = $f.size }
        if (Has $f 'name') { $r.Font.Name = $f.name }
        if (Has $f 'numfmt') { $r.NumberFormat = $f.numfmt }
        if (Has $f 'halign') { $r.HorizontalAlignment = @{ left = -4131; center = -4108; right = -4152 }[$f.halign] }
        if (Has $f 'wrap') { $r.WrapText = $f.wrap }
        if (Has $f 'borderBottom') { $r.Borders(9).LineStyle = 1 }
        if (Has $f 'merge') { $r.Merge() }
        if (Has $f 'icons') {
            # Conditional icon set: 3 traffic lights on the range.
            $ic = $r.FormatConditions.AddIconSetCondition()
            $ic.IconSet = $book.Application.ActiveWorkbook.IconSets.Item([int]$f.icons)
        }
    }
}

function Table-Of($book, [string]$name) {
    foreach ($s in $book.Worksheets) { foreach ($t in $s.ListObjects) { if ($t.Name -eq $name) { return $t } } }
    throw "no table $name"
}

function Run-Step($book, $step) {
    $ws = Sheet $book (Opt $step 'sheet' 'Sheet1')
    switch ($step.kind) {
        'set' {
            foreach ($c in $step.cells.PSObject.Properties) { Put-Cell $ws $c.Name $c.Value }
        }
        'format' { Apply-Formats $book @($step) }
        'note' { $null = $ws.Range($step.cell).AddComment($step.text) }
        'hide-rows' { $ws.Range($step.range).EntireRow.Hidden = $true }
        'table' {
            $lo = $ws.ListObjects.Add(1, $ws.Range($step.range), $missing, $(if ((Opt $step 'headers' $true)) { 1 } else { 2 }))
            if (Has $step 'name') { $lo.Name = $step.name }
            if (Has $step 'style') { $lo.TableStyle = $step.style }
        }
        'table-style-name' { (Table-Of $book $step.table).TableStyle = $step.style }
        'filter-on' { $null = $ws.Range($step.range).AutoFilter() }
        'filter-off' { if ($ws.AutoFilterMode) { $ws.AutoFilterMode = $false } }
        'filter' {
            if (Has $step 'table') { $rng = (Table-Of $book $step.table).Range } else { $rng = $ws.Range($step.range) }
            $c1 = $missing
            if (Has $step 'criteria1') {
                $c1 = $step.criteria1
                if ($c1 -is [System.Array]) { if ($c1.Count -eq 0) { $c1 = $missing } else { $c1 = [string[]]$c1 } } elseif ($c1 -is [long]) { $c1 = [int]$c1 }
                elseif ((Has $step 'color') -and $step.color) { $c1 = Bgr $c1 }
            }
            if (Has $step 'iconSet') { $c1 = $book.Application.ActiveWorkbook.IconSets.Item([int]$step.iconSet).Item([int]$step.iconId) }
            if (Has $step 'noIcon') { $c1 = 14 }
            $op = $missing
            if (Has $step 'operator') { $op = [int]$step.operator }
            $c2 = $missing
            if (Has $step 'criteria2') {
                $c2 = $step.criteria2
                if ($c2 -is [System.Array]) { $c2 = [object[]]($c2 | ForEach-Object { if ($_ -is [long]) { [int]$_ } else { $_ } }) }
            }
            $null = $rng.AutoFilter([int]$step.field, $c1, $op, $c2)
        }
        'reapply' { $ws.AutoFilter.ApplyFilter() }
        'show-all' { if ($ws.FilterMode) { $ws.ShowAllData() } }
        'sort' { Run-Sort $book $ws $step }
        'advanced-filter' {
            $copy = $missing; $crit = $missing; $action = 1
            if (Has $step 'copyTo') { $copy = $ws.Range($step.copyTo); $action = 2 }
            if (Has $step 'criteria') { $crit = $ws.Range($step.criteria) }
            $null = $ws.Range($step.range).AdvancedFilter($action, $crit, $copy, [bool](Opt $step 'unique' $false))
        }
        'remove-duplicates' {
            $cols = [object[]]($step.columns | ForEach-Object { [int]$_ })
            $null = $ws.Range($step.range).RemoveDuplicates($cols, (Yn (Opt $step 'header' 'no')))
        }
        'text-to-columns' {
            $dest = if (Has $step 'destination') { $ws.Range($step.destination) } else { $missing }
            $fixed = (Opt $step 'type' 'delimited') -eq 'fixed'
            $q = switch (Opt $step 'qualifier' 'none') { 'double' { 1 } 'single' { 2 } default { -4142 } }
            $info = $missing
            if (Has $step 'fieldInfo') {
                $rows = @(); foreach ($fi in $step.fieldInfo) { $rows += , @([int]$fi[0], [int]$fi[1]) }
                $info = [object[]]$rows
            }
            $null = $ws.Range($step.range).TextToColumns($dest, $(if ($fixed) { 2 } else { 1 }), $q, [bool](Opt $step 'consecutive' $false), [bool](Opt $step 'tab' $false), [bool](Opt $step 'semicolon' $false), [bool](Opt $step 'comma' $false), [bool](Opt $step 'space' $false), [bool](Opt $step 'other' $false), (Opt $step 'otherChar' $missing), $info)
        }
        'consolidate' {
            $fn = @{ sum = -4157; count = -4112; average = -4106; max = -4136; min = -4139; product = -4149; countnums = -4113; stdev = -4155; stdevp = -4156; var = -4164; varp = -4165 }[(Opt $step 'function' 'sum')]
            $src = [string[]]$step.sources
            $null = $ws.Range($step.destination).Consolidate($src, $fn, [bool](Opt $step 'topRow' $false), [bool](Opt $step 'leftColumn' $false), [bool](Opt $step 'links' $false))
        }
        'table-resize' { (Table-Of $book $step.table).Resize($ws.Range($step.range)) }
        'table-unlist' { (Table-Of $book $step.table).Unlist() }
        'table-totals' { (Table-Of $book $step.table).ShowTotals = [bool]$step.show }
        'table-header' { (Table-Of $book $step.table).ShowHeaders = [bool]$step.show }
        'table-total-fn' {
            $t = Table-Of $book $step.table
            $fnn = @{ none = 0; sum = 1; average = 2; count = 3; countnums = 4; min = 5; max = 6; stddev = 7; var = 8; custom = 9 }[$step.function]
            $t.ListColumns.Item($step.column).TotalsCalculation = $fnn
        }
        'table-total-label' { (Table-Of $book $step.table).ListColumns.Item($step.column).Range.Cells.Item((Table-Of $book $step.table).Range.Rows.Count).Value2 = $step.text }
        'table-style' {
            $t = Table-Of $book $step.table
            if (Has $step 'style') { $t.TableStyle = $step.style }
            if (Has $step 'rowStripes') { $t.ShowTableStyleRowStripes = [bool]$step.rowStripes }
            if (Has $step 'columnStripes') { $t.ShowTableStyleColumnStripes = [bool]$step.columnStripes }
            if (Has $step 'firstColumn') { $t.ShowTableStyleFirstColumn = [bool]$step.firstColumn }
            if (Has $step 'lastColumn') { $t.ShowTableStyleLastColumn = [bool]$step.lastColumn }
            if (Has $step 'filterButton') { $t.ShowAutoFilterDropDown = [bool]$step.filterButton }
        }
        'table-rename' { (Table-Of $book $step.table).Name = $step.name }
        'table-column-name' { (Table-Of $book $step.table).ListColumns.Item($step.column).Name = $step.name }
        'table-add-column' { $c = (Table-Of $book $step.table).ListColumns.Add(); if (Has $step 'name') { $c.Name = $step.name } }
        'type-in-table' {
            # Type a formula into one cell of a table column, as a person does, with auto fill on.
            $excel = $book.Application
            $excel.AutoCorrect.AutoFillFormulasInLists = [bool](Opt $step 'autofill' $true)
            Put-Cell $ws $step.cell $step.formula
            $excel.AutoCorrect.AutoFillFormulasInLists = $true
        }
        'delete-table-rows' { $null = (Table-Of $book $step.table).ListRows.Item($step.row).Delete() }
        'add-table-row' { $null = (Table-Of $book $step.table).ListRows.Add() }
        'recalculate' { $book.Application.CalculateFull() }
        default { throw "unknown step kind $($step.kind)" }
    }
}

# The Sort object never guesses a header (xlGuess means no header there), but Range.Sort does, and so does
# the Sort dialog, so a guessing sort goes through Range.Sort with up to three value keys.
function Run-GuessSort($ws, $step) {
    $rng = $ws.Range($step.range)
    $keys = @($step.levels)
    $k1 = $ws.Range($keys[0].key); $o1 = $(if ((Opt $keys[0] 'order' 'asc') -eq 'desc') { 2 } else { 1 })
    $k2 = $missing; $o2 = 1; $k3 = $missing; $o3 = 1
    if ($keys.Count -gt 1) { $k2 = $ws.Range($keys[1].key); $o2 = $(if ((Opt $keys[1] 'order' 'asc') -eq 'desc') { 2 } else { 1 }) }
    if ($keys.Count -gt 2) { $k3 = $ws.Range($keys[2].key); $o3 = $(if ((Opt $keys[2] 'order' 'asc') -eq 'desc') { 2 } else { 1 }) }
    $orient = $(if ((Opt $step 'orientation' 'rows') -eq 'columns') { 2 } else { 1 })
    $null = $rng.Sort($k1, $o1, $k2, $missing, $o2, $k3, $o3, 0, $missing, [bool](Opt $step 'matchCase' $false), $orient)
}

function Run-Sort($book, $ws, $step) {
    if ((Opt $step 'header' 'guess') -eq 'guess' -and -not (Has $step 'table') -and (Opt $step 'via' '') -ne 'filter') { Run-GuessSort $ws $step; return }
    $sort = $ws.Sort
    $own = $false
    if (Has $step 'table') { $sort = (Table-Of $book $step.table).Sort; $own = $true }
    elseif ((Opt $step 'via' '') -eq 'filter') { $sort = $ws.AutoFilter.Sort; $own = $true }
    $sort.SortFields.Clear()
    $rng = $null
    if (Has $step 'range') { $rng = $ws.Range($step.range) }
    foreach ($lv in $step.levels) {
        $key = $ws.Range($lv.key)
        $on = switch (Opt $lv 'on' 'values') { 'values' { 0 } 'cellcolor' { 1 } 'fontcolor' { 2 } 'icon' { 3 } }
        $order = if ((Opt $lv 'order' 'asc') -eq 'desc') { 2 } else { 1 }
        $custom = $missing
        if (Has $lv 'custom') { $custom = $lv.custom }
        $f = $sort.SortFields.Add2($key, $on, $order, $custom, 0)
        if ($on -eq 1 -or $on -eq 2) {
            if (Has $lv 'color') { $f.SortOnValue.Color = (Bgr $lv.color) }
            else { $f.SortOnValue.ColorIndex = -4142 }
        }
        if ($on -eq 3) { $f.SortOnValue.Icon = $book.Application.ActiveWorkbook.IconSets.Item([int]$lv.iconSet).Item([int]$lv.iconId) }
    }
    if (-not $own) {
        $sort.SetRange($rng)
        $sort.Header = (Yn (Opt $step 'header' 'guess'))
        $sort.Orientation = $(if ((Opt $step 'orientation' 'rows') -eq 'columns') { 2 } else { 1 })
    }
    $sort.MatchCase = [bool](Opt $step 'matchCase' $false)
    $sort.Apply()
}

function Col-Name([int]$n) {
    $s = ''
    while ($n -gt 0) { $m = ($n - 1) % 26; $s = [char](65 + $m) + $s; $n = [math]::Floor(($n - 1) / 26) }
    return $s
}

function Dump-Sheet($ws) {
    $cells = @()
    $used = $ws.UsedRange
    $row0 = $used.Row; $col0 = $used.Column
    $lastRow = $row0 + $used.Rows.Count - 1
    $lastCol = $col0 + $used.Columns.Count - 1
    $nr = $used.Rows.Count; $nc = $used.Columns.Count
    $values = $used.Value2; $formulas = $used.Formula; $texts = $used.Text
    $deep = [bool]$script:dumpFormat
    for ($i = 1; $i -le $nr; $i++) {
        for ($j = 1; $j -le $nc; $j++) {
            if ($nr -eq 1 -and $nc -eq 1) { $value = $values; $formula = $formulas } else { $value = $values[$i, $j]; $formula = $formulas[$i, $j] }
            $cell = $null
            $nf = 'General'; $fill = ''; $fontc = ''; $bold = $false
            if ($deep -or $null -ne $value -or $formula -ne '') {
                $cell = $ws.Cells.Item($row0 + $i - 1, $col0 + $j - 1)
                $nf = $cell.NumberFormat
            }
            if ($deep) {
                if ($cell.Interior.ColorIndex -ne -4142) { $fill = '{0:X6}' -f [int]$cell.Interior.Color }
                $fontc = '{0:X6}' -f [int]$cell.Font.Color
                $bold = [bool]$cell.Font.Bold
            }
            if ($null -eq $value -and $formula -eq '' -and $fill -eq '' -and $nf -eq 'General') { continue }
            $kind = if ($null -eq $value) { 'empty' } elseif ($value -is [double]) { 'number' } elseif ($value -is [bool]) { 'bool' } elseif ($value -is [int]) { 'error' } else { 'text' }
            if ($kind -eq 'number') { $shown = $value.ToString('R', [Globalization.CultureInfo]::InvariantCulture) }
            elseif ($kind -eq 'error') { $shown = $ws.Cells.Item($row0 + $i - 1, $col0 + $j - 1).Text }
            else { $shown = "$value" }
            $isF = ($formula -is [string]) -and $formula.StartsWith('=')
            $addr = (Col-Name ($col0 + $j - 1)) + ($row0 + $i - 1)
            $cells += [pscustomobject]@{ a = $addr; k = $kind; v = $shown; f = $(if ($isF) { $formula } else { '' }); nf = $nf; fill = $fill; font = $fontc; bold = $bold }
        }
    }
    $hidden = @()
    try {
        $visible = $ws.Range($ws.Cells.Item(1, 1), $ws.Cells.Item($lastRow + 2, 1)).SpecialCells(12)
        $seen = @{}
        foreach ($a in $visible.Areas) { for ($r = $a.Row; $r -lt $a.Row + $a.Rows.Count; $r++) { $seen[$r] = $true } }
        for ($r = 1; $r -le $lastRow + 2; $r++) { if (-not $seen.ContainsKey($r)) { $hidden += $r } }
    } catch { $hidden = @(1..($lastRow + 2)) }
    $hiddenCols = @()
    try {
        $visible = $ws.Range($ws.Cells.Item(1, 1), $ws.Cells.Item(1, $lastCol + 2)).SpecialCells(12)
        $seen = @{}
        foreach ($a in $visible.Areas) { for ($c = $a.Column; $c -lt $a.Column + $a.Columns.Count; $c++) { $seen[$c] = $true } }
        for ($c = 1; $c -le $lastCol + 2; $c++) { if (-not $seen.ContainsKey($c)) { $hiddenCols += $c } }
    } catch { $hiddenCols = @(1..($lastCol + 2)) }
    $tables = @()
    foreach ($t in $ws.ListObjects) {
        $cols = @()
        foreach ($c in $t.ListColumns) {
            $tc = $null; try { $tc = $c.TotalsCalculation } catch {}
            $cols += [pscustomobject]@{ name = $c.Name; total = $tc }
        }
        $tables += [pscustomobject]@{ name = $t.Name; range = $t.Range.Address($false, $false); headers = $t.ShowHeaders; totals = $t.ShowTotals; style = $(if ($t.TableStyle) { $t.TableStyle.Name } else { '' }); rowStripes = $t.ShowTableStyleRowStripes; columnStripes = $t.ShowTableStyleColumnStripes; firstColumn = $t.ShowTableStyleFirstColumn; lastColumn = $t.ShowTableStyleLastColumn; filterButton = $t.ShowAutoFilterDropDown; columns = $cols }
    }
    $af = $null
    if ($ws.AutoFilterMode) { $af = $ws.AutoFilter.Range.Address($false, $false) }
    $outlineRows = @(); $outlineCols = @()
    if ($script:dumpOutline) {
        for ($r = 1; $r -le $lastRow + 2; $r++) { $l = $ws.Rows.Item($r).OutlineLevel; if ($l -gt 1) { $outlineRows += [pscustomobject]@{ row = $r; level = $l } } }
        for ($c = 1; $c -le $lastCol + 2; $c++) { $l = $ws.Columns.Item($c).OutlineLevel; if ($l -gt 1) { $outlineCols += [pscustomobject]@{ col = $c; level = $l } } }
    }
    $notes = @()
    foreach ($cm in $ws.Comments) { $notes += [pscustomobject]@{ a = $cm.Parent.Address($false, $false); text = $cm.Text() } }
    return [pscustomobject]@{ name = $ws.Name; notes = $notes; outlineRows = $outlineRows; outlineColumns = $outlineCols; cells = $cells; hiddenRows = $hidden; hiddenColumns = $hiddenCols; tables = $tables; autoFilter = $af; filterMode = [bool]$ws.FilterMode }
}

function Dump-Xml($book, [string]$tmp) {
    if (Test-Path $tmp) { Remove-Item -LiteralPath $tmp }
    $book.SaveCopyAs($tmp)
    $zip = [IO.Compression.ZipFile]::OpenRead($tmp)
    $sheetsXml = @(); $tablesXml = @(); $namesXml = ''; $dxfsXml = ''
    try {
        foreach ($e in $zip.Entries) {
            $reader = New-Object IO.StreamReader($e.Open(), [Text.Encoding]::UTF8)
            try {
                if ($e.FullName -match '^xl/worksheets/sheet\d+\.xml$') {
                    $text = $reader.ReadToEnd()
                    $m = [regex]::Match($text, '<autoFilter\b[^>]*?(/>|>.*?</autoFilter>)', 'Singleline')
                    $sheetsXml += [pscustomobject]@{ part = $e.FullName; autoFilter = $(if ($m.Success) { $m.Value } else { '' }) }
                } elseif ($e.FullName -match '^xl/tables/table\d+\.xml$') {
                    $tablesXml += [pscustomobject]@{ part = $e.FullName; xml = $reader.ReadToEnd() }
                } elseif ($e.FullName -eq 'xl/styles.xml') {
                    $text = $reader.ReadToEnd()
                    $m = [regex]::Match($text, '<dxfs\b[^>]*?(/>|>.*?</dxfs>)', 'Singleline')
                    $dxfsXml = if ($m.Success) { $m.Value } else { '' }
                } elseif ($e.FullName -eq 'xl/workbook.xml') {
                    $text = $reader.ReadToEnd()
                    $m = [regex]::Match($text, '<definedNames>.*?</definedNames>', 'Singleline')
                    $namesXml = if ($m.Success) { $m.Value } else { '' }
                }
            } finally { $reader.Dispose() }
        }
    } finally { $zip.Dispose() }
    return [pscustomobject]@{ sheets = $sheetsXml; tables = $tablesXml; names = $namesXml; dxfs = $dxfsXml }
}


Add-Type -Namespace Win -Name User32 -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);'

# A watchdog thread stops an Excel that has not answered for two minutes, so a scenario that leaves Excel
# waiting for a dialog costs one retry and not the run. It only ever stops the Excel this script started.
$sync = [hashtable]::Synchronized(@{ deadline = [DateTime]::MaxValue; excelPid = 0; stop = $false })
$watchdog = Start-ThreadJob -ArgumentList $sync -ScriptBlock {
    param($sync)
    while (-not $sync.stop) {
        Start-Sleep -Seconds 3
        if ([DateTime]::Now -gt $sync.deadline -and $sync.excelPid -gt 0) {
            try { Stop-Process -Id $sync.excelPid -Force } catch {}
            $sync.deadline = [DateTime]::MaxValue
        }
    }
}

function New-Excel {
    $x = New-Object -ComObject Excel.Application
    $x.DisplayAlerts = $false
    $x.Visible = $false
    $x.AutomationSecurity = 3
    $owner = [uint32]0
    $null = [Win.User32]::GetWindowThreadProcessId([IntPtr]$x.Hwnd, [ref]$owner)
    $sync.excelPid = [int]$owner
    return $x
}

# Run one scenario in a new workbook and answer what Excel did.
function Run-Scenario($excel, $sc, [string]$tmp) {
    $book = $excel.Workbooks.Add()
    $err = ''
    $script:dumpFormat = Has $sc 'dumpFormat'
    $script:dumpOutline = Has $sc 'dumpOutline'
    try {
        while ($book.Worksheets.Count -gt 1) { $book.Worksheets.Item($book.Worksheets.Count).Delete() }
        $book.Worksheets.Item(1).Name = 'Sheet1'
        if (Has $sc 'sheets') { Fill-Cells $book $sc.sheets }
        if (Has $sc 'formats') { Apply-Formats $book $sc.formats }
        if (Has $sc 'setup') { foreach ($step in $sc.setup) { Run-Step $book $step } }
        try {
            foreach ($step in @($sc.op)) { Run-Step $book $step }
        } catch { $err = $_.Exception.Message + " @ " + $_.ScriptStackTrace }
        if ($err -match 'RPC server|remote procedure call|disconnected from its clients|null-valued expression') { throw $err }
        $book.Application.Calculate()
        $dump = @(); foreach ($ws in $book.Worksheets) { $dump += (Dump-Sheet $ws) }
        $names = @(); foreach ($n in $book.Names) { $names += [pscustomobject]@{ name = $n.Name; refersTo = $n.RefersTo; visible = $n.Visible } }
        $xml = Dump-Xml $book $tmp
        $today = [double]$book.Application.Evaluate('TODAY()')
        return [pscustomobject]@{ name = $sc.name; error = $err; today = $today; sheets = $dump; names = $names; xml = $xml }
    } finally {
        try { $book.Close($false) } catch {}
    }
}

$list = Get-Content -LiteralPath $Scenarios -Raw -Encoding UTF8 | ConvertFrom-Json -Depth 30
$partial = "$Out.partial"
$done = @{}
$results = @()
if ((Test-Path $partial) -and $Resume) {
    foreach ($line in Get-Content -LiteralPath $partial -Encoding UTF8) { if ($line) { $r = $line | ConvertFrom-Json -Depth 30; $done[$r.name] = $true; $results += $r } }
} elseif (Test-Path $partial) { Remove-Item -LiteralPath $partial }
$excel = New-Excel
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("datatools-" + [Guid]::NewGuid().ToString('N') + '.xlsx')
try {
    foreach ($sc in $list) {
        if ($Only -and $sc.name -ne $Only) { continue }
        if ($done.ContainsKey($sc.name)) { continue }
        $result = $null
        for ($attempt = 1; $attempt -le 3 -and $null -eq $result; $attempt++) {
            try {
                $sync.deadline = [DateTime]::Now.AddSeconds(120)
                $result = Run-Scenario $excel $sc $tmp
                $sync.deadline = [DateTime]::MaxValue
            } catch {
                $sync.deadline = [DateTime]::MaxValue
                $message = $_.Exception.Message
                if ($message -match 'RPC server|remote procedure call|disconnected from its clients|0x800706BA|0x80010108|null-valued expression' -and $attempt -lt 3) {
                    try { $excel.Quit() } catch {}
                    Start-Sleep -Seconds 2
                    $excel = New-Excel
                } else {
                    $result = [pscustomobject]@{ name = $sc.name; error = "HARNESS: $message $($_.ScriptStackTrace)"; today = 0; sheets = @(); names = @(); xml = $null }
                }
            }
        }
        $results += $result
        ($result | ConvertTo-Json -Depth 12 -Compress) | Add-Content -LiteralPath $partial -Encoding UTF8
    }
} finally {
    $sync.stop = $true
    try { $excel.Quit() } catch {}
    if (Test-Path $tmp) { Remove-Item -LiteralPath $tmp }
}
ConvertTo-Json -InputObject @($results) -Depth 12 | Set-Content -LiteralPath $Out -Encoding UTF8
if (Test-Path $partial) { Remove-Item -LiteralPath $partial }
