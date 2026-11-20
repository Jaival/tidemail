# Theme pass (1G-U3): every built-in theme in its mode and three accents in both modes,
# in all 3 presets: list, reader, search, star toast, Appearance overlay, after Esc.
# Usage: themes.ps1 [-Only tide] [-Colors midnight] [-Tag th] [-Exe ...] [-BenchDb ...]
# Shots: %TEMP%\tidemail-shots\shots\<tag>-<preset>-<colors>-<mode>-NN-state.png
param([string]$Only = "", [string]$Colors = "", [string]$Tag = "th",
      [string]$Exe = "target\release\tidemail.exe", [string]$BenchDb = "bench.db")
$open = @{ tide = "send:j"; paper = "send:j"; command = "send:{ENTER}" }
$combos = @(
  @("midnight", "dark", ""), @("graphite", "dark", ""), @("fjord", "dark", ""), @("forest", "dark", ""),
  @("rose", "dark", ""), @("high-contrast", "dark", ""), @("daylight", "light", ""), @("linen", "light", ""),
  @("accent", "dark", "268,normal,true"), @("accent", "light", "268,normal,true"),
  @("accent", "dark", "150,muted,true"), @("accent", "light", "150,muted,true"),
  @("accent", "dark", "25,normal,false"), @("accent", "light", "25,normal,false")
)
foreach ($preset in "tide", "command", "paper") {
  if ($Only -and $preset -ne $Only) { continue }
  foreach ($c in $combos) {
    $col, $mode, $accent = $c
    if ($Colors -and $col -ne $Colors) { continue }
    $label = if ($accent) { "accent" + $accent.Split(",")[0] } else { $col }
    $name = "$Tag-$preset-$label-$mode"
    $steps = @(
      "wait:1200", "shot:01-list",
      $open[$preset], "wait:1500", "shot:02-reader",
      "send:s", "wait:500", "shot:03-toast", "send:^z", "wait:400",
      "send:/", "wait:300", "text:budget", "wait:800", "shot:04-search", "send:{ESC}", "wait:300",
      "send:^k", "wait:400", "text:accent col", "wait:300", "send:{ENTER}", "wait:700", "shot:05-appearance",
      "send:{ESC}", "wait:500", "shot:06-after-esc"
    ) -join "|"
    $opt = @{ Preset = $preset; Mode = $mode; Name = $name; Exe = $Exe; BenchDb = $BenchDb; Steps = $steps; Colors = $col }
    if ($accent) { $opt.Accent = $accent }
    & $PSScriptRoot\drive.ps1 @opt | Out-Null
    Write-Output "done $name"
  }
}
