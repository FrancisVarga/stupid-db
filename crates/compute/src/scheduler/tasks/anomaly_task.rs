use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tracing::info;
use uuid::Uuid;

use crate::scheduler::state::KnowledgeState;
use crate::scheduler::task::{ComputeError, ComputeTask};
use crate::scheduler::types::{ComputeResult, Insight, InsightSeverity, Priority};

/// Multi-signal anomaly detection as a schedulable compute task.
///
/// Reads existing anomaly scores from KnowledgeState (written by
/// `Pipeline::warm_compute`) and generates insights for anomalous members.
///
/// The actual multi-signal scoring happens in the pipeline's warm compute
/// stage via `multi_signal_score_all`. This task complements it by
/// periodically generating insights and capping the insight queue.
pub struct AnomalyDetectionTask {
    interval: Duration,
}

impl AnomalyDetectionTask {
    pub fn new(interval: Duration) -> Self {
        Self { interval }
    }
}

impl ComputeTask for AnomalyDetectionTask {
    fn name(&self) -> &str {
        "anomaly_detection"
    }

    fn priority(&self) -> Priority {
        Priority::P2
    }

    fn estimated_duration(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn execute(&self, state: &mut KnowledgeState) -> Result<ComputeResult, ComputeError> {
        let start = Instant::now();

        let member_count = state.anomalies.len();
        let mut anomaly_count = 0usize;

        for (_member_id, score) in &state.anomalies {
            if score.is_anomalous {
                anomaly_count += 1;
            }
        }

        // Generate insights for high-scoring members.
        if anomaly_count > 0 {
            for (member_id, score) in &state.anomalies {
                if !score.is_anomalous {
                    continue;
                }

                let severity = if score.score > 4.0 {
                    InsightSeverity::Critical
                } else if score.score > 3.0 {
                    InsightSeverity::Warning
                } else {
                    InsightSeverity::Info
                };

                let insight = Insight {
                    id: Uuid::new_v4().to_string(),
                    title: format!("Anomaly detected (score={:.2})", score.score),
                    description: format!(
                        "Member {} flagged by anomaly detection with score {:.2}",
                        member_id, score.score
                    ),
                    severity,
                    created_at: Utc::now(),
                    related_nodes: vec![*member_id],
                };

                state.insights.push_back(insight);
            }

            // Cap insight queue.
            const MAX_INSIGHTS: usize = 10_000;
            while state.insights.len() > MAX_INSIGHTS {
                state.insights.pop_front();
            }
        }

        let duration = start.elapsed();

        info!(
            "Anomaly detection: {} members scored, {} anomalous ({:.1}s)",
            member_count,
            anomaly_count,
            duration.as_secs_f64()
        );

        Ok(ComputeResult {
            task_name: self.name().to_string(),
            duration,
            items_processed: member_count,
            summary: Some(format!(
                "{} members scored, {} anomalous",
                member_count, anomaly_count
            )),
        })
    }

    fn should_run(&self, last_run: Option<DateTime<Utc>>, _state: &KnowledgeState) -> bool {
        match last_run {
            None => true,
            Some(last) => {
                let elapsed = Utc::now().signed_duration_since(last);
                elapsed.to_std().unwrap_or_default() >= self.interval
            }
        }
    }
}
