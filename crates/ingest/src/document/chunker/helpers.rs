//! Text splitting and merging utilities used by chunking strategies.

use super::types::{Chunk, ChunkConfig};

/// Approximate token count via whitespace splitting.
pub(crate) fn count_tokens(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Extract the last `overlap_tokens` words from `text`.
pub(crate) fn get_overlap_text(text: &str, overlap_tokens: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= overlap_tokens {
        return text.to_string();
    }
    words[words.len() - overlap_tokens..].join(" ")
}

/// Split `text` at sentence boundaries (`. `, `! `, `? ` followed by uppercase
/// or newline). Returns non-empty fragments.
pub(crate) fn split_sentences(text: &str) -> Vec<String> {
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
/// Returns pieces each <= max_chunk_tokens (best-effort -- a single gigantic
/// sentence is left intact rather than mid-word splitting).
pub(crate) fn split_oversized(text: &str, max_tokens: usize) -> Vec<String> {
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
pub(crate) fn merge_tiny(fragments: Vec<String>, min_tokens: usize) -> Vec<String> {
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
pub(crate) fn build_chunks_with_overlap(
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
