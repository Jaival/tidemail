# Font pass (1G-U3b): every interface font paired with a code font (11 runs), in
# all 3 presets, dark and light (preset colours): list, plain-text mail (inbox row 3),
# HTML mail with code (row 20), Fonts tab, its Sample.
# Usage: fonts.ps1 [-Only tide] [-Mode dark] [-Font geist] [-Tag fo] [-Exe ...] [-BenchDb ...]
# Shots: %TEMP%\tidemail-shots\shots\<tag>-<preset>-<mode>-<ui>-NN-state.png
param([string]$Only = "", [string]$Mode = "", [string]$Font = "", [string]$Tag = "fo",
      [string]$Exe = "target\release\tidemail.exe", [string]$BenchDb = "bench.db")
$open = @{ tide = "send:j"; paper = "send:j"; command = "send:{ENTER}" }
$ui = "inter", "geist", "ibm-plex-sans", "figtree", "atkinson-hyperlegible-next", "source-serif-4",
  "google-sans", "space-grotesk", "google-sans-flex", "manrope", "plus-jakarta-sans"
$code = "obel-mono", "jetbrains-mono", "geist-mono", "ibm-plex-mono", "source-code-pro",
  "atkinson-hyperlegible-mono", "space-mono", "google-sans-code", "commit-mono", "monaspace-neon", "obel-mono"
foreach ($preset in "tide", "command", "paper") {
  if ($Only -and $preset -ne $Only) { continue }
  foreach ($md in "dark", "light") {
    if ($Mode -and $md -ne $Mode) { continue }
    for ($i = 0; $i -lt $ui.Count; $i++) {
      if ($Font -and $ui[$i] -ne $Font -and $code[$i] -ne $Font) { continue }
      $name = "$Tag-$preset-$md-$($ui[$i])"
      $steps = @(
        "wait:1200", "shot:01-list",
        $open[$preset], "wait:600", "send:j", "wait:300", "send:j", "wait:1200", "shot:02-plain",
        "send:jjjjjjjjjjjjjjjjj", "wait:1800", "shot:03-code",
        "send:^k", "wait:400", "text:fonts", "wait:300", "send:{ENTER}", "wait:900", "shot:04-fonts", "wheel:1030,500,-10", "shot:05-sample",
        "send:{ESC}", "wait:300"
      ) -join "|"
      & $PSScriptRoot\drive.ps1 -Preset $preset -Mode $md -Name $name -Exe $Exe -BenchDb $BenchDb `
        -Steps $steps -UiFont $ui[$i] -MonoFont $code[$i] | Out-Null
      Write-Output "done $name"
    }
  }
}
