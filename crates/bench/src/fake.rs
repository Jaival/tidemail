//! Fake mailbox generator. Fixed seed => same mailbox, byte for byte.
//!
//! Default: 3 accounts, 50,000 messages, 8,000 threads. Newsletters,
//! receipts (with inline `cid:` logos), CI notifications, conversations
//! (plain text and HTML replies), attachments metadata, and hostile HTML from
//! `fixtures/hostile/` mixed into ~2.5% of HTML messages.

use std::fmt::Write as _;

use tidemail_db::rusqlite::Connection;
use tidemail_db::types::BodyState;
use tidemail_db::write::{self as w, NewAccount, NewAttachment, NewMessage};
use tidemail_db::Result;
use tidemail_render as render;
use tidemail_search::fts::{self, FtsRow};

use crate::rng::Rng;

/// 2026-11-01T00:00:00Z. Fixed so the mailbox does not depend on the clock.
pub const NOW_MS: i64 = 1_793_491_200_000;
const MINUTE: i64 = 60_000;
const DAY: i64 = 86_400_000;

const LOGO_PNG: &[u8] = include_bytes!("../fixtures/logo.png");

/// Hostile HTML samples (also used by renderer tests, read from disk).
pub const HOSTILE: &[(&str, &str)] = &[
    ("script", include_str!("../fixtures/hostile/script.html")),
    (
        "event-handlers",
        include_str!("../fixtures/hostile/event-handlers.html"),
    ),
    (
        "javascript-links",
        include_str!("../fixtures/hostile/javascript-links.html"),
    ),
    (
        "forms-frames",
        include_str!("../fixtures/hostile/forms-frames.html"),
    ),
    (
        "tracking",
        include_str!("../fixtures/hostile/tracking.html"),
    ),
    (
        "css-overlay",
        include_str!("../fixtures/hostile/css-overlay.html"),
    ),
    (
        "wide-table",
        include_str!("../fixtures/hostile/wide-table.html"),
    ),
    (
        "cid-images",
        include_str!("../fixtures/hostile/cid-images.html"),
    ),
    (
        "mutation-xss",
        include_str!("../fixtures/hostile/mutation-xss.html"),
    ),
    (
        "spoofing",
        include_str!("../fixtures/hostile/spoofing.html"),
    ),
];
const HOSTILE_CID: &str = "image001.png@01DA0001";

pub struct Params {
    pub seed: u64,
    pub messages: usize,
    pub threads: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            seed: 42,
            messages: 50_000,
            threads: 8_000,
        }
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Stats {
    pub accounts: usize,
    pub threads: usize,
    pub messages: usize,
    pub bodies: usize,
    pub attachments: usize,
    pub hostile: usize,
    pub big_newsletters: usize,
}

const ACCOUNTS: &[(&str, &str, &str, &[&str])] = &[
    (
        "alex.morgan@personal.example",
        "Alex Morgan",
        "#3b82f6",
        &["Family", "Travel", "Receipts", "Finance"],
    ),
    (
        "alex@atlas-work.example",
        "Alex Morgan",
        "#f59e0b",
        &["Projects/Atlas", "Projects/Borealis", "Hiring", "Ops"],
    ),
    (
        "hello@sideproject.example",
        "Side Project",
        "#10b981",
        &["Users", "Billing"],
    ),
];
const ACCOUNT_WEIGHTS: [f64; 3] = [0.5, 0.35, 0.15];

const SYSTEM_LABELS: &[(&str, &str)] = &[
    ("INBOX", "Inbox"),
    ("SENT", "Sent"),
    ("DRAFT", "Drafts"),
    ("STARRED", "Starred"),
    ("UNREAD", "Unread"),
    ("IMPORTANT", "Important"),
    ("TRASH", "Trash"),
    ("SPAM", "Spam"),
    ("CATEGORY_PROMOTIONS", "Promotions"),
    ("CATEGORY_UPDATES", "Updates"),
];

const FIRST: &[&str] = &[
    "Ava", "Ben", "Chloé", "Daniel", "Elif", "Farah", "Gabriel", "Hana", "Ivan", "Jonas", "Keiko",
    "Liam", "Maya", "Noah", "Olivia", "Priya", "Quinn", "Rafael", "Sofia", "Tomás", "Uma",
    "Victor", "Wen", "Ximena", "Yusuf", "Zoë", "Amara", "Bruno", "Camille", "Dmitri", "Emeka",
    "Freya", "Giulia", "Hiro", "Ingrid", "José", "Kai", "Leila", "Mateo", "Nina",
];
const LAST: &[&str] = &[
    "Anderson",
    "Becker",
    "Costa",
    "Dubois",
    "Eriksen",
    "Fischer",
    "García",
    "Haddad",
    "Ito",
    "Jensen",
    "Kowalski",
    "López",
    "Müller",
    "Nakamura",
    "Okafor",
    "Patel",
    "Quintero",
    "Rossi",
    "Schmidt",
    "Tanaka",
    "Ueda",
    "Varga",
    "Wójcik",
    "Xu",
    "Yilmaz",
    "Zhang",
    "Ahmed",
    "Brown",
    "Chen",
    "Diaz",
    "Evans",
    "Fernández",
    "Gupta",
    "Hughes",
    "Iyer",
    "Johansson",
    "Kim",
    "Larsen",
    "Moreau",
    "Novak",
];
const DOMAINS: &[&str] = &[
    "mail.example",
    "corp.example",
    "studio.test",
    "uni.example",
    "design.test",
    "ops.example",
    "fastmail.test",
    "consulting.example",
];
const BRANDS: &[(&str, &str)] = &[
    ("The Morning Brief", "morningbrief.example"),
    ("Rust Weekly Digest", "rustdigest.example"),
    ("Product Notes", "productnotes.example"),
    ("City Events", "cityevents.example"),
    ("Frontend Focus Daily", "frontendfocus.example"),
    ("Trailhead Outdoors", "trailhead.example"),
    ("Kitchen Table Recipes", "kitchentable.example"),
    ("Market Pulse", "marketpulse.example"),
    ("Design Systems Monthly", "dsmonthly.example"),
    ("Indie Hackers Roundup", "indieroundup.example"),
    ("Science Sunday", "sciencesunday.example"),
    ("Travel Deals Weekly", "traveldeals.example"),
];
const SHOPS: &[(&str, &str)] = &[
    ("Northwind Outfitters", "northwind.example"),
    ("Contoso Electronics", "contoso.example"),
    ("Fabrikam Books", "fabrikam.example"),
    ("Tailspin Air", "tailspin.example"),
    ("Wingtip Grocery", "wingtip.example"),
    ("Adventure Works Cycles", "adventureworks.example"),
    ("Litware Cloud", "litware.example"),
    ("Proseware Rides", "proseware.example"),
];
const TOPICS: &[&str] = &[
    "Q3 planning doc",
    "Dinner on Friday?",
    "Contract renewal",
    "Offsite agenda",
    "Flight to Lisbon",
    "Design review notes",
    "Hiring loop for the backend role",
    "Bug in the export job",
    "Apartment viewing",
    "Budget for next year",
    "Weekend hike",
    "Release checklist v2.4",
    "Invoice question",
    "Intro: you two should meet",
    "Server migration window",
    "Birthday plans",
    "Feedback on the draft",
    "Moving the standup",
    "Customer escalation",
    "Board deck",
    "Kids' school pickup",
    "Conference talk proposal",
    "Database upgrade",
    "Photos from the trip",
    "Onboarding plan",
    "Pricing page copy",
    "On-call handover",
    "Tax documents",
    "Book club pick",
    "API rate limits",
    "Wedding RSVP",
    "Security review findings",
    "Garden project",
    "Roadmap sync",
    "Lunch next week",
    "Data retention policy",
    "New laptop setup",
    "Team retro",
    "Car insurance renewal",
    "Workshop materials",
];
const WORDS: &[&str] = &[
    "the",
    "a",
    "we",
    "should",
    "maybe",
    "next",
    "week",
    "plan",
    "update",
    "team",
    "project",
    "quickly",
    "review",
    "draft",
    "send",
    "numbers",
    "meeting",
    "tomorrow",
    "please",
    "thanks",
    "budget",
    "launch",
    "customer",
    "issue",
    "fix",
    "deploy",
    "notes",
    "idea",
    "think",
    "agree",
    "schedule",
    "call",
    "time",
    "works",
    "for",
    "me",
    "you",
    "it",
    "this",
    "that",
    "with",
    "about",
    "before",
    "after",
    "friday",
    "monday",
    "morning",
    "afternoon",
    "report",
    "attached",
    "version",
    "latest",
    "feedback",
    "changes",
    "small",
    "large",
    "important",
    "later",
    "soon",
    "today",
    "question",
    "answer",
    "data",
    "server",
    "design",
    "copy",
    "price",
    "travel",
    "hotel",
    "flight",
    "dinner",
    "coffee",
    "office",
    "remote",
    "doc",
    "sheet",
    "slides",
    "timeline",
    "risk",
    "blocker",
    "done",
    "pending",
    "approve",
    "confirm",
    "budget",
    "estimate",
    "scope",
    "goal",
    "metric",
    "users",
    "growth",
    "release",
    "test",
    "bug",
    "crash",
    "export",
    "import",
    "backup",
    "restore",
    "weekend",
    "family",
    "photos",
    "trip",
    "garden",
    "school",
    "book",
    "recipe",
    "market",
    "city",
    "event",
    "ticket",
    "seat",
    "window",
    "sync",
    "async",
    "queue",
    "cache",
    "index",
    "query",
];
const FILES: &[(&str, &str)] = &[
    ("report.pdf", "application/pdf"),
    ("photo.jpg", "image/jpeg"),
    (
        "budget.xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    (
        "slides.pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
    ("notes.txt", "text/plain"),
    ("archive.zip", "application/zip"),
    ("setup.exe", "application/x-msdownload"),
    ("statement.html", "text/html"),
    (
        "contract.docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Newsletter,
    Receipt,
    Notification,
    Conversation,
}

#[derive(Clone, Copy, PartialEq)]
enum Fate {
    Inbox,
    Archived,
    Trash,
    Spam,
}

#[derive(Clone)]
struct Person {
    name: String,
    email: String,
}

struct Sys {
    inbox: i64,
    sent: i64,
    draft: i64,
    starred: i64,
    unread: i64,
    important: i64,
    trash: i64,
    spam: i64,
    promotions: i64,
    updates: i64,
}

struct Acct {
    id: i64,
    me: Person,
    sys: Sys,
    user: Vec<i64>,
    receipts: Option<i64>,
}

struct FakeAtt {
    filename: String,
    mime: &'static str,
    size: u32,
    content_id: Option<String>,
    blob: bool,
}

struct Msg {
    from: Person,
    to_text: String,
    subject: String,
    date: i64,
    labels: Vec<i64>,
    html: Option<String>,
    text: String,
    atts: Vec<FakeAtt>,
}

struct Gen<'c> {
    tx: &'c Connection,
    rng: Rng,
    stats: Stats,
    people: Vec<Person>,
}

pub fn generate(conn: &mut Connection, p: &Params) -> Result<Stats> {
    let tx = conn.transaction()?;
    let mut g = Gen {
        tx: &tx,
        rng: Rng::new(p.seed),
        stats: Stats::default(),
        people: Vec::new(),
    };
    let accts = g.accounts()?;
    g.people = (0..300)
        .map(|_| {
            let f = *g.rng.pick(FIRST);
            let l = *g.rng.pick(LAST);
            let d = *g.rng.pick(DOMAINS);
            Person {
                name: format!("{f} {l}"),
                email: format!("{}.{}@{d}", ascii(f), ascii(l)),
            }
        })
        .collect();

    // Plan thread kinds and sizes: 45% single-message threads, the rest
    // conversations with a heavy tail, summing to exactly `p.messages`.
    let n_single = p.threads * 45 / 100;
    let n_conv = p.threads - n_single;
    assert!(
        p.messages >= n_single + 2 * n_conv,
        "too few messages for the thread count"
    );
    let mut plan: Vec<(Kind, usize)> = (0..n_single)
        .map(|i| match i % 9 {
            0..=4 => (Kind::Newsletter, 1),
            5..=7 => (Kind::Receipt, 1),
            _ => (Kind::Notification, 1),
        })
        .collect();
    let tail: Vec<f64> = (0..n_conv).map(|_| g.rng.f64().powf(-0.6) - 1.0).collect();
    let budget = (p.messages - n_single - 2 * n_conv) as f64;
    let sum: f64 = tail.iter().sum::<f64>().max(1e-9);
    let mut counts: Vec<usize> = tail
        .iter()
        .map(|x| 2 + ((x / sum * budget) as usize).min(400))
        .collect();
    let mut total: usize = counts.iter().sum();
    while total < p.messages - n_single {
        let i = g.rng.below(n_conv);
        counts[i] += 1;
        total += 1;
    }
    plan.extend(counts.into_iter().map(|c| (Kind::Conversation, c)));
    g.rng.shuffle(&mut plan);

    let n_news = plan.iter().filter(|(k, _)| *k == Kind::Newsletter).count();
    let big_every = (n_news / 30).max(1);
    let mut news_i = 0;

    let mut thread_ids = Vec::with_capacity(plan.len());
    for (kind, count) in plan {
        let acct = &accts[g.weighted_account()];
        let age = (g.rng.f64().powi(2) * (6 * 365 * DAY) as f64) as i64;
        let end = NOW_MS - age - g.rng.range(0, 6 * 60) * MINUTE;
        let r = g.rng.f64();
        let fate = if r < 0.02 {
            Fate::Trash
        } else if r < 0.03 && kind == Kind::Newsletter {
            Fate::Spam
        } else if g.rng.chance(if age < 45 * DAY { 0.75 } else { 0.06 }) {
            Fate::Inbox
        } else {
            Fate::Archived
        };
        let recent = age < 30 * DAY;
        let gid = g.rng.hex_id();
        let tid = w::ensure_thread(g.tx, acct.id, &gid)?;
        thread_ids.push(tid);
        let msgs = match kind {
            Kind::Newsletter => {
                let big = news_i % big_every == 0 && g.stats.big_newsletters < 30;
                news_i += 1;
                if big {
                    g.stats.big_newsletters += 1;
                }
                vec![g.newsletter(acct, end, fate, recent, big)]
            }
            Kind::Receipt => vec![g.receipt(acct, end, fate, recent)],
            Kind::Notification => vec![g.notification(acct, end, fate, recent)],
            Kind::Conversation => g.conversation(acct, end, count, fate, recent),
        };
        for m in msgs {
            g.emit(acct, tid, m)?;
        }
    }
    for tid in &thread_ids {
        w::recompute_thread(g.tx, *tid)?;
    }
    g.stats.threads = thread_ids.len();
    let stats = g.stats;
    tx.commit()?;

    conn.execute(
        "INSERT INTO messages_fts(messages_fts) VALUES('optimize')",
        [],
    )?;
    conn.execute_batch("ANALYZE")?;
    Ok(stats)
}

impl Gen<'_> {
    fn accounts(&mut self) -> Result<Vec<Acct>> {
        let mut out = Vec::new();
        for (i, (email, name, color, user)) in ACCOUNTS.iter().enumerate() {
            let id = w::insert_account(
                self.tx,
                &NewAccount {
                    email,
                    display_name: Some(name),
                    color,
                    sort_order: i as i64,
                },
            )?;
            let mut sys = Vec::new();
            for (gid, lname) in SYSTEM_LABELS {
                sys.push(w::upsert_label(self.tx, id, gid, lname, true, None)?);
            }
            let mut user_ids = Vec::new();
            let mut receipts = None;
            for (j, lname) in user.iter().enumerate() {
                let lid =
                    w::upsert_label(self.tx, id, &format!("Label_{}", j + 1), lname, false, None)?;
                if *lname == "Receipts" {
                    receipts = Some(lid);
                }
                user_ids.push(lid);
            }
            out.push(Acct {
                id,
                me: Person {
                    name: (*name).to_owned(),
                    email: (*email).to_owned(),
                },
                sys: Sys {
                    inbox: sys[0],
                    sent: sys[1],
                    draft: sys[2],
                    starred: sys[3],
                    unread: sys[4],
                    important: sys[5],
                    trash: sys[6],
                    spam: sys[7],
                    promotions: sys[8],
                    updates: sys[9],
                },
                user: user_ids,
                receipts,
            });
        }
        self.stats.accounts = out.len();
        Ok(out)
    }

    fn weighted_account(&mut self) -> usize {
        let mut r = self.rng.f64();
        for (i, wgt) in ACCOUNT_WEIGHTS.iter().enumerate() {
            if r <= *wgt {
                return i;
            }
            r -= wgt;
        }
        ACCOUNT_WEIGHTS.len() - 1
    }

    fn sentence(&mut self) -> String {
        let n = self.rng.range(6, 16);
        let mut s = String::new();
        for i in 0..n {
            let wd = *self.rng.pick(WORDS);
            if i == 0 {
                let mut c = wd.chars();
                s.extend(c.next().map(|c| c.to_ascii_uppercase()));
                s.push_str(c.as_str());
            } else {
                s.push(' ');
                s.push_str(wd);
            }
        }
        s.push(if self.rng.chance(0.15) { '?' } else { '.' });
        s
    }

    fn paragraph(&mut self, min: i64, max: i64) -> String {
        let n = self.rng.range(min, max);
        (0..n)
            .map(|_| self.sentence())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn headline(&mut self) -> String {
        let mut s = self.sentence();
        s.pop();
        s
    }

    /// Common labels for a received message.
    fn inbox_labels(&mut self, acct: &Acct, fate: Fate, recent: bool, unread_p: f64) -> Vec<i64> {
        let mut ls = Vec::new();
        match fate {
            Fate::Inbox => ls.push(acct.sys.inbox),
            Fate::Trash => ls.push(acct.sys.trash),
            Fate::Spam => ls.push(acct.sys.spam),
            Fate::Archived => {}
        }
        let p = if fate == Fate::Inbox && recent {
            unread_p
        } else {
            unread_p / 10.0
        };
        if self.rng.chance(p) {
            ls.push(acct.sys.unread);
        }
        if self.rng.chance(0.03) {
            ls.push(acct.sys.starred);
        }
        ls
    }

    fn newsletter(&mut self, acct: &Acct, date: i64, fate: Fate, recent: bool, big: bool) -> Msg {
        let (brand, domain) = *self.rng.pick(BRANDS);
        let issue = self.rng.range(10, 999);
        let headline = self.headline();
        let token = self.rng.hex_id();
        let target = if big {
            1_000_000
        } else {
            self.rng.range(8_000, 30_000) as usize
        };
        let mut h = String::with_capacity(target + 4_000);
        let _ = write!(
            h,
            "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{brand}</title><style>\
             body{{margin:0;background:#f4f4f7;font-family:Georgia,serif;color:#222}}\
             .wrap{{max-width:640px;margin:0 auto;background:#fff}}\
             .hero{{background:#0b3d91;color:#fff;padding:24px}}\
             h2{{font-size:20px;margin:0 0 8px}}\
             a.btn{{display:inline-block;padding:10px 16px;background:#e63946;color:#fff;text-decoration:none;border-radius:4px}}\
             @media (max-width:600px){{.col{{display:block!important;width:100%!important}}}}\
             </style></head><body><table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\">\
             <tr><td align=\"center\"><div class=\"wrap\"><div class=\"hero\"><h1>{brand}</h1>\
             <p>Issue #{issue}: {}</p></div>",
            esc(&headline)
        );
        let mut n = 0;
        while h.len() < target {
            n += 1;
            let hl = self.headline();
            let para = self.paragraph(3, 7);
            let _ = write!(
                h,
                "<table role=\"presentation\" width=\"100%\"><tr>\
                 <td class=\"col\" width=\"160\" style=\"padding:12px\"><img src=\"https://img.{domain}/a/{issue}-{n}.jpg\" width=\"160\" height=\"100\" alt=\"{0}\"></td>\
                 <td class=\"col\" style=\"padding:12px;vertical-align:top\"><h2>{0}</h2><p>{1}</p>\
                 <a class=\"btn\" href=\"https://{domain}/r/{token}/{n}?utm_source=newsletter&amp;utm_medium=email\">Read more</a></td>\
                 </tr></table>",
                esc(&hl),
                esc(&para)
            );
        }
        let _ = write!(
            h,
            "<p style=\"font-size:11px;color:#888;padding:16px\">You get this email because you subscribed to {brand}. \
             <a href=\"https://{domain}/unsubscribe?u={token}\">Unsubscribe</a></p>\
             <img src=\"https://t.{domain}/open.gif?u={token}\" width=\"1\" height=\"1\" alt=\"\" style=\"display:none\">\
             </div></td></tr></table></body></html>"
        );
        let mut labels = self.inbox_labels(acct, fate, recent, 0.6);
        labels.push(acct.sys.promotions);
        Msg {
            from: Person {
                name: brand.to_owned(),
                email: format!("news@{domain}"),
            },
            to_text: format!("{} <{}>", acct.me.name, acct.me.email),
            subject: format!("{brand} #{issue}: {headline}"),
            date,
            labels,
            html: Some(h),
            text: String::new(),
            atts: Vec::new(),
        }
    }

    fn receipt(&mut self, acct: &Acct, date: i64, fate: Fate, recent: bool) -> Msg {
        let (shop, domain) = *self.rng.pick(SHOPS);
        let order = self.rng.range(100_000, 999_999);
        let cid = format!("logo@{domain}");
        let mut h = format!(
            "<html><body style=\"font-family:Segoe UI,Arial,sans-serif\">\
             <img src=\"cid:{cid}\" alt=\"{shop}\" width=\"120\" height=\"40\">\
             <h2>Thanks for your order #{order}</h2>\
             <table cellpadding=\"6\" style=\"border-collapse:collapse;width:100%\">\
             <tr style=\"background:#eee\"><th align=\"left\">Item</th><th>Qty</th><th align=\"right\">Price</th></tr>"
        );
        let mut total = 0;
        for _ in 0..self.rng.range(1, 6) {
            let item = self.headline();
            let qty = self.rng.range(1, 3);
            let cents = self.rng.range(199, 25_000);
            total += qty * cents;
            let _ = write!(
                h,
                "<tr><td>{}</td><td align=\"center\">{qty}</td><td align=\"right\">${}.{:02}</td></tr>",
                esc(&item),
                cents / 100,
                cents % 100
            );
        }
        let _ = write!(
            h,
            "<tr><td colspan=\"2\"><b>Total</b></td><td align=\"right\"><b>${}.{:02}</b></td></tr></table>\
             <p><a href=\"https://{domain}/orders/{order}\">View your order</a></p></body></html>",
            total / 100,
            total % 100
        );
        let mut atts = vec![FakeAtt {
            filename: "logo.png".into(),
            mime: "image/png",
            size: LOGO_PNG.len() as u32,
            content_id: Some(cid),
            blob: true,
        }];
        if self.rng.chance(0.5) {
            atts.push(FakeAtt {
                filename: format!("invoice-{order}.pdf"),
                mime: "application/pdf",
                size: self.rng.range(20_000, 400_000) as u32,
                content_id: None,
                blob: false,
            });
        }
        let mut labels = self.inbox_labels(acct, fate, recent, 0.4);
        labels.push(acct.sys.updates);
        labels.extend(acct.receipts);
        let subject = match self.rng.below(3) {
            0 => format!("Your {shop} order #{order}"),
            1 => format!("Receipt for your payment to {shop}"),
            _ => format!("Order #{order} has shipped"),
        };
        Msg {
            from: Person {
                name: shop.to_owned(),
                email: format!("orders@{domain}"),
            },
            to_text: format!("{} <{}>", acct.me.name, acct.me.email),
            subject,
            date,
            labels,
            html: Some(h),
            text: String::new(),
            atts,
        }
    }

    fn notification(&mut self, acct: &Acct, date: i64, fate: Fate, recent: bool) -> Msg {
        let build = self.rng.range(1_000, 99_999);
        let ok = self.rng.chance(0.6);
        let status = if ok { "passed" } else { "failed" };
        let h = format!(
            "<div style=\"font-family:Consolas,monospace\"><p>Build #{build} {status} on <b>main</b>.</p>\
             <pre style=\"background:#111;color:#eee;padding:8px\">{}</pre>\
             <p><a href=\"https://ci.example/builds/{build}\">View build</a></p></div>",
            if ok {
                "test result: ok. 412 passed; 0 failed".to_owned()
            } else {
                format!("error[E0308]: mismatched types\n  --> src/sync/queue.rs:{}:9", self.rng.range(10, 900))
            }
        );
        let mut labels = self.inbox_labels(acct, fate, recent, 0.5);
        labels.push(acct.sys.updates);
        Msg {
            from: Person {
                name: "Buildbot".into(),
                email: "noreply@ci.example".into(),
            },
            to_text: format!("{} <{}>", acct.me.name, acct.me.email),
            subject: format!("[atlas] Build #{build} {status} on main"),
            date,
            labels,
            html: Some(h),
            text: String::new(),
            atts: Vec::new(),
        }
    }

    fn conversation(
        &mut self,
        acct: &Acct,
        end: i64,
        count: usize,
        fate: Fate,
        recent: bool,
    ) -> Vec<Msg> {
        let others: Vec<Person> = (0..self.rng.range(1, 4))
            .map(|_| self.rng.pick(&self.people).clone())
            .collect();
        let topic = *self.rng.pick(TOPICS);
        let user_label =
            (!acct.user.is_empty() && self.rng.chance(0.3)).then(|| *self.rng.pick(&acct.user));
        let draft_thread = self.rng.chance(0.015);
        let important = self.rng.chance(0.2);

        let mut dates = vec![end];
        for _ in 1..count {
            let gap = self
                .rng
                .range(10 * MINUTE, if count > 50 { DAY / 2 } else { 3 * DAY });
            dates.push(dates.last().unwrap() - gap);
        }
        dates.reverse();

        let mut out: Vec<Msg> = Vec::with_capacity(count);
        for (i, date) in dates.into_iter().enumerate() {
            let last = i + 1 == count;
            let from_me = if i == 0 {
                self.rng.chance(0.2)
            } else {
                self.rng.chance(0.35)
            } || (draft_thread && last);
            let from = if from_me {
                acct.me.clone()
            } else {
                self.rng.pick(&others).clone()
            };
            let to_text = std::iter::once(&acct.me)
                .chain(others.iter())
                .filter(|p| p.email != from.email)
                .map(|p| format!("{} <{}>", p.name, p.email))
                .collect::<Vec<_>>()
                .join(", ");
            let subject = if i == 0 {
                topic.to_owned()
            } else {
                format!("Re: {topic}")
            };

            let mut body = self.paragraph(1, 5);
            if self.rng.chance(0.4) {
                body.push_str("\n\n");
                body.push_str(&self.paragraph(1, 4));
            }
            let first_name = from.name.split(' ').next().unwrap_or("").to_owned();
            let quote = out.last().map(|p: &Msg| {
                let prev = if p.html.is_some() {
                    render::html_to_text(p.html.as_deref().unwrap())
                } else {
                    p.text.clone()
                };
                let cut: String = prev.chars().take(400).collect();
                (p.from.name.clone(), cut)
            });
            let (html, text) = if self.rng.chance(0.5) {
                let mut t = format!("{body}\n\n{first_name}");
                if let Some((who, q)) = &quote {
                    let _ = write!(t, "\n\nOn an earlier day, {who} wrote:\n");
                    for l in q.lines() {
                        let _ = writeln!(t, "> {l}");
                    }
                }
                (None, t)
            } else {
                let mut h = String::from("<div dir=\"ltr\">");
                for p in body.split("\n\n") {
                    let _ = write!(h, "<p>{}</p>", esc(p));
                }
                let _ = write!(h, "<p>{}</p></div>", esc(&first_name));
                if let Some((who, q)) = &quote {
                    let _ = write!(
                        h,
                        "<div class=\"gmail_quote\"><div>On an earlier day, {} wrote:</div>\
                         <blockquote class=\"gmail_quote\" style=\"margin:0 0 0 .8ex;border-left:1px #ccc solid;padding-left:1ex\">{}</blockquote></div>",
                        esc(who),
                        esc(q).replace('\n', "<br>")
                    );
                }
                (Some(h), String::new())
            };

            let mut labels = if from_me {
                vec![if draft_thread && last {
                    acct.sys.draft
                } else {
                    acct.sys.sent
                }]
            } else {
                self.inbox_labels(acct, fate, recent, 0.5)
            };
            if from_me && fate == Fate::Trash {
                labels.push(acct.sys.trash);
            }
            if important {
                labels.push(acct.sys.important);
            }
            labels.extend(user_label);

            let mut atts = Vec::new();
            if self.rng.chance(0.08) {
                for _ in 0..self.rng.range(1, 2) {
                    let (name, mime) = *self.rng.pick(FILES);
                    atts.push(FakeAtt {
                        filename: name.to_owned(),
                        mime,
                        size: self.rng.range(2_000, 8_000_000) as u32,
                        content_id: None,
                        blob: false,
                    });
                }
            }
            out.push(Msg {
                from,
                to_text,
                subject,
                date,
                labels,
                html,
                text,
                atts,
            });
        }
        out
    }

    /// Mix hostile HTML into ~2.5% of HTML messages. The first ones cycle
    /// through every fixture so each appears at least once.
    fn maybe_hostile(&mut self, m: &mut Msg) {
        let Some(h) = m.html.as_mut() else { return };
        if !(self.stats.hostile < HOSTILE.len() || self.rng.chance(0.025)) {
            return;
        }
        let i = if self.stats.hostile < HOSTILE.len() {
            self.stats.hostile
        } else {
            self.rng.below(HOSTILE.len())
        };
        let (name, sample) = HOSTILE[i];
        let at = h.rfind("</body>").unwrap_or(h.len());
        h.insert_str(at, sample);
        if name == "cid-images" {
            m.atts.push(FakeAtt {
                filename: "image001.png".into(),
                mime: "image/png",
                size: LOGO_PNG.len() as u32,
                content_id: Some(HOSTILE_CID.into()),
                blob: true,
            });
        }
        self.stats.hostile += 1;
    }

    fn emit(&mut self, acct: &Acct, thread_id: i64, mut m: Msg) -> Result<()> {
        self.maybe_hostile(&mut m);
        let gid = self.rng.hex_id();
        let missing = self.rng.chance(0.01);
        let text = match &m.html {
            Some(h) => render::html_to_text(h),
            None => m.text.clone(),
        };
        let snippet: String = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(140)
            .collect();
        let from_email = m.from.email.to_ascii_lowercase();
        let id = w::insert_message(
            self.tx,
            &NewMessage {
                account_id: acct.id,
                gmail_id: &gid,
                thread_id,
                from_name: &m.from.name,
                from_email: &from_email,
                to_text: &m.to_text,
                subject: &m.subject,
                snippet: &snippet,
                internal_date: m.date,
                is_unread: m.labels.contains(&acct.sys.unread),
                is_starred: m.labels.contains(&acct.sys.starred),
                has_attachment: m.atts.iter().any(|a| !a.blob),
                body_state: BodyState::Missing,
            },
        )?;
        w::set_message_labels(self.tx, id, &m.labels)?;
        for (i, a) in m.atts.iter().enumerate() {
            let part = format!("{}", i + 1);
            let att_gid = format!("ANGjdJ{gid}{i}");
            let aid = w::insert_attachment(
                self.tx,
                &NewAttachment {
                    message_id: id,
                    part_id: &part,
                    gmail_attachment_id: Some(&att_gid),
                    filename: &a.filename,
                    mime_type: a.mime,
                    size: a.size,
                    content_id: a.content_id.as_deref(),
                    is_inline: a.blob,
                },
            )?;
            if a.blob {
                w::insert_inline_blob(self.tx, aid, LOGO_PNG)?;
            }
            self.stats.attachments += 1;
        }
        if !missing {
            let html = m.html.as_deref().map(|h| render::sanitize(h, id));
            w::upsert_body(
                self.tx,
                id,
                html.as_deref(),
                &text,
                render::SANITIZER_VERSION,
            )?;
            self.stats.bodies += 1;
        }
        fts::insert(
            self.tx,
            id,
            &FtsRow {
                from_name: &m.from.name,
                from_email: &from_email,
                to_text: &m.to_text,
                subject: &m.subject,
                body_text: if missing { "" } else { &text },
            },
        )?;
        self.stats.messages += 1;
        Ok(())
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// ASCII-only, lower-case local part for email addresses.
fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' => 'i',
            'ó' | 'ö' => 'o',
            'ú' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'ł' => 'l',
            c => c,
        })
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> (Connection, Stats) {
        let mut conn = tidemail_db::open_in_memory().unwrap();
        let stats = generate(
            &mut conn,
            &Params {
                seed: 7,
                messages: 1_500,
                threads: 200,
            },
        )
        .unwrap();
        (conn, stats)
    }

    fn fingerprint(conn: &Connection) -> String {
        conn.query_row(
            "SELECT group_concat(gmail_id || subject || internal_date || from_email, '|') \
             FROM (SELECT * FROM messages ORDER BY id)",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn exact_counts_and_deterministic() {
        let (a, sa) = small();
        let (b, _) = small();
        assert_eq!(sa.messages, 1_500);
        assert_eq!(sa.threads, 200);
        assert!(sa.hostile >= HOSTILE.len());
        let threads: i64 = a
            .query_row("SELECT count(*) FROM threads", [], |r| r.get(0))
            .unwrap();
        assert_eq!(threads, 200);
        assert_eq!(fingerprint(&a), fingerprint(&b));
        a.execute(
            "INSERT INTO messages_fts(messages_fts) VALUES('integrity-check')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn stored_html_has_no_script() {
        let (conn, _) = small();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM bodies WHERE html_sanitized LIKE '%<script%' \
                 OR html_sanitized LIKE '%onerror=%' OR html_sanitized LIKE '%javascript:%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }
}
