//! Chunking strategies: markdown (heading-aware), PDF (page-aware), and plain text.

use super::helpers::{build_chunks_with_overlap, merge_tiny, split_oversized};
use super::types::{Chunk, ChunkConfig};
use crate::document::ExtractedDocument;

/// Chunk a document using a strategy appropriate for its file type.
pub fn chunk_document(doc: &ExtractedDocument, config: &ChunkConfig) -> Vec<Chunk> {
    match doc.file_type.as_str() {
        "md" | "markdown" => chunk_markdown(doc, config),
        "pdf" => chunk_pdf(doc, config),
        _ => chunk_text(doc, config),
    }
}

// ── Markdown strategy ───────────────────────────────────────────────────────

fn chunk_markdown(doc: &ExtractedDocument, config: &ChunkConfig) -> Vec<Chunk> {
    let full = doc.full_text();
    let mut sections: Vec<(Option<String>, String)> = Vec::new();

    let mut current_heading: Option<String> = None;
    let mut current_text = String::new();

    for line in full.lines() {
        if line.starts_with("## ") || line.starts_with("### ") || line.starts_with("#### ") {
            // Flush previous section.
            let text = current_text.trim().to_string();
            if !text.is_empty() {
                sections.push((current_heading.clone(), text));
            }
            current_heading = Some(line.trim_start_matches('#').trim().to_string());
            current_text = String::new();
        } else {
            if !current_text.is_empty() {
                current_text.push('\n');
            }
            current_text.push_str(line);
        }
    }
    // Flush last section.
    let text = current_text.trim().to_string();
    if !text.is_empty() {
        sections.push((current_heading, text));
    }

    // Split oversized sections and merge tiny ones per-heading group.
    let mut all_chunks = Vec::new();
    let mut char_offset = 0usize;

    for (heading, text) in &sections {
        let pieces = split_oversized(text, config.max_chunk_tokens);
        let pieces = merge_tiny(pieces, config.min_chunk_tokens);
        let mut section_chunks = build_chunks_with_overlap(
            pieces,
            config,
            Some(1),
            heading.clone(),
            char_offset,
        );
        all_chunks.append(&mut section_chunks);
        char_offset += text.len() + 2; // account for section gap
    }

    // Assign global indices.
    for (i, c) in all_chunks.iter_mut().enumerate() {
        c.index = i;
    }
    all_chunks
}

// ── Text strategy ───────────────────────────────────────────────────────────

fn chunk_text(doc: &ExtractedDocument, config: &ChunkConfig) -> Vec<Chunk> {
    let full = doc.full_text();
    let pieces = split_oversized(&full, config.max_chunk_tokens);
    let pieces = merge_tiny(pieces, config.min_chunk_tokens);
    let mut chunks = build_chunks_with_overlap(pieces, config, Some(1), None, 0);
    for (i, c) in chunks.iter_mut().enumerate() {
        c.index = i;
    }
    chunks
}

// ── PDF strategy ────────────────────────────────────────────────────────────

fn chunk_pdf(doc: &ExtractedDocument, config: &ChunkConfig) -> Vec<Chunk> {
    let mut all_chunks = Vec::new();
    let mut char_offset = 0usize;

    for page in &doc.pages {
        let pieces = split_oversized(&page.text, config.max_chunk_tokens);
        let pieces = merge_tiny(pieces, config.min_chunk_tokens);

        // No overlap across page boundaries -- build per-page, then extend.
        for frag in &pieces {
            all_chunks.push(Chunk {
                index: 0,
                content: frag.clone(),
                page_number: Some(page.page_number),
                section_heading: None,
                char_offset,
            });
            char_offset += frag.len() + 2;
        }
        char_offset += 2; // page gap
    }

    // Assign global indices.
    for (i, c) in all_chunks.iter_mut().enumerate() {
        c.index = i;
    }
    all_chunks
}
