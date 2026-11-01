//! Fake mailbox generator and Rust-side benchmarks.

pub mod fake;
pub mod rng;

use std::path::PathBuf;

/// Value of `--<name> <value>` in the process args.
pub fn arg(name: &str) -> Option<String> {
    let flag = format!("--{name}");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == flag {
            return args.next();
        }
    }
    None
}

pub fn db_arg() -> PathBuf {
    arg("db").map_or_else(|| PathBuf::from("bench.db"), PathBuf::from)
}
