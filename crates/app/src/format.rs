//! Display formatting shared by views.

use chrono::{Datelike, Local, TimeZone};
use gpui_kit::{rgb, Hsla, SharedString};

/// List date: time today, "Mar 4" this year, "Mar 4, 2024" before.
pub fn list_date(ms: i64) -> SharedString {
    let Some(d) = Local.timestamp_millis_opt(ms).single() else {
        return SharedString::default();
    };
    let now = Local::now();
    let s = if d.year() != now.year() {
        d.format("%b %-d, %Y").to_string()
    } else if d.date_naive() == now.date_naive() {
        d.format("%-H:%M").to_string()
    } else {
        d.format("%b %-d").to_string()
    };
    s.into()
}

/// "12 s", "5 min", "3 h".
pub fn age(ms: i64) -> String {
    let s = (ms.max(0) as f64 / 1000.0).round() as i64;
    match s {
        0..60 => format!("{s} s"),
        60..3600 => format!("{} min", s / 60),
        _ => format!("{} h", s / 3600),
    }
}

/// `#rrggbb` or `#rgb`; anything else is gray.
pub fn parse_color(hex: &str) -> Hsla {
    let h = hex.trim_start_matches('#');
    let full = match h.len() {
        3 => h.chars().flat_map(|c| [c, c]).collect(),
        _ => h.to_string(),
    };
    u32::from_str_radix(&full, 16)
        .ok()
        .filter(|_| full.len() == 6)
        .map_or_else(|| rgb(0x888888).into(), |v| rgb(v).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages() {
        assert_eq!(age(12_000), "12 s");
        assert_eq!(age(5 * 60_000), "5 min");
        assert_eq!(age(3 * 3_600_000), "3 h");
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#fff"), parse_color("#ffffff"));
        assert_eq!(parse_color("nope"), rgb(0x888888).into());
    }
}
