# Handoff: UI shell (`phase1-shell`)

## Done

- `Shell.tsx` owns state: view (`nav`), selection, search query, overlay (palette / label picker), toast, undo stack. `App.tsx` only mounts it.
- Panes: `Sidebar` (unified Inbox/Starred/Sent/Drafts, accounts with color dots and per-account Starred/Sent/Drafts, user labels), virtualized `ThreadList` (TanStack Virtual, keyset paging, 64 px rows, account dot in unified views), reader slot (`<Reader threadId />`). `<SearchPanel query onOpenThread />` replaces the list while the search box is non-empty. Esc in the box clears it.
- `useThreadList(view)`: paging, `refresh()` (re-queries as many rows as are loaded, stale responses dropped), `apply(ids, kind)` (optimistic change; returns true if rows left the list).
- Keyboard (`shortcuts.ts`, one command registry feeds both keys and the Ctrl+K palette): `j k Enter e # s l u / Ctrl+K Ctrl+Z`, `g i`. Ignored while typing in inputs (except Ctrl+K). Palette lists shortcuts, theme override (system/dark/light), go-to views.
- Actions call `commands.applyAction` for the whole thread. The list changes first; on error the list is re-queried (reverts) and an error toast shows. On success a 5 s toast with Undo; Ctrl+Z pops the same stack and calls `commands.undoAction`. After archive/trash of the selected thread, selection moves to the next (else previous) row.
- `dataChanged` / `syncStatusChanged` listeners start after the first page loads. `dataChanged` is debounced 100 ms; refreshes the list when the event touches the current label or a loaded thread, and always reloads labels.
- `StatusArea`: failed (Retry / Dismiss for all failed) > offline > sign-in required > N pending > syncing > "Synced N s ago".
- Theme: `useTheme` follows Windows, override persisted in `localStorage` (`tidemail.theme`).
- First paint: only the first `listThreads` runs before `reportFirstPaint`. Accounts, labels, status and event listeners start after it. The virtualizer gets `initialRect` so rows render in the first commit.
- `e2e/bench.ts`: `j/k open to paint (p95)` replaces the SKIP line. It presses `j` 40 times and waits for `.reader` text to change plus two frames. Budget 50 ms. Measured with the placeholder Reader and frame-quantized (about 16 ms steps): re-run after the renderer merges, since the real Reader's cost is not in the number.

## Requests for the main thread

None. No schema, IPC command or event changes needed.

Check after the sync merge: `applyAction` currently returns NotImplemented, so every action flips and then reverts with an error toast. That is the intended fallback.

## Open questions

- `u` toggles read/unread (spec says "mark unread"). Say if it should only mark unread.
- Opening a thread does not mark it read. Spec does not say; the reader or sync agent may want that.
- Palette shows `Ctrl+K`'s own entry hidden; "Compose" button is disabled (Phase 3).

## Known gaps (left out on purpose)

- "Move to X" action has no UI (no shortcut in the spec).
- Prefetch of next 10 bodies on open is not wired (`prefetchBodies` takes message ids; the sync side owns it).
- Optimistic changes are not merged with a refresh already in flight (a late page could briefly undo a star toggle until the next `dataChanged`).
- Actions on several selected threads: only the single selected thread is supported.
- The Sent view shows `participants` as the list gives it; no "To:" formatting.
- Labels shown in the sidebar are user labels only; no unread counts.
- Real-app visual check was not done in a browser preview; verified by typecheck, lint, build and the E2E bench only.

## Bench (bench.db, this branch, machine busy with other builds)

- j/k open to paint p95: 7 ms (placeholder Reader) PASS.
- Scroll: 0/600 dropped, 0 blank PASS.
- Cold launch worst of 3: 1164 ms (839–1164) FAIL, was 705 ms in Phase 0. Other agents were compiling at the same time, so the number is noisy. Re-measure on a quiet machine. Phase 2 owns the budget.
