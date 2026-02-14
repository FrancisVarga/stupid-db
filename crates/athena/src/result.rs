use std::fmt;

use serde::{Deserialize, Serialize};

/// Column definition returned by an Athena query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AthenaColumn {
    /// Column name as declared in the result set.
    pub name: String,
    /// Athena data type (e.g. "varchar", "bigint", "double", "boolean", "timestamp").
    pub data_type: String,
}

/// Execution metadata for a completed Athena query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMetadata {
    /// Athena query execution ID.
    pub query_id: String,
    /// Total bytes scanned during execution.
    pub bytes_scanned: u64,
    /// Wall-clock execution time in milliseconds.
    pub execution_time_ms: u64,
    /// Final execution state ("SUCCEEDED", "FAILED", "CANCELLED").
    pub state: String,
    /// S3 output location where results were written, if available.
    pub output_location: Option<String>,
}

/// Structured result set from an Athena query execution.
///
/// Rows are stored as `Vec<Option<String>>` where `None` represents SQL NULL.
/// Column ordering in each row matches the `columns` vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AthenaQueryResult {
    /// Column definitions in result-set order.
    pub columns: Vec<AthenaColumn>,
    /// Row data. Each inner vector has the same length as `columns`.
    pub rows: Vec<Vec<Option<String>>>,
    /// Query execution metadata.
    pub metadata: QueryMetadata,
}

/// Athena pricing: $5 per TB scanned.
const DOLLARS_PER_BYTE: f64 = 5.0 / (1024.0 * 1024.0 * 1024.0 * 1024.0);

impl AthenaQueryResult {
    /// Returns the number of data rows in the result set.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Returns the number of columns in the result set.
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Returns `true` if the result set contains no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Finds the zero-based index of a column by name (case-sensitive).
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }

    /// Retrieves the value at the given row index and column name.
    ///
    /// Returns `None` if the row index is out of bounds, the column name
    /// does not exist, or the cell value is SQL NULL.
    pub fn get_value(&self, row: usize, col: &str) -> Option<&str> {
        let col_idx = self.column_index(col)?;
        let row_data = self.rows.get(row)?;
        row_data.get(col_idx)?.as_deref()
    }

    /// Estimates the query cost in USD based on Athena's $5/TB pricing model.
    pub fn cost_estimate_usd(&self) -> f64 {
        self.metadata.bytes_scanned as f64 * DOLLARS_PER_BYTE
    }
}

impl fmt::Display for AthenaQueryResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.columns.is_empty() {
            return write!(f, "(empty result set)");
        }

        // Compute column widths (minimum = header length).
        let mut widths: Vec<usize> = self.columns.iter().map(|c| c.name.len()).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    let cell_len = cell.as_deref().unwrap_or("NULL").len();
                    if cell_len > widths[i] {
                        widths[i] = cell_len;
                    }
                }
            }
        }

        // Header row.
        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                write!(f, " | ")?;
            }
            write!(f, "{:<width$}", col.name, width = widths[i])?;
        }
        writeln!(f)?;

        // Separator.
        for (i, w) in widths.iter().enumerate() {
            if i > 0 {
                write!(f, "-+-")?;
            }
            write!(f, "{}", "-".repeat(*w))?;
        }
        writeln!(f)?;

        // Data rows.
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    write!(f, " | ")?;
                }
                let value = cell.as_deref().unwrap_or("NULL");
                write!(f, "{:<width$}", value, width = widths[i])?;
            }
            writeln!(f)?;
        }

        // Metadata summary.
        writeln!(f)?;
        write!(
            f,
            "Query {} | {} rows | {:.3} MB scanned | {}ms | ${:.6}",
            self.metadata.query_id,
            self.rows.len(),
            self.metadata.bytes_scanned as f64 / (1024.0 * 1024.0),
            self.metadata.execution_time_ms,
            self.cost_estimate_usd(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a simple result for testing.
    fn sample_result() -> AthenaQueryResult {
        AthenaQueryResult {
            columns: vec![
                AthenaColumn { name: "id".into(), data_type: "bigint".into() },
                AthenaColumn { name: "name".into(), data_type: "varchar".into() },
                AthenaColumn { name: "score".into(), data_type: "double".into() },
            ],
            rows: vec![
                vec![Some("1".into()), Some("alice".into()), Some("9.5".into())],
                vec![Some("2".into()), Some("bob".into()), None],
                vec![Some("3".into()), None, Some("7.0".into())],
            ],
            metadata: QueryMetadata {
                query_id: "abc-123".into(),
                bytes_scanned: 1_073_741_824, // 1 GB
                execution_time_ms: 4200,
                state: "SUCCEEDED".into(),
                output_location: Some("s3://bucket/results/abc-123.csv".into()),
            },
        }
    }

    fn empty_result() -> AthenaQueryResult {
        AthenaQueryResult {
            columns: vec![],
            rows: vec![],
            metadata: QueryMetadata {
                query_id: "empty-0".into(),
                bytes_scanned: 0,
                execution_time_ms: 50,
                state: "SUCCEEDED".into(),
                output_location: None,
            },
        }
    }

    #[test]
    fn test_construction_and_accessors() {
        let r = sample_result();
        assert_eq!(r.row_count(), 3);
        assert_eq!(r.column_count(), 3);
        assert!(!r.is_empty());
        assert_eq!(r.metadata.state, "SUCCEEDED");
        assert_eq!(
            r.metadata.output_location.as_deref(),
            Some("s3://bucket/results/abc-123.csv"),
        );
    }

    #[test]
    fn test_column_index() {
        let r = sample_result();
        assert_eq!(r.column_index("id"), Some(0));
        assert_eq!(r.column_index("name"), Some(1));
        assert_eq!(r.column_index("score"), Some(2));
        assert_eq!(r.column_index("missing"), None);
    }

    #[test]
    fn test_get_value() {
        let r = sample_result();
        // Normal cell.
        assert_eq!(r.get_value(0, "name"), Some("alice"));
        // NULL cell.
        assert_eq!(r.get_value(1, "score"), None);
        assert_eq!(r.get_value(2, "name"), None);
        // Out-of-bounds row.
        assert_eq!(r.get_value(99, "id"), None);
        // Unknown column.
        assert_eq!(r.get_value(0, "nope"), None);
    }

    #[test]
    fn test_cost_estimate() {
        let r = sample_result();
        // 1 GB = 1/1024 TB -> cost = 5.0 / 1024 ~ 0.00488281
        let cost = r.cost_estimate_usd();
        let expected = 5.0 / 1024.0;
        assert!(
            (cost - expected).abs() < 1e-9,
            "expected ~{expected}, got {cost}",
        );
    }

    #[test]
    fn test_cost_estimate_zero() {
        let r = empty_result();
        assert!((r.cost_estimate_usd()).abs() < f64::EPSILON);
    }

    #[test]
    fn test_empty_result() {
        let r = empty_result();
        assert_eq!(r.row_count(), 0);
        assert_eq!(r.column_count(), 0);
        assert!(r.is_empty());
        assert_eq!(r.column_index("any"), None);
        assert_eq!(r.get_value(0, "any"), None);
    }

    #[test]
    fn test_display_formatting() {
        let r = sample_result();
        let output = r.to_string();

        // Header present.
        assert!(output.contains("id"));
        assert!(output.contains("name"));
        assert!(output.contains("score"));
        // Data present.
        assert!(output.contains("alice"));
        assert!(output.contains("bob"));
        assert!(output.contains("NULL"));
        // Metadata summary present.
        assert!(output.contains("abc-123"));
        assert!(output.contains("3 rows"));
        assert!(output.contains("4200ms"));
        // Cost shown.
        assert!(output.contains("$"));
    }

    #[test]
    fn test_display_empty() {
        let r = empty_result();
        let output = r.to_string();
        assert!(output.contains("empty result set"));
    }

    #[test]
    fn test_serde_roundtrip() {
        let r = sample_result();
        let json = serde_json::to_string(&r).expect("serialize");
        let deserialized: AthenaQueryResult =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.row_count(), r.row_count());
        assert_eq!(deserialized.column_count(), r.column_count());
        assert_eq!(deserialized.metadata.query_id, r.metadata.query_id);
        assert_eq!(
            deserialized.get_value(0, "name"),
            r.get_value(0, "name"),
        );
    }
}
