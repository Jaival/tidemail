# Phase 1G-B: commands (palette, pickers, toasts, status)

Branch `phase1g-b-commands`. Files: `features/palette/{mod,filter,toast}.rs`, `features/status/mod.rs`, overlay layer and handlers in `features/shell/mod.rs`, `main.rs`, `state.rs` (two narrow `allow(dead_code)`).

## Done

- **Picker widget** (`palette::Picker`): filter input (`gpui-component` `Input`), up/down/Enter through the existing `SelectNext` / `SelectPrev` / `OpenThread` actions, mouse click, click on the backdrop closes. Esc is `Dismiss` (or `CloseReader`, which closes the overlay first). Entries are ranked by `palette::filter::rank` (all words must match; prefix > word start > substring > scattered letters; tests).
- **Palette** (`Ctrl+K`, toggles): every `SHORTCUTS` entry with its keys (runs the action on Enter via `window.dispatch_action` after the overlay closed), Theme System / Light / Dark, Prefetch 5 / 10 / 20 / 25 / 50, Layout Tide / Command / Paper. Current value has a check mark. A test checks every `SHORTCUTS` row maps to an action.
- **Label picker** (`l`): user labels of the thread's account. The thread's current labels load async (one query) and are checked; picking adds or removes the label on the whole thread (`AddLabel` / `RemoveLabel`) with toast "Added label X" / "Removed label X". Closes after a pick.
- **Move picker** (`v`): user labels of the account, `ActionKind::Move` (add label, remove INBOX), toast "Moved to X".
- **Go picker** (`g l`, `GoLabel`): unified mailboxes (`nav::items`), Trash and Spam when synced, each account's inbox, user labels (account email as detail). Current view is checked. Uses `AppState::navigate`. `shell::nav` is now `pub mod`.
- **Toasts**: shell keeps one `Toast` (text + undo ids, 5 s). From `ActionDone` (with "Undo  Ctrl+Z") and `ActionFailed` (red, no undo). `Ctrl+Z` (`Undo`) undoes the current toast's action ids. **Decision: `Overlay::Toast` is not pushed.** Tide and Paper: card at the bottom of the list pane. Command: replaces the hints in the hint bar. Never over the webview, so the reader needs no extra hiding for toasts.
- **Status** (`status::line`, same signature): "Synced 12 s ago" / "Synced just now" / "Offline" / "Sign-in needed" / "Syncing…" / "N actions pending" / "N actions failed" with Retry and Dismiss (engine `retry_failed` / `dismiss_failed` for every failed action, on the background executor). The shell repaints every 5 s (`status::TICK`) so the age updates. `status::text` is pure (tests).
- **Keys wired in the shell**: `PickLabel`, `PickMove`, `CommandPalette`, `GoLabel`, `Undo`, `Dismiss`. `FocusSearch` is left to the search agent. While a picker is open its input has focus; printable keys go to the input (GPUI prefers text input), up/down/Enter go to the picker. On close, focus returns to the shell.
- Dead-code allows removed from `mod actions/format/settings/state`. Two narrow allows remain in `state.rs`: `Overlay::Toast` (never pushed, by design) and `AppState::overlay_open` (the reader agent uses it).

## Checks (2026-11-19)

`cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p tidemail`, `cargo test --workspace` pass. `--bench` on a copy of `bench.db` (debug build, Tide) completes.

## Known gaps

- Not checked by a person or screenshot: overlay look in the three presets, focus return after close, Retry / Dismiss against a real failed action. Phase 1G-U2 should cover palette, pickers and toasts.
- The reader agent must hide the webview while `overlay_open()`. The overlay is a GPUI layer: it would sit under the webview otherwise.
- Palette "Search" (`/`) does nothing until the search agent handles `FocusSearch`.
- Label picker closes after one pick (no multi-toggle). Go picker has no Starred / Sent / Drafts per account (the sidebar reaches them).
- The 5 s repaint timer re-renders the whole shell; cheap, but it runs while the window is hidden.
- `Esc` in the palette while `reading` goes through `CloseReader`, which closes the overlay first (relies on binding order in `actions.rs`).

## Requests

None.
