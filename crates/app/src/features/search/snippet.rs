//! `SearchHit::snippet_html` -> plain text plus the byte ranges of the
//! `<mark>` runs. The search crate escapes everything except the marks, so
//! only `<mark>`, `</mark>` and five entities occur.

use std::ops::Range;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snippet {
    pub text: String,
    /// Byte ranges into `text`.
    pub marks: Vec<Range<usize>>,
}

pub fn parse(html: &str) -> Snippet {
    let mut out = Snippet::default();
    let mut start = None;
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("<mark>") {
            start = Some(out.text.len());
            rest = r;
        } else if let Some(r) = rest.strip_prefix("</mark>") {
            if let Some(s) = start.take() {
                out.marks.push(s..out.text.len());
            }
            rest = r;
        } else if let Some((ch, r)) = entity(rest) {
            out.text.push(ch);
            rest = r;
        } else {
            let ch = rest.chars().next().expect("non-empty");
            out.text.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

fn entity(s: &str) -> Option<(char, &str)> {
    [
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&#39;", '\''),
    ]
    .into_iter()
    .find_map(|(e, ch)| s.strip_prefix(e).map(|r| (ch, r)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_become_ranges() {
        let s = parse("the <mark>budget</mark> and <mark>plan</mark>");
        assert_eq!(s.text, "the budget and plan");
        assert_eq!(s.marks, vec![4..10, 15..19]);
        assert_eq!(&s.text[s.marks[1].clone()], "plan");
    }

    #[test]
    fn entities_decode_and_shift_ranges() {
        let s = parse("&lt;a&gt; &amp; <mark>it&#39;s</mark> &quot;ok&quot;");
        assert_eq!(s.text, "<a> & it's \"ok\"");
        assert_eq!(&s.text[s.marks[0].clone()], "it's");
    }

    #[test]
    fn no_marks_and_unicode() {
        let s = parse("… <mark>é</mark>");
        assert_eq!(s.text, "… é");
        assert_eq!(&s.text[s.marks[0].clone()], "é");
        assert!(parse("plain").marks.is_empty());
    }

    #[test]
    fn unclosed_mark_is_dropped() {
        let s = parse("a <mark>b");
        assert_eq!(s.text, "a b");
        assert!(s.marks.is_empty());
    }
}
