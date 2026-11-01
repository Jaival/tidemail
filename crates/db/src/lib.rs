//! Tidemail database: one SQLite file, schema migrations, list/read queries
//! and the shared row types that cross the IPC boundary.

pub mod read;
pub mod types;
pub mod write;

use std::path::Path;

pub use rusqlite;
pub use rusqlite::{Connection, Error, Result};

const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_init.sql")];

/// Schema version this build expects.
pub const SCHEMA_VERSION: usize = MIGRATIONS.len();

/// Open (or create) the database, set pragmas and run pending migrations.
pub fn open(path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&mut conn)?;
    Ok(conn)
}

/// In-memory database with the full schema (tests).
pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<()> {
    // journal_mode and mmap_size return a row (or none for :memory:);
    // execute_batch ignores result rows.
    conn.execute_batch(
        "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA mmap_size = 268435456; \
         PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;",
    )?;
    conn.set_prepared_statement_cache_capacity(128);
    Ok(())
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

/// Current time in ms since the Unix epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
