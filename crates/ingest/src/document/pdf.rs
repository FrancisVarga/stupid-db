use super::{ExtractionError, PageContent};
use std::process::Command;
use serde::Deserialize;

#[derive(Deserialize)]
struct OcrPage {
    page_number: usize,
    text: String,
}

#[derive(Deserialize)]
struct OcrResult {
    pages: Vec<OcrPage>,
}

/// Attempt OCR extraction using Python EasyOCR script.
/// Returns Ok(pages) if successful, Err if OCR unavailable or fails.
fn try_ocr_fallback(bytes: &[u8]) -> Result<Vec<PageContent>, ExtractionError> {
    // Write PDF bytes to temp file
    let temp_path = std::env::temp_dir().join(format!("ocr_{}.pdf", uuid::Uuid::new_v4()));
    std::fs::write(&temp_path, bytes)
        .map_err(|e| ExtractionError::PdfError(format!("Failed to write temp PDF: {e}")))?;

    // Call Python OCR script via uv
    let output = Command::new("uv")
        .args(["run", "python", "scripts/ocr_pdf.py"])
        .arg(&temp_path)
        .output()
        .map_err(|e| ExtractionError::PdfError(format!("Failed to run OCR script: {e}")))?;

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ExtractionError::PdfError(format!("OCR script failed: {stderr}")));
    }

    // Parse JSON output
    let result: OcrResult = serde_json::from_slice(&output.stdout)
        .map_err(|e| ExtractionError::PdfError(format!("Failed to parse OCR output: {e}")))?;

    // Convert to PageContent
    let pages: Vec<PageContent> = result
        .pages
        .into_iter()
        .filter(|p| !p.text.trim().is_empty())
        .map(|p| PageContent {
            page_number: p.page_number,
            text: p.text,
            headings: Vec::new(),
        })
        .collect();

    if pages.is_empty() {
        return Err(ExtractionError::PdfError(
            "OCR found no text (PDF may be blank or heavily corrupted)".to_string(),
        ));
    }

    Ok(pages)
}

pub fn extract_pdf(bytes: &[u8]) -> Result<Vec<PageContent>, ExtractionError> {
    let text = pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| ExtractionError::PdfError(e.to_string()))?;

    // pdf-extract returns all text as one string.
    // Split on form feed characters (\x0C) which typically separate pages.
    let trimmed = text.trim();
    if trimmed.is_empty() {
        // pdf-extract succeeded but no text found (scanned/image PDF)
        // Try OCR fallback
        match try_ocr_fallback(bytes) {
            Ok(pages) => {
                tracing::info!("PDF text extraction via OCR succeeded ({} pages)", pages.len());
                return Ok(pages);
            }
            Err(e) => {
                tracing::warn!("OCR fallback failed: {}", e);
                // Return empty pages (will trigger user-facing error in embedding.rs)
                return Ok(vec![PageContent {
                    page_number: 1,
                    text: String::new(),
                    headings: Vec::new(),
                }]);
            }
        }
    }

    let pages: Vec<PageContent> = if text.contains('\x0C') {
        text.split('\x0C')
            .enumerate()
            .filter(|(_, page_text)| !page_text.trim().is_empty())
            .map(|(i, page_text)| PageContent {
                page_number: i + 1,
                text: page_text.trim().to_string(),
                headings: Vec::new(),
            })
            .collect()
    } else {
        // No page breaks found — treat as single page
        vec![PageContent {
            page_number: 1,
            text: trimmed.to_string(),
            headings: Vec::new(),
        }]
    };

    Ok(pages)
}
