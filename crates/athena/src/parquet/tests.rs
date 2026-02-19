//! Tests for Parquet conversion.

#[cfg(test)]
mod tests {
    use arrow::datatypes::{DataType, TimeUnit};
    use arrow::record_batch::RecordBatch;

    use crate::parquet::builders::parse_timestamp_ms;
    use crate::parquet::schema::{athena_type_to_arrow, build_schema};
    use crate::parquet::writer::{result_to_record_batch, write_parquet, write_parquet_bytes};
    use crate::result::{AthenaColumn, AthenaQueryResult, QueryMetadata};

    fn test_metadata() -> QueryMetadata {
        QueryMetadata {
            query_id: "test-parquet-001".to_string(),
            bytes_scanned: 2048,
            execution_time_ms: 150,
            state: "SUCCEEDED".to_string(),
            output_location: Some("s3://bucket/results/test.csv".into()),
        }
    }

    fn sample_result() -> AthenaQueryResult {
        AthenaQueryResult {
            columns: vec![
                AthenaColumn { name: "id".into(), data_type: "bigint".into() },
                AthenaColumn { name: "name".into(), data_type: "varchar".into() },
                AthenaColumn { name: "score".into(), data_type: "double".into() },
                AthenaColumn { name: "active".into(), data_type: "boolean".into() },
                AthenaColumn { name: "created_at".into(), data_type: "timestamp".into() },
            ],
            rows: vec![
                vec![
                    Some("1".into()),
                    Some("alice".into()),
                    Some("9.5".into()),
                    Some("true".into()),
                    Some("2025-06-14T10:30:00Z".into()),
                ],
                vec![
                    Some("2".into()),
                    Some("bob".into()),
                    None,
                    Some("false".into()),
                    Some("2025-06-14 11:00:00".into()),
                ],
                vec![
                    Some("3".into()),
                    None,
                    Some("7.0".into()),
                    None,
                    None,
                ],
            ],
            metadata: test_metadata(),
        }
    }

    #[test]
    fn test_athena_type_mapping() {
        assert_eq!(athena_type_to_arrow("bigint"), DataType::Int64);
        assert_eq!(athena_type_to_arrow("int"), DataType::Int64);
        assert_eq!(athena_type_to_arrow("INTEGER"), DataType::Int64);
        assert_eq!(athena_type_to_arrow("tinyint"), DataType::Int64);
        assert_eq!(athena_type_to_arrow("double"), DataType::Float64);
        assert_eq!(athena_type_to_arrow("FLOAT"), DataType::Float64);
        assert_eq!(athena_type_to_arrow("decimal"), DataType::Float64);
        assert_eq!(athena_type_to_arrow("boolean"), DataType::Boolean);
        assert_eq!(
            athena_type_to_arrow("timestamp"),
            DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into()))
        );
        assert_eq!(athena_type_to_arrow("varchar"), DataType::Utf8);
        assert_eq!(athena_type_to_arrow("string"), DataType::Utf8);
        assert_eq!(athena_type_to_arrow("date"), DataType::Utf8);
        assert_eq!(athena_type_to_arrow("array<string>"), DataType::Utf8);
    }

    #[test]
    fn test_build_schema() {
        let columns = vec![
            AthenaColumn { name: "id".into(), data_type: "bigint".into() },
            AthenaColumn { name: "name".into(), data_type: "varchar".into() },
        ];
        let schema = build_schema(&columns);
        assert_eq!(schema.fields().len(), 2);
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(*schema.field(0).data_type(), DataType::Int64);
        assert_eq!(schema.field(1).name(), "name");
        assert_eq!(*schema.field(1).data_type(), DataType::Utf8);
    }

    #[test]
    fn test_result_to_record_batch() {
        let result = sample_result();
        let batch = result_to_record_batch(&result).unwrap();
        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.num_columns(), 5);
        assert_eq!(batch.schema().field(0).name(), "id");
        assert_eq!(*batch.schema().field(0).data_type(), DataType::Int64);
    }

    #[test]
    fn test_null_handling_in_batch() {
        let result = sample_result();
        let batch = result_to_record_batch(&result).unwrap();

        // Column "score" (index 2): row 1 is NULL
        let score_col = batch.column(2);
        assert!(score_col.is_valid(0)); // 9.5
        assert!(!score_col.is_valid(1)); // NULL
        assert!(score_col.is_valid(2)); // 7.0

        // Column "name" (index 1): row 2 is NULL
        let name_col = batch.column(1);
        assert!(name_col.is_valid(0)); // alice
        assert!(name_col.is_valid(1)); // bob
        assert!(!name_col.is_valid(2)); // NULL
    }

    #[test]
    fn test_write_parquet_to_file() {
        let result = sample_result();
        let dir = std::env::temp_dir().join("stupid-db-test-parquet");
        let path = dir.join("test_output.parquet");

        let row_count = write_parquet(&result, &path).unwrap();
        assert_eq!(row_count, 3);
        assert!(path.exists());

        // Read back and verify.
        let file = std::fs::File::open(&path).unwrap();
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReader::try_new(file, 1024).unwrap();
        let batches: Vec<RecordBatch> = reader.into_iter().map(|r| r.unwrap()).collect();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].num_rows(), 3);
        assert_eq!(batches[0].num_columns(), 5);

        // Cleanup.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_parquet_bytes() {
        let result = sample_result();
        let bytes = write_parquet_bytes(&result).unwrap();

        // Parquet files start with magic bytes "PAR1".
        assert!(bytes.len() > 4);
        assert_eq!(&bytes[..4], b"PAR1");
    }

    #[test]
    fn test_parquet_metadata_in_file() {
        use parquet::file::reader::FileReader;

        let result = sample_result();
        let dir = std::env::temp_dir().join("stupid-db-test-parquet-meta");
        let path = dir.join("meta_test.parquet");

        write_parquet(&result, &path).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let reader = parquet::file::reader::SerializedFileReader::new(file).unwrap();
        let file_metadata = reader.metadata().file_metadata();
        let kv = file_metadata.key_value_metadata().expect("metadata present");

        let query_id_kv = kv.iter().find(|kv| kv.key == "athena.query_id").unwrap();
        assert_eq!(query_id_kv.value.as_deref(), Some("test-parquet-001"));

        let scanned_kv = kv.iter().find(|kv| kv.key == "athena.bytes_scanned").unwrap();
        assert_eq!(scanned_kv.value.as_deref(), Some("2048"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_empty_result_writes_valid_parquet() {
        let result = AthenaQueryResult {
            columns: vec![
                AthenaColumn { name: "col1".into(), data_type: "varchar".into() },
            ],
            rows: vec![],
            metadata: test_metadata(),
        };

        let bytes = write_parquet_bytes(&result).unwrap();
        assert!(bytes.len() > 4);
        assert_eq!(&bytes[..4], b"PAR1");
    }

    #[test]
    fn test_parse_timestamp_formats() {
        // RFC3339
        let ms = parse_timestamp_ms("2025-06-14T10:30:00Z").unwrap();
        assert!(ms > 0);

        // Space-separated
        let ms2 = parse_timestamp_ms("2025-06-14 10:30:00").unwrap();
        assert!(ms2 > 0);

        // With fractional seconds
        let ms3 = parse_timestamp_ms("2025-06-14 10:30:00.123").unwrap();
        assert!(ms3 > 0);

        // Date only
        let ms4 = parse_timestamp_ms("2025-06-14").unwrap();
        assert!(ms4 > 0);

        // Invalid
        assert!(parse_timestamp_ms("not-a-date").is_none());
    }

    #[test]
    fn test_invalid_numbers_become_null() {
        let result = AthenaQueryResult {
            columns: vec![
                AthenaColumn { name: "val".into(), data_type: "bigint".into() },
            ],
            rows: vec![
                vec![Some("not-a-number".into())],
                vec![Some("42".into())],
            ],
            metadata: test_metadata(),
        };

        let batch = result_to_record_batch(&result).unwrap();
        let col = batch.column(0);
        assert!(!col.is_valid(0)); // "not-a-number" -> NULL
        assert!(col.is_valid(1)); // 42 -> valid
    }
}
