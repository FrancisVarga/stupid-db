//! Parse queue message JSON bodies into [`Document`]s.

use chrono::{DateTime, Utc};
use serde_json::Value;
use tracing::warn;
use uuid::Uuid;

use stupid_core::document::{Document, FieldValue};

use crate::consumer::QueueMessage;
use crate::error::QueueError;

/// Convert a JSON [`Value`] to our typed [`FieldValue`].
fn json_to_field_value(v: &Value) -> FieldValue {
    match v {
        Value::String(s) => FieldValue::Text(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                FieldValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                FieldValue::Float(f)
            } else {
                // Fallback: render as text
                FieldValue::Text(n.to_string())
            }
        }
        Value::Bool(b) => FieldValue::Boolean(*b),
        Value::Null => FieldValue::Null,
        // Arrays and objects: serialize back to JSON text
        other => FieldValue::Text(other.to_string()),
    }
}

/// Well-known field names for event type (tried in order).
const EVENT_TYPE_KEYS: &[&str] = &[
    "event_type", "eventType", "EventType",
    "type", "Type",
    "action", "Action",
    "event", "Event",
];

/// Well-known field names for document ID.
const ID_KEYS: &[&str] = &["id", "Id", "ID", "uuid", "messageId"];

/// Well-known field names for timestamp.
const TIMESTAMP_KEYS: &[&str] = &[
    "timestamp", "Timestamp",
    "created_at", "createdAt",
    "time", "date", "datetime",
    "occurred_at", "occurredAt",
    "sent_at", "sentAt",
];

/// Parse a single queue message body into a [`Document`].
///
/// Dynamically adapts to any JSON object schema:
/// - **event_type**: probes common field names; defaults to `"Unknown"` if none found
/// - **id**: probes common ID fields; generates a UUID if missing
/// - **timestamp**: probes common time fields; falls back to SQS message timestamp
/// - **fields**: all key-value pairs are preserved as typed [`FieldValue`]s
///
/// Only rejects messages that are not valid JSON or not a JSON object.
pub fn parse_message(msg: &QueueMessage) -> Result<Document, QueueError> {
    let json: Value = serde_json::from_str(&msg.body)
        .map_err(|e| QueueError::Parse(format!("Invalid JSON in message {}: {}", msg.id, e)))?;

    let obj = json
        .as_object()
        .ok_or_else(|| QueueError::Parse(format!("Message {} body is not a JSON object", msg.id)))?;

    // event_type: probe well-known keys, default to "Unknown"
    let event_type = EVENT_TYPE_KEYS
        .iter()
        .find_map(|k| obj.get(*k).and_then(|v| v.as_str()))
        .unwrap_or("Unknown")
        .to_string();

    // id: probe well-known keys, fall back to generated UUID
    let id = ID_KEYS
        .iter()
        .find_map(|k| obj.get(*k).and_then(|v| v.as_str()))
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::new_v4);

    // timestamp: probe well-known keys, fall back to SQS message timestamp
    let timestamp = TIMESTAMP_KEYS
        .iter()
        .find_map(|k| obj.get(*k).and_then(|v| v.as_str()))
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or(msg.timestamp);

    // All fields go into the map (except the extracted id key, to avoid duplication)
    let id_key_used = ID_KEYS.iter().find(|k| obj.contains_key(**k));
    let fields = obj
        .iter()
        .filter(|(k, _)| id_key_used.map_or(true, |ik| k != ik))
        .map(|(k, v)| (k.clone(), json_to_field_value(v)))
        .collect();

    Ok(Document {
        id,
        event_type,
        timestamp,
        fields,
    })
}

/// Parse a batch of messages, separating successes from failures.
///
/// Returns `(documents, errors)`. Good messages are never blocked by bad ones,
/// allowing partial batch processing.
pub fn parse_batch(messages: &[QueueMessage]) -> (Vec<Document>, Vec<(String, QueueError)>) {
    let mut docs = Vec::with_capacity(messages.len());
    let mut errors = Vec::new();

    for msg in messages {
        match parse_message(msg) {
            Ok(doc) => docs.push(doc),
            Err(e) => {
                warn!(message_id = %msg.id, error = %e, "Failed to parse queue message");
                errors.push((msg.id.clone(), e));
            }
        }
    }

    (docs, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    /// Helper: build a QueueMessage with the given JSON body.
    fn make_msg(id: &str, body: &str) -> QueueMessage {
        QueueMessage {
            id: id.to_string(),
            body: body.to_string(),
            receipt_handle: "handle-test".to_string(),
            timestamp: Utc::now(),
            attempt_count: 1,
        }
    }

    #[test]
    fn test_parse_login_event() {
        let body = r#"{
            "event_type": "Login",
            "memberCode": "M001",
            "fingerprint": "fp-abc123",
            "platform": "web",
            "ip": "192.168.1.1"
        }"#;
        let msg = make_msg("msg-1", body);
        let doc = parse_message(&msg).unwrap();

        assert_eq!(doc.event_type, "Login");
        assert_eq!(doc.fields.get("memberCode"), Some(&FieldValue::Text("M001".into())));
        assert_eq!(doc.fields.get("fingerprint"), Some(&FieldValue::Text("fp-abc123".into())));
        assert_eq!(doc.fields.get("platform"), Some(&FieldValue::Text("web".into())));
    }

    #[test]
    fn test_parse_game_opened_event() {
        let body = r#"{
            "event_type": "GameOpened",
            "game": "slots-fortune",
            "platform": "mobile",
            "betAmount": 100
        }"#;
        let msg = make_msg("msg-2", body);
        let doc = parse_message(&msg).unwrap();

        assert_eq!(doc.event_type, "GameOpened");
        assert_eq!(doc.fields.get("game"), Some(&FieldValue::Text("slots-fortune".into())));
        assert_eq!(doc.fields.get("betAmount"), Some(&FieldValue::Integer(100)));
    }

    #[test]
    fn test_parse_popup_module_event() {
        let body = r#"{
            "event_type": "PopupModule",
            "trackingId": "trk-xyz",
            "popupType": "promotion",
            "dismissed": true
        }"#;
        let msg = make_msg("msg-3", body);
        let doc = parse_message(&msg).unwrap();

        assert_eq!(doc.event_type, "PopupModule");
        assert_eq!(doc.fields.get("trackingId"), Some(&FieldValue::Text("trk-xyz".into())));
        assert_eq!(doc.fields.get("popupType"), Some(&FieldValue::Text("promotion".into())));
        assert_eq!(doc.fields.get("dismissed"), Some(&FieldValue::Boolean(true)));
    }

    #[test]
    fn test_parse_missing_event_type_defaults_to_unknown() {
        let body = r#"{"memberCode": "M001"}"#;
        let msg = make_msg("msg-no-type", body);
        let doc = parse_message(&msg).unwrap();

        assert_eq!(doc.event_type, "Unknown");
        assert_eq!(doc.fields.get("memberCode"), Some(&FieldValue::Text("M001".into())));
    }

    #[test]
    fn test_parse_alternate_type_field_names() {
        // "eventType" (camelCase)
        let msg = make_msg("msg-camel", r#"{"eventType":"Login","x":1}"#);
        assert_eq!(parse_message(&msg).unwrap().event_type, "Login");

        // "type"
        let msg = make_msg("msg-type", r#"{"type":"Signup","x":1}"#);
        assert_eq!(parse_message(&msg).unwrap().event_type, "Signup");

        // "action"
        let msg = make_msg("msg-action", r#"{"action":"Click","x":1}"#);
        assert_eq!(parse_message(&msg).unwrap().event_type, "Click");
    }

    #[test]
    fn test_parse_invalid_json() {
        let msg = make_msg("msg-bad-json", "not json at all");
        let err = parse_message(&msg).unwrap_err();

        assert!(matches!(err, QueueError::Parse(_)));
        assert!(err.to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_parse_batch_partial_success() {
        let messages = vec![
            make_msg("good-1", r#"{"event_type":"Login","memberCode":"M001"}"#),
            make_msg("bad-1", "invalid json"),
            make_msg("good-2", r#"{"event_type":"GameOpened","game":"slots"}"#),
            make_msg("no-type", r#"{"foo":"bar"}"#),
            make_msg("bad-2", "not an object"),
        ];

        let (docs, errors) = parse_batch(&messages);

        assert_eq!(docs.len(), 3); // good-1, good-2, no-type (defaults to Unknown)
        assert_eq!(errors.len(), 2); // bad-1 (invalid json), bad-2 (invalid json)
        assert_eq!(docs[0].event_type, "Login");
        assert_eq!(docs[1].event_type, "GameOpened");
        assert_eq!(docs[2].event_type, "Unknown");
    }

    #[test]
    fn test_parse_extra_fields_preserved() {
        let body = r#"{
            "event_type": "CustomEvent",
            "knownField": "hello",
            "unknownField1": 42,
            "unknownField2": 3.14,
            "unknownNull": null,
            "nestedObj": {"a": 1}
        }"#;
        let msg = make_msg("msg-extra", body);
        let doc = parse_message(&msg).unwrap();

        assert_eq!(doc.fields.get("knownField"), Some(&FieldValue::Text("hello".into())));
        assert_eq!(doc.fields.get("unknownField1"), Some(&FieldValue::Integer(42)));
        assert_eq!(doc.fields.get("unknownField2"), Some(&FieldValue::Float(3.14)));
        assert_eq!(doc.fields.get("unknownNull"), Some(&FieldValue::Null));
        // Nested objects serialize to JSON text
        assert!(matches!(doc.fields.get("nestedObj"), Some(FieldValue::Text(_))));
    }

    #[test]
    fn test_parse_explicit_id_and_timestamp() {
        let body = r#"{
            "event_type": "Login",
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "timestamp": "2025-06-14T12:00:00Z"
        }"#;
        let msg = make_msg("msg-explicit", body);
        let doc = parse_message(&msg).unwrap();

        assert_eq!(
            doc.id,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
        );
        assert_eq!(doc.timestamp.to_rfc3339().starts_with("2025-06-14"), true);
        // `id` should be excluded from fields, but timestamp stays
        assert!(doc.fields.get("id").is_none());
        assert!(doc.fields.get("timestamp").is_some());
    }
}
