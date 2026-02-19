//! Chunk configuration and output types.

// ── Configuration ───────────────────────────────────────────────────────────

/// Configuration for the chunking engine.
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Maximum tokens per chunk (default: 500).
    pub max_chunk_tokens: usize,
    /// Minimum tokens per chunk — merge smaller chunks (default: 50).
    pub min_chunk_tokens: usize,
    /// Overlap tokens between adjacent chunks (default: 50).
    pub overlap_tokens: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            max_chunk_tokens: 500,
            min_chunk_tokens: 50,
            overlap_tokens: 50,
        }
    }
}

// ── Chunk output ────────────────────────────────────────────────────────────

/// A chunk of text with metadata for attribution.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// 0-based index within the document.
    pub index: usize,
    /// The chunk text content.
    pub content: String,
    /// Page number (from PDF page or 1 for TXT/MD).
    pub page_number: Option<usize>,
    /// Section heading (from MD headings).
    pub section_heading: Option<String>,
    /// Character offset in the original document.
    pub char_offset: usize,
}
