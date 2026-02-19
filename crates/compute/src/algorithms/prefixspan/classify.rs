use super::types::{EventTypeCompressed, PatternCategory};

/// Classify a pattern based on its event sequence.
pub fn classify_pattern(sequence: &[EventTypeCompressed]) -> PatternCategory {
    let has_error = sequence.iter().any(|e| e.0.starts_with('E'));
    let has_login = sequence.iter().any(|e| e.0 == "L");
    let has_game = sequence.iter().any(|e| e.0.starts_with('G'));
    let _has_popup = sequence.iter().any(|e| e.0.starts_with('P'));

    // Error chains: multiple errors in sequence.
    let error_count = sequence.iter().filter(|e| e.0.starts_with('E')).count();
    if error_count >= 2 {
        return PatternCategory::ErrorChain;
    }

    // Churn signals: errors followed by no more games/logins.
    if has_error {
        let last_error_pos = sequence.iter().rposition(|e| e.0.starts_with('E')).unwrap();
        let has_activity_after = sequence[last_error_pos + 1..]
            .iter()
            .any(|e| e.0 == "L" || e.0.starts_with('G'));
        if !has_activity_after {
            return PatternCategory::Churn;
        }
    }

    // Funnel: Login -> Game sequence.
    if has_login && has_game {
        let first_login = sequence.iter().position(|e| e.0 == "L");
        let first_game = sequence.iter().position(|e| e.0.starts_with('G'));
        if let (Some(l), Some(g)) = (first_login, first_game) {
            if l < g {
                return PatternCategory::Funnel;
            }
        }
    }

    // Engagement: multiple game events.
    let game_count = sequence.iter().filter(|e| e.0.starts_with('G')).count();
    if game_count >= 2 {
        return PatternCategory::Engagement;
    }

    PatternCategory::Unknown
}

/// Classify a pattern using declarative PatternConfig rules.
///
/// Evaluates each classification rule in order; first match wins.
/// If no rule matches, returns `PatternCategory::Unknown`.
pub fn classify_pattern_with_rules(
    sequence: &[EventTypeCompressed],
    rules: &[stupid_rules::pattern_config::ClassificationRule],
) -> PatternCategory {
    for rule in rules {
        if matches_classification_condition(sequence, &rule.condition) {
            return pattern_category_from_str(&rule.category);
        }
    }
    PatternCategory::Unknown
}

/// Evaluate a single classification condition against a pattern sequence.
fn matches_classification_condition(
    sequence: &[EventTypeCompressed],
    condition: &stupid_rules::pattern_config::ClassificationCondition,
) -> bool {
    match condition.check.as_str() {
        "count_gte" => {
            let code = condition.event_code.as_deref().unwrap_or("");
            let min = condition.min_count.unwrap_or(0);
            let count = sequence.iter().filter(|e| {
                if code.is_empty() { true } else { e.0.starts_with(code) }
            }).count();
            count >= min
        }
        "sequence_match" => {
            if let Some(ref seq_codes) = condition.sequence {
                // Check if the pattern contains the specified sequence in order.
                let mut seq_idx = 0;
                for event in sequence {
                    if seq_idx < seq_codes.len() && event.0.starts_with(&seq_codes[seq_idx]) {
                        seq_idx += 1;
                    }
                }
                seq_idx >= seq_codes.len()
            } else {
                false
            }
        }
        "has_then_absent" => {
            let present = condition.present_code.as_deref().unwrap_or("");
            let absent = condition.absent_code.as_deref().unwrap_or("");
            let has_present = sequence.iter().any(|e| e.0.starts_with(present));
            if !has_present {
                return false;
            }
            // Check that the absent code doesn't appear after the last occurrence of present.
            let last_present_pos = sequence.iter().rposition(|e| e.0.starts_with(present));
            if let Some(pos) = last_present_pos {
                !sequence[pos + 1..].iter().any(|e| e.0.starts_with(absent))
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Map a category string from PatternConfig YAML to our PatternCategory enum.
fn pattern_category_from_str(s: &str) -> PatternCategory {
    match s {
        "ErrorChain" => PatternCategory::ErrorChain,
        "Churn" => PatternCategory::Churn,
        "Funnel" => PatternCategory::Funnel,
        "Engagement" => PatternCategory::Engagement,
        _ => PatternCategory::Unknown,
    }
}
