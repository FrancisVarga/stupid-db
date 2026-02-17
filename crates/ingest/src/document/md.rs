use super::{ExtractionError, PageContent};

pub fn extract_md(bytes: &[u8]) -> Result<Vec<PageContent>, ExtractionError> {
    let text = String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned());

    // Extract headings (lines starting with #)
    let headings: Vec<String> = text
        .lines()
        .filter(|line| line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .collect();

    Ok(vec![PageContent {
        page_number: 1,
        text: text.trim().to_string(),
        headings,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_headings() {
        let content = b"# Title\n\nSome text.\n\n## Section 1\n\nMore text.\n\n### Subsection\n";
        let pages = extract_md(content).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].headings, vec!["Title", "Section 1", "Subsection"]);
    }

    #[test]
    fn preserves_full_content() {
        let content = b"# Hello\n\nParagraph one.\n\n## World\n\nParagraph two.";
        let pages = extract_md(content).unwrap();
        assert!(pages[0].text.contains("Paragraph one."));
        assert!(pages[0].text.contains("Paragraph two."));
    }

    #[test]
    fn no_headings() {
        let content = b"Just plain text without any headings.";
        let pages = extract_md(content).unwrap();
        assert!(pages[0].headings.is_empty());
        assert_eq!(pages[0].text, "Just plain text without any headings.");
    }

    #[test]
    fn empty_markdown() {
        let pages = extract_md(b"").unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].text, "");
        assert!(pages[0].headings.is_empty());
    }
}
