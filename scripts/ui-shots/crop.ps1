# crop.ps1 <in> <x> <y> <w> <h> [scale] -> <in>-crop.png
param([string]$In, [int]$X, [int]$Y, [int]$W, [int]$H, [int]$Scale = 2)
Add-Type -AssemblyName System.Drawing
$src = [System.Drawing.Bitmap]::FromFile($In)
$dst = New-Object System.Drawing.Bitmap ($W * $Scale), ($H * $Scale)
$g = [System.Drawing.Graphics]::FromImage($dst)
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
$g.DrawImage($src, (New-Object System.Drawing.Rectangle 0, 0, ($W * $Scale), ($H * $Scale)), (New-Object System.Drawing.Rectangle $X, $Y, $W, $H), [System.Drawing.GraphicsUnit]::Pixel)
$o = $In -replace '\.png$', '-crop.png'
$dst.Save($o); $g.Dispose(); $dst.Dispose(); $src.Dispose(); $o
