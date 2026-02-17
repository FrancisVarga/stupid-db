use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

use stupid_core::{Document, FieldValue};

/// Compressed event type representation for sequence mining.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventTypeCompressed(pub String);

impl std::fmt::Display for EventTypeCompressed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Category of a discovered temporal pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternCategory {
    /// Leads to inactivity.
    Churn,
    /// Leads to increased activity.
    Engagement,
    /// Cascading errors.
    ErrorChain,
    /// Conversion sequence.
    Funnel,
    /// Unclassified.
    Unknown,
}

/// A temporal pattern discovered by PrefixSpan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalPattern {
    /// Unique pattern identifier.
    pub id: String,
    /// The event sequence comprising this pattern.
    pub sequence: Vec<EventTypeCompressed>,
    /// Fraction of members exhibiting this pattern.
    pub support: f64,
    /// Absolute number of members exhibiting this pattern.
    pub member_count: usize,
    /// Average time span of the sequence across members.
    pub avg_duration_secs: f64,
    /// When this pattern was first detected.
    pub first_seen: DateTime<Utc>,
    /// Classification of the pattern.
    pub category: PatternCategory,
    /// Optional human-readable description (e.g. LLM-generated).
    pub description: Option<String>,
}

/// Configuration for PrefixSpan mining.
#[derive(Debug, Clone)]
pub struct PrefixSpanConfig {
    /// Minimum fraction of members exhibiting the pattern.
    pub min_support: f64,
    /// Maximum sequence length to mine.
    pub max_length: usize,
    /// Minimum absolute member count for a pattern.
    pub min_members: usize,
}

impl Default for PrefixSpanConfig {
    fn default() -> Self {
        Self {
            min_support: 0.01,
            max_length: 10,
            min_members: 50,
        }
    }
}

/// Compress a document's event type into a short code.
///
/// - Login -> "L"
/// - GameOpened -> "G:S" (with game subtype) or "G"
/// - PopupModule -> "P:click" (with action) or "P"
/// - API Error -> "E:auth" (with error category) or "E"
/// - Other -> first 3 chars
pub fn compress_event(doc: &Document) -> EventTypeCompressed {
    let get = |name: &str| -> Option<&str> {
        doc.fields.get(name).and_then(FieldValue::as_str).filter(|s| !s.is_empty())
    };

    let code = match doc.event_type.as_str() {
        "Login" => "L".to_string(),
        "GameOpened" | "GridClick" => {
            if let Some(game) = get("game").or_else(|| get("gameName")) {
                // Use first word or short identifier
                let short = game.split_whitespace().next().unwrap_or(game);
                let truncated = if short.len() > 8 { &short[..8] } else { short };
                format!("G:{}", truncated)
            } else {
                "G".to_string()
            }
        }
        "PopupModule" | "PopUpModule" => {
            if let Some(action) = get("action").or_else(|| get("popupType")) {
                let short = if action.len() > 8 { &action[..8] } else { action };
                format!("P:{}", short)
            } else {
                "P".to_string()
            }
        }
        "API Error" => {
            if let Some(code) = get("statusCode") {
                format!("E:{}", code)
            } else if let Some(url) = get("url") {
                let short = url.split('/').last().unwrap_or("unknown");
                let truncated = if short.len() > 8 { &short[..8] } else { short };
                format!("E:{}", truncated)
            } else {
                "E".to_string()
            }
        }
        other => {
            let short = if other.len() > 3 { &other[..3] } else { other };
            short.to_string()
        }
    };

    EventTypeCompressed(code)
}

/// Build per-member event sequences from documents.
///
/// Returns a map from member code to a sequence of compressed events,
/// sorted by timestamp.
pub fn build_sequences(docs: &[Document]) -> HashMap<String, Vec<(DateTime<Utc>, EventTypeCompressed)>> {
    let mut sequences: HashMap<String, Vec<(DateTime<Utc>, EventTypeCompressed)>> = HashMap::new();

    for doc in docs {
        let member_code = match doc.fields.get("memberCode").and_then(FieldValue::as_str) {
            Some(code) if !code.is_empty() => code,
            _ => continue,
        };

        let compressed = compress_event(doc);
        sequences
            .entry(member_code.to_owned())
            .or_default()
            .push((doc.timestamp, compressed));
    }

    // Sort each member's sequence by timestamp.
    for seq in sequences.values_mut() {
        seq.sort_by_key(|(ts, _)| *ts);
    }

    sequences
}

/// Run PrefixSpan sequential pattern mining on member event sequences.
///
/// Returns discovered patterns sorted by support (descending).
pub fn prefixspan(
    sequences: &HashMap<String, Vec<(DateTime<Utc>, EventTypeCompressed)>>,
    config: &PrefixSpanConfig,
) -> Vec<TemporalPattern> {
    let total_members = sequences.len();
    if total_members == 0 {
        return Vec::new();
    }

    let min_count = (config.min_support * total_members as f64).ceil() as usize;
    let min_count = min_count.max(config.min_members);

    // Extract just the event type sequences (drop timestamps for pattern mining).
    let db: Vec<(&str, Vec<&EventTypeCompressed>)> = sequences
        .iter()
        .map(|(member, events)| {
            let seq: Vec<&EventTypeCompressed> = events.iter().map(|(_, e)| e).collect();
            (member.as_str(), seq)
        })
        .collect();

    // Projected database: (member_index, position_in_sequence)
    type ProjectedDB = Vec<(usize, usize)>;

    let mut patterns: Vec<(Vec<EventTypeCompressed>, usize, Vec<usize>)> = Vec::new();

    // Initial projected database: all members start at position 0.
    let initial_db: ProjectedDB = db.iter().enumerate().map(|(i, _)| (i, 0)).collect();

    // Recursive PrefixSpan.
    fn mine(
        prefix: &[EventTypeCompressed],
        projected: &ProjectedDB,
        db: &[(&str, Vec<&EventTypeCompressed>)],
        min_count: usize,
        max_length: usize,
        patterns: &mut Vec<(Vec<EventTypeCompressed>, usize, Vec<usize>)>,
    ) {
        if prefix.len() >= max_length {
            return;
        }

        // Count frequency of each item in the projected database.
        let mut item_projections: HashMap<&EventTypeCompressed, Vec<(usize, usize)>> = HashMap::new();

        for &(member_idx, pos) in projected {
            let seq = &db[member_idx].1;
            // Track which items we've already seen for this member to avoid double-counting.
            let mut seen = std::collections::HashSet::new();
            for j in pos..seq.len() {
                let item = seq[j];
                if seen.insert(item) {
                    item_projections
                        .entry(item)
                        .or_default()
                        .push((member_idx, j + 1));
                }
            }
        }

        for (item, new_projected) in &item_projections {
            // Count unique members in the projection.
            let mut unique_members: Vec<usize> = new_projected.iter().map(|(m, _)| *m).collect();
            unique_members.sort_unstable();
            unique_members.dedup();
            let member_count = unique_members.len();

            if member_count < min_count {
                continue;
            }

            let mut new_prefix = prefix.to_vec();
            new_prefix.push((*item).clone());

            // Only store patterns of length >= 2 (single events aren't interesting).
            if new_prefix.len() >= 2 {
                patterns.push((new_prefix.clone(), member_count, unique_members.clone()));
            }

            // Deduplicate: keep only first occurrence per member.
            let mut deduped: Vec<(usize, usize)> = Vec::new();
            let mut seen_members = std::collections::HashSet::new();
            for &(m, p) in new_projected {
                if seen_members.insert(m) {
                    deduped.push((m, p));
                }
            }

            mine(&new_prefix, &deduped, db, min_count, max_length, patterns);
        }
    }

    mine(&[], &initial_db, &db, min_count, config.max_length, &mut patterns);

    // Convert raw patterns to TemporalPattern structs.
    let now = Utc::now();
    let mut result: Vec<TemporalPattern> = patterns
        .into_iter()
        .map(|(sequence, member_count, member_indices)| {
            let support = member_count as f64 / total_members as f64;

            // Compute average duration across members.
            let avg_duration = compute_avg_pattern_duration(&sequence, &member_indices, sequences, &db);

            let category = classify_pattern(&sequence);
            let id = pattern_id(&sequence);

            TemporalPattern {
                id,
                sequence,
                support,
                member_count,
                avg_duration_secs: avg_duration,
                first_seen: now,
                category,
                description: None,
            }
        })
        .collect();

    // Sort by support descending, then by length descending.
    result.sort_by(|a, b| {
        b.support
            .partial_cmp(&a.support)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.sequence.len().cmp(&a.sequence.len()))
    });

    debug!(
        patterns = result.len(),
        total_members,
        min_count,
        "PrefixSpan mining complete"
    );

    result
}

/// Compute average duration of a pattern across members.
fn compute_avg_pattern_duration(
    pattern: &[EventTypeCompressed],
    member_indices: &[usize],
    sequences: &HashMap<String, Vec<(DateTime<Utc>, EventTypeCompressed)>>,
    db: &[(&str, Vec<&EventTypeCompressed>)],
) -> f64 {
    if pattern.is_empty() || member_indices.is_empty() {
        return 0.0;
    }

    let mut total_secs = 0.0;
    let mut count = 0u64;

    for &member_idx in member_indices {
        let member_code = db[member_idx].0;
        if let Some(events) = sequences.get(member_code) {
            // Find the pattern occurrence and measure its duration.
            if let Some(duration) = find_pattern_duration(pattern, events) {
                total_secs += duration;
                count += 1;
            }
        }
    }

    if count > 0 {
        total_secs / count as f64
    } else {
        0.0
    }
}

/// Find the first occurrence of a pattern in an event sequence and return its duration in seconds.
fn find_pattern_duration(
    pattern: &[EventTypeCompressed],
    events: &[(DateTime<Utc>, EventTypeCompressed)],
) -> Option<f64> {
    if pattern.is_empty() || events.is_empty() {
        return None;
    }

    let mut pattern_idx = 0;
    let mut first_ts = None;
    let mut last_ts = None;

    for (ts, event) in events {
        if pattern_idx < pattern.len() && *event == pattern[pattern_idx] {
            if pattern_idx == 0 {
                first_ts = Some(*ts);
            }
            last_ts = Some(*ts);
            pattern_idx += 1;

            if pattern_idx == pattern.len() {
                break;
            }
        }
    }

    match (first_ts, last_ts) {
        (Some(first), Some(last)) if pattern_idx == pattern.len() => {
            Some((last - first).num_seconds().unsigned_abs() as f64)
        }
        _ => None,
    }
}

/// Classify a pattern based on its event sequence.
fn classify_pattern(sequence: &[EventTypeCompressed]) -> PatternCategory {
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

// ── Config-driven variants ──────────────────────────────────────────

/// Compress an event using a compiled FeatureConfig's event compression rules.
///
/// Looks up `doc.event_type` in `config.event_compression` for the compression
/// code and optional subtype field. Falls back to first 3 characters for
/// unknown event types.
pub fn compress_event_with_config(
    doc: &Document,
    config: &stupid_rules::feature_config::CompiledFeatureConfig,
) -> EventTypeCompressed {
    let get = |name: &str| -> Option<&str> {
        doc.fields.get(name).and_then(FieldValue::as_str).filter(|s| !s.is_empty())
    };

    if let Some(rule) = config.event_compression.get(doc.event_type.as_str()) {
        let code = if let Some(ref field) = rule.subtype_field {
            if let Some(subtype) = get(field) {
                let truncated = if subtype.len() > 8 { &subtype[..8] } else { subtype };
                format!("{}:{}", rule.code, truncated)
            } else {
                rule.code.clone()
            }
        } else {
            rule.code.clone()
        };
        EventTypeCompressed(code)
    } else {
        // Fallback for unknown event types.
        let short = if doc.event_type.len() > 3 { &doc.event_type[..3] } else { &doc.event_type };
        EventTypeCompressed(short.to_string())
    }
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

/// Generate a deterministic pattern ID from the sequence.
fn pattern_id(sequence: &[EventTypeCompressed]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for item in sequence {
        item.0.hash(&mut hasher);
    }
    format!("pat_{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn make_doc(event_type: &str, member: &str, fields: Vec<(&str, &str)>, ts: DateTime<Utc>) -> Document {
        let mut field_map = HashMap::new();
        field_map.insert("memberCode".to_owned(), FieldValue::Text(member.to_owned()));
        for (k, v) in fields {
            field_map.insert(k.to_owned(), FieldValue::Text(v.to_owned()));
        }
        Document {
            id: Uuid::new_v4(),
            timestamp: ts,
            event_type: event_type.to_owned(),
            fields: field_map,
        }
    }

    #[test]
    fn compress_event_types() {
        let ts = Utc::now();

        let login = make_doc("Login", "M001", vec![], ts);
        assert_eq!(compress_event(&login).0, "L");

        let game = make_doc("GameOpened", "M001", vec![("game", "Slots")], ts);
        assert_eq!(compress_event(&game).0, "G:Slots");

        let error = make_doc("API Error", "M001", vec![("statusCode", "401")], ts);
        assert_eq!(compress_event(&error).0, "E:401");

        let popup = make_doc("PopupModule", "M001", vec![("action", "click")], ts);
        assert_eq!(compress_event(&popup).0, "P:click");
    }

    #[test]
    fn build_sequences_groups_by_member() {
        let ts = Utc::now();
        let docs = vec![
            make_doc("Login", "M001", vec![], ts),
            make_doc("Login", "M002", vec![], ts),
            make_doc("GameOpened", "M001", vec![("game", "Slots")], ts + chrono::Duration::seconds(10)),
        ];

        let seqs = build_sequences(&docs);
        assert_eq!(seqs.len(), 2);
        assert_eq!(seqs["M001"].len(), 2);
        assert_eq!(seqs["M002"].len(), 1);
    }

    #[test]
    fn prefixspan_finds_frequent_patterns() {
        let ts = Utc::now();
        let config = PrefixSpanConfig {
            min_support: 0.5,
            max_length: 5,
            min_members: 2,
        };

        // Create 4 members, 3 of which have L -> G:Slots pattern.
        let mut docs = Vec::new();
        for i in 0..3 {
            let member = format!("M{:03}", i);
            docs.push(make_doc("Login", &member, vec![], ts));
            docs.push(make_doc("GameOpened", &member, vec![("game", "Slots")], ts + chrono::Duration::seconds(10)));
        }
        // 4th member has different pattern.
        docs.push(make_doc("Login", "M003", vec![], ts));
        docs.push(make_doc("API Error", "M003", vec![("statusCode", "500")], ts + chrono::Duration::seconds(10)));

        let seqs = build_sequences(&docs);
        let patterns = prefixspan(&seqs, &config);

        // Should find L -> G:Slots as a frequent pattern (3/4 = 0.75 support).
        assert!(!patterns.is_empty());

        let l_g = patterns.iter().find(|p| {
            p.sequence.len() == 2 && p.sequence[0].0 == "L" && p.sequence[1].0 == "G:Slots"
        });
        assert!(l_g.is_some(), "Should find L -> G:Slots pattern");
        assert_eq!(l_g.unwrap().member_count, 3);
    }

    #[test]
    fn prefixspan_empty_input() {
        let seqs: HashMap<String, Vec<(DateTime<Utc>, EventTypeCompressed)>> = HashMap::new();
        let config = PrefixSpanConfig::default();
        let patterns = prefixspan(&seqs, &config);
        assert!(patterns.is_empty());
    }

    #[test]
    fn classify_error_chain() {
        let seq = vec![
            EventTypeCompressed("E:401".into()),
            EventTypeCompressed("E:500".into()),
        ];
        assert_eq!(classify_pattern(&seq), PatternCategory::ErrorChain);
    }

    #[test]
    fn classify_funnel() {
        let seq = vec![
            EventTypeCompressed("L".into()),
            EventTypeCompressed("G:Slots".into()),
        ];
        assert_eq!(classify_pattern(&seq), PatternCategory::Funnel);
    }

    #[test]
    fn classify_churn() {
        let seq = vec![
            EventTypeCompressed("L".into()),
            EventTypeCompressed("E:500".into()),
        ];
        assert_eq!(classify_pattern(&seq), PatternCategory::Churn);
    }

    #[test]
    fn classify_engagement() {
        let seq = vec![
            EventTypeCompressed("G:Slots".into()),
            EventTypeCompressed("G:Poker".into()),
        ];
        assert_eq!(classify_pattern(&seq), PatternCategory::Engagement);
    }

    #[test]
    fn pattern_id_deterministic() {
        let seq = vec![
            EventTypeCompressed("L".into()),
            EventTypeCompressed("G:Slots".into()),
        ];
        let id1 = pattern_id(&seq);
        let id2 = pattern_id(&seq);
        assert_eq!(id1, id2);
    }
}
