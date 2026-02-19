use stupid_core::{Document, FieldValue};

use super::types::EventTypeCompressed;

/// Compress a document's event type into a short code.
///
/// - Login -> "L"
/// - GameOpened -> "G:S" (with game subtype) or "G"
/// - PopupModule -> "P:click" (with action) or "P"
/// - API Error -> "E:auth" (with error category) or "E"
/// - Other -> first 3 chars
pub fn compress_event(doc: &Document) -> EventTypeCompressed {
    let get = |name: &str| -> Option<&str> {
        doc.fields.get(name).and_then(FieldValue::as_str).filter(|s| !s.is_empty())
    };

    let code = match doc.event_type.as_str() {
        "Login" => "L".to_string(),
        "GameOpened" | "GridClick" => {
            if let Some(game) = get("game").or_else(|| get("gameName")) {
                // Use first word or short identifier
                let short = game.split_whitespace().next().unwrap_or(game);
                let truncated = if short.len() > 8 { &short[..8] } else { short };
                format!("G:{}", truncated)
            } else {
                "G".to_string()
            }
        }
        "PopupModule" | "PopUpModule" => {
            if let Some(action) = get("action").or_else(|| get("popupType")) {
                let short = if action.len() > 8 { &action[..8] } else { action };
                format!("P:{}", short)
            } else {
                "P".to_string()
            }
        }
        "API Error" => {
            if let Some(code) = get("statusCode") {
                format!("E:{}", code)
            } else if let Some(url) = get("url") {
                let short = url.split('/').last().unwrap_or("unknown");
                let truncated = if short.len() > 8 { &short[..8] } else { short };
                format!("E:{}", truncated)
            } else {
                "E".to_string()
            }
        }
        other => {
            let short = if other.len() > 3 { &other[..3] } else { other };
            short.to_string()
        }
    };

    EventTypeCompressed(code)
}

/// Compress an event using a compiled FeatureConfig's event compression rules.
///
/// Looks up `doc.event_type` in `config.event_compression` for the compression
/// code and optional subtype field. Falls back to first 3 characters for
/// unknown event types.
pub fn compress_event_with_config(
    doc: &Document,
    config: &stupid_rules::feature_config::CompiledFeatureConfig,
) -> EventTypeCompressed {
    let get = |name: &str| -> Option<&str> {
        doc.fields.get(name).and_then(FieldValue::as_str).filter(|s| !s.is_empty())
    };

    if let Some(rule) = config.event_compression.get(doc.event_type.as_str()) {
        let code = if let Some(ref field) = rule.subtype_field {
            if let Some(subtype) = get(field) {
                let truncated = if subtype.len() > 8 { &subtype[..8] } else { subtype };
                format!("{}:{}", rule.code, truncated)
            } else {
                rule.code.clone()
            }
        } else {
            rule.code.clone()
        };
        EventTypeCompressed(code)
    } else {
        // Fallback for unknown event types.
        let short = if doc.event_type.len() > 3 { &doc.event_type[..3] } else { &doc.event_type };
        EventTypeCompressed(short.to_string())
    }
}
