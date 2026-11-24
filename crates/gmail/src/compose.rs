//! Outgoing mail: address lists as typed in compose, and the raw RFC 822
//! message for `messages.send` (plain text, UTF-8, base64 body).

use base64::{engine::general_purpose::STANDARD, Engine};

/// One address: `"Name" <email>` or a bare email (`name` empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mailbox {
    pub name: String,
    pub email: String,
}

impl Mailbox {
    /// Header form: RFC 2047 name when it is not plain ASCII.
    fn header(&self) -> String {
        if self.name.is_empty() {
            return self.email.clone();
        }
        let name = if self.name.is_ascii() {
            let escaped = self.name.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        } else {
            encoded_words(&self.name).join(" ")
        };
        format!("{name} <{}>", self.email)
    }
}

impl std::fmt::Display for Mailbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.name.is_empty() {
            f.write_str(&self.email)
        } else {
            write!(f, "{} <{}>", self.name, self.email)
        }
    }
}

/// Parse `a@b.c, "Last, First" <d@e.f>; g@h.i`. Commas and semicolons
/// separate addresses outside quotes and angle brackets. The error names the
/// first entry that is not an email address.
pub fn parse_addresses(input: &str) -> Result<Vec<Mailbox>, String> {
    let mut parts = Vec::new();
    let (mut cur, mut quoted, mut angle) = (String::new(), false, false);
    for ch in input.chars() {
        match ch {
            '"' if !angle => quoted = !quoted,
            '<' if !quoted => angle = true,
            '>' if !quoted => angle = false,
            ',' | ';' if !quoted && !angle => {
                parts.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    parts.push(cur);
    parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (name, email) = match (p.rfind('<'), p.rfind('>')) {
                (Some(a), Some(b)) if a < b && p[b + 1..].trim().is_empty() => {
                    (p[..a].trim().trim_matches('"').trim(), p[a + 1..b].trim())
                }
                _ => ("", p),
            };
            if !is_email(email) {
                return Err(format!("\u{201c}{p}\u{201d} is not an email address"));
            }
            Ok(Mailbox {
                name: name.to_owned(),
                email: email.to_owned(),
            })
        })
        .collect()
}

fn is_email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    let bad = |c: char| c.is_whitespace() || c.is_control() || "<>\"(),;:@[]\\".contains(c);
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !local.chars().any(bad)
        && !domain.chars().any(bad)
}

/// RFC 2047 `=?UTF-8?B?…?=` words, each at most 75 chars, never splitting a
/// character.
fn encoded_words(text: &str) -> Vec<String> {
    // 45 bytes of input -> 60 base64 chars + 12 for the wrapper.
    const MAX: usize = 45;
    let mut words = Vec::new();
    let mut start = 0;
    let mut end = 0;
    for (i, ch) in text.char_indices() {
        if i + ch.len_utf8() - start > MAX {
            words.push(&text[start..end]);
            start = i;
        }
        end = i + ch.len_utf8();
    }
    if start < text.len() {
        words.push(&text[start..]);
    }
    words
        .into_iter()
        .map(|w| format!("=?UTF-8?B?{}?=", STANDARD.encode(w)))
        .collect()
}

fn header_text(text: &str) -> String {
    if text.is_ascii() && !text.chars().any(|c| c.is_control()) {
        text.to_owned()
    } else {
        encoded_words(text).join("\r\n ")
    }
}

/// `Tue, 24 Nov 2026 09:05:00 +0000` from epoch ms (UTC).
fn rfc2822_date(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{}, {day:02} {} {year} {:02}:{:02}:{:02} +0000",
        WEEKDAYS[days.rem_euclid(7) as usize],
        MONTHS[(month - 1) as usize],
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// A new plain-text message. Bcc stays in the headers: Gmail delivers to it
/// and removes it from what recipients get.
pub struct PlainMessage<'a> {
    pub from: &'a Mailbox,
    pub to: &'a [Mailbox],
    pub cc: &'a [Mailbox],
    pub bcc: &'a [Mailbox],
    pub subject: &'a str,
    pub body: &'a str,
    pub date_ms: i64,
}

impl PlainMessage<'_> {
    pub fn to_rfc822(&self) -> Vec<u8> {
        let mut out = String::new();
        let mut header = |name: &str, value: &str| {
            out.push_str(name);
            out.push_str(": ");
            out.push_str(value);
            out.push_str("\r\n");
        };
        let list = |v: &[Mailbox]| {
            v.iter()
                .map(Mailbox::header)
                .collect::<Vec<_>>()
                .join(",\r\n ")
        };
        header("Date", &rfc2822_date(self.date_ms));
        header("From", &self.from.header());
        for (name, v) in [("To", self.to), ("Cc", self.cc), ("Bcc", self.bcc)] {
            if !v.is_empty() {
                header(name, &list(v));
            }
        }
        header("Subject", &header_text(self.subject));
        header("MIME-Version", "1.0");
        header("Content-Type", "text/plain; charset=UTF-8");
        header("Content-Transfer-Encoding", "base64");
        out.push_str("\r\n");
        let body = self.body.replace("\r\n", "\n").replace('\n', "\r\n");
        let encoded = STANDARD.encode(body.as_bytes());
        for line in encoded.as_bytes().chunks(76) {
            out.push_str(std::str::from_utf8(line).expect("base64 is ASCII"));
            out.push_str("\r\n");
        }
        out.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_address_lists() {
        let v = parse_addresses(" a@b.co, \"Last, First\" <d@e.fr>; Ann Lee <g@h.io>,").unwrap();
        assert_eq!(
            v,
            vec![
                Mailbox { name: "".into(), email: "a@b.co".into() },
                Mailbox { name: "Last, First".into(), email: "d@e.fr".into() },
                Mailbox { name: "Ann Lee".into(), email: "g@h.io".into() },
            ]
        );
        assert!(parse_addresses("").unwrap().is_empty());
        for bad in ["bob", "a@b", "a b@c.de", "<>", "x@y.z, nope"] {
            assert!(parse_addresses(bad).is_err(), "{bad}");
        }
        assert_eq!(
            parse_addresses("x@y.z, nope").unwrap_err(),
            "\u{201c}nope\u{201d} is not an email address"
        );
    }

    #[test]
    fn date_is_rfc2822() {
        assert_eq!(rfc2822_date(0), "Thu, 01 Jan 1970 00:00:00 +0000");
        // 2026-11-24 09:05:07 UTC
        assert_eq!(rfc2822_date(1_795_511_107_000), "Tue, 24 Nov 2026 09:05:07 +0000");
        assert_eq!(rfc2822_date(951_782_400_000), "Tue, 29 Feb 2000 00:00:00 +0000");
    }

    #[test]
    fn encoded_words_stay_short_and_whole() {
        let text = "Grüße aus Köln — 東京でお会いしましょう 🎉".repeat(3);
        let words = encoded_words(&text);
        assert!(words.len() > 1);
        let mut back = Vec::new();
        for w in &words {
            assert!(w.len() <= 75, "{w}");
            let b64 = w.strip_prefix("=?UTF-8?B?").unwrap().strip_suffix("?=").unwrap();
            let bytes = STANDARD.decode(b64).unwrap();
            assert!(std::str::from_utf8(&bytes).is_ok());
            back.extend(bytes);
        }
        assert_eq!(String::from_utf8(back).unwrap(), text);
    }

    #[test]
    fn builds_plain_message() {
        let from = Mailbox { name: "Me".into(), email: "me@x.io".into() };
        let to = parse_addresses("Zoë <z@y.io>, b@c.de").unwrap();
        let bcc = parse_addresses("hidden@c.de").unwrap();
        let raw = PlainMessage {
            from: &from,
            to: &to,
            cc: &[],
            bcc: &bcc,
            subject: "Café plans",
            body: "Hi\nsee you ☕\n",
            date_ms: 0,
        }
        .to_rfc822();
        let raw = String::from_utf8(raw).unwrap();
        let (head, body) = raw.split_once("\r\n\r\n").unwrap();
        assert!(raw.is_ascii());
        assert!(head.contains("From: \"Me\" <me@x.io>\r\n"));
        assert!(head.contains("To: =?UTF-8?B?Wm/Dqw==?= <z@y.io>,\r\n b@c.de\r\n"));
        assert!(!head.contains("Cc:"));
        assert!(head.contains("Bcc: hidden@c.de\r\n"));
        assert!(head.contains("Subject: =?UTF-8?B?"));
        assert!(head.contains("Content-Type: text/plain; charset=UTF-8"));
        let decoded = STANDARD.decode(body.replace("\r\n", "")).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hi\r\nsee you ☕\r\n");
    }
}
