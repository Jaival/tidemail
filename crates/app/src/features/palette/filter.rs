//! Filter and rank picker entries: every query word must match; prefix and
//! word-start hits rank above substrings, substrings above scattered letters.

/// Score of one word in `text` (both lower case), or `None` if it does not match.
fn word_score(word: &str, text: &str) -> Option<u32> {
    if let Some(pos) = text.find(word) {
        let at_word_start = pos == 0
            || text[..pos]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_alphanumeric());
        return Some(match (pos, at_word_start) {
            (0, _) => 3000,
            (_, true) => 2000,
            _ => 1000,
        });
    }
    // Letters in order, anywhere: "gtl" finds "Go to label".
    let mut rest = text.chars();
    word.chars().all(|w| rest.any(|t| t == w)).then_some(100)
}

/// Indices of `texts` that match `query`, best first (ties keep input order).
/// An empty query keeps everything in order.
pub fn rank<'a>(query: &str, texts: impl Iterator<Item = &'a str>) -> Vec<usize> {
    let query = query.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut scored: Vec<(usize, u32)> = texts
        .enumerate()
        .filter_map(|(i, text)| {
            let text = text.to_lowercase();
            let score = words
                .iter()
                .map(|w| word_score(w, &text))
                .sum::<Option<u32>>()?;
            // Shorter entries first among equal matches.
            Some((i, score.saturating_sub(text.len() as u32)))
        })
        .collect();
    scored.sort_by_key(|&(_, s)| std::cmp::Reverse(s));
    scored.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::rank;

    fn ranked(q: &str, items: &[&str]) -> Vec<usize> {
        rank(q, items.iter().copied())
    }

    #[test]
    fn empty_query_keeps_order() {
        assert_eq!(ranked("", &["b", "a", "c"]), [0, 1, 2]);
        assert_eq!(ranked("  ", &["b", "a"]), [0, 1]);
    }

    #[test]
    fn prefix_beats_word_start_beats_substring() {
        let items = ["Unarchive", "Mark archive", "Archive"];
        assert_eq!(ranked("arch", &items), [2, 1, 0]);
    }

    #[test]
    fn all_words_must_match() {
        let items = ["Theme: Dark", "Theme: Light", "Layout: Dark Paper"];
        assert_eq!(ranked("theme dark", &items), [0]);
        assert_eq!(ranked("dark", &items), [0, 2]);
    }

    #[test]
    fn scattered_letters_match_last() {
        let items = ["Go to label", "Archive", "Trash"];
        assert_eq!(ranked("gtl", &items), [0]);
        assert_eq!(ranked("zzz", &items), Vec::<usize>::new());
    }

    #[test]
    fn case_is_ignored() {
        assert_eq!(ranked("ARCHIVE", &["archive"]), [0]);
    }
}
