use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tracing::debug;

use stupid_core::{Document, FieldValue};

use super::classify::classify_pattern;
use super::compress::compress_event;
use super::types::{EventTypeCompressed, PrefixSpanConfig, TemporalPattern};

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

/// Generate a deterministic pattern ID from the sequence.
pub(crate) fn pattern_id(sequence: &[EventTypeCompressed]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for item in sequence {
        item.0.hash(&mut hasher);
    }
    format!("pat_{:016x}", hasher.finish())
}
