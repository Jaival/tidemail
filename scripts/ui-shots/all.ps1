# Full screenshot pass: 3 presets x 2 modes, 17 states each. Run from the repo root.
# Usage: all.ps1 [-Only tide-dark] [-Tag base] [-Exe target\release\tidemail.exe] [-BenchDb bench.db]
# Then: sheet.ps1 base-tide-dark sheet.png 3 0.4 (contact sheet of one combination).
param([string]$Only = "", [string]$Tag = "base", [string]$Exe = "target\release\tidemail.exe",
      [string]$BenchDb = "bench.db")
$open = @{ tide = "send:j"; paper = "send:j"; command = "send:{ENTER}" }
foreach ($preset in "tide", "command", "paper") {
  foreach ($mode in "dark", "light") {
    $name = "$Tag-$preset-$mode"
    if ($Only -and "$preset-$mode" -ne $Only) { continue }
    $steps = @(
      "wait:1200", "shot:01-list",
      $open[$preset], "wait:1500", "shot:02-reader",
      "send:^k", "wait:500", "shot:03-palette", "send:{ESC}", "wait:400", "shot:04-after-esc",
      "send:l", "wait:600", "shot:05-label", "send:{ESC}", "wait:300",
      "send:v", "wait:500", "shot:06-move", "send:{ESC}", "wait:300",
      "send:g", "send:l", "wait:500", "shot:07-go", "send:{ESC}", "wait:300",
      "send:s", "wait:500", "shot:08-toast", "send:^z", "wait:500", "shot:09-undone",
      "send:/", "wait:300", "text:budget", "wait:800", "shot:10-search",
      "send:^k", "wait:400", "shot:11-ctrlk-in-search", "send:{ESC}", "wait:300",
      "send:{ESC}", "wait:400", "shot:12-search-esc", "send:j", "wait:800", "shot:13-j-after-esc",
      "send:/", "wait:300", "text:zzqxw", "wait:800", "shot:14-no-results", "send:{ESC}", "wait:300",
      "send:g", "send:l", "wait:400", "text:drafts", "wait:300", "send:{ENTER}", "wait:1200", "shot:15-drafts",
      "send:/", "wait:300", 'text:"a*(', "wait:800", "shot:16-search-odd", "send:{ESC}", "wait:300",
      "send:g", "send:l", "wait:400", "text:spam", "wait:300", "send:{ENTER}", "wait:1200", "shot:17-spam"
    ) -join "|"
    & $PSScriptRoot\drive.ps1 -Preset $preset -Mode $mode -Name $name -Exe $Exe -BenchDb $BenchDb -Steps $steps | Out-Null
    Write-Output "done $name"
  }
}
