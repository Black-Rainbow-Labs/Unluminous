# Build the comment workbooks the review tests compare against, with Excel itself.
#
# `comments-start.xlsx` is a small workbook with no comments. Each scenario opens it, does what a person
# does with threaded comments and notes through Excel's object model, and saves the result as
# `comments-<scenario>.xlsx`; the same scenario is then read back through `review-read.ps1` into
# `comments-<scenario>.json`, which is what Excel says the file holds. The tests replay each scenario on
# the start workbook with the engine and compare.
#
#   pwsh tools/excel-oracle/make-review-comments.ps1 [-Folder crates/unluminous-sheet/tests/fixtures/review]
param([string]$Folder = 'crates/unluminous-sheet/tests/fixtures/review')
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Folder | Out-Null
$folder = (Resolve-Path -LiteralPath $Folder).Path
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false
$excel.Visible = $false
function Save-Book($book, $name) {
    $out = Join-Path $folder $name
    if (Test-Path -LiteralPath $out) { Remove-Item -LiteralPath $out }
    $book.SaveAs($out, 51)
    $book.Close($false)
}
function Open-Start() { $excel.Workbooks.Open((Join-Path $folder 'comments-start.xlsx')) }
try {
    $book = $excel.Workbooks.Add()
    while ($book.Worksheets.Count -lt 2) { $book.Worksheets.Add([Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count)) | Out-Null }
    foreach ($sheet in $book.Worksheets) {
        $sheet.Range('A1:L1').ColumnWidth = 10
        $sheet.Range('A1').Value2 = 'Item'
        $sheet.Range('B1').Value2 = 'Qty'
        $sheet.Range('A2').Value2 = 'Pens'
        $sheet.Range('B2').Value2 = 4
    }
    Save-Book $book 'comments-start.xlsx'

    # threads: new, reply, resolve, reopen, edit, delete a reply, delete a thread.
    $book = Open-Start
    $s = $book.Worksheets.Item(1)
    $t = $s.Range('A1').AddCommentThreaded('First thread')
    $t.AddReply('Reply one') | Out-Null
    $t.AddReply("Reply two`nsecond line") | Out-Null
    $t.Resolved = $true
    $u = $s.Range('B2').AddCommentThreaded('Second thread')
    $u.Text('Second thread edited') | Out-Null
    $u.AddReply('Dropped reply') | Out-Null
    $u.AddReply('Kept reply') | Out-Null
    $u.Replies.Item(1).Delete()
    $v = $s.Range('C3').AddCommentThreaded('Third thread, deleted')
    $v.Delete()
    $w = $s.Range('D4').AddCommentThreaded('Fourth thread, reopened')
    $w.Resolved = $true
    $w.Resolved = $false
    $x = $book.Worksheets.Item(2).Range('A2').AddCommentThreaded('On the second sheet')
    Save-Book $book 'comments-threads.xlsx'

    # notes: shown, hidden, resized, and one with several lines.
    $book = Open-Start
    $s = $book.Worksheets.Item(1)
    $a = $s.Range('A1').AddComment('First note')
    $a.Visible = $true
    $a.Shape.Width = 150
    $a.Shape.Height = 80
    $s.Range('B2').AddComment("Second note`nwith two lines") | Out-Null
    $c = $s.Range('D4').AddComment('Third note')
    $c.Visible = $true
    $c.Shape.Width = 200
    $c.Shape.Height = 45
    $c.Shape.Left = $c.Shape.Left + 30
    $c.Shape.Top = $c.Shape.Top + 20
    $s.Range('F6').AddComment('Deleted note') | Out-Null
    $s.Range('F6').ClearComments() | Out-Null
    Save-Book $book 'comments-notes.xlsx'

    # both kinds on one sheet, and a note shown then hidden again.
    $book = Open-Start
    $s = $book.Worksheets.Item(1)
    $s.Range('A1').AddCommentThreaded('A thread') | Out-Null
    $s.Range('B2').AddComment('A note') | Out-Null
    $s.Range('B2').Comment.Visible = $true
    $s.Range('B2').Comment.Visible = $false
    $s.Range('C3').AddCommentThreaded('Another thread') | Out-Null
    $s.Range('C3').CommentThreaded.AddReply('With a reply') | Out-Null
    Save-Book $book 'comments-mixed.xlsx'
} finally {
    $excel.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}
foreach ($name in 'start', 'threads', 'notes', 'mixed') {
    $json = & pwsh -NoProfile -File (Join-Path $here 'review-read.ps1') -Path (Join-Path $folder "comments-$name.xlsx")
    Set-Content -LiteralPath (Join-Path $folder "comments-$name.json") -Value $json -Encoding utf8
}
