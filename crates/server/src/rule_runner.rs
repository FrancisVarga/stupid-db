//! Background rule evaluation loop.
//!
//! Spawns a tokio task that waits for data loading to complete, then
//! periodically evaluates due anomaly rules against the current
//! knowledge state according to their cron schedules.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use chrono::Utc;
use tracing::{debug, info, warn};

use stupid_rules::audit_log::{ExecutionPhase, LogLevel};
use stupid_rules::evaluator::{RuleEvaluator, SignalScores};
use stupid_rules::scheduler::RuleScheduler;

use crate::anomaly_rules::MatchSummary;
use stupid_rules::templates::{ClusterStats, EntityData};

use crate::anomaly_rules::TriggerEntry;
use crate::state::AppState;

/// Interval between scheduler ticks (seconds).
const TICK_INTERVAL_SECS: u64 = 60;
/// Maximum trigger history entries per rule.
const MAX_HISTORY_ENTRIES: usize = 500;

/// Build `EntityData` and `SignalScores` maps from the compute pipeline state.
///
/// This bridges the gap between `stupid_compute`'s internal representation
/// (NodeId-keyed feature vectors + anomaly scores) and `stupid_rules`'s
/// evaluator input (String-keyed entity data + signal scores).
pub fn build_evaluation_context(
    state: &AppState,
) -> (
    HashMap<String, EntityData>,
    HashMap<usize, ClusterStats>,
    HashMap<String, SignalScores>,
) {
    let pipeline = state.pipeline.lock().expect("pipeline lock poisoned");
    let knowledge = state.knowledge.read().expect("knowledge lock poisoned");

    let mut entities = HashMap::new();
    let mut signal_scores_map = HashMap::new();

    for member_id in pipeline.features.members() {
        let key = match pipeline.features.member_key(member_id) {
            Some(k) => k.to_string(),
            None => continue,
        };

        let features = match pipeline.features.to_feature_vector(member_id) {
            Some(fv) => fv,
            None => continue,
        };

        let score = knowledge
            .anomalies
            .get(member_id)
            .map(|a| a.score)
            .unwrap_or(0.0);

        let cluster_id = knowledge.clusters.get(member_id).copied();

        entities.insert(
            key.clone(),
            EntityData {
                key: key.clone(),
                entity_type: "Member".to_string(),
                features,
                score,
                cluster_id: cluster_id.map(|c| c as usize),
            },
        );

        // Build signal scores from anomaly score.
        // The compute pipeline stores a single composite score; individual
        // signal breakdowns are not persisted in KnowledgeState. We expose
        // the composite as `z_score` so that rules using `z_score > X`
        // thresholds work against the pipeline's output.
        let mut scores = HashMap::new();
        scores.insert("z_score".to_string(), score);
        signal_scores_map.insert(
            key,
            SignalScores { scores },
        );
    }

    // Build cluster stats from cluster_info.
    let cluster_stats: HashMap<usize, ClusterStats> = knowledge
        .cluster_info
        .iter()
        .map(|(&id, info)| {
            (
                id as usize,
                ClusterStats {
                    centroid: info.centroid.clone(),
                    member_count: info.member_count,
                },
            )
        })
        .collect();

    drop(knowledge);
    drop(pipeline);

    (entities, cluster_stats, signal_scores_map)
}

/// Main rule evaluation loop. Spawned as a tokio task.
///
/// 1. Waits for data loading to complete (polls `LoadingState`).
/// 2. On each 60s tick, syncs rules, finds due rules, evaluates them.
/// 3. Records trigger history and audit log entries.
pub async fn run_rule_loop(state: Arc<AppState>) {
    info!("Rule auto-runner started, waiting for data loading...");

    // Wait until data loading completes or fails (max 5 minutes).
    // Rules should still run even without loaded data — they'll just get
    // 0 matches against an empty pipeline, but the scheduler remains active
    // so the sidebar shows trigger history.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(300);
    loop {
        let status = state.loading.to_status().await;
        if status.is_ready {
            break;
        }
        if status.phase == "failed" {
            warn!("Data loading failed — rule auto-runner will evaluate against empty data");
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            warn!("Data loading timed out after 5m — rule auto-runner starting anyway");
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    let rules_arc = state.rule_loader.rules();
    let rule_count = rules_arc.read().expect("rules lock").len();
    info!(
        "Rule auto-runner active: {} rules synced, evaluating due rules every {}s",
        rule_count, TICK_INTERVAL_SECS
    );

    let mut scheduler = RuleScheduler::new();

    // Initial sync.
    {
        let guard = rules_arc.read().expect("rules lock");
        let rules_vec: Vec<_> = guard.values().cloned().collect();
        scheduler.sync_rules(&rules_vec);
    }

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(TICK_INTERVAL_SECS));

    loop {
        interval.tick().await;

        // Re-sync rules (picks up hot-reloaded changes).
        {
            let guard = rules_arc.read().expect("rules lock");
            let rules_vec: Vec<_> = guard.values().cloned().collect();
            scheduler.sync_rules(&rules_vec);
        }

        let now = Utc::now();
        let due: Vec<String> = scheduler
            .due_rules(now)
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        if due.is_empty() {
            debug!("Rule tick: no rules due at {}", now);
            continue;
        }

        info!("Rule tick: {} rule(s) due for evaluation", due.len());

        // Build evaluation context on a blocking thread to avoid holding
        // std::sync locks on the tokio runtime.
        let state_clone = state.clone();
        let due_clone = due.clone();
        let eval_result = tokio::task::spawn_blocking(move || {
            let (entities, cluster_stats, signal_scores) =
                build_evaluation_context(&state_clone);

            let rules_arc = state_clone.rule_loader.rules();
            let guard = rules_arc.read().expect("rules lock");

            let mut results = Vec::new();

            for rule_id in &due_clone {
                let rule = match guard.get(rule_id) {
                    Some(r) => r,
                    None => continue,
                };

                state_clone.audit_log.log(
                    rule_id,
                    LogLevel::Info,
                    ExecutionPhase::ScheduleCheck,
                    "Rule due for evaluation",
                );

                let start = std::time::Instant::now();
                match RuleEvaluator::evaluate(rule, &entities, &cluster_stats, &signal_scores) {
                    Ok(mut matches) => {
                        let evaluation_ms = start.elapsed().as_millis() as u64;
                        let matches_found = matches.len();

                        // Sort by score descending and keep top 50 for history.
                        matches.sort_by(|a, b| {
                            b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        let summaries: Vec<MatchSummary> = matches
                            .iter()
                            .take(50)
                            .map(|m| MatchSummary {
                                entity_key: m.entity_key.clone(),
                                entity_type: m.entity_type.clone(),
                                score: m.score,
                                reason: m.matched_reason.clone(),
                            })
                            .collect();

                        state_clone.audit_log.log(
                            rule_id,
                            LogLevel::Info,
                            ExecutionPhase::Complete,
                            format!(
                                "Evaluation complete: {} matches in {}ms",
                                matches_found, evaluation_ms
                            ),
                        );

                        results.push((rule_id.clone(), matches_found, evaluation_ms, summaries));
                    }
                    Err(e) => {
                        state_clone.audit_log.log(
                            rule_id,
                            LogLevel::Error,
                            ExecutionPhase::Evaluation,
                            format!("Evaluation failed: {}", e),
                        );
                        warn!(rule_id = %rule_id, error = %e, "Rule evaluation failed");
                    }
                }
            }

            // Record trigger history.
            {
                let mut history = state_clone
                    .trigger_history
                    .write()
                    .expect("trigger_history lock");
                for (rule_id, matches_found, evaluation_ms, match_summaries) in &results {
                    let entry = TriggerEntry {
                        timestamp: Utc::now().to_rfc3339(),
                        matches_found: *matches_found,
                        evaluation_ms: *evaluation_ms,
                        matches: match_summaries.clone(),
                    };
                    let deque = history
                        .entry(rule_id.clone())
                        .or_insert_with(|| VecDeque::with_capacity(MAX_HISTORY_ENTRIES));
                    deque.push_back(entry);
                    while deque.len() > MAX_HISTORY_ENTRIES {
                        deque.pop_front();
                    }
                }
            }

            results
        })
        .await;

        match eval_result {
            Ok(results) => {
                for (rule_id, matches_found, evaluation_ms, _) in &results {
                    scheduler.record_trigger(rule_id);
                    info!(
                        rule_id = %rule_id,
                        matches = matches_found,
                        ms = evaluation_ms,
                        "Rule evaluated"
                    );
                }
            }
            Err(e) => {
                warn!("Rule evaluation task panicked: {}", e);
            }
        }
    }
}
