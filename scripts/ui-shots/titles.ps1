# List visible top-level window titles matching a pattern.
param([string]$Pattern)
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public class T {
  public delegate bool P(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(P f, IntPtr l);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  public static List<string> All() { var o = new List<string>();
    EnumWindows((h, l) => { if (IsWindowVisible(h)) { var s = new StringBuilder(512); GetWindowText(h, s, 512); if (s.Length > 0) o.Add(s.ToString()); } return true; }, IntPtr.Zero); return o; }
}
"@
[T]::All() | Where-Object { $_ -match $Pattern }
