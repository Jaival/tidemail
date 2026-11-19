# 0002: Three layout presets (Tide, Command, Paper)

Date: 2026-11-11. Status: accepted.

## Context

The Phase 1G-A UI is a plain three-pane shell on the default `gpui-component` theme. Four directions were mocked up. Three were kept, and the user chooses between them at run time instead of the project picking one.

## Decision

The app ships three presets. A preset is **data** (`PresetSpec`), not a separate shell: one `Shell`, one `ThreadList`, one `Reader`, each reading the spec of the active preset.

| | Tide (default) | Command | Paper |
|---|---|---|---|
| Navigation | Sidebar, 208 px | Tabs in the title bar | Icon rail, 44 px |
| Row | Two lines, 56 px | One line, 32 px | Three lines + avatar, 76 px |
| Reader | Right pane, always shown | Replaces the list on Enter, Esc returns | Right pane, always shown, wide |
| j / k | Opens the thread | Moves the selection; opens only while reading | Opens the thread |
| Accent | Teal | Violet | Terracotta, serif display font |
| Extra | | Key-hint bar at the bottom | |

```rust
pub enum Preset { Tide, Command, Paper }          // settings.json "preset"
pub struct PresetSpec {
    pub nav: NavStyle,            // Sidebar | Tabs | Rail
    pub row: RowStyle,            // TwoLine | OneLine | ThreeLine
    pub row_height: f32,
    pub reader: ReaderPlacement,  // Split | Replace
    pub open_on_move: bool,
    pub hint_bar: bool,
    pub display_font: &'static str,
    pub dark: Palette,
    pub light: Palette,
}
```

- **Preset and theme mode are independent.** Every preset has a dark and a light palette; System / Light / Dark still picks between them.
- **Selection and reading are split.** `AppState::selected` is the current thread in every preset. New `AppState::reading` says the reader is on screen. `select()` moves the selection without marking read; `open()` selects, marks read and sets `reading`. With `open_on_move` the list calls `open()` on j/k (Tide, Paper). Without it, j/k call `select()` unless `reading` is already true (Command).
- **Navigation is one model, three renderers.** `nav::items()` builds the entries (unified mailboxes, accounts, user labels, unread counts); `sidebar`, `tabs` and `rail` draw them. Tabs and the rail cannot show every label, so both open a "Go to label" picker (`g l`, overlay `GoPicker`). Account dots in tabs and the rail filter the view to one account.
- **Mail body is always on a light "paper" surface**, in every preset and both modes. Inverting HTML mail colours breaks too many messages.
- **Toasts never sit over the webview.** Tide and Paper: bottom of the list pane. Command: in the hint bar.
- **Switching is live** (palette entry "Layout"), with no restart. `--preset <name>` overrides the setting for one run (bench, screenshots).

## Consequences

- Every speed budget applies to every preset. `bench-all` runs the app bench once per preset; the scroll step is 3 rows of the active preset. In Command the j/k bench presses Enter once first, then measures j/k while reading.
- `ROW_HEIGHT` becomes `PresetSpec::row_height`. `uniform_list` still gets one height per view.
- Additions to the frozen 1G-B contract (applied by the main thread before the agents start): `AppState::{reading, select, close_reader, set_preset}`, `Settings::preset`, `Overlay::GoPicker`, `StateEvent::ReadingChanged`, actions `GoLabel` and `CloseReader`.
- Cost: about 600–800 more lines of UI code than one layout, and every visual check runs 3 presets × 2 modes.
- `docs/SPEC.md` feature 2 ("Three panes") is now the Tide preset; Command and Paper depart from it on purpose.
- Rejected: a fourth "Console" direction (column table over a preview pane). Highest cost, and the preview pane is short for long mail.
