# Open a workbook in Excel and report how its protection behaves, as JSON: for each sheet whether its cells,
# objects and scenarios are protected, which actions are allowed and the edit ranges; and, for each password
# the plan names, whether a wrong password is refused and the right one unprotects.
#
#   pwsh tools/excel-oracle/check-protection.ps1 -Path book.xlsx -Plan <base64 of the JSON plan>
#
# The plan is JSON, as base64 so that no quoting of the command line can change it:
#   {"sheets":[{"index":1,"password":"secret"}],"workbook":"bpw","ranges":[{"sheet":3,"title":"Locked range","password":"rangepw"}]}
param([Parameter(Mandatory = $true)][string]$Path, [string]$Plan = 'e30=')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$spec = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($Plan)) | ConvertFrom-Json
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
function Attempt([scriptblock]$Do) { try { & $Do | Out-Null; return 'ok' } catch { return ($_.Exception.Message -replace "`r|`n", ' ') } }
try {
    $book = $excel.Workbooks.Open((Resolve-Path -LiteralPath $Path).Path)
    $sheets = @()
    foreach ($s in $book.Worksheets) {
        $p = $s.Protection
        $ranges = @()
        $count = $p.AllowEditRanges.Count
        for ($i = 1; $i -le $count; $i++) { $r = $p.AllowEditRanges.Item($i); $ranges += [pscustomobject]@{ title = $r.Title; address = $r.Range.Address($false, $false) } }
        $sheets += [pscustomobject]@{
            name = $s.Name
            contents = [bool]$s.ProtectContents
            objects = [bool]$s.ProtectDrawingObjects
            scenarios = [bool]$s.ProtectScenarios
            formatCells = [bool]$p.AllowFormattingCells
            formatColumns = [bool]$p.AllowFormattingColumns
            formatRows = [bool]$p.AllowFormattingRows
            insertColumns = [bool]$p.AllowInsertingColumns
            insertRows = [bool]$p.AllowInsertingRows
            insertHyperlinks = [bool]$p.AllowInsertingHyperlinks
            deleteColumns = [bool]$p.AllowDeletingColumns
            deleteRows = [bool]$p.AllowDeletingRows
            sort = [bool]$p.AllowSorting
            autoFilter = [bool]$p.AllowFiltering
            pivotTables = [bool]$p.AllowUsingPivotTables
            ranges = $ranges
        }
    }
    $checks = @()
    # The edit ranges first: a range is unlocked on a sheet that is still protected.
    foreach ($e in @($spec.ranges | Where-Object { $_ })) {
        $s = $book.Worksheets.Item([int]$e.sheet)
        $r = $null
        for ($i = 1; $i -le $s.Protection.AllowEditRanges.Count; $i++) { $x = $s.Protection.AllowEditRanges.Item($i); if ($x.Title -eq $e.title) { $r = $x } }
        $checks += [pscustomobject]@{ what = "range $($e.title) wrong password"; result = Attempt { $r.Unprotect('not the password') } }
        $checks += [pscustomobject]@{ what = "range $($e.title) right password"; result = Attempt { $r.Unprotect($e.password) } }
    }
    foreach ($e in @($spec.sheets | Where-Object { $_ })) {
        $s = $book.Worksheets.Item([int]$e.index)
        $checks += [pscustomobject]@{ what = "sheet $($e.index) wrong password"; result = Attempt { $s.Unprotect('not the password') } }
        $checks += [pscustomobject]@{ what = "sheet $($e.index) right password"; result = Attempt { $s.Unprotect($e.password) } }
        $checks += [pscustomobject]@{ what = "sheet $($e.index) unprotected"; result = [string](-not $s.ProtectContents) }
    }
    $workbook = [pscustomobject]@{ structure = [bool]$book.ProtectStructure; windows = [bool]$book.ProtectWindows }
    if ($spec.workbook) {
        $checks += [pscustomobject]@{ what = 'workbook wrong password'; result = Attempt { $book.Unprotect('not the password') } }
        $checks += [pscustomobject]@{ what = 'workbook right password'; result = Attempt { $book.Unprotect($spec.workbook) } }
        $checks += [pscustomobject]@{ what = 'workbook unprotected'; result = [string](-not $book.ProtectStructure) }
    }
    [pscustomobject]@{ sheets = $sheets; workbook = $workbook; checks = $checks } | ConvertTo-Json -Depth 6 -Compress
    $book.Close($false)
} finally {
    $excel.Quit()
}
