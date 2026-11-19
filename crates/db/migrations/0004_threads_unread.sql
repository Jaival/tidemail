-- Unread counts per label read only unread threads.
CREATE INDEX threads_unread ON threads(id) WHERE unread_count > 0;
