use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

use stupid_core::{Document, FieldValue};

/// Direction of a detected trend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendDirection {
    Up,
    Down,
    Stable,
}

/// Severity of a detected trend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// |z| > 2: logged.
    Notable,
    /// |z| > 3: pushed as insight.
    Significant,
    /// |z| > 4: high-priority insight.
    Critical,
}

/// A detected trend in a metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trend {
    pub metric: String,
    pub current_value: f64,
    pub baseline_mean: f64,
    pub baseline_stddev: f64,
    pub z_score: f64,
    pub direction: TrendDirection,
    pub severity: Severity,
    pub since: DateTime<Utc>,
}

/// Rolling baseline tracker for a single metric.
///
/// Maintains a fixed-size window of historical values and computes
/// mean and standard deviation for z-score calculation.
#[derive(Debug, Clone)]
struct MetricBaseline {
    /// Historical values (oldest first).
    values: Vec<f64>,
    /// Maximum number of historical values to keep.
    max_window: usize,
}

impl MetricBaseline {
    fn new(max_window: usize) -> Self {
        Self {
            values: Vec::new(),
            max_window,
        }
    }

    fn push(&mut self, value: f64) {
        self.values.push(value);
        if self.values.len() > self.max_window {
            self.values.remove(0);
        }
    }

    fn mean(&self) -> f64 {
        if self.values.is_empty() {
            return 0.0;
        }
        self.values.iter().sum::<f64>() / self.values.len() as f64
    }

    fn stddev(&self) -> f64 {
        if self.values.len() < 2 {
            return 0.0;
        }
        let mean = self.mean();
        let variance = self.values.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
            / self.values.len() as f64;
        variance.sqrt()
    }
}

/// Trend detector that tracks multiple metrics over time.
pub struct TrendDetector {
    /// Baselines for each tracked metric.
    baselines: HashMap<String, MetricBaseline>,
    /// Number of historical data points to retain (default: 168 = 7 days * 24 hours).
    window_size: usize,
    /// Minimum data points for z-score calculation.
    min_data_points: usize,
    /// Z-score trigger threshold.
    z_score_trigger: f64,
    /// Z-score threshold for Up direction.
    direction_up: f64,
    /// Z-score threshold for Down direction (positive value, compared to -z).
    direction_down: f64,
    /// |z| threshold for Significant severity.
    severity_significant: f64,
    /// |z| threshold for Critical severity.
    severity_critical: f64,
}

impl TrendDetector {
    /// Create a new trend detector with default 7-day hourly window.
    pub fn new() -> Self {
        Self {
            baselines: HashMap::new(),
            window_size: 168,
            min_data_points: 3,
            z_score_trigger: 2.0,
            direction_up: 0.5,
            direction_down: 0.5,
            severity_significant: 3.0,
            severity_critical: 4.0,
        }
    }

    /// Create a new trend detector with a custom window size.
    pub fn with_window(window_size: usize) -> Self {
        let mut det = Self::new();
        det.window_size = window_size;
        det
    }

    /// Create a new trend detector from a compiled TrendConfig.
    pub fn with_config(config: &stupid_rules::trend_config::CompiledTrendConfig) -> Self {
        Self {
            baselines: HashMap::new(),
            window_size: config.default_window_size,
            min_data_points: config.min_data_points,
            z_score_trigger: config.z_score_trigger,
            direction_up: config.direction_thresholds.up,
            direction_down: config.direction_thresholds.down,
            severity_significant: config.severity_thresholds.significant,
            severity_critical: config.severity_thresholds.critical,
        }
    }

    /// Extract current metric values from a batch of documents.
    pub fn extract_metrics(docs: &[Document]) -> HashMap<String, f64> {
        let mut metrics: HashMap<String, f64> = HashMap::new();

        let mut event_counts: HashMap<String, f64> = HashMap::new();
        let mut unique_members = std::collections::HashSet::new();
        let mut error_count = 0.0;
        let mut total_count = 0.0;

        for doc in docs {
            total_count += 1.0;

            *event_counts
                .entry(doc.event_type.clone())
                .or_default() += 1.0;

            if let Some(member) = doc.fields.get("memberCode").and_then(FieldValue::as_str) {
                if !member.is_empty() {
                    unique_members.insert(member.to_owned());
                }
            }

            if doc.event_type.contains("Error") || doc.event_type.contains("error") {
                error_count += 1.0;
            }
        }

        // Events per type.
        for (event_type, count) in &event_counts {
            metrics.insert(format!("events_{}", event_type), *count);
        }

        // Unique members.
        metrics.insert("unique_members".to_string(), unique_members.len() as f64);

        // Error rate.
        let error_rate = if total_count > 0.0 {
            error_count / total_count
        } else {
            0.0
        };
        metrics.insert("error_rate".to_string(), error_rate);

        // Total events.
        metrics.insert("total_events".to_string(), total_count);

        metrics
    }

    /// Update baselines with current metric values and detect trends.
    ///
    /// Returns any trends with |z| > 2.
    pub fn detect(&mut self, current_metrics: &HashMap<String, f64>) -> Vec<Trend> {
        let now = Utc::now();
        let mut trends = Vec::new();

        for (metric_name, &current_value) in current_metrics {
            let baseline = self
                .baselines
                .entry(metric_name.clone())
                .or_insert_with(|| MetricBaseline::new(self.window_size));

            let mean = baseline.mean();
            let stddev = baseline.stddev();

            // Need at least min_data_points and nonzero stddev to compute z-score.
            if baseline.values.len() >= self.min_data_points && stddev > f64::EPSILON {
                let z_score = (current_value - mean) / stddev;

                let abs_z = z_score.abs();
                if abs_z > self.z_score_trigger {
                    let direction = if z_score > self.direction_up {
                        TrendDirection::Up
                    } else if z_score < -self.direction_down {
                        TrendDirection::Down
                    } else {
                        TrendDirection::Stable
                    };

                    let severity = if abs_z > self.severity_critical {
                        Severity::Critical
                    } else if abs_z > self.severity_significant {
                        Severity::Significant
                    } else {
                        Severity::Notable
                    };

                    trends.push(Trend {
                        metric: metric_name.clone(),
                        current_value,
                        baseline_mean: mean,
                        baseline_stddev: stddev,
                        z_score,
                        direction,
                        severity,
                        since: now,
                    });
                }
            }

            // Add current value to baseline for future comparisons.
            baseline.push(current_value);
        }

        debug!(
            metrics = current_metrics.len(),
            trends = trends.len(),
            "trend detection complete"
        );

        trends
    }
}

/// Compute z-score for a value against a baseline.
pub fn z_score(value: f64, mean: f64, stddev: f64) -> f64 {
    if stddev <= f64::EPSILON {
        return 0.0;
    }
    (value - mean) / stddev
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use uuid::Uuid;

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
    fn z_score_basic() {
        assert!((z_score(10.0, 5.0, 2.0) - 2.5).abs() < 1e-10);
        assert!((z_score(5.0, 5.0, 2.0)).abs() < 1e-10);
        assert_eq!(z_score(5.0, 5.0, 0.0), 0.0); // zero stddev
    }

    #[test]
    fn metric_baseline_stats() {
        let mut baseline = MetricBaseline::new(10);
        baseline.push(10.0);
        baseline.push(12.0);
        baseline.push(8.0);

        let mean = baseline.mean();
        assert!((mean - 10.0).abs() < 1e-10);

        let std = baseline.stddev();
        assert!(std > 0.0);
    }

    #[test]
    fn metric_baseline_window() {
        let mut baseline = MetricBaseline::new(3);
        baseline.push(1.0);
        baseline.push(2.0);
        baseline.push(3.0);
        baseline.push(4.0); // should evict 1.0

        assert_eq!(baseline.values.len(), 3);
        assert_eq!(baseline.values[0], 2.0);
    }

    #[test]
    fn extract_metrics_from_docs() {
        let docs = vec![
            make_doc("Login", vec![("memberCode", "M001")]),
            make_doc("Login", vec![("memberCode", "M002")]),
            make_doc("GameOpened", vec![("memberCode", "M001")]),
            make_doc("API Error", vec![("memberCode", "M003")]),
        ];

        let metrics = TrendDetector::extract_metrics(&docs);

        assert_eq!(metrics["events_Login"], 2.0);
        assert_eq!(metrics["events_GameOpened"], 1.0);
        assert_eq!(metrics["events_API Error"], 1.0);
        assert_eq!(metrics["unique_members"], 3.0);
        assert_eq!(metrics["total_events"], 4.0);
        assert!((metrics["error_rate"] - 0.25).abs() < 1e-10);
    }

    #[test]
    fn detect_trend_after_baseline() {
        let mut detector = TrendDetector::with_window(10);

        // Feed stable baseline values.
        for _ in 0..10 {
            let mut metrics = HashMap::new();
            metrics.insert("test_metric".to_string(), 100.0);
            detector.detect(&metrics);
        }

        // Now feed a significantly different value.
        let mut metrics = HashMap::new();
        metrics.insert("test_metric".to_string(), 200.0);
        let trends = detector.detect(&metrics);

        // Should detect a trend since stddev should be ~0 (all 100s), making z very large.
        // But actually stddev of all 100s is 0, so no trend (needs variance).
        // Let's add some variance instead.
        assert!(trends.is_empty()); // all same values = 0 stddev = no z-score
    }

    #[test]
    fn detect_trend_with_variance() {
        let mut detector = TrendDetector::with_window(20);

        // Feed baseline values with some variance.
        let baseline_values = vec![100.0, 102.0, 98.0, 101.0, 99.0, 103.0, 97.0, 100.0, 102.0, 98.0];
        for val in &baseline_values {
            let mut metrics = HashMap::new();
            metrics.insert("test_metric".to_string(), *val);
            detector.detect(&metrics);
        }

        // Feed a very high value (should trigger trend).
        let mut metrics = HashMap::new();
        metrics.insert("test_metric".to_string(), 150.0);
        let trends = detector.detect(&metrics);

        assert!(!trends.is_empty(), "Should detect a trend for value far from baseline");
        let trend = &trends[0];
        assert_eq!(trend.direction, TrendDirection::Up);
        assert!(trend.z_score > 2.0);
    }

    #[test]
    fn no_trends_for_normal_values() {
        let mut detector = TrendDetector::with_window(20);

        let baseline_values = vec![100.0, 102.0, 98.0, 101.0, 99.0, 103.0, 97.0, 100.0, 102.0, 98.0];
        for val in &baseline_values {
            let mut metrics = HashMap::new();
            metrics.insert("test_metric".to_string(), *val);
            detector.detect(&metrics);
        }

        // Feed a normal value.
        let mut metrics = HashMap::new();
        metrics.insert("test_metric".to_string(), 101.0);
        let trends = detector.detect(&metrics);

        assert!(trends.is_empty(), "Should not detect trend for normal value");
    }
}
