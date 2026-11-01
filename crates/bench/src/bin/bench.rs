//! Rust bench: `bench --db bench.db [--json]`. Build with `--release`.
//! Exits 1 if any budget is missed. Stub features (Phase 1) report SKIP.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tidemail_bench::{arg, db_arg};
use tidemail_db::read;
use tidemail_db::types::{ActionKind, ActionRequest, SyncStatus, View};

/// Rust-side budgets (ms). The spec budgets are end to end; these leave room
/// for IPC and paint.
const LIST_PAGE_P95: f64 = 10.0;
const OPEN_DB_FIRST_PAGE: f64 = 100.0;
const THREAD_OPEN_P95: f64 = 15.0;
const SEARCH_P95: f64 = 30.0;
const ACTION_P95: f64 = 16.0;

#[derive(serde::Serialize)]
struct Line {
    name: &'static str,
    status: &'static str,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
    budget_ms: f64,
    samples: usize,
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn line(name: &'static str, mut t: Vec<f64>, budget: f64, use_max: bool) -> Line {
    t.sort_by(|a, b| a.total_cmp(b));
    let p95 = pct(&t, 0.95);
    let max = *t.last().unwrap_or(&0.0);
    let measured = if use_max { max } else { p95 };
    Line {
        name,
        status: if measured <= budget { "PASS" } else { "FAIL" },
        p50_ms: pct(&t, 0.5),
        p95_ms: p95,
        max_ms: max,
        budget_ms: budget,
        samples: t.len(),
    }
}

fn skip(name: &'static str, budget: f64) -> Line {
    Line {
        name,
        status: "SKIP",
        p50_ms: 0.0,
        p95_ms: 0.0,
        max_ms: 0.0,
        budget_ms: budget,
        samples: 0,
    }
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

struct Quiet;
impl tidemail_sync::Notifier for Quiet {
    fn data_changed(&self, _: Vec<i64>, _: Vec<String>) {}
    fn status_changed(&self, _: SyncStatus) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path: PathBuf = db_arg();
    if !path.exists() {
        return Err(format!("{} not found; run fakegen first", path.display()).into());
    }
    let mut lines = Vec::new();

    // Open + first page (cold-launch path, without the webview).
    let t = Instant::now();
    let conn = tidemail_db::open(&path)?;
    let inbox = View {
        label_key: "INBOX".into(),
        account_id: None,
    };
    let first = read::list_threads(&conn, &inbox, None, 100)?;
    lines.push(line(
        "open db + first inbox page",
        vec![ms(t)],
        OPEN_DB_FIRST_PAGE,
        true,
    ));
    if first.items.is_empty() {
        return Err("inbox is empty".into());
    }

    // List pages: walk unified and per-account views, 100 rows per page.
    let accounts = read::list_accounts(&conn)?;
    let mut views = vec![
        inbox.clone(),
        View {
            label_key: "SENT".into(),
            account_id: None,
        },
        View {
            label_key: "STARRED".into(),
            account_id: None,
        },
        View {
            label_key: "CATEGORY_PROMOTIONS".into(),
            account_id: None,
        },
    ];
    for a in &accounts {
        views.push(View {
            label_key: "INBOX".into(),
            account_id: Some(a.id),
        });
        views.push(View {
            label_key: "IMPORTANT".into(),
            account_id: Some(a.id),
        });
    }
    let mut times = Vec::new();
    for _ in 0..5 {
        for v in &views {
            let mut cursor = None;
            for _ in 0..20 {
                let t = Instant::now();
                let page = read::list_threads(&conn, v, cursor, 100)?;
                times.push(ms(t));
                match page.next {
                    Some(c) => cursor = Some(c),
                    None => break,
                }
            }
        }
    }
    lines.push(line("list page (100 rows)", times, LIST_PAGE_P95, false));

    // Thread open + newest body, in inbox order (what j/k does).
    let mut ids = Vec::new();
    let mut cursor = None;
    while ids.len() < 500 {
        let page = read::list_threads(&conn, &inbox, cursor, 100)?;
        ids.extend(page.items.iter().map(|t| t.id));
        match page.next {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    let mut times = Vec::new();
    for id in &ids {
        let t = Instant::now();
        let detail = read::get_thread(&conn, *id)?.ok_or("thread vanished")?;
        if let Some(m) = detail.messages.last() {
            let _ = read::get_body(&conn, m.id)?;
        }
        times.push(ms(t));
    }
    lines.push(line("thread open + body", times, THREAD_OPEN_P95, false));

    // Search, one query per keystroke.
    let typed = [
        "bu",
        "bud",
        "budg",
        "budge",
        "budget",
        "budget r",
        "budget re",
        "budget rev",
    ];
    let ops = [
        "from:news",
        "has:attachment",
        "is:unread",
        "label:inbox is:unread",
        "after:2026/01/01 deploy",
    ];
    match tidemail_search::search(&conn, "probe", 1) {
        Err(tidemail_search::Error::NotImplemented) => {
            lines.push(skip("search per keystroke", SEARCH_P95))
        }
        _ => {
            let mut times = Vec::new();
            for _ in 0..10 {
                for q in typed.iter().chain(ops.iter()) {
                    let t = Instant::now();
                    tidemail_search::search(&conn, q, 50)?;
                    times.push(ms(t));
                }
            }
            lines.push(line("search per keystroke", times, SEARCH_P95, false));
        }
    }

    // Local action write (no network).
    let engine = tidemail_sync::Engine::start(path.clone(), Arc::new(Quiet));
    let probe = ActionRequest {
        thread_ids: vec![ids[0]],
        kind: ActionKind::Star,
    };
    match engine.apply_action(probe) {
        Err(tidemail_sync::Error::NotImplemented) => {
            lines.push(skip("action write (local)", ACTION_P95))
        }
        r => {
            let ticket = r?;
            engine.undo_action(&ticket.action_ids)?;
            let mut times = Vec::new();
            for id in ids.iter().take(100) {
                let t = Instant::now();
                let tk = engine.apply_action(ActionRequest {
                    thread_ids: vec![*id],
                    kind: ActionKind::MarkRead,
                })?;
                times.push(ms(t));
                engine.undo_action(&tk.action_ids)?;
            }
            lines.push(line("action write (local)", times, ACTION_P95, false));
        }
    }

    let failed = lines.iter().any(|l| l.status == "FAIL");
    if arg("json").is_some() || std::env::args().any(|a| a == "--json") {
        println!("{}", serde_json::to_string(&lines)?);
    } else {
        println!(
            "{:<30} {:>6} {:>9} {:>9} {:>9} {:>9}",
            "rust bench", "status", "p50", "p95", "max", "budget"
        );
        for l in &lines {
            println!(
                "{:<30} {:>6} {:>8.2}m {:>8.2}m {:>8.2}m {:>8.1}m",
                l.name, l.status, l.p50_ms, l.p95_ms, l.max_ms, l.budget_ms
            );
        }
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}
