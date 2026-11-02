# Handoff: search (Phase 1, agent 5)

Branch `phase1-search`.

## Done

- `crates/search`: `search(conn, query, limit) -> Vec<SearchHit>` (same signature; `Error::NotImplemented` removed, bench was the only user).
  Files: `lib.rs` (strategy, snippets), `query.rs` (parser, filters), `tests.rs` (14 tests incl. `EXPLAIN QUERY PLAN` check and dense/small agreement).
- Operators (names case-insensitive, space = AND): `from:`, `to:`, `has:attachment`, `is:unread`, `label:`, `before:`, `after:`, `"quoted phrase"`, free words.
  - `from:`/`to:` match FTS columns (`from_name from_email` / `to_text`), also with quoted values.
  - `label:` matches label name (any case) or label key (`INBOX`). Quote names with spaces. Unknown label = no results.
  - Dates: `YYYY-MM-DD` or `YYYY/MM/DD`, midnight **UTC**. `after:` inclusive, `before:` exclusive.
  - Prefix match on the last token typed (free text, `from:`, `to:`), not when the query ends in a space. A last token under 2 chars is ignored (so `is:unread p` runs as `is:unread`). Free text alone under 2 chars returns `[]`.
  - Unusable operators (`from:`, half-typed date, `is:foo`) are ignored. User input is quoted for FTS5 (`"` doubled), so no syntax errors (`"`, `*`, `-`, `:`, `AND`, `NEAR(` tested).
- Results: threads, newest first (`threads.last_date DESC, id DESC`), one hit per thread. The hit message is the **newest matching message** of the thread (not bm25-best; bm25 per-thread ranking cost too much, see Strategy).
- Snippet: `snippet(messages_fts, -1, char(1), char(2), '…', 14)`; markers are replaced after HTML escape, so `snippet_html` has `<mark>` and everything else escaped. No type change. Operator-only hits use the stored `messages.snippet` (escaped, no marks).
- Strategy (no scan of `messages`; checked with `EXPLAIN QUERY PLAN` in tests and on `bench.db`):
  - no FTS terms: walk `threads` newest first with `EXISTS` on `messages` (`messages_thread` index), stop at `limit`.
  - FTS terms, raw matches <= 2000 (capped count probe): join matches to threads, order by date.
  - raw matches > 2000 ("dense", e.g. `th`): collect matching rowids into a set, walk threads newest first through `messages_thread`, stop at `limit`.
  - Snippets: one pass over the match stream with `+rowid IN (picked ids)`. Per-id `rowid = ?` probes were tested and rejected: for prefixes of 4+ chars one probe costs 3 to 9 ms (as much as the whole stream).
  - `label:` is resolved to label ids once per query, then inlined (ints only).
- `crates/bench`: "search per keystroke" now types 10 queries char by char (3 rounds, ~700 samples, incl. operators, phrase, dense prefixes). `--verbose` prints keystrokes over 20 ms. The `NotImplemented` SKIP path is gone.
- `src/features/search/SearchPanel.tsx` (+ `snippet.ts`): same props. One `commands.search(query, 50)` per change, stale responses dropped (sequence counter), TanStack Virtual list, highlights rendered as React text nodes (`parseSnippet` splits on `<mark>` and decodes the 5 entities; no `dangerouslySetInnerHTML`), Up/Down/Enter via a window listener (input keeps focus), theme via the existing CSS variables.

## Bench (bench.db, release, 50,000 messages)

| Setup | p50 | p95 | max | Budget |
|---|---|---|---|---|
| with request 1 (index) | 1.8 ms | 11.6 to 12.9 ms | 20 to 23 ms | p95 < 30 ms |
| without index (current schema) | 3.5 ms | 23 to 25 ms | 42 to 50 ms | p95 < 30 ms |

Both pass. Without the index it is close; the index is cheap and moves the walk from a sort of all threads to an index scan. Noise on this machine is high (first runs were 2x slower while the machine was busy).

The fake corpus has a tiny vocabulary: words like `budget`, `after`, `the` match 75 to 95 % of messages, which makes every 2+ char prefix "dense". Real mail will be sparser, so this is a worst case.

## Requests for the main thread

1. **Index (recommended)**: add to a migration:
   ```sql
   CREATE INDEX threads_last_date ON threads(last_date DESC, id DESC);
   ```
   Measured above. Code works without it. (My local `bench.db` has it; `/tmp` copy without.)
2. No IPC, event, type or dependency changes needed. `SearchHit` kept as is. `commands.search` already exists; `limit` is capped at 100 inside `search`.
3. Writers must keep following `fts.rs` rules (insert on metadata, `replace_body` when body arrives, `delete` before deleting a message). Search assumes `messages_fts` rows exist for every message; messages with an empty body still match on sender, recipients and subject.

## Open questions

- Dates are UTC midnight. Local-time days need `chrono` (or `windows` APIs); say if you want it.
- "Best-matching message": newest matching message was chosen for speed (see above). bm25 choice would add one `ORDER BY rank` probe per thread (3 to 9 ms each for long prefixes).

## Known gaps (not built, edge cases)

- No negation (`-word`), `OR`, parentheses, or Gmail's other operators (`cc:`, `subject:`, `in:`, `older_than:` ...). `foo:bar` with unknown `foo` is searched as the phrase `foo:bar`.
- Dense term plus a very selective filter (e.g. `th label:rare`) walks all threads once: estimated 30+ ms. Not seen in bench.
- Trash/Spam are not excluded from results (Gmail excludes them by default). Not in the spec; add `in:` / label rules later if wanted.
- Snippet quality: `snippet(-1)` picks the best column; for `from:`-only queries it shows the sender field, not body text.
- Diacritics and case follow the FTS tokenizer (`unicode61 remove_diacritics 2`); `label:` name matching is case-insensitive ASCII only.
- No TS unit test for `parseSnippet`: the repo has no JS test runner.
- `SearchPanel` was not run in the app (shell agent owns rendering it); checked with lint, typecheck, build only.
