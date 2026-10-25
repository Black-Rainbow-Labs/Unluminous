# Build pivot tables in Excel from a cases file and print the cells Excel lays out, or open pivot tables
# Unluminous wrote, refresh them in Excel and print the cells Excel lays out from them.
#
# The cases file is JSON: `datasets` (name -> headers, formats, rows) and `cases` (one pivot each: which
# dataset, the row, column, page and value fields, the layout and the field settings). The same file is read by
# `crates/unluminous-sheet/tests/pivot.rs`, which builds each case with the engine and compares its cells with
# the cells printed here, one JSON object a line.
#
#   pwsh tools/excel-oracle/pivot-check.ps1 -Cases cases.json                       # Excel builds each case
#   pwsh tools/excel-oracle/pivot-check.ps1 -Cases cases.json -Open <folder>        # Excel opens <folder>\<id>.xlsx
#   pwsh tools/excel-oracle/pivot-check.ps1 -Cases cases.json -Only id1,id2         # some of the cases
#   pwsh tools/excel-oracle/pivot-check.ps1 -Cases cases.json -Save <folder>        # also save Excel's workbook per case
#
# The line for a case holds `id`, `range` (the pivot's table range with its page fields), `cells` (rows of
# `{v, t, f}`: the value at full precision, the text Excel shows and the number format) and `tables`, the
# rowItems and colItems Excel keeps for the pivot after refreshing it, as XML, when -Save is given.
param(
    [Parameter(Mandatory = $true)][string]$Cases,
    [string]$Open,
    [string[]]$Only,
    [string]$Save,
    [switch]$Pre,
    [switch]$Isolate
)
$ErrorActionPreference = 'Stop'
if ($Only) { $Only = @($Only | ForEach-Object { $_ -split ',' }) }
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$inv = [Globalization.CultureInfo]::InvariantCulture

$FUNCS = @{ sum = -4157; count = -4112; average = -4106; max = -4136; min = -4139; product = -4149; countnums = -4113; stdev = -4155; stdevp = -4156; var = -4164; varp = -4165 }
$SHOW = @{ none = -4143; difference = 2; percentof = 3; percentdifference = 4; running = 5; percentrow = 6; percentcolumn = 7; percenttotal = 8; index = 9; percentparentrow = 10; percentparentcolumn = 11; percentparent = 12; percentrunning = 13; rankasc = 14; rankdesc = 15 }
$LAYOUT = @{ compact = 0; tabular = 1; outline = 2 }

function Read-Cases { Get-Content -LiteralPath $Cases -Raw -Encoding UTF8 | ConvertFrom-Json -Depth 30 }

# The rows of a dataset into a sheet named Data_<name>, with the formats of its columns.
function Write-Dataset($book, $name, $set) {
    $sheet = $book.Worksheets.Add([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count))
    $sheet.Name = "Data_$name"
    $cols = $set.headers.Count
    $rows = $set.rows.Count
    $arr = New-Object 'object[,]' ($rows + 1), $cols
    $errors = @()
    for ($c = 0; $c -lt $cols; $c++) { $arr[0, $c] = $set.headers[$c] }
    for ($r = 0; $r -lt $rows; $r++) {
        for ($c = 0; $c -lt $cols; $c++) {
            $v = $set.rows[$r][$c]
            if ($v -is [pscustomobject]) { $errors += , @(($r + 2), ($c + 1)) } elseif ($null -ne $v) { $arr[($r + 1), $c] = $v }
        }
    }
    for ($c = 0; $c -lt $cols; $c++) {
        $fmt = $set.formats[$c]
        if ($fmt -and $fmt -ne 'General') { $sheet.Range($sheet.Cells(2, $c + 1), $sheet.Cells($rows + 1, $c + 1)).NumberFormat = $fmt }
    }
    $sheet.Range($sheet.Cells(1, 1), $sheet.Cells($rows + 1, $cols)).Value2 = $arr
    foreach ($e in $errors) { $sheet.Cells($e[0], $e[1]).Formula = '=NA()' }
    return $sheet
}

# Set the field options of one pivot field from the case.
function Set-FieldOptions($pt, $case) {
    if (-not $case.fields) { return }
    foreach ($prop in $case.fields.PSObject.Properties) {
        $pf = $pt.PivotFields($prop.Name)
        $o = $prop.Value
        if ($null -ne $o.subtotals) {
            if ($o.subtotals -eq 'off') {
                $pf.Subtotals = @($false, $false, $false, $false, $false, $false, $false, $false, $false, $false, $false, $false)
            } elseif ($o.subtotals -is [string]) {
            } else {
                $flags = @($false) * 12
                $index = @{ automatic = 0; sum = 1; count = 2; average = 3; max = 4; min = 5; product = 6; countnums = 7; stdev = 8; stdevp = 9; var = 10; varp = 11 }
                foreach ($s in $o.subtotals) { $flags[$index[$s]] = $true }
                $pf.Subtotals = $flags
            }
        }
        if ($o.subtotal_at) { $pf.LayoutSubtotalLocation = $(if ($o.subtotal_at -eq 'bottom') { 2 } else { 1 }) }
        if ($null -ne $o.blank_row) { $pf.LayoutBlankLine = [bool]$o.blank_row }
        if ($null -ne $o.repeat) { $pf.RepeatLabels = [bool]$o.repeat }
        if ($o.layout -eq 'compact') { $pf.LayoutCompactRow = $true } elseif ($o.layout -eq 'outline') { $pf.LayoutCompactRow = $false; $pf.LayoutForm = 1 } elseif ($o.layout -eq 'tabular') { $pf.LayoutCompactRow = $false; $pf.LayoutForm = 0 }
        if ($o.caption) { $pf.Caption = $o.caption }
        if ($o.sort) {
            $order = $(if ($o.sort -eq 'desc') { 2 } else { 1 })
            if ($o.sort_by) { $pf.AutoSort($order, $o.sort_by) } else { $pf.AutoSort($order, $pf.SourceName) }
        }
        if ($o.hide) { foreach ($item in $o.hide) { $pf.PivotItems($item).Visible = $false } }
        if ($o.order) { $n = 0; foreach ($item in $o.order) { $n++; $pf.PivotItems($item).Position = $n } }
        if ($o.multi_page) { $pf.EnableMultiplePageItems = $true }
        if ($o.filter) {
            $f = $o.filter
            $types = @{ top = 1; bottom = 2; toppercent = 3; bottompercent = 4; topsum = 5; bottomsum = 6; veq = 7; vne = 8; vgt = 9; vge = 10; vlt = 11; vle = 12; vbetween = 13; vnotbetween = 14; eq = 15; ne = 16; begins = 17; notbegins = 18; ends = 19; notends = 20; contains = 21; notcontains = 22; gt = 23; ge = 24; lt = 25; le = 26; between = 27; notbetween = 28 }
            $data = $null; if ($f.data) { $data = $pt.DataFields($f.data) }
            if ($null -ne $f.value2) { $null = $pf.PivotFilters.Add2($types[$f.type], $data, $f.value, $f.value2) } else { $null = $pf.PivotFilters.Add2($types[$f.type], $data, $f.value) }
        }
        if ($o.show_all_items) { $pf.ShowAllItems = $true }
        if ($o.numfmt) { $pf.NumberFormat = $o.numfmt }
        if ($o.hidden_items_in_totals) { $pt.VisualTotals = $true }
    }
}

# Group a field: dates by periods, numbers by a step, or chosen items into groups.
function Group-Field($excel, $pt, $g) {
    $pf = $pt.PivotFields($g.field)
    if ($g.kind -eq 'date') {
        $names = @('seconds', 'minutes', 'hours', 'days', 'months', 'quarters', 'years')
        $periods = @(); foreach ($n in $names) { $periods += [bool]($g.by -contains $n) }
        $start = $(if ($null -ne $g.start) { $g.start } else { $true })
        $end = $(if ($null -ne $g.end) { $g.end } else { $true })
        $by = $(if ($null -ne $g.step) { $g.step } else { [Type]::Missing })
        $null = $pf.DataRange.Cells(1).Group($start, $end, $by, $periods)
    } elseif ($g.kind -eq 'number') {
        $from = $(if ($null -ne $g.start) { $g.start } else { $true })
        $to = $(if ($null -ne $g.end) { $g.end } else { $true })
        $null = $pf.DataRange.Cells(1).Group($from, $to, $g.step)
    } else {
        foreach ($members in $g.groups) {
            $range = $null
            foreach ($m in $members) { $cell = $pf.PivotItems($m).LabelRange; if ($null -eq $range) { $range = $cell } else { $range = $excel.Union($range, $cell) } }
            $null = $range.Group()
        }
    }
}

# Build one case's pivot table on a new sheet.
function Build-Pivot($excel, $book, $case, $dataSheet) {
    $sheet = $book.Worksheets.Add([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count))
    $sheet.Name = "P_" + $case.id.Substring(0, [Math]::Min(25, $case.id.Length))
    if ($Isolate) {
        # Excel shares a cache between tables with the same source, so each case reads a copy of its own.
        $dataSheet.Copy([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count))
        $dataSheet = $book.Worksheets.Item($book.Worksheets.Count)
        $dataSheet.Name = 'D_' + $case.id.Substring(0, [Math]::Min(25, $case.id.Length))
        $sheet.Move([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count))
    }
    $last = $dataSheet.UsedRange
    $source = "'" + $dataSheet.Name + "'!" + $last.Address($true, $true, -4150)
    $cache = $book.PivotCaches().Create(1, $source)
    $at = $(if ($case.at) { $case.at } else { 'A6' })
    $pt = $cache.CreatePivotTable($sheet.Range($at), 'PT1')
    $pt.ManualUpdate = $true
    if ($case.layout) { $pt.RowAxisLayout($LAYOUT[$case.layout]) }
    if ($case.repeat) { $pt.RepeatAllLabels(2) }
    foreach ($cf in @($case.calc_fields)) { if ($cf) { $null = $pt.CalculatedFields().Add($cf.name, $cf.formula, $true) } }
    $i = 0; foreach ($name in @($case.rows)) { if ($name) { $i++; $pf = $pt.PivotFields($name); $pf.Orientation = 1; $pf.Position = $i } }
    $i = 0; foreach ($name in @($case.cols)) { if ($name) { $i++; $pf = $pt.PivotFields($name); $pf.Orientation = 2; $pf.Position = $i } }
    # A new filter field goes on top, so adding them last first leaves them in the order listed.
    $listed = @($case.pages); [array]::Reverse($listed); foreach ($name in $listed) { if ($name) { $pf = $pt.PivotFields($name); $pf.Orientation = 3 } }
    foreach ($v in @($case.values)) {
        if (-not $v) { continue }
        if ($v.name) { $df = $pt.AddDataField($pt.PivotFields($v.field), $v.name, $FUNCS[$v.func]) } else { $df = $pt.AddDataField($pt.PivotFields($v.field), [Type]::Missing, $FUNCS[$v.func]) }
        if ($v.show -and $v.show -ne 'none') {
            $df.Calculation = $SHOW[$v.show]
            if ($v.base) { $df.BaseField = $v.base }
            if ($null -ne $v.item) { $df.BaseItem = $v.item }
        }
        if ($v.fmt) { $df.NumberFormat = $v.fmt }
    }
    if ($case.values_on -eq 'rows') { $pt.DataPivotField.Orientation = 1 }
    if ($null -ne $case.values_pos) { $pt.DataPivotField.Position = [int]$case.values_pos }
    if ($case.grand) {
        if ($null -ne $case.grand.row) { $pt.ColumnGrand = [bool]$case.grand.row }
        if ($null -ne $case.grand.col) { $pt.RowGrand = [bool]$case.grand.col }
    }
    foreach ($ci in @($case.calc_items)) { if ($ci) { $null = $pt.PivotFields($ci.field).CalculatedItems().Add($ci.name, $ci.formula, $true) } }
    $pt.ManualUpdate = $false
    foreach ($g in @($case.group)) { if ($g) { Group-Field $excel $pt $g } }
    Set-FieldOptions $pt $case
    if ($case.page_select) {
        foreach ($prop in $case.page_select.PSObject.Properties) { $pt.PivotFields($prop.Name).CurrentPage = $prop.Value }
    }
    if ($case.captions) {
        $c = $case.captions
        if ($c.row_header) { $pt.CompactLayoutRowHeader = $c.row_header }
        if ($c.col_header) { $pt.CompactLayoutColumnHeader = $c.col_header }
        if ($c.grand) { $pt.GrandTotalName = $c.grand }
        if ($null -ne $c.empty) { $pt.NullString = $c.empty; $pt.DisplayNullString = $true }
        if ($null -ne $c.error) { $pt.ErrorString = $c.error; $pt.DisplayErrorString = $true }
    }
    if ($case.options) {
        $o = $case.options
        if ($null -ne $o.show_empty_rows) { $pt.ShowTableStyleRowStripes = $false }
        if ($null -ne $o.subtotals_hidden) { $pt.SubtotalHiddenItems = [bool]$o.subtotals_hidden }
    }
    return $pt
}

# The cells of the pivot's table range with its page fields.
function Dump-Pivot($sheet, $pt) {
    $range = $pt.TableRange2
    $rows = @()
    for ($r = 1; $r -le $range.Rows.Count; $r++) {
        $row = @()
        for ($c = 1; $c -le $range.Columns.Count; $c++) {
            $cell = $range.Cells.Item($r, $c)
            $v = $cell.Value2
            if ($null -eq $v) { $row += $null; continue }
            $kind = if ($v -is [double]) { 'n' } elseif ($v -is [bool]) { 'b' } elseif ($v -is [int]) { 'e' } else { 's' }
            $shown = if ($kind -eq 'n') { $v.ToString('R', $inv) } elseif ($kind -eq 'e') { $cell.Text } else { "$v" }
            $row += [pscustomobject]@{ k = $kind; v = $shown; t = $cell.Text; f = $cell.NumberFormat }
        }
        $rows += , $row
    }
    return [pscustomobject]@{ range = $range.Address($false, $false); data = $pt.TableRange1.Address($false, $false); cells = $rows }
}

$spec = Read-Cases
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$excel.AutomationSecurity = 3
try {
    $selected = @($spec.cases | Where-Object { (-not $Only) -or ($Only -contains $_.id) })
    if ($Open) {
        foreach ($case in $selected) {
            $path = Join-Path (Resolve-Path -LiteralPath $Open).Path ($case.id + '.xlsx')
            $repaired = $false
            try { $book = $excel.Workbooks.Open($path, 0, $false) } catch { $repaired = $true; $book = $excel.Workbooks.Open($path, 0, $false, 5, '', '', $true, 2, '', $false, $false, 0, $false, $false, 1) }
            $caption = $book.Windows.Item(1).Caption
            $repaired = $repaired -or ($caption -match 'Repaired')
            $sheet = $null; $pt = $null
            foreach ($s in $book.Worksheets) { if ($s.PivotTables().Count -gt 0) { $sheet = $s; $pt = $s.PivotTables(1); break } }
            $pre = $null
            if ($pt) {
                if ($Pre) { $pre = Dump-Pivot $sheet $pt }
                foreach ($edit in @($case.after)) {
                    if (-not $edit) { continue }
                    $target = $book.Worksheets.Item($edit.sheet).Range($edit.cell)
                    if ($null -eq $edit.value) { $target.ClearContents() | Out-Null } else { $target.Value2 = $edit.value }
                }
                $pt.RefreshTable() | Out-Null
                $post = Dump-Pivot $sheet $pt
            }
            $book.Close($false)
            [pscustomobject]@{ id = $case.id; repaired = $repaired; range = $post.range; data = $post.data; cells = $post.cells; pre = $pre } | ConvertTo-Json -Compress -Depth 8
        }
    } else {
        $book = $excel.Workbooks.Add()
        $sheets = @{}
        foreach ($name in ($spec.datasets.PSObject.Properties.Name)) { $sheets[$name] = Write-Dataset $book $name $spec.datasets.$name }
        foreach ($case in $selected) {
            try {
                $pt = Build-Pivot $excel $book $case $sheets[$case.data]
                $dump = Dump-Pivot $pt.Parent $pt
                $answers = @()
                foreach ($f in @($case.formulas)) {
                    if (-not $f) { continue }
                    $cell = $pt.Parent.Range('Z1')
                    $cell.Formula2 = '=' + $f
                    $v = $cell.Value2
                    $kind = if ($null -eq $v) { 'empty' } elseif ($v -is [double]) { 'number' } elseif ($v -is [bool]) { 'bool' } elseif ($v -is [int]) { 'error' } else { 'text' }
                    $shown = if ($kind -eq 'number') { $v.ToString('R', $inv) } elseif ($kind -eq 'error') { $cell.Text } else { "$v" }
                    $answers += [pscustomobject]@{ f = $f; k = $kind; v = $shown }
                    $cell.Clear() | Out-Null
                }
                [pscustomobject]@{ id = $case.id; repaired = $false; range = $dump.range; data = $dump.data; cells = $dump.cells; formulas = $answers } | ConvertTo-Json -Compress -Depth 8
            } catch {
                [pscustomobject]@{ id = $case.id; error = $_.Exception.Message + ' @' + $_.InvocationInfo.ScriptLineNumber } | ConvertTo-Json -Compress
            }
        }
        if ($Save) {
            New-Item -ItemType Directory -Force -Path $Save | Out-Null
            $book.SaveAs((Join-Path (Resolve-Path -LiteralPath $Save).Path 'excel-built.xlsx'), 51)
        }
        $book.Close($false)
    }
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
