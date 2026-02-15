use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use stupid_core::{Document, FieldValue, NodeId};
use uuid::Uuid;

/// Derive a stable deterministic `NodeId` from a member code string.
///
/// Uses a simple hash-based construction (not RFC 4122 v5, since the `v5`
/// feature is not enabled). The result is consistent across runs for the
/// same input string.
pub(crate) fn member_code_to_node_id(code: &str) -> NodeId {
    let mut bytes = [0u8; 16];
    // Simple FNV-1a style hash spread across 16 bytes.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in code.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    bytes[..8].copy_from_slice(&h.to_le_bytes());
    // Second hash pass with different seed for remaining bytes.
    let mut h2: u64 = 0x517cc1b727220a95;
    for b in code.as_bytes() {
        h2 ^= *b as u64;
        h2 = h2.wrapping_mul(0x100000001b3);
    }
    bytes[8..].copy_from_slice(&h2.to_le_bytes());
    Uuid::from_bytes(bytes)
}

/// Member feature vector (10-dimensional):
/// [login_count_7d, game_count_7d, unique_games_7d, error_count_7d,
///  popup_interaction_7d, platform_mobile_ratio, session_count_7d,
///  avg_session_gap_hours, vip_group_numeric, currency_encoded]
pub struct MemberFeatures {
    /// Accumulated feature counts per member.
    features: HashMap<NodeId, FeatureAccumulator>,
    /// Reverse mapping from pipeline NodeId back to member code string.
    member_keys: HashMap<NodeId, String>,
}

/// Raw counters accumulated from documents for a single member.
struct FeatureAccumulator {
    login_count: u32,
    game_count: u32,
    unique_games: HashSet<String>,
    error_count: u32,
    popup_count: u32,
    mobile_events: u32,
    total_events: u32,
    session_count: u32,
    session_timestamps: Vec<DateTime<Utc>>,
    vip_group: Option<String>,
    currency: Option<String>,
}

impl FeatureAccumulator {
    fn new() -> Self {
        Self {
            login_count: 0,
            game_count: 0,
            unique_games: HashSet::new(),
            error_count: 0,
            popup_count: 0,
            mobile_events: 0,
            total_events: 0,
            session_count: 0,
            session_timestamps: Vec::new(),
            vip_group: None,
            currency: None,
        }
    }
}

/// Helper to extract a text field from a document's fields map.
fn get_field<'a>(doc: &'a Document, key: &str) -> Option<&'a str> {
    doc.fields.get(key).and_then(FieldValue::as_str)
}

impl MemberFeatures {
    /// Create a new empty feature accumulator.
    pub fn new() -> Self {
        Self {
            features: HashMap::new(),
            member_keys: HashMap::new(),
        }
    }

    /// Update feature accumulators from a single document.
    ///
    /// Extracts `memberCode` from the document fields. If absent, the document
    /// is silently skipped (not every event has a member).
    pub fn update(&mut self, doc: &Document) {
        let member_code = match get_field(doc, "memberCode") {
            Some(code) if !code.is_empty() => code,
            _ => return,
        };

        // Derive a stable NodeId from the member code string.
        let member_id = member_code_to_node_id(member_code);

        // Store reverse mapping so we can resolve NodeId → member code in API.
        self.member_keys
            .entry(member_id)
            .or_insert_with(|| member_code.to_owned());

        let acc = self
            .features
            .entry(member_id)
            .or_insert_with(FeatureAccumulator::new);

        acc.total_events += 1;

        // Classify by event_type.
        let event = doc.event_type.as_str();
        match event {
            e if e.contains("login") || e.contains("Login") => {
                acc.login_count += 1;
                acc.session_count += 1;
                acc.session_timestamps.push(doc.timestamp);
            }
            e if e.contains("game") || e.contains("Game") => {
                acc.game_count += 1;
                if let Some(game_name) = get_field(doc, "gameName") {
                    acc.unique_games.insert(game_name.to_owned());
                }
            }
            e if e.contains("error") || e.contains("Error") => {
                acc.error_count += 1;
            }
            e if e.contains("popup") || e.contains("Popup") => {
                acc.popup_count += 1;
            }
            _ => {}
        }

        // Platform detection.
        if let Some(platform) = get_field(doc, "platform") {
            let lower = platform.to_lowercase();
            if lower.contains("mobile") || lower.contains("android") || lower.contains("ios") {
                acc.mobile_events += 1;
            }
        }

        // VIP group and currency (take latest seen).
        if let Some(vip) = get_field(doc, "vipGroup") {
            acc.vip_group = Some(vip.to_owned());
        }
        if let Some(currency) = get_field(doc, "currency") {
            acc.currency = Some(currency.to_owned());
        }
    }

    /// Produce the 10-dimensional feature vector for a member.
    ///
    /// Returns `None` if the member has not been observed.
    pub fn to_feature_vector(&self, member_id: &NodeId) -> Option<Vec<f64>> {
        let acc = self.features.get(member_id)?;

        let platform_mobile_ratio = if acc.total_events > 0 {
            acc.mobile_events as f64 / acc.total_events as f64
        } else {
            0.0
        };

        let avg_session_gap_hours = compute_avg_session_gap(&acc.session_timestamps);

        let vip_group_numeric = acc
            .vip_group
            .as_deref()
            .map(encode_vip_group)
            .unwrap_or(0.0);

        let currency_encoded = acc
            .currency
            .as_deref()
            .map(encode_currency)
            .unwrap_or(0.0);

        Some(vec![
            acc.login_count as f64,
            acc.game_count as f64,
            acc.unique_games.len() as f64,
            acc.error_count as f64,
            acc.popup_count as f64,
            platform_mobile_ratio,
            acc.session_count as f64,
            avg_session_gap_hours,
            vip_group_numeric,
            currency_encoded,
        ])
    }

    /// Iterate over all tracked member IDs.
    pub fn members(&self) -> impl Iterator<Item = &NodeId> {
        self.features.keys()
    }

    /// Look up the original member code string for a pipeline NodeId.
    pub fn member_key(&self, node_id: &NodeId) -> Option<&str> {
        self.member_keys.get(node_id).map(|s| s.as_str())
    }
}

/// Compute the average gap (in hours) between sorted session timestamps.
fn compute_avg_session_gap(timestamps: &[DateTime<Utc>]) -> f64 {
    if timestamps.len() < 2 {
        return 0.0;
    }

    let mut sorted = timestamps.to_vec();
    sorted.sort();

    let total_gap: f64 = sorted
        .windows(2)
        .map(|w| (w[1] - w[0]).num_seconds().unsigned_abs() as f64 / 3600.0)
        .sum();

    total_gap / (sorted.len() - 1) as f64
}

/// Encode VIP group string to a numeric value.
/// Uses a simple hash-based encoding for unknown groups.
fn encode_vip_group(group: &str) -> f64 {
    match group.to_lowercase().as_str() {
        "bronze" => 1.0,
        "silver" => 2.0,
        "gold" => 3.0,
        "platinum" => 4.0,
        "diamond" => 5.0,
        "vip" => 6.0,
        _ => {
            // Stable numeric encoding for unknown groups.
            let hash: u32 = group.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
            (hash % 100) as f64 / 100.0
        }
    }
}

/// Encode currency string to a numeric value.
fn encode_currency(currency: &str) -> f64 {
    match currency.to_uppercase().as_str() {
        "USD" => 1.0,
        "EUR" => 2.0,
        "GBP" => 3.0,
        "CNY" | "RMB" => 4.0,
        "JPY" => 5.0,
        "KRW" => 6.0,
        "THB" => 7.0,
        "VND" => 8.0,
        "IDR" => 9.0,
        "MYR" => 10.0,
        _ => {
            let hash: u32 = currency.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
            (hash % 100) as f64 / 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_doc(event_type: &str, fields: Vec<(&str, &str)>) -> Document {
        let mut field_map = HashMap::new();
        for (k, v) in fields {
            field_map.insert(k.to_owned(), FieldValue::Text(v.to_owned()));
        }
        Document {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: event_type.to_owned(),
            fields: field_map,
        }
    }

    #[test]
    fn update_and_extract_features() {
        let mut mf = MemberFeatures::new();

        let doc = make_doc("login", vec![("memberCode", "M001")]);
        mf.update(&doc);

        let doc2 = make_doc("gameOpen", vec![
            ("memberCode", "M001"),
            ("gameName", "slots"),
        ]);
        mf.update(&doc2);

        assert_eq!(mf.members().count(), 1);

        let member_id = member_code_to_node_id("M001");
        let vec = mf.to_feature_vector(&member_id).unwrap();
        assert_eq!(vec.len(), 10);
        assert_eq!(vec[0], 1.0); // login_count
        assert_eq!(vec[1], 1.0); // game_count
        assert_eq!(vec[2], 1.0); // unique_games
    }

    #[test]
    fn skips_docs_without_member() {
        let mut mf = MemberFeatures::new();
        let doc = make_doc("login", vec![]);
        mf.update(&doc);
        assert_eq!(mf.members().count(), 0);
    }

    #[test]
    fn unknown_member_returns_none() {
        let mf = MemberFeatures::new();
        let fake_id = Uuid::new_v4();
        assert!(mf.to_feature_vector(&fake_id).is_none());
    }
}
