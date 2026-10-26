Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$excel = New-Object -ComObject Excel.Application
$excel.DisplayAlerts = $false; $excel.Visible = $true
try {
  $wb = $excel.Workbooks.Add(); $ws = $wb.Worksheets.Item(1)
  $sets = @(@(5,7,9,6), @(1,-2,3,-4), @(2,2,2,2), @(-3,-5,-4,-9), @(0,0,4,0))
  for ($r = 0; $r -lt $sets.Count; $r++) { for ($c = 0; $c -lt 4; $c++) { $ws.Cells.Item($r + 1, $c + 1).Value2 = $sets[$r][$c] } }
  $ws.Columns.Item(6).ColumnWidth = 30; $ws.Columns.Item(7).ColumnWidth = 30
  for ($r = 1; $r -le $sets.Count; $r++) { $ws.Rows.Item($r).RowHeight = 60 }
  $g1 = $ws.Range("F1:F5").SparklineGroups.Add(2, "A1:D5"); $g1.SeriesColor.Color = 255   # column, red (BGR 0000FF)
  $g2 = $ws.Range("G1:G5").SparklineGroups.Add(1, "A1:D5"); $g2.SeriesColor.Color = 255; $g2.LineWeight = 3
  $out = @()
  foreach ($col in 'F', 'G') {
    for ($r = 1; $r -le $sets.Count; $r++) {
      $cell = $ws.Range("$col$r")
      [void]$cell.CopyPicture(1, 2)
      Start-Sleep -Milliseconds 300
      $img = [Windows.Forms.Clipboard]::GetImage()
      if ($null -eq $img) { $out += "$col$r no image"; continue }
      $bmp = New-Object System.Drawing.Bitmap $img
      # Red pixels: for each x slice find top and bottom y of red.
      $rows = @{}; $minY = 9999; $maxY = -1; $segments = @()
      for ($x = 0; $x -lt $bmp.Width; $x++) {
        $top = -1; $bot = -1
        for ($y = 0; $y -lt $bmp.Height; $y++) { $p = $bmp.GetPixel($x, $y); if ($p.R -gt 200 -and $p.G -lt 60 -and $p.B -lt 60) { if ($top -lt 0) { $top = $y }; $bot = $y } }
        if ($top -ge 0) { $segments += "$x`:$top-$bot"; $minY = [Math]::Min($minY, $top); $maxY = [Math]::Max($maxY, $bot) }
      }
      $out += "$col$r size=$($bmp.Width)x$($bmp.Height) redY=$minY..$maxY segs=" + (($segments | Select-Object -Unique) -join ' ')
      $bmp.Dispose()
    }
  }
  $out | Set-Content 'D:\agent-worktrees\scratch-objects\com\probe-scale.txt'
  $wb.Close($false)
} finally { $excel.Quit(); [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel) }
