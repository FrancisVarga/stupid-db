use super::{ExtractionError, PageContent};

pub fn extract_txt(bytes: &[u8]) -> Result<Vec<PageContent>, ExtractionError> {
    // Try UTF-8 first, fall back to lossy conversion
    let text = String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned());

    Ok(vec![PageContent {
        page_number: 1,
        text: text.trim().to_string(),
        headings: Vec::new(),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_simple_text() {
        let content = b"Hello, world!\nThis is a test file.";
        let pages = extract_txt(content).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].page_number, 1);
        assert!(pages[0].text.contains("Hello, world!"));
        assert!(pages[0].headings.is_empty());
    }

    #[test]
    fn extract_utf8_text() {
        let content = "Ünïcödé text with émojis 🎉".as_bytes();
        let pages = extract_txt(content).unwrap();
        assert_eq!(pages[0].text, "Ünïcödé text with émojis 🎉");
    }

    #[test]
    fn extract_empty_text() {
        let pages = extract_txt(b"").unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].text, "");
    }

    #[test]
    fn trims_whitespace() {
        let content = b"  \n  Hello  \n  ";
        let pages = extract_txt(content).unwrap();
        assert_eq!(pages[0].text, "Hello");
    }
}
