-- Gmail's messagesTotal for the account; the status line shows first-sync progress.
ALTER TABLE sync_state ADD COLUMN messages_total INTEGER;
