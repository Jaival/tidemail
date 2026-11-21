# Plan: colour themes, custom accent and fonts (Phase 1G-U3)

Date: 2026-11-17. Status: built 2026-11-20 on branch `phase1g-u3`; the font
picker, the 19 non-default fonts and the reader's code font on branch
`phase1g-u3b` (see `docs/HANDOFF.md`, "Phase 1G-U3 status" and "Phase 1G-U3b
status"). Board values as
built: `docs/design/themes-canvas-values.md`.
Mockups: the "Theme options" and "Custom accent" boards on the design canvas
(https://claude.ai/artifact/KB3eRiBLYpyWRS8SiJBpcA).

## When

Start after Phase 1G-U2 (visual pass) is merged. Until then nothing here
changes the 1G-U1 / 1G-B work. The phase does not depend on Phase 2, so it can
also wait until after Phase 2 if real-account work comes first.

## Goal

Each user picks how the app looks, without changing the layout:

- a **named theme** (eight built in), or
- a **custom accent**: the user picks one colour and the rest of the palette is
  worked out from it, for dark and light, and
- **fonts**: an interface font and a code font, from eleven interface and
  ten code fonts.

## Decisions

- **Colour is separate from layout.** ADR 0002 ties one dark and one light
  palette to each preset (Tide teal, Command violet, Paper terracotta). Keep
  those as each preset's default ("Preset colours"), and let the user override
  them with a theme or an accent. Write this as ADR 0003, which amends 0002.
- **Built-in themes.** Dark: Midnight (slate, indigo), Graphite (warm grey,
  amber), Fjord (soft blue-grey, cyan), Forest (green-black, emerald), Rosé
  (plum, pink), High contrast (black, white text, yellow). Light: Daylight
  (white, indigo), Linen (off-white, deep teal). "Linen" avoids a clash with
  the Paper preset. Values: the canvas boards; copy them into `theme.rs`.
- **System mode picks between two choices.** Settings hold one dark choice
  and one light choice. Light / Dark / System still chooses which one is used.
  Defaults: the preset's own palettes.
- **Custom accent is one hue plus two switches.** Stored: hue (0-359), chroma
  level (normal / muted), and "tint backgrounds" (on / off, off = neutral
  grey). Lightness of every token comes from fixed per-mode targets, so any
  hue gives a usable theme.
- **Readability is enforced, not hoped for.** After deriving, check WCAG
  ratios and nudge lightness until they pass: text on background >= 7:1, muted
  text >= 4.5:1, accent used as text or icon >= 3:1, button text on accent
  >= 4.5:1 (white or near-black, whichever passes). High contrast skips the
  derivation and uses fixed values.
- **The mail body keeps its light paper surface** in every theme (ADR 0002).
- **Account colours stay as they are.** They mark accounts, not the theme.
  Check that each account colour still reads on every built-in background.

## Fonts

Each user picks an **interface font** (lists, sidebar, reader chrome, plain-text
mail) and a **code font** (plain-text mail when "show plain text in a fixed-width
font" is on, raw headers). Defaults: Inter and Obel Mono. Inter replaces
Segoe UI Variable, which 1G-U1 step 2 plans as the UI font.

| Font | Used for | Why offer it |
|---|---|---|
| **Inter** (default) | interface | built for screens; very clear at 12-14 px |
| Geist | interface | tighter and more modern |
| IBM Plex Sans | interface | a little more character |
| Figtree | interface | friendly and round, still compact |
| Atkinson Hyperlegible Next | interface | made for low vision; pairs with High contrast |
| Source Serif 4 | interface (serif) | for reading-heavy use |
| Google Sans | interface | Google's own UI face; round and open |
| Space Grotesk | interface | quirky and technical; strongest in headings |
| Google Sans Flex | interface | adjustable Google Sans (width, roundness); could replace Google Sans |
| Manrope | interface | modern, geometric, clean |
| Plus Jakarta Sans | interface | modern with a little warmth |
| JetBrains Mono | code | tall letters, clear `0` / `O` and `1` / `l` / `I` |
| Geist Mono | code | matches Geist |
| IBM Plex Mono | code | matches IBM Plex Sans |
| Source Code Pro | code | light and classic |
| Atkinson Hyperlegible Mono | code | matches Atkinson; the most legible |
| Space Mono | code | retro and wide; pairs with Space Grotesk. Only regular and bold |
| Google Sans Code | code | pairs with Google Sans; Google uses it for code in Gemini |
| Commit Mono | code | neutral and calm, made for reading code |
| Monaspace Neon | code | GitHub's own code font family (Neon is its neutral member) |
| **Obel Mono** (default) | code | made for terminals, code and agents (Typographic Systems, 2026) |

All twenty-one are bundled and under the SIL Open Font License. Samples: the
"Font options" board on the canvas; the "after" boards use the defaults and
have a font switch in their Tweaks.

- **Presets keep their display font.** Paper uses Georgia for subjects and list
  titles (ADR 0002). The user's interface font replaces the body font in every
  preset; a switch "Use my font for headings too" replaces Paper's display font
  as well.
- **Hide secrets in code** (on by default). With Obel Mono as the code font,
  its `ss01` feature shows tokens and secrets as mask dots: a known key
  prefix (`ghp_`, `sk-ant-`, `AKIA` ...) keeps four characters and masks the
  rest, and a value after a name that says secret (`WIFI_PASSWORD=`,
  `"password": "`) keeps its first four characters. It applies wherever the
  code font is used (plain-text mail in fixed width, raw headers). It changes
  only what is drawn: copy, search and the clipboard still get the real
  text. The switch shows only while the code font is Obel Mono. HTML mail is
  out of its reach (the sender's fonts render it).
- **HTML mail keeps the sender's fonts.** The choice applies to the native UI
  and to plain-text mail only.
- **A fixed set, not any font on the PC.** Every font is tested against the
  fixed row heights and the contrast rules; an arbitrary installed font is
  not. The Windows system font is only the fallback.
- **The fonts are embedded in the exe**, with their licence texts. Only the two
  chosen fonts are registered at startup (`cx.text_system().add_fonts`); the
  others load when the picker previews them. Expect the exe to grow by about
  8-12 MB. The default font now loads on every launch, so measure cold launch
  before and after.
- **Fixed row heights stay fixed.** Virtual lists need one row height, so every
  text style sets an explicit line height. A font with taller metrics then
  cannot change a row height or clip text.
- **An unknown font name falls back to the default** (for example a
  hand-edited setting).
- **Text size is a separate, open question** (Small / Default / Large). Say if
  it should join this phase.

## Settings (`settings.json`)

Add, all optional. Colour defaults keep the preset's look; font defaults are
Inter and Obel Mono:

```json
{
  "colorsDark":  { "kind": "preset" },
  "colorsLight": { "kind": "theme", "name": "daylight" },
  "accent":      { "hue": 268, "chroma": "normal", "tint": true },
  "uiFont":      "inter",
  "monoFont":    "obel-mono",
  "maskSecrets": true,
  "headingsUseUiFont": false
}
```

`kind` is `preset` | `theme` (with `name`) | `accent` (uses `accent`). Unknown
names fall back to `preset`, so an old or edited file never breaks launch.
A font is a name from the Fonts table; unknown names fall back to the
default.

## Steps

Each step builds, passes clippy and tests on its own, and gets a note in
`docs/HANDOFF.md`.

1. **ADR 0003 and settings.** Write the ADR. Add the settings fields and
   `AppState::set_colors(mode, choice)`; emit `SettingsChanged`.
2. **Theme table.** `theme.rs`: `Palette` gets one constructor per built-in
   theme. `apply` picks preset palette, built-in theme or derived accent, then
   writes it over the `gpui-component` theme as 1G-U1 does.
3. **Accent derivation.** `theme/derive.rs`: hue + switches + mode ->
   `Palette`, in OKLCH (convert to `Hsla` at the end). Token targets as on the
   canvas (dark: background L 0.19, surface 0.165, raised 0.245, border 0.27,
   selected 0.29 at chroma 0.06, text 0.94, muted 0.71, accent 0.70 at chroma
   0.16). Contrast pass as above. Pure function, no GPUI types in its tests.
4. **Picker.** An "Appearance" overlay (`Overlay::Appearance`) opened from the
   Ctrl+K palette ("Theme…", "Accent colour…", "Fonts…"): theme cards with a small live
   preview, 12 accent swatches, a hue slider, the two switches and a contrast
   readout. Changes apply live; Esc restores what was there when it opened.
5. **Fonts.** `fonts.rs`: the table above, embedded font bytes, registration
   of the chosen fonts at startup, explicit line heights on every text style. A Fonts section in the Appearance overlay
   with a live sample (a list row, a subject line, plain-text mail), and the
   "Hide secrets in code" switch (`ss01` on the code text style). First check
   that GPUI on Windows applies `ss01` through DirectWrite; if it does not,
   ship Obel Mono Masked as a second font for the switch instead.
6. **Checks.** Screenshots: 3 presets x (8 themes + 3 accents) x list and
   reader, dark and light where it applies; each bundled font once in Tide. Full `bench-all`: theme apply must
   not move cold launch (derivation is microseconds; the picker is built on
   first open only).

## Tests

- `derive.rs`: for every hue in steps of 5 degrees, both modes, both tint
  settings, every contrast rule passes.
- Every built-in theme passes the same rules (High contrast against its own,
  higher targets).
- Settings: missing fields, unknown theme names and out-of-range hues load as
  defaults; so do unknown font names.
- Hide secrets: with the switch on, a plain-text line holding `sk-ant-` plus
  a key draws mask dots after the first four characters; copying it gives the
  full key; with another code font the switch is hidden.
- Each bundled font: list rows and the title bar lay out at their fixed
  heights with no clipped text.

## Out of scope

- Importing or sharing theme files.
- Text size (open question, see Fonts).
- Colouring the HTML mail body.
- Per-account themes.
