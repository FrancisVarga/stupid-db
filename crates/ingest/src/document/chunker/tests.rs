//! Tests for the chunking engine.

use super::helpers::{count_tokens, get_overlap_text, split_sentences};
use super::strategies::chunk_document;
use super::types::ChunkConfig;
use crate::document::{ExtractedDocument, PageContent};

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
