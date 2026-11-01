//! Search: FTS5 row upkeep and queries.
//!
//! Phase 0 state: `fts` upkeep is real (the fake mailbox generator uses it).
//! `search` is a stub. Phase 1 (search agent) owns this crate: operator
//! parser, per-keystroke queries, highlights. Keep the public signatures.

pub mod fts;

use tidemail_db::rusqlite::Connection;
use tidemail_db::types::SearchHit;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("search is not implemented yet")]
    NotImplemented,
    #[error(transparent)]
    Db(#[from] tidemail_db::Error),
}

/// Thread results for a query, newest first. Free text needs >= 2
/// characters; operator-only queries run at once.
pub fn search(conn: &Connection, query: &str, limit: u32) -> Result<Vec<SearchHit>, Error> {
    let _ = (conn, query, limit);
    Err(Error::NotImplemented)
}
