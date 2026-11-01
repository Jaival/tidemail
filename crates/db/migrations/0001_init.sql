-- Tidemail schema v1. Frozen for Phase 1; changes go through docs/HANDOFF.md.

CREATE TABLE accounts (
  id           INTEGER PRIMARY KEY,
  email        TEXT NOT NULL UNIQUE,
  display_name TEXT,
  color        TEXT NOT NULL,          -- '#rrggbb', unified-inbox dot
  sort_order   INTEGER NOT NULL
);

CREATE TABLE labels (
  id         INTEGER PRIMARY KEY,
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  gmail_id   TEXT NOT NULL,            -- 'INBOX', 'Label_12'
  label_key  TEXT NOT NULL,            -- system: 'INBOX'; user: 'u:<account_id>:<gmail_id>'
  name       TEXT NOT NULL,
  is_system  INTEGER NOT NULL,
  color      TEXT,
  UNIQUE (account_id, gmail_id)
);
CREATE INDEX labels_key ON labels(label_key);

CREATE TABLE threads (
  id             INTEGER PRIMARY KEY,
  account_id     INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  gmail_id       TEXT NOT NULL,
  last_date      INTEGER NOT NULL,     -- ms epoch of newest message
  subject        TEXT NOT NULL,
  snippet        TEXT NOT NULL,
  participants   TEXT NOT NULL,
  message_count  INTEGER NOT NULL,
  unread_count   INTEGER NOT NULL,
  has_attachment INTEGER NOT NULL,
  is_starred     INTEGER NOT NULL,
  UNIQUE (account_id, gmail_id)
);

CREATE TABLE messages (
  id             INTEGER PRIMARY KEY,  -- also the FTS rowid
  account_id     INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  gmail_id       TEXT NOT NULL,
  thread_id      INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
  from_name      TEXT NOT NULL,
  from_email     TEXT NOT NULL,
  to_text        TEXT NOT NULL,        -- to + cc names and emails
  subject        TEXT NOT NULL,
  snippet        TEXT NOT NULL,
  internal_date  INTEGER NOT NULL,     -- ms epoch
  is_unread      INTEGER NOT NULL,
  is_starred     INTEGER NOT NULL,
  has_attachment INTEGER NOT NULL,
  body_state     INTEGER NOT NULL,     -- 0 missing, 1 stored, 2 deferred (older than body policy)
  UNIQUE (account_id, gmail_id)
);
CREATE INDEX messages_thread ON messages(thread_id, internal_date);
CREATE INDEX messages_body_todo ON messages(internal_date DESC) WHERE body_state = 0;

CREATE TABLE message_labels (
  message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  label_id   INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  PRIMARY KEY (message_id, label_id)
) WITHOUT ROWID;
CREATE INDEX message_labels_label ON message_labels(label_id, message_id);

-- Denormalized: a thread has a label if any of its messages has it.
CREATE TABLE thread_labels (
  label_key  TEXT NOT NULL,
  last_date  INTEGER NOT NULL,
  thread_id  INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
  account_id INTEGER NOT NULL,
  PRIMARY KEY (label_key, last_date DESC, thread_id DESC)
) WITHOUT ROWID;
CREATE INDEX thread_labels_acct ON thread_labels(account_id, label_key, last_date DESC, thread_id DESC);
CREATE INDEX thread_labels_thread ON thread_labels(thread_id);

CREATE TABLE bodies (
  message_id        INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
  html_sanitized    TEXT,
  text              TEXT NOT NULL,     -- text/plain part, or text derived from HTML
  sanitizer_version INTEGER NOT NULL
);

CREATE TABLE attachments (
  id                  INTEGER PRIMARY KEY,
  message_id          INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  part_id             TEXT NOT NULL,
  gmail_attachment_id TEXT,
  filename            TEXT NOT NULL,
  mime_type           TEXT NOT NULL,
  size                INTEGER NOT NULL,
  content_id          TEXT,            -- without angle brackets
  is_inline           INTEGER NOT NULL,
  local_path          TEXT
);
CREATE INDEX attachments_msg ON attachments(message_id);
CREATE INDEX attachments_cid ON attachments(message_id, content_id) WHERE content_id IS NOT NULL;

-- Inline cid: parts under 200 KB, served by tidemail-cid://
CREATE TABLE inline_blobs (
  attachment_id INTEGER PRIMARY KEY REFERENCES attachments(id) ON DELETE CASCADE,
  data          BLOB NOT NULL
);

CREATE TABLE action_queue (
  id              INTEGER PRIMARY KEY,
  account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  thread_id       INTEGER NOT NULL,
  kind            TEXT NOT NULL,
  payload_json    TEXT NOT NULL,
  attempts        INTEGER NOT NULL DEFAULT 0,
  next_attempt_at INTEGER NOT NULL,
  last_error      TEXT,
  failed          INTEGER NOT NULL DEFAULT 0,
  created_at      INTEGER NOT NULL
);
CREATE INDEX action_queue_due ON action_queue(failed, next_attempt_at);
CREATE INDEX action_queue_thread ON action_queue(thread_id);

CREATE TABLE sync_state (
  account_id      INTEGER PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  history_id      TEXT,
  backfill_cursor TEXT,
  last_sync_at    INTEGER
);

CREATE TABLE sender_prefs (
  email       TEXT PRIMARY KEY,
  load_images INTEGER NOT NULL
) WITHOUT ROWID;

-- FTS5 external content over a view. Views have no triggers:
-- Rust (crates/search) keeps messages_fts in step explicitly.
CREATE VIEW fts_src AS
  SELECT m.id, m.from_name, m.from_email, m.to_text, m.subject,
         COALESCE(b.text, '') AS body_text
  FROM messages m LEFT JOIN bodies b ON b.message_id = m.id;

CREATE VIRTUAL TABLE messages_fts USING fts5(
  from_name, from_email, to_text, subject, body_text,
  content = 'fts_src', content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2',
  prefix = '2 3'
);
