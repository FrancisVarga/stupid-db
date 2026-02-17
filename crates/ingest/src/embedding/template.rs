use stupid_core::document::Document;

/// Convert a document to its text representation for embedding.
/// Templates match docs/ingestion/embedding.md exactly.
pub fn document_to_text(doc: &Document) -> String {
    match doc.event_type.as_str() {
        "Login" => format_login(doc),
        "GameOpened" => format_game_opened(doc),
        "APIError" => format_api_error(doc),
        "PopupModule" => format_popup(doc),
        _ => format_generic(doc),
    }
}

fn field_str(doc: &Document, key: &str) -> String {
    doc.fields
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn format_login(doc: &Document) -> String {
    format!(
        "Login member:{} platform:{} currency:{} vip:{} success:{} method:{} device:{}",
        field_str(doc, "memberCode"),
        field_str(doc, "platform"),
        field_str(doc, "currency"),
        field_str(doc, "rGroup"),
        field_str(doc, "success"),
        field_str(doc, "method"),
        field_str(doc, "device"),
    )
}

fn format_game_opened(doc: &Document) -> String {
    format!(
        "GameOpened member:{} game:{} category:{} provider:{} platform:{} from:{} currency:{} vip:{}",
        field_str(doc, "memberCode"),
        field_str(doc, "game"),
        field_str(doc, "category"),
        field_str(doc, "gameTrackingProvider"),
        field_str(doc, "platform"),
        field_str(doc, "from"),
        field_str(doc, "currency"),
        field_str(doc, "rGroup"),
    )
}

fn format_api_error(doc: &Document) -> String {
    let error_raw = field_str(doc, "error");
    let error_truncated: String = error_raw.chars().take(100).collect();

    format!(
        "APIError stage:{} error:{} platform:{} page:{} status:{} member:{}",
        field_str(doc, "stage"),
        error_truncated,
        field_str(doc, "platform"),
        field_str(doc, "page"),
        field_str(doc, "status"),
        field_str(doc, "memberCode"),
    )
}

fn format_popup(doc: &Document) -> String {
    format!(
        "Popup member:{} type:{} click:{} component:{} game:{} platform:{}",
        field_str(doc, "memberCode"),
        field_str(doc, "popupType"),
        field_str(doc, "clickType"),
        field_str(doc, "componentId"),
        field_str(doc, "game"),
        field_str(doc, "platform"),
    )
}

fn format_generic(doc: &Document) -> String {
    let mut parts = vec![doc.event_type.clone()];
    let mut keys: Vec<&String> = doc.fields.keys().collect();
    keys.sort();
    for key in keys {
        if let Some(val) = doc.fields.get(key) {
            if let Some(s) = val.as_str() {
                parts.push(format!("{key}:{s}"));
            }
        }
    }
    parts.join(" ")
}

// ── Config-driven embedding templates ─────────────────────────────

use stupid_rules::entity_schema::{CompiledEntitySchema, TemplateSegment};

/// Convert a document to text using compiled embedding templates from the EntitySchema.
///
/// Looks up the event type (including aliases) in the schema's embedding_templates.
/// Falls back to the generic format if no template is found.
pub fn document_to_text_with_schema(doc: &Document, schema: &CompiledEntitySchema) -> String {
    if let Some(segments) = schema.embedding_templates.get(doc.event_type.as_str()) {
        render_template(doc, segments)
    } else {
        format_generic(doc)
    }
}

/// Render a pre-parsed template against a document's fields.
fn render_template(doc: &Document, segments: &[TemplateSegment]) -> String {
    let mut output = String::new();
    for segment in segments {
        match segment {
            TemplateSegment::Literal(text) => output.push_str(text),
            TemplateSegment::Field(name) => {
                output.push_str(&field_str(doc, name));
            }
            TemplateSegment::FieldTruncated(name, max_len) => {
                let val = field_str(doc, name);
                let truncated: String = val.chars().take(*max_len).collect();
                output.push_str(&truncated);
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;
    use stupid_core::document::{FieldValue, Document};
    use uuid::Uuid;

    fn make_doc(event_type: &str, fields: Vec<(&str, &str)>) -> Document {
        let mut map = HashMap::new();
        for (k, v) in fields {
            map.insert(k.to_string(), FieldValue::Text(v.to_string()));
        }
        Document {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: event_type.to_string(),
            fields: map,
        }
    }

    #[test]
    fn test_login_template() {
        let doc = make_doc("Login", vec![
            ("memberCode", "USR001"),
            ("platform", "mobile"),
            ("currency", "USD"),
            ("rGroup", "VIP3"),
            ("success", "true"),
            ("method", "password"),
            ("device", "iPhone"),
        ]);
        let text = document_to_text(&doc);
        assert!(text.starts_with("Login member:USR001"));
        assert!(text.contains("platform:mobile"));
        assert!(text.contains("vip:VIP3"));
    }

    #[test]
    fn test_api_error_truncation() {
        let long_error = "x".repeat(200);
        let doc = make_doc("APIError", vec![
            ("error", &long_error),
            ("stage", "auth"),
            ("platform", "web"),
            ("page", "/login"),
            ("status", "500"),
            ("memberCode", "USR002"),
        ]);
        let text = document_to_text(&doc);
        // Error should be truncated to 100 chars
        let error_part: &str = text.split("error:").nth(1).unwrap().split(" platform:").next().unwrap();
        assert_eq!(error_part.len(), 100);
    }

    #[test]
    fn test_generic_skips_null() {
        let mut fields = HashMap::new();
        fields.insert("alpha".to_string(), FieldValue::Text("a".to_string()));
        fields.insert("beta".to_string(), FieldValue::Null);
        fields.insert("gamma".to_string(), FieldValue::Text("g".to_string()));
        let doc = Document {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: "Custom".to_string(),
            fields,
        };
        let text = document_to_text(&doc);
        assert!(text.contains("alpha:a"));
        assert!(!text.contains("beta"));
        assert!(text.contains("gamma:g"));
    }
}
