//! Search: FTS5 row upkeep and queries.
//!
//! `fts` keeps `messages_fts` in step. `search` parses the operator syntax
//! (see `query`) and returns threads, newest first, one hit per thread.

pub mod fts;
mod query;

use std::collections::{HashMap, HashSet};
use tidemail_db::read::thread_row;

use tidemail_db::rusqlite::types::Value;
use tidemail_db::rusqlite::{params_from_iter, Connection};
use tidemail_db::types::{SearchHit, ThreadRow};

use query::Parsed;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Db(#[from] tidemail_db::Error),
}

/// Marker chars `snippet()` puts around matches. They are replaced by
/// `<mark>` after the text is HTML-escaped.
const OPEN: char = '\u{1}';
const CLOSE: char = '\u{2}';

/// FTS matches above this count switch to the "dense" strategy.
const DENSE: usize = 2000;

const THREAD_COLS: &str = "id, account_id, last_date, subject, snippet, participants, \
     message_count, unread_count, has_attachment, is_starred";

/// Thread results for a query, newest first. Free text needs >= 2
/// characters; operator-only queries run at once. Each hit carries the
/// newest matching message of the thread and its snippet
/// (`SearchHit::snippet_html`: matches in `<mark>`, everything else
/// HTML-escaped).
///
/// Strategy (every probe is an index lookup; `messages` is never scanned):
/// - no FTS terms: walk threads newest first, keep those with a message that
///   passes the filters, stop at `limit`;
/// - FTS terms, up to `DENSE` matching messages: join the matches to their
///   threads and order by thread date;
/// - more matches: collect the matching ids, walk threads newest first and
///   stop at `limit`, so the cost is one scan of the id list.
pub fn search(conn: &Connection, query: &str, limit: u32) -> Result<Vec<SearchHit>, Error> {
    let limit = limit.min(tidemail_db::read::MAX_PAGE) as usize;
    let Some(mut parsed) = Parsed::parse(query) else {
        return Ok(Vec::new());
    };
    if !parsed.resolve_labels(conn)? {
        return Ok(Vec::new());
    }
    let fts_match = parsed.fts_match();
    let (thread_ids, matching) = match &fts_match {
        None => (walk_threads(conn, &parsed, limit)?, None),
        Some(m) => {
            let (ids, set) = match_threads(conn, &parsed, m, limit)?;
            (ids, Some(set))
        }
    };

    let mut thread_stmt =
        conn.prepare_cached(&format!("SELECT {THREAD_COLS} FROM threads WHERE id = ?1"))?;
    let (messages_sql, filter_binds) = thread_messages_sql(&parsed);
    let mut messages_stmt = conn.prepare_cached(&messages_sql)?;
    let mut picked: Vec<(ThreadRow, i64, String)> = Vec::with_capacity(thread_ids.len());
    for tid in thread_ids {
        let thread = thread_stmt.query_row([tid], thread_row)?;
        let mut binds = vec![Value::Integer(tid)];
        binds.extend(filter_binds.iter().cloned());
        let mut rows = messages_stmt.query(params_from_iter(binds))?;
        let (message_id, stored) = loop {
            let r = rows
                .next()?
                .ok_or(tidemail_db::Error::QueryReturnedNoRows)?;
            let id: i64 = r.get(0)?;
            if matching.as_ref().is_none_or(|s| s.contains(&id)) {
                break (id, r.get::<_, String>(1)?);
            }
        };
        picked.push((thread, message_id, stored));
    }

    // One pass over the match stream; `+rowid` keeps SQLite from probing
    // each id separately (a probe costs about as much as the whole pass for
    // long prefixes). `snippet()` is costly, so it runs for picked ids only.
    let mut marked: HashMap<i64, String> = HashMap::new();
    if let Some(m) = &fts_match {
        let ids = picked
            .iter()
            .map(|p| p.1.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT rowid, snippet(messages_fts, -1, char(1), char(2), '…', 14) FROM messages_fts              WHERE messages_fts MATCH ?1 AND +rowid IN ({ids})"
        ))?;
        let rows = stmt.query_map([m], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (id, s) = row?;
            if !s.is_empty() {
                marked.insert(id, s);
            }
        }
    }
    Ok(picked
        .into_iter()
        .map(|(thread, message_id, stored)| SearchHit {
            thread,
            message_id,
            snippet_html: marked
                .get(&message_id)
                .map_or_else(|| escape(&stored), |s| highlight(s)),
        })
        .collect())
}

fn walk_threads_sql(parsed: &Parsed) -> (String, Vec<Value>) {
    let mut binds = Vec::new();
    let filter = parsed.filter(&mut binds);
    (
        format!(
            "SELECT t.id FROM threads t WHERE EXISTS \
             (SELECT 1 FROM messages m WHERE m.thread_id = t.id{filter}) \
             ORDER BY t.last_date DESC, t.id DESC LIMIT ?{}",
            binds.len() + 1
        ),
        binds,
    )
}

fn walk_threads(conn: &Connection, parsed: &Parsed, limit: usize) -> Result<Vec<i64>, Error> {
    let (sql, mut binds) = walk_threads_sql(parsed);
    binds.push(Value::Integer(limit as i64));
    let ids = conn
        .prepare_cached(&sql)?
        .query_map(params_from_iter(binds), |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Filtered FTS matches: `(thread id, message id)`. Only run when the raw
/// match count is small.
fn small_sql(parsed: &Parsed) -> (String, Vec<Value>) {
    let mut binds = vec![Value::Integer(0)]; // ?1 = MATCH, bound by the caller
    let filter = parsed.filter(&mut binds);
    (
        format!(
            "SELECT m.thread_id, m.id FROM messages_fts f JOIN messages m ON m.id = f.rowid \
             WHERE messages_fts MATCH ?1{filter}"
        ),
        binds,
    )
}

fn dense_walk_sql(parsed: &Parsed) -> (String, Vec<Value>) {
    let mut binds = Vec::new();
    let filter = parsed.filter(&mut binds);
    (
        format!(
            "SELECT t.id, m.id FROM threads t CROSS JOIN messages m ON m.thread_id = t.id \
             WHERE 1 = 1{filter} ORDER BY t.last_date DESC, t.id DESC"
        ),
        binds,
    )
}

/// Messages of one thread that pass the filters, newest first.
/// Parameters: thread id, filter values.
fn thread_messages_sql(parsed: &Parsed) -> (String, Vec<Value>) {
    let mut binds = vec![Value::Null];
    let filter = parsed.filter(&mut binds);
    (
        format!(
            "SELECT m.id, m.snippet FROM messages m WHERE m.thread_id = ?1{filter} \
             ORDER BY m.internal_date DESC"
        ),
        binds.split_off(1),
    )
}

/// Ordered thread ids plus the set of matching message ids that can be
/// picked for each thread.
fn match_threads(
    conn: &Connection,
    parsed: &Parsed,
    fts_match: &str,
    limit: usize,
) -> Result<(Vec<i64>, HashSet<i64>), Error> {
    // Raw match count, capped: picks the strategy without joins or filters.
    let raw = conn
        .prepare_cached(&format!(
            "SELECT count(*) FROM (SELECT 1 FROM messages_fts WHERE messages_fts MATCH ?1 LIMIT {})",
            DENSE + 1
        ))?
        .query_row([fts_match], |r| r.get::<_, i64>(0))?;
    if raw <= DENSE as i64 {
        let (sql, mut binds) = small_sql(parsed);
        binds[0] = Value::Text(fts_match.to_owned());
        let found: Vec<(i64, i64)> = conn
            .prepare_cached(&sql)?
            .query_map(params_from_iter(binds), |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let mut threads: Vec<i64> = found.iter().map(|f| f.0).collect();
        threads.sort_unstable();
        threads.dedup();
        // Integer ids only, so inlining them is safe.
        let list = threads
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let ids = conn
            .prepare(&format!(
                "SELECT id FROM threads WHERE id IN ({list}) \
                 ORDER BY last_date DESC, id DESC LIMIT ?1"
            ))?
            .query_map([limit as i64], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        return Ok((ids, found.into_iter().map(|f| f.1).collect()));
    }

    let matching: HashSet<i64> = conn
        .prepare_cached("SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?1")?
        .query_map([fts_match], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let (sql, binds) = dense_walk_sql(parsed);
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(params_from_iter(binds))?;
    let mut ids: Vec<i64> = Vec::with_capacity(limit);
    while let Some(r) = rows.next()? {
        let (tid, mid): (i64, i64) = (r.get(0)?, r.get(1)?);
        if matching.contains(&mid) && ids.last() != Some(&tid) {
            ids.push(tid);
            if ids.len() == limit {
                break;
            }
        }
    }
    drop(rows);
    Ok((ids, matching))
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            OPEN | CLOSE => {}
            c => out.push(c),
        }
    }
    out
}

/// HTML-escape `snippet()` output and turn its marker chars into `<mark>`.
fn highlight(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    let mut plain = String::new();
    for c in s.chars() {
        match c {
            OPEN | CLOSE => {
                out.push_str(&escape(&plain));
                plain.clear();
                out.push_str(if c == OPEN { "<mark>" } else { "</mark>" });
            }
            c => plain.push(c),
        }
    }
    out.push_str(&escape(&plain));
    out
}

#[cfg(test)]
mod tests;
