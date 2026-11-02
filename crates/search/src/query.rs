//! Query parser and SQL builder.
//!
//! Syntax (case-insensitive operator names, space = AND):
//! - `from:x`, `to:x`: FTS on the sender / recipient columns, prefix on the
//!   last token typed; `"quoted value"` allowed.
//! - `has:attachment`, `is:unread`
//! - `label:x`: label name (any case) or label key (`INBOX`); quote names
//!   with spaces.
//! - `before:D`, `after:D`: `D` is `YYYY-MM-DD` or `YYYY/MM/DD` (Gmail
//!   style), midnight UTC. `after:` is inclusive, `before:` exclusive.
//! - `"a phrase"` and free words. The last word gets prefix matching unless
//!   the query ends in a space; a last word shorter than 2 characters is
//!   ignored.
//! - Unusable operators (`from:` with no value, a half-typed date, unknown
//!   `is:` values) are ignored, so typing never errors.

use tidemail_db::rusqlite::types::Value;
use tidemail_db::rusqlite::{self, Connection};

#[derive(Default, Debug, PartialEq)]
pub struct Parsed {
    /// FTS5 expressions, ANDed.
    fts: Vec<String>,
    attachment: bool,
    unread: bool,
    labels: Vec<String>,
    /// Per `label:` operator, the ids of the labels it names.
    label_ids: Vec<Vec<i64>>,
    after: Option<i64>,
    before: Option<i64>,
}

/// One FTS5 string: `"..."` with inner quotes doubled, optional prefix star.
fn fts_str(s: &str, prefix: bool) -> String {
    format!(
        "\"{}\"{}",
        s.replace('"', "\"\""),
        if prefix { " *" } else { "" }
    )
}

struct Token {
    text: String,
    quoted: bool,
    /// Quote was never closed (still typing).
    open: bool,
}

fn tokenize(q: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut in_q = false;
    let mut has = false;
    for c in q.chars() {
        match c {
            '"' => {
                in_q = !in_q;
                quoted = true;
                has = true;
            }
            c if c.is_whitespace() && !in_q => {
                if has {
                    out.push(Token {
                        text: std::mem::take(&mut cur),
                        quoted,
                        open: false,
                    });
                }
                quoted = false;
                has = false;
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(Token {
            text: cur,
            quoted,
            open: in_q,
        });
    }
    out
}

/// Midnight UTC of `YYYY-MM-DD` / `YYYY/MM/DD` as ms since the epoch.
fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.split(['-', '/']);
    let (y, m, d): (i64, i64, i64) = (
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    );
    if it.next().is_some()
        || !(1..=12).contains(&m)
        || !(1..=31).contains(&d)
        || !(1970..=9999).contains(&y)
    {
        return None;
    }
    // Days from civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400_000)
}

impl Parsed {
    /// `None` when there is nothing to run (empty query, or free text under
    /// 2 characters with no operator).
    pub fn parse(q: &str) -> Option<Parsed> {
        let tokens = tokenize(q);
        let typing = !q.ends_with(char::is_whitespace);
        let n = tokens.len();
        let mut p = Parsed::default();
        for (i, t) in tokens.iter().enumerate() {
            // Prefix matching applies to the token being typed.
            let last = typing && i + 1 == n && (!t.quoted || t.open);
            let known = if t.quoted && !t.text.contains(':') {
                None
            } else {
                t.text.split_once(':').and_then(|(k, v)| {
                    let k = k.to_ascii_lowercase();
                    matches!(
                        k.as_str(),
                        "from" | "to" | "has" | "is" | "label" | "before" | "after"
                    )
                    .then_some((k, v))
                })
            };
            let Some((key, val)) = known else {
                if last && !t.quoted && t.text.chars().count() < 2 {
                    continue;
                }
                p.fts.push(fts_str(&t.text, last));
                continue;
            };
            let val = val.trim();
            if val.is_empty() {
                continue;
            }
            match key.as_str() {
                "from" | "to" => {
                    // Same 2-character floor as free text.
                    if last && !t.quoted && val.chars().count() < 2 {
                        continue;
                    }
                    let cols = if key == "from" {
                        "from_name from_email"
                    } else {
                        "to_text"
                    };
                    p.fts.push(format!("{{{cols}}} : {}", fts_str(val, last)));
                }
                "has" => p.attachment |= val.eq_ignore_ascii_case("attachment"),
                "is" => p.unread |= val.eq_ignore_ascii_case("unread"),
                "label" => p.labels.push(val.to_string()),
                "after" => p.after = parse_date(val).or(p.after),
                _ => p.before = parse_date(val).or(p.before),
            }
        }
        let runnable = !p.fts.is_empty()
            || p.attachment
            || p.unread
            || !p.labels.is_empty()
            || p.after.is_some()
            || p.before.is_some();
        runnable.then_some(p)
    }

    /// Look up the labels named by `label:` operators. `false` when one
    /// names no label, so nothing can match.
    pub fn resolve_labels(&mut self, conn: &Connection) -> Result<bool, rusqlite::Error> {
        let mut stmt = conn.prepare_cached(
            "SELECT id FROM labels WHERE label_key = ?1 OR name = ?2 COLLATE NOCASE",
        )?;
        self.label_ids.clear();
        for l in &self.labels {
            let ids: Vec<i64> = stmt
                .query_map([l.to_uppercase(), l.clone()], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            if ids.is_empty() {
                return Ok(false);
            }
            self.label_ids.push(ids);
        }
        Ok(true)
    }

    /// The FTS5 MATCH expression, if any.
    pub fn fts_match(&self) -> Option<String> {
        (!self.fts.is_empty()).then(|| self.fts.join(" "))
    }

    /// Row filter on message alias `m`; values are appended to `binds`
    /// (which may already hold leading parameters).
    pub fn filter(&self, binds: &mut Vec<Value>) -> String {
        let mut f = String::new();
        if self.attachment {
            f.push_str(" AND m.has_attachment = 1");
        }
        if self.unread {
            f.push_str(" AND m.is_unread = 1");
        }
        if let Some(a) = self.after {
            binds.push(Value::Integer(a));
            f.push_str(&format!(" AND m.internal_date >= ?{}", binds.len()));
        }
        if let Some(b) = self.before {
            binds.push(Value::Integer(b));
            f.push_str(&format!(" AND m.internal_date < ?{}", binds.len()));
        }
        // Label ids are integers from `resolve_labels`, so inlining is safe.
        for ids in &self.label_ids {
            let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            f.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM message_labels ml                  WHERE ml.message_id = m.id AND ml.label_id IN ({list}))"
            ));
        }
        f
    }
}
