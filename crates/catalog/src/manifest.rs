use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Manifest tracking which segments are loaded in the catalog.
///
/// The `segments_hash` is a SHA-256 hex digest of the sorted, newline-joined
/// segment IDs. This allows cheap equality checks to determine whether the
/// catalog needs to be rebuilt after segments change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogManifest {
    /// Sorted list of segment IDs included in this manifest.
    pub segment_ids: Vec<String>,
    /// SHA-256 hex hash of sorted segment IDs joined by newlines.
    pub segments_hash: String,
    /// ISO 8601 timestamp of when this manifest was created.
    pub created_at: String,
    /// Schema version for forward compatibility.
    pub version: u32,
}

impl CatalogManifest {
    /// Create a new manifest from the given segment IDs.
    ///
    /// Segment IDs are sorted internally so that order does not affect the hash.
    pub fn new(segment_ids: &[String]) -> Self {
        let mut sorted = segment_ids.to_vec();
        sorted.sort();
        let segments_hash = compute_segments_hash(&sorted);

        Self {
            segment_ids: sorted,
            segments_hash,
            created_at: Utc::now().to_rfc3339(),
            version: 1,
        }
    }

    /// Check whether this manifest is still fresh given the current segment IDs.
    ///
    /// Returns `true` if the hash of `current_segment_ids` matches the stored hash.
    pub fn is_fresh(&self, current_segment_ids: &[String]) -> bool {
        let mut sorted = current_segment_ids.to_vec();
        sorted.sort();
        let current_hash = compute_segments_hash(&sorted);
        self.segments_hash == current_hash
    }
}

/// Compute a deterministic SHA-256 hex hash from a **pre-sorted** slice of segment IDs.
fn compute_segments_hash(sorted_ids: &[String]) -> String {
    let joined = sorted_ids.join("\n");
    let digest = Sha256::digest(joined.as_bytes());
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        let ids = vec!["seg-a".into(), "seg-b".into(), "seg-c".into()];
        let m1 = CatalogManifest::new(&ids);
        let m2 = CatalogManifest::new(&ids);
        assert_eq!(m1.segments_hash, m2.segments_hash);
    }

    #[test]
    fn hash_is_order_independent() {
        let ids_a = vec!["seg-c".into(), "seg-a".into(), "seg-b".into()];
        let ids_b = vec!["seg-a".into(), "seg-b".into(), "seg-c".into()];
        let m_a = CatalogManifest::new(&ids_a);
        let m_b = CatalogManifest::new(&ids_b);
        assert_eq!(m_a.segments_hash, m_b.segments_hash);
    }

    #[test]
    fn segment_ids_are_stored_sorted() {
        let ids = vec!["seg-c".into(), "seg-a".into(), "seg-b".into()];
        let m = CatalogManifest::new(&ids);
        assert_eq!(m.segment_ids, vec!["seg-a", "seg-b", "seg-c"]);
    }

    #[test]
    fn is_fresh_with_matching_ids() {
        let ids = vec!["seg-1".into(), "seg-2".into()];
        let m = CatalogManifest::new(&ids);
        assert!(m.is_fresh(&ids));
        // Different order should still be fresh
        let reversed: Vec<String> = ids.into_iter().rev().collect();
        assert!(m.is_fresh(&reversed));
    }

    #[test]
    fn is_fresh_with_different_ids() {
        let ids = vec!["seg-1".into(), "seg-2".into()];
        let m = CatalogManifest::new(&ids);
        let different = vec!["seg-1".into(), "seg-3".into()];
        assert!(!m.is_fresh(&different));
    }

    #[test]
    fn is_fresh_with_extra_segment() {
        let ids = vec!["seg-1".into(), "seg-2".into()];
        let m = CatalogManifest::new(&ids);
        let extra = vec!["seg-1".into(), "seg-2".into(), "seg-3".into()];
        assert!(!m.is_fresh(&extra));
    }

    #[test]
    fn empty_segment_list() {
        let ids: Vec<String> = vec![];
        let m = CatalogManifest::new(&ids);
        assert!(m.is_fresh(&ids));
        assert!(!m.is_fresh(&["seg-1".to_string()]));
    }

    #[test]
    fn version_is_one() {
        let m = CatalogManifest::new(&[]);
        assert_eq!(m.version, 1);
    }

    #[test]
    fn created_at_is_valid_rfc3339() {
        let m = CatalogManifest::new(&[]);
        // chrono can parse its own rfc3339 output
        chrono::DateTime::parse_from_rfc3339(&m.created_at)
            .expect("created_at should be valid RFC 3339");
    }

    #[test]
    fn serialization_roundtrip() {
        let ids = vec!["seg-x".into(), "seg-y".into()];
        let m = CatalogManifest::new(&ids);
        let json = serde_json::to_string(&m).expect("serialize");
        let m2: CatalogManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m.segments_hash, m2.segments_hash);
        assert_eq!(m.segment_ids, m2.segment_ids);
    }
}
