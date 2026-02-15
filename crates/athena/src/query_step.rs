use serde::{Deserialize, Serialize};

use crate::client::{AthenaClient, AthenaError};
use crate::result::AthenaQueryResult;
use crate::convert::result_to_documents;
use stupid_core::Document;

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

/// Parameters for an Athena query step in a query plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AthenaQueryStepParams {
    /// SQL query to execute against Athena.
    pub sql: String,
    /// Optional scan limit in GB (converted to bytes for execution).
    /// If None, uses the config default from AthenaClient.
    #[serde(default)]
    pub max_scan_gb: Option<f64>,
    /// Event type for result-to-document conversion (default: "AthenaResult").
    #[serde(default)]
    pub event_type: Option<String>,
    /// Column name containing timestamps for Document.timestamp (ISO 8601 format).
    /// If None, uses current time for all documents.
    #[serde(default)]
    pub timestamp_column: Option<String>,
}

// ---------------------------------------------------------------------------
// Query Step
// ---------------------------------------------------------------------------

/// A single Athena query step within a broader query plan.
///
/// Represents a node in the query plan that executes a SQL query against
/// AWS Athena and optionally converts the results to Documents for
/// downstream processing.
///
/// # Example JSON (from design doc)
/// ```json
/// {
///   "id": "s3",
///   "store": "athena",
///   "op": "query",
///   "params": {
///     "sql": "SELECT date_trunc('day', timestamp) as day, count(*) as error_count FROM events WHERE event_type = 'API Error' AND timestamp BETWEEN '2024-10-01' AND '2024-12-31' GROUP BY 1 ORDER BY 1",
///     "max_scan_gb": 5
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AthenaQueryStep {
    /// Step identifier from the query plan.
    pub id: String,
    /// Query parameters.
    pub params: AthenaQueryStepParams,
}

impl AthenaQueryStep {
    /// Create a new Athena query step.
    pub fn new(id: String, params: AthenaQueryStepParams) -> Self {
        Self { id, params }
    }

    /// Execute the query against Athena and return the raw result.
    ///
    /// If `params.max_scan_gb` is set, the query will fail with
    /// [`AthenaError::ScanLimitExceeded`] if the scan exceeds the limit.
    pub async fn execute(&self, client: &AthenaClient) -> Result<AthenaQueryResult, AthenaError> {
        if let Some(max_gb) = self.params.max_scan_gb {
            let max_bytes = gb_to_bytes(max_gb);
            client.execute_query_with_limit(&self.params.sql, max_bytes).await
        } else {
            client.execute_query(&self.params.sql).await
        }
    }

    /// Execute the query and convert the results to Documents.
    ///
    /// Uses the `event_type` and `timestamp_column` parameters to control
    /// how rows are mapped to Documents.
    pub async fn execute_to_documents(
        &self,
        client: &AthenaClient,
    ) -> Result<Vec<Document>, AthenaError> {
        let result = self.execute(client).await?;
        let event_type = self.params.event_type.as_deref().unwrap_or("AthenaResult");
        let timestamp_column = self.params.timestamp_column.as_deref();

        Ok(result_to_documents(&result, event_type, timestamp_column))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert gigabytes to bytes.
///
/// 1 GB = 1,073,741,824 bytes (2^30).
fn gb_to_bytes(gb: f64) -> u64 {
    (gb * 1_073_741_824.0) as u64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_construction() {
        let params = AthenaQueryStepParams {
            sql: "SELECT 1".into(),
            max_scan_gb: Some(5.0),
            event_type: Some("TestEvent".into()),
            timestamp_column: Some("ts".into()),
        };

        let step = AthenaQueryStep::new("s3".into(), params.clone());
        assert_eq!(step.id, "s3");
        assert_eq!(step.params.sql, "SELECT 1");
        assert_eq!(step.params.max_scan_gb, Some(5.0));
        assert_eq!(step.params.event_type.as_deref(), Some("TestEvent"));
        assert_eq!(step.params.timestamp_column.as_deref(), Some("ts"));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let params = AthenaQueryStepParams {
            sql: "SELECT date_trunc('day', timestamp) as day, count(*) as error_count FROM events WHERE event_type = 'API Error' AND timestamp BETWEEN '2024-10-01' AND '2024-12-31' GROUP BY 1 ORDER BY 1".into(),
            max_scan_gb: Some(5.0),
            event_type: None,
            timestamp_column: None,
        };

        let step = AthenaQueryStep::new("s3".into(), params);

        let json = serde_json::to_string(&step).expect("serialize");
        let deserialized: AthenaQueryStep = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.id, "s3");
        assert_eq!(deserialized.params.sql, step.params.sql);
        assert_eq!(deserialized.params.max_scan_gb, Some(5.0));
    }

    #[test]
    fn test_deserialization_from_design_doc_format() {
        // Format from docs/architecture/storage/aws-integration.md
        let json = r#"{
            "id": "s3",
            "params": {
                "sql": "SELECT date_trunc('day', timestamp) as day, count(*) as error_count FROM events WHERE event_type = 'API Error' AND timestamp BETWEEN '2024-10-01' AND '2024-12-31' GROUP BY 1 ORDER BY 1",
                "max_scan_gb": 5
            }
        }"#;

        let step: AthenaQueryStep = serde_json::from_str(json).expect("deserialize");
        assert_eq!(step.id, "s3");
        assert_eq!(step.params.max_scan_gb, Some(5.0));
        assert!(step.params.sql.contains("API Error"));
    }

    #[test]
    fn test_deserialization_with_all_params() {
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
        assert_eq!(step.params.max_scan_gb, Some(2.5));
        assert_eq!(step.params.event_type.as_deref(), Some("ErrorLog"));
        assert_eq!(step.params.timestamp_column.as_deref(), Some("timestamp"));
    }

    #[test]
    fn test_gb_to_bytes_conversion() {
        // 1 GB = 2^30 bytes = 1,073,741,824 bytes
        assert_eq!(gb_to_bytes(1.0), 1_073_741_824);
        // 5 GB
        assert_eq!(gb_to_bytes(5.0), 5_368_709_120);
        // 0.5 GB
        assert_eq!(gb_to_bytes(0.5), 536_870_912);
        // 2.5 GB
        assert_eq!(gb_to_bytes(2.5), 2_684_354_560);
    }

    #[test]
    fn test_params_default_values() {
        let json = r#"{
            "sql": "SELECT 1"
        }"#;

        let params: AthenaQueryStepParams = serde_json::from_str(json).expect("deserialize");
        assert_eq!(params.sql, "SELECT 1");
        assert_eq!(params.max_scan_gb, None);
        assert_eq!(params.event_type, None);
        assert_eq!(params.timestamp_column, None);
    }
}
