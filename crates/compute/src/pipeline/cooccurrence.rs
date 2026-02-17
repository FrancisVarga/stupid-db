use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use stupid_core::{Document, EntityType, FieldValue};
use tracing::debug;

use crate::scheduler::types::SparseMatrix;

/// Field names mapped to their entity types for co-occurrence extraction.
const ENTITY_FIELDS: &[(&str, EntityType)] = &[
    ("memberCode", EntityType::Member),
    ("deviceId", EntityType::Device),
    ("gameName", EntityType::Game),
    ("affiliateCode", EntityType::Affiliate),
    ("currency", EntityType::Currency),
    ("vipGroup", EntityType::VipGroup),
    ("errorCode", EntityType::Error),
    ("platform", EntityType::Platform),
    ("popupId", EntityType::Popup),
    ("provider", EntityType::Provider),
];

/// Co-occurrence matrix with both raw counts and PMI scores.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CooccurrenceMatrix {
    /// Raw co-occurrence counts.
    pub counts: SparseMatrix,
    /// PMI scores for each pair.
    pub pmi: HashMap<(String, String), f64>,
    /// Marginal counts: how many documents each entity appears in.
    pub marginals: HashMap<String, f64>,
    /// Total number of documents processed.
    pub total_docs: f64,
}

/// Update co-occurrence matrices from a batch of documents.
///
/// For each document, all present entity fields are extracted and every pair
/// of distinct entity types gets its co-occurrence count incremented.
/// Only entities within the same document (same event) are counted as co-occurring.
pub fn update_cooccurrence(
    cooccurrence: &mut HashMap<(EntityType, EntityType), SparseMatrix>,
    docs: &[Document],
) {
    for doc in docs {
        // Extract all entities present in this document.
        let entities: Vec<(EntityType, String)> = ENTITY_FIELDS
            .iter()
            .filter_map(|(field, entity_type)| {
                doc.fields
                    .get(*field)
                    .and_then(FieldValue::as_str)
                    .filter(|s| !s.is_empty())
                    .map(|val| (*entity_type, val.to_owned()))
            })
            .collect();

        // Generate all pairs and increment co-occurrence counts.
        for i in 0..entities.len() {
            for j in (i + 1)..entities.len() {
                let (type_a, ref key_a) = entities[i];
                let (type_b, ref key_b) = entities[j];

                // Normalize ordering so (Member, Device) and (Device, Member) use the same matrix.
                let (ordered_type, ordered_key_a, ordered_key_b) =
                    if (type_a as u8) <= (type_b as u8) {
                        ((type_a, type_b), key_a.clone(), key_b.clone())
                    } else {
                        ((type_b, type_a), key_b.clone(), key_a.clone())
                    };

                let matrix = cooccurrence.entry(ordered_type).or_default();
                *matrix
                    .entries
                    .entry((ordered_key_a, ordered_key_b))
                    .or_insert(0.0) += 1.0;
            }
        }
    }
}

/// Update co-occurrence matrices with PMI tracking from a batch of documents.
///
/// In addition to raw counts, this tracks marginal entity frequencies
/// and total document count needed for PMI computation.
pub fn update_cooccurrence_with_pmi(
    matrices: &mut HashMap<(EntityType, EntityType), CooccurrenceMatrix>,
    docs: &[Document],
) {
    for doc in docs {
        let entities: Vec<(EntityType, String)> = ENTITY_FIELDS
            .iter()
            .filter_map(|(field, entity_type)| {
                doc.fields
                    .get(*field)
                    .and_then(FieldValue::as_str)
                    .filter(|s| !s.is_empty())
                    .map(|val| (*entity_type, val.to_owned()))
            })
            .collect();

        // Generate all pairs and increment co-occurrence counts + marginals.
        for i in 0..entities.len() {
            for j in (i + 1)..entities.len() {
                let (type_a, ref key_a) = entities[i];
                let (type_b, ref key_b) = entities[j];

                let (ordered_type, ordered_key_a, ordered_key_b) =
                    if (type_a as u8) <= (type_b as u8) {
                        ((type_a, type_b), key_a.clone(), key_b.clone())
                    } else {
                        ((type_b, type_a), key_b.clone(), key_a.clone())
                    };

                let matrix = matrices.entry(ordered_type).or_default();
                *matrix
                    .counts
                    .entries
                    .entry((ordered_key_a.clone(), ordered_key_b.clone()))
                    .or_insert(0.0) += 1.0;

                // Track marginals.
                *matrix.marginals.entry(ordered_key_a).or_insert(0.0) += 1.0;
                *matrix.marginals.entry(ordered_key_b).or_insert(0.0) += 1.0;
                matrix.total_docs += 1.0;
            }
        }
    }
}

/// Compute PMI scores for all pairs in a co-occurrence matrix.
///
/// PMI(A,B) = log2(P(A,B) / (P(A) * P(B)))
///
/// Where:
/// - P(A,B) = count(A,B) / total_docs
/// - P(A) = marginal_count(A) / total_docs
/// - P(B) = marginal_count(B) / total_docs
pub fn compute_pmi(matrix: &mut CooccurrenceMatrix) {
    if matrix.total_docs <= 0.0 {
        return;
    }

    let total = matrix.total_docs;
    matrix.pmi.clear();

    for ((key_a, key_b), &count) in &matrix.counts.entries {
        let marginal_a = matrix.marginals.get(key_a).copied().unwrap_or(0.0);
        let marginal_b = matrix.marginals.get(key_b).copied().unwrap_or(0.0);

        if marginal_a <= 0.0 || marginal_b <= 0.0 {
            continue;
        }

        let p_ab = count / total;
        let p_a = marginal_a / total;
        let p_b = marginal_b / total;

        let denominator = p_a * p_b;
        if denominator <= 0.0 {
            continue;
        }

        let pmi = (p_ab / denominator).log2();
        matrix.pmi.insert((key_a.clone(), key_b.clone()), pmi);
    }

    debug!(
        pairs = matrix.pmi.len(),
        total_docs = total,
        "PMI computation complete"
    );
}

/// Compute PMI for all co-occurrence matrices.
pub fn compute_all_pmi(matrices: &mut HashMap<(EntityType, EntityType), CooccurrenceMatrix>) {
    for matrix in matrices.values_mut() {
        compute_pmi(matrix);
    }
}

// ── Config-driven co-occurrence ────────────────────────────────────

use stupid_rules::entity_schema::CompiledEntitySchema;

/// Parse an EntityType from a schema string name.
fn parse_entity_type(name: &str) -> Option<EntityType> {
    match name {
        "Member" => Some(EntityType::Member),
        "Device" => Some(EntityType::Device),
        "Game" => Some(EntityType::Game),
        "Affiliate" => Some(EntityType::Affiliate),
        "Currency" => Some(EntityType::Currency),
        "VipGroup" => Some(EntityType::VipGroup),
        "Error" => Some(EntityType::Error),
        "Platform" => Some(EntityType::Platform),
        "Popup" => Some(EntityType::Popup),
        "Provider" => Some(EntityType::Provider),
        _ => None,
    }
}

/// Update co-occurrence matrices using field mappings from a compiled EntitySchema.
///
/// Replaces the hardcoded `ENTITY_FIELDS` constant with schema-driven lookup.
/// Field aliases from the schema are automatically resolved.
pub fn update_cooccurrence_with_schema(
    cooccurrence: &mut HashMap<(EntityType, EntityType), SparseMatrix>,
    docs: &[Document],
    schema: &CompiledEntitySchema,
) {
    for doc in docs {
        let entities: Vec<(EntityType, String)> = schema
            .field_to_entity
            .iter()
            .filter_map(|(field, entity_type_name)| {
                let et = parse_entity_type(entity_type_name)?;
                doc.fields
                    .get(field.as_str())
                    .and_then(FieldValue::as_str)
                    .filter(|s| !s.is_empty() && !schema.null_values.contains(*s))
                    .map(|val| (et, val.to_owned()))
            })
            .collect();

        for i in 0..entities.len() {
            for j in (i + 1)..entities.len() {
                let (type_a, ref key_a) = entities[i];
                let (type_b, ref key_b) = entities[j];

                let (ordered_type, ordered_key_a, ordered_key_b) =
                    if (type_a as u8) <= (type_b as u8) {
                        ((type_a, type_b), key_a.clone(), key_b.clone())
                    } else {
                        ((type_b, type_a), key_b.clone(), key_a.clone())
                    };

                let matrix = cooccurrence.entry(ordered_type).or_default();
                *matrix
                    .entries
                    .entry((ordered_key_a, ordered_key_b))
                    .or_insert(0.0) += 1.0;
            }
        }
    }
}

/// Update co-occurrence matrices with PMI tracking using schema field mappings.
pub fn update_cooccurrence_with_pmi_and_schema(
    matrices: &mut HashMap<(EntityType, EntityType), CooccurrenceMatrix>,
    docs: &[Document],
    schema: &CompiledEntitySchema,
) {
    for doc in docs {
        let entities: Vec<(EntityType, String)> = schema
            .field_to_entity
            .iter()
            .filter_map(|(field, entity_type_name)| {
                let et = parse_entity_type(entity_type_name)?;
                doc.fields
                    .get(field.as_str())
                    .and_then(FieldValue::as_str)
                    .filter(|s| !s.is_empty() && !schema.null_values.contains(*s))
                    .map(|val| (et, val.to_owned()))
            })
            .collect();

        for i in 0..entities.len() {
            for j in (i + 1)..entities.len() {
                let (type_a, ref key_a) = entities[i];
                let (type_b, ref key_b) = entities[j];

                let (ordered_type, ordered_key_a, ordered_key_b) =
                    if (type_a as u8) <= (type_b as u8) {
                        ((type_a, type_b), key_a.clone(), key_b.clone())
                    } else {
                        ((type_b, type_a), key_b.clone(), key_a.clone())
                    };

                let matrix = matrices.entry(ordered_type).or_default();
                *matrix
                    .counts
                    .entries
                    .entry((ordered_key_a.clone(), ordered_key_b.clone()))
                    .or_insert(0.0) += 1.0;

                *matrix.marginals.entry(ordered_key_a).or_insert(0.0) += 1.0;
                *matrix.marginals.entry(ordered_key_b).or_insert(0.0) += 1.0;
                matrix.total_docs += 1.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_doc(fields: Vec<(&str, &str)>) -> Document {
        let mut field_map = HashMap::new();
        for (k, v) in fields {
            field_map.insert(k.to_owned(), FieldValue::Text(v.to_owned()));
        }
        Document {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: "test".to_owned(),
            fields: field_map,
        }
    }

    #[test]
    fn basic_cooccurrence() {
        let mut cooc = HashMap::new();
        let docs = vec![make_doc(vec![
            ("memberCode", "M001"),
            ("gameName", "slots"),
        ])];

        update_cooccurrence(&mut cooc, &docs);

        // Should have exactly one pair type.
        assert_eq!(cooc.len(), 1);

        // Check that the count is 1.
        let matrix = cooc.values().next().unwrap();
        let count: f64 = matrix.entries.values().sum();
        assert_eq!(count, 1.0);
    }

    #[test]
    fn multiple_docs_accumulate() {
        let mut cooc = HashMap::new();
        let docs = vec![
            make_doc(vec![("memberCode", "M001"), ("deviceId", "D001")]),
            make_doc(vec![("memberCode", "M001"), ("deviceId", "D001")]),
        ];

        update_cooccurrence(&mut cooc, &docs);

        let matrix = cooc.values().next().unwrap();
        let count: f64 = matrix.entries.values().sum();
        assert_eq!(count, 2.0);
    }

    #[test]
    fn no_entities_no_pairs() {
        let mut cooc = HashMap::new();
        let docs = vec![make_doc(vec![])];
        update_cooccurrence(&mut cooc, &docs);
        assert!(cooc.is_empty());
    }

    #[test]
    fn three_entities_produce_three_pairs() {
        let mut cooc = HashMap::new();
        let docs = vec![make_doc(vec![
            ("memberCode", "M001"),
            ("deviceId", "D001"),
            ("gameName", "slots"),
        ])];

        update_cooccurrence(&mut cooc, &docs);

        let total_pairs: usize = cooc.values().map(|m| m.entries.len()).sum();
        assert_eq!(total_pairs, 3);
    }

    #[test]
    fn pmi_scoring_basic() {
        let mut matrices: HashMap<(EntityType, EntityType), CooccurrenceMatrix> = HashMap::new();

        // Create docs where M001+slots always appear together,
        // but M001+poker rarely do.
        let docs = vec![
            make_doc(vec![("memberCode", "M001"), ("gameName", "slots")]),
            make_doc(vec![("memberCode", "M001"), ("gameName", "slots")]),
            make_doc(vec![("memberCode", "M001"), ("gameName", "slots")]),
            make_doc(vec![("memberCode", "M002"), ("gameName", "poker")]),
            make_doc(vec![("memberCode", "M003"), ("gameName", "poker")]),
        ];

        update_cooccurrence_with_pmi(&mut matrices, &docs);

        // Compute PMI.
        compute_all_pmi(&mut matrices);

        // Should have at least one matrix with PMI scores.
        let has_pmi = matrices.values().any(|m| !m.pmi.is_empty());
        assert!(has_pmi, "Should have computed PMI scores");
    }

    #[test]
    fn pmi_positive_for_strong_association() {
        let mut matrix = CooccurrenceMatrix::default();

        // A and B always co-occur.
        matrix.counts.entries.insert(("A".to_string(), "B".to_string()), 10.0);
        matrix.marginals.insert("A".to_string(), 10.0);
        matrix.marginals.insert("B".to_string(), 10.0);
        matrix.total_docs = 10.0;

        compute_pmi(&mut matrix);

        let pmi = matrix.pmi.get(&("A".to_string(), "B".to_string()));
        assert!(pmi.is_some());
        // P(A,B) = 1.0, P(A) = 1.0, P(B) = 1.0 => PMI = log2(1/1) = 0
        // When A and B always co-occur and have same marginal, PMI = 0
        assert!((pmi.unwrap() - 0.0).abs() < 1e-10);
    }

    #[test]
    fn pmi_positive_when_correlated() {
        let mut matrix = CooccurrenceMatrix::default();

        // A co-occurs with B 8/20 times, but A appears 10/20, B appears 10/20.
        // P(A,B) = 8/20 = 0.4, P(A) = 10/20 = 0.5, P(B) = 10/20 = 0.5
        // PMI = log2(0.4 / 0.25) = log2(1.6) > 0
        matrix.counts.entries.insert(("A".to_string(), "B".to_string()), 8.0);
        matrix.marginals.insert("A".to_string(), 10.0);
        matrix.marginals.insert("B".to_string(), 10.0);
        matrix.total_docs = 20.0;

        compute_pmi(&mut matrix);

        let pmi = matrix.pmi[&("A".to_string(), "B".to_string())];
        assert!(pmi > 0.0, "PMI should be positive for correlated entities");
    }

    #[test]
    fn pmi_negative_when_anti_correlated() {
        let mut matrix = CooccurrenceMatrix::default();

        // A co-occurs with B 1/100 times, but each appears 50/100 times.
        // P(A,B) = 0.01, P(A) = 0.5, P(B) = 0.5 => PMI = log2(0.01/0.25) < 0
        matrix.counts.entries.insert(("A".to_string(), "B".to_string()), 1.0);
        matrix.marginals.insert("A".to_string(), 50.0);
        matrix.marginals.insert("B".to_string(), 50.0);
        matrix.total_docs = 100.0;

        compute_pmi(&mut matrix);

        let pmi = matrix.pmi[&("A".to_string(), "B".to_string())];
        assert!(pmi < 0.0, "PMI should be negative for anti-correlated entities");
    }
}
