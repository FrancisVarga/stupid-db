#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::{DateTime, Utc};
    use uuid::Uuid;

    use stupid_core::{Document, FieldValue};

    use crate::algorithms::prefixspan::{
        build_sequences, classify_pattern, compress_event, prefixspan,
        EventTypeCompressed, PatternCategory, PrefixSpanConfig,
    };
    use crate::algorithms::prefixspan::mining::pattern_id;

    fn make_doc(event_type: &str, member: &str, fields: Vec<(&str, &str)>, ts: DateTime<Utc>) -> Document {
        let mut field_map = HashMap::new();
        field_map.insert("memberCode".to_owned(), FieldValue::Text(member.to_owned()));
        for (k, v) in fields {
            field_map.insert(k.to_owned(), FieldValue::Text(v.to_owned()));
        }
        Document {
            id: Uuid::new_v4(),
            timestamp: ts,
            event_type: event_type.to_owned(),
            fields: field_map,
        }
    }

    #[test]
    fn compress_event_types() {
        let ts = Utc::now();

        let login = make_doc("Login", "M001", vec![], ts);
        assert_eq!(compress_event(&login).0, "L");

        let game = make_doc("GameOpened", "M001", vec![("game", "Slots")], ts);
        assert_eq!(compress_event(&game).0, "G:Slots");

        let error = make_doc("API Error", "M001", vec![("statusCode", "401")], ts);
        assert_eq!(compress_event(&error).0, "E:401");

        let popup = make_doc("PopupModule", "M001", vec![("action", "click")], ts);
        assert_eq!(compress_event(&popup).0, "P:click");
    }

    #[test]
    fn build_sequences_groups_by_member() {
        let ts = Utc::now();
        let docs = vec![
            make_doc("Login", "M001", vec![], ts),
            make_doc("Login", "M002", vec![], ts),
            make_doc("GameOpened", "M001", vec![("game", "Slots")], ts + chrono::Duration::seconds(10)),
        ];

        let seqs = build_sequences(&docs);
        assert_eq!(seqs.len(), 2);
        assert_eq!(seqs["M001"].len(), 2);
        assert_eq!(seqs["M002"].len(), 1);
    }

    #[test]
    fn prefixspan_finds_frequent_patterns() {
        let ts = Utc::now();
        let config = PrefixSpanConfig {
            min_support: 0.5,
            max_length: 5,
            min_members: 2,
        };

        // Create 4 members, 3 of which have L -> G:Slots pattern.
        let mut docs = Vec::new();
        for i in 0..3 {
            let member = format!("M{:03}", i);
            docs.push(make_doc("Login", &member, vec![], ts));
            docs.push(make_doc("GameOpened", &member, vec![("game", "Slots")], ts + chrono::Duration::seconds(10)));
        }
        // 4th member has different pattern.
        docs.push(make_doc("Login", "M003", vec![], ts));
        docs.push(make_doc("API Error", "M003", vec![("statusCode", "500")], ts + chrono::Duration::seconds(10)));

        let seqs = build_sequences(&docs);
        let patterns = prefixspan(&seqs, &config);

        // Should find L -> G:Slots as a frequent pattern (3/4 = 0.75 support).
        assert!(!patterns.is_empty());

        let l_g = patterns.iter().find(|p| {
            p.sequence.len() == 2 && p.sequence[0].0 == "L" && p.sequence[1].0 == "G:Slots"
        });
        assert!(l_g.is_some(), "Should find L -> G:Slots pattern");
        assert_eq!(l_g.unwrap().member_count, 3);
    }

    #[test]
    fn prefixspan_empty_input() {
        let seqs: HashMap<String, Vec<(DateTime<Utc>, EventTypeCompressed)>> = HashMap::new();
        let config = PrefixSpanConfig::default();
        let patterns = prefixspan(&seqs, &config);
        assert!(patterns.is_empty());
    }

    #[test]
    fn classify_error_chain() {
        let seq = vec![
            EventTypeCompressed("E:401".into()),
            EventTypeCompressed("E:500".into()),
        ];
        assert_eq!(classify_pattern(&seq), PatternCategory::ErrorChain);
    }

    #[test]
    fn classify_funnel() {
        let seq = vec![
            EventTypeCompressed("L".into()),
            EventTypeCompressed("G:Slots".into()),
        ];
        assert_eq!(classify_pattern(&seq), PatternCategory::Funnel);
    }

    #[test]
    fn classify_churn() {
        let seq = vec![
            EventTypeCompressed("L".into()),
            EventTypeCompressed("E:500".into()),
        ];
        assert_eq!(classify_pattern(&seq), PatternCategory::Churn);
    }

    #[test]
    fn classify_engagement() {
        let seq = vec![
            EventTypeCompressed("G:Slots".into()),
            EventTypeCompressed("G:Poker".into()),
        ];
        assert_eq!(classify_pattern(&seq), PatternCategory::Engagement);
    }

    #[test]
    fn pattern_id_deterministic() {
        let seq = vec![
            EventTypeCompressed("L".into()),
            EventTypeCompressed("G:Slots".into()),
        ];
        let id1 = pattern_id(&seq);
        let id2 = pattern_id(&seq);
        assert_eq!(id1, id2);
    }
}
