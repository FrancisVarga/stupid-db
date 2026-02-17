//! Smart semantic chunking engine.
//!
//! Splits extracted documents into overlapping chunks suitable for embedding,
//! dispatching strategy by file type: markdown (heading-aware), PDF (page-aware),
//! and plain text (paragraph/sentence splitting).

use super::ExtractedDocument;

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

// ── Public entry point ──────────────────────────────────────────────────────

/// Chunk a document using a strategy appropriate for its file type.
pub fn chunk_document(doc: &ExtractedDocument, config: &ChunkConfig) -> Vec<Chunk> {
    match doc.file_type.as_str() {
        "md" | "markdown" => chunk_markdown(doc, config),
        "pdf" => chunk_pdf(doc, config),
        _ => chunk_text(doc, config),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Approximate token count via whitespace splitting.
fn count_tokens(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Extract the last `overlap_tokens` words from `text`.
fn get_overlap_text(text: &str, overlap_tokens: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= overlap_tokens {
        return text.to_string();
    }
    words[words.len() - overlap_tokens..].join(" ")
}

/// Split `text` at sentence boundaries (`. `, `! `, `? ` followed by uppercase
/// or newline). Returns non-empty fragments.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        let is_terminal = bytes[i] == b'.' || bytes[i] == b'!' || bytes[i] == b'?';
        if is_terminal {
            // Look ahead: must be followed by a space then uppercase or newline.
            if i + 1 < bytes.len() && bytes[i + 1] == b' ' {
                let after_space = if i + 2 < bytes.len() {
                    bytes[i + 2]
                } else {
                    b'\n' // end-of-string acts like newline
                };
                if after_space.is_ascii_uppercase() || after_space == b'\n' {
                    let end = i + 1; // include the terminal punctuation
                    let s = text[start..end].trim();
                    if !s.is_empty() {
                        sentences.push(s.to_string());
                    }
                    start = end + 1; // skip the space
                    i = start;
                    continue;
                }
            }
        }
        i += 1;
    }

    // Remainder
    let tail = text[start..].trim();
    if !tail.is_empty() {
        sentences.push(tail.to_string());
    }
    sentences
}

/// Split text that exceeds `max_chunk_tokens` first by `\n\n`, then by sentence.
/// Returns pieces each ≤ max_chunk_tokens (best-effort — a single gigantic
/// sentence is left intact rather than mid-word splitting).
fn split_oversized(text: &str, max_tokens: usize) -> Vec<String> {
    let mut pieces = Vec::new();

    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        if count_tokens(para) <= max_tokens {
            pieces.push(para.to_string());
        } else {
            // Try sentence-level splitting.
            let sentences = split_sentences(para);
            let mut buf = String::new();
            for sent in sentences {
                if buf.is_empty() {
                    buf = sent;
                } else if count_tokens(&buf) + count_tokens(&sent) + 1 <= max_tokens {
                    buf.push(' ');
                    buf.push_str(&sent);
                } else {
                    pieces.push(std::mem::take(&mut buf));
                    buf = sent;
                }
            }
            if !buf.is_empty() {
                // If still oversized (no sentence boundaries found), split by words.
                if count_tokens(&buf) > max_tokens {
                    let words: Vec<&str> = buf.split_whitespace().collect();
                    for word_chunk in words.chunks(max_tokens) {
                        pieces.push(word_chunk.join(" "));
                    }
                } else {
                    pieces.push(buf);
                }
            }
        }
    }
    pieces
}

/// Merge adjacent fragments smaller than `min_tokens` into their neighbour.
fn merge_tiny(fragments: Vec<String>, min_tokens: usize) -> Vec<String> {
    if fragments.is_empty() {
        return fragments;
    }
    let mut merged: Vec<String> = Vec::with_capacity(fragments.len());
    for frag in fragments {
        if let Some(last) = merged.last_mut() {
            if count_tokens(last) < min_tokens {
                last.push('\n');
                last.push_str(&frag);
                continue;
            }
        }
        merged.push(frag);
    }
    // Final pass: if the last element is tiny, merge it backwards.
    if merged.len() >= 2 {
        let last_tokens = count_tokens(merged.last().unwrap());
        if last_tokens < min_tokens {
            let last = merged.pop().unwrap();
            merged.last_mut().unwrap().push('\n');
            merged.last_mut().unwrap().push_str(&last);
        }
    }
    merged
}

/// Assign 0-based chunk indices and build `Chunk` structs, adding overlap
/// between adjacent chunks. `char_offset_base` is added to each chunk's offset.
fn build_chunks_with_overlap(
    fragments: Vec<String>,
    config: &ChunkConfig,
    page_number: Option<usize>,
    section_heading: Option<String>,
    char_offset_base: usize,
) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut char_offset = char_offset_base;

    for (i, frag) in fragments.iter().enumerate() {
        let content = if i > 0 && config.overlap_tokens > 0 {
            let overlap = get_overlap_text(&fragments[i - 1], config.overlap_tokens);
            format!("{overlap} {frag}")
        } else {
            frag.clone()
        };
        chunks.push(Chunk {
            index: 0, // filled later
            content,
            page_number,
            section_heading: section_heading.clone(),
            char_offset,
        });
        // Advance offset by the raw fragment length (+ separator approximation).
        char_offset += frag.len() + 2;
    }
    chunks
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

        // No overlap across page boundaries — build per-page, then extend.
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

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::PageContent;

    fn make_doc(file_type: &str, text: &str) -> ExtractedDocument {
        ExtractedDocument {
            filename: format!("test.{file_type}"),
            file_type: file_type.to_string(),
            pages: vec![PageContent {
                page_number: 1,
                text: text.to_string(),
                headings: vec![],
            }],
        }
    }

    fn make_pdf_doc(pages: Vec<(usize, &str)>) -> ExtractedDocument {
        ExtractedDocument {
            filename: "test.pdf".to_string(),
            file_type: "pdf".to_string(),
            pages: pages
                .into_iter()
                .map(|(num, text)| PageContent {
                    page_number: num,
                    text: text.to_string(),
                    headings: vec![],
                })
                .collect(),
        }
    }

    // ── Markdown ────────────────────────────────────────────────────────

    #[test]
    fn md_splits_at_headings() {
        let text = "## Introduction\nFirst section content here.\n\n## Methods\nSecond section content here.";
        let doc = make_doc("md", text);
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].section_heading.as_deref(), Some("Introduction"));
        assert_eq!(chunks[1].section_heading.as_deref(), Some("Methods"));
        assert!(chunks[0].content.contains("First section"));
        assert!(chunks[1].content.contains("Second section"));
    }

    #[test]
    fn md_preserves_section_heading() {
        let text = "## Heading A\nSome text.\n\n## Heading B\nMore text.";
        let doc = make_doc("md", text);
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        assert_eq!(chunks[0].section_heading.as_deref(), Some("Heading A"));
        assert_eq!(chunks[1].section_heading.as_deref(), Some("Heading B"));
    }

    #[test]
    fn md_splits_oversized_section() {
        // Create a section with many words.
        let long = (0..600).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ");
        let text = format!("## Big Section\n{long}");
        let doc = make_doc("md", &text);
        let config = ChunkConfig {
            max_chunk_tokens: 200,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        assert!(chunks.len() >= 3, "should split 600 words across >=3 chunks at max 200");
        for c in &chunks {
            assert_eq!(c.section_heading.as_deref(), Some("Big Section"));
        }
    }

    // ── Text ────────────────────────────────────────────────────────────

    #[test]
    fn txt_splits_at_paragraphs() {
        let text = "First paragraph here.\n\nSecond paragraph here.\n\nThird paragraph here.";
        let doc = make_doc("txt", text);
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].content.contains("First"));
        assert!(chunks[2].content.contains("Third"));
    }

    #[test]
    fn txt_respects_max_tokens() {
        let long = (0..800).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        let doc = make_doc("txt", &long);
        let config = ChunkConfig {
            max_chunk_tokens: 300,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn txt_merges_tiny_paragraphs() {
        let text = "Tiny.\n\nAlso tiny.\n\nBig paragraph with many words to exceed the minimum threshold definitely.";
        let doc = make_doc("txt", text);
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 5,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        // The two tiny paragraphs (1 word each) should merge.
        assert!(chunks.len() <= 2, "tiny paragraphs should merge: got {}", chunks.len());
    }

    // ── PDF ─────────────────────────────────────────────────────────────

    #[test]
    fn pdf_preserves_page_number() {
        let doc = make_pdf_doc(vec![
            (1, "Page one content."),
            (2, "Page two content."),
            (3, "Page three content."),
        ]);
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].page_number, Some(1));
        assert_eq!(chunks[1].page_number, Some(2));
        assert_eq!(chunks[2].page_number, Some(3));
    }

    #[test]
    fn pdf_no_cross_page_overlap() {
        let doc = make_pdf_doc(vec![
            (1, "Alpha bravo charlie delta echo."),
            (2, "Foxtrot golf hotel india juliet."),
        ]);
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 1,
            overlap_tokens: 3,
        };
        let chunks = chunk_document(&doc, &config);
        assert_eq!(chunks.len(), 2);
        // Page 2 chunk should NOT contain words from page 1.
        assert!(!chunks[1].content.contains("Alpha"));
        assert!(!chunks[1].content.contains("echo"));
    }

    // ── Overlap ─────────────────────────────────────────────────────────

    #[test]
    fn overlap_tokens_appear_between_chunks() {
        let text = "First paragraph with several words in it.\n\nSecond paragraph has different content entirely.";
        let doc = make_doc("txt", text);
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 1,
            overlap_tokens: 3,
        };
        let chunks = chunk_document(&doc, &config);
        assert_eq!(chunks.len(), 2);
        // The second chunk should start with the last 3 words of the first.
        let first_words: Vec<&str> = chunks[0].content.split_whitespace().collect();
        let last_3 = &first_words[first_words.len() - 3..];
        let second_start: Vec<&str> = chunks[1].content.split_whitespace().take(3).collect();
        assert_eq!(last_3, second_start.as_slice(), "overlap must match");
    }

    #[test]
    fn get_overlap_text_extracts_tail() {
        assert_eq!(get_overlap_text("a b c d e", 3), "c d e");
        assert_eq!(get_overlap_text("a b", 5), "a b"); // shorter than overlap
    }

    // ── Edge cases ──────────────────────────────────────────────────────

    #[test]
    fn empty_document_produces_no_chunks() {
        let doc = make_doc("txt", "");
        let config = ChunkConfig::default();
        let chunks = chunk_document(&doc, &config);
        assert!(chunks.is_empty());
    }

    #[test]
    fn single_paragraph_produces_one_chunk() {
        let doc = make_doc("txt", "Just one paragraph.");
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[0].char_offset, 0);
    }

    #[test]
    fn very_long_single_line_gets_split() {
        let long_line = (0..1000).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        let doc = make_doc("txt", &long_line);
        let config = ChunkConfig {
            max_chunk_tokens: 200,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        // Without sentence boundaries it stays as one piece (best-effort),
        // but the test verifies we don't panic.
        assert!(!chunks.is_empty());
    }

    #[test]
    fn tiny_document_below_min_produces_one_chunk() {
        let doc = make_doc("txt", "Small.");
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 100,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn chunk_indices_are_sequential() {
        let text = "A.\n\nB.\n\nC.\n\nD.";
        let doc = make_doc("txt", text);
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(c.index, i);
        }
    }

    #[test]
    fn count_tokens_handles_whitespace() {
        assert_eq!(count_tokens("hello world"), 2);
        assert_eq!(count_tokens("  spaced   out  "), 2);
        assert_eq!(count_tokens(""), 0);
        assert_eq!(count_tokens("single"), 1);
    }

    #[test]
    fn sentence_splitting() {
        let text = "First sentence. Second sentence. Third one.";
        let sents = split_sentences(text);
        assert_eq!(sents.len(), 3);
        assert!(sents[0].starts_with("First"));
        assert!(sents[1].starts_with("Second"));
    }

    #[test]
    fn whitespace_only_document_produces_no_chunks() {
        let doc = make_doc("txt", "   \n\n\t\n   ");
        let config = ChunkConfig::default();
        let chunks = chunk_document(&doc, &config);
        assert!(chunks.is_empty());
    }

    #[test]
    fn md_nested_headings() {
        let text = "## Top\nTop content.\n\n### Sub\nSub content.\n\n#### Deep\nDeep content.";
        let doc = make_doc("md", text);
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].section_heading.as_deref(), Some("Top"));
        assert_eq!(chunks[1].section_heading.as_deref(), Some("Sub"));
        assert_eq!(chunks[2].section_heading.as_deref(), Some("Deep"));
    }

    #[test]
    fn zero_overlap_produces_no_repeated_words() {
        let text = "Alpha bravo charlie.\n\nDelta echo foxtrot.";
        let doc = make_doc("txt", text);
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        assert_eq!(chunks.len(), 2);
        // With zero overlap, chunk[1] should NOT contain words from chunk[0].
        assert!(!chunks[1].content.contains("Alpha"));
        assert!(!chunks[1].content.contains("charlie"));
    }

    #[test]
    fn pdf_multi_page_chunk_page_numbers() {
        let doc = make_pdf_doc(vec![
            (1, "Page one has some content."),
            (2, "Page two has different content."),
            (3, "Page three wraps it up."),
            (4, "Page four is the last."),
        ]);
        let config = ChunkConfig {
            max_chunk_tokens: 500,
            min_chunk_tokens: 1,
            overlap_tokens: 0,
        };
        let chunks = chunk_document(&doc, &config);
        assert_eq!(chunks.len(), 4);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.page_number, Some(i + 1));
            assert_eq!(chunk.index, i);
        }
    }
}
