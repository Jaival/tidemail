# sheet.ps1 <prefix> <out> [cols] [scale] [-Match regex]: grid of shots matching prefix*.png
# (and the regex, if given), labelled.
param([string]$Prefix, [string]$Out, [int]$Cols = 2, [double]$Scale = 0.5, [string]$Match = "")
Add-Type -AssemblyName System.Drawing
$files = Get-ChildItem "$env:TEMP\tidemail-shots\shots\$Prefix*.png" | Where-Object { $_.Name -notmatch 'crop' -and (-not $Match -or $_.Name -match $Match) } | Sort-Object Name
$first = [System.Drawing.Bitmap]::FromFile($files[0].FullName)
$w = [int]($first.Width * $Scale); $h = [int]($first.Height * $Scale); $first.Dispose()
$rows = [math]::Ceiling($files.Count / $Cols)
$bmp = New-Object System.Drawing.Bitmap ($w * $Cols), (($h + 22) * $rows)
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::Magenta)
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$font = New-Object System.Drawing.Font "Segoe UI", 11
for ($i = 0; $i -lt $files.Count; $i++) {
  $x = ($i % $Cols) * $w; $y = [math]::Floor($i / $Cols) * ($h + 22)
  $img = [System.Drawing.Bitmap]::FromFile($files[$i].FullName)
  $g.FillRectangle([System.Drawing.Brushes]::White, $x, $y, $w, 22)
  $g.DrawString($files[$i].BaseName, $font, [System.Drawing.Brushes]::Black, $x + 4, $y + 2)
  $g.DrawImage($img, $x, $y + 22, $w, $h); $img.Dispose()
}
$bmp.Save($Out); $g.Dispose(); $bmp.Dispose(); $Out
