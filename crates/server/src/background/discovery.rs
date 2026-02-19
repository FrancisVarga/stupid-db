use std::path::Path;

/// Walk `<data_dir>/segments/` and return segment IDs for every directory
/// that contains a `documents.dat` file.
pub(crate) fn discover_segments(data_dir: &Path) -> Vec<String> {
    let segments_dir = data_dir.join("segments");
    if !segments_dir.exists() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    for entry in walkdir::WalkDir::new(&segments_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.file_name().map(|n| n == "documents.dat").unwrap_or(false) {
            // segment_id = path relative to segments_dir, minus the filename
            if let Ok(rel) = path.parent().unwrap_or(path).strip_prefix(&segments_dir) {
                if let Some(seg_id) = rel.to_str() {
                    // Normalize backslashes to forward slashes
                    segments.push(seg_id.replace('\\', "/"));
                }
            }
        }
    }

    segments.sort();
    segments
}
