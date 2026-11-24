-- Phase 3 send queue (ADR 0004). One row per message waiting for
-- `messages.send`. The local copy (`message_id`, gmail_id 'local-<id>') is in
-- `messages` with SENT until Gmail confirms; then Gmail's ids replace the
-- local ones and the row is deleted.
CREATE TABLE outbox (
  id              INTEGER PRIMARY KEY,
  account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  message_id      INTEGER UNIQUE REFERENCES messages(id) ON DELETE SET NULL,
  mail_json       TEXT NOT NULL,      -- OutgoingMail: compose fields, to reopen it
  raw             BLOB NOT NULL,      -- RFC 822 message as sent
  attempts        INTEGER NOT NULL DEFAULT 0,
  next_attempt_at INTEGER NOT NULL,
  last_error      TEXT,
  offline         INTEGER NOT NULL DEFAULT 0,
  failed          INTEGER NOT NULL DEFAULT 0,
  created_at      INTEGER NOT NULL
);
CREATE INDEX outbox_due ON outbox(failed, next_attempt_at);
