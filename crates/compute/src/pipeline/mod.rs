//! Compute pipeline orchestrator.
//!
//! Wires together feature extraction, anomaly scoring, co-occurrence updates,
//! and metrics into a multi-stage pipeline:
//!
//! - **Stage 2 (hot)**: Per-document feature updates and streaming K-means.
//! - **Stage 3 (warm)**: Co-occurrence, anomaly scoring, insight generation.

pub mod anomaly;
pub mod cooccurrence;
pub mod features;
pub mod metrics;
pub mod trend;

use std::time::Instant;

use chrono::Utc;
use tracing::{debug, info};
use uuid::Uuid;

use stupid_core::Document;

use crate::algorithms::streaming_kmeans::StreamingKMeans;
use crate::scheduler::state::KnowledgeState;
use crate::scheduler::types::{ClusterInfo, Insight, InsightSeverity};

use crate::algorithms::prefixspan;

use self::anomaly::score_all_members;
use self::cooccurrence::update_cooccurrence;
use self::features::{member_code_to_node_id, MemberFeatures};
use self::metrics::PipelineMetrics;
use self::trend::TrendDetector;

/// Default number of clusters for streaming K-means.
const DEFAULT_K: usize = 8;

/// Feature vector dimensionality (10-dimensional member features).
const FEATURE_DIM: usize = 10;

/// Main pipeline orchestrator combining all compute stages.
pub struct Pipeline {
    /// Accumulated member feature vectors.
    pub features: MemberFeatures,
    /// Pipeline performance metrics.
    pub metrics: PipelineMetrics,
    /// Streaming K-means instance for online clustering.
    kmeans: StreamingKMeans,
    /// Trend detector for z-score based trend detection.
    trend_detector: TrendDetector,
}

impl Pipeline {
    /// Create a new pipeline with default configuration.
    pub fn new() -> Self {
        Self {
            features: MemberFeatures::new(),
            metrics: PipelineMetrics::default(),
            kmeans: StreamingKMeans::new(DEFAULT_K, FEATURE_DIM),
            trend_detector: TrendDetector::new(),
        }
    }

    /// Create a new pipeline with a custom number of clusters.
    pub fn with_k(k: usize) -> Self {
        Self {
            features: MemberFeatures::new(),
            metrics: PipelineMetrics::default(),
            kmeans: StreamingKMeans::new(k, FEATURE_DIM),
            trend_detector: TrendDetector::new(),
        }
    }

    /// Stage 2 hot path: process incoming documents in real time.
    ///
    /// For each document:
    /// 1. Update member feature accumulators.
    /// 2. Feed updated feature vectors into streaming K-means.
    /// 3. Sync cluster assignments and centroids to `KnowledgeState`.
    /// 4. Record throughput metrics.
    pub fn hot_connect(&mut self, docs: &[Document], state: &mut KnowledgeState) {
        if docs.is_empty() {
            return;
        }

        let start = Instant::now();

        for doc in docs {
            self.features.update(doc);
        }

        // Update K-means for any members whose features changed.
        // We re-extract the member IDs from the docs to know which to update.
        let mut updated_members = std::collections::HashSet::new();
        for doc in docs {
            if let Some(member_code) = doc
                .fields
                .get("memberCode")
                .and_then(|fv| fv.as_str())
                .filter(|s| !s.is_empty())
            {
                let member_id = member_code_to_node_id(member_code);
                updated_members.insert(member_id);
            }
        }

        for member_id in &updated_members {
            if let Some(fv) = self.features.to_feature_vector(member_id) {
                self.kmeans.update(*member_id, fv);
            }
        }

        // Sync cluster state.
        for member_id in &updated_members {
            if let Some(cluster_id) = self.kmeans.get_cluster(member_id) {
                state.clusters.insert(*member_id, cluster_id);
            }
        }

        // Update cluster info with current centroids.
        for (i, centroid) in self.kmeans.centroids().iter().enumerate() {
            let cluster_id = i as u64;
            let counts = self.kmeans.cluster_counts();
            state.cluster_info.insert(
                cluster_id,
                ClusterInfo {
                    id: cluster_id,
                    centroid: centroid.clone(),
                    member_count: counts.get(i).copied().unwrap_or(0),
                    label: None,
                },
            );
        }

        let elapsed = start.elapsed();
        self.metrics.record_hot_batch(docs.len() as u64, elapsed);

        debug!(
            docs = docs.len(),
            members = updated_members.len(),
            elapsed_us = elapsed.as_micros(),
            "hot_connect completed"
        );
    }

    /// Stage 3 warm compute: periodic analysis on recent data.
    ///
    /// 1. Update co-occurrence matrices from recent documents.
    /// 2. Run anomaly scoring on all tracked members.
    /// 3. Push anomaly insights into the insight queue.
    /// 4. Record warm compute metrics.
    pub fn warm_compute(&mut self, state: &mut KnowledgeState, recent_docs: &[Document]) {
        let start = Instant::now();

        // Step 1: Co-occurrence with PMI scoring.
        cooccurrence::update_cooccurrence_with_pmi(&mut state.cooccurrence_pmi, recent_docs);
        // Also update raw counts for backward compatibility.
        update_cooccurrence(&mut state.cooccurrence, recent_docs);
        // Compute PMI scores for all matrices.
        for matrix in state.cooccurrence_pmi.values_mut() {
            cooccurrence::compute_pmi(matrix);
        }

        // Step 2: Anomaly scoring.
        let anomaly_results = score_all_members(&self.features, &self.kmeans);

        let mut anomaly_count = 0usize;
        for (member_id, score) in &anomaly_results {
            state.anomalies.insert(*member_id, *score);
            if score.is_anomalous {
                anomaly_count += 1;
            }
        }

        // Step 3: Generate insights for anomalous members.
        if anomaly_count > 0 {
            info!(anomaly_count, "anomalous members detected");

            for (member_id, score) in &anomaly_results {
                if score.is_anomalous {
                    let severity = if score.score > 4.0 {
                        InsightSeverity::Critical
                    } else if score.score > 3.0 {
                        InsightSeverity::Warning
                    } else {
                        InsightSeverity::Info
                    };

                    let insight = Insight {
                        id: Uuid::new_v4().to_string(),
                        title: format!("Anomalous behavior detected (z={:.2})", score.score),
                        description: format!(
                            "Member {} has anomaly score {:.2}, exceeding threshold",
                            member_id, score.score
                        ),
                        severity,
                        created_at: Utc::now(),
                        related_nodes: vec![*member_id],
                    };

                    state.insights.push_back(insight);
                }
            }

            // Cap insight queue to prevent unbounded growth.
            const MAX_INSIGHTS: usize = 10_000;
            while state.insights.len() > MAX_INSIGHTS {
                state.insights.pop_front();
            }
        }

        // Step 4: Trend detection.
        let current_metrics = TrendDetector::extract_metrics(recent_docs);
        let detected_trends = self.trend_detector.detect(&current_metrics);

        for t in &detected_trends {
            // Update trends in state.
            state.trends.insert(
                t.metric.clone(),
                crate::scheduler::types::Trend {
                    metric_name: t.metric.clone(),
                    direction: match t.direction {
                        trend::TrendDirection::Up => crate::scheduler::types::TrendDirection::Rising,
                        trend::TrendDirection::Down => crate::scheduler::types::TrendDirection::Falling,
                        trend::TrendDirection::Stable => crate::scheduler::types::TrendDirection::Stable,
                    },
                    magnitude: t.z_score.abs(),
                    baseline: t.baseline_mean,
                    current: t.current_value,
                },
            );

            // Generate insights for significant trends.
            if t.z_score.abs() > 3.0 {
                let severity = if t.z_score.abs() > 4.0 {
                    InsightSeverity::Critical
                } else {
                    InsightSeverity::Warning
                };

                let direction_str = match t.direction {
                    trend::TrendDirection::Up => "increased",
                    trend::TrendDirection::Down => "decreased",
                    trend::TrendDirection::Stable => "changed",
                };

                let insight = Insight {
                    id: Uuid::new_v4().to_string(),
                    title: format!("{} {} significantly (z={:.2})", t.metric, direction_str, t.z_score),
                    description: format!(
                        "{}: current={:.1}, baseline={:.1} +/- {:.1}",
                        t.metric, t.current_value, t.baseline_mean, t.baseline_stddev
                    ),
                    severity,
                    created_at: Utc::now(),
                    related_nodes: vec![],
                };

                state.insights.push_back(insight);
            }
        }

        // Step 5: PrefixSpan pattern mining.
        let sequences = prefixspan::build_sequences(recent_docs);
        let config = prefixspan::PrefixSpanConfig::default();
        let patterns = prefixspan::prefixspan(&sequences, &config);
        let pattern_count = patterns.len();
        state.prefixspan_patterns = patterns;

        let elapsed = start.elapsed();
        self.metrics
            .record_warm_run(recent_docs.len() as u64, elapsed);

        info!(
            docs = recent_docs.len(),
            anomalies = anomaly_count,
            trends = detected_trends.len(),
            patterns = pattern_count,
            elapsed_ms = elapsed.as_millis(),
            "warm_compute completed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use stupid_core::FieldValue;

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
    fn pipeline_hot_connect_basic() {
        let mut pipeline = Pipeline::new();
        let mut state = KnowledgeState::default();

        let docs = vec![
            make_doc("login", vec![("memberCode", "M001")]),
            make_doc("gameOpen", vec![
                ("memberCode", "M001"),
                ("gameName", "slots"),
            ]),
            make_doc("login", vec![("memberCode", "M002")]),
        ];

        pipeline.hot_connect(&docs, &mut state);

        // Should have cluster assignments for both members.
        assert!(!state.clusters.is_empty());
        assert!(pipeline.metrics.hot_docs_per_second > 0.0);
    }

    #[test]
    fn pipeline_warm_compute_basic() {
        let mut pipeline = Pipeline::with_k(2);
        let mut state = KnowledgeState::default();

        // Feed some data through hot path first.
        let docs: Vec<Document> = (0..20)
            .map(|i| {
                make_doc("login", vec![
                    ("memberCode", &format!("M{:03}", i % 5)),
                    ("deviceId", &format!("D{:03}", i % 3)),
                ])
            })
            .collect();

        pipeline.hot_connect(&docs, &mut state);

        // Now run warm compute.
        pipeline.warm_compute(&mut state, &docs);

        assert!(pipeline.metrics.warm_last_run.is_some());
        assert!(!state.cooccurrence.is_empty());
    }

    #[test]
    fn pipeline_empty_docs_noop() {
        let mut pipeline = Pipeline::new();
        let mut state = KnowledgeState::default();

        pipeline.hot_connect(&[], &mut state);
        assert!(state.clusters.is_empty());
        assert_eq!(pipeline.metrics.hot_docs_per_second, 0.0);
    }
}
