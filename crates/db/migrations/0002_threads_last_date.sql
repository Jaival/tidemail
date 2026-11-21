-- Search walks threads newest first (see tidemail_search::search).
CREATE INDEX threads_last_date ON threads(last_date DESC, id DESC);
