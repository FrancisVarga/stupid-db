pub mod chunker;
mod md;
mod pdf;
mod txt;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExtractionError {
    #[error("Unsupported file type: {0}")]
    UnsupportedType(String),
    #[error("PDF extraction failed: {0}")]
    PdfError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// A page of extracted text with metadata.
#[derive(Debug, Clone)]
pub struct PageContent {
    /// 1-based page number (for PDFs). For TXT/MD, always 1.
    pub page_number: usize,
    /// The extracted text content.
    pub text: String,
    /// Headings found on this page (for MD files).
    pub headings: Vec<String>,
}

/// Result of extracting text from a document.
#[derive(Debug, Clone)]
pub struct ExtractedDocument {
    /// Original filename.
    pub filename: String,
    /// File type: "pdf", "txt", "md"
    pub file_type: String,
    /// Extracted pages with text and metadata.
    pub pages: Vec<PageContent>,
}

impl ExtractedDocument {
    /// Get all text concatenated.
    pub fn full_text(&self) -> String {
        self.pages
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Total character count across all pages.
    pub fn total_chars(&self) -> usize {
        self.pages.iter().map(|p| p.text.len()).sum()
    }
}

/// Extract text from file bytes based on file type.
pub fn extract_text(bytes: &[u8], filename: &str) -> Result<ExtractedDocument, ExtractionError> {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    let file_type = ext.as_str();

    let pages = match file_type {
        "pdf" => pdf::extract_pdf(bytes)?,
        "txt" | "text" => txt::extract_txt(bytes)?,
        "md" | "markdown" => md::extract_md(bytes)?,
        other => return Err(ExtractionError::UnsupportedType(other.to_string())),
    };

    Ok(ExtractedDocument {
        filename: filename.to_string(),
        file_type: file_type.to_string(),
        pages,
    })
}
