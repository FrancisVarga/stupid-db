use std::env;

use serde::{Deserialize, Serialize};

/// Default S3 output location for Athena query results.
const DEFAULT_OUTPUT_LOCATION: &str = "s3://stupid-db-athena-results/";

/// 10 GB in bytes (10 * 1024^3).
const DEFAULT_MAX_SCAN_BYTES: u64 = 10_737_418_240;

// ── Env helpers (mirrors core/config.rs, kept local to avoid circular dep) ──

fn env_opt(key: &str) -> Option<String> {
    env::var(key).ok().filter(|s| !s.is_empty())
}

/// Read a profiled env var: tries `{PROFILE}_{KEY}` first, falls back to `{KEY}`.
fn profiled_env_opt(profile: &str, key: &str) -> Option<String> {
    if !profile.is_empty() {
        let prefixed = format!("{}_{}", profile, key);
        if let Some(v) = env_opt(&prefixed) {
            return Some(v);
        }
    }
    env_opt(key)
}

fn profiled_env_or(profile: &str, key: &str, default: &str) -> String {
    profiled_env_opt(profile, key).unwrap_or_else(|| default.to_string())
}

fn profiled_env_u32(profile: &str, key: &str, default: u32) -> u32 {
    profiled_env_opt(profile, key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn profiled_env_u64(profile: &str, key: &str, default: u64) -> u64 {
    profiled_env_opt(profile, key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn profiled_env_bool(profile: &str, key: &str, default: bool) -> bool {
    match profiled_env_opt(profile, key) {
        Some(v) => matches!(v.as_str(), "true" | "1"),
        None => default,
    }
}

// ── AthenaConfig ─────────────────────────────────────────────────

/// Configuration for AWS Athena integration.
///
/// Reads from environment variables with optional profile prefix.
/// When `STUPID_PROFILE=PROD`, checks `PROD_ATHENA_DATABASE` before `ATHENA_DATABASE`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AthenaConfig {
    /// Whether the Athena integration is enabled.
    pub enabled: bool,
    /// AWS region for Athena queries.
    pub region: String,
    /// Athena database name.
    pub database: String,
    /// Athena workgroup.
    pub workgroup: String,
    /// S3 path for query results.
    pub output_location: String,
    /// Maximum bytes to scan per query (0 = unlimited).
    pub max_scan_bytes: u64,
    /// Query timeout in seconds.
    pub timeout_seconds: u32,
}

impl AthenaConfig {
    /// Build config from environment variables.
    ///
    /// Reads `STUPID_PROFILE` to determine profile prefix.
    /// For each key, tries `{PROFILE}_ATHENA_*` first, then `ATHENA_*`.
    /// `ATHENA_REGION` falls back to `AWS_REGION` before using the default.
    pub fn from_env() -> Self {
        let profile = env_opt("STUPID_PROFILE")
            .map(|s| s.to_uppercase())
            .unwrap_or_default();
        Self::from_env_profiled(&profile)
    }

    /// Build config for a specific named profile.
    pub fn from_env_profiled(profile: &str) -> Self {
        let region = profiled_env_opt(profile, "ATHENA_REGION")
            .or_else(|| profiled_env_opt(profile, "AWS_REGION"))
            .unwrap_or_else(|| "ap-southeast-1".to_string());

        Self {
            enabled: profiled_env_bool(profile, "ATHENA_ENABLED", false),
            region,
            database: profiled_env_or(profile, "ATHENA_DATABASE", "default"),
            workgroup: profiled_env_or(profile, "ATHENA_WORKGROUP", "primary"),
            output_location: profiled_env_or(
                profile,
                "ATHENA_OUTPUT_LOCATION",
                DEFAULT_OUTPUT_LOCATION,
            ),
            max_scan_bytes: profiled_env_u64(
                profile,
                "ATHENA_MAX_SCAN_BYTES",
                DEFAULT_MAX_SCAN_BYTES,
            ),
            timeout_seconds: profiled_env_u32(profile, "ATHENA_TIMEOUT_SECONDS", 300),
        }
    }

    /// Returns `true` when Athena is enabled and the output location has been
    /// explicitly configured (differs from the placeholder default).
    pub fn is_configured(&self) -> bool {
        self.enabled && self.output_location != DEFAULT_OUTPUT_LOCATION
    }

    /// Convenience: max scan budget expressed in gigabytes.
    pub fn max_scan_gb(&self) -> f64 {
        self.max_scan_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env-based tests must run serially to avoid interfering with each other.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Helper: clear all ATHENA_* and profile env vars used by the config.
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
        ];
        for k in keys {
            env::remove_var(k);
        }
    }

    #[test]
    fn defaults_when_no_env_vars() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_athena_env();

        let cfg = AthenaConfig::from_env_profiled("");

        assert!(!cfg.enabled);
        assert_eq!(cfg.region, "ap-southeast-1");
        assert_eq!(cfg.database, "default");
        assert_eq!(cfg.workgroup, "primary");
        assert_eq!(cfg.output_location, DEFAULT_OUTPUT_LOCATION);
        assert_eq!(cfg.max_scan_bytes, DEFAULT_MAX_SCAN_BYTES);
        assert_eq!(cfg.timeout_seconds, 300);
    }

    #[test]
    fn from_env_reads_vars() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_athena_env();

        env::set_var("ATHENA_ENABLED", "true");
        env::set_var("ATHENA_DATABASE", "analytics");
        env::set_var("ATHENA_MAX_SCAN_BYTES", "5368709120");

        let cfg = AthenaConfig::from_env_profiled("");

        assert!(cfg.enabled);
        assert_eq!(cfg.database, "analytics");
        assert_eq!(cfg.max_scan_bytes, 5_368_709_120);

        clear_athena_env();
    }

    #[test]
    fn enabled_with_1() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_athena_env();

        env::set_var("ATHENA_ENABLED", "1");

        let cfg = AthenaConfig::from_env_profiled("");
        assert!(cfg.enabled);

        clear_athena_env();
    }

    #[test]
    fn region_falls_back_to_aws_region() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_athena_env();

        env::set_var("AWS_REGION", "us-west-2");

        let cfg = AthenaConfig::from_env_profiled("");
        assert_eq!(cfg.region, "us-west-2");

        clear_athena_env();
    }

    #[test]
    fn athena_region_takes_precedence_over_aws_region() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_athena_env();

        env::set_var("AWS_REGION", "us-west-2");
        env::set_var("ATHENA_REGION", "eu-west-1");

        let cfg = AthenaConfig::from_env_profiled("");
        assert_eq!(cfg.region, "eu-west-1");

        clear_athena_env();
    }

    #[test]
    fn profiled_env_takes_precedence() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_athena_env();

        env::set_var("ATHENA_DATABASE", "base_db");
        env::set_var("TEST_ATHENA_DATABASE", "test_db");

        let cfg = AthenaConfig::from_env_profiled("TEST");
        assert_eq!(cfg.database, "test_db");

        clear_athena_env();
    }

    #[test]
    fn profiled_region_fallback_chain() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_athena_env();

        // Profile AWS_REGION should be used when no ATHENA_REGION is set.
        env::set_var("TEST_AWS_REGION", "ap-northeast-1");

        let cfg = AthenaConfig::from_env_profiled("TEST");
        assert_eq!(cfg.region, "ap-northeast-1");

        // Now set profiled ATHENA_REGION — it should win.
        env::set_var("TEST_ATHENA_REGION", "eu-central-1");

        let cfg = AthenaConfig::from_env_profiled("TEST");
        assert_eq!(cfg.region, "eu-central-1");

        clear_athena_env();
    }

    #[test]
    fn is_configured_false_when_disabled() {
        let cfg = AthenaConfig {
            enabled: false,
            region: "ap-southeast-1".into(),
            database: "default".into(),
            workgroup: "primary".into(),
            output_location: "s3://my-bucket/results/".into(),
            max_scan_bytes: DEFAULT_MAX_SCAN_BYTES,
            timeout_seconds: 300,
        };
        assert!(!cfg.is_configured());
    }

    #[test]
    fn is_configured_false_when_default_output() {
        let cfg = AthenaConfig {
            enabled: true,
            region: "ap-southeast-1".into(),
            database: "default".into(),
            workgroup: "primary".into(),
            output_location: DEFAULT_OUTPUT_LOCATION.into(),
            max_scan_bytes: DEFAULT_MAX_SCAN_BYTES,
            timeout_seconds: 300,
        };
        assert!(!cfg.is_configured());
    }

    #[test]
    fn is_configured_true_when_enabled_and_custom_output() {
        let cfg = AthenaConfig {
            enabled: true,
            region: "ap-southeast-1".into(),
            database: "analytics".into(),
            workgroup: "primary".into(),
            output_location: "s3://my-bucket/results/".into(),
            max_scan_bytes: DEFAULT_MAX_SCAN_BYTES,
            timeout_seconds: 300,
        };
        assert!(cfg.is_configured());
    }

    #[test]
    fn max_scan_gb_conversion() {
        let cfg = AthenaConfig {
            enabled: false,
            region: String::new(),
            database: String::new(),
            workgroup: String::new(),
            output_location: String::new(),
            max_scan_bytes: DEFAULT_MAX_SCAN_BYTES,
            timeout_seconds: 0,
        };
        assert!((cfg.max_scan_gb() - 10.0).abs() < 0.001);

        let zero = AthenaConfig { max_scan_bytes: 0, ..cfg.clone() };
        assert!((zero.max_scan_gb() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn invalid_u64_falls_back_to_default() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_athena_env();

        env::set_var("ATHENA_MAX_SCAN_BYTES", "not_a_number");

        let cfg = AthenaConfig::from_env_profiled("");
        assert_eq!(cfg.max_scan_bytes, DEFAULT_MAX_SCAN_BYTES);

        clear_athena_env();
    }
}
