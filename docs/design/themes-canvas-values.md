# Theme values from the design canvas

Copied 2026-11-20 from the "Theme options" and "Custom accent" boards
(`Tidemail & Forkline — before after.html`, a self-unpacking bundle: the
boards are gzip+base64 pages in the `__bundler/manifest` script). Saved here
so nobody has to unpack the file again. The code is the source of truth:
`crates/app/src/theme.rs` (built-in themes) and `crates/app/src/theme/derive.rs`
(accent).

## Built-in themes (CSS HSL)

The canvas calls the warm light theme "Paper"; Tidemail ships it as **Linen**
(Paper is a layout preset).

| Theme | Mode | bg | surface | raised | selected | border | fg | muted | accent | accentFg | star |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Midnight | dark | 225,12%,10% | 225,13%,7.5% | 225,10%,15% | 228,38%,17.5% | 225,9%,16.5% | 220,14%,92% | 222,8%,60% | 228,92%,69% | #fff | 42,95%,58% |
| Graphite | dark | 30,4%,10% | 30,4%,7.5% | 30,4%,15.5% | 36,32%,16% | 30,4%,17% | 40,10%,91% | 35,5%,61% | 36,92%,60% | 30,40%,10% | 48,95%,60% |
| Fjord | dark | 218,22%,15% | 220,23%,12% | 218,20%,20.5% | 205,35%,23% | 218,18%,22% | 215,25%,90% | 215,14%,66% | 193,60%,62% | 218,30%,12% | 40,85%,66% |
| Forest | dark | 160,14%,9% | 160,16%,6.5% | 160,12%,14.5% | 155,35%,16% | 160,10%,16% | 150,12%,91% | 155,7%,61% | 152,60%,52% | 160,40%,8% | 45,90%,60% |
| Rosé | dark | 300,10%,10% | 300,11%,7.5% | 300,9%,15.5% | 330,35%,18% | 300,8%,17% | 320,15%,92% | 310,7%,63% | 336,85%,70% | 336,50%,12% | 42,95%,62% |
| Daylight | light | 0,0%,100% | 225,25%,97% | 225,20%,92.5% | 228,95%,95% | 225,14%,90% | 225,25%,12% | 222,10%,42% | 228,78%,56% | #fff | 38,95%,46% |
| Linen ("Paper") | light | 40,30%,98% | 40,22%,94% | 40,16%,88% | 178,40%,90% | 40,14%,86% | 30,15%,14% | 30,8%,38% | 178,65%,30% | #fff | 36,95%,42% |
| High contrast | dark | 0,0%,0% | 0,0%,0% | 0,0%,16% | 55,100%,16% | 0,0%,55% | 0,0%,100% | 0,0%,82% | 55,100%,55% | #000 | 55,100%,55% |

Mapping to Tidemail's `Palette`: bg = `bg`, surface = `chrome`, selected =
`selected`, star = `warning`. `raised` has no Tidemail token. No danger
colour on the board; Tidemail picks one per mode.

## Custom accent (OKLCH targets, `k` = 1 with "tint backgrounds", else 0)

| Token | Dark L, C | Light L, C |
|---|---|---|
| bg | 0.19, 0.014k | 0.995, 0.002k |
| surface | 0.165, 0.014k | 0.972, 0.008k |
| raised | 0.245, 0.016k | 0.935, 0.012k |
| border | 0.27, 0.014k | 0.91, 0.01k |
| selected | 0.29, 0.06 | 0.95, 0.035 |
| fg | 0.94, 0.006k | 0.22, 0.02k |
| muted | 0.71, 0.014k | 0.50, 0.016k |
| accent | 0.70, 0.16 | 0.55, 0.17 |
| accent text | 0.78, 0.13 | 0.50, 0.17 |

Button text: white if white on the accent is >= 3.5:1, else OKLCH
(0.2, 0.03, hue). The board has no "muted" chroma level; Forkline chose
accent 0.09 / 0.10 and selection 0.035 / 0.02 (dark / light).

Swatches (OKLCH 0.66, 0.16): Indigo 268, Blue 250, Sky 230, Cyan 210,
Teal 185, Green 150, Lime 125, Amber 75, Orange 50, Red 25, Pink 355,
Violet 300.

## Prior art

Forkline (`D:\Projects\forkline`, G2 steps 2-6) built the same themes, accent
derivation, Appearance dialog and fonts in GPUI with the same `gpui-kit`
0.9.0. Its lessons used here:

- GPUI passes font features down to every text run below an element, so
  "Hide secrets" switches the family to Obel Mono Masked (the foundry's build
  with `ss01` on by default) instead of setting `ss01`.
- Contrast pass: move OKLCH L in 0.005 steps away from the backgrounds until
  the rule passes; reduce chroma (binary search) for colours outside sRGB.
