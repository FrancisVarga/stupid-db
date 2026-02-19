//! Built-in detection template evaluators.
//!
//! Templates: spike, drift, absence, threshold.
//! Each evaluator takes typed parameters plus a map of entity data,
//! returning a `Vec<RuleMatch>` of entities that triggered.

mod evaluators;
mod features;
mod math;
mod types;

pub use evaluators::*;
pub use features::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DetectionTemplate, ThresholdOperator};
    use std::collections::HashMap;

    use crate::schema::{AbsenceParams, DriftParams, SpikeParams, ThresholdParams};

    /// Build a test entity with the given feature values.
    fn make_entity(key: &str, features: Vec<f64>, cluster_id: Option<usize>) -> EntityData {
        EntityData {
            key: key.to_string(),
            entity_type: "Member".to_string(),
            features,
            score: 0.5,
            cluster_id,
        }
    }

    /// Default 10-element feature vector with all zeros.
    fn zero_features() -> Vec<f64> {
        vec![0.0; FEATURE_COUNT]
    }

    #[test]
    fn feature_index_known_names() {
        assert_eq!(feature_index("login_count"), Some(0));
        assert_eq!(feature_index("game_count"), Some(1));
        assert_eq!(feature_index("unique_games"), Some(2));
        assert_eq!(feature_index("error_count"), Some(3));
        assert_eq!(feature_index("popup_count"), Some(4));
        assert_eq!(feature_index("platform_mobile_ratio"), Some(5));
        assert_eq!(feature_index("session_count"), Some(6));
        assert_eq!(feature_index("avg_session_gap_hours"), Some(7));
        assert_eq!(feature_index("vip_group"), Some(8));
        assert_eq!(feature_index("currency"), Some(9));
        assert_eq!(feature_index("nonexistent"), None);
    }

    #[test]
    fn spike_detection_with_known_data() {
        let mut entities = HashMap::new();

        // Normal entity: login_count=10, game_count=5
        let mut normal = zero_features();
        normal[0] = 10.0; // login_count
        normal[1] = 5.0; // game_count
        entities.insert("e1".to_string(), make_entity("M001", normal, Some(0)));

        // Spike entity: login_count=100, game_count=5
        let mut spike = zero_features();
        spike[0] = 100.0;
        spike[1] = 5.0;
        entities.insert("e2".to_string(), make_entity("M002", spike, Some(0)));

        let mut cluster_stats = HashMap::new();
        let mut centroid = zero_features();
        centroid[0] = 10.0; // cluster mean login_count = 10
        cluster_stats.insert(
            0,
            ClusterStats {
                centroid,
                member_count: 2,
            },
        );

        let params = SpikeParams {
            feature: "login_count".to_string(),
            multiplier: 3.0,
            baseline: Some("cluster_centroid".to_string()),
            min_samples: Some(5),
        };

        let results = evaluate_spike(&params, &entities, &cluster_stats);

        // Only entity e2 should match: 100 > 10 * 3 = 30
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_key, "M002");
        assert_eq!(results[0].score, 100.0);
        assert!(results[0].matched_reason.contains("login_count"));
    }

    #[test]
    fn spike_skips_low_sample_entities() {
        let mut entities = HashMap::new();

        // Entity with login_count=0, game_count=2 => samples = 2
        let mut feat = zero_features();
        feat[0] = 0.0;
        feat[1] = 2.0;
        feat[3] = 50.0; // error_count is high
        entities.insert("e1".to_string(), make_entity("M001", feat, None));

        let params = SpikeParams {
            feature: "error_count".to_string(),
            multiplier: 2.0,
            baseline: Some("global_mean".to_string()),
            min_samples: Some(5),
        };

        let results = evaluate_spike(&params, &entities, &HashMap::new());
        assert_eq!(results.len(), 0, "Entity with 2 samples should be skipped");
    }

    #[test]
    fn threshold_all_six_operators() {
        let mut entities = HashMap::new();
        let mut feat = zero_features();
        feat[0] = 10.0; // login_count = 10
        feat[1] = 5.0;
        entities.insert("e1".to_string(), make_entity("M001", feat, None));

        let cases = vec![
            (ThresholdOperator::Gt, 9.0, true),
            (ThresholdOperator::Gt, 10.0, false),
            (ThresholdOperator::Gte, 10.0, true),
            (ThresholdOperator::Gte, 11.0, false),
            (ThresholdOperator::Lt, 11.0, true),
            (ThresholdOperator::Lt, 10.0, false),
            (ThresholdOperator::Lte, 10.0, true),
            (ThresholdOperator::Lte, 9.0, false),
            (ThresholdOperator::Eq, 10.0, true),
            (ThresholdOperator::Eq, 10.1, false),
            (ThresholdOperator::Neq, 10.1, true),
            (ThresholdOperator::Neq, 10.0, false),
        ];

        for (op, value, expected) in cases {
            let params = ThresholdParams {
                feature: "login_count".to_string(),
                operator: op.clone(),
                value,
            };
            let results = evaluate_threshold(&params, &entities);
            assert_eq!(
                !results.is_empty(),
                expected,
                "login_count=10 {:?} {}: expected match={}",
                op,
                value,
                expected
            );
        }
    }

    #[test]
    fn drift_cosine_distance() {
        let mut entities = HashMap::new();

        // Entity close to baseline
        let mut close = zero_features();
        close[0] = 10.0;
        close[1] = 10.0;
        entities.insert("e1".to_string(), make_entity("M001", close, None));

        // Entity far from baseline (orthogonal-ish)
        let mut far = zero_features();
        far[0] = 0.0;
        far[1] = 100.0;
        entities.insert("e2".to_string(), make_entity("M002", far, None));

        let params = DriftParams {
            features: vec!["login_count".to_string(), "game_count".to_string()],
            method: Some("cosine".to_string()),
            threshold: 0.05, // very tight threshold
            window: None,
            baseline_window: None,
        };

        let results = evaluate_drift(&params, &entities);

        // The mean is (5, 55). e1=(10,10) vs mean=(5,55) has large cosine distance.
        // e2=(0,100) vs mean=(5,55) has smaller cosine distance.
        // At least one should exceed 0.05.
        assert!(
            !results.is_empty(),
            "At least one entity should show cosine drift"
        );
    }

    #[test]
    fn drift_euclidean_distance() {
        let mut entities = HashMap::new();

        let mut feat = zero_features();
        feat[0] = 100.0;
        feat[1] = 0.0;
        entities.insert("e1".to_string(), make_entity("M001", feat, None));

        let mut feat2 = zero_features();
        feat2[0] = 0.0;
        feat2[1] = 0.0;
        entities.insert("e2".to_string(), make_entity("M002", feat2, None));

        let params = DriftParams {
            features: vec!["login_count".to_string()],
            method: Some("euclidean".to_string()),
            threshold: 40.0,
            window: None,
            baseline_window: None,
        };

        let results = evaluate_drift(&params, &entities);
        // Mean login_count = 50. e1 is at 100 (dist=50 > 40), e2 is at 0 (dist=50 > 40).
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn absence_previously_active_entity() {
        let mut entities = HashMap::new();

        // Previously active entity whose error_count dropped to 0
        let mut feat = zero_features();
        feat[0] = 20.0; // login_count (shows activity)
        feat[1] = 10.0; // game_count
        feat[3] = 0.0; // error_count dropped to 0
        feat[6] = 5.0; // session_count
        entities.insert("e1".to_string(), make_entity("M001", feat, None));

        // Entity that was never active (all zeros, score 0)
        let inactive = EntityData {
            key: "M002".to_string(),
            entity_type: "Member".to_string(),
            features: zero_features(),
            score: 0.0,
            cluster_id: None,
        };
        // Ensure login + game + session = 0, score = 0
        entities.insert("e2".to_string(), inactive);

        let params = AbsenceParams {
            feature: "error_count".to_string(),
            threshold: 1.0,
            lookback_days: 7,
            compare_to: None,
        };

        let results = evaluate_absence(&params, &entities);

        // Only e1 should match (was active, error_count=0 <= 1.0)
        // e2 should NOT match (was never active)
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_key, "M001");
        assert!(results[0].matched_reason.contains("previously active"));
    }

    #[test]
    fn absence_does_not_match_always_zero() {
        let mut entities = HashMap::new();

        let data = EntityData {
            key: "M_INACTIVE".to_string(),
            entity_type: "Member".to_string(),
            features: zero_features(),
            score: 0.0,
            cluster_id: None,
        };
        entities.insert("e1".to_string(), data);

        let params = AbsenceParams {
            feature: "login_count".to_string(),
            threshold: 1.0,
            lookback_days: 7,
            compare_to: None,
        };

        let results = evaluate_absence(&params, &entities);
        assert!(
            results.is_empty(),
            "Entity with no prior activity should not trigger absence"
        );
    }

    #[test]
    fn evaluate_template_dispatcher() {
        let mut entities = HashMap::new();
        let mut feat = zero_features();
        feat[0] = 50.0;
        feat[1] = 10.0;
        entities.insert("e1".to_string(), make_entity("M001", feat, None));

        let params_yaml = serde_yaml::to_value(&ThresholdParams {
            feature: "login_count".to_string(),
            operator: ThresholdOperator::Gte,
            value: 25.0,
        })
        .unwrap();

        let results = evaluate_template(
            &DetectionTemplate::Threshold,
            &params_yaml,
            &entities,
            &HashMap::new(),
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_key, "M001");
    }

    #[test]
    fn evaluate_template_bad_params() {
        let entities = HashMap::new();
        let bad_yaml = serde_yaml::Value::String("not a struct".to_string());

        let result = evaluate_template(
            &DetectionTemplate::Spike,
            &bad_yaml,
            &entities,
            &HashMap::new(),
        );

        assert!(result.is_err(), "Bad params should return Err");
    }

    #[test]
    fn unknown_feature_returns_empty() {
        let mut entities = HashMap::new();
        entities.insert(
            "e1".to_string(),
            make_entity("M001", zero_features(), None),
        );

        let params = ThresholdParams {
            feature: "nonexistent_feature".to_string(),
            operator: ThresholdOperator::Gt,
            value: 0.0,
        };

        let results = evaluate_threshold(&params, &entities);
        assert!(results.is_empty());
    }

    #[test]
    fn cosine_distance_identical_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let d = math::cosine_distance(&a, &a);
        assert!(d.abs() < 1e-10, "Distance to self should be ~0");
    }

    #[test]
    fn cosine_distance_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let d = math::cosine_distance(&a, &b);
        assert!((d - 1.0).abs() < 1e-10, "Orthogonal vectors should have distance ~1.0");
    }
}
