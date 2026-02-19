use std::sync::Arc;

use tracing::info;

use crate::state::{self, SharedGraph, SharedPipeline};

/// Run initial graph algorithms (PageRank, degree centrality, community detection)
/// and the hot_connect + warm_compute pipeline, then start the background scheduler.
pub(super) async fn run_compute(
    segments: &[String],
    effective_data_dir: &std::path::Path,
    shared_graph: SharedGraph,
    knowledge: stupid_compute::SharedKnowledgeState,
    pipeline: SharedPipeline,
    app_state: &Arc<state::AppState>,
) {
    let sched_config = stupid_compute::SchedulerConfig::default();
    let mut scheduler = stupid_compute::Scheduler::new(sched_config, knowledge.clone());

    let p2_interval = std::time::Duration::from_secs(3600);
    scheduler.register_task(Arc::new(
        stupid_compute::scheduler::tasks::PageRankTask::new(shared_graph.clone(), p2_interval),
    ));
    scheduler.register_task(Arc::new(
        stupid_compute::scheduler::tasks::DegreeCentralityTask::new(shared_graph.clone(), p2_interval),
    ));
    scheduler.register_task(Arc::new(
        stupid_compute::scheduler::tasks::CommunityDetectionTask::new(shared_graph.clone(), p2_interval),
    ));
    scheduler.register_task(Arc::new(
        stupid_compute::AnomalyDetectionTask::new(p2_interval),
    ));

    scheduler.add_dependency("entity_extraction", "pagerank");
    scheduler.add_dependency("entity_extraction", "community_detection");

    run_initial_algorithms(&shared_graph, &knowledge).await;
    run_pipeline(segments, effective_data_dir, &pipeline, &knowledge);

    let shutdown = scheduler.shutdown_signal();
    let metrics = scheduler.metrics_handle();
    {
        let mut sched_lock = app_state.scheduler.write().await;
        *sched_lock = Some(state::SchedulerHandle { shutdown, metrics });
    }

    std::thread::spawn(move || {
        scheduler.run();
    });
}

/// Run PageRank, degree centrality, and community detection on the current graph.
async fn run_initial_algorithms(
    shared_graph: &SharedGraph,
    knowledge: &stupid_compute::SharedKnowledgeState,
) {
    info!("Running initial PageRank, degree, community computations...");
    let graph_read = shared_graph.read().await;

    let t = std::time::Instant::now();
    let pagerank = stupid_compute::algorithms::pagerank::pagerank_default(&graph_read);
    info!("  pagerank done in {:.1}s ({} nodes)", t.elapsed().as_secs_f64(), pagerank.len());

    let t = std::time::Instant::now();
    let degrees = stupid_compute::algorithms::degree::degree_centrality(&graph_read);
    info!("  degree_centrality done in {:.1}s ({} nodes)", t.elapsed().as_secs_f64(), degrees.len());

    let t = std::time::Instant::now();
    let communities = stupid_compute::algorithms::communities::label_propagation_default(&graph_read);
    info!("  community_detection done in {:.1}s ({} communities)", t.elapsed().as_secs_f64(), communities.len());

    drop(graph_read);

    let mut state = knowledge.write().unwrap();
    state.pagerank = pagerank;
    state.degrees = degrees;
    state.communities = communities;

    drop(state);

    let k = knowledge.read().unwrap();
    info!(
        "Initial compute complete — knowledge state: {} pagerank scores, {} degree entries, {} communities",
        k.pagerank.len(), k.degrees.len(), k.communities.len()
    );
}

/// Run hot_connect and warm_compute across all segments.
fn run_pipeline(
    segments: &[String],
    effective_data_dir: &std::path::Path,
    pipeline: &SharedPipeline,
    knowledge: &stupid_compute::SharedKnowledgeState,
) {
    info!("Running compute pipeline (hot_connect + warm_compute)...");
    let pipeline_start = std::time::Instant::now();
    let mut all_docs: Vec<stupid_core::Document> = Vec::new();

    for seg_id in segments {
        let reader = match stupid_segment::reader::SegmentReader::open(effective_data_dir, seg_id) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let mut seg_docs: Vec<stupid_core::Document> = Vec::new();
        for doc_result in reader.iter() {
            if let Ok(doc) = doc_result {
                seg_docs.push(doc);
            }
        }

        {
            let mut pipe = pipeline.lock().unwrap();
            let mut state = knowledge.write().unwrap();
            pipe.hot_connect(&seg_docs, &mut state);
        }

        all_docs.extend(seg_docs);

        if all_docs.len() > 100_000 {
            let mut pipe = pipeline.lock().unwrap();
            let mut state = knowledge.write().unwrap();
            pipe.warm_compute(&mut state, &all_docs);
            all_docs.clear();
        }
    }

    if !all_docs.is_empty() {
        let mut pipe = pipeline.lock().unwrap();
        let mut state = knowledge.write().unwrap();
        pipe.warm_compute(&mut state, &all_docs);
    }

    let k = knowledge.read().unwrap();
    info!(
        "Pipeline complete in {:.1}s — {} anomalies, {} trends, {} co-occurrence matrices, {} clusters",
        pipeline_start.elapsed().as_secs_f64(),
        k.anomalies.len(),
        k.trends.len(),
        k.cooccurrence.len(),
        k.clusters.len()
    );
}
