//! `fakegen --db bench.db [--seed 42] [--messages 50000] [--threads 8000]`
//! Replaces the file with a fresh fake mailbox.

use std::time::Instant;

use tidemail_bench::fake::{generate, Params};
use tidemail_bench::{arg, db_arg};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = db_arg();
    let d = Params::default();
    let p = Params {
        seed: arg("seed").map_or(Ok(d.seed), |s| s.parse())?,
        messages: arg("messages").map_or(Ok(d.messages), |s| s.parse())?,
        threads: arg("threads").map_or(Ok(d.threads), |s| s.parse())?,
    };
    for suffix in ["", "-wal", "-shm"] {
        let f = format!("{}{suffix}", path.display());
        if std::path::Path::new(&f).exists() {
            std::fs::remove_file(&f)?;
        }
    }
    let t = Instant::now();
    let mut conn = tidemail_db::open(&path)?;
    let stats = generate(&mut conn, &p)?;
    conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")?;
    drop(conn);
    let size = std::fs::metadata(&path)?.len();
    println!(
        "{} in {:.1}s, {:.0} MB: {}",
        path.display(),
        t.elapsed().as_secs_f64(),
        size as f64 / 1e6,
        serde_json::to_string(&stats)?
    );
    Ok(())
}
