//! Feature vector index mapping.
//!
//! Maps feature names to positions in the standard 10-element feature vector,
//! with both hardcoded and config-driven lookup variants.

/// Number of elements in the standard feature vector.
pub const FEATURE_COUNT: usize = 10;

/// Feature names in the order produced by `MemberFeatures::to_feature_vector`.
pub const FEATURE_NAMES: [&str; FEATURE_COUNT] = [
    "login_count",
    "game_count",
    "unique_games",
    "error_count",
    "popup_count",
    "platform_mobile_ratio",
    "session_count",
    "avg_session_gap_hours",
    "vip_group",
    "currency",
];

/// Map a feature name to its index in the 10-element feature vector.
///
/// Returns `None` if the name is not recognized.
/// Uses the hardcoded `FEATURE_NAMES` array; for config-driven lookup,
/// use [`feature_index_from_config`].
pub fn feature_index(name: &str) -> Option<usize> {
    FEATURE_NAMES.iter().position(|&n| n == name)
}

/// Map a feature name to its index using a compiled FeatureConfig.
///
/// Prefer this over [`feature_index`] when a loaded config is available,
/// as it reflects the actual YAML-defined feature vector.
pub fn feature_index_from_config(
    name: &str,
    config: &crate::feature_config::CompiledFeatureConfig,
) -> Option<usize> {
    config.feature_index(name)
}

/// Get feature count from a compiled FeatureConfig.
pub fn feature_count_from_config(
    config: &crate::feature_config::CompiledFeatureConfig,
) -> usize {
    config.feature_count()
}

/// Get ordered feature names from a compiled FeatureConfig.
pub fn feature_names_from_config(
    config: &crate::feature_config::CompiledFeatureConfig,
) -> &[String] {
    &config.feature_names
}
