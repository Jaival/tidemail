# Bundled fonts

Fonts for the themes plan (`docs/design/themes-plan.md`, Fonts). All 21 are
embedded in the exe (`src/fonts.rs`, `include_bytes!`; about +12 MB, no
measurable cold-launch cost, see `docs/HANDOFF.md`, 1G-U3b). Only the chosen
interface and code font are registered at startup (`cx.text_system().add_fonts`);
the Fonts tab registers the rest. The reader's webview gets the code font from
`http://tidemail-cid.localhost/font/<family>` (first file of the family).

The nine other code fonts were copied unmodified from Forkline
(`crates/fl-app/fonts`, commit `f1c06da`) in Phase 1G-U3b; the projects below are
from each `OFL.txt` copyright line.

Each family folder holds static weights (400 Regular, 500 Medium, 600 SemiBold,
700 Bold, unless noted) and its licence. All are under the SIL Open Font
License 1.1 (checked: every `OFL.txt` is the 1.1 text; test
`the_table_has_21_fonts_with_unique_ids_and_ofl_licences`), which allows bundling them in the app; keep each `OFL.txt` (and
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
| Google Sans | `google-sans/` | 400 500 700 | googlefonts/googlesans release v14.000, `android/static/GoogleSansText-*`, unmodified. Family name is "Google Sans Text"; DirectWrite registers it as "Google Sans Text Text" (the small-size optical cut, right for UI text). No 600: SemiBold falls back to Medium |
| Obel Mono (default code font) | `obel-mono/` | variable, `wght` | Obel Mono 1.004 (Typographic Systems, 2026), `ObelMono[wght].ttf` and `ObelMonoMasked[wght].ttf` (the build with `ss01`, secret masking, on by default) unmodified; copied from Forkline |
| JetBrains Mono | `jetbrains-mono/` | variable, `wght` | `JetBrainsMono[wght].ttf`; project github.com/JetBrains/JetBrainsMono |
| Geist Mono | `geist-mono/` | variable, `wght` | `GeistMono[wght].ttf`; project github.com/vercel/geist-font |
| IBM Plex Mono | `ibm-plex-mono/` | 400 500 600 700 | static TTFs; Reserved Font Name "Plex" (used unmodified, so the name may stay) |
| Source Code Pro | `source-code-pro/` | variable, `wght` | `SourceCodePro[wght].ttf`; Reserved Font Name "Source" (unmodified) |
| Atkinson Hyperlegible Mono | `atkinson-hyperlegible-mono/` | variable, `wght` | `AtkinsonHyperlegibleMono[wght].ttf`; project github.com/googlefonts/atkinson-hyperlegible-next-mono |
| Space Mono | `space-mono/` | 400 700 | the family has only regular and bold; project github.com/googlefonts/spacemono |
| Google Sans Code | `google-sans-code/` | variable, `wght` | `GoogleSansCode[wght].ttf`; project github.com/googlefonts/googlesans-code |
| Commit Mono | `commit-mono/` | 400 | `CommitMono-400-Regular.ttf`; registers as family "CommitMono 400" (one family per weight in the release); bold is synthesized |
| Monaspace Neon | `monaspace-neon/` | variable | `Monaspace Neon Var.ttf`; project github.com/githubnext/monaspace; registers as "Monaspace Neon Var"; Reserved Font Name "Monaspace" (unmodified) |
| Google Sans Flex | `google-sans-flex/` | variable, 100-1000 | googlefonts/googlesans-flex release v5.000, unmodified variable font. Kept as shipped because of Google's trademark terms (see `TRADEMARKS.md`). GPUI picks its weights (test `every_family_registers_under_its_name_with_a_bold`) |

"Cut from the variable font" means each weight was made with fontTools'
instancer (other axes at their defaults) and renamed to the usual family /
style names. The OFL allows this; none of those fonts has a Reserved Font
Name.
