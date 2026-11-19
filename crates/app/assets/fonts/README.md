# Bundled fonts

Interface fonts for the themes plan (`docs/design/themes-plan.md`, Fonts).
Not wired into the app yet: the plan's Fonts step embeds them with
`include_bytes!` and registers the chosen one with `cx.text_system().add_fonts`.

Each family folder holds static weights (400 Regular, 500 Medium, 600 SemiBold,
700 Bold, unless noted) and its licence. All are under the SIL Open Font
License 1.1, which allows bundling them in the app; keep each `OFL.txt` (and
`TRADEMARKS.md` where present) next to its fonts.

| Family | Folder | Weights | Version and source |
|---|---|---|---|
| Inter (default) | `inter/` | 400 500 600 700 | Inter 4.1, npm `inter-ui` 4.1.1 (`web/*.woff2`, unpacked to TTF) |
| Geist | `geist/` | 400 500 600 700 | npm `geist` 1.9.2 (`dist/fonts/geist-sans`) |
| IBM Plex Sans | `ibm-plex-sans/` | 400 500 600 700 | npm `@ibm/plex-sans` 1.1.0 (`fonts/complete/woff2`, unpacked to TTF) |
| Source Serif 4 | `source-serif-4/` | 400 600 700 | npm `source-serif` 4.5.1 (`OTF/`); the family has no 500, so Medium falls back to Regular |
| Figtree | `figtree/` | 400 500 600 700 | google/fonts `ofl/figtree`, static weights cut from the variable font |
| Atkinson Hyperlegible Next | `atkinson-hyperlegible-next/` | 400 500 600 700 | google/fonts `ofl/atkinsonhyperlegiblenext`, cut from the variable font |
| Space Grotesk | `space-grotesk/` | 400 500 600 700 | google/fonts `ofl/spacegrotesk`, cut from the variable font |
| Plus Jakarta Sans | `plus-jakarta-sans/` | 400 500 600 700 | google/fonts `ofl/plusjakartasans`, cut from the variable font |
| Manrope | `manrope/` | 400 500 600 700 | google/fonts `ofl/manrope`, cut from the variable font |
| Google Sans | `google-sans/` | 400 500 700 | googlefonts/googlesans release v14.000, `android/static/GoogleSansText-*`, unmodified. Family name is "Google Sans Text" (the small-size optical cut, right for UI text). No 600: SemiBold falls back to Medium |
| Google Sans Flex | `google-sans-flex/` | variable, 100-1000 | googlefonts/googlesans-flex release v5.000, unmodified variable font. Kept as shipped because of Google's trademark terms (see `TRADEMARKS.md`). Check that GPUI picks weights from a variable font; if it does not, drop Google Sans Flex or ask Google's terms first before cutting statics |

"Cut from the variable font" means each weight was made with fontTools'
instancer (other axes at their defaults) and renamed to the usual family /
style names. The OFL allows this; none of those fonts has a Reserved Font
Name.
