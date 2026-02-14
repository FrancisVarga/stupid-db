//! Integration tests for stupid-athena crate.
//!
//! These tests verify the integration of all athena modules without requiring AWS credentials.
//! Tests marked with `#[ignore]` require AWS credentials and must be run explicitly.

use std::env;
use std::sync::Mutex;

use chrono::Utc;
use stupid_athena::*;
use stupid_core::FieldValue;

// Env-based tests must run serially to avoid interfering with each other.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// Helper: clear all ATHENA_* and profile env vars used by the config.
fn clear_athena_env() {
    let keys = [
        "STUPID_PROFILE",
        "ATHENA_ENABLED",
        "ATHENA_REGION",
        "ATHENA_DATABASE",
        "ATHENA_WORKGROUP",
        "ATHENA_OUTPUT_LOCATION",
        "ATHENA_MAX_SCAN_BYTES",
        "ATHENA_TIMEOUT_SECONDS",
        "AWS_REGION",
        "TEST_ATHENA_ENABLED",
        "TEST_ATHENA_DATABASE",
        "TEST_ATHENA_REGION",
        "TEST_AWS_REGION",
        "TEST_ATHENA_OUTPUT_LOCATION",
    ];
    for k in keys {
        env::remove_var(k);
    }
}

// ── Config Tests ─────────────────────────────────────────────────────

#[test]
fn test_config_from_env() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_athena_env();

    env::set_var("ATHENA_ENABLED", "true");
    env::set_var("ATHENA_REGION", "us-west-2");
    env::set_var("ATHENA_DATABASE", "analytics");
    env::set_var("ATHENA_WORKGROUP", "custom");
    env::set_var("ATHENA_OUTPUT_LOCATION", "s3://my-bucket/results/");
    env::set_var("ATHENA_MAX_SCAN_BYTES", "5368709120"); // 5 GB
    env::set_var("ATHENA_TIMEOUT_SECONDS", "600");

    let cfg = AthenaConfig::from_env();

    assert!(cfg.enabled);
    assert_eq!(cfg.region, "us-west-2");
    assert_eq!(cfg.database, "analytics");
    assert_eq!(cfg.workgroup, "custom");
    assert_eq!(cfg.output_location, "s3://my-bucket/results/");
    assert_eq!(cfg.max_scan_bytes, 5_368_709_120);
    assert_eq!(cfg.timeout_seconds, 600);

    // Should be configured because enabled and custom output location
    assert!(cfg.is_configured());

    // Check GB conversion
    assert!((cfg.max_scan_gb() - 5.0).abs() < 0.001);

    clear_athena_env();
}

#[test]
fn test_config_profile() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_athena_env();

    // Set base config
    env::set_var("ATHENA_DATABASE", "base_db");
    env::set_var("ATHENA_ENABLED", "false");

    // Set profiled config
    env::set_var("STUPID_PROFILE", "TEST");
    env::set_var("TEST_ATHENA_DATABASE", "test_db");
    env::set_var("TEST_ATHENA_ENABLED", "true");
    env::set_var("TEST_ATHENA_REGION", "eu-west-1");
    env::set_var("TEST_ATHENA_OUTPUT_LOCATION", "s3://test-bucket/");

    let cfg = AthenaConfig::from_env();

    // Should use profiled values
    assert!(cfg.enabled);
    assert_eq!(cfg.database, "test_db");
    assert_eq!(cfg.region, "eu-west-1");
    assert_eq!(cfg.output_location, "s3://test-bucket/");
    assert!(cfg.is_configured());

    clear_athena_env();
}

// ── Result Tests ─────────────────────────────────────────────────────

#[test]
fn test_result_parsing() {
    let result = AthenaQueryResult {
        columns: vec![
            AthenaColumn {
                name: "id".to_string(),
                data_type: "bigint".to_string(),
            },
            AthenaColumn {
                name: "name".to_string(),
                data_type: "varchar".to_string(),
            },
            AthenaColumn {
                name: "score".to_string(),
                data_type: "double".to_string(),
            },
        ],
        rows: vec![
            vec![Some("123".to_string()), Some("Alice".to_string()), Some("9.5".to_string())],
            vec![Some("456".to_string()), Some("Bob".to_string()), None],
            vec![Some("789".to_string()), None, Some("7.2".to_string())],
        ],
        metadata: QueryMetadata {
            query_id: "test-123".to_string(),
            bytes_scanned: 1_073_741_824, // 1 GB
            execution_time_ms: 1500,
            state: "SUCCEEDED".to_string(),
            output_location: Some("s3://bucket/results/".to_string()),
        },
    };

    // Test accessors
    assert_eq!(result.row_count(), 3);
    assert_eq!(result.column_count(), 3);
    assert!(!result.is_empty());

    // Test column_index
    assert_eq!(result.column_index("id"), Some(0));
    assert_eq!(result.column_index("name"), Some(1));
    assert_eq!(result.column_index("score"), Some(2));
    assert_eq!(result.column_index("missing"), None);

    // Test get_value
    assert_eq!(result.get_value(0, "id"), Some("123"));
    assert_eq!(result.get_value(0, "name"), Some("Alice"));
    assert_eq!(result.get_value(0, "score"), Some("9.5"));

    // Test NULL handling
    assert_eq!(result.get_value(1, "score"), None);
    assert_eq!(result.get_value(2, "name"), None);

    // Test out-of-bounds
    assert_eq!(result.get_value(99, "id"), None);
    assert_eq!(result.get_value(0, "nonexistent"), None);

    // Test cost estimate (1 GB -> $5/1024)
    let expected_cost = 5.0 / 1024.0;
    assert!((result.cost_estimate_usd() - expected_cost).abs() < 1e-9);
}

#[test]
fn test_result_display() {
    let result = AthenaQueryResult {
        columns: vec![
            AthenaColumn {
                name: "id".to_string(),
                data_type: "bigint".to_string(),
            },
            AthenaColumn {
                name: "value".to_string(),
                data_type: "varchar".to_string(),
            },
        ],
        rows: vec![
            vec![Some("1".to_string()), Some("alpha".to_string())],
            vec![Some("2".to_string()), None],
        ],
        metadata: QueryMetadata {
            query_id: "disp-123".to_string(),
            bytes_scanned: 500_000_000,
            execution_time_ms: 750,
            state: "SUCCEEDED".to_string(),
            output_location: None,
        },
    };

    let output = result.to_string();

    // Verify headers present
    assert!(output.contains("id"));
    assert!(output.contains("value"));

    // Verify data present
    assert!(output.contains("alpha"));
    assert!(output.contains("NULL")); // NULL displayed for None

    // Verify metadata
    assert!(output.contains("disp-123"));
    assert!(output.contains("2 rows"));
    assert!(output.contains("750ms"));
    assert!(output.contains("$")); // Cost displayed
}

// ── Convert Tests ────────────────────────────────────────────────────

#[test]
fn test_convert_to_documents() {
    let result = AthenaQueryResult {
        columns: vec![
            AthenaColumn {
                name: "member_id".to_string(),
                data_type: "bigint".to_string(),
            },
            AthenaColumn {
                name: "username".to_string(),
                data_type: "varchar".to_string(),
            },
            AthenaColumn {
                name: "balance".to_string(),
                data_type: "double".to_string(),
            },
            AthenaColumn {
                name: "is_vip".to_string(),
                data_type: "boolean".to_string(),
            },
            AthenaColumn {
                name: "created_at".to_string(),
                data_type: "timestamp".to_string(),
            },
        ],
        rows: vec![
            vec![
                Some("1001".to_string()),
                Some("alice".to_string()),
                Some("250.75".to_string()),
                Some("true".to_string()),
                Some("2025-01-15T10:30:00Z".to_string()),
            ],
            vec![
                Some("1002".to_string()),
                Some("bob".to_string()),
                None, // NULL balance
                Some("false".to_string()),
                Some("2025-01-16T14:20:00Z".to_string()),
            ],
        ],
        metadata: QueryMetadata {
            query_id: "conv-123".to_string(),
            bytes_scanned: 1024,
            execution_time_ms: 100,
            state: "SUCCEEDED".to_string(),
            output_location: None,
        },
    };

    let docs = result_to_documents(&result, "member_query", Some("created_at"));

    // Verify correct number of documents
    assert_eq!(docs.len(), 2);

    // Verify event type
    assert_eq!(docs[0].event_type, "member_query");
    assert_eq!(docs[1].event_type, "member_query");

    // Verify field types mapped correctly
    let doc1_fields = &docs[0].fields;
    assert_eq!(
        doc1_fields.get("member_id"),
        Some(&FieldValue::Integer(1001))
    );
    assert_eq!(
        doc1_fields.get("username"),
        Some(&FieldValue::Text("alice".to_string()))
    );
    assert_eq!(
        doc1_fields.get("balance"),
        Some(&FieldValue::Float(250.75))
    );
    assert_eq!(doc1_fields.get("is_vip"), Some(&FieldValue::Boolean(true)));

    // Verify NULL values are skipped (not included in fields map)
    let doc2_fields = &docs[1].fields;
    assert_eq!(doc2_fields.get("balance"), None);

    // Verify timestamps parsed from created_at column
    assert_eq!(
        docs[0].timestamp.to_rfc3339(),
        "2025-01-15T10:30:00+00:00"
    );
    assert_eq!(
        docs[1].timestamp.to_rfc3339(),
        "2025-01-16T14:20:00+00:00"
    );

    // created_at should still be in fields (not removed like we initially thought)
    assert!(doc1_fields.contains_key("created_at"));
}

#[test]
fn test_convert_missing_timestamp() {
    let result = AthenaQueryResult {
        columns: vec![AthenaColumn {
            name: "col1".to_string(),
            data_type: "varchar".to_string(),
        }],
        rows: vec![vec![Some("value1".to_string())]],
        metadata: QueryMetadata {
            query_id: "ts-test".to_string(),
            bytes_scanned: 100,
            execution_time_ms: 50,
            state: "SUCCEEDED".to_string(),
            output_location: None,
        },
    };

    let docs = result_to_documents(&result, "no_timestamp", None);

    assert_eq!(docs.len(), 1);

    // Verify timestamp is recent (within 5 seconds of now)
    let now = Utc::now();
    let diff = (now - docs[0].timestamp).num_seconds().abs();
    assert!(
        diff < 5,
        "Timestamp should be recent (within 5s), got diff: {}s",
        diff
    );
}

// ── QueryStep Tests ──────────────────────────────────────────────────

#[test]
fn test_query_step_construction() {
    let json = r#"{
        "id": "event_query",
        "params": {
            "sql": "SELECT * FROM events WHERE date = '2025-01-15'",
            "max_scan_gb": 5.0
        }
    }"#;

    let step: AthenaQueryStep = serde_json::from_str(json).expect("deserialize");

    assert_eq!(step.id, "event_query");
    assert_eq!(
        step.params.sql,
        "SELECT * FROM events WHERE date = '2025-01-15'"
    );
    assert_eq!(step.params.max_scan_gb, Some(5.0));
}

#[test]
fn test_query_step_optional_params() {
    let json = r#"{
        "id": "simple_query",
        "params": {
            "sql": "SELECT COUNT(*) FROM users"
        }
    }"#;

    let step: AthenaQueryStep = serde_json::from_str(json).expect("deserialize");

    assert_eq!(step.id, "simple_query");
    assert_eq!(step.params.sql, "SELECT COUNT(*) FROM users");
    assert_eq!(step.params.max_scan_gb, None);
    assert_eq!(step.params.event_type, None);
    assert_eq!(step.params.timestamp_column, None);
}

#[test]
fn test_max_scan_conversion() {
    let cfg = AthenaConfig {
        enabled: true,
        region: "us-east-1".to_string(),
        database: "test".to_string(),
        workgroup: "primary".to_string(),
        output_location: "s3://test/".to_string(),
        max_scan_bytes: 5_368_709_120, // 5 GB
        timeout_seconds: 300,
    };

    let gb = cfg.max_scan_gb();
    assert!((gb - 5.0).abs() < 0.001, "Expected ~5.0 GB, got {}", gb);

    // Test with 10.5 GB
    let cfg2 = AthenaConfig {
        max_scan_bytes: 11_274_289_152, // 10.5 * 1024^3
        ..cfg.clone()
    };
    let gb2 = cfg2.max_scan_gb();
    assert!(
        (gb2 - 10.5).abs() < 0.001,
        "Expected ~10.5 GB, got {}",
        gb2
    );
}

#[test]
fn test_query_step_with_all_params() {
    let json = r#"{
        "id": "error_analysis",
        "params": {
            "sql": "SELECT timestamp, message FROM errors",
            "max_scan_gb": 2.5,
            "event_type": "ErrorLog",
            "timestamp_column": "timestamp"
        }
    }"#;

    let step: AthenaQueryStep = serde_json::from_str(json).expect("deserialize");

    assert_eq!(step.id, "error_analysis");
    assert_eq!(step.params.sql, "SELECT timestamp, message FROM errors");
    assert_eq!(step.params.max_scan_gb, Some(2.5));
    assert_eq!(step.params.event_type.as_deref(), Some("ErrorLog"));
    assert_eq!(
        step.params.timestamp_column.as_deref(),
        Some("timestamp")
    );
}

// ── Real AWS Tests (ignored by default) ──────────────────────────────

/// This test requires valid AWS credentials and network access.
///
/// Run with: `cargo test test_real_athena_query -- --ignored`
///
/// Set environment variables before running:
/// - `ATHENA_ENABLED=true`
/// - `ATHENA_DATABASE=<your-database>`
/// - `ATHENA_OUTPUT_LOCATION=s3://<your-bucket>/results/`
/// - AWS credentials must be configured (via env vars or ~/.aws/credentials)
#[test]
#[ignore]
fn test_real_athena_query() {
    // This test is async, so we need a runtime
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let config = AthenaConfig::from_env();

        // Create client
        let client = AthenaClient::new(config)
            .await
            .expect("Failed to create AthenaClient - ensure ATHENA_ENABLED=true");

        // Execute a simple query
        let result = client
            .execute_query("SELECT 1 as test_column")
            .await
            .expect("Query execution failed");

        // Verify result structure
        assert_eq!(result.column_count(), 1);
        assert_eq!(result.row_count(), 1);
        assert_eq!(result.column_index("test_column"), Some(0));
        assert_eq!(result.get_value(0, "test_column"), Some("1"));
        assert_eq!(result.metadata.state, "SUCCEEDED");

        println!("Real Athena query succeeded!");
        println!("{}", result);
    });
}

// ── Additional Edge Case Tests ──────────────────────────────────────

#[test]
fn test_empty_result_set() {
    let result = AthenaQueryResult {
        columns: vec![],
        rows: vec![],
        metadata: QueryMetadata {
            query_id: "empty".to_string(),
            bytes_scanned: 0,
            execution_time_ms: 10,
            state: "SUCCEEDED".to_string(),
            output_location: None,
        },
    };

    assert_eq!(result.row_count(), 0);
    assert_eq!(result.column_count(), 0);
    assert!(result.is_empty());
    assert!((result.cost_estimate_usd()).abs() < f64::EPSILON);

    let docs = result_to_documents(&result, "empty_query", None);
    assert!(docs.is_empty());
}

#[test]
fn test_mixed_type_parsing() {
    let result = AthenaQueryResult {
        columns: vec![
            AthenaColumn {
                name: "int_col".to_string(),
                data_type: "integer".to_string(),
            },
            AthenaColumn {
                name: "bigint_col".to_string(),
                data_type: "bigint".to_string(),
            },
            AthenaColumn {
                name: "float_col".to_string(),
                data_type: "float".to_string(),
            },
            AthenaColumn {
                name: "decimal_col".to_string(),
                data_type: "decimal".to_string(),
            },
            AthenaColumn {
                name: "bool_col".to_string(),
                data_type: "boolean".to_string(),
            },
            AthenaColumn {
                name: "varchar_col".to_string(),
                data_type: "varchar".to_string(),
            },
        ],
        rows: vec![vec![
            Some("42".to_string()),
            Some("9223372036854775807".to_string()),
            Some("3.14159".to_string()),
            Some("99.99".to_string()),
            Some("true".to_string()),
            Some("hello world".to_string()),
        ]],
        metadata: QueryMetadata {
            query_id: "type-test".to_string(),
            bytes_scanned: 512,
            execution_time_ms: 20,
            state: "SUCCEEDED".to_string(),
            output_location: None,
        },
    };

    let docs = result_to_documents(&result, "type_test", None);
    assert_eq!(docs.len(), 1);

    let fields = &docs[0].fields;
    assert_eq!(fields.get("int_col"), Some(&FieldValue::Integer(42)));
    assert_eq!(
        fields.get("bigint_col"),
        Some(&FieldValue::Integer(9223372036854775807))
    );
    assert_eq!(
        fields.get("float_col"),
        Some(&FieldValue::Float(3.14159))
    );
    assert_eq!(fields.get("decimal_col"), Some(&FieldValue::Float(99.99)));
    assert_eq!(fields.get("bool_col"), Some(&FieldValue::Boolean(true)));
    assert_eq!(
        fields.get("varchar_col"),
        Some(&FieldValue::Text("hello world".to_string()))
    );
}

#[test]
fn test_config_is_configured_logic() {
    // enabled=false -> not configured
    let cfg1 = AthenaConfig {
        enabled: false,
        region: "us-east-1".to_string(),
        database: "db".to_string(),
        workgroup: "primary".to_string(),
        output_location: "s3://custom-bucket/".to_string(),
        max_scan_bytes: 0,
        timeout_seconds: 300,
    };
    assert!(!cfg1.is_configured());

    // enabled=true but default output location -> not configured
    let cfg2 = AthenaConfig {
        enabled: true,
        region: "us-east-1".to_string(),
        database: "db".to_string(),
        workgroup: "primary".to_string(),
        output_location: "s3://stupid-db-athena-results/".to_string(),
        max_scan_bytes: 0,
        timeout_seconds: 300,
    };
    assert!(!cfg2.is_configured());

    // enabled=true and custom output location -> configured
    let cfg3 = AthenaConfig {
        enabled: true,
        region: "us-east-1".to_string(),
        database: "db".to_string(),
        workgroup: "primary".to_string(),
        output_location: "s3://my-custom-bucket/results/".to_string(),
        max_scan_bytes: 0,
        timeout_seconds: 300,
    };
    assert!(cfg3.is_configured());
}
