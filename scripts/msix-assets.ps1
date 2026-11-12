# Generates the MSIX tile, Start menu and taskbar images from assets/loki-icon-1024.png.
# Uses the System.Drawing that ships with Windows PowerShell; run once when the icon changes:
#   powershell -NoProfile -File scripts/msix-assets.ps1
Add-Type -AssemblyName System.Drawing
$root = Split-Path -Parent $PSScriptRoot
$src = [System.Drawing.Image]::FromFile((Join-Path $root 'assets\loki-icon-1024.png'))
$out = Join-Path $root 'src-tauri\msix\Assets'
New-Item -ItemType Directory -Force $out | Out-Null

function Save-Png([string]$name, [int]$size) {
  $bmp = New-Object System.Drawing.Bitmap $size, $size
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
  $g.Clear([System.Drawing.Color]::Transparent)
  $g.DrawImage($src, 0, 0, $size, $size)
  $bmp.Save((Join-Path $out $name), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
}

foreach ($s in @(@(100, 1), @(125, 1.25), @(150, 1.5), @(200, 2), @(400, 4))) {
  $tag = "scale-$($s[0])"; $k = $s[1]
  Save-Png "StoreLogo.$tag.png" ([int](50 * $k))
  Save-Png "Square44x44Logo.$tag.png" ([int](44 * $k))
  Save-Png "Square150x150Logo.$tag.png" ([int](150 * $k))
}
foreach ($t in 16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 256) {
  Save-Png "Square44x44Logo.targetsize-$t.png" $t
  Save-Png "Square44x44Logo.targetsize-${t}_altform-unplated.png" $t
  Save-Png "Square44x44Logo.targetsize-${t}_altform-lightunplated.png" $t
}
$src.Dispose()
Write-Output "MSIX images written to $out"
