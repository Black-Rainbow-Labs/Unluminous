# Open a workbook in Excel and print the comments Excel sees, as JSON: every threaded comment with its
# author, words, replies and whether it is resolved, and every note with its author, words, whether it is
# shown and where its box is. Also says whether Excel had to repair the file.
#
#   pwsh tools/excel-oracle/review-read.ps1 -Path book.xlsx
param([Parameter(Mandatory = $true)][string]$Path)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$full = (Resolve-Path -LiteralPath $Path).Path
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
$excel.AutomationSecurity = 3
try {
    $repaired = $false
    try { $book = $excel.Workbooks.Open($full, 0, $true) }
    catch {
        $repaired = $true
        $book = $excel.Workbooks.Open($full, 0, $true, 5, '', '', $true, 2, '', $false, $false, 0, $false, $false, 1)
    }
    $sheets = @()
    foreach ($sheet in $book.Worksheets) {
        $threads = @()
        foreach ($t in $sheet.CommentsThreaded) {
            $replies = @()
            foreach ($r in $t.Replies) { $replies += [pscustomobject]@{ author = $r.Author.Name; text = $r.Text() } }
            $threads += [pscustomobject]@{
                cell = $t.Parent.Address($false, $false)
                author = $t.Author.Name
                text = $t.Text()
                resolved = [bool]$t.Resolved
                replies = $replies
            }
        }
        $notes = @()
        foreach ($c in $sheet.Comments) {
            $notes += [pscustomobject]@{
                cell = $c.Parent.Address($false, $false)
                author = $c.Author
                text = $c.Text()
                visible = [bool]$c.Visible
                left = $c.Shape.Left
                top = $c.Shape.Top
                width = $c.Shape.Width
                height = $c.Shape.Height
            }
        }
        $widths = @(); foreach ($i in 1..12) { $widths += $sheet.Columns.Item($i).Width }
        $heights = @(); foreach ($i in 1..12) { $heights += $sheet.Rows.Item($i).Height }
        $sheets += [pscustomobject]@{ name = $sheet.Name; threads = $threads; notes = $notes; columnPoints = $widths; rowPoints = $heights }
    }
    [pscustomobject]@{ repaired = $repaired; sheets = $sheets } | ConvertTo-Json -Depth 8 -Compress
    $book.Close($false)
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
