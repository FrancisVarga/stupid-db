use std::path::Path;

use tracing::info;

/// Parse ISO week from a date-like filename stem (e.g., "2025-06-14" -> "2025-W24").
pub(crate) fn date_to_iso_week(date_stem: &str) -> String {
    use chrono::{Datelike, NaiveDate};

    if let Ok(date) = NaiveDate::parse_from_str(date_stem, "%Y-%m-%d") {
        let iso = date.iso_week();
        format!("{}-W{:02}", iso.year(), iso.week())
    } else {
        // Can't parse date — put in "misc" bucket
        "misc".to_string()
    }
}

/// A group of parquet files that will be merged into one weekly segment.
pub(crate) struct ImportGroup {
    pub segment_id: String,
    pub event_type: String,
    pub files: Vec<std::path::PathBuf>,
}

pub(crate) fn import(config: &stupid_core::Config, parquet_path: &Path, segment_id: &str) -> anyhow::Result<()> {
    info!("Importing {} as segment '{}'", parquet_path.display(), segment_id);

    let event_type = parquet_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown");

    let documents = stupid_ingest::parquet_import::ParquetImporter::import(parquet_path, event_type)?;
    info!("Read {} documents from parquet", documents.len());

    let data_dir = &config.storage.data_dir;
    let mut writer = stupid_segment::writer::SegmentWriter::new(data_dir, segment_id)?;

    for doc in &documents {
        writer.append(doc)?;
    }
    writer.finalize()?;

    // Build graph from segment
    let reader = stupid_segment::reader::SegmentReader::open(data_dir, segment_id)?;
    let mut graph = stupid_graph::GraphStore::new();
    let seg_id = segment_id.to_string();

    let mut doc_count = 0u64;
    for doc_result in reader.iter() {
        let doc = doc_result?;
        stupid_connector::entity_extract::EntityExtractor::extract(&doc, &mut graph, &seg_id);
        doc_count += 1;
    }

    info!("Processed {} documents for entity extraction", doc_count);

    let stats = graph.stats();
    info!("Graph stats:");
    info!("  Nodes: {}", stats.node_count);
    for (typ, count) in &stats.nodes_by_type {
        info!("    {}: {}", typ, count);
    }
    info!("  Edges: {}", stats.edge_count);
    for (typ, count) in &stats.edges_by_type {
        info!("    {}: {}", typ, count);
    }

    Ok(())
}

pub(crate) fn import_dir(config: &stupid_core::Config, dir_path: &Path) -> anyhow::Result<()> {
    use rayon::prelude::*;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    info!("Scanning {} for parquet files...", dir_path.display());

    let mut parquet_files: Vec<std::path::PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(dir_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "parquet").unwrap_or(false) {
            parquet_files.push(path.to_path_buf());
        }
    }

    parquet_files.sort();
    info!("Found {} parquet files", parquet_files.len());

    if parquet_files.is_empty() {
        anyhow::bail!("No .parquet files found in {}", dir_path.display());
    }

    // Group files by (event_type, iso_week)
    let mut groups: BTreeMap<String, ImportGroup> = BTreeMap::new();

    for parquet_path in &parquet_files {
        let event_type = parquet_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let date_stem = parquet_path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let week = date_to_iso_week(date_stem);
        let segment_id = format!("{}/{}", event_type, week);

        groups
            .entry(segment_id.clone())
            .or_insert_with(|| ImportGroup {
                segment_id,
                event_type: event_type.clone(),
                files: Vec::new(),
            })
            .files
            .push(parquet_path.clone());
    }

    let group_list: Vec<ImportGroup> = groups.into_values().collect();
    let total_groups = group_list.len();
    info!(
        "Grouped into {} weekly segments — importing with {} threads",
        total_groups,
        rayon::current_num_threads()
    );

    let total_docs = AtomicU64::new(0);
    let completed = AtomicU64::new(0);
    let failed = AtomicU64::new(0);
    let data_dir = &config.storage.data_dir;
    let start = std::time::Instant::now();

    // Parallel import: one group = one segment, each group processes independently
    group_list.par_iter().for_each(|group| {
        let mut writer = match stupid_segment::writer::SegmentWriter::new(data_dir, &group.segment_id) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to create segment '{}': {}", group.segment_id, e);
                failed.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };

        let mut group_docs = 0u64;

        for parquet_path in &group.files {
            let documents = match stupid_ingest::parquet_import::ParquetImporter::import(
                parquet_path,
                &group.event_type,
            ) {
                Ok(docs) => docs,
                Err(e) => {
                    tracing::warn!("Failed to read {}: {}", parquet_path.display(), e);
                    continue;
                }
            };

            for doc in &documents {
                if let Err(e) = writer.append(doc) {
                    tracing::warn!("Write error in '{}': {}", group.segment_id, e);
                    break;
                }
            }

            group_docs += documents.len() as u64;
        }

        if let Err(e) = writer.finalize() {
            tracing::warn!("Failed to finalize '{}': {}", group.segment_id, e);
            failed.fetch_add(1, Ordering::Relaxed);
            return;
        }

        total_docs.fetch_add(group_docs, Ordering::Relaxed);
        let done = completed.fetch_add(1, Ordering::Relaxed) + 1;

        if done % 5 == 0 || done as usize == total_groups {
            info!(
                "  Progress: {}/{} segments ({} docs, {:.1}s)",
                done,
                total_groups,
                total_docs.load(Ordering::Relaxed),
                start.elapsed().as_secs_f64()
            );
        }
    });

    let elapsed = start.elapsed();
    let final_docs = total_docs.load(Ordering::Relaxed);
    let final_done = completed.load(Ordering::Relaxed);
    let final_failed = failed.load(Ordering::Relaxed);
    info!(
        "Import complete: {} segments ({} parquet files), {} docs total in {:.1}s ({} failed)",
        final_done,
        parquet_files.len(),
        final_docs,
        elapsed.as_secs_f64(),
        final_failed
    );
    Ok(())
}
