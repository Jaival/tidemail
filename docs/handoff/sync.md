# Handoff: sync engine (`crates/sync`, branch `phase1-sync`)

## Done

- `Engine` keeps its public signatures. `Engine::start(db_path, notifier)` returns at once, owns a small tokio runtime, and runs **no network tasks** (no provider). Actions still apply locally and queue.
- New: `Engine::start_with(db_path, notifier, Option<Arc<dyn GmailProvider>>, Config)` (own runtime) and `Engine::start_on(handle, ...)` (given runtime, used by tests).
- Gmail seam: `crates/sync/src/api.rs`. Traits `GmailApi` (per account) and `GmailProvider` (sign-in, `api(email)`, sign-out). `add_account` goes through `provider.sign_in()`.
- Action queue, backfill (meta pages, then bodies, resumable via `sync_state.backfill_cursor`), history polling, full id diff, reconciliation, failure/restore, prefetch, attachment download, status. Details below.
- Bench metric "action write (local)" now runs (`crates/bench/src/bin/bench.rs`, removed the SKIP arm only).
- 16 tests in `crates/sync/src/tests.rs`, tokio time paused, fake Gmail in `fake.rs`.

### Bench (bench.db, release, 2026-11-02)

| Metric | p50 | p95 | max | Budget |
|---|---|---|---|---|
| action write (local) | 0.71 ms | 1.30 ms | 43.6 ms (first cold write) | 16 ms |

## Merge work for the main thread

### 1. Adapter `tidemail_gmail` -> `tidemail_sync::api`

Implement `GmailApi` over the gmail client and `GmailProvider` over sign-in/keyring. Contract (also in the `api.rs` header):

- `get_messages` takes <= 50 ids (one `/batch/gmail/v1`). Ids that are 404 are omitted; any other per-id error fails the call.
- Retry 429 / 403 `rateLimitExceeded` inside the adapter; return `ApiError::RateLimited` only when giving up. Transport errors -> `Offline`; 401/revoked -> `Auth`; 404 on `history.list` -> `HistoryExpired`.
- `RemoteMessage.from/to/cc/subject` are raw header values. `label_ids` includes `UNREAD` and `STARRED`. `internal_date_ms` is ms.
- `Metadata` format: `body = None`. `Full`: fill `body.html`, `body.text`, and `attachments` (inline `data` if Gmail sent it, else `attachment_id`). `has_attachment` is best effort in metadata; the engine corrects it from `Full`.
- `list_message_ids(label, token)`: `label` is a label id filter (`labelIds=`). Used by the full id diff.
- `list_history`: merge `messagesAdded` -> `added`, `messagesDeleted` -> `deleted`, `labelsAdded/Removed` -> `label_changed`; set `history_id` from the response.
- Wire it: `Engine::start_with(path, notifier, Some(Arc::new(adapter)), Config::default())` in `src-tauri/src/lib.rs`.

### 2. Renderer: `save_attachment`

`lib.rs::save_attachment` is a local stand-in (create dirs + write file, **no Mark-of-the-Web**). On merge, replace its body with `tidemail_render::save_attachment(path, data)`. Needed from render: `attachment_path` sanitizes the filename for Windows; `save_attachment` creates parent dirs, writes the file and the `Zone.Identifier` stream.

### 3. Requests

- **Schema:** none. Everything uses the frozen schema (`action_queue.failed` etc.).
- **IPC:** none changed.
- **New `tidemail_db` functions (optional, "move to tidemail_db on merge"):** the SQL in `crates/sync/src/store.rs` (`ingest_meta`, `ingest_full`, `delete_message`, `apply_op`, `snapshot`, `restore_snapshot`) and `actions.rs` (`reapply_pending`) is sync-only today. Move only if another crate needs it.
- **Root dependencies:** none. `crates/sync/Cargo.toml` adds `tokio`, `serde`, `serde_json` (workspace) and `async-trait = "0.1"` (already in `Cargo.lock`); dev-dep `tokio` with `test-util`. `Cargo.lock` changed for this.
- **Events:** none new. Uses `data_changed(thread_ids, label_keys)` and `status_changed`.

## Behavior notes

- **Action row:** one `action_queue` row per thread. `payload_json` = `{kind, thread_gmail_id, op:{add,remove,remote}, before:[{id,labels}]}`. `kind` column = serde tag (`archive`, `move`, ...). `before` is the label snapshot for exact undo inside the window.
- **Local apply:** label rows + `is_unread` / `is_starred` flags are derived from `UNREAD` / `STARRED` labels; `recompute_thread` runs; missing system labels are created.
- **Undo:** row exists, `attempts = 0` and not yet due -> delete row + restore snapshot. Otherwise enqueue the inverse op (due at once) and apply it locally. The id -> payload map for undo after the row is gone is in memory (last 500 actions); after an app restart, undo of a sent action returns `NotFound`.
- **Sender:** one task. Earlier unfinished rows of a thread block later ones. Backoff `5 s * 2^(n-1)`, cap 300 s, max 8 attempts, then `failed = 1` and a restore is queued. `Offline` / `Auth` errors do not use an attempt (retry after 10 s / 60 s).
- **Failure restore:** a failed or dismissed action queues a re-fetch of that thread's local messages; pending (non-failed) actions are re-applied on top. `retry_failed` re-applies the op locally and resets attempts.
- **History:** poll every 7 s per account (`Config.poll_interval`). Changed ids are re-fetched as metadata (labels), ids Gmail no longer has are deleted. 404 -> full id diff: list all ids, fetch new, delete removed, and re-fetch messages whose membership differs for `INBOX, UNREAD, STARRED, TRASH, SPAM`.
- **Backfill:** `backfill_cursor` = `NULL | meta:<page token> | done`. Body pass runs whenever `body_state = 0` rows exist (also for new live messages). Over 300k messages: bodies older than 2 years -> `body_state = 2` (deferred); `prefetch_bodies` fetches them on open.
- **Quota:** engine pauses `Config.batch_gap` (1 s) after each 50-message batch. 429 backoff is the adapter's job.
- **Status:** `AccountSyncState` per account; `last_sync_at` persisted in `sync_state`. The app crate's `get_sync_status` and `status_changed` event work as before.
- **Remove account:** deletes FTS rows (per message), then the account row (cascades); calls `provider.sign_out`. Attachment cache files stay on disk.

## Open questions

- Poll interval is fixed at 7 s, visible or hidden. Fine for the spec's 5-10 s.
- Should the engine expose a `sync_now` / wake for a manual refresh? Not in the spec; not added.

## Known gaps (unlikely cases, not coded)

- Label renames and new labels are fetched at account start and when a message carries an unknown label id; a rename alone is not seen until restart.
- The full id diff compares only `INBOX, UNREAD, STARRED, TRASH, SPAM`; changes to other user labels are missed during a history gap.
- A poll that races with a just-sent action can briefly show the pre-action state until the next poll (Gmail's own history record for the modify corrects it).
- Undo of an older action while a newer action on the same thread is still queued restores the older snapshot for the whole thread (UI undo is LIFO, so unlikely).
- Undo/snapshot restore of a message deleted remotely in between fails the undo with a DB error (FK).
- `get_messages` returning fewer ids than asked (404) is treated as "deleted remotely" for body fetches too.
- Rate-limited and other API errors share one loop backoff (5 s doubling, cap 5 min); no per-endpoint tuning.
- Sanitizer-version re-sanitize job is not part of this crate (spec lists it under rendering).
- Tests use a temp WAL file, not `:memory:` (several connections need to share one DB).
