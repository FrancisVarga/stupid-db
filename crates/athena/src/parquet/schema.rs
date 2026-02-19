//! Athena SQL type to Arrow type mapping and schema construction.

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

use crate::result::AthenaColumn;

/// Map an Athena SQL type string to an Arrow DataType.
///
/// Athena types are documented at:
/// <https://docs.aws.amazon.com/athena/latest/ug/data-types.html>
pub(crate) fn athena_type_to_arrow(athena_type: &str) -> DataType {
    match athena_type.to_lowercase().as_str() {
        // Integer family
        "tinyint" | "smallint" | "int" | "integer" | "bigint" => DataType::Int64,

        // Floating-point family
        "float" | "real" | "double" | "decimal" => DataType::Float64,

        // Boolean
        "boolean" => DataType::Boolean,

        // Timestamps
        "timestamp" | "timestamp with time zone" => {
            DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into()))
        }

        // Date stored as UTF-8 (Athena returns dates as strings like "2025-01-15")
        "date" => DataType::Utf8,

        // String family (varchar, char, string, arrays, maps, structs, etc.)
        _ => DataType::Utf8,
    }
}

/// Build an Arrow [`Schema`] from Athena column definitions.
pub(crate) fn build_schema(columns: &[AthenaColumn]) -> Schema {
    let fields: Vec<Field> = columns
        .iter()
        .map(|col| Field::new(&col.name, athena_type_to_arrow(&col.data_type), true))
        .collect();
    Schema::new(fields)
}
