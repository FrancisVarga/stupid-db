//! Tests for AthenaConfig: environment loading, profiles, and is_configured logic.

use std::env;
use std::sync::Mutex;

use stupid_athena::*;

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
