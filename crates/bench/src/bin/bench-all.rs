//! `bench-all --db bench.db [--exe target/release/tidemail.exe] [--skip-app]`
//!
//! Runs the Rust bench, then the release app 3 times per layout preset with
//! `--bench` on a copy of the database (the app marks threads read). Every
//! budget applies to every preset. Exits 1 if any budget is missed.
//! Build first: `cargo build --release -p tidemail -p tidemail-bench`.

use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{Duration, Instant},
};

use tidemail_bench::{arg, db_arg};

const COLD_LAUNCH_MS: f64 = 400.0;
const OPEN_TO_PAINT_MS: f64 = 50.0;
const SCROLL_DROPPED_PCT: f64 = 2.0;
const LAUNCHES: usize = 3;
const PRESETS: [&str; 3] = ["tide", "command", "paper"];
const APP_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(serde::Deserialize)]
struct Report {
    cold_launch_ms: f64,
    jk_ms: Vec<f64>,
    scroll_frames: usize,
    scroll_dropped: usize,
    scroll_blank: usize,
    scroll_max_frame_ms: f64,
}

fn sibling(name: &str) -> PathBuf {
    let exe = std::env::current_exe().expect("current exe");
    exe.with_file_name(name)
}

fn run_app(
    exe: &Path,
    source_db: &Path,
    work: &Path,
    preset: &str,
    n: usize,
) -> Result<Report, String> {
    let db = work.join(format!("bench-{n}.db"));
    for suffix in ["", "-wal"] {
        let from = PathBuf::from(format!("{}{suffix}", source_db.display()));
        let to = PathBuf::from(format!("{}{suffix}", db.display()));
        let _ = std::fs::remove_file(&to);
        if from.exists() {
            std::fs::copy(&from, &to).map_err(|e| format!("copy {}: {e}", from.display()))?;
        }
    }
    let report = work.join(format!("report-{n}.json"));
    let _ = std::fs::remove_file(&report);
    let mut child = Command::new(exe)
        .arg("--db")
        .arg(&db)
        .arg("--bench")
        .arg(&report)
        .arg("--preset")
        .arg(preset)
        .spawn()
        .map_err(|e| format!("start {}: {e}", exe.display()))?;
    let deadline = Instant::now() + APP_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(format!("app exited with {status}"));
            }
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            return Err("app timed out".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let bytes = std::fs::read(&report).map_err(|e| format!("read report: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse report: {e}"))
}

fn p95(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v.get(((v.len().max(1) - 1) as f64 * 0.95).round() as usize)
        .copied()
        .unwrap_or(0.0)
}

/// Print one preset's results; false if a budget is missed.
fn summarize(preset: &str, db: &Path, reports: &[Report]) -> bool {
    let mut ok_all = true;
    let colds: Vec<f64> = reports.iter().map(|r| r.cold_launch_ms).collect();
    let cold = colds.iter().copied().fold(0.0, f64::max);
    let jk = p95(reports.iter().flat_map(|r| r.jk_ms.clone()).collect());
    let frames: usize = reports.iter().map(|r| r.scroll_frames).sum();
    let dropped: usize = reports.iter().map(|r| r.scroll_dropped).sum();
    let blank: usize = reports.iter().map(|r| r.scroll_blank).sum();
    let max_frame = reports
        .iter()
        .map(|r| r.scroll_max_frame_ms)
        .fold(0.0, f64::max);
    let dropped_pct = 100.0 * dropped as f64 / frames.max(1) as f64;

    let mut line = |name: &str, ok: bool, value: String, budget: String| {
        ok_all &= ok;
        println!(
            "{:<4}  {name:<34} {value:>22}   budget {budget}",
            if ok { "PASS" } else { "FAIL" }
        );
    };
    println!(
        "
app bench, {preset} ({LAUNCHES} launches, {})",
        db.display()
    );
    line(
        "cold launch (worst)",
        cold <= COLD_LAUNCH_MS,
        format!(
            "{cold:.0} ms ({})",
            colds
                .iter()
                .map(|c| format!("{c:.0}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        format!("{COLD_LAUNCH_MS} ms"),
    );
    line(
        "j/k open to paint p95",
        jk <= OPEN_TO_PAINT_MS,
        format!("{jk:.1} ms"),
        format!("{OPEN_TO_PAINT_MS} ms"),
    );
    line(
        "scroll dropped frames",
        dropped_pct <= SCROLL_DROPPED_PCT,
        format!("{dropped} / {frames} (max {max_frame:.1} ms)"),
        format!("<= {SCROLL_DROPPED_PCT} %"),
    );
    line(
        "scroll blank frames",
        blank == 0,
        blank.to_string(),
        "0".into(),
    );
    ok_all
}

fn main() -> ExitCode {
    let db = db_arg();
    let mut failed = false;

    let status = Command::new(sibling("bench.exe"))
        .arg("--db")
        .arg(&db)
        .status();
    match status {
        Ok(s) if s.success() => {}
        Ok(_) => failed = true,
        Err(e) => {
            eprintln!("bench-all: cannot run bench: {e}");
            return ExitCode::FAILURE;
        }
    }
    if std::env::args().any(|a| a == "--skip-app") {
        return if failed {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }

    let exe = arg("exe").map_or_else(|| sibling("tidemail.exe"), PathBuf::from);
    let work = std::env::temp_dir().join("tidemail-bench");
    if let Err(e) = std::fs::create_dir_all(&work) {
        eprintln!("bench-all: {e}");
        return ExitCode::FAILURE;
    }
    for preset in PRESETS {
        let mut reports = Vec::new();
        for n in 0..LAUNCHES {
            match run_app(&exe, &db, &work, preset, n) {
                Ok(r) => reports.push(r),
                Err(e) => {
                    eprintln!("bench-all: {preset} launch {}: {e}", n + 1);
                    return ExitCode::FAILURE;
                }
            }
        }
        failed |= !summarize(preset, &db, &reports);
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
