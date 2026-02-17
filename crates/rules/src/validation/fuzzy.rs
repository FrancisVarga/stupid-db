//! Fuzzy string matching utilities: Levenshtein distance and kebab-case validation.
//!
//! These are `pub(crate)` so other modules in `stupid-rules` can reuse them.

/// Find the closest match using Levenshtein distance. Returns None if best
/// distance exceeds half the candidate length (too dissimilar).
pub(crate) fn fuzzy_match<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let input_lower = input.to_lowercase();
    let mut best: Option<(&str, usize)> = None;

    for &candidate in candidates {
        let dist = levenshtein(&input_lower, &candidate.to_lowercase());
        match best {
            None => best = Some((candidate, dist)),
            Some((_, best_dist)) if dist < best_dist => best = Some((candidate, dist)),
            _ => {}
        }
    }

    best.and_then(|(name, dist)| {
        // Only suggest if edit distance is reasonable (≤ half the longer string)
        let max_len = input.len().max(name.len());
        if dist <= max_len / 2 {
            Some(name)
        } else {
            None
        }
    })
}

/// Levenshtein edit distance between two strings.
pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Check if a string is valid kebab-case: `^[a-z0-9]+(-[a-z0-9]+)*$`
pub(crate) fn is_kebab_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut prev_was_hyphen = true; // treat start as "after separator" to require leading alnum
    for ch in s.chars() {
        if ch == '-' {
            if prev_was_hyphen {
                return false; // double hyphen or leading hyphen
            }
            prev_was_hyphen = true;
        } else if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            prev_was_hyphen = false;
        } else {
            return false; // uppercase, special chars, etc.
        }
    }
    !prev_was_hyphen // must not end with hyphen
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_FEATURES: &[&str] = &[
        "login_count_7d",
        "game_count_7d",
        "unique_games_7d",
        "error_count_7d",
        "popup_interaction_7d",
        "platform_mobile_ratio",
        "session_count_7d",
        "avg_session_gap_hours",
        "vip_group_numeric",
        "currency_encoded",
    ];

    const VALID_ENTITY_TYPES: &[&str] = &[
        "Member", "Device", "Game", "Affiliate", "Currency", "VipGroup", "Error",
        "Platform", "Popup", "Provider",
    ];

    #[test]
    fn levenshtein_basic() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn fuzzy_match_finds_close() {
        assert_eq!(fuzzy_match("login_count", VALID_FEATURES), Some("login_count_7d"));
        assert_eq!(fuzzy_match("Memer", VALID_ENTITY_TYPES), Some("Member"));
    }

    #[test]
    fn fuzzy_match_rejects_distant() {
        assert_eq!(fuzzy_match("zzzzzzzzzzzzz", VALID_FEATURES), None);
    }
}
