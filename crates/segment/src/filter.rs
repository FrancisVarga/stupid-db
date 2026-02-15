use chrono::{DateTime, Utc};
use stupid_core::{Document, FieldValue};

/// Field-level predicate for filtering documents.
#[derive(Debug, Clone)]
pub enum FieldPredicate {
    /// Field equals the specified string value.
    Eq(String),
    /// Field contains the specified substring (text only).
    Contains(String),
    /// Field is greater than the specified numeric value.
    Gt(f64),
    /// Field is less than the specified numeric value.
    Lt(f64),
}

impl FieldPredicate {
    /// Test if a field value matches this predicate.
    pub fn matches(&self, value: &FieldValue) -> bool {
        match self {
            FieldPredicate::Eq(expected) => match value {
                FieldValue::Text(s) => s == expected,
                FieldValue::Integer(i) => expected.parse::<i64>().ok() == Some(*i),
                FieldValue::Float(f) => expected.parse::<f64>().ok() == Some(*f),
                FieldValue::Boolean(b) => {
                    expected.eq_ignore_ascii_case("true") && *b
                        || expected.eq_ignore_ascii_case("false") && !*b
                }
                FieldValue::Null => false,
            },
            FieldPredicate::Contains(substring) => match value {
                FieldValue::Text(s) => s.contains(substring),
                _ => false,
            },
            FieldPredicate::Gt(threshold) => match value {
                FieldValue::Integer(i) => (*i as f64) > *threshold,
                FieldValue::Float(f) => *f > *threshold,
                _ => false,
            },
            FieldPredicate::Lt(threshold) => match value {
                FieldValue::Integer(i) => (*i as f64) < *threshold,
                FieldValue::Float(f) => *f < *threshold,
                _ => false,
            },
        }
    }
}

/// Filter specification for scanning documents.
#[derive(Debug, Clone)]
pub struct ScanFilter {
    /// Filter documents with timestamp >= this value.
    pub time_start: Option<DateTime<Utc>>,
    /// Filter documents with timestamp <= this value.
    pub time_end: Option<DateTime<Utc>>,
    /// Filter by event type (exact match).
    pub event_type: Option<String>,
    /// Field-level predicates (all must match).
    pub field_filters: Vec<(String, FieldPredicate)>,
}

impl ScanFilter {
    /// Create an empty filter that matches all documents.
    pub fn new() -> Self {
        Self {
            time_start: None,
            time_end: None,
            event_type: None,
            field_filters: Vec::new(),
        }
    }

    /// Create a filter with a time range (inclusive).
    pub fn time_range(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self {
            time_start: Some(start),
            time_end: Some(end),
            event_type: None,
            field_filters: Vec::new(),
        }
    }

    /// Set the start of the time range (inclusive).
    pub fn time_start(mut self, start: DateTime<Utc>) -> Self {
        self.time_start = Some(start);
        self
    }

    /// Set the end of the time range (inclusive).
    pub fn time_end(mut self, end: DateTime<Utc>) -> Self {
        self.time_end = Some(end);
        self
    }

    /// Filter by event type.
    pub fn event_type(mut self, event_type: impl Into<String>) -> Self {
        self.event_type = Some(event_type.into());
        self
    }

    /// Add a field equality predicate.
    pub fn field_eq(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.field_filters
            .push((name.into(), FieldPredicate::Eq(value.into())));
        self
    }

    /// Add a field substring predicate.
    pub fn field_contains(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.field_filters
            .push((name.into(), FieldPredicate::Contains(value.into())));
        self
    }

    /// Add a field greater-than predicate.
    pub fn field_gt(mut self, name: impl Into<String>, value: f64) -> Self {
        self.field_filters
            .push((name.into(), FieldPredicate::Gt(value)));
        self
    }

    /// Add a field less-than predicate.
    pub fn field_lt(mut self, name: impl Into<String>, value: f64) -> Self {
        self.field_filters
            .push((name.into(), FieldPredicate::Lt(value)));
        self
    }

    /// Test if a document matches this filter.
    pub fn matches(&self, doc: &Document) -> bool {
        // Time range check
        if let Some(start) = &self.time_start {
            if doc.timestamp < *start {
                return false;
            }
        }
        if let Some(end) = &self.time_end {
            if doc.timestamp > *end {
                return false;
            }
        }

        // Event type check
        if let Some(ref event_type) = self.event_type {
            if &doc.event_type != event_type {
                return false;
            }
        }

        // Field filters (all must match)
        for (field_name, predicate) in &self.field_filters {
            match doc.fields.get(field_name) {
                Some(value) => {
                    if !predicate.matches(value) {
                        return false;
                    }
                }
                None => return false, // Field not present = filter fails
            }
        }

        true
    }
}

impl Default for ScanFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn make_doc(event_type: &str, ts: DateTime<Utc>, fields: Vec<(&str, FieldValue)>) -> Document {
        Document {
            id: Uuid::new_v4(),
            timestamp: ts,
            event_type: event_type.to_string(),
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }

    #[test]
    fn test_predicate_eq_text() {
        let pred = FieldPredicate::Eq("alice".to_string());
        assert!(pred.matches(&FieldValue::Text("alice".to_string())));
        assert!(!pred.matches(&FieldValue::Text("bob".to_string())));
        assert!(!pred.matches(&FieldValue::Null));
    }

    #[test]
    fn test_predicate_eq_integer() {
        let pred = FieldPredicate::Eq("42".to_string());
        assert!(pred.matches(&FieldValue::Integer(42)));
        assert!(!pred.matches(&FieldValue::Integer(99)));
    }

    #[test]
    fn test_predicate_eq_boolean() {
        let pred_true = FieldPredicate::Eq("true".to_string());
        assert!(pred_true.matches(&FieldValue::Boolean(true)));
        assert!(!pred_true.matches(&FieldValue::Boolean(false)));

        let pred_false = FieldPredicate::Eq("FALSE".to_string());
        assert!(pred_false.matches(&FieldValue::Boolean(false)));
        assert!(!pred_false.matches(&FieldValue::Boolean(true)));
    }

    #[test]
    fn test_predicate_contains() {
        let pred = FieldPredicate::Contains("alice".to_string());
        assert!(pred.matches(&FieldValue::Text("hello alice world".to_string())));
        assert!(!pred.matches(&FieldValue::Text("hello bob".to_string())));
        assert!(!pred.matches(&FieldValue::Integer(42)));
    }

    #[test]
    fn test_predicate_gt() {
        let pred = FieldPredicate::Gt(10.0);
        assert!(pred.matches(&FieldValue::Float(15.5)));
        assert!(pred.matches(&FieldValue::Integer(20)));
        assert!(!pred.matches(&FieldValue::Float(5.0)));
        assert!(!pred.matches(&FieldValue::Integer(10)));
        assert!(!pred.matches(&FieldValue::Text("20".to_string())));
    }

    #[test]
    fn test_predicate_lt() {
        let pred = FieldPredicate::Lt(10.0);
        assert!(pred.matches(&FieldValue::Float(5.5)));
        assert!(pred.matches(&FieldValue::Integer(3)));
        assert!(!pred.matches(&FieldValue::Float(15.0)));
        assert!(!pred.matches(&FieldValue::Integer(10)));
    }

    #[test]
    fn test_filter_empty_matches_all() {
        let filter = ScanFilter::new();
        let doc = make_doc("Login", Utc::now(), vec![]);
        assert!(filter.matches(&doc));
    }

    #[test]
    fn test_filter_time_start() {
        let start = Utc.with_ymd_and_hms(2025, 6, 14, 12, 0, 0).unwrap();
        let filter = ScanFilter::new().time_start(start);

        let doc_before = make_doc(
            "Login",
            Utc.with_ymd_and_hms(2025, 6, 14, 11, 59, 59).unwrap(),
            vec![],
        );
        let doc_after = make_doc(
            "Login",
            Utc.with_ymd_and_hms(2025, 6, 14, 12, 0, 1).unwrap(),
            vec![],
        );

        assert!(!filter.matches(&doc_before));
        assert!(filter.matches(&doc_after));
    }

    #[test]
    fn test_filter_time_end() {
        let end = Utc.with_ymd_and_hms(2025, 6, 14, 12, 0, 0).unwrap();
        let filter = ScanFilter::new().time_end(end);

        let doc_before = make_doc(
            "Login",
            Utc.with_ymd_and_hms(2025, 6, 14, 11, 59, 59).unwrap(),
            vec![],
        );
        let doc_after = make_doc(
            "Login",
            Utc.with_ymd_and_hms(2025, 6, 14, 12, 0, 1).unwrap(),
            vec![],
        );

        assert!(filter.matches(&doc_before));
        assert!(!filter.matches(&doc_after));
    }

    #[test]
    fn test_filter_time_range() {
        let start = Utc.with_ymd_and_hms(2025, 6, 14, 12, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, 6, 14, 13, 0, 0).unwrap();
        let filter = ScanFilter::time_range(start, end);

        let doc_before = make_doc(
            "Login",
            Utc.with_ymd_and_hms(2025, 6, 14, 11, 0, 0).unwrap(),
            vec![],
        );
        let doc_inside = make_doc(
            "Login",
            Utc.with_ymd_and_hms(2025, 6, 14, 12, 30, 0).unwrap(),
            vec![],
        );
        let doc_after = make_doc(
            "Login",
            Utc.with_ymd_and_hms(2025, 6, 14, 14, 0, 0).unwrap(),
            vec![],
        );

        assert!(!filter.matches(&doc_before));
        assert!(filter.matches(&doc_inside));
        assert!(!filter.matches(&doc_after));
    }

    #[test]
    fn test_filter_event_type() {
        let filter = ScanFilter::new().event_type("Login");

        let login_doc = make_doc("Login", Utc::now(), vec![]);
        let game_doc = make_doc("GameOpened", Utc::now(), vec![]);

        assert!(filter.matches(&login_doc));
        assert!(!filter.matches(&game_doc));
    }

    #[test]
    fn test_filter_field_eq() {
        let filter = ScanFilter::new().field_eq("member", "alice");

        let alice_doc = make_doc(
            "Login",
            Utc::now(),
            vec![("member", FieldValue::Text("alice".to_string()))],
        );
        let bob_doc = make_doc(
            "Login",
            Utc::now(),
            vec![("member", FieldValue::Text("bob".to_string()))],
        );
        let missing_doc = make_doc("Login", Utc::now(), vec![]);

        assert!(filter.matches(&alice_doc));
        assert!(!filter.matches(&bob_doc));
        assert!(!filter.matches(&missing_doc));
    }

    #[test]
    fn test_filter_field_contains() {
        let filter = ScanFilter::new().field_contains("message", "error");

        let error_doc = make_doc(
            "Log",
            Utc::now(),
            vec![("message", FieldValue::Text("an error occurred".to_string()))],
        );
        let info_doc = make_doc(
            "Log",
            Utc::now(),
            vec![("message", FieldValue::Text("info message".to_string()))],
        );

        assert!(filter.matches(&error_doc));
        assert!(!filter.matches(&info_doc));
    }

    #[test]
    fn test_filter_field_gt() {
        let filter = ScanFilter::new().field_gt("score", 100.0);

        let high_doc = make_doc("Game", Utc::now(), vec![("score", FieldValue::Integer(150))]);
        let low_doc = make_doc("Game", Utc::now(), vec![("score", FieldValue::Integer(50))]);

        assert!(filter.matches(&high_doc));
        assert!(!filter.matches(&low_doc));
    }

    #[test]
    fn test_filter_field_lt() {
        let filter = ScanFilter::new().field_lt("score", 100.0);

        let high_doc = make_doc("Game", Utc::now(), vec![("score", FieldValue::Integer(150))]);
        let low_doc = make_doc("Game", Utc::now(), vec![("score", FieldValue::Integer(50))]);

        assert!(!filter.matches(&high_doc));
        assert!(filter.matches(&low_doc));
    }

    #[test]
    fn test_filter_multiple_predicates() {
        let filter = ScanFilter::new()
            .event_type("Login")
            .field_eq("member", "alice")
            .field_gt("score", 50.0);

        let matching_doc = make_doc(
            "Login",
            Utc::now(),
            vec![
                ("member", FieldValue::Text("alice".to_string())),
                ("score", FieldValue::Integer(100)),
            ],
        );

        let wrong_event = make_doc(
            "Logout",
            Utc::now(),
            vec![
                ("member", FieldValue::Text("alice".to_string())),
                ("score", FieldValue::Integer(100)),
            ],
        );

        let wrong_member = make_doc(
            "Login",
            Utc::now(),
            vec![
                ("member", FieldValue::Text("bob".to_string())),
                ("score", FieldValue::Integer(100)),
            ],
        );

        let low_score = make_doc(
            "Login",
            Utc::now(),
            vec![
                ("member", FieldValue::Text("alice".to_string())),
                ("score", FieldValue::Integer(30)),
            ],
        );

        assert!(filter.matches(&matching_doc));
        assert!(!filter.matches(&wrong_event));
        assert!(!filter.matches(&wrong_member));
        assert!(!filter.matches(&low_score));
    }

    #[test]
    fn test_filter_builder_chaining() {
        let start = Utc.with_ymd_and_hms(2025, 6, 14, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, 6, 15, 0, 0, 0).unwrap();

        let filter = ScanFilter::new()
            .time_start(start)
            .time_end(end)
            .event_type("GameOpened")
            .field_eq("gameUid", "poker")
            .field_gt("duration", 60.0);

        assert_eq!(filter.event_type, Some("GameOpened".to_string()));
        assert_eq!(filter.field_filters.len(), 2);
    }

    #[test]
    fn test_default_filter() {
        let filter = ScanFilter::default();
        assert!(filter.time_start.is_none());
        assert!(filter.time_end.is_none());
        assert!(filter.event_type.is_none());
        assert!(filter.field_filters.is_empty());
    }
}
