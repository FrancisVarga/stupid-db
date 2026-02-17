use tracing::info;

use stupid_storage::{StorageEngine, S3Exporter, S3Importer};

use crate::background::discover_segments;
use crate::graph_ops::build_graph_multi;

pub(crate) async fn import_s3(config: &stupid_core::Config, s3_prefix: &str) -> anyhow::Result<()> {
    let storage = StorageEngine::from_config(config)?;
    if !storage.backend.is_remote() {
        anyhow::bail!("import-s3 requires S3 configuration (S3_BUCKET, AWS_REGION, etc.)");
    }
    info!("Importing parquet files from S3 prefix: {}", s3_prefix);
    let (docs, segments) = S3Importer::import_all(
        &storage.backend,
        s3_prefix,
        &storage.data_dir,
    )
    .await?;
    info!(
        "S3 import complete: {} documents in {} segments",
        docs, segments
    );
    Ok(())
}

pub(crate) async fn export(config: &stupid_core::Config, export_segments: bool, export_graph: bool) -> anyhow::Result<()> {
    let storage = StorageEngine::from_config(config)?;
    if !storage.backend.is_remote() {
        anyhow::bail!("export requires S3 configuration (S3_BUCKET, AWS_REGION, etc.)");
    }
    let data_dir = &config.storage.data_dir;

    if export_segments {
        let segment_ids = discover_segments(data_dir);
        if segment_ids.is_empty() {
            info!("No local segments to export");
        } else {
            info!("Exporting {} segments to S3...", segment_ids.len());
            let (uploaded, skipped) =
                S3Exporter::export_segments(&storage.backend, data_dir, &segment_ids).await?;
            info!("Segment export: {} uploaded, {} skipped", uploaded, skipped);
        }
    }

    if export_graph {
        let segment_ids = discover_segments(data_dir);
        if segment_ids.is_empty() {
            info!("No segments — skipping graph export");
        } else {
            info!("Building graph for export...");
            let (graph, _) = build_graph_multi(data_dir, &segment_ids)?;
            let stats = graph.stats();
            S3Exporter::export_graph(&storage.backend, &stats).await?;
            info!("Graph stats exported to S3");
        }
    }

    Ok(())
}
