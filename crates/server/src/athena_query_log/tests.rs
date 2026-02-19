use chrono::Utc;

use super::*;

#[test]
fn test_zero_scan_is_free() {
    assert_eq!(calculate_query_cost(0), 0.0);
}

#[test]
fn test_minimum_billing() {
    // Anything below 10 MB should bill as 10 MB.
    let cost_1byte = calculate_query_cost(1);
    let cost_10mb = calculate_query_cost(10 * 1024 * 1024);
    assert_eq!(cost_1byte, cost_10mb);
}

#[test]
fn test_1tb_costs_5_dollars() {
    let one_tb: i64 = 1024 * 1024 * 1024 * 1024;
    let cost = calculate_query_cost(one_tb);
    assert!((cost - 5.0).abs() < 0.001);
}

#[test]
fn test_above_minimum() {
    let one_gb: i64 = 1024 * 1024 * 1024;
    let cost = calculate_query_cost(one_gb);
    // $5 per TB = $5/1024 per GB
    let expected = 5.0 / 1024.0;
    assert!((cost - expected).abs() < 0.0001);
}

#[test]
fn test_append_and_query() {
    let dir = std::env::temp_dir().join(format!("athena_log_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = AthenaQueryLog::new(&dir);

    let entry = AthenaQueryLogEntry {
        entry_id: 0,
        connection_id: "test-conn".into(),
        query_execution_id: Some("qid-1".into()),
        source: QuerySource::UserQuery,
        sql: "SELECT 1".into(),
        database: "default".into(),
        workgroup: "primary".into(),
        outcome: QueryOutcome::Succeeded,
        error_message: None,
        data_scanned_bytes: 1024 * 1024 * 100, // 100MB
        engine_execution_time_ms: 500,
        total_rows: Some(1),
        estimated_cost_usd: calculate_query_cost(1024 * 1024 * 100),
        started_at: Utc::now(),
        completed_at: Utc::now(),
        wall_clock_ms: 500,
    };

    log.append(entry);

    let params = QueryLogParams {
        source: None,
        outcome: None,
        since: None,
        until: None,
        limit: None,
        sql_contains: None,
    };
    let results = log.query("test-conn", &params);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry_id, 1);
    assert_eq!(results[0].sql, "SELECT 1");

    // Cleanup
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_fifo_eviction() {
    let dir =
        std::env::temp_dir().join(format!("athena_log_evict_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut log = AthenaQueryLog::new(&dir);
    log.max_entries_per_connection = 3;

    for i in 0..5 {
        log.append(AthenaQueryLogEntry {
            entry_id: 0,
            connection_id: "conn".into(),
            query_execution_id: Some(format!("q-{}", i)),
            source: QuerySource::UserQuery,
            sql: format!("SELECT {}", i),
            database: "db".into(),
            workgroup: "wg".into(),
            outcome: QueryOutcome::Succeeded,
            error_message: None,
            data_scanned_bytes: 0,
            engine_execution_time_ms: 0,
            total_rows: None,
            estimated_cost_usd: 0.0,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            wall_clock_ms: 0,
        });
    }

    let params = QueryLogParams {
        source: None,
        outcome: None,
        since: None,
        until: None,
        limit: None,
        sql_contains: None,
    };
    let results = log.query("conn", &params);
    assert_eq!(results.len(), 3);
    // Oldest (0, 1) should be evicted; newest first in results
    assert_eq!(results[0].sql, "SELECT 4");
    assert_eq!(results[2].sql, "SELECT 2");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_persistence_round_trip() {
    let dir =
        std::env::temp_dir().join(format!("athena_log_persist_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // Write with one instance
    {
        let log = AthenaQueryLog::new(&dir);
        log.append(AthenaQueryLogEntry {
            entry_id: 0,
            connection_id: "persist-conn".into(),
            query_execution_id: Some("q-persist".into()),
            source: QuerySource::SchemaRefreshDatabases,
            sql: "SHOW DATABASES".into(),
            database: "default".into(),
            workgroup: "primary".into(),
            outcome: QueryOutcome::Succeeded,
            error_message: None,
            data_scanned_bytes: 0,
            engine_execution_time_ms: 100,
            total_rows: Some(5),
            estimated_cost_usd: 0.0,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            wall_clock_ms: 100,
        });
    }

    // Read with new instance (simulates server restart)
    {
        let log = AthenaQueryLog::new(&dir);
        let params = QueryLogParams {
            source: None,
            outcome: None,
            since: None,
            until: None,
            limit: None,
            sql_contains: None,
        };
        let results = log.query("persist-conn", &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sql, "SHOW DATABASES");
        assert_eq!(results[0].entry_id, 1);
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_summary_daily_aggregates() {
    let dir =
        std::env::temp_dir().join(format!("athena_log_summary_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = AthenaQueryLog::new(&dir);

    let one_gb: i64 = 1024 * 1024 * 1024;

    for i in 0..3 {
        log.append(AthenaQueryLogEntry {
            entry_id: 0,
            connection_id: "sum-conn".into(),
            query_execution_id: Some(format!("q-{}", i)),
            source: if i == 0 {
                QuerySource::UserQuery
            } else {
                QuerySource::SchemaRefreshTables
            },
            sql: format!("query {}", i),
            database: "db".into(),
            workgroup: "wg".into(),
            outcome: QueryOutcome::Succeeded,
            error_message: None,
            data_scanned_bytes: one_gb,
            engine_execution_time_ms: 1000,
            total_rows: Some(100),
            estimated_cost_usd: calculate_query_cost(one_gb),
            started_at: Utc::now(),
            completed_at: Utc::now(),
            wall_clock_ms: 1000,
        });
    }

    let summary = log.summary("sum-conn");
    assert_eq!(summary.total_queries, 3);
    assert_eq!(summary.total_bytes_scanned, 3 * one_gb);
    assert!(summary.total_cost_usd > 0.0);
    assert_eq!(summary.daily.len(), 1); // All same day
    assert_eq!(summary.daily[0].query_count, 3);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_clear_removes_entries_and_file() {
    let dir =
        std::env::temp_dir().join(format!("athena_log_clear_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = AthenaQueryLog::new(&dir);

    log.append(AthenaQueryLogEntry {
        entry_id: 0,
        connection_id: "clear-conn".into(),
        query_execution_id: None,
        source: QuerySource::UserQuery,
        sql: "SELECT 1".into(),
        database: "db".into(),
        workgroup: "wg".into(),
        outcome: QueryOutcome::Failed,
        error_message: Some("test error".into()),
        data_scanned_bytes: 0,
        engine_execution_time_ms: 0,
        total_rows: None,
        estimated_cost_usd: 0.0,
        started_at: Utc::now(),
        completed_at: Utc::now(),
        wall_clock_ms: 0,
    });

    // File should exist
    assert!(log.log_path("clear-conn").exists());

    log.clear("clear-conn");

    // File should be deleted
    assert!(!log.log_path("clear-conn").exists());

    // Query should return empty
    let params = QueryLogParams {
        source: None,
        outcome: None,
        since: None,
        until: None,
        limit: None,
        sql_contains: None,
    };
    let results = log.query("clear-conn", &params);
    assert!(results.is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_filter_by_source() {
    let dir =
        std::env::temp_dir().join(format!("athena_log_filter_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = AthenaQueryLog::new(&dir);

    for source in [
        QuerySource::UserQuery,
        QuerySource::SchemaRefreshDatabases,
        QuerySource::UserQuery,
    ] {
        log.append(AthenaQueryLogEntry {
            entry_id: 0,
            connection_id: "filter-conn".into(),
            query_execution_id: None,
            source,
            sql: "test".into(),
            database: "db".into(),
            workgroup: "wg".into(),
            outcome: QueryOutcome::Succeeded,
            error_message: None,
            data_scanned_bytes: 0,
            engine_execution_time_ms: 0,
            total_rows: None,
            estimated_cost_usd: 0.0,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            wall_clock_ms: 0,
        });
    }

    let params = QueryLogParams {
        source: Some(QuerySource::UserQuery),
        outcome: None,
        since: None,
        until: None,
        limit: None,
        sql_contains: None,
    };
    let results = log.query("filter-conn", &params);
    assert_eq!(results.len(), 2);

    std::fs::remove_dir_all(&dir).ok();
}
