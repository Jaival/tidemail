# Phase 3 slice 0: screenshot checks for the editor spike (crates/app/examples/editor_spike.rs).
# Usage: spike.ps1 [-Exe target\release\examples\editor_spike.exe]
param([string]$Exe = "target\release\examples\editor_spike.exe")
$ErrorActionPreference = "Stop"
$out = "$env:TEMP\tidemail-shots\spike"
New-Item -ItemType Directory -Force $out | Out-Null

Add-Type @"
using System; using System.Runtime.InteropServices;
public class S {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte s, uint f, UIntPtr e);
  [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] i, int size);
  public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT { public ushort vk, scan; public uint flags, time; public IntPtr extra; }
  [StructLayout(LayoutKind.Explicit, Size = 40)] struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEYBDINPUT ki; }
  // Types text as UTF-16 units (KEYEVENTF_UNICODE): the path the emoji panel and IMEs commit through.
  public static void Unicode(string s) {
    foreach (char c in s) {
      var d = new INPUT { type = 1, ki = new KEYBDINPUT { scan = c, flags = 4 } };
      var u = new INPUT { type = 1, ki = new KEYBDINPUT { scan = c, flags = 4 | 2 } };
      SendInput(2, new[] { d, u }, Marshal.SizeOf(typeof(INPUT)));
      System.Threading.Thread.Sleep(30);
    }
  }
}
"@
[S]::SetProcessDPIAware() | Out-Null
Add-Type -AssemblyName System.Drawing, System.Windows.Forms

$p = Start-Process $Exe -PassThru
for ($i = 0; $i -lt 100 -and $p.MainWindowHandle -eq 0; $i++) { Start-Sleep -Milliseconds 100; $p.Refresh() }
$h = $p.MainWindowHandle
if ($h -eq 0) { throw "no window" }
Start-Sleep -Milliseconds 1200
[S]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [S]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
[S]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 300

function Shot($n) {
  $r = New-Object S+RECT; [S]::GetWindowRect($h, [ref]$r) | Out-Null
  $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
  $g = [System.Drawing.Graphics]::FromImage($bmp); $dc = $g.GetHdc()
  [S]::PrintWindow($h, $dc, 2) | Out-Null; $g.ReleaseHdc($dc)
  $f = "$out\$n.png"; $bmp.Save($f); $g.Dispose(); $bmp.Dispose(); Write-Output $f
}
function Keys($k) { [System.Windows.Forms.SendKeys]::SendWait($k); Start-Sleep -Milliseconds 200 }

Shot "1-start"
# Unicode / emoji / CJK through SendInput.
Keys "{ENTER}{ENTER}"
[S]::Unicode("Emoji: " + [char]::ConvertFromUtf32(0x1F600) + " " + [char]::ConvertFromUtf32(0x1F44D) + [char]::ConvertFromUtf32(0x1F3FD) + " cafe" + [char]0x0301 + " " + [char]0x4E2D + [char]0x6587 + " end")
Start-Sleep -Milliseconds 300
Shot "2-unicode"
# Backspace over the last word, type three words, Ctrl+Left, then Ctrl+Shift+Left selects "beta ".
Keys "{BACKSPACE}{BACKSPACE}{BACKSPACE}"
[S]::Unicode(" alpha beta gamma")
Keys "^{LEFT}^+{LEFT}"
Shot "3-word-select"
# Brackets, quotes and Enter on an indented list line: no closer, no auto-indent expected.
Keys "{END}{ENTER}"
[S]::Unicode("  - item one (note")
Keys "{ENTER}"
[S]::Unicode("next ""quoted"" [x] {y}")
Start-Sleep -Milliseconds 300
Shot "4-brackets-indent"
Stop-Process -Id $p.Id -Force
