# cropsheet.ps1 -Prefix <p> -Match <regex> -Rect "x,y,w,h" -Out <png> [-Cols 3] [-Scale 1]:
# the same region of every matching shot, tiled and labelled (compare fonts side by side).
param([string]$Prefix, [string]$Match = "", [string]$Rect, [string]$Out, [int]$Cols = 3, [double]$Scale = 1)
Add-Type -AssemblyName System.Drawing
$x, $y, $w, $h = $Rect.Split(",") | ForEach-Object { [int]$_ }
$files = Get-ChildItem "$env:TEMP\tidemail-shots\shots\$Prefix*.png" | Where-Object { -not $Match -or $_.Name -match $Match } | Sort-Object Name
$tw = [int]($w * $Scale); $th = [int]($h * $Scale)
$rows = [math]::Ceiling($files.Count / $Cols)
$bmp = New-Object System.Drawing.Bitmap ($tw * $Cols), (($th + 20) * $rows)
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::Magenta)
$font = New-Object System.Drawing.Font "Segoe UI", 10
for ($i = 0; $i -lt $files.Count; $i++) {
  $cx = ($i % $Cols) * $tw; $cy = [math]::Floor($i / $Cols) * ($th + 20)
  $img = [System.Drawing.Bitmap]::FromFile($files[$i].FullName)
  $g.FillRectangle([System.Drawing.Brushes]::White, $cx, $cy, $tw, 20)
  $g.DrawString($files[$i].BaseName, $font, [System.Drawing.Brushes]::Black, $cx + 4, $cy + 2)
  $g.DrawImage($img, (New-Object System.Drawing.Rectangle $cx, ($cy + 20), $tw, $th), (New-Object System.Drawing.Rectangle $x, $y, $w, $h), [System.Drawing.GraphicsUnit]::Pixel)
  $img.Dispose()
}
$bmp.Save($Out); $g.Dispose(); $bmp.Dispose(); $Out
