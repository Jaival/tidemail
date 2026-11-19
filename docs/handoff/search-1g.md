# Handoff: search (Phase 1G-B)

Branch `phase1g-b-search`, from `phase1g-b` (f90fa48).

## Done

- `features/search/mod.rs`: `Search` entity (input in the slot, owned by the shell) and `Results` entity (list that replaces the thread list). `snippet.rs`: `<mark>` parser (`parse(html) -> Snippet { text, marks }`) with 4 unit tests.
- Input: `gpui-component` `Input` / `InputState`. `/` (`FocusSearch`) focuses it. Escape (the input's `Escape` action propagates) clears the query and emits `SearchEvent::Leave`; the shell then focuses its root. Enter opens the current (or first) result and leaves the input.
- Single-key bindings: while the input has focus the shell root's key context is `TidemailSearch` instead of `Tidemail`, so j/k/e/#/`/` etc. do not match and reach the input as text.
- Query: each `InputEvent::Change` runs `tidemail_search::search(conn, q, 100)` through `Db::read`. A new query replaces (drops) the previous task, so only the newest result is applied. The 2-character rule stays in `tidemail_search` (the shell does not repeat it). `DataChanged` re-runs the active query (no scroll reset).
- Results: one row per thread, rendered by `list/row.rs` with the preset's style and `spec.row_height`. Header shows "N results"; empty state "No results"; error shows "Search failed: ...".
- j/k/Enter/click in results: same rules as the list (`open_on_move` or `reading` opens, else `select`; prefetch on open). `e # s u` act on the current result (`Shell::act`).
- Placement (shell): Tide = title bar; Command = title bar, tabs hidden while the input has focus or a query is active, results replace the list pane (the reader still replaces it while reading); Paper = top of the list pane, list header replaced by the result header.
- Esc with focus on the shell and a query active (`Dismiss`) clears the query; with the reader open, `CloseReader` (later binding) wins as before.
- `row.rs`: new optional `Row::snippet: Option<&Snippet>`; marked runs drawn with `StyledText::with_highlights` (accent colour, semibold). `ThreadList` passes `None`. `list::row` is now `pub(crate)`.

## Known gaps

- Not driven by real key presses (no automation here). Checked: build, clippy `-D warnings`, tests, bench smoke (completes). To verify by hand: `/` leaves no "/" in the input (relies on the action consuming the key), Esc/Enter focus return, highlight colours in all presets.
- Ctrl+K and Ctrl+Z are not bound while the input has focus (context swap). Add a binding in context `TidemailSearch` when the palette lands, if wanted.
- Up/Down inside the input are the input's own; they do not move the result selection. Use Enter, then j/k.
- One character shows "No results" (search returns `[]` under 2 chars).
- `Shell` has an `on_action` for `Dismiss`; if the commands agent adds another on the shell root, merge them (the first handler that does not call `cx.propagate()` stops the chain).
- Hits show the matched message's snippet; the search crate picks the newest matching message (see `docs/handoff/search.md`).

## Requests

None for the frozen contract. Additions: `Row::snippet` field (`row.rs`), `Shell::search`, `search::{Search, SearchEvent, Results, Snippet}`.
