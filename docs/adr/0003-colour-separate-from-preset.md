# 0003: Colour is separate from the layout preset

Date: 2026-11-20. Status: accepted. Amends [0002](0002-layout-presets.md).

## Context

ADR 0002 gives each preset one dark and one light palette (Tide teal, Command
violet, Paper terracotta), so picking a layout also picks the colours. Users
want to choose colours without changing the layout (plan:
`docs/design/themes-plan.md`).

## Decision

- **A colour choice per mode.** Settings hold `colorsDark` and `colorsLight`,
  each one of:
  - `{"kind":"preset"}`: the active preset's own palette for that mode (the
    ADR 0002 palettes, now called "Preset colours"). Default.
  - `{"kind":"theme","name":"<id>"}`: a built-in theme. Dark: `midnight`,
    `graphite`, `fjord`, `forest`, `rose`, `high-contrast`. Light:
    `daylight`, `linen`.
  - `{"kind":"accent"}`: derived from `accent`
    (`{"hue":0-359,"chroma":"normal"|"muted","tint":true|false}`).
- **Light / Dark / System** (`theme`) still picks which of the two is used.
- **The preset keeps its layout and its display font.** Changing the preset
  with "Preset colours" chosen changes the colours too; with a theme or an
  accent chosen it does not.
- **The accent palette is derived, not stored.** Fixed OKLCH lightness
  targets per mode, then a contrast pass that moves lightness until text
  >= 7:1, muted text >= 4.5:1, accent >= 3:1 and button text on the accent
  >= 4.5:1. Built-in themes are tested against the same rules (High contrast
  against its own).
- **Unchanged from 0002:** the mail body stays on the light paper surface in
  every theme; account colours stay as they are (they mark accounts).
- **Fonts** are a separate choice too (`uiFont`, `monoFont`, `maskSecrets`,
  `headingsUseUiFont`). The interface font replaces the body font in every
  preset; Paper's Georgia display font stays unless `headingsUseUiFont`.
- **Unknown or malformed values load as defaults**, field by field, so a bad
  value never breaks launch or resets the other settings.

## Consequences

- `PresetSpec::{dark, light}` remain, as the "Preset colours" choice. Seven
  of their values moved a few percent in lightness so the presets pass the
  same contrast rules (list in `docs/HANDOFF.md`, Phase 1G-U3 status).
- `theme::apply` takes the settings, not just the mode and preset.
- Screenshot checks grow from 3 presets × 2 modes to 3 presets × (8 themes +
  accents), in the modes each theme has.
- Rejected: theme files the user can import or share, per-account themes,
  colouring the HTML mail body (out of scope in the plan).
