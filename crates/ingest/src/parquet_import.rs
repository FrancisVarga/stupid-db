use std::path::Path;

use arrow::array::{Array, StringArray};
use chrono::{DateTime, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use stupid_core::{DocId, Document, FieldValue, StupidError};
use tracing::info;

pub struct ParquetImporter;

impl ParquetImporter {
    pub fn import(path: &Path, event_type: &str) -> Result<Vec<Document>, StupidError> {
        let file = std::fs::File::open(path).map_err(StupidError::Io)?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)
            .map_err(|e| StupidError::Parquet(e.to_string()))?;

        let reader = builder.build().map_err(|e| StupidError::Parquet(e.to_string()))?;

        let mut documents = Vec::new();

        for batch_result in reader {
            let batch = batch_result.map_err(|e| StupidError::Parquet(e.to_string()))?;
            let schema = batch.schema();
            let num_rows = batch.num_rows();

            // Pre-collect column names and arrays
            let columns: Vec<(&str, &StringArray)> = schema
                .fields()
                .iter()
                .enumerate()
                .filter_map(|(i, field)| {
                    batch
                        .column(i)
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .map(|arr| (field.name().as_str(), arr))
                })
                .collect();

            for row_idx in 0..num_rows {
                let mut fields = std::collections::HashMap::new();
                let mut timestamp: Option<DateTime<Utc>> = None;

                for &(col_name, arr) in &columns {
                    if arr.is_null(row_idx) {
                        continue;
                    }
                    let val = arr.value(row_idx).trim();
                    if val.is_empty() || val == "None" || val == "null" || val == "undefined" {
                        continue;
                    }

                    // Parse @timestamp
                    if col_name == "@timestamp" {
                        if let Ok(ts) = val.parse::<DateTime<Utc>>() {
                            timestamp = Some(ts);
                        }
                    }

                    fields.insert(col_name.to_string(), FieldValue::Text(val.to_string()));
                }

                let ts = timestamp.unwrap_or_else(Utc::now);

                documents.push(Document {
                    id: DocId::new_v4(),
                    timestamp: ts,
                    event_type: event_type.to_string(),
                    fields,
                });
            }
        }

        info!("Imported {} documents from {}", documents.len(), path.display());
        Ok(documents)
    }
}
