# Drive tidemail: launch on a db copy with preset+mode, run steps, screenshot.
# Usage: drive.ps1 -Preset tide -Mode dark -Name tide-dark -Steps "wait:1500|shot:list|send:j|wait:800|shot:reader"
# Steps: send:<SendKeys string>, text:<literal text>, wait:<ms>, shot:<name>, click:<x>,<y> (window px), fg
param([string]$Preset, [string]$Mode, [string]$Name, [string]$Steps,
      [string]$Exe = "target\release\tidemail.exe", [string]$BenchDb = "bench.db", [switch]$Fresh)
$ErrorActionPreference = "Stop"
$dir = "$env:TEMP\tidemail-shots\db"
$out = "$env:TEMP\tidemail-shots\shots"
New-Item -ItemType Directory -Force $dir, $out | Out-Null
if ($Fresh -or -not (Test-Path "$dir\mail.db")) {
  Remove-Item "$dir\mail.db*" -ErrorAction SilentlyContinue
  Copy-Item $BenchDb "$dir\mail.db" -Force
}
Set-Content "$dir\settings.json" "{`"theme`":`"$Mode`",`"prefetch`":10,`"preset`":`"$Preset`"}"

Add-Type @"
using System; using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int x, int y, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte s, uint f, UIntPtr e);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  public struct RECT { public int L, T, R, B; }
}
"@
[W]::SetProcessDPIAware() | Out-Null
Add-Type -AssemblyName System.Drawing, System.Windows.Forms

$p = Start-Process $Exe -ArgumentList "--db", "$dir\mail.db" -PassThru
Start-Sleep -Milliseconds 300
for ($i = 0; $i -lt 100 -and $p.MainWindowHandle -eq 0; $i++) { Start-Sleep -Milliseconds 100; $p.Refresh() }
$h = $p.MainWindowHandle
if ($h -eq 0) { throw "no window (exited: $($p.HasExited))" }
function Fg { [W]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [W]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero); [W]::SetForegroundWindow($h) | Out-Null; Start-Sleep -Milliseconds 150 }
Start-Sleep -Milliseconds 1500
[W]::SetWindowPos($h, [IntPtr]::Zero, 40, 40, 1440, 900, 0x0044) | Out-Null
Start-Sleep -Milliseconds 300
Fg
function Shot($n) {
  if ($p.HasExited) { throw "app exited before shot $n (code $($p.ExitCode))" }
  $r = New-Object W+RECT; [W]::GetWindowRect($h, [ref]$r) | Out-Null
  $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
  $g = [System.Drawing.Graphics]::FromImage($bmp); $dc = $g.GetHdc()
  [W]::PrintWindow($h, $dc, 2) | Out-Null; $g.ReleaseHdc($dc)
  $f = "$out\$Name-$n.png"; $bmp.Save($f); $g.Dispose(); $bmp.Dispose(); Write-Output $f
}
foreach ($s in $Steps.Split("|")) {
  $k, $v = $s.Split(":", 2)
  switch ($k) {
    "send" { if ([W]::GetForegroundWindow() -ne $h) { Fg }; [System.Windows.Forms.SendKeys]::SendWait($v); Start-Sleep -Milliseconds 120 }
    "text" { if ([W]::GetForegroundWindow() -ne $h) { Fg }
             foreach ($c in $v.ToCharArray()) { $e = [regex]::Replace([string]$c, '[+^%~(){}\[\]]', '{$0}'); [System.Windows.Forms.SendKeys]::SendWait($e); Start-Sleep -Milliseconds 60 } }
    "wait" { Start-Sleep -Milliseconds ([int]$v) }
    "shot" { Shot $v }
    "fg" { Fg }
    "click" { $x, $y = $v.Split(","); $r = New-Object W+RECT; [W]::GetWindowRect($h, [ref]$r) | Out-Null
              [W]::SetCursorPos($r.L + [int]$x, $r.T + [int]$y) | Out-Null; Start-Sleep -Milliseconds 80
              [W]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); [W]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 200 }
  }
}
Stop-Process -Id $p.Id -Force; $p.WaitForExit(10000) | Out-Null
