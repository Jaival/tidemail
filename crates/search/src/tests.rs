use super::*;
use crate::fts::{self, FtsRow};
use query::Parsed;
use tidemail_db::rusqlite::params_from_iter;
use tidemail_db::rusqlite::types::Value;
use tidemail_db::types::BodyState;
use tidemail_db::write::*;

const DAY: i64 = 86_400_000;
/// 2026-03-01 00:00 UTC.
const MARCH: i64 = 1_772_323_200_000;

struct Msg<'a> {
    thread: &'a str,
    from_name: &'a str,
    from_email: &'a str,
    to: &'a str,
    subject: &'a str,
    body: &'a str,
    date: i64,
    unread: bool,
    attach: bool,
    labels: &'a [&'a str],
}

fn msg<'a>(thread: &'a str, subject: &'a str, body: &'a str, date: i64) -> Msg<'a> {
    Msg {
        thread,
        from_name: "Alice Example",
        from_email: "alice@example.com",
        to: "Bob <bob@corp.test>",
        subject,
        body,
        date,
        unread: false,
        attach: false,
        labels: &["INBOX"],
    }
}

fn seed(msgs: &[Msg]) -> Connection {
    let conn = tidemail_db::open_in_memory().unwrap();
    let a = insert_account(
        &conn,
        &NewAccount {
            email: "me@x",
            display_name: None,
            color: "#000",
            sort_order: 0,
        },
    )
    .unwrap();
    let inbox = upsert_label(&conn, a, "INBOX", "Inbox", true, None).unwrap();
    let work = upsert_label(&conn, a, "Label_1", "Work Stuff", false, None).unwrap();
    for (i, m) in msgs.iter().enumerate() {
        let t = ensure_thread(&conn, a, m.thread).unwrap();
        let id = insert_message(
            &conn,
            &NewMessage {
                account_id: a,
                gmail_id: &format!("m{i}"),
                thread_id: t,
                from_name: m.from_name,
                from_email: m.from_email,
                to_text: m.to,
                subject: m.subject,
                snippet: "plain <b>snippet</b>",
                internal_date: m.date,
                is_unread: m.unread,
                is_starred: false,
                has_attachment: m.attach,
                body_state: BodyState::Missing,
            },
        )
        .unwrap();
        let labels: Vec<i64> = m
            .labels
            .iter()
            .map(|l| if *l == "INBOX" { inbox } else { work })
            .collect();
        set_message_labels(&conn, id, &labels).unwrap();
        upsert_body(&conn, id, None, m.body, 1).unwrap();
        fts::insert(
            &conn,
            id,
            &FtsRow {
                from_name: m.from_name,
                from_email: m.from_email,
                to_text: m.to,
                subject: m.subject,
                body_text: m.body,
            },
        )
        .unwrap();
        recompute_thread(&conn, t).unwrap();
    }
    conn
}

fn subjects(conn: &Connection, q: &str) -> Vec<String> {
    search(conn, q, 50)
        .unwrap()
        .into_iter()
        .map(|h| h.thread.subject)
        .collect()
}

fn mailbox() -> Connection {
    let mut m1 = msg(
        "t1",
        "Quarterly budget",
        "The pineapple budget is attached",
        MARCH,
    );
    m1.attach = true;
    m1.unread = true;
    let mut m2 = msg(
        "t1",
        "Quarterly budget",
        "Looks fine, ship the budget",
        MARCH + DAY,
    );
    m2.from_name = "Bob";
    m2.from_email = "bob@corp.test";
    let m3 = msg("t2", "Lunch", "Pizza on friday?", MARCH + 3 * DAY);
    let mut m4 = msg("t3", "Old news", "budget from last year", MARCH - 400 * DAY);
    m4.labels = &["Label_1"];
    seed(&[m1, m2, m3, m4])
}

#[test]
fn free_text_threads_newest_first_deduped() {
    let c = mailbox();
    assert_eq!(subjects(&c, "budget"), ["Quarterly budget", "Old news"]);
    assert_eq!(subjects(&c, "pizza"), ["Lunch"]);
}

#[test]
fn prefix_on_last_token_only() {
    let c = mailbox();
    assert_eq!(subjects(&c, "pinea").len(), 1);
    assert!(subjects(&c, "pinea ").is_empty());
    assert_eq!(subjects(&c, "pineapple bud").len(), 1);
}

#[test]
fn free_text_needs_two_chars() {
    let c = mailbox();
    assert!(subjects(&c, "p").is_empty());
    assert!(subjects(&c, "").is_empty());
    assert!(subjects(&c, "   ").is_empty());
    assert_eq!(subjects(&c, "pi").len(), 2);
    // Operators run at once; a 1-char tail is ignored.
    assert_eq!(subjects(&c, "is:unread p").len(), 1);
}

#[test]
fn operators() {
    let c = mailbox();
    assert_eq!(subjects(&c, "from:alice").len(), 3);
    assert_eq!(subjects(&c, "from:bob"), ["Quarterly budget"]);
    assert_eq!(subjects(&c, "from:bob@corp.test"), ["Quarterly budget"]);
    assert_eq!(subjects(&c, "to:bob").len(), 3);
    assert_eq!(subjects(&c, "has:attachment"), ["Quarterly budget"]);
    assert_eq!(subjects(&c, "is:unread budget"), ["Quarterly budget"]);
    assert_eq!(subjects(&c, "label:inbox").len(), 2);
    assert_eq!(subjects(&c, "label:INBOX budget").len(), 1);
    assert_eq!(subjects(&c, "label:\"work stuff\""), ["Old news"]);
    assert_eq!(subjects(&c, "after:2026-03-03"), ["Lunch"]);
    assert_eq!(subjects(&c, "after:2026/03/01 before:2026/03/03").len(), 1);
    assert_eq!(subjects(&c, "before:2026-01-01"), ["Old news"]);
    assert_eq!(subjects(&c, "budget before:2026-01-01"), ["Old news"]);
}

#[test]
fn operator_without_free_text_returns_best_message() {
    let c = mailbox();
    let hits = search(&c, "has:attachment", 10).unwrap();
    assert_eq!(hits.len(), 1);
    // Thread t1 matches through its first message only.
    let subj: String = c
        .query_row(
            "SELECT subject FROM messages WHERE id = ?1",
            [hits[0].message_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(subj, "Quarterly budget");
    assert_eq!(hits[0].snippet_html, "plain &lt;b&gt;snippet&lt;/b&gt;");
}

#[test]
fn snippet_has_marks_and_is_escaped() {
    let c = seed(&[msg(
        "t",
        "Hi",
        "before <script>alert(1)</script> & the needle in a haystack",
        MARCH,
    )]);
    let hits = search(&c, "needle", 10).unwrap();
    let s = &hits[0].snippet_html;
    assert!(s.contains("<mark>needle</mark>"), "{s}");
    assert!(s.contains("&lt;script&gt;"), "{s}");
    assert!(s.contains("&amp;"), "{s}");
    assert!(!s.contains("<script>"), "{s}");
    assert!(!s.contains('\u{1}') && !s.contains('\u{2}'));
}

#[test]
fn quoted_phrase() {
    let c = mailbox();
    assert_eq!(subjects(&c, "\"ship the budget\"").len(), 1);
    assert!(subjects(&c, "\"budget ship\"").is_empty());
    // Unterminated phrase still works while typing.
    assert_eq!(subjects(&c, "\"ship the bud").len(), 1);
}

#[test]
fn hostile_input_never_errors() {
    let c = mailbox();
    for q in [
        "\"",
        "*",
        "-",
        ":",
        "::",
        "a:b:c",
        "\"\"",
        "AND",
        "OR",
        "NOT",
        "NEAR(",
        "(",
        ")",
        "^x",
        "-budget",
        "budget*",
        "from:",
        "from:\"",
        "to:*",
        "label:",
        "after:",
        "after:2026-13-40",
        "has:",
        "is:foo",
        "col:x",
        "\\",
        "'; DROP TABLE messages;--",
        "{",
        "}",
        "x\"y",
        "\u{1}",
    ] {
        search(&c, q, 10).unwrap_or_else(|e| panic!("{q:?}: {e}"));
    }
}

#[test]
fn diacritics_and_case() {
    let mut m = msg("t", "Café", "Zoë writes", MARCH);
    m.from_name = "Zoë";
    let c = seed(&[m]);
    assert_eq!(subjects(&c, "cafe").len(), 1);
    assert_eq!(subjects(&c, "from:ZOE").len(), 1);
    assert_eq!(subjects(&c, "FROM:zoe").len(), 1);
}

#[test]
fn limit_is_applied() {
    let ms: Vec<Msg> = (0..5)
        .map(|i| {
            msg(
                ["a", "b", "c", "d", "e"][i],
                "Same subject",
                "common word",
                MARCH + i as i64,
            )
        })
        .collect();
    let c = seed(&ms);
    assert_eq!(search(&c, "common", 3).unwrap().len(), 3);
}

#[test]
fn dates() {
    let p = |s| Parsed::parse(s).is_some();
    assert!(p("after:2026-03-01"));
    assert!(!p("after:2026-03"));
    assert!(!p("after:2026-02-xx"));
    assert!(!p("before:1969/01/01"));
}

fn plan(c: &Connection, sql: &str, binds: Vec<Value>) -> Vec<String> {
    let mut st = c.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    st.query_map(params_from_iter(binds), |r| r.get::<_, String>(3))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// A full scan of `messages` (alias m or mm) is never allowed.
#[test]
fn query_plans_avoid_message_scans() {
    let c = mailbox();
    for q in [
        "budget",
        "pine",
        "from:alice budget",
        "to:bob",
        "budget is:unread has:attachment",
        "budget label:inbox after:2026-01-01 before:2027-01-01",
        "\"ship the\" from:bob",
        "has:attachment",
        "is:unread",
        "label:inbox is:unread",
        "after:2026-01-01",
        "label:inbox",
    ] {
        let mut p = Parsed::parse(q).unwrap();
        assert!(p.resolve_labels(&c).unwrap());
        let fts = p.fts_match();
        let mut plans = Vec::new();
        if let Some(m) = &fts {
            let (sql, mut b) = small_sql(&p);
            b[0] = Value::Text(m.clone());
            plans.push(plan(&c, &sql, b));
            let (sql, b) = dense_walk_sql(&p);
            plans.push(plan(&c, &sql, b));
        } else {
            let (sql, mut b) = walk_threads_sql(&p);
            b.push(Value::Integer(50));
            plans.push(plan(&c, &sql, b));
        }
        let (sql, mut b) = thread_messages_sql(&p);
        let mut all = vec![Value::Integer(1)];
        all.append(&mut b);
        plans.push(plan(&c, &sql, all));
        for line in plans.iter().flatten() {
            assert!(
                !(line.starts_with("SCAN") && (line.ends_with(" m") || line.ends_with("messages"))),
                "{q}: {line}
{plans:#?}"
            );
        }
    }
}

#[test]
fn dense_strategy_matches_small_strategy() {
    // More than DENSE matching messages, spread over many threads.
    let ms: Vec<Msg> = (0..(DENSE + 300))
        .map(|i| {
            let mut m = msg("x", "S", "needle", MARCH + i as i64 * 1000);
            // Leak-free unique thread ids via a static table.
            m.thread = Box::leak(format!("th{}", i / 2).into_boxed_str());
            if i % 3 == 0 {
                m.unread = true;
            }
            m
        })
        .collect();
    let c = seed(&ms);
    let hits = search(&c, "needle", 50).unwrap();
    assert_eq!(hits.len(), 50);
    assert!(hits
        .windows(2)
        .all(|w| w[0].thread.last_date >= w[1].thread.last_date));
    assert_eq!(
        hits[0].thread.last_date,
        MARCH + (DENSE as i64 + 299) * 1000
    );
    assert!(hits[0].snippet_html.contains("<mark>needle</mark>"));
    let hits = search(&c, "needle is:unread", 50).unwrap();
    assert_eq!(hits.len(), 50);
    assert!(hits.iter().all(|h| h.thread.unread_count > 0));
}
