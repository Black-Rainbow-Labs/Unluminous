# Has Excel open a workbook with slicers and timelines and report what it sees as one JSON object: whether it
# repaired the file, each slicer cache's items and whether each is selected (SlicerCaches(i).SlicerItems(j).Selected),
# each timeline's filter, and the items of every pivot field that are visible.
# Usage: pwsh -File slicer-check.ps1 -Path <xlsx>
param([Parameter(Mandatory = $true)][string]$Path)
$ErrorActionPreference = 'Stop'
$Path = [IO.Path]::GetFullPath($Path)
$xl = New-Object -ComObject Excel.Application
$xl.Visible = $false
$xl.DisplayAlerts = $false
try {
    $repaired = $false
    try { $wb = $xl.Workbooks.Open($Path, 0, $true) } catch { $repaired = $true; $wb = $xl.Workbooks.Open($Path, 0, $true, 5, '', '', $true, 2, '', $false, $false, 0, $false, $false, 1) }
    $repaired = $repaired -or ($xl.ActiveWindow.Caption -match 'Repaired')
    $caches = @()
    foreach ($sc in $wb.SlicerCaches) {
        $entry = [ordered]@{ name = $sc.Name; source = $sc.SourceName; kind = $sc.Slicers.Count; items = @() }
        try {
            foreach ($item in $sc.SlicerItems) { $entry.items += [ordered]@{ name = $item.Name; selected = [bool]$item.Selected } }
        } catch { $entry.timeline = $true }
        try {
            $state = $sc.TimelineState
            $entry.filter = [ordered]@{ start = ([datetime]$state.StartDate).ToString('yyyy-MM-dd'); end = ([datetime]$state.EndDate).ToString('yyyy-MM-dd'); type = [int]$state.FilterType }
        } catch { if ($entry.timeline) { $entry.filterError = $_.Exception.Message } }
        $entry.slicers = @($sc.Slicers | ForEach-Object { [ordered]@{ name = $_.Name; caption = $_.Caption } })
        $caches += $entry
    }
    $pivots = @()
    foreach ($ws in $wb.Worksheets) {
        foreach ($pt in $ws.PivotTables()) {
            $fields = @()
            foreach ($pf in $pt.PivotFields()) {
                try {
                    $visible = @($pf.PivotItems() | Where-Object { $_.Visible } | ForEach-Object { $_.Name })
                    $fields += [ordered]@{ field = $pf.Name; visible = $visible }
                } catch {}
            }
            $pivots += [ordered]@{ name = $pt.Name; sheet = $ws.Name; range = $pt.TableRange1.Address($false, $false); fields = $fields }
        }
    }
    [ordered]@{ repaired = $repaired; slicerCaches = $caches; pivots = $pivots } | ConvertTo-Json -Depth 8 -Compress
    $wb.Close($false)
} finally {
    $xl.Quit()
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($xl)
}
