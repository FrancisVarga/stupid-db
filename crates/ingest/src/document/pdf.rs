use super::{ExtractionError, PageContent};

pub fn extract_pdf(bytes: &[u8]) -> Result<Vec<PageContent>, ExtractionError> {
    let text = pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| ExtractionError::PdfError(e.to_string()))?;

    // pdf-extract returns all text as one string.
    // Split on form feed characters (\x0C) which typically separate pages.
    let trimmed = text.trim();
    if trimmed.is_empty() {
        // pdf-extract succeeded but no text found (scanned/image PDF)
        return Ok(vec![PageContent {
            page_number: 1,
            text: String::new(),
            headings: Vec::new(),
        }]);
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
