//! Convert [`AthenaQueryResult`] to Apache Parquet files.
//!
//! Maps Athena SQL types to Arrow data types and writes typed, columnar
//! Parquet files with Zstd compression. This avoids the naive approach of
//! storing everything as strings and enables downstream tools (DuckDB,
//! Polars, Spark) to read the data with proper types and predicate pushdown.

mod error;
pub(crate) mod schema;
pub(crate) mod builders;
mod writer;

#[cfg(test)]
mod tests;

pub use error::ParquetError;
pub use writer::{result_to_record_batch, write_parquet, write_parquet_bytes};
