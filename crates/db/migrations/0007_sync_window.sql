-- Mail older than sync_from (epoch ms) is not listed by the backfill; NULL = all mail.
ALTER TABLE sync_state ADD COLUMN sync_from INTEGER;

-- An account still in its first backfill switches to the default window of
-- 183 days (sync::Config::sync_window). Its cursor belongs to an unfiltered
-- list, so the backfill starts again; ids already stored are skipped.
UPDATE sync_state
SET sync_from = CAST(strftime('%s', 'now') AS INTEGER) * 1000 - 183 * 86400000,
    backfill_cursor = NULL
WHERE backfill_cursor IS NOT 'done';

-- Backfill progress: the oldest stored mail inside the window, per account.
CREATE INDEX messages_account_date ON messages(account_id, internal_date);
